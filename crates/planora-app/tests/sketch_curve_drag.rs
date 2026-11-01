//! Ganze Kurven ziehen (Skizzen 2, Task 8): direkt an der Sitzung und über Rohereignisse gegen das
//! Overlay bei 1920 × 1080.

use planora_app::sketch_state::{Item, SketchSession, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{fonts, sketch_view, theme};
use planora_feature::SketchPlane;
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;

const R: f64 = 0.5;
const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(W, H),
};

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn sketch_with(build: impl FnOnce(&mut Sketch)) -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        build(sk);
        assert_eq!(sk.solve().status, SolveStatus::Solved);
    }
    s.observe_sketch(false);
    s
}

fn line(sk: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let p = sk.add_point(a.0, a.1).unwrap();
    let q = sk.add_point(b.0, b.1).unwrap();
    sk.add_line(p, q).unwrap()
}

fn ends(sk: &Sketch, c: CurveId) -> (PointId, PointId) {
    match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        _ => panic!("keine Linie"),
    }
}

fn curves(s: &AppState) -> Vec<CurveId> {
    sk(s).curves().map(|(c, _)| c).collect()
}

fn sk(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn se(s: &AppState) -> &SketchSession {
    s.sketch_session.as_ref().unwrap()
}

fn begin(s: &mut AppState, at: DVec2) -> bool {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.begin_drag(sk, at, R)
}

fn drag_to(s: &mut AppState, at: DVec2) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.drag_to(sk, at);
}

fn end(s: &mut AppState) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.end_drag(sk);
}

/// Alle Werte der Skizze als Bits: „bitgleich“ heißt gleich in jedem davon.
fn bits(s: &AppState) -> Vec<u64> {
    sk(s).variables().iter().map(|v| v.to_bits()).collect()
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

fn message(s: &AppState) -> Option<String> {
    se(s).message_text()
}

fn blocked() -> Option<String> {
    Some(planora_i18n::tr!("sketch-drag-blocked"))
}

fn status(s: &AppState) -> SolveStatus {
    se(s).last_report.as_ref().unwrap().status
}

#[track_caller]
fn assert_near(got: DVec2, want: DVec2) {
    assert!(got.distance(want) < 1e-6, "{got:?} statt {want:?}");
}

/// Eine Stelle auf der Kurve, weit genug von jedem Punkt, dass sie die Kurve greift.
fn grip_on(sketch: &Sketch, c: CurveId) -> DVec2 {
    sketch
        .curve_polyline(c, 96)
        .unwrap()
        .into_iter()
        .find(|&p| sketch.pick_point(p, 2.0 * R).is_none())
        .expect("die Kurve hat eine Stelle ohne Punkt")
}

// ---- Eine Kurve ---------------------------------------------------------------------------

#[test]
fn a_free_line_follows_the_grip_without_a_jump() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
    });
    let c = curves(&s)[0];
    let (a, b) = ends(sk(&s), c);
    let before = bits(&s);
    let revision = sk(&s).revision();
    // Neben der Linie gegriffen: Der Griffversatz bleibt, nichts springt auf den Zeiger.
    let grab = v(10.0, 0.25);
    assert!(begin(&mut s, grab));
    assert!(se(&s).is_dragging());
    assert!(se(&s).dragging.is_none(), "kein Punkt gegriffen");
    assert_eq!(se(&s).selection, vec![Item::Curve(c)]);
    drag_to(&mut s, grab);
    assert_eq!(bits(&s), before, "erstes Bild ohne Bewegung");
    assert_eq!(sk(&s).revision(), revision, "kein neuer Stand");

    drag_to(&mut s, grab + v(10.0, 0.0));
    assert_eq!(sk(&s).point_xy(a).unwrap(), v(10.0, 0.0));
    assert_eq!(sk(&s).point_xy(b).unwrap(), v(30.0, 0.0));
    drag_to(&mut s, grab + v(-2.0, 5.0));
    assert_eq!(sk(&s).point_xy(a).unwrap(), v(-2.0, 5.0));
    assert_eq!(sk(&s).point_xy(b).unwrap(), v(18.0, 5.0));
    end(&mut s);
    assert!(!se(&s).is_dragging());
    assert!(se(&s).curve_drag.is_none());
}

