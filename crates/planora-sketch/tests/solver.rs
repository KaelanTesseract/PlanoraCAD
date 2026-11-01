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

/// Linie unter 0° bis (10, 0), dort der Bogen um (10, 5) weiter.
fn line_and_arc(arc_starts_there: bool) -> (Sketch, PointId, planora_sketch::model::CurveId) {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let m = s.add_point(10.0, 5.0).unwrap();
    let other = s.add_point(15.0, 5.0).unwrap();
    let line = s.add_line(a, b).unwrap();
    let arc = if arc_starts_there {
        s.add_arc(m, b, other).unwrap()
    } else {
        s.add_arc(m, other, b).unwrap()
    };
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
    s.add_constraint(K::Tangent(line, arc)).unwrap();
    (s, m, arc)
}

#[test]
fn a_tangent_at_a_shared_end_is_no_redundancy_and_holds_the_direction() {
    for arc_starts_there in [true, false] {
        let (mut s, m, _) = line_and_arc(arc_starts_there);
        let free = s.solve();
        assert_eq!(free.status, SolveStatus::Solved, "{free:?}");
        assert!(free.redundant.is_empty(), "{free:?}");
        // Mitte nur noch auf der Senkrechten, anderes Bogenende auf dem Kreis: 2 Freiheitsgrade.
        assert_eq!(free.dof, 2, "{free:?}");
        let r = s
            .drag(&[planora_sketch::solver::Drag {
                point: m,
                target: DVec2::new(14.0, 9.0),
            }])
            .unwrap();
        assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
        assert!((xy(&s, m).x - 10.0).abs() < 1e-7, "{:?}", xy(&s, m));
    }
}

#[test]
fn a_tangent_between_two_arcs_at_a_shared_end_keeps_the_centres_in_line() {
    let mut s = sketch();
    let m1 = s.add_point(0.0, 0.0).unwrap();
    let start = s.add_point(10.0, 0.0).unwrap();
    let shared = s.add_point(0.0, 10.0).unwrap();
    let m2 = s.add_point(0.0, 6.0).unwrap();
    let end = s.add_point(-4.0, 6.0).unwrap();
    let first = s.add_arc(m1, start, shared).unwrap();
    let second = s.add_arc(m2, shared, end).unwrap();
    s.add_constraint(K::Fixed(m1, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(start, 10.0, 0.0)).unwrap();
    s.add_constraint(K::Tangent(second, first)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.redundant.is_empty(), "{r:?}");
    let d = s
        .drag(&[planora_sketch::solver::Drag {
            point: m2,
            target: DVec2::new(3.0, 5.0),
        }])
        .unwrap();
    assert_eq!(d.status, SolveStatus::Solved, "{d:?}");
    let (p, q) = (xy(&s, shared), xy(&s, m2));
    assert!(p.perp_dot(q).abs() < 1e-6, "{p:?} {q:?}");
}

/// Alles fest: Linie bis (10, 0), Bogen mit dem Anfang dort, Mitte `off` neben der Senkrechten.
fn fixed_tangent_off_by(off: f64) -> (Sketch, PointId, PointId) {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let m = s.add_point(10.0 + off, 5.0).unwrap();
    let other = s.add_point(15.0 + off, 5.0).unwrap();
    let line = s.add_line(a, b).unwrap();
    let arc = s.add_arc(m, b, other).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(m, 10.0 + off, 5.0)).unwrap();
    s.add_constraint(K::Tangent(line, arc)).unwrap();
    (s, m, other)
}

#[test]
fn a_drifted_tangent_at_a_shared_end_of_an_old_sketch_stays_solved() {
    for off in [1e-4, 1e-5, -1e-4] {
        let (mut s, m, other) = fixed_tangent_off_by(off);
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Solved, "{off}: {r:?}");
        assert!(r.conflicting.is_empty(), "{off}: {r:?}");
        assert_eq!(xy(&s, m), DVec2::new(10.0 + off, 5.0));
        // Was noch frei ist, lässt sich weiter ziehen.
        let d = s
            .drag(&[planora_sketch::solver::Drag {
                point: other,
                target: DVec2::new(10.0, 9.0),
            }])
            .unwrap();
        assert_eq!(d.status, SolveStatus::Solved, "{off}: {d:?}");
        assert!(xy(&s, other).y > 8.0, "{:?}", xy(&s, other));
    }
}

