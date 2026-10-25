use crate::blend_dialog::BlendDialog;
use crate::boolean_dialog::BooleanDialog;
use crate::create_dialog::CreateDialog;
use crate::drawing_dialog::drawing_menu;
use crate::drawing_state::WorkArea;
use crate::export_dialog::ExportDialog;
use crate::feature_dialog::FeatureDialog;
use crate::file_chooser::default_dir;
use crate::icons::{self, Icon};
use crate::import_dialog::ImportDialog;
use crate::keymap::overview::{OverviewEvent, OverviewState};
use crate::keymap::{self, Action};
use crate::project_actions::ProjectsUi;
use crate::projects_window::{ProjectsEvent, ProjectsWindow};
use crate::settings_window::{CheckHint, SettingsEvent, SettingsWindow};
use crate::shell::ShellState;
use crate::state::{AppState, BodyKind, SketchRequest, StandardPlane};
use crate::step_import_dialog::{self, StepImportDialog};
use crate::update_check::{self, CheckRequest, CheckResult, NoNetwork, UpdateChecker};
use crate::update_dialog::{self, InstallView, UpdateAction, UpdateDialog};
use crate::update_install::{
    self, AfterUpdate, InstallDecision, InstallState, Installer, MARKER_WAIT, NewProcess,
    NoDownload, NoRestart, ProcessRestarter, Restarter,
};
use crate::versions_panel::VersionsUi;
use crate::workspace::{Prompt, Workspace};
use crate::{
    about_dialog, browser, commands, drawing_view, fonts, parameter_dialog, prompts, shell_panels,
    sketch_panel, theme, viewport,
};
use planora_feature::SketchPlane;
use planora_kernel::BoolOp;
use planora_render::gpu::GpuScene;
use planora_render::{FaceRef, Pick, ViewFace};
use planora_sketch::plane_of_face;
use planora_update::check::{CheckOutcome, HttpSource, NetError, ReleaseSource, due};
use planora_update::install::{
    Downloader, HttpDownloader, InstallError, Paths, discard_download, rollback, swap,
};
use planora_update::{Update, Version};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// Wie oft die App ohne Eingaben neu zeichnet, damit die Autosicherung auch im Leerlauf läuft.
const AUTOSAVE_REPAINT: Duration = Duration::from_secs(30);

/// So lange wartet das Programmende ([`eframe::App::on_exit`]) höchstens auf eine gerade
/// entstehende Miniatur, damit sie noch gespeichert wird (danach wird nur ihre Marke entfernt).
pub const EXIT_WAIT: Duration = Duration::from_secs(2);

/// Menüeintrag „Datei → Projekte …“ (Kürzel [`Action::Projects`]).
pub const PROJECTS_ENTRY: &str = "Projekte …";
/// Hinweis, wenn Projekte oder Einstellungen nicht öffnen, weil gerade ein Export schreibt.
pub const EXPORT_BUSY: &str = "Bitte erst den Export beenden.";
/// Menüeintrag „Datei → Einstellungen …“.
pub const SETTINGS_ENTRY: &str = "Einstellungen …";
/// Menüeintrag „Ansicht → Tastenkürzel …“ (Kürzel [`Action::ShowShortcuts`]).
pub const VIEW_SHORTCUTS_ENTRY: &str = "Tastenkürzel …";

/// Was die Menüs außer ihren eigenen Aktionen verlangen: das Fenster „Projekte“,
/// „Einstellungen“ oder „Tastenkürzel“ öffnen (die Fenster gehören der App, nicht dem
/// Arbeitsbereich).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuRequest {
    #[default]
    None,
    Projects,
    Settings,
    Shortcuts,
}

pub struct PlanoraApp {
    pub(crate) ws: Workspace,
    /// Sichtbarkeit und Reiter der schwebenden Felder (`shell_panels`).
    pub(crate) shell: ShellState,
    /// Fenster „Projekte“: eines je App (nicht je Tab); sein Miniatur-Arbeiter lebt so lange wie
    /// die App und wird in [`eframe::App::on_exit`] geordnet beendet.
    pub(crate) projects: ProjectsWindow,
    /// Fenster „Einstellungen“.
    pub(crate) settings: SettingsWindow,
    /// Fenster „Tastenkürzel“ (Übersicht, [`keymap::overview`]).
    pub(crate) overview: OverviewState,
    pub(crate) versions_ui: VersionsUi,
    applied_dark: Option<bool>,
    /// `ViewportCommand::Close` wurde nach freigegebenem Beenden schon geschickt.
    close_sent: bool,
    /// Quelle der Release-Liste: im Programm GitHub ([`HttpSource`]), in Tests eine Nachbildung.
    update_source: Arc<dyn ReleaseSource + Send + Sync>,
    /// Prüfung auf neue Versionen im Hintergrund.
    update_checker: UpdateChecker,
    /// Die Prüfung beim Start ist erledigt (gestartet oder nicht fällig).
    update_start_done: bool,
    /// Der Knopf „Jetzt nach Updates suchen“ kam, während schon eine Prüfung lief: deren
    /// Ergebnis zählt dann als manuell (mit Hinweis).
    update_manual_waiting: bool,
    /// Gefundenes Update, für die Meldung „Neue Version verfügbar“ vorgemerkt.
    pub(crate) pending_notice: Option<Update>,
    /// Meldung „Neue Version verfügbar“ ([`update_dialog`]).
    pub(crate) update_dialog: UpdateDialog,
    /// Download und Prüfung eines Updates im Hintergrund ([`update_install`]).
    pub(crate) installer: Installer,
    /// Download der Release-Dateien: im Programm [`HttpDownloader`], in Tests eine Nachbildung.
    downloader: Arc<dyn Downloader + Send + Sync>,
    /// Start der neuen Version: im Programm [`ProcessRestarter`], in Tests eine Nachbildung.
    restarter: Arc<dyn Restarter + Send + Sync>,
    /// Die laufende Programmdatei (`None`: unbekannt, dann kein Austausch).
    exe_path: Option<PathBuf>,
    /// Gibt es das automatische Aktualisieren hier? Im Programm `cfg!(windows)`.
    auto_install: bool,
    /// Schritt des Neustarts nach dem Download.
    pub(crate) restart: RestartPhase,
    /// So lange wartet die alte Version auf die Rückmeldung der neuen ([`MARKER_WAIT`]).
    marker_wait: Duration,
    /// Hinweis „Aktualisiert auf Version X“ nach einem Update (einmal).
    after_update: Option<AfterUpdate>,
    /// Ergebnis der zweiten Prüfung von `.new` ([`RestartPhase::Verifying`]).
    reverify: Option<Receiver<Result<(), InstallError>>>,
    /// Die gestartete neue Version, solange auf ihre Rückmeldung gewartet wird; meldet sie sich
    /// nicht oder wird der Neustart abgebrochen, wird sie damit beendet.
    new_process: Option<Box<dyn NewProcess>>,
    /// Gestartet mit [`update_install::AFTER_UPDATE_ARG`]: die Rückmeldung an die alte Version
    /// steht noch aus (sie kommt nach dem ersten fertig gezeichneten Bild).
    ok_marker_due: bool,
    /// Nummer des Bildes, in dem `ui` zum ersten Mal ganz durchlief
    /// (`egui::Context::cumulative_frame_nr`); ist die laufende Nummer größer, ist dieses Bild
    /// auch gezeichnet (eframe zeichnet nach dem Durchlauf, auch nach mehreren Durchgängen).
    first_frame: Option<u64>,
}

/// Schritt des Neustarts nach einem fertig geladenen und geprüften Update.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RestartPhase {
    /// Kein Neustart begonnen (oder er wartet, bis nichts mehr offen ist).
    #[default]
    None,
    /// Die Rückfrage wegen ungespeicherter Arbeit läuft (wie beim Schließen).
    AwaitingPrompt,
    /// `.new` wird unmittelbar vor dem Austausch noch einmal geprüft (im Hintergrund).
    Verifying,
    /// Ausgetauscht und die neue Version gestartet; wartet bis `deadline` auf ihre Rückmeldung.
    WaitingForMarker { deadline: Instant, to: Version },
    /// Die neue Version läuft; das Programm beendet sich.
    Exiting,
}

impl PlanoraApp {
    /// Das Programm. `after_update`: gestartet mit [`update_install::AFTER_UPDATE_ARG`] (von der
    /// alten Version nach dem Austausch).
    pub fn new(cc: &eframe::CreationContext<'_>, after_update: bool) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("PlanoraCAD benötigt den wgpu-Renderer");
        cc.egui_ctx.set_fonts(fonts::definitions());
        Self::setup_context(&cc.egui_ctx);
        GpuScene::install(render_state);
        let running = crate::running_version();
        let exe_path = std::env::current_exe().ok();

        let config = planora_project::config::Config::load();
        let root = config
            .root
            .clone()
            .unwrap_or_else(planora_project::config::Config::default_root);
        // Ist die Wurzel nicht nutzbar, startet die App ohne Bibliothek; Speichern fragt dann
        // zuerst nach einem Projektordner.
        let library = planora_project::library::Library::open(&root).ok();
        let ws = Workspace::new(library, config);
        // Nur hier geht das Programm ins Netz und startet sich neu; Tests bekommen Nachbildungen.
        let source = Arc::new(HttpSource::new(&running));
        let mut app = Self::with_workspace(ws, source);
        app.downloader = Arc::new(HttpDownloader::new(&running));
        app.restarter = Arc::new(ProcessRestarter);
        app.exe_path = exe_path;
        // Nach einem Update meldet sich die neue Version erst nach ihrem ersten fertigen Bild bei
        // der alten (in `logic`); bis dahin kann die alte noch zurücklegen.
        app.ok_marker_due = after_update;
        app.finish_update_startup(after_update);
        app
    }

    /// Einstellungen des egui-Kontexts, die nicht vom Aussehen abhängen. Kein Tastatur-Zoom
    /// (Strg und Plus oder Minus): Die Felder sind in Punkten auf die Mindestgröße 800 × 600
    /// ausgelegt, ein Zoom ließe sie dort nicht mehr hineinpassen.
    #[doc(hidden)]
    pub fn setup_context(ctx: &egui::Context) {
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
    }

    fn with_workspace(ws: Workspace, update_source: Arc<dyn ReleaseSource + Send + Sync>) -> Self {
        Self {
            ws,
            shell: ShellState::default(),
            projects: ProjectsWindow::default(),
            settings: SettingsWindow::default(),
            overview: OverviewState::default(),
            versions_ui: VersionsUi::default(),
            applied_dark: None,
            close_sent: false,
            update_source,
            update_checker: UpdateChecker::default(),
            update_start_done: false,
            update_manual_waiting: false,
            pending_notice: None,
            update_dialog: UpdateDialog::default(),
            installer: Installer::default(),
            downloader: Arc::new(NoDownload),
            restarter: Arc::new(NoRestart),
            exe_path: None,
            auto_install: cfg!(windows),
            restart: RestartPhase::None,
            marker_wait: MARKER_WAIT,
            after_update: None,
            reverify: None,
            new_process: None,
            ok_marker_due: false,
            first_frame: None,
        }
    }

