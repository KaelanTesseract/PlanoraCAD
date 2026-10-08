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

fn edge_midpoint(mesh: &Mesh, want: Vec3) -> (Vec3, Id<Edge>) {
    mesh.edge_lines
        .iter()
        .map(|l| {
            let a = l.points[0];
            let b = *l.points.last().unwrap();
            let m = Vec3::new(
                ((a.x + b.x) / 2.0) as f32,
                ((a.y + b.y) / 2.0) as f32,
                ((a.z + b.z) / 2.0) as f32,
            );
            (m, l.edge)
        })
        .min_by(|x, y| (x.0 - want).length().total_cmp(&(y.0 - want).length()))
        .unwrap()
}

/// Rückt die Kamera weit weg und verengt den Blickwinkel so, dass das Bild gleich groß bleibt
/// (Teleobjektiv). So bleiben die Kanten im Bild auseinander, auch bei großem Abstand.
fn far_away(cam: &mut OrbitCamera, distance: f32) {
    let half_height = cam.distance * (cam.fov_y * 0.5).tan();
    cam.distance = distance;
    cam.fov_y = 2.0 * (half_height / distance).atan();
}

fn occluder_at(mesh: &Mesh, cam: &OrbitCamera, viewport: Vec2, cursor: Vec2) -> Option<f32> {
    let ndc = Vec2::new(
        2.0 * cursor.x / viewport.x - 1.0,
        1.0 - 2.0 * cursor.y / viewport.y,
    );
    let (origin, dir) = cam.ray(ndc);
    pick_face(mesh, origin, dir).map(|h| h.distance)
}

#[test]
fn visible_edge_stays_pickable_at_large_camera_distances() {
    let (_, mesh) = box_mesh();
    let viewport = Vec2::new(800.0, 600.0);
    // Standardansicht: Kamera bei +x, -y, +z; die Kante (2, 0, 1) liegt vorn.
    let (mid, edge) = edge_midpoint(&mesh, Vec3::new(2.0, 0.0, 1.0));
    for distance in [50.0, 5000.0, 20000.0] {
        let mut cam = OrbitCamera::default();
        cam.frame_bounds(Vec3::ZERO, Vec3::splat(2.0));
        far_away(&mut cam, distance);
        cam.set_viewport(viewport.x, viewport.y);
        let cursor = cam.project(mid, viewport).unwrap();
        let occluder = occluder_at(&mesh, &cam, viewport, cursor).expect("Fläche getroffen");
        let hit = pick_edge(&mesh, &cam, viewport, cursor, 6.0, Some(occluder));
        assert_eq!(hit.map(|h| h.edge), Some(edge), "Abstand {distance}");
    }
}

/// Realistische Kamera: Standard-Blickwinkel (45 Grad), 100-mm-Würfel, 1280x800.
fn realistic_setup(distance_factor: f32) -> (Mesh, OrbitCamera, Vec2) {
    let solid = AnalyticKernel::default()
        .make_box(DVec3::new(100.0, 100.0, 100.0))
        .unwrap();
    let mesh = tessellate(&solid, &TessellationOptions::default()).unwrap();
    let viewport = Vec2::new(1280.0, 800.0);
    let mut cam = OrbitCamera::default();
    cam.frame_bounds(Vec3::ZERO, Vec3::splat(100.0));
    cam.distance *= distance_factor;
    cam.set_viewport(viewport.x, viewport.y);
    (mesh, cam, viewport)
}

fn dvec_to_vec3(p: DVec3) -> Vec3 {
    Vec3::new(p.x as f32, p.y as f32, p.z as f32)
}

/// Strahl durch den Cursor und Abstand der vordersten getroffenen Fläche.
fn cursor_hit(mesh: &Mesh, cam: &OrbitCamera, viewport: Vec2, cursor: Vec2) -> Option<(f32, Vec3)> {
    let ndc = Vec2::new(
        2.0 * cursor.x / viewport.x - 1.0,
        1.0 - 2.0 * cursor.y / viewport.y,
    );
    let (origin, dir) = cam.ray(ndc);
    pick_face(mesh, origin, dir).map(|h| (h.distance, origin + dir * h.distance))
}

