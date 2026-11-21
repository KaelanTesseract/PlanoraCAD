//! Feld `catalog_hint_shown` der Konfiguration (Hinweis auf den Katalog der Zusatzmodule beim
//! ersten Start): fehlt es, ist es aus; ein falscher Wert gilt als aus; geschrieben wird nur
//! „an“. Nur mit eigenen Dateien im Temp-Ordner.

use planora_project::config::Config;
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-hint-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn config_catalog_hint_roundtrip() {
    // Fehlt das Feld, ist es false.
    let p = temp_file("fehlt");
    std::fs::write(&p, r#"{ "panel_transparency": 30 }"#).unwrap();
    let c = Config::read_from(&p).unwrap();
    assert!(!c.catalog_hint_shown);
    assert_eq!(c.panel_transparency, 30);
    assert!(!Config::default().catalog_hint_shown);
    cleanup(&p);

    // false wird nicht geschrieben.
    let p = temp_file("aus");
    Config::default().save_to(&p).unwrap();
    assert!(
        !std::fs::read_to_string(&p)
            .unwrap()
            .contains("catalog_hint_shown")
    );
    cleanup(&p);

    // true wird geschrieben und wieder gelesen.
    let p = temp_file("an");
    let c = Config {
        catalog_hint_shown: true,
        ..Config::default()
    };
    c.save_to(&p).unwrap();
    assert!(
        std::fs::read_to_string(&p)
            .unwrap()
            .contains("\"catalog_hint_shown\": true")
    );
    assert!(Config::read_from(&p).unwrap().catalog_hint_shown);
    cleanup(&p);

    // Kein Wahrheitswert: false, die Datei bleibt lesbar.
    for bad in ["42", "null", "\"ja\"", "\"true\"", "[true]", "{}"] {
        let p = temp_file("kaputt");
        std::fs::write(
            &p,
            format!(r#"{{ "catalog_hint_shown": {bad}, "panel_transparency": 25 }}"#),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap();
        assert!(!c.catalog_hint_shown, "{bad}");
        assert_eq!(c.panel_transparency, 25, "{bad}");
        cleanup(&p);
    }
}
