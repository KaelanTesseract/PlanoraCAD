//! 3D-Punkte im Skizzenmodell (Skizzen 10, Task 1).

use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::model::{MAX_COORD, Sketch};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn point3_roundtrips_xyz() {
    let mut s = sketch();
    let p = s.add_point3(DVec3::new(1.0, 2.0, 3.0)).unwrap();
    assert!(s.is_3d_point(p));
    assert_eq!(s.point_xyz(p).unwrap(), DVec3::new(1.0, 2.0, 3.0));
    // Die Draufsicht liefert (x, y).
    assert_eq!(s.point_xy(p).unwrap(), DVec2::new(1.0, 2.0));
    assert_eq!(s.variable_count(), 3);
    s.set_point_xyz(p, DVec3::new(-4.0, 5.0, -6.0)).unwrap();
    assert_eq!(s.point_xyz(p).unwrap(), DVec3::new(-4.0, 5.0, -6.0));
}

#[test]
fn point3_at_z0_stays_3d() {
    let mut s = sketch();
    let p = s.add_point3(DVec3::new(1.0, 2.0, 0.0)).unwrap();
    assert!(s.is_3d_point(p));
    assert_eq!(s.variable_count(), 3);
    s.set_point_xyz(p, DVec3::new(1.0, 2.0, 7.0)).unwrap();
    s.set_point_xyz(p, DVec3::new(1.0, 2.0, 0.0)).unwrap();
    assert!(
        s.is_3d_point(p),
        "z = 0 macht einen 3D-Punkt nicht wieder 2D"
    );
    // Daten tragen z mit und laden ihn wieder.
    let d = s.to_data();
    assert!(d.points[p.index()].z.is_some());
    let back = Sketch::from_data(d.clone()).unwrap();
    assert!(back.is_3d_point(p));
    assert_eq!(back.to_data(), d);
}

#[test]
fn plain_point_has_no_z_var() {
    let mut s = sketch();
    let before = s.variable_count();
    let p = s.add_point(1.0, 2.0).unwrap();
    assert_eq!(s.variable_count(), before + 2);
    assert!(!s.is_3d_point(p));
    assert_eq!(s.point_xyz(p).unwrap(), DVec3::new(1.0, 2.0, 0.0));
    assert_eq!(s.to_data().points[p.index()].z, None);
    // Setzen mit z = 0 bleibt 2D und erlaubt.
    s.set_point_xyz(p, DVec3::new(3.0, 4.0, 0.0)).unwrap();
    assert!(!s.is_3d_point(p));
    assert_eq!(s.variable_count(), before + 2);
    assert_eq!(s.point_xy(p).unwrap(), DVec2::new(3.0, 4.0));
}

#[test]
fn z_over_max_coord_is_rejected() {
    let mut s = sketch();
    let n = s.variable_count();
    for z in [2.0 * MAX_COORD, -2.0 * MAX_COORD, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            s.add_point3(DVec3::new(0.0, 0.0, z)),
            Err(SketchError::InvalidInput(_))
        ));
    }
    assert_eq!(s.variable_count(), n, "abgelehnt hinterlässt nichts");
    let p = s.add_point3(DVec3::new(0.0, 0.0, 1.0)).unwrap();
    assert!(matches!(
        s.set_point_xyz(p, DVec3::new(0.0, 0.0, 2.0 * MAX_COORD)),
        Err(SketchError::InvalidInput(_))
    ));
    assert_eq!(s.point_xyz(p).unwrap(), DVec3::new(0.0, 0.0, 1.0));
}

#[test]
fn set_z_on_2d_point_is_rejected() {
    let mut s = sketch();
    let p = s.add_point(1.0, 2.0).unwrap();
    assert!(matches!(
        s.set_point_xyz(p, DVec3::new(5.0, 6.0, 1.0)),
        Err(SketchError::InvalidInput(_))
    ));
    assert!(!s.is_3d_point(p));
    assert_eq!(s.point_xy(p).unwrap(), DVec2::new(1.0, 2.0), "unverändert");
}

