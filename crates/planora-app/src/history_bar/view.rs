//! Oberfläche der Verlaufsleiste: Abspielknöpfe, Schrittreihe, Marker und Stand.

use super::model::{StepState, StepView, body_color, go_to, marker_position, reachable_end, steps};
use crate::icons::{self, Icon, IconSource};
use crate::shell::{self, ShellMode, ShellState};
use crate::state::AppState;
use crate::{browser, properties, shell_panels, theme, viewport};
use egui::{Rect, pos2, vec2};
use planora_feature::{FeatureId, FeatureKind};
use planora_i18n::tr;

/// Kante eines Schrittsymbols.
const STEP_SIZE: f32 = 30.0;
/// Rand zwischen Schrittfläche und gezeichnetem Symbol.
const STEP_PAD: f32 = 7.0;
/// Kante des Warnzeichens an einem Schritt mit Fehler.
const WARN_SIZE: f32 = 12.0;
/// Abstand zwischen zwei Schritten; in seiner Mitte steht der Marker.
const STEP_GAP: f32 = theme::SPACE_1;
/// Breite des Markers (Fläche zum Ziehen; gezeichnet wird eine Linie mit Griff).
const MARKER_W: f32 = 12.0;
/// So breit bleibt die Schrittreihe mindestens, bevor der Stand rechts kürzer wird oder entfällt
/// (drei Schritte mit Marker).
const MIN_STRIP: f32 = 3.0 * (STEP_SIZE + STEP_GAP) + MARKER_W;
/// Takt beim Abspielen in Sekunden.
const PLAY_INTERVAL: f64 = 0.6;
/// Höchstzahl der Maße im Hinweis zu einem Schritt.
const HOVER_ROWS: usize = 2;

/// Kennung des Felds (auch für [`crate::shell_panels::field_rects`]).
pub(crate) fn bar_id() -> egui::Id {
    egui::Id::new("planora-field-history")
}

fn rects_key() -> egui::Id {
    bar_id().with("rects")
}

/// Laufendes Abspielen (höchstens eines): Tab, Körper und Zeit des letzten Schritts.
#[derive(Clone, Copy)]
struct Playing {
    instance: u64,
    tip: FeatureId,
    last: f64,
}

fn play_key() -> egui::Id {
    bar_id().with("play")
}

/// Läuft gerade ein Abspielen? Die Anwendung zählt das als Geste für Rückgängig: Die ganze
/// Wiedergabe wird höchstens ein Schritt, sonst würden die Schritte der Wiedergabe den echten
/// Rückgängig-Verlauf aus der Liste drängen.
#[doc(hidden)]
pub fn is_playing(ctx: &egui::Context) -> bool {
    playing(ctx).is_some()
}

fn playing(ctx: &egui::Context) -> Option<Playing> {
    ctx.data(|d| d.get_temp::<Playing>(play_key()))
}

fn stop_playing(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Playing>(play_key()));
}

/// Körper und Markerstelle, zu denen die Reihe im Tab `instance` zuletzt gerollt wurde.
fn revealed_key(instance: u64) -> egui::Id {
    bar_id().with(("revealed", instance))
}

fn marker_id(instance: u64) -> egui::Id {
    bar_id().with(("marker", instance))
}

/// Lage der Leiste und ihrer Teile im letzten Bild (für Tests und die Lage der Felder).
#[derive(Clone, Debug)]
pub struct BarRects {
    /// Das ganze Feld.
    pub bar: Rect,
    /// Sichtbarer Ausschnitt der Schrittreihe (sie rollt waagerecht).
    pub strip: Rect,
    /// Je Schritt die Fläche des Symbols (auch außerhalb des sichtbaren Ausschnitts).
    pub steps: Vec<(FeatureId, Rect)>,
    /// Fläche des Markers; leer ohne Körper.
    pub marker: Rect,
    /// Abgeblendete Schritte (hinter dem Marker, unterdrückt oder übersprungen).
    pub dimmed: Vec<FeatureId>,
    /// Knöpfe: Beschriftung, Fläche, bedienbar.
    pub buttons: Vec<(String, Rect, bool)>,
    /// Stand rechts (Text und gegebenenfalls „Alles aktivieren“); leer, wenn er entfällt.
    pub status: Rect,
}

