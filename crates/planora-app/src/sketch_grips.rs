//! Griffe von Verschieben/Kopieren und Maßstab (Entscheidungen 10 und 12 des Plans Skizzen 4): Lage
//! und Treffer in Bildpunkten, die reine Griff-Rechnung und der Ablauf an der Sitzung. Ein Griffzug
//! ändert nur `transform`, die Skizze bleibt unberührt, bis der Befehl abschließt; Vorschau und
//! Abschluss rechnen mit denselben Funktionen aus `planora_sketch::transform`.

use crate::msg::Msg;
use crate::sketch_cursor::DrawCursor;
use crate::sketch_input::FieldKind;
use crate::sketch_kinds::PREVIEW_STEPS_PER_TURN;
use crate::sketch_state::{Item, SketchSession, Snap, Tool, TransformSet, TransformStage, err};
use crate::sketch_view::Mapper;
use crate::theme::SketchColors;
use egui::{Color32, Pos2, Shape, Stroke, Vec2};
use planora_feature::SketchFeature;
use planora_math::{DQuat, DVec2, DVec3};
use planora_sketch::model::{CurveId, LineStyle, PointId, Sketch};
use planora_sketch::transform::{
    CopyMap, Rigid3, Similarity, TransformBlock, TransformSet as SketchTransformSet,
};

/// Länge der Pfeile und Abstand der Handhabe des Maßstabs in Bildpunkten.
pub const GRIP_ARROW_PX: f32 = 60.0;
/// Radius des Rings in Bildpunkten.
pub const GRIP_RING_PX: f32 = 40.0;
/// So nah muss der Zeiger an einem Griff liegen, in Bildpunkten.
pub const GRIP_HIT_PX: f32 = 8.0;
/// Kleinster Faktor am Griff: Durch den Bezugspunkt hindurch bleibt er über null.
const GRIP_MIN_FACTOR: f64 = 1e-6;
/// Radius des Griffs in der Mitte und halbe Größe der Handhabe in Bildpunkten.
const GRIP_CENTER_PX: f32 = 5.0;
/// Länge und halbe Breite der Pfeilspitze in Bildpunkten.
const GRIP_HEAD_PX: (f32, f32) = (12.0, 5.0);
/// Strichstärke der Griffe in Bildpunkten, normal und unter dem Zeiger.
const GRIP_WIDTH: (f32, f32) = (1.5, 2.5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grip {
    Free,
    AxisX,
    AxisY,
    Rotate,
    Scale,
    /// Nur mit „3D-Skizze“ (Skizzen 10): Z-Pfeil und die Drehbögen um X und Y.
    AxisZ,
    RotateX,
    RotateY,
}

impl Grip {
    /// Ein Griff für 3D: Er rechnet im Raum ([`SketchSession::grip_to3`]), nicht auf der Ebene.
    pub fn is_3d(self) -> bool {
        matches!(self, Grip::AxisZ | Grip::RotateX | Grip::RotateY)
    }
}

/// Lage der Griffe auf dem Bildschirm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GripLayout {
    pub center: Pos2,
    pub x_tip: Pos2,
    pub y_tip: Pos2,
    pub ring: f32,
    /// Nur die Handhabe des Maßstabs.
    pub scale: bool,
}

/// Griffe um `center`; `x_dir` und `y_dir` sind die normierten Bilder der Skizzenachsen, also auf
/// gekippten Ansichten richtig.
pub fn layout(center: Pos2, x_dir: Vec2, y_dir: Vec2, scale: bool) -> GripLayout {
    GripLayout {
        center,
        x_tip: center + x_dir * GRIP_ARROW_PX,
        y_tip: center + y_dir * GRIP_ARROW_PX,
        ring: GRIP_RING_PX,
        scale,
    }
}

/// Abstand des Punkts `p` zur Strecke `a`–`b`.
fn to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 {
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * t).distance(p)
}

/// Der Griff unter `p`: Mitte, dann Pfeile, dann Ring; im Maßstab nur die Handhabe.
pub fn hit(l: &GripLayout, p: Pos2) -> Option<Grip> {
    if l.scale {
        return (l.x_tip.distance(p) <= GRIP_HIT_PX).then_some(Grip::Scale);
    }
    if l.center.distance(p) <= GRIP_HIT_PX {
        Some(Grip::Free)
    } else if to_segment(p, l.center, l.x_tip) <= GRIP_HIT_PX {
        Some(Grip::AxisX)
    } else if to_segment(p, l.center, l.y_tip) <= GRIP_HIT_PX {
        Some(Grip::AxisY)
    } else if (l.center.distance(p) - l.ring).abs() <= GRIP_HIT_PX {
        Some(Grip::Rotate)
    } else {
        None
    }
}

/// Zeichnet die Griffe; `hot` hebt einen hervor. Die Farben kommen vom Aufrufer aus dem Thema.
pub fn paint(
    painter: &egui::Painter,
    l: &GripLayout,
    hot: Option<Grip>,
    color: Color32,
    hot_color: Color32,
) {
    let stroke = |grip: Grip| {
        if hot == Some(grip) {
            Stroke::new(GRIP_WIDTH.1, hot_color)
        } else {
            Stroke::new(GRIP_WIDTH.0, color)
        }
    };
    if l.scale {
        let s = stroke(Grip::Scale);
        painter.line_segment([l.center, l.x_tip], Stroke::new(GRIP_WIDTH.0, color));
        painter.rect_filled(
            egui::Rect::from_center_size(l.x_tip, Vec2::splat(GRIP_CENTER_PX * 2.0)),
            0.0,
            s.color,
        );
        painter.circle_filled(l.center, GRIP_CENTER_PX * 0.6, color);
        return;
    }
    painter.circle_stroke(l.center, l.ring, stroke(Grip::Rotate));
    for (grip, tip) in [(Grip::AxisX, l.x_tip), (Grip::AxisY, l.y_tip)] {
        let s = stroke(grip);
        painter.line_segment([l.center, tip], s);
        let along = (tip - l.center).normalized();
        let side = along.rot90() * GRIP_HEAD_PX.1;
        let root = tip - along * GRIP_HEAD_PX.0;
        painter.add(Shape::convex_polygon(
            vec![tip, root + side, root - side],
            s.color,
            Stroke::NONE,
        ));
    }
    let center = if hot == Some(Grip::Free) {
        hot_color
    } else {
        color
    };
    painter.circle_filled(l.center, GRIP_CENTER_PX, center);
}