#[test]
fn three_d_flag_is_stored_and_carried_in_data() {
    let mut s = sketch();
    assert!(!s.three_d);
    assert!(!s.to_data().three_d);
    s.set_three_d(true);
    assert!(s.three_d);
    let d = s.to_data();
    assert!(d.three_d);
    assert!(Sketch::from_data(d).unwrap().three_d);
    assert!(s.clone().three_d);
    s.set_three_d(false);
    assert!(!s.three_d);
}

// ---- Ebene Kurven mit eigener Normale (Skizzen 10, Task 2) -------------------------------------

use planora_math::TAU;
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::{CurveId, CurveKind};
use planora_sketch::solver::SolveStatus;

const EPS: f64 = 1e-9;

/// Punkte gleichmäßig über den Bereich der Kurve.
fn samples(s: &Sketch, c: CurveId) -> Vec<DVec3> {
    let (lo, hi) = s.curve_domain3(c).unwrap();
    (0..=32)
        .map(|k| {
            s.curve_point3(c, lo + (hi - lo) * f64::from(k) / 32.0)
                .unwrap()
        })
        .collect()
}

#[test]
fn circle3_lies_in_its_plane() {
    let mut s = sketch();
    let m = s.add_point3(DVec3::new(0.0, 0.0, 5.0)).unwrap();
    let c = s.add_circle3(m, 2.0, DVec3::new(1.0, 0.0, 0.0)).unwrap();
    assert_eq!(s.curve(c).unwrap().normal, Some(DVec3::X));
    assert_eq!(s.curve_normal(c), DVec3::X);
    assert!(s.is_3d_curve(c));
    let center = DVec3::new(0.0, 0.0, 5.0);
    for p in samples(&s, c) {
        assert!(p.x.abs() < EPS, "x = 0: {p}");
        assert!((p.distance(center) - 2.0).abs() < EPS, "Abstand 2: {p}");
    }
    // Die Basis ist stabil: Die Skizzen-X-Achse steht senkrecht auf der Ebene, also beginnt der
    // Kreis in Richtung der Skizzen-Y-Achse.
    let p0 = s.curve_point3(c, 0.0).unwrap();
    assert!(p0.distance(DVec3::new(0.0, 2.0, 5.0)) < EPS, "{p0}");
}

#[test]
fn circle3_basis_starts_along_projected_x() {
    let mut s = sketch();
    let m = s.add_point3(DVec3::ZERO).unwrap();
    let n = DVec3::new(1.0, 0.0, 1.0);
    let c = s.add_circle3(m, 1.0, n).unwrap();
    let n = n.normalize();
    assert!(s.curve_normal(c).distance(n) < EPS, "normiert");
    let u = (DVec3::X - n * n.x).normalize();
    assert!(s.curve_point3(c, 0.0).unwrap().distance(u) < EPS);
    // Gegen den Uhrzeigersinn um die Normale: v = n × u.
    let v = n.cross(u);
    assert!(s.curve_point3(c, TAU / 4.0).unwrap().distance(v) < EPS);
}

#[test]
fn arc3_keeps_ends_in_plane_after_solve() {
    let mut s = sketch();
    let n = DVec3::new(0.0, 1.0, 1.0).normalize();
    let (u, v) = (DVec3::X, n.cross(DVec3::X));
    let c0 = DVec3::new(1.0, 2.0, 3.0);
    let m = s.add_point3(c0).unwrap();
    let a = s.add_point3(c0 + u * 3.0).unwrap();
    // Das Ende liegt neben der Ebene und neben dem Kreis; der Löser zieht es hin.
    let b = s.add_point3(c0 + v * 3.5 + n * 0.4).unwrap();
    let arc = s.add_arc3(m, a, b, n).unwrap();
    assert!(s.curve_normal(arc).distance(n) < EPS);
    let report = s.solve();
    assert_eq!(report.status, SolveStatus::Solved, "{report:?}");
    let (pm, pa, pb) = (
        s.point_xyz(m).unwrap(),
        s.point_xyz(a).unwrap(),
        s.point_xyz(b).unwrap(),
    );
    assert!((pa - pm).dot(n).abs() < EPS, "Start in der Ebene");
    assert!((pb - pm).dot(n).abs() < EPS, "Ende in der Ebene");
    assert!(
        ((pa - pm).length() - (pb - pm).length()).abs() < EPS,
        "Ende auf dem Kreis durch den Start"
    );
    for p in samples(&s, arc) {
        assert!((p - pm).dot(n).abs() < EPS);
        assert!(((p - pm).length() - (pa - pm).length()).abs() < EPS);
    }
    let (lo, hi) = s.curve_domain3(arc).unwrap();
    assert!(s.curve_point3(arc, lo).unwrap().distance(pa) < EPS);
    assert!(s.curve_point3(arc, hi).unwrap().distance(pb) < EPS);
}

