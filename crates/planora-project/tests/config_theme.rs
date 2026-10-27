//! Feld `theme` der Konfiguration („hell“ / „dunkel“): fehlt es, gilt das dunkle Design; ein
//! falscher Wert macht die Datei nicht unlesbar; geschrieben wird nur eine Wahl, und andere
//! Felder bleiben beim Lesen-Ändern-Schreiben erhalten. Nur mit eigenen Dateien im Temp-Ordner.

use planora_project::config::{Config, THEME_DARK, THEME_LIGHT};
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-theme-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn old_files_without_the_field_are_dark_and_keep_their_other_fields() {
    let p = temp_file("alt");
    std::fs::write(&p, r#"{ "root": "D:/P", "panel_transparency": 30 }"#).unwrap();
    let c = Config::read_from(&p).unwrap();
    assert_eq!(c.theme, None);
    assert!(c.dark());
    assert_eq!(c.panel_transparency, 30);
    assert_eq!(c.root, Some(PathBuf::from("D:/P")));
    assert!(Config::default().dark());
    cleanup(&p);
}

#[test]
fn the_choice_is_written_and_read_back() {
    let p = temp_file("roundtrip");
    Config::update_at(&p, |c| c.theme = Some(THEME_LIGHT.to_string())).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("\"theme\""), "{text}");
    let c = Config::read_from(&p).unwrap();
    assert_eq!(c.theme.as_deref(), Some("hell"));
    assert!(!c.dark());
    Config::update_at(&p, |c| c.theme = Some(THEME_DARK.to_string())).unwrap();
    assert!(Config::read_from(&p).unwrap().dark());
    cleanup(&p);
}

#[test]
fn no_choice_is_not_written() {
    let p = temp_file("leer");
    Config::update_at(&p, |c| c.panel_transparency = 10).unwrap();
    assert!(!std::fs::read_to_string(&p).unwrap().contains("theme"));
    cleanup(&p);
}

#[test]
fn wrong_values_are_ignored_without_losing_other_fields() {
    for bad in ["42", "null", "\"blau\"", "\"\"", "[1]", "\"Hell\""] {
        let p = temp_file("kaputt");
        std::fs::write(
            &p,
            format!(r#"{{ "root": "D:/P", "theme": {bad}, "panel_transparency": 25 }}"#),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap();
        assert_eq!(c.theme, None, "{bad}");
        assert!(c.dark());
        assert_eq!(c.panel_transparency, 25, "{bad}");
        assert_eq!(c.root, Some(PathBuf::from("D:/P")), "{bad}");
        cleanup(&p);
    }
}

#[test]
fn other_changes_keep_the_choice() {
    let p = temp_file("behalten");
    Config::update_at(&p, |c| c.theme = Some(THEME_LIGHT.to_string())).unwrap();
    Config::update_at(&p, |c| c.panel_transparency = 5).unwrap();
    let c = Config::read_from(&p).unwrap();
    assert!(!c.dark());
    assert_eq!(c.panel_transparency, 5);
    cleanup(&p);
}
