//! Formen mit ihren Abhängigkeiten (Skizzen 3, Task 2): Freiheitsgrade, genau eine Fläche und
//! vor allem: Maß ändern hält die Form.
//! Messtest: `cargo test --release -p planora-sketch --test shapes_constraints -- --ignored`

use planora_math::{DVec2, Frame, PI, Tolerance};
use planora_sketch::ConstraintKind as K;
use planora_sketch::shapes::*;
use planora_sketch::{
    ArcSlotIds, CenterRectIds, ConstraintId, CurveId, CurveKind, Drag, LineOrPoints, PointId,
    PolygonIds, RectIds, Sketch, SlotIds, SlotStyle, SolveStatus,
};
use std::time::Instant;

const TOL: f64 = 1e-6;
const SLOTS: [SlotStyle; 2] = [SlotStyle::CenterToCenter, SlotStyle::CenterPoint];

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn at(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn ends(s: &Sketch, line: CurveId) -> (DVec2, DVec2) {
    match s.curve(line).unwrap().kind {
        CurveKind::Line { a, b } => (at(s, a), at(s, b)),
        ref k => panic!("keine Linie: {k:?}"),
    }
}

fn dir(s: &Sketch, line: CurveId) -> DVec2 {
    let (a, b) = ends(s, line);
    (b - a).normalize()
}

fn len(s: &Sketch, line: CurveId) -> f64 {
    let (a, b) = ends(s, line);
    a.distance(b)
}

fn arc_points(s: &Sketch, arc: CurveId) -> (DVec2, DVec2, DVec2) {
    match s.curve(arc).unwrap().kind {
        CurveKind::Arc { center, start, end } => (at(s, center), at(s, start), at(s, end)),
        ref k => panic!("kein Bogen: {k:?}"),
    }
}

#[track_caller]
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < TOL, "{a} != {b}");
}

#[track_caller]
fn close_v(a: DVec2, b: DVec2) {
    assert!(a.distance(b) < TOL, "{a:?} != {b:?}");
}

/// Keine Bewegung, nichts überzählig: so muss jede frisch angelegte Form dastehen.
#[track_caller]
fn assert_fresh(s: &mut Sketch, dof: usize) {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.redundant.is_empty(), "{r:?}");
    assert!(r.conflicting.is_empty(), "{r:?}");
    assert_eq!(r.iterations, 0, "{r:?}");
    assert_eq!(r.dof, dof, "{r:?}");
}

#[track_caller]
fn set(s: &mut Sketch, id: ConstraintId, value: f64) {
    s.set_constraint_value(id, value).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "Wert {value}: {r:?}");
    assert!(r.redundant.is_empty(), "Wert {value}: {r:?}");
}

/// Der Stand aller Punkte, um „bleibt, wie er war“ zu prüfen.
fn state(s: &Sketch) -> Vec<DVec2> {
    s.points().map(|(p, _)| at(s, p)).collect()
}

fn rect_3p() -> (Sketch, RectIds) {
    let (d, n) = (DVec2::from_angle(0.4), DVec2::from_angle(0.4).perp());
    let a = v(3.0, 4.0);
    let plan = rect_three_point(a, a + d * 30.0, a + n * 12.0, TOL).unwrap();
    let mut s = sketch();
    let ids = s.add_rect_three_point(plan).unwrap();
    (s, ids)
}

fn rect_mid() -> (Sketch, CenterRectIds) {
    let mut s = sketch();
    let ids = s.add_rect_center(v(5.0, -2.0), v(25.0, 8.0)).unwrap();
    (s, ids)
}

fn polygon(sides: usize) -> (Sketch, PolygonIds) {
    let plan = polygon_inscribed(v(7.0, 3.0), v(27.0, 11.0), sides, TOL).unwrap();
    let mut s = sketch();
    let ids = s.add_polygon(&plan).unwrap();
    (s, ids)
}

fn slot(style: SlotStyle) -> (Sketch, SlotIds) {
    let d = DVec2::from_angle(0.5);
    let a = v(2.0, 1.0);
    let plan = SlotPlan {
        a,
        b: a + d * 40.0,
        radius: 5.0,
    };
    let mut s = sketch();
    let ids = s.add_slot(&plan, style).unwrap();
    (s, ids)
}

