//! Tab-Leiste über der 3D-Ansicht.

use crate::workspace::Workspace;

/// Kennzeichen geänderter Tabs hinter dem Namen.
const DIRTY_MARK: char = '●';

/// Radius des gezeichneten Kennzeichens im Verhältnis zur Schriftgröße.
const DIRTY_MARK_RADIUS: f32 = 0.22;

/// Tab-Leiste: Klick aktiviert, „×“ schließt (mit Rückfrage bei Änderungen), „+“ legt einen
/// neuen Tab an. Geänderte Tabs tragen „●“ hinter dem Namen.
pub fn show(ui: &mut egui::Ui, ws: &mut Workspace) {
    let mut activate = None;
    let mut close = None;
    let mut add = false;
    ui.horizontal_wrapped(|ui| {
        for (i, t) in ws.tabs.iter().enumerate() {
            if tab_label(ui, i == ws.active, &t.title(), t.is_dirty()).clicked() {
                activate = Some(i);
            }
            if ui
                .small_button("×")
                .on_hover_text("Schließen (Strg+W)")
                .clicked()
            {
                close = Some(i);
            }
            ui.add_space(6.0);
        }
        if ui
            .small_button("+")
            .on_hover_text("Neues Dokument (Strg+N)")
            .clicked()
        {
            add = true;
        }
    });
    if let Some(i) = activate {
        ws.activate(i);
    }
    if let Some(i) = close {
        ws.request_close(i);
    }
    if add {
        ws.new_tab();
    }
}

/// Wählbarer Tab-Kopf „Name ●“. Die Oberflächenschrift (Roboto) hat kein Zeichen „●“; der Text
/// enthält es trotzdem (für Bildschirmleser und Tests), es wird aber unsichtbar gesetzt und als
/// gefüllter Kreis an seiner Stelle gezeichnet.
fn tab_label(ui: &mut egui::Ui, selected: bool, title: &str, dirty: bool) -> egui::Response {
    if !dirty {
        return ui.selectable_label(selected, title);
    }
    let font = egui::TextStyle::Button.resolve(ui.style());
    let mut job = egui::text::LayoutJob::default();
    // PLACEHOLDER: egui setzt die passende Textfarbe des Knopfs ein
    job.append(
        &format!("{title} "),
        0.0,
        egui::TextFormat::simple(font.clone(), egui::Color32::PLACEHOLDER),
    );
    job.append(
        &DIRTY_MARK.to_string(),
        0.0,
        egui::TextFormat::simple(font.clone(), egui::Color32::TRANSPARENT),
    );
    // Breite des (Ersatz-)Zeichens, um den Kreis mittig darauf zu setzen
    let mark_width = ui
        .painter()
        .layout_job(job.clone())
        .rows
        .last()
        .and_then(|r| r.glyphs.last())
        .map(|g| g.advance_width)
        .unwrap_or(font.size * 0.5);
    let response = ui.selectable_label(selected, job);
    let visuals = ui.style().interact_selectable(&response, selected);
    let center = egui::pos2(
        response.rect.right() - ui.spacing().button_padding.x - mark_width / 2.0,
        response.rect.center().y,
    );
    ui.painter()
        .circle_filled(center, font.size * DIRTY_MARK_RADIUS, visuals.text_color());
    response
}
