//! Skizzen 7, Task 11: Schneiden am schrägen Kegel. Die Ebene schneidet den Kegel in jeder Lage;
//! Ellipse, Parabel und Hyperbel sind exakt, der falsche Ast fällt beim Beschneiden weg.

use planora_kernel::boolean::{
    Containment, Seg, SurfaceIntersection, face_geom, intersect_surfaces, point_in_face,
};
use planora_kernel::primitives::make_cone;
use planora_kernel::{
    Curve, Face, Id, ProfileLoop, SectionError, SectionPiece, Segment, Solid, Surface, revolve,
    section_by_plane,
};
use planora_math::nurbs::{MAX_COORD, MAX_WEIGHT, MIN_WEIGHT};
use planora_math::{DVec2, DVec3, Frame, TAU, Tolerance};

fn tol() -> Tolerance {
    Tolerance::default()
}

fn cone_face(s: &Solid) -> Id<Face> {
    s.faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cone { .. }))
        .map(|(id, _)| id)
        .expect("Kegelfläche")
}

fn seg(p: &SectionPiece) -> Seg {
    Seg::new(p.curve.clone(), p.t0, p.t1)
}

/// Kegelstumpf um z: unten r = 5, oben r = 2, Höhe 6.
fn frustum() -> Solid {
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(5.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(2.0, 6.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, 6.0),
            },
        ],
    };
    revolve(&chain, &Frame::WORLD, &tol()).unwrap()
}

/// Normale, die um `a` gegen die Achse z gekippt ist und in Richtung `phi` zeigt.
fn normal(a: f64, phi: f64) -> DVec3 {
    DVec3::new(a.sin() * phi.cos(), a.sin() * phi.sin(), a.cos())
}

/// Normale einer Ebene, die zu der Mantellinie in Richtung `phi` parallel ist, für den Kegel mit
/// Steigung `slope = −r/h` (Spitze oben).
fn normal_along_generator(slope: f64, phi: f64) -> DVec3 {
    let radial = DVec3::new(phi.cos(), phi.sin(), 0.0);
    let generator = radial * slope + DVec3::Z;
    let side = DVec3::Z.cross(radial);
    generator.cross(side).normalize()
}

