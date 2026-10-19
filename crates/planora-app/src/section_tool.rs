//! Werkzeug „Schnitt“ (Teilprojekt 3d, Aufgabe 9): Schnittlinie mit zwei Klicks in einer geraden
//! Ansicht, daraus eine Schnittansicht; dazu die Änderungen im Ansichtsfeld (Lage der Ebene,
//! Blickrichtung, Schraffur).
//!
//! **Ablauf:** der erste Klick liegt in der Hüllbox einer geraden Ansicht ohne Schnitt (sonst
//! [`NOT_FROM_HERE`]) und merkt Ansicht und Punkt (in Ansichtskoordinaten). Der zweite Klick legt
//! die Richtung fest ([`snap`]: waagerecht, wenn |dx| ≥ |dy|); die quer liegende Koordinate ist die
//! des ersten Klicks und rastet auf die Mitte der Hüllbox ein, wenn sie auf dem Bildschirm näher
//! als [`SNAP_PX`] daran liegt ([`line_for`]). [`create`] ist genau eine Änderung des Dokuments.
//!
//! **Lage der neuen Ansicht (S12):** abhängig von der Quellansicht mit deren Maßstab, im
//! Vorgabeabstand; liegt schon ein Kind derselben Quellansicht in derselben Richtung, dahinter.

use crate::dimension_state::{NO_DRAWING, NO_VIEW, VIEWS_PENDING};
use crate::dimension_tool::sentence;
use crate::drawing_dialog::SKETCH_BLOCKED;
use crate::drawing_state::{default_distance, estimate_view_size};
use crate::sketch_view::parse_number;
use crate::state::AppState;
use egui::Pos2;
use planora_drawing::annotate::{
    Mark, MarkKind, SECTION_ARROW_LEN, SECTION_OVERHANG_MM, SECTION_SHAFT_MM, SectionPen,
};
use planora_drawing::sheet::{SheetTransform, view_transform};
use planora_drawing::sheet_frame::sheet_scale;
use planora_drawing::{
    Basis, Drawing, DrawingId, DrawingKind, GAP_MM, MAX_SECTIONS, Placement, ViewId, ViewKind,
    ViewLines, ViewSpec, align_dir, plane_offset_from_view, section_kind, section_line_coord,
};
use planora_feature::FeatureError;
use planora_math::DVec2;
use std::collections::BTreeMap;

/// Beschriftung des Schalters.
pub const TOOL: &str = "Schnitt";
/// Tooltip des Schalters.
pub const TOOL_TIP: &str = "Zwei Klicks in einer geraden Ansicht legen die Schnittlinie fest. Daraus entsteht eine Schnittansicht.";
/// Hinweis vor dem ersten Klick.
pub const HINT_FIRST: &str =
    "Anfang der Schnittlinie in einer geraden Ansicht anklicken. Esc beendet.";
/// Hinweis vor dem zweiten Klick.
pub const HINT_SECOND: &str =
    "Ende der Schnittlinie anklicken. Die Linie rastet waagerecht oder senkrecht ein.";
/// Ablehnung: der Schnitt geht nicht aus dieser Ansicht.
pub const NOT_FROM_HERE: &str = "Ein Schnitt geht nur aus einer geraden Ansicht.";
/// Ablehnung: beide Klicks liegen (fast) auf demselben Punkt.
pub const TOO_SHORT: &str = "Bitte zwei verschiedene Punkte anklicken.";
/// Ablehnung: zu viele Schnitte auf dem Blatt.
pub const FULL: &str = "Es sind höchstens 12 Schnitte je Blatt möglich.";
/// Hinweis im Ansichtsfeld, wenn die Ebene verschoben werden musste.
pub const NUDGED: &str =
    "Die Schnittebene lag genau auf einer Fläche oder Kante und wurde um 0,001 mm verschoben.";
