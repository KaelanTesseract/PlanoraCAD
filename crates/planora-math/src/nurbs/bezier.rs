//! Knoteneinfügen (A5.1 nach Piegl/Tiller) in homogenen Koordinaten und die Zerlegung
//! von NURBS-Kurven und -Flächen in rationale Bézier-Stücke bzw. -Flicken.

use super::{NurbsCurve, NurbsError};
use crate::DVec3;
use glam::DVec4;

/// Höchstzahl Jacobi-Durchläufe für die Eigenrichtungen der 3×3-Kovarianz (harte Grenze).
const JACOBI_MAX_SWEEPS: usize = 32;

/// Homogener Punkt `(w·P, w)`.
pub(super) fn homogeneous(p: DVec3, w: f64) -> DVec4 {
    (p * w).extend(w)
}

/// Kartesischer Punkt und Gewicht aus `(w·P, w)`.
pub(super) fn cartesian(h: DVec4) -> (DVec3, f64) {
    (h.truncate() / h.w, h.w)
}

/// Abschnitt `k` (letzter Index mit `knots[k] <= u`) und Vielfachheit `s` von `u` im Knotenvektor;
/// nur für innere Parameter `knots[p] < u < knots[m - p]` aufgerufen.
fn span_and_multiplicity(knots: &[f64], p: usize, u: f64) -> (usize, usize) {
    let last = knots.len() - p - 2; // höchster zulässiger Abschnitt
    let mut k = p;
    while k < last && knots[k + 1] <= u {
        k += 1;
    }
    let s = knots.iter().filter(|x| **x == u).count();
    (k, s)
}

/// Prüft ein Einfügen von `u` mit `r` Wiederholungen: `u` muss im Inneren des Bereichs liegen
/// und die neue Vielfachheit `s + r` darf den Grad nicht übersteigen.
pub(super) fn check_insertion(knots: &[f64], p: usize, u: f64, r: usize) -> Result<(), NurbsError> {
    let (a, b) = (knots[p], knots[knots.len() - p - 1]);
    if !u.is_finite() || u <= a || u >= b {
        return Err(NurbsError::Input(format!(
            "der Knoten {u} liegt nicht im Inneren des Bereichs ({a}, {b})"
        )));
    }
    let (_, s) = span_and_multiplicity(knots, p, u);
    // `r > p` zuerst prüfen: so kann `s + r` (mit `s ≤ p`) nicht überlaufen.
    if r > p || s + r > p {
        return Err(NurbsError::Knots(format!(
            "{r}-faches Einfügen des Knotens {u} (bisher {s}-fach) ergäbe eine Vielfachheit über dem Grad {p}"
        )));
    }
    Ok(())
}

/// Fügt `u` `r`-mal in eine Reihe homogener Kontrollpunkte ein (A5.1). Voraussetzung:
/// `check_insertion` war erfolgreich.
pub(super) fn insert_row(
    p: usize,
    knots: &[f64],
    pw: &[DVec4],
    u: f64,
    r: usize,
) -> (Vec<f64>, Vec<DVec4>) {
    if r == 0 {
        return (knots.to_vec(), pw.to_vec());
    }
    let (k, s) = span_and_multiplicity(knots, p, u);
    let n = pw.len() - 1;
    let mut uq = Vec::with_capacity(knots.len() + r);
    uq.extend_from_slice(&knots[..=k]);
    uq.extend(std::iter::repeat_n(u, r));
    uq.extend_from_slice(&knots[k + 1..]);

    let mut qw = vec![DVec4::ZERO; n + 1 + r];
    qw[..=k - p].copy_from_slice(&pw[..=k - p]);
    qw[k - s + r..=n + r].copy_from_slice(&pw[k - s..=n]);
    let mut rw: Vec<DVec4> = pw[k - p..=k - s].to_vec();
    for j in 1..=r {
        let l = k - p + j;
        for i in 0..=p - j - s {
            let alpha = (u - knots[l + i]) / (knots[i + k + 1] - knots[l + i]);
            rw[i] = rw[i + 1] * alpha + rw[i] * (1.0 - alpha);
        }
        qw[l] = rw[0];
        qw[k + r - j - s] = rw[p - j - s];
    }
    let l = k - p + r;
    if l + 1 < k - s {
        qw[l + 1..k - s].copy_from_slice(&rw[1..k - s - l]);
    }
    (uq, qw)
}