impl Default for BarRects {
    fn default() -> Self {
        BarRects {
            bar: Rect::NOTHING,
            strip: Rect::NOTHING,
            steps: Vec::new(),
            marker: Rect::NOTHING,
            dimmed: Vec::new(),
            buttons: Vec::new(),
            status: Rect::NOTHING,
        }
    }
}

/// Lage der Leiste aus dem letzten Bild; leer, wenn sie nicht gezeigt wurde.
pub fn bar_rects(ctx: &egui::Context) -> BarRects {
    ctx.data(|d| d.get_temp(rects_key())).unwrap_or_default()
}

/// Die Leiste ist in diesem Bild nicht da (Skizze, Zeichnung): keine alte Lage melden, und ein
/// laufendes Abspielen endet (es geht nach der Rückkehr nicht von selbst weiter).
pub(crate) fn forget(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<BarRects>(rects_key()));
    stop_playing(ctx);
}

/// Lage der Leiste in einem Fenster `content` ohne gemessene Felder (Geometrie des Modells mit
/// leeren Breiten).
#[doc(hidden)]
pub fn default_rect(content: Rect) -> Rect {
    shell::layout_in_mode(
        content,
        ShellMode::Model,
        &ShellState::default(),
        (0.0, 0.0),
        0.0,
        0.0,
        false,
    )
    .history
    .unwrap_or(Rect::NOTHING)
}

/// Zeichnet die Leiste an der Stelle aus der Lage der Felder (ohne Felder, etwa in Tests, aus
/// [`default_rect`]); nicht gesperrt.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let rect = shell_panels::last_layout(ctx)
        .and_then(|r| r.history)
        .unwrap_or_else(|| default_rect(ctx.content_rect()));
    show_with(ctx, state, rect, None);
}

/// Was ein Bedienelement der Leiste auslöst; ausgeführt nach dem Zeichnen.
#[derive(Clone, Copy)]
enum Action {
    Select(FeatureId),
    /// Doppelklick: Einstellungen bzw. Skizze öffnen.
    Open(FeatureId),
    Go(usize),
    Play,
    Pause,
    ActivateAll,
    /// Kontextmenü eines Schritts (wie früher die Zeilen im Feature-Baum).
    Rename(FeatureId),
    ToggleSuppress(FeatureId),
    Delete(FeatureId),
}

/// Stand der Leiste für den gewählten Körper.
struct Bar {
    tip: FeatureId,
    instance: u64,
    steps: Vec<StepView>,
    /// [`marker_position`].
    pos: usize,
    /// [`reachable_end`].
    reach: usize,
    color: egui::Color32,
}

impl Bar {
    fn len(&self) -> usize {
        self.steps.len()
    }

    /// Der globale Marker setzt Schritte dieses Körpers zurück.
    fn hidden(&self) -> bool {
        self.reach < self.len()
    }
}

