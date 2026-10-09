//! Auswertung von Ellipse und natürlichem kubischem Spline mit Ableitungen sowie die Suche
//! nach dem Startparameter der Zusatzvariablen parametrischer Abhängigkeiten.

use crate::dual::Dual;
use planora_math::{DVec2, TAU};

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

/// Eine Ellipse oder ein Spline in Gleitkommazahlen (zum Abtasten).
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
        }
    }

    /// Abtastparameter: gleichmäßig über `[0, TAU]` bzw. `[0, n − 1]`.
    fn samples(&self) -> Vec<f64> {
        let (end, steps) = match self {
            Sampled::Ellipse { .. } => (TAU, ELLIPSE_SAMPLES),
            Sampled::Spline { y, .. } => {
                let segs = y[0].len() - 1;
                (segs as f64, segs * SPLINE_SAMPLES_PER_SEGMENT)
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