    /// Wie [`PlanoraApp::for_tests_with_updates`] mit Nachbildungen für Download und Neustart
    /// und einer Programmdatei `exe` in einem Temp-Ordner (nie die echte).
    #[doc(hidden)]
    pub fn for_tests_with_install(
        ws: Workspace,
        source: Arc<dyn ReleaseSource + Send + Sync>,
        downloader: Arc<dyn Downloader + Send + Sync>,
        restarter: Arc<dyn Restarter + Send + Sync>,
        exe: PathBuf,
    ) -> Self {
        let mut app = Self::with_workspace(ws, source);
        app.downloader = downloader;
        app.restarter = restarter;
        app.exe_path = Some(exe);
        app
    }

    /// Gibt es das automatische Aktualisieren (für Tests der Entscheidung ohne `cfg`)?
    #[doc(hidden)]
    pub fn set_auto_install_supported(&mut self, on: bool) {
        self.auto_install = on;
    }

    /// Wartezeit auf die Rückmeldung der neuen Version (für Tests).
    #[doc(hidden)]
    pub fn set_marker_wait(&mut self, wait: Duration) {
        self.marker_wait = wait;
    }

    /// Download eines Updates (für Tests).
    #[doc(hidden)]
    pub fn installer(&self) -> &Installer {
        &self.installer
    }

    /// Schritt des Neustarts (für Tests).
    #[doc(hidden)]
    pub fn restart_phase(&self) -> RestartPhase {
        self.restart
    }

    /// Hinweis nach einem Update, solange er offen ist (für Tests).
    #[doc(hidden)]
    pub fn after_update(&self) -> Option<&AfterUpdate> {
        self.after_update.as_ref()
    }

    /// Pfade des Austauschs zur laufenden Programmdatei.
    fn install_paths(&self) -> Option<Paths> {
        self.exe_path.as_deref().map(Paths::for_exe)
    }

    /// Beim Start wie im Programm ([`PlanoraApp::new`]): mit `after_update`
    /// ([`update_install::AFTER_UPDATE_ARG`]) die Rückmeldung an die alte Version vormerken (sie
    /// kommt nach dem ersten fertigen Bild, [`PlanoraApp::write_due_ok_marker`]), dann
    /// [`PlanoraApp::finish_update_startup`].
    #[doc(hidden)]
    pub fn run_update_startup(&mut self, after_update: bool) {
        self.ok_marker_due = after_update;
        self.finish_update_startup(after_update);
    }

    /// Schreibt die vorgemerkte Rückmeldung an die alte Version, sobald das erste Bild fertig
    /// gezeichnet ist (einmal). `frame_nr`: `egui::Context::cumulative_frame_nr` des laufenden
    /// Bildes.
    fn write_due_ok_marker(&mut self, frame_nr: u64) {
        if !self.ok_marker_due || !self.first_frame.is_some_and(|n| frame_nr > n) {
            return;
        }
        self.ok_marker_due = false;
        if let Some(paths) = self.install_paths() {
            update_install::write_ok_marker(&paths, &crate::running_version());
        }
    }

    /// Ist die Bedienung wegen des Neustarts gesperrt? Von der zweiten Prüfung von `.new` bis
    /// zum Ende des Programms ([`RestartPhase::Verifying`], [`RestartPhase::WaitingForMarker`],
    /// [`RestartPhase::Exiting`]).
    fn restart_locked(&self) -> bool {
        matches!(
            self.restart,
            RestartPhase::Verifying | RestartPhase::WaitingForMarker { .. } | RestartPhase::Exiting
        )
    }

    /// Gibt es Arbeit, die beim Beenden verloren ginge oder eine Antwort braucht? Ein geänderter
    /// Tab, ein Autosicherungs-Angebot oder eine offene Rückfrage.
    fn unsaved_work(&self) -> bool {
        self.ws.prompt.is_some()
            || self
                .ws
                .tabs
                .iter()
                .any(|t| t.is_dirty() || t.pending_autosave.is_some())
    }

    /// Aufräumen nach einem Update und der Hinweis „Aktualisiert auf Version X“
    /// ([`update_install::startup`]).
    fn finish_update_startup(&mut self, after_update: bool) {
        let paths = self.install_paths();
        self.after_update = update_install::startup(
            &mut self.ws,
            after_update,
            paths.as_ref(),
            &crate::running_version(),
        );
    }

    /// „Jetzt aktualisieren“: je nach [`update_install::decide`] den Download starten oder den
    /// passenden Hinweis zeigen (nicht Windows: zusätzlich die Release-Seite öffnen).
    fn begin_install(&mut self, ctx: &egui::Context) {
        let Some(update) = self.update_dialog.update.clone() else {
            return;
        };
        self.installer.set_repaint(ctx);
        match update_install::decide(self.auto_install, self.exe_path.as_deref()) {
            InstallDecision::Download => {
                let Some(paths) = self.install_paths() else {
                    return;
                };
                self.installer
                    .start(&update, paths, Arc::clone(&self.downloader));
            }
            InstallDecision::OnlyWindows => {
                self.installer.fail(update_install::ONLY_WINDOWS);
                open_release_page(ctx, &update);
            }
            InstallDecision::ProtectedFolder => {
                self.installer.fail(InstallError::NotWritable.message());
            }
            InstallDecision::NoExe => self.installer.fail(update_install::NO_EXE),
        }
    }

    /// Hält gerade etwas den Neustart auf? Eine offene Rückfrage, ein Autosicherungs-Angebot, ein
    /// schreibender Export (in irgendeinem Tab), ein Fenster ([`PlanoraApp::windows_open`] ohne
    /// die Meldung des Updates selbst) oder das Fenster Projekte. Dann wartet das fertige Update
    /// ([`update_install::WAITING`]) und es wird in jedem Bild neu geprüft.
    fn restart_blocked(&self) -> bool {
        self.ws.prompt.is_some()
            || self.ws.tabs.iter().any(|t| {
                t.pending_autosave.is_some()
                    || t.export_dialog
                        .as_ref()
                        .is_some_and(|d| d.is_writing_file())
            })
            || self.other_windows_open()
            || self.projects.is_open()
    }

