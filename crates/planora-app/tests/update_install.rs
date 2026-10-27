//! Update einspielen in der ganzen App: Download und Prüfung des Installers im Hintergrund,
//! Rückfrage vor dem Neustart (wie beim Schließen), zweite Prüfung, Start des Installers und
//! Ende des Programms, Abbruch der Admin-Abfrage, Start nach einem Update.
//!
//! Mit echten Bildern der App (`logic` und `ui` wie in eframe) bei 1920 × 1080, Nachbildungen
//! für Release-Liste, Download und Start des Installers; der Download liegt in einem Temp-Ordner,
//! nie startet ein echter Installer, nie geht etwas ins Netz, nie wird die Registry angefasst.

mod common;

use eframe::App as _;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, ViewportCommand, ViewportId, vec2};
use planora_app::app::{PlanoraApp, RestartPhase};
use planora_app::install_mode::InstallMode;
use planora_app::prompts::{SAVE_FOR_UPDATE_TITLE, save_for_update_text};
use planora_app::update_dialog::{
    CANCEL, CLOSE, FAILED_TITLE, INSTALL_NOW, InstallView, LATER, NOTES_ON_GITHUB, OK, RETRY, SKIP,
};
use planora_app::update_install::*;
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{fonts, running_version, theme};
use planora_math::DVec3;
use planora_project::config::{Config, PendingUpdate, WindowState};
use planora_project::library::Library;
use planora_update::check::{NetError, ReleaseSource};
use planora_update::install::{
    DownloadPaths, Downloader, InstallError, STALL_TIMEOUT, copy_limited,
};
use planora_update::sha256::hex_digest;
use planora_update::{REPO, SETUP_NAME, SHA256_NAME, Version};
use serde_json::{Value, json};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NEW_SETUP: &[u8] = b"Installer der Version 99";
const TARGET: &str = "99.0.0-dev.1";
const TAG: &str = "v99.0.0-dev.1";

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

/// Release-Liste mit einer neueren Version.
fn releases() -> String {
    let asset = |name: &str, size: usize| {
        json!({
            "name": name,
            "browser_download_url": format!("https://github.com/{REPO}/releases/download/{TAG}/{name}"),
            "size": size,
        })
    };
    Value::Array(vec![json!({
        "tag_name": TAG,
        "body": "- Schneller beim Öffnen",
        "assets": [asset(SETUP_NAME, NEW_SETUP.len()), asset(SHA256_NAME, 100)],
    })])
    .to_string()
}

struct Source;

impl ReleaseSource for Source {
    fn fetch_releases(&self) -> Result<String, NetError> {
        Ok(releases())
    }
}

/// Leser, der `data` in Stücken liefert; vor jedem Stück wartet er auf `gate`, falls gesetzt.
struct Chunks {
    data: Vec<u8>,
    at: usize,
    step: usize,
    gate: Option<mpsc::Receiver<()>>,
}

