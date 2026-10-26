//! Laufmarker, Befund beim Start, Aufräumen der Berichte und die Absturz-Rückfrage.
//!
//! Marker und Berichte nur in Temp-Ordnern. Die Rückfrage mit echten Bildern der ganzen App
//! (`PlanoraApp::for_tests`, `eframe::App::ui`) und eingespeistem [`StartFinding`]; der
//! Dateimanager ist eine Nachbildung, der Browser wird nie geöffnet.

mod common;

use chrono::NaiveDate;
use eframe::App as _;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::{PlanoraApp, RestartPhase};
use planora_app::crash::{
    self, CRASH_DIR, KEEP_DAYS, KEEP_MAX, ReportInput, StartFinding, cleanup_reports, marker_path,
    start_run,
};
use planora_app::crash_dialog::{self, *};
use planora_app::feedback::{FEEDBACK_FILE, Part, SystemOpen, percent_encode};
use planora_app::feedback_dialog::{self, OPEN_BROWSER, PROJECT_CHECKBOX};
use planora_app::update_install::AfterUpdate;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_project::config::Config;
use planora_project::library::Library;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const SMALL: Vec2 = vec2(800.0, 600.0);
/// Prozessnummern, die es nie gibt (größer als jede echte).
const OWN: u32 = 4_000_000_001;
const OTHER: u32 = 4_000_000_002;

// ---- Hilfen ---------------------------------------------------------------------------------

fn data_dir(prefix: &str) -> PathBuf {
    let d = common::temp_root(prefix);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn set_age(path: &Path, age: Duration) {
    let f = File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - age).unwrap();
}

fn input(version: &str, document: Option<PathBuf>) -> ReportInput {
    ReportInput {
        version: version.into(),
        message: "Testpanik im Modell".into(),
        location: "crates/planora-app/src/app.rs:10:5".into(),
        thread: "main".into(),
        backtrace: "0: planora_app::app::Geheimnis".into(),
        steps: vec![],
        log: vec!["2026-10-26T13:03:19Z INFO Start".into()],
        document,
        time: NaiveDate::from_ymd_opt(2026, 10, 26)
            .and_then(|d| d.and_hms_opt(14, 3, 22))
            .unwrap(),
    }
}

/// Schreibt einen Bericht nach `<data>/abstuerze`.
fn report(data: &Path, version: &str, document: Option<PathBuf>) -> PathBuf {
    crash::write_report(&data.join(CRASH_DIR), &input(version, document)).unwrap()
}

/// Ein liegengebliebener Marker einer fremden, beendeten Instanz.
fn stale_marker(data: &Path, pid: u32, age: Duration) -> PathBuf {
    let p = marker_path(data, pid);
    std::fs::write(&p, b"").unwrap();
    set_age(&p, age);
    p
}

fn reports_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

struct FakeOpen(Arc<Mutex<Vec<PathBuf>>>);

impl SystemOpen for FakeOpen {
    fn open_folder(&self, dir: &Path) -> bool {
        self.0.lock().unwrap().push(dir.to_path_buf());
        true
    }
}

struct Seen {
    text: String,
    rect: Rect,
}

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

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    size: Vec2,
    time: f64,
    seen: Vec<Seen>,
    opened: Arc<Mutex<Vec<PathBuf>>>,
    data: PathBuf,
}

impl Rig {
    /// App vor dem ersten Bild.
    fn bare(prefix: &str, size: Vec2) -> Rig {
        let data = data_dir(prefix);
        let mut ws = Workspace::new(
            Some(Library::open(&data.join("Projekte")).unwrap()),
            Config::default(),
        );
        ws.config_path = data.join("config.json");
        assert_ne!(ws.config_path, Config::config_path(), "nie die echte Datei");
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let opened = Arc::new(Mutex::new(Vec::new()));
        let mut app = PlanoraApp::for_tests(ws);
        app.set_system_open(Box::new(FakeOpen(opened.clone())));
        Rig {
            ctx,
            app,
            size,
            time: 0.0,
            seen: Vec::new(),
            opened,
            data,
        }
    }

