//! Fenster „Zusatzmodule“ (Datei → Zusatzmodule → Verwalten …; Vorbild Fusion „Skripte und
//! Zusatzmodule“): die Verwaltung der Plug-ins in einem eigenen, abdockbaren Fenster.
//!
//! Oben „Aus Datei…“ und das Feld „Repo-Link“ mit „Holen“, darunter ein Hinweis
//! ([`Plugins::note`]) und der Ordner der installierten Plug-ins. Links die Liste (je Plug-in
//! Schalter, Name und Zustand; ein Klick auf den Namen wählt), rechts alles zum gewählten:
//! Version, Herkunft, Zustand, Autor, Lizenz, Beschreibung, die verlangten Rechte mit ihrem
//! Stand und „Zurücknehmen“ (gleich sichtbar, ohne Aufklappen), Quelle und Updates bei einem
//! Repo-Link, „Entfernen“ bei installierten und nicht aktive Fundorte grau darunter.
//! Schalter und „Zurücknehmen“ werden gleich gespeichert ([`app_plugins::persist`]),
//! „Entfernen“ fragt erst nach.

use crate::dialog::{Dialog, DialogSize, close_label};
use crate::file_chooser::{self, ChooserAction, FileChooser, display_path};
use crate::places::Places;
use crate::plugin_prompts::{grant_text, origin_text, right_name, state_text};
use crate::plugins::{self as app_plugins, Plugins, entry_name};
use crate::theme;
use crate::workspace::{Prompt, Workspace};
use planora_i18n::tr;
use planora_plugin_host::licenses::license_ok;
use planora_plugin_host::manifest::Right;
use planora_plugin_host::places::Origin;
use planora_plugin_host::{EntryState, PluginEntry};
use std::cell::RefCell;
use std::collections::BTreeMap;

/// Titel des Fensters.
pub fn title() -> String {
    tr!("addins-title")
}
/// Überschrift der Liste.
pub fn list_title() -> String {
    tr!("settings-section-plugins")
}
/// Knopf „Aus Datei…“ (Plug-in aus einer Paketdatei installieren).
pub fn install_plugin_label() -> String {
    tr!("plugin-install-from-file")
}
/// Knopf „Nach Updates suchen“ eines Plug-ins mit Repo-Link.
pub fn check_updates_label() -> String {
    tr!("plugin-check-updates")
}
/// Knopf „Aktualisieren auf { $version }“.
pub fn update_to_label(version: &str) -> String {
    tr!("plugin-update-to", version = version)
}
/// Knopf „Entfernen“ eines installierten Plug-ins.
pub fn remove_plugin_label() -> String {
    tr!("plugin-settings-remove")
}
/// Knopf „Zurücknehmen“ eines entschiedenen Rechts.
pub fn revoke_label() -> String {
    tr!("plugin-right-revoke")
}
/// Zeile eines nicht aktiven Fundorts: „installiert 0.1.0: überdeckt von Version 0.2.0“.
pub fn shadowed_text(origin: Origin, version: &str, active: &str) -> String {
    tr!(
        "plugin-settings-shadowed",
        origin = origin_text(origin),
        version = version,
        active = active
    )
}

/// Id des Fensters (auch abgedockt).
const WINDOW_ID: &str = "planora_addins_window";
/// Breite der Liste links.
pub const LIST_W: f32 = 320.0;
/// Höhe von Liste und Details (darüber rollen beide).
pub const BODY_H: f32 = 420.0;
/// Breite des Felds „Repo-Link“.
const LINK_FIELD_W: f32 = 320.0;

/// Was im Fenster gedrückt wurde (ausgeführt nach dem Zeichnen).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Select(String),
    Enable(String, bool),
    Revoke(String, Right),
    Remove(String),
    Install,
    /// „Holen“ mit dem Text im Feld „Repo-Link“.
    Fetch,
    CancelFetch,
    /// „Nach Updates suchen“.
    CheckUpdates,
    /// „Aktualisieren auf …“ (öffnet den Aktualisieren-Dialog).
    OpenUpdate(String),
}

