//! Skizzen-Modus in der 3D-Ansicht: Abbildung Bildschirm ↔ Skizzenebene, Darstellung der Skizze
//! über der Szene und Weiterleiten der Eingaben an [`SketchSession`]. Die Logik (Werkzeuge, Fang,
//! Abhängigkeiten) liegt in `sketch_state`; hier wird nur gezeichnet und weitergereicht.

use crate::keymap::Action;
use crate::sketch_cursor::AutoConstraint;
use crate::sketch_grips::{self, Grip, GripLayout};
use crate::sketch_handles::HandlePart;
use crate::sketch_kinds::ToolKind;
use crate::sketch_pattern::{self, PatternGrip, PatternGripLayout};
use crate::sketch_state::{Item, ProjectFilter, SketchSession, Snap, Tool};
use crate::state::{AppState, PickFilter, ProjectTarget};
use crate::theme::{self, lighten as lighter, sketch_colors as colors};
use crate::viewport;
use egui::{Color32, FontId, PointerButton, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use glam::{Vec2, Vec3};
use planora_feature::{FeatureId, FeatureKind, SketchDisplay, SketchFeature};
use planora_i18n::tr;
use planora_math::{DVec2, DVec3, Frame};
use planora_render::{
    EdgeRef, FaceRef, OrbitCamera, Pick, StrokeVertex, StrokeView, exact_step, grid_radius,
    line_weight, push_fill, push_stroke, push_stroke_phased,
};
use planora_sketch::constraint::{ConstraintId, ConstraintKind, LineOrPoints};
use planora_sketch::dimension::{DimPick, DimPlan};
use planora_sketch::model::{CurveId, CurveKind, LineStyle, MAX_COORD, PointId, Sketch};
use planora_sketch::pattern::PatternSlot;
use planora_sketch::solver::{SolveReport, SolveStatus};
use std::collections::HashSet;
use std::f64::consts::{FRAC_PI_4, TAU};

/// Fangradius in Pixeln für Klicks und Hervorhebung.
pub const PICK_RADIUS_PX: f32 = 8.0;
/// Betrag des Kosinus zwischen Sehstrahl und Ebenennormale, unter dem der Strahl als parallel
/// zur Skizzenebene gilt (dann gibt es keinen brauchbaren Schnittpunkt).
const PARALLEL_COS: f64 = 1e-6;
/// Abschnitte je Vollkreis für Kreise, Bögen und Ellipsen in der Darstellung.
const STEPS_PER_TURN: usize = 96;
/// Länge der Achsenkreuz-Striche im Skizzenursprung in logischen Punkten.
const AXIS_PX: f32 = 28.0;
/// Strichstärke des Achsenkreuzes in logischen Punkten.
const AXIS_WIDTH: f32 = 1.5;
/// Radius der Punktsymbole in Pixeln.
const POINT_PX: f32 = 3.5;
/// Strichstärke normaler Kurven in Pixeln.
const CURVE_WIDTH: f32 = 2.0;
/// Halbe Größe eines gedämpften Griffs und des Rho-Griffs in Bildpunkten.
const HANDLE_GRIP_PX: f32 = 4.5;
/// Stärke der Farbe eines gedämpften Griffs.
const HANDLE_GHOST_FADE: f32 = 0.6;
/// Halbe Größe der Fangzeichen in Pixeln.
const SNAP_MARK_PX: f32 = 5.0;
/// Strichstärke der Fangzeichen in Pixeln.
const SNAP_MARK_WIDTH: f32 = 1.5;
/// Höchstzahl der Rasterlinien je Bild; darüber zeichnet das Raster nur die gröbere Stufe.
const GRID_MAX_LINES: f64 = 300.0;
/// Höchster Übergang zur gröberen Stufe: Die feinen Linien, auf denen der Zeiger rastet, bleiben
/// so immer schwach zu sehen.
const GRID_MAX_BLEND: f32 = 0.6;
/// Jede wievielte Linie der feinen Stufe zur gröberen gehört.
const GRID_DECADE: i64 = 10;
/// Rasterlinien, die schwächer als dies erscheinen, werden weggelassen.
const GRID_MIN_ALPHA: f32 = 0.01;
/// Strichstärke der Rasterlinien in Pixeln.
const GRID_WIDTH: f32 = 1.0;
/// Schriftgröße der Abhängigkeitssymbole und Maße.
const LABEL_SIZE: f32 = 12.0;

fn to_vec3(v: DVec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

fn to_dvec3(v: Vec3) -> DVec3 {
    DVec3::new(f64::from(v.x), f64::from(v.y), f64::from(v.z))
}

// ---- Abbildung Bildschirm ↔ Skizzenebene ------------------------------------------------------

/// Schneidet den Sehstrahl durch `pixel` (Ursprung oben links im Viewport der Größe `viewport`)
/// mit der Skizzenebene und liefert den Punkt in Skizzenkoordinaten; `None`, wenn der Strahl
/// parallel zur Ebene verläuft oder die Ebene hinter der Kamera liegt.
pub fn screen_to_sketch(
    camera: &OrbitCamera,
    frame: &Frame,
    viewport: Vec2,
    pixel: Vec2,
) -> Option<DVec2> {
    if viewport.x < 1.0 || viewport.y < 1.0 {
        return None;
    }
    let ndc = Vec2::new(
        pixel.x / viewport.x * 2.0 - 1.0,
        1.0 - pixel.y / viewport.y * 2.0,
    );
    let (origin, dir) = camera.ray(ndc);
    let (origin, dir) = (to_dvec3(origin), to_dvec3(dir));
    let denom = dir.dot(frame.z);
    if !denom.is_finite() || denom.abs() < PARALLEL_COS {
        return None;
    }
    let t = (frame.origin - origin).dot(frame.z) / denom;
    if !t.is_finite() || t < 0.0 {
        return None;
    }
    let local = frame.to_local(origin + dir * t);
    Some(DVec2::new(local.x, local.y))
}

/// Bildschirmposition (Pixel, Ursprung oben links) eines Skizzenpunkts; `None` hinter der Kamera.
pub fn sketch_to_screen(
    camera: &OrbitCamera,
    frame: &Frame,
    viewport: Vec2,
    p: DVec2,
) -> Option<Vec2> {
    camera.project(to_vec3(frame.point_2d(p)), viewport)
}

/// Fangradius in Skizzeneinheiten für `pixel_radius` Pixel an der Stelle `pixel`: Abstand zweier
/// benachbarter Pixel auf der Ebene mal `pixel_radius`.
pub fn pick_radius_units(
    camera: &OrbitCamera,
    frame: &Frame,
    viewport: Vec2,
    pixel: Vec2,
    pixel_radius: f32,
) -> Option<f64> {
    let a = screen_to_sketch(camera, frame, viewport, pixel)?;
    let b = screen_to_sketch(camera, frame, viewport, pixel + Vec2::X)?;
    Some(a.distance(b) * f64::from(pixel_radius))
}

// ---- 3D-Abbildung (Skizzen 10) ------------------------------------------------------------------

/// Kleinster Winkel zwischen Sehstrahl und Ebene, unter dem [`screen_to_plane`] keinen Punkt
/// liefert (5°): Bei fast streifendem Blick läge der Punkt sonst beinahe im Unendlichen.
pub const MIN_PICK_ANGLE: f64 = 5.0 * std::f64::consts::PI / 180.0;

/// Bildschirmposition (Pixel, Ursprung oben links) eines Punkts in Skizzenkoordinaten mit z;
/// `None` hinter der Kamera.
pub fn sketch3_to_screen(
    camera: &OrbitCamera,
    frame: &Frame,
    viewport: Vec2,
    p: DVec3,
) -> Option<Vec2> {
    camera.project(to_vec3(frame.point(p.x, p.y, p.z)), viewport)
}

/// Schneidet den Sehstrahl durch `pixel` (Ursprung oben links im Viewport der Größe `viewport`)
/// mit der Ebene durch `origin` mit der Normale `normal`, beide in Skizzenkoordinaten, und liefert
/// den Punkt in Skizzenkoordinaten. `None`, wenn der Winkel zwischen Strahl und Ebene unter
/// [`MIN_PICK_ANGLE`] liegt, die Ebene hinter der Kamera liegt, die Normale nicht normierbar ist
/// oder eine Koordinate des Ergebnisses über `MAX_COORD` geht.
pub fn screen_to_plane(
    camera: &OrbitCamera,
    frame: &Frame,
    viewport: Vec2,
    pixel: Vec2,
    origin: DVec3,
    normal: DVec3,
) -> Option<DVec3> {
    if viewport.x < 1.0 || viewport.y < 1.0 || !origin.is_finite() {
        return None;
    }
    let n = normal.try_normalize()?;
    let ndc = Vec2::new(
        pixel.x / viewport.x * 2.0 - 1.0,
        1.0 - pixel.y / viewport.y * 2.0,
    );
    let (eye, dir) = camera.ray(ndc);
    let (eye, dir) = (to_dvec3(eye), to_dvec3(dir));
    // Strahl in Skizzenkoordinaten (der Rahmen ist orthonormal, die Länge bleibt).
    let o = frame.to_local(eye);
    let d = (frame.to_local(eye + dir) - o).try_normalize()?;
    let denom = d.dot(n);
    if !denom.is_finite() || denom.abs() < MIN_PICK_ANGLE.sin() {
        return None;
    }
    let t = (origin - o).dot(n) / denom;
    if !t.is_finite() || t < 0.0 {
        return None;
    }
    let p = o + d * t;
    (p.is_finite() && p.abs().max_element() <= MAX_COORD).then_some(p)
}

/// Abschnitte für die Bahn einer 3D-Kurve: je Spline-Segment so viele wie in
/// [`Sketch::curve_polyline`].
const SPLINE_STEPS_PER_SEGMENT: usize = 16;

/// Punkte entlang der Kurve `c` in Skizzenkoordinaten mit z, abgetastet über
/// [`Sketch::curve_point3`] (alle Arten). Kreis und Ellipse sind geschlossen (letzter Punkt =
/// erster). `None` bei unbekannter oder entarteter Kurve.
pub fn curve_points3(sk: &Sketch, c: CurveId) -> Option<Vec<DVec3>> {
    let (lo, hi) = sk.curve_domain3(c)?;
    let span = hi - lo;
    if !span.is_finite() || span < 0.0 {
        return None;
    }
    let kind = &sk.curve(c)?.kind;
    let steps = match kind {
        CurveKind::Line { .. } => 1,
        CurveKind::Circle { .. }
        | CurveKind::Arc { .. }
        | CurveKind::Ellipse { .. }
        | CurveKind::EllipticArc { .. } => {
            ((span / TAU * STEPS_PER_TURN as f64).ceil() as usize).clamp(1, STEPS_PER_TURN)
        }
        CurveKind::Spline { .. } => ((span.round() as usize) * SPLINE_STEPS_PER_SEGMENT).max(1),
        _ => STEPS_PER_TURN,
    };
    let mut out: Vec<DVec3> = (0..=steps)
        .map(|k| sk.curve_point3(c, lo + span * k as f64 / steps as f64))
        .collect::<Option<_>>()?;
    if matches!(kind, CurveKind::Circle { .. } | CurveKind::Ellipse { .. }) {
        out[steps] = out[0];
    }
    Some(out)
}

/// Linienzug der Kurve `c` auf dem Bildschirm (alle Arten über [`Sketch::curve_point3`]). Stellen
/// hinter der Kamera fehlen; wer den Zug dort getrennt braucht, nimmt [`curve_screen_paths`].
pub fn curve_screen_path(sk: &Sketch, c: CurveId, map: &Mapper) -> Vec<Pos2> {
    curve_points3(sk, c)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| map.to_screen3(p))
        .collect()
}

/// Wie [`curve_screen_path`], an Stellen hinter der Kamera in Teilstücke zerlegt.
pub fn curve_screen_paths(sk: &Sketch, c: CurveId, map: &Mapper) -> Vec<Vec<Pos2>> {
    curve_points3(sk, c).map_or_else(Vec::new, |pts| map.paths3(&pts))
}

/// Abstand von `p` zur Strecke `a`–`b` auf dem Bildschirm.
pub(crate) fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

/// Was in 3D weder getroffen noch gefangen wird: unsichtbare Griffe, verborgene Musterteile,
/// Buchstaben und Rahmen eines Textes beim Ändern sowie die Punkte der Buchstaben ([`pick_3d`]
/// und der Fang beim Zeichnen in 3D), dazu was die Anzeigeschalter mit Konstruktion oder
/// Projektion ausblenden (Skizzen 12).
pub(crate) fn hidden_in_3d(
    se: &SketchSession,
    sk: &Sketch,
) -> crate::sketch_handles::HiddenHandles {
    let handles = se.hidden_handles(sk);
    let (mut hidden_curves, mut hidden_points) = se.pattern_hidden(sk);
    hidden_curves.extend(handles.curves);
    hidden_curves.extend(se.text_hidden(sk));
    hidden_points.extend(handles.points);
    hidden_points.extend(se.text_hidden_points(sk));
    hidden_points.extend(sk.glyph_points());
    let gone = se.display.hidden(sk);
    hidden_curves.extend(gone.curves);
    hidden_points.extend(gone.unsnapped);
    crate::sketch_handles::HiddenHandles {
        points: hidden_points,
        curves: hidden_curves,
    }
}

/// 3D-Punkt oder 3D-Kurve unter dem Zeiger `pointer`, über den Bildschirmabstand zur projizierten
/// Lage (höchstens [`PICK_RADIUS_PX`]): Punkte vor Kurven, jeweils der nächste. Was die Ansicht
/// nicht zeichnet, zählt nicht (wie beim 2D-Fang): unsichtbare Griffe, Exemplare eines Musters in
/// Bearbeitung, Buchstaben und Rahmen eines Textes beim Ändern sowie die Punkte der Buchstaben.
/// 2D-Geometrie trifft weiter [`SketchSession::snap`]. Für Wählen und Ziehen: Mit „Punkte
/// anzeigen“ aus (Skizzen 12) trifft es keinen Punkt; der Fang beim Zeichnen nimmt
/// [`pick_3d_for_snap`].
#[doc(hidden)]
pub fn pick_3d(se: &SketchSession, sk: &Sketch, map: &Mapper, pointer: Pos2) -> Option<Item> {
    pick_3d_with(se, sk, map, pointer, se.display.display.points)
}

/// Wie [`pick_3d`], aber für den Fang beim Zeichnen: 3D-Punkte fangen auch mit „Punkte
/// anzeigen“ aus (Skizzen 12, Spec 1: der Fang an End- und Mittelpunkten bleibt).
#[doc(hidden)]
pub fn pick_3d_for_snap(
    se: &SketchSession,
    sk: &Sketch,
    map: &Mapper,
    pointer: Pos2,
) -> Option<Item> {
    pick_3d_with(se, sk, map, pointer, true)
}

/// [`pick_3d`] mit `points_shown`: ob Punkte getroffen werden.
fn pick_3d_with(
    se: &SketchSession,
    sk: &Sketch,
    map: &Mapper,
    pointer: Pos2,
    points_shown: bool,
) -> Option<Item> {
    // Reine 2D-Skizze: nichts zu treffen, ohne die Mengen des Verborgenen zu bauen (eine 3D-Kurve
    // hat einen 3D-Punkt oder eine eigene Normale).
    if !sk.points().any(|(p, _)| sk.is_3d_point(p)) && !sk.curves().any(|(_, d)| d.normal.is_some())
    {
        return None;
    }
    let hidden = hidden_in_3d(se, sk);
    let nearest_point = sk
        .points()
        .map(|(p, _)| p)
        .filter(|&p| points_shown && sk.is_3d_point(p) && !hidden.points.contains(&p))
        .filter_map(|p| {
            let s = map.to_screen3(sk.point_xyz(p).ok()?)?;
            Some((p, s.distance(pointer)))
        })
        .filter(|&(_, d)| d <= PICK_RADIUS_PX)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((p, _)) = nearest_point {
        return Some(Item::Point(p));
    }
    sk.curves()
        .map(|(c, _)| c)
        .filter(|&c| sk.is_3d_curve(c) && !hidden.curves.contains(&c))
        .filter_map(|c| {
            let d = curve_screen_paths(sk, c, map)
                .iter()
                .flat_map(|path| path.windows(2))
                .map(|w| segment_distance(pointer, w[0], w[1]))
                .min_by(f32::total_cmp)?;
            Some((c, d))
        })
        .filter(|&(_, d)| d <= PICK_RADIUS_PX)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(c, _)| Item::Curve(c))
}