    /// App mit dem Befund aus `make` (bekommt den Datenordner) und einigen Bildern.
    fn with(prefix: &str, size: Vec2, make: impl FnOnce(&Path) -> StartFinding) -> Rig {
        let mut r = Rig::bare(prefix, size);
        let finding = make(&r.data);
        r.app.set_start_finding(finding);
        r.settle();
        r
    }

    fn crash_dir(&self) -> PathBuf {
        self.data.join(CRASH_DIR)
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let mut all = vec![Event::ModifiersChanged(Modifiers::NONE)];
        all.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events: all,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        self.seen = seen;
    }

    fn settle(&mut self) {
        for _ in 0..4 {
            self.frame(vec![]);
        }
    }

    fn texts(&self) -> Vec<&str> {
        self.seen.iter().map(|s| s.text.as_str()).collect()
    }

    fn sees(&self, text: &str) -> bool {
        self.seen.iter().any(|s| s.text == text)
    }

    fn sees_part(&self, part: &str) -> bool {
        self.seen.iter().any(|s| s.text.contains(part))
    }

    fn click(&mut self, text: &str) {
        self.settle();
        let p = self
            .seen
            .iter()
            .filter(|s| s.text == text)
            .max_by(|a, b| a.rect.top().partial_cmp(&b.rect.top()).unwrap())
            .unwrap_or_else(|| panic!("„{text}“ nicht zu sehen: {:?}", self.texts()))
            .rect
            .center();
        let button = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.settle();
    }

    fn type_text(&mut self, s: &str) {
        self.frame(vec![Event::Text(s.into())]);
        self.settle();
    }

    fn notice_shown(&self) -> bool {
        self.sees(TITLE)
    }
}

// ---- Marker und Befund ----------------------------------------------------------------------

#[test]
fn clean_exit_leaves_no_marker_and_next_start_is_clean() {
    let data = data_dir("planora-lauf-sauber");
    let (marker, finding) = start_run(&data, OWN);
    assert_eq!(finding, StartFinding::Clean);
    let marker = marker.expect("Marker angelegt");
    assert!(marker_path(&data, OWN).exists());
    marker.finish();
    assert!(!marker_path(&data, OWN).exists(), "beim Beenden gelöscht");
    let (marker, finding) = start_run(&data, OTHER);
    assert_eq!(finding, StartFinding::Clean);
    marker.unwrap().finish();
}

#[test]
fn stale_marker_without_report_is_unexpected() {
    let data = data_dir("planora-lauf-unerwartet");
    let stale = stale_marker(&data, OTHER, Duration::from_secs(60));
    let (marker, finding) = start_run(&data, OWN);
    assert_eq!(finding, StartFinding::Unexpected);
    assert!(!stale.exists(), "liegengebliebener Marker gelöscht");
    marker.unwrap().finish();
    // Beim nächsten Start ist alles sauber.
    let (marker, finding) = start_run(&data, OWN);
    assert_eq!(finding, StartFinding::Clean);
    marker.unwrap().finish();
}

#[test]
fn stale_marker_with_newer_report_is_crashed() {
    let data = data_dir("planora-lauf-absturz");
    let stale = stale_marker(&data, OTHER, Duration::from_secs(120));
    let rep = report(&data, "0.2.0-dev.1", None);
    set_age(&rep, Duration::from_secs(30));
    let (marker, finding) = start_run(&data, OWN);
    assert_eq!(finding, StartFinding::Crashed(vec![rep]));
    assert!(!stale.exists());
    marker.unwrap().finish();
}

#[cfg(windows)]
#[test]
fn locked_marker_of_a_running_instance_is_ignored() {
    let data = data_dir("planora-lauf-gesperrt");
    // Die alte Version läuft noch (Update-Neustart): ihr Marker ist offen.
    let (running, first) = start_run(&data, OTHER);
    assert_eq!(first, StartFinding::Clean);
    let running = running.expect("Marker der laufenden Instanz");
    let (marker, finding) = start_run(&data, OWN);
    assert_eq!(finding, StartFinding::Clean, "kein Absturz");
    assert!(
        marker_path(&data, OTHER).exists(),
        "fremder Marker bleibt liegen"
    );
    running.finish();
    marker.unwrap().finish();
    assert!(!marker_path(&data, OTHER).exists());
}

