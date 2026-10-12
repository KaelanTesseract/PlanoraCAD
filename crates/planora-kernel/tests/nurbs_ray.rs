//! Strahlschnitt von NURBS-Flächen (`NurbsSurfaceData::ray_hits`, Task 10).

use planora_kernel::NurbsSurfaceData;
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DVec3, PI, TAU};

/// Drehfläche: Zylindermantel r, Höhe h um die z-Achse (Naht bei (r, 0)).
fn cylinder(r: f64, h: f64) -> NurbsSurfaceData {
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![DVec3::new(r, 0.0, 0.0), DVec3::new(r, 0.0, h)],
        vec![1.0; 2],
    )
    .unwrap();
    let s = NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    NurbsSurfaceData::new(s, true, false).unwrap()
}

/// Kugel r = 2 um den Ursprung als Drehfläche (Pole auf der z-Achse).
fn sphere() -> NurbsSurfaceData {
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, 2.0),
        DVec3::new(2.0, 0.0, 0.0),
        0.0,
        PI,
    )
    .unwrap();
    let s = NurbsSurface::revolve(&half, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    NurbsSurfaceData::new(s, true, false).unwrap()
}

/// Jeder Treffer liegt auf Strahl und Fläche.
fn assert_on_both(d: &NurbsSurfaceData, o: DVec3, dir: DVec3, hits: &[(f64, f64, f64)]) {
    for &(t, u, v) in hits {
        let p = o + dir * t;
        assert!(
            d.point(u, v).distance(p) < 1e-9,
            "{p:?} vs {:?}",
            d.point(u, v)
        );
    }
}

#[test]
fn a_ray_crosses_the_cylinder_twice_sorted_by_t() {
    let d = cylinder(3.0, 10.0);
    let (o, dir) = (DVec3::new(-10.0, 0.3, 5.0), DVec3::X);
    let hits = d.ray_hits_checked(o, dir, 100.0).unwrap();
    assert_eq!(hits.len(), 2, "{hits:?}");
    let x = (9.0_f64 - 0.09).sqrt();
    assert!((hits[0].0 - (10.0 - x)).abs() < 1e-9);
    assert!((hits[1].0 - (10.0 + x)).abs() < 1e-9);
    assert_on_both(&d, o, dir, &hits);
    assert_eq!(d.ray_hits(o, dir, 100.0), hits);
}

#[test]
fn reach_and_start_limit_the_hits() {
    let d = cylinder(3.0, 10.0);
    // vom Inneren aus nur der Austritt
    let hits = d.ray_hits(DVec3::new(0.0, 0.0, 5.0), DVec3::X, 100.0);
    assert_eq!(hits.len(), 1);
    assert!((hits[0].0 - 3.0).abs() < 1e-9);
    // zu kurz
    assert!(
        d.ray_hits(DVec3::new(-10.0, 0.0, 5.0), DVec3::X, 6.0)
            .is_empty()
    );
    // daneben
    assert!(
        d.ray_hits(DVec3::new(-10.0, 0.0, 11.0), DVec3::X, 100.0)
            .is_empty()
    );
    assert_eq!(
        d.ray_hits_checked(DVec3::new(-10.0, 5.0, 5.0), DVec3::X, 100.0),
        Some(vec![])
    );
}

#[test]
fn a_hit_on_the_seam_is_counted_once() {
    // Die Naht der Drehfläche liegt bei (3, 0, z); der Strahl längs −x trifft sie genau.
    let d = cylinder(3.0, 10.0);
    let (o, dir) = (DVec3::new(10.0, 0.0, 4.0), -DVec3::X);
    let hits = d.ray_hits_checked(o, dir, 100.0).unwrap();
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert!((hits[0].0 - 7.0).abs() < 1e-9 && (hits[1].0 - 13.0).abs() < 1e-9);
    assert_on_both(&d, o, dir, &hits);
}

