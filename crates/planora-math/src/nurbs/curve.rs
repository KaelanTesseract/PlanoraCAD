//! Rationale B-Spline-Kurve (NURBS) mit vollständiger Prüfung im Konstruktor.

use super::{
    KnotVector, MAX_CONTROL_POINTS, MAX_COORD, MAX_DERIV_ORDER, MAX_WEIGHT, MIN_WEIGHT, NurbsError,
};
use crate::DVec3;
use planora_i18n::tr;

#[derive(Clone, Debug, PartialEq)]
pub struct NurbsCurve {
    knots: KnotVector,
    control: Vec<DVec3>,
    weights: Vec<f64>,
}

/// Binomialkoeffizient (kleine Werte, in `f64`).
pub(super) fn binom(n: usize, k: usize) -> f64 {
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
            return Err(NurbsError::Control(tr!(
                "math-control-too-many",
                max = MAX_CONTROL_POINTS.to_string()
            )));
        }
        // Zu wenige Kontrollpunkte fängt der Vergleich mit der Knotenzahl unten ab
        // (KnotVector::new verlangt mindestens 2*(Grad+1) Knoten).
        if knots.basis_count() != control.len() {
            return Err(NurbsError::Knots(tr!("math-knot-count")));
        }
        if weights.len() != control.len() {
            return Err(NurbsError::Weights(tr!("math-weights-count")));
        }
        if weights
            .iter()
            .any(|w| !(MIN_WEIGHT..=MAX_WEIGHT).contains(w))
        {
            return Err(NurbsError::Weights(tr!(
                "math-weights-range",
                min = MIN_WEIGHT.to_string(),
                max = MAX_WEIGHT.to_string()
            )));
        }
        if control
            .iter()
            .any(|p| !p.is_finite() || p.abs().max_element() > MAX_COORD)
        {
            return Err(NurbsError::Control(tr!(
                "math-coords-range",
                max = MAX_COORD.to_string()
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
    /// `n` wird auf `MAX_DERIV_ORDER` begrenzt; das Ergebnis hat `min(n, MAX_DERIV_ORDER) + 1`
    /// Einträge.
    pub fn derivs(&self, u: f64, n: usize) -> Vec<DVec3> {
        super::work::count();
        let n = n.min(MAX_DERIV_ORDER);
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
    /// Fehler, wenn beim Spiegeln der Knoten Rundung Knoten verschmelzen ließe.
    pub fn reversed(&self) -> Result<NurbsCurve, NurbsError> {
        let knots = self.knots.mirrored()?;
        Ok(NurbsCurve {
            knots,
            control: self.control.iter().rev().copied().collect(),
            weights: self.weights.iter().rev().copied().collect(),
        })
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
