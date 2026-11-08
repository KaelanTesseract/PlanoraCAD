//! Tangentengriff am Spline durch Stützpunkte (Skizzen 7, Task 6; Spec B1, B3, B4, B5, B8).

use planora_kernel::{ProfileLoop, Segment};
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::{Drag, SolveStatus};
use planora_sketch::transform::Similarity;
use planora_sketch::{HandleCurvature, SketchError};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn spline(s: &mut Sketch, xy: &[(f64, f64)]) -> (CurveId, Vec<PointId>) {
    let pts: Vec<PointId> = xy
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    (s.add_spline(&pts).unwrap(), pts)
}

const WAVE: [(f64, f64); 6] = [
    (0.0, 0.0),
    (10.0, 6.0),
    (21.0, -3.0),
    (30.0, 8.0),
    (42.0, 5.0),
    (50.0, -7.0),
];

/// Die Griffspitze zum Stützpunkt `at`.
fn tip_of(s: &Sketch, c: CurveId, at: PointId) -> PointId {
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    handles.iter().find(|h| h.at == at).unwrap().tip
}

fn handle_count(s: &Sketch, c: CurveId) -> usize {
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    handles.len()
}

fn dense(s: &Sketch, c: CurveId, per_segment: usize) -> Vec<DVec2> {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let steps = (hi - lo) as usize * per_segment;
    (0..=steps)
        .map(|k| {
            s.curve_eval(c, lo + (hi - lo) * k as f64 / steps as f64)
                .unwrap()
        })
        .collect()
}

/// Einfacher, fester Zufall (keine Abhängigkeit nötig).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let v = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (v >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Zahl in `[-a, a]`.
    fn range(&mut self, a: f64) -> f64 {
        (self.next() * 2.0 - 1.0) * a
    }
}

/// Schrittweite der einseitigen Formeln, Stellen liegen alle im selben Segment.
const STEP: f64 = 0.02;

/// Die sechs Lagen ab `t` zur Seite `side`.
fn six(s: &Sketch, c: CurveId, t: f64, side: f64) -> [DVec2; 6] {
    std::array::from_fn(|k| s.curve_eval(c, t + side * STEP * k as f64).unwrap())
}

/// Ableitung von außen über die Lage: einseitig mit sechs Stellen, also exakt für Grad 5.
fn derivative(s: &Sketch, c: CurveId, t: f64, side: f64) -> DVec2 {
    let f = six(s, c, t, side);
    (f[0] * (-137.0 / 60.0) + f[1] * 5.0 - f[2] * 5.0 + f[3] * (10.0 / 3.0) - f[4] * 1.25
        + f[5] * 0.2)
        / STEP
        * side
}

/// Zweite Ableitung von außen, ebenso exakt für Grad 5.
fn second(s: &Sketch, c: CurveId, t: f64, side: f64) -> DVec2 {
    let f = six(s, c, t, side);
    (f[0] * (15.0 / 4.0) - f[1] * (77.0 / 6.0) + f[2] * (107.0 / 6.0) - f[3] * 13.0
        + f[4] * (61.0 / 12.0)
        - f[5] * (5.0 / 6.0))
        / (STEP * STEP)
}

/// Die zweiten Ableitungen des natürlichen Splines, Rechenschritt für Rechenschritt wie vor den
/// Griffen (`spline_second_derivatives`).
fn old_second(y: &[f64]) -> Vec<f64> {
    let n = y.len();
    let mut m = vec![0.0; n];
    let k = n - 2;
    let mut c = vec![0.0; k];
    let mut d = Vec::with_capacity(k);
    for i in 0..k {
        let rhs = 6.0 * (y[i] - 2.0 * y[i + 1] + y[i + 2]);
        if i == 0 {
            c[0] = 1.0 / 4.0;
            d.push(rhs / 4.0);
        } else {
            let den = 4.0 - c[i - 1];
            c[i] = 1.0 / den;
            d.push((rhs - d[i - 1]) / den);
        }
    }
    let mut x = d.clone();
    for i in (0..k - 1).rev() {
        x[i] = d[i] - c[i] * x[i + 1];
    }
    m[1..=k].copy_from_slice(&x);
    m
}

/// Eine Koordinate wie `Geom::eval` vor den Griffen.
fn old_geom(y: &[f64], m: &[f64], t: f64) -> f64 {
    let i = (t.floor().max(0.0) as usize).min(y.len() - 2);
    let u = t - i as f64;
    let w = 1.0 - u;
    y[i] * w + y[i + 1] * u + m[i] * ((w * w * w - w) / 6.0) + m[i + 1] * ((u * u * u - u) / 6.0)
}

/// Eine Koordinate wie `spline_eval` vor den Griffen.
fn old_sampled(y: &[f64], m: &[f64], t: f64) -> f64 {
    let i = (t.floor().max(0.0) as usize).min(y.len() - 2);
    let u = t - i as f64;
    let w = 1.0 - u;
    w * y[i] + u * y[i + 1] + (w * w * w - w) * m[i] / 6.0 + (u * u * u - u) * m[i + 1] / 6.0
}