#[test]
fn ellipse3_major_perpendicular_to_normal() {
    let mut s = sketch();
    let c0 = DVec3::new(0.0, 1.0, 2.0);
    let m = s.add_point3(c0).unwrap();
    // Die große Halbachse steht schräg zur Ebene; sie wird in die Ebene gelegt.
    let e = s
        .add_ellipse3(m, DVec3::new(0.5, 4.0, 1.0), 2.0, DVec3::X)
        .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let a = s.curve_point3(e, 0.0).unwrap() - c0;
    assert!(
        a.dot(DVec3::X).abs() < EPS,
        "große Halbachse senkrecht zur Normale"
    );
    assert!((a.length() - DVec3::new(0.0, 4.0, 1.0).length()).abs() < EPS);
    let b = s.curve_point3(e, TAU / 4.0).unwrap() - c0;
    assert!((b.length() - 2.0).abs() < EPS, "kleine Halbachse");
    assert!(b.dot(a).abs() < EPS);
    assert!(
        b.distance(DVec3::X.cross(a).normalize() * 2.0) < EPS,
        "links der großen"
    );
    for p in samples(&s, e) {
        assert!(p.x.abs() < EPS);
    }
    let Some(CurveKind::Ellipse { az, .. }) = s.curve(e).map(|d| d.kind.clone()) else {
        panic!("Ellipse erwartet");
    };
    assert!(
        az.is_some(),
        "3D-Ellipse hat eine z-Variable der großen Halbachse"
    );
}

#[test]
fn elliptic_arc3_ends_in_plane_after_solve() {
    let mut s = sketch();
    let n = DVec3::Y;
    let m = s.add_point3(DVec3::new(1.0, 1.0, 1.0)).unwrap();
    let a = s.add_point3(DVec3::new(5.0, 1.3, 1.0)).unwrap();
    let b = s.add_point3(DVec3::new(1.0, 0.8, -1.0)).unwrap();
    let arc = s
        .add_elliptic_arc3(m, DVec3::new(4.0, 0.0, 0.0), 2.0, a, b, n)
        .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let pm = s.point_xyz(m).unwrap();
    for p in [a, b] {
        let q = s.point_xyz(p).unwrap();
        assert!((q - pm).dot(n).abs() < EPS, "Ende in der Ebene");
    }
    let (lo, hi) = s.curve_domain3(arc).unwrap();
    let (pa, pb) = (s.point_xyz(a).unwrap(), s.point_xyz(b).unwrap());
    assert!(s.curve_point3(arc, lo).unwrap().distance(pa) < 1e-7);
    assert!(s.curve_point3(arc, hi).unwrap().distance(pb) < 1e-7);
}

#[test]
fn normal_zero_is_rejected() {
    let mut s = sketch();
    let m = s.add_point3(DVec3::new(0.0, 0.0, 1.0)).unwrap();
    let p = s.add_point(1.0, 0.0).unwrap();
    let q = s.add_point(0.0, 1.0).unwrap();
    let n = s.variable_count();
    for bad in [DVec3::ZERO, DVec3::new(f64::NAN, 0.0, 1.0), DVec3::INFINITY] {
        assert!(matches!(
            s.add_circle3(m, 1.0, bad),
            Err(SketchError::InvalidInput(_))
        ));
        assert!(matches!(
            s.add_arc3(m, p, q, bad),
            Err(SketchError::InvalidInput(_))
        ));
        assert!(matches!(
            s.add_ellipse3(m, DVec3::X * 2.0, 1.0, bad),
            Err(SketchError::InvalidInput(_))
        ));
    }
    assert_eq!(s.variable_count(), n, "abgelehnt hinterlässt nichts");
    assert!(!s.is_3d_point(p) && !s.is_3d_point(q));
    assert_eq!(s.curves().count(), 0);
    let c = s.add_circle3(m, 1.0, DVec3::Z).unwrap();
    assert!(matches!(
        s.set_curve_normal(c, DVec3::ZERO),
        Err(SketchError::InvalidInput(_))
    ));
}

