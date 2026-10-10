//! Entartete Geometrie gilt nicht als Lösung (Spec §4.5): Radius ≤ 0, Linie der Länge null,
//! Bogenstart auf der Mitte. Maße mit Wert null bzw. winzigem Wert werden abgelehnt.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::Sketch;
use planora_sketch::solver::{Drag, SolveStatus};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

/// Feste waagerechte Linie (0,0)–(10,0) und ein tangentialer Kreis darüber (Mitte (5,2), r=2).
fn fixed_line_and_tangent_circle(
    s: &mut Sketch,
) -> (planora_sketch::PointId, planora_sketch::CurveId) {
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
    let m = s.add_point(5.0, 2.0).unwrap();
    let c = s.add_circle(m, 2.0).unwrap();
    s.add_constraint(K::Tangent(l, c)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    (m, c)
}

#[test]
fn dragging_a_tangent_circle_through_its_line_never_gives_a_negative_radius() {
    let mut s = sketch();
    let (m, c) = fixed_line_and_tangent_circle(&mut s);
    for y in [-0.5, -3.0, -6.0] {
        let r = s
            .drag(&[Drag {
                point: m,
                target: DVec2::new(5.0, y),
            }])
            .unwrap();
        let radius = s.radius(c).unwrap();
        let centre = s.point_xy(m).unwrap();
        assert!(
            radius > s.tolerance().linear,
            "Radius {radius} bei Ziel y={y}: {r:?}"
        );
        assert!(centre.y > 0.0, "Kreis gespiegelt: {centre:?}");
        assert!(
            (centre.y - radius).abs() < 1e-6,
            "tangential: {centre:?} r={radius}"
        );
    }
}

#[test]
fn a_solve_that_would_mirror_a_tangent_circle_is_not_solved() {
    let mut s = sketch();
    let (m, c) = fixed_line_and_tangent_circle(&mut s);
    s.set_point_xy(m, 5.0, -3.0).unwrap();
    let r = s.solve();
    assert_ne!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(!r.conflicting.is_empty(), "Ursache benannt: {r:?}");
    assert!(s.radius(c).unwrap() > 0.0);
}

#[test]
fn coincident_endpoints_of_one_line_are_rejected() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 1.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    let id = s.add_constraint(K::Coincident(a, b)).unwrap();
    let r = s.solve();
    assert_ne!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(r.conflicting, vec![id]);
    assert!(
        s.line_length(l).unwrap() > 1.0,
        "Linie bleibt im letzten Zustand"
    );
}

#[test]
fn dragging_an_arc_start_onto_its_fixed_centre_keeps_a_positive_radius() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(4.0, 0.0).unwrap();
    let b = s.add_point(0.0, 4.0).unwrap();
    let arc = s.add_arc(m, a, b).unwrap();
    s.add_constraint(K::Fixed(m, 0.0, 0.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    s.drag(&[Drag {
        point: a,
        target: DVec2::ZERO,
    }])
    .unwrap();
    let r = s.radius(arc).unwrap();
    assert!(r > s.tolerance().linear, "Radius {r}");
}

#[test]
fn a_collapsing_horizontal_line_is_not_accepted_while_dragging() {
    // Rechteckseite nur waagerecht: Ziehen eines Endpunkts auf den anderen ergäbe Länge null.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l)))
        .unwrap();
    s.drag(&[Drag {
        point: b,
        target: DVec2::ZERO,
    }])
    .unwrap();
    assert!(s.line_length(l).unwrap() > s.tolerance().linear);
}

#[test]
fn zero_and_tiny_distances_are_rejected_by_the_api() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    assert!(matches!(
        s.add_constraint(K::Distance(a, b, 0.0)),
        Err(SketchError::Degenerate(_))
    ));
    let d = s.add_constraint(K::Distance(a, b, 5.0)).unwrap();
    assert!(matches!(
        s.set_constraint_value(d, 1e-300),
        Err(SketchError::Degenerate(_))
    ));
    assert!(matches!(
        s.set_constraint_value(d, 0.0),
        Err(SketchError::Degenerate(_))
    ));
    // Abstand Punkt–Linie null bleibt erlaubt (Punkt auf der Linie).
    let p = s.add_point(2.0, 3.0).unwrap();
    let l2 = s.curves().next().unwrap().0;
    assert!(s.add_constraint(K::DistancePointLine(p, l2, 0.0)).is_ok());
}

#[test]
fn the_value_check_is_shared_with_the_user_interface() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    let kind = K::Distance(a, b, 5.0);
    assert!(
        s.check_constraint_value(&kind.with_value(0.0).unwrap())
            .is_err()
    );
    assert!(
        s.check_constraint_value(&kind.with_value(1e-300).unwrap())
            .is_err()
    );
    assert!(
        s.check_constraint_value(&kind.with_value(2.0).unwrap())
            .is_ok()
    );
    let c = s.add_circle(a, 1.0).unwrap();
    let e = s.add_ellipse(b, DVec2::new(3.0, 0.0), 1.0).unwrap();
    // `with_value` kennt alle Maßarten, auch die der Ellipse.
    for kind in [
        K::Radius(c, 1.0),
        K::Diameter(c, 1.0),
        K::EllipseMajor(e, 3.0),
        K::EllipseMinor(e, 1.0),
        K::EllipseAngle(e, 0.0),
        K::Angle(c, c, 0.0),
        K::DistancePointLine(a, c, 0.0),
    ] {
        let changed = kind.with_value(0.5).expect("Maß");
        assert_ne!(changed, kind);
    }
    assert_eq!(K::Coincident(a, b).with_value(1.0), None);
    assert!(s.check_constraint_value(&K::Radius(c, -1.0)).is_err());
    let _ = s.add_constraint(K::Radius(c, 2.0)).unwrap();
}

#[test]
fn coordinates_beyond_the_magnitude_bound_are_rejected() {
    let mut s = sketch();
    assert!(matches!(
        s.add_point(f64::MAX, 0.0),
        Err(SketchError::InvalidInput(_))
    ));
    assert!(matches!(
        s.add_point(0.0, -2e9),
        Err(SketchError::InvalidInput(_))
    ));
    let a = s.add_point(0.0, 0.0).unwrap();
    assert!(matches!(
        s.set_point_xy(a, 0.0, f64::MAX),
        Err(SketchError::InvalidInput(_))
    ));
    assert!(matches!(
        s.add_circle(a, f64::MAX),
        Err(SketchError::InvalidInput(_))
    ));
    assert!(matches!(
        s.add_ellipse(a, DVec2::new(f64::MAX, 0.0), 1.0),
        Err(SketchError::InvalidInput(_))
    ));
    assert!(matches!(
        s.add_constraint(K::Fixed(a, f64::MAX, 0.0)),
        Err(SketchError::InvalidInput(_))
    ));
    let b = s.add_point(1.0, 0.0).unwrap();
    assert!(matches!(
        s.add_constraint(K::Distance(a, b, f64::MAX)),
        Err(SketchError::InvalidInput(_))
    ));
    assert!(matches!(
        s.add_rectangle(DVec2::ZERO, DVec2::splat(f64::MAX)),
        Err(SketchError::InvalidInput(_))
    ));
    assert_eq!(s.point_xy(a).unwrap(), DVec2::ZERO);
    // Große, aber zulässige Werte gehen.
    assert!(s.add_point(1e8, -1e8).is_ok());
}
