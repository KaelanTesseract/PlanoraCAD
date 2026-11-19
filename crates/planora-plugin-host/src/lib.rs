//! Plug-in-Schnittstelle (WebAssembly): Paketformat, Manifest, Rechte, Laufzeit, Fundorte und der
//! [`PluginHost`], der alle Plug-ins des Programms verwaltet.
//!
//! Der Host ruft Plug-ins nur auf dem aufrufenden (Oberflächen-)Thread auf; nur Übersetzen und
//! `init` laufen je Plug-in in einem Hintergrund-Thread. Was ein Plug-in vom Programm will,
//! landet in einem Postfach und kommt über [`PluginHost::poll`] als [`HostEvent`] heraus. Kein
//! Aufruf des Plug-ins wartet und keiner ruft in das Plug-in zurück: Ergebnisse (Rechte, Netz)
//! gehen erst beim nächsten `poll` als `on-result` an das Plug-in.

mod bindings;
pub mod manifest;
pub mod package;
pub mod places;
pub mod rights;
pub mod runtime;
pub mod texts;

use manifest::{Manifest, Right};
use places::{Found, Places};
use planora_i18n::tr;
use rights::{Answer, Grant, Pending, Requests, Rights, check_net_url};
use runtime::{
    Command, Dialog, DialogEvent, Engine, HostCalls, HostInfo, Instance, Level, PluginError,
    Registration, RequestResult,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use texts::PluginTexts;

/// Nach so vielen Ausfällen in einer Sitzung bleibt ein Plug-in bis zum Programmneustart aus.
const MAX_FAILURES: u8 = 3;
/// Mehr Ereignisse oder Ergebnisse als das nimmt das Postfach einer Instanz zwischen zwei
/// Abholungen nicht an (ein Plug-in könnte sonst in einer Schleife den Speicher füllen).
const MAX_QUEUE: usize = 1024;
/// Ergebnis einer Anfrage, die das volle Postfach nicht mehr annimmt.
fn queue_full() -> String {
    tr!("plugin-queue-full")
}

/// Der zu speichernde Zustand eines Plug-ins: eingeschaltet oder nicht und die dauerhaft
/// beantworteten Rechte (Kennung des Rechts → `"allowed"` oder `"denied"`). Das Programm bildet
/// ihn auf `planora_project::config::PluginConfig` ab; der Host kennt die Konfiguration nicht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginState {
    pub enabled: bool,
    pub rights: BTreeMap<String, String>,
}

impl Default for PluginState {
    fn default() -> Self {
        PluginState {
            enabled: true,
            rights: BTreeMap::new(),
        }
    }
}

/// Zustand eines Plug-ins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryState {
    /// Ausgeschaltet (oder noch nicht gestartet).
    Off,
    /// Wird im Hintergrund übersetzt und gestartet.
    Loading,
    /// Läuft; seine Befehle stehen in [`PluginHost::commands`].
    Running,
    /// Ausgefallen oder nicht startbar (Grund); „Neu starten“ ist möglich.
    Failed(String),
    /// Zu oft ausgefallen: bleibt bis zum Programmneustart aus.
    Stopped,
}

/// Ein Plug-in mit allen Fundorten seiner Kennung.
#[derive(Clone, Debug)]
pub struct PluginEntry {
    pub id: String,
    /// Alle Ordner mit dieser Kennung (die Verwaltung zeigt alle).
    pub found: Vec<Found>,
    /// Der Ordner, der läuft (Index in `found`); `None`, wenn keiner startbar ist.
    pub active: Option<usize>,
    pub state: EntryState,
    pub rights: Rights,
    /// Ausfälle in dieser Sitzung.
    pub failures: u8,
}

/// Ein Befehl eines laufenden Plug-ins (für das Menü).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginCommand {
    /// `"<plugin-id>.<befehl-id>"`.
    pub full_id: String,
    /// Die Beschriftung in der eingestellten Sprache.
    pub label: String,
    /// Die Kennung des Plug-ins.
    pub plugin: String,
}

/// Was das Programm für ein Plug-in tun soll.
#[derive(Clone, Debug, PartialEq)]
pub enum HostEvent {
    /// Eine Meldung an den Nutzer (auch ein Fehler, den das Plug-in selbst zurückgibt).
    Message {
        plugin: String,
        level: Level,
        text: String,
    },
    /// Ein Protokolleintrag des Plug-ins (`log.write`, stdout/stderr) fürs Programm-Protokoll.
    Log {
        plugin: String,
        level: Level,
        text: String,
    },
    /// Einen Dialog öffnen.
    OpenDialog { plugin: String, dialog: Dialog },
    /// Einen Dialog schließen.
    CloseDialog { plugin: String, id: String },
    /// Den Nutzer nach einem Recht fragen; die Antwort geht an [`PluginHost::answer`].
    Ask {
        plugin: String,
        generation: u64,
        request: u64,
        right: Right,
    },
    /// Eine Netzanfrage (GET) ausführen; das Ergebnis geht an [`PluginHost::net_result`].
    Net {
        plugin: String,
        generation: u64,
        request: u64,
        url: String,
    },
    /// Das Plug-in ist ausgefallen (je Ausfall genau einmal).
    Failed { plugin: String, reason: String },
    /// Etwas Gespeichertes hat sich geändert: [`PluginHost::plugin_states`] neu speichern.
    Changed,
}

