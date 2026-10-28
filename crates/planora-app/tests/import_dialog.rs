//! Fenster „Netz importieren“: lesen im Arbeitsthread, Bericht, Name, Einheit, Abbrechen,
//! Fehler, volle Breite (Teilprojekt 9, Task 17).

use planora_app::export_dialog::{ExportDialog, ExportStatus};
use planora_app::fonts;
use planora_app::import_dialog::*;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_exchange::{ExchangeMesh, ExportBody, Format, export_file};
use std::fs;
use std::path::{Path, PathBuf};

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
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

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-import-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Schreibt `mesh` als STL nach `dir/name`.
fn write_stl(dir: &Path, name: &str, mesh: ExchangeMesh) -> PathBuf {
    let path = dir.join(name);
    export_file(
        &path,
        Format::Stl,
        &[ExportBody {
            name: "x".into(),
            mesh,
        }],
    )
    .unwrap();
    path
}

fn wide() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    }
}

fn draw(d: &mut ImportDialog, s: &mut AppState) -> String {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut dir = None;
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show_in(
                ui.ctx(),
                s,
                &mut dir,
                &mut planora_app::places::Places::default()
            ));
        });
        text = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

#[test]
fn a_file_is_read_checked_and_imported_with_its_unit() {
    let dir = temp_dir("einheit");
    let path = write_stl(&dir, "Würfel.stl", cube());
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    d.load(path);
    d.tick(true);
    let l = d.loaded.as_ref().unwrap();
    assert_eq!(l.report.triangles, 12);
    assert!(l.report.closed);
    assert_eq!(d.name, "Würfel");
    assert_eq!(d.unit, Unit::Millimeter, "Vorgabe Millimeter");
    d.unit = Unit::Zoll;
    d.import(&mut s).unwrap();
    assert!((s.bodies[0].mass.volume - 25.4f64.powi(3)).abs() < 1e-6);
    let factors: Vec<f64> = Unit::ALL.iter().map(|u| u.factor()).collect();
    assert_eq!(factors, [1.0, 10.0, 1000.0, 25.4]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn broken_files_show_a_named_error_and_change_nothing() {
    let dir = temp_dir("kaputt");
    let p = dir.join("kaputt.obj");
    fs::write(&p, "v 0 0 0\nf 1 2 3\n").unwrap();
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    d.load(p);
    d.tick(true);
    assert!(d.loaded.is_none());
    assert_eq!(
        d.error_text().as_deref(),
        Some("Zeile 2: Die Fläche verweist auf einen Punkt, den es nicht gibt")
    );
    assert!(d.import(&mut s).is_err());
    assert!(s.document.features().is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_drops_the_reading() {
    let dir = temp_dir("abbrechen");
    let path = write_stl(&dir, "w.stl", cube());
    let mut d = ImportDialog::new(dir.clone());
    d.load(path);
    assert!(d.is_loading());
    d.cancel();
    assert!(!d.is_loading());
    assert!(d.loaded.is_none());
    assert_eq!(d.error_text().as_deref(), Some("Abgebrochen"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_open_mesh_is_imported_with_a_warning() {
    let dir = temp_dir("offen");
    let mut open = cube();
    open.indices.truncate(33);
    let path = write_stl(&dir, "offen.stl", open);
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    d.load(path);
    d.tick(true);
    assert!(!d.loaded.as_ref().unwrap().report.closed);
    let text = draw(&mut d, &mut s);
    assert!(text.contains(&open_mesh_hint()), "{text}");
    d.import(&mut s).unwrap();
    assert_eq!(s.bodies[0].kind, BodyKind::Mesh);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_window_is_centred_720_wide_with_equal_cards_and_plain_texts() {
    let dir = temp_dir("fenster");
    let path = write_stl(&dir, "Teil.stl", cube());
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    let text = draw(&mut d, &mut s);
    assert!(
        text.contains("STL oder OBJ") && text.contains("Teil.stl"),
        "{text}"
    );
    d.load(path);
    d.tick(true);
    let text = draw(&mut d, &mut s);
    let rect = d.last_rect.unwrap();
    assert!(
        (rect.width() - 720.0).abs() <= 1.0 && (rect.center().x - 600.0).abs() <= 1.0,
        "720 breit und mittig: {rect:?}"
    );
    assert_eq!(d.last_cards.len(), 2);
    assert!((d.last_cards[0].height() - d.last_cards[1].height()).abs() < 0.5);
    for word in [
        "Netz importieren",
        "Prüfung",
        "12 Dreiecke",
        "Das Netz ist geschlossen.",
        "Name",
        "Einheit der Datei",
        "Millimeter",
        "Zentimeter",
        "Meter",
        "Zoll",
        "Importieren",
    ] {
        assert!(text.contains(word), "„{word}“ fehlt:\n{text}");
    }
    assert!(s.document.features().is_empty(), "Zeichnen ändert nichts");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_imported_mesh_body_can_be_exported_again() {
    let dir = temp_dir("wieder");
    let path = write_stl(&dir, "w.stl", cube());
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    d.load(path);
    d.tick(true);
    d.import(&mut s).unwrap();
    let mut e = ExportDialog::new(&s);
    e.tick(&s, true);
    assert!(matches!(
        e.status(&s),
        ExportStatus::Ready { triangles: 12, .. }
    ));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_result_for_an_older_document_is_never_imported() {
    let dir = temp_dir("veraltet");
    let path = write_stl(&dir, "w.stl", cube());
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    d.load_at(path, s.document.edit_count());
    d.tick(true);
    let mesh = std::sync::Arc::clone(&d.loaded.as_ref().unwrap().mesh);
    // Das Dokument ändert sich, bevor „Importieren“ gedrückt wird.
    s.import_mesh("Anderes", mesh, 1.0).unwrap();
    let before = s.document.features().len();
    assert_eq!(d.import(&mut s), Err(stale().text()));
    assert_eq!(s.document.features().len(), before, "nichts ändert sich");
    // Beim Zeichnen wird das veraltete Ergebnis verworfen und die Meldung gezeigt.
    let text = draw(&mut d, &mut s);
    assert!(d.loaded.is_none());
    assert_eq!(d.error, Some(stale()));
    assert!(text.contains(&stale().text()), "{text}");
    assert_eq!(s.document.features().len(), before);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn choosing_another_file_replaces_the_pending_reading() {
    let dir = temp_dir("zweite");
    let first = write_stl(&dir, "eins.stl", cube());
    let mut open = cube();
    open.indices.truncate(33);
    let second = write_stl(&dir, "zwei.stl", open);
    let mut d = ImportDialog::new(dir.clone());
    d.load(first);
    d.load(second);
    d.tick(true);
    let l = d.loaded.as_ref().unwrap();
    assert_eq!(d.name, "zwei");
    assert!(!l.report.closed, "es zählt nur die zuletzt gewählte Datei");
    // Nach dem Abholen gibt es keinen Auftrag mehr, ein weiteres tick ändert nichts.
    d.tick(true);
    assert!(d.loaded.is_some() && d.error.is_none());
    let _ = fs::remove_dir_all(&dir);
}
