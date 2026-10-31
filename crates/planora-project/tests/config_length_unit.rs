//! Feld `length_unit` der Konfiguration (Id `mm` oder `in`): fehlt es, gibt es keine Wahl (dann
//! gilt Millimeter); ein falscher Wert macht die Datei nicht unlesbar; geschrieben wird nur eine
//! Wahl. Nur mit eigenen Dateien im Temp-Ordner.

use planora_project::config::Config;
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-unit-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn config_length_unit_is_read_leniently_and_not_written_without_choice() {
    let p = temp_file("alt");
    std::fs::write(&p, r#"{ "root": "D:/P", "panel_transparency": 30 }"#).unwrap();
    let c = Config::read_from(&p).unwrap();
    assert_eq!(c.length_unit, None);
    assert_eq!(c.panel_transparency, 30);
    assert_eq!(Config::default().length_unit, None);
    cleanup(&p);

    let p = temp_file("leer");
    Config::update_at(&p, |c| c.panel_transparency = 10).unwrap();
    assert!(!std::fs::read_to_string(&p).unwrap().contains("length_unit"));
    cleanup(&p);

    for id in ["mm", "in"] {
        let p = temp_file("wahl");
        Config::update_at(&p, |c| c.length_unit = Some(id.to_string())).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(
            text.contains(&format!("\"length_unit\": \"{id}\"")),
            "{text}"
        );
        assert_eq!(
            Config::read_from(&p).unwrap().length_unit.as_deref(),
            Some(id)
        );
        Config::update_at(&p, |c| c.panel_transparency = 5).unwrap();
        assert_eq!(
            Config::read_from(&p).unwrap().length_unit.as_deref(),
            Some(id)
        );
        cleanup(&p);
    }

    for bad in ["42", "null", "\"\"", "\"   \"", "[1]", "{}", "true"] {
        let p = temp_file("kaputt");
        std::fs::write(
            &p,
            format!(r#"{{ "root": "D:/P", "length_unit": {bad}, "panel_transparency": 25 }}"#),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap();
        assert_eq!(c.length_unit, None, "{bad}");
        assert_eq!(c.panel_transparency, 25, "{bad}");
        assert_eq!(c.root, Some(PathBuf::from("D:/P")), "{bad}");
        cleanup(&p);
    }
}
