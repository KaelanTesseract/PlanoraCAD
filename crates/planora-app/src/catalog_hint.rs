//! Einmaliger Hinweis beim ersten Start auf den Katalog der Zusatzmodule („Es gibt
//! Zusatzmodule für PlanoraCAD …“). Das Fenster rechnet nichts selbst; wann es erscheint und was
//! danach geschieht (Feld `catalog_hint_shown` speichern, Katalog öffnen), regelt die App
//! (`app::PlanoraApp::catalog_hint_tick`).

use crate::dialog::{Dialog, DialogSize, escape_pressed};
use crate::theme::{self, SPACE_2};
use planora_i18n::tr;

/// Kennung des Fensters.
const WINDOW_ID: &str = "planora_katalog_hinweis";

/// Titel des Hinweises.
pub fn title() -> String {
    tr!("catalog-hint-title")
}
/// Text des Hinweises.
pub fn text() -> String {
    tr!("catalog-hint-text")
}
/// Öffnet das Fenster „Zusatzmodule“ am Reiter Katalog.
pub fn open_label() -> String {
    tr!("catalog-hint-open")
}
/// Schließt den Hinweis; er kommt nicht wieder.
pub fn later_label() -> String {
    tr!("catalog-hint-later")
}

/// Wahl im Hinweis; beide Wahlen beenden ihn für immer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintAction {
    OpenCatalog,
    Later,
}

/// Der Hinweis in einem Bild; Esc und ✕ wirken wie „Später“.
pub fn show(ctx: &egui::Context, dark: bool) -> Option<HintAction> {
    let mut action = None;
    let mut open = true;
    Dialog::new(title(), DialogSize::Prompt)
        .id(egui::Id::new(WINDOW_ID))
        .fixed()
        .center_on_open()
        .open(&mut open)
        .show_with_footer(
            ctx,
            |ui| {
                ui.add(egui::Label::new(text()).wrap());
            },
            |ui| {
                ui.add_space(SPACE_2);
                ui.horizontal_wrapped(|ui| {
                    let r = ui.add(theme::primary_button(&open_label(), dark));
                    theme::focus_ring(ui, &r);
                    if r.clicked() {
                        action = Some(HintAction::OpenCatalog);
                    }
                    let r = ui.add(egui::Button::new(later_label()));
                    theme::focus_ring(ui, &r);
                    if r.clicked() {
                        action = Some(HintAction::Later);
                    }
                });
                if action.is_none() && escape_pressed(ui, true) {
                    action = Some(HintAction::Later);
                }
            },
        );
    if !open && action.is_none() {
        action = Some(HintAction::Later);
    }
    action
}
