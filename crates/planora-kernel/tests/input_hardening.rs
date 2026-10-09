use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, TAU, Tolerance};
use std::time::{Duration, Instant};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

#[test]
fn polygon_with_too_few_or_bad_points_is_an_error_not_a_panic() {
    assert_eq!(
        ProfileLoop::polygon(&[DVec2::ZERO, DVec2::X]),
        Err(KernelError::DegenerateSegment)
    );
    assert_eq!(
        ProfileLoop::polygon(&[]),
        Err(KernelError::DegenerateSegment)
    );
    assert_eq!(
        ProfileLoop::polygon(&[DVec2::ZERO, DVec2::X, DVec2::new(f64::NAN, 1.0)]),
        Err(KernelError::DegenerateSegment)
    );
    assert!(ProfileLoop::polygon(&[DVec2::ZERO, DVec2::X, DVec2::Y]).is_ok());
}

#[test]
fn sampling_huge_or_non_finite_sweeps_terminates_quickly() {
    let start = Instant::now();
    for sweep in [1e300, f64::INFINITY, f64::NAN, -1e18] {
        let lp = ProfileLoop {
            start: DVec2::new(1.0, 0.0),
            segments: vec![Segment::Arc {
                center: DVec2::ZERO,
                sweep,
            }],
        };
        let pts = lp.sample(64);
        assert!(pts.len() <= 70_000, "zu viele Punkte: {}", pts.len());
        let _ = lp.signed_area();
        assert!(lp.sample(usize::MAX).len() <= 70_000);
        assert!(lp.sample(0).len() >= 2);
    }
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn bad_frames_are_rejected_by_extrude_and_revolve() {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(2.0, 2.0)),
        holes: vec![],
    };
    let mut nan = Frame::WORLD;
    nan.origin.x = f64::NAN;
    let mut skew = Frame::WORLD;
    skew.y = DVec3::new(0.1, 1.0, 0.0);
    let mut short = Frame::WORLD;
    short.x = DVec3::new(2.0, 0.0, 0.0);
    let mut left_handed = Frame::WORLD;
    left_handed.z = -DVec3::Z;
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(2.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(2.0, 2.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, 2.0),
            },
        ],
    };
    for f in [nan, skew, short, left_handed] {
        assert_eq!(
            k().extrude(&profile, &f, 1.0).unwrap_err(),
            KernelError::InvalidFrame
        );
        assert_eq!(
            k().revolve(&chain, &f).unwrap_err(),
            KernelError::InvalidFrame
        );
    }
    assert!(k().extrude(&profile, &Frame::WORLD, 1.0).is_ok());
}

#[test]
fn frame_orthonormality_check() {
    assert!(Frame::WORLD.is_orthonormal(1e-9));
    assert!(
        Frame::from_origin_normal(DVec3::new(1.0, 2.0, 3.0), DVec3::new(1.0, 1.0, 1.0))
            .is_orthonormal(1e-9)
    );
}

fn rect_loop(min: (f64, f64), max: (f64, f64)) -> ProfileLoop {
    ProfileLoop::rectangle(DVec2::new(min.0, min.1), DVec2::new(max.0, max.1))
}

#[test]
fn holes_must_lie_inside_and_apart() {
    let outer = rect_loop((0.0, 0.0), (20.0, 20.0));
    let ok = Profile {
        outer: outer.clone(),
        holes: vec![
            rect_loop((2.0, 2.0), (6.0, 6.0)),
            ProfileLoop::circle(DVec2::new(14.0, 14.0), 3.0),
        ],
    };
    assert!(k().extrude(&ok, &Frame::WORLD, 1.0).is_ok());

    let outside = Profile {
        outer: outer.clone(),
        holes: vec![rect_loop((30.0, 30.0), (34.0, 34.0))],
    };
    assert_eq!(
        k().extrude(&outside, &Frame::WORLD, 1.0).unwrap_err(),
        KernelError::HoleOutsideOuter
    );

    let crossing = Profile {
        outer: outer.clone(),
        holes: vec![rect_loop((15.0, 15.0), (25.0, 25.0))],
    };
    assert_eq!(
        k().extrude(&crossing, &Frame::WORLD, 1.0).unwrap_err(),
        KernelError::ProfileLoopsIntersect
    );

    let overlapping = Profile {
        outer: outer.clone(),
        holes: vec![
            rect_loop((2.0, 2.0), (8.0, 8.0)),
            rect_loop((6.0, 6.0), (12.0, 12.0)),
        ],
    };
    assert_eq!(
        k().extrude(&overlapping, &Frame::WORLD, 1.0).unwrap_err(),
        KernelError::ProfileLoopsIntersect
    );

    let touching = Profile {
        outer,
        holes: vec![rect_loop((0.0, 5.0), (4.0, 9.0))],
    };
    assert_eq!(
        k().extrude(&touching, &Frame::WORLD, 1.0).unwrap_err(),
        KernelError::ProfileLoopsIntersect
    );
}

#[test]
fn facade_reports_kinds_and_counts_without_indexing() {
    let solid = k().make_cylinder(2.0, 5.0).unwrap();
    let (faces, edges) = k().counts(&solid);
    assert_eq!(faces, solid.faces.len());
    assert_eq!(edges, solid.edges.len());
    let kinds: Vec<_> = solid
        .faces
        .iter()
        .filter_map(|(id, _)| k().face_kind(&solid, id))
        .collect();
    assert!(kinds.contains(&SurfaceKind::Cylinder) && kinds.contains(&SurfaceKind::Plane));
    let (eid, _) = solid.edges.iter().next().unwrap();
    assert!(k().edge_kind(&solid, eid).is_some());
    assert_eq!(k().face_kind(&solid, Id::from_index(999)), None);
    assert_eq!(k().edge_kind(&solid, Id::from_index(999)), None);
    assert_eq!(SurfaceKind::Plane.label(), "Ebene");
    assert_eq!(CurveKind::Circle.label(), "Kreis");
    let _ = (TAU, Tolerance::default());
}

#[test]
fn nested_holes_are_rejected_disjoint_holes_are_fine() {
    let outer = rect_loop((0.0, 0.0), (20.0, 20.0));
    let nested = Profile {
        outer: outer.clone(),
        holes: vec![
            rect_loop((2.0, 2.0), (12.0, 12.0)),
            rect_loop((4.0, 4.0), (6.0, 6.0)),
        ],
    };
    assert_eq!(
        k().extrude(&nested, &Frame::WORLD, 1.0).unwrap_err(),
        KernelError::NestedHoles
    );
    let nested_reverse_order = Profile {
        outer: outer.clone(),
        holes: vec![
            rect_loop((4.0, 4.0), (6.0, 6.0)),
            rect_loop((2.0, 2.0), (12.0, 12.0)),
        ],
    };
    assert_eq!(
        k().extrude(&nested_reverse_order, &Frame::WORLD, 1.0)
            .unwrap_err(),
        KernelError::NestedHoles
    );
    let disjoint = Profile {
        outer,
        holes: vec![
            rect_loop((2.0, 2.0), (6.0, 6.0)),
            rect_loop((10.0, 10.0), (14.0, 14.0)),
        ],
    };
    assert!(k().extrude(&disjoint, &Frame::WORLD, 1.0).is_ok());
}
