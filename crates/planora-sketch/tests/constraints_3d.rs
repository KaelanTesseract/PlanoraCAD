//! Abhängigkeiten in 3D (Skizzen 10, Task 3): Jede 3D-Lage ist nach `solve()` erfüllt.

use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::{Axis3, ConstraintKind as K, FixToggle, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::{Drag, SketchError, SolveStatus};

const EPS: f64 = 1e-9;

/// Skizze mit `tol.linear = 1e-7`: Der Löser bringt die Residuen dann unter
/// `RESIDUAL_FRACTION · tol.linear = 1e-9`, die Toleranz der Prüfungen hier.
fn sketch() -> Sketch {
    let tol = Tolerance {
        linear: 1e-7,
        ..Tolerance::default()
    };
    Sketch::new(Frame::WORLD, "S", tol)
}

/// Skizze mit der Standardtoleranz (wie in `tests/e2e.rs`).
fn default_sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn p3(s: &mut Sketch, x: f64, y: f64, z: f64) -> PointId {
    s.add_point3(DVec3::new(x, y, z)).unwrap()
}

fn line3(s: &mut Sketch, a: DVec3, b: DVec3) -> CurveId {
    let (pa, pb) = (s.add_point3(a).unwrap(), s.add_point3(b).unwrap());
    s.add_line(pa, pb).unwrap()
}

fn at(s: &Sketch, p: PointId) -> DVec3 {
    s.point_xyz(p).unwrap()
}

fn ends(s: &Sketch, l: CurveId) -> (DVec3, DVec3) {
    let CurveKind::Line { a, b } = s.curve(l).unwrap().kind else {
        panic!("keine Linie")
    };
    (at(s, a), at(s, b))
}

fn dir(s: &Sketch, l: CurveId) -> DVec3 {
    let (a, b) = ends(s, l);
    b - a
}

fn solved(s: &mut Sketch) {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
}

fn not_in_3d(r: Result<impl std::fmt::Debug, SketchError>) {
    match r {
        Err(SketchError::InvalidInput(m)) => {
            assert!(!m.is_empty());
        }
        other => panic!("erwartet InvalidInput, war {other:?}"),
    }
}

#[test]
fn coincident_3d_merges_xyz() {
    let mut s = sketch();
    let a = p3(&mut s, 0.0, 0.0, 0.0);
    let b = p3(&mut s, 1.0, 2.0, 3.0);
    s.add_constraint(K::Coincident(a, b)).unwrap();
    solved(&mut s);
    assert!(at(&s, a).distance(at(&s, b)) < EPS);
}

#[test]
fn along_z_makes_line_vertical_in_z() {
    let mut s = sketch();
    let l = line3(&mut s, DVec3::ZERO, DVec3::new(1.0, 1.0, 5.0));
    s.add_constraint(K::AlongZ(LineOrPoints::Line(l))).unwrap();
    solved(&mut s);
    let d = dir(&s, l);
    assert!(d.x.abs() < EPS && d.y.abs() < EPS, "{d:?}");
    assert!(d.z.abs() > 1.0);
    // An reiner 2D-Geometrie fiele die Linie zu einem Punkt zusammen.
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 1.0).unwrap();
    let flat = s.add_line(a, b).unwrap();
    not_in_3d(s.add_constraint(K::AlongZ(LineOrPoints::Line(flat))));
}

#[test]
fn horizontal_3d_line_has_dy_dz_zero() {
    let mut s = sketch();
    let l = line3(&mut s, DVec3::ZERO, DVec3::new(5.0, 1.0, 2.0));
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l)))
        .unwrap();
    let m = line3(&mut s, DVec3::new(0.0, 0.0, 1.0), DVec3::new(1.0, 6.0, 3.0));
    s.add_constraint(K::Vertical(LineOrPoints::Line(m)))
        .unwrap();
    solved(&mut s);
    let d = dir(&s, l);
    assert!(d.y.abs() < EPS && d.z.abs() < EPS, "{d:?}");
    let e = dir(&s, m);
    assert!(e.x.abs() < EPS && e.z.abs() < EPS, "{e:?}");
    assert_eq!(s.nearest_axis(l), Axis3::X);
    assert_eq!(s.nearest_axis(m), Axis3::Y);
    let up = line3(&mut s, DVec3::ZERO, DVec3::new(0.5, -0.5, 9.0));
    assert_eq!(s.nearest_axis(up), Axis3::Z);
}

