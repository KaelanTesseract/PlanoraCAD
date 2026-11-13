//! Geometrie aus DXF und SVG in die Skizze einfügen (Skizzen 9, Spec B).
//!
//! Ablauf von [`Sketch::insert_geometry`]:
//! 1. Die gewählten Elemente werden mit der [`Placement`] in Skizzenkoordinaten gebracht und in
//!    Bausteine ([`Prim`]) zerlegt, die es als Skizzenkurve gibt. NURBS werden dabei auf Grad 3
//!    oder 5 gebracht und an Knoten geteilt, damit ein Spline höchstens
//!    [`MAX_SPLINE_POINTS`] Punkte hat.
//! 2. Die Endpunkte aller Bausteine werden über ein Raster mit der Toleranz
//!    [`MERGE_REL`] × Diagonale des Hüllquaders zusammengeführt. Vorhandene Punkte der Skizze
//!    bleiben außen vor.
//! 3. Die Kurven entstehen auf einer Kopie der Skizze. Danach wird einmal gelöst, und erst dann
//!    ersetzt die Kopie die Skizze (alles oder nichts).
//!
//! Entartetes (etwa eine Linie, deren Enden zusammenfallen) wird übersprungen und unter
//! `skipped.degenerate` gezählt; ebenso jedes andere Element, das die Skizze ablehnt (etwa ein
//! Spline, dessen Nachbarpunkte zu nah liegen). Ein Text, der nicht geht (zu lang, ohne Umriss,
//! zu viele Kurven, Höhe 0), zählt unter `skipped.texts`. Ein solches Element wird vollständig
//! zurückgenommen, der Rest kommt trotzdem. `Err` gibt es nur für das Ganze: ungültige Lage oder
//! Werte außerhalb des Rechenbereichs.

use crate::constraint::ConstraintKind;
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, MAX_SPLINE_POINTS, PointId, Sketch, within_coord_range};
use crate::text::{HAlign, TextParams, VAlign};
use crate::text_refresh::{CUBIC_KNOTS, QUAD_RHO};
use crate::text_shape::{TextShaper, TextStyle};
use planora_exchange::import2d::{Element2d, HAlign2d, Import2d, Skipped, Text2d, VAlign2d};
use planora_i18n::tr;
use planora_math::DVec2;
use planora_math::nurbs::NurbsCurve;
use std::collections::{HashMap, HashSet};
use std::f64::consts::TAU;
use std::sync::Arc;

/// Endpunkte, die näher als dieser Anteil der Diagonale des Hüllquaders beieinander liegen,
/// werden ein Punkt (Spec B „Zusammenführen“).
pub const MERGE_REL: f64 = 1e-6;
/// Höchste Abweichung beim Zerlegen von NURBS über Grad 5 in kubische Stücke, als Anteil der
/// Diagonale (Spec B „NURBS“).
pub const SPLIT_REL: f64 = 1e-6;
/// Höchste Teilungstiefe eines Bézierstücks beim Zerlegen (2²⁴ Stücke reichen für jede Kurve).
const MAX_SPLIT_DEPTH: u32 = 24;

/// Lage der eingefügten Geometrie: `apply(p) = R(angle) · (scale ⊙ (p − pivot)) + offset`.
/// Erst wird `pivot` auf den Ursprung geschoben, dann gespiegelt und skaliert, gedreht und
/// verschoben.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// mm je Dateieinheit in x und y; negativ heißt spiegeln.
    pub scale: DVec2,
    /// Drehung in Radiant, gegen den Uhrzeigersinn.
    pub angle: f64,
    pub offset: DVec2,
    pub pivot: DVec2,
}

impl Default for Placement {
    /// Die Abbildung, die nichts ändert.
    fn default() -> Self {
        Placement {
            scale: DVec2::ONE,
            angle: 0.0,
            offset: DVec2::ZERO,
            pivot: DVec2::ZERO,
        }
    }
}

impl Placement {
    pub fn apply(&self, p: DVec2) -> DVec2 {
        self.lin(p - self.pivot) + self.offset
    }

    /// Kehrt die Abbildung den Umlaufsinn um (genau eine Achse gespiegelt)?
    pub fn mirrors(&self) -> bool {
        self.scale.x * self.scale.y < 0.0
    }

    /// Nur der lineare Teil (für Richtungen und Halbachsen).
    fn lin(&self, v: DVec2) -> DVec2 {
        let q = v * self.scale;
        let (s, c) = self.angle.sin_cos();
        DVec2::new(c * q.x - s * q.y, s * q.x + c * q.y)
    }

    /// Gleicher Betrag des Maßstabs in beiden Achsen: dann dieser Betrag.
    fn uniform(&self) -> Option<f64> {
        let (a, b) = (self.scale.x.abs(), self.scale.y.abs());
        (a > 0.0 && (a - b).abs() <= 1e-12 * a).then_some(a)
    }

    fn valid(&self) -> bool {
        self.scale.is_finite()
            && self.scale.x != 0.0
            && self.scale.y != 0.0
            && self.angle.is_finite()
            && self.offset.is_finite()
            && self.pivot.is_finite()
    }
}

/// Einstellungen des Einfügens.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InsertOptions {
    /// Alle eingefügten Punkte bekommen `Fixed` (SVG, wie Fusion).
    pub fixed: bool,
    /// Kontrollpunktrahmen: [`crate::model::CurveData::show_frame`] an Splines mit
    /// Kontrollpunkten, Kegelschnitten und festen Splines.
    pub show_frame: bool,
    /// Welche Layer (gleicher Platz wie in `Import2d::layers`). Ein fehlender Eintrag, auch eine
    /// leere Liste, heißt „mit“.
    pub layers: Vec<bool>,
}

/// Ergebnis des Einfügens.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InsertReport {
    /// Neue Kurven der Geometrie (ohne Texte).
    pub curves: usize,
    /// Neue Punkte der Geometrie (ohne Texte).
    pub points: usize,
    pub texts: usize,
    /// Schriften, die fehlen und ersetzt wurden, jede einmal.
    pub missing_fonts: Vec<String>,
    /// Was die Datei übersprang, dazu das beim Einfügen Entartete (`degenerate`).
    pub skipped: Skipped,
    pub new_curves: Vec<CurveId>,
}