/// Das Postfach einer Instanz: alles, was das Plug-in während eines Aufrufs verlangt. Die
/// Host-Aufrufe tragen hier nur ein und kehren sofort zurück.
struct Mailbox {
    plugin: String,
    generation: u64,
    manifest: Manifest,
    /// Abbild von `PluginEntry::rights` (vor jedem Aufruf erneuert).
    rights: Rights,
    requests: Requests,
    events: Vec<HostEvent>,
    /// Ergebnisse für `on-result`, beim nächsten `poll` zugestellt.
    deliveries: Vec<(u64, RequestResult)>,
    /// Die offene Nachfrage je Recht (höchstens eine).
    asks: BTreeMap<Right, Pending>,
    /// Anfragen, die auf die offene Nachfrage ihres Rechts warten.
    waiting: Vec<Pending>,
    /// Netzanfragen, deren Ergebnis noch aussteht.
    net_open: BTreeSet<u64>,
}

/// Das Recht, auf das eine offene Anfrage wartet.
fn pending_right(p: &Pending) -> Right {
    match p {
        Pending::AskRight { right, .. } => *right,
        Pending::Net { .. } => Right::Net,
    }
}

fn pending_request(p: &Pending) -> u64 {
    match p {
        Pending::AskRight { request, .. } | Pending::Net { request, .. } => *request,
    }
}

impl Mailbox {
    fn event(&mut self, e: HostEvent) {
        if self.events.len() < MAX_QUEUE {
            self.events.push(e);
        }
    }

    fn deliver(&mut self, request: u64, r: RequestResult) {
        if self.deliveries.len() < MAX_QUEUE {
            self.deliveries.push((request, r));
        }
    }

    /// Ist das Ereignis-Postfach voll?
    fn events_full(&self) -> bool {
        self.events.len() >= MAX_QUEUE
    }

    /// Eine Anfrage, die das Postfach nicht mehr annimmt: das Plug-in erfährt es über `on-result`,
    /// statt ewig zu warten. Es wird kein Zustand vermerkt, dessen Ereignis verloren ginge.
    fn overflow(&mut self, request: u64) {
        self.deliver(request, RequestResult::Failed(queue_full()));
    }

    fn net(&mut self, request: u64, url: String) {
        if self.events_full() || self.net_open.len() >= MAX_QUEUE {
            self.overflow(request);
            return;
        }
        self.net_open.insert(request);
        let e = HostEvent::Net {
            plugin: self.plugin.clone(),
            generation: self.generation,
            request,
            url,
        };
        self.event(e);
    }

    /// Fragt nach `right`, wenn nicht schon gefragt wird; sonst wartet `p` auf diese Antwort.
    fn ask(&mut self, p: Pending) {
        let right = pending_right(&p);
        let request = pending_request(&p);
        if self.asks.contains_key(&right) {
            if self.waiting.len() < MAX_QUEUE {
                self.waiting.push(p);
            } else {
                self.overflow(request);
            }
            return;
        }
        if self.events_full() {
            self.overflow(request);
            return;
        }
        let e = HostEvent::Ask {
            plugin: self.plugin.clone(),
            generation: self.generation,
            request,
            right,
        };
        self.asks.insert(right, p);
        self.event(e);
    }

    /// Erledigt eine Anfrage nach der Entscheidung für ihr Recht.
    fn resolve(&mut self, p: Pending, allowed: bool) {
        match p {
            Pending::AskRight { request, .. } => self.deliver(
                request,
                if allowed {
                    RequestResult::Granted
                } else {
                    RequestResult::Denied
                },
            ),
            Pending::Net { request, url } if allowed => self.net(request, url),
            Pending::Net { request, .. } => self.deliver(request, RequestResult::Denied),
        }
    }
}

fn lock(m: &Mutex<Mailbox>) -> MutexGuard<'_, Mailbox> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn right_from_wit(r: runtime::Right) -> Right {
    match r {
        runtime::Right::Net => Right::Net,
        runtime::Right::FsFolder => Right::FsFolder,
        runtime::Right::ModelRead => Right::ModelRead,
        runtime::Right::ModelWrite => Right::ModelWrite,
    }
}

