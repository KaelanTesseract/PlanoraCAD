//! Spline mit Kontrollpunkten in allen abhängigen Befehlen (Skizzen 7, Task 3): Stutzen, Brechen,
//! Dehnen, Versatz, Abrunden, Spiegeln, Muster, Kopieren, Fang und Profile.

use planora_kernel::{ProfileLoop, Segment};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::fillet::{CurvePick, EndMove};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::pattern::{CircularMode, PatternKind, PatternPlan};
use planora_sketch::snap::{Snap, SnapIndex};
use planora_sketch::solver::SolveStatus;
use planora_sketch::transform::Similarity;
use std::sync::Arc;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn points(s: &mut Sketch, xy: &[(f64, f64)]) -> Vec<PointId> {
    xy.iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect()
}

fn line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let p = s.add_point(a.0, a.1).unwrap();
    let q = s.add_point(b.0, b.1).unwrap();
    s.add_line(p, q).unwrap()
}

/// Die Kurve hat bei jedem Parameter ihres Bereichs dieselbe Lage wie `old`.
fn assert_same_shape(s: &Sketch, c: CurveId, old: &NurbsCurve, tol: f64) {
    let (lo, hi) = s.curve_domain(c).unwrap();
    assert!(hi > lo);
    for i in 0..=4000 {
        let t = lo + (hi - lo) * i as f64 / 4000.0;
        let got = s.curve_eval(c, t).unwrap();
        let want = old.point_unclamped(t).truncate();
        assert!(got.distance(want) < tol, "t = {t}: {got} statt {want}");
    }
}

fn knots_of(s: &Sketch, c: CurveId) -> Vec<f64> {
    match &s.curve(c).unwrap().kind {
        CurveKind::ControlSpline { knots, .. } => knots.clone(),
        k => panic!("falsche Art: {k:?}"),
    }
}

fn control_points(s: &Sketch, c: CurveId) -> Vec<PointId> {
    match &s.curve(c).unwrap().kind {
        CurveKind::ControlSpline { points, .. } => points.clone(),
        k => panic!("falsche Art: {k:?}"),
    }
}

/// Kleinster Abstand von `p` zur Kurve: dichte Abtastung, um die beste Stelle nachgeschärft.
fn dense_distance(s: &Sketch, c: CurveId, p: DVec2) -> f64 {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let at = |t: f64| s.curve_eval(c, t).unwrap().distance(p);
    let step = (hi - lo) / 20_000.0;
    let best = (0..=20_000)
        .map(|i| (at(lo + step * i as f64), i))
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .unwrap()
        .1;
    let mid = lo + step * best as f64;
    let (mut a, mut b) = ((mid - step).max(lo), (mid + step).min(hi));
    for _ in 0..60 {
        let (x, y) = (b - 0.618 * (b - a), a + 0.618 * (b - a));
        if at(x) <= at(y) {
            b = y;
        } else {
            a = x;
        }
    }
    at(0.5 * (a + b))
}

/// Kleinster Abstand von `p` zur dicht abgetasteten Kurve.
fn distance_to(s: &Sketch, c: CurveId, p: DVec2) -> f64 {
    let (lo, hi) = s.curve_domain(c).unwrap();
    (0..=20_000)
        .map(|i| {
            let t = lo + (hi - lo) * i as f64 / 20_000.0;
            s.curve_eval(c, t).unwrap().distance(p)
        })
        .fold(f64::INFINITY, f64::min)
}

const ARCH: [(f64, f64); 6] = [
    (0.0, 0.0),
    (2.0, 5.0),
    (5.0, 7.0),
    (9.0, 6.0),
    (12.0, 3.0),
    (14.0, 0.0),
];

const WAVE: [(f64, f64); 9] = [
    (0.0, 0.0),
    (2.0, 4.0),
    (5.0, 5.0),
    (7.0, 1.0),
    (10.0, -2.0),
    (13.0, 1.0),
    (15.0, 5.0),
    (18.0, 4.0),
    (20.0, 0.0),
];

// ---- Stutzen und Brechen ------------------------------------------------------------------

#[test]
fn trim_keeps_the_shape_of_the_rest() {
    for degree in [3, 5] {
        let mut s = sketch();
        let pts = points(&mut s, &WAVE);
        let c = s.add_control_spline(&pts, degree).unwrap();
        let old = s.nurbs_of(c).unwrap();
        line(&mut s, (9.0, -20.0), (9.0, 20.0));
        let (lo, hi) = s.curve_domain(c).unwrap();
        // Rechts der Linie geklickt: Dieses Stück fällt weg.
        let click = s.curve_eval(c, lo + 0.9 * (hi - lo)).unwrap();
        let out = s.trim(c, click).unwrap();
        assert_eq!(out, vec![c], "Grad {degree}");
        assert_same_shape(&s, c, &old, 1e-9);
        let (nlo, nhi) = s.curve_domain(c).unwrap();
        assert_eq!(nlo, lo);
        assert!(nhi < hi);
        // Das neue Ende liegt auf der Linie und hängt daran; der Anfang behält seine Id.
        let ctrl = control_points(&s, c);
        assert_eq!(ctrl[0], pts[0]);
        let end = s.point_xy(*ctrl.last().unwrap()).unwrap();
        assert!((end.x - 9.0).abs() < 1e-9, "{end}");
        assert!(
            s.constraints().any(
                |(_, d)| matches!(d.kind, K::PointOnCurve(p, _) if p == *ctrl.last().unwrap())
            )
        );
        // Weggefallene Kontrollpunkte sind fort, die Skizze lädt und rechnet ohne Sprung.
        assert!(s.point_xy(pts[8]).is_err());
        Sketch::from_data(s.to_data()).unwrap();
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
        assert_same_shape(&s, c, &old, 1e-7);
    }
}

#[test]
fn trim_of_the_start_keeps_the_end_and_the_parameter() {
    let mut s = sketch();
    let pts = points(&mut s, &WAVE);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let old = s.nurbs_of(c).unwrap();
    line(&mut s, (9.0, -20.0), (9.0, 20.0));
    let click = s.curve_eval(c, 0.2).unwrap();
    assert_eq!(s.trim(c, click).unwrap(), vec![c]);
    assert_same_shape(&s, c, &old, 1e-9);
    let ctrl = control_points(&s, c);
    assert_eq!(*ctrl.last().unwrap(), pts[8]);
    assert!(s.curve_domain(c).unwrap().0 > 0.0);
    assert_eq!(s.curve_domain(c).unwrap().1, old.domain().1);
}

#[test]
fn trim_in_the_middle_gives_two_exact_pieces_and_one_undo_state() {
    let mut s = sketch();
    let pts = points(&mut s, &WAVE);
    let c = s.add_control_spline(&pts, 5).unwrap();
    let old = s.nurbs_of(c).unwrap();
    line(&mut s, (7.0, -20.0), (7.0, 20.0));
    line(&mut s, (13.0, -20.0), (13.0, 20.0));
    let (lo, hi) = s.curve_domain(c).unwrap();
    let mid = (0..=1000)
        .map(|i| lo + (hi - lo) * i as f64 / 1000.0)
        .map(|t| s.curve_eval(c, t).unwrap())
        .find(|p| (p.x - 10.0).abs() < 0.1)
        .unwrap();
    let out = s.trim(c, mid).unwrap();
    assert_eq!(out.len(), 2);
    for &piece in &out {
        assert_same_shape(&s, piece, &old, 1e-9);
    }
}

