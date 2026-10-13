//! Hautfläche aus Kreisbögen und Strecken (Teilprojekt 8, Task 13).

use planora_math::nurbs::*;
use planora_math::{DVec3, PI};

fn quarter(c: DVec3, x: DVec3, y: DVec3, r: f64) -> SkinSection {
    SkinSection::arc(c, c + x * r, c + y * r).unwrap()
}

/// Querschnitte entlang eines Parameters mit numerischen Ableitungen (Differenzenquotient).
fn sections_with_tangents(
    params: &[f64],
    f: impl Fn(f64) -> SkinSection,
) -> (Vec<SkinSection>, Vec<SkinTangent>) {
    let h = 1e-6;
    let secs: Vec<SkinSection> = params.iter().map(|&x| f(x)).collect();
    let tans = params
        .iter()
        .map(|&x| SkinTangent::between(&f(x - h), &f(x + h), 2.0 * h))
        .collect();
    (secs, tans)
}

#[test]
fn arcs_along_a_line_give_an_exact_cylinder() {
    let params: Vec<f64> = (0..5).map(|i| i as f64).collect();
    let (secs, tans) = sections_with_tangents(&params, |x| {
        quarter(DVec3::new(x, 0.0, 0.0), DVec3::Y, DVec3::Z, 1.5)
    });
    let s = skin(&secs, &tans, &params).unwrap();
    for i in 0..=40 {
        for j in 0..=10 {
            let p = s.point(4.0 * i as f64 / 40.0, j as f64 / 10.0);
            assert!(((p.y * p.y + p.z * p.z).sqrt() - 1.5).abs() < 1e-9, "{p:?}");
        }
    }
    for (k, sec) in secs.iter().enumerate() {
        for j in 0..=8 {
            let v = j as f64 / 8.0;
            assert!(s.point(params[k], v).distance(sec.point(v)) < 1e-12);
        }
    }
}

#[test]
fn arcs_around_an_axis_approximate_a_torus_closely() {
    let n = 33;
    let params: Vec<f64> = (0..n)
        .map(|i| 0.5 * PI * i as f64 / (n - 1) as f64)
        .collect();
    let (secs, tans) = sections_with_tangents(&params, |phi| {
        let radial = DVec3::new(phi.cos(), phi.sin(), 0.0);
        quarter(radial * 5.0, radial, DVec3::Z, 1.0)
    });
    let s = skin(&secs, &tans, &params).unwrap();
    for i in 0..=200 {
        for j in 0..=10 {
            let p = s.point(0.5 * PI * i as f64 / 200.0, j as f64 / 10.0);
            let rho = (p.x * p.x + p.y * p.y).sqrt();
            let tube = ((rho - 5.0).powi(2) + p.z * p.z).sqrt();
            assert!((tube - 1.0).abs() < 1e-6, "{tube}");
        }
    }
}

#[test]
fn line_sections_give_a_ruled_surface_and_boundaries_are_exact() {
    let params = [0.0, 1.0, 2.5, 4.0];
    let (secs, tans) = sections_with_tangents(&params, |x| {
        SkinSection::line(DVec3::new(x, 0.0, 0.0), DVec3::new(x, 1.0, 1.0))
    });
    let s = skin(&secs, &tans, &params).unwrap();
    for i in 0..=20 {
        for j in 0..=4 {
            let p = s.point(4.0 * i as f64 / 20.0, j as f64 / 4.0);
            assert!((p.y - p.z).abs() < 1e-9, "{p:?}");
        }
    }
    let a = s.v_boundary(false).unwrap();
    let b = s.v_boundary(true).unwrap();
    for i in 0..=20 {
        let u = 4.0 * i as f64 / 20.0;
        assert!(a.point(u).distance(s.point(u, 0.0)) < 1e-12);
        assert!(b.point(u).distance(s.point(u, 1.0)) < 1e-12);
    }
}

#[test]
fn hostile_sections_are_rejected() {
    let c = DVec3::ZERO;
    assert!(
        SkinSection::arc(c, DVec3::X, DVec3::Y * 2.0).is_err(),
        "verschiedene Radien"
    );
    assert!(
        SkinSection::arc(c, DVec3::X, -DVec3::X).is_err(),
        "Halbkreis"
    );
    assert!(
        SkinSection::arc(c, DVec3::X, DVec3::X).is_err(),
        "Öffnung null"
    );
    assert!(SkinSection::arc(c, DVec3::new(f64::NAN, 0.0, 0.0), DVec3::Y).is_err());
    let sec = SkinSection::line(DVec3::ZERO, DVec3::X);
    let tan = SkinTangent::between(&sec, &sec, 1.0);
    assert!(skin(&[sec, sec], &[tan, tan], &[0.0]).is_err(), "Anzahl");
    assert!(
        skin(&[sec, sec], &[tan], &[0.0, 1.0]).is_err(),
        "Ableitungen"
    );
    assert!(
        skin(&[sec, sec], &[tan, tan], &[1.0, 0.0]).is_err(),
        "fallend"
    );
    assert!(skin(&[sec], &[tan], &[0.0]).is_err(), "ein Querschnitt");
    // Riesige Gewichtsableitung: negatives Gewicht wird abgelehnt, kein Absturz.
    let arc = SkinSection::arc(c, DVec3::X, DVec3::Y).unwrap();
    let wild = SkinTangent {
        d0: DVec3::ZERO,
        d1: DVec3::ZERO,
        d2: DVec3::ZERO,
        dw: -1e9,
    };
    assert!(skin(&[arc, arc], &[wild, wild], &[0.0, 1.0]).is_err());
}

