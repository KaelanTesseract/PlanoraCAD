//! Zeichnungsbereich (Teilprojekt 11, Aufgabe 15; Maße und Mittellinien Teilprojekt 12,
//! Aufgabe 11): Werkzeugleiste, Blatt mit den Linien der Ansichten und ihren Markierungen
//! (Maße, Mittellinien), Auswahl, Verschieben der Ansichten und Ziehen der Maße, Zoom und
//! Verschieben des Blatts sowie die Hinweiszeile.
//!
//! Blattkoordinaten in Millimetern, y nach oben, Ursprung unten links. Auf dem Bildschirm liegt
//! die Blattmitte in der Mitte des Bereichs plus `pan`; `zoom = 1` heißt „Blatt eingepasst“.
//!
//! **Ziehen eines Maßes** (nur bearbeitbar; Änderung über `Document::update_dimension`, der
//! Modellstand bleibt): Maßlinie eines linearen Maßes → `offset` um die Bewegung quer zur
//! Messrichtung (positiv nach oben bzw. rechts wie im Layout); Text → `text_shift` um die
//! Bewegung (Blatt-mm); Ø/R-Führungslinie → der Knick folgt dem Zeiger, daraus `angle` (Grad)
//! und `offset` (Abstand vom Kreis); Ø-Linie durch die Mitte → `angle` dreht mit; Bogen eines
//! Winkelmaßes → Radius (`|offset|`) wächst mit der Bewegung vom Scheitel weg.

use crate::browser::ERROR_COLOR;
use crate::dimension_state::{SuggestOutcome, marks_in, notes_in, resolve_in, suggest_for};
use crate::dimension_tool::{
    END_PX, Hit, ISOMETRIC_TIP, MISSED, NO_VIEWS_TIP, READ_ONLY_TIP, TOOL, TOOL_TIP, ToolError,
    ToolMode, with_tool,
};
use crate::drawing_state::{
    ViewResult, WorkArea, default_distance, estimate_view_size, suggest_scale,
};
use crate::export_dialog::{REPAINT, shorten_for_display};
use crate::state::AppState;
use crate::{theme, viewport};
use egui::epaint::TextShape;
use egui::{
    Color32, FontId, Galley, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind,
    UiBuilder, pos2, vec2,
};
use planora_drawing::annotate::{
    ARROW_HALF_W, ARROW_LEN, CENTER_PATTERN, CENTER_W, DEFAULT_OFFSET, DIM_LINE_W, Mark, MarkKind,
    ResolvedDims, TextAnchor,
};
use planora_drawing::dimension::{DimId, DimSpec};
use planora_drawing::out2d::Out2d;
use planora_drawing::sheet::{DimMarks, SheetModel, build_sheet_with, view_transform};
use planora_drawing::stroke::StrokeOrigin;
use planora_drawing::{
    Anchor, Drawing, DrawingId, Placement, Scale, ViewId, ViewKind, ViewLines, ViewSpec,
    Visibility, align_dir, view_extent,
};
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
/// Höhe der Hinweiszeile.
const HINT_HEIGHT: f32 = 24.0;
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
/// Kleinster Radius eines Winkelmaßes beim Ziehen (Blatt-mm).
const MIN_ANGLE_RADIUS_MM: f64 = 1.0;
/// Hinweis, wenn es zu den Körpern nichts zu sagen gibt.
pub const HINT: &str = "Klicken wählt eine Ansicht oder ein Maß, Ziehen verschiebt es. Mausrad vergrößert, mittlere Maustaste oder Umschalt + Ziehen verschiebt das Blatt.";
/// Beschriftung des Schalters für die Mittellinien der gewählten Ansicht.
pub const CENTERLINES: &str = "Mittellinien";
/// Erklärung am gesperrten Schalter „Mittellinien“ einer isometrischen Ansicht.
pub const NO_ISO_CENTERLINES: &str = "In der Isometrie gibt es keine Mittellinien; sie erscheinen nur in den geraden Ansichten (vorne, oben, seitlich).";
/// Beschriftung des Knopfs für den Maßvorschlag.
pub const SUGGEST: &str = "Maße vorschlagen";
/// Satzanfang des Tooltips eines Maßes, das seinen Bezug verloren hat.
pub const LOST_TIP: &str = "Das Maß hat seinen Bezug verloren.";
/// Meldung, wenn die Ausgangsansicht einer abgeleiteten Ansicht nicht gezeichnet werden kann.
pub const PARENT_MISSING: &str = "Die Ausgangsansicht lässt sich nicht zeichnen.";
/// Meldung ohne geöffnete Zeichnung.
const NO_DRAWING: &str = "Es ist keine Zeichnung geöffnet.";

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
    pub area: Rect,
    pub toolbar: Rect,
    pub canvas: Rect,
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