#[test]
fn parallel_perpendicular_3d() {
    let mut s = sketch();
    let l1 = line3(&mut s, DVec3::ZERO, DVec3::new(1.0, 2.0, 3.0));
    let l2 = line3(&mut s, DVec3::new(5.0, 0.0, 0.0), DVec3::new(5.0, 3.0, 1.0));
    let l3 = line3(&mut s, DVec3::new(0.0, 5.0, 0.0), DVec3::new(2.0, 5.0, 1.0));
    s.add_constraint(K::Parallel(l1, l2)).unwrap();
    s.add_constraint(K::Perpendicular(l1, l3)).unwrap();
    solved(&mut s);
    let (d1, d2, d3) = (dir(&s, l1), dir(&s, l2), dir(&s, l3));
    assert!(d1.normalize().cross(d2.normalize()).length() < EPS);
    assert!(d1.normalize().dot(d3.normalize()).abs() < EPS);
}

#[test]
fn collinear_3d() {
    let mut s = sketch();
    let l1 = line3(&mut s, DVec3::ZERO, DVec3::new(1.0, 2.0, 3.0));
    let l2 = line3(&mut s, DVec3::new(3.0, 3.0, 7.0), DVec3::new(4.0, 6.0, 8.0));
    s.add_constraint(K::Collinear(l1, l2)).unwrap();
    solved(&mut s);
    let (a, b) = ends(&s, l1);
    let u = (b - a).normalize();
    let (c, d) = ends(&s, l2);
    assert!((c - a).cross(u).length() < EPS);
    assert!((d - a).cross(u).length() < EPS);
}

#[test]
fn equal_length_3d_lines() {
    let mut s = sketch();
    let l1 = line3(&mut s, DVec3::ZERO, DVec3::new(1.0, 2.0, 3.0));
    let l2 = line3(&mut s, DVec3::new(5.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 9.0));
    s.add_constraint(K::Equal(l1, l2)).unwrap();
    solved(&mut s);
    assert!((dir(&s, l1).length() - dir(&s, l2).length()).abs() < EPS);
}

#[test]
fn equal_radius_3d_circle_and_arc() {
    let mut s = sketch();
    let c = p3(&mut s, 0.0, 0.0, 1.0);
    let circle = s.add_circle3(c, 2.0, DVec3::X).unwrap();
    let m = p3(&mut s, 0.0, 0.0, 5.0);
    let a0 = p3(&mut s, 3.0, 0.0, 5.0);
    let a1 = p3(&mut s, 0.0, 3.0, 5.0);
    let arc = s.add_arc3(m, a0, a1, DVec3::Z).unwrap();
    s.add_constraint(K::Equal(circle, arc)).unwrap();
    solved(&mut s);
    let CurveKind::Circle { radius, .. } = s.curve(circle).unwrap().kind else {
        panic!()
    };
    let r_circle = s.variables()[radius];
    let r_arc = at(&s, a0).distance(at(&s, m));
    assert!((r_circle - r_arc).abs() < EPS, "{r_circle} {r_arc}");
    assert!((at(&s, a1).distance(at(&s, m)) - r_arc).abs() < EPS);
}

#[test]
fn tangent_line_arc3_at_shared_point() {
    let mut s = sketch();
    // Bogen in der xz-Ebene (Normale +y), gegen den Uhrzeigersinn von +x nach −z.
    let m = p3(&mut s, 0.0, 0.0, 0.0);
    let a0 = p3(&mut s, 3.0, 0.0, 0.0);
    let a1 = p3(&mut s, 0.0, 0.0, -3.0);
    let arc = s.add_arc3(m, a0, a1, DVec3::Y).unwrap();
    let q = p3(&mut s, 3.5, 2.0, 1.0);
    let l = s.add_line(a0, q).unwrap();
    s.add_constraint(K::Tangent(l, arc)).unwrap();
    solved(&mut s);
    let d = dir(&s, l).normalize();
    let n = s.curve_normal(arc);
    let radial = (at(&s, a0) - at(&s, m)).normalize();
    assert!(d.dot(radial).abs() < EPS, "senkrecht zum Radius: {d:?}");
    assert!(d.dot(n).abs() < EPS, "in der Bogenebene: {d:?}");
    assert!(dir(&s, l).length() > 0.5);
}