#[test]
fn no_handles_is_bit_equal_to_the_old_spline() {
    let mut s = sketch();
    let (c, _) = spline(&mut s, &WAVE);
    let m = [
        old_second(&WAVE.map(|p| p.0)),
        old_second(&WAVE.map(|p| p.1)),
    ];
    let y = [WAVE.map(|p| p.0), WAVE.map(|p| p.1)];
    for k in 0..=2000 {
        let t = 5.0 * k as f64 / 2000.0;
        // Bit für Bit wie vor den Griffen: `Geom::eval` und `Sampled::eval` haben je ihre Formel.
        let geom = DVec2::new(old_geom(&y[0], &m[0], t), old_geom(&y[1], &m[1], t));
        assert_eq!(s.curve_eval(c, t).unwrap(), geom, "t={t}");
        let sampled = DVec2::new(old_sampled(&y[0], &m[0], t), old_sampled(&y[1], &m[1], t));
        assert_eq!(s.spline_point(c, t).unwrap(), sampled, "t={t}");
    }
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!();
    };
    assert!(handles.is_empty());
}

#[test]
fn activating_does_not_change_the_shape() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for run in 0..60 {
        let n = 2 + (rng.next() * 9.0) as usize;
        let mut xy = Vec::with_capacity(n);
        let mut x = 0.0;
        for _ in 0..n {
            x += 3.0 + rng.next() * 20.0;
            xy.push((x, rng.range(40.0)));
        }
        let mut s = sketch();
        let (c, pts) = spline(&mut s, &xy);
        let before = dense(&s, c, 40);
        // Griffe in zufälliger Folge einschalten, nach jedem bleibt die Form.
        let mut order = pts.clone();
        for i in (1..order.len()).rev() {
            order.swap(i, (rng.next() * (i + 1) as f64) as usize);
        }
        for &at in &order {
            s.activate_tangent_handle(c, at).unwrap();
            for (a, b) in dense(&s, c, 40).iter().zip(&before) {
                assert!(a.distance(*b) < 1e-9, "Lauf {run}: {a} statt {b}");
            }
        }
        assert_eq!(handle_count(&s, c), n);
    }
}

#[test]
fn activating_a_second_handle_after_a_drag_does_not_change_the_shape() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_tangent_handle(c, pts[2]).unwrap();
    let tip = tip_of(&s, c, pts[2]);
    let q = s.point_xy(tip).unwrap() + DVec2::new(3.0, -4.0);
    s.set_point_xy(tip, q.x, q.y).unwrap();
    let before = dense(&s, c, 40);
    for i in [0, 3, 1, 5, 4] {
        s.activate_tangent_handle(c, pts[i]).unwrap();
        for (a, b) in dense(&s, c, 40).iter().zip(&before) {
            assert!(a.distance(*b) < 1e-9, "{a} statt {b}");
        }
    }
}

#[test]
fn tip_sets_the_derivative() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let line = s.activate_tangent_handle(c, pts[2]).unwrap();
    assert!(s.curve(line).unwrap().construction);
    assert_eq!(
        s.curve(line).unwrap().kind,
        CurveKind::Line {
            a: pts[2],
            b: tip_of(&s, c, pts[2])
        }
    );
    let tip = tip_of(&s, c, pts[2]);
    let at = s.point_xy(pts[2]).unwrap();
    for offset in [(5.0, 1.0), (-2.0, 7.0), (0.5, 0.25), (20.0, -9.0)] {
        let q = at + DVec2::new(offset.0, offset.1);
        s.set_point_xy(tip, q.x, q.y).unwrap();
        let want = (q - at) * 3.0;
        for side in [-1.0, 1.0] {
            let got = derivative(&s, c, 2.0, side);
            assert!(got.distance(want) < 1e-5, "{got} statt {want}");
        }
    }
    // Enden und ein einzelnes Stück, an beiden Enden vorgegeben
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(0.0, 0.0), (10.0, 0.0)]);
    s.activate_tangent_handle(c, pts[0]).unwrap();
    s.activate_tangent_handle(c, pts[1]).unwrap();
    let (t0, t1) = (tip_of(&s, c, pts[0]), tip_of(&s, c, pts[1]));
    s.set_point_xy(t0, 1.0, 4.0).unwrap();
    s.set_point_xy(t1, 12.0, 3.0).unwrap();
    assert!(derivative(&s, c, 0.0, 1.0).distance(DVec2::new(3.0, 12.0)) < 1e-5);
    assert!(derivative(&s, c, 1.0, -1.0).distance(DVec2::new(6.0, 9.0)) < 1e-5);
}