/// Zeichnet die Leiste in `rect`. `lock` (offene Rückfrage, alte Version; wie beim Befehlsfeld)
/// sperrt alle Bedienelemente und hält das Abspielen an; beim Überfahren steht der Grund.
pub fn show_with(ctx: &egui::Context, state: &mut AppState, rect: Rect, lock: Option<&str>) {
    let dark = state.dark;
    let instance = state.instance;
    let bar = state.history_body().map(|tip| Bar {
        tip,
        instance,
        steps: steps(state, tip),
        pos: marker_position(state, tip),
        reach: reachable_end(state, tip),
        color: body_color(state, tip).get(dark),
    });
    let playing = playing(ctx).is_some_and(|p| {
        p.instance == instance && bar.as_ref().is_some_and(|b| b.tip == p.tip) && lock.is_none()
    });
    let frame = theme::panel_frame(dark).inner_margin(egui::Margin::symmetric(
        theme::SPACE_3 as i8,
        theme::SPACE_1 as i8,
    ));
    let inner = shell_panels::inner_size(&frame, rect.size());
    let mut out = BarRects {
        bar: rect,
        ..BarRects::default()
    };
    let mut action = None;
    let view: &AppState = state;
    shell_panels::field(ctx, bar_id(), rect.min, frame, |ui| {
        ui.set_width(inner.x);
        ui.set_height(inner.y);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_1;
            ui.spacing_mut().button_padding.x = theme::SPACE_1;
            ui.add_enabled_ui(lock.is_none(), |ui| {
                let controls = (bar.as_ref(), playing, lock);
                playback(ui, controls, &mut out, &mut action);
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let fit = status_fit(ui, bar.as_ref());
                    status(ui, bar.as_ref(), dark, fit, &mut out, &mut action);
                    if fit != Fit::Hidden {
                        ui.separator();
                    }
                    let w = ui.available_width().max(0.0);
                    ui.allocate_ui_with_layout(
                        vec2(w, inner.y),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            if let Some(bar) = &bar {
                                strip(ui, view, bar, inner.y, &mut out, &mut action);
                            }
                        },
                    );
                });
            });
        });
    });
    ctx.data_mut(|d| d.insert_temp(rects_key(), out));
    if let (Some(bar), Some(action)) = (&bar, action) {
        run(ctx, state, bar, action);
    }
    tick(ctx, state, lock);
}

/// Knöpfe Anfang, zurück, Abspielen (beim Abspielen: Anhalten), vor, Ende. `controls`: Stand,
/// ob es gerade abspielt, Sperrgrund.
fn playback(
    ui: &mut egui::Ui,
    (bar, playing, lock): (Option<&Bar>, bool, Option<&str>),
    out: &mut BarRects,
    action: &mut Option<Action>,
) {
    let dark = ui.visuals().dark_mode;
    let (pos, reach) = bar.map_or((0, 0), |b| (b.pos, b.reach));
    let some = bar.is_some();
    let (play_icon, play_label, play_action) = if playing {
        (Icon::HistoryPause, tr!("history-pause"), Action::Pause)
    } else {
        (Icon::HistoryPlay, tr!("history-play"), Action::Play)
    };
    let entries = [
        (
            Icon::HistoryStart,
            tr!("history-to-start"),
            pos > 0,
            Action::Go(0),
        ),
        (
            Icon::HistoryBack,
            tr!("history-step-back"),
            pos > 0,
            Action::Go(pos.saturating_sub(1)),
        ),
        (play_icon, play_label, playing || reach > 0, play_action),
        (
            Icon::HistoryForward,
            tr!("history-step-forward"),
            pos < reach,
            Action::Go(pos + 1),
        ),
        (
            Icon::ToTimelineEnd,
            tr!("history-to-end"),
            pos < reach,
            Action::Go(reach),
        ),
    ];
    let reason = lock
        .map(str::to_string)
        .or_else(|| (!some).then(|| tr!("history-choose-body-first")));
    for (icon, label, possible, act) in entries {
        let on = some && possible;
        let r = ui
            .add_enabled_ui(on, |ui| {
                icons::source_icon_button(ui, IconSource::Font(icon), &label, dark)
            })
            .inner;
        let r = match &reason {
            Some(why) => r.on_disabled_hover_ui(|ui| {
                ui.strong(&label);
                ui.label(why);
            }),
            None => r.on_hover_text(&label),
        };
        if r.clicked() {
            *action = Some(act);
        }
        out.buttons.push((label, r.rect, on && ui.is_enabled()));
    }
}

