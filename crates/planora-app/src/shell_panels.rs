//! Schwebende Felder (Graphit, Etappe B): oben Menü und Tabs, links Projekte und Feature-Baum,
//! rechts Eigenschaften oder Versionen, links unten die Statuszeile und bei alten Versionen ein
//! schmaler Hinweis unter dem oberen Feld. Jedes Feld ist ein `egui::Area` an der Stelle aus
//! [`shell::layout`]; die Inhalte zeichnen die bisherigen Bereichsfunktionen.
//!
//! Breiten und Höhen kommen aus der Geometrie (keine Rückkopplung über die Feldgröße). Nur das
//! obere Feld und die Statuszeile messen ihre Wunschbreite; sie wird für das nächste Bild gemerkt.
//! Ändert sie sich, oder läuft gerade der unsichtbare erste Größen-Durchgang eines Felds, wird der
//! Durchgang verworfen und sofort wiederholt (wie bei `prompts::window`), damit kein Feld ein Bild
//! lang falsch steht.

use egui::{Vec2, vec2};

use crate::app::{self, PlanoraApp};
use crate::shell::{self, RightTab, ShellRects};
use crate::state::AppState;
use crate::{browser, projects_panel, properties, tabs, theme, versions_panel};

/// Breite, bis zu der die Tab-Leiste nicht umbricht (sie rollt waagerecht statt dessen).
const TABS_UNWRAPPED: f32 = 100_000.0;

fn top_id() -> egui::Id {
    egui::Id::new("planora-field-top")
}

fn left_id() -> egui::Id {
    egui::Id::new("planora-field-left")
}

fn right_id() -> egui::Id {
    egui::Id::new("planora-field-right")
}

fn status_id() -> egui::Id {
    egui::Id::new("planora-field-status")
}

fn banner_id() -> egui::Id {
    egui::Id::new("planora-field-banner")
}

/// Gemerkte Wunschbreite des oberen Felds (außen).
fn top_width_key() -> egui::Id {
    top_id().with("wanted")
}

/// Gemerkte Breite der Menüknöpfe im oberen Feld.
fn menu_width_key() -> egui::Id {
    top_id().with("menu")
}

/// Gemerkte Wunschbreite der Statuszeile (außen).
fn status_width_key() -> egui::Id {
    status_id().with("wanted")
}

/// Gemerkte Wunschbreite des unteren Befehlsfelds (Aufgabe 5).
fn bottom_width_key() -> egui::Id {
    egui::Id::new("planora-field-bottom").with("wanted")
}

fn stored(ctx: &egui::Context, key: egui::Id) -> Option<f32> {
    ctx.data(|d| d.get_temp::<f32>(key))
}

/// Merkt sich eine gemessene Breite; weicht sie von der zuletzt gemerkten ab, wird der Durchgang
/// verworfen und mit der neuen Breite wiederholt.
fn remember_width(ui: &egui::Ui, key: egui::Id, width: f32) {
    let old = stored(ui.ctx(), key);
    ui.ctx().data_mut(|d| d.insert_temp(key, width));
    if old.is_some_and(|o| (o - width).abs() > 0.5) {
        ui.ctx().request_discard("Feld: Breite geändert");
    }
}

/// Innenmaß eines Felds mit Rahmen `frame` und Außenmaß `outer` (Rand und Strich abgezogen).
fn inner_size(frame: &egui::Frame, outer: Vec2) -> Vec2 {
    (outer - frame.total_margin().sum()).max(Vec2::ZERO)
}

/// Rahmen einer einzeiligen Leiste der Höhe `height`: wie [`theme::panel_frame`], der Rand oben
/// und unten so, dass genau eine Zeile ([`theme::ROW_H`]) hineinpasst.
fn bar_frame(dark: bool, height: f32) -> egui::Frame {
    let frame = theme::panel_frame(dark);
    let v = ((height - 2.0 * frame.stroke.width - theme::ROW_H) / 2.0).max(0.0);
    frame.inner_margin(egui::Margin::symmetric(theme::SPACE_3 as i8, v as i8))
}

/// Ein Feld: Area an `pos` mit Rahmen `frame`. Der erste, unsichtbare Größen-Durchgang eines
/// neuen Felds wird verworfen, damit es schon im ersten Bild sichtbar an seiner Stelle steht.
fn field<R>(
    ctx: &egui::Context,
    id: egui::Id,
    pos: egui::Pos2,
    frame: egui::Frame,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    theme::field_area(id, pos)
        .show(ctx, |ui| {
            if ui.is_sizing_pass() {
                ui.ctx().request_discard("Feld: erste Größenbestimmung");
            }
            frame.show(ui, add_contents).inner
        })
        .inner
}