/// Alle Probepunkte: (Kante, Weltpunkt, Cursor).
fn edge_samples(
    mesh: &Mesh,
    cam: &OrbitCamera,
    viewport: Vec2,
    offset_px: f32,
) -> Vec<(Id<Edge>, Vec3, Vec2)> {
    let mut out = Vec::new();
    for line in &mesh.edge_lines {
        let a = dvec_to_vec3(line.points[0]);
        let b = dvec_to_vec3(*line.points.last().unwrap());
        for t in [0.1_f32, 0.25, 0.5, 0.75, 0.9] {
            let p = a + (b - a) * t;
            let pa = cam.project(a, viewport).unwrap();
            let pb = cam.project(b, viewport).unwrap();
            let dir2 = (pb - pa).normalize_or_zero();
            let normal = Vec2::new(-dir2.y, dir2.x);
            for sign in [1.0_f32, -1.0] {
                let cursor = cam.project(p, viewport).unwrap() + normal * offset_px * sign;
                out.push((line.edge, p, cursor));
                if offset_px == 0.0 {
                    break;
                }
            }
        }
    }
    out
}

#[test]
fn visible_edges_are_pickable_with_a_realistic_camera() {
    for factor in [1.0_f32, 2.0] {
        let (mesh, cam, viewport) = realistic_setup(factor);
        for offset in [0.0_f32, 3.0] {
            let mut checked = 0;
            for (edge, p, cursor) in edge_samples(&mesh, &cam, viewport, offset) {
                let Some((od, hit)) = cursor_hit(&mesh, &cam, viewport, cursor) else {
                    continue;
                };
                // Sichtbar: der Strahl trifft die Fläche praktisch an diesem Kantenpunkt.
                let tolerance = 0.05 + offset * 2.0;
                if (hit - p).length() > tolerance {
                    continue;
                }
                checked += 1;
                let result = pick_edge(&mesh, &cam, viewport, cursor, 6.0, Some(od));
                let h = result.unwrap_or_else(|| {
                    panic!("Kante {edge:?} bei {p:?} (Faktor {factor}, Versatz {offset}) verworfen")
                });
                assert!(h.pixel_distance <= 6.0);
            }
            assert!(checked >= 20, "zu wenige sichtbare Proben: {checked}");
        }
    }
}

#[test]
fn hidden_edges_are_not_picked_with_a_realistic_camera() {
    for factor in [1.0_f32, 2.0] {
        let (mesh, cam, viewport) = realistic_setup(factor);
        // Kanten, die an keinem Probepunkt sichtbar sind (die drei Kanten der abgewandten Ecke).
        let samples = edge_samples(&mesh, &cam, viewport, 0.0);
        // Verdeckt: an jedem Probepunkt liegt eine Fläche deutlich (> 5 mm) vor der Kante.
        let is_visible = |edge: Id<Edge>| {
            samples
                .iter()
                .filter(|s| s.0 == edge)
                .any(|(_, p, cursor)| {
                    let ndc = Vec2::new(
                        2.0 * cursor.x / viewport.x - 1.0,
                        1.0 - 2.0 * cursor.y / viewport.y,
                    );
                    let (origin, dir) = cam.ray(ndc);
                    let along = (*p - origin).dot(dir);
                    cursor_hit(&mesh, &cam, viewport, *cursor)
                        .is_none_or(|(od, _)| od > along - 5.0)
                })
        };
        let hidden: Vec<_> = mesh
            .edge_lines
            .iter()
            .map(|l| l.edge)
            .filter(|e| !is_visible(*e))
            .collect();
        assert_eq!(hidden.len(), 3, "Faktor {factor}: {hidden:?}");
        let mut rejected = 0;
        for (edge, _, cursor) in samples.iter().filter(|s| hidden.contains(&s.0)) {
            let (od, _) = cursor_hit(&mesh, &cam, viewport, *cursor).expect("Würfel verdeckt");
            match pick_edge(&mesh, &cam, viewport, *cursor, 6.0, Some(od)) {
                None => rejected += 1,
                // Liegt zufällig eine sichtbare Kante im Pixelradius, darf sie getroffen werden.
                Some(h) => assert!(
                    !hidden.contains(&h.edge),
                    "verdeckte Kante {edge:?} getroffen"
                ),
            }
        }
        assert!(rejected > 0, "Faktor {factor}");
    }
}
