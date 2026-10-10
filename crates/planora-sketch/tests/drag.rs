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

// ---- Zwischenspeicher je Gruppe (Abschlusswelle I3) ----------------------------------------

#[test]
fn an_untouched_group_is_not_solved_again() {
    let mut s = sketch();
    let unrelated = unrelated_groups(&mut s);
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 4.0)).unwrap();
    let first = s.solve();
    assert_eq!(first.group_solves, 3, "{first:?}");
    // Nichts geändert: kein Lauf, gleiches Ergebnis.
    let second = s.solve();
    assert_eq!(second.group_solves, 0, "{second:?}");
    assert_eq!(second.status, first.status);
    assert_eq!(second.conflicting, first.conflicting);
    assert_eq!(second.dof, first.dof);
    assert_eq!(second.redundant, first.redundant);
    assert_eq!(second.free_points, first.free_points);

    // Ziehen rechnet nur die gezogene Gruppe; die anderen bleiben bitgleich.
    let before = bits(&s, &unrelated);
    let r = s
        .drag(&[Drag {
            point: b,
            target: DVec2::new(0.0, 10.0),
        }])
        .unwrap();
    assert_eq!(r.group_solves, 1, "{r:?}");
    assert_eq!(r.conflicting, first.conflicting);
    assert_eq!(r.dof, first.dof);
    assert_eq!(bits(&s, &unrelated), before);
    // Am Ende des Ziehens ist alles gemerkt: das volle Lösen rechnet nichts mehr.
    let end = s.solve();
    assert_eq!(end.group_solves, 0, "{end:?}");
    assert_eq!(end.status, SolveStatus::Conflicting);
}

#[test]
fn any_change_invalidates_the_remembered_results() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    let d = s.add_constraint(K::Distance(a, b, 4.0)).unwrap();
    s.solve();
    assert_eq!(s.solve().group_solves, 0);
    // Wertänderung über die API
    s.set_constraint_value(d, 6.0).unwrap();
    let r = s.solve();
    assert_eq!(r.group_solves, 1);
    assert!((s.point_xy(b).unwrap().length() - 6.0).abs() < 1e-9);
    // Punkt von außen verschoben
    s.set_point_xy(b, 0.0, 3.0).unwrap();
    let r = s.solve();
    assert_eq!(r.group_solves, 1);
    assert!((s.point_xy(b).unwrap().length() - 6.0).abs() < 1e-9);
    // Abhängigkeit abgeschaltet
    s.set_enabled(d, false).unwrap();
    assert_eq!(s.solve().group_solves, 1);
}

#[test]
fn a_conflict_in_a_large_group_is_reported_without_the_full_search() {
    // Kette p0 … p60 mit waagerechten Verbindungen und Abständen 1: mehr als 48 Abhängigkeiten
    // in einer Gruppe. Ein zusätzlicher Gesamtabstand 100 widerspricht.
    let mut s = sketch();
    let pts: Vec<PointId> = (0..=60)
        .map(|i| s.add_point(f64::from(i), 0.0).unwrap())
        .collect();
    s.add_constraint(K::Fixed(pts[0], 0.0, 0.0)).unwrap();
    for w in pts.windows(2) {
        s.add_line(w[0], w[1]).unwrap();
        s.add_constraint(K::Distance(w[0], w[1], 1.0)).unwrap();
        s.add_constraint(K::Horizontal(LineOrPoints::Points(w[0], w[1])))
            .unwrap();
    }
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let bad = s
        .add_constraint(K::Distance(pts[0], pts[60], 100.0))
        .unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::NotConverged, "{r:?}");
    assert!(!r.conflicting.is_empty());
    assert!(r.conflicting.contains(&bad), "{:?}", r.conflicting);
    // unverändert im letzten Zustand
    assert_eq!(s.point_xy(pts[60]).unwrap(), DVec2::new(60.0, 0.0));
}

// ---- Zwischenspeicher: Kurven, die über die Gruppe hinausreichen -----------------------------

