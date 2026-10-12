use crate::discretize::Discretizer;
use crate::mesh::{EdgeLine, FaceRange, Mesh, TessellationOptions};
use planora_kernel::{Face, Id, Loop, Solid, Surface, UvRect};
use planora_math::{DVec3, Frame};
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
    let disc = Discretizer::new(solid, options.chord_tolerance);
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
                if matches!(face.surface, Surface::Nurbs(_)) || crate::trimmed::is_trimmed(&g) {
                    let rings: Vec<Vec<DVec3>> = std::iter::once(&face.outer)
                        .chain(face.inner.iter())
                        .map(|lp| loop_points(lp, &polylines))
                        .collect();
                    crate::trimmed::tessellate_trimmed(&g, &rings, &disc, &mut mesh, fid.index())?;
                } else {
                    let patch = face.patch.ok_or(MeshError::MissingPatch(fid.index()))?;
                    tessellate_patch(face, &patch, &disc, &mut mesh)?;
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
    let triangles = earcutr::earcut(&flat, &holes, 2).map_err(|_| PlanarError::Triangulation)?;

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

fn tessellate_patch(
    face: &Face,
    patch: &UvRect,
    disc: &Discretizer,
    mesh: &mut Mesh,
) -> Result<(), MeshError> {
    let nu = disc.segments(patch.u1 - patch.u0);
    let nv = match face.surface {
        Surface::Sphere { .. } | Surface::Torus { .. } => disc.segments(patch.v1 - patch.v0),
        _ => 1,
    };
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
        let v = patch.v0 + (patch.v1 - patch.v0) * j as f64 / nv as f64;
        for i in 0..=nu {
            let u = patch.u0 + (patch.u1 - patch.u0) * i as f64 / nu as f64;
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
}
