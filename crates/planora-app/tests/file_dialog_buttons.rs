//! Knöpfe der Dateifenster: Während die Dateiauswahl gezeigt wird, gibt es nur
//! „Öffnen“/„Speichern“ und „Abbrechen“, kein zusätzliches „Schließen“. „Abbrechen“ schließt das
//! Fenster, wenn es nichts gibt, wohin es zurückgehen kann (Netz und STEP importieren), beim
//! Exportieren geht es zurück zum Exportformular.

mod common;

use common::{Screen, Seen, find};
use planora_app::export_dialog::ExportDialog;
use planora_app::import_dialog::ImportDialog;
use planora_app::places::Places;
use planora_app::state::AppState;
use planora_app::step_import_dialog::StepImportDialog;
use planora_exchange::{ExchangeMesh, ExportBody, Format, export_file};
use planora_math::DVec3;
use std::fs;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-knoepfe-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn in_rect<'a>(seen: &'a [Seen], rect: egui::Rect, text: &str) -> Option<&'a Seen> {
    seen.iter()
        .find(|s| s.text == text && rect.contains_rect(s.rect) && s.clip.intersects(s.rect))
}

fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

#[test]
fn mesh_import_has_only_open_and_cancel_and_cancel_closes() {
    let dir = temp_dir("netz");
    let path = dir.join("teil.stl");
    export_file(
        &path,
        Format::Stl,
        &[ExportBody {
            name: "x".into(),
            mesh: cube(),
        }],
    )
    .unwrap();
    for loaded in [false, true] {
        let mut d = ImportDialog::new(dir.clone());
        if loaded {
            d.load(path.clone());
            d.tick(true);
            assert!(d.loaded.is_some());
        }
        let mut s = AppState::new();
        let (mut last, mut places) = (None, Places::default());
        let mut open = true;
        let mut screen = Screen::new(1100.0, 700.0);
        let mut f = |ctx: &egui::Context| {
            if open {
                open = d.show_in(ctx, &mut s, &mut last, &mut places);
            }
        };
        let seen = screen.settle(6, &mut f);
        drop(f);
        let rect = d.last_rect.unwrap();
        let what = format!("Netz importieren, gelesen: {loaded}");
        assert!(in_rect(&seen, rect, "Öffnen").is_some(), "{what}");
        assert!(
            in_rect(&seen, rect, "Schließen").is_none(),
            "{what}: kein „Schließen“"
        );
        assert_eq!(
            in_rect(&seen, rect, "Importieren").is_some(),
            loaded,
            "{what}: „Importieren“ nur mit gelesener Datei"
        );
        let cancel = in_rect(&seen, rect, "Abbrechen").expect("Abbrechen").rect;
        let mut f = |ctx: &egui::Context| {
            if open {
                open = d.show_in(ctx, &mut s, &mut last, &mut places);
            }
        };
        screen.click(cancel.center(), &mut f);
        drop(f);
        assert!(!open, "{what}: „Abbrechen“ schließt das Fenster");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn step_import_without_a_list_has_no_close_button_and_cancel_closes() {
    let dir = temp_dir("step");
    let mut d = StepImportDialog::new(dir.clone());
    let mut s = AppState::new();
    let (mut last, mut places) = (None, Places::default());
    let mut open = true;
    let mut screen = Screen::new(1100.0, 700.0);
    let mut f = |ctx: &egui::Context| {
        if open {
            open = d.show_in(ctx, &mut s, &mut last, &mut places);
        }
    };
    let seen = screen.settle(6, &mut f);
    drop(f);
    let rect = d.last_rect.unwrap();
    assert!(in_rect(&seen, rect, "Öffnen").is_some());
    assert!(
        in_rect(&seen, rect, "Schließen").is_none(),
        "kein „Schließen“"
    );
    let cancel = in_rect(&seen, rect, "Abbrechen").expect("Abbrechen").rect;
    let mut f = |ctx: &egui::Context| {
        if open {
            open = d.show_in(ctx, &mut s, &mut last, &mut places);
        }
    };
    screen.click(cancel.center(), &mut f);
    drop(f);
    assert!(!open, "„Abbrechen“ schließt das Fenster");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn export_chooser_has_only_save_and_cancel_and_cancel_returns_to_the_form() {
    let dir = temp_dir("export");
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    d.begin_save(dir.clone(), &s);
    let (mut last, mut places) = (None, Places::default());
    let mut open = true;
    let mut screen = Screen::new(1100.0, 700.0);
    let mut f = |ctx: &egui::Context| {
        if open {
            open = d.show_in(ctx, &mut s, &mut last, &mut places);
        }
    };
    let seen = screen.settle(6, &mut f);
    drop(f);
    let rect = d.last_rect.unwrap();
    assert!(in_rect(&seen, rect, "Speichern").is_some());
    assert!(
        in_rect(&seen, rect, "Schließen").is_none(),
        "kein „Schließen“"
    );
    assert!(find(&seen, "Exportieren …").is_none());
    let cancel = in_rect(&seen, rect, "Abbrechen").expect("Abbrechen").rect;
    let mut f = |ctx: &egui::Context| {
        if open {
            open = d.show_in(ctx, &mut s, &mut last, &mut places);
        }
    };
    screen.click(cancel.center(), &mut f);
    let seen = screen.settle(3, &mut f);
    drop(f);
    assert!(open, "zurück zum Exportformular");
    assert!(d.chooser().is_none(), "Dateiwahl zu");
    // Im Formular (ohne Dateiwahl) gibt es wieder „Schließen“.
    assert!(find(&seen, "Exportieren …").is_some());
    assert!(find(&seen, "Schließen").is_some());
    let _ = fs::remove_dir_all(&dir);
}
