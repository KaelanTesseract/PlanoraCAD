//! Die Laufzeit der Plug-ins: lädt eine WebAssembly-Komponente, führt ihre Aufrufe aus und hält sie in
//! Grenzen.
//!
//! Abschottung: Das Plug-in sieht nur die Schnittstellen aus `plugin.wit` und ein leeres WASI (keine
//! Ordner, keine Umgebungsvariablen, kein Netz, Wanduhr fest auf 0, Monotonuhr auf 1 ms gerundet).
//! Jeder Aufruf hat eine Frist (Epochen-Unterbrechung im 10-ms-Takt), der Arbeitsspeicher ist begrenzt.
//! Nach einem Ausfall (Absturz, Frist, Speichergrenze) wird die Instanz nicht mehr ausgeführt.

use crate::bindings::Plugin;
use crate::bindings::planora::plugin::{i18n, log, net, rights, types, ui};
use crate::manifest::Manifest;
use crate::texts::PluginTexts;
use bytes::Bytes;
use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::io::AsyncWrite;
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Store, StoreLimits, StoreLimitsBuilder, Trap};
use wasmtime_wasi::cli::{IsTerminal, StdoutStream};
use wasmtime_wasi::clocks::{HostMonotonicClock, HostWallClock};
use wasmtime_wasi::p2::{OutputStream, Pollable, StreamError};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

pub use crate::bindings::planora::plugin::types::{
    Command, Dialog, DialogEvent, Field, FieldKind, FieldValue, HostInfo, HttpResponse, Level,
    Registration, RequestResult, Right,
};

/// Länge eines Takts der Epochen-Uhr.
const TICK: Duration = Duration::from_millis(10);
/// Frist für Aufrufe, die nur die Oberfläche bedienen (200 ms).
const UI_TICKS: u64 = 20;
/// Frist für Befehle und `init` (5 s).
const COMMAND_TICKS: u64 = 500;
/// Mehr Ausgabe (stdout/stderr) als das merkt sich die Laufzeit zwischen zwei Aufrufen nicht.
const OUTPUT_CAP: usize = 64 << 10;

/// Warum ein Aufruf nicht zum Ergebnis kam.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PluginError {
    /// Das Plug-in ließ sich nicht laden oder die Instanz ist nach einem Ausfall nicht mehr benutzbar.
    #[error("{0}")]
    Load(String),
    /// Das Plug-in ist abgestürzt (WebAssembly-Trap).
    #[error("Das Plug-in ist abgestürzt: {0}")]
    Trap(String),
    /// Das Plug-in hat seine Frist überschritten.
    #[error("Das Plug-in hat zu lange gebraucht.")]
    Timeout,
    /// Das Plug-in wollte mehr Arbeitsspeicher, als das Manifest erlaubt.
    #[error("Das Plug-in braucht mehr Arbeitsspeicher als erlaubt.")]
    OutOfMemory,
    /// Das Plug-in hat selbst einen Fehler gemeldet (Text aus `result::err`); kein Ausfall.
    #[error("{0}")]
    Plugin(String),
}

/// Welche Frist ein Aufruf hat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    /// Bedienung der Oberfläche: 20 Takte (200 ms).
    Ui,
    /// Befehle und Start: 500 Takte (5 s).
    Command,
}

impl CallKind {
    /// Die Frist in Takten zu 10 ms.
    pub fn ticks(self) -> u64 {
        match self {
            CallKind::Ui => UI_TICKS,
            CallKind::Command => COMMAND_TICKS,
        }
    }
}

