//! Neuberechnung der Zeitleiste mit Zwischenspeicher (Versionen statt Hashes, Entscheidung F3),
//! Änderungsoperationen und Sichtbarkeit der Körper.

use crate::compute::{Computed, compute_feature};
use crate::error::FeatureError;
use crate::model::{
    BodyRef, Document, Feature, FeatureId, FeatureKind, FeatureResult, FeatureStatus, SketchPlane,
};
use planora_kernel::Solid;
use planora_sketch::Sketch;

/// Welche Features eine Neuberechnung berechnet und welche sie übernommen hat.
#[derive(Clone, Debug, Default)]
pub struct RecomputeReport {
    pub computed: Vec<FeatureId>,
    pub reused: Vec<FeatureId>,
}

/// Ein sichtbarer Körper mit Verweis, Geometrie und Anzeigename.
#[derive(Clone, Debug)]
pub struct VisibleBody<'a> {
    pub body: BodyRef,
    pub solid: &'a Solid,
    pub name: String,
}

/// Grund, aus dem ein Feature wegen eines Eingangsfeatures nicht berechnet wird.
enum Blocked {
    Skipped(String),
    Error(FeatureError),
}

impl Document {
    /// Fügt ein Feature am Rückführungsmarker ein und berechnet neu. Alle Verweise müssen auf
    /// Features vor der Einfügestelle zeigen; sonst bleibt das Dokument unverändert.
    pub fn add_feature(
        &mut self,
        name: impl Into<String>,
        kind: FeatureKind,
    ) -> Result<FeatureId, FeatureError> {
        for dep in kind.dependencies() {
            match self.index_of(dep) {
                None => return Err(FeatureError::UnknownFeature),
                Some(i) if i >= self.marker => return Err(FeatureError::ForwardReference),
                Some(_) => {}
            }
        }
        let id = FeatureId(self.next_id);
        self.next_id += 1;
        let version = self.bump();
        self.features.insert(
            self.marker,
            Feature {
                id,
                name: name.into(),
                suppressed: false,
                kind,
                status: FeatureStatus::NotComputed,
                version,
                result: None,
            },
        );
        self.marker += 1;
        self.recompute();
        Ok(id)
    }

    /// Unterdrückt ein Feature oder hebt die Unterdrückung auf und berechnet neu.
    pub fn set_suppressed(&mut self, id: FeatureId, suppressed: bool) -> Result<(), FeatureError> {
        let idx = self.index_of(id).ok_or(FeatureError::UnknownFeature)?;
        let version = self.bump();
        let f = &mut self.features[idx];
        f.suppressed = suppressed;
        f.version = version;
        self.recompute();
        Ok(())
    }

    /// Setzt den Rückführungsmarker auf `n` (Anzahl der aktiven Features) und berechnet neu.
    pub fn set_marker(&mut self, n: usize) -> Result<(), FeatureError> {
        if n > self.features.len() {
            return Err(FeatureError::InvalidInput(format!(
                "Der Marker kann höchstens hinter Feature {} stehen",
                self.features.len()
            )));
        }
        self.marker = n;
        self.recompute();
        Ok(())
    }

    /// Berechnet alle Features vor dem Marker, deren Eingaben sich geändert haben.
    pub fn recompute(&mut self) -> RecomputeReport {
        let mut report = RecomputeReport::default();
        for idx in 0..self.features.len() {
            let id = self.features[idx].id;
            if idx >= self.marker {
                // Das Ergebnis bleibt, damit das Zurückholen des Markers billig ist.
                self.features[idx].status = FeatureStatus::NotComputed;
                continue;
            }
            if self.features[idx].suppressed {
                self.features[idx].status = FeatureStatus::Suppressed;
                self.features[idx].result = None;
                continue;
            }
            match self.blocked_by_dependency(idx) {
                Some(Blocked::Skipped(why)) => {
                    self.features[idx].status = FeatureStatus::Skipped(why);
                    self.features[idx].result = None;
                    continue;
                }
                Some(Blocked::Error(e)) => {
                    self.features[idx].status = FeatureStatus::Error(e.to_string());
                    self.features[idx].result = None;
                    continue;
                }
                None => {}
            }
            let visible = self.visible_at(idx);
            let key = self.cache_key(idx, &visible);
            if self.features[idx]
                .result
                .as_ref()
                .is_some_and(|r| r.key == key)
            {
                self.features[idx].status = FeatureStatus::Ok;
                report.reused.push(id);
                continue;
            }
            match compute_feature(self, idx, &visible) {
                Ok(computed) => {
                    let result_version = self.bump();
                    self.store(idx, computed, key, result_version);
                    self.features[idx].status = FeatureStatus::Ok;
                }
                Err(e) => {
                    self.features[idx].result = None;
                    self.features[idx].status = FeatureStatus::Error(e.to_string());
                }
            }
            report.computed.push(id);
        }
        report
    }

