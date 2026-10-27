//! Abschnitt „Tastenkürzel“ im Fenster Einstellungen: Tabelle aller Aktionen (Aktion, Bereich,
//! Belegungen) mit Suche, je Belegung „Ändern“ und „Entfernen“, je Aktion „Hinzufügen“ und
//! „Zurücksetzen“, oben „Alles zurücksetzen“ und die Hinweise auf beim Laden Übergangenes.
//!
//! **Erfassen.** Nach „Ändern“ oder „Hinzufügen“ steht in der Zeile „Taste drücken …“; die nächste
//! Taste mit den gerade gehaltenen Umschalttasten (Strg, Umschalt, Alt) wird die Belegung. Esc
//! bricht ab und lässt sich deshalb selbst nicht belegen; eine Umschalttaste allein tut nichts.
//! Bei den Mausaktionen wird stattdessen eine Geste erfasst: „Ansicht schwenken“ und „Ansicht
//! drehen“ mit gehaltener Maustaste und mindestens [`DRAG_MIN`] Pixel Ziehen, „Ansicht zoomen“
//! mit dem Rad, jeweils mit den gehaltenen Umschalttasten. Ein bloßer Klick zählt nicht (so
//! bleibt „Abbrechen“ anklickbar). Nur die Gesten, die die Ansichten auch auswerten, werden
//! angenommen (Ziehen schwenkt und dreht, das Rad zoomt).
//!
//! **Sperre.** Solange erfasst wird, meldet [`is_capturing`] das über die Daten des egui-Kontexts;
//! [`crate::viewport::typing`] (die Sperre des Verteilers und der Tasten der Ansichten) und die
//! Mausgesten ([`super::mouse`]) halten dann still. Die Tastenereignisse werden außerdem
//! verbraucht, bevor das Fenster gezeichnet wird ([`capture_input`]), damit kein Knopf und kein
//! Feld sie sieht. Nach einer erfassten Ziehgeste bleibt die Sperre, bis die Maustaste los ist;
//! nach einer erfassten Taste (auch Esc und Doppelbelegung), bis die Taste los ist, und ihre
//! Wiederholungen werden verbraucht: sonst löste das Halten die neue Belegung gleich aus. Ein
//! Klick mit der linken Maustaste außerhalb der Knöpfe der erfassenden Zeile (Suchfeld, andere
//! Zeile, Projektverzeichnis …) beendet das Erfassen.
//!
//! **Doppelbelegung.** Gehört die erfasste Belegung schon einer anderen Aktion, deren Bereich
//! sich mit dem eigenen stört ([`Keymap::conflict_for`], auch „Überall“), erscheint unter der
//! Zeile „<Taste> ist schon für „<Aktion>“ belegt.“ mit „Tauschen“ ([`Keymap::swap`]: die andere
//! Aktion verliert sie) und „Abbrechen“.
//!
//! Jede Änderung gilt sofort und wird mit [`Workspace::set_keymap`] gespeichert; misslingt das,
//! gilt sie für die Sitzung und der Hinweis [`KEYS_NOT_SAVED`] erscheint (wie bei der
//! Durchsichtigkeit).

use super::{Action, Binding, Gesture, KeyChord, Keymap, MouseButton, search, text};
use crate::settings_window::RootMessage;
use crate::theme;
use crate::workspace::Workspace;

