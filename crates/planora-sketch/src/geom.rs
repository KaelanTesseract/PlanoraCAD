//! Kurvengeometrie der Skizzenkurven in Gleitkommazahlen: Auswertung, Parameterbereich,
//! Krümmung, Länge, nächster Punkt und Linienzug. Grundlage für Schnittpunkte
//! (`intersect.rs`), Profile, Stutzen, Dehnen und Brechen.
//!
//! Parameter je Art: Linie `t ∈ [0, 1]` von `a` nach `b`; Kreis Winkel `t ∈ [0, 2π)` ab der
//! x-Achse; Bogen Winkel `t ∈ [a0, a0 + sweep]` (gegen den Uhrzeigersinn); Ellipse
//! `t ∈ [0, 2π)` auf `c + a·cos t + b·sin t`; Ellipsenbogen `t ∈ [t0, t0 + sweep]` auf
//! derselben Form; Spline `t ∈ [0, n − 1]` (Stützpunkt `i` bei `t = i`).

use crate::curves::Sampled;
use crate::model::{CurveId, CurveKind, Sketch};
use planora_math::{DVec2, TAU};

/// Abtastschritte je Vollkreis bei der Suche nach dem nächsten Punkt (Ellipsen).
const NEAREST_STEPS_PER_TURN: usize = 256;
/// Abtastschritte je Spline-Segment bei der Suche nach dem nächsten Punkt.
const NEAREST_STEPS_PER_SEGMENT: usize = 64;
/// Schritte des Goldenen Schnitts beim Nachschärfen des nächsten Punkts (Intervall schrumpft
/// auf 0,618⁸⁰ ≈ 2e-17 seiner Breite).
const GOLDEN_STEPS: usize = 80;
/// Newton-Schritte nach dem Goldenen Schnitt.
const NEWTON_STEPS: usize = 4;
/// Simpson-Abschnitte je Vollkreis bzw. je Spline-Segment für Längen.
const LENGTH_STEPS_PER_TURN: f64 = 128.0;
const LENGTH_STEPS_PER_SEGMENT: f64 = 32.0;

/// Eine Skizzenkurve als reine Geometrie (siehe Modulbeschreibung für die Parameter).
#[derive(Clone, Debug)]
pub(crate) enum Geom {
    Line {
        a: DVec2,
        b: DVec2,
    },
    Circle {
        c: DVec2,
        r: f64,
    },
    Arc {
        c: DVec2,
        r: f64,
        a0: f64,
        sweep: f64,
    },
    /// `a` große Halbachse (Vektor), `b = b̂·minor`.
    Ellipse {
        c: DVec2,
        a: DVec2,
        b: DVec2,
    },
    EllipticArc {
        c: DVec2,
        a: DVec2,
        b: DVec2,
        t0: f64,
        sweep: f64,
    },
    /// `n` Stützpunkte (mindestens 2).
    Spline {
        s: Sampled,
        n: usize,
    },
}

impl Geom {
    /// Punkt und Ableitung nach `t`. Außerhalb des Bereichs wird die Kurve fortgesetzt
    /// (Gerade, ganzer Kreis bzw. ganze Ellipse, Spline über sein erstes bzw. letztes Segment).
    pub(crate) fn eval(&self, t: f64) -> (DVec2, DVec2) {
        match self {
            Geom::Line { a, b } => (*a + (*b - *a) * t, *b - *a),
            Geom::Circle { c, r } | Geom::Arc { c, r, .. } => {
                let (s, co) = t.sin_cos();
                (*c + DVec2::new(co, s) * *r, DVec2::new(-s, co) * *r)
            }
            Geom::Ellipse { c, a, b } | Geom::EllipticArc { c, a, b, .. } => {
                let (s, co) = t.sin_cos();
                (*c + *a * co + *b * s, *b * co - *a * s)
            }
            Geom::Spline { s, n } => {
                // wie `Sampled::eval`, aber ohne Dual-Zahlen (hier zählt die Geschwindigkeit)
                let Some((u, [y0, y1, m0, m1])) = spline_segment(s, *n, t) else {
                    return s.eval(t);
                };
                let w = 1.0 - u;
                let p =
                    y0 * w + y1 * u + m0 * ((w * w * w - w) / 6.0) + m1 * ((u * u * u - u) / 6.0);
                let d =
                    y1 - y0 - m0 * ((3.0 * w * w - 1.0) / 6.0) + m1 * ((3.0 * u * u - 1.0) / 6.0);
                (p, d)
            }
        }
    }

