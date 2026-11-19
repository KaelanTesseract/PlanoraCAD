//! Plug-ins im Programm (Plug-ins Etappe 1, Task 5): verteilt die Ereignisse des
//! [`PluginHost`] auf das Programm. Meldungen kommen in die Statuszeile des aktiven Tabs
//! („<Plug-in-Name>: <Text>“), Protokolleinträge ins Programm-Protokoll, Rechte-Nachfragen
//! warten in einer Reihe (immer nur eine offen, [`crate::plugin_prompts::show_ask`]),
//! Netzanfragen laufen als [`Job`] im Hintergrund, ausgefallene Plug-ins zeigen die Fehlerleiste.
//! Dazu das Untermenü Datei → „Zusatzmodule“ mit den Befehlen aller laufenden Plug-ins.
//!
//! Kein Aufruf hier wartet: der Host kehrt sofort zurück, das Netz läuft nur in Aufträgen.
//! Die Dialoge der Plug-ins ([`Plugins::dialogs`]) zeichnet [`crate::plugin_dialog`]; ihre
//! Ereignisse gehen hier an den Host.
//!
//! Task 7: Installieren, Entfernen und die Verwaltung in den Einstellungen. Ein Paket wird in
//! einem [`Job`] gelesen (bis 64 MiB samt Entpacken, das soll das Bild nicht anhalten) und dann
//! als [`InstallOffer`] im Installationsdialog angeboten; der Dialog liest selbst nichts, so kann
//! ein zweiter Weg (Repo-Link) ihm ebenso ein fertiges Paket geben ([`Plugins::offer_install`]).
//!
//! Task 8: Installieren und Aktualisieren über den Repo-Link. „Holen“ lädt Release-Liste und
//! Paket in einem [`Job`] ([`crate::plugin_repo`]) und zeigt dann denselben Installationsdialog;
//! installiert wird erst mit „Installieren“, samt `herkunft.json`. Plug-ins mit Herkunft (nur vom
//! Nutzer installierte, nie mitgelieferte) werden auf Updates geprüft ([`Plugins::check_updates`],
//! beim Start täglich über [`Plugins::auto_check`]); ein gefundenes Update wird gleich geladen,
//! damit der Aktualisieren-Dialog die neu verlangten Rechte zeigen kann.

use crate::app::MenuRequest;
use crate::diag;
use crate::jobs::{Job, Poll};
use crate::plugin_dialog;
pub use crate::plugin_dialog::OpenPluginDialog;
use crate::plugin_prompts::{self, BannerAction, BannerLine, UpdateChoice, UpdateView};
pub use crate::plugin_prompts::{InstallChoice, InstallOffer};
use crate::plugin_repo::{self, Fetched};
pub use crate::plugin_repo::{RepoFetch, check_downloaded};
use crate::settings_window::SettingsSection;
use crate::theme;
use crate::workspace::Workspace;
use planora_i18n::tr;
use planora_plugin_host::manifest::Right;
use planora_plugin_host::package::{MAX_PACKAGE, Package, PackageError};
use planora_plugin_host::places::{self as plugin_places, Origin, Places, Provenance};
use planora_plugin_host::rights::{Answer, Rights};
use planora_plugin_host::runtime::{DialogEvent, HttpResponse, Level, RequestResult};
use planora_plugin_host::{EntryState, HostEvent, PluginEntry, PluginHost, PluginState};
use planora_project::config::{Config, PluginConfig};
use planora_update::Channel;
use planora_update::check::due;
use planora_update::plugin_source::{PluginRelease, RepoLink, parse_repo_link};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
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

/// Der echte Netzweg für Repo-Links: [`planora_update::plugin_source::download`] (jede
/// Weiterleitung geprüft, höchstens fünf, je Anfrage 30 s).
pub fn default_repo_fetch(url: &str, max: u64) -> Result<(u16, Vec<u8>), String> {
    planora_update::plugin_source::download(url, max)
}

/// Ein gefundenes Update eines Plug-ins mit Repo-Link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginUpdate {
    pub id: String,
    /// Die laufende Version.
    pub from: semver::Version,
    pub to: PluginRelease,
    /// Rechte, die das neue Paket zusätzlich verlangt; erst bekannt, wenn das Paket geladen ist
    /// (vorher leer).
    pub new_rights: BTreeSet<Right>,
}

