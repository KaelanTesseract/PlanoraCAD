//! Änderungsoperationen am Dokument: Parameter ändern, umbenennen, löschen (mit Folgen) und
//! die Prüfung der Dokument-Invarianten für Tests.

use crate::error::FeatureError;
use crate::formulas::{
    Formulas, check_keys, drop_overwritten_dims, evaluate, field_keys, prune_sketch, same_fields,
    same_values,
};
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
    ///
    /// Formeln (Plan P2): Ändert `f` den Wert eines Feldes mit Formel, entfällt **nur diese**
    /// Formel (die getippte Zahl gilt); Formeln für Felder, die die Art danach nicht mehr hat,
    /// entfallen ebenfalls. Alle übrigen Formeln bleiben und bestimmen ihre Felder weiter.
    /// Skizzen: Die Formeln der Maße stehen in [`crate::SketchFeature::formulas`] und lassen
    /// sich in `f` mit ändern. Ändert `f` den Wert eines Maßes mit Formel, ohne deren Text neu
    /// zu setzen, entfällt diese Formel; Formeln an gelöschten, anzeigenden oder nicht
    /// maßgebenden Abhängigkeiten entfallen ([`crate::SketchFeature::prune_formulas`]).
    pub fn update_with_report(
        &mut self,
        id: FeatureId,
        f: impl FnOnce(&mut FeatureKind),
    ) -> Result<RecomputeReport, FeatureError> {
        let (idx, mut kind) = self.updated_kind(id, f)?;
        let old = &self.features[idx];
        drop_overwritten_dims(&old.kind, &mut kind);
        let keys = field_keys(&kind);
        let formulas: Formulas = old
            .formulas
            .iter()
            .filter(|(k, _)| keys.contains(k) && same_fields(&old.kind, &kind, **k))
            .map(|(k, t)| (*k, t.clone()))
            .collect();
        self.commit_update(idx, kind, formulas)
    }

    /// Wendet `f` auf eine Kopie der Art an und prüft Variante und Verweise.
    pub(crate) fn updated_kind(
        &self,
        id: FeatureId,
        f: impl FnOnce(&mut FeatureKind),
    ) -> Result<(usize, FeatureKind), FeatureError> {
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
        Ok((idx, kind))
    }

    /// Prüft die Formeln des Kandidaten streng (P3) und übernimmt erst dann Art und Formeln:
    /// neue Version, eine Änderung, Neuberechnung. Bei Skizzen entfallen vorher die Formeln an
    /// Abhängigkeiten, die keine haben können.
    pub(crate) fn commit_update(
        &mut self,
        idx: usize,
        mut kind: FeatureKind,
        formulas: Formulas,
    ) -> Result<RecomputeReport, FeatureError> {
        prune_sketch(&mut kind);
        let f = &self.features[idx];
        Document::check_new_broken(
            Some(f.id),
            &f.name,
            &kind,
            &formulas,
            &self.param_scope(),
            &self.broken_set(None),
        )?;
        let version = self.bump();
        let feature = &mut self.features[idx];
        feature.kind = kind;
        feature.formulas = formulas;
        feature.version = version;
        self.edits += 1;
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
        if self.features[idx].name != name {
            self.features[idx].name = name;
            self.edits += 1;
            // Gleichnamige Körper fassen die Stückliste zusammen oder teilen sie wieder.
            self.sync_parts_sheets();
        }
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

    /// Löscht ein Feature samt allen abhängigen Features, passt den Rückrollmarker an, entfernt
    /// Rückrollstellen an gelöschten Features und berechnet neu. Liefert die entfernten Ids in
    /// Zeitleistenreihenfolge.
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
        // Doppelt abgesichert: `prune_rollbacks` in der Neuberechnung entfernt sie ebenfalls.
        self.rollbacks.retain(|r| !removed.contains(r));
        self.prune_meshes();
        self.prune_step_shapes();
        self.edits += 1;
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
        if let FeatureKind::MeshBody { mesh, .. } = kind
            && !self.meshes.contains_key(mesh)
        {
            return Err(FeatureError::MeshMissing(mesh.0));
        }
        if let FeatureKind::StepBody { shape } = kind
            && !self.step_shapes.contains_key(shape)
        {
            return Err(FeatureError::StepShapeMissing(shape.0));
        }
        Ok(())
    }

    /// Prüft die Invarianten des Dokuments (für Tests): eindeutige Ids, Verweise nur auf
    /// frühere Features, Marker im Bereich, gültige Rückrollstellen, Features ab dem Marker oder
    /// von einer Rückrollstelle erfasst `NotComputed` und alle aktiven nie `NotComputed` (außer
    /// unterdrückt), Ergebnis
    /// genau bei `Ok` (alte Ergebnisse dürfen nur `NotComputed`/`Suppressed` behalten) und
    /// gültige sichtbare Körper, gültige Parametertabelle mit aktuellen Werten und
    /// abgeschlossener Formeldurchgang (siehe `check_formula_invariants`). Liefert die erste
    /// Verletzung als Text.
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
        self.check_rollback_invariants()?;
        let rolled_back = self.rolled_back();
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
            if rolled_back.contains(&f.id) && f.status != FeatureStatus::NotComputed {
                return Err(format!(
                    "Feature „{}“ ist zurückgesetzt, hat aber Status {:?}",
                    f.name, f.status
                ));
            }
            let active = idx < self.marker && !rolled_back.contains(&f.id);
            if active && !f.suppressed && f.status == FeatureStatus::NotComputed {
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
            if active && f.suppressed != (f.status == FeatureStatus::Suppressed) {
                return Err(format!(
                    "Feature „{}“: Unterdrückung und Status {:?} passen nicht zusammen",
                    f.name, f.status
                ));
            }
        }
        self.check_parameter_invariants()?;
        self.check_formula_invariants()?;
        for v in self.visible_bodies() {
            if let Err(e) = self.kernel.validate(v.solid) {
                return Err(format!("Sichtbarer Körper „{}“ ist ungültig: {e}", v.name));
            }
        }
        Ok(())
    }

    /// Rückrollstellen: vorhandene Features, ohne Doppelte, in Listenordnung, keine in der Kette
    /// einer anderen (höchstens eine je Kette).
    fn check_rollback_invariants(&self) -> Result<(), String> {
        let mut last = None;
        for r in &self.rollbacks {
            let Some(idx) = self.index_of(*r) else {
                return Err(format!("Rückrollstelle {} verweist auf kein Feature", r.0));
            };
            if last.is_some_and(|l| l >= idx) {
                return Err("Rückrollstellen nicht in Listenordnung oder doppelt".into());
            }
            last = Some(idx);
            let chain = self.chain(*r);
            if let Some(o) = self.rollbacks.iter().find(|o| *o != r && chain.contains(o)) {
                return Err(format!(
                    "Rückrollstelle {} liegt in der Kette der Stelle {}",
                    o.0, r.0
                ));
            }
        }
        Ok(())
    }

    /// Parametertabelle: Kennungen eindeutig und vergeben, Tabelle gültig (Namen, Kreise,
    /// Einheiten, Kommentare), gespeicherte Werte bitgenau wie berechnet.
    fn check_parameter_invariants(&self) -> Result<(), String> {
        if self.next_param == 0 {
            return Err("Parameterzähler ist 0".into());
        }
        for (i, p) in self.parameters.iter().enumerate() {
            if p.id.0 == 0 || p.id.0 >= self.next_param {
                return Err(format!("Parameter-Id {} wurde nie vergeben", p.id.0));
            }
            if self.parameters[..i].iter().any(|q| q.id == p.id) {
                return Err(format!("Parameter-Id {} ist doppelt", p.id.0));
            }
        }
        let values = crate::params::check_table(&self.parameters)
            .map_err(|m| format!("Parametertabelle ungültig: {m}"))?;
        if values.len() != self.param_values.len()
            || values
                .iter()
                .zip(&self.param_values)
                .any(|(a, b)| a.to_bits() != b.to_bits())
        {
            return Err(format!(
                "Parameterwerte veraltet: {:?} statt {values:?}",
                self.param_values
            ));
        }
        Ok(())
    }

    /// Formeln: nur Felder der Art; der Auswertungsdurchgang ist abgeschlossen (Felder und
    /// Formelfehler wie mit den aktuellen Parametern berechnet, unbekannte Namen, falsche
    /// Einheiten und nicht endliche Werte also nur als Formelfehler); ein aktives Feature mit
    /// Fehler (vor dem Marker, nicht zurückgesetzt), nicht unterdrückt, hat den Status
    /// `Error(meldung)`.
    fn check_formula_invariants(&self) -> Result<(), String> {
        let scope = self.param_scope();
        let tolerance = self.kernel.tolerance.linear;
        let rolled_back = self.rolled_back();
        for (idx, f) in self.features.iter().enumerate() {
            check_keys(&f.kind, &f.formulas).map_err(|e| format!("Feature „{}“: {e}", f.name))?;
            if let FeatureKind::Sketch(sf) = &f.kind
                && sf.clone().prune_formulas()
            {
                return Err(format!(
                    "Feature „{}“: Formel an einer Abhängigkeit, die keine haben kann",
                    f.name
                ));
            }
            let ev = evaluate(&f.kind, &f.formulas, &scope, tolerance);
            if let Some(k) = field_keys(&f.kind)
                .into_iter()
                .find(|k| !same_fields(&f.kind, &ev.kind, *k))
            {
                return Err(format!(
                    "Feature „{}“: Feld {k:?} trägt nicht den Wert seiner Formel",
                    f.name
                ));
            }
            if !same_values(&f.kind, &ev.kind) {
                return Err(format!(
                    "Feature „{}“: ein Skizzenmaß trägt nicht den Wert seiner Formel",
                    f.name
                ));
            }
            if ev.issue != f.issue {
                return Err(format!(
                    "Feature „{}“: Formelfehler {:?} statt {:?}",
                    f.name, f.issue, ev.issue
                ));
            }
            if let Some(issue) = &f.issue
                && idx < self.marker
                && !rolled_back.contains(&f.id)
                && !f.suppressed
                && f.status != FeatureStatus::Error(issue.message().to_string())
            {
                return Err(format!(
                    "Feature „{}“ hat einen Formelfehler, aber Status {:?}",
                    f.name, f.status
                ));
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

    #[test]
    fn stale_parameters_and_formulas_violate_the_invariants() {
        use crate::expr::ParamUnit;
        use crate::formulas::{FieldKey, FormulaIssue};
        let fresh = || {
            let mut d = Document::new();
            d.add_parameter("Breite", "40", ParamUnit::Length, "")
                .unwrap();
            let mut f = crate::formulas::Formulas::new();
            f.insert(FieldKey::Breite, "Breite".to_string());
            d.add_feature_with_formulas(
                "A",
                FeatureKind::Box {
                    size: DVec3::ONE,
                    origin: DVec3::ZERO,
                },
                f,
            )
            .unwrap();
            d.check_invariants().unwrap();
            (d, 0)
        };
        let check = |d: &Document, want: &str| {
            let e = d.check_invariants().unwrap_err();
            assert!(e.contains(want), "{e}");
        };
        // Werte der Tabelle veraltet
        let (mut d, _) = fresh();
        d.param_values[0] = 41.0;
        check(&d, "Parameterwerte veraltet");
        // Tabelle ungültig (Kreis)
        let (mut d, _) = fresh();
        d.parameters[0].formula = "Breite+1".into();
        check(&d, "Parametertabelle ungültig");
        // doppelte Kennung
        let (mut d, _) = fresh();
        let mut p = d.parameters[0].clone();
        p.name = "Zweite".into();
        d.parameters.push(p);
        d.param_values.push(40.0);
        check(&d, "doppelt");
        // Feld trägt nicht den Formelwert
        let (mut d, i) = fresh();
        if let FeatureKind::Box { size, .. } = &mut d.features[i].kind {
            size.x = 39.0;
        }
        check(&d, "trägt nicht den Wert");
        // Formel an einem Feld, das die Art nicht hat
        let (mut d, i) = fresh();
        d.features[i]
            .formulas
            .insert(FieldKey::Radius, "1".to_string());
        check(&d, "kann keine Formel haben");
        // unbekannter Name ohne Formelfehler
        let (mut d, i) = fresh();
        d.features[i]
            .formulas
            .insert(FieldKey::Breite, "Gibtsnicht".to_string());
        check(&d, "Formelfehler");
        // Formelfehler ohne Fehlerstatus
        let (mut d, i) = fresh();
        d.features[i]
            .formulas
            .insert(FieldKey::Breite, "Gibtsnicht".to_string());
        d.features[i].issue = Some(FormulaIssue::Broken(
            "Den Namen „Gibtsnicht“ gibt es nicht.".into(),
        ));
        check(&d, "hat einen Formelfehler, aber Status");
    }
}
