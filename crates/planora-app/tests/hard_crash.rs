//! Bericht bei harten Abstürzen unter Windows (Zugriffsfehler, Stapelüberlauf): Sie gehen am
//! Panik-Hook vorbei; ein Ausnahmefilter schreibt dann einen kurzen Bericht nach `abstuerze/`,
//! damit der nächste Start ihn findet. Geprüft in einem Kindprozess dieses Tests, der wirklich
//! abstürzt. Nur Temp-Ordner.

#![cfg(windows)]

mod common;

use planora_app::{crash, hard_crash};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Umgebungsvariable des Kindprozesses: Art des Absturzes.
const CHILD_KIND: &str = "PLANORA_HARD_CRASH_CHILD";
/// Umgebungsvariable des Kindprozesses: Ordner der Berichte.
const CHILD_DIR: &str = "PLANORA_HARD_CRASH_DIR";

#[test]
fn exception_names_are_plain() {
    assert_eq!(hard_crash::exception_name(0xC000_0005), "access violation");
    assert_eq!(hard_crash::exception_name(0xC000_00FD), "stack overflow");
    assert_eq!(
        hard_crash::exception_name(0xC000_0094),
        "integer division by zero"
    );
    assert_eq!(hard_crash::exception_name(0x1234_5678), "unknown exception");
    assert_eq!(
        hard_crash::message(0xC000_0005),
        "Windows exception 0xC0000005 (access violation)"
    );
}

/// Im Kindprozess: Filter setzen und abstürzen. Im normalen Lauf tut der Test nichts.
#[test]
fn child_crashes_on_request() {
    let (Ok(kind), Ok(dir)) = (std::env::var(CHILD_KIND), std::env::var(CHILD_DIR)) else {
        return;
    };
    hard_crash::install(PathBuf::from(dir), crash::UserMask::none());
    planora_app::diag::info("child before crash");
    match kind.as_str() {
        "av" => {
            // SAFETY: absichtlich nicht – der Schreibzugriff auf Adresse 16 soll abstürzen.
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::write_volatile(16 as *mut u64, 1);
            }
        }
        "so" => {
            #[allow(unconditional_recursion)]
            fn deep(n: u64, pad: &mut [u8; 4096]) -> u64 {
                pad[0] = n as u8;
                let mut next = [0u8; 4096];
                deep(n + 1, &mut next) + u64::from(std::hint::black_box(pad)[1])
            }
            let mut pad = [0u8; 4096];
            std::hint::black_box(deep(0, &mut pad));
        }
        _ => {}
    }
}

/// Startet den Kindprozess für `kind` und gibt die Berichte im Ordner zurück.
fn crash_child(kind: &str) -> (std::process::ExitStatus, Vec<PathBuf>) {
    let dir = common::temp_root(&format!("planora-hard-crash-{kind}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_crashes_on_request",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_KIND, kind)
        .env(CHILD_DIR, &dir)
        .status()
        .expect("Kindprozess startet");
    (status, reports(&dir))
}

fn reports(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|r| {
            r.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "txt"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn access_violation_leaves_a_report() {
    let (status, found) = crash_child("av");
    assert!(!status.success(), "der Kindprozess stürzt ab");
    assert_eq!(found.len(), 1, "genau ein Bericht: {found:?}");
    let text = std::fs::read_to_string(&found[0]).unwrap();
    assert!(text.contains("0xC0000005"), "{text}");
    assert!(text.contains("access violation"), "{text}");
    assert!(text.contains(crash_addr_section()), "Adressen: {text}");
    assert!(text.contains("child before crash"), "Protokoll: {text}");
}

#[test]
fn stack_overflow_leaves_a_report() {
    let (status, found) = crash_child("so");
    assert!(!status.success(), "der Kindprozess stürzt ab");
    assert_eq!(found.len(), 1, "genau ein Bericht: {found:?}");
    let text = std::fs::read_to_string(&found[0]).unwrap();
    assert!(text.contains("0xC00000FD"), "{text}");
    assert!(text.contains("stack overflow"), "{text}");
}

fn crash_addr_section() -> &'static str {
    planora_app::crash_addr::SECTION
}
