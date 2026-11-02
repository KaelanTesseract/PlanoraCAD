//! Geometrie von Rundung und Fase: reine Pläne, die Skizze wird nur gelesen. Vorschau und Klick
//! rechnen mit denselben Funktionen; wer anlegt, bekommt den Plan und setzt ihn um.
//!
//! **Rundung** (`fillet_plan`): Linie, Bogen und Kreis rechnet die Funktion exakt (Versatzkurven
//! im Abstand ±r schneiden), Ellipse, Ellipsenbogen und Spline numerisch (`exact == false`).
//! Von allen Kandidaten zählen nur die, deren Berührpunkte auf dem behaltenen Stück liegen,
//! das ist das Stück von der Ecke zum fernen Ende der Kurve, auf dem geklickt wurde. Davon
//! gewinnt der mit der kleinsten Summe `|T1 − Klick1| + |T2 − Klick2|`, bei Gleichstand der mit
//! dem kleinsten Abstand Mitte–Ecke. Linien und Bögen, die sich nur in der Verlängerung treffen,
//! werden bis zum Berührpunkt verlängert (ihr nahes Ende wandert).
//!
//! **Fase** (`chamfer_plan`): nur Linien, drei Arten.
//!
//! **Anlegen** (`add_fillet`, `add_chamfer`): atomar, die Punkte kommen bitgleich aus dem Plan.
//! Die Enden der Vorlage wandern auf die Berührpunkte wie beim Stutzen; es bleibt, was danach
//! ohne Bewegung erfüllt ist. Reihenfolge immer: Kurven, Tangenten, Ecke, Maß.

use crate::constraint::{ConstraintId, ConstraintKind};
use crate::error::SketchError;
use crate::geom::Geom;
use crate::intersect::intersect;
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use crate::shapes::ArcPlan;
use planora_i18n::tr;
use planora_math::{DVec2, PI, TAU};

/// Sinus des Winkels, unter dem zwei Richtungen als parallel (oder tangential) gelten.
const PARALLEL: f64 = 1e-7;
/// Abschnitte je Lauf der numerischen Suche nach Berührpunkten (Splines: je Segment 64).
const SCAN_STEPS: usize = 512;
const SCAN_STEPS_PER_SEGMENT: usize = 64;
/// Halbierungsschritte beim Nachschärfen eines Vorzeichenwechsels.
const BISECT_STEPS: usize = 80;

/// Gewählte Kurve samt Klickstelle. Der Klick wählt das Stück, das bleibt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurvePick {
    pub curve: CurveId,
    pub at: DVec2,
}

