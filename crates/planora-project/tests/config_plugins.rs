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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
        }),
        "ein kaputtes Recht kostet nicht den Schalter"
    );
    assert_eq!(
        c.plugins.get("c.d"),
        Some(&PluginConfig {
            enabled: true,
            rights: rights(&[("net", "denied")]),
            ..Default::default()
        }),
        "ein kaputter Schalter kostet nicht die Rechte"
    );
    assert_eq!(
        c.plugins.get("e.f"),
        Some(&PluginConfig {
            enabled: false,
            rights: BTreeMap::new(),
            ..Default::default()
        })
    );
    assert!(!c.plugins.contains_key("g.h"), "kein Objekt: übergangen");
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn plugins_author_and_hosts() {
    let p = temp_file("autor");
    let mut c = Config::default();
    c.plugins.insert(
        "a.b".to_string(),
        PluginConfig {
            enabled: true,
            rights: BTreeMap::from([("net".to_string(), "allowed".to_string())]),
            author: Some("Dennis Guse".to_string()),
            net_hosts: vec!["example.org".to_string()],
        },
    );
    c.plugins.insert(
        "c.d".to_string(),
        PluginConfig {
            enabled: false,
            ..Default::default()
        },
    );
    c.save_to(&p).unwrap();
    assert_eq!(Config::read_from(&p).unwrap(), c);
    let text = std::fs::read_to_string(&p).unwrap();
    // Leer wird nichts geschrieben: nur einmal `author`, einmal `net_hosts`.
    assert_eq!(text.matches("author").count(), 1, "{text}");
    assert_eq!(text.matches("net_hosts").count(), 1, "{text}");

    std::fs::write(
        &p,
        r#"{ "plugins": {
            "a.b": {"rights": {"net": "allowed"}, "author": 5, "net_hosts": "example.org"},
            "c.d": {"rights": {"net": "allowed"}, "author": "", "net_hosts": ["x.org", 3]},
            "e.f": {"rights": {"net": "allowed"}, "author": "A", "net_hosts": ["y.org"]}
        } }"#,
    )
    .unwrap();
    let c = Config::read_from(&p).unwrap();
    let net = BTreeMap::from([("net".to_string(), "allowed".to_string())]);
    assert_eq!(
        c.plugins["a.b"],
        PluginConfig {
            rights: net.clone(),
            ..Default::default()
        },
        "falsche Typen: unbekannt, die Rechte bleiben (der Host fragt neu)"
    );
    assert_eq!(
        c.plugins["c.d"],
        PluginConfig {
            rights: net.clone(),
            author: None,
            net_hosts: vec!["x.org".to_string()],
            ..Default::default()
        }
    );
    assert_eq!(
        c.plugins["e.f"],
        PluginConfig {
            rights: net,
            author: Some("A".to_string()),
            net_hosts: vec!["y.org".to_string()],
            ..Default::default()
        }
    );
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn last_plugin_check_lenient() {
    let p = temp_file("pruefung");
    let c = Config::default();
    assert_eq!(c.last_plugin_check, None);
    c.save_to(&p).unwrap();
    assert!(
        !std::fs::read_to_string(&p)
            .unwrap()
            .contains("last_plugin_check")
    );
    let c = Config {
        last_plugin_check: Some(1_800_000_000),
        ..Config::default()
    };
    c.save_to(&p).unwrap();
    assert_eq!(Config::read_from(&p).unwrap(), c);
    for (text, expected) in [
        (
            r#"{"last_plugin_check": "gestern", "language": "en"}"#,
            None,
        ),
        (r#"{"last_plugin_check": -5, "language": "en"}"#, None),
        (r#"{"last_plugin_check": 42, "language": "en"}"#, Some(42)),
    ] {
        std::fs::write(&p, text).unwrap();
        let c = Config::read_from(&p).unwrap();
        assert_eq!(c.last_plugin_check, expected, "{text}");
        assert_eq!(c.language.as_deref(), Some("en"));
    }
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}
