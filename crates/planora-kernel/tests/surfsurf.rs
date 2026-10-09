use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn plane(o: DVec3, n: DVec3) -> Surface {
    Surface::Plane {
        frame: Frame::from_origin_normal(o, n),
    }
}

fn cyl(o: DVec3, axis: DVec3, r: f64) -> Surface {
    Surface::Cylinder {
        frame: Frame::from_origin_normal(o, axis),
        radius: r,
    }
}

fn cone(o: DVec3, axis: DVec3, r: f64, slope: f64) -> Surface {
    Surface::Cone {
        frame: Frame::from_origin_normal(o, axis),
        radius: r,
        slope,
    }
}

fn sphere(c: DVec3, r: f64) -> Surface {
    Surface::Sphere {
        frame: Frame::from_origin_normal(c, DVec3::Z),
        radius: r,
    }
}

fn torus(o: DVec3, axis: DVec3, major: f64, minor: f64) -> Surface {
    Surface::Torus {
        frame: Frame::from_origin_normal(o, axis),
        major,
        minor,
    }
}

fn curves(r: SurfaceIntersection) -> Vec<Curve> {
    match r {
        SurfaceIntersection::Curves(c) => c,
        other => panic!("Kurven erwartet, bekam {other:?}"),
    }
}

/// Alle abgetasteten Punkte der Kurven liegen auf beiden Flächen. `ts` bestimmt den Bereich bei Geraden.
fn check_on_both(cs: &[Curve], a: &Surface, b: &Surface, line_range: (f64, f64)) {
    for c in cs {
        let n = 24;
        for i in 0..n {
            let t = match c {
                Curve::Line { .. } => {
                    line_range.0 + (line_range.1 - line_range.0) * i as f64 / n as f64
                }
                _ => TAU * i as f64 / n as f64,
            };
            let p = c.point(t);
            assert!(
                a.implicit(p).abs() < 1e-8,
                "{c:?} t={t} nicht auf A: {}",
                a.implicit(p)
            );
            assert!(
                b.implicit(p).abs() < 1e-8,
                "{c:?} t={t} nicht auf B: {}",
                b.implicit(p)
            );
        }
    }
}

fn both_orders(a: &Surface, b: &Surface) -> (SurfaceIntersection, SurfaceIntersection) {
    (
        intersect_surfaces(a, b, &tol()),
        intersect_surfaces(b, a, &tol()),
    )
}

#[test]
fn plane_plane() {
    let a = plane(DVec3::ZERO, DVec3::Z);
    let b = plane(DVec3::new(1.0, 0.0, 0.0), DVec3::X);
    let (ab, ba) = both_orders(&a, &b);
    let c = curves(ab);
    assert_eq!(c.len(), 1);
    check_on_both(&c, &a, &b, (-5.0, 5.0));
    assert_eq!(curves(ba).len(), 1);
    // parallel getrennt, identisch
    let p2 = plane(DVec3::new(0.0, 0.0, 1.0), DVec3::Z);
    assert!(matches!(
        intersect_surfaces(&a, &p2, &tol()),
        SurfaceIntersection::None
    ));
    assert!(matches!(
        intersect_surfaces(&a, &a, &tol()),
        SurfaceIntersection::Coincident
    ));
    // entgegengesetzte Normale, gleiche Ebene
    let flipped = plane(DVec3::ZERO, -DVec3::Z);
    assert!(matches!(
        intersect_surfaces(&a, &flipped, &tol()),
        SurfaceIntersection::Coincident
    ));
}

