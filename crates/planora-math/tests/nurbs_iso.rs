//! Skizzen 11b, Task 15: Isokurven einer NURBS-Fläche (`u = const` über v, `v = const` über u),
//! exakt über Knoteneinfügen bis zur vollen Vielfachheit.

use planora_math::DVec3;
use planora_math::nurbs::*;

/// Bikubische rationale Fläche; u-Knoten mit einem doppelten inneren Knoten bei 0,5.
fn sample() -> NurbsSurface {
    let ku = vec![0.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0];
    let kv = vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.7, 1.0, 1.0, 1.0, 1.0];
    let (nu, nv) = (7, 6);
    let mut c = Vec::new();
    let mut w = Vec::new();
    for i in 0..nu {
        for j in 0..nv {
            c.push(DVec3::new(
                i as f64 * 2.0 + (j % 2) as f64 * 0.4,
                j as f64 * 1.5,
                ((i * 3 + j * 5) % 7) as f64 * 0.3,
            ));
            w.push(1.0 + ((i + 2 * j) % 3) as f64 * 0.5);
        }
    }
    NurbsSurface::new(3, 3, ku, kv, nu, nv, c, w).unwrap()
}

/// 20 Punkte der Kurve gegen die Fläche (Abstand ≤ 1e-12).
fn check_u(s: &NurbsSurface, u: f64) {
    let c = s.iso_curve_u(u).unwrap();
    assert_eq!(c.domain(), s.domain().1);
    for k in 0..20 {
        let v = k as f64 / 19.0;
        let gap = c.point(v).distance(s.point(u, v));
        assert!(gap <= 1e-12, "u = {u}, v = {v}: {gap}");
    }
}

fn check_v(s: &NurbsSurface, v: f64) {
    let c = s.iso_curve_v(v).unwrap();
    assert_eq!(c.domain(), s.domain().0);
    for k in 0..20 {
        let u = k as f64 / 19.0;
        let gap = c.point(u).distance(s.point(u, v));
        assert!(gap <= 1e-12, "v = {v}, u = {u}: {gap}");
    }
}

#[test]
fn iso_curve_matches_surface_points() {
    let s = sample();
    for u in [0.13, 0.37, 0.61, 0.93] {
        check_u(&s, u);
    }
    for v in [0.05, 0.42, 0.88] {
        check_v(&s, v);
    }
}

#[test]
fn iso_at_knot_and_end() {
    let s = sample();
    // Einfacher (0,25) und doppelter (0,5) innerer Knoten, beide Ränder.
    for u in [0.0, 0.25, 0.5, 1.0] {
        check_u(&s, u);
    }
    for v in [0.0, 0.3, 0.7, 1.0] {
        check_v(&s, v);
    }
    // Am Rand ist die Kurve die Randzeile (Gewichte eingeschlossen).
    let c = s.iso_curve_u(0.0).unwrap();
    let (_, nv) = s.counts();
    for j in 0..nv {
        assert_eq!(c.control_points()[j], s.control_at(0, j));
        assert_eq!(c.weights()[j], s.weight_at(0, j));
    }
    // Außerhalb des Bereichs und nicht endlich: Fehler, kein Absturz.
    assert!(s.iso_curve_u(1.5).is_err());
    assert!(s.iso_curve_v(-0.1).is_err());
    assert!(s.iso_curve_u(f64::NAN).is_err());
}