    /// Download und Neustart in jedem Bild (in `logic`, vor dem Schließen): Stand abholen; ist
    /// die Datei fertig und geprüft und hält nichts auf ([`PlanoraApp::restart_blocked`]), die
    /// Rückfrage wie beim Schließen stellen (`Workspace::request_quit`); danach `.new` noch
    /// einmal prüfen und austauschen ([`PlanoraApp::restart_now`]) oder bei „Abbrechen“ das
    /// Update verwerfen; nach dem Start der neuen Version auf ihre Rückmeldung warten. Wartet
    /// nie.
    fn install_tick(&mut self, ctx: &egui::Context) {
        self.installer.poll();
        if self.restart == RestartPhase::None
            && *self.installer.state() == InstallState::ReadyToRestart
            && !self.restart_blocked()
        {
            self.restart = RestartPhase::AwaitingPrompt;
            // Dieselbe Rückfrage wie beim Schließen; ohne ungespeicherte Arbeit setzt sie
            // `quit` sofort.
            self.ws.request_quit();
        }
        match self.restart {
            RestartPhase::None | RestartPhase::Exiting => {}
            RestartPhase::AwaitingPrompt => {
                if self.ws.quit {
                    // Das Programm beendet sich erst, wenn die neue Version läuft.
                    self.ws.quit = false;
                    self.start_reverify(ctx);
                } else if self.ws.prompt.is_none()
                    && self.ws.tabs.iter().any(|t| t.pending_autosave.is_some())
                {
                    // Ein Autosicherungs-Angebot kam dazwischen: das ist kein „Abbrechen“;
                    // warten, bis es beantwortet ist, dann neu fragen.
                    self.restart = RestartPhase::None;
                } else if self.ws.prompt.is_none() {
                    // „Abbrechen“ in der Rückfrage: Update abbrechen, `.new` löschen.
                    self.restart = RestartPhase::None;
                    self.installer.cancel();
                    self.update_dialog.apply(
                        &mut self.ws,
                        UpdateAction::CancelInstall,
                        update_check::now_unix(),
                    );
                }
            }
            RestartPhase::Verifying => {
                let result = match self.reverify.as_ref().map(Receiver::try_recv) {
                    Some(Ok(result)) => result,
                    Some(Err(TryRecvError::Empty)) => {
                        ctx.request_repaint_after(Duration::from_millis(50));
                        return;
                    }
                    Some(Err(TryRecvError::Disconnected)) | None => {
                        Err(InstallError::Io("Prüfung abgebrochen".into()))
                    }
                };
                self.reverify = None;
                match result {
                    // Inzwischen ist neue Arbeit entstanden (die Sperre hält das eigentlich
                    // ab): noch ist nichts ausgetauscht, also zurück zur Rückfrage.
                    Ok(()) if self.unsaved_work() => self.restart = RestartPhase::None,
                    Ok(()) => self.restart_now(),
                    Err(e) => {
                        self.restart = RestartPhase::None;
                        if let Some(paths) = self.installer.paths() {
                            discard_download(paths);
                        }
                        self.installer.fail(e.message());
                    }
                }
            }
            RestartPhase::WaitingForMarker { deadline, to } => {
                let Some(paths) = self.installer.paths().cloned() else {
                    self.restart = RestartPhase::None;
                    return;
                };
                if self.unsaved_work() {
                    // Nie ohne Rückfrage schließen: die neue Version beenden, zurücklegen,
                    // mit der Arbeit weiterlaufen.
                    self.restart_stopped(&paths);
                } else if update_install::ok_marker_for(&paths, &to) {
                    let _ = std::fs::remove_file(&paths.ok_marker);
                    // Die neue Version läuft weiter; der Griff wird nur losgelassen.
                    self.new_process = None;
                    self.restart = RestartPhase::Exiting;
                    // Ohne weitere Rückfrage: `handle_close_request` schickt `Close`.
                    self.ws.quit = true;
                } else if Instant::now() >= deadline {
                    self.restart_failed(&paths, to);
                } else {
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
            }
        }
    }

    /// Nach der Rückfrage: `.new` im Hintergrund noch einmal gegen die beim Download geprüfte
    /// Summe prüfen ([`update_install::reverify`]); weiter in [`RestartPhase::Verifying`].
    fn start_reverify(&mut self, ctx: &egui::Context) {
        let (Some(paths), Some(hex)) = (self.installer.paths(), self.installer.verified_sha256())
        else {
            self.restart = RestartPhase::None;
            self.installer.fail(InstallError::Checksum.message());
            return;
        };
        self.reverify = Some(update_install::reverify(
            paths.new_exe.clone(),
            hex.to_string(),
            Some(ctx.clone()),
        ));
        self.restart = RestartPhase::Verifying;
    }

    /// Nach der zweiten Prüfung: `update_pending` speichern (sonst nichts austauschen), Patchnotes für
    /// den Hinweis hinterlegen, austauschen, die neue Version starten und auf sie warten. Jeder
    /// Fehler legt den alten Stand zurück und zeigt eine Meldung; das Programm läuft weiter.
    fn restart_now(&mut self) {
        self.restart = RestartPhase::None;
        let (Some(paths), Some(to)) = (self.installer.paths().cloned(), self.installer.target())
        else {
            self.installer.fail(update_install::NO_EXE);
            return;
        };
        let from = crate::running_version();
        if !self.ws.set_pending_update(Some((from, to))) {
            let _ = self.ws.set_pending_update(None);
            discard_download(&paths);
            self.installer.fail(update_install::PENDING_NOT_SAVED);
            return;
        }
        if let Some(update) = &self.update_dialog.update
            && update.latest.version == to
        {
            update_install::write_notes(
                &update_install::notes_file(&self.ws),
                &to,
                &update.latest.notes,
            );
        }
        // Eine alte Rückmeldung darf nicht als Erfolg zählen.
        if paths.ok_marker.exists() && std::fs::remove_file(&paths.ok_marker).is_err() {
            let _ = self.ws.set_pending_update(None);
            discard_download(&paths);
            self.installer.fail(
                InstallError::Io(format!(
                    "{} lässt sich nicht löschen",
                    paths.ok_marker.display()
                ))
                .message(),
            );
            return;
        }
        if let Err(e) = swap(&paths) {
            let _ = self.ws.set_pending_update(None);
            // Ließ sich die bisherige Datei nicht zurücklegen, ist `.new` die einzige gute
            // Programmdatei neben `.old`: dann nicht löschen.
            if paths.running_exe.exists() {
                discard_download(&paths);
            }
            self.installer.fail(e.message());
            return;
        }
        match self.restarter.start_new(&paths.running_exe) {
            Ok(process) => self.new_process = Some(process),
            Err(e) => {
                let back = rollback(&paths);
                let _ = self.ws.set_pending_update(None);
                let message = match back {
                    Ok(()) => InstallError::Io(format!("neue Version starten: {e}")).message(),
                    Err(b) => update_install::rollback_failed(&b),
                };
                self.installer.fail(message);
                return;
            }
        }
        self.restart = RestartPhase::WaitingForMarker {
            deadline: Instant::now() + self.marker_wait,
            to,
        };
    }

    /// Die neue Version hat sich nicht rechtzeitig gemeldet: alten Stand zurücklegen, die Version
    /// überspringen, Meldung zeigen. Das Programm beendet sich nicht.
    fn restart_failed(&mut self, paths: &Paths, to: Version) {
        self.restart = RestartPhase::None;
        // Erst die neue Version beenden: sonst liefe sie, falls sie doch noch startet, aus der
        // wieder in `.new` umbenannten Datei parallel weiter (und `.new` ließe sich nicht löschen).
        self.stop_new_process();
        let back = rollback(paths);
        let _ = self.ws.set_skipped_version(Some(to));
        let _ = self.ws.set_pending_update(None);
        let _ = std::fs::remove_file(update_install::notes_file(&self.ws));
        self.pending_notice = None;
        let message = match back {
            Ok(()) => update_install::not_started(&to),
            Err(e) => update_install::rollback_failed(&e),
        };
        self.installer.fail(message);
        self.update_dialog.installing = true;
        self.update_dialog.install_hidden = false;
    }

    /// Während des Wartens auf die Rückmeldung ist neue ungespeicherte Arbeit entstanden: die
    /// neue Version beenden, den alten Stand zurücklegen, Meldung zeigen. Die Version wird nicht
    /// übersprungen; das Update lässt sich später noch einmal starten.
    fn restart_stopped(&mut self, paths: &Paths) {
        self.restart = RestartPhase::None;
        self.stop_new_process();
        let _ = std::fs::remove_file(&paths.ok_marker);
        let back = rollback(paths);
        let _ = self.ws.set_pending_update(None);
        let _ = std::fs::remove_file(update_install::notes_file(&self.ws));
        let message = match back {
            Ok(()) => update_install::RESTART_STOPPED.to_string(),
            Err(e) => update_install::rollback_failed(&e),
        };
        self.installer.fail(message);
        self.update_dialog.installing = true;
        self.update_dialog.install_hidden = false;
    }

    /// Beendet die gestartete neue Version, falls es eine gibt.
    fn stop_new_process(&mut self) {
        if let Some(mut process) = self.new_process.take() {
            process.stop();
        }
    }

    /// App ohne Fenster und Renderer für die Oberflächentests; ohne Netz (die Prüfung auf
    /// Updates meldet immer „kein Netz“ und speichert nichts).
    #[doc(hidden)]
    pub fn for_tests(ws: Workspace) -> Self {
        Self::with_workspace(ws, Arc::new(NoNetwork))
    }

    /// Wie [`PlanoraApp::for_tests`] mit einer eigenen Quelle der Release-Liste.
    #[doc(hidden)]
    pub fn for_tests_with_updates(
        ws: Workspace,
        source: Arc<dyn ReleaseSource + Send + Sync>,
    ) -> Self {
        Self::with_workspace(ws, source)
    }

    /// Vorgemerktes Update für die Meldung (für Tests).
    #[doc(hidden)]
    pub fn pending_notice(&self) -> Option<&Update> {
        self.pending_notice.as_ref()
    }

    /// Zustand der Meldung „Neue Version verfügbar“ (für Tests).
    #[doc(hidden)]
    pub fn update_dialog_mut(&mut self) -> &mut UpdateDialog {
        &mut self.update_dialog
    }

    /// Läuft gerade eine Prüfung auf Updates (für Tests)?
    #[doc(hidden)]
    pub fn update_check_running(&self) -> bool {
        self.update_checker.is_running()
    }

    /// Prüfung auf Updates in jedem Bild: im ersten Bild die Prüfung beim Start, wenn sie fällig
    /// ist ([`due`]: Schalter an, letzte Prüfung mindestens 24 Stunden her); danach ein fertiges
    /// Ergebnis abholen ([`PlanoraApp::apply_check_result`]). Wartet nie.
    fn update_tick(&mut self, ctx: &egui::Context) {
        if !self.update_start_done {
            self.update_start_done = true;
            self.update_checker.set_repaint(ctx);
            let enabled = self.ws.update_check_enabled();
            if due(
                update_check::now_unix(),
                self.ws.last_update_check(),
                enabled,
                false,
            ) {
                self.start_update_check(false);
            }
        }
        if let Some(result) = self.update_checker.poll() {
            self.apply_check_result(result);
        }
    }

    /// Startet eine Prüfung mit Kanal und übersprungener Version des Arbeitsbereichs. Läuft schon
    /// eine (etwa die beim Start), zählt deren Ergebnis bei `manual` als manuell.
    fn start_update_check(&mut self, manual: bool) {
        if manual {
            self.settings.set_check_hint(Some(CheckHint::Running));
        }
        if self.update_checker.is_running() {
            self.update_manual_waiting |= manual;
            return;
        }
        let request = CheckRequest {
            running: crate::running_version(),
            channel: self.ws.update_channel(),
            skipped: self.ws.skipped_version(),
            manual,
        };
        self.update_checker
            .start(Arc::clone(&self.update_source), request);
    }

    /// Ergebnis einer Prüfung: Zeitpunkt merken (außer bei „kein Netz“ und Zeitüberschreitung,
    /// [`update_check::records_check`]; misslingt das Speichern, kommt er beim nächsten
    /// gelungenen Schreiben mit), ein Update für die Meldung vormerken. Nur nach dem Knopf ein
    /// Hinweis in den Einstellungen; die Prüfung beim Start meldet nie einen Fehler.
    fn apply_check_result(&mut self, result: CheckResult) {
        let manual = result.manual || std::mem::take(&mut self.update_manual_waiting);
        if update_check::records_check(&result.outcome) {
            self.ws.set_last_update_check(result.at);
        }
        let hint = match result.outcome {
            CheckOutcome::Available(update) => {
                let hint = CheckHint::Found(update.latest.version);
                self.pending_notice = Some(*update);
                hint
            }
            CheckOutcome::UpToDate => CheckHint::UpToDate,
            CheckOutcome::Failed(NetError::Unsupported) => CheckHint::Unsupported,
            CheckOutcome::Failed(_) => CheckHint::Failed,
        };
        if manual {
            self.settings.set_check_hint(Some(hint));
        }
    }

    /// Zeigt die Meldung von allein, wenn ein Update vorgemerkt ist, sie nicht nach „Später“
    /// oder „Jetzt aktualisieren“ ruht ([`UpdateDialog::may_auto_open`]) und nichts anderes offen
    /// ist: keine Rückfrage, kein Fenster ([`PlanoraApp::windows_open`]), nicht Projekte. Sonst
    /// wartet sie bis zum ersten Bild, in dem das gilt.
    fn auto_open_update_notice(&mut self) {
        let Some(update) = self.notice_update() else {
            return;
        };
        if self.update_dialog.may_auto_open(update_check::now_unix())
            && self.ws.prompt.is_none()
            && !self.windows_open()
            && !self.projects.is_open()
        {
            self.update_dialog.open_with(update);
        }
    }

    /// Das Update der Meldung: während eines Updates das gewählte, sonst das vorgemerkte.
    fn notice_update(&self) -> Option<Update> {
        if self.update_dialog.installing {
            self.update_dialog.update.clone()
        } else {
            self.pending_notice.clone()
        }
    }

    /// Klick auf den Hinweis in der Statuszeile („Update verfügbar“ oder der Stand eines
    /// laufenden Updates): öffnet die Meldung, auch nach „Später“ ([`open_update_notice`]).
    pub(crate) fn open_update_notice(&mut self) {
        if let Some(update) = self.notice_update() {
            open_update_notice(
                &mut self.ws,
                &mut self.projects,
                &mut self.settings,
                &mut self.overview,
                &mut self.update_dialog,
                &update,
            );
        }
    }

    /// Folgen einer Wahl in der Meldung über die Meldung hinaus: nach „Überspringen“ ist kein
    /// Update mehr vorgemerkt (kein Hinweis in der Statuszeile); „Jetzt aktualisieren“ startet
    /// den Download ([`PlanoraApp::begin_install`]), „Abbrechen“ bricht ihn ab, „Schließen“ nach
    /// einem Fehler setzt ihn zurück, „Download-Seite öffnen“ öffnet die Release-Seite.
    fn after_update_action(&mut self, ctx: &egui::Context, action: UpdateAction) {
        match action {
            UpdateAction::Skip => self.pending_notice = None,
            UpdateAction::InstallNow => self.begin_install(ctx),
            UpdateAction::CancelInstall => self.installer.cancel(),
            UpdateAction::CloseFailed => self.installer.reset(),
            UpdateAction::OpenDownloadPage => {
                if let Some(update) = &self.update_dialog.update {
                    open_release_page(ctx, update);
                }
            }
            UpdateAction::Later | UpdateAction::Hide => {}
        }
    }

    /// Esc oder ✕ der Meldung: wie [`UpdateDialog::close_action`].
    fn close_update_notice(&mut self, ctx: &egui::Context) {
        let failed = matches!(self.installer.state(), InstallState::Failed(_));
        let action = self.update_dialog.close_action(failed);
        self.update_dialog
            .apply(&mut self.ws, action, update_check::now_unix());
        self.after_update_action(ctx, action);
    }

    /// Die Meldung „Neue Version verfügbar“ in jedem Bild nach den übrigen Fenstern und vor den
    /// Rückfragen: erst die Regeln ([`enforce_update_rules`]), dann von allein öffnen, dann
    /// zeichnen. Danach einmal der Hinweis „Aktualisiert auf Version X“ (nicht während einer
    /// Rückfrage).
    fn show_update_notice(&mut self, ctx: &egui::Context) {
        enforce_update_rules(
            &self.ws,
            &self.projects,
            &self.settings,
            &self.overview,
            &mut self.update_dialog,
        );
        self.auto_open_update_notice();
        let view = install_view(self.restart, self.installer.state());
        if let Some(action) = update_dialog::show(ctx, &mut self.ws, &mut self.update_dialog, view)
        {
            self.after_update_action(ctx, action);
        }
        if self.ws.prompt.is_none() {
            update_dialog::show_after_update(ctx, &mut self.after_update);
        }
    }

    /// Arbeitsbereich (für Tests).
    #[doc(hidden)]
    pub fn ws_mut(&mut self) -> &mut Workspace {
        &mut self.ws
    }

    /// Arbeitsbereich, die Auswahl und Fenster der Aktionen im Fenster Projekte und der Zustand
    /// des Reiters Versionen (für Tests).
    #[doc(hidden)]
    pub fn panels_mut(&mut self) -> (&mut Workspace, &mut ProjectsUi, &mut VersionsUi) {
        (&mut self.ws, &mut self.projects.ui, &mut self.versions_ui)
    }

    /// Arbeitsbereich und die Fenster Projekte und Einstellungen (für Tests).
    #[doc(hidden)]
    pub fn windows_mut(&mut self) -> (&mut Workspace, &mut ProjectsWindow, &mut SettingsWindow) {
        (&mut self.ws, &mut self.projects, &mut self.settings)
    }

    /// Zustand des Fensters „Tastenkürzel“ (für Tests).
    #[doc(hidden)]
    pub fn overview_mut(&mut self) -> &mut OverviewState {
        &mut self.overview
    }

    /// Zustand der schwebenden Felder (für Tests).
    #[doc(hidden)]
    pub fn shell_mut(&mut self) -> &mut ShellState {
        &mut self.shell
    }

    /// Wunsch aus dem Menü „Datei“ ausführen.
    pub(crate) fn apply_menu_request(&mut self, request: MenuRequest) {
        match request {
            MenuRequest::None => {}
            MenuRequest::Projects => {
                open_projects(&mut self.ws, &mut self.projects, &mut self.settings)
            }
            MenuRequest::Settings => {
                open_settings(&mut self.ws, &mut self.projects, &mut self.settings, false)
            }
            MenuRequest::Shortcuts => open_overview(
                &mut self.ws,
                &mut self.projects,
                &mut self.settings,
                &mut self.overview,
            ),
        }
    }

    /// Tastenkürzel über die Tastenbelegung ([`Workspace::keymap`], Verteiler
    /// [`keymap::dispatch`]), ausgeführt wie Menü und Knöpfe über [`commands::run_key`]: Datei-
    /// Befehle, Rückgängig und Wiederholen, im Modell Skizze extrudieren, Verrunden,
    /// Sichtbarkeit, Löschen und Letzten Befehl wiederholen, in der offenen Skizze die Werkzeuge
    /// (L, R, C, P, Leertaste), „Konstruktion umschalten“ (X) und in der Zeichnung die Schalter
    /// „Maß“ und „Position“; Esc, Enter und Entf von Skizze und Zeichnung bleiben dort fest. Nur
    /// ohne aktives Eingabefeld (dort gehören die Tasten dem Feld; ein über Tab fokussierter
    /// Knopf zählt mit, die Leertaste drückt ihn), ohne offene Rückfrage (die sonst überschrieben
    /// würde) und nur im ersten Durchgang eines Bildes (ein verworfener Durchgang würde das
    /// Kürzel sonst zweimal ausführen). Der Marker der Verlaufsleiste und die Ansicht sperren
    /// nichts, auch nicht die Leertaste: Sie wird hier vor dem Aufbau der Bedienelemente
    /// verbraucht, der Marker bekommt also keinen Ersatzklick. Das Kürzel für Projekte öffnet das
    /// Fenster Projekte ([`open_projects`]), S die Übersicht der Tastenkürzel ([`open_overview`];
    /// der getippte Text der Taste wird sicherheitshalber verbraucht, damit er nicht im
    /// Suchfeld landet). Die Befehle von Modell, Skizze und Zeichnung sind wie Rückgängig
    /// gesperrt, solange ein Fenster offen ist ([`PlanoraApp::windows_open`]), und außerdem,
    /// solange das Fenster Projekte offen ist (sonst träfe Entf das Dokument dahinter); die
    /// Taste bleibt dann liegen. Ausgeführt wird in der Reihenfolge der Tabelle, Rückgängig und
    /// Wiederholen zuletzt; sie sind zusätzlich gesperrt, solange eine Maustaste gedrückt ist
    /// ([`PlanoraApp::undo_keys`]).
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if viewport::typing(ctx) || self.ws.prompt.is_some() || ctx.current_pass_index() != 0 {
            return;
        }
        let area = keymap::dispatch::current_area(&self.ws.active_tab().state);
        let windows = self.windows_open() || self.projects.is_open();
        let pressed = keymap::dispatch::pressed_if(ctx, &self.ws.keymap, area, |a| {
            !is_history_key(a) && !(windows && a.area() != keymap::Area::All)
        });
        for &a in Action::ALL.iter().filter(|a| pressed.contains(a)) {
            let request = commands::run_key(&mut self.ws, a);
            if request == MenuRequest::Shortcuts {
                // Sicherheitshalber: der Text der Taste S soll nicht im neuen Suchfeld landen.
                ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
            }
            self.apply_menu_request(request);
        }
        self.undo_keys(ctx);
    }