/// Was die Laufzeit vom Programm braucht. `i18n.text` beantwortet die Laufzeit selbst.
pub trait HostCalls: Send {
    /// Ein Protokolleintrag des Plug-ins (auch seine Ausgabe auf stdout/stderr).
    fn log(&mut self, level: Level, text: &str);
    /// Eine Meldung an den Nutzer.
    fn message(&mut self, level: Level, text: &str);
    /// Ein Dialog soll geöffnet werden.
    fn open_dialog(&mut self, d: Dialog) -> Result<(), String>;
    /// Ein Dialog soll geschlossen werden.
    fn close_dialog(&mut self, id: &str);
    /// Das Plug-in bittet um ein Recht; die Kennung des Antrags kommt später in `on_result` zurück.
    fn request_right(&mut self, r: Right) -> u64;
    /// Ist das Recht schon erteilt?
    fn right_granted(&self, r: Right) -> bool;
    /// Eine HTTP-Abfrage; das Ergebnis kommt später in `on_result` zurück.
    fn net_get(&mut self, url: &str) -> Result<u64, String>;
}

/// Zwischenspeicher für die Ausgabe des Plug-ins (stdout oder stderr), bis die Laufzeit sie an
/// [`HostCalls::log`] weitergibt.
#[derive(Clone, Default)]
struct Ausgabe(Arc<Mutex<Vec<u8>>>);