fn arc_slot_shape() -> (Sketch, ArcSlotIds) {
    let center = v(4.0, -3.0);
    let axis = ArcPlan {
        center,
        start: center + DVec2::from_angle(0.3) * 60.0,
        end: center + DVec2::from_angle(0.3 + PI / 2.0) * 60.0,
    };
    let plan = ArcSlotPlan {
        axis,
        half_width: 5.0,
    };
    let mut s = sketch();
    let ids = s.add_arc_slot(&plan).unwrap();
    (s, ids)
}

#[test]
fn a_new_shape_is_solved_without_moving_and_has_the_planned_freedom() {
    assert_fresh(&mut rect_3p().0, 5);
    assert_fresh(&mut rect_mid().0, 4);
    for n in [3, 4, 5, 6, 8, 12, 64] {
        assert_fresh(&mut polygon(n).0, 4);
    }
    for style in SLOTS {
        assert_fresh(&mut slot(style).0, 5);
    }
    assert_fresh(&mut arc_slot_shape().0, 6);
}

#[test]
fn a_second_rectangle_hv_pair_would_be_redundant() {
    let (mut s, ids) = rect_mid();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(ids.rect.lines[2])))
        .unwrap();
    assert!(!s.solve().redundant.is_empty());
}

#[test]
fn every_shape_is_exactly_one_face_and_its_helpers_are_construction() {
    let (s, _) = rect_3p();
    assert_eq!(s.profiles().profiles.len(), 1);

    let (s, ids) = rect_mid();
    assert_eq!(s.profiles().profiles.len(), 1);
    for d in ids.diagonals {
        assert!(s.curve(d).unwrap().construction);
    }

    for n in [3, 4, 6, 64] {
        let (s, ids) = polygon(n);
        assert_eq!(s.profiles().profiles.len(), 1, "n = {n}");
        assert!(s.curve(ids.circle).unwrap().construction);
        assert!(ids.lines.iter().all(|&l| !s.curve(l).unwrap().construction));
        assert_eq!((ids.points.len(), ids.lines.len()), (n, n));
    }

    for style in SLOTS {
        let (s, ids) = slot(style);
        assert_eq!(s.profiles().profiles.len(), 1, "{style:?}");
        assert!(s.curve(ids.axis).unwrap().construction);
        assert_eq!(ids.mid.is_some(), style == SlotStyle::CenterPoint);
    }

    let (s, ids) = arc_slot_shape();
    assert_eq!(s.profiles().profiles.len(), 1);
    assert!(s.curve(ids.axis).unwrap().construction);
}

#[test]
fn slot_points_lie_where_the_plan_says() {
    let d = DVec2::from_angle(0.5);
    let a = v(2.0, 1.0);

    let (s, ids) = slot(SlotStyle::CenterPoint);
    close_v(at(&s, ids.centers[0]), a);
    close_v(at(&s, ids.centers[1]), a + d * 40.0);
    close_v(at(&s, ids.mid.unwrap()), a + d * 20.0);

    // Der Bogen läuft außen um das Ende: gegen den Uhrzeigersinn über die Spitze.
    for (cap, tip) in [(ids.caps[0], a - d * 5.0), (ids.caps[1], a + d * 45.0)] {
        let (center, start, end) = arc_points(&s, cap);
        close(ArcPlan { center, start, end }.sweep(), PI);
        close_v(center + (start - center).perp(), tip);
    }
}

#[test]
fn arc_slot_arcs_run_counter_clockwise_the_short_way() {
    let (s, ids) = arc_slot_shape();
    for (arc, sweep) in [
        (ids.axis, PI / 2.0),
        (ids.outer, PI / 2.0),
        (ids.inner, PI / 2.0),
        (ids.caps[0], PI),
        (ids.caps[1], PI),
    ] {
        let (center, start, end) = arc_points(&s, arc);
        close(ArcPlan { center, start, end }.sweep(), sweep);
    }
}

#[track_caller]
fn assert_rectangle(s: &Sketch, r: &RectIds) {
    close(dir(s, r.lines[0]).dot(dir(s, r.lines[1])), 0.0);
    close(len(s, r.lines[0]), len(s, r.lines[2]));
    close(len(s, r.lines[1]), len(s, r.lines[3]));
    close(dir(s, r.lines[0]).perp_dot(dir(s, r.lines[2])), 0.0);
    close(dir(s, r.lines[1]).perp_dot(dir(s, r.lines[3])), 0.0);
}

