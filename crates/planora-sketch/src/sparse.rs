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
/// darunter dicht (bitgleich zu früher). Gemessen in Task 3 (`sparse_threshold_crossover`):
/// In `chain_2d` ist der dünne Weg ab 30 Variablen beim Lösen und beim Zugschritt schneller.
pub const SPARSE_MIN_VARS: usize = 30;

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

/// Ergebnis der dünnen Rangprüfung ([`analyze_rows_sparse`]); Spalten in der ursprünglichen
/// Nummerierung.
#[derive(Clone, Debug, PartialEq)]
pub struct SparseRowSpace {
    /// Je Zeile: `true`, wenn sie vom Zeilenraum der früheren Zeilen unabhängig ist.
    pub independent: Vec<bool>,
    pub rank: usize,
    /// Je Spalte: `true`, wenn der Nullraum sie bewegt (`‖P_N e_c‖ > free_tol`).
    pub free: Vec<bool>,
}

/// Rang, abhängige Zeilen und freie Spalten wie [`crate::linalg::analyze_rows`] samt der
/// Freiheitsprüfung des Lösers (`‖P_N e_c‖ > free_tol`), aber dünn.
///
/// **Unabhängige Zeilen:** Der Test von `analyze_rows` ist Gram-Schmidt in Zeilenreihenfolge:
/// Zeile `i` ist unabhängig, wenn ihr Rest gegen den Raum der früheren unabhängigen Zeilen
/// länger als `rel_tol · ‖a_i‖` ist. Dieser Rest ist die Diagonale `L_ii` der Cholesky-Zerlegung
/// von `A·Aᵀ` in Zeilenreihenfolge, in der abhängige Zeilen übersprungen werden (`A = L·Q`).
/// `A·Aᵀ` ist dünn (zwei Zeilen koppeln nur über gemeinsame Spalten), die Zerlegung bleibt in
/// ihrer Hülle. Weil `A·Aᵀ` die Kondition quadriert, rechnet sie doppelt genau (zwei `f64` je
/// Zahl, Dekker/Knuth): Der Rest `L_ii²` wird so bis weit unter `rel_tol²` genau. Der Rest
/// einer Givens-Elimination gegen ein `R` wäre nicht derselbe Wert (die Zeilen von `R` sind nicht
/// orthonormal) und entschiede an fast abhängigen Stellen anders.
///
/// **Freie Spalten:** `‖P_N e_c‖² = 1 − ‖L⁻¹ a_c‖²` (`a_c` die Spalte `c` der unabhängigen
/// Zeilen), genau mit dem doppelt genauen `L` durch Vorwärtseinsetzen; sobald die Summe
/// `‖L⁻¹ a_c‖²` die Grenze erreicht, steht „nicht frei“ fest. Weil das für jede Spalte bis ans
/// Ende laufen kann, gibt es einen Kurzweg für „frei“: eine inkrementelle Givens-QR der
/// unabhängigen Zeilen, Spalten in der Ordnung `perm` (neu → alt, etwa RCM), `R` als Hülle (je
/// Pivotspalte `k` eine Zeile ab `k`). Je Spalte `f` ohne Pivot liefert Rückwärtseinsetzen einen
/// Nullraumvektor `z_f` (`z_f[f] = 1`, null an den übrigen Spalten ohne Pivot). Er zählt nur, wenn
/// `A·z_f` wirklich verschwindet (`R` kann an Stellen mit sehr kleinem Pivot ungenau sein); dann
/// ist jede Spalte `c` mit `|z_f[c]| / ‖z_f‖` deutlich über `free_tol` frei
/// (`‖P_N e_c‖ ≥ |z_f[c]| / ‖z_f‖`). Alle übrigen Spalten prüft der genaue Weg.
pub fn analyze_rows_sparse(
    rows: &SparseRows,
    perm: &[usize],
    rel_tol: f64,
    free_tol: f64,
) -> SparseRowSpace {
    let n = rows.cols;
    assert_eq!(perm.len(), n);
    let chol = RowCholesky::new(rows, rel_tol);
    let rank = chol.independent.iter().filter(|&&b| b).count();
    let mut free = vec![false; n];
    if rank < n {
        for (c, &f) in null_space_shortcut(rows, perm, &chol.independent, free_tol)
            .iter()
            .enumerate()
        {
            free[c] = f;
        }
        let by_col = columns(rows, &chol.independent);
        for c in 0..n {
            if !free[c] {
                free[c] = chol.column_free(&by_col[c], free_tol);
            }
        }
    }
    SparseRowSpace {
        independent: chol.independent,
        rank,
        free,
    }
}