#[test]
fn normal_with_2d_center_promotes_center() {
    let mut s = sketch();
    let m = s.add_point(1.0, 2.0).unwrap();
    let n = s.variable_count();
    let c = s.add_circle3(m, 1.0, DVec3::Y).unwrap();
    assert!(s.is_3d_point(m), "der Mittelpunkt wird 3D");
    assert_eq!(
        s.variable_count(),
        n + 2,
        "z des Mittelpunkts und der Radius"
    );
    assert_eq!(s.point_xyz(m).unwrap(), DVec3::new(1.0, 2.0, 0.0));
    assert_eq!(s.curve(c).unwrap().normal, Some(DVec3::Y));
}

#[test]
fn normal_001_on_2d_points_is_plain_circle() {
    let mut s = sketch();
    let m = s.add_point(1.0, 2.0).unwrap();
    let n = s.variable_count();
    let c = s.add_circle3(m, 1.0, DVec3::new(0.0, 0.0, 3.0)).unwrap();
    assert_eq!(s.curve(c).unwrap().normal, None);
    assert!(!s.is_3d_curve(c));
    assert!(!s.is_3d_point(m));
    assert_eq!(s.variable_count(), n + 1, "nur der Radius");
    assert_eq!(s.curve_normal(c), DVec3::Z);
    // Wie ein gewöhnlicher Kreis.
    let mut t = sketch();
    let tm = t.add_point(1.0, 2.0).unwrap();
    t.add_circle(tm, 1.0).unwrap();
    assert_eq!(s.to_data(), t.to_data());
    // Bogen und Ellipse ebenso: keine Normale, keine interne Abhängigkeit „In Ebene“.
    let a = s.add_point(2.0, 2.0).unwrap();
    let b = s.add_point(1.0, 3.0).unwrap();
    let arc = s.add_arc3(m, a, b, DVec3::Z).unwrap();
    let e = s.add_ellipse3(m, DVec3::X * 3.0, 1.0, DVec3::Z).unwrap();
    for k in [arc, e] {
        assert_eq!(s.curve(k).unwrap().normal, None);
        assert!(!s.is_3d_curve(k));
    }
    assert!(
        !s.constraint_entries()
            .any(|k| matches!(k, ConstraintKind::InPlane(_)))
    );
    // Ein 3D-Mittelpunkt gibt dagegen eine Normale.
    let m3 = s.add_point3(DVec3::new(0.0, 0.0, 4.0)).unwrap();
    let c3 = s.add_circle3(m3, 1.0, DVec3::Z).unwrap();
    assert_eq!(s.curve(c3).unwrap().normal, Some(DVec3::Z));
    assert!(s.is_3d_curve(c3));
}

#[test]
fn plain_curves_sample_like_curve_eval() {
    let mut s = sketch();
    let m = s.add_point(1.0, 2.0).unwrap();
    let a = s.add_point(3.0, 2.0).unwrap();
    let b = s.add_point(1.0, 4.0).unwrap();
    let arc = s.add_arc(m, a, b).unwrap();
    let line = s.add_line(a, b).unwrap();
    let e = s.add_ellipse(m, DVec2::new(3.0, 1.0), 1.0).unwrap();
    for c in [arc, line, e] {
        assert!(!s.is_3d_curve(c));
        assert_eq!(s.curve_domain3(c), s.curve_domain(c));
        let (lo, hi) = s.curve_domain(c).unwrap();
        for k in 0..=8 {
            let t = lo + (hi - lo) * f64::from(k) / 8.0;
            assert_eq!(
                s.curve_point3(c, t).unwrap(),
                s.curve_eval(c, t).unwrap().extend(0.0)
            );
        }
    }
}