/// Zeichnet alle Felder (Area-basiert) und liefert die freie Fläche; Reihenfolge: Felder vor dem
/// Viewport-Aufruf des Aufrufers.
pub fn show_fields(ctx: &egui::Context, app: &mut PlanoraApp) -> egui::Rect {
    let content = ctx.content_rect();
    let read_only = app.ws.active_tab().read_only;
    // Vor der ersten Messung: oben so breit wie möglich, Status so breit wie die Spalte.
    let top_w = stored(ctx, top_width_key()).unwrap_or(content.width());
    let status_w = stored(ctx, status_width_key()).unwrap_or(theme::PANEL_WIDTH);
    let bottom_w = stored(ctx, bottom_width_key()).unwrap_or(0.0);
    let rects = shell::layout(content, &app.shell, top_w, bottom_w, status_w, read_only);
    top_field(ctx, &rects, app);
    left_field(ctx, &rects, app);
    right_field(ctx, &rects, app);
    bottom_field(ctx, &rects, app);
    status_field(ctx, &rects, app);
    banner_field(ctx, &rects, app);
    rects.free
}

/// Menü + Trennstrich + Tabs (Tabs in waagerechter ScrollArea). Solange eine Rückfrage offen ist,
/// sind Menü und Tabs gesperrt (sonst könnte z. B. „Speichern“ die Rückfrage ersetzen).
fn top_field(ctx: &egui::Context, rects: &ShellRects, app: &mut PlanoraApp) {
    let dark = app.ws.active_tab().state.dark;
    let frame = bar_frame(dark, shell::TOP_H);
    let inner = inner_size(&frame, rects.top.size());
    let margin = frame.total_margin().sum().x;
    let no_prompt = app.ws.prompt.is_none();
    let menu_w = stored(ctx, menu_width_key()).unwrap_or(inner.x);
    field(ctx, top_id(), rects.top.min, frame, |ui| {
        ui.set_width(inner.x);
        ui.set_height(inner.y);
        ui.horizontal_centered(|ui| {
            let menu = ui.allocate_ui(vec2(menu_w.min(ui.available_width()), inner.y), |ui| {
                ui.add_enabled_ui(no_prompt, |ui| {
                    app::menu_bar_shell_measured(ui, &mut app.ws, &mut app.shell)
                })
                .inner
            });
            let (open_projects, menu_natural) = menu.inner;
            if open_projects {
                // „Öffnen …“: Projekte einblenden und den Baum neu lesen.
                app.shell.show_projects();
                app.projects_ui.invalidate();
            }
            ui.separator();
            let tabs = egui::ScrollArea::horizontal()
                .id_salt("planora-field-tabs")
                .auto_shrink([true, true])
                .show(ui, |ui| {
                    ui.set_max_width(TABS_UNWRAPPED);
                    ui.add_enabled_ui(no_prompt, |ui| tabs::show(ui, &mut app.ws));
                });
            // Wunschbreite: Menüknöpfe, Trennstrich mit Abständen, alle Tabs, Rand.
            let gap = tabs.inner_rect.left() - menu.response.rect.right();
            remember_width(ui, menu_width_key(), menu_natural);
            remember_width(
                ui,
                top_width_key(),
                menu_natural + gap + tabs.content_size.x + margin,
            );
        });
    });
}

/// Projekte (wenn sichtbar) + Feature-Baum in einer senkrechten ScrollArea, höchstens so hoch
/// wie die Spalte. Schreibgeschützt ist der Baum gesperrt.
fn left_field(ctx: &egui::Context, rects: &ShellRects, app: &mut PlanoraApp) {
    let dark = app.ws.active_tab().state.dark;
    let frame = theme::panel_frame(dark);
    let inner = inner_size(&frame, rects.left.size());
    let read_only = app.ws.active_tab().read_only;
    field(ctx, left_id(), rects.left.min, frame, |ui| {
        // Feste Breite und Höchsthöhe aus der Geometrie, nicht aus der Feldgröße des letzten
        // Bildes (das gäbe eine Rückkopplung).
        ui.set_width(inner.x);
        ui.set_max_height(inner.y);
        egui::ScrollArea::vertical()
            .id_salt("planora-field-left-scroll")
            .max_height(inner.y)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if app.shell.projects_visible {
                    theme::section_header(ui, "Projekte", theme::HEADER_H);
                    ui.add_space(theme::SPACE_1);
                    projects_panel::show(ui, &mut app.ws, &mut app.projects_ui);
                    ui.add_space(theme::SPACE_3);
                }
                let state = &mut app.ws.active_tab_mut().state;
                browser::show_with(ui, state, read_only);
            });
    });
}

