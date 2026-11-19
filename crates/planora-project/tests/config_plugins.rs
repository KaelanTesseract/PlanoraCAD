//! Abschnitt `plugins` der Konfiguration (Plug-ins Etappe 1, Task 4): je Plug-in „an/aus“ und die
//! gespeicherten Rechte. Fehlt er, bleibt die Datei bytegleich; kaputte Einträge fallen einzeln weg,
//! ohne die übrigen Felder zu kosten. Nur mit eigenen Dateien im Temp-Ordner.

use planora_project::config::{Config, PluginConfig};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-plugins-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

#[test]
fn plugins_roundtrip() {
    let p = temp_file("rund");
    let mut c = Config::default();
    c.plugins.insert(
        "planora.beispiel-hallo".to_string(),
        PluginConfig {
            enabled: false,
            rights: BTreeMap::from([("net".to_string(), "allowed".to_string())]),
        },
    );
    c.save_to(&p).unwrap();
    assert_eq!(Config::read_from(&p).unwrap(), c);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn plugins_absent_not_written() {
    let p = temp_file("bytes");
    Config {
        root: Some(PathBuf::from("D:/Projekte")),
        language: Some("en".to_string()),
        ..Config::default()
    }
    .save_to(&p)
    .unwrap();
    let before = std::fs::read(&p).unwrap();
    assert!(!String::from_utf8_lossy(&before).contains("plugins"));
    let c = Config::read_from(&p).unwrap();
    assert!(c.plugins.is_empty());
    c.save_to(&p).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), before);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn plugins_lenient() {
    let p = temp_file("kaputt");
    std::fs::write(
        &p,
        r#"{ "root": "D:/Projekte", "language": "en", "plugins": 5 }"#,
    )
    .unwrap();
    let c = Config::read_from(&p).unwrap();
    assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")));
    assert_eq!(c.language.as_deref(), Some("en"));
    assert!(c.plugins.is_empty());

    std::fs::write(
        &p,
        r#"{ "root": "D:/Projekte", "language": "en", "plugins": {
            "a.b": {"enabled": "ja"},
            "c.d": {"enabled": false, "rights": {"net": "denied"}}
        } }"#,
    )
    .unwrap();
    let c = Config::read_from(&p).unwrap();
    assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")));
    assert_eq!(c.language.as_deref(), Some("en"));
    // Falsches `enabled`: der Eintrag bleibt, eingeschaltet (Voreinstellung).
    assert_eq!(c.plugins.get("a.b"), Some(&PluginConfig::default()));
    assert_eq!(
        c.plugins.get("c.d"),
        Some(&PluginConfig {
            enabled: false,
            rights: BTreeMap::from([("net".to_string(), "denied".to_string())]),
        })
    );
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn plugins_bad_right_keeps_enabled() {
    let p = temp_file("recht");
    std::fs::write(
        &p,
        r#"{ "plugins": {
            "a.b": {"enabled": false, "rights": {"net": 5, "model-read": "allowed", "fs-folder": "vielleicht"}},
            "c.d": {"enabled": "ja", "rights": {"net": "denied"}},
            "e.f": {"enabled": false, "rights": 7},
            "g.h": 3
        } }"#,
    )
    .unwrap();
    let c = Config::read_from(&p).unwrap();
    let rights = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(
        c.plugins.get("a.b"),
        Some(&PluginConfig {
            enabled: false,
            rights: rights(&[("model-read", "allowed")]),
        }),
        "ein kaputtes Recht kostet nicht den Schalter"
    );
    assert_eq!(
        c.plugins.get("c.d"),
        Some(&PluginConfig {
            enabled: true,
            rights: rights(&[("net", "denied")]),
        }),
        "ein kaputter Schalter kostet nicht die Rechte"
    );
    assert_eq!(
        c.plugins.get("e.f"),
        Some(&PluginConfig {
            enabled: false,
            rights: BTreeMap::new(),
        })
    );
    assert!(!c.plugins.contains_key("g.h"), "kein Objekt: übergangen");
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}
