//! Rationale B-Spline-Fläche (NURBS) mit Prüfung im Konstruktor, Ableitungen nach A4.4,
//! Knoteneinfügen und Zerlegung in rationale Bézier-Flicken.

use super::bezier::{cartesian, check_insertion, decompose_row, homogeneous, insert_row};
use super::curve::binom;
use super::{
    BezierPatch, KnotVector, MAX_CONTROL_POINTS, MAX_CONTROL_TOTAL, MAX_COORD, MAX_WEIGHT,
    MIN_WEIGHT, NurbsError,
};
use crate::DVec3;
use glam::DVec4;

/// Höchste Ableitungsordnung von `NurbsSurface::derivs` (harte Grenze; größere Werte werden
/// auf diese Ordnung begrenzt).
pub const MAX_SURFACE_DERIV: usize = 2;

/// Ist `|S_u × S_v|` kleiner als dieser Bruchteil von `|S_u|·|S_v|`, gilt die Normale als
/// entartet (Pol, zusammenfallende Kontrollpunkte) und `normal` liefert `DVec3::ZERO`.
const DEGENERATE_NORMAL_RATIO: f64 = 1e-14;
/// Untergrenze für das Produkt der Tangentenlängen im Entartungstest (verhindert 0 · Schranke).
const DEGENERATE_NORMAL_FLOOR: f64 = 1e-300;

#[derive(Clone, Debug, PartialEq)]
pub struct NurbsSurface {
    knots_u: KnotVector,
    knots_v: KnotVector,
    nu: usize,
    nv: usize,
    /// Kontrollpunkte, Index `i * nv + j` (`i` in u-, `j` in v-Richtung).
    control: Vec<DVec3>,
    weights: Vec<f64>,
}