/// Wie viel vom Stand rechts Platz hat, ohne die Schrittreihe unter [`MIN_STRIP`] zu drücken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fit {
    /// „Schritt 2 von 3“ bzw. Hinweis und „Alles aktivieren“.
    Full,
    /// „2/3“ bzw. nur ein Symbolknopf für „Alles aktivieren“ (Hinweis beim Überfahren).
    Short,
    /// Ohne Stand (nur bei normaler Leiste; „Alles aktivieren“ bleibt immer).
    Hidden,
}

/// Breite eines Textes in der Schrift der Beschriftungen.
fn text_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    ui.painter()
        .layout_no_wrap(text.to_string(), font, egui::Color32::PLACEHOLDER)
        .size()
        .x
}

/// Texte des Stands: lang und kurz.
fn status_texts(bar: &Bar) -> (String, String) {
    let long = if bar.pos == 0 && bar.len() > 0 {
        // Noch kein Schritt ist aktiv: „Schritt 0 von 3“ liest sich schief.
        tr!("history-before-step-one")
    } else {
        tr!("history-step-of", pos = bar.pos, len = bar.len())
    };
    (long, format!("{}/{}", bar.pos, bar.len()))
}

/// Wählt die längste Form des Stands, neben der die Schrittreihe noch [`MIN_STRIP`] breit
/// bleibt. Erst wird der Text kürzer, dann entfällt er, bevor die Reihe schrumpft.
fn status_fit(ui: &egui::Ui, bar: Option<&Bar>) -> Fit {
    let Some(bar) = bar else {
        return Fit::Full;
    };
    let gap = ui.spacing().item_spacing.x;
    // Trennstrich mit Abständen.
    let sep = 2.0 * gap + 1.0;
    let avail = ui.available_width();
    let fits = |w: f32| avail - w - sep >= MIN_STRIP;
    if bar.hidden() {
        let button = text_width(ui, &activate_all()) + 2.0 * ui.spacing().button_padding.x;
        let full = button + gap + text_width(ui, &global_reset());
        return if fits(full) { Fit::Full } else { Fit::Short };
    }
    let (long, short) = status_texts(bar);
    if fits(text_width(ui, &long)) {
        Fit::Full
    } else if fits(text_width(ui, &short)) {
        Fit::Short
    } else {
        Fit::Hidden
    }
}

/// Beschriftung des Knopfs, der den globalen Marker ans Ende setzt.
fn activate_all() -> String {
    tr!("history-activate-all")
}
/// Hinweis, wenn der globale Marker Schritte des Körpers zurücksetzt.
fn global_reset() -> String {
    tr!("history-global-reset")
}

/// Rechts der Stand: „Schritt n von m“, ohne Körper „Körper wählen“, bei vom globalen Marker
/// zurückgesetztem Verlauf der Hinweis mit „Alles aktivieren“ (Zeile von rechts nach links, also
/// in umgekehrter Reihenfolge gesetzt).
fn status(
    ui: &mut egui::Ui,
    bar: Option<&Bar>,
    dark: bool,
    fit: Fit,
    out: &mut BarRects,
    action: &mut Option<Action>,
) {
    let p = theme::palette(dark);
    match bar {
        None => {
            let r =
                ui.label(egui::RichText::new(tr!("history-choose-body")).color(p.text_secondary));
            out.status = r.rect;
        }
        Some(bar) if bar.hidden() => {
            // Schmal: nur ein Symbolknopf (Warnzeichen), Hinweis und Befehl beim Überfahren.
            let activate = activate_all();
            let r = if fit == Fit::Full {
                ui.button(&activate)
            } else {
                icons::source_icon_button(ui, IconSource::Font(Icon::Warning), &activate, dark)
            };
            theme::focus_ring(ui, &r);
            let hint = if fit == Fit::Full {
                tr!("history-recompute-all")
            } else {
                tr!(
                    "history-global-reset-hint",
                    reset = global_reset(),
                    activate = activate.as_str()
                )
            };
            let r = r.on_hover_text(hint);
            if r.clicked() {
                *action = Some(Action::ActivateAll);
            }
            out.buttons.push((activate, r.rect, ui.is_enabled()));
            out.status = r.rect;
            if fit == Fit::Full {
                let l = ui.label(egui::RichText::new(global_reset()).color(p.warning));
                out.status = out.status.union(l.rect);
            }
        }
        Some(bar) => {
            let (long, short) = status_texts(bar);
            let text = match fit {
                Fit::Full => long,
                Fit::Short => short,
                Fit::Hidden => return,
            };
            let r = ui.label(egui::RichText::new(text).color(p.text_secondary));
            out.status = r.rect;
        }
    }
}

