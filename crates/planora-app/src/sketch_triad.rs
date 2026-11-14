//! Triade der 3D-Skizze (Skizzen 10, Fusions „3D Sketch Manipulator“): Lage und Drehung der
//! Zeichenebene in Skizzenkoordinaten, ihr Bild auf dem Bildschirm (Ebenenquadrate, Achsen,
//! Drehbögen), Treffer, Achsfang und Z-Sperre für das Setzen von Punkten ([`triad_point`]) sowie
//! die Bedienung in der Ansicht: Klick auf ein Quadrat, Ziehen am Drehbogen mit Winkelfang,
//! getippter Winkel am aktiven Drehgriff (Tab) und das Rechtsklickmenü.
//!
//! Die Triade gehört zur Sitzung, nicht zum Dokument und nicht zum Rückgängig (Spec Abschnitt 4).

use crate::sketch_state::{SketchSession, Tool};
use crate::sketch_view::{Mapper, format_angle, parse_number, sketch3_to_screen};
use crate::state::AppState;
use crate::theme::{SketchColors, lighten, with_alpha};
use crate::viewport;
use egui::{Color32, FontId, PointerButton, Pos2, Rect, Shape, Stroke, vec2};
use glam::Vec2;
use planora_i18n::tr;
use planora_math::{DQuat, DVec3, Frame};
use planora_render::OrbitCamera;
use planora_sketch::model::{MAX_COORD, Sketch};
use std::f64::consts::FRAC_PI_2;

/// Länge der Achsen der Triade in Bildpunkten (fest, unabhängig vom Zoom).
pub const TRIAD_SIZE_PX: f32 = 90.0;
/// So nah muss der Zeiger an einem Teil der Triade liegen, in Bildpunkten.
pub const TRIAD_HIT_PX: f32 = 8.0;
/// So nah muss der Zeiger am Bild einer Achse der aktiven Ebene liegen, damit der Punkt auf ihr
/// einrastet, in Bildpunkten.
pub const AXIS_SNAP_PX: f32 = 8.0;
/// Lage der Ebenenquadrate entlang ihrer beiden Achsen, als Anteil der Achslänge.
pub const SQUARE_FROM: f32 = 0.18;
/// Äußere Kante der Ebenenquadrate, als Anteil der Achslänge.
pub const SQUARE_TO: f32 = 0.45;
/// Radius der Drehbögen, als Anteil der Achslänge.
pub const ARC_FRAC: f32 = 0.75;
/// Abschnitte je Drehbogen (Viertelkreis).
const ARC_STEPS: usize = 16;
/// Winkelfang beim Ziehen am Drehbogen: Stufen von 15°, die bis 3° heran einrasten; dazwischen
/// ganze Grad. Strg schaltet den Fang aus. *(Claudes Festlegung, Fusion fängt ebenfalls an
/// runden Winkeln.)*
pub const ANGLE_SNAP_STEP_DEG: f64 = 15.0;
/// Fangweite des Winkelfangs in Grad.
pub const ANGLE_SNAP_CATCH_DEG: f64 = 3.0;
/// Toleranz für „unverdreht“ und „z = 0“ in [`Triad::is_sketch_plane`].
const SKETCH_PLANE_TOL: f64 = 1e-9;
/// Strichstärken der Triade in Bildpunkten: normal und hervorgehoben.
const STROKE_PX: (f32, f32) = (2.0, 3.5);
/// Radius des Knopfs in der Mitte jedes Drehbogens und der Mitte der Triade.
const KNOB_PX: f32 = 4.0;
/// Deckkraft der Ebenenquadrate: inaktiv, aktiv, unter dem Zeiger.
const PLANE_ALPHA: (u8, u8, u8) = (0x30, 0x80, 0xa0);
/// Schriftgröße des Winkelfelds beim Ziehen.
const LABEL_SIZE: f32 = 12.0;
/// Abstand des Winkelfelds vom Zeiger bzw. von der Mitte der Triade.
const LABEL_GAP: f32 = 18.0;
/// Breite des Eingabefelds für den Winkel.
const FIELD_WIDTH: f32 = 56.0;

// ---- Zustand --------------------------------------------------------------------------------------

/// Ebene der Triade (Skizzen 10). Die Reihenfolge von Alt ist XY, YZ, ZX.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TriadPlane {
    /// Die Skizzenebene.
    #[default]
    Xy,
    Yz,
    Zx,
}

impl TriadPlane {
    /// Alle Ebenen in der Reihenfolge von Alt.
    pub const ALL: [TriadPlane; 3] = [TriadPlane::Xy, TriadPlane::Yz, TriadPlane::Zx];

    /// Die nächste Ebene reihum.
    pub fn next(self) -> TriadPlane {
        match self {
            TriadPlane::Xy => TriadPlane::Yz,
            TriadPlane::Yz => TriadPlane::Zx,
            TriadPlane::Zx => TriadPlane::Xy,
        }
    }

    /// Die beiden Achsen in der Ebene (in der Reihenfolge des Namens).
    pub fn axes(self) -> (TriadAxis, TriadAxis) {
        match self {
            TriadPlane::Xy => (TriadAxis::X, TriadAxis::Y),
            TriadPlane::Yz => (TriadAxis::Y, TriadAxis::Z),
            TriadPlane::Zx => (TriadAxis::Z, TriadAxis::X),
        }
    }