/// Prüft alle Stücke: Punkte auf Kegel und Ebene, nicht außerhalb der Fläche, gültige Kurven.
/// Dazu die Gegenprobe: Jeder Punkt, in dem eine Mantellinie im Inneren der Fläche die Ebene
/// trifft, liegt auf einem Stück. So fällt auch auf, wenn ein Stück verloren geht.
fn check_section(solid: &Solid, plane: &Frame, what: &str) -> Vec<SectionPiece> {
    let t = tol();
    let face = cone_face(solid);
    let ps = section_by_plane(solid, plane, Some(&[face]), &t)
        .unwrap_or_else(|e| panic!("{what}: abgelehnt: {e}"));
    let geom = face_geom(solid, face);
    for p in &ps {
        assert!(p.t0.is_finite() && p.t1.is_finite(), "{what}: {p:?}");
        if let Curve::Nurbs(c) = &p.curve {
            assert_eq!(c.degree(), 2, "{what}");
            assert!(
                c.weights()
                    .iter()
                    .all(|w| (MIN_WEIGHT..=MAX_WEIGHT).contains(w)),
                "{what}: {:?}",
                c.weights()
            );
            assert!(
                c.control_points()
                    .iter()
                    .all(|q| q.abs().max_element() < MAX_COORD)
            );
        }
        let s = seg(p);
        for i in 0..=16 {
            let q = s.at(i as f64 / 16.0);
            let on_cone = geom.surface.implicit(q).abs();
            let on_plane = plane.z.dot(q - plane.origin).abs();
            assert!(on_cone < 1e-7, "{what}: {on_cone} neben dem Kegel, {p:?}");
            assert!(on_plane < 1e-7, "{what}: {on_plane} neben der Ebene, {p:?}");
            assert_ne!(
                point_in_face(&geom, q, &t),
                Containment::Outside,
                "{what}: außerhalb der Fläche bei {q}: {p:?}"
            );
        }
    }
    let Surface::Cone {
        frame,
        radius,
        slope,
    } = geom.surface
    else {
        unreachable!();
    };
    let segs: Vec<Seg> = ps.iter().map(seg).collect();
    for i in 0..24 {
        let u = i as f64 * TAU / 24.0 + 0.01;
        let radial = frame.x * u.cos() + frame.y * u.sin();
        let denom = plane.z.dot(radial * slope + frame.z);
        if denom.abs() < 1e-9 {
            continue;
        }
        let v = plane.z.dot(plane.origin - frame.origin - radial * radius) / denom;
        let q = frame.origin + radial * (radius + slope * v) + frame.z * v;
        if point_in_face(&geom, q, &t) != Containment::Inside {
            continue;
        }
        // `Seg::closest` sucht nur um die beste Stützstelle: An der engen Spitze einer schmalen
        // Parabel kann es den falschen Schenkel erwischen. Dann fein geteilt nachsuchen.
        let nearest = |s: &Seg| {
            let coarse = s.closest(q).1;
            if coarse < 1e-5 {
                return coarse;
            }
            (0..256)
                .map(|k| s.sub(k as f64 / 256.0, (k + 1) as f64 / 256.0).closest(q).1)
                .fold(coarse, f64::min)
        };
        let best = segs.iter().map(nearest).fold(f64::INFINITY, f64::min);
        assert!(
            best < 1e-5,
            "{what}: Schnittpunkt {q} fehlt (Abstand {best}), Stücke {ps:?}"
        );
    }
    ps
}

#[test]
fn every_section_point_is_on_the_cone_and_in_the_plane() {
    let t = tol();
    let solids = [
        ("spitz", make_cone(5.0, 10.0, &t).unwrap()),
        ("flach", make_cone(8.0, 2.0, &t).unwrap()),
        ("Stumpf", frustum()),
    ];
    for (name, solid) in &solids {
        for deg in (0..180).step_by(2) {
            let a = (deg as f64).to_radians();
            for phi in [0.6f64, 2.5] {
                let n = normal(a, phi);
                let sideways = DVec3::new(phi.cos(), phi.sin(), 0.0);
                for (z, lateral) in [(1.0, 0.0), (1.7, 3.1), (5.3, 0.0), (5.7, -1.3), (9.3, 0.4)] {
                    let origin = sideways * lateral + DVec3::Z * z;
                    let plane = Frame::from_origin_normal(origin, n);
                    let what = format!("{name}, {deg}°, phi {phi}, z {z}, seitlich {lateral}");
                    check_section(solid, &plane, &what);
                }
            }
        }
    }
}

/// Lagen, die numerisch weh tun: dicht an der Spitze, dicht an der Parabel, fast senkrecht.
#[test]
fn awkward_positions_stay_exact() {
    let t = tol();
    let solid = make_cone(5.0, 10.0, &t).unwrap();
    let apex = DVec3::Z * 10.0;
    for phi in [0.6f64, 2.5] {
        let side = DVec3::new(phi.cos(), phi.sin(), 0.0);
        // Dicht an der Spitze, parallel zur Achse: sehr schmale Hyperbeln.
        for h in [2e-6, 1e-5, 1e-4, 1e-3, 1e-2, 0.1] {
            let plane = Frame::from_origin_normal(apex - DVec3::Z + side * h, side);
            check_section(
                &solid,
                &plane,
                &format!("schmale Hyperbel, h {h}, phi {phi}"),
            );
        }
        // Dicht neben der Parabellage, beiderseits und innerhalb des Fensters.
        for tilt in [
            1e-3, 1e-5, 1e-7, 1e-8, 1e-10, 0.0, -1e-10, -1e-8, -1e-7, -1e-5,
        ] {
            let n = normal_along_generator(-0.5, phi);
            let n = (n + DVec3::Z * tilt).normalize();
            let plane = Frame::from_origin_normal(DVec3::Z * 4.0, n);
            check_section(&solid, &plane, &format!("Parabelnähe {tilt}, phi {phi}"));
        }
        // Fast senkrecht zur Achse, knapp neben dem Kreis. Unter etwa 4,5e-5 antwortet noch der
        // alte Kreisweg von `plane_cone` (dessen Näherung bleibt, siehe G1).
        for a in [1e-3, 1e-4, 5e-5] {
            let plane = Frame::from_origin_normal(DVec3::Z * 4.0, normal(a, phi));
            check_section(&solid, &plane, &format!("fast Kreis, {a}, phi {phi}"));
        }
    }
}