#[test]
fn three_point_rectangle_keeps_its_shape_when_dimensioned() {
    let (mut s, r) = rect_3p();
    let w = s
        .add_constraint(K::Distance(r.points[0], r.points[1], 30.0))
        .unwrap();
    let h = s
        .add_constraint(K::Distance(r.points[1], r.points[2], 12.0))
        .unwrap();
    for (id, line, value) in [
        (w, r.lines[0], 50.0),
        (w, r.lines[0], 7.0),
        (h, r.lines[1], 50.0),
        (h, r.lines[1], 7.0),
    ] {
        set(&mut s, id, value);
        close(len(&s, line), value);
        assert_rectangle(&s, &r);
    }
}

#[test]
fn center_rectangle_keeps_its_center_and_axes_when_dimensioned() {
    let (mut s, ids) = rect_mid();
    let r = &ids.rect;
    let w = s
        .add_constraint(K::Distance(r.points[0], r.points[1], 40.0))
        .unwrap();
    set(&mut s, w, 50.0);
    close(len(&s, r.lines[0]), 50.0);
    let m = at(&s, ids.center);
    close_v(m, (at(&s, r.points[0]) + at(&s, r.points[2])) * 0.5);
    close_v(m, (at(&s, r.points[1]) + at(&s, r.points[3])) * 0.5);
    for line in [r.lines[0], r.lines[2]] {
        close(dir(&s, line).y, 0.0);
    }
    for line in [r.lines[1], r.lines[3]] {
        close(dir(&s, line).x, 0.0);
    }
}

#[track_caller]
fn assert_regular(s: &Sketch, p: &PolygonIds) {
    let m = at(s, p.center);
    let r = s.radius(p.circle).unwrap();
    let side = len(s, p.lines[0]);
    let n = p.points.len();
    close(side, 2.0 * r * (PI / n as f64).sin());
    for i in 0..n {
        close(at(s, p.points[i]).distance(m), r);
        close(len(s, p.lines[i]), side);
    }
}

#[test]
fn polygon_stays_regular_and_in_place_when_dimensioned() {
    for n in [3, 4, 6, 64] {
        let (mut s, p) = polygon(n);
        let m0 = at(&s, p.center);
        let angle0 = (at(&s, p.points[0]) - m0).to_angle();
        let same_place = |s: &Sketch| {
            close_v(at(s, p.center), m0);
            close((at(s, p.points[0]) - m0).to_angle(), angle0);
        };

        let radius = s.radius(p.circle).unwrap();
        let id = s.add_constraint(K::Radius(p.circle, radius)).unwrap();
        for value in [200.0, 2.0, 37.0] {
            set(&mut s, id, value);
            close(s.radius(p.circle).unwrap(), value);
            assert_regular(&s, &p);
            same_place(&s);
        }
        s.remove_constraint(id).unwrap();

        let id = s
            .add_constraint(K::Distance(p.points[0], p.points[1], len(&s, p.lines[0])))
            .unwrap();
        set(&mut s, id, 33.0);
        close(len(&s, p.lines[0]), 33.0);
        assert_regular(&s, &p);
        same_place(&s);
        s.remove_constraint(id).unwrap();

        if n == 6 {
            let apothem =
                at(&s, p.center).distance((at(&s, p.points[0]) + at(&s, p.points[1])) * 0.5);
            let id = s
                .add_constraint(K::DistancePointLine(p.center, p.lines[0], apothem))
                .unwrap();
            set(&mut s, id, 10.0);
            close(len(&s, p.lines[0]), 20.0 / 3f64.sqrt());
            assert!((len(&s, p.lines[0]) - 11.547).abs() < 1e-3);
            assert_regular(&s, &p);
            same_place(&s);
        }
    }
}

#[test]
fn dragging_a_polygon_corner_keeps_it_regular() {
    for n in [3, 4, 6, 64] {
        let (mut s, p) = polygon(n);
        let before = at(&s, p.points[0]);
        let target = before + v(3.0, 4.0);
        let r = s
            .drag(&[Drag {
                point: p.points[0],
                target,
            }])
            .unwrap();
        assert!(!r.drag_failed, "n = {n}: {r:?}");
        assert_eq!(r.status, SolveStatus::Solved, "n = {n}: {r:?}");
        assert!(at(&s, p.points[0]).distance(before) > 1.0, "n = {n}");
        assert_regular(&s, &p);
    }
}