/// Ein Baustein in Skizzenkoordinaten, den es als Skizzenkurve gibt.
#[derive(Clone, Debug)]
enum Prim {
    Point(DVec2),
    Line([DVec2; 2]),
    Circle {
        c: DVec2,
        r: f64,
    },
    /// Gegen den Uhrzeigersinn von `ends[0]` nach `ends[1]`.
    Arc {
        c: DVec2,
        ends: [DVec2; 2],
    },
    /// Kleine Halbachse links der großen (wie [`CurveKind::Ellipse`]).
    Ellipse {
        c: DVec2,
        major: DVec2,
        minor: f64,
    },
    EllipticArc {
        c: DVec2,
        major: DVec2,
        minor: f64,
        ends: [DVec2; 2],
    },
    /// Quadratische Bézierkurve: Anfang, Ende, Kontrollpunkt.
    Conic {
        ends: [DVec2; 2],
        apex: DVec2,
    },
    /// Nicht rational, Grad 3 oder 5, höchstens [`MAX_SPLINE_POINTS`] Kontrollpunkte (`z = 0`).
    Control(NurbsCurve),
    /// Nicht rational mit Grad über 5; wird nach dem Hüllquader in kubische Stücke zerlegt.
    High(NurbsCurve),
    /// Stützpunkte.
    Spline(Vec<DVec2>),
    /// Fest (rational), `z = 0`.
    Fixed(NurbsCurve),
    Text(Text2d),
}

impl Prim {
    /// Anfang und Ende, die zusammengeführt werden.
    fn ends(&self) -> Option<[DVec2; 2]> {
        match self {
            Prim::Line(e)
            | Prim::Arc { ends: e, .. }
            | Prim::EllipticArc { ends: e, .. }
            | Prim::Conic { ends: e, .. } => Some(*e),
            Prim::Control(c) | Prim::Fixed(c) | Prim::High(c) => {
                let p = c.control_points();
                Some([p[0].truncate(), p[p.len() - 1].truncate()])
            }
            Prim::Spline(p) => Some([p[0], p[p.len() - 1]]),
            Prim::Point(_) | Prim::Circle { .. } | Prim::Ellipse { .. } | Prim::Text(_) => None,
        }
    }

    /// Punkte, die den Hüllquader bestimmen.
    fn bounds(&self, f: &mut dyn FnMut(DVec2)) {
        match self {
            Prim::Point(p) => f(*p),
            Prim::Line(e) => e.iter().for_each(|p| f(*p)),
            Prim::Circle { c, r } => {
                f(*c - DVec2::splat(*r));
                f(*c + DVec2::splat(*r));
            }
            Prim::Arc { c, ends } => {
                let r = ends[0].distance(*c);
                let (a0, a1) = ((ends[0] - *c).to_angle(), (ends[1] - *c).to_angle());
                let span = (a1 - a0).rem_euclid(TAU);
                ends.iter().for_each(|p| f(*p));
                for dir in [DVec2::X, DVec2::Y, -DVec2::X, -DVec2::Y] {
                    if (dir.to_angle() - a0).rem_euclid(TAU) <= span {
                        f(*c + dir * r);
                    }
                }
            }
            Prim::Ellipse { c, major, minor } => {
                let e = ellipse_eval(*c, *major, *minor);
                (0..64).for_each(|k| f(e(k as f64 * TAU / 64.0)));
            }
            Prim::EllipticArc {
                c,
                major,
                minor,
                ends,
            } => {
                let (t0, t1) = ellipse_range(*c, *major, *minor, *ends);
                let e = ellipse_eval(*c, *major, *minor);
                (0..=64).for_each(|k| f(e(t0 + (t1 - t0) * k as f64 / 64.0)));
            }
            Prim::Conic { ends, apex } => {
                f(ends[0]);
                f(ends[1]);
                f(*apex);
            }
            Prim::Control(c) | Prim::High(c) | Prim::Fixed(c) => {
                c.control_points().iter().for_each(|p| f(p.truncate()));
            }
            Prim::Spline(p) => p.iter().for_each(|q| f(*q)),
            Prim::Text(t) => f(t.pos),
        }
    }
}

/// Punkt der Ellipse `c + major·cos t + n̂·minor·sin t` (`n̂` links der großen Halbachse).
fn ellipse_eval(c: DVec2, major: DVec2, minor: f64) -> impl Fn(f64) -> DVec2 {
    let n = major.perp().normalize_or_zero() * minor;
    move |t: f64| c + major * t.cos() + n * t.sin()
}

/// Parameterbereich eines Ellipsenbogens aus seinen Enden (gegen den Uhrzeigersinn).
fn ellipse_range(c: DVec2, major: DVec2, minor: f64, ends: [DVec2; 2]) -> (f64, f64) {
    let len = major.length();
    let param = |p: DVec2| {
        let d = p - c;
        (major.perp_dot(d) / (len * minor)).atan2(d.dot(major) / (len * len))
    };
    let t0 = param(ends[0]);
    let mut span = (param(ends[1]) - t0).rem_euclid(TAU);
    if span == 0.0 {
        span = TAU;
    }
    (t0, t0 + span)
}

/// Ist der Layer `i` gewählt?
fn layer_on(layers: &[bool], i: usize) -> bool {
    layers.get(i).copied().unwrap_or(true)
}

/// Die gewählten Elemente als Bausteine in Skizzenkoordinaten; zählt Entartetes in `degenerate`.
fn place_all(
    src: &Import2d,
    place: &Placement,
    layers: &[bool],
    degenerate: &mut usize,
) -> Vec<Prim> {
    let mut out = Vec::new();
    for (i, layer) in src.layers.iter().enumerate() {
        if !layer_on(layers, i) {
            continue;
        }
        for e in &layer.elements {
            if !place_element(e, place, &mut out) {
                *degenerate += 1;
            }
        }
    }
    out
}

