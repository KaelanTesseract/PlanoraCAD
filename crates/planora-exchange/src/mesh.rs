//! Netz für den Austausch: Punkte in `f32` (Millimeter), Dreiecke als Punktnummern; dazu die
//! Lage eines Netz-Körpers.

use crate::error::ExchangeError;
use crate::limits::{MAX_TRIANGLES, MAX_VERTICES};
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
            return Err(ExchangeError::Corrupt(
                "Die Zahl der Punktnummern ist kein Vielfaches von drei".into(),
            ));
        }
        if self.triangle_count() > MAX_TRIANGLES {
            return Err(ExchangeError::TooManyTriangles);
        }
        if self.positions.len() > MAX_VERTICES {
            return Err(ExchangeError::TooManyVertices);
        }
        let n = self.positions.len();
        if self.indices.iter().any(|&i| i as usize >= n) {
            return Err(ExchangeError::Corrupt(
                "Ein Dreieck verweist auf einen Punkt, den es nicht gibt".into(),
            ));
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
