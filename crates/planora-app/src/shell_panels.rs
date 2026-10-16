//! Schwebende Felder (Graphit, Etappe B): oben Menü und Tabs, links Projekte und Feature-Baum,
//! rechts Eigenschaften oder Versionen, links unten die Statuszeile und bei alten Versionen ein
//! schmaler Hinweis unter dem oberen Feld. Jedes Feld ist ein `egui::Area` an der Stelle aus
//! [`shell::layout`]; die Inhalte zeichnen die bisherigen Bereichsfunktionen.
//!
//! Breiten und Höhen kommen aus der Geometrie (keine Rückkopplung über die Feldgröße). Status und
//! Hinweis werden vorab gesetzt (bis zu zwei Zeilen, der Status wächst nach oben). Nur das obere
//! Feld misst seine Wunschbreite; sie wird für das nächste Bild gemerkt. Ändert sie sich, oder
//! läuft gerade der unsichtbare erste Größen-Durchgang eines Felds, wird der Durchgang verworfen
//! und sofort wiederholt (wie bei `prompts::window`), damit kein Feld ein Bild lang falsch steht.
//!
//! Das Befehlsfeld unten lässt dem Status die Breite, die dessen Text in zwei Zeilen braucht:
//! Es wird bei langen Meldungen schmaler (der Inhalt rollt), aber nie unter die drei Chips mit
//! Trennstrich. Reicht der Rest dem Status nicht, bricht er in bis zu drei Zeilen um.

use egui::{Vec2, vec2};

use crate::app::{self, CreateKind, PlanoraApp};
use crate::shell::{self, RightTab, ShellRects};
use crate::state::AppState;
use crate::{browser, icons, projects_panel, properties, tabs, theme, versions_panel};
use planora_kernel::BoolOp;

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