#[test]
fn tangent_line_circle3_away_from_shared_point() {
    let mut s = sketch();
    let c = p3(&mut s, 0.0, 0.0, 2.0);
    let circle = s.add_circle3(c, 2.0, DVec3::Z).unwrap();
    let l = line3(
        &mut s,
        DVec3::new(-5.0, 2.5, 3.0),
        DVec3::new(5.0, 2.2, 2.5),
    );
    s.add_constraint(K::Tangent(l, circle)).unwrap();
    solved(&mut s);
    let (a, b) = ends(&s, l);
    let m = at(&s, c);
    let u = (b - a).normalize();
    // In der Kreisebene und im Abstand des Radius von der Mitte.
    assert!((a - m).dot(DVec3::Z).abs() < EPS && (b - m).dot(DVec3::Z).abs() < EPS);
    let r = s.radius(circle).unwrap();
    assert!(((m - a).cross(u).length() - r).abs() < EPS);
}

#[test]
fn midpoint_3d_line() {
    let mut s = sketch();
    let p = p3(&mut s, 9.0, 9.0, 9.0);
    let l = line3(&mut s, DVec3::ZERO, DVec3::new(2.0, 4.0, 6.0));
    s.add_constraint(K::Midpoint(p, l)).unwrap();
    solved(&mut s);
    let (a, b) = ends(&s, l);
    assert!(at(&s, p).distance((a + b) * 0.5) < EPS);
}

#[test]
fn concentric_3d() {
    let mut s = sketch();
    let c1 = p3(&mut s, 0.0, 0.0, 1.0);
    let c2 = p3(&mut s, 2.0, 3.0, 4.0);
    s.add_circle3(c1, 2.0, DVec3::X).unwrap();
    s.add_circle3(c2, 1.0, DVec3::X).unwrap();
    // Konzentrisch ist in der Skizze „deckungsgleich“ der Mittelpunkte.
    s.add_constraint(K::Coincident(c1, c2)).unwrap();
    solved(&mut s);
    assert!(at(&s, c1).distance(at(&s, c2)) < EPS);
}

#[test]
fn fixed3_holds_z() {
    let mut s = sketch();
    let p = p3(&mut s, 1.0, 2.0, 3.0);
    let q = p3(&mut s, 4.0, 2.0, 7.0);
    s.add_line(p, q).unwrap();
    s.add_constraint(K::Fixed3(p, 1.0, 2.0, 3.0)).unwrap();
    s.add_constraint(K::Distance(p, q, 10.0)).unwrap();
    solved(&mut s);
    assert!(at(&s, p).distance(DVec3::new(1.0, 2.0, 3.0)) < EPS);
    assert!((at(&s, p).distance(at(&s, q)) - 10.0).abs() < EPS);
    // Fixieren als Schalter: am 3D-Punkt `Fixed3`, Lösen entfernt es (Ruling R2).
    assert_eq!(s.toggle_fixed(&[q]), FixToggle::Fixed(1));
    let qz = at(&s, q);
    assert!(
        s.constraints()
            .any(|(_, c)| c.kind == K::Fixed3(q, qz.x, qz.y, qz.z))
    );
    assert_eq!(s.toggle_fixed(&[p, q]), FixToggle::Released(2));
    assert!(
        !s.constraints()
            .any(|(_, c)| matches!(c.kind, K::Fixed3(..) | K::Fixed(..)))
    );
    // `Fixed3` an einem 2D-Punkt mit z ≠ 0 geht nicht.
    let flat = s.add_point(0.0, 0.0).unwrap();
    not_in_3d(s.add_constraint(K::Fixed3(flat, 0.0, 0.0, 1.0)));
}

