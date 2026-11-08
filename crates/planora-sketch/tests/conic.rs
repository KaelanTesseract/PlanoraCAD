//! Kegelschnitt (Skizzen 7, Task 5): rationale quadratische Bézierkurve mit Rho, Stutzen, Dehnen,
//! Spiegeln, Abrunden, Versatz, Profile, Abhängigkeiten und Laden.

use planora_kernel::Segment;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::fillet::{CurvePick, EndMove};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::pattern::{CircularMode, PatternKind, PatternPlan};
use planora_sketch::snap::{Snap, SnapIndex};
use planora_sketch::solver::SolveStatus;
use planora_sketch::{SketchError, conic_nurbs};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

const P0: (f64, f64) = (0.0, 0.0);
const APEX: (f64, f64) = (5.0, 8.0);
const P2: (f64, f64) = (10.0, 0.0);

fn conic_at(
    s: &mut Sketch,
    p0: (f64, f64),
    a: (f64, f64),
    p2: (f64, f64),
    rho: f64,
) -> (CurveId, [PointId; 3]) {
    let start = s.add_point(p0.0, p0.1).unwrap();
    let apex = s.add_point(a.0, a.1).unwrap();
    let end = s.add_point(p2.0, p2.1).unwrap();
    let c = s.add_conic(start, end, apex, rho).unwrap();
    (c, [start, apex, end])
}

fn conic(s: &mut Sketch, rho: f64) -> (CurveId, [PointId; 3]) {
    conic_at(s, P0, APEX, P2, rho)
}

fn line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let p = s.add_point(a.0, a.1).unwrap();
    let q = s.add_point(b.0, b.1).unwrap();
    s.add_line(p, q).unwrap()
}

/// Die Teile des Kegelschnitts `c`: `(start, end, apex, rho)`.
fn parts(s: &Sketch, c: CurveId) -> (PointId, PointId, PointId, f64) {
    match s.curve(c).unwrap().kind {
        CurveKind::Conic {
            start,
            end,
            apex,
            rho,
        } => (start, end, apex, rho),
        ref k => panic!("falsche Art: {k:?}"),
    }
}

/// Jeder Punkt von `c` liegt höchstens `tol` neben der Kurve `old` einer anderen Skizze.
fn assert_lies_on(s: &Sketch, c: CurveId, old: &Sketch, old_c: CurveId, tol: f64) {
    assert_lies_on_at(s, c, old, old_c, tol, 2000);
}

fn assert_lies_on_at(s: &Sketch, c: CurveId, old: &Sketch, old_c: CurveId, tol: f64, n: usize) {
    let (lo, hi) = s.curve_domain(c).unwrap();
    assert!(hi > lo);
    for i in 0..=n {
        let t = lo + (hi - lo) * i as f64 / n as f64;
        let q = s.curve_eval(c, t).unwrap();
        let d = old.nearest_on_curve(old_c, q).unwrap().distance(q);
        assert!(d < tol, "t = {t}: {q} liegt {d} neben der alten Kurve");
    }
}

// ---- Form -----------------------------------------------------------------------------------

#[test]
fn shoulder_point_is_at_rho() {
    let (a, b, m) = (v(0.0, 0.0), v(10.0, 0.0), v(5.0, 0.0));
    let apex = v(4.0, 7.0);
    for i in 1..100 {
        let rho = i as f64 / 100.0;
        let want = m + (apex - m) * rho;
        let n = conic_nurbs(a, apex, b, rho).unwrap();
        assert_eq!(n.degree(), 2);
        assert_eq!(n.domain(), (0.0, 1.0));
        assert!(n.point(0.5).truncate().distance(want) < 1e-12, "{rho}");
        let mut s = sketch();
        let (c, _) = conic_at(&mut s, (0.0, 0.0), (4.0, 7.0), (10.0, 0.0), rho);
        assert_eq!(s.curve_domain(c), Some((0.0, 1.0)));
        assert!(
            s.curve_eval(c, 0.5).unwrap().distance(want) < 1e-12,
            "{rho}"
        );
        assert!(s.curve_eval(c, 0.0).unwrap().distance(a) < 1e-12);
        assert!(s.curve_eval(c, 1.0).unwrap().distance(b) < 1e-12);
    }
}

#[test]
fn end_tangents_point_at_apex() {
    let (a, apex, b) = (v(1.0, -2.0), v(4.0, 7.0), v(10.0, 3.0));
    for rho in [0.01, 0.2, 0.5, 0.8, 0.99] {
        let n = conic_nurbs(a, apex, b, rho).unwrap();
        let (d0, d1) = (
            n.derivs(0.0, 1)[1].truncate().normalize(),
            n.derivs(1.0, 1)[1].truncate().normalize(),
        );
        let (u0, u1) = ((apex - a).normalize(), (b - apex).normalize());
        assert!(d0.distance(u0) < 1e-12, "{rho}");
        assert!(d1.distance(u1) < 1e-12, "{rho}");
    }
}

