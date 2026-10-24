//! Modell der Verlaufsleiste ohne Oberfläche: Schritte, Markerstelle, Bewegen, Farbe, Sperre.

use crate::icons::FeatureIcon;
use crate::state::AppState;
pub use crate::theme::BodyColor;
use planora_feature::{Document, FeatureId, FeatureKind, FeatureStatus};
use std::collections::{HashMap, HashSet};

/// Hinweis, wenn eine neue Aktion an einem zurückgesetzten Körper gesperrt ist.
pub const LOCK_HINT: &str =
    "Dieser Körper ist zurückgesetzt. Springe zuerst zum Ende des Verlaufs.";

/// Zustand eines Schritts in der Leiste.
#[derive(Clone, Debug, PartialEq)]
pub enum StepState {
    /// Wird berechnet.
    Active,
    /// Hinter dem Marker (Rückrollstelle oder globaler Marker), unterdrückt oder übersprungen.
    Inactive,
    /// Berechnung fehlgeschlagen; der Text nennt den Grund.
    Error(String),
}

/// Ein Schritt der Leiste (aus dem Dokument kopiert).
#[derive(Clone, Debug, PartialEq)]
pub struct StepView {
    pub id: FeatureId,
    pub name: String,
    pub icon: FeatureIcon,
    pub state: StepState,
    /// Der Schritt liegt auch in der Kette eines anderen Körpers (etwa eine zweimal extrudierte
    /// Skizze).
    pub shared: bool,
}

/// Meldung, wenn der Marker eines Körpers hinter Schritte soll, die nur der globale Marker
/// zurücksetzt.
pub const GLOBAL_HINT: &str =
    "Der ganze Verlauf ist zurückgesetzt. Aktiviere ihn zuerst mit „Alles aktivieren“.";

/// Aktivität aller Features, einmal je Aufruf aus dem Dokument berechnet (wie
/// `Document::is_active`, ohne den Satz für jeden Schritt neu zu bauen).
struct Activity {
    index: HashMap<FeatureId, usize>,
    rolled: HashSet<FeatureId>,
    marker: usize,
}

impl Activity {
    fn of(doc: &Document) -> Activity {
        Activity {
            index: doc
                .features()
                .iter()
                .enumerate()
                .map(|(i, f)| (f.id, i))
                .collect(),
            rolled: rolled_back(doc, doc.rollbacks()),
            marker: doc.marker(),
        }
    }

    /// Vor dem globalen Marker und von keiner Rückrollstelle erfasst.
    fn is_active(&self, id: FeatureId) -> bool {
        self.index.get(&id).is_some_and(|&i| i < self.marker) && !self.rolled.contains(&id)
    }

    /// Steht hinter dem globalen Marker.
    fn behind_global(&self, id: FeatureId) -> bool {
        self.index.get(&id).is_some_and(|&i| i >= self.marker)
    }
}

/// Die Stellen `stellen` und alles, was von ihnen abhängt (Verweise zeigen nur nach vorn, ein
/// Durchlauf in Listenordnung reicht).
fn rolled_back(doc: &Document, stellen: &[FeatureId]) -> HashSet<FeatureId> {
    let mut found: HashSet<FeatureId> = stellen.iter().copied().collect();
    if found.is_empty() {
        return found;
    }
    for f in doc.features() {
        if !found.contains(&f.id) && f.kind.dependencies().iter().any(|d| found.contains(d)) {
            found.insert(f.id);
        }
    }
    found
}

/// Die Schritte der Kette von `tip` in Listenordnung, Skizzen eingeschlossen; für einen
/// unbekannten Körper keine.
pub fn steps(state: &AppState, tip: FeatureId) -> Vec<StepView> {
    let doc = &state.document;
    let activity = Activity::of(doc);
    // Andere Körper: alle Spitzen außer `tip` und denen, in denen `tip` selbst aufgeht.
    let others: HashSet<FeatureId> = doc
        .body_tips()
        .into_iter()
        .filter(|t| *t != tip)
        .map(|t| doc.chain(t))
        .filter(|c| !c.contains(&tip))
        .flatten()
        .collect();
    doc.chain(tip)
        .into_iter()
        .filter_map(|id| doc.feature(id))
        .map(|f| {
            let state = if !activity.is_active(f.id) {
                StepState::Inactive
            } else {
                match &f.status {
                    FeatureStatus::Ok => StepState::Active,
                    FeatureStatus::Error(e) => StepState::Error(e.clone()),
                    FeatureStatus::Skipped(_)
                    | FeatureStatus::Suppressed
                    | FeatureStatus::NotComputed => StepState::Inactive,
                }
            };
            StepView {
                id: f.id,
                name: f.name.clone(),
                icon: FeatureIcon::of(&f.kind),
                state,
                shared: others.contains(&f.id),
            }
        })
        .collect()
}

