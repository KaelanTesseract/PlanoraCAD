//! Spline mit Kontrollpunkten (Skizzen 7, Task 2): Modell, Löser, Rohdaten.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::{ConstraintKind as K, CurveId, CurveKind, PointId, Sketch, SolveStatus};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "Skizze", Tolerance::default())
}

fn points(s: &mut Sketch, xy: &[(f64, f64)]) -> Vec<PointId> {
    xy.iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect()
}

fn fix_all(s: &mut Sketch, pts: &[PointId]) {
    for &p in pts {
        let q = s.point_xy(p).unwrap();
        s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
    }
}

/// Kleinster Abstand von `p` zur dicht abgetasteten Kurve.
fn distance_to(s: &Sketch, c: CurveId, p: DVec2) -> f64 {
    let (lo, hi) = s.curve_domain(c).unwrap();
    (0..=200_000)
        .map(|i| {
            let t = lo + (hi - lo) * i as f64 / 200_000.0;
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

#[test]
fn control_spline_ends_on_first_and_last_point() {
    for degree in [3, 5] {
        let mut s = sketch();
        let pts = points(&mut s, &ARCH);
        let c = s.add_control_spline(&pts, degree).unwrap();
        let (lo, hi) = s.curve_domain(c).unwrap();
        let (first, last) = (s.point_xy(pts[0]).unwrap(), s.point_xy(pts[5]).unwrap());
        assert!(s.curve_eval(c, lo).unwrap().distance(first) < 1e-12);
        assert!(s.curve_eval(c, hi).unwrap().distance(last) < 1e-12);
        // Innere Kontrollpunkte liegen im Allgemeinen nicht auf der Kurve.
        assert!(distance_to(&s, c, s.point_xy(pts[2]).unwrap()) > 1e-3);
        // Der Linienzug trifft die Enden genau und hat sie als erste und letzte Stelle.
        let pl = s.curve_polyline(c, 64).unwrap();
        assert_eq!(pl[0], first);
        assert_eq!(*pl.last().unwrap(), last);
        assert!(pl.len() > 16, "{}", pl.len());
    }
}

#[test]
fn degree_five_with_five_points_is_degree_four() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH[..5]);
    let c = s.add_control_spline(&pts, 5).unwrap();
    let CurveKind::ControlSpline {
        points: got,
        degree,
        knots,
    } = &s.curve(c).unwrap().kind
    else {
        panic!("falsche Art");
    };
    assert_eq!(got, &pts);
    assert_eq!(*degree, 4);
    assert_eq!(knots, &[0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
    let n = s.nurbs_of(c).unwrap();
    assert_eq!(n.degree(), 4);
    assert_eq!(n.control_points().len(), 5);
    assert!(n.weights().iter().all(|w| *w == 1.0));
}

#[test]
fn degree_three_with_two_points_is_a_line() {
    let mut s = sketch();
    let pts = points(&mut s, &[(1.0, 1.0), (5.0, 3.0)]);
    let c = s.add_control_spline(&pts, 3).unwrap();
    assert_eq!(s.nurbs_of(c).unwrap().degree(), 1);
    assert_eq!(s.curve_domain(c).unwrap(), (0.0, 1.0));
    let mid = s.curve_eval(c, 0.5).unwrap();
    assert!(mid.distance(DVec2::new(3.0, 2.0)) < 1e-12);
    for p in s.curve_polyline(c, 64).unwrap() {
        assert!(
            (p - DVec2::new(1.0, 1.0))
                .perp_dot(DVec2::new(2.0, 1.0))
                .abs()
                < 1e-12
        );
    }
    // Drei Punkte bei Grad 3: wirksam Grad 2.
    let q = points(&mut s, &[(0.0, 9.0), (1.0, 10.0), (2.0, 9.0)]);
    let c3 = s.add_control_spline(&q, 3).unwrap();
    assert_eq!(s.nurbs_of(c3).unwrap().degree(), 2);
}

#[test]
fn uniform_knots_for_more_points_and_points_of_the_kind() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let kind = &s.curve(c).unwrap().kind;
    assert_eq!(kind.points(), pts);
    let CurveKind::ControlSpline { degree, knots, .. } = kind else {
        panic!("falsche Art");
    };
    assert_eq!(*degree, 3);
    assert_eq!(knots, &[0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 3.0]);
    assert_eq!(s.curve_domain(c).unwrap(), (0.0, 3.0));
}

#[test]
fn creation_rejects_what_add_spline_rejects_and_other_degrees() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let doubled = s.add_point(0.0, 0.0).unwrap();
    let before = s.curves().count();
    assert!(s.add_control_spline(&pts[..1], 3).is_err());
    assert!(s.add_control_spline(&[pts[0], pts[0]], 3).is_err());
    assert!(s.add_control_spline(&[pts[0], doubled], 3).is_err());
    for bad in [0, 1, 2, 4, 6, 255] {
        assert!(s.add_control_spline(&pts, bad).is_err(), "{bad}");
    }
    let many: Vec<PointId> = (0..65)
        .map(|i| s.add_point(i as f64, 20.0).unwrap())
        .collect();
    assert!(s.add_control_spline(&many, 3).is_err());
    assert!(s.add_control_spline(&many[..64], 3).is_ok());
    assert_eq!(s.curves().count(), before + 1);
}

#[test]
fn nurbs_of_is_none_for_other_kinds() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let fit = s.add_spline(&pts).unwrap();
    let line = s.add_line(pts[0], pts[1]).unwrap();
    assert!(s.nurbs_of(fit).is_none());
    assert!(s.nurbs_of(line).is_none());
}

