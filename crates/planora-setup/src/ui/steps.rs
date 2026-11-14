//! Linkes Feld „Installation“: die fünf Schritte als Zeilen wie im Feature-Baum der App.
//! Erledigt = Haken in `ok`, aktuell = Fläche `bg_active` mit vollem Punkt, offen = Kreis in
//! `text_secondary`.

use super::style::{self, ROW_H, SPACE_1, SPACE_2};
use crate::wizard::{Page, StepState, Wizard};
use egui::{Color32, Sense, vec2};
use planora_i18n::tr;
use planora_style::{Palette, fonts};

/// Phosphor „check“ (Haken).
const ICON_CHECK: char = '\u{e182}';
/// Phosphor „circle“ (offener Kreis), wie `Icon::ToolCircle` der App.
const ICON_CIRCLE: char = '\u{e18a}';
/// Kantenlänge der Zeichen vor den Zeilen.
const MARK_SIZE: f32 = 14.0;

/// Zeichen vor einer Zeile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Check,
    Dot,
    Circle,
}

/// Aussehen einer Zeile: Zeichen, Farbe des Zeichens, Schriftfarbe, Hintergrund.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowLook {
    pub mark: Mark,
    pub mark_color: Color32,
    pub text: Color32,
    pub fill: Option<Color32>,
}

/// Aussehen einer Zeile im Zustand `state`.
pub fn row_look(state: StepState, p: &Palette) -> RowLook {
    match state {
        StepState::Done => RowLook {
            mark: Mark::Check,
            mark_color: p.ok,
            text: p.ok,
            fill: None,
        },
        StepState::Current => RowLook {
            mark: Mark::Dot,
            mark_color: p.text_primary,
            text: p.text_primary,
            fill: Some(p.bg_active),
        },
        StepState::Open => RowLook {
            mark: Mark::Circle,
            mark_color: p.text_secondary,
            text: p.text_secondary,
            fill: None,
        },
    }
}

/// Name des Schritts, zu dem `page` gehört (die Fehlerseite gehört zu „Installieren“).
pub fn step_name(page: &Page) -> String {
    match page {
        Page::Welcome => tr!("setup-step-welcome"),
        Page::License => tr!("setup-step-license"),
        Page::Folder => tr!("setup-step-folder"),
        Page::Installing | Page::Failed { .. } => tr!("setup-step-install"),
        Page::Done => tr!("setup-step-done"),
    }
}

/// Zeichnet den Inhalt des linken Felds.
pub fn show(ui: &mut egui::Ui, wizard: &Wizard, p: &Palette) {
    ui.add_space(SPACE_1);
    ui.horizontal(|ui| {
        ui.add_space(SPACE_2);
        ui.label(
            egui::RichText::new(tr!("setup-steps-title"))
                .font(style::heading_font())
                .color(p.text_primary),
        );
    });
    ui.add_space(SPACE_2);
    for (page, state) in wizard.steps() {
        row(ui, &step_name(&page), row_look(state, p));
    }
}

/// Eine Zeile in voller Breite des Felds.
fn row(ui: &mut egui::Ui, name: &str, look: RowLook) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    if let Some(fill) = look.fill {
        painter.rect_filled(rect, style::RADIUS_SM, fill);
    }
    let mark_center = egui::pos2(rect.left() + SPACE_2 + MARK_SIZE / 2.0, rect.center().y);
    match look.mark {
        Mark::Dot => {
            painter.circle_filled(mark_center, 4.0, look.mark_color);
        }
        Mark::Check | Mark::Circle => {
            let glyph = if look.mark == Mark::Check {
                ICON_CHECK
            } else {
                ICON_CIRCLE
            };
            painter.text(
                mark_center,
                egui::Align2::CENTER_CENTER,
                glyph,
                egui::FontId::new(MARK_SIZE, fonts::icons()),
                look.mark_color,
            );
        }
    }
    painter.text(
        egui::pos2(rect.left() + SPACE_2 + MARK_SIZE + SPACE_2, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        style::label_font(),
        look.text,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use planora_i18n::{german, with_lang};
    use planora_style::palette;
    use std::path::PathBuf;

    #[test]
    fn rows_follow_feature_tree_look() {
        let p = palette(true);
        let done = row_look(StepState::Done, p);
        assert_eq!(
            (done.mark, done.mark_color, done.fill),
            (Mark::Check, p.ok, None)
        );
        let current = row_look(StepState::Current, p);
        assert_eq!(current.fill, Some(p.bg_active));
        assert_eq!(current.text, p.text_primary);
        let open = row_look(StepState::Open, p);
        assert_eq!((open.mark, open.text), (Mark::Circle, p.text_secondary));
    }

    #[test]
    fn failed_page_belongs_to_install_step() {
        with_lang(&german(), || {
            assert_eq!(step_name(&Page::Welcome), "Willkommen");
            assert_eq!(
                step_name(&Page::Failed {
                    log: PathBuf::from("x")
                }),
                step_name(&Page::Installing)
            );
        });
    }
}
