//! Update einspielen im Programm: Download und Prüfung des Installers im Hintergrund
//! ([`Installer`]), Start des Installers über eine austauschbare Schnittstelle
//! ([`SetupLauncher`]) und was beim Start nach einem Update geschieht ([`startup`]).
//!
//! **Ablauf.** „Jetzt aktualisieren“ → [`decide`] (nur Windows, nur eine installierte Fassung;
//! eine nicht installierte bietet nur die Download-Seite, [`NOT_INSTALLED_HINT`]) →
//! [`Installer::start`]: ein eigener Thread lädt die Prüfsummen-Datei und den Installer in den
//! Ordner [`download_dir`] und prüft die Summe (`Downloading` → `ReadyToRestart`, sonst
//! `Failed`); die erwartete Summe bleibt gemerkt. Weiter geht es erst, wenn nichts mehr offen ist
//! (keine Rückfrage, kein Fenster, kein schreibender Export, kein Autosicherungs-Angebot; bis
//! dahin [`WAITING`]). Dann fragt die App wie beim Schließen nach ungespeicherter Arbeit
//! (`Workspace::request_quit`); „Abbrechen“ dort bricht das Update ab und löscht den Download.
//! Sonst: den Installer im Hintergrund noch einmal gegen die gemerkte Summe prüfen
//! ([`reverify`]), `update_pending` samt Fenster speichern (gelingt das nicht, startet nichts:
//! [`PENDING_NOT_SAVED`]), die Patchnotes hinterlegen und den Installer starten
//! ([`SetupLauncher::launch`], still, mit [`setup_args`]). Startet er, beendet sich das Programm
//! ohne weitere Rückfrage (sauber, wie beim Schließen); der Installer wartet darauf, ersetzt die
//! Dateien und startet die neue Version (nur mit [`SETUP_RESTART_PARAM`]). Lehnt jemand die
//! Admin-Abfrage von Windows ab ([`UPDATE_CANCELLED`]) oder startet der Installer nicht
//! ([`launch_failed`]), läuft das Programm in der bisherigen Version weiter und bietet die
//! Version weiter an. Von der zweiten Prüfung bis zum Ende ist die Bedienung gesperrt
//! ([`RESTART_BOX`]). Die Schritte nach dem Download stehen in [`crate::app`]
//! (`PlanoraApp::install_tick`).
//!
//! **Start.** [`startup`] löscht `update_pending` und einen liegengebliebenen Download und zeigt
//! nach einem gelungenen Update (das Ziel in `update_pending` ist die laufende Version) einmal
//! „Aktualisiert auf Version X“ mit den Patchnotes aus der Zwischenablage [`NOTES_FILE_NAME`].
//! Das Fenster öffnet dann in Größe und Lage der alten Version ([`window_for_start`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;

use crate::install_mode::InstallMode;
use crate::workspace::Workspace;
use planora_project::config::{Config, WindowState};
use planora_update::install::{
    DownloadPaths, Downloader, InstallError, MAX_CHECKSUM_BYTES, checksum_hex, discard_download,
    download_limit, file_matches_sha256, prepare_dir, release_url_ok,
};
use planora_update::{REPO, SETUP_NAME, Update, Version};

/// Statuszeile und Meldung, solange ein fertiges Update auf das Ende der laufenden Arbeit wartet.
pub const WAITING: &str = "Update ist bereit und wartet, bis du fertig bist.";

/// Zwischenablage der Patchnotes der neuen Version neben der Konfiguration (erste Zeile die
/// Version, dann die Notizen); die neue Version liest und löscht sie beim Start.
pub const NOTES_FILE_NAME: &str = "planoracad-update-notes.md";

/// Größte gelesene Zwischenablage der Patchnotes.
const MAX_NOTES_BYTES: u64 = 64 * 1024;

/// Name des Download-Ordners im Temp-Ordner.
pub const DOWNLOAD_DIR_NAME: &str = "PlanoraCAD-Update";

/// Name des Protokolls, das der Installer im Temp-Ordner schreibt.
pub const SETUP_LOG_NAME: &str = "planoracad-update.log";

