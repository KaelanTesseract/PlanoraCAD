use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn roots(c: CurveSurface) -> Vec<Root> {
    match c {
        CurveSurface::Roots(r) => r,
        CurveSurface::Lies => panic!("Kurve liegt auf der Fläche"),
    }
}

#[test]
fn line_hits_a_plane_once() {
    let line = Curve::Line {
        origin: DVec3::new(0.0, 0.0, -2.0),
        dir: DVec3::Z,
    };
    let plane = Surface::Plane {
        frame: Frame::WORLD,
    };
    let r = roots(curve_surface_roots(&line, 0.0, 4.0, &plane, &tol()));
    assert_eq!(r.len(), 1);
    assert!((r[0].t - 2.0).abs() < 1e-9 && !r[0].touching);
}

#[test]
fn line_hits_a_sphere_twice() {
    let line = Curve::Line {
        origin: DVec3::new(-3.0, 0.0, 0.0),
        dir: DVec3::X,
    };
    let sphere = Surface::Sphere {
        frame: Frame::WORLD,
        radius: 1.0,
    };
    let mut r = roots(curve_surface_roots(&line, 0.0, 6.0, &sphere, &tol()));
    r.sort_by(|a, b| a.t.total_cmp(&b.t));
    assert_eq!(r.len(), 2);
    assert!((r[0].t - 2.0).abs() < 1e-9 && (r[1].t - 4.0).abs() < 1e-9);
}

#[test]
fn circle_crosses_a_plane_twice_and_touches_once() {
    let plane = Surface::Plane {
        frame: Frame::WORLD,
    };
    // Kreis in der xz-Ebene, Mittelpunkt im Ursprung: schneidet z = 0 zweimal
    let f = Frame {
        origin: DVec3::ZERO,
        x: DVec3::X,
        y: DVec3::Z,
        z: -DVec3::Y,
    };
    let c = Curve::Circle {
        frame: f,
        radius: 1.0,
    };
    let r = roots(curve_surface_roots(&c, 0.0, TAU, &plane, &tol()));
    assert_eq!(r.len(), 2, "{r:?}");
    // Kreis, dessen tiefster Punkt die Ebene berührt
    let f2 = Frame {
        origin: DVec3::new(0.0, 0.0, 1.0),
        ..f
    };
    let c2 = Curve::Circle {
        frame: f2,
        radius: 1.0,
    };
    let r2 = roots(curve_surface_roots(&c2, 0.0, TAU, &plane, &tol()));
    assert_eq!(r2.len(), 1);
    assert!(r2[0].touching);
}

#[test]
fn curve_lying_on_the_surface_is_reported() {
    let plane = Surface::Plane {
        frame: Frame::WORLD,
    };
    let c = Curve::Circle {
        frame: Frame::WORLD,
        radius: 2.0,
    };
    assert!(matches!(
        curve_surface_roots(&c, 0.0, TAU, &plane, &tol()),
        CurveSurface::Lies
    ));
}

#[test]
fn endpoint_on_the_surface_counts_as_a_root() {
    let line = Curve::Line {
        origin: DVec3::ZERO,
        dir: DVec3::Z,
    };
    let plane = Surface::Plane {
        frame: Frame::WORLD,
    };
    let r = roots(curve_surface_roots(&line, 0.0, 3.0, &plane, &tol()));
    assert_eq!(r.len(), 1);
    assert!(r[0].t.abs() < 1e-9);
}

fn line_seg(o: DVec3, d: DVec3, len: f64) -> Seg {
    Seg::new(
        Curve::Line {
            origin: o,
            dir: d.normalize(),
        },
        0.0,
        len,
    )
}

#[test]
fn crossing_lines_meet_once() {
    let a = line_seg(DVec3::new(-1.0, 0.0, 0.0), DVec3::X, 2.0);
    let b = line_seg(DVec3::new(0.0, -1.0, 0.0), DVec3::Y, 2.0);
    let r = seg_intersections(&a, &b, &tol());
    assert_eq!(r.len(), 1, "{r:?}");
    assert!((r[0].0 - 0.5).abs() < 1e-6 && (r[0].1 - 0.5).abs() < 1e-6);
}

