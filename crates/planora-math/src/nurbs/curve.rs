//! Rationale B-Spline-Kurve (NURBS) mit vollständiger Prüfung im Konstruktor.

use super::{KnotVector, MAX_CONTROL_POINTS, MAX_COORD, MAX_WEIGHT, MIN_WEIGHT, NurbsError};
use crate::DVec3;

#[derive(Clone, Debug, PartialEq)]
pub struct NurbsCurve {
    knots: KnotVector,
    control: Vec<DVec3>,
    weights: Vec<f64>,
}

/// Binomialkoeffizient (kleine Werte, in `f64`).
fn binom(n: usize, k: usize) -> f64 {
    let mut r = 1.0;
    for i in 0..k {
        r = r * (n - i) as f64 / (i + 1) as f64;
    }
    r
}

impl NurbsCurve {
    /// Geprüfter Konstruktor; feindliche Daten ergeben einen Fehler, nie einen Absturz.
    pub fn new(
        degree: usize,
        knots: Vec<f64>,
        control: Vec<DVec3>,
        weights: Vec<f64>,
    ) -> Result<Self, NurbsError> {
        let knots = KnotVector::new(degree, knots)?;
        if control.len() > MAX_CONTROL_POINTS {
            return Err(NurbsError::Control(format!(
                "zu viele Kontrollpunkte (höchstens {MAX_CONTROL_POINTS})"
            )));
        }
        if control.len() < degree + 1 {
            return Err(NurbsError::Control(format!(
                "mindestens {} Kontrollpunkte nötig",
                degree + 1
            )));
        }
        if knots.basis_count() != control.len() {
            return Err(NurbsError::Knots(
                "die Knotenzahl passt nicht zu Kontrollpunkten und Grad".into(),
            ));
        }
        if weights.len() != control.len() {
            return Err(NurbsError::Weights(
                "Gewichte und Kontrollpunkte haben verschiedene Anzahl".into(),
            ));
        }
        if weights
            .iter()
            .any(|w| !(MIN_WEIGHT..=MAX_WEIGHT).contains(w))
        {
            return Err(NurbsError::Weights(format!(
                "Gewichte müssen zwischen {MIN_WEIGHT} und {MAX_WEIGHT} liegen"
            )));
        }
        if control
            .iter()
            .any(|p| !p.is_finite() || p.abs().max_element() > MAX_COORD)
        {
            return Err(NurbsError::Control(format!(
                "Koordinaten müssen endlich sein und höchstens {MAX_COORD} betragen"
            )));
        }
        Ok(NurbsCurve {
            knots,
            control,
            weights,
        })
    }

    pub fn degree(&self) -> usize {
        self.knots.degree()
    }

    pub fn knots(&self) -> &KnotVector {
        &self.knots
    }

    pub fn control_points(&self) -> &[DVec3] {
        &self.control
    }

    pub fn weights(&self) -> &[f64] {
        &self.weights
    }

    pub fn domain(&self) -> (f64, f64) {
        self.knots.domain()
    }

    /// Punkt bei Parameter `u` (außerhalb begrenzt, nicht endliches `u` ergibt den Anfang).
    pub fn point(&self, u: f64) -> DVec3 {
        self.derivs(u, 0)[0]
    }

    /// C, C', C'', ... (`n + 1` Einträge) über homogene Ableitungen (A4.2):
    /// C^(k) = ( A^(k) − Σ_{i=1..k} binom(k,i) · w^(i) · C^(k−i) ) / w^(0),
    /// mit A = Σ N·w·P und w = Σ N·w.
    pub fn derivs(&self, u: f64, n: usize) -> Vec<DVec3> {
        let p = self.knots.degree();
        let (a, b) = self.knots.domain();
        let u = if u.is_finite() { u.clamp(a, b) } else { a };
        let span = self.knots.find_span(u);
        let nd = self.knots.basis_derivs(span, u, n);
        let mut av = vec![DVec3::ZERO; n + 1];
        let mut wv = vec![0.0; n + 1];
        for (k, row) in nd.iter().enumerate() {
            for (j, d) in row.iter().enumerate() {
                let i = span - p + j;
                let w = self.weights[i] * d;
                av[k] += self.control[i] * w;
                wv[k] += w;
            }
        }
        let mut c = vec![DVec3::ZERO; n + 1];
        for k in 0..=n {
            let mut v = av[k];
            for i in 1..=k {
                v -= c[k - i] * (binom(k, i) * wv[i]);
            }
            c[k] = v / wv[0];
        }
        c
    }

    pub fn tangent(&self, u: f64) -> DVec3 {
        self.derivs(u, 1)[1]
    }

    /// Anfang und Ende fallen innerhalb von `tol` zusammen.
    pub fn is_closed(&self, tol: f64) -> bool {
        let (a, b) = self.domain();
        self.point(a).distance(self.point(b)) <= tol
    }

    /// Dieselbe Form in umgekehrter Richtung (gleicher Parameterbereich).
    pub fn reversed(&self) -> NurbsCurve {
        let (a, b) = self.domain();
        let knots: Vec<f64> = self.knots.knots().iter().rev().map(|k| a + b - k).collect();
        let knots = KnotVector::new(self.degree(), knots)
            .expect("gespiegelter Knotenvektor eines gültigen Vektors ist gültig");
        NurbsCurve {
            knots,
            control: self.control.iter().rev().copied().collect(),
            weights: self.weights.iter().rev().copied().collect(),
        }
    }

    /// Affine Abbildung der Kontrollpunkte; die Prüfung läuft erneut.
    pub fn transformed(&self, f: impl Fn(DVec3) -> DVec3) -> Result<NurbsCurve, NurbsError> {
        NurbsCurve::new(
            self.degree(),
            self.knots.knots().to_vec(),
            self.control.iter().map(|p| f(*p)).collect(),
            self.weights.clone(),
        )
    }
}