impl Read for Chunks {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at < self.data.len()
            && let Some(gate) = &self.gate
            && gate.recv().is_err()
        {
            return Err(io::Error::from(io::ErrorKind::ConnectionReset));
        }
        let n = buf.len().min(self.step).min(self.data.len() - self.at);
        buf[..n].copy_from_slice(&self.data[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// Nachbildung des Downloads: liefert die Prüfsummen-Datei `checksum` und den Installer
/// [`NEW_SETUP`] über die echte Kopierschleife; zählt die Aufrufe.
struct FakeDownloader {
    checksum: String,
    calls: AtomicUsize,
    /// Wird vor dem Laden des Installers abgeholt: steuert, wann Stücke kommen.
    gate: Mutex<Option<mpsc::Receiver<()>>>,
    step: usize,
    /// Fehler beim Laden des Installers, nachdem schon ein Teil geschrieben ist (etwa
    /// „Speicherplatz voll“).
    setup_error: Option<InstallError>,
}

impl FakeDownloader {
    fn new(checksum: String) -> Arc<FakeDownloader> {
        Arc::new(FakeDownloader {
            checksum,
            calls: AtomicUsize::new(0),
            gate: Mutex::new(None),
            step: 7,
            setup_error: None,
        })
    }

    fn failing(error: InstallError) -> Arc<FakeDownloader> {
        Arc::new(FakeDownloader {
            checksum: format!("{}  {SETUP_NAME}\n", hex_digest(NEW_SETUP)),
            calls: AtomicUsize::new(0),
            gate: Mutex::new(None),
            step: 7,
            setup_error: Some(error),
        })
    }

    fn good() -> Arc<FakeDownloader> {
        Self::new(format!("{}  {SETUP_NAME}\n", hex_digest(NEW_SETUP)))
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Downloader for FakeDownloader {
    fn download(
        &self,
        url: &str,
        dest: &Path,
        max_bytes: u64,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<(), InstallError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(planora_update::install::release_url_ok(url), "{url}");
        let (data, gate) = if url.ends_with(SHA256_NAME) {
            (self.checksum.clone().into_bytes(), None)
        } else {
            assert!(url.ends_with(SETUP_NAME));
            if let Some(e) = &self.setup_error {
                // Ein Teil liegt schon auf der Platte, dann scheitert das Schreiben.
                std::fs::write(dest, &NEW_SETUP[..10]).unwrap();
                return Err(e.clone());
            }
            (NEW_SETUP.to_vec(), self.gate.lock().unwrap().take())
        };
        let total = data.len() as u64;
        let reader = Chunks {
            data,
            at: 0,
            step: self.step,
            gate,
        };
        copy_limited(reader, total, dest, max_bytes, STALL_TIMEOUT, progress)
    }
}

/// Nachbildung des Starts des Installers: merkt sich Datei, Modus und den Inhalt der Datei im
/// Augenblick des Starts und antwortet mit `answer`. Startet nie etwas.
struct FakeLauncher {
    calls: Mutex<Vec<(PathBuf, InstallMode, Vec<u8>)>>,
    answer: Result<(), LaunchError>,
}

impl FakeLauncher {
    fn ok() -> Arc<FakeLauncher> {
        Self::answering(Ok(()))
    }

    fn answering(answer: Result<(), LaunchError>) -> Arc<FakeLauncher> {
        Arc::new(FakeLauncher {
            calls: Mutex::new(Vec::new()),
            answer,
        })
    }

    fn calls(&self) -> Vec<(PathBuf, InstallMode)> {
        let calls = self.calls.lock().unwrap();
        calls.iter().map(|(p, m, _)| (p.clone(), *m)).collect()
    }

    fn contents(&self) -> Vec<Vec<u8>> {
        let calls = self.calls.lock().unwrap();
        calls.iter().map(|(_, _, c)| c.clone()).collect()
    }
}

impl SetupLauncher for FakeLauncher {
    fn launch(&self, setup: &Path, mode: InstallMode) -> Result<(), LaunchError> {
        let contents = std::fs::read(setup).unwrap_or_default();
        self.calls
            .lock()
            .unwrap()
            .push((setup.to_path_buf(), mode, contents));
        self.answer.clone()
    }
}

struct Seen {
    text: String,
    rect: Rect,
}

struct Harness {
    ctx: egui::Context,
    app: PlanoraApp,
    cfg: PathBuf,
    paths: DownloadPaths,
    time: f64,
    seen: Vec<Seen>,
    commands: Vec<ViewportCommand>,
    urls: Vec<String>,
    /// Lage und Zustand des Programmfensters, wie eframe sie meldet (sonst keine Angaben).
    viewport: Option<egui::ViewportInfo>,
}

/// Arbeitsbereich mit Bibliothek und Konfiguration im Temp-Ordner; der Download-Ordner liegt
/// darin (`download`, noch nicht angelegt).
fn workspace(prefix: &str) -> (Workspace, PathBuf, DownloadPaths) {
    let tmp = common::temp_root(prefix);
    std::fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(
        Some(Library::open(&tmp.join("Projekte")).unwrap()),
        Config::default(),
    );
    let cfg = tmp.join("config.json");
    ws.config_path = cfg.clone();
    assert_ne!(cfg, Config::config_path(), "nie die echte Datei");
    (ws, cfg, DownloadPaths::in_dir(&tmp.join("download")))
}

/// Die Konfiguration liegt „in“ einer Datei: jedes Schreiben scheitert.
fn unwritable_config(ws: &mut Workspace, cfg: &Path) {
    let blocker = cfg.with_file_name("keine-mappe");
    std::fs::write(&blocker, "x").unwrap();
    ws.config_path = blocker.join("config.json");
}

impl Harness {
    fn new(
        prefix: &str,
        downloader: Arc<dyn Downloader + Send + Sync>,
        launcher: Arc<dyn SetupLauncher + Send + Sync>,
    ) -> Harness {
        let (ws, cfg, paths) = workspace(prefix);
        Self::with(ws, cfg, paths, downloader, launcher)
    }

    fn with(
        ws: Workspace,
        cfg: PathBuf,
        paths: DownloadPaths,
        downloader: Arc<dyn Downloader + Send + Sync>,
        launcher: Arc<dyn SetupLauncher + Send + Sync>,
    ) -> Harness {
        Self::with_mode(
            ws,
            cfg,
            paths,
            downloader,
            launcher,
            Some(InstallMode::User),
        )
    }

    fn with_mode(
        ws: Workspace,
        cfg: PathBuf,
        paths: DownloadPaths,
        downloader: Arc<dyn Downloader + Send + Sync>,
        launcher: Arc<dyn SetupLauncher + Send + Sync>,
        mode: Option<InstallMode>,
    ) -> Harness {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut app = PlanoraApp::for_tests_with_install(
            ws,
            Arc::new(Source),
            downloader,
            launcher,
            mode,
            paths.dir.clone(),
        );
        // Den Ablauf auf jedem System prüfen (auch im Linux-Teil der CI); nur
        // `non_windows_shows_hint_and_opens_release_page` schaltet das ab.
        app.set_auto_install_supported(true);
        Harness {
            ctx,
            app,
            cfg,
            paths,
            time: 0.0,
            seen: Vec::new(),
            commands: Vec::new(),
            urls: Vec::new(),
            viewport: None,
        }
    }

    /// Wie [`Harness::new`], bis die Meldung „Neue Version verfügbar“ offen ist.
    fn offered(
        prefix: &str,
        downloader: Arc<dyn Downloader + Send + Sync>,
        launcher: Arc<dyn SetupLauncher + Send + Sync>,
    ) -> Harness {
        let mut h = Self::new(prefix, downloader, launcher);
        h.until_offered();
        h
    }

    fn until_offered(&mut self) {
        self.until(|a| a.pending_notice().is_some());
        self.settle();
        assert!(self.app.update_dialog_mut().open);
    }

    /// Ein Bild wie in eframe: erst `logic`, dann `ui`. Merkt Texte, Fensterbefehle und
    /// geöffnete Adressen.
    fn frame(&mut self, events: Vec<Event>) -> Duration {
        self.frame_with(events, false)
    }

    /// Ein Bild mit einer Schließanfrage des Fensters (Titelleiste, Alt+F4).
    fn frame_close(&mut self) {
        self.frame_with(vec![], true);
    }

    fn frame_with(&mut self, events: Vec<Event>, close: bool) -> Duration {
        self.time += 1.0 / 60.0;
        let mut raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut info = self.viewport.clone().unwrap_or_default();
        if close {
            info.events = vec![egui::ViewportEvent::Close];
        }
        if close || self.viewport.is_some() {
            raw.viewports.insert(ViewportId::ROOT, info);
        }
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let start = Instant::now();
        let out = self.ctx.run_ui(raw, |ui| {
            app.logic(&ui.ctx().clone(), &mut f);
            app.ui(ui, &mut f);
        });
        let took = start.elapsed();
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Seen>) {
            match shape {
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                egui::epaint::Shape::Text(t) => out.push(Seen {
                    text: t.galley.text().to_string(),
                    rect: Rect::from_min_size(t.pos, t.galley.size()),
                }),
                _ => {}
            }
        }
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        self.seen = seen;
        if let Some(vp) = out.viewport_output.get(&ViewportId::ROOT) {
            self.commands.extend(vp.commands.iter().cloned());
        }
        for c in &out.platform_output.commands {
            if let egui::OutputCommand::OpenUrl(u) = c {
                self.urls.push(u.url.clone());
            }
        }
        out.drop_without_applying_deltas();
        took
    }

    fn settle(&mut self) {
        for _ in 0..4 {
            self.frame(vec![]);
        }
    }

    fn until(&mut self, mut done: impl FnMut(&PlanoraApp) -> bool) {
        let start = Instant::now();
        loop {
            self.frame(vec![]);
            if done(&self.app) {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "Zeit abgelaufen: {:?}",
                self.texts()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn sees(&self, text: &str) -> bool {
        self.seen.iter().any(|s| s.text == text)
    }

    fn sees_part(&self, part: &str) -> bool {
        self.seen.iter().any(|s| s.text.contains(part))
    }

    fn texts(&self) -> Vec<&str> {
        self.seen.iter().map(|s| s.text.as_str()).collect()
    }

    fn click(&mut self, text: &str) {
        self.settle();
        let at = self
            .seen
            .iter()
            .filter(|s| s.text == text)
            .max_by(|a, b| a.rect.top().partial_cmp(&b.rect.top()).unwrap())
            .unwrap_or_else(|| panic!("„{text}“ nicht zu sehen: {:?}", self.texts()))
            .rect
            .center();
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.settle();
    }

    fn state(&self) -> InstallState {
        self.app.installer().state().clone()
    }

    fn close_sent(&self) -> bool {
        self.commands
            .iter()
            .any(|c| matches!(c, ViewportCommand::Close))
    }

    fn pending_on_disk(&self) -> Option<(String, String)> {
        Config::load_from(&self.cfg)
            .update_pending
            .map(|p| (p.from, p.to))
    }

    fn notes_file(&self) -> PathBuf {
        self.cfg.with_file_name(NOTES_FILE_NAME)
    }

    /// Hält die zweite Prüfung an, bis über den gelieferten Sender etwas kommt (oder er fällt).
    fn hold_reverify(&mut self) -> mpsc::Sender<()> {
        let (tx, rx) = mpsc::channel();
        self.app.hold_reverify(rx);
        tx
    }
}

fn dirty(ws: &mut Workspace) {
    ws.active_tab_mut()
        .state
        .create_box(DVec3::splat(4.0))
        .unwrap();
}

/// Ein sichtbares Programmfenster, wie eframe es meldet.
fn visible_window() -> egui::ViewportInfo {
    egui::ViewportInfo {
        inner_rect: Some(Rect::from_min_size(
            Pos2::new(208.0, 131.0),
            vec2(1500.0, 850.0),
        )),
        outer_rect: Some(Rect::from_min_size(
            Pos2::new(200.0, 100.0),
            vec2(1516.0, 889.0),
        )),
        minimized: Some(false),
        maximized: Some(false),
        ..Default::default()
    }
}

#[test]
fn wrong_checksum_never_launches_and_shows_message() {
    let launcher = FakeLauncher::ok();
    for checksum in [
        format!("{}  {SETUP_NAME}", hex_digest(b"etwas anderes")),
        format!("{}  andere.exe", hex_digest(NEW_SETUP)),
        "Not Found".to_string(),
    ] {
        let downloader = FakeDownloader::new(checksum);
        let mut h = Harness::offered(
            "planora-install-summe",
            downloader.clone(),
            launcher.clone(),
        );
        h.click(INSTALL_NOW);
        h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
        assert_eq!(
            h.state(),
            InstallState::Failed(InstallError::Checksum.message())
        );
        assert_eq!(downloader.calls(), 2);
        h.settle();
        assert!(h.sees(&InstallError::Checksum.message()), "{:?}", h.texts());
        assert!(h.sees(OPEN_DOWNLOAD_PAGE));
        // Nichts gestartet, nichts liegen geblieben, nichts vorgemerkt.
        assert!(!h.paths.setup.exists());
        assert!(!h.paths.checksum.exists());
        assert!(!h.paths.dir.exists(), "leerer Download-Ordner gelöscht");
        assert!(launcher.calls().is_empty());
        assert_eq!(h.app.ws_mut().pending_update(), None);
        assert_eq!(h.app.restart_phase(), RestartPhase::None);
        assert!(!h.close_sent());
        // „Schließen“ beendet das Update; die Meldung ruht dann wie nach „Später“.
        h.click(CLOSE);
        assert_eq!(h.state(), InstallState::Idle);
        assert!(!h.app.update_dialog_mut().open);
        assert!(!h.app.update_dialog_mut().installing);
        assert!(h.sees(&planora_app::update_dialog::status_hint(&v(TARGET))));
    }
}

#[test]
fn prompt_for_unsaved_work_before_restart_and_cancel_aborts_the_update_and_deletes_the_setup() {
    let launcher = FakeLauncher::ok();
    let (mut ws, cfg, paths) = workspace("planora-install-rueckfrage");
    // Zwei Tabs mit ungespeicherten Änderungen.
    dirty(&mut ws);
    ws.new_tab();
    dirty(&mut ws);
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), launcher.clone());
    h.until_offered();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    h.settle();
    assert!(matches!(
        h.app.ws_mut().prompt,
        Some(Prompt::SaveChanges { .. })
    ));
    // Die Rückfrage sagt, dass für das Update neu gestartet wird.
    assert!(h.sees(SAVE_FOR_UPDATE_TITLE), "{:?}", h.texts());
    assert!(!h.sees("Änderungen speichern?"), "{:?}", h.texts());
    // Gefragt wird zuerst nach dem ersten geänderten Tab.
    let name = h.app.ws_mut().tabs[0].title();
    assert!(h.sees(&save_for_update_text(&name)), "{:?}", h.texts());
    assert!(h.sees("Speichern") && h.sees("Verwerfen") && h.sees("Abbrechen"));
    // Noch startet nichts neu: weder die Statuszeile noch die Meldung sagen „Neustart …“.
    assert!(!h.sees(RESTARTING), "{:?}", h.texts());
    assert!(!h.sees(RESTART_BOX), "{:?}", h.texts());
    assert!(h.sees(WAITING), "{:?}", h.texts());
    // Auch die Meldung selbst zeigt „wartet“, nicht den Neustart.
    assert_eq!(h.app.notice_view(), InstallView::Waiting);
    assert_eq!(h.app.restart_phase(), RestartPhase::AwaitingPrompt);
    assert_eq!(std::fs::read(&h.paths.setup).unwrap(), NEW_SETUP);
    // Erster Tab: Verwerfen; dann kommt der zweite.
    h.click("Verwerfen");
    assert!(matches!(
        h.app.ws_mut().prompt,
        Some(Prompt::SaveChanges { .. })
    ));
    assert_eq!(h.app.restart_phase(), RestartPhase::AwaitingPrompt);
    // Zweiter Tab: Abbrechen bricht das Update ab.
    h.click("Abbrechen");
    h.settle();
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
    assert_eq!(h.state(), InstallState::Idle);
    assert!(!h.paths.setup.exists(), "Installer gelöscht");
    assert!(!h.paths.dir.exists());
    assert!(launcher.calls().is_empty());
    assert_eq!(h.app.ws_mut().pending_update(), None);
    assert_eq!(h.pending_on_disk(), None);
    assert!(!h.app.ws_mut().quit);
    assert!(!h.close_sent());
    assert!(!h.app.update_dialog_mut().installing);
    // Der zweite Tab ist noch da und geändert.
    assert!(h.app.ws_mut().active_tab().is_dirty());
    // Eine Rückfrage beim gewöhnlichen Schließen nennt das Update nicht.
    h.frame_close();
    h.settle();
    assert!(h.sees("Änderungen speichern?"), "{:?}", h.texts());
    assert!(!h.sees(SAVE_FOR_UPDATE_TITLE));
}

#[test]
fn successful_flow_saves_pending_launches_setup_and_quits() {
    let launcher = FakeLauncher::ok();
    let mut h = Harness::offered(
        "planora-install-gut",
        FakeDownloader::good(),
        launcher.clone(),
    );
    h.viewport = Some(visible_window());
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    h.settle();
    // Einmal gestartet: der geladene Installer, „nur für mich“, mit dem geprüften Inhalt.
    assert_eq!(
        launcher.calls(),
        vec![(h.paths.dir.join(SETUP_NAME), InstallMode::User)]
    );
    assert_eq!(launcher.contents(), vec![NEW_SETUP.to_vec()]);
    // Der Installer bleibt liegen (er braucht seine Datei noch); die neue Version räumt auf.
    assert_eq!(std::fs::read(&h.paths.setup).unwrap(), NEW_SETUP);
    // Vorgemerkt, auch auf der Platte, mit dem Fenster.
    let running = running_version();
    assert_eq!(h.app.ws_mut().pending_update(), Some((running, v(TARGET))));
    assert_eq!(
        h.pending_on_disk(),
        Some((running.to_string(), TARGET.to_string()))
    );
    assert!(
        Config::load_from(&h.cfg)
            .update_pending
            .and_then(|p| p.window)
            .is_some(),
        "Fenster gemerkt"
    );
    // Die Patchnotes liegen für den Hinweis der neuen Version bereit.
    let notes = std::fs::read_to_string(h.notes_file()).unwrap();
    assert_eq!(notes, format!("{TARGET}\n- Schneller beim Öffnen"));
    // Das Programm beendet sich ohne weitere Rückfrage (sauber, über das Schließen).
    assert_eq!(h.app.restart_phase(), RestartPhase::Exiting);
    assert!(h.app.ws_mut().quit);
    assert!(h.app.ws_mut().prompt.is_none());
    assert!(h.close_sent());
    assert_eq!(
        RESTART_BOX,
        "PlanoraCAD wird für das Update beendet und startet danach von selbst neu."
    );
}

#[test]
fn all_users_mode_hands_the_mode_to_the_launcher() {
    let launcher = FakeLauncher::ok();
    let (ws, cfg, paths) = workspace("planora-install-alle");
    let mut h = Harness::with_mode(
        ws,
        cfg,
        paths,
        FakeDownloader::good(),
        launcher.clone(),
        Some(InstallMode::AllUsers),
    );
    h.until_offered();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(
        launcher.calls(),
        vec![(h.paths.setup.clone(), InstallMode::AllUsers)]
    );
}

/// Gemeinsamer Teil der Fälle, in denen der Installer nicht startet: das Programm läuft weiter,
/// nichts ist vorgemerkt, der Download ist weg, die Meldung zeigt den Fehler und bietet die
/// Version weiter an (auch wenn sie inzwischen nicht mehr vorgemerkt war).
fn launch_refused(prefix: &str, answer: LaunchError, message: &str) {
    let launcher = FakeLauncher::answering(Err(answer));
    let mut h = Harness::offered(prefix, FakeDownloader::good(), launcher.clone());
    let gate = h.hold_reverify();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::Verifying);
    *h.app.pending_notice_mut() = None;
    drop(gate);
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    h.settle();
    assert_eq!(h.state(), InstallState::Failed(message.to_string()));
    assert_eq!(launcher.calls().len(), 1);
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
    assert_eq!(h.app.ws_mut().pending_update(), None);
    assert_eq!(h.pending_on_disk(), None);
    assert!(
        !h.notes_file().exists(),
        "keine Patchnotes für eine neue Version"
    );
    assert!(!h.paths.setup.exists(), "Installer gelöscht");
    assert!(!h.app.ws_mut().quit);
    assert!(!h.close_sent());
    assert_eq!(h.app.ws_mut().skipped_version(), None);
    assert!(h.sees(FAILED_TITLE), "{:?}", h.texts());
    assert!(h.sees(message), "{:?}", h.texts());
    assert!(h.sees(RETRY), "{:?}", h.texts());
    assert_eq!(
        h.app.pending_notice().map(|u| u.latest.version),
        Some(v(TARGET))
    );
    // „Schließen“: die Meldung ruht, die Statuszeile bietet die Version weiter an.
    h.click(CLOSE);
    assert!(h.sees(&planora_app::update_dialog::status_hint(&v(TARGET))));
}

#[test]
fn declined_admin_prompt_keeps_running_with_cancel_hint() {
    launch_refused(
        "planora-install-admin-abgelehnt",
        LaunchError::Cancelled,
        UPDATE_CANCELLED,
    );
    assert_eq!(
        UPDATE_CANCELLED,
        "Update abgebrochen. Ohne die Erlaubnis von Windows lässt sich PlanoraCAD nicht für alle Benutzer aktualisieren. Du arbeitest in der bisherigen Version weiter."
    );
}

#[test]
fn launch_error_keeps_running_with_message() {
    launch_refused(
        "planora-install-startfehler",
        LaunchError::Failed("kaputt".into()),
        &launch_failed("kaputt"),
    );
    assert_eq!(
        launch_failed("kaputt"),
        "Das Update ließ sich nicht starten (kaputt). Du arbeitest in der bisherigen Version weiter."
    );
}

#[test]
fn pending_that_cannot_be_saved_aborts_before_launch() {
    let launcher = FakeLauncher::ok();
    let (mut ws, cfg, paths) = workspace("planora-install-vormerken");
    unwritable_config(&mut ws, &cfg);
    let cfg = ws.config_path.clone();
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), launcher.clone());
    h.until_offered();
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert_eq!(h.state(), InstallState::Failed(PENDING_NOT_SAVED.into()));
    assert!(!h.paths.setup.exists());
    assert!(launcher.calls().is_empty());
    assert!(!h.app.ws_mut().quit);
    h.settle();
    assert!(!h.close_sent());
}

#[test]
fn setup_changed_before_launch_is_not_started() {
    let launcher = FakeLauncher::ok();
    let (mut ws, cfg, paths) = workspace("planora-install-zweite-pruefung");
    dirty(&mut ws);
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), launcher.clone());
    h.until_offered();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    // Während die Rückfrage steht, verändert ein anderes Programm den Installer.
    std::fs::write(&h.paths.setup, b"halb geschrieben").unwrap();
    h.click("Verwerfen");
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert_eq!(
        h.state(),
        InstallState::Failed(InstallError::Checksum.message())
    );
    assert!(launcher.calls().is_empty(), "nichts gestartet");
    assert!(!h.paths.setup.exists(), "Installer gelöscht");
    assert_eq!(h.app.ws_mut().pending_update(), None);
    assert!(!h.app.ws_mut().quit);
    h.settle();
    assert!(!h.close_sent());
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
}

