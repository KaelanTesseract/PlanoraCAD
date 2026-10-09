use planora_kernel::*;
use planora_math::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn counts(s: &Solid) -> (usize, usize, usize, usize, usize) {
    let r = validate(s).expect("Körper muss gültig sein");
    (r.vertices, r.edges, r.faces, r.inner_loops, r.genus)
}

fn rect_profile(w: f64, h: f64) -> Profile {
    Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(w, h)),
        holes: vec![],
    }
}

#[test]
fn box_is_valid_genus_zero() {
    let s = k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap();
    assert_eq!(counts(&s), (8, 12, 6, 0, 0));
}

#[test]
fn box_face_normals_point_outward() {
    let s = k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap();
    let center = DVec3::new(1.0, 1.5, 2.0);
    for (_, f) in s.faces.iter() {
        let Surface::Plane { frame } = &f.surface else {
            panic!("Quader hat nur ebene Flächen")
        };
        let n = if f.same_sense { frame.z } else { -frame.z };
        assert!(
            n.dot(frame.origin - center) > 0.0,
            "Normale zeigt nach innen"
        );
    }
}

#[test]
fn cylinder_is_valid_genus_zero() {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    assert_eq!(counts(&s), (2, 3, 3, 0, 0));
}

#[test]
fn plate_with_round_hole_has_genus_one() {
    let p = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0)],
    };
    let s = k().extrude(&p, &Frame::WORLD, 5.0).unwrap();
    assert_eq!(counts(&s), (10, 15, 7, 2, 1));
}

#[test]
fn plate_with_two_holes_has_genus_two() {
    let p = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(40.0, 20.0)),
        holes: vec![
            ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0),
            ProfileLoop::circle(DVec2::new(30.0, 10.0), 3.0),
        ],
    };
    let s = k().extrude(&p, &Frame::WORLD, 5.0).unwrap();
    assert_eq!(counts(&s).4, 2);
}

#[test]
fn clockwise_outer_and_ccw_hole_are_fixed_automatically() {
    let p = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)).reversed(),
        holes: vec![ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0)],
    };
    let s = k().extrude(&p, &Frame::WORLD, 5.0).unwrap();
    assert_eq!(counts(&s), (10, 15, 7, 2, 1));
}

#[test]
fn concave_l_shape_is_valid() {
    let p = Profile {
        outer: ProfileLoop::polygon(&[
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 1.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 3.0),
            DVec2::new(0.0, 3.0),
        ])
        .unwrap(),
        holes: vec![],
    };
    let s = k().extrude(&p, &Frame::WORLD, 2.0).unwrap();
    assert_eq!(counts(&s), (12, 18, 8, 0, 0));
}

#[test]
fn notched_bar_with_concave_arc_is_valid() {
    let outer = ProfileLoop {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            Segment::Line {
                to: DVec2::new(4.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(4.0, 2.0),
            },
            Segment::Line {
                to: DVec2::new(3.0, 2.0),
            },
            Segment::Arc {
                center: DVec2::new(2.0, 2.0),
                sweep: -PI,
            },
            Segment::Line {
                to: DVec2::new(0.0, 2.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, 0.0),
            },
        ],
    };
    let s = k()
        .extrude(
            &Profile {
                outer,
                holes: vec![],
            },
            &Frame::WORLD,
            3.0,
        )
        .unwrap();
    assert_eq!(counts(&s), (12, 18, 8, 0, 0));
}

#[test]
fn extrusion_in_a_tilted_frame_is_valid() {
    let frame = Frame::from_origin_normal(DVec3::new(5.0, 6.0, 7.0), DVec3::new(1.0, 2.0, 3.0));
    let s = k().extrude(&rect_profile(2.0, 3.0), &frame, 4.0).unwrap();
    assert_eq!(counts(&s), (8, 12, 6, 0, 0));
}