/// Die Schritte in einer waagerecht rollenden Reihe und der Marker darüber.
fn strip(
    ui: &mut egui::Ui,
    state: &AppState,
    bar: &Bar,
    height: f32,
    out: &mut BarRects,
    action: &mut Option<Action>,
) {
    // Ohne Mindestbreite: Die Reihe bleibt in ihrem Platz und läuft nicht unter den Stand.
    let area = egui::ScrollArea::horizontal()
        .id_salt(bar_id().with(("strip", bar.instance)))
        .min_scrolled_width(0.0)
        .auto_shrink([false, false]);
    let scroll = theme::scroll_area(ui, area, |ui| {
        ui.set_height(height);
        ui.spacing_mut().item_spacing.x = STEP_GAP;
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            // Platz für den Marker vor dem ersten und hinter dem letzten Schritt.
            ui.add_space(MARKER_W / 2.0);
            // Erst die Lage der Schritte, dann die Fläche des Markers, dann die Schritte selbst:
            // Sie liegen damit über dem Marker und bekommen Klicks an ihrem Rand, den die Fläche
            // des Markers überdeckt (gezogen wird der Marker dort trotzdem).
            let rects: Vec<(FeatureId, Rect)> = bar
                .steps
                .iter()
                .map(|v| {
                    let size = vec2(STEP_SIZE, STEP_SIZE);
                    (v.id, ui.allocate_exact_size(size, egui::Sense::hover()).0)
                })
                .collect();
            ui.add_space(MARKER_W / 2.0);
            let row = ui.max_rect();
            let hit = marker(ui, bar, &rects, row, action);
            for (v, (_, rect)) in bar.steps.iter().zip(&rects) {
                let r = step_button(ui, state, v, *rect, bar);
                if r.double_clicked() {
                    *action = Some(Action::Open(v.id));
                } else if r.clicked() {
                    *action = Some(Action::Select(v.id));
                    // Danach bewegen die Pfeiltasten den Marker.
                    ui.memory_mut(|m| m.request_focus(marker_id(bar.instance)));
                }
                step_menu(&r, state, v.id, action);
                if v.state == StepState::Inactive {
                    out.dimmed.push(v.id);
                }
            }
            if let Some(hit) = &hit {
                paint_marker(ui, hit, row);
            }
            out.marker = hit.map_or(Rect::NOTHING, |h| h.rect);
            out.steps = rects;
        });
    });
    out.strip = scroll.inner_rect;
}