/// Nicht installiert (etwa aus dem Bauordner gestartet): die Meldung bietet nur die
/// Download-Seite, „Später“ und „Überspringen“; nichts wird geladen.
#[test]
fn not_installed_offers_only_the_download_page() {
    assert_eq!(decide(true, None), InstallDecision::NotInstalled);
    assert_eq!(
        decide(true, Some(InstallMode::User)),
        InstallDecision::Download
    );
    assert_eq!(
        decide(true, Some(InstallMode::AllUsers)),
        InstallDecision::Download
    );
    assert_eq!(
        decide(false, Some(InstallMode::User)),
        InstallDecision::OnlyWindows
    );

    let downloader = FakeDownloader::good();
    let launcher = FakeLauncher::ok();
    let (ws, cfg, paths) = workspace("planora-install-nicht-installiert");
    let mut h = Harness::with_mode(ws, cfg, paths, downloader.clone(), launcher.clone(), None);
    h.until_offered();
    assert!(h.sees(NOT_INSTALLED_HINT), "{:?}", h.texts());
    assert!(h.sees(OPEN_DOWNLOAD_PAGE), "{:?}", h.texts());
    assert!(h.sees(LATER) && h.sees(SKIP), "{:?}", h.texts());
    assert!(!h.sees(INSTALL_NOW), "{:?}", h.texts());
    h.click(OPEN_DOWNLOAD_PAGE);
    assert_eq!(h.urls, vec![release_page(TAG)]);
    assert_eq!(h.state(), InstallState::Idle);
    assert_eq!(downloader.calls(), 0);
    assert!(!h.paths.dir.exists(), "kein Download");
    assert!(launcher.calls().is_empty());
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
}