/// Zustand des Fensters (gehört der App wie die Einstellungen).
#[derive(Default)]
pub struct AddinsWindow {
    open: bool,
    /// Das gewählte Plug-in (Kennung); fehlt es, gilt das erste der Liste.
    selected: Option<String>,
    /// Dateiauswahl für `*.pcplugin`, solange gewählt wird.
    chooser: Option<FileChooser>,
    /// Rechteck des Fensters im letzten Bild (Layouttests).
    pub last_rect: Option<egui::Rect>,
    /// Rechteck des Schalters je Plug-in im letzten Bild (Tests klicken darauf).
    pub last_toggles: BTreeMap<String, egui::Rect>,
}

impl AddinsWindow {
    /// Öffnet das Fenster; eine offene Auswahl geht zu.
    pub fn open(&mut self) {
        self.open = true;
        self.chooser = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.chooser = None;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Wählt das Plug-in `id` (rechts stehen seine Details).
    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.to_string());
    }

    /// Das gewählte Plug-in, wie es das Fenster zeigt (ohne Wahl das erste).
    pub fn selected_in(&self, plugins: &Plugins) -> Option<String> {
        let entries = plugins.host.entries();
        self.selected
            .as_ref()
            .filter(|id| entries.iter().any(|e| &e.id == *id))
            .cloned()
            .or_else(|| entries.first().map(|e| e.id.clone()))
    }

    /// Öffnet die Auswahl einer Datei `*.pcplugin` in [`file_chooser::default_dir`]; die gewählte
    /// Datei liest [`Plugins::read_package`].
    pub fn start_choosing(&mut self) {
        self.chooser = Some(FileChooser::open(
            file_chooser::default_dir(),
            vec!["pcplugin"],
        ));
    }

    /// Wird gerade eine Paketdatei gewählt?
    pub fn is_choosing(&self) -> bool {
        self.chooser.is_some()
    }

    pub fn chooser_mut(&mut self) -> Option<&mut FileChooser> {
        self.chooser.as_mut()
    }

    /// Wie in der App: [`AddinsWindow::show`] mit den Schnellzugriffen des Arbeitsbereichs;
    /// geänderte Favoriten werden danach gespeichert.
    pub fn show_in(&mut self, ctx: &egui::Context, ws: &mut Workspace) {
        if !self.open {
            self.last_rect = None;
            return;
        }
        let mut places = ws.places();
        self.show(ctx, ws, &mut places);
        ws.save_places(places);
    }

    /// Zeichnet das Fenster (abdockbar unter `"addins"`, `center_on_open`, ✕, Fußzeile
    /// „Schließen“ bzw. beim Wählen die Fußzeile der Auswahl). Bei offener Rückfrage ist der
    /// Inhalt gesperrt; „Schließen“ und ✕ bleiben.
    pub fn show(&mut self, ctx: &egui::Context, ws: &mut Workspace, places: &mut Places) {
        if !self.open {
            self.last_rect = None;
            return;
        }
        let locked = ws.prompt.is_some();
        let (mut open, mut close) = (true, false);
        let selected = self.selected_in(&ws.plugins);
        let mut link_text = ws.plugins.link_text.clone();
        let choosing = self.chooser.is_some();
        let folder = folder_state(&ws.plugins);
        let chooser = RefCell::new(self.chooser.as_mut());
        let (mut body_action, mut footer_action) = (ChooserAction::None, ChooserAction::None);
        let mut toggles = BTreeMap::new();
        let response = Dialog::new(title(), DialogSize::File)
            .dock_key("addins")
            .id(egui::Id::new(WINDOW_ID))
            .center_on_open()
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| {
                    // Abgedockt kann dieser Teil in einem Bild mehrmals laufen: es gilt der letzte.
                    let mut actions = Vec::new();
                    toggles.clear();
                    ui.add_enabled_ui(!locked, |ui| {
                        show_toolbar(ui, &ws.plugins, choosing, &mut link_text, &mut actions);
                        show_notes(ui, &ws.plugins, &folder);
                        ui.add_space(theme::SPACE_2);
                        ui.horizontal_top(|ui| {
                            show_list(
                                ui,
                                &ws.plugins,
                                selected.as_deref(),
                                &mut actions,
                                &mut toggles,
                            );
                            ui.add_space(theme::SPACE_4);
                            show_details(ui, &ws.plugins, selected.as_deref(), &mut actions);
                        });
                        if let Some(c) = chooser.borrow_mut().as_mut() {
                            ui.add_space(theme::SPACE_4);
                            ui.strong(tr!("plugin-settings-install"));
                            body_action = c.show_body(ui, places);
                        }
                    });
                    actions
                },
                |ui| match chooser.borrow_mut().as_mut() {
                    Some(c) => {
                        footer_action = ui.add_enabled_ui(!locked, |ui| c.show_footer(ui)).inner
                    }
                    None => {
                        if ui.button(close_label()).clicked() {
                            close = true;
                        }
                    }
                },
            );
        self.last_toggles = toggles;
        let actions = match response {
            Some(r) => {
                self.last_rect = Some(r.response.rect);
                r.inner.unwrap_or_default()
            }
            None => {
                self.last_rect = None;
                Vec::new()
            }
        };
        ws.plugins.link_text = link_text;
        if !locked {
            for action in actions {
                self.apply(ws, action);
            }
        }
        for action in [body_action, footer_action] {
            let target = match action {
                ChooserAction::Chosen(p) => Some(p),
                ChooserAction::Cancel => None,
                ChooserAction::None => continue,
            };
            self.chooser = None;
            if let Some(file) = target {
                ws.plugins.read_package(file);
            }
        }
        if !open || close {
            self.close();
        }
    }

    /// Führt eine Handlung aus; Schalter und „Zurücknehmen“ werden gleich gespeichert,
    /// „Entfernen“ fragt erst nach.
    fn apply(&mut self, ws: &mut Workspace, action: Action) {
        if !matches!(action, Action::Select(_)) {
            ws.plugins.note = None;
        }
        match action {
            Action::Select(id) => self.selected = Some(id),
            Action::Enable(id, on) => {
                ws.plugins.set_enabled(&id, on);
                app_plugins::persist(ws);
            }
            Action::Revoke(id, right) => {
                ws.plugins.host.revoke(&id, right);
                app_plugins::persist(ws);
            }
            Action::Remove(id) => {
                let name = ws.plugins.plugin_name(&id);
                ws.prompt = Some(Prompt::RemovePlugin { id, name });
            }
            Action::Install => self.start_choosing(),
            Action::Fetch => {
                let text = ws.plugins.link_text.clone();
                ws.plugins.install_from_link(&text);
            }
            Action::CancelFetch => ws.plugins.cancel_fetch(),
            Action::CheckUpdates => ws.plugins.check_updates(true),
            Action::OpenUpdate(id) => ws.plugins.open_update(&id),
        }
    }
}