#[test]
fn arc_ellipse_spline_and_elliptic_arc_keep_their_shape() {
    let build: [fn(&mut Sketch); 4] = [
        |sk| {
            let m = sk.add_point(0.0, 0.0).unwrap();
            let a = sk.add_point(10.0, 0.0).unwrap();
            let b = sk.add_point(0.0, 10.0).unwrap();
            sk.add_arc(m, a, b).unwrap();
        },
        |sk| {
            let m = sk.add_point(1.0, 2.0).unwrap();
            sk.add_ellipse(m, v(8.0, 6.0), 4.0).unwrap();
        },
        |sk| {
            let p: Vec<PointId> = [(0.0, 0.0), (10.0, 8.0), (20.0, 0.0), (30.0, 8.0)]
                .iter()
                .map(|q| sk.add_point(q.0, q.1).unwrap())
                .collect();
            sk.add_spline(&p).unwrap();
        },
        |sk| {
            let m = sk.add_point(0.0, 0.0).unwrap();
            let a = sk.add_point(10.0, 0.0).unwrap();
            let b = sk.add_point(0.0, 5.0).unwrap();
            sk.add_elliptic_arc(m, v(10.0, 0.0), 5.0, a, b).unwrap();
        },
    ];
    for b in build {
        let mut s = sketch_with(b);
        let c = curves(&s)[0];
        let shape = sk(&s).curve_polyline(c, 96).unwrap();
        let grab = grip_on(sk(&s), c);
        let delta = v(7.0, -3.0);
        assert!(begin(&mut s, grab));
        assert!(
            se(&s).curve_drag.as_ref().unwrap().rim.is_none(),
            "nur der Kreis zieht am Rand den Radius"
        );
        drag_to(&mut s, grab + delta);
        end(&mut s);
        assert_eq!(message(&s), None);
        let moved = sk(&s).curve_polyline(c, 96).unwrap();
        assert_eq!(moved.len(), shape.len());
        for (m, o) in moved.iter().zip(&shape) {
            assert_near(*m, *o + delta);
        }
        assert_eq!(status(&s), SolveStatus::Solved);
    }
}

// ---- Kreis ---------------------------------------------------------------------------------

fn circle() -> AppState {
    sketch_with(|sk| {
        let m = sk.add_point(3.0, 4.0).unwrap();
        sk.add_circle(m, 10.0).unwrap();
    })
}

#[test]
fn the_rim_of_a_circle_changes_the_radius_with_the_grip_offset() {
    let mut s = circle();
    let c = curves(&s)[0];
    let m = v(3.0, 4.0);
    let before = bits(&s);
    // 0,2 außerhalb des Rands gegriffen.
    let grab = m + v(10.2, 0.0);
    assert!(begin(&mut s, grab));
    let drag = se(&s).curve_drag.clone().unwrap();
    let (rim, offset) = drag.rim.unwrap();
    assert_eq!(rim, c);
    assert!((offset - 0.2).abs() < 1e-9, "{offset}");
    drag_to(&mut s, grab);
    assert_eq!(bits(&s), before, "ohne Bewegung kein Sprung");
    drag_to(&mut s, m + v(0.0, 15.2));
    assert!((sk(&s).radius(c).unwrap() - 15.0).abs() < 1e-9);
    drag_to(&mut s, m + v(-6.2, 0.0));
    assert!((sk(&s).radius(c).unwrap() - 6.0).abs() < 1e-9);
    end(&mut s);
    let center = sk(&s).points().next().unwrap().0;
    assert_eq!(sk(&s).point_xy(center).unwrap(), m, "die Mitte bleibt");
}

#[test]
fn a_radius_dimension_holds_and_no_message_appears() {
    let mut s = sketch_with(|sk| {
        let m = sk.add_point(0.0, 0.0).unwrap();
        let c = sk.add_circle(m, 10.0).unwrap();
        sk.add_constraint(K::Radius(c, 10.0)).unwrap();
    });
    let c = curves(&s)[0];
    let before = bits(&s);
    let revision = sk(&s).revision();
    let steps = undo_len(&s);
    assert!(begin(&mut s, v(10.0, 0.0)));
    for x in [11.0, 13.0, 20.0] {
        drag_to(&mut s, v(x, 0.0));
        assert_eq!(message(&s), None, "gelöst: keine Meldung");
        s.observe_sketch(true);
    }
    end(&mut s);
    s.observe_sketch(false);
    assert_eq!(sk(&s).radius(c), Some(10.0));
    assert_eq!(bits(&s), before);
    assert_eq!(sk(&s).revision(), revision);
    assert_eq!(undo_len(&s), steps, "nichts bewegt: kein Schritt");
    assert!(se(&s).edit_label.is_none());
}

#[test]
fn dragging_through_the_center_stops_at_the_last_valid_radius() {
    let mut s = circle();
    let c = curves(&s)[0];
    let m = v(3.0, 4.0);
    assert!(begin(&mut s, m + v(10.0, 0.0)));
    drag_to(&mut s, m + v(4.0, 0.0));
    assert!((sk(&s).radius(c).unwrap() - 4.0).abs() < 1e-9);
    assert_eq!(message(&s), None);
    drag_to(&mut s, m);
    assert!(
        (sk(&s).radius(c).unwrap() - 4.0).abs() < 1e-9,
        "Stand davor"
    );
    assert_eq!(message(&s), blocked());
    drag_to(&mut s, m + v(7.0, 0.0));
    assert!((sk(&s).radius(c).unwrap() - 7.0).abs() < 1e-9);
    assert_eq!(message(&s), None, "wieder möglich: Meldung weg");
    end(&mut s);
}