#[test]
fn curve_is_g2_across_a_handle() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_tangent_handle(c, pts[2]).unwrap();
    s.activate_tangent_handle(c, pts[4]).unwrap();
    let at = s.point_xy(pts[2]).unwrap();
    let tip = tip_of(&s, c, pts[2]);
    let mut seen_change = false;
    for k in 0..40 {
        let a = k as f64 * 0.31;
        let q = at + DVec2::new(a.cos(), a.sin()) * (2.0 + 0.3 * k as f64);
        s.set_point_xy(tip, q.x, q.y).unwrap();
        // Nicht an den Enden: Dort gibt es nur eine Seite.
        for t in [1.0, 2.0, 3.0, 4.0] {
            // innerer Sprung über die Fuge, gemessen von außen über die Lage
            let (d0, d1) = (derivative(&s, c, t, -1.0), derivative(&s, c, t, 1.0));
            let (a0, a1) = (second(&s, c, t, -1.0), second(&s, c, t, 1.0));
            assert!(d0.distance(d1) < 1e-6, "Tangente bei {t}: {d0} / {d1}");
            let kappa = |d: DVec2, a: DVec2| d.perp_dot(a) / d.length().powi(3);
            let jump = (kappa(d0, a0) - kappa(d1, a1)).abs();
            assert!(jump < 1e-6, "Krümmung bei {t}: Sprung {jump}");
            if t == 2.0 && kappa(d0, a0).abs() > 1e-3 {
                seen_change = true;
            }
        }
    }
    assert!(seen_change);
}

#[test]
fn horizontal_on_the_handle_line_holds_after_solve() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let line = s.activate_tangent_handle(c, pts[2]).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(line)))
        .unwrap();
    let report = s.solve();
    assert_eq!(report.status, SolveStatus::Solved);
    let tip = tip_of(&s, c, pts[2]);
    let (a, b) = (s.point_xy(pts[2]).unwrap(), s.point_xy(tip).unwrap());
    assert!((a.y - b.y).abs() < 1e-7);
    assert!(derivative(&s, c, 2.0, 1.0).y.abs() < 1e-4);
    assert!(derivative(&s, c, 2.0, -1.0).y.abs() < 1e-4);
}

#[test]
fn length_dimension_on_the_handle_works() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_tangent_handle(c, pts[3]).unwrap();
    let tip = tip_of(&s, c, pts[3]);
    s.add_constraint(K::Distance(pts[3], tip, 5.0)).unwrap();
    let report = s.solve();
    assert_eq!(report.status, SolveStatus::Solved);
    let d = derivative(&s, c, 3.0, 1.0).length();
    assert!((d - 15.0).abs() < 1e-4, "{d}");
}

#[test]
fn handle_on_two_point_spline_and_tiny_derivative() {
    // Zwei Punkte: Die Ableitung ist die Sehne.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(1.0, 1.0), (10.0, 4.0)]);
    let before = dense(&s, c, 40);
    s.activate_tangent_handle(c, pts[0]).unwrap();
    let tip = s.point_xy(tip_of(&s, c, pts[0])).unwrap();
    assert!(tip.distance(DVec2::new(4.0, 2.0)) < 1e-12, "{tip}");
    for (a, b) in dense(&s, c, 40).iter().zip(&before) {
        assert!(a.distance(*b) < 1e-9);
    }
    // Hin und zurück: Am mittleren Stützpunkt steht die Kurve (Ableitung genau null).
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(0.0, 0.0), (10.0, 0.0), (0.0, 0.0)]);
    let curves = s.curves().count();
    let points = s.points().count();
    let err = s.activate_tangent_handle(c, pts[1]).unwrap_err();
    assert!(matches!(err, SketchError::Degenerate(_)), "{err:?}");
    assert_eq!(s.curves().count(), curves);
    assert_eq!(s.points().count(), points);
    assert_eq!(handle_count(&s, c), 0);
    // Fast null, knapp über der Toleranz: Am mittleren Punkt ist die Spitze |y₂ − y₀| / 6 entfernt,
    // hier etwa 1,67e-6 bei `tol.linear` = 1e-6. Der Griff entsteht, die Form bleibt, alles endlich.
    let mut tiny = sketch();
    let (k, ids) = spline(&mut tiny, &[(0.0, 0.0), (10.0, 0.0), (1e-5, 0.0)]);
    let before = dense(&tiny, k, 40);
    tiny.activate_tangent_handle(k, ids[1]).unwrap();
    let reach = tiny
        .point_xy(tip_of(&tiny, k, ids[1]))
        .unwrap()
        .distance(tiny.point_xy(ids[1]).unwrap());
    assert!((reach - 1e-5 / 6.0).abs() < 1e-9, "{reach}");
    let after = dense(&tiny, k, 40);
    for (a, b) in after.iter().zip(&before) {
        assert!(a.is_finite());
        assert!(a.distance(*b) < 1e-9, "{a} statt {b}");
    }
    assert!(second(&tiny, k, 1.0, 1.0).is_finite());
    assert!(tiny.handled_spline_nurbs(k).is_ok());
    assert_eq!(tiny.solve().status, SolveStatus::Solved);
    assert!(tiny.curve_eval(k, 0.5).unwrap().is_finite());
    // Fast null, unter der Toleranz (Spitze etwa 1,67e-7 entfernt): abgelehnt, nichts bleibt zurück.
    let mut tiny = sketch();
    let (k, ids) = spline(&mut tiny, &[(0.0, 0.0), (10.0, 0.0), (1e-6, 0.0)]);
    let before = tiny.to_data();
    let err = tiny.activate_tangent_handle(k, ids[1]).unwrap_err();
    assert!(matches!(err, SketchError::Degenerate(_)), "{err:?}");
    assert_eq!(tiny.to_data(), before);
    // Kein Griff an einem fremden Punkt, kein zweiter am selben, keiner an einer Linie.
    let other = s.add_point(50.0, 50.0).unwrap();
    assert!(s.activate_tangent_handle(c, other).is_err());
    s.activate_tangent_handle(c, pts[0]).unwrap();
    assert!(s.activate_tangent_handle(c, pts[0]).is_err());
    assert_eq!(handle_count(&s, c), 1);
    let q = s.add_point(60.0, 50.0).unwrap();
    let plain = s.add_line(other, q).unwrap();
    assert!(matches!(
        s.activate_tangent_handle(plain, other),
        Err(SketchError::WrongType(_))
    ));
    assert!(s.deactivate_handle(c, pts[2]).is_err());
}