#[test]
fn a_really_wrong_tangent_at_a_shared_end_is_still_a_conflict() {
    // 0,1 rad bei r = 5: ein halber Millimeter neben der Senkrechten.
    for off in [0.5, 0.05] {
        let (mut s, ..) = fixed_tangent_off_by(off);
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Conflicting, "{off}: {r:?}");
    }
}

/// Kette Linie–Bogen–Bogen: Linie und erste Mitte fest, erste Tangente um `off` daneben, die
/// zweite Mitte frei und die zweite Tangente exakt.
fn tangent_chain(off: f64) -> (Sketch, [PointId; 3]) {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let m1 = s.add_point(10.0 + off, 5.0).unwrap();
    let c = s.add_point(15.0 + off, 5.0).unwrap();
    let m2 = s.add_point(18.0 + off, 5.0).unwrap();
    let e = s.add_point(18.0 + off, 8.0).unwrap();
    let line = s.add_line(a, b).unwrap();
    let first = s.add_arc(m1, b, c).unwrap();
    let second = s.add_arc(m2, e, c).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(m1, 10.0 + off, 5.0)).unwrap();
    s.add_constraint(K::Tangent(line, first)).unwrap();
    s.add_constraint(K::Tangent(first, second)).unwrap();
    (s, [c, m2, e])
}

#[test]
fn free_geometry_in_a_tangent_chain_stays_draggable_beside_a_drifted_tangent() {
    let targets = [(13.0, 9.0), (20.0, 5.0), (18.0, 9.0)];
    for off in [0.0, 1e-4] {
        let (mut s, points) = tangent_chain(off);
        assert_eq!(s.solve().status, SolveStatus::Solved, "{off}");
        for (p, t) in points.into_iter().zip(targets) {
            let mut moved = s.clone();
            let d = moved
                .drag(&[planora_sketch::solver::Drag {
                    point: p,
                    target: DVec2::new(t.0, t.1),
                }])
                .unwrap();
            assert_eq!(d.status, SolveStatus::Solved, "{off} {t:?}: {d:?}");
            // Die exakte Tangente bleibt exakt: beide Mitten auf einer Geraden durch den Punkt.
            let (c, m2) = (xy(&moved, points[0]), xy(&moved, points[1]));
            let m1 = DVec2::new(10.0 + off, 5.0);
            let cross = (m1 - c).perp_dot(m2 - c) / (m1 - c).length();
            assert!(cross.abs() < 1e-6, "{off} {t:?}: {cross}");
        }
    }
}

#[test]
fn a_dimension_can_still_change_a_tangent_chain_beside_a_drifted_tangent() {
    for off in [0.0, 1e-4] {
        let mut s = sketch();
        let a = s.add_point(0.0, 0.0).unwrap();
        let b = s.add_point(10.0, 0.0).unwrap();
        let m = s.add_point(10.0 + off, 5.0).unwrap();
        let c = s.add_point(15.0 + off, 5.0).unwrap();
        let d = s.add_point(15.0 + off, 15.0).unwrap();
        let line = s.add_line(a, b).unwrap();
        let arc = s.add_arc(m, b, c).unwrap();
        let next = s.add_line(c, d).unwrap();
        s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
        s.add_constraint(K::Fixed(m, 10.0 + off, 5.0)).unwrap();
        s.add_constraint(K::Tangent(line, arc)).unwrap();
        s.add_constraint(K::Tangent(arc, next)).unwrap();
        let angle = s
            .add_constraint(K::Angle(line, next, 90f64.to_radians()))
            .unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved, "{off}");
        s.set_constraint_value(angle, 60f64.to_radians()).unwrap();
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Solved, "{off}: {r:?}");
        // Die Linie steht unter 60°, ihr Anfang liegt also 30° um die Mitte weiter.
        let radial = xy(&s, c) - DVec2::new(10.0 + off, 5.0);
        assert!(
            (radial.y.atan2(radial.x).abs() - 30f64.to_radians()).abs() < 1e-3,
            "{off}: {radial:?}"
        );
    }
}