/// Namen der Ansichten wie auf dem Blatt: Art, bei mehrfach gleicher Art mit Nummer.
pub fn view_names(drawing: &Drawing) -> BTreeMap<ViewId, String> {
    let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for v in &drawing.views {
        let i = ViewKind::ALL.iter().position(|k| *k == v.kind).unwrap_or(0);
        let n = counts.entry(i).or_insert(0);
        *n += 1;
        let name = if *n == 1 {
            v.kind.label().to_string()
        } else {
            format!("{} {}", v.kind.label(), n)
        };
        out.insert(v.id, name);
    }
    out
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
        .map(|v| (v.id, state.drawing_ui.result(v.kind, model)))
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
                    None => DrawnState::Failed(PARENT_MISSING.into()),
                };
                (placeholder(view_extent(lines.bbox, v.scale)), st)
            }
            (ViewResult::Pending { fraction }, _) => (
                placeholder(estimated(state, v.kind, v.scale)),
                DrawnState::Pending(*fraction),
            ),
            (ViewResult::Failed(m), _) => (
                placeholder(estimated(state, v.kind, v.scale)),
                DrawnState::Failed(m.clone()),
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
    drawing: Drawing,
    model: u64,
    ptrs: Vec<usize>,
    /// Aufgelöste Maße (festgehalten, damit ihre Adresse eindeutig bleibt).
    dims: Arc<ResolvedDims>,
    sheet: Arc<SheetModel>,
    /// Stand der Linien: bleibt gleich, solange sich nur Maße oder Mittellinien ändern (die
    /// Linienzüge werden dann nicht neu zerlegt).
    lines_gen: u64,
}

/// Fortlaufende Nummer der Linienstände (eindeutig im Programm).
static LINES_GEN: AtomicU64 = AtomicU64::new(1);

/// Haben zwei Stände der Zeichnung dieselben Linien (Blatt, Projektion, Ansichten bis auf den
/// Schalter der Mittellinien; Maße zählen nicht)?
fn same_lines(a: &Drawing, b: &Drawing) -> bool {
    a.sheet == b.sheet
        && a.projection == b.projection
        && a.views.len() == b.views.len()
        && a.views.iter().zip(&b.views).all(|(x, y)| {
            ViewSpec {
                centerlines: y.centerlines,
                ..x.clone()
            } == *y
        })
}

/// Zuletzt im Zeichnungsbereich gezeichnetes Blattmodell (mit Mittellinien und Maßen).
pub fn last_sheet(ctx: &egui::Context) -> Option<Arc<SheetModel>> {
    ctx.data(|d| d.get_temp::<SheetCache>(sheet_cache_id()))
        .map(|c| c.sheet)
}

/// Blattmodell der Zeichnung mit Mittellinien und den Maßen `dims` und der Stand seiner Linien;
/// aus dem Zwischenspeicher, solange sich Zeichnung, Ergebnisse und Maße nicht geändert haben.
fn sheet_model(
    ctx: &egui::Context,
    state: &AppState,
    drawing: &Drawing,
    model: u64,
    dims: &Arc<ResolvedDims>,
) -> (Arc<SheetModel>, u64) {
    let owned = state.drawing_ui.results_for(drawing, model);
    let ptrs: Vec<usize> = owned.values().map(|a| Arc::as_ptr(a) as usize).collect();
    let cached = ctx.data(|d| d.get_temp::<SheetCache>(sheet_cache_id()));
    let same_base = cached
        .as_ref()
        .filter(|c| c.model == model && c.ptrs == ptrs);
    if let Some(c) = same_base
        && Arc::ptr_eq(&c.dims, dims)
        && c.drawing == *drawing
    {
        return (Arc::clone(&c.sheet), c.lines_gen);
    }
    let lines_gen = match same_base {
        Some(c) if same_lines(&c.drawing, drawing) => c.lines_gen,
        _ => LINES_GEN.fetch_add(1, Ordering::Relaxed),
    };
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, v)| (*k, v.as_ref())).collect();
    let sheet = Arc::new(build_sheet_with(drawing, &refs, dims));
    ctx.data_mut(|d| {
        d.insert_temp(
            sheet_cache_id(),
            SheetCache {
                drawing: drawing.clone(),
                model,
                ptrs,
                dims: Arc::clone(dims),
                sheet: Arc::clone(&sheet),
                lines_gen,
            },
        )
    });
    (sheet, lines_gen)
}

