//! Einstellungen → Updates: Schalter „Ausführliches Protokoll“ (unter „Beim Start nach Updates
//! suchen“) mit einzeiligem Hinweis, dass nichts gesendet wird. Gespeichert wird sofort (`verbose_log`), misslingt das, gilt der Wert für die Sitzung
//! (mit Hinweis) und kommt auf jedem Schreibweg mit. Eingeschaltet schreibt das Protokoll je
//! Befehl eine Zeile mit Name und Dauer, nie Projektinhalt. Konfiguration nur im Temp-Ordner.

mod common;

use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands::run_action;
use planora_app::diag;
use planora_app::keymap::{Action, Keymap};
use planora_app::settings_window::*;
use planora_app::theme;
use planora_app::workspace::Workspace;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// Das gemeinsame Protokoll (und sein Schalter) gehört immer nur einem Test.
static DIAG: Mutex<()> = Mutex::new(());

fn diag_lock() -> MutexGuard<'static, ()> {
    DIAG.lock().unwrap_or_else(|e| e.into_inner())
}

struct Env {
    ws: Workspace,
    tmp: PathBuf,
    cfg: PathBuf,
}

fn env(prefix: &str) -> Env {
    let tmp = common::temp_root(prefix);
    fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(None, Config::default());
    let cfg = tmp.join("config.json");
    ws.config_path = cfg.clone();
    assert_ne!(cfg, Config::config_path(), "nie die echte Datei");
    Env { ws, tmp, cfg }
}

/// Lenkt das Schreiben auf einen Pfad unter einer Datei um (Schreiben scheitert).
fn block(e: &mut Env) -> PathBuf {
    let blocker = e.tmp.join("blockiert");
    fs::write(&blocker, b"x").unwrap();
    e.ws.config_path = blocker.join("config.json");
    blocker
}

#[derive(Clone, Debug)]
struct Shot {
    text: String,
    rect: Rect,
    clip: Rect,
    color: Color32,
    size: f32,
    bold: bool,
}

fn walk(shape: &egui::epaint::Shape, clip: Rect, out: &mut Vec<Shot>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
        egui::epaint::Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut color = section.map_or(t.fallback_color, |s| s.format.color);
            if color == Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            out.push(Shot {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                clip,
                color,
                size: section.map_or(0.0, |s| s.format.font_id.size),
                bold: section.is_some_and(|s| {
                    s.format.font_id.family == egui::FontFamily::Name("bold".into())
                }),
            })
        }
        _ => {}
    }
}

/// Das Fenster ohne Bildschirm, wie in der App über `show_in`.
struct App {
    ctx: egui::Context,
    size: egui::Vec2,
    time: f64,
    w: SettingsWindow,
    e: Env,
}

impl App {
    fn new(e: Env, dark: bool) -> App {
        App::sized(e, dark, 1920.0, 1080.0)
    }

