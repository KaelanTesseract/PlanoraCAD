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
//!
//! Oben zwei Reiter: „Installiert“ (die Ansicht oben) und „Katalog“ (Plug-in-Katalog, siehe
//! [`crate::plugin_catalog`]): Suchfeld und Kategorie, links die Liste mit Symbol, Name und
//! Kategorie, rechts die Details mit „Installieren“, „Installiert“ bzw. „Aktualisieren auf …“.
//! Der Wechsel auf „Katalog“ holt den Katalog (nicht öfter als dessen Wartezeit erlaubt).

use crate::dialog::{Dialog, DialogSize, close_label};
use crate::file_chooser::{self, ChooserAction, FileChooser, display_path};
use crate::local_time::unix_time_text;
use crate::places::Places;
use crate::plugin_catalog::{CatalogState, StaleReason};
use crate::plugin_prompts::{grant_text, origin_text, right_name, state_text};
use crate::plugins::{self as app_plugins, Plugins, entry_name};
use crate::theme;
use crate::update_check::now_unix;
use crate::workspace::{Prompt, Workspace};
use planora_i18n::tr;
use planora_plugin_host::licenses::license_ok;
use planora_plugin_host::manifest::Right;
use planora_plugin_host::places::Origin;
use planora_plugin_host::{EntryState, PluginEntry};
use planora_update::catalog::{CatalogEntry, MAX_SYMBOL_BYTES};
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

/// Die Reiter des Fensters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AddinsTab {
    /// Die installierten Plug-ins (Liste und Details).
    #[default]
    Installed,
    /// Der Plug-in-Katalog.
    Catalog,
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
    /// Reiter gewechselt.
    Tab(AddinsTab),
    /// Eintrag des Katalogs gewählt.
    CatalogSelect(String),
    /// „Installieren“ eines Katalogeintrags (Kennung).
    CatalogInstall(String),
    /// „Erneut versuchen“: den Katalog sofort neu holen.
    CatalogRetry,
}

/// Zustand des Fensters (gehört der App wie die Einstellungen).
#[derive(Default)]
pub struct AddinsWindow {
    open: bool,
    /// Das gewählte Plug-in (Kennung); fehlt es, gilt das erste der Liste.
    selected: Option<String>,
    /// Dateiauswahl für `*.pcplugin`, solange gewählt wird.
    chooser: Option<FileChooser>,
    /// Der gezeigte Reiter.
    tab: AddinsTab,
    /// Der Katalog wurde seit dem Öffnen bzw. Wechseln des Reiters schon angefordert.
    catalog_requested: bool,
    /// Suchtext im Katalog.
    search: String,
    /// Gewählte Kategorie im Katalog (Schlüssel, siehe [`AddinsWindow::categories`]); `None` ist
    /// „Alle“.
    category: Option<String>,
    /// Der gewählte Eintrag im Katalog (Kennung); fehlt er in der Liste, gilt der erste.
    catalog_selected: Option<String>,
    /// Die gelesenen Symbole des Katalogs (Texturen), je Stand des Katalogs neu aufgebaut.
    symbols: SymbolCache,
    /// Rechteck des Fensters im letzten Bild (Layouttests).
    pub last_rect: Option<egui::Rect>,
    /// Rechteck des Schalters je Plug-in im letzten Bild (Tests klicken darauf).
    pub last_toggles: BTreeMap<String, egui::Rect>,
}

impl AddinsWindow {
    /// Öffnet das Fenster; eine offene Auswahl geht zu.
    pub fn open(&mut self) {
        self.open_at(AddinsTab::Installed);
    }

    /// Öffnet das Fenster auf dem Reiter `tab`; „Katalog“ holt den Katalog.
    pub fn open_at(&mut self, tab: AddinsTab) {
        self.open = true;
        self.chooser = None;
        self.tab = tab;
        self.catalog_requested = false;
    }

    /// Der gezeigte Reiter.
    pub fn tab(&self) -> AddinsTab {
        self.tab
    }

    /// Setzt den Suchtext des Katalogs.
    pub fn set_search(&mut self, text: &str) {
        self.search = text.to_string();
    }

    /// Wählt die Kategorie des Katalogs (`None`: „Alle“; `Some("")`: „Sonstiges“).
    pub fn set_category(&mut self, category: Option<&str>) {
        self.category = category.map(str::to_string);
    }

