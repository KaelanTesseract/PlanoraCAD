//! Auswertung von Ellipse und natürlichem kubischem Spline mit Ableitungen sowie die Suche
//! nach dem Startparameter der Zusatzvariablen parametrischer Abhängigkeiten.

use crate::dual::Dual;
use crate::model::within_coord_range;
use crate::spline_handles::Nodes;
use planora_math::nurbs::{MAX_WEIGHT, MIN_WEIGHT, NurbsCurve, NurbsError, uniform_clamped_knots};
use planora_math::{DVec2, TAU};
use std::sync::Arc;

pub type V2 = (Dual, Dual);

/// Abtastschritte je Spline-Segment bei der Suche nach dem Startparameter.
const SPLINE_SAMPLES_PER_SEGMENT: usize = 64;
/// Abtastschritte über die ganze Ellipse bei der Suche nach dem Startparameter.
const ELLIPSE_SAMPLES: usize = 256;
/// Halbierungsschritte der Verfeinerung (2⁻⁶⁰ der Abtastweite, weit unter jeder Toleranz).
const BISECTION_STEPS: usize = 60;

/// Zweite Ableitungen `M` eines natürlichen Splines mit Parameterabstand 1 durch die Werte `y`
/// (tridiagonal: `M[i−1] + 4·M[i] + M[i+1] = 6·(y[i−1] − 2·y[i] + y[i+1])`, `M[0] = M[n−1] = 0`).
pub fn spline_second_derivatives(y: &[Dual]) -> Vec<Dual> {
    let n = y.len();
    let mut m = vec![Dual::constant(0.0); n];
    if n < 3 {
        return m;
    }
    let k = n - 2;
    // Thomas-Verfahren mit konstanten Koeffizienten (1, 4, 1)
    let mut c = vec![0.0; k];
    let mut d: Vec<Dual> = Vec::with_capacity(k);
    for i in 0..k {
        let rhs = 6.0 * (&y[i] - &(2.0 * &y[i + 1]) + &y[i + 2]);
        if i == 0 {
            c[0] = 1.0 / 4.0;
            d.push(rhs / 4.0);
        } else {
            let den = 4.0 - c[i - 1];
            c[i] = 1.0 / den;
            let di = (rhs - &d[i - 1]) / den;
            d.push(di);
        }
    }
    let mut x = d.clone();
    for i in (0..k.saturating_sub(1)).rev() {
        x[i] = &d[i] - &(c[i] * &x[i + 1]);
    }
    for (i, xi) in x.into_iter().enumerate() {
        m[i + 1] = xi;
    }
    m
}

/// Wert und Ableitung nach `t` einer Koordinate des Splines bei Parameter `t`. Außerhalb von
/// `[0, n − 1]` wird das erste bzw. letzte Segment fortgesetzt. `y` hat mindestens zwei Werte.
pub fn spline_eval(y: &[Dual], m: &[Dual], t: &Dual) -> (Dual, Dual) {
    let n = y.len();
    // `NaN.max(0.0)` ist 0; zu große Werte sättigen beim Umwandeln und werden begrenzt.
    let i = (t.v.floor().max(0.0) as usize).min(n - 2);
    let u = t - i as f64;
    let w = 1.0 - &u;
    let value = &w * &y[i]
        + &u * &y[i + 1]
        + (&w * &w * &w - &w) * &m[i] / 6.0
        + (&u * &u * &u - &u) * &m[i + 1] / 6.0;
    let deriv = &y[i + 1] - &y[i] - (3.0 * &w * &w - 1.0) * &m[i] / 6.0
        + (3.0 * &u * &u - 1.0) * &m[i + 1] / 6.0;
    (value, deriv)
}

/// Punkt und Ableitung nach `t` der Ellipse `c + a·cos t + b̂·minor·sin t`, `b̂ = (−ay, ax)/|a|`.
/// `a_len` ist `|a|` (vom Aufrufer gegen null abgesichert).
pub fn ellipse_eval(c: &V2, a: &V2, a_len: &Dual, minor: &Dual, t: &Dual) -> (V2, V2) {
    let (ct, st) = (t.cos(), t.sin());
    let k = minor / a_len;
    // b = b̂·minor
    let b = (-(&k * &a.1), &k * &a.0);
    let p = (
        &c.0 + &(&a.0 * &ct) + &b.0 * &st,
        &c.1 + &(&a.1 * &ct) + &b.1 * &st,
    );
    let d = (&b.0 * &ct - &a.0 * &st, &b.1 * &ct - &a.1 * &st);
    (p, d)
}

