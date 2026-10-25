//! Update einspielen im Programm: Download und Prüfung im Hintergrund ([`Installer`]), Neustart
//! über eine austauschbare Schnittstelle ([`Restarter`]) und was beim Start nach einem Update
//! geschieht ([`startup`]).
//!
//! **Ablauf.** „Jetzt aktualisieren“ → [`decide`] (nur Windows, Ordner beschreibbar) →
//! [`Installer::start`]: ein eigener Thread lädt die Prüfsummen-Datei und die Programmdatei nach
//! `<exe>.new` und prüft die Summe (`Downloading` → `ReadyToRestart`, sonst `Failed`); die
//! erwartete Summe bleibt gemerkt. Der Neustart beginnt erst, wenn nichts mehr offen ist (keine
//! Rückfrage, kein Fenster, kein schreibender Export, kein Autosicherungs-Angebot; bis dahin
//! [`WAITING`]). Dann fragt die App wie beim Schließen nach ungespeicherter Arbeit
//! (`Workspace::request_quit`); „Abbrechen“ dort bricht das Update ab und löscht `.new`. Sonst:
//! `.new` im Hintergrund noch einmal gegen die gemerkte Summe prüfen, `update_pending`
//! speichern (gelingt das nicht, wird nichts ausgetauscht), austauschen, die neue exe mit
//! [`AFTER_UPDATE_ARG`] starten und bis zu [`MARKER_WAIT`] auf die Datei
//! `planoracad.update-ok` mit der Zielversion als Inhalt warten. Kommt sie, beendet sich das
//! Programm; sonst legt es den alten Stand zurück, überspringt die Version und läuft weiter. Die
//! Schritte nach dem Download stehen in [`crate::app`] (`PlanoraApp::install_tick`).
//!
//! **Start.** Die neue Version schreibt die Rückmeldung so früh wie möglich
//! ([`write_ok_marker`], in `PlanoraApp::new` gleich nach dem Einrichten des Renderers, vor
//! Konfiguration und Bibliothek). [`startup`] räumt `.old` weg, löscht `update_pending` und
//! zeigt nach einem gelungenen Update einmal „Aktualisiert auf Version X“ (mit den Patchnotes
//! aus der Zwischenablage [`NOTES_FILE_NAME`], die die alte Version vor dem Austausch schreibt).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::workspace::Workspace;
use planora_update::install::{
    Downloader, InstallError, MAX_CHECKSUM_BYTES, Paths, checksum_hex, cleanup_old,
    discard_download, download_limit, exe_dir_writable, file_matches_sha256, release_url_ok,
};
use planora_update::{EXE_NAME, REPO, Update, Version};

/// Startargument der neu gestarteten Version nach einem Austausch.
pub const AFTER_UPDATE_ARG: &str = "--nach-update";

/// So lange wartet die alte Version auf die Rückmeldung der neuen. Großzügig, weil eine frisch
/// geladene, unsignierte exe beim ersten Start vom Virenschutz geprüft werden kann.
pub const MARKER_WAIT: Duration = Duration::from_secs(30);

/// Statuszeile und Meldung, solange ein fertiges Update auf das Ende der laufenden Arbeit wartet.
pub const WAITING: &str = "Update ist bereit und wartet, bis du fertig bist.";

/// Zwischenablage der Patchnotes der neuen Version neben der Konfiguration (erste Zeile die
/// Version, dann die Notizen); die neue Version liest und löscht sie beim Start.
pub const NOTES_FILE_NAME: &str = "planoracad-update-notes.md";

/// Größte gelesene Zwischenablage der Patchnotes.
const MAX_NOTES_BYTES: u64 = 64 * 1024;

/// Hinweis, wenn das Programm nicht unter Windows läuft.
pub const ONLY_WINDOWS: &str = "Automatisches Aktualisieren gibt es nur unter Windows.";
/// Knopf bei einem Fehler: öffnet die Release-Seite auf GitHub.
pub const OPEN_DOWNLOAD_PAGE: &str = "Download-Seite öffnen";
/// Meldung, wenn `update_pending` nicht gespeichert werden kann.
pub const PENDING_NOT_SAVED: &str = "Das Update ließ sich nicht vormerken, weil die Einstellungen nicht gespeichert werden können. Es wurde nichts ausgetauscht.";
/// Meldung, wenn der Ort des Programms unbekannt ist.
pub const NO_EXE: &str =
    "Der Ort des Programms ist unbekannt. Lade die neue Version von GitHub herunter.";
/// Meldung, solange ein abgebrochener Download noch endet.
pub const STILL_STOPPING: &str =
    "Der abgebrochene Download endet gerade noch. Bitte gleich noch einmal versuchen.";

