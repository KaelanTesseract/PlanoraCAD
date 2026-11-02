//! Rundung und Fase mit ihren Abhängigkeiten (Skizzen 4, Task 2): Freiheitsgrade wie gemessen,
//! Vorschau gleich Ergebnis, dichte Wertefolgen statt Stichproben.

use planora_math::{DVec2, Frame, PI, Tolerance};
use planora_sketch::constraint::{ConstraintId, ConstraintKind, LineOrPoints};
use planora_sketch::fillet::{
    ChamferIds, ChamferKind, ChamferPlan, CurvePick, EndMove, FilletIds, FilletPlan,
};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::shapes::folded_over;
use planora_sketch::solver::{SolveReport, SolveStatus};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn pt(s: &mut Sketch, x: f64, y: f64) -> PointId {
    s.add_point(x, y).unwrap()
}

fn pick(curve: CurveId, x: f64, y: f64) -> CurvePick {
    CurvePick {
        curve,
        at: DVec2::new(x, y),
    }
}

fn at(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn dof(s: &Sketch) -> usize {
    s.clone().solve().dof
}

fn has(s: &Sketch, kind: ConstraintKind) -> bool {
    s.constraints().any(|(_, d)| d.kind == kind)
}

fn line_ends(s: &Sketch, l: CurveId) -> (PointId, PointId) {
    match s.curve(l).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        ref k => panic!("keine Linie: {k:?}"),
    }
}

/// `Solved`, nichts überzählig, kein Widerspruch.
fn assert_clean(r: &SolveReport) {
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.redundant.is_empty(), "überzählig: {r:?}");
    assert!(r.conflicting.is_empty(), "Widerspruch: {r:?}");
}

/// Wie [`assert_clean`], und beim Lösen bewegt sich nichts.
fn assert_still(s: &Sketch) -> SolveReport {
    let mut solved = s.clone();
    let r = solved.solve();
    assert_clean(&r);
    assert_eq!(r.iterations, 0, "{r:?}");
    assert_eq!(solved.variables(), s.variables());
    r
}

/// Zwei Linien mit gemeinsamer Ecke im Ursprung, Länge 40, Winkel `phi` zwischen ihnen.
struct Vee {
    s: Sketch,
    a: PointId,
    p: PointId,
    b: PointId,
    l1: CurveId,
    l2: CurveId,
}

fn vee(phi: f64) -> Vee {
    let mut s = sketch();
    let p = pt(&mut s, 0.0, 0.0);
    let a = pt(&mut s, 40.0, 0.0);
    let b = pt(&mut s, 40.0 * phi.cos(), 40.0 * phi.sin());
    let l1 = s.add_line(a, p).unwrap();
    let l2 = s.add_line(p, b).unwrap();
    Vee { s, a, p, b, l1, l2 }
}

fn picks(v: &Vee) -> (CurvePick, CurvePick) {
    (
        CurvePick {
            curve: v.l1,
            at: at(&v.s, v.a) * 0.5,
        },
        CurvePick {
            curve: v.l2,
            at: at(&v.s, v.b) * 0.5,
        },
    )
}

fn round(v: &mut Vee, r: f64) -> (FilletPlan, FilletIds) {
    let (a, b) = picks(v);
    let plan = v.s.fillet_plan(a, b, r).expect("Plan");
    let ids = v.s.add_fillet(&plan, None).unwrap();
    (plan, ids)
}

fn chamfer(v: &mut Vee, kind: ChamferKind) -> (ChamferPlan, ChamferIds) {
    let (a, b) = picks(v);
    let plan = v.s.chamfer_plan(a, b, kind).expect("Plan");
    let ids = v.s.add_chamfer(&plan).unwrap();
    (plan, ids)
}

/// Größter Winkelfehler (Kosinus Radius–Linie) an den Berührpunkten zweier Linien.
fn tangent_error(s: &Sketch, ids: &FilletIds, lines: [CurveId; 2]) -> f64 {
    let m = at(s, ids.center);
    let mut worst = 0.0_f64;
    for (i, &line) in lines.iter().enumerate() {
        let (a, b) = line_ends(s, line);
        let d = (at(s, b) - at(s, a)).normalize();
        let radial = (at(s, ids.touch[i]) - m).normalize();
        worst = worst.max(radial.dot(d).abs());
    }
    worst
}

/// Setzt ein Längenmaß wie der Falt-Wächter der Oberfläche: direkt, sonst in Schritten.
/// `None`: abgelehnt.
fn change(s: &Sketch, id: ConstraintId, old: f64, new: f64) -> Option<Sketch> {
    let mut trial = s.clone();
    trial.set_constraint_value(id, new).unwrap();
    let unsolved = trial.clone();
    let r = trial.solve();
    if r.status == SolveStatus::Solved && !folded_over(&unsolved, &trial) {
        return Some(trial);
    }
    let accept =
        |t: &Sketch, r: &SolveReport| r.status == SolveStatus::Solved && !folded_over(&unsolved, t);
    unsolved
        .solve_in_steps(&[(id, old, new)], accept)
        .map(|(done, _)| done)
}

// ---- Tabelle der Freiheitsgrade -------------------------------------------------------------

