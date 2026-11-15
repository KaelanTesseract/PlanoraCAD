//! Dünne lineare Algebra für große Lösergruppen (Skizzen 11a): Jacobi-Matrix als dünne Zeilen
//! (CSR), Ordnung der Spalten nach umgekehrtem Cuthill-McKee (RCM) und Cholesky-Zerlegung in der
//! unteren Hülle von `JᵀJ`.
//!
//! Lange Ketten (verkettete Linien und Bögen) koppeln jede Variable nur mit wenigen Nachbarn.
//! Nach RCM liegt `JᵀJ` in einem schmalen Band, und die Hülle (je Zeile vom ersten Eintrag bis
//! zur Diagonale) bleibt bei der Zerlegung ohne Auffüllen außerhalb. So kostet ein Schritt
//! `O(n · b²)` statt `O(n³)` (`b` die Bandbreite).

use crate::linalg::Matrix;
use std::collections::VecDeque;
use std::sync::OnceLock;

/// Ab so vielen Variablen einer Gruppe rechnet der Löser im Weg [`SolverPath::Auto`] dünn;
/// darunter dicht (bitgleich zu früher). Startwert, endgültig nach Task 3.
pub const SPARSE_MIN_VARS: usize = 60;

/// Welchen Weg der Löser je Gruppe nimmt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SolverPath {
    /// Dünn ab [`SPARSE_MIN_VARS`] Variablen, sonst dicht.
    #[default]
    Auto,
    /// Immer dicht.
    Dense,
    /// Immer dünn (zum Prüfen).
    Sparse,
}

impl SolverPath {
    /// Weg aus der Umgebungsvariablen `PLANORA_SOLVER` (`dense` oder `sparse`, sonst
    /// [`SolverPath::Auto`]); einmal gelesen und gemerkt.
    pub fn from_env() -> SolverPath {
        static PATH: OnceLock<SolverPath> = OnceLock::new();
        *PATH.get_or_init(|| match std::env::var("PLANORA_SOLVER").as_deref() {
            Ok("dense") => SolverPath::Dense,
            Ok("sparse") => SolverPath::Sparse,
            _ => SolverPath::Auto,
        })
    }

    /// Rechnet eine Gruppe mit `vars` Variablen dünn?
    pub(crate) fn sparse_for(self, vars: usize) -> bool {
        match self {
            SolverPath::Auto => vars >= SPARSE_MIN_VARS,
            SolverPath::Dense => false,
            SolverPath::Sparse => true,
        }
    }
}

/// Dünne Matrix, zeilenweise (CSR). Spalten je Zeile aufsteigend, ohne Doppelte.
#[derive(Clone, Debug, PartialEq)]
pub struct SparseRows {
    pub cols: usize,
    /// Zeile `i` liegt in `col[row_start[i]..row_start[i + 1]]`.
    pub row_start: Vec<usize>,
    pub col: Vec<usize>,
    pub val: Vec<f64>,
}

impl SparseRows {
    /// Aus Zeilen mit `(Spalte, Wert)`. Doppelte Spalten einer Zeile werden in ihrer Reihenfolge
    /// summiert (wie `Matrix::add_to`), Nullen bleiben stehen (die Struktur hängt so nicht von
    /// den Werten ab).
    pub fn from_rows(cols: usize, rows: &[Vec<(usize, f64)>]) -> SparseRows {
        let nnz = rows.iter().map(Vec::len).sum();
        let mut out = SparseRows {
            cols,
            row_start: Vec::with_capacity(rows.len() + 1),
            col: Vec::with_capacity(nnz),
            val: Vec::with_capacity(nnz),
        };
        out.row_start.push(0);
        let mut buf: Vec<(usize, f64)> = Vec::new();
        for r in rows {
            buf.clear();
            buf.extend_from_slice(r);
            // Stabil: gleiche Spalten behalten ihre Reihenfolge, die Summe also auch.
            buf.sort_by_key(|e| e.0);
            for &(c, v) in &buf {
                assert!(c < cols, "column out of range");
                let start = *out.row_start.last().unwrap_or(&0);
                if out.col.len() > start && out.col.last() == Some(&c) {
                    *out.val.last_mut().unwrap_or(&mut 0.0) += v;
                } else {
                    out.col.push(c);
                    out.val.push(v);
                }
            }
            out.row_start.push(out.col.len());
        }
        out
    }

