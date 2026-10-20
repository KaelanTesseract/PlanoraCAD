//! Reine Daten eines Dokuments zum Speichern und Laden (ohne Ergebnisse und Zwischenspeicher).

use crate::error::FeatureError;
use crate::formulas::{Formulas, check_keys};
use crate::model::{Document, Feature, FeatureId, FeatureKind, FeatureStatus, MeshId};
use crate::params::{Parameter, check_table};
use planora_drawing::sheet_info::{ProjectInfo, validate_project_info};
use planora_drawing::{BodyKey, Drawing, RowOverride};
use planora_exchange::ExchangeMesh;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;

thread_local! {
    static MESH_VALIDATIONS: Cell<u64> = const { Cell::new(0) };
}

/// Zahl der Netzprüfungen (`ExchangeMesh::validate`) beim Bauen aus einem Snapshot
/// ([`Document::from_snapshot`], [`Document::restore`]) in diesem Thread seit Programmstart.
/// Nur für Tests, die prüfen, dass schon bekannte Netze nicht erneut geprüft werden (zählt im
/// aufrufenden Thread, deshalb unabhängig von parallel laufenden Tests).
#[doc(hidden)]
pub fn mesh_validations() -> u64 {
    MESH_VALIDATIONS.with(Cell::get)
}

/// Wie [`Document::restore`] ein Bild übernommen hat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreKind {
    /// Nur Zeichnungen, Projektangaben und Stücklistentexte ersetzt; die Körper blieben, wie sie
    /// waren (keine Neuberechnung, [`Document::model_edit_count`] unverändert).
    Annotations,
    /// Ganzes Dokument neu gebaut und berechnet (wie beim Laden).
    Full,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SnapshotFeature {
    pub id: FeatureId,
    pub name: String,
    pub suppressed: bool,
    pub kind: FeatureKind,
    /// Formeln der Zahlenfelder; die Felder in `kind` tragen den zuletzt berechneten Wert.
    pub formulas: Formulas,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub features: Vec<SnapshotFeature>,
    pub marker: usize,
    pub next_id: u64,
    /// Netze der Netz-Körper; nur die, auf die ein Feature verweist (Plan P11).
    pub meshes: BTreeMap<MeshId, Arc<ExchangeMesh>>,
    /// Zeichnungen des Dokuments.
    pub drawings: Vec<Drawing>,
    /// Nächste Zeichnungskennung (mindestens 1).
    pub next_drawing: u64,
    /// Projektangaben für das Schriftfeld.
    pub project_info: Option<ProjectInfo>,
    /// Eigene Texte der Stücklistenzeilen.
    pub parts_overrides: BTreeMap<BodyKey, RowOverride>,
    /// Parametertabelle (ohne Werte; sie werden beim Bauen berechnet).
    pub parameters: Vec<Parameter>,
    /// Nächste Parameterkennung (mindestens 1).
    pub next_param: u64,
}

/// Gleiche Netztabellen: gleiche Kennungen mit jeweils demselben `Arc` (nicht nur gleichem
/// Inhalt).
fn same_meshes(
    a: &BTreeMap<MeshId, Arc<ExchangeMesh>>,
    b: &BTreeMap<MeshId, Arc<ExchangeMesh>>,
) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|((ka, ma), (kb, mb))| ka == kb && Arc::ptr_eq(ma, mb))
}

impl Snapshot {
    /// Gleicher Inhalt in allen Feldern. Netze gelten nur als gleich, wenn es dieselben `Arc`
    /// sind; Skizzen nach ihrem Stand (siehe [`crate::SketchFeature`]).
    pub fn same_content(&self, other: &Snapshot) -> bool {
        self.same_model(other)
            && self.drawings == other.drawings
            && self.next_drawing == other.next_drawing
            && self.project_info == other.project_info
            && self.parts_overrides == other.parts_overrides
    }

