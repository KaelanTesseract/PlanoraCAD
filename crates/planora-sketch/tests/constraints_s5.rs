//! Neue Abhängigkeitsarten aus Skizzen 5 (Task 1): Kollinear, Glatt, waagerechter und senkrechter
//! Abstand, Durchmesser zur Mittellinie und Gesamtlänge des Langlochs. Zeilen und Freiheitsgrade,
//! dichte Wertefolgen mit Prüfung auf Umklappen und Seitenwechsel, Überzähliges, Typprüfung,
//! Umschalter Radius/Durchmesser und Verschieben, Drehen, Skalieren und Kopieren.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::shapes::{SlotIds, SlotPlan, SlotStyle, folded_over};
use planora_sketch::solver::{Drag, SolveReport, SolveStatus};
use planora_sketch::transform::Similarity;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn pt(s: &mut Sketch, x: f64, y: f64) -> PointId {
    s.add_point(x, y).unwrap()
}

fn at(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn line(s: &mut Sketch, a: DVec2, b: DVec2) -> (CurveId, PointId, PointId) {
    let (pa, pb) = (pt(s, a.x, a.y), pt(s, b.x, b.y));
    (s.add_line(pa, pb).unwrap(), pa, pb)
}

fn ends(s: &Sketch, l: CurveId) -> (PointId, PointId) {
    match s.curve(l).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        ref k => panic!("keine Linie: {k:?}"),
    }
}

fn dir(s: &Sketch, l: CurveId) -> DVec2 {
    let (a, b) = ends(s, l);
    at(s, b) - at(s, a)
}

/// Gelöst, ohne Widerspruch.
fn solved(s: &mut Sketch) -> SolveReport {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.conflicting.is_empty(), "{r:?}");
    r
}

/// Freiheitsgrade vor und nach dem Anlegen von `kind`; die Skizze ist danach gelöst, ohne
/// Überzähliges.
fn rel(s: &mut Sketch, kind: K) -> (i64, ConstraintId) {
    let before = solved(s).dof as i64;
    let id = s.add_constraint(kind).unwrap();
    let r = solved(s);
    assert!(r.redundant.is_empty(), "{r:?}");
    (r.dof as i64 - before, id)
}

fn row(s: &Sketch, id: ConstraintId, i: usize) -> f64 {
    s.evaluate(id, s.variables()).unwrap().residuals[i].v
}

/// Richtung aus dem Ende `p` in die Kurve `c` (nur für die Prüfung, numerisch).
fn into_curve(s: &Sketch, c: CurveId, p: PointId) -> DVec2 {
    match s.curve(c).unwrap().kind.clone() {
        CurveKind::Line { a, b } => {
            if p == a {
                at(s, b) - at(s, a)
            } else {
                at(s, a) - at(s, b)
            }
        }
        CurveKind::Arc { center, start, .. } => {
            let r = at(s, p) - at(s, center);
            if p == start { r.perp() } else { -r.perp() }
        }
        CurveKind::Spline { points } => {
            let n = (points.len() - 1) as f64;
            let h = 1e-6;
            if p == points[0] {
                s.spline_point(c, h).unwrap() - at(s, p)
            } else {
                s.spline_point(c, n - h).unwrap() - at(s, p)
            }
        }
        k => panic!("{k:?}"),
    }
}

/// Winkel zwischen der Richtung in die erste Kurve und der umgekehrten Richtung in die zweite.
fn smooth_angle(s: &Sketch, a: CurveId, pa: PointId, b: CurveId, pb: PointId) -> f64 {
    let u = into_curve(s, a, pa);
    let w = -into_curve(s, b, pb);
    u.perp_dot(w).atan2(u.dot(w))
}

/// Vergleicht die Ableitungen aller Residuen mit zentralen Differenzenquotienten.
fn check_jacobian(s: &Sketch, id: ConstraintId) {
    let x0 = s.variables().to_vec();
    let ev = s.evaluate(id, &x0).unwrap();
    assert!(!ev.residuals.is_empty());
    let h = 1e-6;
    for (k, &var) in ev.locals.iter().enumerate() {
        let (mut xp, mut xm) = (x0.clone(), x0.clone());
        xp[var] += h;
        xm[var] -= h;
        let (ep, em) = (s.evaluate(id, &xp).unwrap(), s.evaluate(id, &xm).unwrap());
        for (i, r) in ev.residuals.iter().enumerate() {
            let numeric = (ep.residuals[i].v - em.residuals[i].v) / (2.0 * h);
            assert!(
                (r.grad(k) - numeric).abs() < 1e-5,
                "Zeile {i}, Variable {var}: {} gegen {numeric}",
                r.grad(k)
            );
        }
    }
}

fn spline(s: &mut Sketch, xy: &[DVec2]) -> (CurveId, Vec<PointId>) {
    let pts: Vec<PointId> = xy.iter().map(|q| pt(s, q.x, q.y)).collect();
    (s.add_spline(&pts).unwrap(), pts)
}

