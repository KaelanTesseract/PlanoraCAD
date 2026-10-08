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

fn solids() -> Vec<Solid> {
    let puck = ProfileLoop {
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
    };
    vec![
        k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap(),
        k().make_cylinder(2.0, 5.0).unwrap(),
        k().make_sphere(3.0).unwrap(),
        k().make_cone(2.0, 4.0).unwrap(),
        k().revolve(&puck, &Frame::WORLD).unwrap(),
    ]
}

#[test]
fn mirrored_bodies_keep_positive_volume_and_closed_meshes() {
    let mirrors = [
        DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)),
        DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)),
        DMat4::from_translation(DVec3::new(1.0, 2.0, 3.0))
            * DMat4::from_axis_angle(DVec3::Y, 1.1)
            * DMat4::from_scale(DVec3::new(-2.0, 2.0, 2.0)),
    ];
    for solid in solids() {
        let original = mass_properties(&tessellate(&solid, &opts()).unwrap());
        for m in mirrors {
            let t = k().transform(&solid, &m).unwrap();
            let mesh = tessellate(&t, &opts()).unwrap();
            let p = mass_properties(&mesh);
            let scale = m.determinant().abs();
            assert!(p.volume > 0.0, "Volumen negativ: Orientierung falsch");
            assert!(
                ((p.volume - original.volume * scale) / (original.volume * scale)).abs() < 0.01,
                "{} vs {}",
                p.volume,
                original.volume * scale
            );
            assert!(is_watertight(&mesh, 1e-6));
        }
    }
}

#[test]
fn rotated_and_translated_bodies_keep_volume() {
    let m = DMat4::from_translation(DVec3::new(10.0, -5.0, 2.0))
        * DMat4::from_axis_angle(DVec3::new(1.0, 2.0, 3.0).normalize(), 0.9);
    for solid in solids() {
        let original = mass_properties(&tessellate(&solid, &opts()).unwrap());
        let t = k().transform(&solid, &m).unwrap();
        let mesh = tessellate(&t, &opts()).unwrap();
        let p = mass_properties(&mesh);
        assert!(((p.volume - original.volume) / original.volume).abs() < 1e-6);
        assert!(is_watertight(&mesh, 1e-6));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn rigid_transforms_preserve_box_volume(
        sx in 0.1f64..50.0, sy in 0.1f64..50.0, sz in 0.1f64..50.0,
        ax in -1.0f64..1.0, ay in -1.0f64..1.0, az in 0.2f64..1.0,
        angle in 0.0f64..TAU,
        tx in -100.0f64..100.0, ty in -100.0f64..100.0, tz in -100.0f64..100.0,
    ) {
        let axis = DVec3::new(ax, ay, az).normalize();
        let m = DMat4::from_translation(DVec3::new(tx, ty, tz)) * DMat4::from_axis_angle(axis, angle);
        let solid = k().make_box(DVec3::new(sx, sy, sz)).unwrap();
        let t = k().transform(&solid, &m).unwrap();
        prop_assert!(k().validate(&t).is_ok());
        let mesh = tessellate(&t, &opts()).unwrap();
        let p = mass_properties(&mesh);
        let expected = sx * sy * sz;
        prop_assert!(((p.volume - expected) / expected).abs() < 1e-6);
        prop_assert!(is_watertight(&mesh, 1e-6));
    }
}