/// Meldung, wenn die neue Version sich nicht zurückmeldet und der alte Stand zurückliegt.
pub fn not_started(to: &Version) -> String {
    format!(
        "Die neue Version {to} ist nicht richtig gestartet. PlanoraCAD läuft in der bisherigen Version weiter; Version {to} wird übersprungen."
    )
}

/// Meldung, wenn der alte Stand sich nicht zurücklegen lässt (dann fehlt womöglich die
/// Programmdatei; die bisherige liegt noch als `.old` daneben).
pub fn rollback_failed(e: &InstallError) -> String {
    let detail = match e {
        InstallError::Io(detail) => detail.clone(),
        other => other.message(),
    };
    format!(
        "Die bisherige Version ließ sich nicht zurücklegen ({detail}). Bitte lade PlanoraCAD neu von GitHub herunter."
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

/// Zustand des Downloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallState {
    /// Nichts läuft.
    Idle,
    /// Die Programmdatei wird geladen (Bytes geladen, gesamt).
    Downloading { done: u64, total: u64 },
    /// `.new` liegt da und die Prüfsumme stimmt; jetzt der Neustart.
    ReadyToRestart,
    /// Gescheitert; die Meldung in einfacher Sprache.
    Failed(String),
}

/// Was „Jetzt aktualisieren“ tut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallDecision {
    /// Herunterladen und austauschen.
    Download,
    /// Nicht unter Windows: Hinweis [`ONLY_WINDOWS`] und die Release-Seite öffnen.
    OnlyWindows,
    /// Ordner der exe nicht beschreibbar: Hinweis mit Knopf [`OPEN_DOWNLOAD_PAGE`].
    ProtectedFolder,
    /// Ort der exe unbekannt.
    NoExe,
}

