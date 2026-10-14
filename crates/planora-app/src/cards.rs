//! Karten nebeneinander über die ganze Breite, alle gleich hoch (Nutzervorgabe).

/// Höhenunterschied in Punkten, ab dem die gemerkte Kartenhöhe neu gesetzt wird (darunter Rundung).
const HEIGHT_SLACK: f32 = 0.5;

/// Zeichnet `cards` nebeneinander in gleich breiten Spalten über die volle Breite. Alle Karten
/// bekommen die Höhe der höchsten; sie wird aus dem vorigen Bild gemerkt, nach einer Änderung
/// gleicht das nächste Bild sie an. Liefert die Rechtecke der Karten.
pub fn card_row(
    ui: &mut egui::Ui,
    id_salt: &str,
    cards: &mut [&mut dyn FnMut(&mut egui::Ui)],
) -> Vec<egui::Rect> {
    let mut rects = Vec::with_capacity(cards.len());
    if cards.is_empty() {
        return rects;
    }
    let id = ui.id().with(("card_row", id_salt));
    let min_height: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    let mut tallest = 0.0f32;
    ui.columns(cards.len(), |columns| {
        for (column, card) in columns.iter_mut().zip(cards.iter_mut()) {
            let inner = egui::Frame::group(column.style()).show(column, |ui| {
                ui.set_width(ui.available_width());
                (*card)(ui);
                let natural = ui.min_rect().height();
                // Auffüllen bis zur gemerkten Höhe (`set_min_height` nach dem Inhalt würde ab dem Ende
                // des Inhalts zählen und die Karte zu hoch machen).
                ui.allocate_space(egui::vec2(0.0, (min_height - natural).max(0.0)));
                natural
            });
            tallest = tallest.max(inner.inner);
            rects.push(inner.response.rect);
        }
    });
    if (tallest - min_height).abs() > HEIGHT_SLACK {
        ui.data_mut(|d| d.insert_temp(id, tallest));
        ui.ctx().request_repaint();
    }
    rects
}