    pub fn rows(&self) -> usize {
        self.row_start.len() - 1
    }

    /// Spalten und Werte der Zeile `i`.
    pub fn row(&self, i: usize) -> (&[usize], &[f64]) {
        let (a, b) = (self.row_start[i], self.row_start[i + 1]);
        (&self.col[a..b], &self.val[a..b])
    }

    /// A·x
    pub fn mul_vec(&self, x: &[f64]) -> Vec<f64> {
        assert_eq!(x.len(), self.cols);
        (0..self.rows())
            .map(|i| {
                let (c, v) = self.row(i);
                c.iter().zip(v).map(|(&c, v)| v * x[c]).sum()
            })
            .collect()
    }

    /// Aᵀ·y, Zeile für Zeile in derselben Reihenfolge wie die dichte Rechnung.
    pub fn transpose_mul_vec(&self, y: &[f64]) -> Vec<f64> {
        assert_eq!(y.len(), self.rows());
        let mut out = vec![0.0; self.cols];
        for (i, yi) in y.iter().enumerate() {
            let (c, v) = self.row(i);
            for (&c, a) in c.iter().zip(v) {
                out[c] += a * yi;
            }
        }
        out
    }

    /// Dieselbe Matrix dicht (für die Rangprüfung).
    pub fn to_dense(&self) -> Matrix {
        let mut m = Matrix::zeros(self.rows(), self.cols);
        for i in 0..self.rows() {
            let (c, v) = self.row(i);
            for (&c, &v) in c.iter().zip(v) {
                m.add_to(i, c, v);
            }
        }
        m
    }
}

/// Permutation neu → alt nach umgekehrtem Cuthill-McKee über den Graphen von `JᵀJ` (zwei
/// Spalten sind Nachbarn, wenn eine Zeile beide enthält). Jeder Zusammenhang beginnt am noch
/// nicht besuchten Knoten kleinsten Grads, Nachbarn kommen nach Grad aufsteigend, gleiche Grade
/// nach Index. Deterministisch.
pub fn rcm_order(rows: &SparseRows) -> Vec<usize> {
    let n = rows.cols;
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..rows.rows() {
        let (c, _) = rows.row(i);
        for &a in c {
            for &b in c {
                if a != b {
                    adj[a].push(b);
                }
            }
        }
    }
    for a in &mut adj {
        a.sort_unstable();
        a.dedup();
    }
    let degree: Vec<usize> = adj.iter().map(Vec::len).collect();
    // Startkandidaten nach (Grad, Index).
    let mut starts: Vec<usize> = (0..n).collect();
    starts.sort_by_key(|&v| (degree[v], v));
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    let mut queue = VecDeque::new();
    let mut next: Vec<usize> = Vec::new();
    for &s in &starts {
        if visited[s] {
            continue;
        }
        visited[s] = true;
        queue.push_back(s);
        while let Some(v) = queue.pop_front() {
            order.push(v);
            next.clear();
            next.extend(adj[v].iter().copied().filter(|&w| !visited[w]));
            next.sort_by_key(|&w| (degree[w], w));
            for &w in &next {
                visited[w] = true;
                queue.push_back(w);
            }
        }
    }
    order.reverse();
    order
}

/// Symmetrische Matrix, gespeichert als untere Hülle: Zeile `i` von `first[i]` bis zur
/// Diagonale, zusammenhängend ab `start[i]` in `data`.
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    n: usize,
    first: Vec<usize>,
    start: Vec<usize>,
    data: Vec<f64>,
}

