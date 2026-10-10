//! Änderungsoperationen am Dokument: Parameter ändern, umbenennen, löschen (mit Folgen) und
//! die Prüfung der Dokument-Invarianten für Tests.

use crate::error::FeatureError;
use crate::model::{Document, FeatureId, FeatureKind, FeatureStatus};
use crate::recompute::RecomputeReport;
use planora_kernel::Kernel;

impl Document {
    /// Ändert die Parameter eines Features und berechnet neu; liefert den Bericht des Laufs.
    ///
    /// `f` arbeitet auf einer Kopie der Art. Die Variante darf sich nicht ändern
    /// ([`FeatureError::InvalidInput`]), alle Verweise müssen auf vorhandene Features vor dem
    /// Feature zeigen ([`FeatureError::UnknownFeature`], [`FeatureError::ForwardReference`]).
    /// Bei einem Fehler bleibt das Dokument unverändert.
    pub fn update_with_report(
        &mut self,
        id: FeatureId,
        f: impl FnOnce(&mut FeatureKind),
    ) -> Result<RecomputeReport, FeatureError> {
        let idx = self.index_of(id).ok_or(FeatureError::UnknownFeature)?;
        let mut kind = self.features[idx].kind.clone();
        let before = std::mem::discriminant(&kind);
        f(&mut kind);
        if std::mem::discriminant(&kind) != before {
            return Err(FeatureError::InvalidInput(
                "Die Art des Features lässt sich nicht ändern".into(),
            ));
        }
        self.check_references(&kind, idx)?;
        let version = self.bump();
        let feature = &mut self.features[idx];
        feature.kind = kind;
        feature.version = version;
        Ok(self.recompute())
    }

    /// Wie [`Document::update_with_report`], ohne den Bericht.
    pub fn update(
        &mut self,
        id: FeatureId,
        f: impl FnOnce(&mut FeatureKind),
    ) -> Result<(), FeatureError> {
        self.update_with_report(id, f).map(|_| ())
    }