#[test]
fn plane_cylinder_all_orientations() {
    let cy = cyl(DVec3::ZERO, DVec3::Z, 2.0);
    // Achse senkrecht zur Ebene → Kreis
    let p = plane(DVec3::new(0.0, 0.0, 3.0), DVec3::Z);
    let (ab, ba) = both_orders(&p, &cy);
    let c = curves(ab);
    assert_eq!(c.len(), 1);
    assert!(matches!(&c[0], Curve::Circle { radius, .. } if (radius - 2.0).abs() < 1e-9));
    check_on_both(&c, &p, &cy, (0.0, 0.0));
    assert_eq!(curves(ba).len(), 1);
    // Achse parallel zur Ebene → zwei Geraden
    let pv = plane(DVec3::new(1.0, 0.0, 0.0), DVec3::X);
    let c = curves(intersect_surfaces(&pv, &cy, &tol()));
    assert_eq!(c.len(), 2);
    assert!(c.iter().all(|k| matches!(k, Curve::Line { .. })));
    check_on_both(&c, &pv, &cy, (-4.0, 4.0));
    // tangential und weit weg
    let tan = plane(DVec3::new(2.0, 0.0, 0.0), DVec3::X);
    assert!(matches!(
        intersect_surfaces(&tan, &cy, &tol()),
        SurfaceIntersection::Touch
    ));
    let far = plane(DVec3::new(5.0, 0.0, 0.0), DVec3::X);
    assert!(matches!(
        intersect_surfaces(&far, &cy, &tol()),
        SurfaceIntersection::None
    ));
    // schräg → Ellipse mit großer Halbachse r / cos(θ)
    let n = DVec3::new(0.0, 0.5, 1.0).normalize();
    let obl = plane(DVec3::ZERO, n);
    let c = curves(intersect_surfaces(&obl, &cy, &tol()));
    assert_eq!(c.len(), 1);
    let Curve::Ellipse { major, minor, .. } = &c[0] else {
        panic!("Ellipse erwartet")
    };
    assert!((minor - 2.0).abs() < 1e-9);
    assert!((major - 2.0 / n.z).abs() < 1e-9);
    check_on_both(&c, &obl, &cy, (0.0, 0.0));
}

#[test]
fn plane_cone_circle_and_generators() {
    let co = cone(DVec3::ZERO, DVec3::Z, 3.0, -0.5); // Spitze bei z = 6
    let p = plane(DVec3::new(0.0, 0.0, 2.0), DVec3::Z);
    let c = curves(intersect_surfaces(&p, &co, &tol()));
    assert_eq!(c.len(), 1);
    assert!(matches!(&c[0], Curve::Circle { radius, .. } if (radius - 2.0).abs() < 1e-9));
    check_on_both(&c, &p, &co, (0.0, 0.0));
    // Ebene enthält die Achse → zwei Erzeugende (nur der Ast mit positivem Radius wird geprüft)
    let pa = plane(DVec3::ZERO, DVec3::Y);
    let c = curves(intersect_surfaces(&pa, &co, &tol()));
    assert_eq!(c.len(), 2);
    check_on_both(&c, &pa, &co, (0.0, 2.0));
    // schräge Ebene: noch nicht unterstützt
    let obl = plane(DVec3::new(0.0, 0.0, 1.0), DVec3::new(0.3, 0.0, 1.0));
    assert!(matches!(
        intersect_surfaces(&obl, &co, &tol()),
        SurfaceIntersection::Unsupported
    ));
}

#[test]
fn plane_sphere() {
    let s = sphere(DVec3::new(1.0, 2.0, 3.0), 5.0);
    let p = plane(DVec3::new(0.0, 0.0, 6.0), DVec3::new(0.0, 0.0, 1.0));
    let c = curves(intersect_surfaces(&p, &s, &tol()));
    assert!(matches!(&c[0], Curve::Circle { radius, .. } if (radius - 4.0).abs() < 1e-9));
    check_on_both(&c, &p, &s, (0.0, 0.0));
    let tangent = plane(DVec3::new(0.0, 0.0, 8.0), DVec3::Z);
    assert!(matches!(
        intersect_surfaces(&tangent, &s, &tol()),
        SurfaceIntersection::Touch
    ));
    let far = plane(DVec3::new(0.0, 0.0, 20.0), DVec3::Z);
    assert!(matches!(
        intersect_surfaces(&far, &s, &tol()),
        SurfaceIntersection::None
    ));
    // beliebig geneigte Ebene durch die Kugel
    let tilted = plane(DVec3::new(1.0, 2.0, 3.0), DVec3::new(1.0, 1.0, 1.0));
    let c = curves(intersect_surfaces(&tilted, &s, &tol()));
    check_on_both(&c, &tilted, &s, (0.0, 0.0));
}

#[test]
fn plane_torus() {
    let t = torus(DVec3::ZERO, DVec3::Z, 5.0, 1.5);
    let p = plane(DVec3::new(0.0, 0.0, 0.5), DVec3::Z);
    let c = curves(intersect_surfaces(&p, &t, &tol()));
    assert_eq!(c.len(), 2);
    check_on_both(&c, &p, &t, (0.0, 0.0));
    let through = plane(DVec3::ZERO, DVec3::Y);
    let c = curves(intersect_surfaces(&through, &t, &tol()));
    assert_eq!(c.len(), 2);
    check_on_both(&c, &through, &t, (0.0, 0.0));
    let obl = plane(DVec3::ZERO, DVec3::new(0.3, 0.0, 1.0));
    assert!(matches!(
        intersect_surfaces(&obl, &t, &tol()),
        SurfaceIntersection::Unsupported
    ));
}