/// Zahl mit doppelter Genauigkeit: `hi + lo`, `|lo| ≤ ulp(hi) / 2`.
#[derive(Clone, Copy, Debug, Default)]
struct Dd {
    hi: f64,
    lo: f64,
}

/// `a + b` exakt als Summe und Rundungsrest (Knuth).
#[inline]
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

/// `a + b` exakt, wenn `|a| ≥ |b|`.
#[inline]
fn quick_two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    Dd {
        hi: s,
        lo: b - (s - a),
    }
}

/// Teilt `a` in zwei Hälften mit je 26 Bit (Dekker).
#[inline]
fn split(a: f64) -> (f64, f64) {
    let t = 134_217_729.0 * a;
    let hi = t - (t - a);
    (hi, a - hi)
}

/// `a · b` exakt als Produkt und Rundungsrest (Dekker, ohne FMA).
#[inline]
fn two_prod(a: f64, b: f64) -> Dd {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    Dd {
        hi: p,
        lo: ((ah * bh - p) + ah * bl + al * bh) + al * bl,
    }
}

impl Dd {
    const ONE: Dd = Dd { hi: 1.0, lo: 0.0 };

    fn of(v: f64) -> Dd {
        Dd { hi: v, lo: 0.0 }
    }

    fn value(self) -> f64 {
        self.hi + self.lo
    }

    #[inline]
    fn add(self, o: Dd) -> Dd {
        let (s, e) = two_sum(self.hi, o.hi);
        quick_two_sum(s, e + self.lo + o.lo)
    }

    #[inline]
    fn sub(self, o: Dd) -> Dd {
        self.add(Dd {
            hi: -o.hi,
            lo: -o.lo,
        })
    }

    #[inline]
    fn mul(self, o: Dd) -> Dd {
        let p = two_prod(self.hi, o.hi);
        quick_two_sum(p.hi, p.lo + self.hi * o.lo + self.lo * o.hi)
    }

    fn div(self, o: Dd) -> Dd {
        let q1 = self.hi / o.hi;
        let r = self.sub(o.mul(Dd::of(q1)));
        let q2 = r.hi / o.hi;
        let r = r.sub(o.mul(Dd::of(q2)));
        let q3 = r.hi / o.hi;
        quick_two_sum(q1, q2).add(Dd::of(q3))
    }

    fn sqrt(self) -> Dd {
        let s = self.hi.sqrt();
        let rest = self.sub(two_prod(s, s));
        quick_two_sum(s, rest.hi / (2.0 * s))
    }
}

/// Cholesky-Zerlegung von `A·Aᵀ` in Zeilenreihenfolge, doppelt genau, abhängige Zeilen
/// übersprungen (siehe [`analyze_rows_sparse`]).
struct RowCholesky {
    independent: Vec<bool>,
    /// Zeile `i` koppelt höchstens mit den Zeilen `first[i]..i` (Hülle von `A·Aᵀ`).
    first: Vec<usize>,
    /// `l[i][k - first[i]]` ist `L_ik`; abhängige Zeilen bleiben leer.
    l: Vec<Vec<Dd>>,
}