/// Die Rechnung hängt nicht an der Weltlage und nicht an der Größe des Kegels.
#[test]
fn moved_turned_and_scaled_cones() {
    let t = tol();
    let frame = Frame::from_origin_normal(
        DVec3::new(30.0, -20.0, 10.0),
        DVec3::new(1.0, 2.0, 2.0).normalize(),
    );
    for scale in [0.02, 1.0, 400.0] {
        let chain = ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(5.0 * scale, 0.0),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 10.0 * scale),
                },
            ],
        };
        let solid = revolve(&chain, &frame, &t).unwrap();
        for deg in (2..180).step_by(4) {
            let a = (deg as f64).to_radians();
            let n = frame.x * a.sin() * 0.6f64.cos()
                + frame.y * a.sin() * 0.6f64.sin()
                + frame.z * a.cos();
            for (lateral, z) in [(0.0, 4.0), (1.3, 2.0)] {
                let origin = frame.origin + frame.x * (lateral * scale) + frame.z * (z * scale);
                let plane = Frame::from_origin_normal(origin, n);
                // Bezogen auf die Größe wäre dieselbe Prüfung strenger als nötig: Die Grenzen
                // gelten absolut, wie überall im Kernel.
                check_section(
                    &solid,
                    &plane,
                    &format!("Maßstab {scale}, {deg}°, {lateral}"),
                );
            }
        }
    }
}

/// Kegel um eine zu z parallele Achse durch `origin`, Spitze oben.
fn cone_at(origin: DVec3, r: f64, h: f64) -> Solid {
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(r, 0.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, h),
            },
        ],
    };
    let frame = Frame {
        origin,
        ..Frame::WORLD
    };
    revolve(&chain, &frame, &tol()).unwrap()
}

/// Korrektur 1: Parabel, Hyperbel und Ellipsenbogen laufen als ein Stück über die Naht. Sonst
/// folgte die Skizzenkurve beim Verweis auf das Stück 0 nur noch einem Teil des Schnitts, ohne Warnung.
#[test]
fn seam_does_not_split_a_conic() {
    for tilt in [0.0f64, 0.2, 0.4, 0.6, 0.8, 1.0, 1.2] {
        for (nx, ny) in [(0.894f64, 0.447f64), (1.0, 0.0), (0.7, 0.7)] {
            let side = DVec3::new(nx, ny, 0.0).normalize();
            let n = (side * tilt.cos() + DVec3::Z * tilt.sin()).normalize();
            let plane = Frame::from_origin_normal(DVec3::Z * 2.0, n);
            for i in 0..=60 {
                let r = 2.0 + i as f64 * 0.1;
                let solid = cone_at(DVec3::new(-3.0, 0.0, 0.0), r, 10.0);
                let what = format!("Kippung {tilt}, Normale ({nx}, {ny}), r {r}");
                let ps = check_section(&solid, &plane, &what);
                assert!(ps.len() <= 1, "{what}: {ps:?}");
            }
        }
    }
}