pub const SECTION: &str = "Tastenkürzel";
pub const SEARCH_HINT: &str = "Suchen: Aktion, Bereich oder Taste";
pub const HELP: &str = "„Ändern“ wählen und die neue Taste drücken. Esc bricht ab.";
pub const COLUMN_ACTION: &str = "Aktion";
pub const COLUMN_AREA: &str = "Bereich";
pub const COLUMN_BINDINGS: &str = "Belegung";
pub const CHANGE: &str = "Ändern";
pub const REMOVE: &str = "Entfernen";
pub const ADD: &str = "Hinzufügen";
pub const RESET: &str = "Zurücksetzen";
pub const RESET_ALL: &str = "Alles zurücksetzen";
/// Steht in der Zeile, solange eine Taste erfasst wird.
pub const PRESS_KEY: &str = "Taste drücken …";
/// Wie [`PRESS_KEY`] für „Ansicht schwenken“ und „Ansicht drehen“.
pub const DRAG_NOW: &str = "Maustaste halten und ziehen …";
/// Wie [`PRESS_KEY`] für „Ansicht zoomen“.
pub const WHEEL_NOW: &str = "Mausrad drehen …";
pub const CANCEL: &str = "Abbrechen";
pub const SWAP: &str = "Tauschen";
/// Statt einer Belegung, wenn die Aktion keine hat.
pub const UNBOUND: &str = "ohne Belegung";
pub const NOTHING_FOUND: &str = "Keine Aktion gefunden.";
pub const NOTES_HEADING: &str = "Beim Laden übergangen:";
/// Warnung, wenn die Belegung gilt, aber nicht gespeichert werden konnte.
pub const KEYS_NOT_SAVED: &str = "Die Tastenkürzel gelten nur bis zum Programmende: die Einstellung konnte nicht gespeichert werden.";
/// Mindestweg einer Ziehgeste beim Erfassen (Pixel); darunter ist es ein Klick.
pub const DRAG_MIN: f32 = 4.0;
/// Kleinste Höhe der Tabelle (in kleinen Fenstern rollt dann das ganze Einstellungsfenster).
pub const TABLE_MIN_H: f32 = 120.0;
/// Größte Höhe der Tabelle; darüber rollt sie im Fenster.
pub const TABLE_MAX_H: f32 = 800.0;
/// Was neben der Tabelle Platz braucht: die übrigen Abschnitte des Einstellungsfensters mit Kopf
/// und Fußzeile (rund 580 Pixel, „Darstellung“ und „Updates“ nebeneinander) und je rund 100
/// Pixel frei darüber und darunter. Das Fenster steht mittig, lässt so die Menüleiste (rund 50
/// Pixel) frei, auch mit zusätzlichen Hinweiszeilen.
pub const TABLE_RESERVED_H: f32 = 780.0;
/// Zusätzlich belegt, wenn „Darstellung“ und „Updates“ untereinander stehen (Programmfenster
/// schmaler als [`crate::settings_window::TWO_COLUMNS_FROM`]).
pub const ONE_COLUMN_EXTRA_H: f32 = 110.0;

/// Höhe der Tabelle im Fenster der Höhe `content_h`: was neben den übrigen Abschnitten frei ist
/// ([`TABLE_RESERVED_H`]), zwischen [`TABLE_MIN_H`] und [`TABLE_MAX_H`]. Bei 1920 × 1080 sind
/// das 300 Pixel; größere Fenster zeigen mehr Zeilen.
pub fn table_max_height(content_h: f32) -> f32 {
    (content_h - TABLE_RESERVED_H).clamp(TABLE_MIN_H, TABLE_MAX_H)
}

/// Wie [`table_max_height`] für ein Programmfenster `content`: stehen „Darstellung“ und
/// „Updates“ untereinander, bleibt [`ONE_COLUMN_EXTRA_H`] weniger.
fn table_height_in(content: egui::Rect) -> f32 {
    let h = if content.width() >= crate::settings_window::TWO_COLUMNS_FROM {
        content.height()
    } else {
        content.height() - ONE_COLUMN_EXTRA_H
    };
    table_max_height(h)
}

const AREA_W: f32 = 80.0;
const RESET_W: f32 = 100.0;

/// Breite der Spalte „Aktion“: ein Viertel der Tabelle, 140 bis 240 Pixel (das Fenster ist bei
/// 800 × 600 nur 720 breit, die Belegungen brauchen dann den Platz).
fn action_width(total: f32) -> f32 {
    (total * 0.25).clamp(140.0, 240.0)
}

/// „Strg+S ist schon für „Speichern“ belegt.“
pub fn conflict_text(b: &Binding, other: Action) -> String {
    format!(
        "{} ist schon für „{}“ belegt.",
        text::binding_text(b),
        other.label()
    )
}

/// Hinweis nach „Zurücksetzen“, wenn eine andere Aktion eine Voreinstellung abgeben musste.
pub fn taken_text(b: &Binding, other: Action) -> String {
    format!(
        "{} gehört wieder zur Voreinstellung und gilt nicht mehr für „{}“.",
        text::binding_text(b),
        other.label()
    )
}