#[test]
fn the_center_of_a_circle_is_grabbed_as_a_point() {
    let mut s = circle();
    let c = curves(&s)[0];
    let center = sk(&s).points().next().unwrap().0;
    assert!(begin(&mut s, v(3.0, 4.0)));
    assert_eq!(se(&s).dragging, Some(center));
    assert!(se(&s).curve_drag.is_none());
    assert!(se(&s).is_dragging());
    drag_to(&mut s, v(5.0, 5.0));
    end(&mut s);
    assert_eq!(sk(&s).point_xy(center).unwrap(), v(5.0, 5.0));
    assert_eq!(sk(&s).radius(c), Some(10.0));
}

// ---- Abhängigkeiten ------------------------------------------------------------------------

#[test]
fn a_horizontal_line_stays_horizontal() {
    let mut s = sketch_with(|sk| {
        let c = line(sk, (0.0, 0.0), (20.0, 0.0));
        sk.add_constraint(K::Horizontal(LineOrPoints::Line(c)))
            .unwrap();
    });
    let (a, b) = ends(sk(&s), curves(&s)[0]);
    assert!(begin(&mut s, v(10.0, 0.0)));
    drag_to(&mut s, v(15.0, 7.0));
    assert_eq!(message(&s), None);
    end(&mut s);
    let (pa, pb) = (sk(&s).point_xy(a).unwrap(), sk(&s).point_xy(b).unwrap());
    assert_near(pa, v(5.0, 7.0));
    assert_near(pb, v(25.0, 7.0));
    assert!((pa.y - pb.y).abs() < 1e-6);
    assert_eq!(status(&s), SolveStatus::Solved);
}

#[test]
fn a_line_fixed_at_one_end_turns_about_it() {
    let mut s = sketch_with(|sk| {
        let c = line(sk, (0.0, 0.0), (20.0, 0.0));
        let (a, b) = ends(sk, c);
        sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        sk.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    });
    let (a, b) = ends(sk(&s), curves(&s)[0]);
    assert!(begin(&mut s, v(10.0, 0.0)));
    drag_to(&mut s, v(10.0, 8.0));
    assert_eq!(message(&s), None);
    end(&mut s);
    let (pa, pb) = (sk(&s).point_xy(a).unwrap(), sk(&s).point_xy(b).unwrap());
    assert_near(pa, DVec2::ZERO);
    assert!((pb.length() - 20.0).abs() < 1e-6, "{pb:?}");
    assert!(pb.y > 1.0, "gedreht: {pb:?}");
}

#[test]
fn a_fully_determined_line_stays_bit_for_bit_and_makes_no_step() {
    let mut s = sketch_with(|sk| {
        let c = line(sk, (0.0, 0.0), (20.0, 0.0));
        let (a, b) = ends(sk, c);
        sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        sk.add_constraint(K::Fixed(b, 20.0, 0.0)).unwrap();
    });
    let before = bits(&s);
    let revision = sk(&s).revision();
    let steps = undo_len(&s);
    assert!(begin(&mut s, v(10.0, 0.0)));
    for y in [3.0, 8.0, -20.0] {
        drag_to(&mut s, v(10.0, y));
        assert_eq!(bits(&s), before);
        assert_eq!(message(&s), None);
        s.observe_sketch(true);
    }
    end(&mut s);
    s.observe_sketch(false);
    assert_eq!(bits(&s), before);
    assert_eq!(sk(&s).revision(), revision);
    assert_eq!(undo_len(&s), steps);
    assert!(se(&s).edit_label.is_none());
}

/// Zwei Punkte, die zugleich 3 und 5 auseinander sein sollen; schon vor dem Zug da.
fn add_conflict(s: &mut AppState) {
    let sk = s.session_sketch_mut().unwrap().1;
    let c = sk.add_point(40.0, 0.0).unwrap();
    let d = sk.add_point(44.0, 0.0).unwrap();
    sk.add_constraint(K::Distance(c, d, 3.0)).unwrap();
    sk.add_constraint(K::Distance(c, d, 5.0)).unwrap();
    let report = sk.solve();
    assert_eq!(report.status, SolveStatus::Conflicting);
    assert!(!report.conflicting.is_empty());
    s.observe_sketch(false);
}

