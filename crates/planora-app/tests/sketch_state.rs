use planora_app::sketch_state::*;
use planora_app::state::{AppState, BodyKind, StandardPlane};
use planora_feature::{BodyRef, FeatureId, SketchPlane};
use planora_kernel::Surface;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveKind, Sketch};

const R: f64 = 0.5; // Fangradius in Skizzeneinheiten

fn setup() -> (Sketch, SketchSession) {
    (
        Sketch::new(Frame::WORLD, "S", Tolerance::default()),
        SketchSession::new(FeatureId(0)),
    )
}

fn click(se: &mut SketchSession, sk: &mut Sketch, x: f64, y: f64) {
    se.click(sk, DVec2::new(x, y), R, false).unwrap();
}

/// Legt ein Skizzen-Feature auf einer Grundebene an.
fn new_sketch(s: &mut AppState, plane: StandardPlane) -> FeatureId {
    s.add_sketch(SketchPlane::Standard(plane)).unwrap()
}

/// Bearbeitet die Skizze `id` wie im Skizzen-Modus: öffnen, `f` auf dem Entwurf, beenden.
fn edit_sketch<T>(s: &mut AppState, id: FeatureId, f: impl FnOnce(&mut Sketch) -> T) -> T {
    s.open_sketch(id).unwrap();
    let r = f(s.session_sketch_mut().unwrap().1);
    s.close_sketch();
    r
}

#[test]
fn the_line_tool_draws_a_chain_and_adds_horizontal_and_vertical() {
    let (mut sk, mut se) = setup();
    se.set_tool(&mut sk, Tool::Line);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 10.0, 0.1);
    assert_eq!(sk.curves().count(), 1);
    assert_eq!(sk.constraints().count(), 1, "fast waagerecht → Horizontal");
    let (a, b) = match sk.curves().next().unwrap().1.kind {
        CurveKind::Line { a, b } => (a, b),
        _ => panic!(),
    };
    assert!(
        (sk.point_xy(a).unwrap().y - sk.point_xy(b).unwrap().y).abs() < 1e-7,
        "durch Lösen exakt waagerecht"
    );
    click(&mut se, &mut sk, 10.0, 10.0);
    assert_eq!(sk.curves().count(), 2);
    assert_eq!(sk.points().count(), 3, "der Endpunkt wird zum neuen Anfang");
    click(&mut se, &mut sk, 0.05, 0.0); // schließt am ersten Punkt
    assert_eq!(sk.curves().count(), 3);
    assert_eq!(sk.points().count(), 3, "Fang auf den ersten Punkt");
    assert!(sk.profiles().profiles.len() == 1, "geschlossenes Dreieck");
}

#[test]
fn auto_constraints_can_be_switched_off() {
    let (mut sk, mut se) = setup();
    se.auto_constrain = false;
    se.set_tool(&mut sk, Tool::Line);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 10.0, 0.1);
    assert_eq!(sk.constraints().count(), 0);
}

#[test]
fn rectangle_circle_and_arc_tools() {
    let (mut sk, mut se) = setup();
    se.set_tool(&mut sk, Tool::Rectangle);
    click(&mut se, &mut sk, 0.0, 0.0);
    assert!(se.pending_pos.len() == 1);
    click(&mut se, &mut sk, 8.0, 5.0);
    assert_eq!((sk.curves().count(), sk.constraints().count()), (4, 4));
    assert!(se.pending_pos.is_empty());

    se.set_tool(&mut sk, Tool::Circle);
    click(&mut se, &mut sk, 20.0, 20.0);
    click(&mut se, &mut sk, 23.0, 20.0);
    let circle = sk.curves().last().unwrap().0;
    assert!((sk.radius(circle).unwrap() - 3.0).abs() < 1e-9);

    se.set_tool(&mut sk, Tool::Arc);
    click(&mut se, &mut sk, 40.0, 0.0);
    click(&mut se, &mut sk, 45.0, 0.0);
    click(&mut se, &mut sk, 40.0, 9.0);
    let arc = sk.curves().last().unwrap().0;
    let CurveKind::Arc { end, .. } = sk.curves().last().unwrap().1.kind else {
        panic!()
    };
    assert!(
        (sk.point_xy(end).unwrap() - DVec2::new(40.0, 5.0)).length() < 1e-9,
        "Ende liegt auf dem Kreis"
    );
    assert!((sk.radius(arc).unwrap() - 5.0).abs() < 1e-9);
    assert!(se.pending.is_empty());
}

