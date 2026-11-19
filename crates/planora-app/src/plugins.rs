//! Plug-ins im Programm (Plug-ins Etappe 1, Task 5): verteilt die Ereignisse des
//! [`PluginHost`] auf das Programm. Meldungen kommen in die Statuszeile des aktiven Tabs
//! („<Plug-in-Name>: <Text>“), Protokolleinträge ins Programm-Protokoll, Rechte-Nachfragen
//! warten in einer Reihe (immer nur eine offen, [`crate::plugin_prompts::show_ask`]),
//! Netzanfragen laufen als [`Job`] im Hintergrund, ausgefallene Plug-ins zeigen die Fehlerleiste.
//! Dazu das Untermenü Datei → „Zusatzmodule“ mit den Befehlen aller laufenden Plug-ins.
//!
//! Kein Aufruf hier wartet: der Host kehrt sofort zurück, das Netz läuft nur in Aufträgen.
//! Dialoge der Plug-ins werden hier nur vermerkt ([`Plugins::dialogs`]); gezeichnet werden sie
//! ab Task 6.

use crate::app::MenuRequest;
use crate::diag;
use crate::jobs::{Job, Poll};
use crate::plugin_prompts::{self, BannerAction, BannerLine};
use crate::settings_window::SettingsSection;
use crate::theme;
use crate::workspace::Workspace;
use planora_i18n::tr;
use planora_plugin_host::manifest::Right;
use planora_plugin_host::places::Places;
use planora_plugin_host::rights::Answer;
use planora_plugin_host::runtime::{Dialog, HttpResponse, Level, RequestResult};
use planora_plugin_host::{EntryState, HostEvent, PluginEntry, PluginHost, PluginState};
use planora_project::config::{Config, PluginConfig};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

/// Größte Antwort einer Netzanfrage eines Plug-ins (Bytes); eine größere zählt als Fehler.
pub const NET_MAX_BYTES: u64 = 16 << 20;
/// Zeitgrenze einer ganzen Netzanfrage eines Plug-ins.
pub const NET_TIMEOUT: Duration = Duration::from_secs(30);

/// Ein GET mit Größen- und Zeitgrenze: (Status, Inhalt) oder ein Fehlertext. Läuft im
/// Arbeitsthread eines [`Job`]; Tests setzen in [`Plugins::fetch`] eine Nachbildung.
pub type NetFetch = fn(&str) -> Result<(u16, Vec<u8>), String>;

/// Der echte Netzweg: [`planora_update::get_limited`] mit den Grenzen für Plug-ins, ohne
/// Weiterleitungen, genau mit der Adresse des Plug-ins.
pub fn default_fetch(url: &str) -> Result<(u16, Vec<u8>), String> {
    planora_update::get_limited(url, NET_MAX_BYTES, NET_TIMEOUT)
}

/// Eine wartende Rechte-Nachfrage ([`HostEvent::Ask`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptRequest {
    pub plugin: String,
    pub generation: u64,
    pub request: u64,
    pub right: Right,
}

/// Ein offener Dialog eines Plug-ins (gezeichnet ab Task 6).
#[derive(Clone, Debug, PartialEq)]
pub struct OpenPluginDialog {
    pub plugin: String,
    pub dialog: Dialog,
}

/// Eine Zeile der Fehlerleiste: ein ausgefallenes Plug-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureBanner {
    pub plugin: String,
    /// Zu oft ausgefallen: bleibt bis zum Programmneustart aus (kein „Neu starten“).
    pub stopped: bool,
}

/// Ein laufender Netz-Auftrag: Plug-in, Generation, Anfrage und der Auftrag selbst.
pub type NetJob = (String, u64, u64, Job<Result<(u16, Vec<u8>), String>>);

/// Die Plug-ins des Programms und was sie gerade vom Programm wollen.
pub struct Plugins {
    pub host: PluginHost,
    /// Wartende Rechte-Nachfragen; gezeigt wird nur die vorderste.
    pub prompts: VecDeque<PromptRequest>,
    /// Offene Dialoge der Plug-ins (öffnen, ersetzen, schließen; gezeichnet ab Task 6).
    pub dialogs: Vec<OpenPluginDialog>,
    pub net_jobs: Vec<NetJob>,
    pub banners: Vec<FailureBanner>,
    /// Der Netzweg (Vorgabe [`default_fetch`]).
    pub fetch: NetFetch,
    /// Meldungen für die Statuszeile, noch nicht an einen Tab gegeben ([`show`]).
    messages: Vec<String>,
    /// Die neueste Generation je Plug-in aus seinen Ereignissen: ältere Nachfragen und Aufträge
    /// gehören zu einer beendeten Instanz.
    generations: BTreeMap<String, u64>,
}

