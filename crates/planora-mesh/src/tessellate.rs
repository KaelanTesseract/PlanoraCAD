use crate::discretize::Discretizer;
use crate::mesh::{EdgeLine, FaceRange, Mesh, TessellationOptions};
use planora_kernel::{Face, Id, Loop, Solid, Surface, UvRect};
use planora_math::{DVec2, DVec3, Frame};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MeshError {
    #[error("Fläche {0}: Triangulation fehlgeschlagen")]
    Triangulation(usize),
    #[error("Fläche {0}: gekrümmte Fläche ohne Parameterbereich")]
    MissingPatch(usize),
    #[error("Netz zu groß: mehr als 2³² Eckpunkte oder Indizes")]
    TooLarge,
    #[error("Fläche {0}: ein Randpunkt lässt sich nicht auf die Freiformfläche zurückrechnen")]
    OffSurface(usize),
}

/// Wandelt eine Anzahl in einen `u32`-Index um, ohne still abzuschneiden.
pub(crate) fn index_u32(n: usize) -> Result<u32, MeshError> {
    u32::try_from(n).map_err(|_| MeshError::TooLarge)
}

/// Prüft vorab, ob nach dem Anhängen von `extra` Elementen alle Indizes noch in `u32` passen.
pub(crate) fn ensure_room(current: usize, extra: usize) -> Result<(), MeshError> {
    current
        .checked_add(extra)
        .filter(|&n| n <= u32::MAX as usize)
        .map(|_| ())
        .ok_or(MeshError::TooLarge)
}

/// Zerlegt einen Körper in Dreiecke. Alle Dreiecke sind so orientiert, dass sie nach außen zeigen.
pub fn tessellate(solid: &Solid, options: &TessellationOptions) -> Result<Mesh, MeshError> {
    tessellate_with(solid, &Discretizer::new(solid, options.chord_tolerance))
}

/// Wie [`tessellate`], mit vorgegebener Teilung (etwa [`Discretizer::for_solids`], damit mehrere
/// Körper vergleichbar vernetzt werden).
pub fn tessellate_with(solid: &Solid, disc: &Discretizer) -> Result<Mesh, MeshError> {
    let disc = *disc;
    let polylines: Vec<Vec<DVec3>> = (0..solid.edges.len())
        .map(|i| disc.edge_polyline(solid, Id::from_index(i)))
        .collect();

    let mut mesh = Mesh::default();
    for (fid, face) in solid.faces.iter() {
        let first_index = index_u32(mesh.indices.len())?;
        match &face.surface {
            Surface::Plane { frame } => {
                tessellate_planar(face, frame, &polylines, &mut mesh).map_err(|e| match e {
                    PlanarError::Triangulation => MeshError::Triangulation(fid.index()),
                    PlanarError::Mesh(e) => e,
                })?;
            }
            _ => {
                let g = planora_kernel::boolean::face_geom(solid, fid);
                // Freiformflächen immer über die Triangulierung im Parameterraum: Der Rand
                // besteht dann genau aus den gemeinsamen Kantenpunkten (auch wenn die Kanten
                // analytisch sind und ihre Teilpunkte nicht auf Parameterlinien des Rasters
                // liegen), das Netz bleibt geschlossen.
                //
                // Analytische Flächen ohne Beschnitt nehmen das Raster über dem Parameterrechteck
                // (`tessellate_patch`), aber nur, wenn dessen Randpunkte genau die gemeinsamen
                // Kantenpunkte sind. Sonst (etwa ein Mantel, dessen Randkreis in mehrere Kanten
                // geteilt ist: gleiche Parameterfläche, aber zusätzliche Teilpunkte) hätte das
                // Raster andere Randpunkte als die Nachbarflächen, das Netz wäre offen; dann
                // ebenfalls die Triangulierung im Parameterraum.
                let rings = || -> Vec<Vec<DVec3>> {
                    std::iter::once(&face.outer)
                        .chain(face.inner.iter())
                        .map(|lp| loop_points(lp, &polylines))
                        .collect()
                };
                let patch = if matches!(face.surface, Surface::Nurbs(_))
                    || crate::trimmed::is_trimmed(&g)
                {
                    None
                } else {
                    let patch = face.patch.ok_or(MeshError::MissingPatch(fid.index()))?;
                    patch_boundary_matches(face, &patch, &disc, &rings()).then_some(patch)
                };
                match patch {
                    Some(patch) => tessellate_patch(face, &patch, &disc, &mut mesh)?,
                    None => crate::trimmed::tessellate_trimmed(
                        &g,
                        &rings(),
                        &disc,
                        &mut mesh,
                        fid.index(),
                    )?,
                }
            }
        }
        mesh.face_ranges.push(FaceRange {
            face: fid,
            first_index,
            index_count: index_u32(mesh.indices.len())? - first_index,
        });
    }
    mesh.edge_lines = polylines
        .into_iter()
        .enumerate()
        .map(|(i, points)| EdgeLine {
            edge: Id::from_index(i),
            points,
        })
        .collect();
    // Sicherheitsnetz in Debug-Builds: Jede Fläche hat genau ihre Schleifenpunkte als Rand.
    #[cfg(debug_assertions)]
    if let Err(e) = crate::check::check_face_boundaries(solid, &mesh) {
        panic!("Netz verletzt die Randbedingung: {e}");
    }
    Ok(mesh)
}