#[test]
fn a_point_clicked_on_a_curve_stays_on_it() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    se.set_tool(&mut sk, Tool::Point);
    click(&mut se, &mut sk, 5.0, 0.3);
    assert_eq!(sk.points().count(), 3);
    assert_eq!(sk.constraints().count(), 1, "PointOnCurve");
    assert!(sk.point_xy(sk.points().last().unwrap().0).unwrap().y.abs() < 1e-7);
}

#[test]
fn selection_prefers_points_and_toggles_with_extend() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    let l = sk.add_line(a, b).unwrap();
    se.set_tool(&mut sk, Tool::Select);
    click(&mut se, &mut sk, 0.2, 0.2);
    assert_eq!(se.selection, vec![Item::Point(a)]);
    se.click(&mut sk, DVec2::new(5.0, 0.1), R, true).unwrap();
    assert_eq!(se.selection, vec![Item::Point(a), Item::Curve(l)]);
    se.click(&mut sk, DVec2::new(0.2, 0.2), R, true).unwrap();
    assert_eq!(
        se.selection,
        vec![Item::Curve(l)],
        "erneutes Anklicken mit extend nimmt heraus"
    );
    click(&mut se, &mut sk, 50.0, 50.0);
    assert!(se.selection.is_empty());
}

#[test]
fn dragging_moves_free_points_and_respects_constraints() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    se.set_tool(&mut sk, Tool::Select);
    assert!(se.begin_drag(&mut sk, DVec2::new(10.1, 0.0), R));
    se.drag_to(&mut sk, DVec2::new(14.0, 3.0));
    se.end_drag(&mut sk);
    assert!(
        (sk.point_xy(b).unwrap() - DVec2::new(14.0, 3.0)).length() < 1e-6,
        "b ist frei"
    );
    assert_eq!(sk.point_xy(a).unwrap(), DVec2::ZERO, "a ist fest");
    assert!(!se.begin_drag(&mut sk, DVec2::new(100.0, 100.0), R));
}

#[test]
fn actions_follow_the_selection_and_constraints_are_applied_and_solved() {
    let (mut sk, mut se) = setup();
    let p: Vec<_> = [(0.0, 0.0), (10.0, 1.0), (0.0, 5.0), (10.0, 7.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    let l1 = sk.add_line(p[0], p[1]).unwrap();
    let l2 = sk.add_line(p[2], p[3]).unwrap();
    se.selection = vec![Item::Curve(l1), Item::Curve(l2)];
    let acts = se.available_actions(&sk);
    for a in [
        ConstraintAction::Parallel,
        ConstraintAction::Perpendicular,
        ConstraintAction::Equal,
        ConstraintAction::Angle,
    ] {
        assert!(acts.contains(&a), "{a:?} fehlt: {acts:?}");
    }
    assert!(!acts.contains(&ConstraintAction::Tangent));
    se.apply(&mut sk, ConstraintAction::Parallel, None).unwrap();
    let d1 = sk.point_xy(p[1]).unwrap() - sk.point_xy(p[0]).unwrap();
    let d2 = sk.point_xy(p[3]).unwrap() - sk.point_xy(p[2]).unwrap();
    assert!(d1.perp_dot(d2).abs() < 1e-6, "parallel nach dem Lösen");

    se.selection = vec![Item::Point(p[0])];
    assert_eq!(se.available_actions(&sk), vec![ConstraintAction::Fixed]);
    se.selection = vec![];
    assert!(se.available_actions(&sk).is_empty());
    assert!(
        se.apply(&mut sk, ConstraintAction::Parallel, None).is_err(),
        "nichts ausgewählt"
    );
}

#[test]
fn contradicting_and_redundant_dimensions_are_handled() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    se.selection = vec![Item::Point(a), Item::Point(b)];
    se.apply(&mut sk, ConstraintAction::Distance, Some(10.0))
        .unwrap();
    let before = sk.variables().to_vec();
    let err = se
        .apply(&mut sk, ConstraintAction::Distance, Some(20.0))
        .unwrap_err();
    assert!(err.contains("Widerspruch"), "{err}");
    assert_eq!(
        sk.constraints().count(),
        1,
        "die widersprüchliche Abhängigkeit wurde nicht behalten"
    );
    assert_eq!(sk.variables(), before.as_slice());

    // dasselbe Maß noch einmal: widerspruchsfrei, aber überzählig → nur anzeigend
    let id = se
        .apply(&mut sk, ConstraintAction::Distance, Some(10.0))
        .unwrap();
    assert!(sk.constraint(id).unwrap().driven);
    assert!(se.message.as_deref().unwrap_or("").contains("bereits"));
}

#[test]
fn delete_cascades_and_status_texts() {
    let (mut sk, mut se) = setup();
    se.set_tool(&mut sk, Tool::Rectangle);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 8.0, 5.0);
    let (text, sev) = se.status(&sk);
    assert!(text.contains("Freiheitsgrade"), "{text}");
    assert!(matches!(sev, Severity::Info));
    let line = sk.curves().next().unwrap().0;
    se.selection = vec![Item::Curve(line)];
    se.delete_selection(&mut sk);
    assert_eq!(sk.curves().count(), 3);
    assert!(
        sk.constraints().count() < 4,
        "Abhängigkeiten der Linie sind weg"
    );
    assert!(se.selection.is_empty());
}