impl Default for Plugins {
    fn default() -> Self {
        Plugins::empty()
    }
}

/// Der gespeicherte Zustand aller Plug-ins aus der Konfiguration.
pub fn states_from_config(config: &Config) -> BTreeMap<String, PluginState> {
    config
        .plugins
        .iter()
        .map(|(id, c)| {
            (
                id.clone(),
                PluginState {
                    enabled: c.enabled,
                    rights: c.rights.clone(),
                },
            )
        })
        .collect()
}

/// Schreibt den Zustand des Hosts nach `config.plugins` (Plug-ins mit Vorgabewerten fallen weg).
pub fn write_states(host: &PluginHost, config: &mut Config) {
    config.plugins = host
        .plugin_states()
        .into_iter()
        .map(|(id, s)| {
            (
                id,
                PluginConfig {
                    enabled: s.enabled,
                    rights: s.rights,
                },
            )
        })
        .collect();
}

/// Der Anzeigename eines Plug-ins (aus dem Manifest des aktiven Fundorts, sonst die Kennung).
fn entry_name(e: &PluginEntry) -> String {
    e.active
        .or(if e.found.is_empty() { None } else { Some(0) })
        .map_or_else(|| e.id.clone(), |a| e.found[a].manifest.name.clone())
}

impl Plugins {
    /// Plug-ins aus den Fundorten `places`; startet alle eingeschalteten im Hintergrund (mit der
    /// Sprache des Programms und dem Stand aus `config.plugins`).
    pub fn new(places: Places, config: &Config) -> Plugins {
        let mut host = PluginHost::new(places);
        host.set_language(planora_i18n::current().id());
        host.start_all(&states_from_config(config));
        Plugins::with_host(host)
    }

    /// Ohne Fundorte und ohne Plug-ins (Vorgabe des [`Workspace`], damit Tests nie die echten
    /// Ordner lesen).
    pub fn empty() -> Plugins {
        Plugins::with_host(PluginHost::new(Places {
            bundled: Vec::new(),
            user: PathBuf::new(),
            cache: PathBuf::new(),
        }))
    }

    fn with_host(host: PluginHost) -> Plugins {
        Plugins {
            host,
            prompts: VecDeque::new(),
            dialogs: Vec::new(),
            net_jobs: Vec::new(),
            banners: Vec::new(),
            fetch: default_fetch,
            messages: Vec::new(),
            generations: BTreeMap::new(),
        }
    }

    /// Der Anzeigename des Plug-ins `id` (die Kennung, wenn es unbekannt ist).
    pub fn plugin_name(&self, id: &str) -> String {
        self.entry(id).map_or_else(|| id.to_string(), entry_name)
    }

    fn entry(&self, id: &str) -> Option<&PluginEntry> {
        self.host.entries().iter().find(|e| e.id == id)
    }

    /// Die Rechner aus dem Manifest des Plug-ins `id` (für die Nachfrage zu `net`).
    fn net_hosts(&self, id: &str) -> Vec<String> {
        self.entry(id)
            .and_then(|e| e.active.map(|a| e.found[a].manifest.net_hosts.clone()))
            .unwrap_or_default()
    }

    /// Holt die Ereignisse des Hosts ab und verteilt sie, holt fertige Netz-Aufträge ab und
    /// räumt Nachfragen, Dialoge und Aufträge beendeter Instanzen weg. `true`, wenn sich
    /// `config.plugins` geändert hat (dann speichern, siehe [`show`]).
    pub fn update(&mut self, config: &mut Config) -> bool {
        let mut changed = false;
        for event in self.host.poll() {
            changed |= self.handle(event, config);
        }
        self.poll_net();
        self.prune();
        changed
    }