/// Hinweis nach „Tauschen“: welche Aktion die Belegung abgeben musste.
pub fn swapped_text(b: &Binding, other: Action) -> String {
    format!(
        "{} gilt nicht mehr für „{}“.",
        text::binding_text(b),
        other.label()
    )
}

/// Zustand des Abschnitts zwischen den Bildern.
#[derive(Clone, Debug, Default)]
pub struct CaptureState {
    /// Suchtext über der Tabelle.
    pub filter: String,
    /// Es wird erfasst: Aktion und Stelle der Belegung (ab der Länge der Liste: neu anhängen).
    pub capturing: Option<(Action, usize)>,
    /// Offene Doppelbelegung: Aktion, Stelle, neue Belegung, Aktion, der sie schon gehört.
    pub pending_conflict: Option<(Action, usize, Binding, Action)>,
    /// Speichern fehlgeschlagen ([`KEYS_NOT_SAVED`]).
    pub message: Option<RootMessage>,
    /// Kurzer Hinweis nach einer Änderung (etwa [`taken_text`]).
    pub info: Option<String>,
    /// Beim Erfassen einer Ziehgeste: gedrückte Taste und wo.
    drag_start: Option<(MouseButton, egui::Pos2)>,
    /// Eine Ziehgeste ist erfasst, die Taste aber noch gedrückt: bis zum Loslassen gesperrt.
    hold_until_release: bool,
    /// Die Rückfrage zur Doppelbelegung ist neu und noch nicht ganz zu sehen: die Tabelle rollt
    /// sie ins Bild (sie steht unter der Zeile, bei der letzten sichtbaren Zeile sonst außerhalb).
    scroll_to_conflict: bool,
    /// Die zuletzt erfasste Taste ist noch gedrückt: bis zum Loslassen gesperrt, ihre
    /// Wiederholungen werden verbraucht.
    held_key: Option<egui::Key>,
    /// Durchgang, dessen Ereignisse [`capture_input`] schon gelesen hat (nur einmal je Durchgang).
    read_pass: Option<u64>,
}

impl CaptureState {
    /// Erfassen und offene Doppelbelegung beenden (Fenster zu).
    pub fn stop(&mut self) {
        self.capturing = None;
        self.pending_conflict = None;
        self.drag_start = None;
        self.hold_until_release = false;
        self.held_key = None;
    }
}

fn capturing_id() -> egui::Id {
    egui::Id::new("planora-keymap-capturing")
}

/// Erfassen die Einstellungen gerade eine Belegung? Dann löst keine Taste und keine Mausgeste
/// etwas aus (siehe Moduldoku).
pub fn is_capturing(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(capturing_id()))
        .unwrap_or(false)
}

/// Meldet den Stand für [`is_capturing`] (die Einstellungen in jedem Bild, geschlossen `false`).
pub fn set_capturing(ctx: &egui::Context, on: bool) {
    if is_capturing(ctx) != on {
        ctx.data_mut(|d| d.insert_temp(capturing_id(), on));
    }
}

/// Wird für `a` eine Mausgeste statt einer Taste erfasst?
pub fn is_mouse_action(a: Action) -> bool {
    matches!(a, Action::ViewPan | Action::ViewOrbit | Action::ViewZoom)
}

fn mouse_button(b: egui::PointerButton) -> Option<MouseButton> {
    match b {
        egui::PointerButton::Primary => Some(MouseButton::Left),
        egui::PointerButton::Middle => Some(MouseButton::Middle),
        egui::PointerButton::Secondary => Some(MouseButton::Right),
        _ => None,
    }
}

enum Caught {
    /// Esc.
    Cancel,
    Bound(Binding),
}