/// Der Ordner der installierten Plug-ins zur Anzeige und, falls er da ist, sich aber nicht lesen
/// lässt, der Fehler (ein fehlender Ordner ist kein Fehler: er entsteht beim ersten Installieren).
struct Folder {
    path: String,
    problem: Option<String>,
}

fn folder_state(plugins: &Plugins) -> Folder {
    let dir = &plugins.host.places().user;
    let problem = match std::fs::read_dir(dir) {
        Ok(_) => None,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => Some(e.to_string()),
    };
    Folder {
        path: display_path(dir),
        problem,
    }
}

/// „Aus Datei…“, „Repo-Link“ und „Holen“ (nicht, solange gewählt, gelesen, geholt oder der
/// Dialog gezeigt wird; beim Holen „Wird geholt…“ und „Abbrechen“).
fn show_toolbar(
    ui: &mut egui::Ui,
    plugins: &Plugins,
    choosing: bool,
    link: &mut String,
    actions: &mut Vec<Action>,
) {
    let busy =
        choosing || plugins.is_reading() || plugins.is_fetching() || plugins.install.is_some();
    let enabled = ui.is_enabled();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(enabled && !busy, egui::Button::new(install_plugin_label()))
            .clicked()
        {
            actions.push(Action::Install);
        }
        ui.add_space(theme::SPACE_2);
        if plugins.is_fetching() {
            ui.label(tr!("plugin-install-fetching"));
            if ui
                .add_enabled(enabled, egui::Button::new(tr!("common-cancel")))
                .clicked()
            {
                actions.push(Action::CancelFetch);
            }
        } else {
            let field = theme::text_field_enabled(
                ui,
                enabled && !busy,
                egui::TextEdit::singleline(link)
                    .hint_text(tr!("plugin-install-link"))
                    .desired_width(LINK_FIELD_W),
            );
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let fetch = ui.add_enabled(
                enabled && !busy && !link.trim().is_empty(),
                egui::Button::new(tr!("plugin-install-fetch")),
            );
            if fetch.clicked() || (entered && !busy && !link.trim().is_empty()) {
                actions.push(Action::Fetch);
            }
        }
        if plugins.is_reading() {
            ui.label(tr!("plugin-settings-reading"));
        }
    });
}