#[test]
fn line_and_circle_meet_twice() {
    let circle = Seg::new(
        Curve::Circle {
            frame: Frame::WORLD,
            radius: 1.0,
        },
        0.0,
        TAU,
    );
    let line = line_seg(DVec3::new(-2.0, 0.0, 0.0), DVec3::X, 4.0);
    let r = seg_intersections(&line, &circle, &tol());
    assert_eq!(r.len(), 2, "{r:?}");
}

#[test]
fn two_circles_meet_twice() {
    let a = Seg::new(
        Curve::Circle {
            frame: Frame::WORLD,
            radius: 1.0,
        },
        0.0,
        TAU,
    );
    let f = Frame {
        origin: DVec3::new(1.0, 0.0, 0.0),
        ..Frame::WORLD
    };
    let b = Seg::new(
        Curve::Circle {
            frame: f,
            radius: 1.0,
        },
        0.0,
        TAU,
    );
    assert_eq!(seg_intersections(&a, &b, &tol()).len(), 2);
}

#[test]
fn disjoint_segments_do_not_meet() {
    let a = line_seg(DVec3::ZERO, DVec3::X, 1.0);
    let b = line_seg(DVec3::new(0.0, 1.0, 0.0), DVec3::X, 1.0);
    assert!(seg_intersections(&a, &b, &tol()).is_empty());
}

#[test]
fn overlapping_collinear_segments_report_the_overlap_ends() {
    let a = line_seg(DVec3::ZERO, DVec3::X, 4.0);
    let b = line_seg(DVec3::new(2.0, 0.0, 0.0), DVec3::X, 4.0);
    let r = seg_intersections(&a, &b, &tol());
    assert!(r.len() >= 2, "{r:?}");
    let fa: Vec<f64> = r.iter().map(|x| x.0).collect();
    assert!(
        fa.iter().any(|f| (f - 0.5).abs() < 1e-3),
        "Überlappungsanfang bei 2/4: {fa:?}"
    );
    assert!(
        fa.iter().any(|f| (f - 1.0).abs() < 1e-3),
        "Überlappungsende bei 4/4: {fa:?}"
    );
}

#[test]
fn seg_closest_finds_nearest_point() {
    let c = Seg::new(
        Curve::Circle {
            frame: Frame::WORLD,
            radius: 2.0,
        },
        0.0,
        PI,
    );
    let (f, d) = c.closest(DVec3::new(0.0, 5.0, 0.0));
    assert!((f - 0.5).abs() < 1e-6 && (d - 3.0).abs() < 1e-6);
    // Punkt hinter dem Bogenende: nächster Punkt ist ein Endpunkt
    let (f2, d2) = c.closest(DVec3::new(0.0, -2.5, 0.0));
    assert!(f2 < 1e-6 || (f2 - 1.0).abs() < 1e-6);
    assert!(d2 > 0.4);
}

#[test]
fn reverse_loop_flips_order_and_direction() {
    let a = line_seg(DVec3::ZERO, DVec3::X, 1.0);
    let b = line_seg(DVec3::new(1.0, 0.0, 0.0), DVec3::Y, 1.0);
    let r = reverse_loop(&[a.clone(), b.clone()]);
    assert!(r[0].start().distance(b.end()) < 1e-12);
    assert!(r[1].end().distance(a.start()) < 1e-12);
}

