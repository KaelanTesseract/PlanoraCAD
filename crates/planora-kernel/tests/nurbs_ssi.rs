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
fn booleans_with_nurbs_faces_keep_the_old_named_error_until_the_pipeline_follows() {
    // Die Boolesche Pipeline verarbeitet Freiformflächen erst mit Task 9: bis dahin ergibt jedes
    // Flächenpaar mit einer Freiformfläche (überlappende Hüllquader) den benannten Fehler wie
    // vor dem Schnittrechner – auch ohne Schnitt (Würfel ganz im Körper) und bei Durchdringung.
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
    for (size, at) in [
        (2.0, DVec3::new(1.5, 0.5, 1.0)),
        (1.0, DVec3::new(-0.5, -0.5, 2.0)),
    ] {
        let cube = k
            .transform(
                &k.make_box(DVec3::splat(size)).unwrap(),
                &planora_math::DMat4::from_translation(at),
            )
            .unwrap();
        match boolean(&body, &cube, BoolOp::Union, &tol()) {
            Err(BooleanError::UnsupportedIntersection { a, b }) => {
                assert!(a == "Freiformfläche" || b == "Freiformfläche", "{a} {b}")
            }
            other => panic!("{other:?}"),
        }
    }
}

// ---- Nachbesserung Task 8 (Review): Eingaben des Reviews ----

/// Einfacher Zufallsgenerator (reproduzierbar, keine Abhängigkeit).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn rotate(radial: DVec3, axis: DVec3, phi: f64) -> DVec3 {
    radial * phi.cos() + axis.cross(radial) * phi.sin()
}

fn curve_length(c: &Curve) -> f64 {
    let pts = sample(c, 4000);
    pts.windows(2).map(|w| w[0].distance(w[1])).sum()
}

fn nurbs_sphere(c: DVec3, axis: DVec3, radial: DVec3, r: f64, phase: f64) -> Surface {
    let semi = NurbsCurve::elliptic_arc(c, radial * r, axis * r, -PI / 2.0, PI).unwrap();
    Surface::nurbs(
        NurbsSurface::revolve(&semi, c, axis, phase, TAU).unwrap(),
        true,
        false,
    )
    .unwrap()
}

/// Torus aus einem Vollkreis (Phase `p0`) um die Achse gedreht (Phase `p1`).
fn nurbs_torus(
    c: DVec3,
    axis: DVec3,
    radial: DVec3,
    radii: (f64, f64),
    p0: f64,
    p1: f64,
) -> Surface {
    let (big, small) = radii;
    let prof =
        NurbsCurve::elliptic_arc(c + radial * big, radial * small, axis * small, p0, TAU).unwrap();
    Surface::nurbs(
        NurbsSurface::revolve(&prof, c, axis, p1, TAU).unwrap(),
        true,
        true,
    )
    .unwrap()
}

#[test]
fn a_plane_just_below_the_top_of_a_nurbs_torus_gives_both_circles() {
    // Review C1: Früher fand der Rechner nur einen der beiden Kreise (beide lagen in einem
    // flachen Kandidaten). Naht nicht oben, zwei Phasen.
    for phase in [0.3, 2.0] {
        let t = nurbs_torus(DVec3::ZERO, DVec3::Z, DVec3::X, (3.0, 1.0), phase, phase);
        let pl = plane(DVec3::new(0.0, 0.0, 0.999), DVec3::Z);
        let SsiOutcome::Curves(cs) = intersect_nurbs(&t, &pl, &tol()) else {
            panic!("Kurven erwartet")
        };
        assert_eq!(cs.len(), 2);
        let total: f64 = cs.iter().map(curve_length).sum();
        assert!((total - 4.0 * PI * 3.0).abs() < 1e-4, "{total}");
        on_both(&cs, &t, &pl);
    }
}

#[test]
fn random_quadrics_near_tangent_planes_match_the_analytic_solution() {
    // Review C1: 40 Fälle Torus/Zylinder/Kugel (Achse, Lage, Nähte zufällig) gegen eine Ebene im
    // Abstand r·(1 − ε), ε zwischen 1e-4 und 0,3. Anzahl und Gesamtlänge wie analytisch; eine
    // benannte Meldung mit „tangential“ wäre ehrlich, kommt hier aber nicht vor.
    let mut rng = Rng(2026);
    for k in 0..40 {
        let axis = DVec3::new(rng.next() - 0.5, rng.next() - 0.5, rng.next() - 0.5).normalize();
        let radial = rotate(axis.any_orthonormal_vector(), axis, rng.next() * TAU);
        let eps = 10f64.powf(-4.0 + rng.next() * (4.0 + 0.3f64.log10()));
        let o = DVec3::new(rng.next(), rng.next(), rng.next()) * 4.0 - 2.0;
        let (s, pl, count, len) = match k % 3 {
            0 => {
                let (big, r) = (3.0, 1.0);
                let (p0, p1) = (rng.next() * TAU, rng.next() * TAU);
                let s = nurbs_torus(o, axis, radial, (big, r), p0, p1);
                (
                    s,
                    plane(o + axis * r * (1.0 - eps), axis),
                    2,
                    4.0 * PI * big,
                )
            }
            1 => {
                let (r, h) = (2.0, 10.0);
                let line = NurbsCurve::new(
                    1,
                    vec![0.0, 0.0, 1.0, 1.0],
                    vec![o + radial * r, o + radial * r + axis * h],
                    vec![1.0; 2],
                )
                .unwrap();
                let s = Surface::nurbs(
                    NurbsSurface::revolve(&line, o, axis, 0.0, TAU).unwrap(),
                    true,
                    false,
                )
                .unwrap();
                let n = rotate(radial, axis, rng.next() * TAU);
                (s, plane(o + n * r * (1.0 - eps), n), 2, 2.0 * h)
            }
            _ => {
                let r = 3.0;
                let s = nurbs_sphere(o, axis, radial, r, rng.next() * TAU);
                let n =
                    DVec3::new(rng.next() - 0.5, rng.next() - 0.5, rng.next() - 0.5).normalize();
                let rho = r * (1.0 - (1.0 - eps) * (1.0 - eps)).sqrt();
                (s, plane(o + n * r * (1.0 - eps), n), 1, TAU * rho)
            }
        };
        match intersect_nurbs(&s, &pl, &tol()) {
            SsiOutcome::Curves(cs) => {
                assert_eq!(cs.len(), count, "Fall {k}, eps = {eps}");
                let total: f64 = cs.iter().map(curve_length).sum();
                assert!(
                    (total - len).abs() / len < 1e-5,
                    "Fall {k}: {total} statt {len}"
                );
                on_both(&cs, &s, &pl);
            }
            SsiOutcome::Unsupported(msg) => assert!(msg.contains("tangential"), "Fall {k}: {msg}"),
            other => panic!("Fall {k}: {other:?}"),
        }
    }
}