/// Hinweis ([`Plugins::note`]), ein unlesbarer Ordner und der Ordner der installierten Plug-ins.
fn show_notes(ui: &mut egui::Ui, plugins: &Plugins, folder: &Folder) {
    let warning = theme::palette(ui.visuals().dark_mode).warning;
    if let Some(note) = &plugins.note {
        ui.add(egui::Label::new(egui::RichText::new(note.as_str()).color(warning)).wrap());
    }
    if let Some(problem) = &folder.problem {
        ui.add(
            egui::Label::new(
                egui::RichText::new(tr!("addins-folder-unreadable", error = problem.as_str()))
                    .color(warning),
            )
            .wrap(),
        );
    }
    ui.add(
        egui::Label::new(
            egui::RichText::new(tr!("addins-folder", path = folder.path.as_str())).weak(),
        )
        .wrap(),
    );
}

/// Der angezeigte Fundort eines Eintrags (der aktive, sonst der erste).
fn shown(e: &PluginEntry) -> Option<usize> {
    e.active.or((!e.found.is_empty()).then_some(0))
}

/// Zustandstext eines Eintrags (kaputte Ordner mit ihrem Problem).
fn entry_state_text(e: &PluginEntry) -> String {
    match shown(e).map(|i| (&e.found[i].problem, e.active)) {
        Some((Some(problem), None)) => tr!("plugin-state-broken", problem = problem.as_str()),
        _ => state_text(&e.state),
    }
}

/// Links die Liste: je Plug-in Schalter, Name (wählt) und Zustand; leer „Keine Plug-ins
/// gefunden.“
fn show_list(
    ui: &mut egui::Ui,
    plugins: &Plugins,
    selected: Option<&str>,
    actions: &mut Vec<Action>,
    toggles: &mut BTreeMap<String, egui::Rect>,
) {
    let dark = ui.visuals().dark_mode;
    let states = plugins.host.plugin_states();
    ui.allocate_ui_with_layout(
        egui::vec2(LIST_W, BODY_H),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(LIST_W);
            theme::section_header(ui, &list_title(), theme::SUB_HEADER_H);
            ui.add_space(theme::SPACE_1);
            let entries = plugins.host.entries();
            if entries.is_empty() {
                ui.weak(tr!("plugin-settings-none"));
                return;
            }
            let list = egui::ScrollArea::vertical()
                .id_salt("planora_addins_list")
                .max_height(BODY_H - theme::SUB_HEADER_H)
                .auto_shrink([false, true]);
            theme::scroll_area(ui, list, |ui| {
                for e in entries {
                    if shown(e).is_none() {
                        continue;
                    }
                    let enabled = states.get(&e.id).is_none_or(|s| s.enabled);
                    ui.horizontal(|ui| {
                        let mut on = enabled;
                        let toggle = ui
                            .add_enabled(
                                e.state != EntryState::Stopped,
                                egui::Checkbox::without_text(&mut on),
                            )
                            .on_hover_text(tr!("plugin-settings-enabled-hint"));
                        toggles.insert(e.id.clone(), toggle.rect);
                        if toggle.changed() {
                            actions.push(Action::Enable(e.id.clone(), on));
                        }
                        let is_selected = selected == Some(e.id.as_str());
                        let name = egui::Button::selectable(is_selected, entry_name(e))
                            .min_size(egui::vec2(ui.available_width() * 0.6, 0.0));
                        if ui.add(name).clicked() {
                            actions.push(Action::Select(e.id.clone()));
                        }
                        ui.label(
                            egui::RichText::new(entry_state_text(e))
                                .color(theme::palette(dark).text_secondary),
                        );
                    });
                }
            });
        },
    );
}

