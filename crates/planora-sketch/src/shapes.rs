//! Reine Geometrie der Zeichenvarianten (Rechteck, Kreis, Bogen, Polygon, Langloch).
//!
//! Alle Funktionen rechnen nur mit Lagen und kennen die Skizze nicht; Vorschau und Anlegen
//! nehmen denselben Plan. `tol` ist die lineare Toleranz. Bei Entartung (zusammenfallende oder
//! auf einer Geraden liegende Punkte, Größe null) liefern sie `None`.

use planora_math::{DVec2, PI, TAU};

fn all_finite(points: &[DVec2]) -> bool {
    points.iter().all(|p| p.is_finite())
}

fn rotated(v: DVec2, angle: f64) -> DVec2 {
    let (s, c) = angle.sin_cos();
    DVec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Bogen gegen den Uhrzeigersinn von `start` nach `end` um `center`; der Radius ist der Abstand
/// von `start` zur Mitte.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcPlan {
    pub center: DVec2,
    pub start: DVec2,
    pub end: DVec2,
}

impl ArcPlan {
    /// Öffnungswinkel in (0, 2π], gegen den Uhrzeigersinn von Start nach Ende.
    pub fn sweep(&self) -> f64 {
        let s = (self.end - self.center).to_angle() - (self.start - self.center).to_angle();
        let s = s.rem_euclid(TAU);
        if s == 0.0 { TAU } else { s }
    }

    pub fn radius(&self) -> f64 {
        (self.start - self.center).length()
    }
}

/// Rechteck aus Kante `a`–`b` und einem Punkt, der die Tiefe (mit Vorzeichen) bestimmt.
/// Reihenfolge der Ecken: a, b, b+n, a+n.
pub fn rect_three_point(a: DVec2, b: DVec2, depth_at: DVec2, tol: f64) -> Option<[DVec2; 4]> {
    if !all_finite(&[a, b, depth_at]) {
        return None;
    }
    let d = b - a;
    let len = d.length();
    if len <= tol {
        return None;
    }
    let normal = d.perp() / len;
    let depth = (depth_at - a).dot(normal);
    if depth.abs() <= tol {
        return None;
    }
    let n = normal * depth;
    Some([a, b, b + n, a + n])
}

/// Achsenparalleles Rechteck um `center` mit Ecke `corner`; Reihenfolge wie `RectIds`.
pub fn rect_center(center: DVec2, corner: DVec2, tol: f64) -> Option<[DVec2; 4]> {
    if !all_finite(&[center, corner]) {
        return None;
    }
    let h = (corner - center).abs();
    if h.x <= tol || h.y <= tol {
        return None;
    }
    Some([
        center + DVec2::new(-h.x, -h.y),
        center + DVec2::new(h.x, -h.y),
        center + DVec2::new(h.x, h.y),
        center + DVec2::new(-h.x, h.y),
    ])
}

/// Kreis durch drei Punkte: Mitte und Radius.
pub fn circle_through(a: DVec2, b: DVec2, c: DVec2, tol: f64) -> Option<(DVec2, f64)> {
    if !all_finite(&[a, b, c]) {
        return None;
    }
    let ab = b - a;
    let ac = c - a;
    if ab.length() <= tol || ac.length() <= tol || (c - b).length() <= tol {
        return None;
    }
    let cross = ab.perp_dot(ac);
    // Abstand von c zur Geraden ab
    if cross.abs() / ab.length() <= tol {
        return None;
    }
    let d = 2.0 * cross;
    let (ab2, ac2) = (ab.length_squared(), ac.length_squared());
    let u = DVec2::new((ac.y * ab2 - ab.y * ac2) / d, (ab.x * ac2 - ac.x * ab2) / d);
    let radius = u.length();
    let center = a + u;
    (center.is_finite() && radius.is_finite()).then_some((center, radius))
}