/// Die Host-Aufrufe einer Instanz: tragen nur ins Postfach ein.
struct Recorder(Arc<Mutex<Mailbox>>);

impl HostCalls for Recorder {
    fn log(&mut self, level: Level, text: &str) {
        let mut mb = lock(&self.0);
        let plugin = mb.plugin.clone();
        mb.event(HostEvent::Log {
            plugin,
            level,
            text: text.to_string(),
        });
    }

    fn message(&mut self, level: Level, text: &str) {
        let mut mb = lock(&self.0);
        let plugin = mb.plugin.clone();
        mb.event(HostEvent::Message {
            plugin,
            level,
            text: text.to_string(),
        });
    }

    fn open_dialog(&mut self, d: Dialog) -> Result<(), String> {
        let mut mb = lock(&self.0);
        let plugin = mb.plugin.clone();
        mb.event(HostEvent::OpenDialog { plugin, dialog: d });
        Ok(())
    }

    fn close_dialog(&mut self, id: &str) {
        let mut mb = lock(&self.0);
        let plugin = mb.plugin.clone();
        mb.event(HostEvent::CloseDialog {
            plugin,
            id: id.to_string(),
        });
    }

    fn request_right(&mut self, r: runtime::Right) -> u64 {
        let right = right_from_wit(r);
        let mut mb = lock(&self.0);
        let request = mb.requests.next();
        match mb.rights.decide(&mb.manifest.rights, right) {
            Some(Grant::Allowed) => mb.deliver(request, RequestResult::Granted),
            Some(Grant::Denied) => mb.deliver(request, RequestResult::Denied),
            None => mb.ask(Pending::AskRight { request, right }),
        }
        request
    }

    fn right_granted(&self, r: runtime::Right) -> bool {
        let mb = lock(&self.0);
        mb.rights.decide(&mb.manifest.rights, right_from_wit(r)) == Some(Grant::Allowed)
    }

    fn net_get(&mut self, url: &str) -> Result<u64, String> {
        let mut mb = lock(&self.0);
        check_net_url(&mb.manifest, url)?;
        let request = mb.requests.next();
        match mb.rights.decide(&mb.manifest.rights, Right::Net) {
            Some(Grant::Allowed) => mb.net(request, url.to_string()),
            Some(Grant::Denied) => mb.deliver(request, RequestResult::Denied),
            None => mb.ask(Pending::Net {
                request,
                url: url.to_string(),
            }),
        }
        Ok(request)
    }
}

/// Was der Hintergrund-Thread nach Übersetzen und `init` zurückgibt.
type Started = Result<(Instance, Registration, PluginTexts), PluginError>;

/// Nachricht eines Start-Threads.
struct Loaded {
    generation: u64,
    result: Started,
}

/// Was der Host je Eintrag zusätzlich braucht (Index wie `entries`).
#[derive(Default)]
struct Slot {
    enabled: bool,
    /// Generation der aktuellen (oder ladenden) Instanz; 0 = noch keine.
    generation: u64,
    instance: Option<Instance>,
    mailbox: Option<Arc<Mutex<Mailbox>>>,
    commands: Vec<Command>,
    texts: Option<PluginTexts>,
}

impl Slot {
    fn new() -> Slot {
        Slot {
            enabled: true,
            ..Slot::default()
        }
    }
}

/// Durchsucht die Fundorte und fasst die Ordner je Kennung zu Einträgen zusammen (Zustand `Off`,
/// ohne startbaren Ordner `Failed(problem)`).
fn scan_entries(places: &Places) -> Vec<PluginEntry> {
    let mut groups: BTreeMap<String, Vec<Found>> = BTreeMap::new();
    for f in places::scan(places) {
        groups.entry(f.manifest.id.clone()).or_default().push(f);
    }
    groups
        .into_iter()
        .map(|(id, found)| {
            let active = places::pick_active(&found);
            let state = match active {
                Some(_) => EntryState::Off,
                None => EntryState::Failed(
                    found
                        .iter()
                        .find_map(|f| f.problem.clone())
                        .unwrap_or_default(),
                ),
            };
            PluginEntry {
                id,
                found,
                active,
                state,
                rights: Rights::default(),
                failures: 0,
            }
        })
        .collect()
}

/// Woran `rescan` erkennt, ob ein Plug-in gleich blieb: Ordner und Version des aktiven Fundorts.
fn active_key(e: &PluginEntry) -> Option<(PathBuf, semver::Version)> {
    e.active
        .map(|a| (e.found[a].dir.clone(), e.found[a].manifest.version.clone()))
}