/// Fangweite der Mitte der Hüllbox in Bildschirmpunkten.
pub const SNAP_PX: f32 = 8.0;
/// Erklärung am gesperrten Schalter in einer alten Version.
pub const READ_ONLY_TIP: &str = "In einer alten Version lassen sich keine Schnitte anlegen.";
/// Erklärung am gesperrten Schalter eines Stücklistenblatts.
pub const PARTS_SHEET_TIP: &str = "Auf dem Stücklistenblatt gibt es keine Schnitte.";
/// Erklärung, wenn es noch keine gerade Ansicht gibt.
pub const NO_STRAIGHT_TIP: &str = "Für einen Schnitt braucht das Blatt eine gerade Ansicht.";
/// Tooltip des gesperrten Maßstabs einer Schnittansicht.
pub const SCALE_LOCKED_TIP: &str = "Der Maßstab folgt der Ausgangsansicht.";
/// Meldung bei einer Eingabe, die keine Zahl ist.
pub const NOT_A_NUMBER: &str = "Bitte eine Zahl eingeben.";
/// Mindestabstand der beiden Klicks auf dem Blatt in Millimetern.
pub const MIN_LENGTH_MM: f64 = 1.0;

/// Schlüssel einer Meldung: Stand des Dokuments und gewählte Ansicht; ändert sich einer, verfällt
/// die Meldung.
pub type MessageKey = (u64, Option<ViewId>);

/// Zustand des Werkzeugs „Schnitt“.
#[derive(Debug, Default)]
pub struct SectionTool {
    active: bool,
    /// Erster Klick: Ansicht und Punkt in Ansichtskoordinaten.
    start: Option<(ViewId, DVec2)>,
    message: Option<String>,
    message_key: Option<MessageKey>,
}

impl SectionTool {
    /// Ist das Werkzeug eingeschaltet?
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Schaltet das Werkzeug ein oder aus; der erste Klick und die Meldung verfallen dabei.
    pub fn set_active(&mut self, on: bool) {
        self.active = on;
        self.start = None;
        self.message = None;
        self.message_key = None;
    }

    /// Der gemerkte erste Klick (Ansicht und Punkt in Ansichtskoordinaten).
    pub fn start(&self) -> Option<(ViewId, DVec2)> {
        self.start
    }

    /// Verwirft den ersten Klick; `true`, wenn es einen gab.
    pub fn cancel(&mut self) -> bool {
        self.start.take().is_some()
    }

    /// Letzte Ablehnung (rot in der Hinweiszeile).
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Setzt die Meldung für die Hinweiszeile.
    pub fn set_message(&mut self, message: Option<String>) {
        self.message = message;
        self.message_key = None;
    }

    /// Lässt die Meldung verfallen, sobald sich der Stand `key` gegenüber dem Bild ihres
    /// Entstehens ändert.
    pub fn expire(&mut self, key: MessageKey) {
        if self.message.is_none() {
            return;
        }
        match self.message_key {
            None => self.message_key = Some(key),
            Some(k) if k != key => {
                self.message = None;
                self.message_key = None;
            }
            Some(_) => {}
        }
    }

    /// Hinweis zur Bedienung bei eingeschaltetem Werkzeug.
    pub fn hint(&self) -> Option<&'static str> {
        self.active.then(|| {
            if self.start.is_some() {
                HINT_SECOND
            } else {
                HINT_FIRST
            }
        })
    }
}

/// Kann das Werkzeug benutzt werden? Sonst die Erklärung (alte Version, Stücklistenblatt, keine
/// gerade Ansicht ohne Schnitt, gewählte Ansicht ist Isometrie oder selbst ein Schnitt, schon 12
/// Schnitte).
pub fn blocked(
    drawing: &Drawing,
    selected: Option<&ViewSpec>,
    editable: bool,
) -> Option<&'static str> {
    if !editable {
        Some(READ_ONLY_TIP)
    } else if matches!(drawing.kind, DrawingKind::PartsList { .. }) {
        Some(PARTS_SHEET_TIP)
    } else if !drawing
        .views
        .iter()
        .any(|v| v.kind.is_normal() && v.section.is_none())
    {
        Some(NO_STRAIGHT_TIP)
    } else if selected.is_some_and(|v| !v.kind.is_normal() || v.section.is_some()) {
        Some(NOT_FROM_HERE)
    } else if drawing.views.iter().filter(|v| v.section.is_some()).count() >= MAX_SECTIONS {
        Some(FULL)
    } else {
        None
    }
}