/// Hinweis, wenn das Programm nicht unter Windows läuft.
pub const ONLY_WINDOWS: &str = "Automatisches Aktualisieren gibt es nur unter Windows.";
/// Knopf: öffnet die Release-Seite auf GitHub.
pub const OPEN_DOWNLOAD_PAGE: &str = "Download-Seite öffnen";
/// Meldung, wenn `update_pending` nicht gespeichert werden kann.
pub const PENDING_NOT_SAVED: &str = "Das Update ließ sich nicht vormerken, weil die Einstellungen nicht gespeichert werden können. Es wurde nichts installiert.";
/// Meldung, solange ein abgebrochener Download noch endet.
pub const STILL_STOPPING: &str =
    "Der abgebrochene Download endet gerade noch. Bitte gleich noch einmal versuchen.";
/// Meldung, wenn die Admin-Abfrage von Windows abgelehnt wurde.
pub const UPDATE_CANCELLED: &str = "Update abgebrochen. Ohne die Erlaubnis von Windows lässt sich PlanoraCAD nicht für alle Benutzer aktualisieren. Du arbeitest in der bisherigen Version weiter.";
/// Hinweis in der Meldung, wenn diese Fassung nicht installiert ist (etwa aus dem Bauordner).
pub const NOT_INSTALLED_HINT: &str = "Diese Fassung von PlanoraCAD ist nicht installiert und kann sich nicht selbst aktualisieren. Lade die neue Version auf der Download-Seite herunter.";

/// Meldung, wenn der Installer nicht startet (`detail`: der Grund von Windows).
pub fn launch_failed(detail: &str) -> String {
    format!(
        "Das Update ließ sich nicht starten ({detail}). Du arbeitest in der bisherigen Version weiter."
    )
}

/// Titel des Hinweises nach einem Update: „Aktualisiert auf Version 0.2.0-dev.2“.
pub fn updated_title(v: &Version) -> String {
    format!("Aktualisiert auf Version {v}")
}

/// Release-Seite einer Version auf GitHub.
pub fn release_page(tag: &str) -> String {
    format!("https://github.com/{REPO}/releases/tag/{tag}")
}

/// Ordner des Downloads: [`DOWNLOAD_DIR_NAME`] im Temp-Ordner des Benutzers.
pub fn download_dir() -> PathBuf {
    std::env::temp_dir().join(DOWNLOAD_DIR_NAME)
}

/// Ort des Protokolls des Installers: [`SETUP_LOG_NAME`] im Temp-Ordner des Benutzers.
pub fn setup_log_path() -> PathBuf {
    std::env::temp_dir().join(SETUP_LOG_NAME)
}

/// Zustand des Downloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallState {
    /// Nichts läuft.
    Idle,
    /// Der Installer wird geladen (Bytes geladen, gesamt).
    Downloading { done: u64, total: u64 },
    /// Der Installer liegt da und die Prüfsumme stimmt; jetzt der Neustart.
    ReadyToRestart,
    /// Gescheitert; die Meldung in einfacher Sprache.
    Failed(String),
}

/// Was „Jetzt aktualisieren“ tut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallDecision {
    /// Den Installer laden und starten.
    Download,
    /// Nicht unter Windows: Hinweis [`ONLY_WINDOWS`] und die Release-Seite öffnen.
    OnlyWindows,
    /// Nicht installiert (etwa aus dem Bauordner): die Meldung bietet nur
    /// [`OPEN_DOWNLOAD_PAGE`] ([`NOT_INSTALLED_HINT`]).
    NotInstalled,
}

/// Entscheidung zu „Jetzt aktualisieren“ ohne `cfg`: `auto_supported` ist im Programm
/// `cfg!(windows)`, `mode` der Install-Modus aus [`crate::install_mode::detect`].
pub fn decide(auto_supported: bool, mode: Option<InstallMode>) -> InstallDecision {
    match (auto_supported, mode) {
        (false, _) => InstallDecision::OnlyWindows,
        (true, None) => InstallDecision::NotInstalled,
        (true, Some(_)) => InstallDecision::Download,
    }
}

