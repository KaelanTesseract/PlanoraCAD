//! Abschnitt `keys` der Konfiguration (Abweichungen der Tastenbelegung): fehlt er, ist er leer;
//! ein falscher Typ macht die Datei nicht unlesbar; andere Einstellungen und die Tasten
//! überschreiben einander beim Lesen-Ändern-Schreiben nicht. Nur mit eigenen Dateien im
//! Temp-Ordner, nie mit der echten Konfiguration.

use planora_project::config::Config;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-keys-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

fn keys(entries: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
        .collect()
}

#[test]
fn config_without_keys_reads_as_defaults() {
    let p = temp_file("ohne");
    std::fs::write(&p, r#"{ "root": "D:/Projekte", "panel_transparency": 30 }"#).unwrap();
    let c = Config::read_from(&p).unwrap();
    assert!(c.keys.is_empty());
    assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")));
    assert!(Config::default().keys.is_empty());
    // Ohne Abweichungen wird der Abschnitt gar nicht geschrieben.
    Config::default().save_to(&p).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(!text.contains("\"keys\""), "{text}");
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn config_with_wrong_type_reads_empty() {
    let p = temp_file("falsch");
    for text in ["5", "\"Strg+S\"", "[\"Strg+S\"]", "null", "true"] {
        std::fs::write(
            &p,
            format!(r#"{{ "root": "D:/Projekte", "favorites": ["D:/A"], "keys": {text} }}"#),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert!(c.keys.is_empty(), "{text}: {:?}", c.keys);
        assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")), "{text}");
        assert_eq!(c.favorites, [PathBuf::from("D:/A")], "{text}");
    }
    // Einzelne kaputte Einträge bleiben als Text erhalten (die Tastenbelegung meldet sie als
    // ungültig), statt still zu verschwinden; eine leere Liste bleibt leer (= ohne Belegung).
    std::fs::write(
        &p,
        r#"{ "keys": { "datei.neu": ["Strg+N", 5], "datei.oeffnen": "Strg+O", "datei.speichern": [] } }"#,
    )
    .unwrap();
    let c = Config::read_from(&p).unwrap();
    assert_eq!(c.keys["datei.neu"], ["Strg+N", "5"]);
    assert_eq!(c.keys["datei.oeffnen"], ["\"Strg+O\""]);
    assert!(c.keys["datei.speichern"].is_empty());
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn keys_round_trip() {
    let p = temp_file("rund");
    let c = Config {
        keys: keys(&[("bearbeiten.wiederholen", &["Strg+Y"]), ("datei.neu", &[])]),
        ..Config::default()
    };
    c.save_to(&p).unwrap();
    assert_eq!(Config::read_from(&p).unwrap(), c);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn keys_survive_other_config_updates() {
    let p = temp_file("andere");
    let k = keys(&[("datei.neu", &["Strg+Umschalt+N"])]);
    Config {
        keys: k.clone(),
        ..Config::default()
    }
    .save_to(&p)
    .unwrap();
    Config::update_at(&p, |c| c.root = Some(PathBuf::from("D:/Neu"))).unwrap();
    Config::update_at(&p, |c| c.favorites = vec![PathBuf::from("D:/Fav")]).unwrap();
    Config::update_at(&p, |c| c.panel_transparency = 12).unwrap();
    let disk = Config::read_from(&p).unwrap();
    assert_eq!(disk.keys, k);
    assert_eq!(disk.root, Some(PathBuf::from("D:/Neu")));
    assert_eq!(disk.favorites, [PathBuf::from("D:/Fav")]);
    assert_eq!(disk.panel_transparency, 12);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn other_settings_survive_keys_updates() {
    let p = temp_file("umgekehrt");
    Config {
        root: Some(PathBuf::from("D:/Projekte")),
        favorites: vec![PathBuf::from("D:/Fav")],
        panel_transparency: 25,
        ..Config::default()
    }
    .save_to(&p)
    .unwrap();
    let k = keys(&[("bearbeiten.rueckgaengig", &["Alt+Z"])]);
    Config::update_at(&p, |c| c.keys = k.clone()).unwrap();
    let disk = Config::read_from(&p).unwrap();
    assert_eq!(disk.keys, k);
    assert_eq!(disk.root, Some(PathBuf::from("D:/Projekte")));
    assert_eq!(disk.favorites, [PathBuf::from("D:/Fav")]);
    assert_eq!(disk.panel_transparency, 25);
    // Leeren entfernt den Abschnitt wieder.
    Config::update_at(&p, |c| c.keys.clear()).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(!text.contains("\"keys\""), "{text}");
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}