/// Was ein Zug im Auswahlwerkzeug bei `pointer` an 3D-Geometrie greift, mit der gefassten Stelle
/// in Skizzenkoordinaten: ein 3D-Punkt an seiner Lage, eine 3D-Kurve an der Stelle ihrer Bahn, die
/// dem Zeiger auf dem Bildschirm am nächsten liegt. `None`, wo ein 2D-Punkt fängt (er geht vor,
/// wie beim Klick) oder keine 3D-Geometrie liegt.
fn grab_3d(se: &SketchSession, sk: &Sketch, map: &Mapper, pointer: Pos2) -> Option<(Item, DVec3)> {
    let flat = map
        .to_sketch(pointer)
        .zip(map.radius_at(pointer))
        .map(|(at, r)| se.snap(sk, at, r));
    if matches!(flat, Some(Snap::Point(_))) {
        return None;
    }
    let item = pick_3d(se, sk, map, pointer)?;
    let grab = match item {
        Item::Point(p) => sk.point_xyz(p).ok()?,
        Item::Curve(c) => {
            let pts = curve_points3(sk, c)?;
            pts.windows(2)
                .filter_map(|w| {
                    let (a, b) = (map.to_screen3(w[0])?, map.to_screen3(w[1])?);
                    let ab = b - a;
                    let len2 = ab.length_sq();
                    let t = if len2 > 0.0 {
                        ((pointer - a).dot(ab) / len2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let d = pointer.distance(a + ab * t);
                    Some((d, w[0] + (w[1] - w[0]) * f64::from(t)))
                })
                .min_by(|x, y| x.0.total_cmp(&y.0))?
                .1
        }
        Item::Constraint(_) => return None,
    };
    Some((item, grab))
}

/// Was ein Klick im Auswahlwerkzeug am Zeiger `pointer` trifft: ein 2D-Punkt (Fang in der
/// Draufsicht) vor 3D-Punkten und 3D-Kurven ([`pick_3d`]), diese vor 2D-Kurven.
fn select_hit(se: &SketchSession, sk: &Sketch, map: &Mapper, pointer: Pos2) -> Option<Item> {
    let flat = map
        .to_sketch(pointer)
        .zip(map.radius_at(pointer))
        .map_or(Snap::None, |(at, r)| se.snap(sk, at, r));
    match flat {
        Snap::Point(p) => Some(Item::Point(p)),
        other => pick_3d(se, sk, map, pointer).or(match other {
            Snap::Curve(c) => Some(Item::Curve(c)),
            _ => None,
        }),
    }
}

// ---- Anzeigeschalter (Skizzen 12) ---------------------------------------------------------

/// Blendet ein Anzeigeschalter der Skizze (Skizzen 12, Spec 1) `item` aus? Der eine Filter für
/// Zeichnen, Wählen, Rahmenauswahl und Fang (über [`DisplayFilter::hidden`]). `projected` sind
/// die Kurven aus den Kopien des Entwurfs.
///
/// - Kurve: mit „Konstruktionsgeometrie anzeigen“ aus jede Konstruktions- und Mittellinie, mit
///   „Projizierte Geometrie anzeigen“ aus jede projizierte Kurve.
/// - Punkt: mit „Punkte anzeigen“ aus jeder (der Fang beim Zeichnen bleibt, siehe
///   [`DisplayHidden::unsnapped`]); sonst ein Konstruktionspunkt mit der Konstruktion und ein
///   Punkt, dessen Kurven alle ausgeblendet sind, mit ihnen.
/// - Abhängigkeit: ein Maß mit „Maße anzeigen“ aus, jede andere mit „Abhängigkeiten anzeigen“ aus.
///
/// Ausgeblendetes bleibt im Modell; Profile und Löser rechnen weiter damit.
pub fn hidden_by_display(
    d: &SketchDisplay,
    sk: &Sketch,
    projected: &HashSet<CurveId>,
    item: Item,
) -> bool {
    let curve_hidden = |c: CurveId| {
        sk.curve(c).is_some_and(|data| {
            (!d.construction && data.construction) || (!d.projected && projected.contains(&c))
        })
    };
    match item {
        Item::Curve(c) => curve_hidden(c),
        Item::Point(p) => {
            if !d.points {
                return true;
            }
            if d.construction && d.projected {
                return false;
            }
            if !d.construction && sk.points().any(|(q, data)| q == p && data.construction) {
                return true;
            }
            let mut used = false;
            for (c, data) in sk.curves() {
                if data.kind.points().contains(&p) {
                    if !curve_hidden(c) {
                        return false;
                    }
                    used = true;
                }
            }
            used
        }
        Item::Constraint(k) => sk.constraint(k).is_some_and(|c| {
            if c.kind.is_dimension() {
                !d.dimensions
            } else {
                !d.constraints
            }
        }),
    }
}

/// Die Anzeigeschalter einer Skizze mit ihrer projizierten Geometrie: was [`hidden_by_display`]
/// braucht, dazu die eingezogenen Punkte der Kopien (Ecken, Ursprung, Schnittpunkte), die mit
/// „Projizierte Geometrie anzeigen“ ebenfalls verschwinden.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayFilter {
    pub display: SketchDisplay,
    pub projected: HashSet<CurveId>,
    pub projected_points: HashSet<PointId>,
}

/// Was die Anzeigeschalter in einer Skizze ausblenden, als Mengen.
#[derive(Clone, Debug, Default)]
pub struct DisplayHidden {
    /// Nicht gezeichnet, nicht wählbar, kein Fang.
    pub curves: HashSet<CurveId>,
    /// Nicht gezeichnet, nicht wählbar (mit „Punkte anzeigen“ aus alle Punkte).
    pub points: HashSet<PointId>,
    /// Auch kein Fang beim Zeichnen: die Punkte ausgeblendeter Konstruktion und Projektion.
    /// „Punkte anzeigen“ allein nimmt den Fang an End- und Mittelpunkten nicht (Spec 1).
    pub unsnapped: HashSet<PointId>,
}

impl DisplayFilter {
    /// Schalter der Skizze `draft` (aus dem Dokument) und die Kopien ihres Entwurfs.
    pub fn of(display: SketchDisplay, draft: &SketchFeature) -> DisplayFilter {
        DisplayFilter {
            display,
            projected: draft.copies.iter().filter_map(|c| c.curve()).collect(),
            projected_points: draft.copies.iter().filter_map(|c| c.point()).collect(),
        }
    }

    /// Wie [`DisplayFilter::of`] für die geöffnete Skizze; ohne sie die Vorgabe (nichts aus).
    pub fn of_open(state: &AppState) -> DisplayFilter {
        match (state.open_sketch_id(), state.sketch_draft.as_ref()) {
            (Some(id), Some(draft)) => DisplayFilter::of(state.document.sketch_display(id), draft),
            _ => DisplayFilter::default(),
        }
    }

    /// Blendet der Filter `item` aus ([`hidden_by_display`], dazu die eingezogenen Punkte)?
    pub fn hides(&self, sk: &Sketch, item: Item) -> bool {
        hidden_by_display(&self.display, sk, &self.projected, item)
            || matches!(item, Item::Point(p) if !self.display.projected
                && self.projected_points.contains(&p))
    }

    /// Alles, was der Filter in `sk` ausblendet. Dasselbe wie [`hides`](Self::hides) je
    /// Element, für die Punkte aber in einem Durchgang über die Kurven.
    pub fn hidden(&self, sk: &Sketch) -> DisplayHidden {
        let d = &self.display;
        let mut out = DisplayHidden::default();
        if d.construction && d.projected {
            if !d.points {
                out.points = sk.points().map(|(p, _)| p).collect();
            }
            return out;
        }
        out.curves = sk
            .curves()
            .map(|(c, _)| c)
            .filter(|&c| hidden_by_display(d, sk, &self.projected, Item::Curve(c)))
            .collect();
        // Punkte an mindestens einer Kurve und an mindestens einer sichtbaren.
        let mut used = HashSet::new();
        let mut shown = HashSet::new();
        for (c, data) in sk.curves() {
            for p in data.kind.points() {
                used.insert(p);
                if !out.curves.contains(&c) {
                    shown.insert(p);
                }
            }
        }
        for (p, data) in sk.points() {
            let gone = (!d.construction && data.construction)
                || (used.contains(&p) && !shown.contains(&p))
                || (!d.projected && self.projected_points.contains(&p));
            if gone {
                out.unsnapped.insert(p);
            }
        }
        out.points = if d.points {
            out.unsnapped.clone()
        } else {
            sk.points().map(|(p, _)| p).collect()
        };
        out
    }
}

/// Ist `item` ein 3D-Punkt oder eine 3D-Kurve?
fn is_3d_item(sk: &Sketch, item: Item) -> bool {
    match item {
        Item::Point(p) => sk.is_3d_point(p),
        Item::Curve(c) => sk.is_3d_curve(c),
        _ => false,
    }
}

// ---- Zahlen und Namen ---------------------------------------------------------------------

/// Zahl mit `decimals` Nachkommastellen und dem Dezimaltrenner der Sprache (nur für die
/// Anzeige; ohne „-0,00“). Weiterleitung an [`planora_i18n::fmt_num`].
pub fn format_number(v: f64, decimals: usize) -> String {
    planora_i18n::fmt_num(v, decimals)
}

/// Liest eine Zahl; Komma und Punkt gelten als Dezimaltrenner. Nicht endliche Werte sind ungültig.
/// Weiterleitung an [`planora_i18n::parse_num`].
pub fn parse_number(text: &str) -> Option<f64> {
    planora_i18n::parse_num(text)
}

/// Länge (in mm) mit Einheit der Anzeige, z. B. „40,00 mm“ oder „1,575 in“.
pub fn format_length(v: f64) -> String {
    planora_i18n::fmt_len(v)
}

/// Winkel (Radiant) in Grad mit Einheit, z. B. „90,0°“.
pub fn format_angle(rad: f64) -> String {
    format!("{}°", format_number(rad.to_degrees(), 1))
}

/// Abstand der Winkelanzeige vom Zeiger in Bildpunkten (rechts unten daneben).
const ROTATE_LABEL_GAP: f32 = 18.0;
/// Schriftgröße der Winkelanzeige.
const ROTATE_LABEL_SIZE: f32 = 12.0;
/// Rand um den Text der Feldchen am Zeiger.
const LABEL_PAD: f32 = 3.0;

/// Der Winkel, den ein laufender Drehzug gerade zeigt (Radiant), sonst `None`: Zug am Drehgriff eines
/// Textes (Winkel des Textes), am Drehring und an den Drehbögen der Triade bei Verschieben/Kopieren
/// (gedrehter Winkel um die Achse des Griffs). Bei den Griffen der Triade gilt der Wert dieses
/// Bilds, ein gesperrtes Feld geht vor dem Griff (`transform_now`, `angles3`).
pub fn rotate_drag_angle(se: &SketchSession, sk: &Sketch) -> Option<f64> {
    if let Some(grab) = se.curve_drag.as_ref().and_then(|d| d.text_rot) {
        return sk.text_angle(grab.text);
    }
    let grip = se.grip_drag?.grip;
    match grip {
        Grip::Rotate => se.transform_now().map(|m| m.angle),
        Grip::RotateX => Some(se.angles3().0),
        Grip::RotateY => Some(se.angles3().1),
        _ => None,
    }
}

/// Obere linke Ecke eines Feldes der Größe `size` am Zeiger `pointer`: Es steht bevorzugt rechts unten
/// neben dem Zeiger, liegt dort ein Eingabefeld aus `avoid` darunter oder ragt es aus `bounds`, geht
/// es nach links und nach oben. Es bleibt immer in `bounds`; gibt es keinen freien Platz, gilt die
/// erste Lage, an den Rand geklemmt.
pub fn place_pointer_label(pointer: Pos2, size: egui::Vec2, avoid: &[Rect], bounds: Rect) -> Pos2 {
    let gap = ROTATE_LABEL_GAP;
    let clamp = |p: Pos2| {
        let max = (bounds.max - size).max(bounds.min);
        pos2(
            p.x.clamp(bounds.min.x, max.x),
            p.y.clamp(bounds.min.y, max.y),
        )
    };
    let tries = [
        pointer + vec2(gap, gap),
        pointer + vec2(-gap - size.x, gap),
        pointer + vec2(gap, -gap - size.y),
        pointer + vec2(-gap - size.x, -gap - size.y),
    ];
    let free = |p: Pos2| {
        let r = Rect::from_min_size(p, size).expand(LABEL_PAD);
        !avoid.iter().any(|a| a.intersects(r))
    };
    tries
        .iter()
        .map(|&p| clamp(p))
        .find(|&p| free(p))
        .unwrap_or_else(|| clamp(tries[0]))
}

/// Die Rechtecke der Eingabefelder, unter die kein Feld am Zeiger geraten soll: die Felder der
/// laufenden Eingabe und das Winkelfeld der Triade (Lage aus dem letzten Bild).
pub(crate) fn input_field_rects(ctx: &egui::Context, se: &SketchSession) -> Vec<Rect> {
    let mut out = crate::sketch_inline::field_rects(ctx, se);
    out.extend(crate::sketch_triad::angle_field_rect(ctx, se));
    out
}

/// Zeichnet ein Feld mit `text` am Zeiger, so gelegt, dass es kein Eingabefeld deckt und in `bounds`
/// bleibt ([`place_pointer_label`]). Gemeinsam für die Winkelanzeige und das Winkelfeld der Triade.
pub(crate) fn paint_pointer_label(
    painter: &egui::Painter,
    text: String,
    pointer: Pos2,
    bounds: Rect,
    avoid: &[Rect],
    col: &theme::SketchColors,
) {
    let galley = painter.layout_no_wrap(text, FontId::proportional(ROTATE_LABEL_SIZE), col.text);
    let size = galley.size();
    let at = place_pointer_label(
        pointer,
        size + vec2(2.0 * LABEL_PAD, 2.0 * LABEL_PAD),
        avoid,
        bounds,
    ) + vec2(LABEL_PAD, LABEL_PAD);
    let r = Rect::from_min_size(at, size).expand(LABEL_PAD);
    painter.rect_filled(r, 3.0, col.label_bg);
    painter.galley(at, galley, col.text);
}

/// Zeichnet die Winkelanzeige während eines Drehzugs am Zeiger.
fn paint_rotate_angle(
    painter: &egui::Painter,
    se: &SketchSession,
    sk: &Sketch,
    pointer: Option<Pos2>,
    rect: Rect,
    col: &theme::SketchColors,
) {
    let (Some(angle), Some(p)) = (rotate_drag_angle(se, sk), pointer) else {
        return;
    };
    let avoid = input_field_rects(painter.ctx(), se);
    paint_pointer_label(painter, format_angle(angle), p, rect, &avoid, col);
}

/// Ist das ein Winkelmaß (Anzeige und Eingabe in Grad)?
pub fn is_angle(kind: &ConstraintKind) -> bool {
    matches!(
        kind,
        ConstraintKind::Angle(..)
            | ConstraintKind::EllipseAngle(..)
            | ConstraintKind::PatternValue(_, PatternSlot::Angle, _)
    )
}

/// Eine Anzahl eines Musters (ohne Einheit, ganzzahlig)?
pub fn is_count(kind: &ConstraintKind) -> bool {
    matches!(kind, ConstraintKind::PatternValue(_, slot, _) if slot.is_count())
}

/// Gespeicherter Wert eines Maßes ([`ConstraintKind::value`]), auch eines Musterwerts (Skizzen 5:
/// Liste, Doppelklick und Formeln gehen denselben Weg wie bei jedem Maß).
pub fn stored_value(kind: &ConstraintKind) -> Option<f64> {
    kind.value()
}

/// Anzuzeigender Wert eines Maßes: bei anzeigenden (`driven`) Maßen der gemessene Wert, bei einem
/// Winkel mit `supplement` der Gegenwinkel (Skizzen 11a, A7).
pub fn shown_value(
    sk: &Sketch,
    kind: &ConstraintKind,
    driven: bool,
    supplement: bool,
) -> Option<f64> {
    if driven {
        crate::sketch_state::measure_kind(sk, kind, supplement).or_else(|| stored_value(kind))
    } else {
        stored_value(kind)
    }
}

/// Wert mit Einheit (Anzeigeeinheit oder °).
pub fn value_text(kind: &ConstraintKind, v: f64) -> String {
    if is_angle(kind) {
        format_angle(v)
    } else if is_count(kind) {
        format_number(v, 0)
    } else {
        format_length(v)
    }
}

/// Name der Abhängigkeitsart als gespeicherte Meldung (wechselt mit der Sprache).
pub fn kind_msg(kind: &ConstraintKind) -> crate::msg::Msg {
    use ConstraintKind as K;
    match kind {
        K::Coincident(..) => crate::msg!("sketch-constraint-coincident"),
        K::PointOnCurve(..) => crate::msg!("sketch-constraint-point-on-curve"),
        K::Horizontal(..) => crate::msg!("sketch-constraint-horizontal"),
        K::Vertical(..) => crate::msg!("sketch-constraint-vertical"),
        K::AlongZ(..) => crate::msg!("sketch-constraint-along-z"),
        K::Parallel(..) => crate::msg!("sketch-constraint-parallel"),
        K::Perpendicular(..) => crate::msg!("sketch-constraint-perpendicular"),
        K::Tangent(..) => crate::msg!("sketch-constraint-tangent"),
        K::Equal(..) => crate::msg!("sketch-constraint-equal"),
        K::Fixed(..) | K::Fixed3(..) => crate::msg!("sketch-constraint-fixed"),
        K::Symmetric(..) => crate::msg!("sketch-constraint-symmetric"),
        K::Midpoint(..) => crate::msg!("sketch-constraint-midpoint"),
        K::Distance(..) => crate::msg!("sketch-constraint-distance"),
        K::DistancePointLine(..) => crate::msg!("sketch-constraint-distance-point-line"),
        K::Radius(..) => crate::msg!("sketch-constraint-radius"),
        K::Diameter(..) => crate::msg!("sketch-constraint-diameter"),
        K::Angle(..) => crate::msg!("sketch-constraint-angle"),
        K::ArcRadius(..) => crate::msg!("sketch-constraint-arc-radius"),
        K::SplineCurvatureHandle(..) => crate::msg!("sketch-constraint-curvature-handle"),
        K::EllipticArcEnds(..) => crate::msg!("sketch-constraint-elliptic-arc-ends"),
        K::InPlane(..) => crate::msg!("sketch-constraint-in-plane"),
        K::EllipseAxes3(..) => crate::msg!("sketch-constraint-ellipse-axes"),
        K::EllipseMajor(..) => crate::msg!("sketch-constraint-ellipse-major"),
        K::EllipseMinor(..) => crate::msg!("sketch-constraint-ellipse-minor"),
        K::EllipseAngle(..) => crate::msg!("sketch-constraint-ellipse-angle"),
        K::Collinear(..) => crate::msg!("sketch-constraint-collinear"),
        K::Smooth(..) => crate::msg!("sketch-constraint-smooth"),
        K::EndTangent(..) => crate::msg!("sketch-constraint-end-tangent"),
        K::DistanceX(..) => crate::msg!("sketch-constraint-distance-x"),
        K::DistanceY(..) => crate::msg!("sketch-constraint-distance-y"),
        K::CenterlineDiameter(..) => crate::msg!("sketch-constraint-centerline-diameter"),
        K::SlotOverall(..) => crate::msg!("sketch-constraint-slot-overall"),
        K::PatternValue(_, slot, _) => match slot {
            PatternSlot::Count1 => crate::msg!("sketch-pattern-slot-count1"),
            PatternSlot::Spacing1 => crate::msg!("sketch-pattern-slot-spacing1"),
            PatternSlot::Count2 => crate::msg!("sketch-pattern-slot-count2"),
            PatternSlot::Spacing2 => crate::msg!("sketch-pattern-slot-spacing2"),
            PatternSlot::Count => crate::msg!("sketch-pattern-slot-count"),
            PatternSlot::Angle => crate::msg!("sketch-pattern-slot-angle"),
        },
    }
}

/// Name der Abhängigkeitsart in der aktuellen Sprache.
pub fn kind_label(kind: &ConstraintKind) -> String {
    kind_msg(kind).text()
}

/// Mitten aller Kreise, Bögen, Ellipsen und Ellipsenbögen: einmal je Bild oder Liste gebaut, damit
/// [`is_concentric_among`] nicht für jedes `Coincident` alle Kurven durchsucht.
pub fn round_centers(sk: &Sketch) -> HashSet<PointId> {
    sk.curves()
        .filter_map(|(c, _)| crate::sketch_state::round_center(sk, c))
        .collect()
}

/// Ist `kind` ein Konzentrisch (Skizzen 5)? Fusion „Konzentrisch“ ist hier ein `Coincident`
/// zweier Punkte, die beide Mitte eines Kreises, Bogens, einer Ellipse oder eines
/// Ellipsenbogens sind; Liste und Ansicht nennen es so. `centers` aus [`round_centers`].
pub fn is_concentric_among(centers: &HashSet<PointId>, kind: &ConstraintKind) -> bool {
    matches!(*kind, ConstraintKind::Coincident(a, b)
        if a != b && centers.contains(&a) && centers.contains(&b))
}

/// Wie [`is_concentric_among`] für eine einzelne Abhängigkeit.
pub fn is_concentric(sk: &Sketch, kind: &ConstraintKind) -> bool {
    matches!(kind, ConstraintKind::Coincident(..)) && is_concentric_among(&round_centers(sk), kind)
}

/// Name einer Abhängigkeit in Liste und Meldungen: wie [`kind_label`], ein Konzentrisch heißt so.
pub fn constraint_label(centers: &HashSet<PointId>, kind: &ConstraintKind) -> String {
    if is_concentric_among(centers, kind) {
        tr!("sketch-constraint-concentric")
    } else {
        kind_label(kind)
    }
}

/// Eintrag der Abhängigkeitsliste, z. B. „Abstand 40,00 mm“; anzeigende Maße in Klammern.
pub fn constraint_name(sk: &Sketch, kind: &ConstraintKind, driven: bool) -> String {
    constraint_name_among(sk, &round_centers(sk), kind, driven, false)
}

/// Wie [`constraint_name`] mit den Mitten aus [`round_centers`] (für die ganze Liste einmal).
pub fn constraint_name_among(
    sk: &Sketch,
    centers: &HashSet<PointId>,
    kind: &ConstraintKind,
    driven: bool,
    supplement: bool,
) -> String {
    let label = constraint_label(centers, kind);
    match shown_value(sk, kind, driven, supplement) {
        Some(v) if driven => format!("{label} ({})", value_text(kind, v)),
        Some(v) => format!("{label} {}", value_text(kind, v)),
        None => label,
    }
}

/// Gezeichnetes Zeichen einer Abhängigkeit in der Ansicht (Skizzen 5), im Stil der
/// Symbole aus [`crate::icons::HandIcon`] statt eines Schriftzeichens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaintedMark {
    /// Zwei Kreise um dieselbe Mitte.
    Concentric,
    /// Zwei Strecken auf einer Geraden.
    Collinear,
    /// Linie, die ohne Knick in eine Kurve übergeht.
    Smooth,
}

impl PaintedMark {
    fn icon(self) -> crate::icons::HandIcon {
        use crate::icons::HandIcon;
        match self {
            PaintedMark::Concentric => HandIcon::Concentric,
            PaintedMark::Collinear => HandIcon::Collinear,
            PaintedMark::Smooth => HandIcon::Smooth,
        }
    }
}

/// Das gezeichnete Zeichen von `kind`, wenn es eins hat (sonst steht dort [`overlay_text`]).
pub fn overlay_mark(centers: &HashSet<PointId>, kind: &ConstraintKind) -> Option<PaintedMark> {
    match kind {
        ConstraintKind::Coincident(..) if is_concentric_among(centers, kind) => {
            Some(PaintedMark::Concentric)
        }
        ConstraintKind::Collinear(..) => Some(PaintedMark::Collinear),
        ConstraintKind::Smooth(..) => Some(PaintedMark::Smooth),
        _ => None,
    }
}

/// Liefert `preferred`, wenn die Schrift alle Zeichen hat, sonst `fallback`.
fn symbol(ctx: &egui::Context, preferred: &'static str, fallback: &'static str) -> &'static str {
    let font = FontId::proportional(LABEL_SIZE);
    if ctx.fonts_mut(|f| f.has_glyphs(&font, preferred)) {
        preferred
    } else {
        fallback
    }
}

fn parallel_symbol(ctx: &egui::Context) -> &'static str {
    symbol(ctx, "∥", "||")
}

fn perpendicular_symbol(ctx: &egui::Context) -> String {
    match symbol(ctx, "⟂", "") {
        "" => tr!("sketch-overlay-perpendicular"),
        sym => sym.to_string(),
    }
}

/// Beschriftung einer Abhängigkeit in der Ansicht: Symbol bzw. Maß mit Einheit.
pub fn overlay_text(
    ctx: &egui::Context,
    sk: &Sketch,
    kind: &ConstraintKind,
    driven: bool,
) -> String {
    overlay_text_with(ctx, sk, kind, driven, false)
}

/// Wie [`overlay_text`]; `supplement`: das Winkelmaß misst den Gegenwinkel (Skizzen 11a, A7).
pub fn overlay_text_with(
    ctx: &egui::Context,
    sk: &Sketch,
    kind: &ConstraintKind,
    driven: bool,
    supplement: bool,
) -> String {
    use ConstraintKind as K;
    let plain = match kind {
        K::Coincident(..)
        | K::ArcRadius(..)
        | K::EllipticArcEnds(..)
        | K::InPlane(..)
        | K::EllipseAxes3(..)
        | K::SplineCurvatureHandle(..) => return String::new(),
        K::Horizontal(..) => "H".to_string(),
        K::Vertical(..) => "V".to_string(),
        K::AlongZ(..) => "Z".to_string(),
        K::Parallel(..) => parallel_symbol(ctx).to_string(),
        K::Perpendicular(..) => perpendicular_symbol(ctx),
        K::Equal(..) => "=".to_string(),
        K::Tangent(..) | K::EndTangent(..) => "T".to_string(),
        // Der Punkt im Ursprung ist „deckungsgleich mit dem Ursprung“, kein Festsetzen von Hand.
        K::Fixed(_, x, y) if *x == 0.0 && *y == 0.0 => return String::new(),
        K::Fixed3(_, x, y, z) if *x == 0.0 && *y == 0.0 && *z == 0.0 => return String::new(),
        K::Fixed(..) | K::Fixed3(..) => "fix".to_string(),
        K::Midpoint(..) => tr!("sketch-overlay-midpoint"),
        K::Symmetric(..) => tr!("sketch-overlay-symmetric"),
        K::PointOnCurve(..) => tr!("sketch-overlay-on-curve"),
        _ => String::new(),
    };
    if !plain.is_empty() {
        return plain;
    }
    let Some(v) = shown_value(sk, kind, driven, supplement) else {
        return String::new();
    };
    let text = match kind {
        K::Radius(..) => format!("R {}", format_length(v)),
        // Ø ohne Ersatz: Montserrat hat das Zeichen (tests/fonts.rs prüft das), und die
        // Glyphenabfrage von egui meldete es fälschlich als fehlend.
        K::Diameter(..) | K::CenterlineDiameter(..) => format!("Ø {}", format_length(v)),
        K::EllipseMajor(..) => format!("a {}", format_length(v)),
        K::EllipseMinor(..) => format!("b {}", format_length(v)),
        _ => value_text(kind, v),
    };
    if driven { format!("({text})") } else { text }
}

/// Zeichen einer automatischen Abhängigkeit, wie später an der fertigen Kurve.
fn auto_text(ctx: &egui::Context, auto: AutoConstraint) -> String {
    match auto {
        AutoConstraint::Horizontal => "H".to_string(),
        AutoConstraint::Vertical => "V".to_string(),
        AutoConstraint::Parallel(_) => parallel_symbol(ctx).to_string(),
        AutoConstraint::Perpendicular(_) => perpendicular_symbol(ctx),
        AutoConstraint::Tangent(_) => "T".to_string(),
    }
}

/// Stellen der Zeichen für die automatischen Abhängigkeiten des Zeigers: an der entstehenden Kurve
/// (Linie: Mitte, Bogen: am Zeiger) und, wenn es eine gibt, an der Bezugskurve.
pub fn auto_marks(sk: &Sketch, se: &SketchSession) -> Vec<(DVec2, AutoConstraint)> {
    let Some(cur) = &se.draw_cursor else {
        return Vec::new();
    };
    let own = match (se.tool, se.pending.last().and_then(|&p| pos(sk, p))) {
        // Mittelpunktlinie: Die Zeichen stehen in der Mitte der ganzen Linie, also an ihrer Mitte.
        (Tool::Line, Some(start)) if se.is_midpoint_line() => start,
        (Tool::Line, Some(start)) => (start + cur.at) * 0.5,
        _ => cur.at,
    };
    let mut marks = Vec::new();
    for &auto in &cur.autos {
        marks.push((own, auto));
        if let AutoConstraint::Parallel(c)
        | AutoConstraint::Perpendicular(c)
        | AutoConstraint::Tangent(c) = auto
            && let Some(at) = curve_anchor(sk, c)
        {
            marks.push((at, auto));
        }
    }
    marks
}

// ---- Lage der Beschriftungen ----------------------------------------------------------------

fn pos(sk: &Sketch, p: PointId) -> Option<DVec2> {
    sk.point_xy(p).ok()
}

/// Gut sichtbare Stelle an einer Kurve: Mitte einer Linie, 45° am Kreis, sonst die Mitte des
/// Linienzugs.
fn curve_anchor(sk: &Sketch, c: CurveId) -> Option<DVec2> {
    match &sk.curve(c)?.kind {
        CurveKind::Line { a, b } => Some((pos(sk, *a)? + pos(sk, *b)?) * 0.5),
        CurveKind::Circle { center, .. } => {
            let r = sk.radius(c)?;
            Some(pos(sk, *center)? + DVec2::new(FRAC_PI_4.cos(), FRAC_PI_4.sin()) * r)
        }
        _ => {
            let pl = sk.curve_polyline(c, STEPS_PER_TURN)?;
            pl.get(pl.len() / 2).copied()
        }
    }
}

/// Die Kurven, an denen eine reine Zeichen-Abhängigkeit (H, V, ∥, ⟂, =, T) ihr Zeichen zeigt; nur
/// dafür gilt „je Kurve und Art einmal“. Maße und Abhängigkeiten an Punkten: `None`.
fn marked_curves(kind: &ConstraintKind) -> Option<Vec<CurveId>> {
    use ConstraintKind as K;
    match kind {
        K::Horizontal(LineOrPoints::Line(c)) | K::Vertical(LineOrPoints::Line(c)) => Some(vec![*c]),
        K::Parallel(a, b)
        | K::Perpendicular(a, b)
        | K::Equal(a, b)
        | K::Tangent(a, b)
        | K::Collinear(a, b) => Some(vec![*a, *b]),
        _ => None,
    }
}

fn pos3(sk: &Sketch, p: PointId) -> Option<DVec3> {
    sk.point_xyz(p).ok()
}

/// Wie [`curve_anchor`] im Raum: An einer 3D-Kurve liegt die Stelle auf ihrer Bahn (Skizzen 10),
/// sonst in der Skizzenebene.
fn curve_anchor3(sk: &Sketch, c: CurveId) -> Option<DVec3> {
    if !sk.is_3d_curve(c) {
        return curve_anchor(sk, c).map(|a| a.extend(0.0));
    }
    match &sk.curve(c)?.kind {
        CurveKind::Line { a, b } => Some((pos3(sk, *a)? + pos3(sk, *b)?) * 0.5),
        kind => {
            let pts = curve_points3(sk, c)?;
            // Am Kreis ein Achtel der Bahn weit, wie die 45° in der Ebene.
            let k = if matches!(kind, CurveKind::Circle { .. }) {
                pts.len() / 8
            } else {
                pts.len() / 2
            };
            pts.get(k).copied()
        }
    }
}

/// Stellen (Skizzenkoordinaten mit z), an denen die Beschriftung einer Abhängigkeit erscheint: an
/// 3D-Geometrie an ihrer Lage im Raum (Skizzen 10), sonst in der Skizzenebene.
#[doc(hidden)]
pub fn constraint_anchors(sk: &Sketch, kind: &ConstraintKind) -> Vec<DVec3> {
    use ConstraintKind as K;
    let mid = |a: Option<DVec3>, b: Option<DVec3>| a.zip(b).map(|(a, b)| (a + b) * 0.5);
    let found = match kind {
        K::Horizontal(LineOrPoints::Line(c))
        | K::Vertical(LineOrPoints::Line(c))
        | K::AlongZ(LineOrPoints::Line(c)) => {
            vec![curve_anchor3(sk, *c)]
        }
        K::Horizontal(LineOrPoints::Points(a, b))
        | K::Vertical(LineOrPoints::Points(a, b))
        | K::AlongZ(LineOrPoints::Points(a, b))
        | K::Distance(a, b, _)
        | K::DistanceX(a, b, _)
        | K::DistanceY(a, b, _)
        | K::SlotOverall(a, b, ..) => vec![mid(pos3(sk, *a), pos3(sk, *b))],
        K::Parallel(a, b)
        | K::Perpendicular(a, b)
        | K::Equal(a, b)
        | K::Tangent(a, b)
        | K::Collinear(a, b) => {
            vec![curve_anchor3(sk, *a), curve_anchor3(sk, *b)]
        }
        // Am gemeinsamen (oder zusammengelegten) Ende ein Zeichen, nicht zwei übereinander.
        K::Smooth(_, a, _, b) | K::EndTangent(_, a, _, b) => {
            let (pa, pb) = (pos3(sk, *a), pos3(sk, *b));
            match pa.zip(pb) {
                Some((x, y)) if x.distance(y) <= sk.tolerance().linear => vec![pa],
                _ => vec![pa, pb],
            }
        }
        K::Angle(a, b, _) => vec![mid(curve_anchor3(sk, *a), curve_anchor3(sk, *b))],
        K::Fixed(p, ..)
        | K::Fixed3(p, ..)
        | K::Midpoint(p, _)
        | K::PointOnCurve(p, _)
        | K::Coincident(p, _) => {
            vec![pos3(sk, *p)]
        }
        K::Symmetric(a, b, _) => vec![pos3(sk, *a), pos3(sk, *b)],
        // Nur in der Skizzenebene (ohne 3D-Fassung).
        K::DistancePointLine(p, c, _) | K::CenterlineDiameter(p, c, _) => {
            let at = pos(sk, *p);
            let foot = at.and_then(|q| sk.nearest_on_curve(*c, q));
            vec![at.zip(foot).map(|(a, b)| ((a + b) * 0.5).extend(0.0))]
        }
        K::Radius(c, _)
        | K::Diameter(c, _)
        | K::ArcRadius(c)
        | K::SplineCurvatureHandle(c, _)
        | K::EllipticArcEnds(c)
        | K::InPlane(c)
        | K::EllipseAxes3(c, ..)
        | K::EllipseMajor(c, _)
        | K::EllipseMinor(c, _)
        | K::EllipseAngle(c, _) => vec![curve_anchor3(sk, *c)],
        // Musterwerte stehen in der Liste; in der Ansicht zeigt sie erst das Musterwerkzeug.
        K::PatternValue(..) => Vec::new(),
    };
    found.into_iter().flatten().collect()
}

// ---- Overlay ----------------------------------------------------------------------------------

/// Abbildung Skizze → Bildschirm für ein Bild (Kamera, Ebene und Viewport festgehalten).
pub struct Mapper {
    pub camera: OrbitCamera,
    /// Lage der Skizzenebene in der Welt.
    pub frame: Frame,
    /// Rechteck der Ansicht auf dem Bildschirm.
    pub rect: Rect,
}

impl Mapper {
    /// Bildschirmlage eines Punkts in Skizzenkoordinaten mit z (Skizzen 10); `None` hinter der
    /// Kamera.
    pub fn to_screen3(&self, p: DVec3) -> Option<Pos2> {
        let v = sketch3_to_screen(&self.camera, &self.frame, self.viewport(), p)?;
        v.is_finite()
            .then(|| pos2(self.rect.left() + v.x, self.rect.top() + v.y))
    }

    /// Wie [`Mapper::to_screen3`] für viele Punkte: Die Matrix der Kamera entsteht einmal, nicht
    /// je Punkt (Anklicken fertiger Skizzen, Task 12b). Gleiches Ergebnis wie `to_screen3`.
    pub fn screen3_fn(&self) -> impl Fn(DVec3) -> Option<Pos2> + '_ {
        let view_proj = self.camera.view_proj();
        let viewport = self.viewport();
        move |p| {
            let v = planora_render::OrbitCamera::project_with(
                &view_proj,
                to_vec3(self.frame.point(p.x, p.y, p.z)),
                viewport,
            )?;
            v.is_finite()
                .then(|| pos2(self.rect.left() + v.x, self.rect.top() + v.y))
        }
    }

    /// Blickrichtung in Skizzenkoordinaten (normiert): die Normale der Ebene parallel zum
    /// Bildschirm (Ziehen ohne Triade, Spec 10). `None` bei entarteter Kamera.
    pub fn view_normal(&self) -> Option<DVec3> {
        let (eye, dir) = self.camera.ray(Vec2::ZERO);
        let (eye, dir) = (to_dvec3(eye), to_dvec3(dir));
        (self.frame.to_local(eye + dir) - self.frame.to_local(eye)).try_normalize()
    }

    /// Der Bildschirmpunkt `p` auf der Ebene durch `origin` mit der Normale `normal`
    /// (Skizzenkoordinaten), siehe [`screen_to_plane`].
    pub fn screen_to_plane(&self, p: Pos2, origin: DVec3, normal: DVec3) -> Option<DVec3> {
        screen_to_plane(
            &self.camera,
            &self.frame,
            self.viewport(),
            self.local(p),
            origin,
            normal,
        )
    }

    /// Wie [`paths`](Self::paths) für Punkte mit z.
    fn paths3(&self, points: &[DVec3]) -> Vec<Vec<Pos2>> {
        let mut out: Vec<Vec<Pos2>> = vec![Vec::new()];
        for &p in points {
            match self.to_screen3(p) {
                Some(s) => {
                    if let Some(v) = out.last_mut() {
                        v.push(s);
                    }
                }
                None => out.push(Vec::new()),
            }
        }
        out.retain(|v| v.len() >= 2);
        out
    }

    fn viewport(&self) -> Vec2 {
        Vec2::new(self.rect.width(), self.rect.height())
    }

    fn to_screen(&self, p: DVec2) -> Option<Pos2> {
        let v = sketch_to_screen(&self.camera, &self.frame, self.viewport(), p)?;
        v.is_finite()
            .then(|| pos2(self.rect.left() + v.x, self.rect.top() + v.y))
    }

    fn local(&self, p: Pos2) -> Vec2 {
        Vec2::new(p.x - self.rect.left(), p.y - self.rect.top())
    }

    fn to_sketch(&self, p: Pos2) -> Option<DVec2> {
        screen_to_sketch(&self.camera, &self.frame, self.viewport(), self.local(p))
    }

    fn radius_at(&self, p: Pos2) -> Option<f64> {
        pick_radius_units(
            &self.camera,
            &self.frame,
            self.viewport(),
            self.local(p),
            PICK_RADIUS_PX,
        )
    }

    /// Linienzug auf dem Bildschirm; zerfällt an Stellen hinter der Kamera in Teilstücke.
    fn paths(&self, points: &[DVec2]) -> Vec<Vec<Pos2>> {
        let mut out: Vec<Vec<Pos2>> = vec![Vec::new()];
        for &p in points {
            match self.to_screen(p) {
                Some(s) => {
                    if let Some(v) = out.last_mut() {
                        v.push(s);
                    }
                }
                None => out.push(Vec::new()),
            }
        }
        out.retain(|v| v.len() >= 2);
        out
    }
}