impl Ausgabe {
    fn append(&self, bytes: &[u8]) {
        let mut buf = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let platz = OUTPUT_CAP.saturating_sub(buf.len());
        buf.extend_from_slice(&bytes[..bytes.len().min(platz)]);
    }

    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl IsTerminal for Ausgabe {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for Ausgabe {
    fn async_stream(&self) -> Box<dyn AsyncWrite + Send + Sync> {
        Box::new(self.clone())
    }

    fn p2_stream(&self) -> Box<dyn OutputStream> {
        Box::new(self.clone())
    }
}

impl AsyncWrite for Ausgabe {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.append(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl OutputStream for Ausgabe {
    fn write(&mut self, bytes: Bytes) -> Result<(), StreamError> {
        self.append(&bytes);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), StreamError> {
        Ok(())
    }

    fn check_write(&mut self) -> Result<usize, StreamError> {
        Ok(OUTPUT_CAP)
    }
}

#[async_trait::async_trait]
impl Pollable for Ausgabe {
    async fn ready(&mut self) {}
}

/// Die Wanduhr des Plug-ins steht fest auf 0 (1970), damit es keine Uhrzeit erfährt.
struct FesteWanduhr;

impl HostWallClock for FesteWanduhr {
    fn resolution(&self) -> Duration {
        Duration::from_secs(1)
    }

    fn now(&self) -> Duration {
        Duration::ZERO
    }
}

/// Die Monotonuhr des Plug-ins läuft ab dem Start der Instanz, auf 1 ms gerundet (gegen Zeitmessung
/// im Mikrosekundenbereich).
struct GrobeMonotonuhr(std::time::Instant);

impl HostMonotonicClock for GrobeMonotonuhr {
    fn resolution(&self) -> u64 {
        1_000_000
    }

    fn now(&self) -> u64 {
        let ms = self.0.elapsed().as_millis();
        u64::try_from(ms.saturating_mul(1_000_000)).unwrap_or(u64::MAX)
    }
}

/// Der Zustand eines Plug-ins im `Store`.
struct State {
    host: Box<dyn HostCalls>,
    wasi: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
    texts: PluginTexts,
    language: String,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl types::Host for State {}

impl log::Host for State {
    fn write(&mut self, level: Level, text: String) {
        self.host.log(level, &text);
    }
}

impl i18n::Host for State {
    fn text(&mut self, key: String, args: Vec<(String, String)>) -> String {
        self.texts.text(&self.language, &key, &args)
    }
}

impl ui::Host for State {
    fn message(&mut self, level: Level, text: String) {
        self.host.message(level, &text);
    }

    fn open_dialog(&mut self, d: Dialog) -> Result<(), String> {
        self.host.open_dialog(d)
    }

    fn close_dialog(&mut self, id: String) {
        self.host.close_dialog(&id);
    }
}

impl rights::Host for State {
    fn request(&mut self, r: Right) -> u64 {
        self.host.request_right(r)
    }

    fn granted(&mut self, r: Right) -> bool {
        self.host.right_granted(r)
    }
}

impl net::Host for State {
    fn get(&mut self, url: String) -> Result<u64, String> {
        self.host.net_get(&url)
    }
}

/// Der Epochen-Taktgeber: erhöht alle 10 ms die Epoche der Engine, bis er fallen gelassen wird.
struct Ticker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Ticker {
    fn start(engine: wasmtime::Engine) -> Result<Ticker, PluginError> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("plugin-epochen".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(TICK);
                    engine.increment_epoch();
                }
            })
            .map_err(|e| {
                PluginError::Load(format!("Der Taktgeber ließ sich nicht starten: {e}"))
            })?;
        Ok(Ticker {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Ticker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Die Laufzeit: übersetzt Plug-ins und startet Instanzen. Der Taktgeber läuft, solange die Engine oder
/// eine ihrer Instanzen lebt.
pub struct Engine {
    inner: wasmtime::Engine,
    ticker: Arc<Ticker>,
}

impl Engine {
    /// Richtet die Laufzeit ein (Cranelift, Komponentenmodell, Epochen-Unterbrechung).
    pub fn new() -> Result<Engine, PluginError> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        config.epoch_interruption(true);
        let inner = wasmtime::Engine::new(&config).map_err(|e| {
            PluginError::Load(format!("Die Laufzeit ließ sich nicht einrichten: {e}"))
        })?;
        let ticker = Arc::new(Ticker::start(inner.clone())?);
        Ok(Engine { inner, ticker })
    }

    /// Übersetzt `wasm` und legt eine Instanz an. `languages` sind die Sprachdateien des Pakets
    /// (`Package::languages`), `host` nimmt die Aufrufe des Plug-ins entgegen.
    pub fn instantiate(
        &self,
        manifest: &Manifest,
        wasm: &[u8],
        languages: &BTreeMap<String, String>,
        host: Box<dyn HostCalls>,
    ) -> Result<Instance, PluginError> {
        if manifest.api != planora_plugin_api::API_VERSION {
            return Err(PluginError::Load(format!(
                "Das Plug-in „{}“ braucht PlanoraCAD-Schnittstelle {}, dieses Programm spricht {}.",
                manifest.name,
                manifest.api,
                planora_plugin_api::API_VERSION
            )));
        }
        let load = |what: &str, e: wasmtime::Error| {
            PluginError::Load(format!(
                "Das Plug-in „{}“ ließ sich nicht {what}: {e:#}",
                manifest.name
            ))
        };
        let component = Component::new(&self.inner, wasm).map_err(|e| load("laden", e))?;
        let mut linker = Linker::<State>::new(&self.inner);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|e| load("einrichten", e))?;
        Plugin::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
            .map_err(|e| load("einrichten", e))?;

        let stdout = Ausgabe::default();
        let stderr = Ausgabe::default();
        // Leeres WASI: keine Ordner, keine Umgebung, keine Argumente, kein Netz.
        let wasi = WasiCtxBuilder::new()
            .stdout(stdout.clone())
            .stderr(stderr.clone())
            .wall_clock(FesteWanduhr)
            .monotonic_clock(GrobeMonotonuhr(std::time::Instant::now()))
            .build();
        let limits = StoreLimitsBuilder::new()
            .memory_size((manifest.memory_mb as usize) << 20)
            .trap_on_grow_failure(true)
            .build();
        let state = State {
            host,
            wasi,
            table: ResourceTable::new(),
            limits,
            texts: PluginTexts::new(languages),
            language: "en".to_string(),
        };
        let mut store = Store::new(&self.inner, state);
        store.limiter(|s| &mut s.limits);
        // Auch das Einrichten der Instanz hat eine Frist (ein Start-Abschnitt könnte endlos laufen).
        store.set_epoch_deadline(CallKind::Command.ticks());
        let plugin = Plugin::instantiate(&mut store, &component, &linker).map_err(|e| {
            match classify(&e) {
                PluginError::Trap(_) => load("einrichten", e),
                other => other,
            }
        })?;
        Ok(Instance {
            store,
            plugin,
            dead: false,
            stdout,
            stderr,
            _ticker: Arc::clone(&self.ticker),
        })
    }
}

/// Ordnet einen Fehler der Laufzeit ein.
fn classify(e: &wasmtime::Error) -> PluginError {
    if e.downcast_ref::<Trap>() == Some(&Trap::Interrupt) {
        return PluginError::Timeout;
    }
    let text = format!("{e:#}");
    // Der Trap-Grund der Speichergrenze („forcing trap when growing memory to N bytes“).
    if text.contains("forcing trap when growing memory") || text.contains("memory growth failure") {
        return PluginError::OutOfMemory;
    }
    PluginError::Trap(text)
}

/// Eine laufende Plug-in-Instanz. Nach einem Ausfall ist sie „tot“: jeder weitere Aufruf gibt
/// `Err(Load(_))` zurück, ohne WebAssembly auszuführen.
pub struct Instance {
    store: Store<State>,
    plugin: Plugin,
    dead: bool,
    stdout: Ausgabe,
    stderr: Ausgabe,
    _ticker: Arc<Ticker>,
}

impl Instance {
    /// Führt einen Aufruf mit Frist aus und ordnet den Ausgang ein.
    fn call<T>(
        &mut self,
        kind: CallKind,
        f: impl FnOnce(&Plugin, &mut Store<State>) -> wasmtime::Result<T>,
    ) -> Result<T, PluginError> {
        if self.dead {
            return Err(PluginError::Load(
                "Das Plug-in ist ausgefallen und wird nicht mehr aufgerufen.".to_string(),
            ));
        }
        self.store.set_epoch_deadline(kind.ticks());
        let result = f(&self.plugin, &mut self.store);
        self.forward_output();
        result.map_err(|e| {
            self.dead = true;
            classify(&e)
        })
    }

    /// Gibt gesammelte Ausgabe des Plug-ins zeilenweise an `HostCalls::log` weiter.
    fn forward_output(&mut self) {
        for (buffer, level) in [(&self.stdout, Level::Info), (&self.stderr, Level::Warning)] {
            let bytes = buffer.take();
            if bytes.is_empty() {
                continue;
            }
            for line in String::from_utf8_lossy(&bytes).lines() {
                self.store.data_mut().host.log(level, line);
            }
        }
    }

    /// Startet das Plug-in und holt seine Befehle ab (Frist: Befehl).
    pub fn init(&mut self, info: HostInfo) -> Result<Registration, PluginError> {
        self.store.data_mut().language = info.language.clone();
        self.call(CallKind::Command, |p, s| p.call_init(s, &info))?
            .map_err(PluginError::Plugin)
    }

    /// Führt einen Befehl aus (Frist: Befehl).
    pub fn run_command(&mut self, id: &str) -> Result<(), PluginError> {
        self.call(CallKind::Command, |p, s| p.call_run_command(s, id))?
            .map_err(PluginError::Plugin)
    }

    /// Meldet ein Ereignis eines Dialogs (Frist: Oberfläche).
    pub fn handle_dialog(&mut self, id: &str, ev: DialogEvent) -> Result<(), PluginError> {
        self.call(CallKind::Ui, |p, s| p.call_handle_dialog(s, id, &ev))?
            .map_err(PluginError::Plugin)
    }

    /// Liefert das Ergebnis eines Antrags (Recht oder Netzabfrage) ab (Frist: Oberfläche).
    pub fn on_result(&mut self, id: u64, r: RequestResult) -> Result<(), PluginError> {
        self.call(CallKind::Ui, |p, s| p.call_on_result(s, id, &r))
    }

    /// Beendet das Plug-in (Frist: Oberfläche). Fehler werden nur protokolliert.
    pub fn shutdown(&mut self) {
        if let Err(e) = self.call(CallKind::Ui, |p, s| p.call_shutdown(s)) {
            self.store
                .data_mut()
                .host
                .log(Level::Error, &format!("Beim Beenden des Plug-ins: {e}"));
        }
    }
}