enum Msg {
    Progress(u64, u64),
    /// Fertig; bei Erfolg mit der geprüften Summe.
    Done(Result<String, InstallError>),
}

/// Download und Prüfung im Hintergrund; höchstens einer zur Zeit. Die Oberfläche fragt den
/// Stand in jedem Bild ab ([`Installer::poll`]), ohne zu warten.
pub struct Installer {
    state: InstallState,
    rx: Option<Receiver<Msg>>,
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    repaint: Option<egui::Context>,
    paths: Option<DownloadPaths>,
    target: Option<Version>,
    /// Die geprüfte Summe des Installers (für die zweite Prüfung vor dem Start).
    verified: Option<String>,
}

impl Default for Installer {
    fn default() -> Self {
        Installer {
            state: InstallState::Idle,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            handle: None,
            repaint: None,
            paths: None,
            target: None,
            verified: None,
        }
    }
}

impl Installer {
    /// Hinterlegt den Kontext, dem der Thread bei jedem Fortschritt ein neues Bild meldet.
    pub fn set_repaint(&mut self, ctx: &egui::Context) {
        self.repaint = Some(ctx.clone());
    }

    pub fn state(&self) -> &InstallState {
        &self.state
    }

    /// Pfade des laufenden oder fertigen Downloads.
    pub fn paths(&self) -> Option<&DownloadPaths> {
        self.paths.as_ref()
    }

    /// Version des laufenden oder fertigen Downloads.
    pub fn target(&self) -> Option<Version> {
        self.target
    }

    /// Die beim Download geprüfte SHA-256 des Installers (nur in `ReadyToRestart`).
    pub fn verified_sha256(&self) -> Option<&str> {
        match self.state {
            InstallState::ReadyToRestart => self.verified.as_deref(),
            _ => None,
        }
    }

    /// Läuft der Thread eines (auch abgebrochenen) Downloads noch?
    pub fn thread_running(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    /// Setzt den Zustand auf `Failed(message)`.
    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = InstallState::Failed(message.into());
    }

    /// Zurück auf `Idle` (nach dem Schließen einer Fehlermeldung).
    pub fn reset(&mut self) {
        if !matches!(self.state, InstallState::Downloading { .. }) {
            self.state = InstallState::Idle;
        }
    }