    /// Zweite Ableitung nach `t`.
    fn second(&self, t: f64) -> DVec2 {
        match self {
            Geom::Line { .. } => DVec2::ZERO,
            Geom::Circle { r, .. } | Geom::Arc { r, .. } => {
                let (s, co) = t.sin_cos();
                -DVec2::new(co, s) * *r
            }
            Geom::Ellipse { a, b, .. } | Geom::EllipticArc { a, b, .. } => {
                let (s, co) = t.sin_cos();
                -(*a * co + *b * s)
            }
            Geom::Spline { s, n } => {
                // linear zwischen den zweiten Ableitungen der Segmentenden
                spline_segment(s, *n, t)
                    .map_or(DVec2::ZERO, |(u, [_, _, m0, m1])| m0 * (1.0 - u) + m1 * u)
            }
        }
    }

    /// Parameterbereich `(lo, hi)`; bei periodischen Kurven ist `hi` gleich `lo` + Periode.
    pub(crate) fn domain(&self) -> (f64, f64) {
        match self {
            Geom::Line { .. } => (0.0, 1.0),
            Geom::Circle { .. } | Geom::Ellipse { .. } => (0.0, TAU),
            Geom::Arc { a0, sweep, .. } => (*a0, a0 + sweep),
            Geom::EllipticArc { t0, sweep, .. } => (*t0, t0 + sweep),
            Geom::Spline { n, .. } => (0.0, (n - 1) as f64),
        }
    }

    /// Geschlossene Kurve (Kreis, Ellipse) mit Periode 2π.
    pub(crate) fn periodic(&self) -> bool {
        matches!(self, Geom::Circle { .. } | Geom::Ellipse { .. })
    }

    /// Parameter in den Bereich gebracht: periodisch nach `[0, 2π)`, sonst geklemmt.
    pub(crate) fn normalize(&self, t: f64) -> f64 {
        let (lo, hi) = self.domain();
        if self.periodic() {
            let w = t.rem_euclid(TAU);
            // `rem_euclid` kann für winzige negative Werte genau 2π liefern.
            if w >= TAU { 0.0 } else { w }
        } else {
            t.clamp(lo, hi)
        }
    }

    /// Vorzeichenbehaftete Krümmung (positiv bei Linksdrehung); 0, wo die Ableitung verschwindet.
    #[allow(dead_code, reason = "used by profiles, trim and extend (later tasks)")]
    pub(crate) fn curvature(&self, t: f64) -> f64 {
        let (_, d) = self.eval(t);
        let len = d.length();
        if len.is_nan() || len <= 0.0 {
            return 0.0;
        }
        let k = d.perp_dot(self.second(t)) / (len * len * len);
        if k.is_finite() { k } else { 0.0 }
    }

    /// Länge der ganzen Kurve.
    #[allow(dead_code, reason = "used by profiles, trim and extend (later tasks)")]
    pub(crate) fn length(&self) -> f64 {
        let (lo, hi) = self.domain();
        self.arc_length(lo, hi)
    }

    /// Bogenlänge zwischen den Parametern `t0` und `t1` (Reihenfolge beliebig, ohne Umlauf).
    pub(crate) fn arc_length(&self, t0: f64, t1: f64) -> f64 {
        let (lo, hi) = if t0 <= t1 { (t0, t1) } else { (t1, t0) };
        let span = hi - lo;
        if span.is_nan() || span <= 0.0 {
            return 0.0;
        }
        match self {
            Geom::Line { a, b } => a.distance(*b) * span,
            Geom::Circle { r, .. } | Geom::Arc { r, .. } => r * span,
            Geom::Ellipse { .. } | Geom::EllipticArc { .. } | Geom::Spline { .. } => {
                let per_unit = if matches!(self, Geom::Spline { .. }) {
                    LENGTH_STEPS_PER_SEGMENT
                } else {
                    LENGTH_STEPS_PER_TURN / TAU
                };
                // Simpson mit gerader Abschnittszahl
                let k = ((span * per_unit / 2.0).ceil() as usize).clamp(1, 1 << 16) * 2;
                let h = span / k as f64;
                let speed = |t: f64| self.eval(t).1.length();
                let mut sum = speed(lo) + speed(hi);
                for i in 1..k {
                    let w = if i % 2 == 1 { 4.0 } else { 2.0 };
                    sum += w * speed(lo + h * i as f64);
                }
                sum * h / 3.0
            }
        }
    }

