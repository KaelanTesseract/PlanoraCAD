use crate::icons::paint_body_icon;
use crate::state::AppState;
use egui::{Sense, vec2};

/// Liste der Körper mit Symbol; Klick wählt einen Körper aus.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Körper");
    ui.add_space(4.0);
    if state.bodies.is_empty() {
        ui.weak("Noch keine Körper. Über „Erzeugen“ im Menü lässt sich einer anlegen.");
        return;
    }
    let mut clicked = None;
    for (i, body) in state.bodies.iter().enumerate() {
        let selected = state.selected_body == Some(i);
        let row_height = 28.0;
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::click());
        if selected {
            ui.painter()
                .rect_filled(rect, 6.0, ui.visuals().selection.bg_fill);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, 6.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let color = ui.visuals().text_color();
        let icon_rect = egui::Rect::from_min_size(rect.min + vec2(4.0, 2.0), vec2(24.0, 24.0));
        paint_body_icon(ui.painter(), icon_rect, body.kind, color);
        ui.painter().text(
            rect.min + vec2(36.0, row_height / 2.0),
            egui::Align2::LEFT_CENTER,
            &body.name,
            egui::FontId::proportional(14.0),
            color,
        );
        if response.clicked() {
            clicked = Some(i);
        }
    }
    if let Some(i) = clicked {
        state.selected_body = Some(i);
    }
}