#[test]
fn deleting_the_line_deactivates() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let natural = dense(&s, c, 20);
    let line = s.activate_tangent_handle(c, pts[2]).unwrap();
    let tip = tip_of(&s, c, pts[2]);
    s.set_point_xy(tip, 40.0, 30.0).unwrap();
    assert!(
        dense(&s, c, 20)
            .iter()
            .zip(&natural)
            .any(|(a, b)| a.distance(*b) > 1.0)
    );
    s.remove_curve(line).unwrap();
    assert_eq!(handle_count(&s, c), 0);
    assert!(s.curve(c).is_some());
    assert!(s.curve(line).is_none());
    assert!(s.point_xy(tip).is_err());
    assert!(s.point_xy(pts[2]).is_ok(), "der Stützpunkt bleibt");
    // Zurück auf der Form ohne Griff
    for (a, b) in dense(&s, c, 20).iter().zip(&natural) {
        assert!(a.distance(*b) < 1e-9);
    }
    // Wie die Oberfläche: Waisen aufräumen darf den Stützpunkt nicht treffen.
    s.prune_points(&pts);
    assert!(s.point_xy(pts[2]).is_ok());

    // `deactivate_handle` tut dasselbe.
    let line = s.activate_tangent_handle(c, pts[1]).unwrap();
    let tip = tip_of(&s, c, pts[1]);
    s.add_constraint(K::Horizontal(LineOrPoints::Line(line)))
        .unwrap();
    let dist = s.add_constraint(K::Distance(pts[1], tip, 5.0)).unwrap();
    s.deactivate_handle(c, pts[1]).unwrap();
    assert_eq!(handle_count(&s, c), 0);
    assert!(s.curve(line).is_none());
    assert!(s.point_xy(tip).is_err());
    assert!(s.constraint(dist).is_none());
    assert_eq!(s.constraints().count(), 0);

    // Die Spitze löschen nimmt den Griff weg, aber nicht den Spline.
    let line = s.activate_tangent_handle(c, pts[3]).unwrap();
    let tip = tip_of(&s, c, pts[3]);
    s.remove_point(tip).unwrap();
    assert!(s.curve(c).is_some());
    assert!(s.curve(line).is_none());
    assert_eq!(handle_count(&s, c), 0);
    assert!(s.point_xy(pts[3]).is_ok());
}

#[test]
fn a_shared_tip_survives_the_handle() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let line = s.activate_tangent_handle(c, pts[2]).unwrap();
    let tip = tip_of(&s, c, pts[2]);
    let far = s.add_point(90.0, 90.0).unwrap();
    let other = s.add_line(tip, far).unwrap();
    s.remove_curve(line).unwrap();
    assert!(s.point_xy(tip).is_ok());
    assert!(s.curve(other).is_some());
    assert_eq!(handle_count(&s, c), 0);
}

#[test]
fn deleting_the_fit_point_takes_the_spline_and_the_handle() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let line = s.activate_tangent_handle(c, pts[2]).unwrap();
    let tip = tip_of(&s, c, pts[2]);
    s.remove_point(pts[2]).unwrap();
    assert!(s.curve(c).is_none());
    assert!(s.curve(line).is_none());
    assert!(s.point_xy(tip).is_err());
}

#[test]
fn deleting_the_spline_removes_handles() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let lines = [
        s.activate_tangent_handle(c, pts[0]).unwrap(),
        s.activate_tangent_handle(c, pts[3]).unwrap(),
    ];
    let tips = [tip_of(&s, c, pts[0]), tip_of(&s, c, pts[3])];
    s.remove_curve(c).unwrap();
    for l in lines {
        assert!(s.curve(l).is_none());
    }
    for t in tips {
        assert!(s.point_xy(t).is_err());
    }
    assert_eq!(s.curves().count(), 0);
    // Die Stützpunkte räumt wie bisher der Aufrufer auf.
    assert_eq!(s.prune_unused_points(), pts.len());
    // Geladen ist alles noch stimmig.
    let back = Sketch::from_data(s.to_data()).unwrap();
    assert_eq!(back.curves().count(), 0);
}