/// Bild einer Ellipse `c + a·cos s + b·sin s` (s von `s0` bis `s1`, `full` = ganz) mit den
/// konjugierten Halbmessern `a`, `b`, wieder mit senkrechten Achsen (wie `Aff::ellipse_from` im
/// DXF-Leser). `None`, wenn eine Halbachse verschwindet.
fn ellipse_prim(
    place: &Placement,
    c: DVec2,
    a: DVec2,
    b: DVec2,
    (s0, s1): (f64, f64),
    full: bool,
) -> Option<Prim> {
    let (a, b) = (place.lin(a), place.lin(b));
    // Parameterverschiebung t0, bei der der Punkt auf der großen Halbachse liegt.
    let t0 = 0.5 * (2.0 * a.dot(b)).atan2(a.length_squared() - b.length_squared());
    let (st, ct) = t0.sin_cos();
    let major = a * ct + b * st;
    let minor_v = b * ct - a * st;
    let len = major.length();
    let minor = minor_v.length().min(len);
    if !(len > 0.0 && minor > 0.0 && len.is_finite()) {
        return None;
    }
    let c = place.apply(c);
    if full {
        return Some(Prim::Ellipse { c, major, minor });
    }
    // Liegt die kleine Halbachse rechts der großen, läuft der Parameter rückwärts.
    let (t_a, t_b) = if major.perp_dot(minor_v) < 0.0 {
        (-(s1 - t0), -(s0 - t0))
    } else {
        (s0 - t0, s1 - t0)
    };
    let e = ellipse_eval(c, major, minor);
    Some(Prim::EllipticArc {
        c,
        major,
        minor,
        ends: [e(t_a), e(t_b)],
    })
}

/// Hängt die Bausteine des Elements `e` an; `false`, wenn es entartet ist.
fn place_element(e: &Element2d, place: &Placement, out: &mut Vec<Prim>) -> bool {
    let full_turn = |s: f64, e: f64| e - s >= TAU - 1e-12;
    match e {
        Element2d::Point(p) => out.push(Prim::Point(place.apply(*p))),
        Element2d::Line(a, b) => out.push(Prim::Line([place.apply(*a), place.apply(*b)])),
        Element2d::Circle { c, r } => return place_arc(place, *c, *r, 0.0, TAU, true, out),
        Element2d::Arc { c, r, start, end } => {
            return place_arc(place, *c, *r, *start, *end, full_turn(*start, *end), out);
        }
        Element2d::Ellipse {
            c,
            major,
            ratio,
            start,
            end,
        } => {
            let minor = major.perp() * *ratio;
            let full = *start == 0.0 && full_turn(*start, *end);
            match ellipse_prim(place, *c, *major, minor, (*start, *end), full) {
                Some(p) => out.push(p),
                None => return false,
            }
        }
        Element2d::Quad(q) => out.push(Prim::Conic {
            ends: [place.apply(q[0]), place.apply(q[2])],
            apex: place.apply(q[1]),
        }),
        Element2d::Cubic(q) => {
            let pts: Vec<DVec2> = q.iter().map(|p| place.apply(*p)).collect();
            match nurbs(3, CUBIC_KNOTS.to_vec(), &pts, None) {
                Some(c) => out.push(Prim::Control(c)),
                None => return false,
            }
        }
        Element2d::Nurbs {
            degree,
            knots,
            points,
            weights,
        } => {
            let pts: Vec<DVec2> = points.iter().map(|p| place.apply(*p)).collect();
            return place_nurbs(*degree, knots, &pts, weights.as_deref(), out);
        }
        Element2d::FitSpline(p) => {
            if p.len() < 2 {
                return false;
            }
            out.push(Prim::Spline(p.iter().map(|q| place.apply(*q)).collect()));
        }
        Element2d::Text(t) => {
            let dir = place.lin(DVec2::from_angle(t.angle));
            let up = place.lin(DVec2::from_angle(t.angle).perp());
            out.push(Prim::Text(Text2d {
                pos: place.apply(t.pos),
                angle: dir.y.atan2(dir.x),
                height: t.height * up.length(),
                ..t.clone()
            }));
        }
    }
    true
}

/// Kreis oder Bogen (`full`: ganz). Unter gleichem Maßstab bleibt es Kreis bzw. Bogen; eine
/// Spiegelung kehrt den Umlauf um, darum tauschen die Enden. Sonst wird es eine Ellipse.
fn place_arc(
    place: &Placement,
    c: DVec2,
    r: f64,
    start: f64,
    end: f64,
    full: bool,
    out: &mut Vec<Prim>,
) -> bool {
    if !(r > 0.0 && r.is_finite()) {
        return false;
    }
    let Some(s) = place.uniform() else {
        let p = ellipse_prim(place, c, DVec2::X * r, DVec2::Y * r, (start, end), full);
        return p.map(|p| out.push(p)).is_some();
    };
    if full {
        out.push(Prim::Circle {
            c: place.apply(c),
            r: r * s,
        });
        return true;
    }
    let at = |a: f64| place.apply(c + DVec2::from_angle(a) * r);
    let (a, b) = (at(start), at(end));
    out.push(Prim::Arc {
        c: place.apply(c),
        ends: if place.mirrors() { [b, a] } else { [a, b] },
    });
    true
}

/// Nicht rationale bzw. rationale NURBS-Kurve in Skizzenkoordinaten (`z = 0`); `None`, wenn die
/// Daten keine gültige Kurve ergeben.
fn nurbs(
    degree: usize,
    knots: Vec<f64>,
    points: &[DVec2],
    weights: Option<&[f64]>,
) -> Option<NurbsCurve> {
    NurbsCurve::new(
        degree,
        knots,
        points.iter().map(|p| p.extend(0.0)).collect(),
        weights.map_or_else(|| vec![1.0; points.len()], <[f64]>::to_vec),
    )
    .ok()
}