/// Laufender Zug an einem Griff: Stelle des Drückens und Stand von `transform` davor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GripDrag {
    pub grip: Grip,
    pub press: DVec2,
    pub start_offset: DVec2,
    pub start_angle: f64,
    pub start_factor: f64,
}

/// Der Teil eines Zugs an einem Griff für 3D (Skizzen 10), neben [`GripDrag`]: die gefasste
/// Stelle im Raum und Z-Abstand und Winkel um X und Y davor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GripDrag3 {
    pub grip: Grip,
    pub press: DVec3,
    pub start_z: f64,
    pub start_angle_x: f64,
    pub start_angle_y: f64,
}

/// Verschiebung, Winkel und Faktor für den Zeiger `now`, rein. `base` ist der Drehpunkt auf der
/// Skizze. Der Winkel liegt im halben Umlauf um `start_angle`; ganze Umläufe zählt
/// [`unwrapped_angle`]. Der Faktor ist nie null oder negativ, und ein Zeiger auf `base` oder außerhalb
/// der Zahlen lässt Winkel und Faktor beim Startwert.
pub fn grip_motion(d: &GripDrag, base: DVec2, now: DVec2) -> (DVec2, f64, f64) {
    let mut out = (d.start_offset, d.start_angle, d.start_factor);
    if !now.is_finite() {
        return out;
    }
    let moved = now - d.press;
    let (from, to) = (d.press - base, now - base);
    match d.grip {
        Grip::Free => out.0 += moved,
        Grip::AxisX => out.0.x += moved.x,
        Grip::AxisY => out.0.y += moved.y,
        Grip::Rotate => {
            if from.length() > 0.0 && to.length() > 0.0 {
                out.1 += from.perp_dot(to).atan2(from.dot(to));
            }
        }
        Grip::Scale => {
            if from.length() > 0.0 {
                out.2 = (d.start_factor * to.length() / from.length()).max(GRIP_MIN_FACTOR);
            }
        }
        // Rechnen im Raum, siehe `grip_motion3`.
        Grip::AxisZ | Grip::RotateX | Grip::RotateY => {}
    }
    out
}

/// Z-Abstand und Winkel um X und Y für die Stelle `now` im Raum (Skizzenkoordinaten), rein.
/// `center` ist die Mitte des Griffs. Der Z-Pfeil nimmt die Höhe des Zeigers, ein Drehbogen den
/// Winkel um seine Achse von der gefassten Stelle zu `now`, in der Ebene senkrecht zur Achse
/// gemessen (im halben Umlauf um den Startwinkel, ganze Umläufe zählt [`unwrapped_angle`]).
pub fn grip_motion3(d: &GripDrag3, center: DVec3, now: DVec3) -> (f64, f64, f64) {
    let mut out = (d.start_z, d.start_angle_x, d.start_angle_y);
    if !now.is_finite() {
        return out;
    }
    let turn = |axis: DVec3| {
        let flat = |v: DVec3| v - axis * v.dot(axis);
        let (from, to) = (flat(d.press - center), flat(now - center));
        (from.length() > 0.0 && to.length() > 0.0)
            .then(|| axis.dot(from.cross(to)).atan2(from.dot(to)))
    };
    match d.grip {
        Grip::AxisZ => out.0 += now.z - d.press.z,
        Grip::RotateX => out.1 += turn(DVec3::X).unwrap_or(0.0),
        Grip::RotateY => out.2 += turn(DVec3::Y).unwrap_or(0.0),
        _ => {}
    }
    out
}

/// Die Drehung aus den Winkeln um X, Y und Z: erst um X, dann um Y, dann um Z (wie die Felder in
/// Fusion von oben nach unten).
pub fn rotation3(angle_x: f64, angle_y: f64, angle_z: f64) -> DQuat {
    DQuat::from_rotation_z(angle_z)
        * DQuat::from_rotation_y(angle_y)
        * DQuat::from_rotation_x(angle_x)
}

/// Lage der Griffe für 3D auf dem Bildschirm: Spitze des Z-Pfeils und die Drehbögen um X und Y
/// (Viertelkreise zwischen den beiden anderen Achsen, wie an der Triade).
#[derive(Clone, Debug, PartialEq)]
pub struct GripLayout3 {
    pub center: Pos2,
    pub z_tip: Option<Pos2>,
    pub arc_x: Vec<Pos2>,
    pub arc_y: Vec<Pos2>,
}

/// Abschnitte eines Drehbogens.
const GRIP_ARC_STEPS: usize = 16;
/// Kürzer als so viele Bildpunkte (Z-Pfeil) bzw. flacher (Mitte eines Drehbogens neben seiner
/// Sehne) ist ein Griff für 3D von der Seite gesehen und fehlt: Von oben läge der Bogen um X
/// sonst auf dem Pfeil Y.
const GRIP_FLAT_PX: f32 = 6.0;

/// Griffe für 3D um die Stelle `center` (Skizzenkoordinaten) mit Pfeil [`GRIP_ARROW_PX`] und Bögen
/// mit dem Radius [`GRIP_RING_PX`], in Bildpunkten fest wie die Triade; `None`, wenn die Mitte
/// hinter der Kamera liegt oder die Ansicht keinen Maßstab hat.
pub fn layout3(map: &Mapper, center: DVec3) -> Option<GripLayout3> {
    let c = map.to_screen3(center)?;
    let unit = crate::sketch_triad::units_per_px_at(map, center)?;
    let arrow = unit * f64::from(GRIP_ARROW_PX);
    let ring = unit * f64::from(GRIP_RING_PX);
    let arc = |a: DVec3, b: DVec3| {
        (0..=GRIP_ARC_STEPS)
            .map(|i| {
                let th = std::f64::consts::FRAC_PI_2 * i as f64 / GRIP_ARC_STEPS as f64;
                map.to_screen3(center + (a * th.cos() + b * th.sin()) * ring)
            })
            .collect::<Option<Vec<_>>>()
            .filter(|path| {
                let (a, b, m) = (path[0], path[GRIP_ARC_STEPS], path[GRIP_ARC_STEPS / 2]);
                let chord = b - a;
                chord.length() > 0.0
                    && (m - a).dot(chord.rot90()).abs() / chord.length() >= GRIP_FLAT_PX
            })
            .unwrap_or_default()
    };
    let z_tip = map
        .to_screen3(center + DVec3::Z * arrow)
        .filter(|tip| tip.distance(c) >= GRIP_FLAT_PX);
    Some(GripLayout3 {
        center: c,
        z_tip,
        arc_x: arc(DVec3::Y, DVec3::Z),
        arc_y: arc(DVec3::Z, DVec3::X),
    })
}

