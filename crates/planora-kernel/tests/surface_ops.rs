use planora_kernel::*;
use planora_math::*;

fn frame() -> Frame {
    Frame::from_origin_normal(DVec3::new(1.0, -2.0, 3.0), DVec3::new(0.2, 0.5, 0.8))
}

fn samples() -> Vec<(Surface, f64, f64)> {
    let f = frame();
    vec![
        (Surface::Plane { frame: f }, 0.7, -1.2),
        (
            Surface::Cylinder {
                frame: f,
                radius: 2.5,
            },
            1.1,
            0.8,
        ),
        (
            Surface::Cone {
                frame: f,
                radius: 3.0,
                slope: 0.4,
            },
            2.0,
            0.5,
        ),
        (
            Surface::Cone {
                frame: f,
                radius: 3.0,
                slope: -0.4,
            },
            2.0,
            0.5,
        ),
        (
            Surface::Sphere {
                frame: f,
                radius: 4.0,
            },
            0.9,
            0.4,
        ),
        (
            Surface::Torus {
                frame: f,
                major: 5.0,
                minor: 1.5,
            },
            2.2,
            1.1,
        ),
    ]
}

#[test]
fn implicit_is_zero_on_the_surface() {
    for (s, u, v) in samples() {
        assert!(s.implicit(s.point(u, v)).abs() < 1e-12, "{s:?}");
    }
}

#[test]
fn implicit_is_positive_on_the_normal_side() {
    for (s, u, v) in samples() {
        let p = s.point(u, v);
        let n = s.normal(u, v);
        assert!(s.implicit(p + n * 0.1) > 0.05, "{s:?}");
        assert!(s.implicit(p - n * 0.1) < -0.05, "{s:?}");
    }
}

#[test]
fn project_inverts_point() {
    for (s, u, v) in samples() {
        let p = s.point(u, v);
        let (pu, pv) = s.project(p);
        assert!(s.point(pu, pv).distance(p) < 1e-9, "{s:?}");
    }
}

#[test]
fn periodicity_and_poles() {
    let f = Frame::WORLD;
    assert!(!Surface::Plane { frame: f }.is_periodic());
    assert!(
        Surface::Cylinder {
            frame: f,
            radius: 1.0
        }
        .is_periodic()
    );
    assert!(
        Surface::Cylinder {
            frame: f,
            radius: 1.0
        }
        .pole_vs()
        .is_empty()
    );
    let poles = Surface::Sphere {
        frame: f,
        radius: 2.0,
    }
    .pole_vs();
    assert_eq!(poles.len(), 2);
    assert!((poles[0] + PI / 2.0).abs() < 1e-12 && (poles[1] - PI / 2.0).abs() < 1e-12);
    // Kegel mit radius 3 und slope -0.5: Spitze bei v = 6
    let apex = Surface::Cone {
        frame: f,
        radius: 3.0,
        slope: -0.5,
    }
    .pole_vs();
    assert_eq!(apex.len(), 1);
    assert!((apex[0] - 6.0).abs() < 1e-12);
    assert!(
        Surface::Torus {
            frame: f,
            major: 3.0,
            minor: 1.0
        }
        .pole_vs()
        .is_empty()
    );
}

#[test]
fn curve_project_inverts_point() {
    let f = frame();
    let curves = [
        Curve::Line {
            origin: DVec3::new(1.0, 2.0, 3.0),
            dir: DVec3::new(0.0, 0.6, 0.8),
        },
        Curve::Circle {
            frame: f,
            radius: 2.0,
        },
        Curve::Ellipse {
            frame: f,
            major: 3.0,
            minor: 1.5,
        },
    ];
    for c in &curves {
        for t in [0.3, 1.2, 2.5] {
            let p = c.point(t);
            let back = c.project(p);
            assert!(c.point(back).distance(p) < 1e-9, "{c:?} t={t}");
        }
    }
}

#[test]
fn curve_tangent_matches_finite_difference() {
    let f = frame();
    let curves = [
        Curve::Line {
            origin: DVec3::ZERO,
            dir: DVec3::X,
        },
        Curve::Circle {
            frame: f,
            radius: 2.0,
        },
        Curve::Ellipse {
            frame: f,
            major: 3.0,
            minor: 1.5,
        },
    ];
    let h = 1e-6;
    for c in &curves {
        for t in [0.3, 1.2, 2.5] {
            let num = (c.point(t + h) - c.point(t - h)) / (2.0 * h);
            assert!(c.tangent(t).distance(num) < 1e-5, "{c:?} t={t}");
        }
    }
}
