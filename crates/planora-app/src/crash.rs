//! Absturzbericht: Ein Panic-Hook schreibt bei einer Panik einen Textbericht nach
//! `<Datenordner>/abstuerze/absturz-JJJJ-MM-TT_hh-mm-ss.txt` (Ortszeit) und ruft danach immer
//! den vorherigen Hook.
//!
//! Der Bericht bleibt nur auf dem Rechner und wird unbereinigt gespeichert; bereinigt wird
//! erst bei der Anzeige mit [`crate::feedback::sanitize`].
//!
//! Der Hook tut nichts anderes: kein Netz, kein Fenster, kein Warten. Das Protokoll liest er
//! nur per `try_lock`; ist es gesperrt, steht „nicht verfügbar“ im Bericht. Eine Panik im Hook
//! selbst beendet das Programm sofort (so verhält sich Rust), deshalb meidet der Code hier
//! alles, was paniken kann; `catch_unwind` und ein Wiedereintritts-Merker sichern zusätzlich ab.

use std::backtrace::Backtrace;
use std::cell::Cell;
use std::fs::OpenOptions;
use std::io::Write;
use std::panic::{AssertUnwindSafe, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, TryLockError};

use crate::diag::{self, Diag, Step};

/// Unterordner des Datenordners für die Berichte.
pub const CRASH_DIR: &str = "abstuerze";
/// So viele letzte Protokollzeilen stehen im Bericht.
pub const LOG_LINES_IN_REPORT: usize = 100;
/// Steht im Bericht, wenn das Protokoll gerade gesperrt war.
pub const UNAVAILABLE: &str = "nicht verfügbar";
/// Nachricht, wenn die Panik keinen Text trägt.
pub const UNKNOWN: &str = "unbekannt";

/// Höchstens so viele Namen mit Zusatz `_2`, `_3`, … werden bei Kollision probiert.
const MAX_SUFFIX: u32 = 1000;

/// Alles, was in einen Bericht kommt.
#[derive(Clone, Debug)]
pub struct ReportInput {
    pub version: String,
    pub message: String,
    pub location: String,
    pub thread: String,
    pub backtrace: String,
    pub steps: Vec<Step>,
    pub log: Vec<String>,
    pub document: Option<PathBuf>,
    pub time: chrono::NaiveDateTime,
}

/// Der Berichtstext (unbereinigt).
pub fn report_text(input: &ReportInput) -> String {
    let mut t = String::new();
    t.push_str("PlanoraCAD – Absturzbericht\n");
    t.push_str(&format!(
        "Zeit: {}\n\n",
        input.time.format("%d.%m.%Y, %H:%M:%S")
    ));
    t.push_str(&format!("Version: {}\n", input.version));
    t.push_str(&format!("Fehler: {}\n", input.message));
    t.push_str(&format!("Ort: {}\n", input.location));
    t.push_str(&format!("Faden: {}\n", input.thread));
    let doc = input
        .document
        .as_ref()
        .map_or_else(|| "keines".to_string(), |p| p.display().to_string());
    t.push_str(&format!("Dokument: {doc}\n"));
    t.push_str("\nLetzte Schritte:\n");
    if input.steps.is_empty() {
        t.push_str("keine\n");
    }
    for s in &input.steps {
        t.push_str(format!("{} {}", s.time, s.label).trim());
        t.push('\n');
    }
    t.push_str("\nProtokoll:\n");
    if input.log.is_empty() {
        t.push_str("leer\n");
    }
    for l in &input.log {
        t.push_str(l);
        t.push('\n');
    }
    t.push_str("\nAblauf (Backtrace):\n");
    t.push_str(&input.backtrace);
    if !input.backtrace.ends_with('\n') {
        t.push('\n');
    }
    t
}

/// Dateiname zum Zeitpunkt (Ortszeit), ohne `:`: `absturz-2026-10-26_14-03-22.txt`.
pub fn report_file_name(t: chrono::NaiveDateTime) -> String {
    format!("absturz-{}.txt", t.format("%Y-%m-%d_%H-%M-%S"))
}

/// Schreibt den Bericht in `dir` (wird angelegt). Gibt es den Namen schon, kommt `_2`, `_3`, …
/// dazu; eine vorhandene Datei wird nie überschrieben.
pub fn write_report(dir: &Path, input: &ReportInput) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = report_file_name(input.time);
    let stem = name.trim_end_matches(".txt");
    let text = report_text(input);
    for n in 1..=MAX_SUFFIX {
        let path = if n == 1 {
            dir.join(&name)
        } else {
            dir.join(format!("{stem}_{n}.txt"))
        };
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                f.write_all(text.as_bytes())?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "zu viele Berichte mit gleicher Zeit",
    ))
}

/// Setzt den Hook für das gemeinsame Protokoll; der bisherige Hook wird danach gerufen.
pub fn install_hook(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(hook_with(dir, diag::global(), previous));
}

thread_local! {
    /// Gesetzt, solange dieser Faden einen Bericht schreibt.
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
}

type Hook = Box<dyn Fn(&PanicHookInfo) + Send + Sync>;

/// Baut den Hook: schreibt den Bericht nach `dir` (Protokoll aus `diag`) und ruft danach immer
/// `previous`.
pub fn hook_with(dir: PathBuf, diag: &'static Mutex<Diag>, previous: Hook) -> Hook {
    Box::new(move |info| {
        let entered = IN_HOOK.try_with(|f| !f.replace(true)).unwrap_or(false);
        if entered {
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let input = collect(info, diag);
                let _ = write_report(&dir, &input);
            }));
            let _ = IN_HOOK.try_with(|f| f.set(false));
        }
        previous(info);
    })
}

/// Sammelt die Angaben zur Panik, ohne je zu warten.
fn collect(info: &PanicHookInfo, diag: &Mutex<Diag>) -> ReportInput {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| UNKNOWN.to_string());
    let location = info
        .location()
        .map_or_else(|| UNKNOWN.to_string(), |l| l.to_string());
    let thread = std::thread::current()
        .name()
        .unwrap_or("<ohne Namen>")
        .to_string();
    let (steps, log, document) = match diag.try_lock() {
        Ok(d) => read_diag(&d),
        Err(TryLockError::Poisoned(e)) => read_diag(&e.into_inner()),
        Err(TryLockError::WouldBlock) => (
            vec![Step {
                time: String::new(),
                label: UNAVAILABLE.to_string(),
            }],
            vec![UNAVAILABLE.to_string()],
            None,
        ),
    };
    ReportInput {
        version: crate::RUNNING_VERSION.to_string(),
        message,
        location,
        thread,
        backtrace: Backtrace::force_capture().to_string(),
        steps,
        log,
        document,
        time: chrono::Local::now().naive_local(),
    }
}

fn read_diag(d: &Diag) -> (Vec<Step>, Vec<String>, Option<PathBuf>) {
    (
        d.recent_steps(),
        d.recent_lines(LOG_LINES_IN_REPORT),
        d.current_document(),
    )
}