/// Der Griff für 3D unter `p`: Z-Pfeil, dann die Bögen um X und Y.
pub fn hit3(l: &GripLayout3, p: Pos2) -> Option<Grip> {
    let near_path = |path: &[Pos2]| {
        path.windows(2)
            .any(|w| to_segment(p, w[0], w[1]) <= GRIP_HIT_PX)
    };
    if l.z_tip
        .is_some_and(|tip| to_segment(p, l.center, tip) <= GRIP_HIT_PX)
    {
        Some(Grip::AxisZ)
    } else if near_path(&l.arc_x) {
        Some(Grip::RotateX)
    } else if near_path(&l.arc_y) {
        Some(Grip::RotateY)
    } else {
        None
    }
}

/// Der Griff unter `p` mit den Griffen für 3D: Mitte und Pfeile X und Y zuerst, dann Z-Pfeil und
/// Drehbögen, zuletzt der Ring.
pub fn hit_any(l: &GripLayout, l3: Option<&GripLayout3>, p: Pos2) -> Option<Grip> {
    let flat = hit(l, p);
    if matches!(
        flat,
        Some(Grip::Free | Grip::AxisX | Grip::AxisY | Grip::Scale)
    ) {
        return flat;
    }
    l3.and_then(|l3| hit3(l3, p)).or(flat)
}

/// Zeichnet die Griffe für 3D in den Achsfarben wie die Triade (Z blau, Bogen um X rot, um Y
/// grün); `hot` hebt einen hervor.
pub fn paint3(painter: &egui::Painter, l: &GripLayout3, hot: Option<Grip>, col: &SketchColors) {
    let stroke = |grip: Grip, color: Color32| {
        if hot == Some(grip) {
            Stroke::new(GRIP_WIDTH.1, crate::theme::lighten(color))
        } else {
            Stroke::new(GRIP_WIDTH.0, color)
        }
    };
    for (grip, path, color) in [
        (Grip::RotateX, &l.arc_x, col.axis_x),
        (Grip::RotateY, &l.arc_y, col.axis_y),
    ] {
        if path.len() < 2 {
            continue;
        }
        let s = stroke(grip, color);
        painter.add(Shape::line(path.clone(), s));
        painter.circle_filled(path[path.len() / 2], GRIP_CENTER_PX * 0.8, s.color);
    }
    if let Some(tip) = l.z_tip {
        let s = stroke(Grip::AxisZ, col.axis_z);
        painter.line_segment([l.center, tip], s);
        let along = (tip - l.center).normalized();
        let side = along.rot90() * GRIP_HEAD_PX.1;
        let root = tip - along * GRIP_HEAD_PX.0;
        painter.add(Shape::convex_polygon(
            vec![tip, root + side, root - side],
            s.color,
            Stroke::NONE,
        ));
    }
}

/// Die Stelle im Raum unter dem Zeiger `pointer` für den Griff für 3D `grip` an `center`
/// (Skizzenkoordinaten): beim Z-Pfeil auf der Ebene, die die Z-Achse durch `center` enthält und
/// dem Blick am meisten zugewandt ist, bei einem Drehbogen auf der Ebene senkrecht zu seiner
/// Achse. `None` bei streifendem Blick (Z-Pfeil von oben) oder hinter der Kamera.
pub fn grip_point3(map: &Mapper, grip: Grip, center: DVec3, pointer: Pos2) -> Option<DVec3> {
    let normal = match grip {
        Grip::AxisZ => {
            let n = map.view_normal()?;
            (n - DVec3::Z * n.z).try_normalize()?
        }
        Grip::RotateX => DVec3::X,
        Grip::RotateY => DVec3::Y,
        _ => return None,
    };
    map.screen_to_plane(pointer, center, normal)
}

/// Der Winkel `raw` in dem Umlauf, der `previous` am nächsten liegt: So läuft der Griff über ±180°
/// hinweg durch, und 720° bleiben 720°.
pub fn unwrapped_angle(previous: f64, raw: f64) -> f64 {
    let d = raw - previous;
    previous + d.sin().atan2(d.cos())
}

/// Zahl auf höchstens 12 gültige Stellen als Text mit Punkt; `None`, wenn sie nicht positiv und
/// endlich ist, mehr als 1e-9 (relativ) abweicht oder der Text lang würde (winzige und riesige
/// Zahlen): Dann steht die Formel nicht in kurzer Form da.
pub(crate) fn short_factor(p: f64) -> Option<String> {
    if !(p.is_finite() && p > 0.0) {
        return None;
    }
    let r: f64 = format!("{p:.11e}").parse().ok()?;
    if (r - p).abs() > 1e-9 * p {
        return None;
    }
    let text = format!("{r}");
    (text.chars().count() <= FACTOR_TEXT_MAX).then_some(text)
}

/// Längster Text eines Faktors in einer Formel.
const FACTOR_TEXT_MAX: usize = 30;

/// Ein Name oder eine Zahl ohne Operator, Leerraum oder Klammer: braucht keine Klammern.
pub(crate) fn is_plain(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ','))
}

/// Steht der ganze Text in einem Klammerpaar, das sich erst am Ende schließt?
pub(crate) fn is_wrapped(s: &str) -> bool {
    if !(s.starts_with('(') && s.ends_with(')')) {
        return false;
    }
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1 == s.len();
                }
            }
            _ => {}
        }
    }
    false
}

/// Trennt `kopf * zahl` ab, wenn der Kopf ein Name, eine Zahl oder ein Klammerausdruck ist und
/// `zahl` eine reine Zahl: nur dann darf der Faktor mit einem neuen zusammengefasst werden.
pub(crate) fn split_factor(text: &str) -> Option<(&str, f64)> {
    let i = text.rfind(" * ")?;
    let (head, tail) = (&text[..i], &text[i + 3..]);
    if !(is_plain(head) || is_wrapped(head))
        || tail.is_empty()
        || !tail
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ','))
    {
        return None;
    }
    let n: f64 = tail.replace(',', ".").parse().ok()?;
    Some((head, n))
}