#[test]
fn line_line_keeps_the_degrees_of_freedom_and_drops_a_bare_corner() {
    let mut v = vee(1.2);
    let before = dof(&v.s);
    let (plan, ids) = round(&mut v, 5.0);
    assert!(plan.exact);
    assert_eq!(assert_still(&v.s).dof, before);
    assert_eq!(ids.corner, None);
    assert!(v.s.point_xy(v.p).is_err(), "die Ecke ohne Bezug entfällt");
    assert_eq!(ids.helper, None);
    assert!(ids.radius_dim.is_some());
    let arc = v.s.curve(ids.arc).unwrap();
    assert!(matches!(arc.kind, CurveKind::Arc { center, .. } if center == ids.center));
    assert_eq!(line_ends(&v.s, v.l1), (v.a, ids.touch[0]));
    assert_eq!(line_ends(&v.s, v.l2), (ids.touch[1], v.b));
    assert!(has(&v.s, ConstraintKind::Tangent(v.l1, ids.arc)));
    assert!(has(&v.s, ConstraintKind::Tangent(v.l2, ids.arc)));
}

#[test]
fn a_dimension_on_the_corner_survives_with_the_same_id() {
    let mut v = vee(1.2);
    let dim =
        v.s.add_constraint(ConstraintKind::Distance(v.a, v.p, 40.0))
            .unwrap();
    let before = dof(&v.s);
    let (_, ids) = round(&mut v, 5.0);
    assert_eq!(assert_still(&v.s).dof, before);
    assert_eq!(ids.corner, Some(v.p));
    assert!(v.s.points().any(|(p, d)| p == v.p && d.construction));
    assert!(matches!(
        v.s.constraint(dim).unwrap().kind,
        ConstraintKind::Distance(a, p, _) if a == v.a && p == v.p
    ));
    assert!(has(&v.s, ConstraintKind::PointOnCurve(v.p, v.l1)));
    assert!(has(&v.s, ConstraintKind::PointOnCurve(v.p, v.l2)));
    // Das Maß zählt: die Folge aus Entscheidung 1, 30 bis 70.
    for i in 0..=400 {
        let d = 30.0 + 0.1 * f64::from(i);
        let next = change(&v.s, dim, 40.0, d).unwrap_or_else(|| panic!("Maß {d}"));
        assert!((at(&next, v.a).distance(at(&next, v.p)) - d).abs() < 1e-6);
        assert!(tangent_error(&next, &ids, [v.l1, v.l2]) < 1e-6, "Maß {d}");
        assert!(!folded_over(&v.s, &next), "Maß {d}");
    }
}

/// Linie von `(-30, 0)` in die Ecke `(0, 0)`, die das Ende eines Bogens um `(20, 0)` ist.
fn line_arc() -> (Sketch, CurveId, CurveId, PointId) {
    let mut s = sketch();
    let m = pt(&mut s, 20.0, 0.0);
    let start = pt(&mut s, 20.0, -20.0);
    let corner = pt(&mut s, 0.0, 0.0);
    let arc = s.add_arc(m, start, corner).unwrap();
    let a = pt(&mut s, -30.0, 0.0);
    let line = s.add_line(a, corner).unwrap();
    (s, line, arc, corner)
}

fn line_arc_picks(line: CurveId, arc: CurveId) -> (CurvePick, CurvePick) {
    let u = 0.75 * PI;
    (
        pick(line, -15.0, 0.0),
        pick(arc, 20.0 + 20.0 * u.cos(), 20.0 * u.sin()),
    )
}

#[test]
fn line_arc_keeps_the_degrees_of_freedom_over_the_radius_sequence() {
    let (mut s, line, arc, corner) = line_arc();
    let before = dof(&s);
    let (a, b) = line_arc_picks(line, arc);
    let plan = s.fillet_plan(a, b, 4.0).expect("Plan");
    let ids = s.add_fillet(&plan, None).unwrap();
    assert_eq!(assert_still(&s).dof, before);
    assert!(s.point_xy(corner).is_err());
    assert_eq!(ids.corner, None);
    let radius = ids.radius_dim.unwrap();
    for i in 1..=120 {
        let r = 0.1 * f64::from(i);
        let next = change(&s, radius, 4.0, r).unwrap_or_else(|| panic!("r {r}"));
        assert!((next.radius(ids.arc).unwrap() - r).abs() < 1e-6);
        assert!(!folded_over(&s, &next), "r {r}");
    }
}

#[test]
fn line_arc_with_a_dimension_on_the_corner_keeps_it() {
    let (mut s, line, arc, corner) = line_arc();
    let far = pt(&mut s, 0.0, 50.0);
    let dim = s
        .add_constraint(ConstraintKind::Distance(far, corner, 50.0))
        .unwrap();
    let before = dof(&s);
    let (a, b) = line_arc_picks(line, arc);
    let plan = s.fillet_plan(a, b, 4.0).expect("Plan");
    let ids = s.add_fillet(&plan, None).unwrap();
    assert_eq!(assert_still(&s).dof, before);
    assert_eq!(ids.corner, Some(corner));
    assert!(s.constraint(dim).is_some());
    assert!(has(&s, ConstraintKind::PointOnCurve(corner, line)));
    assert!(has(&s, ConstraintKind::PointOnCurve(corner, arc)));
}

#[test]
fn arc_arc_keeps_the_degrees_of_freedom_over_the_radius_sequence() {
    let mut s = sketch();
    let m1 = pt(&mut s, -20.0, 0.0);
    let s1 = pt(&mut s, -20.0, -20.0);
    let corner = pt(&mut s, 0.0, 0.0);
    let a1 = s.add_arc(m1, s1, corner).unwrap();
    let m2 = pt(&mut s, 0.0, 20.0);
    let e2 = pt(&mut s, 20.0, 20.0);
    let a2 = s.add_arc(m2, corner, e2).unwrap();
    let before = dof(&s);
    let u = 0.25_f64 * PI;
    let plan = s
        .fillet_plan(
            pick(a1, -20.0 + 20.0 * u.cos(), -20.0 * u.sin()),
            pick(a2, 20.0 * u.sin(), 20.0 - 20.0 * u.cos()),
            4.0,
        )
        .expect("Plan");
    let ids = s.add_fillet(&plan, None).unwrap();
    assert_eq!(assert_still(&s).dof, before);
    let radius = ids.radius_dim.unwrap();
    for i in 1..=120 {
        let r = 0.1 * f64::from(i);
        let next = change(&s, radius, 4.0, r).unwrap_or_else(|| panic!("r {r}"));
        assert!((next.radius(ids.arc).unwrap() - r).abs() < 1e-6);
        assert!(!folded_over(&s, &next), "r {r}");
    }
}

