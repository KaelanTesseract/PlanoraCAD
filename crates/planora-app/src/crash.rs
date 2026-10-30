//! Absturzbericht: Ein Panic-Hook schreibt bei einer Panik einen Textbericht nach
//! `<Datenordner>/abstuerze/absturz-JJJJ-MM-TT_hh-mm-ss.txt` (Ortszeit) und ruft danach immer
//! den vorherigen Hook.
//!
//! Der Bericht bleibt nur auf dem Rechner. Schon beim Schreiben steht beim Dokument nur der
//! Dateiname, und der Benutzerordner ist überall durch [`USER_PLACEHOLDER`] ersetzt
//! ([`UserMask`]); den vollen Dokumentpfad (für die Projektdatei in der Rückmeldung) hält eine
//! Begleitdatei `absturz-….dokument` daneben, die nie angehängt wird. Bei der Anzeige
//! bereinigt [`crate::feedback::sanitize`] zusätzlich.
//!
//! Der Hook tut nichts anderes: kein Netz, kein Fenster, kein Warten. Das Protokoll liest er
//! nur per `try_lock`; ist es gesperrt, steht „not available“ im Bericht. Eine Panik im Hook
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
pub const UNAVAILABLE: &str = "not available";
/// Nachricht, wenn die Panik keinen Text trägt.
pub const UNKNOWN: &str = "unknown";

/// Höchstens so viele Namen mit Zusatz `_2`, `_3`, … werden bei Kollision probiert.
const MAX_SUFFIX: u32 = 1000;
/// Steht im Bericht statt des Benutzerordners.
pub const USER_PLACEHOLDER: &str = "<user>";
/// So viele Zeilen des Ablaufs (Backtrace) stehen höchstens im Bericht ([`trim_backtrace`]).
pub const BACKTRACE_LINES: usize = 40;
/// Endung der Begleitdatei mit dem vollen Dokumentpfad ([`companion_path`]).
const COMPANION_EXT: &str = "dokument";
/// Unterordner der Berichte, die schon als Rückmeldung geöffnet wurden ([`mark_sent`]).
pub const SENT_DIR: &str = "gesendet";
/// Schlüssel der Zeile mit der Zeit im Bericht (ältere Berichte: „Zeit“).
pub const TIME_KEY: &str = "Time";
/// Format der Zeit im Bericht (ältere Berichte: `%d.%m.%Y, %H:%M:%S`).
pub const REPORT_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
/// Schlüssel der Zeile mit der Nachricht der Panik (ältere Berichte: „Fehler“).
pub const ERROR_KEY: &str = "Error";
/// Schlüssel der Zeile mit dem Ort der Panik (ältere Berichte: „Ort“).
pub const LOCATION_KEY: &str = "Location";
/// Schlüssel der Zeile mit dem Dokument (ältere Berichte: „Dokument“).
pub const DOCUMENT_KEY: &str = "Document";
/// Steht beim Dokument, wenn keins offen war (ältere Berichte: „keines“).
pub const NO_DOCUMENT: &str = "none";

/// Ersetzt den Benutzerordner (`C:\Users\<Name>`, `\\?\C:\Users\<Name>`, `/home/<name>`, auch
/// mit `/` und in anderer Groß-/Kleinschreibung) durch [`USER_PLACEHOLDER`]. Die Muster
/// entstehen einmal beim Setzen des Hooks ([`UserMask::from_env`]); [`UserMask::apply`] ist
/// reine Zeichenkettenarbeit, die nicht paniken kann.
#[derive(Clone, Debug, Default)]
pub struct UserMask {
    /// Vollständige Ordner (aus `USERPROFILE`), längste zuerst.
    full: Vec<Vec<u8>>,
    /// Ordnerenden wie `\Users\<Name>` (ein Laufwerk davor zählt mit), längste zuerst.
    tails: Vec<Vec<u8>>,
}

impl UserMask {
    /// Ersetzt nichts.
    pub fn none() -> UserMask {
        UserMask::default()
    }