#[test]
fn startup_with_matching_pending_shows_notice_once_and_cleans_up() {
    let (mut ws, cfg, paths) = workspace("planora-install-start");
    let running = running_version();
    assert!(ws.set_pending_update(Some((v("0.0.1"), running))));
    write_notes(
        &cfg.with_file_name(NOTES_FILE_NAME),
        &running,
        "# Neu\n\n- Schneller beim Öffnen",
    );
    // Der Installer der alten Version liegt noch da.
    std::fs::create_dir_all(&paths.dir).unwrap();
    std::fs::write(&paths.setup, NEW_SETUP).unwrap();
    let mut h = Harness::with(
        ws,
        cfg.clone(),
        paths.clone(),
        FakeDownloader::good(),
        FakeLauncher::ok(),
    );
    h.app.run_update_startup();
    // Eintrag gelöscht, Download weg, Ablage gelesen.
    assert_eq!(h.app.ws_mut().pending_update(), None);
    assert_eq!(h.pending_on_disk(), None);
    assert!(!paths.setup.exists());
    assert!(!paths.dir.exists());
    assert!(!cfg.with_file_name(NOTES_FILE_NAME).exists());
    h.settle();
    assert!(h.sees(&updated_title(&running)), "{:?}", h.texts());
    assert!(h.sees("Schneller beim Öffnen"));
    assert!(h.sees(planora_app::update_dialog::FEEDBACK_BUTTON));
    h.click(OK);
    assert!(h.app.after_update().is_none());
    assert!(!h.sees(&updated_title(&running)));
    // Einmal: beim nächsten Start kein Hinweis mehr.
    h.app.run_update_startup();
    h.settle();
    assert!(h.app.after_update().is_none());
    assert!(!h.sees(&updated_title(&running)));

    // Ohne hinterlegte Patchnotes: nur die Zeile mit dem Hinweis auf GitHub.
    let (mut ws, cfg, paths) = workspace("planora-install-start-ohne-notizen");
    assert!(ws.set_pending_update(Some((v("0.0.1"), running))));
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), FakeLauncher::ok());
    h.app.run_update_startup();
    h.settle();
    assert!(h.sees(&updated_title(&running)));
    assert!(h.sees(NOTES_ON_GITHUB));
}