/// Linienzug als Formen: gestrichelt (Konstruktionsgeometrie) oder durchgezogen.
fn path_shapes(path: Vec<Pos2>, stroke: Stroke, dashed: bool) -> Vec<Shape> {
    if dashed {
        Shape::dashed_line(&path, stroke, 6.0, 4.0)
    } else {
        vec![Shape::line(path, stroke)]
    }
}

fn draw_path(painter: &egui::Painter, path: Vec<Pos2>, stroke: Stroke, dashed: bool) {
    painter.extend(path_shapes(path, stroke, dashed));
}

/// Linienzug im Stil der Kurve: durchgezogen, gestrichelt (Konstruktion) oder als Strichpunkt
/// (Mittellinie: lange und kurze Striche im Wechsel).
pub fn style_shapes(path: Vec<Pos2>, stroke: Stroke, style: LineStyle) -> Vec<Shape> {
    match style {
        LineStyle::Normal => path_shapes(path, stroke, false),
        LineStyle::Construction => path_shapes(path, stroke, true),
        LineStyle::Centerline => {
            Shape::dashed_line_with_offset(&path, stroke, &[12.0, 2.0], &[3.0, 3.0], 0.0)
        }
    }
}

/// Bildschirmlage des Punkts `p`: ein 3D-Punkt mit z, alle anderen über die Draufsicht.
fn point_screen(sk: &Sketch, p: PointId, map: &Mapper) -> Option<Pos2> {
    if sk.is_3d_point(p) {
        map.to_screen3(sk.point_xyz(p).ok()?)
    } else {
        map.to_screen(pos(sk, p)?)
    }
}

/// Strichstärke des Randes einer Konstruktionsebene in der 3D-Ansicht (logische Punkte).
const PLANE_EDGE_WIDTH: f32 = 1.5;
/// Randbreite der Ebene unter dem Zeiger oder einer gewählten Ebene: deutlich dicker, damit sie
/// sich beim Wählen klar abhebt (Dennis' Handtest „Skizze erstellen“).
const PLANE_EDGE_WIDTH_ACTIVE: f32 = 3.0;

/// Farbe für die Skizzenlinien der GPU (Skizzen 12): sRGB-Werte 0..1 wie die übrige Szene,
/// Alpha nicht vormultipliziert. Die Pipeline mischt mit `ALPHA_BLENDING`, egui mit
/// vormultiplizierten Farben; beides ergibt dasselbe Bild.
pub fn stroke_rgba(c: Color32) -> [f32; 4] {
    let f = |v: u8| f32::from(v) / 255.0;
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    [f(r), f(g), f(b), f(a)]
}

/// Strich und Lücke der Konstruktionsgeometrie in logischen Punkten (wie [`path_shapes`]).
const DASH_PT: [f32; 2] = [6.0, 4.0];

/// Linienzug in Weltkoordinaten im Stil der Kurve für die GPU, Breite in logischen Punkten:
/// durchgezogen, gestrichelt oder als Strichpunkt, mit denselben Mustern wie [`style_shapes`].
fn push_styled(
    out: &mut Vec<StrokeVertex>,
    view: &StrokeView,
    path: &[DVec3],
    width: f32,
    color: [f32; 4],
    style: LineStyle,
) {
    let ppp = view.pixels_per_point;
    let w = width * ppp;
    match style {
        LineStyle::Normal => push_stroke(out, view, path, w, color, None),
        LineStyle::Construction => {
            push_stroke(out, view, path, w, color, Some(DASH_PT.map(|v| v * ppp)));
        }
        LineStyle::Centerline => {
            // 12 Strich, 3 Lücke, 2 Strich, 3 Lücke (Periode 20) als zwei versetzte Muster.
            let long = Some([12.0 * ppp, 8.0 * ppp]);
            let short = Some([2.0 * ppp, 18.0 * ppp]);
            push_stroke_phased(out, view, path, w, color, long, 0.0);
            push_stroke_phased(out, view, path, w, color, short, 5.0 * ppp);
        }
    }
}

/// Konstruktionsebenen in der 3D-Ansicht mit der Angabe, ob sie hell (gewählt, unter dem
/// Mauszeiger, vom Dialog markiert, Vorschau) gezeichnet werden: eingeblendete Grundebenen
/// zuerst, dann die Konstruktionsebenen, zuletzt die Vorschau des offenen Ebenen-Fensters.
fn shown_planes(state: &AppState) -> Vec<(planora_render::PlaneRect, bool)> {
    let origins = state.origin_plane_rects().into_iter().map(|(p, r)| {
        let lit = state.origin_hover == Some(p)
            || state.selected_origin_plane() == Some(p)
            || state.dialog_marked(&crate::state::IncludePick::Plane(
                planora_feature::PlaneId::Standard(p),
            ));
        (r, lit)
    });
    let features = state
        .plane_rects()
        .into_iter()
        .map(|(id, r)| (r, state.plane_highlighted(id)));
    // Vorschau des offenen Ebenen-Fensters in der Hervorhebungsfarbe (noch kein Feature).
    let preview = state.plane_pick.and_then(|p| p.preview).map(|r| (r, true));
    origins.chain(features).chain(preview).collect()
}

/// Konstruktionsebenen (Skizzen 6) als halbdurchsichtige orange Rechtecke mit Rand für die GPU
/// ([`AppState::plane_rects`]); gewählte (auch im Browser) und die unter dem Mauszeiger heller
/// ([`AppState::plane_highlighted`]). Mit Tiefenprüfung wie in Fusion: Körper verdecken sie
/// (Skizzen 12). Teile hinter der Kamera werden abgeschnitten.
pub fn plane_strokes(state: &AppState, view: &StrokeView, out: &mut Vec<StrokeVertex>) {
    if !view_usable(view) {
        return;
    }
    let col = colors(state.dark);
    for (plane, lit) in shown_planes(state) {
        let (fill, edge, width) = if lit {
            (
                col.plane_fill_active,
                col.plane_edge_active,
                PLANE_EDGE_WIDTH_ACTIVE,
            )
        } else {
            (col.plane_fill, col.plane_edge, PLANE_EDGE_WIDTH)
        };
        let [a, b, c, d] = plane.corners();
        push_fill(out, view, &[[a, b, c], [a, c, d]], stroke_rgba(fill));
        push_stroke(
            out,
            view,
            &[a, b, c, d, a],
            width * view.pixels_per_point,
            stroke_rgba(edge),
            None,
        );
    }
}

/// Ist die Ansicht groß genug zum Zeichnen (wie die 2 Punkte des egui-Overlays)?
fn view_usable(view: &StrokeView) -> bool {
    let min = 2.0 * view.pixels_per_point;
    view.size_px.x >= min && view.size_px.y >= min
}

/// Halbmesser eines gewählten Eckpunkts in der Ansicht.
const MARK_VERTEX_RADIUS: f32 = 4.0;
/// Strichstärke einer gewählten Achse des Ursprungs.
const MARK_AXIS_WIDTH: f32 = 2.0;

/// Eckpunkte und Achsen des Ursprungs, die ein offener Dialog gewählt hat
/// ([`AppState::dialog_marks`], Task H3), in der Auswahlfarbe über der Ansicht in `rect`;
/// Flächen, Kanten, Ebenen und Skizzenkurven hebt ihre eigene Zeichnung hervor.
pub fn dialog_mark_shapes(state: &AppState, rect: Rect) -> Vec<Shape> {
    use crate::state::IncludePick;
    use planora_feature::AxisId;
    let mut out = Vec::new();
    if state.dialog_marks.is_empty() || rect.width() < 2.0 || rect.height() < 2.0 {
        return out;
    }
    let col = colors(state.dark);
    let map = Mapper {
        camera: state.camera,
        frame: Frame::WORLD,
        rect,
    };
    let world = |q: DVec3| map.to_screen3(map.frame.to_local(q));
    // Die Achsen so lang, wie das Einbeziehen sie anlegt.
    let h =
        planora_feature::plane_rect::plane_half_for_bounds(state.bodies.iter().map(|b| b.bounds));
    for m in &state.dialog_marks {
        match *m {
            IncludePick::Vertex { body, edge, end } => {
                let Some(solid) = state.bodies.get(body).map(|b| &b.solid) else {
                    continue;
                };
                let Some(v) = solid
                    .edges
                    .get(edge)
                    .and_then(|e| solid.vertices.get(if end { e.end } else { e.start }))
                else {
                    continue;
                };
                if let Some(s) = world(v.point) {
                    out.push(Shape::circle_filled(s, MARK_VERTEX_RADIUS, col.selected));
                }
            }
            IncludePick::Axis(axis) => {
                let dir = match axis {
                    AxisId::X => DVec3::X,
                    AxisId::Y => DVec3::Y,
                    AxisId::Z => DVec3::Z,
                };
                if let (Some(a), Some(b)) = (world(-dir * h), world(dir * h)) {
                    out.push(Shape::line_segment(
                        [a, b],
                        Stroke::new(MARK_AXIS_WIDTH, col.selected),
                    ));
                }
            }
            _ => {}
        }
    }
    out
}

/// Strichstärke der Vorschau eines Einfügen-Fensters.
const INSERT_PREVIEW_WIDTH: f32 = 1.5;

/// Vorschau eines offenen Einfügen-Fensters ([`AppState::insert_preview`]) als Linien in der
/// Hervorhebungsfarbe über der Ansicht in `rect` (wie die fertigen Skizzen ohne Verdeckung).
/// Abschnitte mit einem Punkt hinter der Kamera fehlen.
pub fn insert_preview_shapes(state: &AppState, rect: Rect) -> Vec<Shape> {
    let mut out = Vec::new();
    let Some(preview) = &state.insert_preview else {
        return out;
    };
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return out;
    }
    let col = colors(state.dark);
    let map = Mapper {
        camera: state.camera,
        frame: preview.frame,
        rect,
    };
    let stroke = Stroke::new(INSERT_PREVIEW_WIDTH, col.plane_edge_active);
    for line in &preview.lines {
        let mut run: Vec<Pos2> = Vec::new();
        for p in line {
            match map.to_screen(*p) {
                Some(q) => run.push(q),
                None => {
                    if run.len() >= 2 {
                        out.push(Shape::line(std::mem::take(&mut run), stroke));
                    }
                    run.clear();
                }
            }
        }
        if run.len() >= 2 {
            out.push(Shape::line(run, stroke));
        }
    }
    out
}

/// Vorschau des offenen Dialogs „Isoparametrische Kurve“ (Skizzen 11b) als Linien in der
/// Hervorhebungsfarbe über der Ansicht in `rect` (wie die Vorschau eines Einfügen-Fensters).
/// Abschnitte mit einem Punkt hinter der Kamera fehlen.
pub fn iso_preview_shapes(state: &AppState, rect: Rect) -> Vec<Shape> {
    let mut out = Vec::new();
    let Some(crate::surface_project_dialog::SurfaceDialog::Iso(d)) = &state.surface_dialog else {
        return out;
    };
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return out;
    }
    let col = colors(state.dark);
    let map = Mapper {
        camera: state.camera,
        frame: Frame::WORLD,
        rect,
    };
    let stroke = Stroke::new(INSERT_PREVIEW_WIDTH, col.plane_edge_active);
    for line in d.preview_lines() {
        let mut run: Vec<Pos2> = Vec::new();
        for p in line {
            match map.to_screen3(*p) {
                Some(q) => run.push(q),
                None => {
                    if run.len() >= 2 {
                        out.push(Shape::line(std::mem::take(&mut run), stroke));
                    }
                    run.clear();
                }
            }
        }
        if run.len() >= 2 {
            out.push(Shape::line(run, stroke));
        }
    }
    out
}

/// Bildpunkte je Millimeter der Skizze in der Mitte der Ansicht (für die Mindestlänge der Zähne
/// des Krümmungskamms); `None`, wenn die Ansicht sie dort nicht bestimmen kann.
fn px_per_mm(map: &Mapper) -> Option<f64> {
    let r = map.radius_at(map.rect.center())?;
    (r.is_finite() && r > 0.0).then(|| f64::from(PICK_RADIUS_PX) / r)
}

/// Bildschirmlage für den Kamm: Punkte hinter der Kamera werden zu einem nicht endlichen Punkt,
/// den die Zeichnung auslässt.
fn comb_screen(map: &Mapper) -> impl Fn(DVec3) -> Pos2 + '_ {
    |p| map.to_screen3(p).unwrap_or(Pos2::new(f32::NAN, f32::NAN))
}

/// Krümmungskämme der Skizze `sk` (Skizzen 11a) für die Ansicht `map`.
fn comb_overlay(
    sk: &Sketch,
    map: &Mapper,
    color: Color32,
    hidden: &HashSet<CurveId>,
) -> Vec<Shape> {
    let Some(scale) = px_per_mm(map) else {
        return Vec::new();
    };
    crate::curvature_dialog::comb_shapes_skipping(sk, comb_screen(map), scale, color, hidden)
}

/// Strichstärke fertiger Skizzen in der 3D-Ansicht (dünner als im Skizzen-Modus).
const FINISHED_WIDTH: f32 = 1.2;
/// Strichstärke der Konstruktionsgeometrie fertiger Skizzen.
const FINISHED_CONSTRUCTION_WIDTH: f32 = 1.0;

/// Die fertigen (nicht geöffneten) Skizzen, die die 3D-Ansicht zeigt: die berechneten Skizzen
/// der übrigen Skizzen-Features (aufgelöste Ebene, nachgeführte Kantenkopien). Im Baum
/// ausgeblendete fehlen, es sei denn, ein Dialog wählt gerade ihre Profile (dann braucht man
/// sie). Die geöffnete zeichnet das Overlay aus dem Entwurf.
fn finished_sketches(state: &AppState) -> Vec<(FeatureId, &Sketch)> {
    let open = state.open_sketch_id();
    state
        .sketch_features()
        .into_iter()
        .filter(|(id, _)| open != Some(*id))
        .filter(|(id, _)| {
            let picking = state.profile_pick.as_ref().is_some_and(|p| p.sketch == *id);
            picking || !state.is_feature_hidden(*id)
        })
        .filter_map(|(id, _)| Some((id, state.document.sketch_of(id)?)))
        .collect()
}

/// Weltpunkte der Kurve `c` mit derselben Abtastung wie das frühere egui-Overlay: 3D-Kurven über
/// [`curve_points3`], alle anderen über [`Sketch::curve_polyline`]. `None`, wenn eine 2D-Kurve
/// keinen Linienzug hat.
pub(crate) fn curve_world_points(sk: &Sketch, c: CurveId) -> Option<Vec<DVec3>> {
    let frame = sk.frame();
    if sk.is_3d_curve(c) {
        Some(
            curve_points3(sk, c)
                .unwrap_or_default()
                .into_iter()
                .map(|p| frame.point(p.x, p.y, p.z))
                .collect(),
        )
    } else {
        Some(
            sk.curve_polyline(c, STEPS_PER_TURN)?
                .into_iter()
                .map(|p| frame.point(p.x, p.y, 0.0))
                .collect(),
        )
    }
}

/// Die Anzeigeschalter der fertigen Skizze `id` mit den Projektionen ihres Entwurfs.
fn finished_filter(state: &AppState, id: FeatureId) -> DisplayFilter {
    match state.document.feature(id).map(|f| &f.kind) {
        Some(FeatureKind::Sketch(def)) => DisplayFilter::of(state.document.sketch_display(id), def),
        _ => DisplayFilter::default(),
    }
}

/// Abgetastete Kurven und feste Mengen der fertigen Skizze `sk` des Features `id` aus dem
/// Zwischenspeicher ([`crate::finished_cache`], Task 12b).
fn finished_data(
    state: &AppState,
    id: FeatureId,
    sk: &Sketch,
) -> std::sync::Arc<crate::finished_cache::FinishedSketch> {
    state.finished_cache.get(id, sk, finished_filter(state, id))
}

/// Fertige (nicht geöffnete) Skizzen als dünne Linien für die GPU mit Tiefenprüfung
/// (Skizzen 12): Körper verdecken sie wie in Fusion. Konstruktionsgeometrie grau und
/// gestrichelt, Projektionen lila, verlorene Projektionen gelb, von einem Dialog gewählte
/// Kurven in der Auswahlfarbe, Kontrollpunktrahmen dünn und gestrichelt. Wählt ein Dialog
/// Profile ([`AppState::profile_pick`]), liegen die Flächen dieser Skizze unter ihren Linien:
/// gewählte kräftiger, die unter dem Mauszeiger heller. Die geöffnete Skizze zeichnet
/// [`show_overlay`]. Kurven fertiger Skizzen lassen sich anklicken ([`finished_sketch_under`]);
/// die gewählte und die unter dem Mauszeiger färben sich.
pub fn finished_sketch_strokes(state: &AppState, view: &StrokeView, out: &mut Vec<StrokeVertex>) {
    if !view_usable(view) {
        return;
    }
    let col = colors(state.dark);
    let shown = finished_sketches(state);
    // Was gerade nicht gezeigt wird, fliegt aus dem Zwischenspeicher.
    state
        .finished_cache
        .retain(|f| shown.iter().any(|(id, _)| *id == f));
    for (id, sk) in shown {
        let frame = *sk.frame();
        if let Some(pick) = state.profile_pick.as_ref().filter(|p| p.sketch == id) {
            let extracted = state.profile_cache.get(sk);
            for i in 0..extracted.profiles.len() {
                let fill = if pick.hover.contains(&i) {
                    col.profile_fill_hover
                } else if pick.selected.contains(&i) {
                    col.profile_fill_selected
                } else {
                    col.profile_fill
                };
                let triangles: Vec<[DVec3; 3]> = extracted
                    .fill(i)
                    .iter()
                    .map(|t| t.map(|p| frame.point(p.x, p.y, 0.0)))
                    .collect();
                push_fill(out, view, &triangles, stroke_rgba(fill));
            }
        }
        // Projektionen lila wie in Fusion (Skizzen 6), Konstruktion weiter gestrichelt.
        // „Konstruktionsgeometrie anzeigen“ und „Projizierte Geometrie anzeigen“ der Skizze
        // gelten auch fertig (Skizzen 12, Spec 1, **Annahme**). Der Zwischenspeicher hält die
        // Linienzüge ohne die ausgeblendeten Kurven und ohne Griffe ohne Abhängigkeit (eine
        // fertige Skizze hat keine Auswahl, B6).
        let display = finished_filter(state, id);
        let projected = &display.projected;
        let cached = state.finished_cache.get(id, sk, display.clone());
        // Verlorene Projektionen gelb, mit „Übrige Geometrie abblenden“ alles andere gedämpft
        // (Skizzen 11a).
        let lost = crate::lost_projections_dialog::marks(state, id);
        for (c, points) in &cached.curves {
            let c = *c;
            let Some(data) = sk.curve(c) else {
                continue;
            };
            let width = if data.construction {
                FINISHED_CONSTRUCTION_WIDTH
            } else {
                FINISHED_WIDTH
            };
            // Von einem offenen Dialog gewählt (Task H3): in der Auswahlfarbe.
            let marked = state.dialog_marked(&crate::state::IncludePick::SketchCurve {
                sketch: id,
                curve: c,
            });
            // Angeklickt (Auswahl) oder unter dem Zeiger (Skizzen 12, Task 8).
            let this = Pick::SketchCurve {
                sketch: id,
                curve: c,
            };
            let chosen = state.selection == Some(this);
            let base = if marked || chosen {
                col.selected
            } else if state.hover == Some(this) {
                col.preview
            } else if projected.contains(&c) {
                col.projected
            } else if data.construction {
                col.construction
            } else {
                col.finished
            };
            let color = lost_ink(
                base,
                marked || chosen,
                lost.curves.contains(&c),
                lost.dim_rest,
                col.lost,
            );
            push_styled(out, view, points, width, stroke_rgba(color), data.style());
        }
        // Kontrollpunktrahmen (Skizzen 9) dünn und gestrichelt.
        let frame_color = stroke_rgba(col.construction);
        for points in &cached.frames {
            push_styled(
                out,
                view,
                points,
                FINISHED_CONSTRUCTION_WIDTH,
                frame_color,
                LineStyle::Construction,
            );
        }
    }
}

/// Was von den fertigen Skizzen über der 3D-Ansicht in `rect` im egui-Overlay bleibt: der
/// Krümmungskamm (Skizzen 11a), auch außerhalb des Skizzen-Modus. Die Linien und Profilflächen
/// zeichnet die GPU ([`finished_sketch_strokes`]).
pub fn finished_sketch_shapes(state: &AppState, rect: Rect) -> Vec<Shape> {
    let mut out = Vec::new();
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return out;
    }
    let col = colors(state.dark);
    for (id, sk) in finished_sketches(state) {
        // Ohne Kamm bleibt nichts zu tun (und die Griffe müssen nicht gesucht werden).
        if !sk.curves().any(|(_, d)| d.comb.is_some()) {
            continue;
        }
        let map = Mapper {
            camera: state.camera,
            frame: *sk.frame(),
            rect,
        };
        let cached = finished_data(state, id, sk);
        out.extend(comb_overlay(sk, &map, col.comb, &cached.handles.curves));
    }
    // Der angeklickte oder überfahrene Skizzenpunkt (Task 8): kleiner Kreis, die Punkte fertiger
    // Skizzen werden sonst nicht gezeichnet.
    let marker = |pick: Option<Pick>, color: Color32| -> Option<Shape> {
        let Some(Pick::SketchPoint { sketch, point }) = pick else {
            return None;
        };
        let sk = state.document.sketch_of(sketch)?;
        let local = sk.point_xyz(point).ok()?;
        let world = sk.frame().point(local.x, local.y, local.z);
        let map = Mapper {
            camera: state.camera,
            frame: Frame::WORLD,
            rect,
        };
        let at = map.to_screen3(world)?;
        Some(Shape::circle_stroke(at, 4.5, Stroke::new(2.0, color)))
    };
    out.extend(marker(state.hover, col.preview));
    out.extend(marker(state.selection, col.selected));
    out
}

/// Die Maße der fertigen Skizzen mit „Maße anzeigen“ ([`SketchDisplay::dimensions_finished`],
/// Vorgabe aus) als Overlay obenauf (Skizzen 12, Task 9): derselbe Text wie in der offenen
/// Skizze ([`overlay_text_with`]) am selben Anker ([`constraint_anchors`]), nur zur Ansicht, nicht
/// anklickbar (kein [`LabelHit`]). Sichtbar wie die Linien der Skizze (Auge, Ordner, Gruppe über
/// [`finished_sketches`]); die Schalter der Skizze gelten ([`DisplayFilter`], „Maße anzeigen“ im
/// Skizzenfeld aus blendet auch hier aus). Wie bei der offenen Skizze zeigen Maße nur ihren Wert,
/// Hilfslinien gibt es nur in der Vorschau der Bemaßung ([`dimension_helpers`]).
/// `ctx` liefert die Schrift für die Textanordnung.
///
/// [`SketchDisplay::dimensions_finished`]: planora_feature::SketchDisplay::dimensions_finished
pub fn finished_dimension_shapes(ctx: &egui::Context, state: &AppState, rect: Rect) -> Vec<Shape> {
    let mut out = Vec::new();
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return out;
    }
    let col = colors(state.dark);
    let mut placed: Vec<Rect> = Vec::new();
    for (id, sk) in finished_sketches(state) {
        if !state.document.sketch_display(id).dimensions_finished {
            continue;
        }
        let display = finished_filter(state, id);
        let map = Mapper {
            camera: state.camera,
            frame: *sk.frame(),
            rect,
        };
        for (cid, c) in sk.constraints() {
            if !c.kind.is_dimension() || display.hides(sk, Item::Constraint(cid)) {
                continue;
            }
            let text = overlay_text_with(ctx, sk, &c.kind, c.driven, c.supplement());
            if text.is_empty() {
                continue;
            }
            let color = if c.driven { col.construction } else { col.text };
            for a in constraint_anchors(sk, &c.kind) {
                let Some(s) = map.to_screen3(a) else {
                    continue;
                };
                let galley = ctx.fonts_mut(|f| {
                    f.layout_job(egui::text::LayoutJob::simple_singleline(
                        text.clone(),
                        FontId::proportional(LABEL_SIZE),
                        color,
                    ))
                });
                let at = s + vec2(6.0, -18.0);
                let r = place_label(
                    Rect::from_min_size(at, galley.size()).expand(2.0),
                    &mut placed,
                );
                out.push(Shape::rect_filled(r, 3.0, col.label_bg));
                out.push(Shape::galley(r.min + vec2(2.0, 2.0), galley, color));
            }
        }
    }
    out
}

/// Sichtbarer Bereich der Skizzenebene `(min, max)`: die Ecken der Ansicht auf der Ebene, höchstens
/// so weit wie das Bodenraster um das Ziel der Kamera reicht (der ganze Umkreis, wenn eine Ecke die
/// Ebene nicht trifft).
fn grid_window(map: &Mapper) -> (DVec2, DVec2) {
    let target = map.frame.to_local(to_dvec3(map.camera.target));
    let target = DVec2::new(target.x, target.y);
    let reach = DVec2::splat(f64::from(grid_radius(map.camera.distance)));
    let (lo, hi) = (target - reach, target + reach);
    let r = map.rect;
    let corners: Option<Vec<DVec2>> = [
        r.left_top(),
        r.right_top(),
        r.left_bottom(),
        r.right_bottom(),
    ]
    .into_iter()
    .map(|c| map.to_sketch(c))
    .collect();
    let Some(corners) = corners else {
        return (lo, hi);
    };
    let seen = corners
        .iter()
        .fold((hi, lo), |(a, b), &c| (a.min(c), b.max(c)));
    (seen.0.max(lo), seen.1.min(hi))
}

/// Was das Skizzenraster in diesem Bild zeichnet; Zeichnen und Raster-Fang lesen beide von hier.
struct GridPlan {
    fine: f64,
    blend: f32,
    /// Jede wievielte Linie der feinen Stufe gezeichnet wird: 1, oder [`GRID_DECADE`], wenn es sonst
    /// mehr als [`GRID_MAX_LINES`] wären (geneigte Ansicht).
    every: i64,
    lo: DVec2,
    hi: DVec2,
}

