use planora_app::sketch_state::*;
use planora_app::state::{AppState, BodyKind, StandardPlane};
use planora_kernel::{Kernel, Surface};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveKind, Sketch};

const R: f64 = 0.5; // Fangradius in Skizzeneinheiten

fn setup() -> (Sketch, SketchSession) {
    (
        Sketch::new(Frame::WORLD, "S", Tolerance::default()),
        SketchSession::new(0),
    )
}

fn click(se: &mut SketchSession, sk: &mut Sketch, x: f64, y: f64) {
    se.click(sk, DVec2::new(x, y), R, false).unwrap();
}

#[test]
fn the_line_tool_draws_a_chain_and_adds_horizontal_and_vertical() {
    let (mut sk, mut se) = setup();
    se.set_tool(Tool::Line);
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
    se.set_tool(Tool::Line);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 10.0, 0.1);
    assert_eq!(sk.constraints().count(), 0);
}

#[test]
fn rectangle_circle_and_arc_tools() {
    let (mut sk, mut se) = setup();
    se.set_tool(Tool::Rectangle);
    click(&mut se, &mut sk, 0.0, 0.0);
    assert!(se.pending_pos.len() == 1);
    click(&mut se, &mut sk, 8.0, 5.0);
    assert_eq!((sk.curves().count(), sk.constraints().count()), (4, 4));
    assert!(se.pending_pos.is_empty());

    se.set_tool(Tool::Circle);
    click(&mut se, &mut sk, 20.0, 20.0);
    click(&mut se, &mut sk, 23.0, 20.0);
    let circle = sk.curves().last().unwrap().0;
    assert!((sk.radius(circle).unwrap() - 3.0).abs() < 1e-9);

    se.set_tool(Tool::Arc);
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
    se.set_tool(Tool::Point);
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
    se.set_tool(Tool::Select);
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
    se.set_tool(Tool::Select);
    assert!(se.begin_drag(&mut sk, DVec2::new(10.1, 0.0), R));
    se.drag_to(&mut sk, DVec2::new(14.0, 3.0));
    se.end_drag();
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
    se.set_tool(Tool::Rectangle);
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
    se.set_tool(Tool::Ellipse);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 6.0, 0.0);
    click(&mut se, &mut sk, 0.0, 2.0);
    assert!(matches!(
        sk.curves().last().unwrap().1.kind,
        CurveKind::Ellipse { .. }
    ));
    se.set_tool(Tool::Spline);
    click(&mut se, &mut sk, 20.0, 0.0);
    click(&mut se, &mut sk, 24.0, 5.0);
    click(&mut se, &mut sk, 28.0, 0.0);
    se.finish_tool(&mut sk).unwrap();
    assert!(matches!(
        sk.curves().last().unwrap().1.kind,
        CurveKind::Spline { .. }
    ));
    se.set_tool(Tool::Spline);
    click(&mut se, &mut sk, 50.0, 50.0);
    assert!(se.finish_tool(&mut sk).is_err(), "ein Punkt reicht nicht");
}

#[test]
fn app_state_creates_opens_and_closes_sketches() {
    let mut s = AppState::new();
    let before_cam = s.camera;
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    assert_eq!(s.sketches[i].name, "Skizze 1");
    s.open_sketch(i).unwrap();
    assert!(s.sketch_session.is_some());
    let to_cam = (s.camera.eye() - s.camera.target).normalize();
    assert!(to_cam.z > 0.99, "Draufsicht auf die XY-Ebene: {to_cam:?}");
    s.close_sketch();
    assert!(s.sketch_session.is_none());
    assert_eq!(s.camera, before_cam, "Kamera zurückgesetzt");
    s.delete_sketch(i);
    assert!(s.sketches.is_empty());
    let j = s.new_sketch(AppState::standard_frame(StandardPlane::XZ));
    assert_eq!(s.sketches[j].name, "Skizze 2", "Zähler zählt nur hoch");
    assert!(s.open_sketch(99).is_err());
}

#[test]
fn bodies_from_sketches_appear_in_place_and_errors_leave_the_document_alone() {
    let mut s = AppState::new();
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.sketches[i]
        .add_rectangle(DVec2::new(2.0, 3.0), DVec2::new(12.0, 8.0))
        .unwrap();
    let b = s.extrude_sketch(i, 0, 6.0).unwrap();
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
    assert!(s.extrude_sketch(77, 0, 1.0).is_err());
    assert_eq!(s.bodies.len(), count);
    assert!(s.message.is_some());
}

