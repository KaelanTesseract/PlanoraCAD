//! Absturzbericht: Panic-Hook, Berichtstext, Dateiname.
//!
//! Tests, die den globalen Hook setzen, laufen nacheinander (gemeinsame Sperre) und stellen
//! den vorherigen Hook wieder her.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use planora_app::crash::{self, ReportInput};
use planora_app::diag::{Diag, Level, Step};
use planora_app::feedback::{self, Personal};

/// Nur ein Test zur Zeit darf den globalen Hook tauschen.
static HOOK_LOCK: Mutex<()> = Mutex::new(());

fn time() -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 26)
        .and_then(|d| d.and_hms_opt(14, 3, 22))
        .expect("gültige Zeit")
}

fn input() -> ReportInput {
    ReportInput {
        version: "0.2.0-dev.1".into(),
        message: "Index außerhalb".into(),
        location: "crates/planora-app/src/app.rs:10:5".into(),
        thread: "main".into(),
        backtrace: "0: planora_app::app::run".into(),
        steps: vec![Step {
            time: "14:03:20".into(),
            label: "Skizze extrudieren".into(),
        }],
        log: vec!["2026-10-26T13:03:19Z INFO Start".into()],
        document: Some(PathBuf::from("Teil.pcad")),
        time: time(),
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "planora-crash-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn leaked_diag() -> &'static Mutex<Diag> {
    Box::leak(Box::new(Mutex::new(Diag::in_memory())))
}

fn reports(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Setzt den Hook aus [`crash::hook_with`] mit einem Zähler als `previous`, führt `f` aus und
/// stellt den alten Hook wieder her.
fn with_hook(dir: &Path, diag: &'static Mutex<Diag>, f: impl FnOnce()) -> usize {
    let _g = HOOK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let old = std::panic::take_hook();
    let hook = crash::hook_with(
        dir.to_path_buf(),
        diag,
        Box::new(move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        }),
    );
    std::panic::set_hook(hook);
    f();
    std::panic::set_hook(old);
    count.load(Ordering::SeqCst)
}

fn panic_in_thread(name: &str, f: impl FnOnce() + Send + 'static) {
    let h = std::thread::Builder::new()
        .name(name.into())
        .spawn(f)
        .expect("Faden startet");
    assert!(h.join().is_err(), "der Faden muss paniken");
}

#[test]
fn report_text_has_all_sections() {
    let t = crash::report_text(&input());
    for s in [
        "Version",
        "Fehler",
        "Ort",
        "Faden",
        "Letzte Schritte",
        "Protokoll",
        "Ablauf (Backtrace)",
    ] {
        assert!(t.contains(s), "Abschnitt {s} fehlt in:\n{t}");
    }
    assert!(t.contains("0.2.0-dev.1"));
    assert!(t.contains("Index außerhalb"));
    assert!(t.contains("app.rs:10:5"));
    assert!(t.contains("14:03:20 Skizze extrudieren"));
    assert!(t.contains("INFO Start"));
    assert!(t.contains("planora_app::app::run"));
    assert!(t.contains("Teil.pcad"));
}

#[test]
fn report_file_name_format() {
    assert_eq!(
        crash::report_file_name(time()),
        "absturz-2026-10-26_14-03-22.txt"
    );
    assert!(!crash::report_file_name(time()).contains(':'));
    assert_eq!(crash::CRASH_DIR, "abstuerze");
    assert_eq!(crash::LOG_LINES_IN_REPORT, 100);
}

#[test]
fn write_report_adds_suffix_on_collision() {
    let dir = temp_dir("kollision");
    let a = crash::write_report(&dir, &input()).expect("erster Bericht");
    let b = crash::write_report(&dir, &input()).expect("zweiter Bericht");
    assert_eq!(
        a.file_name().and_then(|n| n.to_str()),
        Some("absturz-2026-10-26_14-03-22.txt")
    );
    assert_eq!(
        b.file_name().and_then(|n| n.to_str()),
        Some("absturz-2026-10-26_14-03-22_2.txt")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn panicking_thread_writes_a_report_and_calls_previous_hook() {
    let dir = temp_dir("einfach");
    let calls = with_hook(&dir, leaked_diag(), || {
        panic_in_thread("absturz-faden", || panic!("Testpanik"));
    });
    assert_eq!(calls, 1);
    let files = reports(&dir);
    assert_eq!(files.len(), 1, "genau ein Bericht: {files:?}");
    let text = std::fs::read_to_string(&files[0]).expect("lesbar");
    assert!(text.contains("Testpanik"), "{text}");
    assert!(text.contains("crash.rs"), "{text}");
    assert!(text.contains("absturz-faden"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hook_does_not_block_when_diag_is_locked() {
    let dir = temp_dir("gesperrt");
    let diag = leaked_diag();
    let start = Instant::now();
    let calls = with_hook(&dir, diag, || {
        let _held = diag.lock().expect("Sperre");
        panic_in_thread("gesperrt", || panic!("Panik bei Sperre"));
    });
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(calls, 1);
    let files = reports(&dir);
    assert_eq!(files.len(), 1);
    let text = std::fs::read_to_string(&files[0]).expect("lesbar");
    assert!(text.contains("nicht verfügbar"), "{text}");
    assert!(text.contains("Panik bei Sperre"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn panic_inside_the_hook_is_swallowed() {
    // Der Berichtsordner ist eine Datei: Schreiben scheitert, der Hook bleibt ruhig.
    let base = temp_dir("datei");
    std::fs::create_dir_all(&base).expect("Ordner");
    let dir = base.join("abstuerze");
    std::fs::write(&dir, "keine Mappe").expect("Datei");
    let calls = with_hook(&dir, leaked_diag(), || {
        panic_in_thread("datei", || panic!("Panik ohne Ordner"));
    });
    assert_eq!(calls, 1, "der vorherige Hook wird trotzdem gerufen");
    assert!(dir.is_file());
    assert_eq!(reports(&base).len(), 1, "nur die Datei selbst");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn two_panics_write_two_reports() {
    let dir = temp_dir("zwei");
    let calls = with_hook(&dir, leaked_diag(), || {
        panic_in_thread("eins", || panic!("erste Panik"));
        panic_in_thread("zwei", || panic!("zweite Panik"));
    });
    assert_eq!(calls, 2);
    assert_eq!(reports(&dir).len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn non_string_payload_is_unknown() {
    let dir = temp_dir("zahl");
    let calls = with_hook(&dir, leaked_diag(), || {
        panic_in_thread("zahl", || std::panic::panic_any(42_i32));
    });
    assert_eq!(calls, 1);
    let files = reports(&dir);
    assert_eq!(files.len(), 1);
    let text = std::fs::read_to_string(&files[0]).expect("lesbar");
    assert!(text.contains("unbekannt"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn report_keeps_steps_and_100_log_lines() {
    let dir = temp_dir("zeilen");
    let diag = leaked_diag();
    {
        let mut d = diag.lock().expect("Sperre");
        d.step("Skizze anlegen");
        d.step("Extrudieren");
        for i in 0..150 {
            d.log(Level::Info, &format!("Zeile {i:03}"));
        }
    }
    with_hook(&dir, diag, || {
        panic_in_thread("zeilen", || panic!("Panik mit Protokoll"));
    });
    let files = reports(&dir);
    assert_eq!(files.len(), 1);
    let text = std::fs::read_to_string(&files[0]).expect("lesbar");
    assert!(text.contains("Skizze anlegen"));
    assert!(text.contains("Extrudieren"));
    assert!(!text.contains("Zeile 049"), "nur die letzten 100 Zeilen");
    assert!(text.contains("Zeile 050"));
    assert!(text.contains("Zeile 149"));
    let n = text.lines().filter(|l| l.contains("INFO Zeile")).count();
    assert_eq!(n, crash::LOG_LINES_IN_REPORT);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shown_report_is_sanitized() {
    let mut i = input();
    i.backtrace = "0: planora_app::app::run\n  at C:\\Users\\Mustermann\\src\\app.rs:10".into();
    i.document = Some(PathBuf::from(r"C:\Users\Mustermann\Kunde\Teil.pcad"));
    let raw = crash::report_text(&i);
    assert!(raw.contains("Mustermann"), "unbereinigt gespeichert");
    let who = Personal {
        user_names: vec!["Mustermann".into()],
        home_dirs: vec![r"C:\Users\Mustermann".into()],
        project_paths: vec![],
    };
    let shown = feedback::sanitize(&raw, &who);
    assert!(!shown.contains("Mustermann"), "{shown}");
    assert!(!shown.contains("Kunde"), "{shown}");
    assert!(shown.contains("Teil.pcad"), "{shown}");
}

#[test]
fn diag_keeps_current_document() {
    let mut d = Diag::in_memory();
    assert_eq!(d.current_document(), None);
    d.set_document(Some(PathBuf::from("Teil.pcad")));
    assert_eq!(d.current_document(), Some(PathBuf::from("Teil.pcad")));
    d.set_document(None);
    assert_eq!(d.current_document(), None);
}