#[test]
fn an_unrelated_conflict_does_not_block_the_drag() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
    });
    add_conflict(&mut s);
    let (a, b) = ends(sk(&s), curves(&s)[0]);
    let conflict: Vec<u64> = sk(&s).variables()[4..]
        .iter()
        .map(|v| v.to_bits())
        .collect();
    let expected = sk(&s).clone().solve().conflicting;
    let steps = undo_len(&s);
    assert!(begin(&mut s, v(10.0, 0.0)));
    for (i, y) in [2.0, 5.0, 9.0].into_iter().enumerate() {
        drag_to(&mut s, v(10.0, y));
        assert_eq!(message(&s), None, "Bild {i}");
        assert_eq!(sk(&s).point_xy(a).unwrap(), v(0.0, y));
        assert_eq!(sk(&s).point_xy(b).unwrap(), v(20.0, y));
        // Inhalt und Reihenfolge bleiben über die Bilder gleich.
        let report = se(&s).last_report.as_ref().unwrap();
        assert_eq!(report.conflicting, expected, "Bild {i}");
        assert_eq!(report.status, SolveStatus::Conflicting);
        s.observe_sketch(true);
    }
    end(&mut s);
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps + 1);
    let after: Vec<u64> = sk(&s).variables()[4..]
        .iter()
        .map(|v| v.to_bits())
        .collect();
    assert_eq!(after, conflict, "der Widerspruch bleibt, wie er war");
    assert_eq!(se(&s).last_report.as_ref().unwrap().conflicting, expected);
}

/// Linie b–c hängt am festen b, Linie c–d ist frei; zieht man c–d so, dass c auf b liegt, entartet
/// b–c, und die Gruppe von b lässt sich nicht lösen.
fn collapsing_line(s: &mut AppState) -> (CurveId, PointId) {
    let sk = s.session_sketch_mut().unwrap().1;
    let b = sk.add_point(0.0, 0.0).unwrap();
    let c = sk.add_point(5.0, 5.0).unwrap();
    let d = sk.add_point(25.0, 5.0).unwrap();
    sk.add_constraint(K::Fixed(b, 0.0, 0.0)).unwrap();
    sk.add_line(b, c).unwrap();
    let dragged = sk.add_line(c, d).unwrap();
    sk.solve();
    s.observe_sketch(false);
    (dragged, c)
}

#[test]
fn a_failure_in_the_dragged_geometry_keeps_the_state_before_the_frame() {
    let mut s = sketch_with(|_| {});
    let (_, c) = collapsing_line(&mut s);
    assert!(begin(&mut s, v(15.0, 5.0)));
    drag_to(&mut s, v(16.0, 6.0));
    assert_eq!(message(&s), None);
    let before = bits(&s);
    let revision = sk(&s).revision();
    drag_to(&mut s, v(10.0, 0.0));
    assert_eq!(bits(&s), before, "Stand vor diesem Bild");
    assert_eq!(sk(&s).revision(), revision);
    assert_eq!(message(&s), blocked());
    drag_to(&mut s, v(17.0, 7.0));
    assert_eq!(message(&s), None, "wieder möglich");
    assert_eq!(sk(&s).point_xy(c).unwrap(), v(7.0, 7.0));
    end(&mut s);
    assert_eq!(status(&s), SolveStatus::Solved);
}

/// Auch neben einem fremden Widerspruch: Der Status war schon vorher schlecht, und die Entartung
/// nennt keine neue Abhängigkeit; trotzdem scheitert die gezogene Gruppe.
#[test]
fn a_failure_in_the_dragged_geometry_blocks_beside_an_unrelated_conflict_too() {
    let mut s = sketch_with(|_| {});
    let (_, c) = collapsing_line(&mut s);
    add_conflict(&mut s);
    assert!(begin(&mut s, v(15.0, 5.0)));
    drag_to(&mut s, v(16.0, 6.0));
    assert_eq!(message(&s), None);
    assert_eq!(sk(&s).point_xy(c).unwrap(), v(6.0, 6.0));
    drag_to(&mut s, v(10.0, 0.0));
    assert_eq!(sk(&s).point_xy(c).unwrap(), v(6.0, 6.0));
    assert_eq!(message(&s), blocked());
    end(&mut s);
}

#[test]
fn a_conflict_inside_the_dragged_group_keeps_the_state_in_every_frame() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
    });
    {
        let sk = s.session_sketch_mut().unwrap().1;
        let (a, b) = ends(sk, sk.curves().next().unwrap().0);
        sk.add_constraint(K::Distance(a, b, 3.0)).unwrap();
        sk.add_constraint(K::Distance(a, b, 5.0)).unwrap();
        assert_eq!(sk.solve().status, SolveStatus::Conflicting);
    }
    s.observe_sketch(false);
    let before = bits(&s);
    let revision = sk(&s).revision();
    let steps = undo_len(&s);
    assert!(begin(&mut s, v(10.0, 0.0)));
    for y in [2.0, 5.0, 9.0] {
        drag_to(&mut s, v(10.0, y));
        assert_eq!(bits(&s), before);
        assert_eq!(message(&s), blocked());
    }
    end(&mut s);
    s.observe_sketch(false);
    assert_eq!(sk(&s).revision(), revision);
    assert_eq!(undo_len(&s), steps);
}