    /// Parameter des Kurvenpunkts, der `p` am nächsten liegt (im Bereich; bei periodischen
    /// Kurven in `[0, 2π)`).
    pub(crate) fn nearest(&self, p: DVec2) -> f64 {
        let (lo, hi) = self.domain();
        if !p.is_finite() {
            return lo;
        }
        match self {
            Geom::Line { a, b } => {
                let d = *b - *a;
                let len2 = d.length_squared();
                if len2 > 0.0 {
                    ((p - *a).dot(d) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
            Geom::Circle { c, .. } => {
                let v = p - *c;
                if v == DVec2::ZERO {
                    0.0
                } else {
                    self.normalize(v.y.atan2(v.x))
                }
            }
            Geom::Arc { c, a0, sweep, .. } => {
                let v = p - *c;
                if v == DVec2::ZERO {
                    return *a0;
                }
                let rel = (v.y.atan2(v.x) - a0).rem_euclid(TAU);
                if rel <= *sweep {
                    a0 + rel
                } else if self.eval(lo).0.distance_squared(p) <= self.eval(hi).0.distance_squared(p)
                {
                    lo
                } else {
                    hi
                }
            }
            Geom::Ellipse { .. } | Geom::EllipticArc { .. } | Geom::Spline { .. } => {
                let steps = match self {
                    Geom::Spline { n, .. } => (n - 1) * NEAREST_STEPS_PER_SEGMENT,
                    _ => steps_for(hi - lo, NEAREST_STEPS_PER_TURN),
                };
                let ts: Vec<f64> = (0..=steps)
                    .map(|k| lo + (hi - lo) * k as f64 / steps as f64)
                    .collect();
                let dist = |t: f64| self.eval(t).0.distance_squared(p);
                let k = ts
                    .iter()
                    .enumerate()
                    .min_by(|x, y| dist(*x.1).total_cmp(&dist(*y.1)))
                    .map_or(0, |(k, _)| k);
                let a = ts[k.saturating_sub(1)];
                let b = ts[(k + 1).min(steps)];
                let mut t = golden_min(a, b, dist);
                // Newton auf (C − p)·C' schärft nach (der Goldene Schnitt allein trifft die
                // flache Abstandsfunktion nur auf etwa √ε genau)
                let slope = |t: f64| {
                    let (q, d) = self.eval(t);
                    (q - p).dot(d)
                };
                for _ in 0..NEWTON_STEPS {
                    let (q, d) = self.eval(t);
                    let curv = d.length_squared() + (q - p).dot(self.second(t));
                    let next = t - slope(t) / curv;
                    let better = slope(next).abs() < slope(t).abs();
                    if !(curv > 0.0 && (a..=b).contains(&next) && better) {
                        break;
                    }
                    t = next;
                }
                let best = if dist(t) <= dist(ts[k]) { t } else { ts[k] };
                self.normalize(best)
            }
        }
    }

    /// Parameter des Linienzugs von `t0` nach `t1`: Linie ein Abschnitt; Kreise, Bögen und
    /// Ellipsen `steps_per_turn` Abschnitte je Vollkreis; Splines `steps_per_turn / 4` je
    /// Segment (wie [`Sketch::curve_polyline`] mit 16 je Segment bei 64 je Vollkreis).
    pub(crate) fn polyline_params(&self, t0: f64, t1: f64, steps_per_turn: usize) -> Vec<f64> {
        let span = t1 - t0;
        let steps = if !span.is_finite() {
            1
        } else {
            match self {
                Geom::Line { .. } => 1,
                Geom::Spline { .. } => {
                    let per_seg = (steps_per_turn / 4).max(1) as f64;
                    ((span.abs() * per_seg).ceil() as usize).clamp(1, 1 << 16)
                }
                _ => steps_for(span, steps_per_turn),
            }
        };
        (0..=steps)
            .map(|k| {
                if k == steps {
                    t1
                } else {
                    t0 + span * k as f64 / steps as f64
                }
            })
            .collect()
    }

    /// Linienzug von `t0` nach `t1` (Teilung siehe [`Geom::polyline_params`]).
    #[allow(dead_code, reason = "used by profiles, trim and extend (later tasks)")]
    pub(crate) fn polyline(&self, t0: f64, t1: f64, steps_per_turn: usize) -> Vec<DVec2> {
        self.polyline_params(t0, t1, steps_per_turn)
            .into_iter()
            .map(|t| self.eval(t).0)
            .collect()
    }

    /// Achsenparallele Hülle `(min, max)`, die die ganze Kurve sicher enthält (Bögen exakt,
    /// Ellipsenbögen über ihre ganze Ellipse, Splines über die Bézier-Kontrollpunkte).
    pub(crate) fn bbox(&self) -> (DVec2, DVec2) {
        let fold = |pts: &mut dyn Iterator<Item = DVec2>| {
            pts.fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p), hi.max(p))
            })
        };
        match self {
            Geom::Line { a, b } => (a.min(*b), a.max(*b)),
            Geom::Circle { c, r } => (*c - DVec2::splat(*r), *c + DVec2::splat(*r)),
            Geom::Arc { a0, sweep, .. } => {
                // Enden und alle Achsrichtungen im Winkelbereich
                let first = (a0 / (TAU / 4.0)).ceil() as i64;
                let quarters = (0..5)
                    .map(|k| (first + k) as f64 * TAU / 4.0)
                    .filter(|q| *q <= a0 + sweep);
                let ends = [*a0, a0 + sweep];
                fold(&mut ends.into_iter().chain(quarters).map(|t| self.eval(t).0))
            }
            Geom::Ellipse { c, a, b } | Geom::EllipticArc { c, a, b, .. } => {
                let half = DVec2::new(a.x.hypot(b.x), a.y.hypot(b.y));
                (*c - half, *c + half)
            }
            Geom::Spline { n, .. } => {
                let mut pts = (0..n - 1).flat_map(|i| {
                    let (p0, d0) = self.eval(i as f64);
                    // Ableitung am Segmentende aus demselben Segment (der Spline ist C¹)
                    let (p3, d1) = self.eval((i + 1) as f64);
                    [p0, p0 + d0 / 3.0, p3 - d1 / 3.0, p3]
                });
                fold(&mut pts)
            }
        }
    }
}