/// Ist die Schnittlinie waagerecht? Ja, wenn |dx| ≥ |dy|.
pub fn snap(start: DVec2, end: DVec2) -> bool {
    let d = end - start;
    d.x.abs() >= d.y.abs()
}

/// Richtung und quer liegende Koordinate der Schnittlinie in Ansichtskoordinaten: die Koordinate
/// des Anfangs quer zur Richtung, auf die Mitte der Hüllbox `bbox` eingerastet, wenn sie bei
/// `px_per_unit` Bildschirmpunkten je Einheit näher als [`SNAP_PX`] daran liegt.
pub fn line_for(
    start: DVec2,
    end: DVec2,
    bbox: ((f64, f64), (f64, f64)),
    px_per_unit: f64,
) -> (bool, f64) {
    let horizontal = snap(start, end);
    let ((x0, y0), (x1, y1)) = bbox;
    let (coord, mid) = if horizontal {
        (start.y, 0.5 * (y0 + y1))
    } else {
        (start.x, 0.5 * (x0 + x1))
    };
    let near = (coord - mid).abs() * px_per_unit < f64::from(SNAP_PX);
    (horizontal, if near { mid } else { coord })
}

/// Ansichtskoordinaten des Blattpunkts `at`.
pub fn to_view(xf: &SheetTransform, at: DVec2) -> DVec2 {
    let s = if xf.scale.is_normal() { xf.scale } else { 1.0 };
    (at - DVec2::new(xf.tx, xf.ty)) / s + xf.bbox_center
}

fn feature_text(e: FeatureError) -> String {
    match e {
        FeatureError::InvalidInput(m) => sentence(&m),
        e => sentence(&e.to_string()),
    }
}

/// Abstand der neuen Schnittansicht `kind` aus `parent` (S12).
fn section_distance(state: &AppState, d: &Drawing, parent: &ViewSpec, kind: ViewKind) -> f64 {
    let base = default_distance(&state.bodies, parent.kind, kind, parent.scale, parent.scale);
    let Some(dir) = align_dir(d.projection, parent.kind, kind) else {
        return base;
    };
    let widest = d
        .views
        .iter()
        .filter(|c| c.parent == Some(parent.id))
        .filter(|c| align_dir(d.projection, parent.kind, c.kind) == Some(dir))
        .filter_map(|c| match c.placement {
            Placement::Aligned { distance } => Some(distance),
            Placement::Free { .. } => None,
        })
        .fold(None::<f64>, |m, x| Some(m.map_or(x, |m| m.max(x))));
    match widest {
        Some(w) => {
            let (ew, eh) = estimate_view_size(&state.bodies, kind);
            let ext = if dir.0 != 0.0 { ew } else { eh };
            base.max(w + ext * parent.scale.factor() + GAP_MM)
        }
        None => base,
    }
}

/// Legt eine Schnittansicht aus `parent` an: Schnittlinie `horizontal` bei der Ansichtskoordinate
/// `coord`, Blickrichtung nach S5. Eine Änderung des Dokuments; bei einem Fehler bleibt es
/// unverändert, und der Grund kommt als Satz zurück. Die neue Ansicht ist danach gewählt.
pub fn create(
    state: &mut AppState,
    drawing: DrawingId,
    parent: ViewId,
    horizontal: bool,
    coord: f64,
) -> Result<ViewId, String> {
    if state.sketch_session.is_some() {
        return Err(SKETCH_BLOCKED.into());
    }
    let d = state.document.drawing(drawing).ok_or(NO_DRAWING)?;
    let source = d.view(parent).ok_or(NO_VIEW)?;
    if let Some(why) = blocked(d, Some(source), true) {
        return Err(why.into());
    }
    if !coord.is_finite() {
        return Err(TOO_SHORT.into());
    }
    let kind = section_kind(source.kind, horizontal, false).ok_or(NOT_FROM_HERE)?;
    let offset = plane_offset_from_view(source.kind, kind, coord).ok_or(NOT_FROM_HERE)?;
    let distance = section_distance(state, d, source, kind);
    let mut made = None;
    state
        .document
        .update_drawing(drawing, |dr| {
            made = Some(dr.add_section(parent, kind, offset, distance));
        })
        .map_err(feature_text)?;
    let id = made.ok_or(NO_DRAWING)?.map_err(|m| sentence(&m))?;
    let ui = &mut state.drawing_ui;
    ui.selected_view = Some(id);
    ui.selected_dim = None;
    ui.selected_balloon = None;
    Ok(id)
}