/// Der Text eines Maßes mit Formel nach dem Skalieren um `k`: `(alt) * k` mit `k` in kurzer
/// Schreibweise und Punkt als Dezimaltrenner. So stimmt der Wert, und die Parameter bleiben am Maß.
/// Ein Faktor am Ende wird zusammengefasst (`(alt) * n` wird `(alt) * n·k`), damit die Formel bei
/// vielem Skalieren nicht wächst; ein Name oder eine Zahl bekommt keine Klammern. `None`, wenn
/// der Faktor nicht kurz zu schreiben ist.
pub(crate) fn scaled_formula(text: &str, k: f64) -> Option<String> {
    let text = text.trim();
    if let Some((head, n)) = split_factor(text) {
        return Some(format!("{head} * {}", short_factor(n * k)?));
    }
    let m = short_factor(k)?;
    Some(if is_plain(text) {
        format!("{text} * {m}")
    } else {
        format!("({text}) * {m}")
    })
}

/// Wert eines Längenmaßes (auch des Abstands eines Musters, Skizzen 5).
fn length_of(kind: &planora_sketch::constraint::ConstraintKind) -> Option<f64> {
    use planora_sketch::constraint::ConstraintKind as K;
    match *kind {
        K::EllipseMajor(_, v) | K::EllipseMinor(_, v) => Some(v),
        K::PatternValue(_, slot, v) if slot.is_spacing() => Some(v),
        ref k => k.length_value(),
    }
}

/// Die Formel nach dem Skalieren, oder `None`, wenn sie nicht mehr als Formel taugt (zu lang, nicht
/// lesbar): Dann steht der Wert des Maßes statt ihrer da, und es sind keine Parameter mehr dran.
fn scaled_or_value(text: &str, k: f64, value: Option<f64>) -> (Option<String>, bool) {
    let kept = scaled_formula(text, k).filter(|t| planora_feature::expr::parse(t).is_ok());
    match kept {
        Some(t) => (Some(t), false),
        None => (
            value.and_then(|v| planora_feature::expr::value_text(v).ok()),
            true,
        ),
    }
}

/// Warum die Bewegung nicht geht, als Meldung.
fn block_message(block: TransformBlock) -> Msg {
    match block {
        TransformBlock::FixedInSet(_) | TransformBlock::BoundToFixed(_) => {
            crate::msg!("sketch-move-fixed")
        }
        TransformBlock::NothingToDo => crate::msg!("sketch-move-nothing"),
        TransformBlock::Degenerate => crate::msg!("sketch-move-degenerate"),
        TransformBlock::PatternInstance => crate::msg!("sketch-move-pattern-instance"),
        TransformBlock::TextOnPath => crate::msg!("sketch-text-path-follows"),
        TransformBlock::NotIn3d => crate::msg!("sketch-err-tool-not-in-3d"),
    }
}

/// Mitte der Höhen der Punkte der Kurven und der Punkte (Skizzen 10): Um diese Höhe drehen die
/// Bögen um X und Y; in der Ebene 0.
fn hull_z(sk: &Sketch, curves: &[CurveId], points: &[PointId]) -> f64 {
    let own = curves
        .iter()
        .filter_map(|&c| sk.curve(c))
        .flat_map(|d| d.kind.points());
    let zs = own
        .chain(points.iter().copied())
        .filter_map(|p| sk.point_xyz(p).ok())
        .map(|p| p.z);
    let (lo, hi) = zs.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), z| {
        (lo.min(z), hi.max(z))
    });
    if lo.is_finite() && hi.is_finite() {
        (lo + hi) * 0.5
    } else {
        0.0
    }
}

/// Mitte der Hülle der Kurven und Punkte; `None` ohne Lage.
fn hull_center(sk: &Sketch, curves: &[CurveId], points: &[PointId]) -> Option<DVec2> {
    let lines = curves
        .iter()
        .filter_map(|&c| sk.curve_polyline(c, PREVIEW_STEPS_PER_TURN))
        .flatten();
    let spots = points.iter().filter_map(|&p| sk.point_xy(p).ok());
    let (lo, hi) = lines.chain(spots).fold(None, |range, p| match range {
        None => Some((p, p)),
        Some((lo, hi)) => Some((p.min(lo), p.max(hi))),
    })?;
    Some((lo + hi) * 0.5)
}

impl SketchSession {
    /// Geht die Stufenfolge von Verschieben/Kopieren und Maßstab für dieses Werkzeug?
    pub fn runs_transform(&self) -> bool {
        matches!(self.tool, Tool::MoveCopy | Tool::Scale)
    }

    /// Wird gerade gewählt? Dann gilt der Rahmen wie im Auswahlwerkzeug.
    pub fn transform_picking(&self) -> bool {
        (matches!(self.tool, Tool::MoveCopy | Tool::Scale)
            && self.transform.stage == TransformStage::Picking)
            || self.pattern_picking()
    }

    /// Sind Griffe zu sehen? Sobald eine Menge gesetzt ist, auch beim Wählen; beim Setzen des
    /// Bezugspunkts nicht.
    pub fn grips_shown(&self) -> bool {
        matches!(self.tool, Tool::MoveCopy | Tool::Scale)
            && self.transform.stage != TransformStage::SetBase
            && self.has_set()
    }

    fn has_set(&self) -> bool {
        !(self.transform.set.curves.is_empty() && self.transform.set.points.is_empty())
    }

    /// Läuft ein Zug mit der Maus, der mit dem Abschluss und dem Löschen nichts zu tun hat?
    pub fn in_gesture(&self) -> bool {
        self.grip_drag.is_some()
            || self.box_select.is_some()
            || self.pattern.drag.is_some()
            || self.handle_drag_running()
    }

    /// Die Kurven und Punkte der Auswahl als Menge.
    fn selected_set(&self, sk: &Sketch) -> TransformSet {
        let mut set = TransformSet::default();
        for item in &self.selection {
            match *item {
                Item::Curve(c) if sk.curve(c).is_some() => set.curves.push(c),
                Item::Point(p) if sk.point_xy(p).is_ok() => set.points.push(p),
                _ => {}
            }
        }
        set
    }

