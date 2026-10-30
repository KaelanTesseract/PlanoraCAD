//! Protokoll des Programms: die letzten Zeilen im Speicher, dazu eine Datei, die sich bei
//! etwa 1 MB dreht (`planora.log`, `planora.1.log`, `planora.2.log`; ältere fallen weg).
//!
//! Ins Protokoll kommen nur kurze Hinweise (Start, Speichern, Öffnen, Fehler), nie der Inhalt
//! eines Projekts. Lässt sich die Datei nicht anlegen oder beschreiben, bleibt das Protokoll im
//! Speicher und das Programm läuft weiter.
//!
//! Wichtig: Meldungen an `diag::*` dürfen keine rohen `io::Error` oder Pfade enthalten. Die
//! Bereinigung vor dem Versenden (Task 6) greift nur auf feste Muster.
//!
//! Alle Zeilen und Schritte sind englisch, unabhängig von der Sprache des Programms: feste Texte
//! als englische Literale, Texte aus den Sprachdateien über [`in_english`].

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
/// Längste Zeile (Zeichen, ohne Zeitangabe).
const MAX_LINE_CHARS: usize = 2000;
/// Nach einem Schreibfehler so viele Zeilen überspringen, bevor es erneut versucht wird.
const RETRY_AFTER: u32 = 50;

/// So viele letzte Schritte hält das Programm im Speicher.
pub const STEPS: usize = 50;

/// Ein Schritt der Nutzerin: Uhrzeit (Ortszeit, `14:03:22`) und Name der Aktion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub time: String,
    pub label: String,
}

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
            Level::Fehler => "ERROR",
            Level::Detail => "DETAIL",
        }
    }
}

/// Protokoll mit Speicherring und (wenn möglich) Datei.
pub struct Diag {
    ring: VecDeque<String>,
    /// Letzte Schritte: Zeit, Name, Anzahl direkt aufeinanderfolgender Wiederholungen.
    steps: VecDeque<(String, String, u32)>,
    dir: Option<PathBuf>,
    /// Zeilen, die nicht in die Datei kamen (Schreibfehler); gehen beim nächsten Versuch nach.
    pending: Vec<String>,
    /// So viele Zeilen noch überspringen, bevor erneut geschrieben wird.
    skip: u32,
    /// So viele Zeilen noch nicht drehen (nach einem gescheiterten Drehen; geschrieben wird
    /// weiter in die aktuelle Datei).
    rotate_skip: u32,
    verbose: bool,
    file_error: bool,
    /// Pfad des aktiven Dokuments, nur für den Absturzbericht (nie ins Protokoll).
    document: Option<PathBuf>,
}

impl Diag {
    /// Nur im Speicher (für Tests und solange nichts anderes eingerichtet ist).
    pub fn in_memory() -> Diag {
        Diag {
            ring: VecDeque::new(),
            steps: VecDeque::new(),
            dir: None,
            pending: Vec::new(),
            skip: 0,
            rotate_skip: 0,
            verbose: false,
            file_error: false,
            document: None,
        }
    }

    /// Mit Datei in `dir/logs`. Klappt das Anlegen nicht, bleibt es im Speicher und
    /// [`Diag::file_error`] meldet das.
    pub fn with_dir(dir: &Path) -> Diag {
        let mut d = Diag::in_memory();
        let logs = dir.join(LOG_DIR);
        match fs::create_dir_all(&logs) {
            Ok(()) => {
                d.dir = Some(logs);
            }
            Err(_) => d.file_error = true,
        }
        d
    }

    /// Schreibt eine Zeile `JJJJ-MM-TTThh:mm:ssZ STUFE Text` (UTC). Steuerzeichen werden zu
    /// Leerzeichen, sehr lange Texte gekürzt.
    pub fn log(&mut self, level: Level, msg: &str) {
        if level == Level::Detail && !self.verbose {
            return;
        }
        let line = format!(
            "{} {} {}",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            level.label(),
            clean(msg)
        );
        if self.ring.len() >= RING_LINES {
            self.ring.pop_front();
        }
        self.ring.push_back(line.clone());
        self.write_file(line);
    }