    /// Startet Download und Prüfung des Installers von `update` nach `paths.setup` in einem
    /// eigenen Thread. Vorher geprüft: Adressen aus dem Release und Größe ([`download_limit`]);
    /// ein liegengebliebener Download wird verworfen, der Ordner angelegt ([`prepare_dir`]).
    /// Läuft schon einer, bleibt es bei ihm.
    pub fn start(
        &mut self,
        update: &Update,
        paths: DownloadPaths,
        downloader: Arc<dyn Downloader + Send + Sync>,
    ) {
        if matches!(self.state, InstallState::Downloading { .. }) {
            return;
        }
        if self.thread_running() {
            self.fail(STILL_STOPPING);
            return;
        }
        let release = &update.latest;
        if !release_url_ok(&release.setup.url) || !release_url_ok(&release.sha256.url) {
            self.fail(InstallError::Network("Adresse gehört nicht zum Release".into()).message());
            return;
        }
        let limit = match download_limit(release.setup.size) {
            Ok(limit) => limit,
            Err(e) => return self.fail(e.message()),
        };
        discard_download(&paths);
        if let Err(e) = prepare_dir(&paths) {
            return self.fail(e.message());
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let repaint = self.repaint.clone();
        let size = release.setup.size;
        let (setup_url, sha_url) = (release.setup.url.clone(), release.sha256.url.clone());
        let job_paths = paths.clone();
        let job_cancel = Arc::clone(&cancel);
        let spawned = std::thread::Builder::new()
            .name("planora-update-download".into())
            .spawn(move || {
                let paths = job_paths;
                let cancel = job_cancel;
                let ping = || {
                    if let Some(ctx) = &repaint {
                        ctx.request_repaint();
                    }
                };
                let result = (|| {
                    let checksum = &paths.checksum;
                    downloader.download(&sha_url, checksum, MAX_CHECKSUM_BYTES, &mut |_, _| {
                        !cancel.load(Ordering::SeqCst)
                    })?;
                    let text =
                        std::fs::read(checksum).map_err(|e| InstallError::Io(e.to_string()))?;
                    let _ = std::fs::remove_file(checksum);
                    let text = String::from_utf8(text).map_err(|_| InstallError::Checksum)?;
                    let mut last = None;
                    downloader.download(&setup_url, &paths.setup, limit, &mut |done, total| {
                        let total = if total > 0 { total } else { size };
                        if last != Some(done) {
                            last = Some(done);
                            let _ = tx.send(Msg::Progress(done, total));
                            ping();
                        }
                        !cancel.load(Ordering::SeqCst)
                    })?;
                    let expected = checksum_hex(&text, SETUP_NAME)?;
                    file_matches_sha256(&paths.setup, &expected)?;
                    if cancel.load(Ordering::SeqCst) {
                        return Err(InstallError::Cancelled);
                    }
                    Ok(expected)
                })();
                if result.is_err() {
                    discard_download(&paths);
                }
                let _ = tx.send(Msg::Done(result));
                ping();
            });
        match spawned {
            Ok(handle) => {
                self.handle = Some(handle);
                self.rx = Some(rx);
                self.cancel = cancel;
                self.paths = Some(paths);
                self.target = Some(release.version);
                self.verified = None;
                self.state = InstallState::Downloading {
                    done: 0,
                    total: size,
                };
            }
            Err(e) => {
                discard_download(&paths);
                self.fail(InstallError::Io(e.to_string()).message());
            }
        }
    }

    /// Holt Fortschritt und Ergebnis ab; wartet nie.
    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        loop {
            match rx.try_recv() {
                Ok(Msg::Progress(done, total)) => {
                    self.state = InstallState::Downloading { done, total };
                }
                Ok(Msg::Done(result)) => {
                    self.rx = None;
                    self.state = match result {
                        Ok(hex) => {
                            self.verified = Some(hex);
                            InstallState::ReadyToRestart
                        }
                        Err(InstallError::Cancelled) => InstallState::Idle,
                        Err(e) => InstallState::Failed(e.message()),
                    };
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.state = InstallState::Failed(
                        InstallError::Io("Download abgebrochen".into()).message(),
                    );
                    return;
                }
            }
        }
    }

    /// Bricht ab: ein laufender Download hört beim nächsten Rückruf auf und löscht seine Datei;
    /// ein fertiger (`ReadyToRestart`) wird gelöscht. Danach `Idle`.
    pub fn cancel(&mut self) {
        match self.state {
            InstallState::Downloading { .. } => {
                self.cancel.store(true, Ordering::SeqCst);
                self.rx = None;
            }
            InstallState::ReadyToRestart => {
                if let Some(paths) = &self.paths {
                    discard_download(paths);
                }
            }
            InstallState::Idle | InstallState::Failed(_) => {}
        }
        self.verified = None;
        self.state = InstallState::Idle;
    }
}

/// Zweite Prüfung des Installers gegen die beim Download geprüfte Summe, unmittelbar vor dem
/// Start, in einem eigenen Thread (große Datei, die Oberfläche wartet nicht). Ein anderes
/// Programm hätte die Datei inzwischen verändern können (etwa eine zweite Instanz, die selbst
/// ein Update lädt). Ist ein Kontext da, meldet der Thread danach ein neues Bild. `gate` (nur in
/// Tests): der Thread wartet vor der Prüfung, bis dort etwas ankommt oder es geschlossen wird.
pub fn reverify(
    file: PathBuf,
    expected_hex: String,
    repaint: Option<egui::Context>,
    gate: Option<Receiver<()>>,
) -> Receiver<Result<(), InstallError>> {
    let (tx, rx) = mpsc::channel();
    let job_tx = tx.clone();
    let spawned = std::thread::Builder::new()
        .name("planora-update-pruefen".into())
        .spawn(move || {
            if let Some(gate) = gate {
                let _ = gate.recv();
            }
            let _ = job_tx.send(file_matches_sha256(&file, &expected_hex));
            if let Some(ctx) = repaint {
                ctx.request_repaint();
            }
        });
    if let Err(e) = spawned {
        let _ = tx.send(Err(InstallError::Io(e.to_string())));
    }
    rx
}