/// Bogen von `start` nach `end` durch `on`. Liegt `on` im Uhrzeigersinn, sind Start und Ende
/// getauscht, damit der Bogen gegen den Uhrzeigersinn läuft.
pub fn arc_through(start: DVec2, end: DVec2, on: DVec2, tol: f64) -> Option<ArcPlan> {
    let (center, _) = circle_through(start, end, on, tol)?;
    if (end - start).perp_dot(on - start) > 0.0 {
        Some(ArcPlan {
            center,
            start: end,
            end: start,
        })
    } else {
        Some(ArcPlan { center, start, end })
    }
}

/// Tangentialer Bogen ab `start` in Richtung `leaving` bis `end`. Die Mitte liegt auf der
/// Normalen durch `start`; ist der Radius negativ, sind Start und Ende getauscht.
pub fn tangent_arc(start: DVec2, leaving: DVec2, end: DVec2, tol: f64) -> Option<ArcPlan> {
    if !all_finite(&[start, leaving, end]) {
        return None;
    }
    let dir = leaving.try_normalize()?;
    let n = dir.perp();
    let e = end - start;
    let en = e.dot(n);
    if e.length() <= tol || en.abs() <= tol {
        return None;
    }
    let rho = e.length_squared() / (2.0 * en);
    let center = start + n * rho;
    if !center.is_finite() {
        return None;
    }
    if rho > 0.0 {
        Some(ArcPlan { center, start, end })
    } else {
        Some(ArcPlan {
            center,
            start: end,
            end: start,
        })
    }
}

/// Ende eines tangentialen Bogens bei gesperrtem Radius und/oder Öffnungswinkel. `toward`
/// wählt die Seite und, wo nicht gesperrt, Radius und Winkel.
pub fn tangent_arc_locked(
    start: DVec2,
    leaving: DVec2,
    radius: Option<f64>,
    sweep: Option<f64>,
    toward: DVec2,
    tol: f64,
) -> Option<DVec2> {
    if !all_finite(&[start, leaving, toward]) {
        return None;
    }
    let dir = leaving.try_normalize()?;
    let n = dir.perp();
    let off = toward - start;
    let side_dist = off.dot(n);
    if side_dist.abs() <= tol {
        return None;
    }
    let side = side_dist.signum();
    let sweep_ok = |t: f64| t.is_finite() && t > 0.0 && t <= TAU;
    match (radius, sweep) {
        (None, None) => Some(toward),
        (Some(r), None) => {
            if !(r.is_finite() && r > tol) {
                return None;
            }
            let center = start + n * (side * r);
            let to = (toward - center).try_normalize()?;
            Some(center + to * r)
        }
        (Some(r), Some(theta)) => {
            if !(r.is_finite() && r > tol && sweep_ok(theta)) {
                return None;
            }
            let center = start + n * (side * r);
            Some(center + rotated(start - center, side * theta))
        }
        (None, Some(theta)) => {
            if !sweep_ok(theta) {
                return None;
            }
            let half_sin = (theta / 2.0).sin();
            if half_sin <= 1e-9 {
                return None;
            }
            let chord_dir = rotated(dir, side * theta / 2.0);
            let r = off.dot(chord_dir) / (2.0 * half_sin);
            if !(r.is_finite() && r > tol) {
                return None;
            }
            let center = start + n * (side * r);
            Some(center + rotated(start - center, side * theta))
        }
    }
}

/// Regelmäßiges Vieleck: Mitte und Ecken in Umlaufrichtung der jeweiligen Art.
#[derive(Clone, Debug, PartialEq)]
pub struct PolygonPlan {
    pub center: DVec2,
    pub corners: Vec<DVec2>,
}

fn sides_ok(sides: usize) -> bool {
    (3..=64).contains(&sides)
}

fn regular_corners(center: DVec2, first: DVec2, sides: usize, sign: f64) -> Vec<DVec2> {
    let v = first - center;
    let step = sign * TAU / sides as f64;
    (0..sides)
        .map(|i| {
            if i == 0 {
                first
            } else {
                center + rotated(v, step * i as f64)
            }
        })
        .collect()
}