    /// Die Achse senkrecht zur Ebene (ihre Normale).
    pub fn normal_axis(self) -> TriadAxis {
        match self {
            TriadPlane::Xy => TriadAxis::Z,
            TriadPlane::Yz => TriadAxis::X,
            TriadPlane::Zx => TriadAxis::Y,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Achse der Triade, auch die eines Drehgriffs (Skizzen 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriadAxis {
    X,
    Y,
    Z,
}

impl TriadAxis {
    /// Alle Achsen.
    pub const ALL: [TriadAxis; 3] = [TriadAxis::X, TriadAxis::Y, TriadAxis::Z];

    /// Der nächste Griff reihum (`None`: noch keiner aktiv, dann X).
    pub fn next(current: Option<TriadAxis>) -> TriadAxis {
        match current {
            None | Some(TriadAxis::Z) => TriadAxis::X,
            Some(TriadAxis::X) => TriadAxis::Y,
            Some(TriadAxis::Y) => TriadAxis::Z,
        }
    }

    /// Einheitsvektor der Achse in den eigenen Koordinaten der Triade.
    pub fn unit(self) -> DVec3 {
        match self {
            TriadAxis::X => DVec3::X,
            TriadAxis::Y => DVec3::Y,
            TriadAxis::Z => DVec3::Z,
        }
    }

    /// Die beiden anderen Achsen, im Drehsinn um diese (X: Y→Z, Y: Z→X, Z: X→Y).
    fn others(self) -> (TriadAxis, TriadAxis) {
        match self {
            TriadAxis::X => (TriadAxis::Y, TriadAxis::Z),
            TriadAxis::Y => (TriadAxis::Z, TriadAxis::X),
            TriadAxis::Z => (TriadAxis::X, TriadAxis::Y),
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    /// Name des Winkelfelds dieses Drehgriffs („X-Winkel“ …).
    pub fn angle_label(self) -> String {
        match self {
            TriadAxis::X => tr!("sketch-triad-angle-x"),
            TriadAxis::Y => tr!("sketch-triad-angle-y"),
            TriadAxis::Z => tr!("sketch-triad-angle-z"),
        }
    }
}

/// Die Triade: Ursprung und Drehung in Skizzenkoordinaten, aktive Ebene, aktiver Drehgriff (Tab)
/// und die Z-Sperre (Pfeil hoch gehalten). `Default`: am Skizzenursprung, unverdreht, XY aktiv.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triad {
    pub origin: DVec3,
    pub rot: DQuat,
    pub active: TriadPlane,
    pub handle: Option<TriadAxis>,
    pub lock_z: bool,
}

impl Default for Triad {
    fn default() -> Self {
        Triad {
            origin: DVec3::ZERO,
            rot: DQuat::IDENTITY,
            active: TriadPlane::Xy,
            handle: None,
            lock_z: false,
        }
    }
}

impl Triad {
    /// Die aktive Ebene: ein Punkt (der Ursprung) und ihre Normale, in Skizzenkoordinaten.
    pub fn plane(&self) -> (DVec3, DVec3) {
        (self.origin, self.axes()[self.active.normal_axis().index()])
    }

    /// Die drei Achsen der Triade in Skizzenkoordinaten (normiert).
    pub fn axes(&self) -> [DVec3; 3] {
        TriadAxis::ALL.map(|a| self.rot * a.unit())
    }

    /// Die Achse `a` in Skizzenkoordinaten.
    pub fn axis(&self, a: TriadAxis) -> DVec3 {
        self.rot * a.unit()
    }

    /// Ist die aktive Ebene die Skizzenebene (XY, unverdreht, Ursprung auf z = 0)? Dann entsteht
    /// beim Zeichnen normale 2D-Geometrie (Spec Abschnitt 4).
    pub fn is_sketch_plane(&self) -> bool {
        self.active == TriadPlane::Xy
            && self.rot.xyz().length() < SKETCH_PLANE_TOL
            && self.origin.z.abs() < SKETCH_PLANE_TOL
    }

    /// Alt allein: die nächste Ebene reihum.
    pub fn cycle_plane(&mut self) {
        self.active = self.active.next();
    }

    /// Tab: der nächste Drehgriff reihum.
    pub fn cycle_handle(&mut self) {
        self.handle = Some(TriadAxis::next(self.handle));
    }

    /// Dreht die Triade um ihre eigene Achse `axis` um `angle` (Radiant, rechtshändig).
    pub fn rotate(&mut self, axis: TriadAxis, angle: f64) {
        if angle.is_finite() {
            self.rot = (self.rot * DQuat::from_axis_angle(axis.unit(), angle)).normalize();
        }
    }

    /// „Triadenausrichtung zurücksetzen“: die Achsen der Skizze, Ursprung und aktive Ebene bleiben.
    pub fn reset_orientation(&mut self) {
        self.rot = DQuat::IDENTITY;
    }

    /// Nach einem gesetzten Punkt springt die Triade auf ihn.
    pub fn moved_to(&mut self, p: DVec3) {
        if p.is_finite() {
            self.origin = p;
        }
    }
}

/// Ist die Triade zu sehen? Nur in einer 3D-Skizze und mit einem der 3D-Werkzeuge, jeweils in
/// allen Arten: Linie, Bogen, Spline (beide Arten und Kegelschnitt), Rechteck, Kreis, Ellipse,
/// Punkt, Text, Polygon und Langloch (Spec Abschnitt 4, Task H).
pub fn triad_visible(se: &SketchSession, sk: &Sketch) -> bool {
    sk.three_d
        && matches!(
            se.tool,
            Tool::Line
                | Tool::Point
                | Tool::Ellipse
                | Tool::Text
                | Tool::Spline
                | Tool::Arc
                | Tool::Rectangle
                | Tool::Circle
                | Tool::Polygon
                | Tool::Slot
        )
}

/// Winkelfang am Drehbogen: nahe an einer Stufe von 15° (bis 3°) die Stufe, sonst ganze Grad;
/// `free` (Strg) lässt den Winkel, wie er ist.
pub fn snap_angle(raw: f64, free: bool) -> f64 {
    if free || !raw.is_finite() {
        return raw;
    }
    let deg = raw.to_degrees();
    let step = (deg / ANGLE_SNAP_STEP_DEG).round() * ANGLE_SNAP_STEP_DEG;
    if (deg - step).abs() <= ANGLE_SNAP_CATCH_DEG {
        step.to_radians()
    } else {
        deg.round().to_radians()
    }
}

// ---- Abbildung -------------------------------------------------------------------------------------

/// Sehstrahl durch `pixel` (Ursprung oben links im Viewport `viewport`) in Skizzenkoordinaten:
/// Anfang und normierte Richtung.
fn sketch_ray(
    camera: &OrbitCamera,
    frame: &Frame,
    viewport: Vec2,
    pixel: Vec2,
) -> Option<(DVec3, DVec3)> {
    if viewport.x < 1.0 || viewport.y < 1.0 || !pixel.is_finite() {
        return None;
    }
    let ndc = Vec2::new(
        pixel.x / viewport.x * 2.0 - 1.0,
        1.0 - pixel.y / viewport.y * 2.0,
    );
    let (eye, dir) = camera.ray(ndc);
    let eye = eye.as_dvec3();
    let o = frame.to_local(eye);
    let d = (frame.to_local(eye + dir.as_dvec3()) - o).try_normalize()?;
    o.is_finite().then_some((o, d))
}

/// Skizzeneinheiten je Bildpunkt an der Stelle `p` (quer zum Blick).
fn units_per_px(camera: &OrbitCamera, frame: &Frame, viewport: Vec2, p: DVec3) -> Option<f64> {
    let s = sketch3_to_screen(camera, frame, viewport, p)?;
    let (_, d) = sketch_ray(camera, frame, viewport, s)?;
    let (o2, d2) = sketch_ray(camera, frame, viewport, s + Vec2::X)?;
    let denom = d2.dot(d);
    if denom.abs() < 1e-9 {
        return None;
    }
    let t = (p - o2).dot(d) / denom;
    let u = (o2 + d2 * t).distance(p);
    (u.is_finite() && u > 0.0).then_some(u)
}

/// Abstand von `p` zur Geraden durch `a` und `b` (unbegrenzt) auf dem Bildschirm.
fn line_distance(p: Vec2, a: Vec2, b: Vec2) -> Option<f32> {
    let ab = b - a;
    let len = ab.length();
    (len >= 1.0).then(|| ab.perp_dot(p - a).abs() / len)
}

/// Punkt für das Zeichnen in 3D unter dem Bildpunkt `pixel`, in Skizzenkoordinaten, und die
/// Triadenachse, auf der er eingerastet ist (Achsfang).
///
/// - Ohne `lock_from`: der Sehstrahl geschnitten mit der aktiven Ebene der Triade. Liegt das Bild
///   des Zeigers höchstens [`AXIS_SNAP_PX`] neben dem Bild einer der beiden Achsen der Ebene, liegt
///   der Punkt genau auf dieser Achse. `None` bei streifendem Blick (unter
///   [`crate::sketch_view::MIN_PICK_ANGLE`]) oder über `MAX_COORD`.
/// - Mit `lock_from` (Z-Sperre, Pfeil hoch): Der Punkt bewegt sich nur auf der Geraden durch
///   `lock_from` entlang der Ebenennormale, an der Stelle, die dem Sehstrahl am nächsten liegt.
///   `None`, wenn der Blick fast entlang der Normale geht (unter `MIN_PICK_ANGLE`, dann ist keine
///   Höhe abzulesen), die Stelle hinter der Kamera liegt oder über `MAX_COORD` geht. Einen Achsfang
///   gibt es dabei nicht.
pub fn triad_point(
    t: &Triad,
    camera: &OrbitCamera,
    frame: &Frame,
    viewport: Vec2,
    pixel: Vec2,
    lock_from: Option<DVec3>,
) -> Option<(DVec3, Option<TriadAxis>)> {
    let (origin, n) = t.plane();
    let inside = |p: DVec3| p.is_finite() && p.abs().max_element() <= MAX_COORD;
    if let Some(base) = lock_from {
        if !base.is_finite() {
            return None;
        }
        let (o, d) = sketch_ray(camera, frame, viewport, pixel)?;
        let b = d.dot(n);
        let denom = 1.0 - b * b;
        let min = crate::sketch_view::MIN_PICK_ANGLE.sin();
        if !denom.is_finite() || denom < min * min {
            return None;
        }
        let w = base - o;
        let (dn, dw) = (n.dot(w), d.dot(w));
        let s = (b * dw - dn) / denom;
        let along_ray = dw + b * s;
        if !along_ray.is_finite() || along_ray < 0.0 {
            return None;
        }
        let p = base + n * s;
        return inside(p).then_some((p, None));
    }
    let p = crate::sketch_view::screen_to_plane(camera, frame, viewport, pixel, origin, n)?;
    // Achsfang: die nähere der beiden Achsen der Ebene, wenn ihr Bild nah genug am Zeiger liegt.
    let snapped = (|| {
        let s0 = sketch3_to_screen(camera, frame, viewport, origin)?;
        let reach = units_per_px(camera, frame, viewport, origin)? * f64::from(TRIAD_SIZE_PX);
        let (u, v) = t.active.axes();
        [u, v]
            .into_iter()
            .filter_map(|a| {
                let dir = t.axis(a);
                let s1 = sketch3_to_screen(camera, frame, viewport, origin + dir * reach)?;
                let dist = line_distance(pixel, s0, s1)?;
                (dist <= AXIS_SNAP_PX).then_some((dist, a, dir))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
    })();
    match snapped {
        Some((_, a, dir)) => {
            let q = origin + dir * (p - origin).dot(dir);
            inside(q).then_some((q, Some(a)))
        }
        None => Some((p, None)),
    }
}

// ---- Bild und Treffer --------------------------------------------------------------------------------

/// Teil der Triade unter dem Zeiger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriadPart {
    Plane(TriadPlane),
    Handle(TriadAxis),
    Axis(TriadAxis),
}

/// Bild der Triade auf dem Bildschirm. Fehlt ein Teil (hinter der Kamera), ist er `None` bzw. leer.
#[derive(Clone, Debug, PartialEq)]
pub struct TriadLayout {
    /// Bild des Ursprungs.
    pub center: Pos2,
    /// Spitzen der Achsen X, Y, Z.
    pub tips: [Option<Pos2>; 3],
    /// Ecken der Ebenenquadrate XY, YZ, ZX.
    pub squares: [Option<[Pos2; 4]>; 3],
    /// Drehbögen um X, Y, Z (Viertelkreise zwischen den beiden anderen Achsen).
    pub arcs: [Vec<Pos2>; 3],
    /// „Ebenen anzeigen“: Ohne sie werden die Quadrate weder gezeichnet noch getroffen.
    pub show_planes: bool,
    /// „Drehgriffe anzeigen“: ebenso für die Drehbögen.
    pub show_handles: bool,
}

/// Bild der Triade `t` mit Achsen von `size_px` Bildpunkten; `None`, wenn ihr Ursprung hinter der
/// Kamera liegt oder die Ansicht keinen Maßstab hat. Ebenen und Drehgriffe sind eingeschaltet; die
/// Schalter setzt der Aufrufer.
pub fn triad_layout(t: &Triad, mapper: &Mapper, size_px: f32) -> Option<TriadLayout> {
    let viewport = Vec2::new(mapper.rect.width(), mapper.rect.height());
    let center = mapper.to_screen3(t.origin)?;
    let len = units_per_px(&mapper.camera, &mapper.frame, viewport, t.origin)? * f64::from(size_px);
    let axes = t.axes();
    let at = |v: DVec3| mapper.to_screen3(t.origin + v * len);
    let tips = axes.map(at);
    let squares = TriadPlane::ALL.map(|plane| {
        let (u, v) = plane.axes();
        let (u, v) = (axes[u.index()], axes[v.index()]);
        let (a, b) = (f64::from(SQUARE_FROM), f64::from(SQUARE_TO));
        Some([
            at(u * a + v * a)?,
            at(u * b + v * a)?,
            at(u * b + v * b)?,
            at(u * a + v * b)?,
        ])
    });
    let arcs = TriadAxis::ALL.map(|axis| {
        let (a, b) = axis.others();
        let (a, b) = (axes[a.index()], axes[b.index()]);
        let r = f64::from(ARC_FRAC);
        (0..=ARC_STEPS)
            .map(|i| {
                let th = FRAC_PI_2 * i as f64 / ARC_STEPS as f64;
                at((a * th.cos() + b * th.sin()) * r)
            })
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default()
    });
    Some(TriadLayout {
        center,
        tips,
        squares,
        arcs,
        show_planes: true,
        show_handles: true,
    })
}

/// Abstand des Punkts `p` zur Strecke `a`–`b`.
fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 {
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * t).distance(p)
}

fn polyline_distance(p: Pos2, path: &[Pos2]) -> Option<f32> {
    path.windows(2)
        .map(|w| segment_distance(p, w[0], w[1]))
        .min_by(f32::total_cmp)
}

/// Liegt `p` im Vieleck `q` (gerade-ungerade-Regel)?
fn inside_polygon(p: Pos2, q: &[Pos2]) -> bool {
    let mut inside = false;
    let mut j = q.len().wrapping_sub(1);
    for i in 0..q.len() {
        let (a, b) = (q[i], q[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Der Teil der Triade unter `pos`: zuerst die Ebenenquadrate, dann die Drehbögen, dann die
/// Achsen (jeweils der nächste, höchstens [`TRIAD_HIT_PX`] entfernt). Ausgeblendete Teile zählen
/// nicht.
pub fn triad_hit(l: &TriadLayout, pos: Pos2) -> Option<TriadPart> {
    if l.show_planes
        && let Some(plane) = TriadPlane::ALL
            .into_iter()
            .find(|p| l.squares[p.index()].is_some_and(|q| inside_polygon(pos, &q)))
    {
        return Some(TriadPart::Plane(plane));
    }
    let nearest = |dist: &dyn Fn(TriadAxis) -> Option<f32>| {
        TriadAxis::ALL
            .into_iter()
            .filter_map(|a| dist(a).filter(|d| *d <= TRIAD_HIT_PX).map(|d| (d, a)))
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, a)| a)
    };
    if l.show_handles
        && let Some(a) = nearest(&|a| polyline_distance(pos, &l.arcs[a.index()]))
    {
        return Some(TriadPart::Handle(a));
    }
    nearest(&|a| l.tips[a.index()].map(|tip| segment_distance(pos, l.center, tip)))
        .map(TriadPart::Axis)
}

/// Liegt `pos` auf der Triade (ein Teil oder ihre Mitte)? Für das Rechtsklickmenü.
fn over_triad(l: &TriadLayout, pos: Pos2) -> bool {
    triad_hit(l, pos).is_some() || l.center.distance(pos) <= TRIAD_HIT_PX
}

/// Farbe einer Achse aus dem Thema (rot, grün, blau wie die Ursprungsachsen).
fn axis_color(col: &SketchColors, a: TriadAxis) -> Color32 {
    match a {
        TriadAxis::X => col.axis_x,
        TriadAxis::Y => col.axis_y,
        TriadAxis::Z => col.axis_z,
    }
}

/// Zeichnet die Triade: Ebenenquadrate in der Farbe ihrer Normale (die aktive kräftiger), Achsen
/// und Drehbögen in den Achsfarben. `hot` (unter dem Zeiger) und der aktive Drehgriff sind heller
/// und breiter; `dragging` ist der Drehgriff im Zug.
pub fn triad_paint(
    painter: &egui::Painter,
    l: &TriadLayout,
    t: &Triad,
    hot: Option<TriadPart>,
    dragging: Option<TriadAxis>,
    col: &SketchColors,
) {
    if l.show_planes {
        for plane in TriadPlane::ALL {
            let Some(q) = l.squares[plane.index()] else {
                continue;
            };
            let base = axis_color(col, plane.normal_axis());
            let is_hot = hot == Some(TriadPart::Plane(plane));
            let alpha = if is_hot {
                PLANE_ALPHA.2
            } else if plane == t.active {
                PLANE_ALPHA.1
            } else {
                PLANE_ALPHA.0
            };
            let fill = with_alpha(base, alpha);
            let edge = if plane == t.active || is_hot {
                Stroke::new(1.5, lighten(base))
            } else {
                Stroke::new(1.0, fill)
            };
            painter.add(Shape::convex_polygon(q.to_vec(), fill, edge));
        }
    }
    let wide = |on: bool, c: Color32| {
        if on {
            Stroke::new(STROKE_PX.1, lighten(c))
        } else {
            Stroke::new(STROKE_PX.0, c)
        }
    };
    if l.show_handles {
        for a in TriadAxis::ALL {
            let path = &l.arcs[a.index()];
            if path.len() < 2 {
                continue;
            }
            let on =
                hot == Some(TriadPart::Handle(a)) || t.handle == Some(a) || dragging == Some(a);
            let s = wide(on, axis_color(col, a));
            painter.add(Shape::line(path.clone(), s));
            painter.circle_filled(path[path.len() / 2], KNOB_PX, s.color);
        }
    }
    for a in TriadAxis::ALL {
        if let Some(tip) = l.tips[a.index()] {
            let s = wide(hot == Some(TriadPart::Axis(a)), axis_color(col, a));
            painter.line_segment([l.center, tip], s);
        }
    }
    painter.circle_filled(l.center, KNOB_PX, col.text);
}

// ---- Bedienung in der Ansicht --------------------------------------------------------------------

/// Laufender Zug an einem Drehbogen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriadDrag {
    pub axis: TriadAxis,
    /// Drehung vor dem Zug (Rechtsklick und Esc stellen sie wieder her).
    pub start_rot: DQuat,
    /// Vom Ursprung zum gefassten Punkt, in der Drehebene (Skizzenkoordinaten).
    pub from: DVec3,
    /// Bisheriger Winkel ohne Fang, über ±180° hinweg durchgezählt.
    pub raw: f64,
    /// Gezeigter und angewandter Winkel (mit Fang).
    pub angle: f64,
}

/// Ein Drücken auf der Triade, das bis zum Loslassen ihr gehört.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriadPress {
    /// Links auf ein Ebenenquadrat: Beim Loslassen darauf wird die Ebene aktiv.
    Plane(TriadPlane),
    /// Rechts auf die Triade: Beim Loslassen öffnet das Menü.
    Menu,
    /// Abgebrochener Zug: bis alle Tasten los sind, gehört nichts der Skizze.
    Swallow,
}

/// Bedienzustand der Triade in der Ansicht (gehört zur Sitzung, nicht zum Dokument).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriadUi {
    /// Bild der Triade im letzten Bild; `None`, wenn sie nicht zu sehen ist.
    pub layout: Option<TriadLayout>,
    /// Teil unter dem Zeiger.
    pub hot: Option<TriadPart>,
    pub drag: Option<TriadDrag>,
    pub press: Option<TriadPress>,
    /// Text des Winkelfelds am aktiven Drehgriff.
    pub field: String,
    /// Drehgriff, für den das Winkelfeld zuletzt erschien (neu: Fokus und Vorschlag 0).
    pub field_for: Option<TriadAxis>,
}

impl TriadUi {
    /// Bricht einen laufenden Zug ab und stellt die Drehung von davor wieder her.
    pub fn abort(&mut self, t: &mut Triad) {
        if let Some(d) = self.drag.take() {
            t.rot = d.start_rot;
        }
        self.press = None;
        self.hot = None;
    }

    /// Beendet einen laufenden Zug von außen (Sprung im Verlauf): Die Drehung von davor kommt
    /// zurück, und bis alle Tasten los sind, gehört nichts der Skizze (das Loslassen ist kein
    /// Klick). Ohne Zug geschieht nichts.
    pub fn end_drag(&mut self, t: &mut Triad) {
        if let Some(d) = self.drag.take() {
            t.rot = d.start_rot;
            self.press = Some(TriadPress::Swallow);
        }
    }
}

/// Beginnt das Ereignis eine Zahl (Ziffer, Vorzeichen, Komma)?
fn starts_number(e: &egui::Event) -> bool {
    matches!(e, egui::Event::Text(t)
        if t.chars().next().is_some_and(|c| c.is_ascii_digit() || "+-.,−".contains(c)))
}

fn menu_id() -> egui::Id {
    egui::Id::new("planora-triad-menu")
}

fn field_id() -> egui::Id {
    egui::Id::new("planora-triad-angle")
}

/// Punkt in der Drehebene um `axis` (durch den Ursprung) unter `pointer`, als Vektor vom Ursprung.
fn on_rotation_plane(
    t: &Triad,
    rot: DQuat,
    axis: TriadAxis,
    map: &Mapper,
    pointer: Pos2,
) -> Option<DVec3> {
    let viewport = Vec2::new(map.rect.width(), map.rect.height());
    let pixel = Vec2::new(pointer.x - map.rect.left(), pointer.y - map.rect.top());
    let n = rot * axis.unit();
    let p =
        crate::sketch_view::screen_to_plane(&map.camera, &map.frame, viewport, pixel, t.origin, n)?;
    let v = p - t.origin;
    (v.length() > 1e-12).then_some(v)
}

/// Eingaben an der Triade, vor den Werkzeugen. `hover`: Zeiger über der Ansicht (nicht über einem
/// Fenster oder Menü). `true`, wenn die Triade den Zeiger in diesem Bild nimmt; dann sieht die
/// Skizze das Drücken, Ziehen und Loslassen nicht (kein Punkt, kein Abbruch durch Rechtsklick).
pub(crate) fn input(ui: &egui::Ui, rect: Rect, state: &mut AppState, hover: Option<Pos2>) -> bool {
    let ctx = ui.ctx().clone();
    let (show_planes, show_handles) = (state.triad_planes, state.triad_handles);
    let map = crate::sketch_view::mapper(rect, state);
    let Some((se, sk)) = state.session_sketch_mut() else {
        return false;
    };
    let layout = triad_visible(se, sk)
        .then(|| {
            map.as_ref()
                .and_then(|m| triad_layout(&se.triad, m, TRIAD_SIZE_PX))
        })
        .flatten()
        .map(|mut l| {
            l.show_planes = show_planes;
            l.show_handles = show_handles;
            l
        });
    se.triad_ui.layout = layout.clone();
    let (Some(l), Some(map)) = (layout, map) else {
        se.triad_ui.abort(&mut se.triad);
        return false;
    };
    let (down, pressed, sec_down, sec_pressed, latest, shift, ctrl) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.primary_pressed(),
            i.pointer.secondary_down(),
            i.pointer.button_pressed(PointerButton::Secondary),
            i.pointer.latest_pos(),
            i.modifiers.shift,
            i.modifiers.ctrl,
        )
    });
    // Zug am Drehbogen. Esc gehört ihm auch, wenn noch ein Feld die Tastatur hält (Korrektur
    // Task 10): Der Zug nimmt die Maus, getippt wird währenddessen nicht.
    if let Some(mut d) = se.triad_ui.drag {
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if sec_pressed || esc || !show_handles {
            se.triad.rot = d.start_rot;
            se.triad_ui.drag = None;
            se.triad_ui.press = Some(TriadPress::Swallow);
            if esc {
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            }
            return true;
        }
        if let Some(v) =
            latest.and_then(|p| on_rotation_plane(&se.triad, d.start_rot, d.axis, &map, p))
        {
            let n = d.start_rot * d.axis.unit();
            let raw = n.dot(d.from.cross(v)).atan2(d.from.dot(v));
            d.raw = crate::sketch_grips::unwrapped_angle(d.raw, raw);
            d.angle = snap_angle(d.raw, ctrl);
            se.triad.rot =
                (d.start_rot * DQuat::from_axis_angle(d.axis.unit(), d.angle)).normalize();
        }
        se.triad_ui.drag = (down).then_some(d);
        return true;
    }

    // Ein Drücken, das der Triade gehört, bis zum Loslassen.
    if let Some(press) = se.triad_ui.press {
        match press {
            TriadPress::Plane(plane) => {
                if !down {
                    if hover.and_then(|p| triad_hit(&l, p)) == Some(TriadPart::Plane(plane)) {
                        se.triad.active = plane;
                    }
                    se.triad_ui.press = None;
                }
            }
            TriadPress::Menu => {
                if !sec_down {
                    se.triad_ui.press = None;
                    if let Some(at) = latest {
                        egui::Popup::open_id(&ctx, menu_id());
                        ctx.data_mut(|d| d.insert_temp(menu_id().with("pos"), at));
                    }
                }
            }
            TriadPress::Swallow => {
                if !down && !sec_down {
                    se.triad_ui.press = None;
                }
            }
        }
        return true;
    }

    let hot = hover.and_then(|p| triad_hit(&l, p));
    se.triad_ui.hot = hot;
    if crate::keymap::settings_ui::is_capturing(&ctx) || shift {
        return false;
    }
    if pressed && let Some(at) = hover {
        match triad_hit(&l, at) {
            Some(TriadPart::Plane(plane)) => {
                se.triad_ui.press = Some(TriadPress::Plane(plane));
                return true;
            }
            Some(TriadPart::Handle(axis)) => {
                match on_rotation_plane(&se.triad, se.triad.rot, axis, &map, at) {
                    Some(from) => {
                        se.triad_ui.drag = Some(TriadDrag {
                            axis,
                            start_rot: se.triad.rot,
                            from,
                            raw: 0.0,
                            angle: 0.0,
                        });
                        // Der Zug nimmt die Tastatur aus dem Winkelfeld: Esc bricht ihn ab.
                        ctx.memory_mut(|m| m.surrender_focus(field_id().with("edit")));
                    }
                    // Die Ebene des Bogens steht auf der Kante: Drehen geht nicht, der Klick
                    // gehört trotzdem der Triade und setzt keinen Punkt (Korrektur Task 10).
                    None => se.triad_ui.press = Some(TriadPress::Swallow),
                }
                return true;
            }
            _ => {}
        }
    }
    if sec_pressed && hover.is_some_and(|p| over_triad(&l, p)) {
        se.triad_ui.press = Some(TriadPress::Menu);
        return true;
    }
    false
}

/// Zeichnet die Triade über der Skizze, die gestrichelte Achsverlängerung beim Achsfang unter dem
/// Zeiger und beim Ziehen das Winkelfeld.
pub(crate) fn paint(painter: &egui::Painter, rect: Rect, state: &AppState, pointer: Option<Pos2>) {
    let Some(se) = state.sketch_session.as_ref() else {
        return;
    };
    let Some(l) = se.triad_ui.layout.as_ref() else {
        return;
    };
    let col = crate::theme::sketch_colors(state.dark);
    let drag = se.triad_ui.drag;
    triad_paint(
        painter,
        l,
        &se.triad,
        drag.map(|d| TriadPart::Handle(d.axis)).or(se.triad_ui.hot),
        drag.map(|d| d.axis),
        &col,
    );
    if let Some(d) = drag {
        let at = pointer.unwrap_or(l.center) + vec2(LABEL_GAP, LABEL_GAP);
        let text = format!("{} {}", d.axis.angle_label(), format_angle(d.angle));
        let galley = painter.layout_no_wrap(text, FontId::proportional(LABEL_SIZE), col.text);
        let r = Rect::from_min_size(at, galley.size()).expand(3.0);
        painter.rect_filled(r, 3.0, col.label_bg);
        painter.galley(at, galley, col.text);
        return;
    }
    // Achsfang: die Verlängerung der Achse bis über den Zeiger hinaus.
    let (Some(p), Some(sf)) = (pointer, state.sketch_draft.as_ref()) else {
        return;
    };
    if se.triad_ui.hot.is_some() {
        return;
    }
    if se.triad.lock_z {
        return;
    }
    let viewport = Vec2::new(rect.width(), rect.height());
    let pixel = Vec2::new(p.x - rect.left(), p.y - rect.top());
    if let Some((q, Some(_))) = triad_point(
        &se.triad,
        &state.camera,
        sf.sketch.frame(),
        viewport,
        pixel,
        None,
    ) && let Some(s) = sketch3_to_screen(&state.camera, sf.sketch.frame(), viewport, q)
    {
        let s = Pos2::new(s.x + rect.left(), s.y + rect.top());
        let along = (s - l.center).normalized();
        if along.is_finite() {
            let end = s + along * LABEL_GAP * 2.0;
            painter.extend(Shape::dashed_line(
                &[l.center, end],
                Stroke::new(1.0, col.guide),
                6.0,
                4.0,
            ));
        }
    }
}

/// Fenster der Triade über der Ansicht: das Rechtsklickmenü und das Winkelfeld am aktiven
/// Drehgriff (Tab). Nach [`input`] und dem Zeichnen der Skizze.
pub(crate) fn show(ui: &egui::Ui, state: &mut AppState) {
    menu(ui, state);
    angle_field(ui, state);
}

/// Rechtsklickmenü der Triade: „Triadenausrichtung zurücksetzen“, „Drehgriffe anzeigen“,
/// „Ebenen anzeigen“. Die beiden Haken gehen über [`AppState::triad_view_changed`] in die
/// Einstellungen.
fn menu(ui: &egui::Ui, state: &mut AppState) {
    let ctx = ui.ctx().clone();
    if !egui::Popup::is_id_open(&ctx, menu_id()) {
        return;
    }
    let Some(pos) = ctx.data(|d| d.get_temp::<Pos2>(menu_id().with("pos"))) else {
        egui::Popup::close_id(&ctx, menu_id());
        return;
    };
    let (mut planes, mut handles, mut reset) = (state.triad_planes, state.triad_handles, false);
    egui::Popup::new(menu_id(), ctx.clone(), pos, ui.layer_id())
        .kind(egui::PopupKind::Menu)
        .open_memory(None)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .show(|ui| {
            reset = ui.button(tr!("sketch-triad-reset")).clicked();
            ui.checkbox(&mut handles, tr!("sketch-triad-show-handles"));
            ui.checkbox(&mut planes, tr!("sketch-triad-show-planes"));
        });
    if (planes, handles) != (state.triad_planes, state.triad_handles) {
        state.triad_planes = planes;
        state.triad_handles = handles;
        state.triad_view_changed = true;
    }
    if reset && let Some(se) = state.sketch_session.as_mut() {
        se.triad.reset_orientation();
    }
}

/// Winkelfeld am aktiven Drehgriff (Tab, Spec Abschnitt 4): Es erscheint mit „0“, nimmt die
/// Tastatur aber erst, wenn eine Zahl getippt wird (sonst ruhten bis dahin alle Kürzel, auch Alt
/// und Pfeil hoch; Korrektur Task 10). Eingabe dreht die Triade um den getippten Winkel (Grad) um
/// die Achse des Griffs, Esc schließt das Feld, Tab im Feld wechselt zum nächsten Griff.
fn angle_field(ui: &egui::Ui, state: &mut AppState) {
    let ctx = ui.ctx().clone();
    let show_handles = state.triad_handles;
    let Some(se) = state.sketch_session.as_mut() else {
        return;
    };
    let (Some(axis), Some(l)) = (se.triad.handle, se.triad_ui.layout.as_ref()) else {
        se.triad_ui.field_for = None;
        return;
    };
    if !show_handles || se.triad_ui.drag.is_some() {
        return;
    }
    let edit_id = field_id().with("edit");
    let fresh = se.triad_ui.field_for != Some(axis);
    if fresh {
        se.triad_ui.field = "0".to_string();
        se.triad_ui.field_for = Some(axis);
    }
    let knob = l.arcs[axis.index()]
        .get(l.arcs[axis.index()].len() / 2)
        .copied()
        .unwrap_or(l.center);
    let at = knob + vec2(LABEL_GAP, LABEL_GAP);
    let mut text = std::mem::take(&mut se.triad_ui.field);
    let mut has_focus = ctx.memory(|m| m.has_focus(edit_id));
    // Eine getippte Zahl öffnet das Feld, solange kein anderes Feld die Tastatur hat; das erste
    // Zeichen ersetzt den Vorschlag.
    let starts = !has_focus
        && !viewport::keyboard_taken(&ctx)
        && ctx.input(|i| i.events.iter().any(starts_number));
    if starts {
        text.clear();
        ctx.memory_mut(|m| m.request_focus(edit_id));
        has_focus = true;
    }
    // Tab im Feld: der nächste Drehgriff (die Tastenkürzel ruhen, solange das Feld tippt).
    if has_focus && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
        se.triad.cycle_handle();
        se.triad_ui.field = text;
        return;
    }
    let response = egui::Area::new(field_id())
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            let p = crate::theme::palette(ui.visuals().dark_mode);
            egui::Frame::new()
                .fill(p.bg_panel)
                .corner_radius(3.0)
                .inner_margin(1.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(axis.angle_label());
                        let r = crate::theme::text_field(
                            ui,
                            egui::TextEdit::singleline(&mut text)
                                .id(edit_id)
                                .desired_width(FIELD_WIDTH),
                        );
                        ui.label("°");
                        r
                    })
                    .inner
                })
                .inner
        })
        .inner;
    if response.lost_focus() {
        let (enter, esc) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if enter {
            if let Some(deg) = parse_number(&text) {
                se.triad.rotate(axis, deg.to_radians());
            }
            text = "0".to_string();
        } else if esc {
            ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            se.triad.handle = None;
            se.triad_ui.field_for = None;
        }
    }
    se.triad_ui.field = text;
}