impl GridPlan {
    /// Das Raster dieses Bilds; `None` bei unbrauchbarer Kamera oder Ansicht.
    fn of(map: &Mapper) -> Option<GridPlan> {
        let distance = map.camera.distance;
        if map.rect.width() < 2.0
            || map.rect.height() < 2.0
            || !distance.is_finite()
            || distance <= 0.0
        {
            return None;
        }
        let (fine, blend) = exact_step(distance, planora_i18n::current_unit().mm_per_unit());
        let (lo, hi) = grid_window(map);
        if !(lo.is_finite() && hi.is_finite()) {
            return None;
        }
        let count = |a: f64, b: f64| (b / fine).floor() - (a / fine).ceil() + 1.0;
        let every = if count(lo.x, hi.x) + count(lo.y, hi.y) > GRID_MAX_LINES {
            GRID_DECADE
        } else {
            1
        };
        Some(GridPlan {
            fine,
            blend,
            every,
            lo,
            hi,
        })
    }

    /// Stufe des Raster-Fangs: die feinste gezeichnete; ist das Raster ausgedünnt, bleiben nur die
    /// Linien der gröberen Stufe zu sehen, also rastet auch der Zeiger nur dort.
    fn snap_step(&self) -> f64 {
        self.fine * self.every as f64
    }
}

/// Stück `from..to` der Rasterlinie durch den Skizzenursprung in Richtung `dir` (Welt), auf dem
/// keine Achse der 3D-Szene liegt ([`planora_render::axis_lines`]): Die x- und die y-Achse laufen
/// beidseits des Weltursprungs, die z-Achse nur von dort nach oben. `None`: ganz belegt. Andere
/// Ebenen behalten die ganze Linie.
fn zero_line_span(frame: &Frame, dir: DVec3, from: f64, to: f64, tol: f64) -> Option<(f64, f64)> {
    /// Ab hier läuft eine Richtung der Ebene genau entlang einer Weltachse.
    const ALIGNED: f64 = 1.0 - 1e-9;
    if frame.origin.length() > tol {
        return Some((from, to));
    }
    if dir.x.abs() >= ALIGNED || dir.y.abs() >= ALIGNED {
        return None;
    }
    if dir.z >= ALIGNED {
        return (from < 0.0).then_some((from, to.min(0.0)));
    }
    if dir.z <= -ALIGNED {
        return (to > 0.0).then_some((from.max(0.0), to));
    }
    Some((from, to))
}

/// Eine Linie des Skizzenrasters: Anfang und Ende in Skizzenkoordinaten, Farbe samt Abblendung.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLine {
    pub from: DVec2,
    pub to: DVec2,
    pub color: Color32,
}

/// Skizzenraster in der Ebene der offenen Skizze im Viewport `rect` (leer, wenn es ausgeschaltet
/// ist): Stufe und Übergang wie das Bodenraster, jede Linie auf einem Vielfachen der feinen Stufe.
/// Höchstens [`GRID_MAX_LINES`] Linien. Wo eine Achse der 3D-Szene liegt, läuft keine Rasterlinie.
/// Gezeichnet von [`open_sketch_strokes`] auf der GPU mit Tiefenprüfung.
pub fn sketch_grid_lines(state: &AppState, rect: Rect) -> Vec<GridLine> {
    let mut out = Vec::new();
    let Some(map) = mapper(rect, state).filter(|_| state.sketch_show_grid) else {
        return out;
    };
    let Some(plan) = GridPlan::of(&map) else {
        return out;
    };
    let GridPlan {
        fine,
        blend,
        every,
        lo,
        hi,
    } = plan;
    let colors = theme::palette(state.dark);
    let tol = state
        .session_sketch()
        .map_or(0.0, |sk| sk.tolerance().linear);
    for along_x in [true, false] {
        let (first, last) = if along_x { (lo.y, hi.y) } else { (lo.x, hi.x) };
        let (start, end) = if along_x { (lo.x, hi.x) } else { (lo.y, hi.y) };
        let dir = if along_x { map.frame.x } else { map.frame.y };
        for k in (first / fine).ceil() as i64..=(last / fine).floor() as i64 {
            if k.rem_euclid(every) != 0 {
                continue;
            }
            let span = if k == 0 {
                zero_line_span(&map.frame, dir, start, end, tol)
            } else {
                Some((start, end))
            };
            let Some((from, to)) = span else {
                continue;
            };
            let (share, alpha) = line_weight(k, blend.min(GRID_MAX_BLEND));
            if alpha < GRID_MIN_ALPHA {
                continue;
            }
            let at = k as f64 * fine;
            let point = |v: f64| {
                if along_x {
                    DVec2::new(v, at)
                } else {
                    DVec2::new(at, v)
                }
            };
            let color = colors
                .viewport_grid
                .lerp_to_gamma(colors.viewport_grid_major, share)
                .gamma_multiply(alpha);
            out.push(GridLine {
                from: point(from),
                to: point(to),
                color,
            });
        }
    }
    out
}

/// Skizzenraster ([`sketch_grid_lines`]) als GPU-Linien mit Tiefenprüfung (Handtest 1, Fund 1):
/// Körper verdecken es wie in Fusion; Slice schneidet es nicht. Farben, Abstände, Ausdünnung und
/// Strichstärken wie vorher im Overlay. Teile hinter der Kamera schneidet [`push_stroke`] ab,
/// darum reichen Anfang und Ende je Linie.
fn grid_strokes(state: &AppState, view: &StrokeView, map: &Mapper, out: &mut Vec<StrokeVertex>) {
    let ppp = view.pixels_per_point;
    let frame = map.frame;
    let world = |p: DVec2| frame.point(p.x, p.y, 0.0);
    for line in sketch_grid_lines(state, map.rect) {
        push_stroke(
            out,
            view,
            &[world(line.from), world(line.to)],
            GRID_WIDTH * ppp,
            stroke_rgba(line.color),
            None,
        );
    }
}

/// Achsenkreuz im Skizzenursprung als GPU-Linien mit Tiefenprüfung, wie [`grid_strokes`]. Es
/// liegt in Fusion über den Profilflächen, darum zeichnet [`open_sketch_strokes`] es nach ihnen.
fn axis_strokes(state: &AppState, view: &StrokeView, map: &Mapper, out: &mut Vec<StrokeVertex>) {
    let ppp = view.pixels_per_point;
    let frame = map.frame;
    let world = |p: DVec2| frame.point(p.x, p.y, 0.0);
    // Achsenkreuz: zwei Striche von `AXIS_PX` Punkten Länge auf dem Bildschirm.
    let col = colors(state.dark);
    let Some(o) = map.to_screen(DVec2::ZERO) else {
        return;
    };
    for (dir, color) in [(DVec2::X, col.axis_x), (DVec2::Y, col.axis_y)] {
        // Länge in der Skizze so, dass der Strich `AXIS_PX` lang erscheint; zweimal nachgemessen,
        // weil die Perspektive nicht linear ist.
        let mut t = 1.0_f64;
        for _ in 0..3 {
            let Some(e) = map.to_screen(dir * t) else {
                break;
            };
            let len = f64::from((e - o).length());
            if !(len.is_finite() && len > 1e-9) {
                break;
            }
            t *= f64::from(AXIS_PX) / len;
        }
        if t.is_finite() && t > 0.0 {
            push_stroke(
                out,
                view,
                &[world(DVec2::ZERO), world(dir * t)],
                AXIS_WIDTH * ppp,
                stroke_rgba(color),
                None,
            );
        }
    }
}

/// Zeichnet die Skizze über die 3D-Ansicht in `rect` und verarbeitet die Eingaben des
/// Skizzen-Modus. In der Skizzenfläche selbst wird nicht gedreht; Verschieben und Zoomen gelten
/// dort nach der Tastenbelegung (Voreinstellung: mittlere Maustaste oder Umschalt + Ziehen,
/// Mausrad). Gedreht wird durch Ziehen am
/// Navigationswürfel (siehe `viewport::show_in`).
///
/// Gibt zurück, ob der Zeiger über der Skizzenfläche steht; das Mausrad-Zoomen macht der Aufrufer
/// (`viewport::show`) erst nach Würfel und Home-Knopf, damit es auch über ihnen wirkt.
pub fn show_overlay(ui: &mut egui::Ui, rect: Rect, state: &mut AppState) -> bool {
    if state.session_sketch().is_none() || rect.width() < 2.0 || rect.height() < 2.0 {
        return false;
    }
    let response = ui.interact(
        rect,
        ui.scope_id().with("planora-sketch-overlay"),
        Sense::click_and_drag(),
    );
    // Die Zeichenfläche nimmt den Fokus an (Tab, Pfeile), liest aber keinen Text: ihr Fokus
    // sperrt Esc, Entf und Eingabe der Skizze nicht.
    viewport::focus_is_not_typing(ui.ctx(), response.id);
    state.camera.set_viewport(rect.width(), rect.height());
    // Anzeigeschalter für Wählen und Fang dieses Bilds (Skizzen 12).
    state.sync_sketch_display();
    // Griffe eines Einfügen-Fensters nehmen den Zeiger vor den Werkzeugen, danach die Triade der
    // 3D-Skizze (Skizzen 10).
    if !insert_grips_input(ui, rect, state, response.hover_pos())
        && !kind_grips_input(ui, rect, state, response.hover_pos())
        && !crate::sketch_triad::input(ui, rect, state, response.hover_pos())
    {
        handle_input(ui, &response, rect, state);
    }
    let pointer = response.hover_pos().or(response.interact_pointer_pos());
    if crate::sketch_inline::show(ui, rect, state, pointer) {
        cursor_after_fields(rect, state, pointer);
    }
    // Was die Anzeige unter dem Zeiger heller zeigt, für Overlay und GPU-Linien gleich.
    let hover = hovered_item(state, rect, response.hover_pos());
    if let Some(se) = state.sketch_session.as_mut() {
        se.draw_hover = hover;
    }
    draw(ui, rect, state, response.hover_pos());
    show_curve_menu(ui, state);
    crate::sketch_triad::show(ui, state);
    response.hovered()
}

/// Rechtsklickmenü der Skizze (Skizzen 11a, 11b): „Kopieren“, „Ausschneiden“ und „Einfügen“ mit
/// ihren Tasten (Kopieren und Ausschneiden gedämpft ohne Auswahl, Einfügen gedämpft bei leerer
/// Zwischenablage), dann „Krümmungsanzeige ein/aus“, gedämpft, wenn nur Linien gewählt sind. Ein
/// Klick auf einen Punkt führt ihn aus (die Zwischenablage gehört dem Arbeitsbereich, der Eintrag
/// geht über [`AppState::clip_request`]); ein Linksklick daneben schließt das Menü (Esc und ein
/// neuer Rechtsklick über die Sitzung).
fn show_curve_menu(ui: &egui::Ui, state: &mut AppState) {
    let Some(at) = state.sketch_session.as_ref().and_then(|se| se.curve_menu) else {
        return;
    };
    let can = state
        .session_sketch()
        .zip(state.sketch_session.as_ref())
        .is_some_and(|(sk, se)| se.can_toggle_curvature(sk));
    let has_selection = state.sketch_session.as_ref().is_some_and(|se| {
        se.selection
            .iter()
            .any(|i| matches!(i, Item::Curve(_) | Item::Point(_)))
    });
    let has_clip = state
        .sketch_session
        .as_ref()
        .is_some_and(|se| se.clip_filled);
    let km = crate::keymap::mouse::current(ui.ctx());
    let mut run = false;
    let mut clip = None;
    let response = theme::field_area(
        egui::Id::unique("planora-sketch-curve-menu"),
        pos2(at.0, at.1),
    )
    .order(egui::Order::Foreground)
    .show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.scope(|ui| {
                theme::menu_style(ui.style_mut());
                for (action, label, enabled) in [
                    (Action::SketchCopy, tr!("action-sketch-copy"), has_selection),
                    (Action::SketchCut, tr!("action-sketch-cut"), has_selection),
                    (Action::SketchPaste, tr!("action-sketch-paste"), has_clip),
                ] {
                    let button = egui::Button::new(label).shortcut_text(
                        egui::RichText::new(crate::keymap::shortcut_hint(&km, action))
                            .font(theme::kbd_font()),
                    );
                    if ui.add_enabled(enabled, button).clicked() {
                        clip = Some(action);
                    }
                }
                ui.separator();
                let item = ui.add_enabled(
                    can,
                    egui::Button::new(tr!("action-sketch-toggle-curvature")),
                );
                if !can {
                    item.on_disabled_hover_text(tr!("sketch-comb-select-curve"));
                } else if item.clicked() {
                    run = true;
                }
            });
        });
    });
    let outside = ui.input(|i| i.pointer.primary_pressed())
        && ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| !response.response.rect.contains(p));
    if let Some(action) = clip {
        state.clip_request = Some(action);
        if let Some(se) = state.sketch_session.as_mut() {
            se.curve_menu = None;
        }
    } else if run {
        crate::curvature_dialog::toggle(state);
    } else if outside && let Some(se) = state.sketch_session.as_mut() {
        se.curve_menu = None;
    }
}

/// Rechtsklickmenü auf fertiger Skizzengeometrie in der 3D-Ansicht (Skizzen 12, Task 8) am Ort
/// [`AppState::sketch_pick_menu`]: „Skizze bearbeiten“ und „Ausblenden“ (Auge der Skizze) wie im
/// Baum, beide belegbar ([`Action::SketchEditPicked`], [`Action::SketchHidePicked`]). Ein
/// Linksklick daneben schließt das Menü, ebenso Esc.
pub fn show_sketch_pick_menu(ui: &egui::Ui, state: &mut AppState) {
    let Some(at) = state.sketch_pick_menu else {
        return;
    };
    if state.picked_sketch().is_none() {
        state.sketch_pick_menu = None;
        return;
    }
    let km = crate::keymap::mouse::current(ui.ctx());
    let mut chosen = None;
    let response = theme::field_area(
        egui::Id::unique("planora-sketch-pick-menu"),
        pos2(at.0, at.1),
    )
    .order(egui::Order::Foreground)
    .show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.scope(|ui| {
                theme::menu_style(ui.style_mut());
                let entry = |ui: &mut egui::Ui, action: Action, label: String| {
                    let keys = crate::keymap::shortcut_hint(&km, action);
                    let mut b = egui::Button::new(label);
                    if !keys.is_empty() {
                        b = b.shortcut_text(egui::RichText::new(keys).font(theme::kbd_font()));
                    }
                    ui.add(b).clicked().then_some(action)
                };
                chosen = chosen.or(entry(
                    ui,
                    Action::SketchEditPicked,
                    tr!("action-sketch-edit-picked"),
                ));
                // Haken „Maße anzeigen“ der fertigen Skizze (Task 9), danach „Ausblenden“.
                if let Some(on) = state
                    .picked_sketch()
                    .and_then(|id| state.sketch_dimension_menu(id))
                {
                    let mut checked = on;
                    if ui
                        .checkbox(&mut checked, tr!("tree-sketch-show-dimension"))
                        .clicked()
                    {
                        chosen = chosen.or(Some(Action::SketchShowDimensionFinished));
                    }
                }
                chosen = chosen.or(entry(
                    ui,
                    Action::SketchHidePicked,
                    tr!("action-sketch-hide-picked"),
                ));
            });
        });
    });
    let outside = ui.input(|i| i.pointer.primary_pressed())
        && ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| !response.response.rect.contains(p));
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    match chosen {
        Some(Action::SketchEditPicked) => state.edit_picked_sketch(),
        Some(Action::SketchHidePicked) => state.hide_picked_sketch(),
        Some(Action::SketchShowDimensionFinished) => {
            state.sketch_pick_menu = None;
            if let Some(id) = state.picked_sketch() {
                state.toggle_finished_dimensions(id);
            }
        }
        _ if outside || escape => state.sketch_pick_menu = None,
        _ => {}
    }
}

/// Die Abbildung auf die Skizze im Rechteck `rect` für dieses Bild.
pub(crate) fn mapper(rect: Rect, state: &AppState) -> Option<Mapper> {
    Some(Mapper {
        camera: state.camera,
        frame: *state.session_sketch()?.frame(),
        rect,
    })
}

/// Fangradius in Skizzeneinheiten am Zeiger `p`.
pub(crate) fn pointer_radius(state: &AppState, rect: Rect, p: Pos2) -> f64 {
    mapper(rect, state)
        .and_then(|map| map.radius_at(p))
        .unwrap_or(0.0)
}

/// Rohlage des aufgelösten Zeigers: `at`, ohne Zeiger über der Skizze mit einer Sperre die letzte
/// Lage, damit die Vorschau die Sperre weiter zeigt.
fn cursor_raw(se: &SketchSession, at: Option<DVec2>) -> Option<DVec2> {
    let locked = se.input.fields.iter().any(|f| f.locked.is_some());
    at.or(se.cursor.filter(|_| locked))
}

/// Aufgelöster Zeiger nach einer Änderung in den Feldern: Was eben getippt wurde, gilt schon in der
/// Vorschau dieses Bilds.
fn cursor_after_fields(rect: Rect, state: &mut AppState, pointer: Option<Pos2>) {
    let Some(map) = mapper(rect, state) else {
        return;
    };
    let at = pointer.and_then(|p| map.to_sketch(p));
    let radius = pointer.and_then(|p| map.radius_at(p)).unwrap_or(0.0);
    let Some((se, sk)) = state.session_sketch_mut() else {
        return;
    };
    se.sync_input(sk);
    if se.input.fields.is_empty() {
        return;
    }
    let raw = cursor_raw(se, at);
    se.update_cursor(sk, raw, radius);
    se.update_cursor3(sk, &map, pointer);
}

/// Bildschirmlage als Doppelvektor, wie ihn der Auswahlrahmen führt.
fn dvec(p: Pos2) -> DVec2 {
    DVec2::new(f64::from(p.x), f64::from(p.y))
}

/// Beginnt der eben erkannte Zug mit der linken Taste einen Rahmen? Nur im Auswahlwerkzeug und auf
/// freier Fläche (eine Lage außerhalb der Skizzenebene ist auch frei). Umschalt+links wählt dort
/// statt zu schwenken (Fusion schwenkt mit der Mitte); jede andere Schwenk-Belegung bleibt.
fn box_can_start(
    ui: &egui::Ui,
    rect: Rect,
    state: &AppState,
    pan_gesture: bool,
    ctrl: bool,
) -> bool {
    let (Some(se), Some(sk)) = (state.sketch_session.as_ref(), state.session_sketch()) else {
        return false;
    };
    let shift_only = ui.input(|i| i.modifiers.shift_only()) && !ctrl;
    if (se.tool != Tool::Select && !se.transform_picking())
        || (pan_gesture && !shift_only)
        || crate::keymap::settings_ui::is_capturing(ui.ctx())
    {
        return false;
    }
    let Some(map) = mapper(rect, state) else {
        return false;
    };
    // Ein Griff geht vor dem Rahmen.
    if let Some(p) = ui.input(|i| i.pointer.press_origin())
        && grip_hit(state, rect, p).is_some()
    {
        return false;
    }
    ui.input(|i| i.pointer.press_origin()).is_some_and(|p| {
        // 3D-Geometrie unter dem Zeiger (Bildschirmabstand) ist nicht frei.
        pick_3d(se, sk, &map, p).is_none()
            && match (map.to_sketch(p), map.radius_at(p)) {
                (Some(at), Some(radius)) => se.is_free_at(sk, at, radius),
                _ => true,
            }
    })
}

/// Lage der Griffe in diesem Bild; `None`, wenn keine zu sehen sind oder die Ansicht ihre Mitte nicht
/// zeigt. Die Pfeile folgen dem Bild der Skizzenachsen an der Mitte, also auch auf gekippten Ansichten.
pub fn grip_layout(state: &AppState, rect: Rect) -> Option<GripLayout> {
    let se = state.sketch_session.as_ref()?;
    if !se.grips_shown() {
        return None;
    }
    let now = se.transform_now()?;
    let map = mapper(rect, state)?;
    // Mit „3D-Skizze“ liegt das ganze Griffkreuz an der Mitte im Raum (Fix-Runde 1 Task 20).
    if se.tool == Tool::MoveCopy && state.session_sketch()?.three_d {
        let c = se.grip_center3()?;
        let center = map.to_screen3(c)?;
        let step = crate::sketch_triad::units_per_px_at(&map, c)?;
        let dir = |axis: DVec3| {
            map.to_screen3(c + axis * step)
                .map(|q| (q - center).normalized())
        };
        return Some(sketch_grips::layout(
            center,
            dir(DVec3::X)?,
            dir(DVec3::Y)?,
            false,
        ));
    }
    let at = now.base + now.offset;
    let center = map.to_screen(at)?;
    // Ein kleiner Schritt, der auf dem Bildschirm etwa so lang ist wie der Fangradius: Eine feste
    // Länge läge bei starkem Zoom hinter der Kamera.
    let step = map.radius_at(center).filter(|r| *r > 0.0)?;
    let dir = |axis: DVec2| {
        map.to_screen(at + axis * step)
            .map(|q| (q - center).normalized())
    };
    Some(sketch_grips::layout(
        center,
        dir(DVec2::X)?,
        dir(DVec2::Y)?,
        se.tool == Tool::Scale,
    ))
}

/// Lage der Griffe für 3D (Z-Pfeil, Drehbögen um X und Y) in diesem Bild: nur bei
/// Verschieben/Kopieren mit „3D-Skizze“ (Skizzen 10, A1), an der Mitte der Griffe im Raum.
pub fn grip_layout3(state: &AppState, rect: Rect) -> Option<sketch_grips::GripLayout3> {
    let se = state.sketch_session.as_ref()?;
    let sk = state.session_sketch()?;
    if !se.grips_shown() || se.tool != Tool::MoveCopy || !sk.three_d {
        return None;
    }
    let map = mapper(rect, state)?;
    sketch_grips::layout3(&map, se.grip_center3()?, se.grip_arc_axes())
}

/// Der Griff von Verschieben/Kopieren und Maßstab unter `p`, samt den Griffen für 3D.
pub fn grip_hit(state: &AppState, rect: Rect, p: Pos2) -> Option<Grip> {
    let l = grip_layout(state, rect)?;
    sketch_grips::hit_any(&l, grip_layout3(state, rect).as_ref(), p)
}

/// Lage der Griffe eines Einfügen-Fensters ([`AppState::insert_grips`]) in diesem Bild: die
/// Griffe zum Verschieben und Drehen und die Handhabe des Maßstabs schräg unten rechts zwischen
/// den Pfeilen. `None`, wenn das Fenster keine Griffe hat oder die Ansicht ihre Mitte nicht zeigt.
pub fn insert_grip_layout(state: &AppState, rect: Rect) -> Option<(GripLayout, GripLayout)> {
    let g = state.insert_grips.as_ref()?;
    let map = Mapper {
        camera: state.camera,
        frame: g.frame,
        rect,
    };
    insert_layout(&map, g.offset)
}

fn insert_layout(map: &Mapper, at: DVec2) -> Option<(GripLayout, GripLayout)> {
    let center = map.to_screen(at)?;
    let step = map.radius_at(center).filter(|r| *r > 0.0)?;
    let dir = |axis: DVec2| {
        map.to_screen(at + axis * step)
            .map(|q| (q - center).normalized())
            .filter(|v| v.length() > 0.5)
    };
    let (x, y) = (dir(DVec2::X)?, dir(DVec2::Y)?);
    let diagonal = (x - y).normalized();
    Some((
        sketch_grips::layout(center, x, y, false),
        sketch_grips::layout(center, diagonal, y, true),
    ))
}

/// Eingaben an den Griffen eines Einfügen-Fensters (Modell und Skizze): Griff unter dem Zeiger
/// (`hover`, nur über der Ansicht), Drücken beginnt einen Zug, Ziehen schreibt Verschiebung,
/// Winkel und Faktor in [`AppState::insert_grips`] (das Fenster übernimmt sie), Loslassen endet.
/// `true`, wenn die Griffe den Zeiger in diesem Bild nehmen; dann wählt die Ansicht nichts.
pub fn insert_grips_input(
    ui: &egui::Ui,
    rect: Rect,
    state: &mut AppState,
    hover: Option<Pos2>,
) -> bool {
    let camera = state.camera;
    let Some(g) = state.insert_grips.as_mut() else {
        return false;
    };
    let map = Mapper {
        camera,
        frame: g.frame,
        rect,
    };
    let (down, pressed, latest, origin) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.primary_pressed(),
            i.pointer.latest_pos(),
            i.pointer.press_origin(),
        )
    });
    if let Some(d) = g.drag {
        // Wie bei den Griffen von Verschieben/Kopieren: Rechtsklick oder Esc nimmt nur den Zug
        // zurück, das Fenster bleibt offen (hat ein Feld die Tastatur, gilt Esc dort).
        let (secondary, esc) = ui.input(|i| {
            (
                i.pointer.button_pressed(PointerButton::Secondary),
                i.key_pressed(egui::Key::Escape),
            )
        });
        let esc = esc && !viewport::typing(ui.ctx());
        if secondary || esc {
            (g.offset, g.angle, g.factor) = (d.start_offset, d.start_angle, d.start_factor);
            g.moved = true;
            g.drag = None;
            g.esc_taken = esc;
            return true;
        }
        if let Some(now) = latest.and_then(|p| map.to_sketch(p)) {
            let (offset, raw, factor) = sketch_grips::grip_motion(&d, d.start_offset, now);
            let angle = sketch_grips::unwrapped_angle(g.angle, raw);
            if (offset, angle, factor) != (g.offset, g.angle, g.factor) {
                (g.offset, g.angle, g.factor) = (offset, angle, factor);
                g.moved = true;
            }
        }
        if !down {
            g.drag = None;
        }
        return true;
    }
    let Some((main, scale)) = insert_layout(&map, g.offset) else {
        g.hot = None;
        return false;
    };
    let hot =
        hover.and_then(|p| sketch_grips::hit(&scale, p).or_else(|| sketch_grips::hit(&main, p)));
    g.hot = hot;
    if pressed
        && let Some(grip) = hot
        && let Some(press) = origin.or(hover).and_then(|p| map.to_sketch(p))
    {
        g.drag = Some(sketch_grips::GripDrag {
            grip,
            press,
            start_offset: g.offset,
            start_angle: g.angle,
            start_factor: g.factor,
        });
    }
    hot.is_some()
}

/// Halbmesser der Griffe von Rundung und Fase in Pixeln (Treffer etwas großzügiger).
const KIND_GRIP_PX: f32 = 5.0;
const KIND_GRIP_HIT_PX: f32 = 9.0;

/// Ab dieser Bewegung (Pixel) wird aus einem Druck auf einen Griff ein Zug; ein Klick ohne
/// Bewegung gehört dem Werkzeug (legt die Rundung oder Fase an).
const KIND_GRIP_START_PX: f32 = 4.0;