#[test]
fn grazing_rays_are_reported_as_uncertain() {
    let d = cylinder(3.0, 10.0);
    // tangential an den Mantel (y = 3): kein verlässliches Ergebnis
    assert_eq!(
        d.ray_hits_checked(DVec3::new(-10.0, 3.0, 5.0), DVec3::X, 100.0),
        None
    );
    // in der Fläche (längs einer Mantellinie)
    assert_eq!(
        d.ray_hits_checked(DVec3::new(0.0, 3.0, -5.0), DVec3::Z, 100.0),
        None
    );
}

#[test]
fn hostile_rays_give_no_hits_and_no_panic() {
    let d = cylinder(3.0, 10.0);
    for (o, dir, reach) in [
        (DVec3::new(f64::NAN, 0.0, 0.0), DVec3::X, 10.0),
        (DVec3::ZERO, DVec3::ZERO, 10.0),
        (DVec3::ZERO, DVec3::new(f64::INFINITY, 0.0, 0.0), 10.0),
        (DVec3::ZERO, DVec3::X, f64::NAN),
        (DVec3::ZERO, DVec3::X, -1.0),
    ] {
        assert_eq!(d.ray_hits_checked(o, dir, reach), None);
        assert!(d.ray_hits(o, dir, reach).is_empty());
    }
}