    /// Ist ein Fenster offen, das Rückgängig und die Befehle von Modell, Skizze und Zeichnung
    /// sperrt? Ein Fenster des Tabs ([`crate::workspace::DocTab::dialog_open`], auch Export und
    /// Import), „Über“, ein Fenster der Aktionen im Fenster Projekte („Neues Projekt“,
    /// „Umbenennen“, „Löschen“, …), ein Fenster des Reiters Versionen, das Fenster Einstellungen
    /// (es ist wie die Dateifenster ein Fenster mit Dateiauswahl) oder die Übersicht
    /// „Tastenkürzel“ (wie „Über“; S öffnet sie also nicht noch einmal). Das Fenster Projekte
    /// selbst sperrt Rückgängig nicht (es ändert kein Dokument), wohl aber die Befehle der
    /// Bereiche (siehe [`PlanoraApp::shortcuts`]). Ebenso die Meldung „Neue Version verfügbar“.
    fn windows_open(&self) -> bool {
        self.update_dialog.open || self.other_windows_open()
    }

    /// [`PlanoraApp::windows_open`] ohne die Meldung „Neue Version verfügbar“ (die den Neustart
    /// nach einem Update nicht aufhält).
    fn other_windows_open(&self) -> bool {
        self.ws.about_open
            || self.overview.open
            || self.after_update.is_some()
            || self.ws.active_tab().dialog_open()
            || crate::project_actions::dialog_open(&self.projects.ui)
            || self.settings.is_open()
            || crate::versions_panel::has_pending(&self.versions_ui)
    }

    /// Rückgängig und Wiederholen (R8; Tasten laut Tastenbelegung). Gesperrt (die Tasten
    /// bleiben dann unverbraucht), solange ein Fenster offen ist ([`PlanoraApp::windows_open`])
    /// und solange eine Maustaste gedrückt ist: Mitten im Ziehen würde das halbe Ziehen sonst
    /// ein Schritt und gleich wieder zurückgenommen. Der Verteiler nimmt Umschalttasten genau,
    /// Strg+Umschalt+Z wird also nie zu Strg+Z. Gehaltene Tasten: jedes Bild mit
    /// (Wiederholungs-)Ereignis ist ein Schritt, mehrere Ereignisse desselben Kürzels in einem
    /// Bild zusammen nur einer (der Verteiler verbraucht sie alle). Kommen beide in einem Bild,
    /// erst Rückgängig, dann Wiederholen.
    fn undo_keys(&mut self, ctx: &egui::Context) {
        if self.windows_open() || ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let area = keymap::dispatch::current_area(&self.ws.active_tab().state);
        let pressed = keymap::dispatch::pressed_if(ctx, &self.ws.keymap, area, is_history_key);
        for a in [Action::Undo, Action::Redo] {
            if pressed.contains(&a) {
                commands::run_action(&mut self.ws, a);
            }
        }
    }
}

/// Rückgängig oder Wiederholen: deren Tasten prüft [`PlanoraApp::undo_keys`] mit eigenen Sperren.
fn is_history_key(a: Action) -> bool {
    matches!(a, Action::Undo | Action::Redo)
}

/// Rückgängig im aktiven Tab (bei offener Skizze in der Skizze). Die Statuszeile nennt danach
/// den Schritt als Hinweis in normaler Farbe (`state.info`, „Rückgängig: Quader anlegen“) oder
/// sagt als Meldung, warum es nicht geht (etwa „Es gibt nichts rückgängig zu machen.“).
/// Schlägt das Zurückladen fehl, steht dort die Meldung von [`crate::workspace::DocTab::undo`];
/// hat der erreichte Stand ein fehlerhaftes Feature, dessen Fehler.
pub(crate) fn undo_active(ws: &mut Workspace) {
    history_step(ws, true);
}

/// Wiederholen im aktiven Tab; Meldung wie bei [`undo_active`].
pub(crate) fn redo_active(ws: &mut Workspace) {
    history_step(ws, false);
}

fn history_step(ws: &mut Workspace, undo: bool) {
    let tab = ws.active_tab_mut();
    let blocked = if undo {
        tab.undo_blocked()
    } else {
        tab.redo_blocked()
    };
    if let Some(why) = blocked {
        tab.state.message = Some(why.into());
        tab.state.info = None;
        return;
    }
    // Alte Meldungen gehören zum alten Stand; was danach in `message` steht, kommt vom Sprung
    // (etwa „Rückgängig ist nicht möglich.“) und bleibt.
    tab.state.message = None;
    tab.state.info = None;
    let (label, done, word) = if undo {
        (tab.undo_label(), tab.undo(), "Rückgängig")
    } else {
        (tab.redo_label(), tab.redo(), "Wiederholen")
    };
    // Landet der Sprung auf einem Stand mit fehlerhaftem Feature, zeigt die Statuszeile dessen
    // Fehler (Fehlerfarbe) statt des Hinweises; in der offenen Skizze steht der Hinweis hinter
    // der Zeile „Skizzen-Modus: …“, die dort vor Feature-Fehlern kommt.
    let shows_error = tab.state.message.is_some()
        || (tab.state.sketch_session.is_none()
            && crate::browser::status_text(&tab.state).is_some());
    if done.is_ok() && !shows_error {
        tab.state.info = Some(match label {
            Some(l) => format!("{word}: {l}"),
            None => word.into(),
        });
    }
}

/// Kürzel oder Menü „Speichern“: fragt nach Kommentar (Dialog), Ort oder Zweig.
pub(crate) fn begin_save(ws: &mut Workspace, as_new: bool) {
    let tab = ws.active_tab();
    if tab.read_only {
        return;
    }
    let (id, has_store, name) = (tab.id, tab.store.is_some(), tab.name.clone());
    ws.prompt = Some(if as_new || !has_store {
        Prompt::SaveAs {
            tab: id,
            then: None,
            parent: Vec::new(),
            name,
            comment: String::new(),
        }
    } else {
        Prompt::SaveComment {
            tab: id,
            then: None,
            comment: String::new(),
            branch: None,
        }
    });
}

/// Datei → Exportieren … (auch über sein Kürzel): öffnet das Fenster, wenn es Körper gibt; eine
/// offene Skizze wird vorher beendet. Auch in schreibgeschützten Ansichten möglich.
pub(crate) fn open_export(ws: &mut Workspace) {
    let tab = ws.active_tab_mut();
    if tab.state.bodies.is_empty() || tab.state.try_close_sketch().is_err() {
        return;
    }
    tab.export_dialog = Some(ExportDialog::new(&tab.state));
}

