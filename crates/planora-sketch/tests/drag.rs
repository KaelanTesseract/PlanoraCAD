use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{PointId, Sketch};
use planora_sketch::solver::{Drag, SolveStatus};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn dragging_a_free_point_moves_it_exactly() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let r = s
        .drag(&[Drag {
            point: a,
            target: DVec2::new(3.0, -4.0),
        }])
        .unwrap();
    assert_eq!(r.status, SolveStatus::Solved);
    assert_eq!(s.point_xy(a).unwrap(), DVec2::new(3.0, -4.0));
}

#[test]
fn dragging_respects_hard_constraints() {
    // Linie fest an a (0,0), Länge 10, horizontal; Ziehen von b nach (7, 7) darf sie nicht
    // verlängern oder kippen, b bleibt auf der x-Achse bei Länge 10.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l)))
        .unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let r = s
        .drag(&[Drag {
            point: b,
            target: DVec2::new(7.0, 7.0),
        }])
        .unwrap();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    let pb = s.point_xy(b).unwrap();
    assert!((pb - DVec2::new(10.0, 0.0)).length() < 1e-6, "{pb:?}");
}

#[test]
fn dragging_a_hinge_never_flips_to_the_mirror_solution() {
    // Zwei Stäbe a–m (Länge 5) und m–b (Länge 5) mit festen Enden a=(0,0), b=(8,0): m hat genau
    // zwei Lösungen (4, ±3). Egal wohin gezogen wird, m bleibt oben und exakt auf der Lösung.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(8.0, 0.0).unwrap();
    let m = s.add_point(4.0, 3.0).unwrap();
    s.add_line(a, m).unwrap();
    s.add_line(m, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 8.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(a, m, 5.0)).unwrap();
    s.add_constraint(K::Distance(m, b, 5.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    for target in [
        DVec2::new(4.0, 9.0),
        DVec2::new(4.5, 2.0),
        DVec2::new(3.5, 3.5),
    ] {
        let r = s.drag(&[Drag { point: m, target }]).unwrap();
        assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
        let pm = s.point_xy(m).unwrap();
        assert!(pm.y > 0.0, "ist nach unten umgeklappt: {pm:?}");
        assert!((pm.distance(DVec2::ZERO) - 5.0).abs() < 1e-6);
        assert!((pm.distance(DVec2::new(8.0, 0.0)) - 5.0).abs() < 1e-6);
        // Zusatz (Review Task 4): Ergebnis ist die nächstgelegene gültige Lage (4, 3).
        assert!((pm - DVec2::new(4.0, 3.0)).length() < 1e-6, "{pm:?}");
    }
}

#[test]
fn dragging_in_a_degenerate_direction_keeps_the_state_valid() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 4.0)).unwrap();
    s.solve();
    // Ziel genau auf a: kein NaN, Abstand bleibt 4
    let r = s
        .drag(&[Drag {
            point: b,
            target: DVec2::ZERO,
        }])
        .unwrap();
    assert!(s.variables().iter().all(|v| v.is_finite()));
    assert_eq!(r.status, SolveStatus::Solved);
    let pb = s.point_xy(b).unwrap();
    assert!((pb.length() - 4.0).abs() < 1e-6, "{pb:?}");
}

#[test]
fn drag_rejects_bad_input() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    assert!(matches!(
        s.drag(&[Drag {
            point: a,
            target: DVec2::new(f64::NAN, 0.0)
        }]),
        Err(SketchError::InvalidInput(_))
    ));
    let bogus = planora_kernel::Id::from_index(42);
    assert_eq!(
        s.drag(&[Drag {
            point: bogus,
            target: DVec2::ZERO
        }])
        .unwrap_err(),
        SketchError::UnknownObject
    );
    assert_eq!(s.point_xy(a).unwrap(), DVec2::ZERO);
}

/// Zwei vom Ziehen unabhängige Gruppen: eine widersprüchliche (c–d soll gleichzeitig 3 und 5
/// lang sein) und eine lösbare, die erst noch bewegt werden muss (e fest, f im Abstand 5).
fn unrelated_groups(s: &mut Sketch) -> [PointId; 4] {
    let c = s.add_point(20.0, 0.0).unwrap();
    let d = s.add_point(24.0, 0.0).unwrap();
    s.add_line(c, d).unwrap();
    s.add_constraint(K::Fixed(c, 20.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(c, d, 3.0)).unwrap();
    s.add_constraint(K::Distance(c, d, 5.0)).unwrap();
    let e = s.add_point(30.0, 1.0).unwrap();
    let f = s.add_point(35.0, 2.0).unwrap();
    s.add_line(e, f).unwrap();
    s.add_constraint(K::Fixed(e, 30.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(e, f, 5.0)).unwrap();
    [c, d, e, f]
}

fn bits(s: &Sketch, ps: &[PointId]) -> Vec<(u64, u64)> {
    ps.iter()
        .map(|&p| {
            let v = s.point_xy(p).unwrap();
            (v.x.to_bits(), v.y.to_bits())
        })
        .collect()
}

#[test]
fn dragging_a_free_point_solves_other_groups_like_a_plain_solve() {
    let mut s = sketch();
    let unrelated = unrelated_groups(&mut s);
    let free = s.add_point(-5.0, -5.0).unwrap();
    let before = bits(&s, &unrelated[..2]);

    let mut plain = s.clone();
    let expected = plain.solve();
    assert_eq!(expected.status, SolveStatus::Conflicting);

    let r = s
        .drag(&[Drag {
            point: free,
            target: DVec2::new(1.0, 2.0),
        }])
        .unwrap();
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    assert_eq!(s.point_xy(free).unwrap(), DVec2::new(1.0, 2.0));
    // Widersprüchliche Gruppe bleibt bitgleich, die lösbare wird genau wie von `solve` gelöst.
    assert_eq!(bits(&s, &unrelated[..2]), before);
    assert_eq!(bits(&s, &unrelated), bits(&plain, &unrelated));
    // Ohne weiche Zeilen läuft kein Zug- und Politurlauf: gleiche Schrittzahl wie `solve`.
    assert_eq!(r.iterations, expected.iterations);
    assert_eq!(r.conflicting, expected.conflicting);
}

#[test]
fn dragging_one_group_does_not_touch_an_unrelated_conflicting_group() {
    let mut s = sketch();
    let unrelated = unrelated_groups(&mut s);
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 4.0)).unwrap();
    let before = bits(&s, &unrelated[..2]);

    let mut plain = s.clone();
    let expected = plain.solve();

    let r = s
        .drag(&[Drag {
            point: b,
            target: DVec2::new(0.0, 10.0),
        }])
        .unwrap();
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    assert_eq!(r.conflicting, expected.conflicting);
    assert_eq!(bits(&s, &unrelated[..2]), before);
    assert_eq!(bits(&s, &unrelated), bits(&plain, &unrelated));
    // Die gezogene Gruppe ist trotzdem gelöst und folgt dem Zug.
    let pb = s.point_xy(b).unwrap();
    assert!((pb.length() - 4.0).abs() < 1e-6, "{pb:?}");
    assert!(pb.y > 3.9, "b folgt dem Zug: {pb:?}");
}