/// Gerades Langloch mit Mittelpunkt, Mittelpunkt fest, Achse waagerecht.
fn slot(s: &mut Sketch, len: f64, r: f64) -> SlotIds {
    let plan = SlotPlan {
        a: v(-len / 2.0, 0.0),
        b: v(len / 2.0, 0.0),
        radius: r,
    };
    let ids = s.add_slot(&plan, SlotStyle::CenterPoint).unwrap();
    s.add_constraint(K::Fixed(ids.mid.unwrap(), 0.0, 0.0))
        .unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(ids.axis)))
        .unwrap();
    ids
}

// ---- Zeilen und Freiheitsgrade -----------------------------------------------------------

#[test]
fn collinear_with_gap_takes_two_and_with_shared_end_one() {
    let mut s = sketch();
    let (l1, _, _) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    let (l2, _, _) = line(&mut s, v(15.0, 1.0), v(25.0, 2.0));
    let (d, id) = rel(&mut s, K::Collinear(l1, l2));
    assert_eq!(d, -2);
    check_jacobian(&s, id);
    assert!(row(&s, id, 0).abs() < 1e-9 && row(&s, id, 1).abs() < 1e-9);
    assert!(dir(&s, l1).perp_dot(dir(&s, l2)).abs() < 1e-6);

    let mut s = sketch();
    let (l1, _, b) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    let c = pt(&mut s, 20.0, 3.0);
    let l2 = s.add_line(b, c).unwrap();
    let (d, id) = rel(&mut s, K::Collinear(l1, l2));
    assert_eq!(d, -1);
    check_jacobian(&s, id);
    assert!(dir(&s, l1).perp_dot(dir(&s, l2)).abs() < 1e-6);
}

#[test]
fn smooth_takes_one_for_line_arc_and_spline() {
    // Linie–Spline am gemeinsamen Punkt
    let mut s = sketch();
    let (l, _, joint) = line(&mut s, v(-10.0, 0.0), v(0.0, 0.0));
    let q = [pt(&mut s, 5.0, 3.0), pt(&mut s, 10.0, 0.0)];
    let sp = s.add_spline(&[joint, q[0], q[1]]).unwrap();
    let (d, id) = rel(&mut s, K::Smooth(l, joint, sp, joint));
    assert_eq!(d, -1);
    check_jacobian(&s, id);
    assert!(row(&s, id, 0).abs() < 1e-8);
    assert!(smooth_angle(&s, l, joint, sp, joint).abs() < 1e-4);

    // Bogen–Spline am Ende des Bogens
    let mut s = sketch();
    let (m, st, en) = (
        pt(&mut s, 0.0, 0.0),
        pt(&mut s, 5.0, 0.0),
        pt(&mut s, 0.0, 5.0),
    );
    let arc = s.add_arc(m, st, en).unwrap();
    let q = [pt(&mut s, -5.0, 8.0), pt(&mut s, -10.0, 6.0)];
    let sp = s.add_spline(&[en, q[0], q[1]]).unwrap();
    let (d, id) = rel(&mut s, K::Smooth(arc, en, sp, en));
    assert_eq!(d, -1);
    check_jacobian(&s, id);
    assert!(smooth_angle(&s, arc, en, sp, en).abs() < 1e-4);

    // Spline–Spline
    let mut s = sketch();
    let (s1, p1) = spline(&mut s, &[v(-10.0, 0.0), v(-5.0, 2.0), v(0.0, 0.0)]);
    let joint = p1[2];
    let q = [pt(&mut s, 5.0, 3.0), pt(&mut s, 10.0, 0.0)];
    let s2 = s.add_spline(&[joint, q[0], q[1]]).unwrap();
    let (d, id) = rel(&mut s, K::Smooth(s1, joint, s2, joint));
    assert_eq!(d, -1);
    check_jacobian(&s, id);
    assert!(smooth_angle(&s, s1, joint, s2, joint).abs() < 1e-4);
}

#[test]
fn horizontal_and_vertical_distances_take_one() {
    for vertical in [false, true] {
        let mut s = sketch();
        let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 7.0, 4.0));
        let kind = if vertical {
            K::DistanceY(a, b, 6.0)
        } else {
            K::DistanceX(a, b, 6.0)
        };
        let (d, id) = rel(&mut s, kind.clone());
        assert_eq!(d, -1);
        check_jacobian(&s, id);
        let delta = at(&s, b) - at(&s, a);
        let got = if vertical { delta.y } else { delta.x };
        assert!((got - 6.0).abs() < 1e-9, "{got}");
        assert!((s.measure(&kind).unwrap() - 6.0).abs() < 1e-9);
        assert!(kind.is_dimension());
        assert_eq!(kind.length_value(), Some(6.0));
    }
    // Der Betrag zählt: links von a gemessen bleibt der Wert positiv.
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, -7.0, -4.0));
    assert!((s.measure(&K::DistanceX(a, b, 1.0)).unwrap() - 7.0).abs() < 1e-12);
    assert!((s.measure(&K::DistanceY(a, b, 1.0)).unwrap() - 4.0).abs() < 1e-12);
}