/// Waagerechte Linie von `(50, 16)` nach `(12, 16)`, die auf dem Kreis endet (Mitte im Ursprung,
/// Radius 20).
fn line_circle() -> (Sketch, CurveId, CurveId, PointId, PointId) {
    let mut s = sketch();
    let m = pt(&mut s, 0.0, 0.0);
    let k = s.add_circle(m, 20.0).unwrap();
    let a = pt(&mut s, 50.0, 16.0);
    let e = pt(&mut s, 12.0, 16.0);
    let l = s.add_line(a, e).unwrap();
    (s, l, k, m, e)
}

#[test]
fn line_circle_is_one_less_than_a_free_end_and_even_with_a_line_on_the_circle() {
    let (mut s, l, k, m, e) = line_circle();
    let before = dof(&s);
    let plan = s
        .fillet_plan(pick(l, 30.0, 16.0), pick(k, 20.0, 0.0), 4.0)
        .expect("Plan");
    assert_eq!(plan.moves[1], EndMove::Whole);
    let ids = s.add_fillet(&plan, None).unwrap();
    assert_eq!(assert_still(&s).dof + 1, before);
    assert!(s.point_xy(e).is_err());
    let helper = ids.helper.expect("Hilfslinie");
    assert!(s.curve(helper).unwrap().construction);
    assert_eq!(line_ends(&s, helper), (m, ids.center));
    assert!(has(&s, ConstraintKind::PointOnCurve(ids.touch[1], helper)));

    let (mut s, l, k, _, e) = line_circle();
    s.add_constraint(ConstraintKind::PointOnCurve(e, k))
        .unwrap();
    let before = dof(&s);
    let plan = s
        .fillet_plan(pick(l, 30.0, 16.0), pick(k, 20.0, 0.0), 4.0)
        .expect("Plan");
    s.add_fillet(&plan, None).unwrap();
    assert_eq!(assert_still(&s).dof, before);
}

#[test]
fn line_circle_radius_sequence_stays_solved_with_a_free_line() {
    let (mut s, l, k, _, _) = line_circle();
    let plan = s
        .fillet_plan(pick(l, 30.0, 16.0), pick(k, 20.0, 0.0), 4.0)
        .expect("Plan");
    let ids = s.add_fillet(&plan, None).unwrap();
    let radius = ids.radius_dim.unwrap();
    let mut solved = 0;
    for i in 1..=300 {
        let r = 0.1 * f64::from(i);
        match change(&s, radius, 4.0, r) {
            Some(next) => {
                solved += 1;
                assert!((next.radius(ids.arc).unwrap() - r).abs() < 1e-6);
                assert!(!folded_over(&s, &next), "r {r}");
                let circle_m = at(&next, ids.helper.map(|h| line_ends(&next, h).0).unwrap());
                let (m, t) = (at(&next, ids.center), at(&next, ids.touch[1]));
                // Berührpunkt auf dem Kreis und auf der Verbindung der Mitten
                assert!((t.distance(circle_m) - next.radius(k).unwrap()).abs() < 1e-6);
                let (u, w) = ((m - t).normalize(), (t - circle_m).normalize());
                assert!(u.perp_dot(w).abs() < 1e-6, "r {r}");
            }
            None => panic!("r {r} abgelehnt"),
        }
    }
    assert_eq!(solved, 300);
}

#[test]
fn a_projected_line_with_a_fixed_near_end_stays_whole() {
    let mut s = sketch();
    let f0 = pt(&mut s, 0.0, 0.0);
    let f1 = pt(&mut s, 200.0, 0.0);
    let fixed = s.add_line(f0, f1).unwrap();
    s.add_constraint(ConstraintKind::Fixed(f0, 0.0, 0.0))
        .unwrap();
    s.add_constraint(ConstraintKind::Fixed(f1, 200.0, 0.0))
        .unwrap();
    let a = pt(&mut s, 30.0, 80.0);
    let e = pt(&mut s, 30.0, 1.0);
    let free = s.add_line(a, e).unwrap();
    let before = dof(&s);
    let plan = s
        .fillet_plan(pick(fixed, 10.0, 0.0), pick(free, 30.0, 40.0), 4.0)
        .expect("der Plan bleibt gültig");
    assert_eq!(plan.moves[0], EndMove::Whole);
    assert_eq!(plan.moves[1], EndMove::Replace(e));
    let ids = s.add_fillet(&plan, None).unwrap();
    assert_eq!(assert_still(&s).dof + 1, before);
    assert_eq!(line_ends(&s, fixed), (f0, f1), "nicht umgehängt");
    let helper = ids.helper.expect("Hilfslinie");
    assert!(s.curve(helper).unwrap().construction);
    assert_eq!(line_ends(&s, helper), (ids.center, ids.touch[0]));
    assert!(has(&s, ConstraintKind::Perpendicular(fixed, helper)));
    assert!(has(&s, ConstraintKind::Tangent(fixed, ids.arc)));
    let radius = ids.radius_dim.unwrap();
    for i in 1..=300 {
        let r = 0.1 * f64::from(i);
        let next = change(&s, radius, 4.0, r).unwrap_or_else(|| panic!("r {r}"));
        assert!(!folded_over(&s, &next), "r {r}");
        assert!(at(&next, ids.touch[0]).y.abs() < 1e-6, "r {r}");
        assert!(at(&next, f0).distance(DVec2::ZERO) < 1e-6, "r {r}");
        assert!(
            at(&next, f1).distance(DVec2::new(200.0, 0.0)) < 1e-6,
            "r {r}"
        );
    }
}

