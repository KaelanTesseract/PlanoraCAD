//! Prüfung auf neue Versionen in der App: läuft in einem eigenen Thread, beim Start nur wenn
//! fällig und der Schalter an ist, merkt den Zeitpunkt (nicht bei „kein Netz“), zeigt nur nach
//! dem Knopf „Jetzt nach Updates suchen“ einen Hinweis und beim Start nie einen Fehler. Ohne
//! Netz: eine Nachbildung der Quelle; Konfiguration nur im Temp-Ordner.

mod common;

use eframe::App as _;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::settings_window::{
    CHECK_FAILED, CHECK_NOW, CHECK_RUNNING, CheckHint, NOT_ON_THIS_SYSTEM, SHOW_NOTICE, UNSKIP,
    UP_TO_DATE, skipped_line, update_found_line,
};
use planora_app::update_check::{CheckRequest, UpdateChecker, now_unix, records_check};
use planora_app::update_dialog::{INSTALL_NOW, LATER, SKIP};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_update::check::{CheckOutcome, NetError, ReleaseSource};
use planora_update::{Channel, REPO, SETUP_NAME, SHA256_NAME, Version};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const HOUR: u64 = 3600;

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

/// Release-Liste mit einer Version weit über der laufenden.
fn newer_list() -> String {
    let tag = "v99.0.0-dev.1";
    let asset = |name: &str| {
        json!({
            "name": name,
            "browser_download_url": format!("https://github.com/{REPO}/releases/download/{tag}/{name}"),
            "size": 10,
        })
    };
    Value::Array(vec![json!({
        "tag_name": tag,
        "body": "Neu",
        "assets": [asset(SETUP_NAME), asset(SHA256_NAME)],
    })])
    .to_string()
}

/// Nachbildung der Quelle: zählt die Abrufe, merkt den Thread und wartet auf Wunsch, bis der
/// Test sie freigibt.
struct Fake {
    answer: Result<String, NetError>,
    calls: AtomicUsize,
    thread: Mutex<Option<ThreadId>>,
    gate: Mutex<Option<mpsc::Receiver<()>>>,
}