/// Liest die Ereignisse dieses Bilds, solange erfasst wird, und übernimmt eine erfasste Belegung.
/// Vor dem Zeichnen des Fensters aufrufen (das Fenster tut das, [`show`] auch): die
/// Tastenereignisse werden verbraucht, Knöpfe und Felder sehen sie nicht. Bei offener Rückfrage
/// (`locked`) wird nicht erfasst, ein laufendes Erfassen endet.
pub fn capture_input(
    ctx: &egui::Context,
    ws: &mut Workspace,
    state: &mut CaptureState,
    locked: bool,
) {
    let pass = ctx.cumulative_pass_nr();
    if state.read_pass == Some(pass) {
        return;
    }
    state.read_pass = Some(pass);
    if state.hold_until_release && !ctx.input(|i| i.pointer.any_down()) {
        state.hold_until_release = false;
    }
    if let Some(k) = state.held_key {
        let down = ctx.input_mut(|i| {
            i.events
                .retain(|e| !matches!(e, egui::Event::Key { key, .. } if *key == k));
            i.keys_down.contains(&k)
        });
        if !down {
            state.held_key = None;
        }
    }
    if locked {
        state.capturing = None;
        state.drag_start = None;
    }
    if let Some((a, index)) = state.capturing {
        let caught = ctx.input_mut(|i| catch(i, a, &mut state.drag_start));
        match caught {
            Some(Caught::Cancel) => {
                state.capturing = None;
                state.drag_start = None;
                state.held_key = Some(egui::Key::Escape);
            }
            Some(Caught::Bound(b)) => {
                match b {
                    Binding::Mouse(Gesture::Drag { .. }) => state.hold_until_release = true,
                    Binding::Key(c) => state.held_key = Some(c.key),
                    Binding::Mouse(Gesture::Wheel { .. }) => {}
                }
                state.drag_start = None;
                take(ws, state, a, index, b);
            }
            None => {}
        }
    }
    publish(ctx, state);
}

fn publish(ctx: &egui::Context, state: &CaptureState) {
    set_capturing(
        ctx,
        state.capturing.is_some() || state.hold_until_release || state.held_key.is_some(),
    );
}

/// Sucht in den Ereignissen eine Belegung für `a` und verbraucht alle Tastenereignisse.
fn catch(
    i: &mut egui::InputState,
    a: Action,
    drag_start: &mut Option<(MouseButton, egui::Pos2)>,
) -> Option<Caught> {
    let held = i.modifiers;
    let mut caught = None;
    for e in &i.events {
        if caught.is_some() {
            break;
        }
        match e {
            egui::Event::Key {
                key: egui::Key::Escape,
                pressed: true,
                ..
            } => caught = Some(Caught::Cancel),
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if !is_mouse_action(a) && !text::is_modifier_key(*key) => {
                caught = Some(Caught::Bound(Binding::Key(KeyChord {
                    key: *key,
                    ctrl: modifiers.ctrl || modifiers.command,
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                })));
            }
            egui::Event::PointerButton {
                button,
                pressed,
                pos,
                ..
            } if is_mouse_action(a) && a != Action::ViewZoom => {
                if let Some(b) = mouse_button(*button) {
                    if *pressed {
                        *drag_start = Some((b, *pos));
                    } else if drag_start.is_some_and(|(s, _)| s == b) {
                        // Losgelassen, ohne weit genug zu ziehen: ein Klick, keine Geste.
                        *drag_start = None;
                    }
                }
            }
            egui::Event::PointerMoved(p) if a != Action::ViewZoom => {
                if let Some((button, start)) = *drag_start
                    && is_mouse_action(a)
                    && start.distance(*p) >= DRAG_MIN
                {
                    caught = Some(Caught::Bound(Binding::Mouse(Gesture::Drag {
                        button,
                        ctrl: held.ctrl || held.command,
                        shift: held.shift,
                        alt: held.alt,
                    })));
                }
            }
            egui::Event::MouseWheel {
                delta, modifiers, ..
            } if a == Action::ViewZoom && *delta != egui::Vec2::ZERO => {
                caught = Some(Caught::Bound(Binding::Mouse(Gesture::Wheel {
                    ctrl: modifiers.ctrl || modifiers.command,
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                })));
            }
            _ => {}
        }
    }
    // Keine Taste und kein getippter Text erreicht beim Erfassen etwas anderes.
    i.events.retain(|e| {
        !matches!(
            e,
            egui::Event::Key { .. }
                | egui::Event::Text(_)
                | egui::Event::Copy
                | egui::Event::Cut
                | egui::Event::Paste(_)
        )
    });
    if a == Action::ViewZoom {
        // Das Rad gehört dem Erfassen, nicht dem Rollen des Fensters.
        i.smooth_scroll_delta = egui::Vec2::ZERO;
    }
    caught
}

