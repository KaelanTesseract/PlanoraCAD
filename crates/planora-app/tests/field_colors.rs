//! Eingegebene Werte in Text- und Zahlenfeldern stehen in `text_primary` (hell und dunkel),
//! Hinweistext bleibt schwach (`text_tertiary`), gesperrte Felder bleiben gedämpft. Geprüft an
//! den Theme-Helfern und an typischen Feldern der App (Maß-/Parameterfeld, Suche der
//! Tastenkürzel, Dateiname im Dateifenster); die Rückmeldung prüft `feedback_dialog.rs`.

mod common;

use egui::{Color32, Pos2, Rect, vec2};
use planora_app::file_chooser::FileChooser;
use planora_app::formula_field::formula_field;
use planora_app::keymap::settings_ui::{self, CaptureState, search_hint};
use planora_app::places::Places;
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::ParamUnit;
use planora_project::config::Config;
use planora_project::library::Library;

/// Ein gezeichneter Text mit seiner Farbe.
struct Painted {
    text: String,
    color: Color32,
}

fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Painted>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        egui::epaint::Shape::Text(t) => {
            let section = t
                .galley
                .job
                .sections
                .first()
                .map(|s| s.format.color)
                .filter(|c| *c != Color32::PLACEHOLDER);
            out.push(Painted {
                text: t.galley.text().to_string(),
                color: t
                    .override_text_color
                    .or(section)
                    .unwrap_or(t.fallback_color),
            });
        }
        _ => {}
    }
}

/// Zeichnet `f` drei Bilder lang im Thema `dark` und liefert die Texte des letzten Bilds.
fn draw(dark: bool, f: &mut dyn FnMut(&mut egui::Ui)) -> Vec<Painted> {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, dark);
    let mut painted = Vec::new();
    for i in 0..3 {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 900.0))),
            time: Some(f64::from(i) * 0.05),
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| f(ui));
        });
        painted.clear();
        for c in &out.shapes {
            walk(&c.shape, &mut painted);
        }
        out.drop_without_applying_deltas();
    }
    painted
}

fn colour_of(painted: &[Painted], text: &str) -> Color32 {
    painted
        .iter()
        .find(|p| p.text == text)
        .unwrap_or_else(|| {
            let all: Vec<&str> = painted.iter().map(|p| p.text.as_str()).collect();
            panic!("„{text}“ nicht gezeichnet: {all:?}")
        })
        .color
}

#[test]
fn values_in_theme_fields_use_the_primary_text_colour() {
    for dark in [true, false] {
        let p = theme::palette(dark);
        let mut name = String::from("Halter links");
        let mut empty = String::new();
        let mut locked = String::from("gesperrt 12");
        let mut number = 42.5_f64;
        let painted = draw(dark, &mut |ui| {
            theme::text_field(ui, egui::TextEdit::singleline(&mut name));
            theme::text_field(
                ui,
                egui::TextEdit::singleline(&mut empty).hint_text("Name eingeben"),
            );
            theme::text_field_enabled(ui, false, egui::TextEdit::singleline(&mut locked));
            theme::field(ui, egui::DragValue::new(&mut number));
            let mut shown = "Nur lesbar";
            theme::field(ui, egui::TextEdit::multiline(&mut shown));
            ui.label("Beschriftung");
        });
        assert_eq!(
            colour_of(&painted, "Halter links"),
            p.text_primary,
            "{dark}"
        );
        assert_eq!(colour_of(&painted, "42.5"), p.text_primary, "{dark}");
        assert_eq!(colour_of(&painted, "Nur lesbar"), p.text_primary, "{dark}");
        // Hinweis schwach, Beschriftung grau, gesperrt nicht in voller Farbe.
        assert_eq!(
            colour_of(&painted, "Name eingeben"),
            p.text_tertiary,
            "{dark}"
        );
        assert_eq!(
            colour_of(&painted, "Beschriftung"),
            p.text_secondary,
            "{dark}"
        );
        assert_ne!(colour_of(&painted, "gesperrt 12"), p.text_primary, "{dark}");
        // Der Feldstil gilt nur für das Feld: danach ist die Schriftfarbe wieder die alte.
        let after = draw(dark, &mut |ui| {
            theme::text_field(ui, egui::TextEdit::singleline(&mut name));
            ui.add(egui::Button::new("Knopf danach"));
        });
        assert_eq!(
            colour_of(&after, "Knopf danach"),
            p.text_secondary,
            "{dark}"
        );
    }
}

#[test]
fn formula_field_value_uses_the_primary_text_colour() {
    let scope = AppState::new().document.param_scope();
    for dark in [true, false] {
        let mut text = String::from("40");
        let painted = draw(dark, &mut |ui| {
            formula_field(ui, "mass", &mut text, ParamUnit::Length, &scope, 120.0);
        });
        assert_eq!(colour_of(&painted, "40"), theme::palette(dark).text_primary);
    }
}

#[test]
fn shortcut_search_shows_input_strong_and_hint_weak() {
    let root = common::temp_root("planora-feldfarben-keys");
    let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    ws.config_path = root.join("config.json");
    for dark in [true, false] {
        let p = theme::palette(dark);
        let mut state = CaptureState::default();
        let painted = draw(dark, &mut |ui| settings_ui::show(ui, &mut ws, &mut state));
        assert_eq!(
            colour_of(&painted, &search_hint()),
            p.text_tertiary,
            "{dark}"
        );
        let mut state = CaptureState::default();
        state.filter = "speichern".into();
        let painted = draw(dark, &mut |ui| settings_ui::show(ui, &mut ws, &mut state));
        assert_eq!(colour_of(&painted, "speichern"), p.text_primary, "{dark}");
    }
}

#[test]
fn file_name_in_the_file_window_uses_the_primary_text_colour() {
    let dir = common::temp_root("planora-feldfarben-datei");
    std::fs::create_dir_all(&dir).unwrap();
    for dark in [true, false] {
        let mut chooser = FileChooser::save(dir.clone(), "Halter.planora".into(), "planora");
        let mut places = Places::default();
        let painted = draw(dark, &mut |ui| {
            chooser.show_body(ui, &mut places);
        });
        assert_eq!(
            colour_of(&painted, &chooser.file_name),
            theme::palette(dark).text_primary,
            "{dark}"
        );
    }
}