/// Gezogener Teil eines Maßes mit den Werten zu Beginn des Ziehens (Blatt-mm).
#[derive(Clone, Copy, Debug)]
enum DimPart {
    /// Maßlinie eines linearen Maßes: `offset` wächst um die Bewegung entlang der Querachse `w`.
    Offset { w: DVec2, offset: f64 },
    /// Text: `text_shift` wächst um die Bewegung.
    Text { shift: (f64, f64) },
    /// Ø/R-Führungslinie: der Knick folgt der Bewegung; Kreis mit Mitte und Radius auf dem Blatt.
    Leader { center: DVec2, r: f64, knee: DVec2 },
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
}

/// Aktion aus der Werkzeugleiste.
enum Action {
    ToModel,
    Add(ViewKind),
    Fit,
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
}

/// Kann das Werkzeug „Maß“ benutzt werden? Sonst die Erklärung (alte Version, keine Ansicht,
/// nur Isometrie).
pub fn tool_blocked(drawing: &Drawing, editable: bool) -> Option<&'static str> {
    if !editable {
        Some(READ_ONLY_TIP)
    } else if drawing.views.is_empty() {
        Some(NO_VIEWS_TIP)
    } else if !drawing.views.iter().any(|v| v.kind.is_normal()) {
        Some(ISOMETRIC_TIP)
    } else {
        None
    }
}

/// Zeichnungsbereich (bearbeitbar).
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    show_with(ui, state, true);
}