/// Anzahl der aktiven Schritte von vorn: Index des ersten inaktiven Schritts der Kette (vor dem
/// globalen Marker und außerhalb jeder Rückrollstelle, siehe `Document::is_active`), sonst die
/// Länge. Liegen in der Kette eines Vereinigens zwei Stellen, zählt die vordere.
pub fn marker_position(state: &AppState, tip: FeatureId) -> usize {
    let activity = Activity::of(&state.document);
    let chain = state.document.chain(tip);
    chain
        .iter()
        .position(|id| !activity.is_active(*id))
        .unwrap_or(chain.len())
}

/// Setzt den Marker von `tip` auf `position`: `position == Anzahl der Schritte` hebt die
/// Rückrollstellen der Kette auf, sonst kommt die Stelle auf den Schritt an `position`. Eine
/// offene Skizze wird vorher beendet (wie bei `browser::roll_back_to`); betrifft die Stelle die
/// geöffnete Skizze selbst, bleibt alles unverändert. Danach einmal `refresh`.
///
/// **Lineare Leiste:** Die Leiste eines Körpers ist eine Reihe über seine ganze Kette in
/// Listenordnung, auch nach einem Vereinigen (die Eingangskörper sind dann verbraucht und haben
/// keine eigene Leiste mehr). Danach sind die ersten `position` Schritte aktiv: Die neue Stelle
/// ersetzt die vergleichbaren Stellen (`Document::set_rollback`); eine Stelle aus einer anderen
/// Eingangskette bleibt nur, wenn sie keinen Schritt vor `position` zurücksetzt, sonst entfällt
/// sie. Die Liste der entfallenden Stellen wird vorher einmal bestimmt. Die Schnittstelle des
/// Dokuments kennt kein Setzen einer ganzen Liste: Jede entfallende Stelle braucht einen
/// eigenen Aufruf (Neuberechnung aus dem Zwischenspeicher); im üblichen Fall ist es einer.
///
/// Abgelehnt (Dokument unverändert, Meldung [`GLOBAL_HINT`]) wird ein Ziel hinter Schritten,
/// die nur der globale Marker zurücksetzt: Eine Stelle dort hätte keine sichtbare Wirkung,
/// würde den Körper aber sperren.
pub fn move_marker(state: &mut AppState, tip: FeatureId, position: usize) -> Result<(), String> {
    state.message = None;
    if state.document.feature(tip).is_none() {
        return state.fail("Den Körper gibt es nicht mehr".into());
    }
    let chain = state.document.chain(tip);
    if position > chain.len() {
        return state.fail("Diese Stelle gibt es im Verlauf nicht".into());
    }
    let activity = Activity::of(&state.document);
    if chain[..position]
        .iter()
        .any(|id| activity.behind_global(*id))
    {
        return state.fail(GLOBAL_HINT.into());
    }
    let target = chain.get(position).copied();
    state.end_sketch_for(target)?;
    let result = match target {
        None => state.document.clear_rollbacks(tip),
        Some(step) => {
            let dropped = dropped_rollbacks(&state.document, &chain, position, step);
            dropped
                .into_iter()
                .try_for_each(|r| state.document.clear_rollbacks(r))
                .and_then(|()| state.document.set_rollback(tip, step))
        }
    };
    state.refresh();
    if let Err(e) = result {
        return state.fail(e.to_string());
    }
    Ok(())
}

/// Stellen, die neben der neuen Stelle `step` (an `position` der Kette) entfallen müssen,
/// damit die ersten `position` Schritte aktiv sind: Stellen der Kette, die `set_rollback` nicht
/// als vergleichbar ersetzt und die einen Schritt vor `position` zurücksetzen. Keine liegt in
/// der Kette einer anderen Stelle, `clear_rollbacks(r)` entfernt also genau `r`.
fn dropped_rollbacks(
    doc: &Document,
    chain: &[FeatureId],
    position: usize,
    step: FeatureId,
) -> Vec<FeatureId> {
    let before: HashSet<FeatureId> = chain[..position].iter().copied().collect();
    if before.is_empty() {
        return Vec::new();
    }
    let comparable: HashSet<FeatureId> = doc
        .chain(step)
        .into_iter()
        .chain(doc.dependents(step))
        .collect();
    doc.rollbacks()
        .iter()
        .copied()
        .filter(|r| *r != step && !comparable.contains(r) && chain.contains(r))
        .filter(|r| rolled_back(doc, &[*r]).iter().any(|id| before.contains(id)))
        .collect()
}