#[test]
fn nearly_coincident_and_stacked_cylinders_finish_quickly() {
    // Review C2: früher Minuten. Erlaubt: sauberes Ergebnis oder benannte Meldung, schnell.
    let a = nurbs_cylinder(DVec3::ZERO, DVec3::Z, 2.0, 20.0);
    for b in [
        nurbs_cylinder(DVec3::ZERO, DVec3::Z, 2.0001, 20.0),
        nurbs_cylinder(DVec3::new(0.0, 0.0, 20.0 + 1e-7), DVec3::Z, 2.0, 20.0),
    ] {
        let start = std::time::Instant::now();
        let r = intersect_nurbs(&a, &b, &tol());
        assert!(
            start.elapsed().as_secs_f64() < 10.0,
            "{:?}",
            start.elapsed()
        );
        assert!(
            matches!(
                r,
                SsiOutcome::None | SsiOutcome::Touch | SsiOutcome::Unsupported(_)
            ),
            "{r:?}"
        );
    }
}

#[test]
fn cylinders_sharing_a_rim_touch_and_overlapping_ones_are_partly_coincident() {
    // Review I2: Proben jenseits einer beschränkten Fläche zählen nicht als Durchdringung.
    let a = nurbs_cylinder(DVec3::ZERO, DVec3::Z, 2.0, 20.0);
    let rim = nurbs_cylinder(DVec3::new(0.0, 0.0, 20.0 - 1e-7), DVec3::Z, 2.0, 20.0);
    let r = intersect_nurbs(&a, &rim, &tol());
    assert!(matches!(r, SsiOutcome::Touch), "{r:?}");
    let overlap = nurbs_cylinder(DVec3::new(0.0, 0.0, 19.8), DVec3::Z, 2.0, 20.0);
    match intersect_nurbs(&a, &overlap, &tol()) {
        SsiOutcome::Unsupported(msg) => assert!(msg.contains("teilweise"), "{msg}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn large_parts_are_cut_like_small_ones() {
    // Review I1: Toleranzen wachsen mit der Größe; keine falsche Meldung „verzweigt“ oder
    // „zu lang“ bei Radien 300 und 1000 mm und bei 1-m-Teilen.
    let n = DVec3::new(0.3, 0.1, 1.0).normalize();
    for r in [300.0, 1000.0] {
        let cyl = nurbs_cylinder(DVec3::new(0.0, 0.0, -r), DVec3::Z, r, 2.0 * r);
        let pl = plane(DVec3::new(0.0, 0.0, 30.0), n);
        let SsiOutcome::Curves(cs) = intersect_nurbs(&cyl, &pl, &tol()) else {
            panic!("r = {r}")
        };
        assert_eq!(cs.len(), 1, "r = {r}");
        assert!(cs[0].period().is_some());
        on_both(&cs, &cyl, &pl);
    }
    let ax = DVec3::new(0.1, 0.2, 1.0).normalize();
    let sph = nurbs_sphere(DVec3::ZERO, ax, ax.any_orthonormal_vector(), 1000.0, 0.0);
    let pl = plane(DVec3::new(0.0, 0.0, 300.0), n);
    let SsiOutcome::Curves(cs) = intersect_nurbs(&sph, &pl, &tol()) else {
        panic!("Kugel")
    };
    assert_eq!(cs.len(), 1);
    let d = 300.0 * n.z;
    let expect = TAU * (1000.0f64 * 1000.0 - d * d).sqrt();
    assert!((curve_length(&cs[0]) - expect).abs() / expect < 1e-5);
    on_both(&cs, &sph, &pl);
    // zwei sich kreuzende Freiform-Zylinder in Metergröße
    let a = nurbs_cylinder(DVec3::new(0.0, 0.0, -500.0), DVec3::Z, 200.0, 1000.0);
    let b = nurbs_cylinder(DVec3::new(-500.0, 0.0, 0.0), DVec3::X, 100.0, 1000.0);
    let SsiOutcome::Curves(cs) = intersect_nurbs(&a, &b, &tol()) else {
        panic!("Meterteile")
    };
    assert_eq!(cs.len(), 2);
    for c in &cs {
        assert!(c.period().is_some());
    }
    on_both(&cs, &a, &b);
}

#[test]
fn a_random_bump_cut_by_a_tilted_plane_is_fitted_quickly() {
    // Review I3: früher scheiterte die Annäherung eines langen Zweigs nach 19 s.
    let n = 13usize;
    let mut kn = vec![0.0; 4];
    for i in 1..=(n - 4) {
        kn.push(i as f64 / (n - 3) as f64);
    }
    kn.extend([1.0; 4]);
    let mut rng = Rng(99);
    let mut c = Vec::new();
    for i in 0..n {
        for j in 0..n {
            c.push(DVec3::new(
                i as f64 * 10.0 / 12.0,
                j as f64 * 10.0 / 12.0,
                (rng.next() - 0.5) * 2.0 * 1.53,
            ));
        }
    }
    let s = NurbsSurface::new(3, 3, kn.clone(), kn, n, n, c, vec![1.0; n * n]).unwrap();
    let bump = Surface::nurbs(s, false, false).unwrap();
    let pl = plane(
        DVec3::new(5.0, 5.0, 0.0),
        DVec3::new(0.2, 0.1, 1.0).normalize(),
    );
    let start = std::time::Instant::now();
    let SsiOutcome::Curves(cs) = intersect_nurbs(&bump, &pl, &tol()) else {
        panic!("Kurven erwartet")
    };
    assert!(
        start.elapsed().as_secs_f64() < 10.0,
        "{:?}",
        start.elapsed()
    );
    assert!(!cs.is_empty());
    on_both(&cs, &bump, &pl);
}

#[test]
fn invalid_analytic_surfaces_and_tolerance_contacts_are_named_like_the_analytic_solver() {
    // Review M1
    let cyl = nurbs_cylinder(DVec3::ZERO, DVec3::Z, 2.0, 10.0);
    let bad_cone = Surface::Cone {
        frame: Frame::WORLD,
        radius: 1.0,
        slope: f64::NAN,
    };
    let zero_plane = Surface::Plane {
        frame: Frame {
            origin: DVec3::ZERO,
            x: DVec3::ZERO,
            y: DVec3::ZERO,
            z: DVec3::ZERO,
        },
    };
    let thin = Surface::Cylinder {
        frame: Frame::from_origin_normal(DVec3::new(0.0, 0.0, 5.0), DVec3::X),
        radius: 1e-5,
    };
    for s in [bad_cone, zero_plane, thin] {
        match intersect_nurbs(&cyl, &s, &tol()) {
            SsiOutcome::Unsupported(msg) => assert!(msg.contains("Ungültige Fläche"), "{msg}"),
            other => panic!("{other:?}"),
        }
    }
    // Ebene 1e-9 neben der Berührlinie: innerhalb der Toleranz eine Berührung
    let near = plane(DVec3::new(2.0 + 1e-9, 0.0, 0.0), DVec3::X);
    assert!(matches!(
        intersect_nurbs(&cyl, &near, &tol()),
        SsiOutcome::Touch
    ));
    // Kreis mit r = 1,3e-3 am Pol einer Freiform-Kugel: die Ebene liegt 2,8e-7 unter dem Pol
    let sph = nurbs_sphere(DVec3::ZERO, DVec3::Z, DVec3::X, 3.0, 0.0);
    let cap = plane(
        DVec3::new(0.0, 0.0, (9.0f64 - 1.3e-3 * 1.3e-3).sqrt()),
        DVec3::Z,
    );
    assert!(matches!(
        intersect_nurbs(&sph, &cap, &tol()),
        SsiOutcome::Touch
    ));
}

// ---- Nachbesserung Task 8, Runde 2 (Review): kurze Bögen an Rändern und Ecken ----

/// Drehfläche einer Strecke `a → b` um die z-Achse, Naht bei `start`.
fn revolved_line(a: DVec3, b: DVec3, start: f64) -> Surface {
    let line = NurbsCurve::new(1, vec![0.0, 0.0, 1.0, 1.0], vec![a, b], vec![1.0; 2]).unwrap();
    Surface::nurbs(
        NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, start, TAU).unwrap(),
        true,
        false,
    )
    .unwrap()
}

/// Zufällige 13 × 13-Beule wie in `a_random_bump_cut_by_a_tilted_plane_is_fitted_quickly`.
fn random_bump(seed: u64) -> Surface {
    let n = 13usize;
    let mut kn = vec![0.0; 4];
    for i in 1..=(n - 4) {
        kn.push(i as f64 / (n - 3) as f64);
    }
    kn.extend([1.0; 4]);
    let mut rng = Rng(seed);
    let mut c = Vec::new();
    for i in 0..n {
        for j in 0..n {
            c.push(DVec3::new(
                i as f64 * 10.0 / 12.0,
                j as f64 * 10.0 / 12.0,
                (rng.next() - 0.5) * 2.0 * 1.53,
            ));
        }
    }
    let s = NurbsSurface::new(3, 3, kn.clone(), kn, n, n, c, vec![1.0; n * n]).unwrap();
    Surface::nurbs(s, false, false).unwrap()
}

/// Parameterlinie eines Rands: `t ∈ [0, 1]` ↦ `(u, v)`.
type EdgeParam = Box<dyn Fn(f64) -> (f64, f64)>;

/// Bodenwahrheit: Schnittpunkte der offenen Ränder der Freiformfläche `a` mit `b` (fein
/// abgetastet, Vorzeichenwechsel halbiert).
fn boundary_crossings(a: &Surface, b: &Surface) -> Vec<DVec3> {
    let Surface::Nurbs(d) = a else { unreachable!() };
    let ((u0, u1), (v0, v1)) = d.surface().domain();
    let mut edges: Vec<EdgeParam> = Vec::new();
    if !d.is_periodic_v() {
        edges.push(Box::new(move |t| (u0 + (u1 - u0) * t, v0)));
        edges.push(Box::new(move |t| (u0 + (u1 - u0) * t, v1)));
    }
    if !d.is_periodic_u() {
        edges.push(Box::new(move |t| (u0, v0 + (v1 - v0) * t)));
        edges.push(Box::new(move |t| (u1, v0 + (v1 - v0) * t)));
    }
    let n = 4000;
    let mut out = Vec::new();
    for e in &edges {
        let f = |t: f64| {
            let (u, v) = e(t);
            b.implicit(d.point(u, v))
        };
        let mut prev = f(0.0);
        for i in 1..=n {
            let (ta, tb) = ((i - 1) as f64 / n as f64, i as f64 / n as f64);
            let fb = f(tb);
            if prev * fb < 0.0 {
                let (mut lo, mut hi) = (ta, tb);
                for _ in 0..60 {
                    let m = 0.5 * (lo + hi);
                    if (f(m) < 0.0) == (prev < 0.0) {
                        lo = m
                    } else {
                        hi = m
                    }
                }
                let (u, v) = e(0.5 * (lo + hi));
                out.push(d.point(u, v));
            }
            prev = fb;
        }
    }
    out
}

/// Jede Randkreuzung liegt auf einer der Kurven (bis 1e-5), alle Kurvenpunkte auf beiden Flächen.
fn assert_boundary_covered(name: &str, a: &Surface, b: &Surface, r: &SsiOutcome) {
    let crossings = boundary_crossings(a, b);
    let SsiOutcome::Curves(cs) = r else {
        panic!(
            "{name}: Kurven erwartet ({} Randkreuzungen), erhalten {r:?}",
            crossings.len()
        )
    };
    let pts: Vec<DVec3> = cs.iter().flat_map(|c| sample(c, 4000)).collect();
    for x in &crossings {
        let d = pts
            .iter()
            .map(|p| p.distance(*x))
            .fold(f64::INFINITY, f64::min);
        assert!(
            d < 1e-5,
            "{name}: Randkreuzung {x:?} fehlt (Abstand {d:.1e})"
        );
    }
    on_both(cs, a, b);
}

#[test]
fn short_arcs_clipping_a_rim_are_found() {
    // Review C-new: Bögen von 0,02 … 0,2 mm Länge am Rand fehlten (None).
    let cyl = |s: f64| revolved_line(DVec3::new(2.0, 0.0, 0.0), DVec3::new(2.0, 0.0, 10.0), s);
    let cone = |s: f64| revolved_line(DVec3::new(2.0, 0.0, 0.0), DVec3::new(6.0, 0.0, 10.0), s);
    let sph = Surface::Sphere {
        frame: Frame::from_origin_normal(DVec3::new(2.0, 0.0, 10.999), DVec3::Z),
        radius: 1.0,
    };
    let rim_plane = plane(
        DVec3::new(2.0, 0.0, 9.9999),
        DVec3::new(1.0, 0.0, 1.0).normalize(),
    );
    for s in [0.0, 1.0] {
        let c = cyl(s);
        assert_boundary_covered(
            "Zylinder–Kugel",
            &c,
            &sph,
            &intersect_nurbs(&c, &sph, &tol()),
        );
    }
    for s in [0.0, 0.5, 1.0, 4.0] {
        let c = cyl(s);
        let r = intersect_nurbs(&c, &rim_plane, &tol());
        assert_boundary_covered("Zylinder–Ebene", &c, &rim_plane, &r);
    }
    for (x, starts) in [(5.999, vec![0.0, 0.5, 1.0]), (5.997, vec![1.0])] {
        let pl = plane(DVec3::new(x, 0.0, 0.0), DVec3::X);
        for s in starts {
            let c = cone(s);
            assert_boundary_covered("Kegel–Ebene", &c, &pl, &intersect_nurbs(&c, &pl, &tol()));
        }
    }
}

#[test]
fn a_corner_arc_of_a_bump_is_found() {
    // Review C-new: Der 17-µm-Bogen an der Ecke (10, 10) der Beule mit Seed 5 fehlte.
    let bump = random_bump(5);
    let pl = plane(
        DVec3::new(5.0, 5.0, 0.0),
        DVec3::new(0.2, 0.1, 1.0).normalize(),
    );
    let r = intersect_nurbs(&bump, &pl, &tol());
    assert_boundary_covered("Beule 5", &bump, &pl, &r);
    let SsiOutcome::Curves(cs) = &r else {
        unreachable!()
    };
    let corner = DVec3::new(9.9863, 10.0, -1.498);
    let near = cs
        .iter()
        .flat_map(|c| sample(c, 2000))
        .map(|p| p.distance(corner))
        .fold(f64::INFINITY, f64::min);
    assert!(near < 1e-3, "Eckbogen fehlt ({near})");
}

#[test]
fn random_rim_and_corner_clips_are_found() {
    // Review C-new: zufällige Anschnitte von Rändern und Ecken (Tiefe 1e-4 … 1e-2) gegen die
    // Bodenwahrheit aus feiner Abtastung der Ränder.
    let mut rng = Rng(808);
    let mut checked = 0;
    for k in 0..24 {
        let depth = 10f64.powf(-4.0 + 2.0 * rng.next());
        let alpha = rng.next() * TAU;
        let start = rng.next() * TAU;
        let (a, b, name) = match k % 4 {
            0 | 1 => {
                // Zylinder (r = 2) oder Kegel (oben r = 6), oberer Rand bei z = 10
                let top = if k % 4 == 0 { 2.0 } else { 6.0 };
                let s = revolved_line(DVec3::new(2.0, 0.0, 0.0), DVec3::new(top, 0.0, 10.0), start);
                let radial = DVec3::new(alpha.cos(), alpha.sin(), 0.0);
                let p = radial * top + DVec3::Z * 10.0;
                let other = if k % 8 < 4 {
                    let n = (radial + DVec3::Z * (0.5 + 1.5 * rng.next())).normalize();
                    plane(p - DVec3::Z * depth, n)
                } else {
                    let rho = 0.5 + 1.5 * rng.next();
                    Surface::Sphere {
                        frame: Frame::from_origin_normal(p + DVec3::Z * (rho - depth), DVec3::Z),
                        radius: rho,
                    }
                };
                (s, other, format!("Fall {k} (Rand, Tiefe {depth:.1e})"))
            }
            _ => {
                // Ecke einer Beule: Ebene knapp über bzw. unter einer Ecke, zufällig geneigt
                let bump = random_bump(100 + k as u64);
                let Surface::Nurbs(d) = &bump else {
                    unreachable!()
                };
                let (cu, cv) = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)][k % 4];
                let c = d.point(cu, cv);
                let n = d.normal(cu, cv);
                let t = n.any_orthonormal_vector();
                let tilt = (n + t * (rng.next() - 0.5)).normalize();
                let sign = if rng.next() < 0.5 { 1.0 } else { -1.0 };
                let pl = plane(c + n * (sign * depth), tilt);
                (bump, pl, format!("Fall {k} (Ecke, Tiefe {depth:.1e})"))
            }
        };
        let r = intersect_nurbs(&a, &b, &tol());
        if boundary_crossings(&a, &b).is_empty() {
            // kein Randschnitt in der Abtastung: nur Kurven auf beiden Flächen verlangt
            if let SsiOutcome::Curves(cs) = &r {
                on_both(cs, &a, &b);
            }
            continue;
        }
        checked += 1;
        match &r {
            SsiOutcome::Unsupported(msg) => panic!("{name}: {msg}"),
            _ => assert_boundary_covered(&name, &a, &b, &r),
        }
    }
    assert!(checked >= 18, "nur {checked} Fälle mit Randschnitt");
}