impl Fake {
    fn new(answer: Result<String, NetError>) -> Arc<Fake> {
        Arc::new(Fake {
            answer,
            calls: AtomicUsize::new(0),
            thread: Mutex::new(None),
            gate: Mutex::new(None),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ReleaseSource for Fake {
    fn fetch_releases(&self) -> Result<String, NetError> {
        *self.thread.lock().unwrap() = Some(std::thread::current().id());
        if let Some(gate) = self.gate.lock().unwrap().take() {
            let _ = gate.recv_timeout(Duration::from_secs(10));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.answer.clone()
    }
}

/// Arbeitsbereich mit Bibliothek und Konfiguration im Temp-Ordner.
fn workspace(prefix: &str) -> (Workspace, PathBuf) {
    let tmp = common::temp_root(prefix);
    std::fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(
        Some(Library::open(&tmp.join("Projekte")).unwrap()),
        Config::default(),
    );
    let cfg = tmp.join("config.json");
    ws.config_path = cfg.clone();
    assert_ne!(cfg, Config::config_path(), "nie die echte Datei");
    (ws, cfg)
}

struct Harness {
    ctx: egui::Context,
    app: PlanoraApp,
    cfg: PathBuf,
    time: f64,
}

/// Erfasster Text eines Bildes.
struct Seen {
    text: String,
    rect: Rect,
    color: egui::Color32,
}

impl Harness {
    fn new(ws: Workspace, cfg: PathBuf, source: Arc<dyn ReleaseSource + Send + Sync>) -> Self {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        Harness {
            ctx,
            app: PlanoraApp::for_tests_with_updates(ws, source),
            cfg,
            time: 0.0,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Seen>) {
            match shape {
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                egui::epaint::Shape::Text(t) => {
                    let section = t.galley.job.sections.first();
                    let mut color = section.map_or(t.fallback_color, |s| s.format.color);
                    if color == egui::Color32::PLACEHOLDER {
                        color = t.fallback_color;
                    }
                    out.push(Seen {
                        text: t.galley.text().to_string(),
                        rect: Rect::from_min_size(t.pos, t.galley.size()),
                        color,
                    });
                }
                _ => {}
            }
        }
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    /// Bilder, bis `done` gilt (höchstens 10 s); liefert die Texte des letzten Bildes.
    fn until(&mut self, mut done: impl FnMut(&PlanoraApp) -> bool) -> Vec<Seen> {
        let start = Instant::now();
        loop {
            let seen = self.frame(vec![]);
            if done(&self.app) {
                return seen;
            }
            assert!(start.elapsed() < Duration::from_secs(10), "Zeit abgelaufen");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Bilder, bis keine Prüfung mehr läuft.
    fn until_idle(&mut self) -> Vec<Seen> {
        self.until(|a| !a.update_check_running());
        // Ergebnis anzeigen.
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn open_settings(&mut self) {
        self.app.windows_mut().2.open(false);
        for _ in 0..4 {
            self.frame(vec![]);
        }
    }

    /// Klick auf den sichtbaren Text `text`.
    fn click(&mut self, text: &str) {
        let seen = self.frame(vec![]);
        let at = seen
            .iter()
            .find(|s| s.text == text)
            .unwrap_or_else(|| panic!("„{text}“ nicht sichtbar"))
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
    }
}

fn has(seen: &[Seen], text: &str) -> bool {
    seen.iter().any(|s| s.text == text)
}

fn disk_last_check(cfg: &std::path::Path) -> Option<u64> {
    Config::read_from(cfg)
        .ok()
        .and_then(|c| c.last_update_check)
}

#[test]
fn start_check_runs_off_the_ui_thread_and_poll_returns_result() {
    let fake = Fake::new(Ok("[]".into()));
    let (open, gate) = mpsc::channel();
    *fake.gate.lock().unwrap() = Some(gate);
    let mut checker = UpdateChecker::default();
    assert!(!checker.is_running());
    assert_eq!(checker.poll(), None);
    let t0 = Instant::now();
    checker.start(
        fake.clone(),
        CheckRequest {
            running: v("0.2.0-dev.1"),
            channel: Channel::Dev,
            skipped: None,
            manual: true,
        },
    );
    // Kehrt sofort zurück, obwohl die Quelle noch wartet.
    assert!(t0.elapsed() < Duration::from_secs(1));
    assert!(checker.is_running());
    assert_eq!(checker.poll(), None, "nicht blockierend, noch nichts da");
    // Eine zweite Prüfung startet nicht, solange eine läuft.
    checker.start(
        fake.clone(),
        CheckRequest {
            running: v("0.2.0-dev.1"),
            channel: Channel::Dev,
            skipped: None,
            manual: false,
        },
    );
    open.send(()).unwrap();
    let start = Instant::now();
    let result = loop {
        if let Some(r) = checker.poll() {
            break r;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(result.outcome, CheckOutcome::UpToDate);
    assert!(result.manual);
    let now = now_unix();
    assert!(result.at <= now && result.at + 60 >= now);
    assert_ne!(
        fake.thread.lock().unwrap().unwrap(),
        std::thread::current().id(),
        "nicht im Oberflächen-Thread"
    );
    assert_eq!(fake.calls(), 1, "nur eine Prüfung");
    assert!(!checker.is_running());
    assert_eq!(checker.poll(), None);

    // Was gemerkt wird.
    assert!(records_check(&CheckOutcome::UpToDate));
    assert!(records_check(&CheckOutcome::Failed(NetError::RateLimited)));
    assert!(records_check(&CheckOutcome::Failed(NetError::Http(500))));
    assert!(!records_check(&CheckOutcome::Failed(NetError::Offline)));
    assert!(!records_check(&CheckOutcome::Failed(NetError::Timeout)));
}

#[test]
fn no_automatic_check_when_switch_is_off() {
    let (mut ws, cfg) = workspace("planora-update-schalter-aus");
    assert!(ws.set_update_check(false));
    let fake = Fake::new(Ok(newer_list()));
    let mut h = Harness::new(ws, cfg.clone(), fake.clone());
    for _ in 0..10 {
        h.frame(vec![]);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fake.calls(), 0, "nie von allein im Netz");
    assert!(!h.app.update_check_running());
    assert_eq!(h.app.pending_notice(), None);
    assert_eq!(disk_last_check(&h.cfg), None);

    // Ebenso nicht, wenn die letzte Prüfung erst 23 Stunden her ist.
    let (mut ws, cfg) = workspace("planora-update-nicht-faellig");
    assert!(ws.set_last_update_check(now_unix() - 23 * HOUR));
    let fake = Fake::new(Ok(newer_list()));
    let mut h = Harness::new(ws, cfg, fake.clone());
    for _ in 0..10 {
        h.frame(vec![]);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fake.calls(), 0);
}

#[test]
fn automatic_check_at_start_when_due_updates_last_check() {
    let (mut ws, cfg) = workspace("planora-update-beim-start");
    let old = now_unix() - 25 * HOUR;
    assert!(ws.set_last_update_check(old));
    let fake = Fake::new(Ok(newer_list()));
    let mut h = Harness::new(ws, cfg, fake.clone());
    let seen = h.until(|a| a.pending_notice().is_some());
    assert_eq!(fake.calls(), 1, "genau eine Prüfung beim Start");
    let notice = h.app.pending_notice().unwrap();
    assert_eq!(notice.latest.version, v("99.0.0-dev.1"));
    let last = h.app.ws_mut().last_update_check().unwrap();
    assert!(last > old && last + 60 >= now_unix(), "Zeitpunkt gemerkt");
    assert_eq!(disk_last_check(&h.cfg), Some(last), "und gespeichert");
    // Kein Hinweis in den Einstellungen (die Meldung selbst kommt mit der nächsten Aufgabe).
    assert_eq!(h.app.windows_mut().2.check_hint(), None);
    assert!(!has(&seen, &update_found_line(&v("99.0.0-dev.1"))));
    // Weitere Bilder fragen nicht noch einmal.
    for _ in 0..10 {
        h.frame(vec![]);
    }
    assert_eq!(fake.calls(), 1);

    // Noch nie geprüft: ebenso fällig.
    let (ws, cfg) = workspace("planora-update-nie");
    let fake = Fake::new(Ok("[]".into()));
    let mut h = Harness::new(ws, cfg, fake.clone());
    h.until(|_| fake.calls() == 1);
    h.until_idle();
    assert_eq!(h.app.pending_notice(), None);
    assert!(h.app.ws_mut().last_update_check().is_some());
}

#[test]
fn offline_does_not_update_last_check() {
    for err in [NetError::Offline, NetError::Timeout] {
        let (ws, cfg) = workspace("planora-update-offline");
        let fake = Fake::new(Err(err.clone()));
        let mut h = Harness::new(ws, cfg, fake.clone());
        h.until(|_| fake.calls() == 1);
        h.until_idle();
        assert_eq!(h.app.ws_mut().last_update_check(), None, "{err:?}");
        assert!(!h.cfg.exists(), "{err:?}: nichts geschrieben");
    }
    // Andere Fehler (Rate-Limit, Statuscode, kaputte Antwort) werden gemerkt: sonst fragte jeder
    // Start erneut bei GitHub.
    for answer in [
        Err(NetError::RateLimited),
        Err(NetError::Http(500)),
        Ok("{kaputt".to_string()),
    ] {
        let (ws, cfg) = workspace("planora-update-anderer-fehler");
        let fake = Fake::new(answer.clone());
        let mut h = Harness::new(ws, cfg, fake.clone());
        h.until(|_| fake.calls() == 1);
        h.until_idle();
        assert!(
            h.app.ws_mut().last_update_check().is_some(),
            "{answer:?} gemerkt"
        );
        assert!(disk_last_check(&h.cfg).is_some(), "{answer:?} gespeichert");
    }
}

#[test]
fn manual_check_shows_up_to_date_hint() {
    // Gerade erst geprüft und Schalter aus: der Knopf fragt trotzdem.
    let (mut ws, cfg) = workspace("planora-update-knopf-aktuell");
    assert!(ws.set_update_check(false));
    let recent = now_unix() - HOUR;
    assert!(ws.set_last_update_check(recent));
    let fake = Fake::new(Ok("[]".into()));
    let (open, gate) = mpsc::channel();
    *fake.gate.lock().unwrap() = Some(gate);
    let mut h = Harness::new(ws, cfg, fake.clone());
    h.open_settings();
    assert_eq!(fake.calls(), 0);
    h.click(CHECK_NOW);
    // Läuft: Hinweis, Knopf aus.
    let seen = h.frame(vec![]);
    assert!(h.app.update_check_running());
    assert!(has(&seen, CHECK_RUNNING), "läuft");
    assert_eq!(
        h.app.windows_mut().2.check_hint(),
        Some(&CheckHint::Running)
    );
    open.send(()).unwrap();
    let seen = h.until_idle();
    assert_eq!(fake.calls(), 1);
    assert!(has(&seen, UP_TO_DATE), "Hinweis „aktuell“");
    assert!(!has(&seen, CHECK_RUNNING));
    assert_eq!(UP_TO_DATE, "Du hast die neueste Version.");
    let last = h.app.ws_mut().last_update_check().unwrap();
    assert!(last > recent, "Zeitpunkt gemerkt");
    // Der Hinweis steht rechts neben dem Knopf, in derselben Zeile.
    let button = seen.iter().find(|s| s.text == CHECK_NOW).unwrap();
    let hint = seen.iter().find(|s| s.text == UP_TO_DATE).unwrap();
    assert!(button.rect.right() < hint.rect.left());
    assert!((button.rect.center().y - hint.rect.center().y).abs() < 4.0);
    // Neu öffnen leert das alte Ergebnis.
    h.app.windows_mut().2.close();
    h.open_settings();
    assert_eq!(h.app.windows_mut().2.check_hint(), None);

    // Neue Version gefunden: Hinweis mit Versionsnummer, Meldung vorgemerkt.
    let (ws, cfg) = workspace("planora-update-knopf-neu");
    let mut ws = ws;
    assert!(ws.set_last_update_check(now_unix()));
    let fake = Fake::new(Ok(newer_list()));
    let mut h = Harness::new(ws, cfg, fake.clone());
    h.open_settings();
    h.click(CHECK_NOW);
    let seen = h.until_idle();
    assert!(has(&seen, &update_found_line(&v("99.0.0-dev.1"))));
    assert_eq!(
        update_found_line(&v("0.2.0-dev.2")),
        "Neue Version gefunden: 0.2.0-dev.2"
    );
    assert!(h.app.pending_notice().is_some());
}

#[test]
fn manual_result_survives_closing_settings_while_running() {
    let (mut ws, cfg) = workspace("planora-update-knopf-zu-waehrend-suche");
    assert!(ws.set_last_update_check(now_unix()));
    let fake = Fake::new(Ok("[]".into()));
    let (open, gate) = mpsc::channel();
    *fake.gate.lock().unwrap() = Some(gate);
    let mut h = Harness::new(ws, cfg, fake.clone());
    h.open_settings();
    h.click(CHECK_NOW);
    assert!(h.app.update_check_running());
    // Fenster zu, während die Suche läuft; das Ergebnis kommt bei geschlossenem Fenster.
    h.app.windows_mut().2.close();
    open.send(()).unwrap();
    h.until_idle();
    assert_eq!(
        h.app.windows_mut().2.check_hint(),
        Some(&CheckHint::UpToDate)
    );
    // Wieder geöffnet: das noch nie gezeigte Ergebnis steht da.
    h.open_settings();
    let seen = h.frame(vec![]);
    assert!(has(&seen, UP_TO_DATE), "Ergebnis nicht verloren");
    // Einmal gezeigt: beim nächsten Öffnen ist es weg.
    h.app.windows_mut().2.close();
    h.open_settings();
    let seen = h.frame(vec![]);
    assert!(!has(&seen, UP_TO_DATE));
    assert_eq!(h.app.windows_mut().2.check_hint(), None);
}

#[test]
fn manual_check_failure_hint() {
    for err in [
        NetError::Offline,
        NetError::Timeout,
        NetError::RateLimited,
        NetError::Http(503),
    ] {
        let (mut ws, cfg) = workspace("planora-update-knopf-fehler");
        assert!(ws.set_last_update_check(now_unix()));
        let fake = Fake::new(Err(err.clone()));
        let mut h = Harness::new(ws, cfg, fake.clone());
        h.open_settings();
        h.click(CHECK_NOW);
        let seen = h.until_idle();
        assert_eq!(fake.calls(), 1);
        let hint = seen
            .iter()
            .find(|s| s.text == CHECK_FAILED)
            .unwrap_or_else(|| panic!("{err:?}: Hinweis fehlt"));
        assert_eq!(hint.color, theme::palette(true).warning, "Warnfarbe");
        assert_eq!(h.app.windows_mut().2.check_hint(), Some(&CheckHint::Failed));
        // Kein Fehlerfenster, keine Rückfrage.
        assert!(h.app.ws_mut().prompt.is_none());
    }
    assert_eq!(CHECK_FAILED, "Die Suche hat nicht geklappt (kein Netz?).");
}

/// Auf einem System ohne Aktualisierung über das Programm (Linux, macOS) sagt der Knopf das
/// offen, statt „kein Netz?“ zu vermuten.
#[test]
fn manual_check_on_a_system_without_updates_says_so() {
    let (mut ws, cfg) = workspace("planora-update-knopf-system");
    assert!(ws.set_last_update_check(now_unix()));
    let fake = Fake::new(Err(NetError::Unsupported));
    let mut h = Harness::new(ws, cfg, fake.clone());
    h.open_settings();
    h.click(CHECK_NOW);
    let seen = h.until_idle();
    assert!(has(&seen, NOT_ON_THIS_SYSTEM), "Hinweis fehlt");
    assert!(!has(&seen, CHECK_FAILED));
    assert_eq!(
        h.app.windows_mut().2.check_hint(),
        Some(&CheckHint::Unsupported)
    );
    assert_eq!(
        NOT_ON_THIS_SYSTEM,
        "Auf diesem System gibt es keine Aktualisierung über das Programm."
    );
}

#[test]
fn manual_click_during_start_check_counts_as_manual() {
    let (ws, cfg) = workspace("planora-update-knopf-waehrend-start");
    let fake = Fake::new(Ok("[]".into()));
    let (open, gate) = mpsc::channel();
    *fake.gate.lock().unwrap() = Some(gate);
    let mut h = Harness::new(ws, cfg, fake.clone());
    h.frame(vec![]);
    assert!(h.app.update_check_running(), "Prüfung beim Start läuft");
    h.open_settings();
    // Die Prüfung beim Start zeigt nichts an, der Knopf bleibt also bedienbar; gedrückt
    // übernimmt er deren Ergebnis, statt ein zweites Mal zu fragen.
    h.click(CHECK_NOW);
    open.send(()).unwrap();
    let seen = h.until_idle();
    assert_eq!(fake.calls(), 1, "keine zweite Abfrage");
    assert!(has(&seen, UP_TO_DATE));
}

#[test]
fn no_error_window_at_start_on_failure() {
    for answer in [
        Err(NetError::Offline),
        Err(NetError::Http(500)),
        Err(NetError::Other("kaputt".into())),
        Ok("{kaputt".to_string()),
    ] {
        let (ws, cfg) = workspace("planora-update-start-fehler");
        let fake = Fake::new(answer.clone());
        let mut h = Harness::new(ws, cfg, fake.clone());
        h.until(|_| fake.calls() == 1);
        let seen = h.until_idle();
        assert!(
            h.app.ws_mut().prompt.is_none(),
            "{answer:?}: keine Rückfrage"
        );
        assert!(!h.app.windows_mut().2.is_open(), "{answer:?}");
        assert_eq!(h.app.windows_mut().2.check_hint(), None, "{answer:?}");
        assert_eq!(h.app.pending_notice(), None);
        for t in [CHECK_FAILED, CHECK_RUNNING, UP_TO_DATE] {
            assert!(!has(&seen, t), "{answer:?}: {t}");
        }
        // Auch nach dem Öffnen der Einstellungen kein Hinweis.
        h.open_settings();
        let seen = h.frame(vec![]);
        assert!(!has(&seen, CHECK_FAILED), "{answer:?}");
    }
}

#[test]
fn test_app_never_reaches_the_network() {
    // `for_tests` hat eine Quelle ohne Netz: sie meldet „kein Netz“, gespeichert wird nichts.
    let (ws, cfg) = workspace("planora-update-for-tests");
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut h = Harness {
        ctx,
        app: PlanoraApp::for_tests(ws),
        cfg,
        time: 0.0,
    };
    h.frame(vec![]);
    h.until_idle();
    assert_eq!(h.app.ws_mut().last_update_check(), None);
    assert!(!h.cfg.exists());
}

/// Die Suche von Hand übergeht eine übersprungene Version: sie meldet sie wieder (mit der
/// Meldung samt Jetzt, Später, Überspringen), statt „Du hast die neueste Version.“ zu sagen. Die
/// Prüfung beim Start beachtet das Überspringen weiter.
#[test]
fn manual_check_ignores_the_skipped_version() {
    let (mut ws, cfg) = workspace("planora-update-knopf-uebersprungen");
    assert!(ws.set_skipped_version(Some(v("99.0.0-dev.1"))));
    let fake = Fake::new(Ok(newer_list()));
    let mut h = Harness::new(ws, cfg, fake.clone());
    // Beim Start: übersprungen, nichts vorgemerkt.
    h.until(|_| fake.calls() == 1);
    h.until_idle();
    assert_eq!(h.app.pending_notice(), None);
    // Nach „Später“ in dieser Sitzung: die Suche von Hand zeigt die Meldung trotzdem wieder.
    h.app.update_dialog_mut().snooze_until = Some(now_unix() + 3600);
    h.open_settings();
    h.click(CHECK_NOW);
    let seen = h.until_idle();
    assert_eq!(fake.calls(), 2);
    assert!(!has(&seen, UP_TO_DATE));
    assert!(has(&seen, &update_found_line(&v("99.0.0-dev.1"))));
    assert_eq!(
        h.app.pending_notice().map(|u| u.latest.version),
        Some(v("99.0.0-dev.1"))
    );
    // „Meldung anzeigen“ öffnet sie (die Einstellungen gehen dabei zu).
    h.click(SHOW_NOTICE);
    for _ in 0..4 {
        h.frame(vec![]);
    }
    assert!(h.app.update_dialog_mut().open);
    assert!(!h.app.windows_mut().2.is_open());
    let seen = h.frame(vec![]);
    for t in [INSTALL_NOW, LATER, SKIP] {
        assert!(has(&seen, t), "{t}");
    }
    // Das Überspringen selbst bleibt gespeichert (die Prüfung beim Start beachtet es weiter).
    assert_eq!(h.app.ws_mut().skipped_version(), Some(v("99.0.0-dev.1")));
}

/// Kommt der Knopf, während die Prüfung beim Start (die das Überspringen beachtet) noch läuft,
/// fragt die App danach noch einmal ohne das Überspringen.
#[test]
fn manual_check_during_start_check_asks_again_without_the_skip() {
    let (mut ws, cfg) = workspace("planora-update-knopf-start-uebersprungen");
    assert!(ws.set_skipped_version(Some(v("99.0.0-dev.1"))));
    let fake = Fake::new(Ok(newer_list()));
    let (open, gate) = mpsc::channel();
    *fake.gate.lock().unwrap() = Some(gate);
    let mut h = Harness::new(ws, cfg, fake.clone());
    h.frame(vec![]);
    assert!(h.app.update_check_running());
    h.open_settings();
    h.click(CHECK_NOW);
    open.send(()).unwrap();
    h.until(|_| fake.calls() == 2);
    let seen = h.until_idle();
    assert!(has(&seen, &update_found_line(&v("99.0.0-dev.1"))));
    assert!(h.app.pending_notice().is_some());
}

/// In den Einstellungen steht eine übersprungene Version mit dem Knopf „Doch wieder anzeigen“:
/// er hebt das Überspringen auf (gespeichert) und sucht gleich nach.
#[test]
fn settings_show_the_skipped_version_and_undo_it() {
    let (mut ws, cfg) = workspace("planora-update-uebersprungen-zurueck");
    assert!(ws.set_last_update_check(now_unix()));
    let fake = Fake::new(Ok(newer_list()));
    let mut h = Harness::new(ws, cfg.clone(), fake.clone());
    h.open_settings();
    let seen = h.frame(vec![]);
    assert!(!has(&seen, UNSKIP), "nichts übersprungen");
    assert!(h.app.ws_mut().set_skipped_version(Some(v("99.0.0-dev.1"))));
    h.frame(vec![]);
    let seen = h.frame(vec![]);
    assert!(has(&seen, &skipped_line(&v("99.0.0-dev.1"))));
    assert_eq!(
        skipped_line(&v("0.2.0-dev.2")),
        "Übersprungen: Version 0.2.0-dev.2"
    );
    assert!(has(&seen, UNSKIP));
    assert_eq!(UNSKIP, "Doch wieder anzeigen");
    h.click(UNSKIP);
    let seen = h.until_idle();
    assert_eq!(h.app.ws_mut().skipped_version(), None);
    assert_eq!(Config::read_from(&cfg).unwrap().skipped_version, None);
    assert!(!has(&seen, UNSKIP));
    assert_eq!(fake.calls(), 1, "gleich gesucht");
    assert!(has(&seen, &update_found_line(&v("99.0.0-dev.1"))));
    assert!(h.app.pending_notice().is_some());
}