/// Rechts die Details des gewählten Plug-ins, Rechte mit Stand und „Zurücknehmen“ gleich zu
/// sehen.
fn show_details(
    ui: &mut egui::Ui,
    plugins: &Plugins,
    selected: Option<&str>,
    actions: &mut Vec<Action>,
) {
    let dark = ui.visuals().dark_mode;
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(width, BODY_H),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(width);
            let entry = selected.and_then(|id| plugins.host.entries().iter().find(|e| e.id == id));
            let Some((e, i)) = entry.and_then(|e| shown(e).map(|i| (e, i))) else {
                theme::section_header(ui, &title(), theme::SUB_HEADER_H);
                ui.add_space(theme::SPACE_1);
                ui.weak(tr!("addins-details-empty"));
                return;
            };
            let f = &e.found[i];
            let m = &f.manifest;
            theme::section_header(ui, &entry_name(e), theme::SUB_HEADER_H);
            ui.add_space(theme::SPACE_1);
            let details = egui::ScrollArea::vertical()
                .id_salt("planora_addins_details")
                .max_height(BODY_H - theme::SUB_HEADER_H)
                .auto_shrink([false, true]);
            theme::scroll_area(ui, details, |ui| {
                let version = m.version.to_string();
                ui.horizontal(|ui| {
                    ui.label(version.as_str());
                    ui.weak(origin_text(f.origin));
                    ui.label(entry_state_text(e));
                });
                ui.label(tr!("plugin-settings-author", author = m.author.as_str()));
                ui.label(tr!("plugin-settings-license", license = m.license.as_str()));
                if !license_ok(&m.license) {
                    ui.colored_label(theme::palette(dark).warning, tr!("plugin-license-unknown"));
                }
                if !m.description.is_empty() {
                    ui.add(egui::Label::new(m.description.as_str()).wrap());
                }
                ui.add_space(theme::SPACE_2);
                if m.rights.is_empty() {
                    ui.label(tr!("plugin-settings-no-rights"));
                } else {
                    ui.strong(tr!("plugin-settings-rights"));
                    for &r in &m.rights {
                        let grant = e.rights.state(r);
                        ui.horizontal(|ui| {
                            ui.label(right_name(r, &m.net_hosts));
                            ui.weak(grant_text(grant));
                            if ui
                                .add_enabled(grant.is_some(), egui::Button::new(revoke_label()))
                                .on_hover_text(tr!("plugin-right-revoke-hint"))
                                .clicked()
                            {
                                actions.push(Action::Revoke(e.id.clone(), r));
                            }
                        });
                    }
                }
                if let Some(source) = plugins.source_link(&e.id) {
                    ui.add_space(theme::SPACE_2);
                    ui.add(
                        egui::Label::new(tr!("plugin-install-source", source = source.web_url()))
                            .wrap(),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !plugins.is_checking(),
                                egui::Button::new(check_updates_label()),
                            )
                            .clicked()
                        {
                            actions.push(Action::CheckUpdates);
                        }
                        if let Some(u) = plugins.found_update(&e.id)
                            && ui
                                .button(update_to_label(&u.to.version.to_string()))
                                .clicked()
                        {
                            actions.push(Action::OpenUpdate(e.id.clone()));
                        }
                    });
                }
                if let Some(active) = e.active {
                    for (j, g) in e.found.iter().enumerate() {
                        if j != active {
                            ui.weak(shadowed_text(
                                g.origin,
                                &g.manifest.version.to_string(),
                                &version,
                            ));
                        }
                    }
                }
                if e.found.iter().any(|g| g.origin == Origin::User) {
                    ui.add_space(theme::SPACE_2);
                    if ui.button(remove_plugin_label()).clicked() {
                        actions.push(Action::Remove(e.id.clone()));
                    }
                }
            });
        },
    );
}