    /// Benennt ein Feature um. Ein leerer (oder nur aus Leerzeichen bestehender) Name ist
    /// ungültig. Die Version ändert sich nicht, es wird nichts neu berechnet.
    pub fn rename(&mut self, id: FeatureId, name: impl Into<String>) -> Result<(), FeatureError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(FeatureError::InvalidInput(
                "Der Name darf nicht leer sein".into(),
            ));
        }
        let idx = self.index_of(id).ok_or(FeatureError::UnknownFeature)?;
        self.features[idx].name = name;
        Ok(())
    }

    /// Alle Features, die direkt oder über andere abhängige Features auf `id` verweisen, in
    /// der Reihenfolge der Zeitleiste. Da Verweise nur nach vorn zeigen, reicht ein Durchlauf.
    pub fn dependents(&self, id: FeatureId) -> Vec<FeatureId> {
        let mut found: Vec<FeatureId> = Vec::new();
        for f in &self.features {
            if f.id == id {
                continue;
            }
            if f.kind
                .dependencies()
                .iter()
                .any(|d| *d == id || found.contains(d))
            {
                found.push(f.id);
            }
        }
        found
    }

    /// Löscht ein Feature, von dem nichts abhängt, und berechnet neu. Hängen andere Features
    /// davon ab, liefert es [`FeatureError::HasDependents`] und ändert nichts.
    pub fn delete(&mut self, id: FeatureId) -> Result<(), FeatureError> {
        self.index_of(id).ok_or(FeatureError::UnknownFeature)?;
        let deps = self.dependents(id);
        if !deps.is_empty() {
            return Err(FeatureError::HasDependents(
                deps.into_iter().map(|d| d.0).collect(),
            ));
        }
        self.delete_with_dependents(id).map(|_| ())
    }

    /// Löscht ein Feature samt allen abhängigen Features, passt den Rückrollmarker an und
    /// berechnet neu. Liefert die entfernten Ids in Zeitleistenreihenfolge.
    pub fn delete_with_dependents(
        &mut self,
        id: FeatureId,
    ) -> Result<Vec<FeatureId>, FeatureError> {
        self.index_of(id).ok_or(FeatureError::UnknownFeature)?;
        let mut removed = vec![id];
        removed.extend(self.dependents(id));
        let before_marker = self.features[..self.marker.min(self.features.len())]
            .iter()
            .filter(|f| removed.contains(&f.id))
            .count();
        self.features.retain(|f| !removed.contains(&f.id));
        self.marker -= before_marker;
        self.recompute();
        Ok(removed)
    }

    /// Prüft, ob alle Verweise von `kind` auf vorhandene Features vor `idx` zeigen.
    pub(crate) fn check_references(
        &self,
        kind: &FeatureKind,
        idx: usize,
    ) -> Result<(), FeatureError> {
        for dep in kind.dependencies() {
            match self.index_of(dep) {
                None => return Err(FeatureError::UnknownFeature),
                Some(i) if i >= idx => return Err(FeatureError::ForwardReference),
                Some(_) => {}
            }
        }
        Ok(())
    }

    /// Prüft die Invarianten des Dokuments (für Tests): eindeutige Ids, Verweise nur auf
    /// frühere Features, Marker im Bereich, Features ab dem Marker `NotComputed` und davor nie
    /// `NotComputed` (außer unterdrückt), Ergebnis
    /// genau bei `Ok` (alte Ergebnisse dürfen nur `NotComputed`/`Suppressed` behalten) und
    /// gültige sichtbare Körper. Liefert die erste Verletzung als Text.
    pub fn check_invariants(&self) -> Result<(), String> {
        for (i, f) in self.features.iter().enumerate() {
            if self.features[..i].iter().any(|g| g.id == f.id) {
                return Err(format!("Feature-Id {} ist doppelt", f.id.0));
            }
            if f.id.0 == 0 || f.id.0 >= self.next_id {
                return Err(format!("Feature-Id {} wurde nie vergeben", f.id.0));
            }
        }
        if self.marker > self.features.len() {
            return Err(format!(
                "Marker {} hinter dem Ende ({} Features)",
                self.marker,
                self.features.len()
            ));
        }
        for (idx, f) in self.features.iter().enumerate() {
            if let Err(e) = self.check_references(&f.kind, idx) {
                return Err(format!("Feature „{}“: {e}", f.name));
            }
            if idx >= self.marker && f.status != FeatureStatus::NotComputed {
                return Err(format!(
                    "Feature „{}“ steht hinter dem Marker, hat aber Status {:?}",
                    f.name, f.status
                ));
            }
            if idx < self.marker && !f.suppressed && f.status == FeatureStatus::NotComputed {
                return Err(format!(
                    "Feature „{}“ steht vor dem Marker, wurde aber nicht berechnet",
                    f.name
                ));
            }
            match (&f.status, f.result.is_some()) {
                (FeatureStatus::Ok, false) => {
                    return Err(format!(
                        "Feature „{}“ ist Ok, hat aber kein Ergebnis",
                        f.name
                    ));
                }
                (FeatureStatus::Error(_) | FeatureStatus::Skipped(_), true) => {
                    return Err(format!(
                        "Feature „{}“ hat Status {:?} und trotzdem ein Ergebnis",
                        f.name, f.status
                    ));
                }
                _ => {}
            }
            if idx < self.marker && f.suppressed != (f.status == FeatureStatus::Suppressed) {
                return Err(format!(
                    "Feature „{}“: Unterdrückung und Status {:?} passen nicht zusammen",
                    f.name, f.status
                ));
            }
        }
        for v in self.visible_bodies() {
            if let Err(e) = self.kernel.validate(v.solid) {
                return Err(format!("Sichtbarer Körper „{}“ ist ungültig: {e}", v.name));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Document, FeatureKind, FeatureStatus};
    use planora_math::DVec3;

    #[test]
    fn a_not_computed_feature_before_the_marker_violates_the_invariants() {
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
        d.check_invariants().unwrap();
        let idx = d.index_of(a).unwrap();
        d.features[idx].status = FeatureStatus::NotComputed;
        d.features[idx].result = None;
        let e = d.check_invariants().unwrap_err();
        assert!(e.contains("nicht berechnet"), "{e}");
        // unterdrückt vor dem Marker ist erlaubt (Status Suppressed), hinter dem Marker auch
        d.set_suppressed(a, true).unwrap();
        d.check_invariants().unwrap();
        d.set_marker(0).unwrap();
        d.check_invariants().unwrap();
    }
}
