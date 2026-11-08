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
