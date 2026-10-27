//! Umschalten der Sprache bei offenem Fenster (Review-Schwerpunkt 5): Wählt man in
//! Einstellungen → Darstellung „English“, wechseln die Texte der Sprachauswahl im nächsten Bild,
//! und die Wahl steht in der Konfiguration. Einzige Testdatei, die die globale Sprache setzt;
//! darum genau ein Test (parallele Tests im selben Programm würden sich sonst stören).
//! Konfiguration nur im Temp-Ordner.

mod common;

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::settings_window::SettingsWindow;
use planora_app::theme;
use planora_app::workspace::Workspace;
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

    /// Klick auf den sichtbaren Text `text`; bei mehreren der unterste. Danach genau ein Bild.
    fn click(&mut self, text: &str) -> Vec<Shot> {
        self.time += 1.0;
        let seen = self.settle(4);
        let s = seen
            .iter()
            .filter(|s| s.text == text && s.clip.contains(s.rect.center()))
            .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .unwrap_or_else(|| panic!("„{text}“ nicht sichtbar: {:?}", texts(&seen)));
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
        self.frame(vec![])
    }
}

fn texts(seen: &[Shot]) -> Vec<&str> {
    seen.iter().map(|s| s.text.as_str()).collect()
}

#[test]
fn choosing_english_switches_the_window_at_once() {
    let tmp = common::temp_root("planora-sprache-global");
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

    let before = app.settle(4);
    assert!(texts(&before).contains(&"Sprache"), "{:?}", texts(&before));
    assert_eq!(planora_i18n::current().id(), "de");

    // Auswahl öffnen (zeigt die aktuelle Sprache) und „English“ wählen.
    app.click("Deutsch");
    let after = app.click("English");
    let shown = texts(&after);
    assert!(shown.contains(&"Language"), "{shown:?}");
    assert!(shown.contains(&"English"), "{shown:?}");
    assert!(!shown.contains(&"Sprache"), "{shown:?}");
    assert!(!shown.contains(&"Deutsch"), "{shown:?}");
    assert_eq!(planora_i18n::current().id(), "en");
    assert_eq!(app.ws.language_id(), Some("en"));
    let text = fs::read_to_string(&app.ws.config_path).unwrap();
    assert!(text.contains("\"language\": \"en\""), "{text}");
    assert!(!app.ws.language_pending);

    // Lässt sich nicht schreiben: Die Wahl gilt trotzdem sofort und ist vorgemerkt; das nächste
    // gelungene Schreiben nimmt sie mit.
    let good = app.ws.config_path.clone();
    let blocker = tmp.join("blockiert");
    fs::write(&blocker, b"x").unwrap();
    app.ws.config_path = blocker.join("config.json");
    assert!(!app.ws.set_language(&planora_i18n::german()));
    assert_eq!(planora_i18n::current().id(), "de");
    assert!(app.ws.language_pending);
    app.ws.config_path = good;
    assert!(app.ws.set_theme(true));
    let text = fs::read_to_string(&app.ws.config_path).unwrap();
    assert!(text.contains("\"language\": \"de\""), "{text}");
    assert!(!app.ws.language_pending);

    let _ = fs::remove_dir_all(tmp);
}