/// NURBS nach Spec B: Grad 1 eine Linie je Spanne, rational fest, Grad 2 und 4 exakt erhöht,
/// Grad 3 und 5 mit den Knoten der Datei, darüber in kubische Stücke (später, nach der Größe).
fn place_nurbs(
    degree: usize,
    knots: &[f64],
    pts: &[DVec2],
    weights: Option<&[f64]>,
    out: &mut Vec<Prim>,
) -> bool {
    let rational = weights.is_some_and(|w| w.windows(2).any(|x| x[0] != x[1]));
    let Some(curve) = nurbs(degree, knots.to_vec(), pts, weights.filter(|_| rational)) else {
        return false;
    };
    if degree == 1 {
        // Die Kurve ist das Kontrollpolygon; gleiche Nachbarn ergeben keine Linie.
        let mut any = false;
        for w in pts.windows(2).filter(|w| w[0] != w[1]) {
            out.push(Prim::Line([w[0], w[1]]));
            any = true;
        }
        return any;
    }
    if rational {
        out.push(Prim::Fixed(curve));
        return true;
    }
    let curve = match degree {
        2 => curve.elevated(3),
        4 => curve.elevated(5),
        _ => Ok(curve),
    };
    let Ok(curve) = curve else {
        return false;
    };
    if curve.degree() > 5 {
        out.push(Prim::High(curve));
        return true;
    }
    match split_for_limit(&curve) {
        Some(pieces) => out.extend(pieces.into_iter().map(Prim::Control)),
        None => return false,
    }
    true
}

/// Teilt die Kurve an inneren Knoten exakt so, dass jedes Stück höchstens
/// [`MAX_SPLINE_POINTS`] Kontrollpunkte hat. Ein Stück `[a, b]` hat `p + 1` plus die
/// Vielfachheiten der Knoten in seinem Inneren.
fn split_for_limit(curve: &NurbsCurve) -> Option<Vec<NurbsCurve>> {
    if curve.control_points().len() <= MAX_SPLINE_POINTS {
        return Some(vec![curve.clone()]);
    }
    let p = curve.degree();
    let (lo, hi) = curve.domain();
    let mut breaks = vec![lo];
    let mut inside = 0;
    for (u, m) in interior_knots(curve) {
        if p + 1 + inside + m <= MAX_SPLINE_POINTS {
            inside += m;
        } else {
            breaks.push(u);
            inside = 0;
        }
    }
    breaks.push(hi);
    breaks
        .windows(2)
        .map(|w| curve.sub_curve(w[0], w[1]).ok())
        .collect()
}

/// Die verschiedenen inneren Knoten mit ihrer Vielfachheit.
fn interior_knots(curve: &NurbsCurve) -> Vec<(f64, usize)> {
    let p = curve.degree();
    let k = curve.knots().knots();
    let mut out: Vec<(f64, usize)> = Vec::new();
    for &u in &k[p + 1..k.len() - p - 1] {
        match out.last_mut() {
            Some((v, m)) if *v == u => *m += 1,
            _ => out.push((u, 1)),
        }
    }
    out
}

/// Teilt eine geschlossene Kurve in zwei: am mittleren inneren Knoten, ohne innere Knoten in der
/// Mitte des Bereichs. Exakt.
fn split_closed(curve: &NurbsCurve) -> Option<[NurbsCurve; 2]> {
    let (lo, hi) = curve.domain();
    let inner = interior_knots(curve);
    let u = inner.get(inner.len() / 2).map_or(0.5 * (lo + hi), |k| k.0);
    Some([curve.sub_curve(lo, u).ok()?, curve.sub_curve(u, hi).ok()?])
}

/// Punkt einer Bézierkurve (de Casteljau).
fn bezier_point(cp: &[DVec2], t: f64) -> DVec2 {
    let mut w = cp.to_vec();
    for k in (1..w.len()).rev() {
        for i in 0..k {
            w[i] = w[i].lerp(w[i + 1], t);
        }
    }
    w[0]
}

/// Teilt eine Bézierkurve bei `t = ½`.
fn bezier_halves(cp: &[DVec2]) -> (Vec<DVec2>, Vec<DVec2>) {
    let mut w = cp.to_vec();
    let mut left = vec![w[0]];
    let mut right = vec![w[w.len() - 1]];
    for k in (1..w.len()).rev() {
        for i in 0..k {
            w[i] = w[i].lerp(w[i + 1], 0.5);
        }
        left.push(w[0]);
        right.push(w[k - 1]);
    }
    right.reverse();
    (left, right)
}

/// Kubische Hermite-Näherung des Bézierstücks `cp` (gleiche Enden und Endableitungen), bei
/// Bedarf halbiert, bis der Abstand bei gleichem Parameter höchstens `tol` ist. Der Abstand bei
/// gleichem Parameter ist eine obere Schranke des geometrischen Abstands.
fn cubic_pieces(cp: &[DVec2], tol: f64, depth: u32, out: &mut Vec<[DVec2; 4]>) {
    let p = (cp.len() - 1) as f64;
    let n = cp.len() - 1;
    let d0 = (cp[1] - cp[0]) * p;
    let d1 = (cp[n] - cp[n - 1]) * p;
    let cubic = [cp[0], cp[0] + d0 / 3.0, cp[n] - d1 / 3.0, cp[n]];
    let err = (1..32)
        .map(|k| {
            let t = k as f64 / 32.0;
            bezier_point(cp, t).distance(bezier_point(&cubic, t))
        })
        .fold(0.0, f64::max);
    if err <= tol || depth >= MAX_SPLIT_DEPTH {
        out.push(cubic);
    } else {
        let (l, r) = bezier_halves(cp);
        cubic_pieces(&l, tol, depth + 1, out);
        cubic_pieces(&r, tol, depth + 1, out);
    }
}

/// Rastert Endpunkte mit der Toleranz `tol`: Ein Punkt gehört zum ersten Vertreter in höchstens
/// `tol` Abstand, sonst wird er selbst einer.
struct Merger {
    tol: f64,
    cell: f64,
    grid: HashMap<(i64, i64), Vec<usize>>,
    reps: Vec<DVec2>,
}

impl Merger {
    fn new(tol: f64) -> Merger {
        Merger {
            tol,
            cell: tol.max(1e-300),
            grid: HashMap::new(),
            reps: Vec::new(),
        }
    }

    fn key(&self, p: DVec2) -> (i64, i64) {
        // `as` sättigt; sehr große Werte landen am Rand und werden dort genau verglichen.
        (
            (p.x / self.cell).floor() as i64,
            (p.y / self.cell).floor() as i64,
        )
    }

