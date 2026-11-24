//! Absturzbericht bei `abort`: Der Test startet sich selbst als Kindprozess, der den Abfang
//! einrichtet und `abort()` der C-Laufzeit ruft; danach liegt ein Bericht im Ordner.

#![allow(unsafe_code)]

mod common;

use planora_app::crash::{self, ABORT_MESSAGE, UserMask};
use std::path::PathBuf;
use std::process::Command;

/// Umgebungsvariable: Ordner für den Bericht; gesetzt nur im Kindprozess.
const CHILD_DIR: &str = "PLANORA_TEST_ABORT_DIR";

unsafe extern "C" {
    fn abort() -> !;
    #[cfg(windows)]
    fn _set_abort_behavior(flags: u32, mask: u32) -> u32;
}

/// Im Kindprozess: Abfang einrichten und abbrechen.
fn child(dir: PathBuf) -> ! {
    crash::install_abort_handler(dir, UserMask::none());
    planora_app::diag::step("Vor dem Abbruch");
    // Unter Windows ohne Meldungsfenster und ohne Windows-Fehlerberichterstattung.
    #[cfg(windows)]
    unsafe {
        _set_abort_behavior(0, 0x1 | 0x2);
    }
    unsafe { abort() }
}

#[test]
fn abort_writes_a_report() {
    if let Ok(dir) = std::env::var(CHILD_DIR) {
        child(PathBuf::from(dir));
    }
    let dir = common::temp_root("planora-abort");
    std::fs::create_dir_all(&dir).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "abort_writes_a_report", "--test-threads=1"])
        .env(CHILD_DIR, &dir)
        .status()
        .unwrap();
    assert!(!status.success(), "Kindprozess bricht ab");
    let reports: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    assert_eq!(reports.len(), 1, "genau ein Bericht: {reports:?}");
    let text = std::fs::read_to_string(&reports[0]).unwrap();
    assert!(text.contains(ABORT_MESSAGE), "{text}");
    assert!(text.contains("Vor dem Abbruch"), "{text}");
    assert!(text.contains("Backtrace:"), "{text}");
}
