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
    assert_eq!(RING_LINES, 500);
    for i in 0..520 {
        d.log(Level::Info, &format!("Zeile {i}"));
    }
    let all = d.recent_lines(10_000);
    assert_eq!(all.len(), 500);
    assert!(all[0].ends_with("Zeile 20"));
    assert!(all[499].ends_with("Zeile 519"));
    let last3 = d.recent_lines(3);
    assert_eq!(last3.len(), 3);
    assert!(last3[2].ends_with("Zeile 519"));
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
    // JJJJ-MM-TTThh:mm:ssZ STUFE Text (UTC)
    let stamp = &lines[0][..20];
    let t = chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%dT%H:%M:%SZ").unwrap();
    assert!(t.and_utc().timestamp() > 1_700_000_000);
    assert!(lines[0].ends_with("Z INFO Hallo"));
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

#[test]
fn control_characters_become_spaces() {
    let mut d = Diag::in_memory();
    d.log(Level::Info, "a\r\nb\x1b[31m\tc\u{2028}d\u{2029}e\u{85}f");
    let lines = d.recent_lines(5);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].ends_with(" INFO a  b [31m c d e f"),
        "{:?}",
        lines[0]
    );
    assert!(!lines[0].chars().any(|c| c.is_control()));
}

#[test]
fn long_lines_are_cut_at_a_char_boundary() {
    let mut d = Diag::in_memory();
    d.log(Level::Info, &"x".repeat(5000));
    d.log(Level::Info, &"ä".repeat(3000));
    // Mehrbyte-Zeichen genau an der Grenze: kein Panik, gültiger Text
    d.log(Level::Info, &format!("{}€€€", "x".repeat(1990)));
    let lines = d.recent_lines(5);
    for l in &lines {
        let text = l.splitn(3, ' ').nth(2).unwrap();
        assert!(text.chars().count() <= 2000, "{}", text.chars().count());
    }
    let first = lines[0].splitn(3, ' ').nth(2).unwrap();
    assert_eq!(first.chars().count(), 2000);
    assert!(first.ends_with('…'));
    assert!(lines[1].ends_with('…'));
}

#[test]
fn rotation_survives_a_foreign_growth_of_the_file() {
    let dir = temp("foreign");
    let mut d = Diag::with_dir(&dir);
    d.log(Level::Info, "eins");
    // eine zweite Programminstanz hat die Datei inzwischen verlängert
    let path = logs(&dir).join(LOG_NAME);
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all("y".repeat(1_100_000).as_bytes()).unwrap();
        f.write_all(b"\n").unwrap();
    }
    d.log(Level::Info, "zwei");
    let old = std::fs::read_to_string(logs(&dir).join("planora.1.log")).unwrap();
    let new = std::fs::read_to_string(&path).unwrap();
    assert!(old.contains("eins"));
    assert!(new.contains("zwei") && new.len() < 1000);
    assert!(!old.contains("zwei"));
}

#[test]
fn a_write_error_is_not_permanent() {
    let dir = temp("retry");
    let mut d = Diag::with_dir(&dir);
    d.log(Level::Info, "davor");
    let path = logs(&dir).join(LOG_NAME);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap(); // Schreiben scheitert
    d.log(Level::Info, "verpasst");
    assert!(d.file_error());
    d.log(Level::Info, "im Speicher");
    std::fs::remove_dir(&path).unwrap();
    for i in 0..60 {
        d.log(Level::Info, &format!("danach {i}"));
    }
    assert!(!d.file_error());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("danach 59"));
    // was in der Zwischenzeit nicht geschrieben werden konnte, kommt nach
    assert!(text.contains("verpasst") && text.contains("im Speicher"));
}

#[test]
fn init_global_twice_keeps_lines_without_doubling() {
    let dir = temp("init");
    diag::info("vor dem Einrichten");
    diag::init_global(&dir);
    diag::init_global(&dir);
    diag::info("danach");
    let text = std::fs::read_to_string(logs(&dir).join(LOG_NAME)).unwrap();
    assert_eq!(text.matches("vor dem Einrichten").count(), 1);
    assert_eq!(text.matches(" danach").count(), 1);
    let ring = diag::recent_lines(500);
    assert_eq!(
        ring.iter()
            .filter(|l| l.ends_with("vor dem Einrichten"))
            .count(),
        1
    );
}