    fn add(&mut self, p: DVec2) -> usize {
        let (kx, ky) = self.key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                let k = (kx.saturating_add(dx), ky.saturating_add(dy));
                if let Some(list) = self.grid.get(&k)
                    && let Some(&i) = list.iter().find(|&&i| self.reps[i].distance(p) <= self.tol)
                {
                    return i;
                }
            }
        }
        let i = self.reps.len();
        self.reps.push(p);
        self.grid.entry((kx, ky)).or_default().push(i);
        i
    }
}

/// Die Arbeitskopie beim Einfügen mit allem, was dabei mitgezählt wird.
struct Builder<'a> {
    s: Sketch,
    opt: &'a InsertOptions,
    /// Punkte der zusammengeführten Endpunkte, angelegt beim ersten Gebrauch.
    shared: Vec<Option<PointId>>,
    reps: Vec<DVec2>,
    /// Alle neuen Punkte der Geometrie in Anlegereihenfolge.
    points: Vec<PointId>,
    /// Punkte, die `Fixed` bekommen, auch ohne `opt.fixed` (Enden fester Splines).
    must_fix: HashSet<PointId>,
    curves: Vec<CurveId>,
    /// Toleranz des Zusammenführens.
    merge: f64,
    degenerate: usize,
    texts: usize,
    /// Texte, die die Skizze ablehnte (zählen zu `skipped.texts`).
    skipped_texts: usize,
    missing_fonts: Vec<String>,
    /// Gemeinsame Punkte, die das laufende Element angelegt hat (für [`Builder::rollback`]).
    fresh_shared: Vec<usize>,
}

/// Stand der Skizze und des Bauens vor einem Element: Alles danach Angehängte lässt sich exakt
/// zurücknehmen (die Skizze hängt beim Anlegen nur an).
struct Mark {
    vars: usize,
    points: usize,
    curves: usize,
    constraints: usize,
    texts: usize,
    new_points: usize,
    new_curves: usize,
}

