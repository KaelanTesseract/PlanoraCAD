//! Freiformkörper in der App (ohne Fenster): Skizze mit Ellipse oder Spline, Extrusion,
//! Boolesche Operation, Verschieben, Löschen, Zurückrollen und die Anzeige „Freiformfläche“ /
//! „Freiformkurve“ im Eigenschaftenfenster.

use planora_app::state::AppState;
use planora_app::{fonts, properties};
use planora_feature::StandardPlane;
use planora_feature::{FeatureId, FeatureKind, FeatureStatus, SketchFeature, SketchPlane};
use planora_kernel::{BoolOp, CurveKind, Kernel, SurfaceKind};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_render::{EdgeRef, FaceRef, Pick};
use planora_sketch::ConstraintKind as K;
use planora_sketch::model::Sketch;

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn texts(s: &mut AppState) -> String {
    let c = ctx();
    let mut text = String::new();
    for _ in 0..3 {
        let out = c.run_ui(egui::RawInput::default(), |ui| properties::show(ui, s));
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

fn ellipse_sketch(a: f64, b: f64) -> Sketch {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let c = sk.add_point(0.0, 0.0).unwrap();
    let e = sk.add_ellipse(c, DVec2::new(a, 0.0), b).unwrap();
    sk.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    sk.add_constraint(K::EllipseAngle(e, 0.0)).unwrap();
    sk.add_constraint(K::EllipseMajor(e, a)).unwrap();
    sk.add_constraint(K::EllipseMinor(e, b)).unwrap();
    sk
}

fn spline_sketch() -> Sketch {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let p: Vec<_> = [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, 7.0), (0.0, 5.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    sk.add_line(p[0], p[1]).unwrap();
    sk.add_line(p[1], p[2]).unwrap();
    sk.add_spline(&p[2..5]).unwrap();
    sk.add_line(p[4], p[0]).unwrap();
    sk
}

fn add_sketch(s: &mut AppState, sk: Sketch) -> FeatureId {
    let id = s
        .document
        .add_feature(
            "Skizze",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    s.refresh();
    id
}

fn has_nurbs_face(s: &AppState, body: usize) -> bool {
    s.bodies[body]
        .solid
        .faces
        .iter()
        .any(|(_, f)| f.surface.kind() == SurfaceKind::Nurbs)
}

#[test]
fn ellipse_and_spline_sketches_extrude_to_freeform_bodies() {
    let mut s = AppState::new();
    let e = add_sketch(&mut s, ellipse_sketch(5.0, 3.0));
    s.extrude_sketch(e, 0, 4.0).unwrap();
    let sp = add_sketch(&mut s, spline_sketch());
    s.extrude_sketch(sp, 0, 2.0).unwrap();
    assert_eq!(s.bodies.len(), 2);
    assert!(has_nurbs_face(&s, 0) && has_nurbs_face(&s, 1));
    let want = std::f64::consts::PI * 5.0 * 3.0 * 4.0;
    // Die App tesselliert mit ihrer gröberen Sehnentoleranz (Netzvolumen etwas unter dem exakten).
    assert!(
        (s.bodies[0].mass.volume - want).abs() / want < 1e-2,
        "{}",
        s.bodies[0].mass.volume
    );
    assert!(s.bodies[1].mass.volume > 0.0);
    assert!(s.message.is_none(), "{:?}", s.message);
}

#[test]
fn the_properties_panel_names_freeform_faces_and_curves() {
    let mut s = AppState::new();
    let e = add_sketch(&mut s, ellipse_sketch(5.0, 3.0));
    s.extrude_sketch(e, 0, 4.0).unwrap();
    s.selected_body = Some(0);
    let solid = s.bodies[0].solid.clone();
    let face = solid
        .faces
        .iter()
        .find(|(_, f)| f.surface.kind() == SurfaceKind::Nurbs)
        .map(|(id, _)| id)
        .unwrap();
    s.set_selection(Some(Pick::Face(FaceRef { body: 0, face })));
    let text = texts(&mut s);
    assert!(
        text.contains("Ausgewählte Fläche: Freiformfläche"),
        "{text}"
    );

    let edge = solid
        .edges
        .iter()
        .find(|(_, e)| e.curve.kind() == CurveKind::Nurbs)
        .map(|(id, _)| id)
        .unwrap();
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge })));
    let text = texts(&mut s);
    assert!(text.contains("Ausgewählte Kante: Freiformkurve"), "{text}");
    // Die Ebenen der Deckel bleiben „Ebene“.
    assert_eq!(s.kernel.face_kind(&solid, face), Some(SurfaceKind::Nurbs));
}

/// Ellipsenzylinder, bei dem ein Quader als Werkzeug 1 mm in den Mantel ragt.
fn ellipse_and_box() -> AppState {
    let mut s = AppState::new();
    let e = add_sketch(&mut s, ellipse_sketch(5.0, 3.0));
    s.extrude_sketch(e, 0, 4.0).unwrap();
    s.document
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::splat(10.0),
                origin: DVec3::new(4.0, -5.0, -3.0),
            },
        )
        .unwrap();
    s.refresh();
    assert_eq!(s.bodies.len(), 2);
    s
}

