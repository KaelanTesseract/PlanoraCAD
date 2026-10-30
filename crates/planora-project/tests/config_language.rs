//! Feld `language` der Konfiguration (Sprach-Id wie `en`): fehlt es, gibt es keine Wahl (dann
//! gilt beim Start die Sprache von Windows); ein falscher Wert macht die Datei nicht unlesbar;
//! geschrieben wird nur eine Wahl. Nur mit eigenen Dateien im Temp-Ordner.

use planora_project::config::Config;
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-lang-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn config_language_is_read_leniently_and_not_written_without_choice() {
    // Ältere Datei ohne das Feld: keine Wahl, andere Felder bleiben.
    let p = temp_file("alt");
    std::fs::write(&p, r#"{ "root": "D:/P", "panel_transparency": 30 }"#).unwrap();
    let c = Config::read_from(&p).unwrap();
    assert_eq!(c.language, None);
    assert_eq!(c.panel_transparency, 30);
    assert_eq!(Config::default().language, None);
    cleanup(&p);

    // Ohne Wahl wird nichts geschrieben.
    let p = temp_file("leer");
    Config::update_at(&p, |c| c.panel_transparency = 10).unwrap();
    assert!(!std::fs::read_to_string(&p).unwrap().contains("language"));
    cleanup(&p);

    // Eine Wahl wird geschrieben und wieder gelesen, auch eine nicht eingebaute Sprache.
    for id in ["en", "de", "pt-BR"] {
        let p = temp_file("wahl");
        Config::update_at(&p, |c| c.language = Some(id.to_string())).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains(&format!("\"language\": \"{id}\"")), "{text}");
        assert_eq!(Config::read_from(&p).unwrap().language.as_deref(), Some(id));
        // Andere Änderungen behalten die Wahl.
        Config::update_at(&p, |c| c.panel_transparency = 5).unwrap();
        assert_eq!(Config::read_from(&p).unwrap().language.as_deref(), Some(id));
        cleanup(&p);
    }

    // Falsche Werte: keine Wahl, die Datei bleibt lesbar.
    for bad in ["42", "null", "\"\"", "\"   \"", "[1]", "{}", "true"] {
        let p = temp_file("kaputt");
        std::fs::write(
            &p,
            format!(r#"{{ "root": "D:/P", "language": {bad}, "panel_transparency": 25 }}"#),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap();
        assert_eq!(c.language, None, "{bad}");
        assert_eq!(c.panel_transparency, 25, "{bad}");
        assert_eq!(c.root, Some(PathBuf::from("D:/P")), "{bad}");
        cleanup(&p);
    }
}