/// Eingaben an den Griffen von Rundung und Fase (wie Fusion): Ein Druck auf einen Griff wird
/// zum Zug, sobald sich der Zeiger bewegt; Ziehen schreibt den Wert ins Feld (wie getippt),
/// Loslassen endet. Esc oder Rechtsklick nimmt nur den Zug zurück (das Werkzeug bleibt in seiner
/// Stufe). `true`, solange ein Zug läuft; dann setzt das Werkzeug nichts. Ein Klick ohne
/// Bewegung und bloßes Schweben bleiben beim Werkzeug.
pub fn kind_grips_input(
    ui: &egui::Ui,
    rect: Rect,
    state: &mut AppState,
    hover: Option<Pos2>,
) -> bool {
    let busy = state.sketch_session.as_ref().is_some_and(|se| {
        se.kind_grip.drag.is_some() || se.planned_kind().is_some_and(|k| k.is_fillet_or_chamfer())
    });
    if !busy {
        if let Some(se) = state.sketch_session.as_mut() {
            se.kind_grip.hot = None;
            se.kind_grip.over = None;
        }
        return false;
    }
    let Some(map) = mapper(rect, state) else {
        return false;
    };
    let scope = state.document.param_scope();
    let Some(sk) = state.session_sketch().cloned() else {
        return false;
    };
    let Some(se) = state.sketch_session.as_mut() else {
        return false;
    };
    let grips = se.kind_grips(&sk);
    se.kind_grip.over = se.too_large_field(&sk);
    let (down, pressed, latest) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.primary_pressed(),
            i.pointer.latest_pos(),
        )
    });
    if let Some(mut d) = se.kind_grip.drag.clone() {
        // Der Griff ist weg (andere Stufe, andere Kurven): Der Zug endet ohne Wirkung.
        if !grips.iter().any(|g| g.field == d.grip.field) {
            se.kind_grip.drag = None;
            return false;
        }
        if !d.started {
            let moved = latest
                .is_some_and(|p| p.distance(pos2(d.press.0, d.press.1)) >= KIND_GRIP_START_PX);
            if !down {
                // Nur geklickt: Der Klick gehört dem Werkzeug.
                se.kind_grip.drag = None;
                return false;
            }
            if !moved {
                return false;
            }
            d.started = true;
            // Was egui jetzt als Zug sieht, ist kein Auswahlrahmen und kein Klick.
            se.box_aborted = true;
            se.kind_grip.drag = Some(d.clone());
        }
        let (secondary, esc) = ui.input(|i| {
            (
                i.pointer.button_pressed(PointerButton::Secondary),
                i.key_pressed(egui::Key::Escape),
            )
        });
        // Esc gilt dem Zug, auch wenn ein Feld am Zeiger die Tastatur hat (es gehört demselben
        // Werkzeug).
        if secondary || esc {
            if let Some(before) = d.before {
                se.restore_field(before);
            }
            se.kind_grip.drag = None;
            // Nur der Zug geht zurück: Esc sehen Werkzeug und Felder nicht, der Rechtsklick
            // öffnet nichts und bricht die Stufe nicht ab.
            se.box_aborted = true;
            if esc {
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            }
            return true;
        }
        if let Some(p) = latest.and_then(|p| map.to_sketch(p)) {
            // Über die Grenze hinaus bleibt der größte passende Wert (die Vorschau bleibt).
            let v = se.fit_grip_value(&sk, d.grip.field, d.grip.value, d.grip.value_at(p));
            se.set_grip_value(d.grip.field, v, &scope);
            se.kind_grip.over = None;
        }
        if !down {
            se.kind_grip.drag = None;
        }
        return true;
    }
    let hot = hover.and_then(|h| {
        grips.iter().copied().find(|g| {
            map.to_screen(g.at)
                .is_some_and(|s| s.distance(h) <= KIND_GRIP_HIT_PX)
        })
    });
    se.kind_grip.hot = hot.map(|g| g.field);
    if pressed
        && let Some(grip) = hot
        && let Some(at) = latest.or(hover)
    {
        se.kind_grip.drag = Some(crate::sketch_kinds::KindGripDrag {
            before: se.field_state(grip.field),
            grip,
            press: (at.x, at.y),
            started: false,
        });
    }
    false
}

/// Zeichnet die Griffe von Rundung und Fase über der Vorschau.
pub fn paint_kind_grips(painter: &egui::Painter, rect: Rect, state: &AppState) {
    let (Some(se), Some(sk), Some(map)) = (
        state.sketch_session.as_ref(),
        state.session_sketch(),
        mapper(rect, state),
    ) else {
        return;
    };
    let col = colors(state.dark);
    let active = se
        .kind_grip
        .drag
        .as_ref()
        .map(|d| d.grip.field)
        .or(se.kind_grip.hot);
    for g in se.kind_grips(sk) {
        let Some(p) = map.to_screen(g.at) else {
            continue;
        };
        let fill = if active == Some(g.field) {
            lighter(col.selected)
        } else {
            col.selected
        };
        painter.circle_filled(p, KIND_GRIP_PX, fill);
    }
}

/// Zeichnet die Griffe eines Einfügen-Fensters über der Ansicht.
pub fn paint_insert_grips(painter: &egui::Painter, rect: Rect, state: &AppState) {
    let (Some(g), Some((main, scale))) =
        (state.insert_grips.as_ref(), insert_grip_layout(state, rect))
    else {
        return;
    };
    let col = colors(state.dark);
    let hot = g.drag.map(|d| d.grip).or(g.hot);
    let (color, hot_color) = (col.selected, lighter(col.selected));
    sketch_grips::paint(painter, &main, hot, color, hot_color);
    sketch_grips::paint(painter, &scale, hot, color, hot_color);
}

/// Lage der Griffe der Anordnung in diesem Bild (Pfeil je Richtung am letzten Exemplar, beim
/// Kreismuster der Winkelgriff am Ende des Bogens); `None`, wenn keine zu sehen sind.
pub fn pattern_grip_layout(state: &AppState, rect: Rect) -> Option<PatternGripLayout> {
    let (se, sk) = (state.sketch_session.as_ref()?, state.session_sketch()?);
    let map = mapper(rect, state)?;
    // Kreismuster: der Winkelgriff am Ende des Bogens.
    if let Some((center, tip)) = se.pattern_angle_handle(sk) {
        let tip = map.to_screen(tip)?;
        return Some(sketch_pattern::pattern_layout(
            map.to_screen(center)?,
            [None; 2],
            Some(tip),
        ));
    }
    let handles = se.pattern_handles(sk)?;
    let origin = map.to_screen(handles.origin)?;
    let step = map.radius_at(origin).filter(|r| *r > 0.0)?;
    let tips = handles.arrows.map(|a| {
        let (at, out) = a?;
        sketch_pattern::screen_arrow(|q| map.to_screen(q), at, out, step)
    });
    Some(sketch_pattern::pattern_layout(origin, tips, None))
}

/// Fester Platz der Felder der Anordnung in diesem Bild: der Ankerpunkt auf dem Bildschirm und die
/// Bildschirmrichtung, in die die Felder von ihm weg stehen. Er folgt Schwenken und Zoomen, nicht
/// der Vorschau; `None` ohne laufende Anordnung.
pub fn pattern_field_place(state: &AppState, rect: Rect) -> Option<(Pos2, egui::Vec2)> {
    let (se, sk) = (state.sketch_session.as_ref()?, state.session_sketch()?);
    let anchor = se.pattern_field_anchor(sk)?;
    let map = mapper(rect, state)?;
    let at = map.to_screen(anchor.at)?;
    let step = map.radius_at(at).filter(|r| *r > 0.0)?;
    let away = map.to_screen(anchor.at + anchor.away * step)? - at;
    (away.length() > 0.0).then_some((at, away))
}

/// Der Griff der Anordnung, den der eben erkannte Zug mit der linken Taste greift (`layout`: die
/// Lage der Griffe in diesem Bild).
fn pattern_grip_can_start(
    ui: &egui::Ui,
    layout: Option<&PatternGripLayout>,
    pan_gesture: bool,
    ctrl: bool,
) -> Option<PatternGrip> {
    let shift_only = ui.input(|i| i.modifiers.shift_only()) && !ctrl;
    if (pan_gesture && !shift_only) || crate::keymap::settings_ui::is_capturing(ui.ctx()) {
        return None;
    }
    sketch_pattern::pattern_hit(layout?, ui.input(|i| i.pointer.press_origin())?)
}

/// Der Griff, den der eben erkannte Zug mit der linken Taste greift. Wie beim Rahmen zählt nur die
/// Lage des Drückens, und Umschalt+links greift statt zu schwenken; jede andere Schwenk-Belegung bleibt.
fn grip_can_start(
    ui: &egui::Ui,
    rect: Rect,
    state: &AppState,
    pan_gesture: bool,
    ctrl: bool,
) -> Option<Grip> {
    let shift_only = ui.input(|i| i.modifiers.shift_only()) && !ctrl;
    if (pan_gesture && !shift_only) || crate::keymap::settings_ui::is_capturing(ui.ctx()) {
        return None;
    }
    grip_hit(state, rect, ui.input(|i| i.pointer.press_origin())?)
}

/// Der laufende Auswahlrahmen: Fläche und Rand; Fenster durchgezogen, Kreuzung gestrichelt.
pub fn box_select_shapes(state: &AppState) -> Vec<Shape> {
    let Some(b) = state.sketch_session.as_ref().and_then(|se| se.box_select) else {
        return Vec::new();
    };
    let col = colors(state.dark);
    let r = Rect::from_two_pos(
        pos2(b.start.x as f32, b.start.y as f32),
        pos2(b.current.x as f32, b.current.y as f32),
    );
    let corners = vec![
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    let mut shapes = vec![Shape::rect_filled(r, 0.0, col.box_fill)];
    shapes.extend(path_shapes(
        corners,
        Stroke::new(1.0, col.selected),
        !b.is_window(),
    ));
    shapes
}

fn handle_input(ui: &egui::Ui, response: &egui::Response, rect: Rect, state: &mut AppState) {
    // Umschalt zählt auch, wenn sie nur beim Klick selbst gedrückt war (Ereignis und Loslassen der
    // Taste können im selben Frame ankommen).
    let shift = ui.input(|i| {
        i.modifiers.shift
            || i.events.iter().any(
                |e| matches!(e, egui::Event::PointerButton { modifiers, .. } if modifiers.shift),
            )
    });
    // Strg ebenso: Sie schaltet Fang, Ausrichtung, Raster und Automatik aus (wie Fusion).
    let ctrl = ui.input(|i| {
        i.modifiers.ctrl
            || i.events.iter().any(
                |e| matches!(e, egui::Event::PointerButton { modifiers, .. } if modifiers.ctrl),
            )
    });
    if state.session_sketch().is_none() {
        return;
    }
    // Offene Liste der Arten (die Leiste kommt in diesem Bild erst nach der Ansicht): Esc und
    // Eingabe gehören ihr, ein Drücken schließt nur sie und setzt weder Punkt noch Zug.
    let flyout = crate::sketch_panel::flyout_open(ui.ctx());
    if flyout
        && ui.input(|i| i.pointer.any_pressed())
        && let Some(se) = state.sketch_session.as_mut()
    {
        se.box_aborted = true;
        se.click_after_flyout = true;
    }
    let aborted = state.sketch_session.as_ref().is_some_and(|s| s.box_aborted);

    // Navigation: Verschieben und Zoomen; kein Orbit.
    let dragging = state
        .sketch_session
        .as_ref()
        .is_some_and(SketchSession::is_dragging);
    // Schwenken nach der Tastenbelegung (Voreinstellung: Mitte oder Umschalt+links, außer auf freier
    // Fläche im Auswahlwerkzeug, dort zieht Umschalt+links einen Rahmen); nie
    // während gezogen wird. Die linke Taste ohne Umschalttasten bleibt dem Zeichnen.
    // Ein Zug, der als Schwenken beginnt (z. B. Schwenken auf Strg+links umgelegt), zieht keinen
    // Punkt.
    let km = crate::keymap::mouse::current(ui.ctx());
    let pan_gesture = crate::keymap::mouse::pan_2d(&km, ui, response);
    let primary_start = response.drag_started_by(PointerButton::Primary) && !aborted;
    let primary_click = response.clicked_by(PointerButton::Primary);
    // Die Griffe der Anordnung einmal je Bild und nur, wenn gedrückt oder geklickt wurde: vor jeder
    // Änderung durch die Eingabe. Ein Zug, der schwenkt, ist kein Klick; ein gegriffener Griff
    // schwenkt nicht. So gilt die Lage für den Zugbeginn und den Klick gleichermaßen.
    let pattern_layout = (primary_start || primary_click)
        .then(|| pattern_grip_layout(state, rect))
        .flatten();
    let starts_box = primary_start && box_can_start(ui, rect, state, pan_gesture, ctrl);
    let starts_grip = primary_start
        .then(|| grip_can_start(ui, rect, state, pan_gesture, ctrl))
        .flatten();
    let starts_pattern_grip = primary_start
        .then(|| pattern_grip_can_start(ui, pattern_layout.as_ref(), pan_gesture, ctrl))
        .flatten();
    // Ein laufender Rahmen schwenkt nicht, auch wenn Umschalt erst währenddessen dazukommt; ein
    // Zug auf dem letzten Punkt des Linienzugs ebenso. Beim Griff schwenkt die Mitte trotzdem.
    let gripping = starts_grip.is_some()
        || starts_pattern_grip.is_some()
        || state
            .sketch_session
            .as_ref()
            .is_some_and(|s| s.grip_drag.is_some() || s.pattern.drag.is_some());
    let boxing = starts_box
        || state
            .sketch_session
            .as_ref()
            .is_some_and(|s| s.box_select.is_some() || s.line_arc.is_some());
    let middle = response.dragged_by(PointerButton::Middle);
    let panning = pan_gesture && !boxing && (!gripping || middle);
    if panning && !dragging {
        let d = response.drag_motion();
        state.camera.pan(d.x, d.y, rect.height());
    }

    let Some(map) = mapper(rect, state) else {
        return;
    };
    let pointer = response.hover_pos().or(response.interact_pointer_pos());
    let at = pointer.and_then(|p| map.to_sketch(p));
    let radius = pointer.and_then(|p| map.radius_at(p)).unwrap_or(0.0);

    // Raster-Fang auf der gezeichneten Stufe; ohne Raster oder ohne „Fang“ keiner.
    let grid_step = if state.sketch_show_grid && state.sketch_snap_grid {
        GridPlan::of(&map).map(|plan| plan.snap_step())
    } else {
        None
    };

    // Ein Klick auf einen Griff schaltet beim Wählen nichts in der Auswahl.
    let click_on_grip = primary_click
        && (pointer.is_some_and(|p| grip_hit(state, rect, p).is_some())
            || pointer
                .zip(pattern_layout.as_ref())
                .is_some_and(|(p, l)| sketch_pattern::pattern_hit(l, p).is_some()));
    // Ein Klick auf ein Feld der Anordnung (die Felder nehmen keine Mausereignisse an) schaltet
    // kein Exemplar darunter (Handtest Skizzen 5). Die anderen Befehle führen ihre Felder neben
    // dem Zeiger: Sie nehmen keinen Klick an, er wirkt auf die Zeichnung (Skizzen 11a, A9, Fusion).
    let click_on_field = (primary_click || response.double_clicked() || response.triple_clicked())
        && state.sketch_session.as_ref().is_some_and(|se| {
            se.runs_pattern()
                && pointer.is_some_and(|p| crate::sketch_inline::over_field(ui.ctx(), se, p))
        });

    // Getippte Werte werden beim Klick und bei der Eingabe zu Maßen; die Formeln darin brauchen die
    // Parameter.
    let commit_scope = ((response.clicked_by(PointerButton::Primary)
        || ui.input(|i| i.key_pressed(egui::Key::Enter)))
        && state
            .sketch_session
            .as_ref()
            .is_some_and(SketchSession::is_drawing_tool))
    .then(|| state.document.param_scope());
    // Text: OK über Eingabe setzt mit der Schriftliste.
    let fonts = state.fonts().cloned();
    let dialog_takes_esc = state.insert_dialog_open;
    let Some((se, sf)) = state.session_draft_mut() else {
        return;
    };
    if let Some(at) = at {
        se.cursor = Some(at);
    }
    se.cursor_opts.suppressed = ctrl;
    se.cursor_opts.grid_step = grid_step;
    // Zeichnen in 3D (Skizzen 10): Der Klick setzt dort, wo der Zeiger jetzt steht.
    se.update_cursor3(&sf.sketch, &map, pointer);
    let draws3 = se.draw3;
    // 3D-Geometrie unter dem Zeiger über den Bildschirmabstand (Skizzen 10), nur wo kein
    // 2D-Punkt fängt: für Bemaßung und Abhängigkeitsbefehl Punkte vor Kurven, für das Zeichnen in
    // der Ebene nur Punkte (der Fang-Index kennt keine 3D-Geometrie).
    let picks_3d = matches!(se.tool, Tool::Dimension | Tool::Constraint(_));
    let draws_flat = se.is_drawing_tool() && !draws3;
    se.pick3 = if picks_3d || draws_flat {
        let flat_point =
            at.is_some_and(|a| matches!(se.snap(&sf.sketch, a, radius), Snap::Point(_)));
        pointer
            .filter(|_| !flat_point)
            .and_then(|p| {
                // Bemaßung und Abhängigkeitsbefehl wählen (ohne Punkte bei „Punkte anzeigen“
                // aus); das Zeichnen in der Ebene fängt 3D-Punkte immer (Skizzen 12).
                if picks_3d {
                    pick_3d(se, &sf.sketch, &map, p)
                } else {
                    pick_3d_for_snap(se, &sf.sketch, &map, p)
                }
            })
            .filter(|&i| picks_3d || matches!(i, Item::Point(_)))
    } else {
        None
    };

    // Rahmen auf freier Fläche; alles andere greift (Punkt vor Kurve vor Rahmen).
    if starts_box {
        if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
            se.begin_box(dvec(origin), shift);
        }
    } else if let Some(grip) = starts_grip {
        let origin = ui.input(|i| i.pointer.press_origin());
        if grip.is_3d() {
            // Griffe für 3D fassen eine Stelle im Raum (Skizzen 10).
            let arcs = se.grip_arc_axes();
            let at3 = origin
                .zip(se.grip_center3())
                .and_then(|(p, c)| sketch_grips::grip_point3(&map, grip, c, arcs, p));
            if let Some(at3) = at3 {
                se.begin_grip3(grip, at3);
            }
        } else if let Some(start) = origin.and_then(|p| map.to_sketch(p)) {
            se.begin_grip(grip, start);
        }
    } else if let Some(grip) = starts_pattern_grip {
        let origin = ui.input(|i| i.pointer.press_origin());
        if let Some(start) = origin.and_then(|p| map.to_sketch(p)) {
            se.begin_pattern_grip(&sf.sketch, grip, start);
        }
    } else if response.drag_started_by(PointerButton::Primary) && !shift && !panning && !aborted {
        // Punkt oder Kurve ziehen (Auswahlwerkzeug), tangentialer Bogen (Linienwerkzeug)
        let origin = ui.input(|i| i.pointer.press_origin());
        // 3D-Geometrie zieht in der Ebene parallel zum Bildschirm durch den gefassten Punkt
        // (Skizzen 10, auch wo der Sehstrahl die Skizzenebene nicht trifft).
        let grab3 = origin
            .filter(|_| se.tool == Tool::Select)
            .and_then(|p| grab_3d(se, &sf.sketch, &map, p));
        if let Some((item, grab)) = grab3
            && let Some(normal) = map.view_normal()
        {
            se.begin_drag3(&sf.sketch, item, grab, normal);
        } else if let Some(start) = origin.and_then(|p| map.to_sketch(p)) {
            let r = origin.and_then(|p| map.radius_at(p)).unwrap_or(radius);
            se.begin_drag(&mut sf.sketch, start, r);
            // Der Bogen aus dem Linienzug zeichnet nur in der Ebene.
            if !draws3 {
                se.begin_line_arc(&sf.sketch, start, r);
            }
        }
    }
    // Ein offenes Einfügen-Fenster (DXF, SVG) nimmt Esc selbst; die Skizze sieht sie nicht.
    let esc = !flyout
        && !dialog_takes_esc
        && !viewport::typing(ui.ctx())
        && ui.input(|i| i.key_pressed(egui::Key::Escape));
    // Griffzug: Die rechte Taste und Esc nehmen ihn ganz zurück. Das steht vor dem Ende des Zugs, denn
    // Esc beendet schon hier den Zug von egui, und das hieße Annehmen. Die Taste gilt damit als
    // verbraucht: Sie leert nicht auch noch die Auswahl.
    let handle_aborted = se.handle_drag_running()
        && (esc || ui.input(|i| i.pointer.button_pressed(PointerButton::Secondary)))
        && se.abort_handle_drag(&mut sf.sketch);
    let esc = esc && !handle_aborted;
    if handle_aborted {
        ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    }
    if se.is_dragging() {
        if response.dragged_by(PointerButton::Primary) {
            if let Some(d) = &se.drag3 {
                let (grab, normal) = (d.grab, d.normal);
                if let Some(target) = pointer.and_then(|p| map.screen_to_plane(p, grab, normal)) {
                    se.drag_to3(&mut sf.sketch, target);
                }
            } else if let Some(at) = at {
                se.drag_to(&mut sf.sketch, at);
            }
        }
        if response.drag_stopped() || !response.dragged() {
            se.end_drag(&mut sf.sketch);
        }
    }

    if se.box_select.is_some() {
        // Die rechte Taste bricht schon beim Drücken ab: Ein Klick wird es während des Zugs nicht
        // immer. Esc beendet schon hier den Zug von egui, der Rahmen fällt aber unten weg.
        if ui.input(|i| i.pointer.button_pressed(PointerButton::Secondary)) {
            se.abort_box();
        } else if !esc {
            if let Some(p) = ui.input(|i| i.pointer.latest_pos()) {
                se.box_to(dvec(p));
            }
            if response.drag_stopped() || !response.dragged() {
                se.end_box3(&sf.sketch, &|q| map.to_screen3(q).map(dvec));
            }
        }
    }

    // Zug auf dem letzten Punkt: wie beim Rahmen bricht die rechte Taste schon beim Drücken ab, und
    // Esc beendet schon hier den Zug von egui, der Bogenzug fällt aber unten weg.
    if se.line_arc.is_some() {
        if ui.input(|i| i.pointer.button_pressed(PointerButton::Secondary)) {
            se.abort_box();
        } else if !esc && response.drag_stopped() {
            match at.or(se.cursor) {
                Some(end) => {
                    let _ = se.finish_line_arc(&mut sf.sketch, end, radius);
                }
                None => se.line_arc = None,
            }
        } else if !esc && !response.dragged() {
            se.line_arc = None;
        }
    }

    // Griff: Die Skizze bleibt unberührt, der Zug führt nur `transform`. Wie beim Rahmen bricht die
    // rechte Taste schon beim Drücken ab, und Esc beendet schon hier den Zug von egui, der Griff fällt
    // aber unten weg; hat ein Feld die Tastatur, dort in den Feldern.
    if se.grip_drag.is_some() {
        if ui.input(|i| i.pointer.button_pressed(PointerButton::Secondary)) {
            se.abort_grip();
        } else if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            let latest = ui.input(|i| i.pointer.latest_pos());
            let grip = se.grip_drag.map(|d| d.grip);
            match grip {
                Some(g) if g.is_3d() => {
                    // Die Ebene des Bogens wie beim Fassen (sie hängt nicht am eigenen Winkel).
                    let arcs = se.grip_arc_axes();
                    let at3 = latest
                        .zip(se.grip_center3())
                        .and_then(|(p, c)| sketch_grips::grip_point3(&map, g, c, arcs, p));
                    if let Some(at3) = at3 {
                        se.grip_to3(at3);
                    }
                }
                _ => {
                    if let Some(p) = latest.and_then(|p| map.to_sketch(p)) {
                        se.grip_to(p);
                    }
                }
            }
            if response.drag_stopped() || !response.dragged() {
                se.end_grip();
            }
        }
    }

    // Griff der Anordnung: Der Zug führt nur den Plan; Rechtsklick nimmt ihn zurück wie oben.
    if se.pattern.drag.is_some() {
        if ui.input(|i| i.pointer.button_pressed(PointerButton::Secondary)) {
            se.abort_pattern_grip();
        } else if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            let latest = ui.input(|i| i.pointer.latest_pos());
            if let Some(p) = latest.and_then(|p| map.to_sketch(p)) {
                se.pattern_grip_to(&sf.sketch, p);
            }
            if response.drag_stopped() || !response.dragged() {
                se.end_pattern_grip();
            }
        }
    }

    // Stutzen durch Ziehen: jede überfahrene Kurve wird gestutzt (ein Schritt, da der Verlauf
    // erst nach dem Loslassen fortgeschrieben wird).
    if se.tool == Tool::Trim && !panning {
        if response.drag_started_by(PointerButton::Primary) && !shift && !aborted {
            let origin = ui.input(|i| i.pointer.press_origin());
            if let Some(start) = origin.and_then(|p| map.to_sketch(p)) {
                let r = origin.and_then(|p| map.radius_at(p)).unwrap_or(radius);
                se.begin_trim_drag(&mut sf.sketch, start, r);
            }
        }
        if se.trim_drag.is_some()
            && response.dragged_by(PointerButton::Primary)
            && let Some(at) = at
        {
            se.trim_drag_to(&mut sf.sketch, at, radius);
        }
    }
    if se.trim_drag.is_some() && (response.drag_stopped() || !response.dragged()) {
        se.end_trim_drag();
    }

    // Klicks
    let mut project_at = None;
    // Klick der Wahl „Neu verknüpfen“ (Skizzen 11a).
    let mut relink_at = None;
    // Doppelklick auf einen Maßtext im Auswahlwerkzeug: öffnet sein Feld wie die Liste (das Feld
    // braucht den ganzen Stand und öffnet darum erst nach der Eingabe dieses Bilds).
    let mut open_label = None;
    // egui zählt Klicks nur als Doppel-/Dreifachklick, wenn sie höchstens `max_click_dist`
    // (6 Pixel) vom vorigen Klick entfernt sind. Ein „Dreifachklick“ entsteht schon, wenn zwei
    // schnelle Klicks an derselben Stelle einem Klick anderswo folgen (Zeit zum vorletzten Klick,
    // Abstand nur zum letzten) – das ist für den Nutzer ein Doppelklick und beendet ebenso.
    if se.box_aborted {
        // Das Loslassen nach dem Abbruch ist kein Klick.
    } else if click_on_field {
        // Der Klick gehört dem Feld der Anordnung.
    } else if se.relink.is_some() && response.clicked_by(PointerButton::Primary) {
        // „Neu verknüpfen“ (Skizzen 11a): Der Klick wählt die neue Herkunft in der 3D-Szene wie
        // „Projizieren“ (Ruling F9), auch wo der Sehstrahl die Skizzenebene nicht trifft.
        relink_at = pointer;
    } else if (response.double_clicked() || response.triple_clicked()) && !se.click_after_flyout {
        let label = pointer
            .filter(|_| se.tool == Tool::Select)
            .and_then(|p| label_at(ui.ctx(), se.sketch, p))
            .filter(|&id| {
                sf.sketch
                    .constraint(id)
                    .is_some_and(|c| c.kind.is_dimension())
            });
        if let Some(id) = label {
            // Wie ein Doppelklick in der Liste: nur dieses Maß gewählt, ohne Klicklage auf einer
            // Kurve (Skizzen 5, Task 7: eine Klicklage gehört nur zu ihrer Auswahl).
            se.drop_curve_picks();
            se.selection = vec![Item::Constraint(id)];
            open_label = Some(id);
        } else if se.tool == Tool::Select || se.transform_picking() {
            // Wie beim einfachen Klick: Punkt vor Kurve. Ein Exemplar oder eine Vorlage eines
            // Musters öffnet im Auswahlwerkzeug das Muster zum Bearbeiten (Skizzen 5).
            if let Some(at) = at {
                let mut snap = se.snap(&sf.sketch, at, radius);
                // 3D-Kurven trifft nur der Bildschirmabstand (Skizzen 10).
                if matches!(snap, Snap::None)
                    && let Some(Item::Curve(c)) =
                        pointer.and_then(|p| pick_3d(se, &sf.sketch, &map, p))
                {
                    snap = Snap::Curve(c);
                }
                let item = match snap {
                    Snap::Point(p) => Some(Item::Point(p)),
                    Snap::Curve(c) => Some(Item::Curve(c)),
                    _ => None,
                };
                // Ein Buchstabe öffnet seinen Text zum Ändern (Skizzen 8, Spec C8), sonst wählt
                // er den ganzen Text statt einer Kette.
                let text = item.and_then(|i| crate::sketch_text::text_of_item(&sf.sketch, i));
                if let Some(t) = text {
                    if se.tool != Tool::Select || !se.begin_text_edit(&mut sf.sketch, t) {
                        se.select_text(&sf.sketch, t, shift);
                    }
                } else {
                    let edited =
                        item.is_some_and(|i| se.begin_pattern_edit_item(&mut sf.sketch, i));
                    if !edited && let Snap::Curve(c) = snap {
                        se.select_chain(&sf.sketch, c, shift);
                    }
                }
            }
        } else if matches!(se.tool, Tool::Constraint(_)) {
            // Abhängigkeitsbefehl: Trifft der schnelle zweite Klick etwas anderes (zwei nahe
            // Punkte für Deckungsgleich), wählt er es wie jeder Klick; derselbe Bezug wird nicht
            // wieder abgewählt, und nichts schließt ab.
            if let Some(at) = at
                && se.constraint_second_click_counts(&sf.sketch, at, radius)
            {
                let _ = se.click(&mut sf.sketch, at, radius, shift);
            }
        } else if se.runs_pattern() {
            // Anordnung: Der zweite schnelle Klick schaltet das nächste Exemplar wie jeder Klick
            // (ein Doppelklick schließt nichts ab).
            if let Some(at) = at {
                let _ = se.click(&mut sf.sketch, at, radius, shift);
            }
        } else if se.tool == Tool::InsertControlPoint {
            // Der schnelle zweite Klick fügt den nächsten Punkt ein wie jeder Klick.
            if let Some(at) = at {
                let _ = se.click(&mut sf.sketch, at, radius, shift);
            }
        } else if !matches!(se.tool, Tool::Offset | Tool::MoveCopy | Tool::Scale) {
            // Beim Versatz hat der erste Klick schon alles getan; der zweite läge dicht an der Kurve
            // und ergäbe eine hauchdünne Kopie. Verschieben schließt nur Eingabe oder OK ab.
            let _ = se.finish_tool(&mut sf.sketch);
        }
    } else if response.clicked_by(PointerButton::Primary)
        && !click_on_grip
        && se.tool == Tool::Select
        && let Some(item) = pointer
            .filter(|_| at.is_none_or(|at| se.handle_grip_at(&sf.sketch, at, radius).is_none()))
            .and_then(|p| select_hit(se, &sf.sketch, &map, p))
            .filter(|&i| is_3d_item(&sf.sketch, i))
    {
        // 3D-Punkt oder 3D-Kurve über den Bildschirmabstand (Skizzen 10); auch dort, wo der
        // Sehstrahl die Skizzenebene nicht trifft.
        se.select_clicked(&sf.sketch, item, shift);
    } else if draws3 && response.clicked_by(PointerButton::Primary) && !click_on_grip {
        // Zeichnen in 3D: Punkt in der Ebene der Triade (auch wo die Skizzenebene nicht zu
        // treffen ist); getippte Werte werden zu Maßen.
        let _ = match &commit_scope {
            Some(scope) => se.commit_input3(sf, scope),
            None => se.click3(&mut sf.sketch),
        };
    } else if response.clicked_by(PointerButton::Primary)
        && !click_on_grip
        && se.tool == Tool::ProjectEdge
        && se.kinds.project.picks_like_include()
    {
        // Einbeziehen, Auf Fläche projizieren und Schnittkurve wählen in der 3D-Szene: auch dort,
        // wo der Sehstrahl die Skizzenebene nicht trifft (Seitenansicht, Gesamtprüfung).
        project_at = pointer;
    } else if response.clicked_by(PointerButton::Primary)
        && !click_on_grip
        && at.is_none()
        && picks_3d
        && se.pick3.is_some()
    {
        // Der Sehstrahl trifft die Skizzenebene nicht: Der Klick wählt 3D-Geometrie allein über
        // den Bildschirmabstand, ohne eine Lage in der Ebene anzunehmen.
        let _ = se.click_off_plane(&mut sf.sketch);
    } else if response.clicked_by(PointerButton::Primary)
        && !click_on_grip
        && let Some(at) = at
    {
        if se.tool == Tool::ProjectEdge {
            project_at = pointer;
        } else if let Some(scope) = &commit_scope {
            let _ = se.commit_input(sf, at, radius, shift, scope);
        } else {
            let _ = se.click(&mut sf.sketch, at, radius, shift);
        }
    }
    if response.clicked_by(PointerButton::Primary) && !se.box_aborted {
        se.click_after_flyout = false;
    }
    if response.clicked_by(PointerButton::Secondary) && !se.box_aborted {
        // Im Auswahlwerkzeug mit gewählter Kurve öffnet der Rechtsklick das Menü, sonst bricht er
        // ab wie bisher.
        let at = pointer.unwrap_or(Pos2::ZERO);
        se.secondary_click(&mut sf.sketch, (at.x, at.y));
    }
    if !ui.input(|i| i.pointer.any_down()) {
        se.box_aborted = false;
    }

    // Tasten (nicht, solange ein Eingabefeld die Tastatur hat)
    if !viewport::typing(ui.ctx()) {
        let (enter, delete) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Delete),
            )
        });
        if esc {
            // Nimmt Esc nur den Griff zurück, sehen die Felder sie nicht mehr: Sonst beendete sie
            // dort im selben Bild auch das Werkzeug.
            if se.grip_drag.is_some() {
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            }
            se.cancel(&mut sf.sketch);
        }
        // Mitten in einem Griff oder Rahmen gelten Eingabe und Entf nicht.
        let gesture = se.in_gesture();
        // Bei „Auf Fläche projizieren“ und „Schnittkurve“ gehört die Eingabe dem Dialog (OK).
        let dialog = se.tool == Tool::ProjectEdge && se.kinds.project.has_dialog();
        if enter && !flyout && !gesture && !dialog {
            let _ = match &commit_scope {
                // Gesetztes Maß, dessen Feld die Tastatur nicht mehr hat (etwa nach einem
                // abgelehnten Klick daneben): Eingabe legt es an wie dieser Klick; geht das nicht,
                // bleiben Maß, Text und Meldung, statt stumm zu verfallen.
                Some(scope) if se.tool == Tool::Dimension && se.dim.plan.is_some() => {
                    se.accept_planned(sf, scope)
                }
                // Felder da, aber ohne Fokus: derselbe Weg wie die Eingabe im Feld.
                Some(scope)
                    if !se.runs_transform()
                        && !se.runs_pattern()
                        && !se.input.fields.is_empty() =>
                {
                    se.commit_at_cursor(sf, radius, scope)
                }
                _ if se.runs_transform() => se.confirm_transform(sf),
                _ if se.runs_pattern() => se.confirm_pattern(sf),
                _ if se.runs_text() => se.confirm_text(sf, fonts.as_deref()),
                _ => se.finish_tool(&mut sf.sketch),
            };
        }
        // In der Anordnung gehört die Auswahl der Vorlage; Entf löscht dort nichts. Ebenso bei
        // offenem Abschnitt „Text“: Beim Ändern ist der Text selbst gewählt.
        if delete && !gesture && !se.runs_pattern() && !se.runs_text() {
            se.delete_selection_in(sf);
        }
    }
    // Vorschau von Stutzen, Dehnen und Brechen unter dem Mauszeiger (nach den Änderungen
    // dieses Bilds).
    let hover_at = response.hover_pos().and_then(|p| map.to_sketch(p));
    se.update_hover(&sf.sketch, hover_at, radius);
    // Aufgelöster Zeiger für Zeichen und Vorschau (nach den Änderungen dieses Bilds).
    let raw = cursor_raw(se, at);
    se.update_cursor(&sf.sketch, raw, radius);
    se.update_cursor3(&sf.sketch, &map, pointer);
    se.update_transform_block(&sf.sketch);
    se.update_pattern_note(&sf.sketch);
    let tool = se.tool;
    let kind = se.kinds.project;
    let settings = se.project;
    let relink_family = se.relink.as_ref().map(|r| r.family.clone());

    // Projizieren: Kante oder Fläche in der 3D-Szene wählen; mit dem Filter „Körper“ zählt der
    // Körper unter dem Zeiger (die Markierung zeigt, was der Zeiger trifft). Schneiden und
    // Silhouette wählen nur Flächen (Schneiden mit „Körper“ den ganzen Körper). Einbeziehen
    // (Skizzen 10) wählt dazu Ecken und Ebenen.
    // Die Isokurve (Skizzen 11b) wählt nur Flächen: keine Ebene davor verdeckt sie.
    let faces_only = matches!(
        kind,
        ToolKind::Intersect | ToolKind::Silhouette | ToolKind::Isoparametric
    );
    let pick_at = |state: &AppState, p: Pos2| {
        if faces_only {
            state
                .pick_face_under(map.local(p), map.viewport())
                .map(Pick::Face)
        } else if kind.picks_like_include() {
            state.pick_with(
                map.local(p),
                map.viewport(),
                crate::spun_profile_dialog::pick_filter(state),
            )
        } else {
            state.pick_with(map.local(p), map.viewport(), PickFilter::GEOMETRY)
        }
    };
    // „Neu verknüpfen“: Eingezogene 3D-Geometrie wählt wie „3D-Geometrie einbeziehen“ (Ecken,
    // Ebenen, Achsen, Ursprung; Ruling T18-1), alles andere wie „Projizieren“ (Ruling F9).
    let relink_filter = |family: &planora_feature::CopySource| {
        if crate::state::relink_picks_like_include(family) {
            PickFilter::INCLUDE
        } else if matches!(family, planora_feature::CopySource::Isoparametric { .. }) {
            // Die Isokurve wählt eine neue Fläche (Skizzen 11b).
            PickFilter::FACES
        } else {
            PickFilter::GEOMETRY
        }
    };
    let hover = if let Some(family) = relink_family.as_ref() {
        pointer.and_then(|p| state.pick_with(map.local(p), map.viewport(), relink_filter(family)))
    } else if tool == Tool::ProjectEdge {
        pointer.and_then(|p| pick_at(state, p))
    } else {
        None
    };
    state.set_hover(hover);
    if let (Some(p), Some(family)) = (relink_at, relink_family.as_ref()) {
        let picked = state.pick_with(map.local(p), map.viewport(), relink_filter(family));
        let done = if crate::state::relink_picks_like_include(family) {
            include_pick(state, &map, p, picked).map(|pick| {
                let _ = state.relink_include_pick(pick);
            })
        } else {
            let curve = sketch_curve_under(state, &map, p);
            relink_target(family, picked, curve).map(|t| {
                let _ = state.relink_pick(t);
            })
        };
        if done.is_none()
            && let Some(se) = state.sketch_session.as_mut()
        {
            se.message = Some(crate::msg!("lost-relink-wrong-pick"));
        }
        return;
    }
    if let Some(id) = open_label {
        crate::sketch_panel::open_existing_dimension_editor(ui.ctx(), state, id);
    }
    if let Some(p) = project_at {
        // Fehler (etwa ein Körper, der erst nach der Skizze entsteht) stehen in der Meldung der
        // Sitzung.
        let picked = pick_at(state, p);
        // „Auf Fläche projizieren“ und „Schnittkurve“ (Skizzen 10): Der Klick füllt das aktive
        // Feld des Dialogs; erst dessen OK legt an.
        // „Isoparametrische Kurve“ (Skizzen 11b): Fläche oder Punkt darauf; ein Klick neben
        // jede Fläche bekommt den Hinweis im Dialog.
        if crate::isoparametric_dialog::is_open(state) {
            match state.face_point_under(map.local(p), map.viewport()) {
                Some((body, face, at)) => {
                    crate::isoparametric_dialog::click(state, body, face, at);
                }
                None => crate::surface_project_dialog::refuse(state),
            }
            return;
        }
        if kind.has_dialog() {
            // „Drehprofil“: Wartet der Dialog auf die Achse, gilt zuerst eine Linie der offenen
            // Skizze (Skizzen 11b).
            if crate::spun_profile_dialog::wants_own_axis(state)
                && let (Some(at), Some(id)) = (map.to_sketch(p), state.open_sketch_id())
                && let Some(curve) = state.session_sketch().and_then(|sk| {
                    sk.pick_curve(at, map.radius_at(p).unwrap_or(0.0))
                        .filter(|c| crate::spun_profile_dialog::own_axis_of(sk, *c).is_some())
                })
            {
                crate::surface_project_dialog::deliver(
                    state,
                    crate::state::IncludePick::SketchCurve { sketch: id, curve },
                );
                return;
            }
            // „Drehprofil“ mit Filter „Körper“: nur Körper zählen, auch nahe der Mittellinie.
            let found = if crate::spun_profile_dialog::wants_body_only(state) {
                crate::spun_profile_dialog::body_pick(picked)
            } else {
                include_pick(state, &map, p, picked)
            };
            match found {
                Some(pick) => {
                    // Passt der Klick nicht, steht der Grund im Dialog.
                    crate::surface_project_dialog::deliver(state, pick);
                }
                // Der Hinweis steht nur im Dialog (Task H3).
                None => crate::surface_project_dialog::refuse(state),
            }
            return;
        }
        if kind == ToolKind::Include3d {
            match include_pick(state, &map, p, picked) {
                Some(pick) => {
                    let _ = state.include_into_sketch(pick);
                }
                None => {
                    if let Some(se) = state.sketch_session.as_mut() {
                        se.message = Some(crate::msg!("sketch-pick-include-3d"));
                    }
                }
            }
            return;
        }
        // Projizieren und Schneiden nehmen auch Kurven fertiger Skizzen (Skizzen 10, Spec 3);
        // Kanten und Ecken eines Körpers gehen vor.
        let curve = match (kind, picked) {
            (ToolKind::Project | ToolKind::Intersect, None | Some(Pick::Face(_))) => {
                sketch_curve_under(state, &map, p)
            }
            _ => None,
        };
        let target = match curve {
            Some((sketch, c)) => Some(ProjectTarget::SketchCurve(sketch, c)),
            None => picked.and_then(|pick| project_target(pick, settings.filter)),
        };
        match (kind, picked, target) {
            (ToolKind::Intersect, _, Some(t)) => {
                let _ = state.intersect_into_sketch(t);
            }
            (ToolKind::Silhouette, Some(Pick::Face(FaceRef { body, face })), _) => {
                let _ = state.silhouette_into_sketch(body, face);
            }
            (ToolKind::Project, _, Some(t)) => {
                let _ = state.project_into_sketch(t, settings.link);
            }
            _ => {
                let m = match kind {
                    ToolKind::Intersect => crate::msg!("sketch-pick-intersect"),
                    ToolKind::Silhouette => crate::msg!("sketch-pick-silhouette"),
                    _ => crate::msg!("sketch-pick-project"),
                };
                if let Some(se) = state.sketch_session.as_mut() {
                    se.message = Some(m);
                }
            }
        }
    }
}