fn bottom_id() -> egui::Id {
    egui::Id::new("planora-field-bottom")
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

/// Gemerkte Wunschbreite des unteren Befehlsfelds (Aufgabe 5).
fn bottom_width_key() -> egui::Id {
    bottom_id().with("wanted")
}

/// Gemerkte Mindestbreite des Befehlsfelds (drei Chips, Trennstrich, Rand).
fn bottom_min_key() -> egui::Id {
    bottom_id().with("min")
}

fn stored(ctx: &egui::Context, key: egui::Id) -> Option<f32> {
    ctx.data(|d| d.get_temp::<f32>(key))
}

/// Verwirft den Durchgang. Lehnt egui das ab (höchstens `max_passes` Durchgänge je Bild), wird
/// wenigstens sofort ein neues Bild angefordert, damit kein veraltetes Bild stehen bleibt.
#[doc(hidden)]
pub fn discard(ctx: &egui::Context, reason: &'static str) {
    ctx.request_discard(reason);
    if !ctx.will_discard() {
        ctx.request_repaint();
    }
}

/// Merkt sich eine gemessene Breite; weicht sie von der zuletzt gemerkten ab, wird der Durchgang
/// verworfen und mit der neuen Breite wiederholt.
fn remember_width(ui: &egui::Ui, key: egui::Id, width: f32) {
    let old = stored(ui.ctx(), key);
    ui.ctx().data_mut(|d| d.insert_temp(key, width));
    if old.is_some_and(|o| (o - width).abs() > 0.5) {
        discard(ui.ctx(), "Feld: Breite geändert");
    }
}

/// Aktiver Tab und Tabzahl, zu denen die Tab-Leiste zuletzt gerollt wurde.
fn revealed_key() -> egui::Id {
    top_id().with("revealed")
}

/// Höchstzahl der Zeilen in Status und Hinweis; erst darüber endet der Text mit „…“.
const MAX_ROWS: usize = 2;

/// Höchstzahl der Zeilen im Status: bei schmalem Platz (Befehlsfeld daneben) bis zu drei.
const STATUS_ROWS: usize = 3;

/// Bricht `text` (Textstil `Body`) bei `max_width` um, höchstens [`MAX_ROWS`] Zeilen.
fn wrapped(
    ctx: &egui::Context,
    text: &str,
    color: egui::Color32,
    max_width: f32,
    rows: usize,
) -> std::sync::Arc<egui::Galley> {
    let font = egui::TextStyle::Body.resolve(&ctx.global_style());
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat::simple(font, color),
    );
    job.wrap = egui::text::TextWrapping {
        max_width,
        max_rows: rows,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    ctx.fonts_mut(|f| f.layout_job(job))
}

/// Breite von `text` in einer Zeile (Textstil `Body`).
fn natural_width(ctx: &egui::Context, text: &str) -> f32 {
    wrapped(
        ctx,
        text,
        egui::Color32::PLACEHOLDER,
        f32::INFINITY,
        MAX_ROWS,
    )
    .size()
    .x
    .ceil()
}

/// Schmalste Breite, in der `text` ohne Kürzung in höchstens [`MAX_ROWS`] Zeilen passt.
fn two_row_width(ctx: &egui::Context, text: &str) -> f32 {
    let full = natural_width(ctx, text);
    let (mut lo, mut hi) = (0.0_f32, full);
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        if wrapped(ctx, text, egui::Color32::PLACEHOLDER, mid, MAX_ROWS).elided {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi.ceil()
}

/// Feld mit Text in bis zu zwei Zeilen: mindestens `min_h` hoch, sonst so hoch wie der Text.
/// `(grow_up, rows)`: höchstens `rows` Zeilen; `grow_up`: die Unterkante von `rect` bleibt fest (Status), sonst die Oberkante (Hinweis).
/// Wird der Text gekürzt, zeigt er sich beim Überfahren ganz.
fn text_field(
    ctx: &egui::Context,
    id: egui::Id,
    rect: egui::Rect,
    min_h: f32,
    (grow_up, rows): (bool, usize),
    dark: bool,
    (text, color): (String, egui::Color32),
) {
    let frame = bar_frame(dark, min_h);
    let margin = frame.total_margin().sum();
    let inner_w = (rect.width() - margin.x).max(0.0);
    let galley = wrapped(ctx, &text, color, inner_w, rows);
    let h = min_h.max(galley.size().y + margin.y);
    let y = if grow_up {
        rect.bottom() - h
    } else {
        rect.top()
    };
    field(ctx, id, egui::pos2(rect.left(), y), frame, |ui| {
        ui.set_width(inner_w);
        ui.set_height(h - margin.y);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let (r, response) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
            let enabled = ui.is_enabled();
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, &text));
            let elided = galley.elided;
            ui.painter().galley(r.min, galley, color);
            if elided {
                response.on_hover_text(text);
            }
        });
    });
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
                discard(ui.ctx(), "Feld: erste Größenbestimmung");
            }
            frame.show(ui, add_contents).inner
        })
        .inner
}

/// Zeichnet alle Felder (Area-basiert) und liefert die freie Fläche; Reihenfolge: Felder vor dem
/// Viewport-Aufruf des Aufrufers.
pub fn show_fields(ctx: &egui::Context, app: &mut PlanoraApp) -> egui::Rect {
    let content = ctx.content_rect();
    // Vor der ersten Messung: oben so breit wie möglich.
    let top_w = stored(ctx, top_width_key()).unwrap_or(content.width());
    let layout = |app: &PlanoraApp| {
        // Der Status ist so breit wie sein Text in einer Zeile (die Geometrie begrenzt ihn).
        let tab = app.ws.active_tab();
        let status = status_line(&tab.state, tab.state.dark);
        let margin = bar_frame(tab.state.dark, shell::STATUS_H)
            .total_margin()
            .sum()
            .x;
        let status_w = natural_width(ctx, &status.0) + margin;
        // Das Befehlsfeld lässt dem Status die Breite, die sein Text in zwei Zeilen braucht
        // (Meldungen und Hinweise bleiben ganz lesbar); reicht der Platz nicht, rollt das Feld.
        let status_need = two_row_width(ctx, &status.0) + margin;
        let bottom_cap = content.width() - 2.0 * (shell::SHELL_INSET + shell::GAP + status_need);
        // Das Feld bleibt immer da: mindestens die drei Chips mit Trennstrich; der Status nimmt
        // den Rest links davon (bis zu drei Zeilen, sonst gekürzt mit vollem Text beim Überfahren).
        let bottom_w = match stored(ctx, bottom_width_key()) {
            Some(wanted) => wanted
                .min(bottom_cap)
                .max(stored(ctx, bottom_min_key()).unwrap_or(0.0)),
            None => 0.0,
        };
        let rects = shell::layout(
            content,
            &app.shell,
            top_w,
            bottom_w,
            status_w,
            tab.read_only,
        );
        (rects, status, tab.state.dark)
    };
    let (rects, _, _) = layout(app);
    top_field(ctx, &rects, app);
    left_field(ctx, &rects, app);
    right_field(ctx, &rects, app);
    bottom_field(ctx, &rects, app);
    // Die Felder können den Tab gewechselt oder eine Meldung gesetzt haben: Status und Hinweis
    // zeigen schon den neuen Stand.
    let (rects, status, dark) = layout(app);
    text_field(
        ctx,
        status_id(),
        rects.status,
        shell::STATUS_H,
        (true, STATUS_ROWS),
        dark,
        status,
    );
    banner_field(ctx, &rects, app);
    ctx.data_mut(|d| d.insert_temp(layout_key(), rects));
    rects.free
}