/// Die Fenster mit Dateiauswahl des aktiven Tabs (Exportieren, Netz importieren, STEP
/// importieren): gemeinsamer zuletzt benutzter Ordner und gemeinsame Schnellzugriffe.
/// Geänderte Favoriten werden gleich gespeichert ([`Workspace::save_places`]).
#[doc(hidden)]
pub fn show_file_dialogs(ctx: &egui::Context, ws: &mut Workspace) {
    // Der Ordner der Dateiauswahl wird für die Fenster kurz herausgenommen.
    let mut exchange_dir = ws.exchange_dir.take();
    let mut places = ws.places();
    let tab = ws.active_tab_mut();
    if let Some(mut dialog) = tab.export_dialog.take()
        && dialog.show_in(ctx, &mut tab.state, &mut exchange_dir, &mut places)
    {
        tab.export_dialog = Some(dialog);
    }
    if let Some(mut dialog) = tab.import_dialog.take()
        && dialog.show_in(ctx, &mut tab.state, &mut exchange_dir, &mut places)
    {
        tab.import_dialog = Some(dialog);
    }
    if let Some(mut dialog) = tab.step_import_dialog.take()
        && dialog.show_in(ctx, &mut tab.state, &mut exchange_dir, &mut places)
    {
        tab.step_import_dialog = Some(dialog);
    }
    ws.exchange_dir = exchange_dir;
    ws.save_places(places);
}

/// Ist ein Dateifenster (Exportieren, Netz importieren, STEP importieren) des aktiven Tabs offen?
fn file_window_open(ws: &Workspace) -> bool {
    let tab = ws.active_tab();
    tab.export_dialog.is_some() || tab.import_dialog.is_some() || tab.step_import_dialog.is_some()
}

/// P10: schließt die Dateifenster des aktiven Tabs für Projekte oder Einstellungen. Ein
/// schreibendes Exportfenster bleibt (sonst endete das Schreiben ohne Meldung); dann `false`,
/// das andere Fenster öffnet nicht und die Statuszeile sagt warum ([`EXPORT_BUSY`]).
fn close_file_windows(ws: &mut Workspace) -> bool {
    let tab = ws.active_tab_mut();
    if tab
        .export_dialog
        .as_ref()
        .is_some_and(|d| d.is_writing_file())
    {
        tab.state.message = Some(EXPORT_BUSY.into());
        tab.state.info = None;
        return false;
    }
    tab.export_dialog = None;
    tab.import_dialog = None;
    tab.step_import_dialog = None;
    true
}

/// Schließt das Fenster Projekte samt den Fenstern seiner Aktionen (sonst blieben sie unsichtbar
/// offen und sperrten Rückgängig).
fn close_projects(projects: &mut ProjectsWindow) {
    projects.ui.close_dialogs();
    if projects.is_open() {
        projects.close();
    }
}

/// P10: schließt die Dateifenster des aktiven Tabs (nicht ein schreibendes Export-Fenster; dann
/// bleibt Projekte zu) und Einstellungen; öffnet Projekte (der Stand wird neu gelesen).
pub fn open_projects(
    ws: &mut Workspace,
    projects: &mut ProjectsWindow,
    settings: &mut SettingsWindow,
) {
    if !close_file_windows(ws) {
        return;
    }
    settings.close();
    projects.open();
}

/// P10: wie [`open_projects`] für die Einstellungen; Projekte schließt dabei. Mit
/// `return_to_projects` (Knopf „Projektverzeichnis …“) öffnet das Schließen der Einstellungen
/// wieder Projekte. Bei offener Rückfrage öffnen die Einstellungen nicht: deren Pfade (etwa
/// „Dokument gesperrt“) gehören zum aktuellen Projektverzeichnis.
pub fn open_settings(
    ws: &mut Workspace,
    projects: &mut ProjectsWindow,
    settings: &mut SettingsWindow,
    return_to_projects: bool,
) {
    if ws.prompt.is_some() || !close_file_windows(ws) {
        return;
    }
    close_projects(projects);
    settings.open(return_to_projects);
}

/// Öffnet die Übersicht „Tastenkürzel“ (Menü Ansicht und Taste S) mit aktivem Suchfeld. Sie
/// steht für sich wie Projekte und Einstellungen (P10): diese und die Dateifenster des aktiven
/// Tabs schließen dabei (ein schreibendes Exportfenster bleibt, dann öffnet sie nicht,
/// [`EXPORT_BUSY`]). Bei offener Rückfrage öffnet sie nicht (wie die Einstellungen).
pub fn open_overview(
    ws: &mut Workspace,
    projects: &mut ProjectsWindow,
    settings: &mut SettingsWindow,
    overview: &mut OverviewState,
) {
    if ws.prompt.is_some() || !close_file_windows(ws) {
        return;
    }
    close_projects(projects);
    settings.close();
    overview.open_focused();
}

/// Die Übersicht „Tastenkürzel“ schließt, sobald Projekte, Einstellungen oder ein Dateifenster
/// des aktiven Tabs offen ist (das zuletzt geöffnete Fenster gewinnt; die Übersicht öffnet sie
/// selbst zu, [`open_overview`]).
pub fn enforce_overview_rules(
    ws: &Workspace,
    projects: &ProjectsWindow,
    settings: &SettingsWindow,
    overview: &mut OverviewState,
) {
    if overview.open && (file_window_open(ws) || projects.is_open() || settings.is_open()) {
        overview.close();
    }
}

/// Was die Meldung während eines Updates zeigt: während des Neustarts „Neustart …“, sonst den
/// Stand des Downloads.
fn install_view(restart: RestartPhase, state: &InstallState) -> InstallView<'_> {
    if restart != RestartPhase::None {
        return InstallView::Restarting;
    }
    match state {
        InstallState::Idle => InstallView::Preparing,
        InstallState::Downloading { done, total } => InstallView::Downloading {
            done: *done,
            total: *total,
        },
        InstallState::ReadyToRestart => InstallView::Waiting,
        InstallState::Failed(message) => InstallView::Failed(message),
    }
}

/// Öffnet die Release-Seite von `update` im Browser (über egui, das `webbrowser` benutzt).
fn open_release_page(ctx: &egui::Context, update: &Update) {
    ctx.open_url(egui::OpenUrl::new_tab(update_install::release_page(
        &update.latest.tag,
    )));
}

/// Öffnet die Meldung „Neue Version verfügbar“ für `update` (Klick auf den Hinweis in der
/// Statuszeile). Sie steht für sich wie die Übersicht der Tastenkürzel: Projekte, Einstellungen,
/// die Übersicht und die Dateifenster des aktiven Tabs schließen dabei (ein schreibendes
/// Exportfenster bleibt, dann öffnet sie nicht, [`EXPORT_BUSY`]). Bei offener Rückfrage öffnet
/// sie nicht.
pub fn open_update_notice(
    ws: &mut Workspace,
    projects: &mut ProjectsWindow,
    settings: &mut SettingsWindow,
    overview: &mut OverviewState,
    notice: &mut UpdateDialog,
    update: &Update,
) {
    if ws.prompt.is_some() || !close_file_windows(ws) {
        return;
    }
    close_projects(projects);
    settings.close();
    overview.close();
    notice.open_with(update.clone());
}

/// Die Meldung „Neue Version verfügbar“ tritt zurück, sobald eine Rückfrage, Projekte,
/// Einstellungen, die Übersicht der Tastenkürzel oder ein Dateifenster des aktiven Tabs offen ist
/// (das zuletzt geöffnete Fenster gewinnt; die Meldung öffnet nur, wenn nichts davon offen ist,
/// oder schließt es selbst, [`open_update_notice`]). Das ist kein „Später“: Sie erscheint wieder,
/// sobald all das zu ist.
pub fn enforce_update_rules(
    ws: &Workspace,
    projects: &ProjectsWindow,
    settings: &SettingsWindow,
    overview: &OverviewState,
    notice: &mut UpdateDialog,
) {
    if notice.open
        && (ws.prompt.is_some()
            || file_window_open(ws)
            || projects.is_open()
            || settings.is_open()
            || overview.open)
    {
        notice.close();
    }
}

/// P10: Ist ein Dateifenster des aktiven Tabs offen (neu geöffnet oder nach einem Tabwechsel),
/// schließen Projekte und Einstellungen (die Dateifenster haben Vorrang, ohne Rückweg).
pub fn enforce_window_rules(
    ws: &Workspace,
    projects: &mut ProjectsWindow,
    settings: &mut SettingsWindow,
) {
    if file_window_open(ws) {
        close_projects(projects);
        settings.close();
    }
}

/// Die Fenster Projekte und Einstellungen; nach [`show_file_dialogs`], vor den Rückfragen. Erst
/// die Regeln ([`enforce_window_rules`]), dann Projekte (liest vor dem Zeichnen den Stand;
/// „Projektverzeichnis …“ führt mit Rückweg zu den Einstellungen), dann die Einstellungen mit den
/// Schnellzugriffen des Arbeitsbereichs (geänderte Favoriten werden gespeichert; Schließen mit
/// Rückweg öffnet wieder Projekte).
#[doc(hidden)]
pub fn show_windows(
    ctx: &egui::Context,
    ws: &mut Workspace,
    projects: &mut ProjectsWindow,
    settings: &mut SettingsWindow,
) {
    enforce_window_rules(ws, projects, settings);
    match projects.show(ctx, ws) {
        ProjectsEvent::OpenSettings => open_settings(ws, projects, settings, true),
        ProjectsEvent::None | ProjectsEvent::Closed => {}
    }
    if let SettingsEvent::Closed {
        return_to_projects: true,
    } = settings.show_in(ctx, ws)
    {
        open_projects(ws, projects, settings);
    }
}

/// Datei → Netz importieren …: öffnet das Fenster im zuletzt benutzten Ordner; eine offene
/// Skizze wird vorher beendet. Nicht in schreibgeschützten Ansichten.
fn open_import(ws: &mut Workspace) {
    let dir = ws.exchange_dir.clone().unwrap_or_else(default_dir);
    let tab = ws.active_tab_mut();
    if tab.read_only || tab.state.try_close_sketch().is_err() {
        return;
    }
    // Die beiden Import-Fenster schließen einander aus (sie lägen sonst übereinander).
    tab.step_import_dialog = None;
    tab.import_dialog = Some(ImportDialog::new(dir));
}

/// Datei → STEP importieren …: wie [`open_import`]. Die offene Skizze wird schon hier beendet
/// (nicht erst beim Import), damit Rückgängig des Imports nur die Körper zurücknimmt.
fn open_step_import(ws: &mut Workspace) {
    let dir = ws.exchange_dir.clone().unwrap_or_else(default_dir);
    let tab = ws.active_tab_mut();
    if tab.read_only || tab.state.try_close_sketch().is_err() {
        return;
    }
    tab.import_dialog = None;
    tab.step_import_dialog = Some(StepImportDialog::new(dir));
}

/// Menüeintrag (gesperrt, wenn nicht `enabled`) mit sichtbarem Tastaturfokus: die Einträge haben
/// keinen Rand, der Fokus zeigt sich als 2-px-Ring in Akzentfarbe.
fn entry(ui: &mut egui::Ui, enabled: bool, widget: impl egui::Widget) -> egui::Response {
    theme::entry(ui, enabled, widget)
}

