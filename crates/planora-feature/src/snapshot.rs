//! Reine Daten eines Dokuments zum Speichern und Laden (ohne Ergebnisse und Zwischenspeicher).

use crate::error::FeatureError;
use crate::model::{Document, Feature, FeatureId, FeatureKind, FeatureStatus, MeshId};
use planora_exchange::ExchangeMesh;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct SnapshotFeature {
    pub id: FeatureId,
    pub name: String,
    pub suppressed: bool,
    pub kind: FeatureKind,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub features: Vec<SnapshotFeature>,
    pub marker: usize,
    pub next_id: u64,
    /// Netze der Netz-Körper; nur die, auf die ein Feature verweist (Plan P11).
    pub meshes: BTreeMap<MeshId, Arc<ExchangeMesh>>,
}

impl Document {
    pub fn snapshot(&self) -> Snapshot {
        let used = self.used_meshes();
        Snapshot {
            features: self
                .features
                .iter()
                .map(|f| SnapshotFeature {
                    id: f.id,
                    name: f.name.clone(),
                    suppressed: f.suppressed,
                    kind: f.kind.clone(),
                })
                .collect(),
            marker: self.marker,
            next_id: self.next_id,
            meshes: self
                .meshes
                .iter()
                .filter(|(id, _)| used.contains(id))
                .map(|(id, m)| (*id, Arc::clone(m)))
                .collect(),
        }
    }

    /// Baut ein Dokument aus einem Snapshot und berechnet es einmal. Prüft Ids, Namen, Marker,
    /// Netze und Verweise; ungültige Zahlen führen erst bei der Berechnung zu einem Fehlerstatus
    /// am betroffenen Feature (kein Panic, kein Abbruch des Ladens).
    pub fn from_snapshot(s: Snapshot, chord_tolerance: f64) -> Result<Document, FeatureError> {
        let bad = |m: &str| FeatureError::InvalidInput(m.to_string());
        if s.next_id == u64::MAX {
            return Err(bad("Der Kennungszähler ist ungültig"));
        }
        if s.marker > s.features.len() {
            return Err(bad("Der Rückrollmarker liegt hinter dem letzten Feature"));
        }
        let mut seen = std::collections::HashSet::with_capacity(s.features.len());
        let mut doc = Document::with_options(chord_tolerance);
        for f in s.features.iter() {
            if f.id.0 == 0 || f.id.0 >= s.next_id {
                return Err(bad("Ein Feature hat eine ungültige Kennung"));
            }
            if !seen.insert(f.id) {
                return Err(bad("Eine Feature-Kennung kommt doppelt vor"));
            }
            if f.name.trim().is_empty() {
                return Err(bad("Ein Feature hat keinen Namen"));
            }
        }
        for (id, mesh) in &s.meshes {
            if id.0 == 0 {
                return Err(bad("Ein Netz hat eine ungültige Kennung"));
            }
            mesh.validate()
                .map_err(|e| FeatureError::InvalidInput(format!("Netz {}: {e}", id.0)))?;
        }
        doc.next_mesh = match s.meshes.last_key_value() {
            Some((id, _)) => {
                id.0.checked_add(1)
                    .ok_or_else(|| bad("Ein Netz hat eine ungültige Kennung"))?
            }
            None => 1,
        };
        doc.meshes = s.meshes;
        for f in s.features {
            let version = doc.bump();
            doc.features.push(Feature {
                id: f.id,
                name: f.name,
                suppressed: f.suppressed,
                kind: f.kind,
                status: FeatureStatus::NotComputed,
                version,
                result: None,
                failed: None,
            });
        }
        for idx in 0..doc.features.len() {
            doc.check_references(&doc.features[idx].kind, idx)?;
        }
        doc.marker = s.marker;
        doc.next_id = s.next_id;
        doc.recompute();
        doc.edits = 0;
        Ok(doc)
    }
}