/// Ein Schritt vor (`forward`) oder zurück; am Anfang bzw. Ende bleibt der Marker stehen.
pub fn step(state: &mut AppState, tip: FeatureId, forward: bool) -> Result<(), String> {
    let len = state.document.chain(tip).len();
    let pos = marker_position(state, tip);
    let next = if forward {
        (pos + 1).min(len)
    } else {
        pos.saturating_sub(1)
    };
    if next == pos && state.document.feature(tip).is_some() {
        return Ok(());
    }
    move_marker(state, tip, next)
}

/// Marker vor den ersten Schritt: der Körper ist ganz zurückgesetzt.
pub fn to_start(state: &mut AppState, tip: FeatureId) -> Result<(), String> {
    move_marker(state, tip, 0)
}

/// Marker ans Ende: die Rückrollstellen der Kette entfallen.
pub fn to_end(state: &mut AppState, tip: FeatureId) -> Result<(), String> {
    let len = state.document.chain(tip).len();
    move_marker(state, tip, len)
}

/// Farbe des Körpers: aus der Reihe [`crate::theme::BODY_COLORS`], Index aus der kleinsten
/// Kennung der Körperschritte der Kette (Skizzen zählen nicht, sonst hätten zwei Körper aus
/// einer Skizze dieselbe Farbe) modulo Länge. Ändert sich nicht, wenn andere Körper gelöscht
/// werden; ein unbekannter Körper bekommt die erste Farbe.
pub fn body_color(state: &AppState, tip: FeatureId) -> BodyColor {
    let colors = &crate::theme::BODY_COLORS;
    let doc = &state.document;
    let first = doc
        .chain(tip)
        .into_iter()
        .filter(|id| {
            doc.feature(*id)
                .is_some_and(|f| !matches!(f.kind, FeatureKind::Sketch(_)))
        })
        .min()
        .map_or(0, |id| id.0);
    colors[(first % colors.len() as u64) as usize]
}

/// Ist der Körper zurückgesetzt (eine Rückrollstelle in seiner Kette)? Dann sind neue Aktionen
/// an ihm gesperrt ([`LOCK_HINT`]).
pub fn is_locked(state: &AppState, tip: FeatureId) -> bool {
    state.document.rollback_in_chain(tip).is_some()
}

/// Anzahl der Schritte von vorn, die der globale Marker nicht zurücksetzt: Index des ersten
/// Schritts der Kette hinter dem globalen Marker, sonst die Länge. Weiter als bis hierher kann der
/// Marker des Körpers nicht (dafür gibt es „Alles aktivieren“).
pub(crate) fn reachable_end(state: &AppState, tip: FeatureId) -> usize {
    let activity = Activity::of(&state.document);
    let chain = state.document.chain(tip);
    chain
        .iter()
        .position(|id| activity.behind_global(*id))
        .unwrap_or(chain.len())
}

/// Marker von `tip` auf `position` für die Leiste: höchstens bis [`reachable_end`]. Steht der
/// globale Marker vor Schritten des Körpers, hebt das Ziel `reachable_end` die Stelle des Körpers
/// auf, statt eine Stelle auf einen ohnehin zurückgesetzten Schritt zu setzen (die den Körper
/// ohne sichtbare Wirkung sperren würde). Sonst wie [`move_marker`].
pub(crate) fn go_to(state: &mut AppState, tip: FeatureId, position: usize) -> Result<(), String> {
    let len = state.document.chain(tip).len();
    let reach = reachable_end(state, tip);
    let target = position.min(reach);
    if target == reach && reach < len {
        if state.document.rollback_in_chain(tip).is_none() {
            return Ok(());
        }
        state.message = None;
        state.end_sketch_for(None)?;
        let result = state.document.clear_rollbacks(tip);
        state.refresh();
        return match result {
            Ok(()) => Ok(()),
            Err(e) => state.fail(e.to_string()),
        };
    }
    if target == marker_position(state, tip) && state.document.feature(tip).is_some() {
        return Ok(());
    }
    move_marker(state, tip, target)
}
