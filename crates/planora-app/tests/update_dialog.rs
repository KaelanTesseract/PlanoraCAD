//! Meldung „Neue Version verfügbar“: erscheint von allein, wenn die Prüfung ein Update findet
//! und nichts anderes offen ist (sonst später), zeigt die Patchnotes aller Versionen dazwischen,
//! „Später“ hält sie für 24 Stunden der Sitzung zurück, „Überspringen“ für diese Version immer,
//! Hinweis in der Statuszeile, Fensterregeln, „Jetzt aktualisieren“ hält die Meldung für den
//! Fortschritt offen (der Ablauf selbst in `tests/update_install.rs`).
//!
//! Mit echten Bildern der ganzen App bei 1920 × 1080 und einer Nachbildung der Release-Quelle
//! (nie Netz); Bibliothek und Konfiguration nur im Temp-Ordner.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::{PlanoraApp, open_projects};
use planora_app::settings_window::CHECK_NOW;
use planora_app::update_check::now_unix;
use planora_app::update_dialog::*;
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{fonts, running_version, theme};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_update::check::{NetError, ReleaseSource};
use planora_update::{EXE_NAME, REPO, SHA256_NAME, Version};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const SIZE: Vec2 = vec2(1920.0, 1080.0);

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

/// Release-Liste aus (Tag, Patchnotes).
fn list(entries: &[(&str, &str)]) -> String {
    let items = entries
        .iter()
        .map(|(tag, body)| {
            let asset = |name: &str| {
                json!({
                    "name": name,
                    "browser_download_url":
                        format!("https://github.com/{REPO}/releases/download/{tag}/{name}"),
                    "size": 10,
                })
            };
            json!({
                "tag_name": tag,
                "body": body,
                "assets": [asset(EXE_NAME), asset(SHA256_NAME)],
            })
        })
        .collect();
    Value::Array(items).to_string()
}

fn one_newer() -> String {
    list(&[("v99.0.0-dev.1", "- Schneller")])
}

/// Nachbildung der Quelle: Antwort austauschbar, zählt die Abrufe, wartet auf Wunsch.
struct Fake {
    answer: Mutex<Result<String, NetError>>,
    calls: AtomicUsize,
    gate: Mutex<Option<mpsc::Receiver<()>>>,
}

impl Fake {
    fn new(answer: String) -> Arc<Fake> {
        Arc::new(Fake {
            answer: Mutex::new(Ok(answer)),
            calls: AtomicUsize::new(0),
            gate: Mutex::new(None),
        })
    }

