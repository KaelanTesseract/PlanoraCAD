//! Update einspielen in der ganzen App: Download und Prüfung im Hintergrund, Rückfrage vor dem
//! Neustart (wie beim Schließen), Austausch, Start der neuen Version und Warten auf ihre
//! Rückmeldung, Rücksetzen, Start nach einem Update.
//!
//! Mit echten Bildern der App (`logic` und `ui` wie in eframe) bei 1920 × 1080, Nachbildungen
//! für Release-Liste, Download und Neustart; die „Programmdatei“ liegt in einem Temp-Ordner, nie
//! wird die echte exe angefasst, nie geht etwas ins Netz.

mod common;

use eframe::App as _;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, ViewportCommand, ViewportId, vec2};
use planora_app::app::{PlanoraApp, RestartPhase};
use planora_app::prompts::{SAVE_FOR_UPDATE_TITLE, save_for_update_text};
use planora_app::update_dialog::{
    CANCEL, CLOSE, FAILED_TITLE, INSTALL_NOW, InstallView, NOTES_ON_GITHUB, OK, RETRY,
};
use planora_app::update_install::*;
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{fonts, running_version, theme};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_update::check::{NetError, ReleaseSource};
use planora_update::install::{
    Downloader, InstallError, OK_MARKER_NAME, Paths, STALL_TIMEOUT, copy_limited,
};
use planora_update::sha256::hex_digest;
use planora_update::{EXE_NAME, REPO, SHA256_NAME, Version};
use serde_json::{Value, json};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NEW_EXE: &[u8] = b"neue Programmdatei, Version 99";
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
        "assets": [asset(EXE_NAME, NEW_EXE.len()), asset(SHA256_NAME, 100)],
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

/// Nachbildung des Downloads: liefert die Prüfsummen-Datei `checksum` und die Programmdatei
/// [`NEW_EXE`] über die echte Kopierschleife; zählt die Aufrufe.
struct FakeDownloader {
    checksum: String,
    calls: AtomicUsize,
    /// Wird vor dem Laden der Programmdatei abgeholt: steuert, wann Stücke kommen.
    gate: Mutex<Option<mpsc::Receiver<()>>>,
    step: usize,
    /// Fehler beim Laden der Programmdatei, nachdem schon ein Teil geschrieben ist (etwa
    /// „Speicherplatz voll“).
    exe_error: Option<InstallError>,
}

impl FakeDownloader {
    fn new(checksum: String) -> Arc<FakeDownloader> {
        Arc::new(FakeDownloader {
            checksum,
            calls: AtomicUsize::new(0),
            gate: Mutex::new(None),
            step: 7,
            exe_error: None,
        })
    }

    fn failing(error: InstallError) -> Arc<FakeDownloader> {
        Arc::new(FakeDownloader {
            checksum: format!("{}  {EXE_NAME}\n", hex_digest(NEW_EXE)),
            calls: AtomicUsize::new(0),
            gate: Mutex::new(None),
            step: 7,
            exe_error: Some(error),
        })
    }