impl Envelope {
    /// `JᵀJ` in der Ordnung `perm` (neu → alt): Eintrag `(i, j)` ist `(JᵀJ)[perm[i]][perm[j]]`.
    /// Die Beiträge der Zeilen werden in derselben Reihenfolge summiert wie bei
    /// [`Matrix::gram`].
    pub fn gram(rows: &SparseRows, perm: &[usize]) -> Envelope {
        let n = rows.cols;
        assert_eq!(perm.len(), n);
        let mut inv = vec![usize::MAX; n];
        for (new, &old) in perm.iter().enumerate() {
            inv[old] = new;
        }
        let mut first: Vec<usize> = (0..n).collect();
        for i in 0..rows.rows() {
            let (c, _) = rows.row(i);
            let Some(lo) = c.iter().map(|&c| inv[c]).min() else {
                continue;
            };
            for &c in c {
                let k = inv[c];
                first[k] = first[k].min(lo);
            }
        }
        let mut start = Vec::with_capacity(n + 1);
        let mut len = 0;
        for (i, &f) in first.iter().enumerate() {
            start.push(len);
            len += i - f + 1;
        }
        start.push(len);
        let mut env = Envelope {
            n,
            first,
            start,
            data: vec![0.0; len],
        };
        for i in 0..rows.rows() {
            let (c, v) = rows.row(i);
            for (&ca, &va) in c.iter().zip(v) {
                let a = inv[ca];
                for (&cb, &vb) in c.iter().zip(v) {
                    let b = inv[cb];
                    if b <= a {
                        let at = env.index(a, b);
                        env.data[at] += va * vb;
                    }
                }
            }
        }
        env
    }

    /// Platz von `(i, j)` mit `first[i] ≤ j ≤ i` in `data`.
    #[inline]
    fn index(&self, i: usize, j: usize) -> usize {
        self.start[i] + j - self.first[i]
    }

    /// Größe der (quadratischen) Matrix.
    pub fn n(&self) -> usize {
        self.n
    }

    /// Addiert `v` auf jede Diagonale.
    pub fn add_diagonal(&mut self, v: f64) {
        for i in 0..self.n {
            let at = self.index(i, i);
            self.data[at] += v;
        }
    }

    /// Größter Diagonaleintrag (NaN zählt nicht, wie bei `f64::max`; leer: −∞).
    pub fn max_diagonal(&self) -> f64 {
        (0..self.n)
            .map(|i| self.data[self.index(i, i)])
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Eintrag `(i, j)` (symmetrisch; außerhalb der Hülle 0).
    pub fn get(&self, i: usize, j: usize) -> f64 {
        let (i, j) = if j > i { (j, i) } else { (i, j) };
        if j < self.first[i] {
            0.0
        } else {
            self.data[self.index(i, j)]
        }
    }
}

/// Löst `A·x = b` für symmetrisch positiv definites `A` in Hüllenform (Cholesky; die Zerlegung
/// füllt nur innerhalb der Hülle auf). `None` unter denselben Bedingungen wie
/// [`crate::linalg::cholesky_solve`]: ein Pivot ≤ 0, NaN oder nicht endlich, oder ein nicht
/// endliches Ergebnis.
pub fn envelope_cholesky_solve(a: &Envelope, b: &[f64]) -> Option<Vec<f64>> {
    let n = a.n;
    assert_eq!(b.len(), n);
    let mut l = a.data.clone();
    for i in 0..n {
        let fi = a.first[i];
        let si = a.start[i];
        for j in fi..=i {
            let fj = a.first[j];
            let sj = a.start[j];
            let k0 = fi.max(fj);
            let mut s = l[si + j - fi];
            for k in k0..j {
                s -= l[si + k - fi] * l[sj + k - fj];
            }
            if i == j {
                if s.is_nan() || s <= 0.0 || !s.is_finite() {
                    return None;
                }
                l[si + i - fi] = s.sqrt();
            } else {
                l[si + j - fi] = s / l[sj + j - fj];
            }
        }
    }
    // Vorwärts: L·y = b.
    let mut x = b.to_vec();
    for i in 0..n {
        let fi = a.first[i];
        let si = a.start[i];
        let mut s = x[i];
        for k in fi..i {
            s -= l[si + k - fi] * x[k];
        }
        x[i] = s / l[si + i - fi];
    }
    // Rückwärts: Lᵀ·x = y, spaltenweise über die Zeilen von L.
    for i in (0..n).rev() {
        let fi = a.first[i];
        let si = a.start[i];
        x[i] /= l[si + i - fi];
        let xi = x[i];
        for k in fi..i {
            x[k] -= l[si + k - fi] * xi;
        }
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}