    /// Gleiches Modell: Features (samt Formeltexten), Marker, Kennungszähler, Netze und Parametertabelle (samt
    /// Parameterzähler) gleich; Zeichnungen, Projektangaben und Stücklistentexte zählen nicht.
    /// Eine Änderung nur an Parametern oder Formeln nimmt deshalb in [`Document::restore`] den
    /// vollen Weg.
    pub fn same_model(&self, other: &Snapshot) -> bool {
        self.features == other.features
            && self.marker == other.marker
            && self.next_id == other.next_id
            && same_meshes(&self.meshes, &other.meshes)
            && self.parameters == other.parameters
            && self.next_param == other.next_param
    }
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
                    formulas: f.formulas.clone(),
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
            drawings: self.drawings.clone(),
            next_drawing: self.next_drawing,
            project_info: self.project_info.clone(),
            parts_overrides: self.parts_overrides.clone(),
            parameters: self.parameters.clone(),
            next_param: self.next_param,
        }
    }

    /// Baut ein Dokument aus einem Snapshot und berechnet es einmal. Prüft Ids, Namen, Marker,
    /// Netze und Verweise; ungültige Zahlen führen erst bei der Berechnung zu einem Fehlerstatus
    /// am betroffenen Feature (kein Panic, kein Abbruch des Ladens).
    pub fn from_snapshot(s: Snapshot, chord_tolerance: f64) -> Result<Document, FeatureError> {
        let mut doc = Document::build(s, chord_tolerance, &BTreeMap::new(), 1)?;
        doc.edits = 0;
        doc.drawing_edits = 0;
        Ok(doc)
    }

    /// Übernimmt den Zustand eines Bildes (Rückgängig/Wiederholen). Stimmt das Modell mit dem
    /// Dokument überein ([`Snapshot::same_model`]), werden nur Zeichnungen, Projektangaben und
    /// Stücklistentexte ersetzt (geprüft wie beim Laden, ohne Neuberechnung,
    /// [`RestoreKind::Annotations`]); sonst wird wie beim Laden gebaut und gerechnet
    /// ([`RestoreKind::Full`]), wobei Netze, die als derselbe `Arc` schon im Dokument liegen,
    /// nicht erneut geprüft werden.
    ///
    /// Die Änderungszähler steigen immer weiter (nie zurück auf 0 oder auf einen früheren Wert),
    /// damit Zwischenspeicher, die an [`Document::edit_count`] oder
    /// [`Document::model_edit_count`] hängen, nie ein altes Ergebnis zeigen:
    /// [`Document::edit_count`] steigt in beiden Fällen, [`Document::model_edit_count`] nur beim
    /// vollen Weg. Auch Feature-Versionen und Netz-Kennungen werden nicht wiederverwendet.
    ///
    /// Ein Bild mit einem NaN-Parameter ist nie gleich mit sich selbst und nimmt deshalb immer
    /// den vollen Weg (sicher, nur langsamer). Der volle Weg verwirft Netze, auf die kein
    /// Feature verweist (sie stehen nicht im Bild); eine solche `MeshId` gilt danach nicht mehr.
    ///
    /// Bei Fehler bleibt `self` unverändert.
    pub fn restore(&mut self, s: Snapshot) -> Result<RestoreKind, FeatureError> {
        if self.snapshot().same_model(&s) {
            if let Some(p) = &s.project_info {
                validate_project_info(p)
                    .map_err(|e| FeatureError::InvalidInput(format!("Projektangaben: {e}")))?;
            }
            crate::parts::check_overrides(&s.parts_overrides)?;
            // Als letzte Prüfung: `set_drawings` ändert das Dokument erst, wenn alles stimmt.
            self.set_drawings(s.drawings, s.next_drawing)?;
            self.project_info = s.project_info;
            self.parts_overrides = s.parts_overrides;
            self.drawing_edits += 1;
            return Ok(RestoreKind::Annotations);
        }
        let mut doc = Document::build(s, self.chord_tolerance, &self.meshes, self.next_version)?;
        doc.edits = self.edits + 1;
        doc.drawing_edits = self.drawing_edits + 1;
        doc.next_mesh = doc.next_mesh.max(self.next_mesh);
        *self = doc;
        Ok(RestoreKind::Full)
    }

    /// Baut und berechnet ein Dokument aus einem Snapshot. Netze, die als derselbe `Arc` in
    /// `known` liegen, gelten als geprüft; Versionen beginnen bei `first_version`.
    fn build(
        s: Snapshot,
        chord_tolerance: f64,
        known: &BTreeMap<MeshId, Arc<ExchangeMesh>>,
        first_version: u64,
    ) -> Result<Document, FeatureError> {
        let bad = |m: &str| FeatureError::InvalidInput(m.to_string());
        if s.next_id == u64::MAX {
            return Err(bad("Der Kennungszähler ist ungültig"));
        }
        if s.marker > s.features.len() {
            return Err(bad("Der Rückrollmarker liegt hinter dem letzten Feature"));
        }
        // Parametertabelle vor den Features: Kennungen, dann die ganze Tabelle.
        if s.next_param == 0 || s.next_param == u64::MAX {
            return Err(bad("Der Parameterzähler ist ungültig"));
        }
        let mut seen_params = std::collections::HashSet::with_capacity(s.parameters.len());
        for p in &s.parameters {
            if p.id.0 == 0 || p.id.0 >= s.next_param {
                return Err(bad("Ein Parameter hat eine ungültige Kennung"));
            }
            if !seen_params.insert(p.id) {
                return Err(bad("Eine Parameter-Kennung kommt doppelt vor"));
            }
        }
        let param_values = check_table(&s.parameters)
            .map_err(|m| FeatureError::InvalidInput(format!("Parametertabelle: {m}")))?;
        let mut seen = std::collections::HashSet::with_capacity(s.features.len());
        let mut doc = Document::with_options(chord_tolerance);
        doc.next_version = first_version.max(1);
        doc.parameters = s.parameters;
        doc.param_values = param_values;
        doc.next_param = s.next_param;
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
            // Formeln an Feldern, die die Art nicht hat: Bild kaputt. Eine kaputte Formel selbst
            // lädt dagegen; das Feature zeigt dann den Fehlerstatus (Spec §4a.7, P3).
            check_keys(&f.kind, &f.formulas).map_err(|_| {
                bad(&format!(
                    "Feature „{}“: Dieses Feld kann keine Formel haben",
                    f.name
                ))
            })?;
        }
        for (id, mesh) in &s.meshes {
            if id.0 == 0 {
                return Err(bad("Ein Netz hat eine ungültige Kennung"));
            }
            if known.values().any(|k| Arc::ptr_eq(k, mesh)) {
                continue;
            }
            MESH_VALIDATIONS.with(|c| c.set(c.get() + 1));
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
                formulas: f.formulas,
                issue: None,
            });
        }
        for idx in 0..doc.features.len() {
            doc.check_references(&doc.features[idx].kind, idx)?;
        }
        doc.set_drawings(s.drawings, s.next_drawing)?;
        if let Some(p) = &s.project_info {
            validate_project_info(p).map_err(|e| bad(&format!("Projektangaben: {e}")))?;
        }
        doc.project_info = s.project_info;
        crate::parts::check_overrides(&s.parts_overrides)?;
        doc.parts_overrides = s.parts_overrides;
        doc.marker = s.marker;
        doc.next_id = s.next_id;
        doc.recompute_features();
        Ok(doc)
    }
}