    fn good() -> Arc<FakeDownloader> {
        Self::new(format!("{}  {EXE_NAME}\n", hex_digest(NEW_EXE)))
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
            assert!(url.ends_with(EXE_NAME));
            if let Some(e) = &self.exe_error {
                // Ein Teil liegt schon auf der Platte, dann scheitert das Schreiben.
                std::fs::write(dest, &NEW_EXE[..10]).unwrap();
                return Err(e.clone());
            }
            (NEW_EXE.to_vec(), self.gate.lock().unwrap().take())
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

/// Nachbildung des Neustarts: merkt sich die gestartete Datei; legt auf Wunsch die Rückmeldung
/// ab wie eine neue Version, die gut startet.
struct FakeRestarter {
    started: Mutex<Vec<PathBuf>>,
    /// Inhalt der Rückmeldung, die die „neue Version“ schreibt (`None`: keine).
    answer: Option<String>,
    fails: bool,
    /// Die gestartete „neue Version“ endet gleich von selbst (ohne Rückmeldung).
    exits: bool,
    /// Je beendeter „neuer Version“ der Inhalt der Programmdatei im Augenblick des Beendens
    /// (so lässt sich prüfen, ob vor oder nach dem Zurücklegen beendet wurde).
    stopped: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl FakeRestarter {
    /// `answers`: schreibt die Rückmeldung mit der Zielversion.
    fn new(answers: bool) -> Arc<FakeRestarter> {
        Self::answering(answers.then(|| TARGET.to_string()))
    }

    fn answering(answer: Option<String>) -> Arc<FakeRestarter> {
        Arc::new(FakeRestarter {
            started: Mutex::new(Vec::new()),
            answer,
            fails: false,
            exits: false,
            stopped: Arc::default(),
        })
    }

    fn started(&self) -> Vec<PathBuf> {
        self.started.lock().unwrap().clone()
    }

    fn stopped(&self) -> Vec<Vec<u8>> {
        self.stopped.lock().unwrap().clone()
    }
}

/// Die „neue Version“ der Nachbildung: merkt sich, wann sie beendet wird.
struct FakeProcess {
    exe: PathBuf,
    stopped: Arc<Mutex<Vec<Vec<u8>>>>,
    exits: bool,
}

impl NewProcess for FakeProcess {
    fn stop(&mut self) {
        let now = std::fs::read(&self.exe).unwrap_or_default();
        self.stopped.lock().unwrap().push(now);
    }

    fn exited(&mut self) -> bool {
        self.exits
    }
}

impl Restarter for FakeRestarter {
    fn start_new(&self, exe: &Path) -> Result<Box<dyn NewProcess>, String> {
        self.started.lock().unwrap().push(exe.to_path_buf());
        if self.fails {
            return Err("geht nicht".into());
        }
        if let Some(answer) = &self.answer {
            std::fs::write(exe.parent().unwrap().join(OK_MARKER_NAME), answer).unwrap();
        }
        Ok(Box::new(FakeProcess {
            exe: exe.to_path_buf(),
            stopped: Arc::clone(&self.stopped),
            exits: self.exits,
        }))
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
    paths: Paths,
    time: f64,
    seen: Vec<Seen>,
    commands: Vec<ViewportCommand>,
    urls: Vec<String>,
    /// Lage und Zustand des Programmfensters, wie eframe sie meldet (sonst keine Angaben).
    viewport: Option<egui::ViewportInfo>,
}

/// Arbeitsbereich mit Bibliothek und Konfiguration im Temp-Ordner und „Programmdatei“ in einem
/// eigenen Ordner darin (Inhalt `alt`).
fn workspace(prefix: &str) -> (Workspace, PathBuf, Paths) {
    let tmp = common::temp_root(prefix);
    std::fs::create_dir_all(tmp.join("Programm")).unwrap();
    let mut ws = Workspace::new(
        Some(Library::open(&tmp.join("Projekte")).unwrap()),
        Config::default(),
    );
    let cfg = tmp.join("config.json");
    ws.config_path = cfg.clone();
    assert_ne!(cfg, Config::config_path(), "nie die echte Datei");
    let exe = tmp.join("Programm").join("planoracad.exe");
    std::fs::write(&exe, "alt").unwrap();
    (ws, cfg, Paths::for_exe(&exe))
}

impl Harness {
    fn new(
        prefix: &str,
        downloader: Arc<dyn Downloader + Send + Sync>,
        restarter: Arc<dyn Restarter + Send + Sync>,
    ) -> Harness {
        let (ws, cfg, paths) = workspace(prefix);
        Self::with(ws, cfg, paths, downloader, restarter)
    }

    fn with(
        ws: Workspace,
        cfg: PathBuf,
        paths: Paths,
        downloader: Arc<dyn Downloader + Send + Sync>,
        restarter: Arc<dyn Restarter + Send + Sync>,
    ) -> Harness {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut app = PlanoraApp::for_tests_with_install(
            ws,
            Arc::new(Source),
            downloader,
            restarter,
            paths.running_exe.clone(),
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
        restarter: Arc<dyn Restarter + Send + Sync>,
    ) -> Harness {
        let mut h = Self::new(prefix, downloader, restarter);
        h.until(|a| a.pending_notice().is_some());
        h.settle();
        assert!(h.app.update_dialog_mut().open);
        h
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

    fn read_exe(&self) -> Vec<u8> {
        std::fs::read(&self.paths.running_exe).unwrap()
    }
}

fn dirty(ws: &mut Workspace) {
    ws.active_tab_mut()
        .state
        .create_box(DVec3::splat(4.0))
        .unwrap();
}

#[test]
fn wrong_checksum_never_swaps_and_shows_message() {
    let restarter = FakeRestarter::new(true);
    for checksum in [
        format!("{}  {EXE_NAME}", hex_digest(b"etwas anderes")),
        format!("{}  andere.exe", hex_digest(NEW_EXE)),
        "Not Found".to_string(),
    ] {
        let downloader = FakeDownloader::new(checksum);
        let mut h = Harness::offered(
            "planora-install-summe",
            downloader.clone(),
            restarter.clone(),
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
        // Nichts ausgetauscht, nichts liegen geblieben, nichts gestartet, nichts vorgemerkt.
        assert_eq!(h.read_exe(), b"alt");
        assert!(!h.paths.new_exe.exists());
        assert!(!h.paths.checksum_file().exists());
        assert!(!h.paths.old_exe.exists());
        assert!(restarter.started().is_empty());
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
fn prompt_for_unsaved_work_before_restart_and_cancel_aborts_the_update_and_deletes_new_file() {
    let restarter = FakeRestarter::new(true);
    let (mut ws, cfg, paths) = workspace("planora-install-rueckfrage");
    // Zwei Tabs mit ungespeicherten Änderungen.
    dirty(&mut ws);
    ws.new_tab();
    dirty(&mut ws);
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), restarter.clone());
    h.until(|a| a.pending_notice().is_some());
    h.settle();
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
    assert!(h.paths.new_exe.exists(), "fertig geladen und geprüft");
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
    assert!(!h.paths.new_exe.exists(), ".new gelöscht");
    assert_eq!(h.read_exe(), b"alt");
    assert!(!h.paths.old_exe.exists());
    assert!(restarter.started().is_empty());
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
fn successful_flow_sets_pending_swaps_and_requests_restart() {
    let restarter = FakeRestarter::new(true);
    let mut h = Harness::offered(
        "planora-install-gut",
        FakeDownloader::good(),
        restarter.clone(),
    );
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    h.settle();
    // Vorgemerkt (auch auf der Platte), ausgetauscht, neu gestartet mit der neuen Datei.
    let running = running_version();
    assert_eq!(h.app.ws_mut().pending_update(), Some((running, v(TARGET))));
    assert_eq!(
        h.pending_on_disk(),
        Some((running.to_string(), TARGET.to_string()))
    );
    assert_eq!(h.read_exe(), NEW_EXE);
    assert_eq!(std::fs::read(&h.paths.old_exe).unwrap(), b"alt");
    assert!(!h.paths.new_exe.exists());
    assert_eq!(restarter.started(), vec![h.paths.running_exe.clone()]);
    // Die neue Version läuft weiter.
    assert!(restarter.stopped().is_empty());
    // Die Rückmeldung ist abgeholt; das Programm beendet sich ohne weitere Rückfrage.
    assert!(!h.paths.ok_marker.exists());
    assert!(h.app.ws_mut().quit);
    assert!(h.app.ws_mut().prompt.is_none());
    assert!(h.close_sent());
    // Die Patchnotes liegen für den Hinweis der neuen Version bereit.
    let notes = std::fs::read_to_string(h.cfg.with_file_name(NOTES_FILE_NAME)).unwrap();
    assert_eq!(notes, format!("{TARGET}\n- Schneller beim Öffnen"));
}

#[test]
fn pending_that_cannot_be_saved_aborts_before_swapping() {
    let restarter = FakeRestarter::new(true);
    let (mut ws, _cfg, paths) = workspace("planora-install-vormerken");
    // Die Konfiguration liegt „in“ einer Datei: Schreiben scheitert immer.
    let blocker = paths.dir().join("keine-mappe");
    std::fs::write(&blocker, "x").unwrap();
    let cfg = blocker.join("config.json");
    ws.config_path = cfg.clone();
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), restarter.clone());
    h.until(|a| a.pending_notice().is_some());
    h.settle();
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert_eq!(h.state(), InstallState::Failed(PENDING_NOT_SAVED.into()));
    assert_eq!(h.read_exe(), b"alt");
    assert!(!h.paths.new_exe.exists());
    assert!(!h.paths.old_exe.exists());
    assert!(restarter.started().is_empty());
    assert!(!h.app.ws_mut().quit);
    h.settle();
    assert!(!h.close_sent());
}

#[test]
fn failing_start_of_the_new_version_rolls_back() {
    let restarter = Arc::new(FakeRestarter {
        started: Mutex::new(Vec::new()),
        answer: None,
        fails: true,
        exits: false,
        stopped: Arc::default(),
    });
    let mut h = Harness::offered(
        "planora-install-startfehler",
        FakeDownloader::good(),
        restarter.clone(),
    );
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert_eq!(h.read_exe(), b"alt");
    assert!(!h.paths.old_exe.exists());
    assert_eq!(h.app.ws_mut().pending_update(), None);
    assert_eq!(restarter.started().len(), 1);
    assert!(!h.app.ws_mut().quit);
    h.settle();
    assert!(!h.close_sent());
}

#[test]
fn missing_ok_marker_triggers_rollback_and_keeps_offering_the_version() {
    let restarter = FakeRestarter::new(false);
    let mut h = Harness::offered(
        "planora-install-keine-antwort",
        FakeDownloader::good(),
        restarter.clone(),
    );
    h.app.set_marker_wait(Duration::from_millis(300));
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.restart_phase(), RestartPhase::WaitingForMarker { .. }));
    // Während des Wartens ist die neue Datei schon an ihrem Platz.
    assert_eq!(h.read_exe(), NEW_EXE);
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    h.settle();
    assert_eq!(
        h.state(),
        InstallState::Failed(not_started(&v(TARGET), &running_version()))
    );
    // Die zu spät oder gar nicht antwortende neue Version wird beendet, und zwar bevor der
    // alte Stand zurückkommt (sonst liefe sie aus der umbenannten Datei parallel weiter).
    assert_eq!(restarter.stopped(), vec![NEW_EXE.to_vec()]);
    // Alter Stand zurück, Eintrag gelöscht, Programm läuft weiter. Die Version wird nicht
    // übersprungen, sondern weiter angeboten.
    assert_eq!(h.read_exe(), b"alt");
    assert!(!h.paths.old_exe.exists());
    assert_eq!(h.app.ws_mut().skipped_version(), None);
    assert_eq!(Config::load_from(&h.cfg).skipped_version, None);
    assert_eq!(h.app.ws_mut().pending_update(), None);
    assert_eq!(h.pending_on_disk(), None);
    assert_eq!(
        h.app.pending_notice().map(|u| u.latest.version),
        Some(v(TARGET))
    );
    assert!(!h.app.ws_mut().quit);
    assert!(!h.close_sent());
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
    assert!(
        h.sees(&not_started(&v(TARGET), &running_version())),
        "{:?}",
        h.texts()
    );
    assert_eq!(
        not_started(&v("0.2.0-dev.2"), &v("0.2.0-dev.1")),
        "Das Update auf Version 0.2.0-dev.2 hat nicht geklappt, du bleibst auf Version 0.2.0-dev.1. Du kannst es später erneut versuchen."
    );
    assert_eq!(MARKER_WAIT, Duration::from_secs(30));
    // „Schließen“: die Meldung ruht, die Statuszeile bietet die Version weiter an.
    h.click(CLOSE);
    assert!(h.sees(&planora_app::update_dialog::status_hint(&v(TARGET))));
}

/// Eine Rückmeldung, die schon vor dem Austausch daliegt (etwa von einem früheren Update), zählt
/// nicht: sie wird vor dem Austausch gelöscht, die neue Version meldet sich nicht → zurück.
#[test]
fn stale_ok_marker_does_not_count() {
    let mut h = Harness::offered(
        "planora-install-alte-rueckmeldung",
        FakeDownloader::good(),
        FakeRestarter::new(false),
    );
    h.app.set_marker_wait(Duration::from_millis(300));
    std::fs::write(&h.paths.ok_marker, TARGET).unwrap();
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.restart_phase(), RestartPhase::WaitingForMarker { .. }));
    assert!(!h.paths.ok_marker.exists(), "vor dem Austausch gelöscht");
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert_eq!(h.read_exe(), b"alt");
    assert_eq!(h.app.ws_mut().skipped_version(), None);
    assert!(!h.app.ws_mut().quit);
    h.settle();
    assert!(!h.close_sent());
}

/// Eine Rückmeldung mit anderer Version (nicht die Zielversion) zählt nicht.
#[test]
fn ok_marker_with_wrong_version_does_not_count() {
    let restarter = FakeRestarter::answering(Some("0.0.1".into()));
    let mut h = Harness::offered(
        "planora-install-falsche-rueckmeldung",
        FakeDownloader::good(),
        restarter.clone(),
    );
    h.app.set_marker_wait(Duration::from_millis(300));
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert_eq!(restarter.started().len(), 1);
    assert_eq!(restarter.stopped().len(), 1, "falsche Antwort: beendet");
    assert_eq!(
        h.state(),
        InstallState::Failed(not_started(&v(TARGET), &running_version()))
    );
    assert_eq!(h.read_exe(), b"alt");
    assert!(!h.app.ws_mut().quit);
    h.settle();
    assert!(!h.close_sent());
    // Pure Prüfung: nur der genaue Inhalt zählt (Leerraum am Rand egal).
    std::fs::write(&h.paths.ok_marker, format!(" {TARGET}\n")).unwrap();
    assert!(ok_marker_for(&h.paths, &v(TARGET)));
    std::fs::write(&h.paths.ok_marker, "99.0.0-dev.2").unwrap();
    assert!(!ok_marker_for(&h.paths, &v(TARGET)));
}

#[test]
fn startup_with_matching_pending_shows_notice_once_and_cleans_up() {
    let (mut ws, cfg, paths) = workspace("planora-install-start");
    let running = running_version();
    assert!(ws.set_pending_update(Some((v("0.0.1"), running))));
    std::fs::write(&paths.old_exe, "alt").unwrap();
    write_notes(
        &cfg.with_file_name(NOTES_FILE_NAME),
        &running,
        "# Neu\n\n- Schneller beim Öffnen",
    );
    let mut h = Harness::with(
        ws,
        cfg.clone(),
        paths.clone(),
        FakeDownloader::good(),
        FakeRestarter::new(true),
    );
    h.app.run_update_startup(false);
    // Eintrag gelöscht, `.old` weg, Ablage gelesen. Ohne `--nach-update` wartet keine alte
    // Version: keine Rückmeldung.
    assert!(!paths.ok_marker.exists());
    assert_eq!(h.app.ws_mut().pending_update(), None);
    assert_eq!(h.pending_on_disk(), None);
    assert!(!paths.old_exe.exists());
    assert!(!cfg.with_file_name(NOTES_FILE_NAME).exists());
    h.settle();
    assert!(h.sees(&updated_title(&running)), "{:?}", h.texts());
    assert!(h.sees("Schneller beim Öffnen"));
    h.click(OK);
    assert!(h.app.after_update().is_none());
    assert!(!h.sees(&updated_title(&running)));
    // Einmal: beim nächsten Start kein Hinweis mehr.
    h.app.run_update_startup(false);
    h.settle();
    assert!(h.app.after_update().is_none());
    assert!(!h.sees(&updated_title(&running)));

    // Mit `--nach-update` ohne Eintrag und ohne Ablage: nur die Zeile.
    let (ws, cfg, paths) = workspace("planora-install-start-arg");
    std::fs::write(&paths.old_exe, "alt").unwrap();
    let mut h = Harness::with(
        ws,
        cfg,
        paths.clone(),
        FakeDownloader::good(),
        FakeRestarter::new(true),
    );
    h.app.run_update_startup(true);
    // Die Rückmeldung kommt erst, wenn das erste Bild fertig gezeichnet ist: vorher könnte die
    // neue Version noch beim Laden scheitern, und die alte muss dann zurücklegen können.
    assert!(!paths.ok_marker.exists(), "noch kein Bild");
    h.frame(vec![]);
    assert!(!paths.ok_marker.exists(), "das erste Bild läuft noch");
    h.frame(vec![]);
    // `.old` bleibt bei diesem Start liegen (die alte Version braucht sie zum Zurücklegen); erst
    // der nächste normale Start räumt sie weg.
    assert!(
        paths.old_exe.exists(),
        ".old bleibt beim Start nach dem Update"
    );
    // Rückmeldung mit der laufenden Version als Inhalt.
    assert_eq!(
        std::fs::read_to_string(&paths.ok_marker).unwrap(),
        running.to_string()
    );
    assert!(ok_marker_for(&paths, &running));
    h.settle();
    assert!(h.sees(&updated_title(&running)));
    assert!(h.sees(NOTES_ON_GITHUB));
    assert_eq!(AFTER_UPDATE_ARG, "--nach-update");
    // Nur einmal: weitere Bilder schreiben die Rückmeldung nicht neu.
    std::fs::remove_file(&paths.ok_marker).unwrap();
    h.settle();
    assert!(!paths.ok_marker.exists());
    // Der nächste normale Start räumt `.old` weg.
    h.app.run_update_startup(false);
    assert!(!paths.old_exe.exists());
}

/// Lassen sich die Einstellungen beim Start nach einem Update nicht schreiben, erscheint der
/// Hinweis trotzdem nur einmal in diesem Programmlauf; das nächste gelungene Schreiben löscht
/// das vorgemerkte Update auch in der Datei.
#[test]
fn startup_notice_once_even_if_the_settings_cannot_be_written() {
    let (mut ws, cfg, paths) = workspace("planora-install-start-schreibfehler");
    let running = running_version();
    assert!(ws.set_pending_update(Some((v("0.0.1"), running))));
    // Ab jetzt scheitert jedes Schreiben (die Konfiguration läge „in“ einer Datei).
    let blocker = paths.dir().join("keine-mappe");
    std::fs::write(&blocker, "x").unwrap();
    ws.config_path = blocker.join("config.json");
    let mut h = Harness::with(
        ws,
        cfg.clone(),
        paths,
        FakeDownloader::good(),
        FakeRestarter::new(true),
    );
    h.app.run_update_startup(false);
    h.settle();
    assert!(h.sees(&updated_title(&running)), "{:?}", h.texts());
    h.click(OK);
    // Noch auf der Platte, aber nicht mehr in diesem Lauf.
    assert_eq!(
        h.pending_on_disk(),
        Some(("0.0.1".to_string(), running.to_string()))
    );
    h.app.run_update_startup(false);
    h.settle();
    assert!(h.app.after_update().is_none());
    assert!(!h.sees(&updated_title(&running)));
    // Die Datei ist wieder erreichbar: das nächste Schreiben nimmt die Löschung mit.
    h.app.ws_mut().config_path = cfg;
    assert!(h.app.ws_mut().set_last_update_check(1));
    assert_eq!(h.pending_on_disk(), None);
}

/// Neue ungespeicherte Arbeit, während die alte Version auf die Rückmeldung der neuen wartet
/// (die Bedienung ist dann gesperrt, aber etwa ein Autosicherungs-Angebot oder ein Weg an der
/// Sperre vorbei bleibt denkbar): Das Programm schließt sich nicht ohne Rückfrage. Es beendet
/// die neue Version, legt den alten Stand zurück und läuft mit der Arbeit weiter.
#[test]
fn changes_while_waiting_for_the_new_version_never_close_without_asking() {
    let restarter = FakeRestarter::new(false);
    let mut h = Harness::offered(
        "planora-install-aenderung-beim-warten",
        FakeDownloader::good(),
        restarter.clone(),
    );
    h.app.set_marker_wait(Duration::from_secs(60));
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.restart_phase(), RestartPhase::WaitingForMarker { .. }));
    dirty(h.app.ws_mut());
    // Die neue Version meldet sich.
    std::fs::write(&h.paths.ok_marker, TARGET).unwrap();
    h.settle();
    h.settle();
    assert!(!h.close_sent(), "nie ohne Rückfrage schließen");
    assert!(!h.app.ws_mut().quit);
    assert!(h.app.ws_mut().active_tab().is_dirty(), "Arbeit noch da");
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
    // Keine zwei Programme: die neue Version ist beendet (vor dem Zurücklegen).
    assert_eq!(restarter.stopped(), vec![NEW_EXE.to_vec()]);
    assert_eq!(h.read_exe(), b"alt");
    assert!(!h.paths.old_exe.exists());
    assert_eq!(h.app.ws_mut().pending_update(), None);
    assert_eq!(h.pending_on_disk(), None);
    // Die Version wird nicht übersprungen; eine Meldung erklärt es.
    assert_eq!(h.app.ws_mut().skipped_version(), None);
    assert_eq!(h.state(), InstallState::Failed(RESTART_STOPPED.into()));
    assert!(h.sees(RESTART_STOPPED), "{:?}", h.texts());
    // Danach lässt sich das Fenster wie immer schließen: erst mit Rückfrage.
    h.frame_close();
    assert!(!h.app.ws_mut().quit);
    assert!(matches!(
        h.app.ws_mut().prompt,
        Some(Prompt::SaveChanges { .. })
    ));
}