/// Übernimmt die erfasste Belegung `b` an Stelle `index` von `a`: unverändert → nichts;
/// Doppelbelegung → Rückfrage; sonst gesetzt und gespeichert.
fn take(ws: &mut Workspace, state: &mut CaptureState, a: Action, index: usize, b: Binding) {
    state.capturing = None;
    state.info = None;
    if ws.keymap.bindings(a).get(index) == Some(&b) {
        return;
    }
    if let Some(text) = Keymap::fixed_key_text(a, &b) {
        state.info = Some(text);
        return;
    }
    if let Some(other) = ws.keymap.conflict_for(a, &b) {
        state.pending_conflict = Some((a, index, b, other));
        state.scroll_to_conflict = true;
        return;
    }
    let mut km = ws.keymap.clone();
    km.set(a, index, b);
    commit(ws, state, km);
}

/// Übernimmt `km` und speichert sie; misslingt das Speichern, der Hinweis [`KEYS_NOT_SAVED`].
fn commit(ws: &mut Workspace, state: &mut CaptureState, km: Keymap) {
    state.message = (!ws.set_keymap(km)).then(|| RootMessage::Warning(KEYS_NOT_SAVED.into()));
}

/// Was ein Klick im Abschnitt auslöst (nach dem Zeichnen ausgeführt).
enum Click {
    Change(Action, usize),
    Remove(Action, usize),
    Reset(Action),
    ResetAll,
    Swap,
    CancelConflict,
    CancelCapture,
}

