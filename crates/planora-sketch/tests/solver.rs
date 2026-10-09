use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{PointId, Sketch};
use planora_sketch::solver::SolveStatus;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

/// Rechteck aus vier Linien mit gemeinsamen Eckpunkten, leicht schief gestartet.
fn rectangle(s: &mut Sketch, at: DVec2) -> [PointId; 4] {
    let p = [
        s.add_point(at.x + 1.0, at.y + 2.0).unwrap(),
        s.add_point(at.x + 37.0, at.y + 3.0).unwrap(),
        s.add_point(at.x + 41.0, at.y + 33.0).unwrap(),
        s.add_point(at.x - 2.0, at.y + 29.0).unwrap(),
    ];
    let l: Vec<_> = (0..4)
        .map(|i| s.add_line(p[i], p[(i + 1) % 4]).unwrap())
        .collect();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l[0])))
        .unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Line(l[1])))
        .unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l[2])))
        .unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Line(l[3])))
        .unwrap();
    p
}

fn xy(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

#[test]
fn a_dimensioned_and_anchored_rectangle_is_fully_determined() {
    let mut s = sketch();
    let p = rectangle(&mut s, DVec2::ZERO);
    s.add_constraint(K::Fixed(p[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(p[0], p[1], 40.0)).unwrap();
    s.add_constraint(K::Distance(p[1], p[2], 30.0)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(r.dof, 0);
    assert!(r.free_points.is_empty(), "{:?}", r.free_points);
    assert!(r.redundant.is_empty());
    for (pt, want) in p
        .iter()
        .zip([(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)])
    {
        let got = xy(&s, *pt);
        assert!(
            (got - DVec2::new(want.0, want.1)).length() < 1e-7,
            "{got:?} vs {want:?}"
        );
    }
}

#[test]
fn an_unanchored_rectangle_keeps_its_translation_free() {
    let mut s = sketch();
    let p = rectangle(&mut s, DVec2::ZERO);
    s.add_constraint(K::Distance(p[0], p[1], 40.0)).unwrap();
    s.add_constraint(K::Distance(p[1], p[2], 30.0)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved);
    assert_eq!(r.dof, 2, "nur die Verschiebung in x und y bleibt frei");
    assert_eq!(r.free_points.len(), 4);
    let w = xy(&s, p[1]) - xy(&s, p[0]);
    assert!((w.length() - 40.0).abs() < 1e-7);
}

#[test]
fn a_third_right_angle_is_redundant_but_solvable() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(9.0, 1.0).unwrap();
    let c = s.add_point(10.0, 7.0).unwrap();
    let ab = s.add_line(a, b).unwrap();
    let bc = s.add_line(b, c).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(ab)))
        .unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Line(bc)))
        .unwrap();
    let perp = s.add_constraint(K::Perpendicular(ab, bc)).unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    s.add_constraint(K::Distance(b, c, 6.0)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(
        r.redundant,
        vec![perp],
        "die später angelegte Abhängigkeit ist die überzählige"
    );
    assert_eq!(r.dof, 0);
    assert!((xy(&s, c) - DVec2::new(10.0, 6.0)).length() < 1e-7);
}

#[test]
fn contradicting_dimensions_are_reported_and_leave_the_state_alone() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    let d1 = s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    let d2 = s.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    let before = s.variables().to_vec();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    assert!(
        r.conflicting.contains(&d1) && r.conflicting.contains(&d2),
        "{:?}",
        r.conflicting
    );
    assert_eq!(
        s.variables(),
        before.as_slice(),
        "letzter gültiger Zustand bleibt"
    );
}