#[test]
fn centerline_diameter_takes_one_and_measures_twice_the_distance() {
    let mut s = sketch();
    let (axis, _, _) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    s.set_centerline(axis, true).unwrap();
    let p = pt(&mut s, 3.0, 4.0);
    let kind = K::CenterlineDiameter(p, axis, 8.0);
    assert!((s.measure(&kind).unwrap() - 8.0).abs() < 1e-12);
    let (d, id) = rel(&mut s, K::CenterlineDiameter(p, axis, 12.0));
    assert_eq!(d, -1);
    check_jacobian(&s, id);
    assert!((s.measure(&kind).unwrap() - 12.0).abs() < 1e-9);
    assert_eq!(kind.length_value(), Some(8.0));
}

#[test]
fn slot_overall_leaves_the_slot_fully_defined() {
    let mut s = sketch();
    let ids = slot(&mut s, 20.0, 3.0);
    s.add_constraint(K::Diameter(ids.caps[0], 6.0)).unwrap();
    assert_eq!(solved(&mut s).dof, 1);
    let kind = K::SlotOverall(ids.centers[0], ids.centers[1], ids.caps[0], 30.0);
    assert!((s.measure(&kind).unwrap() - 26.0).abs() < 1e-9);
    let id = s.add_constraint(kind.clone()).unwrap();
    let r = solved(&mut s);
    assert_eq!(r.dof, 0);
    assert!(r.redundant.is_empty());
    check_jacobian(&s, id);
    assert!((s.measure(&kind).unwrap() - 30.0).abs() < 1e-9);
    assert_eq!(kind.length_value(), Some(30.0));
}

// ---- Dichte Folgen -----------------------------------------------------------------------

#[test]
fn collinear_over_dense_angles_and_gaps_never_turns_the_second_line() {
    let mut cases = Vec::new();
    for deg in -170..=170 {
        for gap in [0.5, 5.0, 50.0, 200.0] {
            cases.push((f64::from(deg), gap));
        }
    }
    for k in 0..=400 {
        cases.push((30.0, 0.5 + 199.5 * f64::from(k) / 400.0));
        cases.push((-150.0, 0.5 + 199.5 * f64::from(k) / 400.0));
    }
    for (deg, gap) in cases {
        let mut s = sketch();
        let (l1, a, b) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
        s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
        let start = v(10.0 + gap, 1.0);
        let (sin, cos) = f64::to_radians(deg).sin_cos();
        let (l2, _, _) = line(&mut s, start, start + v(cos, sin) * 8.0);
        solved(&mut s);
        let before = s.clone();
        let d0 = dir(&s, l2);
        let id = s.add_constraint(K::Collinear(l1, l2)).unwrap();
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Solved, "{deg}° Lücke {gap}: {r:?}");
        assert!(r.conflicting.is_empty());
        assert!(!folded_over(&before, &s), "{deg}° Lücke {gap}");
        let d2 = dir(&s, l2);
        assert!(d2.dot(d0) > 0.0, "{deg}° Lücke {gap}: L2 gedreht");
        assert!(row(&s, id, 0).abs() < 1e-8 && row(&s, id, 1).abs() < 1e-8);
    }
}

#[test]
fn distance_x_and_y_over_dense_values_keep_their_side() {
    for vertical in [false, true] {
        for side in [1.0, -1.0] {
            let mut s = sketch();
            let (l, a, b) = line(&mut s, v(0.0, 0.0), v(10.0 * side, 3.0 * side));
            s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
            let kind = if vertical {
                K::DistanceY(a, b, 3.0)
            } else {
                K::DistanceX(a, b, 10.0)
            };
            let id = s.add_constraint(kind).unwrap();
            solved(&mut s);
            for k in 0..1000 {
                let value = 0.05 + (50.0 - 0.05) * f64::from(k) / 999.0;
                let before = s.clone();
                s.set_constraint_value(id, value).unwrap();
                solved(&mut s);
                assert!(!folded_over(&before, &s), "{value}");
                let d = dir(&s, l);
                let got = if vertical { d.y } else { d.x };
                assert!(got * side > 0.0, "Seite gewechselt bei {value}");
                assert!((got.abs() - value).abs() < 1e-6, "{value}: {got}");
            }
        }
    }
}

#[test]
fn centerline_diameter_over_dense_values_keeps_its_side() {
    for side in [1.0, -1.0] {
        let mut s = sketch();
        let (axis, a, b) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
        s.set_centerline(axis, true).unwrap();
        s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
        let (_, p, q) = line(&mut s, v(3.0, 4.0 * side), v(8.0, 6.0 * side));
        s.add_constraint(K::Fixed(q, 8.0, 6.0 * side)).unwrap();
        let id = s
            .add_constraint(K::CenterlineDiameter(p, axis, 8.0))
            .unwrap();
        solved(&mut s);
        for k in 0..1000 {
            let value = 0.3 + (300.0 - 0.3) * f64::from(k) / 999.0;
            let before = s.clone();
            s.set_constraint_value(id, value).unwrap();
            solved(&mut s);
            assert!(!folded_over(&before, &s), "{value}");
            let y = at(&s, p).y;
            assert!(y * side > 0.0, "Seite gewechselt bei {value}");
            assert!((2.0 * y.abs() - value).abs() < 1e-6, "{value}: {y}");
        }
    }
}