#[track_caller]
fn assert_slot(s: &Sketch, ids: &SlotIds, drawn_along: DVec2) {
    let (a, b) = (at(s, ids.centers[0]), at(s, ids.centers[1]));
    let axis = (b - a).normalize();
    // Nicht umgeklappt: Die Mitten liegen noch in der Richtung, in der die Form angelegt wurde.
    assert!(axis.dot(drawn_along) > 0.0, "umgeklappt: {axis:?}");
    let radius = s.radius(ids.caps[0]).unwrap();
    close(s.radius(ids.caps[1]).unwrap(), radius);
    for side in ids.sides {
        close(dir(s, side).perp_dot(axis), 0.0);
        // Tangential: Die Enden der Seite stehen im Radius-Abstand senkrecht neben den Mitten.
        let (p, q) = ends(s, side);
        let (near, far) = if p.distance(a) < q.distance(a) {
            (p, q)
        } else {
            (q, p)
        };
        close(axis.dot(near - a), 0.0);
        close(axis.dot(far - b), 0.0);
        close(near.distance(a), radius);
        close(far.distance(b), radius);
    }
    close(dir(s, ids.axis).perp_dot(axis), 0.0);
}

#[test]
fn slot_stays_tangent_when_dimensioned() {
    for style in SLOTS {
        let (mut s, ids) = slot(style);
        let id = s.add_constraint(K::Diameter(ids.caps[0], 10.0)).unwrap();
        for value in [30.0, 2.0] {
            set(&mut s, id, value);
            close(s.radius(ids.caps[0]).unwrap(), value / 2.0);
            assert_slot(&s, &ids, DVec2::from_angle(0.5));
        }
        s.remove_constraint(id).unwrap();

        let [p, q] = ids.centers;
        let start = at(&s, p).distance(at(&s, q));
        let id = s.add_constraint(K::Distance(p, q, start)).unwrap();
        for value in [100.0, 5.0] {
            set(&mut s, id, value);
            close(at(&s, p).distance(at(&s, q)), value);
            assert_slot(&s, &ids, DVec2::from_angle(0.5));
        }
        if let Some(mid) = ids.mid {
            let m = (at(&s, ids.centers[0]) + at(&s, ids.centers[1])) * 0.5;
            close_v(at(&s, mid), m);
        }
    }
}

/// Die Art „Gesamt“ gibt es nur bei der Eingabe: `slot_overall` macht aus den beiden äußeren
/// Klicks die Mitten und den Radius, die Skizze kennt nur das Langloch von Mitte zu Mitte.
#[test]
fn a_slot_from_two_outer_clicks_is_a_plain_slot_and_does_not_fold() {
    let plan = slot_overall(v(0.0, 0.0), v(40.0, 0.0), v(20.0, 6.0), TOL).unwrap();
    let mut s = sketch();
    let ids = s.add_slot(&plan, SlotStyle::CenterToCenter).unwrap();
    close_v(at(&s, ids.centers[0]), v(6.0, 0.0));
    close_v(at(&s, ids.centers[1]), v(34.0, 0.0));
    close(s.radius(ids.caps[0]).unwrap(), 6.0);
    assert_fresh(&mut s, 5);
    assert_eq!(s.profiles().profiles.len(), 1);

    let width = s.add_constraint(K::Diameter(ids.caps[0], 12.0)).unwrap();
    set(&mut s, width, 20.0);
    close(s.radius(ids.caps[0]).unwrap(), 10.0);
    assert_slot(&s, &ids, v(1.0, 0.0));
    assert_eq!(s.profiles().profiles.len(), 1);

    let [a, b] = ids.centers;
    let length = s.add_constraint(K::Distance(a, b, 28.0)).unwrap();
    set(&mut s, length, 10.0);
    close(at(&s, a).distance(at(&s, b)), 10.0);
    close(s.radius(ids.caps[0]).unwrap(), 10.0);
    assert_slot(&s, &ids, v(1.0, 0.0));
    assert_eq!(s.profiles().profiles.len(), 1);
}

#[track_caller]
fn assert_arc_slot(s: &Sketch, ids: &ArcSlotIds, big_r: f64, h: f64) {
    let m = at(s, ids.center);
    close(s.radius(ids.axis).unwrap(), big_r);
    close(s.radius(ids.outer).unwrap(), big_r + h);
    close(s.radius(ids.inner).unwrap(), big_r - h);
    for arc in [ids.axis, ids.outer, ids.inner] {
        close_v(arc_points(s, arc).0, m);
    }
    for (cap, end) in ids.caps.iter().zip(ids.ends) {
        close(s.radius(*cap).unwrap(), h);
        close_v(arc_points(s, *cap).0, at(s, end));
    }
}

