//! Geklemmte Knotenvektoren mit Basisfunktionen und deren Ableitungen
//! (Algorithmen A2.1 bis A2.3 nach Piegl/Tiller).

use super::{
    MAX_DEGREE, MAX_DERIV_ORDER, MAX_KNOT_MAGNITUDE, MIN_KNOT_DOMAIN, MIN_KNOT_GAP_RATIO,
    NurbsError,
};

#[derive(Clone, Debug, PartialEq)]
pub struct KnotVector {
    degree: usize,
    knots: Vec<f64>,
}

impl KnotVector {
    /// Geklemmter Knotenvektor: die ersten und letzten `degree + 1` Knoten sind gleich.
    pub fn new(degree: usize, knots: Vec<f64>) -> Result<Self, NurbsError> {
        if degree == 0 || degree > MAX_DEGREE {
            return Err(NurbsError::Degree(degree));
        }
        if knots.len() < 2 * (degree + 1) {
            return Err(NurbsError::Knots("zu wenige Knoten".into()));
        }
        if knots.iter().any(|k| !k.is_finite()) {
            return Err(NurbsError::Knots("nicht endliche Zahl".into()));
        }
        if knots.windows(2).any(|w| w[1] < w[0]) {
            return Err(NurbsError::Knots("Knoten fallen".into()));
        }
        let (n, p) = (knots.len(), degree);
        let clamped = knots[..=p].iter().all(|k| *k == knots[0])
            && knots[n - p - 1..].iter().all(|k| *k == knots[n - 1]);
        if !clamped {
            return Err(NurbsError::Knots(
                "nicht geklemmt (Anfang und Ende brauchen Grad+1 gleiche Knoten)".into(),
            ));
        }
        if knots[n - 1] <= knots[0] {
            return Err(NurbsError::Knots("der Parameterbereich ist leer".into()));
        }
        // Anfang und Ende dürfen nicht mehr als Grad+1 gleiche Knoten haben,
        // sonst gäbe es leere Randabschnitte.
        if knots[p + 1] <= knots[p] || knots[n - p - 2] >= knots[n - p - 1] {
            return Err(NurbsError::Knots(
                "Anfang oder Ende hat mehr als Grad+1 gleiche Knoten".into(),
            ));
        }
        let length = knots[n - 1] - knots[0];
        if knots.iter().any(|k| k.abs() > MAX_KNOT_MAGNITUDE) || length > MAX_KNOT_MAGNITUDE {
            return Err(NurbsError::Knots(format!(
                "Knoten und Parameterbereich dürfen höchstens {MAX_KNOT_MAGNITUDE} betragen"
            )));
        }
        if length < MIN_KNOT_DOMAIN {
            return Err(NurbsError::Knots(format!(
                "der Parameterbereich ist zu klein (mindestens {MIN_KNOT_DOMAIN})"
            )));
        }
        let min_gap = knots
            .windows(2)
            .map(|w| w[1] - w[0])
            .filter(|g| *g > 0.0)
            .fold(f64::INFINITY, f64::min);
        if min_gap < MIN_KNOT_GAP_RATIO * length {
            return Err(NurbsError::Knots(
                "zwei verschiedene Knoten liegen zu nah beieinander".into(),
            ));
        }
        // Keine Mehrfachknoten über dem Grad im Inneren (sonst unstetig/entartet).
        let mut run = 1;
        for w in knots[p + 1..n - p - 1].windows(2) {
            run = if w[1] == w[0] { run + 1 } else { 1 };
            if run > p {
                return Err(NurbsError::Knots("innerer Knoten zu oft wiederholt".into()));
            }
        }
        Ok(KnotVector { degree, knots })
    }

    pub fn degree(&self) -> usize {
        self.degree
    }

    pub fn knots(&self) -> &[f64] {
        &self.knots
    }

    pub fn basis_count(&self) -> usize {
        self.knots.len() - self.degree - 1
    }

    pub fn domain(&self) -> (f64, f64) {
        (
            self.knots[self.degree],
            self.knots[self.knots.len() - self.degree - 1],
        )
    }

    /// Spiegelt den Knotenvektor am Parameterbereich (`a + b − k`, rückwärts). Fehler, wenn
    /// Rundung Knoten verschmelzen lässt (dann ist der Vektor nicht mehr gültig).
    pub fn mirrored(&self) -> Result<KnotVector, NurbsError> {
        let (a, b) = self.domain();
        let mut knots: Vec<f64> = self.knots.iter().rev().map(|k| a + b - k).collect();
        // Die Ränder sind exakt (Rundung darf die Klemmung nicht stören).
        let (n, p) = (knots.len(), self.degree);
        for k in &mut knots[..=p] {
            *k = a;
        }
        for k in &mut knots[n - p - 1..] {
            *k = b;
        }
        KnotVector::new(self.degree, knots)
    }