/// Fügt alle inneren Knoten bis zur Vielfachheit `p` ein. Ergebnis: die Bruchstellen
/// (verschiedene Knotenwerte von Anfang bis Ende) und die Kontrollpunkte, in denen je
/// `p + 1` aufeinanderfolgende (Überlapp um einen) ein Bézier-Stück bilden.
pub(super) fn decompose_row(p: usize, knots: &[f64], pw: &[DVec4]) -> (Vec<f64>, Vec<DVec4>) {
    // A5.6 (DecomposeCurve) nach Piegl/Tiller: jedes Stück wird nur aus seinen p + 1 örtlichen
    // Kontrollpunkten gebildet, Aufwand O(Stücke · p²) statt eines Einfügens über den ganzen
    // Vektor je Knoten (das war bei Tausenden Kontrollpunkten quadratisch).
    let mut breaks: Vec<f64> = Vec::new();
    for &k in knots {
        if breaks.last() != Some(&k) {
            breaks.push(k);
        }
    }
    let m = knots.len() - 1;
    let mut pieces: Vec<Vec<DVec4>> = vec![pw[..=p].to_vec()];
    let mut alphas = vec![0.0; p.max(1)];
    let (mut a, mut b) = (p, p + 1);
    while b < m {
        let i = b;
        while b < m && knots[b + 1] == knots[b] {
            b += 1;
        }
        let mult = b - i + 1;
        let nb = pieces.len() - 1;
        if mult < p {
            let numer = knots[b] - knots[a];
            for j in (mult + 1..=p).rev() {
                alphas[j - mult - 1] = numer / (knots[a + j] - knots[a]);
            }
            let r = p - mult;
            let mut next = vec![DVec4::ZERO; p + 1];
            for j in 1..=r {
                let save = r - j;
                let s = mult + j;
                for k in (s..=p).rev() {
                    let alpha = alphas[k - s];
                    pieces[nb][k] = pieces[nb][k] * alpha + pieces[nb][k - 1] * (1.0 - alpha);
                }
                if b < m {
                    next[save] = pieces[nb][p];
                }
            }
            if b < m {
                for (k, slot) in next.iter_mut().enumerate().take(p + 1).skip(p - mult) {
                    *slot = pw[b - p + k];
                }
                pieces.push(next);
            }
        } else if b < m {
            pieces.push((0..=p).map(|k| pw[b - p + k]).collect());
        }
        if b < m {
            a = b;
            b += 1;
        }
    }
    let mut cp = Vec::with_capacity(pieces.len() * p + 1);
    for (k, piece) in pieces.iter().enumerate() {
        cp.extend_from_slice(if k == 0 { &piece[..] } else { &piece[1..] });
    }
    (breaks, cp)
}

/// De Casteljau bei `t = 0.5` für homogene Punkte: linke und rechte Hälfte.
fn split_row_half(row: &[DVec4]) -> (Vec<DVec4>, Vec<DVec4>) {
    let n = row.len();
    let mut work = row.to_vec();
    let mut left = Vec::with_capacity(n);
    let mut right = vec![DVec4::ZERO; n];
    left.push(work[0]);
    right[n - 1] = work[n - 1];
    for level in 1..n {
        for i in 0..n - level {
            work[i] = (work[i] + work[i + 1]) * 0.5;
        }
        left.push(work[0]);
        right[n - 1 - level] = work[n - 1 - level];
    }
    (left, right)
}

/// Hüllquader von Punkten.
fn bounds_of(points: &[DVec3]) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for p in points {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    (lo, hi)
}

/// Rationales Bézier-Kurvenstück über dem Parameterbereich `u` der Ausgangskurve.
#[derive(Clone, Debug, PartialEq)]
pub struct BezierSegment {
    pub u: (f64, f64),
    pub degree: usize,
    pub control: Vec<DVec3>,
    pub weights: Vec<f64>,
}