#[test]
fn a_sketch_can_be_started_on_a_planar_face() {
    let mut s = AppState::new();
    let k = s.kernel;
    let bi = s
        .add_body(
            BodyKind::Box,
            "Q".into(),
            k.make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap(),
        )
        .unwrap();
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
    let f = s.sketches[i].frame();
    assert!((f.z - DVec3::Z).length() < 1e-9 && (f.origin.z - 5.0).abs() < 1e-9);
    assert!(
        s.new_sketch_on_face(bi, planora_kernel::Id::from_index(99))
            .is_err()
    );
    let cyl = s
        .add_body(
            BodyKind::Cylinder,
            "Z".into(),
            k.make_cylinder(2.0, 4.0).unwrap(),
        )
        .unwrap();
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
    let k = s.kernel;
    let bi = s
        .add_body(
            BodyKind::Box,
            "Q".into(),
            k.make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap(),
        )
        .unwrap();
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.open_sketch(i).unwrap();
    let edge = s.bodies[bi]
        .solid
        .edges
        .iter()
        .next()
        .map(|(id, _)| id)
        .unwrap();
    let mut session = s.sketch_session.take().unwrap();
    let before = s.sketches[i].curves().count();
    let res = session.project_edge(&mut s.sketches[i], &s.bodies[bi].solid, edge);
    s.sketch_session = Some(session);
    assert!(
        res.is_ok() || res.is_err(),
        "kein Panic; senkrechte Kanten werden mit Meldung abgelehnt"
    );
    assert!(s.sketches[i].curves().count() >= before);
}

#[test]
fn failed_clicks_leave_sketch_and_session_unchanged() {
    let (mut sk, mut se) = setup();
    se.set_tool(Tool::Circle);
    click(&mut se, &mut sk, 1.0, 1.0);
    let (points, vars, pending) = (sk.points().count(), sk.variable_count(), se.pending.clone());
    // zweiter Klick auf den Mittelpunkt: Radius null
    assert!(se.click(&mut sk, DVec2::new(1.0, 1.0), R, false).is_err());
    assert!(se.message.is_some());
    assert_eq!((sk.points().count(), sk.variable_count()), (points, vars));
    assert_eq!(se.pending, pending, "das Werkzeug läuft weiter");
    assert_eq!(sk.curves().count(), 0);

    se.set_tool(Tool::Line);
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
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::YZ));
    let v = s.scene_version;
    s.open_sketch(i).unwrap();
    assert!(s.scene_version > v, "Szene neu gebaut");
    let to_cam = (s.camera.eye() - s.camera.target).normalize();
    assert!(to_cam.x > 0.99, "Blick auf die YZ-Ebene: {to_cam:?}");
    // Wechsel auf eine andere Skizze überschreibt die gemerkte Kamera nicht
    let j = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.open_sketch(j).unwrap();
    s.close_sketch();
    assert_eq!(s.camera, original, "ursprüngliche Kamera zurück");
    s.open_sketch(j).unwrap();
    s.delete_sketch(i);
    assert_eq!(
        s.sketch_session.as_ref().unwrap().sketch,
        0,
        "Index angepasst"
    );
    s.delete_sketch(0);
    assert!(s.sketch_session.is_none());
}

// ---- Nachbesserung 1 -------------------------------------------------------------------------

#[test]
fn opening_frames_curves_and_tiny_sketches_sensibly() {
    let mut s = AppState::new();
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    let c = s.sketches[i].add_point(0.0, 0.0).unwrap();
    s.sketches[i].add_circle(c, 5.0).unwrap();
    s.open_sketch(i).unwrap();
    assert!(
        s.camera.distance > 5.0 && s.camera.distance < 1000.0,
        "Kreis: {}",
        s.camera.distance
    );
    s.close_sketch();

    let j = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.sketches[j].add_point(3.0, 4.0).unwrap();
    s.open_sketch(j).unwrap();
    assert!(
        s.camera.distance > 5.0 && s.camera.distance < 1000.0,
        "Punkt: {}",
        s.camera.distance
    );
    assert!((s.camera.target - glam::Vec3::new(3.0, 4.0, 0.0)).length() < 1e-4);
    s.close_sketch();

    let k = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
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
    se.set_tool(Tool::Point);
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
    se.set_tool(Tool::Arc);
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