    /// Beim Wählen folgen Menge und Bezugspunkt der Auswahl: Die Griffe erscheinen, sobald etwas
    /// gewählt ist, in der Mitte der Hülle. Beim Maßstab kommt das mit Task 10.
    pub(crate) fn sync_picked(&mut self, sk: &Sketch) {
        if !self.runs_transform() || self.transform.stage != TransformStage::Picking {
            return;
        }
        let set = self.selected_set(sk);
        if set != self.transform.set {
            self.transform.base = hull_center(sk, &set.curves, &set.points);
            self.transform.base_z = hull_z(sk, &set.curves, &set.points);
            self.transform.set = set;
        }
    }

    /// Mit Vorauswahl geht es weiter, Bezugspunkt in der Mitte der Hülle; ohne bleibt die Stufe beim
    /// Wählen. Verschieben/Kopieren ist gleich aktiv, der Maßstab wartet auf den Bezugspunkt. Der
    /// Haken „Kopie erstellen“ bleibt, wie er ist.
    pub fn begin_transform(&mut self, sk: &Sketch) {
        let set = self.selected_set(sk);
        self.start_transform(sk, set, false);
    }

    /// Maßstab ohne Auswahl: die ganze Skizze ohne feste Geometrie, dann der Bezugspunkt. Leer lässt
    /// die Stufe, wie sie ist.
    fn begin_whole(&mut self, sk: &Sketch) {
        let whole = sk.whole_sketch_set();
        let set = TransformSet {
            curves: whole.curves,
            points: whole.points,
        };
        self.start_transform(sk, set, true);
    }

    /// Eingabe beim Wählen: mit Auswahl weiter, beim Maßstab ohne Auswahl die ganze Skizze; sonst die
    /// Meldung, dass nichts gewählt ist.
    pub(crate) fn leave_picking(&mut self, sk: &Sketch) {
        self.begin_transform(sk);
        if self.transform.stage == TransformStage::Picking && self.tool == Tool::Scale {
            self.begin_whole(sk);
        }
        if self.transform.stage == TransformStage::Picking {
            self.message = Some(crate::msg!("sketch-nothing-selected"));
        }
    }

    fn start_transform(&mut self, sk: &Sketch, set: TransformSet, whole: bool) {
        let Some(center) = hull_center(sk, &set.curves, &set.points) else {
            self.transform.stage = TransformStage::Picking;
            return;
        };
        self.transform.stage = if self.tool == Tool::Scale {
            TransformStage::SetBase
        } else {
            TransformStage::Active
        };
        self.transform.whole = whole;
        self.transform.base_z = hull_z(sk, &set.curves, &set.points);
        self.transform.set = set;
        self.transform.base = Some(center);
        self.transform.offset = DVec2::ZERO;
        self.transform.angle = 0.0;
        self.transform.factor = 1.0;
        self.transform.shift_z = 0.0;
        self.transform.angle_x = 0.0;
        self.transform.angle_y = 0.0;
    }

    /// Die Bewegung dieses Bilds: Gesperrte Felder gehen vor dem Griff. `None` ohne Menge.
    pub fn transform_now(&self) -> Option<Similarity> {
        let t = &self.transform;
        if !self.has_set() {
            return None;
        }
        let input = &self.input;
        Some(Similarity {
            base: t.base?,
            offset: DVec2::new(
                input.locked(FieldKind::MoveX).unwrap_or(t.offset.x),
                input.locked(FieldKind::MoveY).unwrap_or(t.offset.y),
            ),
            angle: input.locked(FieldKind::MoveAngle).unwrap_or(t.angle),
            factor: input.locked(FieldKind::ScaleFactor).unwrap_or(t.factor),
        })
    }

    /// Die Bewegung dieses Bilds im Raum (Skizzen 10): Drehpunkt ist der Bezugspunkt auf der
    /// Höhe `base_z`, gedreht wird erst um X, dann um Y, dann um Z ([`rotation3`]). Gesperrte
    /// Felder gehen vor dem Griff. `None` ohne Menge.
    pub fn transform_now3(&self) -> Option<Rigid3> {
        let s = self.transform_now()?;
        let t = &self.transform;
        let input = &self.input;
        let z = input.locked(FieldKind::MoveZ).unwrap_or(t.shift_z);
        let ax = input.locked(FieldKind::MoveAngleX).unwrap_or(t.angle_x);
        let ay = input.locked(FieldKind::MoveAngleY).unwrap_or(t.angle_y);
        Some(Rigid3 {
            rot: rotation3(ax, ay, s.angle),
            shift: s.offset.extend(z),
            pivot: s.base.extend(t.base_z),
        })
    }

    /// Die Bewegung im Raum, wenn Verschieben/Kopieren mit „3D-Skizze“ läuft und sie die Ebene
    /// verlässt (z-Anteil oder Drehung um X oder Y); sonst `None`, dann gilt der Weg der Ebene
    /// mit [`transform_now`](Self::transform_now), Bit für Bit wie bisher.
    pub fn moving3(&self, sk: &Sketch) -> Option<Rigid3> {
        if self.tool != Tool::MoveCopy || !sk.three_d {
            return None;
        }
        let m = self.transform_now3()?;
        m.planar(sk.tolerance()).is_none().then_some(m)
    }

    /// Mitte der Griffe im Raum: der bewegte Bezugspunkt.
    pub fn grip_center3(&self) -> Option<DVec3> {
        let m = self.transform_now3()?;
        Some(m.pivot + m.shift)
    }

    /// Beginnt den Zug an einem Griff bei `at` (Skizzeneinheiten).
    pub fn begin_grip(&mut self, grip: Grip, at: DVec2) {
        if !self.grips_shown() || !at.is_finite() {
            return;
        }
        self.grip_drag = Some(self.grip_start(grip, at));
        self.grip_drag3 = None;
    }

