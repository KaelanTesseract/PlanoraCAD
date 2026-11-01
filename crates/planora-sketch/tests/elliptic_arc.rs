use planora_math::{DVec2, Frame, PI, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

/// Ellipsenbogen um (0, 0) mit a = (10, 0), kleiner Halbachse 5, von (10, 0) nach (0, 5).
fn quarter(s: &mut Sketch) -> (PointId, PointId, PointId, CurveId) {
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(10.0, 0.0).unwrap();
    let b = s.add_point(0.0, 5.0).unwrap();
    let e = s
        .add_elliptic_arc(c, DVec2::new(10.0, 0.0), 5.0, a, b)
        .unwrap();
    (c, a, b, e)
}

/// Mittelpunkt, große Halbachse und kleine Halbachse eines Ellipsenbogens.
fn axes(s: &Sketch, e: CurveId) -> (DVec2, DVec2, f64) {
    let CurveKind::EllipticArc {
        center,
        ax,
        ay,
        minor,
        ..
    } = s.curve(e).unwrap().kind
    else {
        panic!("kein Ellipsenbogen")
    };
    let v = s.variables();
    (
        s.point_xy(center).unwrap(),
        DVec2::new(v[ax], v[ay]),
        v[minor],
    )
}

/// Implizite Form `x²/|a|² + y²/minor² − 1` in Achskoordinaten.
fn implicit(s: &Sketch, e: CurveId, p: DVec2) -> f64 {
    let (m, a, minor) = axes(s, e);
    let u = a.normalize();
    let d = p - m;
    let (x, y) = (d.dot(u), u.perp_dot(d));
    x * x / a.length_squared() + y * y / (minor * minor) - 1.0
}

#[test]
fn the_polyline_runs_on_the_ellipse_from_start_to_end() {
    let mut s = sketch();
    let (_, a, b, e) = quarter(&mut s);
    let pl = s.curve_polyline(e, 64).unwrap();
    assert_eq!(pl[0], s.point_xy(a).unwrap());
    assert_eq!(*pl.last().unwrap(), s.point_xy(b).unwrap());
    assert!(pl.len() > 4, "{pl:?}");
    for p in &pl {
        assert!(
            (p.x * p.x / 100.0 + p.y * p.y / 25.0 - 1.0).abs() < 1e-9,
            "{p:?}"
        );
        assert!(p.x >= -1e-9 && p.y >= -1e-9, "erster Quadrant: {p:?}");
    }
    let t = s.ellipse_param_of(e, DVec2::new(0.0, 5.0)).unwrap();
    assert!((t - PI / 2.0).abs() < 1e-12, "{t}");
    let p = s.ellipse_point(e, PI / 2.0).unwrap();
    assert!(p.distance(DVec2::new(0.0, 5.0)) < 1e-12);
}

#[test]
fn adding_checks_the_input_like_an_ellipse() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(10.0, 0.0).unwrap();
    let b = s.add_point(0.0, 5.0).unwrap();
    assert!(s.add_elliptic_arc(c, DVec2::ZERO, 1.0, a, b).is_err());
    assert!(
        s.add_elliptic_arc(c, DVec2::new(1.0, 0.0), 2.0, a, b)
            .is_err(),
        "klein > groß"
    );
    assert!(
        s.add_elliptic_arc(c, DVec2::new(f64::NAN, 0.0), 1.0, a, b)
            .is_err()
    );
    assert_eq!(s.curves().count(), 0);
}

#[test]
fn the_ends_stay_on_the_ellipse_after_solving() {
    let mut s = sketch();
    let (_, _, b, e) = quarter(&mut s);
    assert_eq!(s.internal_constraints().count(), 1);
    s.set_point_xy(b, 3.0, 9.0).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let p = s.point_xy(b).unwrap();
    assert!(implicit(&s, e, p).abs() < 1e-7, "{p:?}");
}

#[test]
fn ellipse_dimensions_act_on_elliptic_arcs() {
    let mut s = sketch();
    let (c, a, b, e) = quarter(&mut s);
    s.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    s.add_constraint(K::EllipseMajor(e, 12.0)).unwrap();
    s.add_constraint(K::EllipseMinor(e, 5.0)).unwrap();
    s.add_constraint(K::EllipseAngle(e, 0.0)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let (_, axis, minor) = axes(&s, e);
    assert!((axis.length() - 12.0).abs() < 1e-8, "{axis:?}");
    assert!((minor - 5.0).abs() < 1e-8);
    assert!((s.measure(&K::EllipseMajor(e, 0.0)).unwrap() - 12.0).abs() < 1e-8);
    for p in [a, b] {
        assert!(implicit(&s, e, s.point_xy(p).unwrap()).abs() < 1e-7);
    }
}

#[test]
fn tangent_and_point_on_curve_solve() {
    let mut s = sketch();
    let (c, _, _, e) = quarter(&mut s);
    s.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    s.add_constraint(K::EllipseMajor(e, 10.0)).unwrap();
    s.add_constraint(K::EllipseMinor(e, 5.0)).unwrap();
    s.add_constraint(K::EllipseAngle(e, 0.0)).unwrap();
    let l0 = s.add_point(-3.0, 6.0).unwrap();
    let l1 = s.add_point(3.0, 6.0).unwrap();
    let line = s.add_line(l0, l1).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(line)))
        .unwrap();
    s.add_constraint(K::Tangent(line, e)).unwrap();
    let p = s.add_point(6.0, 6.0).unwrap();
    s.add_constraint(K::PointOnCurve(p, e)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let y = s.point_xy(l0).unwrap().y;
    assert!((y.abs() - 5.0).abs() < 1e-7, "Linie berührt oben: {y}");
    assert!(implicit(&s, e, s.point_xy(p).unwrap()).abs() < 1e-7);
}