/// Menüeintrag: Symbol, Beschriftung und rechtsbündig die Taste `keys` (der Tastentext aus
/// [`keymap::shortcut_hint`], `kbd`-Schrift, schwach), jedes ein eigener Text; die Beschriftung
/// wird nicht mit Leerzeichen aufgefüllt. Ohne Taste (leerer Text) steht rechts nichts.
fn shortcut_button(
    ui: &mut egui::Ui,
    enabled: bool,
    icon: Icon,
    label: &str,
    keys: String,
) -> egui::Response {
    let mut button = icons::button(icon, label);
    if !keys.is_empty() {
        button = button.shortcut_text(egui::RichText::new(keys).font(theme::kbd_font()));
    }
    entry(ui, enabled, button)
}

/// Menü „Datei“. „Projekte …“ und „Einstellungen …“ öffnen Fenster der App: sie kommen als
/// [`MenuRequest`] zurück, der Aufrufer öffnet sie ([`open_projects`], [`open_settings`]).
#[doc(hidden)]
pub fn file_menu(ui: &mut egui::Ui, ws: &mut Workspace) -> MenuRequest {
    let read_only = ws.active_tab().read_only;
    let mut request = MenuRequest::None;
    let hint = |ws: &Workspace, a| keymap::shortcut_hint(&ws.keymap, a);
    let (new_keys, projects_keys, save_keys, export_keys, close_keys) = (
        hint(ws, Action::New),
        hint(ws, Action::Projects),
        hint(ws, Action::Save),
        hint(ws, Action::Export),
        hint(ws, Action::Close),
    );
    if shortcut_button(ui, true, Icon::New, "Neu", new_keys).clicked() {
        commands::run_action(ws, Action::New);
        ui.close();
    }
    if shortcut_button(ui, true, Icon::Projects, PROJECTS_ENTRY, projects_keys).clicked() {
        request = MenuRequest::Projects;
        ui.close();
    }
    ui.separator();
    if shortcut_button(ui, !read_only, Icon::Save, "Speichern", save_keys).clicked() {
        commands::run_action(ws, Action::Save);
        ui.close();
    }
    if entry(
        ui,
        !read_only,
        icons::button(Icon::SaveAs, "Speichern unter …"),
    )
    .clicked()
    {
        begin_save(ws, true);
        ui.close();
    }
    ui.separator();
    let any = !ws.active_tab().state.bodies.is_empty();
    if shortcut_button(ui, any, Icon::Export, "Exportieren …", export_keys)
        .on_disabled_hover_text("Es gibt noch keinen Körper.")
        .clicked()
    {
        commands::run_action(ws, Action::Export);
        ui.close();
    }
    if entry(
        ui,
        !read_only,
        icons::button(Icon::Import, "Netz importieren …"),
    )
    .on_disabled_hover_text("In einer alten Version lässt sich nichts hinzufügen.")
    .clicked()
    {
        open_import(ws);
        ui.close();
    }
    if entry(
        ui,
        !read_only,
        icons::button(Icon::Import, step_import_dialog::MENU_LABEL),
    )
    .on_disabled_hover_text("In einer alten Version lässt sich nichts hinzufügen.")
    .clicked()
    {
        open_step_import(ws);
        ui.close();
    }
    ui.separator();
    if shortcut_button(ui, true, Icon::Close, "Schließen", close_keys).clicked() {
        commands::run_action(ws, Action::Close);
        ui.close();
    }
    if entry(
        ui,
        ws.prompt.is_none(),
        icons::button(Icon::Settings, SETTINGS_ENTRY),
    )
    .on_disabled_hover_text("Zuerst die offene Rückfrage beantworten.")
    .clicked()
    {
        request = MenuRequest::Settings;
        ui.close();
    }
    if entry(ui, true, icons::button(Icon::About, about_dialog::TITLE)).clicked() {
        ws.about_open = true;
        ui.close();
    }
    if entry(ui, true, icons::button(Icon::Quit, "Beenden")).clicked() {
        // Ist Beenden freigegeben, schickt `handle_close_request` im nächsten Bild `Close`.
        ws.request_quit();
        ui.close();
    }
    request
}

/// Menüleiste im oberen Feld; „Ansicht“ blendet Versionen über den [`ShellState`] ein und aus.
/// Liefert den Wunsch aus „Datei“ ([`file_menu`]).
#[doc(hidden)]
pub fn menu_bar(ui: &mut egui::Ui, ws: &mut Workspace, shell: &mut ShellState) -> MenuRequest {
    menu_bar_measured(ui, ws, shell).0
}

/// Wie [`menu_bar`], dazu die Breite, die die Menüknöpfe selbst brauchen (die Leiste füllt sonst
/// die ganze verfügbare Breite).
pub(crate) fn menu_bar_measured(
    ui: &mut egui::Ui,
    ws: &mut Workspace,
    shell: &mut ShellState,
) -> (MenuRequest, f32) {
    let read_only = ws.active_tab().read_only;
    let mut request = MenuRequest::None;
    let mut width = 0.0;
    egui::MenuBar::new()
        .config(egui::containers::menu::MenuConfig::new().style(theme::menu_style))
        .ui(ui, |ui| {
            let start = ui.cursor().left();
            ui.menu_button("Datei", |ui| request = file_menu(ui, ws));
            ui.add_enabled_ui(!read_only, |ui| {
                ui.menu_button("Erzeugen", |ui| create_menu(ui, ws));
            });
            // Zeichnungen lassen sich auch in alten Versionen öffnen, aber nicht anlegen.
            ui.menu_button("Zeichnung", |ui| {
                let tab = ws.active_tab_mut();
                drawing_menu(ui, &mut tab.state, &mut tab.drawing_dialog, read_only);
            });
            ui.add_enabled_ui(!read_only, |ui| {
                ui.menu_button("Bearbeiten", |ui| edit_menu(ui, ws));
            });
            ui.menu_button("Ansicht", |ui| {
                if view_menu(ui, ws, shell) {
                    request = MenuRequest::Shortcuts;
                }
            });
            // Der Zeiger steht hinter dem letzten Knopf und einem Abstand.
            width = ui.cursor().left() - start - ui.spacing().item_spacing.x;
        });
    (request, width)
}

/// „Ansicht“: Versionen im rechten Feld (die Projekte stehen im Fenster „Projekte“).
fn shell_toggles(ui: &mut egui::Ui, shell: &mut ShellState) {
    let mut versions = shell.versions_visible;
    if ui
        .checkbox(
            &mut versions,
            (icons::atom(Icon::PanelVersions), "Versionen"),
        )
        .changed()
    {
        shell.set_versions_visible(versions);
    }
}

/// Körper, die sich über „Erzeugen“ (Menü und Befehlsfeld) anlegen lassen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateKind {
    Box,
    Cylinder,
    Sphere,
    Cone,
    Extrusion,
    Revolution,
}

impl CreateKind {
    pub const ALL: [CreateKind; 6] = [
        CreateKind::Box,
        CreateKind::Cylinder,
        CreateKind::Sphere,
        CreateKind::Cone,
        CreateKind::Extrusion,
        CreateKind::Revolution,
    ];

    /// Beschriftung im Menü (und Hinweistext im Befehlsfeld).
    pub fn label(self) -> &'static str {
        match self {
            CreateKind::Box => "Quader …",
            CreateKind::Cylinder => "Zylinder …",
            CreateKind::Sphere => "Kugel …",
            CreateKind::Cone => "Kegel …",
            CreateKind::Extrusion => "Skizze extrudieren …",
            CreateKind::Revolution => "Skizze drehen …",
        }
    }

    /// Die Aktion der Tastenbelegung zu diesem Befehl (nur Skizze extrudieren hat eine).
    pub fn action(self) -> Option<Action> {
        (self == CreateKind::Extrusion).then_some(Action::ExtrudeSketch)
    }

    /// Symbol (wie im Menü).
    pub fn icon(self) -> icons::IconSource {
        icons::body_icon(match self {
            CreateKind::Box => BodyKind::Box,
            CreateKind::Cylinder => BodyKind::Cylinder,
            CreateKind::Sphere => BodyKind::Sphere,
            CreateKind::Cone => BodyKind::Cone,
            CreateKind::Extrusion => BodyKind::Extrusion,
            CreateKind::Revolution => BodyKind::Revolution,
        })
    }
}

/// Die Skizze für „Extrudieren“ und „Drehen“: die gewählte, sonst die geöffnete, sonst die letzte.
fn sketch_target(state: &AppState) -> Option<planora_feature::FeatureId> {
    let sketches = state.sketch_features();
    state
        .selected_feature
        .or(state.open_sketch_id())
        .filter(|id| sketches.iter().any(|(s, _)| s == id))
        .or(sketches.last().map(|(id, _)| *id))
}

/// Grund, warum `kind` gerade nicht geht (Menü und Befehlsfeld); `None`: erlaubt.
/// Körper aus der Skizze eines zurückgesetzten Körpers sind gesperrt
/// ([`crate::history_bar::lock_reason`]); freistehende Grundkörper nie.
pub fn create_blocked(ws: &Workspace, kind: CreateKind) -> Option<&'static str> {
    let state = &ws.active_tab().state;
    match kind {
        CreateKind::Extrusion | CreateKind::Revolution => match sketch_target(state) {
            None => Some("Zuerst eine Skizze anlegen."),
            Some(sketch) => crate::history_bar::lock_reason(state, &[sketch]),
        },
        _ => None,
    }
}

/// Öffnet den Erzeugen-Dialog zu `kind` im aktiven Tab. Körper entstehen außerhalb des
/// Skizzen-Modus: eine offene Skizze wird vorher beendet. Wird der Entwurf abgelehnt, bleibt die
/// Skizze offen und der Dialog entfällt. Ist `kind` gesperrt ([`create_blocked`], etwa die
/// Skizze eines zurückgesetzten Körpers), steht der Grund in `message`.
pub fn do_create(ws: &mut Workspace, kind: CreateKind) {
    if let Some(why) = create_blocked(ws, kind).filter(|w| *w == crate::history_bar::LOCK_HINT) {
        ws.active_tab_mut().state.message = Some(why.into());
        return;
    }
    let tab = ws.active_tab_mut();
    let dialog = match kind {
        CreateKind::Box => CreateDialog::Box {
            x: 40.0,
            y: 30.0,
            z: 20.0,
        },
        CreateKind::Cylinder => CreateDialog::Cylinder {
            radius: 15.0,
            height: 30.0,
        },
        CreateKind::Sphere => CreateDialog::Sphere { radius: 20.0 },
        CreateKind::Cone => CreateDialog::Cone {
            radius: 20.0,
            height: 40.0,
        },
        CreateKind::Extrusion | CreateKind::Revolution => {
            let Some(i) = sketch_target(&tab.state) else {
                return;
            };
            if tab.state.try_close_sketch().is_err() {
                return;
            }
            tab.dialog = Some(if kind == CreateKind::Revolution {
                CreateDialog::revolve_sketch(&tab.state, i)
            } else {
                CreateDialog::extrude_sketch(i)
            });
            return;
        }
    };
    if tab.state.try_close_sketch().is_ok() {
        tab.dialog = Some(dialog);
    }
}

