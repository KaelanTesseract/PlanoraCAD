//! Zeichnungsbereich (Teilprojekt 11, Aufgabe 15; Maße und Mittellinien Teilprojekt 12,
//! Aufgabe 11; schwebende Felder Graphit Etappe C, Aufgabe 4): Werkzeugfeld, Ansichtsfeld,
//! Blatt mit den Linien der Ansichten und ihren Markierungen (Maße, Mittellinien), Auswahl,
//! Verschieben der Ansichten und Ziehen der Maße, Zoom und Verschieben des Blatts sowie das
//! Hinweisfeld.
//!
//! Blattkoordinaten in Millimetern, y nach oben, Ursprung unten links. Auf dem Bildschirm liegt
//! die Blattmitte in der Mitte der freien Fläche (`DrawingLayout::canvas`) plus `pan`;
//! `zoom = 1` heißt „Blatt in die freie Fläche eingepasst“. Gemalt wird das Blatt über das ganze
//! Fenster (`DrawingLayout::paint`), auch unter den Feldern.
//!
//! **Ziehen eines Maßes** (nur bearbeitbar; Änderung über `Document::update_dimension`, der
//! Modellstand bleibt): Maßlinie eines linearen Maßes → `offset` um die Bewegung quer zur
//! Messrichtung (positiv nach oben bzw. rechts wie im Layout); Text → `text_shift` um die
//! Bewegung (Blatt-mm); Ø/R-Führungslinie → der Knick folgt dem Zeiger, daraus `angle` (Grad)
//! und `offset` (Abstand vom Kreis); Ø-Linie durch die Mitte → `angle` dreht mit; Bogen eines
//! Winkelmaßes → Radius (`|offset|`) wächst mit der Bewegung vom Scheitel weg.

use crate::balloon_tool;
use crate::browser::error_color;
use crate::dimension_state::{SuggestOutcome, marks_in, notes_in, resolve_in, suggest_for};
use crate::dimension_tool::{END_PX, Hit, ToolError, ToolMode, with_tool};
use crate::drawing_state::{
    FreeArea, ViewResult, WorkArea, block_origin, default_distance, estimate_view_size,
    iso_placement, keep_in_free, suggest_scale_in,
};
use crate::export_dialog::{REPAINT, shorten_for_display};
use crate::icons::{self, Icon};
use crate::msg::{Msg, MsgArg};
use crate::section_tool;
use crate::shell::{self, ShellMode, ShellRects, ShellState};
use crate::state::AppState;
use crate::{shell_panels, theme, viewport};
use egui::epaint::TextShape;
use egui::{
    Color32, FontId, Galley, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind,
    UiBuilder, pos2, vec2,
};
use planora_drawing::annotate::{
    ARROW_HALF_W, ARROW_LEN, CENTER_PATTERN, CENTER_W, DEFAULT_OFFSET, DIM_LINE_W, Mark, MarkKind,
    ResolvedDims, SECTION_END_W, SectionPen, TextAnchor, arrow_size, clamp_leader_deg,
    leader_angle,
};
use planora_drawing::balloon::{BalloonMarks, add_balloon_marks};
use planora_drawing::dimension::{ArcSpan, DimId, DimSpec};
use planora_drawing::out2d::Out2d;
use planora_drawing::parts::{PARTS_ON_SHEET, page_rows};
use planora_drawing::section_marks::{add_section_marks, section_marks};
use planora_drawing::sheet::{DimMarks, SheetModel, build_sheet_with, view_transform};
use planora_drawing::sheet_frame::{
    PartsSpace, REV_VISIBLE, SheetLayout, parts_rows_per_page, sheet_layout_with, sheet_scale,
};
use planora_drawing::sheet_info::{
    ProjectInfo, SheetContext, TitleTexts, main_scale, resolve_texts,
};
use planora_drawing::sheet_marks::sheet_frame;
use planora_drawing::stroke::StrokeOrigin;
use planora_drawing::{
    Anchor, BalloonId, Drawing, DrawingId, DrawingKind, PartsCells, Placement, Projection, Scale,
    SectionData, Sheet, ViewId, ViewKey, ViewKind, ViewLines, ViewSpec, Visibility, align_dir,
    view_extent,
};
use planora_i18n::tr;
use planora_math::DVec2;
use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Breite sichtbarer Linien in Blatt-mm.
pub const VISIBLE_MM: f64 = 0.5;
/// Breite verdeckter Linien in Blatt-mm.
pub const HIDDEN_MM: f64 = 0.25;
/// Strichlänge verdeckter Linien in Blatt-mm.
pub const DASH_MM: f64 = 3.0;
/// Lücke verdeckter Linien in Blatt-mm.
pub const DASH_GAP_MM: f64 = 1.0;
/// Größter Abstand eines Klicks zu einer Linie, der die Ansicht wählt (Pixel).
pub const PICK_PX: f32 = 6.0;
/// Rand um das eingepasste Blatt (Pixel).
pub const FIT_MARGIN_PX: f32 = 24.0;
/// Grenzen des Zooms (relativ zum eingepassten Blatt).
pub const MIN_ZOOM: f32 = 0.05;
pub const MAX_ZOOM: f32 = 400.0;
/// Faktor der Lupen-Schaltflächen.
const ZOOM_STEP: f32 = 1.25;
/// Zoom je Punkt Mausrad.
const WHEEL_RATE: f32 = 0.0015;
/// Höchstabweichung der Polylinie von der Kurve (Pixel, unter der Grenze von 1 px).
const FLATTEN_TOL_PX: f64 = 0.9;
/// Höchstzahl der Stücke einer Kurve.
const MAX_SEGMENTS: usize = 4096;
/// Rand des Auswahlrahmens um die Ansicht (Blatt-mm).
const SELECT_PAD_MM: f64 = 3.0;
/// Kleinste Seitenlänge des Platzhalters einer Ansicht ohne Linien (Blatt-mm).
const PLACEHOLDER_MIN_MM: f64 = 30.0;
/// Schriftgröße der Texte auf dem Blatt.
const SHEET_FONT: f32 = 13.0;
/// Kleinste Texthöhe auf dem Bildschirm, ab der Maßtexte gezeichnet werden (Pixel).
const MIN_TEXT_PX: f64 = 1.0;
/// Größte Schriftgröße der Maßtexte (Pixel); bei stärkerer Vergrößerung bleibt der Text so
/// groß (die Schrift wird nicht beliebig groß gerastert).
const MAX_TEXT_PX: f64 = 400.0;
/// Kleinster Betrag des Abstands einer Maßlinie beim Ziehen (Blatt-mm, auf der Seite, auf der
/// das Ziehen begann).
pub const MIN_DRAG_OFFSET_MM: f64 = 1.0;
/// Name (Tooltip) des Symbolknopfs „Blatt einpassen“.
pub fn fit_sheet() -> String {
    tr!("drawing-fit-sheet")
}
/// Kleinster Radius eines Winkelmaßes beim Ziehen (Blatt-mm).
const MIN_ANGLE_RADIUS_MM: f64 = 1.0;
/// Hinweis, wenn es zu den Körpern nichts zu sagen gibt.
pub fn default_hint() -> String {
    tr!("drawing-hint")
}
/// Beschriftung des Schalters für die Mittellinien der gewählten Ansicht.
pub fn centerlines_label() -> String {
    tr!("drawing-centerlines")
}
/// Erklärung am gesperrten Schalter „Mittellinien“ einer isometrischen Ansicht.
pub fn no_iso_centerlines() -> String {
    tr!("drawing-no-iso-centerlines")
}
/// Hinweis im Ansichtsfeld, solange keine Ansicht gewählt ist.
pub fn no_view_selected() -> String {
    tr!("drawing-no-view-selected")
}
/// Beschriftung des Knopfs für den Maßvorschlag.
pub fn suggest() -> String {
    tr!("drawing-suggest")
}
/// Satzanfang des Tooltips eines Maßes, das seinen Bezug verloren hat.
pub fn lost_tip() -> String {
    tr!("drawing-lost-tip")
}
/// Meldung, wenn die Ausgangsansicht einer abgeleiteten Ansicht nicht gezeichnet werden kann.
pub fn parent_missing() -> Msg {
    crate::msg!("drawing-parent-missing")
}
/// Meldung ohne geöffnete Zeichnung.
fn no_drawing_open() -> Msg {
    crate::msg!("drawing-none-open")
}
/// Hinweis am gesperrten „Ansicht hinzufügen“ eines Stücklistenblatts.
pub fn no_views_on_parts_sheet() -> Msg {
    crate::msg!("drawing-no-views-on-parts-sheet")
}

/// Was an der Stelle einer Ansicht zu sehen ist.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawnState {
    Lines,
    /// Wird berechnet (Anteil 0…1).
    Pending(f32),
    /// Meldung statt Linien.
    Failed(String),
}

/// Eine Ansicht im zuletzt gezeichneten Bild.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawnView {
    pub id: ViewId,
    /// Hüllbox auf dem Bildschirm (ohne Auswahlrand).
    pub rect: Rect,
    pub state: DrawnState,
    /// Fortschrittsbalken: ganzer Balken und gefüllter Teil.
    pub bar: Option<(Rect, Rect)>,
}

/// Aufteilung und Kennzahlen des zuletzt gezeichneten Zeichnungsbereichs (für Tests).
#[derive(Clone, Debug, PartialEq)]
pub struct DrawingLayout {
    /// Ganzer Bereich (in der App: das Fenster).
    pub area: Rect,
    /// Werkzeugfeld.
    pub toolbar: Rect,
    /// Fläche, in die das Blatt eingepasst und in der es zentriert wird (die freie Fläche
    /// zwischen den Feldern); alle Umrechnungen Blatt ↔ Bildschirm beziehen sich auf sie.
    pub canvas: Rect,
    /// Fläche, auf die das Blatt gemalt wird und die Eingaben annimmt (das ganze Fenster; die
    /// Felder liegen darüber).
    pub paint: Rect,
    /// Hinweisfeld.
    pub hint: Rect,
    pub sheet_mm: (f64, f64),
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub px_per_mm: f32,
    /// Gezeichnete Blattlinien (je Linie einmal, auch wenn gestrichelt).
    pub lines_drawn: usize,
    /// Gezeichnete Markierungen (Maße, Mittellinien; je Markierung einmal).
    pub marks_drawn: usize,
    /// Gezeichnete Markierungen der Vorschau des Werkzeugs „Maß“.
    pub preview_drawn: usize,
    pub views: Vec<DrawnView>,
    /// Die Nummernkreise, wie sie gezeichnet sind (beim Ziehen an der neuen Stelle).
    pub balloons: Vec<BalloonMarks>,
}

fn layout_id() -> egui::Id {
    egui::Id::new("planora-drawing-layout")
}

fn drag_id() -> egui::Id {
    egui::Id::new("planora-drawing-drag")
}

fn sheet_cache_id() -> egui::Id {
    egui::Id::new("planora-drawing-sheet")
}

/// Aufteilung des zuletzt gezeichneten Zeichnungsbereichs.
pub fn last_layout(ctx: &egui::Context) -> Option<DrawingLayout> {
    ctx.data(|d| d.get_temp(layout_id()))
}

/// Pixel je Blatt-mm bei `zoom = 1` (Blatt mit Rand eingepasst).
pub fn fit_px_per_mm(canvas: Rect, sheet_mm: (f64, f64)) -> f32 {
    let (w, h) = sheet_mm;
    let fx = (canvas.width() - 2.0 * FIT_MARGIN_PX) / w as f32;
    let fy = (canvas.height() - 2.0 * FIT_MARGIN_PX) / h as f32;
    fx.min(fy).max(1e-3)
}

/// Bildschirmpunkt eines Blattpunkts (mm).
pub fn sheet_to_screen(
    canvas: Rect,
    sheet_mm: (f64, f64),
    zoom: f32,
    pan: egui::Vec2,
    p: DVec2,
) -> Pos2 {
    let k = f64::from(fit_px_per_mm(canvas, sheet_mm) * zoom);
    let c = canvas.center() + pan;
    pos2(
        (f64::from(c.x) + (p.x - sheet_mm.0 / 2.0) * k) as f32,
        (f64::from(c.y) - (p.y - sheet_mm.1 / 2.0) * k) as f32,
    )
}

/// Blattpunkt (mm) eines Bildschirmpunkts.
pub fn screen_to_sheet(
    canvas: Rect,
    sheet_mm: (f64, f64),
    zoom: f32,
    pan: egui::Vec2,
    p: Pos2,
) -> DVec2 {
    let k = f64::from(fit_px_per_mm(canvas, sheet_mm) * zoom);
    let c = canvas.center() + pan;
    DVec2::new(
        sheet_mm.0 / 2.0 + f64::from(p.x - c.x) / k,
        sheet_mm.1 / 2.0 - f64::from(p.y - c.y) / k,
    )
}

/// Stückzahl für einen Bogen mit Radius `r_px` (Pixel) und Öffnungswinkel `sweep`, so dass
/// die Sehnen höchstens [`FLATTEN_TOL_PX`] abweichen.
fn arc_segments(r_px: f64, sweep: f64) -> usize {
    let sweep = sweep.abs();
    if !(r_px.is_finite() && sweep.is_finite()) {
        return 1;
    }
    let step = if r_px <= FLATTEN_TOL_PX {
        PI / 2.0
    } else {
        2.0 * (1.0 - FLATTEN_TOL_PX / r_px).acos()
    };
    ((sweep / step).ceil() as usize).clamp(1, MAX_SEGMENTS)
}

fn uniform(c: &Out2d, t0: f64, t1: f64, n: usize) -> Vec<DVec2> {
    (0..=n)
        .map(|i| c.point(t0 + (t1 - t0) * i as f64 / n as f64))
        .collect()
}

