use planora_math::nurbs::*;
use planora_math::{DVec3, PI, TAU};

#[test]
fn a_full_circle_is_exact() {
    let c = NurbsCurve::elliptic_arc(
        DVec3::new(1.0, 2.0, 3.0),
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(0.0, 5.0, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    let (a, b) = c.domain();
    for i in 0..=200 {
        let p = c.point(a + (b - a) * i as f64 / 200.0);
        let r = (p - DVec3::new(1.0, 2.0, 3.0)).length();
        assert!((r - 5.0).abs() < 1e-12, "{r}");
        assert!((p.z - 3.0).abs() < 1e-13);
    }
    assert!(c.is_closed(1e-12));
}

#[test]
fn an_ellipse_arc_matches_the_parametric_form_at_its_own_angle() {
    let (a, b) = (DVec3::new(4.0, 0.0, 0.0), DVec3::new(0.0, 2.0, 0.0));
    let c = NurbsCurve::elliptic_arc(DVec3::ZERO, a, b, 0.3, 2.0).unwrap();
    // Alle Punkte liegen auf der Ellipse (x/4)^2 + (y/2)^2 = 1; Anfang und Ende stimmen.
    let (u0, u1) = c.domain();
    for i in 0..=100 {
        let p = c.point(u0 + (u1 - u0) * i as f64 / 100.0);
        assert!(((p.x / 4.0).powi(2) + (p.y / 2.0).powi(2) - 1.0).abs() < 1e-12);
    }
    assert!(c.point(u0).distance(a * 0.3f64.cos() + b * 0.3f64.sin()) < 1e-12);
    assert!(c.point(u1).distance(a * 2.3f64.cos() + b * 2.3f64.sin()) < 1e-12);
    assert!(
        NurbsCurve::elliptic_arc(DVec3::ZERO, a, a, 0.0, 1.0).is_err(),
        "parallele Achsen"
    );
    assert!(
        NurbsCurve::elliptic_arc(DVec3::ZERO, a, b, 0.0, 7.0).is_err(),
        "mehr als ein Umlauf"
    );
    assert!(NurbsCurve::elliptic_arc(DVec3::ZERO, a, b, 0.0, f64::NAN).is_err());
}

#[test]
fn the_natural_cubic_interpolates_and_has_zero_end_curvature() {
    let pts = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(1.0, 2.0, 0.0),
        DVec3::new(2.5, 1.0, 1.0),
        DVec3::new(4.0, 3.0, 0.5),
        DVec3::new(5.0, 0.0, 0.0),
    ];
    let params = [0.0, 1.0, 2.0, 3.0, 4.0];
    let c = NurbsCurve::natural_cubic(&pts, &params).unwrap();
    for (p, t) in pts.iter().zip(&params) {
        assert!(c.point(*t).distance(*p) < 1e-10, "t={t}");
    }
    let (u0, u1) = c.domain();
    assert!((u0 - 0.0).abs() < 1e-15 && (u1 - 4.0).abs() < 1e-15);
    assert!(c.derivs(u0, 2)[2].length() < 1e-9);
    assert!(c.derivs(u1, 2)[2].length() < 1e-9);
    assert!(
        NurbsCurve::natural_cubic(&pts, &[0.0, 1.0, 1.0, 3.0, 4.0]).is_err(),
        "nicht steigend"
    );
    assert!(NurbsCurve::natural_cubic(&pts[..1], &params[..1]).is_err());
}

#[test]
fn fit_keeps_the_error_bound() {
    // Punkte auf einer Schraubenlinie
    let pts: Vec<DVec3> = (0..400)
        .map(|i| {
            let t = i as f64 * 0.05;
            DVec3::new(3.0 * t.cos(), 3.0 * t.sin(), 0.2 * t)
        })
        .collect();
    let tol = 1e-5;
    let c = NurbsCurve::fit(&pts, tol).unwrap();
    for p in &pts {
        assert!(c.distance_to(*p) <= tol * 1.01, "{}", c.distance_to(*p));
    }
    assert!(
        c.control_points().len() < pts.len(),
        "es wird wirklich genähert"
    );
    assert!(NurbsCurve::fit(&pts[..1], tol).is_err());
    assert!(
        NurbsCurve::fit(&[DVec3::ZERO, DVec3::ZERO, DVec3::ZERO], tol).is_err(),
        "entartet"
    );
    assert!(
        NurbsCurve::fit(
            &[DVec3::ZERO, DVec3::new(f64::NAN, 0.0, 0.0), DVec3::X],
            tol
        )
        .is_err()
    );
}

#[test]
fn extrusion_translates_the_curve() {
    let c = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(2.0, 0.0, 0.0),
        DVec3::new(0.0, 1.0, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    let s = NurbsSurface::extrude(&c, DVec3::new(0.0, 0.0, 7.0)).unwrap();
    let ((u0, u1), (v0, v1)) = s.domain();
    for i in 0..=20 {
        let u = u0 + (u1 - u0) * i as f64 / 20.0;
        for v in [v0, 0.5 * (v0 + v1), v1] {
            let p = s.point(u, v);
            let q = c.point(u) + DVec3::new(0.0, 0.0, 7.0 * (v - v0) / (v1 - v0));
            assert!(p.distance(q) < 1e-12);
        }
    }
}

#[test]
fn revolution_of_a_line_gives_a_cylinder_and_of_a_half_circle_a_sphere() {
    // Gerade parallel zur Achse im Abstand 3 -> Zylinder r = 3
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![DVec3::new(3.0, 0.0, 0.0), DVec3::new(3.0, 0.0, 5.0)],
        vec![1.0; 2],
    )
    .unwrap();
    let cyl = NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    let ((u0, u1), (v0, v1)) = cyl.domain();
    for i in 0..=10 {
        for j in 0..=40 {
            let p = cyl.point(
                u0 + (u1 - u0) * i as f64 / 10.0,
                v0 + (v1 - v0) * j as f64 / 40.0,
            );
            assert!(((p.x * p.x + p.y * p.y).sqrt() - 3.0).abs() < 1e-12);
        }
    }
    // Halbkreis in der xz-Ebene (Achse z) -> Kugel r = 2
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, 2.0),
        DVec3::new(2.0, 0.0, 0.0),
        0.0,
        PI,
    )
    .unwrap();
    let sph = NurbsSurface::revolve(&half, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    let ((u0, u1), (v0, v1)) = sph.domain();
    for i in 0..=20 {
        for j in 0..=40 {
            let p = sph.point(
                u0 + (u1 - u0) * i as f64 / 20.0,
                v0 + (v1 - v0) * j as f64 / 40.0,
            );
            assert!((p.length() - 2.0).abs() < 1e-12, "{}", p.length());
        }
    }
    assert!(
        NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::ZERO, 0.0, TAU).is_err(),
        "Achse ohne Richtung"
    );
    assert!(NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, 8.0).is_err());
}