#[test]
fn eval_outside_continues_the_last_polynomial_piece() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let n = s.nurbs_of(c).unwrap();
    let (lo, hi) = s.curve_domain(c).unwrap();
    for t in [lo - 0.7, lo - 0.1, hi + 0.1, hi + 0.7] {
        let got = s.curve_eval(c, t).unwrap();
        let want = n.point_unclamped(t);
        assert!(got.distance(want.truncate()) < 1e-12, "{t}");
        // nicht auf das Ende begrenzt
        assert!(
            got.distance(s.curve_eval(c, t.clamp(lo, hi)).unwrap()) > 1e-3,
            "{t}"
        );
    }
}

#[test]
fn query_helpers_find_the_curve() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 5).unwrap();
    let on = s.curve_eval(c, 0.37).unwrap();
    assert!(
        s.nearest_on_curve(c, on + DVec2::new(0.0, 1e-3))
            .unwrap()
            .distance(on)
            < 0.01
    );
    assert!(s.distance_to_curve(c, on).unwrap() < 1e-2);
    assert_eq!(s.pick_curve(on, 0.05), Some(c));
    // Die Enden hängen an einer anschließenden Linie, der innere Kontrollpunkt nicht.
    let p = s.add_point(20.0, 20.0).unwrap();
    let joined = s.add_line(pts[5], p).unwrap();
    let chain = s.connected_chain(c);
    assert!(chain.contains(&joined), "{chain:?}");
    let q = s.add_point(30.0, 20.0).unwrap();
    let off = s.add_line(pts[3], q).unwrap();
    assert!(!s.connected_chain(c).contains(&off));
    let (lo, hi) = s.extent().unwrap();
    assert!(lo.x <= 0.0 && hi.x >= 14.0);
}

#[test]
fn dimensions_treat_control_points_like_points_and_not_the_curve() {
    use planora_sketch::DimPick;
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 5).unwrap();
    let at = DVec2::new(5.0, 5.0);
    assert!(s.dimension_plan(&[DimPick::Curve(c, at)], at).is_none());
    assert!(
        s.dimension_plan(&[DimPick::Point(pts[1]), DimPick::Point(pts[2])], at)
            .is_some()
    );
}

#[test]
fn point_on_control_spline_solves() {
    for degree in [3, 5] {
        let mut s = sketch();
        let pts = points(&mut s, &ARCH);
        let c = s.add_control_spline(&pts, degree).unwrap();
        fix_all(&mut s, &pts);
        let q = s.add_point(6.0, 7.5).unwrap();
        s.add_constraint(K::PointOnCurve(q, c)).unwrap();
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
        let pq = s.point_xy(q).unwrap();
        assert!(distance_to(&s, c, pq) < 1e-3, "Grad {degree}");
    }
}