#[test]
fn transversal_and_disjoint_quadric_pairs_are_not_refused() {
    // Review I-B: früher abgelehnt.
    let cyl1 = revolved_line(DVec3::new(2.0, 0.0, 0.0), DVec3::new(2.0, 0.0, 10.0), 1.0);
    for r in [2.28, 2.1, 2.05, 2.01] {
        let s = Surface::Sphere {
            frame: Frame::from_origin_normal(DVec3::new(0.0, 0.0, 5.0), DVec3::Z),
            radius: r,
        };
        let SsiOutcome::Curves(cs) = intersect_nurbs(&cyl1, &s, &tol()) else {
            panic!("r = {r}")
        };
        assert_eq!(cs.len(), 2, "r = {r}");
        on_both(&cs, &cyl1, &s);
    }
    let cone = revolved_line(DVec3::new(1.0, 0.0, 0.0), DVec3::new(2.0, 0.0, 10.0), 0.0);
    let c13 = Surface::Cylinder {
        frame: Frame::WORLD,
        radius: 1.3,
    };
    let SsiOutcome::Curves(cs) = intersect_nurbs(&cone, &c13, &tol()) else {
        panic!("Kegel–Zylinder")
    };
    assert_eq!(cs.len(), 1);
    on_both(&cs, &cone, &c13);
    let cyl0 = revolved_line(DVec3::new(2.0, 0.0, 0.0), DVec3::new(2.0, 0.0, 10.0), 0.0);
    for r in [2.1, 2.05, 2.02, 2.01] {
        let c = Surface::Cylinder {
            frame: Frame::WORLD,
            radius: r,
        };
        assert!(
            matches!(intersect_nurbs(&cyl0, &c, &tol()), SsiOutcome::None),
            "r = {r}"
        );
    }
    let sph = nurbs_sphere(DVec3::ZERO, DVec3::Z, DVec3::X, 3.0, 0.0);
    for r in [3.05, 3.02, 3.01] {
        let s = Surface::Sphere {
            frame: Frame::WORLD,
            radius: r,
        };
        assert!(
            matches!(intersect_nurbs(&sph, &s, &tol()), SsiOutcome::None),
            "r = {r}"
        );
    }
}

