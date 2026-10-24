//! Befehle aus Menü, Befehlsfeld und Tasten: eine gemeinsame Ausführung je Aktion
//! ([`run_action`]; über eine Taste zuerst deren Vorbedingungen, [`run_key`]) und das
//! Gedächtnis für „Letzten Befehl wiederholen“ ([`LastCommand`]).
//!
//! Gemerkt wird ein wiederholbarer Befehl ([`Action::repeatable`]) erst, wenn er abgeschlossen
//! ist: Sein Fenster ist zu und hat dabei ein Feature angelegt ([`observe`]). Abbrechen,
//! Schließen oder ein anderes Fenster an seiner Stelle ersetzen den zuvor gemerkten Befehl also
//! nicht. Die Leertaste startet den gemerkten Befehl wie sein Menüeintrag.

use crate::app::{self, CreateKind, MenuRequest};
use crate::create_dialog::CreateDialog;
use crate::keymap::{Action, Area};
use crate::state::AppState;
use crate::workspace::{DocTab, Workspace};

/// Hinweis, wenn „Skizze extrudieren“ über die Taste keine passende Skizze findet.
pub const NEED_SKETCH: &str = "Zuerst eine Skizze mit geschlossenem Profil wählen.";
/// Hinweis, wenn „Sichtbarkeit umschalten“ keinen gewählten Körper findet.
pub const NEED_BODY: &str = "Zuerst einen Körper wählen.";
/// Hinweis, wenn „Löschen“ nichts Gewähltes findet.
pub const NEED_SELECTION: &str = "Zuerst einen Körper oder Schritt wählen.";

/// Der zuletzt abgeschlossene wiederholbare Befehl und der gerade laufende (Fenster offen).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LastCommand {
    last: Option<Action>,
    pending: Option<Pending>,
}

/// Ein gestarteter Befehl, dessen Fenster noch offen ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pending {
    action: Action,
    /// Tab, in dem das Fenster offen ist.
    tab: u64,
    /// Anzahl der Features beim Start: mehr beim Schließen heißt „ausgeführt“.
    features: usize,
}

impl LastCommand {
    /// Merkt sich `a`, wenn sie wiederholbar ist; sonst bleibt der bisherige Befehl.
    pub fn remember(&mut self, a: Action) {
        if a.repeatable() {
            self.last = Some(a);
        }
    }

    /// Der gemerkte Befehl (bleibt gemerkt).
    pub fn take(&self) -> Option<Action> {
        self.last
    }
}

/// Führt `a` aus wie Menüeintrag oder Knopf. „Projekte“ öffnet ein Fenster der App und kommt
/// als [`MenuRequest`] zurück. Die Befehle des Modells tun in alten Versionen nichts (dort sind
/// Menü und Befehlsfeld gesperrt).
pub fn run_action(ws: &mut Workspace, a: Action) -> MenuRequest {
    if a.area() == Area::Model && ws.active_tab().read_only {
        return MenuRequest::None;
    }
    match a {
        Action::New => {
            ws.new_tab();
        }
        Action::Projects => return MenuRequest::Projects,
        Action::Save => app::begin_save(ws, false),
        Action::Close => {
            let i = ws.active;
            ws.request_close(i);
        }
        Action::Export => app::open_export(ws),
        Action::Undo => app::undo_active(ws),
        Action::Redo => app::redo_active(ws),
        // Die Ansicht hört auf Mausgesten (`keymap::mouse`).
        Action::ViewPan | Action::ViewOrbit | Action::ViewZoom => {}
        Action::ExtrudeSketch => {
            app::do_create(ws, CreateKind::Extrusion);
            track(ws, a);
        }
        Action::Fillet => {
            let state = &mut ws.active_tab_mut().state;
            // Eine Sperre durch einen zurückgesetzten Körper meldet `open_blend` selbst.
            match state.blend_blocked() {
                Some(why) if state.chosen_lock().is_none() => hint(state, why),
                _ => {
                    app::open_blend(ws, false);
                    track(ws, a);
                }
            }
        }
        Action::ToggleVisibility => toggle_visibility(&mut ws.active_tab_mut().state),
        Action::DeleteSelection => delete_selection(&mut ws.active_tab_mut().state),
        Action::RepeatLast => {
            if let Some(last) = ws.last_command.take() {
                return run_action(ws, last);
            }
        }
    }
    MenuRequest::None
}