#[test]
fn invalid_dimensions_are_rejected_without_panic() {
    let bad = [0.0, -1.0, f64::NAN, f64::INFINITY * 0.0];
    for d in bad {
        assert!(matches!(
            k().make_box(DVec3::new(d, 1.0, 1.0)),
            Err(KernelError::InvalidDimension(_))
        ));
        assert!(matches!(
            k().make_cylinder(d, 1.0),
            Err(KernelError::InvalidDimension(_))
        ));
        assert!(matches!(
            k().make_cylinder(1.0, d),
            Err(KernelError::InvalidDimension(_))
        ));
        assert!(matches!(
            k().extrude(&rect_profile(1.0, 1.0), &Frame::WORLD, d),
            Err(KernelError::InvalidDimension(_))
        ));
    }
}

#[test]
fn zero_area_profile_is_rejected() {
    let r = k().extrude(&rect_profile(0.0, 5.0), &Frame::WORLD, 1.0);
    assert!(matches!(
        r,
        Err(KernelError::ZeroAreaProfile) | Err(KernelError::DegenerateSegment)
    ));
}

#[test]
fn open_profile_is_rejected() {
    let open = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(1.0, 0.0),
                },
                Segment::Line {
                    to: DVec2::new(1.0, 1.0),
                },
            ],
        },
        holes: vec![],
    };
    assert_eq!(
        k().extrude(&open, &Frame::WORLD, 1.0).unwrap_err(),
        KernelError::ProfileNotClosed
    );
}

#[test]
fn validator_detects_a_flipped_half_edge() {
    let mut s = k().make_box(DVec3::ONE).unwrap();
    let id = Id::<Face>::from_index(0);
    let he = &mut s.faces[id].outer.half_edges[0];
    he.forward = !he.forward;
    assert!(validate(&s).is_err());
}

fn arc_profile(sweep: f64) -> Profile {
    Profile {
        outer: ProfileLoop {
            start: DVec2::new(1.0, 0.0),
            segments: vec![Segment::Arc {
                center: DVec2::ZERO,
                sweep,
            }],
        },
        holes: vec![],
    }
}

#[test]
fn non_finite_or_oversized_arc_sweeps_are_rejected() {
    for sweep in [
        f64::INFINITY,
        f64::NEG_INFINITY,
        1e10,
        f64::NAN,
        2.0 * TAU,
        -2.0 * TAU,
    ] {
        assert!(
            k().extrude(&arc_profile(sweep), &Frame::WORLD, 1.0)
                .is_err(),
            "sweep {sweep} muss abgelehnt werden"
        );
    }
}

#[test]
fn empty_hole_is_rejected() {
    let p = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(10.0, 10.0)),
        holes: vec![ProfileLoop {
            start: DVec2::new(5.0, 5.0),
            segments: vec![],
        }],
    };
    assert!(k().extrude(&p, &Frame::WORLD, 1.0).is_err());
}

#[test]
fn out_and_back_hole_is_rejected() {
    let p = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(10.0, 10.0)),
        holes: vec![ProfileLoop {
            start: DVec2::new(2.0, 2.0),
            segments: vec![
                Segment::Line {
                    to: DVec2::new(5.0, 5.0),
                },
                Segment::Line {
                    to: DVec2::new(2.0, 2.0),
                },
            ],
        }],
    };
    assert!(k().extrude(&p, &Frame::WORLD, 1.0).is_err());
}

#[test]
fn hole_with_infinite_coordinates_is_rejected() {
    let p = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(10.0, 10.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(5.0, f64::INFINITY), 1.0)],
    };
    assert!(k().extrude(&p, &Frame::WORLD, 1.0).is_err());
}

#[test]
fn infinite_dimensions_are_rejected() {
    let inf = f64::INFINITY;
    assert!(matches!(
        k().make_box(DVec3::new(inf, 1.0, 1.0)),
        Err(KernelError::InvalidDimension(_))
    ));
    assert!(matches!(
        k().make_cylinder(inf, 1.0),
        Err(KernelError::InvalidDimension(_))
    ));
    assert!(matches!(
        k().make_cylinder(1.0, inf),
        Err(KernelError::InvalidDimension(_))
    ));
    assert!(matches!(
        k().extrude(&rect_profile(1.0, 1.0), &Frame::WORLD, inf),
        Err(KernelError::InvalidDimension(_))
    ));
}