/// Zeichnet den Abschnitt (Überschrift, Hinweise, Suche, Tabelle) und führt Klicks aus. Liest
/// vorher die Ereignisse des Erfassens ([`capture_input`]; das Fenster hat das meist schon
/// getan, dann ist nichts mehr da).
pub fn show(ui: &mut egui::Ui, ws: &mut Workspace, state: &mut CaptureState) {
    let locked = !ui.is_enabled();
    capture_input(ui.ctx(), ws, state, locked);
    // Erfassen seit dem vorigen Bild (nicht gerade erst mit diesem Klick begonnen).
    let was_capturing = state.capturing;
    let dark = ui.visuals().dark_mode;
    let warning = theme::palette(dark).warning;
    let muted = theme::palette(dark).text_secondary;
    let mut click = None;
    theme::section_header(ui, SECTION, theme::SUB_HEADER_H);
    ui.add_space(theme::SPACE_2);
    if !ws.key_notes.is_empty() {
        ui.colored_label(warning, NOTES_HEADING);
        for n in &ws.key_notes {
            ui.add(egui::Label::new(egui::RichText::new(n).color(warning)).wrap());
        }
        ui.add_space(theme::SPACE_2);
    }
    ui.horizontal(|ui| {
        theme::text_field(
            ui,
            egui::TextEdit::singleline(&mut state.filter)
                .hint_text(SEARCH_HINT)
                .desired_width(320.0),
        );
        let changed = Action::ALL.iter().any(|&a| !ws.keymap.is_default(a));
        if ui
            .add_enabled(changed, egui::Button::new(RESET_ALL))
            .clicked()
        {
            click = Some(Click::ResetAll);
        }
    });
    ui.label(egui::RichText::new(HELP).color(muted));
    if let Some(m) = &state.message {
        ui.colored_label(m.color(dark), m.text());
    }
    if let Some(info) = &state.info {
        ui.label(info.as_str());
    }
    ui.add_space(theme::SPACE_1);
    // Alle Breiten aus der Breite des Abschnitts, nicht aus dem Platz in der Zeile: der wächst
    // in der rollenden Tabelle mit ihrem Inhalt mit, das Fenster würde Bild für Bild breiter.
    let table_w = ui.available_width();
    let action_w = action_width(table_w);
    ui.horizontal(|ui| {
        column(ui, action_w, |ui| {
            ui.strong(COLUMN_ACTION);
        });
        column(ui, AREA_W, |ui| {
            ui.strong(COLUMN_AREA);
        });
        ui.strong(COLUMN_BINDINGS);
    });
    ui.separator();
    // Nach einer erfassten Ziehgeste ist die Taste noch unten: ihr Loslassen soll keinen Knopf
    // drücken.
    let hold = state.hold_until_release;
    let shown: Vec<Action> = Action::ALL
        .iter()
        .copied()
        .filter(|&a| search::matches(a, &ws.keymap, &state.filter))
        .collect();
    // Ist die Rückfrage zur Doppelbelegung ganz zu sehen (dann muss nicht mehr gerollt werden)?
    let mut visible_conflict = false;
    theme::scroll_area(
        ui,
        egui::ScrollArea::vertical()
            .id_salt("tastenkuerzel_tabelle")
            .auto_shrink([false, true])
            .max_height(table_height_in(ui.ctx().content_rect())),
        |ui| {
            ui.set_max_width(table_w);
            ui.add_enabled_ui(!hold, |ui| {
                if shown.is_empty() {
                    ui.label(NOTHING_FOUND);
                }
                let mut conflict_seen = false;
                for &a in &shown {
                    conflict_seen |= row(ui, &ws.keymap, state, a, table_w, action_w, &mut click);
                }
                visible_conflict = conflict_seen;
            });
        },
    );
    if visible_conflict {
        state.scroll_to_conflict = false;
    }
    match click {
        Some(Click::Change(a, i)) => {
            state.capturing = Some((a, i));
            state.pending_conflict = None;
            state.drag_start = None;
            state.info = None;
        }
        Some(Click::Remove(a, i)) => {
            let mut km = ws.keymap.clone();
            km.remove(a, i);
            state.stop();
            state.info = None;
            commit(ws, state, km);
        }
        Some(Click::Reset(a)) => {
            let mut km = ws.keymap.clone();
            let taken = km.reset(a);
            state.stop();
            state.info = (!taken.is_empty()).then(|| {
                taken
                    .iter()
                    .map(|(b, o)| taken_text(b, *o))
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            commit(ws, state, km);
        }
        Some(Click::ResetAll) => {
            let mut km = ws.keymap.clone();
            km.reset_all();
            state.stop();
            state.info = None;
            commit(ws, state, km);
        }
        Some(Click::Swap) => {
            if let Some((a, i, b, _)) = state.pending_conflict.take() {
                let mut km = ws.keymap.clone();
                // Alle, die sie verlieren (mit „Überall“ können es mehrere sein).
                let losers: Vec<Action> = Action::ALL
                    .iter()
                    .copied()
                    .filter(|&o| {
                        o != a
                            && Keymap::areas_overlap(a.area(), o.area())
                            && km.bindings(o).contains(&b)
                    })
                    .collect();
                km.swap(a, i, b);
                state.info = Some(
                    losers
                        .iter()
                        .map(|o| swapped_text(&b, *o))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                commit(ws, state, km);
            }
        }
        Some(Click::CancelConflict) => state.pending_conflict = None,
        Some(Click::CancelCapture) => {
            state.capturing = None;
            state.drag_start = None;
        }
        None => {
            // Ein Klick woanders (Suchfeld, Projektverzeichnis, außerhalb des Fensters) beendet
            // das Erfassen: sonst landete die nächste Taste für das Feld in der Belegung.
            let elsewhere = ui.input(|i| i.pointer.primary_clicked());
            if elsewhere && was_capturing.is_some() && state.capturing == was_capturing {
                state.capturing = None;
                state.drag_start = None;
            }
        }
    }
    publish(ui.ctx(), state);
}

/// Spalte fester Breite (auch in der Übersicht, [`super::overview`]).
pub(super) fn column(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, theme::ROW_H),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(width);
            add(ui);
        },
    );
}

/// Eine Belegung als Chip (vertiefte Fläche, nicht anklickbar). Als Widget mit bekannter Größe,
/// damit die umbrechende Zeile vor ihm umbricht statt ihn hinauszuschieben. Auch in der
/// Übersicht ([`super::overview`]).
pub(super) fn chip(ui: &mut egui::Ui, text: &str, color: Option<egui::Color32>) {
    let dark = ui.visuals().dark_mode;
    let t = egui::RichText::new(text);
    ui.add(
        egui::Button::new(match color {
            Some(c) => t.color(c),
            None => t,
        })
        .fill(theme::palette(dark).bg_inset)
        .stroke(egui::Stroke::NONE)
        .sense(egui::Sense::hover()),
    );
}