/// Ein Schritt in `rect`: Symbol in der Körperfarbe (abgeblendet, wenn er nicht berechnet wird),
/// bei einem Fehler mit Warnzeichen; gewählt mit Fläche. Beim Überfahren Name, die wichtigsten
/// Maße, gegebenenfalls der Fehler und der Hinweis auf den Doppelklick.
fn step_button(
    ui: &mut egui::Ui,
    state: &AppState,
    v: &StepView,
    rect: Rect,
    bar: &Bar,
) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let p = theme::palette(dark);
    let color = bar.color;
    let id = bar_id().with(("step", bar.instance, v.id));
    let response = ui.interact(rect, id, egui::Sense::click());
    let enabled = ui.is_enabled();
    let selected = state.selected_feature == Some(v.id);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, &v.name)
    });
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = egui::CornerRadius::same(theme::RADIUS_SM);
        if selected {
            painter.rect_filled(rect, radius, p.bg_active);
        } else if enabled && response.hovered() {
            painter.rect_filled(rect, radius, p.bg_field);
        }
        let mut ink = color;
        if v.state == StepState::Inactive {
            ink = ink.gamma_multiply(theme::HISTORY_DIM_ALPHA);
        }
        if !enabled {
            ink = ink.gamma_multiply(theme::DISABLED_ALPHA);
        }
        icons::paint_feature_icon(painter, rect.shrink(STEP_PAD), v.icon, ink);
        if matches!(v.state, StepState::Error(_)) {
            let warn = Rect::from_min_size(
                pos2(rect.right() - WARN_SIZE, rect.top()),
                vec2(WARN_SIZE, WARN_SIZE),
            );
            icons::paint_source(painter, warn, IconSource::Font(Icon::Warning), p.error);
        }
    }
    theme::focus_ring(ui, &response);
    response.on_hover_ui(|ui| {
        ui.strong(&v.name);
        if let Some(f) = state.document.feature(v.id) {
            for (label, value) in hover_rows(state, &f.kind) {
                ui.label(format!("{label}: {value}"));
            }
        }
        match &v.state {
            StepState::Error(e) => {
                ui.colored_label(p.error, e);
            }
            StepState::Inactive => {
                ui.label(egui::RichText::new(tr!("history-rolled-back")).color(p.text_secondary));
            }
            StepState::Active => {}
        }
        let hint = match state.document.feature(v.id).map(|f| &f.kind) {
            Some(FeatureKind::Sketch(_)) => tr!("history-double-click-sketch"),
            _ => tr!("history-double-click-settings"),
        };
        ui.label(egui::RichText::new(hint).color(p.text_secondary));
    })
}

/// Kontextmenü eines Schritts: Bearbeiten, Umbenennen, Unterdrücken bzw. Aktivieren, Löschen
/// (die Befehle, die früher die Schrittzeilen im Feature-Baum hatten).
fn step_menu(r: &egui::Response, state: &AppState, id: FeatureId, action: &mut Option<Action>) {
    let suppressed = state.document.feature(id).is_some_and(|f| f.suppressed);
    r.context_menu(|ui| {
        let mut pick = |a: Action, ui: &mut egui::Ui| {
            *action = Some(a);
            ui.close();
        };
        if ui.button(tr!("history-edit")).clicked() {
            pick(Action::Open(id), ui);
        }
        if ui.button(tr!("history-rename")).clicked() {
            pick(Action::Rename(id), ui);
        }
        let toggle = if suppressed {
            tr!("history-activate")
        } else {
            tr!("history-suppress")
        };
        if ui.button(&toggle).clicked() {
            pick(Action::ToggleSuppress(id), ui);
        }
        ui.separator();
        if ui.button(tr!("history-delete")).clicked() {
            pick(Action::Delete(id), ui);
        }
    });
}

/// Die wichtigsten Maße eines Schritts für den Hinweis: höchstens [`HOVER_ROWS`] Zeilen aus
/// [`properties::parameter_rows`], Maße (mm, Grad) zuerst; ohne Maße die ersten Zeilen.
#[doc(hidden)]
pub fn hover_rows(state: &AppState, kind: &FeatureKind) -> Vec<(String, String)> {
    let (mut rows, rest): (Vec<_>, Vec<_>) = properties::parameter_rows(state, kind)
        .into_iter()
        .partition(|(_, value)| value.ends_with(" mm") || value.ends_with('°'));
    rows.extend(rest);
    rows.truncate(HOVER_ROWS);
    rows
}

/// Lage der Lücken vor jedem Schritt und hinter dem letzten (die möglichen Markerstellen).
fn gaps(rects: &[(FeatureId, Rect)]) -> Vec<f32> {
    let mut gaps: Vec<f32> = rects
        .iter()
        .map(|(_, r)| r.left() - STEP_GAP / 2.0)
        .collect();
    if let Some((_, last)) = rects.last() {
        gaps.push(last.right() + STEP_GAP / 2.0);
    }
    gaps
}