#[test]
fn handle_is_not_a_profile_edge() {
    // Schließt mit einer Linie ab; der Griff ragt ins Innere und nach außen.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(0.0, 0.0), (10.0, 12.0), (20.0, 0.0)]);
    s.add_line(pts[2], pts[0]).unwrap();
    let before = s.profiles();
    assert_eq!(before.profiles.len(), 1);
    let line = s.activate_tangent_handle(c, pts[1]).unwrap();
    let tip = tip_of(&s, c, pts[1]);
    s.set_point_xy(tip, 18.0, 13.0).unwrap();
    let found = s.profiles();
    assert_eq!(found.profiles.len(), 1, "{:?}", found.rejected);
    assert!(found.rejected.is_empty());
    assert!(!found.curve_loops[0].contains(&line));
    let nurbs = found.profiles[0]
        .outer
        .segments
        .iter()
        .find_map(|seg| match seg {
            Segment::Nurbs { curve } => Some(curve.clone()),
            _ => None,
        })
        .expect("Freiformsegment");
    assert_eq!(nurbs.degree(), 5, "exakt als Grad 5");
    // Auch die alte Profilsuche nimmt die exakte Kurve und keinen Griff.
    let legacy = s.legacy_profiles();
    assert_eq!(legacy.profiles.len(), 1, "{:?}", legacy.rejected);
    assert!(
        legacy.profiles[0]
            .outer
            .segments
            .iter()
            .any(|seg| matches!(seg, Segment::Nurbs { curve } if curve.degree() == 5))
    );
}

#[test]
fn handled_spline_nurbs_is_exact() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    assert!(s.handled_spline_nurbs(c).is_ok(), "ohne Griff auch");
    s.activate_tangent_handle(c, pts[1]).unwrap();
    s.activate_tangent_handle(c, pts[4]).unwrap();
    let tip = tip_of(&s, c, pts[1]);
    s.set_point_xy(tip, 16.0, 12.0).unwrap();
    let n = s.handled_spline_nurbs(c).unwrap();
    assert_eq!(n.degree(), 5);
    assert_eq!(n.domain(), (0.0, 5.0));
    // Innere Knoten dreifach, Enden sechsfach
    let knots = n.knots().knots();
    assert_eq!(knots.len(), 6 + 4 * 3 + 6);
    for k in 1..=4 {
        let count = knots.iter().filter(|x| **x == k as f64).count();
        assert_eq!(count, 3, "Knoten {k}");
    }
    for i in 0..=2000 {
        let t = 5.0 * i as f64 / 2000.0;
        let q = n.point(t);
        let got = DVec2::new(q.x, q.y);
        let want = s.curve_eval(c, t).unwrap();
        assert!(got.distance(want) < 1e-9, "t={t}: {got} statt {want}");
    }
}

#[test]
fn dragging_tip_keeps_other_fit_points_fixed() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_tangent_handle(c, pts[2]).unwrap();
    let tip = tip_of(&s, c, pts[2]);
    let before: Vec<DVec2> = pts.iter().map(|&p| s.point_xy(p).unwrap()).collect();
    let target = DVec2::new(30.0, 20.0);
    let report = s.drag(&[Drag { point: tip, target }]).unwrap();
    assert!(!report.drag_failed);
    assert!(s.point_xy(tip).unwrap().distance(target) < 1e-6);
    for (&p, b) in pts.iter().zip(&before) {
        assert_eq!(s.point_xy(p).unwrap(), *b);
    }
    // Der Spline geht durch alle Stützpunkte, die Kurve folgt der Spitze.
    for (i, b) in before.iter().enumerate() {
        assert!(s.curve_eval(c, i as f64).unwrap().distance(*b) < 1e-12);
    }
    let want = (target - before[2]) * 3.0;
    assert!(derivative(&s, c, 2.0, 1.0).distance(want) < 1e-5);
    // Auch mit einer Abhängigkeit am Griff: der Löser zieht, andere Stützpunkte bleiben.
    s.add_constraint(K::Distance(pts[2], tip, 8.0)).unwrap();
    s.solve();
    let others = [0, 1, 3, 4, 5].map(|i| s.point_xy(pts[i]).unwrap());
    let target = DVec2::new(40.0, 3.0);
    s.drag(&[Drag { point: tip, target }]).unwrap();
    for (i, q) in [0, 1, 3, 4, 5].iter().zip(others) {
        assert!(s.point_xy(pts[*i]).unwrap().distance(q) < 1e-9);
    }
}

/// Der Kurvenparameter von „Punkt auf Kurve“.
fn parameter(s: &Sketch, k: planora_sketch::ConstraintId) -> f64 {
    s.to_data().vars[s.constraint(k).unwrap().extra().unwrap()]
}

#[test]
fn point_on_handled_spline_solves_and_follows_the_tip() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_tangent_handle(c, pts[2]).unwrap();
    let p = s.add_point(15.0, 5.0).unwrap();
    let k = s.add_constraint(K::PointOnCurve(p, c)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    // Griff bewegen: Der Punkt folgt der Kurve.
    let tip = tip_of(&s, c, pts[2]);
    s.drag(&[Drag {
        point: tip,
        target: DVec2::new(26.0, 12.0),
    }])
    .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let t = parameter(&s, k);
    let q = s.point_xy(p).unwrap();
    assert!(s.curve_eval(c, t).unwrap().distance(q) < 1e-6);
    // Griff aus: Die Kurve springt, der Punkt bleibt nach dem Lösen auf ihr.
    s.deactivate_handle(c, pts[2]).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let t = parameter(&s, k);
    assert!(s.curve_eval(c, t).unwrap().distance(s.point_xy(p).unwrap()) < 1e-6);
}