#[test]
fn rays_through_the_poles_of_a_sphere() {
    let d = sphere();
    // durch beide Pole: Treffer bei z = ±2 oder (Normale am Pol) eine unsichere Meldung, nie
    // ein falscher Treffer
    let (o, dir) = (DVec3::new(0.0, 0.0, -5.0), DVec3::Z);
    if let Some(hits) = d.ray_hits_checked(o, dir, 20.0) {
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!((hits[0].0 - 3.0).abs() < 1e-7 && (hits[1].0 - 7.0).abs() < 1e-7);
    }
    // schräg durch die Mitte
    let dir = DVec3::new(0.3, 0.4, 0.866).normalize();
    let o = -dir * 5.0;
    let hits = d.ray_hits_checked(o, dir, 20.0).unwrap();
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert!((hits[0].0 - 3.0).abs() < 1e-9 && (hits[1].0 - 7.0).abs() < 1e-9);
    assert_on_both(&d, o, dir, &hits);
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

/// Zufälliger rationaler bikubischer Flicken (wie in `nurbs_geometry.rs`).
fn random_patch(rng: &mut Lcg) -> NurbsSurface {
    let n = 6;
    let k = vec![0.0, 0.0, 0.0, 0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0, 1.0, 1.0, 1.0];
    let mut c = Vec::new();
    let mut w = Vec::new();
    for i in 0..n {
        for j in 0..n {
            c.push(DVec3::new(
                2.0 * i as f64 + rng.range(-0.7, 0.7),
                2.0 * j as f64 + rng.range(-0.7, 0.7),
                rng.range(-3.0, 3.0),
            ));
            w.push(rng.range(0.3, 3.0));
        }
    }
    NurbsSurface::new(3, 3, k.clone(), k, n, n, c, w).unwrap()
}

/// Strahl gegen ein Dreieck (Möller–Trumbore): `t`.
fn ray_triangle(o: DVec3, dir: DVec3, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-14 {
        return None;
    }
    let s = o - a;
    let u = s.dot(p) / det;
    let q = s.cross(e1);
    let v = dir.dot(q) / det;
    if u < 0.0 || v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(e2.dot(q) / det)
}

#[test]
fn ray_hits_agree_with_a_fine_triangulation_on_random_patches() {
    // Gegenprobe: feines Dreiecksnetz (200 × 200) des Flickens. Jeder Netztreffer, der nicht
    // nah am Flickenrand liegt, hat einen Strahltreffer bei fast gleichem t und umgekehrt – oder
    // der Strahl ist als unsicher gemeldet (dann zählt er nicht, wird aber gezählt).
    let mut rng = Lcg(0x7a11);
    let (mut rays, mut uncertain, mut total_hits) = (0, 0, 0);
    for _ in 0..6 {
        let s = random_patch(&mut rng);
        let n = 200;
        let grid: Vec<DVec3> = (0..=n)
            .flat_map(|i| (0..=n).map(move |j| (i, j)))
            .map(|(i, j)| s.point(i as f64 / n as f64, j as f64 / n as f64))
            .collect();
        let at = |i: usize, j: usize| grid[i * (n + 1) + j];
        let d = NurbsSurfaceData::new(s.clone(), false, false).unwrap();
        for _ in 0..25 {
            let target = DVec3::new(rng.range(1.5, 8.5), rng.range(1.5, 8.5), 0.0);
            let dir = DVec3::new(rng.range(-0.6, 0.6), rng.range(-0.6, 0.6), 1.0).normalize();
            let o = target - dir * 20.0;
            rays += 1;
            let Some(hits) = d.ray_hits_checked(o, dir, 40.0) else {
                uncertain += 1;
                continue;
            };
            let mut mesh: Vec<(f64, bool)> = Vec::new();
            for i in 0..n {
                for j in 0..n {
                    let near_rim = i < 2 || j < 2 || i + 2 >= n || j + 2 >= n;
                    for (a, b, c) in [
                        (at(i, j), at(i + 1, j), at(i + 1, j + 1)),
                        (at(i, j), at(i + 1, j + 1), at(i, j + 1)),
                    ] {
                        if let Some(t) = ray_triangle(o, dir, a, b, c) {
                            mesh.push((t, near_rim));
                        }
                    }
                }
            }
            for &(t, rim) in &mesh {
                if !rim {
                    assert!(
                        hits.iter().any(|h| (h.0 - t).abs() < 1e-2),
                        "Netztreffer t = {t} fehlt: {hits:?}"
                    );
                }
            }
            for h in &hits {
                assert!(
                    mesh.iter().any(|m| (m.0 - h.0).abs() < 1e-2)
                        || h.1 < 0.02
                        || h.1 > 0.98
                        || h.2 < 0.02
                        || h.2 > 0.98,
                    "Strahltreffer {h:?} ohne Netztreffer"
                );
            }
            assert_on_both(&d, o, dir, &hits);
            total_hits += hits.len();
        }
    }
    // nicht alles als unsicher ausweichen
    assert!(uncertain * 5 <= rays, "{uncertain} von {rays} unsicher");
    assert!(total_hits >= rays - uncertain);
}

// ---- Einordnen von Punkten gegen einen Körper mit Freiformfläche ----

use planora_kernel::boolean::{BooleanError, Location, solid_contains, solid_faces};
use planora_kernel::{AnalyticKernel, Kernel, Profile, ProfileLoop, Solid, Surface, UvRect};
use planora_math::{DVec2, Frame, Tolerance};

/// Zylinder r, h (Boden bei z = 0) mit NURBS-Mantel (wie in `nurbs_boolean.rs`).
fn nurbs_cylinder_solid(r: f64, h: f64) -> Solid {
    let k = AnalyticKernel::default();
    let profile = Profile {
        outer: ProfileLoop::circle(DVec2::ZERO, r),
        holes: vec![],
    };
    let mut solid = k
        .extrude(
            &profile,
            &Frame::from_origin_normal(DVec3::ZERO, DVec3::Z),
            h,
        )
        .unwrap();
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![DVec3::new(r, 0.0, 0.0), DVec3::new(r, 0.0, h)],
        vec![1.0; 2],
    )
    .unwrap();
    let nurbs = Surface::nurbs(
        NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
        true,
        false,
    )
    .unwrap();
    let id = solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .unwrap();
    solid.faces[id].surface = nurbs;
    solid.faces[id].patch = Some(UvRect {
        u0: 0.0,
        u1: TAU,
        v0: 0.0,
        v1: 1.0,
    });
    solid
}