/// Öffnet den Dialog „Boolesche Operation“ mit vorgewählter Operation (Menü und Chips).
pub fn open_boolean(ws: &mut Workspace, op: BoolOp) {
    let tab = ws.active_tab_mut();
    if let Some(why) = tab.state.chosen_lock() {
        tab.state.message = Some(why.into());
        return;
    }
    if tab.state.try_close_sketch().is_ok() {
        tab.boolean_dialog = Some(BooleanDialog::with_op(&tab.state, op));
    }
}

/// Öffnet den Dialog Verrunden (`chamfer == false`) oder Fase.
pub fn open_blend(ws: &mut Workspace, chamfer: bool) {
    let tab = ws.active_tab_mut();
    if let Some(why) = tab.state.chosen_lock() {
        tab.state.message = Some(why.into());
        return;
    }
    if tab.state.try_close_sketch().is_ok() {
        tab.blend_dialog = Some(if chamfer {
            BlendDialog::chamfer()
        } else {
            BlendDialog::fillet()
        });
    }
}

/// Menü „Erzeugen“: Grundkörper, Skizzen und Körper aus Skizzen. „Skizze extrudieren …“ läuft
/// über [`commands::run_action`] und zeigt seine Taste.
#[doc(hidden)]
pub fn create_menu(ui: &mut egui::Ui, ws: &mut Workspace) {
    for kind in &CreateKind::ALL[..4] {
        if icons::source_entry(ui, kind.icon(), kind.label(), true).clicked() {
            do_create(ws, *kind);
            ui.close();
        }
    }
    ui.separator();
    sketch_menu(ui, ws);
    ui.separator();
    // Körper aus Skizzen: die gewählte (sonst die geöffnete, sonst die letzte) Skizze.
    for kind in &CreateKind::ALL[4..] {
        let blocked = create_blocked(ws, *kind);
        let action = kind.action();
        let keys = action.map_or_else(String::new, |a| keymap::shortcut_hint(&ws.keymap, a));
        if icons::source_entry_keys(ui, kind.icon(), kind.label(), &keys, blocked.is_none())
            .on_disabled_hover_text(blocked.unwrap_or_default())
            .clicked()
        {
            run_create(ws, *kind);
            ui.close();
        }
    }
}

/// Erzeugen-Befehl aus Menü oder Befehlsfeld: mit eigener Aktion (Skizze extrudieren) über
/// [`commands::run_action`], sonst [`do_create`].
pub(crate) fn run_create(ws: &mut Workspace, kind: CreateKind) {
    match kind.action() {
        Some(a) => {
            commands::run_action(ws, a);
        }
        None => do_create(ws, kind),
    }
}

/// Verrunden (über [`commands::run_action`]) oder Fase aus Menü oder Befehlsfeld.
pub(crate) fn run_blend(ws: &mut Workspace, chamfer: bool) {
    if chamfer {
        open_blend(ws, true);
    } else {
        commands::run_action(ws, Action::Fillet);
    }
}

/// Text des Menüeintrags „Rückgängig“: mit Aktionsnamen „Rückgängig: Quader anlegen“, ohne
/// Schritt nur „Rückgängig“.
pub fn undo_menu_text(label: Option<&str>) -> String {
    match label {
        Some(l) => format!("Rückgängig: {l}"),
        None => "Rückgängig".into(),
    }
}

/// Text des Menüeintrags „Wiederholen“ (wie [`undo_menu_text`]).
pub fn redo_menu_text(label: Option<&str>) -> String {
    match label {
        Some(l) => format!("Wiederholen: {l}"),
        None => "Wiederholen".into(),
    }
}

/// Menüeintrag „Bearbeiten → Parameter …“.
pub const PARAMETERS_ENTRY: &str = "Parameter …";

/// Menü „Bearbeiten“: Rückgängig, Wiederholen, Parameter, Boolesche Operationen, Verrunden und
/// Fase (Verrunden mit seiner Taste), Zum Ende der Zeitleiste.
#[doc(hidden)]
pub fn edit_menu(ui: &mut egui::Ui, ws: &mut Workspace) {
    // Rückgängig und Wiederholen: gesperrt mit Hilfetext, wenn es nichts zu tun gibt, in alten
    // Versionen und während des Exports. Ein offenes Fenster sperrt nicht, es wird geschlossen.
    let tab = ws.active_tab();
    let (undo_block, redo_block) = (tab.undo_blocked(), tab.redo_blocked());
    let undo_text = undo_menu_text(tab.undo_label());
    let redo_text = redo_menu_text(tab.redo_label());
    let undo_keys = keymap::shortcut_hint(&ws.keymap, Action::Undo);
    let redo_keys = keymap::shortcut_hint(&ws.keymap, Action::Redo);
    if shortcut_button(ui, undo_block.is_none(), Icon::Undo, &undo_text, undo_keys)
        .on_disabled_hover_text(undo_block.unwrap_or_default())
        .clicked()
    {
        commands::run_action(ws, Action::Undo);
        ui.close();
    }
    if shortcut_button(ui, redo_block.is_none(), Icon::Redo, &redo_text, redo_keys)
        .on_disabled_hover_text(redo_block.unwrap_or_default())
        .clicked()
    {
        commands::run_action(ws, Action::Redo);
        ui.close();
    }
    ui.separator();
    // Parameter: gesperrt, solange eine Skizze offen ist (ihr Entwurf hängt an den alten Werten).
    let tab = ws.active_tab_mut();
    let sketch_open = tab.state.sketch_session.is_some();
    if entry(
        ui,
        !sketch_open,
        icons::button(Icon::Parameters, PARAMETERS_ENTRY),
    )
    .on_disabled_hover_text(crate::state::SKETCH_OPEN_LOCK)
    .clicked()
    {
        parameter_dialog::open(tab);
        ui.close();
    }
    ui.separator();
    // Boolesche Operationen brauchen zwei gewöhnliche Körper; Netz-Körper sind gesperrt.
    let boolean = ws.active_tab().state.boolean_blocked();
    for (icon, label, op) in [
        (Icon::Union, "Vereinigen …", BoolOp::Union),
        (Icon::Subtract, "Abziehen …", BoolOp::Difference),
        (Icon::Intersect, "Schneiden …", BoolOp::Intersection),
    ] {
        if entry(ui, boolean.is_none(), icons::button(icon, label))
            .on_disabled_hover_text(boolean.unwrap_or_default())
            .clicked()
        {
            open_boolean(ws, op);
            ui.close();
        }
    }
    ui.separator();
    let blend = ws.active_tab().state.blend_blocked();
    for (label, source, chamfer) in icons::blend_entries() {
        let keys = if chamfer {
            String::new()
        } else {
            keymap::shortcut_hint(&ws.keymap, Action::Fillet)
        };
        if icons::source_entry_keys(ui, source, label, &keys, blend.is_none())
            .on_disabled_hover_text(blend.unwrap_or_default())
            .clicked()
        {
            run_blend(ws, chamfer);
            ui.close();
        }
    }
    ui.separator();
    let tab = ws.active_tab_mut();
    let rolled_back = tab.state.document.marker() < tab.state.document.features().len();
    if entry(
        ui,
        rolled_back,
        icons::button(Icon::ToTimelineEnd, "Zum Ende der Zeitleiste"),
    )
    .on_disabled_hover_text("Es wird bereits die ganze Zeitleiste berechnet.")
    .clicked()
    {
        let _ = browser::roll_to_end(&mut tab.state);
        ui.close();
    }
}

/// Menü „Ansicht“: Blickrichtungen, Einpassen, Design, „Tastenkürzel …“ (mit seiner Taste) und
/// Versionen. Liefert, ob „Tastenkürzel …“ gewählt wurde (das Fenster gehört der App).
fn view_menu(ui: &mut egui::Ui, ws: &mut Workspace, shell: &mut ShellState) -> bool {
    let mut shortcuts = false;
    let state = &mut ws.active_tab_mut().state;
    for face in ViewFace::ALL {
        if entry(ui, true, icons::button(Icon::ViewFace, face.label())).clicked() {
            state.camera.set_view(face);
            ui.close();
        }
    }
    if entry(ui, true, icons::button(Icon::ViewIso, "Isometrisch")).clicked() {
        state.camera.set_iso_view();
        ui.close();
    }
    ui.separator();
    if entry(ui, true, icons::button(Icon::FitAll, "Alles einpassen")).clicked() {
        state.frame_all();
        ui.close();
    }
    ui.separator();
    let dark = state.dark;
    let (icon, label) = if dark {
        (Icon::ThemeLight, "Helles Design")
    } else {
        (Icon::ThemeDark, "Dunkles Design")
    };
    if entry(ui, true, icons::button(icon, label)).clicked() {
        ws.set_dark(!dark);
        ui.close();
    }
    ui.separator();
    let keys = keymap::shortcut_hint(&ws.keymap, Action::ShowShortcuts);
    if shortcut_button(ui, true, Icon::Keyboard, VIEW_SHORTCUTS_ENTRY, keys).clicked() {
        shortcuts = true;
        ui.close();
    }
    ui.separator();
    shell_toggles(ui, shell);
    shortcuts
}

/// Einträge „Skizze auf …“ im Menü „Erzeugen“ (ohne Untermenü).
fn sketch_menu(ui: &mut egui::Ui, ws: &mut Workspace) {
    let state = &mut ws.active_tab_mut().state;
    for (label, plane) in [
        ("Skizze auf XY-Ebene", StandardPlane::XY),
        ("Skizze auf XZ-Ebene", StandardPlane::XZ),
        ("Skizze auf YZ-Ebene", StandardPlane::YZ),
    ] {
        if entry(ui, true, icons::button(Icon::SketchOnPlane, label)).clicked() {
            if let Ok(id) = state.add_sketch(SketchPlane::Standard(plane)) {
                let _ = state.open_sketch(id);
            }
            ui.close();
        }
    }
    let face = match state.selection {
        Some(Pick::Face(FaceRef { body, face })) => state
            .bodies
            .get(body)
            .and_then(|b| plane_of_face(&b.solid, face))
            .map(|_| (body, face)),
        _ => None,
    };
    // Gesperrt auch, wenn der Körper der Fläche zurückgesetzt ist (Verlaufsleiste).
    let blocked = state.sketch_on_face_blocked();
    if entry(
        ui,
        blocked.is_none(),
        icons::button(Icon::SketchOnFace, "Skizze auf gewählter Fläche"),
    )
    .on_disabled_hover_text(blocked.unwrap_or_default())
    .clicked()
        && let Some((body, face)) = face
    {
        if let Ok(id) = state.new_sketch_on_face(body, face) {
            let _ = state.open_sketch(id);
        }
        ui.close();
    }
}

/// Schließen des Fensters (Titelleiste, Alt+F4, Taskleiste – auch minimiert): erst nach
/// Rückfrage bei ungespeicherten Änderungen oder offenem Autosicherungs-Angebot. Dann wird das
/// Schließen abgebrochen und das Fenster wieder hervorgeholt, damit die Rückfrage sichtbar ist.
/// Ist das Beenden freigegeben (`ws.quit`, z. B. nach „Verwerfen“ in der letzten Rückfrage),
/// schickt die Funktion selbst einmal `Close` (`close_sent` merkt sich das).
///
/// Läuft in `App::logic`, also vor jedem Bild und auch bei minimiertem Fenster. Mehrfaches
/// Aufrufen mit derselben Schließanfrage (mehrere Durchgänge eines Bildes, oder erst minimiert,
/// dann sichtbar) ist harmlos: `request_quit` setzt dieselbe Rückfrage noch einmal.
pub fn handle_close_request(ctx: &egui::Context, ws: &mut Workspace, close_sent: &mut bool) {
    if ctx.input(|i| i.viewport().close_requested()) {
        if !ws.quit && !ws.request_quit() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if ctx.input(|i| i.viewport().minimized == Some(true)) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    } else if ws.quit && !*close_sent {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        *close_sent = true;
    }
}

