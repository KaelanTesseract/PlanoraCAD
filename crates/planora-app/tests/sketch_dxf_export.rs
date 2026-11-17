//! „Als DXF speichern“ im Browser (Skizzen 11b, Task 6): Vorgaben des Dialogs, Schreiben ohne
//! neuen Schritt, Meldung bei 3D-Skizzen und das Kontextmenü nur an Skizzenzeilen.

use egui::{Event, Modifiers, PointerButton, Rect, pos2};
use planora_app::browser;
use planora_app::sketch_dxf_dialog::{SketchDxfDialog, save_sketch_dxf};
use planora_app::state::AppState;
use planora_app::{fonts, theme};
use planora_drawing::plain_dxf::DxfUnit;
use planora_exchange::import2d::Element2d;
use planora_feature::{FeatureId, SketchDxfOptions, SketchPlane, StandardPlane};
use planora_i18n::{DisplayUnit, with_unit};
use planora_math::{DVec2, DVec3};
use std::path::PathBuf;

fn opts() -> SketchDxfOptions {
    SketchDxfOptions {
        unit: DxfUnit::Millimeter,
        points: true,
        projected: true,
        construction: false,
    }
}

fn sketch_with_rectangle(s: &mut AppState) -> FeatureId {
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::ZERO, DVec2::new(40.0, 30.0))
        .unwrap();
    s.close_sketch();
    id
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-sketch-dxf-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn defaults_follow_document_unit() {
    let mut s = AppState::new();
    let id = sketch_with_rectangle(&mut s);
    let d = with_unit(DisplayUnit::Zoll, || SketchDxfDialog::new(&s, id));
    assert_eq!(d.sketch, id);
    assert_eq!(d.options.unit, DxfUnit::Inch);
    assert!(d.options.points && d.options.projected);
    assert!(!d.options.construction);
    let d = with_unit(DisplayUnit::Millimeter, || SketchDxfDialog::new(&s, id));
    assert_eq!(d.options.unit, DxfUnit::Millimeter);
}

#[test]
fn save_writes_file_and_no_step() {
    let mut s = AppState::new();
    let id = sketch_with_rectangle(&mut s);
    let dir = temp_dir("save");
    let path = dir.join("Skizze 1.dxf");
    let edits = s.document.edit_count();
    save_sketch_dxf(&s, id, &opts(), &path).unwrap();
    assert!(path.exists());
    assert_eq!(s.document.edit_count(), edits, "kein neuer Schritt");
    let bytes = std::fs::read(&path).unwrap();
    let imp = planora_exchange::read_dxf(&bytes, &|| false).unwrap();
    let lines = imp
        .layers
        .iter()
        .flat_map(|l| &l.elements)
        .filter(|e| matches!(e, Element2d::Line(..)))
        .count();
    assert_eq!(lines, 4);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn three_d_sketch_message_no_file() {
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        let a = sk.add_point3(DVec3::new(0.0, 0.0, 5.0)).unwrap();
        let b = sk.add_point3(DVec3::new(10.0, 0.0, 5.0)).unwrap();
        sk.add_line(a, b).unwrap();
    }
    s.close_sketch();
    let dir = temp_dir("3d");
    let path = dir.join("x.dxf");
    let err = save_sketch_dxf(&s, id, &opts(), &path).unwrap_err();
    assert_eq!(err, planora_i18n::tr!("sketch-dxf-not-planar"));
    assert!(!path.exists(), "bei Fehler keine Datei");
    let _ = std::fs::remove_dir_all(dir);
}

fn right_click_texts(s: &mut AppState, row: &str) -> Vec<String> {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    let screen = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 800.0));
    let mut time = 0.0;
    let mut frame = |s: &mut AppState, events: Vec<Event>| {
        time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            events,
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| browser::show(ui, s));
        let mut texts = Vec::new();
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
                _ => {}
            }
        }
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut texts));
        out.drop_without_applying_deltas();
        texts
    };
    frame(s, vec![]);
    frame(s, vec![]);
    let rect = browser::row_rects(&ctx)
        .into_iter()
        .find(|(n, _)| n == row)
        .unwrap_or_else(|| panic!("Zeile {row} fehlt"))
        .1;
    let p = rect.center();
    let press = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Secondary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(s, vec![Event::PointerMoved(p), press(true), press(false)]);
    frame(s, vec![])
}

#[test]
fn menu_entry_on_sketch_rows_only() {
    let mut s = AppState::new();
    let sk = sketch_with_rectangle(&mut s);
    let name = s.document.feature(sk).unwrap().name.clone();
    s.create_box(DVec3::splat(4.0)).unwrap();
    let entry = planora_i18n::tr!("tree-save-as-dxf");
    assert_eq!(entry, "Als DXF speichern");
    let on_sketch = right_click_texts(&mut s, &name);
    assert!(on_sketch.contains(&entry), "{on_sketch:?}");
    let on_body = right_click_texts(&mut s, "Quader 1");
    assert!(on_body.contains(&"Unterdrücken".to_string()), "{on_body:?}");
    assert!(!on_body.contains(&entry), "{on_body:?}");
}