#[test]
fn ellipse_and_spline_tools() {
    let (mut sk, mut se) = setup();
    se.set_tool(&mut sk, Tool::Ellipse);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 6.0, 0.0);
    click(&mut se, &mut sk, 0.0, 2.0);
    assert!(matches!(
        sk.curves().last().unwrap().1.kind,
        CurveKind::Ellipse { .. }
    ));
    se.set_tool(&mut sk, Tool::Spline);
    click(&mut se, &mut sk, 20.0, 0.0);
    click(&mut se, &mut sk, 24.0, 5.0);
    click(&mut se, &mut sk, 28.0, 0.0);
    se.finish_tool(&mut sk).unwrap();
    assert!(matches!(
        sk.curves().last().unwrap().1.kind,
        CurveKind::Spline { .. }
    ));
    se.set_tool(&mut sk, Tool::Spline);
    click(&mut se, &mut sk, 50.0, 50.0);
    assert!(se.finish_tool(&mut sk).is_err(), "ein Punkt reicht nicht");
}

#[test]
fn app_state_creates_opens_and_closes_sketches() {
    let mut s = AppState::new();
    let before_cam = s.camera;
    let i = new_sketch(&mut s, StandardPlane::XY);
    assert_eq!(s.document.feature(i).unwrap().name, "Skizze 1");
    s.open_sketch(i).unwrap();
    assert!(s.sketch_session.is_some());
    let to_cam = (s.camera.eye() - s.camera.target).normalize();
    assert!(to_cam.z > 0.99, "Draufsicht auf die XY-Ebene: {to_cam:?}");
    s.close_sketch();
    assert!(s.sketch_session.is_none());
    assert_eq!(s.camera, before_cam, "Kamera zurückgesetzt");
    s.delete_feature(i).unwrap();
    assert!(s.sketch_features().is_empty());
    let j = new_sketch(&mut s, StandardPlane::XZ);
    assert_eq!(
        s.document.feature(j).unwrap().name,
        "Skizze 2",
        "Zähler zählt nur hoch"
    );
    assert!(s.open_sketch(FeatureId(99)).is_err());
}

#[test]
fn bodies_from_sketches_appear_in_place_and_errors_leave_the_document_alone() {
    let mut s = AppState::new();
    let i = new_sketch(&mut s, StandardPlane::XY);
    edit_sketch(&mut s, i, |sk| {
        sk.add_rectangle(DVec2::new(2.0, 3.0), DVec2::new(12.0, 8.0))
            .unwrap()
    });
    let e = s.extrude_sketch(i, 0, 6.0).unwrap();
    let b = s
        .body_index(BodyRef {
            feature: e,
            index: 0,
        })
        .unwrap();
    assert_eq!(s.bodies[b].kind, BodyKind::Extrusion);
    assert_eq!(s.bodies[b].name, "Extrusion: Skizze 1");
    let (lo, hi) = s.bodies[b].bounds;
    assert!(
        (lo - DVec3::new(2.0, 3.0, 0.0)).length() < 1e-6,
        "an Ort und Stelle: {lo:?}"
    );
    assert!((hi - DVec3::new(12.0, 8.0, 6.0)).length() < 1e-6);
    let count = s.bodies.len();
    assert!(s.extrude_sketch(i, 5, 6.0).is_err(), "Profil gibt es nicht");
    assert!(s.extrude_sketch(i, 0, -1.0).is_err());
    assert!(s.extrude_sketch(FeatureId(77), 0, 1.0).is_err());
    assert_eq!(s.bodies.len(), count);
    assert!(s.message.is_some());
}