    /// Index `i` mit `knots[i] <= u < knots[i+1]` (am rechten Rand der letzte nichtleere Abschnitt);
    /// `u` wird auf den Bereich begrenzt, ein nicht endliches `u` liefert den ersten Abschnitt.
    pub fn find_span(&self, u: f64) -> usize {
        let p = self.degree;
        let n = self.knots.len() - p - 2; // Index der letzten Basisfunktion
        if !u.is_finite() {
            return p;
        }
        let (a, b) = self.domain();
        let u = u.clamp(a, b);
        if u >= self.knots[n + 1] {
            return n;
        }
        let (mut lo, mut hi) = (p, n + 1);
        let mut mid = (lo + hi) / 2;
        while u < self.knots[mid] || u >= self.knots[mid + 1] {
            if u < self.knots[mid] {
                hi = mid
            } else {
                lo = mid
            }
            mid = (lo + hi) / 2;
        }
        mid
    }

    /// Die `degree + 1` nichtverschwindenden Basisfunktionen am Abschnitt `span` (A2.2).
    /// `span` sollte von `find_span` stammen; Werte außerhalb des gültigen Bereichs
    /// `degree..=basis_count()-1` werden darauf begrenzt (kein Absturz).
    pub fn basis(&self, span: usize, u: f64) -> Vec<f64> {
        let p = self.degree;
        let span = span.clamp(p, self.knots.len() - p - 2);
        let mut n = vec![0.0; p + 1];
        let mut left = vec![0.0; p + 1];
        let mut right = vec![0.0; p + 1];
        n[0] = 1.0;
        for j in 1..=p {
            left[j] = u - self.knots[span + 1 - j];
            right[j] = self.knots[span + j] - u;
            let mut saved = 0.0;
            for r in 0..j {
                let den = right[r + 1] + left[j - r];
                let temp = if den != 0.0 { n[r] / den } else { 0.0 };
                n[r] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            n[j] = saved;
        }
        n
    }

    /// Ableitungen der Basisfunktionen bis zur Ordnung `nd` (A2.3); Zeilen jenseits des Grades sind 0.
    /// `nd` wird auf `MAX_DERIV_ORDER` begrenzt, das Ergebnis hat `min(nd, MAX_DERIV_ORDER) + 1`
    /// Zeilen. `span` wie bei `basis` (wird bei Bedarf begrenzt).
    pub fn basis_derivs(&self, span: usize, u: f64, nd: usize) -> Vec<Vec<f64>> {
        let p = self.degree;
        let span = span.clamp(p, self.knots.len() - p - 2);
        let nd = nd.min(MAX_DERIV_ORDER);
        let mut ders = vec![vec![0.0; p + 1]; nd + 1];
        let k_max = nd.min(p);
        let mut ndu = vec![vec![0.0; p + 1]; p + 1];
        let mut left = vec![0.0; p + 1];
        let mut right = vec![0.0; p + 1];
        ndu[0][0] = 1.0;
        for j in 1..=p {
            left[j] = u - self.knots[span + 1 - j];
            right[j] = self.knots[span + j] - u;
            let mut saved = 0.0;
            for r in 0..j {
                ndu[j][r] = right[r + 1] + left[j - r];
                let temp = if ndu[j][r] != 0.0 {
                    ndu[r][j - 1] / ndu[j][r]
                } else {
                    0.0
                };
                ndu[r][j] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            ndu[j][j] = saved;
        }
        for j in 0..=p {
            ders[0][j] = ndu[j][p];
        }
        let mut a = vec![vec![0.0; p + 1]; 2];
        for r in 0..=p {
            let (mut s1, mut s2) = (0usize, 1usize);
            a[0][0] = 1.0;
            for k in 1..=k_max {
                let mut d = 0.0;
                let rk = r as isize - k as isize;
                let pk = (p - k) as isize;
                if r >= k {
                    a[s2][0] = a[s1][0] / ndu[(pk + 1) as usize][rk as usize];
                    d = a[s2][0] * ndu[rk as usize][pk as usize];
                }
                let j1 = if rk >= -1 { 1 } else { (-rk) as usize };
                let j2 = if (r as isize - 1) <= pk { k - 1 } else { p - r };
                for j in j1..=j2 {
                    a[s2][j] = (a[s1][j] - a[s1][j - 1])
                        / ndu[(pk + 1) as usize][(rk + j as isize) as usize];
                    d += a[s2][j] * ndu[(rk + j as isize) as usize][pk as usize];
                }
                if r as isize <= pk {
                    a[s2][k] = -a[s1][k - 1] / ndu[(pk + 1) as usize][r];
                    d += a[s2][k] * ndu[r][pk as usize];
                }
                ders[k][r] = d;
                std::mem::swap(&mut s1, &mut s2);
            }
        }
        let mut f = p as f64;
        for (k, row) in ders.iter_mut().enumerate().take(k_max + 1).skip(1) {
            for v in row.iter_mut() {
                *v *= f;
            }
            f *= (p - k) as f64;
        }
        ders
    }
}