impl Builder<'_> {
    fn mark(&mut self) -> Mark {
        self.fresh_shared.clear();
        Mark {
            vars: self.s.vars.len(),
            points: self.s.points.len(),
            curves: self.s.curves.len(),
            constraints: self.s.constraints.len(),
            texts: self.s.texts.len(),
            new_points: self.points.len(),
            new_curves: self.curves.len(),
        }
    }

    /// Nimmt alles zurück, was seit `m` angelegt wurde (ein abgelehntes Element).
    fn rollback(&mut self, m: &Mark) {
        self.s.vars.truncate(m.vars);
        self.s.points.truncate(m.points);
        self.s.curves.truncate(m.curves);
        self.s.constraints.truncate(m.constraints);
        self.s.texts.truncate(m.texts);
        self.s.touch_structure();
        self.points.truncate(m.new_points);
        self.curves.truncate(m.new_curves);
        for i in self.fresh_shared.drain(..) {
            self.shared[i] = None;
        }
        let alive = m.points;
        self.must_fix.retain(|p| p.index() < alive);
    }

    fn point(&mut self, p: DVec2) -> Result<PointId, SketchError> {
        let id = self.s.add_point(p.x, p.y)?;
        self.points.push(id);
        Ok(id)
    }

    fn shared(&mut self, i: usize) -> Result<PointId, SketchError> {
        if let Some(p) = self.shared[i] {
            return Ok(p);
        }
        let p = self.point(self.reps[i])?;
        self.shared[i] = Some(p);
        self.fresh_shared.push(i);
        Ok(p)
    }

    fn curve(&mut self, c: CurveId, frame: bool) {
        if frame && self.opt.show_frame {
            self.s.curves[c].show_frame = true;
        }
        self.curves.push(c);
    }

    /// Spline mit Kontrollpunkten über `curve`, Anfang `a` und Ende `b` schon angelegt.
    fn control(&mut self, curve: &NurbsCurve, a: PointId, b: PointId) -> Result<(), SketchError> {
        let ctrl = curve.control_points();
        let mut ids = Vec::with_capacity(ctrl.len());
        ids.push(a);
        for q in &ctrl[1..ctrl.len() - 1] {
            ids.push(self.point(q.truncate())?);
        }
        ids.push(b);
        let degree = u8::try_from(curve.degree()).unwrap_or(3);
        let c = self
            .s
            .add_control_spline_as(&ids, degree, curve.knots().knots().to_vec())?;
        self.curve(c, true);
        Ok(())
    }

    /// Fester Spline mit den Enden auf `a` und `b` (Positionen `pa`, `pb`): Die Endkontrollpunkte
    /// werden auf die zusammengeführten Punkte gesetzt, damit die Enden genau auf der Kurve liegen.
    fn fixed(&mut self, curve: &NurbsCurve, ends: [usize; 2]) -> Result<bool, SketchError> {
        let mut ctrl = curve.control_points().to_vec();
        let n = ctrl.len();
        ctrl[0] = self.reps[ends[0]].extend(0.0);
        ctrl[n - 1] = self.reps[ends[1]].extend(0.0);
        let Ok(curve) = NurbsCurve::new(
            curve.degree(),
            curve.knots().knots().to_vec(),
            ctrl,
            curve.weights().to_vec(),
        ) else {
            return Ok(false);
        };
        let a = self.shared(ends[0])?;
        let b = self.shared(ends[1])?;
        self.must_fix.extend([a, b]);
        let c = self.s.push_curve(CurveKind::FixedSpline {
            start: a,
            end: b,
            curve: Arc::new(curve),
        });
        self.curve(c, true);
        Ok(true)
    }

    /// Legt einen Baustein an; `false`, wenn er entartet ist.
    fn prim(&mut self, prim: &Prim, ends: Option<[usize; 2]>) -> Result<bool, SketchError> {
        let lin = self.s.tol.linear;
        // Zusammengefallene Enden einer Kurve, die nicht geschlossen sein kann.
        let closed = ends.is_some_and(|e| e[0] == e[1]);
        let at = |b: &Self, i: usize| ends.map(|e| b.reps[e[i]]).unwrap_or_default();
        match prim {
            Prim::Point(p) => {
                self.point(*p)?;
            }
            Prim::Line(_) => {
                let e = ends.unwrap_or_default();
                if closed || at(self, 0).distance(at(self, 1)) <= lin {
                    return Ok(false);
                }
                let (a, b) = (self.shared(e[0])?, self.shared(e[1])?);
                let c = self.s.add_line(a, b)?;
                self.curve(c, false);
            }
            Prim::Circle { c, r } => {
                if *r <= lin {
                    return Ok(false);
                }
                let m = self.point(*c)?;
                let id = self.s.add_circle(m, *r)?;
                self.curve(id, false);
            }
            Prim::Arc { c, .. } => {
                let (s0, s1) = (at(self, 0), at(self, 1));
                if closed || s0.distance(s1) <= lin {
                    return Ok(false);
                }
                // Mitte auf die Mittelsenkrechte der (zusammengeführten) Enden: Der Bogen ist
                // dann genau, `ArcRadius` gilt ohne Lösen, auch wenn alles fest ist.
                let mid = (s0 + s1) * 0.5;
                let n = (s1 - s0).perp().normalize();
                let center = mid + n * (*c - mid).dot(n);
                if center.distance(s0) <= lin {
                    return Ok(false);
                }
                let e = ends.unwrap_or_default();
                let (a, b) = (self.shared(e[0])?, self.shared(e[1])?);
                let m = self.point(center)?;
                let id = self.s.add_arc(m, a, b)?;
                self.curve(id, false);
            }
            Prim::Ellipse { c, major, minor } => {
                if major.length() <= lin || *minor <= lin {
                    return Ok(false);
                }
                let m = self.point(*c)?;
                let id = self.s.add_ellipse(m, *major, *minor)?;
                self.curve(id, false);
            }
            Prim::EllipticArc {
                c, major, minor, ..
            } => {
                let (s0, s1) = (at(self, 0), at(self, 1));
                if closed
                    || major.length() <= lin
                    || *minor <= lin
                    || s0.distance(*c) <= lin
                    || s1.distance(*c) <= lin
                {
                    return Ok(false);
                }
                let e = ends.unwrap_or_default();
                let (a, b) = (self.shared(e[0])?, self.shared(e[1])?);
                let m = self.point(*c)?;
                let id = self.s.add_elliptic_arc(m, *major, *minor, a, b)?;
                self.curve(id, false);
            }
            Prim::Conic { apex, .. } => {
                if closed {
                    return Ok(false);
                }
                let e = ends.unwrap_or_default();
                let (a, b) = (self.shared(e[0])?, self.shared(e[1])?);
                let q = self.point(*apex)?;
                let id = self.s.add_conic_as(a, b, q, QUAD_RHO)?;
                self.curve(id, true);
            }
            Prim::Control(curve) => {
                let e = ends.unwrap_or_default();
                let a = self.shared(e[0])?;
                if !closed {
                    let b = self.shared(e[1])?;
                    self.control(curve, a, b)?;
                    return Ok(true);
                }
                // Geschlossen: Anfang und Ende wären derselbe Punkt, das nimmt der Spline nicht.
                // In zwei Stücke teilen, beide Stöße als Punkte.
                let mut ctrl = curve.control_points().to_vec();
                let n = ctrl.len();
                ctrl[0] = self.reps[e[0]].extend(0.0);
                ctrl[n - 1] = ctrl[0];
                let whole = NurbsCurve::new(
                    curve.degree(),
                    curve.knots().knots().to_vec(),
                    ctrl,
                    curve.weights().to_vec(),
                )
                .ok();
                let Some([first, second]) = whole.as_ref().and_then(split_closed) else {
                    return Ok(false);
                };
                let cp = first.control_points();
                let joint = cp[cp.len() - 1].truncate();
                if joint.distance(self.reps[e[0]]) <= lin {
                    return Ok(false);
                }
                let j = self.point(joint)?;
                self.control(&first, a, j)?;
                self.control(&second, j, a)?;
            }
            Prim::High(_) => unreachable!("split before building"),
            Prim::Spline(pts) => return self.spline(pts, ends.unwrap_or_default()),
            Prim::Fixed(curve) => return self.fixed(curve, ends.unwrap_or_default()),
            Prim::Text(_) => unreachable!("texts go through `text`"),
        }
        Ok(true)
    }

    /// Spline durch Stützpunkte (Grenzen von [`CurveKind::Spline`]: 2 bis
    /// [`MAX_SPLINE_POINTS`] Punkte, keine doppelten, Nachbarn weiter als `tol.linear`
    /// auseinander, offen). Geschlossen oder mit mehr Punkten wird es ein fester Spline über
    /// dieselbe natürliche kubische Kurve (Parameter `t = i`).
    fn spline(&mut self, pts: &[DVec2], ends: [usize; 2]) -> Result<bool, SketchError> {
        let gap = self.s.tol.linear.max(self.merge);
        let mut fit: Vec<DVec2> = vec![self.reps[ends[0]]];
        for &p in &pts[1..pts.len() - 1] {
            if p.distance(fit[fit.len() - 1]) > gap {
                fit.push(p);
            }
        }
        let last = self.reps[ends[1]];
        let closed = ends[0] == ends[1];
        // Ein innerer Punkt zu nah am Ende fällt weg (das Ende bleibt).
        while fit.len() > 1 && fit[fit.len() - 1].distance(last) <= gap {
            fit.pop();
        }
        fit.push(last);
        if fit.len() < 2 || (closed && fit.len() < 3) {
            return Ok(false);
        }
        // Der Spline verlangt Nachbarn weiter als `tol.linear` auseinander; zwei fast gleiche
        // Enden (kürzer als die Toleranz, aber nicht zusammengeführt) sind entartet.
        let lin = self.s.tol.linear;
        if fit.windows(2).any(|w| w[0].distance(w[1]) <= lin) {
            return Ok(false);
        }
        if !closed && fit.len() <= MAX_SPLINE_POINTS {
            let a = self.shared(ends[0])?;
            let mut ids = vec![a];
            for &p in &fit[1..fit.len() - 1] {
                ids.push(self.point(p)?);
            }
            ids.push(self.shared(ends[1])?);
            let c = self.s.add_spline(&ids)?;
            self.curve(c, false);
            return Ok(true);
        }
        let params: Vec<f64> = (0..fit.len()).map(|i| i as f64).collect();
        let pts3: Vec<_> = fit.iter().map(|p| p.extend(0.0)).collect();
        match NurbsCurve::natural_cubic(&pts3, &params) {
            Ok(curve) => self.fixed(&curve, ends),
            Err(_) => Ok(false),
        }
    }

    /// Text aus DXF als Text im Rahmen (Skizzen 8): Der Rahmen umschließt den gesetzten Block, so
    /// steht der Ausrichtepunkt `pos` dort, wo DXF ihn meint (links/Mitte/rechts, Grundlinie/Mitte/
    /// oben). Gedrehter Text dreht den Rahmen um seine Ecke 0.
    fn text(&mut self, t: &Text2d, shaper: &dyn TextShaper) -> Result<(), SketchError> {
        if !(t.height.is_finite() && t.height > 0.0) {
            return Err(SketchError::InvalidInput(tr!("sketch-text-height")));
        }
        let style = TextStyle {
            font: t.font.clone(),
            bold: false,
            italic: false,
            spacing: 0.0,
        };
        let shape = shaper.shape(&t.text, &style)?;
        let h = t.height;
        let width = shape.lines.iter().map(|l| l.width).fold(0.0, f64::max) * h;
        let below = shape.lines.len().saturating_sub(1) as f64 * shape.line_height * h;
        let top = shape.ascender * h;
        let bottom = shape.descender * h - below;
        let total = top - bottom;
        let (x0, ha) = match t.halign {
            HAlign2d::Left => (0.0, HAlign::Left),
            HAlign2d::Center => (-0.5 * width, HAlign::Center),
            HAlign2d::Right => (-width, HAlign::Right),
        };
        let (y0, va) = match t.valign {
            VAlign2d::Bottom => (bottom, VAlign::Bottom),
            VAlign2d::Middle => (-0.5 * total, VAlign::Middle),
            VAlign2d::Top => (-total, VAlign::Top),
        };
        let dir = DVec2::from_angle(t.angle);
        let c0 = t.pos + dir.rotate(DVec2::new(x0, y0));
        let substituted = shape.substituted.clone();
        let params = TextParams {
            content: t.text.clone(),
            style,
            height: h,
            flip_h: false,
            flip_v: false,
        };
        let id = self.s.add_text_box(
            c0,
            c0 + DVec2::new(width, total),
            params,
            ha,
            va,
            Arc::new(shape),
        )?;
        self.s.set_text_angle(id, t.angle)?;
        if let Some(name) = substituted
            && !self.missing_fonts.contains(&name)
        {
            self.missing_fonts.push(name);
        }
        self.texts += 1;
        Ok(())
    }
}