/// Punkt und Ableitung nach `t` einer NURBS-Kurve: de Boor in homogenen Koordinaten, Knoten und
/// Gewichte konstant (`None`: alle 1). Der Abschnitt wird auf den Bereich geklemmt; außerhalb
/// wird also das erste bzw. letzte Polynomstück fortgesetzt. Der Knotenvektor ist geprüft
/// (geklemmt, `pts.len() + degree + 1` Knoten). Fallen Kontrollpunkte zusammen, bleibt alles
/// endlich; die Ableitung kann dann null sein.
pub(crate) fn nurbs_eval_dual(
    degree: usize,
    knots: &[f64],
    weights: Option<&[f64]>,
    pts: &[V2],
    t: &Dual,
) -> (V2, V2) {
    let p = degree;
    // NaN: kein Knoten ist `<= t`, also der erste Abschnitt.
    let span = knots
        .partition_point(|k| *k <= t.v)
        .saturating_sub(1)
        .clamp(p, pts.len() - 1);
    let mut d: Vec<[Dual; 3]> = (span - p..=span)
        .map(|i| {
            let w = weights.map_or(1.0, |ws| ws[i]);
            [&pts[i].0 * w, &pts[i].1 * w, c(w)]
        })
        .collect();
    let mut last = [c(0.0), c(0.0), c(0.0)];
    for r in 1..=p {
        // Die letzte Stufe verbindet genau die beiden Punkte, deren Differenz die Ableitung gibt.
        if r == p {
            let scale = p as f64 / (knots[span + 1] - knots[span]);
            last = std::array::from_fn(|k| scale * (&d[p][k] - &d[p - 1][k]));
        }
        for j in (r..=p).rev() {
            let i = span - p + j;
            let a = (t - knots[i]) / (knots[i + p - r + 1] - knots[i]);
            let b = 1.0 - &a;
            d[j] = std::array::from_fn(|k| &b * &d[j - 1][k] + &a * &d[j][k]);
        }
    }
    let [ax, ay, w] = &d[p];
    let point = (ax / w, ay / w);
    // C' = (A' − w'·C) / w
    let deriv = (
        (&last[0] - &last[2] * &point.0) / w,
        (&last[1] - &last[2] * &point.1) / w,
    );
    (point, deriv)
}

/// Die Stellen von `t0` bis `t1` (`t0 < t1`), an denen ein neues Bézier-Stück beginnt: `t0`, alle
/// verschiedenen Knoten dazwischen, `t1`.
pub(crate) fn nurbs_breaks(curve: &NurbsCurve, t0: f64, t1: f64) -> Vec<f64> {
    let mut out = vec![t0];
    for &k in curve.knots().knots() {
        if k > t0 && k < t1 && out.last().is_some_and(|last| *last != k) {
            out.push(k);
        }
    }
    out.push(t1);
    out
}

/// Eine Ellipse, ein Spline oder eine NURBS-Kurve in Gleitkommazahlen (zum Abtasten).
#[derive(Clone, Debug)]
pub(crate) enum Sampled {
    Ellipse {
        c: DVec2,
        a: DVec2,
        /// `b̂·minor`
        b: DVec2,
    },
    Spline {
        y: [Vec<Dual>; 2],
        m: [Vec<Dual>; 2],
    },
    Nurbs(Arc<NurbsCurve>),
    /// Spline mit Griffen: quintische Hermite-Stücke durch die Knotenwerte (`spline_handles.rs`).
    Hermite {
        nodes: Nodes<DVec2>,
    },
}

fn c(v: f64) -> Dual {
    Dual::constant(v)
}