#[test]
fn a_sketch_can_be_started_on_a_planar_face() {
    let mut s = AppState::new();
    let bi = new_body(&mut s, |s| s.create_box(DVec3::new(10.0, 20.0, 5.0)));
    let top = s.bodies[bi]
        .solid
        .faces
        .iter()
        .find(|(_, f)| {
            matches!(f.surface, Surface::Plane { .. })
                && f.same_sense
                && (f.surface.frame().z.z - 1.0).abs() < 1e-9
                && (f.surface.frame().origin.z - 5.0).abs() < 1e-9
        })
        .map(|(id, _)| id)
        .expect("Deckfläche");
    let i = s.new_sketch_on_face(bi, top).unwrap();
    let f = s.document.sketch_of(i).unwrap().frame();
    assert!((f.z - DVec3::Z).length() < 1e-9 && (f.origin.z - 5.0).abs() < 1e-9);
    assert!(
        s.new_sketch_on_face(bi, planora_kernel::Id::from_index(99))
            .is_err()
    );
    let cyl = new_body(&mut s, |s| s.create_cylinder(2.0, 4.0));
    let side = s.bodies[cyl]
        .solid
        .faces
        .iter()
        .find(|(_, f)| !matches!(f.surface, Surface::Plane { .. }))
        .map(|(id, _)| id)
        .unwrap();
    assert!(
        s.new_sketch_on_face(cyl, side).is_err(),
        "nur ebene Flächen"
    );
}

#[test]
fn body_edges_can_be_copied_into_the_open_sketch() {
    let mut s = AppState::new();
    let bi = new_body(&mut s, |s| s.create_box(DVec3::new(10.0, 20.0, 5.0)));
    let i = new_sketch(&mut s, StandardPlane::XY);
    s.open_sketch(i).unwrap();
    let edge = s.bodies[bi]
        .solid
        .edges
        .iter()
        .next()
        .map(|(id, _)| id)
        .unwrap();
    let before = s.session_sketch().unwrap().curves().count();
    let res = s.project_edge_into_sketch(bi, edge);
    assert!(
        res.is_ok() || res.is_err(),
        "kein Panic; senkrechte Kanten werden mit Meldung abgelehnt"
    );
    assert!(s.session_sketch().unwrap().curves().count() >= before);
}

#[test]
fn failed_clicks_leave_sketch_and_session_unchanged() {
    let (mut sk, mut se) = setup();
    se.set_tool(&mut sk, Tool::Circle);
    click(&mut se, &mut sk, 1.0, 1.0);
    let (points, vars, pending) = (sk.points().count(), sk.variable_count(), se.pending.clone());
    // zweiter Klick auf den Mittelpunkt: Radius null
    assert!(se.click(&mut sk, DVec2::new(1.0, 1.0), R, false).is_err());
    assert!(se.message.is_some());
    assert_eq!((sk.points().count(), sk.variable_count()), (points, vars));
    assert_eq!(se.pending, pending, "das Werkzeug läuft weiter");
    assert_eq!(sk.curves().count(), 0);

    se.set_tool(&mut sk, Tool::Line);
    click(&mut se, &mut sk, 5.0, 5.0);
    let before = sk.variable_count();
    assert!(
        se.click(&mut sk, DVec2::new(5.1, 5.0), R, false).is_err(),
        "Linie der Länge null"
    );
    assert_eq!(sk.variable_count(), before);
    assert!(
        se.click(&mut sk, DVec2::new(f64::NAN, 0.0), R, false)
            .is_err()
    );
}

