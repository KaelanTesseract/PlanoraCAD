//! Feld `verbose_log` der Konfiguration (Einstellungen → Updates → „Ausführliches Protokoll“):
//! fehlt es, ist es aus; ein falscher Wert gilt als aus und macht die Datei nicht unlesbar;
//! geschrieben wird nur „an“. Nur mit eigenen Dateien im Temp-Ordner.

use planora_project::config::Config;
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-verbose-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn missing_field_is_off() {
    let p = temp_file("alt");
    std::fs::write(&p, r#"{ "root": "D:/P", "panel_transparency": 30 }"#).unwrap();
    let c = Config::read_from(&p).unwrap();
    assert!(!c.verbose_log);
    assert_eq!(c.panel_transparency, 30);
    assert!(!Config::default().verbose_log);
    cleanup(&p);
}

#[test]
fn non_bool_reads_as_off_and_keeps_other_fields() {
    for bad in ["42", "null", "\"ja\"", "\"true\"", "[true]", "{}"] {
        let p = temp_file("kaputt");
        std::fs::write(
            &p,
            format!(r#"{{ "root": "D:/P", "verbose_log": {bad}, "panel_transparency": 25 }}"#),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap();
        assert!(!c.verbose_log, "{bad}");
        assert_eq!(c.panel_transparency, 25, "{bad}");
        assert_eq!(c.root, Some(PathBuf::from("D:/P")), "{bad}");
        cleanup(&p);
    }
    let p = temp_file("an");
    std::fs::write(&p, r#"{ "verbose_log": true }"#).unwrap();
    assert!(Config::read_from(&p).unwrap().verbose_log);
    cleanup(&p);
}

#[test]
fn only_on_is_written() {
    let p = temp_file("schreiben");
    Config::update_at(&p, |c| c.verbose_log = false).unwrap();
    assert!(!std::fs::read_to_string(&p).unwrap().contains("verbose_log"));
    Config::update_at(&p, |c| c.verbose_log = true).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("\"verbose_log\": true"), "{text}");
    assert!(Config::read_from(&p).unwrap().verbose_log);
    Config::update_at(&p, |c| c.verbose_log = false).unwrap();
    assert!(!std::fs::read_to_string(&p).unwrap().contains("verbose_log"));
    cleanup(&p);
}
