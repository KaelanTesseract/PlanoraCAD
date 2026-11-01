//! Schnittpunkte zweier Skizzenkurven und Selbstschnitte von Splines.
//!
//! Linie, Kreis und Bogen untereinander analytisch; alles mit Ellipse, Ellipsenbogen oder
//! Spline über Linienzüge: Kreuzungen der Linienzüge und lokale Abstandsminima (Berührungen)
//! liefern Startwerte, Gauß-Newton auf `C1(t1) − C2(t2)` schärft nach. Ein Schnitt gilt bei
//! Restabstand ≤ `tol`. Enden offener Kurven, die auf der anderen liegen, zählen immer.

use crate::geom::Geom;
use crate::model::{CurveId, Sketch};
use planora_math::{DVec2, TAU};
use std::cell::OnceCell;

/// Abschnitte je Vollkreis der Linienzüge für die Startwerte (Splines 16 je Segment).
const STEPS_PER_TURN: usize = 64;
/// Linienzug-Abschnitte je Hüllrechteck-Gruppe beim Vortest der Abschnittspaare.
const CHUNK: usize = 8;
/// Höchstzahl der Gauß-Newton-Schritte.
const MAX_NEWTON_STEPS: usize = 20;
/// Höchstzahl der Halbierungen eines Gauß-Newton-Schritts, der den Rest nicht verkleinert.
const MAX_HALVINGS: usize = 30;
/// Spielraum der Abschnittsparameter beim Kreuzungstest (fängt Kreuzungen genau an Ecken).
const CROSS_SLACK: f64 = 1e-9;

/// Parameterpaare `(t1, t2)` aller Schnitt- und Berührpunkte von `g1` und `g2` mit Restabstand
/// ≤ `tol`, entdoppelt. Kollineare Linien sowie Kreise und Ellipsen auf derselben Kurve liefern
/// die Enden des gemeinsamen Bereichs (zwei gleiche Vollkreise keinen Punkt).
#[allow(dead_code, reason = "used by extend (later task)")]
pub(crate) fn intersect(g1: &Geom, g2: &Geom, tol: f64) -> Vec<(f64, f64)> {
    intersect_prepared(&Prepared::new(g1), &Prepared::new(g2), tol)
}

/// Kurve mit einmal berechneter Hülle und bei Bedarf berechnetem Linienzug, damit eine Kurve
/// gegen viele andere geschnitten werden kann, ohne beides jedes Mal neu zu bestimmen.
pub(crate) struct Prepared<'a> {
    g: &'a Geom,
    bbox: (DVec2, DVec2),
    poly: OnceCell<Poly>,
}