#[test]
fn point_on_circle3() {
    let mut s = sketch();
    let c = p3(&mut s, 0.0, 0.0, 0.0);
    let circle = s.add_circle3(c, 2.0, DVec3::X).unwrap();
    let p = p3(&mut s, 1.0, 3.0, 1.0);
    s.add_constraint(K::PointOnCurve(p, circle)).unwrap();
    let l = line3(&mut s, DVec3::new(1.0, 1.0, 1.0), DVec3::new(4.0, 2.0, 7.0));
    let q = p3(&mut s, 3.0, -1.0, 2.0);
    s.add_constraint(K::PointOnCurve(q, l)).unwrap();
    solved(&mut s);
    let v = at(&s, p) - at(&s, c);
    assert!((v.length() - s.radius(circle).unwrap()).abs() < EPS);
    assert!(v.dot(DVec3::X).abs() < EPS);
    let (a, b) = ends(&s, l);
    assert!((at(&s, q) - a).cross((b - a).normalize()).length() < EPS);
}

#[test]
fn point_on_3d_spline_and_ellipse() {
    let mut s = sketch();
    let pts: Vec<PointId> = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(3.0, 1.0, 2.0),
        DVec3::new(6.0, 0.0, 5.0),
    ]
    .iter()
    .map(|&v| s.add_point3(v).unwrap())
    .collect();
    let spline = s.add_spline(&pts).unwrap();
    let p = p3(&mut s, 3.0, 2.0, 1.0);
    s.add_constraint(K::PointOnCurve(p, spline)).unwrap();
    let c = p3(&mut s, 0.0, 0.0, 4.0);
    let e = s
        .add_ellipse3(c, DVec3::new(0.0, 4.0, 0.0), 2.0, DVec3::X)
        .unwrap();
    let q = p3(&mut s, 1.0, 1.0, 6.5);
    s.add_constraint(K::PointOnCurve(q, e)).unwrap();
    solved(&mut s);
    assert!(nearest_on(&s, spline, at(&s, p)) < 1e-6, "auf dem Spline");
    // Auf der Ellipse: in ihrer Ebene und auf dem Umfang.
    let v = at(&s, q) - at(&s, c);
    assert!(v.dot(DVec3::X).abs() < EPS, "{v:?}");
    assert!(nearest_on(&s, e, at(&s, q)) < 1e-6, "auf der Ellipse");
}

/// Kleinster Abstand von `p` zur Kurve `c` (abgetastet und nachgeschärft).
fn nearest_on(s: &Sketch, c: CurveId, p: DVec3) -> f64 {
    let (lo, hi) = s.curve_domain3(c).unwrap();
    let dist = |t: f64| s.curve_point3(c, t).unwrap().distance(p);
    let n = 4000;
    let k = (0..=n)
        .min_by(|&i, &j| {
            let (ti, tj) = (
                lo + (hi - lo) * i as f64 / n as f64,
                lo + (hi - lo) * j as f64 / n as f64,
            );
            dist(ti).total_cmp(&dist(tj))
        })
        .unwrap();
    let step = (hi - lo) / n as f64;
    let (mut a, mut b) = (lo + step * (k as f64 - 1.0), lo + step * (k as f64 + 1.0));
    for _ in 0..200 {
        let (m1, m2) = (a + (b - a) / 3.0, b - (b - a) / 3.0);
        if dist(m1) < dist(m2) {
            b = m2;
        } else {
            a = m1;
        }
    }
    dist((a + b) * 0.5)
}