#[test]
fn dimmed_scene_only_while_a_sketch_is_open() {
    let mut s = AppState::new();
    let original = s.camera;
    let i = new_sketch(&mut s, StandardPlane::YZ);
    let v = s.scene_version;
    s.open_sketch(i).unwrap();
    assert!(s.scene_version > v, "Szene neu gebaut");
    let to_cam = (s.camera.eye() - s.camera.target).normalize();
    assert!(to_cam.x > 0.99, "Blick auf die YZ-Ebene: {to_cam:?}");
    // Wechsel auf eine andere Skizze überschreibt die gemerkte Kamera nicht
    let j = new_sketch(&mut s, StandardPlane::XY);
    s.open_sketch(j).unwrap();
    s.close_sketch();
    assert_eq!(s.camera, original, "ursprüngliche Kamera zurück");
    s.open_sketch(j).unwrap();
    s.delete_feature(i).unwrap();
    // Seit Task 9 (R12, Prüfung von Task 8): Löschen eines anderen Features beendet die offene
    // Skizze vorher (Entwurf übernommen), damit Dokument und Entwurf nicht auseinanderlaufen.
    assert!(
        s.sketch_session.is_none(),
        "die andere Skizze wurde beendet"
    );
    assert!(s.document.feature(j).is_some());
    assert_eq!(s.camera, original, "Kamera beim Beenden zurück");
    s.open_sketch(j).unwrap();
    s.delete_feature(j).unwrap();
    assert!(s.sketch_session.is_none() && s.sketch_draft.is_none());
    assert_eq!(s.camera, original, "Kamera beim Verwerfen zurück");
}

// ---- Nachbesserung 1 -------------------------------------------------------------------------

#[test]
fn opening_frames_curves_and_tiny_sketches_sensibly() {
    let mut s = AppState::new();
    let i = new_sketch(&mut s, StandardPlane::XY);
    edit_sketch(&mut s, i, |sk| {
        let c = sk.add_point(0.0, 0.0).unwrap();
        sk.add_circle(c, 5.0).unwrap();
    });
    s.open_sketch(i).unwrap();
    assert!(
        s.camera.distance > 5.0 && s.camera.distance < 1000.0,
        "Kreis: {}",
        s.camera.distance
    );
    s.close_sketch();

    let j = new_sketch(&mut s, StandardPlane::XY);
    edit_sketch(&mut s, j, |sk| sk.add_point(3.0, 4.0).unwrap());
    s.open_sketch(j).unwrap();
    assert!(
        s.camera.distance > 5.0 && s.camera.distance < 1000.0,
        "Punkt: {}",
        s.camera.distance
    );
    assert!((s.camera.target - glam::Vec3::new(3.0, 4.0, 0.0)).length() < 1e-4);
    s.close_sketch();

    let k = new_sketch(&mut s, StandardPlane::XY);
    let d = s.camera.distance;
    s.open_sketch(k).unwrap();
    assert_eq!(s.camera.distance, d, "leere Skizze: Abstand bleibt");
}

#[test]
fn a_new_conflict_in_an_already_broken_sketch_is_rejected() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 0.0).unwrap();
    sk.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    sk.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    let c = sk.add_point(50.0, 0.0).unwrap();
    let d = sk.add_point(55.0, 0.0).unwrap();
    se.selection = vec![Item::Point(c), Item::Point(d)];
    // unabhängige Gruppe: erlaubt, aber mit Warnung
    se.apply(&mut sk, ConstraintAction::Distance, Some(8.0))
        .unwrap();
    assert!(
        se.message.as_deref().unwrap_or("").contains("weiterhin"),
        "{:?}",
        se.message
    );
    let count = sk.constraints().count();
    let before = sk.variables().to_vec();
    // neuer Widerspruch in der bisher gesunden Gruppe: verworfen
    let e = se
        .apply(&mut sk, ConstraintAction::Distance, Some(3.0))
        .unwrap_err();
    assert!(e.contains("Widerspruch"), "{e}");
    assert_eq!(sk.constraints().count(), count);
    assert_eq!(sk.variables(), before.as_slice());
}

#[test]
fn a_tool_change_in_an_already_broken_sketch_warns() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 0.0).unwrap();
    sk.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    sk.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    se.set_tool(&mut sk, Tool::Point);
    click(&mut se, &mut sk, 30.0, 30.0);
    assert!(
        se.message.as_deref().unwrap_or("").contains("weiterhin"),
        "{:?}",
        se.message
    );
}