impl Sampled {
    pub(crate) fn ellipse(center: DVec2, a: DVec2, minor: f64) -> Sampled {
        Sampled::Ellipse {
            c: center,
            a,
            b: a.perp() / a.length() * minor,
        }
    }

    /// Spline durch mindestens zwei Stützpunkte.
    pub(crate) fn spline(points: &[DVec2]) -> Sampled {
        let y = [
            points.iter().map(|p| c(p.x)).collect::<Vec<_>>(),
            points.iter().map(|p| c(p.y)).collect::<Vec<_>>(),
        ];
        let m = [
            spline_second_derivatives(&y[0]),
            spline_second_derivatives(&y[1]),
        ];
        Sampled::Spline { y, m }
    }

    /// Punkt und Ableitung bei `t` (Splines werden außerhalb fortgesetzt, siehe `spline_eval`).
    pub(crate) fn eval(&self, t: f64) -> (DVec2, DVec2) {
        match self {
            Sampled::Ellipse { c, a, b } => {
                let (s, co) = t.sin_cos();
                (*c + *a * co + *b * s, *b * co - *a * s)
            }
            Sampled::Spline { y, m } => {
                let tt = c(t);
                let (x, dx) = spline_eval(&y[0], &m[0], &tt);
                let (yv, dy) = spline_eval(&y[1], &m[1], &tt);
                (DVec2::new(x.v, yv.v), DVec2::new(dx.v, dy.v))
            }
            Sampled::Nurbs(curve) => {
                let d = curve.derivs_unclamped(t, 1);
                (d[0].truncate(), d[1].truncate())
            }
            Sampled::Hermite { nodes } => nodes.eval(t),
        }
    }

    /// Abtastparameter: gleichmäßig über `[0, TAU]` bzw. `[0, n − 1]`, bei NURBS-Kurven
    /// gleichmäßig je Bézier-Stück.
    fn samples(&self) -> Vec<f64> {
        let (end, steps) = match self {
            Sampled::Ellipse { .. } => (TAU, ELLIPSE_SAMPLES),
            Sampled::Spline { y, .. } => {
                let segs = y[0].len() - 1;
                (segs as f64, segs * SPLINE_SAMPLES_PER_SEGMENT)
            }
            Sampled::Hermite { nodes } => {
                let segs = nodes.value.len() - 1;
                (segs as f64, segs * SPLINE_SAMPLES_PER_SEGMENT)
            }
            Sampled::Nurbs(curve) => {
                let (lo, hi) = curve.domain();
                return nurbs_samples(curve, lo, hi, SPLINE_SAMPLES_PER_SEGMENT);
            }
        };
        (0..=steps).map(|k| end * k as f64 / steps as f64).collect()
    }

    /// Kurvenparameter des Punktes, der `p` am nächsten liegt.
    pub(crate) fn nearest_to_point(&self, p: DVec2) -> f64 {
        let ts = self.samples();
        let dist = |t: f64| self.eval(t).0.distance_squared(p);
        // Ableitung von ½·|C(t) − p|²: wechselt am Minimum von − nach +.
        let slope = |t: f64| {
            let (q, d) = self.eval(t);
            (q - p).dot(d)
        };
        let k = argmin(&ts, dist);
        let mut best = ts[k];
        let mut intervals = Vec::new();
        if k > 0 {
            intervals.push((ts[k - 1], ts[k]));
        }
        if k + 1 < ts.len() {
            intervals.push((ts[k], ts[k + 1]));
        }
        for (lo, hi) in intervals {
            if slope(lo) <= 0.0
                && slope(hi) >= 0.0
                && let Some(t) = bisect(lo, hi, slope)
                && dist(t) <= dist(best)
            {
                best = t;
            }
        }
        best
    }