/// Alle Plug-ins des Programms: Fundorte, Zustand, Instanzen und das Ereignis-Postfach.
pub struct PluginHost {
    places: Places,
    engine: Result<Arc<Engine>, String>,
    entries: Vec<PluginEntry>,
    slots: Vec<Slot>,
    /// Der Stand aus der Konfiguration (auch für Plug-ins, die gerade nicht gefunden werden).
    base: BTreeMap<String, PluginState>,
    outbox: Vec<HostEvent>,
    next_generation: u64,
    language: String,
    tx: Sender<Loaded>,
    rx: Receiver<Loaded>,
}

impl PluginHost {
    /// Durchsucht die Fundorte und richtet die Laufzeit ein; gestartet wird erst mit
    /// [`start_all`](Self::start_all).
    pub fn new(places: Places) -> PluginHost {
        let entries = scan_entries(&places);
        let slots = entries.iter().map(|_| Slot::new()).collect();
        let (tx, rx) = std::sync::mpsc::channel();
        PluginHost {
            places,
            engine: Engine::new().map(Arc::new).map_err(|e| e.to_string()),
            entries,
            slots,
            base: BTreeMap::new(),
            outbox: Vec::new(),
            next_generation: 1,
            language: "en".to_string(),
            tx,
            rx,
        }
    }

    /// Die Fundorte, die dieser Host durchsucht hat.
    pub fn places(&self) -> &Places {
        &self.places
    }

    /// Alle Plug-ins, nach Kennung sortiert.
    pub fn entries(&self) -> &[PluginEntry] {
        &self.entries
    }

    /// Die Sprache für Beschriftungen und `host-info` (gilt für später gestartete Instanzen).
    pub fn set_language(&mut self, language: &str) {
        self.language = language.to_string();
    }

    /// Übernimmt den gespeicherten Zustand und startet alle eingeschalteten Plug-ins im
    /// Hintergrund.
    pub fn start_all(&mut self, states: &BTreeMap<String, PluginState>) {
        self.base = states.clone();
        for i in 0..self.entries.len() {
            self.apply_state(i, states);
            if self.slots[i].enabled && self.entries[i].state == EntryState::Off {
                self.start(i);
            }
        }
    }

    /// Durchsucht die Fundorte neu (nach Installieren oder Entfernen). Plug-ins, deren aktiver
    /// Ordner gleich blieb (Ordner und Version), laufen weiter; geänderte werden beendet und neu
    /// gestartet (neue Generation), verschwundene beendet, neue mit `states` übernommen und,
    /// wenn eingeschaltet, gestartet. Schalter, Rechte und Ausfallzähler schon bekannter
    /// Plug-ins bleiben, wie sie in dieser Sitzung sind.
    pub fn rescan(&mut self, states: &BTreeMap<String, PluginState>) {
        // Den Stand dieser Sitzung behalten, auch für Plug-ins, die gleich verschwinden.
        let mut base = self.plugin_states();
        for (k, v) in states {
            base.entry(k.clone()).or_insert_with(|| v.clone());
        }
        self.base = base;
        let fresh = scan_entries(&self.places);
        // Beenden, was wegfällt oder sich ändert.
        for i in 0..self.entries.len() {
            let keep = fresh
                .iter()
                .find(|f| f.id == self.entries[i].id)
                .is_some_and(|f| active_key(f) == active_key(&self.entries[i]));
            if !keep {
                self.stop_instance(i);
            }
        }
        let mut old: BTreeMap<String, (PluginEntry, Slot)> = std::mem::take(&mut self.entries)
            .into_iter()
            .zip(std::mem::take(&mut self.slots))
            .map(|(e, s)| (e.id.clone(), (e, s)))
            .collect();
        let mut to_start = Vec::new();
        for mut entry in fresh {
            let i = self.entries.len();
            match old.remove(&entry.id) {
                Some((before, slot)) => {
                    let unchanged = active_key(&before) == active_key(&entry);
                    entry.rights = before.rights;
                    entry.failures = before.failures;
                    if unchanged {
                        entry.state = before.state;
                    } else if entry.active.is_some() {
                        entry.state = if entry.failures >= MAX_FAILURES {
                            EntryState::Stopped
                        } else {
                            EntryState::Off
                        };
                        if slot.enabled {
                            to_start.push(i);
                        }
                    }
                    self.entries.push(entry);
                    self.slots.push(slot);
                }
                None => {
                    self.entries.push(entry);
                    self.slots.push(Slot::new());
                    self.apply_state(i, &self.base.clone());
                    if self.slots[i].enabled {
                        to_start.push(i);
                    }
                }
            }
        }
        for i in to_start {
            if self.entries[i].state == EntryState::Off {
                self.start(i);
            }
        }
    }