/// Liegen alle Bausteine im Rechenbereich der Skizze (sonst bricht das ganze Einfügen ab)?
fn in_range(prims: &[Prim]) -> bool {
    prims.iter().all(|p| {
        let mut ok = true;
        p.bounds(&mut |q| ok &= within_coord_range(q));
        ok
    })
}

/// Diagonale des Hüllquaders der Bausteine (0 ohne Bausteine).
fn diagonal(prims: &[Prim]) -> f64 {
    let mut b: Option<(DVec2, DVec2)> = None;
    for p in prims {
        p.bounds(&mut |q| {
            b = Some(match b {
                None => (q, q),
                Some((lo, hi)) => (lo.min(q), hi.max(q)),
            });
        });
    }
    b.map_or(0.0, |(lo, hi)| lo.distance(hi))
}

/// Ersetzt Grad über 5 durch kubische Stücke mit höchstens `tol` Abweichung.
fn expand_high(prims: Vec<Prim>, tol: f64, degenerate: &mut usize) -> Vec<Prim> {
    let mut out = Vec::with_capacity(prims.len());
    for p in prims {
        let Prim::High(curve) = p else {
            out.push(p);
            continue;
        };
        let mut pieces = Vec::new();
        for seg in curve.bezier_segments() {
            let cp: Vec<DVec2> = seg.control.iter().map(|q| q.truncate()).collect();
            cubic_pieces(&cp, tol, 0, &mut pieces);
        }
        let before = out.len();
        for q in pieces {
            if let Some(c) = nurbs(3, CUBIC_KNOTS.to_vec(), &q, None) {
                out.push(Prim::Control(c));
            }
        }
        if out.len() == before {
            *degenerate += 1;
        }
    }
    out
}

impl Sketch {
    /// Fügt die Geometrie `src` (nur die Layer aus `opt.layers`) mit der Lage `place` in einem
    /// Zug ein (Spec B). `shaper` setzt die Texte; fehlt eine Schrift, setzt er mit der
    /// Ersatzschrift, und ihr Name steht in `missing_fonts`. Danach wird einmal gelöst. Bei einem
    /// Fehler bleibt die Skizze unverändert.
    pub fn insert_geometry(
        &mut self,
        src: &Import2d,
        place: &Placement,
        opt: &InsertOptions,
        shaper: &dyn TextShaper,
    ) -> Result<InsertReport, SketchError> {
        if !place.valid() {
            return Err(SketchError::InvalidInput(tr!("sketch-import-placement")));
        }
        let mut degenerate = 0;
        let prims = place_all(src, place, &opt.layers, &mut degenerate);
        if !in_range(&prims) {
            return Err(SketchError::InvalidInput(tr!("sketch-import-out-of-range")));
        }
        let diag = diagonal(&prims);
        let prims = expand_high(prims, (SPLIT_REL * diag).max(1e-12), &mut degenerate);

        // Endpunkte zusammenführen, nur innerhalb dieser Einfügung.
        let mut merger = Merger::new(MERGE_REL * diag);
        let ends: Vec<Option<[usize; 2]>> = prims
            .iter()
            .map(|p| p.ends().map(|e| [merger.add(e[0]), merger.add(e[1])]))
            .collect();

        let mut b = Builder {
            s: self.clone(),
            opt,
            shared: vec![None; merger.reps.len()],
            reps: merger.reps,
            points: Vec::new(),
            must_fix: HashSet::new(),
            curves: Vec::new(),
            merge: merger.tol,
            degenerate,
            texts: 0,
            skipped_texts: 0,
            missing_fonts: Vec::new(),
            fresh_shared: Vec::new(),
        };
        // Ein Element, das die Skizze ablehnt, wird zurückgenommen und gezählt; die anderen
        // kommen trotzdem (ein einzelner zu langer Text bricht nicht das ganze Einfügen ab).
        for (prim, e) in prims.iter().zip(&ends) {
            let mark = b.mark();
            if let Prim::Text(t) = prim {
                if b.text(t, shaper).is_err() {
                    b.rollback(&mark);
                    b.skipped_texts += 1;
                }
            } else if !matches!(b.prim(prim, *e), Ok(true)) {
                b.rollback(&mark);
                b.degenerate += 1;
            }
        }
        let fix: Vec<PointId> = if opt.fixed {
            b.points.clone()
        } else {
            b.points
                .iter()
                .copied()
                .filter(|p| b.must_fix.contains(p))
                .collect()
        };
        for p in fix {
            let at = b.s.pos(p);
            b.s.add_constraint(ConstraintKind::Fixed(p, at.x, at.y))?;
        }
        b.s.solve();
        let mut skipped = src.skipped;
        skipped.degenerate += b.degenerate;
        skipped.texts += b.skipped_texts;
        let report = InsertReport {
            curves: b.curves.len(),
            points: b.points.len(),
            texts: b.texts,
            missing_fonts: b.missing_fonts,
            skipped,
            new_curves: b.curves,
        };
        *self = b.s;
        Ok(report)
    }
}