#[test]
fn break_gives_two_exact_pieces() {
    for degree in [3, 5] {
        let mut s = sketch();
        let pts = points(&mut s, &WAVE);
        let c = s.add_control_spline(&pts, degree).unwrap();
        let old = s.nurbs_of(c).unwrap();
        let (lo, hi) = s.curve_domain(c).unwrap();
        line(&mut s, (9.0, -20.0), (9.0, 20.0));
        let click = s.curve_eval(c, lo + 0.9 * (hi - lo)).unwrap();
        let out = s.break_curve(c, click).unwrap();
        assert_eq!(out.len(), 2, "Grad {degree}");
        assert_eq!(out[0], c);
        for &piece in &out {
            assert_same_shape(&s, piece, &old, 1e-9);
        }
        // Die Teile schließen im Parameter und im Raum aneinander an.
        let (a, b) = (
            s.curve_domain(out[0]).unwrap(),
            s.curve_domain(out[1]).unwrap(),
        );
        assert_eq!((a.0, a.1), (lo, b.0));
        assert_eq!(b.1, hi);
        let (end, start) = (
            *control_points(&s, out[0]).last().unwrap(),
            control_points(&s, out[1])[0],
        );
        assert_ne!(end, start);
        assert!(s.constraints().any(|(_, d)| {
            matches!(d.kind, K::Coincident(p, q) if (p, q) == (end, start) || (p, q) == (start, end))
        }));
        Sketch::from_data(s.to_data()).unwrap();
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
        for &piece in &out {
            assert_same_shape(&s, piece, &old, 1e-7);
        }
    }
}

#[test]
fn trim_close_to_a_knot_is_stable() {
    // Review Focus 5: Knoten bei 1 und 2 (ARCH, Grad 3). Eine Schnittstelle in jedem Abstand
    // vom Knoten ergibt eine gültige Kurve oder eine Meldung, nie zu nahe Knoten oder eine Panik.
    for knot in [1.0, 2.0] {
        for side in [-1.0, 1.0] {
            for gap in [0.0, 1e-14, 1e-12, 1e-10, 1e-9, 1e-8, 1e-7, 1e-6, 1e-5, 1e-3] {
                let t = knot + side * gap;
                let mut s = sketch();
                let pts = points(&mut s, &ARCH);
                let c = s.add_control_spline(&pts, 3).unwrap();
                let old = s.nurbs_of(c).unwrap();
                let (at, d) = (s.curve_eval(c, t).unwrap(), old.derivs(t, 1)[1].truncate());
                let n = d.normalize().perp() * 30.0;
                line(&mut s, (at.x - n.x, at.y - n.y), (at.x + n.x, at.y + n.y));
                let click = s.curve_eval(c, 2.9).unwrap();
                s.trim(c, click).unwrap();
                Sketch::from_data(s.to_data()).unwrap();
                // Nach dem Legen auf den Knoten weicht die Form nur um den Schnappweg ab.
                assert_same_shape(&s, c, &old, 1e-5);
                let knots = knots_of(&s, c);
                let range = knots[knots.len() - 1] - knots[0];
                for w in knots.windows(2) {
                    assert!(
                        w[1] == w[0] || w[1] - w[0] >= 1e-9 * range,
                        "{knots:?} (t = {t})"
                    );
                }
                let end = s.point_xy(*control_points(&s, c).last().unwrap()).unwrap();
                assert!(end.distance(at) < 1e-5, "{end} statt {at}");
                let rep = s.solve();
                assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
            }
        }
    }
}

#[test]
fn trim_close_to_the_ends_changes_nothing_or_stays_valid() {
    for t in [1e-12, 1e-9, 1e-7, 1e-5, 3.0 - 1e-12, 3.0 - 1e-9, 3.0 - 1e-7] {
        let mut s = sketch();
        let pts = points(&mut s, &ARCH);
        let c = s.add_control_spline(&pts, 3).unwrap();
        let old = s.nurbs_of(c).unwrap();
        let (at, d) = (s.curve_eval(c, t).unwrap(), old.derivs(t, 1)[1].truncate());
        let n = d.normalize().perp() * 30.0;
        line(&mut s, (at.x - n.x, at.y - n.y), (at.x + n.x, at.y + n.y));
        let click = s.curve_eval(c, 1.5).unwrap();
        let before = s.to_data();
        match s.trim(c, click) {
            Ok(_) => {
                Sketch::from_data(s.to_data()).unwrap();
            }
            // Eine Ablehnung nennt ihren Grund und ändert nichts.
            Err(e) => {
                assert!(!e.to_string().is_empty());
                assert_eq!(s.to_data(), before);
            }
        }
    }
}

#[test]
fn point_on_the_kept_part_stays_and_on_the_removed_part_is_dropped() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    // Beide Punkte im letzten Polynomstück, die Schnittstelle dazwischen: Ein alter Parameter
    // ließe den weggefallenen Punkt über die Fortsetzung der Kurve weiter „erfüllt“ aussehen.
    let keep_at = s.curve_eval(c, 2.2).unwrap();
    let drop_at = s.curve_eval(c, 2.7).unwrap();
    let keep = s.add_point(keep_at.x, keep_at.y).unwrap();
    let gone = s.add_point(drop_at.x, drop_at.y).unwrap();
    s.add_constraint(K::PointOnCurve(keep, c)).unwrap();
    s.add_constraint(K::PointOnCurve(gone, c)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let keep_at = s.point_xy(keep).unwrap();
    let cut = s.curve_eval(c, 2.45).unwrap();
    line(
        &mut s,
        (cut.x + 20.0, cut.y + 20.0),
        (cut.x - 20.0, cut.y - 20.0),
    );
    let click = s.curve_eval(c, 2.9).unwrap();
    s.trim(c, click).unwrap();
    let on = |s: &Sketch, p: PointId| {
        s.constraints()
            .any(|(_, d)| matches!(d.kind, K::PointOnCurve(q, k) if q == p && k == c))
    };
    assert!(on(&s, keep));
    assert!(!on(&s, gone), "der Punkt liegt nicht mehr auf der Kurve");
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(s.point_xy(keep).unwrap().distance(keep_at) < 1e-6);
}

#[test]
fn trim_without_a_cut_removes_the_curve_and_its_points() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let at = s.curve_eval(c, 1.0).unwrap();
    assert_eq!(s.trim(c, at).unwrap(), Vec::<CurveId>::new());
    assert!(s.curve(c).is_none());
    assert_eq!(s.points().count(), 0);
}

// ---- Dehnen -------------------------------------------------------------------------------