/// Was mit der Kurve an der Rundung oder Fase geschieht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EndMove {
    /// Das Ende wandert auf den Berührpunkt (Linie, Bogen, Ellipsenbogen).
    Replace(PointId),
    /// Die Kurve bleibt, wie sie ist (Kreis, Ellipse, Spline, Ende mit `Fixed`).
    Whole,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FilletPlan {
    pub picks: [CurvePick; 2],
    /// Der Rundungsbogen, gegen den Uhrzeigersinn.
    pub arc: ArcPlan,
    /// Berührpunkte; `touch[i]` gehört zu `picks[i]`.
    pub touch: [DVec2; 2],
    pub moves: [EndMove; 2],
    /// Gemeinsamer Endpunkt beider Kurven (die alte Ecke), falls es einen gibt.
    pub corner: Option<PointId>,
    /// Die Ecke als Lage: gemeinsamer Punkt oder Schnitt, sonst die Mitte der Klicks.
    pub corner_at: DVec2,
    pub radius: f64,
    /// `false`, wenn eine Kurve Ellipse, Ellipsenbogen oder Spline ist (numerisch gerechnet).
    pub exact: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChamferKind {
    /// Gleicher Abstand von der Ecke auf beiden Linien.
    Equal(f64),
    /// Abstand `d` auf der ersten Linie und Winkel `θ` bei `T1` zwischen `T1→X` und `T1→T2`.
    DistanceAngle(f64, f64),
    /// Zwei Abstände, der erste auf der ersten Linie.
    TwoDistances(f64, f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChamferPlan {
    pub picks: [CurvePick; 2],
    pub touch: [DVec2; 2],
    /// Die Enden, die auf die Berührpunkte wandern.
    pub moves: [PointId; 2],
    pub corner: Option<PointId>,
    pub corner_at: DVec2,
    pub kind: ChamferKind,
}

/// Was [`Sketch::add_fillet`] angelegt hat.
#[derive(Clone, Debug, PartialEq)]
pub struct FilletIds {
    pub arc: CurveId,
    pub center: PointId,
    /// Berührpunkte; `touch[i]` gehört zu `picks[i]` des Plans.
    pub touch: [PointId; 2],
    /// Die alte Ecke, wenn sie bleibt.
    pub corner: Option<PointId>,
    /// Die Konstruktionslinie an einer Kurve, die ganz bleibt. Bleiben beide ganz, gibt es zwei
    /// Linien, hier steht die der ersten.
    pub helper: Option<CurveId>,
    /// Das Radiusmaß; `None` bei einer weiteren Rundung mit `Equal`.
    pub radius_dim: Option<ConstraintId>,
}

/// Was [`Sketch::add_chamfer`] angelegt hat.
#[derive(Clone, Debug, PartialEq)]
pub struct ChamferIds {
    pub line: CurveId,
    /// Enden der Fase; `touch[i]` gehört zu `picks[i]` des Plans.
    pub touch: [PointId; 2],
    pub corner: PointId,
    /// Konstruktionslinien Ecke–Ende (nur bei gleichem Abstand).
    pub helpers: Option<[CurveId; 2]>,
    /// Die Maße in der Reihenfolge der Art.
    pub dims: Vec<ConstraintId>,
}

// ---------------------------------------------------------------------------------------------
// Geometrie
// ---------------------------------------------------------------------------------------------

/// Gerade oder Kreis (Versatzkurven und Kurven selbst).
#[derive(Clone, Copy, Debug)]
enum Shape {
    Line { a: DVec2, d: DVec2 },
    Round { c: DVec2, r: f64 },
}

fn shape(g: &Geom) -> Option<Shape> {
    match g {
        Geom::Line { a, b } => Some(Shape::Line {
            a: *a,
            d: (*b - *a).try_normalize()?,
        }),
        Geom::Circle { c, r } | Geom::Arc { c, r, .. } => Some(Shape::Round { c: *c, r: *r }),
        _ => None,
    }
}

/// Versatzkurve im Abstand `sign · r`: Gerade parallel verschoben, Kreis mit Radius `R + sign·r`.
/// Ein Kreis, der den Kreis umschließt (negativer Radius), zählt nicht.
fn offset_shape(s: Shape, sign: f64, r: f64, tol: f64) -> Option<Shape> {
    match s {
        Shape::Line { a, d } => Some(Shape::Line {
            a: a + d.perp() * (sign * r),
            d,
        }),
        Shape::Round { c, r: big } => {
            let rr = big + sign * r;
            (rr > tol).then_some(Shape::Round { c, r: rr })
        }
    }
}

/// Schnittpunkte zweier Geraden/Kreise (unbegrenzt, Kreise ganz).
fn meet(a: Shape, b: Shape) -> Vec<DVec2> {
    match (a, b) {
        (Shape::Line { a: p, d }, Shape::Line { a: q, d: e }) => {
            let den = d.perp_dot(e);
            if den.abs() < PARALLEL {
                return Vec::new();
            }
            let t = (q - p).perp_dot(e) / den;
            vec![p + d * t]
        }
        (Shape::Line { a, d }, Shape::Round { c, r })
        | (Shape::Round { c, r }, Shape::Line { a, d }) => {
            let foot = a + d * (c - a).dot(d);
            let h = c.distance(foot);
            let eps = 1e-9 * (1.0 + r);
            if h > r + eps {
                return Vec::new();
            }
            let k = (r * r - h * h).max(0.0).sqrt();
            vec![foot + d * k, foot - d * k]
        }
        (Shape::Round { c: c1, r: r1 }, Shape::Round { c: c2, r: r2 }) => {
            let dist = c1.distance(c2);
            let eps = 1e-9 * (1.0 + r1 + r2);
            if dist <= 1e-12 || dist > r1 + r2 + eps || dist < (r1 - r2).abs() - eps {
                return Vec::new();
            }
            let u = (c2 - c1) / dist;
            let x = (dist * dist + r1 * r1 - r2 * r2) / (2.0 * dist);
            let h = (r1 * r1 - x * x).max(0.0).sqrt();
            let base = c1 + u * x;
            vec![base + u.perp() * h, base - u.perp() * h]
        }
    }
}

/// Fußpunkt von `p` auf die Kurve; Geraden und Kreise unbegrenzt, alles andere im Bereich.
fn foot(g: &Geom, p: DVec2) -> DVec2 {
    match g {
        Geom::Line { a, b } => {
            let d = *b - *a;
            *a + d * ((p - *a).dot(d) / d.length_squared())
        }
        Geom::Circle { c, r } | Geom::Arc { c, r, .. } => {
            let v = p - *c;
            match v.try_normalize() {
                Some(u) => *c + u * *r,
                None => *c + DVec2::new(*r, 0.0),
            }
        }
        _ => g.eval(g.nearest(p)).0,
    }
}

/// Abstand von `p` zur Kurve im Bereich (wie `Geom::nearest`).
fn dist_to_curve(g: &Geom, p: DVec2) -> f64 {
    g.eval(g.nearest(p)).0.distance(p)
}

/// Einheitstangente der Kurve bei `x` (am Fußpunkt).
fn tangent_at(g: &Geom, x: DVec2) -> Option<DVec2> {
    match g {
        Geom::Line { a, b } => (*b - *a).try_normalize(),
        Geom::Circle { c, .. } | Geom::Arc { c, .. } => (x - *c).try_normalize().map(DVec2::perp),
        _ => g.eval(g.nearest(x)).1.try_normalize(),
    }
}

/// Koordinate `u` eines Punkts auf offenen Kurven: Anfang bei 0, Ende bei [`total_of`]; bei
/// Bögen ist `u` ein Winkel und nur modulo 2π bestimmt. Für den Ellipsenbogen zählt der Punkt
/// auf der ganzen Ellipse (er darf hinter den Enden liegen).
fn coord(g: &Geom, p: DVec2) -> Option<f64> {
    match g {
        Geom::Line { a, b } => Some((p - *a).dot((*b - *a).try_normalize()?)),
        Geom::Arc { c, a0, .. } => Some((p - *c).to_angle() - *a0),
        Geom::EllipticArc { c, a, b, t0, .. } => {
            let v = p - *c;
            let cos = v.dot(*a) / a.length_squared();
            let sin = v.dot(*b) / b.length_squared();
            Some(sin.atan2(cos) - *t0)
        }
        _ => None,
    }
}

/// Länge (Linie) bzw. Öffnungswinkel (Bogen, Ellipsenbogen).
fn total_of(g: &Geom) -> Option<f64> {
    match g {
        Geom::Line { a, b } => Some(a.distance(*b)),
        Geom::Arc { sweep, .. } | Geom::EllipticArc { sweep, .. } => Some(*sweep),
        _ => None,
    }
}

fn wrap_pi(x: f64) -> f64 {
    let w = x.rem_euclid(TAU);
    if w > PI { w - TAU } else { w }
}

/// Eine gewählte Kurve mit allem, was die Pläne brauchen.
struct Side {
    pick: CurvePick,
    g: Geom,
    /// Anfang und Ende offener Kurven (Linie, Bogen, Ellipsenbogen).
    ends: Option<[PointId; 2]>,
    /// `Fixed` an Anfang und Ende.
    fixed: [bool; 2],
    /// Linie, Kreis oder Bogen (analytisch gerechnet).
    exact: bool,
}

/// Das behaltene Stück einer offenen Kurve: von der Ecke (Koordinate `ux`) in Richtung `dir`
/// bis zum fernen Ende `far`. `e` ist der Weg von der Ecke aus in dieser Richtung.
#[derive(Clone, Copy, Debug)]
struct Piece {
    ux: f64,
    dir: f64,
    /// Weg von der Ecke bis zum fernen Ende.
    e_max: f64,
    /// Ohne Ecke (Kurven schneiden sich nicht): hinter dem nahen Ende geht es beliebig weiter.
    lower_open: bool,
    period: bool,
    tol_e: f64,
    far: usize,
    total: f64,
}

impl Piece {
    fn contains(&self, u: f64) -> bool {
        let mut e = self.dir * (u - self.ux);
        if self.period {
            e = e.rem_euclid(TAU);
            if self.lower_open {
                // Hinter dem nahen Ende (e > total) gilt die Hälfte der Lücke als Verlängerung.
                if e > self.total + (TAU - self.total) / 2.0 {
                    e -= TAU;
                }
            } else if e > TAU - self.tol_e {
                e -= TAU;
            }
        }
        (self.lower_open || e >= -self.tol_e) && e < self.e_max - self.tol_e
    }
}

struct Corner {
    /// Gemeinsamer Punkt oder Schnittstelle; `None`, wenn sich die Kurven nicht treffen.
    x: Option<DVec2>,
    shared: Option<PointId>,
    at: DVec2,
}

struct Cand {
    m: DVec2,
    t: [DVec2; 2],
}

impl Sketch {
    fn fillet_side(&self, pick: CurvePick) -> Option<Side> {
        if !pick.at.is_finite() {
            return None;
        }
        let g = self.geom(pick.curve)?;
        let ends = match &self.curve(pick.curve)?.kind {
            CurveKind::Line { a, b } => Some([*a, *b]),
            CurveKind::Arc { start, end, .. } | CurveKind::EllipticArc { start, end, .. } => {
                Some([*start, *end])
            }
            _ => None,
        };
        let is_fixed = |p: PointId| {
            self.constraints().any(|(_, d)| {
                d.enabled && !d.driven && matches!(d.kind, ConstraintKind::Fixed(q, ..) if q == p)
            })
        };
        let fixed = ends.map_or([false; 2], |e| [is_fixed(e[0]), is_fixed(e[1])]);
        let exact = matches!(
            g,
            Geom::Line { .. } | Geom::Circle { .. } | Geom::Arc { .. }
        );
        Some(Side {
            pick,
            g,
            ends,
            fixed,
            exact,
        })
    }

    /// Das behaltene Stück von `side`; `None` für Kurven ohne Enden (sie bleiben ganz).
    fn fillet_piece(&self, side: &Side, x: Option<DVec2>, other: &Geom) -> Option<Piece> {
        let ends = side.ends?;
        let g = &side.g;
        let total = total_of(g)?;
        let period = !matches!(g, Geom::Line { .. });
        let unit = match g {
            Geom::Arc { r, .. } => *r,
            Geom::EllipticArc { a, .. } => a.length(),
            _ => 1.0,
        };
        let tol_e = self.tolerance().linear / unit;
        match x {
            Some(x) => {
                let ux = coord(g, x)?;
                let uc = coord(g, side.pick.at)?;
                let mut delta = uc - ux;
                if period {
                    delta = wrap_pi(delta);
                }
                let dir = if delta.abs() > tol_e {
                    delta.signum()
                } else {
                    // Klick auf der Ecke: die längere Seite bleibt
                    let u = if period { ux.rem_euclid(TAU) } else { ux };
                    if total - u > u { 1.0 } else { -1.0 }
                };
                let far = usize::from(dir > 0.0);
                let u_far = if far == 1 { total } else { 0.0 };
                let mut e_max = dir * (u_far - ux);
                if period {
                    e_max = e_max.rem_euclid(TAU);
                }
                Some(Piece {
                    ux,
                    dir,
                    e_max,
                    lower_open: false,
                    period,
                    tol_e,
                    far,
                    total,
                })
            }
            None => {
                let d = |p: PointId| {
                    let q = self.pos(p);
                    q.distance(foot(other, q))
                };
                let near = usize::from(d(ends[0]) > d(ends[1]));
                let far = 1 - near;
                Some(Piece {
                    ux: if near == 0 { 0.0 } else { total },
                    dir: if far == 1 { 1.0 } else { -1.0 },
                    e_max: total,
                    lower_open: true,
                    period,
                    tol_e,
                    far,
                    total,
                })
            }
        }
    }

    /// Ecke der beiden Kurven; `None`, wenn es keine sinnvolle gibt (parallel, tangential).
    fn fillet_corner_of(&self, a: &Side, b: &Side) -> Option<Corner> {
        let tol = self.tolerance().linear;
        let (ca, cb) = (a.pick.at, b.pick.at);
        let by_clicks = |p: &DVec2| p.distance(ca) + p.distance(cb);
        let mut shared: Option<(PointId, DVec2)> = None;
        if let (Some(ea), Some(eb)) = (a.ends, b.ends) {
            for p in ea {
                if eb.contains(&p) {
                    let at = self.pos(p);
                    if shared.is_none_or(|(_, best)| by_clicks(&at) < by_clicks(&best)) {
                        shared = Some((p, at));
                    }
                }
            }
        }
        let x = if let Some((_, at)) = shared {
            Some(at)
        } else if a.exact && b.exact {
            let (sa, sb) = (shape(&a.g)?, shape(&b.g)?);
            if let (Shape::Line { d, .. }, Shape::Line { d: e, .. }) = (sa, sb)
                && d.perp_dot(e).abs() < PARALLEL
            {
                return None;
            }
            // Treffen auf den Kurven selbst vor Treffen in der Verlängerung
            let ext = |p: DVec2| dist_to_curve(&a.g, p) + dist_to_curve(&b.g, p);
            meet(sa, sb)
                .into_iter()
                .filter(|p| p.is_finite())
                .min_by(|p, q| {
                    let (ep, eq) = (ext(*p), ext(*q));
                    if (ep - eq).abs() > 1e-6 {
                        ep.total_cmp(&eq)
                    } else {
                        by_clicks(p).total_cmp(&by_clicks(q))
                    }
                })
        } else {
            intersect(&a.g, &b.g, tol)
                .into_iter()
                .map(|(t1, _)| a.g.eval(t1).0)
                .filter(|p| p.is_finite())
                .min_by(|p, q| by_clicks(p).total_cmp(&by_clicks(q)))
        };
        if let Some(x) = x {
            let (ta, tb) = (tangent_at(&a.g, x)?, tangent_at(&b.g, x)?);
            if ta.perp_dot(tb).abs() < PARALLEL {
                return None;
            }
        }
        Some(Corner {
            x,
            shared: shared.map(|(p, _)| p),
            at: x.unwrap_or((ca + cb) / 2.0),
        })
    }

    /// Kandidaten für Linie/Kreis/Bogen untereinander.
    fn fillet_exact_candidates(&self, a: &Side, b: &Side, r: f64) -> Vec<Cand> {
        let tol = self.tolerance().linear;
        let (Some(sa), Some(sb)) = (shape(&a.g), shape(&b.g)) else {
            return Vec::new();
        };
        let touch = |s: Shape, m: DVec2| match s {
            Shape::Line { a, d } => Some(a + d * (m - a).dot(d)),
            Shape::Round { c, r } => (m - c).try_normalize().map(|u| c + u * r),
        };
        let mut out = Vec::new();
        for s1 in [1.0, -1.0] {
            for s2 in [1.0, -1.0] {
                let (Some(oa), Some(ob)) =
                    (offset_shape(sa, s1, r, tol), offset_shape(sb, s2, r, tol))
                else {
                    continue;
                };
                for m in meet(oa, ob) {
                    if let (Some(t1), Some(t2)) = (touch(sa, m), touch(sb, m)) {
                        out.push(Cand { m, t: [t1, t2] });
                    }
                }
            }
        }
        out
    }

    /// Kandidaten, wenn eine Kurve Ellipse, Ellipsenbogen oder Spline ist: Die Mitte liegt im
    /// Abstand `r` entlang der Normalen der numerischen Kurve; wo ihr Abstand zur anderen Kurve
    /// `r` wird, ist sie tangential.
    fn fillet_numeric_candidates(&self, a: &Side, b: &Side, r: f64) -> Vec<Cand> {
        let (p, q, swapped) = if a.exact { (b, a, true) } else { (a, b, false) };
        let (lo, hi) = p.g.domain();
        let steps = match &p.g {
            Geom::Spline { n, .. } => SCAN_STEPS_PER_SEGMENT * (n - 1),
            _ => SCAN_STEPS,
        }
        .max(8);
        let mut out = Vec::new();
        for sign in [1.0, -1.0] {
            let at = |t: f64| -> Option<(DVec2, f64)> {
                let (pt, d) = p.g.eval(t);
                let m = pt + d.try_normalize()?.perp() * (sign * r);
                let g = m.distance(foot(&q.g, m)) - r;
                (m.is_finite() && g.is_finite()).then_some((m, g))
            };
            let mut prev: Option<(f64, f64)> = None;
            for i in 0..=steps {
                let t = lo + (hi - lo) * (i as f64) / (steps as f64);
                let cur = at(t).map(|(_, g)| (t, g));
                if let (Some((t0, g0)), Some((t1, g1))) = (prev, cur)
                    && (g0 == 0.0 || g0.signum() != g1.signum())
                {
                    let (mut ta, mut tb, mut ga) = (t0, t1, g0);
                    for _ in 0..BISECT_STEPS {
                        let tm = 0.5 * (ta + tb);
                        let Some((_, gm)) = at(tm) else { break };
                        if gm.signum() == ga.signum() {
                            ta = tm;
                            ga = gm;
                        } else {
                            tb = tm;
                        }
                    }
                    let t = 0.5 * (ta + tb);
                    if let Some((m, g)) = at(t) {
                        let tp = p.g.eval(t).0;
                        let dp = m.distance(foot(&p.g, m));
                        let tol = 1e-6 * (1.0 + r);
                        if g.abs() < tol && (dp - r).abs() < tol {
                            let tq = foot(&q.g, m);
                            let t = if swapped { [tq, tp] } else { [tp, tq] };
                            out.push(Cand { m, t });
                        }
                    }
                }
                prev = cur;
            }
        }
        out
    }

    /// Rundung zwischen zwei Kurven mit Radius `radius`. `None` bei unmöglichen Eingaben:
    /// Radius nicht endlich oder nicht positiv, dieselbe Kurve zweimal, parallele oder
    /// tangentiale Kurven, Berührpunkt jenseits des behaltenen Endes.
    pub fn fillet_plan(&self, a: CurvePick, b: CurvePick, radius: f64) -> Option<FilletPlan> {
        let tol = self.tolerance().linear;
        if !(radius.is_finite() && radius > tol) || a.curve == b.curve {
            return None;
        }
        let (sa, sb) = (self.fillet_side(a)?, self.fillet_side(b)?);
        let corner = self.fillet_corner_of(&sa, &sb)?;
        let pa = self.fillet_piece(&sa, corner.x, &sb.g);
        let pb = self.fillet_piece(&sb, corner.x, &sa.g);
        let exact = sa.exact && sb.exact;
        let cands = if exact {
            self.fillet_exact_candidates(&sa, &sb, radius)
        } else {
            self.fillet_numeric_candidates(&sa, &sb, radius)
        };
        let on_piece = |side: &Side, piece: &Option<Piece>, t: DVec2| match piece {
            Some(p) => coord(&side.g, t).is_some_and(|u| p.contains(u)),
            None => true,
        };
        let best = cands
            .into_iter()
            .filter(|c| c.m.is_finite() && c.t[0].is_finite() && c.t[1].is_finite())
            .filter(|c| c.t[0].distance(c.t[1]) > tol)
            .filter(|c| on_piece(&sa, &pa, c.t[0]) && on_piece(&sb, &pb, c.t[1]))
            .min_by(|c, d| {
                let score = |c: &Cand| c.t[0].distance(a.at) + c.t[1].distance(b.at);
                let (s, t) = (score(c), score(d));
                if (s - t).abs() > 1e-9 * (1.0 + s.abs()) {
                    s.total_cmp(&t)
                } else {
                    c.m.distance(corner.at).total_cmp(&d.m.distance(corner.at))
                }
            })?;
        // der kurze Bogen zwischen den Berührpunkten, gegen den Uhrzeigersinn
        let (u, v) = (best.t[0] - best.m, best.t[1] - best.m);
        let arc = if u.perp_dot(v) > 0.0 {
            ArcPlan {
                center: best.m,
                start: best.t[0],
                end: best.t[1],
            }
        } else {
            ArcPlan {
                center: best.m,
                start: best.t[1],
                end: best.t[0],
            }
        };
        let mv = |side: &Side, piece: &Option<Piece>| match (side.ends, piece) {
            (Some(ends), Some(p)) if !side.fixed[1 - p.far] => EndMove::Replace(ends[1 - p.far]),
            _ => EndMove::Whole,
        };
        let plan = FilletPlan {
            picks: [a, b],
            arc,
            touch: best.t,
            moves: [mv(&sa, &pa), mv(&sb, &pb)],
            corner: corner.shared,
            corner_at: corner.at,
            radius,
            exact,
        };
        let finite = plan.arc.sweep().is_finite() && plan.arc.radius().is_finite();
        finite.then_some(plan)
    }

    /// Die Ecke, auf die sich die Rundung der beiden Kurven bezieht (für die Vorschau des
    /// Radius am Zeiger); `None`, wo `fillet_plan` für jeden Radius `None` wäre.
    pub fn fillet_corner(&self, a: CurvePick, b: CurvePick) -> Option<DVec2> {
        if a.curve == b.curve {
            return None;
        }
        let (sa, sb) = (self.fillet_side(a)?, self.fillet_side(b)?);
        Some(self.fillet_corner_of(&sa, &sb)?.at)
    }

    /// Fase zwischen zwei Linien. `None` bei Bogen und anderen Kurven, bei parallelen Linien,
    /// bei einem Abstand, der das ferne Ende erreicht, bei nicht endlichen oder nicht
    /// positiven Maßen, bei festem nahem Ende und bei `DistanceAngle`, wenn der Nenner
    /// `sin(π − φ − θ)` nicht positiv ist.
    pub fn chamfer_plan(
        &self,
        a: CurvePick,
        b: CurvePick,
        kind: ChamferKind,
    ) -> Option<ChamferPlan> {
        let tol = self.tolerance().linear;
        let positive = |d: f64| d.is_finite() && d > tol;
        let valid = match kind {
            ChamferKind::Equal(d) => positive(d),
            ChamferKind::TwoDistances(d1, d2) => positive(d1) && positive(d2),
            ChamferKind::DistanceAngle(d, t) => positive(d) && t.is_finite() && t > 0.0 && t < PI,
        };
        if !valid || a.curve == b.curve {
            return None;
        }
        let (sa, sb) = (self.fillet_side(a)?, self.fillet_side(b)?);
        if !matches!(sa.g, Geom::Line { .. }) || !matches!(sb.g, Geom::Line { .. }) {
            return None;
        }
        let corner = self.fillet_corner_of(&sa, &sb)?;
        let x = corner.x?;
        let (pa, pb) = (
            self.fillet_piece(&sa, Some(x), &sb.g)?,
            self.fillet_piece(&sb, Some(x), &sa.g)?,
        );
        let dir = |s: &Side, p: &Piece| match s.g {
            Geom::Line { a, b } => (b - a).try_normalize().map(|d| d * p.dir),
            _ => None,
        };
        let (da, db) = (dir(&sa, &pa)?, dir(&sb, &pb)?);
        let phi = da.dot(db).clamp(-1.0, 1.0).acos();
        let (d1, d2) = match kind {
            ChamferKind::Equal(d) => (d, d),
            ChamferKind::TwoDistances(d1, d2) => (d1, d2),
            ChamferKind::DistanceAngle(d, theta) => {
                let rest = PI - phi - theta;
                let den = rest.sin();
                if rest <= 0.0 || den <= 1e-12 {
                    return None;
                }
                (d, d * theta.sin() / den)
            }
        };
        if !(d1 < pa.e_max - tol && d2 < pb.e_max - tol && positive(d1) && positive(d2)) {
            return None;
        }
        let ends = (sa.ends?, sb.ends?);
        let (na, nb) = (1 - pa.far, 1 - pb.far);
        if sa.fixed[na] || sb.fixed[nb] {
            return None;
        }
        let touch = [x + da * d1, x + db * d2];
        if !(touch[0].is_finite() && touch[1].is_finite()) {
            return None;
        }
        Some(ChamferPlan {
            picks: [a, b],
            touch,
            moves: [ends.0[na], ends.1[nb]],
            corner: corner.shared,
            corner_at: corner.at,
            kind,
        })
    }

    /// Hängt das Ende `from` der Linie, des Bogens oder des Ellipsenbogens `c` auf `to` um.
    /// Ein Plan, der nicht mehr zur Skizze passt, ergibt einen Fehler.
    fn rehang_end(&mut self, c: CurveId, from: PointId, to: PointId) -> Result<(), SketchError> {
        self.check_curve(c)?;
        let swap = |p: PointId| if p == from { to } else { p };
        let kind = match self.curves[c].kind.clone() {
            CurveKind::Line { a, b } if a == from || b == from => CurveKind::Line {
                a: swap(a),
                b: swap(b),
            },
            CurveKind::Arc { center, start, end } if start == from || end == from => {
                CurveKind::Arc {
                    center,
                    start: swap(start),
                    end: swap(end),
                }
            }
            CurveKind::EllipticArc {
                center,
                ax,
                ay,
                minor,
                start,
                end,
            } if start == from || end == from => CurveKind::EllipticArc {
                center,
                ax,
                ay,
                minor,
                start: swap(start),
                end: swap(end),
            },
            _ => return Err(SketchError::UnknownObject),
        };
        self.set_curve_kind(c, kind);
        Ok(())
    }

    fn fillet_helper_line(&mut self, a: PointId, b: PointId) -> Result<CurveId, SketchError> {
        let line = self.add_line(a, b)?;
        self.set_construction(line, true)?;
        Ok(line)
    }

    /// Die Ecke `p` bleibt als Konstruktionspunkt auf beiden Kurven.
    fn hold_corner(&mut self, p: PointId, curves: [CurveId; 2]) -> Result<(), SketchError> {
        self.points[p].construction = true;
        for c in curves {
            self.add_constraint(ConstraintKind::PointOnCurve(p, c))?;
        }
        Ok(())
    }

    /// Bindung der Rundung `arc` (Mitte `m`) an die Kurve `c`, die ganz bleibt, im Berührpunkt
    /// `t`. Gibt die Hilfslinie zurück, falls es eine braucht.
    ///
    /// Linie: tangential ohne gemeinsames Ende, Hilfslinie Mitte–Berührpunkt rechtwinklig.
    /// Kreis, Bogen: tangential, `t` auf der Verbindung der Mitten. Ellipse, Spline: nur `t` auf
    /// der Kurve, denn `Tangent(Bogen, Ellipse)` gibt es nicht.
    fn bind_whole(
        &mut self,
        c: CurveId,
        arc: CurveId,
        m: PointId,
        t: PointId,
    ) -> Result<Option<CurveId>, SketchError> {
        Ok(match self.curves[c].kind {
            CurveKind::Line { .. } => {
                self.add_constraint(ConstraintKind::Tangent(c, arc))?;
                let h = self.fillet_helper_line(m, t)?;
                self.add_constraint(ConstraintKind::Perpendicular(c, h))?;
                Some(h)
            }
            CurveKind::Circle { center, .. } | CurveKind::Arc { center, .. } => {
                self.add_constraint(ConstraintKind::Tangent(arc, c))?;
                let h = self.fillet_helper_line(center, m)?;
                self.add_constraint(ConstraintKind::PointOnCurve(t, h))?;
                Some(h)
            }
            _ => {
                self.add_constraint(ConstraintKind::PointOnCurve(t, c))?;
                None
            }
        })
    }

    /// Legt die Rundung aus `plan` an (Entscheidung 1 und 2 des Plans): Bogen mit eigener Mitte
    /// und eigenen Berührpunkten, Abhängigkeiten je nach Kurve und das Radiusmaß. Mit `equal_to`
    /// statt des Maßes `Equal` zu diesem Bogen; `plan` muss dann mit dessen Radius gerechnet
    /// sein, sonst bewegt das nächste Lösen die Skizze. Bei einem Fehler bleibt die Skizze
    /// unverändert.
    pub fn add_fillet(
        &mut self,
        plan: &FilletPlan,
        equal_to: Option<CurveId>,
    ) -> Result<FilletIds, SketchError> {
        self.atomically(|s| {
            let curves = plan.picks.map(|p| p.curve);
            for c in curves {
                s.check_curve(c)?;
            }
            let touch = [
                s.add_point(plan.touch[0].x, plan.touch[0].y)?,
                s.add_point(plan.touch[1].x, plan.touch[1].y)?,
            ];
            let center = s.add_point(plan.arc.center.x, plan.arc.center.y)?;
            let first = usize::from(plan.arc.start != plan.touch[0]);
            let arc = s.add_arc(center, touch[first], touch[1 - first])?;

            let mut touched = Vec::new();
            let mut moved = Vec::new();
            for i in 0..2 {
                if let EndMove::Replace(p) = plan.moves[i] {
                    s.rehang_end(curves[i], p, touch[i])?;
                    touched.push(curves[i]);
                    moved.push(p);
                }
            }
            s.settle_constraints(&touched, &[]);

            let mut helper = None;
            for i in 0..2 {
                let elliptic = matches!(s.curves[curves[i]].kind, CurveKind::EllipticArc { .. });
                match plan.moves[i] {
                    // Ein Ellipsenbogen hält sein neues Ende selbst auf der Ellipse.
                    EndMove::Replace(_) if elliptic => {}
                    EndMove::Replace(_) => {
                        s.add_constraint(ConstraintKind::Tangent(curves[i], arc))?;
                    }
                    EndMove::Whole => {
                        let h = s.bind_whole(curves[i], arc, center, touch[i])?;
                        helper = helper.or(h);
                    }
                }
            }

            // Hängt nach dem Umhängen noch etwas an der Ecke, bleibt sie; sonst entfällt sie
            // mit den übrigen verwaisten Enden. Ohne Umhängen ist sie weiter das Ende beider.
            let corner = plan.corner.filter(|p| {
                !moved.contains(p)
                    || s.curves().any(|(_, d)| d.kind.points().contains(p))
                    || s.constraints().any(|(_, d)| d.kind.points().contains(p))
            });
            if let Some(p) = corner
                && moved.contains(&p)
            {
                s.hold_corner(p, curves)?;
            } else {
                s.drop_orphans(&moved)?;
            }

            let radius_dim = match equal_to {
                Some(f0) => {
                    s.add_constraint(ConstraintKind::Equal(f0, arc))?;
                    None
                }
                None => Some(s.add_constraint(ConstraintKind::Radius(arc, plan.radius))?),
            };
            Ok(FilletIds {
                arc,
                center,
                touch,
                corner,
                helper,
                radius_dim,
            })
        })
    }

    /// Legt die Fase aus `plan` an (Entscheidung 9 des Plans): Linie zwischen den Enden, die
    /// Ecke bleibt immer als Konstruktionspunkt auf beiden Linien, weil die Maße von ihr aus
    /// zählen. Den Winkel misst sie aus der Lage. Bei einem Fehler bleibt die Skizze unverändert.
    pub fn add_chamfer(&mut self, plan: &ChamferPlan) -> Result<ChamferIds, SketchError> {
        self.atomically(|s| {
            let curves = plan.picks.map(|p| p.curve);
            for c in curves {
                s.check_curve(c)?;
            }
            let touch = [
                s.add_point(plan.touch[0].x, plan.touch[0].y)?,
                s.add_point(plan.touch[1].x, plan.touch[1].y)?,
            ];
            let line = s.add_line(touch[0], touch[1])?;
            for i in 0..2 {
                s.rehang_end(curves[i], plan.moves[i], touch[i])?;
            }
            s.settle_constraints(&curves, &[]);

            let corner = match plan.corner {
                Some(p) => p,
                None => s.add_point(plan.corner_at.x, plan.corner_at.y)?,
            };
            s.hold_corner(corner, curves)?;
            let orphans: Vec<PointId> = plan.moves.into_iter().filter(|p| *p != corner).collect();
            s.drop_orphans(&orphans)?;

            let mut helpers = None;
            let mut dims = Vec::new();
            match plan.kind {
                ChamferKind::Equal(d) => {
                    let h1 = s.fillet_helper_line(corner, touch[0])?;
                    let h2 = s.fillet_helper_line(corner, touch[1])?;
                    s.add_constraint(ConstraintKind::Equal(h1, h2))?;
                    helpers = Some([h1, h2]);
                    dims.push(s.add_constraint(ConstraintKind::Distance(corner, touch[0], d))?);
                }
                ChamferKind::TwoDistances(d1, d2) => {
                    dims.push(s.add_constraint(ConstraintKind::Distance(corner, touch[0], d1))?);
                    dims.push(s.add_constraint(ConstraintKind::Distance(corner, touch[1], d2))?);
                }
                ChamferKind::DistanceAngle(d, _) => {
                    dims.push(s.add_constraint(ConstraintKind::Distance(corner, touch[0], d))?);
                    let value = s
                        .measure(&ConstraintKind::Angle(curves[0], line, 0.0))
                        .ok_or_else(|| SketchError::Degenerate(tr!("sketch-err-line-zero")))?;
                    dims.push(s.add_constraint(ConstraintKind::Angle(curves[0], line, value))?);
                }
            }
            Ok(ChamferIds {
                line,
                touch,
                corner,
                helpers,
                dims,
            })
        })
    }

    /// Lebende Linien, Bögen und Ellipsenbögen, die in `p` enden (nicht die, deren Mitte `p` ist).
    pub fn curves_at_point(&self, p: PointId) -> Vec<CurveId> {
        self.curves()
            .filter(|(_, c)| match c.kind {
                CurveKind::Line { a, b } => a == p || b == p,
                CurveKind::Arc { start, end, .. } | CurveKind::EllipticArc { start, end, .. } => {
                    start == p || end == p
                }
                _ => false,
            })
            .map(|(id, _)| id)
            .collect()
    }
}