#[test]
fn a_boolean_with_freeform_bodies_deletes_and_rolls_back() {
    let mut s = ellipse_and_box();
    let before = s.bodies[0].mass.volume;
    let f = s.add_boolean(0, 1, BoolOp::Difference).unwrap();
    assert_eq!(s.bodies.len(), 1);
    assert!(has_nurbs_face(&s, 0));
    assert!(s.bodies[0].mass.volume < before);
    s.document.check_invariants().unwrap();
    s.kernel.validate(&s.bodies[0].solid).unwrap();

    // Zurückrollen: Der Marker vor die Operation zeigt wieder zwei Körper.
    let n = s.document.features().len();
    s.document.set_marker(n - 1).unwrap();
    s.refresh();
    assert_eq!(s.bodies.len(), 2);
    s.document.set_marker(n).unwrap();
    s.refresh();
    assert_eq!(s.bodies.len(), 1);

    // Löschen der Operation bringt beide Körper zurück.
    s.delete_feature(f).unwrap();
    assert_eq!(s.bodies.len(), 2);
    assert!((s.bodies[0].mass.volume - before).abs() < 1e-9);
    s.document.check_invariants().unwrap();
}

#[test]
fn a_failed_freeform_boolean_leaves_the_document_unchanged_and_says_freeform() {
    let mut s = ellipse_and_box();
    // Quaderfläche 1e-3 mm innerhalb der Mantelspitze: Der Rechner lehnt die Lage benannt ab.
    s.document
        .update(s.document.features()[2].id, |k| {
            if let FeatureKind::Box { origin, .. } = k {
                origin.x = 5.0 - 1e-3;
            }
        })
        .unwrap();
    s.refresh();
    let n = s.document.features().len();
    let err = s.add_boolean(0, 1, BoolOp::Difference).unwrap_err();
    assert!(err.contains("Freiformfläche"), "{err}");
    assert_eq!(s.document.features().len(), n);
    assert_eq!(s.bodies.len(), 2);
    assert_eq!(s.message.as_deref(), Some(err.as_str()));
}

#[test]
fn moving_a_freeform_body_moves_it_without_changing_its_volume() {
    let mut s = AppState::new();
    let e = add_sketch(&mut s, ellipse_sketch(5.0, 3.0));
    s.extrude_sketch(e, 0, 4.0).unwrap();
    let (volume, bounds) = (s.bodies[0].mass.volume, s.bodies[0].bounds);
    let delta = DVec3::new(7.0, -2.0, 3.0);
    s.move_body(0, delta).unwrap();
    assert_eq!(s.bodies.len(), 1);
    assert!(has_nurbs_face(&s, 0));
    assert!((s.bodies[0].mass.volume - volume).abs() < 1e-9 * volume);
    assert!((s.bodies[0].bounds.0 - bounds.0 - delta).length() < 1e-6);
    assert!((s.bodies[0].bounds.1 - bounds.1 - delta).length() < 1e-6);
    // Ein zweites Verschieben erhöht dasselbe Feature (keine Kette).
    let n = s.document.features().len();
    s.move_body(0, delta).unwrap();
    assert_eq!(s.document.features().len(), n);
    s.delete_body(0).unwrap();
    assert!(s.bodies.is_empty());
    s.document.check_invariants().unwrap();
    assert!(
        s.document
            .features()
            .iter()
            .all(|f| f.status == FeatureStatus::Ok)
    );
}

/// Ungenaues Netz (Review, I4): Die Statuszeile bekommt einen Hinweis, sonst ändert sich nichts.
/// Eine große Ellipse bei feiner Netzgenauigkeit überschreitet die Höchstteilung ihrer Kanten
/// (`MAX_NURBS_DIVISIONS`); die Fläche steht dann in `Mesh::inexact_faces`.
#[test]
fn an_inexact_mesh_shows_a_note_in_the_status_line() {
    let mut s = AppState::new();
    let e = add_sketch(&mut s, ellipse_sketch(5.0, 3.0));
    s.extrude_sketch(e, 0, 4.0).unwrap();
    assert!(!s.bodies[0].mesh_inexact);
    assert_eq!(s.mesh_note(), None);

    let mut fine = AppState::new();
    fine.chord_tolerance = 1e-4;
    let big = add_sketch(&mut fine, ellipse_sketch(500.0, 300.0));
    fine.extrude_sketch(big, 0, 1.0).unwrap();
    assert_eq!(fine.bodies.len(), 1);
    assert!(fine.bodies[0].mesh_inexact);
    assert!(!fine.bodies[0].mesh.inexact_faces.is_empty());
    let note = fine.mesh_note().expect("Hinweis");
    assert!(note.contains(planora_app::state::MESH_NOTE), "{note}");
    assert!(note.contains(&fine.bodies[0].name), "{note}");
    // Nur ein Hinweis: keine Fehlermeldung, der Körper bleibt.
    assert!(fine.message.is_none(), "{:?}", fine.message);
    assert!(fine.bodies[0].mass.volume > 0.0);
}