    /// Wählt den Eintrag `id` im Katalog.
    pub fn select_catalog(&mut self, id: &str) {
        self.catalog_selected = Some(id.to_string());
    }

    /// Die Kategorien, die im Katalog vorkommen (in der Reihenfolge des ersten Auftretens); ein
    /// leerer Schlüssel steht für „Sonstiges“ (keine oder unbekannte Kategorie).
    pub fn categories(&self, plugins: &Plugins) -> Vec<String> {
        match plugins.catalog.state() {
            CatalogState::Ready { entries, .. } => category_keys(entries)
                .into_iter()
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Die gewählte Kategorie (`None`: „Alle“).
    pub fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }

    /// Die Textur des Symbols `name` im Zwischenspeicher des Fensters (Tests).
    pub fn symbol_texture_id(&self, name: &str) -> Option<egui::TextureId> {
        self.symbols
            .map
            .get(name)
            .and_then(|(_, t)| t.as_ref().map(|t| t.id()))
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
        if self.tab == AddinsTab::Catalog && !self.catalog_requested {
            self.catalog_requested = true;
            let fetch = ws.plugins.repo_fetch;
            ws.plugins.catalog.request(fetch, now_unix(), false);
        }
        let locked = ws.prompt.is_some();
        let (mut open, mut close) = (true, false);
        // Eine gewählte Kategorie, die der geladene Katalog nicht (mehr) hat, wird zu „Alle“.
        if let (Some(c), CatalogState::Ready { entries, .. }) =
            (&self.category, ws.plugins.catalog.state())
            && !category_keys(entries).contains(&c.as_str())
        {
            self.category = None;
        }
        let tab = self.tab;
        let mut symbols = std::mem::take(&mut self.symbols);
        let mut search = self.search.clone();
        let mut category = self.category.clone();
        let catalog_selected = self.catalog_selected.clone();
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
                        show_tabs(ui, tab, &mut actions);
                        ui.add_space(theme::SPACE_2);
                        if tab == AddinsTab::Catalog {
                            let mut view = CatalogView {
                                search: &mut search,
                                category: &mut category,
                                symbols: &mut symbols,
                                selected: catalog_selected.as_deref(),
                            };
                            show_catalog(ui, &ws.plugins, &mut view, &mut actions);
                            return;
                        }
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
        self.symbols = symbols;
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
        self.search = search;
        self.category = category;
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
            Action::Tab(tab) => {
                self.tab = tab;
                self.catalog_requested = false;
                // Eine offene Dateiauswahl gehört zum Reiter „Installiert“.
                self.chooser = None;
            }
            Action::CatalogSelect(id) => self.catalog_selected = Some(id),
            Action::CatalogInstall(id) => {
                let entry = match ws.plugins.catalog.state() {
                    CatalogState::Ready { entries, .. } => {
                        entries.iter().find(|e| e.id == id).cloned()
                    }
                    _ => None,
                };
                if let Some(entry) = entry {
                    ws.plugins.install_from_catalog(&entry);
                }
            }
            Action::CatalogRetry => {
                let fetch = ws.plugins.repo_fetch;
                ws.plugins.catalog.request(fetch, now_unix(), true);
            }
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

/// Der Hinweis [`Plugins::note`] (Fehler beim Holen und Installieren), falls es einen gibt.
fn show_note(ui: &mut egui::Ui, plugins: &Plugins) {
    let warning = theme::palette(ui.visuals().dark_mode).warning;
    if let Some(note) = &plugins.note {
        ui.add(egui::Label::new(egui::RichText::new(note.as_str()).color(warning)).wrap());
    }
}

/// Hinweis ([`Plugins::note`]), ein unlesbarer Ordner und der Ordner der installierten Plug-ins.
fn show_notes(ui: &mut egui::Ui, plugins: &Plugins, folder: &Folder) {
    let warning = theme::palette(ui.visuals().dark_mode).warning;
    show_note(ui, plugins);
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
                    if f.provenance.as_ref().is_some_and(|p| p.catalog) {
                        ui.weak(tr!("addins-from-catalog"));
                    }
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

// ---------------------------------------------------------------------------------------------
// Reiter „Katalog“
// ---------------------------------------------------------------------------------------------

/// Kantenlänge des Symbols in der Liste (Punkte).
const SYMBOL_SIZE: f32 = 24.0;
/// Größte Kantenlänge eines Symbols in Pixeln (es wird mit 24 Punkten gezeigt); größere Bilder
/// gelten als unbrauchbar (Standardsymbol), so bleibt der Speicher je Symbol klein.
pub const SYMBOL_MAX_PIXELS: u32 = 256;
/// Die Kategorien, für die das Programm eine Übersetzung kennt (alles andere ist „Sonstiges“).
const KNOWN_CATEGORIES: [&str; 4] = ["elektronik", "import-export", "werkzeuge", "beispiele"];
/// Breite des Suchfelds.
const SEARCH_FIELD_W: f32 = 320.0;

/// Der Schlüssel einer Kategorie: sie selbst, wenn sie bekannt ist, sonst leer („Sonstiges“).
fn category_key(category: &str) -> &str {
    KNOWN_CATEGORIES
        .iter()
        .find(|k| **k == category)
        .copied()
        .unwrap_or("")
}

/// Die Kategorie-Schlüssel der Einträge in der Reihenfolge des ersten Auftretens (ohne
/// Wiederholung); der leere Schlüssel steht für „Sonstiges“.
fn category_keys(entries: &[CatalogEntry]) -> Vec<&str> {
    let mut keys: Vec<&str> = Vec::new();
    for e in entries {
        let key = category_key(&e.category);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

/// Anzeigetext einer Kategorie (nach [`category_key`]).
fn category_text(key: &str) -> String {
    match key {
        "elektronik" => tr!("catalog-category-elektronik"),
        "import-export" => tr!("catalog-category-import-export"),
        "werkzeuge" => tr!("catalog-category-werkzeuge"),
        "beispiele" => tr!("catalog-category-beispiele"),
        _ => tr!("catalog-category-other"),
    }
}

/// Der Name eines Katalogeintrags in `lang`; ohne Namen die Kennung.
fn catalog_name(e: &CatalogEntry, lang: &str) -> String {
    let name = e.name_in(lang).trim();
    if name.is_empty() {
        e.id.clone()
    } else {
        name.to_string()
    }
}

/// Passt der Eintrag zu Suchtext (Name und Beschreibung, ohne Groß/Klein) und Kategorie?
fn matches(e: &CatalogEntry, lang: &str, search: &str, category: Option<&str>) -> bool {
    if category.is_some_and(|c| c != category_key(&e.category)) {
        return false;
    }
    let needle = search.trim().to_lowercase();
    needle.is_empty()
        || catalog_name(e, lang).to_lowercase().contains(&needle)
        || e.description_in(lang).to_lowercase().contains(&needle)
}

/// Was der Katalog-Reiter vom Fenster braucht: Suchtext und Kategorie (veränderbar) und die Wahl.
struct CatalogView<'a> {
    search: &'a mut String,
    category: &'a mut Option<String>,
    symbols: &'a mut SymbolCache,
    selected: Option<&'a str>,
}

/// Die Texturen der Symbole eines Stands des Katalogs: Name → (Hash der Bytes, Textur oder
/// `None` für ein unlesbares Symbol). Ändert sich der Stand (`fetched_at`), wird alles neu
/// aufgebaut, die alten Texturen fallen weg.
#[derive(Default)]
struct SymbolCache {
    stamp: Option<u64>,
    map: BTreeMap<String, (u64, Option<egui::TextureHandle>)>,
}

impl SymbolCache {
    fn sync(&mut self, stamp: u64) {
        if self.stamp != Some(stamp) {
            self.map.clear();
            self.stamp = Some(stamp);
        }
    }
}

/// Die Reiter „Installiert“ und „Katalog“.
fn show_tabs(ui: &mut egui::Ui, tab: AddinsTab, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        for (this, label) in [
            (AddinsTab::Installed, tr!("addins-tab-installed")),
            (AddinsTab::Catalog, tr!("addins-tab-catalog")),
        ] {
            if ui
                .add(egui::Button::selectable(tab == this, label))
                .clicked()
                && tab != this
            {
                actions.push(Action::Tab(this));
            }
        }
    });
}

/// „Erneut versuchen“: holt den Katalog sofort neu (auch innerhalb der Wartezeit); gesperrt,
/// solange er lädt.
fn retry_button(ui: &mut egui::Ui, plugins: &Plugins, enabled: bool, actions: &mut Vec<Action>) {
    if ui
        .add_enabled(
            enabled && !plugins.catalog.is_loading(),
            egui::Button::new(tr!("catalog-retry")),
        )
        .clicked()
    {
        actions.push(Action::CatalogRetry);
    }
}

/// Der Reiter „Katalog“: Suche und Kategorie, Hinweise zum Stand, darunter je nach Zustand die
/// Liste mit den Details oder die Meldung.
fn show_catalog(
    ui: &mut egui::Ui,
    plugins: &Plugins,
    view: &mut CatalogView<'_>,
    actions: &mut Vec<Action>,
) {
    let dark = ui.visuals().dark_mode;
    let warning = theme::palette(dark).warning;
    let enabled = ui.is_enabled();
    show_note(ui, plugins);
    let state = plugins.catalog.state();
    if matches!(state, CatalogState::Ready { .. }) {
        show_filters(ui, plugins, view, enabled);
    }
    // Hinweise: Stand (offline oder fehlerhaft) und „wird geladen“.
    if let CatalogState::Ready {
        fetched_at,
        stale: Some(reason),
        ..
    } = state
    {
        let date = unix_time_text(*fetched_at as i64);
        let text = match reason {
            StaleReason::Offline => tr!("catalog-offline-since", datum = date.as_str()),
            StaleReason::Invalid => format!(
                "{} {}",
                reason.text(),
                tr!("catalog-state-date", datum = date.as_str())
            ),
        };
        ui.add(egui::Label::new(egui::RichText::new(text).color(warning)).wrap());
        retry_button(ui, plugins, enabled, actions);
    }
    if plugins.catalog.is_loading() {
        ui.weak(tr!("catalog-loading"));
    }
    ui.add_space(theme::SPACE_1);
    match state {
        CatalogState::Never => {
            if !plugins.catalog.is_loading() {
                ui.weak(tr!("catalog-loading"));
            }
        }
        CatalogState::Failed(text) => {
            ui.colored_label(warning, tr!("catalog-failed"));
            ui.add(egui::Label::new(text.as_str()).wrap());
            retry_button(ui, plugins, enabled, actions);
        }
        CatalogState::TooNew(_) => {
            ui.colored_label(warning, tr!("catalog-too-new"));
        }
        CatalogState::Ready {
            entries,
            symbols,
            fetched_at,
            ..
        } => {
            view.symbols.sync(*fetched_at);
            let lang = planora_i18n::current().id().to_string();
            let shown: Vec<&CatalogEntry> = entries
                .iter()
                .filter(|e| matches(e, &lang, view.search, view.category.as_deref()))
                .collect();
            let current = view
                .selected
                .and_then(|id| shown.iter().find(|e| e.id == id))
                .or(shown.first())
                .copied();
            ui.horizontal_top(|ui| {
                show_catalog_list(
                    ui,
                    &shown,
                    (symbols, view.symbols),
                    current.map(|e| e.id.as_str()),
                    entries.is_empty(),
                    &lang,
                    actions,
                );
                ui.add_space(theme::SPACE_4);
                show_catalog_details(ui, plugins, current, &lang, actions);
            });
        }
    }
}

/// Suchfeld und Auswahl der Kategorie.
fn show_filters(ui: &mut egui::Ui, plugins: &Plugins, view: &mut CatalogView<'_>, enabled: bool) {
    let keys = match plugins.catalog.state() {
        CatalogState::Ready { entries, .. } => category_keys(entries),
        _ => Vec::new(),
    };
    ui.horizontal(|ui| {
        theme::text_field_enabled(
            ui,
            enabled,
            egui::TextEdit::singleline(view.search)
                .hint_text(tr!("catalog-search-hint"))
                .desired_width(SEARCH_FIELD_W),
        );
        ui.add_space(theme::SPACE_2);
        ui.label(tr!("catalog-category-label"));
        let current = match view.category.as_deref() {
            Some(key) => category_text(key),
            None => tr!("catalog-category-all"),
        };
        ui.add_enabled_ui(enabled, |ui| {
            egui::ComboBox::from_id_salt("planora_catalog_category")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(view.category.is_none(), tr!("catalog-category-all"))
                        .clicked()
                    {
                        *view.category = None;
                    }
                    for key in keys {
                        let on = view.category.as_deref() == Some(key);
                        if ui.selectable_label(on, category_text(key)).clicked() {
                            *view.category = Some(key.to_string());
                        }
                    }
                });
        });
    });
}

/// Links die Liste der Katalogeinträge: Symbol, Name (wählt) und Kategorie.
fn show_catalog_list(
    ui: &mut egui::Ui,
    shown: &[&CatalogEntry],
    symbols: (&BTreeMap<String, Vec<u8>>, &mut SymbolCache),
    current: Option<&str>,
    catalog_empty: bool,
    lang: &str,
    actions: &mut Vec<Action>,
) {
    let dark = ui.visuals().dark_mode;
    ui.allocate_ui_with_layout(
        egui::vec2(LIST_W, BODY_H),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(LIST_W);
            theme::section_header(ui, &list_title(), theme::SUB_HEADER_H);
            ui.add_space(theme::SPACE_1);
            if shown.is_empty() {
                ui.weak(if catalog_empty {
                    tr!("catalog-empty")
                } else {
                    tr!("catalog-no-match")
                });
                return;
            }
            let list = egui::ScrollArea::vertical()
                .id_salt("planora_catalog_list")
                .max_height(BODY_H - theme::SUB_HEADER_H)
                .auto_shrink([false, true]);
            theme::scroll_area(ui, list, |ui| {
                for e in shown {
                    ui.horizontal(|ui| {
                        show_symbol(ui, e, symbols.0, symbols.1);
                        let name = egui::Button::selectable(
                            current == Some(e.id.as_str()),
                            catalog_name(e, lang),
                        )
                        .min_size(egui::vec2(ui.available_width() * 0.6, 0.0));
                        if ui.add(name).clicked() {
                            actions.push(Action::CatalogSelect(e.id.clone()));
                        }
                        ui.label(
                            egui::RichText::new(category_text(category_key(&e.category)))
                                .color(theme::palette(dark).text_secondary),
                        );
                    });
                }
            });
        },
    );
}