// ---- Hin und zurück -----------------------------------------------------------------------

#[test]
fn dragging_away_and_exactly_back_makes_no_step() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
    });
    let before = bits(&s);
    let revision = sk(&s).revision();
    let steps = undo_len(&s);
    let label = s.sketch_undo_label();
    let grab = v(10.0, 0.0);
    assert!(begin(&mut s, grab));
    drag_to(&mut s, v(14.0, 9.0));
    s.observe_sketch(true);
    assert_ne!(bits(&s), before);
    drag_to(&mut s, grab);
    s.observe_sketch(true);
    end(&mut s);
    s.observe_sketch(false);
    assert_eq!(bits(&s), before);
    assert_eq!(sk(&s).revision(), revision, "wieder der Stand von vorher");
    assert_eq!(undo_len(&s), steps);
    assert!(se(&s).edit_label.is_none());
    assert_eq!(s.sketch_undo_label(), label);
}

#[test]
fn a_constrained_curve_dragged_back_restores_the_exact_values() {
    let mut s = sketch_with(|sk| {
        let c = line(sk, (0.0, 0.0), (20.0, 0.0));
        let (a, b) = ends(sk, c);
        sk.add_constraint(K::Distance(a, b, 20.0)).unwrap();
        sk.add_constraint(K::Horizontal(LineOrPoints::Line(c)))
            .unwrap();
    });
    let before = bits(&s);
    let steps = undo_len(&s);
    assert!(begin(&mut s, v(10.0, 0.0)));
    drag_to(&mut s, v(14.0, 9.0));
    drag_to(&mut s, v(10.0, 0.0));
    end(&mut s);
    s.observe_sketch(false);
    assert_eq!(bits(&s), before);
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn a_change_made_during_the_drag_survives_dragging_back() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
    });
    let c = curves(&s)[0];
    let grab = v(10.0, 0.0);
    assert!(begin(&mut s, grab));
    drag_to(&mut s, v(14.0, 9.0));
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.toggle_construction(sk);
    }
    drag_to(&mut s, grab);
    end(&mut s);
    assert!(sk(&s).curve(c).unwrap().construction);
    // Dasselbe, wenn die Änderung erst nach dem letzten Bild kommt.
    assert!(begin(&mut s, grab));
    drag_to(&mut s, v(14.0, 9.0));
    drag_to(&mut s, grab);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.toggle_construction(sk);
    }
    end(&mut s);
    assert!(!sk(&s).curve(c).unwrap().construction);
}

// ---- Auswahl -------------------------------------------------------------------------------

#[test]
fn selected_curves_and_points_move_together() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
        let m = sk.add_point(0.0, 30.0).unwrap();
        sk.add_circle(m, 5.0).unwrap();
        sk.add_point(40.0, 40.0).unwrap();
        line(sk, (50.0, 0.0), (70.0, 0.0));
    });
    let c = curves(&s);
    let lone = sk(&s).pick_point(v(40.0, 40.0), R).expect("freier Punkt");
    let items = vec![Item::Curve(c[0]), Item::Curve(c[1]), Item::Point(lone)];
    s.sketch_session.as_mut().unwrap().selection = items.clone();
    let other = sk(&s).curve_polyline(c[2], 96).unwrap();

    // Der Kreis ist nicht allein gewählt: Er wandert als Ganzes, sein Radius bleibt.
    assert!(begin(&mut s, v(5.0, 30.0)));
    assert!(se(&s).curve_drag.as_ref().unwrap().rim.is_none());
    assert_eq!(se(&s).selection, items, "gewählt: Die Auswahl bleibt");
    drag_to(&mut s, v(8.0, 26.0));
    end(&mut s);
    let (a, b) = ends(sk(&s), c[0]);
    assert_eq!(sk(&s).point_xy(a).unwrap(), v(3.0, -4.0));
    assert_eq!(sk(&s).point_xy(b).unwrap(), v(23.0, -4.0));
    assert_eq!(sk(&s).point_xy(lone).unwrap(), v(43.0, 36.0));
    assert_eq!(sk(&s).radius(c[1]), Some(5.0));
    assert_eq!(
        sk(&s).curve_polyline(c[2], 96).unwrap(),
        other,
        "nicht gewählt"
    );
}

#[test]
fn a_curve_outside_the_selection_replaces_it() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
        line(sk, (0.0, 10.0), (20.0, 10.0));
    });
    let c = curves(&s);
    s.sketch_session.as_mut().unwrap().selection = vec![Item::Curve(c[0])];
    let first = sk(&s).curve_polyline(c[0], 96).unwrap();
    assert!(begin(&mut s, v(10.0, 10.0)));
    assert_eq!(se(&s).selection, vec![Item::Curve(c[1])]);
    drag_to(&mut s, v(10.0, 15.0));
    end(&mut s);
    assert_eq!(sk(&s).curve_polyline(c[0], 96).unwrap(), first);
    let (a, _) = ends(sk(&s), c[1]);
    assert_eq!(sk(&s).point_xy(a).unwrap(), v(0.0, 15.0));
}