/// Neue ungespeicherte Arbeit, während `.new` vor dem Austausch noch einmal geprüft wird: noch
/// ist nichts ausgetauscht, also zurück zur Rückfrage wie beim Schließen.
#[test]
fn changes_while_checking_the_new_file_go_back_to_the_prompt() {
    let restarter = FakeRestarter::new(true);
    let (mut ws, cfg, paths) = workspace("planora-install-aenderung-beim-pruefen");
    dirty(&mut ws);
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), restarter.clone());
    h.until(|a| a.pending_notice().is_some());
    h.settle();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    // „Verwerfen“ ohne Klick; gleich danach entsteht neue Arbeit (im Programm verhindert das die
    // Sperre). Die zweite Prüfung läuft trotzdem an; ist sie fertig, geht es zurück zur
    // Rückfrage statt zum Austausch.
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
    assert!(restarter.started().is_empty());
    assert_eq!(h.read_exe(), b"alt");
    assert!(h.paths.new_exe.exists(), "nicht verworfen");
    assert!(!h.close_sent());
    // Beantwortet: weiter wie immer.
    h.click("Verwerfen");
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(h.read_exe(), NEW_EXE);
}

/// Solange die neue Version geprüft und gestartet wird, ist die Bedienung gesperrt: ein
/// Schließen des Fensters wird zurückgehalten (das Programm beendet sich gleich selbst), es
/// fragt nichts und setzt nichts in Gang.
#[test]
fn closing_the_window_while_waiting_is_held_back() {
    let mut h = Harness::offered(
        "planora-install-schliessen-beim-warten",
        FakeDownloader::good(),
        FakeRestarter::new(false),
    );
    h.app.set_marker_wait(Duration::from_secs(60));
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.restart_phase(), RestartPhase::WaitingForMarker { .. }));
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
    assert!(h.sees(RESTARTING), "{:?}", h.texts());
    // Die neue Version meldet sich: jetzt beendet sich das Programm.
    std::fs::write(&h.paths.ok_marker, TARGET).unwrap();
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    h.settle();
    assert!(h.close_sent());
}

