//! Schwebende Felder (Graphit, Etappe B): oben Menü und Tabs, links Projekte und Feature-Baum,
//! rechts Eigenschaften oder Versionen, links unten die Statuszeile und bei alten Versionen ein
//! schmaler Hinweis unter dem oberen Feld. Jedes Feld ist ein `egui::Area` an der Stelle aus
//! [`shell::layout_in_mode`]; die Inhalte zeichnen die bisherigen Bereichsfunktionen. Im
//! Skizzenmodus (Etappe C) entfallen Baum, Eigenschaften und Befehlsfeld; Leiste, Skizzenfeld und
//! Fuß zeichnet `sketch_panel::show` an den hier berechneten Rechtecken. Im Zeichnungsmodus bleibt
//! der Baum links; Werkzeugfeld, Ansichtsfeld (statt Eigenschaften) und Hinweisfeld (statt des
//! Befehlsfelds) zeichnet `drawing_view::show_with` an den hier berechneten Rechtecken.
//!
//! Breiten und Höhen kommen aus der Geometrie (keine Rückkopplung über die Feldgröße). Status und
//! Hinweis werden vorab gesetzt (der Hinweis bis zu zwei Zeilen, der Status bis zu drei und er
//! wächst nach oben). Das obere und das untere Feld messen ihre Wunschbreite und ihre
//! Mindestbreite; beide werden für das nächste Bild gemerkt. Ändert sich eine Breite, oder
//! läuft gerade der unsichtbare erste Größen-Durchgang eines Felds, wird der Durchgang verworfen
//! und sofort wiederholt (wie bei `prompts::window`), damit kein Feld ein Bild lang falsch steht.
//!
//! Das Befehlsfeld unten lässt dem Status die Breite, die dessen Text in zwei Zeilen braucht:
//! Es wird bei langen Meldungen schmaler (der Inhalt rollt), aber nie unter die drei Chips mit
//! Trennstrich. Reicht der Rest dem Status nicht, bricht er in bis zu drei Zeilen um.

use egui::{Vec2, vec2};

use crate::app::{self, CreateKind, PlanoraApp};
use crate::shell::{self, RightTab, ShellMode, ShellRects};
use crate::state::AppState;
use crate::{
    browser, icons, logo, projects_panel, properties, tabs, theme, versions_panel, viewport,
};
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

/// Kennung des Texts der Statuszeile (Tab-Halt für die Bedienhilfe).
pub fn status_text_id() -> egui::Id {
    status_id().with("text")
}

fn bottom_id() -> egui::Id {
    egui::Id::new("planora-field-bottom")
}

fn banner_id() -> egui::Id {
    egui::Id::new("planora-field-banner")
}

/// Werkzeugleiste des Skizzenmodus (gezeichnet von [`crate::sketch_panel::show`]).
pub(crate) fn sketch_strip_id() -> egui::Id {
    egui::Id::new("planora-field-sketch-strip")
}

/// Skizzenfeld rechts (gezeichnet von [`crate::sketch_panel::show`]).
pub(crate) fn sketch_side_id() -> egui::Id {
    egui::Id::new("planora-field-sketch-side")
}

/// Fußfeld des Skizzenmodus (gezeichnet von [`crate::sketch_panel::show`]).
pub(crate) fn sketch_foot_id() -> egui::Id {
    egui::Id::new("planora-field-sketch-foot")
}

/// Werkzeugfeld der Zeichnung (gezeichnet von [`crate::drawing_view::show_with`]).
pub(crate) fn drawing_tool_id() -> egui::Id {
    egui::Id::new("planora-field-drawing-tool")
}

/// Ansichtsfeld der Zeichnung rechts (gezeichnet von [`crate::drawing_view::show_with`]).
pub(crate) fn drawing_view_id() -> egui::Id {
    egui::Id::new("planora-field-drawing-view")
}

