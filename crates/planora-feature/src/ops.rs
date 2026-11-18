//! Änderungsoperationen am Dokument: Parameter ändern, umbenennen, löschen (mit Folgen) und
//! die Prüfung der Dokument-Invarianten für Tests.

use crate::compute::text_warning_lines;
use crate::error::FeatureError;
use crate::formulas::{
    Formulas, check_keys, drop_overwritten_dims, evaluate, field_keys, prune_sketch, same_fields,
    same_values,
};
use crate::model::{
    CopySource, CopyTarget, Document, FeatureId, FeatureKind, FeatureStatus, ProfileSel,
    SketchFeature, SketchPlane,
};
use crate::recompute::RecomputeReport;
use planora_i18n::tr;
use planora_kernel::Kernel;
use planora_sketch::text::TextId;
use planora_sketch::text_shape::TextShaper;
use planora_sketch::{ProfileRef, Sketch};

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
            return Err(FeatureError::InvalidInput(tr!("feature-kind-fixed")));
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

    /// „Skizzenebene neu festlegen“ (Skizzen 6, wie Fusion): Legt die Skizze `sketch` auf die
    /// Ebene `plane`. Die 2D-Geometrie bleibt in Skizzenkoordinaten gleich, nur das
    /// Koordinatensystem wechselt; Projektionen werden bei der Neuberechnung neu projiziert.
    /// Genau eine Änderung (ein Rückgängig-Schritt); eine gleiche Ebene ändert nichts.
    ///
    /// Die neue Ebene darf nur auf Features **vor** der Skizze verweisen, sonst
    /// [`FeatureError::ForwardReference`]. Das schließt auch Schleifen aus: Ein Feature, das von
    /// der Skizze abhängt (etwa ihre Extrusion), steht immer hinter ihr. Eine
    /// Konstruktionsebene muss ein Feature „Ebene“ sein. Bei einem Fehler bleibt das Dokument
    /// unverändert.
    pub fn set_sketch_plane(
        &mut self,
        sketch: FeatureId,
        plane: SketchPlane,
    ) -> Result<(), FeatureError> {
        let idx = self.index_of(sketch).ok_or(FeatureError::UnknownFeature)?;
        let f = &self.features[idx];
        let FeatureKind::Sketch(sf) = &f.kind else {
            return Err(FeatureError::InvalidInput(tr!(
                "feature-not-sketch",
                name = f.name.clone()
            )));
        };
        if sf.plane == plane {
            return Ok(());
        }
        if let SketchPlane::OnPlane(id) = &plane
            && let Some(p) = self.feature(*id)
            && !matches!(p.kind, FeatureKind::Plane(_))
        {
            return Err(FeatureError::InvalidInput(tr!(
                "feature-plane-not-plane",
                name = p.name.clone()
            )));
        }
        let mut sf = sf.clone();
        sf.plane = plane;
        let kind = FeatureKind::Sketch(sf);
        self.check_references(&kind, idx)?;
        // Die Formeln der Skizzenmaße stehen in der Art; Feldformeln bleiben wie sie sind.
        let formulas = self.features[idx].formulas.clone();
        self.commit_update(idx, kind, formulas).map(|_| ())
    }

    /// Setzt alle Texte aller Skizzen mit `shaper` neu (Spec G4): nach dem Laden, sobald die
    /// Schriftliste da ist, und wenn sich die Schriften ändern. Die Kurven behalten nach Spec B3
    /// ihre Ids, Extrusionen bleiben an ihren Profilen.
    ///
    /// `true` nur, wenn sich die Form einer Skizze wirklich geändert hat (fehlende oder andere
    /// Schrift). Dann gilt das Dokument als geändert ([`Document::edit_count`] steigt, damit
    /// Speichern schreibt, was zu sehen ist, und kein Zwischenspeicher ein altes Ergebnis zeigt).
    /// Ein Rückgängig-Schritt ist das nicht: Der Aufrufer nimmt den neuen Zählerstand ohne Schritt
    /// hin, wie nach dem Zurückladen. Liefert das Setzen dieselben Kurven, bleiben Daten und
    /// Zähler gleich. Ändert sich dabei nur die Warnung zu fehlenden Schriften oder zu Texten, die
    /// sich nicht setzen lassen (Spec C10), bekommt die Skizze eine neue Version und wird neu
    /// berechnet, damit die Warnung erscheint.
    ///
    /// Vorher bekommt jeder Text seinen gezeigten Inhalt mit den aktuellen Parametern und der
    /// Anzeigeeinheit (Skizzen 11b, [`crate::text_refs::apply_shown`]): Die App ruft das nach
    /// jeder Änderung der Parametertabelle, nach Rückgängig/Wiederholen und nach einem Wechsel
    /// der Längeneinheit.
    pub fn reshape_texts(&mut self, shaper: &dyn TextShaper) -> bool {
        let mut changed = false;
        let mut stale = false;
        let scope = self.param_scope();
        for idx in 0..self.features.len() {
            let FeatureKind::Sketch(sf) = &mut self.features[idx].kind else {
                continue;
            };
            if sf.sketch.texts().next().is_none() {
                continue;
            }
            let before = text_warning_lines(&sf.sketch);
            let old = sf.sketch.clone();
            crate::text_refs::apply_shown(&mut sf.sketch, &scope);
            let report = sf.sketch.reshape_texts(shaper);
            let warning_changed = text_warning_lines(&sf.sketch) != before;
            if report.changed {
                self.follow_text_profiles(idx, &old);
            }
            if report.changed || warning_changed {
                // Neuer Stand: Die Skizze und alles Abhängige rechnen neu.
                let version = self.bump();
                self.features[idx].version = version;
                stale = true;
            }
            changed |= report.changed;
        }
        if changed {
            self.edits += 1;
        }
        if stale {
            self.recompute();
        }
        changed
    }

    /// Profile, die dem Text folgen (Skizzen 11b, Prüf-Schwerpunkt 4): Hat das Neusetzen die
    /// Buchstaben der Skizze `idx` geändert (anderer Wert, gelöschter Parameter, andere Einheit),
    /// finden Extrusionen und Rotationen ihre Glyphenflächen nicht mehr; bei einem kürzeren Wert
    /// sind deren Kurven sogar tot. Wie in Fusion, wo ein Text als Ganzes ein Profil ist, wird
    /// eine solche Auswahl in der Definition umgeschrieben: Jeder Verweis, der in `old` (der Skizze
    /// vor dem Neusetzen) zu einem noch lebenden Text gehörte, wird durch alle heutigen Flächen
    /// dieses Textes ersetzt; die übrigen Verweise bleiben, wie sie sind (ein verlorener bleibt
    /// verloren). Eine Auswahl, die sich noch ganz auflösen lässt, bleibt unberührt. So stehen die
    /// neuen Verweise in Datei und Verlauf (kein eigener Schritt, wie das Neusetzen).
    fn follow_text_profiles(&mut self, idx: usize, old: &Sketch) {
        let sid = self.features[idx].id;
        let FeatureKind::Sketch(sf) = &self.features[idx].kind else {
            return;
        };
        let new = &sf.sketch;
        let mut extracted = None;
        let mut updates: Vec<(usize, Vec<ProfileRef>)> = Vec::new();
        for (j, f) in self.features.iter().enumerate() {
            let (FeatureKind::Extrude {
                sketch,
                profiles: ProfileSel::Refs(refs),
                ..
            }
            | FeatureKind::Revolve {
                sketch,
                profiles: ProfileSel::Refs(refs),
                ..
            }) = &f.kind
            else {
                continue;
            };
            if *sketch != sid {
                continue;
            }
            let extracted = extracted.get_or_insert_with(|| new.profiles());
            if extracted.resolve(refs).is_ok_and(|i| !i.is_empty()) {
                continue;
            }
            let mut texts: Vec<TextId> = Vec::new();
            let mut out: Vec<ProfileRef> = Vec::new();
            for r in refs {
                match old.text_of_profile(r).filter(|t| new.text(*t).is_some()) {
                    Some(t) => {
                        if !texts.contains(&t) {
                            texts.push(t);
                        }
                    }
                    None => out.push(r.clone()),
                }
            }
            if texts.is_empty() {
                continue;
            }
            for r in &extracted.refs {
                if new.text_of_profile(r).is_some_and(|t| texts.contains(&t)) && !out.contains(r) {
                    out.push(r.clone());
                }
            }
            updates.push((j, out));
        }
        for (j, refs) in updates {
            match &mut self.features[j].kind {
                FeatureKind::Extrude { profiles, .. } | FeatureKind::Revolve { profiles, .. } => {
                    *profiles = ProfileSel::Refs(refs);
                }
                _ => continue,
            }
            let version = self.bump();
            self.features[j].version = version;
        }
    }

    /// Die Definition der Skizze `sketch` mit der berechneten Skizze (letzte Lage der
    /// Projektionen) statt der gespeicherten, wie beim Öffnen einer Skizze; ohne Ergebnis die
    /// Definition. Fehler: kein Feature oder keine Skizze.
    fn sketch_with_result(&self, sketch: FeatureId) -> Result<SketchFeature, FeatureError> {
        let f = self.feature(sketch).ok_or(FeatureError::UnknownFeature)?;
        let FeatureKind::Sketch(def) = &f.kind else {
            return Err(FeatureError::InvalidInput(tr!(
                "feature-not-sketch",
                name = f.name.clone()
            )));
        };
        let mut sf = def.clone();
        if let Some(sk) = self.sketch_of(sketch) {
            let name = sf.sketch.name.clone();
            sf.sketch = sk.clone();
            sf.sketch.name = name;
        }
        Ok(sf)
    }

    /// Übernimmt die geänderte Skizze `sf` als Definition von `sketch` (ein Schritt, neu
    /// gerechnet; die Formeln der Felder bleiben).
    fn commit_sketch(&mut self, sketch: FeatureId, sf: SketchFeature) -> Result<(), FeatureError> {
        let kind = FeatureKind::Sketch(sf);
        self.update(sketch, |k| *k = kind)
    }

    /// „Neu verknüpfen“ (Skizzen 11a, Fusion „Relink“): gibt der Kopie mit dem Ziel `target` die
    /// neue Herkunft `source` und rechnet neu. Bei den Gruppenarten aus Skizzen 10 bekommt die
    /// ganze Gruppe (gleiche alte Herkunft) die neue; Stückindex und Stückzahl bleiben, damit
    /// Abhängigkeiten und Maße bleiben, soweit die neue Quelle gleich viele Stücke ergibt. Eine
    /// Herkunft einer anderen Familie ([`CopySource::same_family`]) lehnt es ab
    /// ([`FeatureError::Sketch`]), ebenso ein Ziel ohne Kopie und eine Quelle, der die Kurve
    /// nicht folgen kann (das Ziel bliebe verloren, Ruling T18-2); dann bleibt alles unverändert.
    /// Eine gleiche Herkunft ändert nichts.
    pub fn relink_projection(
        &mut self,
        sketch: FeatureId,
        target: CopyTarget,
        source: CopySource,
    ) -> Result<(), FeatureError> {
        let mut sf = self.sketch_with_result(sketch)?;
        let old = sf
            .copies
            .iter()
            .find(|c| c.target == target)
            .map(|c| c.source.clone())
            .ok_or_else(|| FeatureError::Sketch(tr!("feature-relink-no-copy")))?;
        if !old.same_family(&source) {
            return Err(FeatureError::Sketch(tr!("feature-relink-kind")));
        }
        if old == source {
            return Ok(());
        }
        relink_copies(&mut sf, target, &old, &source);
        // Erst an einer Kopie der Skizze probieren, wie die Neuberechnung (M-8): Bleibt das Ziel
        // verloren, ändert sich am Dokument nichts (auch Zähler und Verlauf nicht). Geprobt
        // werden alle Kopien mit der neuen Herkunft, also auch die übrige Gruppe.
        let relinked: Vec<CopyTarget> = sf
            .copies
            .iter()
            .filter(|c| c.source == source)
            .map(|c| c.target)
            .collect();
        let mut probe = sf.clone();
        let lost = self.follow_draft_copies(sketch, &mut probe, &relinked)?;
        if lost.contains(&target) {
            return Err(FeatureError::Sketch(tr!("lost-relink-failed")));
        }
        self.commit_sketch(sketch, sf)
    }

    /// „Verknüpfung aufheben“ für die Ziele `targets` der Skizze `sketch` (Skizzen 11a, Fusion
    /// „Break Link“, über [`SketchFeature::break_link_target`]): Die Geometrie bleibt an ihrer
    /// letzten Lage und folgt nichts mehr. Alle zusammen sind eine Änderung; liefert die Zahl der
    /// gelösten Kopien (0: nichts geändert).
    pub fn unlink_projections(
        &mut self,
        sketch: FeatureId,
        targets: &[CopyTarget],
    ) -> Result<usize, FeatureError> {
        let mut sf = self.sketch_with_result(sketch)?;
        let n = targets.iter().filter(|t| sf.break_link_target(**t)).count();
        if n > 0 {
            self.commit_sketch(sketch, sf)?;
        }
        Ok(n)
    }

    /// „Geometrie löschen“ für die Ziele `targets` der Skizze `sketch` (Skizzen 11a, Fusion
    /// „Delete“): löscht die Kurven und Punkte samt Kopie-Eintrag; die Enden einer Kurve gehen mit,
    /// wenn keine andere Kurve sie benutzt. Alle zusammen sind eine Änderung; liefert die Zahl der
    /// gelöschten Ziele (nur solche mit Kopie; 0: nichts geändert). Lehnt die Skizze ein Löschen
    /// ab (etwa eine Kurve eines Textes), bleibt alles unverändert.
    pub fn delete_lost_geometry(
        &mut self,
        sketch: FeatureId,
        targets: &[CopyTarget],
    ) -> Result<usize, FeatureError> {
        let mut sf = self.sketch_with_result(sketch)?;
        let n = delete_lost_targets(&mut sf, targets)
            .map_err(|e| FeatureError::Sketch(e.to_string()))?;
        if n > 0 {
            self.commit_sketch(sketch, sf)?;
        }
        Ok(n)
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
            return Err(FeatureError::InvalidInput(tr!("feature-name-empty")));
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
    /// Nur harte Bezüge ([`FeatureKind::dependencies`]); schwache Bezüge zählen nicht: Eine
    /// Skizze, die nur Kanten oder Flächen von `id` projiziert, bleibt beim Löschen stehen, ihr
    /// Bezug ist danach verloren (Warnung, Skizzen 11a A5).
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
        self.sketch_groups.forget_sketches(&removed);
        self.prune_meshes();
        self.prune_step_shapes();
        self.prune_images();
        // Verweise auf gelöschte Features und der Ordner „Leinwände“ ohne Leinwand fallen weg.
        let v = std::mem::take(&mut self.visibility);
        self.visibility = self.pruned_visibility(v);
        let d = std::mem::take(&mut self.sketch_display);
        self.sketch_display = self.pruned_sketch_display(d);
        self.edits += 1;
        self.recompute();
        Ok(removed)
    }

    /// Prüft, ob alle harten Verweise von `kind` auf vorhandene Features vor `idx` zeigen.
    /// Schwache Bezüge ([`FeatureKind::weak_dependencies`]) dürfen fehlen oder dahinter stehen;
    /// dann ist der Bezug verloren (Warnung), kein Fehler.
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
        if let FeatureKind::Canvas(c) = kind
            && !self.images.contains_key(&c.image)
        {
            return Err(crate::canvas::image_missing(c.image));
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
                return Err(format!("feature id {} is duplicated", f.id.0));
            }
            if f.id.0 == 0 || f.id.0 >= self.next_id {
                return Err(format!("feature id {} was never assigned", f.id.0));
            }
        }
        if self.marker > self.features.len() {
            return Err(format!(
                "marker {} past the end ({} features)",
                self.marker,
                self.features.len()
            ));
        }
        self.check_rollback_invariants()?;
        let rolled_back = self.rolled_back();
        for (idx, f) in self.features.iter().enumerate() {
            if let Err(e) = self.check_references(&f.kind, idx) {
                return Err(format!("feature \"{}\": {e}", f.name));
            }
            if idx >= self.marker && f.status != FeatureStatus::NotComputed {
                return Err(format!(
                    "feature \"{}\" is after the marker but has status {:?}",
                    f.name, f.status
                ));
            }
            if rolled_back.contains(&f.id) && f.status != FeatureStatus::NotComputed {
                return Err(format!(
                    "feature \"{}\" is rolled back but has status {:?}",
                    f.name, f.status
                ));
            }
            let active = idx < self.marker && !rolled_back.contains(&f.id);
            if active && !f.suppressed && f.status == FeatureStatus::NotComputed {
                return Err(format!(
                    "feature \"{}\" is before the marker but was not computed",
                    f.name
                ));
            }
            match (&f.status, f.result.is_some()) {
                (FeatureStatus::Ok, false) => {
                    return Err(format!("feature \"{}\" is Ok but has no result", f.name));
                }
                (FeatureStatus::Error(_) | FeatureStatus::Skipped(_), true) => {
                    return Err(format!(
                        "feature \"{}\" has status {:?} and still a result",
                        f.name, f.status
                    ));
                }
                _ => {}
            }
            if active && f.suppressed != (f.status == FeatureStatus::Suppressed) {
                return Err(format!(
                    "feature \"{}\": suppression and status {:?} do not match",
                    f.name, f.status
                ));
            }
        }
        self.check_parameter_invariants()?;
        self.check_formula_invariants()?;
        for v in self.visible_bodies() {
            if let Err(e) = self.kernel.validate(v.solid) {
                return Err(format!("visible body \"{}\" is invalid: {e}", v.name));
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
                return Err(format!("rollback point {} refers to no feature", r.0));
            };
            if last.is_some_and(|l| l >= idx) {
                return Err("rollback points not in list order or duplicated".into());
            }
            last = Some(idx);
            let chain = self.chain(*r);
            if let Some(o) = self.rollbacks.iter().find(|o| *o != r && chain.contains(o)) {
                return Err(format!(
                    "rollback point {} lies in the chain of point {}",
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
            return Err("parameter counter is 0".into());
        }
        for (i, p) in self.parameters.iter().enumerate() {
            if p.id.0 == 0 || p.id.0 >= self.next_param {
                return Err(format!("parameter id {} was never assigned", p.id.0));
            }
            if self.parameters[..i].iter().any(|q| q.id == p.id) {
                return Err(format!("parameter id {} is duplicated", p.id.0));
            }
        }
        let values = crate::params::check_table(&self.parameters)
            .map_err(|m| format!("parameter table invalid: {m}"))?;
        if values.len() != self.param_values.len()
            || values
                .iter()
                .zip(&self.param_values)
                .any(|(a, b)| a.to_bits() != b.to_bits())
        {
            return Err(format!(
                "parameter values stale: {:?} instead of {values:?}",
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
            check_keys(&f.kind, &f.formulas).map_err(|e| format!("feature \"{}\": {e}", f.name))?;
            if let FeatureKind::Sketch(sf) = &f.kind
                && sf.clone().prune_formulas()
            {
                return Err(format!(
                    "feature \"{}\": formula on a constraint that cannot have one",
                    f.name
                ));
            }
            let ev = evaluate(&f.kind, &f.formulas, &scope, tolerance);
            if let Some(k) = field_keys(&f.kind)
                .into_iter()
                .find(|k| !same_fields(&f.kind, &ev.kind, *k))
            {
                return Err(format!(
                    "feature \"{}\": field {k:?} does not hold the value of its formula",
                    f.name
                ));
            }
            if !same_values(&f.kind, &ev.kind) {
                return Err(format!(
                    "feature \"{}\": a sketch dimension does not hold the value of its formula",
                    f.name
                ));
            }
            if ev.issue != f.issue {
                return Err(format!(
                    "feature \"{}\": formula issue {:?} instead of {:?}",
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
                    "feature \"{}\" has a formula issue but status {:?}",
                    f.name, f.status
                ));
            }
        }
        Ok(())
    }
}

/// Setzt die Herkunft `source` an die Kopie mit dem Ziel `target`; bei den Gruppenarten aus
/// Skizzen 10 an alle Kopien mit der alten Herkunft `old` (eine Gruppe). Stückindex und
/// Stückzahl bleiben. Gemeinsamer Weg für Dokument und offene Skizze (App).
pub fn relink_copies(
    sf: &mut SketchFeature,
    target: CopyTarget,
    old: &CopySource,
    source: &CopySource,
) {
    let group = old.is_3d_kind();
    for c in &mut sf.copies {
        if c.target == target || (group && c.source == *old) {
            c.source = source.clone();
        }
    }
}

/// „Geometrie löschen“ an der Skizze `sf` (gemeinsamer Weg für Dokument und offene Skizze der
/// App): je Ziel aus `targets` mit Kopie der Kopie-Eintrag und die projizierte Geometrie. Eine
/// Kurve geht samt ihren Enden, die keine andere lebende Kurve benutzt. Ein Punkt, an dem noch
/// eine Kurve hängt (etwa eine Linie des Nutzers an einer eingezogenen Ecke, A7), bleibt als
/// gewöhnlicher Punkt stehen: Nur seine Fixierung entfällt (wie Fusion, dort fällt nur der Bezug
/// weg). Liefert die Zahl der gelöschten Ziele. Bei einem Fehler kann `sf` halb geändert sein;
/// der Aufrufer arbeitet darum an einer Kopie.
pub fn delete_lost_targets(
    sf: &mut SketchFeature,
    targets: &[CopyTarget],
) -> Result<usize, planora_sketch::SketchError> {
    let mut n = 0;
    for t in targets {
        if !sf.break_link_target(*t) {
            continue;
        }
        delete_target(&mut sf.sketch, *t)?;
        n += 1;
    }
    if n > 0 {
        sf.prune_copies();
    }
    Ok(n)
}

/// Benutzt eine lebende Kurve den Punkt `p`?
fn point_used(sk: &Sketch, p: planora_sketch::model::PointId) -> bool {
    sk.curves().any(|(_, d)| d.kind.points().contains(&p))
}

/// Löscht das Ziel einer Kopie aus der Skizze (siehe [`delete_lost_targets`]).
fn delete_target(sk: &mut Sketch, target: CopyTarget) -> Result<(), planora_sketch::SketchError> {
    match target {
        CopyTarget::Curve(c) => {
            let ends = sk.curve(c).map(|d| d.kind.points()).unwrap_or_default();
            sk.remove_curve(c)?;
            for p in ends {
                if !point_used(sk, p) && sk.point_xyz(p).is_ok() {
                    sk.remove_point(p)?;
                }
            }
            Ok(())
        }
        CopyTarget::Point(p) if point_used(sk, p) => {
            let pins: Vec<_> = sk
                .constraints()
                .filter(|(_, c)| c.kind.fixed_point() == Some(p))
                .map(|(id, _)| id)
                .collect();
            for id in pins {
                sk.remove_constraint(id)?;
            }
            Ok(())
        }
        CopyTarget::Point(p) => sk.remove_point(p),
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
        assert!(e.contains("not computed"), "{e}");
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
        check(&d, "parameter values stale");
        // Tabelle ungültig (Kreis)
        let (mut d, _) = fresh();
        d.parameters[0].formula = "Breite+1".into();
        check(&d, "parameter table invalid");
        // doppelte Kennung
        let (mut d, _) = fresh();
        let mut p = d.parameters[0].clone();
        p.name = "Zweite".into();
        d.parameters.push(p);
        d.param_values.push(40.0);
        check(&d, "duplicated");
        // Feld trägt nicht den Formelwert
        let (mut d, i) = fresh();
        if let FeatureKind::Box { size, .. } = &mut d.features[i].kind {
            size.x = 39.0;
        }
        check(&d, "does not hold the value");
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
        check(&d, "formula issue");
        // Formelfehler ohne Fehlerstatus
        let (mut d, i) = fresh();
        d.features[i]
            .formulas
            .insert(FieldKey::Breite, "Gibtsnicht".to_string());
        d.features[i].issue = Some(FormulaIssue::Broken(
            "Den Namen „Gibtsnicht“ gibt es nicht.".into(),
        ));
        check(&d, "has a formula issue but status");
    }
}