    /// Beginnt den Zug an einem Griff für 3D bei der Stelle `at` im Raum (Skizzen 10, siehe
    /// [`grip_point3`]); nur bei Verschieben/Kopieren mit „3D-Skizze“.
    pub fn begin_grip3(&mut self, grip: Grip, at: DVec3) {
        if !self.grips_shown() || !self.move3 || self.tool != Tool::MoveCopy || !at.is_finite() {
            return;
        }
        self.grip_drag = Some(self.grip_start(grip, at.truncate()));
        let t = &self.transform;
        self.grip_drag3 = Some(GripDrag3 {
            grip,
            press: at,
            start_z: t.shift_z,
            start_angle_x: t.angle_x,
            start_angle_y: t.angle_y,
        });
    }

    fn grip_start(&self, grip: Grip, press: DVec2) -> GripDrag {
        let t = &self.transform;
        GripDrag {
            grip,
            press,
            start_offset: t.offset,
            start_angle: t.angle,
            start_factor: t.factor,
        }
    }

    /// Führt einen Griff für 3D zur Stelle `at` im Raum.
    pub fn grip_to3(&mut self, at: DVec3) {
        let (Some(_), Some(d), Some(center)) =
            (self.grip_drag, self.grip_drag3, self.grip_center3())
        else {
            return;
        };
        let (z, ax, ay) = grip_motion3(&d, center, at);
        self.transform.shift_z = z;
        self.transform.angle_x = unwrapped_angle(self.transform.angle_x, ax);
        self.transform.angle_y = unwrapped_angle(self.transform.angle_y, ay);
    }

    /// Führt den Griff zum Zeiger `at`. Gedreht und skaliert wird um den Bezugspunkt an seiner
    /// jetzigen Lage.
    pub fn grip_to(&mut self, at: DVec2) {
        let (Some(d), Some(now)) = (self.grip_drag, self.transform_now()) else {
            return;
        };
        if d.grip.is_3d() {
            return;
        }
        let (offset, angle, factor) = grip_motion(&d, now.base + now.offset, at);
        self.transform.offset = offset;
        self.transform.angle = unwrapped_angle(self.transform.angle, angle);
        self.transform.factor = factor;
    }

    /// Beendet den Griff; der Stand bleibt in `transform`. War noch die Wahl offen, ist sie damit zu
    /// Ende, denn ein neuer Bezugspunkt nähme der Bewegung ihren Sinn.
    pub fn end_grip(&mut self) {
        self.grip_drag3 = None;
        if self.grip_drag.take().is_some() && self.transform.stage == TransformStage::Picking {
            self.transform.stage = TransformStage::Active;
        }
    }

    /// Geht die Bewegung nicht, sagt `message` das schon, sobald die Vorschau fehlt, und nicht erst
    /// beim Abschluss; geht sie wieder, verschwindet die Meldung. Eine Kopie ist frei und bleibt ohne.
    pub fn update_transform_block(&mut self, sk: &Sketch) {
        if !self.runs_transform() {
            return;
        }
        // In 3D (Skizzen 10) rechnet die Prüfung die Bewegung an einer Kopie der Skizze.
        if let (false, Some(m), Some(set)) =
            (self.transform.copy, self.moving3(sk), self.sketch_set(sk))
        {
            match sk.clone().move_rigid3(&set, &m, false) {
                Ok(_) => {
                    if std::mem::take(&mut self.transform.blocked_note) {
                        self.message = None;
                    }
                }
                Err(e) => {
                    self.message = Some(err(e));
                    self.transform.blocked_note = true;
                }
            }
            return;
        }
        let block = match (
            self.transform.copy,
            self.transform_now(),
            self.sketch_set(sk),
        ) {
            (false, Some(now), Some(set)) => sk.check_transform(&set, &now).err(),
            _ => None,
        };
        match block {
            None | Some(TransformBlock::NothingToDo) => {
                if std::mem::take(&mut self.transform.blocked_note) {
                    self.message = None;
                }
            }
            Some(block) => {
                self.message = Some(block_message(block));
                self.transform.blocked_note = true;
            }
        }
    }

    /// Nimmt nur den laufenden Griff zurück; bis alle Tasten los sind, ist nichts ein Klick oder ein
    /// neuer Griff ([`box_aborted`](SketchSession::box_aborted)).
    pub fn abort_grip(&mut self) {
        if let Some(d) = self.grip_drag.take() {
            self.transform.offset = d.start_offset;
            self.transform.angle = d.start_angle;
            self.transform.factor = d.start_factor;
            if let Some(d3) = self.grip_drag3.take() {
                self.transform.shift_z = d3.start_z;
                self.transform.angle_x = d3.start_angle_x;
                self.transform.angle_y = d3.start_angle_y;
            }
            self.box_aborted = true;
        }
    }

    /// „Bezugspunkt setzen“: Der nächste Klick setzt ihn, Verschiebung und Winkel bleiben. Getipptes
    /// wird dafür zum Stand, denn die Felder der aktiven Stufe gehen mit ihr.
    pub fn begin_set_base(&mut self) {
        if self.transform.stage != TransformStage::Active {
            return;
        }
        if let Some(now) = self.transform_now() {
            self.transform.offset = now.offset;
            self.transform.angle = now.angle;
            self.transform.factor = now.factor;
        }
        let input = &self.input;
        let t = &mut self.transform;
        t.shift_z = input.locked(FieldKind::MoveZ).unwrap_or(t.shift_z);
        t.angle_x = input.locked(FieldKind::MoveAngleX).unwrap_or(t.angle_x);
        t.angle_y = input.locked(FieldKind::MoveAngleY).unwrap_or(t.angle_y);
        self.grip_drag = None;
        self.draw_cursor = None;
        self.transform.stage = TransformStage::SetBase;
    }

    /// Klick: Beim Wählen schaltet er Kurve oder Punkt unter dem Zeiger, beim Bezugspunkt setzt er
    /// ihn an der aufgelösten Lage; in der aktiven Stufe tut er nichts.
    pub(crate) fn click_transform(&mut self, sk: &Sketch, cur: &DrawCursor) {
        match self.transform.stage {
            TransformStage::Picking => {
                let item = match self.snap(sk, cur.raw, self.pick_radius) {
                    Snap::Point(p) => Item::Point(p),
                    Snap::Curve(c) => Item::Curve(c),
                    _ => return,
                };
                self.drop_curve_picks();
                match self.selection.iter().position(|&s| s == item) {
                    Some(k) => {
                        self.selection.remove(k);
                    }
                    None => self.selection.push(item),
                }
                self.sync_picked(sk);
            }
            TransformStage::SetBase => {
                // Der Klick liegt in der Skizzenebene.
                self.transform.base = Some(cur.at);
                self.transform.base_z = 0.0;
                self.transform.stage = TransformStage::Active;
            }
            TransformStage::Active => {}
        }
    }