    /// Übernimmt Schalter und Rechte für Eintrag `i` aus `states`.
    fn apply_state(&mut self, i: usize, states: &BTreeMap<String, PluginState>) {
        let Some(c) = states.get(&self.entries[i].id) else {
            return;
        };
        self.slots[i].enabled = c.enabled;
        for (id, grant) in &c.rights {
            let answer = match grant.as_str() {
                "allowed" => Answer::Allow,
                "denied" => Answer::Deny,
                _ => continue,
            };
            if let Some(r) = Right::from_id(id) {
                self.entries[i].rights.answer(r, answer);
            }
        }
    }

    /// Der zu speichernde Zustand: der Stand aus `start_all`/`rescan`, darüber der aktuelle jedes
    /// gefundenen Plug-ins. Plug-ins in Voreinstellung (an, keine Rechte) fehlen.
    pub fn plugin_states(&self) -> BTreeMap<String, PluginState> {
        let mut out = self.base.clone();
        for (e, s) in self.entries.iter().zip(&self.slots) {
            // Unbekannte Rechte (aus einer neueren Programmversion) bleiben erhalten.
            let mut rights: BTreeMap<String, String> = out
                .get(&e.id)
                .map(|c| {
                    c.rights
                        .iter()
                        .filter(|(k, _)| Right::from_id(k).is_none())
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .unwrap_or_default();
            for (r, g) in e.rights.saved() {
                let g = match g {
                    Grant::Allowed => "allowed",
                    Grant::Denied => "denied",
                };
                rights.insert(r.id().to_string(), g.to_string());
            }
            let c = PluginState {
                enabled: s.enabled,
                rights,
            };
            if c == PluginState::default() {
                out.remove(&e.id);
            } else {
                out.insert(e.id.clone(), c);
            }
        }
        out
    }

    /// Holt fertig gestartete Instanzen ab, stellt wartende Ergebnisse zu (`on-result`, Frist:
    /// Oberfläche) und gibt alle angefallenen Ereignisse zurück.
    pub fn poll(&mut self) -> Vec<HostEvent> {
        while let Ok(loaded) = self.rx.try_recv() {
            self.finish_start(loaded);
        }
        for i in 0..self.slots.len() {
            let Some(mb) = self.slots[i].mailbox.clone() else {
                continue;
            };
            if self.slots[i].instance.is_none() {
                continue;
            }
            // Nur was jetzt da ist; was dabei neu anfällt, kommt beim nächsten `poll`.
            let due = std::mem::take(&mut lock(&mb).deliveries);
            for (request, result) in due {
                if !self.call(i, |inst| inst.on_result(request, result)) {
                    break;
                }
            }
        }
        std::mem::take(&mut self.outbox)
    }

    /// Führt einen Befehl `"<plugin-id>.<befehl-id>"` eines laufenden Plug-ins aus.
    pub fn run_command(&mut self, full_id: &str) {
        let found = self.entries.iter().enumerate().find_map(|(i, e)| {
            let rest = full_id.strip_prefix(e.id.as_str())?.strip_prefix('.')?;
            (e.state == EntryState::Running && self.slots[i].commands.iter().any(|c| c.id == rest))
                .then(|| (i, rest.to_string()))
        });
        if let Some((i, cmd)) = found {
            self.call(i, |inst| inst.run_command(&cmd));
        }
    }

    /// Meldet ein Ereignis eines Dialogs an sein Plug-in.
    pub fn dialog_event(&mut self, plugin: &str, dialog: &str, ev: DialogEvent) {
        if let Some(i) = self.running(plugin) {
            self.call(i, |inst| inst.handle_dialog(dialog, ev));
        }
    }

    /// Die Antwort des Nutzers auf eine Nachfrage ([`HostEvent::Ask`]). Gehört sie zu einer
    /// anderen Generation oder ist sie unbekannt, wird sie still verworfen.
    pub fn answer(&mut self, plugin: &str, generation: u64, request: u64, a: Answer) {
        let Some(i) = self.running(plugin) else {
            return;
        };
        let Some(mb) = self.slots[i].mailbox.clone() else {
            return;
        };
        let mut mb = lock(&mb);
        if mb.generation != generation {
            return;
        }
        let Some(right) = mb
            .asks
            .iter()
            .find(|(_, p)| pending_request(p) == request)
            .map(|(r, _)| *r)
        else {
            return;
        };
        self.entries[i].rights.answer(right, a);
        mb.rights = self.entries[i].rights.clone();
        if a != Answer::Once {
            self.outbox.push(HostEvent::Changed);
        }
        let allowed = a != Answer::Deny;
        let mut done = Vec::new();
        done.extend(mb.asks.remove(&right));
        let (same, other): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut mb.waiting)
            .into_iter()
            .partition(|p| pending_right(p) == right);
        mb.waiting = other;
        done.extend(same);
        for p in done {
            mb.resolve(p, allowed);
        }
        self.outbox.append(&mut mb.events);
    }

    /// Nimmt die Entscheidung über ein Recht zurück (auch „Nur dieses Mal“): beim nächsten Antrag
    /// wird wieder gefragt. Meldet [`HostEvent::Changed`].
    pub fn revoke(&mut self, id: &str, right: Right) {
        let Some(i) = self.index(id) else {
            return;
        };
        self.entries[i].rights.revoke(right);
        if let Some(mb) = &self.slots[i].mailbox {
            lock(mb).rights = self.entries[i].rights.clone();
        }
        self.outbox.push(HostEvent::Changed);
    }

    /// Das Ergebnis einer Netzanfrage ([`HostEvent::Net`]); geht beim nächsten `poll` an das
    /// Plug-in. Andere Generation oder unbekannte Anfrage: still verworfen.
    pub fn net_result(
        &mut self,
        plugin: &str,
        generation: u64,
        request: u64,
        result: RequestResult,
    ) {
        let Some(i) = self.running(plugin) else {
            return;
        };
        if let Some(mb) = &self.slots[i].mailbox {
            let mut mb = lock(mb);
            if mb.generation == generation && mb.net_open.remove(&request) {
                mb.deliver(request, result);
            }
        }
    }

    /// Startet ein laufendes oder ausgefallenes Plug-in neu (neue Instanz, neue Generation).
    /// Nach [`MAX_FAILURES`] Ausfällen wirkungslos.
    pub fn restart(&mut self, id: &str) {
        let Some(i) = self.index(id) else {
            return;
        };
        if !self.slots[i].enabled
            || !matches!(
                self.entries[i].state,
                EntryState::Running | EntryState::Failed(_)
            )
        {
            return;
        }
        self.stop_instance(i);
        self.start(i);
    }

    /// Schaltet ein Plug-in ein (startet es) oder aus (beendet es). Ein zu oft ausgefallenes
    /// Plug-in bleibt bis zum Programmneustart aus.
    pub fn set_enabled(&mut self, id: &str, on: bool) {
        let Some(i) = self.index(id) else {
            return;
        };
        if self.slots[i].enabled != on {
            self.slots[i].enabled = on;
            self.outbox.push(HostEvent::Changed);
        }
        if on {
            if self.entries[i].state == EntryState::Off {
                self.start(i);
            }
        } else if self.entries[i].state != EntryState::Stopped {
            self.stop_instance(i);
            if self.entries[i].active.is_some() {
                self.entries[i].state = EntryState::Off;
            }
        }
    }

    /// Die Befehle aller laufenden Plug-ins mit Beschriftung in der eingestellten Sprache.
    pub fn commands(&self) -> Vec<PluginCommand> {
        let mut out = Vec::new();
        for (e, s) in self.entries.iter().zip(&self.slots) {
            if e.state != EntryState::Running {
                continue;
            }
            for c in &s.commands {
                let label = s.texts.as_ref().map_or_else(
                    || c.label_key.clone(),
                    |t| t.text(&self.language, &c.label_key, &[]),
                );
                out.push(PluginCommand {
                    full_id: format!("{}.{}", e.id, c.id),
                    label,
                    plugin: e.id.clone(),
                });
            }
        }
        out
    }

    /// Der Text `key` aus den Sprachdateien des laufenden Plug-ins `plugin` in der Sprache
    /// `language` (sonst Englisch, sonst der Schlüssel selbst). Für die Beschriftungen seiner
    /// Dialoge, die das Programm in jedem Bild neu übersetzt.
    pub fn text(&self, plugin: &str, language: &str, key: &str) -> String {
        self.running(plugin)
            .and_then(|i| self.slots[i].texts.as_ref())
            .map_or_else(|| key.to_string(), |t| t.text(language, key, &[]))
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }

    fn running(&self, id: &str) -> Option<usize> {
        self.index(id)
            .filter(|&i| self.entries[i].state == EntryState::Running)
    }

    /// Beendet die Instanz (falls eine läuft) und vergisst ihr Postfach; ein noch ladender Start
    /// wird beim Abholen verworfen (seine Generation stimmt dann nicht mehr).
    fn stop_instance(&mut self, i: usize) {
        self.retract(i);
        let slot = &mut self.slots[i];
        if let Some(mut inst) = slot.instance.take() {
            inst.shutdown();
        }
        if let Some(mb) = slot.mailbox.take() {
            // Protokoll aus `shutdown` noch weitergeben, alles andere verfällt.
            let events = std::mem::take(&mut lock(&mb).events);
            self.outbox.extend(
                events
                    .into_iter()
                    .filter(|e| matches!(e, HostEvent::Log { .. })),
            );
        }
        slot.commands.clear();
        slot.texts = None;
        slot.generation = 0;
    }

    /// Startet Eintrag `i` im Hintergrund: Dateien lesen, übersetzen (mit Zwischenspeicher),
    /// Instanz anlegen, `init`.
    fn start(&mut self, i: usize) {
        if self.entries[i].failures >= MAX_FAILURES {
            self.entries[i].state = EntryState::Stopped;
            return;
        }
        let Some(active) = self.entries[i].active else {
            return;
        };
        let engine = match &self.engine {
            Ok(e) => Arc::clone(e),
            Err(reason) => {
                let reason = reason.clone();
                self.entries[i].state = EntryState::Failed(reason.clone());
                self.outbox.push(HostEvent::Failed {
                    plugin: self.entries[i].id.clone(),
                    reason,
                });
                return;
            }
        };
        let generation = self.next_generation;
        self.next_generation += 1;
        let found = &self.entries[i].found[active];
        let manifest = found.manifest.clone();
        let dir: PathBuf = found.dir.clone();
        let mailbox = Arc::new(Mutex::new(Mailbox {
            plugin: self.entries[i].id.clone(),
            generation,
            manifest: manifest.clone(),
            rights: self.entries[i].rights.clone(),
            requests: Requests::default(),
            events: Vec::new(),
            deliveries: Vec::new(),
            asks: BTreeMap::new(),
            waiting: Vec::new(),
            net_open: BTreeSet::new(),
        }));
        let slot = &mut self.slots[i];
        slot.generation = generation;
        slot.mailbox = Some(Arc::clone(&mailbox));
        self.entries[i].state = EntryState::Loading;

        let info = HostInfo {
            api_version: planora_plugin_api::API_VERSION.to_string(),
            program_version: env!("CARGO_PKG_VERSION").to_string(),
            language: self.language.clone(),
        };
        let cache = self.places.cache.clone();
        let tx = self.tx.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("plugin-start-{}", manifest.id))
            .spawn(move || {
                let result = start_instance(&engine, &cache, &dir, &manifest, info, mailbox);
                let _ = tx.send(Loaded { generation, result });
            });
        if let Err(e) = spawned {
            self.fail(i, tr!("plugin-start-thread-failed", error = e.to_string()));
        }
    }