#[test]
fn two_thousand_detail_lines_stay_quick() {
    let dir = temp("tempo");
    let mut d = Diag::with_dir(&dir);
    d.set_verbose(true);
    let start = std::time::Instant::now();
    for i in 0..2000 {
        d.log(Level::Detail, &format!("Befehl Rückgängig: {i} ms"));
    }
    let took = start.elapsed();
    eprintln!("2000 Zeilen: {took:?}");
    let text = std::fs::read_to_string(logs(&dir).join(LOG_NAME)).unwrap();
    assert_eq!(text.lines().count(), 2000);
    // Grob: im Mittel unter 1 ms je Zeile (im UI-Faden kaum spürbar).
    assert!(took < std::time::Duration::from_secs(2), "{took:?}");
}

/// Schreibt `n` Zeilen zu je rund 1 kB (überschreitet bei 1100 Zeilen die Grenze).
fn fill(d: &mut Diag, tag: &str, n: usize) {
    for i in 0..n {
        d.log(Level::Info, &format!("{tag} {i} {}", "z".repeat(1000)));
    }
}

#[test]
fn a_failed_rotation_keeps_older_files_and_pauses() {
    let dir = temp("rot-fail");
    let l = logs(&dir);
    let mut d = Diag::with_dir(&dir);
    std::fs::write(l.join("planora.1.log"), "eins-alt\n").unwrap();
    // Die älteste Datei lässt sich nicht entfernen (ein Ordner mit Inhalt an ihrer Stelle):
    // dann darf nichts weitergeschoben werden.
    let oldest = l.join("planora.2.log");
    std::fs::create_dir(&oldest).unwrap();
    std::fs::write(oldest.join("drin"), "x").unwrap();
    fill(&mut d, "a", 1100);
    assert!(d.file_error(), "Drehen gescheitert wird gemeldet");
    assert_eq!(
        std::fs::read_to_string(l.join("planora.1.log")).unwrap(),
        "eins-alt\n",
        "ältere Datei bleibt"
    );
    let size = std::fs::metadata(l.join(LOG_NAME)).unwrap().len();
    assert!(size > MAX_LOG_BYTES, "weiter in die aktuelle Datei");
    // Hindernis weg: nicht gleich in der nächsten Zeile erneut versucht, erst nach einer Pause.
    std::fs::remove_dir_all(&oldest).unwrap();
    d.log(Level::Info, "gleich danach");
    assert_eq!(
        std::fs::read_to_string(l.join("planora.1.log")).unwrap(),
        "eins-alt\n",
        "kein erneuter Versuch in jeder Zeile"
    );
    fill(&mut d, "b", 60);
    let one = std::fs::read_to_string(l.join("planora.1.log")).unwrap();
    assert!(one.contains("gleich danach"), "nach der Pause gedreht");
    assert_eq!(
        std::fs::read_to_string(l.join("planora.2.log")).unwrap(),
        "eins-alt\n"
    );
    assert!(!d.file_error());
    let text = std::fs::read_to_string(l.join(LOG_NAME)).unwrap();
    assert!(text.contains("b 59"));
}

/// Unter Windows: die aktuelle Datei ist offen ohne Erlaubnis zum Umbenennen (etwa ein
/// Virenscanner); sie bleibt, wird weiter beschrieben, und die älteren Dateien gehen über
/// wiederholte Versuche nicht nach und nach verloren.
#[cfg(windows)]
#[test]
fn a_locked_current_file_never_eats_the_older_ones() {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    let dir = temp("rot-lock");
    let l = logs(&dir);
    let mut d = Diag::with_dir(&dir);
    d.log(Level::Info, "start");
    std::fs::write(l.join("planora.1.log"), "eins-alt\n").unwrap();
    std::fs::write(l.join("planora.2.log"), "zwei-alt\n").unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(l.join(LOG_NAME))
        .unwrap();
    fill(&mut d, "a", 1100);
    assert!(d.file_error());
    // Viele weitere Versuche (je nach der Pause).
    fill(&mut d, "b", 300);
    let older: Vec<String> = ["planora.1.log", "planora.2.log"]
        .iter()
        .filter_map(|n| std::fs::read_to_string(l.join(n)).ok())
        .collect();
    assert!(
        older.iter().any(|t| t == "eins-alt\n"),
        "die jüngere ältere Datei bleibt: {:?}",
        older.iter().map(|t| t.len()).collect::<Vec<_>>()
    );
    let text = std::fs::read_to_string(l.join(LOG_NAME)).unwrap();
    assert!(text.contains("b 299"), "weiter geschrieben");
    drop(held);
    fill(&mut d, "c", 60);
    assert!(!d.file_error());
    assert!(
        std::fs::read_to_string(l.join("planora.1.log"))
            .unwrap()
            .contains("b 299")
    );
}