/// Hinweisfeld der Zeichnung unten (gezeichnet von [`crate::drawing_view::show_with`]).
pub(crate) fn drawing_hint_id() -> egui::Id {
    egui::Id::new("planora-field-drawing-hint")
}

/// Gemerkte Wunschbreite des Werkzeugfelds der Zeichnung (beide Zeilen ohne Rollen, mit Rand).
pub(crate) fn drawing_tool_width_key() -> egui::Id {
    drawing_tool_id().with("wanted")
}

/// Hinweis zu den Tasten im Skizzenmodus; steht nur beim Überfahren der Statuszeile da.
pub const SKETCH_KEYS_HINT: &str = "Esc bricht ab, Eingabe beendet, Entf löscht, mittlere Maustaste oder Umschalt + Ziehen verschiebt";

/// Gemerkte Wunschbreite des Fußfelds (Hinweis, Status, „Skizze beenden“, Rand).
pub(crate) fn sketch_foot_width_key() -> egui::Id {
    sketch_foot_id().with("wanted")
}

/// Gemerkte Mindestbreite des Fußfelds (Status, „Skizze beenden“, Rand; der Hinweis kürzt sich).
pub(crate) fn sketch_foot_min_key() -> egui::Id {
    sketch_foot_id().with("min")
}

/// Modus der Oberfläche für den Zustand eines Tabs: offene Skizze, sonst Zeichnung oder Modell.
pub fn mode_of(state: &AppState) -> ShellMode {
    if state.sketch_session.is_some() {
        ShellMode::Sketch
    } else if matches!(
        state.drawing_ui.area,
        crate::drawing_state::WorkArea::Drawing(_)
    ) {
        ShellMode::Drawing
    } else {
        ShellMode::Model
    }
}

/// Gemerkte Wunschbreite des oberen Felds (außen).
fn top_width_key() -> egui::Id {
    top_id().with("wanted")
}

/// Gemerkte Wunschbreite des oberen Felds ohne das Programmzeichen (außen).
fn top_base_key() -> egui::Id {
    top_id().with("base")
}

/// Mindestbreite des Hinweisfelds der Zeichnung (außen); ein langer Status bricht dann um.
const DRAWING_HINT_MIN_W: f32 = 2.0 * shell::MIN_FIELD_W;

/// Platz, den die Tab-Leiste neben dem Menü mindestens bekommt (etwa ein Tab).
const MIN_TABS_W: f32 = 120.0;

/// Gemerkte Mindestbreite des oberen Felds: Menü, Trennstrich, [`MIN_TABS_W`], Rand.
fn top_min_key() -> egui::Id {
    top_id().with("min")
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

pub(crate) fn stored(ctx: &egui::Context, key: egui::Id) -> Option<f32> {
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
pub(crate) fn remember_width(ui: &egui::Ui, key: egui::Id, width: f32) {
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

/// Bricht `text` (Textstil `Body`) bei `max_width` um, höchstens `rows` Zeilen.
pub(crate) fn wrapped(
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
    (text, color, hover): (String, egui::Color32, Option<&'static str>),
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
            let (r, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
            // Ziel für Hover und Bedienhilfe ist das ganze Feld samt Rand, nicht nur die Textzeile.
            let whole = ui
                .max_rect()
                .expand2(egui::vec2(margin.x / 2.0, margin.y / 2.0));
            // Für die Statuszeile ist das `status_text_id()`.
            let response = ui.interact(
                whole,
                id.with("text"),
                egui::Sense::focusable_noninteractive(),
            );
            // Tab-Halt nur zum Vorlesen: der Fokus hier sperrt keine Tasten der Ansicht und ist
            // als Fokusrahmen sichtbar.
            viewport::focus_is_not_typing(ctx, response.id);
            theme::focus_ring(ui, &response);
            let enabled = ui.is_enabled();
            // Der Tastenhinweis steht auch im Namen für die Bedienhilfe, der sichtbare Text bleibt kurz.
            let label = match hover {
                Some(h) => format!("{text}. {h}"),
                None => text.clone(),
            };
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, &label)
            });
            let elided = galley.elided;
            ui.painter().galley(r.min, galley, color);
            if elided {
                response.on_hover_text(text);
            } else if let Some(h) = hover {
                response.on_hover_text(h);
            }
        });
    });
}