#[test]
fn sphere_sphere() {
    let a = sphere(DVec3::ZERO, 3.0);
    let b = sphere(DVec3::new(4.0, 0.0, 0.0), 2.0);
    let c = curves(intersect_surfaces(&a, &b, &tol()));
    assert_eq!(c.len(), 1);
    check_on_both(&c, &a, &b, (0.0, 0.0));
    assert!(matches!(
        intersect_surfaces(&a, &sphere(DVec3::new(5.0, 0.0, 0.0), 2.0), &tol()),
        SurfaceIntersection::Touch
    ));
    assert!(matches!(
        intersect_surfaces(&a, &sphere(DVec3::new(9.0, 0.0, 0.0), 2.0), &tol()),
        SurfaceIntersection::None
    ));
    assert!(matches!(
        intersect_surfaces(&a, &a, &tol()),
        SurfaceIntersection::Coincident
    ));
    assert!(matches!(
        intersect_surfaces(&a, &sphere(DVec3::ZERO, 1.0), &tol()),
        SurfaceIntersection::None
    ));
}

#[test]
fn coaxial_surfaces_intersect_in_circles() {
    let cy = cyl(DVec3::ZERO, DVec3::Z, 2.0);
    let sp = sphere(DVec3::new(0.0, 0.0, 1.0), 3.0);
    let c = curves(intersect_surfaces(&cy, &sp, &tol()));
    assert_eq!(c.len(), 2);
    check_on_both(&c, &cy, &sp, (0.0, 0.0));
    let co = cone(DVec3::ZERO, DVec3::Z, 4.0, -0.5);
    let c = curves(intersect_surfaces(&cy, &co, &tol()));
    assert_eq!(c.len(), 1);
    check_on_both(&c, &cy, &co, (0.0, 0.0));
    let co2 = cone(DVec3::ZERO, DVec3::Z, 3.0, -0.5);
    let c = curves(intersect_surfaces(&co2, &sp, &tol()));
    assert_eq!(c.len(), 2);
    check_on_both(&c, &co2, &sp, (0.0, 0.0));
    assert!(matches!(
        intersect_surfaces(&co, &sp, &tol()),
        SurfaceIntersection::None
    ));
    let to = torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 5.0, 1.5);
    let c = curves(intersect_surfaces(
        &cy,
        &torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 2.5, 1.0),
        &tol(),
    ));
    check_on_both(
        &c,
        &cy,
        &torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 2.5, 1.0),
        (0.0, 0.0),
    );
    let c = curves(intersect_surfaces(
        &sp,
        &torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 2.5, 1.0),
        &tol(),
    ));
    check_on_both(
        &c,
        &sp,
        &torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 2.5, 1.0),
        (0.0, 0.0),
    );
    let c = curves(intersect_surfaces(
        &to,
        &torus(DVec3::new(0.0, 0.0, 2.0), DVec3::Z, 5.0, 1.5),
        &tol(),
    ));
    check_on_both(
        &c,
        &to,
        &torus(DVec3::new(0.0, 0.0, 2.0), DVec3::Z, 5.0, 1.5),
        (0.0, 0.0),
    );
    // gleiche Zylinder / unterschiedliche Radien
    assert!(matches!(
        intersect_surfaces(&cy, &cy, &tol()),
        SurfaceIntersection::Coincident
    ));
    assert!(matches!(
        intersect_surfaces(&cy, &cyl(DVec3::ZERO, DVec3::Z, 3.0), &tol()),
        SurfaceIntersection::None
    ));
    // umgekehrte Achsrichtung ändert nichts
    let cy_down = cyl(DVec3::ZERO, -DVec3::Z, 2.0);
    let c = curves(intersect_surfaces(&cy_down, &sp, &tol()));
    check_on_both(&c, &cy_down, &sp, (0.0, 0.0));
}