/// Abstand und Winkelfehler (Sinus) der Linie `a`–`b` zur Kurve beim Parameter `t`.
fn tangency(s: &Sketch, c: CurveId, a: PointId, b: PointId, t: f64) -> (f64, f64) {
    let (p, q) = (s.point_xy(a).unwrap(), s.point_xy(b).unwrap());
    let dir = (q - p).normalize();
    let at = s.curve_eval(c, t).unwrap();
    let d = derivative(s, c, t, if t < 4.0 { 1.0 } else { -1.0 }).normalize();
    (dir.perp_dot(at - p).abs(), dir.perp_dot(d).abs())
}

#[test]
fn tangent_line_on_a_handled_spline_solves_and_follows_the_tip() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_tangent_handle(c, pts[2]).unwrap();
    let tip = tip_of(&s, c, pts[2]);
    // Griff weggezogen: Das Segment neben dem Griff ist jetzt wirklich quintisch.
    s.set_point_xy(tip, 26.0, 9.0).unwrap();
    let at = s.curve_eval(c, 1.5).unwrap();
    let d = derivative(&s, c, 1.5, 1.0).normalize() * 8.0;
    let side = d.perp().normalize() * 0.2;
    let a = s
        .add_point(at.x - d.x + side.x, at.y - d.y + side.y)
        .unwrap();
    let b = s
        .add_point(at.x + d.x + side.x, at.y + d.y + side.y)
        .unwrap();
    let line = s.add_line(a, b).unwrap();
    let k = s.add_constraint(K::Tangent(line, c)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let (gap, angle) = tangency(&s, c, a, b, parameter(&s, k));
    assert!(gap < 1e-6, "Abstand {gap}");
    assert!(angle < 1e-5, "Winkel {angle}");
    // Griff weiter ziehen: Die Linie bleibt tangential und die Spitze folgt dem Zug.
    let before = s.point_xy(a).unwrap();
    let target = DVec2::new(27.0, 11.0);
    let report = s.drag(&[Drag { point: tip, target }]).unwrap();
    assert!(!report.drag_failed);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let (gap, angle) = tangency(&s, c, a, b, parameter(&s, k));
    assert!(gap < 1e-6, "Abstand {gap}");
    assert!(angle < 1e-5, "Winkel {angle}");
    assert!(s.point_xy(tip).unwrap().distance(target) < 1e-3);
    assert!(
        s.point_xy(a).unwrap().distance(before) > 1e-4,
        "die Linie folgt"
    );
}

/// Abstand von `q` zum Linienzug `poly`.
fn to_polyline(q: DVec2, poly: &[DVec2]) -> f64 {
    poly.windows(2)
        .map(|w| {
            let d = w[1] - w[0];
            let u = ((q - w[0]).dot(d) / d.length_squared()).clamp(0.0, 1.0);
            q.distance(w[0] + d * u)
        })
        .fold(f64::INFINITY, f64::min)
}

/// Fläche einer Schleife aus Linien und Freiformstücken über das Randintegral (Simpson, fein genug
/// für 1e-6). `ProfileLoop::signed_area` misst nur über Sehnen und liegt hier um etwa 3e-3 daneben.
fn exact_area(lp: &ProfileLoop) -> f64 {
    let mut sum = 0.0;
    let mut p = lp.start;
    for seg in &lp.segments {
        match seg {
            Segment::Line { to } => {
                sum += p.perp_dot(*to);
                p = *to;
            }
            Segment::Nurbs { curve } => {
                let (lo, hi) = curve.domain();
                let steps = 4000;
                let h = (hi - lo) / steps as f64;
                let flux = |t: f64| {
                    let d = curve.derivs_unclamped(t, 1);
                    d[0].truncate().perp_dot(d[1].truncate())
                };
                let mut s = flux(lo) + flux(hi);
                for k in 1..steps {
                    let w = if k % 2 == 1 { 4.0 } else { 2.0 };
                    s += w * flux(lo + h * k as f64);
                }
                sum += s * h / 3.0;
                let q = curve.point(hi);
                p = DVec2::new(q.x, q.y);
            }
            Segment::Arc { .. } => panic!("hier gibt es keine Bögen"),
        }
    }
    (sum / 2.0).abs()
}

