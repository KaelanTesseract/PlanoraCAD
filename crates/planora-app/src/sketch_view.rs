//! Skizzen-Modus in der 3D-Ansicht: Abbildung Bildschirm ↔ Skizzenebene, Darstellung der Skizze
//! über der Szene und Weiterleiten der Eingaben an [`SketchSession`]. Die Logik (Werkzeuge, Fang,
//! Abhängigkeiten) liegt in `sketch_state`; hier wird nur gezeichnet und weitergereicht.

use crate::sketch_cursor::AutoConstraint;
use crate::sketch_state::{Item, SketchSession, Snap, Tool};
use crate::state::AppState;
use crate::theme::{self, lighten as lighter, sketch_colors as colors};
use crate::viewport;
use egui::{Color32, FontId, Mesh, PointerButton, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use glam::{Vec2, Vec3};
use planora_i18n::tr;
use planora_math::{DVec2, DVec3, Frame};
use planora_render::{
    EdgeRef, OrbitCamera, Pick, exact_step, grid_radius, line_weight, visible_step,
};
use planora_sketch::ExtractedProfiles;
use planora_sketch::constraint::{ConstraintKind, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::{SolveReport, SolveStatus};
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
/// Halbe Größe der Fangzeichen in Pixeln.
const SNAP_MARK_PX: f32 = 5.0;
/// Strichstärke der Fangzeichen in Pixeln.
const SNAP_MARK_WIDTH: f32 = 1.5;
/// Höchstzahl der Rasterlinien je Bild; darüber zeichnet das Raster nur die gröbere Stufe.
const GRID_MAX_LINES: f64 = 200.0;
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
        ConstraintKind::Angle(..) | ConstraintKind::EllipseAngle(..)
    )
}

/// Gespeicherter Wert eines Maßes.
pub fn stored_value(kind: &ConstraintKind) -> Option<f64> {
    use ConstraintKind as K;
    match kind {
        K::Distance(_, _, v)
        | K::DistancePointLine(_, _, v)
        | K::Radius(_, v)
        | K::Diameter(_, v)
        | K::Angle(_, _, v)
        | K::EllipseMajor(_, v)
        | K::EllipseMinor(_, v)
        | K::EllipseAngle(_, v) => Some(*v),
        _ => None,
    }
}

/// Anzuzeigender Wert eines Maßes: bei anzeigenden (`driven`) Maßen der gemessene Wert.
pub fn shown_value(sk: &Sketch, kind: &ConstraintKind, driven: bool) -> Option<f64> {
    if driven {
        sk.measure(kind).or_else(|| stored_value(kind))
    } else {
        stored_value(kind)
    }
}

/// Wert mit Einheit (Anzeigeeinheit oder °).
pub fn value_text(kind: &ConstraintKind, v: f64) -> String {
    if is_angle(kind) {
        format_angle(v)
    } else {
        format_length(v)
    }
}

/// Name der Abhängigkeitsart in der aktuellen Sprache.
pub fn kind_label(kind: &ConstraintKind) -> String {
    use ConstraintKind as K;
    match kind {
        K::Coincident(..) => tr!("sketch-constraint-coincident"),
        K::PointOnCurve(..) => tr!("sketch-constraint-point-on-curve"),
        K::Horizontal(..) => tr!("sketch-constraint-horizontal"),
        K::Vertical(..) => tr!("sketch-constraint-vertical"),
        K::Parallel(..) => tr!("sketch-constraint-parallel"),
        K::Perpendicular(..) => tr!("sketch-constraint-perpendicular"),
        K::Tangent(..) => tr!("sketch-constraint-tangent"),
        K::Equal(..) => tr!("sketch-constraint-equal"),
        K::Fixed(..) => tr!("sketch-constraint-fixed"),
        K::Symmetric(..) => tr!("sketch-constraint-symmetric"),
        K::Midpoint(..) => tr!("sketch-constraint-midpoint"),
        K::Distance(..) => tr!("sketch-constraint-distance"),
        K::DistancePointLine(..) => tr!("sketch-constraint-distance-point-line"),
        K::Radius(..) => tr!("sketch-constraint-radius"),
        K::Diameter(..) => tr!("sketch-constraint-diameter"),
        K::Angle(..) => tr!("sketch-constraint-angle"),
        K::ArcRadius(..) => tr!("sketch-constraint-arc-radius"),
        K::EllipticArcEnds(..) => tr!("sketch-constraint-elliptic-arc-ends"),
        K::EllipseMajor(..) => tr!("sketch-constraint-ellipse-major"),
        K::EllipseMinor(..) => tr!("sketch-constraint-ellipse-minor"),
        K::EllipseAngle(..) => tr!("sketch-constraint-ellipse-angle"),
    }
}

