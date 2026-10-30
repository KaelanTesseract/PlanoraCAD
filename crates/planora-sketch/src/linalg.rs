//! Kleine dichte lineare Algebra für den Skizzen-Solver (Skizzengruppen haben typisch < 300
//! Variablen, dafür genügen dichte Matrizen).

/// Dichte Matrix, zeilenweise gespeichert.
#[derive(Clone, Debug, PartialEq)]
pub struct Matrix {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

impl Matrix {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    pub fn identity(n: usize) -> Self {
        let mut m = Self::zeros(n, n);
        for i in 0..n {
            m.data[i * n + i] = 1.0;
        }
        m
    }

    /// Alle Zeilen müssen gleich lang sein (interne Schnittstelle).
    pub fn from_rows(rows: &[Vec<f64>]) -> Self {
        let cols = rows.first().map_or(0, Vec::len);
        let mut m = Self::zeros(rows.len(), cols);
        for (i, r) in rows.iter().enumerate() {
            assert_eq!(r.len(), cols, "rows must have the same length");
            m.data[i * cols..(i + 1) * cols].copy_from_slice(r);
        }
        m
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn get(&self, i: usize, j: usize) -> f64 {
        self.data[i * self.cols + j]
    }

    pub fn set(&mut self, i: usize, j: usize, v: f64) {
        self.data[i * self.cols + j] = v;
    }

    pub fn add_to(&mut self, i: usize, j: usize, v: f64) {
        self.data[i * self.cols + j] += v;
    }

    pub fn row(&self, i: usize) -> &[f64] {
        &self.data[i * self.cols..(i + 1) * self.cols]
    }

    /// A·x
    pub fn mul_vec(&self, x: &[f64]) -> Vec<f64> {
        assert_eq!(x.len(), self.cols);
        (0..self.rows).map(|i| dot(self.row(i), x)).collect()
    }

    /// Aᵀ·y
    pub fn transpose_mul_vec(&self, y: &[f64]) -> Vec<f64> {
        assert_eq!(y.len(), self.rows);
        let mut out = vec![0.0; self.cols];
        for (i, yi) in y.iter().enumerate() {
            for (o, a) in out.iter_mut().zip(self.row(i)) {
                *o += a * yi;
            }
        }
        out
    }

    /// AᵀA
    pub fn gram(&self) -> Matrix {
        let n = self.cols;
        let mut g = Matrix::zeros(n, n);
        for i in 0..self.rows {
            let r = self.row(i);
            for (j, rj) in r.iter().enumerate() {
                if *rj == 0.0 {
                    continue;
                }
                for (k, rk) in r.iter().enumerate() {
                    g.data[j * n + k] += rj * rk;
                }
            }
        }
        g
    }
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm(a: &[f64]) -> f64 {
    dot(a, a).sqrt()
}

/// Löst `A·x = b` für symmetrisch positiv definites `A` (Cholesky). `None`, wenn `A` nicht
/// positiv definit ist oder das Ergebnis nicht endlich ist.
pub fn cholesky_solve(a: &Matrix, b: &[f64]) -> Option<Vec<f64>> {
    let n = a.rows;
    assert_eq!(a.cols, n);
    assert_eq!(b.len(), n);
    let mut l = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a.get(i, j);
            for k in 0..j {
                s -= l[i * n + k] * l[j * n + k];
            }
            if i == j {
                // `!(s > 0.0)` fängt auch NaN ab.
                if s.is_nan() || s <= 0.0 || !s.is_finite() {
                    return None;
                }
                l[i * n + i] = s.sqrt();
            } else {
                l[i * n + j] = s / l[j * n + j];
            }
        }
    }
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= l[i * n + k] * y[k];
        }
        y[i] = s / l[i * n + i];
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in i + 1..n {
            s -= l[k * n + i] * x[k];
        }
        x[i] = s / l[i * n + i];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Ergebnis von [`analyze_rows`].
#[derive(Clone, Debug, PartialEq)]
pub struct RowSpace {
    /// Je Zeile: `true`, wenn sie vom Zeilenraum der früheren Zeilen unabhängig ist.
    pub independent: Vec<bool>,
    pub rank: usize,
    /// Orthonormale Basis des Nullraums (`n − rank` Vektoren).
    pub null_space: Vec<Vec<f64>>,
}

/// Zieht den Anteil von `v` ab, der im Raum der orthonormalen `basis` liegt (zweimal, gegen
/// Rundungsfehler).
fn orthogonalize(v: &mut [f64], basis: &[Vec<f64>]) {
    for _ in 0..2 {
        for b in basis {
            let d = dot(v, b);
            for (x, y) in v.iter_mut().zip(b) {
                *x -= d * y;
            }
        }
    }
}

/// Bestimmt Rang, abhängige Zeilen und Nullraum. Zeilen werden in Reihenfolge geprüft; eine Zeile
/// ist abhängig, wenn nach Abzug des Raums der vorigen unabhängigen Zeilen weniger als `rel_tol`
/// ihrer ursprünglichen Länge übrig bleibt.
pub fn analyze_rows(a: &Matrix, rel_tol: f64) -> RowSpace {
    let n = a.cols();
    let mut basis: Vec<Vec<f64>> = Vec::new();
    let mut independent = vec![false; a.rows()];
    for (i, flag) in independent.iter_mut().enumerate() {
        let mut v = a.row(i).to_vec();
        let norm0 = norm(&v);
        if norm0.is_nan() || norm0 <= 0.0 || !norm0.is_finite() {
            continue;
        }
        orthogonalize(&mut v, &basis);
        let norm1 = norm(&v);
        if norm1 > rel_tol * norm0 {
            for x in &mut v {
                *x /= norm1;
            }
            basis.push(v);
            *flag = true;
        }
    }
    let rank = basis.len();

    // Nullraum: Einheitsvektoren gegen den Zeilenraum orthogonalisieren; dann wiederholt den mit
    // dem größten Rest nehmen, normieren und aus den übrigen entfernen.
    let mut rest: Vec<Vec<f64>> = (0..n)
        .map(|j| {
            let mut e = vec![0.0; n];
            e[j] = 1.0;
            orthogonalize(&mut e, &basis);
            e
        })
        .collect();
    let mut null_space = Vec::new();
    for _ in 0..n.saturating_sub(rank) {
        let Some((best, len)) = rest
            .iter()
            .enumerate()
            .map(|(j, e)| (j, norm(e)))
            .max_by(|x, y| x.1.total_cmp(&y.1))
        else {
            break;
        };
        if len.is_nan() || len <= 0.0 {
            break;
        }
        let q: Vec<f64> = rest[best].iter().map(|x| x / len).collect();
        for e in rest.iter_mut() {
            let d = dot(e, &q);
            for (x, y) in e.iter_mut().zip(&q) {
                *x -= d * y;
            }
        }
        null_space.push(q);
    }
    RowSpace {
        independent,
        rank,
        null_space,
    }
}
