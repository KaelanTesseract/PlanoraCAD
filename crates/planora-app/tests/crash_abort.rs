//! Ende durch `abort`: Der Test startet sich selbst als Kindprozess, der einen Laufmarker mit
//! Mitschrift anlegt und `abort()` der C-Laufzeit ruft. Der nächste Start findet den Marker und
//! zeigt den Hinweis mit der Mitschrift (kein Abfang im Prozess: Er bräuchte Speicher und könnte
//! nach einem Heap-Fehler hängen bleiben).

#![allow(unsafe_code)]

mod common;

use planora_app::crash::{self, StartFinding, start_run};
use std::path::PathBuf;
use std::process::Command;

/// Umgebungsvariable: Datenordner; gesetzt nur im Kindprozess.
const CHILD_DIR: &str = "PLANORA_TEST_ABORT_DIR";
/// Prozessnummer, die es nie gibt (größer als jede echte).
const OWN: u32 = 4_000_000_001;

unsafe extern "C" {
    fn abort() -> !;
    #[cfg(windows)]
    fn _set_abort_behavior(flags: u32, mask: u32) -> u32;
}

/// Im Kindprozess: Marker mit Mitschrift anlegen und abbrechen.
fn child(dir: PathBuf) -> ! {
    let (marker, _) = start_run(&dir, std::process::id());
    let mut marker = marker.expect("Marker im Kindprozess");
    let text = crash::run_snapshot(
        "0.2.0-dev.7",
        &[planora_app::diag::Step {
            time: "09:15:00".into(),
            label: "Vor dem Abbruch".into(),
        }],
        &[],
        None,
    );
    assert!(marker.refresh(&text));
    // Unter Windows ohne Meldungsfenster und ohne Windows-Fehlerberichterstattung.
    #[cfg(windows)]
    unsafe {
        _set_abort_behavior(0, 0x1 | 0x2);
    }
    unsafe { abort() }
}

#[test]
fn an_abort_leaves_the_marker_and_the_next_start_offers_its_log() {
    if let Ok(dir) = std::env::var(CHILD_DIR) {
        child(PathBuf::from(dir));
    }
    let dir = common::temp_root("planora-abort");
    std::fs::create_dir_all(&dir).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "an_abort_leaves_the_marker_and_the_next_start_offers_its_log",
            "--test-threads=1",
        ])
        .env(CHILD_DIR, &dir)
        .status()
        .unwrap();
    assert!(!status.success(), "Kindprozess bricht ab");
    let (marker, finding) = start_run(&dir, OWN);
    let StartFinding::Unexpected(Some(log)) = finding else {
        panic!("erwartet Hinweis mit Mitschrift, war {finding:?}");
    };
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains(crash::UNCLEAN_MESSAGE), "{text}");
    assert!(text.contains("09:15:00 Vor dem Abbruch"), "{text}");
    marker.unwrap().finish();
}