/// Eintrag der Abhängigkeitsliste, z. B. „Abstand 40,00 mm“; anzeigende Maße in Klammern.
pub fn constraint_name(sk: &Sketch, kind: &ConstraintKind, driven: bool) -> String {
    let label = kind_label(kind);
    match shown_value(sk, kind, driven) {
        Some(v) if driven => format!("{label} ({})", value_text(kind, v)),
        Some(v) => format!("{label} {}", value_text(kind, v)),
        None => label,
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
    use ConstraintKind as K;
    let plain = match kind {
        K::Coincident(..) | K::ArcRadius(..) | K::EllipticArcEnds(..) => return String::new(),
        K::Horizontal(..) => "H".to_string(),
        K::Vertical(..) => "V".to_string(),
        K::Parallel(..) => parallel_symbol(ctx).to_string(),
        K::Perpendicular(..) => perpendicular_symbol(ctx),
        K::Equal(..) => "=".to_string(),
        K::Tangent(..) => "T".to_string(),
        // Der Punkt im Ursprung ist „deckungsgleich mit dem Ursprung“, kein Festsetzen von Hand.
        K::Fixed(_, x, y) if *x == 0.0 && *y == 0.0 => return String::new(),
        K::Fixed(..) => "fix".to_string(),
        K::Midpoint(..) => tr!("sketch-overlay-midpoint"),
        K::Symmetric(..) => tr!("sketch-overlay-symmetric"),
        K::PointOnCurve(..) => tr!("sketch-overlay-on-curve"),
        _ => String::new(),
    };
    if !plain.is_empty() {
        return plain;
    }
    let Some(v) = shown_value(sk, kind, driven) else {
        return String::new();
    };
    let text = match kind {
        K::Radius(..) => format!("R {}", format_length(v)),
        // Ø ohne Ersatz: Montserrat hat das Zeichen (tests/fonts.rs prüft das), und die
        // Glyphenabfrage von egui meldete es fälschlich als fehlend.
        K::Diameter(..) => format!("Ø {}", format_length(v)),
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

/// Stellen (Skizzenkoordinaten), an denen die Beschriftung einer Abhängigkeit erscheint.
fn anchors(sk: &Sketch, kind: &ConstraintKind) -> Vec<DVec2> {
    use ConstraintKind as K;
    let mid = |a: Option<DVec2>, b: Option<DVec2>| a.zip(b).map(|(a, b)| (a + b) * 0.5);
    let found = match kind {
        K::Horizontal(LineOrPoints::Line(c)) | K::Vertical(LineOrPoints::Line(c)) => {
            vec![curve_anchor(sk, *c)]
        }
        K::Horizontal(LineOrPoints::Points(a, b))
        | K::Vertical(LineOrPoints::Points(a, b))
        | K::Distance(a, b, _) => vec![mid(pos(sk, *a), pos(sk, *b))],
        K::Parallel(a, b) | K::Perpendicular(a, b) | K::Equal(a, b) | K::Tangent(a, b) => {
            vec![curve_anchor(sk, *a), curve_anchor(sk, *b)]
        }
        K::Angle(a, b, _) => vec![mid(curve_anchor(sk, *a), curve_anchor(sk, *b))],
        K::Fixed(p, ..) | K::Midpoint(p, _) | K::PointOnCurve(p, _) | K::Coincident(p, _) => {
            vec![pos(sk, *p)]
        }
        K::Symmetric(a, b, _) => vec![pos(sk, *a), pos(sk, *b)],
        K::DistancePointLine(p, c, _) => {
            let at = pos(sk, *p);
            vec![mid(at, at.and_then(|q| sk.nearest_on_curve(*c, q)))]
        }
        K::Radius(c, _)
        | K::Diameter(c, _)
        | K::ArcRadius(c)
        | K::EllipticArcEnds(c)
        | K::EllipseMajor(c, _)
        | K::EllipseMinor(c, _)
        | K::EllipseAngle(c, _) => vec![curve_anchor(sk, *c)],
    };
    found.into_iter().flatten().collect()
}

// ---- Overlay ----------------------------------------------------------------------------------

/// Abbildung Skizze → Bildschirm für ein Bild (Kamera, Ebene und Viewport festgehalten).
struct Mapper {
    camera: OrbitCamera,
    frame: Frame,
    rect: Rect,
}

impl Mapper {
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
                Some(if pick.hover == Some(i) {
                    col.profile_fill_hover
                } else if pick.selected.contains(&i) {
                    col.profile_fill_selected
                } else {
                    col.profile_fill
                })
            }));
        }
        for (c, data) in sk.curves() {
            let Some(pl) = sk.curve_polyline(c, STEPS_PER_TURN) else {
                continue;
            };
            let stroke = if data.construction {
                Stroke::new(FINISHED_CONSTRUCTION_WIDTH, col.construction)
            } else {
                Stroke::new(FINISHED_WIDTH, col.finished)
            };
            for path in map.paths(&pl) {
                out.extend(path_shapes(path, stroke, data.construction));
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

    /// Stufe des Raster-Fangs: die sichtbare ([`visible_step`]); ist das Raster ausgedünnt, bleiben
    /// nur die Linien der gröberen Stufe zu sehen, also rastet auch der Zeiger nur dort.
    fn snap_step(&self, distance: f32) -> f64 {
        if self.every > 1 {
            self.fine * self.every as f64
        } else {
            visible_step(distance, planora_i18n::current_unit().mm_per_unit())
        }
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
            let (share, alpha) = line_weight(k, blend);
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
    handle_input(ui, &response, rect, state);
    let pointer = response.hover_pos().or(response.interact_pointer_pos());
    if crate::sketch_inline::show(ui, rect, state, pointer) {
        cursor_after_fields(rect, state, pointer);
    }
    draw(ui, rect, state, response.hover_pos());
    response.hovered()
}

/// Die Abbildung auf die Skizze im Rechteck `rect` für dieses Bild.
fn mapper(rect: Rect, state: &AppState) -> Option<Mapper> {
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
    if se.tool != Tool::Select
        || (pan_gesture && !shift_only)
        || crate::keymap::settings_ui::is_capturing(ui.ctx())
    {
        return false;
    }
    let Some(map) = mapper(rect, state) else {
        return false;
    };
    ui.input(|i| i.pointer.press_origin()).is_some_and(|p| {
        match (map.to_sketch(p), map.radius_at(p)) {
            (Some(at), Some(radius)) => se.is_free_at(sk, at, radius),
            _ => true,
        }
    })
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
    let starts_box = response.drag_started_by(PointerButton::Primary)
        && box_can_start(ui, rect, state, pan_gesture, ctrl);
    // Ein laufender Rahmen schwenkt nicht, auch wenn Umschalt erst währenddessen dazukommt.
    let boxing = starts_box
        || state
            .sketch_session
            .as_ref()
            .is_some_and(|s| s.box_select.is_some());
    let panning = pan_gesture && !boxing;
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
        GridPlan::of(&map).map(|plan| plan.snap_step(map.camera.distance))
    } else {
        None
    };

    // Getippte Werte werden beim Klick zu Maßen; die Formeln darin brauchen die Parameter.
    let commit_scope = (response.clicked_by(PointerButton::Primary)
        && state
            .sketch_session
            .as_ref()
            .is_some_and(SketchSession::is_drawing_tool))
    .then(|| state.document.param_scope());
    let Some((se, sf)) = state.session_draft_mut() else {
        return;
    };
    if let Some(at) = at {
        se.cursor = Some(at);
    }
    se.cursor_opts.suppressed = ctrl;
    se.cursor_opts.grid_step = grid_step;

    // Rahmen auf freier Fläche; alles andere greift (Punkt vor Kurve vor Rahmen).
    if starts_box {
        if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
            se.begin_box(dvec(origin), shift);
        }
    } else if response.drag_started_by(PointerButton::Primary) && !shift && !panning {
        // Punkt oder Kurve ziehen (Auswahlwerkzeug)
        let origin = ui.input(|i| i.pointer.press_origin());
        if let Some(start) = origin.and_then(|p| map.to_sketch(p)) {
            let r = origin.and_then(|p| map.radius_at(p)).unwrap_or(radius);
            se.begin_drag(&mut sf.sketch, start, r);
        }
    }
    if se.is_dragging() {
        if response.dragged_by(PointerButton::Primary)
            && let Some(at) = at
        {
            se.drag_to(&mut sf.sketch, at);
        }
        if response.drag_stopped() || !response.dragged() {
            se.end_drag(&mut sf.sketch);
        }
    }

    let esc = !viewport::typing(ui.ctx()) && ui.input(|i| i.key_pressed(egui::Key::Escape));
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
                se.end_box(&sf.sketch, &|q| map.to_screen(q).map(dvec));
            }
        }
    }

    // Stutzen durch Ziehen: jede überfahrene Kurve wird gestutzt (ein Schritt, da der Verlauf
    // erst nach dem Loslassen fortgeschrieben wird).
    if se.tool == Tool::Trim && !panning {
        if response.drag_started_by(PointerButton::Primary) && !shift {
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
    // egui zählt Klicks nur als Doppel-/Dreifachklick, wenn sie höchstens `max_click_dist`
    // (6 Pixel) vom vorigen Klick entfernt sind. Ein „Dreifachklick“ entsteht schon, wenn zwei
    // schnelle Klicks an derselben Stelle einem Klick anderswo folgen (Zeit zum vorletzten Klick,
    // Abstand nur zum letzten) – das ist für den Nutzer ein Doppelklick und beendet ebenso.
    if se.box_aborted {
        // Das Loslassen nach dem Abbruch ist kein Klick.
    } else if response.double_clicked() || response.triple_clicked() {
        if se.tool == Tool::Select {
            // Wie beim einfachen Klick: Punkt vor Kurve.
            if let Some(at) = at
                && let Snap::Curve(c) = se.snap(&sf.sketch, at, radius)
            {
                se.select_chain(&sf.sketch, c, shift);
            }
        } else {
            let _ = se.finish_tool(&mut sf.sketch);
        }
    } else if response.clicked_by(PointerButton::Primary)
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
    if response.clicked_by(PointerButton::Secondary) && !se.box_aborted {
        se.cancel(&mut sf.sketch);
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
            se.cancel(&mut sf.sketch);
        }
        if enter {
            let _ = se.finish_tool(&mut sf.sketch);
        }
        if delete {
            se.delete_selection(&mut sf.sketch);
        }
    }
    // Vorschau von Stutzen, Dehnen und Brechen unter dem Mauszeiger (nach den Änderungen
    // dieses Bilds).
    let hover_at = response.hover_pos().and_then(|p| map.to_sketch(p));
    se.update_hover(&sf.sketch, hover_at, radius);
    // Aufgelöster Zeiger für Zeichen und Vorschau (nach den Änderungen dieses Bilds).
    let raw = cursor_raw(se, at);
    se.update_cursor(&sf.sketch, raw, radius);
    let tool = se.tool;

    // Körperkante kopieren: Kante in der 3D-Szene wählen.
    let hover = if tool == Tool::ProjectEdge {
        pointer
            .and_then(|p| state.pick(map.local(p), map.viewport()))
            .filter(|h| matches!(h, Pick::Edge(_)))
    } else {
        None
    };
    state.set_hover(hover);
    if let Some(p) = project_at {
        match state.pick(map.local(p), map.viewport()) {
            // Fehler (etwa ein Körper, der erst nach der Skizze entsteht) stehen in der
            // Meldung der Sitzung.
            Some(Pick::Edge(EdgeRef { body, edge })) => {
                let _ = state.project_edge_into_sketch(body, edge);
            }
            _ => {
                if let Some(se) = state.sketch_session.as_mut() {
                    se.message = Some(crate::msg!("sketch-pick-edge"));
                }
            }
        }
    }
}