#[test]
fn report_without_marker_is_crashed() {
    let data = data_dir("planora-lauf-bericht");
    let older = report(&data, "0.2.0-dev.1", None);
    let newer = report(&data, "0.2.0-dev.1", None);
    set_age(&older, Duration::from_secs(3600));
    set_age(&newer, Duration::from_secs(60));
    let (marker, finding) = start_run(&data, OWN);
    assert_eq!(finding, StartFinding::Crashed(vec![newer, older]));
    marker.unwrap().finish();
}

// ---- Aufräumen ------------------------------------------------------------------------------

#[test]
fn cleanup_keeps_at_most_10_and_drops_older_than_30_days() {
    let data = data_dir("planora-aufraeumen");
    let dir = data.join(CRASH_DIR);
    let day = Duration::from_secs(24 * 3600);
    let mut recent = Vec::new();
    for i in 0..12u64 {
        let p = report(&data, "0.2.0-dev.1", None);
        set_age(&p, Duration::from_secs(60 * (i + 1)));
        recent.push(p);
    }
    let mut old = Vec::new();
    for i in 0..2u64 {
        let p = report(&data, "0.2.0-dev.1", None);
        set_age(&p, day * (KEEP_DAYS as u32 + 1 + i as u32));
        old.push(p);
    }
    cleanup_reports(&dir, SystemTime::now());
    for p in &old {
        assert!(!p.exists(), "älter als {KEEP_DAYS} Tage: weg");
    }
    for p in &recent[..KEEP_MAX] {
        assert!(p.exists(), "die {KEEP_MAX} neuesten bleiben");
    }
    for p in &recent[KEEP_MAX..] {
        assert!(!p.exists(), "mehr als {KEEP_MAX}: die ältesten weg");
    }
    assert_eq!(reports_in(&dir).len(), KEEP_MAX);
}

#[test]
fn cleanup_ignores_unreadable_entries() {
    let data = data_dir("planora-aufraeumen-fremd");
    let dir = data.join(CRASH_DIR);
    // Fehlender Ordner: nichts passiert.
    cleanup_reports(&dir, SystemTime::now());
    for _ in 0..11 {
        report(&data, "0.2.0-dev.1", None);
    }
    std::fs::create_dir_all(dir.join("absturz-2020-01-01_00-00-00.txt")).unwrap();
    std::fs::write(dir.join("notiz.txt"), "fremd").unwrap();
    cleanup_reports(&dir, SystemTime::now());
    assert!(dir.join("absturz-2020-01-01_00-00-00.txt").is_dir());
    assert!(dir.join("notiz.txt").exists());
    let reports = reports_in(&dir)
        .into_iter()
        .filter(|p| p.is_file() && p.file_name().unwrap() != "notiz.txt")
        .count();
    assert_eq!(reports, KEEP_MAX);
}

// ---- Rückfrage in der App -------------------------------------------------------------------

fn crashed(data: &Path) -> StartFinding {
    StartFinding::Crashed(vec![report(data, "0.2.0-dev.1", None)])
}

#[test]
fn notice_waits_for_first_frame_autosave_offer_and_after_update_notice() {
    let mut r = Rig::bare("planora-rueckfrage-reihenfolge", SIZE);
    let finding = crashed(&r.data);
    r.app.set_start_finding(finding);
    // Autosicherungs-Angebot und Hinweis „Aktualisiert“ stehen schon an.
    let offer = planora_project::docstore::Autosave {
        based_on: None,
        time: "2026-10-25 12:00".into(),
        snapshot: r.app.ws_mut().tabs[0].state.snapshot(),
    };
    r.app.ws_mut().tabs[0].pending_autosave = Some(offer);
    r.app.set_after_update(Some(AfterUpdate {
        version: planora_app::running_version(),
        notes: None,
    }));
    // Erstes Bild: noch nichts.
    r.frame(vec![]);
    assert!(!r.notice_shown(), "nicht im ersten Bild");
    r.settle();
    assert!(r.app.ws_mut().prompt.is_some(), "Angebot steht");
    assert!(!r.notice_shown(), "Autosicherungs-Frage zuerst");
    // Angebot beantwortet: jetzt der Hinweis nach dem Update.
    r.app.ws_mut().tabs[0].pending_autosave = None;
    r.app.ws_mut().prompt = None;
    r.settle();
    assert!(r.app.after_update().is_some());
    assert!(!r.notice_shown(), "nach dem Hinweis „Aktualisiert“");
    // Hinweis geschlossen: jetzt die Rückfrage.
    r.app.set_after_update(None);
    r.settle();
    assert!(r.notice_shown(), "{:?}", r.texts());
    assert!(r.sees(QUESTION));
}