/// Zeichnungsbereich; `editable = false` (alte Version) sperrt Hinzufügen, Löschen, Maßstab
/// und Verschieben der Ansichten. Ruft in jedem Bild `tick` und fordert ein Neuzeichnen an,
/// solange eine Ansicht berechnet wird.
pub fn show_with(ui: &mut egui::Ui, state: &mut AppState, editable: bool) {
    state.drawing_ui.tick(&state.bodies, &state.document, false);
    let WorkArea::Drawing(id) = state.drawing_ui.area else {
        return;
    };
    let Some(drawing) = state.document.drawing(id).cloned() else {
        return;
    };
    // Gesperrtes Werkzeug ist aus (alte Version, Zeichnung nur mit Isometrie).
    if tool_blocked(&drawing, editable).is_some() && state.dimension_tool.is_active() {
        state.dimension_tool.set_mode(ToolMode::Off);
    }
    let area = ui.available_rect_before_wrap();
    let (toolbar, action) = toolbar(ui, state, &drawing, editable);
    match action {
        Some(Action::ToModel) => {
            state.drawing_ui.area = WorkArea::Model;
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
        Some(Action::Zoom(f)) => {
            let z = (state.drawing_ui.zoom * f).clamp(MIN_ZOOM, MAX_ZOOM);
            let f = z / state.drawing_ui.zoom;
            state.drawing_ui.zoom = z;
            state.drawing_ui.pan *= f;
        }
        Some(Action::Scale(s)) => set_selected(state, id, |v| v.scale = s),
        Some(Action::Hidden(h)) => set_selected(state, id, |v| v.show_hidden = h),
        Some(Action::Centerlines(c)) => set_selected(state, id, |v| {
            if v.kind.is_normal() {
                v.centerlines = c;
            }
        }),
        Some(Action::Suggest) => suggest_dimensions(state, id),
        Some(Action::Delete) => delete_selected(state),
        Some(Action::ToolToggle) => state.dimension_tool.toggle(),
        Some(Action::Tool(m)) => state.dimension_tool.set_mode(m),
        None => {}
    }
    let hint_top = (area.bottom() - HINT_HEIGHT).max(toolbar.bottom());
    let hint = Rect::from_min_max(pos2(area.left(), hint_top), area.right_bottom());
    let canvas = Rect::from_min_max(
        pos2(area.left(), toolbar.bottom()),
        pos2(area.right(), hint_top),
    );
    let response = ui.allocate_rect(canvas, Sense::click_and_drag());
    let views = canvas_ui(ui, state, id, canvas, &response, editable);

    let note = state.drawing_ui.note(&state.bodies);
    let dims = resolve_in(state, id);
    let notes = notes_in(state, id, &dims);
    // Die Meldung des Vorschlags gilt nur, bis sich das Dokument oder die Auswahl ändert.
    if state.drawing_ui.dim_message_key != Some(message_key(state)) {
        state.drawing_ui.dim_message = None;
        state.drawing_ui.dim_message_key = None;
    }
    // Eine Ablehnung des Werkzeugs „Maß“ geht der Meldung vor (rot); bei eingeschaltetem
    // Werkzeug steht an Stelle des Hinweises der nächste Schritt.
    let rejected = state.dimension_tool.message().map(str::to_owned);
    let message = rejected
        .clone()
        .or_else(|| state.dimension_tool.notice().map(str::to_owned))
        .or_else(|| state.drawing_ui.dim_message.clone());
    let hint_text = state
        .dimension_tool
        .hint()
        .or(note)
        .unwrap_or_else(|| HINT.to_string());
    ui.scope_builder(UiBuilder::new().max_rect(hint), |ui| {
        ui.set_min_size(hint.size());
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            // Zuerst die Hinweise zu den Maßen (die verlorenen rot), dann die Meldung (höchstens
            // die halbe Zeile), zuletzt der bisherige Hinweis; zu lange Texte enden mit „…“
            // (ganzer Text im Tooltip), nichts wird aus der Zeile geschoben.
            for (i, t) in notes.texts.iter().enumerate() {
                if i == 0 && notes.lost > 0 {
                    ui.colored_label(ERROR_COLOR, t);
                } else {
                    ui.label(t);
                }
            }
            if !notes.texts.is_empty() {
                ui.separator();
            }
            if let Some(m) = &message {
                let w = (0.5 * ui.available_width()).max(40.0);
                ui.scope(|ui| {
                    ui.set_max_width(w);
                    let text = if rejected.is_some() {
                        egui::RichText::new(m.as_str()).color(ERROR_COLOR)
                    } else {
                        egui::RichText::new(m.as_str())
                    };
                    ui.add(egui::Label::new(text).truncate());
                });
                ui.separator();
            }
            ui.add(egui::Label::new(egui::RichText::new(hint_text).weak()).truncate());
        });
    });
    if let Some((layout, pending)) = views {
        if pending {
            ui.ctx().request_repaint_after(REPAINT);
        }
        let layout = DrawingLayout {
            area,
            toolbar,
            hint,
            ..layout
        };
        ui.ctx().data_mut(|d| d.insert_temp(layout_id(), layout));
    }
}

