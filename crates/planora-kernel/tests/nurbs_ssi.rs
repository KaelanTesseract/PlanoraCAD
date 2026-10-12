use planora_kernel::boolean::{NURBS_FIT_TOL, SsiOutcome, intersect_nurbs};
use planora_kernel::{Curve, Surface};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DVec3, Frame, PI, TAU, Tolerance};

fn tol() -> Tolerance {
    Tolerance::default()
}

fn nurbs_cylinder(axis_origin: DVec3, axis: DVec3, r: f64, h: f64) -> Surface {
    // Drehfläche einer zur Achse parallelen Linie
    let x = if axis.cross(DVec3::X).length() > 0.5 {
        DVec3::X
    } else {
        DVec3::Y
    };
    let radial = axis.cross(x).cross(axis).normalize();
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![
            axis_origin + radial * r,
            axis_origin + radial * r + axis * h,
        ],
        vec![1.0; 2],
    )
    .unwrap();
    Surface::nurbs(
        NurbsSurface::revolve(&line, axis_origin, axis, 0.0, TAU).unwrap(),
        true,
        false,
    )
    .unwrap()
}

fn plane(origin: DVec3, normal: DVec3) -> Surface {
    Surface::Plane {
        frame: Frame::from_origin_normal(origin, normal),
    }
}

fn sample(c: &Curve, n: usize) -> Vec<DVec3> {
    let (a, b) = c.domain().expect("NURBS-Kurve");
    (0..=n)
        .map(|i| c.point(a + (b - a) * i as f64 / n as f64))
        .collect()
}

#[test]
fn a_horizontal_plane_cuts_a_nurbs_cylinder_in_a_circle() {
    let cyl = nurbs_cylinder(DVec3::ZERO, DVec3::Z, 3.0, 10.0);
    let pl = plane(DVec3::new(0.0, 0.0, 4.0), DVec3::Z);
    let SsiOutcome::Curves(cs) = intersect_nurbs(&cyl, &pl, &tol()) else {
        panic!("Kurven erwartet")
    };
    assert_eq!(cs.len(), 1);
    let pts = sample(&cs[0], 400);
    for p in &pts {
        assert!(
            ((p.x * p.x + p.y * p.y).sqrt() - 3.0).abs() < 2.0 * NURBS_FIT_TOL,
            "{p:?}"
        );
        assert!((p.z - 4.0).abs() < 2.0 * NURBS_FIT_TOL);
    }
    assert!(cs[0].period().is_some(), "geschlossen");
    let len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    assert!((len - TAU * 3.0).abs() / (TAU * 3.0) < 1e-4, "{len}");
}

#[test]
fn a_tilted_plane_cuts_in_an_ellipse() {
    let cyl = nurbs_cylinder(DVec3::ZERO, DVec3::Z, 2.0, 10.0);
    let n = DVec3::new(0.3, 0.0, 1.0).normalize();
    let pl = plane(DVec3::new(0.0, 0.0, 5.0), n);
    let SsiOutcome::Curves(cs) = intersect_nurbs(&cyl, &pl, &tol()) else {
        panic!()
    };
    assert_eq!(cs.len(), 1);
    for p in sample(&cs[0], 300) {
        assert!(((p.x * p.x + p.y * p.y).sqrt() - 2.0).abs() < 2.0 * NURBS_FIT_TOL);
        assert!((p - DVec3::new(0.0, 0.0, 5.0)).dot(n).abs() < 2.0 * NURBS_FIT_TOL);
    }
}

#[test]
fn an_analytic_sphere_cuts_a_nurbs_cylinder() {
    let cyl = nurbs_cylinder(DVec3::ZERO, DVec3::Z, 2.0, 10.0);
    let sph = Surface::Sphere {
        frame: Frame::from_origin_normal(DVec3::new(0.0, 0.0, 5.0), DVec3::Z),
        radius: 3.0,
    };
    let SsiOutcome::Curves(cs) = intersect_nurbs(&cyl, &sph, &tol()) else {
        panic!()
    };
    assert_eq!(cs.len(), 2, "zwei Kreise oben und unten");
    for c in &cs {
        for p in sample(c, 200) {
            assert!(((p.x * p.x + p.y * p.y).sqrt() - 2.0).abs() < 2.0 * NURBS_FIT_TOL);
            assert!(((p - DVec3::new(0.0, 0.0, 5.0)).length() - 3.0).abs() < 2.0 * NURBS_FIT_TOL);
        }
    }
}

