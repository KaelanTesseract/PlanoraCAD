//! Breite Dialoge mit Karten (Etappe D, Aufgabe 2): Exportieren, Netz importieren, Neue Zeichnung
//! im neuen Dialog-Rahmen: 720 px breit, mittig, Karten nebeneinander auch bei 800 × 600, Dialog
//! scrollt und bleibt im Fenster. (Maß bearbeiten: `dimension_dialog.rs`.)

mod common;

use common::{Screen, assert_cards_inside, find};
use egui::{Rect, pos2};
use planora_app::drawing_dialog::NewDrawingDialog;
use planora_app::export_dialog::*;
use planora_app::import_dialog::ImportDialog;
use planora_app::state::*;
use planora_drawing::{DrawingFormat, Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_exchange::{ExchangeMesh, ExportBody, Format, export_file};
use planora_math::DVec3;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

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
    let d = std::env::temp_dir().join(format!("planora-wide-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Quader und ein Netz-Körper (damit bei STEP die Hinweise erscheinen).
fn export_state() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    s.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap();
    s.selected_body = None;
    s
}

/// Wie `export_state`, dazu eine Zeichnung mit Vorderansicht.
fn drawing_state() -> AppState {
    let mut s = export_state();
    let d = s
        .document
        .add_drawing(
            "Blatt A",
            Sheet {
                paper: Paper::A3,
                landscape: true,
            },
            Projection::First,
        )
        .unwrap();
    s.document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 120.0, y: 200.0 },
            None,
        )
        .unwrap();
    s
}

fn export_dialog(s: &AppState, format: ExportFormat) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    d.format = format;
    d.tick(s, true);
    d
}

fn show_export<'a>(
    d: &'a mut ExportDialog,
    s: &'a mut AppState,
) -> impl FnMut(&egui::Context) + 'a {
    let mut dir = None;
    move |ctx| {
        assert!(d.show(ctx, s, &mut dir));
    }
}

fn window(screen: &Screen) -> Rect {
    Rect::from_min_size(egui::Pos2::ZERO, screen.size)
}

fn check_export(format: ExportFormat, state: fn() -> AppState) {
    let mut s = state();
    let mut d = export_dialog(&s, format);
    let mut screen = Screen::new(800.0, 600.0);
    let seen = screen.settle(8, &mut show_export(&mut d, &mut s));
    let rect = d.last_rect.expect("Dialog gezeichnet");
    assert!(window(&screen).contains_rect(rect), "{rect:?}");
    assert!(rect.width() <= 720.0 + 1.0, "{rect:?}");
    assert_cards_inside(rect, &d.last_cards, &seen);
    // Der Knopf „Exportieren …“ liegt ohne Dateiwahl sichtbar im Fenster und lässt sich klicken.
    let button = find(&seen, "Exportieren …").expect("Knopf „Exportieren …“");
    assert!(
        button.clip.contains_rect(button.rect) && rect.contains_rect(button.rect),
        "{:?} in {:?}",
        button.rect,
        button.clip
    );
    let mut f = show_export(&mut d, &mut s);
    screen.click(button.rect.center(), &mut f);
    drop(f);
    assert!(
        d.chooser().is_some(),
        "Klick öffnet die Dateiwahl ({format:?})"
    );
}

#[test]
fn cards_stay_side_by_side_and_inside_at_800x600() {
    // Exportieren: Netzformat, STEP und Zeichnungsformat.
    check_export(ExportFormat::Mesh(Format::Stl), export_state);
    check_export(ExportFormat::Step, export_state);
    check_export(ExportFormat::Drawing(DrawingFormat::Svg), drawing_state);

    // Netz importieren (Karten „Prüfung“ und „Name“).
    let dir = temp_dir("karten");
    let path = dir.join("Teil.stl");
    export_file(
        &path,
        Format::Stl,
        &[ExportBody {
            name: "x".into(),
            mesh: cube(),
        }],
    )
    .unwrap();
    let mut s = AppState::new();
    let mut d = ImportDialog::new(dir.clone());
    d.load(path);
    d.tick(true);
    let mut screen = Screen::new(800.0, 600.0);
    let mut last = None;
    let seen = screen.settle(8, &mut |ctx| {
        assert!(d.show(ctx, &mut s, &mut last));
    });
    let rect = d.last_rect.unwrap();
    assert!(window(&screen).contains_rect(rect), "{rect:?}");
    assert_cards_inside(rect, &d.last_cards, &seen);
    let _ = fs::remove_dir_all(&dir);

    // Neue Zeichnung.
    let mut s = AppState::new();
    s.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    let mut d = NewDrawingDialog::new(&s);
    let mut screen = Screen::new(800.0, 600.0);
    let seen = screen.settle(8, &mut |ctx| {
        assert!(d.show(ctx, &mut s));
    });
    let rect = d.last_rect.unwrap();
    assert!(window(&screen).contains_rect(rect), "{rect:?}");
    assert_cards_inside(rect, &d.last_cards, &seen);
}