fn layout_key() -> egui::Id {
    egui::Id::new("planora-field-layout")
}

/// Geometrie der Felder aus dem letzten Bild (für Tests).
#[doc(hidden)]
pub fn last_layout(ctx: &egui::Context) -> Option<ShellRects> {
    ctx.data(|d| d.get_temp(layout_key()))
}

/// Tatsächlich belegte Rechtecke der sichtbaren Felder im letzten Bild, je mit Namen (für
/// Tests; die Seitenfelder sind so hoch wie ihr Inhalt, höchstens wie die Spalte).
#[doc(hidden)]
pub fn field_rects(ctx: &egui::Context) -> Vec<(&'static str, egui::Rect)> {
    [
        ("oben", top_id()),
        ("links", left_id()),
        ("rechts", right_id()),
        ("unten", bottom_id()),
        ("Status", status_id()),
        ("Hinweis", banner_id()),
    ]
    .into_iter()
    .filter(|(_, id)| {
        ctx.memory(|m| {
            m.areas()
                .is_visible(&egui::LayerId::new(egui::Order::Middle, *id))
        })
    })
    .filter_map(|(name, id)| ctx.memory(|m| m.area_rect(id)).map(|r| (name, r)))
    .collect()
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
    // Den aktiven Tab nur dann ins Bild rollen, wenn er oder die Tabzahl gewechselt hat (sonst
    // ließe sich die Leiste nicht frei rollen).
    let shown = (app.ws.active_tab().id, app.ws.tabs.len());
    let reveal = ctx.data(|d| d.get_temp::<(u64, usize)>(revealed_key())) != Some(shown);
    field(ctx, top_id(), rects.top.min, frame, |ui| {
        ui.set_width(inner.x);
        ui.set_height(inner.y);
        ui.horizontal_centered(|ui| {
            let menu = ui.allocate_ui(vec2(menu_w.min(ui.available_width()), inner.y), |ui| {
                ui.add_enabled_ui(no_prompt, |ui| {
                    app::menu_bar_measured(ui, &mut app.ws, &mut app.shell)
                })
                .inner
            });
            let (open_projects, menu_natural) = menu.inner;
            if open_projects {
                // „Öffnen …“: Projekte einblenden und den Baum neu lesen.
                app.show_projects();
            }
            ui.separator();
            let tabs = egui::ScrollArea::horizontal()
                .id_salt("planora-field-tabs")
                .auto_shrink([true, true])
                .show(ui, |ui| {
                    ui.set_max_width(TABS_UNWRAPPED);
                    ui.add_enabled_ui(no_prompt, |ui| {
                        tabs::show_revealing(ui, &mut app.ws, reveal)
                    });
                });
            // Wunschbreite: Menüknöpfe, Trennstrich mit Abständen, alle Tabs, Rand.
            let gap = tabs.inner_rect.left() - menu.response.rect.right();
            remember_width(ui, menu_width_key(), menu_natural);
            remember_width(
                ui,
                top_width_key(),
                menu_natural + gap + tabs.content_size.x + margin,
            );
            // Erst als erledigt merken, wenn dieser Durchgang gilt (sonst rollt der nächste).
            if !ui.ctx().will_discard() {
                ui.ctx().data_mut(|d| d.insert_temp(revealed_key(), shown));
            }
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
        // Mit Umschalter steht der Name des Bereichs dort; ohne ihn zeigt der Inhalt seinen Kopf.
        let switch = app.shell.versions_visible;
        if switch {
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
                    ui.add_enabled_ui(enabled, |ui| {
                        if switch {
                            properties::show_content(ui, state);
                        } else {
                            properties::show(ui, state);
                        }
                    });
                }
                RightTab::Versions => {
                    versions_panel::show_with_header(
                        ui,
                        &mut app.ws,
                        &mut app.versions_ui,
                        !switch,
                    );
                }
            });
    });
}