/// Bleiben beide Kurven ganz, braucht jede ihre Hilfslinie; `helper` nennt die der ersten.
#[test]
fn two_circles_stay_whole_with_two_helpers() {
    let mut s = sketch();
    let m1 = pt(&mut s, -20.0, 0.0);
    let k1 = s.add_circle(m1, 15.0).unwrap();
    let m2 = pt(&mut s, 20.0, 0.0);
    let k2 = s.add_circle(m2, 15.0).unwrap();
    // Die Kreise schneiden sich nicht: Der Bogen liegt zwischen ihnen, oberhalb der Mitten.
    let before = dof(&s);
    let plan = s
        .fillet_plan(pick(k1, -10.0, 11.0), pick(k2, 10.0, 11.0), 10.0)
        .expect("Plan");
    assert_eq!(plan.moves, [EndMove::Whole, EndMove::Whole]);
    let ids = s.add_fillet(&plan, None).unwrap();
    assert_eq!(assert_still(&s).dof, before);
    let first = ids.helper.expect("Hilfslinie");
    assert_eq!(line_ends(&s, first), (m1, ids.center));
    assert!(has(&s, ConstraintKind::PointOnCurve(ids.touch[0], first)));
    assert!(s.curves().any(|(c, d)| {
        d.construction
            && matches!(d.kind, CurveKind::Line { a, b } if a == m2 && b == ids.center)
            && has(&s, ConstraintKind::PointOnCurve(ids.touch[1], c))
    }));
}

#[test]
fn every_chamfer_kind_keeps_the_degrees_of_freedom() {
    for kind in [
        ChamferKind::Equal(5.0),
        ChamferKind::TwoDistances(5.0, 8.0),
        ChamferKind::DistanceAngle(5.0, 0.6),
    ] {
        let mut v = vee(1.2);
        let before = dof(&v.s);
        let (_, ids) = chamfer(&mut v, kind);
        assert_eq!(assert_still(&v.s).dof, before, "{kind:?}");
        assert_eq!(ids.corner, v.p);
        assert!(v.s.points().any(|(p, d)| p == v.p && d.construction));
        assert!(has(&v.s, ConstraintKind::PointOnCurve(v.p, v.l1)));
        assert!(has(&v.s, ConstraintKind::PointOnCurve(v.p, v.l2)));
        assert_eq!(line_ends(&v.s, ids.line), (ids.touch[0], ids.touch[1]));
        match kind {
            ChamferKind::Equal(_) => {
                assert_eq!(ids.dims.len(), 1);
                let [h1, h2] = ids.helpers.expect("Hilfslinien");
                assert!(v.s.curve(h1).unwrap().construction);
                assert!(v.s.curve(h2).unwrap().construction);
                assert!(has(&v.s, ConstraintKind::Equal(h1, h2)));
            }
            _ => {
                assert_eq!(ids.dims.len(), 2);
                assert_eq!(ids.helpers, None);
            }
        }
    }
}

#[test]
fn a_chamfer_without_a_common_end_gets_a_corner_point() {
    let mut s = sketch();
    let a = pt(&mut s, 40.0, 0.0);
    let e1 = pt(&mut s, 3.0, 0.0);
    let l1 = s.add_line(a, e1).unwrap();
    let b = pt(&mut s, 0.0, 40.0);
    let e2 = pt(&mut s, 0.0, 3.0);
    let l2 = s.add_line(e2, b).unwrap();
    let before = dof(&s);
    let plan = s
        .chamfer_plan(
            pick(l1, 20.0, 0.0),
            pick(l2, 0.0, 20.0),
            ChamferKind::Equal(5.0),
        )
        .expect("Plan");
    let ids = s.add_chamfer(&plan).unwrap();
    assert!(at(&s, ids.corner).length() < 1e-12);
    assert!(s.point_xy(e1).is_err());
    assert!(s.point_xy(e2).is_err());
    // Zwei freie Enden entfallen gegen die Ecke: a, b, T1, T2, P mit vier Zeilen.
    assert_eq!(assert_still(&s).dof, before - 2);
}

// ---- Vorschau gleich Ergebnis ---------------------------------------------------------------