/// Was ein Klick mit „3D-Geometrie einbeziehen“ (Skizzen 10, Spec 5) holt: zuerst Ecke oder Kante
/// eines Körpers (`picked`), dann eine Kurve einer fertigen Skizze davor, der Ursprung und die
/// Achsen des Ursprungs (über den Bildschirmabstand), dann Fläche oder Konstruktionsebene, zuletzt
/// eine eingeblendete Grundebene.
pub fn include_pick(
    state: &AppState,
    map: &Mapper,
    p: Pos2,
    picked: Option<Pick>,
) -> Option<crate::state::IncludePick> {
    use crate::state::IncludePick;
    use planora_feature::{AxisId, PlaneId};
    match picked {
        Some(Pick::Vertex { body, edge, end }) => {
            return Some(IncludePick::Vertex { body, edge, end });
        }
        Some(Pick::Edge(EdgeRef { body, edge })) => return Some(IncludePick::Edge(body, edge)),
        _ => {}
    }
    if let Some((sketch, curve)) = sketch_curve_under(state, map, p) {
        return Some(IncludePick::SketchCurve { sketch, curve });
    }
    let world = |q: DVec3| map.to_screen3(map.frame.to_local(q));
    if world(DVec3::ZERO).is_some_and(|s| s.distance(p) <= PICK_RADIUS_PX) {
        return Some(IncludePick::Origin);
    }
    // Die Achsen überall, wo die Ansicht sie zeichnet (Korrektur K2b), mindestens aber so lang,
    // wie das Einbeziehen sie anlegt.
    let h =
        planora_feature::plane_rect::plane_half_for_bounds(state.bodies.iter().map(|b| b.bounds));
    let drawn = planora_render::axis_half_length(map.camera.target, map.camera.distance)
        .map_or(0.0, f64::from);
    for (axis, dir) in [
        (AxisId::X, DVec3::X),
        (AxisId::Y, DVec3::Y),
        (AxisId::Z, DVec3::Z),
    ] {
        let (back, ahead) = axis_pick_span(axis, h, drawn);
        if let (Some(a), Some(b)) = (
            axis_end_on_screen(&world, -dir, back, h),
            axis_end_on_screen(&world, dir, ahead, h),
        ) && dist_to_segment(p, a, b) <= PICK_RADIUS_PX / 2.0
        {
            return Some(IncludePick::Axis(axis));
        }
    }
    match picked {
        Some(Pick::Face(FaceRef { body, face })) => return Some(IncludePick::Face(body, face)),
        Some(Pick::Plane(id)) => return Some(IncludePick::Plane(PlaneId::Feature(id))),
        _ => {}
    }
    let (l, vp) = (map.local(p), map.viewport());
    let ndc = Vec2::new(2.0 * l.x / vp.x - 1.0, 1.0 - 2.0 * l.y / vp.y);
    let (origin, dir) = map.camera.ray(ndc);
    let ray = planora_render::Ray {
        origin: to_dvec3(origin),
        dir: to_dvec3(dir),
    };
    state
        .origin_plane_rects()
        .into_iter()
        .filter_map(|(plane, rect)| planora_render::pick_plane(&ray, &rect).map(|t| (plane, t)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(plane, _)| IncludePick::Plane(PlaneId::Standard(plane)))
}

/// Wählbares Stück einer Ursprungsachse für [`include_pick`] (Korrektur K2b): wie weit hinter und
/// vor dem Ursprung (mm). Die Ansicht zeichnet x und y beidseits bis `drawn`, z nur nach oben
/// ([`planora_render::axis_lines`]); das angelegte Stück `±include_half` bleibt immer wählbar.
fn axis_pick_span(axis: planora_feature::AxisId, include_half: f64, drawn: f64) -> (f64, f64) {
    let ahead = include_half.max(drawn);
    match axis {
        planora_feature::AxisId::Z => (include_half, ahead),
        _ => (ahead, ahead),
    }
}

/// Bildschirmlage des Achsenendes `dir · len`. Liegt es hinter der Kamera (eine lange Achse bei
/// naher Kamera), wird das Stück halbiert, bis das Ende im Bild liegt, höchstens bis `min_len`.
fn axis_end_on_screen(
    world: &impl Fn(DVec3) -> Option<Pos2>,
    dir: DVec3,
    len: f64,
    min_len: f64,
) -> Option<Pos2> {
    let mut t = len;
    while t > min_len {
        if let Some(s) = world(dir * t) {
            return Some(s);
        }
        t *= 0.5;
    }
    world(dir * min_len)
}

/// Abstand des Bildschirmpunkts `p` von der Strecke `a`–`b` mit dem Parameter des nächsten
/// Punkts (0 bei `a`, 1 bei `b`).
fn segment_distance_t(p: Pos2, a: Pos2, b: Pos2) -> (f32, f32) {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.distance(a + ab * t), t)
}

/// Ist die Stelle `at` (Welt) für die Kamera von `map` durch einen Körper verdeckt? Der Strahl
/// vom Auge durch die Stelle trifft vorher eine Fläche (Slice: nur hinter der Skizzenebene). Die
/// Toleranz (0,1 % der Entfernung) lässt Linien auf einer Fläche sichtbar.
fn covered_by_body(state: &AppState, map: &Mapper, at: DVec3, slice: Option<[f32; 4]>) -> bool {
    let Some(screen) = map.to_screen3(map.frame.to_local(at)) else {
        return true;
    };
    let local = map.local(screen);
    let size = map.viewport();
    let ndc = Vec2::new(2.0 * local.x / size.x - 1.0, 1.0 - 2.0 * local.y / size.y);
    let (origin, dir) = map.camera.ray(ndc);
    let (origin, dir) = (to_dvec3(origin), to_dvec3(dir));
    let along = (at - origin).dot(dir);
    state
        .body_hit_distance(origin, dir, slice)
        .is_some_and(|hit| hit < along - 1e-3 * along.abs().max(1.0))
}

/// Skizzenpunkt oder Skizzenkurve einer fertigen, sichtbaren Skizze unter dem Zeiger `p` in der
/// Modellansicht (Skizzen 12, Spec 4): Punkte vor Kurven, jeweils der nächste im Fangkreis
/// ([`PICK_RADIUS_PX`]). Nicht gezählt wird, was die Ansicht nicht zeigt (ausgeblendete Skizze,
/// Gruppe oder Ordner, Konstruktion und Projektion mit ausgeschaltetem Schalter, Punkte mit
/// „Punkte anzeigen“ aus, unsichtbare Griffe) und was ein Körper verdeckt
/// ([`AppState::body_hit_distance`]; bei Slice zählt nur, was hinter der Skizzenebene liegt).
/// Die Stelle wird je Kandidat geprüft: Ist der nächste verdeckt, gewinnt der nächste sichtbare.
/// Körperkanten und -ecken gehen vor (siehe [`AppState::pick_with`]). `map` bildet Weltpunkte
/// ab (`Frame::WORLD`). `with_points` und `with_curves` wählen, was überhaupt zählt (die
/// Ebenen-Fenster nehmen nur Kurven bzw. nur Punkte).
pub fn finished_sketch_under(
    state: &AppState,
    map: &Mapper,
    p: Pos2,
    slice: Option<[f32; 4]>,
    with_points: bool,
    with_curves: bool,
) -> Option<Pick> {
    // Kandidaten: (Abstand, Treffer, Stelle in der Welt).
    let mut points: Vec<(f32, Pick, DVec3)> = Vec::new();
    let mut curves: Vec<(f32, Pick, DVec3)> = Vec::new();
    // Die Kameramatrix einmal je Zeigerstellung statt je Punkt (Task 12b).
    let to_screen = map.screen3_fn();
    for (id, sk) in finished_sketches(state) {
        if state.is_feature_hidden(id) {
            continue;
        }
        // Mengen und Linienzüge aus dem Zwischenspeicher (Task 12b): je Zeigerstellung bleibt
        // nur das Abbilden auf den Bildschirm.
        let cached = finished_data(state, id, sk);
        let frame = *sk.frame();
        for (pt, _) in sk.points().filter(|_| with_points) {
            if cached.hidden.points.contains(&pt)
                || cached.handles.points.contains(&pt)
                || cached.glyphs.contains(&pt)
            {
                continue;
            }
            let Ok(local) = sk.point_xyz(pt) else {
                continue;
            };
            let world = frame.point(local.x, local.y, local.z);
            if let Some(s) = to_screen(map.frame.to_local(world))
                && s.distance(p) <= PICK_RADIUS_PX
            {
                points.push((
                    s.distance(p),
                    Pick::SketchPoint {
                        sketch: id,
                        point: pt,
                    },
                    world,
                ));
            }
        }
        for (c, world) in cached.curves.iter().filter(|_| with_curves) {
            let c = *c;
            let screen: Vec<Option<Pos2>> = world
                .iter()
                .map(|w| to_screen(map.frame.to_local(*w)))
                .collect();
            let mut best: Option<(f32, DVec3)> = None;
            for i in 1..world.len() {
                let (Some(a), Some(b)) = (screen[i - 1], screen[i]) else {
                    continue;
                };
                let (d, t) = segment_distance_t(p, a, b);
                if d <= PICK_RADIUS_PX && best.is_none_or(|(e, _)| d < e) {
                    best = Some((d, world[i - 1].lerp(world[i], f64::from(t))));
                }
            }
            if let Some((d, at)) = best {
                curves.push((
                    d,
                    Pick::SketchCurve {
                        sketch: id,
                        curve: c,
                    },
                    at,
                ));
            }
        }
    }
    // Punkte vor Kurven, der nächste zuerst; verdeckte Stellen fallen weg.
    for list in [&mut points, &mut curves] {
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, pick, _)) = list
            .iter()
            .find(|(_, _, at)| !covered_by_body(state, map, *at, slice))
        {
            return Some(*pick);
        }
    }
    None
}

/// Die Kurve einer fertigen, eingeblendeten Skizze vor der offenen (Spec 10) unter dem Zeiger
/// `p`, über den Bildschirmabstand zur Kurve in Raumlage; die nächste gewinnt.
pub fn sketch_curve_under(state: &AppState, map: &Mapper, p: Pos2) -> Option<(FeatureId, CurveId)> {
    /// Stützstellen je Kurve für den Abstand.
    const SAMPLES: usize = 48;
    let open = state.open_sketch_id()?;
    let mut best: Option<(f32, FeatureId, CurveId)> = None;
    for f in state.document.features() {
        if !planora_feature::sketch_source_allowed(&state.document, open, f.id)
            || state.is_feature_hidden(f.id)
        {
            continue;
        }
        let Some(sk) = state.document.sketch_of(f.id) else {
            continue;
        };
        for (c, _) in sk.curves() {
            if sk.is_handle_curve(c) {
                continue;
            }
            let Some((curve, t0, t1)) = sk.curve_world(c) else {
                continue;
            };
            let screen: Vec<Pos2> = (0..=SAMPLES)
                .filter_map(|i| {
                    let t = t0 + (t1 - t0) * i as f64 / SAMPLES as f64;
                    map.to_screen3(map.frame.to_local(curve.point(t)))
                })
                .collect();
            let d = screen
                .windows(2)
                .map(|w| dist_to_segment(p, w[0], w[1]))
                .fold(f32::INFINITY, f32::min);
            if d <= PICK_RADIUS_PX && best.is_none_or(|(e, ..)| d < e) {
                best = Some((d, f.id, c));
            }
        }
    }
    best.map(|(_, s, c)| (s, c))
}

/// Bildschirmabstand des Punkts `p` von der Strecke `a`–`b`.
fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

/// Was ein Klick mit „Projizieren“ auf `pick` projiziert: mit „Angegebene Elemente“ die Kante
/// oder Fläche, mit „Körper“ deren ganzen Körper. Ebenen und Eckpunkte (Skizzen 6) projiziert
/// es nicht: `None`.
pub fn project_target(pick: Pick, filter: ProjectFilter) -> Option<ProjectTarget> {
    Some(match (filter, pick) {
        (ProjectFilter::Entities, Pick::Edge(EdgeRef { body, edge })) => {
            ProjectTarget::Edge(body, edge)
        }
        (ProjectFilter::Entities, Pick::Face(FaceRef { body, face })) => {
            ProjectTarget::Face(body, face)
        }
        (ProjectFilter::Bodies, Pick::Edge(EdgeRef { body, .. }))
        | (ProjectFilter::Bodies, Pick::Face(FaceRef { body, .. })) => ProjectTarget::Body(body),
        (
            _,
            Pick::Plane(_)
            | Pick::Vertex { .. }
            | Pick::SketchCurve { .. }
            | Pick::SketchPoint { .. },
        ) => return None,
    })
}