/// Dreht die Blickrichtung der Schnittansicht `view` um (Gegenansicht, `offset` bleibt). Eine
/// Änderung des Dokuments.
pub fn flip(state: &mut AppState, drawing: DrawingId, view: ViewId) -> Result<(), String> {
    let d = state.document.drawing(drawing).ok_or(NO_DRAWING)?;
    let v = d.view(view).ok_or(NO_VIEW)?;
    if v.section.is_none() {
        return Err(NOT_FROM_HERE.into());
    }
    let opposite = ViewKind::ALL
        .into_iter()
        .filter(|k| k.is_normal())
        .find(|k| (Basis::of(*k).d + Basis::of(v.kind).d).length() < 1e-9)
        .ok_or(NOT_FROM_HERE)?;
    edit(state, drawing, view, |v| v.kind = opposite)
}

/// Ändert die Schnittansicht `view` in einer Änderung des Dokuments.
fn edit(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    f: impl FnOnce(&mut ViewSpec),
) -> Result<(), String> {
    state
        .document
        .update_drawing(drawing, |d| {
            if let Some(v) = d
                .views
                .iter_mut()
                .find(|v| v.id == view && v.section.is_some())
            {
                f(v);
            }
        })
        .map_err(feature_text)
}

/// Setzt die Schnittposition (Weltkoordinate der Ebene in mm) aus einem eingegebenen Text.
pub fn set_offset_text(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    text: &str,
) -> Result<(), String> {
    let value = parse_number(text).ok_or(NOT_A_NUMBER)?;
    edit(state, drawing, view, |v| {
        if let Some(s) = v.section.as_mut() {
            s.offset = value;
        }
    })
}

/// Schaltet die Schraffur der Schnittansicht `view` ein oder aus.
pub fn set_hatch(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    on: bool,
) -> Result<(), String> {
    edit(state, drawing, view, |v| {
        if let Some(s) = v.section.as_mut() {
            s.hatch.on = on;
        }
    })
}

/// Setzt den Schraffurabstand (A3-Millimeter) aus einem eingegebenen Text. Ein Abstand, der auf
/// diesem Blatt unter 0,5 mm käme, wird abgelehnt (S11).
pub fn set_spacing_text(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    text: &str,
) -> Result<(), String> {
    let value = parse_number(text).ok_or(NOT_A_NUMBER)?;
    edit(state, drawing, view, |v| {
        if let Some(s) = v.section.as_mut() {
            s.hatch.spacing = value;
        }
    })
}

/// Setzt den Maßstab der Ansicht `view` und überträgt ihn in derselben Änderung auf ihre
/// Schnittansichten (sie folgen der Ausgangsansicht).
pub fn set_scale(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    scale: planora_drawing::Scale,
) -> Result<(), String> {
    state
        .document
        .update_drawing(drawing, |d| {
            for v in &mut d.views {
                if v.id == view || (v.parent == Some(view) && v.section.is_some()) {
                    v.scale = scale;
                }
            }
        })
        .map_err(feature_text)
}

/// Zahl für ein Eingabefeld: bis zu drei Nachkommastellen, ohne überflüssige Nullen, mit Komma.
pub fn field_text(v: f64) -> String {
    let s = crate::sketch_view::format_number(v, 3);
    if s.contains(',') {
        s.trim_end_matches('0').trim_end_matches(',').to_string()
    } else {
        s
    }
}

// ---- Klicks ---------------------------------------------------------------------------------

/// Eine Ansicht mit Abbildung aufs Blatt und ihrer Hüllbox.
struct Shown<'a> {
    spec: &'a ViewSpec,
    lines: &'a ViewLines,
    xf: SheetTransform,
}