/// Download ohne Netz für [`crate::app::PlanoraApp::for_tests`]: meldet immer „kein Netz“.
pub struct NoDownload;

impl Downloader for NoDownload {
    fn download(
        &self,
        _url: &str,
        _dest: &Path,
        _max_bytes: u64,
        _progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<(), InstallError> {
        Err(InstallError::Network("keine Verbindung".into()))
    }
}

/// Hinweis nach einem gelungenen Update (einmal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AfterUpdate {
    /// Die jetzt laufende Version.
    pub version: Version,
    /// Patchnotes dieser Version, falls die alte Version sie hinterlegt hat.
    pub notes: Option<String>,
}

/// Ort der Zwischenablage der Patchnotes (neben der Konfiguration).
pub fn notes_file(ws: &Workspace) -> PathBuf {
    ws.config_path.with_file_name(NOTES_FILE_NAME)
}

/// Hinterlegt die Patchnotes von `version` für den Hinweis nach dem Neustart; Fehler werden
/// übergangen (dann zeigt der Hinweis nur die Zeile).
pub fn write_notes(file: &Path, version: &Version, notes: &str) {
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(file, format!("{version}\n{notes}"));
}

/// Liest und löscht die Zwischenablage; nur die Notizen zu `version`.
fn take_notes(file: &Path, version: &Version) -> Option<String> {
    let small = std::fs::metadata(file).is_ok_and(|m| m.len() <= MAX_NOTES_BYTES);
    let text = small.then(|| std::fs::read_to_string(file).ok()).flatten();
    let _ = std::fs::remove_file(file);
    let text = text?;
    let (first, notes) = text.split_once('\n').unwrap_or((text.as_str(), ""));
    (Version::parse(first.trim()).ok()? == *version).then(|| notes.to_string())
}

/// Beim Programmstart: Steht in der Konfiguration ein begonnenes Update (`update_pending`), wird
/// es gelöscht, ebenso ein liegengebliebener Download `download` (der Installer bleibt nach
/// seinem Start liegen, weil er ihn selbst noch braucht). War das Ziel des Updates die laufende
/// Version, den Hinweis „Aktualisiert auf Version X“ liefern (mit den hinterlegten Patchnotes);
/// sonst ist das Update nicht durchgelaufen und wird nicht wiederholt.
pub fn startup(
    ws: &mut Workspace,
    download: &DownloadPaths,
    running: &Version,
) -> Option<AfterUpdate> {
    let pending = ws.pending_update();
    let matched = pending.is_some_and(|(_, to)| to == *running);
    if pending.is_some() {
        // Misslingt das Speichern, kommt der Eintrag beim nächsten Start wieder; dann gilt
        // dasselbe noch einmal (kein erneuter Versuch).
        let _ = ws.set_pending_update(None);
    }
    discard_download(download);
    let notes_file = notes_file(ws);
    if !matched {
        let _ = std::fs::remove_file(&notes_file);
        return None;
    }
    Some(AfterUpdate {
        version: *running,
        notes: take_notes(&notes_file, running),
    })
}

/// Fenster für den Start: Größe, Lage und Zustand aus `update_pending`, aber nur, wenn das Ziel
/// des Updates die laufende Version ist (so öffnet die neue Version wie die alte).
pub fn window_for_start(config: &Config, running: &Version) -> Option<WindowState> {
    let pending = config.update_pending.as_ref()?;
    if Version::parse(&pending.to).ok()? != *running {
        return None;
    }
    pending.window
}