// --- Nachbesserung 1 (Review) ---

fn wiggly(n: usize) -> Vec<DVec3> {
    (0..n)
        .map(|i| {
            let x = i as f64;
            DVec3::new(
                x + (1.7 * x).sin(),
                3.0 * (2.3 * x).cos(),
                2.0 * (0.9 * x).sin(),
            )
        })
        .collect()
}

#[test]
fn fit_survives_a_failing_least_squares_step() {
    let tol = 1e-6;
    for n in [33, 65, 66, 67, 68, 69, 70] {
        let pts = wiggly(n);
        let c = NurbsCurve::fit(&pts, tol).unwrap_or_else(|e| panic!("n={n}: {e}"));
        for p in &pts {
            let d = c.distance_to(*p);
            assert!(d <= tol, "n={n}: {d}");
        }
    }
}

/// Abstand per dichter Probe (obere Schranke des wahren Abstands).
fn brute_distance(c: &NurbsCurve, p: DVec3, samples: usize) -> f64 {
    let (a, b) = c.domain();
    (0..=samples)
        .map(|i| c.point(a + (b - a) * i as f64 / samples as f64).distance(p))
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn distance_to_finds_the_minimum_behind_the_seam() {
    let circle =
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * 5.0, DVec3::Y * 5.0, 0.0, TAU).unwrap();
    let p = DVec3::new(3.70582, -0.054275, -3.40160);
    let d = circle.distance_to(p);
    let exact = ((DVec3::new(p.x, p.y, 0.0).length() - 5.0).powi(2) + p.z * p.z).sqrt();
    assert!((d - exact).abs() < 1e-9, "{d} {exact}");
    assert!(d <= brute_distance(&circle, p, 200_000) + 1e-9);

    let ellipse =
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * 100.0, DVec3::Y, 0.0, TAU).unwrap();
    let p = DVec3::new(101.779, -2.720, 0.180);
    let d = ellipse.distance_to(p);
    let brute = brute_distance(&ellipse, p, 200_000);
    assert!(d <= brute + 1e-9 && d >= brute - 1e-6, "{d} {brute}");
}

#[test]
fn distance_to_matches_brute_force_on_a_zigzag_spline() {
    let pts: Vec<DVec3> = (0..12)
        .map(|i| {
            DVec3::new(
                i as f64,
                if i % 2 == 0 { 0.0 } else { 5.0 },
                ((i * 7) % 3) as f64 - 1.0,
            )
        })
        .collect();
    let params: Vec<f64> = (0..12).map(|i| i as f64).collect();
    let c = NurbsCurve::natural_cubic(&pts, &params).unwrap();
    let mut probes = vec![DVec3::new(3.9969, 4.8030, -0.9258)];
    let mut s: u64 = 12345;
    for _ in 0..40 {
        let mut r = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        probes.push(DVec3::new(
            -1.0 + 13.0 * r(),
            -2.0 + 9.0 * r(),
            -3.0 + 6.0 * r(),
        ));
    }
    for p in probes {
        let d = c.distance_to(p);
        let brute = brute_distance(&c, p, 50_000);
        assert!(d <= brute + 1e-9, "{p}: {d} > {brute}");
    }
}