#[test]
fn parallel_cylinders_give_lines() {
    let a = cyl(DVec3::ZERO, DVec3::Z, 2.0);
    let b = cyl(DVec3::new(3.0, 0.0, 0.0), DVec3::Z, 2.0);
    let c = curves(intersect_surfaces(&a, &b, &tol()));
    assert_eq!(c.len(), 2);
    check_on_both(&c, &a, &b, (-3.0, 3.0));
    assert!(matches!(
        intersect_surfaces(&a, &cyl(DVec3::new(4.0, 0.0, 0.0), DVec3::Z, 2.0), &tol()),
        SurfaceIntersection::Touch
    ));
    assert!(matches!(
        intersect_surfaces(&a, &cyl(DVec3::new(9.0, 0.0, 0.0), DVec3::Z, 2.0), &tol()),
        SurfaceIntersection::None
    ));
}

#[test]
fn unsupported_pairs_are_reported() {
    let a = cyl(DVec3::ZERO, DVec3::Z, 2.0);
    let b = cyl(DVec3::ZERO, DVec3::X, 2.0);
    assert!(matches!(
        intersect_surfaces(&a, &b, &tol()),
        SurfaceIntersection::Unsupported
    ));
    let off_sphere = sphere(DVec3::new(0.5, 0.0, 0.0), 3.0);
    assert!(matches!(
        intersect_surfaces(&a, &off_sphere, &tol()),
        SurfaceIntersection::Unsupported
    ));
}

#[test]
fn surface_names_are_german() {
    assert_eq!(surface_name(&plane(DVec3::ZERO, DVec3::Z)), "Ebene");
    assert_eq!(surface_name(&sphere(DVec3::ZERO, 1.0)), "Kugel");
    assert_eq!(surface_name(&cyl(DVec3::ZERO, DVec3::Z, 1.0)), "Zylinder");
}

fn kind(r: &SurfaceIntersection) -> &'static str {
    match r {
        SurfaceIntersection::None => "None",
        SurfaceIntersection::Touch => "Touch",
        SurfaceIntersection::Coincident => "Coincident",
        SurfaceIntersection::Curves(_) => "Curves",
        SurfaceIntersection::Unsupported => "Unsupported",
    }
}

/// Beide Argumentreihenfolgen müssen dieselbe Kategorie liefern.
fn kind_both(a: &Surface, b: &Surface) -> &'static str {
    let (ab, ba) = both_orders(a, b);
    assert_eq!(
        kind(&ab),
        kind(&ba),
        "Kategorie hängt von der Reihenfolge ab"
    );
    kind(&ab)
}

#[test]
fn near_tangent_cylinder_sphere_is_consistent() {
    let cy = cyl(DVec3::ZERO, DVec3::Z, 2.0);
    // Kugel berührt den Zylinder entlang eines Kreises
    assert_eq!(kind_both(&cy, &sphere(DVec3::ZERO, 2.0 + 1e-8)), "Touch");
    assert_eq!(kind_both(&cy, &sphere(DVec3::ZERO, 2.0 - 1e-8)), "Touch");
    // deutlich außerhalb der Toleranz
    assert_eq!(kind_both(&cy, &sphere(DVec3::ZERO, 2.0 + 1e-4)), "Curves");
    assert_eq!(kind_both(&cy, &sphere(DVec3::ZERO, 2.0 - 1e-4)), "None");
    let s = sphere(DVec3::ZERO, 2.0 + 1e-4);
    check_on_both(
        &curves(intersect_surfaces(&cy, &s, &tol())),
        &cy,
        &s,
        (0.0, 0.0),
    );
}

#[test]
fn near_tangent_cone_sphere_and_sphere_torus() {
    let co = cone(DVec3::ZERO, DVec3::Z, 3.0, -0.5);
    let d = 3.0 / 1.25f64.sqrt(); // Abstand des Ursprungs von der Mantellinie
    for (gap, expected) in [
        (-2e-6, "None"),
        (-0.5e-6, "Touch"),
        (0.0, "Touch"),
        (0.5e-6, "Touch"),
        (2e-6, "Curves"),
    ] {
        assert_eq!(
            kind_both(&co, &sphere(DVec3::ZERO, d + gap)),
            expected,
            "Kegel/Kugel gap {gap}"
        );
    }
    // Kugel um den Ursprung gegen Torus (Hauptradius 5, Nebenradius 1): außen bei R = 4, innen bei R = 6
    let to = torus(DVec3::ZERO, DVec3::Z, 5.0, 1.0);
    for (r, expected) in [
        (4.0 - 2e-6, "None"),
        (4.0 - 0.5e-6, "Touch"),
        (4.0 + 0.5e-6, "Touch"),
        (4.0 + 2e-6, "Curves"),
        (6.0 - 2e-6, "Curves"),
        (6.0 - 0.5e-6, "Touch"),
        (6.0 + 0.5e-6, "Touch"),
        (6.0 + 2e-6, "None"),
    ] {
        assert_eq!(
            kind_both(&sphere(DVec3::ZERO, r), &to),
            expected,
            "Kugel/Torus R {r}"
        );
    }
    // Zylinder gegen Torus: Mantellinie ρ = 6 berührt den Meridiankreis (5, 0, 1)
    let cy = |r: f64| cyl(DVec3::ZERO, DVec3::Z, r);
    assert_eq!(kind_both(&cy(6.0 + 0.5e-6), &to), "Touch");
    assert_eq!(kind_both(&cy(6.0 + 2e-6), &to), "None");
    assert_eq!(kind_both(&cy(6.0 - 2e-6), &to), "Curves");
}