fn capture_prompt(a: Action) -> &'static str {
    match a {
        Action::ViewZoom => WHEEL_NOW,
        a if is_mouse_action(a) => DRAG_NOW,
        _ => PRESS_KEY,
    }
}

/// Eine Zeile der Tabelle, darunter gegebenenfalls die Doppelbelegung. Ist diese neu
/// (`scroll_to_conflict`), wird sie ins Bild gerollt, bis sie ganz zu sehen ist; `true`, wenn sie
/// ganz zu sehen ist.
fn row(
    ui: &mut egui::Ui,
    km: &Keymap,
    state: &CaptureState,
    a: Action,
    table_w: f32,
    action_w: f32,
    click: &mut Option<Click>,
) -> bool {
    let dark = ui.visuals().dark_mode;
    let accent = theme::accent(dark);
    let muted = theme::palette(dark).text_secondary;
    let list = km.bindings(a);
    let capturing = state.capturing.filter(|(x, _)| *x == a).map(|(_, i)| i);
    let mut visible = false;
    ui.push_id(a.id(), |ui| {
        ui.horizontal_top(|ui| {
            column(ui, action_w, |ui| {
                ui.add(egui::Label::new(a.label()).wrap());
            });
            column(ui, AREA_W, |ui| {
                ui.label(a.area().label());
            });
            let gaps = 3.0 * ui.spacing().item_spacing.x;
            let width = (table_w - action_w - AREA_W - RESET_W - gaps).max(0.0);
            ui.allocate_ui_with_layout(
                egui::vec2(width, theme::ROW_H),
                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                |ui| {
                    ui.set_min_width(width);
                    if list.is_empty() && capturing.is_none() {
                        chip(ui, UNBOUND, Some(muted));
                    }
                    // Chip und Knöpfe einzeln in die umbrechende Zeile (nicht als Block): so
                    // rutschen bei wenig Platz die Knöpfe in die nächste Zeile.
                    for (i, b) in list.iter().enumerate() {
                        if capturing == Some(i) {
                            chip(ui, capture_prompt(a), Some(accent));
                            if ui.small_button(CANCEL).clicked() {
                                *click = Some(Click::CancelCapture);
                            }
                        } else {
                            chip(ui, &text::binding_text(b), None);
                            if ui.small_button(CHANGE).clicked() {
                                *click = Some(Click::Change(a, i));
                            }
                            if ui.small_button(REMOVE).clicked() {
                                *click = Some(Click::Remove(a, i));
                            }
                        }
                    }
                    if capturing.is_some_and(|i| i >= list.len()) {
                        chip(ui, capture_prompt(a), Some(accent));
                        if ui.small_button(CANCEL).clicked() {
                            *click = Some(Click::CancelCapture);
                        }
                    } else if ui.small_button(ADD).clicked() {
                        *click = Some(Click::Change(a, list.len()));
                    }
                },
            );
            column(ui, RESET_W, |ui| {
                if ui
                    .add_enabled(!km.is_default(a), egui::Button::new(RESET))
                    .clicked()
                {
                    *click = Some(Click::Reset(a));
                }
            });
        });
        if let Some((ca, _, b, other)) = &state.pending_conflict
            && *ca == a
        {
            let r = ui.horizontal_wrapped(|ui| {
                ui.add_space(action_w + AREA_W);
                ui.colored_label(theme::palette(dark).warning, conflict_text(b, *other));
                if ui.button(SWAP).clicked() {
                    *click = Some(Click::Swap);
                }
                if ui.button(CANCEL).clicked() {
                    *click = Some(Click::CancelConflict);
                }
            });
            visible = ui.clip_rect().contains_rect(r.response.rect);
            if state.scroll_to_conflict && !visible {
                r.response.scroll_to_me(Some(egui::Align::Center));
            }
        }
    });
    visible
}