#[test]
fn extend_hits_the_target_exactly() {
    for (xy, degree) in [
        (&ARCH[..], 3),
        (&ARCH[..], 5),
        (&ARCH[..4], 5), // Bézier Grad 3
        (&ARCH[..3], 3), // wirksam Grad 2
        (&ARCH[..2], 3), // wirksam Grad 1
    ] {
        let mut s = sketch();
        let pts = points(&mut s, xy);
        let c = s.add_control_spline(&pts, degree).unwrap();
        let old = s.nurbs_of(c).unwrap();
        let (_, hi) = old.domain();
        let last = *pts.last().unwrap();
        // Das Ziel: quer zur Kurve hinter dem Ende, bei einem Viertel des Parameterbereichs.
        let tip = old
            .point_unclamped(hi + 0.25 * (hi - old.domain().0))
            .truncate();
        let n = old.derivs(hi, 1)[1].truncate().normalize().perp() * 30.0;
        let target = line(
            &mut s,
            (tip.x - n.x, tip.y - n.y),
            (tip.x + n.x, tip.y + n.y),
        );
        let from = s.point_xy(last).unwrap();
        assert!(s.extend_preview(c, from).is_some());
        assert!(s.extend(c, from).unwrap(), "{} Punkte", xy.len());
        // Der Endpunkt behält seine Id, trifft die Linie genau und ist das Ende der Kurve.
        let ctrl = control_points(&s, c);
        assert_eq!(ctrl.len(), pts.len());
        assert_eq!(*ctrl.last().unwrap(), last);
        let (lo2, hi2) = s.curve_domain(c).unwrap();
        assert!(hi2 > hi);
        assert_eq!(lo2, old.domain().0);
        let end = s.point_xy(last).unwrap();
        assert!(end.distance(tip) < 1e-9, "{end} statt {tip}");
        assert!(s.curve_eval(c, hi2).unwrap().distance(end) < 1e-12);
        assert!(
            s.constraints()
                .any(|(_, d)| matches!(d.kind, K::PointOnCurve(p, k) if p == last && k == target))
        );
        Sketch::from_data(s.to_data()).unwrap();
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
        assert!(s.point_xy(last).unwrap().distance(tip) < 1e-6);
    }
}

#[test]
fn extend_at_the_start_works_the_same_way() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let old = s.nurbs_of(c).unwrap();
    let tip = old.point_unclamped(-0.5).truncate();
    let n = old.derivs(0.0, 1)[1].truncate().normalize().perp() * 30.0;
    line(
        &mut s,
        (tip.x - n.x, tip.y - n.y),
        (tip.x + n.x, tip.y + n.y),
    );
    assert!(s.extend(c, v(0.0, 0.0)).unwrap());
    let (lo, hi) = s.curve_domain(c).unwrap();
    assert!((lo + 0.5).abs() < 1e-9, "{lo}");
    assert_eq!(hi, 3.0);
    assert!(s.point_xy(pts[0]).unwrap().distance(tip) < 1e-9);
    assert_same_shape(&s, c, &old, 1e-8);
}

#[test]
fn extend_keeps_the_old_part() {
    for degree in [3, 5] {
        let mut s = sketch();
        let pts = points(&mut s, &WAVE);
        let c = s.add_control_spline(&pts, degree).unwrap();
        let old = s.nurbs_of(c).unwrap();
        let (lo, hi) = old.domain();
        // Ein Punkt auf dem alten Stück und ein fester innerer Kontrollpunkt vorn bleiben.
        let on = s.curve_eval(c, lo + 0.4 * (hi - lo)).unwrap();
        let q = s.add_point(on.x, on.y).unwrap();
        s.add_constraint(K::PointOnCurve(q, c)).unwrap();
        s.add_constraint(K::Fixed(pts[1], WAVE[1].0, WAVE[1].1))
            .unwrap();
        // Die letzten Kontrollpunkte wandern: Ein fester davon entfällt nach Regel 7.
        s.add_constraint(K::Fixed(pts[7], WAVE[7].0, WAVE[7].1))
            .unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);
        let tip = old.point_unclamped(hi + 0.1).truncate();
        let n = old.derivs(hi, 1)[1].truncate().normalize().perp() * 30.0;
        line(
            &mut s,
            (tip.x - n.x, tip.y - n.y),
            (tip.x + n.x, tip.y + n.y),
        );
        assert!(s.extend(c, v(20.0, 0.0)).unwrap());
        // Das alte Stück und seine Fortsetzung sind dieselbe Kurve.
        assert_same_shape(&s, c, &old, 1e-8);
        let held = |s: &Sketch, p: PointId| {
            s.constraints()
                .any(|(_, d)| matches!(d.kind, K::Fixed(q, ..) if q == p))
        };
        assert!(held(&s, pts[1]));
        assert!(!held(&s, pts[7]), "Grad {degree}");
        assert!(
            s.constraints()
                .any(|(_, d)| matches!(d.kind, K::PointOnCurve(p, k) if p == q && k == c))
        );
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
        assert_same_shape(&s, c, &old, 1e-6);
    }
}

#[test]
fn extend_without_a_target_or_on_a_bound_end_changes_nothing() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let before = s.to_data();
    assert!(!s.extend(c, v(14.0, 0.0)).unwrap());
    assert_eq!(s.to_data(), before);
    // Weiter weg als die dreifache Bereichsbreite: Die Suche endet vorher.
    let old = s.nurbs_of(c).unwrap();
    let tip = old.point_unclamped(3.0 + 3.5 * 3.0).truncate();
    let n = old.derivs(3.0, 1)[1].truncate().normalize().perp() * 1e5;
    line(
        &mut s,
        (tip.x - n.x, tip.y - n.y),
        (tip.x + n.x, tip.y + n.y),
    );
    assert!(s.extend_preview(c, v(14.0, 0.0)).is_none());

    // Ein Ende, an dem eine andere Kurve hängt, ist nicht offen.
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let far = s.add_point(20.0, 20.0).unwrap();
    s.add_line(pts[5], far).unwrap();
    line(&mut s, (15.0, -30.0), (15.0, 0.5));
    assert!(s.extend_preview(c, v(14.0, 0.0)).is_none());

    // Ein mitwandernder Kontrollpunkt, an dem eine andere Kurve hängt: Sie bewegte sich mit.
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let far = s.add_point(20.0, 20.0).unwrap();
    s.add_line(pts[4], far).unwrap();
    let old = s.nurbs_of(c).unwrap();
    let tip = old.point_unclamped(3.5).truncate();
    let n = old.derivs(3.0, 1)[1].truncate().normalize().perp() * 30.0;
    line(
        &mut s,
        (tip.x - n.x, tip.y - n.y),
        (tip.x + n.x, tip.y + n.y),
    );
    let before = s.to_data();
    assert!(!s.extend(c, v(14.0, 0.0)).unwrap());
    assert_eq!(s.to_data(), before);
}

// ---- Versatz und Abrunden -----------------------------------------------------------------