fn seg_dist(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

thread_local! {
    static FLATTEN_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Zahl der [`flatten`]-Aufrufe in diesem Thread (nur für Tests: ein Bild ohne Änderung darf
/// nichts zerlegen).
#[doc(hidden)]
pub fn flatten_calls() -> u64 {
    FLATTEN_CALLS.with(std::cell::Cell::get)
}

/// Zerlegt eine Kurve (Blatt-mm) in eine Polylinie, die bei `px_per_mm` höchstens 1 px von
/// der Kurve abweicht. Die Stückzahl folgt aus Länge bzw. Krümmung und Zoom. Ungültige Kurven
/// liefern keine Punkte.
pub fn flatten(curve: &Out2d, px_per_mm: f64) -> Vec<DVec2> {
    FLATTEN_CALLS.with(|c| c.set(c.get() + 1));
    if !curve.is_finite() || !(px_per_mm.is_finite() && px_per_mm > 0.0) {
        return Vec::new();
    }
    match curve {
        Out2d::Line { a, b } => vec![*a, *b],
        Out2d::Arc { r, start, end, .. } => uniform(
            curve,
            *start,
            *end,
            arc_segments(r * px_per_mm, end - start),
        ),
        Out2d::Ellipse {
            major,
            ratio,
            start,
            end,
            ..
        } => {
            // Die Ellipse ist das affine Bild eines Kreises; die Abweichung ist höchstens die
            // eines Kreises mit dem größeren Halbmesser.
            let r = major.length() * ratio.abs().max(1.0);
            uniform(
                curve,
                *start,
                *end,
                arc_segments(r * px_per_mm, end - start),
            )
        }
        Out2d::Spline { degree, knots, .. } => {
            let n = knots.len();
            if n < 2 * degree + 2 {
                return Vec::new();
            }
            let (t0, t1) = (knots[*degree], knots[n - 1 - degree]);
            flatten_adaptive(curve, t0, t1, px_per_mm)
        }
    }
}

/// Spline: gleichmäßiger Anfang (Anzahl aus Länge·Zoom), dann teilen, bis drei Prüfpunkte je
/// Stück höchstens die halbe Toleranz von der Sehne abweichen.
fn flatten_adaptive(c: &Out2d, t0: f64, t1: f64, k: f64) -> Vec<DVec2> {
    let len_px = c.length() * k;
    let n0 = if len_px.is_finite() {
        ((len_px / 32.0).ceil() as usize).clamp(4, 256)
    } else {
        4
    };
    let tol = 0.5 * FLATTEN_TOL_PX / k;
    let mut out = vec![c.point(t0)];
    for i in 0..n0 {
        let a = t0 + (t1 - t0) * i as f64 / n0 as f64;
        let b = t0 + (t1 - t0) * (i + 1) as f64 / n0 as f64;
        // Stapel von Teilstücken, links zuerst.
        let mut stack = vec![(a, b, 0u32)];
        while let Some((a, b, depth)) = stack.pop() {
            let (pa, pb) = (c.point(a), c.point(b));
            let worst = [0.25, 0.5, 0.75]
                .iter()
                .map(|f| seg_dist(c.point(a + (b - a) * f), pa, pb))
                .fold(0.0, f64::max);
            if worst > tol && depth < 24 && out.len() < MAX_SEGMENTS {
                let m = 0.5 * (a + b);
                stack.push((m, b, depth + 1));
                stack.push((a, m, depth + 1));
            } else {
                out.push(pb);
            }
        }
    }
    out
}

/// Teilt eine Polylinie in Striche der Länge `dash` mit Lücken `gap` (gleiche Einheit); das
/// Muster läuft über Ecken hinweg weiter.
pub fn dashes(points: &[DVec2], dash: f64, gap: f64) -> Vec<Vec<DVec2>> {
    dash_pattern(points, &[dash, gap])
}

/// Teilt eine Polylinie nach einem Strichmuster `[Strich, Lücke, Strich, Lücke, …]` (gleiche
/// Einheit wie die Punkte, z. B. das Strichpunkt-Muster `[12, 3, 2, 3]` der Mittellinien); das
/// Muster läuft über Ecken hinweg weiter. Ein ungültiges Muster (ungerade Länge, Werte nicht
/// positiv oder nicht endlich) ergibt die ganze Linie.
pub fn dash_pattern(points: &[DVec2], pattern: &[f64]) -> Vec<Vec<DVec2>> {
    if points.len() < 2 {
        return Vec::new();
    }
    if pattern.is_empty()
        || !pattern.len().is_multiple_of(2)
        || pattern.iter().any(|v| !(v.is_finite() && *v > 0.0))
    {
        return vec![points.to_vec()];
    }
    const EPS: f64 = 1e-12;
    let mut out = Vec::new();
    let mut cur = vec![points[0]];
    // Stück des Musters (gerade: Strich, ungerade: Lücke) und Länge darin.
    let mut idx = 0usize;
    let mut phase = 0.0;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        if len <= 0.0 || !len.is_finite() {
            continue;
        }
        let dir = (b - a) / len;
        let mut done = 0.0;
        while len - done > EPS {
            let on = idx.is_multiple_of(2);
            let step = (pattern[idx] - phase).min(len - done);
            done += step;
            phase += step;
            let p = if len - done <= EPS { b } else { a + dir * done };
            if on {
                cur.push(p);
            }
            if phase >= pattern[idx] - EPS {
                if on {
                    out.push(std::mem::take(&mut cur));
                }
                idx = (idx + 1) % pattern.len();
                phase = 0.0;
                if idx.is_multiple_of(2) {
                    cur = vec![p];
                }
            }
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// Name einer Ansichtsart in der Oberfläche.
pub fn view_kind_msg(kind: ViewKind) -> Msg {
    match kind {
        ViewKind::Front => crate::msg!("drawing-view-front"),
        ViewKind::Back => crate::msg!("drawing-view-back"),
        ViewKind::Left => crate::msg!("drawing-view-left"),
        ViewKind::Right => crate::msg!("drawing-view-right"),
        ViewKind::Top => crate::msg!("drawing-view-top"),
        ViewKind::Bottom => crate::msg!("drawing-view-bottom"),
        ViewKind::Isometric => crate::msg!("drawing-view-isometric"),
    }
}

/// Wie [`view_kind_msg`], als Text in der aktuellen Sprache.
pub fn view_kind_label(kind: ViewKind) -> String {
    view_kind_msg(kind).text()
}

/// Namen der Ansichten wie auf dem Blatt: Art, bei mehrfach gleicher Art mit Nummer, Schnitte
/// als „Schnitt A–A“ (wie [`Drawing::view_names`], aber übersetzt erst beim Zeichnen).
pub fn view_name_msgs(drawing: &Drawing) -> BTreeMap<ViewId, Msg> {
    let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for v in &drawing.views {
        if let Some(sec) = &v.section {
            out.insert(
                v.id,
                crate::msg!("drawing-view-section", letter = sec.letter.to_string()),
            );
            continue;
        }
        let i = ViewKind::ALL.iter().position(|k| *k == v.kind).unwrap_or(0);
        let n = counts.entry(i).or_insert(0);
        *n += 1;
        let name = if *n == 1 {
            view_kind_msg(v.kind)
        } else {
            crate::msg!(
                "drawing-view-numbered",
                name = MsgArg::from(view_kind_msg(v.kind)),
                n = *n
            )
        };
        out.insert(v.id, name);
    }
    out
}

/// Namen der Ansichten in der aktuellen Sprache (siehe [`view_name_msgs`]).
pub fn view_names(drawing: &Drawing) -> BTreeMap<ViewId, String> {
    view_name_msgs(drawing)
        .into_iter()
        .map(|(id, m)| (id, m.text()))
        .collect()
}

/// Mitte einer Ansicht auf dem Blatt (auch wenn noch nichts berechnet ist).
fn center_of(drawing: &Drawing, id: ViewId, depth: usize) -> Option<DVec2> {
    let v = drawing.view(id)?;
    let c = match v.placement {
        Placement::Free { x, y } => DVec2::new(x, y),
        Placement::Aligned { distance } => {
            let parent = drawing.view(v.parent?)?;
            let (dx, dy) = align_dir(drawing.projection, parent.kind, v.kind)?;
            center_of(drawing, parent.id, depth.checked_sub(1)?)? + DVec2::new(dx, dy) * distance
        }
    };
    c.is_finite().then_some(c)
}

/// Ansicht mit Lage und Zustand für dieses Bild.
struct Placed {
    id: ViewId,
    name: String,
    /// Hüllbox auf dem Blatt (mm): min, max.
    min: DVec2,
    max: DVec2,
    state: DrawnState,
}

/// Fortschritt der nächsten noch rechnenden Ausgangsansicht.
fn pending_ancestor(
    drawing: &Drawing,
    id: ViewId,
    results: &[(ViewId, ViewResult)],
) -> Option<f32> {
    let mut cur = drawing.view(id)?.parent;
    for _ in 0..drawing.views.len() {
        let p = cur?;
        if let Some((_, ViewResult::Pending { fraction })) = results.iter().find(|(v, _)| *v == p) {
            return Some(*fraction);
        }
        cur = drawing.view(p)?.parent;
    }
    None
}

fn place_views(state: &AppState, drawing: &Drawing, sheet: &SheetModel, model: u64) -> Vec<Placed> {
    let names = view_names(drawing);
    let results: Vec<(ViewId, ViewResult)> = drawing
        .views
        .iter()
        .map(|v| (v.id, state.drawing_ui.result_of(v, model)))
        .collect();
    let mut out = Vec::new();
    for (v, (_, result)) in drawing.views.iter().zip(&results) {
        let Some(center) = center_of(drawing, v.id, drawing.views.len()) else {
            continue;
        };
        let drawn = sheet.views.iter().find(|s| s.id == v.id);
        let (size, st) = match (result, drawn) {
            (ViewResult::Ready(_), Some(s)) => (s.extent_mm, DrawnState::Lines),
            (ViewResult::Ready(lines), None) => {
                let st = match pending_ancestor(drawing, v.id, &results) {
                    Some(f) => DrawnState::Pending(f),
                    None => DrawnState::Failed(parent_missing().text()),
                };
                (placeholder(view_extent(lines.bbox, v.scale)), st)
            }
            (ViewResult::Pending { fraction }, _) => (
                placeholder(estimated(state, v.kind, v.scale)),
                DrawnState::Pending(*fraction),
            ),
            (ViewResult::Failed(m), _) => (
                placeholder(estimated(state, v.kind, v.scale)),
                DrawnState::Failed(m.text()),
            ),
        };
        let half = DVec2::new(size.0, size.1) * 0.5;
        out.push(Placed {
            id: v.id,
            name: names.get(&v.id).cloned().unwrap_or_default(),
            min: center - half,
            max: center + half,
            state: st,
        });
    }
    out
}

fn estimated(state: &AppState, kind: ViewKind, scale: Scale) -> (f64, f64) {
    let (w, h) = estimate_view_size(&state.bodies, kind);
    (w * scale.factor(), h * scale.factor())
}

fn placeholder(size: (f64, f64)) -> (f64, f64) {
    let fix = |v: f64| {
        if v.is_finite() {
            v.max(PLACEHOLDER_MIN_MM)
        } else {
            PLACEHOLDER_MIN_MM
        }
    };
    (fix(size.0), fix(size.1))
}

#[derive(Clone)]
struct SheetCache {
    /// Zustand (Tab), dessen Zeichnung hier liegt ([`AppState::instance`]).
    instance: u64,
    drawing: Drawing,
    model: u64,
    ptrs: Vec<usize>,
    /// Aufgelöste Maße (festgehalten, damit ihre Adresse eindeutig bleibt).
    dims: Arc<ResolvedDims>,
    sheet: Arc<SheetModel>,
    /// Angaben, die in die Texte des Schriftfelds eingehen.
    project: Option<ProjectInfo>,
    context: Option<SheetContext>,
    /// Zeilen der Stückliste auf diesem Blatt (sie hängen an Körpern, Namen und eigenen Texten,
    /// nicht an der Zeichnung).
    parts: Vec<PartsCells>,
    /// Nummernkreise mit Nummern (die Nummern folgen der Stückliste, nicht der Zeichnung).
    balloons: Arc<Vec<BalloonMarks>>,
    /// Stand der Linien: bleibt gleich, solange sich nur Maße oder Mittellinien ändern (die
    /// Linienzüge werden dann nicht neu zerlegt).
    lines_gen: u64,
    /// Meldungen zu den Schnittansichten (zu feine Schraffur u. Ä.): sperren die Ausgabe.
    section_errors: Arc<Vec<String>>,
}

/// Fortlaufende Nummer der Linienstände (eindeutig im Programm).
static LINES_GEN: AtomicU64 = AtomicU64::new(1);

/// Haben zwei Stände der Zeichnung dieselben Linien (Blatt, Projektion, Ansichten bis auf den
/// Schalter der Mittellinien und die Angaben der Schnittansicht; Maße zählen nicht)? Bei einer
/// Schnittansicht zählt nur der Schlüssel ([`ViewKey::of`]: Art und Lage der Ebene): Schraffur
/// und Buchstabe ändern keine Kantenlinien.
fn same_lines(a: &Drawing, b: &Drawing) -> bool {
    a.sheet == b.sheet
        && a.projection == b.projection
        && a.views.len() == b.views.len()
        && a.views.iter().zip(&b.views).all(|(x, y)| {
            ViewKey::of(x) == ViewKey::of(y)
                && ViewSpec {
                    centerlines: y.centerlines,
                    section: y.section,
                    ..x.clone()
                } == *y
        })
}

/// Zeiger der Ergebnisse einer Zeichnung für den Vergleich im Zwischenspeicher: erst die der
/// Linien, dann (nach einer Null) die der Schnittdaten. Ein neues Ergebnis hat eine neue
/// Adresse, ein altes bleibt nie stehen.
fn result_ptrs(
    lines: &BTreeMap<ViewId, Arc<ViewLines>>,
    sections: &BTreeMap<ViewId, Arc<SectionData>>,
) -> Vec<usize> {
    lines
        .values()
        .map(|a| Arc::as_ptr(a) as usize)
        .chain(std::iter::once(0))
        .chain(sections.values().map(|a| Arc::as_ptr(a) as usize))
        .collect()
}

/// Stellung der Zeichnung `id` im Dokument: Platz (ab 1) und Anzahl der Zeichnungen sowie der
/// Name des Dokuments.
pub fn sheet_context(state: &AppState, id: DrawingId) -> Option<SheetContext> {
    let all = state.document.drawings();
    let index = all.iter().position(|d| d.id == id)? + 1;
    Some(SheetContext {
        doc_name: state.document_name.clone(),
        index,
        count: all.len(),
    })
}

/// Stellung für eine Zeichnung, die (noch) nicht im Dokument steht: einziges Blatt.
pub fn context_or_single(state: &AppState, id: DrawingId) -> SheetContext {
    sheet_context(state, id).unwrap_or_else(|| SheetContext {
        doc_name: state.document_name.clone(),
        index: 1,
        count: 1,
    })
}

/// Platz der Stückliste auf dem Blatt der Zeichnung: auf einem Stücklistenblatt die ganze
/// Tabelle ([`PartsSpace::ListPage`]); auf einer Zeichnung mit Ansichten und eingeschaltetem
/// Schalter „Stückliste auf diesem Blatt“ die Tabelle über dem Schriftfeld mit den tatsächlichen
/// Zeilen (`placement = false`, so wird gezeichnet) bzw. immer mit [`PARTS_ON_SHEET`] Zeilen
/// (`placement = true`, für Anordnung und Maßstabsvorschlag: eine später längere Liste überdeckt
/// keine automatisch gelegte Ansicht); sonst keine.
///
/// Der Schalter eines Stücklistenblatts zählt nicht (eine Datei kann ihn gesetzt haben; das
/// Blatt ist ohnehin ganz Stückliste).
pub fn parts_space(state: &AppState, drawing: &Drawing, placement: bool) -> PartsSpace {
    // Die Zeilen nur zählen, wenn sie gebraucht werden.
    match parts_space_for(drawing, 0, true) {
        PartsSpace::OnSheet { .. } if !placement => {
            parts_space_for(drawing, state.document.parts_rows().len(), false)
        }
        space => space,
    }
}

/// Wie [`parts_space`] mit der schon bekannten Zahl der Zeilen `rows`.
pub fn parts_space_for(drawing: &Drawing, rows: usize, placement: bool) -> PartsSpace {
    match drawing.kind {
        DrawingKind::PartsList { .. } => PartsSpace::ListPage,
        DrawingKind::Views if drawing.info.as_ref().is_some_and(|i| i.parts_list) => {
            let rows = if placement { PARTS_ON_SHEET } else { rows };
            PartsSpace::OnSheet { rows }
        }
        DrawingKind::Views => PartsSpace::None,
    }
}

/// Die Zeilen der Stückliste (aus allen Zeilen `cells`), die auf dem Blatt der Zeichnung stehen:
/// auf dem Zeichnungsblatt die ersten [`PARTS_ON_SHEET`], auf einem Stücklistenblatt die seiner
/// Seite, sonst keine.
pub(crate) fn parts_on(drawing: &Drawing, cells: &[PartsCells]) -> Vec<PartsCells> {
    match parts_space_for(drawing, cells.len(), false) {
        PartsSpace::None => Vec::new(),
        PartsSpace::OnSheet { .. } => cells[..cells.len().min(PARTS_ON_SHEET)].to_vec(),
        PartsSpace::ListPage => {
            let page = match drawing.kind {
                DrawingKind::PartsList { page } => page,
                DrawingKind::Views => 1,
            };
            page_rows(cells, page, parts_rows_per_page(drawing.sheet)).to_vec()
        }
    }
}

/// Die Texte des Schriftfelds der Zeichnung (Maßstab: der der ersten Ansicht) mit den Zeilen der
/// Stückliste dieses Blatts.
pub fn sheet_texts(state: &AppState, drawing: &Drawing) -> TitleTexts {
    let cells = if parts_space_for(drawing, 0, true) == PartsSpace::None {
        Vec::new()
    } else {
        state.document.parts_cells()
    };
    sheet_texts_with(state, drawing, &cells)
}

/// Wie [`sheet_texts`] mit allen Zeilen `cells` der Stückliste.
fn sheet_texts_with(state: &AppState, drawing: &Drawing, cells: &[PartsCells]) -> TitleTexts {
    let mut texts = resolve_texts(
        drawing,
        state.document.project_info(),
        &context_or_single(state, drawing.id),
        main_scale(drawing),
    );
    texts.parts = parts_on(drawing, cells);
    texts
}

/// Zwischenspeicher der Zeilen der Stückliste: sie hängen nur am Dokument und ändern sich nur
/// mit seinem Änderungszähler.
#[derive(Clone)]
struct CellsCache {
    instance: u64,
    edit: u64,
    cells: Arc<Vec<PartsCells>>,
}

/// Alle Zeilen der Stückliste des Dokuments, je Stand des Dokuments einmal gerechnet.
pub(crate) fn cached_cells(ctx: &egui::Context, state: &AppState) -> Arc<Vec<PartsCells>> {
    let id = egui::Id::new("planora-parts-cells");
    let edit = state.document.edit_count();
    if let Some(c) = ctx
        .data(|d| d.get_temp::<CellsCache>(id))
        .filter(|c| c.instance == state.instance && c.edit == edit)
    {
        return c.cells;
    }
    let cells = Arc::new(state.document.parts_cells());
    ctx.data_mut(|d| {
        d.insert_temp(
            id,
            CellsCache {
                instance: state.instance,
                edit,
                cells: Arc::clone(&cells),
            },
        )
    });
    cells
}

/// Die Blattgeometrie zu den Texten: die Zeilenzahlen von Änderungstabelle und Hinweisfeld
/// folgen ihnen (der Maßstab beeinflusst sie nicht), der Platz der Stückliste `parts`.
pub fn layout_from_texts(drawing: &Drawing, texts: &TitleTexts, parts: PartsSpace) -> SheetLayout {
    sheet_layout_with(
        drawing.sheet,
        texts.visible_revisions().len(),
        texts.notes.len(),
        parts,
    )
}

/// Die Blattgeometrie der Zeichnung (so wird sie gezeichnet).
pub fn layout_of(state: &AppState, drawing: &Drawing) -> SheetLayout {
    let texts = resolve_texts(
        drawing,
        state.document.project_info(),
        &context_or_single(state, drawing.id),
        None,
    );
    layout_from_texts(drawing, &texts, parts_space(state, drawing, false))
}

/// Blattgeometrie für die automatische Anordnung von Ansichten und den Maßstabsvorschlag: wie
/// [`layout_of`], aber die Änderungstabelle immer mit Kopf und allen [`REV_VISIBLE`] Zeilen und
/// eine Stückliste auf dem Blatt immer mit [`PARTS_ON_SHEET`] Zeilen, so dass später gefüllte
/// Tabellen keine automatisch gelegte Ansicht überdecken. Gezeichnet werden die Tabellen mit den
/// tatsächlichen Zeilen ([`layout_of`]).
pub fn placement_layout(state: &AppState, drawing: &Drawing) -> SheetLayout {
    let texts = resolve_texts(
        drawing,
        state.document.project_info(),
        &context_or_single(state, drawing.id),
        None,
    );
    sheet_layout_with(
        drawing.sheet,
        REV_VISIBLE,
        texts.notes.len(),
        parts_space(state, drawing, true),
    )
}

/// Die Blattgeometrie für die Anordnung in einer neuen Zeichnung auf `sheet`: mit den
/// Hinweiszeilen der Projektangaben (sind noch keine gesetzt, die der Vorgabe), Platz für die
/// volle Änderungstabelle und – wenn die neue Zeichnung die Stückliste bekommt (mehr als ein
/// Körper) – für die volle Stückliste (wie [`placement_layout`]).
pub fn new_sheet_layout(state: &AppState, sheet: Sheet, projection: Projection) -> SheetLayout {
    let blank = Drawing::new(DrawingId(0), String::new(), sheet, projection);
    let ctx = SheetContext {
        doc_name: state.document_name.clone(),
        index: 1,
        count: 1,
    };
    let texts = resolve_texts(&blank, state.document.project_info(), &ctx, None);
    // Dieselbe Vorgabe wie beim Anlegen (`Document::add_drawing_stamped`).
    let parts = if state.document.visible_bodies().len() > 1 {
        PartsSpace::OnSheet {
            rows: PARTS_ON_SHEET,
        }
    } else {
        PartsSpace::None
    };
    sheet_layout_with(sheet, REV_VISIBLE, texts.notes.len(), parts)
}

/// Zuletzt im Zeichnungsbereich gezeichnetes Blattmodell (mit Mittellinien und Maßen).
pub fn last_sheet(ctx: &egui::Context) -> Option<Arc<SheetModel>> {
    ctx.data(|d| d.get_temp::<SheetCache>(sheet_cache_id()))
        .map(|c| c.sheet)
}

/// Wie [`last_sheet`], aber nur, wenn das zuletzt gezeichnete Blatt das der Zeichnung `id` im
/// Dokument von `state` ist (der Zwischenspeicher gilt für das ganze Programm, nicht je
/// Zeichnung oder Tab; Zeichnungen verschiedener Tabs können dieselbe Kennung haben).
pub fn last_sheet_for(
    ctx: &egui::Context,
    state: &AppState,
    id: DrawingId,
) -> Option<Arc<SheetModel>> {
    ctx.data(|d| d.get_temp::<SheetCache>(sheet_cache_id()))
        .filter(|c| c.instance == state.instance && c.drawing.id == id)
        .map(|c| c.sheet)
}

/// Blattmodell der Zeichnung mit Mittellinien, den Maßen `dims` und den Nummernkreisen
/// `balloons` und der Stand seiner Linien; aus dem Zwischenspeicher, solange sich Zeichnung,
/// Ergebnisse, Maße und Kreise nicht geändert haben.
fn sheet_model(
    ctx: &egui::Context,
    state: &AppState,
    drawing: &Drawing,
    model: u64,
    dims: &Arc<ResolvedDims>,
    balloons: &Arc<Vec<BalloonMarks>>,
) -> (Arc<SheetModel>, u64) {
    let owned = state.drawing_ui.results_for(drawing, model);
    let section_data = state.drawing_ui.sections_for(drawing, model);
    let ptrs = result_ptrs(&owned, &section_data);
    let cached = ctx.data(|d| d.get_temp::<SheetCache>(sheet_cache_id()));
    let project = state.document.project_info().cloned();
    let context = sheet_context(state, drawing.id);
    let cells = if parts_space_for(drawing, 0, true) == PartsSpace::None {
        Arc::new(Vec::new())
    } else {
        cached_cells(ctx, state)
    };
    let texts = sheet_texts_with(state, drawing, &cells);
    let same_base = cached
        .as_ref()
        .filter(|c| c.instance == state.instance && c.model == model && c.ptrs == ptrs);
    if let Some(c) = same_base
        && Arc::ptr_eq(&c.dims, dims)
        && c.drawing == *drawing
        && c.project == project
        && c.context == context
        && c.parts == texts.parts
        && (Arc::ptr_eq(&c.balloons, balloons) || c.balloons == *balloons)
    {
        return (Arc::clone(&c.sheet), c.lines_gen);
    }
    let lines_gen = match same_base {
        Some(c) if same_lines(&c.drawing, drawing) => c.lines_gen,
        _ => LINES_GEN.fetch_add(1, Ordering::Relaxed),
    };
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, v)| (*k, v.as_ref())).collect();
    let mut built = build_sheet_with(drawing, &refs, dims);
    add_balloon_marks(&mut built, balloons);
    let data_refs: BTreeMap<ViewId, &SectionData> =
        section_data.iter().map(|(k, v)| (*k, v.as_ref())).collect();
    let (marks, section_errors) = section_marks(drawing, &refs, &data_refs);
    add_section_marks(&mut built, &marks);
    built.frame = Some(sheet_frame(
        &layout_from_texts(
            drawing,
            &texts,
            parts_space_for(drawing, cells.len(), false),
        ),
        &texts,
        drawing.projection,
    ));
    let parts = texts.parts;
    let sheet = Arc::new(built);
    ctx.data_mut(|d| {
        d.insert_temp(
            sheet_cache_id(),
            SheetCache {
                instance: state.instance,
                drawing: drawing.clone(),
                model,
                ptrs,
                dims: Arc::clone(dims),
                sheet: Arc::clone(&sheet),
                project,
                context,
                parts,
                balloons: Arc::clone(balloons),
                lines_gen,
                section_errors: Arc::new(section_errors),
            },
        )
    });
    (sheet, lines_gen)
}

/// Meldungen zu den Schnittansichten des zuletzt gezeichneten Blatts der Zeichnung `id` (etwa
/// eine zu feine Schraffur); leer ohne gezeichnetes Blatt dieser Zeichnung.
pub fn section_errors(ctx: &egui::Context, state: &AppState, id: DrawingId) -> Vec<String> {
    ctx.data(|d| d.get_temp::<SheetCache>(sheet_cache_id()))
        .filter(|c| c.instance == state.instance && c.drawing.id == id)
        .map(|c| c.section_errors.to_vec())
        .unwrap_or_default()
}

/// Zwischengespeicherte Nummernkreise der offenen Zeichnung: gültig, solange Dokument
/// (Änderungszähler), Modellstand und Ansichtsergebnisse gleich sind.
#[derive(Clone)]
struct BalloonsCache {
    instance: u64,
    edit: u64,
    model: u64,
    drawing: DrawingId,
    ptrs: Vec<usize>,
    list: Arc<Vec<BalloonMarks>>,
}

/// Die Nummernkreise der Zeichnung auf dem Blatt (mit Nummern), je Stand einmal gerechnet.
fn balloons_cached(
    ctx: &egui::Context,
    state: &AppState,
    drawing: &Drawing,
    model: u64,
) -> Arc<Vec<BalloonMarks>> {
    let id = egui::Id::new("planora-drawing-balloons");
    let owned = state.drawing_ui.results_for(drawing, model);
    let ptrs = result_ptrs(&owned, &BTreeMap::new());
    let edit = state.document.edit_count();
    if let Some(c) = ctx.data(|d| d.get_temp::<BalloonsCache>(id)).filter(|c| {
        c.instance == state.instance
            && c.edit == edit
            && c.model == model
            && c.drawing == drawing.id
            && c.ptrs == ptrs
    }) {
        return c.list;
    }
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, v)| (*k, v.as_ref())).collect();
    let numbers = balloon_tool::numbers_cached(ctx, state, drawing.id);
    let list = Arc::new(balloon_tool::balloon_list(drawing, &refs, &numbers));
    ctx.data_mut(|d| {
        d.insert_temp(
            id,
            BalloonsCache {
                instance: state.instance,
                edit,
                model,
                drawing: drawing.id,
                ptrs,
                list: Arc::clone(&list),
            },
        )
    });
    list
}

/// Gezogener Teil eines Maßes mit den Werten zu Beginn des Ziehens (Blatt-mm).
#[derive(Clone, Copy, Debug)]
enum DimPart {
    /// Maßlinie eines linearen Maßes: `offset` wächst um die Bewegung entlang der Querachse `w`.
    Offset { w: DVec2, offset: f64 },
    /// Text: `text_shift` wächst um die Bewegung.
    Text { shift: (f64, f64) },
    /// Ø/R-Führungslinie: der Knick folgt der Bewegung; Kreis mit Mitte und Radius auf dem Blatt,
    /// bei einem Bogen dessen Lage (der Winkel bleibt auf dem Bogen).
    Leader {
        center: DVec2,
        r: f64,
        knee: DVec2,
        arc: Option<ArcSpan>,
    },
    /// Ø-Maßlinie durch die Mitte: dreht um die Mitte mit dem Zeiger (Griff bei `from`).
    Rotate {
        center: DVec2,
        from: DVec2,
        angle: f64,
    },
    /// Bogen eines Winkelmaßes: der Radius wächst um die Bewegung entlang `dir` (vom Scheitel weg).
    Radial { dir: DVec2, radius: f64, sign: f64 },
}

/// Laufendes Ziehen im Blatt.
#[derive(Clone, Copy, Debug)]
enum Drag {
    Pan {
        origin: Pos2,
        start: egui::Vec2,
    },
    View {
        origin: Pos2,
        view: ViewId,
        start: Placement,
    },
    Dim {
        origin: Pos2,
        id: DimId,
        part: DimPart,
    },
    /// Nummernkreis: nur der Versatz ändert sich (der Anker `at` bleibt am Teil); ins Dokument
    /// erst beim Loslassen (eine Änderung je Ziehen).
    Balloon {
        origin: Pos2,
        id: BalloonId,
        at: DVec2,
        start: (f64, f64),
    },
}

/// Aktion aus der Werkzeugleiste.
enum Action {
    ToModel,
    Add(ViewKind),
    Fit,
    /// Fenster „Blattangaben“ öffnen.
    SheetInfo,
    Zoom(f32),
    Scale(Scale),
    Hidden(bool),
    Centerlines(bool),
    Suggest,
    Delete,
    /// Werkzeug „Maß“ ein- oder ausschalten.
    ToolToggle,
    /// Untermodus des Werkzeugs wählen (schaltet es ein).
    Tool(ToolMode),
    /// Werkzeug „Position“ ein- oder ausschalten.
    BalloonTool,
    /// Werkzeug „Schnitt“ ein- oder ausschalten.
    SectionTool,
    /// Eingabe im Ansichtsfeld einer Schnittansicht.
    Section(SectionEdit),
}

/// Änderung einer Schnittansicht aus ihrem Ansichtsfeld.
enum SectionEdit {
    /// Schnittposition (Text aus dem Feld).
    Offset(String),
    /// Blickrichtung umdrehen.
    Flip,
    /// Schraffur ein oder aus.
    Hatch(bool),
    /// Schraffurabstand (Text aus dem Feld).
    Spacing(String),
}

/// Kann das Werkzeug „Maß“ benutzt werden? Sonst die Erklärung (alte Version, keine Ansicht,
/// nur Isometrie).
pub fn tool_blocked(drawing: &Drawing, editable: bool) -> Option<String> {
    if !editable {
        Some(crate::dimension_tool::read_only_tip())
    } else if drawing.views.is_empty() {
        Some(crate::dimension_tool::no_views_tip())
    } else if !drawing.views.iter().any(|v| v.kind.is_normal()) {
        Some(crate::dimension_tool::isometric_tip())
    } else {
        None
    }
}

/// Schalter „Maß“ (Klick oder Taste): ein oder aus; eingeschaltet sind „Position“ und
/// „Schnitt“ aus (die drei schließen sich aus).
pub(crate) fn toggle_dimension_tool(state: &mut AppState) {
    state.dimension_tool.toggle();
    if state.dimension_tool.is_active() {
        state.balloon_tool.set_active(false);
        state.section_tool.set_active(false);
    }
}

/// Schalter „Position“ (Klick oder Taste): ein oder aus; eingeschaltet sind „Maß“ und
/// „Schnitt“ aus.
pub(crate) fn toggle_balloon_tool(state: &mut AppState) {
    let on = !state.balloon_tool.is_active();
    state.balloon_tool.set_active(on);
    if on {
        state.dimension_tool.set_mode(ToolMode::Off);
        state.section_tool.set_active(false);
    }
}

/// Werkzeug der Zeichnung über seine Taste ([`crate::keymap::Action::DrawingDimension`],
/// [`crate::keymap::Action::DrawingBalloon`]): wie der Klick auf den Schalter, also nichts,
/// solange der Schalter gesperrt ist (alte Version, keine passende Ansicht, Stücklistenblatt;
/// die Erklärung steht an ihm).
pub(crate) fn tool_key(state: &mut AppState, balloon: bool, editable: bool) {
    let WorkArea::Drawing(id) = state.drawing_ui.area else {
        return;
    };
    let Some(drawing) = state.document.drawing(id) else {
        return;
    };
    if balloon {
        // Die Prüfung auf eine offene Skizze bleibt, obwohl die Taste dann gar nicht im Bereich
        // Zeichnung ankommt: `show_with` schaltet „Position“ bei offener Skizze ebenso aus, und
        // ein Aufruf über `run_action` (ohne Verteiler) soll dieselbe Sperre haben.
        if balloon_tool::blocked(drawing, editable).is_none() && state.sketch_session.is_none() {
            toggle_balloon_tool(state);
        }
    } else if tool_blocked(drawing, editable).is_none() {
        toggle_dimension_tool(state);
    }
}

/// Hinweistext eines Werkzeugschalters: Name mit Taste („Position (B)“), darunter die Erklärung.
fn tool_tip(ui: &mut egui::Ui, action: crate::keymap::Action, name: &str, tip: &str) {
    let km = crate::keymap::mouse::current(ui.ctx());
    ui.strong(crate::keymap::with_hint(&km, action, name));
    ui.label(tip);
}

/// Zeichnungsbereich (bearbeitbar).
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    show_with(ui, state, true);
}