#[test]
fn slot_overall_over_dense_lengths_and_widths_holds_the_total() {
    let check = |s: &Sketch, ids: &SlotIds, total: f64, what: &str| {
        let (a, b) = (at(s, ids.centers[0]), at(s, ids.centers[1]));
        let r = s.radius(ids.caps[0]).unwrap();
        assert!(((a - b).length() + 2.0 * r - total).abs() < 1e-6, "{what}");
        assert!(b.x > a.x, "{what}: Mitten vertauscht");
    };
    // Länge 11…211 bei Breite 10
    let mut s = sketch();
    let ids = slot(&mut s, 20.0, 5.0);
    let w = s.add_constraint(K::Diameter(ids.caps[0], 10.0)).unwrap();
    let id = s
        .add_constraint(K::SlotOverall(
            ids.centers[0],
            ids.centers[1],
            ids.caps[0],
            30.0,
        ))
        .unwrap();
    solved(&mut s);
    for k in 0..1000 {
        let total = 11.0 + 200.0 * f64::from(k) / 999.0;
        let before = s.clone();
        s.set_constraint_value(id, total).unwrap();
        solved(&mut s);
        assert!(!folded_over(&before, &s), "L {total}");
        check(&s, &ids, total, &format!("L {total}"));
    }
    // Breite 0,5…59 bei Länge 60
    s.set_constraint_value(id, 60.0).unwrap();
    solved(&mut s);
    for k in 0..1000 {
        let width = 0.5 + 58.5 * f64::from(k) / 999.0;
        let before = s.clone();
        s.set_constraint_value(w, width).unwrap();
        solved(&mut s);
        assert!(!folded_over(&before, &s), "W {width}");
        check(&s, &ids, 60.0, &format!("W {width}"));
        assert!((s.radius(ids.caps[0]).unwrap() * 2.0 - width).abs() < 1e-6);
    }
}

#[test]
fn smooth_over_a_dense_drag_grid_never_makes_a_cusp() {
    let mut base = sketch();
    let (l, a, joint) = line(&mut base, v(-10.0, 0.0), v(0.0, 0.0));
    base.add_constraint(K::Fixed(a, -10.0, 0.0)).unwrap();
    base.add_constraint(K::Fixed(joint, 0.0, 0.0)).unwrap();
    let inner = pt(&mut base, 5.0, 3.0);
    let last = pt(&mut base, 10.0, 0.0);
    base.add_constraint(K::Fixed(last, 10.0, 0.0)).unwrap();
    let sp = base.add_spline(&[joint, inner, last]).unwrap();
    let id = base.add_constraint(K::Smooth(l, joint, sp, joint)).unwrap();
    solved(&mut base);
    for i in 0..21 {
        for j in 0..21 {
            let target = v(
                2.0 + 6.0 * f64::from(i) / 20.0,
                -5.0 + 10.0 * f64::from(j) / 20.0,
            );
            let mut s = base.clone();
            let r = s
                .drag(&[Drag {
                    point: inner,
                    target,
                }])
                .unwrap();
            assert_eq!(r.status, SolveStatus::Solved, "{target}: {r:?}");
            assert!(!folded_over(&base, &s), "{target}");
            assert!(row(&s, id, 0).abs() < 1e-8, "{target}: {}", row(&s, id, 0));
            let u = into_curve(&s, l, joint);
            let w = -into_curve(&s, sp, joint);
            assert!(u.dot(w) > 0.0, "{target}: Spitze");
        }
    }
}

// ---- Überzählig, nicht widersprüchlich ---------------------------------------------------

fn redundant_and_solved(s: &mut Sketch, id: ConstraintId) {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.conflicting.is_empty(), "{r:?}");
    assert!(r.redundant.contains(&id), "{r:?}");
}

#[test]
fn already_met_new_constraints_are_reported_redundant() {
    // Kollinear nach Parallel und Punkt auf Linie
    let mut s = sketch();
    let (l1, _, _) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    let (l2, q, _) = line(&mut s, v(15.0, 1.0), v(25.0, 2.0));
    s.add_constraint(K::Parallel(l1, l2)).unwrap();
    s.add_constraint(K::PointOnCurve(q, l1)).unwrap();
    solved(&mut s);
    let id = s.add_constraint(K::Collinear(l1, l2)).unwrap();
    redundant_and_solved(&mut s, id);

    // Waagerechter Abstand an waagerechter Linie mit Länge
    let mut s = sketch();
    let (l, a, b) = line(&mut s, v(0.0, 0.0), v(10.0, 1.0));
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l)))
        .unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    solved(&mut s);
    let id = s.add_constraint(K::DistanceX(a, b, 10.0)).unwrap();
    redundant_and_solved(&mut s, id);

    // Konzentrisch transitiv
    let mut s = sketch();
    let m: Vec<PointId> = (0..3).map(|i| pt(&mut s, f64::from(i), 0.5)).collect();
    for (i, &c) in m.iter().enumerate() {
        s.add_circle(c, 2.0 + f64::from(i as u8)).unwrap();
    }
    s.add_constraint(K::Coincident(m[0], m[1])).unwrap();
    s.add_constraint(K::Coincident(m[1], m[2])).unwrap();
    solved(&mut s);
    let id = s.add_constraint(K::Coincident(m[0], m[2])).unwrap();
    redundant_and_solved(&mut s, id);

    // Senkrechter Abstand 1 an waagerechter Linie: Widerspruch
    let mut s = sketch();
    let (l, a, b) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l)))
        .unwrap();
    solved(&mut s);
    s.add_constraint(K::DistanceY(a, b, 1.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Conflicting);
}

