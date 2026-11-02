//! Felder `sketch_grid` und `sketch_snap` der Konfiguration (Skizzenraster und Fang): fehlen sie,
//! sind beide an; nur „aus“ wird geschrieben; ein falscher Wert macht die Datei nicht unlesbar.
//! Nur mit eigenen Dateien im Temp-Ordner.

use planora_project::config::Config;
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-grid-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn missing_fields_mean_on() {
    let p = temp_file("fehlt");
    std::fs::write(&p, r#"{ "root": "D:/P" }"#).unwrap();
    let c = Config::load_from(&p);
    assert!(c.sketch_grid);
    assert!(c.sketch_snap);
    assert!(Config::default().sketch_grid);
    assert!(Config::default().sketch_snap);
    cleanup(&p);
}

#[test]
fn off_survives_the_round_trip() {
    let p = temp_file("rundlauf");
    Config::update_at(&p, |c| {
        c.sketch_grid = false;
        c.sketch_snap = false;
    })
    .unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("\"sketch_grid\": false"), "{text}");
    assert!(text.contains("\"sketch_snap\": false"), "{text}");
    let c = Config::read_from(&p).unwrap();
    assert!(!c.sketch_grid);
    assert!(!c.sketch_snap);
    // Einzeln: der andere Schalter bleibt, wo er ist.
    Config::update_at(&p, |c| c.sketch_grid = true).unwrap();
    let c = Config::read_from(&p).unwrap();
    assert!(c.sketch_grid);
    assert!(!c.sketch_snap);
    assert!(!std::fs::read_to_string(&p).unwrap().contains("sketch_grid"));
    cleanup(&p);
}

#[test]
fn a_file_without_a_change_stays_without_the_fields() {
    let p = temp_file("ohne");
    Config::default().save_to(&p).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(!text.contains("sketch_grid"), "{text}");
    assert!(!text.contains("sketch_snap"), "{text}");
    Config::update_at(&p, |c| c.panel_transparency = 25).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(!text.contains("sketch_grid"), "{text}");
    assert!(!text.contains("sketch_snap"), "{text}");
    cleanup(&p);
}

#[test]
fn a_broken_value_gives_the_default_and_keeps_the_file_readable() {
    let p = temp_file("kaputt");
    for bad in ["7", "\"aus\"", "null", "[]", "{}"] {
        std::fs::write(
            &p,
            format!(
                r#"{{ "root": "D:/P", "sketch_grid": {bad}, "sketch_snap": {bad}, "panel_transparency": 25 }}"#
            ),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap();
        assert!(c.sketch_grid, "{bad}");
        assert!(c.sketch_snap, "{bad}");
        assert_eq!(c.panel_transparency, 25, "{bad}");
        assert_eq!(c.root, Some(PathBuf::from("D:/P")), "{bad}");
    }
    cleanup(&p);
}