/// Rechtecke der Zeichnungsfelder: aus dem Layout der Shell in diesem Bild (die App zeichnet die
/// Felder vorher), sonst – ohne Shell, z. B. in Tests – aus [`shell::layout_in_mode`] über das
/// ganze Fenster (schreibgeschützt mit dem Platz für den Hinweis unter dem oberen Feld).
fn drawing_rects(ctx: &egui::Context, editable: bool) -> ShellRects {
    if let Some(r) = shell_panels::last_layout(ctx).filter(|r| r.tool.is_some()) {
        return r;
    }
    let content = ctx.content_rect();
    shell::layout_in_mode(
        content,
        ShellMode::Drawing,
        &ShellState::default(),
        (content.width(), 0.0),
        f32::INFINITY,
        0.0,
        !editable,
    )
}

/// Zeichnungsbereich; `editable = false` (alte Version) sperrt Hinzufügen, Löschen, Maßstab
/// und Verschieben der Ansichten. Ruft in jedem Bild `tick` und fordert ein Neuzeichnen an,
/// solange eine Ansicht berechnet wird.
///
/// Das Blatt wird über die ganze Fläche von `ui` (in der App: das Fenster) gemalt und nimmt dort
/// Eingaben an; eingepasst und zentriert wird es in der freien Fläche zwischen den Feldern. Die
/// Felder der Zeichnung – Werkzeugfeld, Ansichtsfeld und Hinweisfeld – zeichnet diese Funktion
/// selbst als schwebende Felder an den Rechtecken der Shell (ohne Shell: eigene Rechnung).
pub fn show_with(ui: &mut egui::Ui, state: &mut AppState, editable: bool) {
    state.drawing_ui.tick(&state.bodies, &state.document, false);
    let WorkArea::Drawing(id) = state.drawing_ui.area else {
        return;
    };
    let Some(drawing) = state.document.drawing(id).cloned() else {
        return;
    };
    // Gesperrtes Werkzeug ist aus (alte Version, Zeichnung nur mit Isometrie, Skizzen-Modus).
    if tool_blocked(&drawing, editable).is_some() && state.dimension_tool.is_active() {
        state.dimension_tool.set_mode(ToolMode::Off);
    }
    if (balloon_tool::blocked(&drawing, editable).is_some() || state.sketch_session.is_some())
        && state.balloon_tool.is_active()
    {
        state.balloon_tool.set_active(false);
    }
    let selected_spec = state
        .drawing_ui
        .selected_view
        .and_then(|v| drawing.view(v))
        .cloned();
    if (section_tool::blocked(&drawing, selected_spec.as_ref(), editable).is_some()
        || state.sketch_session.is_some())
        && state.section_tool.is_active()
    {
        state.section_tool.set_active(false);
    }
    let ctx = ui.ctx().clone();
    let paint = ui.max_rect();
    let rects = drawing_rects(&ctx, editable);
    let canvas = rects.free;
    let tool_rect = rects.tool.unwrap_or(canvas);
    let tool_action = tool_field(&ctx, tool_rect, state, &drawing, editable);
    let view_action = view_field(&ctx, rects.right, state, &drawing, editable);
    match tool_action.or(view_action) {
        Some(Action::ToModel) => {
            state.drawing_ui.area = WorkArea::Model;
            ctx.request_repaint();
            return;
        }
        Some(Action::Add(kind)) => {
            if let Err(e) = add_view(state, kind) {
                state.message = Some(e);
            }
        }
        Some(Action::Fit) => {
            state.drawing_ui.zoom = 1.0;
            state.drawing_ui.pan = egui::Vec2::ZERO;
        }
        Some(Action::SheetInfo) => crate::sheet_dialog::open(state, id),
        Some(Action::Zoom(f)) => {
            let z = (state.drawing_ui.zoom * f).clamp(MIN_ZOOM, MAX_ZOOM);
            let f = z / state.drawing_ui.zoom;
            state.drawing_ui.zoom = z;
            state.drawing_ui.pan *= f;
        }
        Some(Action::Scale(s)) => {
            if let Some(sel) = state.drawing_ui.selected_view
                && let Err(e) = section_tool::set_scale(state, id, sel, s)
            {
                state.message = Some(e.text());
            }
        }
        Some(Action::Hidden(h)) => set_selected(state, id, |v| v.show_hidden = h),
        Some(Action::Centerlines(c)) => set_selected(state, id, |v| {
            if v.kind.is_normal() {
                v.centerlines = c;
            }
        }),
        Some(Action::Suggest) => suggest_dimensions(state, id),
        Some(Action::Delete) => delete_selected(state),
        Some(Action::ToolToggle) => toggle_dimension_tool(state),
        Some(Action::Tool(m)) => {
            state.dimension_tool.set_mode(m);
            state.balloon_tool.set_active(false);
            state.section_tool.set_active(false);
        }
        Some(Action::BalloonTool) => toggle_balloon_tool(state),
        Some(Action::SectionTool) => {
            let on = !state.section_tool.is_active();
            state.section_tool.set_active(on);
            if on {
                state.dimension_tool.set_mode(ToolMode::Off);
                state.balloon_tool.set_active(false);
            }
        }
        Some(Action::Section(edit)) => {
            if let Some(sel) = state.drawing_ui.selected_view {
                let r = match edit {
                    SectionEdit::Offset(t) => section_tool::set_offset_text(state, id, sel, &t),
                    SectionEdit::Flip => section_tool::flip(state, id, sel),
                    SectionEdit::Hatch(on) => section_tool::set_hatch(state, id, sel, on),
                    SectionEdit::Spacing(t) => section_tool::set_spacing_text(state, id, sel, &t),
                };
                state.section_tool.set_message(r.err());
            }
        }
        None => {}
    }
    let response = ui.allocate_rect(paint, Sense::click_and_drag());
    let views = canvas_ui(ui, state, id, (paint, canvas), &response, editable);
    if views.is_none() {
        // Keine Blattfläche (Fenster zu klein): wenigstens die Umgebung des Blatts malen.
        ui.painter()
            .rect_filled(paint, 0.0, theme::drawing_surround(state.dark));
    }

    let note = state.drawing_ui.note(&state.bodies);
    let dims = resolve_in(state, id);
    let mut notes = notes_in(state, id, &dims);
    // Verlorene Positionsnummern zuerst (rot), dann die Hinweise zu den Maßen.
    let lost_balloons = balloon_tool::lost_count(&balloon_tool::numbers_cached(&ctx, state, id));
    let mut red = usize::from(notes.lost > 0);
    // Meldungen zu den Schnittansichten (zu feine Schraffur) vor allem anderen, rot.
    let section_notes = section_errors(&ctx, state, id);
    red += section_notes.len();
    for (i, t) in section_notes.into_iter().enumerate() {
        notes.texts.insert(i, Msg::Raw(t));
    }
    if lost_balloons > 0 {
        notes
            .texts
            .insert(0, balloon_tool::lost_note(lost_balloons));
        red += 1;
    }
    // Die Meldung des Vorschlags gilt nur, bis sich das Dokument oder die Auswahl ändert.
    if state.drawing_ui.dim_message_key != Some(message_key(state)) {
        state.drawing_ui.dim_message = None;
        state.drawing_ui.dim_message_key = None;
    }
    // Eine Ablehnung des Werkzeugs „Maß“ geht der Meldung vor (rot); bei eingeschaltetem
    // Werkzeug steht an Stelle des Hinweises der nächste Schritt.
    state
        .section_tool
        .expire((state.document.edit_count(), state.drawing_ui.selected_view));
    let rejected = state
        .dimension_tool
        .message()
        .or(state.balloon_tool.message())
        .or(state.section_tool.message());
    let message = rejected
        .clone()
        .or_else(|| state.dimension_tool.notice())
        .or_else(|| state.drawing_ui.dim_message.as_ref().map(Msg::text));
    let hint_text = state
        .dimension_tool
        .hint()
        .or_else(|| state.balloon_tool.is_active().then(balloon_tool::hint))
        .or_else(|| state.section_tool.hint())
        .or(note)
        .unwrap_or_else(default_hint);
    let note_texts: Vec<String> = notes.texts.iter().map(Msg::text).collect();
    let lines = HintLines {
        notes: &note_texts,
        red,
        message: message.map(|m| (m, rejected.is_some())),
        hint: hint_text,
    };
    hint_field(&ctx, rects.bottom, state.dark, lines);
    if let Some((layout, pending)) = views {
        if pending {
            ctx.request_repaint_after(REPAINT);
        }
        let layout = DrawingLayout {
            area: paint,
            toolbar: tool_rect,
            hint: rects.bottom,
            ..layout
        };
        ctx.data_mut(|d| d.insert_temp(layout_id(), layout));
    }
}