// ---- Typprüfung --------------------------------------------------------------------------

#[test]
fn type_checks_reject_and_leave_the_sketch_unchanged() {
    let mut s = sketch();
    let (l1, a1, b1) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    let (l2, a2, _) = line(&mut s, v(15.0, 1.0), v(25.0, 2.0));
    let c = s.add_circle(a2, 3.0).unwrap();
    let (m, st, en) = (
        pt(&mut s, 0.0, 20.0),
        pt(&mut s, 5.0, 20.0),
        pt(&mut s, 0.0, 25.0),
    );
    let arc = s.add_arc(m, st, en).unwrap();
    let e = {
        let ec = pt(&mut s, 40.0, 0.0);
        s.add_ellipse(ec, v(4.0, 0.0), 2.0).unwrap()
    };
    let (sp, sp_pts) = spline(&mut s, &[v(10.0, 0.0), v(12.0, 5.0), v(14.0, 0.0)]);
    let tol = s.tolerance().linear;
    // Eine zweite Linie, die beide Punkte mit l1 teilt.
    let twin = s.add_line(b1, a1).unwrap();
    let bad = [
        K::Collinear(l1, l1),
        K::Collinear(l1, c),
        K::Collinear(c, l1),
        K::Collinear(l1, twin),
        K::Smooth(l1, b1, arc, en),
        K::Smooth(l1, a1, l2, a2),
        K::Smooth(e, b1, sp, sp_pts[0]),
        K::Smooth(sp, sp_pts[0], e, a2),
        K::Smooth(sp, sp_pts[1], l1, b1),
        K::Smooth(sp, sp_pts[0], l1, a2),
        K::Smooth(sp, sp_pts[0], sp, sp_pts[2]),
        K::CenterlineDiameter(a2, l1, 5.0),
        K::CenterlineDiameter(a2, c, 5.0),
        K::DistanceX(a1, a1, 5.0),
        K::DistanceY(a1, a1, 5.0),
        K::DistanceX(a1, b1, tol),
        K::DistanceY(a1, b1, 0.0),
        K::DistanceX(a1, b1, f64::NAN),
        K::DistanceY(a1, b1, f64::INFINITY),
        K::SlotOverall(m, m, arc, 10.0),
        K::SlotOverall(m, a2, l1, 10.0),
        K::SlotOverall(m, a2, arc, tol),
        K::SlotOverall(m, a2, arc, f64::NAN),
    ];
    let revision = s.revision();
    let count = s.constraints().count();
    for kind in bad {
        assert!(s.add_constraint(kind.clone()).is_err(), "{kind:?}");
        assert_eq!(s.revision(), revision, "{kind:?}");
        assert_eq!(s.constraints().count(), count, "{kind:?}");
    }
    // Auf einer Mittellinie gelten dieselben Wertgrenzen.
    let (axis, _, _) = line(&mut s, v(0.0, -10.0), v(10.0, -10.0));
    s.set_centerline(axis, true).unwrap();
    let revision = s.revision();
    for value in [0.0, tol, f64::NAN, -2.0] {
        let kind = K::CenterlineDiameter(a2, axis, value);
        assert!(s.add_constraint(kind.clone()).is_err(), "{kind:?}");
        assert_eq!(s.revision(), revision);
    }
    // Die gültigen Gegenstücke gehen.
    for kind in [
        K::Collinear(l1, l2),
        K::Smooth(l1, b1, sp, sp_pts[0]),
        K::Smooth(arc, en, sp, sp_pts[2]),
        K::CenterlineDiameter(a2, axis, 5.0),
        K::DistanceX(a1, b1, 10.0),
        K::SlotOverall(m, a2, arc, 10.0),
    ] {
        s.add_constraint(kind.clone())
            .unwrap_or_else(|e| panic!("{kind:?}: {e}"));
    }
    // Ein Wert über set_constraint_value wird genauso geprüft.
    let id = s.add_constraint(K::DistanceY(a1, b1, 2.0)).unwrap();
    let revision = s.revision();
    assert!(s.set_constraint_value(id, 0.0).is_err());
    assert!(s.set_constraint_value(id, f64::NAN).is_err());
    assert_eq!(s.revision(), revision);
}

#[test]
fn new_kinds_need_format_15_and_old_ones_do_not() {
    let mut s = sketch();
    let (l1, a, b) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    let (l2, _, _) = line(&mut s, v(15.0, 1.0), v(25.0, 2.0));
    let new = [
        K::Collinear(l1, l2),
        K::Smooth(l1, b, l2, a),
        K::DistanceX(a, b, 1.0),
        K::DistanceY(a, b, 1.0),
        K::CenterlineDiameter(a, l1, 1.0),
        K::SlotOverall(a, b, l1, 1.0),
    ];
    assert!(new.iter().all(K::needs_format_15));
    let old = [
        K::Coincident(a, b),
        K::Parallel(l1, l2),
        K::Distance(a, b, 1.0),
        K::DistancePointLine(a, l1, 1.0),
        K::Horizontal(LineOrPoints::Line(l1)),
        K::Tangent(l1, l2),
    ];
    assert!(!old.iter().any(K::needs_format_15));
    assert!(!K::Collinear(l1, l2).is_dimension() && !K::Smooth(l1, b, l2, a).is_dimension());
}