#[test]
fn offset_control_spline_is_a_fit_spline_at_distance() {
    for (xy, degree) in [(&WAVE[..], 3), (&WAVE[..], 5), (&ARCH[..5], 5)] {
        let mut s = sketch();
        let pts = points(&mut s, xy);
        let c = s.add_control_spline(&pts, degree).unwrap();
        let chains = s.ordered_chains(&[c]).unwrap();
        let d = 0.4;
        let plan = s.offset_plan(&chains, d).unwrap();
        let ids = s.add_offset(&plan).unwrap();
        assert_eq!(ids.curves.len(), 1);
        let copy = ids.curves[0];
        assert!(matches!(
            s.curve(copy).unwrap().kind,
            CurveKind::Spline { .. }
        ));
        // Alle Stützpunkte liegen genau im Abstand, die Kurve dazwischen nahe daran.
        let CurveKind::Spline { points: fit, .. } = s.curve(copy).unwrap().kind.clone() else {
            unreachable!()
        };
        for p in &fit {
            let q = s.point_xy(*p).unwrap();
            let on = s.nearest_on_curve(c, q).unwrap();
            assert!((on.distance(q) - d).abs() < 1e-9, "{q}");
        }
        let (lo, hi) = s.curve_domain(copy).unwrap();
        for k in 0..=200 {
            let q = s
                .curve_eval(copy, lo + (hi - lo) * k as f64 / 200.0)
                .unwrap();
            let gap = (s.nearest_on_curve(c, q).unwrap().distance(q) - d).abs();
            assert!(gap < 0.1, "{}: {gap} an {q}", xy.len());
        }
    }
}

#[test]
fn offset_of_a_tight_control_spline_reports_the_cusp() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let chains = s.ordered_chains(&[c]).unwrap();
    // Weit nach innen klappt die Kurve um.
    let a = s.offset_plan(&chains, 50.0);
    let b = s.offset_plan(&chains, -50.0);
    assert!(a.is_err() || b.is_err());
}

fn pick(c: CurveId, x: f64, y: f64) -> CurvePick {
    CurvePick {
        curve: c,
        at: v(x, y),
    }
}

/// Größte Abweichung des Versatz-Splines vom Abstand, bezogen auf den Abstand.
fn offset_error(xy: &[(f64, f64)], degree: u8, d: f64) -> f64 {
    let mut s = sketch();
    let pts = points(&mut s, xy);
    let c = s.add_control_spline(&pts, degree).unwrap();
    let chains = s.ordered_chains(&[c]).unwrap();
    let plan = s.offset_plan(&chains, d).unwrap();
    let k = s.add_offset(&plan).unwrap().curves[0];
    let hi = s.curve_domain(k).unwrap().1;
    let mut worst = 0.0_f64;
    for i in 0..=2000 {
        let q = s.curve_eval(k, hi * i as f64 / 2000.0).unwrap();
        worst = worst.max((s.nearest_on_curve(c, q).unwrap().distance(q) - d).abs());
    }
    worst / d
}

#[test]
fn offset_follows_uneven_and_long_control_polygons() {
    // Gleiche Schritte je Bézier-Stück lagen hier bei 0,17 bis 0,20 und 4,07 des Abstands.
    let uneven = [
        (0.0, 0.0),
        (0.2, 3.0),
        (0.4, 3.2),
        (9.0, 3.3),
        (9.2, 0.0),
        (20.0, 0.0),
        (20.0, 8.0),
    ];
    let long: Vec<(f64, f64)> = (0..40)
        .map(|i| (i as f64 * 2.0, 6.0 * (i as f64 * 0.9).sin()))
        .collect();
    for degree in [3, 5] {
        let e = offset_error(&uneven, degree, 0.3);
        assert!(e < 1e-2, "ungleich, Grad {degree}: {e}");
        // Die 64 Stützpunkte begrenzen, was bei 40 Punkten geht.
        let e = offset_error(&long, degree, 0.3);
        assert!(e < 0.1, "lang, Grad {degree}: {e}");
    }
}

#[test]
fn fillet_line_and_control_spline() {
    let mut s = sketch();
    let pts = points(&mut s, &WAVE);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let old = s.nurbs_of(c).unwrap();
    let l = line(&mut s, (9.0, 20.0), (9.0, -20.0));
    let plan = s
        .fillet_plan(pick(l, 9.0, 15.0), pick(c, 2.0, 2.0), 1.0)
        .expect("Plan");
    assert!(!plan.exact);
    let EndMove::Cut { end, .. } = plan.moves[1] else {
        panic!("{:?}", plan.moves);
    };
    assert_eq!(end, pts[8], "das Ende hinter der Ecke fällt weg");
    // Die Mitte liegt im Abstand r von beiden Kurven, der Berührpunkt auf der Kurve.
    assert!((distance_to(&s, c, plan.arc.center) - 1.0).abs() < 1e-3);
    assert!(((plan.arc.center.x.abs() - 9.0).abs() - 1.0).abs() < 1e-6);
    assert!(distance_to(&s, c, plan.touch[1]) < 1e-3);
    let ids = s.add_fillet(&plan, None).unwrap();
    // Gekürzt über denselben Weg wie Stutzen: Form gleich, das Ende ist der Berührpunkt.
    let ctrl = control_points(&s, c);
    assert_eq!(ctrl[0], pts[0]);
    assert_eq!(*ctrl.last().unwrap(), ids.touch[1]);
    assert!(s.point_xy(pts[8]).is_err());
    assert_same_shape(&s, c, &old, 1e-9);
    Sketch::from_data(s.to_data()).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(rep.conflicting.is_empty(), "{rep:?}");
}

#[test]
fn fillet_with_a_fixed_end_leaves_the_control_spline_whole() {
    let mut s = sketch();
    let pts = points(&mut s, &WAVE);
    let c = s.add_control_spline(&pts, 3).unwrap();
    s.add_constraint(K::Fixed(pts[8], 20.0, 0.0)).unwrap();
    let l = line(&mut s, (9.0, 20.0), (9.0, -20.0));
    let plan = s
        .fillet_plan(pick(l, 9.0, 15.0), pick(c, 2.0, 2.0), 1.0)
        .expect("Plan");
    assert_eq!(plan.moves[1], EndMove::Whole);
    let knots = knots_of(&s, c);
    let ids = s.add_fillet(&plan, None).unwrap();
    assert_eq!(knots_of(&s, c), knots);
    assert!(
        s.constraints()
            .any(|(_, d)| matches!(d.kind, K::PointOnCurve(p, k) if p == ids.touch[1] && k == c))
    );
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
}

#[test]
fn chamfer_stays_for_lines_only() {
    // Wie heute beim Spline: Die Fase geht nur zwischen Linien.
    let mut s = sketch();
    let pts = points(&mut s, &WAVE);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let l = line(&mut s, (9.0, 20.0), (9.0, -20.0));
    assert!(
        s.chamfer_plan(
            pick(l, 9.0, 15.0),
            pick(c, 2.0, 2.0),
            planora_sketch::fillet::ChamferKind::Equal(1.0)
        )
        .is_none()
    );
}

// ---- Spiegeln, Muster, Kopieren -----------------------------------------------------------

