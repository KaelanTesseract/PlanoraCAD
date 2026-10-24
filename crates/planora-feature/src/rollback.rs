//! Rückrollstellen je Körper (Verlaufsleiste): Körperspitzen, Ketten und die Stellen, die einen
//! Schritt samt allem, was von ihm abhängt, zurücksetzen. Der globale Marker bleibt daneben
//! unverändert; aktiv ist ein Schritt nur vor dem Marker und außerhalb jeder Rückrollstelle.

use crate::error::FeatureError;
use crate::model::{Document, FeatureId, FeatureKind};
use crate::recompute::RecomputeReport;
use std::collections::HashSet;

/// Erzeugt die Art einen Körper (Skizzen nicht)?
fn produces_body(kind: &FeatureKind) -> bool {
    !matches!(kind, FeatureKind::Sketch(_))
}

/// Die Features, deren Körper die Art verbraucht (ersetzt), wie in der Sichtbarkeitsberechnung.
fn consumed_by(kind: &FeatureKind) -> Vec<FeatureId> {
    match kind {
        FeatureKind::Boolean { target, tool, .. } => vec![target.feature, tool.feature],
        FeatureKind::Move { body, .. } | FeatureKind::Verrunden { body, .. } => vec![body.feature],
        _ => Vec::new(),
    }
}

impl Document {
    /// Schritte, die einen Körper erzeugen und von keinem späteren Schritt verbraucht werden
    /// (Vereinigen, Verschieben, Verrunden), in Listenordnung. Hängt nur von den Arten ab, nicht
    /// vom Status: Ein zurückgesetzter Körper behält seine Spitze.
    pub fn body_tips(&self) -> Vec<FeatureId> {
        let consumed: HashSet<FeatureId> = self
            .features
            .iter()
            .flat_map(|f| consumed_by(&f.kind))
            .collect();
        self.features
            .iter()
            .filter(|f| produces_body(&f.kind) && !consumed.contains(&f.id))
            .map(|f| f.id)
            .collect()
    }

    /// `tip` und alles, wovon er (auch über Umwege) abhängt, in Listenordnung; Skizzen
    /// eingeschlossen. Für ein unbekanntes Feature eine leere Liste.
    pub fn chain(&self, tip: FeatureId) -> Vec<FeatureId> {
        let Some(end) = self.index_of(tip) else {
            return Vec::new();
        };
        // Verweise zeigen nur nach vorn: ein Durchlauf rückwärts reicht.
        let mut needed: HashSet<FeatureId> = HashSet::from([tip]);
        for f in self.features[..=end].iter().rev() {
            if needed.contains(&f.id) {
                needed.extend(f.kind.dependencies());
            }
        }
        self.features[..=end]
            .iter()
            .filter(|f| needed.contains(&f.id))
            .map(|f| f.id)
            .collect()
    }

    /// Die aktuellen Rückrollstellen in Listenordnung.
    pub fn rollbacks(&self) -> &[FeatureId] {
        &self.rollbacks
    }

    /// Die Rückrollstelle in der Kette von `tip` (höchstens eine, siehe
    /// [`Document::set_rollback`]); liegen mehrere darin (Kette eines Vereinigens mit Stellen in
    /// beiden Eingangsketten), die vorderste.
    pub fn rollback_in_chain(&self, tip: FeatureId) -> Option<FeatureId> {
        let chain = self.chain(tip);
        self.rollbacks.iter().copied().find(|r| chain.contains(r))
    }

    /// Setzt die Rückrollstelle der Kette von `tip` auf `step` und berechnet neu. `step` muss
    /// in [`Document::chain`] von `tip` liegen, sonst [`FeatureError::InvalidInput`]; ein
    /// unbekannter `tip` ergibt [`FeatureError::UnknownFeature`]. Bei einem Fehler bleibt das
    /// Dokument unverändert.
    ///
    /// Die Stelle ersetzt alle Stellen in der Kette von `tip` und alle Stellen hinter `step`
    /// (abhängige Schritte sind ohnehin zurückgesetzt und haben keine eigene Stelle).
    pub fn set_rollback(&mut self, tip: FeatureId, step: FeatureId) -> Result<(), FeatureError> {
        self.set_rollback_with_report(tip, step).map(|_| ())
    }