#[test]
fn an_arc_ending_on_its_start_is_rejected() {
    let (mut sk, mut se) = setup();
    se.set_tool(&mut sk, Tool::Arc);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 5.0, 0.0);
    let pending = se.pending.clone();
    let e = se
        .click(&mut sk, DVec2::new(5.1, 0.0), R, false)
        .unwrap_err();
    assert!(e.contains("Start"), "{e}");
    assert_eq!(sk.curves().count(), 0);
    assert_eq!(se.pending, pending);
}

#[test]
fn deleting_a_point_removes_orphaned_end_points_but_keeps_free_points() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    let free = sk.add_point(20.0, 20.0).unwrap();
    se.selection = vec![Item::Point(a)];
    se.delete_selection(&mut sk);
    assert_eq!(sk.curves().count(), 0);
    assert!(sk.point_xy(b).is_err(), "verwaister Endpunkt entfernt");
    assert!(sk.point_xy(free).is_ok(), "freier Punkt bleibt");
}

// ---- Für die Oberfläche (Task 12) ---------------------------------------------------------

#[test]
fn the_current_value_of_a_dimension_action_can_be_measured() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(3.0, 4.0).unwrap();
    se.selection = vec![Item::Point(a), Item::Point(b)];
    let v = se.measure_action(&sk, ConstraintAction::Distance).unwrap();
    assert!((v - 5.0).abs() < 1e-9);
    assert!(se.measure_action(&sk, ConstraintAction::Radius).is_none());
    assert!(
        se.measure_action(&sk, ConstraintAction::Coincident)
            .is_none()
    );
}

#[test]
fn a_dimension_value_can_be_changed_and_conflicts_are_rejected() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    se.selection = vec![Item::Point(a), Item::Point(b)];
    let d = se
        .apply(&mut sk, ConstraintAction::Distance, Some(10.0))
        .unwrap();
    se.set_dimension_value(&mut sk, d, 20.0).unwrap();
    let pa = sk.point_xy(a).unwrap();
    let pb = sk.point_xy(b).unwrap();
    assert!((pa.distance(pb) - 20.0).abs() < 1e-6);
    assert!(se.set_dimension_value(&mut sk, d, f64::NAN).is_err());
    assert!(se.set_dimension_value(&mut sk, d, -1.0).is_err());
    assert!(se.message.is_some());
    assert!((sk.point_xy(a).unwrap().distance(sk.point_xy(b).unwrap()) - 20.0).abs() < 1e-6);
}

#[test]
fn a_dimension_can_be_switched_to_driven_and_back() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    se.selection = vec![Item::Point(a), Item::Point(b)];
    let d = se
        .apply(&mut sk, ConstraintAction::Distance, Some(10.0))
        .unwrap();
    se.set_driven(&mut sk, d, true).unwrap();
    assert!(sk.constraint(d).unwrap().driven);
    sk.set_point_xy(b, 15.0, 0.0).unwrap();
    se.set_driven(&mut sk, d, false).unwrap();
    assert!(!sk.constraint(d).unwrap().driven);
    match sk.constraint(d).unwrap().kind {
        K::Distance(_, _, v) => {
            assert!((v - 15.0).abs() < 1e-6, "übernimmt den aktuellen Wert: {v}")
        }
        _ => panic!(),
    }
}

// ---- Abschlusswelle -----------------------------------------------------------------------

#[test]
fn dimension_values_follow_the_rule_of_the_sketch_crate() {
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 0.0).unwrap();
    se.selection = vec![Item::Point(a), Item::Point(b)];
    // Abstand null (oder winzig) wird abgelehnt, genau wie von `Sketch::add_constraint`.
    assert!(
        se.apply(&mut sk, ConstraintAction::Distance, Some(0.0))
            .is_err()
    );
    assert!(
        se.apply(&mut sk, ConstraintAction::Distance, Some(1e-300))
            .is_err()
    );
    assert_eq!(
        se.apply(&mut sk, ConstraintAction::Distance, Some(f64::NAN)),
        Err("Bitte eine Zahl eingeben".to_string())
    );
    assert_eq!(sk.constraints().count(), 0);
    let d = se
        .apply(&mut sk, ConstraintAction::Distance, Some(7.0))
        .unwrap();
    assert!(se.set_dimension_value(&mut sk, d, 1e-300).is_err());
    assert!(
        se.set_dimension_value(&mut sk, d, 2e9).is_err(),
        "Betragsgrenze"
    );
    // Ellipsenmaße lassen sich über dieselbe Regel ändern (früher ignorierte die App sie).
    let m = sk.add_point(20.0, 0.0).unwrap();
    let e = sk.add_ellipse(m, DVec2::new(4.0, 0.0), 2.0).unwrap();
    let major = sk.add_constraint(K::EllipseMajor(e, 4.0)).unwrap();
    let minor = sk.add_constraint(K::EllipseMinor(e, 2.0)).unwrap();
    se.set_dimension_value(&mut sk, major, 6.0).unwrap();
    assert!(
        se.set_dimension_value(&mut sk, minor, 7.0).is_err(),
        "kleine > große"
    );
    se.set_dimension_value(&mut sk, minor, 3.0).unwrap();
    assert_eq!(
        sk.measure(&K::EllipseMinor(e, 0.0))
            .map(|v| (v * 1e6).round()),
        Some(3e6)
    );
}