/// Was ein Knopf des Befehlsfelds auslöst; dieselben Funktionen wie im Menü.
#[derive(Clone, Copy)]
enum Command {
    Boolean(BoolOp),
    Create(CreateKind),
    /// `true`: Fase, sonst Verrunden.
    Blend(bool),
}

/// Warum das Befehlsfeld gerade ganz gesperrt ist (wie das Menü).
fn bottom_lock(ws: &crate::workspace::Workspace) -> Option<&'static str> {
    let tab = ws.active_tab();
    if ws.prompt.is_some() {
        Some("Zuerst die offene Rückfrage beantworten.")
    } else if tab.read_only {
        Some("In einer alten Version lässt sich nichts ändern.")
    } else {
        None
    }
}

fn buttons_key() -> egui::Id {
    bottom_id().with("buttons")
}

/// Beschriftung und Lage aller Knöpfe des Befehlsfelds im letzten Bild (für Tests). Beschriftung
/// wie im Menü, bei den Chips der Name ohne „ …“.
#[doc(hidden)]
pub fn bottom_buttons(ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
    ctx.data(|d| d.get_temp::<Vec<(String, egui::Rect)>>(buttons_key()))
        .unwrap_or_default()
}

/// Chip in den Farben `(Fläche, Schrift)` aus der Palette; beim Überfahren und Drücken wird die
/// Fläche zur Schriftfarbe hin heller bzw. kräftiger ([`theme::chip_fill`]).
fn chip(
    ui: &mut egui::Ui,
    label: &str,
    (fill, ink): (egui::Color32, egui::Color32),
) -> egui::Response {
    let enabled = ui.is_enabled();
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::TextStyle::Button.resolve(ui.style()),
        ink,
    );
    let size = vec2(galley.size().x + 2.0 * theme::SPACE_2, theme::ROW_H);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if ui.is_rect_visible(rect) {
        let fade = |c: egui::Color32| if enabled { c } else { c.gamma_multiply(0.45) };
        let shown = theme::chip_fill(
            fill,
            ink,
            enabled && response.hovered(),
            enabled && response.is_pointer_button_down_on(),
        );
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(theme::RADIUS_MD),
            fade(shown),
        );
        let pos = rect.center() - galley.size() / 2.0;
        ui.painter().galley(pos, galley, fade(ink));
    }
    theme::focus_ring(ui, &response);
    response
}

/// Gesperrter Knopf: Beschriftung und Grund beim Überfahren.
fn explain(response: egui::Response, label: &str, reason: &'static str) -> egui::Response {
    response.on_disabled_hover_ui(|ui| {
        ui.strong(label);
        ui.label(reason);
    })
}