    /// Wie [`Document::set_rollback`], liefert den Bericht der Neuberechnung.
    pub fn set_rollback_with_report(
        &mut self,
        tip: FeatureId,
        step: FeatureId,
    ) -> Result<RecomputeReport, FeatureError> {
        self.index_of(tip).ok_or(FeatureError::UnknownFeature)?;
        let chain = self.chain(tip);
        if !chain.contains(&step) {
            return Err(FeatureError::InvalidInput(
                "Der Schritt gehört nicht zum Verlauf dieses Körpers".into(),
            ));
        }
        let behind = self.dependents(step);
        let mut next: Vec<FeatureId> = self
            .rollbacks
            .iter()
            .copied()
            .filter(|r| !chain.contains(r) && !behind.contains(r))
            .collect();
        next.push(step);
        self.replace_rollbacks(next);
        Ok(self.recompute())
    }

    /// Entfernt die Rückrollstelle(n) in der Kette von `tip` und berechnet neu;
    /// [`FeatureError::UnknownFeature`] bei unbekanntem `tip`.
    pub fn clear_rollbacks(&mut self, tip: FeatureId) -> Result<(), FeatureError> {
        self.clear_rollbacks_with_report(tip).map(|_| ())
    }

    /// Wie [`Document::clear_rollbacks`], liefert den Bericht der Neuberechnung (unveränderte
    /// Schritte kommen aus dem Zwischenspeicher).
    pub fn clear_rollbacks_with_report(
        &mut self,
        tip: FeatureId,
    ) -> Result<RecomputeReport, FeatureError> {
        self.index_of(tip).ok_or(FeatureError::UnknownFeature)?;
        let chain = self.chain(tip);
        let next = self
            .rollbacks
            .iter()
            .copied()
            .filter(|r| !chain.contains(r))
            .collect();
        self.replace_rollbacks(next);
        Ok(self.recompute())
    }

    /// Entfernt alle Rückrollstellen und berechnet neu.
    pub fn clear_all_rollbacks(&mut self) {
        self.replace_rollbacks(Vec::new());
        self.recompute();
    }

    /// Ist das Feature aktiv: vor dem globalen Marker und weder selbst eine Rückrollstelle noch
    /// von einer abhängig? Für ein unbekanntes Feature `false`.
    pub fn is_active(&self, id: FeatureId) -> bool {
        self.index_of(id)
            .is_some_and(|idx| idx < self.marker && !self.rolled_back().contains(&id))
    }

    /// Alle von einer Rückrollstelle erfassten Features: die Stellen selbst und alles, was von
    /// ihnen abhängt (Verweise zeigen nur nach vorn, ein Durchlauf reicht).
    pub(crate) fn rolled_back(&self) -> HashSet<FeatureId> {
        let mut found: HashSet<FeatureId> = self.rollbacks.iter().copied().collect();
        if found.is_empty() {
            return found;
        }
        for f in &self.features {
            if !found.contains(&f.id) && f.kind.dependencies().iter().any(|d| found.contains(d)) {
                found.insert(f.id);
            }
        }
        found
    }

    /// Übernimmt die Stellen in Listenordnung; zählt eine Änderung, wenn sie sich ändern.
    fn replace_rollbacks(&mut self, mut next: Vec<FeatureId>) {
        next.sort_by_key(|r| self.index_of(*r));
        next.dedup();
        if next != self.rollbacks {
            self.rollbacks = next;
            self.edits += 1;
        }
    }

    /// Hält die Stellen gültig (vor jeder Neuberechnung): Stellen gelöschter Features entfallen,
    /// ebenso Stellen hinter einer anderen Stelle (nach einer Änderung der Verweise); der Rest
    /// steht in Listenordnung. Zählt keine Änderung (Folge einer schon gezählten).
    pub(crate) fn prune_rollbacks(&mut self) {
        if self.rollbacks.is_empty() {
            return;
        }
        let mut kept: Vec<FeatureId> = self
            .rollbacks
            .iter()
            .copied()
            .filter(|r| self.index_of(*r).is_some())
            .collect();
        kept.sort_by_key(|r| self.index_of(*r));
        kept.dedup();
        let all = kept.clone();
        kept.retain(|r| {
            let chain = self.chain(*r);
            !all.iter().any(|o| o != r && chain.contains(o))
        });
        self.rollbacks = kept;
    }
}
