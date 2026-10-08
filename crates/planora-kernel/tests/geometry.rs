use planora_kernel::*;
use planora_math::*;

fn frame() -> Frame {
    Frame::from_origin_normal(DVec3::new(1.0, -2.0, 3.0), DVec3::new(0.2, 0.5, 0.8))
}

fn numeric_normal(s: &Surface, u: f64, v: f64) -> DVec3 {
    let h = 1e-6;
    let du = (s.point(u + h, v) - s.point(u - h, v)) / (2.0 * h);
    let dv = (s.point(u, v + h) - s.point(u, v - h)) / (2.0 * h);
    du.cross(dv).normalize()
}

#[test]
fn analytic_normals_match_numeric_cross_product() {
    let f = frame();
    let surfaces = [
        Surface::Plane { frame: f },
        Surface::Cylinder {
            frame: f,
            radius: 2.5,
        },
        Surface::Cone {
            frame: f,
            radius: 3.0,
            slope: 0.4,
        },
        Surface::Cone {
            frame: f,
            radius: 3.0,
            slope: -0.4,
        },
        Surface::Sphere {
            frame: f,
            radius: 4.0,
        },
        Surface::Torus {
            frame: f,
            major: 5.0,
            minor: 1.5,
        },
    ];
    for s in &surfaces {
        for &(u, v) in &[(0.3, 0.2), (1.9, -0.7), (4.4, 0.9), (6.0, 0.1)] {
            let n = s.normal(u, v);
            assert!((n.length() - 1.0).abs() < 1e-12);
            assert!(
                n.distance(numeric_normal(s, u, v)) < 1e-5,
                "{s:?} at ({u},{v})"
            );
        }
    }
}

#[test]
fn sphere_and_cylinder_points_have_expected_distance() {
    let f = Frame::WORLD;
    let sphere = Surface::Sphere {
        frame: f,
        radius: 4.0,
    };
    assert!((sphere.point(0.7, 0.3).length() - 4.0).abs() < 1e-12);
    let cyl = Surface::Cylinder {
        frame: f,
        radius: 2.0,
    };
    let p = cyl.point(1.1, 5.0);
    assert!((DVec2::new(p.x, p.y).length() - 2.0).abs() < 1e-12);
    assert!((p.z - 5.0).abs() < 1e-12);
}

#[test]
fn circle_curve_point_is_on_circle() {
    let c = Curve::Circle {
        frame: frame(),
        radius: 3.0,
    };
    for t in [0.0, 1.0, 2.5, 6.0] {
        assert!((c.point(t).distance(frame().origin) - 3.0).abs() < 1e-12);
    }
}

#[test]
fn line_curve_point_is_linear() {
    let l = Curve::Line {
        origin: DVec3::new(1.0, 1.0, 1.0),
        dir: DVec3::X,
    };
    assert!(l.point(2.0).distance(DVec3::new(3.0, 1.0, 1.0)) < 1e-12);
}