/// Segment des Splines bei `t` wie in `curves::spline_eval` (außerhalb erstes bzw. letztes
/// Segment): lokaler Parameter `u` und `[y_i, y_i+1, M_i, M_i+1]`.
fn spline_segment(s: &Sampled, n: usize, t: f64) -> Option<(f64, [DVec2; 4])> {
    let Sampled::Spline { y, m } = s else {
        return None;
    };
    // `NaN.max(0.0)` ist 0; zu große Werte sättigen beim Umwandeln und werden begrenzt.
    let i = (t.floor().max(0.0) as usize).min(n - 2);
    let at = |v: &[Vec<crate::dual::Dual>; 2], k: usize| DVec2::new(v[0][k].v, v[1][k].v);
    Some((
        t - i as f64,
        [at(y, i), at(y, i + 1), at(m, i), at(m, i + 1)],
    ))
}

/// Abschnitte für den Parameterbereich `span` bei `per_turn` Abschnitten je 2π (mindestens 1).
fn steps_for(span: f64, per_turn: usize) -> usize {
    ((span.abs() / TAU * per_turn as f64).ceil() as usize).clamp(1, 1 << 16)
}

/// Kleinste Stelle von `f` in `[a, b]` per Goldenem Schnitt (lokales Minimum).
fn golden_min(mut a: f64, mut b: f64, f: impl Fn(f64) -> f64) -> f64 {
    const R: f64 = 0.618_033_988_749_894_8;
    let mut x1 = b - R * (b - a);
    let mut x2 = a + R * (b - a);
    let (mut f1, mut f2) = (f(x1), f(x2));
    for _ in 0..GOLDEN_STEPS {
        if f1 <= f2 {
            b = x2;
            x2 = x1;
            f2 = f1;
            x1 = b - R * (b - a);
            f1 = f(x1);
        } else {
            a = x1;
            x1 = x2;
            f1 = f2;
            x2 = a + R * (b - a);
            f2 = f(x2);
        }
    }
    0.5 * (a + b)
}

