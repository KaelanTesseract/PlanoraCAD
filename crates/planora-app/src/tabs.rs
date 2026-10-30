//! Tab-Leiste über der 3D-Ansicht.

use crate::icons::{self, Icon};
use crate::keymap::{self, Action};
use crate::theme;
use crate::workspace::Workspace;
use planora_i18n::tr;

/// Kennzeichen geänderter Tabs hinter dem Namen.
const DIRTY_MARK: char = '●';

/// Radius des gezeichneten Kennzeichens im Verhältnis zur Schriftgröße.
const DIRTY_MARK_RADIUS: f32 = 0.22;

/// Größe der Symbole „Schließen“ und „Neu“.
const ICON_SIZE: f32 = 14.0;

/// Symbolknopf ohne Rand (Fläche nur beim Überfahren).
fn icon_button(ui: &mut egui::Ui, icon: Icon) -> egui::Response {
    let response = ui.add(egui::Button::new(icons::text(
        icon,
        ICON_SIZE,
        egui::Color32::PLACEHOLDER,
    )));
    // Ohne Rand: der Tastaturfokus zeigt sich als Ring.
    theme::focus_ring(ui, &response);
    response
}

/// Tab-Leiste: Klick aktiviert, das Symbol „Schließen“ schließt (mit Rückfrage bei Änderungen),
/// „Neu“ legt einen neuen Tab an. Geänderte Tabs tragen „●“ hinter dem Namen. Aktiver Tab:
/// Fläche `bg_active`, Text `text_primary`; inaktive Tabs `text_secondary` ohne Fläche.
pub fn show(ui: &mut egui::Ui, ws: &mut Workspace) {
    show_revealing(ui, ws, false);
}

/// Wie [`show`]; mit `reveal_active` rollt eine umgebende ScrollArea den aktiven Tab (mit seinem
/// Schließen-Symbol) in den sichtbaren Bereich.
pub fn show_revealing(ui: &mut egui::Ui, ws: &mut Workspace, reveal_active: bool) {
    show_impl(ui, ws, reveal_active, true);
}

/// Nur die Tabs, ohne das Symbol „Neu“ (das steht dann mit [`new_tab_button`] außerhalb einer
/// rollenden Leiste und bleibt so immer sichtbar). `reveal_active` wie bei [`show_revealing`].
pub fn show_tabs_only(ui: &mut egui::Ui, ws: &mut Workspace, reveal_active: bool) {
    show_impl(ui, ws, reveal_active, false);
}

/// Tooltip des Symbols „Neu“ mit der Taste aus der Tastenbelegung.
fn new_tab_hint(ws: &Workspace) -> String {
    keymap::with_hint(&ws.keymap, Action::New, &Action::New.label())
}

/// Symbol „Neu“ (Neues Dokument) für sich; legt beim Klick einen neuen Tab an.
pub fn new_tab_button(ui: &mut egui::Ui, ws: &mut Workspace) -> egui::Response {
    let response = ui
        .scope(|ui| {
            theme::frameless(ui.style_mut());
            icon_button(ui, Icon::NewTab)
        })
        .inner
        .on_hover_text(new_tab_hint(ws));
    if response.clicked() {
        ws.new_tab();
    }
    response
}

fn show_impl(ui: &mut egui::Ui, ws: &mut Workspace, reveal_active: bool, with_new: bool) {
    let mut activate = None;
    let mut close = None;
    let mut add = false;
    let (close_hint, new_hint) = (
        keymap::with_hint(&ws.keymap, Action::Close, &tr!("tab-close")),
        new_tab_hint(ws),
    );
    ui.horizontal_wrapped(|ui| {
        theme::frameless(ui.style_mut());
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        let last = ws.tabs.len().saturating_sub(1);
        for (i, t) in ws.tabs.iter().enumerate() {
            let head = tab_label(ui, i == ws.active, &t.title(), t.is_dirty());
            if head.clicked() {
                activate = Some(i);
            }
            let close_button = icon_button(ui, Icon::Close).on_hover_text(&close_hint);
            if reveal_active && i == ws.active {
                ui.scroll_to_rect(head.rect.union(close_button.rect), None);
            }
            if close_button.clicked() {
                close = Some(i);
            }
            if with_new || i < last {
                ui.add_space(theme::SPACE_2);
            }
        }
        if with_new
            && icon_button(ui, Icon::NewTab)
                .on_hover_text(&new_hint)
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

/// Wählbarer Tab-Kopf „Name ●“. Auf das Zeichen „●“ der Schrift ist kein Verlass; der Text
/// enthält es trotzdem (für Bildschirmleser und Tests), es wird aber unsichtbar gesetzt und als
/// gefüllter Kreis an seiner Stelle gezeichnet.
fn tab_label(ui: &mut egui::Ui, selected: bool, title: &str, dirty: bool) -> egui::Response {
    if !dirty {
        let response = ui.selectable_label(selected, title);
        theme::focus_ring(ui, &response);
        return response;
    }
    let font = egui::TextStyle::Button.resolve(ui.style());
    let mut job = egui::text::LayoutJob::default();
    // Farbe `Color32::PLACEHOLDER` ersetzt egui beim Zeichnen durch die Textfarbe des Knopfs
    // (normal, überfahren, gewählt).
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
    // Der Kopf bricht nie um (einzeilig), sonst stünde der Kreis nicht auf dem Zeichen: er wird
    // vom rechten Rand und der Mitte des Kopfs aus gesetzt.
    let response = ui
        .scope(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.selectable_label(selected, job)
        })
        .inner;
    let visuals = ui.style().interact_selectable(&response, selected);
    let center = egui::pos2(
        response.rect.right() - ui.spacing().button_padding.x - mark_width / 2.0,
        response.rect.center().y,
    );
    ui.painter()
        .circle_filled(center, font.size * DIRTY_MARK_RADIUS, visuals.text_color());
    theme::focus_ring(ui, &response);
    response
}