/// Wie [`run_action`], über eine Taste: „Skizze extrudieren“ braucht dann eine gewählte Skizze
/// mit geschlossenem Profil (sonst ein Hinweis in der Statuszeile und nichts weiter); das Menü
/// nimmt ohne Wahl die geöffnete oder letzte Skizze.
pub fn run_key(ws: &mut Workspace, a: Action) -> MenuRequest {
    let tab = ws.active_tab_mut();
    if a == Action::ExtrudeSketch && !tab.read_only && !selected_sketch_has_profile(&tab.state) {
        hint(&mut tab.state, NEED_SKETCH);
        return MenuRequest::None;
    }
    run_action(ws, a)
}

/// Ist eine Skizze gewählt, die ein geschlossenes Profil hat?
fn selected_sketch_has_profile(state: &AppState) -> bool {
    state
        .selected_feature
        .and_then(|id| state.document.sketch_of(id))
        .is_some_and(|sk| !state.profile_cache.get(sk).profiles.is_empty())
}

/// Hinweis in der Statuszeile (normale Farbe), ohne etwas zu ändern.
fn hint(state: &mut AppState, text: &str) {
    state.message = None;
    state.info = Some(text.into());
}

/// Das Auge des gewählten Körpers ([`AppState::chosen_body`]), wie im Feature-Baum.
fn toggle_visibility(state: &mut AppState) {
    match state.chosen_body() {
        Some(j) => {
            let hidden = state.is_body_hidden(j);
            state.set_body_hidden(j, !hidden);
        }
        None => hint(state, NEED_BODY),
    }
}

/// „Löschen“ wie im Kontextmenü ([`crate::browser::request_delete`]): der gewählte Schritt
/// (Baum, Verlaufsleiste), sonst der gewählte Körper (sein Ursprung, wie die Körperzeile im
/// Baum). Zurückgesetzte Körper sind nicht gesperrt.
fn delete_selection(state: &mut AppState) {
    let target = state
        .selected_feature
        .filter(|id| state.document.feature(*id).is_some())
        .or_else(|| {
            let b = state.bodies.get(state.chosen_body()?)?;
            Some(state.document.naming_origin(b.source).feature)
        });
    match target {
        Some(id) => crate::browser::request_delete(state, id),
        None => hint(state, NEED_SELECTION),
    }
}

/// Ist das Fenster des Befehls `a` im Tab offen?
fn window_open(tab: &DocTab, a: Action) -> bool {
    match a {
        Action::ExtrudeSketch => matches!(tab.dialog, Some(CreateDialog::ExtrudeSketch { .. })),
        Action::Fillet => tab.blend_dialog.is_some(),
        _ => false,
    }
}

/// Nach dem Start von `a` (Menü, Knopf, Taste, Kontextmenü im Baum): ist sein Fenster jetzt
/// offen, wartet [`observe`] auf dessen Abschluss.
pub(crate) fn track(ws: &mut Workspace, a: Action) {
    let tab = ws.active_tab();
    ws.last_command.pending = window_open(tab, a).then(|| Pending {
        action: a,
        tab: tab.id,
        features: tab.state.document.features().len(),
    });
}

/// Einmal je Bild nach den Fenstern: Ist das Fenster des laufenden Befehls zu (auch ersetzt
/// durch ein anderes), wird er gemerkt, wenn dabei ein Feature entstand, sonst vergessen. Ein
/// Fenster in einem anderen Tab wartet, bis der Tab wieder vorn ist; ist der Tab zu, entfällt es.
pub fn observe(ws: &mut Workspace) {
    let Some(p) = ws.last_command.pending else {
        return;
    };
    let Some(tab) = ws.tabs.iter().find(|t| t.id == p.tab) else {
        ws.last_command.pending = None;
        return;
    };
    if ws.active_tab().id != p.tab || window_open(tab, p.action) {
        return;
    }
    let done = tab.state.document.features().len() > p.features;
    ws.last_command.pending = None;
    if done {
        ws.last_command.remember(p.action);
    }
}