#[test]
fn free_control_points_follow_a_point_on_the_curve() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let q = s.add_point(6.0, 5.5).unwrap();
    s.add_constraint(K::Fixed(q, 6.0, 5.5)).unwrap();
    s.add_constraint(K::PointOnCurve(q, c)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(distance_to(&s, c, DVec2::new(6.0, 5.5)) < 1e-3);
}

#[test]
fn tangent_line_to_control_spline_solves() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 5).unwrap();
    fix_all(&mut s, &pts);
    let a = s.add_point(0.0, 8.0).unwrap();
    let b = s.add_point(14.0, 8.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Tangent(l, c)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let (pa, pb) = (s.point_xy(a).unwrap(), s.point_xy(b).unwrap());
    let d = (pb - pa).normalize();
    // Der Berührpunkt: die Stelle mit dem kleinsten Abstand zur Geraden, dort parallel.
    let (lo, hi) = s.curve_domain(c).unwrap();
    let n = s.nurbs_of(c).unwrap();
    let (mut best, mut best_t) = (f64::INFINITY, lo);
    for i in 0..=200_000 {
        let t = lo + (hi - lo) * i as f64 / 200_000.0;
        let g = d.perp_dot(s.curve_eval(c, t).unwrap() - pa).abs();
        if g < best {
            (best, best_t) = (g, t);
        }
    }
    assert!(best < 1e-6, "Abstand {best}");
    let tangent = n.derivs(best_t, 1)[1].truncate().normalize();
    assert!(d.perp_dot(tangent).abs() < 1e-3, "{}", d.perp_dot(tangent));
}

#[test]
fn parameter_range_follows_the_knots() {
    // Bereich [10, 12] statt [0, n − p]: Der Parameter startet und bleibt darin, das Ende liegt
    // nicht bei n − 1.
    let mut s = sketch();
    let pts = points(&mut s, &ARCH[..5]);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let mut data = s.to_data();
    let CurveKind::ControlSpline { knots, .. } = &mut data.curves[c.index()].kind else {
        panic!("falsche Art");
    };
    *knots = vec![10.0, 10.0, 10.0, 10.0, 11.0, 12.0, 12.0, 12.0, 12.0];
    let mut s = Sketch::from_data(data).unwrap();
    assert_eq!(s.curve_domain(c).unwrap(), (10.0, 12.0));
    fix_all(&mut s, &pts);
    let q = s.add_point(5.0, 6.5).unwrap();
    s.add_constraint(K::PointOnCurve(q, c)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(distance_to(&s, c, s.point_xy(q).unwrap()) < 1e-3);
}

#[test]
fn collinear_and_doubled_control_points_stay_finite() {
    let finite = |s: &Sketch, c: CurveId| {
        let pl = s.curve_polyline(c, 64).expect("Linienzug");
        assert!(pl.iter().all(|p| p.is_finite()), "{pl:?}");
        let (lo, hi) = s.curve_domain(c).unwrap();
        for i in 0..=64 {
            let t = lo + (hi - lo) * i as f64 / 64.0;
            assert!(s.curve_eval(c, t).unwrap().is_finite());
        }
    };
    // Alle auf einer Geraden.
    let mut s = sketch();
    let pts = points(
        &mut s,
        &[
            (0.0, 0.0),
            (1.0, 0.0),
            (2.0, 0.0),
            (3.0, 0.0),
            (4.0, 0.0),
            (5.0, 0.0),
        ],
    );
    let c = s.add_control_spline(&pts, 5).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    finite(&s, c);

    // Ein innerer Punkt per Deckungsgleich auf den Nachbarn gezogen, dazu ein Punkt auf der Kurve.
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let q = s.add_point(6.0, 5.5).unwrap();
    s.add_constraint(K::PointOnCurve(q, c)).unwrap();
    s.add_constraint(K::Coincident(pts[2], pts[3])).unwrap();
    let rep = s.solve();
    assert!(s.variable_count() > 0);
    assert!(
        s.points().all(|(p, _)| s.point_xy(p).unwrap().is_finite()),
        "{rep:?}"
    );
    finite(&s, c);

    // Das erste Kontrollpolygon-Stück auf null gezogen: Die Ableitung am Anfang verschwindet.
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 5).unwrap();
    let a = s.add_point(0.0, 9.0).unwrap();
    let b = s.add_point(14.0, 9.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Tangent(l, c)).unwrap();
    s.set_point_xy(pts[1], 0.0, 0.0).unwrap();
    let rep = s.solve();
    assert!(
        s.points().all(|(p, _)| s.point_xy(p).unwrap().is_finite()),
        "{rep:?}"
    );
    finite(&s, c);

    // Alle Kontrollpunkte auf einer Stelle: Länge null, trotzdem endlich.
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    for &p in &pts {
        s.set_point_xy(p, 2.0, 2.0).unwrap();
    }
    s.solve();
    finite(&s, c);
}

