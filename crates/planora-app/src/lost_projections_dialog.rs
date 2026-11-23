//! „Verlorene Projektionen verwalten“ (Skizzen 11a), wie Fusions „Manage Lost Projections“:
//! eine Zeile je projizierter Geometrie mit verlorenem Bezug, je Zeile „Neu verknüpfen“,
//! „Verknüpfung aufheben“ und „Geometrie löschen“, unten „Alle löschen“, „Alle Verknüpfungen
//! aufheben“ und der Haken „Übrige Geometrie abblenden“.
//!
//! Jede Aktion ist genau ein Rückgängig-Schritt (am Dokument, bei offener Skizze in ihrem
//! Verlauf); Schließen (OK, ✕, Esc) hinterlässt nichts. „Neu verknüpfen“ öffnet die Skizze und
//! wählt die neue Herkunft über die Auswahl von „Projizieren“ (Ruling F9). Die Ansicht zeichnet
//! verlorene Kurven gelb ([`crate::theme::SketchColors::lost`]); mit dem Haken dämpft sie, solange
//! der Dialog offen ist, alles andere ([`marks`]).

use crate::dialog::{Dialog, DialogSize};
use crate::state::AppState;
use crate::surface_project_dialog::{DialogOutcome, keys};
use planora_feature::{CopyTarget, FeatureId, FeatureKind};
use planora_i18n::tr;
use planora_sketch::CheckItem;
use planora_sketch::model::{CurveId, PointId};
use std::collections::HashSet;

/// Der Dialog „Verlorene Projektionen verwalten“ einer Skizze.
#[derive(Clone, Debug, PartialEq)]
pub struct LostProjectionsDialog {
    /// Die Skizze, deren verlorene Projektionen der Dialog zeigt.
    pub sketch: FeatureId,
    /// „Übrige Geometrie abblenden“: Die Ansicht dämpft alles außer den verlorenen Kurven.
    pub dim_rest: bool,
    /// Die Zeile, für die gerade „Neu verknüpfen“ läuft (die Wahl steht in der Sitzung).
    pub relink: Option<CopyTarget>,
}

/// Eine Zeile des Dialogs.
#[derive(Clone, Debug, PartialEq)]
pub struct LostRow {
    pub target: CopyTarget,
    /// „Linie 3“, „Punkt 1“ (Namen wie in „Unterbestimmte finden“).
    pub name: String,
    /// Lässt sich die Herkunft über die Auswahl von „Projizieren“ neu wählen?
    pub relinkable: bool,
}

/// Was ein Knopf des Dialogs auslöst.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LostAction {
    Relink(CopyTarget),
    Unlink(CopyTarget),
    Delete(CopyTarget),
    DeleteAll,
    UnlinkAll,
}

/// Hat die Skizze `sketch` verlorene Projektionen (Menüeintrag in Verlaufsleiste und Baum)?
pub fn offers(state: &AppState, sketch: FeatureId) -> bool {
    state.document.feature(sketch).is_some_and(|f| {
        matches!(f.kind, FeatureKind::Sketch(_)) && !f.lost_projections().is_empty()
    })
}

impl LostProjectionsDialog {
    pub fn new(sketch: FeatureId) -> LostProjectionsDialog {
        LostProjectionsDialog {
            sketch,
            dim_rest: false,
            relink: None,
        }
    }