impl<'a> Prepared<'a> {
    pub(crate) fn new(g: &'a Geom) -> Prepared<'a> {
        Prepared {
            g,
            bbox: g.bbox(),
            poly: OnceCell::new(),
        }
    }

    fn poly(&self) -> &Poly {
        self.poly.get_or_init(|| Poly::new(self.g))
    }
}

/// [`intersect`] für vorbereitete Kurven.
pub(crate) fn intersect_prepared(p1: &Prepared, p2: &Prepared, tol: f64) -> Vec<(f64, f64)> {
    let (g1, g2) = (p1.g, p2.g);
    if !boxes_touch(p1.bbox, p2.bbox, 2.0 * tol) {
        return Vec::new();
    }
    let mut raw = match (circle_like(g1), circle_like(g2)) {
        (Some(a), Some(b)) => analytic(&a, &b, tol),
        _ => numeric(p1, p2, tol),
    };
    raw.extend(end_touches(g1, p2, tol));
    raw.extend(end_touches(g2, p1, tol).into_iter().map(|(a, b)| (b, a)));
    finish(g1, g2, raw, tol)
}

/// Selbstschnitte eines Splines als Parameterpaare `t1 < t2` (Kreuzungen nicht benachbarter
/// Linienzug-Abschnitte, nachgeschärft); andere Kurven haben keine.
pub(crate) fn self_intersect(g: &Geom, tol: f64) -> Vec<(f64, f64)> {
    if !matches!(g, Geom::Spline { .. }) {
        return Vec::new();
    }
    let poly = Poly::new(g);
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (i, j) in poly.chunk_pairs(&poly, 0.0) {
        if j < i {
            continue;
        }
        for k in poly.chunk_segments(i) {
            for l in poly.chunk_segments(j) {
                if l < k + 2 {
                    continue;
                }
                let Some((u, v)) = segment_cross(poly.seg(k), poly.seg(l)) else {
                    continue;
                };
                let Some((t1, t2)) = refine(g, g, poly.param(k, u), poly.param(l, v), tol) else {
                    continue;
                };
                let (t1, t2) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
                // zum selben Punkt zusammengelaufen: kein Selbstschnitt
                if close_along(g, t1, t2, tol) {
                    continue;
                }
                out.push((t1, t2));
            }
        }
    }
    dedupe(g, g, out, tol)
}

impl Sketch {
    /// Sortierte Parameter (siehe [`Sketch::curve_eval`]) aller Schnitt- und Berührpunkte der
    /// lebenden Kurve `c` mit allen anderen lebenden Kurven (Konstruktionskurven nur bei
    /// `with_construction`) und mit sich selbst. Stellen, die längs der Kurve höchstens
    /// `tol.linear` auseinanderliegen, zählen einmal; Enden offener Kurven fehlen.
    pub fn split_params(&self, c: CurveId, with_construction: bool) -> Vec<f64> {
        let Some(g) = self.geom(c) else {
            return Vec::new();
        };
        let tol = self.tolerance().linear;
        let mut ts = Vec::new();
        let pg = Prepared::new(&g);
        for (id, data) in self.curves() {
            if id == c || (data.construction && !with_construction) {
                continue;
            }
            if let Some(h) = self.geom(id) {
                let hits = intersect_prepared(&pg, &Prepared::new(&h), tol);
                ts.extend(hits.into_iter().map(|(t, _)| t));
            }
        }
        for (a, b) in self_intersect(&g, tol) {
            ts.push(a);
            ts.push(b);
        }
        clean_params(&g, ts, tol)
    }
}

/// Parameter in den Bereich gebracht, sortiert, längs der Kurve auf `tol` entdoppelt
/// (periodisch auch über den Umlauf); bei offenen Kurven ohne die Enden.
pub(crate) fn clean_params(g: &Geom, ts: Vec<f64>, tol: f64) -> Vec<f64> {
    let (lo, hi) = g.domain();
    let mut ts: Vec<f64> = ts
        .into_iter()
        .filter(|t| t.is_finite())
        .map(|t| g.normalize(t))
        .collect();
    ts.sort_by(f64::total_cmp);
    let mut out: Vec<f64> = Vec::with_capacity(ts.len());
    for t in ts {
        if !g.periodic() && (close_along(g, lo, t, tol) || close_along(g, t, hi, tol)) {
            continue;
        }
        if out.last().is_some_and(|&p| close_along(g, p, t, tol)) {
            continue;
        }
        out.push(t);
    }
    if g.periodic() && out.len() > 1 && g.arc_length(out[out.len() - 1], out[0] + TAU) <= tol {
        out.pop();
    }
    out
}

/// Überlappen sich die Hüllrechtecke mit Spielraum `margin`?
fn boxes_touch(a: (DVec2, DVec2), b: (DVec2, DVec2), margin: f64) -> bool {
    a.0.x <= b.1.x + margin
        && b.0.x <= a.1.x + margin
        && a.0.y <= b.1.y + margin
        && b.0.y <= a.1.y + margin
}

/// Linie, Kreis oder Bogen in analytischer Form.
enum Simple {
    Seg {
        a: DVec2,
        b: DVec2,
    },
    /// `range = Some((a0, sweep))` für Bögen.
    Circ {
        c: DVec2,
        r: f64,
        range: Option<(f64, f64)>,
    },
}

fn circle_like(g: &Geom) -> Option<Simple> {
    match g {
        Geom::Line { a, b } => Some(Simple::Seg { a: *a, b: *b }),
        Geom::Circle { c, r } => Some(Simple::Circ {
            c: *c,
            r: *r,
            range: None,
        }),
        Geom::Arc { c, r, a0, sweep } => Some(Simple::Circ {
            c: *c,
            r: *r,
            range: Some((*a0, *sweep)),
        }),
        _ => None,
    }
}

/// Parameter der Strecke `a`–`b` beim Punkt `p`, falls er höchstens `tol` jenseits der Enden
/// liegt (dann auf das Ende geklemmt).
fn seg_param(a: DVec2, b: DVec2, p: DVec2, tol: f64) -> Option<f64> {
    let d = b - a;
    let len2 = d.length_squared();
    if len2.is_nan() || len2 <= 0.0 {
        return (p.distance(a) <= tol).then_some(0.0);
    }
    let t = (p - a).dot(d) / len2;
    let e = tol / len2.sqrt();
    (t >= -e && t <= 1.0 + e).then(|| t.clamp(0.0, 1.0))
}

/// Winkelparameter beim Winkel `theta` auf einem Kreis bzw. Bogen (`range`), mit `tol` Spielraum
/// längs des Kreises an den Bogenenden.
fn angle_param(range: Option<(f64, f64)>, r: f64, theta: f64, tol: f64) -> Option<f64> {
    let Some((a0, sweep)) = range else {
        return Some(theta.rem_euclid(TAU));
    };
    let e = if r > 0.0 { tol / r } else { 0.0 };
    let rel = (theta - a0).rem_euclid(TAU);
    if rel <= sweep + e {
        Some(a0 + rel.min(sweep))
    } else if rel >= TAU - e {
        Some(a0)
    } else {
        None
    }
}

fn angle_of(c: DVec2, p: DVec2) -> f64 {
    let v = p - c;
    v.y.atan2(v.x)
}

/// Parameter eines Punkts auf einer analytischen Kurve (mit Spielraum `tol`).
fn simple_param(s: &Simple, p: DVec2, tol: f64) -> Option<f64> {
    match s {
        Simple::Seg { a, b } => seg_param(*a, *b, p, tol),
        Simple::Circ { c, r, range } => angle_param(*range, *r, angle_of(*c, p), tol),
    }
}

/// Analytische Schnitte von Linie, Kreis und Bogen.
fn analytic(s1: &Simple, s2: &Simple, tol: f64) -> Vec<(f64, f64)> {
    match (s1, s2) {
        (Simple::Seg { a: a1, b: b1 }, Simple::Seg { a: a2, b: b2 }) => {
            line_line(*a1, *b1, *a2, *b2, tol)
        }
        (Simple::Circ { c: c1, r: r1, .. }, Simple::Circ { c: c2, r: r2, .. })
            if c1.distance(*c2) <= tol && (r1 - r2).abs() <= tol =>
        {
            same_circle(s1, s2, tol)
        }
        _ => {
            let points = match (s1, s2) {
                (Simple::Seg { a, b }, Simple::Circ { c, r, .. })
                | (Simple::Circ { c, r, .. }, Simple::Seg { a, b }) => {
                    line_circle(*a, *b, *c, *r, tol)
                }
                (Simple::Circ { c: c1, r: r1, .. }, Simple::Circ { c: c2, r: r2, .. }) => {
                    circle_circle(*c1, *r1, *c2, *r2, tol)
                }
                _ => Vec::new(),
            };
            points
                .into_iter()
                .filter_map(|p| Some((simple_param(s1, p, tol)?, simple_param(s2, p, tol)?)))
                .collect()
        }
    }
}

/// Schnitt zweier Strecken; kollinear die Enden des gemeinsamen Bereichs.
fn line_line(a1: DVec2, b1: DVec2, a2: DVec2, b2: DVec2, tol: f64) -> Vec<(f64, f64)> {
    let (d1, d2) = (b1 - a1, b2 - a2);
    let (l1, l2) = (d1.length(), d2.length());
    if !(l1 > 0.0 && l2 > 0.0) {
        // zu einem Punkt zusammengefallen: Enden prüft `end_touches`
        return Vec::new();
    }
    let off = |p: DVec2| d1.perp_dot(p - a1).abs() / l1;
    if off(a2) <= tol && off(b2) <= tol {
        // kollinear: Bereich der zweiten Strecke auf der ersten
        let s0 = (a2 - a1).dot(d1) / (l1 * l1);
        let s1 = (b2 - a1).dot(d1) / (l1 * l1);
        let lo = s0.min(s1).max(0.0);
        let hi = s0.max(s1).min(1.0);
        let e1 = tol / l1;
        if hi < lo - e1 {
            return Vec::new();
        }
        let ends = if hi - lo <= e1 {
            vec![0.5 * (lo + hi)]
        } else {
            vec![lo, hi]
        };
        return ends
            .into_iter()
            .map(|t| {
                let p = a1 + d1 * t;
                (t, ((p - a2).dot(d2) / (l2 * l2)).clamp(0.0, 1.0))
            })
            .collect();
    }
    let den = d1.perp_dot(d2);
    if den.abs() <= f64::EPSILON * l1 * l2 {
        return Vec::new();
    }
    let w = a2 - a1;
    let t1 = w.perp_dot(d2) / den;
    let t2 = w.perp_dot(d1) / den;
    let (e1, e2) = (tol / l1, tol / l2);
    if (-e1..=1.0 + e1).contains(&t1) && (-e2..=1.0 + e2).contains(&t2) {
        vec![(t1.clamp(0.0, 1.0), t2.clamp(0.0, 1.0))]
    } else {
        Vec::new()
    }
}

/// Schnittpunkte der Geraden durch `a`, `b` mit dem Kreis; Abstand `r ± tol` = Berührpunkt.
fn line_circle(a: DVec2, b: DVec2, c: DVec2, r: f64, tol: f64) -> Vec<DVec2> {
    let d = b - a;
    let len2 = d.length_squared();
    if len2.is_nan() || len2 <= 0.0 {
        return Vec::new();
    }
    let tp = (c - a).dot(d) / len2;
    let foot = a + d * tp;
    let h = foot.distance(c);
    if h > r + tol {
        Vec::new()
    } else if (h - r).abs() <= tol {
        vec![foot]
    } else {
        let s = (r * r - h * h).max(0.0).sqrt() / len2.sqrt();
        vec![a + d * (tp - s), a + d * (tp + s)]
    }
}

/// Schnittpunkte zweier Kreise mit verschiedenen Mitten oder Radien; Berührung innen oder außen
/// (auf `tol`) gibt einen Punkt.
fn circle_circle(c1: DVec2, r1: f64, c2: DVec2, r2: f64, tol: f64) -> Vec<DVec2> {
    let v = c2 - c1;
    let d = v.length();
    if d.is_nan() || d <= 0.0 || d > r1 + r2 + tol || d < (r1 - r2).abs() - tol {
        return Vec::new();
    }
    let u = v / d;
    if (d - (r1 + r2)).abs() <= tol {
        return vec![(c1 + u * r1 + (c2 - u * r2)) * 0.5];
    }
    if (d - (r1 - r2).abs()).abs() <= tol {
        let s = if r1 >= r2 { 1.0 } else { -1.0 };
        return vec![(c1 + u * (s * r1) + (c2 + u * (s * r2))) * 0.5];
    }
    let a = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let base = c1 + u * a;
    vec![base + u.perp() * h, base - u.perp() * h]
}

/// Zwei Kreise oder Bögen auf demselben Kreis: Bogenenden, die im Bereich der anderen Kurve
/// liegen.
fn same_circle(s1: &Simple, s2: &Simple, tol: f64) -> Vec<(f64, f64)> {
    let (
        Simple::Circ {
            c: c1,
            r: r1,
            range: g1,
        },
        Simple::Circ {
            c: c2,
            r: r2,
            range: g2,
        },
    ) = (s1, s2)
    else {
        return Vec::new();
    };
    let mut ends = Vec::new();
    for (a0, sweep) in [*g1, *g2].into_iter().flatten() {
        ends.push(a0);
        ends.push(a0 + sweep);
    }
    // Winkel gegen die eigene Mitte des anderen Kreises (Mitten können um ≤ tol abweichen)
    ends.into_iter()
        .filter_map(|theta| {
            let p = *c1 + DVec2::new(theta.cos(), theta.sin()) * *r1;
            Some((
                angle_param(*g1, *r1, angle_of(*c1, p), tol)?,
                angle_param(*g2, *r2, angle_of(*c2, p), tol)?,
            ))
        })
        .collect()
}

/// Ellipsen bzw. Ellipsenbögen auf derselben Ellipse: Verschiebung des Parameters von `g1`
/// nach `g2` (0 oder π, weil `a` und `b` gemeinsam das Vorzeichen wechseln dürfen).
fn same_ellipse_shift(g1: &Geom, g2: &Geom, tol: f64) -> Option<f64> {
    let axes = |g: &Geom| match g {
        Geom::Ellipse { c, a, b } | Geom::EllipticArc { c, a, b, .. } => Some((*c, *a, *b)),
        _ => None,
    };
    let ((c1, a1, b1), (c2, a2, b2)) = (axes(g1)?, axes(g2)?);
    if c1.distance(c2) > tol {
        return None;
    }
    if a1.distance(a2) <= tol && b1.distance(b2) <= tol {
        Some(0.0)
    } else if a1.distance(-a2) <= tol && b1.distance(-b2) <= tol {
        Some(TAU / 2.0)
    } else {
        None
    }
}

/// Enden des gemeinsamen Bereichs zweier Kurven auf derselben Ellipse.
fn same_ellipse(g1: &Geom, g2: &Geom, shift: f64, tol: f64) -> Vec<(f64, f64)> {
    let range = |g: &Geom| match g {
        Geom::EllipticArc { t0, sweep, .. } => Some((*t0, *sweep)),
        _ => None,
    };
    let minor = |g: &Geom| match g {
        Geom::Ellipse { b, .. } | Geom::EllipticArc { b, .. } => b.length(),
        _ => 0.0,
    };
    let (r1, r2) = (range(g1), range(g2));
    let mut out = Vec::new();
    for (a0, sweep) in r1.into_iter() {
        for t in [a0, a0 + sweep] {
            if let Some(t2) = angle_param(r2, minor(g2), t + shift, tol) {
                out.push((t, t2));
            }
        }
    }
    for (a0, sweep) in r2.into_iter() {
        for t in [a0, a0 + sweep] {
            if let Some(t1) = angle_param(r1, minor(g1), t - shift, tol) {
                out.push((t1, t));
            }
        }
    }
    out
}

/// Numerische Schnitte über Linienzüge mit Nachschärfen.
fn numeric(q1: &Prepared, q2: &Prepared, tol: f64) -> Vec<(f64, f64)> {
    let (g1, g2) = (q1.g, q2.g);
    if let Some(shift) = same_ellipse_shift(g1, g2, tol) {
        return same_ellipse(g1, g2, shift, tol);
    }
    let (p1, p2) = (q1.poly(), q2.poly());
    let mut seeds = Vec::new();
    // Kreuzungen der Linienzüge
    for (i, j) in p1.chunk_pairs(p2, 0.0) {
        for k in p1.chunk_segments(i) {
            for l in p2.chunk_segments(j) {
                if let Some((u, v)) = segment_cross(p1.seg(k), p2.seg(l)) {
                    seeds.push((p1.param(k, u), p2.param(l, v)));
                }
            }
        }
    }
    // Berührungen: lokale Abstandsminima unter 2·tol plus Sehnenfehler der Linienzüge
    let near = 2.0 * tol + 4.0 * (p1.sag + p2.sag);
    seeds.extend(p1.close_minima(p2, near));
    seeds.extend(p2.close_minima(p1, near).into_iter().map(|(a, b)| (b, a)));
    seeds
        .into_iter()
        .filter_map(|(t1, t2)| refine(g1, g2, t1, t2, tol))
        .collect()
}

/// Enden der offenen Kurve `g1`, die höchstens `tol` neben `q2` liegen, als `(t1, t2)`.
fn end_touches(g1: &Geom, q2: &Prepared, tol: f64) -> Vec<(f64, f64)> {
    if g1.periodic() {
        return Vec::new();
    }
    let g2 = q2.g;
    let (lo, hi) = g1.domain();
    [lo, hi]
        .into_iter()
        .filter_map(|t| {
            let p = g1.eval(t).0;
            if !boxes_touch((p, p), q2.bbox, tol) {
                return None;
            }
            let t2 = g2.nearest(p);
            (g2.eval(t2).0.distance(p) <= tol).then_some((t, t2))
        })
        .collect()
}

/// Gauß-Newton auf `C1(t1) − C2(t2)` (höchstens 20 Schritte, Schritte ohne Verbesserung
/// halbiert, offene Kurven auf ihren Bereich geklemmt); `Some`, wenn der Rest ≤ `tol` ist.
fn refine(g1: &Geom, g2: &Geom, mut t1: f64, mut t2: f64, tol: f64) -> Option<(f64, f64)> {
    let clamp = |g: &Geom, t: f64| {
        if g.periodic() {
            t
        } else {
            let (lo, hi) = g.domain();
            t.clamp(lo, hi)
        }
    };
    let residual = |t1: f64, t2: f64| g1.eval(t1).0 - g2.eval(t2).0;
    let mut f = residual(t1, t2);
    for _ in 0..MAX_NEWTON_STEPS {
        let r = f.length();
        if !r.is_finite() {
            return None;
        }
        if r <= tol * 1e-9 {
            break;
        }
        let (d1, d2) = (g1.eval(t1).1, -g2.eval(t2).1);
        // Normalgleichungen mit winziger Dämpfung (Berührungen machen J fast singulär)
        let (j11, j12, j22) = (d1.dot(d1), d1.dot(d2), d2.dot(d2));
        let damp = 1e-12 * (j11 + j22) + f64::MIN_POSITIVE;
        let (m11, m22) = (j11 + damp, j22 + damp);
        let (g1v, g2v) = (d1.dot(f), d2.dot(f));
        let det = m11 * m22 - j12 * j12;
        if !(det.is_finite() && det > 0.0) {
            break;
        }
        let mut s1 = -(m22 * g1v - j12 * g2v) / det;
        let mut s2 = -(m11 * g2v - j12 * g1v) / det;
        let mut improved = false;
        for _ in 0..MAX_HALVINGS {
            let (n1, n2) = (clamp(g1, t1 + s1), clamp(g2, t2 + s2));
            let nf = residual(n1, n2);
            if nf.length() < r {
                (t1, t2, f) = (n1, n2, nf);
                improved = true;
                break;
            }
            s1 *= 0.5;
            s2 *= 0.5;
        }
        if !improved {
            break;
        }
    }
    (f.length() <= tol && t1.is_finite() && t2.is_finite()).then_some((t1, t2))
}

/// Ergebnisse in die Bereiche bringen, Rest prüfen, entdoppeln.
fn finish(g1: &Geom, g2: &Geom, raw: Vec<(f64, f64)>, tol: f64) -> Vec<(f64, f64)> {
    let ok: Vec<(f64, f64)> = raw
        .into_iter()
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .map(|(a, b)| (g1.normalize(a), g2.normalize(b)))
        .filter(|&(a, b)| g1.eval(a).0.distance(g2.eval(b).0) <= tol)
        .collect();
    dedupe(g1, g2, ok, tol)
}

/// Liegen die Parameter `a` und `b` längs der Kurve höchstens `tol` auseinander? Erst die
/// Sehne (billig, nie länger als der Bogen), dann die Bogenlänge; periodisch über den kürzeren
/// Parameterweg.
fn close_along(g: &Geom, a: f64, b: f64, tol: f64) -> bool {
    if g.eval(a).0.distance(g.eval(b).0) > tol {
        return false;
    }
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let (lo, hi) = if g.periodic() && hi - lo > TAU / 2.0 {
        (hi, lo + TAU)
    } else {
        (lo, hi)
    };
    g.arc_length(lo, hi) <= tol
}

/// Paare, die auf beiden Kurven höchstens `tol` auseinanderliegen, zählen einmal.
fn dedupe(g1: &Geom, g2: &Geom, pairs: Vec<(f64, f64)>, tol: f64) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(pairs.len());
    for (a, b) in pairs {
        let dup = out
            .iter()
            .any(|&(x, y)| close_along(g1, a, x, tol) && close_along(g2, b, y, tol));
        if !dup {
            out.push((a, b));
        }
    }
    out.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
    out
}