/// Was ein Klick der Wahl „Neu verknüpfen“ (Skizzen 11a) für die alte Herkunft `family` wählt:
/// dieselbe Auswahl wie „Projizieren“ mit „Angegebene Elemente“ (Ruling F9). Kante oder Fläche
/// unter dem Zeiger (`picked`); bei einer Skizzenkurve als Herkunft die Kurve einer früheren
/// Skizze unter dem Zeiger (`curve`). `None`, wenn nichts Passendes getroffen ist.
pub fn relink_target(
    family: &planora_feature::CopySource,
    picked: Option<Pick>,
    curve: Option<(FeatureId, CurveId)>,
) -> Option<ProjectTarget> {
    use planora_feature::{CopySource, IncludeRef};
    let wants_curve = matches!(
        family,
        CopySource::SketchCurve { .. } | CopySource::Include(IncludeRef::SketchCurve { .. })
    );
    if wants_curve {
        return curve.map(|(s, c)| ProjectTarget::SketchCurve(s, c));
    }
    picked.and_then(|p| project_target(p, ProjectFilter::Entities))
}

/// Ist der Punkt vollständig bestimmt (laut letztem Bericht)?
fn point_determined(report: Option<&SolveReport>, p: PointId) -> bool {
    report.is_some_and(|r| r.status == SolveStatus::Solved && !r.free_points.contains(&p))
}

fn curve_determined(report: Option<&SolveReport>, c: CurveId) -> bool {
    report.is_some_and(|r| r.status == SolveStatus::Solved && !r.free_curves.contains(&c))
}

/// Kurven und Punkte der offenen Skizze, die als Projektion ihrer Herkunft folgen (Skizzen 6):
/// die Kurven aus den Kopien des Entwurfs und die Punkte, die diese Kurven benutzen.
struct Projected {
    curves: HashSet<CurveId>,
    points: HashSet<PointId>,
    /// Verlorene Projektionen (gelb) und „Übrige Geometrie abblenden“ (Skizzen 11a).
    lost: crate::lost_projections_dialog::LostMarks,
}

impl Projected {
    fn of(draft: &SketchFeature) -> Projected {
        let curves: HashSet<CurveId> = draft.copies.iter().filter_map(|c| c.curve()).collect();
        // Dazu eingezogene Punkte (Ecken, Ursprung, Schnittpunkte; Skizzen 10).
        let points = curves
            .iter()
            .filter_map(|c| draft.sketch.curve(*c))
            .flat_map(|d| d.kind.points())
            .chain(draft.copies.iter().filter_map(|c| c.point()))
            .collect();
        Projected {
            curves,
            points,
            lost: Default::default(),
        }
    }

    /// Wie [`Projected::of`] für die offene Skizze, mit den verlorenen Projektionen und dem
    /// Abblenden des Dialogs „Verlorene Projektionen verwalten“.
    fn of_open(state: &AppState, draft: &SketchFeature) -> Projected {
        let mut p = Projected::of(draft);
        if let Some(id) = state.open_sketch_id() {
            p.lost = crate::lost_projections_dialog::marks(state, id);
        }
        p
    }
}

/// Farbe mit verlorenen Projektionen (gelb) und „Übrige Geometrie abblenden“ (Skizzen 11a):
/// Gewähltes hat Vorrang, dann die verlorene Geometrie; alles andere ist mit dem Haken gedämpft.
fn lost_ink(base: Color32, selected: bool, lost: bool, dim_rest: bool, col: Color32) -> Color32 {
    if selected {
        base
    } else if lost {
        col
    } else if dim_rest {
        dimmed(base)
    } else {
        base
    }
}

/// Anteil der Deckkraft, mit dem „Bestimmte ausblenden“ voll bestimmte Geometrie zeichnet.
const DIMMED_ALPHA: f32 = 0.2;

fn dimmed(c: Color32) -> Color32 {
    c.gamma_multiply(DIMMED_ALPHA)
}

/// Farbe einer Kurve der offenen Skizze (ohne Aufhellung unter dem Zeiger): gewählt (auch die
/// schon gewählten Kurven einer laufenden Art) > projiziert (lila wie Fusion, auch als
/// Konstruktion; gestrichelt macht der Linienstil) > Konstruktion > bestimmt > frei.
fn curve_ink(
    col: &theme::SketchColors,
    se: &SketchSession,
    sk: &Sketch,
    projected: &Projected,
    c: CurveId,
) -> Color32 {
    let selected = se.selection.contains(&Item::Curve(c))
        || se.pending_curves.contains(&c)
        || se.blend.first.is_some_and(|e| e.curve == c)
        || dim_picked(se, Item::Curve(c));
    let base = curve_ink_base(col, se, sk, projected, c, selected);
    lost_ink(
        base,
        selected,
        projected.lost.curves.contains(&c),
        projected.lost.dim_rest,
        col.lost,
    )
}

/// [`curve_ink`] ohne verlorene Projektionen und Abblenden.
fn curve_ink_base(
    col: &theme::SketchColors,
    se: &SketchSession,
    sk: &Sketch,
    projected: &Projected,
    c: CurveId,
    selected: bool,
) -> Color32 {
    if selected {
        col.selected
    } else if se.hides_determined() && curve_determined(se.last_report.as_ref(), c) {
        // Haken „Bestimmte ausblenden“ (Skizzen 11a): voll bestimmte Geometrie gedämpft.
        dimmed(col.determined)
    } else if projected.curves.contains(&c) {
        col.projected
    } else if sk.curve(c).is_some_and(|d| d.construction) {
        col.construction
    } else if curve_determined(se.last_report.as_ref(), c) {
        col.determined
    } else {
        col.free
    }
}

/// Die Rahmenlinien der gewählten Texte (Spec B9: Der Rahmen erscheint hervorgehoben).
fn selected_frames(se: &SketchSession, sk: &Sketch) -> HashSet<CurveId> {
    se.selected_texts(sk)
        .into_iter()
        .filter_map(|t| match sk.text(t)?.placement {
            planora_sketch::text::TextPlacement::Box { lines, .. } => Some(lines),
            planora_sketch::text::TextPlacement::Path { .. } => None,
        })
        .flatten()
        .collect()
}

/// Farbe eines Punkts der offenen Skizze (ohne Aufhellung): gewählt oder gezogen > Punkt einer
/// projizierten Kurve (lila wie Fusion) > bestimmt > frei.
fn point_ink(
    col: &theme::SketchColors,
    se: &SketchSession,
    projected: &Projected,
    p: PointId,
) -> Color32 {
    let selected = se.selection.contains(&Item::Point(p))
        || se.is_dragged_point(p)
        || dim_picked(se, Item::Point(p));
    let base = if selected {
        col.selected
    } else if se.hides_determined() && point_determined(se.last_report.as_ref(), p) {
        dimmed(col.determined)
    } else if projected.points.contains(&p) {
        col.projected
    } else if point_determined(se.last_report.as_ref(), p) {
        col.determined
    } else {
        col.free
    };
    lost_ink(
        base,
        selected,
        projected.lost.points.contains(&p),
        projected.lost.dim_rest,
        col.lost,
    )
}

/// Farbe, in der die Ansicht die Kurve `c` der offenen Skizze zeichnet (ohne Aufhellung unter
/// dem Zeiger); ohne offene Skizze die Farbe freier Geometrie.
pub fn curve_color(state: &AppState, c: CurveId) -> Color32 {
    let col = colors(state.dark);
    match (state.sketch_session.as_ref(), state.sketch_draft.as_ref()) {
        (Some(se), Some(draft)) => curve_ink(
            &col,
            se,
            &draft.sketch,
            &Projected::of_open(state, draft),
            c,
        ),
        _ => col.free,
    }
}

/// Wie [`curve_color`] für den Punkt `p`.
pub fn point_color(state: &AppState, p: PointId) -> Color32 {
    let col = colors(state.dark);
    match (state.sketch_session.as_ref(), state.sketch_draft.as_ref()) {
        (Some(se), Some(draft)) => point_ink(&col, se, &Projected::of_open(state, draft), p),
        _ => col.free,
    }
}

/// Element unter dem Mauszeiger, das die Anzeige heller zeigt: beim Zeichnen das Fangziel des
/// aufgelösten Zeigers, beim Abhängigkeitsbefehl, was der Klick wählen würde, sonst der Treffer der
/// Auswahl. Bestimmt je Bild [`show_overlay`] für Overlay und GPU-Linien.
fn hovered_item(state: &AppState, rect: Rect, pointer: Option<Pos2>) -> Option<Item> {
    let (se, sk, map) = (
        state.sketch_session.as_ref()?,
        state.session_sketch()?,
        mapper(rect, state)?,
    );
    if se.is_drawing_tool() {
        se.draw_cursor.as_ref().and_then(|cur| match cur.snap {
            Snap::Point(p) | Snap::Center(p) => Some(Item::Point(p)),
            Snap::Curve(c) | Snap::Midpoint(c) => Some(Item::Curve(c)),
            _ => None,
        })
    } else if matches!(se.tool, Tool::Constraint(_)) {
        // Abhängigkeitsbefehl: hervorgehoben ist, was der Klick wählen würde (passt der Punkt
        // nicht, die Kurve darunter), und nichts, was er ablehnt.
        pointer
            .and_then(|p| Some((map.to_sketch(p)?, map.radius_at(p)?)))
            .and_then(|(at, r)| se.constraint_hover(sk, at, r))
    } else {
        pointer
            .filter(|_| !se.is_dragging())
            .and_then(|p| select_hit(se, sk, &map, p))
    }
}

/// Kurven und Punkte der geöffneten Skizze, die nicht erscheinen: Beim Bearbeiten eines Musters
/// steht an der Stelle seiner Exemplare, beim Ändern eines Textes an der Stelle seiner
/// Buchstaben und seines Rahmens die Vorschau; dazu Griffe ohne Abhängigkeit. Buchstaben haben
/// keine sichtbaren Punkte (Spec B9, wie Fusion): Sie fangen nicht und lassen sich nicht einzeln
/// wählen. Dazu, was die Anzeigeschalter `display` ausblenden (Skizzen 12).
fn open_hidden(
    se: &SketchSession,
    sk: &Sketch,
    display: &DisplayFilter,
) -> (HashSet<CurveId>, HashSet<PointId>) {
    let (mut hidden, mut hidden_points) = se.pattern_hidden(sk);
    let handles_hidden = se.hidden_handles(sk);
    hidden.extend(handles_hidden.curves);
    hidden_points.extend(handles_hidden.points);
    hidden.extend(se.text_hidden(sk));
    hidden_points.extend(se.text_hidden_points(sk));
    hidden_points.extend(sk.glyph_points());
    let gone = display.hidden(sk);
    hidden.extend(gone.curves);
    hidden_points.extend(gone.points);
    (hidden, hidden_points)
}

/// Die geöffnete Skizze als Linien und Flächen für die GPU mit Tiefenprüfung (Skizzen 12):
/// Körper verdecken sie wie in Fusion (eine Skizze in einem Körper bleibt verborgen, bis die
/// Schnittansicht an ist). Zuunterst das Skizzenraster, darauf die Profilflächen (nur mit
/// [`SketchDisplay::profiles`] und nicht beim Ziehen) und das Achsenkreuz, darüber die Kurven in den Farben von `curve_ink`/`lost_ink` (Auswahl,
/// Zeiger heller, bestimmt/unbestimmt, Konstruktion, Mittellinie, Projektion; der Rahmen eines
/// gewählten Textes in der Auswahlfarbe), die Hilfslinien der gewählten Kurve, die Vorschau des
/// laufenden Werkzeugs, Stutzen/Dehnen und die Enden der Übergangskurve. Punkte, Zeichen, Maße,
/// Griffe und Marken zeichnet [`show_overlay`] obenauf.
///
/// [`SketchDisplay::profiles`]: planora_feature::SketchDisplay::profiles
pub fn open_sketch_strokes(state: &AppState, view: &StrokeView, out: &mut Vec<StrokeVertex>) {
    let (Some(se), Some(draft)) = (state.sketch_session.as_ref(), state.sketch_draft.as_ref())
    else {
        return;
    };
    if !view_usable(view) {
        return;
    }
    let sk = &draft.sketch;
    let frame = *sk.frame();
    let col = colors(state.dark);
    let projected = Projected::of_open(state, draft);

    // Zuunterst das Skizzenraster (Handtest 1, Fund 1).
    let ppp = view.pixels_per_point;
    let grid_map = Mapper {
        camera: state.camera,
        frame,
        rect: Rect::from_min_size(Pos2::ZERO, vec2(view.size_px.x / ppp, view.size_px.y / ppp)),
    };
    grid_strokes(state, view, &grid_map, out);

    // Profile als helle Flächen über dem Raster („Profil anzeigen“, wie Fusion). Beim Ziehen
    // ändert sich die Skizze in jedem Frame; die Flächen kommen erst danach wieder (sonst würden
    // sie in jedem Frame neu bestimmt).
    if state.document.sketch_display(se.sketch).profiles && !se.is_dragging() {
        let extracted = state.profile_cache.get(sk);
        // Beim Ändern eines Textes ohne seine Buchstaben (Handtest B2): An ihrer Stelle steht
        // nur die Vorschau. Ein Profil des Textes hat nur Buchstaben am äußeren Rand; die
        // Buchstaben einmal je Bild als Menge statt einer Suche je Profil.
        let glyphs: HashSet<CurveId> = se
            .edited_text()
            .and_then(|t| sk.text(t))
            .map(|d| d.curves.iter().copied().collect())
            .unwrap_or_default();
        let fill = stroke_rgba(col.profile_fill);
        for i in 0..extracted.profiles.len() {
            let own = !glyphs.is_empty()
                && extracted
                    .refs
                    .get(i)
                    .is_some_and(|r| r.outer.first().is_some_and(|e| glyphs.contains(&e.curve)));
            if own {
                continue;
            }
            let triangles: Vec<[DVec3; 3]> = extracted
                .fill(i)
                .iter()
                .map(|t| t.map(|p| frame.point(p.x, p.y, 0.0)))
                .collect();
            push_fill(out, view, &triangles, fill);
        }
    }
    // Das Achsenkreuz liegt wie in Fusion über den Profilflächen.
    axis_strokes(state, view, &grid_map, out);

    // Kurven (beim Bearbeiten eines Musters ohne seine Exemplare, siehe [`open_hidden`]).
    let (hidden, _) = open_hidden(se, sk, &DisplayFilter::of_open(state));
    // Der Rahmen eines gewählten Textes erscheint hervorgehoben (Spec B9).
    let frames = selected_frames(se, sk);
    for (c, data) in sk.curves() {
        if hidden.contains(&c) {
            continue;
        }
        let Some(points) = curve_world_points(sk, c) else {
            continue;
        };
        let mut color = if frames.contains(&c) {
            col.selected
        } else {
            curve_ink(&col, se, sk, &projected, c)
        };
        if se.draw_hover == Some(Item::Curve(c)) {
            color = lighter(color);
        }
        let width = if data.construction { 1.2 } else { CURVE_WIDTH };
        push_styled(out, view, &points, width, stroke_rgba(color), data.style());
    }

    let mut ink = Ink::Lines { view, out, frame };
    // Hilfslinien der gewählten Kurve (Kontrollpolygon, Kegelschnitt), gestrichelt.
    for line in curve_guides3(se, sk) {
        ink.line3(&line, 1.0, col.preview, LineStyle::Construction);
    }
    // Die Karte braucht die Vorschau nur für Marken, die hier entfallen.
    let map = Mapper {
        camera: state.camera,
        frame,
        rect: Rect::from_min_size(Pos2::ZERO, vec2(view.size_px.x / ppp, view.size_px.y / ppp)),
    };
    draw_preview(&mut ink, &map, sk, se, col.preview);

    // Stutzen/Brechen: das Stück rot; Dehnen: der Weg zum neuen Ende rot gestrichelt.
    if let Some(pl) = &se.hover_preview {
        // Übergangskurve: die Vorschau der neuen Kurve in der Vorschaufarbe, nicht rot wie das
        // Stück, das Stutzen entfernt.
        let color = if se.tool == Tool::BlendCurve {
            col.preview
        } else {
            col.trim_highlight
        };
        let dashed = matches!(se.tool, Tool::Extend | Tool::BlendCurve);
        ink.line(pl, CURVE_WIDTH + 1.0, color, dash(dashed));
    }
    // Übergangskurve: gewähltes und überfahrenes Ende kräftig (die Ringe im Overlay).
    if se.tool == Tool::BlendCurve {
        for (end, color) in [
            (se.blend.hover, col.preview),
            (se.blend.first, col.selected),
        ] {
            if let Some(points) = end.and_then(|e| curve_points3(sk, e.curve)) {
                ink.line3(&points, CURVE_WIDTH + 1.0, color, LineStyle::Normal);
            }
        }
    }
}

fn draw(ui: &egui::Ui, rect: Rect, state: &AppState, pointer: Option<Pos2>) {
    let (Some(se), Some(draft)) = (state.sketch_session.as_ref(), state.sketch_draft.as_ref())
    else {
        return;
    };
    let sk = &draft.sketch;
    let projected = Projected::of_open(state, draft);
    let painter = ui.painter_at(rect);
    let col = colors(state.dark);
    let map = Mapper {
        camera: state.camera,
        frame: *sk.frame(),
        rect,
    };
    let report = se.last_report.as_ref();

    // Raster, Achsenkreuz, Profilflächen, Kurven und die Linien der Vorschau zeichnet die GPU mit
    // Tiefenprüfung ([`open_sketch_strokes`], Skizzen 12); hier bleibt, was obenauf liegt.

    // Element unter dem Mauszeiger (in `show_overlay` bestimmt, siehe [`hovered_item`]).
    let hovered = se.draw_hover;

    let display = DisplayFilter::of_open(state);
    let (hidden, hidden_points) = open_hidden(se, sk, &display);

    // Krümmungskamm (Skizzen 11a) und die Vorschau des offenen Dialogs „Krümmungsanzeige
    // einrichten“; die Vorschau ändert die Skizze nicht.
    painter.extend(comb_overlay(sk, &map, col.comb, &hidden));
    if let (Some(d), Some(scale)) = (state.curvature_dialog.as_ref(), px_per_mm(&map)) {
        painter.extend(crate::curvature_dialog::preview_shapes(
            sk,
            d,
            comb_screen(&map),
            scale,
            col.comb,
        ));
    }

    // Punkte
    for (p, _) in sk.points() {
        if hidden_points.contains(&p) {
            continue;
        }
        let Some(s) = point_screen(sk, p, &map) else {
            continue;
        };
        let determined = point_determined(report, p);
        let mut color = point_ink(&col, se, &projected, p);
        let hover = hovered == Some(Item::Point(p));
        if hover {
            color = lighter(color);
        }
        let r = if hover { POINT_PX + 1.5 } else { POINT_PX };
        if determined {
            painter.circle_filled(s, r, color);
        } else {
            painter.circle(s, r, ui.visuals().extreme_bg_color, Stroke::new(1.5, color));
        }
    }

    draw_handle_grips(&painter, &map, se, sk, &col, pointer);
    draw_preview(&mut Ink::Marks(&painter), &map, sk, se, col.preview);
    let mut placed: Vec<Rect> = Vec::new();
    // Skizzenbemaßung: Hilfsstriche und der Wert am Anker (dem Zeiger bzw. der gesetzten Stelle).
    if let Some(plan) = se.dimension_preview(sk) {
        let stroke = Stroke::new(1.0, col.preview);
        // An 3D-Geometrie (Skizzen 10) stehen Hilfsstrich und Wert im Raum an der Geometrie.
        let spatial = sk.constraint_is_3d(&plan.kind);
        if spatial {
            if let ConstraintKind::Distance(a, b, _) = plan.kind
                && let (Some(a), Some(b)) = (pos3(sk, a), pos3(sk, b))
            {
                for path in map.paths3(&[a, b]) {
                    draw_path(&painter, path, stroke, true);
                }
            }
        } else {
            for line in dimension_helpers(sk, &plan) {
                for path in map.paths(&line) {
                    draw_path(&painter, path, stroke, true);
                }
            }
        }
        let anchor = if spatial {
            constraint_anchors(sk, &plan.kind)
                .first()
                .and_then(|&a| map.to_screen3(a))
        } else {
            map.to_screen(plan.anchor)
        };
        if let Some(s) = anchor {
            let text = dimension_text(ui.ctx(), sk, &plan);
            label(
                &painter,
                s + vec2(6.0, -18.0),
                &text,
                col.preview,
                col.label_bg,
                &mut placed,
            );
        }
    }

    if let Some(l) = grip_layout(state, rect) {
        let l3 = grip_layout3(state, rect);
        let hot = se
            .grip_drag
            .map(|d| d.grip)
            .or(pointer.and_then(|p| sketch_grips::hit_any(&l, l3.as_ref(), p)));
        sketch_grips::paint(&painter, &l, hot, col.selected, lighter(col.selected));
        if let Some(l3) = &l3 {
            sketch_grips::paint3(&painter, l3, hot, &col);
        }
    }
    if let Some(l) = pattern_grip_layout(state, rect) {
        let hot = se
            .pattern
            .drag
            .as_ref()
            .map(|d| d.grip)
            .or(pointer.and_then(|p| sketch_pattern::pattern_hit(&l, p)));
        sketch_pattern::paint_pattern_grips(&painter, &l, hot, col.selected, lighter(col.selected));
    }

    // Übergangskurve: gewähltes und überfahrenes Ende als Ring (die Kurve kräftig auf der GPU).
    if se.tool == Tool::BlendCurve {
        for (end, ink) in [
            (se.blend.hover, col.preview),
            (se.blend.first, col.selected),
        ] {
            let Some(end) = end else {
                continue;
            };
            if let Some(at) = point_screen(sk, end.point, &map) {
                painter.circle_stroke(at, POINT_PX + 4.0, Stroke::new(2.0, ink));
            }
        }
    }

    // Abhängigkeiten und Maße
    let bad: Vec<_> = report
        .map(|r| r.conflicting.iter().chain(&r.redundant).copied().collect())
        .unwrap_or_default();
    // Skizzen 3: Eine Kurve zeigt jede Art Zeichen höchstens einmal (n-1 gleiche Abhängigkeiten
    // einer Vieleck-Linie ergäben sonst eine lange Spalte). Fehlerhafte zuerst, damit ihre rote
    // Farbe nicht von einer unauffälligen Doppelung verdeckt wird.
    let mut shown: HashSet<(CurveId, String)> = HashSet::new();
    let mut constraints: Vec<_> = sk.constraints().collect();
    constraints.sort_by_key(|(id, _)| !bad.contains(id));
    let centers = round_centers(sk);
    let mut hits: Vec<LabelHit> = Vec::new();
    for (id, c) in constraints {
        // „Maße anzeigen“ und „Abhängigkeiten anzeigen“ (Skizzen 12): Ein ausgeblendetes Maß
        // steht auch nicht zum Doppelklick bereit. Das Maß, das die Bemaßung gerade setzt, ist
        // noch keine Abhängigkeit und zeigt oben weiter seinen Wert.
        if display.hides(sk, Item::Constraint(id)) {
            continue;
        }
        let mark = overlay_mark(&centers, &c.kind);
        let text = match mark {
            // Nur als Schlüssel für „je Kurve und Art einmal“; gezeichnet wird das Symbol.
            Some(m) => format!("{m:?}"),
            None => overlay_text_with(ui.ctx(), sk, &c.kind, c.driven, c.supplement()),
        };
        if text.is_empty() {
            continue;
        }
        // Fest (Handtest B4, wie Fusion): Fixierte Geometrie erkennt man an ihrer Farbe, nicht an
        // einem Schild je Punkt (ein aufgelöster Text hätte sonst Hunderte). Ihr Schild zeigt
        // nur eine in der Liste gewählte oder fehlerhafte Fixierung.
        if c.kind.fixed_point().is_some()
            && !bad.contains(&id)
            && !se.selection.contains(&Item::Constraint(id))
        {
            continue;
        }
        // „auf“ an einer Hilfskurve (z. B. dem Hilfskreis des Vielecks) wäre nur Lärm.
        if let ConstraintKind::PointOnCurve(_, curve) = &c.kind
            && sk.curve(*curve).is_some_and(|d| d.construction)
        {
            continue;
        }
        let color = if bad.contains(&id) {
            col.error
        } else if se.selection.contains(&Item::Constraint(id)) {
            col.selected
        } else if c.driven {
            col.construction
        } else {
            col.text
        };
        let places: Vec<DVec3> = match marked_curves(&c.kind) {
            Some(curves) => curves
                .into_iter()
                .filter(|&curve| shown.insert((curve, text.clone())))
                .filter_map(|curve| curve_anchor3(sk, curve))
                .collect(),
            None => constraint_anchors(sk, &c.kind),
        };
        for a in places {
            if let Some(s) = map.to_screen3(a) {
                let at = s + vec2(6.0, -18.0);
                match mark {
                    Some(m) => mark_label(&painter, at, m, color, col.label_bg, &mut placed),
                    None => {
                        let rect = label(&painter, at, &text, color, col.label_bg, &mut placed);
                        if c.kind.is_dimension() {
                            hits.push(LabelHit { id, rect });
                        }
                    }
                }
            }
        }
    }
    // Für den Doppelklick im nächsten Bild (die Eingabe kommt vor dem Zeichnen).
    ui.ctx()
        .data_mut(|d| d.insert_temp(label_hits_id(se.sketch), hits));

    // Zeichen der automatischen Abhängigkeiten, bevor sie entstehen
    for (at, auto) in auto_marks(sk, se) {
        if let Some(s) = map.to_screen(at) {
            let text = auto_text(ui.ctx(), auto);
            let at = s + vec2(6.0, -18.0);
            label(&painter, at, &text, col.snap, col.label_bg, &mut placed);
        }
    }

    // Hilfslinien vom ausgerichteten Punkt zum Zeiger und Fangzeichen
    if let Some(cur) = &se.draw_cursor {
        for (p, _) in [cur.guides.x, cur.guides.y].into_iter().flatten() {
            let Some(from) = pos(sk, p) else {
                continue;
            };
            for path in map.paths(&[from, cur.at]) {
                draw_path(&painter, path, Stroke::new(1.0, col.guide), true);
            }
        }
        // Ein gefangener 3D-Punkt (Skizzen 10) zeigt sein Zeichen an seiner Lage im Raum.
        let mark_at = match cur.snap {
            Snap::Point(p) if sk.is_3d_point(p) => pos3(sk, p).and_then(|q| map.to_screen3(q)),
            _ => cur.snap_at.and_then(|q| map.to_screen(q)),
        };
        if let Some(s) = mark_at {
            paint_snap_mark(&painter, s, cur.snap, col.snap);
        }
    }

    // Fanghinweis unter dem Mauszeiger in den übrigen Werkzeugen
    if se.tool != Tool::Select
        && !se.is_drawing_tool()
        && let Some(Item::Point(p)) = hovered
        && let Some(s) = point_screen(sk, p, &map)
    {
        painter.circle_stroke(s, POINT_PX + 5.0, Stroke::new(1.5, col.selected));
    }

    // Triade der 3D-Skizze über der Skizze (Skizzen 10).
    crate::sketch_triad::paint(&painter, rect, state, pointer);

    // Winkel am Zeiger beim Zug an einem Drehgriff (Text, Drehring und Drehbögen der Triade).
    paint_rotate_angle(&painter, se, sk, pointer, rect, &col);

    painter.extend(box_select_shapes(state));
}