    /// Kurvenparameter eines Berührpunkts mit der Geraden durch `o` mit Richtung `d`: unter
    /// allen Stellen mit zur Geraden paralleler Tangente die mit dem kleinsten Abstand; gibt es
    /// keine, die abgetastete Stelle mit dem kleinsten Abstand.
    pub(crate) fn tangent_to_line(&self, o: DVec2, d: DVec2) -> f64 {
        let ts = self.samples();
        let dist = |t: f64| d.perp_dot(self.eval(t).0 - o).abs();
        let turn = |t: f64| self.eval(t).1.perp_dot(d);
        let values: Vec<f64> = ts.iter().map(|&t| turn(t)).collect();
        let mut best: Option<f64> = None;
        for k in 0..ts.len() - 1 {
            let (a, b) = (values[k], values[k + 1]);
            if a * b > 0.0 || (a == 0.0 && b == 0.0) {
                continue;
            }
            let t = if a == 0.0 {
                ts[k]
            } else if b == 0.0 {
                ts[k + 1]
            } else {
                // Vorzeichen so drehen, dass die Funktion von − nach + wechselt.
                match bisect(ts[k], ts[k + 1], |t| -turn(t) * a.signum()) {
                    Some(t) => t,
                    None => continue,
                }
            };
            if best.is_none_or(|bt| dist(t) < dist(bt)) {
                best = Some(t);
            }
        }
        best.unwrap_or_else(|| ts[argmin(&ts, dist)])
    }
}

/// Teilungsstellen eines Splines, die relativ zum Parameterbereich höchstens so nah an einem
/// Stützpunkt bzw. Knoten liegen, werden auf ihn gelegt (deutlich über `MIN_KNOT_GAP_RATIO` = 1e-9).
pub(crate) const KNOT_SNAP: f64 = 1e-7;

/// `t`, oder der Knoten von `curve`, der höchstens [`KNOT_SNAP`] des Bereichs daneben liegt: Sonst
/// entstünden beim Teilen zu nahe Knoten (Grenze `MIN_KNOT_GAP_RATIO` der NURBS).
pub(crate) fn snap_to_knot(curve: &NurbsCurve, t: f64) -> f64 {
    let (lo, hi) = curve.domain();
    let reach = KNOT_SNAP * (hi - lo);
    curve
        .knots()
        .knots()
        .iter()
        .copied()
        .filter(|k| (t - k).abs() <= reach)
        .min_by(|a, b| (t - a).abs().total_cmp(&(t - b).abs()))
        .unwrap_or(t)
}

/// `per_piece` gleiche Schritte je Bézier-Stück von `t0` bis `t1` (`t0 < t1`), mit beiden Enden.
pub(crate) fn nurbs_samples(curve: &NurbsCurve, t0: f64, t1: f64, per_piece: usize) -> Vec<f64> {
    let breaks = nurbs_breaks(curve, t0, t1);
    let mut out = Vec::with_capacity((breaks.len() - 1) * per_piece + 1);
    for w in breaks.windows(2) {
        out.extend((0..per_piece).map(|k| w[0] + (w[1] - w[0]) * k as f64 / per_piece as f64));
    }
    out.push(t1);
    out
}

/// Knoten eines Kegelschnitts: ein Bézier-Stück über `[0, 1]`.
pub(crate) const CONIC_KNOTS: [f64; 6] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];

/// Inneres Gewicht des Kegelschnitts mit dem Rho `rho`; die Endgewichte sind 1.
pub(crate) fn conic_weight(rho: f64) -> f64 {
    rho / (1.0 - rho)
}

/// Kegelschnitt von `p0` nach `p2` mit dem Scheitel `a` als NURBS-Kurve in Skizzenkoordinaten
/// (`z = 0`): Grad 2 über `[0, 1]`, Gewichte `1, rho / (1 − rho), 1`. Ein Rho, dessen Gewicht außerhalb
/// des Bereichs der NURBS liegt, und Punkte außerhalb des Rechenbereichs ergeben einen Fehler.
pub fn conic_nurbs(p0: DVec2, a: DVec2, p2: DVec2, rho: f64) -> Result<NurbsCurve, NurbsError> {
    NurbsCurve::new(
        2,
        CONIC_KNOTS.to_vec(),
        vec![p0.extend(0.0), a.extend(0.0), p2.extend(0.0)],
        vec![1.0, conic_weight(rho), 1.0],
    )
}