/// Ein liegengebliebener Download (etwa nach einem abgestürzten Lauf) wird beim Start
/// weggeräumt, auch ohne vorgemerktes Update; einen Hinweis gibt es dann nicht.
#[test]
fn startup_removes_a_leftover_download() {
    let (ws, cfg, paths) = workspace("planora-install-start-reste");
    std::fs::create_dir_all(&paths.dir).unwrap();
    std::fs::write(&paths.setup, NEW_SETUP).unwrap();
    std::fs::write(&paths.checksum, "x").unwrap();
    let mut h = Harness::with(
        ws,
        cfg,
        paths.clone(),
        FakeDownloader::good(),
        FakeLauncher::ok(),
    );
    h.app.run_update_startup();
    assert!(!paths.setup.exists());
    assert!(!paths.checksum.exists());
    assert!(!paths.dir.exists());
    assert!(h.app.after_update().is_none());
    assert_eq!(
        download_dir(),
        std::env::temp_dir().join("PlanoraCAD-Update")
    );
}

#[test]
fn window_for_start_only_for_the_target_version() {
    let running = running_version();
    let window = WindowState {
        x: Some(200.0),
        y: Some(100.0),
        width: 1500.0,
        height: 850.0,
        maximized: true,
    };
    let config = |to: String| Config {
        update_pending: Some(PendingUpdate {
            from: "0.0.1".into(),
            to,
            window: Some(window),
        }),
        ..Config::default()
    };
    assert_eq!(
        window_for_start(&config(running.to_string()), &running),
        Some(window)
    );
    assert_eq!(window_for_start(&config(TARGET.into()), &running), None);
    assert_eq!(window_for_start(&config("Unsinn".into()), &running), None);
    assert_eq!(window_for_start(&Config::default(), &running), None);
}

/// Lassen sich die Einstellungen beim Start nach einem Update nicht schreiben, erscheint der
/// Hinweis trotzdem nur einmal in diesem Programmlauf; das nächste gelungene Schreiben löscht
/// das vorgemerkte Update auch in der Datei.
#[test]
fn startup_notice_once_even_if_the_settings_cannot_be_written() {
    let (mut ws, cfg, paths) = workspace("planora-install-start-schreibfehler");
    let running = running_version();
    assert!(ws.set_pending_update(Some((v("0.0.1"), running))));
    // Ab jetzt scheitert jedes Schreiben.
    unwritable_config(&mut ws, &cfg);
    let mut h = Harness::with(
        ws,
        cfg.clone(),
        paths,
        FakeDownloader::good(),
        FakeLauncher::ok(),
    );
    h.app.run_update_startup();
    h.settle();
    assert!(h.sees(&updated_title(&running)), "{:?}", h.texts());
    h.click(OK);
    // Noch auf der Platte, aber nicht mehr in diesem Lauf.
    assert_eq!(
        h.pending_on_disk(),
        Some(("0.0.1".to_string(), running.to_string()))
    );
    h.app.run_update_startup();
    h.settle();
    assert!(h.app.after_update().is_none());
    assert!(!h.sees(&updated_title(&running)));
    // Die Datei ist wieder erreichbar: das nächste Schreiben nimmt die Löschung mit.
    h.app.ws_mut().config_path = cfg;
    assert!(h.app.ws_mut().set_last_update_check(1));
    assert_eq!(h.pending_on_disk(), None);
}

/// Neue ungespeicherte Arbeit, während der Installer vor dem Start noch einmal geprüft wird:
/// noch ist nichts gestartet, also zurück zur Rückfrage wie beim Schließen.
#[test]
fn changes_while_checking_the_new_file_go_back_to_the_prompt() {
    let launcher = FakeLauncher::ok();
    let (mut ws, cfg, paths) = workspace("planora-install-aenderung-beim-pruefen");
    dirty(&mut ws);
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), launcher.clone());
    h.until_offered();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    // „Verwerfen“ ohne Klick; gleich danach entsteht neue Arbeit (im Programm verhindert das die
    // Sperre). Die zweite Prüfung läuft trotzdem an; ist sie fertig, geht es zurück zur
    // Rückfrage statt zum Start.
    h.app.ws_mut().discard_and_continue();
    assert!(h.app.ws_mut().quit);
    dirty(h.app.ws_mut());
    h.frame(vec![]);
    assert!(!h.close_sent());
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    h.settle();
    assert_eq!(h.app.restart_phase(), RestartPhase::AwaitingPrompt);
    assert!(matches!(
        h.app.ws_mut().prompt,
        Some(Prompt::SaveChanges { .. })
    ));
    assert!(launcher.calls().is_empty());
    assert!(h.paths.setup.exists(), "nicht verworfen");
    assert!(!h.close_sent());
    // Beantwortet: weiter wie immer.
    h.click("Verwerfen");
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(launcher.calls().len(), 1);
}

/// Solange der Installer geprüft und gestartet wird, ist die Bedienung gesperrt: ein Schließen
/// des Fensters wird zurückgehalten (das Programm beendet sich gleich selbst), es fragt nichts
/// und setzt nichts in Gang.
#[test]
fn closing_the_window_while_locked_is_held_back() {
    let launcher = FakeLauncher::ok();
    let mut h = Harness::offered(
        "planora-install-schliessen-gesperrt",
        FakeDownloader::good(),
        launcher.clone(),
    );
    let gate = h.hold_reverify();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::Verifying);
    h.commands.clear();
    h.frame_close();
    assert!(
        h.commands
            .iter()
            .any(|c| matches!(c, ViewportCommand::CancelClose)),
        "{:?}",
        h.commands
    );
    assert!(!h.close_sent());
    assert!(!h.app.ws_mut().quit);
    assert!(h.app.ws_mut().prompt.is_none());
    h.settle();
    assert!(h.sees(RESTARTING), "{:?}", h.texts());
    assert!(launcher.calls().is_empty());
    // Die Prüfung ist fertig: Installer gestartet, jetzt beendet sich das Programm.
    drop(gate);
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    h.settle();
    assert_eq!(launcher.calls().len(), 1);
    assert!(h.close_sent());
}

#[test]
fn startup_with_stale_pending_clears_it_and_does_not_retry() {
    let downloader = FakeDownloader::good();
    let launcher = FakeLauncher::ok();
    let (mut ws, cfg, paths) = workspace("planora-install-alt-eintrag");
    let running = running_version();
    // Das Update auf TARGET ist nicht durchgelaufen: es läuft noch die bisherige Version.
    assert!(ws.set_pending_update(Some((running, v(TARGET)))));
    write_notes(&cfg.with_file_name(NOTES_FILE_NAME), &v(TARGET), "- Neu");
    let mut h = Harness::with(ws, cfg, paths, downloader.clone(), launcher.clone());
    h.app.run_update_startup();
    assert_eq!(h.app.ws_mut().pending_update(), None);
    assert_eq!(h.pending_on_disk(), None);
    assert!(h.app.after_update().is_none());
    assert!(!h.notes_file().exists());
    h.until(|a| a.pending_notice().is_some());
    h.settle();
    // Kein neuer Versuch von allein: nichts geladen, nichts gestartet; die Version wird
    // weiter angeboten.
    assert_eq!(downloader.calls(), 0);
    assert_eq!(h.state(), InstallState::Idle);
    assert!(launcher.calls().is_empty());
    assert!(!h.sees(&updated_title(&running)));
    assert_eq!(
        h.app.pending_notice().map(|u| u.latest.version),
        Some(v(TARGET))
    );
}