// ---- Nachbesserung Task 8, Runde 3 (Review): sehr große und sehr kleine Teile ----

/// Freiform-Zylinder (r = 2·s, Höhe 10·s um die Mitte), gedreht und verschoben.
fn scaled_cylinder(s: f64, start: f64, rot: planora_math::DMat3, off: DVec3) -> Surface {
    let l = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![
            DVec3::new(2.0 * s, 0.0, -5.0 * s),
            DVec3::new(2.0 * s, 0.0, 5.0 * s),
        ],
        vec![1.0; 2],
    )
    .unwrap();
    let n = NurbsSurface::revolve(&l, DVec3::ZERO, DVec3::Z, start, TAU).unwrap();
    Surface::nurbs(n.transformed(|p| rot * p + off).unwrap(), true, false).unwrap()
}

#[test]
fn transversal_cuts_work_at_every_scale() {
    // Review Runde 3: Bei 1000 mm scheiterte die Annäherung (die Konvergenzschranke
    // 1e-9 · Diagonale lag über NURBS_FIT_TOL); jetzt ist sie auf 1 % davon begrenzt.
    use planora_math::DMat3;
    for s in [0.01, 1.0, 300.0, 1000.0] {
        let cyl = scaled_cylinder(s, 0.3, DMat3::IDENTITY, DVec3::ZERO);
        let a20 = 20f64.to_radians();
        let mut others = vec![
            (
                "Ebene 20°",
                plane(
                    DVec3::new(0.0, 0.0, 0.3 * s),
                    DVec3::new(a20.cos(), 0.0, a20.sin()),
                ),
            ),
            (
                "Kugel",
                Surface::Sphere {
                    frame: Frame::from_origin_normal(DVec3::new(2.0 * s * 1.7, 0.0, 0.0), DVec3::Z),
                    radius: 2.0 * s,
                },
            ),
            (
                "Zylinder 45°",
                Surface::Cylinder {
                    frame: Frame::from_origin_normal(
                        DVec3::new(1.2 * s, 0.0, 0.0),
                        DMat3::from_axis_angle(DVec3::X, 45f64.to_radians()) * DVec3::Z,
                    ),
                    radius: 1.4 * s,
                },
            ),
        ];
        if s == 0.01 || s == 1000.0 {
            others.push((
                "Freiform-Zylinder 20°",
                scaled_cylinder(
                    0.7 * s,
                    1.1,
                    DMat3::from_axis_angle(DVec3::X, a20),
                    DVec3::new(1.2 * s, 0.0, 0.0),
                ),
            ));
        }
        for (name, other) in &others {
            let SsiOutcome::Curves(cs) = intersect_nurbs(&cyl, other, &tol()) else {
                panic!(
                    "s = {s}, {name}: {:?}",
                    intersect_nurbs(&cyl, other, &tol())
                )
            };
            assert!(!cs.is_empty());
            on_both(&cs, &cyl, other);
        }
        // Beule (Seed 3, um die Mitte) gegen eine um 19° geneigte Ebene
        let bump = random_bump(3);
        let Surface::Nurbs(d) = &bump else {
            unreachable!()
        };
        let scaled = Surface::nurbs(
            d.surface()
                .transformed(|p| (p - DVec3::new(5.0, 5.0, 0.0)) * s)
                .unwrap(),
            false,
            false,
        )
        .unwrap();
        let pl = plane(DVec3::ZERO, DVec3::new(0.34, 0.0, 1.0).normalize());
        let SsiOutcome::Curves(cs) = intersect_nurbs(&scaled, &pl, &tol()) else {
            panic!("s = {s}, Beule")
        };
        on_both(&cs, &scaled, &pl);
    }
}