#[test]
fn startup_with_stale_pending_clears_it_and_does_not_retry() {
    let downloader = FakeDownloader::good();
    let restarter = FakeRestarter::new(true);
    let (mut ws, cfg, paths) = workspace("planora-install-alt-eintrag");
    let running = running_version();
    assert!(ws.set_pending_update(Some((running, v(TARGET)))));
    std::fs::write(&paths.old_exe, "uralt").unwrap();
    let mut h = Harness::with(
        ws,
        cfg,
        paths.clone(),
        downloader.clone(),
        restarter.clone(),
    );
    h.app.run_update_startup(false);
    assert_eq!(h.app.ws_mut().pending_update(), None);
    assert_eq!(h.pending_on_disk(), None);
    assert!(!paths.ok_marker.exists(), "keine Rückmeldung");
    assert!(h.app.after_update().is_none());
    // `.old` wird trotzdem aufgeräumt.
    assert!(!paths.old_exe.exists());
    h.until(|a| a.pending_notice().is_some());
    h.settle();
    // Kein neuer Versuch von allein: nichts geladen, nichts getauscht, nichts gestartet.
    assert_eq!(downloader.calls(), 0);
    assert_eq!(h.state(), InstallState::Idle);
    assert_eq!(h.read_exe(), b"alt");
    assert!(restarter.started().is_empty());
    assert!(!h.sees(&updated_title(&running)));
}

