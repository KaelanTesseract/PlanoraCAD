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
use std::time::SystemTime;

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
        if !entered {
            // Panik im Hook selbst: nichts schreiben.
        } else if panic_expected() {
            // Abgefangen: kein Absturz, nur eine Zeile ins Protokoll (ohne zu warten).
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let d = match diag.try_lock() {
                    Ok(d) => Some(d),
                    Err(TryLockError::Poisoned(e)) => Some(e.into_inner()),
                    Err(TryLockError::WouldBlock) => None,
                };
                if let Some(mut d) = d {
                    d.log(diag::Level::Warn, CAUGHT_LINE);
                }
            }));
        } else {
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let input = collect(info, diag);
                let _ = write_report(&dir, &input);
            }));
        }
        if entered {
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

// ---- Laufmarker, Befund beim Start, Aufräumen -----------------------------------------------

/// Berichte, die älter sind, löscht [`cleanup_reports`].
pub const KEEP_DAYS: u64 = 30;
/// Höchstens so viele Berichte bleiben nach [`cleanup_reports`].
pub const KEEP_MAX: usize = 10;

const MARKER_PREFIX: &str = "laeuft-";
const MARKER_SUFFIX: &str = ".marker";
const REPORT_PREFIX: &str = "absturz-";
const REPORT_SUFFIX: &str = ".txt";

/// Laufmarker der Instanz mit der Prozessnummer `pid`: `laeuft-<pid>.marker` im Datenordner.
pub fn marker_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("{MARKER_PREFIX}{pid}{MARKER_SUFFIX}"))
}

/// Der Laufmarker dieser Instanz. Er bleibt offen, solange das Programm läuft; unter Windows
/// ohne Mitbenutzung, so dass ein zweiter Start ihn als „gehört einer laufenden Instanz“
/// erkennt (etwa die alte Version beim Update-Neustart). Nur [`RunMarker::finish`] löscht ihn;
/// endet das Programm anders (Absturz, Abbruch), bleibt er liegen.
#[derive(Debug)]
pub struct RunMarker {
    path: PathBuf,
    file: Option<std::fs::File>,
}

impl RunMarker {
    /// Sauberes Ende: Marker schließen und löschen.
    pub fn finish(mut self) {
        self.file = None;
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Was der Start über den letzten Lauf herausfindet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartFinding {
    /// Nichts Besonderes.
    Clean,
    /// Es gibt Absturzberichte (neueste zuerst).
    Crashed(Vec<PathBuf>),
    /// Ein Lauf endete ohne sauberes Ende und ohne Bericht.
    Unexpected,
}

/// Öffnet einen Marker so, dass ihn kein anderer öffnen kann, solange er offen ist.
fn open_exclusive(path: &Path, create: bool) -> std::io::Result<std::fs::File> {
    let mut o = OpenOptions::new();
    o.read(true).write(true);
    if create {
        o.create(true).truncate(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.share_mode(0);
    }
    o.open(path)
}

/// Läuft die Instanz mit dieser Prozessnummer noch? Unter Windows entscheidet die Sperre des
/// Markers; außerhalb von Windows wird nur `/proc/<pid>` geprüft (Linux). Unter macOS gibt es
/// kein `/proc`: Dort gilt jeder fremde Marker als liegengeblieben, auch der einer gleichzeitig
/// laufenden Instanz. Das nehmen wir hin, weil es dort keine Updates (also keinen Neustart mit
/// zwei Instanzen) gibt.
#[cfg(not(windows))]
fn other_running(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

/// Prozessnummer aus einem Markernamen.
fn marker_pid(name: &str) -> Option<u32> {
    name.strip_prefix(MARKER_PREFIX)?
        .strip_suffix(MARKER_SUFFIX)?
        .parse()
        .ok()
}

/// Räumt fremde, liegengebliebene Marker weg und gibt die Änderungszeit des jüngsten zurück
/// (`Some(None)`: es gab einen, Zeit unbekannt). Gesperrte gehören einer laufenden Instanz und
/// bleiben.
fn take_stale_markers(data_dir: &Path) -> Option<Option<SystemTime>> {
    let mut found: Option<Option<SystemTime>> = None;
    let Ok(entries) = std::fs::read_dir(data_dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(marker_pid) else {
            continue;
        };
        let path = entry.path();
        #[cfg(windows)]
        let stale = {
            let _ = pid;
            open_exclusive(&path, false).is_ok()
        };
        #[cfg(not(windows))]
        let stale = !other_running(pid);
        if !stale {
            continue;
        }
        let time = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let _ = std::fs::remove_file(&path);
        found = Some(match (found.flatten(), time) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        });
    }
    found
}

/// Die Berichte in `dir` mit Änderungszeit, neueste zuerst (Fehler übergehen).
fn list_reports(dir: &Path) -> Vec<(PathBuf, SystemTime)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<(PathBuf, SystemTime)> = entries
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(REPORT_PREFIX) && n.ends_with(REPORT_SUFFIX))
        })
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            m.is_file().then_some(())?;
            Some((e.path(), m.modified().ok()?))
        })
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    v
}