    /// Die Zeilen ([`AppState::lost_rows`]) mit ihren Namen aus der Skizze, wie sie zu sehen ist
    /// (Entwurf bei offener Skizze, sonst die berechnete).
    pub fn rows(&self, state: &AppState) -> Vec<LostRow> {
        let targets = state.lost_rows(self.sketch);
        let sk = if state.open_sketch_id() == Some(self.sketch) {
            state.session_sketch()
        } else {
            state.document.sketch_of(self.sketch)
        };
        let Some(sk) = sk else {
            return Vec::new();
        };
        let items: Vec<CheckItem> = targets
            .iter()
            .map(|t| match *t {
                CopyTarget::Curve(c) => CheckItem::Curve(c),
                CopyTarget::Point(p) => CheckItem::Point(p),
            })
            .collect();
        let numbers = sk.item_ordinals(&items);
        let sources = match state.document.feature(self.sketch).map(|f| &f.kind) {
            Some(FeatureKind::Sketch(def)) => def.copies.clone(),
            _ => Vec::new(),
        };
        targets
            .into_iter()
            .zip(items.into_iter().zip(numbers))
            .map(|(target, (it, n))| LostRow {
                target,
                name: crate::sketch_under::row_text(sk, it, n),
                relinkable: sources
                    .iter()
                    .find(|c| c.target == target)
                    .is_some_and(|c| crate::state::relink_pickable(&c.source)),
            })
            .collect()
    }

    /// Führt eine Aktion aus (ein Rückgängig-Schritt je Aktion; „Neu verknüpfen“ legt erst der
    /// Klick in der Ansicht an). Fehler stehen in der Meldung.
    pub fn act(&mut self, state: &mut AppState, action: LostAction) {
        match action {
            LostAction::Relink(t) => {
                if state.start_relink(self.sketch, t).is_ok() {
                    self.relink = Some(t);
                }
            }
            LostAction::Unlink(t) => {
                let _ = state.unlink_lost_projections(self.sketch, &[t]);
            }
            LostAction::Delete(t) => {
                let _ = state.delete_lost_geometry(self.sketch, &[t]);
            }
            LostAction::UnlinkAll => {
                let targets = state.lost_rows(self.sketch);
                let _ = state.unlink_lost_projections(self.sketch, &targets);
            }
            LostAction::DeleteAll => {
                let targets = state.lost_rows(self.sketch);
                let _ = state.delete_lost_geometry(self.sketch, &targets);
            }
        }
        // Die Zeile der laufenden Wahl gilt nur, solange die Sitzung sie hält.
        if !relinking(state) {
            self.relink = None;
        }
    }

    /// Abschluss eines Bilds: eine Aktion ausführen; OK, ✕ und Esc schließen (eine laufende Wahl
    /// „Neu verknüpfen“ endet ohne Spur). Ohne Zeilen schließt der Dialog von selbst.
    pub fn finish(
        &mut self,
        state: &mut AppState,
        action: Option<LostAction>,
        ok: bool,
        cancel: bool,
        open: bool,
    ) -> DialogOutcome {
        if let Some(a) = action {
            self.act(state, a);
        }
        if !relinking(state) {
            self.relink = None;
        }
        let closed = if ok {
            Some(DialogOutcome::Applied)
        } else if cancel || !open {
            Some(DialogOutcome::Cancelled)
        } else if state.lost_rows(self.sketch).is_empty() {
            Some(DialogOutcome::Applied)
        } else {
            None
        };
        match closed {
            Some(out) => {
                if let Some(se) = state.sketch_session.as_mut()
                    && se.relink.take().is_some()
                {
                    se.message = None;
                }
                out
            }
            None => DialogOutcome::Open,
        }
    }