#[test]
fn arc_slot_stays_concentric_when_dimensioned() {
    let (mut s, ids) = arc_slot_shape();
    let width = s.add_constraint(K::Diameter(ids.caps[0], 10.0)).unwrap();
    let radius = s.add_constraint(K::Radius(ids.axis, 60.0)).unwrap();
    for value in [20.0, 3.0] {
        set(&mut s, width, value);
        assert_arc_slot(&s, &ids, 60.0, value / 2.0);
    }
    for value in [120.0, 30.0] {
        set(&mut s, radius, value);
        assert_arc_slot(&s, &ids, value, 1.5);
    }
}

/// Achsenradius 60 mit der Breite `width`: ab 120 gäbe es keinen Innenradius mehr.
#[track_caller]
fn assert_arc_slot_width_refused(width: f64) {
    let (mut s, ids) = arc_slot_shape();
    s.add_constraint(K::Radius(ids.axis, 60.0)).unwrap();
    let id = s.add_constraint(K::Diameter(ids.caps[0], 10.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let before = state(&s);
    s.set_constraint_value(id, width).unwrap();
    assert_ne!(s.solve().status, SolveStatus::Solved, "Breite {width}");
    assert_eq!(state(&s), before);
}

#[test]
fn a_width_at_the_limit_is_not_solved_and_the_shape_stays() {
    assert_arc_slot_width_refused(120.0);
}

/// Das gerade Langloch hat keine Breitengrenze mehr, seine Grenze ist die Länge null: Null und
/// Werte unter der Toleranz lehnt schon die Wertprüfung des Maßes ab.
#[test]
fn a_slot_length_of_zero_is_refused_and_the_shape_stays() {
    for style in SLOTS {
        let (mut s, ids) = slot(style);
        let [p, q] = ids.centers;
        let id = s.add_constraint(K::Distance(p, q, 40.0)).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);
        let before = state(&s);
        for value in [0.0, 1e-9] {
            assert!(s.set_constraint_value(id, value).is_err(), "{style:?}");
            assert_eq!(s.solve().status, SolveStatus::Solved, "{style:?}");
            assert_eq!(state(&s), before);
        }
    }
}

#[test]
fn a_rejected_shape_leaves_the_sketch_as_it_was() {
    let mut s = sketch();
    let counts = |s: &Sketch| {
        (
            s.points().count(),
            s.curves().count(),
            s.constraints().count(),
        )
    };
    let before = counts(&s);
    // Zusammenfallende Ecken und ein ungültiger Wert.
    let dup = [v(0.0, 0.0), v(5.0, 0.0), v(5.0, 0.0), v(0.0, 5.0)];
    assert!(s.add_rect_three_point(dup).is_err());
    let nan = [v(f64::NAN, 0.0), v(5.0, 0.0), v(5.0, 5.0), v(0.0, 5.0)];
    assert!(s.add_rect_three_point(nan).is_err());
    assert!(s.add_rect_center(v(0.0, 0.0), v(5.0, 0.0)).is_err());
    let two = PolygonPlan {
        center: v(0.0, 0.0),
        corners: vec![v(1.0, 0.0), v(-1.0, 0.0)],
    };
    assert!(s.add_polygon(&two).is_err());
    let a = v(0.0, 0.0);
    let far = v(9.0, 0.0);
    for (b, radius) in [(a, 5.0), (far, 0.0), (far, -1.0), (far, f64::NAN)] {
        let plan = SlotPlan { a, b, radius };
        assert!(s.add_slot(&plan, SlotStyle::CenterToCenter).is_err());
    }
    let axis = ArcPlan {
        center: a,
        start: v(10.0, 0.0),
        end: v(0.0, 10.0),
    };
    for half_width in [-5.0, 0.0, 10.0, 12.0] {
        assert!(s.add_arc_slot(&ArcSlotPlan { axis, half_width }).is_err());
    }
    assert_eq!(counts(&s), before);
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn changing_the_radius_of_a_64_gon_takes_under_20_ms() {
    let (mut s, p) = polygon(64);
    let radius = s.radius(p.circle).unwrap();
    let id = s.add_constraint(K::Radius(p.circle, radius)).unwrap();
    s.solve();
    s.set_constraint_value(id, 200.0).unwrap();
    let start = Instant::now();
    let r = s.solve();
    let took = start.elapsed();
    assert_eq!(r.status, SolveStatus::Solved);
    println!("64-Eck, Radius ändern: {took:?}");
    assert!(took.as_millis() < 20, "{took:?}");
}