/// Leistenknopf ohne Rand (Fläche nur beim Überfahren). Der Stil gilt nur für diesen Knopf.
fn flat(button: egui::Button<'_>) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| {
        let saved = ui.style().clone();
        theme::frameless(ui.style_mut());
        let response = ui.add(button);
        ui.set_style(saved);
        theme::focus_ring(ui, &response);
        response
    }
}

/// Symbolknopf ohne Text (Vergrößern, Verkleinern); der Hinweis steht im Tooltip.
fn icon_only<'a>(icon: Icon) -> egui::Button<'a> {
    egui::Button::new(icons::atom(icon))
}

/// Rahmen des Werkzeugfelds: wie [`theme::panel_frame`], oben und unten so viel Rand, dass genau
/// zwei Zeilen ([`theme::ROW_H`] mit Zeilenabstand) in [`shell::TOOL_H`] passen.
fn tool_frame(dark: bool) -> egui::Frame {
    let frame = theme::panel_frame(dark);
    let v = ((shell::TOOL_H - 2.0 * frame.stroke.width - 2.0 * theme::ROW_H - theme::SPACE_1)
        / 2.0)
        .max(0.0);
    frame.inner_margin(egui::Margin::symmetric(theme::SPACE_3 as i8, v as i8))
}

/// Eine Zeile des Werkzeugfelds: genau [`theme::ROW_H`] hoch, links beginnend, mittig.
fn tool_row<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    // `horizontal` wächst in der Rollfläche nach rechts (ein fest breiter Bereich schnitte den
    // Rest ab, statt ihn rollbar zu machen); seine Höhe ist die Zeilenhöhe `interact_size.y`.
    ui.horizontal(|ui| {
        ui.set_min_height(theme::ROW_H);
        add_contents(ui)
    })
    .inner
}

/// Werkzeugfeld unter dem oberen Feld: zwei feste Zeilen (Modell, Zeichnung und Ansichten; Blatt,
/// Zoom und Maße). Ist das
/// Feld zu schmal (kleine Fenster), rollt sein Inhalt waagerecht. Liefert die gewählte Aktion.
fn tool_field(
    ctx: &egui::Context,
    rect: Rect,
    state: &AppState,
    drawing: &Drawing,
    editable: bool,
) -> Option<Action> {
    let dark = state.dark;
    let p = theme::palette(dark);
    let frame = tool_frame(dark);
    let inner = shell_panels::inner_size(&frame, rect.size());
    let margin_x = frame.total_margin().sum().x;
    let mut action = None;
    let selected = state
        .drawing_ui
        .selected_view
        .is_some_and(|v| drawing.view(v).is_some());
    shell_panels::field(
        ctx,
        shell_panels::drawing_tool_id(),
        rect.min,
        frame,
        |ui| {
            ui.set_width(inner.x);
            ui.set_height(inner.y);
            // Die Rollfläche reicht bis in den unteren Rand des Felds: ihr schmaler, schwebender
            // Rollbalken liegt dort, unter der zweiten Zeile (das Feld wird nicht höher). Passt
            // nicht alles hinein (kleine Fenster), ist er auch in Ruhe zu sehen.
            // Rand unten abzüglich Strich und 2 px, damit der Balken im Feld bleibt.
            let below = (f32::from(frame.inner_margin.bottom) - frame.stroke.width - 2.0).max(0.0);
            let area_rect = Rect::from_min_size(ui.cursor().min, inner + vec2(0.0, below));
            let mut child = ui.new_child(
                UiBuilder::new()
                    .max_rect(area_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.spacing_mut().scroll.dormant_handle_opacity =
                child.spacing().scroll.active_handle_opacity;
            let area = egui::ScrollArea::horizontal()
                .id_salt("planora-field-drawing-tool-scroll")
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded);
            let scroll = theme::scroll_area(&mut child, area, |ui| {
                ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                // Eng gesetzt (wie das Befehlsfeld), damit beide Zeilen schon bei 1100 px
                // Fensterbreite ganz sichtbar sind.
                ui.spacing_mut().item_spacing.x = theme::SPACE_1;
                ui.spacing_mut().button_padding.x = theme::SPACE_1;
                // Erste Zeile: zurück zum Modell, Name der Zeichnung, Ansicht hinzufügen.
                tool_row(ui, |ui| {
                    if ui
                        .add(flat(icons::selectable(
                            false,
                            Icon::ToModel,
                            tr!("drawing-to-model"),
                        )))
                        .clicked()
                    {
                        action = Some(Action::ToModel);
                    }
                    let _ = ui.add(flat(icons::selectable(
                        true,
                        Icon::OpenDrawing,
                        tr!("drawing-current", name = shorten_for_display(&drawing.name)),
                    )));
                    ui.separator();
                    // Auf einem Stücklistenblatt gibt es keine Ansichten.
                    let parts_sheet = matches!(drawing.kind, DrawingKind::PartsList { .. });
                    let add_why = if parts_sheet {
                        no_views_on_parts_sheet().text()
                    } else {
                        crate::drawing_browser::read_only_hint()
                    };
                    ui.add_enabled(editable && !parts_sheet, |ui: &mut egui::Ui| {
                        let saved = ui.style().clone();
                        theme::frameless(ui.style_mut());
                        let r = ui
                            .menu_button(
                                (icons::atom(Icon::AddView), tr!("drawing-add-view")),
                                |ui| {
                                    for k in ViewKind::ALL {
                                        if theme::entry(
                                            ui,
                                            true,
                                            icons::button(Icon::ViewFace, view_kind_label(k)),
                                        )
                                        .clicked()
                                        {
                                            action = Some(Action::Add(k));
                                            ui.close();
                                        }
                                    }
                                },
                            )
                            .response;
                        ui.set_style(saved);
                        theme::focus_ring(ui, &r);
                        r
                    })
                    .on_disabled_hover_text(add_why);
                    // Werkzeug „Schnitt“ direkt nach „Ansicht hinzufügen“ (eingeschaltet mit
                    // Akzent; gesperrt mit Erklärung).
                    let selected_spec =
                        state.drawing_ui.selected_view.and_then(|v| drawing.view(v));
                    let section_why = section_tool::blocked(drawing, selected_spec, editable);
                    let button = if state.section_tool.is_active() {
                        egui::Button::new((
                            icons::text(Icon::SectionTool, icons::LABEL_ICON_SIZE, p.on_accent),
                            egui::RichText::new(section_tool::tool()).color(p.on_accent),
                        ))
                        .fill(p.accent)
                        .selected(true)
                    } else {
                        icons::button(Icon::SectionTool, section_tool::tool())
                    };
                    if ui
                        .add_enabled(section_why.is_none(), flat(button))
                        .on_hover_text(section_tool::tool_tip())
                        .on_disabled_hover_text(section_why.unwrap_or_else(section_tool::tool_tip))
                        .clicked()
                    {
                        action = Some(Action::SectionTool);
                    }
                    // Am Ende der ersten Zeile, damit die zweite nicht breiter wird. Nur das
                    // Symbol (wie „Blatt einpassen“), damit die Zeile mit dem Werkzeug „Schnitt“
                    // bei 1100 × 700 ins Feld passt; der Name steht im Tooltip.
                    let info = ui.add_enabled(editable, flat(icon_only(Icon::SheetInfo)));
                    info.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            editable,
                            crate::sheet_dialog::button(),
                        )
                    });
                    if info
                        .on_hover_text(format!(
                            "{}: {}",
                            crate::sheet_dialog::button(),
                            crate::sheet_dialog::button_tip()
                        ))
                        .on_disabled_hover_text(format!(
                            "{}: {}",
                            crate::sheet_dialog::button(),
                            crate::drawing_browser::read_only_hint()
                        ))
                        .clicked()
                    {
                        action = Some(Action::SheetInfo);
                    }
                });
                // Zweite Zeile: Blatt einpassen und Zoom, dann die Maße (die Einstellungen der
                // gewählten Ansicht stehen im Ansichtsfeld rechts; das Feld bleibt so immer gleich
                // hoch).
                tool_row(ui, |ui| {
                    // Symbolknopf (wie die Lupen), damit die Zeile mit dem Werkzeug „Position“
                    // bei 1100 × 700 ins Feld passt; der Name steht im Tooltip.
                    let fit = ui.add(flat(icon_only(Icon::FitSheet)));
                    fit.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, fit_sheet())
                    });
                    if fit.on_hover_text(fit_sheet()).clicked() {
                        action = Some(Action::Fit);
                    }
                    if ui
                        .add(flat(icon_only(Icon::ZoomOut)))
                        .on_hover_text(tr!("drawing-zoom-out"))
                        .clicked()
                    {
                        action = Some(Action::Zoom(1.0 / ZOOM_STEP));
                    }
                    if ui
                        .add(flat(icon_only(Icon::ZoomIn)))
                        .on_hover_text(tr!("drawing-zoom-in"))
                        .clicked()
                    {
                        action = Some(Action::Zoom(ZOOM_STEP));
                    }
                    ui.separator();
                    let tip = if selected {
                        tr!("drawing-suggest-tip-view")
                    } else {
                        tr!("drawing-suggest-tip-all")
                    };
                    if ui
                        .add_enabled(
                            editable,
                            flat(icons::button(Icon::SuggestDimensions, suggest())),
                        )
                        .on_hover_text(tip)
                        .clicked()
                    {
                        action = Some(Action::Suggest);
                    }
                    // Werkzeug „Maß“: Schalter und Untermodus (immer da, gesperrt mit Erklärung).
                    // Eingeschaltet trägt der Schalter den Akzent (Modus an).
                    let blocked = tool_blocked(drawing, editable);
                    let free = blocked.is_none();
                    let tool = &state.dimension_tool;
                    let why = blocked.unwrap_or_else(crate::dimension_tool::tool_tip);
                    let button = if tool.is_active() {
                        egui::Button::new((
                            icons::text(Icon::DimensionTool, icons::LABEL_ICON_SIZE, p.on_accent),
                            egui::RichText::new(crate::dimension_tool::tool()).color(p.on_accent),
                        ))
                        .fill(p.accent)
                        .selected(true)
                    } else {
                        icons::button(Icon::DimensionTool, crate::dimension_tool::tool())
                    };
                    if ui
                        .add_enabled(free, flat(button))
                        .on_hover_ui(|ui| {
                            tool_tip(
                                ui,
                                crate::keymap::Action::DrawingDimension,
                                &crate::dimension_tool::tool(),
                                &crate::dimension_tool::tool_tip(),
                            )
                        })
                        .on_disabled_hover_text(why.clone())
                        .clicked()
                    {
                        action = Some(Action::ToolToggle);
                    }
                    ui.add_enabled(free, |ui: &mut egui::Ui| {
                        // Feste Breite für die längste Maßart: die Zeile ändert ihre Breite nicht
                        // mit der Wahl.
                        let width = mode_combo_width(ui);
                        egui::ComboBox::from_id_salt("planora-dimension-tool")
                            .width(width)
                            .selected_text(tool.choice().label())
                            .show_ui(ui, |ui| {
                                for m in ToolMode::CHOICES {
                                    if ui.selectable_label(tool.choice() == m, m.label()).clicked()
                                    {
                                        action = Some(Action::Tool(m));
                                    }
                                }
                            })
                            .response
                    })
                    .on_disabled_hover_text(why);
                    // Werkzeug „Position“ direkt nach dem Maß-Werkzeug (eingeschaltet mit Akzent).
                    let balloon_why = balloon_tool::blocked(drawing, editable);
                    let button = if state.balloon_tool.is_active() {
                        egui::Button::new((
                            icons::text(Icon::PositionTool, icons::LABEL_ICON_SIZE, p.on_accent),
                            egui::RichText::new(balloon_tool::tool()).color(p.on_accent),
                        ))
                        .fill(p.accent)
                        .selected(true)
                    } else {
                        icons::button(Icon::PositionTool, balloon_tool::tool())
                    };
                    if ui
                        .add_enabled(balloon_why.is_none(), flat(button))
                        .on_hover_ui(|ui| {
                            tool_tip(
                                ui,
                                crate::keymap::Action::DrawingBalloon,
                                &balloon_tool::tool(),
                                &balloon_tool::tool_tip(),
                            )
                        })
                        .on_disabled_hover_text(balloon_why.unwrap_or_else(balloon_tool::tool_tip))
                        .clicked()
                    {
                        action = Some(Action::BalloonTool);
                    }
                });
                ui.add_space(below);
            });
            // Wunschbreite: beide Zeilen ohne Rollen, mit Rand; die Shell passt das Feld an.
            let key = shell_panels::drawing_tool_width_key();
            let first = shell_panels::stored(ui.ctx(), key).is_none();
            // Auf gerade Breite aufgerundet: so liegt das mittige Feld auf ganzen Pixeln.
            let wanted = ((scroll.content_size.x + margin_x) / 2.0).ceil() * 2.0;
            shell_panels::remember_width(ui, key, wanted);
            if first {
                // Noch keine Breite gemerkt: das Feld stand in voller Breite, also wiederholen.
                shell_panels::discard(ui.ctx(), "Werkzeugfeld: erste Breite");
            }
            ui.advance_cursor_after_rect(Rect::from_min_size(area_rect.min, inner));
        },
    );
    action
}

/// Breite des Auswahlfelds der Maßarten: die längste Bezeichnung mit Pfeil und Rand (so bleibt
/// das Feld bei jeder Wahl gleich breit).
pub fn mode_combo_width(ui: &egui::Ui) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let widest = ui.fonts_mut(|f| {
        ToolMode::CHOICES
            .iter()
            .map(|m| {
                f.layout_no_wrap(m.label(), font.clone(), Color32::PLACEHOLDER)
                    .size()
                    .x
            })
            .fold(0.0, f32::max)
    });
    let s = ui.spacing();
    (widest + s.icon_spacing + s.icon_width + 2.0 * s.button_padding.x).ceil()
}

/// Ansichtsfeld rechts (Spaltenbreite 250): Name der gewählten Ansicht, Maßstab, Schalter für
/// verdeckte Kanten und Mittellinien, „Ansicht löschen“; ohne gewählte Ansicht ein Hinweis.
/// Liefert die gewählte Aktion.
fn view_field(
    ctx: &egui::Context,
    rect: Rect,
    state: &AppState,
    drawing: &Drawing,
    editable: bool,
) -> Option<Action> {
    let frame = theme::panel_frame(state.dark);
    let inner = shell_panels::inner_size(&frame, rect.size());
    let margin = f32::from(frame.inner_margin.left);
    let selected = state
        .drawing_ui
        .selected_view
        .and_then(|v| drawing.view(v))
        .cloned();
    let names = view_names(drawing);
    let mut action = None;
    shell_panels::field(
        ctx,
        shell_panels::drawing_view_id(),
        rect.min,
        frame,
        |ui| {
            shell_panels::side_column(ui, inner, margin, |ui| {
                let area = egui::ScrollArea::vertical()
                    .id_salt("planora-field-drawing-view-scroll")
                    .max_height(inner.y)
                    .auto_shrink([false, true]);
                theme::scroll_area(ui, area, |ui| {
                    let Some(v) = &selected else {
                        theme::section_header(ui, &tr!("drawing-view-header"), theme::HEADER_H);
                        ui.add_space(theme::SPACE_1);
                        ui.weak(no_view_selected());
                        return;
                    };
                    let name = names.get(&v.id).cloned().unwrap_or_default();
                    // Eine Schnittansicht heißt im Kopf wie auf dem Blatt („Schnitt A–A“).
                    let title = if v.section.is_some() {
                        name.clone()
                    } else {
                        tr!("drawing-view-title", name = name.as_str())
                    };
                    theme::section_header(ui, &title, theme::HEADER_H);
                    ui.add_space(theme::SPACE_1);
                    // Der Maßstab einer Schnittansicht folgt ihrer Ausgangsansicht.
                    let scale_free = editable && v.section.is_none();
                    ui.horizontal(|ui| {
                        ui.add_enabled(scale_free, egui::Label::new(tr!("drawing-scale")))
                            .on_disabled_hover_text(section_tool::scale_locked_tip());
                        ui.add_enabled(scale_free, |ui: &mut egui::Ui| {
                            egui::ComboBox::from_id_salt("planora-drawing-scale")
                                .selected_text(v.scale.label())
                                .show_ui(ui, |ui| {
                                    for s in Scale::SERIES {
                                        if ui.selectable_label(s == v.scale, s.label()).clicked() {
                                            action = Some(Action::Scale(s));
                                        }
                                    }
                                })
                                .response
                        })
                        .on_disabled_hover_text(section_tool::scale_locked_tip());
                    });
                    if let Some(sec) = &v.section {
                        let model = state.document.model_edit_count();
                        let nudged = state
                            .drawing_ui
                            .section_of(v, model)
                            .is_some_and(|d| d.nudged);
                        if let Some(edit) = section_fields(ui, v, sec, editable, nudged) {
                            action = Some(Action::Section(edit));
                        }
                    }
                    let mut hidden = v.show_hidden;
                    if ui
                        .add_enabled(
                            editable,
                            theme::Toggle::new(&mut hidden, tr!("drawing-show-hidden")),
                        )
                        .changed()
                    {
                        action = Some(Action::Hidden(hidden));
                    }
                    // Mittellinien gibt es nur in Normansichten (Blick entlang einer Weltachse); in
                    // der Isometrie bleibt der Schalter gesperrt.
                    let mut centerlines = v.centerlines;
                    if ui
                        .add_enabled(
                            editable && v.kind.is_normal(),
                            theme::Toggle::new(&mut centerlines, centerlines_label()),
                        )
                        .on_disabled_hover_text(no_iso_centerlines())
                        .changed()
                    {
                        action = Some(Action::Centerlines(centerlines));
                    }
                    ui.add_space(theme::SPACE_1);
                    if ui
                        .add_enabled(
                            editable,
                            flat(icons::button(Icon::Delete, tr!("drawing-view-delete"))),
                        )
                        .clicked()
                    {
                        action = Some(Action::Delete);
                    }
                });
            });
        },
    );
    action
}

/// Zahlenfeld, das den eingegebenen Text liefert, sobald es verlassen wird (Enter oder Klick
/// daneben) und der Text von der Anzeige `shown` abweicht. Esc verwirft die Eingabe. Solange das
/// Feld nicht den Fokus hat, zeigt es `shown` (nach einer abgelehnten Eingabe also den alten Wert).
fn commit_field(
    ui: &mut egui::Ui,
    key: &'static str,
    view: ViewId,
    shown: &str,
    enabled: bool,
) -> Option<String> {
    let id = egui::Id::new(("planora-section-field", key, view));
    let editing = ui.memory(|m| m.has_focus(id) || m.had_focus_last_frame(id));
    let stored: Option<String> = ui.data(|d| d.get_temp(id)).filter(|_| editing);
    let mut text = stored.clone().unwrap_or_else(|| shown.to_string());
    let r = theme::text_field_enabled(
        ui,
        enabled,
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(SECTION_FIELD_W),
    );
    if r.lost_focus() || !editing && !r.has_focus() {
        ui.data_mut(|d| d.remove_temp::<String>(id));
    } else if r.changed() || (r.has_focus() && stored.is_none()) {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    }
    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
    (r.lost_focus() && !esc && text.trim() != shown).then_some(text)
}