impl Sketch {
    /// Geometrie einer lebenden Kurve; `None` für unbekannte Kurven sowie Kreise und Bögen
    /// ohne Radius und Ellipsen ohne Halbachsen. Bögen und Ellipsenbögen, deren Ende in
    /// Richtung des Starts liegt, laufen einmal ganz herum (wie [`Sketch::curve_polyline`]).
    pub(crate) fn geom(&self, c: CurveId) -> Option<Geom> {
        let angular = self.tolerance().angular;
        let sweep_of = |t0: f64, t1: f64| {
            let s = (t1 - t0).rem_euclid(TAU);
            if s <= angular || s >= TAU { TAU } else { s }
        };
        let positive = |v: f64| v.is_finite() && v > 0.0;
        let g = match &self.curve(c)?.kind {
            CurveKind::Line { a, b } => Geom::Line {
                a: self.pos(*a),
                b: self.pos(*b),
            },
            CurveKind::Circle { center, radius } => {
                let r = self.vars[*radius];
                if !positive(r) {
                    return None;
                }
                Geom::Circle {
                    c: self.pos(*center),
                    r,
                }
            }
            CurveKind::Arc { center, start, end } => {
                let (m, s, e) = (self.pos(*center), self.pos(*start), self.pos(*end));
                let r = s.distance(m);
                if !positive(r) {
                    return None;
                }
                let a0 = (s - m).y.atan2((s - m).x);
                let a1 = (e - m).y.atan2((e - m).x);
                Geom::Arc {
                    c: m,
                    r,
                    a0,
                    sweep: sweep_of(a0, a1),
                }
            }
            CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => {
                let (m, a, minor) = self.ellipse_axes(c)?;
                let len = a.length();
                if !(positive(len) && positive(minor)) {
                    return None;
                }
                let b = a.perp() / len * minor;
                if let CurveKind::EllipticArc { start, end, .. } = self.curves[c].kind {
                    let t0 = self.ellipse_param_of(c, self.pos(start))?;
                    let t1 = self.ellipse_param_of(c, self.pos(end))?;
                    Geom::EllipticArc {
                        c: m,
                        a,
                        b,
                        t0,
                        sweep: sweep_of(t0, t1),
                    }
                } else {
                    Geom::Ellipse { c: m, a, b }
                }
            }
            CurveKind::Spline { points } => {
                let xy: Vec<DVec2> = points.iter().map(|&p| self.pos(p)).collect();
                Geom::Spline {
                    s: Sampled::spline(&xy),
                    n: xy.len(),
                }
            }
        };
        Some(g)
    }

    /// Punkt der lebenden Kurve `c` beim Parameter `t` (Parameter je Art: Linie `[0, 1]`,
    /// Kreis und Bogen Winkel ab der x-Achse, Ellipse und Ellipsenbogen der Parameter von
    /// [`Sketch::ellipse_point`], Spline `[0, n − 1]`). Außerhalb von [`Sketch::curve_domain`]
    /// wird die Kurve fortgesetzt. `None` für unbekannte oder entartete Kurven und nicht
    /// endliches `t`.
    pub fn curve_eval(&self, c: CurveId, t: f64) -> Option<DVec2> {
        if !t.is_finite() {
            return None;
        }
        let p = self.geom(c)?.eval(t).0;
        p.is_finite().then_some(p)
    }

