//! Gradwechsel von NURBS-Kurven: exakte Erhöhung und Absenkung per Ausgleich, dazu gleichmäßige
//! geklemmte Knotenvektoren.

use super::dense::solve_dense;
use super::{KnotVector, MAX_DEGREE, NurbsCurve, NurbsError};
use crate::DVec3;
use glam::DVec4;
use planora_i18n::tr;

/// Gleichmäßiger geklemmter Knotenvektor für `n` Kontrollpunkte vom Grad `degree`:
/// `[0; p+1], 1, …, n−p−1, [n−p; p+1]`. Bei `n ≤ p` (kein gültiger Vektor) entsteht ein Vektor
/// mit den Endknoten `0` und `1`, den `KnotVector::new` dann ablehnt.
pub fn uniform_clamped_knots(degree: usize, n: usize) -> Vec<f64> {
    let p = degree;
    let last = n.saturating_sub(p).max(1) as f64;
    let mut k = vec![0.0; p + 1];
    for i in 1..n.saturating_sub(p) {
        k.push(i as f64);
    }
    k.extend(std::iter::repeat_n(last, p + 1));
    k
}

/// Zeile der Kollokationsmatrix: Basisfunktionen am Parameter `u` als (erster Index, Werte).
fn basis_row(kv: &KnotVector, u: f64) -> (usize, Vec<f64>) {
    let span = kv.find_span(u);
    (span - kv.degree(), kv.basis(span, u))
}

impl NurbsCurve {
    /// Dieselbe Kurve mit Grad `to ≥ degree()`. Die inneren Knoten bekommen die Vielfachheit
    /// `m + (to − p)` (gleiche Stetigkeit), die Kontrollpunkte folgen aus der Interpolation an den
    /// Greville-Abszissen des neuen Knotenvektors. Gerechnet wird homogen, damit auch rationale
    /// Kurven exakt bleiben. Fehler, wenn `to` außerhalb `degree()..=MAX_DEGREE` liegt, die Kurve
    /// einen Sprung hat (Vielfachheit über dem Grad) oder das System zu groß wird.
    pub fn elevated(&self, to: usize) -> Result<NurbsCurve, NurbsError> {
        let p = self.degree();
        if to < p || to > MAX_DEGREE {
            return Err(NurbsError::Input(tr!(
                "math-elevate-degree",
                to = to.to_string(),
                p = p.to_string(),
                max = MAX_DEGREE.to_string()
            )));
        }
        if to == p {
            return Ok(self.clone());
        }
        let old = self.knots().knots();
        let (a, b) = self.domain();
        // Neuer Knotenvektor: Enden `to + 1`, innere Knoten um `to − p` häufiger.
        let mut knots = vec![a; to + 1];
        let mut i = p + 1;
        while i < old.len() - p - 1 {
            let u = old[i];
            let m = old[i..].iter().take_while(|k| **k == u).count();
            if m > p {
                return Err(NurbsError::Input(tr!("math-elevate-break")));
            }
            knots.extend(std::iter::repeat_n(u, m + to - p));
            i += m;
        }
        knots.extend(std::iter::repeat_n(b, to + 1));
        let kv = KnotVector::new(to, knots)?;
        let n = kv.basis_count();
        let t = kv.knots();
        // Homogene Werte der alten Kurve an den Greville-Abszissen, daraus Kollokation.
        let mut mat = vec![0.0; n * n];
        let mut rhs = vec![[0.0; 4]; n];
        for r in 0..n {
            let g = t[r + 1..=r + to].iter().sum::<f64>() / to as f64;
            let g = g.clamp(a, b);
            let (first, row) = basis_row(&kv, g);
            for (j, v) in row.iter().enumerate() {
                mat[r * n + first + j] = *v;
            }
            let span = self.knots().find_span(g);
            let nb = self.knots().basis(span, g);
            let mut h = DVec4::ZERO;
            for (j, v) in nb.iter().enumerate() {
                let idx = span - p + j;
                let w = self.weights()[idx];
                let c = self.control_points()[idx];
                h += DVec4::new(c.x * w, c.y * w, c.z * w, w) * *v;
            }
            rhs[r] = h.to_array();
        }
        let mut sol = vec![[0.0; 4]; n];
        for comp in 0..4 {
            let mut m = mat.clone();
            let mut bvec: Vec<f64> = rhs.iter().map(|h| h[comp]).collect();
            solve_dense(&mut m, &mut bvec, n)?;
            for (s, v) in sol.iter_mut().zip(bvec) {
                s[comp] = v;
            }
        }
        let weights: Vec<f64> = sol.iter().map(|h| h[3]).collect();
        let control: Vec<DVec3> = sol
            .iter()
            .map(|h| DVec3::new(h[0], h[1], h[2]) / h[3])
            .collect();
        NurbsCurve::new(to, kv.knots().to_vec(), control, weights)
    }

