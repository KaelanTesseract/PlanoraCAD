//! Fenster „Exportieren“: Umfang, Qualität, Schätzung, Bericht, Schreiben, Abbrechen, volle
//! Breite und gleich hohe Karten (Teilprojekt 9, Task 16).

use planora_app::export_dialog::*;
use planora_app::fonts;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_app::workspace::Workspace;
use planora_exchange::{ExchangeMesh, Format, Progress, import_file};
use planora_math::DVec3;
use planora_project::config::Config;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

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
    let d = std::env::temp_dir().join(format!("planora-export-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
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

/// Zeichnet das Fenster dreimal und liefert die Texte des letzten Bildes.
fn draw(d: &mut ExportDialog, s: &mut AppState) -> String {
    let ctx = context();
    let mut dir = None;
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show(ui.ctx(), s, &mut dir));
        });
        text = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

fn box_state() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s
}

#[test]
fn the_scope_follows_the_selection_and_suppressed_features_do_not_count() {
    let mut s = box_state();
    s.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap(); // gewählt
    let d = ExportDialog::new(&s);
    assert_eq!(d.scope, Scope::Selected);
    let items = d.items(&s);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].source, ExportSource::Mesh(..)));
    s.selected_body = None;
    let d = ExportDialog::new(&s);
    assert_eq!(d.scope, Scope::Visible);
    assert_eq!(d.items(&s).len(), 2);
    let q = s.document.features()[0].id;
    s.document.set_suppressed(q, true).unwrap();
    s.refresh();
    assert_eq!(ExportDialog::new(&s).items(&s).len(), 1);
}

#[test]
fn preparing_shows_triangles_estimate_and_report() {
    let s = box_state();
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    match d.status(&s) {
        ExportStatus::Ready {
            triangles,
            bytes,
            report,
        } => {
            assert_eq!(triangles, 12);
            assert_eq!(bytes, 84 + 50 * 12);
            assert!(report.closed);
        }
        other => panic!("{other:?}"),
    }
    // Mechanisch angepasst (Teilprojekt 10, Task 13): Formate des Fensters als `ExportFormat`.
    d.format = ExportFormat::Mesh(Format::Obj);
    assert!(
        matches!(d.status(&s), ExportStatus::Ready { .. }),
        "das Format ändert das Netz nicht"
    );
    d.quality = Quality::Fine;
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::Ready { .. }));
    let tolerances: Vec<f64> = Quality::ALL.iter().map(|q| q.chord_tolerance()).collect();
    assert_eq!(tolerances, [0.1, 0.01, 0.001]);
}

#[test]
fn finer_quality_gives_more_triangles_on_curved_bodies() {
    let mut s = AppState::new();
    s.create_sphere(10.0).unwrap();
    let mut d = ExportDialog::new(&s);
    let mut counts = Vec::new();
    for q in Quality::ALL {
        d.quality = q;
        d.tick(&s, true);
        if let ExportStatus::Ready { triangles, .. } = d.status(&s) {
            counts.push(triangles);
        }
    }
    assert_eq!(counts.len(), 3);
    assert!(counts[0] < counts[1] && counts[1] < counts[2], "{counts:?}");
}

#[test]
fn writing_goes_to_the_chosen_file_and_names_errors() {
    let s = box_state();
    let dir = temp_dir("schreiben");
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    d.begin_save(dir.clone(), &s);
    assert_eq!(
        d.chooser().unwrap().target(),
        Some(dir.join("Quader 1.stl"))
    );
    let path = dir.join("Quader 1.stl");
    d.write_to(path.clone(), &s);
    d.tick(&s, true);
    assert_eq!(
        d.write_state(),
        WriteState::Done(path.clone(), 84 + 50 * 12)
    );
    assert_eq!(
        import_file(&path, &Progress::new())
            .unwrap()
            .triangle_count(),
        12
    );
    d.write_to(dir.join("fehlt").join("x.stl"), &s);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Failed(_)));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_stops_the_computation_and_says_so() {
    let mut s = AppState::new();
    s.create_sphere(10.0).unwrap();
    let mut d = ExportDialog::new(&s);
    d.quality = Quality::Fine;
    d.tick(&s, false);
    d.cancel();
    assert_eq!(d.status(&s), ExportStatus::Failed("Abgebrochen".into()));
    d.recompute();
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::Ready { .. }));
}

#[test]
fn prepare_reports_the_bodies_one_by_one() {
    let s = box_state();
    let items = ExportDialog::new(&s).items(&s);
    let p = Progress::new();
    let prepared = prepare(&items, Quality::Normal, &p).unwrap();
    assert_eq!((prepared.triangles(), prepared.vertices()), (12, 8));
    assert_eq!(p.fraction(), 1.0);
    let cancelled = Progress::new();
    cancelled.cancel();
    assert_eq!(
        prepare(&items, Quality::Normal, &cancelled).unwrap_err(),
        "Abgebrochen"
    );
}