/// Korrektur 1: Große Kegel im Fenster der Parabel. Die weggelassene Größe `A·u²` darf das Stück
/// weder über die Fläche hinauslaufen lassen noch die Kurve vom Kegel wegschieben.
#[test]
fn large_cones_near_the_parabola_stay_on_the_face() {
    let t = tol();
    let positions = [
        (100.0, 200.0, 1e-4, 1e-10),
        (100.0, 200.0, 5.0, 1e-7),
        (250.0, 500.0, 5.0, 1e-9),
        (1000.0, 2000.0, 1e-2, 1e-10),
        (1000.0, 2000.0, 1e-5, 1e-12),
        (1000.0, 2000.0, 20.0, 1e-10),
        (1000.0, 2000.0, 20.0, 3e-9),
        (1000.0, 2000.0, 20.0, 1e-6),
        (1000.0, 2000.0, 1e-3, 1e-7),
        (1000.0, 2000.0, 1e-3, 1e-5),
        (1000.0, 2000.0, 1e-5, 1e-9),
    ];
    for (r, height, h, tilt) in positions {
        for sign in [1.0, -1.0, 0.0] {
            for phi in [1.1f64, 3.3] {
                let solid = make_cone(r, height, &t).unwrap();
                let slope = -r / height;
                let along = normal_along_generator(slope, phi);
                let radial = DVec3::new(phi.cos(), phi.sin(), 0.0);
                let apex = DVec3::Z * height;
                // Die Ebene liegt um `h` neben der Mantellinie, auf der Seite der Achse.
                let origin = apex - along * (h * along.dot(radial).signum());
                let n = (along + DVec3::Z * (tilt * sign)).normalize();
                let plane = Frame::from_origin_normal(origin, n);
                let what = format!(
                    "r {r}, H {height}, h {h}, Kippung {}, phi {phi}",
                    tilt * sign
                );
                let ps = check_section(&solid, &plane, &what);
                assert!(!ps.is_empty(), "{what}");
                let geom = face_geom(&solid, cone_face(&solid));
                for p in &ps {
                    let s = seg(p);
                    for i in 0..=256 {
                        let q = s.at(i as f64 / 256.0);
                        assert!(q.z >= -1e-5, "{what}: unter der Fläche {q}");
                        let off = geom.surface.implicit(q).abs();
                        assert!(off < 1e-7, "{what}: {off} neben dem Kegel");
                    }
                }
            }
        }
    }
}

/// Ebene mit der Normalen `normal(a, phi)`, deren ferner Scheitel um `dip` unter dem Grundkreis
/// liegt (auf der Mantellinie in Richtung `phi`).
fn plane_with_far_vertex_below_rim(r: f64, height: f64, a: f64, phi: f64, dip: f64) -> Frame {
    let radial = DVec3::new(phi.cos(), phi.sin(), 0.0);
    let far = radial * (r + r / height * dip) - DVec3::Z * dip;
    Frame::from_origin_normal(far, normal(a, phi))
}