    fn write_file(&mut self, line: String) {
        let Some(dir) = self.dir.clone() else { return };
        self.pending.push(line);
        if self.pending.len() > RING_LINES {
            self.pending.remove(0);
        }
        if self.skip > 0 {
            self.skip -= 1;
            return;
        }
        let mut data = self.pending.join("\n");
        data.push('\n');
        let path = dir.join(LOG_NAME);
        // Die Größe immer aus der Datei lesen: eine zweite Programminstanz kann sie verändert haben.
        let size = fs::metadata(&path).map_or(0, |m| m.len());
        let mut rotate_failed = false;
        if size > 0 && size + data.len() as u64 > MAX_LOG_BYTES {
            if self.rotate_skip > 0 {
                self.rotate_skip -= 1;
                rotate_failed = true;
            } else if !rotate(&dir) {
                // Nicht in jeder Zeile neu versuchen; bis dahin wächst die aktuelle Datei etwas
                // über die Grenze.
                rotate_failed = true;
                self.rotate_skip = RETRY_AFTER;
            }
        }
        let result = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| f.write_all(data.as_bytes()));
        match result {
            Ok(()) => {
                self.pending.clear();
                self.file_error = rotate_failed;
            }
            Err(_) => {
                // Später erneut versuchen; die Zeilen bleiben vorgemerkt.
                self.file_error = true;
                self.skip = RETRY_AFTER;
            }
        }
    }

    /// Die letzten `n` Zeilen, älteste zuerst.
    pub fn recent_lines(&self, n: usize) -> Vec<String> {
        let skip = self.ring.len().saturating_sub(n);
        self.ring.iter().skip(skip).cloned().collect()
    }

    /// Merkt einen Schritt (nur der Name einer Aktion, nie Projektinhalt). Gleiche Schritte
    /// direkt hintereinander werden zu einem mit Zähler. Der Name wird wie eine Protokollzeile
    /// entschärft.
    pub fn step(&mut self, label: &str) {
        let label = clean(label);
        let time = chrono::Local::now().format("%H:%M:%S").to_string();
        if let Some(last) = self.steps.back_mut()
            && last.1 == label
        {
            last.0 = time;
            last.2 += 1;
            return;
        }
        if self.steps.len() >= STEPS {
            self.steps.pop_front();
        }
        self.steps.push_back((time, label, 1));
    }

    /// Merkt den Pfad des aktiven Dokuments für den Absturzbericht. Er kommt nie ins
    /// Protokoll; bereinigt wird erst bei der Anzeige des Berichts.
    pub fn set_document(&mut self, path: Option<PathBuf>) {
        self.document = path;
    }

    /// Pfad des aktiven Dokuments (siehe [`Diag::set_document`]).
    pub fn current_document(&self) -> Option<PathBuf> {
        self.document.clone()
    }

    /// Die letzten Schritte, älteste zuerst.
    pub fn recent_steps(&self) -> Vec<Step> {
        self.steps
            .iter()
            .map(|(time, label, n)| Step {
                time: time.clone(),
                label: if *n > 1 {
                    format!("{label} ×{n}")
                } else {
                    label.clone()
                },
            })
            .collect()
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

/// `planora.log` → `.1.log` → `.2.log`; die älteste Datei fällt weg. Wahr, wenn die aktuelle
/// Datei erfolgreich weggeschoben wurde.
///
/// Von der ältesten Datei her, und beim ersten Fehler ist Schluss: so bleibt nie eine Datei
/// auf der Strecke, weil ein späterer Schritt scheiterte. Nachgerückt wird nur bis zum ersten
/// freien Platz (ein früherer, gescheiterter Versuch hat `.1.log` schon frei gemacht); dann wird
/// nur noch die aktuelle Datei verschoben, und wiederholte Versuche löschen keine älteren
/// Dateien nach und nach.
///
/// Ist die aktuelle Datei gesperrt (etwa von einem Virenscanner), kann beim ersten Versuch die
/// älteste Datei (`.2.log`) schon weggefallen und `.1.log` nachgerückt sein, bevor das
/// Verschieben der aktuellen scheitert – wie bei einem gelungenen Drehen, nur ohne neue Datei.
fn rotate(dir: &Path) -> bool {
    let name = |i: usize| {
        if i == 0 {
            dir.join(LOG_NAME)
        } else {
            dir.join(format!("planora.{i}.log"))
        }
    };
    // Der erste freie Platz ab `.1.log`; fehlt einer, fällt die älteste Datei weg.
    let exists = |i: usize| fs::symlink_metadata(name(i)).is_ok();
    let free = match (1..LOG_FILES).find(|&i| !exists(i)) {
        Some(k) => k,
        None => {
            if fs::remove_file(name(LOG_FILES - 1)).is_err() {
                return false;
            }
            LOG_FILES - 1
        }
    };
    // Nur die Dateien vor dem freien Platz rücken nach.
    for i in (1..free).rev() {
        if fs::rename(name(i), name(i + 1)).is_err() {
            return false;
        }
    }
    fs::rename(name(0), name(1)).is_ok()
}

/// Steuerzeichen (auch Zeilentrenner) zu Leerzeichen; auf [`MAX_LINE_CHARS`] Zeichen gekürzt.
fn clean(msg: &str) -> String {
    let mut out: String = msg
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}' | '\u{85}') {
                ' '
            } else {
                c
            }
        })
        .take(MAX_LINE_CHARS + 1)
        .collect();
    if out.chars().count() > MAX_LINE_CHARS {
        out = out.chars().take(MAX_LINE_CHARS - 1).collect();
        out.push('…');
    }
    out
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
    let mut g = lock();
    let logs = data_dir.join(LOG_DIR);
    if g.dir.as_deref() == Some(logs.as_path()) {
        // schon eingerichtet: nichts doppeln
        return;
    }
    let mut fresh = Diag::with_dir(data_dir);
    fresh.verbose = g.verbose;
    fresh.steps = std::mem::take(&mut g.steps);
    fresh.document = g.document.take();
    // Was bisher nur im Speicher stand, kommt in die Datei und bleibt im Ring.
    let old: Vec<String> = std::mem::take(&mut g.ring).into();
    for line in old {
        if fresh.ring.len() >= RING_LINES {
            fresh.ring.pop_front();
        }
        fresh.ring.push_back(line.clone());
        fresh.write_file(line);
    }
    *g = fresh;
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