    /// Aus `USERNAME`, `USER` und `USERPROFILE` (einmal beim Setzen des Hooks).
    pub fn from_env() -> UserMask {
        let get = |k: &str| std::env::var(k).ok();
        UserMask::from_values(
            get("USERNAME").as_deref(),
            get("USER").as_deref(),
            get("USERPROFILE").as_deref(),
        )
    }

    /// Aus den Werten von `USERNAME`, `USER` und `USERPROFILE`; leere Werte zählen nicht.
    pub fn from_values(
        username: Option<&str>,
        user: Option<&str>,
        profile: Option<&str>,
    ) -> UserMask {
        let mut m = UserMask::default();
        if let Some(p) = profile.map(|p| p.trim().trim_end_matches(['\\', '/']))
            && p.len() > 3
        {
            m.full.push(p.as_bytes().to_vec());
            m.full.push(p.replace('\\', "/").into_bytes());
        }
        for name in [username, user].into_iter().flatten().map(str::trim) {
            if name.is_empty() || name.contains(['\\', '/']) {
                continue;
            }
            for t in [
                format!("\\Users\\{name}"),
                format!("/Users/{name}"),
                format!("/home/{name}"),
            ] {
                m.tails.push(t.into_bytes());
            }
        }
        for v in [&mut m.full, &mut m.tails] {
            v.sort_by_key(|b| std::cmp::Reverse(b.len()));
            v.dedup();
        }
        m
    }

    /// `text` mit [`USER_PLACEHOLDER`] statt des Benutzerordners.
    pub fn apply(&self, text: &str) -> String {
        if self.full.is_empty() && self.tails.is_empty() {
            return text.to_string();
        }
        let b = text.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(b.len());
        let mut i = 0usize;
        while i < b.len() {
            let hit = self
                .full
                .iter()
                .map(|p| (p, false))
                .chain(self.tails.iter().map(|p| (p, true)))
                .find(|(p, _)| matches_at(b, i, p));
            let Some((p, tail)) = hit else {
                if let Some(&c) = b.get(i) {
                    out.push(c);
                }
                i += 1;
                continue;
            };
            // Ein Laufwerk davor (`C:`, nur bei Ordnerenden) und `\\?\` gehören mit dazu.
            let mut start = out.len();
            if tail
                && start >= 2
                && out.get(start - 1) == Some(&b':')
                && out.get(start - 2).is_some_and(u8::is_ascii_alphabetic)
            {
                start -= 2;
            }
            for pre in [&b"\\\\?\\"[..], &b"//?/"[..]] {
                if start >= pre.len() && out.get(start - pre.len()..start) == Some(pre) {
                    start -= pre.len();
                }
            }
            out.truncate(start);
            out.extend_from_slice(USER_PLACEHOLDER.as_bytes());
            i += p.len();
        }
        String::from_utf8(out)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
    }
}