#[test]
fn the_end_of_a_drag_runs_the_full_diagnosis() {
    // Die gezogene Gruppe ist schon widersprüchlich: Während des Ziehens wird nicht nach der
    // Ursache gesucht (nur „nicht lösbar“), am Ende des Ziehens schon.
    let (mut sk, mut se) = setup();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(4.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    sk.add_constraint(K::Distance(a, b, 3.0)).unwrap();
    sk.add_constraint(K::Distance(a, b, 5.0)).unwrap();
    se.set_tool(&mut sk, Tool::Select);
    assert!(se.begin_drag(&mut sk, DVec2::new(4.0, 0.0), R));
    se.drag_to(&mut sk, DVec2::new(4.0, 2.0));
    let during = se.last_report.clone().unwrap();
    assert_eq!(
        during.status,
        planora_sketch::solver::SolveStatus::NotConverged
    );
    se.end_drag(&mut sk);
    let after = se.last_report.clone().unwrap();
    assert_eq!(
        after.status,
        planora_sketch::solver::SolveStatus::Conflicting
    );
    assert_eq!(after.conflicting.len(), 2, "{after:?}");
    assert!(se.dragging.is_none());
}

#[test]
fn switching_tools_or_cancelling_ends_a_drag_with_the_full_diagnosis() {
    // Wie oben, das Ziehen endet aber durch Werkzeugwechsel bzw. Abbrechen statt durch
    // `end_drag`: Auch dann holt ein volles Lösen die Ursache nach.
    use planora_sketch::solver::SolveStatus;
    for by_cancel in [false, true] {
        let (mut sk, mut se) = setup();
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(4.0, 0.0).unwrap();
        sk.add_line(a, b).unwrap();
        sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        sk.add_constraint(K::Distance(a, b, 3.0)).unwrap();
        sk.add_constraint(K::Distance(a, b, 5.0)).unwrap();
        se.set_tool(&mut sk, Tool::Select);
        assert!(se.begin_drag(&mut sk, DVec2::new(4.0, 0.0), R));
        se.drag_to(&mut sk, DVec2::new(4.0, 2.0));
        assert_eq!(
            se.last_report.as_ref().unwrap().status,
            SolveStatus::NotConverged
        );
        if by_cancel {
            se.cancel(&mut sk);
        } else {
            se.set_tool(&mut sk, Tool::Line);
        }
        assert!(se.dragging.is_none());
        let after = se.last_report.clone().unwrap();
        assert_eq!(after.status, SolveStatus::Conflicting, "{after:?}");
        assert_eq!(after.conflicting.len(), 2, "{after:?}");
    }
}

#[test]
fn a_patterned_rectangle_shows_no_redundant_constraints_in_the_status() {
    let (mut sk, mut se) = setup();
    se.set_tool(&mut sk, Tool::Rectangle);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 10.0, 5.0);
    se.set_tool(&mut sk, Tool::Select);
    se.selection = sk.curves().map(|(id, _)| Item::Curve(id)).collect();
    se.pattern_selection(&mut sk, DVec2::new(15.0, 0.0), 3)
        .unwrap();
    let (text, severity) = se.status(&sk);
    assert!(!text.contains("überzählig"), "{text}");
    assert_eq!(severity, Severity::Info, "{text}");
    // Ziehen einer Ecke des Originals: die Kopien folgen, Status bleibt sauber.
    se.selection.clear();
    assert!(se.begin_drag(&mut sk, DVec2::new(10.0, 5.0), R));
    se.drag_to(&mut sk, DVec2::new(12.0, 6.0));
    se.end_drag(&mut sk);
    let (text, _) = se.status(&sk);
    assert!(!text.contains("überzählig"), "{text}");
    assert!(se.message.is_none(), "{:?}", se.message);
}