// ---- Enden für Glatt ---------------------------------------------------------------------

#[test]
fn smooth_ends_finds_shared_or_nearest_ends() {
    let mut s = sketch();
    let (l, _, joint) = line(&mut s, v(-10.0, 0.0), v(0.0, 0.0));
    let q = [pt(&mut s, 5.0, 3.0), pt(&mut s, 10.0, 0.0)];
    let sp = s.add_spline(&[joint, q[0], q[1]]).unwrap();
    assert_eq!(s.smooth_ends(l, None, sp, None), Some((joint, joint)));
    assert_eq!(
        s.smooth_ends(sp, Some(v(0.5, 0.2)), l, None),
        Some((joint, joint))
    );

    // getrennt: das nächste Endenpaar, in beiden Reihenfolgen
    let mut s = sketch();
    let (l, la, lb) = line(&mut s, v(-10.0, 0.0), v(-1.0, 0.0));
    let (sp, p) = spline(&mut s, &[v(0.0, 0.5), v(5.0, 3.0), v(10.0, 0.0)]);
    assert_eq!(s.smooth_ends(l, None, sp, None), Some((lb, p[0])));
    assert_eq!(s.smooth_ends(sp, None, l, None), Some((p[0], lb)));
    assert_eq!(
        s.smooth_ends(l, Some(v(-0.5, 0.2)), sp, None),
        Some((lb, p[0]))
    );
    // Der Klick wählt das Ende, auch weit weg (Fusion: das Ende nächst dem Klick).
    assert_eq!(
        s.smooth_ends(l, Some(v(-9.5, 0.0)), sp, None),
        Some((la, p[0]))
    );
    assert_eq!(
        s.smooth_ends(l, Some(v(-9.5, 0.0)), sp, Some(v(9.0, 0.5))),
        Some((la, p[2]))
    );
    // Kurven ohne Enden und dieselbe Kurve
    let m = pt(&mut s, 50.0, 50.0);
    let c = s.add_circle(m, 2.0).unwrap();
    assert_eq!(s.smooth_ends(c, None, sp, None), None);
    assert_eq!(s.smooth_ends(sp, None, sp, None), None);
}

// ---- Umschalter Radius/Durchmesser -------------------------------------------------------

#[test]
fn toggle_radius_diameter_doubles_and_halves_in_place() {
    let mut s = sketch();
    let m = pt(&mut s, 0.0, 0.0);
    let c = s.add_circle(m, 3.0).unwrap();
    let id = s.add_constraint(K::Radius(c, 3.0)).unwrap();
    let before = s.constraint(id).unwrap().kind.clone();
    s.toggle_radius_diameter(id).unwrap();
    assert_eq!(s.constraint(id).unwrap().kind, K::Diameter(c, 6.0));
    s.toggle_radius_diameter(id).unwrap();
    assert_eq!(s.constraint(id).unwrap().kind, before);

    let (axis, _, _) = line(&mut s, v(0.0, 10.0), v(10.0, 10.0));
    s.set_centerline(axis, true).unwrap();
    let p = pt(&mut s, 3.0, 6.7);
    let id = s
        .add_constraint(K::DistancePointLine(p, axis, 3.3))
        .unwrap();
    let branch = s.constraint(id).unwrap().branch();
    assert_eq!(branch, -1);
    let before = s.constraint(id).unwrap().kind.clone();
    s.toggle_radius_diameter(id).unwrap();
    assert_eq!(
        s.constraint(id).unwrap().kind,
        K::CenterlineDiameter(p, axis, 6.6)
    );
    assert_eq!(s.constraint(id).unwrap().branch(), branch);
    solved(&mut s);
    assert!((at(&s, p).y - 6.7).abs() < 1e-9);
    s.toggle_radius_diameter(id).unwrap();
    let after = s.constraint(id).unwrap().kind.clone();
    match (&before, &after) {
        (K::DistancePointLine(_, _, x), K::DistancePointLine(_, _, y)) => {
            assert_eq!(x.to_bits(), y.to_bits());
        }
        _ => panic!("{after:?}"),
    }
    assert_eq!(s.constraint(id).unwrap().branch(), branch);

    // an einer normalen Linie geht es nicht, und an anderen Arten auch nicht
    let (plain, _, _) = line(&mut s, v(0.0, 30.0), v(10.0, 30.0));
    let id = s
        .add_constraint(K::DistancePointLine(p, plain, 23.3))
        .unwrap();
    let revision = s.revision();
    assert!(s.toggle_radius_diameter(id).is_err());
    let (a, b) = (pt(&mut s, 0.0, 50.0), pt(&mut s, 5.0, 50.0));
    let other = s.add_constraint(K::Distance(a, b, 5.0)).unwrap();
    let revision2 = s.revision();
    assert!(s.toggle_radius_diameter(other).is_err());
    assert_eq!(s.revision(), revision2);
    assert!(revision2 > revision);
    assert_eq!(
        s.constraint(id).unwrap().kind,
        K::DistancePointLine(p, plain, 23.3)
    );
}