/// Steht `pat` (ASCII-Groß/klein egal) bei `i` in `hay`, und endet dort ein Ordnername?
fn matches_at(hay: &[u8], i: usize, pat: &[u8]) -> bool {
    let Some(end) = i.checked_add(pat.len()) else {
        return false;
    };
    let Some(part) = hay.get(i..end) else {
        return false;
    };
    if !part.eq_ignore_ascii_case(pat) {
        return false;
    }
    // Ein längerer Name, der nur so anfängt, zählt nicht.
    !hay.get(end)
        .is_some_and(|&c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.') || c >= 0x80)
}

/// Funktionsname einer Rahmenzeile `  12: name` des Ablaufs.
fn frame_function(line: &str) -> Option<&str> {
    let (num, rest) = line.trim_start().split_once(": ")?;
    (!num.is_empty() && num.bytes().all(|c| c.is_ascii_digit())).then_some(rest.trim())
}

/// Rahmen des Hooks und der Rust-Innereien, die vor dem eigentlichen Ort stehen.
fn machinery_frame(f: &str) -> bool {
    const PREFIXES: [&str; 11] = [
        "std::",
        "core::",
        "alloc::",
        "<std::",
        "<core::",
        "<alloc::",
        "rust_begin_unwind",
        "__rust",
        "planora_app::crash::",
        "<planora_app::crash::",
        "<unknown>",
    ];
    PREFIXES.iter().any(|p| f.starts_with(p))
}

/// Kürzt den Ablauf: ab dem ersten eigenen Rahmen (Hook und Rust-Innereien davor fallen weg;
/// gibt es keinen, vom Anfang), höchstens [`BACKTRACE_LINES`] Zeilen, der Rest als „…“.
pub fn trim_backtrace(bt: &str) -> String {
    let lines: Vec<&str> = bt.lines().collect();
    let start = lines
        .iter()
        .position(|l| frame_function(l).is_some_and(|f| !machinery_frame(f)))
        .unwrap_or(0);
    let rest = lines.get(start..).unwrap_or(&[]);
    let mut out = rest
        .iter()
        .take(BACKTRACE_LINES)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    if rest.len() > BACKTRACE_LINES {
        out.push_str("\n…");
    }
    out
}

/// Begleitdatei eines Berichts mit dem vollen Pfad des Dokuments:
/// `absturz-….txt` → `absturz-….dokument`. Sie bleibt auf dem Rechner und wird nie angehängt.
pub fn companion_path(report: &Path) -> PathBuf {
    report.with_extension(COMPANION_EXT)
}

/// Das Projekt (voller Pfad) zum Bericht `report`: aus der Begleitdatei, bei älteren Berichten
/// ohne Begleitdatei aus einer Zeile „Document: …“ (früher „Dokument: …“) mit vollem Pfad.
pub fn report_project(report: &Path) -> Option<PathBuf> {
    if let Ok(b) = std::fs::read(companion_path(report)) {
        let p = String::from_utf8_lossy(&b).trim().to_string();
        return (!p.is_empty()).then(|| PathBuf::from(p));
    }
    let text = String::from_utf8_lossy(&std::fs::read(report).ok()?).into_owned();
    report_document(&text).filter(|p| p.is_absolute())
}

/// Legt den Bericht (samt Begleitdatei) nach `gesendet/` neben ihm: Er gilt als erledigt und
/// wird nicht wieder angeboten. Gibt den neuen Ort zurück.
pub fn mark_sent(report: &Path) -> std::io::Result<PathBuf> {
    let dir = report
        .parent()
        .ok_or_else(|| std::io::Error::other("no folder"))?
        .join(SENT_DIR);
    let name = report
        .file_name()
        .ok_or_else(|| std::io::Error::other("no name"))?;
    std::fs::create_dir_all(&dir)?;
    let to = dir.join(name);
    std::fs::rename(report, &to)?;
    let side = companion_path(report);
    if side.exists() {
        let _ = std::fs::rename(&side, companion_path(&to));
    }
    Ok(to)
}

/// Alles, was in einen Bericht kommt.
#[derive(Clone, Debug)]
pub struct ReportInput {
    pub version: String,
    pub message: String,
    pub location: String,
    pub thread: String,
    pub backtrace: String,
    /// Rücksprungadressen und Bau-Kennung (nur Windows; [`crate::crash_addr`]).
    pub addresses: Option<crate::crash_addr::Addresses>,
    pub steps: Vec<Step>,
    pub log: Vec<String>,
    pub document: Option<PathBuf>,
    pub time: chrono::NaiveDateTime,
}

/// Der Berichtstext: beim Dokument nur der Dateiname, der Ablauf gekürzt
/// ([`trim_backtrace`]), am Ende die Rücksprungadressen
/// ([`crate::crash_addr::section_text`]). Den Benutzerordner ersetzt erst [`write_report`].
///
/// Der Bericht ist immer englisch (die Schritte und Protokollzeilen sind es schon); nur die
/// Nachricht der Panik steht, wie sie ist.
pub fn report_text(input: &ReportInput) -> String {
    let mut t = String::new();
    t.push_str("PlanoraCAD – crash report\n");
    t.push_str(&format!(
        "{TIME_KEY}: {}\n\n",
        input.time.format(REPORT_TIME_FORMAT)
    ));
    t.push_str(&format!("Version: {}\n", input.version));
    t.push_str(&format!("{ERROR_KEY}: {}\n", input.message));
    t.push_str(&format!("{LOCATION_KEY}: {}\n", input.location));
    t.push_str(&format!("Thread: {}\n", input.thread));
    // Nach dem letzten `\` oder `/`, auf jedem System (auch ein Windows-Pfad unter Linux).
    let doc = input.document.as_ref().map_or_else(
        || NO_DOCUMENT.to_string(),
        |p| {
            let full = p.to_string_lossy();
            let name = full
                .trim_end_matches(['\\', '/'])
                .rsplit(['\\', '/'])
                .next();
            name.filter(|n| !n.is_empty())
                .unwrap_or(UNKNOWN)
                .to_string()
        },
    );
    t.push_str(&format!("{DOCUMENT_KEY}: {doc}\n"));
    t.push_str("\nLast steps:\n");
    if input.steps.is_empty() {
        t.push_str("none\n");
    }
    for s in &input.steps {
        t.push_str(format!("{} {}", s.time, s.label).trim());
        t.push('\n');
    }
    t.push_str("\nLog:\n");
    if input.log.is_empty() {
        t.push_str("empty\n");
    }
    for l in &input.log {
        t.push_str(l);
        t.push('\n');
    }
    t.push_str("\nBacktrace:\n");
    t.push_str(&trim_backtrace(&input.backtrace));
    t.push_str("\n\n");
    t.push_str(&crate::crash_addr::section_text(input.addresses.as_ref()));
    t
}

/// Dateiname zum Zeitpunkt (Ortszeit), ohne `:`: `absturz-2026-10-26_14-03-22.txt`.
pub fn report_file_name(t: chrono::NaiveDateTime) -> String {
    format!("absturz-{}.txt", t.format("%Y-%m-%d_%H-%M-%S"))
}

/// Schreibt den Bericht in `dir` (wird angelegt), mit `mask` ohne Benutzerordner. Gibt es den
/// Namen schon, kommt `_2`, `_3`, … dazu; eine vorhandene Datei wird nie überschrieben. Nennt
/// der Bericht ein Dokument, steht dessen voller Pfad in der Begleitdatei ([`companion_path`]).
pub fn write_report(dir: &Path, input: &ReportInput, mask: &UserMask) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = report_file_name(input.time);
    let stem = name.trim_end_matches(".txt");
    let text = mask.apply(&report_text(input));
    for n in 1..=MAX_SUFFIX {
        let path = if n == 1 {
            dir.join(&name)
        } else {
            dir.join(format!("{stem}_{n}.txt"))
        };
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                f.write_all(text.as_bytes())?;
                if let Some(doc) = &input.document {
                    let _ = std::fs::write(companion_path(&path), doc.to_string_lossy().as_bytes());
                }
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "too many reports with the same time",
    ))
}

