//! „Tangente am Ende“ (Skizzen 11a, Task 4): Die Richtungen zweier Kurven an ihrem gemeinsamen
//! Endpunkt sind gleich oder entgegengesetzt; es gibt keine Krümmungszeile. Gilt auch am
//! Ellipsenbogen und im Raum, auf dem dichten wie auf dem dünnen Löserweg.

use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::{SolveReport, SolveStatus};
use planora_sketch::sparse::SolverPath;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn pt(s: &mut Sketch, x: f64, y: f64) -> PointId {
    s.add_point(x, y).unwrap()
}

fn pos(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

/// Sinus des Winkels zwischen den Richtungen `a→b` und `c→d` (0 heißt: gleich oder gegenläufig).
fn cross_unit(s: &Sketch, a: PointId, b: PointId, c: PointId, d: PointId) -> f64 {
    let u = (pos(s, b) - pos(s, a)).normalize();
    let v = (pos(s, d) - pos(s, c)).normalize();
    u.perp_dot(v)
}

/// Sechs-Punkt-Formeln für Richtung und zweite Ableitung am Ende einer Kurve (wie in
/// `smooth_g2.rs`); `side` = +1 vom Anfang des Bereichs aus, sonst −1.
fn ends_of(s: &Sketch, c: CurveId, side: f64) -> (DVec2, DVec2) {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let h = (hi - lo) * 2e-3;
    let t = if side > 0.0 { lo } else { hi };
    let f: [DVec2; 6] = std::array::from_fn(|k| s.curve_eval(c, t + side * k as f64 * h).unwrap());
    let d = (f[0] * (-137.0 / 60.0) + f[1] * 5.0 - f[2] * 5.0 + f[3] * (10.0 / 3.0) - f[4] * 1.25
        + f[5] * 0.2)
        / h
        * side;
    let a = (f[0] * (15.0 / 4.0) - f[1] * (77.0 / 6.0) + f[2] * (107.0 / 6.0) - f[3] * 13.0
        + f[4] * (61.0 / 12.0)
        - f[5] * (5.0 / 6.0))
        / (h * h);
    (d, a)
}

/// Krümmung am Ende `p` in Richtung in die Kurve hinein.
fn kappa_into(s: &Sketch, c: CurveId, p: PointId) -> f64 {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let q = pos(s, p);
    let first = s.curve_eval(c, lo).unwrap().distance(q) < s.curve_eval(c, hi).unwrap().distance(q);
    let (d, a) = ends_of(s, c, if first { 1.0 } else { -1.0 });
    let k = d.perp_dot(a) / d.length().powi(3);
    if first { k } else { -k }
}

#[test]
fn end_tangent_line_line_aligns() {
    let mut s = sketch();
    // Zwei Linien am gemeinsamen Punkt b; die zweite knickt um 30° ab.
    let a = pt(&mut s, 0.0, 0.0);
    let b = pt(&mut s, 10.0, 0.0);
    let c = pt(&mut s, 10.0 + 8.0 * 30f64.to_radians().cos(), 8.0 * 0.5);
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(b, c).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
    s.add_constraint(K::EndTangent(l1, b, l2, b)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(cross_unit(&s, a, b, b, c).abs() <= 1e-9);
    // Die Länge bleibt frei: c liegt auf der Geraden, nicht auf einer festen Stelle.
    assert!(pos(&s, c).y.abs() <= 1e-9);
}

#[test]
fn end_tangent_allows_reverse_direction() {
    let mut s = sketch();
    // Die zweite Linie läuft auf der ersten zurück (180°): Das Residuum ist schon 0.
    let a = pt(&mut s, 0.0, 0.0);
    let b = pt(&mut s, 10.0, 0.0);
    let c = pt(&mut s, 4.0, 0.0);
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(b, c).unwrap();
    s.add_constraint(K::EndTangent(l1, b, l2, b)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(r.iterations, 0, "ohne Bewegung");
    assert_eq!(pos(&s, c), DVec2::new(4.0, 0.0));
}

#[test]
fn end_tangent_has_no_curvature_row() {
    let mut s = sketch();
    // Viertelbogen um den Ursprung, Radius 3, von (3, 0) nach (0, 3); der Spline beginnt in (0, 3).
    let m = pt(&mut s, 0.0, 0.0);
    let st = pt(&mut s, 3.0, 0.0);
    let en = pt(&mut s, 0.0, 3.0);
    let arc = s.add_arc(m, st, en).unwrap();
    let q: Vec<PointId> = [(-2.0, 4.0), (-5.0, 5.0), (-8.0, 3.5), (-11.0, 5.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let sp = s
        .add_control_spline(&[en, q[0], q[1], q[2], q[3]], 3)
        .unwrap();
    s.add_constraint(K::Radius(arc, 3.0)).unwrap();
    let id = s.add_constraint(K::EndTangent(arc, en, sp, en)).unwrap();
    assert_eq!(
        s.evaluate(id, s.variables()).unwrap().residuals.len(),
        1,
        "nur die Richtungszeile"
    );
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    // Die Richtungen passen (gleich oder gegenläufig) …
    let (da, _) = ends_of(&s, arc, -1.0);
    let (db, _) = ends_of(&s, sp, 1.0);
    assert!(da.normalize().perp_dot(db.normalize()).abs() <= 1e-8);
    // … die Krümmungen laufen nicht mit (bei Glatt wäre die Summe 0).
    let (ka, kb) = (kappa_into(&s, arc, en), kappa_into(&s, sp, en));
    assert!((ka + kb).abs() > 0.01, "κ {ka} und {kb}");
}

#[test]
fn end_tangent_on_elliptic_arc() {
    let mut s = sketch();
    // Viertel einer Ellipse mit a = 10, b = 5, von (10, 0) nach (0, 5); dort läuft sie nach −x.
    let c = pt(&mut s, 0.0, 0.0);
    let a = pt(&mut s, 10.0, 0.0);
    let b = pt(&mut s, 0.0, 5.0);
    let e = s
        .add_elliptic_arc(c, DVec2::new(10.0, 0.0), 5.0, a, b)
        .unwrap();
    let d = pt(&mut s, -6.0, 8.0);
    let line = s.add_line(b, d).unwrap();
    s.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    s.add_constraint(K::EllipseMajor(e, 10.0)).unwrap();
    s.add_constraint(K::EllipseMinor(e, 5.0)).unwrap();
    s.add_constraint(K::EllipseAngle(e, 0.0)).unwrap();
    s.add_constraint(K::EndTangent(e, b, line, b)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    // Ellipsentangente am Ende: senkrecht zum Gradienten der impliziten Form.
    let p = pos(&s, b);
    let tangent = DVec2::new(-p.y / 25.0, p.x / 100.0).normalize();
    let along = (pos(&s, d) - p).normalize();
    assert!(tangent.perp_dot(along).abs() <= 1e-8, "{tangent} / {along}");
}

#[test]
fn end_tangent_rejects_inner_point() {
    let mut s = sketch();
    let q: Vec<PointId> = [(0.0, 0.0), (5.0, 3.0), (10.0, 0.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let sp = s.add_spline(&q).unwrap();
    let far = pt(&mut s, 5.0, 9.0);
    let line = s.add_line(q[1], far).unwrap();
    let before = s.to_data();
    let err = s.add_constraint(K::EndTangent(sp, q[1], line, q[1]));
    assert!(matches!(err, Err(SketchError::InvalidInput(_))), "{err:?}");
    assert_eq!(s.to_data(), before, "nichts angelegt");
}

#[test]
fn end_tangent_rejects_circle() {
    let mut s = sketch();
    let m = pt(&mut s, 0.0, 0.0);
    let circle = s.add_circle(m, 4.0).unwrap();
    let on = pt(&mut s, 4.0, 0.0);
    let far = pt(&mut s, 4.0, 5.0);
    let line = s.add_line(on, far).unwrap();
    let err = s.add_constraint(K::EndTangent(circle, on, line, on));
    assert!(matches!(err, Err(SketchError::InvalidInput(_))), "{err:?}");
}

#[test]
fn end_tangent_needs_joined_ends() {
    let mut s = sketch();
    let a = pt(&mut s, 0.0, 0.0);
    let b = pt(&mut s, 10.0, 0.0);
    let c = pt(&mut s, 12.0, 1.0);
    let d = pt(&mut s, 20.0, 5.0);
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(c, d).unwrap();
    // Getrennte Enden: abgelehnt, erst Deckungsgleich verbindet sie.
    let err = s.add_constraint(K::EndTangent(l1, b, l2, c));
    assert!(matches!(err, Err(SketchError::InvalidInput(_))), "{err:?}");
    s.add_constraint(K::Coincident(b, c)).unwrap();
    s.add_constraint(K::EndTangent(l1, b, l2, c)).unwrap();
}

#[test]
fn end_tangent_3d_two_lines() {
    let mut s = sketch();
    let a = s.add_point3(DVec3::new(0.0, 0.0, 0.0)).unwrap();
    let b = s.add_point3(DVec3::new(10.0, 2.0, 1.0)).unwrap();
    let c = s.add_point3(DVec3::new(15.0, 9.0, 4.0)).unwrap();
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(b, c).unwrap();
    s.add_constraint(K::Fixed3(a, 0.0, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed3(b, 10.0, 2.0, 1.0)).unwrap();
    s.add_constraint(K::EndTangent(l1, b, l2, b)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    let u = (s.point_xyz(b).unwrap() - s.point_xyz(a).unwrap()).normalize();
    let v = (s.point_xyz(c).unwrap() - s.point_xyz(b).unwrap()).normalize();
    assert!(u.cross(v).length() <= 1e-9, "{u} / {v}");
}

#[test]
fn end_tangent_counts_one_dof_in_2d() {
    let mut s = sketch();
    let a = pt(&mut s, 0.0, 0.0);
    let b = pt(&mut s, 10.0, 0.0);
    let c = pt(&mut s, 14.0, 3.0);
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(b, c).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
    let before = s.solve().dof;
    assert_eq!(before, 2);
    s.add_constraint(K::EndTangent(l1, b, l2, b)).unwrap();
    let after = s.solve();
    assert_eq!(after.status, SolveStatus::Solved, "{after:?}");
    assert_eq!(after.dof, before - 1);
}

// ---- 3D an Bogen, Kontrollpunkt-Spline und Ellipsenbogen ---------------------------------------

fn p3(s: &mut Sketch, x: f64, y: f64, z: f64) -> PointId {
    s.add_point3(DVec3::new(x, y, z)).unwrap()
}

fn at3(s: &Sketch, p: PointId) -> DVec3 {
    s.point_xyz(p).unwrap()
}

/// Sinus des Winkels zwischen den Richtungen `u` und `v` (Länge des Kreuzprodukts der Einheiten).
fn sin3(u: DVec3, v: DVec3) -> f64 {
    u.normalize().cross(v.normalize()).length()
}

#[test]
fn end_tangent_3d_arc_and_line() {
    let mut s = sketch();
    // Viertelbogen in der Ebene y = 1 (Normale y), Mitte (4, 1, 7), Radius 5.
    let m = p3(&mut s, 4.0, 1.0, 7.0);
    let st = p3(&mut s, 4.0, 1.0, 2.0);
    let en = p3(&mut s, 7.0, 1.0, 3.0);
    let arc = s.add_arc3(m, st, en, DVec3::Y).unwrap();
    let far = p3(&mut s, 12.0, 6.0, 9.0);
    let line = s.add_line(en, far).unwrap();
    for p in [m, st, en] {
        let at = at3(&s, p);
        s.add_constraint(K::Fixed3(p, at.x, at.y, at.z)).unwrap();
    }
    s.add_constraint(K::EndTangent(arc, en, line, en)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    // Tangente des Bogens am Ende: n × r.
    let tangent = DVec3::Y.cross(at3(&s, en) - at3(&s, m));
    let along = at3(&s, far) - at3(&s, en);
    assert!(sin3(tangent, along) <= 1e-9, "{tangent} / {along}");
}

#[test]
fn end_tangent_3d_control_spline_and_line() {
    let mut s = sketch();
    let a = p3(&mut s, 0.0, 0.0, 0.0);
    let b = p3(&mut s, 5.0, 2.0, 1.0);
    let q: Vec<PointId> = [(8.0, 6.0, 3.0), (12.0, 5.0, 6.0), (15.0, 9.0, 4.0)]
        .iter()
        .map(|&(x, y, z)| p3(&mut s, x, y, z))
        .collect();
    let sp = s.add_control_spline(&[b, q[0], q[1], q[2]], 3).unwrap();
    let line = s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed3(a, 0.0, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed3(b, 5.0, 2.0, 1.0)).unwrap();
    s.add_constraint(K::EndTangent(line, b, sp, b)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    // Am Anfang eines geklemmten B-Splines zeigt die Ableitung zum zweiten Kontrollpunkt.
    let leg = at3(&s, q[0]) - at3(&s, b);
    let dir = at3(&s, b) - at3(&s, a);
    assert!(sin3(dir, leg) <= 1e-9, "{dir} / {leg}");
}

#[test]
fn end_tangent_3d_elliptic_arc_and_line() {
    let mut s = sketch();
    let c = p3(&mut s, 0.0, 0.0, -5.0);
    let st = p3(&mut s, 6.0, 0.0, -5.0);
    let en = p3(&mut s, 0.0, 3.0, -5.0);
    let e = s
        .add_elliptic_arc3(c, DVec3::new(6.0, 0.0, 0.0), 3.0, st, en, DVec3::Z)
        .unwrap();
    let far = p3(&mut s, -3.0, 6.0, -2.0);
    let line = s.add_line(en, far).unwrap();
    s.add_constraint(K::Fixed3(c, 0.0, 0.0, -5.0)).unwrap();
    s.add_constraint(K::EndTangent(e, en, line, en)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    // Tangente der Ellipse (in der Ebene z = −5) am Punkt: senkrecht zum Gradienten.
    let CurveKind::EllipticArc {
        ax, ay, az, minor, ..
    } = s.curve(e).unwrap().kind
    else {
        panic!("kein Ellipsenbogen")
    };
    let v = s.variables();
    // Die Hauptachse mit ihrem z-Anteil (3D-Ellipsenbogen): Sie bleibt in der Ebene z = −5.
    let az = v[az.expect("3D-Ellipsenbogen hat eine z-Achse")];
    assert!(
        az.abs() <= 1e-9,
        "Hauptachse kippt aus der Ebene: z = {az:e}"
    );
    let a = DVec3::new(v[ax], v[ay], az);
    let (u, w) = (a.normalize(), DVec3::Z.cross(a.normalize()));
    // Mitte und Ende bleiben in der Ebene z = −5.
    for p in [c, st, en] {
        let z = at3(&s, p).z;
        assert!((z + 5.0).abs() <= 1e-9, "Punkt {p:?} bei z = {z}");
    }
    let d = at3(&s, en) - at3(&s, c);
    let (x, y) = (d.dot(u), d.dot(w));
    let tangent = u * (-y / (v[minor] * v[minor])) + w * (x / a.length_squared());
    let along = at3(&s, far) - at3(&s, en);
    // Die Linie läuft in der Ebene der Ellipse weiter: auch in z tangential.
    assert!(
        along.z.abs() <= 1e-9 * along.length(),
        "Linie verlässt die Ebene: {along}"
    );
    assert!(sin3(tangent, along) <= 1e-9, "{tangent} / {along}");
}

// ---- Dichter und dünner Weg ----------------------------------------------------------------------

fn same(a: &SolveReport, b: &SolveReport, sa: &Sketch, sb: &Sketch) {
    assert_eq!(a.status, b.status);
    assert_eq!(a.dof, b.dof);
    assert_eq!(a.redundant, b.redundant);
    assert_eq!(a.conflicting, b.conflicting);
    for (p, _) in sa.points() {
        let d = (sa.point_xyz(p).unwrap() - sb.point_xyz(p).unwrap())
            .abs()
            .max_element();
        assert!(d <= 1e-9, "Punkt {p:?} weicht um {d:e} ab");
    }
}

/// Gestörte Linienkette mit `EndTangent` an jedem Knick; 21 Punkte, also 42 Variablen (über
/// `SPARSE_MIN_VARS`). Mit `three_d` liegt sie im Raum (63 Variablen).
fn tangent_chain(three_d: bool, path: SolverPath) -> Sketch {
    let mut s = sketch();
    s.set_solver_path(path);
    let n = 20;
    let p: Vec<PointId> = (0..=n)
        .map(|i| {
            let x = 3.0 * i as f64;
            let y = 0.7 * (i as f64 * 1.3).sin();
            if three_d {
                s.add_point3(DVec3::new(x, y, 0.4 * (i as f64 * 0.9).cos()))
                    .unwrap()
            } else {
                s.add_point(x, y).unwrap()
            }
        })
        .collect();
    let l: Vec<CurveId> = (0..n)
        .map(|i| s.add_line(p[i], p[i + 1]).unwrap())
        .collect();
    if three_d {
        for q in &p[..2] {
            let at = s.point_xyz(*q).unwrap();
            s.add_constraint(K::Fixed3(*q, at.x, at.y, at.z)).unwrap();
        }
    } else {
        for q in &p[..2] {
            let at = pos(&s, *q);
            s.add_constraint(K::Fixed(*q, at.x, at.y)).unwrap();
        }
    }
    for i in 1..n {
        s.add_constraint(K::EndTangent(l[i - 1], p[i], l[i], p[i]))
            .unwrap();
    }
    // Ein Maß am Ende bindet die Länge der letzten Linie.
    s.add_constraint(K::Distance(p[n - 1], p[n], 3.0)).unwrap();
    s
}

#[test]
fn end_tangent_solves_alike_on_both_paths() {
    for three_d in [false, true] {
        let mut dense = tangent_chain(three_d, SolverPath::Dense);
        let mut sparse = tangent_chain(three_d, SolverPath::Sparse);
        assert!(dense.variables().len() >= 30);
        let (rd, rs) = (dense.solve(), sparse.solve());
        assert_eq!(rd.status, SolveStatus::Solved, "{three_d}: {rd:?}");
        same(&rd, &rs, &dense, &sparse);
        // Die Kette ist wirklich gerade geworden.
        let [first, second] = [0, 1].map(|i| dense.points().nth(i).unwrap().0);
        let dir = (dense.point_xyz(second).unwrap() - dense.point_xyz(first).unwrap()).normalize();
        for (p, _) in dense.points() {
            let v = dense.point_xyz(p).unwrap() - dense.point_xyz(first).unwrap();
            assert!(v.cross(dir).length() <= 1e-6, "{three_d}: Punkt {p:?}");
        }
    }
}

#[test]
fn end_tangent_kind_is_known() {
    let mut s = sketch();
    let a = pt(&mut s, 0.0, 0.0);
    let b = pt(&mut s, 10.0, 0.0);
    let c = pt(&mut s, 14.0, 3.0);
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(b, c).unwrap();
    let k = K::EndTangent(l1, b, l2, b);
    assert!(k.allowed_in_3d());
    assert!(!k.is_dimension());
    assert!(!k.needs_format_15());
    assert!(k.needs_format_21());
    assert!(!K::Tangent(l1, l2).needs_format_21());
}