#[test]
fn arcs_within_the_tolerance_of_a_small_rim_are_found() {
    // Review Runde 3: bei 0,01–0,1 mm großen Teilen und Anschnitttiefe bis 1e-7 meldete der
    // Rechner „lässt sich nicht verfolgen“ (feste kleinste Schrittweite 1e-4 mm, Randstart
    // über den Rand hinaus); jetzt relativ zur Größe und mit Korrektur in der Ebene.
    use planora_math::DMat3;
    for s in [0.01, 0.1] {
        let cyl = scaled_cylinder(s, 0.3, DMat3::IDENTITY, DVec3::ZERO);
        for depth in [1e-6, 1e-7] {
            let pl = plane(
                DVec3::new(2.0 * s, 0.0, 5.0 * s - depth),
                DVec3::new(1.0, 0.0, 1.0).normalize(),
            );
            let sph = Surface::Sphere {
                frame: Frame::from_origin_normal(
                    DVec3::new(2.0 * s, 0.0, 5.5 * s - depth),
                    DVec3::Z,
                ),
                radius: 0.5 * s,
            };
            for other in [pl, sph] {
                let r = intersect_nurbs(&cyl, &other, &tol());
                assert_boundary_covered(&format!("s = {s}, Tiefe {depth}"), &cyl, &other, &r);
            }
        }
    }
}

