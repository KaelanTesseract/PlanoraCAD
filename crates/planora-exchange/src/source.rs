//! Einheitliche Quelle für die Ausgabe (Spec §2, Plan P1): Netze von Kernel-Körpern und
//! Netz-Körper laufen über [`TriSource`] in ein [`ExportBody`].

use crate::check::weld_exact;
use crate::error::ExchangeError;
use crate::limits::{MAX_TRIANGLES, MAX_VERTICES};
use crate::mesh::{ExchangeMesh, MeshTransform};
use planora_math::DVec3;
use planora_mesh::Mesh;

/// Dreiecke mit Punkten in `f64` (Millimeter), schon nach der Körpertransformation.
pub trait TriSource {
    fn name(&self) -> &str;
    fn vertex_count(&self) -> usize;
    /// Punkt `i` (`i < vertex_count()`).
    fn vertex(&self, i: usize) -> DVec3;
    fn triangle_count(&self) -> usize;
    /// Punktnummern des Dreiecks `i` (`i < triangle_count()`).
    fn triangle(&self, i: usize) -> [u32; 3];
}

/// Netz eines Kernel-Körpers (aus `planora_mesh::tessellate`, Punkte schon an ihrer Lage).
pub struct SolidSource<'a> {
    pub name: &'a str,
    pub mesh: &'a Mesh,
}

impl TriSource for SolidSource<'_> {
    fn name(&self) -> &str {
        self.name
    }

    fn vertex_count(&self) -> usize {
        self.mesh.positions.len()
    }

    fn vertex(&self, i: usize) -> DVec3 {
        self.mesh.positions[i]
    }

    fn triangle_count(&self) -> usize {
        self.mesh.indices.len() / 3
    }

    fn triangle(&self, i: usize) -> [u32; 3] {
        self.mesh.indices.as_chunks::<3>().0[i]
    }
}

/// Netz-Körper: eingelesenes Netz mit seiner Lage.
pub struct PlacedSource<'a> {
    pub name: &'a str,
    pub mesh: &'a ExchangeMesh,
    pub transform: MeshTransform,
}

impl TriSource for PlacedSource<'_> {
    fn name(&self) -> &str {
        self.name
    }

    fn vertex_count(&self) -> usize {
        self.mesh.positions.len()
    }

    fn vertex(&self, i: usize) -> DVec3 {
        self.transform.apply(self.mesh.point(i))
    }

    fn triangle_count(&self) -> usize {
        self.mesh.triangle_count()
    }

    fn triangle(&self, i: usize) -> [u32; 3] {
        self.mesh.indices.as_chunks::<3>().0[i]
    }
}

/// Ein Körper, fertig für die Schreiber: Name und verschmolzenes Netz in `f32`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportBody {
    pub name: String,
    pub mesh: ExchangeMesh,
}

/// Liest eine Quelle in ein [`ExportBody`]: Punkte nach `f32` (nicht darstellbar → `NotFinite`),
/// Punktnummern geprüft, bitgleiche Punkte verschmolzen.
pub fn collect_body(src: &dyn TriSource) -> Result<ExportBody, ExchangeError> {
    let (n, t) = (src.vertex_count(), src.triangle_count());
    if t > MAX_TRIANGLES {
        return Err(ExchangeError::TooManyTriangles);
    }
    if n > MAX_VERTICES {
        return Err(ExchangeError::TooManyVertices);
    }
    let mut positions = Vec::with_capacity(n);
    for i in 0..n {
        let p = src.vertex(i);
        let q = [p.x as f32, p.y as f32, p.z as f32];
        if !q.iter().all(|c| c.is_finite()) {
            return Err(ExchangeError::NotFinite { line: None });
        }
        positions.push(q);
    }
    let mut indices = Vec::with_capacity(3 * t);
    for i in 0..t {
        let tri = src.triangle(i);
        if tri.iter().any(|&k| k as usize >= n) {
            return Err(ExchangeError::Corrupt(
                "Ein Dreieck verweist auf einen Punkt, den es nicht gibt".into(),
            ));
        }
        indices.extend_from_slice(&tri);
    }
    Ok(ExportBody {
        name: src.name().to_string(),
        mesh: weld_exact(&positions, &indices),
    })
}
