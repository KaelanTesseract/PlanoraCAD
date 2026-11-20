//! Plug-in-Schnittstelle (WebAssembly): Paketformat, Manifest, Rechte, Laufzeit, Fundorte und der
//! [`PluginHost`], der alle Plug-ins des Programms verwaltet.
//!
//! Der Host ruft Plug-ins nur auf dem aufrufenden (Oberflächen-)Thread auf; nur Übersetzen und
//! `init` laufen je Plug-in in einem Hintergrund-Thread. Was ein Plug-in vom Programm will,
//! landet in einem Postfach und kommt über [`PluginHost::poll`] als [`HostEvent`] heraus. Kein
//! Aufruf des Plug-ins wartet und keiner ruft in das Plug-in zurück: Ergebnisse (Rechte, Netz)
//! gehen erst beim nächsten `poll` als `on-result` an das Plug-in.

mod bindings;
pub mod licenses;
pub mod manifest;
pub mod package;
pub mod places;
pub mod rights;
pub mod runtime;
pub mod texts;

use manifest::{Manifest, Right};
use places::{Found, Places};
use planora_i18n::tr;
use rights::{Answer, Grant, Pending, Requests, Rights, Source, check_net_url};
use runtime::{
    Command, Dialog, DialogEvent, Engine, FieldKind, HostCalls, HostInfo, Instance, Level,
    PluginError, Registration, RequestResult,
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
/// So viele Bytes (ungefähr: Texte, Dialoge, Adressen) nehmen die Ereignisse im Postfach einer
/// Instanz höchstens auf; was darüber hinausgeht, wird verworfen (einmal mit einem
/// Protokoll-Hinweis). Ergebnisse zählen nicht mit: Netz-Antworten sind schon durch [`MAX_NET`]
/// gleichzeitige Anfragen und die Größengrenze des Programms begrenzt, die übrigen sind klein.
const MAX_MAILBOX_BYTES: usize = 8 << 20;
/// Höchstens so viele Netzanfragen eines Plug-ins laufen gleichzeitig.
const MAX_NET: usize = 4;
/// Längster Text einer Meldung oder eines Protokolleintrags (Bytes); länger wird gekürzt.
const MAX_TEXT: usize = 4 << 10;
/// Höchstzahl der Felder eines Dialogs.
const MAX_FIELDS: usize = 200;
/// Längste Zeichenkette in einem Dialog (Kennungen, Schlüssel, Textwerte; Bytes).
const MAX_DIALOG_STRING: usize = 1 << 10;
/// Höchstzahl der Befehle, die ein Plug-in in `init` anmeldet.
const MAX_COMMANDS: usize = 100;
/// Längste Zeichenkette eines Befehls (Kennung, Schlüssel, Symbol, Kürzel; Bytes).
const MAX_COMMAND_STRING: usize = 256;
/// Zeit je Plug-in und [`PluginHost::poll`] für alle Zustellungen (`on-result`) zusammen.
const POLL_BUDGET: std::time::Duration = std::time::Duration::from_millis(200);
/// Höchstens so viele Zustellungen je Plug-in und `poll`.
const MAX_DELIVERIES_PER_POLL: usize = 16;
/// Reicht das Budget in so vielen Bildern hintereinander nicht, zählt das als Ausfall.
const MAX_OVERLOADED_POLLS: u8 = 3;

/// Ergebnis einer Anfrage, die das volle Postfach nicht mehr annimmt.
fn queue_full() -> String {
    tr!("plugin-queue-full")
}

/// Kürzt `text` auf höchstens [`MAX_TEXT`] Bytes (an einer Zeichengrenze, mit „…“ am Ende).
fn clip(text: &str) -> String {
    if text.len() <= MAX_TEXT {
        return text.to_string();
    }
    let ellipsis = '\u{2026}';
    let mut end = MAX_TEXT - ellipsis.len_utf8();
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = text[..end].to_string();
    out.push(ellipsis);
    out
}

/// Ungefähre Größe eines Ereignisses im Postfach (Bytes).
fn event_size(e: &HostEvent) -> usize {
    const BASE: usize = 64;
    BASE + match e {
        HostEvent::Message { text, .. } | HostEvent::Log { text, .. } => text.len(),
        HostEvent::OpenDialog { dialog, .. } => {
            dialog.id.len()
                + dialog.title_key.len()
                + dialog
                    .fields
                    .iter()
                    .map(|f| BASE + f.id.len() + f.label_key.len() + field_text(&f.kind).len())
                    .sum::<usize>()
        }
        HostEvent::CloseDialog { id, .. } => id.len(),
        HostEvent::Net { url, .. } => url.len(),
        HostEvent::Failed { reason, .. } => reason.len(),
        HostEvent::Ask { .. } | HostEvent::Changed => 0,
    }
}

/// Der Text eines Textfelds (sonst leer).
fn field_text(kind: &FieldKind) -> &str {
    match kind {
        FieldKind::Text(t) => t,
        _ => "",
    }
}

/// Prüft einen Dialog eines Plug-ins: höchstens [`MAX_FIELDS`] Felder, jede Zeichenkette
/// höchstens [`MAX_DIALOG_STRING`] Bytes, Feld-Kennungen nicht leer und nicht doppelt.
fn check_dialog(d: &Dialog) -> Result<(), String> {
    if d.fields.len() > MAX_FIELDS {
        return Err(tr!(
            "plugin-dialog-too-many-fields",
            max = MAX_FIELDS.to_string()
        ));
    }
    let too_long = [d.id.as_str(), d.title_key.as_str()]
        .into_iter()
        .chain(
            d.fields
                .iter()
                .flat_map(|f| [f.id.as_str(), f.label_key.as_str(), field_text(&f.kind)]),
        )
        .any(|s| s.len() > MAX_DIALOG_STRING);
    if too_long {
        return Err(tr!(
            "plugin-dialog-string-too-long",
            max = MAX_DIALOG_STRING.to_string()
        ));
    }
    let mut ids = BTreeSet::new();
    for f in &d.fields {
        if f.id.is_empty() {
            return Err(tr!("plugin-dialog-empty-field-id"));
        }
        if !ids.insert(f.id.as_str()) {
            return Err(tr!("plugin-dialog-duplicate-field-id", id = f.id.as_str()));
        }
    }
    Ok(())
}

/// Prüft die Anmeldung aus `init`: höchstens [`MAX_COMMANDS`] Befehle, jede Zeichenkette
/// höchstens [`MAX_COMMAND_STRING`] Bytes.
fn check_registration(r: &Registration) -> Result<(), PluginError> {
    if r.commands.len() > MAX_COMMANDS {
        return Err(PluginError::Load(tr!(
            "plugin-too-many-commands",
            max = MAX_COMMANDS.to_string()
        )));
    }
    let too_long = r.commands.iter().any(|c| {
        [
            Some(&c.id),
            Some(&c.label_key),
            c.icon.as_ref(),
            c.shortcut.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|s| s.len() > MAX_COMMAND_STRING)
    });
    if too_long {
        return Err(PluginError::Load(tr!(
            "plugin-command-string-too-long",
            max = MAX_COMMAND_STRING.to_string()
        )));
    }
    Ok(())
}

/// Der zu speichernde Zustand eines Plug-ins: eingeschaltet oder nicht und die dauerhaft
/// beantworteten Rechte (Kennung des Rechts → `"allowed"` oder `"denied"`), dazu Autor und
/// Rechner (`net-hosts`) und Herkunft des Manifests, für das die Rechte erteilt wurden. Das Programm bildet
/// ihn auf `planora_project::config::PluginConfig` ab; der Host kennt die Konfiguration nicht.
///
/// Beim Start gelten gespeicherte Rechte nur nach [`Rights::carry_over_from`] gegen das
/// gefundene Manifest: ein anderer (oder kein gespeicherter) Autor oder eine andere Herkunft kostet
/// alle, andere Rechner kosten `net` (etwa nach einem Update über den Installer oder einem von Hand kopierten Ordner).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginState {
    pub enabled: bool,
    pub rights: BTreeMap<String, String>,
    /// Autor des Manifests beim Speichern; `None`: unbekannt (Rechte werden neu gefragt).
    pub author: Option<String>,
    /// Rechner aus `net-hosts` des Manifests beim Speichern.
    pub net_hosts: Vec<String>,
    /// Herkunft beim Speichern ([`Source::to_config`]); `None`: unbekannt (Rechte werden neu
    /// gefragt).
    pub source: Option<String>,
}

