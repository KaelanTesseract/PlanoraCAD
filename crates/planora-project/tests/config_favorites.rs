//! Favoriten der Dateiauswahl in der Konfiguration: alte Dateien ohne das Feld laden samt
//! Wurzel, neue Dateien behalten die Favoriten. Nur mit eigenen Dateien im Temp-Ordner, nie mit
//! der echten Konfiguration.

use planora_project::config::Config;
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-fav-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

#[test]
fn an_old_config_without_favorites_keeps_its_root() {
    let p = temp_file("alt");
    std::fs::write(&p, r#"{ "root": "D:/Projekte" }"#).unwrap();
    let c = Config::load_from(&p);
    assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")));
    assert!(c.favorites.is_empty());
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn favorites_round_trip_in_order() {
    let p = temp_file("rund");
    let c = Config {
        root: Some(PathBuf::from("D:/Projekte")),
        favorites: vec![PathBuf::from("D:/Teile"), PathBuf::from("C:/Ablage")],
    };
    c.save_to(&p).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("\"favorites\""), "{text}");
    assert_eq!(Config::load_from(&p), c);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn unknown_fields_of_newer_versions_are_ignored() {
    let p = temp_file("neu");
    std::fs::write(
        &p,
        r#"{ "root": "D:/Projekte", "favorites": ["D:/Teile"], "spaeter": 3 }"#,
    )
    .unwrap();
    let c = Config::load_from(&p);
    assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")));
    assert_eq!(c.favorites, [PathBuf::from("D:/Teile")]);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn an_unreadable_config_is_not_overwritten_by_favorites() {
    let p = temp_file("kaputt");
    assert!(Config::safe_to_overwrite(&p), "fehlende Datei");
    Config::default().save_to(&p).unwrap();
    assert!(Config::safe_to_overwrite(&p), "lesbare Datei");
    std::fs::write(&p, r#"{ "root": "D:/Projekte", "#).unwrap();
    assert!(!Config::safe_to_overwrite(&p), "unlesbare Datei bleibt");
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}