#[test]
fn non_windows_shows_hint_and_opens_release_page() {
    // Die Entscheidung ohne `cfg`.
    let dir = common::temp_root("planora-install-entscheidung");
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("planoracad.exe");
    assert_eq!(decide(false, Some(&exe)), InstallDecision::OnlyWindows);
    assert_eq!(decide(true, None), InstallDecision::NoExe);
    assert_eq!(decide(true, Some(&exe)), InstallDecision::Download);
    assert_eq!(
        decide(true, Some(&dir.join("fehlt").join("planoracad.exe"))),
        InstallDecision::ProtectedFolder
    );

    let downloader = FakeDownloader::good();
    let mut h = Harness::offered(
        "planora-install-nur-windows",
        downloader.clone(),
        FakeRestarter::new(true),
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
    assert_eq!(h.read_exe(), b"alt");

    // Geschützter Ordner: Meldung mit Knopf zur Download-Seite.
    let (ws, cfg, _) = workspace("planora-install-geschuetzt");
    let missing = Paths::for_exe(&cfg.with_file_name("fehlt").join("planoracad.exe"));
    let mut h = Harness::with(
        ws,
        cfg,
        missing,
        downloader.clone(),
        FakeRestarter::new(true),
    );
    h.until(|a| a.pending_notice().is_some());
    h.settle();
    h.click(INSTALL_NOW);
    assert!(
        h.sees(&InstallError::NotWritable.message()),
        "{:?}",
        h.texts()
    );
    assert_eq!(
        InstallError::NotWritable.message(),
        "Das Programm liegt in einem geschützten Ordner. Lade die neue Version von GitHub herunter."
    );
    assert!(h.urls.is_empty());
    h.click(OPEN_DOWNLOAD_PAGE);
    assert_eq!(h.urls, vec![page]);
    assert_eq!(downloader.calls(), 0);
}

#[test]
fn cancel_during_download_returns_to_idle() {
    let downloader = FakeDownloader::good();
    let (gate_tx, gate) = mpsc::channel();
    *downloader.gate.lock().unwrap() = Some(gate);
    let mut h = Harness::offered(
        "planora-install-abbruch",
        downloader.clone(),
        FakeRestarter::new(true),
    );
    h.click(INSTALL_NOW);
    // Ein erstes Stück, dann steht der Download.
    gate_tx.send(()).unwrap();
    h.until(
        |a| matches!(a.installer().state(), InstallState::Downloading { done, .. } if *done > 0),
    );
    h.settle();
    assert!(h.sees(CANCEL), "{:?}", h.texts());
    assert!(h.paths.new_exe.exists());
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
    assert!(!h.paths.new_exe.exists());
    assert_eq!(h.read_exe(), b"alt");
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
    assert!(!h.close_sent());
}

#[test]
fn ui_stays_responsive_state_machine_progress() {
    let downloader = FakeDownloader::good();
    let (gate_tx, gate) = mpsc::channel();
    *downloader.gate.lock().unwrap() = Some(gate);
    let restarter = FakeRestarter::new(true);
    let mut h = Harness::offered(
        "planora-install-fortschritt",
        downloader.clone(),
        restarter.clone(),
    );
    assert_eq!(h.state(), InstallState::Idle);
    h.click(INSTALL_NOW);
    let total = NEW_EXE.len() as u64;
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
    // Der Rest kommt: geprüft, dann Neustart.
    drop(gate_tx_rest(gate_tx));
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(h.read_exe(), NEW_EXE);
    assert_eq!(restarter.started().len(), 1);
    assert!(download_text(5, 10).ends_with("50 %"));
    assert_eq!(download_text(0, 0), "Wird heruntergeladen … 0,0 MB");
    assert_eq!(fraction(5, 10), 0.5);
    assert_eq!(fraction(5, 0), 0.0);
}

/// Gibt alle übrigen Stücke frei.
fn gate_tx_rest(tx: mpsc::Sender<()>) -> mpsc::Sender<()> {
    for _ in 0..NEW_EXE.len() {
        let _ = tx.send(());
    }
    tx
}

// ---- Nachbesserung: zweite Prüfung, Warten auf freie Bahn, Speicherplatz, Tabs ------------

#[test]
fn new_file_changed_before_swap_is_not_swapped() {
    let restarter = FakeRestarter::new(true);
    let (mut ws, cfg, paths) = workspace("planora-install-zweite-pruefung");
    dirty(&mut ws);
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), restarter.clone());
    h.until(|a| a.pending_notice().is_some());
    h.settle();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    // Während die Rückfrage steht, verändert ein anderes Programm `.new`.
    std::fs::write(&h.paths.new_exe, b"halb geschrieben").unwrap();
    h.click("Verwerfen");
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert_eq!(
        h.state(),
        InstallState::Failed(InstallError::Checksum.message())
    );
    assert_eq!(h.read_exe(), b"alt", "nicht ausgetauscht");
    assert!(!h.paths.new_exe.exists(), ".new gelöscht");
    assert!(!h.paths.old_exe.exists());
    assert!(restarter.started().is_empty());
    assert_eq!(h.app.ws_mut().pending_update(), None);
    assert!(!h.app.ws_mut().quit);
    h.settle();
    assert!(!h.close_sent());
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
}