#[test]
fn the_points_come_from_the_plan_bit_for_bit() {
    for deg in [20.0_f64, 60.0, 90.0, 135.0, 170.0] {
        let mut v = vee(deg.to_radians());
        let (plan, ids) = round(&mut v, 3.3);
        for i in 0..2 {
            assert_eq!(at(&v.s, ids.touch[i]), plan.touch[i]);
        }
        assert_eq!(at(&v.s, ids.center), plan.arc.center);
        let CurveKind::Arc { start, end, .. } = v.s.curve(ids.arc).unwrap().kind else {
            panic!()
        };
        assert_eq!(at(&v.s, start), plan.arc.start);
        assert_eq!(at(&v.s, end), plan.arc.end);
        // und bleiben nach dem Lösen, wo sie sind
        let mut solved = v.s.clone();
        assert_clean(&solved.solve());
        for i in 0..2 {
            assert!(at(&solved, ids.touch[i]).distance(plan.touch[i]) < 1e-12);
        }
        let half = plan.arc.sweep() / 2.0;
        let (m, s0) = (
            at(&solved, ids.center),
            at(&solved, start) - at(&solved, ids.center),
        );
        let got = m + DVec2::from_angle(half).rotate(s0);
        let want =
            plan.arc.center + DVec2::from_angle(half).rotate(plan.arc.start - plan.arc.center);
        assert!(got.distance(want) < 1e-12, "{deg}");

        let mut v = vee(deg.to_radians());
        let (plan, ids) = chamfer(&mut v, ChamferKind::TwoDistances(3.0, 4.0));
        for i in 0..2 {
            assert_eq!(at(&v.s, ids.touch[i]), plan.touch[i]);
        }
    }
}

// ---- Dichte Folgen --------------------------------------------------------------------------

/// Grenze des Radius: Bei ihr erreicht der Berührpunkt das ferne Ende (Länge 40).
fn limit(phi: f64) -> f64 {
    40.0 * (phi / 2.0).tan()
}