/// Innenmaß eines Felds mit Rahmen `frame` und Außenmaß `outer` (Rand und Strich abgezogen).
pub(crate) fn inner_size(frame: &egui::Frame, outer: Vec2) -> Vec2 {
    (outer - frame.total_margin().sum()).max(Vec2::ZERO)
}

/// Rahmen einer einzeiligen Leiste der Höhe `height`: wie [`theme::panel_frame`], der Rand oben
/// und unten so, dass genau eine Zeile ([`theme::ROW_H`]) hineinpasst.
pub(crate) fn bar_frame(dark: bool, height: f32) -> egui::Frame {
    let frame = theme::panel_frame(dark);
    let v = ((height - 2.0 * frame.stroke.width - theme::ROW_H) / 2.0).max(0.0);
    frame.inner_margin(egui::Margin::symmetric(theme::SPACE_3 as i8, v as i8))
}

/// Inhalt eines Seitenfelds in genau `inner.x` Breite und höchstens `inner.y` Höhe. Ein Feld ist
/// so groß wie sein Inhalt; wäre ein Inhalt breiter (egui bricht z. B. Zeilen in waagerechten
/// Reihen nicht um), wüchse das Feld über seine Spalte und deckte den Navigationswürfel ab.
/// Deshalb belegt der Inhalt eine eigene Fläche, deren Größe nicht in das Feld zurückwirkt, und
/// wird am Innenrand des Rahmens (`margin`) abgeschnitten. Die Inhalte selbst sind so gesetzt,
/// dass sie in die Breite passen (Hand-Prüfung Haltepunkt 2).
pub(crate) fn side_column<R>(
    ui: &mut egui::Ui,
    inner: Vec2,
    margin: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let rect = egui::Rect::from_min_size(ui.cursor().min, inner);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect.expand(margin).intersect(ui.clip_rect()));
    let r = add_contents(&mut child);
    let height = child.min_rect().height().min(inner.y);
    ui.advance_cursor_after_rect(egui::Rect::from_min_size(rect.min, vec2(inner.x, height)));
    r
}

