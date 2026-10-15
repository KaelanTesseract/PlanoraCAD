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
    Arc {
        c: DVec2,
        r: f64,
        start: f64,
        end: f64,
    },
    /// Ellipse mit Hauptachse `major` (Vektor vom Mittelpunkt zum Hauptscheitel), Nebenachse `ratio·|major|`, Parameter gegen den Uhrzeigersinn (Radiant).
    Ellipse {
        c: DVec2,
        major: DVec2,
        ratio: f64,
        start: f64,
        end: f64,
    },
    /// Rationale B-Spline-Kurve (Grad, Knoten, Kontrollpunkte, Gewichte), Parameterbereich = Knotenbereich.
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
            } => {
                let q = homogeneous(ctrl, weights);
                let h = eval_h(*degree, knots, &q, t);
                DVec2::new(h[0] / h[2], h[1] / h[2])
            }
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
                let q = homogeneous(ctrl, weights);
                let dq = derivative_curve(*degree, knots, &q);
                let speed = |t: f64| rational_derivative(*degree, knots, &q, &dq, t).length();
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
            Out2d::Arc { start, end, .. } => {
                add(self.point(*start));
                add(self.point(*end));
                // Achsextrema bei Winkeln k·π/2.
                let (k0, k1) = (
                    (*start / (0.5 * PI)).ceil() as i64,
                    (*end / (0.5 * PI)).floor() as i64,
                );
                for k in k0..=k1.min(k0 + 4) {
                    add(self.point(k as f64 * 0.5 * PI));
                }
            }
            Out2d::Ellipse {
                major,
                ratio,
                start,
                end,
                ..
            } => {
                add(self.point(*start));
                add(self.point(*end));
                let minor = DVec2::new(-major.y, major.x) * *ratio;
                for (m, n) in [(major.x, minor.x), (major.y, minor.y)] {
                    // d/dt (m cos t + n sin t) = 0 bei t = atan2(n, m) + k·π.
                    let t_star = n.atan2(m);
                    let k0 = ((*start - t_star) / PI).ceil() as i64;
                    let k1 = ((*end - t_star) / PI).floor() as i64;
                    for k in k0..=k1.min(k0 + 2) {
                        add(self.point(t_star + k as f64 * PI));
                    }
                }
            }
            Out2d::Spline {
                degree,
                knots,
                ctrl,
                weights,
            } => {
                let q = homogeneous(ctrl, weights);
                let dq = derivative_curve(*degree, knots, &q);
                let edges = span_edges(*degree, knots);
                const N: usize = 32;
                for w in edges.windows(2) {
                    let ts: Vec<f64> = (0..=N)
                        .map(|i| w[0] + (w[1] - w[0]) * i as f64 / N as f64)
                        .collect();
                    for &t in &ts {
                        add(self.point(t));
                    }
                    for axis in 0..2 {
                        let f = |t: f64| {
                            let d = rational_derivative(*degree, knots, &q, &dq, t);
                            if axis == 0 { d.x } else { d.y }
                        };
                        for p in ts.windows(2) {
                            let (mut a, mut b) = (p[0], p[1]);
                            let (fa, fb) = (f(a), f(b));
                            if fa == 0.0 {
                                add(self.point(a));
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
                                add(self.point(0.5 * (a + b)));
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

    /// Sind alle Zahlen der Kurve endlich?
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
                knots,
                ctrl,
                weights,
                ..
            } => {
                knots.iter().all(|k| k.is_finite())
                    && ctrl.iter().all(|p| p.is_finite())
                    && weights.iter().all(|w| w.is_finite())
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// B-Spline-Werkzeuge in homogenen Koordinaten (w·x, w·y, w)
// ---------------------------------------------------------------------------------------------

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

fn spline_out(c: &NurbsCurve, basis: &Basis, t0: f64, t1: f64, xf: &SheetTransform) -> Out2d {
    let p = c.degree();
    let proj = |v: DVec3| basis.project(v);
    let (a, b) = c.domain();
    let (lo, hi) = (t0.min(t1).clamp(a, b), t0.max(t1).clamp(a, b));
    let ctrl2: Vec<DVec2> = c.control_points().iter().map(|&v| proj(v)).collect();
    let q = homogeneous(&ctrl2, c.weights());
    if hi <= lo {
        let pt = xf.apply(proj(c.point(lo)));
        return Out2d::Line { a: pt, b: pt };
    }
    let (kn, qq) = cut_spline(p, c.knots().knots(), &q, lo, hi);
    let mut ctrl = Vec::with_capacity(qq.len());
    let mut weights = Vec::with_capacity(qq.len());
    for h in &qq {
        ctrl.push(xf.apply(DVec2::new(h[0] / h[2], h[1] / h[2])));
        weights.push(h[2]);
    }
    Out2d::Spline {
        degree: p,
        knots: kn,
        ctrl,
        weights,
    }
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

/// Wandelt das Stück `[t0, t1]` des Strichs in eine Ausgabekurve in Blattkoordinaten
/// (`Blatt = scale·(p − bbox_center) + (tx, ty)`).
///
/// * Gerade → `Line` (projizierte Endpunkte).
/// * Kreis und Ellipse: projizierte konjugierte Halbmesser `A`, `B` (`P(t) = c + A cos t + B sin t`),
///   Hauptachsen über die Singulärwertzerlegung von `[A B]`; `Arc` bei Verhältnis ≥ 1 − 1e-9,
///   `Line` (Strecke zwischen den Extrempunkten des gezeichneten Stücks) bei Verhältnis ≤ 1e-9,
///   sonst `Ellipse`. Bei negativer Determinante (gespiegelte Abbildung) werden Parameterbereich
///   und Richtung so gewendet, dass `start < end` und gegen den Uhrzeigersinn gilt.
/// * Freiformkurve → `Spline`, Teilbereich durch Knoteneinfügen exakt zugeschnitten.
///
/// Nicht endliche Eingaben ergeben eine Nullstrecke im Ursprung (kein Absturz).
pub fn piece_to_out(
    stroke: &Stroke,
    basis: &Basis,
    t0: f64,
    t1: f64,
    xf: &SheetTransform,
) -> Out2d {
    try_piece_to_out(stroke, basis, t0, t1, xf).unwrap_or(Out2d::Line {
        a: DVec2::ZERO,
        b: DVec2::ZERO,
    })
}

/// Wie [`piece_to_out`], aber `None` statt der Nullstrecke, wenn Eingaben oder Ergebnis nicht
/// endlich sind.
pub fn try_piece_to_out(
    stroke: &Stroke,
    basis: &Basis,
    t0: f64,
    t1: f64,
    xf: &SheetTransform,
) -> Option<Out2d> {
    if !(t0.is_finite() && t1.is_finite() && xf.scale.is_finite() && xf.bbox_center.is_finite()) {
        return None;
    }
    let out = match &stroke.curve {
        Curve::Line { .. } => Some(Out2d::Line {
            a: xf.apply(stroke.point2(basis, t0)),
            b: xf.apply(stroke.point2(basis, t1)),
        }),
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
            conic_out(c, a, b, t0, t1, xf)
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
            conic_out(c, a, b, t0, t1, xf)
        }
        Curve::Nurbs(c) => Some(spline_out(c, basis, t0, t1, xf)),
    }?;
    out.is_finite().then_some(out)
}