#[test]
fn non_windows_shows_hint_and_opens_release_page() {
    // Die Entscheidung ohne `cfg`.
    assert_eq!(decide(false, None), InstallDecision::OnlyWindows);
    assert_eq!(
        decide(false, Some(InstallMode::AllUsers)),
        InstallDecision::OnlyWindows
    );

    let downloader = FakeDownloader::good();
    let launcher = FakeLauncher::ok();
    let mut h = Harness::offered(
        "planora-install-nur-windows",
        downloader.clone(),
        launcher.clone(),
    );
    h.app.set_auto_install_supported(false);
    h.click(INSTALL_NOW);
    assert!(h.sees(ONLY_WINDOWS), "{:?}", h.texts());
    let page = release_page(TAG);
    assert_eq!(
        page,
        format!("https://github.com/{REPO}/releases/tag/v99.0.0-dev.1")
    );
    assert_eq!(h.urls, vec![page.clone()]);
    assert_eq!(downloader.calls(), 0);
    assert!(launcher.calls().is_empty());
    // Der Knopf in der Fehlermeldung öffnet die Seite noch einmal.
    h.click(OPEN_DOWNLOAD_PAGE);
    assert_eq!(h.urls, vec![page.clone(), page]);
}

#[test]
fn cancel_during_download_returns_to_idle() {
    let downloader = FakeDownloader::good();
    let (gate_tx, gate) = mpsc::channel();
    *downloader.gate.lock().unwrap() = Some(gate);
    let launcher = FakeLauncher::ok();
    let mut h = Harness::offered(
        "planora-install-abbruch",
        downloader.clone(),
        launcher.clone(),
    );
    h.click(INSTALL_NOW);
    // Ein erstes Stück, dann steht der Download.
    gate_tx.send(()).unwrap();
    h.until(
        |a| matches!(a.installer().state(), InstallState::Downloading { done, .. } if *done > 0),
    );
    h.settle();
    assert!(h.sees(CANCEL), "{:?}", h.texts());
    assert!(h.paths.setup.exists());
    h.click(CANCEL);
    assert_eq!(h.state(), InstallState::Idle);
    assert!(!h.app.update_dialog_mut().open);
    assert!(!h.app.update_dialog_mut().installing);
    // Der Thread hört beim nächsten Rückruf auf und löscht seine Datei.
    let start = Instant::now();
    while h.app.installer().thread_running() {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(gate_tx);
    assert!(!h.paths.setup.exists());
    assert!(launcher.calls().is_empty());
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
    assert!(!h.close_sent());
}

#[test]
fn ui_stays_responsive_state_machine_progress() {
    let downloader = FakeDownloader::good();
    let (gate_tx, gate) = mpsc::channel();
    *downloader.gate.lock().unwrap() = Some(gate);
    let launcher = FakeLauncher::ok();
    let mut h = Harness::offered(
        "planora-install-fortschritt",
        downloader.clone(),
        launcher.clone(),
    );
    assert_eq!(h.state(), InstallState::Idle);
    h.click(INSTALL_NOW);
    let total = NEW_SETUP.len() as u64;
    assert_eq!(
        h.state(),
        InstallState::Downloading { done: 0, total },
        "Download läuft, ohne dass ein Bild wartet"
    );
    // Solange der Download steht, bleiben die Bilder schnell.
    for _ in 0..10 {
        let took = h.frame(vec![]);
        assert!(took < Duration::from_millis(500), "{took:?}");
    }
    assert_eq!(h.state(), InstallState::Downloading { done: 0, total });
    // Stück für Stück: der Stand steigt.
    let mut last = 0;
    for _ in 0..2 {
        gate_tx.send(()).unwrap();
        h.until(|a| matches!(a.installer().state(), InstallState::Downloading { done, .. } if *done > last));
        let InstallState::Downloading { done, .. } = h.state() else {
            unreachable!()
        };
        last = done;
    }
    h.settle();
    assert!(h.sees(&download_text(last, total)), "{:?}", h.texts());
    assert!(h.sees_part("Wird heruntergeladen …"));
    // Ausblenden mit Esc: der Download läuft weiter, die Statuszeile zeigt den Stand.
    let esc = |pressed| Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    h.frame(vec![esc(true)]);
    h.frame(vec![esc(false)]);
    h.settle();
    assert!(!h.app.update_dialog_mut().open);
    assert!(matches!(h.state(), InstallState::Downloading { .. }));
    assert!(
        h.sees(&download_text(last, total)),
        "Statuszeile: {:?}",
        h.texts()
    );
    // Der Rest kommt: geprüft, dann der Start des Installers.
    drop(gate_tx_rest(gate_tx));
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(launcher.calls().len(), 1);
    assert!(download_text(5, 10).ends_with("50 %"));
    assert_eq!(download_text(0, 0), "Wird heruntergeladen … 0,0 MB");
    assert_eq!(fraction(5, 10), 0.5);
    assert_eq!(fraction(5, 0), 0.0);
}

/// Gibt alle übrigen Stücke frei.
fn gate_tx_rest(tx: mpsc::Sender<()>) -> mpsc::Sender<()> {
    for _ in 0..NEW_SETUP.len() {
        let _ = tx.send(());
    }
    tx
}

/// Lädt das Update hinter einem Tor, setzt dann `block`, gibt den Download frei und prüft, dass
/// der Neustart wartet (sichtbar), bis `unblock` gilt; danach geht er von allein weiter.
fn waits_until_free(
    prefix: &str,
    ws: Option<(Workspace, PathBuf, DownloadPaths)>,
    block: impl FnOnce(&mut PlanoraApp),
    unblock: impl FnOnce(&mut PlanoraApp),
) -> (Harness, Arc<FakeLauncher>) {
    let downloader = FakeDownloader::good();
    let (gate_tx, gate) = mpsc::channel();
    *downloader.gate.lock().unwrap() = Some(gate);
    let launcher = FakeLauncher::ok();
    let (ws, cfg, paths) = ws.unwrap_or_else(|| workspace(prefix));
    let mut h = Harness::with(ws, cfg, paths, downloader, launcher.clone());
    h.until_offered();
    h.click(INSTALL_NOW);
    block(&mut h.app);
    drop(gate_tx_rest(gate_tx));
    h.until(|a| *a.installer().state() == InstallState::ReadyToRestart);
    for _ in 0..10 {
        h.frame(vec![]);
    }
    assert_eq!(h.app.restart_phase(), RestartPhase::None, "wartet");
    assert_eq!(*h.app.installer().state(), InstallState::ReadyToRestart);
    assert!(h.paths.setup.exists());
    assert!(launcher.calls().is_empty());
    assert!(!h.app.ws_mut().quit);
    assert!(
        h.sees_part("Update ist bereit"),
        "sichtbar: {:?}",
        h.texts()
    );
    assert!(!h.close_sent());
    unblock(&mut h.app);
    (h, launcher)
}

#[test]
fn restart_waits_while_a_prompt_is_open() {
    let (mut h, launcher) = waits_until_free(
        "planora-install-warten-rueckfrage",
        None,
        |a| a.ws_mut().prompt = Some(Prompt::Notice("Hinweis".into())),
        |a| a.ws_mut().prompt = None,
    );
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(launcher.calls().len(), 1);
    assert_eq!(WAITING, "Update ist bereit und wartet, bis du fertig bist.");
}

#[test]
fn restart_waits_while_a_window_is_open() {
    let (mut h, launcher) = waits_until_free(
        "planora-install-warten-fenster",
        None,
        |a| a.ws_mut().about_open = true,
        |a| a.ws_mut().about_open = false,
    );
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(launcher.calls().len(), 1);
}

fn slow_writer(
    _: &Path,
    _: &planora_drawing::sheet::SheetModel,
    _: planora_drawing::DrawingFormat,
    p: &planora_exchange::Progress,
) -> Result<u64, planora_drawing::DrawError> {
    loop {
        p.check()?;
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn restart_waits_while_an_export_is_writing() {
    use planora_app::export_dialog::{ExportDialog, ExportFormat};
    use planora_drawing::{DrawingFormat, Paper, Placement, Projection, Scale, Sheet, ViewKind};
    let (mut ws, cfg, paths) = workspace("planora-install-warten-export");
    let out_dir = cfg.parent().unwrap().to_path_buf();
    // Der Export schreibt in einem Tab, der nicht vorne ist.
    let tab = ws.active_tab_mut();
    tab.state.create_box(DVec3::new(30.0, 20.0, 10.0)).unwrap();
    let sheet = Sheet {
        paper: Paper::A4,
        landscape: true,
    };
    let d = tab
        .state
        .document
        .add_drawing("Blatt 1", sheet, Projection::First)
        .unwrap();
    tab.state
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 148.5, y: 105.0 },
            None,
        )
        .unwrap();
    let mut e = ExportDialog::new(&tab.state);
    e.format = ExportFormat::Drawing(DrawingFormat::Svg);
    e.drawing = Some(d);
    e.tick(&tab.state, true);
    e.set_writer(slow_writer);
    e.write_to(out_dir.join("z.svg"), &tab.state);
    assert!(e.is_writing_file());
    tab.export_dialog = Some(e);
    ws.new_tab();
    let (mut h, launcher) = waits_until_free(
        "planora-install-warten-export",
        Some((ws, cfg, paths)),
        |_| {},
        |a| a.ws_mut().tabs[0].export_dialog = None,
    );
    // Danach die Rückfrage für den geänderten ersten Tab, dann der Start.
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    h.click("Verwerfen");
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(launcher.calls().len(), 1);
}

fn autosave(ws: &Workspace, tab: usize) -> planora_project::docstore::Autosave {
    planora_project::docstore::Autosave {
        based_on: None,
        time: "2026-10-25 12:00".into(),
        snapshot: ws.tabs[tab].state.snapshot(),
    }
}

#[test]
fn restart_waits_while_an_autosave_offer_is_pending() {
    let (mut h, launcher) = waits_until_free(
        "planora-install-warten-autosicherung",
        None,
        |a| {
            let offer = autosave(a.ws_mut(), 0);
            a.ws_mut().tabs[0].pending_autosave = Some(offer);
        },
        |a| {
            a.ws_mut().tabs[0].pending_autosave = None;
            a.ws_mut().prompt = None;
        },
    );
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(launcher.calls().len(), 1);
}

/// Kommt während der Rückfrage ein Autosicherungs-Angebot dazwischen, ist das kein „Abbrechen“:
/// das Update wartet und fragt danach neu.
#[test]
fn autosave_offer_during_the_prompt_is_not_a_cancel() {
    let launcher = FakeLauncher::ok();
    let (mut ws, cfg, paths) = workspace("planora-install-autosicherung-dazwischen");
    dirty(&mut ws);
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), launcher.clone());
    h.until_offered();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    // Das Angebot ersetzt die Rückfrage.
    let offer = autosave(h.app.ws_mut(), 0);
    h.app.ws_mut().tabs[0].pending_autosave = Some(offer);
    h.app.ws_mut().prompt = None;
    h.settle();
    assert_eq!(h.app.restart_phase(), RestartPhase::None, "wartet");
    assert_eq!(*h.app.installer().state(), InstallState::ReadyToRestart);
    assert!(h.paths.setup.exists(), "nicht verworfen");
    assert!(matches!(
        h.app.ws_mut().prompt,
        Some(Prompt::AutosaveOffer { .. })
    ));
    // Beantwortet: die Rückfrage kommt wieder, dann der Start.
    h.app.ws_mut().tabs[0].pending_autosave = None;
    h.app.ws_mut().prompt = None;
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    h.click("Verwerfen");
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(launcher.calls().len(), 1);
}