/// Rechts die Details des gewählten Katalogeintrags und der Knopf je nach Stand.
fn show_catalog_details(
    ui: &mut egui::Ui,
    plugins: &Plugins,
    current: Option<&CatalogEntry>,
    lang: &str,
    actions: &mut Vec<Action>,
) {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(width, BODY_H),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(width);
            let Some(e) = current else {
                theme::section_header(ui, &title(), theme::SUB_HEADER_H);
                ui.add_space(theme::SPACE_1);
                ui.weak(tr!("catalog-details-empty"));
                return;
            };
            theme::section_header(ui, &catalog_name(e, lang), theme::SUB_HEADER_H);
            ui.add_space(theme::SPACE_1);
            let details = egui::ScrollArea::vertical()
                .id_salt("planora_catalog_details")
                .max_height(BODY_H - theme::SUB_HEADER_H)
                .auto_shrink([false, true]);
            theme::scroll_area(ui, details, |ui| {
                let description = e.description_in(lang);
                if !description.is_empty() {
                    ui.add(egui::Label::new(description).wrap());
                }
                ui.add_space(theme::SPACE_2);
                ui.label(tr!(
                    "catalog-detail-category",
                    category = category_text(category_key(&e.category))
                ));
                ui.add(
                    egui::Label::new(tr!("catalog-detail-source", source = e.repo.web_url()))
                        .wrap(),
                );
                let update = plugins.found_update(&e.id);
                // Ohne Update ist die installierte Version die neueste bekannte; nicht installiert „–“.
                let installed_version = plugins
                    .host
                    .entries()
                    .iter()
                    .find(|p| p.id == e.id)
                    .and_then(|p| shown(p).map(|i| p.found[i].manifest.version.to_string()));
                let latest = update
                    .map(|u| u.to.version.to_string())
                    .or(installed_version)
                    .unwrap_or_else(|| tr!("catalog-version-unknown"));
                ui.label(tr!("catalog-detail-latest", version = latest));
                ui.add(egui::Label::new(tr!("catalog-rights-later")).wrap());
                ui.add_space(theme::SPACE_2);
                let installed = plugins
                    .host
                    .entries()
                    .iter()
                    .any(|p| p.id == e.id && !p.found.is_empty());
                let busy =
                    plugins.is_fetching() || plugins.is_reading() || plugins.install.is_some();
                let enabled = ui.is_enabled();
                ui.horizontal(|ui| {
                    if installed && let Some(u) = update {
                        let label = update_to_label(&u.to.version.to_string());
                        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                            actions.push(Action::OpenUpdate(e.id.clone()));
                        }
                    } else if installed {
                        ui.add_enabled(false, egui::Button::new(tr!("catalog-installed")));
                    } else {
                        let install = egui::Button::new(tr!("catalog-install"));
                        if ui.add_enabled(enabled && !busy, install).clicked() {
                            actions.push(Action::CatalogInstall(e.id.clone()));
                        }
                        if plugins.is_fetching() {
                            ui.label(tr!("plugin-install-fetching"));
                            let cancel = egui::Button::new(tr!("common-cancel"));
                            if ui.add_enabled(enabled, cancel).clicked() {
                                actions.push(Action::CancelFetch);
                            }
                        }
                    }
                });
            });
        },
    );
}

