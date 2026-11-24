//! Gemerktes Programmfenster (`main_window`): Größe, Lage, maximiert und die Skalierung des
//! Monitors beim Schließen. Fehlt es, gilt die Vorgabe; Unbrauchbares ergibt keines und kostet
//! keine anderen Felder. Nur mit eigenen Dateien im Temp-Ordner.

use planora_project::config::{Config, MainWindow, WindowState};
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "planora-cfg-mainwin-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

fn window() -> MainWindow {
    MainWindow {
        window: WindowState {
            x: Some(120.0),
            y: Some(80.0),
            width: 1400.0,
            height: 900.0,
            maximized: true,
        },
        scale: Some(1.25),
    }
}

#[test]
fn main_window_round_trips_and_keeps_other_fields() {
    let p = temp_file("rund");
    let mut c = Config::default();
    c.verbose_log = true;
    c.main_window = Some(window());
    c.save_to(&p).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("\"main_window\""), "{text}");
    let back = Config::read_from(&p).unwrap();
    assert_eq!(back.main_window, Some(window()));
    assert!(back.verbose_log);
    // Lesen-Ändern-Schreiben anderer Felder lässt das Fenster stehen.
    Config::update_at(&p, |c| c.verbose_log = false).unwrap();
    assert_eq!(Config::read_from(&p).unwrap().main_window, Some(window()));
    cleanup(&p);
}

#[test]
fn old_files_and_defaults_have_no_main_window() {
    assert_eq!(Config::default().main_window, None);
    let p = temp_file("alt");
    std::fs::write(&p, r#"{"update_channel":"dev"}"#).unwrap();
    assert_eq!(Config::read_from(&p).unwrap().main_window, None);
    // Ohne Fenster wird nichts geschrieben.
    Config::default().save_to(&p).unwrap();
    assert!(!std::fs::read_to_string(&p).unwrap().contains("main_window"));
    cleanup(&p);
}

#[test]
fn unusable_main_window_is_dropped_without_losing_other_fields() {
    let p = temp_file("kaputt");
    for bad in [
        r#""Text""#,
        r#"{"x":1,"y":2}"#,
        r#"{"width":-5,"height":600}"#,
        r#"{"width":"breit","height":600}"#,
    ] {
        std::fs::write(
            &p,
            format!(r#"{{"verbose_log":true,"main_window":{bad}}}"#),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap();
        assert_eq!(c.main_window, None, "{bad}");
        assert!(c.verbose_log, "{bad}");
    }
    // Ohne oder mit unbrauchbarer Skalierung bleibt das Fenster, nur ohne Skalierung.
    for scale in ["", r#","scale":"groß""#, r#","scale":0"#, r#","scale":-1"#] {
        std::fs::write(
            &p,
            format!(r#"{{"main_window":{{"x":10,"y":20,"width":1000,"height":700{scale}}}}}"#),
        )
        .unwrap();
        let w = Config::read_from(&p).unwrap().main_window.expect(scale);
        assert_eq!(w.scale, None, "{scale}");
        assert_eq!(w.window.width, 1000.0);
        assert!(!w.window.maximized);
    }
    cleanup(&p);
}