#[test]
fn mirror_control_spline_is_symmetric() {
    for degree in [3, 5] {
        let mut s = sketch();
        let pts = points(&mut s, &ARCH);
        let c = s.add_control_spline(&pts, degree).unwrap();
        let axis = line(&mut s, (-5.0, -20.0), (-5.0, 20.0));
        for p in s.curve(axis).unwrap().kind.points() {
            let q = s.point_xy(p).unwrap();
            s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
        }
        let out = s.add_mirrored(&[c], axis).unwrap();
        let copy = out[0];
        let (a, b) = (
            s.curve(c).unwrap().kind.clone(),
            s.curve(copy).unwrap().kind.clone(),
        );
        let (
            CurveKind::ControlSpline {
                points: pa,
                degree: da,
                knots: ka,
            },
            CurveKind::ControlSpline {
                points: pb,
                degree: db,
                knots: kb,
            },
        ) = (a, b)
        else {
            panic!("falsche Art");
        };
        assert_eq!(da, db);
        assert_eq!(ka, kb);
        assert_eq!(pa.len(), pb.len());
        let (lo, hi) = s.curve_domain(c).unwrap();
        let mirrored = |p: DVec2| v(-10.0 - p.x, p.y);
        for i in 0..=2000 {
            let t = lo + (hi - lo) * i as f64 / 2000.0;
            let (q, r) = (s.curve_eval(c, t).unwrap(), s.curve_eval(copy, t).unwrap());
            assert!(mirrored(q).distance(r) < 1e-9, "{t}");
        }
        // Jeder Kontrollpunkt hat `Symmetric`; das Bild folgt beim Lösen.
        for (p, q) in pa.iter().zip(&pb) {
            assert!(s.constraints().any(|(_, d)| {
                matches!(d.kind, K::Symmetric(x, y, k) if (x, y, k) == (*p, *q, axis))
            }));
        }
        s.set_point_xy(pts[2], 5.0, 9.0).unwrap();
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
        let moved = s.point_xy(pts[2]).unwrap();
        assert!(s.point_xy(pb[2]).unwrap().distance(mirrored(moved)) < 1e-6);
    }
}

#[test]
fn mirror_reuses_a_control_point_on_the_axis() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let axis = line(&mut s, (0.0, -20.0), (0.0, 20.0));
    let copy = s.add_mirrored(&[c], axis).unwrap()[0];
    assert_eq!(control_points(&s, copy)[0], pts[0]);
    assert_ne!(control_points(&s, copy)[1], pts[1]);
}

#[test]
fn mirror_fit_spline_now_works() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_spline(&pts).unwrap();
    let axis = line(&mut s, (-5.0, -20.0), (-5.0, 20.0));
    let copy = s.add_mirrored(&[c], axis).unwrap()[0];
    assert!(matches!(
        s.curve(copy).unwrap().kind,
        CurveKind::Spline { .. }
    ));
    for i in 0..=500 {
        let t = 5.0 * i as f64 / 500.0;
        let (q, r) = (s.curve_eval(c, t).unwrap(), s.curve_eval(copy, t).unwrap());
        assert!(v(-10.0 - q.x, q.y).distance(r) < 1e-9, "{t}");
    }
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
}

#[test]
fn mirror_ellipse_still_refused() {
    let mut s = sketch();
    let m = s.add_point(10.0, 0.0).unwrap();
    let e = s.add_ellipse(m, v(5.0, 0.0), 2.0).unwrap();
    let axis = line(&mut s, (-5.0, -20.0), (-5.0, 20.0));
    let before = s.to_data();
    assert!(s.add_mirrored(&[e], axis).is_err());
    assert_eq!(s.to_data(), before);
}

#[test]
fn pattern_control_spline_follows_source() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 5).unwrap();
    let m = s.add_point(-20.0, 0.0).unwrap();
    let id = s
        .add_pattern(&PatternPlan {
            kind: PatternKind::Circular {
                center: m,
                mode: CircularMode::Full,
            },
            sources: vec![c],
            counts: [4, 1],
            values: [0.0, 0.0],
            suppressed: Vec::new(),
        })
        .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let data = s.pattern(id).unwrap().clone();
    assert_eq!(data.instances.len(), 3);
    let knots = knots_of(&s, c);
    let copy = data.instances[0].curves[0];
    assert_eq!(knots_of(&s, copy), knots);
    let turn = data.instances[0].at[0] as f64 * std::f64::consts::TAU / 4.0;
    let image = |p: DVec2| {
        let d = p - v(-20.0, 0.0);
        v(-20.0, 0.0)
            + DVec2::new(
                d.x * turn.cos() - d.y * turn.sin(),
                d.x * turn.sin() + d.y * turn.cos(),
            )
    };
    let check = |s: &Sketch| {
        let (lo, hi) = s.curve_domain(c).unwrap();
        for i in 0..=500 {
            let t = lo + (hi - lo) * i as f64 / 500.0;
            let (q, r) = (s.curve_eval(c, t).unwrap(), s.curve_eval(copy, t).unwrap());
            assert!(image(q).distance(r) < 1e-6, "{t}: {r} statt {}", image(q));
        }
    };
    check(&s);
    s.set_point_xy(pts[2], 6.0, 9.0).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(s.point_xy(pts[2]).unwrap().distance(v(6.0, 9.0)) < 1e-6);
    check(&s);
}

#[test]
fn linear_pattern_and_copy_keep_degree_and_knots() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let knots = knots_of(&s, c);
    let copies = s.add_linear_pattern(&[c], v(0.0, 20.0), 2).unwrap();
    assert_eq!(copies.len(), 2);
    assert_eq!(knots_of(&s, copies[1][0]), knots);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let (lo, hi) = s.curve_domain(c).unwrap();
    for i in 0..=100 {
        let t = lo + (hi - lo) * i as f64 / 100.0;
        let (q, r) = (
            s.curve_eval(c, t).unwrap(),
            s.curve_eval(copies[1][0], t).unwrap(),
        );
        assert!((q + v(0.0, 40.0)).distance(r) < 1e-6, "{t}");
    }

    // Kopieren mit Drehung und Maßstab: Punkte abgebildet, gleicher Grad und gleiche Knoten.
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 5).unwrap();
    let knots = knots_of(&s, c);
    let t = Similarity {
        base: DVec2::ZERO,
        offset: v(30.0, 0.0),
        angle: 0.5,
        factor: 2.0,
    };
    let set = s.transform_set(&[c], &[]);
    s.copy_transformed(&set, &t).unwrap();
    let copy = s
        .curves()
        .map(|(id, _)| id)
        .find(|&id| id != c)
        .expect("Kopie");
    assert_eq!(knots_of(&s, copy), knots);
    let (lo, hi) = s.curve_domain(c).unwrap();
    for i in 0..=100 {
        let u = lo + (hi - lo) * i as f64 / 100.0;
        let (q, r) = (s.curve_eval(c, u).unwrap(), s.curve_eval(copy, u).unwrap());
        assert!(t.apply(q).distance(r) < 1e-9, "{u}");
    }
}