    fn handle(&mut self, event: HostEvent, config: &mut Config) -> bool {
        match event {
            HostEvent::Message { plugin, text, .. } => {
                let name = self.plugin_name(&plugin);
                self.messages
                    .push(tr!("plugin-message", plugin = name, text = text));
            }
            HostEvent::Log {
                plugin,
                level,
                text,
            } => {
                let line = format!("Plug-in {plugin}: {text}");
                match level {
                    Level::Info => diag::info(&line),
                    Level::Warning => diag::warn(&line),
                    Level::Error => diag::fehler(&line),
                }
            }
            HostEvent::OpenDialog { plugin, dialog } => {
                match self
                    .dialogs
                    .iter_mut()
                    .find(|d| d.plugin == plugin && d.dialog.id == dialog.id)
                {
                    Some(open) => open.dialog = dialog,
                    None => self.dialogs.push(OpenPluginDialog { plugin, dialog }),
                }
            }
            HostEvent::CloseDialog { plugin, id } => {
                self.dialogs
                    .retain(|d| !(d.plugin == plugin && d.dialog.id == id));
            }
            HostEvent::Ask {
                plugin,
                generation,
                request,
                right,
            } => {
                self.note_generation(&plugin, generation);
                self.prompts.push_back(PromptRequest {
                    plugin,
                    generation,
                    request,
                    right,
                });
            }
            HostEvent::Net {
                plugin,
                generation,
                request,
                url,
            } => {
                self.note_generation(&plugin, generation);
                let fetch = self.fetch;
                // Genau die Adresse des Plug-ins (vom Host gegen das Manifest geprüft).
                let job = Job::spawn(move |_| fetch(&url));
                self.net_jobs.push((plugin, generation, request, job));
            }
            HostEvent::Failed { plugin, reason } => {
                diag::warn(&format!("Plug-in {plugin} failed: {reason}"));
                self.forget(&plugin);
                let stopped = self
                    .entry(&plugin)
                    .is_some_and(|e| e.state == EntryState::Stopped);
                match self.banners.iter_mut().find(|b| b.plugin == plugin) {
                    Some(b) => b.stopped = stopped,
                    None => self.banners.push(FailureBanner { plugin, stopped }),
                }
            }
            HostEvent::Changed => {
                write_states(&self.host, config);
                return true;
            }
        }
        false
    }

    /// Merkt die neueste Generation eines Plug-ins; mit einer neuen fallen Nachfragen und
    /// Aufträge der alten weg.
    fn note_generation(&mut self, plugin: &str, generation: u64) {
        let newest = self.generations.entry(plugin.to_string()).or_insert(0);
        if generation > *newest {
            *newest = generation;
            self.prompts
                .retain(|p| p.plugin != plugin || p.generation >= generation);
            self.net_jobs
                .retain(|(p, g, _, _)| p != plugin || *g >= generation);
        }
    }

    /// Vergisst Nachfragen, Dialoge und Netz-Aufträge des Plug-ins `plugin` (seine Instanz ist
    /// fort). Ein fallengelassener Auftrag bricht ab.
    fn forget(&mut self, plugin: &str) {
        self.prompts.retain(|p| p.plugin != plugin);
        self.dialogs.retain(|d| d.plugin != plugin);
        self.net_jobs.retain(|(p, ..)| p != plugin);
    }

    /// Nachfragen und Dialoge gelten nur für laufende Plug-ins, Aufträge ebenso.
    fn prune(&mut self) {
        let running: Vec<String> = self
            .host
            .entries()
            .iter()
            .filter(|e| e.state == EntryState::Running)
            .map(|e| e.id.clone())
            .collect();
        let alive = |id: &str| running.iter().any(|r| r == id);
        self.prompts.retain(|p| alive(&p.plugin));
        self.dialogs.retain(|d| alive(&d.plugin));
        self.net_jobs.retain(|(p, ..)| alive(p));
    }

    /// Fertige Netz-Aufträge an den Host (eine Antwort mit jedem Statuscode ist `Http`).
    fn poll_net(&mut self) {
        let mut i = 0;
        while i < self.net_jobs.len() {
            let result = match self.net_jobs[i].3.poll() {
                Poll::Pending => {
                    i += 1;
                    continue;
                }
                Poll::Ready(Ok((status, body))) => {
                    RequestResult::Http(HttpResponse { status, body })
                }
                Poll::Ready(Err(e)) => RequestResult::Failed(e),
                Poll::Lost => RequestResult::Failed("network request aborted".to_string()),
            };
            let (plugin, generation, request, _) = self.net_jobs.remove(i);
            self.host.net_result(&plugin, generation, request, result);
        }
    }

    /// Beantwortet die vorderste Nachfrage (der Host verwirft eine Antwort an eine beendete
    /// Instanz still). Gespeichert wird nach dem `Changed` beim nächsten [`Plugins::update`].
    pub fn answer(&mut self, a: Answer) {
        if let Some(p) = self.prompts.pop_front() {
            self.host.answer(&p.plugin, p.generation, p.request, a);
        }
    }

    /// Startet ein Plug-in neu; seine Nachfragen, Dialoge, Aufträge und seine Zeile der
    /// Fehlerleiste fallen weg.
    pub fn restart(&mut self, id: &str) {
        self.forget(id);
        self.banners.retain(|b| b.plugin != id);
        self.host.restart(id);
    }

