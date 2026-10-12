use crate::geometry::{Curve, nurbs_divisions};
use planora_math::{DVec3, TAU};

/// Obergrenze für die Zahl der Stützstellen einer Abtastung (schützt vor riesigen Parameterbereichen).
pub(crate) const MAX_SAMPLES: usize = 4096;

/// Orientiertes Stück einer 3D-Kurve: Parameter `t0 → t1` (auch absteigend erlaubt).
#[derive(Clone, Debug)]
pub struct Seg {
    pub curve: Curve,
    pub t0: f64,
    pub t1: f64,
}

impl Seg {
    pub fn new(curve: Curve, t0: f64, t1: f64) -> Self {
        Self { curve, t0, t1 }
    }

    /// Kurvenparameter am Bruchteil `f` ∈ [0, 1] des Stücks.
    pub fn param(&self, f: f64) -> f64 {
        self.t0 + (self.t1 - self.t0) * f
    }

    pub fn at(&self, f: f64) -> DVec3 {
        self.curve.point(self.param(f))
    }

    pub fn start(&self) -> DVec3 {
        self.at(0.0)
    }

    pub fn end(&self) -> DVec3 {
        self.at(1.0)
    }

    pub fn reversed(&self) -> Seg {
        Seg {
            curve: self.curve.clone(),
            t0: self.t1,
            t1: self.t0,
        }
    }

    /// Teilstück zwischen den Bruchteilen `f0` und `f1`.
    pub fn sub(&self, f0: f64, f1: f64) -> Seg {
        Seg {
            curve: self.curve.clone(),
            t0: self.param(f0),
            t1: self.param(f1),
        }
    }

    /// Tangente in Laufrichtung des Stücks (nicht normiert).
    pub fn tangent_at(&self, f: f64) -> DVec3 {
        let d = self.curve.tangent(self.param(f));
        if self.t1 >= self.t0 { d } else { -d }
    }

    /// Länge, aus einem Streckenzug mit 48 Abschnitten geschätzt (Freiformkurven: mindestens
    /// 48, sonst nach ihrer Gestalt wie [`curve_sample_count`]).
    pub fn length(&self) -> f64 {
        let n = match self.curve {
            Curve::Nurbs(_) => curve_sample_count(&self.curve, self.t0, self.t1, 72, 48),
            _ => 48,
        };
        (0..n)
            .map(|i| {
                self.at(i as f64 / n as f64)
                    .distance(self.at((i + 1) as f64 / n as f64))
            })
            .sum()
    }

    /// Nächster Punkt des Stücks zu `p`: `(Bruchteil f, Abstand)`.
    /// Bei Geraden muss `dir` normiert sein (Länge 1).
    pub fn closest(&self, p: DVec3) -> (f64, f64) {
        if let Curve::Line { origin, dir } = &self.curve {
            let (lo, hi) = (self.t0.min(self.t1), self.t0.max(self.t1));
            let t = (p - *origin).dot(*dir).clamp(lo, hi);
            let span = self.t1 - self.t0;
            let f = if span.abs() < 1e-300 {
                0.0
            } else {
                (t - self.t0) / span
            };
            return (f, (self.curve.point(t) - p).length());
        }
        let n = curve_sample_count(&self.curve, self.t0, self.t1, 96, 8);
        let mut best = (0usize, f64::MAX);
        for i in 0..=n {
            let d = (self.at(i as f64 / n as f64) - p).length();
            if d < best.1 {
                best = (i, d);
            }
        }
        let lo = best.0.saturating_sub(1) as f64 / n as f64;
        let hi = (best.0 + 1).min(n) as f64 / n as f64;
        golden_min(lo, hi, |f| (self.at(f) - p).length())
    }
}

/// Anzahl der Abschnitte für einen Parameterbereich: `per_turn` je Vollkreis, mindestens `min`,
/// höchstens [`MAX_SAMPLES`]. Nicht endliche Bereiche liefern `min`.
pub(crate) fn sample_count(t0: f64, t1: f64, per_turn: usize, min: usize) -> usize {
    let span = (t1 - t0).abs();
    if !span.is_finite() {
        return min;
    }
    let n = (span / (TAU / per_turn as f64)).ceil();
    (n as usize).clamp(min, MAX_SAMPLES)
}

/// Wie [`sample_count`], aber Freiformkurven nach ihrer Struktur ([`nurbs_divisions`] mit dem
/// Winkelschritt `TAU / per_turn`) statt nach der Breite des Parameterbereichs; ebenfalls
/// mindestens `min`, höchstens [`MAX_SAMPLES`]. Andere Kurven: genau `sample_count`.
pub(crate) fn curve_sample_count(
    curve: &Curve,
    t0: f64,
    t1: f64,
    per_turn: usize,
    min: usize,
) -> usize {
    match curve {
        Curve::Nurbs(c) => {
            nurbs_divisions(c, t0, t1, TAU / per_turn.max(1) as f64).clamp(min, MAX_SAMPLES)
        }
        _ => sample_count(t0, t1, per_turn, min),
    }
}

/// Dreht eine Schleife um: Reihenfolge umkehren und jedes Stück umdrehen.
pub fn reverse_loop(lp: &[Seg]) -> Vec<Seg> {
    lp.iter().rev().map(Seg::reversed).collect()
}

/// Goldener Schnitt: Minimum von `f` auf `[a, b]` → `(x, f(x))`.
pub(crate) fn golden_min(mut a: f64, mut b: f64, f: impl Fn(f64) -> f64) -> (f64, f64) {
    const R: f64 = 0.618_033_988_749_894_9;
    let mut c = b - R * (b - a);
    let mut d = a + R * (b - a);
    let (mut fc, mut fd) = (f(c), f(d));
    for _ in 0..80 {
        // `fc < fd` ist bei NaN falsch: dann läuft die Suche trotzdem endlich durch.
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - R * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + R * (b - a);
            fd = f(d);
        }
    }
    let x = 0.5 * (a + b);
    (x, f(x))
}