// ---- Nachbesserung Task 8, Runde 4 (Review) ----

/// Freiform-Zylinder mit Achse `axis` durch `centre`, Länge `len`, Naht `seam`.
fn axis_cylinder(r: f64, len: f64, seam: f64, axis: DVec3, centre: DVec3) -> Surface {
    let s = NurbsSurface::revolve(
        &NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![
                DVec3::new(r, 0.0, -len / 2.0),
                DVec3::new(r, 0.0, len / 2.0),
            ],
            vec![1.0; 2],
        )
        .unwrap(),
        DVec3::ZERO,
        DVec3::Z,
        seam,
        TAU,
    )
    .unwrap();
    let z = axis.normalize();
    let x = z.any_orthonormal_vector();
    let m = planora_math::DMat3::from_cols(x, z.cross(x), z);
    Surface::nurbs(s.transformed(|p| m * p + centre).unwrap(), true, false).unwrap()
}

/// Kegel von r0 = 2 (z = −5) nach `r1` (z = 5), Naht `seam`.
fn rim_cone(r1: f64, seam: f64) -> Surface {
    revolved_line(DVec3::new(2.0, 0.0, -5.0), DVec3::new(r1, 0.0, 5.0), seam)
}

/// Schnittpunkte des oberen Kegelrands (z = 5) mit `b`, fein abgetastet und auf |F| < 1e-6
/// nachgehalbiert (Vorzeichensprünge jenseits einer beschränkten Fläche zählen nicht).
fn rim_crossings(a: &Surface, b: &Surface) -> Vec<DVec3> {
    boundary_crossings(a, b)
        .into_iter()
        .filter(|x| b.implicit(*x).abs() < 1e-6)
        .collect()
}