#[test]
fn a_shared_point_moves_only_once() {
    let mut s = sketch_with(|sk| {
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(10.0, 0.0).unwrap();
        let d = sk.add_point(10.0, 10.0).unwrap();
        sk.add_line(a, b).unwrap();
        sk.add_line(b, d).unwrap();
    });
    let c = curves(&s);
    s.sketch_session.as_mut().unwrap().selection = vec![Item::Curve(c[0]), Item::Curve(c[1])];
    assert!(begin(&mut s, v(5.0, 0.0)));
    let start = se(&s).curve_drag.as_ref().unwrap().start.clone();
    assert_eq!(start.len(), 3, "{start:?}");
    let (_, b) = ends(sk(&s), c[0]);
    drag_to(&mut s, v(6.0, 2.0));
    end(&mut s);
    assert_eq!(sk(&s).point_xy(b).unwrap(), v(11.0, 2.0));
}

/// Beide fragen dasselbe: Wo ein Zug greift, beginnt kein Rahmen, und umgekehrt.
#[test]
fn begin_drag_and_is_free_at_agree_everywhere() {
    let s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
        let m = sk.add_point(0.0, 30.0).unwrap();
        sk.add_circle(m, 5.0).unwrap();
    });
    let (mut grabbed, mut free) = (0, 0);
    for ix in -20..=60 {
        for iy in -20..=80 {
            let at = v(f64::from(ix) * 0.5, f64::from(iy) * 0.5);
            let mut t = s.sketch_session.clone().unwrap();
            let mut sketch = sk(&s).clone();
            let is_free = t.is_free_at(&sketch, at, R);
            let begun = t.begin_drag(&mut sketch, at, R);
            assert_eq!(begun, !is_free, "bei {at:?}");
            if begun {
                grabbed += 1;
            } else {
                free += 1;
            }
        }
    }
    assert!(grabbed > 0 && free > 0);
}

#[test]
fn begin_drag_works_only_in_the_select_tool() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
    });
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
    }
    assert!(!begin(&mut s, v(10.0, 0.0)));
    assert!(!se(&s).is_dragging());
}

// ---- Abbruch während des Zugs -----------------------------------------------------------------

#[test]
fn deleting_the_dragged_curve_ends_the_drag() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
    });
    assert!(begin(&mut s, v(10.0, 0.0)));
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.delete_selection(sk);
    }
    assert!(!se(&s).is_dragging());
}

#[test]
fn deleting_one_side_of_a_triangle_ends_the_drag() {
    let mut s = sketch_with(|sk| {
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(20.0, 0.0).unwrap();
        let c = sk.add_point(10.0, 15.0).unwrap();
        sk.add_line(a, b).unwrap();
        sk.add_line(b, c).unwrap();
        sk.add_line(c, a).unwrap();
    });
    let sides = curves(&s);
    assert!(begin(&mut s, v(10.0, 0.0)));
    drag_to(&mut s, v(10.0, 2.0));
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        // Beide Enden der Seite leben in den anderen Seiten weiter.
        se.delete_selection(sk);
    }
    assert!(!se(&s).is_dragging());
    assert_eq!(curves(&s), sides[1..].to_vec());
    let before = bits(&s);
    drag_to(&mut s, v(10.0, 9.0));
    assert_eq!(bits(&s), before, "kein Zug mehr");
}

#[test]
fn undo_during_the_drag_ends_it() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
    });
    let before = bits(&s);
    assert!(begin(&mut s, v(10.0, 0.0)));
    drag_to(&mut s, v(10.0, 5.0));
    assert!(s.undo_sketch());
    assert!(!se(&s).is_dragging());
    assert_eq!(bits(&s), before);
}

#[test]
fn switching_the_tool_ends_the_drag_with_one_step() {
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
    });
    let steps = undo_len(&s);
    let c = curves(&s)[0];
    assert!(begin(&mut s, v(10.0, 0.0)));
    drag_to(&mut s, v(10.0, 5.0));
    s.observe_sketch(true);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
    }
    assert!(!se(&s).is_dragging());
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps + 1);
    assert_eq!(
        s.sketch_history().unwrap().undo_label(),
        Some("Kurven verschieben")
    );
    let (a, _) = ends(sk(&s), c);
    assert_eq!(sk(&s).point_xy(a).unwrap(), v(0.0, 5.0), "der Stand bleibt");
}

// ---- Über das Overlay ------------------------------------------------------------------------