#[test]
fn disk_full_during_download_shows_message_and_leaves_no_file() {
    let error = InstallError::Io("Auf dem Datenträger ist nicht genügend Speicherplatz".into());
    let launcher = FakeLauncher::ok();
    let mut h = Harness::offered(
        "planora-install-platte-voll",
        FakeDownloader::failing(error.clone()),
        launcher.clone(),
    );
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert_eq!(h.state(), InstallState::Failed(error.message()));
    h.settle();
    assert!(h.sees(&error.message()), "{:?}", h.texts());
    assert!(!h.paths.setup.exists(), "Teil-Download gelöscht");
    assert!(!h.paths.checksum.exists());
    assert!(launcher.calls().is_empty());
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
}

#[test]
fn unsaved_tabs_discarded_then_restart_is_requested() {
    let launcher = FakeLauncher::ok();
    let (mut ws, cfg, paths) = workspace("planora-install-verwerfen-alle");
    dirty(&mut ws);
    ws.new_tab();
    dirty(&mut ws);
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), launcher.clone());
    h.until_offered();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    h.click("Verwerfen");
    assert_eq!(h.app.restart_phase(), RestartPhase::AwaitingPrompt);
    h.click("Verwerfen");
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    h.settle();
    assert_eq!(
        launcher.calls(),
        vec![(h.paths.setup.clone(), InstallMode::User)]
    );
    assert!(h.close_sent());
}

/// Ein Fehler steht deutlich da: eigener Titel, die Meldung über den Patchnotes (die weiter zu
/// lesen sind) und „Erneut versuchen“, das den Download noch einmal startet.
#[test]
fn failure_has_its_own_title_message_on_top_and_retry() {
    let downloader = FakeDownloader::new(format!("{}  {SETUP_NAME}", hex_digest(b"falsch")));
    let mut h = Harness::offered(
        "planora-install-fehler-deutlich",
        downloader.clone(),
        FakeLauncher::ok(),
    );
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    h.settle();
    let message = InstallError::Checksum.message();
    assert!(h.sees(FAILED_TITLE), "{:?}", h.texts());
    assert_eq!(FAILED_TITLE, "Update hat nicht geklappt");
    assert!(
        !h.sees(&planora_app::update_dialog::title(&v(TARGET))),
        "{:?}",
        h.texts()
    );
    let top = |h: &Harness, t: &str| {
        h.seen
            .iter()
            .find(|s| s.text == t)
            .unwrap_or_else(|| panic!("{t}: {:?}", h.texts()))
            .rect
            .top()
    };
    assert!(top(&h, &message) < top(&h, "Schneller beim Öffnen"));
    assert!(h.sees(RETRY) && h.sees(OPEN_DOWNLOAD_PAGE) && h.sees(CLOSE));
    assert_eq!(downloader.calls(), 2);
    h.click(RETRY);
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert_eq!(downloader.calls(), 4, "noch einmal geladen");
    assert!(h.app.update_dialog_mut().installing);

    // Nicht unter Windows: erneut versuchen hilft nicht, also kein Knopf dafür.
    let mut h = Harness::offered(
        "planora-install-fehler-kein-erneut",
        FakeDownloader::good(),
        FakeLauncher::ok(),
    );
    h.app.set_auto_install_supported(false);
    h.click(INSTALL_NOW);
    h.settle();
    assert!(h.sees(FAILED_TITLE), "{:?}", h.texts());
    assert!(h.sees(ONLY_WINDOWS));
    assert!(!h.sees(RETRY), "{:?}", h.texts());
}

/// „Neustart …“ steht während der Prüfung vor dem Start nur einmal da (in der Statuszeile); der
/// Kasten über allem erklärt es in einem Satz.
#[test]
fn restart_text_appears_once_while_waiting() {
    let mut h = Harness::offered(
        "planora-install-neustart-einmal",
        FakeDownloader::good(),
        FakeLauncher::ok(),
    );
    let gate = h.hold_reverify();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::Verifying);
    h.settle();
    assert_eq!(h.app.notice_view(), InstallView::Restarting);
    let restarting = h.seen.iter().filter(|s| s.text == RESTARTING).count();
    assert_eq!(restarting, 1, "{:?}", h.texts());
    assert!(h.sees(RESTART_BOX), "{:?}", h.texts());
    drop(gate);
}