impl NurbsSurface {
    /// Geprüfter Konstruktor; feindliche Daten ergeben einen Fehler, nie einen Absturz.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        degree_u: usize,
        degree_v: usize,
        knots_u: Vec<f64>,
        knots_v: Vec<f64>,
        nu: usize,
        nv: usize,
        control: Vec<DVec3>,
        weights: Vec<f64>,
    ) -> Result<Self, NurbsError> {
        let knots_u = KnotVector::new(degree_u, knots_u)?;
        let knots_v = KnotVector::new(degree_v, knots_v)?;
        if nu > MAX_CONTROL_POINTS || nv > MAX_CONTROL_POINTS {
            return Err(NurbsError::Control(format!(
                "zu viele Kontrollpunkte je Richtung (höchstens {MAX_CONTROL_POINTS})"
            )));
        }
        let total = nu
            .checked_mul(nv)
            .filter(|t| *t <= MAX_CONTROL_TOTAL)
            .ok_or_else(|| {
                NurbsError::Control(format!(
                    "zu viele Kontrollpunkte insgesamt (höchstens {MAX_CONTROL_TOTAL})"
                ))
            })?;
        if knots_u.basis_count() != nu || knots_v.basis_count() != nv {
            return Err(NurbsError::Knots(
                "die Knotenzahl passt nicht zu Kontrollpunkten und Grad".into(),
            ));
        }
        if control.len() != total {
            return Err(NurbsError::Control(format!(
                "erwartet {nu} × {nv} = {total} Kontrollpunkte, erhalten {}",
                control.len()
            )));
        }
        if weights.len() != total {
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
        Ok(NurbsSurface {
            knots_u,
            knots_v,
            nu,
            nv,
            control,
            weights,
        })
    }

    pub fn degree_u(&self) -> usize {
        self.knots_u.degree()
    }

    pub fn degree_v(&self) -> usize {
        self.knots_v.degree()
    }

    pub fn knots_u(&self) -> &KnotVector {
        &self.knots_u
    }

    pub fn knots_v(&self) -> &KnotVector {
        &self.knots_v
    }

    /// Anzahl der Kontrollpunkte `(nu, nv)`.
    pub fn counts(&self) -> (usize, usize) {
        (self.nu, self.nv)
    }

    pub fn control_at(&self, i: usize, j: usize) -> DVec3 {
        self.control[i * self.nv + j]
    }

    pub fn weight_at(&self, i: usize, j: usize) -> f64 {
        self.weights[i * self.nv + j]
    }

    /// `((u0, u1), (v0, v1))`.
    pub fn domain(&self) -> ((f64, f64), (f64, f64)) {
        (self.knots_u.domain(), self.knots_v.domain())
    }

    fn hom(&self, i: usize, j: usize) -> DVec4 {
        homogeneous(self.control_at(i, j), self.weight_at(i, j))
    }

    /// Punkt bei `(u, v)` (außerhalb begrenzt, nicht endliche Parameter ergeben den Anfang).
    pub fn point(&self, u: f64, v: f64) -> DVec3 {
        self.derivs(u, v, 0)[0][0]
    }

    /// `S[k][l] = ∂^(k+l) S / ∂u^k ∂v^l` für `k + l ≤ d` (übrige Einträge 0), nach A4.4:
    /// homogene Summen `A[k][l] = ΣΣ Nu^(k)·Nv^(l)·w·P`, `W[k][l] = ΣΣ Nu^(k)·Nv^(l)·w`, dann
    /// `S[k][l] = (A[k][l] − Σ_j C(l,j)·W[0][j]·S[k][l−j] − Σ_i C(k,i)·W[i][0]·S[k−i][l]
    ///            − Σ_i C(k,i) Σ_j C(l,j)·W[i][j]·S[k−i][l−j]) / W[0][0]`.
    /// `d` wird auf `MAX_SURFACE_DERIV` begrenzt.
    pub fn derivs(&self, u: f64, v: f64, d: usize) -> Vec<Vec<DVec3>> {
        let d = d.min(MAX_SURFACE_DERIV);
        let (p, q) = (self.degree_u(), self.degree_v());
        let ((u0, u1), (v0, v1)) = self.domain();
        let u = if u.is_finite() { u.clamp(u0, u1) } else { u0 };
        let v = if v.is_finite() { v.clamp(v0, v1) } else { v0 };
        let su = self.knots_u.find_span(u);
        let sv = self.knots_v.find_span(v);
        let nu = self.knots_u.basis_derivs(su, u, d);
        let nv = self.knots_v.basis_derivs(sv, v, d);

        // Homogene Ableitungen A (xyz) und W (w) gemeinsam als DVec4.
        let mut h = vec![vec![DVec4::ZERO; d + 1]; d + 1];
        for k in 0..=d {
            for l in 0..=d - k {
                let mut sum = DVec4::ZERO;
                for (a, nuk) in nu[k].iter().enumerate() {
                    let i = su - p + a;
                    let mut row = DVec4::ZERO;
                    for (b, nvl) in nv[l].iter().enumerate() {
                        row += self.hom(i, sv - q + b) * *nvl;
                    }
                    sum += row * *nuk;
                }
                h[k][l] = sum;
            }
        }
        let w = |k: usize, l: usize| h[k][l].w;
        let mut s = vec![vec![DVec3::ZERO; d + 1]; d + 1];
        for k in 0..=d {
            for l in 0..=d - k {
                let mut val = h[k][l].truncate();
                for j in 1..=l {
                    val -= s[k][l - j] * (binom(l, j) * w(0, j));
                }
                for i in 1..=k {
                    val -= s[k - i][l] * (binom(k, i) * w(i, 0));
                    let mut v2 = DVec3::ZERO;
                    for j in 1..=l {
                        v2 += s[k - i][l - j] * (binom(l, j) * w(i, j));
                    }
                    val -= v2 * binom(k, i);
                }
                s[k][l] = val / w(0, 0);
            }
        }
        s
    }

    /// Einheitsnormale `S_u × S_v`; `DVec3::ZERO` an entarteten Stellen (Pole).
    pub fn normal(&self, u: f64, v: f64) -> DVec3 {
        let d = self.derivs(u, v, 1);
        let (su, sv) = (d[1][0], d[0][1]);
        let n = su.cross(sv);
        let len = n.length();
        let limit =
            DEGENERATE_NORMAL_RATIO * (su.length() * sv.length()).max(DEGENERATE_NORMAL_FLOOR);
        if !len.is_finite() || len == 0.0 || len < limit {
            return DVec3::ZERO;
        }
        n / len
    }

    /// Affine Abbildung der Kontrollpunkte; die Prüfung läuft erneut.
    pub fn transformed(&self, f: impl Fn(DVec3) -> DVec3) -> Result<NurbsSurface, NurbsError> {
        NurbsSurface::new(
            self.degree_u(),
            self.degree_v(),
            self.knots_u.knots().to_vec(),
            self.knots_v.knots().to_vec(),
            self.nu,
            self.nv,
            self.control.iter().map(|p| f(*p)).collect(),
            self.weights.clone(),
        )
    }

    /// Dieselbe Fläche mit umgekehrter u-Richtung (gleicher Bereich); die Normale kehrt sich um.
    /// Fehler, wenn beim Spiegeln der Knoten Rundung Knoten verschmelzen ließe.
    pub fn reversed_u(&self) -> Result<NurbsSurface, NurbsError> {
        let knots_u = self.knots_u.mirrored()?;
        let mut control = Vec::with_capacity(self.control.len());
        let mut weights = Vec::with_capacity(self.weights.len());
        for i in (0..self.nu).rev() {
            for j in 0..self.nv {
                control.push(self.control_at(i, j));
                weights.push(self.weight_at(i, j));
            }
        }
        Ok(NurbsSurface {
            knots_u,
            knots_v: self.knots_v.clone(),
            nu: self.nu,
            nv: self.nv,
            control,
            weights,
        })
    }

    /// Homogene Kontrollpunkte als Gitter `[i][j]`.
    fn grid(&self) -> Vec<Vec<DVec4>> {
        (0..self.nu)
            .map(|i| (0..self.nv).map(|j| self.hom(i, j)).collect())
            .collect()
    }

    /// Baut aus einem homogenen Gitter `[i][j]` eine geprüfte Fläche.
    fn from_grid(
        degree_u: usize,
        degree_v: usize,
        knots_u: Vec<f64>,
        knots_v: Vec<f64>,
        grid: &[Vec<DVec4>],
    ) -> Result<NurbsSurface, NurbsError> {
        let nu = grid.len();
        let nv = grid.first().map_or(0, Vec::len);
        let (control, weights) = grid.iter().flatten().map(|h| cartesian(*h)).unzip();
        NurbsSurface::new(
            degree_u, degree_v, knots_u, knots_v, nu, nv, control, weights,
        )
    }

    /// Fügt den u-Knoten `u` `times`-mal ein (A5.3, homogen, spaltenweise); die Fläche ändert
    /// sich nicht. Fehler, wenn `u` nicht im Inneren liegt oder die Vielfachheit den Grad
    /// übersteigen würde.
    pub fn insert_knot_u(&self, u: f64, times: usize) -> Result<NurbsSurface, NurbsError> {
        let p = self.degree_u();
        let knots = self.knots_u.knots();
        check_insertion(knots, p, u, times)?;
        let grid = self.grid();
        let mut new_knots = Vec::new();
        let mut out = vec![vec![DVec4::ZERO; self.nv]; self.nu + times];
        for j in 0..self.nv {
            let col: Vec<DVec4> = grid.iter().map(|row| row[j]).collect();
            let (kn, qw) = insert_row(p, knots, &col, u, times);
            for (i, h) in qw.into_iter().enumerate() {
                out[i][j] = h;
            }
            new_knots = kn;
        }
        NurbsSurface::from_grid(
            p,
            self.degree_v(),
            new_knots,
            self.knots_v.knots().to_vec(),
            &out,
        )
    }

    /// Fügt den v-Knoten `v` `times`-mal ein (zeilenweise); sonst wie `insert_knot_u`.
    pub fn insert_knot_v(&self, v: f64, times: usize) -> Result<NurbsSurface, NurbsError> {
        let q = self.degree_v();
        let knots = self.knots_v.knots();
        check_insertion(knots, q, v, times)?;
        let mut new_knots = Vec::new();
        let mut out = Vec::with_capacity(self.nu);
        for row in self.grid() {
            let (kn, qw) = insert_row(q, knots, &row, v, times);
            out.push(qw);
            new_knots = kn;
        }
        NurbsSurface::from_grid(
            self.degree_u(),
            q,
            self.knots_u.knots().to_vec(),
            new_knots,
            &out,
        )
    }

    /// Zerlegung in rationale Bézier-Flicken: alle inneren Knoten werden bis Vielfachheit Grad
    /// eingefügt (erst u, dann v); je ein Flicken pro nichtleerem Knotenrechteck, Reihenfolge
    /// u-Abschnitt außen, v-Abschnitt innen.
    pub fn bezier_patches(&self) -> Vec<BezierPatch> {
        let (p, q) = (self.degree_u(), self.degree_v());
        let grid = self.grid();
        // u-Richtung: jede Spalte zerlegen.
        let mut breaks_u = Vec::new();
        let mut cols: Vec<Vec<DVec4>> = Vec::with_capacity(self.nv);
        for j in 0..self.nv {
            let col: Vec<DVec4> = grid.iter().map(|row| row[j]).collect();
            let (br, cp) = decompose_row(p, self.knots_u.knots(), &col);
            breaks_u = br;
            cols.push(cp);
        }
        let nu2 = cols.first().map_or(0, Vec::len);
        // v-Richtung: jede neue Zeile zerlegen.
        let mut breaks_v = Vec::new();
        let mut rows: Vec<Vec<DVec4>> = Vec::with_capacity(nu2);
        for i in 0..nu2 {
            let row: Vec<DVec4> = cols.iter().map(|c| c[i]).collect();
            let (br, cp) = decompose_row(q, self.knots_v.knots(), &row);
            breaks_v = br;
            rows.push(cp);
        }
        let mut patches = Vec::new();
        for (a, wu) in breaks_u.windows(2).enumerate() {
            for (b, wv) in breaks_v.windows(2).enumerate() {
                let mut control = Vec::with_capacity((p + 1) * (q + 1));
                let mut weights = Vec::with_capacity((p + 1) * (q + 1));
                for row in &rows[a * p..=a * p + p] {
                    for h in &row[b * q..=b * q + q] {
                        let (c, w) = cartesian(*h);
                        control.push(c);
                        weights.push(w);
                    }
                }
                patches.push(BezierPatch {
                    u: (wu[0], wu[1]),
                    v: (wv[0], wv[1]),
                    degree_u: p,
                    degree_v: q,
                    control,
                    weights,
                });
            }
        }
        patches
    }
}
