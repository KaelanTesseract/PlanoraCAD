use crate::icons::{paint_body_icon, paint_sketch_icon};
use crate::state::AppState;
use egui::{Sense, vec2};

const ROW_HEIGHT: f32 = 28.0;

/// Eine Zeile mit Symbol und Name; liefert die Antwort für Klicks.
fn row(
    ui: &mut egui::Ui,
    selected: bool,
    name: &str,
    paint_icon: impl FnOnce(&egui::Painter, egui::Rect, egui::Color32),
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, 6.0, ui.visuals().selection.bg_fill);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 6.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let color = ui.visuals().text_color();
    let icon_rect = egui::Rect::from_min_size(rect.min + vec2(4.0, 2.0), vec2(24.0, 24.0));
    paint_icon(ui.painter(), icon_rect, color);
    ui.painter().text(
        rect.min + vec2(36.0, ROW_HEIGHT / 2.0),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(14.0),
        color,
    );
    response
}

/// Liste der Körper und Skizzen mit Symbol; Klick wählt, Doppelklick auf eine Skizze öffnet sie.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Körper");
    ui.add_space(4.0);
    if state.bodies.is_empty() {
        ui.weak("Noch keine Körper. Über „Erzeugen“ im Menü lässt sich einer anlegen.");
    }
    let mut clicked = None;
    for (i, body) in state.bodies.iter().enumerate() {
        let selected = state.selected_body == Some(i);
        let kind = body.kind;
        if row(ui, selected, &body.name, |p, r, c| {
            paint_body_icon(p, r, kind, c)
        })
        .clicked()
        {
            clicked = Some(i);
        }
    }
    if let Some(i) = clicked {
        state.selected_body = Some(i);
        state.selected_sketch = None;
    }

    ui.add_space(12.0);
    ui.heading("Skizzen");
    ui.add_space(4.0);
    if state.sketches.is_empty() {
        ui.weak("Noch keine Skizzen. Über „Erzeugen“ lässt sich eine auf einer Ebene anlegen.");
        return;
    }
    let open = state.sketch_session.as_ref().map(|s| s.sketch);
    let (mut select, mut edit) = (None, None);
    for (i, sketch) in state.sketches.iter().enumerate() {
        let selected = state.selected_sketch == Some(i) || open == Some(i);
        let name = if open == Some(i) {
            format!("{} (in Bearbeitung)", sketch.name)
        } else {
            sketch.name.clone()
        };
        let r = row(ui, selected, &name, paint_sketch_icon)
            .on_hover_text("Doppelklick öffnet die Skizze zum Bearbeiten.");
        if r.double_clicked() {
            edit = Some(i);
        } else if r.clicked() {
            select = Some(i);
        }
    }
    if let Some(i) = select {
        state.selected_sketch = Some(i);
    }
    if let Some(i) = edit {
        state.selected_sketch = Some(i);
        if open != Some(i) {
            let _ = state.open_sketch(i);
        }
    }
}