    fn set(&self, answer: String) {
        *self.answer.lock().unwrap() = Ok(answer);
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ReleaseSource for Fake {
    fn fetch_releases(&self) -> Result<String, NetError> {
        if let Some(gate) = self.gate.lock().unwrap().take() {
            let _ = gate.recv_timeout(Duration::from_secs(10));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.answer.lock().unwrap().clone()
    }
}

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

/// Erfasster Text eines Bildes mit den Abschnitten (Text, Schriftfamilie, Größe).
struct Seen {
    text: String,
    rect: Rect,
    sections: Vec<(String, egui::FontFamily, f32)>,
}

struct Harness {
    ctx: egui::Context,
    app: PlanoraApp,
    cfg: PathBuf,
    time: f64,
    seen: Vec<Seen>,
}

impl Harness {
    fn new(ws: Workspace, cfg: PathBuf, source: Arc<dyn ReleaseSource + Send + Sync>) -> Self {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        Harness {
            ctx,
            app: PlanoraApp::for_tests_with_updates(ws, source),
            cfg,
            time: 0.0,
            seen: Vec::new(),
        }
    }

    /// Wie [`Harness::new`] mit einem fälligen Start-Check gegen `fake`, bis dessen Ergebnis da ist.
    fn started(prefix: &str, fake: Arc<Fake>) -> Self {
        let (ws, cfg) = workspace(prefix);
        let mut h = Harness::new(ws, cfg, fake);
        h.until_idle();
        h
    }

    fn frame(&mut self, events: Vec<Event>) {
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
                    let job = &t.galley.job;
                    let sections = job
                        .sections
                        .iter()
                        .map(|s| {
                            (
                                job.text[s.byte_range.start.0..s.byte_range.end.0].to_string(),
                                s.format.font_id.family.clone(),
                                s.format.font_id.size,
                            )
                        })
                        .collect();
                    out.push(Seen {
                        text: t.galley.text().to_string(),
                        rect: Rect::from_min_size(t.pos, t.galley.size()),
                        sections,
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
        self.seen = seen;
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
            assert!(start.elapsed() < Duration::from_secs(10), "Zeit abgelaufen");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn until_idle(&mut self) {
        self.until(|a| !a.update_check_running());
        self.settle();
    }

    fn sees(&self, text: &str) -> bool {
        self.seen.iter().any(|s| s.text == text)
    }

    fn texts(&self) -> Vec<&str> {
        self.seen.iter().map(|s| s.text.as_str()).collect()
    }

    fn find(&self, text: &str) -> &Seen {
        self.seen
            .iter()
            .filter(|s| s.text == text)
            .max_by(|a, b| a.rect.top().partial_cmp(&b.rect.top()).unwrap())
            .unwrap_or_else(|| panic!("„{text}“ nicht zu sehen: {:?}", self.texts()))
    }

    fn click(&mut self, text: &str) {
        self.settle();
        let at = self.find(text).rect.center();
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

    fn press(&mut self, k: Key) {
        let ev = |pressed| Event::Key {
            key: k,
            physical_key: Some(k),
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![ev(true)]);
        self.frame(vec![ev(false)]);
        self.settle();
    }

    fn notice_open(&mut self) -> bool {
        self.app.update_dialog_mut().open
    }

    /// Suche über den Knopf in den Einstellungen; schließt die Einstellungen danach wieder.
    fn manual_check(&mut self) {
        self.app.windows_mut().2.open(false);
        self.settle();
        self.click(CHECK_NOW);
        self.until_idle();
        self.app.windows_mut().2.close();
        self.settle();
    }
}

#[test]
fn notice_opens_when_an_update_is_found_and_nothing_else_is_open() {
    let mut h = Harness::started("planora-meldung-auf", Fake::new(one_newer()));
    assert!(h.app.pending_notice().is_some());
    assert!(h.notice_open(), "erscheint von allein");
    let latest = v("99.0.0-dev.1");
    for t in [
        title(&latest).as_str(),
        running_line(&running_version()).as_str(),
        version_heading(&latest).as_str(),
        "Schneller",
        INSTALL_NOW,
        LATER,
        SKIP,
    ] {
        assert!(h.sees(t), "„{t}“ fehlt: {:?}", h.texts());
    }
    assert_eq!(title(&latest), "Neue Version verfügbar: 99.0.0-dev.1 (Dev)");
    assert_eq!(
        running_line(&v("0.2.0-dev.1")),
        "Du hast Version 0.2.0-dev.1"
    );
    assert!(!h.sees(MORE_ON_GITHUB), "nicht gekürzt");
    // Solange die Meldung offen ist, kein Hinweis in der Statuszeile.
    assert!(!h.sees(&status_hint(&latest)));
    // Ganz im Fenster.
    let screen = Rect::from_min_size(Pos2::ZERO, SIZE);
    for t in [INSTALL_NOW, SKIP] {
        assert!(screen.contains_rect(h.find(t).rect), "{t}");
    }
}

#[test]
fn notice_waits_while_a_prompt_or_window_is_open_then_appears() {
    // Eine Rückfrage ist offen, als das Ergebnis kommt.
    let (mut ws, cfg) = workspace("planora-meldung-rueckfrage");
    ws.prompt = Some(Prompt::Notice("Hinweis".into()));
    let mut h = Harness::new(ws, cfg, Fake::new(one_newer()));
    h.until_idle();
    assert!(h.app.pending_notice().is_some());
    assert!(!h.notice_open(), "nicht mitten in einer Rückfrage");
    assert!(!h.sees(&title(&v("99.0.0-dev.1"))));
    h.app.ws_mut().prompt = None;
    h.settle();
    assert!(h.notice_open(), "erscheint, sobald frei");

    // Kommt eine Rückfrage, während die Meldung offen ist, tritt sie zurück und kommt danach
    // wieder (das ist kein „Später“).
    h.app.ws_mut().prompt = Some(Prompt::Notice("Hinweis".into()));
    h.settle();
    assert!(!h.notice_open());
    assert_eq!(h.app.update_dialog_mut().snooze_until, None);
    h.app.ws_mut().prompt = None;
    h.settle();
    assert!(h.notice_open());

    // Die Einstellungen sind offen, als das Ergebnis kommt.
    let (ws, cfg) = workspace("planora-meldung-fenster");
    let fake = Fake::new(one_newer());
    let (release, gate) = mpsc::channel();
    *fake.gate.lock().unwrap() = Some(gate);
    let mut h = Harness::new(ws, cfg, fake.clone());
    h.frame(vec![]);
    assert!(h.app.update_check_running());
    h.app.windows_mut().2.open(false);
    h.settle();
    release.send(()).unwrap();
    h.until_idle();
    assert!(h.app.pending_notice().is_some());
    assert!(
        !h.notice_open(),
        "wartet, solange die Einstellungen offen sind"
    );
    assert!(h.app.windows_mut().2.is_open());
    h.app.windows_mut().2.close();
    h.settle();
    assert!(h.notice_open(), "erscheint nach dem Schließen");

    // Ebenso ein Fenster des Tabs (hier „Über“).
    let (mut ws, cfg) = workspace("planora-meldung-ueber");
    ws.about_open = true;
    let mut h = Harness::new(ws, cfg, Fake::new(one_newer()));
    h.until_idle();
    assert!(!h.notice_open());
    h.app.ws_mut().about_open = false;
    h.settle();
    assert!(h.notice_open());
}

#[test]
fn notes_of_all_skipped_versions_are_shown_newest_first() {
    let fake = Fake::new(list(&[
        ("v99.0.0-dev.1", "Erste Notiz"),
        ("v99.0.0-dev.3", "Dritte Notiz"),
        ("v99.0.0-dev.2", ""),
    ]));
    let mut h = Harness::started("planora-meldung-mehrere", fake);
    assert!(h.notice_open());
    assert!(h.sees(&title(&v("99.0.0-dev.3"))));
    let y = |h: &Harness, t: &str| h.find(t).rect.top();
    let (h3, h2, h1) = (
        version_heading(&v("99.0.0-dev.3")),
        version_heading(&v("99.0.0-dev.2")),
        version_heading(&v("99.0.0-dev.1")),
    );
    assert_eq!(h1, "Version 99.0.0-dev.1");
    assert!(y(&h, &h3) < y(&h, "Dritte Notiz"));
    assert!(y(&h, "Dritte Notiz") < y(&h, &h2));
    // Version ohne Beschreibung.
    assert!(y(&h, &h2) < y(&h, NO_NOTES));
    assert!(y(&h, NO_NOTES) < y(&h, &h1));
    assert!(y(&h, &h1) < y(&h, "Erste Notiz"));
}

#[test]
fn bold_headings_and_bullets_are_rendered() {
    let notes = "# Neu\n\n### Klein\n\n- Ein Punkt mit **fett** darin\n* Noch einer\n\nEin Absatz\nüber zwei Zeilen.";
    let fake = Fake::new(list(&[("v99.0.0-dev.1", notes)]));
    let mut h = Harness::started("planora-meldung-markdown", fake);
    assert!(h.notice_open());
    for t in [
        "Neu",
        "Klein",
        "Ein Punkt mit fett darin",
        "Noch einer",
        "Ein Absatz über zwei Zeilen.",
    ] {
        assert!(h.sees(t), "„{t}“ fehlt: {:?}", h.texts());
    }
    let body = h.find("Ein Absatz über zwei Zeilen.").sections[0].2;
    // Überschriften größer als der Fließtext und halbfett, die erste Ebene am größten.
    let neu = &h.find("Neu").sections[0];
    let klein = &h.find("Klein").sections[0];
    assert_eq!(neu.1, fonts::semibold());
    assert!(
        neu.2 > klein.2 && klein.2 > body,
        "{neu:?} {klein:?} {body}"
    );
    // Fetter Text halbfett, der Rest nicht.
    let bullet = h.find("Ein Punkt mit fett darin");
    let fett = bullet.sections.iter().find(|s| s.0 == "fett").unwrap();
    assert_eq!(fett.1, fonts::semibold());
    let rest = bullet
        .sections
        .iter()
        .find(|s| s.0 == "Ein Punkt mit ")
        .unwrap();
    assert_ne!(rest.1, fonts::semibold());
    // Aufzählungspunkte mit Spiegelstrich links davor, auf gleicher Höhe.
    let dashes: Vec<Rect> = h
        .seen
        .iter()
        .filter(|s| s.text == "–")
        .map(|s| s.rect)
        .collect();
    assert_eq!(dashes.len(), 2, "{:?}", h.texts());
    let r = bullet.rect;
    assert!(
        dashes
            .iter()
            .any(|d| d.right() <= r.left() && (d.top() - r.top()).abs() < 2.0)
    );
}

#[test]
fn truncated_notes_show_the_hint() {
    let tags: Vec<String> = (1..=22).map(|i| format!("v99.0.0-dev.{i}")).collect();
    let entries: Vec<(&str, &str)> = tags.iter().map(|t| (t.as_str(), "Notiz")).collect();
    let mut h = Harness::started("planora-meldung-gekuerzt", Fake::new(list(&entries)));
    assert!(h.notice_open());
    assert!(h.app.pending_notice().unwrap().truncated);
    assert!(h.sees(MORE_ON_GITHUB), "{:?}", h.texts());
    assert_eq!(MORE_ON_GITHUB, "… und weitere Änderungen auf GitHub.");
    // Der Hinweis steht über den Knöpfen und ganz im Fenster.
    let screen = Rect::from_min_size(Pos2::ZERO, SIZE);
    let hint = h.find(MORE_ON_GITHUB).rect;
    assert!(screen.contains_rect(hint));
    assert!(hint.bottom() <= h.find(LATER).rect.top());
}

#[test]
fn later_closes_and_does_not_return_in_this_session_within_24h() {
    let fake = Fake::new(one_newer());
    let mut h = Harness::started("planora-meldung-spaeter", fake.clone());
    assert!(h.notice_open());
    let before = now_unix();
    h.click(LATER);
    assert!(!h.notice_open());
    let until = h.app.update_dialog_mut().snooze_until.unwrap();
    assert!(until >= before + SNOOZE_SECS && until <= now_unix() + SNOOZE_SECS);
    assert_eq!(SNOOZE_SECS, 24 * 3600);
    // Nichts gespeichert: weder übersprungen noch ein Zeitpunkt für „Später“.
    assert_eq!(h.app.ws_mut().skipped_version(), None);
    // Von allein bleibt sie zu, auch wenn das Update vorgemerkt bleibt.
    h.settle();
    assert!(h.app.pending_notice().is_some());
    assert!(!h.notice_open(), "nicht vor Ablauf der 24 Stunden");
    // Nach Ablauf erscheint sie wieder.
    h.app.update_dialog_mut().snooze_until = Some(now_unix() - 1);
    h.settle();
    assert!(h.notice_open());
    // Die Suche von Hand zeigt sie auch vor Ablauf wieder (sie hebt „Später“ auf).
    h.click(LATER);
    assert!(!h.notice_open());
    h.manual_check();
    assert_eq!(fake.calls(), 2);
    assert!(h.notice_open(), "nach der Suche von Hand wieder da");
    assert_eq!(h.app.update_dialog_mut().snooze_until, None);

    // Esc wirkt wie „Später“, nie wie „Überspringen“.
    h.press(Key::Escape);
    assert!(!h.notice_open());
    assert!(h.app.update_dialog_mut().snooze_until.unwrap() > now_unix());
    assert_eq!(h.app.ws_mut().skipped_version(), None);
    assert!(h.app.pending_notice().is_some());

    // ✕ ebenso.
    h.app.update_dialog_mut().snooze_until = None;
    h.settle();
    assert!(h.notice_open());
    // Das Schließen-Symbol ist der Text rechts vom Titel in derselben Zeile.
    let head = h.find(title(&v("99.0.0-dev.1")).as_str()).rect;
    let at = h
        .seen
        .iter()
        .filter(|s| {
            s.rect.left() > head.right() && (s.rect.center().y - head.center().y).abs() < 6.0
        })
        .max_by(|a, b| a.rect.left().partial_cmp(&b.rect.left()).unwrap())
        .expect("Schließen-Symbol")
        .rect
        .center();
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    h.frame(vec![Event::PointerMoved(at)]);
    h.frame(vec![button(true)]);
    h.frame(vec![button(false)]);
    h.settle();
    assert!(!h.notice_open());
    assert!(
        h.app.update_dialog_mut().snooze_until.is_some(),
        "✕ wie „Später“"
    );
    assert_eq!(h.app.ws_mut().skipped_version(), None);
}

#[test]
fn skip_saves_the_version_and_the_same_version_is_not_offered_again_but_the_next_is() {
    let fake = Fake::new(one_newer());
    let mut h = Harness::started("planora-meldung-ueberspringen", fake.clone());
    assert!(h.notice_open());
    h.click(SKIP);
    assert!(!h.notice_open());
    assert_eq!(h.app.ws_mut().skipped_version(), Some(v("99.0.0-dev.1")));
    let disk = Config::read_from(&h.cfg).unwrap();
    assert_eq!(
        disk.skipped_version.as_deref(),
        Some("99.0.0-dev.1"),
        "gespeichert"
    );
    assert_eq!(h.app.pending_notice(), None);
    assert!(!h.sees(&status_hint(&v("99.0.0-dev.1"))));
    // Die Prüfung beim Start bietet dieselbe Version nicht wieder an (ein neuer Programmstart
    // mit derselben Konfiguration) …
    let fresh = Fake::new(one_newer());
    let mut c = Config::read_from(&h.cfg).unwrap();
    // Fällig: die letzte Prüfung vergessen.
    c.last_update_check = None;
    let mut ws = Workspace::new(None, c);
    ws.config_path = h.cfg.clone();
    let mut again = Harness::new(ws, h.cfg.clone(), fresh.clone());
    again.until(|_| fresh.calls() == 1);
    again.until_idle();
    assert_eq!(again.app.pending_notice(), None);
    assert!(!again.notice_open());
    // … die Suche von Hand schon (sie übergeht das Überspringen) …
    h.manual_check();
    assert_eq!(fake.calls(), 2);
    assert_eq!(
        h.app.pending_notice().map(|u| u.latest.version),
        Some(v("99.0.0-dev.1"))
    );
    assert!(h.notice_open());
    h.click(SKIP);
    // … und die nächste Version auch beim Start.
    fake.set(list(&[
        ("v99.0.0-dev.1", "Alt"),
        ("v99.0.0-dev.2", "Neuer"),
    ]));
    h.manual_check();
    assert_eq!(
        h.app.pending_notice().map(|u| u.latest.version),
        Some(v("99.0.0-dev.2"))
    );
    assert!(h.notice_open());
    assert!(h.sees(&title(&v("99.0.0-dev.2"))));
}

#[test]
fn statusline_hint_while_pending() {
    let mut h = Harness::started("planora-meldung-status", Fake::new(one_newer()));
    let hint = status_hint(&v("99.0.0-dev.1"));
    assert_eq!(hint, "Update verfügbar: 99.0.0-dev.1");
    assert!(h.notice_open());
    assert!(!h.sees(&hint));
    h.click(LATER);
    assert!(h.sees(&hint), "{:?}", h.texts());
    // Der Hinweis steht unten in der Statuszeile neben deren Text.
    let status = h.find("0 Körper").rect;
    let r = h.find(&hint).rect;
    assert!(r.left() > status.right());
    assert!((r.center().y - status.center().y).abs() < 4.0);
    assert!(r.bottom() <= SIZE.y);
    // Ein Klick öffnet die Meldung (trotz „Später“).
    h.click(&hint);
    assert!(h.notice_open());
    assert!(!h.sees(&hint));
    // Ohne vorgemerktes Update kein Hinweis.
    let mut h = Harness::started("planora-meldung-status-leer", Fake::new("[]".into()));
    assert!(!h.texts().iter().any(|t| t.starts_with("Update verfügbar")));
    assert!(!h.notice_open());
    let _ = &mut h;
}

#[test]
fn window_exclusion_with_projects_and_settings() {
    let mut h = Harness::started("planora-meldung-fenster-regeln", Fake::new(one_newer()));
    assert!(h.notice_open());
    // Bereichstasten sind gesperrt, solange die Meldung offen ist (S öffnet die Übersicht nicht).
    h.press(Key::S);
    assert!(!h.app.overview_mut().open);
    assert!(h.notice_open());
    // Die Einstellungen öffnen: die Meldung tritt zurück (zuletzt geöffnet gewinnt) …
    h.app.windows_mut().2.open(false);
    h.settle();
    assert!(!h.notice_open());
    assert!(h.app.windows_mut().2.is_open());
    // … und kommt wieder, wenn sie zu sind (kein „Später“).
    h.app.windows_mut().2.close();
    h.settle();
    assert!(h.notice_open());
    // Ebenso Projekte.
    {
        let (ws, projects, settings) = h.app.windows_mut();
        open_projects(ws, projects, settings);
    }
    h.settle();
    assert!(!h.notice_open());
    assert!(h.app.windows_mut().1.is_open());
    // Der Hinweis in der Statuszeile öffnet die Meldung und schließt Projekte.
    let hint = status_hint(&v("99.0.0-dev.1"));
    h.click(&hint);
    assert!(h.notice_open());
    assert!(!h.app.windows_mut().1.is_open());
    // Die Übersicht der Tastenkürzel schließt die Meldung ebenso.
    h.app.overview_mut().open_focused();
    h.settle();
    assert!(!h.notice_open());
    assert!(h.app.overview_mut().open);
    h.app.overview_mut().close();
    h.settle();
    assert!(h.notice_open());
    // Nie beides zugleich sichtbar.
    assert!(!h.app.windows_mut().2.is_open() && !h.app.windows_mut().1.is_open());
}

#[test]
fn install_now_keeps_the_notice_for_the_progress() {
    // Ohne bekannte Programmdatei (Testaufbau) endet „Jetzt aktualisieren“ sofort mit einem
    // Hinweis; Download und Austausch prüft `tests/update_install.rs`.
    let mut h = Harness::started("planora-meldung-jetzt", Fake::new(one_newer()));
    assert!(h.notice_open());
    h.click(INSTALL_NOW);
    assert!(h.notice_open(), "bleibt offen für Fortschritt oder Hinweis");
    let d = h.app.update_dialog_mut();
    assert!(d.installing);
    assert_eq!(
        d.update.as_ref().map(|u| u.latest.version),
        Some(v("99.0.0-dev.1")),
        "das einzuspielende Update bleibt"
    );
    assert_eq!(d.snooze_until, None);
    let expected = if cfg!(windows) {
        planora_app::update_install::NO_EXE
    } else {
        planora_app::update_install::ONLY_WINDOWS
    };
    assert!(h.sees(expected), "{:?}", h.texts());
    assert!(!h.sees(INSTALL_NOW), "statt der Knöpfe");
    assert!(!h.sees(&status_hint(&v("99.0.0-dev.1"))));
    assert_eq!(h.app.ws_mut().skipped_version(), None);
    // ✕ bzw. Esc nach dem Hinweis schließt; die Meldung ruht dann wie nach „Später“.
    h.press(Key::Escape);
    assert!(!h.notice_open());
    assert!(!h.app.update_dialog_mut().installing);
    assert!(h.app.update_dialog_mut().snooze_until.is_some());
    h.settle();
    assert!(!h.notice_open());
    assert!(h.sees(&status_hint(&v("99.0.0-dev.1"))));
}

#[test]
fn no_notice_when_the_check_found_nothing() {
    for answer in ["[]".to_string(), list(&[("v0.0.1", "Alt")])] {
        let fake = Fake::new(answer);
        let mut h = Harness::started("planora-meldung-nichts", fake.clone());
        assert_eq!(fake.calls(), 1);
        assert_eq!(h.app.pending_notice(), None);
        assert!(!h.notice_open());
        assert!(
            !h.texts()
                .iter()
                .any(|t| t.starts_with("Neue Version verfügbar"))
        );
        assert!(!h.texts().iter().any(|t| t.starts_with("Update verfügbar")));
    }
}
