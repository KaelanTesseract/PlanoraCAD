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
use egui::{Color32, FontId, Mesh, PointerButton, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use glam::{Vec2, Vec3};
use planora_feature::{FeatureId, FeatureKind, SketchFeature};
use planora_i18n::tr;
use planora_math::{DVec2, DVec3, Frame};
use planora_render::{EdgeRef, FaceRef, OrbitCamera, Pick, exact_step, grid_radius, line_weight};
use planora_sketch::ExtractedProfiles;
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
/// Länge der Achsenkreuz-Striche im Skizzenursprung in Pixeln.
const AXIS_PX: f32 = 28.0;
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
/// Teilstücke einer Rasterlinie: Liegt ein Teil bei geneigter Ansicht hinter der Kamera, bleibt der
/// Rest sichtbar.
const GRID_SEGMENTS: usize = 8;
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
/// und der Fang beim Zeichnen in 3D).
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
    crate::sketch_handles::HiddenHandles {
        points: hidden_points,
        curves: hidden_curves,
    }
}

/// 3D-Punkt oder 3D-Kurve unter dem Zeiger `pointer`, über den Bildschirmabstand zur projizierten
/// Lage (höchstens [`PICK_RADIUS_PX`]): Punkte vor Kurven, jeweils der nächste. Was die Ansicht
/// nicht zeichnet, zählt nicht (wie beim 2D-Fang): unsichtbare Griffe, Exemplare eines Musters in
/// Bearbeitung, Buchstaben und Rahmen eines Textes beim Ändern sowie die Punkte der Buchstaben.
/// 2D-Geometrie trifft weiter [`SketchSession::snap`].
#[doc(hidden)]
pub fn pick_3d(se: &SketchSession, sk: &Sketch, map: &Mapper, pointer: Pos2) -> Option<Item> {
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
        .filter(|&p| sk.is_3d_point(p) && !hidden.points.contains(&p))
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

/// Linienzüge der Kurve `c` auf dem Bildschirm: 3D-Kurven über [`curve_screen_paths`] mit z,
/// alle anderen wie bisher über die Draufsicht. `None`, wenn eine 2D-Kurve keinen Linienzug hat.
fn curve_paths(sk: &Sketch, c: CurveId, map: &Mapper) -> Option<Vec<Vec<Pos2>>> {
    if sk.is_3d_curve(c) {
        Some(curve_screen_paths(sk, c, map))
    } else {
        Some(map.paths(&sk.curve_polyline(c, STEPS_PER_TURN)?))
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

/// Flächen der Profile als ein Dreiecksnetz (Füllung aus [`ExtractedProfiles::fill`]), Farbe je
/// Profil aus `color` (`None`: nicht zeichnen). Leer, wenn nichts zu zeichnen ist.
fn profile_fill_shape(
    map: &Mapper,
    extracted: &ExtractedProfiles,
    color: impl Fn(usize) -> Option<Color32>,
) -> Option<Shape> {
    let mut mesh = Mesh::default();
    for i in 0..extracted.profiles.len() {
        let Some(c) = color(i) else {
            continue;
        };
        for t in extracted.fill(i) {
            let (Some(a), Some(b), Some(d)) = (
                map.to_screen(t[0]),
                map.to_screen(t[1]),
                map.to_screen(t[2]),
            ) else {
                continue;
            };
            let base = mesh.vertices.len() as u32;
            for q in [a, b, d] {
                mesh.colored_vertex(q, c);
            }
            mesh.add_triangle(base, base + 1, base + 2);
        }
    }
    (!mesh.is_empty()).then(|| Shape::mesh(mesh))
}

/// Strichstärke des Randes einer Konstruktionsebene in der 3D-Ansicht.
const PLANE_EDGE_WIDTH: f32 = 1.5;

/// Konstruktionsebenen (Skizzen 6) als halbdurchsichtige orange Rechtecke über der 3D-Ansicht
/// in `rect` ([`AppState::plane_rects`]); gewählte (auch im Browser) und die unter dem Mauszeiger
/// heller ([`AppState::plane_highlighted`]). Wie die
/// fertigen Skizzen liegen sie immer über den Körpern (keine Verdeckung, Abweichung von Fusion).
/// Eine Ebene, deren Ecke hinter der Kamera liegt, fehlt.
pub fn plane_shapes(state: &AppState, rect: Rect) -> Vec<Shape> {
    let mut out = Vec::new();
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return out;
    }
    let col = colors(state.dark);
    // Eingeblendete Grundebenen (Ordner „Ursprung“) zuerst, darüber die Konstruktionsebenen.
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
    for (plane, lit) in origins.chain(features) {
        let map = Mapper {
            camera: state.camera,
            frame: plane.frame,
            rect,
        };
        let corners: Option<Vec<Pos2>> = plane
            .corners()
            .into_iter()
            .map(|c| map.to_screen(plane.frame.to_local(c).truncate()))
            .collect();
        let Some(corners) = corners else {
            continue;
        };
        let (fill, edge) = if lit {
            (col.plane_fill_active, col.plane_edge_active)
        } else {
            (col.plane_fill, col.plane_edge)
        };
        out.push(Shape::convex_polygon(
            corners,
            fill,
            Stroke::new(PLANE_EDGE_WIDTH, edge),
        ));
    }
    // Vorschau des offenen Ebenen-Fensters in der Hervorhebungsfarbe (noch kein Feature).
    if let Some(plane) = state.plane_pick.and_then(|p| p.preview) {
        let map = Mapper {
            camera: state.camera,
            frame: plane.frame,
            rect,
        };
        let corners: Option<Vec<Pos2>> = plane
            .corners()
            .into_iter()
            .map(|c| map.to_screen(plane.frame.to_local(c).truncate()))
            .collect();
        if let Some(corners) = corners {
            out.push(Shape::convex_polygon(
                corners,
                col.plane_fill_active,
                Stroke::new(PLANE_EDGE_WIDTH, col.plane_edge_active),
            ));
        }
    }
    out
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

/// Fertige (nicht geöffnete) Skizzen als dünne Linien über der 3D-Ansicht in `rect`;
/// Konstruktionsgeometrie grau und gestrichelt. Die geöffnete Skizze zeichnet
/// [`show_overlay`]. Skizzenkurven lassen sich in 3D nicht anklicken, und die Linien liegen
/// immer über den Körpern (keine Verdeckung). Wählt ein Dialog Profile
/// ([`AppState::profile_pick`]), liegen die Flächen dieser Skizze unter ihren Linien: gewählte
/// kräftiger, die unter dem Mauszeiger heller.
pub fn finished_sketch_shapes(state: &AppState, rect: Rect) -> Vec<Shape> {
    let mut out = Vec::new();
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return out;
    }
    let open = state.open_sketch_id();
    let col = colors(state.dark);
    // Die berechneten Skizzen der übrigen Skizzen-Features (aufgelöste Ebene, nachgeführte
    // Kantenkopien); die geöffnete zeichnet das Overlay aus dem Entwurf.
    for (id, _) in state.sketch_features() {
        if open == Some(id) {
            continue;
        }
        // Im Baum ausgeblendet: nicht zeichnen, es sei denn, ein Dialog wählt gerade ihre
        // Profile (dann braucht man sie).
        let picking = state.profile_pick.as_ref().is_some_and(|p| p.sketch == id);
        if state.is_feature_hidden(id) && !picking {
            continue;
        }
        let Some(sk) = state.document.sketch_of(id) else {
            continue;
        };
        let map = Mapper {
            camera: state.camera,
            frame: *sk.frame(),
            rect,
        };
        if let Some(pick) = state.profile_pick.as_ref().filter(|p| p.sketch == id) {
            let extracted = state.profile_cache.get(sk);
            out.extend(profile_fill_shape(&map, &extracted, |i| {
                Some(if pick.hover.contains(&i) {
                    col.profile_fill_hover
                } else if pick.selected.contains(&i) {
                    col.profile_fill_selected
                } else {
                    col.profile_fill
                })
            }));
        }
        // Projektionen lila wie in Fusion (Skizzen 6), Konstruktion weiter gestrichelt.
        let projected: HashSet<CurveId> = match state.document.feature(id).map(|f| &f.kind) {
            Some(FeatureKind::Sketch(def)) => def.copies.iter().filter_map(|c| c.curve()).collect(),
            _ => HashSet::new(),
        };
        // Verlorene Projektionen gelb, mit „Übrige Geometrie abblenden“ alles andere gedämpft
        // (Skizzen 11a).
        let lost = crate::lost_projections_dialog::marks(state, id);
        // Eine fertige Skizze hat keine Auswahl: Griffe ohne Abhängigkeit sind nicht zu sehen (B6).
        let handles_hidden = crate::sketch_handles::hidden_handles_of(sk, &[]);
        for (c, data) in sk.curves() {
            if handles_hidden.curves.contains(&c) {
                continue;
            }
            let Some(paths) = curve_paths(sk, c, &map) else {
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
            let base = if marked {
                col.selected
            } else if projected.contains(&c) {
                col.projected
            } else if data.construction {
                col.construction
            } else {
                col.finished
            };
            let color = lost_ink(
                base,
                marked,
                lost.curves.contains(&c),
                lost.dim_rest,
                col.lost,
            );
            let stroke = Stroke::new(width, color);
            for path in paths {
                out.extend(style_shapes(path, stroke, data.style()));
            }
        }
        // Krümmungskamm (Skizzen 11a), auch außerhalb des Skizzen-Modus.
        out.extend(comb_overlay(sk, &map, col.comb, &handles_hidden.curves));
        // Kontrollpunktrahmen (Skizzen 9) dünn und gestrichelt.
        let frame_stroke = Stroke::new(FINISHED_CONSTRUCTION_WIDTH, col.construction);
        for line in frame_polygons3(sk) {
            for path in map.paths3(&line) {
                out.extend(path_shapes(path, frame_stroke, true));
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

/// Skizzenraster in der Ebene der offenen Skizze als Formen (leer, wenn es ausgeschaltet ist): Stufe
/// und Übergang wie das Bodenraster, jede Linie auf einem Vielfachen der feinen Stufe. Höchstens
/// [`GRID_MAX_LINES`] Linien. Wo eine Achse der 3D-Szene liegt, läuft keine Rasterlinie.
pub fn sketch_grid_shapes(state: &AppState, rect: Rect) -> Vec<Shape> {
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
            let points: Vec<DVec2> = (0..=GRID_SEGMENTS)
                .map(|i| {
                    let v = from + (to - from) * i as f64 / GRID_SEGMENTS as f64;
                    if along_x {
                        DVec2::new(v, at)
                    } else {
                        DVec2::new(at, v)
                    }
                })
                .collect();
            let color = colors
                .viewport_grid
                .lerp_to_gamma(colors.viewport_grid_major, share)
                .gamma_multiply(alpha);
            for path in map.paths(&points) {
                out.push(Shape::line(path, Stroke::new(GRID_WIDTH, color)));
            }
        }
    }
    out
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
        ui.id().with("planora-sketch-overlay"),
        Sense::click_and_drag(),
    );
    // Die Zeichenfläche nimmt den Fokus an (Tab, Pfeile), liest aber keinen Text: ihr Fokus
    // sperrt Esc, Entf und Eingabe der Skizze nicht.
    viewport::focus_is_not_typing(ui.ctx(), response.id);
    state.camera.set_viewport(rect.width(), rect.height());
    // Griffe eines Einfügen-Fensters nehmen den Zeiger vor den Werkzeugen, danach die Triade der
    // 3D-Skizze (Skizzen 10).
    if !insert_grips_input(ui, rect, state, response.hover_pos())
        && !crate::sketch_triad::input(ui, rect, state, response.hover_pos())
    {
        handle_input(ui, &response, rect, state);
    }
    let pointer = response.hover_pos().or(response.interact_pointer_pos());
    if crate::sketch_inline::show(ui, rect, state, pointer) {
        cursor_after_fields(rect, state, pointer);
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
    let response = theme::field_area(egui::Id::new("planora-sketch-curve-menu"), pos2(at.0, at.1))
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
            .and_then(|p| pick_3d(se, &sf.sketch, &map, p))
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
    let faces_only = matches!(kind, ToolKind::Intersect | ToolKind::Silhouette);
    let pick_at = |state: &AppState, p: Pos2| {
        if kind.picks_like_include() {
            state.pick_with(map.local(p), map.viewport(), PickFilter::INCLUDE)
        } else if faces_only {
            state
                .pick_face_under(map.local(p), map.viewport())
                .map(Pick::Face)
        } else {
            state.pick_with(map.local(p), map.viewport(), PickFilter::GEOMETRY)
        }
    };
    // „Neu verknüpfen“: Eingezogene 3D-Geometrie wählt wie „3D-Geometrie einbeziehen“ (Ecken,
    // Ebenen, Achsen, Ursprung; Ruling T18-1), alles andere wie „Projizieren“ (Ruling F9).
    let relink_filter = |family: &planora_feature::CopySource| {
        if crate::state::relink_picks_like_include(family) {
            PickFilter::INCLUDE
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
        if kind.has_dialog() {
            match include_pick(state, &map, p, picked) {
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
fn include_pick(
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
    // Die Achsen so lang, wie das Einbeziehen sie anlegt.
    let h =
        planora_feature::plane_rect::plane_half_for_bounds(state.bodies.iter().map(|b| b.bounds));
    for (axis, dir) in [
        (AxisId::X, DVec3::X),
        (AxisId::Y, DVec3::Y),
        (AxisId::Z, DVec3::Z),
    ] {
        if let (Some(a), Some(b)) = (world(-dir * h), world(dir * h))
            && dist_to_segment(p, a, b) <= PICK_RADIUS_PX / 2.0
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
        (_, Pick::Plane(_) | Pick::Vertex { .. }) => return None,
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

    // Skizzenraster zuunterst.
    painter.extend(sketch_grid_shapes(state, rect));

    // Profile als helle Flächen über dem Raster, unter allem anderen („Profil anzeigen“, wie Fusion). Beim
    // Ziehen ändert sich die Skizze in jedem Frame; die Flächen kommen erst danach
    // wieder (sonst würden sie in jedem Frame neu bestimmt).
    if state.show_sketch_profiles && !se.is_dragging() {
        let extracted = state.profile_cache.get(sk);
        // Beim Ändern eines Textes ohne seine Buchstaben (Handtest B2): An ihrer Stelle steht
        // nur die Vorschau. Ein Profil des Textes hat nur Buchstaben am äußeren Rand; die
        // Buchstaben einmal je Bild als Menge statt einer Suche je Profil.
        let glyphs: HashSet<CurveId> = se
            .edited_text()
            .and_then(|t| sk.text(t))
            .map(|d| d.curves.iter().copied().collect())
            .unwrap_or_default();
        painter.extend(profile_fill_shape(&map, &extracted, |i| {
            let own = !glyphs.is_empty()
                && extracted
                    .refs
                    .get(i)
                    .is_some_and(|r| r.outer.first().is_some_and(|e| glyphs.contains(&e.curve)));
            (!own).then_some(col.profile_fill)
        }));
    }

    // Achsenkreuz im Skizzenursprung
    if let Some(o) = map.to_screen(DVec2::ZERO) {
        for (dir, color) in [(DVec2::X, col.axis_x), (DVec2::Y, col.axis_y)] {
            if let Some(e) = map.to_screen(dir) {
                let d = (e - o).normalized() * AXIS_PX;
                if d.is_finite() {
                    painter.line_segment([o, o + d], Stroke::new(1.5, color));
                }
            }
        }
    }

    // Element unter dem Mauszeiger: beim Zeichnen das Fangziel des aufgelösten Zeigers.
    let hovered: Option<Item> = if se.is_drawing_tool() {
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
    };

    // Kurven (beim Bearbeiten eines Musters ohne seine Exemplare: An ihrer Stelle steht die
    // Vorschau).
    let (mut hidden, mut hidden_points) = se.pattern_hidden(sk);
    let handles_hidden = se.hidden_handles(sk);
    hidden.extend(handles_hidden.curves);
    hidden_points.extend(handles_hidden.points);
    // Beim Ändern eines Textes steht die Vorschau an der Stelle seiner Buchstaben und seines
    // Rahmens.
    hidden.extend(se.text_hidden(sk));
    hidden_points.extend(se.text_hidden_points(sk));
    // Buchstaben haben keine sichtbaren Punkte (Spec B9, wie Fusion): Sie fangen nicht und
    // lassen sich nicht einzeln wählen.
    hidden_points.extend(sk.glyph_points());
    // Der Rahmen eines gewählten Textes erscheint hervorgehoben (Spec B9).
    let frames = selected_frames(se, sk);
    for (c, data) in sk.curves() {
        if hidden.contains(&c) {
            continue;
        }
        let Some(paths) = curve_paths(sk, c, &map) else {
            continue;
        };
        let mut color = if frames.contains(&c) {
            col.selected
        } else {
            curve_ink(&col, se, sk, &projected, c)
        };
        if hovered == Some(Item::Curve(c)) {
            color = lighter(color);
        }
        let width = if data.construction { 1.2 } else { CURVE_WIDTH };
        for path in paths {
            painter.extend(style_shapes(path, Stroke::new(width, color), data.style()));
        }
    }

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

    // Hilfslinien der gewählten Kurve (Kontrollpolygon, Kegelschnitt), gestrichelt unter den Punkten.
    for line in curve_guides3(se, sk) {
        for path in map.paths3(&line) {
            draw_path(&painter, path, Stroke::new(1.0, col.preview), true);
        }
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
    draw_preview(&painter, &map, sk, se, col.preview);
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

    // Stutzen/Brechen: das Stück rot; Dehnen: der Weg zum neuen Ende rot gestrichelt.
    if let Some(pl) = &se.hover_preview {
        // Übergangskurve: die Vorschau der neuen Kurve in der Vorschaufarbe, nicht rot wie das
        // Stück, das Stutzen entfernt.
        let ink = if se.tool == Tool::BlendCurve {
            col.preview
        } else {
            col.trim_highlight
        };
        let stroke = Stroke::new(CURVE_WIDTH + 1.0, ink);
        for path in map.paths(pl) {
            draw_path(
                &painter,
                path,
                stroke,
                matches!(se.tool, Tool::Extend | Tool::BlendCurve),
            );
        }
    }
    // Übergangskurve: gewähltes und überfahrenes Ende hervorgehoben (Kurve kräftig, Ende als Ring).
    if se.tool == Tool::BlendCurve {
        for (end, ink) in [
            (se.blend.hover, col.preview),
            (se.blend.first, col.selected),
        ] {
            let Some(end) = end else {
                continue;
            };
            for path in curve_screen_paths(sk, end.curve, &map) {
                draw_path(&painter, path, Stroke::new(CURVE_WIDTH + 1.0, ink), false);
            }
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
    let mut at_px = per_px(map.rect.center());
    for _ in 0..2 {
        if let Some(s) = se
            .text_rotate_grips(sk, at_px)
            .first()
            .and_then(|&(_, q)| map.to_screen(q))
        {
            at_px = per_px(s);
        }
    }
    for (t, q) in se.text_rotate_grips(sk, at_px) {
        let top = match sk.text(t).map(|d| d.placement) {
            Some(planora_sketch::text::TextPlacement::Box { corners, .. }) => {
                match (sk.point_xy(corners[2]), sk.point_xy(corners[3])) {
                    (Ok(a), Ok(b)) => (a + b) * 0.5,
                    _ => continue,
                }
            }
            _ => continue,
        };
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
    egui::Id::new("planora-sketch-label-hits").with(sketch)
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

/// Vorschau des Werkzeugs „Text“ ([`crate::sketch_text`]): nichts davon steht in der Skizze.
fn draw_text_preview(
    painter: &egui::Painter,
    map: &Mapper,
    sk: &Sketch,
    se: &SketchSession,
    color: Color32,
) {
    use crate::sketch_text::TextToolKind;
    let Some(t) = se.text.as_ref() else {
        return;
    };
    // Ein Rahmen in 3D (Skizzen 10): Ecken, Rahmen und Buchstaben stehen in der Basis seiner
    // Ebene und werden in den Raum gelegt.
    // Auf dem Pfad (nur 2D) gilt sie nicht.
    let plane = t.plane3.filter(|_| t.kind == TextToolKind::Box);
    let paths = |line: &[DVec2]| match plane {
        Some(pl) => map.paths3(&line.iter().map(|&q| pl.to3(q)).collect::<Vec<_>>()),
        None => map.paths(line),
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
        for path in frame.iter().flat_map(|f| paths(f)) {
            draw_path(painter, path, Stroke::new(1.5, color), true);
        }
        // Beim Aufziehen in 3D: der Zeiger in der Ebene der Triade.
        if se.draw3
            && let Some(at) = se.cursor3.and_then(|c| map.to_screen3(c.at))
        {
            painter.circle_filled(at, 2.5, color);
        }
    }
    if t.kind == TextToolKind::Path
        && let Some(pl) = t.path.and_then(|p| sk.curve_polyline(p, STEPS_PER_TURN))
    {
        for path in map.paths(&pl) {
            draw_path(painter, path, Stroke::new(CURVE_WIDTH + 1.0, color), false);
        }
    }
    let stroke = Stroke::new(CURVE_WIDTH, color);
    // Die gemerkte Lage: Gelegt wird nur nach einer Änderung (`update_text_preview`).
    let placed = se.placed_text_preview().unwrap_or_default();
    for line in crate::sketch_text::contour_polylines(&placed) {
        for path in paths(&line) {
            draw_path(painter, path, stroke, false);
        }
    }
}

/// Vorschau beim Zeichnen in 3D: Linie vom Anfang, Linienzug durch die Punkte des Splines (bei
/// Kontrollpunkten das Kontrollpolygon) bis zum Zeiger, gestrichelt; dazu ein Fangzeichen am
/// gefangenen Punkt oder Ursprung.
fn draw_preview3(
    painter: &egui::Painter,
    map: &Mapper,
    sk: &Sketch,
    se: &SketchSession,
    color: Color32,
) {
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
        for path in curve_screen_paths(sk, c, map) {
            draw_path(painter, path, Stroke::new(3.0, color), false);
        }
    }
    // Ebene Formen (Task 12): die Form in ihrer Ebene, wie der Klick sie anlegte.
    if se.planar3().is_some() {
        for line in se.planar3_preview(sk, cur.at) {
            for path in map.paths3(&line) {
                draw_path(painter, path, Stroke::new(1.5, color), true);
            }
        }
    }
    let shape: Vec<DVec3> = match (se.tool, pending.as_slice()) {
        (Tool::Line, [.., last]) if se.is_midpoint_line() => vec![*last * 2.0 - cur.at, cur.at],
        (Tool::Line, [.., last]) => vec![*last, cur.at],
        (Tool::Spline, [_, ..]) => pending.iter().copied().chain([cur.at]).collect(),
        _ => Vec::new(),
    };
    for path in map.paths3(&shape) {
        draw_path(painter, path, Stroke::new(1.5, color), true);
    }
    if let Some(s) = map.to_screen3(cur.at) {
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
fn draw_preview(
    painter: &egui::Painter,
    map: &Mapper,
    sk: &Sketch,
    se: &SketchSession,
    color: Color32,
) {
    // Verschieben/Kopieren und Maßstab brauchen keinen Zeiger; mitgezogene Hilfsgeometrie erscheint
    // in ihrem Stil.
    if matches!(se.tool, Tool::MoveCopy | Tool::Scale) {
        let stroke = Stroke::new(CURVE_WIDTH, color);
        // Mit „3D-Skizze“ im Raum (Skizzen 10).
        if se.tool == Tool::MoveCopy && sk.three_d {
            for (style, line) in se.transform_preview3_styled(sk).unwrap_or_default() {
                for path in map.paths3(&line) {
                    painter.extend(style_shapes(path, stroke, style));
                }
            }
            return;
        }
        for (style, line) in se.transform_preview_styled(sk).unwrap_or_default() {
            for path in map.paths(&line) {
                painter.extend(style_shapes(path, stroke, style));
            }
        }
        return;
    }
    // Anordnung: die Exemplare, die Eingabe anlegt; abgewählte gestrichelt und dünn.
    if se.runs_pattern() {
        for (_, suppressed, lines) in se.pattern_preview_all(sk).unwrap_or_default() {
            let stroke = if suppressed {
                Stroke::new(1.0, color)
            } else {
                Stroke::new(CURVE_WIDTH, color)
            };
            for line in lines {
                for path in map.paths(&line) {
                    draw_path(painter, path, stroke, suppressed);
                }
            }
        }
        // Die Mitte des Kreismusters, die erst das Anlegen als Punkt setzt.
        if let Some(at) = se.pattern.new_center.and_then(|hit| map.to_screen(hit.at)) {
            painter.circle_filled(at, 3.0, color);
        }
        return;
    }
    // Text: Rahmen gestrichelt (beim Aufziehen bis zum Zeiger), der Pfad kräftig, der Text, wie
    // OK ihn anlegt.
    if se.runs_text() {
        draw_text_preview(painter, map, sk, se, color);
        return;
    }
    // Zeichnen in 3D (Skizzen 10): vom gesetzten Punkt zum Zeiger in der Ebene der Triade.
    if se.draw3 {
        draw_preview3(painter, map, sk, se, color);
        return;
    }
    // Ohne Zeiger über der Skizze bleibt die Vorschau an der letzten Stelle stehen.
    let Some(cursor) = se.draw_cursor.as_ref().map(|c| c.at).or(se.cursor) else {
        return;
    };
    let stroke = Stroke::new(1.5, color);
    if let Some(lines) = se.offset_preview(sk, cursor) {
        for line in lines {
            for path in map.paths(&line) {
                draw_path(painter, path, stroke, true);
            }
        }
        return;
    }
    if se.line_arc.is_some() {
        for line in se.line_arc_preview(sk, cursor).unwrap_or_default() {
            for path in map.paths(&line) {
                draw_path(painter, path, stroke, true);
            }
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
            for path in map.paths(&line) {
                draw_path(painter, path, stroke, true);
            }
        }
        return;
    }
    // Spline mit Kontrollpunkten: die Kurve durchgezogen, das Polygon gestrichelt (unten).
    for path in se
        .control_spline_preview(sk, cursor)
        .iter()
        .flat_map(|curve| map.paths(curve))
    {
        draw_path(painter, path, stroke, false);
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
    for path in map.paths(&shape) {
        draw_path(painter, path, stroke, true);
    }
}
