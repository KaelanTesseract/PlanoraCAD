//! Ziehgriff an der Vorschau einer Konstruktionsebene (wie Fusion): ein Pfeil entlang der Normale
//! für den Abstand der Versatzebene, ein Drehpfeil um die Achse für den Winkel der Ebene unter
//! Winkel. Das Fenster gibt die Lage über [`crate::plane_dialog::PlanePick::grip`]; die Ansicht
//! liest Zeiger und Tasten ([`input`]), zeichnet den Griff ([`paint`]) und meldet einen neuen
//! Wert über [`PlaneGrip::moved`], den das Fenster als Zahl in sein Feld übernimmt.

use crate::state::AppState;
use crate::theme::{lighten, sketch_colors};
use egui::{PointerButton, Pos2, Rect, Shape, Stroke, pos2};
use glam::Vec2;
use planora_math::DVec3;

/// Länge des Abstandspfeils in Pixeln.
pub const ARROW_PX: f32 = 70.0;
/// Fangweite um Pfeil und Drehgriff in Pixeln.
pub const HIT_PX: f32 = 9.0;
/// Halber Öffnungswinkel des Drehpfeils (Grad).
const ARC_HALF_DEG: f64 = 18.0;

/// Form des Griffs, in Weltkoordinaten (Millimeter).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GripShape {
    /// Abstand: Pfeil von `at` in Richtung `dir` (Einheitsvektor, Normale der Ebene); `value` in
    /// Millimetern.
    Distance { at: DVec3, dir: DVec3, value: f64 },
    /// Winkel um die Achse durch `center` mit der Richtung `axis` (Einheitsvektor); der Griff
    /// sitzt bei `handle` am Rand der Vorschau. `value` in Grad.
    Angle {
        center: DVec3,
        axis: DVec3,
        handle: DVec3,
        value: f64,
    },
}

impl GripShape {
    pub fn value(self) -> f64 {
        match self {
            GripShape::Distance { value, .. } | GripShape::Angle { value, .. } => value,
        }
    }
}

/// Laufender Zug: Wert und Lage des Zeigers (Parameter auf dem Pfeil oder Richtung um die
/// Achse) beim Greifen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GripDrag {
    pub start_value: f64,
    pub start: DVec3,
    /// Form beim Greifen: Während des Zugs rechnet der Griff gegen sie, nicht gegen die
    /// mitwandernde Vorschau.
    pub shape: GripShape,
}

/// Griff an der Vorschau und sein Zustand zwischen den Bildern.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneGrip {
    pub shape: GripShape,
    /// Der Zeiger steht auf dem Griff.
    pub hot: bool,
    pub drag: Option<GripDrag>,
    /// Neuer Wert aus der Ansicht, den das Fenster noch nicht abgeholt hat.
    pub moved: Option<f64>,
    /// Esc hat in diesem Bild nur den Zug abgebrochen: Das Fenster schließt darauf nicht.
    pub esc_taken: bool,
}

impl PlaneGrip {
    pub fn new(shape: GripShape) -> PlaneGrip {
        PlaneGrip {
            shape,
            hot: false,
            drag: None,
            moved: None,
            esc_taken: false,
        }
    }

    /// Nimmt der Griff den Zeiger (unter dem Zeiger oder im Zug)?
    pub fn busy(&self) -> bool {
        self.hot || self.drag.is_some()
    }

    /// Neue Form aus dem Fenster; Zug, Zeigerzustand und ein noch nicht abgeholter Wert bleiben.
    pub fn follow(old: Option<PlaneGrip>, shape: GripShape) -> PlaneGrip {
        match old {
            Some(g) => PlaneGrip {
                shape,
                esc_taken: false,
                ..g
            },
            None => PlaneGrip::new(shape),
        }
    }
}

/// Abbildung zwischen Welt und Bildschirm der Ansicht `rect`.
struct Map<'a> {
    state: &'a AppState,
    rect: Rect,
}

