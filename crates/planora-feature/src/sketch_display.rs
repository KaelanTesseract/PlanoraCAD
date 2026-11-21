//! Anzeigeschalter je Skizze (Skizzen 12): Schnittansicht, Profile, Punkte, Maße, Abhängigkeiten,
//! Konstruktions- und projizierte Geometrie sowie „Maß fertig“. Gehört zum Dokument und wird mit
//! ihm gespeichert; ändert nur die Ansicht, nicht das Modell. Nur Abweichungen von der Vorgabe
//! werden gehalten.

use crate::model::{Document, FeatureId, FeatureKind};
use std::collections::BTreeMap;

/// Anzeigeschalter einer Skizze.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SketchDisplay {
    /// Schnittansicht („Slice“): Körper an der Skizzenebene geschnitten zeigen.
    pub slice: bool,
    pub profiles: bool,
    pub points: bool,
    pub dimensions: bool,
    pub constraints: bool,
    pub construction: bool,
    pub projected: bool,
    /// Maße auch in der fertigen Skizze zeigen.
    pub dimensions_finished: bool,
}

impl Default for SketchDisplay {
    fn default() -> Self {
        SketchDisplay {
            slice: false,
            profiles: true,
            points: true,
            dimensions: true,
            constraints: true,
            construction: true,
            projected: true,
            dimensions_finished: false,
        }
    }
}

impl SketchDisplay {
    /// Der Grundzustand? Dann steht nichts in der Datei.
    pub fn is_default(&self) -> bool {
        *self == SketchDisplay::default()
    }
}

impl Document {
    /// Anzeigeschalter der Skizze `id`; die Vorgabe, wenn nichts abweicht.
    pub fn sketch_display(&self, id: FeatureId) -> SketchDisplay {
        self.sketch_display.get(&id).copied().unwrap_or_default()
    }

    /// Setzt die Anzeigeschalter einer Skizze; für andere Features geschieht nichts. Die Vorgabe
    /// entfernt den Eintrag. Der Änderungszähler steigt nur bei einer Änderung und das Modell
    /// rechnet nicht neu ([`Document::edit_count`], nicht [`Document::model_edit_count`]).
    pub fn set_sketch_display(&mut self, id: FeatureId, d: SketchDisplay) {
        if !self
            .feature(id)
            .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        {
            return;
        }
        if d == self.sketch_display(id) {
            return;
        }
        if d.is_default() {
            self.sketch_display.remove(&id);
        } else {
            self.sketch_display.insert(id, d);
        }
        self.drawing_edits += 1;
    }

    /// `m` ohne Einträge zu Skizzen, die es nicht gibt, und ohne Vorgabewerte (gelöschte Skizzen,
    /// von Hand geänderte Dateien).
    pub(crate) fn pruned_sketch_display(
        &self,
        mut m: BTreeMap<FeatureId, SketchDisplay>,
    ) -> BTreeMap<FeatureId, SketchDisplay> {
        m.retain(|id, d| {
            !d.is_default()
                && self
                    .feature(*id)
                    .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        });
        m
    }
}