/// Die Meldung steht fest unten rechts über der Verlaufsleiste; ein Fenster wie „Quader
/// erzeugen“ öffnet in der Mitte und verdeckt sie nicht.
#[test]
fn notice_sits_bottom_right_and_dialogs_do_not_cover_it() {
    let mut h = Harness::offered(
        "planora-install-meldung-unten-rechts",
        FakeDownloader::good(),
        FakeLauncher::ok(),
    );
    let title = planora_app::update_dialog::title(&v(TARGET));
    let find = |h: &Harness, t: &str| {
        h.seen
            .iter()
            .find(|s| s.text == t)
            .unwrap_or_else(|| panic!("{t}: {:?}", h.texts()))
            .rect
    };
    let notice = find(&h, &title);
    let skip = find(&h, SKIP);
    // Rechts von jedem mittigen Fenster bis zur Breite „Wide“ (720) und über der Verlaufsleiste.
    let wide_right = SIZE.x / 2.0 + planora_app::dialog::DialogSize::Wide.width() / 2.0;
    assert!(notice.left() > wide_right, "{notice:?}");
    assert!(skip.right() < SIZE.x - planora_app::shell::SHELL_INSET + 1.0);
    let history_top = SIZE.y
        - planora_app::shell::SHELL_INSET
        - planora_app::shell::BOTTOM_H
        - planora_app::shell::GAP
        - planora_app::shell::HISTORY_H;
    assert!(skip.bottom() < history_top, "{skip:?}");
    // „Quader erzeugen“ öffnen: die Meldung bleibt, wo sie ist, und ist nicht verdeckt.
    h.app.ws_mut().active_tab_mut().dialog = Some(planora_app::create_dialog::CreateDialog::Box {
        x: 40.0,
        y: 30.0,
        z: 20.0,
    });
    h.settle();
    assert!(h.app.update_dialog_mut().open);
    let again = find(&h, &title);
    assert_eq!(again, notice);
    let quader = find(&h, "Quader erzeugen");
    assert!(quader.right() < notice.left(), "{quader:?} {notice:?}");
}

/// Größe, Lage und Zustand (maximiert) des Fensters gehen mit dem vorgemerkten Update in die
/// Konfiguration; die neue Version öffnet ihr Fenster daraus.
#[test]
fn window_state_goes_with_the_pending_update() {
    let mut h = Harness::offered(
        "planora-install-fenster",
        FakeDownloader::good(),
        FakeLauncher::ok(),
    );
    h.viewport = Some(egui::ViewportInfo {
        inner_rect: Some(Rect::from_min_size(Pos2::new(0.0, 23.0), SIZE)),
        outer_rect: Some(Rect::from_min_size(
            Pos2::new(-8.0, -8.0),
            vec2(1936.0, 1111.0),
        )),
        maximized: Some(true),
        ..Default::default()
    });
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    let config = Config::load_from(&h.cfg);
    let window = config
        .update_pending
        .as_ref()
        .and_then(|p| p.window)
        .expect("Fenster gemerkt");
    assert_eq!(
        (window.x, window.y, window.width, window.height),
        (Some(-8.0), Some(-8.0), SIZE.x, SIZE.y)
    );
    assert!(window.maximized);
    // Die neue Version (Zielversion) bekommt dieses Fenster, die alte nicht.
    assert_eq!(window_for_start(&config, &v(TARGET)), Some(window));
    assert_eq!(window_for_start(&config, &running_version()), None);
    // Pure Prüfung: kein Fenster ohne Angaben, zu kleine Fenster auf die Mindestgröße.
    assert_eq!(window_state(&egui::ViewportInfo::default()), None);
    let small = WindowState {
        x: Some(5.0),
        y: Some(6.0),
        width: 300.0,
        height: 200.0,
        maximized: false,
    };
    let b = restore_window(egui::ViewportBuilder::default(), &small);
    assert_eq!(b.inner_size, Some(vec2(800.0, 600.0)));
    assert_eq!(b.position, Some(Pos2::new(5.0, 6.0)));
    assert_eq!(b.maximized, Some(false));
}

/// Ein unplausibles Fenster öffnet nie unsichtbar: zu große Maße begrenzt, eine Lage weit
/// draußen weggelassen (das System setzt das Fenster dann selbst).
#[test]
fn restore_window_limits_size_and_drops_far_positions() {
    let wild = WindowState {
        x: Some(-32000.0),
        y: Some(-32000.0),
        width: 1e30,
        height: 1e30,
        maximized: false,
    };
    let b = restore_window(egui::ViewportBuilder::default(), &wild);
    let max = planora_project::config::MAX_WINDOW_SIZE;
    assert_eq!(b.inner_size, Some(vec2(max, max)));
    assert_eq!(b.position, None);
    // Unbrauchbare Maße: nichts übernommen, die Vorgabe des Programms bleibt.
    let broken = WindowState {
        width: f32::NAN,
        ..wild
    };
    let b = restore_window(egui::ViewportBuilder::default(), &broken);
    assert_eq!((b.inner_size, b.position, b.maximized), (None, None, None));
}

/// Ein minimiertes Fenster (Windows meldet dann die Lage -32000/-32000) wird nicht gemerkt;
/// mit dem Update geht die letzte Lage, in der das Fenster zu sehen war.
#[test]
fn minimized_window_keeps_the_last_visible_state() {
    let minimized = egui::ViewportInfo {
        inner_rect: Some(Rect::from_min_size(
            Pos2::new(-32000.0, -32000.0),
            vec2(160.0, 28.0),
        )),
        outer_rect: Some(Rect::from_min_size(
            Pos2::new(-32000.0, -32000.0),
            vec2(160.0, 28.0),
        )),
        minimized: Some(true),
        ..Default::default()
    };
    assert_eq!(window_state(&minimized), None);
    let mut h = Harness::offered(
        "planora-install-fenster-minimiert",
        FakeDownloader::good(),
        FakeLauncher::ok(),
    );
    h.viewport = Some(visible_window());
    h.settle();
    h.viewport = Some(minimized);
    // Die Meldung bleibt bedienbar; der Klick kommt beim „minimierten“ Fenster an.
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    let window = Config::load_from(&h.cfg)
        .update_pending
        .and_then(|p| p.window)
        .expect("letzte sichtbare Lage gemerkt");
    assert_eq!(
        (window.x, window.y, window.width, window.height),
        (Some(200.0), Some(100.0), 1500.0, 850.0)
    );
}

/// Nach dem Start des Installers endet die alte Version sauber (wie beim Schließen): `on_exit`
/// löscht ihren Laufmarker, also meldet der nächste Start (die neue Version, vom Installer
/// gestartet) kein „unerwartet beendet“.
#[test]
fn update_exit_leaves_no_run_marker() {
    let mut h = Harness::offered(
        "planora-install-laufmarker",
        FakeDownloader::good(),
        FakeLauncher::ok(),
    );
    let data = h.cfg.parent().unwrap().to_path_buf();
    let (marker, finding) = planora_app::crash::start_run(&data, 4_000_000_011);
    assert_eq!(finding, planora_app::crash::StartFinding::Clean);
    h.app.set_run_marker(marker);
    assert!(planora_app::crash::marker_path(&data, 4_000_000_011).exists());
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    h.settle();
    assert!(h.close_sent());
    // eframe ruft beim Schließen `on_exit`.
    h.app.on_exit();
    assert!(!planora_app::crash::marker_path(&data, 4_000_000_011).exists());
    let (own, finding) = planora_app::crash::start_run(&data, 4_000_000_012);
    assert_eq!(finding, planora_app::crash::StartFinding::Clean);
    own.unwrap().finish();
}