#[test]
fn from_data_rejects_bad_knots() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 3).unwrap();
    let good = s.to_data();
    assert!(Sketch::from_data(good.clone()).is_ok());
    type Change = dyn Fn(&mut Vec<PointId>, &mut u8, &mut Vec<f64>);
    let with = |f: &Change| {
        let mut d = good.clone();
        let CurveKind::ControlSpline {
            points,
            degree,
            knots,
        } = &mut d.curves[c.index()].kind
        else {
            panic!("falsche Art");
        };
        f(points, degree, knots);
        Sketch::from_data(d)
    };
    // zu wenige, fallend, nicht geklemmt, nicht endlich
    assert!(
        with(&|_, _, k| {
            k.pop();
        })
        .is_err()
    );
    assert!(with(&|_, _, k| k.swap(4, 5)).is_err());
    assert!(with(&|_, _, k| k[0] = -1.0).is_err());
    assert!(with(&|_, _, k| k[4] = f64::NAN).is_err());
    assert!(with(&|_, _, k| k[5] = f64::INFINITY).is_err());
    // Grad passt nicht, Grad außerhalb, Grad null
    assert!(with(&|_, g, _| *g = 2).is_err());
    assert!(with(&|_, g, _| *g = 0).is_err());
    assert!(
        with(&|_, g, k| {
            *g = 6;
            *k = vec![0.0; 6];
            k.extend([1.0; 6]);
        })
        .is_err()
    );
    // Zahl der Punkte
    assert!(with(&|p, _, _| p.truncate(1)).is_err());
    // Punkt fehlt
    assert!(with(&|p, _, _| p[1] = PointId::from_index(9999)).is_err());
    // Auch ein gelöschter Eintrag wird geprüft.
    let mut d = good.clone();
    d.curves[c.index()].alive = false;
    assert!(Sketch::from_data(d.clone()).is_ok());
    let CurveKind::ControlSpline { knots, .. } = &mut d.curves[c.index()].kind else {
        panic!("falsche Art");
    };
    knots.pop();
    assert!(Sketch::from_data(d).is_err());
}

#[test]
fn data_roundtrip_keeps_shape_and_ids() {
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 5).unwrap();
    let back = Sketch::from_data(s.to_data()).unwrap();
    assert_eq!(back.to_data(), s.to_data());
    assert_eq!(back.curve(c).unwrap().kind, s.curve(c).unwrap().kind);
}

#[test]
fn other_commands_refuse_it_for_now() {
    // Vorläufig bis Task 3: klare Ablehnung statt Absturz; die Skizze bleibt, wie sie ist.
    let mut s = sketch();
    let pts = points(&mut s, &ARCH);
    let c = s.add_control_spline(&pts, 5).unwrap();
    let a = s.add_point(-5.0, -5.0).unwrap();
    let b = s.add_point(-5.0, 5.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    // Eine Linie, die die Kurve kreuzt: Sonst löscht Stutzen die ganze Kurve (wie bei jeder Art).
    let d = s.add_point(6.0, -2.0).unwrap();
    let e = s.add_point(6.0, 10.0).unwrap();
    s.add_line(d, e).unwrap();
    let before = s.to_data();
    assert!(s.add_mirrored(&[c], axis).is_err());
    assert_eq!(s.to_data(), before);
    let at = s.curve_eval(c, 0.5).unwrap();
    assert!(s.trim(c, at).is_err());
    assert!(s.break_curve(c, at).is_err());
    assert_eq!(s.to_data(), before);
    // Keine Profilkante; die Kurve wird benannt abgelehnt.
    let found = s.profiles();
    assert!(found.profiles.is_empty());
    assert!(found.rejected.iter().any(|r| r.curves == vec![c]));
}