#[test]
fn distance_radius_diameter_angle_3d() {
    let mut s = sketch();
    let a = p3(&mut s, 0.0, 0.0, 0.0);
    let b = p3(&mut s, 1.0, 1.0, 1.0);
    s.add_constraint(K::Distance(a, b, 5.0)).unwrap();
    let c = p3(&mut s, 10.0, 0.0, 3.0);
    let circle = s.add_circle3(c, 2.0, DVec3::new(1.0, 1.0, 0.0)).unwrap();
    s.add_constraint(K::Radius(circle, 3.0)).unwrap();
    // Gekippter Bogen: Der Radius zählt im Raum, nicht in der Draufsicht.
    let m = p3(&mut s, 0.0, 10.0, 0.0);
    let s0 = p3(&mut s, 0.0, 10.0, 2.0);
    let s1 = p3(&mut s, 0.0, 12.0, 0.0);
    let arc = s.add_arc3(m, s0, s1, DVec3::X).unwrap();
    s.add_constraint(K::Diameter(arc, 8.0)).unwrap();
    let l1 = line3(
        &mut s,
        DVec3::new(20.0, 0.0, 0.0),
        DVec3::new(21.0, 0.0, 1.0),
    );
    let l2 = line3(
        &mut s,
        DVec3::new(20.0, 5.0, 0.0),
        DVec3::new(20.0, 6.0, 0.0),
    );
    let sixty = 60f64.to_radians();
    s.add_constraint(K::Angle(l1, l2, sixty)).unwrap();
    solved(&mut s);
    assert!((at(&s, a).distance(at(&s, b)) - 5.0).abs() < EPS);
    assert!((s.measure(&K::Radius(circle, 0.0)).unwrap() - 3.0).abs() < EPS);
    assert!((at(&s, s0).distance(at(&s, m)) - 4.0).abs() < EPS);
    assert!((at(&s, s1).distance(at(&s, m)) - 4.0).abs() < EPS);
    assert!((s.measure(&K::Diameter(arc, 0.0)).unwrap() - 8.0).abs() < EPS);
    let (d1, d2) = (dir(&s, l1).normalize(), dir(&s, l2).normalize());
    assert!((d1.dot(d2).acos() - sixty).abs() < EPS);
    assert!((s.measure(&K::Angle(l1, l2, 0.0)).unwrap() - sixty).abs() < EPS);
    assert!((s.measure(&K::Distance(a, b, 0.0)).unwrap() - 5.0).abs() < EPS);
}

#[test]
fn ellipse_axes_measure_in_3d_and_angle_is_rejected() {
    let mut s = sketch();
    let c = p3(&mut s, 0.0, 0.0, 4.0);
    // Große Halbachse senkrecht nach oben: In der Draufsicht hätte sie die Länge 0.
    let e = s
        .add_ellipse3(c, DVec3::new(0.0, 0.0, 5.0), 2.0, DVec3::X)
        .unwrap();
    assert!((s.measure(&K::EllipseMajor(e, 0.0)).unwrap() - 5.0).abs() < EPS);
    assert!((s.measure(&K::EllipseMinor(e, 0.0)).unwrap() - 2.0).abs() < EPS);
    not_in_3d(s.add_constraint(K::EllipseAngle(e, 0.3)));
    not_in_3d(s.add_constraint(K::EllipseMajor(e, 6.0)));
}

#[test]
fn symmetric_on_3d_is_rejected() {
    let mut s = sketch();
    let a = p3(&mut s, 0.0, 0.0, 1.0);
    let b = s.add_point(4.0, 1.0).unwrap();
    let (o, t) = (
        s.add_point(2.0, -5.0).unwrap(),
        s.add_point(2.0, 5.0).unwrap(),
    );
    let axis = s.add_line(o, t).unwrap();
    let n = s.constraints().count();
    not_in_3d(s.add_constraint(K::Symmetric(a, b, axis)));
    assert_eq!(s.constraints().count(), n);
    assert!(!s.is_3d_point(b), "abgelehnt hebt nichts an");
}

#[test]
fn distance_x_on_3d_is_rejected() {
    let mut s = sketch();
    let a = p3(&mut s, 0.0, 0.0, 1.0);
    let b = p3(&mut s, 4.0, 1.0, 2.0);
    not_in_3d(s.add_constraint(K::DistanceX(a, b, 3.0)));
    not_in_3d(s.add_constraint(K::DistanceY(a, b, 3.0)));
    let l = s.add_line(a, b).unwrap();
    let p = s.add_point(9.0, 9.0).unwrap();
    not_in_3d(s.add_constraint(K::DistancePointLine(p, l, 3.0)));
}

/// Viereck aus vier 2D-Linien ohne waagerecht/senkrecht: Ecken, Linien.
fn quad(s: &mut Sketch) -> ([PointId; 4], [CurveId; 4]) {
    let c = [
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(10.0, 6.0),
        DVec2::new(0.0, 6.0),
    ];
    let p = c.map(|v| s.add_point(v.x, v.y).unwrap());
    let l = [0, 1, 2, 3].map(|i| s.add_line(p[i], p[(i + 1) % 4]).unwrap());
    (p, l)
}

