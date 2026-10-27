//! Fenster „Tastenkürzel“: Übersicht aller Aktionen mit ihrer aktuellen Belegung, nur zum Lesen.
//! Sortiert nach Bereich (Überall, Modell, Skizze, Zeichnung, je mit Überschrift), darüber ein
//! Suchfeld (sucht Aktion, Bereich und Taste wie die Einstellungen, [`search::matches`]), unten
//! „In den Einstellungen ändern“ und „Schließen“. Die Belegung wird in jedem Bild aus
//! [`Workspace::keymap`] gelesen: eine Änderung zeigt sich sofort.
//!
//! **Öffnen.** Über „Ansicht → Tastenkürzel …“ und die Taste von [`Action::ShowShortcuts`] (in
//! der Skizze [`Action::SketchShowShortcuts`], Voreinstellung beide S); das Suchfeld hat dann die
//! Tastatur. Die Taste wirkt wie jedes Kürzel nur, wenn kein Textfeld die Tastatur hat. Solange
//! das Suchfeld sie hat, löst keine Taste etwas aus (der Verteiler hält still,
//! [`crate::viewport::typing`]).
//!
//! **Schließen.** ✕, „Schließen“ und Esc. Esc schließt immer, auch mit Text im Suchfeld, aber
//! nicht, solange ein anderes Textfeld die Tastatur hat oder eine Rückfrage offen ist
//! ([`take_escape`]).
//!
//! Welche anderen Fenster die Übersicht schließt und welche sie schließen, regelt die App
//! (`app::open_overview`, `app::enforce_window_rules`).

use super::settings_ui::{chip, column};
use super::{Action, Keymap, search, text};
use crate::dialog::{Dialog, DialogSize, close_label};
use crate::theme;
use crate::workspace::Workspace;
use planora_i18n::tr;

pub fn title() -> String {
    tr!("shortcuts-title")
}
pub fn search_hint() -> String {
    tr!("shortcuts-search-hint")
}
pub fn nothing_found() -> String {
    tr!("shortcuts-nothing-found")
}
/// Knopf in der Fußzeile: schließt die Übersicht und öffnet die Einstellungen.
pub fn to_settings() -> String {
    tr!("shortcuts-to-settings")
}
/// Statt einer Belegung, wenn die Aktion keine hat (blass).
pub fn unbound() -> String {
    planora_i18n::tr!("keymap-unbound-dash")
}
/// Größte Höhe der Liste; darüber rollt sie (Suchfeld und Fußzeile bleiben stehen).
pub const LIST_MAX_H: f32 = 720.0;
/// So viel Fensterhöhe bleibt für Kopfzeile, Suchfeld, Fußzeile und Rand.
const LIST_RESERVE_H: f32 = 300.0;

/// Breite der Spalte „Aktion“.
const ACTION_W: f32 = 280.0;
/// Kennung des Fensters (unabhängig vom Titel; die Einstellungen haben einen gleichnamigen
/// Abschnitt).
const WINDOW_ID: &str = "planora_shortcuts_window";

/// Kennung des Suchfelds (Fokus beim Öffnen; für Tests).
pub fn search_id() -> egui::Id {
    egui::Id::new("planora_shortcuts_search")
}

/// Zustand der Übersicht zwischen den Bildern (gehört der App).
#[derive(Clone, Debug, Default)]
pub struct OverviewState {
    pub open: bool,
    /// Suchtext.
    pub filter: String,
    /// Das Suchfeld soll die Tastatur bekommen (bis es sie hat).
    pub focus_search: bool,
    /// Hatte das Suchfeld im vorigen Bild die Tastatur? (Esc nimmt egui den Fokus schon zu
    /// Beginn des Bilds weg.)
    search_had_focus: bool,
}

impl OverviewState {
    /// Öffnet die Übersicht mit leerem Suchfeld, das die Tastatur bekommt.
    pub fn open_focused(&mut self) {
        self.open = true;
        self.filter.clear();
        self.focus_search = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.focus_search = false;
        self.search_had_focus = false;
    }
}

/// Was der Aufrufer nach [`show`] tun soll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverviewEvent {
    None,
    /// „In den Einstellungen ändern“: die Einstellungen öffnen (die Übersicht ist schon zu).
    OpenSettings,
}