    /// Näherung vom Grad `to` mit `control_count` Kontrollpunkten auf gleichmäßigen Knoten (auf den
    /// Parameterbereich der Kurve abgebildet), nicht rational, nach der Methode der kleinsten
    /// Quadrate; erster und letzter Kontrollpunkt sind fest (die Enden der Kurve). Fehler bei
    /// `to` außerhalb `1..=MAX_DEGREE` oder `control_count < to + 1`.
    pub fn reduced(&self, to: usize, control_count: usize) -> Result<NurbsCurve, NurbsError> {
        if to == 0 || to > MAX_DEGREE || control_count < to + 1 {
            return Err(NurbsError::Input(tr!(
                "math-reduce-args",
                to = to.to_string(),
                count = control_count.to_string(),
                max = MAX_DEGREE.to_string()
            )));
        }
        let (a, b) = self.domain();
        let n = control_count;
        let unit = uniform_clamped_knots(to, n);
        let span_len = unit[unit.len() - 1];
        let mut knots: Vec<f64> = unit.iter().map(|k| a + k / span_len * (b - a)).collect();
        // Die Enden genau treffen (Rundung bei `a + 1·(b − a)`).
        let len = knots.len();
        knots[..=to].fill(a);
        knots[len - to - 1..].fill(b);
        let kv = KnotVector::new(to, knots)?;
        let p0 = self.point(a);
        let p1 = self.point(b);
        let mut control = vec![DVec3::ZERO; n];
        control[0] = p0;
        control[n - 1] = p1;
        let k = n - 2;
        if k > 0 {
            let samples = 16 * n + 1;
            let mut mat = vec![0.0; k * k];
            let mut rhs = vec![DVec3::ZERO; k];
            for s in 0..samples {
                let u = if s + 1 == samples {
                    b
                } else {
                    a + (b - a) * s as f64 / (samples - 1) as f64
                };
                let (first, row) = basis_row(&kv, u);
                let at = |i: usize| -> f64 {
                    if i >= first && i < first + row.len() {
                        row[i - first]
                    } else {
                        0.0
                    }
                };
                let target = self.point(u) - p0 * at(0) - p1 * at(n - 1);
                for (j, nj) in row.iter().enumerate() {
                    let i = first + j;
                    if i == 0 || i == n - 1 {
                        continue;
                    }
                    rhs[i - 1] += target * *nj;
                    for (l, nl) in row.iter().enumerate() {
                        let c = first + l;
                        if c == 0 || c == n - 1 {
                            continue;
                        }
                        mat[(i - 1) * k + (c - 1)] += nj * nl;
                    }
                }
            }
            for comp in 0..3 {
                let mut m = mat.clone();
                let mut bvec: Vec<f64> = rhs.iter().map(|v| v[comp]).collect();
                solve_dense(&mut m, &mut bvec, k)?;
                for (i, v) in bvec.into_iter().enumerate() {
                    control[i + 1][comp] = v;
                }
            }
        }
        NurbsCurve::new(to, kv.knots().to_vec(), control, vec![1.0; n])
    }
}