fn loop_points(lp: &Loop, polylines: &[Vec<DVec3>]) -> Vec<DVec3> {
    let mut pts = Vec::new();
    for he in &lp.half_edges {
        let poly = &polylines[he.edge.index()];
        if he.forward {
            pts.extend_from_slice(&poly[..poly.len() - 1]);
        } else {
            pts.extend(poly[1..].iter().rev());
        }
    }
    pts
}

enum PlanarError {
    Triangulation,
    Mesh(MeshError),
}

impl From<MeshError> for PlanarError {
    fn from(e: MeshError) -> Self {
        PlanarError::Mesh(e)
    }
}

fn tessellate_planar(
    face: &Face,
    frame: &Frame,
    polylines: &[Vec<DVec3>],
    mesh: &mut Mesh,
) -> Result<(), PlanarError> {
    let normal = if face.same_sense { frame.z } else { -frame.z };
    let rings: Vec<Vec<DVec3>> = std::iter::once(&face.outer)
        .chain(face.inner.iter())
        .map(|lp| loop_points(lp, polylines))
        .collect();

    let mut flat: Vec<f64> = Vec::new();
    let mut holes: Vec<usize> = Vec::new();
    for (i, ring) in rings.iter().enumerate() {
        if ring.len() < 3 {
            return Err(PlanarError::Triangulation);
        }
        if i > 0 {
            holes.push(flat.len() / 2);
        }
        for p in ring {
            let local = frame.to_local(*p);
            flat.push(local.x);
            flat.push(local.y);
        }
    }
    let mut triangles =
        earcutr::earcut(&flat, &holes, 2).map_err(|_| PlanarError::Triangulation)?;
    // `earcut` entfernt beim Verbinden der Löcher Punkte, die mit ihren Nachbarn auf einer
    // Geraden liegen (etwa wenn die Brücke zu einem Loch genau auf dessen Kante trifft); die
    // Nachbarflächen behalten diese Punkte, das Netz wäre an einem T-Stoß offen. Dann wird
    // dieselbe Fläche mit der eingeschränkten Delaunay-Triangulierung aufgebaut, die jeden
    // Randpunkt als Ecke behält. Gute `earcut`-Ergebnisse bleiben unverändert.
    let mut ranges = Vec::with_capacity(rings.len());
    let mut start = 0;
    for ring in &rings {
        ranges.push(start..start + ring.len());
        start += ring.len();
    }
    if !uses_every_boundary_edge_once(&triangles, &ranges) {
        let pts: Vec<DVec2> = flat
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| DVec2::new(c[0], c[1]))
            .collect();
        triangles = crate::trimmed::triangulate_planar_cdt(&pts, &ranges)
            .ok_or(PlanarError::Triangulation)?
            .into_iter()
            .flatten()
            .collect();
    }

    ensure_room(mesh.positions.len(), flat.len() / 2)?;
    ensure_room(mesh.indices.len(), triangles.len())?;
    let base = index_u32(mesh.positions.len())?;
    for p in rings.iter().flatten() {
        mesh.positions.push(*p);
        mesh.normals.push(normal);
    }
    // Durch `ensure_room` passen `base + a` usw. sicher in `u32`.
    for t in triangles.as_chunks::<3>().0 {
        let (a, mut b, mut c) = (t[0], t[1], t[2]);
        let area2 = (flat[2 * b] - flat[2 * a]) * (flat[2 * c + 1] - flat[2 * a + 1])
            - (flat[2 * c] - flat[2 * a]) * (flat[2 * b + 1] - flat[2 * a + 1]);
        // Gegen den Uhrzeigersinn in der Ebene = Normale +z des Frames.
        if area2 < 0.0 {
            std::mem::swap(&mut b, &mut c);
        }
        if !face.same_sense {
            std::mem::swap(&mut b, &mut c);
        }
        mesh.indices
            .extend_from_slice(&[base + a as u32, base + b as u32, base + c as u32]);
    }
    Ok(())
}