/// Lädt das Update hinter einem Tor, setzt dann `block`, gibt den Download frei und prüft, dass
/// der Neustart wartet (sichtbar), bis `unblock` gilt; danach geht er von allein weiter.
fn waits_until_free(
    prefix: &str,
    ws: Option<(Workspace, PathBuf, Paths)>,
    block: impl FnOnce(&mut PlanoraApp),
    unblock: impl FnOnce(&mut PlanoraApp),
) -> Harness {
    let downloader = FakeDownloader::good();
    let (gate_tx, gate) = mpsc::channel();
    *downloader.gate.lock().unwrap() = Some(gate);
    let restarter = FakeRestarter::new(true);
    let (ws, cfg, paths) = ws.unwrap_or_else(|| workspace(prefix));
    let mut h = Harness::with(ws, cfg, paths, downloader, restarter.clone());
    h.until(|a| a.pending_notice().is_some());
    h.settle();
    h.click(INSTALL_NOW);
    block(&mut h.app);
    drop(gate_tx_rest(gate_tx));
    h.until(|a| *a.installer().state() == InstallState::ReadyToRestart);
    for _ in 0..10 {
        h.frame(vec![]);
    }
    assert_eq!(h.app.restart_phase(), RestartPhase::None, "wartet");
    assert_eq!(*h.app.installer().state(), InstallState::ReadyToRestart);
    assert!(h.paths.new_exe.exists());
    assert_eq!(h.read_exe(), b"alt");
    assert!(restarter.started().is_empty());
    assert!(!h.app.ws_mut().quit);
    assert!(
        h.sees_part("Update ist bereit"),
        "sichtbar: {:?}",
        h.texts()
    );
    assert!(!h.close_sent());
    unblock(&mut h.app);
    h
}

