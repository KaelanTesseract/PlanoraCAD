use planora_kernel::*;
use planora_math::*;

fn close(a: f64, b: f64, eps: f64) {
    assert!((a - b).abs() < eps, "{a} != {b}");
}

#[test]
fn rectangle_area_and_orientation() {
    let r = ProfileLoop::rectangle(DVec2::new(0.0, 0.0), DVec2::new(2.0, 3.0));
    close(r.signed_area(), 6.0, 1e-12);
    close(r.reversed().signed_area(), -6.0, 1e-12);
}

#[test]
fn circle_area_is_close_to_pi_r_squared() {
    let c = ProfileLoop::circle(DVec2::new(1.0, 1.0), 2.0);
    close(c.signed_area() / (PI * 4.0), 1.0, 0.005);
}

#[test]
fn arc_end_point_is_rotated_start() {
    let end = segment_end(
        DVec2::new(1.0, 0.0),
        &Segment::Arc {
            center: DVec2::ZERO,
            sweep: PI / 2.0,
        },
    );
    close(end.x, 0.0, 1e-12);
    close(end.y, 1.0, 1e-12);
}

#[test]
fn reversed_twice_visits_same_points() {
    let r = ProfileLoop::polygon(&[
        DVec2::new(0.0, 0.0),
        DVec2::new(4.0, 0.0),
        DVec2::new(4.0, 2.0),
        DVec2::new(0.0, 2.0),
    ])
    .unwrap();
    let rr = r.reversed().reversed();
    assert_eq!(r.chain_points().len(), rr.chain_points().len());
    for (a, b) in r.chain_points().iter().zip(rr.chain_points().iter()) {
        assert!(a.distance(*b) < 1e-12);
    }
}

#[test]
fn closed_and_open_loops_are_detected() {
    let tol = Tolerance::default();
    let closed = ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(1.0, 1.0));
    assert!(closed.is_closed(&tol));
    let open = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(1.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(1.0, 1.0),
            },
        ],
    };
    assert!(!open.is_closed(&tol));
}

#[test]
fn oriented_profile_has_ccw_outer_and_cw_holes() {
    let cw_outer = ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(10.0, 10.0)).reversed();
    let ccw_hole = ProfileLoop::circle(DVec2::new(5.0, 5.0), 1.0);
    let p = Profile {
        outer: cw_outer,
        holes: vec![ccw_hole],
    }
    .oriented();
    assert!(p.outer.signed_area() > 0.0);
    assert!(p.holes[0].signed_area() < 0.0);
}