/// Entscheidung zu „Jetzt aktualisieren“ ohne `cfg`: `auto_supported` ist im Programm
/// `cfg!(windows)`.
pub fn decide(auto_supported: bool, exe: Option<&Path>) -> InstallDecision {
    if !auto_supported {
        return InstallDecision::OnlyWindows;
    }
    let Some(exe) = exe else {
        return InstallDecision::NoExe;
    };
    let dir = exe.parent().unwrap_or(Path::new("."));
    if exe_dir_writable(dir) {
        InstallDecision::Download
    } else {
        InstallDecision::ProtectedFolder
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
    paths: Option<Paths>,
    target: Option<Version>,
    /// Die geprüfte Summe von `.new` (für die zweite Prüfung vor dem Austausch).
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
    pub fn paths(&self) -> Option<&Paths> {
        self.paths.as_ref()
    }

    /// Version des laufenden oder fertigen Downloads.
    pub fn target(&self) -> Option<Version> {
        self.target
    }

    /// Die beim Download geprüfte SHA-256 von `.new` (nur in `ReadyToRestart`).
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

    /// Startet Download und Prüfung von `update` nach `paths.new_exe` in einem eigenen Thread.
    /// Vorher geprüft: Adressen aus dem Release, Größe ([`download_limit`]), beschreibbarer
    /// Ordner; ein liegengebliebener Download wird verworfen. Läuft schon einer, bleibt es bei
    /// ihm.
    pub fn start(
        &mut self,
        update: &Update,
        paths: Paths,
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
        if !release_url_ok(&release.exe.url) || !release_url_ok(&release.sha256.url) {
            self.fail(InstallError::Network("Adresse gehört nicht zum Release".into()).message());
            return;
        }
        let limit = match download_limit(release.exe.size) {
            Ok(limit) => limit,
            Err(e) => return self.fail(e.message()),
        };
        if !exe_dir_writable(paths.dir()) {
            self.fail(InstallError::NotWritable.message());
            return;
        }
        discard_download(&paths);

        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let repaint = self.repaint.clone();
        let size = release.exe.size;
        let (exe_url, sha_url) = (release.exe.url.clone(), release.sha256.url.clone());
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
                    let checksum = paths.checksum_file();
                    downloader.download(&sha_url, &checksum, MAX_CHECKSUM_BYTES, &mut |_, _| {
                        !cancel.load(Ordering::SeqCst)
                    })?;
                    let text =
                        std::fs::read(&checksum).map_err(|e| InstallError::Io(e.to_string()))?;
                    let _ = std::fs::remove_file(&checksum);
                    let text = String::from_utf8(text).map_err(|_| InstallError::Checksum)?;
                    let mut last = None;
                    downloader.download(&exe_url, &paths.new_exe, limit, &mut |done, total| {
                        let total = if total > 0 { total } else { size };
                        if last != Some(done) {
                            last = Some(done);
                            let _ = tx.send(Msg::Progress(done, total));
                            ping();
                        }
                        !cancel.load(Ordering::SeqCst)
                    })?;
                    let expected = checksum_hex(&text, EXE_NAME)?;
                    file_matches_sha256(&paths.new_exe, &expected)?;
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
            Err(e) => self.fail(InstallError::Io(e.to_string()).message()),
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

/// Zweite Prüfung von `.new` gegen die beim Download geprüfte Summe, unmittelbar vor dem
/// Austausch, in einem eigenen Thread (große Datei, die Oberfläche wartet nicht). Ein anderes
/// Programm hätte die Datei inzwischen verändern können (etwa eine zweite Instanz, die selbst
/// ein Update lädt). Ist ein Kontext da, meldet der Thread danach ein neues Bild.
pub fn reverify(
    file: PathBuf,
    expected_hex: String,
    repaint: Option<egui::Context>,
) -> Receiver<Result<(), InstallError>> {
    let (tx, rx) = mpsc::channel();
    let job_tx = tx.clone();
    let spawned = std::thread::Builder::new()
        .name("planora-update-pruefen".into())
        .spawn(move || {
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

/// Startet die neue Version nach dem Austausch. Im Programm [`ProcessRestarter`], in Tests eine
/// Nachbildung.
pub trait Restarter {
    /// Startet `exe` mit [`AFTER_UPDATE_ARG`]; wartet nicht auf sie.
    fn start_new(&self, exe: &Path) -> Result<(), String>;
}

/// Startet die neue exe als eigenen Prozess (nur Windows; sonst Fehler).
pub struct ProcessRestarter;

impl Restarter for ProcessRestarter {
    #[cfg(windows)]
    fn start_new(&self, exe: &Path) -> Result<(), String> {
        std::process::Command::new(exe)
            .arg(AFTER_UPDATE_ARG)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(not(windows))]
    fn start_new(&self, _exe: &Path) -> Result<(), String> {
        Err(ONLY_WINDOWS.into())
    }
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

/// Neustart, der nie etwas startet, für [`crate::app::PlanoraApp::for_tests`].
pub struct NoRestart;

impl Restarter for NoRestart {
    fn start_new(&self, _exe: &Path) -> Result<(), String> {
        Err("in Tests nicht möglich".into())
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

/// Rückmeldung der neu gestarteten Version an die alte: die Datei `planoracad.update-ok` mit der
/// laufenden Version als Inhalt. Nur mit [`AFTER_UPDATE_ARG`] und so früh wie möglich aufrufen;
/// Fehler werden übergangen (dann legt die alte Version nach [`MARKER_WAIT`] zurück).
pub fn write_ok_marker(paths: &Paths, running: &Version) {
    let _ = std::fs::write(&paths.ok_marker, running.to_string());
}

/// Gilt die Rückmeldung für `to`? Nur, wenn die Datei da ist und genau diese Version enthält
/// (eine liegengebliebene Datei eines früheren Updates zählt nicht).
pub fn ok_marker_for(paths: &Paths, to: &Version) -> bool {
    std::fs::read_to_string(&paths.ok_marker).is_ok_and(|c| c.trim() == to.to_string())
}

/// Beim Programmstart: `.old` eines früheren Updates löschen (Fehler übergehen). Steht in der
/// Konfiguration ein begonnenes Update (`update_pending`), wird es gelöscht; war sein Ziel nicht
/// die laufende Version, ist der Austausch gescheitert und wird nicht wiederholt. Mit
/// [`AFTER_UPDATE_ARG`] oder einem Ziel gleich der laufenden Version den Hinweis „Aktualisiert
/// auf Version X“ liefern. Die Rückmeldung an die alte Version schreibt [`write_ok_marker`]
/// (früher, nicht hier).
pub fn startup(
    ws: &mut Workspace,
    after_update_arg: bool,
    paths: Option<&Paths>,
    running: &Version,
) -> Option<AfterUpdate> {
    let pending = ws.pending_update();
    let matched = pending.is_some_and(|(_, to)| to == *running);
    if pending.is_some() {
        // Misslingt das Speichern, kommt der Eintrag beim nächsten Start wieder; dann gilt
        // dasselbe noch einmal (kein erneuter Austausch).
        let _ = ws.set_pending_update(None);
    }
    if let Some(paths) = paths {
        cleanup_old(paths);
    }
    let notes_file = notes_file(ws);
    if !after_update_arg && !matched {
        let _ = std::fs::remove_file(&notes_file);
        return None;
    }
    Some(AfterUpdate {
        version: *running,
        notes: take_notes(&notes_file, running),
    })
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

/// Statuszeile und Meldung, solange die neue Version gestartet wird.
pub const RESTARTING: &str = "Neustart …";
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