#[test]
fn two_crossing_nurbs_cylinders_of_different_radius_give_a_closed_curve_on_both() {
    let a = nurbs_cylinder(DVec3::new(0.0, 0.0, -10.0), DVec3::Z, 2.0, 20.0);
    let b = nurbs_cylinder(DVec3::new(-10.0, 0.0, 0.0), DVec3::X, 1.0, 20.0);
    let SsiOutcome::Curves(cs) = intersect_nurbs(&a, &b, &tol()) else {
        panic!("Kurven erwartet")
    };
    assert!(!cs.is_empty());
    for c in &cs {
        assert!(c.period().is_some(), "geschlossen");
        for p in sample(c, 300) {
            assert!(
                a.implicit(p).abs() < 2.0 * NURBS_FIT_TOL
                    && b.implicit(p).abs() < 2.0 * NURBS_FIT_TOL,
                "{p:?}"
            );
        }
    }
}

#[test]
fn equal_radii_crossing_cylinders_are_reported_not_solved_wrongly() {
    let a = nurbs_cylinder(DVec3::new(0.0, 0.0, -10.0), DVec3::Z, 2.0, 20.0);
    let b = nurbs_cylinder(DVec3::new(-10.0, 0.0, 0.0), DVec3::X, 2.0, 20.0);
    match intersect_nurbs(&a, &b, &tol()) {
        SsiOutcome::Unsupported(msg) => assert!(
            msg.contains("verzweigt") || msg.contains("tangential"),
            "{msg}"
        ),
        SsiOutcome::Curves(cs) => {
            // Falls der Rechner die beiden Ellipsen trotzdem sauber liefert, müssen sie auf beiden Flächen liegen.
            for c in cs {
                for p in sample(&c, 100) {
                    assert!(a.implicit(p).abs() < 1e-4 && b.implicit(p).abs() < 1e-4);
                }
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn separate_touching_and_coincident_cases() {
    let cyl = nurbs_cylinder(DVec3::ZERO, DVec3::Z, 2.0, 10.0);
    assert!(matches!(
        intersect_nurbs(&cyl, &plane(DVec3::new(0.0, 0.0, 50.0), DVec3::Z), &tol()),
        SsiOutcome::None
    ));
    // Ebene tangential an den Zylinder (x = 2)
    assert!(matches!(
        intersect_nurbs(&cyl, &plane(DVec3::new(2.0, 0.0, 0.0), DVec3::X), &tol()),
        SsiOutcome::Touch
    ));
    // NURBS-Ebene gegen dieselbe analytische Ebene
    let flat = NurbsSurface::new(
        1,
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![0.0, 0.0, 1.0, 1.0],
        2,
        2,
        vec![
            DVec3::ZERO,
            DVec3::Y * 4.0,
            DVec3::X * 4.0,
            DVec3::new(4.0, 4.0, 0.0),
        ],
        vec![1.0; 4],
    )
    .unwrap();
    let nf = Surface::nurbs(flat, false, false).unwrap();
    assert!(matches!(
        intersect_nurbs(&nf, &plane(DVec3::ZERO, DVec3::Z), &tol()),
        SsiOutcome::Coincident
    ));
}

#[test]
fn the_curve_lies_within_the_fit_tolerance_of_the_exact_circle_between_samples() {
    let cyl = nurbs_cylinder(DVec3::ZERO, DVec3::Z, 3.0, 10.0);
    let SsiOutcome::Curves(cs) =
        intersect_nurbs(&cyl, &plane(DVec3::new(0.0, 0.0, 4.0), DVec3::Z), &tol())
    else {
        panic!()
    };
    for p in sample(&cs[0], 5000) {
        assert!(((p.x * p.x + p.y * p.y).sqrt() - 3.0).abs() < 2.0 * NURBS_FIT_TOL);
    }
    let _ = PI;
}

#[test]
fn work_is_bounded_even_for_wild_input() {
    // sehr gewellte Fläche mit vielen Kontrollpunkten gegen eine Ebene: endet in begrenzter Zeit mit Kurven oder benannter Grenze
    let n = 40usize;
    let mut kn = vec![0.0; 4];
    for i in 1..=(n - 4) {
        kn.push(i as f64 / (n - 3) as f64);
    }
    kn.extend([1.0; 4]);
    let mut c = Vec::new();
    for i in 0..n {
        for j in 0..n {
            c.push(DVec3::new(
                i as f64,
                j as f64,
                ((i * 7 + j * 13) % 9) as f64 * 0.5 - 2.0,
            ));
        }
    }
    let s = NurbsSurface::new(3, 3, kn.clone(), kn, n, n, c, vec![1.0; n * n]).unwrap();
    let surf = Surface::nurbs(s, false, false).unwrap();
    let start = std::time::Instant::now();
    let r = intersect_nurbs(&surf, &plane(DVec3::new(0.0, 0.0, 0.0), DVec3::Z), &tol());
    assert!(start.elapsed().as_secs() < 60, "dauert zu lange");
    assert!(matches!(
        r,
        SsiOutcome::Curves(_) | SsiOutcome::Unsupported(_)
    ));
}

// ---- Zusätzliche Fälle (über den Plan hinaus): Abbildung, Berührung, Nähe, Grenzen ----

fn square(x0: f64, y0: f64, w: f64) -> Surface {
    let f = NurbsSurface::new(
        1,
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![0.0, 0.0, 1.0, 1.0],
        2,
        2,
        vec![
            DVec3::new(x0, y0, 0.0),
            DVec3::new(x0, y0 + w, 0.0),
            DVec3::new(x0 + w, y0, 0.0),
            DVec3::new(x0 + w, y0 + w, 0.0),
        ],
        vec![1.0; 4],
    )
    .unwrap();
    Surface::nurbs(f, false, false).unwrap()
}

fn on_both(cs: &[Curve], a: &Surface, b: &Surface) {
    for c in cs {
        for p in sample(c, 200) {
            assert!(
                a.implicit(p).abs() < 2.0 * NURBS_FIT_TOL
                    && b.implicit(p).abs() < 2.0 * NURBS_FIT_TOL,
                "{p:?}"
            );
        }
    }
}

#[test]
fn intersect_surfaces_maps_the_numerical_outcomes() {
    use planora_kernel::boolean::{SurfaceIntersection, intersect_surfaces};
    let cyl = nurbs_cylinder(DVec3::ZERO, DVec3::Z, 2.0, 10.0);
    let far = plane(DVec3::new(0.0, 0.0, 50.0), DVec3::Z);
    assert!(matches!(
        intersect_surfaces(&far, &cyl, &tol()),
        SurfaceIntersection::None
    ));
    assert!(matches!(
        intersect_surfaces(
            &square(0.0, 0.0, 4.0),
            &plane(DVec3::ZERO, DVec3::Z),
            &tol()
        ),
        SurfaceIntersection::Coincident
    ));
    let a = nurbs_cylinder(DVec3::new(0.0, 0.0, -10.0), DVec3::Z, 2.0, 20.0);
    let b = nurbs_cylinder(DVec3::new(-10.0, 0.0, 0.0), DVec3::X, 2.0, 20.0);
    match intersect_surfaces(&a, &b, &tol()) {
        SurfaceIntersection::UnsupportedBecause(msg) => {
            assert!(
                msg.contains("tangential") || msg.contains("verzweigt"),
                "{msg}"
            )
        }
        SurfaceIntersection::Curves(cs) => on_both(&cs, &a, &b),
        other => panic!("{other:?}"),
    }
}

#[test]
fn touching_and_partly_coincident_surfaces_are_named() {
    let cyl = nurbs_cylinder(DVec3::ZERO, DVec3::Z, 2.0, 10.0);
    // Kugel berührt den Zylinder von außen in einem Punkt
    let outside = Surface::Sphere {
        frame: Frame::from_origin_normal(DVec3::new(3.0, 0.0, 5.0), DVec3::Z),
        radius: 1.0,
    };
    assert!(matches!(
        intersect_nurbs(&cyl, &outside, &tol()),
        SsiOutcome::Touch
    ));
    // Zwei Quadrate in derselben Ebene, die sich zur Hälfte überdecken
    match intersect_nurbs(&square(0.0, 0.0, 4.0), &square(2.0, 0.0, 4.0), &tol()) {
        SsiOutcome::Unsupported(msg) => assert!(msg.contains("teilweise"), "{msg}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn close_curves_in_one_flat_patch_are_both_found() {
    // Ein ebenes Freiformquadrat (ein einziger flacher Flicken) wird von einem flach liegenden
    // Zylinder in zwei Geraden geschnitten; ebenso eine Ebene durch einen Torus in zwei Kreisen.
    let sq = square(0.0, 0.0, 10.0);
    let cyl = Surface::Cylinder {
        frame: Frame::from_origin_normal(DVec3::new(0.0, 5.0, 0.5), DVec3::X),
        radius: 1.0,
    };
    let SsiOutcome::Curves(cs) = intersect_nurbs(&sq, &cyl, &tol()) else {
        panic!("Kurven erwartet")
    };
    assert_eq!(cs.len(), 2);
    on_both(&cs, &sq, &cyl);
    for c in &cs {
        assert!(c.period().is_none(), "offen, von Rand zu Rand");
        let (t0, t1) = c.domain().unwrap();
        // Enden auf dem Rand x = 0 bzw. x = 10 (Bisektion auf 1e-9)
        for p in [c.point(t0), c.point(t1)] {
            assert!(p.x.abs() < 1e-8 || (p.x - 10.0).abs() < 1e-8, "{p:?}");
        }
    }
    let big = square(-50.0, -50.0, 100.0);
    let torus = Surface::Torus {
        frame: Frame::from_origin_normal(DVec3::new(3.0, 7.0, 0.0), DVec3::Z),
        major: 2.0,
        minor: 0.2,
    };
    let SsiOutcome::Curves(cs) = intersect_nurbs(&big, &torus, &tol()) else {
        panic!("Kurven erwartet")
    };
    assert_eq!(cs.len(), 2, "Kreise mit r = 1,8 und 2,2");
    on_both(&cs, &big, &torus);
}

#[test]
fn invalid_tolerances_are_a_named_error_not_a_panic() {
    let cyl = nurbs_cylinder(DVec3::ZERO, DVec3::Z, 2.0, 10.0);
    let pl = plane(DVec3::new(0.0, 0.0, 4.0), DVec3::Z);
    for linear in [f64::NAN, 0.0, -1.0, f64::INFINITY] {
        let t = Tolerance {
            linear,
            angular: 1e-9,
        };
        match intersect_nurbs(&cyl, &pl, &t) {
            SsiOutcome::Unsupported(msg) => assert!(msg.contains("Numerischer Fehler"), "{msg}"),
            other => panic!("{other:?}"),
        }
    }
    // zwei analytische Flächen gehören nicht hierher
    assert!(matches!(
        intersect_nurbs(&pl, &plane(DVec3::ZERO, DVec3::X), &tol()),
        SsiOutcome::Unsupported(_)
    ));
}

#[test]
fn booleans_with_nurbs_faces_keep_a_named_error_until_the_pipeline_follows() {
    // Die Schnitte werden jetzt berechnet, die Boolesche Pipeline verarbeitet Freiformkurven aber
    // erst mit Task 9: bis dahin bleibt es beim benannten Fehler (kein stilles Teilergebnis).
    use planora_kernel::boolean::{BoolOp, BooleanError, boolean};
    use planora_kernel::{AnalyticKernel, Kernel, Profile, ProfileLoop};
    let ellipse = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(3.0, 0.0, 0.0),
        DVec3::new(0.0, 2.0, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop::nurbs_loop(ellipse).unwrap(),
        holes: vec![],
    };
    let k = AnalyticKernel::default();
    let body = k
        .extrude(
            &profile,
            &Frame::from_origin_normal(DVec3::ZERO, DVec3::Z),
            5.0,
        )
        .unwrap();
    let cube = k
        .transform(
            &k.make_box(DVec3::splat(2.0)).unwrap(),
            &planora_math::DMat4::from_translation(DVec3::new(1.5, 0.5, 1.0)),
        )
        .unwrap();
    match boolean(&body, &cube, BoolOp::Union, &tol()) {
        Err(BooleanError::UnsupportedIntersection { a, b }) => {
            assert!(a == "Freiformfläche" || b == "Freiformfläche", "{a} {b}")
        }
        Err(BooleanError::Unsupported(msg)) => assert!(msg.contains("Freiformfläche"), "{msg}"),
        other => panic!("{other:?}"),
    }
}