#[test]
fn reversed_axes_give_same_results() {
    let sp = sphere(DVec3::new(0.0, 0.0, 1.0), 3.0);
    let cy = cyl(DVec3::ZERO, DVec3::Z, 2.0);
    // Kegel ρ = 3 − h/2: mit umgekehrter Achse identisch durch (Radius 3, Steigung +0.5)
    let co = cone(DVec3::ZERO, DVec3::Z, 3.0, -0.5);
    let co_rev = cone(DVec3::ZERO, -DVec3::Z, 3.0, 0.5);
    assert_eq!(kind_both(&co, &co_rev), "Coincident");
    for other in [&cy, &sp] {
        assert_eq!(kind_both(&co, other), kind_both(&co_rev, other));
        let (ab, ba) = both_orders(&co_rev, other);
        check_on_both(&curves(ab), &co_rev, other, (0.0, 0.0));
        check_on_both(&curves(ba), &co_rev, other, (0.0, 0.0));
    }
    let (ab, _) = both_orders(&co_rev, &cy);
    assert_eq!(curves(ab).len(), 1);
    // Torus mit umgekehrter Achse
    let to = torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 2.5, 1.0);
    let to_rev = torus(DVec3::new(0.0, 0.0, 1.0), -DVec3::Z, 2.5, 1.0);
    assert_eq!(kind_both(&to, &to_rev), "Coincident");
    for other in [&cy, &sp, &co, &co_rev] {
        assert_eq!(kind_both(&to, other), kind_both(&to_rev, other));
        let (ab, ba) = both_orders(&to_rev, other);
        check_on_both(&curves(ab), &to_rev, other, (0.0, 0.0));
        check_on_both(&curves(ba), &to_rev, other, (0.0, 0.0));
    }
    // verschobener Ursprung bei umgekehrter Achse: gleiche Fläche wie co, Ursprung bei der Spitze
    let co_apex = cone(DVec3::new(0.0, 0.0, 6.0), -DVec3::Z, 0.0, 0.5);
    assert_eq!(kind_both(&co, &co_apex), "Coincident");
}

#[test]
fn coincident_and_inside_touching() {
    let co = cone(DVec3::ZERO, DVec3::Z, 3.0, -0.5);
    assert!(matches!(
        intersect_surfaces(&co, &co, &tol()),
        SurfaceIntersection::Coincident
    ));
    let to = torus(DVec3::new(0.0, 0.0, 1.0), DVec3::Z, 5.0, 1.5);
    assert!(matches!(
        intersect_surfaces(&to, &to, &tol()),
        SurfaceIntersection::Coincident
    ));
    // Kugel im Kegel, berührt ihn entlang eines Kreises
    let d = 3.0 / 1.25f64.sqrt();
    assert_eq!(kind_both(&co, &sphere(DVec3::ZERO, d)), "Touch");
}

#[test]
fn plane_through_cone_apex_touches() {
    let co = cone(DVec3::ZERO, DVec3::Z, 3.0, -0.5); // Spitze bei z = 6
    let at = |z: f64| plane(DVec3::new(0.0, 0.0, z), DVec3::Z);
    assert_eq!(kind_both(&at(6.0), &co), "Touch");
    assert_eq!(kind_both(&at(6.0 + 0.5e-6), &co), "Touch");
    assert_eq!(kind_both(&at(6.1), &co), "None");
    assert_eq!(kind_both(&at(5.9), &co), "Curves");
}