    /// Übernimmt das Ergebnis eines Start-Threads, wenn es noch zur aktuellen Generation gehört.
    fn finish_start(&mut self, loaded: Loaded) {
        // Generationen sind im ganzen Host eindeutig; `rescan` darf die Reihenfolge ändern.
        let slot = self
            .slots
            .iter()
            .position(|s| s.generation != 0 && s.generation == loaded.generation);
        let Some(i) = slot.filter(|&i| self.entries[i].state == EntryState::Loading) else {
            // Veraltet (ausgeschaltet, neu gestartet, entfernt): `init` lief schon, also sauber
            // beenden.
            if let Ok((mut instance, _, _)) = loaded.result {
                instance.shutdown();
            }
            return;
        };
        match loaded.result {
            Ok((instance, registration, texts)) => {
                let slot = &mut self.slots[i];
                slot.instance = Some(instance);
                slot.commands = registration.commands;
                slot.texts = Some(texts);
                self.entries[i].state = EntryState::Running;
                self.drain(i);
            }
            Err(e) => {
                self.drain(i);
                self.fail(i, e.to_string());
            }
        }
    }

    /// Ruft die Instanz von Eintrag `i` auf (Rechte vorher abgeglichen, Postfach danach geleert).
    /// Ein Fehler des Plug-ins selbst wird eine Meldung, ein Ausfall stoppt die Instanz.
    /// `false`, wenn die Instanz danach nicht mehr läuft.
    fn call(&mut self, i: usize, f: impl FnOnce(&mut Instance) -> Result<(), PluginError>) -> bool {
        if let Some(mb) = &self.slots[i].mailbox {
            lock(mb).rights = self.entries[i].rights.clone();
        }
        let Some(inst) = self.slots[i].instance.as_mut() else {
            return false;
        };
        let result = f(inst);
        self.drain(i);
        match result {
            Ok(()) => true,
            Err(PluginError::Plugin(text)) => {
                self.outbox.push(HostEvent::Message {
                    plugin: self.entries[i].id.clone(),
                    level: Level::Error,
                    text,
                });
                true
            }
            Err(e) => {
                self.fail(i, e.to_string());
                false
            }
        }
    }

