use planora_kernel::*;
use planora_math::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn counts(s: &Solid) -> (usize, usize, usize, usize, usize) {
    let r = validate(s).expect("Körper muss gültig sein");
    (r.vertices, r.edges, r.faces, r.inner_loops, r.genus)
}

fn chain(start: DVec2, segments: Vec<Segment>) -> ProfileLoop {
    ProfileLoop { start, segments }
}

#[test]
fn sphere_is_valid() {
    let s = k().make_sphere(5.0).unwrap();
    assert_eq!(counts(&s), (2, 1, 1, 0, 0));
    assert!(matches!(
        s.faces.iter().next().unwrap().1.surface,
        Surface::Sphere { .. }
    ));
}

#[test]
fn cone_is_valid() {
    let s = k().make_cone(3.0, 6.0).unwrap();
    assert_eq!(counts(&s), (2, 2, 2, 0, 0));
}

#[test]
fn cylinder_by_revolution_is_valid() {
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![
            Segment::Line {
                to: DVec2::new(2.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(2.0, 5.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, 5.0),
            },
        ],
    );
    let s = k().revolve(&c, &Frame::WORLD).unwrap();
    assert_eq!(counts(&s), (2, 3, 3, 0, 0));
}

#[test]
fn capsule_with_sphere_caps_is_valid() {
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![
            Segment::Arc {
                center: DVec2::new(0.0, 10.0),
                sweep: PI / 2.0,
            },
            Segment::Line {
                to: DVec2::new(10.0, 30.0),
            },
            Segment::Arc {
                center: DVec2::new(0.0, 30.0),
                sweep: PI / 2.0,
            },
        ],
    );
    let s = k().revolve(&c, &Frame::WORLD).unwrap();
    validate(&s).unwrap();
    assert_eq!(s.faces.len(), 3);
}

#[test]
fn puck_with_rounded_edge_uses_a_torus_patch() {
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![
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
    );
    let s = k().revolve(&c, &Frame::WORLD).unwrap();
    validate(&s).unwrap();
    assert!(
        s.faces
            .iter()
            .any(|(_, f)| matches!(f.surface, Surface::Torus { .. }))
    );
}

#[test]
fn clockwise_chain_is_reoriented() {
    let c = chain(
        DVec2::new(0.0, 5.0),
        vec![
            Segment::Line {
                to: DVec2::new(2.0, 5.0),
            },
            Segment::Line {
                to: DVec2::new(2.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, 0.0),
            },
        ],
    );
    let s = k().revolve(&c, &Frame::WORLD).unwrap();
    assert_eq!(counts(&s), (2, 3, 3, 0, 0));
}

#[test]
fn chain_not_starting_on_the_axis_is_rejected() {
    let c = chain(
        DVec2::new(1.0, 0.0),
        vec![
            Segment::Line {
                to: DVec2::new(2.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, 3.0),
            },
        ],
    );
    assert_eq!(
        k().revolve(&c, &Frame::WORLD).unwrap_err(),
        KernelError::ProfileOffAxis
    );
}

#[test]
fn chain_touching_the_axis_in_the_middle_is_rejected() {
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![
            Segment::Line {
                to: DVec2::new(2.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, 2.0),
            },
            Segment::Line {
                to: DVec2::new(2.0, 4.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, 4.0),
            },
        ],
    );
    assert_eq!(
        k().revolve(&c, &Frame::WORLD).unwrap_err(),
        KernelError::ProfileCrossesAxis
    );
}

#[test]
fn zero_area_chain_is_rejected() {
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![Segment::Line {
            to: DVec2::new(0.0, 5.0),
        }],
    );
    assert!(k().revolve(&c, &Frame::WORLD).is_err());
}

#[test]
fn invalid_dimensions_are_rejected_without_panic() {
    for d in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            k().make_sphere(d),
            Err(KernelError::InvalidDimension(_))
        ));
        assert!(matches!(
            k().make_cone(d, 1.0),
            Err(KernelError::InvalidDimension(_))
        ));
        assert!(matches!(
            k().make_cone(1.0, d),
            Err(KernelError::InvalidDimension(_))
        ));
    }
}

#[test]
fn revolution_around_a_tilted_axis_is_valid() {
    let frame = Frame::from_origin_normal(DVec3::new(1.0, 2.0, 3.0), DVec3::new(1.0, 1.0, 0.5));
    let c = chain(
        DVec2::new(0.0, 0.0),
        vec![
            Segment::Line {
                to: DVec2::new(2.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, 4.0),
            },
        ],
    );
    let s = k().revolve(&c, &frame).unwrap();
    assert_eq!(counts(&s), (2, 2, 2, 0, 0));
}

#[test]
fn malformed_chains_return_errors_immediately() {
    let arc = |sweep: f64| {
        chain(
            DVec2::new(0.0, -1.0),
            vec![Segment::Arc {
                center: DVec2::ZERO,
                sweep,
            }],
        )
    };
    for sweep in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 0.0, 7.0] {
        assert!(
            k().revolve(&arc(sweep), &Frame::WORLD).is_err(),
            "sweep {sweep}"
        );
    }
    let empty = chain(DVec2::ZERO, vec![]);
    assert!(k().revolve(&empty, &Frame::WORLD).is_err());
    let nan = chain(
        DVec2::ZERO,
        vec![Segment::Line {
            to: DVec2::new(f64::NAN, 1.0),
        }],
    );
    assert!(k().revolve(&nan, &Frame::WORLD).is_err());
}
