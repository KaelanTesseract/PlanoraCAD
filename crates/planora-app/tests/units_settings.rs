//! Umschalten der Maßeinheit in Einstellungen → Darstellung (Auswahl setzt die globale Einheit
//! und schreibt die Konfiguration; Vorgemerktes geht nicht verloren). Einzige Testdatei, die die
//! globale Einheit setzt; darum genau ein Test. Konfiguration nur im Temp-Ordner.
//! (Vorlage: language_global.rs) Umschalten der Sprache bei offenem Fenster (Review-Schwerpunkt 5): Wählt man in
//! Einstellungen → Darstellung „English“, wechseln die Texte der Sprachauswahl im nächsten Bild,
//! und die Wahl steht in der Konfiguration. Einzige Testdatei, die die globale Sprache setzt;
//! darum genau ein Test (parallele Tests im selben Programm würden sich sonst stören).
//! Konfiguration nur im Temp-Ordner.

mod common;

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::settings_window::SettingsWindow;
use planora_app::theme;
use planora_app::workspace::Workspace;
use planora_i18n::DisplayUnit;
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
fn choosing_inch_sets_the_unit_and_writes_the_config() {
    let tmp = common::temp_root("planora-einheit-global");
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
    assert!(
        texts(&before).contains(&"Maßeinheit"),
        "{:?}",
        texts(&before)
    );
    assert_eq!(planora_i18n::current_unit(), DisplayUnit::Millimeter);

    app.click("Millimeter (mm)");
    app.click("Zoll (in)");
    assert_eq!(planora_i18n::current_unit(), DisplayUnit::Zoll);
    assert_eq!(app.ws.length_unit_id(), Some("in"));
    let text = fs::read_to_string(&app.ws.config_path).unwrap();
    assert!(text.contains("\"length_unit\": \"in\""), "{text}");
    assert!(!app.ws.length_unit_pending);

    // Lässt sich nicht schreiben: gilt trotzdem sofort, vorgemerkt; das nächste Schreiben nimmt
    // es mit, und eine andere ausstehende Einstellung (Design) geht nicht verloren.
    let good = app.ws.config_path.clone();
    let blocker = tmp.join("blockiert");
    fs::write(&blocker, b"x").unwrap();
    app.ws.config_path = blocker.join("config.json");
    assert!(!app.ws.set_theme(false));
    assert!(!app.ws.set_length_unit(DisplayUnit::Millimeter));
    assert_eq!(planora_i18n::current_unit(), DisplayUnit::Millimeter);
    assert!(app.ws.length_unit_pending);
    assert!(app.ws.theme_pending);
    app.ws.config_path = good;
    // Ein Schreiben durch eine andere Einstellung nimmt die Einheit mit.
    assert!(app.ws.set_language(&planora_i18n::german()));
    let text = fs::read_to_string(&app.ws.config_path).unwrap();
    assert!(text.contains("\"length_unit\": \"mm\""), "{text}");
    assert!(text.contains("\"theme\""), "{text}");
    assert!(!app.ws.length_unit_pending);
    assert!(!app.ws.theme_pending);

    // Umgekehrt: eine vorgemerkte Sprache geht beim Schreiben der Einheit nicht verloren.
    app.ws.config_path = blocker.join("config.json");
    assert!(!app.ws.set_language(&planora_i18n::english()));
    app.ws.config_path = tmp.join("config.json");
    assert!(app.ws.set_length_unit(DisplayUnit::Zoll));
    let text = fs::read_to_string(&app.ws.config_path).unwrap();
    assert!(text.contains("\"language\": \"en\""), "{text}");
    assert!(!app.ws.language_pending);

    planora_i18n::set_global(planora_i18n::german());
    planora_i18n::set_global_unit(DisplayUnit::Millimeter);
    let _ = fs::remove_dir_all(tmp);
}
