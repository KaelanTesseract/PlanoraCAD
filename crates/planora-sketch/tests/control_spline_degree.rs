//! Grad ändern und Kontrollpunkt einfügen bei Splines mit Kontrollpunkten (Skizzen 7, Task 4).

use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::error::SketchError;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use std::sync::Arc;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn spline(s: &mut Sketch, xy: &[(f64, f64)], degree: u8) -> (CurveId, Vec<PointId>) {
    let ids: Vec<PointId> = xy
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    (s.add_control_spline(&ids, degree).unwrap(), ids)
}

const WAVE: [(f64, f64); 7] = [
    (0.0, 0.0),
    (10.0, 12.0),
    (20.0, -8.0),
    (30.0, 10.0),
    (40.0, -6.0),
    (50.0, 9.0),
    (60.0, 0.0),
];

fn nurbs(s: &Sketch, c: CurveId) -> Arc<NurbsCurve> {
    s.nurbs_of(c).unwrap()
}

fn parts(s: &Sketch, c: CurveId) -> (Vec<PointId>, u8) {
    match &s.curve(c).unwrap().kind {
        CurveKind::ControlSpline { points, degree, .. } => (points.clone(), *degree),
        k => panic!("falsche Art: {k:?}"),
    }
}

fn max_gap(s: &Sketch, c: CurveId, old: &NurbsCurve) -> f64 {
    let (lo, hi) = s.curve_domain(c).unwrap();
    (0..=2000)
        .map(|i| {
            let t = lo + (hi - lo) * i as f64 / 2000.0;
            s.curve_eval(c, t)
                .unwrap()
                .distance(old.point_unclamped(t).truncate())
        })
        .fold(0.0, f64::max)
}

#[test]
fn raising_degree_keeps_the_shape() {
    let mut s = sketch();
    let (c, ids) = spline(&mut s, &WAVE, 3);
    let old = nurbs(&s, c);
    s.set_control_spline_degree(c, 5).unwrap();
    let (points, degree) = parts(&s, c);
    assert_eq!(degree, 5);
    assert!(points.len() > ids.len());
    assert_eq!(points[0], ids[0]);
    assert_eq!(points[points.len() - 1], ids[ids.len() - 1]);
    assert_eq!(&points[..3], &ids[..3]);
    assert!(max_gap(&s, c, &old) < 1e-6);
}

#[test]
fn lowering_degree_has_fewer_points_and_same_ends() {
    let mut s = sketch();
    let (c, ids) = spline(&mut s, &WAVE, 5);
    let before = s.points().count();
    let (a, b) = (s.point_xy(ids[0]).unwrap(), s.point_xy(ids[6]).unwrap());
    s.set_control_spline_degree(c, 3).unwrap();
    let (points, degree) = parts(&s, c);
    assert_eq!(degree, 3);
    assert_eq!(points.len(), 5);
    assert_eq!(&points[..4], &ids[..4]);
    assert_eq!(points[4], ids[6]);
    assert_eq!(s.point_xy(points[0]).unwrap(), a);
    assert_eq!(s.point_xy(points[4]).unwrap(), b);
    // Die überzähligen Punkte sind gelöscht.
    assert_eq!(s.points().count(), before - 2);
    assert!(s.curve_domain(c).is_some());
}

#[test]
fn insert_keeps_the_shape_and_old_ids() {
    let mut s = sketch();
    let (c, ids) = spline(&mut s, &WAVE, 3);
    let old = nurbs(&s, c);
    let (p, _) = s.insert_control_point(c, DVec2::new(25.0, 2.0)).unwrap();
    let (points, degree) = parts(&s, c);
    assert_eq!(degree, 3);
    assert_eq!(points.len(), ids.len() + 1);
    assert!(points.contains(&p));
    assert!(!ids.contains(&p));
    let rest: Vec<PointId> = points.iter().copied().filter(|q| *q != p).collect();
    assert_eq!(rest, ids);
    assert!(max_gap(&s, c, &old) < 1e-9);
}

#[test]
fn insert_drops_constraints_that_no_longer_hold() {
    let mut s = sketch();
    // Die beiden mittleren Kontrollpunkte liegen auf gleicher Höhe.
    let (c, ids) = spline(
        &mut s,
        &[
            (0.0, 0.0),
            (10.0, 8.0),
            (20.0, 8.0),
            (30.0, -4.0),
            (40.0, 0.0),
        ],
        3,
    );
    let flat = s
        .add_constraint(K::Horizontal(LineOrPoints::Points(ids[1], ids[2])))
        .unwrap();
    let fixed = s.add_constraint(K::Fixed(ids[0], 0.0, 0.0)).unwrap();
    let (_, dropped) = s.insert_control_point(c, DVec2::new(15.0, 6.0)).unwrap();
    assert!(dropped >= 1);
    let alive = |id| s.constraints().any(|(i, _)| i == id);
    assert!(!alive(flat), "waagerecht gilt nach dem Einfügen nicht mehr");
    // Der feste Anfang bewegt sich nicht und bleibt.
    assert!(alive(fixed));
}

#[test]
fn degree_change_rejects_other_kinds() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    assert!(matches!(
        s.set_control_spline_degree(l, 3),
        Err(SketchError::WrongType(_))
    ));
    assert!(matches!(
        s.insert_control_point(l, DVec2::new(5.0, 0.0)),
        Err(SketchError::WrongType(_))
    ));
    let (c, _) = spline(&mut s, &WAVE, 3);
    assert!(matches!(
        s.set_control_spline_degree(c, 4),
        Err(SketchError::InvalidInput(_))
    ));
    let before = parts(&s, c);
    // Ein Punkt am Ende ist kein Innenknoten.
    assert!(s.insert_control_point(c, DVec2::new(0.0, 0.0)).is_err());
    assert_eq!(parts(&s, c), before);
}

