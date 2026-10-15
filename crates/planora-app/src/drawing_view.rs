//! Zeichnungsbereich (Teilprojekt 11, Aufgabe 15): Werkzeugleiste, Blatt mit den Linien der
//! Ansichten, Auswahl, Verschieben der Ansichten, Zoom und Verschieben des Blatts sowie die
//! Hinweiszeile.
//!
//! Blattkoordinaten in Millimetern, y nach oben, Ursprung unten links. Auf dem Bildschirm liegt
//! die Blattmitte in der Mitte des Bereichs plus `pan`; `zoom = 1` heißt „Blatt eingepasst“.

use crate::browser::ERROR_COLOR;
use crate::drawing_state::{
    ViewResult, WorkArea, default_distance, estimate_view_size, suggest_scale,
};
use crate::export_dialog::{REPAINT, shorten_for_display};
use crate::state::AppState;
use crate::{theme, viewport};
use egui::{
    Color32, FontId, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, UiBuilder, pos2,
    vec2,
};
use planora_drawing::out2d::Out2d;
use planora_drawing::sheet::{SheetModel, build_sheet};
use planora_drawing::{
    Drawing, DrawingId, Placement, Scale, ViewId, ViewKind, ViewLines, Visibility, align_dir,
    view_extent,
};
use planora_math::DVec2;
use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::sync::Arc;

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
/// Hinweis, wenn es zu den Körpern nichts zu sagen gibt.
pub const HINT: &str = "Klicken wählt eine Ansicht, Ziehen verschiebt sie. Mausrad vergrößert, mittlere Maustaste oder Umschalt + Ziehen verschiebt das Blatt.";
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

/// Zerlegt eine Kurve (Blatt-mm) in eine Polylinie, die bei `px_per_mm` höchstens 1 px von
/// der Kurve abweicht. Die Stückzahl folgt aus Länge bzw. Krümmung und Zoom. Ungültige Kurven
/// liefern keine Punkte.
pub fn flatten(curve: &Out2d, px_per_mm: f64) -> Vec<DVec2> {
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
    if points.len() < 2 {
        return Vec::new();
    }
    if dash <= 0.0 || gap <= 0.0 {
        return vec![points.to_vec()];
    }
    const EPS: f64 = 1e-12;
    let period = dash + gap;
    let mut out = Vec::new();
    let mut cur = vec![points[0]];
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
            let in_dash = phase < dash;
            let boundary = if in_dash { dash } else { period };
            let step = (boundary - phase).min(len - done);
            done += step;
            phase += step;
            let p = if len - done <= EPS { b } else { a + dir * done };
            if in_dash {
                cur.push(p);
                if phase >= dash - EPS {
                    out.push(std::mem::take(&mut cur));
                }
            } else if phase >= period - EPS {
                phase = 0.0;
                cur = vec![p];
            }
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// Namen der Ansichten wie auf dem Blatt: Art, bei mehrfach gleicher Art mit Nummer.
fn view_names(drawing: &Drawing) -> BTreeMap<ViewId, String> {
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
    sheet: Arc<SheetModel>,
}

/// Blattmodell der Zeichnung; aus dem Zwischenspeicher, solange sich Zeichnung und Ergebnisse
/// nicht geändert haben.
fn sheet_model(
    ctx: &egui::Context,
    state: &AppState,
    drawing: &Drawing,
    model: u64,
) -> Arc<SheetModel> {
    let owned = state.drawing_ui.results_for(drawing, model);
    let ptrs: Vec<usize> = owned.values().map(|a| Arc::as_ptr(a) as usize).collect();
    if let Some(c) = ctx.data(|d| d.get_temp::<SheetCache>(sheet_cache_id()))
        && c.model == model
        && c.ptrs == ptrs
        && c.drawing == *drawing
    {
        return c.sheet;
    }
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, v)| (*k, v.as_ref())).collect();
    let sheet = Arc::new(build_sheet(drawing, &refs));
    ctx.data_mut(|d| {
        d.insert_temp(
            sheet_cache_id(),
            SheetCache {
                drawing: drawing.clone(),
                model,
                ptrs,
                sheet: Arc::clone(&sheet),
            },
        )
    });
    sheet
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
}

/// Aktion aus der Werkzeugleiste.
enum Action {
    ToModel,
    Add(ViewKind),
    Fit,
    Zoom(f32),
    Scale(Scale),
    Hidden(bool),
    Delete,
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
        Some(Action::Delete) => delete_selected(state),
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
    ui.scope_builder(UiBuilder::new().max_rect(hint), |ui| {
        ui.set_min_size(hint.size());
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            ui.weak(note.as_deref().unwrap_or(HINT));
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

/// Löscht die gewählte Ansicht samt allen von ihr abgeleiteten (ohne Rückfrage).
pub fn delete_selected(state: &mut AppState) {
    let (WorkArea::Drawing(id), Some(sel)) =
        (state.drawing_ui.area, state.drawing_ui.selected_view)
    else {
        return;
    };
    let r = state.document.update_drawing(id, |d| {
        d.remove_view(sel);
    });
    if let Err(e) = r {
        state.message = Some(e.to_string());
    }
    state.drawing_ui.selected_view = None;
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
struct Poly {
    view: ViewId,
    vis: Visibility,
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
    let sheet = sheet_model(&ctx, state, &drawing, model);
    let placed = place_views(state, &drawing, &sheet, model);
    let polys = polylines(&sheet, k0);
    let (zoom0, pan0) = (state.drawing_ui.zoom, state.drawing_ui.pan);
    let to_screen0 = |p: DVec2| sheet_to_screen(canvas, sheet_mm, zoom0, pan0, p);
    let hit = |pos: Pos2| hit_test(&placed, &polys, &to_screen0, pos);

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
            } else if response.dragged_by(PointerButton::Primary) {
                hit(origin).and_then(|view| {
                    state.drawing_ui.selected_view = Some(view);
                    let start = drawing.view(view)?.placement;
                    editable.then_some(Drag::View {
                        origin,
                        view,
                        start,
                    })
                })
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
        }
    }
    if response.drag_stopped() || !response.is_pointer_button_down_on() && !response.dragged() {
        ctx.data_mut(|d| d.remove::<Option<Drag>>(drag_id()));
    }
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        state.drawing_ui.selected_view = hit(pos);
    }
    if !viewport::typing(&ctx) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.drawing_ui.selected_view = None;
    }

    // --- Zeichnen mit dem neuen Stand ---
    let drawing = state.document.drawing(id)?.clone();
    let (zoom, pan) = (state.drawing_ui.zoom, state.drawing_ui.pan);
    let k = f64::from(fit_px_per_mm(canvas, sheet_mm) * zoom);
    let sheet = sheet_model(&ctx, state, &drawing, model);
    let placed = place_views(state, &drawing, &sheet, model);
    let polys = polylines(&sheet, k);
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
    for p in &polys {
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

fn polylines(sheet: &SheetModel, k: f64) -> Vec<Poly> {
    let mut out = Vec::new();
    for v in &sheet.views {
        for l in &v.lines {
            let points = flatten(&l.curve, k);
            if points.len() >= 2 {
                out.push(Poly {
                    view: v.id,
                    vis: l.vis,
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