/// Setzt den Hook für das gemeinsame Protokoll; der bisherige Hook wird danach gerufen. Den
/// Benutzernamen ermittelt das hier einmal ([`UserMask::from_env`]), nicht erst im Hook.
pub fn install_hook(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(hook_with(
        dir,
        UserMask::from_env(),
        diag::global(),
        previous,
    ));
}

thread_local! {
    /// Gesetzt, solange dieser Faden einen Bericht schreibt.
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
}

type Hook = Box<dyn Fn(&PanicHookInfo) + Send + Sync>;

/// Baut den Hook: schreibt den Bericht nach `dir` (Protokoll aus `diag`, ohne Benutzerordner
/// nach `mask`) und ruft danach immer `previous`.
pub fn hook_with(dir: PathBuf, mask: UserMask, diag: &'static Mutex<Diag>, previous: Hook) -> Hook {
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
                let _ = write_report(&dir, &input, &mask);
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
        .unwrap_or("<unnamed>")
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
        addresses: crate::crash_addr::capture(),
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
/// erkennt (etwa die alte Version, solange der Installer sie beim Update schließt). Nur
/// [`RunMarker::finish`] löscht ihn (beim sauberen Ende, auch dem für ein Update); endet das
/// Programm anders (Absturz, Abbruch), bleibt er liegen.
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

/// Beim Start (vor dem Fenster): räumt die Berichte und alten Rückmeldungsordner auf
/// ([`cleanup_reports`], [`crate::feedback::cleanup_feedback_dirs`]), wertet
/// liegengebliebene Marker fremder Instanzen aus und legt den eigenen an. Ein gesperrter Marker
/// gehört einer laufenden Instanz und zählt nicht. Ein liegengebliebener mit einem neueren
/// Bericht ergibt [`StartFinding::Crashed`], ohne einen [`StartFinding::Unexpected`]; Berichte
/// ohne Marker (Panik mit sauberem Ende) ergeben [`StartFinding::Crashed`]. Lässt sich der
/// eigene Marker nicht anlegen, läuft das Programm ohne.
pub fn start_run(data_dir: &Path, pid: u32) -> (Option<RunMarker>, StartFinding) {
    let crash_dir = data_dir.join(CRASH_DIR);
    cleanup_reports(&crash_dir, SystemTime::now());
    cleanup_reports(&crash_dir.join(SENT_DIR), SystemTime::now());
    crate::feedback::cleanup_feedback_dirs(
        &data_dir.join(crate::feedback::FEEDBACK_DIR),
        chrono::Local::now().naive_local(),
    );
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
            diag::warn(&format!("Run marker not created ({:?})", e.kind()));
            (None, finding)
        }
    }
}