/// Ein Feld: Area an `pos` mit Rahmen `frame`. Der erste, unsichtbare Größen-Durchgang eines
/// neuen Felds wird verworfen, damit es schon im ersten Bild sichtbar an seiner Stelle steht.
///
/// Rand: egui sucht Widgets in einem Kreis (`interact_radius`) um den Zeiger und lässt die
/// Ebenen darunter mitzählen, solange kein Widget der oberen Ebene den ganzen Kreis abdeckt.
/// Knapp innerhalb des Feldrands galt so der 3D-Bereich (bzw. die Skizze) darunter noch als
/// „hovered“: Mausrad, Ziehen und Vorschau gingen durch. Ein unsichtbares Widget (nur Hover)
/// über dem Feld, um `interact_radius` vergrößert, deckt den Kreis ab, sobald der Zeiger im Feld
/// ist. Knapp außerhalb deckt es ihn nicht ab: dort bleibt der Bereich darunter erreichbar.
pub(crate) fn field<R>(
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
            let shown = frame.show(ui, add_contents);
            let radius = ui.style().interaction.interact_radius;
            ui.interact(
                shown.response.rect.expand(radius),
                id.with("rand"),
                egui::Sense::hover(),
            );
            shown.inner
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
        // Im Skizzenmodus steht unten das Fußfeld statt des Befehlsfelds (gleiche Regeln).
        let mode = mode_of(&tab.state);
        let (wanted_key, min_key) = if mode == ShellMode::Sketch {
            (sketch_foot_width_key(), sketch_foot_min_key())
        } else {
            (bottom_width_key(), bottom_min_key())
        };
        // Das Feld bleibt immer da: mindestens die drei Chips mit Trennstrich; der Status nimmt
        // den Rest links davon (bis zu drei Zeilen, sonst gekürzt mit vollem Text beim Überfahren).
        let bottom_w = match stored(ctx, wanted_key) {
            // Das Hinweisfeld der Zeichnung nimmt die ganze Breite, die der Status ihm lässt,
            // mindestens aber [`DRAWING_HINT_MIN_W`].
            _ if mode == ShellMode::Drawing => bottom_cap.max(DRAWING_HINT_MIN_W),
            Some(wanted) => wanted
                .min(bottom_cap)
                .max(stored(ctx, min_key).unwrap_or(0.0)),
            None => 0.0,
        };
        let top_min = stored(ctx, top_min_key()).unwrap_or(0.0);
        let rects = shell::layout_in_mode(
            content,
            mode,
            &app.shell,
            (top_w, top_min),
            bottom_w,
            status_w,
            tab.read_only,
        );
        let mut rects = rects;
        // Das Werkzeugfeld der Zeichnung ist so breit wie sein Inhalt (mittig, höchstens so breit
        // wie zwischen den Spalten).
        if let (Some(tool), Some(wanted)) = (rects.tool, stored(ctx, drawing_tool_width_key())) {
            rects.tool = Some(shell::fit_tool(tool, wanted));
        }
        (rects, status, tab.state.dark)
    };
    let (rects, _, _) = layout(app);
    let mode_before = mode_of(&app.ws.active_tab().state);
    top_field(ctx, &rects, app);
    // Im Skizzenmodus zeichnet `sketch_panel::show` Leiste, Skizzenfeld und Fuß an `rects`, im
    // Zeichnungsmodus `drawing_view::show_with` Werkzeug-, Ansichts- und Hinweisfeld.
    if rects.strip.is_none() {
        left_field(ctx, &rects, app);
    }
    if rects.strip.is_none() && rects.tool.is_none() {
        right_field(ctx, &rects, app);
        bottom_field(ctx, &rects, app);
    } else {
        // Kein Befehlsfeld: auch keine Knopflagen aus einem früheren Bild melden.
        ctx.data_mut(|d| d.remove::<Vec<(String, egui::Rect)>>(buttons_key()));
    }
    // Die Felder können den Tab gewechselt oder eine Meldung gesetzt haben: Status und Hinweis
    // zeigen schon den neuen Stand.
    // Wechselte dabei der Modus, stimmen die Felder dieses Durchgangs nicht mehr: verwerfen.
    if mode_of(&app.ws.active_tab().state) != mode_before {
        discard(ctx, "Moduswechsel");
    }
    let (rects, status, dark) = layout(app);
    let hover = (mode_of(&app.ws.active_tab().state) == ShellMode::Sketch
        && app.ws.active_tab().state.message.is_none())
    .then_some(SKETCH_KEYS_HINT);
    text_field(
        ctx,
        status_id(),
        rects.status,
        shell::STATUS_H,
        (true, STATUS_ROWS),
        dark,
        (status.0, status.1, hover),
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

/// Gemerkte Wunschbreite des Werkzeugfelds der Zeichnung (für Tests).
#[doc(hidden)]
pub fn drawing_tool_wanted(ctx: &egui::Context) -> Option<f32> {
    stored(ctx, drawing_tool_width_key())
}

/// Tatsächlich belegte Rechtecke der sichtbaren Felder, je mit Namen (für Tests). Gelesen wird
/// der Stand, den egui am Ende des vorigen Bildes gespeichert hat; nach einem Bild also die
/// Felder dieses Bildes. Die Seitenfelder sind so hoch wie ihr Inhalt, höchstens wie die Spalte.
#[doc(hidden)]
pub fn field_rects(ctx: &egui::Context) -> Vec<(&'static str, egui::Rect)> {
    [
        ("oben", top_id()),
        ("links", left_id()),
        ("rechts", right_id()),
        ("unten", bottom_id()),
        ("Status", status_id()),
        ("Hinweis", banner_id()),
        ("Leiste", sketch_strip_id()),
        ("Skizze", sketch_side_id()),
        ("Fuß", sketch_foot_id()),
        ("Werkzeugfeld", drawing_tool_id()),
        ("Ansichtsfeld", drawing_view_id()),
        ("Hinweisfeld", drawing_hint_id()),
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
    // Platz unter den Tabs für den Rollbalken: der untere Innenrand des Felds.
    let below_tabs = f32::from(frame.inner_margin.bottom);
    field(ctx, top_id(), rects.top.min, frame, |ui| {
        ui.set_width(inner.x);
        ui.set_height(inner.y);
        ui.horizontal_centered(|ui| {
            // Kleines Programmzeichen links vom Menü; sein Platz (mit Abstand) zählt zur
            // Wunsch- und Mindestbreite des Felds.
            // Es steht nur da, wenn das ganze Feld damit ohne Rollen zwischen die Spalten passt
            // (`shell::top_icon_fits`); vor der ersten Messung fehlt es.
            let lead_w = logo::SMALL_SIZE + ui.spacing().item_spacing.x;
            let fits = stored(ctx, top_base_key())
                .is_some_and(|base| shell::top_icon_fits(ctx.content_rect().width(), base, lead_w));
            let image = if fits {
                logo::image(ctx, "top-64", logo::ICON_64, logo::SMALL_SIZE)
            } else {
                None
            };
            if let Some(image) = image {
                ui.add(image);
            }
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
            // Die Tab-Leiste reicht bis in den unteren Rand des Felds: ihr schwebender Rollbalken
            // liegt dort, unter den Namen der Tabs statt über ihnen. Das Feld selbst wird dadurch
            // nicht höher (eigene Fläche, belegt wird nur die Zeilenhöhe).
            let row = ui.max_rect();
            let area_rect = egui::Rect::from_min_max(
                egui::pos2(ui.cursor().left(), row.top()),
                egui::pos2(row.right(), row.bottom() + below_tabs),
            );
            let mut strip = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(area_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Min)),
            );
            let area = egui::ScrollArea::horizontal()
                .id_salt("planora-field-tabs")
                .auto_shrink([true, true]);
            let tabs = theme::scroll_area(&mut strip, area, |ui| {
                ui.set_max_width(TABS_UNWRAPPED);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(no_prompt, |ui| {
                            tabs::show_revealing(ui, &mut app.ws, reveal)
                        });
                    });
                    ui.add_space(below_tabs);
                });
            });
            ui.advance_cursor_after_rect(egui::Rect::from_min_size(
                area_rect.min,
                vec2(tabs.inner_rect.width(), row.height()),
            ));
            // Wunschbreite: Menüknöpfe, Trennstrich mit Abständen, alle Tabs, Rand.
            let gap = tabs.inner_rect.left() - menu.response.rect.right();
            remember_width(ui, menu_width_key(), menu_natural);
            // Das Zeichen zählt nach der Regel mit, auch wenn es in diesem Durchgang noch fehlt:
            // so stehen die gemerkten Breiten schon nach dem ersten Durchgang fest (kein Sprung).
            let base = menu_natural + gap + tabs.content_size.x + margin;
            let planned = if shell::top_icon_fits(ctx.content_rect().width(), base, lead_w) {
                lead_w
            } else {
                0.0
            };
            remember_width(
                ui,
                top_min_key(),
                planned + menu_natural + gap + MIN_TABS_W + margin,
            );
            remember_width(ui, top_base_key(), base);
            remember_width(ui, top_width_key(), planned + base);
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
    let margin = f32::from(frame.inner_margin.left);
    field(ctx, left_id(), rects.left.min, frame, |ui| {
        // Feste Breite und Höchsthöhe aus der Geometrie, nicht aus der Feldgröße des letzten
        // Bildes (das gäbe eine Rückkopplung).
        side_column(ui, inner, margin, |ui| {
            let area = egui::ScrollArea::vertical()
                .id_salt("planora-field-left-scroll")
                .max_height(inner.y)
                .auto_shrink([false, true]);
            theme::scroll_area(ui, area, |ui| {
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
    });
}

/// Umschalter Eigenschaften | Versionen (nur wenn Versionen eingeblendet sind) und darunter der
/// Inhalt in einer senkrechten ScrollArea. Schreibgeschützt sind die Eigenschaften gesperrt.
fn right_field(ctx: &egui::Context, rects: &ShellRects, app: &mut PlanoraApp) {
    let dark = app.ws.active_tab().state.dark;
    let frame = theme::panel_frame(dark);
    let inner = inner_size(&frame, rects.right.size());
    let enabled = !app.ws.active_tab().read_only;
    let margin = f32::from(frame.inner_margin.left);
    field(ctx, right_id(), rects.right.min, frame, |ui| {
        side_column(ui, inner, margin, |ui| right_contents(ui, app, enabled));
    });
}

/// Inhalt des rechten Felds (siehe [`right_field`]).
fn right_contents(ui: &mut egui::Ui, app: &mut PlanoraApp, enabled: bool) {
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
    let area = egui::ScrollArea::vertical()
        .id_salt(("planora-field-right-scroll", tab == RightTab::Versions))
        .max_height(ui.available_height())
        .auto_shrink([false, true]);
    theme::scroll_area(ui, area, |ui| match tab {
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
            versions_panel::show_with_header(ui, &mut app.ws, &mut app.versions_ui, !switch);
        }
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
/// der Menüeintrag und ist wie das Menü gesperrt (alte Version, offene Rückfrage). Im Skizzen-Modus
/// gibt es das Feld nicht (dort steht das Fußfeld der Skizze). Zu schmal für alle Knöpfe, rollt
/// der Inhalt waagerecht.
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
                    let mut items: Vec<(&str, icons::IconSource, Command, Option<&'static str>)> =
                        CreateKind::ALL
                            .iter()
                            .map(|k| {
                                let own = app::create_blocked(&app.ws, *k);
                                (k.label(), k.icon(), Command::Create(*k), own)
                            })
                            .collect();
                    for (label, source, chamfer) in icons::blend_entries() {
                        items.push((label, source, Command::Blend(chamfer), blend_blocked));
                    }
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

/// Text und Farbe der Statuszeile. Vorrang: letzte Meldung (Fehlerfarbe), Hinweis `info`
/// (text_secondary; im Skizzen-Modus hinter der Zeile „Skizzen-Modus: …“, damit diese sichtbar
/// bleibt), Skizzen-Modus (text_secondary), erstes Feature mit Fehler (Fehlerfarbe), Körperzahl
/// (mit Hinweis bei ungenauem Netz in der Warnfarbe, sonst text_secondary).
fn status_line(state: &AppState, dark: bool) -> (String, egui::Color32) {
    let p = theme::palette(dark);
    if state.message.is_none()
        && let Some(info) = &state.info
    {
        return match state.session_sketch() {
            Some(sk) => (
                format!("Skizzen-Modus: {} · {info}", sk.name),
                p.text_secondary,
            ),
            None => (info.clone(), p.text_secondary),
        };
    }
    match (
        &state.message,
        state.session_sketch(),
        browser::status_text(state),
    ) {
        (Some(m), _, _) => (m.clone(), p.error),
        (None, Some(sk), _) => (format!("Skizzen-Modus: {}", sk.name), p.text_secondary),
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
        (text, color, None),
    );
}