    /// Eingabe im Werkzeug: Beim Wählen weiter zu den Griffen, beim Bezugspunkt zurück, sonst
    /// abschließen.
    pub fn confirm_transform(&mut self, sf: &mut SketchFeature) -> Result<(), String> {
        // Mitten in einem Zug würde sonst eine halbe Bewegung oder eine halbe Wahl fest.
        if self.in_gesture() {
            return Ok(());
        }
        if self.transform.stage == TransformStage::Active {
            self.finish_transform(sf)
        } else {
            self.finish_tool(&mut sf.sketch)
        }
    }

    /// Enter oder OK: verschiebt oder kopiert, löst, und das Werkzeug geht zurück zur Auswahl. Die
    /// Auswahl bleibt, bei einer Kopie sind es die Kopien. Was nicht geht, meldet `message`, und alles
    /// bleibt, wie es war.
    pub fn finish_transform(&mut self, sf: &mut SketchFeature) -> Result<(), String> {
        self.sync_input(&sf.sketch);
        if let Some(e) = self.input.fields.iter().find_map(|f| f.error.clone()) {
            self.message = Some(Msg::Raw(e.clone()));
            return Err(e);
        }
        let scale = self.tool == Tool::Scale;
        let mut formulas = sf.formulas.clone();
        let copied = self.atomic(&mut sf.sketch, |se, sk, prior| {
            let (Some(now), Some(set)) = (se.transform_now(), se.sketch_set(sk)) else {
                return Err(crate::msg!("sketch-nothing-selected"));
            };
            let mut notes = Vec::new();
            let mut copied = None;
            if let Some(m) = se.moving3(sk) {
                // Aus der Ebene (Skizzen 10): ein Weg für Verschieben und Kopieren.
                let report = sk.move_rigid3(&set, &m, se.transform.copy).map_err(err)?;
                if report.removed_constraints > 0 {
                    notes.push(crate::msg!(
                        "sketch-move-dropped",
                        n = report.removed_constraints
                    ));
                }
                if report.converted_dimensions > 0 {
                    notes.push(crate::msg!(
                        "sketch-move-converted",
                        n = report.converted_dimensions
                    ));
                }
                if report.split > 0 {
                    notes.push(crate::msg!("sketch-move-split", n = report.split));
                }
                copied = report.copy;
            } else if se.transform.copy {
                copied = Some(sk.copy_transformed(&set, &now).map_err(err)?);
            } else {
                let report = sk.apply_transform(&set, &now).map_err(block_message)?;
                if scale {
                    // Die Formeln nehmen den Faktor mit; was dabei nicht mehr als Formel taugt,
                    // wird durch den Wert ersetzt (das Skalieren selbst scheitert daran nie).
                    let mut replaced = 0;
                    for &id in &report.scaled {
                        let Some(text) = formulas.get(&id).cloned() else {
                            continue;
                        };
                        let value = sk.constraint(id).and_then(|c| length_of(&c.kind));
                        let (new, plain) = scaled_or_value(&text, now.factor, value);
                        replaced += usize::from(plain);
                        match new {
                            Some(t) => formulas.insert(id, t),
                            None => formulas.remove(&id),
                        };
                    }
                    if replaced > 0 {
                        notes.push(crate::msg!("sketch-scale-formulas-replaced", n = replaced));
                    }
                }
                if !report.dropped.is_empty() {
                    notes.push(crate::msg!("sketch-move-dropped", n = report.dropped.len()));
                }
                if report.split > 0 {
                    notes.push(crate::msg!("sketch-move-split", n = report.split));
                }
            }
            se.solve_checked(sk, prior)?;
            se.transform.blocked_note = false;
            // Was das Lösen zu sagen hatte, bleibt neben dem, was die Bewegung zu sagen hat.
            notes.extend(se.message.take());
            se.message = match notes.len() {
                0 | 1 => notes.pop(),
                _ => Some(Msg::Join(notes)),
            };
            se.edit_label = Some(if scale {
                crate::msg!("undo-scale")
            } else {
                crate::msg!("undo-move-copy")
            });
            se.modify_edits = (se.modify_edits.0 + 1, Some(se.tool));
            Ok(copied)
        })?;
        if scale {
            sf.formulas = formulas;
        }
        if let Some(map) = &copied {
            self.take_copies(sf, map);
        }
        sf.prune_formulas();
        self.forget_removed(&sf.sketch);
        self.set_tool(&mut sf.sketch, Tool::Select);
        Ok(())
    }

    /// Nach dem Kopieren: Die Formeln gehen mit den Maßen mit, die Auswahl wird zu den Kopien.
    fn take_copies(&mut self, sf: &mut SketchFeature, map: &CopyMap) {
        for (old, new) in &map.constraints {
            if let Some(text) = sf.formulas.get(old).cloned() {
                sf.formulas.insert(*new, text);
            }
        }
        let curves = self.transform.set.curves.iter();
        let points = self.transform.set.points.iter();
        self.curve_picks = Default::default();
        self.selection = curves
            .filter_map(|c| map.curves.get(c).map(|&c| Item::Curve(c)))
            .chain(points.filter_map(|p| map.points.get(p).map(|&p| Item::Point(p))))
            .collect();
    }

    /// Die gewählte Menge samt mitgezogener Hilfsgeometrie (beim Maßstab ohne Auswahl die ganze
    /// Skizze); `None` ohne Menge.
    fn sketch_set(&self, sk: &Sketch) -> Option<SketchTransformSet> {
        let t = &self.transform.set;
        let set = if self.transform.whole {
            sk.whole_sketch_set()
        } else {
            sk.transform_set(&t.curves, &t.points)
        };
        (!set.points.is_empty()).then_some(set)
    }

    /// Vorschau als Linienzüge in der Reihenfolge der Menge; `None`, solange nichts zu zeigen ist oder
    /// die Bewegung nicht geht. Dieselbe Rechnung wie der Abschluss.
    pub fn transform_preview(&self, sk: &Sketch) -> Option<Vec<Vec<DVec2>>> {
        let styled = self.transform_preview_styled(sk)?;
        Some(styled.into_iter().map(|(_, line)| line).collect())
    }