#[test]
fn coincident_lifts_rect_corner_and_profile_disappears() {
    let mut s = sketch();
    let (p, _) = quad(&mut s);
    // Eine 3D-Linie, die an der Ecke beginnt, benutzt die Ecke selbst; die bleibt 2D.
    let top = p3(&mut s, 0.0, 0.0, 8.0);
    s.add_line(p[0], top).unwrap();
    assert!(!s.is_3d_point(p[0]));
    assert_eq!(s.profiles().profiles.len(), 1);
    // „Deckungsgleich“ mit einem hochliegenden Punkt hebt die Ecke an.
    let high = p3(&mut s, 0.0, 0.0, 3.0);
    s.add_constraint(K::Fixed3(high, 0.0, 0.0, 3.0)).unwrap();
    s.add_constraint(K::Coincident(p[0], high)).unwrap();
    assert!(s.is_3d_point(p[0]));
    assert!(!s.is_3d_point(p[1]));
    solved(&mut s);
    assert!(at(&s, p[0]).distance(DVec3::new(0.0, 0.0, 3.0)) < EPS);
    assert!(
        s.profiles().profiles.is_empty(),
        "das Viereck hat seine Ebene verloren"
    );
}

#[test]
fn coincident_lifts_arc_with_its_plane() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a0 = s.add_point(3.0, 0.0).unwrap();
    let a1 = s.add_point(0.0, 3.0).unwrap();
    let arc = s.add_arc(m, a0, a1).unwrap();
    let high = p3(&mut s, 3.0, 0.0, 2.0);
    s.add_constraint(K::Fixed3(high, 3.0, 0.0, 2.0)).unwrap();
    s.add_constraint(K::Coincident(a0, high)).unwrap();
    // Ein Bogen an einem 3D-Punkt hat immer eine Ebene (Task 2): alle Punkte 3D, Normale z.
    assert!(s.is_3d_point(m) && s.is_3d_point(a1));
    assert_eq!(s.curve(arc).unwrap().normal, Some(DVec3::Z));
    solved(&mut s);
    assert!((at(&s, m).z - 2.0).abs() < EPS && (at(&s, a1).z - 2.0).abs() < EPS);
}

#[test]
fn coincident_blocked_when_corner_has_2d_only_constraint() {
    let mut s = sketch();
    let (p, _) = quad(&mut s);
    s.add_constraint(K::DistanceX(p[0], p[1], 10.0)).unwrap();
    let high = p3(&mut s, 0.0, 0.0, 3.0);
    let n = s.constraints().count();
    let vars = s.variable_count();
    match s.add_constraint(K::Coincident(p[0], high)) {
        Err(SketchError::InvalidInput(m)) => assert!(!m.is_empty()),
        other => panic!("erwartet gesperrt, war {other:?}"),
    }
    assert!(!s.is_3d_point(p[0]));
    assert_eq!(s.constraints().count(), n);
    assert_eq!(s.variable_count(), vars, "abgelehnt hinterlässt nichts");
}

