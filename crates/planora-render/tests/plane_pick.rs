//! Skizzen 6: Konstruktionsebenen und Eckpunkte anklicken.

use glam::{Vec2, Vec3};
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame};
use planora_mesh::*;
use planora_render::*;

/// Ebene XY im Ursprung, halbe Kante 25 mm.
fn xy_rect() -> PlaneRect {
    PlaneRect {
        frame: Frame::WORLD,
        half: DVec2::splat(25.0),
    }
}

#[test]
fn ray_hits_plane_inside_rect() {
    let ray = Ray {
        origin: DVec3::new(10.0, -20.0, 50.0),
        dir: DVec3::new(0.0, 0.0, -1.0),
    };
    let d = pick_plane(&ray, &xy_rect()).expect("Strahl trifft das Rechteck");
    assert!((d - 50.0).abs() < 1e-9);
    // Von unten (Rückseite) trifft der Strahl genauso.
    let below = Ray {
        origin: DVec3::new(0.0, 0.0, -5.0),
        dir: DVec3::new(0.0, 0.0, 1.0),
    };
    assert!((pick_plane(&below, &xy_rect()).unwrap() - 5.0).abs() < 1e-9);
}

#[test]
fn ray_misses_outside_rect() {
    let outside = Ray {
        origin: DVec3::new(26.0, 0.0, 50.0),
        dir: DVec3::new(0.0, 0.0, -1.0),
    };
    assert!(pick_plane(&outside, &xy_rect()).is_none());
    // Parallel zur Ebene, hinter dem Strahl oder ohne Richtung: kein Treffer.
    let parallel = Ray {
        origin: DVec3::new(0.0, 0.0, 1.0),
        dir: DVec3::X,
    };
    assert!(pick_plane(&parallel, &xy_rect()).is_none());
    let away = Ray {
        origin: DVec3::new(0.0, 0.0, 10.0),
        dir: DVec3::Z,
    };
    assert!(pick_plane(&away, &xy_rect()).is_none());
    let none = Ray {
        origin: DVec3::new(0.0, 0.0, 10.0),
        dir: DVec3::ZERO,
    };
    assert!(pick_plane(&none, &xy_rect()).is_none());
}

fn box_mesh() -> Mesh {
    let solid = AnalyticKernel::default()
        .make_box(DVec3::new(20.0, 20.0, 20.0))
        .unwrap();
    tessellate(&solid, &TessellationOptions::default()).unwrap()
}

fn camera(viewport: Vec2) -> OrbitCamera {
    let mut cam = OrbitCamera::default();
    cam.frame_bounds(Vec3::ZERO, Vec3::splat(20.0));
    cam.set_viewport(viewport.x, viewport.y);
    cam
}

fn occluder_at(mesh: &Mesh, cam: &OrbitCamera, viewport: Vec2, cursor: Vec2) -> Option<f32> {
    let ndc = Vec2::new(
        2.0 * cursor.x / viewport.x - 1.0,
        1.0 - 2.0 * cursor.y / viewport.y,
    );
    let (origin, dir) = cam.ray(ndc);
    pick_face(mesh, origin, dir).map(|h| h.distance)
}

fn v3(p: DVec3) -> Vec3 {
    Vec3::new(p.x as f32, p.y as f32, p.z as f32)
}

#[test]
fn vertex_pick_within_8px() {
    let mesh = box_mesh();
    let viewport = Vec2::new(1920.0, 1080.0);
    let cam = camera(viewport);
    // Die Ecke (20, 0, 20) zeigt bei der Standardansicht (von vorn rechts oben) zur Kamera.
    let corner = DVec3::new(20.0, 0.0, 20.0);
    let at = cam.project(v3(corner), viewport).unwrap();
    for offset in [Vec2::new(5.0, 4.0), Vec2::new(-7.0, 0.0)] {
        let cursor = at + offset;
        let occ = occluder_at(&mesh, &cam, viewport, cursor);
        let (body, edge, end) = pick_vertex(&[(3, &mesh)], &cam, viewport, cursor, 8.0, occ)
            .expect("Ecke innerhalb 8 px");
        assert_eq!(body, 3);
        let line = &mesh.edge_lines[edge.index()];
        let p = if end {
            *line.points.last().unwrap()
        } else {
            line.points[0]
        };
        assert!((p - corner).length() < 1e-9, "gefunden: {p:?}");
    }
    // 9 px daneben: nichts.
    let far = at + Vec2::new(9.0, 0.0);
    let occ = occluder_at(&mesh, &cam, viewport, far);
    assert!(pick_vertex(&[(0, &mesh)], &cam, viewport, far, 8.0, occ).is_none());
}

#[test]
fn hidden_vertex_is_not_picked() {
    let mesh = box_mesh();
    let viewport = Vec2::new(1920.0, 1080.0);
    let cam = camera(viewport);
    // Die Ecke (0, 20, 0) liegt hinten unten links, hinter dem Würfel.
    let hidden = DVec3::new(0.0, 20.0, 0.0);
    let at = cam.project(v3(hidden), viewport).unwrap();
    let occ = occluder_at(&mesh, &cam, viewport, at);
    assert!(occ.is_some());
    let hit = pick_vertex(&[(0, &mesh)], &cam, viewport, at, 8.0, occ);
    if let Some((_, edge, end)) = hit {
        let line = &mesh.edge_lines[edge.index()];
        let p = if end {
            *line.points.last().unwrap()
        } else {
            line.points[0]
        };
        assert!((p - hidden).length() > 1e-9, "verdeckte Ecke gewählt");
    }
}