#[test]
fn points_are_located_against_a_body_with_a_nurbs_face() {
    let faces = solid_faces(&nurbs_cylinder_solid(3.0, 10.0));
    let tol = Tolerance::default();
    let mut rng = Lcg(0xc1a5);
    let mut checked = 0;
    for _ in 0..60 {
        let p = DVec3::new(
            rng.range(-5.0, 5.0),
            rng.range(-5.0, 5.0),
            rng.range(-2.0, 12.0),
        );
        let r = (p.x * p.x + p.y * p.y).sqrt();
        // nicht zu nah an der Oberfläche (dort ist „auf dem Rand“ richtig)
        if (r - 3.0).abs() < 0.05 || p.z.abs() < 0.05 || (p.z - 10.0).abs() < 0.05 {
            continue;
        }
        let inside = r < 3.0 && p.z > 0.0 && p.z < 10.0;
        let expect = if inside {
            Location::Inside
        } else {
            Location::Outside
        };
        assert_eq!(solid_contains(&faces, p, &tol), Ok(expect), "{p:?}");
        checked += 1;
    }
    assert!(checked > 40);
    // auf dem Mantel
    assert_eq!(
        solid_contains(&faces, DVec3::new(0.0, -3.0, 4.0), &tol),
        Ok(Location::OnBoundary)
    );
}

/// Kugel r um den Ursprung mit NURBS-Fläche (Drehfläche eines Halbkreises).
fn nurbs_sphere_solid(r: f64) -> Solid {
    let k = AnalyticKernel::default();
    let mut solid = k.make_sphere(r).unwrap();
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, -r),
        DVec3::new(r, 0.0, 0.0),
        0.0,
        PI,
    )
    .unwrap();
    let ns = NurbsSurface::revolve(&half, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    let ((u0, u1), (v0, v1)) = ns.domain();
    let surface = Surface::nurbs(ns, true, false).unwrap();
    let (id, f) = solid.faces.iter().next().unwrap();
    let p = DVec3::new(r, 0.0, 0.0) * 0.6 + DVec3::new(0.0, r, 0.0) * 0.8;
    let (ou, ov) = f.surface.project(p);
    let outward = f.surface.normal(ou, ov) * if f.same_sense { 1.0 } else { -1.0 };
    let (nu, nv) = surface.project(p);
    let same_sense = surface.normal(nu, nv).dot(outward) > 0.0;
    let f = &mut solid.faces[id];
    f.surface = surface;
    f.same_sense = same_sense;
    f.patch = Some(UvRect { u0, u1, v0, v1 });
    solid
}

#[test]
fn points_are_located_against_a_nurbs_sphere() {
    let s = nurbs_sphere_solid(2.0);
    planora_kernel::validate(&s).unwrap();
    let faces = solid_faces(&s);
    let tol = Tolerance::default();
    for (p, expect) in [
        (DVec3::ZERO, Location::Inside),
        (DVec3::new(0.3, -0.5, 1.2), Location::Inside),
        (DVec3::new(0.0, 0.0, 1.9), Location::Inside),
        (DVec3::new(2.5, 0.0, 0.0), Location::Outside),
        (DVec3::new(0.0, 0.0, -2.2), Location::Outside),
    ] {
        assert_eq!(solid_contains(&faces, p, &tol), Ok(expect), "{p:?}");
    }
}

#[test]
fn a_body_smaller_than_the_boundary_band_reports_a_grazing_ray() {
    // Ein Treffer im Band 0,01 · Strahllänge um eine Randkante oder einen Pol der Freiformfläche
    // ist unsicher. Bei einer winzigen Freiformkugel (r = 0,002) liegt jeder Treffer darin: Alle
    // Richtungen sind ungeeignet, benannte Meldung statt eines geratenen Ergebnisses.
    let faces = solid_faces(&nurbs_sphere_solid(0.002));
    match solid_contains(&faces, DVec3::ZERO, &Tolerance::default()) {
        Err(BooleanError::Unsupported(msg)) => assert!(msg.contains("Streifschuss"), "{msg}"),
        other => panic!("{other:?}"),
    }
}