    /// Schaltet ein Plug-in ein oder aus; ausgeschaltet fallen seine Nachfragen, Dialoge und
    /// Aufträge weg. Gespeichert wird nach dem `Changed` beim nächsten [`Plugins::update`].
    pub fn set_enabled(&mut self, id: &str, on: bool) {
        if !on {
            self.forget(id);
        }
        self.host.set_enabled(id, on);
    }

    /// Die Meldungen für die Statuszeile seit dem letzten Abholen.
    pub fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }

    /// Ein Bild: [`Plugins::update`], dann die vorderste Nachfrage und die Fehlerleiste.
    /// `true`, wenn sich `config.plugins` geändert hat.
    pub fn frame(&mut self, ctx: &egui::Context, config: &mut Config) -> bool {
        let changed = self.update(config);
        let mut acted = changed || !self.messages.is_empty();
        if let Some(p) = self.prompts.front() {
            let name = self.plugin_name(&p.plugin);
            let hosts = self.net_hosts(&p.plugin);
            if let Some(a) = plugin_prompts::show_ask(ctx, &name, p.right, &hosts) {
                self.answer(a);
                acted = true;
            }
        }
        let names: Vec<String> = self
            .banners
            .iter()
            .map(|b| self.plugin_name(&b.plugin))
            .collect();
        let lines: Vec<BannerLine<'_>> = self
            .banners
            .iter()
            .zip(&names)
            .map(|(b, name)| BannerLine {
                id: &b.plugin,
                name,
                stopped: b.stopped,
            })
            .collect();
        match plugin_prompts::show_banners(ctx, &lines) {
            Some(BannerAction::Restart(id)) => {
                self.restart(&id);
                acted = true;
            }
            Some(BannerAction::Close(id)) => self.banners.retain(|b| b.plugin != id),
            None => {}
        }
        // Antworten und Ergebnisse erreichen das Plug-in erst beim nächsten `poll`.
        if acted {
            ctx.request_repaint();
        }
        let loading = self
            .host
            .entries()
            .iter()
            .any(|e| e.state == EntryState::Loading);
        if loading || !self.net_jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        // Eine Antwort in diesem Bild ändert `config.plugins` erst nach dem nächsten `poll`.
        changed
    }

    /// Untermenü Datei → „Zusatzmodule“: „Verwalten …“ (öffnet die Einstellungen am Abschnitt
    /// Plug-ins), dann die Befehle aller laufenden Plug-ins nach Plug-in-Name; ein Plug-in mit
    /// mehr als einem Befehl bekommt ein Untermenü. Ein gewählter Befehl läuft sofort.
    pub fn addins_menu(&mut self, ui: &mut egui::Ui) -> MenuRequest {
        let mut request = MenuRequest::None;
        let addins = egui::Button::new(tr!("menu-file-addins-manage"));
        if theme::entry(ui, true, addins).clicked() {
            request = MenuRequest::Settings(Some(SettingsSection::Plugins));
            ui.close();
        }
        let mut groups: BTreeMap<(String, String), Vec<(String, String)>> = BTreeMap::new();
        for c in self.host.commands() {
            groups
                .entry((self.plugin_name(&c.plugin), c.plugin))
                .or_default()
                .push((c.full_id, c.label));
        }
        if !groups.is_empty() {
            ui.separator();
        }
        let mut run = None;
        for ((name, _), commands) in &groups {
            let mut entry = |ui: &mut egui::Ui, (full_id, label): &(String, String)| {
                if theme::entry(ui, true, egui::Button::new(label)).clicked() {
                    run = Some(full_id.clone());
                    ui.close();
                }
            };
            if let [only] = commands.as_slice() {
                entry(ui, only);
            } else {
                ui.menu_button(name, |ui| {
                    for c in commands {
                        entry(ui, c);
                    }
                });
            }
        }
        if let Some(id) = run {
            self.host.run_command(&id);
        }
        request
    }
}

/// Ein Bild der Plug-ins im Programm ([`Plugins::frame`]): Meldungen in die Statuszeile des
/// aktiven Tabs (der Arbeitsbereich hat immer einen), geänderter Zustand wird gleich gespeichert
/// ([`Workspace::try_persist_config`]; misslingt das, später mit dem nächsten Schreiben).
pub fn show(ctx: &egui::Context, ws: &mut Workspace) {
    if ws.plugins.frame(ctx, &mut ws.config) {
        ws.plugins_pending = true;
        ws.try_persist_config();
    }
    if let Some(last) = ws.plugins.take_messages().pop() {
        ws.active_tab_mut().state.message = Some(last);
    }
}