/// Linienzüge für die Vorschau der Dialoge, ohne Skizze: je Kurve ein Zug in Skizzenkoordinaten,
/// höchstens etwa `tol` neben der Kurve. Punkte und Texte fehlen.
pub fn preview_polylines(
    src: &Import2d,
    place: &Placement,
    layers: &[bool],
    tol: f64,
) -> Vec<Vec<DVec2>> {
    if !place.valid() {
        return Vec::new();
    }
    let tol = if tol.is_finite() && tol > 0.0 {
        tol
    } else {
        1e-3
    };
    let mut ignored = 0;
    let prims = place_all(src, place, layers, &mut ignored);
    prims.iter().filter_map(|p| sample_prim(p, tol)).collect()
}

/// Zahl der Abschnitte für einen Bogen vom Radius `r` über `span` mit höchstens `tol` Abweichung.
fn arc_steps(r: f64, span: f64, tol: f64) -> usize {
    let step = if tol < r {
        2.0 * (1.0 - tol / r).acos()
    } else {
        span
    };
    ((span / step.max(1e-6)).ceil() as usize).clamp(4, 1024)
}

/// Zahl der Abschnitte einer Freiformkurve nach der Länge ihres Kontrollpolygons.
fn free_steps(ctrl_len: f64, tol: f64) -> usize {
    ((ctrl_len / tol).sqrt().ceil() as usize).clamp(8, 1024)
}

fn polygon_len(p: impl Iterator<Item = DVec2>) -> f64 {
    let pts: Vec<DVec2> = p.collect();
    pts.windows(2).map(|w| w[0].distance(w[1])).sum()
}

fn sample_curve(c: &NurbsCurve, tol: f64) -> Vec<DVec2> {
    let n = free_steps(
        polygon_len(c.control_points().iter().map(|q| q.truncate())),
        tol,
    );
    let (a, b) = c.domain();
    (0..=n)
        .map(|i| c.point(a + (b - a) * i as f64 / n as f64).truncate())
        .collect()
}

fn sample_prim(p: &Prim, tol: f64) -> Option<Vec<DVec2>> {
    Some(match p {
        Prim::Point(_) | Prim::Text(_) => return None,
        Prim::Line(e) => e.to_vec(),
        Prim::Circle { c, r } => {
            let n = arc_steps(*r, TAU, tol);
            (0..=n)
                .map(|i| *c + DVec2::from_angle(TAU * (i % n) as f64 / n as f64) * *r)
                .collect()
        }
        Prim::Arc { c, ends } => {
            let r = ends[0].distance(*c);
            let a0 = (ends[0] - *c).to_angle();
            let mut span = ((ends[1] - *c).to_angle() - a0).rem_euclid(TAU);
            if span == 0.0 {
                span = TAU;
            }
            let n = arc_steps(r, span, tol);
            let mut out: Vec<DVec2> = (0..n)
                .map(|i| *c + DVec2::from_angle(a0 + span * i as f64 / n as f64) * r)
                .collect();
            // Die Enden genau.
            out[0] = ends[0];
            out.push(ends[1]);
            out
        }
        Prim::Ellipse { c, major, minor } => {
            let n = arc_steps(major.length(), TAU, tol);
            let e = ellipse_eval(*c, *major, *minor);
            (0..=n)
                .map(|i| e(TAU * (i % n) as f64 / n as f64))
                .collect()
        }
        Prim::EllipticArc {
            c,
            major,
            minor,
            ends,
        } => {
            let (t0, t1) = ellipse_range(*c, *major, *minor, *ends);
            let n = arc_steps(major.length(), t1 - t0, tol);
            let e = ellipse_eval(*c, *major, *minor);
            (0..=n)
                .map(|i| e(t0 + (t1 - t0) * i as f64 / n as f64))
                .collect()
        }
        Prim::Conic { ends, apex } => {
            let cp = [ends[0], *apex, ends[1]];
            let n = free_steps(polygon_len(cp.into_iter()), tol);
            (0..=n)
                .map(|i| bezier_point(&cp, i as f64 / n as f64))
                .collect()
        }
        Prim::Control(c) | Prim::High(c) | Prim::Fixed(c) => sample_curve(c, tol),
        Prim::Spline(pts) => {
            let params: Vec<f64> = (0..pts.len()).map(|i| i as f64).collect();
            let pts3: Vec<_> = pts.iter().map(|p| p.extend(0.0)).collect();
            match NurbsCurve::natural_cubic(&pts3, &params) {
                Ok(c) => sample_curve(&c, tol),
                Err(_) => pts.clone(),
            }
        }
    })
}