#[test]
fn moving_and_scaling_keep_the_knots() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let old = s.nurbs_of(c).unwrap();
    let knots = knots_of(&s, c);
    let t = Similarity {
        base: DVec2::ZERO,
        offset: v(1.0, 2.0),
        angle: 0.0,
        factor: 3.0,
    };
    let set = s.transform_set(&[c], &[]);
    assert!(s.apply_transform(&set, &t).is_ok());
    assert_eq!(knots_of(&s, c), knots);
    let (lo, hi) = old.domain();
    for i in 0..=100 {
        let u = lo + (hi - lo) * i as f64 / 100.0;
        let want = t.apply(old.point(u).truncate());
        assert!(s.curve_eval(c, u).unwrap().distance(want) < 1e-9, "{u}");
    }
}

// ---- Fang und nächster Punkt --------------------------------------------------------------

#[test]
fn snap_finds_control_points_and_curve() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let index = SnapIndex::build(&s);
    // Ein innerer Kontrollpunkt (nicht auf der Kurve) und die Enden sind Punkte.
    for p in [pts[0], pts[2], pts[5]] {
        let q = s.point_xy(p).unwrap();
        let hit = index.find(q + v(0.05, 0.05), 0.5).unwrap();
        assert_eq!(hit.snap, Snap::Point(p));
        assert!(hit.at.distance(q) < 1e-12);
    }
    // Die Kurve selbst fängt auf der Kurve.
    let on = s.curve_eval(c, 1.5).unwrap();
    assert!(s.point_xy(pts[2]).unwrap().distance(on) > 0.5);
    let hit = index.find(on + v(0.02, 0.02), 0.1).unwrap();
    assert_eq!(hit.snap, Snap::Curve(c));
    assert!(distance_to(&s, c, hit.at) < 1e-3);
}

#[test]
fn nearest_on_curve_agrees_with_a_dense_search() {
    // Die schnelle Suche (Skizzen 7: etwa 1000 Auswertungen statt 62000) trifft dieselbe Stelle.
    let mut xy: Vec<(f64, f64)> = (0..64)
        .map(|i| {
            let x = i as f64;
            (x, 6.0 * (x * 0.7).sin() + 2.0 * (x * 1.9).cos())
        })
        .collect();
    xy[20].1 += 5.0;
    for degree in [3, 5] {
        let mut s = sketch();
        let pts = points(&mut s, &xy);
        let c = s.add_control_spline(&pts, degree).unwrap();
        let mut worst = 0.0_f64;
        let mut seed = 12345_u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        for _ in 0..50 {
            let p = v(next() * 70.0 - 3.0, next() * 24.0 - 12.0);
            let got = s.nearest_on_curve(c, p).unwrap();
            let want = dense_distance(&s, c, p);
            assert!(got.is_finite());
            // Auf der Kurve: Die Suche ab dem Fundort bleibt dort.
            assert!(s.nearest_on_curve(c, got).unwrap().distance(got) < 1e-9);
            // Die dichte Suche liegt selbst nur auf ihrem Raster, darum nur nach oben begrenzt.
            worst = worst.max(got.distance(p) - want);
        }
        assert!(worst < 1e-6, "Grad {degree}: {worst}");
    }
}

// ---- Profile ------------------------------------------------------------------------------

/// Bogenförmiges Profil: Spline mit Kontrollpunkten, unten eine Linie.
fn arch_profile(degree: u8) -> (Sketch, CurveId, CurveId, Arc<NurbsCurve>) {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, degree).unwrap();
    let l = s.add_line(pts[5], pts[0]).unwrap();
    let n = s.nurbs_of(c).unwrap();
    (s, c, l, n)
}

/// Prüft, dass genau ein Freiformsegment der Schleife die Kurve `n` (vorwärts oder rückwärts) ist.
fn assert_loop_has_exact_nurbs(lp: &ProfileLoop, n: &NurbsCurve) {
    let segs: Vec<&Arc<NurbsCurve>> = lp
        .segments
        .iter()
        .filter_map(|s| match s {
            Segment::Nurbs { curve } => Some(curve),
            _ => None,
        })
        .collect();
    assert_eq!(segs.len(), 1, "{lp:?}");
    let seg = segs[0];
    assert_eq!(seg.degree(), n.degree());
    let (a, b) = seg.domain();
    let (lo, hi) = n.domain();
    assert!(((b - a) - (hi - lo)).abs() < 1e-12);
    let start = seg.point(a).truncate();
    let forward = start.distance(n.point(lo).truncate()) < 1e-12;
    for i in 0..=1000 {
        let u = i as f64 / 1000.0;
        let got = seg.point(a + (b - a) * u).truncate();
        let t = if forward { u } else { 1.0 - u };
        let want = n.point(lo + (hi - lo) * t).truncate();
        assert!(got.distance(want) < 1e-9, "{u}");
    }
}

#[test]
fn profile_with_control_spline_is_exact_nurbs() {
    for degree in [3, 5] {
        let (s, c, l, n) = arch_profile(degree);
        let found = s.profiles();
        assert!(found.rejected.is_empty(), "{:?}", found.rejected);
        assert_eq!(found.profiles.len(), 1);
        assert_loop_has_exact_nurbs(&found.profiles[0].outer, &n);
        assert_eq!(found.curve_loops[0].len(), 2);
        assert!(found.curve_loops[0].contains(&c) && found.curve_loops[0].contains(&l));
        // Der Verweis zählt `u` über den Bereich der Kurve, nicht als Winkel.
        let piece = found.refs[0]
            .outer
            .iter()
            .find(|p| p.curve == c)
            .expect("Stück");
        assert_eq!((piece.u0, piece.u1), (0.0, 1.0), "{piece:?}");
        // Derselbe Rand nach dem Laden findet denselben Verweis wieder.
        let again = Sketch::from_data(s.to_data()).unwrap().profiles();
        assert_eq!(again.refs, found.refs);

        // Der alte Rechenweg nimmt dieselbe Kurve.
        let legacy = s.legacy_profiles();
        assert!(legacy.rejected.is_empty(), "{:?}", legacy.rejected);
        assert_eq!(legacy.profiles.len(), 1);
        assert_loop_has_exact_nurbs(&legacy.profiles[0].outer, &n);
    }
}

#[test]
fn a_control_spline_closed_on_itself_is_a_profile() {
    let xy = [
        (0.0, 0.0),
        (10.0, 0.0),
        (14.0, 8.0),
        (5.0, 14.0),
        (-4.0, 8.0),
        (0.0, 0.0),
    ];
    let mut s = sketch();
    let pts = points(&mut s, &xy);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let n = s.nurbs_of(c).unwrap();
    let found = s.profiles();
    assert!(found.rejected.is_empty(), "{:?}", found.rejected);
    assert_eq!(found.profiles.len(), 1);
    assert_loop_has_exact_nurbs(&found.profiles[0].outer, &n);
    let legacy = s.legacy_profiles();
    assert!(legacy.rejected.is_empty(), "{:?}", legacy.rejected);
    assert_eq!(legacy.profiles.len(), 1);
}