/// `0 < rho < 1`, und das Gewicht liegt im Bereich, den eine NURBS-Kurve erlaubt (das schließt nur
/// Werte aus, die dichter als etwa 1e-6 an 0 oder 1 liegen). `NaN` ist ungültig.
pub fn conic_rho_ok(rho: f64) -> bool {
    rho > 0.0 && rho < 1.0 && (MIN_WEIGHT..=MAX_WEIGHT).contains(&conic_weight(rho))
}

/// Taugt `a` als Scheitel über der Sehne `p0`–`p2`? Die Enden sind getrennt, und der Scheitel liegt
/// weiter als `tol` neben ihrer Geraden. Dieselbe Frage für den Klick auf den Scheitel, die Vorschau
/// und [`crate::model::Sketch::add_conic`].
pub fn conic_apex_ok(p0: DVec2, a: DVec2, p2: DVec2, tol: f64) -> bool {
    let chord = p2 - p0;
    let len = chord.length();
    len > tol && (chord.perp_dot(a - p0) / len).abs() > tol
}

/// Wirksamer Grad und gleichmäßiger geklemmter Knotenvektor eines neu gezeichneten Splines mit `n`
/// Kontrollpunkten beim gewählten Grad `degree`: der kleinere Wert aus `degree` und `n − 1`
/// (**Annahme**, deckt den Foreneintrag zu Fusion).
pub(crate) fn control_spline_layout(degree: u8, n: usize) -> (u8, Vec<f64>) {
    let p = usize::from(degree).min(n.saturating_sub(1));
    (p as u8, uniform_clamped_knots(p, n))
}

/// Die Kurve, die [`crate::model::Sketch::add_control_spline`] aus diesen Kontrollpunkten und dem
/// gewählten Grad anlegt, als NURBS-Kurve in Skizzenkoordinaten (`z = 0`).
pub fn control_spline_nurbs(points: &[DVec2], degree: u8) -> Result<NurbsCurve, NurbsError> {
    let (p, knots) = control_spline_layout(degree, points.len());
    NurbsCurve::new(
        usize::from(p),
        knots,
        points.iter().map(|q| q.extend(0.0)).collect(),
        vec![1.0; points.len()],
    )
}

/// Kontrollpunkte und Gewichte des Stücks `u0..u1` eines Kegelschnitts in homogener Teilung (Blüten),
/// der Parameter bleibt der der ganzen Kurve; auch außerhalb von `[0, 1]`. `None`, wenn ein Gewicht
/// nicht positiv ist oder ein Punkt außerhalb des Rechenbereichs liegt.
fn conic_span(
    p0: DVec2,
    a: DVec2,
    p2: DVec2,
    rho: f64,
    u0: f64,
    u1: f64,
) -> Option<([DVec2; 3], [f64; 3])> {
    let w = conic_weight(rho);
    let blossom = |u: f64, v: f64| {
        let (c0, c1, c2) = ((1.0 - u) * (1.0 - v), (1.0 - u) * v + u * (1.0 - v), u * v);
        (p0 * c0 + a * (w * c1) + p2 * c2, c0 + w * c1 + c2)
    };
    let h = [blossom(u0, u0), blossom(u0, u1), blossom(u1, u1)];
    let weights = h.map(|x| x.1);
    let points = h.map(|x| x.0 / x.1);
    let ok = weights.iter().all(|w| w.is_finite() && *w > 0.0)
        && points.iter().all(|&p| within_coord_range(p));
    ok.then_some((points, weights))
}

/// Das Stück `u0..u1` eines Kegelschnitts wieder als Kegelschnitt: Start, Scheitel, Ende und Rho aus
/// dem Gewicht `w₁/√(w₀·w₂)`. Auch für `u` außerhalb von `[0, 1]` (Dehnen). `None` bei nicht positiven
/// Gewichten, außerhalb des Rechenbereichs und wenn das neue Rho nicht darstellbar ist.
pub(crate) fn conic_piece(
    p0: DVec2,
    a: DVec2,
    p2: DVec2,
    rho: f64,
    u0: f64,
    u1: f64,
) -> Option<(DVec2, DVec2, DVec2, f64)> {
    let ([q0, q1, q2], [w0, w1, w2]) = conic_span(p0, a, p2, rho, u0, u1)?;
    let inner = w1 / (w0 * w2).sqrt();
    let new_rho = inner / (1.0 + inner);
    conic_rho_ok(new_rho).then_some((q0, q1, q2, new_rho))
}

