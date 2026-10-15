//! Ausgabekurven in Blattkoordinaten (mm, y nach oben): Linie, Kreisbogen, Ellipse (Hauptachsen)
//! und rationale B-Spline-Kurve. Die Umwandlung aus einem Strich (`piece_to_out`) projiziert die
//! 3D-Kurve auf die Bildebene und bringt sie in die Normalform der Ausgabe.

use crate::basis::Basis;
use crate::sheet::SheetTransform;
use crate::stroke::Stroke;
use planora_kernel::Curve;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, PI, TAU};

/// Verhältnis der Halbachsen, ab dem eine Ellipse als Kreis (`ratio ≥ 1 − RATIO_EPS`) bzw.
/// als Strecke (`ratio ≤ RATIO_EPS`) gezeichnet wird.
const RATIO_EPS: f64 = 1e-9;

/// Ausgabekurve in Blattkoordinaten.
#[derive(Clone, Debug, PartialEq)]
pub enum Out2d {
    /// Strecke von `a` nach `b`; Parameter `t ∈ [0, 1]`.
    Line { a: DVec2, b: DVec2 },
    /// Kreisbogen gegen den Uhrzeigersinn von `start` bis `end` (Radiant, `end > start`, höchstens TAU); voller Kreis: end = start + TAU.
    /// `start` und `end` dürfen negativ oder größer als TAU sein (sie folgen dem Parameter des
    /// Strichs); Schreiber müssen über [`Out2d::normalized_range`] auf `[0, TAU)` und Öffnungswinkel
    /// umrechnen.
    Arc {
        c: DVec2,
        r: f64,
        start: f64,
        end: f64,
    },
    /// Ellipse mit Hauptachse `major` (Vektor vom Mittelpunkt zum Hauptscheitel), Nebenachse `ratio·|major|`, Parameter gegen den Uhrzeigersinn (Radiant).
    /// `start`/`end` wie beim Bogen unnormiert (siehe [`Out2d::normalized_range`]).
    Ellipse {
        c: DVec2,
        major: DVec2,
        ratio: f64,
        start: f64,
        end: f64,
    },
    /// Rationale B-Spline-Kurve (Grad, Knoten, Kontrollpunkte, Gewichte), Parameterbereich = Knotenbereich.
    /// Ein von Hand gebauter, ungültiger Spline (leer, falsche Knotenzahl, fallende Knoten,
    /// Gewicht ≤ 0) lässt `point` und `length` NaN und `bounds` (NaN, NaN) liefern, nie eine
    /// Panik; `is_finite` ist dann `false`.
    Spline {
        degree: usize,
        knots: Vec<f64>,
        ctrl: Vec<DVec2>,
        weights: Vec<f64>,
    },
}

type H3 = [f64; 3];