#[test]
fn a_line_across_the_curve_splits_the_profile_exactly() {
    let (mut s, c, _, n) = arch_profile(3);
    let whole = s.profiles().profiles[0].outer.signed_area().abs();
    // Genau durch den Knoten bei t = 1 und einmal knapp daneben.
    for t in [1.0, 1.0 + 1e-10, 1.3] {
        let mut s2 = s.clone();
        let at = n.point(t).truncate();
        line(&mut s2, (at.x, at.y + 5.0), (at.x, -5.0));
        let found = s2.profiles();
        assert!(found.rejected.is_empty(), "{t}: {:?}", found.rejected);
        assert_eq!(found.profiles.len(), 2, "{t}");
        let sum: f64 = found
            .profiles
            .iter()
            .map(|p| p.outer.signed_area().abs())
            .sum();
        // Die Fläche der Schleife ist abgetastet, darum nur auf etwa 1e-4 genau.
        assert!((sum / whole - 1.0).abs() < 2e-4, "{t}: {sum} statt {whole}");
        // Die Teilstücke selbst sind exakt: dieselbe Kurve im selben Parameter.
        for pr in &found.profiles {
            for seg in &pr.outer.segments {
                if let Segment::Nurbs { curve } = seg {
                    let (a, b) = curve.domain();
                    for i in 0..=200 {
                        let u = a + (b - a) * i as f64 / 200.0;
                        let q = curve.point(u).truncate();
                        let on = s2.nearest_on_curve(c, q).unwrap();
                        assert!(on.distance(q) < 1e-9, "{t}: {q}");
                    }
                }
            }
        }
        let parts: Vec<_> = found
            .refs
            .iter()
            .flat_map(|r| r.outer.iter().filter(|p| p.curve == c).cloned())
            .collect();
        assert_eq!(parts.len(), 2);
        for p in &parts {
            assert!(
                (0.0..=1.0).contains(&p.u0) && (0.0..=1.0).contains(&p.u1),
                "{p:?}"
            );
        }
    }
    s.solve();
}

#[test]
fn a_trimmed_control_spline_still_makes_a_profile() {
    let mut s = sketch();
    let pts = points(&mut s, &WAVE);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let cut = line(&mut s, (9.0, 20.0), (9.0, -20.0));
    let click = s.curve_eval(c, 5.5).unwrap();
    s.trim(c, click).unwrap();
    // Bereich jetzt nicht mehr bei 0 bis n − p: die Kurve zusammen mit zwei Linien schließen.
    let ctrl = control_points(&s, c);
    let end = *ctrl.last().unwrap();
    let e = s.point_xy(end).unwrap();
    let foot = s.add_point(e.x, -8.0).unwrap();
    let left = s.add_point(0.0, -8.0).unwrap();
    s.add_line(end, foot).unwrap();
    s.add_line(foot, left).unwrap();
    s.add_line(left, pts[0]).unwrap();
    s.remove_curve(cut).unwrap();
    let found = s.profiles();
    assert!(found.rejected.is_empty(), "{:?}", found.rejected);
    assert_eq!(found.profiles.len(), 1);
}

#[test]
fn degenerate_control_polygon_profile_gives_at_most_one_reason() {
    type Case = (&'static str, Vec<(f64, f64)>);
    // Review Focus 2: Die Kurve schließt mit Linien zu einer Schleife; ihr Kontrollpolygon ist
    // entartet. Es gibt ein Profil oder genau einen Grund, keine Panik und keine stille Lücke.
    let cases: Vec<Case> = vec![
        (
            "alle auf einer Geraden",
            vec![(0.0, 0.0), (2.0, 0.0), (5.0, 0.0), (7.0, 0.0), (10.0, 0.0)],
        ),
        (
            "auf einer Geraden, rückläufig",
            vec![(0.0, 0.0), (6.0, 0.0), (3.0, 0.0), (8.0, 0.0), (10.0, 0.0)],
        ),
        (
            "zwei gleiche innere Punkte",
            vec![(0.0, 0.0), (2.0, 5.0), (5.0, 7.0), (5.0, 7.0), (10.0, 0.0)],
        ),
        (
            "der zweite Punkt auf dem ersten",
            vec![(0.0, 0.0), (0.0, 0.0), (5.0, 7.0), (8.0, 6.0), (10.0, 0.0)],
        ),
        (
            "der vorletzte Punkt auf dem letzten",
            vec![(0.0, 0.0), (2.0, 5.0), (5.0, 7.0), (10.0, 0.0), (10.0, 0.0)],
        ),
        (
            "alle bis auf die Enden an einer Stelle",
            vec![(0.0, 0.0), (5.0, 5.0), (5.0, 5.0), (5.0, 5.0), (10.0, 0.0)],
        ),
    ];
    for degree in [3, 5] {
        for (name, xy) in &cases {
            // Punkte zuerst ohne Doppelte anlegen, danach zusammenziehen wie beim Lösen.
            let mut s = sketch();
            let pts: Vec<PointId> = xy
                .iter()
                .enumerate()
                .map(|(i, _)| s.add_point(i as f64, 100.0).unwrap())
                .collect();
            let c = s.add_control_spline(&pts, degree).unwrap();
            for (p, q) in pts.iter().zip(xy) {
                s.set_point_xy(*p, q.0, q.1).unwrap();
            }
            // Zwei Linien unter der Kurve, so hat die Schleife immer eine Fläche.
            let foot = s.add_point(5.0, -8.0).unwrap();
            let a = s.add_line(pts[4], foot).unwrap();
            let b = s.add_line(foot, pts[0]).unwrap();
            for (way, found) in [("Anordnung", s.profiles()), ("alt", s.legacy_profiles())] {
                let reasons = found
                    .rejected
                    .iter()
                    .filter(|r| r.curves.iter().any(|k| [a, b, c].contains(k)))
                    .count();
                let stated = found
                    .rejected
                    .iter()
                    .all(|r| !r.reason.to_string().is_empty());
                assert!(stated, "{name}, {way}");
                assert!(
                    (found.profiles.len() == 1 && reasons == 0)
                        || (found.profiles.is_empty() && reasons == 1),
                    "{name}, Grad {degree}, {way}: {} Profile, Gründe {:?}",
                    found.profiles.len(),
                    found.rejected
                );
            }
        }
    }

    // Alle Kontrollpunkte auf einer Stelle: Länge null, genau ein Grund.
    let mut s = sketch();
    let pts: Vec<PointId> = (0..5)
        .map(|i| s.add_point(i as f64, 100.0).unwrap())
        .collect();
    let c = s.add_control_spline(&pts, 3).unwrap();
    for &p in &pts {
        s.set_point_xy(p, 2.0, 2.0).unwrap();
    }
    let found = s.profiles();
    assert!(found.profiles.is_empty());
    assert_eq!(found.rejected.len(), 1, "{:?}", found.rejected);
    assert_eq!(found.rejected[0].curves, vec![c]);
    assert!(!found.rejected[0].reason.to_string().is_empty());
    let legacy = s.legacy_profiles();
    assert!(legacy.profiles.is_empty());
    assert!(legacy.rejected.len() <= 1, "{:?}", legacy.rejected);

    // Allein geschlossen und entartet: ebenfalls höchstens ein Grund.
    let mut s = sketch();
    let pts: Vec<PointId> = (0..4)
        .map(|i| s.add_point(i as f64, 100.0).unwrap())
        .collect();
    s.add_control_spline(&pts, 3).unwrap();
    for (p, q) in pts
        .iter()
        .zip([(0.0, 0.0), (3.0, 0.0), (6.0, 0.0), (0.0, 0.0)])
    {
        s.set_point_xy(*p, q.0, q.1).unwrap();
    }
    let found = s.profiles();
    assert!(found.profiles.is_empty(), "{:?}", found.profiles);
    assert!(found.rejected.len() <= 1, "{:?}", found.rejected);
    let legacy = s.legacy_profiles();
    assert!(legacy.profiles.is_empty(), "{:?}", legacy.profiles);
    assert!(legacy.rejected.len() <= 1, "{:?}", legacy.rejected);
}

#[test]
fn fillet_at_a_shared_end_drops_the_old_corner() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let old = s.nurbs_of(c).unwrap();
    let far = s.add_point(14.0, -20.0).unwrap();
    let l = s.add_line(pts[5], far).unwrap();
    let plan = s
        .fillet_plan(pick(l, 14.0, -15.0), pick(c, 5.0, 5.0), 1.0)
        .expect("Plan");
    assert_eq!(plan.corner, Some(pts[5]));
    assert_eq!(plan.moves[0], EndMove::Replace(pts[5]));
    assert!(matches!(plan.moves[1], EndMove::Cut { end, .. } if end == pts[5]));
    let ids = s.add_fillet(&plan, None).unwrap();
    assert!(ids.corner.is_none());
    assert!(s.point_xy(pts[5]).is_err(), "die alte Ecke entfällt");
    assert_same_shape(&s, c, &old, 1e-9);
    Sketch::from_data(s.to_data()).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
}

