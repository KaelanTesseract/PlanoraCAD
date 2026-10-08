use planora_kernel::*;
use planora_math::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn all_primitives() -> Vec<Solid> {
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

fn matrices() -> Vec<DMat4> {
    vec![
        DMat4::from_translation(DVec3::new(10.0, -5.0, 2.0)),
        DMat4::from_axis_angle(DVec3::new(1.0, 2.0, 3.0).normalize(), 0.9),
        DMat4::from_scale(DVec3::splat(2.5)),
        DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)),
        DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)),
        DMat4::from_translation(DVec3::new(1.0, 2.0, 3.0))
            * DMat4::from_axis_angle(DVec3::Y, 1.1)
            * DMat4::from_scale(DVec3::new(-2.0, 2.0, 2.0)),
    ]
}

#[test]
fn every_primitive_stays_valid_under_every_similarity() {
    for solid in all_primitives() {
        let before = validate(&solid).unwrap();
        for m in matrices() {
            let t = k().transform(&solid, &m).unwrap();
            let after = validate(&t).unwrap();
            assert_eq!(before, after);
        }
    }
}

#[test]
fn vertices_move_with_the_matrix() {
    let solid = k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap();
    let m =
        DMat4::from_translation(DVec3::new(1.0, 2.0, 3.0)) * DMat4::from_axis_angle(DVec3::Z, 0.5);
    let t = k().transform(&solid, &m).unwrap();
    for ((_, a), (_, b)) in solid.vertices.iter().zip(t.vertices.iter()) {
        assert!(m.transform_point3(a.point).distance(b.point) < 1e-12);
    }
}

#[test]
fn identity_is_a_valid_similarity() {
    let solid = k().make_cylinder(1.0, 2.0).unwrap();
    assert!(k().transform(&solid, &DMat4::IDENTITY).is_ok());
}

#[test]
fn non_similarity_matrices_are_rejected() {
    let solid = k().make_box(DVec3::ONE).unwrap();
    let bad = [
        DMat4::from_scale(DVec3::new(1.0, 2.0, 1.0)),
        DMat4::from_scale(DVec3::ZERO),
        DMat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, //
            0.5, 1.0, 0.0, 0.0, // Scherung
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ]),
        DMat4::from_scale(DVec3::splat(f64::NAN)),
    ];
    for m in bad {
        assert_eq!(
            k().transform(&solid, &m).unwrap_err(),
            KernelError::NonSimilarityTransform
        );
    }
}