type Frame = Vec<egui::Event>;

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn screen(s: &AppState, p: (f64, f64)) -> egui::Pos2 {
    let frame = *sk(s).frame();
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    let q =
        sketch_view::sketch_to_screen(&cam, &frame, glam::Vec2::new(W, H), v(p.0, p.1)).unwrap();
    egui::pos2(q.x, q.y)
}

fn moved(p: egui::Pos2) -> egui::Event {
    egui::Event::PointerMoved(p)
}

fn button(p: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

/// Ein Bild; danach schreibt es den Verlauf fort wie die App. Liefert die gezeichneten Formen.
fn frame(ctx: &egui::Context, s: &mut AppState, events: Frame) -> Vec<egui::Shape> {
    let raw = egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    };
    let mut gesture = false;
    let out = ctx.run_ui(raw, |ui| {
        sketch_view::show_overlay(ui, SCREEN, s);
        gesture = ui.input(|i| i.pointer.any_down());
    });
    let shapes = out.shapes.iter().map(|c| c.shape.clone()).collect();
    out.drop_without_applying_deltas();
    s.observe_sketch(gesture);
    shapes
}

/// Ein leeres Bild vorweg: egui ordnet Zeiger-Ereignisse den Flächen des vorigen Bilds zu.
fn play(ctx: &egui::Context, s: &mut AppState, frames: Vec<Frame>) {
    frame(ctx, s, vec![]);
    for events in frames {
        frame(ctx, s, events);
    }
}

/// Rechteck 40 × 20 mit Breite, Höhe und waagerechten und senkrechten Seiten; die Formel steht an
/// der Höhe.
fn rectangle() -> AppState {
    let mut s = sketch_with(|sk| {
        let p: Vec<PointId> = [(0.0, 0.0), (40.0, 0.0), (40.0, 20.0), (0.0, 20.0)]
            .iter()
            .map(|q| sk.add_point(q.0, q.1).unwrap())
            .collect();
        for i in 0..4 {
            let c = sk.add_line(p[i], p[(i + 1) % 4]).unwrap();
            let side = LineOrPoints::Line(c);
            sk.add_constraint(if i % 2 == 0 {
                K::Horizontal(side)
            } else {
                K::Vertical(side)
            })
            .unwrap();
        }
        sk.add_constraint(K::Distance(p[0], p[1], 40.0)).unwrap();
        sk.add_constraint(K::Distance(p[1], p[2], 20.0)).unwrap();
    });
    let height = sk(&s).constraints().last().unwrap().0;
    s.session_draft_mut()
        .unwrap()
        .1
        .formulas
        .insert(height, "20".to_string());
    s.observe_sketch(false);
    s
}

#[test]
fn dragging_a_dimensioned_rectangle_is_one_step_with_exact_undo_and_redo() {
    let ctx = context();
    let mut s = rectangle();
    let c = curves(&s);
    s.sketch_session.as_mut().unwrap().selection = c.iter().map(|&c| Item::Curve(c)).collect();
    let before = bits(&s);
    let steps = undo_len(&s);
    let formulas = s.sketch_draft.as_ref().unwrap().formulas.clone();
    assert_eq!(formulas.len(), 1);
    let (from, to) = (screen(&s, (20.0, 0.0)), screen(&s, (32.0, 9.0)));
    play(
        &ctx,
        &mut s,
        vec![vec![moved(from)], vec![button(from, true)]],
    );
    for t in [0.25, 0.5, 0.75, 1.0] {
        frame(&ctx, &mut s, vec![moved(from.lerp(to, t))]);
        assert!(se(&s).is_dragging());
        assert_eq!(undo_len(&s), steps, "während des Zugs kein Schritt");
    }
    frame(&ctx, &mut s, vec![button(to, false)]);
    frame(&ctx, &mut s, vec![]);
    assert!(!se(&s).is_dragging());
    assert_eq!(undo_len(&s), steps + 1, "genau ein Schritt");
    assert_eq!(message(&s), None);
    assert_eq!(status(&s), SolveStatus::Solved);
    assert_eq!(s.sketch_draft.as_ref().unwrap().formulas, formulas);

    let (a, b) = ends(sk(&s), c[0]);
    let (pa, pb) = (sk(&s).point_xy(a).unwrap(), sk(&s).point_xy(b).unwrap());
    assert!((pb - pa - v(40.0, 0.0)).length() < 1e-6, "formtreu");
    assert!(
        (pa - v(12.0, 9.0)).length() < 0.2,
        "folgt dem Zeiger: {pa:?}"
    );
    let after = bits(&s);

    assert_eq!(s.sketch_undo_label().as_deref(), Some("Kurven verschieben"));
    assert!(s.undo_sketch());
    assert_eq!(bits(&s), before, "alle Koordinaten bitgleich");
    assert_eq!(s.sketch_redo_label().as_deref(), Some("Kurven verschieben"));
    assert!(s.redo_sketch());
    assert_eq!(bits(&s), after);
    assert_eq!(s.sketch_draft.as_ref().unwrap().formulas, formulas);
}