#[test]
fn restart_waits_while_a_prompt_is_open() {
    let mut h = waits_until_free(
        "planora-install-warten-rueckfrage",
        None,
        |a| a.ws_mut().prompt = Some(Prompt::Notice("Hinweis".into())),
        |a| a.ws_mut().prompt = None,
    );
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(h.read_exe(), NEW_EXE);
    assert_eq!(WAITING, "Update ist bereit und wartet, bis du fertig bist.");
}

#[test]
fn restart_waits_while_a_window_is_open() {
    let mut h = waits_until_free(
        "planora-install-warten-fenster",
        None,
        |a| a.ws_mut().about_open = true,
        |a| a.ws_mut().about_open = false,
    );
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(h.read_exe(), NEW_EXE);
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
    e.write_to(paths.dir().join("z.svg"), &tab.state);
    assert!(e.is_writing_file());
    tab.export_dialog = Some(e);
    ws.new_tab();
    let mut h = waits_until_free(
        "planora-install-warten-export",
        Some((ws, cfg, paths)),
        |_| {},
        |a| a.ws_mut().tabs[0].export_dialog = None,
    );
    // Danach die Rückfrage für den geänderten ersten Tab, dann der Neustart.
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    h.click("Verwerfen");
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(h.read_exe(), NEW_EXE);
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
    let mut h = waits_until_free(
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
    assert_eq!(h.read_exe(), NEW_EXE);
}

/// Kommt während der Rückfrage ein Autosicherungs-Angebot dazwischen, ist das kein „Abbrechen“:
/// das Update wartet und fragt danach neu.
#[test]
fn autosave_offer_during_the_prompt_is_not_a_cancel() {
    let restarter = FakeRestarter::new(true);
    let (mut ws, cfg, paths) = workspace("planora-install-autosicherung-dazwischen");
    dirty(&mut ws);
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), restarter.clone());
    h.until(|a| a.pending_notice().is_some());
    h.settle();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    // Das Angebot ersetzt die Rückfrage.
    let offer = autosave(h.app.ws_mut(), 0);
    h.app.ws_mut().tabs[0].pending_autosave = Some(offer);
    h.app.ws_mut().prompt = None;
    h.settle();
    assert_eq!(h.app.restart_phase(), RestartPhase::None, "wartet");
    assert_eq!(*h.app.installer().state(), InstallState::ReadyToRestart);
    assert!(h.paths.new_exe.exists(), "nicht verworfen");
    assert!(matches!(
        h.app.ws_mut().prompt,
        Some(Prompt::AutosaveOffer { .. })
    ));
    // Beantwortet: die Rückfrage kommt wieder, dann der Neustart.
    h.app.ws_mut().tabs[0].pending_autosave = None;
    h.app.ws_mut().prompt = None;
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    h.click("Verwerfen");
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    assert_eq!(h.read_exe(), NEW_EXE);
    assert_eq!(restarter.started().len(), 1);
}

#[test]
fn disk_full_during_download_shows_message_and_leaves_no_file() {
    let error = InstallError::Io("Auf dem Datenträger ist nicht genügend Speicherplatz".into());
    let mut h = Harness::offered(
        "planora-install-platte-voll",
        FakeDownloader::failing(error.clone()),
        FakeRestarter::new(true),
    );
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert_eq!(h.state(), InstallState::Failed(error.message()));
    h.settle();
    assert!(h.sees(&error.message()), "{:?}", h.texts());
    assert!(!h.paths.new_exe.exists(), "Teil-Download gelöscht");
    assert!(!h.paths.checksum_file().exists());
    assert_eq!(h.read_exe(), b"alt");
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
}

#[test]
fn unsaved_tabs_discarded_then_restart_is_requested() {
    let restarter = FakeRestarter::new(true);
    let (mut ws, cfg, paths) = workspace("planora-install-verwerfen-alle");
    dirty(&mut ws);
    ws.new_tab();
    dirty(&mut ws);
    let mut h = Harness::with(ws, cfg, paths, FakeDownloader::good(), restarter.clone());
    h.until(|a| a.pending_notice().is_some());
    h.settle();
    h.click(INSTALL_NOW);
    h.until(|a| a.restart_phase() == RestartPhase::AwaitingPrompt);
    h.click("Verwerfen");
    assert_eq!(h.app.restart_phase(), RestartPhase::AwaitingPrompt);
    h.click("Verwerfen");
    h.until(|a| a.restart_phase() == RestartPhase::Exiting);
    h.settle();
    assert_eq!(h.read_exe(), NEW_EXE);
    assert_eq!(restarter.started(), vec![h.paths.running_exe.clone()]);
    assert!(h.close_sent());
}

