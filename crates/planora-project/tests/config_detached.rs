//! Abschnitt `detached` der Konfiguration (abgedockte Fenster: Schlüssel → Lage und Größe in
//! physischen Pixeln): fehlt er, bleibt die Datei bytegleich; kaputte Einträge fallen einzeln
//! weg, ohne die übrigen Felder zu kosten. Nur mit eigenen Dateien im Temp-Ordner.

use planora_project::config::{Config, DetachedWindow};
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "planora-cfg-detached-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

#[test]
fn detached_empty_keeps_old_file_bytes() {
    let p = temp_file("bytes");
    // Eine Datei, wie ein Stand ohne `detached` sie geschrieben hat.
    Config {
        root: Some(PathBuf::from("D:/Projekte")),
        favorites: vec![PathBuf::from("D:/Fav")],
        panel_transparency: 25,
        language: Some("en".to_string()),
        ..Config::default()
    }
    .save_to(&p)
    .unwrap();
    let before = std::fs::read(&p).unwrap();
    assert!(!String::from_utf8_lossy(&before).contains("detached"));
    let c = Config::read_from(&p).unwrap();
    assert!(c.detached.is_empty());
    c.save_to(&p).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), before);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn detached_round_trip() {
    let p = temp_file("rund");
    let mut c = Config::default();
    c.detached.insert(
        "dialog:blend".to_string(),
        DetachedWindow {
            x: 2100.0,
            y: 120.0,
            width: 480.0,
            height: 600.0,
        },
    );
    c.save_to(&p).unwrap();
    assert_eq!(Config::read_from(&p).unwrap(), c);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn detached_bad_entries_dropped() {
    let p = temp_file("kaputt");
    std::fs::write(
        &p,
        r#"{ "language": "en", "detached": {
            "tree": {"x": 1, "y": 2, "width": -5, "height": 10},
            "right": "kaputt",
            "dialog:x": {"x": 10, "y": 20, "width": 300, "height": 400},
            "links": {"x": -1920, "y": -10, "width": 300, "height": 400},
            "null": {"x": 1, "y": 2, "width": 0, "height": 10}
        } }"#,
    )
    .unwrap();
    let c = Config::read_from(&p).unwrap();
    assert_eq!(c.language.as_deref(), Some("en"));
    let keys: Vec<&str> = c.detached.keys().map(String::as_str).collect();
    assert_eq!(keys, ["dialog:x", "links"]);
    assert_eq!(c.detached["links"].x, -1920.0);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn detached_whole_field_garbage() {
    let p = temp_file("muell");
    for text in ["5", "\"x\"", "[1]", "null", "true"] {
        std::fs::write(
            &p,
            format!(r#"{{ "language": "en", "root": "D:/P", "detached": {text} }}"#),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert!(c.detached.is_empty(), "{text}");
        assert_eq!(c.language.as_deref(), Some("en"), "{text}");
        assert_eq!(c.root, Some(PathBuf::from("D:/P")), "{text}");
    }
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}
