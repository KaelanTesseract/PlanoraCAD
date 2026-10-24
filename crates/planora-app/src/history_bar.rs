//! Verlaufsleiste: die Schritte des gewählten Körpers mit einem Marker je Körper. Hier steht
//! das Modell ohne Oberfläche (Schritte, Markerstelle, Bewegen, Farbe, Sperre); es ist ohne
//! Fenster testbar. Der globale Marker des Feature-Baums (`browser::roll_back_to`) bleibt
//! daneben unverändert.
//!
//! Der Marker eines Körpers steht hinter dem letzten aktiven Schritt von vorn: Position `p`
//! heißt, die ersten `p` Schritte der Kette sind aktiv, der Schritt an `p` trägt die
//! Rückrollstelle. `p == Anzahl der Schritte` heißt, der Körper hat keine Stelle.

use crate::icons::FeatureIcon;
use crate::state::AppState;
pub use crate::theme::BodyColor;
use planora_feature::{FeatureId, FeatureStatus};

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

/// Die Schritte der Kette von `tip` in Listenordnung, Skizzen eingeschlossen; für einen
/// unbekannten Körper keine.
pub fn steps(state: &AppState, tip: FeatureId) -> Vec<StepView> {
    let doc = &state.document;
    // Andere Körper: alle Spitzen außer `tip` und denen, in denen `tip` selbst aufgeht.
    let others: Vec<Vec<FeatureId>> = doc
        .body_tips()
        .into_iter()
        .filter(|t| *t != tip)
        .map(|t| doc.chain(t))
        .filter(|c| !c.contains(&tip))
        .collect();
    doc.chain(tip)
        .into_iter()
        .filter_map(|id| doc.feature(id))
        .map(|f| {
            let state = if !doc.is_active(f.id) {
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
                shared: others.iter().any(|c| c.contains(&f.id)),
            }
        })
        .collect()
}

/// Anzahl der aktiven Schritte von vorn: Index des ersten inaktiven Schritts der Kette (vor dem
/// globalen Marker und außerhalb jeder Rückrollstelle, siehe `Document::is_active`), sonst die
/// Länge. Liegen in der Kette eines Vereinigens zwei Stellen, zählt die vordere.
pub fn marker_position(state: &AppState, tip: FeatureId) -> usize {
    let chain = state.document.chain(tip);
    chain
        .iter()
        .position(|id| !state.document.is_active(*id))
        .unwrap_or(chain.len())
}

/// Setzt den Marker von `tip` auf `position`: `position == Anzahl der Schritte` hebt die
/// Rückrollstellen der Kette auf, sonst kommt die Stelle auf den Schritt an `position`. Eine
/// offene Skizze wird vorher beendet (wie bei `browser::roll_back_to`); betrifft die Stelle die
/// geöffnete Skizze selbst, bleibt alles unverändert. Danach `refresh`.
///
/// Die neue Stelle ersetzt nur vergleichbare Stellen (`Document::set_rollback`). Bliebe dabei
/// eine Stelle aus einer anderen Eingangskette stehen, die einen Schritt vor `position`
/// zurücksetzt (Kette eines Vereinigens), werden die Stellen der Kette erst aufgehoben: Der
/// Marker steht danach genau an `position`, sofern der globale Marker nicht davor liegt.
pub fn move_marker(state: &mut AppState, tip: FeatureId, position: usize) -> Result<(), String> {
    state.message = None;
    if state.document.feature(tip).is_none() {
        return state.fail("Den Körper gibt es nicht mehr".into());
    }
    let chain = state.document.chain(tip);
    if position > chain.len() {
        return state.fail("Diese Stelle gibt es im Verlauf nicht".into());
    }
    let target = chain.get(position).copied();
    state.end_sketch_for(target)?;
    let result = match target {
        None => state.document.clear_rollbacks(tip),
        Some(step) => state.document.set_rollback(tip, step).and_then(|()| {
            if rolled_back_before(state, &chain[..position]) {
                state.document.clear_rollbacks(tip)?;
                state.document.set_rollback(tip, step)?;
            }
            Ok(())
        }),
    };
    if let Err(e) = result {
        state.refresh();
        return state.fail(e.to_string());
    }
    state.refresh();
    Ok(())
}

/// Ist einer der Schritte von einer Rückrollstelle erfasst (inaktiv, obwohl vor dem globalen
/// Marker)?
fn rolled_back_before(state: &AppState, steps: &[FeatureId]) -> bool {
    let doc = &state.document;
    steps
        .iter()
        .any(|id| !doc.is_active(*id) && doc.index_of(*id).is_some_and(|i| i < doc.marker()))
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
/// Kennung der Kette (dem ersten Schritt) modulo Länge. Ändert sich nicht, wenn andere Körper
/// gelöscht werden; ein unbekannter Körper bekommt die erste Farbe.
pub fn body_color(state: &AppState, tip: FeatureId) -> BodyColor {
    let colors = &crate::theme::BODY_COLORS;
    let first = state
        .document
        .chain(tip)
        .into_iter()
        .min()
        .map_or(0, |id| id.0);
    colors[(first % colors.len() as u64) as usize]
}

/// Ist der Körper zurückgesetzt (eine Rückrollstelle in seiner Kette)? Dann sind neue Aktionen
/// an ihm gesperrt ([`LOCK_HINT`]).
pub fn is_locked(state: &AppState, tip: FeatureId) -> bool {
    state.document.rollback_in_chain(tip).is_some()
}