#[test]
fn a_new_constraint_against_an_exact_tangent_at_a_shared_end_is_a_conflict() {
    for (radius, off) in [(5.0, 0.004), (1000.0, 0.9)] {
        let mut s = sketch();
        let a = s.add_point(0.0, 0.0).unwrap();
        let b = s.add_point(10.0, 0.0).unwrap();
        let m = s.add_point(10.0, radius).unwrap();
        let other = s.add_point(10.0 + radius, radius).unwrap();
        let line = s.add_line(a, b).unwrap();
        let arc = s.add_arc(m, b, other).unwrap();
        s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
        s.add_constraint(K::Tangent(line, arc)).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);
        s.add_constraint(K::Fixed(m, 10.0 + off, radius)).unwrap();
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Conflicting, "{radius}: {r:?}");
        assert_eq!(xy(&s, m), DVec2::new(10.0, radius), "nichts bewegt");
    }
}

#[test]
fn a_drifted_tangent_does_not_hide_a_real_conflict_elsewhere() {
    let (mut s, m, _) = fixed_tangent_off_by(1e-4);
    let q = s.add_point(10.0, 12.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(m, q, 7.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    s.add_constraint(K::Distance(q, b, 1.0)).unwrap();
    assert_ne!(s.solve().status, SolveStatus::Solved);
}

/// Eine Änderung an der Linie (Regel 7) behält eine alte, im Rahmen abweichende Tangente, eine
/// wirklich falsche fällt weg.
#[test]
fn an_edit_keeps_an_old_drifted_tangent_and_drops_a_wrong_one() {
    for (off, kept) in [(1e-4, true), (0.5, false)] {
        let mut s = sketch();
        let a = s.add_point(0.0, 0.0).unwrap();
        let b = s.add_point(10.0, 0.0).unwrap();
        let m = s.add_point(10.0 + off, 5.0).unwrap();
        let other = s.add_point(15.0 + off, 5.0).unwrap();
        let line = s.add_line(a, b).unwrap();
        let arc = s.add_arc(m, b, other).unwrap();
        let p = s.add_point(-5.0, -5.0).unwrap();
        let q = s.add_point(-5.0, 5.0).unwrap();
        s.add_line(p, q).unwrap();
        s.add_constraint(K::Horizontal(LineOrPoints::Line(line)))
            .unwrap();
        s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
        s.add_constraint(K::Fixed(m, 10.0 + off, 5.0)).unwrap();
        let tangent = s.add_constraint(K::Tangent(line, arc)).unwrap();
        s.solve();
        // Dehnen: Das freie Ende der Linie läuft bis zur Senkrechten bei x = -5.
        assert!(s.extend(line, DVec2::new(1.0, 0.0)).unwrap());
        assert!((xy(&s, a).x + 5.0).abs() < 1e-6, "{:?}", xy(&s, a));
        assert_eq!(s.constraint(tangent).is_some(), kept, "{off}");
    }
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
    // Abstand 0 lehnt die API inzwischen ab (Abschlusswelle I1); das Zusammenziehen erzwingt
    // jetzt `Coincident`. Der Solver darf dabei kein NaN erzeugen und die Linie nicht
    // entarten lassen.
    assert!(s.add_constraint(K::Distance(a, b, 0.0)).is_err());
    s.add_constraint(K::Coincident(a, b)).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l)))
        .unwrap();
    let r = s.solve();
    assert!(s.variables().iter().all(|v| v.is_finite()));
    assert_ne!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(s.line_length(l).unwrap() > s.tolerance().linear);
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