#[test]
fn the_window_spans_the_full_width_with_equal_cards_and_plain_texts() {
    let mut s = box_state();
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    let text = draw(&mut d, &mut s);
    let rect = d.last_rect.unwrap();
    assert!(
        rect.width() >= 1200.0 - 2.0 * DIALOG_MARGIN - 1.0 && rect.width() <= 1200.0,
        "{rect:?}"
    );
    assert_eq!(d.last_cards.len(), 3);
    assert!(
        d.last_cards
            .windows(2)
            .all(|w| (w[0].height() - w[1].height()).abs() < 0.5),
        "{:?}",
        d.last_cards
    );
    for word in [
        "Exportieren",
        "Format",
        "Qualität",
        "Umfang",
        "STL",
        "3MF",
        "glTF",
        "Grob",
        "Normal",
        "Fein",
        "12 Dreiecke",
        "unter 0,1 MB",
        "Das Netz ist geschlossen.",
        "Exportieren …",
    ] {
        assert!(text.contains(word), "„{word}“ fehlt:\n{text}");
    }
    assert_eq!(s.document.features().len(), 1, "Zeichnen ändert nichts");
}

#[test]
fn the_window_warns_about_open_meshes_and_still_exports() {
    let mut s = AppState::new();
    let mut open = cube();
    open.indices.truncate(33);
    s.import_mesh("Offen", Arc::new(open), 1.0).unwrap();
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    let text = draw(&mut d, &mut s);
    assert!(text.contains("nicht ganz geschlossen"), "{text}");
    assert!(text.contains(NOT_CLOSED_WARNING), "{text}");
    let dir = temp_dir("offen");
    let path = dir.join("offen.stl");
    d.write_to(path.clone(), &s);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Done(..)));
    assert_eq!(
        import_file(&path, &Progress::new())
            .unwrap()
            .triangle_count(),
        11
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn texts_for_numbers_and_sizes_are_german() {
    assert_eq!(group_digits(12), "12");
    assert_eq!(group_digits(12_345), "12 345");
    assert_eq!(group_digits(1_234_567), "1 234 567");
    assert_eq!(mb_text(50_000), "unter 0,1 MB");
    assert_eq!(mb_text(1_260_000), "1,3 MB");
    assert_eq!(estimate_text(1_260_000), "ca. 1,3 MB");
    assert_eq!(estimate_text(684), "unter 0,1 MB");
    assert!(format_label(Format::Stl).starts_with("STL"));
    let ws = Workspace::new(None, Config::default());
    assert!(ws.exchange_dir.is_none());
    assert!(ws.active_tab().export_dialog.is_none());
}

#[test]
fn a_stale_result_is_never_written_and_the_chooser_closes() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_sphere(10.0).unwrap();
    let dir = temp_dir("veraltet");
    s.selected_body = Some(0);
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    d.begin_save(dir.clone(), &s);
    s.selected_body = Some(1);
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    let path = dir.join("Quader 1.stl");
    d.write_to(path.clone(), &s);
    d.tick(&s, true);
    assert_eq!(d.write_state(), WriteState::Failed(STALE.into()));
    assert!(!path.exists(), "es darf nichts geschrieben werden");
    // Die offene Dateiauswahl schließt, sobald sich die Auswahl ändert.
    draw(&mut d, &mut s);
    assert!(d.chooser().is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_done_line_disappears_when_the_result_no_longer_fits() {
    let mut s = box_state();
    let dir = temp_dir("fertig-weg");
    let mut d = ExportDialog::new(&s);
    d.tick(&s, true);
    d.begin_save(dir.clone(), &s);
    d.write_to(dir.join("Quader 1.stl"), &s);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Done(..)));
    // gleicher Stand: die Zeile bleibt
    draw(&mut d, &mut s);
    assert!(matches!(d.write_state(), WriteState::Done(..)));
    // andere Güte bei offener Dateiauswahl: die Zeile gehört zum alten Stand
    d.quality = Quality::Fine;
    let text = draw(&mut d, &mut s);
    assert_eq!(d.write_state(), WriteState::Idle);
    assert!(!text.contains("Fertig:"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn writing_without_a_result_says_so_and_scope_falls_back_without_selection() {
    let mut s = box_state();
    s.selected_body = Some(0);
    let mut d = ExportDialog::new(&s);
    assert_eq!(d.scope, Scope::Selected);
    d.write_to(temp_dir("leer").join("x.stl"), &s);
    assert_eq!(d.write_state(), WriteState::Failed(NOT_COMPUTED.into()));
    s.selected_body = None;
    d.tick(&s, true);
    assert_eq!(d.scope, Scope::Visible);
    assert_eq!(d.items(&s).len(), 1);
}