    /// [`transform_preview`](Self::transform_preview) mit dem Stil jeder Kurve: Mitgezogene
    /// Hilfsgeometrie erscheint gestrichelt wie auf dem Blatt.
    pub fn transform_preview_styled(&self, sk: &Sketch) -> Option<Vec<(LineStyle, Vec<DVec2>)>> {
        // Beim Wählen steht noch nichts in Bewegung, auch mit dem Haken „Kopie erstellen“.
        if self.transform.stage == TransformStage::Picking && self.grip_drag.is_none() {
            return None;
        }
        let now = self.transform_now()?;
        let set = self.sketch_set(sk)?;
        let mut out = sk.clone();
        if let Some(m) = self.moving3(sk) {
            // In 3D zeigt die Draufsicht nur die Lage in der Ebene; die Ansicht nimmt
            // `transform_preview3_styled`.
            let copies = out.move_rigid3(&set, &m, self.transform.copy).ok()?.copy;
            return Some(preview_lines(&out, &set, copies.as_ref(), |sk, c| {
                sk.curve_polyline(c, PREVIEW_STEPS_PER_TURN)
            }));
        }
        let copies = if self.transform.copy {
            Some(out.copy_transformed(&set, &now).ok()?)
        } else {
            out.apply_transform(&set, &now).ok()?;
            None
        };
        let lines = set.curves.iter().filter_map(|c| {
            let c = copies
                .as_ref()
                .map_or(Some(*c), |m| m.curves.get(c).copied())?;
            Some((
                out.curve(c)?.style(),
                out.curve_polyline(c, PREVIEW_STEPS_PER_TURN)?,
            ))
        });
        Some(lines.collect())
    }

    /// Vorschau im Raum (Skizzen 10): Linienzüge mit z der bewegten Kurven samt Stil, in der
    /// Reihenfolge der Menge, für Verschieben/Kopieren mit „3D-Skizze“, auch in der Ebene. Dieselbe
    /// Rechnung wie der Abschluss; `None` wie bei
    /// [`transform_preview_styled`](Self::transform_preview_styled).
    pub fn transform_preview3_styled(&self, sk: &Sketch) -> Option<Vec<(LineStyle, Vec<DVec3>)>> {
        if self.tool != Tool::MoveCopy || !sk.three_d {
            return None;
        }
        if self.transform.stage == TransformStage::Picking && self.grip_drag.is_none() {
            return None;
        }
        let m = self.transform_now3()?;
        let set = self.sketch_set(sk)?;
        let mut out = sk.clone();
        let copies = out.move_rigid3(&set, &m, self.transform.copy).ok()?.copy;
        Some(preview_lines(&out, &set, copies.as_ref(), |sk, c| {
            crate::sketch_view::curve_points3(sk, c)
        }))
    }

    /// Wert der Felder von Verschieben/Kopieren: der Stand aus Griff und Tippen.
    pub(crate) fn transform_field_value(&self, kind: FieldKind) -> Option<f64> {
        let t = &self.transform;
        match kind {
            FieldKind::MoveX => Some(t.offset.x),
            FieldKind::MoveY => Some(t.offset.y),
            FieldKind::MoveAngle => Some(t.angle),
            FieldKind::MoveZ => Some(t.shift_z),
            FieldKind::MoveAngleX => Some(t.angle_x),
            FieldKind::MoveAngleY => Some(t.angle_y),
            FieldKind::ScaleFactor => Some(t.factor),
            _ => None,
        }
    }
}

/// Die Linienzüge der bewegten Kurven der Menge (bei einer Kopie die der Kopien) in `out`, mit
/// Stil, abgetastet mit `sample`.
fn preview_lines<T>(
    out: &Sketch,
    set: &SketchTransformSet,
    copies: Option<&CopyMap>,
    sample: impl Fn(&Sketch, CurveId) -> Option<Vec<T>>,
) -> Vec<(LineStyle, Vec<T>)> {
    set.curves
        .iter()
        .filter_map(|c| {
            let c = copies.map_or(Some(*c), |m| m.curves.get(c).copied())?;
            Some((out.curve(c)?.style(), sample(out, c)?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::scaled_formula;

    #[test]
    fn the_formula_gets_old_times_k_with_a_short_number_and_a_point() {
        let f = |t: &str, k: f64| scaled_formula(t, k).unwrap();
        assert_eq!(f("Breite / 2", 3.0), "(Breite / 2) * 3");
        assert_eq!(f(" a ", 0.5), "a * 0.5");
        assert_eq!(f("20", 2.0), "20 * 2");
        assert_eq!(f("1 in", 12.25), "(1 in) * 12.25");
        assert_eq!(f("x", 1e-7), "x * 0.0000001");
    }

    #[test]
    fn a_trailing_factor_is_folded_so_the_formula_does_not_grow() {
        let f = |t: &str, k: f64| scaled_formula(t, k).unwrap();
        assert_eq!(f("(x) * 3", 2.0), "(x) * 6");
        assert_eq!(f("(Breite / 2) * 3", 0.5), "(Breite / 2) * 1.5");
        assert_eq!(f("a * 4", 0.25), "a * 1");
        // Kein Kopf, dem man den Faktor ansehen darf: Klammern drum.
        assert_eq!(f("a + b * 3", 2.0), "(a + b * 3) * 2");
        assert_eq!(f("(a) + (b) * 3", 2.0), "((a) + (b) * 3) * 2");
        let mut t = "Breite / 2".to_string();
        for i in 0..40 {
            t = f(&t, 1.0 + f64::from(i) * 0.1234567891234);
        }
        assert!(t.chars().count() < 60, "{t}");
        assert!(planora_feature::expr::parse(&t).is_ok());
    }

    #[test]
    fn a_factor_without_a_short_form_gives_none() {
        assert_eq!(scaled_formula("x", 1e-200), None);
        assert_eq!(scaled_formula("x", 1e40), None);
        assert_eq!(scaled_formula("x * 0", 2.0), None);
        // Viele Stellen werden auf zwölf gerundet, weit unter einem Milliardstel.
        let text = scaled_formula("x", 0.123_456_789_012_345_68).unwrap();
        assert_eq!(text, "x * 0.123456789012");
    }
}