    /// Zeigt den Dialog.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> DialogOutcome {
        let rows = self.rows(state);
        let picking = self.relink;
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        let mut action: Option<LostAction> = None;
        let mut dim_rest = self.dim_rest;
        Dialog::new(tr!("lost-dialog-title"), DialogSize::Prompt)
            .dock_key("lost_projections")
            .id(egui::Id::unique("lost_projections_dialog"))
            .open(&mut open)
            .show(ctx, |ui| {
                // Abgedockt läuft der Inhalt evtl. mehrmals je Bild: Die Aktion wird nur gesetzt,
                // nie zurückgesetzt, die Zeilen stehen schon vor dem Fenster fest.
                egui::Grid::new("lost_projections_rows")
                    .num_columns(4)
                    .show(ui, |ui| {
                        for row in &rows {
                            if picking == Some(row.target) {
                                ui.strong(&row.name);
                            } else {
                                ui.label(&row.name);
                            }
                            let relink = ui
                                .add_enabled(row.relinkable, egui::Button::new(tr!("lost-relink")))
                                .on_disabled_hover_text(tr!("lost-relink-unsupported"));
                            if relink.clicked() {
                                action.get_or_insert(LostAction::Relink(row.target));
                            }
                            if ui.button(tr!("lost-break-link")).clicked() {
                                action.get_or_insert(LostAction::Unlink(row.target));
                            }
                            if ui.button(tr!("lost-delete-geometry")).clicked() {
                                action.get_or_insert(LostAction::Delete(row.target));
                            }
                            ui.end_row();
                        }
                    });
                if picking.is_some() {
                    ui.label(tr!("lost-relink-pick"));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(tr!("lost-delete-all")).clicked() {
                        action.get_or_insert(LostAction::DeleteAll);
                    }
                    if ui.button(tr!("lost-break-all")).clicked() {
                        action.get_or_insert(LostAction::UnlinkAll);
                    }
                });
                ui.checkbox(&mut dim_rest, tr!("lost-dim-rest"));
                ui.add_space(6.0);
                if ui.button(tr!("common-ok")).clicked() {
                    ok = true;
                }
                // Esc gehört während der Wahl „Neu verknüpfen“ der Ansicht (sie nimmt die Wahl
                // zurück); sonst schließt es den Dialog.
                let (_, esc) = keys(ui, false);
                cancel |= esc && picking.is_none();
            });
        self.dim_rest = dim_rest;
        self.finish(state, action, ok, cancel, open)
    }
}

/// Läuft in der offenen Skizze die Wahl „Neu verknüpfen“?
fn relinking(state: &AppState) -> bool {
    state
        .sketch_session
        .as_ref()
        .is_some_and(|se| se.relink.is_some())
}

/// Zeigt den offenen Dialog. Er schließt, wenn die Skizze verschwindet.
pub fn show_open(ctx: &egui::Context, state: &mut AppState) -> Option<DialogOutcome> {
    let mut d = state.lost_dialog.take()?;
    if !state
        .document
        .feature(d.sketch)
        .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_)))
    {
        return Some(DialogOutcome::Cancelled);
    }
    let out = d.show(ctx, state);
    if out == DialogOutcome::Open && state.lost_dialog.is_none() {
        state.lost_dialog = Some(d);
    }
    Some(out)
}

/// Markierung verlorener Projektionen in der Ansicht.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LostMarks {
    /// Kurven, die gelb gezeichnet werden.
    pub curves: HashSet<CurveId>,
    /// Punkte, die gelb gezeichnet werden (Ziele und Enden der Kurven).
    pub points: HashSet<PointId>,
    /// „Übrige Geometrie abblenden“ des offenen Dialogs: alles andere gedämpft.
    pub dim_rest: bool,
}

/// Die Markierung der Skizze `sketch` (Ziele aus [`AppState::lost_rows`], Enden der Kurven aus
/// der Skizze, wie sie zu sehen ist) und ob der offene Dialog die übrige Geometrie dämpft.
pub fn marks(state: &AppState, sketch: FeatureId) -> LostMarks {
    let dim_rest = state.lost_dialog.as_ref().is_some_and(|d| d.dim_rest);
    let targets = state.lost_rows(sketch);
    if targets.is_empty() {
        return LostMarks {
            dim_rest,
            ..LostMarks::default()
        };
    }
    let sk = if state.open_sketch_id() == Some(sketch) {
        state.session_sketch()
    } else {
        state.document.sketch_of(sketch)
    };
    let mut out = LostMarks {
        dim_rest,
        ..LostMarks::default()
    };
    for t in targets {
        match t {
            CopyTarget::Curve(c) => {
                out.curves.insert(c);
                if let Some(d) = sk.and_then(|sk| sk.curve(c)) {
                    out.points.extend(d.kind.points());
                }
            }
            CopyTarget::Point(p) => {
                out.points.insert(p);
            }
        }
    }
    out
}