/// Stützstellen der fünfpunktigen Gauß-Legendre-Formel auf `[-1, 1]` (Knoten und Gewichte).
const GL5: [(f64, f64); 5] = [
    (0.0, 0.568_888_888_888_888_9),
    (0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (-0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (0.906_179_845_938_664, 0.236_926_885_056_189_1),
    (-0.906_179_845_938_664, 0.236_926_885_056_189_1),
];

fn gauss5(f: &dyn Fn(f64) -> f64, a: f64, b: f64) -> f64 {
    let (m, h) = (0.5 * (a + b), 0.5 * (b - a));
    h * GL5.iter().map(|&(x, w)| w * f(m + h * x)).sum::<f64>()
}

/// Punkt auf einer Ellipse in Hauptachsenform.
fn ellipse_point(c: DVec2, major: DVec2, ratio: f64, t: f64) -> DVec2 {
    let minor = DVec2::new(-major.y, major.x) * ratio;
    c + major * t.cos() + minor * t.sin()
}

impl Out2d {
    /// Punkt bei Parameter `t`: Strecke `t ∈ [0, 1]` (`a + t·(b − a)`), Bogen der Winkel
    /// (Radiant), Ellipse der Parameter `c + major·cos t + minor·sin t`, Spline der Knotenparameter
    /// (außerhalb des Bereichs begrenzt).
    pub fn point(&self, t: f64) -> DVec2 {
        match self {
            Out2d::Line { a, b } => *a + (*b - *a) * t,
            Out2d::Arc { c, r, .. } => *c + DVec2::new(t.cos(), t.sin()) * *r,
            Out2d::Ellipse {
                c, major, ratio, ..
            } => ellipse_point(*c, *major, *ratio, t),
            Out2d::Spline {
                degree,
                knots,
                ctrl,
                weights,
            } => Spl::new(*degree, knots, ctrl, weights).map_or(DVec2::NAN, |s| s.point(t)),
        }
    }

    /// Länge der Kurve in mm (Ellipse und Spline numerisch, Fehler weit unter 1e-9 relativ).
    pub fn length(&self) -> f64 {
        match self {
            Out2d::Line { a, b } => a.distance(*b),
            Out2d::Arc { r, start, end, .. } => *r * (*end - *start),
            Out2d::Ellipse {
                major,
                ratio,
                start,
                end,
                ..
            } => {
                let speed = |t: f64| {
                    let (s, c) = t.sin_cos();
                    let v = *major * (-s) + DVec2::new(-major.y, major.x) * (*ratio * c);
                    v.length()
                };
                let width = *end - *start;
                let panels = ((width / (PI / 8.0)).ceil() as usize).clamp(1, 4096);
                (0..panels)
                    .map(|i| {
                        let a = *start + width * i as f64 / panels as f64;
                        let b = *start + width * (i + 1) as f64 / panels as f64;
                        gauss5(&speed, a, b)
                    })
                    .sum()
            }
            Out2d::Spline {
                degree,
                knots,
                ctrl,
                weights,
            } => {
                let Some(sp) = Spl::new(*degree, knots, ctrl, weights) else {
                    return f64::NAN;
                };
                let speed = |t: f64| sp.deriv(t).length();
                span_edges(*degree, knots)
                    .windows(2)
                    .map(|w| {
                        (0..8)
                            .map(|i| {
                                let a = w[0] + (w[1] - w[0]) * i as f64 / 8.0;
                                let b = w[0] + (w[1] - w[0]) * (i + 1) as f64 / 8.0;
                                gauss5(&speed, a, b)
                            })
                            .sum::<f64>()
                    })
                    .sum()
            }
        }
    }

    /// Hüllbox `(min, max)` der Kurve (Bogen und Ellipse über die Achsextrema, Spline über die
    /// Nullstellen der Ableitung; nicht über die Kontrollpunkte).
    pub fn bounds(&self) -> (DVec2, DVec2) {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        let mut add = |p: DVec2| {
            lo = lo.min(p);
            hi = hi.max(p);
        };
        match self {
            Out2d::Line { a, b } => {
                add(*a);
                add(*b);
            }
            Out2d::Arc { .. } | Out2d::Ellipse { .. } => {
                let Some((s0, sweep)) = self.normalized_range() else {
                    return (DVec2::NAN, DVec2::NAN);
                };
                add(self.point(s0));
                add(self.point(s0 + sweep));
                // Achsextrema: Bogen bei Winkeln k·π/2, Ellipse bei atan2(n, m) + k·π je Achse.
                let extrema: Vec<(f64, f64)> = match self {
                    Out2d::Ellipse { major, ratio, .. } => {
                        let minor = DVec2::new(-major.y, major.x) * *ratio;
                        vec![(minor.x.atan2(major.x), PI), (minor.y.atan2(major.y), PI)]
                    }
                    _ => vec![(0.0, 0.5 * PI)],
                };
                for (t_star, step) in extrema {
                    let k0 = ((s0 - t_star) / step).ceil();
                    for i in 0..5 {
                        let t = t_star + (k0 + f64::from(i)) * step;
                        if t <= s0 + sweep {
                            add(self.point(t));
                        }
                    }
                }
            }
            Out2d::Spline {
                degree,
                knots,
                ctrl,
                weights,
            } => {
                let Some(sp) = Spl::new(*degree, knots, ctrl, weights) else {
                    return (DVec2::NAN, DVec2::NAN);
                };
                let edges = span_edges(*degree, knots);
                const N: usize = 32;
                for w in edges.windows(2) {
                    let ts: Vec<f64> = (0..=N)
                        .map(|i| w[0] + (w[1] - w[0]) * i as f64 / N as f64)
                        .collect();
                    for &t in &ts {
                        add(sp.point(t));
                    }
                    for axis in 0..2 {
                        let f = |t: f64| {
                            let d = sp.deriv(t);
                            if axis == 0 { d.x } else { d.y }
                        };
                        for p in ts.windows(2) {
                            let (mut a, mut b) = (p[0], p[1]);
                            let (fa, fb) = (f(a), f(b));
                            if fa == 0.0 {
                                add(sp.point(a));
                            }
                            if fa * fb < 0.0 {
                                let mut fa = fa;
                                for _ in 0..60 {
                                    let m = 0.5 * (a + b);
                                    let fm = f(m);
                                    if fa * fm <= 0.0 {
                                        b = m;
                                    } else {
                                        a = m;
                                        fa = fm;
                                    }
                                }
                                add(sp.point(0.5 * (a + b)));
                            }
                        }
                    }
                }
                if edges.len() < 2 {
                    for &p in ctrl {
                        add(p);
                    }
                }
            }
        }
        (lo, hi)
    }

    /// Für Bogen und Ellipse: Anfang auf `[0, TAU)` gebracht und Öffnungswinkel `end − start`
    /// (höchstens TAU); für die Schreiber der Ausgabeformate. `None` bei anderen Kurven oder
    /// nicht endlichen Winkeln.
    pub fn normalized_range(&self) -> Option<(f64, f64)> {
        let (start, end) = match self {
            Out2d::Arc { start, end, .. } | Out2d::Ellipse { start, end, .. } => (*start, *end),
            _ => return None,
        };
        if !(start.is_finite() && end.is_finite()) {
            return None;
        }
        let s0 = start.rem_euclid(TAU);
        // rem_euclid kann bei winzigen negativen Werten TAU liefern.
        let s0 = if s0 >= TAU { 0.0 } else { s0 };
        Some((s0, (end - start).clamp(0.0, TAU)))
    }

    /// Sind alle Zahlen der Kurve endlich (bei einem Spline außerdem: Aufbau gültig)?
    pub fn is_finite(&self) -> bool {
        match self {
            Out2d::Line { a, b } => a.is_finite() && b.is_finite(),
            Out2d::Arc { c, r, start, end } => {
                c.is_finite() && r.is_finite() && start.is_finite() && end.is_finite()
            }
            Out2d::Ellipse {
                c,
                major,
                ratio,
                start,
                end,
            } => {
                c.is_finite()
                    && major.is_finite()
                    && ratio.is_finite()
                    && start.is_finite()
                    && end.is_finite()
            }
            Out2d::Spline {
                degree,
                knots,
                ctrl,
                weights,
            } => {
                knots.iter().all(|k| k.is_finite())
                    && ctrl.iter().all(|p| p.is_finite())
                    && weights.iter().all(|w| w.is_finite())
                    && Spl::new(*degree, knots, ctrl, weights).is_some()
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// B-Spline-Werkzeuge in homogenen Koordinaten (w·x, w·y, w)
// ---------------------------------------------------------------------------------------------

/// Einmal je Aufruf aufgebaute Auswertung eines geprüften Splines: homogene Kontrollpunkte
/// und Ableitungskurve.
struct Spl<'a> {
    p: usize,
    knots: &'a [f64],
    q: Vec<H3>,
    dq: Vec<H3>,
}

impl<'a> Spl<'a> {
    /// `None` bei ungültigem Aufbau (Grad 0, zu wenige Kontrollpunkte, Knotenzahl passt nicht,
    /// fallende oder nicht endliche Knoten, Gewicht nicht positiv).
    fn new(p: usize, knots: &'a [f64], ctrl: &[DVec2], weights: &[f64]) -> Option<Spl<'a>> {
        let ok = p >= 1
            && ctrl.len() > p
            && weights.len() == ctrl.len()
            && knots.len() == ctrl.len() + p + 1
            && knots.iter().all(|k| k.is_finite())
            && knots.windows(2).all(|w| w[0] <= w[1])
            && weights.iter().all(|w| w.is_finite() && *w > 0.0);
        if !ok {
            return None;
        }
        let q = homogeneous(ctrl, weights);
        let dq = derivative_curve(p, knots, &q);
        Some(Spl { p, knots, q, dq })
    }

    fn point(&self, t: f64) -> DVec2 {
        let h = eval_h(self.p, self.knots, &self.q, t);
        DVec2::new(h[0] / h[2], h[1] / h[2])
    }

    fn deriv(&self, t: f64) -> DVec2 {
        rational_derivative(self.p, self.knots, &self.q, &self.dq, t)
    }
}

fn homogeneous(ctrl: &[DVec2], weights: &[f64]) -> Vec<H3> {
    ctrl.iter()
        .zip(weights)
        .map(|(p, w)| [p.x * w, p.y * w, *w])
        .collect()
}

/// Index `k` in `p..=n` mit `U[k] ≤ u < U[k+1]` (am rechten Rand der letzte nichtleere Abschnitt).
fn find_span(p: usize, knots: &[f64], n: usize, u: f64) -> usize {
    let mut k = p;
    for i in p..=n {
        if knots[i] < knots[i + 1] {
            k = i;
            if knots[i + 1] > u {
                break;
            }
        }
    }
    k
}

/// `(1 − al)·a + al·b` komponentenweise.
fn lerp_h(a: H3, b: H3, al: f64) -> H3 {
    [
        (1.0 - al) * a[0] + al * b[0],
        (1.0 - al) * a[1] + al * b[1],
        (1.0 - al) * a[2] + al * b[2],
    ]
}

/// de Boor in homogenen Koordinaten; `u` wird auf den Knotenbereich begrenzt.
fn eval_h(p: usize, knots: &[f64], q: &[H3], u: f64) -> H3 {
    let n = q.len() - 1;
    let (a, b) = (knots[p], knots[n + 1]);
    let u = if u.is_finite() { u.clamp(a, b) } else { a };
    let k = find_span(p, knots, n, u);
    let mut d: Vec<H3> = (0..=p).map(|j| q[k - p + j]).collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = k - p + j;
            let den = knots[i + p - r + 1] - knots[i];
            let al = if den > 0.0 { (u - knots[i]) / den } else { 0.0 };
            d[j] = lerp_h(d[j - 1], d[j], al);
        }
    }
    d[p]
}

/// Ableitungskurve (Grad `p − 1`, Knoten ohne Randknoten) einer homogenen B-Spline-Kurve.
fn derivative_curve(p: usize, knots: &[f64], q: &[H3]) -> Vec<H3> {
    (0..q.len() - 1)
        .map(|i| {
            let den = knots[i + p + 1] - knots[i + 1];
            let f = if den > 0.0 { p as f64 / den } else { 0.0 };
            [
                (q[i + 1][0] - q[i][0]) * f,
                (q[i + 1][1] - q[i][1]) * f,
                (q[i + 1][2] - q[i][2]) * f,
            ]
        })
        .collect()
}

/// `C'(t) = (A' − w'·C) / w` aus Kurve und Ableitungskurve.
fn rational_derivative(p: usize, knots: &[f64], q: &[H3], dq: &[H3], t: f64) -> DVec2 {
    let h = eval_h(p, knots, q, t);
    let d = if p == 0 || dq.is_empty() {
        [0.0; 3]
    } else {
        eval_h(p - 1, &knots[1..knots.len() - 1], dq, t)
    };
    let c = DVec2::new(h[0] / h[2], h[1] / h[2]);
    (DVec2::new(d[0], d[1]) - c * d[2]) / h[2]
}

/// Verschiedene Knoten im Parameterbereich (Rändern der nichtleeren Abschnitte).
fn span_edges(p: usize, knots: &[f64]) -> Vec<f64> {
    let (a, b) = (knots[p], knots[knots.len() - p - 1]);
    let mut v: Vec<f64> = Vec::new();
    for &k in knots {
        if k >= a && k <= b && v.last() != Some(&k) {
            v.push(k);
        }
    }
    v
}

/// Fügt den inneren Knoten `u` einmal ein (Boehm, homogene Koordinaten).
fn insert_knot(p: usize, knots: &mut Vec<f64>, q: &mut Vec<H3>, u: f64) {
    let n = q.len() - 1;
    let k = find_span(p, knots, n, u);
    let s = knots.iter().filter(|&&x| x == u).count();
    if s > p || k < p || k + 1 > n + p + 1 {
        return;
    }
    let mut nq: Vec<H3> = Vec::with_capacity(q.len() + 1);
    for i in 0..=n + 1 {
        if i <= k - p {
            nq.push(q[i]);
        } else if i <= k - s {
            let den = knots[i + p] - knots[i];
            let al = if den > 0.0 { (u - knots[i]) / den } else { 0.0 };
            let h = lerp_h(q[i - 1], q[i], al);
            nq.push(h);
        } else {
            nq.push(q[i - 1]);
        }
    }
    *q = nq;
    knots.insert(k + 1, u);
}

/// Schneidet `[t0, t1]` (innerhalb des Bereichs, `t0 < t1`) aus der homogenen Kurve; Ergebnis mit
/// Randknoten der Vielfachheit `p + 1`.
fn cut_spline(p: usize, knots: &[f64], q: &[H3], t0: f64, t1: f64) -> (Vec<f64>, Vec<H3>) {
    let (a, b) = (knots[0], knots[knots.len() - 1]);
    let mut kn = knots.to_vec();
    let mut qq = q.to_vec();
    for u in [t0, t1] {
        if u > a && u < b {
            loop {
                let s = kn.iter().filter(|&&x| x == u).count();
                if s > p {
                    break;
                }
                let before = kn.len();
                insert_knot(p, &mut kn, &mut qq, u);
                if kn.len() == before {
                    break;
                }
            }
        }
    }
    // Erster Knoten ≥ t0 und letzter Knoten ≤ t1 (mit Vielfachheit p + 1 an beiden Enden).
    let i0 = kn.iter().position(|&x| x >= t0).unwrap_or(0);
    let last = kn.iter().rposition(|&x| x <= t1).unwrap_or(kn.len() - 1);
    let len = last + 1 - i0;
    let count = len.saturating_sub(p + 1);
    (kn[i0..=last].to_vec(), qq[i0..i0 + count].to_vec())
}

/// Teilstück `[t0, t1]` (innerhalb des Bereichs); `None`, wenn es leer ist.
fn spline_out(
    c: &NurbsCurve,
    basis: &Basis,
    t0: f64,
    t1: f64,
    xf: &SheetTransform,
) -> Option<Out2d> {
    let p = c.degree();
    let proj = |v: DVec3| basis.project(v);
    let (a, b) = c.domain();
    let (lo, hi) = (t0.min(t1).clamp(a, b), t0.max(t1).clamp(a, b));
    let ctrl2: Vec<DVec2> = c.control_points().iter().map(|&v| proj(v)).collect();
    let q = homogeneous(&ctrl2, c.weights());
    if hi <= lo {
        return None;
    }
    let (kn, qq) = cut_spline(p, c.knots().knots(), &q, lo, hi);
    let mut ctrl = Vec::with_capacity(qq.len());
    let mut weights = Vec::with_capacity(qq.len());
    for h in &qq {
        ctrl.push(xf.apply(DVec2::new(h[0] / h[2], h[1] / h[2])));
        weights.push(h[2]);
    }
    Some(Out2d::Spline {
        degree: p,
        knots: kn,
        ctrl,
        weights,
    })
}

// ---------------------------------------------------------------------------------------------
// Kegelschnitte
// ---------------------------------------------------------------------------------------------

/// Kreis oder Ellipse `P(t) = c + A cos t + B sin t` (Bildebene), Parameterbereich `[t0, t1]`,
/// in Hauptachsenform; Details siehe `piece_to_out`.
fn conic_out(c: DVec2, a: DVec2, b: DVec2, t0: f64, t1: f64, xf: &SheetTransform) -> Option<Out2d> {
    let lo = t0.min(t1);
    let hi = lo + (t0.max(t1) - lo).min(TAU);
    // Hauptachsen: Singulärwertzerlegung von [A B].
    let dot = a.dot(b);
    let diff = a.length_squared() - b.length_squared();
    let theta = 0.5 * (2.0 * dot).atan2(diff);
    let (st, ct) = theta.sin_cos();
    let major = a * ct + b * st;
    let minor_s = b * ct - a * st;
    let (s1, s2) = (major.length(), minor_s.length());
    let det = a.x * b.y - a.y * b.x;
    if !(s1.is_finite() && s2.is_finite() && det.is_finite()) {
        return None;
    }
    if s1 <= 0.0 {
        let p = xf.apply(c);
        return Some(Out2d::Line { a: p, b: p });
    }
    let ratio = (s2 / s1).min(1.0);
    // Parameter der Hauptachsendarstellung: s = t − θ (Orientierung erhalten) bzw. u = θ − t
    // (gespiegelt: Umlaufrichtung umgekehrt, damit gegen den Uhrzeigersinn).
    let mirrored = det < 0.0;
    let (start, end) = if mirrored {
        (theta - hi, theta - lo)
    } else {
        (lo - theta, hi - theta)
    };
    if ratio >= 1.0 - RATIO_EPS {
        let phi = major.y.atan2(major.x);
        let (s, e) = (phi + start, phi + end);
        return Some(Out2d::Arc {
            c: xf.apply(c),
            r: s1 * xf.scale,
            start: s,
            end: if hi - lo >= TAU { s + TAU } else { e },
        });
    }
    if ratio <= RATIO_EPS {
        // Strecke: Punkte `c + major·cos s` für s im Bereich; Umkehrpunkte (cos s = ±1) werden
        // aus den exakten Extremen genommen, nicht aus den Endpunkten allein.
        let (sa, sb) = (lo - theta, hi - theta);
        let (k0, k1) = ((sa / PI).ceil(), (sb / PI).floor());
        let folded = k0 <= k1;
        let (cmin, cmax) = if folded {
            let mut cmin = sa.cos().min(sb.cos());
            let mut cmax = sa.cos().max(sb.cos());
            for k in [k0, (k0 + 1.0).min(k1)] {
                if k <= k1 {
                    if k.rem_euclid(2.0) == 0.0 {
                        cmax = 1.0;
                    } else {
                        cmin = -1.0;
                    }
                }
            }
            (cmin, cmax)
        } else {
            (sa.cos().min(sb.cos()), sa.cos().max(sb.cos()))
        };
        let (pa, pb) = if folded {
            (c + major * cmin, c + major * cmax)
        } else {
            (c + major * sa.cos(), c + major * sb.cos())
        };
        return Some(Out2d::Line {
            a: xf.apply(pa),
            b: xf.apply(pb),
        });
    }
    Some(Out2d::Ellipse {
        c: xf.apply(c),
        major: major * xf.scale,
        ratio,
        start,
        end,
    })
}

/// Wandelt das Stück `[t0, t1]` des Strichs in Ausgabekurven in Blattkoordinaten
/// (`Blatt = scale·(p − bbox_center) + (tx, ty)`); meist genau eine, bei einem Stück über die
/// Naht einer geschlossenen Freiformkurve zwei.
///
/// * Gerade → `Line` (projizierte Endpunkte).
/// * Kreis und Ellipse: projizierte konjugierte Halbmesser `A`, `B` (`P(t) = c + A cos t + B sin t`),
///   Hauptachsen über die Singulärwertzerlegung von `[A B]`; `Arc` bei Verhältnis ≥ 1 − 1e-9,
///   `Line` (Strecke zwischen den Extrempunkten des gezeichneten Stücks) bei Verhältnis ≤ 1e-9,
///   sonst `Ellipse`. Bei negativer Determinante (gespiegelte Abbildung) werden Parameterbereich
///   und Richtung so gewendet, dass `start < end` und gegen den Uhrzeigersinn gilt.
/// * Freiformkurve → `Spline`, Teilbereich durch Knoteneinfügen exakt zugeschnitten. Reicht das
///   Stück einer geschlossenen Kurve über das Bereichsende hinaus (so fasst `compute_view` das
///   erste und letzte Stück zusammen), wird der Parameter um die Periode gefaltet und das Stück
///   in `[t0, Ende]` und `[Anfang, t1 − Periode]` geteilt. Ein Stück, das länger als eine
///   Periode ist, kommt aus `compute_view` nicht vor und ergibt keine Kurve.
///
/// Nicht endliche Eingaben ergeben eine leere Liste (nichts zu zeichnen, kein Absturz).
pub fn piece_to_outs(
    stroke: &Stroke,
    basis: &Basis,
    t0: f64,
    t1: f64,
    xf: &SheetTransform,
) -> Vec<Out2d> {
    if !(t0.is_finite() && t1.is_finite() && xf.scale.is_finite() && xf.bbox_center.is_finite()) {
        return Vec::new();
    }
    let outs: Vec<Out2d> = match &stroke.curve {
        Curve::Line { .. } => vec![Out2d::Line {
            a: xf.apply(stroke.point2(basis, t0)),
            b: xf.apply(stroke.point2(basis, t1)),
        }],
        Curve::Circle { frame, radius } => {
            let c = basis.project(frame.origin);
            let a = DVec2::new(
                (frame.x * *radius).dot(basis.ex),
                (frame.x * *radius).dot(basis.ey),
            );
            let b = DVec2::new(
                (frame.y * *radius).dot(basis.ex),
                (frame.y * *radius).dot(basis.ey),
            );
            conic_out(c, a, b, t0, t1, xf).into_iter().collect()
        }
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => {
            let c = basis.project(frame.origin);
            let (u, v) = (frame.x * *major, frame.y * *minor);
            let a = DVec2::new(u.dot(basis.ex), u.dot(basis.ey));
            let b = DVec2::new(v.dot(basis.ex), v.dot(basis.ey));
            conic_out(c, a, b, t0, t1, xf).into_iter().collect()
        }
        Curve::Nurbs(c) => {
            let (a, b) = c.domain();
            let (lo, hi) = (t0.min(t1), t0.max(t1));
            let ranges: Vec<(f64, f64)> = match stroke.curve.period() {
                Some(per) if per > 0.0 && (lo < a || hi > b) => {
                    let width = hi - lo;
                    if width > per * (1.0 + 1e-12) {
                        Vec::new()
                    } else {
                        let mut l = a + (lo - a).rem_euclid(per);
                        let mut h = l + width;
                        if l >= b {
                            l -= per;
                            h -= per;
                        }
                        if h > b {
                            vec![(l, b), (a, h - per)]
                        } else {
                            vec![(l, h)]
                        }
                    }
                }
                _ => vec![(lo.clamp(a, b), hi.clamp(a, b))],
            };
            ranges
                .into_iter()
                .filter_map(|(l, h)| spline_out(c, basis, l, h, xf))
                .collect()
        }
    };
    outs.into_iter().filter(Out2d::is_finite).collect()
}