/// Unteres Befehlsfeld: Abkürzungen zu Menüaktionen (Boolesche Operationen als Chips, dann die
/// Erzeugen- und Bearbeiten-Aktionen als Symbolknöpfe). Jeder Knopf ruft dieselbe Funktion wie
/// der Menüeintrag und ist wie das Menü gesperrt (alte Version, offene Rückfrage); im Skizzen-Modus
/// beendet er die Skizze wie das Menü. Zu schmal für alle Knöpfe, rollt der Inhalt waagerecht.
fn bottom_field(ctx: &egui::Context, rects: &ShellRects, app: &mut PlanoraApp) {
    let dark = app.ws.active_tab().state.dark;
    let p = theme::palette(dark);
    let frame = bar_frame(dark, shell::BOTTOM_H);
    let inner = inner_size(&frame, rects.bottom.size());
    let margin = frame.total_margin().sum().x;
    let lock = bottom_lock(&app.ws);
    let boolean_blocked = app.ws.active_tab().state.boolean_blocked();
    let blend_blocked = app.ws.active_tab().state.blend_blocked();
    let mut command = None;
    let mut seen: Vec<(String, egui::Rect)> = Vec::new();
    let mut chips_end = 0.0;
    field(ctx, bottom_id(), rects.bottom.min, frame, |ui| {
        ui.set_width(inner.x);
        ui.set_height(inner.y);
        let scroll = egui::ScrollArea::horizontal()
            .id_salt("planora-field-bottom-scroll")
            .auto_shrink([true, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.set_max_width(TABS_UNWRAPPED);
                ui.set_height(inner.y);
                // Eng gesetzt, damit alle Knöpfe schon bei 1100 px Fensterbreite sichtbar sind.
                ui.spacing_mut().item_spacing.x = theme::SPACE_1;
                ui.spacing_mut().button_padding.x = theme::SPACE_1;
                let start = ui.cursor().left();
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    for (label, op, colors) in [
                        ("Vereinigen", BoolOp::Union, p.chip_bridge),
                        ("Abziehen", BoolOp::Difference, p.chip_split),
                        ("Schneiden", BoolOp::Intersection, p.chip_boolean),
                    ] {
                        let reason = lock.or(boolean_blocked);
                        let r = ui
                            .add_enabled_ui(reason.is_none(), |ui| chip(ui, label, colors))
                            .inner;
                        let r = match reason {
                            Some(why) => explain(r, label, why),
                            None => r.on_hover_text(format!("{label} …")),
                        };
                        if r.clicked() {
                            command = Some(Command::Boolean(op));
                        }
                        seen.push((label.to_owned(), r.rect));
                    }
                    ui.separator();
                    chips_end = ui.cursor().left() - start;
                    let blend = icons::IconSource::Feature(icons::FeatureIcon::Blend);
                    let mut items: Vec<(&str, icons::IconSource, Command, Option<&'static str>)> =
                        CreateKind::ALL
                            .iter()
                            .map(|k| {
                                let own = app::create_blocked(&app.ws, *k);
                                (k.label(), k.icon(), Command::Create(*k), own)
                            })
                            .collect();
                    items.push(("Verrunden …", blend, Command::Blend(false), blend_blocked));
                    items.push(("Fase …", blend, Command::Blend(true), blend_blocked));
                    for (i, (label, source, cmd, own)) in items.into_iter().enumerate() {
                        if i == CreateKind::ALL.len() {
                            ui.separator();
                        }
                        let reason = lock.or(own);
                        let r = ui
                            .add_enabled_ui(reason.is_none(), |ui| {
                                icons::source_icon_button(ui, source, label, dark)
                            })
                            .inner;
                        let r = match reason {
                            Some(why) => explain(r, label, why),
                            None => r.on_hover_text(label),
                        };
                        if r.clicked() {
                            command = Some(cmd);
                        }
                        seen.push((label.to_owned(), r.rect));
                    }
                });
            });
        remember_width(ui, bottom_min_key(), chips_end + margin);
        remember_width(ui, bottom_width_key(), scroll.content_size.x + margin);
    });
    ctx.data_mut(|d| d.insert_temp(buttons_key(), seen));
    match command {
        Some(Command::Boolean(op)) => app::open_boolean(&mut app.ws, op),
        Some(Command::Create(kind)) => app::do_create(&mut app.ws, kind),
        Some(Command::Blend(chamfer)) => app::open_blend(&mut app.ws, chamfer),
        None => {}
    }
}

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

/// Hinweis unter dem oberen Feld, nur in schreibgeschützten Ansichten alter Versionen; über die
/// ganze Breite zwischen den Spalten, bei schmalen Fenstern zweizeilig.
fn banner_field(ctx: &egui::Context, rects: &ShellRects, app: &mut PlanoraApp) {
    let tab = app.ws.active_tab();
    let Some(rect) = rects.banner.filter(|_| tab.read_only) else {
        return;
    };
    let dark = tab.state.dark;
    let n = tab.based_on.unwrap_or_default();
    let text = format!(
        "Alte Version v{n} – schreibgeschützt. Im Bereich „Versionen“ lässt sie sich wiederherstellen oder abzweigen."
    );
    let color = theme::palette(dark).warning;
    text_field(
        ctx,
        banner_id(),
        rect,
        shell::BANNER_LINE_H,
        (false, MAX_ROWS),
        dark,
        (text, color),
    );
}
