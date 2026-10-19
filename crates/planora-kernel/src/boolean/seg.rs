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

    /// Achsenparalleler Quader, der eine Freiformkurve sicher ganz enthält (Hülle der
    /// Kontrollpunkte: bei positiven Gewichten liegt die Kurve in deren konvexer Hülle, also auch
    /// jedes Stück). `None` für analytische Kurven (dort ist [`Seg::closest`] ohnehin billig).
    pub(crate) fn freeform_hull(&self) -> Option<(DVec3, DVec3)> {
        let Curve::Nurbs(c) = &self.curve else {
            return None;
        };
        Some(bounds(c.control_points().iter().copied()))
    }

    /// Quader, die das Stück einer Freiformkurve zusammen sicher enthalten: je Knotenabschnitt,
    /// den das Stück berührt, die Hülle der `p + 1` Kontrollpunkte, von denen die Kurve dort
    /// abhängt (bei positiven Gewichten liegt sie in deren konvexer Hülle; ohne Zerlegung, linear
    /// in der Zahl der Abschnitte). Reicht das Stück über den Bereich hinaus (gefaltete Parameter
    /// geschlossener Kurven), die Hülle der ganzen Kurve.
    pub(crate) fn piece_boxes(&self) -> Option<Vec<(DVec3, DVec3)>> {
        let Curve::Nurbs(c) = &self.curve else {
            return None;
        };
        let (lo, hi) = (self.t0.min(self.t1), self.t0.max(self.t1));
        let (a, b) = c.domain();
        if !(lo >= a && hi <= b) {
            return self.freeform_hull().map(|h| vec![h]);
        }
        let (knots, p, cp) = (c.knots().knots(), c.degree(), c.control_points());
        Some(
            (p..cp.len())
                .filter(|&k| knots[k] < knots[k + 1] && knots[k + 1] >= lo && knots[k] <= hi)
                .map(|k| bounds(cp[k - p..=k].iter().copied()))
                .collect(),
        )
    }

    /// Quader, der das Stück sicher enthält: Gerade über ihre Enden, Kreis und Ellipse über
    /// Mittelpunkt ± größte Halbachse, Freiformkurve über [`Seg::freeform_hull`] (billig).
    pub(crate) fn sound_box(&self) -> (DVec3, DVec3) {
        match &self.curve {
            Curve::Line { .. } => {
                let (a, b) = (self.start(), self.end());
                (a.min(b), b.max(a))
            }
            Curve::Circle { frame, radius } => (
                frame.origin - DVec3::splat(radius.abs()),
                frame.origin + DVec3::splat(radius.abs()),
            ),
            Curve::Ellipse {
                frame,
                major,
                minor,
            } => {
                let r = major.abs().max(minor.abs());
                (
                    frame.origin - DVec3::splat(r),
                    frame.origin + DVec3::splat(r),
                )
            }
            Curve::Nurbs(_) => self
                .freeform_hull()
                .unwrap_or((DVec3::splat(f64::NEG_INFINITY), DVec3::splat(f64::INFINITY))),
        }
    }

    /// Sicher weiter als `d` von `p` entfernt, ohne Suche: `p` liegt mehr als `d` neben der
    /// Hülle einer Freiformkurve. `false` heißt „nicht entschieden“.
    pub(crate) fn surely_farther(&self, p: DVec3, d: f64) -> bool {
        self.freeform_hull()
            .is_some_and(|(lo, hi)| box_distance(lo, hi, p) > d)
    }

    /// Wie [`Seg::within`], mit vorab berechneten [`Seg::piece_boxes`] (enger als die Hülle
    /// der ganzen Kurve; lohnt, wenn dasselbe Stück gegen viele Punkte geprüft wird).
    pub(crate) fn within_boxes(&self, boxes: Option<&[(DVec3, DVec3)]>, p: DVec3, d: f64) -> bool {
        !boxes.is_some_and(|bx| boxes_distance(bx, p) > d) && self.closest(p).1 <= d
    }

    /// Gleich `self.closest(p).1 <= d`, aber ohne die teure Suche, wenn die Hülle einer
    /// Freiformkurve schon zeigt, dass `p` weiter als `d` entfernt ist (Ergebnis gleich, weil der
    /// gefundene Abstand nie kleiner als der wahre ist).
    pub(crate) fn within(&self, p: DVec3, d: f64) -> bool {
        !self.surely_farther(p, d) && self.closest(p).1 <= d
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
        let found = golden_min(lo, hi, |f| (self.at(f) - p).length());
        // Vollkreis oder Vollellipse als ein Stück (Anfang = Ende): Stützstelle 0 und n sind
        // derselbe Punkt, bei Gleichstand gewinnt zufällig eine von beiden. Liegt `p` kurz vor
        // dem Ende, wäre sonst nur am Anfang gesucht und die Naht geliefert worden; deshalb auch
        // den Abschnitt am anderen Ende prüfen und das kleinere Minimum nehmen. Das alte Ergebnis
        // bleibt nur bei bitgleichem Abstand; liegt `p` genau auf der Naht, kann wegen Rundung
        // jetzt f ≈ 1 statt f ≈ 0 herauskommen (derselbe Punkt). Freiformkurven bleiben bewusst
        // bei der alten Suche (ihre Ablehnungen hängen daran).
        if matches!(self.curve, Curve::Circle { .. } | Curve::Ellipse { .. })
            && (best.0 == 0 || best.0 == n)
            && self.start().distance(self.end()) <= 1e-9
        {
            let (lo, hi) = if best.0 == 0 {
                ((n - 1) as f64 / n as f64, 1.0)
            } else {
                (0.0, 1.0 / n as f64)
            };
            let other = golden_min(lo, hi, |f| (self.at(f) - p).length());
            if other.1 < found.1 {
                return other;
            }
        }
        found
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

/// Abstand von `p` zum Quader `[lo, hi]` (0 innen).
pub(crate) fn box_distance(lo: DVec3, hi: DVec3, p: DVec3) -> f64 {
    (lo - p).max(p - hi).max(DVec3::ZERO).length()
}

/// Abstand zweier Quader (0, wenn sie sich überlappen).
pub(crate) fn box_gap(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> f64 {
    (b.0 - a.1).max(a.0 - b.1).max(DVec3::ZERO).length()
}

/// Hüllquader von Punkten (leer: umgekehrter, unendlicher Quader).
fn bounds(points: impl Iterator<Item = DVec3>) -> (DVec3, DVec3) {
    points.fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(p), hi.max(p)),
    )
}

/// Kleinster Abstand von `p` zu einem der Quader (unendlich ohne Quader).
pub(crate) fn boxes_distance(boxes: &[(DVec3, DVec3)], p: DVec3) -> f64 {
    boxes
        .iter()
        .map(|(lo, hi)| box_distance(*lo, *hi, p))
        .fold(f64::INFINITY, f64::min)
}

/// Hüllquader mehrerer Quader (`None` ohne Quader).
pub(crate) fn union_box(boxes: &[(DVec3, DVec3)]) -> Option<(DVec3, DVec3)> {
    let first = *boxes.first()?;
    Some(
        boxes
            .iter()
            .fold(first, |(lo, hi), (a, b)| (lo.min(*a), hi.max(*b))),
    )
}