#[test]
fn rim_clip_curves_end_on_the_rim_crossing() {
    // Review Runde 4, C(i): Kurvenenden lagen bis 5e-5 neben dem wahren Randschnitt (flach an
    // den Rand laufende Kurve); jetzt werden die Enden auf den Rand gelegt.
    let a = rim_cone(3.3945182100475373, 0.87917892395190);
    let b = axis_cylinder(
        0.6180198733356118,
        4.0,
        5.20000043715323,
        DVec3::new(0.75356856895195, -0.65736931164126, 0.0),
        DVec3::new(-2.18100574332633, -2.60114740565042, 5.61800987333561),
    );
    let SsiOutcome::Curves(cs) = intersect_nurbs(&a, &b, &tol()) else {
        panic!("Kurven erwartet")
    };
    let ends: Vec<DVec3> = cs
        .iter()
        .flat_map(|c| {
            let (t0, t1) = c.domain().unwrap();
            [c.point(t0), c.point(t1)]
        })
        .collect();
    let crossings = rim_crossings(&a, &b);
    assert!(!crossings.is_empty());
    for x in crossings {
        let d = ends
            .iter()
            .map(|e| e.distance(x))
            .fold(f64::INFINITY, f64::min);
        assert!(d < 1e-6, "Randschnitt {x:?}: Kurvenende {d:.1e} entfernt");
    }
    on_both(&cs, &a, &b);
}

#[test]
fn two_shallow_rim_arcs_are_both_found_or_named() {
    // Review Runde 4, D: Ein Freiform-Zylinder liegt quer über dem Rand eines Freiform-Kegels
    // und taucht `depth` unter die Randebene: zwei gleich tiefe Bögen. Ab 1e-5 werden beide
    // gefunden; bis zur Toleranz darf der Rechner benannt ablehnen, aber nie nur einen Bogen
    // liefern.
    let a = rim_cone(2.8754815284568243, 4.49397539284209);
    let r = 0.4978751058286492;
    for depth in [1e-7, 1e-6, 1e-5, 1e-4] {
        let b = axis_cylinder(
            r,
            4.0,
            2.81666411049440,
            DVec3::new(-0.95324478063366, 0.30219925247207, 0.0),
            DVec3::new(0.39517508450724, 2.84819779388319, 5.0 + r - depth),
        );
        let res = intersect_nurbs(&a, &b, &tol());
        match &res {
            SsiOutcome::Curves(cs) => {
                let ends: Vec<DVec3> = cs
                    .iter()
                    .flat_map(|c| {
                        let (t0, t1) = c.domain().unwrap();
                        [c.point(t0), c.point(t1)]
                    })
                    .collect();
                let crossings = rim_crossings(&a, &b);
                assert_eq!(crossings.len(), 4, "Tiefe {depth}");
                for x in crossings {
                    let d = ends
                        .iter()
                        .map(|e| e.distance(x))
                        .fold(f64::INFINITY, f64::min);
                    assert!(d < 1e-6, "Tiefe {depth}: Randschnitt {x:?} fehlt ({d:.1e})");
                }
                on_both(cs, &a, &b);
            }
            SsiOutcome::Unsupported(msg) if depth <= 1e-6 => {
                assert!(msg.contains("tangential"), "Tiefe {depth}: {msg}")
            }
            SsiOutcome::Touch if depth <= 1e-6 => {}
            other => panic!("Tiefe {depth}: {other:?}"),
        }
    }
}

#[test]
fn a_large_tilted_cylinder_against_a_plane() {
    // Review Runde 4, B (Eingabe des Reviews; in meiner Nachbildung schon richtig).
    let s = NurbsSurface::revolve(
        &NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![
                DVec3::new(600.0, 0.0, -1500.0),
                DVec3::new(600.0, 0.0, 1500.0),
            ],
            vec![1.0; 2],
        )
        .unwrap(),
        DVec3::ZERO,
        DVec3::Z,
        5.39464193204463,
        TAU,
    )
    .unwrap();
    let m = planora_math::DMat3::from_cols(
        DVec3::new(0.605122, 0.531300, 0.592915),
        DVec3::new(0.061895, 0.711094, -0.700367),
        DVec3::new(-0.793723, 0.460506, 0.397413),
    );
    let off = DVec3::new(99.472644, -254.789313, 2.296020);
    let pl = plane(
        DVec3::new(100.73215, -255.52006, 1.66539),
        DVec3::new(0.297159, 0.656761, 0.693081).normalize(),
    );
    // Review Runde 5, B (erste Eingabe): die Matrix nach Gram-Schmidt orthonormiert.
    let x = m.x_axis.normalize();
    let y = (m.y_axis - x * m.y_axis.dot(x)).normalize();
    let gs = planora_math::DMat3::from_cols(x, y, x.cross(y));
    for m in [m, m.transpose(), gs] {
        let a = Surface::nurbs(s.transformed(|p| m * p + off).unwrap(), true, false).unwrap();
        let SsiOutcome::Curves(cs) = intersect_nurbs(&a, &pl, &tol()) else {
            panic!("Kurven erwartet")
        };
        on_both(&cs, &a, &pl);
    }
}

// ---- Nachbesserung Task 8, Runde 5 (Review) ----

/// Randschnitte von `a` (oberer Rand) mit `b`, die von Kurvenenden auf 1e-6 getroffen werden,
/// und ihre Anzahl.
fn rim_hits(a: &Surface, b: &Surface, cs: &[Curve]) -> (usize, usize) {
    let ends: Vec<DVec3> = cs
        .iter()
        .flat_map(|c| {
            let (t0, t1) = c.domain().unwrap();
            [c.point(t0), c.point(t1)]
        })
        .collect();
    let crossings = rim_crossings(a, b);
    let hit = crossings
        .iter()
        .filter(|x| ends.iter().any(|e| e.distance(**x) < 1e-6))
        .count();
    (hit, crossings.len())
}

