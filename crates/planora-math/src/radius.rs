//! Radius entlang einer Kantenkette (Spec §3.3, R9, Plan P8): monotone kubische
//! Hermite-Interpolation (Fritsch–Butland) durch Punkte (Anteil, Radius). Zwischen zwei Punkten
//! liegt der Radius immer zwischen ihren Werten (kein Überschwingen, also nie unter null),
//! außerhalb des ersten und letzten Punkts konstant. Stetig differenzierbar.
//!
//! Steigungen: an inneren Punkten das gewichtete harmonische Mittel der Nachbarsteigungen
//! (null bei Vorzeichenwechsel oder einer waagrechten Nachbarstrecke), an den Enden die Steigung
//! der ersten bzw. letzten Strecke. Beide liegen im Monotoniebereich von Fritsch–Carlson
//! (`m / d ∈ [0, 3]` je Stück), also bleibt jedes Stück monoton. Bei zwei Punkten ist das Gesetz
//! linear.

/// Fehler beim Anlegen eines Radiusgesetzes.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum RadiusLawError {
    #[error("Ein variabler Radius braucht mindestens zwei Punkte")]
    TooFewPoints,
    #[error("Die Positionen müssen endlich sein und streng steigen")]
    Positions,
    #[error("Jeder Radius muss endlich und größer als null sein")]
    Radii,
}

/// Radius als Funktion des Anteils der Kettenlänge.
#[derive(Clone, Debug, PartialEq)]
pub struct RadiusLaw {
    s: Vec<f64>,
    r: Vec<f64>,
    /// Steigungen an den Punkten.
    m: Vec<f64>,
}

impl RadiusLaw {
    /// Gesetz durch die Punkte `(Anteil, Radius)`; Anteile endlich und streng steigend, Radien
    /// endlich und größer als null.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn new(points: &[(f64, f64)]) -> Result<RadiusLaw, RadiusLawError> {
        if points.len() < 2 {
            return Err(RadiusLawError::TooFewPoints);
        }
        let s: Vec<f64> = points.iter().map(|p| p.0).collect();
        let r: Vec<f64> = points.iter().map(|p| p.1).collect();
        if s.iter().any(|x| !x.is_finite()) || s.windows(2).any(|w| !(w[1] > w[0])) {
            return Err(RadiusLawError::Positions);
        }
        if r.iter().any(|x| !(x.is_finite() && *x > 0.0)) {
            return Err(RadiusLawError::Radii);
        }
        let n = s.len();
        let h: Vec<f64> = s.windows(2).map(|w| w[1] - w[0]).collect();
        let d: Vec<f64> = (0..n - 1).map(|k| (r[k + 1] - r[k]) / h[k]).collect();
        // Sehr dicht liegende Punkte mit großem Radiensprung ergeben eine Steigung, die nicht
        // mehr endlich ist; dann wäre das Gesetz unbrauchbar.
        if d.iter().any(|x| !x.is_finite()) {
            return Err(RadiusLawError::Positions);
        }
        let mut m = vec![0.0; n];
        m[0] = d[0];
        m[n - 1] = d[n - 2];
        for k in 1..n - 1 {
            m[k] = if d[k - 1] * d[k] <= 0.0 {
                0.0
            } else {
                // Gewichtetes harmonisches Mittel: höchstens das Dreifache der kleineren
                // Nachbarsteigung, damit bleibt jedes Stück monoton (Fritsch–Carlson-Bereich).
                3.0 * (h[k - 1] + h[k])
                    / ((2.0 * h[k] + h[k - 1]) / d[k - 1] + (h[k] + 2.0 * h[k - 1]) / d[k])
            };
        }
        Ok(RadiusLaw { s, r, m })
    }

    /// Positionen der Punkte (Anteile).
    pub fn positions(&self) -> &[f64] {
        &self.s
    }

    /// Radien der Punkte.
    pub fn radii(&self) -> &[f64] {
        &self.r
    }

    /// Radius beim Anteil `x` (außerhalb konstant; NaN ergibt den ersten Radius).
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn radius(&self, x: f64) -> f64 {
        let n = self.s.len();
        if !(x > self.s[0]) {
            return self.r[0];
        }
        if x >= self.s[n - 1] {
            return self.r[n - 1];
        }
        let k = self.s.partition_point(|&t| t <= x) - 1;
        let h = self.s[k + 1] - self.s[k];
        let t = (x - self.s[k]) / h;
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * self.r[k]
            + (t3 - 2.0 * t2 + t) * h * self.m[k]
            + (-2.0 * t3 + 3.0 * t2) * self.r[k + 1]
            + (t3 - t2) * h * self.m[k + 1]
    }
}