impl RowCholesky {
    fn new(rows: &SparseRows, rel_tol: f64) -> RowCholesky {
        let m = rows.rows();
        let mut col_first = vec![usize::MAX; rows.cols];
        let mut first = vec![0; m];
        for (i, f) in first.iter_mut().enumerate() {
            let (cols, _) = rows.row(i);
            *f = i;
            for &c in cols {
                if col_first[c] == usize::MAX {
                    col_first[c] = i;
                }
                *f = (*f).min(col_first[c]);
            }
        }
        let mut l: Vec<Vec<Dd>> = vec![Vec::new(); m];
        let mut independent = vec![false; m];
        let mut x = vec![0.0; rows.cols];
        for i in 0..m {
            let (cols, vals) = rows.row(i);
            let norm0 = vals.iter().map(|v| v * v).sum::<f64>().sqrt();
            if norm0.is_nan() || norm0 <= 0.0 || !norm0.is_finite() {
                continue;
            }
            for (&c, &v) in cols.iter().zip(vals) {
                x[c] = v;
            }
            let fi = first[i];
            let mut li = vec![Dd::default(); i - fi + 1];
            // `d2 = ‖a_i‖² − Σ_k L_ik²`, der quadrierte Gram-Schmidt-Rest.
            let mut d2 = Dd::default();
            for &v in vals {
                d2 = d2.add(two_prod(v, v));
            }
            for k in fi..i {
                if !independent[k] {
                    continue;
                }
                let (kc, kv) = rows.row(k);
                let mut s = Dd::default();
                for (&c, &v) in kc.iter().zip(kv) {
                    if x[c] != 0.0 {
                        s = s.add(two_prod(v, x[c]));
                    }
                }
                let fk = first[k];
                let lk = &l[k];
                for j in fi.max(fk)..k {
                    let (a, b) = (li[j - fi], lk[j - fk]);
                    if a.hi != 0.0 && b.hi != 0.0 {
                        s = s.sub(a.mul(b));
                    }
                }
                let lik = s.div(lk[k - fk]);
                li[k - fi] = lik;
                d2 = d2.sub(lik.mul(lik));
            }
            for &c in cols {
                x[c] = 0.0;
            }
            let tol = rel_tol * norm0;
            if d2.value() > tol * tol {
                li[i - fi] = d2.sqrt();
                l[i] = li;
                independent[i] = true;
            }
        }
        RowCholesky {
            independent,
            first,
            l,
        }
    }

    /// Ist `‖P_N e_c‖ > free_tol`? `col` ist die Spalte `c` als `(Zeile, Wert)` über die
    /// unabhängigen Zeilen, aufsteigend. `P_N e_c = e_c − Qᵀ Q e_c` mit `Q = L⁻¹ A`, also
    /// `‖P_N e_c‖² = 1 − ‖w‖²`, `L·w = a_c`.
    fn column_free(&self, col: &[(usize, f64)], free_tol: f64) -> bool {
        let Some(&(r0, _)) = col.first() else {
            // Keine Zeile liest die Spalte: Sie bewegt sich frei.
            return true;
        };
        let limit = Dd::ONE.sub(two_prod(free_tol, free_tol));
        let m = self.l.len();
        let mut w = vec![Dd::default(); m - r0];
        let mut sum = Dd::default();
        let mut next = 0;
        for k in r0..m {
            if !self.independent[k] {
                continue;
            }
            let mut s = Dd::default();
            if next < col.len() && col[next].0 == k {
                s = Dd::of(col[next].1);
                next += 1;
            }
            let fk = self.first[k];
            let lk = &self.l[k];
            for j in fk.max(r0)..k {
                let (a, b) = (lk[j - fk], w[j - r0]);
                if a.hi != 0.0 && b.hi != 0.0 {
                    s = s.sub(a.mul(b));
                }
            }
            if s.hi == 0.0 {
                continue;
            }
            let wk = s.div(lk[k - fk]);
            w[k - r0] = wk;
            sum = sum.add(wk.mul(wk));
            // Die Summe wächst nur: Ab hier steht „nicht frei“ fest.
            if sum.value() >= limit.value() {
                return false;
            }
        }
        true
    }
}

/// Die Spalten der unabhängigen Zeilen: je Spalte `(Zeile, Wert)`, Zeilen aufsteigend.
fn columns(rows: &SparseRows, independent: &[bool]) -> Vec<Vec<(usize, f64)>> {
    let mut by_col: Vec<Vec<(usize, f64)>> = vec![Vec::new(); rows.cols];
    for i in (0..rows.rows()).filter(|&i| independent[i]) {
        let (cols, vals) = rows.row(i);
        for (&c, &v) in cols.iter().zip(vals) {
            by_col[c].push((i, v));
        }
    }
    by_col
}