/// Der Marker nach der Bedienung: Fläche, Lage der Linie und ob er den Fokus hat.
#[derive(Clone, Copy)]
struct MarkerHit {
    rect: Rect,
    x: f32,
    focused: bool,
}

/// Bedienung des Markers in der Lücke an der Markerstelle. Ziehen setzt ihn in die nächste Lücke
/// (höchstens bis [`reachable_end`]); hat er den Fokus, bewegen ihn die Pfeile nach links und
/// rechts. Ändert sich die Stelle, rollt die Reihe, bis er sichtbar ist. `None` ohne Schritte;
/// gezeichnet wird er mit [`paint_marker`] nach den Schritten.
fn marker(
    ui: &mut egui::Ui,
    bar: &Bar,
    rects: &[(FeatureId, Rect)],
    row: Rect,
    action: &mut Option<Action>,
) -> Option<MarkerHit> {
    let gaps = gaps(rects);
    let &x = gaps.get(bar.pos)?;
    let rect = Rect::from_center_size(pos2(x, row.center().y), vec2(MARKER_W, row.height()));
    let response = ui.interact(rect, marker_id(bar.instance), egui::Sense::click_and_drag());
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Slider,
            enabled,
            tr!("history-marker-label", pos = bar.pos, len = bar.len()),
        )
    });
    // Der Marker liest nur die Pfeiltasten: Esc, Entf und Eingabe bleiben für die Ansicht.
    viewport::focus_is_not_typing(ui.ctx(), response.id);
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if response.dragged()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let reach = bar.reach.min(gaps.len() - 1);
        let nearest = gaps[..=reach]
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1 - pointer.x).abs().total_cmp(&(b.1 - pointer.x).abs()))
            .map(|(i, _)| i);
        if let Some(i) = nearest.filter(|i| *i != bar.pos) {
            *action = Some(Action::Go(i));
        }
    }
    if response.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    ..Default::default()
                },
            );
        });
        let (left, right) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight),
            )
        });
        if left && bar.pos > 0 {
            *action = Some(Action::Go(bar.pos - 1));
        } else if right && bar.pos < bar.reach {
            *action = Some(Action::Go(bar.pos + 1));
        }
    }
    // Nach einer Änderung der Stelle (oder des Körpers) in den sichtbaren Ausschnitt springen
    // (ohne Gleiten: beim Ziehen und Abspielen folgte die Reihe sonst verzögert); erst als
    // erledigt merken, wenn dieser Durchgang gilt.
    let shown = (bar.tip, bar.pos);
    let key = revealed_key(bar.instance);
    if ui.ctx().data(|d| d.get_temp::<(FeatureId, usize)>(key)) != Some(shown) {
        ui.scroll_to_rect_animation(rect, None, egui::style::ScrollAnimation::none());
        if !ui.ctx().will_discard() {
            ui.ctx().data_mut(|d| d.insert_temp(key, shown));
        }
    }
    Some(MarkerHit {
        rect,
        x,
        focused: response.has_focus(),
    })
}

/// Zeichnet den Marker als senkrechte Linie mit Griff (über den Schritten).
fn paint_marker(ui: &egui::Ui, hit: &MarkerHit, row: Rect) {
    let dark = ui.visuals().dark_mode;
    let p = theme::palette(dark);
    let color = if ui.is_enabled() {
        p.marker
    } else {
        p.marker.gamma_multiply(theme::DISABLED_ALPHA)
    };
    let x = hit.x;
    let painter = ui.painter();
    painter.line_segment(
        [pos2(x, row.top() + 2.0), pos2(x, row.bottom() - 2.0)],
        egui::Stroke::new(2.0, color),
    );
    let handle = Rect::from_center_size(pos2(x, row.top() + 5.0), vec2(10.0, 8.0));
    painter.rect_filled(
        handle,
        egui::CornerRadius::same(theme::RADIUS_XS / 2),
        color,
    );
    if hit.focused {
        painter.rect_stroke(
            hit.rect.expand(1.0),
            egui::CornerRadius::same(theme::RADIUS_XS),
            egui::Stroke::new(2.0, theme::accent(dark)),
            egui::StrokeKind::Outside,
        );
    }
}

