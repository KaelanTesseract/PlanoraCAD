//! Eigenschaftstests (Spec §8): zufällige Grundkörper und zufällige Ähnlichkeitstransformationen.

use planora_kernel::*;
use planora_math::*;
use planora_mesh::*;
use proptest::prelude::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn opts() -> TessellationOptions {
    TessellationOptions {
        chord_tolerance: 0.001,
    }
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn puck() -> ProfileLoop {
    ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(3.0, 0.0),
            },
            Segment::Arc {
                center: DVec2::new(3.0, 1.0),
                sweep: PI / 2.0,
            },
            Segment::Line {
                to: DVec2::new(0.0, 1.0),
            },
        ],
    }
}

/// Grundkörper Nummer `i` (0 Quader, 1 Zylinder, 2 Kugel, 3 Kegel, 4 Rundscheibe).
fn primitive(i: usize) -> Solid {
    match i {
        0 => k().make_box(DVec3::new(2.0, 3.0, 4.0)),
        1 => k().make_cylinder(2.0, 5.0),
        2 => k().make_sphere(3.0),
        3 => k().make_cone(2.0, 4.0),
        _ => k().revolve(&puck(), &Frame::WORLD),
    }
    .unwrap()
}

/// Prüft Gültigkeit, Dichtheit und Volumen gegen die geschlossene Formel.
fn check_volume(solid: &Solid, expected: f64, bound: f64) -> Result<(), TestCaseError> {
    prop_assert!(k().validate(solid).is_ok());
    let mesh = tessellate(solid, &opts()).unwrap();
    prop_assert!(is_watertight(&mesh, 1e-6));
    let v = mass_properties(&mesh).volume;
    prop_assert!(rel(v, expected) < bound, "{} vs {}", v, expected);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn random_boxes_have_exact_volume(
        x in 0.5f64..50.0, y in 0.5f64..50.0, z in 0.5f64..50.0,
    ) {
        let s = k().make_box(DVec3::new(x, y, z)).unwrap();
        check_volume(&s, x * y * z, 1e-9)?;
    }

    #[test]
    fn random_cylinders_match_the_formula(r in 0.5f64..50.0, h in 0.5f64..50.0) {
        let s = k().make_cylinder(r, h).unwrap();
        check_volume(&s, PI * r * r * h, 0.005)?;
    }


    #[test]
    fn random_cones_match_the_formula(r in 0.5f64..50.0, h in 0.5f64..50.0) {
        let s = k().make_cone(r, h).unwrap();
        check_volume(&s, PI * r * r * h / 3.0, 0.005)?;
    }

    /// Drehen, Verschieben, gleichmäßig Skalieren und optional Spiegeln an allen Grundkörpern.
    #[test]
    fn random_similarities_keep_bodies_valid_and_volume_scaled(
        which in 0usize..5,
        phi in 0.0f64..TAU, cos_theta in -1.0f64..1.0,
        angle in 0.0f64..TAU,
        tx in -100.0f64..100.0, ty in -100.0f64..100.0, tz in -100.0f64..100.0,
        scale in 0.25f64..4.0,
        mirror in 0usize..4,
    ) {
        let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
        let axis = DVec3::new(sin_theta * phi.cos(), sin_theta * phi.sin(), cos_theta);
        let mut factors = DVec3::splat(scale);
        if mirror > 0 {
            factors[mirror - 1] = -scale;
        }
        let m = DMat4::from_translation(DVec3::new(tx, ty, tz))
            * DMat4::from_axis_angle(axis, angle)
            * DMat4::from_scale(factors);

        let solid = primitive(which);
        let original = mass_properties(&tessellate(&solid, &opts()).unwrap()).volume;
        let t = k().transform(&solid, &m).unwrap();
        prop_assert!(k().validate(&t).is_ok());
        let mesh = tessellate(&t, &opts()).unwrap();
        prop_assert!(is_watertight(&mesh, 1e-6));
        let v = mass_properties(&mesh).volume;
        let expected = original * m.determinant().abs();
        prop_assert!(v > 0.0, "Volumen nicht positiv: {}", v);
        prop_assert!(rel(v, expected) < 0.01, "{} vs {}", v, expected);
    }
}

proptest! {
    // Große Kugeln ergeben bei 0,001 mm Sehnentoleranz viele Dreiecke; daher weniger Fälle.
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Der relative Volumenfehler der Kugel liegt bei etwa 3,3 · Toleranz / Radius (r = 0,5:
    /// 0,66 %), deshalb beginnt der Bereich bei 1 mm, damit die 0,5-%-Schranke gültig bleibt.
    #[test]
    fn random_spheres_match_the_formula(r in 1.0f64..50.0) {
        let s = k().make_sphere(r).unwrap();
        check_volume(&s, 4.0 / 3.0 * PI * r * r * r, 0.005)?;
    }
}
