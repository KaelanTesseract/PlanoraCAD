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
///
/// `reused` enthält auch Features, deren Fehler aus dem letzten Lauf übernommen wurde (gleiche
/// Eingaben wie beim Fehlschlag): Sie haben dann den Status `Error` und kein Ergebnis.
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
    /// Fügt ein Feature am Rückrollmarker ein und berechnet neu. Alle Verweise müssen auf
    /// Features vor der Einfügestelle zeigen; sonst bleibt das Dokument unverändert.
    pub fn add_feature(
        &mut self,
        name: impl Into<String>,
        kind: FeatureKind,
    ) -> Result<FeatureId, FeatureError> {
        self.check_references(&kind, self.marker)?;
        let id = FeatureId(self.next_id);
        let next_id = self.next_id.checked_add(1).ok_or_else(|| {
            FeatureError::InvalidInput("Die Feature-Kennungen sind aufgebraucht".into())
        })?;
        self.next_id = next_id;
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
                failed: None,
            },
        );
        self.marker += 1;
        self.edits += 1;
        self.recompute();
        Ok(id)
    }

    /// Unterdrückt ein Feature oder hebt die Unterdrückung auf und berechnet neu.
    pub fn set_suppressed(&mut self, id: FeatureId, suppressed: bool) -> Result<(), FeatureError> {
        self.set_suppressed_with_report(id, suppressed).map(|_| ())
    }

    /// Wie [`Document::set_suppressed`], liefert den Bericht der Neuberechnung. Hat das
    /// Feature den Zustand schon, ändert sich nichts (keine neue Version, keine Neuberechnung,
    /// leerer Bericht).
    pub fn set_suppressed_with_report(
        &mut self,
        id: FeatureId,
        suppressed: bool,
    ) -> Result<RecomputeReport, FeatureError> {
        let idx = self.index_of(id).ok_or(FeatureError::UnknownFeature)?;
        if self.features[idx].suppressed == suppressed {
            return Ok(RecomputeReport::default());
        }
        let version = self.bump();
        let f = &mut self.features[idx];
        f.suppressed = suppressed;
        f.version = version;
        self.edits += 1;
        Ok(self.recompute())
    }

    /// Setzt den Rückrollmarker auf `n` (Anzahl der aktiven Features) und berechnet neu.
    pub fn set_marker(&mut self, n: usize) -> Result<(), FeatureError> {
        if n > self.features.len() {
            return Err(FeatureError::InvalidInput(format!(
                "Der Marker kann höchstens hinter Feature {} stehen",
                self.features.len()
            )));
        }
        if n != self.marker {
            self.marker = n;
            self.edits += 1;
        }
        self.recompute();
        Ok(())
    }

    /// Berechnet alle Features vor dem Marker, deren Eingaben sich geändert haben.
    pub fn recompute(&mut self) -> RecomputeReport {
        let report = self.recompute_features();
        self.sync_parts_sheets();
        report
    }

    /// Wie [`Document::recompute`], ohne die Stücklistenblätter abzugleichen (Laden ändert nichts).
    pub(crate) fn recompute_features(&mut self) -> RecomputeReport {
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
            let failed_before = self.features[idx]
                .failed
                .as_ref()
                .filter(|(k, _)| *k == key)
                .map(|(_, message)| message.clone());
            if let Some(message) = failed_before {
                // gleiche Eingaben wie beim letzten Fehlschlag: Fehler übernehmen
                self.features[idx].status = FeatureStatus::Error(message);
                self.features[idx].result = None;
                report.reused.push(id);
                continue;
            }
            match compute_feature(self, idx, &visible) {
                Ok(computed) => {
                    let result_version = self.bump();
                    self.store(idx, computed, key, result_version);
                    self.features[idx].status = FeatureStatus::Ok;
                    self.features[idx].failed = None;
                }
                Err(e) => {
                    let message = e.to_string();
                    let f = &mut self.features[idx];
                    f.result = None;
                    f.status = FeatureStatus::Error(message.clone());
                    f.failed = Some((key, message));
                }
            }
            report.computed.push(id);
        }
        report
    }

    /// Sichtbare Körper in der Reihenfolge der Zeitleiste (Ergebnisse von Verschieben und
    /// Booleschen Operationen stehen an der Stelle ihrer Eingangskörper).
    pub fn visible_bodies(&self) -> Vec<VisibleBody<'_>> {
        self.visible_bodies_at(self.features.len())
    }

    /// Die Körper, die unmittelbar vor dem Feature `id` sichtbar sind (in derselben Ordnung wie
    /// [`Document::visible_bodies`]). Für ein unbekanntes Feature eine leere Liste.
    pub fn visible_bodies_before(&self, id: FeatureId) -> Vec<VisibleBody<'_>> {
        match self.index_of(id) {
            Some(idx) => self.visible_bodies_at(idx),
            None => Vec::new(),
        }
    }

    /// Sichtbare Körper vor dem Index `end` mit Geometrie und Anzeigenamen.
    fn visible_bodies_at(&self, end: usize) -> Vec<VisibleBody<'_>> {
        self.visible_at(end)
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

    /// Spielt die Zeitleiste bis vor `end` nach und liefert die dann sichtbaren Körper in der
    /// Reihenfolge ihres Entstehens. Das Ergebnis eines Verschiebens oder einer Booleschen
    /// Operation nimmt die Stelle des vordersten Eingangskörpers ein, damit die Körperliste der
    /// Oberfläche beim Bearbeiten nicht umsortiert wird.
    pub(crate) fn visible_at(&self, end: usize) -> Vec<BodyRef> {
        let mut visible: Vec<BodyRef> = Vec::new();
        for f in &self.features[..end.min(self.features.len())] {
            if f.status != FeatureStatus::Ok {
                continue;
            }
            let Some(result) = &f.result else { continue };
            let produced = (0..result.bodies.len()).map(|index| BodyRef {
                feature: f.id,
                index,
            });
            let consumed = |r: &BodyRef| match &f.kind {
                FeatureKind::Boolean { target, tool, .. } => r == target || r == tool,
                FeatureKind::Move { body, .. } => r == body,
                FeatureKind::Verrunden { body, .. } => r == body,
                _ => false,
            };
            // Die erste verbrauchte Stelle bleibt beim Entfernen gleich (nur Spätere fallen weg).
            let at = visible.iter().position(consumed).unwrap_or(visible.len());
            visible.retain(|r| !consumed(r));
            let at = at.min(visible.len());
            visible.splice(at..at, produced);
        }
        visible
    }

    /// Liest das Feature die sichtbaren Körper (und nicht nur seine Eingangsfeatures)?
    fn reads_visible(kind: &FeatureKind) -> bool {
        match kind {
            FeatureKind::Boolean { .. }
            | FeatureKind::Move { .. }
            | FeatureKind::Verrunden { .. } => true,
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

#[cfg(test)]
mod tests {
    use crate::model::{BodyRef, Document, FeatureKind, FeatureStatus, SketchFeature, SketchPlane};
    use crate::refs::FaceRef;
    use planora_kernel::face_probe_point;
    use planora_math::{DVec2, DVec3, Frame, Tolerance};
    use planora_sketch::Sketch;

    /// Versatz des absichtlich falschen Hinweispunkts in Millimetern (deutlich größer als jede
    /// Toleranz, aber auf derselben Fläche auflösbar, weil die Marke eindeutig ist).
    const HINT_OFFSET: f64 = 3.0;

    #[test]
    fn hints_are_written_back_after_a_resolution_without_a_new_version() {
        let mut d = Document::new();
        let a = d
            .add_feature(
                "A",
                FeatureKind::Box {
                    size: DVec3::new(20.0, 20.0, 5.0),
                    origin: DVec3::ZERO,
                },
            )
            .unwrap();
        let tol = Tolerance::default();
        let solid = d
            .body(BodyRef {
                feature: a,
                index: 0,
            })
            .unwrap();
        let (top, _) = solid
            .faces
            .iter()
            .find(|(f, _)| {
                planora_sketch::plane_of_face(solid, *f)
                    .is_some_and(|fr| fr.z.z > 0.5 && fr.origin.z > 4.0)
            })
            .unwrap();
        let probe = face_probe_point(solid, top, &tol).unwrap();
        let mut face = FaceRef::from_face(solid, top, &tol).unwrap();
        face.hint += DVec3::new(HINT_OFFSET, -HINT_OFFSET, 0.0);
        let mut sk = Sketch::new(Frame::WORLD, "S", tol);
        sk.add_rectangle(DVec2::ZERO, DVec2::new(2.0, 2.0)).unwrap();
        let s = d
            .add_feature(
                "S",
                FeatureKind::Sketch(SketchFeature {
                    sketch: sk,
                    plane: SketchPlane::OnFace(face),
                    copies: vec![],
                }),
            )
            .unwrap();
        let f = d.feature(s).unwrap();
        assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
        let version = f.version;
        let FeatureKind::Sketch(sf) = &f.kind else {
            panic!("keine Skizze")
        };
        let SketchPlane::OnFace(written) = &sf.plane else {
            panic!("Ebene verloren")
        };
        assert_eq!(
            written.hint, probe,
            "Hinweispunkt = Prüfpunkt der gefundenen Fläche"
        );
        let again = d.recompute();
        assert!(again.computed.is_empty(), "{:?}", again.computed);
        assert_eq!(d.feature(s).unwrap().version, version, "keine neue Version");
    }

    #[test]
    fn setting_the_current_suppression_state_keeps_the_version() {
        let mut d = Document::new();
        let a = d
            .add_feature(
                "A",
                FeatureKind::Box {
                    size: DVec3::ONE,
                    origin: DVec3::ZERO,
                },
            )
            .unwrap();
        let (version, next) = (d.feature(a).unwrap().version, d.next_version);
        d.set_suppressed(a, false).unwrap();
        assert_eq!(d.feature(a).unwrap().version, version);
        assert_eq!(d.next_version, next, "nichts wurde neu berechnet");
    }
}
