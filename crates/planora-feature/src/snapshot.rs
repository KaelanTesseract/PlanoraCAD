//! Reine Daten eines Dokuments zum Speichern und Laden (ohne Ergebnisse und Zwischenspeicher).

use crate::error::FeatureError;
use crate::model::{Document, Feature, FeatureId, FeatureKind, FeatureStatus};

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
}

impl Document {
    pub fn snapshot(&self) -> Snapshot {
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
        }
    }

    /// Baut ein Dokument aus einem Snapshot und berechnet es einmal. Prüft Ids, Namen, Marker
    /// und Verweise; ungültige Zahlen führen erst bei der Berechnung zu einem Fehlerstatus am
    /// betroffenen Feature (kein Panic, kein Abbruch des Ladens).
    pub fn from_snapshot(s: Snapshot, chord_tolerance: f64) -> Result<Document, FeatureError> {
        let bad = |m: &str| FeatureError::InvalidInput(m.to_string());
        if s.marker > s.features.len() {
            return Err(bad("Der Rückrollmarker liegt hinter dem letzten Feature"));
        }
        let mut doc = Document::with_options(chord_tolerance);
        for (i, f) in s.features.iter().enumerate() {
            if f.id.0 == 0 || f.id.0 >= s.next_id {
                return Err(bad("Ein Feature hat eine ungültige Kennung"));
            }
            if s.features[..i].iter().any(|g| g.id == f.id) {
                return Err(bad("Eine Feature-Kennung kommt doppelt vor"));
            }
            if f.name.trim().is_empty() {
                return Err(bad("Ein Feature hat keinen Namen"));
            }
        }
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