    /// Parameterbereich `(lo, hi)` der lebenden Kurve `c` (siehe [`Sketch::curve_eval`]); bei
    /// Kreis und Ellipse `(0, 2π)`.
    pub fn curve_domain(&self, c: CurveId) -> Option<(f64, f64)> {
        Some(self.geom(c)?.domain())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use planora_math::PI;

    fn ellipse() -> Geom {
        let a = DVec2::new(10.0, 0.0);
        Geom::Ellipse {
            c: DVec2::ZERO,
            a,
            b: DVec2::new(0.0, 5.0),
        }
    }

    #[test]
    fn nearest_finds_the_foot_on_every_kind() {
        let line = Geom::Line {
            a: DVec2::ZERO,
            b: DVec2::new(10.0, 0.0),
        };
        assert_eq!(line.nearest(DVec2::new(3.0, 7.0)), 0.3);
        assert_eq!(line.nearest(DVec2::new(-3.0, 7.0)), 0.0);
        let arc = Geom::Arc {
            c: DVec2::ZERO,
            r: 5.0,
            a0: 0.0,
            sweep: PI / 2.0,
        };
        assert!((arc.nearest(DVec2::new(1.0, 1.0)) - PI / 4.0).abs() < 1e-12);
        assert_eq!(arc.nearest(DVec2::new(1.0, -0.1)), 0.0);
        let e = ellipse();
        let t = e.nearest(DVec2::new(6.0, 4.5));
        let q = e.eval(t).0;
        // Abstandsvektor steht senkrecht auf der Tangente
        assert!(
            (q - DVec2::new(6.0, 4.5)).dot(e.eval(t).1).abs() < 1e-9,
            "{q}"
        );
        let s = Geom::Spline {
            s: Sampled::spline(&[DVec2::ZERO, DVec2::new(5.0, 5.0), DVec2::new(10.0, 0.0)]),
            n: 3,
        };
        assert!((s.nearest(DVec2::new(5.0, 8.0)) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn spline_evaluation_matches_the_sampled_spline() {
        let pts = [
            DVec2::ZERO,
            DVec2::new(3.0, 4.0),
            DVec2::new(7.0, -1.0),
            DVec2::new(9.0, 2.0),
        ];
        let g = Geom::Spline {
            s: Sampled::spline(&pts),
            n: 4,
        };
        let s = Sampled::spline(&pts);
        for t in [-0.5, 0.0, 0.3, 1.0, 1.7, 2.5, 3.0, 3.4] {
            let ((p, d), (q, e)) = (g.eval(t), s.eval(t));
            assert!(p.distance(q) < 1e-12 && d.distance(e) < 1e-12, "{t}");
            // zweite Ableitung gegen Differenzenquotient
            let h = 1e-6;
            let fd = (g.eval(t + h).1 - g.eval(t - h).1) / (2.0 * h);
            if (t - t.round()).abs() > 1e-3 {
                assert!(g.second(t).distance(fd) < 1e-5, "{t}");
            }
        }
    }

    #[test]
    fn length_and_curvature_match_closed_forms() {
        let circle = Geom::Circle {
            c: DVec2::ZERO,
            r: 2.0,
        };
        assert!((circle.length() - 4.0 * PI).abs() < 1e-12);
        assert!((circle.curvature(1.0) - 0.5).abs() < 1e-12);
        let e = ellipse();
        // Ramanujan: π·(3(a+b) − √((3a+b)(a+3b)))
        let ram = PI * (45.0 - (35.0f64 * 25.0).sqrt());
        assert!((e.length() - ram).abs() < 1e-3, "{}", e.length());
        assert!((e.curvature(0.0) - 10.0 / 25.0).abs() < 1e-12);
        let s = Geom::Spline {
            s: Sampled::spline(&[DVec2::ZERO, DVec2::new(5.0, 0.0), DVec2::new(10.0, 0.0)]),
            n: 3,
        };
        assert!((s.length() - 10.0).abs() < 1e-9);
        assert_eq!(s.curvature(0.7), 0.0);
    }

    #[test]
    fn polylines_and_boxes_cover_the_curve() {
        let arc = Geom::Arc {
            c: DVec2::ZERO,
            r: 1.0,
            a0: -0.1,
            sweep: PI,
        };
        let pl = arc.polyline(-0.1, PI - 0.1, 64);
        assert_eq!(pl.len(), 33);
        let (lo, hi) = arc.bbox();
        assert!(
            (hi.y - 1.0).abs() < 1e-12 && (hi.x - 1.0).abs() < 1e-12,
            "{hi}"
        );
        assert!(lo.x < -0.99 && lo.y < -0.09 && lo.y > -0.11, "{lo}");
        let s = Geom::Spline {
            s: Sampled::spline(&[DVec2::ZERO, DVec2::new(5.0, 5.0), DVec2::new(10.0, 0.0)]),
            n: 3,
        };
        let (lo, hi) = s.bbox();
        for p in s.polyline(0.0, 2.0, 256) {
            assert!(
                p.cmpge(lo - 1e-12).all() && p.cmple(hi + 1e-12).all(),
                "{p}"
            );
        }
    }
}