/// Merkt einen Schritt im gemeinsamen Protokoll (siehe [`Diag::step`]).
pub fn step(label: &str) {
    lock().step(label);
}

/// Merkt den Pfad des aktiven Dokuments im gemeinsamen Protokoll (siehe
/// [`Diag::set_document`]).
pub fn set_document(path: Option<PathBuf>) {
    lock().set_document(path);
}

/// Pfad des aktiven Dokuments im gemeinsamen Protokoll.
pub fn current_document() -> Option<PathBuf> {
    lock().current_document()
}

/// Die letzten Schritte des gemeinsamen Protokolls, älteste zuerst.
pub fn recent_steps() -> Vec<Step> {
    lock().recent_steps()
}

/// Führt `f` in Englisch aus: Texte aus den Sprachdateien (Befehlsnamen, Fehlermeldungen), die
/// ins Protokoll, in die letzten Schritte oder in einen Bericht gehen, entstehen so immer
/// englisch, unabhängig von der Sprache des Programms.
pub fn in_english<R>(f: impl FnOnce() -> R) -> R {
    planora_i18n::with_lang(&planora_i18n::english(), f)
}

/// Eine Zeile je Schritt: `14:03:22 Extrude sketch`.
pub fn steps_text(steps: &[Step]) -> String {
    steps
        .iter()
        .map(|s| format!("{} {}", s.time, s.label))
        .collect::<Vec<_>>()
        .join("\n")
}