/// Einbeschrieben: `corner` ist die erste Ecke.
pub fn polygon_inscribed(
    center: DVec2,
    corner: DVec2,
    sides: usize,
    tol: f64,
) -> Option<PolygonPlan> {
    if !sides_ok(sides) || !all_finite(&[center, corner]) {
        return None;
    }
    if (corner - center).length() <= tol {
        return None;
    }
    Some(PolygonPlan {
        center,
        corners: regular_corners(center, corner, sides, 1.0),
    })
}

/// Umbeschrieben: `side_mid` ist die Mitte der Seite `corners[0]`–`corners[1]`.
pub fn polygon_circumscribed(
    center: DVec2,
    side_mid: DVec2,
    sides: usize,
    tol: f64,
) -> Option<PolygonPlan> {
    if !sides_ok(sides) || !all_finite(&[center, side_mid]) {
        return None;
    }
    let v = side_mid - center;
    let apothem = v.length();
    if apothem <= tol {
        return None;
    }
    let half = PI / sides as f64;
    let circum = apothem / half.cos();
    let first = center + rotated(v / apothem, -half) * circum;
    Some(PolygonPlan {
        center,
        corners: regular_corners(center, first, sides, 1.0),
    })
}

/// Kantenpolygon: `corners[0] == a`, `corners[1] == b`, die Mitte liegt auf der Seite von
/// `side_at`.
pub fn polygon_edge(
    a: DVec2,
    b: DVec2,
    side_at: DVec2,
    sides: usize,
    tol: f64,
) -> Option<PolygonPlan> {
    if !sides_ok(sides) || !all_finite(&[a, b, side_at]) {
        return None;
    }
    let d = b - a;
    let len = d.length();
    if len <= tol {
        return None;
    }
    let normal = d.perp() / len;
    let side_dist = (side_at - a).dot(normal);
    if side_dist.abs() <= tol {
        return None;
    }
    let sign = side_dist.signum();
    let apothem = len / (2.0 * (PI / sides as f64).tan());
    let center = (a + b) * 0.5 + normal * (sign * apothem);
    let mut corners = regular_corners(center, a, sides, sign);
    corners[1] = b;
    Some(PolygonPlan { center, corners })
}

/// Gerades Langloch: Mitten der Endkreise `a` und `b`, Radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotPlan {
    pub a: DVec2,
    pub b: DVec2,
    pub radius: f64,
}

fn dist_to_line(p: DVec2, a: DVec2, dir: DVec2) -> f64 {
    dir.perp_dot(p - a).abs()
}

/// Mitte zu Mitte: Der Abstand von `width_at` zur Achse ist der Radius.
pub fn slot_center_to_center(a: DVec2, b: DVec2, width_at: DVec2, tol: f64) -> Option<SlotPlan> {
    if !all_finite(&[a, b, width_at]) {
        return None;
    }
    let len = (b - a).length();
    if len <= tol {
        return None;
    }
    let radius = dist_to_line(width_at, a, (b - a) / len);
    (radius > tol).then_some(SlotPlan { a, b, radius })
}

/// Gesamtlänge: `o0` und `o1` sind die äußersten Enden.
pub fn slot_overall(o0: DVec2, o1: DVec2, width_at: DVec2, tol: f64) -> Option<SlotPlan> {
    if !all_finite(&[o0, o1, width_at]) {
        return None;
    }
    let len = (o1 - o0).length();
    if len <= tol {
        return None;
    }
    let dir = (o1 - o0) / len;
    let radius = dist_to_line(width_at, o0, dir);
    if radius <= tol || 2.0 * radius >= len - tol {
        return None;
    }
    Some(SlotPlan {
        a: o0 + dir * radius,
        b: o1 - dir * radius,
        radius,
    })
}

/// Mittelpunkt: `c` ist die Mitte, `end` die Mitte eines Endkreises.
pub fn slot_center_point(c: DVec2, end: DVec2, width_at: DVec2, tol: f64) -> Option<SlotPlan> {
    if !all_finite(&[c, end, width_at]) {
        return None;
    }
    let len = (end - c).length();
    if len <= tol {
        return None;
    }
    let radius = dist_to_line(width_at, c, (end - c) / len);
    (radius > tol).then_some(SlotPlan {
        a: c * 2.0 - end,
        b: end,
        radius,
    })
}