#[test]
fn notice_never_shows_while_restart_locked() {
    let mut r = Rig::bare("planora-rueckfrage-neustart", SIZE);
    let finding = crashed(&r.data);
    r.app.set_start_finding(finding);
    r.app.set_restart_phase(RestartPhase::Exiting);
    r.settle();
    r.settle();
    assert!(!r.notice_shown());
    // Während die Rückmeldung an die alte Version aussteht, auch nicht.
    let mut r = Rig::bare("planora-rueckfrage-okmarker", SIZE);
    let finding = crashed(&r.data);
    r.app.set_start_finding(finding);
    r.app.run_update_startup(true);
    r.frame(vec![]);
    assert!(!r.notice_shown());
}

#[test]
fn report_from_a_newer_version_is_offered_with_its_version() {
    let mut r = Rig::with("planora-rueckfrage-version", SIZE, |d| {
        StartFinding::Crashed(vec![report(d, "9.9.9-dev.1", None)])
    });
    assert!(r.notice_shown());
    assert!(r.sees_part("9.9.9-dev.1"), "{:?}", r.texts());
    r.click(VIEW);
    assert!(r.sees_part("Version: 9.9.9-dev.1"), "{:?}", r.texts());
    assert!(r.sees_part("Testpanik im Modell"));
}

#[test]
fn not_now_keeps_the_report_delete_removes_it() {
    let mut keep = Vec::new();
    let mut r = Rig::with("planora-rueckfrage-spaeter", SIZE, |d| {
        let p = report(d, "0.2.0-dev.1", None);
        keep.push(p.clone());
        StartFinding::Crashed(vec![p])
    });
    r.click(NOT_NOW);
    assert!(!r.notice_shown());
    assert!(keep[0].exists(), "bleibt für den nächsten Start");
    let (marker, finding) = start_run(&r.data, OWN);
    assert_eq!(finding, StartFinding::Crashed(keep.clone()), "kommt wieder");
    marker.unwrap().finish();

    let mut gone = Vec::new();
    let mut r = Rig::with("planora-rueckfrage-loeschen", SIZE, |d| {
        let a = report(d, "0.2.0-dev.1", None);
        let b = report(d, "0.2.0-dev.1", None);
        gone.extend([a.clone(), b.clone()]);
        StartFinding::Crashed(vec![b, a])
    });
    r.click(DELETE);
    assert!(!r.notice_shown());
    assert!(gone.iter().all(|p| !p.exists()), "alle angebotenen weg");
    assert!(reports_in(&r.crash_dir()).is_empty());

    // „Ordner öffnen“ öffnet den Ordner der Berichte.
    let mut r = Rig::with("planora-rueckfrage-ordner", SIZE, crashed);
    r.click(OPEN_FOLDER);
    let dir = r.crash_dir();
    assert_eq!(r.opened.lock().unwrap().clone(), vec![dir]);
}

/// Projektordner `Halter.pcad` mit einer Datei.
fn project(data: &Path) -> PathBuf {
    let doc = data.join("Projekte").join("Halter.pcad");
    std::fs::create_dir_all(&doc).unwrap();
    std::fs::write(doc.join("modell.json"), "{\"teil\":1}").unwrap();
    doc
}