/// Breite der Zahlenfelder einer Schnittansicht.
const SECTION_FIELD_W: f32 = 96.0;

/// Felder einer Schnittansicht im Ansichtsfeld: Schnittposition, Blickrichtung umdrehen,
/// Schraffur an/aus und ihr Abstand, bei verschobener Ebene der Hinweis.
fn section_fields(
    ui: &mut egui::Ui,
    v: &planora_drawing::ViewSpec,
    sec: &planora_drawing::SectionSpec,
    editable: bool,
    nudged: bool,
) -> Option<SectionEdit> {
    let mut edit = None;
    ui.add_space(theme::SPACE_1);
    ui.add_enabled(editable, egui::Label::new(tr!("drawing-section-position")));
    if let Some(t) = commit_field(
        ui,
        "offset",
        v.id,
        &section_tool::field_text(sec.offset),
        editable,
    ) {
        edit = Some(SectionEdit::Offset(t));
    }
    ui.add_space(theme::SPACE_1);
    if ui
        .add_enabled(
            editable,
            flat(icons::button(Icon::Mirror, tr!("drawing-section-flip"))),
        )
        .clicked()
    {
        edit = Some(SectionEdit::Flip);
    }
    let mut hatch = sec.hatch.on;
    if ui
        .add_enabled(
            editable,
            theme::Toggle::new(&mut hatch, tr!("drawing-section-hatch")),
        )
        .changed()
    {
        edit = Some(SectionEdit::Hatch(hatch));
    }
    ui.add_enabled(
        editable && sec.hatch.on,
        egui::Label::new(tr!("drawing-section-hatch-spacing")),
    );
    if let Some(t) = commit_field(
        ui,
        "spacing",
        v.id,
        &section_tool::field_text(sec.hatch.spacing),
        editable && sec.hatch.on,
    ) {
        edit = Some(SectionEdit::Spacing(t));
    }
    if nudged {
        ui.add_space(theme::SPACE_1);
        ui.add(egui::Label::new(egui::RichText::new(section_tool::nudged()).weak()).wrap());
    }
    edit
}

/// Inhalt des Hinweisfelds.
struct HintLines<'a> {
    /// Hinweise zu den Maßen (verlorene, außerhalb des Blatts).
    notes: &'a [String],
    /// So viele der ersten Hinweise betreffen Verlorenes (rot): verlorene Positionsnummern und
    /// verlorene Maße.
    red: usize,
    /// Meldung und ob sie eine Ablehnung ist (rot).
    message: Option<(String, bool)>,
    /// Hinweis zur Bedienung (oder des Werkzeugs „Maß“).
    hint: String,
}

/// Rand des Hinweisfelds oben und unten: zwei Textzeilen passen in [`shell::BOTTOM_H`].
const HINT_MARGIN_V: f32 = theme::SPACE_2;

/// Hinweisfeld unten (statt des Befehlsfelds): höchstens zwei Zeilen. Mit Hinweisen zu den Maßen
/// oder einer Meldung stehen diese in der ersten Zeile (die verlorenen Maße zuerst und rot, die
/// Meldung höchstens die halbe Zeile) und der Bedienhinweis in der zweiten; sonst bricht der
/// Bedienhinweis in bis zu zwei Zeilen um. Zu lange Texte enden mit „…“ (ganz beim Überfahren).
fn hint_field(ctx: &egui::Context, rect: Rect, dark: bool, lines: HintLines<'_>) {
    let frame = theme::panel_frame(dark).inner_margin(egui::Margin::symmetric(
        theme::SPACE_3 as i8,
        HINT_MARGIN_V as i8,
    ));
    let inner = shell_panels::inner_size(&frame, rect.size());
    let error = error_color(dark);
    shell_panels::field(
        ctx,
        shell_panels::drawing_hint_id(),
        rect.min,
        frame,
        |ui| {
            ui.set_width(inner.x);
            ui.set_height(inner.y);
            let gap = theme::SPACE_1;
            let row_h = ((inner.y - gap) / 2.0).max(0.0);
            let weak = ui.visuals().weak_text_color();
            let row = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
                ui.allocate_ui_with_layout(
                    vec2(inner.x, row_h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| add(ui),
                );
            };
            ui.spacing_mut().item_spacing.y = gap;
            if lines.notes.is_empty() && lines.message.is_none() {
                // Nur der Bedienhinweis: in bis zu zwei Zeilen, mittig in der Höhe.
                let galley = shell_panels::wrapped(ui.ctx(), &lines.hint, weak, inner.x, 2);
                let size = vec2(inner.x, galley.size().y);
                ui.add_space(((inner.y - size.y) / 2.0).max(0.0));
                let (r, response) = ui.allocate_exact_size(size, Sense::hover());
                let enabled = ui.is_enabled();
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, &lines.hint)
                });
                let elided = galley.elided;
                ui.painter().galley(r.min, galley, weak);
                if elided {
                    response.on_hover_text(lines.hint);
                }
                return;
            }
            row(ui, &mut |ui| {
                // Die Maßhinweise teilen sich die Zeile (mit Meldung höchstens die halbe); zu lange
                // enden mit „…“ und zeigen sich beim Überfahren ganz.
                let share = if lines.message.is_some() { 0.5 } else { 1.0 };
                let gap = ui.spacing().item_spacing.x;
                let mut budget = share * ui.available_width();
                for (i, t) in lines.notes.iter().enumerate() {
                    let w = budget / (lines.notes.len() - i) as f32;
                    let text = if i < lines.red {
                        egui::RichText::new(t.as_str()).color(error)
                    } else {
                        egui::RichText::new(t.as_str())
                    };
                    let used = ui
                        .scope(|ui| {
                            ui.set_max_width(w.max(0.0));
                            ui.add(egui::Label::new(text).truncate()).rect.width()
                        })
                        .inner;
                    budget -= used + gap;
                }
                if let Some((m, rejected)) = &lines.message {
                    if !lines.notes.is_empty() {
                        ui.separator();
                    }
                    // Die Meldung nimmt höchstens die halbe Zeile, wenn auch Maßhinweise dastehen.
                    let w = if lines.notes.is_empty() {
                        ui.available_width()
                    } else {
                        (0.5 * ui.available_width()).max(40.0)
                    };
                    ui.scope(|ui| {
                        ui.set_max_width(w);
                        let text = if *rejected {
                            egui::RichText::new(m.as_str()).color(error)
                        } else {
                            egui::RichText::new(m.as_str())
                        };
                        ui.add(egui::Label::new(text).truncate());
                    });
                }
            });
            row(ui, &mut |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(lines.hint.as_str()).weak()).truncate(),
                );
            });
        },
    );
}

fn set_selected(
    state: &mut AppState,
    id: DrawingId,
    f: impl FnOnce(&mut planora_drawing::ViewSpec),
) {
    let Some(sel) = state.drawing_ui.selected_view else {
        return;
    };
    let r = state.document.update_drawing(id, |d| {
        if let Some(v) = d.views.iter_mut().find(|v| v.id == sel) {
            f(v);
        }
    });
    if let Err(e) = r {
        state.message = Some(e.to_string());
    }
}

/// „Ansicht löschen“ der Werkzeugleiste: derselbe Weg wie „Löschen“ im Browser
/// ([`crate::drawing_browser::request_delete_view`]). Werden abgeleitete Ansichten oder Maße
/// mitgelöscht, kommt erst die Rückfrage, die sie nennt; sonst wird sofort gelöscht. Die Auswahl
/// verfällt, sobald die Ansicht wirklich gelöscht ist.
pub fn delete_selected(state: &mut AppState) {
    let (WorkArea::Drawing(id), Some(sel)) =
        (state.drawing_ui.area, state.drawing_ui.selected_view)
    else {
        return;
    };
    crate::drawing_browser::request_delete_view(state, id, sel);
}