#[test]
fn rho_half_is_a_parabola() {
    let (a, apex, b) = (v(1.0, -2.0), v(4.0, 7.0), v(10.0, 3.0));
    let mut s = sketch();
    let (c, _) = conic_at(&mut s, (a.x, a.y), (apex.x, apex.y), (b.x, b.y), 0.5);
    // C − P0 = α·f + β·e mit f = 2(A − P0), e = P2 − 2A + P0: Auf einer Parabel ist β = α².
    let (f, e) = ((apex - a) * 2.0, b - apex * 2.0 + a);
    let det = f.perp_dot(e);
    for i in 0..=1000 {
        let q = s.curve_eval(c, i as f64 / 1000.0).unwrap() - a;
        let (alpha, beta) = (q.perp_dot(e) / det, f.perp_dot(q) / det);
        assert!((beta - alpha * alpha).abs() < 1e-11, "{i}: {alpha} {beta}");
    }
}

#[test]
fn rho_range_is_refused() {
    let bad = [0.0, 1.0, -0.1, 1.5, f64::NAN, f64::INFINITY, 1e-12];
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let m = s.add_point(5.0, 8.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    for rho in bad {
        assert!(
            matches!(s.add_conic(a, b, m, rho), Err(SketchError::InvalidInput(_))),
            "{rho}"
        );
    }
    assert_eq!(s.curves().count(), 0);
    let c = s.add_conic(a, b, m, 0.3).unwrap();
    for rho in bad {
        assert!(
            matches!(s.set_conic_rho(c, rho), Err(SketchError::InvalidInput(_))),
            "{rho}"
        );
        assert_eq!(parts(&s, c).3, 0.3);
    }
    s.set_conic_rho(c, 0.7).unwrap();
    assert_eq!(parts(&s, c).3, 0.7);
    let want = v(5.0, 8.0 * 0.7);
    assert!(s.curve_eval(c, 0.5).unwrap().distance(want) < 1e-12);
    // Andere Kurven haben kein Rho.
    let l = s.add_line(a, b).unwrap();
    assert!(matches!(
        s.set_conic_rho(l, 0.5),
        Err(SketchError::WrongType(_))
    ));
}

#[test]
fn apex_on_chord_is_refused() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let on = s.add_point(5.0, 0.0).unwrap();
    let beyond = s.add_point(15.0, 1e-9).unwrap();
    let near = s.add_point(0.0, 1e-9).unwrap();
    let apex = s.add_point(5.0, 8.0).unwrap();
    for (p, q, m) in [(a, b, on), (a, b, beyond), (a, near, apex)] {
        assert!(
            matches!(s.add_conic(p, q, m, 0.5), Err(SketchError::Degenerate(_))),
            "{p:?} {q:?} {m:?}"
        );
    }
    // Derselbe Punkt zweimal
    for (p, q, m) in [(a, a, apex), (a, b, a), (a, b, b)] {
        assert!(s.add_conic(p, q, m, 0.5).is_err());
    }
    assert_eq!(s.curves().count(), 0);
}

#[test]
fn conic_has_six_dof() {
    let mut s = sketch();
    let (c, [start, apex, end]) = conic(&mut s, 0.4);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert_eq!(rep.dof, 6);
    assert_eq!(s.internal_constraints().count(), 0);
    assert_eq!(s.curve(c).unwrap().kind.points().len(), 3);
    // Jeder Punkt zieht frei; das Rho bleibt.
    for (p, xy) in [
        (start, v(1.0, 1.0)),
        (apex, v(6.0, 9.0)),
        (end, v(12.0, 1.0)),
    ] {
        s.set_point_xy(p, xy.x, xy.y).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);
        assert!(s.point_xy(p).unwrap().distance(xy) < 1e-9);
    }
    assert_eq!(parts(&s, c).3, 0.4);
}

#[test]
fn collapsed_points_stay_finite_after_solving() {
    // Löser und Zeichnen bleiben stabil, wenn die Punkte später auf einer Geraden landen.
    let mut s = sketch();
    let (c, [_, apex, end]) = conic(&mut s, 0.9);
    s.set_point_xy(apex, 5.0, 0.0).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!(
        s.curve_polyline(c, 64)
            .unwrap()
            .iter()
            .all(|p| p.is_finite())
    );
    s.set_point_xy(end, 5.0, 0.0).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!(
        s.curve_polyline(c, 64)
            .unwrap()
            .iter()
            .all(|p| p.is_finite())
    );
    assert!(s.nearest_on_curve(c, v(1.0, 1.0)).unwrap().is_finite());
    // Ein Profil daraus nennt höchstens einen Grund und stürzt nicht ab.
    let (start, ..) = parts(&s, c);
    s.set_point_xy(start, 5.0, 0.0).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let found = s.profiles();
    assert!(found.profiles.is_empty());
    assert_eq!(found.rejected.len(), 1, "{:?}", found.rejected);
}

