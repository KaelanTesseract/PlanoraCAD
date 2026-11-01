//! Schnittpunkte der Skizzenkurven über `split_params`, `curve_eval` und `curve_domain`.

use planora_math::{DVec2, Frame, TAU, Tolerance};
use planora_sketch::model::{CurveId, Sketch};
use std::f64::consts::FRAC_PI_2;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let a = s.add_point(a.0, a.1).unwrap();
    let b = s.add_point(b.0, b.1).unwrap();
    s.add_line(a, b).unwrap()
}

fn circle(s: &mut Sketch, c: (f64, f64), r: f64) -> CurveId {
    let c = s.add_point(c.0, c.1).unwrap();
    s.add_circle(c, r).unwrap()
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn at(s: &Sketch, c: CurveId, t: f64) -> DVec2 {
    s.curve_eval(c, t).unwrap()
}

#[test]
fn two_crossing_lines_split_each_other_in_the_middle() {
    let mut s = sketch();
    let a = line(&mut s, (0.0, 0.0), (10.0, 10.0));
    let b = line(&mut s, (0.0, 10.0), (10.0, 0.0));
    for c in [a, b] {
        let p = s.split_params(c, false);
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(close(p[0], 0.5, 1e-12), "{p:?}");
    }
    assert_eq!(s.curve_domain(a), Some((0.0, 1.0)));
    assert!(at(&s, a, 0.5).distance(DVec2::new(5.0, 5.0)) <= 1e-12);
}

#[test]
fn a_line_tangent_to_a_circle_gives_exactly_one_parameter() {
    let mut s = sketch();
    let c = circle(&mut s, (0.0, 0.0), 5.0);
    let l = line(&mut s, (-10.0, 5.0), (10.0, 5.0));
    let pl = s.split_params(l, false);
    assert_eq!(pl.len(), 1, "{pl:?}");
    assert!(close(pl[0], 0.5, 1e-9), "{pl:?}");
    let pc = s.split_params(c, false);
    assert_eq!(pc.len(), 1, "{pc:?}");
    assert!(close(pc[0], FRAC_PI_2, 1e-6), "{pc:?}");
    assert_eq!(s.curve_domain(c), Some((0.0, TAU)));
}

#[test]
fn two_circles_cross_twice() {
    let mut s = sketch();
    let a = circle(&mut s, (0.0, 0.0), 5.0);
    let b = circle(&mut s, (6.0, 0.0), 5.0);
    for (c, other) in [(a, DVec2::new(6.0, 0.0)), (b, DVec2::ZERO)] {
        let p = s.split_params(c, false);
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p[0] < p[1]);
        for t in p {
            let q = at(&s, c, t);
            assert!(close(q.distance(other), 5.0, 1e-9), "{q}");
            assert!(close(q.x, 3.0, 1e-9), "{q}");
        }
    }
}

#[test]
fn an_arc_is_only_cut_inside_its_angle() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(5.0, 0.0).unwrap();
    let b = s.add_point(0.0, 5.0).unwrap();
    let arc = s.add_arc(m, a, b).unwrap();
    let l = line(&mut s, (-10.0, 1.0), (10.0, 1.0));
    let p = s.split_params(arc, false);
    assert_eq!(p.len(), 1, "{p:?}");
    let q = at(&s, arc, p[0]);
    assert!(q.distance(DVec2::new(24f64.sqrt(), 1.0)) <= 1e-9, "{q}");
    let pl = s.split_params(l, false);
    assert_eq!(pl.len(), 1, "{pl:?}");
    assert!(at(&s, l, pl[0]).distance(q) <= 1e-9);
}

/// Restfehler der impliziten Ellipsengleichung `(x/10)² + (y/5)² − 1`.
fn ellipse_residual(p: DVec2) -> f64 {
    (p.x / 10.0).powi(2) + (p.y / 5.0).powi(2) - 1.0
}