/// „Maße vorschlagen“: Grundmaße der gewählten Ansicht (sonst aller Ansichten) über
/// [`suggest_for`]; die Meldung steht danach in der Hinweiszeile. Ansichten, die sich nicht
/// berechnen ließen, werden in der Meldung genannt.
pub fn suggest_dimensions(state: &mut AppState, id: DrawingId) {
    let only = state.drawing_ui.selected_view;
    let model = state.document.model_edit_count();
    let failed: Vec<(Msg, Msg)> = state
        .document
        .drawing(id)
        .map(|d| {
            let names = view_name_msgs(d);
            d.views
                .iter()
                .filter(|v| v.kind.is_normal() && only.is_none_or(|o| o == v.id))
                .filter_map(|v| match state.drawing_ui.result_of(v, model) {
                    ViewResult::Failed(m) => Some((
                        names.get(&v.id).cloned().unwrap_or(Msg::Raw(String::new())),
                        m,
                    )),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let outcome = suggest_for(state, id, only);
    let mut text = outcome.message();
    if !failed.is_empty() {
        let quoted = MsgArg::List(
            failed
                .iter()
                .map(|(n, _)| crate::msg!("drawing-quoted", name = n.clone()).into())
                .collect(),
        );
        let count = failed.len();
        match &outcome {
            // Keine Ansicht ließ sich berechnen: der Grund ist der Fehlertext einer Ansicht.
            SuggestOutcome::Unavailable(t) if failed.iter().any(|(_, m)| m == t) => {
                text = crate::msg!(
                    "drawing-views-failed",
                    count = count,
                    names = quoted,
                    reason = t.clone()
                );
            }
            // Anderer Grund (etwa: es wird noch gerechnet).
            SuggestOutcome::Unavailable(_) => {}
            _ => {
                text = Msg::Join(vec![
                    text,
                    crate::msg!(
                        "drawing-views-failed-no-dims",
                        count = count,
                        names = quoted
                    ),
                ]);
            }
        }
    }
    state.drawing_ui.dim_message = Some(text);
    state.drawing_ui.dim_message_key = Some(message_key(state));
}

/// Klick mit eingeschaltetem Werkzeug „Maß“ an der Blattstelle `at`: sind genug Bezüge gewählt,
/// setzt er das Maß (bei Kette und Bezugsmaßen nur ein Klick ins Leere oder ein Doppelklick),
/// sonst wählt er die getroffene Linie `found` als Bezug.
fn tool_click(
    state: &mut AppState,
    id: DrawingId,
    found: Option<(ViewId, StrokeOrigin)>,
    at: DVec2,
    k: f64,
    double: bool,
) {
    let tool = &state.dimension_tool;
    // Ein Doppelklick beendet nur Kette und Bezugsmaße; sonst zählt sein zweiter Klick nicht
    // (das Maß wird nicht vorzeitig gesetzt).
    if double && !tool.collects_more() {
        return;
    }
    if tool.is_ready() && (!tool.collects_more() || found.is_none() || double) {
        // Eine Ablehnung steht danach im Werkzeug (Hinweiszeile).
        tool_finish(state, id, at).ok();
    } else if double {
        // Zweiter Klick eines Doppelklicks, der schon gesetzt hat: nichts Neues beginnen.
    } else if let Some((view, origin)) = found {
        let hit = Hit {
            origin,
            at,
            px_per_mm: k,
        };
        with_tool(state, |t, st| t.click(st, id, view, &hit));
    } else {
        state
            .dimension_tool
            .set_message(Some(crate::dimension_tool::missed()));
    }
}

/// Esc mit eingeschaltetem Werkzeug „Maß“: Kette und Bezugsmaße mit genug Punkten werden an der
/// letzten Zeigerlage gesetzt; sonst verfallen die gewählten Bezüge; ohne Bezüge geht das
/// Werkzeug aus.
fn tool_escape(state: &mut AppState, id: DrawingId) {
    let tool = &state.dimension_tool;
    if tool.collects_more()
        && tool.is_ready()
        && let Some(p) = tool.last_pointer()
    {
        // Esc bleibt nie hängen: lässt sich nichts setzen, verfallen die Bezüge, und der Grund
        // bleibt in der Hinweiszeile.
        if tool_finish(state, id, p).is_err() {
            let reason = state.dimension_tool.message_msg();
            state.dimension_tool.cancel();
            state.dimension_tool.set_message(reason);
        }
    } else if tool.pending().is_empty() {
        state.dimension_tool.set_mode(ToolMode::Off);
    } else {
        state.dimension_tool.cancel();
    }
}

/// Setzt die Maße des Werkzeugs an `at`; die Meldung steht danach in der Hinweiszeile.
fn tool_finish(state: &mut AppState, id: DrawingId, at: DVec2) -> Result<(), ToolError> {
    match with_tool(state, |t, st| t.finish(st, id, at)) {
        Ok(ids) => {
            state.drawing_ui.dim_message = Some(crate::msg!("dim-set", n = ids.len()));
            state.drawing_ui.dim_message_key = Some(message_key(state));
            Ok(())
        }
        // Die Ablehnung steht im Werkzeug (Hinweiszeile).
        Err(e) => Err(e),
    }
}

/// Gilt eine Taste dem Blatt (`sheet` ist die Blattfläche)? Nicht, solange ein Fenster oder
/// Menü offen ist (Ebene `Foreground`, im vorigen oder diesem Bild) oder der letzte Mausdruck
/// nicht auf dem Blatt lag (etwa auf einer Browserzeile). Ein Bedienelement mit Tastaturfokus
/// sperrt schon [`viewport::typing`] (außer den dafür freigegebenen Tab-Halten). Muss in jedem
/// Bild aufgerufen werden (merkt sich den letzten Mausdruck).
fn keys_for_sheet(ctx: &egui::Context, sheet: &egui::Response) -> bool {
    let id = egui::Id::new("planora-drawing-keys-on-sheet");
    if ctx.input(|i| i.pointer.any_pressed()) {
        let on = sheet.hovered();
        ctx.data_mut(|d| d.insert_temp(id, on));
    }
    let pressed_on_sheet = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
    let window_open = ctx.memory(|m| {
        m.areas()
            .visible_layer_ids()
            .iter()
            .any(|l| l.order == egui::Order::Foreground)
    });
    pressed_on_sheet && !window_open
}

/// Wählt den Nummernkreis `b` (Ansicht und Maß sind dann nicht mehr gewählt).
fn select_balloon(state: &mut AppState, b: BalloonId) {
    state.drawing_ui.selected_balloon = Some(b);
    state.drawing_ui.selected_view = None;
    state.drawing_ui.selected_dim = None;
}

/// Klick mit eingeschaltetem Werkzeug „Position“ an der Blattstelle `at`: auf einer Linie eines
/// Teils (`found`) setzt er einen Kreis und wählt ihn, sonst steht der Grund in der Hinweiszeile.
fn balloon_click(
    state: &mut AppState,
    id: DrawingId,
    found: Option<(ViewId, StrokeOrigin)>,
    at: DVec2,
) {
    let Some((view, origin)) = found else {
        state.balloon_tool.set_message(Some(balloon_tool::missed()));
        return;
    };
    let body = match origin {
        StrokeOrigin::Edge { body, .. } | StrokeOrigin::Silhouette { body, .. } => body,
    };
    match balloon_tool::place(state, id, view, body, at) {
        Ok(b) => {
            state.balloon_tool.set_message(None);
            select_balloon(state, b);
        }
        Err(m) => state.balloon_tool.set_message(Some(m)),
    }
}

/// Stand, zu dem eine Meldung des Vorschlags gehört: Änderungszähler des Dokuments, gewählte
/// Ansicht und gewähltes Maß.
fn message_key(state: &AppState) -> (u64, Option<ViewId>, Option<DimId>) {
    (
        state.document.edit_count(),
        state.drawing_ui.selected_view,
        state.drawing_ui.selected_dim,
    )
}

/// Fügt der geöffneten Zeichnung eine Ansicht der Art `kind` hinzu.
///
/// Normansicht: abgeleitet von der gewählten Normansicht (sonst von der Grundansicht) mit
/// deren Maßstab und dem Vorgabeabstand, wenn die Ausrichtungsregel eine Richtung liefert;
/// sonst frei (leere Zeichnung: in der Mitte des freien Bereichs, sonst links oben). Steht die
/// Grundansicht allein noch an ihrer automatischen Stelle in der Mitte, rückt sie mit der ersten
/// weiteren Ansicht so, dass der Block aus drei Ansichten mittig liegt (im selben Schritt; eine
/// vom Benutzer verschobene Grundansicht bleibt). Isometrisch: im leeren Viertel des Blocks um die
/// Grundansicht ([`iso_placement`]: Erstwinkel rechts unten, Drittwinkel links oben) im Maßstab
/// der Grundansicht oder, wenn sie dort sonst nicht hineinpasst, eine Stufe kleiner; ohne frei
/// liegende Grundansicht rechts oben (0,75 · Breite, 0,75 · Höhe des freien Bereichs) im
/// Maßstab der Grundansicht. Freie Ansichten bleiben mit ihrer
/// geschätzten Ausdehnung im freien Bereich, soweit sie hineinpassen; sonst sitzen sie dort
/// bündig an der rechten bzw. oberen Kante.
pub fn add_view(state: &mut AppState, kind: ViewKind) -> Result<ViewId, String> {
    let WorkArea::Drawing(id) = state.drawing_ui.area else {
        return Err(no_drawing_open().text());
    };
    let mut d = state
        .document
        .drawing(id)
        .ok_or_else(|| no_drawing_open().text())?
        .clone();
    if matches!(d.kind, DrawingKind::PartsList { .. }) {
        return Err(no_views_on_parts_sheet().text());
    }
    let (lo, hi) = placement_layout(state, &d).free;
    let free: FreeArea = ((lo.x, lo.y), (hi.x, hi.y));
    let (free_w, free_h) = (hi.x - lo.x, hi.y - lo.y);
    // Steht nur die Grundansicht noch an ihrer automatischen Stelle (Mitte des freien Bereichs),
    // rückt sie mit der ersten weiteren Ansicht so, dass der Block aus drei Ansichten mittig
    // liegt; hat der Benutzer sie verschoben, bleibt sie, wo sie ist.
    let recentred = recentre_single_base(state, &d, free);
    if let Some((vid, x, y)) = recentred
        && let Some(v) = d.views.iter_mut().find(|v| v.id == vid)
    {
        v.placement = Placement::Free { x, y };
    }
    let base = d
        .views
        .iter()
        .find(|v| v.parent.is_none() && v.kind.is_normal())
        .or(d.views.first());
    let base_scale = base.map(|b| b.scale).unwrap_or_else(|| {
        let k = if kind.is_normal() {
            kind
        } else {
            ViewKind::Front
        };
        suggest_scale_in(&state.bodies, (free_w, free_h), k)
    });
    // Freie Ansicht möglichst bei `fx`/`fy` des freien Bereichs, aber mit ihrer geschätzten
    // Ausdehnung darin.
    let at = |fx: f64, fy: f64, scale: Scale| {
        let (ew, eh) = estimate_view_size(&state.bodies, kind);
        let f = scale.factor();
        let (x, y) = keep_in_free(
            (lo.x + fx * free_w, lo.y + fy * free_h),
            (ew * f, eh * f),
            free,
        );
        Placement::Free { x, y }
    };
    let (placement, parent, scale) = if !kind.is_normal() {
        // Ins leere Viertel des Blocks um die frei liegende Grundansicht (wenn nötig kleiner);
        // ohne sie rechts oben im Maßstab der Grundansicht.
        match base {
            Some(b) if b.kind.is_normal() => match b.placement {
                Placement::Free { x, y } => {
                    let (bw, bh) = estimate_view_size(&state.bodies, b.kind);
                    let bf = b.scale.factor();
                    let ((x, y), scale) = iso_placement(
                        d.projection,
                        b.kind,
                        (x, y),
                        (bw * bf, bh * bf),
                        estimate_view_size(&state.bodies, kind),
                        base_scale,
                        free,
                    );
                    (Placement::Free { x, y }, None, scale)
                }
                Placement::Aligned { .. } => (at(0.75, 0.75, base_scale), None, base_scale),
            },
            _ => (at(0.75, 0.75, base_scale), None, base_scale),
        }
    } else {
        let selected = state
            .drawing_ui
            .selected_view
            .and_then(|s| d.view(s))
            .filter(|v| v.kind.is_normal() && v.section.is_none());
        let parent = selected
            .or(base.filter(|b| b.kind.is_normal()))
            .filter(|p| align_dir(d.projection, p.kind, kind).is_some());
        match parent {
            Some(p) => (
                Placement::Aligned {
                    distance: default_distance(&state.bodies, p.kind, kind, p.scale, p.scale),
                },
                Some(p.id),
                p.scale,
            ),
            None if d.views.is_empty() => {
                // Die erste Ansicht allein liegt in der Mitte des freien Bereichs.
                let (x, y) = free_centre(free);
                (Placement::Free { x, y }, None, base_scale)
            }
            None => (at(0.25, 0.75, base_scale), None, base_scale),
        }
    };
    // Grundansicht verschieben und neue Ansicht anlegen in einem Schritt.
    let mut new_id = None;
    state
        .document
        .update_drawing(id, |dr| {
            if let Some((vid, x, y)) = recentred
                && let Some(v) = dr.views.iter_mut().find(|v| v.id == vid)
            {
                v.placement = Placement::Free { x, y };
            }
            new_id = Some(dr.add_view(kind, scale, placement, parent));
        })
        .map_err(|e| e.to_string())?;
    new_id.ok_or_else(|| no_drawing_open().text())
}

/// Mitte des freien Bereichs.
pub fn free_centre(free: FreeArea) -> (f64, f64) {
    ((free.0.0 + free.1.0) / 2.0, (free.0.1 + free.1.1) / 2.0)
}

/// Abstand (mm), bis zu dem die Grundansicht noch als „an ihrer automatischen Stelle“ gilt.
const AUTO_PLACE_TOL_MM: f64 = 0.01;

/// Hat die Zeichnung genau eine Ansicht, eine frei liegende Normansicht in der Mitte des freien
/// Bereichs (so legt sie das Programm an), dann die neue Mitte für sie, bei der der Block aus
/// drei Ansichten mittig liegt ([`block_origin`]); sonst `None`.
fn recentre_single_base(
    state: &AppState,
    d: &Drawing,
    free: FreeArea,
) -> Option<(ViewId, f64, f64)> {
    let [v] = d.views.as_slice() else {
        return None;
    };
    let Placement::Free { x, y } = v.placement else {
        return None;
    };
    if v.parent.is_some() || !v.kind.is_normal() {
        return None;
    }
    let (cx, cy) = free_centre(free);
    if (x - cx).abs() > AUTO_PLACE_TOL_MM || (y - cy).abs() > AUTO_PLACE_TOL_MM {
        return None;
    }
    let (bx, by) = block_origin(&state.bodies, free, d.projection, v.kind, v.scale);
    Some((v.id, bx, by))
}

/// Linienzug einer Blattlinie (mm) mit Ansicht und Sichtbarkeit.
#[derive(Clone)]
struct Poly {
    view: ViewId,
    vis: Visibility,
    /// Herkunft der Blattlinie (für das Werkzeug „Maß“).
    origin: StrokeOrigin,
    points: Vec<DVec2>,
}

/// Blattfläche: Eingaben und Zeichnen auf `paint` (mit `response` darüber), Einpassen und
/// Umrechnen in `canvas`. Liefert die Aufteilung (ohne Werkzeug- und Hinweisfeld) und ob noch
/// etwas berechnet wird.
fn canvas_ui(
    ui: &mut egui::Ui,
    state: &mut AppState,
    id: DrawingId,
    (paint, canvas): (Rect, Rect),
    response: &egui::Response,
    editable: bool,
) -> Option<(DrawingLayout, bool)> {
    let drawing = state.document.drawing(id)?.clone();
    let sheet_mm = drawing.sheet.size_mm();
    if canvas.width() < 2.0 || canvas.height() < 2.0 {
        return None;
    }
    let model = state.document.model_edit_count();
    let ctx = ui.ctx().clone();

    // --- Eingaben (mit dem Stand vor diesem Bild) ---
    let k0 = f64::from(fit_px_per_mm(canvas, sheet_mm) * state.drawing_ui.zoom);
    let dims = resolve_in(state, id);
    let balloons0 = balloons_cached(&ctx, state, &drawing, model);
    let (sheet, lines_gen) = sheet_model(&ctx, state, &drawing, model, &dims, &balloons0);
    let dim_marks = marks_in(state, id, &dims);
    let placed = place_views(state, &drawing, &sheet, model);
    let (zoom0, pan0) = (state.drawing_ui.zoom, state.drawing_ui.pan);
    let to_screen0 = |p: DVec2| sheet_to_screen(canvas, sheet_mm, zoom0, pan0, p);
    // Die Linienzüge zum Treffen entstehen erst, wenn wirklich geklickt wird (und kommen
    // aus dem Zwischenspeicher, wenn das Bild sie ohnehin braucht).
    let hit = |pos: Pos2| {
        let polys = polylines_cached(&ctx, &sheet, lines_gen, k0);
        hit_test(&placed, &polys, &to_screen0, pos)
    };
    // Maße liegen über den Linien und werden zuerst getroffen; Nummernkreise noch davor.
    let hit_dim = |pos: Pos2| dim_hit(&ctx, &dim_marks, |_| true, &to_screen0, k0, pos);
    let hit_balloon = |pos: Pos2| balloon_tool::balloon_hit(&balloons0, &to_screen0, pos);
    // Mit eingeschaltetem Werkzeug „Maß“ wählen Klicks Bezüge statt Ansichten und Maße, und
    // Ziehen verschiebt nichts (nur das Blatt).
    let tool_on = editable && state.dimension_tool.is_active();
    // Mit eingeschaltetem Werkzeug „Position“ setzt ein Klick auf eine Linie einen Kreis; Kreise
    // lassen sich weiter wählen und ziehen, Ansichten und Maße nicht.
    let balloon_on = editable && state.balloon_tool.is_active();
    // Mit eingeschaltetem Werkzeug „Schnitt“ setzen zwei Klicks die Schnittlinie; Ansichten, Maße
    // und Kreise lassen sich dabei weder wählen noch ziehen.
    let section_on = editable && state.section_tool.is_active();

    // Zoom um den Zeiger (Mausrad, Zwei-Finger-Geste).
    if response.hovered()
        && let Some(p) = response.hover_pos()
    {
        // Das Rad nach der Tastenbelegung (`ViewZoom`; Strg+Rad mit egui's Zoomrate wie
        // `zoom_delta`), die Zwei-Finger-Geste immer.
        let km = crate::keymap::mouse::current(ui.ctx());
        let f = crate::keymap::mouse::pinch(ui)
            * crate::keymap::mouse::wheel_factor(&km, ui, WHEEL_RATE);
        if f != 1.0 && f.is_finite() {
            zoom_about(state, canvas, p, f);
        }
    }

    if response.drag_started() {
        let origin = ui
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos());
        if let Some(origin) = origin {
            // Schwenken nach der Tastenbelegung (Voreinstellung: Mitte oder Umschalt+links);
            // links ohne Umschalttasten wählt und zieht.
            let km = crate::keymap::mouse::current(ui.ctx());
            let drag = if crate::keymap::mouse::pan_2d(&km, ui, response) {
                Some(Drag::Pan {
                    origin,
                    start: state.drawing_ui.pan,
                })
            } else if response.dragged_by(PointerButton::Primary) && !tool_on && !section_on {
                if let Some(b) = hit_balloon(origin) {
                    select_balloon(state, b);
                    let at = balloons0.iter().find(|x| x.id == b).map(|x| x.centre);
                    let start = drawing.balloon(b).map(|x| x.offset);
                    match (editable, at, start) {
                        (true, Some(c), Some(start)) => Some(Drag::Balloon {
                            origin,
                            id: b,
                            at: c - DVec2::new(start.0, start.1),
                            start,
                        }),
                        _ => None,
                    }
                } else if balloon_on {
                    None
                } else if let Some((i, on_text)) = hit_dim(origin) {
                    let dm = &dim_marks[i];
                    state.drawing_ui.selected_dim = Some(dm.id);
                    state.drawing_ui.selected_view = None;
                    state.drawing_ui.selected_balloon = None;
                    let from = screen_to_sheet(canvas, sheet_mm, zoom0, pan0, origin);
                    editable
                        .then(|| dim_part(state, &drawing, model, &dims, dm, on_text, from))
                        .flatten()
                        .map(|part| Drag::Dim {
                            origin,
                            id: dm.id,
                            part,
                        })
                } else {
                    hit(origin).and_then(|view| {
                        state.drawing_ui.selected_view = Some(view);
                        state.drawing_ui.selected_dim = None;
                        state.drawing_ui.selected_balloon = None;
                        let start = drawing.view(view)?.placement;
                        editable.then_some(Drag::View {
                            origin,
                            view,
                            start,
                        })
                    })
                }
            } else {
                None
            };
            ctx.data_mut(|d| d.insert_temp(drag_id(), drag));
        }
    }
    let drag: Option<Drag> = ctx.data(|d| d.get_temp(drag_id())).flatten();
    // Versatz des gezogenen Kreises in diesem Bild (das Dokument ändert sich erst beim Loslassen).
    let mut live: Option<(BalloonId, (f64, f64))> = None;
    if let Some(drag) = drag
        && (response.dragged() || response.drag_stopped())
        && let Some(pos) = response
            .interact_pointer_pos()
            .or(ui.input(|i| i.pointer.latest_pos()))
    {
        match drag {
            Drag::Pan { origin, start } => state.drawing_ui.pan = start + (pos - origin),
            Drag::View {
                origin,
                view,
                start,
            } => {
                let delta = pos - origin;
                let dm = DVec2::new(f64::from(delta.x) / k0, -f64::from(delta.y) / k0);
                if let Some(placement) = moved(&drawing, view, start, dm) {
                    let r = state.document.update_drawing(id, |d| {
                        if let Some(v) = d.views.iter_mut().find(|v| v.id == view) {
                            v.placement = placement;
                        }
                    });
                    if let Err(e) = r {
                        state.message = Some(e.to_string());
                    }
                }
            }
            Drag::Dim {
                origin,
                id: dim,
                part,
            } => {
                let delta = pos - origin;
                let dm = DVec2::new(f64::from(delta.x) / k0, -f64::from(delta.y) / k0);
                if let Some(cur) = drawing.dimension(dim) {
                    let changed = dragged_group(&drawing, cur, part, dm);
                    if !changed.is_empty() {
                        // Ein Schritt für die ganze Gruppe (Kette oder Bezugsmaße).
                        let r = state.document.update_drawing(id, |d| {
                            for next in changed {
                                if let Some(m) = d.dimensions.iter_mut().find(|m| m.id == next.id) {
                                    *m = next;
                                }
                            }
                        });
                        if let Err(e) = r {
                            state.message = Some(e.to_string());
                        }
                    }
                }
            }
            Drag::Balloon {
                origin,
                id: b,
                at,
                start,
            } => {
                let delta = pos - origin;
                let moved = (
                    start.0 + f64::from(delta.x) / k0,
                    start.1 - f64::from(delta.y) / k0,
                );
                let offset = balloon_tool::clamp_offset(at, moved, sheet_mm);
                if !response.drag_stopped() {
                    live = Some((b, offset));
                } else if offset != start {
                    // Ein Schritt je Ziehen.
                    let r = state.document.update_drawing(id, |d| {
                        if let Some(x) = d.balloons.iter_mut().find(|x| x.id == b) {
                            x.offset = offset;
                        }
                    });
                    if let Err(e) = r {
                        state.message = Some(e.to_string());
                    }
                }
            }
        }
    }
    if response.drag_stopped() || !response.is_pointer_button_down_on() && !response.dragged() {
        ctx.data_mut(|d| d.remove::<Option<Drag>>(drag_id()));
    }
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        if tool_on {
            let at = screen_to_sheet(canvas, sheet_mm, zoom0, pan0, pos);
            let polys = polylines_cached(&ctx, &sheet, lines_gen, k0);
            let found = tool_hit(&sheet, &polys, &to_screen0, pos);
            tool_click(state, id, found, at, k0, response.double_clicked());
        } else if section_on {
            let at = screen_to_sheet(canvas, sheet_mm, zoom0, pan0, pos);
            section_tool::click(state, id, at, k0);
        } else if let Some(b) = hit_balloon(pos) {
            select_balloon(state, b);
            state.balloon_tool.set_message(None);
        } else if balloon_on {
            let at = screen_to_sheet(canvas, sheet_mm, zoom0, pan0, pos);
            let polys = polylines_cached(&ctx, &sheet, lines_gen, k0);
            let found = tool_hit(&sheet, &polys, &to_screen0, pos);
            balloon_click(state, id, found, at);
        } else if let Some((i, _)) = hit_dim(pos) {
            let dim = dim_marks[i].id;
            state.drawing_ui.selected_dim = Some(dim);
            state.drawing_ui.selected_view = None;
            state.drawing_ui.selected_balloon = None;
            // Doppelklick auf ein Maß öffnet „Maß bearbeiten“ (in einer alten Version nur zum Ansehen).
            if response.double_clicked() {
                crate::dimension_dialog::open(state, id, dim, !editable);
            }
        } else {
            // Die Schnittlinie wählt ihre Schnittansicht (vor den Linien der Quellansicht).
            let on_line = drawing.views.iter().any(|v| v.section.is_some()).then(|| {
                let owned = state.drawing_ui.results_for(&drawing, model);
                let refs: BTreeMap<ViewId, &ViewLines> =
                    owned.iter().map(|(k, l)| (*k, l.as_ref())).collect();
                let lines = section_tool::section_lines(&drawing, &refs);
                section_tool::section_line_hit(&lines, &to_screen0, pos, PICK_PX)
            });
            state.drawing_ui.selected_view = on_line.flatten().or_else(|| hit(pos));
            state.drawing_ui.selected_dim = None;
            state.drawing_ui.selected_balloon = None;
        }
    }
    // Tasten nicht, solange ein Textfeld die Tastatur hat.
    let typing = viewport::typing(&ctx);
    if !typing && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        if tool_on {
            tool_escape(state, id);
        } else if balloon_on {
            state.balloon_tool.set_active(false);
        } else if section_on {
            // Esc nimmt erst den ersten Klick zurück, dann schaltet es das Werkzeug aus.
            if !state.section_tool.cancel() {
                state.section_tool.set_active(false);
            }
        } else {
            state.drawing_ui.selected_view = None;
            state.drawing_ui.selected_dim = None;
            state.drawing_ui.selected_balloon = None;
        }
    }
    // Entf löscht den gewählten Nummernkreis – nur, wenn die Taste dem Blatt gilt.
    let sheet_keys = keys_for_sheet(&ctx, response);
    if editable
        && !typing
        && sheet_keys
        && state.drawing_ui.selected_balloon.is_some()
        && ui.input(|i| i.key_pressed(egui::Key::Delete))
    {
        balloon_tool::delete_selected(state);
    }
    // Tooltip mit dem Grund über einem Kreis, dessen Teil fehlt, oder einem Maß, das seinen
    // Bezug verloren hat.
    if response.hovered()
        && !response.dragged()
        && let Some(p) = response.hover_pos()
    {
        let lost_balloons: Vec<BalloonMarks> = balloons0
            .iter()
            .filter(|b| b.marks.iter().any(|m| mark_kind(m) == MarkKind::Lost))
            .cloned()
            .collect();
        let lost = |dm: &DimMarks| dm.marks.iter().any(|m| mark_kind(m) == MarkKind::Lost);
        if balloon_tool::balloon_hit(&lost_balloons, &to_screen0, p).is_some() {
            response
                .clone()
                .on_hover_text_at_pointer(balloon_tool::lost_tip());
        } else if let Some((i, _)) = dim_hit(&ctx, &dim_marks, lost, &to_screen0, k0, p) {
            let reason = dims
                .by_view
                .values()
                .flatten()
                .find(|(s, _)| s.id == dim_marks[i].id)
                .and_then(|(_, r)| r.as_ref().err())
                .map(|e| e.to_string());
            let text = match reason {
                Some(r) => format!("{} {r}", lost_tip()),
                None => lost_tip(),
            };
            response.clone().on_hover_text_at_pointer(text);
        }
    }

    // --- Zeichnen mit dem neuen Stand ---
    let drawing = state.document.drawing(id)?.clone();
    let (zoom, pan) = (state.drawing_ui.zoom, state.drawing_ui.pan);
    let k = f64::from(fit_px_per_mm(canvas, sheet_mm) * zoom);
    let dims = resolve_in(state, id);
    let balloons = balloons_cached(&ctx, state, &drawing, model);
    let live_balloons = match live {
        Some(m) => {
            let numbers = balloon_tool::numbers_cached(&ctx, state, id);
            balloon_tool::with_moved(balloons.to_vec(), &drawing, &numbers, m)
        }
        None => balloons.to_vec(),
    };
    let (sheet, lines_gen) = sheet_model(&ctx, state, &drawing, model, &dims, &balloons);
    let dim_marks = marks_in(state, id, &dims);
    let placed = place_views(state, &drawing, &sheet, model);
    let polys = polylines_cached(&ctx, &sheet, lines_gen, k);
    let to_screen = |p: DVec2| sheet_to_screen(canvas, sheet_mm, zoom, pan, p);
    let painter = ui.painter_at(paint);
    painter.rect_filled(paint, 0.0, theme::drawing_surround(state.dark));
    let paper = Rect::from_two_pos(
        to_screen(DVec2::ZERO),
        to_screen(DVec2::new(sheet_mm.0, sheet_mm.1)),
    );
    painter.rect_filled(paper.translate(vec2(4.0, 4.0)), 0.0, theme::PAPER_SHADOW);
    painter.rect(
        paper,
        0.0,
        theme::PAPER,
        Stroke::new(1.0, theme::SHEET_FRAME),
        StrokeKind::Outside,
    );
    // Blattrahmen, Schriftfeld und Tabellen: schwarz auf dem Papier, unter den Ansichtslinien.
    if let Some(frame) = &sheet.frame {
        for m in &frame.marks {
            paint_mark(&painter, m, theme::SHEET_INK, &to_screen, k, frame.scale);
        }
    }
    let visible = Stroke::new((VISIBLE_MM * k).max(1.0) as f32, theme::SHEET_INK);
    let hidden = Stroke::new((HIDDEN_MM * k).max(1.0) as f32, theme::SHEET_INK);
    for p in polys.iter() {
        match p.vis {
            Visibility::Visible => {
                painter.add(Shape::line(
                    p.points.iter().map(|q| to_screen(*q)).collect(),
                    visible,
                ));
            }
            Visibility::Hidden => {
                for d in dashes(&p.points, DASH_MM, DASH_GAP_MM) {
                    painter.add(Shape::line(
                        d.iter().map(|q| to_screen(*q)).collect(),
                        hidden,
                    ));
                }
            }
        }
    }

    // Markierungen: Mittellinien, Maße und Nummernkreise schwarz, verlorene rot, das gewählte
    // Maß bzw. der gewählte (oder gezogene) Kreis in der Akzentfarbe (darüber; ihre Markierungen
    // fallen in der normalen Farbe weg).
    let selected: &[Mark] = state
        .drawing_ui
        .selected_dim
        .and_then(|s| dim_marks.iter().find(|dm| dm.id == s))
        .map_or(&[], |dm| dm.marks.as_slice());
    let accent_balloon = live.map(|(b, _)| b).or(state.drawing_ui.selected_balloon);
    let skip: Vec<&Mark> = balloons
        .iter()
        .filter(|b| Some(b.id) == accent_balloon)
        .flat_map(|b| b.marks.iter())
        .collect();
    let mut marks_drawn = 0;
    let f_sheet = sheet_scale(sheet.sheet);
    for v in &sheet.views {
        for m in &v.marks {
            if selected.contains(m) || skip.contains(&m) {
                continue;
            }
            let color = match mark_kind(m) {
                MarkKind::Lost => theme::SHEET_LOST,
                MarkKind::Dimension
                | MarkKind::CenterLine
                | MarkKind::Sheet(_)
                | MarkKind::Position(_)
                | MarkKind::Hatch
                | MarkKind::SectionLine(_) => theme::SHEET_DIM,
            };
            paint_mark(&painter, m, color, &to_screen, k, f_sheet);
            marks_drawn += 1;
        }
    }
    let accent_marks = live_balloons
        .iter()
        .filter(|b| Some(b.id) == accent_balloon)
        .flat_map(|b| b.marks.iter());
    for m in selected.iter().chain(accent_marks) {
        paint_mark(&painter, m, theme::SHEET_SELECTED, &to_screen, k, f_sheet);
        marks_drawn += 1;
    }
    // Vorschau des Werkzeugs „Maß“ am Zeiger (dieselben Markierungen wie das gesetzte Maß).
    let mut preview_drawn = 0;
    if tool_on && let Some(hp) = response.hover_pos() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        let p = screen_to_sheet(canvas, sheet_mm, zoom, pan, hp);
        let preview = with_tool(state, |t, st| t.hover(st, id, p)).unwrap_or_default();
        for m in &preview {
            paint_mark(&painter, m, theme::SHEET_SELECTED, &to_screen, k, 1.0);
            preview_drawn += 1;
        }
    }

    // Vorschau des Werkzeugs „Schnitt“ nach dem ersten Klick: Schnittlinie mit Pfeilen.
    if section_on && let Some(hp) = response.hover_pos() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        let p = screen_to_sheet(canvas, sheet_mm, zoom, pan, hp);
        for m in &section_tool::preview(state, id, p, k) {
            paint_mark(&painter, m, theme::SHEET_SELECTED, &to_screen, k, f_sheet);
            preview_drawn += 1;
        }
    }

    let font = FontId::proportional(SHEET_FONT);
    let accent = theme::accent(state.dark);
    let mut drawn = Vec::with_capacity(placed.len());
    let mut pending = false;
    for v in &placed {
        let rect = Rect::from_two_pos(to_screen(v.min), to_screen(v.max));
        let mut bar = None;
        match &v.state {
            DrawnState::Lines => {}
            DrawnState::Pending(f) => {
                pending = true;
                painter.rect_stroke(
                    rect,
                    2.0,
                    Stroke::new(1.0, theme::SHEET_PENDING),
                    StrokeKind::Inside,
                );
                // Texte und Balken bleiben im Rahmen, auch wenn er zu klein ist.
                let painter = painter.with_clip_rect(rect.intersect(paint));
                let inner = rect.shrink(6.0);
                let name = painter.layout(
                    v.name.clone(),
                    font.clone(),
                    theme::SHEET_PENDING,
                    inner.width().max(1.0),
                );
                let text = painter.layout(
                    pending_text(*f),
                    font.clone(),
                    theme::SHEET_PENDING,
                    inner.width().max(1.0),
                );
                let gap = 4.0;
                let bar_h = 6.0;
                let total = name.size().y + text.size().y + bar_h + 2.0 * gap;
                let mut y = inner.center().y - total / 2.0;
                painter.galley(
                    pos2(inner.center().x - name.size().x / 2.0, y),
                    name.clone(),
                    theme::SHEET_PENDING,
                );
                y += name.size().y + gap;
                painter.galley(
                    pos2(inner.center().x - text.size().x / 2.0, y),
                    text.clone(),
                    theme::SHEET_PENDING,
                );
                y += text.size().y + gap;
                let outer =
                    Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width().max(0.0), bar_h));
                let fill =
                    Rect::from_min_size(outer.min, vec2(outer.width() * f.clamp(0.0, 1.0), bar_h));
                painter.rect_filled(outer, 3.0, theme::SHEET_PROGRESS_TRACK);
                painter.rect_filled(fill, 3.0, accent);
                bar = Some((outer, fill));
            }
            DrawnState::Failed(m) => {
                painter.rect_stroke(
                    rect,
                    2.0,
                    Stroke::new(1.0, theme::SHEET_LOST),
                    StrokeKind::Inside,
                );
                // Der Text bleibt im Rahmen, auch wenn er zu klein ist. Rot des Papiers (das Blatt
                // ist in beiden Themen weiß).
                let painter = painter.with_clip_rect(rect.intersect(paint));
                let inner = rect.shrink(6.0);
                let galley = painter.layout(
                    format!("{}: {m}", v.name),
                    font.clone(),
                    theme::SHEET_LOST,
                    inner.width().max(1.0),
                );
                painter.galley(inner.min, galley, theme::SHEET_LOST);
            }
        }
        if state.drawing_ui.selected_view == Some(v.id) {
            let pad = ((SELECT_PAD_MM * k) as f32).max(4.0);
            let r = rect.expand(pad);
            let path = [
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
                r.left_top(),
            ];
            painter.extend(Shape::dashed_line(
                &path,
                Stroke::new(1.5, accent),
                6.0,
                4.0,
            ));
        }
        drawn.push(DrawnView {
            id: v.id,
            rect,
            state: v.state.clone(),
            bar,
        });
    }
    let layout = DrawingLayout {
        area: paint,
        toolbar: canvas,
        canvas,
        paint,
        hint: canvas,
        sheet_mm,
        zoom,
        pan,
        px_per_mm: k as f32,
        lines_drawn: sheet.views.iter().map(|v| v.lines.len()).sum(),
        marks_drawn,
        preview_drawn,
        views: drawn,
        balloons: live_balloons,
    };
    Some((layout, pending))
}