#[test]
fn conic_with_start_on_end_names_a_reason() {
    let mut s = sketch();
    let (c, [start, _, end]) = conic(&mut s, 0.5);
    s.set_point_xy(end, 0.0, 0.0).unwrap();
    assert_eq!(s.point_xy(start), s.point_xy(end));
    for found in [s.profiles(), s.legacy_profiles()] {
        assert!(found.profiles.is_empty());
        assert_eq!(found.rejected.len(), 1, "{:?}", found.rejected);
        assert_eq!(found.rejected[0].curves, vec![c]);
        assert_eq!(found.rejected[0].reason, SketchError::SelfIntersecting);
    }
}

// ---- Stutzen und Brechen ----------------------------------------------------------------------

#[test]
fn trim_stays_a_conic_with_same_points() {
    for rho in [0.05, 0.2, 0.5, 0.8, 0.95] {
        // Das Ende rechts der Linie fällt weg.
        let mut s = sketch();
        let (c, [start, apex, end]) = conic(&mut s, rho);
        let old = s.clone();
        line(&mut s, (7.0, -20.0), (7.0, 20.0));
        let click = old.nearest_on_curve(c, v(9.0, 1.0)).unwrap();
        assert_eq!(s.trim(c, click).unwrap(), vec![c], "{rho}");
        let (s2, e2, a2, r2) = parts(&s, c);
        assert_eq!(s2, start);
        assert_ne!(e2, end);
        assert_ne!(a2, apex);
        assert!(s.point_xy(end).is_err(), "altes Ende entfällt");
        assert!(s.point_xy(apex).is_err(), "alter Scheitel entfällt");
        assert!(r2 > 0.0 && r2 < 1.0);
        assert!((s.point_xy(e2).unwrap().x - 7.0).abs() < 1e-9);
        assert_lies_on(&s, c, &old, c, 1e-9);
        // Die neue Tangente zeigt weiter auf den neuen Scheitel: wieder ein Kegelschnitt desselben
        // Bereichs, also auch nach dem Laden gleich.
        let back = Sketch::from_data(s.to_data()).unwrap();
        assert_eq!(back.curve(c).unwrap().kind, s.curve(c).unwrap().kind);
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    }
}

#[test]
fn trim_of_the_middle_gives_two_exact_pieces() {
    let mut s = sketch();
    let (c, [start, _, end]) = conic(&mut s, 0.7);
    let old = s.clone();
    line(&mut s, (3.0, -20.0), (3.0, 20.0));
    line(&mut s, (7.0, -20.0), (7.0, 20.0));
    let click = old.nearest_on_curve(c, v(5.0, 8.0)).unwrap();
    let out = s.trim(c, click).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(parts(&s, out[0]).0, start);
    assert_eq!(parts(&s, out[1]).1, end);
    assert_ne!(parts(&s, out[0]).2, parts(&s, out[1]).2);
    for &k in &out {
        assert_lies_on(&s, k, &old, c, 1e-9);
    }
    assert!(s.nearest_on_curve(out[0], click).unwrap().distance(click) > 0.5);
}