#[test]
fn degenerate_and_non_finite_input_terminates() {
    let z = line_seg(DVec3::ZERO, DVec3::X, 0.0);
    let l = line_seg(DVec3::new(-1.0, 0.0, 0.0), DVec3::X, 2.0);
    assert_eq!(seg_intersections(&z, &l, &tol()).len(), 1);
    assert_eq!(seg_intersections(&l, &z, &tol()).len(), 1);
    assert_eq!(seg_intersections(&z, &z, &tol()).len(), 1);
    let nan = Seg::new(
        Curve::Line {
            origin: DVec3::ZERO,
            dir: DVec3::X,
        },
        0.0,
        f64::NAN,
    );
    assert!(seg_intersections(&nan, &l, &tol()).is_empty());
    let inf = Seg::new(
        Curve::Line {
            origin: DVec3::ZERO,
            dir: DVec3::X,
        },
        0.0,
        f64::INFINITY,
    );
    assert!(seg_intersections(&l, &inf, &tol()).is_empty());
    let plane = Surface::Plane {
        frame: Frame::WORLD,
    };
    let line = Curve::Line {
        origin: DVec3::new(0.0, 0.0, -1.0),
        dir: DVec3::Z,
    };
    assert!(roots(curve_surface_roots(&line, 0.0, f64::NAN, &plane, &tol())).is_empty());
    assert!(
        roots(curve_surface_roots(
            &line,
            0.0,
            f64::INFINITY,
            &plane,
            &tol()
        ))
        .is_empty()
    );
    assert_eq!(
        roots(curve_surface_roots(&line, 0.0, 0.0, &plane, &tol())).len(),
        0
    );
}

#[test]
fn long_line_meets_a_small_circle() {
    let line = line_seg(DVec3::new(-500.0, 0.3, 0.0), DVec3::X, 1000.0);
    let circle = Seg::new(
        Curve::Circle {
            frame: Frame::WORLD,
            radius: 1.0,
        },
        0.0,
        TAU,
    );
    assert_eq!(seg_intersections(&line, &circle, &tol()).len(), 2);
    assert_eq!(seg_intersections(&circle, &line, &tol()).len(), 2);
}

#[test]
fn closed_circle_seam_is_reported_once() {
    let circle = Seg::new(
        Curve::Circle {
            frame: Frame::WORLD,
            radius: 1.0,
        },
        0.0,
        TAU,
    );
    let line = line_seg(DVec3::new(1.0, -2.0, 0.0), DVec3::Y, 4.0);
    let r = seg_intersections(&circle, &line, &tol());
    assert_eq!(r.len(), 1, "{r:?}");
}

fn xz_frame(cz: f64) -> Frame {
    Frame {
        origin: DVec3::new(0.0, 0.0, cz),
        x: DVec3::X,
        y: DVec3::Z,
        z: -DVec3::Y,
    }
}

fn single(c: CurveSurface) -> Root {
    let r = roots(c);
    assert_eq!(r.len(), 1, "{r:?}");
    r[0]
}

#[test]
fn endpoint_slightly_off_the_surface_is_one_crossing() {
    let plane = Surface::Plane {
        frame: Frame::WORLD,
    };
    // Anfang 5e-7 unter der Ebene, steigt sofort durch
    let up = Curve::Line {
        origin: DVec3::new(0.0, 0.0, -5e-7),
        dir: DVec3::Z,
    };
    assert!(!single(curve_surface_roots(&up, 0.0, 3.0, &plane, &tol())).touching);
    // Ende 5e-7 über der Ebene
    let up2 = Curve::Line {
        origin: DVec3::new(0.0, 0.0, -3.0 + 5e-7),
        dir: DVec3::Z,
    };
    assert!(!single(curve_surface_roots(&up2, 0.0, 3.0, &plane, &tol())).touching);
    // absteigender Bereich: Start oben, Ende 5e-7 unter der Ebene
    let up3 = Curve::Line {
        origin: DVec3::new(0.0, 0.0, -5e-7),
        dir: DVec3::Z,
    };
    assert!(!single(curve_surface_roots(&up3, 3.0, 0.0, &plane, &tol())).touching);
    // Bogen, der am leicht abseitigen Anfang durch die Ebene läuft
    let arc = Curve::Circle {
        frame: xz_frame(5e-7),
        radius: 1.0,
    };
    assert!(!single(curve_surface_roots(&arc, 0.0, -1.0, &plane, &tol())).touching);
    assert!(!single(curve_surface_roots(&arc, 1.0, -1.0, &plane, &tol())).touching);
}

