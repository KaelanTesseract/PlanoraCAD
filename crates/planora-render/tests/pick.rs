use glam::{Vec2, Vec3};
use planora_kernel::*;
use planora_math::DVec3;
use planora_mesh::*;
use planora_render::*;

fn box_mesh() -> (Solid, Mesh) {
    let solid = AnalyticKernel::default()
        .make_box(DVec3::new(2.0, 2.0, 2.0))
        .unwrap();
    let mesh = tessellate(&solid, &TessellationOptions::default()).unwrap();
    (solid, mesh)
}

#[test]
fn ray_from_above_hits_the_top_face() {
    let (solid, mesh) = box_mesh();
    let hit = pick_face(&mesh, Vec3::new(1.0, 1.0, 10.0), Vec3::new(0.0, 0.0, -1.0)).unwrap();
    assert!((hit.distance - 8.0).abs() < 1e-4);
    let Surface::Plane { frame } = &solid.faces[hit.face].surface else {
        panic!("Quader hat nur ebene Flächen")
    };
    assert!((frame.origin.z - 2.0).abs() < 1e-9);
}

#[test]
fn ray_that_misses_returns_none() {
    let (_, mesh) = box_mesh();
    assert!(
        pick_face(
            &mesh,
            Vec3::new(50.0, 50.0, 10.0),
            Vec3::new(0.0, 0.0, -1.0)
        )
        .is_none()
    );
}

#[test]
fn empty_mesh_never_picks() {
    let mesh = Mesh::default();
    assert!(pick_face(&mesh, Vec3::ZERO, Vec3::Z).is_none());
    let cam = OrbitCamera::default();
    assert!(
        pick_edge(
            &mesh,
            &cam,
            Vec2::new(800.0, 600.0),
            Vec2::new(400.0, 300.0),
            6.0,
            None
        )
        .is_none()
    );
}

#[test]
fn cursor_near_an_edge_picks_that_edge() {
    let (_, mesh) = box_mesh();
    let mut cam = OrbitCamera::default();
    cam.frame_bounds(Vec3::ZERO, Vec3::splat(2.0));
    let viewport = Vec2::new(800.0, 600.0);
    cam.set_viewport(viewport.x, viewport.y);
    let line = &mesh.edge_lines[0];
    let a = line.points[0];
    let b = *line.points.last().unwrap();
    let mid = Vec3::new(
        ((a.x + b.x) / 2.0) as f32,
        ((a.y + b.y) / 2.0) as f32,
        ((a.z + b.z) / 2.0) as f32,
    );
    let cursor = cam.project(mid, viewport).unwrap() + Vec2::new(2.0, 1.0);
    let hit = pick_edge(&mesh, &cam, viewport, cursor, 6.0, None).unwrap();
    assert!(hit.pixel_distance <= 3.0);
}

#[test]
fn cursor_far_from_all_edges_picks_nothing() {
    let (_, mesh) = box_mesh();
    let mut cam = OrbitCamera::default();
    cam.frame_bounds(Vec3::ZERO, Vec3::splat(2.0));
    let viewport = Vec2::new(800.0, 600.0);
    cam.set_viewport(viewport.x, viewport.y);
    assert!(pick_edge(&mesh, &cam, viewport, Vec2::new(2.0, 2.0), 6.0, None).is_none());
}

#[test]
fn occluded_edges_are_ignored() {
    let (_, mesh) = box_mesh();
    let mut cam = OrbitCamera::default();
    cam.frame_bounds(Vec3::ZERO, Vec3::splat(2.0));
    let viewport = Vec2::new(800.0, 600.0);
    cam.set_viewport(viewport.x, viewport.y);
    let line = &mesh.edge_lines[0];
    let a = line.points[0];
    let mid = Vec3::new(a.x as f32, a.y as f32, a.z as f32);
    let cursor = cam.project(mid, viewport).unwrap();
    // Eine Fläche ganz nah an der Kamera verdeckt alles dahinter.
    assert!(pick_edge(&mesh, &cam, viewport, cursor, 6.0, Some(0.1)).is_none());
}
