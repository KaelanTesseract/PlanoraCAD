//! Einstellungen → Projekte → „Name für Zeichnungen“: Vorgabe für „Erstellt“ in neuen Zeichnungen,
//! ab Werk leer (nie der Anmeldename des Rechners). Gespeichert, wenn das Feld den Fokus verliert
//! oder das Fenster schließt; misslingt das, gilt der Name für die Sitzung (mit Hinweis). Einzige
//! Testdatei, die den Namen setzt; darum genau ein Test. Konfiguration nur im Temp-Ordner.

mod common;

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::dialog::close_label;
use planora_app::settings_window::{SettingsWindow, author_label, author_not_saved};
use planora_app::theme;
use planora_app::workspace::Workspace;
use planora_drawing::sheet_info::{ProjectInfo, Stamp};
use planora_project::config::Config;
use std::fs;

struct Shot {
    text: String,
    rect: Rect,
    clip: Rect,
}

fn walk(shape: &egui::epaint::Shape, clip: Rect, out: &mut Vec<Shot>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
        egui::epaint::Shape::Text(t) => out.push(Shot {
            text: t.galley.text().to_string(),
            rect: Rect::from_min_size(t.pos, t.galley.size()),
            clip,
        }),
        _ => {}
    }
}

struct App {
    ctx: egui::Context,
    time: f64,
    w: SettingsWindow,
    ws: Workspace,
}

impl App {
    fn frame(&mut self, events: Vec<Event>) -> Vec<Shot> {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let (w, ws) = (&mut self.w, &mut self.ws);
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

    /// Klick auf den sichtbaren Text `text`, um `dx` nach rechts versetzt; bei mehreren der
    /// unterste. Danach genau ein Bild.
    fn click_at(&mut self, text: &str, dx: f32) -> Vec<Shot> {
        self.time += 1.0;
        let seen = self.settle(4);
        let s = seen
            .iter()
            .filter(|s| s.text == text && s.clip.contains(s.rect.center()))
            .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .unwrap_or_else(|| panic!("„{text}“ nicht sichtbar: {:?}", texts(&seen)));
        let at = s.rect.center()
            + vec2(
                if dx > 0.0 {
                    s.rect.width() / 2.0 + dx
                } else {
                    0.0
                },
                0.0,
            );
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.frame(vec![])
    }
}

impl App {
    fn click(&mut self, text: &str) -> Vec<Shot> {
        self.click_at(text, 0.0)
    }

    fn key(&mut self, key: egui::Key) {
        let ev = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![ev(true), ev(false)]);
        self.settle(2);
    }

    fn typ(&mut self, text: &str) {
        self.frame(vec![Event::Text(text.into())]);
        self.settle(2);
    }
}

fn texts(seen: &[Shot]) -> Vec<&str> {
    seen.iter().map(|s| s.text.as_str()).collect()
}

#[test]
fn the_name_for_drawings_comes_from_the_settings_never_from_the_login() {
    // Ab Werk kein Name, auch wenn der Rechner einen Anmeldenamen hat.
    assert_eq!(Stamp::now().user, "");
    assert_eq!(ProjectInfo::new(&Stamp::now()).login, "");

    let tmp = common::temp_root("planora-name-zeichnungen");
    fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(None, Config::default());
    ws.config_path = tmp.join("config.json");
    assert_ne!(ws.config_path, Config::config_path(), "nie die echte Datei");
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    theme::apply(&ctx, true);
    let mut app = App {
        ctx,
        time: 0.0,
        w: SettingsWindow::default(),
        ws,
    };
    app.w.open(false);
    let seen = app.settle(4);
    assert!(
        texts(&seen).contains(&author_label().as_str()),
        "{:?}",
        texts(&seen)
    );

    // Tippen gilt erst nach Enter (Fokus weg) und wird dann gespeichert.
    app.click_at(&author_label(), 40.0);
    app.typ("Erika Muster");
    assert_eq!(Stamp::now().user, "", "noch nicht übernommen");
    app.key(egui::Key::Enter);
    assert_eq!(app.ws.drawing_author(), "Erika Muster");
    assert_eq!(Stamp::now().user, "Erika Muster");
    assert_eq!(ProjectInfo::new(&Stamp::now()).login, "Erika Muster");
    let text = fs::read_to_string(&app.ws.config_path).unwrap();
    assert!(
        text.contains("\"drawing_author\": \"Erika Muster\""),
        "{text}"
    );

    // Beim Schließen geht eine offene Eingabe nicht verloren.
    app.click_at(&author_label(), 40.0);
    app.key(egui::Key::End);
    app.typ(" B.");
    app.click(&close_label());
    assert!(!app.w.is_open());
    assert_eq!(app.ws.drawing_author(), "Erika Muster B.");
    assert_eq!(
        Config::read_from(&app.ws.config_path)
            .unwrap()
            .drawing_author
            .as_deref(),
        Some("Erika Muster B.")
    );

    // Leer heißt kein Name; dann steht auch nichts in der Datei.
    assert!(app.ws.set_drawing_author("  "));
    assert_eq!(Stamp::now().user, "");
    assert!(
        !fs::read_to_string(&app.ws.config_path)
            .unwrap()
            .contains("drawing_author")
    );

    // Misslingt das Schreiben, gilt der Name für die Sitzung, und der Hinweis erscheint.
    let blocker = tmp.join("blockiert");
    fs::write(&blocker, b"x").unwrap();
    app.ws.config_path = blocker.join("config.json");
    app.w.open(false);
    app.settle(4);
    app.click_at(&author_label(), 40.0);
    app.typ("Max");
    app.key(egui::Key::Enter);
    let seen = app.settle(2);
    assert_eq!(Stamp::now().user, "Max");
    assert!(
        texts(&seen).contains(&author_not_saved().as_str()),
        "{:?}",
        texts(&seen)
    );

    // Ein neuer Start nimmt den gespeicherten Namen.
    let on = Config {
        drawing_author: Some("Anna".into()),
        ..Config::default()
    };
    let _app = planora_app::app::PlanoraApp::for_tests(Workspace::new(None, on));
    assert_eq!(Stamp::now().user, "Anna");
    let _ = fs::remove_dir_all(&tmp);
}
