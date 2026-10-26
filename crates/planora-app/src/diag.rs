//! Protokoll des Programms: die letzten Zeilen im Speicher, dazu eine Datei, die sich bei
//! etwa 1 MB dreht (`planora.log`, `planora.1.log`, `planora.2.log`; ältere fallen weg).
//!
//! Ins Protokoll kommen nur kurze Hinweise (Start, Speichern, Öffnen, Fehler), nie der Inhalt
//! eines Projekts. Lässt sich die Datei nicht anlegen oder beschreiben, bleibt das Protokoll im
//! Speicher und das Programm läuft weiter.

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// So viele Zeilen hält das Protokoll im Speicher.
pub const RING_LINES: usize = 500;
/// Ab dieser Größe wird die Datei weitergedreht.
pub const MAX_LOG_BYTES: u64 = 1_048_576;
/// Anzahl der Dateien (aktuelle plus ältere).
pub const LOG_FILES: usize = 3;
/// Unterordner im Datenordner.
pub const LOG_DIR: &str = "logs";
/// Name der aktuellen Protokolldatei.
pub const LOG_NAME: &str = "planora.log";

/// Wichtigkeit einer Zeile. `Detail` erscheint nur im ausführlichen Protokoll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Fehler,
    Detail,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Fehler => "FEHLER",
            Level::Detail => "DETAIL",
        }
    }
}

/// Protokoll mit Speicherring und (wenn möglich) Datei.
pub struct Diag {
    ring: VecDeque<String>,
    dir: Option<PathBuf>,
    size: u64,
    verbose: bool,
    file_error: bool,
}

impl Diag {
    /// Nur im Speicher (für Tests und solange nichts anderes eingerichtet ist).
    pub fn in_memory() -> Diag {
        Diag {
            ring: VecDeque::new(),
            dir: None,
            size: 0,
            verbose: false,
            file_error: false,
        }
    }

    /// Mit Datei in `dir/logs`. Klappt das Anlegen nicht, bleibt es im Speicher und
    /// [`Diag::file_error`] meldet das.
    pub fn with_dir(dir: &Path) -> Diag {
        let mut d = Diag::in_memory();
        let logs = dir.join(LOG_DIR);
        match fs::create_dir_all(&logs) {
            Ok(()) => {
                d.size = fs::metadata(logs.join(LOG_NAME)).map_or(0, |m| m.len());
                d.dir = Some(logs);
            }
            Err(_) => d.file_error = true,
        }
        d
    }

    /// Schreibt eine Zeile `JJJJ-MM-TT hh:mm:ss STUFE Text` (Ortszeit).
    pub fn log(&mut self, level: Level, msg: &str) {
        if level == Level::Detail && !self.verbose {
            return;
        }
        // Zeilenumbrüche im Text würden eine Zeile in mehrere teilen.
        let msg = msg.replace(['\r', '\n'], " ");
        let line = format!(
            "{} {} {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            level.label(),
            msg
        );
        if self.ring.len() >= RING_LINES {
            self.ring.pop_front();
        }
        self.ring.push_back(line.clone());
        self.write_file(&line);
    }

    fn write_file(&mut self, line: &str) {
        let Some(dir) = self.dir.clone() else { return };
        let bytes = line.len() as u64 + 1;
        if self.size > 0 && self.size + bytes > MAX_LOG_BYTES {
            rotate(&dir);
            self.size = 0;
        }
        let result = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(LOG_NAME))
            .and_then(|mut f| writeln!(f, "{line}"));
        match result {
            Ok(()) => self.size += bytes,
            Err(_) => {
                // Ab hier nur noch im Speicher.
                self.file_error = true;
                self.dir = None;
            }
        }
    }

    /// Die letzten `n` Zeilen, älteste zuerst.
    pub fn recent_lines(&self, n: usize) -> Vec<String> {
        let skip = self.ring.len().saturating_sub(n);
        self.ring.iter().skip(skip).cloned().collect()
    }

    /// Schaltet das ausführliche Protokoll (`Level::Detail`) ein oder aus.
    pub fn set_verbose(&mut self, on: bool) {
        self.verbose = on;
    }

    /// Wahr, wenn die Datei nicht angelegt oder beschrieben werden konnte.
    pub fn file_error(&self) -> bool {
        self.file_error
    }
}

/// `planora.log` → `.1.log` → `.2.log`; die älteste Datei fällt weg.
fn rotate(dir: &Path) {
    let name = |i: usize| {
        if i == 0 {
            dir.join(LOG_NAME)
        } else {
            dir.join(format!("planora.{i}.log"))
        }
    };
    let _ = fs::remove_file(name(LOG_FILES - 1));
    for i in (0..LOG_FILES - 1).rev() {
        let _ = fs::rename(name(i), name(i + 1));
    }
}

static GLOBAL: OnceLock<Mutex<Diag>> = OnceLock::new();

/// Das gemeinsame Protokoll. Ohne [`init_global`] nur im Speicher, damit Tests nie in den
/// Datenordner schreiben.
#[doc(hidden)]
pub fn global() -> &'static Mutex<Diag> {
    GLOBAL.get_or_init(|| Mutex::new(Diag::in_memory()))
}

fn lock() -> MutexGuard<'static, Diag> {
    // Eine vergiftete Sperre (ein Faden ist beim Schreiben abgestürzt) wird weiter benutzt.
    global().lock().unwrap_or_else(|e| e.into_inner())
}

/// Richtet das gemeinsame Protokoll mit Datei in `data_dir/logs` ein (einmal, beim Start).
pub fn init_global(data_dir: &Path) {
    let fresh = Diag::with_dir(data_dir);
    if GLOBAL.set(Mutex::new(fresh)).is_err() {
        // schon benutzt: Einstellungen übernehmen, Zeilen behalten
        let mut g = lock();
        let old = std::mem::replace(&mut *g, Diag::with_dir(data_dir));
        let verbose = old.verbose;
        g.verbose = verbose;
        for l in old.ring {
            if g.ring.len() >= RING_LINES {
                g.ring.pop_front();
            }
            g.ring.push_back(l);
        }
    }
}

pub fn info(msg: &str) {
    lock().log(Level::Info, msg);
}

pub fn warn(msg: &str) {
    lock().log(Level::Warn, msg);
}

pub fn fehler(msg: &str) {
    lock().log(Level::Fehler, msg);
}

pub fn detail(msg: &str) {
    lock().log(Level::Detail, msg);
}

/// Die letzten `n` Zeilen des gemeinsamen Protokolls, älteste zuerst.
pub fn recent_lines(n: usize) -> Vec<String> {
    lock().recent_lines(n)
}

pub fn set_verbose(on: bool) {
    lock().set_verbose(on);
}