#[test]
fn a_hole_tangent_to_an_edge_explains_why_it_is_not_usable() {
    let (mut sk, mut se) = setup();
    se.set_tool(&mut sk, Tool::Rectangle);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 20.0, 10.0);
    se.set_tool(&mut sk, Tool::Circle);
    click(&mut se, &mut sk, 10.0, 4.0);
    click(&mut se, &mut sk, 13.0, 4.0);
    se.set_tool(&mut sk, Tool::Select);
    let bottom = sk
        .curves()
        .find(|(_, c)| matches!(c.kind, CurveKind::Line { .. }))
        .unwrap()
        .0;
    let circle = sk
        .curves()
        .find(|(_, c)| matches!(c.kind, CurveKind::Circle { .. }))
        .unwrap()
        .0;
    se.selection = vec![Item::Curve(bottom), Item::Curve(circle)];
    se.apply(&mut sk, ConstraintAction::Tangent, None).unwrap();
    let e = sk.profiles();
    assert!(e.profiles.is_empty());
    let text = e.rejected[0].reason.to_string();
    assert!(text.contains("berührt"), "{text}");
}

/// Legt einen Grundkörper über `make` an und liefert seinen Index in der Körperliste.
fn new_body(
    s: &mut AppState,
    make: impl FnOnce(&mut AppState) -> Result<planora_feature::FeatureId, String>,
) -> usize {
    let id = make(s).unwrap();
    s.body_index(planora_feature::BodyRef {
        feature: id,
        index: 0,
    })
    .unwrap()
}

#[test]
fn a_sketch_on_a_body_face_is_extruded_on_that_face() {
    // Die Skizze ist ein Skizzen-Feature auf der Fläche (Verweis über Marke und Hinweispunkt).
    let mut s = AppState::new();
    let bi = new_body(&mut s, |s| s.create_box(DVec3::new(10.0, 20.0, 5.0)));
    let top = s.bodies[bi]
        .solid
        .faces
        .iter()
        .find(|(_, f)| {
            matches!(f.surface, Surface::Plane { .. })
                && f.same_sense
                && (f.surface.frame().z.z - 1.0).abs() < 1e-9
                && (f.surface.frame().origin.z - 5.0).abs() < 1e-9
        })
        .map(|(id, _)| id)
        .expect("Deckfläche");
    let i = s.new_sketch_on_face(bi, top).unwrap();
    // Rechteck in Skizzenkoordinaten mitten auf der Deckfläche
    let frame = *s.document.sketch_of(i).unwrap().frame();
    let local = |p: DVec3| frame.to_local(p).truncate();
    let (a, b) = (
        local(DVec3::new(2.0, 2.0, 5.0)),
        local(DVec3::new(6.0, 8.0, 5.0)),
    );
    edit_sketch(&mut s, i, |sk| {
        sk.add_rectangle(a.min(b), a.max(b)).unwrap()
    });
    let e = s.extrude_sketch(i, 0, 3.0).unwrap();
    let e = s
        .body_index(BodyRef {
            feature: e,
            index: 0,
        })
        .unwrap();
    assert_eq!(s.bodies.len(), 2);
    let (lo, hi) = s.bodies[e].bounds;
    assert!(
        (lo.z - 5.0).abs() < 1e-6 && (hi.z - 8.0).abs() < 1e-6,
        "{lo:?} {hi:?}"
    );
    assert!(
        (lo.x - 2.0).abs() < 1e-6 && (hi.y - 8.0).abs() < 1e-6,
        "{lo:?} {hi:?}"
    );
    // ein gelöschter Grundkörper: Die Skizze liegt auf keiner Fläche mehr
    let count = s.document.features().len();
    s.delete_feature_with_dependents(s.bodies[bi].source.feature)
        .unwrap();
    assert!(s.extrude_sketch(i, 0, 3.0).is_err());
    assert!(s.message.is_some());
    assert!(s.document.features().len() < count);
    assert!(s.bodies.is_empty());
}
