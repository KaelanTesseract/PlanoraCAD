use crate::camera::OrbitCamera;
use glam::{Vec2, Vec3};
use planora_kernel::{Edge, Face, Id};
use planora_math::DVec3;
use planora_mesh::Mesh;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceHit {
    pub face: Id<Face>,
    pub distance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeHit {
    pub edge: Id<Edge>,
    pub pixel_distance: f32,
}

fn to_vec3(p: DVec3) -> Vec3 {
    Vec3::new(p.x as f32, p.y as f32, p.z as f32)
}

/// Möller–Trumbore, beidseitig. Liefert die Strahllänge bis zum Treffer.
fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let t_vec = o - a;
    let u = t_vec.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t_vec.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-6).then_some(t)
}

/// Nächstgelegene Fläche, die der Strahl trifft.
pub fn pick_face(mesh: &Mesh, origin: Vec3, dir: Vec3) -> Option<FaceHit> {
    let mut best: Option<FaceHit> = None;
    for range in &mesh.face_ranges {
        let start = range.first_index as usize;
        let end = start + range.index_count as usize;
        let Some(indices) = mesh.indices.get(start..end) else {
            continue;
        };
        for tri in indices.as_chunks::<3>().0 {
            let (Some(a), Some(b), Some(c)) = (
                mesh.positions.get(tri[0] as usize),
                mesh.positions.get(tri[1] as usize),
                mesh.positions.get(tri[2] as usize),
            ) else {
                continue;
            };
            if let Some(t) = ray_triangle(origin, dir, to_vec3(*a), to_vec3(*b), to_vec3(*c))
                && best.is_none_or(|h| t < h.distance)
            {
                best = Some(FaceHit {
                    face: range.face,
                    distance: t,
                });
            }
        }
    }
    best
}

fn closest_on_segment(p: Vec2, a: Vec2, b: Vec2) -> (Vec2, f32) {
    let ab = b - a;
    let len2 = ab.length_squared();
    let s = if len2 > 1e-12 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * s, s)
}

/// Kante, die der Mausposition (in Pixeln, Ursprung oben links) am nächsten liegt, höchstens `max_px` entfernt.
/// `occluder` ist die Entfernung der vordersten getroffenen Fläche entlang des Mausstrahls, gemessen vom
/// Strahlursprung aus, also genau das, was `pick_face` für `camera.ray(..)` derselben Mausposition liefert.
/// Weiter entfernte Kanten gelten als verdeckt.
pub fn pick_edge(
    mesh: &Mesh,
    camera: &OrbitCamera,
    viewport: Vec2,
    cursor: Vec2,
    max_px: f32,
    occluder: Option<f32>,
) -> Option<EdgeHit> {
    if !(viewport.x >= 1.0 && viewport.y >= 1.0 && viewport.is_finite() && cursor.is_finite()) {
        return None;
    }
    // Gleicher Bezugspunkt wie bei `pick_face`: Ursprung und Richtung des Mausstrahls.
    let ndc = Vec2::new(
        2.0 * cursor.x / viewport.x - 1.0,
        1.0 - 2.0 * cursor.y / viewport.y,
    );
    let (ray_origin, ray_dir) = camera.ray(ndc);
    let mut best: Option<EdgeHit> = None;
    for line in &mesh.edge_lines {
        for seg in line.points.windows(2) {
            let (wa, wb) = (to_vec3(seg[0]), to_vec3(seg[1]));
            let (Some(a), Some(b)) = (camera.project(wa, viewport), camera.project(wb, viewport))
            else {
                continue;
            };
            let (closest, s) = closest_on_segment(cursor, a, b);
            let dist = (cursor - closest).length();
            if dist > max_px || best.is_some_and(|h| dist >= h.pixel_distance) {
                continue;
            }
            if let Some(od) = occluder {
                let world = wa + (wb - wa) * s;
                let along = (world - ray_origin).dot(ray_dir);
                if along > od + 2.0e-5 * od + 0.01 {
                    continue;
                }
            }
            best = Some(EdgeHit {
                edge: line.edge,
                pixel_distance: dist,
            });
        }
    }
    best
}
