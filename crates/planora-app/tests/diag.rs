//! Protokoll im Speicher und in der rotierenden Datei.

use planora_app::diag::{self, Diag, LOG_DIR, LOG_NAME, Level, MAX_LOG_BYTES, RING_LINES};
use planora_app::workspace::*;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::docstore::LockMode;
use planora_project::fsutil::new_id;
use planora_project::library::Library;
use std::path::PathBuf;

fn temp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("planora-diag-{tag}-{}", new_id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn logs(dir: &std::path::Path) -> PathBuf {
    dir.join(LOG_DIR)
}

#[test]
fn ring_keeps_the_last_500_lines_in_order() {
    let mut d = Diag::with_dir(&temp("ring"));
    for i in 0..(RING_LINES + 20) {
        d.log(Level::Info, &format!("Zeile {i}"));
    }
    let all = d.recent_lines(10_000);
    assert_eq!(all.len(), RING_LINES);
    assert!(all[0].ends_with("Zeile 20"));
    assert!(all[RING_LINES - 1].ends_with(&format!("Zeile {}", RING_LINES + 19)));
    let last3 = d.recent_lines(3);
    assert_eq!(last3.len(), 3);
    assert!(last3[2].ends_with(&format!("Zeile {}", RING_LINES + 19)));
}

#[test]
fn lines_go_to_the_file_with_level_and_time() {
    let dir = temp("file");
    let mut d = Diag::with_dir(&dir);
    d.log(Level::Info, "Hallo");
    d.log(Level::Warn, "Achtung");
    d.log(Level::Fehler, "Kaputt");
    let text = std::fs::read_to_string(logs(&dir).join(LOG_NAME)).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3);
    // JJJJ-MM-TT hh:mm:ss STUFE Text
    let b = lines[0].as_bytes();
    assert!(lines[0].len() > 20);
    assert_eq!(
        (b[4], b[7], b[10], b[13], b[16], b[19]),
        (b'-', b'-', b' ', b':', b':', b' ')
    );
    assert!(lines[0].ends_with(" INFO Hallo"));
    assert!(lines[1].ends_with(" WARN Achtung"));
    assert!(lines[2].ends_with(" FEHLER Kaputt"));
    assert!(!d.file_error());
}

#[test]
fn file_rotates_at_1_mb_and_keeps_three_files() {
    let dir = temp("rot");
    let mut d = Diag::with_dir(&dir);
    let line = "x".repeat(1000);
    let l = logs(&dir);
    // rund 1030 Byte je Zeile: 1100 Zeilen überschreiten 1 MB
    for _ in 0..1100 {
        d.log(Level::Info, &line);
    }
    let main = std::fs::metadata(l.join(LOG_NAME)).unwrap().len();
    let first = std::fs::metadata(l.join("planora.1.log")).unwrap().len();
    assert!(main < 100_000, "{main}");
    assert!(
        first <= MAX_LOG_BYTES && first > MAX_LOG_BYTES - 2000,
        "{first}"
    );
    for _ in 0..(3 * 1100) {
        d.log(Level::Info, &line);
    }
    assert!(l.join("planora.2.log").exists());
    assert!(!l.join("planora.3.log").exists());
}

#[test]
fn detail_lines_only_when_verbose() {
    let mut d = Diag::with_dir(&temp("detail"));
    d.log(Level::Detail, "leise");
    assert!(d.recent_lines(10).is_empty());
    d.set_verbose(true);
    d.log(Level::Detail, "laut");
    let lines = d.recent_lines(10);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].ends_with(" DETAIL laut"));
    d.set_verbose(false);
    d.log(Level::Detail, "wieder leise");
    assert_eq!(d.recent_lines(10).len(), 1);
}

#[test]
fn unwritable_dir_stays_in_memory_and_reports_file_error() {
    let base = temp("bad").join("eine-datei");
    std::fs::write(&base, b"x").unwrap();
    let mut d = Diag::with_dir(&base);
    assert!(d.file_error());
    d.log(Level::Info, "trotzdem da");
    let lines = d.recent_lines(5);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].ends_with("trotzdem da"));
}

#[test]
fn poisoned_global_lock_still_logs() {
    let r = std::thread::spawn(|| {
        let _g = diag::global().lock().unwrap();
        panic!("absichtlich");
    })
    .join();
    assert!(r.is_err());
    diag::info("nach der Vergiftung");
    assert!(
        diag::recent_lines(50)
            .iter()
            .any(|l| l.ends_with("nach der Vergiftung"))
    );
}

#[test]
fn save_and_open_write_one_line_without_content() {
    let root = temp("ws");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let id = w
        .active_tab_mut()
        .state
        .create_box(DVec3::splat(5.0))
        .unwrap();
    w.active_tab_mut()
        .state
        .rename_feature(id, "Geheimteil")
        .unwrap();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    let path = w.save_as(&p, "Mein Dokument", "").unwrap();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(2.0))
        .unwrap();
    w.save_active("", None).unwrap();
    w.close_tab(0);
    w.open_document(&path, LockMode::Acquire).unwrap();
    let all = diag::recent_lines(500).join("\n");
    assert!(all.contains("Gespeichert"), "{all}");
    assert!(all.contains("Geöffnet"), "{all}");
    assert!(all.contains("Mein Dokument"), "{all}");
    assert!(!all.contains("Geheimteil"));
}