// ---------------------------------------------------------------------------------------------
// Zusätzliche Tests (über den Plan hinaus): Parametrisierung, Kegel, Fehlerschranke, Grenzen.
// ---------------------------------------------------------------------------------------------

/// Fester Zufallsgenerator (wie in `planora-kernel/tests/nurbs_geometry.rs`).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

#[test]
fn parametrisation_and_orientation_follow_the_documentation() {
    // u läuft längs der Bahn über genau die Parameter, v quer über [0, 1] von p0 nach p2.
    let params = [-2.0, 0.5, 3.0];
    let (secs, tans) = sections_with_tangents(&params, |x| {
        quarter(DVec3::new(x, 0.0, 0.0), DVec3::Y, DVec3::Z, 2.0)
    });
    let s = skin(&secs, &tans, &params).unwrap();
    assert_eq!(s.domain(), ((-2.0, 3.0), (0.0, 1.0)));
    assert_eq!((s.degree_u(), s.degree_v()), (3, 2));
    assert_eq!(s.counts(), (3 * 2 + 1, 3));
    assert_eq!(
        s.knots_u().knots(),
        &[-2.0, -2.0, -2.0, -2.0, 0.5, 0.5, 0.5, 3.0, 3.0, 3.0, 3.0]
    );
    for (k, sec) in secs.iter().enumerate() {
        assert!(s.point(params[k], 0.0).distance(sec.p0) < 1e-12);
        assert!(s.point(params[k], 1.0).distance(sec.p2) < 1e-12);
    }
    // Bogen von +Y nach +Z: die Mitte liegt auf der Winkelhalbierenden.
    let mid = s.point(0.5, 0.5);
    let want = DVec3::new(0.5, 2.0 * 0.5f64.sqrt(), 2.0 * 0.5f64.sqrt());
    assert!(mid.distance(want) < 1e-12, "{mid:?}");
    // Der Querschnitt selbst: v = 0 ist `from`, v = 1 ist `to`, gleichmäßig um die Mitte.
    let sec = secs[0];
    assert!(sec.point(0.0).distance(sec.p0) < 1e-15);
    assert!(sec.point(1.0).distance(sec.p2) < 1e-15);
    assert!(sec.point(-3.0).distance(sec.p0) < 1e-15, "begrenzt");
    assert!(sec.point(7.0).distance(sec.p2) < 1e-15, "begrenzt");
    // Längs steigt u in Richtung der Bahn (+x).
    assert!(s.point(1.0, 0.3).x > s.point(0.0, 0.3).x);
}

#[test]
fn arcs_of_linearly_growing_radius_give_an_exact_cone() {
    // Mittelpunkt auf der x-Achse, Radius r(x) = 1 + 0.2 x: die homogenen Reihen hängen linear
    // von x ab, die Hermite-Interpolation gibt sie exakt wieder, die Fläche ist ein Kegelstück.
    let params = [0.0, 0.7, 2.0, 3.1, 5.0];
    let (secs, tans) = sections_with_tangents(&params, |x| {
        let r = 1.0 + 0.2 * x;
        let (a, b) = (DVec3::new(0.0, 0.6, 0.8), DVec3::new(0.0, -0.8, 0.6));
        let c = DVec3::new(x, 0.0, 0.0);
        SkinSection::arc(c, c + a * r, c + (a * 0.3 + b).normalize() * r).unwrap()
    });
    let s = skin(&secs, &tans, &params).unwrap();
    for i in 0..=50 {
        for j in 0..=10 {
            let p = s.point(5.0 * i as f64 / 50.0, j as f64 / 10.0);
            let r = (p.y * p.y + p.z * p.z).sqrt();
            assert!((r - (1.0 + 0.2 * p.x)).abs() < 1e-9, "{p:?}");
        }
    }
}