/// Korrektur 2: Taucht der ferne Scheitel einer schlanken Ellipse knapp unter den Rand, fallen die
/// beiden Treffer des Randes zusammen. Der Schnitt darf dabei nicht verloren gehen.
#[test]
fn far_vertex_just_below_the_rim_keeps_the_section() {
    let t = tol();
    let mut checked = 0;
    for (r, height) in [(5.0, 10.0), (100.0, 200.0), (1000.0, 2000.0)] {
        let solid = make_cone(r, height, &t).unwrap();
        for a in [1.0, 1.05, 1.09, 1.1, 1.105, 1.107] {
            for dip in [2e-6, 1e-5, 3e-5, 1e-4, 3e-4, 1e-3, 1e-2] {
                for phi in [0.6f64, 2.5] {
                    let plane = plane_with_far_vertex_below_rim(r, height, a, phi, dip);
                    let what = format!("r {r}, a {a}, Tiefe {dip}, phi {phi}");
                    let ps = check_section(&solid, &plane, &what);
                    assert!(!ps.is_empty(), "{what}: Schnitt leer");
                    // Bis auf die kleine Lücke am fernen Scheitel ist der Schnitt da.
                    let open = ps
                        .iter()
                        .map(|p| seg(p).start().distance(seg(p).end()))
                        .fold(0.0, f64::max);
                    assert!(open < r, "{what}: Lücke {open}");
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 3 * 6 * 7 * 2);
}

/// Korrektur 2: Dasselbe von innen: Der Scheitel bleibt knapp über dem Rand, der Schnitt ist die
/// ganze Ellipse.
#[test]
fn far_vertex_just_above_the_rim_is_a_whole_ellipse() {
    let t = tol();
    let solid = make_cone(5.0, 10.0, &t).unwrap();
    for a in [1.0, 1.09, 1.1] {
        for dip in [-1e-5, -1e-3, -1e-1] {
            let plane = plane_with_far_vertex_below_rim(5.0, 10.0, a, 0.6, dip);
            let what = format!("a {a}, Höhe {}", -dip);
            let ps = check_section(&solid, &plane, &what);
            assert_eq!(ps.len(), 1, "{what}: {ps:?}");
            let s = seg(&ps[0]);
            assert!(s.start().distance(s.end()) <= t.linear, "{what}: {ps:?}");
        }
    }
}

/// Reicht ein Stück nicht mehr für die Gewichtsgrenze, wird der Ast am Scheitel geteilt.
#[test]
fn very_thin_hyperbola_is_split_at_the_vertex() {
    let t = tol();
    let solid = make_cone(5.0, 10.0, &t).unwrap();
    let plane = Frame::from_origin_normal(DVec3::new(0.0, 2e-6, 9.0), DVec3::Y);
    let ps = check_section(&solid, &plane, "h = 2e-6");
    assert!(ps.len() >= 2, "{ps:?}");
    for p in &ps {
        if let Curve::Nurbs(c) = &p.curve {
            assert!(c.weights().iter().all(|w| *w <= MAX_WEIGHT));
        }
    }
}

#[test]
fn ellipse_when_steeper_than_the_flank() {
    let t = tol();
    let solid = make_cone(5.0, 10.0, &t).unwrap();
    // Kegel mit tan(Halbwinkel) = 0,5: Die Ebene steht zur Achse steiler als die Flanke, solange
    // ihre Normale weniger als 90° − 26,57° gegen die Achse gekippt ist.
    for deg in (2..=62).step_by(2) {
        let a = (deg as f64).to_radians();
        let plane = Frame::from_origin_normal(DVec3::Z * 7.0, normal(a, 0.6));
        let ps = check_section(&solid, &plane, &format!("{deg}°"));
        assert!(!ps.is_empty(), "{deg}°");
        for p in &ps {
            let Curve::Ellipse { major, minor, .. } = p.curve else {
                panic!("{deg}°: keine Ellipse: {p:?}");
            };
            assert!(major >= minor, "{deg}°: {major} < {minor}");
        }
    }
    // Spitze bei z = 10, Ebene z = 7 um 30° gekippt, in Richtung der Kippung gemessen:
    // Halbachse aus Abstand und Winkeln (senkrecht zur Kippung kleiner).
    let a = 30f64.to_radians();
    let plane = Frame::from_origin_normal(DVec3::Z * 7.0, normal(a, 0.0));
    let ps = section_by_plane(&solid, &plane, Some(&[cone_face(&solid)]), &t).unwrap();
    assert_eq!(ps.len(), 1, "{ps:?}");
    let Curve::Ellipse { major, minor, .. } = ps[0].curve else {
        panic!("{ps:?}");
    };
    // Die beiden Scheitel: dort trifft die Ebene die Mantellinien in der Kippebene.
    let hit = |s: f64| {
        // x = s · 0,5 · (10 − z), z = 7 − x · tan 30°
        let x = s * 0.5 * 3.0 / (1.0 - s * 0.5 * a.tan());
        DVec3::new(x, 0.0, 7.0 - x * a.tan())
    };
    let d = hit(1.0).distance(hit(-1.0));
    assert!((2.0 * major - d).abs() < 1e-9, "{major} gegen {}", d / 2.0);
    assert!(minor < major);
    assert!(((ps[0].t1 - ps[0].t0).abs() - TAU).abs() < 1e-9);
}

#[test]
fn parabola_when_parallel_to_a_generator() {
    let t = tol();
    let solid = make_cone(5.0, 10.0, &t).unwrap();
    for i in 0..18 {
        let phi = i as f64 * TAU / 18.0 + 0.05;
        // Auch knapp daneben, aber innerhalb des Fensters 1e-9: dieselbe Art.
        for wobble in [0.0, 1e-10, -1e-10] {
            let n = (normal_along_generator(-0.5, phi) + DVec3::Z * wobble).normalize();
            let plane = Frame::from_origin_normal(DVec3::Z * 4.0, n);
            let ps = check_section(&solid, &plane, &format!("phi {phi}, {wobble}"));
            assert!(!ps.is_empty(), "phi {phi}");
            for p in &ps {
                let Curve::Nurbs(c) = &p.curve else {
                    panic!("phi {phi}: keine Parabel: {p:?}");
                };
                assert_eq!(c.degree(), 2);
                assert_eq!(c.control_points().len(), 3, "ein Bézier-Stück");
                assert!(c.weights().iter().all(|w| *w == 1.0), "nicht rational");
            }
        }
    }
}

#[test]
fn hyperbola_only_the_right_branch() {
    let t = tol();
    // Die Kegelgleichung hat zwei Schalen; die Fläche nur die untere. Von beiden Ästen bleibt der
    // untere: bei y = 2 hat er den Scheitel bei z = 6, und alles liegt darunter.
    let solid = make_cone(5.0, 10.0, &t).unwrap();
    let plane = Frame::from_origin_normal(DVec3::Y * 2.0, DVec3::Y);
    let ps = check_section(&solid, &plane, "y = 2");
    assert_eq!(ps.len(), 1, "{ps:?}");
    let Curve::Nurbs(c) = &ps[0].curve else {
        panic!("keine Hyperbel: {ps:?}");
    };
    assert_eq!(c.degree(), 2);
    assert!(c.weights().iter().any(|w| *w != 1.0), "rational");
    let s = seg(&ps[0]);
    for i in 0..=64 {
        let q = s.at(i as f64 / 64.0);
        assert!(q.z <= 6.0 + 1e-9, "{q}");
        assert!(q.z >= -1e-6, "{q}");
    }
    let top = (0..=512)
        .map(|i| s.at(i as f64 / 512.0).z)
        .fold(f64::MIN, f64::max);
    assert!((top - 6.0).abs() < 1e-4, "Scheitel {top}");
    // Die Enden liegen auf dem Grundkreis.
    for q in [s.start(), s.end()] {
        assert!(q.z.abs() < 1e-6, "{q}");
        assert!((q.truncate().length() - 5.0).abs() < 1e-6, "{q}");
    }
    // Umgedrehte Normale und die andere Seite der Achse liefern denselben Ast (bei y = −2 mit der
    // Normalen +y ist der lineare Beiwert −0).
    for (y, n) in [(2.0, -DVec3::Y), (-2.0, DVec3::Y), (-2.0, -DVec3::Y)] {
        let plane = Frame::from_origin_normal(DVec3::Y * y, n);
        let ps = check_section(&solid, &plane, &format!("y = {y}, Normale {n}"));
        assert_eq!(ps.len(), 1, "{ps:?}");
    }
    // Viele Lagen, immer dieselbe Art und nie über der Spitze.
    for deg in (64..=116).step_by(2) {
        let a = (deg as f64).to_radians();
        for phi in [1.1f64, 4.0] {
            let origin = DVec3::new(phi.cos(), phi.sin(), 0.0) * 1.5 + DVec3::Z * 3.0;
            let plane = Frame::from_origin_normal(origin, normal(a, phi));
            let ps = check_section(&solid, &plane, &format!("{deg}°, {phi}"));
            for p in &ps {
                let s = seg(p);
                for i in 0..=32 {
                    let z = s.at(i as f64 / 32.0).z;
                    assert!(z <= 10.0 + 1e-6, "{deg}°: über der Spitze {z}");
                }
            }
        }
    }
}

#[test]
fn through_apex_gives_two_lines() {
    let t = tol();
    let solid = make_cone(5.0, 10.0, &t).unwrap();
    let apex = DVec3::Z * 10.0;
    // Die Normale liegt fast waagerecht: Die Ebene geht flacher zur Achse als die Flanke. Ohne
    // Lage durch die Achse selbst (das konnte schon der alte Weg).
    for deg in [70.0f64, 75.0, 80.0, 85.0, 95.0, 100.0, 110.0] {
        let a = deg.to_radians();
        let plane = Frame::from_origin_normal(apex, normal(a, 0.6));
        let ps = check_section(&solid, &plane, &format!("{deg}°"));
        assert_eq!(ps.len(), 2, "{deg}°: {ps:?}");
        for p in &ps {
            assert!(matches!(p.curve, Curve::Line { .. }), "{deg}°: {p:?}");
            let s = seg(p);
            assert!(
                (s.length() - 125f64.sqrt()).abs() < 1e-6,
                "{deg}°: {}",
                s.length()
            );
            let top = if s.start().z > s.end().z {
                s.start()
            } else {
                s.end()
            };
            assert!(top.distance(apex) < 1e-6, "{deg}°: {top}");
        }
    }
    // Steiler als die Flanke durch die Spitze: nur die Spitze, also nichts.
    for deg in [0.5f64, 10.0, 40.0, 60.0] {
        let plane = Frame::from_origin_normal(apex, normal(deg.to_radians(), 0.6));
        let ps = section_by_plane(&solid, &plane, Some(&[cone_face(&solid)]), &t).unwrap();
        assert!(ps.is_empty(), "{deg}°: {ps:?}");
    }
}

#[test]
fn touching_generator_gives_nothing() {
    let t = tol();
    let solid = make_cone(5.0, 10.0, &t).unwrap();
    let face = cone_face(&solid);
    for i in 0..18 {
        let phi = i as f64 * TAU / 18.0 + 0.05;
        let plane = Frame::from_origin_normal(DVec3::Z * 10.0, normal_along_generator(-0.5, phi));
        let ps = section_by_plane(&solid, &plane, Some(&[face]), &t).unwrap();
        assert!(ps.is_empty(), "phi {phi}: {ps:?}");
    }
}

/// Enden aller Stücke kommen genau zweimal vor (geschlossene Stücke zählen sich selbst).
fn assert_closed(ps: &[SectionPiece], what: &str) {
    let ends: Vec<DVec3> = ps
        .iter()
        .flat_map(|p| {
            let s = seg(p);
            [s.start(), s.end()]
        })
        .collect();
    for e in &ends {
        let k = ends.iter().filter(|x| x.distance(*e) <= 1e-5).count();
        assert_eq!(k % 2, 0, "{what}: offenes Ende bei {e}: {ps:?}");
    }
}

#[test]
fn whole_cone_section_is_closed_loop_where_expected() {
    let t = tol();
    let solid = make_cone(5.0, 10.0, &t).unwrap();
    // Ellipse ganz im Mantel: ein geschlossenes Stück ohne Grundkreis.
    let plane = Frame::from_origin_normal(DVec3::Z * 6.0, normal(0.3, 0.6));
    let ps = section_by_plane(&solid, &plane, None, &t).unwrap();
    assert_eq!(ps.len(), 1, "{ps:?}");
    assert!(matches!(ps[0].curve, Curve::Ellipse { .. }));
    let s = seg(&ps[0]);
    assert!(s.start().distance(s.end()) <= t.linear, "{ps:?}");
    // Parabel und Hyperbel enden am Grundkreis, die Sehne auf dem Boden schließt.
    let cases = [
        (
            "Parabel",
            Frame::from_origin_normal(DVec3::Z * 4.0, normal_along_generator(-0.5, 1.1)),
        ),
        (
            "Hyperbel",
            Frame::from_origin_normal(DVec3::Y * 2.0, DVec3::Y),
        ),
        (
            "schräge Hyperbel",
            Frame::from_origin_normal(DVec3::new(1.0, 1.0, 3.0), normal(1.3, 0.7)),
        ),
        (
            "Ellipse, unten abgeschnitten",
            Frame::from_origin_normal(DVec3::Z * 1.0, normal(0.8, 0.6)),
        ),
    ];
    for (what, plane) in cases {
        let ps = section_by_plane(&solid, &plane, None, &t).unwrap();
        assert!(ps.len() >= 2, "{what}: {ps:?}");
        assert_closed(&ps, what);
        assert!(
            ps.iter().any(|p| matches!(p.curve, Curve::Line { .. })),
            "{what}: die Sehne fehlt: {ps:?}"
        );
    }
    // Beim Stumpf läge der Scheitel bei y = 1 über dem Deckel: zwei Hyperbelstücke, oben und unten
    // je eine Sehne.
    let f = frustum();
    let ps =
        section_by_plane(&f, &Frame::from_origin_normal(DVec3::Y, DVec3::Y), None, &t).unwrap();
    assert_closed(&ps, "Stumpf");
    assert_eq!(ps.len(), 4, "{ps:?}");
}

#[test]
fn boolean_plane_cone_still_unsupported() {
    // Anti-Ziel: `plane_cone` und damit die Booleschen Operationen bleiben unverändert.
    let t = tol();
    let solid = make_cone(5.0, 10.0, &t).unwrap();
    let geom = face_geom(&solid, cone_face(&solid));
    for (o, n) in [
        (DVec3::Z * 5.0, normal(0.5, 0.0)),
        (DVec3::Y * 2.0, DVec3::Y),
        (DVec3::Z * 4.0, normal_along_generator(-0.5, 1.0)),
    ] {
        let cut = Surface::Plane {
            frame: Frame::from_origin_normal(o, n),
        };
        assert!(
            matches!(
                intersect_surfaces(&cut, &geom.surface, &t),
                SurfaceIntersection::Unsupported
            ),
            "{n}"
        );
    }
}

#[test]
fn torus_still_refused() {
    let t = tol();
    let chain = ProfileLoop {
        start: DVec2::new(0.0, -2.0),
        segments: vec![
            Segment::Line {
                to: DVec2::new(5.0, -2.0),
            },
            Segment::Arc {
                center: DVec2::new(5.0, 0.0),
                sweep: 300f64.to_radians(),
            },
            Segment::Line {
                to: DVec2::new(0.0, -1.0),
            },
        ],
    };
    let solid = revolve(&chain, &Frame::WORLD, &t).unwrap();
    let plane = Frame::from_origin_normal(DVec3::new(5.0, 0.0, 0.0), normal(0.5, 0.2));
    let err = section_by_plane(&solid, &plane, None, &t).unwrap_err();
    assert!(
        matches!(
            err,
            SectionError::Unsupported {
                surface: "Torus",
                ..
            }
        ),
        "{err:?}"
    );
    assert!(!err.to_string().is_empty());
}