impl BezierSegment {
    fn homogeneous(&self) -> Vec<DVec4> {
        self.control
            .iter()
            .zip(&self.weights)
            .map(|(p, w)| homogeneous(*p, *w))
            .collect()
    }

    fn from_homogeneous(u: (f64, f64), degree: usize, h: &[DVec4]) -> BezierSegment {
        let (control, weights) = h.iter().map(|q| cartesian(*q)).unzip();
        BezierSegment {
            u,
            degree,
            control,
            weights,
        }
    }

    /// Hüllquader der Kontrollpunkte (enthält das Stück, weil alle Gewichte positiv sind).
    pub fn bounds(&self) -> (DVec3, DVec3) {
        bounds_of(&self.control)
    }

    /// Teilung in der Parametermitte (de Casteljau in homogenen Koordinaten).
    pub fn split_half(&self) -> [BezierSegment; 2] {
        let (l, r) = split_row_half(&self.homogeneous());
        let m = 0.5 * (self.u.0 + self.u.1);
        [
            BezierSegment::from_homogeneous((self.u.0, m), self.degree, &l),
            BezierSegment::from_homogeneous((m, self.u.1), self.degree, &r),
        ]
    }
}

/// Rationaler Bézier-Flicken über `u × v` der Ausgangsfläche; Index `i*(degree_v+1) + j`.
#[derive(Clone, Debug, PartialEq)]
pub struct BezierPatch {
    pub u: (f64, f64),
    pub v: (f64, f64),
    pub degree_u: usize,
    pub degree_v: usize,
    pub control: Vec<DVec3>,
    pub weights: Vec<f64>,
}

impl BezierPatch {
    fn homogeneous(&self) -> Vec<DVec4> {
        self.control
            .iter()
            .zip(&self.weights)
            .map(|(p, w)| homogeneous(*p, *w))
            .collect()
    }

    fn from_homogeneous(
        u: (f64, f64),
        v: (f64, f64),
        degree_u: usize,
        degree_v: usize,
        h: &[DVec4],
    ) -> BezierPatch {
        let (control, weights) = h.iter().map(|q| cartesian(*q)).unzip();
        BezierPatch {
            u,
            v,
            degree_u,
            degree_v,
            control,
            weights,
        }
    }

    /// Hüllquader aller kartesischen Kontrollpunkte (enthält den Flicken, weil alle Gewichte
    /// positiv sind: die Fläche liegt in der konvexen Hülle).
    pub fn bounds(&self) -> (DVec3, DVec3) {
        bounds_of(&self.control)
    }

    /// Vier Teilflicken durch de Casteljau an u- und v-Mitte (homogen), Reihenfolge:
    /// (u unten, v unten), (u unten, v oben), (u oben, v unten), (u oben, v oben).
    pub fn subdivide(&self) -> [BezierPatch; 4] {
        let (nu, nv) = (self.degree_u + 1, self.degree_v + 1);
        let h = self.homogeneous();
        // Erst in u: jede Spalte j (feste v-Stelle) ist eine Bézier-Kurve in u.
        let mut lo_u = vec![DVec4::ZERO; nu * nv];
        let mut hi_u = vec![DVec4::ZERO; nu * nv];
        for j in 0..nv {
            let col: Vec<DVec4> = (0..nu).map(|i| h[i * nv + j]).collect();
            let (l, r) = split_row_half(&col);
            for i in 0..nu {
                lo_u[i * nv + j] = l[i];
                hi_u[i * nv + j] = r[i];
            }
        }
        // Dann in v: jede Zeile i ist eine Bézier-Kurve in v.
        let split_v = |g: &[DVec4]| {
            let mut lo = Vec::with_capacity(nu * nv);
            let mut hi = Vec::with_capacity(nu * nv);
            for i in 0..nu {
                let (l, r) = split_row_half(&g[i * nv..(i + 1) * nv]);
                lo.extend(l);
                hi.extend(r);
            }
            (lo, hi)
        };
        let (a, b) = split_v(&lo_u);
        let (c, d) = split_v(&hi_u);
        let um = 0.5 * (self.u.0 + self.u.1);
        let vm = 0.5 * (self.v.0 + self.v.1);
        let (du, dv) = (self.degree_u, self.degree_v);
        [
            BezierPatch::from_homogeneous((self.u.0, um), (self.v.0, vm), du, dv, &a),
            BezierPatch::from_homogeneous((self.u.0, um), (vm, self.v.1), du, dv, &b),
            BezierPatch::from_homogeneous((um, self.u.1), (self.v.0, vm), du, dv, &c),
            BezierPatch::from_homogeneous((um, self.u.1), (vm, self.v.1), du, dv, &d),
        ]
    }