/// Text des Platzhalters einer Ansicht, die berechnet wird.
pub fn pending_text(fraction: f32) -> String {
    tr!(
        "drawing-pending",
        percent = (fraction.clamp(0.0, 1.0) * 100.0).round() as i32
    )
}

/// Neue Lage der Ansicht `view` nach einer Bewegung um `dm` (Blatt-mm) ab `start`: frei in x
/// und y, abgeleitet nur der Anteil längs der Ausrichtungsachse.
fn moved(drawing: &Drawing, view: ViewId, start: Placement, dm: DVec2) -> Option<Placement> {
    Some(match start {
        Placement::Free { x, y } => Placement::Free {
            x: x + dm.x,
            y: y + dm.y,
        },
        Placement::Aligned { distance } => {
            let v = drawing.view(view)?;
            let parent = drawing.view(v.parent?)?;
            let (ax, ay) = align_dir(drawing.projection, parent.kind, v.kind)?;
            Placement::Aligned {
                distance: distance + dm.dot(DVec2::new(ax, ay)),
            }
        }
    })
}

fn zoom_about(state: &mut AppState, canvas: Rect, p: Pos2, f: f32) {
    let ui = &mut state.drawing_ui;
    let z = (ui.zoom * f).clamp(MIN_ZOOM, MAX_ZOOM);
    let f = z / ui.zoom;
    let c = canvas.center().to_vec2();
    let rel = p.to_vec2() - c;
    // Der Blattpunkt unter `p` bleibt: p − c − pan' = (p − c − pan) · f.
    ui.pan = rel - (rel - ui.pan) * f;
    ui.zoom = z;
}

/// Zwischengespeicherte Linienzüge: Stand der Linien (eindeutige Nummer, siehe
/// [`sheet_model`]), Zoomstufe und Ergebnis.
#[derive(Clone)]
struct PolyCache {
    lines_gen: u64,
    bucket: i32,
    polys: Arc<Vec<Poly>>,
}

fn polys_cache_id() -> egui::Id {
    egui::Id::new("planora-drawing-polys")
}

/// Zoomstufe: Viertelschritte von Zweierpotenzen, nach oben gerundet.
fn zoom_bucket(k: f64) -> i32 {
    (k.max(1e-9).log2() * 4.0).ceil() as i32
}

/// Linienzüge für `k` Pixel je mm: gezeichnet wird mit der Stufe, die nicht gröber ist als `k`
/// (die Abweichung bleibt unter einem Pixel). Solange sich der Stand der Linien `lines_gen`
/// und die Stufe nicht ändern, wird nichts neu zerlegt (auch nicht, wenn sich nur Maße oder
/// Mittellinien ändern).
fn polylines_cached(
    ctx: &egui::Context,
    sheet: &SheetModel,
    lines_gen: u64,
    k: f64,
) -> Arc<Vec<Poly>> {
    let bucket = zoom_bucket(k);
    if let Some(c) = ctx.data(|d| d.get_temp::<PolyCache>(polys_cache_id()))
        && c.bucket == bucket
        && c.lines_gen == lines_gen
    {
        return c.polys;
    }
    let polys = Arc::new(polylines(sheet, 2f64.powf(f64::from(bucket) / 4.0)));
    ctx.data_mut(|d| {
        d.insert_temp(
            polys_cache_id(),
            PolyCache {
                lines_gen,
                bucket,
                polys: Arc::clone(&polys),
            },
        )
    });
    polys
}

/// Art einer Markierung.
fn mark_kind(m: &Mark) -> MarkKind {
    match m {
        Mark::Line { kind, .. }
        | Mark::Arrow { kind, .. }
        | Mark::Arc { kind, .. }
        | Mark::Text { kind, .. } => *kind,
    }
}

/// Ecken eines Pfeils auf dem Blatt: Spitze und die beiden Ecken am Ende.
fn arrow_corners(tip: DVec2, dir: DVec2, size: (f64, f64)) -> [DVec2; 3] {
    let base = tip + dir * size.0;
    let side = dir.perp() * size.1;
    [tip, base + side, base - side]
}

/// Punkte eines Bogens auf dem Blatt, höchstens [`FLATTEN_TOL_PX`] von ihm entfernt bei `k`
/// Pixeln je mm.
fn arc_points(center: DVec2, r: f64, start: f64, end: f64, k: f64) -> Vec<DVec2> {
    let n = arc_segments(r * k, end - start);
    (0..=n)
        .map(|i| {
            let t = start + (end - start) * i as f64 / n as f64;
            center + DVec2::new(t.cos(), t.sin()) * r
        })
        .collect()
}

/// Lage eines Maßtextes auf dem Bildschirm.
struct TextGeo {
    /// Ursprung des Textes (obere linke Ecke vor dem Drehen) und Drehpunkt.
    pos: Pos2,
    galley: Arc<Galley>,
    /// Drehung im Uhrzeigersinn (Bildschirm).
    angle: f32,
}

impl TextGeo {
    /// Die vier Ecken der gedrehten Hüllbox.
    fn corners(&self) -> [Pos2; 4] {
        let r = self.galley.rect;
        let rot = egui::emath::Rot2::from_angle(self.angle);
        [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
        ]
        .map(|c| self.pos + rot * c.to_vec2())
    }
}

/// Text einer Markierung als `egui`-Text in `Höhe·k` Pixeln (Roboto), mit Drehung und Anker auf
/// der Grundlinie wie auf dem Blatt; `None` für andere Markierungen und für Texte unter
/// [`MIN_TEXT_PX`]. Die Farbe setzt erst die Form (Platzhalter im Text).
fn text_geo(
    ctx: &egui::Context,
    m: &Mark,
    to_screen: &dyn Fn(DVec2) -> Pos2,
    k: f64,
) -> Option<TextGeo> {
    let Mark::Text {
        pos,
        height,
        angle,
        anchor,
        text,
        ..
    } = m
    else {
        return None;
    };
    let size = height * k;
    // Blatttexte (Schriftfeld, Raster) bleiben auch auf einem weit verkleinerten Blatt da – dann
    // in der kleinsten Größe; alle anderen Texte entfallen unter der Mindestgröße.
    let size = if matches!(mark_kind(m), MarkKind::Sheet(_)) && size.is_finite() {
        size.max(MIN_TEXT_PX)
    } else {
        size
    };
    if !(size.is_finite() && size >= MIN_TEXT_PX) || text.is_empty() {
        return None;
    }
    // Halbe Pixel: beim Zoomen entsteht nicht in jedem Bild eine neue Schriftgröße.
    let size = ((size.min(MAX_TEXT_PX) * 2.0).round() / 2.0).max(MIN_TEXT_PX) as f32;
    let galley = ctx.fonts_mut(|f| {
        f.layout(
            text.clone(),
            FontId::proportional(size),
            Color32::PLACEHOLDER,
            f32::INFINITY,
        )
    });
    let rect = galley.rect;
    let baseline = galley
        .rows
        .first()
        .and_then(|r| r.row.glyphs.first().map(|g| r.pos.y + g.pos.y))
        .unwrap_or(0.8 * size);
    let ax = match anchor {
        TextAnchor::Start => rect.left(),
        TextAnchor::Middle => rect.center().x,
        TextAnchor::End => rect.right(),
    };
    // Blatt: Winkel gegen den Uhrzeigersinn, y nach oben; Bildschirm: im Uhrzeigersinn.
    let a = -(*angle as f32);
    let rot = egui::emath::Rot2::from_angle(a);
    let origin = to_screen(*pos) - rot * vec2(ax, baseline);
    Some(TextGeo {
        pos: origin,
        galley,
        angle: a,
    })
}

/// Strichbreite einer Markierung in Blatt-mm: Mittellinien [`CENTER_W`], Blattrahmen die Breite
/// ihres Stifts beim Faktor `sheet_scale` des Blatts, sonst [`DIM_LINE_W`].
fn line_width_mm(kind: MarkKind, sheet_scale: f64) -> f64 {
    match kind {
        MarkKind::CenterLine => CENTER_W,
        MarkKind::Sheet(pen) => pen.width_at(sheet_scale),
        MarkKind::Position(pen) => pen.width_at(sheet_scale),
        MarkKind::SectionLine(SectionPen::End) => SECTION_END_W,
        _ => DIM_LINE_W,
    }
}

/// Zeichnet eine Markierung: Linien `max(1 px, Breite·k)` (Mittellinien strichpunktiert nach
/// [`CENTER_PATTERN`]), Pfeile gefüllt, Bögen als Polylinie, Texte als `egui`-Text.
fn paint_mark(
    painter: &egui::Painter,
    m: &Mark,
    color: Color32,
    to_screen: &dyn Fn(DVec2) -> Pos2,
    k: f64,
    sheet_scale: f64,
) {
    match m {
        Mark::Line { a, b, kind } => {
            let w = line_width_mm(*kind, sheet_scale);
            let stroke = Stroke::new((w * k).max(1.0) as f32, color);
            if matches!(
                *kind,
                MarkKind::CenterLine | MarkKind::SectionLine(SectionPen::Chain)
            ) {
                for d in dash_pattern(&[*a, *b], &CENTER_PATTERN) {
                    painter.add(Shape::line(
                        d.iter().map(|q| to_screen(*q)).collect(),
                        stroke,
                    ));
                }
            } else {
                painter.line_segment([to_screen(*a), to_screen(*b)], stroke);
            }
        }
        Mark::Arrow { tip, dir, kind } => {
            let pts = arrow_corners(*tip, *dir, arrow_size(*kind, sheet_scale))
                .map(to_screen)
                .to_vec();
            painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Mark::Arc {
            center,
            r,
            start,
            end,
            kind,
        } => {
            let stroke = Stroke::new(
                (line_width_mm(*kind, sheet_scale) * k).max(1.0) as f32,
                color,
            );
            let pts = arc_points(*center, *r, *start, *end, k);
            painter.add(Shape::line(
                pts.into_iter().map(to_screen).collect(),
                stroke,
            ));
        }
        Mark::Text { .. } => {
            if let Some(t) = text_geo(painter.ctx(), m, to_screen, k) {
                painter.add(TextShape::new(t.pos, t.galley, color).with_angle(t.angle));
            }
        }
    }
}

/// Abstand (Pixel) des Punkts `p` vom Dreieck/Viereck `poly` (null innen).
fn polygon_dist(p: Pos2, poly: &[Pos2]) -> f32 {
    let n = poly.len();
    let mut inside = true;
    let mut sign = 0.0f32;
    let mut best = f32::INFINITY;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        best = best.min(screen_seg_dist(p, a, b));
        let c = (b - a).x * (p - a).y - (b - a).y * (p - a).x;
        if c != 0.0 {
            if sign == 0.0 {
                sign = c.signum();
            } else if c.signum() != sign {
                inside = false;
            }
        }
    }
    if inside { 0.0 } else { best }
}

