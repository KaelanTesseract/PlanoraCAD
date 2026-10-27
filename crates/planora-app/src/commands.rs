//! Befehle aus Menü, Befehlsfeld und Tasten: eine gemeinsame Ausführung je Aktion
//! ([`run_action`]; über eine Taste zuerst deren Vorbedingungen, [`run_key`]) und das
//! Gedächtnis für „Letzten Befehl wiederholen“ ([`LastCommand`]).
//!
//! Gemerkt wird ein wiederholbarer Befehl ([`Action::repeatable`]) erst, wenn er abgeschlossen
//! ist: Sein Fenster ist zu und dabei ist ein neues Feature seiner Art entstanden ([`observe`]).
//! Abbrechen, Schließen oder ein anderes Fenster an seiner Stelle ersetzen den zuvor gemerkten
//! Befehl also nicht, ebenso wenig ein anderer Befehl, der nebenher ein Feature anlegt. Die
//! Leertaste startet den gemerkten Befehl wie sein Menüeintrag.
//!
//! Skizzenwerkzeuge haben ein eigenes Gedächtnis (die Leertaste in der Skizze wählt das zuletzt
//! benutzte wieder): gemerkt, sobald das Werkzeug eine Kurve angelegt hat ([`observe`]). Ein
//! gewähltes und mit Esc verlassenes Werkzeug ersetzt so nichts, und das Gedächtnis des Modells
//! (etwa eine Extrusion) bleibt neben dem der Skizze bestehen.

use crate::app::{self, CreateKind, MenuRequest};
use crate::create_dialog::CreateDialog;
use crate::keymap::{Action, Area};
use crate::state::AppState;
use crate::workspace::{DocTab, Workspace};
use planora_feature::{FeatureId, FeatureKind};
use planora_i18n::tr;

/// Hinweis, wenn „Skizze extrudieren“ über die Taste keine passende Skizze findet.
pub fn need_sketch() -> String {
    tr!("cmd-need-sketch")
}
/// Hinweis, wenn „Sichtbarkeit umschalten“ keinen gewählten Körper findet.
pub fn need_body() -> String {
    tr!("cmd-need-body")
}
/// Hinweis, wenn „Löschen“ nichts Gewähltes findet.
pub fn need_selection() -> String {
    tr!("cmd-need-selection")
}

/// Der zuletzt abgeschlossene wiederholbare Befehl und der gerade laufende (Fenster offen),
/// getrennt davon das zuletzt benutzte Skizzenwerkzeug.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LastCommand {
    last: Option<Action>,
    pending: Option<Pending>,
    /// Zuletzt benutztes Skizzenwerkzeug (Leertaste in der Skizze).
    sketch_last: Option<Action>,
    /// Stand der offenen Skizze im vorigen Bild (für [`observe`]).
    sketch_seen: Option<SketchSeen>,
}

/// Was [`observe`] von der offenen Skizze zuletzt gesehen hat: ein Bild später mehr Kurven in
/// derselben Skizze, ohne Rückgängig/Wiederholen dazwischen, heißt „das Werkzeug wurde benutzt“.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SketchSeen {
    tab: u64,
    sketch: FeatureId,
    jumps: u64,
    curves: usize,
}

/// Ein gestarteter Befehl, dessen Fenster noch offen ist.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    action: Action,
    /// Tab, in dem das Fenster offen ist.
    tab: u64,
    /// Die Features beim Öffnen des Fensters: ein neues Feature der Art des Befehls beim
    /// Schließen heißt „ausgeführt“.
    before: Vec<FeatureId>,
}

impl LastCommand {
    /// Merkt sich `a`, wenn sie wiederholbar ist; sonst bleibt der bisherige Befehl. Ein
    /// Skizzenwerkzeug kommt ins eigene Gedächtnis der Skizze und ersetzt den Befehl des Modells
    /// nicht.
    pub fn remember(&mut self, a: Action) {
        if a.sketch_tool().is_some() {
            self.sketch_last = Some(a);
        } else if a.repeatable() {
            self.last = Some(a);
        }
    }

    /// Der gemerkte Befehl des Modells (bleibt gemerkt).
    pub fn last(&self) -> Option<Action> {
        self.last
    }