impl Map<'_> {
    fn size(&self) -> Vec2 {
        Vec2::new(self.rect.width(), self.rect.height())
    }

    fn to_screen(&self, p: DVec3) -> Option<Pos2> {
        let v = self.state.camera.project(p.as_vec3(), self.size())?;
        Some(pos2(self.rect.left() + v.x, self.rect.top() + v.y))
    }

    /// Strahl (Ursprung, Richtung) durch den Bildschirmpunkt `p`.
    fn ray(&self, p: Pos2) -> (DVec3, DVec3) {
        let s = self.size();
        let ndc = Vec2::new(
            (p.x - self.rect.left()) / s.x * 2.0 - 1.0,
            1.0 - (p.y - self.rect.top()) / s.y * 2.0,
        );
        let (o, d) = self.state.camera.ray(ndc);
        (o.as_dvec3(), d.as_dvec3())
    }

    /// Weltlänge, die an `at` in Richtung `dir` `px` Pixel lang erscheint.
    fn world_len(&self, at: DVec3, dir: DVec3, px: f32) -> Option<f64> {
        let a = self.to_screen(at)?;
        let b = self.to_screen(at + dir)?;
        let per_mm = a.distance(b) as f64;
        (per_mm > 1e-6).then(|| px as f64 / per_mm)
    }
}

/// Parameter `t` des Punkts auf der Geraden `at + dir·t`, der dem Strahl am nächsten liegt;
/// `None`, wenn der Strahl fast entlang der Geraden blickt.
fn line_param(at: DVec3, dir: DVec3, ray: (DVec3, DVec3)) -> Option<f64> {
    let (o, r) = ray;
    let b = dir.dot(r);
    let denom = 1.0 - b * b;
    if denom < 1e-6 {
        return None;
    }
    let w = at - o;
    Some((b * r.dot(w) - dir.dot(w)) / denom)
}

/// Schnitt des Strahls mit der Ebene durch `center` senkrecht zu `axis`, als Vektor von `center`.
fn on_disc(center: DVec3, axis: DVec3, ray: (DVec3, DVec3)) -> Option<DVec3> {
    let (o, r) = ray;
    let den = r.dot(axis);
    if den.abs() < 1e-6 {
        return None;
    }
    let s = (center - o).dot(axis) / den;
    let v = o + r * s - center;
    let v = v - axis * v.dot(axis);
    (v.length() > 1e-9).then_some(v)
}

/// Vorzeichenbehafteter Winkel (Grad) von `a` nach `b` um `axis`.
fn signed_deg(a: DVec3, b: DVec3, axis: DVec3) -> f64 {
    axis.dot(a.cross(b)).atan2(a.dot(b)).to_degrees()
}

/// Pfeil auf dem Bildschirm: Fuß und Spitze; `None`, wenn er nicht zu sehen ist.
fn arrow(map: &Map, at: DVec3, dir: DVec3) -> Option<(Pos2, Pos2)> {
    let len = map.world_len(at, dir, ARROW_PX)?;
    Some((map.to_screen(at)?, map.to_screen(at + dir * len)?))
}