#[test]
fn drag_3d_point_follows_target_in_plane() {
    let mut s = sketch();
    let p = p3(&mut s, 1.0, 2.0, 3.0);
    // Ohne Ebene wie bisher: nur x und y folgen, z bleibt.
    s.drag(&[Drag {
        point: p,
        target: DVec3::new(5.0, 6.0, 0.0),
        plane: None,
    }])
    .unwrap();
    assert!(at(&s, p).distance(DVec3::new(5.0, 6.0, 3.0)) < EPS);
    // Mit Ebene (y = 0): Das Ziel wird auf die Ebene geworfen.
    s.drag(&[Drag {
        point: p,
        target: DVec3::new(4.0, 1.0, 9.0),
        plane: Some((DVec3::new(0.0, 0.0, 5.0), DVec3::Y)),
    }])
    .unwrap();
    assert!(at(&s, p).distance(DVec3::new(4.0, 0.0, 9.0)) < EPS);
    // Gebunden: Der Zug zieht auch z, die Abhängigkeit bleibt erfüllt.
    let q = p3(&mut s, 0.0, 0.0, 0.0);
    s.add_constraint(K::Fixed3(q, 0.0, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(p, q, 5.0)).unwrap();
    solved(&mut s);
    let before = at(&s, p);
    let r = s
        .drag(&[Drag {
            point: p,
            target: DVec3::new(0.0, 0.0, 5.0),
            plane: Some((DVec3::ZERO, DVec3::Y)),
        }])
        .unwrap();
    assert_eq!(r.status, SolveStatus::Solved);
    let got = at(&s, p);
    assert!((got.length() - 5.0).abs() < EPS);
    assert!(got.z > before.z + 0.1, "{before:?} -> {got:?}");
}

/// Bitmuster der Variablen nach `solve()`, aufgenommen vor Task 3 (Stand c7df131).
fn bits(s: &Sketch) -> Vec<u64> {
    s.variables().iter().map(|v| v.to_bits()).collect()
}

#[test]
fn pure_2d_results_unchanged() {
    // Die bemaßte Skizze aus `tests/e2e.rs` (a_dimensioned_rectangle_extrudes_to_the_right_volume).
    let mut s = default_sketch();
    let r = s
        .add_rectangle(DVec2::new(1.0, 1.0), DVec2::new(30.0, 20.0))
        .unwrap();
    s.add_constraint(K::Fixed(r.points[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(r.points[0], r.points[1], 40.0))
        .unwrap();
    s.add_constraint(K::Distance(r.points[1], r.points[2], 25.0))
        .unwrap();
    s.solve();
    assert_eq!(
        bits(&s),
        [
            13643305828693114880,
            13645575458436546560,
            4630826316843712439,
            13649552718228357120,
            4630826316843712423,
            4627730092099894984,
            13644626742074933248,
            4627730092099894941
        ]
    );
    // Dazu Tangente am gemeinsamen Ende, Gleich, Rechtwinklig, Punkt auf Kreis und Radius.
    let mut s = default_sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.5).unwrap();
    let m = s.add_point(10.0, 5.0).unwrap();
    let e = s.add_point(15.0, 5.5).unwrap();
    let l = s.add_line(a, b).unwrap();
    let arc = s.add_arc(m, b, e).unwrap();
    let c0 = s.add_point(30.0, 0.0).unwrap();
    let circle = s.add_circle(c0, 3.0).unwrap();
    let q = s.add_point(25.0, 10.0).unwrap();
    let l2 = s.add_line(c0, q).unwrap();
    s.add_constraint(K::Tangent(l, arc)).unwrap();
    s.add_constraint(K::Equal(arc, circle)).unwrap();
    s.add_constraint(K::Perpendicular(l, l2)).unwrap();
    s.add_constraint(K::PointOnCurve(q, circle)).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Radius(circle, 4.0)).unwrap();
    s.solve();
    assert_eq!(
        bits(&s),
        [
            4423941918867855360,
            13660731948409546752,
            4622191336385278524,
            4608776321369896389,
            4621907647709419716,
            4617678110680399611,
            4624158871002046648,
            4617780014315083118,
            4628504701710417696,
            4613973701163364396,
            4616189618054770875,
            4628362857372926192,
            4619549376314140671
        ]
    );
}

/// Gelöst und ohne überzählige Abhängigkeit: Jede Zeile ist eine eigene Bedingung.
fn solved_without_redundant(s: &mut Sketch) {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.redundant.is_empty(), "überzählig: {r:?}");
}

#[test]
fn tangent_3d_arcs_at_shared_end_are_not_redundant() {
    let mut s = sketch();
    // Zwei Bögen mit Normale z (an 3D-Punkten), gemeinsames Ende: ein S-Stück.
    let m1 = p3(&mut s, 0.0, 0.0, 2.0);
    let a0 = p3(&mut s, 3.0, 0.0, 2.0);
    let j = p3(&mut s, 0.0, 3.0, 2.0);
    let arc1 = s.add_arc3(m1, a0, j, DVec3::Z).unwrap();
    let m2 = p3(&mut s, 0.5, 6.0, 2.0);
    let e = p3(&mut s, -2.5, 6.0, 2.0);
    let arc2 = s.add_arc3(m2, e, j, DVec3::Z).unwrap();
    s.add_constraint(K::Tangent(arc1, arc2)).unwrap();
    solved_without_redundant(&mut s);
    let (u, v) = (at(&s, m1) - at(&s, j), at(&s, m2) - at(&s, j));
    assert!(u.normalize().cross(v.normalize()).length() < EPS);
    // Linie–Bogen am gemeinsamen Ende, Linie mit einem 2D-Ende.
    let q = s.add_point(4.0, -2.0).unwrap();
    let l = s.add_line(a0, q).unwrap();
    s.add_constraint(K::Tangent(l, arc1)).unwrap();
    solved_without_redundant(&mut s);
}