/// Ein Freiform-Zylinder (Radius `r`, Länge 8) quer über dem oberen Rand eines Freiform-Kegels
/// (`r1`, Naht `seam_a`), um `depth` unter die Randebene getaucht; Sehne unter dem Winkel `phi`.
fn shallow_rim_case(
    r1: f64,
    seam_a: f64,
    phi: f64,
    alpha: f64,
    r: f64,
    seam_b: f64,
    depth: f64,
) -> (Surface, Surface) {
    let a = rim_cone(r1, seam_a);
    let dir = DVec3::new(phi.cos(), phi.sin(), 0.0);
    let nrm = DVec3::new(-phi.sin(), phi.cos(), 0.0);
    let centre = nrm * (r1 * (alpha / 2.0).cos()) + DVec3::Z * (5.0 + r - depth);
    (a, axis_cylinder(r, 8.0, seam_b, dir, centre))
}

/// Prüft eine Rand-Streifung: Tiefe über `10 · tol.linear` verlangt alle Bögen, darunter alle
/// Bögen oder eine benannte Ablehnung / Berührung; nie Teilergebnisse oder „kein Schnitt“.
fn check_shallow(name: &str, a: &Surface, b: &Surface, depth: f64) {
    let res = intersect_nurbs(a, b, &tol());
    match &res {
        SsiOutcome::Curves(cs) => {
            let (hit, n) = rim_hits(a, b, cs);
            assert!(n >= 2 && hit == n, "{name}: {hit} von {n} Randschnitten");
            on_both(cs, a, b);
        }
        SsiOutcome::Unsupported(msg) if depth < 1e-5 => {
            assert!(msg.contains("tangential"), "{name}: {msg}")
        }
        SsiOutcome::Touch if depth < 1e-5 => {}
        other => panic!("{name} (Tiefe {depth:e}): {other:?}"),
    }
}

#[test]
fn shallow_rim_crossings_of_two_freeform_surfaces_are_all_found() {
    // Review Runde 5, D: zwei Eingaben, bei denen Randstücke gegen eine Freiformfläche nur
    // geschätzt „höchstens eine Nullstelle“ hatten und ein Bogen fehlte.
    let a = revolved_line(
        DVec3::new(2.0, 0.0, -5.0),
        DVec3::new(2.9323886127917342, 0.0, 5.0),
        3.844505991122473,
    );
    let b = axis_cylinder(
        0.9967615504231817,
        8.066634698818216,
        1.3509948157797407,
        DVec3::new(-0.9896545150661898, -0.14347104519416007, 0.0),
        DVec3::new(-0.2118910544972401, 1.4616115642117415, 5.996751550423182),
    );
    check_shallow("Eingabe 1 (Tiefe 1e-5)", &a, &b, 1e-5);
    let a = rim_cone(2.9746381476929447, 5.770924841681106);
    let b = axis_cylinder(
        0.4715909498579281,
        8.029830556680029,
        5.621140119587627,
        DVec3::new(0.6993812961889876, 0.7147487688279088, 0.0),
        DVec3::new(1.1354551340676315, -1.111042254372685, 5.471587949857928),
    );
    check_shallow("Eingabe 2 (Tiefe 3e-6)", &a, &b, 3e-6);
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn shallow_rim_set(cases: usize) {
    let mut rng = Lcg(555);
    for k in 0..cases {
        let r1 = 2.5 + rng.next();
        let seam_a = rng.next() * TAU;
        let phi = rng.next() * TAU;
        let alpha = (3.0 + 57.0 * rng.next()).to_radians();
        let r = 0.3 + 0.7 * rng.next();
        let depth = [1e-3, 1e-4, 3e-5, 1e-5, 3e-6, 1e-6][k % 6];
        let seam_b = rng.next() * TAU;
        let (a, b) = shallow_rim_case(r1, seam_a, phi, alpha, r, seam_b, depth);
        check_shallow(&format!("Fall {k}"), &a, &b, depth);
    }
}

#[test]
fn seeded_shallow_rim_crossings_small() {
    shallow_rim_set(6);
}

#[test]
#[ignore = "40 Fälle, etwa 80 s im Release-Build"]
fn seeded_shallow_rim_crossings_40() {
    shallow_rim_set(40);
}

#[test]
fn a_scaled_freeform_cylinder_against_an_analytic_cylinder() {
    // Review Runde 5, B (zweite Eingabe): Maßstab 300, lange Schnittkurve.
    let r = planora_math::DMat3::from_cols(
        DVec3::new(0.5870045120605493, 0.38069675143709336, 0.7144898083708403),
        DVec3::new(-0.8094678277565011, 0.26106359155993886, 0.525935012134576),
        DVec3::new(
            0.013694475080397228,
            -0.8870827383097188,
            0.46140727860015707,
        ),
    );
    let t = DVec3::new(
        0.19481279522151462,
        -0.5600458562706905,
        -0.9333031311345372,
    );
    let s = NurbsSurface::revolve(
        &NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![
                DVec3::new(2.554591113060023, 0.0, -4.542508659999205),
                DVec3::new(2.554591113060023, 0.0, 4.542508659999205),
            ],
            vec![1.0; 2],
        )
        .unwrap(),
        DVec3::ZERO,
        DVec3::Z,
        0.8599716301799407,
        TAU,
    )
    .unwrap();
    let a = Surface::nurbs(s.transformed(|p| (r * p + t) * 300.0).unwrap(), true, false).unwrap();
    let c = Surface::Cylinder {
        frame: Frame::from_origin_normal(
            DVec3::new(62.141328651864214, -447.44786143791134, 387.4865103610493),
            DVec3::new(
                -0.5928358072025249,
                -0.19363267204946769,
                -0.7816982115967223,
            ),
        ),
        radius: 487.5068642961147,
    };
    let SsiOutcome::Curves(cs) = intersect_nurbs(&a, &c, &tol()) else {
        panic!("Kurven erwartet")
    };
    on_both(&cs, &a, &c);
}
