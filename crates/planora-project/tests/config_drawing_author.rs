//! Feld `drawing_author` der Konfiguration (Einstellungen → Projekte → „Name für Zeichnungen“):
//! fehlt es, gibt es keinen Namen; ein falscher oder leerer Wert gilt als keiner und macht die
//! Datei nicht unlesbar; geschrieben wird nur ein Name. Nur mit eigenen Dateien im Temp-Ordner.

use planora_project::config::Config;
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-author-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn missing_or_bad_field_is_no_name() {
    assert_eq!(Config::default().drawing_author, None);
    for bad in [
        "",
        r#", "drawing_author": 42"#,
        r#", "drawing_author": """#,
        r#", "drawing_author": null"#,
    ] {
        let p = temp_file("kaputt");
        std::fs::write(
            &p,
            format!(r#"{{ "root": "D:/P", "panel_transparency": 25{bad} }}"#),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap();
        assert_eq!(c.drawing_author, None, "{bad}");
        assert_eq!(c.panel_transparency, 25, "{bad}");
        cleanup(&p);
    }
}

#[test]
fn only_a_name_is_written() {
    let p = temp_file("schreiben");
    Config::update_at(&p, |c| c.drawing_author = None).unwrap();
    assert!(
        !std::fs::read_to_string(&p)
            .unwrap()
            .contains("drawing_author")
    );
    Config::update_at(&p, |c| c.drawing_author = Some("Erika Muster".into())).unwrap();
    assert_eq!(
        Config::read_from(&p).unwrap().drawing_author.as_deref(),
        Some("Erika Muster")
    );
    cleanup(&p);
}