#[test]
fn as_feedback_opens_the_feedback_window_with_the_report_part_on_and_project_off() {
    let mut rep = PathBuf::new();
    let mut r = Rig::with("planora-rueckfrage-meldung", SIZE, |d| {
        let doc = project(d);
        rep = report(d, "0.2.0-dev.1", Some(doc));
        StartFinding::Crashed(vec![rep.clone()])
    });
    r.click(AS_FEEDBACK);
    assert!(!r.notice_shown());
    assert!(r.sees(feedback_dialog::TITLE), "{:?}", r.texts());
    let fb = r.app.feedback_mut();
    assert!(fb.open);
    assert!(fb.is_checked(Part::Absturzbericht), "Bericht angehakt");
    assert!(fb.shown_parts().contains(&Part::Projektdatei));
    assert!(!fb.is_checked(Part::Projektdatei), "Projektdatei aus");
    assert!(
        fb.preview_text(Part::Absturzbericht)
            .unwrap()
            .contains("Ablauf (Backtrace)"),
        "Vorschau zeigt den ganzen Bericht"
    );
    // In der Adresse nur Nachricht und Ort.
    fb.description = "Beim Extrudieren ist es abgestürzt.".into();
    let url = fb.built().url;
    assert!(
        url.contains(&percent_encode("Fehler: Testpanik im Modell")),
        "{url}"
    );
    assert!(url.contains(&percent_encode("Ort: crates/planora-app/src/app.rs:10:5")));
    assert!(
        !url.contains(&percent_encode("Geheimnis")),
        "kein Ablauf in der Adresse"
    );
    assert!(!url.contains(&percent_encode("Ablauf (Backtrace)")));
}

fn send_with_project(prefix: &str, check: bool) -> (Rig, PathBuf) {
    let mut r = Rig::with(prefix, SIZE, |d| {
        let doc = project(d);
        StartFinding::Crashed(vec![report(d, "0.2.0-dev.1", Some(doc))])
    });
    r.click(AS_FEEDBACK);
    r.type_text("Beim Extrudieren ist es abgestürzt.");
    if check {
        r.click(PROJECT_CHECKBOX);
        assert!(r.app.feedback_mut().is_checked(Part::Projektdatei));
    }
    r.click(OPEN_BROWSER);
    let bundle = r
        .opened
        .lock()
        .unwrap()
        .first()
        .cloned()
        .expect("Rückmeldungsordner geöffnet");
    (r, bundle)
}

#[test]
fn project_checkbox_copies_the_document_into_the_bundle_only_when_checked() {
    let (_r, bundle) = send_with_project("planora-rueckfrage-projekt", true);
    let copied = bundle.join("Halter.pcad").join("modell.json");
    assert_eq!(
        std::fs::read_to_string(&copied).unwrap(),
        "{\"teil\":1}",
        "Projektordner liegt im Rückmeldungsordner"
    );
    // Der ganze Bericht liegt als Datei im Ordner.
    let text = std::fs::read_to_string(bundle.join(FEEDBACK_FILE)).unwrap();
    assert!(text.contains("Ablauf (Backtrace)"), "{text}");

    let (_r, bundle) = send_with_project("planora-rueckfrage-ohne-projekt", false);
    assert!(bundle.join(FEEDBACK_FILE).exists());
    assert!(
        !bundle.join("Halter.pcad").exists(),
        "ohne Häkchen keine Kopie"
    );
}

#[test]
fn unexpected_shows_the_short_hint_without_report() {
    let mut r = Rig::with("planora-rueckfrage-unerwartet", SIZE, |_| {
        StartFinding::Unexpected
    });
    assert!(r.sees(UNEXPECTED_TITLE), "{:?}", r.texts());
    assert!(r.sees_part(UNEXPECTED_TEXT));
    assert!(!r.sees(VIEW) && !r.sees(DELETE) && !r.sees(OPEN_FOLDER));
    assert!(r.sees(AS_FEEDBACK) && r.sees(NOT_NOW));
    r.click(AS_FEEDBACK);
    let fb = r.app.feedback_mut();
    assert!(fb.open);
    assert!(!fb.shown_parts().contains(&Part::Absturzbericht));

    // Sauberer Start: keine Rückfrage.
    let r = Rig::with("planora-rueckfrage-sauber", SIZE, |_| StartFinding::Clean);
    assert!(!r.notice_shown() && !r.sees(UNEXPECTED_TITLE));
}