/// Der Update-Kanal aus der Konfiguration (unbekannt: stabil).
fn channel_of(config: &Config) -> Channel {
    Channel::from_id(&config.update_channel).unwrap_or(Channel::Stable)
}

/// Ein Auftrag „Nach Updates suchen“ eines Plug-ins.
type CheckJob = (String, Job<Result<Option<PluginRelease>, String>>);
/// Ein Auftrag „Paket des Updates laden“ eines Plug-ins.
type DownloadJob = (String, Job<Result<Package, String>>);

/// Eine wartende Rechte-Nachfrage ([`HostEvent::Ask`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptRequest {
    pub plugin: String,
    pub generation: u64,
    pub request: u64,
    pub right: Right,
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
    /// Offene Dialoge der Plug-ins (öffnen, ersetzen, schließen; gezeichnet in [`Plugins::frame`]).
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
    /// Das Paket im Installationsdialog (gezeichnet in [`Plugins::frame`]).
    pub install: Option<InstallOffer>,
    /// Ein Paket wird gerade gelesen ([`Plugins::read_package`]): Paket und Pfad oder ein Fehler.
    reading: Option<Job<Result<(Package, PathBuf), String>>>,
    /// Hinweis im Abschnitt „Plug-ins“ der Einstellungen (Lesen, Installieren, Entfernen oder
    /// Speichern misslang); die nächste Handlung dort leert ihn.
    pub note: Option<String>,
    /// Der Netzweg für Repo-Links (Vorgabe [`default_repo_fetch`]; Tests setzen eine Nachbildung).
    pub repo_fetch: RepoFetch,
    /// Text im Feld „Repo-Link“ (Abschnitt „Plug-ins“).
    pub link_text: String,
    /// Update-Kanal (aus `config.update_channel`, je Bild in [`Plugins::frame`] übernommen).
    pub channel: Channel,
    /// „Holen“ läuft ([`Plugins::install_from_link`]).
    fetching: Option<Job<Result<Fetched, String>>>,
    /// Laufende Prüfungen auf Updates und Downloads gefundener Updates.
    checks: Vec<CheckJob>,
    downloads: Vec<DownloadJob>,
    /// Die laufende Prüfung kam vom Nutzer: Fehler und „kein Update“ in [`Plugins::note`].
    manual_check: bool,
    updates: Vec<PluginUpdate>,
    /// Geladene, geprüfte Pakete der Updates (Kennung → Paket).
    update_packages: BTreeMap<String, Package>,
    /// Kennung des Plug-ins im Aktualisieren-Dialog.
    pub update_dialog: Option<String>,
    /// Der Hinweis „Für N Plug-ins gibt es Updates.“ ist zu zeigen (bis „Anzeigen“ oder
    /// „Schließen“).
    notice: bool,
    /// „Anzeigen“ gedrückt: das Programm öffnet den Abschnitt „Plug-ins“
    /// ([`Plugins::take_show_section`]).
    show_section: bool,
    /// Herkunft der vom Nutzer installierten Plug-ins mit `herkunft.json` (Kennung → Herkunft).
    provenance: BTreeMap<String, Provenance>,
    /// `config` wurde außerhalb von [`Plugins::frame`] geändert (Zeitpunkt der Prüfung): beim
    /// nächsten Bild speichern.
    config_dirty: bool,
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
                    author: c.author.clone(),
                    net_hosts: c.net_hosts.clone(),
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
                    author: s.author,
                    net_hosts: s.net_hosts,
                },
            )
        })
        .collect();
}

/// Liest das Paket `path` (im Arbeitsthread eines [`Job`]); ein zu großes wird gar nicht erst
/// gelesen.
fn read_package_file(path: &Path) -> Result<Package, String> {
    let failed = |e: String| tr!("plugin-package-read-failed", error = e);
    let size = std::fs::metadata(path)
        .map_err(|e| failed(e.to_string()))?
        .len();
    if size > MAX_PACKAGE {
        return Err(failed(PackageError::TooLarge.to_string()));
    }
    let data = std::fs::read(path).map_err(|e| failed(e.to_string()))?;
    Package::read(&data).map_err(|e| failed(e.to_string()))
}