/// Esc schließt die offene Übersicht und wird verbraucht (vor den Ansichten aufrufen: weder
/// Auswahl noch Skizze sehen es). Nicht bei offener Rückfrage (`prompt_open`) und nicht, solange
/// ein anderes Textfeld als das Suchfeld die Tastatur hat (dort gehört Esc dem Feld).
pub fn take_escape(ctx: &egui::Context, state: &mut OverviewState, prompt_open: bool) {
    if !state.open || prompt_open {
        return;
    }
    if crate::viewport::typing(ctx) && !state.search_had_focus {
        return;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        state.close();
    }
}

/// Die Aktionen, die zu `filter` passen, sortiert nach Bereich (innerhalb in der Reihenfolge
/// der Tabelle).
pub fn shown_actions(km: &Keymap, filter: &str) -> Vec<Action> {
    let mut list: Vec<Action> = Action::ALL
        .iter()
        .copied()
        .filter(|&a| search::matches(a, km, filter))
        .collect();
    list.sort_by_key(|a| a.area());
    list
}

/// Höhe der Liste: was das Fenster neben Suchfeld und Fußzeile hergibt, höchstens
/// [`LIST_MAX_H`] (bei 800 × 600 wenigstens 160).
fn list_height(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().height() - LIST_RESERVE_H).clamp(160.0, LIST_MAX_H)
}

/// Zeichnet das Fenster, solange `state.open` gilt.
pub fn show(ctx: &egui::Context, ws: &Workspace, state: &mut OverviewState) -> OverviewEvent {
    if !state.open {
        state.search_had_focus = false;
        return OverviewEvent::None;
    }
    let (mut open, mut close, mut to_settings) = (true, false, false);
    let km = &ws.keymap;
    Dialog::new(title(), DialogSize::Wide)
        .id(egui::Id::new(WINDOW_ID))
        .center_on_open()
        .open(&mut open)
        .show_with_footer(
            ctx,
            |ui| {
                let r = theme::text_field(
                    ui,
                    egui::TextEdit::singleline(&mut state.filter)
                        .id(search_id())
                        .hint_text(search_hint())
                        .desired_width(320.0),
                );
                if state.focus_search {
                    r.request_focus();
                    state.focus_search = false;
                }
                state.search_had_focus = r.has_focus();
                ui.add_space(theme::SPACE_2);
                let shown = shown_actions(km, &state.filter);
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .id_salt("tastenkuerzel_uebersicht")
                        .auto_shrink([false, true])
                        .max_height(list_height(ctx)),
                    |ui| {
                        if shown.is_empty() {
                            ui.label(nothing_found());
                        }
                        let mut area = None;
                        for &a in &shown {
                            if area != Some(a.area()) {
                                if area.is_some() {
                                    ui.add_space(theme::SPACE_2);
                                }
                                theme::section_header(ui, &a.area().label(), theme::SUB_HEADER_H);
                                area = Some(a.area());
                            }
                            row(ui, km, a);
                        }
                    },
                );
            },
            |ui| {
                ui.horizontal(|ui| {
                    to_settings = ui.button(crate::keymap::overview::to_settings()).clicked();
                    close = ui.button(close_label()).clicked();
                });
            },
        );
    if !open || close || to_settings {
        state.close();
    }
    if to_settings {
        OverviewEvent::OpenSettings
    } else {
        OverviewEvent::None
    }
}

/// Eine Zeile: Aktion und ihre Belegungen als Chips; ohne Belegung Aktion und „–“ blass.
fn row(ui: &mut egui::Ui, km: &Keymap, a: Action) {
    let muted = theme::palette(ui.visuals().dark_mode).text_secondary;
    let list = km.bindings(a);
    ui.push_id(a.id(), |ui| {
        ui.horizontal(|ui| {
            column(ui, ACTION_W, |ui| {
                let label = egui::RichText::new(a.label());
                let label = if list.is_empty() {
                    label.color(muted)
                } else {
                    label
                };
                ui.add(egui::Label::new(label).wrap());
            });
            ui.horizontal_wrapped(|ui| {
                if list.is_empty() {
                    chip(ui, &unbound(), Some(muted));
                }
                for b in list {
                    chip(ui, &text::binding_text(b), None);
                }
            });
        });
    });
}