/// Punkte des Drehpfeils auf dem Bildschirm.
fn arc(map: &Map, center: DVec3, axis: DVec3, handle: DVec3) -> Vec<Pos2> {
    let arm = handle - center;
    let n = 16;
    (0..=n)
        .filter_map(|i| {
            let deg = -ARC_HALF_DEG + 2.0 * ARC_HALF_DEG * i as f64 / n as f64;
            let q = glam::DQuat::from_axis_angle(axis, deg.to_radians());
            map.to_screen(center + q * arm)
        })
        .collect()
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Liegt `p` auf dem Griff?
fn hits(map: &Map, shape: GripShape, p: Pos2) -> bool {
    match shape {
        GripShape::Distance { at, dir, .. } => {
            arrow(map, at, dir).is_some_and(|(a, b)| dist_to_segment(p, a, b) <= HIT_PX)
        }
        GripShape::Angle {
            center,
            axis,
            handle,
            ..
        } => {
            let pts = arc(map, center, axis, handle);
            pts.windows(2)
                .any(|w| dist_to_segment(p, w[0], w[1]) <= HIT_PX)
        }
    }
}

/// Lage des Zeigers für den Zug: Punkt auf dem Pfeil oder Richtung um die Achse.
fn grab(map: &Map, shape: GripShape, p: Pos2) -> Option<DVec3> {
    let ray = map.ray(p);
    match shape {
        GripShape::Distance { at, dir, .. } => line_param(at, dir, ray).map(|t| at + dir * t),
        GripShape::Angle { center, axis, .. } => on_disc(center, axis, ray),
    }
}

/// Wert aus dem Zug bis zum Zeiger `p`.
fn dragged(map: &Map, d: GripDrag, p: Pos2) -> Option<f64> {
    let now = grab(map, d.shape, p)?;
    Some(match d.shape {
        GripShape::Distance { dir, .. } => d.start_value + (now - d.start).dot(dir),
        GripShape::Angle { axis, .. } => d.start_value + signed_deg(d.start, now, axis),
    })
}

/// Zeiger und Tasten für den Griff des Ebenen-Fensters. `hover`: Zeiger über der Ansicht (nicht
/// über Würfel und Home). Liefert `true`, solange der Griff den Zeiger nimmt (dann wählt ein
/// Klick nichts und die Ansicht dreht nicht).
pub fn input(ui: &egui::Ui, rect: Rect, state: &mut AppState, hover: Option<Pos2>) -> bool {
    let Some(mut g) = state.plane_pick.and_then(|p| p.grip) else {
        return false;
    };
    let map = Map { state, rect };
    let (down, pressed, latest, secondary, esc) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.primary_pressed(),
            i.pointer.latest_pos(),
            i.pointer.button_pressed(PointerButton::Secondary),
            i.key_pressed(egui::Key::Escape),
        )
    });
    let esc = esc && !crate::viewport::typing(ui.ctx());
    if let Some(d) = g.drag {
        if secondary || esc {
            // Wie bei den übrigen Griffen: Esc oder Rechtsklick nimmt nur den Zug zurück.
            g.moved = Some(d.start_value);
            g.drag = None;
            g.esc_taken = esc;
        } else if down {
            if let Some(v) = latest.and_then(|p| dragged(&map, d, p))
                && v.is_finite()
            {
                g.moved = Some(v);
            }
        } else {
            g.drag = None;
        }
    } else {
        g.hot = hover.is_some_and(|p| hits(&map, g.shape, p));
        if g.hot
            && pressed
            && let Some(p) = hover
            && let Some(start) = grab(&map, g.shape, p)
        {
            g.drag = Some(GripDrag {
                start_value: g.shape.value(),
                start,
                shape: g.shape,
            });
        }
    }
    let busy = g.busy();
    if let Some(pick) = state.plane_pick.as_mut() {
        pick.grip = Some(g);
    }
    busy
}

/// Zeichnet den Griff des Ebenen-Fensters (über der Szene).
pub fn paint(painter: &egui::Painter, rect: Rect, state: &AppState) {
    let Some(g) = state.plane_pick.and_then(|p| p.grip) else {
        return;
    };
    let map = Map { state, rect };
    let col = sketch_colors(state.dark);
    let color = if g.busy() {
        lighten(col.selected)
    } else {
        col.selected
    };
    let width = if g.busy() { 3.0 } else { 2.0 };
    let stroke = Stroke::new(width, color);
    let head = |painter: &egui::Painter, tip: Pos2, from: Pos2| {
        let v = (tip - from).normalized();
        let n = egui::vec2(-v.y, v.x);
        let back = tip - v * 12.0;
        painter.add(Shape::convex_polygon(
            vec![tip, back + n * 6.0, back - n * 6.0],
            color,
            Stroke::NONE,
        ));
    };
    match g.shape {
        GripShape::Distance { at, dir, .. } => {
            if let Some((a, b)) = arrow(&map, at, dir) {
                painter.line_segment([a, b], stroke);
                head(painter, b, a);
                painter.circle_filled(a, 3.5, color);
            }
        }
        GripShape::Angle {
            center,
            axis,
            handle,
            ..
        } => {
            let pts = arc(&map, center, axis, handle);
            if pts.len() >= 2 {
                painter.add(Shape::line(pts.clone(), stroke));
                head(painter, pts[pts.len() - 1], pts[pts.len() - 2]);
                head(painter, pts[0], pts[1]);
            }
        }
    }
}

/// Hilfen für Tests: Bildschirmlage des Griffs (Pfeilspitze bzw. Mitte des Drehpfeils).
#[doc(hidden)]
pub fn grip_point(state: &AppState, rect: Rect) -> Option<Pos2> {
    let g = state.plane_pick?.grip?;
    let map = Map { state, rect };
    match g.shape {
        GripShape::Distance { at, dir, .. } => {
            let (a, b) = arrow(&map, at, dir)?;
            Some(a + (b - a) * 0.6)
        }
        GripShape::Angle { handle, .. } => map.to_screen(handle),
    }
}

/// Nur für Tests: wo landet der Weltpunkt `p` auf dem Bildschirm der Ansicht `rect`?
#[doc(hidden)]
pub fn screen_of(state: &AppState, rect: Rect, p: DVec3) -> Option<Pos2> {
    Map { state, rect }.to_screen(p)
}