    /// Ebenheitsmaß: größter Abstand der kartesischen Kontrollpunkte zur besten Ebene.
    ///
    /// Kandidaten für die Ebenennormale sind die kleinste Eigenrichtung der Kovarianz der
    /// Kontrollpunkte (Hauptachse) und das Kreuzprodukt der beiden Eckdiagonalen. Je Normale
    /// wird die Ebene mittig zwischen die äußersten Punkte gelegt (halbe Ausdehnung längs der
    /// Normale); das Maß ist das Minimum über die Kandidaten. Für ebene Flicken ist es 0 (bis auf
    /// Rundung), und weil die Kontrollpunkte der Teilflicken in der konvexen Hülle der
    /// Ausgangspunkte liegen, wird es beim Unterteilen praktisch nie größer (es ist eine obere
    /// Schranke der halben Breite der konvexen Hülle, keine exakte Breite).
    pub fn flatness(&self) -> f64 {
        let pts = &self.control;
        let n = pts.len() as f64;
        let c = pts.iter().copied().sum::<DVec3>() / n;
        let mut cov = [[0.0; 3]; 3];
        for p in pts {
            let d = (*p - c).to_array();
            for (r, row) in cov.iter_mut().enumerate() {
                for (s, e) in row.iter_mut().enumerate() {
                    *e += d[r] * d[s];
                }
            }
        }
        let mut normals = vec![smallest_eigenvector(cov)];
        let nv = self.degree_v + 1;
        let corner = |i: usize, j: usize| pts[i * nv + j];
        let (du, dv) = (self.degree_u, self.degree_v);
        let diag = (corner(du, dv) - corner(0, 0)).cross(corner(du, 0) - corner(0, dv));
        if diag.length_squared() > 0.0 {
            normals.push(diag.normalize());
        }
        normals
            .iter()
            .filter(|nrm| nrm.is_finite() && nrm.length_squared() > 0.0)
            .map(|nrm| {
                let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                for p in pts {
                    let t = nrm.dot(*p - c);
                    lo = lo.min(t);
                    hi = hi.max(t);
                }
                0.5 * (hi - lo)
            })
            .fold(f64::INFINITY, f64::min)
    }
}

/// Eigenvektor zum kleinsten Eigenwert einer symmetrischen 3×3-Matrix (zyklisches Jacobi-Verfahren
/// mit fester Höchstzahl an Durchläufen).
fn smallest_eigenvector(mut a: [[f64; 3]; 3]) -> DVec3 {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..JACOBI_MAX_SWEEPS {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        let scale = a[0][0].abs() + a[1][1].abs() + a[2][2].abs();
        if off == 0.0 || off <= f64::EPSILON * scale {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q] == 0.0 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let cs = 1.0 / (t * t + 1.0).sqrt();
            let sn = t * cs;
            for row in a.iter_mut() {
                let (x, y) = (row[p], row[q]);
                row[p] = cs * x - sn * y;
                row[q] = sn * x + cs * y;
            }
            let (rp, rq) = (a[p], a[q]);
            a[p] = std::array::from_fn(|k| cs * rp[k] - sn * rq[k]);
            a[q] = std::array::from_fn(|k| sn * rp[k] + cs * rq[k]);
            for row in v.iter_mut() {
                let (x, y) = (row[p], row[q]);
                row[p] = cs * x - sn * y;
                row[q] = sn * x + cs * y;
            }
        }
    }
    let mut best = 0;
    for i in 1..3 {
        if a[i][i] < a[best][best] {
            best = i;
        }
    }
    DVec3::new(v[0][best], v[1][best], v[2][best])
}