/// Endet die gestartete neue Version von selbst, bevor sie sich meldet, wartet die alte nicht
/// die ganze Frist: sie legt sofort zurück und zeigt die Meldung.
#[test]
fn new_version_that_ends_at_once_rolls_back_without_waiting() {
    let restarter = Arc::new(FakeRestarter {
        started: Mutex::new(Vec::new()),
        answer: None,
        fails: false,
        exits: true,
        stopped: Arc::default(),
    });
    let mut h = Harness::offered(
        "planora-install-endet-sofort",
        FakeDownloader::good(),
        restarter.clone(),
    );
    // Die Frist ist lang; `until` gibt nach 10 s auf.
    h.app.set_marker_wait(Duration::from_secs(600));
    h.click(INSTALL_NOW);
    let start = Instant::now();
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(
        h.state(),
        InstallState::Failed(not_started(&v(TARGET), &running_version()))
    );
    assert_eq!(restarter.started().len(), 1);
    assert_eq!(h.read_exe(), b"alt");
    assert!(!h.paths.old_exe.exists());
    assert_eq!(h.app.restart_phase(), RestartPhase::None);
    assert_eq!(h.app.ws_mut().skipped_version(), None);
    assert!(!h.app.ws_mut().quit);
    h.settle();
    assert!(!h.close_sent());
}

/// Ein Fehler steht deutlich da: eigener Titel, die Meldung über den Patchnotes (die weiter zu
/// lesen sind) und „Erneut versuchen“, das den Download noch einmal startet.
#[test]
fn failure_has_its_own_title_message_on_top_and_retry() {
    let downloader = FakeDownloader::new(format!("{}  {EXE_NAME}", hex_digest(b"falsch")));
    let mut h = Harness::offered(
        "planora-install-fehler-deutlich",
        downloader.clone(),
        FakeRestarter::new(true),
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
        FakeRestarter::new(true),
    );
    h.app.set_auto_install_supported(false);
    h.click(INSTALL_NOW);
    h.settle();
    assert!(h.sees(FAILED_TITLE), "{:?}", h.texts());
    assert!(h.sees(ONLY_WINDOWS));
    assert!(!h.sees(RETRY), "{:?}", h.texts());
}

/// „Neustart …“ steht während des Wartens auf die neue Version nur einmal da (in der
/// Statuszeile); der Kasten über allem erklärt es in einem Satz.
#[test]
fn restart_text_appears_once_while_waiting() {
    let mut h = Harness::offered(
        "planora-install-neustart-einmal",
        FakeDownloader::good(),
        FakeRestarter::new(false),
    );
    h.app.set_marker_wait(Duration::from_secs(60));
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.restart_phase(), RestartPhase::WaitingForMarker { .. }));
    h.settle();
    assert_eq!(h.app.notice_view(), InstallView::Restarting);
    let restarting = h.seen.iter().filter(|s| s.text == RESTARTING).count();
    assert_eq!(restarting, 1, "{:?}", h.texts());
    assert!(h.sees(RESTART_BOX), "{:?}", h.texts());
}

/// Die Meldung steht fest unten rechts über der Verlaufsleiste; ein Fenster wie „Quader
/// erzeugen“ öffnet in der Mitte und verdeckt sie nicht.
#[test]
fn notice_sits_bottom_right_and_dialogs_do_not_cover_it() {
    let mut h = Harness::offered(
        "planora-install-meldung-unten-rechts",
        FakeDownloader::good(),
        FakeRestarter::new(true),
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
    let skip = find(&h, planora_app::update_dialog::SKIP);
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
        FakeRestarter::new(true),
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
    let window = Config::load_from(&h.cfg)
        .update_pending
        .and_then(|p| p.window)
        .expect("Fenster gemerkt");
    assert_eq!(
        (window.x, window.y, window.width, window.height),
        (Some(-8.0), Some(-8.0), SIZE.x, SIZE.y)
    );
    assert!(window.maximized);
    // Pure Prüfung: kein Fenster ohne Angaben, zu kleine Fenster auf die Mindestgröße.
    assert_eq!(window_state(&egui::ViewportInfo::default()), None);
    let small = planora_project::config::WindowState {
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
    let wild = planora_project::config::WindowState {
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
    let broken = planora_project::config::WindowState {
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
        FakeRestarter::new(true),
    );
    h.viewport = Some(egui::ViewportInfo {
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
    });
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

/// War beim Rückfall kein Update mehr vorgemerkt, bietet die App das gewählte wieder an
/// (Statuszeile und nächste Meldung).
#[test]
fn rollback_offers_the_chosen_update_again_even_without_a_pending_notice() {
    let mut h = Harness::offered(
        "planora-install-rueckfall-wieder-anbieten",
        FakeDownloader::good(),
        FakeRestarter::new(false),
    );
    h.app.set_marker_wait(Duration::from_millis(300));
    h.click(INSTALL_NOW);
    h.until(|a| matches!(a.restart_phase(), RestartPhase::WaitingForMarker { .. }));
    *h.app.pending_notice_mut() = None;
    h.until(|a| matches!(a.installer().state(), InstallState::Failed(_)));
    assert_eq!(
        h.app.pending_notice().map(|u| u.latest.version),
        Some(v(TARGET))
    );
    h.click(CLOSE);
    assert!(h.sees(&planora_app::update_dialog::status_hint(&v(TARGET))));
}
