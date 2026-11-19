//! Skizzen 12, Task 9: „Maße anzeigen“ an fertigen Skizzen. Aus ist die Vorgabe; der Haken im
//! Rechtsklickmenü (Baum und 3D-Ansicht) zeigt die Maße der fertigen Skizze als Overlay obenauf,
//! nicht anklickbar. Ein Schritt im Verlauf, gespeichert mit Format 23, nicht bei geöffneter Skizze.

use egui::Shape;
use planora_app::commands;
use planora_app::keymap::Action;
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_view};
use planora_feature::{FeatureId, SketchPlane};
use planora_i18n::tr;
use planora_project::config::Config;
use planora_project::{decode_version, encode_version};
use planora_render::Pick;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::CurveId;

const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(1920.0, 1080.0),
};

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

/// Fertige Skizze auf XY mit einer Linie (0, 0)–(20, 0) und dem Maß 20 an ihr; von oben gesehen.
fn finished_with_dimension(s: &mut AppState) -> (FeatureId, CurveId) {
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let curve = {
        let (_, sk) = s.session_sketch_mut().unwrap();
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(20.0, 0.0).unwrap();
        let line = sk.add_line(a, b).unwrap();
        sk.add_constraint(K::Distance(a, b, 20.0)).unwrap();
        sk.solve();
        line
    };
    s.observe_sketch(false);
    s.close_sketch();
    s.camera.set_viewport(1920.0, 1080.0);
    s.camera.set_view(planora_render::ViewFace::Top);
    s.camera.target = glam::Vec3::new(10.0, 0.0, 0.0);
    s.camera.distance = 80.0;
    (id, curve)
}

fn shown(s: &mut AppState, id: FeatureId, on: bool) {
    let mut d = s.document.sketch_display(id);
    d.dimensions_finished = on;
    s.document.set_sketch_display(id, d);
}

fn texts(shapes: &[Shape]) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(shapes: &[Shape], out: &mut Vec<String>) {
        for s in shapes {
            match s {
                Shape::Vec(v) => walk(v, out),
                Shape::Text(t) => out.push(t.galley.text().to_string()),
                _ => {}
            }
        }
    }
    walk(shapes, &mut out);
    out
}

fn dimension_shapes(ctx: &egui::Context, s: &AppState) -> Vec<Shape> {
    // Schriften stehen erst nach dem ersten Bild bereit.
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(SCREEN),
                ..Default::default()
            },
            |_| {
                shapes = sketch_view::finished_dimension_shapes(ctx, s, SCREEN);
            },
        );
        out.drop_without_applying_deltas();
    }
    shapes
}

#[test]
fn show_dimension_draws_finished_dimensions() {
    let ctx = context();
    let mut s = AppState::new();
    let (id, _) = finished_with_dimension(&mut s);
    assert!(
        dimension_shapes(&ctx, &s).is_empty(),
        "Vorgabe: keine Maße fertiger Skizzen"
    );

    shown(&mut s, id, true);
    let t = texts(&dimension_shapes(&ctx, &s));
    assert_eq!(t.len(), 1, "{t:?}");
    assert!(t[0].contains("20"), "{t:?}");
    // Nur Anzeige: weder ein Maßtext zum Doppelklick noch eine Auswahl.
    assert!(sketch_view::label_hits(&ctx, id).is_empty());

    // Auge aus: Die Skizze und ihre Maße sind weg.
    s.set_feature_hidden(id, true);
    assert!(dimension_shapes(&ctx, &s).is_empty(), "Auge aus");
    s.set_feature_hidden(id, false);
    assert_eq!(texts(&dimension_shapes(&ctx, &s)).len(), 1);

    // Der Schalter „Maße anzeigen“ der Skizze (aus) gilt auch fertig.
    let mut d = s.document.sketch_display(id);
    d.dimensions = false;
    s.document.set_sketch_display(id, d);
    assert!(dimension_shapes(&ctx, &s).is_empty(), "Schalter Maße aus");
}

#[test]
fn menu_entry_absent_while_sketch_open() {
    let mut s = AppState::new();
    let (id, _) = finished_with_dimension(&mut s);
    assert_eq!(s.sketch_dimension_menu(id), Some(false), "aus, Eintrag da");
    shown(&mut s, id, true);
    assert_eq!(s.sketch_dimension_menu(id), Some(true), "Haken gesetzt");

    s.open_sketch(id).unwrap();
    assert_eq!(s.sketch_dimension_menu(id), None, "offen: kein Eintrag");
    // Auch der Befehl tut dann nichts.
    s.toggle_finished_dimensions(id);
    assert!(s.document.sketch_display(id).dimensions_finished);
    // Und die offene Skizze zeichnet keine Maße als „fertig“.
    assert!(dimension_shapes(&context(), &s).is_empty());
}

#[test]
fn toggle_is_undo_step_with_display_label() {
    let mut w = Workspace::new(None, Config::default());
    let (id, curve) = {
        let s = &mut w.active_tab_mut().state;
        finished_with_dimension(s)
    };
    w.observe_history(false);
    let len = w.active_tab().history().undo_len();
    // Wie im 3D-Menü: gewählte Skizzengeometrie bestimmt die Skizze.
    w.active_tab_mut()
        .state
        .set_selection(Some(Pick::SketchCurve { sketch: id, curve }));
    commands::run_action(&mut w, Action::SketchShowDimensionFinished);
    w.observe_history(false);
    assert!(
        w.active_tab()
            .state
            .document
            .sketch_display(id)
            .dimensions_finished
    );
    assert_eq!(w.active_tab().history().undo_len(), len + 1);
    assert_eq!(
        w.active_tab().undo_label().as_deref(),
        Some(tr!("undo-sketch-display").as_str())
    );
    commands::run_action(&mut w, Action::SketchShowDimensionFinished);
    w.observe_history(false);
    assert!(
        !w.active_tab()
            .state
            .document
            .sketch_display(id)
            .dimensions_finished
    );
    assert_eq!(w.active_tab().history().undo_len(), len + 2);
}

#[test]
fn saved_and_loaded_with_format_23() {
    let mut s = AppState::new();
    let (id, _) = finished_with_dimension(&mut s);
    let plain = encode_version(&s.snapshot()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&plain).unwrap();
    assert_ne!(
        v["format"].as_u64(),
        Some(23),
        "Vorgabe schreibt kein Format 23"
    );

    shown(&mut s, id, true);
    let text = encode_version(&s.snapshot()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["format"].as_u64(), Some(23));
    assert_eq!(v["skizzenanzeige"][0]["masse_fertig"], true, "{text}");

    let back = AppState::from_snapshot(decode_version(&text, "x.json").unwrap()).unwrap();
    assert!(back.document.sketch_display(id).dimensions_finished);
}