/// Das Symbol eines Eintrags (24 px) oder, wenn es fehlt oder sich nicht lesen lässt, das
/// Standardsymbol.
fn show_symbol(
    ui: &mut egui::Ui,
    e: &CatalogEntry,
    symbols: &BTreeMap<String, Vec<u8>>,
    cache: &mut SymbolCache,
) {
    let texture = e
        .symbol
        .as_deref()
        .and_then(|name| symbols.get(name).map(|bytes| (name, bytes)))
        .and_then(|(name, bytes)| symbol_texture(ui.ctx(), cache, name, bytes));
    match texture {
        Some(t) => {
            ui.add(egui::Image::new(&t).fit_to_exact_size(egui::vec2(SYMBOL_SIZE, SYMBOL_SIZE)));
        }
        None => default_symbol(ui),
    }
}

/// Das Standardsymbol: ein Baustein (Rechteck mit Zapfen), in der Textfarbe.
fn default_symbol(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(SYMBOL_SIZE, SYMBOL_SIZE), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let color = theme::palette(ui.visuals().dark_mode).text_secondary;
    let body = rect.shrink2(egui::vec2(3.0, 5.0));
    ui.painter().rect_stroke(
        body,
        3.0,
        egui::Stroke::new(1.5, color),
        egui::StrokeKind::Inside,
    );
    let knob = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, body.top() - 1.0),
        egui::vec2(8.0, 4.0),
    );
    ui.painter().rect_filled(knob, 1.0, color);
}