/// Umschalter Eigenschaften | Versionen (nur wenn Versionen eingeblendet sind) und darunter der
/// Inhalt in einer senkrechten ScrollArea. Schreibgeschützt sind die Eigenschaften gesperrt.
fn right_field(ctx: &egui::Context, rects: &ShellRects, app: &mut PlanoraApp) {
    let dark = app.ws.active_tab().state.dark;
    let frame = theme::panel_frame(dark);
    let inner = inner_size(&frame, rects.right.size());
    let enabled = !app.ws.active_tab().read_only;
    field(ctx, right_id(), rects.right.min, frame, |ui| {
        ui.set_width(inner.x);
        ui.set_max_height(inner.y);
        if app.shell.versions_visible {
            let mut tab = app.shell.effective_right_tab();
            ui.horizontal(|ui| {
                theme::frameless(ui.style_mut());
                for (which, label) in [
                    (RightTab::Properties, "Eigenschaften"),
                    (RightTab::Versions, "Versionen"),
                ] {
                    let r = ui.selectable_label(tab == which, label);
                    theme::focus_ring(ui, &r);
                    if r.clicked() {
                        tab = which;
                    }
                }
            });
            app.shell.right_tab = tab;
            ui.add_space(theme::SPACE_2);
        }
        let tab = app.shell.effective_right_tab();
        egui::ScrollArea::vertical()
            .id_salt(("planora-field-right-scroll", tab == RightTab::Versions))
            .max_height(ui.available_height())
            .auto_shrink([false, true])
            .show(ui, |ui| match tab {
                RightTab::Properties => {
                    let state = &mut app.ws.active_tab_mut().state;
                    ui.add_enabled_ui(enabled, |ui| properties::show(ui, state));
                }
                RightTab::Versions => {
                    versions_panel::show(ui, &mut app.ws, &mut app.versions_ui);
                }
            });
    });
}

/// Unteres Befehlsfeld: das Rechteck ist in der Geometrie reserviert; Aufgabe 5 füllt es.
fn bottom_field(_ctx: &egui::Context, _rects: &ShellRects, _app: &mut PlanoraApp) {}

/// Text und Farbe der Statuszeile. Vorrang: letzte Meldung (Fehlerfarbe), Skizzen-Modus
/// (text_secondary), erstes Feature mit Fehler (Fehlerfarbe), Körperzahl (mit Hinweis bei
/// ungenauem Netz in der Warnfarbe, sonst text_secondary).
fn status_line(state: &AppState, dark: bool) -> (String, egui::Color32) {
    let p = theme::palette(dark);
    match (
        &state.message,
        state.session_sketch(),
        browser::status_text(state),
    ) {
        (Some(m), _, _) => (m.clone(), p.error),
        (None, Some(sk), _) => (
            format!(
                "Skizzen-Modus: {} – Esc bricht ab, Eingabe beendet, Entf löscht, mittlere Maustaste oder Umschalt + Ziehen verschiebt",
                sk.name
            ),
            p.text_secondary,
        ),
        (None, None, Some(e)) => (e, p.error),
        (None, None, None) => match state.mesh_note() {
            Some(note) => (format!("{} Körper – {note}", state.bodies.len()), p.warning),
            None => (format!("{} Körper", state.bodies.len()), p.text_secondary),
        },
    }
}

/// Einzeiliger Text, senkrecht mittig; zu lange Texte enden mit „…“ und zeigen sich ganz beim
/// Überfahren.
fn one_line(ui: &mut egui::Ui, text: egui::RichText) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add(egui::Label::new(text).truncate());
    });
}

/// Statuszeile links unten; ihre Breite folgt dem Text (höchstens die Spaltenbreite).
fn status_field(ctx: &egui::Context, rects: &ShellRects, app: &mut PlanoraApp) {
    let state = &app.ws.active_tab().state;
    let dark = state.dark;
    let (text, color) = status_line(state, dark);
    let frame = bar_frame(dark, shell::STATUS_H);
    let inner = inner_size(&frame, rects.status.size());
    let margin = frame.total_margin().sum().x;
    field(ctx, status_id(), rects.status.min, frame, |ui| {
        ui.set_width(inner.x);
        ui.set_height(inner.y);
        let text = egui::RichText::new(text).color(color);
        let natural = egui::WidgetText::from(text.clone())
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Body,
            )
            .size()
            .x;
        remember_width(ui, status_width_key(), natural.ceil() + margin);
        one_line(ui, text);
    });
}

/// Hinweis unter dem oberen Feld, nur in schreibgeschützten Ansichten alter Versionen.
fn banner_field(ctx: &egui::Context, rects: &ShellRects, app: &mut PlanoraApp) {
    let tab = app.ws.active_tab();
    let Some(rect) = rects.banner.filter(|_| tab.read_only) else {
        return;
    };
    let dark = tab.state.dark;
    let n = tab.based_on.unwrap_or_default();
    let frame = bar_frame(dark, shell::BANNER_H);
    let inner = inner_size(&frame, rect.size());
    field(ctx, banner_id(), rect.min, frame, |ui| {
        ui.set_width(inner.x);
        ui.set_height(inner.y);
        one_line(
            ui,
            egui::RichText::new(format!(
                "Alte Version v{n} – schreibgeschützt. Im Bereich „Versionen“ lässt sie sich wiederherstellen oder abzweigen."
            ))
            .color(theme::palette(dark).warning),
        );
    });
}