#[test]
fn fillet_at_a_corner_still_in_use_stays_solvable() {
    // Die Ecke bleibt, weil eine dritte Kurve oder „deckungsgleich“ daran hängt. Sie liegt nach dem
    // Kürzen nicht mehr auf dem Spline, darf dort also auch nicht festgehalten werden.
    for r in [1.0, 3.0] {
        for variant in ["dritte Kurve", "deckungsgleich", "Linie fest"] {
            let mut s = sketch();
            let pts = points(&mut s, &ARCH);
            let c = s.add_control_spline(&pts, 3).unwrap();
            let old = s.nurbs_of(c).unwrap();
            let far = s.add_point(14.0, -20.0).unwrap();
            let l = s.add_line(pts[5], far).unwrap();
            match variant {
                "dritte Kurve" => {
                    let q = s.add_point(30.0, 5.0).unwrap();
                    s.add_line(pts[5], q).unwrap();
                }
                "deckungsgleich" => {
                    let q = s.add_point(14.0, 0.0).unwrap();
                    s.add_constraint(K::Coincident(pts[5], q)).unwrap();
                }
                _ => {
                    s.add_constraint(K::Fixed(pts[5], 14.0, 0.0)).unwrap();
                }
            }
            let plan = s
                .fillet_plan(pick(l, 14.0, -15.0), pick(c, 5.0, 5.0), r)
                .expect("Plan");
            assert_eq!(plan.corner, Some(pts[5]), "{variant}");
            let ids = s.add_fillet(&plan, None).unwrap();
            let held_on_spline = s
                .constraints()
                .any(|(_, d)| matches!(d.kind, K::PointOnCurve(p, k) if p == pts[5] && k == c));
            assert!(!held_on_spline, "{variant}, r = {r}");
            Sketch::from_data(s.to_data()).unwrap();
            let rep = s.solve();
            assert_eq!(
                rep.status,
                SolveStatus::Solved,
                "{variant}, r = {r}: {rep:?}"
            );
            assert!(rep.conflicting.is_empty(), "{variant}, r = {r}: {rep:?}");
            if matches!(plan.moves[1], EndMove::Cut { .. }) {
                assert_eq!(ids.corner, Some(pts[5]), "{variant}");
                assert_same_shape(&s, c, &old, 1e-6);
            }
        }
    }
}

#[test]
fn extend_reaches_a_near_target_on_long_curves() {
    // Die Fortsetzung eines hohen Grades wächst über viele Bereichsbreiten über den Rechenbereich
    // hinaus; ein Ziel dicht hinter dem Ende muss trotzdem getroffen werden.
    for n in [24, 32, 48, 64] {
        for degree in [3, 5] {
            for past in [0.02, 0.5] {
                let mut s = sketch();
                let xy: Vec<(f64, f64)> = (0..n)
                    .map(|i| (i as f64, 6.0 * (i as f64 * 0.7).sin()))
                    .collect();
                let pts = points(&mut s, &xy);
                let c = s.add_control_spline(&pts, degree).unwrap();
                let old = s.nurbs_of(c).unwrap();
                let (_, hi) = old.domain();
                let last = *pts.last().unwrap();
                let from = s.point_xy(last).unwrap();
                let tip = old.point_unclamped(hi + past).truncate();
                let d = old.derivs_unclamped(hi + past, 1)[1].truncate();
                let m = d.normalize().perp() * 2.0;
                line(
                    &mut s,
                    (tip.x - m.x, tip.y - m.y),
                    (tip.x + m.x, tip.y + m.y),
                );
                assert!(
                    s.extend(c, from).unwrap(),
                    "{n} Punkte, Grad {degree}, {past}"
                );
                let end = s.point_xy(last).unwrap();
                assert!(
                    end.distance(tip) < 1e-9,
                    "{n}, {degree}, {past}: {end} statt {tip}"
                );
                assert_same_shape(&s, c, &old, 1e-6);
                Sketch::from_data(s.to_data()).unwrap();
            }
        }
    }
}

#[test]
fn fillet_of_two_control_splines_at_a_corner_in_use_cuts_both() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let a = s.add_control_spline(&pts, 3).unwrap();
    let other = points(
        &mut s,
        &[(18.0, 2.0), (22.0, 7.0), (28.0, 6.0), (32.0, 0.0)],
    );
    let mut chain = vec![pts[5]];
    chain.extend(&other);
    let b = s.add_control_spline(&chain, 3).unwrap();
    let (old_a, old_b) = (s.nurbs_of(a).unwrap(), s.nurbs_of(b).unwrap());
    let q = s.add_point(14.0, -20.0).unwrap();
    s.add_line(pts[5], q).unwrap();
    let plan = s
        .fillet_plan(pick(a, 5.0, 5.0), pick(b, 24.0, 5.0), 0.5)
        .expect("Plan");
    assert!(
        matches!(plan.moves[0], EndMove::Cut { .. }),
        "{:?}",
        plan.moves
    );
    assert!(
        matches!(plan.moves[1], EndMove::Cut { .. }),
        "{:?}",
        plan.moves
    );
    s.add_fillet(&plan, None).unwrap();
    assert!(s.point_xy(pts[5]).is_ok(), "die dritte Kurve hält die Ecke");
    assert!(
        !s.constraints()
            .any(|(_, d)| matches!(d.kind, K::PointOnCurve(p, _) if p == pts[5]))
    );
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert_same_shape(&s, a, &old_a, 1e-6);
    assert_same_shape(&s, b, &old_b, 1e-6);
}