/// Körper- und Boolesche Dialoge gibt es nur außerhalb des Skizzen-Modus: Wird eine Skizze
/// geöffnet (Menü, Doppelklick im Browser), werden offene Dialoge verworfen.
pub fn drop_dialogs_in_sketch_mode(
    state: &AppState,
    create: &mut Option<CreateDialog>,
    boolean: &mut Option<BooleanDialog>,
) {
    if state.sketch_session.is_some() {
        *create = None;
        *boolean = None;
    }
}

/// Setzt einen Dialogwunsch aus dem Browser (Kontextmenü einer Skizze) in den passenden Dialog um
/// und löscht ihn. Der Browser hat die offene Skizze dann bereits beendet.
pub fn dialog_for_request(state: &mut AppState) -> Option<CreateDialog> {
    match state.sketch_request.take()? {
        SketchRequest::Extrude(i) => Some(CreateDialog::extrude_sketch(i)),
        SketchRequest::Revolve(i) => Some(CreateDialog::revolve_sketch(state, i)),
    }
}

impl eframe::App for PlanoraApp {
    /// Läuft vor jedem `ui` und – anders als `ui` – auch bei minimiertem oder verdecktem Fenster
    /// (eframe plant dann gedrosselte Wiederholungen ein). Deshalb stehen hier das Schließen
    /// und die Autosicherung.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.write_due_ok_marker(ctx.cumulative_frame_nr());
        // Vor dem Schließen: Nach der Rückfrage zum Neustart steht `quit`, das Programm darf
        // sich aber erst beenden, wenn die neue Version läuft.
        self.install_tick(ctx);
        if self.restart_locked() && !self.ws.quit && ctx.input(|i| i.viewport().close_requested()) {
            // Während des Neustarts beendet sich das Programm gleich selbst (oder läuft nach
            // dem Zurücklegen weiter); eine Rückfrage jetzt könnte unbeantwortet verloren gehen.
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        } else {
            handle_close_request(ctx, &mut self.ws, &mut self.close_sent);
        }
        self.ws.autosave_tick(Instant::now());
        ctx.request_repaint_after(AUTOSAVE_REPAINT);
    }

    /// Programmende: der Miniatur-Arbeiter des Fensters Projekte nimmt nichts mehr an; eine gerade
    /// entstehende Miniatur bekommt höchstens [`EXIT_WAIT`], um noch gespeichert zu werden.
    /// (Ohne diesen Aufruf, etwa bei bloßem Fallenlassen, wartet der Arbeiter nur
    /// [`crate::thumbnail_worker::DROP_WAIT`].)
    fn on_exit(&mut self) {
        self.projects.shutdown(EXIT_WAIT);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dark = self.ws.active_tab().state.dark;
        if self.applied_dark != Some(dark) {
            theme::apply(&ctx, dark);
            self.applied_dark = Some(dark);
        }

        self.update_tick(&ctx);
        // Schon vor der Statuszeile, damit sie im Bild, in dem die Meldung erscheint, nicht
        // „Update verfügbar“ zeigt.
        self.auto_open_update_notice();
        if !self.restart_locked() {
            self.shortcuts(&ctx);
        }
        // Esc der Übersicht vor den Ansichten (weder Auswahl noch Skizze sollen es sehen).
        keymap::overview::take_escape(&ctx, &mut self.overview, self.ws.prompt.is_some());
        // Esc der Meldung wirkt wie „Später“ (nie wie „Überspringen“), während eines Updates wie
        // ✕ ([`UpdateDialog::close_action`]).
        if update_dialog::take_escape(&ctx, &self.update_dialog, self.ws.prompt.is_some()) {
            self.close_update_notice(&ctx);
        }
        // Das Fenster „Parameter“ vor allen übrigen Bereichen: Übernimmt es eine Zeile, weil ein
        // Klick das Feld verlassen hat, wird die Änderung ein eigener Schritt, bevor die Ansicht
        // im selben Bild mit einem Ziehen beginnt (sonst ein gemeinsamer Schritt). Ein Fenster
        // schwebt ohnehin über den Feldern.
        parameter_dialog::show_in_tab(&ctx, self.ws.active_tab_mut());
        // Im Skizzen-Modus gibt es keinen Zeichnungsbereich: die Skizze braucht die 3D-Ansicht.
        let tab = self.ws.active_tab_mut();
        if tab.state.document_name != tab.name {
            tab.state.document_name.clone_from(&tab.name);
        }
        let state = &mut tab.state;
        if state.sketch_session.is_some() {
            state.drawing_ui.area = WorkArea::Model;
        }

        // Zuerst die schwebenden Felder (Menü und Tabs, Baum, Eigenschaften | Versionen, Befehle,
        // Status, Hinweis); sie liefern die freie Fläche zwischen sich. Der Modus folgt aus dem
        // Tab (`shell_panels::mode_of`): mit offener Skizze ohne Baum, Eigenschaften und Befehle,
        // deren Stelle die Skizzenfelder von `sketch_panel::show` (weiter unten) einnehmen. Menü und Tabs sind bei
        // offener Rückfrage gesperrt, Baum und Eigenschaften in alten Versionen.
        let free = shell_panels::show_fields(&ctx, self);
        // Darunter der Arbeitsbereich über das ganze Fenster: das Modell mit der Bildmitte in der
        // freien Fläche, ebenso das Blatt der Zeichnung (über das ganze Fenster gemalt, in die
        // freie Fläche eingepasst; Werkzeug-, Ansichts- und Hinweisfeld zeichnet `show_with`).
        let enabled = !self.ws.active_tab().read_only;
        // Mausgesten der Ansichten aus der Tastenbelegung (siehe `keymap::mouse`).
        keymap::mouse::set_current(&ctx, &self.ws.keymap);
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let state = &mut self.ws.active_tab_mut().state;
            match state.drawing_ui.area {
                WorkArea::Model => viewport::show_in(ui, state, Some(free)),
                WorkArea::Drawing(_) => drawing_view::show_with(ui, state, enabled),
            }
        });

        // Die Felder (Tab-Leiste, Menü) können den aktiven Tab gewechselt haben; ab hier gilt der
        // aktive Tab dieses Bilds. (Das Fenster Projekte öffnet Dokumente erst weiter unten.)
        let tab = self.ws.active_tab_mut();
        if let Some(dialog) = dialog_for_request(&mut tab.state) {
            tab.dialog = Some(dialog);
            // „Extrudieren …“ im Kontextmenü des Baums zählt wie der Menüeintrag.
            commands::track(&mut self.ws, Action::ExtrudeSketch);
        }
        let tab = self.ws.active_tab_mut();
        if let Some(id) = tab.state.edit_request.take() {
            tab.feature_dialog = FeatureDialog::new(&tab.state, id);
        }
        drop_dialogs_in_sketch_mode(&tab.state, &mut tab.dialog, &mut tab.boolean_dialog);
        // In einer alten Version lässt sich nichts ändern: das Fenster „Blattangaben“ (mit dem
        // Knopf für das Stücklistenblatt) gibt es dort nicht.
        if tab.read_only {
            tab.state.sheet_dialog = None;
        }
        if tab.state.sketch_session.is_some() {
            tab.feature_dialog = None;
            tab.blend_dialog = None;
            // Beim Schreiben einer STEP-Datei oder Zeichnung bleibt das Fenster, damit nichts ohne
            // Meldung endet.
            if tab
                .export_dialog
                .as_ref()
                .is_some_and(|d| !d.is_writing_file())
            {
                tab.export_dialog = None;
            }
            tab.import_dialog = None;
            tab.step_import_dialog = None;
            tab.drawing_dialog = None;
            tab.state.dimension_dialog = None;
            tab.state.sheet_dialog = None;
        }
        sketch_panel::show(&ctx, &mut tab.state);
        if let Some(mut dialog) = tab.dialog.take()
            && dialog.show(&ctx, &mut tab.state)
        {
            tab.dialog = Some(dialog);
        }
        // Auf jedem Weg geschlossen (Rückgängig, Skizze, Tab): die getippten Texte vergessen.
        if tab.dialog.is_none() {
            CreateDialog::forget_texts(&ctx, tab.state.instance);
        }
        if let Some(mut dialog) = tab.feature_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
        {
            tab.feature_dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.boolean_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
        {
            tab.boolean_dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.blend_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
        {
            tab.blend_dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.drawing_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
        {
            tab.drawing_dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.state.dimension_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
            && tab.state.dimension_dialog.is_none()
        {
            tab.state.dimension_dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.state.sheet_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
            && tab.state.sheet_dialog.is_none()
        {
            tab.state.sheet_dialog = Some(dialog);
        }
        if tab.blend_dialog.is_none() && tab.state.edge_picking {
            tab.state.edge_picking = false;
            tab.state.clear_edge_marks();
        }
        // Ein geschlossenes Fenster eines wiederholbaren Befehls: gemerkt, wenn ausgeführt.
        commands::observe(&mut self.ws);

        show_file_dialogs(&ctx, &mut self.ws);
        show_windows(&ctx, &mut self.ws, &mut self.projects, &mut self.settings);
        if self.settings.take_check_request() {
            self.start_update_check(true);
        }
        enforce_overview_rules(&self.ws, &self.projects, &self.settings, &mut self.overview);
        if keymap::overview::show(&ctx, &self.ws, &mut self.overview) == OverviewEvent::OpenSettings
        {
            open_settings(&mut self.ws, &mut self.projects, &mut self.settings, false);
        }
        self.ws.refresh_prompts();
        // Nach `refresh_prompts`: eine gerade entstandene Rückfrage hält die Meldung schon in
        // diesem Bild zurück.
        self.show_update_notice(&ctx);
        about_dialog::show(&ctx, &mut self.ws.about_open);
        prompts::show(&ctx, &mut self.ws);
        if self.restart_locked() {
            // Über allem: sperrt die Bedienung, bis die neue Version läuft oder der alte Stand
            // zurückliegt.
            egui::Modal::new(egui::Id::new("planora_update_restart"))
                .show(&ctx, |ui| ui.label(update_install::RESTARTING));
        }

        // Änderungen am Dokument werden Schritte für Rückgängig (R3, R4): erst wenn keine
        // Maustaste mehr gedrückt ist und kein Eingabefeld die Tastatur hat, damit Ziehen und
        // Tippen je ein Schritt sind. Vor `remember_keyboard_focus` zählt dabei auch das vorige
        // Bild; das verschiebt den Schritt nach Enter nur um ein Bild (beides ein Schritt).
        let gesture = ctx.input(|i| i.pointer.any_down())
            || viewport::typing(&ctx)
            || crate::history_bar::is_playing(&ctx);
        self.ws.observe_history(gesture);
        viewport::remember_keyboard_focus(&ctx);
        if self.ws.quit && !self.close_sent {
            // Beenden ist freigegeben (Menü oder letzte Rückfrage): `logic` des nächsten Bildes
            // schickt `Close`.
            ctx.request_repaint();
        }
        if self.first_frame.is_none() {
            self.first_frame = Some(ctx.cumulative_frame_nr());
        }
    }
}