/// Gedämpfte Griffe und Rho-Griffe. Aktive Griffe sind Kurven und Punkte der Skizze und schon gezeichnet.
fn draw_handle_grips(
    painter: &egui::Painter,
    map: &Mapper,
    se: &SketchSession,
    sk: &Sketch,
    col: &theme::SketchColors,
    pointer: Option<Pos2>,
) {
    let hot = |s: Pos2| pointer.is_some_and(|p| p.distance(s) <= HANDLE_GRIP_PX + 3.0);
    let diamond = |c: Pos2, r: f32| {
        vec![
            c - vec2(r, 0.0),
            c - vec2(0.0, r),
            c + vec2(r, 0.0),
            c + vec2(0.0, r),
        ]
    };
    let dim = col.construction.gamma_multiply(HANDLE_GHOST_FADE);
    for h in se.visible_handles(sk).iter().filter(|h| h.ghost) {
        let (Some(a), Some(t)) = (map.to_screen(h.at), map.to_screen(h.tip)) else {
            continue;
        };
        let ink = if hot(t) { col.construction } else { dim };
        draw_path(painter, vec![a, t], Stroke::new(1.0, ink), true);
        match h.part {
            HandlePart::Tangent => {
                painter.circle_stroke(t, HANDLE_GRIP_PX, Stroke::new(1.2, ink));
            }
            HandlePart::Curvature => {
                painter.add(Shape::closed_line(
                    diamond(t, HANDLE_GRIP_PX),
                    Stroke::new(1.2, ink),
                ));
            }
        }
    }
    // Drehgriff am Text (Skizzen 11b): Der Maßstab hängt von der Stelle ab, also einmal verfeinern.
    let per_px = |p: Pos2| {
        map.radius_at(p)
            .map(|r| r / f64::from(PICK_RADIUS_PX))
            .unwrap_or(0.0)
    };
    // Der gewählte Text wird einmal je Bild gesucht (Skizzen 12, Task 12).
    let rotate = se.text_rotate_base(sk);
    let mut at_px = per_px(map.rect.center());
    if let Some(b) = rotate {
        for _ in 0..2 {
            if at_px.is_finite()
                && at_px > 0.0
                && let Some(s) = map.to_screen(b.at(at_px))
            {
                at_px = per_px(s);
            }
        }
    }
    if let Some(b) = rotate.filter(|_| at_px.is_finite() && at_px > 0.0) {
        let (top, q) = (b.top, b.at(at_px));
        if let (Some(a), Some(g)) = (map.to_screen(top), map.to_screen(q)) {
            let ink = if hot(g) {
                lighter(col.selected)
            } else {
                col.selected
            };
            draw_path(painter, vec![a, g], Stroke::new(1.0, ink), true);
            painter.circle_filled(g, HANDLE_GRIP_PX + 1.0, ink);
            painter.circle_stroke(g, HANDLE_GRIP_PX + 3.5, Stroke::new(1.0, ink));
        }
    }
    for (_, at) in se.rho_grips(sk) {
        if let Some(s) = map.to_screen(at) {
            let ink = if hot(s) {
                lighter(col.selected)
            } else {
                col.selected
            };
            painter.add(Shape::convex_polygon(
                diamond(s, HANDLE_GRIP_PX),
                ink,
                Stroke::NONE,
            ));
        }
    }
}

/// Gehört `item` zu den gewählten Objekten der Skizzenbemaßung?
fn dim_picked(se: &SketchSession, item: Item) -> bool {
    se.tool == Tool::Dimension
        && se.dim.picks.iter().any(|p| match (*p, item) {
            (DimPick::Point(a), Item::Point(b)) => a == b,
            (DimPick::Curve(a, _), Item::Curve(b)) => a == b,
            _ => false,
        })
}

/// Text der Vorschau der Skizzenbemaßung, wie das Maß später in der Ansicht steht
/// (etwa „40,00 mm“, „Ø 30,00 mm“, „R 5,00 mm“, „30,0°“); ein anzeigendes wie in der Ansicht.
pub fn dimension_text(ctx: &egui::Context, sk: &Sketch, plan: &DimPlan) -> String {
    overlay_text_with(ctx, sk, &plan.kind, plan.driven, plan.supplement)
}

/// Hilfsstriche der Vorschau der Skizzenbemaßung (Skizzenkoordinaten): waagerecht und senkrecht
/// zwei Maßhilfslinien von den Punkten bis auf die Höhe (bzw. Lage) des Ankers und die Maßlinie
/// dazwischen; ausgerichtet und zur Linie die Verbindung. Kreis, Bogen, Ellipse und Winkel: keine.
pub fn dimension_helpers(sk: &Sketch, plan: &DimPlan) -> Vec<Vec<DVec2>> {
    use ConstraintKind as K;
    let at = plan.anchor;
    match plan.kind {
        K::DistanceX(a, b, _) => {
            let (Some(a), Some(b)) = (pos(sk, a), pos(sk, b)) else {
                return Vec::new();
            };
            let (ea, eb) = (DVec2::new(a.x, at.y), DVec2::new(b.x, at.y));
            vec![vec![a, ea], vec![b, eb], vec![ea, eb]]
        }
        K::DistanceY(a, b, _) => {
            let (Some(a), Some(b)) = (pos(sk, a), pos(sk, b)) else {
                return Vec::new();
            };
            let (ea, eb) = (DVec2::new(at.x, a.y), DVec2::new(at.x, b.y));
            vec![vec![a, ea], vec![b, eb], vec![ea, eb]]
        }
        K::Distance(a, b, _) => match (pos(sk, a), pos(sk, b)) {
            (Some(a), Some(b)) => vec![vec![a, b]],
            _ => Vec::new(),
        },
        K::DistancePointLine(p, l, _) | K::CenterlineDiameter(p, l, _) => {
            match pos(sk, p).and_then(|q| Some((q, sk.nearest_on_curve(l, q)?))) {
                Some((q, foot)) => vec![vec![q, foot]],
                None => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Zeichen der Fangart am Zeiger, selbst gezeichnet: Quadrat Punkt, Kreis Mitte, Dreieck
/// Mittelpunkt, X Schnitt, Sanduhr Kurve, Kreuz im Kreis Ursprung, kleines Plus Raster.
fn paint_snap_mark(painter: &egui::Painter, at: Pos2, snap: Snap, color: Color32) {
    let stroke = Stroke::new(SNAP_MARK_WIDTH, color);
    let s = SNAP_MARK_PX;
    let closed = |corners: &[(f32, f32)]| {
        let points = corners.iter().map(|&(x, y)| at + vec2(x, y)).collect();
        painter.add(Shape::closed_line(points, stroke));
    };
    let cross = |dx: f32, dy: f32| {
        painter.line_segment([at - vec2(dx, dy), at + vec2(dx, dy)], stroke);
        painter.line_segment([at - vec2(dy, -dx), at + vec2(dy, -dx)], stroke);
    };
    match snap {
        Snap::None => {}
        Snap::Point(_) => closed(&[(-s, -s), (s, -s), (s, s), (-s, s)]),
        Snap::Center(_) => {
            painter.circle_stroke(at, s, stroke);
        }
        Snap::Midpoint(_) => closed(&[(0.0, -s), (s, s), (-s, s)]),
        Snap::Intersection(..) => cross(s, s),
        // Vier Ecken über Kreuz ergeben die Sanduhr.
        Snap::Curve(_) => closed(&[(-s, -s), (s, -s), (-s, s), (s, s)]),
        Snap::Origin => {
            painter.circle_stroke(at, s + 1.0, stroke);
            cross(s + 1.0, 0.0);
        }
        Snap::Grid => cross(s * 0.6, 0.0),
    }
}

/// Kleine Beschriftung mit Hintergrund; weicht bereits gesetzten Beschriftungen nach unten aus.
/// Liefert das Rechteck samt Hintergrund.
fn label(
    painter: &egui::Painter,
    at: Pos2,
    text: &str,
    color: Color32,
    bg: Color32,
    placed: &mut Vec<Rect>,
) -> Rect {
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(LABEL_SIZE), color);
    let r = place_label(Rect::from_min_size(at, galley.size()).expand(2.0), placed);
    painter.rect_filled(r, 3.0, bg);
    painter.galley(r.min + vec2(2.0, 2.0), galley, color);
    r
}

/// Ein gezeichneter Maßtext im Bild: Maß und Rechteck des Texts samt Hintergrund (Bildschirm).
#[derive(Clone, Copy, Debug)]
pub(crate) struct LabelHit {
    pub id: ConstraintId,
    pub rect: Rect,
}

/// Ablage der Maßtexte je Skizze: Nach einem Wechsel des Tabs oder der Skizze trifft kein
/// Rechteck einer anderen Skizze.
fn label_hits_id(sketch: FeatureId) -> egui::Id {
    egui::Id::unique("planora-sketch-label-hits").with(sketch)
}

/// Das Maß der Skizze `sketch`, dessen Text im letzten Bild unter `p` stand (der zuletzt
/// gezeichnete zuerst).
fn label_at(ctx: &egui::Context, sketch: FeatureId, p: Pos2) -> Option<ConstraintId> {
    ctx.data(|d| d.get_temp::<Vec<LabelHit>>(label_hits_id(sketch)))?
        .iter()
        .rev()
        .find(|h| h.rect.contains(p))
        .map(|h| h.id)
}

/// Maßtexte der Skizze `sketch` im letzten Bild mit ihren Rechtecken (für Tests).
#[doc(hidden)]
pub fn label_hits(ctx: &egui::Context, sketch: FeatureId) -> Vec<(ConstraintId, Rect)> {
    ctx.data(|d| d.get_temp::<Vec<LabelHit>>(label_hits_id(sketch)))
        .unwrap_or_default()
        .into_iter()
        .map(|h| (h.id, h.rect))
        .collect()
}

/// Wie [`label`], aber mit einem gezeichneten Zeichen statt Text: ein Feld in Texthöhe, darin das
/// Symbol.
fn mark_label(
    painter: &egui::Painter,
    at: Pos2,
    mark: PaintedMark,
    color: Color32,
    bg: Color32,
    placed: &mut Vec<Rect>,
) {
    let size = painter
        .layout_no_wrap("H".to_string(), FontId::proportional(LABEL_SIZE), color)
        .size()
        .y;
    let r = place_label(
        Rect::from_min_size(at, vec2(size, size)).expand(2.0),
        placed,
    );
    painter.rect_filled(r, 3.0, bg);
    crate::icons::paint_hand_icon(painter, r.shrink(1.0), mark.icon(), color);
}

/// Schiebt `r` nach unten, bis es keine schon gesetzte Beschriftung überdeckt (höchstens einige
/// Male), und merkt es sich.
fn place_label(mut r: Rect, placed: &mut Vec<Rect>) -> Rect {
    /// Höchstens so oft wird einer Überlappung ausgewichen.
    const MAX_SHIFTS: usize = 8;
    for _ in 0..MAX_SHIFTS {
        if !placed.iter().any(|q| q.intersects(r)) {
            break;
        }
        r = r.translate(vec2(0.0, r.height() + 1.0));
    }
    placed.push(r);
    r
}

/// Hilfslinien der gewählten Kurven als Linienzüge (Skizzen 7): beim Spline mit Kontrollpunkten das
/// Kontrollpolygon, beim Kegelschnitt Start–Scheitel–Ende. Sie sind keine Kurven. Die Buchstaben
/// eines Textes, der gerade geändert wird, haben keine (Handtest B2: Die Ansicht zeichnet sie
/// nicht, [`SketchSession::text_hidden`]).
///
/// Dazu kommt immer das Kontrollpolygon der Kurven mit Kontrollpunktrahmen
/// ([`frame_polygons`], Skizzen 9), gewählt oder nicht.
pub fn curve_guides(se: &SketchSession, sk: &Sketch) -> Vec<Vec<DVec2>> {
    flat(curve_guides3(se, sk))
}

/// Wie [`curve_guides`] mit z: An 3D-Geometrie liegen die Hilfslinien im Raum (Skizzen 10).
pub fn curve_guides3(se: &SketchSession, sk: &Sketch) -> Vec<Vec<DVec3>> {
    let hidden = se.text_hidden(sk);
    let framed = |c: &CurveId| sk.curve(*c).is_some_and(|d| d.show_frame);
    let mut out: Vec<Vec<DVec3>> = se
        .selection
        .iter()
        .filter_map(|item| {
            let Item::Curve(c) = item else {
                return None;
            };
            if hidden.contains(c) || framed(c) {
                return None;
            }
            let ids = match &sk.curve(*c)?.kind {
                CurveKind::ControlSpline { points, .. } => points.clone(),
                CurveKind::Conic {
                    start, end, apex, ..
                } => vec![*start, *apex, *end],
                _ => return None,
            };
            ids.iter().map(|&p| pos3(sk, p)).collect()
        })
        .collect();
    out.extend(
        sk.curves()
            .filter(|(c, d)| d.show_frame && !hidden.contains(c))
            .filter_map(|(c, _)| frame_polygon(sk, c)),
    );
    out
}

/// Kontrollpolygone aller Kurven mit Kontrollpunktrahmen
/// ([`planora_sketch::model::CurveData::show_frame`]): beim Spline mit Kontrollpunkten die
/// Kontrollpunkte, beim Kegelschnitt Start–Scheitel–Ende, beim festen Spline seine
/// Kontrollpunkte. Die Ansicht zeichnet sie immer, offen oder fertig.
pub fn frame_polygons(sk: &Sketch) -> Vec<Vec<DVec2>> {
    flat(frame_polygons3(sk))
}

/// Wie [`frame_polygons`] mit z: Rahmen von 3D-Geometrie liegen im Raum (Skizzen 10).
pub fn frame_polygons3(sk: &Sketch) -> Vec<Vec<DVec3>> {
    sk.curves()
        .filter(|(_, d)| d.show_frame)
        .filter_map(|(c, _)| frame_polygon(sk, c))
        .collect()
}

/// Linienzüge in der Draufsicht (ohne z).
fn flat(lines: Vec<Vec<DVec3>>) -> Vec<Vec<DVec2>> {
    lines
        .into_iter()
        .map(|l| l.into_iter().map(|p| p.truncate()).collect())
        .collect()
}

fn frame_polygon(sk: &Sketch, c: CurveId) -> Option<Vec<DVec3>> {
    let ids = match &sk.curve(c)?.kind {
        CurveKind::ControlSpline { points, .. } => points.clone(),
        CurveKind::Conic {
            start, end, apex, ..
        } => vec![*start, *apex, *end],
        CurveKind::FixedSpline { curve, .. } => {
            return Some(curve.control_points().to_vec());
        }
        _ => return None,
    };
    ids.iter().map(|&p| pos3(sk, p)).collect()
}

/// Ausgabe der Linien der geöffneten Skizze (Skizzen 12): Linien gehen als GPU-Linien mit
/// Tiefenprüfung in [`open_sketch_strokes`], Marken (Fangzeichen, Mitten, Zeigerpunkt) bleiben im
/// egui-Overlay obenauf ([`draw`]). Beide Durchgänge laufen durch dieselbe Vorschau; jeder
/// nimmt nur seinen Teil.
enum Ink<'a> {
    /// egui-Overlay: nur die Marken.
    Marks(&'a egui::Painter),
    /// GPU: nur die Linien, in Weltkoordinaten über die Lage der Skizzenebene `frame`.
    Lines {
        view: &'a StrokeView,
        out: &'a mut Vec<StrokeVertex>,
        frame: Frame,
    },
}

impl Ink<'_> {
    /// Linienzug in Skizzenkoordinaten (Ebene der Skizze), Breite in logischen Punkten.
    fn line(&mut self, points: &[DVec2], width: f32, color: Color32, style: LineStyle) {
        if let Ink::Lines { view, out, frame } = self {
            let world: Vec<DVec3> = points.iter().map(|p| frame.point(p.x, p.y, 0.0)).collect();
            push_styled(out, view, &world, width, stroke_rgba(color), style);
        }
    }

    /// Wie [`line`](Self::line) für Punkte mit z (Skizzen 10).
    fn line3(&mut self, points: &[DVec3], width: f32, color: Color32, style: LineStyle) {
        if let Ink::Lines { view, out, frame } = self {
            let world: Vec<DVec3> = points.iter().map(|p| frame.point(p.x, p.y, p.z)).collect();
            push_styled(out, view, &world, width, stroke_rgba(color), style);
        }
    }

    /// Der Maler für die Marken; `None` im GPU-Durchgang.
    fn marks(&self) -> Option<&egui::Painter> {
        match self {
            Ink::Marks(p) => Some(p),
            Ink::Lines { .. } => None,
        }
    }
}

/// Stil einer Vorschaulinie: gestrichelt wie Konstruktionsgeometrie (6/4) oder durchgezogen.
fn dash(dashed: bool) -> LineStyle {
    if dashed {
        LineStyle::Construction
    } else {
        LineStyle::Normal
    }
}

/// Vorschau des Werkzeugs „Text“ ([`crate::sketch_text`]): nichts davon steht in der Skizze.
fn draw_text_preview(ink: &mut Ink, map: &Mapper, sk: &Sketch, se: &SketchSession, color: Color32) {
    use crate::sketch_text::TextToolKind;
    let Some(t) = se.text.as_ref() else {
        return;
    };
    // Ein Rahmen in 3D (Skizzen 10): Ecken, Rahmen und Buchstaben stehen in der Basis seiner
    // Ebene und werden in den Raum gelegt.
    // Auf dem Pfad (nur 2D) gilt sie nicht.
    let plane = t.plane3.filter(|_| t.kind == TextToolKind::Box);
    let lift = |line: &[DVec2]| -> Vec<DVec3> {
        line.iter()
            .map(|&q| match plane {
                Some(pl) => pl.to3(q),
                None => DVec3::new(q.x, q.y, 0.0),
            })
            .collect()
    };
    let cursor = match plane {
        Some(pl) => se.cursor3.map(|c| pl.to2(c.at)),
        None => se.draw_cursor.as_ref().map(|c| c.at).or(se.cursor),
    };
    if t.kind == TextToolKind::Box {
        let frame: Option<Vec<DVec2>> = match (t.frame_corners(), t.corners.as_slice(), cursor) {
            (Some(c), ..) => Some(vec![c[0], c[1], c[2], c[3], c[0]]),
            (None, [a], Some(b)) => {
                Some(vec![*a, DVec2::new(b.x, a.y), b, DVec2::new(a.x, b.y), *a])
            }
            _ => None,
        };
        if let Some(f) = &frame {
            ink.line3(&lift(f), 1.5, color, dash(true));
        }
        // Beim Aufziehen in 3D: der Zeiger in der Ebene der Triade.
        if se.draw3
            && let Some(painter) = ink.marks()
            && let Some(at) = se.cursor3.and_then(|c| map.to_screen3(c.at))
        {
            painter.circle_filled(at, 2.5, color);
        }
    }
    if t.kind == TextToolKind::Path
        && let Some(pl) = t.path.and_then(|p| sk.curve_polyline(p, STEPS_PER_TURN))
    {
        ink.line(&pl, CURVE_WIDTH + 1.0, color, dash(false));
    }
    // Die gemerkte Lage: Gelegt wird nur nach einer Änderung (`update_text_preview`).
    let placed = se.placed_text_preview().unwrap_or_default();
    for line in crate::sketch_text::contour_polylines(&placed) {
        ink.line3(&lift(&line), CURVE_WIDTH, color, dash(false));
    }
}

/// Vorschau beim Zeichnen in 3D: Linie vom Anfang, Linienzug durch die Punkte des Splines (bei
/// Kontrollpunkten das Kontrollpolygon) bis zum Zeiger, gestrichelt; dazu ein Fangzeichen am
/// gefangenen Punkt oder Ursprung.
fn draw_preview3(ink: &mut Ink, map: &Mapper, sk: &Sketch, se: &SketchSession, color: Color32) {
    let Some(cur) = se.cursor3 else {
        return;
    };
    let pending: Vec<DVec3> = se
        .pending
        .iter()
        .filter_map(|&p| sk.point_xyz(p).ok())
        .collect();
    // Tangentenkreise in 3D (Task H): gewählte Kurven und die Kurve unter dem Zeiger hervorgehoben.
    for &c in se.pending_curves.iter().chain(cur.curve.iter()) {
        if let Some(points) = curve_points3(sk, c) {
            ink.line3(&points, 3.0, color, dash(false));
        }
    }
    // Ebene Formen (Task 12): die Form in ihrer Ebene, wie der Klick sie anlegte.
    if se.planar3().is_some() {
        for line in se.planar3_preview(sk, cur.at) {
            ink.line3(&line, 1.5, color, dash(true));
        }
    }
    let shape: Vec<DVec3> = match (se.tool, pending.as_slice()) {
        (Tool::Line, [.., last]) if se.is_midpoint_line() => vec![*last * 2.0 - cur.at, cur.at],
        (Tool::Line, [.., last]) => vec![*last, cur.at],
        (Tool::Spline, [_, ..]) => pending.iter().copied().chain([cur.at]).collect(),
        _ => Vec::new(),
    };
    ink.line3(&shape, 1.5, color, dash(true));
    if let Some(painter) = ink.marks()
        && let Some(s) = map.to_screen3(cur.at)
    {
        if cur.snap.is_some() || cur.origin {
            painter.circle_stroke(s, POINT_PX + 4.0, Stroke::new(1.5, color));
        } else if let Some(c) = cur.mid {
            // Fang auf eine Mitte (Skizzen 10): das Dreieck wie beim 2D-Fang.
            paint_snap_mark(painter, s, Snap::Midpoint(c), color);
        } else {
            painter.circle_filled(s, 2.5, color);
        }
    }
}

/// Vorschau des laufenden Werkzeugs vom zuletzt gesetzten Punkt zum Mauszeiger.
fn draw_preview(ink: &mut Ink, map: &Mapper, sk: &Sketch, se: &SketchSession, color: Color32) {
    // Verschieben/Kopieren und Maßstab brauchen keinen Zeiger; mitgezogene Hilfsgeometrie erscheint
    // in ihrem Stil.
    if matches!(se.tool, Tool::MoveCopy | Tool::Scale) {
        // Mit „3D-Skizze“ im Raum (Skizzen 10).
        if se.tool == Tool::MoveCopy && sk.three_d {
            for (style, line) in se.transform_preview3_styled(sk).unwrap_or_default() {
                ink.line3(&line, CURVE_WIDTH, color, style);
            }
            return;
        }
        for (style, line) in se.transform_preview_styled(sk).unwrap_or_default() {
            ink.line(&line, CURVE_WIDTH, color, style);
        }
        return;
    }
    // Anordnung: die Exemplare, die Eingabe anlegt; abgewählte gestrichelt und dünn.
    if se.runs_pattern() {
        for (_, suppressed, lines) in se.pattern_preview_all(sk).unwrap_or_default() {
            let width = if suppressed { 1.0 } else { CURVE_WIDTH };
            for line in lines {
                ink.line(&line, width, color, dash(suppressed));
            }
        }
        // Die Mitte des Kreismusters, die erst das Anlegen als Punkt setzt.
        if let Some(painter) = ink.marks()
            && let Some(at) = se.pattern.new_center.and_then(|hit| map.to_screen(hit.at))
        {
            painter.circle_filled(at, 3.0, color);
        }
        return;
    }
    // Text: Rahmen gestrichelt (beim Aufziehen bis zum Zeiger), der Pfad kräftig, der Text, wie
    // OK ihn anlegt.
    if se.runs_text() {
        draw_text_preview(ink, map, sk, se, color);
        return;
    }
    // Zeichnen in 3D (Skizzen 10): vom gesetzten Punkt zum Zeiger in der Ebene der Triade.
    if se.draw3 {
        draw_preview3(ink, map, sk, se, color);
        return;
    }
    // Die übrigen Vorschauen sind reine Linien: im Overlay-Durchgang nichts zu tun.
    if ink.marks().is_some() {
        return;
    }
    // Ohne Zeiger über der Skizze bleibt die Vorschau an der letzten Stelle stehen.
    let Some(cursor) = se.draw_cursor.as_ref().map(|c| c.at).or(se.cursor) else {
        return;
    };
    const W: f32 = 1.5;
    if let Some(lines) = se.offset_preview(sk, cursor) {
        for line in lines {
            ink.line(&line, W, color, dash(true));
        }
        return;
    }
    if se.line_arc.is_some() {
        for line in se.line_arc_preview(sk, cursor).unwrap_or_default() {
            ink.line(&line, W, color, dash(true));
        }
        return;
    }
    if se.planned_kind().is_some() {
        // Wer Kurven wählt, rechnet mit der Rohlage, wie der Klick auch.
        let at = match &se.draw_cursor {
            Some(c) if se.picks_curves() => c.raw,
            _ => cursor,
        };
        for line in se.kind_preview(sk, at).unwrap_or_default() {
            ink.line(&line, W, color, dash(true));
        }
        return;
    }
    // Spline mit Kontrollpunkten: die Kurve durchgezogen, das Polygon gestrichelt (unten).
    if let Some(curve) = se.control_spline_preview(sk, cursor) {
        ink.line(&curve, W, color, dash(false));
    }
    let pending: Vec<DVec2> = se.pending.iter().filter_map(|&p| pos(sk, p)).collect();
    let circle = |m: DVec2, r: f64| -> Vec<DVec2> {
        (0..=STEPS_PER_TURN)
            .map(|k| {
                let t = TAU * k as f64 / STEPS_PER_TURN as f64;
                m + DVec2::new(t.cos(), t.sin()) * r
            })
            .collect()
    };
    let shape: Vec<DVec2> = match (se.tool, pending.as_slice(), se.pending_pos.as_slice()) {
        (Tool::Line, [.., last], _) if se.is_midpoint_line() => vec![*last * 2.0 - cursor, cursor],
        (Tool::Line, [.., last], _) => vec![*last, cursor],
        (Tool::Rectangle, _, [a, ..]) => vec![
            *a,
            DVec2::new(cursor.x, a.y),
            cursor,
            DVec2::new(a.x, cursor.y),
            *a,
        ],
        (Tool::Circle, [m, ..], _) => circle(*m, m.distance(cursor)),
        (Tool::Arc, [m], _) => vec![*m, cursor],
        (Tool::Arc, [m, s, ..], _) => {
            let r = s.distance(*m);
            let a0 = (*s - *m).y.atan2((*s - *m).x);
            let d = cursor - *m;
            let a1 = d.y.atan2(d.x);
            // Im Uhrzeigersinn gezogen läuft die Vorschau andersherum (wie der Bogen).
            let sign = if se.arc_cw() { -1.0 } else { 1.0 };
            let sweep = (sign * (a1 - a0)).rem_euclid(TAU);
            let steps = ((sweep / TAU * STEPS_PER_TURN as f64).ceil() as usize).max(1);
            (0..=steps)
                .map(|k| {
                    let t = a0 + sign * sweep * k as f64 / steps as f64;
                    *m + DVec2::new(t.cos(), t.sin()) * r
                })
                .collect()
        }
        (Tool::Ellipse, [m, ..], []) => vec![*m, cursor],
        (Tool::Ellipse, [m, ..], [major_end, ..]) => {
            let major = *major_end - *m;
            let len = major.length();
            if len <= sk.tolerance().linear {
                return;
            }
            let u = major / len;
            let minor = u.perp_dot(cursor - *m).abs().min(len);
            (0..=STEPS_PER_TURN)
                .map(|k| {
                    let t = TAU * k as f64 / STEPS_PER_TURN as f64;
                    *m + major * t.cos() + u.perp() * minor * t.sin()
                })
                .collect()
        }
        (Tool::Spline, [_, ..], _) => pending.iter().copied().chain([cursor]).collect(),
        _ => return,
    };
    ink.line(&shape, W, color, dash(true));
}