#[test]
fn break_gives_two_conics_joined() {
    let mut s = sketch();
    let (c, _) = conic(&mut s, 0.35);
    let old = s.clone();
    line(&mut s, (4.0, -20.0), (4.0, 20.0));
    let click = old.nearest_on_curve(c, v(2.0, 1.0)).unwrap();
    let out = s.break_curve(c, click).unwrap();
    assert_eq!(out.len(), 2);
    for &k in &out {
        assert_lies_on(&s, k, &old, c, 1e-9);
    }
    let (e0, s1) = (parts(&s, out[0]).1, parts(&s, out[1]).0);
    assert!(
        s.constraints()
            .any(|(_, d)| matches!(d.kind, K::Coincident(a, b) if (a, b) == (e0, s1)))
    );
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn trim_near_the_end_and_extreme_rho_is_stable() {
    let mut kept = 0;
    for rho in [0.001, 0.01, 0.5, 0.99, 0.999] {
        for x in [1e-4, 1e-3, 0.01, 0.1, 1.0, 5.0, 9.0, 9.99, 9.9999] {
            // Beide Seiten: das kleine und das große Stück bleiben.
            for keep_left in [true, false] {
                let mut s = sketch();
                let (c, _) = conic(&mut s, rho);
                let old = s.clone();
                line(&mut s, (x, -20.0), (x, 20.0));
                let hits = s.split_params(c, false);
                assert_eq!(hits.len(), 1, "rho {rho}, x {x}: {hits:?}");
                let cut = s.curve_eval(c, hits[0]).unwrap();
                assert!((cut.x - x).abs() < 1e-6, "rho {rho}, x {x}: {cut}");
                let t = if keep_left {
                    0.5 * (hits[0] + 1.0)
                } else {
                    0.5 * hits[0]
                };
                let click = s.curve_eval(c, t).unwrap();
                let before = s.to_data();
                match s.trim(c, click) {
                    Ok(out) => {
                        assert_eq!(out.len(), 1, "rho {rho}, x {x}");
                        let (_, _, apex, r) = parts(&s, c);
                        assert!(r > 0.0 && r < 1.0, "rho {rho}, x {x}: {r}");
                        assert!(s.point_xy(apex).unwrap().is_finite());
                        let keeps_left = s.curve_eval(c, 1.0).unwrap().x <= x + 1e-6;
                        assert_eq!(keeps_left, keep_left, "rho {rho}, x {x}");
                        assert_lies_on_at(&s, c, &old, c, 1e-7, 200);
                        kept += 1;
                        Sketch::from_data(s.to_data()).unwrap();
                        assert_eq!(s.solve().status, SolveStatus::Solved, "rho {rho}, x {x}");
                    }
                    // Eine Meldung ist erlaubt, dann bleibt alles, wie es war.
                    Err(_) => assert_eq!(
                        format!("{:?}", s.to_data()),
                        format!("{before:?}"),
                        "rho {rho}, x {x}"
                    ),
                }
            }
        }
    }
    // 5 Rho · 9 Stellen · 2 Seiten: Im Bereich geht das Stutzen immer, keine Meldung nötig.
    assert_eq!(kept, 90);
}

// ---- Dehnen -------------------------------------------------------------------------------------

#[test]
fn extend_conic_exact_and_refuses_negative_weight() {
    // Parabel, Ellipsenstück und Hyperbelstück bis zu einer Geraden hinter dem Ende.
    for rho in [0.5, 0.45, 0.7] {
        let mut s = sketch();
        let (c, [start, apex, end]) = conic(&mut s, rho);
        let old = s.clone();
        let limit = line(&mut s, (0.0, -1.0), (20.0, -1.0));
        assert!(s.extend(c, v(10.0, 0.0)).unwrap(), "{rho}");
        let (s2, e2, a2, r2) = parts(&s, c);
        assert_eq!((s2, e2, a2), (start, end, apex), "dieselben Punkte");
        assert!(r2 > 0.0 && r2 < 1.0);
        let p = s.point_xy(end).unwrap();
        assert!((p.y + 1.0).abs() < 1e-9, "{rho}: {p}");
        assert_eq!(s.point_xy(start).unwrap(), v(0.0, 0.0));
        // Das alte Stück liegt genau auf der neuen Kurve und umgekehrt.
        assert_lies_on(&old, c, &s, c, 1e-9);
        assert!(
            s.constraints()
                .any(|(_, d)| matches!(d.kind, K::PointOnCurve(q, k) if q == end && k == limit))
        );
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    }

    // Am Anfang ebenso.
    let mut s = sketch();
    let (c, [start, _, end]) = conic(&mut s, 0.5);
    let old = s.clone();
    line(&mut s, (-20.0, -1.0), (20.0, -1.0));
    assert!(s.extend(c, v(0.0, 0.0)).unwrap());
    assert!((s.point_xy(start).unwrap().y + 1.0).abs() < 1e-9);
    assert_eq!(s.point_xy(end).unwrap(), v(10.0, 0.0));
    assert_lies_on(&old, c, &s, c, 1e-9);

    // Flache Ellipse: Über t = 1 / (1 − w) hinaus würde das innere Gewicht negativ. Das Ziel bei
    // t = 2 wird mit Meldung abgelehnt, ein nahes Ziel geht.
    let mut s = sketch();
    let (c, _) = conic_at(&mut s, (0.0, 0.0), (5.0, 4.0), (10.0, 0.0), 0.2);
    line(&mut s, (0.0, -1.0), (20.0, -1.0));
    let before = format!("{:?}", s.to_data());
    assert!(matches!(
        s.extend(c, v(10.0, 0.0)),
        Err(SketchError::Degenerate(_))
    ));
    assert_eq!(format!("{:?}", s.to_data()), before, "nichts geändert");
    assert!(s.extend_preview(c, v(10.0, 0.0)).is_none());

    let mut s = sketch();
    let (c, _) = conic_at(&mut s, (0.0, 0.0), (5.0, 4.0), (10.0, 0.0), 0.2);
    let old = s.clone();
    line(&mut s, (0.0, -0.2), (20.0, -0.2));
    assert!(s.extend(c, v(10.0, 0.0)).unwrap());
    assert_lies_on(&old, c, &s, c, 1e-9);

    // Hyperbel: Hinter der Asymptote gibt es keine Fortsetzung, also kein Ziel und keine Panik.
    let mut s = sketch();
    let (c, _) = conic_at(&mut s, (0.0, 0.0), (5.0, 4.0), (10.0, 0.0), 0.8);
    line(&mut s, (-50.0, 1e6), (50.0, 1e6));
    assert!(!s.extend(c, v(10.0, 0.0)).unwrap());
}

#[test]
fn extend_leaves_a_shared_apex_alone() {
    let mut s = sketch();
    let (c, [_, apex, _]) = conic(&mut s, 0.5);
    let other = s.add_point(30.0, 30.0).unwrap();
    s.add_line(apex, other).unwrap();
    line(&mut s, (0.0, -1.0), (20.0, -1.0));
    let before = format!("{:?}", s.to_data());
    assert!(!s.extend(c, v(10.0, 0.0)).unwrap());
    assert_eq!(format!("{:?}", s.to_data()), before);

    // Auch wenn das Ziel zugleich hinter dem positiven Gewicht liegt: kein Ziel, keine Meldung.
    let mut s = sketch();
    let (c, [_, apex, _]) = conic_at(&mut s, (0.0, 0.0), (5.0, 4.0), (10.0, 0.0), 0.2);
    let other = s.add_point(30.0, 30.0).unwrap();
    s.add_line(apex, other).unwrap();
    line(&mut s, (0.0, -1.0), (20.0, -1.0));
    let before = format!("{:?}", s.to_data());
    assert_eq!(s.extend(c, v(10.0, 0.0)), Ok(false));
    assert_eq!(format!("{:?}", s.to_data()), before);
}

// ---- Spiegeln, Kopieren, Abrunden, Versatz, Fang --------------------------------------------------

#[test]
fn mirror_keeps_rho() {
    let mut s = sketch();
    let (c, [start, apex, end]) = conic(&mut s, 0.37);
    let axis = line(&mut s, (-5.0, -20.0), (-5.0, 20.0));
    for p in s.curve(axis).unwrap().kind.points() {
        let q = s.point_xy(p).unwrap();
        s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
    }
    let copy = s.add_mirrored(&[c], axis).unwrap()[0];
    let (ms, me, ma, r) = parts(&s, copy);
    assert_eq!(r, 0.37);
    let mirrored = |p: DVec2| v(-10.0 - p.x, p.y);
    for (p, q) in [(start, ms), (apex, ma), (end, me)] {
        assert!(mirrored(s.point_xy(p).unwrap()).distance(s.point_xy(q).unwrap()) < 1e-9);
        assert!(s.constraints().any(|(_, d)| {
            matches!(d.kind, K::Symmetric(x, y, k) if (x, y, k) == (p, q, axis))
        }));
    }
    for i in 0..=500 {
        let t = i as f64 / 500.0;
        let (q, m) = (s.curve_eval(c, t).unwrap(), s.curve_eval(copy, t).unwrap());
        assert!(mirrored(q).distance(m) < 1e-9, "{t}");
    }
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
}

#[test]
fn copy_and_linear_pattern_keep_rho() {
    let mut s = sketch();
    let (c, [start, ..]) = conic(&mut s, 0.62);
    let copies = s.add_linear_pattern(&[c], v(20.0, 0.0), 2).unwrap();
    let copy = copies[0][0];
    assert_eq!(parts(&s, copy).3, 0.62);
    assert!(
        (s.curve_eval(copy, 0.5).unwrap() - s.curve_eval(c, 0.5).unwrap()).distance(v(20.0, 0.0))
            < 1e-9
    );
    assert_ne!(parts(&s, copy).0, start);
}

#[test]
fn circular_pattern_keeps_rho_and_a_new_rho_dissolves_it() {
    let mut s = sketch();
    let (c, [_, apex, _]) = conic(&mut s, 0.3);
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
    let copy = s.pattern(id).unwrap().instances[0].curves[0];
    assert_eq!(parts(&s, copy).3, 0.3);
    s.set_point_xy(apex, 6.0, 9.0).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    // Das Rho ist keine Löservariable: Ein Exemplar kann ihm nicht folgen, das Muster löst sich auf.
    s.set_conic_rho(c, 0.6).unwrap();
    assert!(s.pattern(id).is_none());
    assert_eq!(parts(&s, copy).3, 0.3);
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn fillet_cuts_the_conic() {
    let mut s = sketch();
    let (c, [start, _, end]) = conic(&mut s, 0.5);
    let old = s.clone();
    let l = line(&mut s, (7.0, -20.0), (7.0, 20.0));
    let pick = |c: CurveId, x: f64, y: f64| CurvePick {
        curve: c,
        at: v(x, y),
    };
    let plan = s
        .fillet_plan(pick(l, 7.0, -10.0), pick(c, 2.0, 3.0), 0.5)
        .expect("Plan");
    assert!(!plan.exact);
    let EndMove::Cut { end: gone, .. } = plan.moves[1] else {
        panic!("{:?}", plan.moves);
    };
    assert_eq!(gone, end);
    let ids = s.add_fillet(&plan, None).unwrap();
    let (s2, e2, _, r2) = parts(&s, c);
    assert_eq!(s2, start);
    assert_eq!(e2, ids.touch[1]);
    assert!(r2 > 0.0 && r2 < 1.0);
    assert!(s.point_xy(end).is_err());
    assert_lies_on(&s, c, &old, c, 1e-9);
    Sketch::from_data(s.to_data()).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn offset_of_a_conic_is_a_spline_at_the_distance() {
    let mut s = sketch();
    let (c, _) = conic(&mut s, 0.4);
    let chains = s.ordered_chains(&[c]).unwrap();
    let plan = s.offset_plan(&chains, 0.5).unwrap();
    let n0 = s.curves().count();
    let ids = s.add_offset(&plan).unwrap();
    assert_eq!(s.curves().count(), n0 + ids.curves.len());
    let k = ids.curves[0];
    assert!(matches!(s.curve(k).unwrap().kind, CurveKind::Spline { .. }));
    for i in 0..=200 {
        let q = s
            .curve_eval(k, (s.curve_domain(k).unwrap().1) * i as f64 / 200.0)
            .unwrap();
        let d = s.nearest_on_curve(c, q).unwrap().distance(q);
        assert!((d - 0.5).abs() < 1e-2, "{i}: {d}");
    }
}

/// Größte Abweichung vom Abstand `d`, bezogen auf `|d|`: dichte Abtastung des Versatz-Splines gegen
/// den nächsten Punkt der Vorlage. `None`, wenn der Versatz abgelehnt wird (Spitze).
fn offset_error(p0: (f64, f64), a: (f64, f64), p2: (f64, f64), rho: f64, d: f64) -> Option<f64> {
    let mut s = sketch();
    let (c, _) = conic_at(&mut s, p0, a, p2, rho);
    let chains = s.ordered_chains(&[c]).unwrap();
    let plan = s.offset_plan(&chains, d).ok()?;
    let k = s.add_offset(&plan).unwrap().curves[0];
    assert!(matches!(s.curve(k).unwrap().kind, CurveKind::Spline { .. }));
    let hi = s.curve_domain(k).unwrap().1;
    let mut worst = 0.0_f64;
    for i in 0..=2000 {
        let q = s.curve_eval(k, hi * i as f64 / 2000.0).unwrap();
        worst = worst.max((s.nearest_on_curve(c, q).unwrap().distance(q) - d.abs()).abs());
    }
    Some(worst / d.abs())
}

const OFFSET_RHOS: [f64; 13] = [
    0.01, 0.05, 0.1, 0.2, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.95, 0.99, 0.999,
];

#[test]
fn offset_stays_at_the_distance_for_every_rho() {
    // Außen geht jedes Rho; 1 % des Abstands gilt überall.
    for rho in OFFSET_RHOS {
        let e = offset_error(P0, APEX, P2, rho, 0.5).expect("außen gibt es keine Spitze");
        assert!(e < 1e-2, "rho {rho}: {e}");
    }
    // Andere Formen und Abstände, auch innen, soweit dort keine Spitze entsteht.
    let mut inner = 0;
    for a in [(5.0, 8.0), (1.0, 8.0), (5.0, 1.0)] {
        for d in [0.5, 3.0, -0.5] {
            for rho in OFFSET_RHOS {
                if let Some(e) = offset_error(P0, a, P2, rho, d) {
                    assert!(e < 1e-2, "Scheitel {a:?}, Abstand {d}, rho {rho}: {e}");
                    inner += usize::from(d < 0.0);
                } else {
                    assert!(d < 0.0, "außen abgelehnt: {a:?}, {d}, {rho}");
                }
            }
        }
    }
    assert!(inner >= 5, "{inner}");
}

#[test]
fn snap_finds_curve_ends_and_apex() {
    let mut s = sketch();
    let (c, pts) = conic(&mut s, 0.5);
    let index = SnapIndex::build(&s);
    for p in pts {
        let q = s.point_xy(p).unwrap();
        let hit = index.find(q + v(0.05, 0.05), 0.5, &[]).unwrap();
        assert_eq!(hit.snap, Snap::Point(p));
    }
    let on = s.curve_eval(c, 0.3).unwrap();
    let hit = index.find(on + v(0.02, 0.02), 0.1, &[]).unwrap();
    assert_eq!(hit.snap, Snap::Curve(c));
}

#[test]
fn scaling_keeps_rho() {
    use planora_sketch::transform::Similarity;
    let mut s = sketch();
    let (c, _) = conic(&mut s, 0.77);
    let t = Similarity {
        base: DVec2::ZERO,
        offset: DVec2::ZERO,
        angle: 0.0,
        factor: 2.0,
    };
    let set = s.transform_set(&[c], &[]);
    assert!(s.apply_transform(&set, &t).is_ok());
    assert_eq!(parts(&s, c).3, 0.77);
    assert!(s.curve_eval(c, 1.0).unwrap().distance(v(20.0, 0.0)) < 1e-9);
    let n0 = s.curves().count();
    let set = s.transform_set(&[c], &[]);
    s.copy_transformed(&set, &t).unwrap();
    assert_eq!(s.curves().count(), n0 + 1);
    let k = s.curves().map(|(id, _)| id).last().unwrap();
    assert_eq!(parts(&s, k).3, 0.77);
}

// ---- Profile ----------------------------------------------------------------------------------

#[test]
fn profile_with_conic_is_exact() {
    for rho in [0.01, 0.3, 0.5, 0.8, 0.99] {
        let mut s = sketch();
        let (c, [start, _, end]) = conic(&mut s, rho);
        let l = s.add_line(end, start).unwrap();
        let n = s.nurbs_of(c).unwrap();
        for found in [s.profiles(), s.legacy_profiles()] {
            assert!(found.rejected.is_empty(), "{rho}: {:?}", found.rejected);
            assert_eq!(found.profiles.len(), 1, "{rho}");
            let segs: Vec<_> = found.profiles[0]
                .outer
                .segments
                .iter()
                .filter_map(|x| match x {
                    Segment::Nurbs { curve } => Some(curve.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(segs.len(), 1);
            assert_eq!(segs[0].degree(), 2);
            let forward = segs[0].point(0.0).truncate().distance(v(0.0, 0.0)) < 1e-12;
            for i in 0..=1000 {
                let u = i as f64 / 1000.0;
                let want = n.point(if forward { u } else { 1.0 - u }).truncate();
                assert!(segs[0].point(u).truncate().distance(want) < 1e-9);
            }
        }
        let found = s.profiles();
        assert!(found.curve_loops[0].contains(&c) && found.curve_loops[0].contains(&l));
        let piece = found.refs[0].outer.iter().find(|p| p.curve == c).unwrap();
        assert_eq!((piece.u0, piece.u1), (0.0, 1.0));
        let again = Sketch::from_data(s.to_data()).unwrap().profiles();
        assert_eq!(again.refs, found.refs);
    }
}

#[test]
fn profile_of_a_parabola_cut_by_a_line_has_the_exact_part() {
    // Eine Sehne quer durch die Parabel teilt sie; das Stück ist ein exakter Teil der Kurve.
    let mut s = sketch();
    let (c, _) = conic(&mut s, 0.5);
    line(&mut s, (-5.0, 2.0), (15.0, 2.0));
    let found = s.profiles();
    assert_eq!(found.profiles.len(), 1, "{:?}", found.rejected);
    let seg = found.profiles[0]
        .outer
        .segments
        .iter()
        .find_map(|x| match x {
            Segment::Nurbs { curve } => Some(curve.clone()),
            _ => None,
        })
        .unwrap();
    let (a, b) = seg.domain();
    for i in 0..=500 {
        let q = seg.point(a + (b - a) * i as f64 / 500.0).truncate();
        assert!(q.y >= 2.0 - 1e-9, "{q}");
        let d = s.nearest_on_curve(c, q).unwrap().distance(q);
        assert!(d < 1e-9, "{i}: {d}");
    }
}

// ---- Abhängigkeiten --------------------------------------------------------------------------

fn fix_all(s: &mut Sketch, pts: &[PointId]) {
    for &p in pts {
        let q = s.point_xy(p).unwrap();
        s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
    }
}

#[test]
fn point_on_conic_and_tangent_line_solve() {
    for rho in [0.1, 0.5, 0.9] {
        let mut s = sketch();
        let (c, pts) = conic(&mut s, rho);
        fix_all(&mut s, &pts);
        let q = s.add_point(3.0, 9.0).unwrap();
        s.add_constraint(K::PointOnCurve(q, c)).unwrap();
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
        assert_eq!(rep.dof, 1, "{rep:?}");
        let at = s.point_xy(q).unwrap();
        assert!(
            s.nearest_on_curve(c, at).unwrap().distance(at) < 1e-6,
            "{rho}"
        );

        // Rho ändern: Der Punkt folgt der Kurve, die Abhängigkeit bleibt.
        s.set_conic_rho(c, 1.0 - rho).unwrap();
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
        let at = s.point_xy(q).unwrap();
        assert!(
            s.nearest_on_curve(c, at).unwrap().distance(at) < 1e-6,
            "{rho}"
        );
        s.set_conic_rho(c, rho).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);

        // Eine waagerechte Tangente berührt die Kurve in der Schulter.
        let a = s.add_point(0.0, 9.0).unwrap();
        let b = s.add_point(14.0, 9.0).unwrap();
        let l = s.add_line(a, b).unwrap();
        s.add_constraint(K::Horizontal(planora_sketch::LineOrPoints::Line(l)))
            .unwrap();
        s.add_constraint(K::Tangent(l, c)).unwrap();
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
        let y = s.point_xy(a).unwrap().y;
        assert!((y - 8.0 * rho).abs() < 1e-6, "{rho}: {y}");
    }
}

#[test]
fn coincident_line_at_the_end_follows_and_smooth_is_not_there_yet() {
    let mut s = sketch();
    let (c, [_, _, end]) = conic(&mut s, 0.5);
    let far = s.add_point(20.0, 0.0).unwrap();
    let l = s.add_line(end, far).unwrap();
    // Glatt am Kegelschnitt kommt erst mit Task 9.
    assert!(
        s.add_constraint(K::Smooth(c, end, l, end)).is_err(),
        "Glatt verlangt noch einen Spline"
    );
    s.set_point_xy(end, 12.0, 1.0).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!(s.curve_eval(c, 1.0).unwrap().distance(v(12.0, 1.0)) < 1e-9);
}

// ---- Laden -----------------------------------------------------------------------------------

#[test]
fn from_data_rejects_bad_rho() {
    let mut s = sketch();
    let (c, _) = conic(&mut s, 0.4);
    let good = s.to_data();
    assert_eq!(
        Sketch::from_data(good.clone())
            .unwrap()
            .curve(c)
            .unwrap()
            .kind,
        s.curve(c).unwrap().kind
    );
    for rho in [0.0, 1.0, -0.1, 1.5, f64::NAN, f64::INFINITY, 1e-12] {
        let mut bad = good.clone();
        let CurveKind::Conic { rho: slot, .. } = &mut bad.curves[c.index()].kind else {
            panic!("falsche Art");
        };
        *slot = rho;
        assert!(
            matches!(Sketch::from_data(bad), Err(SketchError::InvalidInput(_))),
            "{rho}"
        );
    }
    // Auch ein gelöschter Eintrag wird geprüft, und ein Punkt muss vorhanden sein.
    let mut gone = s.clone();
    gone.remove_curve(c).unwrap();
    let mut bad = gone.to_data();
    let CurveKind::Conic { rho: slot, .. } = &mut bad.curves[c.index()].kind else {
        panic!("falsche Art");
    };
    *slot = 2.0;
    assert!(Sketch::from_data(bad).is_err());
    let mut bad = good.clone();
    let CurveKind::Conic { apex, .. } = &mut bad.curves[c.index()].kind else {
        panic!("falsche Art");
    };
    *apex = PointId::from_index(99);
    assert!(Sketch::from_data(bad).is_err());
    // Derselbe Punkt zweimal, wie bei `add_conic`.
    let CurveKind::Conic {
        start: s0,
        end: e0,
        apex: a0,
        ..
    } = good.curves[c.index()].kind
    else {
        panic!("falsche Art");
    };
    for (x, y, z) in [(s0, s0, a0), (s0, e0, s0), (s0, e0, e0)] {
        let mut bad = good.clone();
        let CurveKind::Conic {
            start, end, apex, ..
        } = &mut bad.curves[c.index()].kind
        else {
            panic!("falsche Art");
        };
        (*start, *end, *apex) = (x, y, z);
        assert!(
            matches!(Sketch::from_data(bad), Err(SketchError::InvalidInput(_))),
            "{x:?} {y:?} {z:?}"
        );
    }
}