impl Default for PluginState {
    fn default() -> Self {
        PluginState {
            enabled: true,
            rights: BTreeMap::new(),
            author: None,
            net_hosts: Vec::new(),
            source: None,
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
    /// `"<plugin-id>.<befehl-id>"` (nur zur Anzeige und als Schlüssel; ausgeführt wird mit
    /// [`PluginHost::run_command`] über `plugin` und `command`).
    pub full_id: String,
    /// Die Kennung des Befehls im Plug-in.
    pub command: String,
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
    /// Ungefähre Größe von `events` (höchstens [`MAX_MAILBOX_BYTES`]).
    bytes: usize,
    /// Der Hinweis „Postfach voll“ steht schon im Protokoll (einmal je Instanz).
    full_noted: bool,
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
    fn new(plugin: String, generation: u64, manifest: Manifest, rights: Rights) -> Mailbox {
        Mailbox {
            plugin,
            generation,
            manifest,
            rights,
            requests: Requests::default(),
            events: Vec::new(),
            deliveries: Vec::new(),
            bytes: 0,
            full_noted: false,
            asks: BTreeMap::new(),
            waiting: Vec::new(),
            net_open: BTreeSet::new(),
        }
    }

    /// Passen `size` weitere Bytes noch ins Postfach? Sonst einmal je Instanz ein Hinweis ins
    /// Protokoll (der selbst nicht mitzählt).
    fn fits(&mut self, size: usize) -> bool {
        if self.bytes.saturating_add(size) <= MAX_MAILBOX_BYTES {
            return true;
        }
        if !self.full_noted && self.events.len() < MAX_QUEUE {
            self.full_noted = true;
            self.events.push(HostEvent::Log {
                plugin: self.plugin.clone(),
                level: Level::Warning,
                text: format!(
                    "mailbox over {} MiB, entries dropped",
                    MAX_MAILBOX_BYTES >> 20
                ),
            });
        }
        false
    }

    fn event(&mut self, e: HostEvent) {
        let size = event_size(&e);
        if self.events.len() < MAX_QUEUE && self.fits(size) {
            self.bytes += size;
            self.events.push(e);
        }
    }

    fn deliver(&mut self, request: u64, r: RequestResult) {
        if self.deliveries.len() < MAX_QUEUE {
            self.deliveries.push((request, r));
        }
    }

    /// Nimmt alle Ereignisse heraus.
    fn take_events(&mut self) -> Vec<HostEvent> {
        let events = std::mem::take(&mut self.events);
        self.bytes = self
            .bytes
            .saturating_sub(events.iter().map(event_size).sum());
        events
    }

    /// Nimmt alle Ergebnisse heraus.
    fn take_deliveries(&mut self) -> Vec<(u64, RequestResult)> {
        std::mem::take(&mut self.deliveries)
    }

    /// Legt nicht zugestellte Ergebnisse vor die inzwischen neu angefallenen zurück.
    fn requeue(&mut self, rest: Vec<(u64, RequestResult)>) {
        let newer = std::mem::replace(&mut self.deliveries, rest);
        self.deliveries.extend(newer);
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
        if self.net_open.len() >= MAX_NET {
            self.deliver(
                request,
                RequestResult::Failed(tr!("plugin-net-too-many", max = MAX_NET.to_string())),
            );
            return;
        }
        let e = HostEvent::Net {
            plugin: self.plugin.clone(),
            generation: self.generation,
            request,
            url,
        };
        let size = event_size(&e);
        if self.events_full() || !self.fits(size) {
            self.overflow(request);
            return;
        }
        self.net_open.insert(request);
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
        let e = HostEvent::Ask {
            plugin: self.plugin.clone(),
            generation: self.generation,
            request,
            right,
        };
        if self.events_full() || !self.fits(event_size(&e)) {
            self.overflow(request);
            return;
        }
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

/// Die Host-Aufrufe einer Instanz: tragen nur ins Postfach ein (Texte gekürzt, Dialoge geprüft).
struct Recorder(Arc<Mutex<Mailbox>>);

impl HostCalls for Recorder {
    fn log(&mut self, level: Level, text: &str) {
        let mut mb = lock(&self.0);
        let plugin = mb.plugin.clone();
        mb.event(HostEvent::Log {
            plugin,
            level,
            text: clip(text),
        });
    }

    fn message(&mut self, level: Level, text: &str) {
        let mut mb = lock(&self.0);
        let plugin = mb.plugin.clone();
        mb.event(HostEvent::Message {
            plugin,
            level,
            text: clip(text),
        });
    }

    fn open_dialog(&mut self, d: Dialog) -> Result<(), String> {
        check_dialog(&d)?;
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
    /// So viele `poll` hintereinander reichte das Zustell-Budget nicht ([`MAX_OVERLOADED_POLLS`]).
    overloaded: u8,
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

/// Der Eintrag, dessen Plug-in `plugin` läuft und den Befehl `command` angemeldet hat.
fn command_target(
    entries: &[PluginEntry],
    slots: &[Slot],
    plugin: &str,
    command: &str,
) -> Option<usize> {
    let i = entries.iter().position(|e| e.id == plugin)?;
    (entries[i].state == EntryState::Running && slots[i].commands.iter().any(|c| c.id == command))
        .then_some(i)
}

/// Der laufende Fundort.
fn active_found(e: &PluginEntry) -> Option<&Found> {
    e.active.map(|a| &e.found[a])
}

/// Woran `rescan` erkennt, ob ein Plug-in gleich blieb: Ordner und Version des aktiven Fundorts.
fn active_key(e: &PluginEntry) -> Option<(PathBuf, semver::Version)> {
    e.active
        .map(|a| (e.found[a].dir.clone(), e.found[a].manifest.version.clone()))
}

/// Alle Plug-ins des Programms: Fundorte, Zustand, Instanzen und das Ereignis-Postfach.
pub struct PluginHost {
    places: Places,
    /// Die Laufzeit samt Epochen-Takt; erst angelegt, wenn das erste Plug-in startet.
    engine: Option<Result<Arc<Engine>, String>>,
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
    /// Durchsucht die Fundorte; die Laufzeit (mit ihrem Epochen-Takt) entsteht erst, wenn das erste
    /// Plug-in startet ([`start_all`](Self::start_all)).
    pub fn new(places: Places) -> PluginHost {
        let entries = scan_entries(&places);
        let slots = entries.iter().map(|_| Slot::new()).collect();
        let (tx, rx) = std::sync::mpsc::channel();
        PluginHost {
            places,
            engine: None,
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
    /// Hintergrund. Danach räumt ein Hintergrund-Thread den Zwischenspeicher auf
    /// ([`places::prune_cache`]: Einträge, die keinem gefundenen Plug-in mehr gehören).
    pub fn start_all(&mut self, states: &BTreeMap<String, PluginState>) {
        self.base = states.clone();
        for i in 0..self.entries.len() {
            self.apply_state(i, states);
            if self.slots[i].enabled && self.entries[i].state == EntryState::Off {
                self.start(i);
            }
        }
        self.prune_cache();
    }

    /// Startet [`places::prune_cache`] im Hintergrund (mit der Laufzeit, falls schon eine läuft).
    fn prune_cache(&self) {
        let engine = match &self.engine {
            Some(Ok(e)) => Some(Arc::clone(e)),
            _ => None,
        };
        let cache = self.places.cache.clone();
        let dirs: Vec<PathBuf> = self
            .entries
            .iter()
            .flat_map(|e| e.found.iter().map(|f| f.dir.clone()))
            .collect();
        let _ = std::thread::Builder::new()
            .name("plugin-cache-cleanup".into())
            .spawn(move || places::prune_cache(engine.as_deref(), &cache, &dirs));
    }

    /// Läuft die Laufzeit (und mit ihr der Epochen-Takt)? Erst, seit ein Plug-in gestartet wurde.
    pub fn runtime_started(&self) -> bool {
        matches!(self.engine, Some(Ok(_)))
    }

    /// Warten Ergebnisse auf die Zustellung beim nächsten [`poll`](Self::poll)? Dann muss das
    /// Programm bald wieder zeichnen, auch wenn sonst nichts geschieht.
    pub fn has_pending(&self) -> bool {
        !self.outbox.is_empty()
            || self.slots.iter().any(|s| {
                s.instance.is_some()
                    && s.mailbox
                        .as_ref()
                        .is_some_and(|mb| !lock(mb).deliveries.is_empty())
            })
    }

    /// Durchsucht die Fundorte neu (nach Installieren oder Entfernen). Plug-ins, deren aktiver
    /// Ordner gleich blieb (Ordner und Version), laufen weiter; geänderte werden beendet und neu
    /// gestartet (neue Generation), verschwundene beendet, neue mit `states` übernommen und,
    /// wenn eingeschaltet, gestartet. Schalter schon bekannter Plug-ins bleiben, wie sie in dieser
    /// Sitzung sind; Ausfallzähler und Rechte auch, solange der aktive Ordner gleich blieb. Sonst
    /// beginnt der Ausfallzähler bei null (auch ein angehaltenes Plug-in startet wieder), und die
    /// Rechte bleiben nur nach [`Rights::carry_over`] (gleicher Autor, gleiche Herkunft, gleiche
    /// Rechner).
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
                    let old = active_found(&before).map(|f| (&f.manifest, f.source()));
                    entry.rights = match (unchanged, active_found(&entry)) {
                        (false, Some(new)) => before.rights.carry_over(
                            old.as_ref().map(|(m, s)| (*m, s)),
                            &new.manifest,
                            &new.source(),
                        ),
                        _ => before.rights,
                    };
                    if unchanged {
                        entry.failures = before.failures;
                        entry.state = before.state;
                    } else if entry.active.is_some() {
                        // Ein anderer Stand (Ordner oder Version) beginnt ohne Ausfälle.
                        entry.state = EntryState::Off;
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

    /// Übernimmt Schalter und Rechte für Eintrag `i` aus `states`. Die Rechte nur nach
    /// [`Rights::carry_over_from`] gegen den aktiven Fundort (gespeicherter Autor, Rechner und
    /// Herkunft);
    /// ohne startbaren Fundort keine.
    fn apply_state(&mut self, i: usize, states: &BTreeMap<String, PluginState>) {
        let Some(c) = states.get(&self.entries[i].id) else {
            return;
        };
        self.slots[i].enabled = c.enabled;
        let Some(found) = active_found(&self.entries[i]) else {
            return;
        };
        let mut loaded = Rights::default();
        for (id, grant) in &c.rights {
            let answer = match grant.as_str() {
                "allowed" => Answer::Allow,
                "denied" => Answer::Deny,
                _ => continue,
            };
            if let Some(r) = Right::from_id(id) {
                loaded.answer(r, answer);
            }
        }
        let saved_source = c.source.as_deref().map(Source::from_config);
        let kept = loaded.carry_over_from(
            c.author.as_deref(),
            &c.net_hosts,
            saved_source.as_ref(),
            &found.manifest,
            &found.source(),
        );
        let kept: Vec<(Right, Grant)> = kept.saved().collect();
        for (r, g) in kept {
            let answer = match g {
                Grant::Allowed => Answer::Allow,
                Grant::Denied => Answer::Deny,
            };
            self.entries[i].rights.answer(r, answer);
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
            // Autor, Rechner und Herkunft des laufenden Fundorts (ohne startbaren die gespeicherten).
            let (author, net_hosts, source) = match active_found(e) {
                Some(f) => (
                    Some(f.manifest.author.clone()),
                    f.manifest.net_hosts.clone(),
                    Some(f.source().to_config()),
                ),
                None => out
                    .get(&e.id)
                    .map(|c| (c.author.clone(), c.net_hosts.clone(), c.source.clone()))
                    .unwrap_or_default(),
            };
            let c = PluginState {
                enabled: s.enabled,
                rights,
                author,
                net_hosts,
                source,
            };
            if c.enabled && c.rights.is_empty() {
                out.remove(&e.id);
            } else {
                out.insert(e.id.clone(), c);
            }
        }
        out
    }

    /// Holt fertig gestartete Instanzen ab, stellt wartende Ergebnisse zu (`on-result`, Frist:
    /// Oberfläche) und gibt alle angefallenen Ereignisse zurück.
    ///
    /// Je Plug-in gilt ein Budget: höchstens [`MAX_DELIVERIES_PER_POLL`] Zustellungen und
    /// zusammen [`POLL_BUDGET`] (eine angefangene Zustellung läuft bis zu ihrer Frist); der Rest
    /// kommt beim nächsten `poll`. Bleibt in [`MAX_OVERLOADED_POLLS`] Aufrufen hintereinander
    /// Arbeit liegen, weil die Zeit nicht reichte, fällt das Plug-in aus („zu viel Arbeit“).
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
            let mut due = lock(&mb).take_deliveries().into_iter();
            let start = std::time::Instant::now();
            let mut delivered = 0;
            let mut alive = true;
            for (request, result) in due.by_ref() {
                if !self.call(i, |inst| inst.on_result(request, result)) {
                    alive = false;
                    break;
                }
                delivered += 1;
                if delivered >= MAX_DELIVERIES_PER_POLL || start.elapsed() >= POLL_BUDGET {
                    break;
                }
            }
            if !alive {
                continue;
            }
            let mut mb = lock(&mb);
            mb.requeue(due.collect());
            let overloaded = start.elapsed() >= POLL_BUDGET && !mb.deliveries.is_empty();
            drop(mb);
            let slot = &mut self.slots[i];
            slot.overloaded = if overloaded {
                slot.overloaded.saturating_add(1)
            } else {
                0
            };
            if slot.overloaded >= MAX_OVERLOADED_POLLS {
                self.fail(i, tr!("plugin-too-much-work"));
            }
        }
        std::mem::take(&mut self.outbox)
    }

    /// Führt den Befehl `command` des laufenden Plug-ins `plugin` aus (beide Kennungen werden
    /// genau verglichen; eine Kennung mit Punkten lässt sich so nicht mit einer anderen
    /// verwechseln).
    pub fn run_command(&mut self, plugin: &str, command: &str) {
        if let Some(i) = command_target(&self.entries, &self.slots, plugin, command) {
            self.call(i, |inst| inst.run_command(command));
        }
    }

    /// Meldet ein Ereignis eines Dialogs an sein Plug-in.
    pub fn dialog_event(&mut self, plugin: &str, dialog: &str, ev: DialogEvent) {
        if let Some(i) = self.running(plugin) {
            self.call(i, |inst| inst.handle_dialog(dialog, ev));
        }
    }

    /// Die Antwort des Nutzers auf eine Nachfrage ([`HostEvent::Ask`]). Gehört sie zu einer
    /// anderen Generation oder ist sie unbekannt, wird sie still verworfen. Sie gilt für die
    /// nachfragende Anfrage und die bis dahin wartenden desselben Rechts; „Nur diesmal“
    /// ([`Answer::Once`]) wird nicht vermerkt, die nächste Anfrage fragt wieder.
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
        self.outbox.extend(mb.take_events());
    }

    /// Nimmt die dauerhafte Entscheidung über ein Recht zurück: beim nächsten Antrag
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

    /// Vergisst den gespeicherten Stand von `id` ganz (nach „Entfernen“ und vor einer
    /// Neuinstallation): kein Eintrag mehr im Stand aus der Konfiguration, ein noch gefundenes
    /// Plug-in dieser Kennung (etwa das mitgelieferte unter dem entfernten) hat keine Rechte mehr
    /// und ist eingeschaltet. Meldet [`HostEvent::Changed`].
    pub fn forget_state(&mut self, id: &str) {
        self.base.remove(id);
        if let Some(i) = self.index(id) {
            self.entries[i].rights = Rights::default();
            self.slots[i].enabled = true;
            if let Some(mb) = &self.slots[i].mailbox {
                lock(mb).rights = Rights::default();
            }
        }
        self.outbox.push(HostEvent::Changed);
    }

    /// Nach dem Ersetzen von `id` (Installieren über ein vorhandenes, nach [`rescan`]): die
    /// Rechte nach [`Rights::carry_over`] gegen den vorigen Fundort `old` (Manifest und Herkunft,
    /// vor dem Installieren gemerkt) abgleichen und das
    /// Plug-in neu starten, damit der neue Code läuft, auch bei gleicher Version (die `rescan`
    /// weiterlaufen lässt). Lädt es schon neu, bleibt es dabei. Meldet [`HostEvent::Changed`].
    ///
    /// [`rescan`]: Self::rescan
    pub fn replaced(&mut self, id: &str, old: Option<&Found>) {
        let Some(i) = self.index(id) else {
            return;
        };
        let Some(new) = active_found(&self.entries[i]).cloned() else {
            return;
        };
        let old = old.map(|f| (&f.manifest, f.source()));
        let rights = self.entries[i].rights.carry_over(
            old.as_ref().map(|(m, s)| (*m, s)),
            &new.manifest,
            &new.source(),
        );
        if let Some(mb) = &self.slots[i].mailbox {
            lock(mb).rights = rights.clone();
        }
        self.entries[i].rights = rights;
        self.outbox.push(HostEvent::Changed);
        self.restart(id);
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
                    command: c.id.clone(),
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

    /// Generation der laufenden Instanz von `id` (jeder Start zählt neu); `None`, wenn es nicht
    /// läuft.
    pub fn generation(&self, id: &str) -> Option<u64> {
        self.running(id).map(|i| self.slots[i].generation)
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
            let events = lock(&mb).take_events();
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
        let engine = self
            .engine
            .get_or_insert_with(|| Engine::new().map(Arc::new).map_err(|e| e.to_string()));
        let engine = match engine {
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
        let mailbox = Arc::new(Mutex::new(Mailbox::new(
            self.entries[i].id.clone(),
            generation,
            manifest.clone(),
            self.entries[i].rights.clone(),
        )));
        let slot = &mut self.slots[i];
        slot.generation = generation;
        slot.overloaded = 0;
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
            self.outbox.extend(lock(mb).take_events());
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
    if let Err(e) = check_registration(&registration) {
        instance.shutdown();
        return Err(e);
    }
    Ok((instance, registration, PluginTexts::new(&languages)))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn mailbox() -> Mailbox {
        let manifest = Manifest::parse(
            "id = \"a.b\"\nname = \"A\"\nversion = \"0.1.0\"\napi = \"0.1\"\nauthor = \"x\"\n\
             license = \"MIT\"\ndescription = \"d\"\nrights = [\"net\"]\nnet-hosts = [\"example.org\"]\n",
        )
        .unwrap();
        Mailbox::new("a.b".into(), 1, manifest, Rights::default())
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

#[cfg(test)]
mod grenzen {
    use super::tests::mailbox;
    use super::*;
    use crate::runtime::Field;

    #[test]
    fn long_texts_are_clipped() {
        assert_eq!(clip("kurz"), "kurz");
        let long = "ä".repeat(MAX_TEXT);
        let c = clip(&long);
        assert!(c.len() <= MAX_TEXT, "{}", c.len());
        assert!(c.ends_with('\u{2026}'));
        let mut r = Recorder(Arc::new(Mutex::new(mailbox())));
        r.message(Level::Info, &"x".repeat(100_000));
        r.log(Level::Info, &"y".repeat(100_000));
        let mb = lock(&r.0);
        for e in &mb.events {
            match e {
                HostEvent::Message { text, .. } | HostEvent::Log { text, .. } => {
                    assert!(text.len() <= MAX_TEXT)
                }
                other => panic!("{other:?}"),
            }
        }
    }

    fn feld(id: &str) -> Field {
        Field {
            id: id.into(),
            label_key: "k".into(),
            kind: FieldKind::Text("t".into()),
        }
    }

    fn dialog(fields: Vec<Field>) -> Dialog {
        Dialog {
            id: "d".into(),
            title_key: "t".into(),
            fields,
        }
    }

    #[test]
    fn dialogs_are_checked() {
        assert!(check_dialog(&dialog(vec![feld("a"), feld("b")])).is_ok());
        assert!(check_dialog(&dialog(vec![feld("a"), feld("a")])).is_err());
        assert!(check_dialog(&dialog(vec![feld("")])).is_err());
        let viele = (0..=MAX_FIELDS).map(|n| feld(&n.to_string())).collect();
        assert!(check_dialog(&dialog(viele)).is_err());
        let genug = (0..MAX_FIELDS).map(|n| feld(&n.to_string())).collect();
        assert!(check_dialog(&dialog(genug)).is_ok());
        let mut lang = feld("a");
        lang.kind = FieldKind::Text("x".repeat(MAX_DIALOG_STRING + 1));
        assert!(check_dialog(&dialog(vec![lang])).is_err());
        let mut d = dialog(vec![]);
        d.title_key = "x".repeat(MAX_DIALOG_STRING + 1);
        assert!(check_dialog(&d).is_err());
        // Über den Host-Aufruf: `open-dialog` gibt den Fehler zurück, nichts landet im Postfach.
        let mut r = Recorder(Arc::new(Mutex::new(mailbox())));
        assert!(r.open_dialog(dialog(vec![feld("a"), feld("a")])).is_err());
        assert!(lock(&r.0).events.is_empty());
    }

    fn befehl(id: &str) -> Command {
        Command {
            id: id.into(),
            label_key: "k".into(),
            icon: None,
            shortcut: None,
        }
    }

    #[test]
    fn registration_is_checked() {
        let ok = Registration {
            commands: (0..MAX_COMMANDS).map(|n| befehl(&n.to_string())).collect(),
        };
        assert!(check_registration(&ok).is_ok());
        let viele = Registration {
            commands: (0..=MAX_COMMANDS).map(|n| befehl(&n.to_string())).collect(),
        };
        assert!(check_registration(&viele).is_err());
        let mut lang = befehl("a");
        lang.shortcut = Some("x".repeat(MAX_COMMAND_STRING + 1));
        assert!(
            check_registration(&Registration {
                commands: vec![lang]
            })
            .is_err()
        );
    }

    #[test]
    fn mailbox_is_capped_in_bytes() {
        let mut mb = mailbox();
        // Direkt ins Postfach (ohne Kürzen): 1024 × 16 KiB wären 16 MiB.
        let big = "x".repeat(16 << 10);
        for _ in 0..MAX_QUEUE {
            mb.event(HostEvent::Message {
                plugin: "a.b".into(),
                level: Level::Info,
                text: big.clone(),
            });
        }
        // Über 8 MiB: verworfen, genau ein Hinweis.
        let hinweise = mb
            .events
            .iter()
            .filter(|e| matches!(e, HostEvent::Log { .. }))
            .count();
        assert_eq!(hinweise, 1);
        assert!(mb.bytes <= MAX_MAILBOX_BYTES);
        assert!(
            mb.events.len() < MAX_QUEUE,
            "nach Bytes begrenzt, nicht nach Zahl"
        );
        // Ein großes Netz-Ergebnis zählt nicht mit (begrenzt durch MAX_NET und das Programm).
        mb.deliver(
            7,
            RequestResult::Http(runtime::HttpResponse {
                status: 200,
                body: vec![0; 16 << 20],
            }),
        );
        assert!(matches!(mb.deliveries[..], [(7, RequestResult::Http(_))]));
        // Abgeholt ist wieder Platz.
        mb.take_events();
        assert_eq!(mb.bytes, 0);
    }

    #[test]
    fn at_most_four_net_requests() {
        let mut mb = mailbox();
        for n in 1..=(MAX_NET as u64 + 2) {
            mb.net(n, "https://example.org/".into());
        }
        assert_eq!(mb.net_open.len(), MAX_NET);
        let failed: Vec<u64> = mb
            .deliveries
            .iter()
            .filter(|(_, r)| matches!(r, RequestResult::Failed(_)))
            .map(|(n, _)| *n)
            .collect();
        assert_eq!(failed, vec![MAX_NET as u64 + 1, MAX_NET as u64 + 2]);
    }

    #[test]
    fn commands_matched_exactly() {
        // `a.b` mit Befehl `c.x` und `a.b.c` mit Befehl `x` ergeben beide „a.b.c.x“.
        let entry = |id: &str| PluginEntry {
            id: id.into(),
            found: Vec::new(),
            active: None,
            state: EntryState::Running,
            rights: Rights::default(),
            failures: 0,
        };
        let slot = |cmd: &str| Slot {
            commands: vec![befehl(cmd)],
            ..Slot::new()
        };
        let entries = vec![entry("a.b"), entry("a.b.c")];
        let slots = vec![slot("c.x"), slot("x")];
        assert_eq!(command_target(&entries, &slots, "a.b.c", "x"), Some(1));
        assert_eq!(command_target(&entries, &slots, "a.b", "c.x"), Some(0));
        assert_eq!(command_target(&entries, &slots, "a.b", "x"), None);
        assert_eq!(command_target(&entries, &slots, "a", "b.c.x"), None);
    }
}