/// Führt eine Aktion der Leiste aus. Fehler stehen danach in `state.message`.
fn run(ctx: &egui::Context, state: &mut AppState, bar: &Bar, action: Action) {
    match action {
        Action::Select(id) => {
            // Wie ein Klick auf die Zeile im Feature-Baum: nur der Schritt ist hervorgehoben.
            state.selected_feature = Some(id);
            state.selected_body = None;
            state.drawing_ui.selected_drawing = None;
            state.set_selection(None);
        }
        // Wie ein Doppelklick im Feature-Baum; den Dialog öffnet die App aus `edit_request`
        // (siehe [`super::open_step`]).
        Action::Open(id) => {
            stop_playing(ctx);
            browser::edit(state, id);
        }
        Action::Go(position) => {
            stop_playing(ctx);
            let _ = go_to(state, bar.tip, position);
        }
        Action::Play => {
            // Am Ende beginnt das Abspielen von vorn.
            if bar.pos >= bar.reach && go_to(state, bar.tip, 0).is_err() {
                return;
            }
            let last = ctx.input(|i| i.time);
            let p = Playing {
                instance: bar.instance,
                tip: bar.tip,
                last,
            };
            ctx.data_mut(|d| d.insert_temp(play_key(), p));
            ctx.request_repaint_after_secs(PLAY_INTERVAL as f32);
        }
        Action::Pause => stop_playing(ctx),
        Action::ActivateAll => {
            stop_playing(ctx);
            let _ = browser::roll_to_end(state);
        }
        // Das Fenster „Umbenennen“ und die Rückfrage zeichnet der Feature-Baum.
        Action::Rename(id) => {
            if let Some(f) = state.document.feature(id) {
                state.rename_draft = Some((id, f.name.clone()));
            }
        }
        Action::ToggleSuppress(id) => {
            stop_playing(ctx);
            let _ = browser::toggle_suppress(state, id);
        }
        Action::Delete(id) => {
            stop_playing(ctx);
            browser::request_delete(state, id);
        }
    }
}

/// Abspielen: alle [`PLAY_INTERVAL`] Sekunden ein Schritt vor; hält am Ende (bei
/// [`reachable_end`]), bei einem Fehler, einem anderen Körper oder Tab oder einer Sperre an. Nur
/// solange es läuft, wird ein neues Bild angefordert.
fn tick(ctx: &egui::Context, state: &mut AppState, lock: Option<&str>) {
    let Some(Playing {
        instance,
        tip,
        last,
    }) = playing(ctx)
    else {
        return;
    };
    let stop = || stop_playing(ctx);
    if lock.is_some() || instance != state.instance || state.history_body() != Some(tip) {
        stop();
        return;
    }
    if marker_position(state, tip) >= reachable_end(state, tip) {
        stop();
        return;
    }
    let now = ctx.input(|i| i.time);
    let waited = now - last;
    if waited < PLAY_INTERVAL {
        ctx.request_repaint_after_secs((PLAY_INTERVAL - waited) as f32);
        return;
    }
    let next = marker_position(state, tip) + 1;
    if go_to(state, tip, next).is_err() || next >= reachable_end(state, tip) {
        stop();
        return;
    }
    let p = Playing {
        instance,
        tip,
        last: now,
    };
    ctx.data_mut(|d| d.insert_temp(play_key(), p));
    ctx.request_repaint_after_secs(PLAY_INTERVAL as f32);
}