/// Werkzeugleiste über die ganze Breite; liefert ihr Rechteck und die gewählte Aktion.
fn toolbar(
    ui: &mut egui::Ui,
    state: &AppState,
    drawing: &Drawing,
    editable: bool,
) -> (Rect, Option<Action>) {
    let mut action = None;
    let selected = state
        .drawing_ui
        .selected_view
        .and_then(|v| drawing.view(v))
        .cloned();
    let names = view_names(drawing);
    let response = egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 6))
        .fill(ui.visuals().panel_fill)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                if ui.selectable_label(false, "3D-Modell").clicked() {
                    action = Some(Action::ToModel);
                }
                let _ = ui.selectable_label(
                    true,
                    format!("Zeichnung: {}", shorten_for_display(&drawing.name)),
                );
                ui.separator();
                ui.add_enabled_ui(editable, |ui| {
                    ui.menu_button("Ansicht hinzufügen", |ui| {
                        for k in ViewKind::ALL {
                            if ui.button(k.label()).clicked() {
                                action = Some(Action::Add(k));
                                ui.close();
                            }
                        }
                    });
                });
                if ui.button("Blatt einpassen").clicked() {
                    action = Some(Action::Fit);
                }
                if ui.button("−").on_hover_text("Verkleinern").clicked() {
                    action = Some(Action::Zoom(1.0 / ZOOM_STEP));
                }
                if ui.button("+").on_hover_text("Vergrößern").clicked() {
                    action = Some(Action::Zoom(ZOOM_STEP));
                }
                // Zweite Zeile: Maße und die Einstellungen der gewählten Ansicht. Die Leiste hat
                // so immer zwei Zeilen; das Wählen einer Ansicht ändert die Höhe der Blattfläche
                // nicht (sonst verschöbe sich das Blatt mitten im Ziehen).
                ui.end_row();
                ui.add_enabled_ui(editable, |ui| {
                    let tip = if selected.is_some() {
                        "Setzt die wichtigsten Maße in die gewählte Ansicht."
                    } else {
                        "Setzt die wichtigsten Maße in alle Ansichten."
                    };
                    if ui.button(SUGGEST).on_hover_text(tip).clicked() {
                        action = Some(Action::Suggest);
                    }
                });
                // Werkzeug „Maß“: Schalter und Untermodus (immer da, damit die Leiste gleich
                // breit bleibt; gesperrt mit Erklärung).
                let blocked = tool_blocked(drawing, editable);
                ui.add_enabled_ui(blocked.is_none(), |ui| {
                    let tool = &state.dimension_tool;
                    let why = blocked.unwrap_or(TOOL_TIP);
                    if ui
                        .selectable_label(tool.is_active(), TOOL)
                        .on_hover_text(TOOL_TIP)
                        .on_disabled_hover_text(why)
                        .clicked()
                    {
                        action = Some(Action::ToolToggle);
                    }
                    egui::ComboBox::from_id_salt("planora-dimension-tool")
                        .selected_text(tool.choice().label())
                        .show_ui(ui, |ui| {
                            for m in ToolMode::CHOICES {
                                if ui.selectable_label(tool.choice() == m, m.label()).clicked() {
                                    action = Some(Action::Tool(m));
                                }
                            }
                        })
                        .response
                        .on_disabled_hover_text(why);
                });
                if let Some(v) = &selected {
                    ui.separator();
                    ui.strong(names.get(&v.id).cloned().unwrap_or_default());
                    ui.add_enabled_ui(editable, |ui| {
                        ui.label("Maßstab");
                        egui::ComboBox::from_id_salt("planora-drawing-scale")
                            .selected_text(v.scale.label())
                            .show_ui(ui, |ui| {
                                for s in Scale::SERIES {
                                    if ui.selectable_label(s == v.scale, s.label()).clicked() {
                                        action = Some(Action::Scale(s));
                                    }
                                }
                            });
                        let mut hidden = v.show_hidden;
                        if ui
                            .checkbox(&mut hidden, "Verdeckte Kanten zeigen")
                            .changed()
                        {
                            action = Some(Action::Hidden(hidden));
                        }
                        // Mittellinien gibt es nur in Normansichten (Blick entlang einer
                        // Weltachse); in der Isometrie bleibt der Schalter gesperrt.
                        let mut centerlines = v.centerlines;
                        if ui
                            .add_enabled(
                                v.kind.is_normal(),
                                egui::Checkbox::new(&mut centerlines, CENTERLINES),
                            )
                            .on_disabled_hover_text(NO_ISO_CENTERLINES)
                            .changed()
                        {
                            action = Some(Action::Centerlines(centerlines));
                        }
                        if ui.button("Ansicht löschen").clicked() {
                            action = Some(Action::Delete);
                        }
                    });
                }
            });
        });
    (response.response.rect, action)
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
    let failed: Vec<(String, String)> = state
        .document
        .drawing(id)
        .map(|d| {
            let names = view_names(d);
            d.views
                .iter()
                .filter(|v| v.kind.is_normal() && only.is_none_or(|o| o == v.id))
                .filter_map(|v| match state.drawing_ui.result(v.kind, model) {
                    ViewResult::Failed(m) => {
                        Some((names.get(&v.id).cloned().unwrap_or_default(), m))
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let outcome = suggest_for(state, id, only);
    let mut text = outcome.message();
    if !failed.is_empty() {
        let quoted: Vec<String> = failed.iter().map(|(n, _)| format!("„{n}“")).collect();
        match &outcome {
            // Keine Ansicht ließ sich berechnen: der Grund ist der Fehlertext einer Ansicht.
            SuggestOutcome::Unavailable(t) if failed.iter().any(|(_, m)| m == t) => {
                text = if failed.len() == 1 {
                    format!("Die Ansicht {} ließ sich nicht berechnen. {t}", quoted[0])
                } else {
                    format!(
                        "Die Ansichten {} ließen sich nicht berechnen. {t}",
                        quoted.join(", ")
                    )
                };
            }
            // Anderer Grund (etwa: es wird noch gerechnet).
            SuggestOutcome::Unavailable(_) => {}
            _ => {
                text.push(' ');
                text.push_str(&if failed.len() == 1 {
                    format!(
                        "Die Ansicht {} ließ sich nicht berechnen und bekam keine Maße.",
                        quoted[0]
                    )
                } else {
                    format!(
                        "Die Ansichten {} ließen sich nicht berechnen und bekamen keine Maße.",
                        quoted.join(", ")
                    )
                });
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
        state.dimension_tool.set_message(Some(MISSED.into()));
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
            let reason = state.dimension_tool.message().map(str::to_owned);
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
            state.drawing_ui.dim_message = Some(match ids.len() {
                1 => "1 Maß wurde gesetzt.".into(),
                n => format!("{n} Maße wurden gesetzt."),
            });
            state.drawing_ui.dim_message_key = Some(message_key(state));
            Ok(())
        }
        // Die Ablehnung steht im Werkzeug (Hinweiszeile).
        Err(e) => Err(e),
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
/// sonst frei (leere Zeichnung: Blattmitte, sonst links oben). Isometrisch: frei rechts oben
/// (0,75 · Breite, 0,75 · Höhe) mit dem Maßstab der Grundansicht.
pub fn add_view(state: &mut AppState, kind: ViewKind) -> Result<ViewId, String> {
    let WorkArea::Drawing(id) = state.drawing_ui.area else {
        return Err(NO_DRAWING.into());
    };
    let d = state.document.drawing(id).ok_or(NO_DRAWING)?.clone();
    let (w, h) = d.sheet.size_mm();
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
        suggest_scale(&state.bodies, d.sheet, d.projection, k)
    });
    let (placement, parent, scale) = if !kind.is_normal() {
        (
            Placement::Free {
                x: 0.75 * w,
                y: 0.75 * h,
            },
            None,
            base_scale,
        )
    } else {
        let selected = state
            .drawing_ui
            .selected_view
            .and_then(|s| d.view(s))
            .filter(|v| v.kind.is_normal());
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
            None if d.views.is_empty() => (
                Placement::Free {
                    x: 0.5 * w,
                    y: 0.5 * h,
                },
                None,
                base_scale,
            ),
            None => (
                Placement::Free {
                    x: 0.25 * w,
                    y: 0.75 * h,
                },
                None,
                base_scale,
            ),
        }
    };
    state
        .document
        .add_view(id, kind, scale, placement, parent)
        .map_err(|e| e.to_string())
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

/// Blattfläche: Eingaben und Zeichnen. Liefert die Aufteilung (ohne Werkzeugleiste und
/// Hinweis) und ob noch etwas berechnet wird.
fn canvas_ui(
    ui: &mut egui::Ui,
    state: &mut AppState,
    id: DrawingId,
    canvas: Rect,
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
    let (sheet, lines_gen) = sheet_model(&ctx, state, &drawing, model, &dims);
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
    // Maße liegen über den Linien und werden zuerst getroffen.
    let hit_dim = |pos: Pos2| dim_hit(&ctx, &dim_marks, |_| true, &to_screen0, k0, pos);
    // Mit eingeschaltetem Werkzeug „Maß“ wählen Klicks Bezüge statt Ansichten und Maße, und
    // Ziehen verschiebt nichts (nur das Blatt).
    let tool_on = editable && state.dimension_tool.is_active();

    // Zoom um den Zeiger (Mausrad, Zwei-Finger-Geste).
    if response.hovered()
        && let Some(p) = response.hover_pos()
    {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta().y, i.zoom_delta()));
        let f = pinch * (scroll * WHEEL_RATE).exp();
        if f != 1.0 && f.is_finite() {
            zoom_about(state, canvas, p, f);
        }
    }

    let shift = ui.input(|i| i.modifiers.shift);
    if response.drag_started() {
        let origin = ui
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos());
        if let Some(origin) = origin {
            let drag = if response.dragged_by(PointerButton::Middle)
                || (response.dragged_by(PointerButton::Primary) && shift)
            {
                Some(Drag::Pan {
                    origin,
                    start: state.drawing_ui.pan,
                })
            } else if response.dragged_by(PointerButton::Primary) && !tool_on {
                if let Some((i, on_text)) = hit_dim(origin) {
                    let dm = &dim_marks[i];
                    state.drawing_ui.selected_dim = Some(dm.id);
                    state.drawing_ui.selected_view = None;
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
                    let mut next = cur.clone();
                    apply_dim_drag(&mut next, part, dm);
                    if next != *cur {
                        let r = state.document.update_dimension(id, dim, |s| *s = next);
                        if let Err(e) = r {
                            state.message = Some(e.to_string());
                        }
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
        } else if let Some((i, _)) = hit_dim(pos) {
            let dim = dim_marks[i].id;
            state.drawing_ui.selected_dim = Some(dim);
            state.drawing_ui.selected_view = None;
            // Doppelklick auf ein Maß öffnet „Maß bearbeiten“ (in einer alten Version nur zum Ansehen).
            if response.double_clicked() {
                crate::dimension_dialog::open(state, id, dim, !editable);
            }
        } else {
            state.drawing_ui.selected_view = hit(pos);
            state.drawing_ui.selected_dim = None;
        }
    }
    if !viewport::typing(&ctx) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        if tool_on {
            tool_escape(state, id);
        } else {
            state.drawing_ui.selected_view = None;
            state.drawing_ui.selected_dim = None;
        }
    }
    // Tooltip mit dem Grund über einem Maß, das seinen Bezug verloren hat.
    if response.hovered()
        && !response.dragged()
        && let Some(p) = response.hover_pos()
    {
        let lost = |dm: &DimMarks| dm.marks.iter().any(|m| mark_kind(m) == MarkKind::Lost);
        if let Some((i, _)) = dim_hit(&ctx, &dim_marks, lost, &to_screen0, k0, p) {
            let reason = dims
                .by_view
                .values()
                .flatten()
                .find(|(s, _)| s.id == dim_marks[i].id)
                .and_then(|(_, r)| r.as_ref().err())
                .map(|e| e.to_string());
            let text = match reason {
                Some(r) => format!("{LOST_TIP} {r}"),
                None => LOST_TIP.to_string(),
            };
            response.clone().on_hover_text_at_pointer(text);
        }
    }

    // --- Zeichnen mit dem neuen Stand ---
    let drawing = state.document.drawing(id)?.clone();
    let (zoom, pan) = (state.drawing_ui.zoom, state.drawing_ui.pan);
    let k = f64::from(fit_px_per_mm(canvas, sheet_mm) * zoom);
    let dims = resolve_in(state, id);
    let (sheet, lines_gen) = sheet_model(&ctx, state, &drawing, model, &dims);
    let dim_marks = marks_in(state, id, &dims);
    let placed = place_views(state, &drawing, &sheet, model);
    let polys = polylines_cached(&ctx, &sheet, lines_gen, k);
    let to_screen = |p: DVec2| sheet_to_screen(canvas, sheet_mm, zoom, pan, p);
    let painter = ui.painter_at(canvas);
    painter.rect_filled(canvas, 0.0, theme::drawing_surround(state.dark));
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
    let (f0, f1) = sheet.frame;
    painter.rect_stroke(
        Rect::from_two_pos(to_screen(f0), to_screen(f1)),
        0.0,
        Stroke::new((VISIBLE_MM * k).max(1.0) as f32, theme::SHEET_FRAME),
        StrokeKind::Middle,
    );

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

    // Markierungen: Mittellinien und Maße schwarz, verlorene Maße rot, das gewählte Maß in der
    // Akzentfarbe (darüber; seine Markierungen fallen in der normalen Farbe weg).
    let selected: &[Mark] = state
        .drawing_ui
        .selected_dim
        .and_then(|s| dim_marks.iter().find(|dm| dm.id == s))
        .map_or(&[], |dm| dm.marks.as_slice());
    let mut marks_drawn = 0;
    for v in &sheet.views {
        for m in &v.marks {
            if selected.contains(m) {
                continue;
            }
            let color = match mark_kind(m) {
                MarkKind::Lost => theme::SHEET_LOST,
                MarkKind::Dimension | MarkKind::CenterLine => theme::SHEET_DIM,
            };
            paint_mark(&painter, m, color, &to_screen, k);
            marks_drawn += 1;
        }
    }
    for m in selected {
        paint_mark(&painter, m, theme::SHEET_SELECTED, &to_screen, k);
        marks_drawn += 1;
    }
    // Vorschau des Werkzeugs „Maß“ am Zeiger (dieselben Markierungen wie das gesetzte Maß).
    let mut preview_drawn = 0;
    if tool_on && let Some(hp) = response.hover_pos() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        let p = screen_to_sheet(canvas, sheet_mm, zoom, pan, hp);
        let preview = with_tool(state, |t, st| t.hover(st, id, p)).unwrap_or_default();
        for m in &preview {
            paint_mark(&painter, m, theme::SHEET_SELECTED, &to_screen, k);
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
                let painter = painter.with_clip_rect(rect.intersect(canvas));
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
                painter.rect_filled(outer, 3.0, Color32::from_gray(225));
                painter.rect_filled(fill, 3.0, accent);
                bar = Some((outer, fill));
            }
            DrawnState::Failed(m) => {
                painter.rect_stroke(rect, 2.0, Stroke::new(1.0, ERROR_COLOR), StrokeKind::Inside);
                // Der Text bleibt im Rahmen, auch wenn er zu klein ist.
                let painter = painter.with_clip_rect(rect.intersect(canvas));
                let inner = rect.shrink(6.0);
                let galley = painter.layout(
                    format!("{}: {m}", v.name),
                    font.clone(),
                    ERROR_COLOR,
                    inner.width().max(1.0),
                );
                painter.galley(inner.min, galley, ERROR_COLOR);
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
        area: canvas,
        toolbar: canvas,
        canvas,
        hint: canvas,
        sheet_mm,
        zoom,
        pan,
        px_per_mm: k as f32,
        lines_drawn: sheet.views.iter().map(|v| v.lines.len()).sum(),
        marks_drawn,
        preview_drawn,
        views: drawn,
    };
    Some((layout, pending))
}

/// Text des Platzhalters einer Ansicht, die berechnet wird.
pub fn pending_text(fraction: f32) -> String {
    format!(
        "wird berechnet … {} %",
        (fraction.clamp(0.0, 1.0) * 100.0).round() as i32
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
fn arrow_corners(tip: DVec2, dir: DVec2) -> [DVec2; 3] {
    let base = tip + dir * ARROW_LEN;
    let side = dir.perp() * ARROW_HALF_W;
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

/// Zeichnet eine Markierung: Linien `max(1 px, Breite·k)` (Mittellinien strichpunktiert nach
/// [`CENTER_PATTERN`]), Pfeile gefüllt, Bögen als Polylinie, Texte als `egui`-Text.
fn paint_mark(
    painter: &egui::Painter,
    m: &Mark,
    color: Color32,
    to_screen: &dyn Fn(DVec2) -> Pos2,
    k: f64,
) {
    match m {
        Mark::Line { a, b, kind } => {
            let w = if *kind == MarkKind::CenterLine {
                CENTER_W
            } else {
                DIM_LINE_W
            };
            let stroke = Stroke::new((w * k).max(1.0) as f32, color);
            if *kind == MarkKind::CenterLine {
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
        Mark::Arrow { tip, dir, .. } => {
            let pts = arrow_corners(*tip, *dir).map(to_screen).to_vec();
            painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Mark::Arc {
            center,
            r,
            start,
            end,
            ..
        } => {
            let stroke = Stroke::new((DIM_LINE_W * k).max(1.0) as f32, color);
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
                    polygon_dist(pos, &arrow_corners(*tip, *dir).map(to_screen)),
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
        Anchor::Round { center, radius } => {
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
            let th = spec.angle.to_radians();
            let u = DVec2::new(th.cos(), th.sin());
            Some(DimPart::Leader {
                center: c,
                r,
                knee: c + u * (r + spec.offset),
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
        DimPart::Leader { center, r, knee } => {
            let v = knee + dm - center;
            let len = v.length();
            if len.is_finite() && len > 1e-9 {
                spec.angle = normalize_deg(v.y.atan2(v.x).to_degrees());
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

fn screen_seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p - (a + ab * t)).length()
}