#[test]
fn a_line_crosses_an_ellipse_twice() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(m, DVec2::new(10.0, 0.0), 5.0).unwrap();
    let l = line(&mut s, (6.0, -10.0), (6.0, 10.0));
    let pe = s.split_params(e, false);
    let pl = s.split_params(l, false);
    assert_eq!(pe.len(), 2, "{pe:?}");
    assert_eq!(pl.len(), 2, "{pl:?}");
    for (c, ts) in [(e, &pe), (l, &pl)] {
        for &t in ts {
            let q = at(&s, c, t);
            assert!(ellipse_residual(q).abs() <= 1e-6, "{q}");
            assert!((q.x - 6.0).abs() <= 1e-6, "{q}");
            assert!(close(q.y.abs(), 4.0, 1e-6), "{q}");
        }
    }
}

#[test]
fn an_elliptic_arc_is_only_cut_inside_its_range() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(10.0, 0.0).unwrap();
    let b = s.add_point(-10.0, 0.0).unwrap();
    // obere Hälfte
    let e = s
        .add_elliptic_arc(m, DVec2::new(10.0, 0.0), 5.0, a, b)
        .unwrap();
    let _l = line(&mut s, (6.0, -10.0), (6.0, 10.0));
    let p = s.split_params(e, false);
    assert_eq!(p.len(), 1, "{p:?}");
    let q = at(&s, e, p[0]);
    assert!(q.distance(DVec2::new(6.0, 4.0)) <= 1e-6, "{q}");
}

#[test]
fn a_looping_spline_has_one_self_intersection() {
    let mut s = sketch();
    let pts: Vec<_> = [
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 10.0),
        (5.0, 10.0),
        (5.0, -5.0),
    ]
    .iter()
    .map(|&(x, y)| s.add_point(x, y).unwrap())
    .collect();
    let sp = s.add_spline(&pts).unwrap();
    assert_eq!(s.curve_domain(sp), Some((0.0, 4.0)));
    let p = s.split_params(sp, false);
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(p[1] - p[0] > 1.0, "{p:?}");
    assert!(at(&s, sp, p[0]).distance(at(&s, sp, p[1])) <= 1e-6);
}

#[test]
fn a_t_junction_splits_only_the_through_line() {
    let mut s = sketch();
    let through = line(&mut s, (0.0, 0.0), (10.0, 0.0));
    let stem = line(&mut s, (5.0, 5.0), (5.0, 0.0));
    let p = s.split_params(through, false);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(close(p[0], 0.5, 1e-12));
    assert!(s.split_params(stem, false).is_empty());
}

#[test]
fn collinear_overlapping_lines_split_at_the_ends_of_the_overlap() {
    let mut s = sketch();
    let a = line(&mut s, (0.0, 0.0), (10.0, 0.0));
    let b = line(&mut s, (5.0, 0.0), (15.0, 0.0));
    for c in [a, b] {
        let p = s.split_params(c, false);
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(close(p[0], 0.5, 1e-12), "{p:?}");
    }
}

#[test]
fn a_construction_line_counts_only_on_request() {
    let mut s = sketch();
    let a = line(&mut s, (0.0, 0.0), (10.0, 0.0));
    let b = line(&mut s, (5.0, -5.0), (5.0, 5.0));
    s.set_construction(b, true).unwrap();
    assert!(s.split_params(a, false).is_empty());
    let p = s.split_params(a, true);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(close(p[0], 0.5, 1e-12));
}