/// Das Stück `u0..u1` als NURBS-Kurve mit dem Parameter der ganzen Kurve, zum Suchen beim Dehnen.
/// `None` wie bei [`conic_piece`] und wenn ein Gewicht außerhalb des Bereichs der NURBS liegt.
pub(crate) fn conic_stretch(
    p0: DVec2,
    a: DVec2,
    p2: DVec2,
    rho: f64,
    u0: f64,
    u1: f64,
) -> Option<NurbsCurve> {
    let (q, w) = conic_span(p0, a, p2, rho, u0, u1)?;
    NurbsCurve::new(
        2,
        vec![u0, u0, u0, u1, u1, u1],
        q.iter().map(|p| p.extend(0.0)).collect(),
        w.to_vec(),
    )
    .ok()
}

/// Index des kleinsten Funktionswerts (NaN zählt als größer als alles andere).
fn argmin(ts: &[f64], f: impl Fn(f64) -> f64) -> usize {
    ts.iter()
        .map(|&t| f(t))
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(0, |(k, _)| k)
}

/// Nullstelle von `f` in `[lo, hi]` mit `f(lo) ≤ 0 ≤ f(hi)` per Intervallhalbierung.
/// `None`, wenn `f` an den Enden nicht endlich ist.
fn bisect(mut lo: f64, mut hi: f64, f: impl Fn(f64) -> f64) -> Option<f64> {
    if !(f(lo).is_finite() && f(hi).is_finite()) {
        return None;
    }
    for _ in 0..BISECTION_STEPS {
        let mid = 0.5 * (lo + hi);
        if f(mid) <= 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(0.5 * (lo + hi))
}

#[cfg(test)]
mod tests {
    use super::*;
    use planora_math::DVec3;

    fn v2(p: DVec2) -> V2 {
        (c(p.x), c(p.y))
    }

    /// Dichte Stellen im Bereich und ein Stück darüber hinaus.
    fn dense(lo: f64, hi: f64) -> impl Iterator<Item = f64> {
        let w = hi - lo;
        (-50..=1050).map(move |i| lo + w * i as f64 / 1000.0)
    }

    fn compare(curve: &NurbsCurve) {
        let pts: Vec<V2> = curve
            .control_points()
            .iter()
            .map(|q| v2(q.truncate()))
            .collect();
        let weights = curve.weights();
        let (lo, hi) = curve.domain();
        for u in dense(lo, hi) {
            // `t` als Variable: Der Gradient des Punkts ist die Ableitung.
            let t = Dual::variable(u, 0, 1);
            let (q, d) = nurbs_eval_dual(
                curve.degree(),
                curve.knots().knots(),
                Some(weights),
                &pts,
                &t,
            );
            let want = curve.derivs_unclamped(u, 1);
            let (qv, dv) = (DVec2::new(q.0.v, q.1.v), DVec2::new(d.0.v, d.1.v));
            assert!(qv.distance(want[0].truncate()) < 1e-12, "Punkt bei {u}");
            assert!(dv.distance(want[1].truncate()) < 1e-11, "Ableitung bei {u}");
            let dt = DVec2::new(q.0.grad(0), q.1.grad(0));
            assert!(dt.distance(dv) < 1e-11, "Gradient bei {u}");
        }
    }

    fn control(xy: &[(f64, f64)]) -> Vec<DVec3> {
        xy.iter().map(|&(x, y)| DVec3::new(x, y, 0.0)).collect()
    }

    const POLY: [(f64, f64); 7] = [
        (0.0, 0.0),
        (2.0, 5.0),
        (5.0, 7.0),
        (9.0, 6.0),
        (12.0, 3.0),
        (14.0, 0.0),
        (15.0, -4.0),
    ];

    #[test]
    fn dual_eval_matches_nurbs_curve() {
        let ones = vec![1.0; POLY.len()];
        for (p, knots) in [
            (1, vec![0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 6.0]),
            (3, vec![0., 0., 0., 0., 1., 2., 3., 4., 4., 4., 4.]),
            (5, vec![0., 0., 0., 0., 0., 0., 1., 2., 2., 2., 2., 2., 2.]),
            // ungleichmäßig, mit doppeltem Knoten, Bereich nicht ab 0
            (3, vec![7., 7., 7., 7., 7.5, 7.5, 9., 11., 11., 11., 11.]),
        ] {
            let curve = NurbsCurve::new(p, knots, control(&POLY), ones.clone()).unwrap();
            compare(&curve);
        }
    }

    #[test]
    fn dual_eval_matches_rational_curves() {
        let weights = vec![1.0, 0.5, 2.0, 1.0, 3.0, 0.25, 1.0];
        let knots = vec![0., 0., 0., 0., 1., 2., 3., 4., 4., 4., 4.];
        let curve = NurbsCurve::new(3, knots, control(&POLY), weights).unwrap();
        compare(&curve);
    }

    #[test]
    fn dual_gradient_by_control_points_matches_differences() {
        let knots = [0., 0., 0., 0., 1., 2., 3., 4., 4., 4., 4.];
        let n = POLY.len();
        let eval = |at: &[f64], t: f64| {
            let pts: Vec<V2> = (0..n)
                .map(|i| {
                    (
                        Dual::variable(at[2 * i], 2 * i, 2 * n),
                        Dual::variable(at[2 * i + 1], 2 * i + 1, 2 * n),
                    )
                })
                .collect();
            nurbs_eval_dual(3, &knots, None, &pts, &c(t))
        };
        let x: Vec<f64> = POLY.iter().flat_map(|&(a, b)| [a, b]).collect();
        let h = 1e-6;
        for t in [0.3, 1.0, 2.71, 4.0] {
            let (q, d) = eval(&x, t);
            for i in 0..x.len() {
                let (mut up, mut down) = (x.clone(), x.clone());
                up[i] += h;
                down[i] -= h;
                let (qu, du) = eval(&up, t);
                let (qd, dd) = eval(&down, t);
                let fd = |a: f64, b: f64| (a - b) / (2.0 * h);
                assert!((fd(qu.0.v, qd.0.v) - q.0.grad(i)).abs() < 1e-8);
                assert!((fd(qu.1.v, qd.1.v) - q.1.grad(i)).abs() < 1e-8);
                assert!((fd(du.0.v, dd.0.v) - d.0.grad(i)).abs() < 1e-8);
                assert!((fd(du.1.v, dd.1.v) - d.1.grad(i)).abs() < 1e-8);
            }
        }
    }

    #[test]
    fn dual_eval_with_coinciding_points_and_nan_stays_defined() {
        let same = (c(2.0), c(3.0));
        let pts = vec![same.clone(), same.clone(), same.clone(), same];
        let knots = [0., 0., 0., 0., 1., 1., 1., 1.];
        let (q, d) = nurbs_eval_dual(3, &knots, None, &pts, &c(0.4));
        assert!((q.0.v - 2.0).abs() < 1e-12 && (q.1.v - 3.0).abs() < 1e-12);
        assert!(d.0.v.abs() < 1e-12 && d.1.v.abs() < 1e-12);
        // NaN darf den Abschnitt nicht zerstören (Löser mit kaputtem Schritt).
        let _ = nurbs_eval_dual(3, &knots, None, &pts, &c(f64::NAN));
    }

    #[test]
    fn samples_cover_every_piece_and_stop_at_the_end() {
        let knots = vec![1., 1., 1., 1., 1.5, 4., 4., 4., 4.];
        let weights = vec![1.0; 5];
        let curve = NurbsCurve::new(3, knots, control(&POLY[..5]), weights).unwrap();
        let s = Sampled::Nurbs(Arc::new(curve)).samples();
        assert_eq!(s.len(), 2 * SPLINE_SAMPLES_PER_SEGMENT + 1);
        assert_eq!(s[0], 1.0);
        assert_eq!(s[SPLINE_SAMPLES_PER_SEGMENT], 1.5);
        assert_eq!(*s.last().unwrap(), 4.0);
        assert!(s.windows(2).all(|w| w[0] < w[1]));
    }
}