/// Löscht Berichte in `dir`, die älter als [`KEEP_DAYS`] Tage sind, und danach alle bis auf die
/// [`KEEP_MAX`] neuesten, jeweils mit Begleitdatei; verwaiste Begleitdateien ebenso. Fehler
/// (fehlender Ordner, gesperrte Datei, fremde Einträge) werden übergangen.
pub fn cleanup_reports(dir: &Path, now: SystemTime) {
    let max_age = std::time::Duration::from_secs(KEEP_DAYS * 24 * 3600);
    let mut kept = 0usize;
    for (path, time) in list_reports(dir) {
        let too_old = now.duration_since(time).is_ok_and(|age| age > max_age);
        if too_old || kept >= KEEP_MAX {
            let _ = remove_report(&path);
        } else {
            kept += 1;
        }
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let suffix = format!(".{COMPANION_EXT}");
    for e in entries.flatten() {
        let path = e.path();
        let companion = e
            .file_name()
            .to_str()
            .is_some_and(|n| n.starts_with(REPORT_PREFIX) && n.ends_with(&suffix));
        if companion
            && !path.with_extension("txt").exists()
            && e.file_type().is_ok_and(|t| t.is_file())
        {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Löscht einen Bericht samt Begleitdatei; zurück kommt das Ergebnis für den Bericht selbst.
pub fn remove_report(path: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_file(companion_path(path));
    std::fs::remove_file(path)
}

/// Wert der Zeile `<key>: …` aus einem Bericht (erste Fundstelle).
pub fn report_field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(": "))
        .map(str::trim)
}

/// Wert der Zeile `<key>: …` aus einem Bericht; `old` ist der Schlüssel älterer, deutscher
/// Berichte (vor dem englischen Bericht), der ebenfalls gilt.
pub fn report_field_either<'a>(text: &'a str, key: &str, old: &str) -> Option<&'a str> {
    report_field(text, key).or_else(|| report_field(text, old))
}

/// Das im Bericht genannte Dokument („Document: …“, in älteren Berichten „Dokument: …“), falls
/// eins genannt ist.
pub fn report_document(text: &str) -> Option<PathBuf> {
    report_field_either(text, DOCUMENT_KEY, "Dokument")
        .filter(|d| !d.is_empty() && *d != NO_DOCUMENT && *d != "keines")
        .map(PathBuf::from)
}

/// Protokollzeile statt eines Berichts bei einer abgefangenen Panik ([`expected_panics`]).
pub const CAUGHT_LINE: &str = "Caught an error while building a preview";

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