#[test]
fn tiny_and_collapsed_curves_give_no_nan_and_no_panic() {
    let tol = Tolerance::default().linear;
    let mut s = sketch();
    let tiny = line(&mut s, (5.0, -0.6 * tol), (5.0, 0.9 * tol));
    let _base = line(&mut s, (0.0, 0.0), (10.0, 0.0));
    let _small_circle = circle(&mut s, (5.0, 0.0), 1.5 * tol);
    let m = s.add_point(5.0, 0.0).unwrap();
    let _e = s
        .add_ellipse(m, DVec2::new(2.0 * tol, 0.0), 1.5 * tol)
        .unwrap();
    let pts: Vec<_> = [(5.0, 0.0), (5.0 + 2.0 * tol, tol), (5.0 + 4.0 * tol, 0.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let _sp = s.add_spline(&pts).unwrap();
    // Linie nachträglich auf einen Punkt zusammengezogen
    let collapsed = line(&mut s, (4.0, -1.0), (4.0, 1.0));
    let ends = s.curve(collapsed).unwrap().kind.points();
    s.set_point_xy(ends[1], 4.0, -1.0).unwrap();
    let _cross = line(&mut s, (4.0, -2.0), (4.0, 2.0));
    let _ = tiny;
    let ids: Vec<CurveId> = s.curves().map(|(id, _)| id).collect();
    for &c in &ids {
        let (lo, hi) = s.curve_domain(c).unwrap();
        assert!(lo.is_finite() && hi.is_finite());
        for t in s.split_params(c, true) {
            assert!(t.is_finite(), "{c:?}: {t}");
            assert!((lo..=hi).contains(&t), "{c:?}: {t} not in {lo}..{hi}");
            assert!(at(&s, c, t).is_finite());
        }
    }
    assert!(s.curve_eval(ids[0], f64::NAN).is_none());
}

#[test]
fn a_line_tangent_to_an_ellipse_touches_once_and_a_near_miss_not_at_all() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(m, DVec2::new(10.0, 0.0), 5.0).unwrap();
    let touch = line(&mut s, (-3.0, 5.0), (7.0, 5.0));
    let pl = s.split_params(touch, false);
    assert_eq!(pl.len(), 1, "{pl:?}");
    assert!(at(&s, touch, pl[0]).distance(DVec2::new(0.0, 5.0)) <= 1e-6);
    let pe = s.split_params(e, false);
    assert_eq!(pe.len(), 1, "{pe:?}");
    assert!(close(pe[0], FRAC_PI_2, 1e-3), "{pe:?}");

    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(m, DVec2::new(10.0, 0.0), 5.0).unwrap();
    let _miss = line(&mut s, (-3.0, 5.001), (7.0, 5.001));
    assert!(s.split_params(e, false).is_empty());
}

#[test]
fn a_line_ending_on_an_ellipse_splits_only_the_ellipse() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(m, DVec2::new(10.0, 0.0), 5.0).unwrap();
    let stem = line(&mut s, (20.0, 0.0), (10.0, 0.0));
    let pe = s.split_params(e, false);
    assert_eq!(pe.len(), 1, "{pe:?}");
    assert!(pe[0].abs() <= 1e-6 || (pe[0] - TAU).abs() <= 1e-6, "{pe:?}");
    assert!(s.split_params(stem, false).is_empty());
}

#[test]
fn internally_tangent_circles_touch_once() {
    let mut s = sketch();
    let a = circle(&mut s, (0.0, 0.0), 5.0);
    let b = circle(&mut s, (2.0, 0.0), 3.0);
    for c in [a, b] {
        let p = s.split_params(c, false);
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(at(&s, c, p[0]).distance(DVec2::new(5.0, 0.0)) <= 1e-9);
    }
}

#[test]
fn a_spline_crosses_a_line_and_an_ellipse() {
    let mut s = sketch();
    let pts: Vec<_> = [(-20.0, -1.0), (0.0, 3.0), (20.0, -1.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let sp = s.add_spline(&pts).unwrap();
    let _l = line(&mut s, (0.0, -10.0), (0.0, 10.0));
    let m = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(m, DVec2::new(10.0, 0.0), 5.0).unwrap();
    let p = s.split_params(sp, false);
    // eine Kreuzung mit der Linie, zwei mit der Ellipse
    assert_eq!(p.len(), 3, "{p:?}");
    let pe = s.split_params(e, false);
    // zwei mit dem Spline, zwei mit der Linie
    assert_eq!(pe.len(), 4, "{pe:?}");
    for t in pe {
        assert!(ellipse_residual(at(&s, e, t)).abs() <= 1e-9);
    }
}

#[test]
fn an_arc_on_a_circle_splits_the_circle_at_its_ends() {
    let mut s = sketch();
    let c = circle(&mut s, (0.0, 0.0), 5.0);
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(5.0, 0.0).unwrap();
    let b = s.add_point(0.0, 5.0).unwrap();
    let arc = s.add_arc(m, a, b).unwrap();
    let p = s.split_params(c, false);
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(
        close(p[0], 0.0, 1e-12) && close(p[1], FRAC_PI_2, 1e-12),
        "{p:?}"
    );
    assert!(s.split_params(arc, false).is_empty());
}