/// Ist die Triangulierung am Rand dicht? Jede Ringkante (Indizes `k`, `k + 1` je Ring) kommt in
/// genau einem Dreieck vor, jede andere Kante in genau zwei.
fn uses_every_boundary_edge_once(triangles: &[usize], rings: &[std::ops::Range<usize>]) -> bool {
    let key = |a: usize, b: usize| (a.min(b), a.max(b));
    let mut count: HashMap<(usize, usize), u32> = HashMap::new();
    for t in triangles.as_chunks::<3>().0 {
        for k in 0..3 {
            *count.entry(key(t[k], t[(k + 1) % 3])).or_insert(0) += 1;
        }
    }
    let mut boundary = 0;
    for r in rings {
        let n = r.len();
        for k in 0..n {
            let e = key(r.start + k, r.start + (k + 1) % n);
            if count.get(&e) != Some(&1) {
                return false;
            }
            boundary += 1;
        }
    }
    let ones = count.values().filter(|c| **c == 1).count();
    ones == boundary && count.values().all(|c| *c == 1 || *c == 2)
}

/// Teilung `(nu, nv)` des Rasters über dem Parameterrechteck einer analytischen Fläche.
fn patch_counts(face: &Face, patch: &UvRect, disc: &Discretizer) -> (usize, usize) {
    let nu = disc.segments(patch.u1 - patch.u0);
    let nv = match face.surface {
        Surface::Sphere { .. } | Surface::Torus { .. } => disc.segments(patch.v1 - patch.v0),
        _ => 1,
    };
    (nu, nv)
}

/// Parameter des Rasterpunkts `(i, j)` (genau wie in [`tessellate_patch`]).
fn patch_uv(patch: &UvRect, (nu, nv): (usize, usize), i: usize, j: usize) -> (f64, f64) {
    (
        patch.u0 + (patch.u1 - patch.u0) * i as f64 / nu as f64,
        patch.v0 + (patch.v1 - patch.v0) * j as f64 / nv as f64,
    )
}

/// Relativer Abstand (bezogen auf die größte Koordinate, mindestens 1), bis zu dem ein Randpunkt
/// des Rasters und ein gemeinsamer Kantenpunkt als derselbe Punkt gelten. Beide werten dieselbe
/// Geometrie nur mit verschiedenen Formeln aus (Kurve gegen Fläche); gemessen weichen sie in
/// geschlossenen Netzen höchstens um etwa 6e-14 ab (Kugel r = 100), echte Teilpunkte liegen
/// dagegen mindestens etwa 1e-4 neben dem nächsten Rasterpunkt.
const PATCH_MATCH_REL: f64 = 1e-12;

/// Sind die Randpunkte des Rasters über dem Parameterrechteck (Zeilen `j = 0, nv`, Spalten
/// `i = 0, nu`) als Punktmenge dieselben wie die gemeinsamen Kantenpunkte der Schleifen `rings`?
/// Nur dann schließt das Raster dicht an die Nachbarflächen an.
fn patch_boundary_matches(
    face: &Face,
    patch: &UvRect,
    disc: &Discretizer,
    rings: &[Vec<DVec3>],
) -> bool {
    let (nu, nv) = patch_counts(face, patch, disc);
    let mut grid: Vec<DVec3> = Vec::with_capacity(2 * (nu + nv) + 4);
    for j in 0..=nv {
        for i in 0..=nu {
            if i == 0 || i == nu || j == 0 || j == nv {
                let (u, v) = patch_uv(patch, (nu, nv), i, j);
                grid.push(face.surface.point(u, v));
            }
        }
    }
    let loops: Vec<DVec3> = rings.iter().flatten().copied().collect();
    let scale = grid
        .iter()
        .chain(loops.iter())
        .map(|p| p.abs().max_element())
        .fold(1.0, f64::max);
    if !scale.is_finite() {
        return false;
    }
    let eps = PATCH_MATCH_REL * scale;
    same_point_sets(&grid, &loops, eps)
}