/// Hash des Inhalts (erkennt ein geändertes Symbol gleicher Länge unter demselben Namen).
fn content_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Die Textur eines Symbols, einmal je Stand des Katalogs gelesen und in `cache` gemerkt; `None`,
/// wenn sich die Bytes nicht als PNG lesen lassen (auch SVG: ohne Rasterer im Programm, dafür
/// steht das Standardsymbol) oder das Bild zu groß ist.
fn symbol_texture(
    ctx: &egui::Context,
    cache: &mut SymbolCache,
    name: &str,
    bytes: &[u8],
) -> Option<egui::TextureHandle> {
    let hash = content_hash(bytes);
    if let Some((cached, texture)) = cache.map.get(name)
        && *cached == hash
    {
        return texture.clone();
    }
    let texture = if bytes.len() as u64 > MAX_SYMBOL_BYTES {
        None
    } else {
        decode_symbol(bytes).map(|image| {
            ctx.load_texture(
                format!("katalog-symbol-{name}"),
                image,
                egui::TextureOptions::LINEAR,
            )
        })
    };
    cache.map.insert(name.to_string(), (hash, texture.clone()));
    texture
}

/// Liest ein PNG; Bilder über [`SYMBOL_MAX_PIXELS`] Kantenlänge gelten als unbrauchbar (die
/// Größe steht im Kopf der Datei, so wird nichts Riesiges entpackt).
pub fn decode_symbol(bytes: &[u8]) -> Option<egui::ColorImage> {
    const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() < 24 || !bytes.starts_with(PNG_MAGIC) {
        return None;
    }
    let dim =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let (w, h) = (dim(16), dim(20));
    if w == 0 || h == 0 || w > SYMBOL_MAX_PIXELS || h > SYMBOL_MAX_PIXELS {
        return None;
    }
    let icon = crate::logo::decode(bytes)?;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    ))
}