#[test]
fn the_internal_constraint_cannot_be_added_by_hand() {
    let mut s = sketch();
    let (.., e) = quarter(&mut s);
    assert!(matches!(
        s.add_constraint(K::EllipticArcEnds(e)),
        Err(SketchError::WrongType(_))
    ));
    assert_eq!(s.constraints().count(), 0);
}

#[test]
fn data_round_trip_keeps_the_elliptic_arc() {
    let mut s = sketch();
    let (.., e) = quarter(&mut s);
    let d = s.to_data();
    let back = Sketch::from_data(d.clone()).unwrap();
    assert_eq!(back.to_data(), d);
    assert!(matches!(
        back.curve(e).unwrap().kind,
        CurveKind::EllipticArc { .. }
    ));
}

#[test]
fn mirroring_copies_the_elliptic_arc() {
    let mut s = sketch();
    let (_, a, b, e) = quarter(&mut s);
    let m0 = s.add_point(-20.0, -10.0).unwrap();
    let m1 = s.add_point(-20.0, 10.0).unwrap();
    let axis = s.add_line(m0, m1).unwrap();
    let copies = s.add_mirrored(&[e], axis).unwrap();
    assert_eq!(copies.len(), 1);
    let copy = copies[0];
    let CurveKind::EllipticArc { start, end, .. } = s.curve(copy).unwrap().kind else {
        panic!("Kopie ist kein Ellipsenbogen")
    };
    let mirror = |p: DVec2| DVec2::new(-40.0 - p.x, p.y);
    // Spiegeln kehrt den Umlaufsinn um: Start und Ende tauschen.
    assert!(
        s.point_xy(start)
            .unwrap()
            .distance(mirror(s.point_xy(b).unwrap()))
            < 1e-9
    );
    assert!(
        s.point_xy(end)
            .unwrap()
            .distance(mirror(s.point_xy(a).unwrap()))
            < 1e-9
    );
    for p in s.curve_polyline(copy, 64).unwrap() {
        let q = mirror(p);
        assert!(
            (q.x * q.x / 100.0 + q.y * q.y / 25.0 - 1.0).abs() < 1e-9,
            "{p:?}"
        );
        assert!(
            q.x >= -1e-9 && q.y >= -1e-9,
            "Spiegelbild des Bogens: {p:?}"
        );
    }
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
}

#[test]
fn an_elliptic_arc_and_its_chord_form_a_profile() {
    // Fläche zwischen Viertelellipse und Sehne: Viertel der Ellipse minus Dreieck.
    let expected = PI * 10.0 * 5.0 / 4.0 - 25.0;
    for line_first in [false, true] {
        let mut s = sketch();
        let c = s.add_point(0.0, 0.0).unwrap();
        let a = s.add_point(10.0, 0.0).unwrap();
        let b = s.add_point(0.0, 5.0).unwrap();
        if line_first {
            // Linie zuerst: die Schleife durchläuft den Bogen gegen seine Richtung.
            s.add_line(a, b).unwrap();
            s.add_elliptic_arc(c, DVec2::new(10.0, 0.0), 5.0, a, b)
                .unwrap();
        } else {
            s.add_elliptic_arc(c, DVec2::new(10.0, 0.0), 5.0, a, b)
                .unwrap();
            s.add_line(b, a).unwrap();
        }
        // bisheriger Algorithmus und Anordnung (Skizzen 1) ergeben dieselbe Fläche
        for ex in [s.legacy_profiles(), s.profiles()] {
            assert!(ex.rejected.is_empty(), "{:?}", ex.rejected);
            assert_eq!(ex.profiles.len(), 1);
            let area = ex.profiles[0].outer.signed_area().abs();
            assert!((area - expected).abs() < 0.1, "{area} statt {expected}");
        }
    }
}

#[test]
fn a_linear_pattern_copies_the_elliptic_arc() {
    let mut s = sketch();
    let (.., e) = quarter(&mut s);
    let groups = s
        .add_linear_pattern(&[e], DVec2::new(30.0, 0.0), 2)
        .unwrap();
    assert_eq!(groups.len(), 2);
    for (k, group) in groups.iter().enumerate() {
        let shift = 30.0 * (k + 1) as f64;
        for p in s.curve_polyline(group[0], 64).unwrap() {
            let q = p - DVec2::new(shift, 0.0);
            assert!(
                (q.x * q.x / 100.0 + q.y * q.y / 25.0 - 1.0).abs() < 1e-9,
                "{p:?}"
            );
        }
    }
}