#[test]
fn profile_of_a_handled_spline_split_by_a_crossing_line_is_exact() {
    // Schnitt im Segment, genau auf dem dreifachen Knoten und nahe daneben. Enger am Knoten lehnt die
    // Profilsuche auch den natürlichen Spline ab, das hängt nicht an den Griffen (Bericht Task 6).
    for x in [6.3, 9.5, 10.0, 10.5, 14.9] {
        let mut s = sketch();
        let (c, pts) = spline(&mut s, &[(0.0, 0.0), (10.0, 12.0), (20.0, 0.0)]);
        s.add_line(pts[2], pts[0]).unwrap();
        s.activate_tangent_handle(c, pts[1]).unwrap();
        let tip = tip_of(&s, c, pts[1]);
        s.set_point_xy(tip, 15.0, 14.0).unwrap();
        let whole = s.profiles();
        assert_eq!(whole.profiles.len(), 1, "{:?}", whole.rejected);
        let area = exact_area(&whole.profiles[0].outer);

        let a = s.add_point(x, -5.0).unwrap();
        let b = s.add_point(x, 40.0).unwrap();
        s.add_line(a, b).unwrap();
        let found = s.profiles();
        assert!(found.rejected.is_empty(), "x={x}: {:?}", found.rejected);
        assert_eq!(found.profiles.len(), 2, "x={x}");
        // Die Stücke füllen die Fläche ohne Rest.
        let sum: f64 = found.profiles.iter().map(|p| exact_area(&p.outer)).sum();
        assert!((sum - area).abs() < 1e-6, "x={x}: {sum} statt {area}");
        // Jedes Freiformstück bleibt Grad 5 und liegt auf der Kurve.
        let poly = dense(&s, c, 400);
        for p in &found.profiles {
            let pieces: Vec<_> = p
                .outer
                .segments
                .iter()
                .filter_map(|seg| match seg {
                    Segment::Nurbs { curve } => Some(curve.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(pieces.len(), 1, "x={x}");
            let piece = &pieces[0];
            assert_eq!(piece.degree(), 5, "x={x}");
            let (lo, hi) = piece.domain();
            for k in 0..=200 {
                let q = piece.point(lo + (hi - lo) * k as f64 / 200.0);
                let gap = to_polyline(DVec2::new(q.x, q.y), &poly);
                assert!(gap < 1e-4, "x={x}: {gap}");
            }
        }
    }
}

#[test]
fn a_valid_arc_handle_loads() {
    let mut s = sketch();
    let (c, pts) = spline(
        &mut s,
        &[(0.0, 0.0), (10.0, 6.0), (20.0, -2.0), (30.0, 3.0)],
    );
    // Skizzen 7: Der Krümmungsgriff kommt jetzt vom Befehl, seine interne Zeile gehört dazu.
    let line = s.activate_tangent_handle(c, pts[1]).unwrap();
    let tip = tip_of(&s, c, pts[1]);
    let arc = s.activate_curvature_handle(c, pts[1]).unwrap();
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!();
    };
    let HandleCurvature::Arc { center, other, .. } = handles[0].curvature else {
        panic!();
    };
    let d = s.to_data();
    let mut back = Sketch::from_data(d.clone()).unwrap();
    assert_eq!(back.to_data().curves, d.curves);
    let kind = back.curve(c).unwrap().kind.clone();
    assert!(kind.points().contains(&center) && kind.points().contains(&other));
    assert_eq!(kind.fit_points(), &pts[..]);
    // Andersherum gezeichnet ist der Bogen ebenso gültig.
    let mut flipped = d.clone();
    flipped.curves[arc.index()].kind = CurveKind::Arc {
        center,
        start: other,
        end: pts[1],
    };
    assert!(Sketch::from_data(flipped).is_ok());
    // Bogen ohne Konstruktion oder um eine andere Mitte: abgelehnt.
    let mut plain = d.clone();
    plain.curves[arc.index()].construction = false;
    assert!(Sketch::from_data(plain).is_err());
    let mut off = d;
    off.curves[arc.index()].kind = CurveKind::Arc {
        center: other,
        start: pts[1],
        end: center,
    };
    assert!(Sketch::from_data(off).is_err());
    // Bogen löschen (B8): nur der Krümmungsgriff geht, der Tangentengriff bleibt.
    back.remove_curve(arc).unwrap();
    let CurveKind::Spline { handles, .. } = &back.curve(c).unwrap().kind else {
        panic!();
    };
    assert_eq!(handles.len(), 1);
    assert_eq!(handles[0].curvature, HandleCurvature::Form);
    assert!(back.curve(line).is_some());
    assert!(back.point_xy(tip).is_ok());
    assert!(back.point_xy(center).is_err() && back.point_xy(other).is_err());
    assert!(back.point_xy(pts[1]).is_ok());
}

#[test]
fn moving_the_spline_moves_the_handle_and_keeps_the_shape() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let line = s.activate_tangent_handle(c, pts[2]).unwrap();
    let tip = tip_of(&s, c, pts[2]);
    s.set_point_xy(tip, 28.0, 4.0).unwrap();
    let before = dense(&s, c, 20);
    let offset = DVec2::new(7.0, -3.0);
    let set = s.transform_set(&[c], &[]);
    assert!(set.curves.contains(&line), "die Griff-Linie geht mit");
    assert!(set.points.contains(&tip));
    let mv = Similarity {
        base: DVec2::ZERO,
        offset,
        angle: 0.0,
        factor: 1.0,
    };
    s.apply_transform(&set, &mv).unwrap();
    for (a, b) in dense(&s, c, 20).iter().zip(&before) {
        assert!(a.distance(*b + offset) < 1e-9);
    }
    // Wer nur die Griff-Linie verschiebt, nimmt den Spline mit: Sie bleibt sein Griff.
    let set = s.transform_set(&[line], &[]);
    assert!(set.curves.contains(&c));
    s.apply_transform(&set, &mv).unwrap();
    let k = s.curve(line).unwrap().kind.clone();
    assert_eq!(
        k,
        CurveKind::Line {
            a: pts[2],
            b: tip_of(&s, c, pts[2])
        }
    );
    assert_eq!(handle_count(&s, c), 1);
}

#[test]
fn commands_not_yet_built_for_handles_refuse_and_change_nothing() {
    // Skizzen 7: Task 8 lässt Griffe durch diese Befehle laufen; bis dahin lehnen sie ab.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let line = s.activate_tangent_handle(c, pts[2]).unwrap();
    let cutter_a = s.add_point(25.0, -40.0).unwrap();
    let cutter_b = s.add_point(25.0, 40.0).unwrap();
    let cutter = s.add_line(cutter_a, cutter_b).unwrap();
    let axis_a = s.add_point(0.0, -60.0).unwrap();
    let axis_b = s.add_point(60.0, -60.0).unwrap();
    let axis = s.add_line(axis_a, axis_b).unwrap();
    let curves = s.curves().count();
    let at = s.curve_eval(c, 2.9).unwrap();
    assert!(s.trim(c, at).is_err());
    assert!(s.break_curve(c, at).is_err());
    assert!(s.add_mirrored(&[c], axis).is_err());
    assert!(s.add_mirrored(&[line], axis).is_err());
    assert!(
        s.add_linear_pattern(&[c], DVec2::new(0.0, 100.0), 2)
            .is_err()
    );
    let set = s.transform_set(&[c], &[]);
    let mv = Similarity {
        base: DVec2::ZERO,
        offset: DVec2::new(0.0, 100.0),
        angle: 0.0,
        factor: 1.0,
    };
    assert!(s.copy_transformed(&set, &mv).is_err());
    assert_eq!(s.curves().count(), curves);
    assert_eq!(handle_count(&s, c), 1);
    // Griff-Linie: bleibt Konstruktion, wird nicht geteilt.
    assert!(s.set_construction(line, false).is_err());
    assert!(s.curve(line).unwrap().construction);
    assert!(s.set_centerline(line, true).is_err());
    let (a, b) = (
        s.point_xy(pts[2]).unwrap(),
        s.point_xy(tip_of(&s, c, pts[2])).unwrap(),
    );
    let mid = (a + b) * 0.5;
    let across = (b - a).perp();
    let u = s.add_point(mid.x - across.x, mid.y - across.y).unwrap();
    let w = s.add_point(mid.x + across.x, mid.y + across.y).unwrap();
    s.add_line(u, w).unwrap();
    assert!(s.break_curve(line, mid).is_err());
    let _ = cutter;
}

#[test]
fn chain_does_not_run_into_a_handle() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let line = s.activate_tangent_handle(c, pts[0]).unwrap();
    assert!(!s.connected_chain(c).contains(&line));
}

