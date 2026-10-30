//! Netz für den Austausch: Punkte in `f32` (Millimeter), Dreiecke als Punktnummern; dazu die
//! Lage eines Netz-Körpers.

use crate::error::ExchangeError;
use crate::limits::{MAX_TRIANGLES, MAX_VERTICES};
use planora_i18n::tr;
use planora_math::{DQuat, DVec3};
use planora_mesh::{MassProperties, Mesh, mass_properties};

/// Eingelesenes oder für die Ausgabe vorbereitetes Netz. Dreieck `k` sind die Punkte
/// `indices[3k..3k + 3]`, von außen gesehen gegen den Uhrzeigersinn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExchangeMesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl ExchangeMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Punkt `i` in `f64` (`i < vertex_count()`).
    pub fn point(&self, i: usize) -> DVec3 {
        let p = self.positions[i];
        DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
    }

    /// Prüft das Netz: mindestens ein Dreieck, nur ganze Dreiecke, Grenzen eingehalten, jede
    /// Punktnummer vorhanden, alle Zahlen endlich.
    pub fn validate(&self) -> Result<(), ExchangeError> {
        if self.indices.is_empty() {
            return Err(ExchangeError::Empty);
        }
        if !self.indices.len().is_multiple_of(3) {
            return Err(ExchangeError::Corrupt(tr!("exchange-index-count")));
        }
        if self.triangle_count() > MAX_TRIANGLES {
            return Err(ExchangeError::TooManyTriangles);
        }
        if self.positions.len() > MAX_VERTICES {
            return Err(ExchangeError::TooManyVertices);
        }
        let n = self.positions.len();
        if self.indices.iter().any(|&i| i as usize >= n) {
            return Err(ExchangeError::Corrupt(tr!("exchange-triangle-dangling")));
        }
        if !self
            .positions
            .iter()
            .all(|p| p.iter().all(|c| c.is_finite()))
        {
            return Err(ExchangeError::NotFinite { line: None });
        }
        Ok(())
    }

    /// Volumen, Oberfläche und Schwerpunkt nach der Lage `t` (Netz muss gültig sein).
    pub fn mass(&self, t: &MeshTransform) -> MassProperties {
        mass_properties(&self.placed(t))
    }

    /// Netz mit Punkten nach der Lage `t` in `f64`, ohne Normalen und Flächenbereiche (für
    /// Masseneigenschaften und Hüllquader).
    pub fn placed(&self, t: &MeshTransform) -> Mesh {
        Mesh {
            positions: (0..self.positions.len())
                .map(|i| t.apply(self.point(i)))
                .collect(),
            indices: self.indices.clone(),
            ..Mesh::default()
        }
    }

    /// Kompakte Form für das Dokument (Spec §7, Plan P12): Punktzahl und Dreieckszahl (u32),
    /// dann die Punkte (f32) und die Punktnummern (u32), alles little endian.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + 12 * self.positions.len() + 4 * self.indices.len());
        out.extend_from_slice(&(self.positions.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.triangle_count() as u32).to_le_bytes());
        for p in &self.positions {
            for c in p {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        for i in &self.indices {
            out.extend_from_slice(&i.to_le_bytes());
        }
        out
    }

    /// Erwartete Länge der Bytes aus ihren ersten acht; prüft die Anzahlen gegen die Grenzen,
    /// bevor etwas reserviert wird.
    pub fn byte_len_from_header(head: &[u8]) -> Result<u64, ExchangeError> {
        let Some(h) = head.get(..8) else {
            return Err(ExchangeError::Corrupt(tr!("exchange-data-too-short")));
        };
        let vertices = u64::from(u32::from_le_bytes([h[0], h[1], h[2], h[3]]));
        let triangles = u64::from(u32::from_le_bytes([h[4], h[5], h[6], h[7]]));
        if triangles > MAX_TRIANGLES as u64 {
            return Err(ExchangeError::TooManyTriangles);
        }
        if vertices > MAX_VERTICES as u64 {
            return Err(ExchangeError::TooManyVertices);
        }
        Ok(8 + 12 * vertices + 12 * triangles)
    }

    /// Liest die Form aus [`ExchangeMesh::to_bytes`] und prüft das Netz.
    pub fn from_bytes(bytes: &[u8]) -> Result<ExchangeMesh, ExchangeError> {
        let expected = Self::byte_len_from_header(bytes)?;
        if expected != bytes.len() as u64 {
            return Err(ExchangeError::Corrupt(tr!("exchange-data-length")));
        }
        let vertices = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let (points, numbers) = bytes[8..].split_at(12 * vertices);
        let floats: Vec<f32> = points
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        let mesh = ExchangeMesh {
            positions: floats.as_chunks::<3>().0.to_vec(),
            indices: numbers
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect(),
        };
        mesh.validate()?;
        Ok(mesh)
    }
}

/// Lage eines Netz-Körpers: erst skalieren, dann drehen, dann verschieben (Millimeter).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshTransform {
    pub offset: DVec3,
    pub rotation: DQuat,
    pub scale: f64,
}

impl MeshTransform {
    pub const IDENTITY: MeshTransform = MeshTransform {
        offset: DVec3::ZERO,
        rotation: DQuat::IDENTITY,
        scale: 1.0,
    };

    pub fn apply(&self, p: DVec3) -> DVec3 {
        self.offset + self.rotation * (p * self.scale)
    }
}

impl Default for MeshTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}