impl NurbsCurve {
    /// Fügt den Knoten `u` `times`-mal ein (A5.1, homogen); die Kurve ändert sich nicht.
    /// Fehler, wenn `u` nicht im Inneren liegt oder die Vielfachheit den Grad übersteigen würde.
    pub fn insert_knot(&self, u: f64, times: usize) -> Result<NurbsCurve, NurbsError> {
        let p = self.degree();
        let knots = self.knots().knots();
        check_insertion(knots, p, u, times)?;
        let pw: Vec<DVec4> = self
            .control_points()
            .iter()
            .zip(self.weights())
            .map(|(c, w)| homogeneous(*c, *w))
            .collect();
        let (kn, qw) = insert_row(p, knots, &pw, u, times);
        let (control, weights) = qw.iter().map(|q| cartesian(*q)).unzip();
        NurbsCurve::new(p, kn, control, weights)
    }

    /// Zerlegung in rationale Bézier-Stücke (je ein Stück pro nichtleerem Knotenabschnitt).
    pub fn bezier_segments(&self) -> Vec<BezierSegment> {
        let p = self.degree();
        let pw: Vec<DVec4> = self
            .control_points()
            .iter()
            .zip(self.weights())
            .map(|(c, w)| homogeneous(*c, *w))
            .collect();
        let (breaks, cp) = decompose_row(p, self.knots().knots(), &pw);
        breaks
            .windows(2)
            .enumerate()
            .map(|(k, w)| BezierSegment::from_homogeneous((w[0], w[1]), p, &cp[k * p..=k * p + p]))
            .collect()
    }
}

#[cfg(test)]
mod decompose_tests {
    use super::*;

    /// Bisheriger Weg: jeden inneren Knoten bis zur Vielfachheit p einfügen (Vergleich).
    fn by_insertion(p: usize, knots: &[f64], pw: &[DVec4]) -> Vec<DVec4> {
        let mut breaks: Vec<f64> = Vec::new();
        for &k in knots {
            if breaks.last() != Some(&k) {
                breaks.push(k);
            }
        }
        let mut kn = knots.to_vec();
        let mut cp = pw.to_vec();
        for &u in &breaks[1..breaks.len() - 1] {
            let (_, s) = span_and_multiplicity(&kn, p, u);
            if s < p {
                let (k2, c2) = insert_row(p, &kn, &cp, u, p - s);
                kn = k2;
                cp = c2;
            }
        }
        cp
    }

    #[test]
    fn local_decomposition_matches_knot_insertion() {
        let cases: Vec<(usize, Vec<f64>)> = vec![
            (1, vec![0.0, 0.0, 0.501, 0.502, 0.503, 1.0, 1.0]),
            (2, vec![0.0, 0.0, 0.0, 0.3, 0.3, 0.7, 1.0, 1.0, 1.0]),
            (
                3,
                vec![0.0, 0.0, 0.0, 0.0, 0.2, 0.5, 0.5, 0.9, 1.0, 1.0, 1.0, 1.0],
            ),
            (3, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]),
            (
                4,
                vec![
                    -2.0, -2.0, -2.0, -2.0, -2.0, -1.0, 0.5, 0.5, 0.5, 3.0, 3.0, 3.0, 3.0, 3.0,
                ],
            ),
        ];
        for (p, knots) in cases {
            let n = knots.len() - p - 1;
            let pw: Vec<DVec4> = (0..n)
                .map(|i| {
                    let t = i as f64;
                    homogeneous(
                        DVec3::new(t.sin() * 3.0, t * 0.7, (t * 1.3).cos()),
                        1.0 + 0.3 * (t * 2.1).sin().abs(),
                    )
                })
                .collect();
            let (_, fast) = decompose_row(p, &knots, &pw);
            let slow = by_insertion(p, &knots, &pw);
            assert_eq!(fast.len(), slow.len(), "Grad {p}");
            for (a, b) in fast.iter().zip(&slow) {
                assert!((*a - *b).length() < 1e-12, "Grad {p}: {a} gegen {b}");
            }
        }
    }
}