    /// Sichtbare Körper in der Reihenfolge der Zeitleiste.
    pub fn visible_bodies(&self) -> Vec<VisibleBody<'_>> {
        self.visible_at(self.features.len())
            .into_iter()
            .filter_map(|r| {
                let f = self.feature(r.feature)?;
                let bodies = &f.result.as_ref()?.bodies;
                let solid = bodies.get(r.index)?;
                let name = if bodies.len() > 1 {
                    format!("{} {}", f.name, r.index + 1)
                } else {
                    f.name.clone()
                };
                Some(VisibleBody {
                    body: r,
                    solid,
                    name,
                })
            })
            .collect()
    }

    /// Ergebniskörper eines Features, unabhängig von seiner Sichtbarkeit.
    pub fn body(&self, r: BodyRef) -> Option<&Solid> {
        self.feature(r.feature)?
            .result
            .as_ref()?
            .bodies
            .get(r.index)
    }

    /// Berechnete Skizze eines Skizzen-Features (aufgelöste Ebene, nachgeführte Kantenkopien).
    pub fn sketch_of(&self, id: FeatureId) -> Option<&Sketch> {
        self.feature(id)?.result.as_ref()?.sketch.as_ref()
    }

    /// Spielt die Zeitleiste bis vor `end` nach und liefert die dann sichtbaren Körper,
    /// geordnet nach dem Feature, das sie erzeugt hat.
    pub(crate) fn visible_at(&self, end: usize) -> Vec<BodyRef> {
        let mut visible: Vec<BodyRef> = Vec::new();
        for f in &self.features[..end.min(self.features.len())] {
            if f.status != FeatureStatus::Ok {
                continue;
            }
            let Some(result) = &f.result else { continue };
            match &f.kind {
                FeatureKind::Boolean { target, tool, .. } => {
                    visible.retain(|r| r != target && r != tool);
                }
                FeatureKind::Move { body, .. } => visible.retain(|r| r != body),
                _ => {}
            }
            visible.extend((0..result.bodies.len()).map(|index| BodyRef {
                feature: f.id,
                index,
            }));
        }
        visible
    }

    /// Liest das Feature die sichtbaren Körper (und nicht nur seine Eingangsfeatures)?
    fn reads_visible(kind: &FeatureKind) -> bool {
        match kind {
            FeatureKind::Boolean { .. } | FeatureKind::Move { .. } => true,
            FeatureKind::Sketch(s) => {
                matches!(s.plane, SketchPlane::OnFace(_)) || !s.copies.is_empty()
            }
            _ => false,
        }
    }

    fn result_version(&self, id: FeatureId) -> u64 {
        self.feature(id)
            .and_then(|f| f.result.as_ref())
            .map_or(0, |r| r.result_version)
    }

    /// Schlüssel der Eingaben: eigene Version, Ergebnisstände der Eingangsfeatures und bei
    /// lesenden Features die Ergebnisstände aller sichtbaren Körper.
    fn cache_key(&self, idx: usize, visible: &[BodyRef]) -> Vec<u64> {
        let f = &self.features[idx];
        let mut key = vec![f.version];
        key.extend(
            f.kind
                .dependencies()
                .into_iter()
                .map(|d| self.result_version(d)),
        );
        if Self::reads_visible(&f.kind) {
            key.extend(visible.iter().map(|r| self.result_version(r.feature)));
        }
        key
    }

    /// Prüft die Eingangsfeatures von `idx` auf Unterdrückung, Fehler und Lage.
    fn blocked_by_dependency(&self, idx: usize) -> Option<Blocked> {
        for dep in self.features[idx].kind.dependencies() {
            let Some(di) = self.index_of(dep) else {
                return Some(Blocked::Error(FeatureError::UnknownFeature));
            };
            if di >= idx {
                return Some(Blocked::Error(FeatureError::ForwardReference));
            }
            let d = &self.features[di];
            if d.suppressed {
                return Some(Blocked::Skipped(format!(
                    "Feature „{}“ ist unterdrückt",
                    d.name
                )));
            }
            match d.status {
                FeatureStatus::Error(_) => {
                    return Some(Blocked::Skipped(format!(
                        "abhängig von Feature „{}“ (Fehler)",
                        d.name
                    )));
                }
                FeatureStatus::Skipped(_) => {
                    return Some(Blocked::Skipped(format!(
                        "abhängig von Feature „{}“ (ausgesetzt)",
                        d.name
                    )));
                }
                _ => {}
            }
        }
        None
    }

    /// Speichert ein neues Ergebnis und schreibt aktualisierte Hinweispunkte ohne
    /// Versionsänderung in die Definition zurück.
    fn store(&mut self, idx: usize, computed: Computed, key: Vec<u64>, result_version: u64) {
        let Computed {
            bodies,
            sketch,
            hints,
        } = computed;
        let f = &mut self.features[idx];
        f.result = Some(FeatureResult {
            bodies,
            sketch,
            result_version,
            key,
        });
        if let (Some(h), FeatureKind::Sketch(sf)) = (hints, &mut f.kind) {
            sf.plane = h.plane;
            sf.copies = h.copies;
        }
    }
}