#[test]
fn driven_and_disabled_constraints_are_ignored() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    let d2 = s.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    s.set_driven(d2, true).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!((xy(&s, b).distance(xy(&s, a)) - 10.0).abs() < 1e-7);
    let e = s.add_constraint(K::Distance(a, b, 30.0)).unwrap();
    s.set_enabled(e, false).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn a_circle_tangent_to_a_fixed_line() {
    let mut s = sketch();
    let l0 = s.add_point(-10.0, 5.0).unwrap();
    let l1 = s.add_point(10.0, 5.0).unwrap();
    let line = s.add_line(l0, l1).unwrap();
    let c = s.add_point(1.0, 0.0).unwrap();
    let q = s.add_point(0.0, 0.0).unwrap();
    let circle = s.add_circle(c, 2.0).unwrap();
    s.add_constraint(K::Fixed(l0, -10.0, 5.0)).unwrap();
    s.add_constraint(K::Fixed(l1, 10.0, 5.0)).unwrap();
    s.add_constraint(K::Fixed(q, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Points(c, q)))
        .unwrap();
    s.add_constraint(K::Radius(circle, 5.0)).unwrap();
    s.add_constraint(K::Tangent(line, circle)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(
        r.dof, 1,
        "der Mittelpunkt kann noch entlang der Linie wandern"
    );
    assert!((s.radius(circle).unwrap() - 5.0).abs() < 1e-7);
    assert!((xy(&s, c).y - 0.0).abs() < 1e-7);
    assert!(r.free_points.contains(&c));
    assert!(
        r.free_curves.contains(&circle),
        "der Kreis hängt am freien Mittelpunkt"
    );
}

#[test]
fn an_arc_end_is_pulled_onto_the_circle_through_its_start() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(5.0, 0.0).unwrap();
    let b = s.add_point(0.0, 7.0).unwrap();
    s.add_arc(c, a, b).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    let ra = xy(&s, a).distance(xy(&s, c));
    let rb = xy(&s, b).distance(xy(&s, c));
    assert!((ra - rb).abs() < 1e-7, "{ra} vs {rb}");
    assert_eq!(r.dof, 5, "6 Koordinaten minus die eine Bogenbedingung");
}

#[test]
fn independent_groups_are_solved_separately() {
    let mut s = sketch();
    let good = rectangle(&mut s, DVec2::ZERO);
    s.add_constraint(K::Fixed(good[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(good[0], good[1], 40.0))
        .unwrap();
    s.add_constraint(K::Distance(good[1], good[2], 30.0))
        .unwrap();
    let a = s.add_point(100.0, 0.0).unwrap();
    let b = s.add_point(105.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    let before_bad = (xy(&s, a), xy(&s, b));
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Conflicting);
    assert!(
        (xy(&s, good[2]) - DVec2::new(40.0, 30.0)).length() < 1e-7,
        "gute Gruppe ist gelöst"
    );
    assert_eq!(
        (xy(&s, a), xy(&s, b)),
        before_bad,
        "schlechte Gruppe unverändert"
    );
}

#[test]
fn empty_and_unconstrained_sketches() {
    let mut s = sketch();
    let r = s.solve();
    assert_eq!((r.status, r.dof), (SolveStatus::Solved, 0));
    s.add_point(1.0, 1.0).unwrap();
    s.add_point(2.0, 2.0).unwrap();
    let r = s.solve();
    assert_eq!(
        (r.status, r.dof, r.free_points.len()),
        (SolveStatus::Solved, 4, 2)
    );
}

#[test]
fn collapsing_geometry_never_produces_nan() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(1.0, 1.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Distance(a, b, 0.0)).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l)))
        .unwrap();
    let _ = s.solve();
    assert!(s.variables().iter().all(|v| v.is_finite()));
}

#[test]
fn solving_twice_changes_nothing_more() {
    let mut s = sketch();
    let p = rectangle(&mut s, DVec2::ZERO);
    s.add_constraint(K::Fixed(p[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(p[0], p[1], 40.0)).unwrap();
    s.add_constraint(K::Distance(p[1], p[2], 30.0)).unwrap();
    s.solve();
    let first = s.variables().to_vec();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved);
    for (a, b) in first.iter().zip(s.variables()) {
        assert!((a - b).abs() < 1e-9);
    }
}

/// Regression: Mit spaltenweiser (Marquardt-)Dämpfung machte der Solver hier riesige Querschritte
/// in der kaum gekoppelten Drehrichtung und meldete fälschlich einen Widerspruch.
#[test]
fn a_point_almost_above_a_fixed_point_converges() {
    for bx in [0.0, 1e-7, 1e-3, 0.5, -3.0] {
        let mut s = sketch();
        let a = s.add_point(0.0, 3.3333).unwrap();
        let b = s.add_point(bx, 16.6667).unwrap();
        s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Solved, "bx = {bx}: {r:?}");
        assert!((xy(&s, b).length() - 10.0).abs() < 1e-7, "bx = {bx}");
        assert_eq!(r.dof, 1, "b kann sich noch um a drehen");
    }
}