/// Hängt an jeden Punkt in `shared` eine Linie zu einem neuen Punkt.
fn share(s: &mut Sketch, shared: &[PointId]) {
    for &p in shared {
        let q = s.add_point(0.0, -50.0 - p.index() as f64).unwrap();
        s.add_line(p, q).unwrap();
    }
}

/// Punkte (mit Lage), Kurvenarten und Zahl der Abhängigkeiten.
type Snapshot = (Vec<(PointId, DVec2)>, Vec<CurveKind>, usize);

fn snapshot(s: &Sketch) -> Snapshot {
    (
        s.points()
            .map(|(p, _)| (p, s.point_xy(p).unwrap()))
            .collect(),
        s.curves().map(|(_, d)| d.kind.clone()).collect(),
        s.constraints().count(),
    )
}

#[test]
fn shared_interior_points_reject_a_degree_change_and_leave_everything_alone() {
    let mut s = sketch();
    let (c, ids) = spline(&mut s, &WAVE, 3);
    share(&mut s, &ids[1..6]);
    s.add_constraint(K::Fixed(ids[0], 0.0, 0.0)).unwrap();
    let before = snapshot(&s);
    assert!(matches!(
        s.set_control_spline_degree(c, 5),
        Err(SketchError::Degenerate(_))
    ));
    assert_eq!(snapshot(&s), before);
}

#[test]
fn shared_interior_points_reject_an_insert_and_leave_everything_alone() {
    let mut s = sketch();
    let (c, ids) = spline(&mut s, &WAVE, 3);
    share(&mut s, &ids[1..6]);
    s.add_constraint(K::Fixed(ids[0], 0.0, 0.0)).unwrap();
    let before = snapshot(&s);
    assert!(matches!(
        s.insert_control_point(c, DVec2::new(25.0, 2.0)),
        Err(SketchError::Degenerate(_))
    ));
    assert_eq!(snapshot(&s), before);
}

#[test]
fn a_shared_point_outside_the_affected_group_does_not_block_an_insert() {
    let mut s = sketch();
    let (c, ids) = spline(&mut s, &WAVE, 3);
    // Eingefügt wird nahe dem Anfang: Der vorletzte Punkt bleibt unberührt.
    share(&mut s, &ids[5..6]);
    let old = nurbs(&s, c);
    let before = s.point_xy(ids[5]).unwrap();
    let (p, _) = s.insert_control_point(c, DVec2::new(6.0, 5.0)).unwrap();
    assert!(parts(&s, c).0.contains(&p));
    assert_eq!(s.point_xy(ids[5]).unwrap(), before);
    assert!(max_gap(&s, c, &old) < 1e-9);
}

/// Skizzen 7, Task 12: Der Baustein der Vorschau rechnet dieselbe Kurve wie das Modell, auch beim
/// wirksamen Grad.
#[test]
fn preview_builder_matches_the_model() {
    for (n, degree) in [
        (2, 5_u8),
        (3, 5),
        (5, 5),
        (6, 5),
        (9, 5),
        (2, 3),
        (4, 3),
        (9, 3),
    ] {
        let xy: Vec<(f64, f64)> = (0..n)
            .map(|i| (i as f64 * 7.0, ((i * i) % 5) as f64 * 3.0))
            .collect();
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let (c, points) = spline(&mut s, &xy, degree);
        let built = planora_sketch::control_spline_nurbs(
            &points
                .iter()
                .map(|&p| s.point_xy(p).unwrap())
                .collect::<Vec<_>>(),
            degree,
        )
        .unwrap();
        let model = s.nurbs_of(c).unwrap();
        assert_eq!(built.degree(), model.degree(), "{n}, {degree}");
        assert_eq!(built.knots(), model.knots(), "{n}, {degree}");
        assert_eq!(
            built.control_points(),
            model.control_points(),
            "{n}, {degree}"
        );
        assert_eq!(built.weights(), model.weights(), "{n}, {degree}");
    }
}

/// Dieselbe Frage nach dem Scheitel wie `add_conic`, und dasselbe Rho wie `add_conic`.
#[test]
fn conic_checks_match_the_model() {
    let tol = Tolerance::default().linear;
    let (a, b) = (DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0));
    for apex in [
        DVec2::new(5.0, 3.0),
        DVec2::new(5.0, 0.0),
        DVec2::new(5.0, tol / 2.0),
        DVec2::new(5.0, tol * 2.0),
        DVec2::new(20.0, 0.0),
    ] {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let [p, q, r] = [a, b, apex].map(|v| s.add_point(v.x, v.y).unwrap());
        assert_eq!(
            planora_sketch::conic_apex_ok(a, apex, b, tol),
            s.add_conic(p, q, r, 0.5).is_ok(),
            "{apex:?}"
        );
    }
    for rho in [
        0.0,
        1.0,
        -0.5,
        1e-7,
        0.000001,
        0.5,
        0.999999,
        0.9999999,
        f64::NAN,
    ] {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let [p, q, r] = [a, b, DVec2::new(5.0, 3.0)].map(|v| s.add_point(v.x, v.y).unwrap());
        assert_eq!(
            planora_sketch::conic_rho_ok(rho),
            s.add_conic(p, q, r, rho).is_ok(),
            "{rho}"
        );
    }
    assert!(planora_sketch::conic_rho_ok(0.000001));
    assert!(planora_sketch::conic_rho_ok(0.999999));
    assert!(!planora_sketch::conic_rho_ok(0.9999999));
    assert!(!planora_sketch::conic_rho_ok(0.0000001));
}