/// Bogenlangloch: Achsenbogen und halbe Breite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcSlotPlan {
    pub axis: ArcPlan,
    pub half_width: f64,
}

pub fn arc_slot(axis: ArcPlan, width_at: DVec2, tol: f64) -> Option<ArcSlotPlan> {
    if !all_finite(&[axis.center, axis.start, axis.end, width_at]) {
        return None;
    }
    let r = axis.radius();
    let h = ((width_at - axis.center).length() - r).abs();
    if !(h.is_finite() && h > tol && h < r - tol) {
        return None;
    }
    let gap = TAU - axis.sweep();
    if gap < PI && 2.0 * r * (gap / 2.0).sin() <= 2.0 * h {
        return None;
    }
    Some(ArcSlotPlan {
        axis,
        half_width: h,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub enum ShapePlan {
    Quad([DVec2; 4]),
    Circle(DVec2, f64),
    Arc(ArcPlan),
    Polygon(PolygonPlan),
    Slot(SlotPlan),
    ArcSlot(ArcSlotPlan),
}

fn push_arc(out: &mut Vec<DVec2>, center: DVec2, from: DVec2, sweep: f64, steps_per_turn: usize) {
    let steps = ((steps_per_turn.max(4) as f64 * sweep.abs() / TAU).ceil() as usize).max(1);
    let v = from - center;
    for k in 0..=steps {
        let p = center + rotated(v, sweep * k as f64 / steps as f64);
        if out.last().is_none_or(|l| (*l - p).length() > 1e-12) {
            out.push(p);
        }
    }
}

fn close(out: &mut Vec<DVec2>) {
    if let Some(&first) = out.first() {
        match out.last_mut() {
            Some(l) if (*l - first).length() <= 1e-12 => *l = first,
            _ => out.push(first),
        }
    }
}

impl ShapePlan {
    /// Umriss als Linienzug. Geschlossene Formen enden mit ihrem ersten Punkt; der Bogen ist
    /// die offene Kurve selbst. Zentren und Achsen gehören nicht dazu.
    pub fn outline(&self, steps_per_turn: usize) -> Vec<Vec<DVec2>> {
        let mut out = Vec::new();
        match self {
            ShapePlan::Quad(q) => {
                out.extend_from_slice(q);
                close(&mut out);
            }
            ShapePlan::Circle(c, r) => {
                push_arc(&mut out, *c, *c + DVec2::new(*r, 0.0), TAU, steps_per_turn);
                close(&mut out);
            }
            ShapePlan::Arc(arc) => {
                push_arc(&mut out, arc.center, arc.start, arc.sweep(), steps_per_turn);
            }
            ShapePlan::Polygon(p) => {
                out.extend_from_slice(&p.corners);
                close(&mut out);
            }
            ShapePlan::Slot(s) => {
                let Some(dir) = (s.b - s.a).try_normalize() else {
                    return Vec::new();
                };
                let n = dir.perp() * s.radius;
                out.push(s.a - n);
                out.push(s.b - n);
                push_arc(&mut out, s.b, s.b - n, PI, steps_per_turn);
                out.push(s.a + n);
                push_arc(&mut out, s.a, s.a + n, PI, steps_per_turn);
                close(&mut out);
            }
            ShapePlan::ArcSlot(s) => {
                let a = s.axis;
                let sweep = a.sweep();
                let (r, h) = (a.radius(), s.half_width);
                let radial_start = (a.start - a.center) / r;
                let radial_end = (a.end - a.center) / r;
                let outer_start = a.center + radial_start * (r + h);
                push_arc(&mut out, a.center, outer_start, sweep, steps_per_turn);
                push_arc(&mut out, a.end, a.end + radial_end * h, PI, steps_per_turn);
                let inner_end = a.center + radial_end * (r - h);
                push_arc(&mut out, a.center, inner_end, -sweep, steps_per_turn);
                push_arc(
                    &mut out,
                    a.start,
                    a.start - radial_start * h,
                    PI,
                    steps_per_turn,
                );
                close(&mut out);
            }
        }
        vec![out]
    }
}