/// Text der Statuszeile während eines Updates.
pub fn status_text(state: &InstallState, restarting: bool) -> String {
    match state {
        _ if restarting => RESTARTING.into(),
        InstallState::Downloading { done, total } => download_text(*done, *total),
        InstallState::ReadyToRestart => WAITING.into(),
        InstallState::Failed(_) => FAILED_SHORT.into(),
        InstallState::Idle => crate::update_dialog::PREPARING.into(),
    }
}

/// Statuszeile, solange der Installer geprüft und gestartet wird.
pub const RESTARTING: &str = "Neustart …";
/// Kasten über allem, solange der Installer geprüft und gestartet wird (die Bedienung ist
/// gesperrt).
pub const RESTART_BOX: &str =
    "PlanoraCAD wird für das Update beendet und startet danach von selbst neu.";
/// Statuszeile nach einem gescheiterten Update.
pub const FAILED_SHORT: &str = "Update fehlgeschlagen";

/// „Wird heruntergeladen … 42 %“ (ohne bekannte Größe: „… 3,4 MB“).
pub fn download_text(done: u64, total: u64) -> String {
    match done.min(total).saturating_mul(100).checked_div(total) {
        Some(pct) => format!("Wird heruntergeladen … {pct} %"),
        None => {
            let mb = done as f64 / (1024.0 * 1024.0);
            format!(
                "Wird heruntergeladen … {} MB",
                format!("{mb:.1}").replace('.', ",")
            )
        }
    }
}

/// Anteil 0 bis 1 für den Fortschrittsbalken.
pub fn fraction(done: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done.min(total) as f64 / total as f64) as f32
    }
}

/// Kleinste Innenfläche des Programmfensters (wie `with_min_inner_size` im Programm).
pub const MIN_WINDOW: egui::Vec2 = egui::vec2(800.0, 600.0);

/// Größe, Lage und Zustand des Programmfensters, wie eframe sie meldet: Lage der Außenkante,
/// Größe der Innenfläche, maximiert oder nicht, geprüft mit [`WindowState::sanitized`]. Keines
/// ohne Größe oder Lage (etwa in Tests) und keines bei minimiertem Fenster (Windows meldet dann
/// die Lage -32000/-32000; die App behält die letzte sichtbare).
pub fn window_state(info: &egui::ViewportInfo) -> Option<WindowState> {
    if info.minimized == Some(true) {
        return None;
    }
    let inner = info.inner_rect?;
    let outer = info.outer_rect?;
    WindowState {
        x: Some(outer.min.x),
        y: Some(outer.min.y),
        width: inner.width(),
        height: inner.height(),
        maximized: info.maximized.unwrap_or(false),
    }
    .sanitized()
}

/// Öffnet das Fenster in Größe, Lage und Zustand `w`; so übernimmt die neue Version nach einem
/// Update das Fenster der alten ([`window_for_start`]). Geprüft mit [`WindowState::sanitized`]:
/// Größe mindestens [`MIN_WINDOW`], höchstens `MAX_WINDOW_SIZE`; eine unplausible Lage bleibt
/// weg (dann setzt das System das Fenster); ohne brauchbare Größe bleibt alles bei der Vorgabe.
pub fn restore_window(builder: egui::ViewportBuilder, w: &WindowState) -> egui::ViewportBuilder {
    let Some(w) = w.sanitized() else {
        return builder;
    };
    let mut builder = builder
        .with_inner_size(egui::vec2(w.width, w.height).max(MIN_WINDOW))
        .with_maximized(w.maximized);
    if let (Some(x), Some(y)) = (w.x, w.y) {
        builder = builder.with_position(egui::pos2(x, y));
    }
    builder
}

// ---------------------------------------------------------------------------------------------
// Installer starten (Update über `planoracad-setup-x64.exe`)
// ---------------------------------------------------------------------------------------------

/// Parameter, an dem der Installer erkennt, dass er die App nach einem Update wieder starten soll
/// (`{param:NACHUPDATE|0}` in `installer/planoracad.iss`).
pub const SETUP_RESTART_PARAM: &str = "/NACHUPDATE=1";