#[test]
fn tangent_2d_line_to_3d_round_and_ellipse_is_not_redundant() {
    let mut s = sketch();
    let c = p3(&mut s, 0.0, 0.0, 0.5);
    let circle = s.add_circle3(c, 2.0, DVec3::Z).unwrap();
    let (a, b) = (
        s.add_point(-5.0, 2.5).unwrap(),
        s.add_point(5.0, 2.2).unwrap(),
    );
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Tangent(l, circle)).unwrap();
    solved_without_redundant(&mut s);
    assert!(
        at(&s, c).z.abs() < EPS,
        "die 2D-Linie zieht den Kreis in die Ebene"
    );
    let ce = p3(&mut s, 20.0, 0.0, 0.5);
    let e = s
        .add_ellipse3(ce, DVec3::new(4.0, 0.0, 0.0), 2.0, DVec3::Z)
        .unwrap();
    let (a, b) = (
        s.add_point(14.0, 2.5).unwrap(),
        s.add_point(26.0, 2.2).unwrap(),
    );
    let l2 = s.add_line(a, b).unwrap();
    s.add_constraint(K::Tangent(l2, e)).unwrap();
    solved_without_redundant(&mut s);
    assert!(at(&s, ce).z.abs() < EPS);
    assert!(
        nearest_on(&s, e, at(&s, a)) > 1e-3,
        "Vorbedingung: Linie nicht auf der Ellipse"
    );
}

#[test]
fn tangent_round_curves_in_tilted_planes_is_rejected() {
    let mut s = sketch();
    let c1 = p3(&mut s, 0.0, 0.0, 0.0);
    let k1 = s.add_circle3(c1, 1.0, DVec3::Z).unwrap();
    let c2 = p3(&mut s, 2.0, 0.0, 0.0);
    let k2 = s.add_circle3(c2, 1.0, DVec3::X).unwrap();
    let n = s.constraints().count();
    not_in_3d(s.add_constraint(K::Tangent(k1, k2)));
    assert_eq!(s.constraints().count(), n);
    // In parallelen Ebenen geht es.
    let c3 = p3(&mut s, 3.0, 0.5, 1.0);
    let k3 = s.add_circle3(c3, 1.0, DVec3::Z).unwrap();
    s.add_constraint(K::Tangent(k1, k3)).unwrap();
    solved_without_redundant(&mut s);
    let gap = at(&s, c3) - at(&s, c1);
    assert!(gap.z.abs() < EPS);
    let (r1, r3) = (s.radius(k1).unwrap(), s.radius(k3).unwrap());
    assert!((gap.length() - (r1 + r3)).abs() < EPS);
}

#[test]
fn fixed_corner_keeps_z_when_lifted() {
    let mut s = sketch();
    let (p, _) = quad(&mut s);
    s.add_constraint(K::Fixed(p[0], 0.0, 0.0)).unwrap();
    let high = p3(&mut s, 0.0, 0.0, 3.0);
    s.add_constraint(K::Coincident(p[0], high)).unwrap();
    assert!(s.is_3d_point(p[0]));
    // Das `Fixed` an der angehobenen Ecke hält jetzt auch z = 0.
    assert!(
        s.constraints()
            .any(|(_, c)| c.kind == K::Fixed3(p[0], 0.0, 0.0, 0.0))
    );
    assert!(!s.constraints().any(|(_, c)| matches!(c.kind, K::Fixed(..))));
    solved(&mut s);
    assert!(at(&s, p[0]).distance(DVec3::ZERO) < EPS, "die Ecke bleibt");
    assert!(
        at(&s, high).distance(DVec3::ZERO) < EPS,
        "der hohe Punkt kommt herunter"
    );
    // `Fixed` direkt an einem 3D-Punkt wird `Fixed3` mit der jetzigen Höhe.
    let q = p3(&mut s, 5.0, 5.0, 7.0);
    let id = s.add_constraint(K::Fixed(q, 5.0, 5.0)).unwrap();
    assert_eq!(s.constraint(id).unwrap().kind, K::Fixed3(q, 5.0, 5.0, 7.0));
}