    /// Nimmt noch nicht abgeholte Nachfragen, Netzanfragen (der aktuellen Generation) und
    /// Dialoge von Eintrag `i` aus dem Ausgang, weil die Instanz gleich verschwindet. Meldungen,
    /// Protokoll und Ausfälle bleiben.
    fn retract(&mut self, i: usize) {
        let id = &self.entries[i].id;
        let current = self.slots[i].generation;
        self.outbox.retain(|e| match e {
            HostEvent::Ask {
                plugin, generation, ..
            }
            | HostEvent::Net {
                plugin, generation, ..
            } => !(plugin == id && *generation == current),
            HostEvent::OpenDialog { plugin, .. } => plugin != id,
            _ => true,
        });
    }

    /// Holt die Ereignisse aus dem Postfach von Eintrag `i`.
    fn drain(&mut self, i: usize) {
        if let Some(mb) = &self.slots[i].mailbox {
            self.outbox.append(&mut lock(mb).events);
        }
    }

    /// Ein Ausfall: Instanz weg, Zähler hoch, genau ein [`HostEvent::Failed`].
    fn fail(&mut self, i: usize, reason: String) {
        self.retract(i);
        let slot = &mut self.slots[i];
        slot.instance = None;
        slot.mailbox = None;
        slot.commands.clear();
        slot.texts = None;
        slot.generation = 0;
        let e = &mut self.entries[i];
        e.failures = e.failures.saturating_add(1);
        e.state = if e.failures >= MAX_FAILURES {
            EntryState::Stopped
        } else {
            EntryState::Failed(reason.clone())
        };
        self.outbox.push(HostEvent::Failed {
            plugin: e.id.clone(),
            reason,
        });
    }
}