/// Seit der Abschluss-Nachbesserung ist ein Dialog so hoch wie sein Inhalt (vorher fest 420 px
/// mit Rollen): Exportieren mit Dateiwahl passt bei 800 × 600 ganz ins Fenster, „Speichern“ ist
/// ohne Blättern sichtbar und schreibt die Datei. Das Rollen zu hoher Dialoge prüft
/// `dialog_frame::tall_content_scrolls_and_stays_inside`.
#[test]
fn export_dialog_with_chooser_fits_at_800x600() {
    let dir = temp_dir("scroll");
    let mut s = export_state();
    let mut d = export_dialog(&s, ExportFormat::Mesh(Format::Stl));
    assert!(matches!(d.status(&s), ExportStatus::Ready { .. }));
    d.begin_save(dir.clone(), &s);
    assert!(d.chooser().is_some(), "Dateiwahl offen");
    let mut screen = Screen::new(800.0, 600.0);
    let seen = screen.settle(8, &mut show_export(&mut d, &mut s));
    let rect = d.last_rect.unwrap();
    assert!(
        rect.height() < 568.0 - 1.0,
        "füllt die ganze Höhe: {rect:?}"
    );
    assert!(window(&screen).contains_rect(rect), "{rect:?}");
    assert!(
        find(&seen, "Speichern unter").is_some(),
        "Dateiwahl ist nicht gezeichnet"
    );
    let button = find(&seen, "Speichern").expect("„Speichern“ ohne Blättern");
    let mut f = show_export(&mut d, &mut s);
    assert!(
        button.clip.contains_rect(button.rect) && rect.contains_rect(button.rect),
        "{:?} in {:?}",
        button.rect,
        button.clip
    );
    screen.click(button.rect.center(), &mut f);
    drop(f);
    for _ in 0..200 {
        if !matches!(d.write_state(), WriteState::Idle | WriteState::Writing) {
            break;
        }
        d.tick(&s, true);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let file = dir.join("Körper.stl");
    assert!(
        matches!(d.write_state(), WriteState::Done(p, _) if *p == file),
        "Klick auf „Speichern“ schreibt {file:?}: {:?}",
        d.write_state()
    );
    assert!(file.exists(), "Datei geschrieben");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn growing_status_keeps_the_dialog_in_place() {
    let dir = temp_dir("wachsen");
    let mut s = export_state();
    let mut d = ExportDialog::new(&s);
    let mut screen = Screen::new(1200.0, 800.0);
    let mut centres = Vec::new();
    let mut step = |d: &mut ExportDialog, s: &mut AppState, screen: &mut Screen| {
        screen.settle(6, &mut show_export(d, s));
        let rect = d.last_rect.unwrap();
        assert!(window(screen).contains_rect(rect), "{rect:?}");
        centres.push(rect.center());
    };
    // Berechnen.
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    step(&mut d, &mut s, &mut screen);
    // Fertig.
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::Ready { .. }));
    step(&mut d, &mut s, &mut screen);
    d.write_to(dir.join("a.stl"), &s);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Done(..)));
    step(&mut d, &mut s, &mut screen);
    // Fehler.
    d.write_to(dir.join("gibt-es-nicht").join("a.stl"), &s);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Failed(_)));
    step(&mut d, &mut s, &mut screen);
    d.recompute();
    d.tick(&s, false);
    d.cancel();
    assert!(matches!(d.status(&s), ExportStatus::Failed(_)));
    step(&mut d, &mut s, &mut screen);
    for c in &centres {
        assert!((*c - centres[0]).abs().max_elem() <= 1.0, "{centres:?}");
    }
    assert!(
        (centres[0] - pos2(600.0, 400.0)).abs().max_elem() <= 1.0,
        "{centres:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}