#[test]
fn from_data_rejects_handle_not_on_a_fit_point() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let free = s.add_point(90.0, 90.0).unwrap();
    let far = s.add_point(95.0, 90.0).unwrap();
    let other = s.add_line(free, far).unwrap();
    let line = s.activate_tangent_handle(c, pts[2]).unwrap();
    s.activate_tangent_handle(c, pts[3]).unwrap();
    let tip = tip_of(&s, c, pts[2]);
    // Unverändert lädt es, und die Griffe bleiben.
    let back = Sketch::from_data(s.to_data()).unwrap();
    assert_eq!(back.curve(c).unwrap().kind, s.curve(c).unwrap().kind);

    let bad = |change: &dyn Fn(&mut planora_sketch::model::SplineHandle)| {
        let mut d = s.to_data();
        let CurveKind::Spline { handles, .. } = &mut d.curves[c.index()].kind else {
            panic!();
        };
        change(&mut handles[0]);
        Sketch::from_data(d).map(|_| ()).unwrap_err()
    };
    let invalid = |e: SketchError| assert!(matches!(e, SketchError::InvalidInput(_)), "{e:?}");
    invalid(bad(&|h| h.at = free));
    invalid(bad(&|h| h.at = tip));
    invalid(bad(&|h| h.at = pts[3])); // doppelt, und die Linie passt nicht
    invalid(bad(&|h| h.at = PointId::from_index(100_000)));
    invalid(bad(&|h| h.tip = PointId::from_index(100_000)));
    invalid(bad(&|h| h.tip = pts[1])); // ein Stützpunkt ist keine Spitze
    invalid(bad(&|h| h.line = other)); // Linie läuft nicht von at nach tip
    invalid(bad(&|h| h.line = c)); // keine Linie
    invalid(bad(&|h| h.line = CurveId::from_index(100_000)));
    invalid(bad(&|h| {
        h.curvature = HandleCurvature::Arc {
            arc: other,
            center: free,
            other: far,
        }
    })); // keine Bogenkurve
    // Griff-Linie nicht Konstruktion
    let mut d = s.to_data();
    d.curves[line.index()].construction = false;
    invalid(Sketch::from_data(d).map(|_| ()).unwrap_err());
    // Griff-Linie gelöscht
    let mut d = s.to_data();
    d.curves[line.index()].alive = false;
    invalid(Sketch::from_data(d).map(|_| ()).unwrap_err());
    // Zwei Griffe an einem Stützpunkt
    let mut d = s.to_data();
    let CurveKind::Spline { handles, .. } = &mut d.curves[c.index()].kind else {
        panic!();
    };
    let first = handles[0];
    handles.push(first);
    invalid(Sketch::from_data(d).map(|_| ()).unwrap_err());
}