#[test]
fn free_lines_solve_over_the_whole_radius_sequence() {
    for sign in [1.0, -1.0] {
        for step in 1..=35 {
            let phi = f64::from(step) * 5.0_f64.to_radians();
            let lim = limit(phi);
            let r0 = 0.2 * lim;
            let mut v = vee(sign * phi);
            let (_, ids) = round(&mut v, r0);
            let radius = ids.radius_dim.unwrap();
            for i in 1..=60 {
                let r = lim * f64::from(i) / 61.0;
                let next = change(&v.s, radius, r0, r)
                    .unwrap_or_else(|| panic!("{step} {sign} r {r} Grenze {lim}"));
                assert!(!folded_over(&v.s, &next), "{step} {sign} r {r}");
                assert!(
                    tangent_error(&next, &ids, [v.l1, v.l2]) < 1e-6,
                    "{step} r {r}"
                );
                assert!((next.radius(ids.arc).unwrap() - r).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn fixed_far_ends_solve_up_to_85_percent_and_never_fold() {
    for sign in [1.0, -1.0] {
        for step in 1..=35 {
            let phi = f64::from(step) * 5.0_f64.to_radians();
            let lim = limit(phi);
            let r0 = 0.2 * lim;
            let mut v = vee(sign * phi);
            let (a, b) = (at(&v.s, v.a), at(&v.s, v.b));
            v.s.add_constraint(ConstraintKind::Fixed(v.a, a.x, a.y))
                .unwrap();
            v.s.add_constraint(ConstraintKind::Fixed(v.b, b.x, b.y))
                .unwrap();
            let (_, ids) = round(&mut v, r0);
            assert_still(&v.s);
            let radius = ids.radius_dim.unwrap();
            for i in 1..=60 {
                let r = lim * f64::from(i) / 61.0;
                match change(&v.s, radius, r0, r) {
                    Some(next) => {
                        assert!(!folded_over(&v.s, &next), "{step} {sign} r {r}");
                        assert!(
                            tangent_error(&next, &ids, [v.l1, v.l2]) < 1e-6,
                            "{step} r {r}"
                        );
                        assert!(at(&next, v.a).distance(a) < 1e-6, "{step} r {r}");
                        assert!(at(&next, v.b).distance(b) < 1e-6, "{step} r {r}");
                    }
                    None => {
                        assert!(r > 0.85 * lim, "{step} {sign} r {r} Grenze {lim}");
                        // Scheitert das Lösen, bleibt der Stand.
                        let mut trial = v.s.clone();
                        trial.set_constraint_value(radius, r).unwrap();
                        if trial.solve().status != SolveStatus::Solved {
                            assert_eq!(trial.variables(), v.s.variables(), "{step} r {r}");
                        }
                    }
                }
            }
        }
    }
}

const RECT_MIDS: [DVec2; 4] = [
    DVec2::new(25.0, 0.0),
    DVec2::new(50.0, 15.0),
    DVec2::new(25.0, 30.0),
    DVec2::new(0.0, 15.0),
];

/// Rechteck 50×30 (H, V, zwei Maße) mit vier Rundungen, die drei letzten `Equal` zur ersten.
fn rounded_rect(r: f64) -> (Sketch, usize, [FilletIds; 4], [CurveId; 4]) {
    let mut s = sketch();
    let rect = s
        .add_rectangle(DVec2::ZERO, DVec2::new(50.0, 30.0))
        .unwrap();
    let (p, l) = (rect.points, rect.lines);
    s.add_constraint(ConstraintKind::Distance(p[0], p[1], 50.0))
        .unwrap();
    s.add_constraint(ConstraintKind::Distance(p[1], p[2], 30.0))
        .unwrap();
    let before = dof(&s);
    let mut out: Vec<FilletIds> = Vec::new();
    for i in 0..4 {
        let j = (i + 1) % 4;
        let plan = s
            .fillet_plan(
                CurvePick {
                    curve: l[i],
                    at: RECT_MIDS[i],
                },
                CurvePick {
                    curve: l[j],
                    at: RECT_MIDS[j],
                },
                r,
            )
            .expect("Plan");
        let first = out.first().map(|f| f.arc);
        let ids = s.add_fillet(&plan, first).unwrap();
        assert_eq!(ids.radius_dim.is_some(), first.is_none());
        out.push(ids);
    }
    (s, before, out.try_into().unwrap(), l)
}

#[test]
fn a_rounded_rectangle_keeps_its_freedom_and_rejects_a_radius_of_half_the_height() {
    let (s, before, ids, lines) = rounded_rect(5.0);
    assert_eq!(assert_still(&s).dof, before);
    let radius = ids[0].radius_dim.unwrap();
    let mut state = s.clone();
    let mut old = 5.0;
    for i in 1..=149 {
        let r = 0.1 * f64::from(i);
        let next = change(&state, radius, old, r).unwrap_or_else(|| panic!("r {r}"));
        assert!(!folded_over(&state, &next), "r {r}");
        for f in &ids {
            assert!((next.radius(f.arc).unwrap() - r).abs() < 1e-6, "r {r}");
        }
        for (k, &l) in lines.iter().enumerate() {
            let (a, b) = line_ends(&next, l);
            let d = at(&next, b) - at(&next, a);
            let off = if k % 2 == 0 { d.y } else { d.x };
            assert!(off.abs() < 1e-6, "H/V r {r}");
        }
        state = next;
        old = r;
    }
    for i in 0..=50 {
        let r = 15.0 + 0.1 * f64::from(i);
        let mut trial = state.clone();
        trial.set_constraint_value(radius, r).unwrap();
        let rep = trial.solve();
        assert_ne!(rep.status, SolveStatus::Solved, "r {r}");
        assert_eq!(trial.variables(), state.variables(), "Stand bleibt, r {r}");
    }
}

#[test]
fn chamfer_sequences_never_fold() {
    for kind in [
        ChamferKind::Equal(5.0),
        ChamferKind::TwoDistances(5.0, 5.0),
        ChamferKind::DistanceAngle(5.0, 0.6),
    ] {
        let mut v = vee(0.5 * PI);
        let (_, ids) = chamfer(&mut v, kind);
        for (k, &dim) in ids.dims.iter().enumerate() {
            if v.s.constraint(dim).unwrap().kind.length_value().is_none() {
                continue;
            }
            for i in 1..=370 {
                let d = 0.1 * f64::from(i);
                let mut next = v.s.clone();
                next.set_constraint_value(dim, d).unwrap();
                let unsolved = next.clone();
                let r = next.solve();
                assert_clean(&r);
                assert!(!folded_over(&unsolved, &next), "{kind:?} d {d}");
                let got = at(&next, ids.corner).distance(at(&next, ids.touch[k]));
                assert!((got - d).abs() < 1e-6, "{kind:?} d {d} {got}");
            }
        }
    }
}

#[test]
fn the_chamfer_angle_follows_over_its_sequence() {
    let mut v = vee(0.5 * PI);
    let (_, ids) = chamfer(&mut v, ChamferKind::DistanceAngle(5.0, 0.6));
    let angle = ids.dims[1];
    let ConstraintKind::Angle(_, _, v0) = v.s.constraint(angle).unwrap().kind else {
        panic!()
    };
    for i in -90..=90 {
        let target = v0 + 0.01 * f64::from(i);
        let mut next = v.s.clone();
        next.set_constraint_value(angle, target).unwrap();
        let unsolved = next.clone();
        assert_clean(&next.solve());
        assert!(!folded_over(&unsolved, &next), "{target}");
        let got = next.measure(&next.constraint(angle).unwrap().kind).unwrap();
        assert!((got - target).abs() < 1e-6, "{got} {target}");
    }
}

#[test]
fn the_angle_of_the_chamfer_is_measured_from_the_layout() {
    let mut v = vee(1.2);
    let (plan, ids) = chamfer(&mut v, ChamferKind::DistanceAngle(5.0, 0.7));
    let kind = v.s.constraint(ids.dims[1]).unwrap().kind.clone();
    let ConstraintKind::Angle(l, c, value) = kind.clone() else {
        panic!()
    };
    assert_eq!((l, c), (v.l1, ids.line));
    assert_eq!(value, v.s.measure(&kind).unwrap());
    assert_eq!(plan.kind, ChamferKind::DistanceAngle(5.0, 0.7));
}

// ---- Abhängigkeiten wie beim Stutzen --------------------------------------------------------

#[test]
fn what_still_holds_stays_and_the_rest_goes() {
    let mut v = vee(0.5 * PI);
    let h =
        v.s.add_constraint(ConstraintKind::Horizontal(LineOrPoints::Line(v.l1)))
            .unwrap();
    let q = pt(&mut v.s, 20.0, 0.0);
    let mid =
        v.s.add_constraint(ConstraintKind::Midpoint(q, v.l1))
            .unwrap();
    let n0 = pt(&mut v.s, 0.0, -50.0);
    let n1 = pt(&mut v.s, 40.0, -50.0);
    let l9 = v.s.add_line(n0, n1).unwrap();
    let eq = v.s.add_constraint(ConstraintKind::Equal(v.l1, l9)).unwrap();
    let fix =
        v.s.add_constraint(ConstraintKind::Fixed(v.a, 40.0, 0.0))
            .unwrap();
    round(&mut v, 5.0);
    assert!(v.s.constraint(h).is_some(), "H bleibt");
    assert!(v.s.constraint(mid).is_none(), "Mittelpunkt entfällt");
    assert!(v.s.constraint(eq).is_none(), "Länge stimmt nicht mehr");
    assert!(v.s.constraint(fix).is_some(), "fest am fernen Ende bleibt");
    assert_still(&v.s);
}

#[test]
fn equal_stays_when_the_length_matches_afterwards() {
    let mut v = vee(0.5 * PI);
    let n0 = pt(&mut v.s, 0.0, -50.0);
    let n1 = pt(&mut v.s, 35.0, -50.0);
    let l9 = v.s.add_line(n0, n1).unwrap();
    let eq = v.s.add_constraint(ConstraintKind::Equal(v.l2, l9)).unwrap();
    round(&mut v, 5.0);
    assert!(v.s.constraint(eq).is_some());
    assert_still(&v.s);
}

#[test]
fn a_third_curve_at_the_corner_keeps_the_point_and_stays_attached() {
    let mut v = vee(0.5 * PI);
    let c = pt(&mut v.s, -30.0, -30.0);
    let third = v.s.add_line(v.p, c).unwrap();
    let before = dof(&v.s);
    let (_, ids) = round(&mut v, 5.0);
    assert_eq!(ids.corner, Some(v.p));
    assert_eq!(assert_still(&v.s).dof, before);
    assert_eq!(line_ends(&v.s, third), (v.p, c));
    assert_eq!(at(&v.s, v.p), DVec2::ZERO);
}

// ---- Feste Geometrie (Review-Focus 4) -------------------------------------------------------

#[test]
fn fixed_far_ends_do_not_move() {
    let mut v = vee(1.0);
    let (a, b) = (at(&v.s, v.a), at(&v.s, v.b));
    v.s.add_constraint(ConstraintKind::Fixed(v.a, a.x, a.y))
        .unwrap();
    v.s.add_constraint(ConstraintKind::Fixed(v.b, b.x, b.y))
        .unwrap();
    round(&mut v, 5.0);
    assert_still(&v.s);
    assert_eq!(at(&v.s, v.a), a);
    assert_eq!(at(&v.s, v.b), b);
}

#[test]
fn a_corner_bound_to_a_fixed_origin_stays_bound() {
    let mut v = vee(1.0);
    let origin = pt(&mut v.s, 0.0, 0.0);
    v.s.add_constraint(ConstraintKind::Fixed(origin, 0.0, 0.0))
        .unwrap();
    let bind =
        v.s.add_constraint(ConstraintKind::Coincident(v.p, origin))
            .unwrap();
    let before = dof(&v.s);
    let (_, ids) = round(&mut v, 5.0);
    assert_eq!(ids.corner, Some(v.p));
    assert!(v.s.constraint(bind).is_some());
    assert_eq!(assert_still(&v.s).dof, before);
    assert_eq!(at(&v.s, v.p), DVec2::ZERO);
}

#[test]
fn a_fixed_corner_leaves_both_lines_whole() {
    let mut v = vee(1.0);
    let fix =
        v.s.add_constraint(ConstraintKind::Fixed(v.p, 0.0, 0.0))
            .unwrap();
    let kinds = (
        v.s.curve(v.l1).unwrap().kind.clone(),
        v.s.curve(v.l2).unwrap().kind.clone(),
    );
    let before = dof(&v.s);
    let (plan, ids) = round(&mut v, 5.0);
    assert_eq!(plan.moves, [EndMove::Whole, EndMove::Whole]);
    assert_eq!(ids.corner, Some(v.p));
    assert!(v.s.constraint(fix).is_some());
    assert_eq!(v.s.curve(v.l1).unwrap().kind, kinds.0);
    assert_eq!(v.s.curve(v.l2).unwrap().kind, kinds.1);
    assert_eq!(assert_still(&v.s).dof, before);
    let radius = ids.radius_dim.unwrap();
    for i in 1..=100 {
        let r = 0.1 * f64::from(i);
        let next = change(&v.s, radius, 5.0, r).unwrap_or_else(|| panic!("r {r}"));
        assert!(tangent_error(&next, &ids, [v.l1, v.l2]) < 1e-6, "r {r}");
    }
}

#[test]
fn a_stale_plan_changes_nothing() {
    let mut v = vee(1.0);
    let (a, b) = picks(&v);
    let plan = v.s.fillet_plan(a, b, 5.0).unwrap();
    let chamfer = v.s.chamfer_plan(a, b, ChamferKind::Equal(5.0)).unwrap();
    v.s.remove_curve(v.l2).unwrap();
    let data = v.s.to_data();
    assert!(v.s.add_fillet(&plan, None).is_err());
    assert_eq!(v.s.to_data(), data);
    assert!(v.s.add_chamfer(&chamfer).is_err());
    assert_eq!(v.s.to_data(), data);
}

// ---- Eine Fläche ----------------------------------------------------------------------------

#[test]
fn a_rounded_rectangle_is_one_profile() {
    let (s, _, _, _) = rounded_rect(5.0);
    assert_eq!(s.profiles().profiles.len(), 1);
}

#[test]
fn a_chamfered_rectangle_is_one_profile_with_corner_and_helpers() {
    let mut s = sketch();
    let rect = s
        .add_rectangle(DVec2::ZERO, DVec2::new(50.0, 30.0))
        .unwrap();
    let l = rect.lines;
    for i in 0..4 {
        let j = (i + 1) % 4;
        let plan = s
            .chamfer_plan(
                CurvePick {
                    curve: l[i],
                    at: RECT_MIDS[i],
                },
                CurvePick {
                    curve: l[j],
                    at: RECT_MIDS[j],
                },
                ChamferKind::Equal(5.0),
            )
            .expect("Plan");
        s.add_chamfer(&plan).unwrap();
    }
    assert_eq!(s.profiles().profiles.len(), 1);
    assert_still(&s);
}

// ---- Ellipse und Spline ---------------------------------------------------------------------

fn line_ellipse() -> (Sketch, CurveId, CurveId) {
    let mut s = sketch();
    let m = pt(&mut s, 0.0, 0.0);
    let e = s.add_ellipse(m, DVec2::new(40.0, 0.0), 20.0).unwrap();
    let a = pt(&mut s, 60.0, 10.0);
    let b = pt(&mut s, 34.0, 10.0);
    let l = s.add_line(a, b).unwrap();
    (s, l, e)
}

#[test]
fn line_ellipse_is_numeric_without_redundancy() {
    let (mut s, l, e) = line_ellipse();
    let before = dof(&s);
    let plan = s
        .fillet_plan(pick(l, 50.0, 10.0), pick(e, 0.0, 20.0), 4.0)
        .expect("Plan");
    assert!(!plan.exact);
    assert_eq!(plan.moves[1], EndMove::Whole);
    let ids = s.add_fillet(&plan, None).unwrap();
    let mut solved = s.clone();
    let r = solved.solve();
    assert_clean(&r);
    assert_eq!(r.dof, before);
    assert!(has(&s, ConstraintKind::PointOnCurve(ids.touch[1], e)));
    assert_eq!(ids.helper, None);
}

/// Grenze festgehalten: Die Tangente an der Ellipse gilt nur beim Anlegen. Wird die Halbachse
/// größer, bleibt der Berührpunkt auf der Ellipse, die Linie geht aber nicht mit.
#[test]
fn the_line_does_not_follow_a_changed_ellipse() {
    let (mut s, l, e) = line_ellipse();
    let plan = s
        .fillet_plan(pick(l, 50.0, 10.0), pick(e, 0.0, 20.0), 4.0)
        .expect("Plan");
    let ids = s.add_fillet(&plan, None).unwrap();
    let dim = s
        .add_constraint(ConstraintKind::EllipseMajor(e, 40.0))
        .unwrap();
    let mut held = s.clone();
    assert_clean(&held.solve());
    let (a0, _) = line_ends(&held, l);
    let start = at(&held, a0);
    let mut next = held.clone();
    next.set_constraint_value(dim, 41.0).unwrap();
    let r = next.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    let t = at(&next, ids.touch[1]);
    let u = next.ellipse_param_of(e, t).unwrap();
    assert!(next.ellipse_point(e, u).unwrap().distance(t) < 1e-6);
    // Das ferne Linienende bleibt fast, wo es war, die Richtung auch.
    assert!(at(&next, a0).distance(start) < 0.05);
    let (a, b) = line_ends(&next, l);
    let d = (at(&next, b) - at(&next, a)).normalize();
    assert!(d.perp_dot(DVec2::X).abs() < 0.01, "{d:?}");
    // Der Bogen berührt die Ellipse dort nicht mehr tangential: gemessen rund 0,04.
    let m = at(&next, ids.center);
    let eps = 1e-5;
    let tangent = (next.ellipse_point(e, u + eps).unwrap()
        - next.ellipse_point(e, u - eps).unwrap())
    .normalize();
    assert!(tangent.dot((t - m).normalize()).abs() > 0.01);
}

#[test]
fn line_spline_is_numeric_without_redundancy() {
    let mut s = sketch();
    let pts: Vec<PointId> = [(-30.0, 0.0), (-10.0, 3.0), (10.0, -3.0), (30.0, 0.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let sp = s.add_spline(&pts).unwrap();
    let a = pt(&mut s, 0.0, 40.0);
    let b = pt(&mut s, 0.0, 2.0);
    let l = s.add_line(a, b).unwrap();
    let before = dof(&s);
    let plan = s
        .fillet_plan(pick(l, 0.0, 20.0), pick(sp, 20.0, -2.0), 3.0)
        .expect("Plan");
    assert!(!plan.exact);
    assert_eq!(plan.moves[1], EndMove::Whole);
    s.add_fillet(&plan, None).unwrap();
    let r = s.clone().solve();
    assert_clean(&r);
    assert_eq!(r.dof, before);
}

#[test]
fn an_elliptic_arc_is_shortened_and_stays_on_its_ellipse() {
    let mut s = sketch();
    let m = pt(&mut s, 0.0, 0.0);
    let start = pt(&mut s, 40.0, 0.0);
    let end = pt(&mut s, 0.0, 20.0);
    let ea = s
        .add_elliptic_arc(m, DVec2::new(40.0, 0.0), 20.0, start, end)
        .unwrap();
    let a = pt(&mut s, 50.0, 40.0);
    let l = s.add_line(a, end).unwrap();
    let before = dof(&s);
    let u = 0.25_f64 * PI;
    let plan = s
        .fillet_plan(
            pick(l, 25.0, 30.0),
            pick(ea, 40.0 * u.cos(), 20.0 * u.sin()),
            3.0,
        )
        .expect("Plan");
    assert!(!plan.exact);
    let ids = s.add_fillet(&plan, None).unwrap();
    assert!(matches!(
        s.curve(ea).unwrap().kind,
        CurveKind::EllipticArc { start: u0, end: u1, .. } if u0 == start && u1 == ids.touch[1]
    ));
    // Die Tangente an der Ellipse hält keine Zeile: ein Freiheitsgrad mehr als vorher.
    let r = s.clone().solve();
    assert_clean(&r);
    assert_eq!(r.dof, before + 1);
}

// ---- Rundlauf -------------------------------------------------------------------------------

#[test]
fn the_data_round_trip_is_equal() {
    let mut v = vee(1.1);
    v.s.add_constraint(ConstraintKind::Distance(v.a, v.p, 40.0))
        .unwrap();
    round(&mut v, 5.0);
    let data = v.s.to_data();
    let back = Sketch::from_data(data.clone()).unwrap();
    assert_eq!(back.to_data(), data);
    assert_still(&back);
}