/// Fehlertext, wenn ein Auftrag zum Repo-Link verloren ging.
fn aborted() -> String {
    tr!(
        "plugin-link-network",
        detail = tr!("plugin-package-read-aborted")
    )
}

/// Der Anzeigename eines Plug-ins (aus dem Manifest des aktiven Fundorts, sonst die Kennung).
pub fn entry_name(e: &PluginEntry) -> String {
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
        let mut plugins = Plugins::with_host(host);
        plugins.channel = channel_of(config);
        plugins
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
        let mut plugins = Plugins {
            host,
            prompts: VecDeque::new(),
            dialogs: Vec::new(),
            net_jobs: Vec::new(),
            banners: Vec::new(),
            fetch: default_fetch,
            messages: Vec::new(),
            generations: BTreeMap::new(),
            install: None,
            reading: None,
            note: None,
            repo_fetch: default_repo_fetch,
            link_text: String::new(),
            channel: Channel::Stable,
            fetching: None,
            checks: Vec::new(),
            downloads: Vec::new(),
            manual_check: false,
            updates: Vec::new(),
            update_packages: BTreeMap::new(),
            update_dialog: None,
            notice: false,
            show_section: false,
            provenance: BTreeMap::new(),
            config_dirty: false,
        };
        plugins.refresh_provenance();
        plugins
    }

    /// Liest `herkunft.json` der vom Nutzer installierten Plug-ins neu (nach jedem Neusuchen).
    fn refresh_provenance(&mut self) {
        self.provenance = self
            .host
            .entries()
            .iter()
            .filter_map(|e| {
                let user = e.found.iter().find(|f| f.origin == Origin::User)?;
                Some((e.id.clone(), plugin_places::read_provenance(&user.dir)?))
            })
            .collect();
    }

    /// Der Repo-Link des vom Nutzer installierten Plug-ins `id` (aus `herkunft.json`).
    pub fn source_link(&self, id: &str) -> Option<RepoLink> {
        parse_repo_link(&self.provenance.get(id)?.link).ok()
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
                    .find(|d| d.plugin == plugin && d.spec.id == dialog.id)
                {
                    // Gleiche id: neue Felder, Werte gleicher Feld-id bleiben.
                    Some(open) => open.reopen(dialog),
                    None => self.dialogs.push(OpenPluginDialog::new(plugin, dialog)),
                }
            }
            HostEvent::CloseDialog { plugin, id } => {
                self.dialogs
                    .retain(|d| !(d.plugin == plugin && d.spec.id == id));
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

    /// Liest das Paket `path` im Hintergrund; ist es gelesen, erscheint der Installationsdialog
    /// (ein Fehler steht in [`Plugins::note`]). Ein noch laufendes Lesen wird verworfen.
    pub fn read_package(&mut self, path: PathBuf) {
        self.note = None;
        self.install = None;
        self.reading = Some(Job::spawn(move |_| {
            read_package_file(&path).map(|pkg| (pkg, path))
        }));
    }

    /// Wird gerade ein Paket gelesen?
    pub fn is_reading(&self) -> bool {
        self.reading.is_some()
    }

    /// Bietet ein fertig gelesenes Paket im Installationsdialog an; `source` ist die Herkunft für
    /// die Anzeige (Pfad der Datei, später der Repo-Link). Gibt es die Kennung schon, fragt der
    /// Dialog nach dem Ersetzen (mit der Version des laufenden, sonst des ersten Fundorts).
    /// Je verlangtem Recht steht im Dialog, was nach dem Installieren gilt
    /// ([`Rights::carry_over`] gegen das laufende Manifest; neu installiert: nichts).
    pub fn offer_install(&mut self, package: Package, source: Option<String>) {
        let existing = self
            .entry(&package.manifest.id)
            .filter(|e| !e.found.is_empty());
        let replaces = existing.map(|e| {
            let i = e.active.unwrap_or(0);
            e.found[i].manifest.version.to_string()
        });
        let carried = existing.map_or_else(Rights::default, |e| {
            e.rights
                .carry_over(e.active.map(|a| &e.found[a].manifest), &package.manifest)
        });
        let grants = package
            .manifest
            .rights
            .iter()
            .map(|&r| (r, carried.state(r)))
            .collect();
        self.install = Some(InstallOffer {
            package,
            source,
            replaces,
            grants,
            provenance: None,
        });
    }

    /// Installiert das angebotene Paket für den Nutzer und sucht die Fundorte neu
    /// ([`PluginHost::rescan`] mit dem Stand aus `config`): ein neues Plug-in startet ohne
    /// Neustart. Gibt es die Kennung noch nicht, beginnt es ohne Rechte (ein alter Eintrag in
    /// `config.plugins` fällt weg); ersetzt es eine vorhandene, bleiben Rechte nur nach
    /// [`Rights::carry_over`], und es startet neu, auch bei gleicher Version
    /// ([`PluginHost::replaced`]). `false` (Hinweis in [`Plugins::note`]), wenn es misslang;
    /// ohne Angebot nichts.
    /// Über den Repo-Link geholt, kommt `herkunft.json` dazu. Danach steht der Stand gleich in
    /// `config.plugins` (keine veralteten Rechte, auch nicht für ein Bild).
    pub fn install_offered(&mut self, config: &mut Config) -> bool {
        let Some(offer) = self.install.take() else {
            return false;
        };
        self.install_package(&offer.package, offer.provenance.as_ref(), config)
    }

    /// Gemeinsamer Teil von [`Plugins::install_offered`] und [`Plugins::apply_update`].
    fn install_package(
        &mut self,
        package: &Package,
        provenance: Option<&Provenance>,
        config: &mut Config,
    ) -> bool {
        self.note = None;
        let id = package.manifest.id.clone();
        let existing = self.entry(&id).filter(|e| !e.found.is_empty());
        let old = existing.and_then(|e| e.active.map(|a| e.found[a].manifest.clone()));
        let fresh = existing.is_none();
        if let Err(e) = plugin_places::install(self.host.places(), package, provenance) {
            self.note = Some(tr!("plugin-install-failed", error = e.to_string()));
            return false;
        }
        if fresh {
            config.plugins.remove(&id);
            self.host.forget_state(&id);
        }
        self.forget(&id);
        self.host.rescan(&states_from_config(config));
        if !fresh {
            self.host.replaced(&id, old.as_ref());
        }
        write_states(&self.host, config);
        self.drop_update(&id);
        self.refresh_provenance();
        true
    }

    /// Entfernt das vom Nutzer installierte Plug-in `id` mit seinem ganzen gespeicherten Stand
    /// (`config.plugins[id]`, Rechte und Schalter) und sucht die Fundorte neu; ein
    /// mitgeliefertes derselben Kennung läuft danach ohne Rechte. `false` (Hinweis in
    /// [`Plugins::note`]), wenn es misslang.
    pub fn remove(&mut self, id: &str, config: &mut Config) -> bool {
        self.note = None;
        if let Err(e) = plugin_places::uninstall(self.host.places(), id) {
            self.note = Some(tr!("plugin-remove-failed", error = e.to_string()));
            return false;
        }
        config.plugins.remove(id);
        self.host.forget_state(id);
        self.forget(id);
        self.banners.retain(|b| b.plugin != id);
        self.host.rescan(&states_from_config(config));
        self.drop_update(id);
        self.refresh_provenance();
        true
    }

    /// „Holen“: prüft den Link (ein ungültiger steht gleich in [`Plugins::note`]) und holt im
    /// Hintergrund das neueste Release im Kanal samt Paket; dann erscheint der
    /// Installationsdialog (Quelle: der Link), ein Fehler steht in [`Plugins::note`]. Ein noch
    /// laufendes Holen wird verworfen.
    pub fn install_from_link(&mut self, text: &str) {
        self.note = None;
        self.install = None;
        self.fetching = None;
        let link = match parse_repo_link(text) {
            Ok(l) => l,
            Err(e) => {
                self.note = Some(e);
                return;
            }
        };
        let (fetch, channel) = (self.repo_fetch, self.channel);
        self.fetching = Some(Job::spawn(move |_| {
            plugin_repo::fetch_for_install(fetch, link, channel)
        }));
    }

    /// Läuft „Holen“?
    pub fn is_fetching(&self) -> bool {
        self.fetching.is_some()
    }

    /// „Abbrechen“ beim Holen: das Ergebnis wird verworfen.
    pub fn cancel_fetch(&mut self) {
        self.fetching = None;
    }

    /// Holt das Ergebnis von „Holen“ ab.
    fn poll_fetching(&mut self) {
        let Some(job) = &self.fetching else {
            return;
        };
        let result = match job.poll() {
            Poll::Pending => return,
            Poll::Ready(r) => r,
            Poll::Lost => Err(aborted()),
        };
        self.fetching = None;
        match result {
            Ok(f) => {
                let link = f.link.web_url();
                self.offer_install(f.package, Some(link.clone()));
                if let Some(offer) = &mut self.install {
                    offer.provenance = Some(Provenance {
                        link,
                        tag: f.release.tag,
                    });
                }
            }
            Err(e) => self.note = Some(e),
        }
    }

    /// Sucht für jedes vom Nutzer installierte Plug-in mit `herkunft.json` im Hintergrund nach
    /// einem Update (mitgelieferte nie). `manual`: vom Nutzer gewünscht, dann stehen Fehler und
    /// „Kein Update gefunden.“ in [`Plugins::note`] (sonst nur im Protokoll).
    pub fn check_updates(&mut self, manual: bool) {
        if manual {
            self.note = None;
            self.manual_check = true;
        }
        let ids: Vec<String> = self.provenance.keys().cloned().collect();
        for id in ids {
            self.check_one(&id);
        }
    }

    /// Startet die Prüfung für `id` (nicht, wenn für `id` schon eine läuft).
    fn check_one(&mut self, id: &str) {
        let running =
            self.checks.iter().any(|(i, _)| i == id) || self.downloads.iter().any(|(i, _)| i == id);
        if running {
            return;
        }
        let Some(link) = self.source_link(id) else {
            return;
        };
        let Some(installed) = self.installed_version(id) else {
            return;
        };
        let (fetch, channel) = (self.repo_fetch, self.channel);
        let job =
            Job::spawn(move |_| plugin_repo::check_release(fetch, &link, &installed, channel));
        self.checks.push((id.to_string(), job));
    }

    /// Die laufende Version von `id`.
    fn installed_version(&self, id: &str) -> Option<semver::Version> {
        let e = self.entry(id)?;
        e.active.map(|a| e.found[a].manifest.version.clone())
    }

    /// Läuft eine Prüfung auf Updates oder der Download eines Updates?
    pub fn is_checking(&self) -> bool {
        !self.checks.is_empty() || !self.downloads.is_empty()
    }

    /// Die gefundenen Updates (auch solche, deren Paket noch geladen wird).
    pub fn updates(&self) -> &[PluginUpdate] {
        &self.updates
    }

    /// Das Update von `id`, wenn sein Paket geladen und geprüft ist (dann lässt es sich
    /// einspielen).
    pub fn ready_update(&self, id: &str) -> Option<&PluginUpdate> {
        self.update_packages.get(id)?;
        self.updates.iter().find(|u| u.id == id)
    }

    /// Öffnet den Aktualisieren-Dialog für `id` (nur mit geladenem Paket).
    pub fn open_update(&mut self, id: &str) {
        if self.ready_update(id).is_some() {
            self.update_dialog = Some(id.to_string());
        }
    }

    /// Spielt das geladene Update von `id` ein (wie „Installieren“ beim Ersetzen: gleicher Autor
    /// behält die Rechte nach [`Rights::carry_over`], neue Rechte werden beim ersten Gebrauch
    /// gefragt; es startet neu) und schreibt den Stand nach `config.plugins`. `false` ohne
    /// geladenes Update oder wenn es misslang (Hinweis in [`Plugins::note`]).
    pub fn apply_update(&mut self, id: &str, config: &mut Config) -> bool {
        self.update_dialog = None;
        let Some(link) = self.provenance.get(id).map(|p| p.link.clone()) else {
            return false;
        };
        let Some(update) = self.ready_update(id).cloned() else {
            return false;
        };
        let Some(package) = self.update_packages.remove(id) else {
            return false;
        };
        let provenance = Provenance {
            link,
            tag: update.to.tag,
        };
        self.install_package(&package, Some(&provenance), config)
    }

    /// Prüft beim Start, wenn `config.update_check` an ist und die letzte Prüfung mindestens
    /// einen Tag her ist ([`due`]), alle Plug-ins mit Link auf Updates und merkt den Zeitpunkt in
    /// `config.last_plugin_check` (gespeichert mit dem nächsten Bild). `true`, wenn geprüft wird.
    pub fn auto_check(&mut self, config: &mut Config, now: u64) -> bool {
        if self.provenance.is_empty()
            || !due(now, config.last_plugin_check, config.update_check, false)
        {
            return false;
        }
        self.channel = channel_of(config);
        self.check_updates(false);
        config.last_plugin_check = Some(now);
        self.config_dirty = true;
        true
    }

    /// „Anzeigen“ im Hinweis auf Updates wurde gedrückt (einmal abholen).
    pub fn take_show_section(&mut self) -> bool {
        std::mem::take(&mut self.show_section)
    }

    /// Vergisst ein Update von `id` (eingespielt, entfernt, keines mehr oder Fehler).
    fn drop_update(&mut self, id: &str) {
        self.updates.retain(|u| u.id != id);
        self.update_packages.remove(id);
        if self.update_dialog.as_deref() == Some(id) {
            self.update_dialog = None;
        }
    }

    /// Ein Fehler einer Prüfung: vom Nutzer gewünscht in [`Plugins::note`], sonst ins Protokoll.
    fn report(&mut self, id: &str, e: String) {
        if self.manual_check {
            self.note = Some(e);
        } else {
            diag::warn(&format!("Plug-in {id}: update check failed: {e}"));
        }
    }

    /// Holt fertige Prüfungen und Downloads ab; ein gefundenes Update wird gleich geladen.
    /// `true`, wenn etwas fertig wurde.
    fn poll_checks(&mut self) -> bool {
        let mut acted = false;
        let mut i = 0;
        while i < self.checks.len() {
            let result = match self.checks[i].1.poll() {
                Poll::Pending => {
                    i += 1;
                    continue;
                }
                Poll::Ready(r) => r,
                Poll::Lost => Err(aborted()),
            };
            acted = true;
            let (id, _) = self.checks.remove(i);
            self.drop_update(&id);
            match result {
                Ok(Some(release)) => {
                    let Some(from) = self.installed_version(&id) else {
                        continue;
                    };
                    self.updates.push(PluginUpdate {
                        id: id.clone(),
                        from,
                        to: release.clone(),
                        new_rights: BTreeSet::new(),
                    });
                    let fetch = self.repo_fetch;
                    let installed = id.clone();
                    let job = Job::spawn(move |_| {
                        plugin_repo::fetch_package(fetch, &release, Some(&installed))
                    });
                    self.downloads.push((id, job));
                }
                Ok(None) => {}
                Err(e) => self.report(&id, e),
            }
        }
        let mut i = 0;
        while i < self.downloads.len() {
            let result = match self.downloads[i].1.poll() {
                Poll::Pending => {
                    i += 1;
                    continue;
                }
                Poll::Ready(r) => r,
                Poll::Lost => Err(aborted()),
            };
            acted = true;
            let (id, _) = self.downloads.remove(i);
            match result {
                Ok(pkg) => {
                    let old: BTreeSet<Right> = self
                        .entry(&id)
                        .and_then(|e| e.active.map(|a| e.found[a].manifest.rights.clone()))
                        .unwrap_or_default();
                    if let Some(u) = self.updates.iter_mut().find(|u| u.id == id) {
                        u.new_rights = pkg.manifest.rights.difference(&old).copied().collect();
                        self.update_packages.insert(id, pkg);
                        self.notice = true;
                    }
                }
                Err(e) => {
                    self.drop_update(&id);
                    self.report(&id, e);
                }
            }
        }
        if self.manual_check && !self.is_checking() {
            self.manual_check = false;
            if self.updates.is_empty() && self.note.is_none() {
                self.note = Some(tr!("plugin-check-none"));
            }
        }
        acted
    }

    /// Was der Aktualisieren-Dialog für `id` zeigt (ohne geladenes Update nichts).
    fn update_view(&self, id: &str) -> Option<UpdateView> {
        let u = self.ready_update(id)?;
        let pkg = self.update_packages.get(id)?;
        Some(UpdateView {
            name: self.plugin_name(id),
            from: u.from.to_string(),
            to: u.to.version.to_string(),
            notes: u.to.notes.clone(),
            new_rights: u.new_rights.iter().copied().collect(),
            hosts: pkg.manifest.net_hosts.clone(),
        })
    }

    /// Holt ein fertig gelesenes Paket ab und bietet es an (Fehler in [`Plugins::note`]).
    fn poll_reading(&mut self) {
        let Some(job) = &self.reading else {
            return;
        };
        let result = match job.poll() {
            Poll::Pending => return,
            Poll::Ready(r) => r,
            Poll::Lost => Err(tr!(
                "plugin-package-read-failed",
                error = tr!("plugin-package-read-aborted")
            )),
        };
        self.reading = None;
        match result {
            Ok((pkg, path)) => self.offer_install(pkg, Some(path.display().to_string())),
            Err(e) => self.note = Some(e),
        }
    }

    /// Die Meldungen für die Statuszeile seit dem letzten Abholen.
    pub fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }

    /// Zeichnet die offenen Dialoge (Texte in der aktuellen Sprache aus den Sprachdateien ihres
    /// Plug-ins) und meldet ihre Ereignisse an den Host; nach `ok` und `cancel` ist der Dialog
    /// zu, bevor das Plug-in davon erfährt. `true`, wenn ein Ereignis gemeldet wurde.
    fn show_dialogs(&mut self, ctx: &egui::Context) -> bool {
        let language = planora_i18n::current().id().to_string();
        let mut events = Vec::new();
        for d in &mut self.dialogs {
            let host = &self.host;
            let plugin = d.plugin.clone();
            let texts = |key: &str| host.text(&plugin, &language, key);
            if let Some(ev) = plugin_dialog::show(ctx, d, &texts) {
                events.push((d.plugin.clone(), d.spec.id.clone(), ev));
            }
        }
        let acted = !events.is_empty();
        for (plugin, id, ev) in events {
            if matches!(ev, DialogEvent::Ok(_) | DialogEvent::Cancel) {
                self.dialogs
                    .retain(|d| !(d.plugin == plugin && d.spec.id == id));
            }
            // Fällt das Plug-in dabei aus, räumt das `Failed` beim nächsten `poll` seine
            // übrigen Dialoge weg.
            self.host.dialog_event(&plugin, &id, ev);
        }
        acted
    }

    /// Ein Bild: [`Plugins::update`], dann die vorderste Nachfrage, die offenen Dialoge und
    /// die Fehlerleiste.
    /// `true`, wenn sich `config.plugins` geändert hat.
    /// `prompt_open`: eine Rückfrage des Arbeitsbereichs ist offen, „Installieren“ ist solange
    /// gesperrt (eine wartende „Entfernen“-Rückfrage träfe sonst das gerade installierte).
    pub fn frame(&mut self, ctx: &egui::Context, config: &mut Config, prompt_open: bool) -> bool {
        self.channel = channel_of(config);
        let mut changed = self.update(config) | std::mem::take(&mut self.config_dirty);
        let mut acted = changed || !self.messages.is_empty();
        let was_reading = self.is_reading();
        self.poll_reading();
        acted |= was_reading && !self.is_reading();
        let was_fetching = self.is_fetching();
        self.poll_fetching();
        acted |= was_fetching && !self.is_fetching();
        acted |= self.poll_checks();
        if let Some(offer) = &self.install
            && let Some(choice) = plugin_prompts::show_install(ctx, offer, prompt_open)
        {
            match choice {
                InstallChoice::Install => changed |= self.install_offered(config),
                InstallChoice::Cancel => self.install = None,
            }
            acted = true;
        }
        if let Some(id) = self.update_dialog.clone() {
            match self.update_view(&id) {
                None => self.update_dialog = None,
                Some(view) => {
                    if let Some(choice) = plugin_prompts::show_update(ctx, &view, prompt_open) {
                        match choice {
                            UpdateChoice::Update => changed |= self.apply_update(&id, config),
                            UpdateChoice::Cancel => self.update_dialog = None,
                        }
                        acted = true;
                    }
                }
            }
        }
        if let Some(p) = self.prompts.front() {
            let name = self.plugin_name(&p.plugin);
            let hosts = self.net_hosts(&p.plugin);
            if let Some(a) = plugin_prompts::show_ask(ctx, &name, p.right, &hosts) {
                self.answer(a);
                acted = true;
            }
        }
        // Nach der Nachfrage: deren Esc (Ablehnen) bricht nicht zugleich einen Dialog ab.
        acted |= self.show_dialogs(ctx);
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
        let ready = if self.notice {
            self.update_packages.len()
        } else {
            0
        };
        match plugin_prompts::show_banners(ctx, &lines, ready) {
            Some(BannerAction::Restart(id)) => {
                self.restart(&id);
                acted = true;
            }
            Some(BannerAction::Close(id)) => self.banners.retain(|b| b.plugin != id),
            Some(BannerAction::ShowUpdates) => {
                self.notice = false;
                self.show_section = true;
                acted = true;
            }
            Some(BannerAction::CloseUpdates) => self.notice = false,
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
        if loading
            || !self.net_jobs.is_empty()
            || self.is_reading()
            || self.is_fetching()
            || self.is_checking()
        {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        // Eine Antwort in diesem Bild ändert `config.plugins` erst nach dem nächsten `poll`; nach
        // dem Installieren wird gleich gespeichert.
        changed
    }

    /// Untermenü Datei → „Zusatzmodule“: „Verwalten …“ (öffnet die Einstellungen am Abschnitt
    /// Plug-ins), dann die Befehle aller laufenden Plug-ins nach Plug-in-Name; ein Plug-in mit
    /// mehr als einem Befehl bekommt ein Untermenü. Ein gewählter Befehl läuft sofort.
    /// `manage`: „Verwalten …“ ist wählbar (nicht bei offener Rückfrage, wie „Einstellungen“).
    pub fn addins_menu(&mut self, ui: &mut egui::Ui, manage: bool) -> MenuRequest {
        let mut request = MenuRequest::None;
        let addins = egui::Button::new(tr!("menu-file-addins-manage"));
        if theme::entry(ui, manage, addins)
            .on_disabled_hover_text(tr!("menu-answer-prompt-first"))
            .clicked()
        {
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
    let prompt_open = ws.prompt.is_some();
    if ws.plugins.frame(ctx, &mut ws.config, prompt_open) {
        ws.plugins_pending = true;
        ws.try_persist_config();
    }
    if let Some(last) = ws.plugins.take_messages().pop() {
        ws.active_tab_mut().state.message = Some(last);
    }
}

/// Speichert den Stand der Plug-ins gleich (nach Schalter, Zurücknehmen, Entfernen in den
/// Einstellungen): `config.plugins` aus dem Host, vormerken, schreiben. Misslingt das Schreiben,
/// steht ein Hinweis in [`Plugins::note`] (der Stand gilt für die Sitzung und kommt mit dem
/// nächsten gelungenen Schreiben in die Datei).
pub fn persist(ws: &mut Workspace) -> bool {
    write_states(&ws.plugins.host, &mut ws.config);
    ws.plugins_pending = true;
    let written = ws.try_persist_config();
    if !written {
        ws.plugins.note = Some(tr!("plugin-settings-not-saved"));
    }
    written
}

/// Entfernt das vom Nutzer installierte Plug-in `id` (nach der Rückfrage „Plug-in X entfernen?“)
/// und speichert den Stand.
pub fn remove(ws: &mut Workspace, id: &str) {
    if ws.plugins.remove(id, &mut ws.config) {
        persist(ws);
    }
}