/// Parameter für den stillen Lauf des Installers im Modus `mode`, durch Leerzeichen getrennt,
/// zuletzt das Protokoll nach `log` (`/LOG=…`, in Anführungszeichen, wenn der Pfad Leerzeichen
/// enthält).
pub fn setup_args(mode: InstallMode, log: &Path) -> String {
    let who = match mode {
        InstallMode::User => "/CURRENTUSER",
        InstallMode::AllUsers => "/ALLUSERS",
    };
    let log = log.to_string_lossy();
    let log = if log.contains(' ') {
        format!("/LOG=\"{log}\"")
    } else {
        format!("/LOG={log}")
    };
    [
        "/VERYSILENT",
        "/SUPPRESSMSGBOXES",
        "/NORESTART",
        "/CLOSEAPPLICATIONS",
        "/RESTARTAPPLICATIONS",
        who,
        SETUP_RESTART_PARAM,
        &log,
    ]
    .join(" ")
}

/// Verb für `ShellExecuteExW`: `runas` (Admin-Abfrage) nur bei „für alle“.
pub fn setup_verb(mode: InstallMode) -> &'static str {
    match mode {
        InstallMode::User => "open",
        InstallMode::AllUsers => "runas",
    }
}

/// Warum der Installer nicht gestartet ist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchError {
    /// Die Admin-Abfrage wurde abgelehnt.
    Cancelled,
    /// Sonstiger Fehler (Text für die Meldung).
    Failed(String),
}

/// Startet den heruntergeladenen Installer.
pub trait SetupLauncher {
    fn launch(&self, setup: &Path, mode: InstallMode) -> Result<(), LaunchError>;
}

/// Startet den Installer über `ShellExecuteExW` (nur Windows) mit [`setup_args`] und dem
/// Protokoll [`setup_log_path`]. Läuft synchron im UI-Faden: Die Admin-Abfrage liegt auf dem
/// sicheren Desktop; das kurze „Keine Rückmeldung“ ist hinnehmbar und spart einen COM-Faden.
pub struct ShellLauncher;

impl SetupLauncher for ShellLauncher {
    #[cfg(windows)]
    fn launch(&self, setup: &Path, mode: InstallMode) -> Result<(), LaunchError> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{ERROR_CANCELLED, GetLastError};
        use windows_sys::Win32::UI::Shell::{
            SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
            s.encode_wide().chain(std::iter::once(0)).collect()
        }
        let verb = wide(setup_verb(mode).as_ref());
        let file = wide(setup.as_os_str());
        let params = wide(setup_args(mode, &setup_log_path()).as_ref());
        let dir = wide(setup.parent().unwrap_or(Path::new("")).as_os_str());

        // SAFETY: SHELLEXECUTEINFOW ist eine reine C-Struktur; alles Null ist ein gültiger
        // Anfangszustand (keine Fenster, keine Griffe).
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.lpDirectory = dir.as_ptr();
        info.nShow = SW_SHOWNORMAL;
        // SAFETY: `info` ist vollständig gesetzt; alle Zeichenketten sind nullterminiert und
        // leben bis nach dem Aufruf.
        if unsafe { ShellExecuteExW(&mut info) } != 0 {
            return Ok(());
        }
        // SAFETY: Liest nur den Fehlercode des eigenen Fadens.
        let code = unsafe { GetLastError() };
        if code == ERROR_CANCELLED {
            Err(LaunchError::Cancelled)
        } else {
            Err(LaunchError::Failed(
                std::io::Error::from_raw_os_error(code as i32).to_string(),
            ))
        }
    }

    #[cfg(not(windows))]
    fn launch(&self, _setup: &Path, _mode: InstallMode) -> Result<(), LaunchError> {
        Err(LaunchError::Failed(ONLY_WINDOWS.into()))
    }
}

/// Start, der nie etwas startet, für [`crate::app::PlanoraApp::for_tests`].
pub struct NoLaunch;

impl SetupLauncher for NoLaunch {
    fn launch(&self, _setup: &Path, _mode: InstallMode) -> Result<(), LaunchError> {
        Err(LaunchError::Failed("in Tests nicht möglich".into()))
    }
}