// ---- Verschieben, Drehen, Skalieren, Kopieren --------------------------------------------

struct All {
    s: Sketch,
    collinear: ConstraintId,
    smooth: ConstraintId,
    dx: ConstraintId,
    dy: ConstraintId,
    diameter: ConstraintId,
    overall: ConstraintId,
}

fn all_kinds() -> All {
    let mut s = sketch();
    let (l1, _, _) = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    let (l2, _, _) = line(&mut s, v(15.0, 0.0), v(25.0, 0.0));
    let collinear = s.add_constraint(K::Collinear(l1, l2)).unwrap();
    let (l, _, joint) = line(&mut s, v(-10.0, 20.0), v(0.0, 20.0));
    let q = [pt(&mut s, 5.0, 23.0), pt(&mut s, 10.0, 20.0)];
    let sp = s.add_spline(&[joint, q[0], q[1]]).unwrap();
    let smooth = s.add_constraint(K::Smooth(l, joint, sp, joint)).unwrap();
    let (_, a, b) = line(&mut s, v(30.0, 0.0), v(37.0, 4.0));
    let dx = s.add_constraint(K::DistanceX(a, b, 7.0)).unwrap();
    let dy = s.add_constraint(K::DistanceY(a, b, 4.0)).unwrap();
    let (axis, _, _) = line(&mut s, v(0.0, -20.0), v(10.0, -20.0));
    s.set_centerline(axis, true).unwrap();
    let p = pt(&mut s, 3.0, -16.0);
    let diameter = s
        .add_constraint(K::CenterlineDiameter(p, axis, 8.0))
        .unwrap();
    let plan = SlotPlan {
        a: v(50.0, 0.0),
        b: v(60.0, 0.0),
        radius: 2.0,
    };
    let ids = s.add_slot(&plan, SlotStyle::CenterToCenter).unwrap();
    let overall = s
        .add_constraint(K::SlotOverall(
            ids.centers[0],
            ids.centers[1],
            ids.caps[0],
            14.0,
        ))
        .unwrap();
    solved(&mut s);
    All {
        s,
        collinear,
        smooth,
        dx,
        dy,
        diameter,
        overall,
    }
}

fn value(s: &Sketch, id: ConstraintId) -> f64 {
    match s.constraint(id).unwrap().kind {
        K::DistanceX(_, _, x)
        | K::DistanceY(_, _, x)
        | K::CenterlineDiameter(_, _, x)
        | K::SlotOverall(_, _, _, x) => x,
        ref k => panic!("{k:?}"),
    }
}

#[test]
fn moving_turning_and_scaling_keep_or_drop_the_new_kinds() {
    let all = all_kinds();
    let ids = [
        all.collinear,
        all.smooth,
        all.dx,
        all.dy,
        all.diameter,
        all.overall,
    ];

    // Verschieben: alles bleibt.
    let mut s = all.s.clone();
    let set = s.whole_sketch_set();
    let shift = Similarity {
        offset: v(3.0, -2.0),
        ..Similarity::identity(DVec2::ZERO)
    };
    let report = s.apply_transform(&set, &shift).unwrap();
    assert!(report.dropped.is_empty(), "{report:?}");
    assert!(ids.iter().all(|&id| s.constraint(id).is_some()));
    solved(&mut s);

    // Drehen um 30°: waagerecht und senkrecht entfallen, der Rest bleibt erfüllt.
    let mut s = all.s.clone();
    let set = s.whole_sketch_set();
    let turn = Similarity {
        angle: 30f64.to_radians(),
        ..Similarity::identity(v(5.0, 5.0))
    };
    let report = s.apply_transform(&set, &turn).unwrap();
    assert!(report.dropped.contains(&all.dx), "{report:?}");
    assert!(report.dropped.contains(&all.dy), "{report:?}");
    assert_eq!(report.dropped.len(), 2, "{report:?}");
    for id in [all.collinear, all.smooth, all.diameter, all.overall] {
        assert!(s.constraint(id).is_some(), "{id:?}");
    }
    let before = s.clone();
    let r = solved(&mut s);
    assert!(r.redundant.is_empty());
    assert!(!folded_over(&before, &s));
    for id in [all.collinear, all.smooth] {
        assert!(row(&s, id, 0).abs() < 1e-8);
    }

    // Skalieren mit k = 2: Längen mal 2.
    let mut s = all.s.clone();
    let set = s.whole_sketch_set();
    let grow = Similarity {
        factor: 2.0,
        ..Similarity::identity(DVec2::ZERO)
    };
    let report = s.apply_transform(&set, &grow).unwrap();
    assert!(report.dropped.is_empty(), "{report:?}");
    for (id, was) in [
        (all.dx, 7.0),
        (all.dy, 4.0),
        (all.diameter, 8.0),
        (all.overall, 14.0),
    ] {
        assert!(report.scaled.contains(&id), "{id:?}");
        assert_eq!(value(&s, id), 2.0 * was);
    }
    let before = s.clone();
    solved(&mut s);
    // Nichts musste sich bewegen.
    for (p, _) in before.points() {
        assert!((at(&s, p) - at(&before, p)).length() < 1e-9);
    }

    // Gedrehte Kopie: waagerecht und senkrecht fehlen, Kollinear und Glatt sind dabei.
    let mut s = all.s.clone();
    let set = s.whole_sketch_set();
    let copy = s.copy_transformed(&set, &turn).unwrap();
    assert!(!copy.constraints.contains_key(&all.dx));
    assert!(!copy.constraints.contains_key(&all.dy));
    for id in [all.collinear, all.smooth, all.diameter, all.overall] {
        let new = copy.constraints[&id];
        assert!(s.constraint(new).is_some());
    }
    assert!(matches!(
        s.constraint(copy.constraints[&all.collinear]).unwrap().kind,
        K::Collinear(..)
    ));
    assert!(matches!(
        s.constraint(copy.constraints[&all.smooth]).unwrap().kind,
        K::Smooth(..)
    ));
    let r = solved(&mut s);
    assert!(r.redundant.is_empty(), "{r:?}");
}