    /// Das gemerkte Skizzenwerkzeug (bleibt gemerkt, auch über das Ende der Skizze hinaus).
    pub fn sketch_tool(&self) -> Option<Action> {
        self.sketch_last
    }
}

/// Führt `a` aus wie Menüeintrag oder Knopf. „Projekte“, „Tastenkürzel anzeigen“ und
/// „Rückmeldung geben“ öffnen ein Fenster der App und kommen als [`MenuRequest`] zurück. Die
/// Befehle des Modells tun in alten Versionen nichts (dort sind Menü und Befehlsfeld gesperrt);
/// die Übersicht der Tastenkürzel ändert nichts und öffnet auch dort.
///
/// Im ausführlichen Protokoll ([`crate::diag::detail`]) steht danach eine Zeile mit dem festen
/// Namen der Aktion und der Dauer (`Befehl Rückgängig: 3 ms`), nie Projektinhalt.
pub fn run_action(ws: &mut Workspace, a: Action) -> MenuRequest {
    timed(&a.label(), || run_action_inner(ws, a))
}

/// Führt `f` aus und schreibt danach die Detailzeile `Befehl <name>: <n> ms` ins ausführliche
/// Protokoll. Die gemeinsame Stelle für Tasten, Menü und Werkzeugleiste: jede Aktion ruft sie
/// genau einmal auf (Aktionen der Tastenbelegung über [`run_action`], die übrigen Einträge
/// direkt). `name` ist ein fester Aktionsname, nie Projektinhalt.
pub fn timed<R>(name: &str, f: impl FnOnce() -> R) -> R {
    let start = std::time::Instant::now();
    let result = f();
    crate::diag::detail(&format!(
        "Befehl {name}: {}",
        duration_text(start.elapsed())
    ));
    result
}

/// Dauer für das Protokoll: ganze Millisekunden, unter einer Millisekunde „<1 ms“.
pub fn duration_text(d: std::time::Duration) -> String {
    match d.as_millis() {
        0 => "<1 ms".into(),
        n => format!("{n} ms"),
    }
}