#[test]
fn line3_and_splines3_sample_in_space() {
    let mut s = sketch();
    let p = s.add_point3(DVec3::new(0.0, 0.0, 0.0)).unwrap();
    let q = s.add_point3(DVec3::new(0.0, 0.0, 4.0)).unwrap();
    let r = s.add_point3(DVec3::new(2.0, 0.0, 6.0)).unwrap();
    let line = s.add_line(p, r).unwrap();
    assert!(s.is_3d_curve(line));
    let mid = s.curve_point3(line, 0.5).unwrap();
    assert!(mid.distance(DVec3::new(1.0, 0.0, 3.0)) < EPS);
    let sp = s.add_spline(&[p, r, q]).unwrap();
    for (t, at) in [(0.0, p), (1.0, r), (2.0, q)] {
        let want = s.point_xyz(at).unwrap();
        assert!(s.curve_point3(sp, t).unwrap().distance(want) < EPS);
    }
    let cs = s.add_control_spline(&[p, r, q], 3).unwrap();
    let (lo, hi) = s.curve_domain3(cs).unwrap();
    assert!(s.curve_point3(cs, lo).unwrap().distance(DVec3::ZERO) < EPS);
    let end = s.curve_point3(cs, hi).unwrap();
    assert!(end.distance(DVec3::new(0.0, 0.0, 4.0)) < EPS);
}

#[test]
fn set_curve_normal_only_on_planar_kinds() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(1.0, 0.0).unwrap();
    let line = s.add_line(a, b).unwrap();
    assert!(matches!(
        s.set_curve_normal(line, DVec3::X),
        Err(SketchError::WrongType(_))
    ));
    assert_eq!(s.curve_normal(line), DVec3::Z);
    // Einen Kreis in der Skizzenebene kippen.
    let c = s.add_circle(a, 2.0).unwrap();
    s.set_curve_normal(c, DVec3::new(0.0, -2.0, 0.0)).unwrap();
    assert_eq!(s.curve(c).unwrap().normal, Some(-DVec3::Y));
    assert!(s.is_3d_point(a), "Mittelpunkt wird 3D");
    for p in samples(&s, c) {
        assert!(p.y.abs() < EPS);
    }
    // Eine Ellipse kippen: Die große Halbachse liegt danach in der neuen Ebene.
    let m = s.add_point(5.0, 0.0).unwrap();
    let e = s.add_ellipse(m, DVec2::new(3.0, 1.0), 1.0).unwrap();
    s.set_curve_normal(e, DVec3::X).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let pm = s.point_xyz(m).unwrap();
    for p in samples(&s, e) {
        assert!((p - pm).x.abs() < EPS);
    }
    assert!(
        s.constraint_entries()
            .any(|k| *k == ConstraintKind::InPlane(e))
    );
}

#[test]
fn in_plane_is_internal_and_survives_data_roundtrip() {
    let mut s = sketch();
    let m = s.add_point3(DVec3::new(0.0, 0.0, 1.0)).unwrap();
    let a = s.add_point3(DVec3::new(0.0, 2.0, 1.0)).unwrap();
    let b = s.add_point3(DVec3::new(0.0, 0.0, 3.0)).unwrap();
    let arc = s.add_arc3(m, a, b, DVec3::X).unwrap();
    assert!(matches!(
        s.add_constraint(ConstraintKind::InPlane(arc)),
        Err(SketchError::WrongType(_))
    ));
    let d = s.to_data();
    assert_eq!(d.curves[arc.index()].normal, Some(DVec3::X));
    let k = d
        .constraints
        .iter()
        .find(|k| k.kind == ConstraintKind::InPlane(arc))
        .expect("InPlane angelegt");
    assert!(k.internal);
    let back = Sketch::from_data(d.clone()).unwrap();
    assert_eq!(back.to_data(), d);
    assert_eq!(back.curve_normal(arc), DVec3::X);
    // Eine nicht normierte Normale in den Daten wird abgelehnt.
    let mut bad = d.clone();
    bad.curves[arc.index()].normal = Some(DVec3::new(2.0, 0.0, 0.0));
    assert!(Sketch::from_data(bad).is_err());
    // Eine Normale an einer Linie ebenso.
    let mut t = sketch();
    let p = t.add_point(0.0, 0.0).unwrap();
    let q = t.add_point(1.0, 0.0).unwrap();
    t.add_line(p, q).unwrap();
    let mut bad = t.to_data();
    bad.curves[0].normal = Some(DVec3::X);
    assert!(Sketch::from_data(bad).is_err());
}