/// Ist der Punkt vollständig bestimmt (laut letztem Bericht)?
fn point_determined(report: Option<&SolveReport>, p: PointId) -> bool {
    report.is_some_and(|r| r.status == SolveStatus::Solved && !r.free_points.contains(&p))
}

fn curve_determined(report: Option<&SolveReport>, c: CurveId) -> bool {
    report.is_some_and(|r| r.status == SolveStatus::Solved && !r.free_curves.contains(&c))
}

fn draw(ui: &egui::Ui, rect: Rect, state: &AppState, pointer: Option<Pos2>) {
    let (Some(se), Some(sk)) = (state.sketch_session.as_ref(), state.session_sketch()) else {
        return;
    };
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
        painter.extend(profile_fill_shape(&map, &extracted, |_| {
            Some(col.profile_fill)
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
    } else {
        pointer
            .filter(|_| !se.is_dragging())
            .and_then(|p| Some((map.to_sketch(p)?, map.radius_at(p)?)))
            .and_then(|(at, r)| match se.snap(sk, at, r) {
                Snap::Point(p) => Some(Item::Point(p)),
                Snap::Curve(c) => Some(Item::Curve(c)),
                _ => None,
            })
    };

    // Kurven
    for (c, data) in sk.curves() {
        let Some(pl) = sk.curve_polyline(c, STEPS_PER_TURN) else {
            continue;
        };
        let selected = se.selection.contains(&Item::Curve(c));
        let mut color = if selected {
            col.selected
        } else if data.construction {
            col.construction
        } else if curve_determined(report, c) {
            col.determined
        } else {
            col.free
        };
        if hovered == Some(Item::Curve(c)) {
            color = lighter(color);
        }
        let width = if data.construction { 1.2 } else { CURVE_WIDTH };
        for path in map.paths(&pl) {
            draw_path(&painter, path, Stroke::new(width, color), data.construction);
        }
    }

    // Punkte
    for (p, _) in sk.points() {
        let Some(s) = pos(sk, p).and_then(|q| map.to_screen(q)) else {
            continue;
        };
        let selected = se.selection.contains(&Item::Point(p)) || se.dragging == Some(p);
        let determined = point_determined(report, p);
        let mut color = if selected {
            col.selected
        } else if determined {
            col.determined
        } else {
            col.free
        };
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

    draw_preview(&painter, &map, sk, se, col.preview);

    // Stutzen/Brechen: das Stück rot; Dehnen: der Weg zum neuen Ende rot gestrichelt.
    if let Some(pl) = &se.hover_preview {
        let stroke = Stroke::new(CURVE_WIDTH + 1.0, col.trim_highlight);
        for path in map.paths(pl) {
            draw_path(&painter, path, stroke, se.tool == Tool::Extend);
        }
    }

    // Abhängigkeiten und Maße
    let mut placed: Vec<Rect> = Vec::new();
    let bad: Vec<_> = report
        .map(|r| r.conflicting.iter().chain(&r.redundant).copied().collect())
        .unwrap_or_default();
    for (id, c) in sk.constraints() {
        let text = overlay_text(ui.ctx(), sk, &c.kind, c.driven);
        if text.is_empty() {
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
        for a in anchors(sk, &c.kind) {
            if let Some(s) = map.to_screen(a) {
                label(
                    &painter,
                    s + vec2(6.0, -18.0),
                    &text,
                    color,
                    col.label_bg,
                    &mut placed,
                );
            }
        }
    }

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
        if let Some(s) = cur.snap_at.and_then(|q| map.to_screen(q)) {
            paint_snap_mark(&painter, s, cur.snap, col.snap);
        }
    }

    // Fanghinweis unter dem Mauszeiger in den übrigen Werkzeugen
    if se.tool != Tool::Select
        && !se.is_drawing_tool()
        && let Some(Item::Point(p)) = hovered
        && let Some(s) = pos(sk, p).and_then(|q| map.to_screen(q))
    {
        painter.circle_stroke(s, POINT_PX + 5.0, Stroke::new(1.5, col.selected));
    }

    painter.extend(box_select_shapes(state));
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
fn label(
    painter: &egui::Painter,
    at: Pos2,
    text: &str,
    color: Color32,
    bg: Color32,
    placed: &mut Vec<Rect>,
) {
    /// Höchstens so oft wird einer Überlappung ausgewichen.
    const MAX_SHIFTS: usize = 8;
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(LABEL_SIZE), color);
    let mut r = Rect::from_min_size(at, galley.size()).expand(2.0);
    for _ in 0..MAX_SHIFTS {
        if !placed.iter().any(|q| q.intersects(r)) {
            break;
        }
        r = r.translate(vec2(0.0, r.height() + 1.0));
    }
    placed.push(r);
    painter.rect_filled(r, 3.0, bg);
    painter.galley(r.min + vec2(2.0, 2.0), galley, color);
}

/// Vorschau des laufenden Werkzeugs vom zuletzt gesetzten Punkt zum Mauszeiger.
fn draw_preview(
    painter: &egui::Painter,
    map: &Mapper,
    sk: &Sketch,
    se: &SketchSession,
    color: Color32,
) {
    // Ohne Zeiger über der Skizze bleibt die Vorschau an der letzten Stelle stehen.
    let Some(cursor) = se.draw_cursor.as_ref().map(|c| c.at).or(se.cursor) else {
        return;
    };
    let stroke = Stroke::new(1.5, color);
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
            let sweep = (a1 - a0).rem_euclid(TAU);
            let steps = ((sweep / TAU * STEPS_PER_TURN as f64).ceil() as usize).max(1);
            (0..=steps)
                .map(|k| {
                    let t = a0 + sweep * k as f64 / steps as f64;
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