impl Drop for PluginHost {
    /// Beendet alle laufenden Instanzen (`shutdown`, Frist: Oberfläche).
    fn drop(&mut self) {
        for slot in &mut self.slots {
            if let Some(inst) = slot.instance.as_mut() {
                inst.shutdown();
            }
        }
    }
}

/// Im Hintergrund-Thread: Dateien lesen, Komponente laden, Instanz anlegen, `init`.
fn start_instance(
    engine: &Engine,
    cache: &std::path::Path,
    dir: &std::path::Path,
    manifest: &Manifest,
    info: HostInfo,
    mailbox: Arc<Mutex<Mailbox>>,
) -> Started {
    Engine::check_api(manifest)?;
    let (wasm, languages) = places::read_files(dir).map_err(|e| {
        PluginError::Load(tr!(
            "plugin-read-failed",
            plugin = &manifest.name,
            error = e.to_string()
        ))
    })?;
    let component = places::load_component(engine, cache, &wasm)?;
    let mut instance = engine.instantiate_component(
        manifest,
        &component,
        &languages,
        Box::new(Recorder(mailbox)),
    )?;
    let registration = instance.init(info)?;
    Ok((instance, registration, PluginTexts::new(&languages)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mailbox() -> Mailbox {
        let manifest = Manifest::parse(
            "id = \"a.b\"\nname = \"A\"\nversion = \"0.1.0\"\napi = \"0.1\"\nauthor = \"x\"\n\
             license = \"MIT\"\ndescription = \"d\"\nrights = [\"net\"]\nnet-hosts = [\"example.org\"]\n",
        )
        .unwrap();
        Mailbox {
            plugin: "a.b".into(),
            generation: 1,
            manifest,
            rights: Rights::default(),
            requests: Requests::default(),
            events: Vec::new(),
            deliveries: Vec::new(),
            asks: BTreeMap::new(),
            waiting: Vec::new(),
            net_open: BTreeSet::new(),
        }
    }

    fn voll() -> RequestResult {
        RequestResult::Failed(queue_full())
    }

    #[test]
    fn full_queue_records_no_state() {
        let mut mb = mailbox();
        mb.events = vec![HostEvent::Changed; MAX_QUEUE];
        mb.ask(Pending::AskRight {
            request: 1,
            right: Right::Net,
        });
        mb.net(2, "https://example.org/".into());
        assert!(mb.asks.is_empty(), "keine Nachfrage ohne Ereignis");
        assert!(
            mb.net_open.is_empty(),
            "keine offene Netzanfrage ohne Ereignis"
        );
        assert_eq!(mb.deliveries, vec![(1, voll()), (2, voll())]);
    }

    #[test]
    fn full_waiting_list_answers_failed() {
        let mut mb = mailbox();
        mb.ask(Pending::AskRight {
            request: 1,
            right: Right::Net,
        });
        mb.waiting = (0..MAX_QUEUE as u64)
            .map(|n| Pending::AskRight {
                request: 100 + n,
                right: Right::Net,
            })
            .collect();
        mb.ask(Pending::Net {
            request: 2,
            url: "https://example.org/".into(),
        });
        assert_eq!(mb.waiting.len(), MAX_QUEUE);
        assert_eq!(mb.deliveries, vec![(2, voll())]);
    }
}