// ---- Glatt nach Stutzen, Brechen und Rundung (Korrektur 1) -------------------------------

/// Linie bis zur Verbindung, Spline ab dort, Glatt an der Verbindung und eine senkrechte Linie,
/// die die erste bei x = −5 kreuzt. `joint_first`: die Verbindung ist Punkt `a` der Linie.
fn smooth_joint(joint_first: bool) -> (Sketch, CurveId, CurveId, PointId, ConstraintId) {
    let mut s = sketch();
    let (far, joint) = (pt(&mut s, -10.0, 0.0), pt(&mut s, 0.0, 0.0));
    let l = if joint_first {
        s.add_line(joint, far).unwrap()
    } else {
        s.add_line(far, joint).unwrap()
    };
    let q = [pt(&mut s, 5.0, 3.0), pt(&mut s, 10.0, 0.0)];
    let sp = s.add_spline(&[joint, q[0], q[1]]).unwrap();
    let id = s.add_constraint(K::Smooth(l, joint, sp, joint)).unwrap();
    line(&mut s, v(-5.0, -3.0), v(-5.0, 3.0));
    solved(&mut s);
    (s, l, sp, joint, id)
}

/// Glatt ist weg oder gilt noch an echten Enden; die Skizze lädt nach dem Speichern wieder.
fn smooth_gone_or_valid(s: &Sketch, id: ConstraintId, what: &str) {
    if let Some(d) = s.constraint(id) {
        let K::Smooth(a, pa, b, pb) = d.kind else {
            panic!("{what}: {:?}", d.kind)
        };
        assert_eq!(s.smooth_ends(a, None, b, None), Some((pa, pb)), "{what}");
    }
    for (cid, d) in s.constraints() {
        if let K::Smooth(a, pa, b, pb) = d.kind {
            assert_eq!(
                s.smooth_ends(a, None, b, None),
                Some((pa, pb)),
                "{what}: {cid:?}"
            );
        }
    }
    Sketch::from_data(s.to_data()).unwrap_or_else(|e| panic!("{what}: lädt nicht: {e}"));
}

#[test]
fn smooth_does_not_keep_a_stale_end_after_trim_break_or_fillet() {
    for joint_first in [false, true] {
        // Stück zwischen Kreuzung und Verbindung weg: Glatt entfällt.
        let (mut s, l, _, _, id) = smooth_joint(joint_first);
        s.trim(l, v(-2.5, 0.0)).unwrap();
        assert!(s.constraint(id).is_none(), "{joint_first}");
        smooth_gone_or_valid(&s, id, "trim innen");
        // Das äußere Stück weg: Glatt bleibt an der Verbindung.
        let (mut s, l, _, _, id) = smooth_joint(joint_first);
        s.trim(l, v(-7.5, 0.0)).unwrap();
        assert!(s.constraint(id).is_some(), "{joint_first}");
        smooth_gone_or_valid(&s, id, "trim außen");
        solved(&mut s);
        // Brechen an der Kreuzung
        let (mut s, l, _, _, id) = smooth_joint(joint_first);
        s.break_curve(l, v(-5.0, 0.0)).unwrap();
        smooth_gone_or_valid(&s, id, "break");
        solved(&mut s);
        // Rundung an der Verbindung mit einer zweiten Linie
        let (mut s, l, _, joint, id) = smooth_joint(joint_first);
        let down = pt(&mut s, 0.0, -10.0);
        let m = s.add_line(joint, down).unwrap();
        solved(&mut s);
        let plan = s
            .fillet_plan(
                planora_sketch::CurvePick {
                    curve: l,
                    at: v(-3.0, 0.0),
                },
                planora_sketch::CurvePick {
                    curve: m,
                    at: v(0.0, -3.0),
                },
                1.0,
            )
            .expect("Plan");
        s.add_fillet(&plan, None).unwrap();
        assert!(s.constraint(id).is_none(), "{joint_first}");
        smooth_gone_or_valid(&s, id, "fillet");
    }
}