#[test]
fn random_tori_stay_within_the_documented_hermite_bound() {
    // Querschnitte um die z-Achse gedreht, Ableitungen exakt (z × Reihe). Dokumentierte Schranke
    // (skin.rs): Fehler der homogenen Reihen ≤ h⁴/384 · max|f⁗|, hier |f⁗| = Abstand der Reihe
    // von der Achse; der Punktfehler ist höchstens dieser Fehler geteilt durch das kleinste
    // Gewicht (`w1`, das Gewicht bleibt bei der Drehung konstant und wird exakt getroffen).
    let mut rng = Lcg(0x5EED_0013);
    for _ in 0..40 {
        let big = rng.range(3.0, 30.0);
        let small = rng.range(0.2, 0.9) * big.min(5.0);
        let sweep = rng.range(0.2, 2.8);
        let tilt = rng.range(-PI, PI);
        let span = rng.range(0.3, 1.9) * PI;
        let n = 4 + (rng.next() * 30.0) as usize;
        let rot = |phi: f64, q: DVec3| {
            let (s, c) = phi.sin_cos();
            DVec3::new(c * q.x - s * q.y, s * q.x + c * q.y, q.z)
        };
        // Querschnitt in der xz-Ebene um (big, 0, 0), von Winkel `tilt` bis `tilt + sweep`.
        let ring = |a: f64| DVec3::new(big + small * a.cos(), 0.0, small * a.sin());
        let base =
            SkinSection::arc(DVec3::new(big, 0.0, 0.0), ring(tilt), ring(tilt + sweep)).unwrap();
        let params: Vec<f64> = (0..n).map(|i| span * i as f64 / (n - 1) as f64).collect();
        let secs: Vec<SkinSection> = params
            .iter()
            .map(|&phi| SkinSection {
                p0: rot(phi, base.p0),
                p1: rot(phi, base.p1),
                p2: rot(phi, base.p2),
                w1: base.w1,
            })
            .collect();
        let tans: Vec<SkinTangent> = secs
            .iter()
            .map(|s| SkinTangent {
                d0: DVec3::Z.cross(s.p0),
                d1: DVec3::Z.cross(s.p1 * s.w1),
                d2: DVec3::Z.cross(s.p2),
                dw: 0.0,
            })
            .collect();
        let s = skin(&secs, &tans, &params).unwrap();
        let h = span / (n - 1) as f64;
        let axis = |p: DVec3| (p.x * p.x + p.y * p.y).sqrt();
        let fmax = axis(base.p0)
            .max(axis(base.p2))
            .max(axis(base.p1) * base.w1);
        let bound = h.powi(4) / 384.0 * fmax / base.w1.min(1.0);
        let mut worst: f64 = 0.0;
        for i in 0..=120 {
            for j in 0..=8 {
                let p = s.point(span * i as f64 / 120.0, j as f64 / 8.0);
                let tube = ((axis(p) - big).powi(2) + p.z * p.z).sqrt();
                worst = worst.max((tube - small).abs());
            }
        }
        // Rundungsrauschen der Auswertung (Beträge bis etwa 40 mm) auf die Schranke.
        assert!(
            worst <= bound + 1e-12,
            "Fehler {worst} über Schranke {bound} (R {big}, r {small}, n {n})"
        );
        // An den Stationen exakt.
        for (k, sec) in secs.iter().enumerate() {
            for j in 0..=4 {
                let v = j as f64 / 4.0;
                assert!(s.point(params[k], v).distance(sec.point(v)) < 1e-11);
            }
        }
    }
}

#[test]
fn the_skin_is_tangent_continuous_across_stations() {
    let n = 7;
    let params: Vec<f64> = (0..n)
        .map(|i| 0.3 * PI * i as f64 / (n - 1) as f64)
        .collect();
    let (secs, tans) = sections_with_tangents(&params, |phi| {
        let radial = DVec3::new(phi.cos(), phi.sin(), 0.0);
        quarter(radial * (4.0 + phi), radial, DVec3::Z, 1.0 + 0.3 * phi)
    });
    let s = skin(&secs, &tans, &params).unwrap();
    let eps = 1e-7;
    for &x in &params[1..n - 1] {
        for j in 0..=4 {
            let v = j as f64 / 4.0;
            let left = s.derivs(x - eps, v, 1)[1][0];
            let right = s.derivs(x + eps, v, 1)[1][0];
            assert!(
                left.distance(right) < 1e-5 * left.length().max(1.0),
                "{left:?} {right:?}"
            );
        }
    }
}