fn run_action_inner(ws: &mut Workspace, a: Action) -> MenuRequest {
    if a.area() == Area::Model && a != Action::ShowShortcuts && ws.active_tab().read_only {
        return MenuRequest::None;
    }
    // Das Öffnen der Rückmeldung ist kein Arbeitsschritt (das Menü legt auch keinen ab).
    if a == Action::FeedbackOpen {
        return MenuRequest::Feedback;
    }
    // „Speichern“ wird erst ein Schritt, wenn wirklich gespeichert wurde (`Workspace`); ein
    // abgebrochenes „Speichern unter“ soll in den letzten Schritten nicht stehen.
    if a != Action::Save {
        crate::diag::step(&a.label());
    }
    match a {
        Action::ShowShortcuts | Action::SketchShowShortcuts => return MenuRequest::Shortcuts,
        Action::New => {
            ws.new_tab();
        }
        Action::Projects => return MenuRequest::Projects,
        Action::FeedbackOpen => return MenuRequest::Feedback,
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
            // Ohne Skizze (etwa eine gemerkte Extrusion in einem Tab ohne Skizzen) ein Hinweis;
            // die Sperre durch einen zurückgesetzten Körper meldet `do_create` selbst.
            match app::create_blocked(ws, CreateKind::Extrusion) {
                Some(why) if why != crate::history_bar::LOCK_HINT => {
                    hint(&mut ws.active_tab_mut().state, &why);
                }
                _ => {
                    app::do_create(ws, CreateKind::Extrusion);
                    track(ws, a);
                }
            }
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
            if let Some(last) = ws.last_command.last() {
                return run_action(ws, last);
            }
        }
        Action::SketchLine
        | Action::SketchRectangle
        | Action::SketchCircle
        | Action::SketchProjectEdge => {
            if let Some(tool) = a.sketch_tool()
                && let Some((se, sk)) = ws.active_tab_mut().state.session_sketch_mut()
            {
                se.choose_tool(sk, tool);
            }
        }
        Action::SketchConstruction => {
            if let Some((se, sk)) = ws.active_tab_mut().state.session_sketch_mut() {
                se.toggle_construction_if_possible(sk);
            }
        }
        Action::SketchRepeatLast => {
            if let Some(last) = ws.last_command.sketch_tool() {
                return run_action(ws, last);
            }
        }
        Action::DrawingDimension | Action::DrawingBalloon => {
            let tab = ws.active_tab_mut();
            let editable = !tab.read_only;
            crate::drawing_view::tool_key(&mut tab.state, a == Action::DrawingBalloon, editable);
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
        hint(&mut tab.state, &need_sketch());
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
        None => hint(state, &need_body()),
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
        None => hint(state, &need_selection()),
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

/// Ist `kind` ein Ergebnis des Befehls `a`?
fn made_by(a: Action, kind: &FeatureKind) -> bool {
    match a {
        Action::ExtrudeSketch => matches!(kind, FeatureKind::Extrude { .. }),
        Action::Fillet => matches!(kind, FeatureKind::Verrunden { .. }),
        _ => false,
    }
}

/// Nach dem Start von `a` (Menü, Knopf, Taste, Kontextmenü im Baum): ist sein Fenster jetzt
/// offen, wartet [`observe`] auf dessen Abschluss. Hat der Befehl kein Fenster geöffnet (nur
/// ein Hinweis), läuft ein schon laufender Befehl weiter.
pub(crate) fn track(ws: &mut Workspace, a: Action) {
    let tab = ws.active_tab();
    if window_open(tab, a) {
        ws.last_command.pending = Some(Pending {
            action: a,
            tab: tab.id,
            before: tab.state.document.features().iter().map(|f| f.id).collect(),
        });
    }
}

/// Einmal je Bild nach den Fenstern: Ist das Fenster des laufenden Befehls zu (auch ersetzt
/// durch ein anderes), wird er gemerkt, wenn seither ein Feature seiner Art entstand, sonst
/// vergessen. Ein Fenster
/// in einem anderen Tab wartet, bis der Tab wieder vorn ist; ist der Tab zu, entfällt es.
pub fn observe(ws: &mut Workspace) {
    observe_sketch(ws);
    let Some(p) = ws.last_command.pending.clone() else {
        return;
    };
    let Some(tab) = ws.tabs.iter().find(|t| t.id == p.tab) else {
        ws.last_command.pending = None;
        return;
    };
    if ws.active_tab().id != p.tab || window_open(tab, p.action) {
        return;
    }
    let done = tab
        .state
        .document
        .features()
        .iter()
        .any(|f| !p.before.contains(&f.id) && made_by(p.action, &f.kind));
    ws.last_command.pending = None;
    if done {
        ws.last_command.remember(p.action);
    }
}

/// Abschluss der Skizzenwerkzeuge (sie haben kein Fenster): gemerkt wird ein Werkzeug mit
/// Eintrag in der Tabelle, sobald es in der offenen Skizze eine Kurve angelegt hat, gleich ob
/// es über Taste, Knopf oder Leertaste gewählt wurde. Nur wählen und mit Esc oder einem anderen
/// Werkzeug verlassen merkt nichts (wie ein abgebrochenes Fenster); Rückgängig/Wiederholen in
/// der Skizze zählt nicht als Benutzen. Ein Werkzeug ohne Eintrag in der Tabelle (Bogen,
/// Spline, …) ersetzt das Gedächtnis nicht. Das Gedächtnis des Modells bleibt in jedem Fall.
fn observe_sketch(ws: &mut Workspace) {
    let tab = ws.active_tab();
    let (Some(se), Some(sk)) = (
        tab.state.sketch_session.as_ref(),
        tab.state.session_sketch(),
    ) else {
        ws.last_command.sketch_seen = None;
        return;
    };
    let now = SketchSeen {
        tab: tab.id,
        sketch: se.sketch,
        jumps: se.history_jumps,
        curves: sk.curves().count(),
    };
    let used = ws.last_command.sketch_seen.is_some_and(|before| {
        before.tab == now.tab
            && before.sketch == now.sketch
            && before.jumps == now.jumps
            && now.curves > before.curves
    });
    if used && let Some(a) = Action::for_sketch_tool(se.tool) {
        ws.last_command.remember(a);
    }
    ws.last_command.sketch_seen = Some(now);
}