/// Kurzweg für „frei“ (siehe [`analyze_rows_sparse`]): Givens-QR der unabhängigen Zeilen,
/// Nullraumvektoren durch Rückwärtseinsetzen, nur geprüfte Vektoren zählen. Je Spalte
/// (ursprüngliche Nummerierung) `true`, wenn sie damit sicher frei ist; `false` heißt „offen“.
fn null_space_shortcut(
    rows: &SparseRows,
    perm: &[usize],
    independent: &[bool],
    free_tol: f64,
) -> Vec<bool> {
    // Ein Rest bis zu diesem Anteil von `‖a_i‖ · ‖z‖` gilt als Rundung.
    const NULL_RESIDUAL: f64 = 1e-10;
    // So weit über `free_tol` muss der Anteil liegen, damit der Kurzweg entscheidet.
    const MARGIN: f64 = 1e4;
    let n = rows.cols;
    let mut inv = vec![0; n];
    for (new, &old) in perm.iter().enumerate() {
        inv[old] = new;
    }
    let r = givens_r(rows, &inv, independent);
    let mut free = vec![false; n];
    let norms: Vec<f64> = (0..rows.rows())
        .map(|i| rows.row(i).1.iter().map(|v| v * v).sum::<f64>().sqrt())
        .collect();
    for f in (0..n).filter(|&k| r[k].is_empty()) {
        let z = null_vector(&r, f);
        let len = z[..=f].iter().map(|x| x * x).sum::<f64>().sqrt();
        if !len.is_finite() {
            continue;
        }
        let genuine = (0..rows.rows()).filter(|&i| independent[i]).all(|i| {
            let (cols, vals) = rows.row(i);
            let dot: f64 = cols.iter().zip(vals).map(|(&c, v)| v * z[inv[c]]).sum();
            dot.abs() <= NULL_RESIDUAL * norms[i] * len
        });
        if !genuine {
            continue;
        }
        for (new, x) in z[..=f].iter().enumerate() {
            if x.abs() > MARGIN * free_tol * len {
                free[perm[new]] = true;
            }
        }
    }
    free
}

/// Givens-QR der unabhängigen Zeilen in der Spaltenordnung `inv` (alt → neu): Zeile `k` von `R`
/// ab Spalte `k`, leer ohne Pivot. Zeilen nach ihrer vordersten Spalte (George/Heath, weniger
/// Auffüllen). Einträge an Spalten ohne Pivot bis `GIVENS_DROP · ‖Zeile‖` gelten als Rundung
/// und fallen weg; ob `R` stimmt, prüft der Aufrufer an den Nullraumvektoren.
fn givens_r(rows: &SparseRows, inv: &[usize], independent: &[bool]) -> Vec<Vec<f64>> {
    const GIVENS_DROP: f64 = 1e-10;
    let n = rows.cols;
    let mut order: Vec<(usize, usize)> = (0..rows.rows())
        .filter(|&i| independent[i])
        .map(|i| (rows.row(i).0.iter().map(|&c| inv[c]).min().unwrap_or(n), i))
        .collect();
    order.sort_unstable();
    let mut r: Vec<Vec<f64>> = vec![Vec::new(); n];
    let mut w = vec![0.0; n];
    for (_, i) in order {
        let (cols, vals) = rows.row(i);
        let norm0 = vals.iter().map(|v| v * v).sum::<f64>().sqrt();
        let (mut lo, mut hi) = (n, 0);
        for (&c, &v) in cols.iter().zip(vals) {
            let k = inv[c];
            w[k] += v;
            lo = lo.min(k);
            hi = hi.max(k);
        }
        let (first, drop) = (lo, GIVENS_DROP * norm0);
        let mut j = lo;
        while j <= hi {
            let b = w[j];
            if b != 0.0 {
                if !r[j].is_empty() {
                    let row = &mut r[j];
                    let end = j + row.len() - 1;
                    if hi > end {
                        row.resize(hi - j + 1, 0.0);
                    }
                    hi = hi.max(end);
                    let a = row[0];
                    let h = a.hypot(b);
                    let (cs, sn) = (a / h, b / h);
                    row[0] = h;
                    w[j] = 0.0;
                    for t in j + 1..=hi {
                        let (x, y) = (row[t - j], w[t]);
                        row[t - j] = cs * x + sn * y;
                        w[t] = cs * y - sn * x;
                    }
                } else if b.abs() > drop {
                    r[j] = w[j..=hi].to_vec();
                    break;
                } else {
                    w[j] = 0.0;
                }
            }
            j += 1;
        }
        for x in &mut w[first..=hi] {
            *x = 0.0;
        }
    }
    r
}

/// Der Nullraumvektor `z_f` zur Spalte `f` ohne Pivot (siehe [`analyze_rows_sparse`]); nur die
/// Einträge bis `f` können von null verschieden sein.
fn null_vector(r: &[Vec<f64>], f: usize) -> Vec<f64> {
    let mut v = vec![0.0; r.len()];
    v[f] = 1.0;
    for k in (0..f).rev() {
        let row = &r[k];
        if row.is_empty() {
            continue;
        }
        let end = (k + row.len()).min(f + 1);
        let s: f64 = (k + 1..end).map(|t| row[t - k] * v[t]).sum();
        v[k] = -s / row[0];
    }
    v
}
