use crate::camera::OrbitCamera;
use glam::{Vec2, Vec3};
use planora_kernel::{Edge, Face, Id};
use planora_math::{DVec2, DVec3, Frame};
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

/// Punkt der Strecke `a..b`, der dem Strahl am nächsten liegt (Parameter auf 0..1 begrenzt).
/// `None` bei entarteter oder fast paralleler Strecke.
fn ray_segment_closest(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3) -> Option<Vec3> {
    let e = b - a;
    let r = a - origin;
    let ee = e.dot(e);
    let de = e.dot(dir);
    let denom = ee - de * de;
    if ee.is_nan() || ee <= 1e-12 || denom <= 1e-6 * ee {
        return None;
    }
    let s = ((de * dir.dot(r) - e.dot(r)) / denom).clamp(0.0, 1.0);
    s.is_finite().then(|| a + e * s)
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
                // Tiefe aus der echten 3D-Geometrie: der Punkt der Kante, der dem Mausstrahl am
                // nächsten liegt (`s` aus dem Bildschirm ist nicht perspektivisch korrekt).
                let world = ray_segment_closest(ray_origin, ray_dir, wa, wb)
                    .unwrap_or_else(|| wa + (wb - wa) * s);
                let along = (world - ray_origin).dot(ray_dir);
                // Toleranz: relativ (Tessellierung, f32) plus die Verschiebung, die entsteht, weil der
                // Cursor bis zu `max_px` Pixel neben der Kante liegen darf (Pixelgröße in der Tiefe `od`,
                // doppelt gerechnet für schräg verlaufende Kanten) plus ein kleiner absoluter Anteil.
                let pixel_world = 2.0 * od * (camera.fov_y * 0.5).tan() / viewport.y;
                let tolerance = 1.0e-3 * od + 2.0 * max_px * pixel_world + 0.01;
                if along > od + tolerance {
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

/// Strahl in Weltkoordinaten; `dir` muss nicht normiert sein (Abstände gelten in Vielfachen von
/// `dir`, bei normiertem `dir` also in mm wie [`FaceHit::distance`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: DVec3,
    pub dir: DVec3,
}

/// Rechteck einer Konstruktionsebene (Skizzen 6): Mitte `frame.origin`, Kanten entlang
/// `frame.x` und `frame.y`, halbe Kantenlängen `half`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneRect {
    pub frame: Frame,
    pub half: DVec2,
}

impl PlaneRect {
    /// Die vier Ecken in Weltkoordinaten, gegen den Uhrzeigersinn um `frame.z`.
    pub fn corners(&self) -> [DVec3; 4] {
        let (hx, hy) = (self.half.x, self.half.y);
        [
            self.frame.point(-hx, -hy, 0.0),
            self.frame.point(hx, -hy, 0.0),
            self.frame.point(hx, hy, 0.0),
            self.frame.point(-hx, hy, 0.0),
        ]
    }
}

/// Trifft der Strahl das Rechteck (beidseitig)? Liefert die Strahllänge bis zum Treffer; parallel
/// zur Ebene, hinter dem Ursprung oder außerhalb des Rechtecks `None`.
pub fn pick_plane(ray: &Ray, rect: &PlaneRect) -> Option<f64> {
    let n = rect.frame.z;
    let denom = ray.dir.dot(n);
    if !denom.is_finite() || denom.abs() < 1e-12 {
        return None;
    }
    let t = (rect.frame.origin - ray.origin).dot(n) / denom;
    if !(t.is_finite() && t > 1e-9) {
        return None;
    }
    let local = rect.frame.to_local(ray.origin + ray.dir * t);
    (local.x.abs() <= rect.half.x && local.y.abs() <= rect.half.y).then_some(t)
}

/// Eckpunkt (Anfang oder Ende einer Kante) nächst der Mausposition, höchstens `max_px` entfernt,
/// unter den Netzen `meshes` (je mit dem Index ihres Körpers). Liefert Körper, Kante und
/// `end` (`false` = Anfang). `occluder` wie bei [`pick_edge`]: Weiter entfernte Ecken gelten als
/// verdeckt und zählen nicht.
pub fn pick_vertex(
    meshes: &[(usize, &Mesh)],
    camera: &OrbitCamera,
    viewport: Vec2,
    cursor: Vec2,
    max_px: f32,
    occluder: Option<f32>,
) -> Option<(usize, Id<Edge>, bool)> {
    if !(viewport.x >= 1.0 && viewport.y >= 1.0 && viewport.is_finite() && cursor.is_finite()) {
        return None;
    }
    let ndc = Vec2::new(
        2.0 * cursor.x / viewport.x - 1.0,
        1.0 - 2.0 * cursor.y / viewport.y,
    );
    let (ray_origin, ray_dir) = camera.ray(ndc);
    // Bester Treffer: Körper, Kante, Ende und Abstand in Pixeln.
    let mut best: Option<(usize, Id<Edge>, bool, f32)> = None;
    for &(body, mesh) in meshes {
        for line in &mesh.edge_lines {
            let (Some(first), Some(last)) = (line.points.first(), line.points.last()) else {
                continue;
            };
            for (end, p) in [(false, *first), (true, *last)] {
                let world = to_vec3(p);
                let Some(at) = camera.project(world, viewport) else {
                    continue;
                };
                let dist = (cursor - at).length();
                if dist.is_nan() || dist > max_px || best.is_some_and(|b| dist >= b.3) {
                    continue;
                }
                if let Some(od) = occluder {
                    // Gleiche Toleranz wie bei `pick_edge`: Der Zeiger darf bis `max_px` neben
                    // der Ecke liegen, der Strahl trifft dann eine Fläche etwas davor oder dahinter.
                    let along = (world - ray_origin).dot(ray_dir);
                    let pixel_world = 2.0 * od * (camera.fov_y * 0.5).tan() / viewport.y;
                    let tolerance = 1.0e-3 * od + 2.0 * max_px * pixel_world + 0.01;
                    if along > od + tolerance {
                        continue;
                    }
                }
                best = Some((body, line.edge, end, dist));
            }
        }
    }
    best.map(|(body, edge, end, _)| (body, edge, end))
}