/// Neue Skizze mit denselben Punkten, Linien und Abhängigkeiten wie `s` (nur für die Fälle
/// unten): liefert den Status eines frischen Laufs ohne Zwischenspeicher.
fn fresh_status(s: &Sketch) -> SolveStatus {
    let mut c = s.clone();
    // Ein strukturelles Ändern leert den Zwischenspeicher: Punkt anlegen und wieder löschen.
    let p = c.add_point(0.0, 0.0).unwrap();
    c.remove_point(p).unwrap();
    c.solve().status
}

#[test]
fn moving_a_free_end_of_a_line_into_a_group_is_seen_by_the_cache() {
    // a und b fest, c frei, Linie b–c: Die Linie gehört zur Gruppe von b, c aber nicht.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    let c = s.add_point(9.0, 3.0).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 5.0, 0.0)).unwrap();
    s.add_line(b, c).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    // c auf b: Linie entartet.
    s.set_point_xy(c, 5.0, 0.0).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::NotConverged, "{r:?}");
    assert_eq!(r.status, fresh_status(&s));
    // c wieder weg: Die Gruppe muss neu bewertet werden.
    s.set_point_xy(c, 9.0, 3.0).unwrap();
    let r = s.solve();
    assert!(r.group_solves >= 1, "{r:?}");
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(r.status, fresh_status(&s));
    // Ziehen von c (frei) wirkt genauso.
    s.set_point_xy(c, 5.0, 0.0).unwrap();
    assert_eq!(s.solve().status, SolveStatus::NotConverged);
    let r = s
        .drag(&[Drag {
            point: c,
            target: DVec2::new(1.0, 7.0),
        }])
        .unwrap();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(r.status, fresh_status(&s));
}

#[test]
fn a_line_between_two_groups_is_seen_by_the_cache() {
    // Gruppe 1: a fest. Gruppe 2: e fest, b im Abstand 8 von e. Linie a–b verbindet beide.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_point(0.0, 8.0).unwrap();
    let b = s.add_point(8.0, 8.0).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(e, 0.0, 8.0)).unwrap();
    s.add_constraint(K::Distance(e, b, 8.0)).unwrap();
    s.add_line(a, b).unwrap();
    s.add_line(e, b).unwrap();
    // b auf a (Abstand zu e bleibt 8): Linie a–b entartet. Strukturänderung danach, damit
    // beide Gruppen frisch (und erfolglos) gerechnet werden.
    s.set_point_xy(b, 0.0, 0.0).unwrap();
    let p = s.add_point(0.0, 0.0).unwrap();
    s.remove_point(p).unwrap();
    let r = s.solve();
    assert_eq!(r.group_solves, 2, "{r:?}");
    assert_eq!(r.status, SolveStatus::NotConverged, "{r:?}");
    assert_eq!(r.status, fresh_status(&s));
    // b nach (8, 8) ziehen: Der Zug bewegt b. Die Gruppe von a wird im selben Lauf noch am
    // Start (b auf a) geprüft, das volle Lösen danach muss die neue Lage von b sehen.
    s.drag(&[Drag {
        point: b,
        target: DVec2::new(8.0, 8.0),
    }])
    .unwrap();
    let pb = s.point_xy(b).unwrap();
    assert!(pb.length() > 1.0, "{pb:?}");
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.group_solves >= 1, "{r:?}");
    assert_eq!(r.status, fresh_status(&s));
    // Danach ist alles gemerkt.
    assert_eq!(s.solve().group_solves, 0);
}

#[test]
fn drag_rejects_targets_beyond_max_coord() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    s.add_constraint(K::Distance(a, b, 4.0)).unwrap();
    for target in [DVec2::new(2e9, 0.0), DVec2::new(0.0, -2e9)] {
        assert!(matches!(
            s.drag(&[Drag { point: b, target }]),
            Err(SketchError::InvalidInput(_))
        ));
    }
    assert_eq!(s.point_xy(b).unwrap(), DVec2::new(4.0, 0.0));
}