    fn sized(e: Env, dark: bool, w: f32, h: f32) -> App {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        theme::apply(&ctx, dark);
        App {
            ctx,
            size: vec2(w, h),
            time: 0.0,
            w: SettingsWindow::default(),
            e,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Shot> {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let (w, ws) = (&mut self.w, &mut self.e.ws);
        let out = self.ctx.run_ui(raw, |ui| {
            w.show_in(ui.ctx(), ws);
        });
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, c.clip_rect, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    fn settle(&mut self, n: usize) -> Vec<Shot> {
        let mut seen = Vec::new();
        for _ in 0..n {
            seen = self.frame(vec![]);
        }
        seen
    }

    /// Klick auf den sichtbaren Text `text`.
    fn click(&mut self, text: &str) {
        self.time += 1.0;
        let seen = self.settle(4);
        let s = seen
            .iter()
            .filter(|s| s.text == text && s.clip.contains(s.rect.center()))
            .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .unwrap_or_else(|| panic!("„{text}“ nicht sichtbar: {:?}", texts(&seen)))
            .clone();
        let at = s.rect.center();
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.settle(3);
    }
}

fn texts(seen: &[Shot]) -> Vec<&str> {
    seen.iter().map(|s| s.text.as_str()).collect()
}

fn find<'a>(seen: &'a [Shot], t: &str) -> &'a Shot {
    seen.iter()
        .find(|s| s.text == t)
        .unwrap_or_else(|| panic!("„{t}“ fehlt: {:?}", texts(seen)))
}

/// Schreibt eine Detailzeile mit eigener Marke; `true`, wenn sie im Protokoll steht.
fn detail_logged(mark: &str) -> bool {
    diag::detail(mark);
    diag::recent_lines(5)
        .iter()
        .any(|l| l.ends_with(&format!("DETAIL {mark}")))
}

#[test]
fn switch_saves_and_turns_on_detail_lines() {
    let _g = diag_lock();
    assert_eq!(verbose_label().as_str(), "Ausführliches Protokoll");
    assert_eq!(
        verbose_note().as_str(),
        "Schreibt mehr in die Protokolldatei auf deinem Rechner, zum Beispiel jeden Befehl und wie lange er dauert. Gesendet wird davon nichts."
    );
    for dark in [false, true] {
        diag::set_verbose(false);
        let e = env("planora-hilfe-schalter");
        let mut a = App::new(e, dark);
        a.w.open(false);
        let seen = a.settle(6);
        // Im Abschnitt „Updates“ unter „Beim Start nach Updates suchen“ (samt dessen Hinweis),
        // vor „Projekte“; kein eigener Abschnitt „Hilfe“. Alles im Fenster sichtbar.
        assert!(!texts(&seen).contains(&"Hilfe"), "{:?}", texts(&seen));
        let order = [
            section_updates(),
            check_on_start_label(),
            privacy_note(),
            verbose_label(),
            verbose_note(),
            section_projects(),
        ];
        for pair in order.windows(2) {
            let (x, y) = (find(&seen, &pair[0]), find(&seen, &pair[1]));
            assert!(
                x.rect.bottom() <= y.rect.top(),
                "„{}“ über „{}“",
                pair[0],
                pair[1]
            );
        }
        for t in [verbose_label().as_str(), verbose_note().as_str()] {
            let s = find(&seen, t);
            assert!(s.clip.contains(s.rect.center()), "{t} sichtbar");
        }
        // Der Hinweis sieht aus wie der Datenschutzhinweis darüber: gleiche Höhe (Schriftgröße
        // und Gewicht) und gleiche Farbe, einzeilig.
        let (note, privacy) = (
            find(&seen, verbose_note().as_str()),
            find(&seen, privacy_note().as_str()),
        );
        assert!(
            note.size == privacy.size && note.bold == privacy.bold,
            "gleiche Schrift: {} / {}",
            note.size,
            privacy.size
        );
        assert_eq!(note.color, privacy.color);
        assert_eq!(note.color, theme::palette(dark).text_tertiary);
        assert!(!a.e.ws.verbose_log(), "Voreinstellung aus");
        assert!(!detail_logged("aus-vorher"));

        a.click(verbose_label().as_str());
        assert!(a.e.ws.verbose_log(), "an");
        let text = fs::read_to_string(&a.e.cfg).unwrap();
        assert!(text.contains("\"verbose_log\": true"), "{text}");
        assert_eq!(a.w.updates_message(), None);
        assert!(
            detail_logged("an-danach"),
            "Detailzeilen gleich nach dem Umschalten"
        );

        a.click(verbose_label().as_str());
        assert!(!a.e.ws.verbose_log(), "wieder aus");
        let text = fs::read_to_string(&a.e.cfg).unwrap();
        assert!(
            !text.contains("verbose_log"),
            "aus wird nicht geschrieben: {text}"
        );
        assert!(!detail_logged("aus-danach"));
    }
    // Beim Start gilt der gespeicherte Schalter.
    let on = Config {
        verbose_log: true,
        ..Config::default()
    };
    let _app = PlanoraApp::for_tests(Workspace::new(None, on));
    assert!(detail_logged("start-an"));
    let _app = PlanoraApp::for_tests(Workspace::new(None, Config::default()));
    assert!(!detail_logged("start-aus"));
}

#[test]
fn verbose_survives_each_of_the_four_write_paths() {
    let _g = diag_lock();
    // Die vier Wege des Plans und dazu das Design (ein fünfter Weg mit eigenem Schreiben).
    for path in ["verzeichnis", "durchsicht", "tasten", "favoriten", "design"] {
        let mut e = env(&format!("planora-hilfe-mit-{path}"));
        let blocker = block(&mut e);
        assert!(!e.ws.set_verbose_log(true), "{path}");
        assert!(e.ws.verbose_log(), "{path}: gilt für die Sitzung");
        assert!(e.ws.update_settings_pending, "{path}: vorgemerkt");
        assert_eq!(fs::read(&blocker).unwrap(), b"x");
        e.ws.config_path = e.cfg.clone();
        let written = match path {
            "verzeichnis" => e.ws.try_persist_config(),
            "durchsicht" => e.ws.set_panel_transparency(20),
            "tasten" => e.ws.set_keymap(Keymap::default()),
            "favoriten" => {
                let mut places = e.ws.places();
                places.add_favorite(&e.tmp.join("Teile")).unwrap();
                e.ws.save_places(places)
            }
            _ => e.ws.set_theme(false),
        };
        assert!(written, "{path}");
        let text = fs::read_to_string(&e.cfg).unwrap();
        assert!(text.contains("\"verbose_log\": true"), "{path}: {text}");
        assert!(!e.ws.update_settings_pending, "{path}");
    }
    diag::set_verbose(false);
}

#[test]
fn other_fields_survive_set_verbose_log() {
    let _g = diag_lock();
    let mut e = env("planora-hilfe-andere");
    Config {
        root: Some(e.tmp.join("Projekte")),
        favorites: vec![e.tmp.join("Fav")],
        panel_transparency: 25,
        update_check: false,
        theme: Some("hell".into()),
        ..Config::default()
    }
    .save_to(&e.cfg)
    .unwrap();
    // Eine vorgemerkte Durchsichtigkeit kommt mit, alles andere bleibt wie auf der Platte.
    block(&mut e);
    assert!(!e.ws.set_panel_transparency(70));
    e.ws.config_path = e.cfg.clone();
    assert!(e.ws.set_verbose_log(true));
    let disk = Config::read_from(&e.cfg).unwrap();
    assert!(disk.verbose_log);
    assert_eq!(disk.root, Some(e.tmp.join("Projekte")));
    assert_eq!(disk.favorites, [e.tmp.join("Fav")]);
    assert_eq!(disk.panel_transparency, 70);
    assert!(!disk.update_check);
    assert_eq!(disk.theme.as_deref(), Some("hell"));
    assert!(!e.ws.panel_transparency_pending);
    // Eine unlesbare Datei bleibt unangetastet.
    fs::write(&e.cfg, b"{kaputt").unwrap();
    assert!(!e.ws.set_verbose_log(false));
    assert!(!e.ws.verbose_log());
    assert_eq!(fs::read(&e.cfg).unwrap(), b"{kaputt");
    diag::set_verbose(false);
}

#[test]
fn write_failure_keeps_value_for_the_session_with_hint() {
    let _g = diag_lock();
    for dark in [false, true] {
        diag::set_verbose(false);
        let e = env("planora-hilfe-fehler");
        let mut a = App::new(e, dark);
        a.w.open(false);
        let blocker = block(&mut a.e);
        a.click(verbose_label().as_str());
        assert!(a.e.ws.verbose_log(), "gilt trotzdem");
        assert!(detail_logged("sitzung"), "wirkt trotzdem");
        assert_eq!(fs::read(&blocker).unwrap(), b"x");
        assert_eq!(
            a.w.updates_message(),
            Some(&RootMessage::Warning(Note::VerboseNotSaved))
        );
        assert_eq!(
            verbose_not_saved().as_str(),
            "Das ausführliche Protokoll gilt nur bis zum Programmende: die Einstellung konnte nicht gespeichert werden."
        );
        let seen = a.settle(3);
        let warn = find(&seen, verbose_not_saved().as_str());
        assert_eq!(warn.color, theme::palette(dark).warning);
        // Wie die anderen Hinweise im Abschnitt „Updates“: vor „Projekte“.
        assert!(warn.rect.bottom() <= find(&seen, section_projects().as_str()).rect.top());
        // Nächstes gelungenes Schreiben nimmt den Wert mit.
        a.e.ws.config_path = a.e.cfg.clone();
        assert!(a.e.ws.try_persist_config());
        assert!(Config::read_from(&a.e.cfg).unwrap().verbose_log);
        // Neu öffnen leert den Hinweis.
        a.w.close();
        a.w.open(false);
        assert_eq!(a.w.updates_message(), None);
    }
    diag::set_verbose(false);
}

#[test]
fn detail_lines_have_no_project_content() {
    let _g = diag_lock();
    let root = common::temp_root("planora-hilfe-inhalt");
    fs::create_dir_all(&root).unwrap();
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.json");
    let id = w
        .active_tab_mut()
        .state
        .create_box(DVec3::splat(12.5))
        .unwrap();
    w.active_tab_mut()
        .state
        .rename_feature(id, "Geheimteil")
        .unwrap();
    let p = w
        .library
        .as_ref()
        .unwrap()
        .create_project("Geheimprojekt")
        .unwrap();
    w.save_as(&p, "Geheimdokument", "").unwrap();
    assert!(w.set_verbose_log(true));
    diag::info("marke-inhalt-start");
    for a in [
        Action::Undo,
        Action::Redo,
        Action::Save,
        Action::ExtrudeSketch,
        Action::New,
    ] {
        run_action(&mut w, a);
    }
    let lines = diag::recent_lines(500);
    let start = lines
        .iter()
        .rposition(|l| l.ends_with("marke-inhalt-start"))
        .unwrap();
    let details: Vec<&String> = lines[start..]
        .iter()
        .filter(|l| l.contains(" DETAIL "))
        .collect();
    assert!(details.len() >= 5, "{lines:#?}");
    let root_text = root.display().to_string();
    for l in &details {
        let text = l.splitn(3, ' ').nth(2).unwrap();
        // Nur „Befehl <Name>: <n> ms“ mit dem festen Namen der Aktion.
        let rest = text
            .strip_prefix("Befehl ")
            .unwrap_or_else(|| panic!("{l}"));
        let (name, dur) = rest.rsplit_once(": ").unwrap_or_else(|| panic!("{l}"));
        assert!(
            [
                Action::Undo,
                Action::Redo,
                Action::Save,
                Action::ExtrudeSketch,
                Action::New
            ]
            .iter()
            .any(|a| a.label() == name),
            "{l}"
        );
        let ms = dur.strip_suffix(" ms").unwrap_or_else(|| panic!("{l}"));
        assert!(ms == "<1" || ms.parse::<u64>().is_ok_and(|n| n >= 1), "{l}");
        for secret in ["Geheim", "12.5", "12,5", root_text.as_str()] {
            assert!(!l.contains(secret), "{secret} in {l}");
        }
    }
    assert!(w.set_verbose_log(false));
    assert!(!detail_logged("inhalt-aus"));
}

#[test]
fn appearance_and_updates_side_by_side_from_1600_pixels() {
    let _g = diag_lock();
    for (w, side) in [
        (1920.0, true),
        (1600.0, true),
        (1599.0, false),
        (1280.0, false),
    ] {
        let e = env("planora-hilfe-spalten");
        let mut a = App::sized(e, true, w, 1080.0);
        a.w.open(false);
        let seen = a.settle(6);
        let (look, upd) = (
            find(&seen, section_appearance().as_str()),
            find(&seen, section_updates().as_str()),
        );
        if side {
            assert!(
                (look.rect.top() - upd.rect.top()).abs() < 1.0
                    && look.rect.right() < upd.rect.left(),
                "{w}: nebeneinander {:?} {:?}",
                look.rect,
                upd.rect
            );
        } else {
            assert!(look.rect.bottom() <= upd.rect.top(), "{w}: untereinander");
        }
        // „Projekte“ steht unter beiden Spalten (die Reihe ist so hoch wie die höhere Spalte).
        let projects = find(&seen, section_projects().as_str()).rect.top();
        for t in [
            transparency_label().as_str(),
            verbose_note().as_str(),
            check_now().as_str(),
        ] {
            assert!(find(&seen, t).rect.bottom() <= projects, "{w}: {t}");
        }
        // Die Menüleiste bleibt frei, auch untereinander.
        let window = a.w.last_rect.expect("Fenster gezeichnet");
        assert!(
            window.top() > 50.0 && window.bottom() < 1030.0,
            "{w}: {window:?}"
        );
        // Bei 1920 Pixeln ist der Hinweis eine Zeile (schmaler bricht er um).
        if w >= 1920.0 {
            let (note, privacy) = (
                find(&seen, verbose_note().as_str()),
                find(&seen, privacy_note().as_str()),
            );
            assert!(
                note.size == privacy.size && note.bold == privacy.bold,
                "{w}: gleiche Schrift"
            );
        }
    }
}