/// Hat jeder Punkt von `a` einen Punkt von `b` höchstens `eps` entfernt und umgekehrt?
pub(crate) fn same_point_sets(a: &[DVec3], b: &[DVec3], eps: f64) -> bool {
    let sorted = |s: &[DVec3]| {
        let mut v = s.to_vec();
        v.sort_by(|p, q| p.x.total_cmp(&q.x));
        v
    };
    let covered = |set: &[DVec3], p: &DVec3| {
        let start = set.partition_point(|q| q.x < p.x - eps);
        set[start..]
            .iter()
            .take_while(|q| q.x <= p.x + eps)
            .any(|q| q.distance(*p) <= eps)
    };
    let (sa, sb) = (sorted(a), sorted(b));
    a.iter().all(|p| covered(&sb, p)) && b.iter().all(|p| covered(&sa, p))
}

fn tessellate_patch(
    face: &Face,
    patch: &UvRect,
    disc: &Discretizer,
    mesh: &mut Mesh,
) -> Result<(), MeshError> {
    let (nu, nv) = patch_counts(face, patch, disc);
    let sign = if face.same_sense { 1.0 } else { -1.0 };
    let vertex_count = (nu + 1).checked_mul(nv + 1).ok_or(MeshError::TooLarge)?;
    let index_count = nu
        .checked_mul(nv)
        .and_then(|n| n.checked_mul(6))
        .ok_or(MeshError::TooLarge)?;
    ensure_room(mesh.positions.len(), vertex_count)?;
    ensure_room(mesh.indices.len(), index_count)?;
    let base = index_u32(mesh.positions.len())?;
    for j in 0..=nv {
        for i in 0..=nu {
            let (u, v) = patch_uv(patch, (nu, nv), i, j);
            mesh.positions.push(face.surface.point(u, v));
            mesh.normals.push(face.surface.normal(u, v) * sign);
        }
    }
    // Durch `ensure_room` passt jeder Index sicher in `u32`.
    let idx = |i: usize, j: usize| base + (j * (nu + 1) + i) as u32;
    for j in 0..nv {
        for i in 0..nu {
            let (a, b, c, d) = (idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
            for tri in [[a, b, c], [a, c, d]] {
                push_triangle(mesh, tri, face.same_sense);
            }
        }
    }
    Ok(())
}

/// Fügt ein Dreieck hinzu und lässt entartete Dreiecke (an den Polen) weg.
fn push_triangle(mesh: &mut Mesh, tri: [u32; 3], keep_order: bool) {
    const EPS: f64 = 1e-12;
    let p = [
        mesh.positions[tri[0] as usize],
        mesh.positions[tri[1] as usize],
        mesh.positions[tri[2] as usize],
    ];
    if p[0].distance(p[1]) < EPS || p[1].distance(p[2]) < EPS || p[0].distance(p[2]) < EPS {
        return;
    }
    let t = if keep_order {
        tri
    } else {
        [tri[0], tri[2], tri[1]]
    };
    mesh.indices.extend_from_slice(&t);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_counts_beyond_u32_are_rejected() {
        assert_eq!(index_u32(7), Ok(7));
        assert_eq!(index_u32(u32::MAX as usize), Ok(u32::MAX));
        assert_eq!(index_u32(u32::MAX as usize + 1), Err(MeshError::TooLarge));
        assert_eq!(ensure_room(10, 20), Ok(()));
        assert_eq!(ensure_room(u32::MAX as usize, 0), Ok(()));
        assert_eq!(ensure_room(u32::MAX as usize, 1), Err(MeshError::TooLarge));
        assert_eq!(ensure_room(usize::MAX, 1), Err(MeshError::TooLarge));
    }

    #[test]
    fn point_sets_compare_both_ways_within_eps() {
        let a = [DVec3::ZERO, DVec3::X, DVec3::X, DVec3::Y];
        let b = [DVec3::Y, DVec3::new(1e-13, 0.0, 0.0), DVec3::X];
        assert!(same_point_sets(&a, &b, 1e-12));
        // ein zusätzlicher Teilpunkt auf einer Seite
        let c = [DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::new(0.5, 0.0, 0.0)];
        assert!(!same_point_sets(&a, &c, 1e-12));
        assert!(!same_point_sets(&c, &a, 1e-12));
        assert!(!same_point_sets(
            &a,
            &[DVec3::new(1e-9, 0.0, 0.0), DVec3::X, DVec3::Y],
            1e-12
        ));
    }
}
