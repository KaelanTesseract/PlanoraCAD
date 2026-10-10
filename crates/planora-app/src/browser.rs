use crate::icons::{paint_body_icon, paint_sketch_icon};
use crate::state::{AppState, SketchRequest};
use egui::{Sense, vec2};
use planora_feature::FeatureId;

const ROW_HEIGHT: f32 = 28.0;

/// Befehl aus dem Kontextmenü einer Skizzenzeile.
enum SketchAction {
    Extrude(FeatureId),
    Revolve(FeatureId),
    Delete(FeatureId),
}

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
        state.selected_feature = None;
    }

    ui.add_space(12.0);
    ui.heading("Skizzen");
    ui.add_space(4.0);
    // Bis zum Feature-Baum: die Skizzen-Features der Zeitleiste.
    let sketches = state.sketch_features();
    if sketches.is_empty() {
        ui.weak("Noch keine Skizzen. Über „Erzeugen“ lässt sich eine auf einer Ebene anlegen.");
        return;
    }
    let open = state.open_sketch_id();
    let (mut select, mut edit, mut action) = (None, None, None);
    for (i, sketch_name) in sketches {
        let selected = state.selected_feature == Some(i) || open == Some(i);
        let name = if open == Some(i) {
            format!("{sketch_name} (in Bearbeitung)")
        } else {
            sketch_name
        };
        let r = row(ui, selected, &name, paint_sketch_icon).on_hover_text(
            "Doppelklick öffnet die Skizze zum Bearbeiten; Rechtsklick zeigt weitere Befehle.",
        );
        if r.double_clicked() {
            edit = Some(i);
        } else if r.clicked() {
            select = Some(i);
        }
        r.context_menu(|ui| {
            if ui.button("Bearbeiten").clicked() {
                edit = Some(i);
                ui.close();
            }
            if ui.button("Extrudieren …").clicked() {
                action = Some(SketchAction::Extrude(i));
                ui.close();
            }
            if ui.button("Drehen …").clicked() {
                action = Some(SketchAction::Revolve(i));
                ui.close();
            }
            ui.separator();
            if ui.button("Löschen").clicked() {
                action = Some(SketchAction::Delete(i));
                ui.close();
            }
        });
    }
    match action {
        // Körper entstehen außerhalb des Skizzen-Modus: die offene Skizze wird vorher beendet.
        Some(SketchAction::Extrude(i)) => state.request_sketch_dialog(SketchRequest::Extrude(i)),
        Some(SketchAction::Revolve(i)) => state.request_sketch_dialog(SketchRequest::Revolve(i)),
        Some(SketchAction::Delete(i)) => {
            // Darauf aufbauende Features nennt die Meldung; gelöscht wird dann nichts.
            let _ = state.delete_feature(i);
            return;
        }
        None => {}
    }
    if let Some(i) = select.or(edit) {
        // Nur eines ist hervorgehoben: Skizze statt Körper bzw. 3D-Auswahl.
        state.selected_feature = Some(i);
        state.selected_body = None;
        state.set_selection(None);
    }
    if let Some(i) = edit
        && open != Some(i)
    {
        let _ = state.open_sketch(i);
    }
}