#[test]
fn esc_during_the_drag_keeps_the_state_with_one_step() {
    let ctx = context();
    let mut s = sketch_with(|sk| {
        line(sk, (-20.0, 0.0), (20.0, 0.0));
    });
    let (a, _) = ends(sk(&s), curves(&s)[0]);
    let steps = undo_len(&s);
    let (from, to) = (screen(&s, (0.0, 0.0)), screen(&s, (0.0, 12.0)));
    let esc = egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    };
    play(
        &ctx,
        &mut s,
        vec![
            vec![moved(from)],
            vec![button(from, true)],
            vec![moved(from.lerp(to, 0.5))],
            vec![moved(to)],
            vec![esc],
        ],
    );
    assert!(!se(&s).is_dragging());
    assert_eq!(undo_len(&s), steps, "die Taste ist noch unten");
    let kept = sk(&s).point_xy(a).unwrap();
    assert!(kept.y > 1.0, "{kept:?}");
    play(&ctx, &mut s, vec![vec![button(to, false)], vec![]]);
    assert_eq!(undo_len(&s), steps + 1);
    assert_eq!(sk(&s).point_xy(a).unwrap(), kept, "der Stand bleibt");
    assert_eq!(
        s.sketch_history().unwrap().undo_label(),
        Some("Kurven verschieben")
    );
}

#[test]
fn a_click_on_a_curve_selects_and_changes_nothing() {
    let ctx = context();
    let mut s = sketch_with(|sk| {
        line(sk, (-20.0, 0.0), (20.0, 0.0));
    });
    let c = curves(&s)[0];
    let before = bits(&s);
    let revision = sk(&s).revision();
    let steps = undo_len(&s);
    let at = screen(&s, (0.0, 0.0));
    play(
        &ctx,
        &mut s,
        vec![
            vec![moved(at)],
            vec![button(at, true)],
            vec![button(at, false)],
            vec![],
        ],
    );
    assert_eq!(se(&s).selection, vec![Item::Curve(c)]);
    assert!(!se(&s).is_dragging());
    assert_eq!(bits(&s), before);
    assert_eq!(sk(&s).revision(), revision);
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn a_drag_on_a_curve_starts_a_curve_drag_and_no_box() {
    let ctx = context();
    let mut s = sketch_with(|sk| {
        line(sk, (-20.0, 0.0), (20.0, 0.0));
    });
    let (from, to) = (screen(&s, (0.0, 0.0)), screen(&s, (0.0, 12.0)));
    play(
        &ctx,
        &mut s,
        vec![vec![moved(from)], vec![button(from, true)], vec![moved(to)]],
    );
    assert!(se(&s).curve_drag.is_some());
    assert!(se(&s).box_select.is_none());
    play(&ctx, &mut s, vec![vec![button(to, false)]]);
    assert!(!se(&s).is_dragging());
}

/// Dreiecke in der Füllfarbe der Profile.
fn profile_triangles(shapes: &[egui::Shape], color: egui::Color32) -> usize {
    shapes
        .iter()
        .map(|s| match s {
            egui::Shape::Mesh(m) => m
                .indices
                .chunks(3)
                .filter(|t| m.vertices[t[0] as usize].color == color)
                .count(),
            egui::Shape::Vec(v) => profile_triangles(v, color),
            _ => 0,
        })
        .sum()
}

#[test]
fn profile_fills_are_gone_during_a_curve_drag() {
    let ctx = context();
    let mut s = rectangle();
    assert!(s.show_sketch_profiles);
    let fill = theme::sketch_colors(s.dark).profile_fill;
    frame(&ctx, &mut s, vec![]);
    let shapes = frame(&ctx, &mut s, vec![]);
    assert!(profile_triangles(&shapes, fill) > 0, "vor dem Zug gefüllt");
    let (from, to) = (screen(&s, (20.0, 0.0)), screen(&s, (24.0, 5.0)));
    frame(&ctx, &mut s, vec![moved(from)]);
    frame(&ctx, &mut s, vec![button(from, true)]);
    frame(&ctx, &mut s, vec![moved(from.lerp(to, 0.5))]);
    let shapes = frame(&ctx, &mut s, vec![moved(to)]);
    assert!(se(&s).curve_drag.is_some());
    assert_eq!(
        profile_triangles(&shapes, fill),
        0,
        "beim Ziehen keine Flächen"
    );
    frame(&ctx, &mut s, vec![button(to, false)]);
    let shapes = frame(&ctx, &mut s, vec![]);
    assert!(
        profile_triangles(&shapes, fill) > 0,
        "nach dem Zug wieder gefüllt"
    );
}