#[test]
fn full_circle_tangent_at_start_or_middle_is_one_touch() {
    let plane = Surface::Plane {
        frame: Frame::WORLD,
    };
    let c = Curve::Circle {
        frame: xz_frame(1.0),
        radius: 1.0,
    };
    let at_start = single(curve_surface_roots(
        &c,
        1.5 * PI,
        1.5 * PI + TAU,
        &plane,
        &tol(),
    ));
    assert!(at_start.touching);
    let in_middle = single(curve_surface_roots(
        &c,
        0.5 * PI,
        0.5 * PI + TAU,
        &plane,
        &tol(),
    ));
    assert!(in_middle.touching);
    // absteigend
    let desc = single(curve_surface_roots(
        &c,
        1.5 * PI,
        1.5 * PI - TAU,
        &plane,
        &tol(),
    ));
    assert!(desc.touching);
}

#[test]
fn full_circle_crossing_exactly_at_the_seam_is_one_crossing() {
    let plane = Surface::Plane {
        frame: Frame::WORLD,
    };
    let c = Curve::Circle {
        frame: xz_frame(0.0),
        radius: 1.0,
    };
    let r = roots(curve_surface_roots(&c, 0.0, TAU, &plane, &tol()));
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(r.iter().all(|x| !x.touching));
}

#[test]
fn long_ray_finds_grazing_roots_between_samples() {
    let sphere = Surface::Sphere {
        frame: Frame::WORLD,
        radius: 1.0,
    };
    let off = (1.0f64 - 0.025 * 0.025).sqrt();
    let line = Curve::Line {
        origin: DVec3::new(-500.1, off, 0.0),
        dir: DVec3::X,
    };
    let mut r = roots(curve_surface_roots(&line, 0.0, 1000.0, &sphere, &tol()));
    r.sort_by(|a, b| a.t.total_cmp(&b.t));
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(
        (r[0].t - 500.075).abs() < 1e-6 && (r[1].t - 500.125).abs() < 1e-6,
        "{r:?}"
    );
    assert!(r.iter().all(|x| !x.touching));
    // genau tangential
    let tang = Curve::Line {
        origin: DVec3::new(-500.1, 1.0, 0.0),
        dir: DVec3::X,
    };
    assert!(single(curve_surface_roots(&tang, 0.0, 1000.0, &sphere, &tol())).touching);
    // knapp daneben
    let miss = Curve::Line {
        origin: DVec3::new(-500.1, 1.001, 0.0),
        dir: DVec3::X,
    };
    assert!(roots(curve_surface_roots(&miss, 0.0, 1000.0, &sphere, &tol())).is_empty());
}

#[test]
fn grazing_inside_the_first_or_last_sampling_interval_is_found() {
    let sphere = Surface::Sphere {
        frame: Frame::WORLD,
        radius: 1.0,
    };
    let off = (1.0f64 - 0.025 * 0.025).sqrt();
    // Mitte der Kugel bei t = 0,1 (erstes Intervall) und t = 999,9 (letztes Intervall)
    for (cx, c) in [(0.1, 0.1), (999.9, 999.9)] {
        let line = Curve::Line {
            origin: DVec3::new(-cx, off, 0.0),
            dir: DVec3::X,
        };
        let mut r = roots(curve_surface_roots(&line, 0.0, 1000.0, &sphere, &tol()));
        r.sort_by(|a, b| a.t.total_cmp(&b.t));
        assert_eq!(r.len(), 2, "{r:?}");
        assert!(
            (r[0].t - (c - 0.025)).abs() < 1e-6 && (r[1].t - (c + 0.025)).abs() < 1e-6,
            "{r:?}"
        );
        assert!(r.iter().all(|x| !x.touching));
        // tangential: eine Berührung
        let tang = Curve::Line {
            origin: DVec3::new(-cx, 1.0, 0.0),
            dir: DVec3::X,
        };
        let t = single(curve_surface_roots(&tang, 0.0, 1000.0, &sphere, &tol()));
        assert!(t.touching && (t.t - c).abs() < 1e-4, "{t:?}");
    }
}