impl Shown<'_> {
    /// Hüllbox auf dem Blatt: (min, max).
    fn sheet_box(&self) -> (DVec2, DVec2) {
        let ((x0, y0), (x1, y1)) = self.lines.bbox;
        let a = self.xf.apply(DVec2::new(x0, y0));
        let b = self.xf.apply(DVec2::new(x1, y1));
        (
            DVec2::new(a.x.min(b.x), a.y.min(b.y)),
            DVec2::new(a.x.max(b.x), a.y.max(b.y)),
        )
    }
}

fn shown_views<'a>(d: &'a Drawing, results: &'a BTreeMap<ViewId, &'a ViewLines>) -> Vec<Shown<'a>> {
    d.views
        .iter()
        .filter_map(|spec| {
            let lines = *results.get(&spec.id)?;
            let xf = view_transform(d, spec.id, results)?;
            Some(Shown { spec, lines, xf })
        })
        .collect()
}

/// Klick mit eingeschaltetem Werkzeug am Blattpunkt `at` bei `px_per_mm` Bildschirmpunkten je
/// Blatt-Millimeter. Der erste Klick merkt Ansicht und Punkt, der zweite legt die Schnittansicht
/// an. Eine Ablehnung steht danach in der Meldung des Werkzeugs.
pub fn click(state: &mut AppState, drawing: DrawingId, at: DVec2, px_per_mm: f64) {
    state.section_tool.set_message(None);
    match try_click(state, drawing, at, px_per_mm) {
        Ok(()) => {}
        Err(m) => state.section_tool.set_message(Some(m)),
    }
}

fn try_click(state: &mut AppState, id: DrawingId, at: DVec2, px_per_mm: f64) -> Result<(), String> {
    if state.sketch_session.is_some() {
        return Err(SKETCH_BLOCKED.into());
    }
    let d = state.document.drawing(id).ok_or(NO_DRAWING)?;
    let owned = state
        .drawing_ui
        .results_for(d, state.document.model_edit_count());
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, l)| (*k, l.as_ref())).collect();
    let shown = shown_views(d, &refs);
    let Some((view, p0)) = state.section_tool.start else {
        // Erster Klick: die oberste Ansicht, in deren Hüllbox der Punkt liegt.
        let hit = shown.iter().rev().find(|s| {
            let (lo, hi) = s.sheet_box();
            at.x >= lo.x && at.x <= hi.x && at.y >= lo.y && at.y <= hi.y
        });
        let Some(s) = hit else {
            return Err(NOT_FROM_HERE.into());
        };
        if !s.spec.kind.is_normal() || s.spec.section.is_some() {
            return Err(NOT_FROM_HERE.into());
        }
        let p = to_view(&s.xf, at);
        let view = s.spec.id;
        state.section_tool.start = Some((view, p));
        return Ok(());
    };
    // Zweiter Klick.
    let Some(s) = shown.iter().find(|s| s.spec.id == view) else {
        state.section_tool.start = None;
        return Err(VIEWS_PENDING.into());
    };
    let end = to_view(&s.xf, at);
    if (end - p0).length() * s.xf.scale < MIN_LENGTH_MM {
        return Err(TOO_SHORT.into());
    }
    let (horizontal, coord) = line_for(p0, end, s.lines.bbox, s.xf.scale * px_per_mm);
    state.section_tool.start = None;
    create(state, id, view, horizontal, coord).map(|_| ())
}