/// Kreuzung der Strecken `p`–`q` und `r`–`s` als Streckenparameter `(u, v)`.
fn segment_cross((p, q): (DVec2, DVec2), (r, s): (DVec2, DVec2)) -> Option<(f64, f64)> {
    let (d1, d2) = (q - p, s - r);
    let den = d1.perp_dot(d2);
    if den.is_nan() || den.abs() <= f64::EPSILON * d1.length() * d2.length() {
        return None;
    }
    let w = r - p;
    let u = w.perp_dot(d2) / den;
    let v = w.perp_dot(d1) / den;
    let range = -CROSS_SLACK..=1.0 + CROSS_SLACK;
    (range.contains(&u) && range.contains(&v)).then(|| (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)))
}

/// Abstand von `p` zur Strecke `a`–`b` und Streckenparameter des Fußpunkts.
fn segment_foot(p: DVec2, a: DVec2, b: DVec2) -> (f64, f64) {
    let d = b - a;
    let len2 = d.length_squared();
    let u = if len2 > 0.0 {
        ((p - a).dot(d) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.distance(a + d * u), u)
}

/// Linienzug einer Kurve über ihren ganzen Bereich mit Hüllrechtecken je `CHUNK` Abschnitte.
struct Poly {
    ts: Vec<f64>,
    ps: Vec<DVec2>,
    /// (erster Abschnitt, Ende exklusiv, min, max)
    chunks: Vec<(usize, usize, DVec2, DVec2)>,
    /// Größter Abstand einer Abschnittsmitte von der Kurve (Sehnenfehler).
    sag: f64,
}

impl Poly {
    fn new(g: &Geom) -> Poly {
        let (lo, hi) = g.domain();
        let ts = g.polyline_params(lo, hi, STEPS_PER_TURN);
        let ps: Vec<DVec2> = ts.iter().map(|&t| g.eval(t).0).collect();
        let segs = ps.len() - 1;
        let sag = (0..segs)
            .map(|k| {
                let mid = g.eval(0.5 * (ts[k] + ts[k + 1])).0;
                mid.distance((ps[k] + ps[k + 1]) * 0.5)
            })
            .filter(|d| d.is_finite())
            .fold(0.0, f64::max);
        // Gruppen-Hüllen um den Sehnenfehler vergrößert, damit sie auch die Kurve enthalten
        let chunks = (0..segs)
            .step_by(CHUNK)
            .map(|s| {
                let e = (s + CHUNK).min(segs);
                let (mn, mx) = ps[s..=e]
                    .iter()
                    .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(a, b), p| {
                        (a.min(*p), b.max(*p))
                    });
                (s, e, mn - DVec2::splat(sag), mx + DVec2::splat(sag))
            })
            .collect();
        Poly {
            ts,
            ps,
            chunks,
            sag,
        }
    }

    fn seg(&self, k: usize) -> (DVec2, DVec2) {
        (self.ps[k], self.ps[k + 1])
    }

    fn param(&self, k: usize, u: f64) -> f64 {
        self.ts[k] + (self.ts[k + 1] - self.ts[k]) * u
    }

    fn chunk_segments(&self, i: usize) -> std::ops::Range<usize> {
        self.chunks[i].0..self.chunks[i].1
    }

    /// Gruppenpaare `(i, j)`, deren Hüllen sich mit Spielraum `margin` berühren.
    fn chunk_pairs(&self, other: &Poly, margin: f64) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (i, a) in self.chunks.iter().enumerate() {
            for (j, b) in other.chunks.iter().enumerate() {
                if boxes_touch((a.2, a.3), (b.2, b.3), margin) {
                    out.push((i, j));
                }
            }
        }
        out
    }

    /// Ecken dieses Linienzugs, deren Abstand zum anderen ein lokales Minimum unter `near` ist,
    /// als Parameterpaare (eigener Parameter, Parameter des Fußpunkts).
    fn close_minima(&self, other: &Poly, near: f64) -> Vec<(f64, f64)> {
        let feet: Vec<Option<(f64, f64)>> = self
            .ps
            .iter()
            .map(|&p| {
                let mut best: Option<(f64, f64)> = None;
                for &(s, e, mn, mx) in &other.chunks {
                    if !boxes_touch((p, p), (mn, mx), near) {
                        continue;
                    }
                    for l in s..e {
                        let (d, u) = segment_foot(p, other.ps[l], other.ps[l + 1]);
                        if best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, other.param(l, u)));
                        }
                    }
                }
                best.filter(|(d, _)| *d <= near)
            })
            .collect();
        let dist = |k: usize| feet[k].map_or(f64::INFINITY, |(d, _)| d);
        (0..feet.len())
            .filter_map(|k| {
                let (d, t2) = feet[k]?;
                let left = k == 0 || d <= dist(k - 1);
                let right = k + 1 == feet.len() || d <= dist(k + 1);
                (left && right).then_some((self.ts[k], t2))
            })
            .collect()
    }
}