#[test]
fn fits_at_1920x1080_and_no_panic_at_800x600() {
    let mut r = Rig::with("planora-rueckfrage-gross", SIZE, crashed);
    r.click(VIEW);
    let rect = r
        .ctx
        .memory(|m| m.area_rect(crash_dialog::window_id()))
        .expect("Rückfrage gezeigt");
    let screen = Rect::from_min_size(Pos2::ZERO, SIZE);
    assert!(screen.contains_rect(rect), "{rect:?}");
    for t in [VIEW, AS_FEEDBACK, OPEN_FOLDER, NOT_NOW, DELETE] {
        assert!(r.sees(t), "Knopf „{t}“ fehlt");
    }

    let mut r = Rig::with("planora-rueckfrage-klein", SMALL, crashed);
    r.click(VIEW);
    r.settle();
    assert!(r.notice_shown());
    let mut r = Rig::with("planora-rueckfrage-klein-unerwartet", SMALL, |_| {
        StartFinding::Unexpected
    });
    r.settle();
    assert!(r.sees(UNEXPECTED_TITLE));
}

#[test]
fn notice_waits_for_the_autosave_offer_alone() {
    let mut r = Rig::bare("planora-rueckfrage-nur-angebot", SIZE);
    let finding = crashed(&r.data);
    r.app.set_start_finding(finding);
    let offer = planora_project::docstore::Autosave {
        based_on: None,
        time: "2026-10-25 12:00".into(),
        snapshot: r.app.ws_mut().tabs[0].state.snapshot(),
    };
    r.app.ws_mut().tabs[0].pending_autosave = Some(offer);
    r.settle();
    r.settle();
    assert!(r.app.ws_mut().prompt.is_some(), "Angebot steht");
    assert!(!r.notice_shown(), "erst das Autosicherungs-Angebot");
    r.app.ws_mut().tabs[0].pending_autosave = None;
    r.app.ws_mut().prompt = None;
    r.settle();
    assert!(r.notice_shown(), "{:?}", r.texts());
}

#[test]
fn project_preview_has_no_double_spaces() {
    let t = feedback_dialog::project_preview("Halter.pcad");
    assert!(!t.contains("  "), "{t}");
    assert!(t.contains("„Halter.pcad“"));
}

#[test]
fn project_is_only_offered_for_a_pcad_folder() {
    let mut r = Rig::with("planora-rueckfrage-kein-pcad", SIZE, |d| {
        let doc = d.join("Projekte").join("Halter");
        std::fs::create_dir_all(&doc).unwrap();
        std::fs::write(doc.join("modell.json"), "{}").unwrap();
        StartFinding::Crashed(vec![report(d, "0.2.0-dev.1", Some(doc))])
    });
    r.click(AS_FEEDBACK);
    let fb = r.app.feedback_mut();
    assert!(fb.open);
    assert!(fb.shown_parts().contains(&Part::Absturzbericht));
    assert!(
        !fb.shown_parts().contains(&Part::Projektdatei),
        "kein .pcad-Ordner: nicht angeboten"
    );
}

#[test]
fn too_big_project_is_not_copied_and_says_so() {
    let mut r = Rig::with("planora-rueckfrage-zu-gross", SIZE, |d| {
        let doc = project(d);
        StartFinding::Crashed(vec![report(d, "0.2.0-dev.1", Some(doc))])
    });
    // Kleine Grenze: die Testdatei (10 Byte) ist darüber.
    r.app.feedback_mut().set_project_limit(5);
    r.click(AS_FEEDBACK);
    assert!(
        !r.app
            .feedback_mut()
            .shown_parts()
            .contains(&Part::Projektdatei)
    );
    assert!(r.sees(feedback_dialog::PROJECT_TOO_BIG), "{:?}", r.texts());
    assert_eq!(
        feedback_dialog::PROJECT_TOO_BIG,
        "Die Projektdatei ist zu groß zum Mitschicken."
    );
    assert_eq!(feedback_dialog::PROJECT_MAX_BYTES, 200 * 1024 * 1024);
}