/// Beim Start (vor dem Fenster): räumt die Berichte auf ([`cleanup_reports`]), wertet
/// liegengebliebene Marker fremder Instanzen aus und legt den eigenen an. Ein gesperrter Marker
/// gehört einer laufenden Instanz und zählt nicht. Ein liegengebliebener mit einem neueren
/// Bericht ergibt [`StartFinding::Crashed`], ohne einen [`StartFinding::Unexpected`]; Berichte
/// ohne Marker (Panik mit sauberem Ende) ergeben [`StartFinding::Crashed`]. Lässt sich der
/// eigene Marker nicht anlegen, läuft das Programm ohne.
pub fn start_run(data_dir: &Path, pid: u32) -> (Option<RunMarker>, StartFinding) {
    let crash_dir = data_dir.join(CRASH_DIR);
    cleanup_reports(&crash_dir, SystemTime::now());
    let stale = take_stale_markers(data_dir);
    let reports = list_reports(&crash_dir);
    let finding = match stale {
        Some(since) => {
            let newer = reports.iter().any(|(_, t)| since.is_none_or(|s| *t >= s));
            if newer {
                StartFinding::Crashed(reports.into_iter().map(|(p, _)| p).collect())
            } else {
                StartFinding::Unexpected
            }
        }
        None if !reports.is_empty() => {
            StartFinding::Crashed(reports.into_iter().map(|(p, _)| p).collect())
        }
        None => StartFinding::Clean,
    };
    let path = marker_path(data_dir, pid);
    let marker = std::fs::create_dir_all(data_dir)
        .and_then(|_| open_exclusive(&path, true))
        .map(|file| RunMarker {
            path,
            file: Some(file),
        });
    match marker {
        Ok(m) => (Some(m), finding),
        Err(e) => {
            diag::warn(&format!("Laufmarker nicht angelegt ({:?})", e.kind()));
            (None, finding)
        }
    }
}

/// Löscht Berichte in `dir`, die älter als [`KEEP_DAYS`] Tage sind, und danach alle bis auf die
/// [`KEEP_MAX`] neuesten. Fehler (fehlender Ordner, gesperrte Datei, fremde Einträge) werden
/// übergangen.
pub fn cleanup_reports(dir: &Path, now: SystemTime) {
    let max_age = std::time::Duration::from_secs(KEEP_DAYS * 24 * 3600);
    let mut kept = 0usize;
    for (path, time) in list_reports(dir) {
        let too_old = now.duration_since(time).is_ok_and(|age| age > max_age);
        if too_old || kept >= KEEP_MAX {
            let _ = std::fs::remove_file(&path);
        } else {
            kept += 1;
        }
    }
}

/// Wert der Zeile `<key>: …` aus einem Bericht (erste Fundstelle).
pub fn report_field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(": "))
        .map(str::trim)
}

/// Das im Bericht genannte Dokument („Dokument: …“), falls eins genannt ist.
pub fn report_document(text: &str) -> Option<PathBuf> {
    report_field(text, "Dokument")
        .filter(|d| !d.is_empty() && *d != "keines")
        .map(PathBuf::from)
}

/// Protokollzeile statt eines Berichts bei einer abgefangenen Panik ([`expected_panics`]).
pub const CAUGHT_LINE: &str = "Fehler beim Erstellen einer Vorschau abgefangen";

thread_local! {
    /// Wie tief dieser Faden gerade in [`expected_panics`] steckt.
    static EXPECTED: Cell<u32> = const { Cell::new(0) };
}

/// Setzt den Merker beim Verlassen zurück, auch wenn `f` panikt.
struct ExpectedGuard;

impl Drop for ExpectedGuard {
    fn drop(&mut self) {
        let _ = EXPECTED.try_with(|c| c.set(c.get().saturating_sub(1)));
    }
}

/// Führt `f` aus, dessen Panik der Aufrufer selbst mit `catch_unwind` abfängt (etwa beim
/// Erstellen einer Vorschau). Der Hook schreibt dafür keinen Bericht, nur [`CAUGHT_LINE`] ins
/// Protokoll; der vorherige Hook läuft trotzdem. Rust ruft den Hook vor dem Abwickeln, ohne
/// diesen Merker gälte also jede abgefangene Panik als Absturz.
pub fn expected_panics<R>(f: impl FnOnce() -> R) -> R {
    let _ = EXPECTED.try_with(|c| c.set(c.get().saturating_add(1)));
    let _guard = ExpectedGuard;
    f()
}

/// Steckt dieser Faden gerade in [`expected_panics`]?
fn panic_expected() -> bool {
    EXPECTED.try_with(|c| c.get() > 0).unwrap_or(false)
}