/// Maß unter `pos` (nur Maße, für die `only` gilt): das nächste im Abstand bis [`PICK_PX`] zu
/// einer Linie, einem Pfeil, einem Bogen oder der Hüllbox eines Textes. Liefert den Index in
/// `marks` und ob der Text getroffen wurde.
fn dim_hit(
    ctx: &egui::Context,
    marks: &[DimMarks],
    only: impl Fn(&DimMarks) -> bool,
    to_screen: &dyn Fn(DVec2) -> Pos2,
    k: f64,
    pos: Pos2,
) -> Option<(usize, bool)> {
    let mut best: Option<(f32, usize, bool)> = None;
    for (i, dm) in marks.iter().enumerate() {
        if !only(dm) {
            continue;
        }
        for m in &dm.marks {
            let (d, text) = match m {
                Mark::Line { a, b, .. } => {
                    (screen_seg_dist(pos, to_screen(*a), to_screen(*b)), false)
                }
                Mark::Arrow { tip, dir, .. } => (
                    polygon_dist(
                        pos,
                        &arrow_corners(*tip, *dir, (ARROW_LEN, ARROW_HALF_W)).map(to_screen),
                    ),
                    false,
                ),
                Mark::Arc {
                    center,
                    r,
                    start,
                    end,
                    ..
                } => {
                    let pts: Vec<Pos2> = arc_points(*center, *r, *start, *end, k)
                        .into_iter()
                        .map(to_screen)
                        .collect();
                    let d = pts
                        .windows(2)
                        .map(|w| screen_seg_dist(pos, w[0], w[1]))
                        .fold(f32::INFINITY, f32::min);
                    (d, false)
                }
                Mark::Text { .. } => match text_geo(ctx, m, to_screen, k) {
                    Some(t) => (polygon_dist(pos, &t.corners()), true),
                    None => continue,
                },
            };
            // Bei gleichem Abstand gewinnt der Text (er liegt über den Linien).
            let better = best.is_none_or(|(b, _, bt)| d < b || (d == b && text && !bt));
            if d <= PICK_PX && better {
                best = Some((d, i, text));
            }
        }
    }
    best.map(|(_, i, t)| (i, t))
}

/// Querachse eines linearen Maßes zur Messrichtung `u` wie im Layout: `u` um 90° gedreht, nach
/// rechts, bei senkrechter Lage nach oben – positiver Abstand liegt also bei waagerechten
/// Maßen oben, bei senkrechten rechts.
fn cross_axis(u: DVec2) -> DVec2 {
    let t = u.perp();
    if t.x < -1e-9 || (t.x.abs() <= 1e-9 && t.y < 0.0) {
        -t
    } else {
        t
    }
}

/// Winkel in Grad auf `[−180, 180)`.
pub(crate) fn normalize_deg(a: f64) -> f64 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// Teil des Maßes `dm`, der ab dem Blattpunkt `from` gezogen wird (Text, wenn `on_text`); die
/// Lage kommt aus den eigenen Markierungen des Maßes und seinem Anker (gemessen oder zuletzt
/// bekannt). `None`, wenn sich das Maß so nicht ziehen lässt.
fn dim_part(
    state: &AppState,
    drawing: &Drawing,
    model: u64,
    dims: &ResolvedDims,
    dm: &DimMarks,
    on_text: bool,
    from: DVec2,
) -> Option<DimPart> {
    let spec = drawing.dimension(dm.id)?;
    if on_text {
        return Some(DimPart::Text {
            shift: spec.text_shift,
        });
    }
    let measured = dims
        .by_view
        .get(&dm.view)
        .and_then(|l| l.iter().find(|(s, _)| s.id == dm.id))
        .and_then(|(_, r)| r.as_ref().ok().map(|m| m.anchor.clone()));
    let anchor = measured.or_else(|| dims.last_known.get(&dm.id).cloned())?;
    let arrows: Vec<DVec2> = dm
        .marks
        .iter()
        .filter_map(|m| match m {
            Mark::Arrow { dir, .. } => Some(*dir),
            _ => None,
        })
        .collect();
    match anchor {
        Anchor::Linear { dir, .. } => {
            // Messrichtung aus den Pfeilen des Maßes (sie liegen auf der Maßlinie).
            let u = arrows.first().copied().unwrap_or(dir).normalize_or_zero();
            (u != DVec2::ZERO).then(|| DimPart::Offset {
                w: cross_axis(u),
                offset: spec.offset,
            })
        }
        Anchor::Round {
            center,
            radius,
            arc,
        } => {
            let owned = state.drawing_ui.results_for(drawing, model);
            let refs: BTreeMap<ViewId, &ViewLines> =
                owned.iter().map(|(k, v)| (*k, v.as_ref())).collect();
            let xf = view_transform(drawing, dm.view, &refs)?;
            let c = xf.apply(center);
            if arrows.len() >= 2 {
                // Ø-Linie durch die Mitte (zwei Pfeile): drehen.
                return Some(DimPart::Rotate {
                    center: c,
                    from,
                    angle: spec.angle,
                });
            }
            let r = radius * xf.scale;
            // Der Knick, wie er gezeichnet ist (an einem Bogen ggf. in der Bogenmitte).
            let th = leader_angle(arc.as_ref(), spec.angle);
            let u = DVec2::new(th.cos(), th.sin());
            Some(DimPart::Leader {
                center: c,
                r,
                knee: c + u * (r + spec.offset),
                arc,
            })
        }
        Anchor::Angle { .. } => {
            let vertex = dm.marks.iter().find_map(|m| match m {
                Mark::Arc { center, .. } => Some(*center),
                _ => None,
            })?;
            let dir = (from - vertex).normalize_or_zero();
            let radius = if spec.offset.abs() > 1e-9 {
                spec.offset.abs()
            } else {
                DEFAULT_OFFSET
            };
            let sign = if spec.offset < 0.0 { -1.0 } else { 1.0 };
            (dir != DVec2::ZERO).then_some(DimPart::Radial { dir, radius, sign })
        }
    }
}

/// Die durch das Ziehen geänderten Maße. Wird die Maßlinie eines Glieds einer Kette oder von
/// Bezugsmaßen gezogen, wandert die ganze Gruppe mit (gleiche Verschiebung für jedes Glied, je
/// Glied auf seiner Seite begrenzt): die gemeinsame Maßlinie bzw. die Stufen bleiben erhalten.
/// Text, Führungslinie und Winkel ziehen nur das eine Maß.
fn dragged_group(drawing: &Drawing, cur: &DimSpec, part: DimPart, dm: DVec2) -> Vec<DimSpec> {
    // `part` hält den Abstand vom Beginn des Ziehens, `dm` die ganze Bewegung seither; das
    // gezogene Maß bekommt seinen Abstand daraus, die übrigen Glieder dieselbe Änderung
    // gegenüber ihrem jetzigen Abstand.
    let mut lead = cur.clone();
    apply_dim_drag(&mut lead, part, dm);
    let delta = lead.offset - cur.offset;
    let group = match (part, cur.chain) {
        (DimPart::Offset { .. }, Some(c)) if delta != 0.0 => Some(c.group),
        _ => None,
    };
    let mut out: Vec<DimSpec> = Vec::new();
    if lead != *cur {
        out.push(lead);
    }
    let Some(group) = group else {
        return out;
    };
    for m in &drawing.dimensions {
        if m.id == cur.id || !m.chain.is_some_and(|c| c.group == group) {
            continue;
        }
        let o = m.offset + delta;
        let mut next = m.clone();
        next.offset = if m.offset < 0.0 {
            o.min(-MIN_DRAG_OFFSET_MM)
        } else {
            o.max(MIN_DRAG_OFFSET_MM)
        };
        if next != *m {
            out.push(next);
        }
    }
    out
}

/// Wendet eine Bewegung `dm` (Blatt-mm, y nach oben) des gezogenen Teils auf das Maß an.
fn apply_dim_drag(spec: &mut DimSpec, part: DimPart, dm: DVec2) {
    // Ohne Bewegung bleibt alles, wie es ist (keine Rundung von Winkel und Abstand).
    if dm == DVec2::ZERO || !dm.is_finite() {
        return;
    }
    match part {
        DimPart::Offset { w, offset } => {
            // Die Seite bleibt beim Ziehen dieselbe: über die Körperkante hinaus springt die
            // Maßlinie nicht auf die andere Seite (Bezugslage wechselt im Layout mit dem
            // Vorzeichen), sie bleibt knapp neben der Kante stehen.
            let next = offset + dm.dot(w);
            spec.offset = if offset < 0.0 {
                next.min(-MIN_DRAG_OFFSET_MM)
            } else {
                next.max(MIN_DRAG_OFFSET_MM)
            };
        }
        DimPart::Text { shift } => spec.text_shift = (shift.0 + dm.x, shift.1 + dm.y),
        DimPart::Leader {
            center,
            r,
            knee,
            arc,
        } => {
            let v = knee + dm - center;
            let len = v.length();
            if len.is_finite() && len > 1e-9 {
                // An einem Bogen bleibt die Spitze auf dem Bogen (Winkel auf sein näheres Ende
                // geklemmt).
                let deg = clamp_leader_deg(arc.as_ref(), v.y.atan2(v.x).to_degrees());
                spec.angle = normalize_deg(deg);
                spec.offset = len - r;
            }
        }
        DimPart::Rotate {
            center,
            from,
            angle,
        } => {
            let (a, b) = (from - center, from + dm - center);
            if a.length() > 1e-9 && b.length() > 1e-9 {
                let turn = b.y.atan2(b.x) - a.y.atan2(a.x);
                spec.angle = normalize_deg(angle + turn.to_degrees());
            }
        }
        DimPart::Radial { dir, radius, sign } => {
            spec.offset = sign * (radius + dm.dot(dir)).max(MIN_ANGLE_RADIUS_MM);
        }
    }
}

fn polylines(sheet: &SheetModel, k: f64) -> Vec<Poly> {
    let mut out = Vec::new();
    for v in &sheet.views {
        for l in &v.lines {
            let points = flatten(&l.curve, k);
            if points.len() >= 2 {
                out.push(Poly {
                    view: v.id,
                    vis: l.vis,
                    origin: l.origin.clone(),
                    points,
                });
            }
        }
    }
    out
}

/// Ansicht unter `pos`: zuerst die nächste Linie im Abstand bis [`PICK_PX`], sonst die
/// zuletzt gezeichnete Ansicht, in deren Hüllbox der Punkt liegt.
fn hit_test(
    placed: &[Placed],
    polys: &[Poly],
    to_screen: &dyn Fn(DVec2) -> Pos2,
    pos: Pos2,
) -> Option<ViewId> {
    let mut best: Option<(f32, ViewId)> = None;
    for p in polys {
        let pts: Vec<Pos2> = p.points.iter().map(|q| to_screen(*q)).collect();
        for w in pts.windows(2) {
            let d = egui::emath::Rect::from_two_pos(w[0], w[1]).expand(PICK_PX);
            if !d.contains(pos) {
                continue;
            }
            let dist = screen_seg_dist(pos, w[0], w[1]);
            if dist <= PICK_PX && best.is_none_or(|(b, _)| dist < b) {
                best = Some((dist, p.view));
            }
        }
    }
    if let Some((_, v)) = best {
        return Some(v);
    }
    placed
        .iter()
        .rev()
        .find(|v| Rect::from_two_pos(to_screen(v.min), to_screen(v.max)).contains(pos))
        .map(|v| v.id)
}

/// Linie unter `pos` für das Werkzeug „Maß“: die nächste Linie im Abstand bis [`PICK_PX`],
/// sonst die Kreiskante, deren Mitte bis [`END_PX`] entfernt ist. Liefert Ansicht und Herkunft.
fn tool_hit(
    sheet: &SheetModel,
    polys: &[Poly],
    to_screen: &dyn Fn(DVec2) -> Pos2,
    pos: Pos2,
) -> Option<(ViewId, StrokeOrigin)> {
    let mut best: Option<(f32, &Poly)> = None;
    for p in polys {
        let pts: Vec<Pos2> = p.points.iter().map(|q| to_screen(*q)).collect();
        for w in pts.windows(2) {
            if !Rect::from_two_pos(w[0], w[1]).expand(PICK_PX).contains(pos) {
                continue;
            }
            let dist = screen_seg_dist(pos, w[0], w[1]);
            if dist <= PICK_PX && best.is_none_or(|(b, _)| dist < b) {
                best = Some((dist, p));
            }
        }
    }
    if let Some((_, p)) = best {
        return Some((p.view, p.origin.clone()));
    }
    let mut center: Option<(f32, ViewId, &StrokeOrigin)> = None;
    for v in &sheet.views {
        for l in &v.lines {
            if let (Out2d::Arc { c, .. }, StrokeOrigin::Edge { .. }) = (&l.curve, &l.origin) {
                let d = (to_screen(*c) - pos).length();
                if d <= END_PX as f32 && center.is_none_or(|(b, _, _)| d < b) {
                    center = Some((d, v.id, &l.origin));
                }
            }
        }
    }
    center.map(|(_, v, o)| (v, o.clone()))
}

pub(crate) fn screen_seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p - (a + ab * t)).length()
}

/// Tests der Malregeln für Schnittmarken und des Linienvergleichs (private Hilfen).
#[cfg(test)]
mod section_paint_tests {
    use super::*;
    use planora_drawing::annotate::{DIM_LINE_W, SECTION_ARROW_HALF_W, SECTION_ARROW_LEN};
    use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewKind};

    /// Pixel je Blatt-mm in diesen Tests.
    const K: f64 = 10.0;

    /// Malt `mark` (1 mm = `K` Pixel) und liefert die flachen Formen.
    fn paint(mark: &Mark, sheet_scale: f64) -> Vec<Shape> {
        fn flat(shape: &Shape, out: &mut Vec<Shape>) {
            match shape {
                Shape::Vec(v) => v.iter().for_each(|s| flat(s, out)),
                s => out.push(s.clone()),
            }
        }
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 800.0))),
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| {
            let painter = ui.painter().clone();
            let to_screen = |p: DVec2| pos2((p.x * K) as f32, (p.y * K) as f32);
            paint_mark(&painter, mark, theme::SHEET_INK, &to_screen, K, sheet_scale);
        });
        let mut shapes = Vec::new();
        for c in &out.shapes {
            flat(&c.shape, &mut shapes);
        }
        out.drop_without_applying_deltas();
        shapes
    }

    fn line(kind: MarkKind) -> Mark {
        Mark::Line {
            a: DVec2::new(5.0, 5.0),
            b: DVec2::new(60.0, 5.0),
            kind,
        }
    }

    fn segment_widths(shapes: &[Shape]) -> Vec<f32> {
        shapes
            .iter()
            .filter_map(|s| match s {
                Shape::LineSegment { stroke, .. } => Some(stroke.width),
                _ => None,
            })
            .collect()
    }

    fn path_widths(shapes: &[Shape]) -> Vec<f32> {
        shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Path(p) => Some(p.stroke.width),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn end_pieces_are_drawn_half_a_millimetre_wide_whatever_the_sheet() {
        for f in [1.0, 2.0, 0.5] {
            let shapes = paint(&line(MarkKind::SectionLine(SectionPen::End)), f);
            assert_eq!(segment_widths(&shapes), vec![(SECTION_END_W * K) as f32]);
            assert!(path_widths(&shapes).is_empty());
        }
        // Dicker als die dünnen Linien.
        const { assert!(SECTION_END_W > DIM_LINE_W) };
    }

    #[test]
    fn the_chain_is_dashed_and_thin() {
        let shapes = paint(&line(MarkKind::SectionLine(SectionPen::Chain)), 1.0);
        assert!(segment_widths(&shapes).is_empty(), "keine ganze Linie");
        let widths = path_widths(&shapes);
        assert!(widths.len() >= 4, "Strichpunkte: {widths:?}");
        assert!(widths.iter().all(|w| *w == (DIM_LINE_W * K) as f32));
    }

    #[test]
    fn hatch_lines_are_whole_thin_lines() {
        let shapes = paint(&line(MarkKind::Hatch), 1.0);
        assert_eq!(segment_widths(&shapes), vec![(DIM_LINE_W * K) as f32]);
        assert!(path_widths(&shapes).is_empty());
    }

    /// Länge des Pfeils (Abstand der Grundlinie von der Spitze) in Pixeln und halbe Breite.
    fn arrow_size_px(shapes: &[Shape], tip: DVec2) -> (f32, f32) {
        let pts = shapes
            .iter()
            .find_map(|s| match s {
                Shape::Path(p) => Some(p.points.clone()),
                _ => None,
            })
            .expect("Pfeil");
        assert_eq!(pts.len(), 3, "{pts:?}");
        let tip = pos2((tip.x * K) as f32, (tip.y * K) as f32);
        let base = pts[1].lerp(pts[2], 0.5);
        ((base - tip).length(), (pts[1] - pts[2]).length() / 2.0)
    }

    #[test]
    fn section_arrows_grow_with_the_sheet_but_dimension_arrows_do_not() {
        let tip = DVec2::new(20.0, 20.0);
        let arrow = |kind| Mark::Arrow {
            tip,
            dir: DVec2::new(0.0, 1.0),
            kind,
        };
        for f in [1.0, 2.0] {
            let (len, half) = arrow_size_px(
                &paint(&arrow(MarkKind::SectionLine(SectionPen::Chain)), f),
                tip,
            );
            let want = (SECTION_ARROW_LEN * f * K) as f32;
            assert!((len - want).abs() < 0.01, "{len} statt {want}");
            let want = (SECTION_ARROW_HALF_W * f * K) as f32;
            assert!((half - want).abs() < 0.01, "{half} statt {want}");
        }
        let (len, half) = arrow_size_px(&paint(&arrow(MarkKind::Dimension), 2.0), tip);
        assert!((len - (ARROW_LEN * K) as f32).abs() < 0.01, "{len}");
        assert!((half - (ARROW_HALF_W * K) as f32).abs() < 0.01, "{half}");
    }

    fn drawing_with_section() -> Drawing {
        let mut d = Drawing::new(
            DrawingId(1),
            "Blatt".into(),
            Sheet {
                paper: Paper::A3,
                landscape: true,
            },
            Projection::First,
        );
        let front = d.add_view(
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 100.0, y: 200.0 },
            None,
        );
        d.add_section(front, ViewKind::Top, 20.0, 80.0).unwrap();
        d
    }

    fn section_mut(d: &mut Drawing) -> &mut planora_drawing::SectionSpec {
        d.views
            .iter_mut()
            .find_map(|v| v.section.as_mut())
            .expect("Schnitt")
    }

    #[test]
    fn only_the_plane_of_a_section_changes_its_lines() {
        let base = drawing_with_section();
        assert!(same_lines(&base, &base.clone()));
        // Schraffur, Abstand und Buchstabe ändern keine Kantenlinien.
        let mut e = base.clone();
        section_mut(&mut e).hatch.spacing = 5.0;
        assert!(same_lines(&base, &e), "Abstand");
        section_mut(&mut e).hatch.on = false;
        assert!(same_lines(&base, &e), "Schraffur aus");
        section_mut(&mut e).letter = 'B';
        assert!(same_lines(&base, &e), "Buchstabe");
        // Mittellinien wie bisher.
        let mut c = base.clone();
        c.views[0].centerlines = !c.views[0].centerlines;
        assert!(same_lines(&base, &c), "Mittellinien");
        // Die Ebene und alles andere ändert sie.
        let mut o = base.clone();
        section_mut(&mut o).offset = 25.0;
        assert!(!same_lines(&base, &o), "Versatz");
        let mut k = base.clone();
        k.views[1].kind = ViewKind::Bottom;
        assert!(!same_lines(&base, &k), "Art");
        let mut s = base.clone();
        s.views[0].scale = Scale { num: 1, den: 2 };
        assert!(!same_lines(&base, &s), "Maßstab");
        let mut n = base.clone();
        n.views[1].section = None;
        assert!(!same_lines(&base, &n), "kein Schnitt mehr");
    }
}