/// Vorschau der Schnittlinie mit Pfeilen in Blickrichtung, wenn der erste Klick gesetzt ist und
/// der Zeiger am Blattpunkt `at` steht.
pub fn preview(state: &AppState, id: DrawingId, at: DVec2, px_per_mm: f64) -> Vec<Mark> {
    let Some((view, p0)) = state.section_tool.start else {
        return Vec::new();
    };
    let Some(d) = state.document.drawing(id) else {
        return Vec::new();
    };
    let owned = state
        .drawing_ui
        .results_for(d, state.document.model_edit_count());
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, l)| (*k, l.as_ref())).collect();
    let shown = shown_views(d, &refs);
    let Some(s) = shown.iter().find(|s| s.spec.id == view) else {
        return Vec::new();
    };
    let end = to_view(&s.xf, at);
    let (horizontal, coord) = line_for(p0, end, s.lines.bbox, s.xf.scale * px_per_mm);
    let f = sheet_scale(d.sheet);
    let Some(kind) = section_kind(s.spec.kind, horizontal, false) else {
        return Vec::new();
    };
    let ((x0, y0), (x1, y1)) = s.lines.bbox;
    let (pa, pb) = if horizontal {
        (DVec2::new(x0, coord), DVec2::new(x1, coord))
    } else {
        (DVec2::new(coord, y0), DVec2::new(coord, y1))
    };
    let u = if horizontal {
        DVec2::new(1.0, 0.0)
    } else {
        DVec2::new(0.0, 1.0)
    };
    let v = view_direction(s.spec.kind, kind);
    let a = s.xf.apply(pa) - u * (SECTION_OVERHANG_MM * f);
    let b = s.xf.apply(pb) + u * (SECTION_OVERHANG_MM * f);
    let pen = MarkKind::SectionLine(SectionPen::Chain);
    let mut marks = vec![Mark::Line { a, b, kind: pen }];
    for e in [a, b] {
        marks.push(Mark::Line {
            a: e,
            b: e + v * (SECTION_SHAFT_MM * f),
            kind: pen,
        });
        marks.push(Mark::Arrow {
            tip: e + v * ((SECTION_SHAFT_MM + SECTION_ARROW_LEN) * f),
            dir: DVec2::new(-v.x, -v.y),
            kind: pen,
        });
    }
    marks
}

/// Blickrichtung der Schnittansicht `kind` auf dem Blatt der Quellansicht `source` (S5).
fn view_direction(source: ViewKind, kind: ViewKind) -> DVec2 {
    let pb = Basis::of(source);
    let kd = Basis::of(kind).d;
    DVec2::new(kd.dot(pb.ex).round(), kd.dot(pb.ey).round())
}

/// Die Schnittlinien der Zeichnung auf dem Blatt: Schnittansicht und Endpunkte (mit Überstand).
pub fn section_lines(
    d: &Drawing,
    results: &BTreeMap<ViewId, &ViewLines>,
) -> Vec<(ViewId, DVec2, DVec2)> {
    let f = sheet_scale(d.sheet);
    let mut out = Vec::new();
    for spec in &d.views {
        let (Some(sec), Some(parent)) = (&spec.section, spec.parent) else {
            continue;
        };
        let (Some(src), Some(lines), Some(xf)) = (
            d.view(parent),
            results.get(&parent),
            view_transform(d, parent, results),
        ) else {
            continue;
        };
        let Some((horizontal, coord)) = section_line_coord(src.kind, &sec.plane(spec.kind)) else {
            continue;
        };
        let ((x0, y0), (x1, y1)) = lines.bbox;
        let (pa, pb, u) = if horizontal {
            (
                DVec2::new(x0, coord),
                DVec2::new(x1, coord),
                DVec2::new(1.0, 0.0),
            )
        } else {
            (
                DVec2::new(coord, y0),
                DVec2::new(coord, y1),
                DVec2::new(0.0, 1.0),
            )
        };
        out.push((
            spec.id,
            xf.apply(pa) - u * (SECTION_OVERHANG_MM * f),
            xf.apply(pb) + u * (SECTION_OVERHANG_MM * f),
        ));
    }
    out
}

/// Schnittansicht, deren Schnittlinie höchstens `pick_px` Bildschirmpunkte von `pos` entfernt
/// liegt (die nächste).
pub fn section_line_hit(
    lines: &[(ViewId, DVec2, DVec2)],
    to_screen: &dyn Fn(DVec2) -> Pos2,
    pos: Pos2,
    pick_px: f32,
) -> Option<ViewId> {
    let mut best: Option<(f32, ViewId)> = None;
    for (id, a, b) in lines {
        let d = crate::drawing_view::screen_seg_dist(pos, to_screen(*a), to_screen(*b));
        if d <= pick_px && best.is_none_or(|(x, _)| d < x) {
            best = Some((d, *id));
        }
    }
    best.map(|(_, id)| id)
}

/// Wählt die Schnittansicht `view` der offenen Zeichnung (Maß und Kreis sind dann nicht gewählt).
pub fn select(state: &mut AppState, view: ViewId) {
    let ui = &mut state.drawing_ui;
    ui.selected_view = Some(view);
    ui.selected_dim = None;
    ui.selected_balloon = None;
}