#[test]
fn boundaries_of_an_arc_skin_are_exact_curves() {
    let params = [0.0, 1.0, 2.0];
    let (secs, tans) = sections_with_tangents(&params, |phi| {
        let radial = DVec3::new(phi.cos(), phi.sin(), 0.0);
        quarter(radial * 5.0, radial, DVec3::Z, 1.0)
    });
    let s = skin(&secs, &tans, &params).unwrap();
    let a = s.v_boundary(false).unwrap();
    let b = s.v_boundary(true).unwrap();
    assert_eq!(a.degree(), 3);
    for i in 0..=40 {
        let u = 2.0 * i as f64 / 40.0;
        assert!(a.point(u).distance(s.point(u, 0.0)) < 1e-12);
        assert!(b.point(u).distance(s.point(u, 1.0)) < 1e-12);
    }
    assert!(a.point(1.0).distance(secs[1].p0) < 1e-12);
    assert!(b.point(2.0).distance(secs[2].p2) < 1e-12);
}

#[test]
fn hostile_numbers_and_limits_are_rejected_by_name() {
    let c = DVec3::ZERO;
    // Radius null und unendliche/riesige Eingaben.
    assert!(SkinSection::arc(c, c, c).is_err(), "Radius null");
    assert!(SkinSection::arc(c, DVec3::X * 1e-300, DVec3::Y * 1e-300).is_err());
    assert!(SkinSection::arc(c, DVec3::new(f64::INFINITY, 0.0, 0.0), DVec3::Y).is_err());
    assert!(SkinSection::arc(c, DVec3::X * 1e300, DVec3::Y * 1e300).is_err());
    assert!(SkinSection::arc(DVec3::splat(f64::NAN), DVec3::X, DVec3::Y).is_err());
    // Fast ein Halbkreis: knapp unter der Grenze erlaubt, darüber nicht.
    let at = |a: f64| DVec3::new(a.cos(), a.sin(), 0.0);
    assert!(SkinSection::arc(c, DVec3::X, at(MAX_SECTION_SWEEP - 1e-4)).is_ok());
    assert!(SkinSection::arc(c, DVec3::X, at(MAX_SECTION_SWEEP + 5e-7)).is_err());
    let line = SkinSection::line(DVec3::ZERO, DVec3::X);
    let still = SkinTangent::between(&line, &line, 1.0);
    // Nicht endliche oder zu dichte Parameter.
    for params in [
        [0.0, f64::NAN],
        [f64::NEG_INFINITY, 1.0],
        [0.0, 0.0],
        [0.0, 1e-9],
        [0.0, 1e12],
    ] {
        assert!(
            skin(&[line, line], &[still, still], &params).is_err(),
            "{params:?}"
        );
    }
    assert!(
        skin(&[line; 3], &[still; 3], &[0.0, 1e-12, 1.0]).is_err(),
        "Knoten zu nah"
    );
    // Nicht endliche oder riesige Ableitungen und Querschnitte.
    let nan = SkinTangent {
        d0: DVec3::splat(f64::NAN),
        ..still
    };
    assert!(skin(&[line, line], &[nan, still], &[0.0, 1.0]).is_err());
    let huge = SkinTangent {
        d2: DVec3::splat(1e300),
        ..still
    };
    assert!(skin(&[line, line], &[huge, still], &[0.0, 1.0]).is_err());
    let far = SkinSection::line(DVec3::splat(1e300), DVec3::splat(2e300));
    assert!(skin(&[far, far], &[still, still], &[0.0, 1.0]).is_err());
    let bad_weight = SkinSection {
        w1: f64::NAN,
        ..line
    };
    assert!(skin(&[bad_weight, line], &[still, still], &[0.0, 1.0]).is_err());
    let neg_weight = SkinSection { w1: -0.5, ..line };
    assert!(skin(&[neg_weight, line], &[still, still], &[0.0, 1.0]).is_err());
    // Querschnitt der Länge null (zusammengefallene Enden).
    let dot = SkinSection::line(DVec3::X, DVec3::X);
    assert!(
        matches!(
            skin(&[dot, line], &[still, still], &[0.0, 1.0]),
            Err(NurbsError::Input(_))
        ),
        "Länge null"
    );
    // Zu viele Querschnitte: die Grenze selbst geht, eine Station mehr nicht.
    let most = (MAX_CONTROL_POINTS - 1) / 3 + 1;
    let many = |m: usize| {
        let params: Vec<f64> = (0..m).map(|i| i as f64).collect();
        let secs: Vec<SkinSection> = params
            .iter()
            .map(|&x| SkinSection::line(DVec3::new(x, 0.0, 0.0), DVec3::new(x, 1.0, 0.0)))
            .collect();
        let tans = vec![
            SkinTangent {
                d0: DVec3::X,
                d1: DVec3::X,
                d2: DVec3::X,
                dw: 0.0
            };
            m
        ];
        skin(&secs, &tans, &params)
    };
    assert!(many(most).is_ok());
    assert!(matches!(many(most + 1), Err(NurbsError::TooLarge(_))));
}
