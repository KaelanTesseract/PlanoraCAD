//! Dünne lineare Algebra des Lösers (Skizzen 11a, Task 2): dünne Zeilen, RCM-Ordnung und
//! Hüllen-Cholesky gegen die dichte Rechnung.

use planora_sketch::linalg::{Matrix, cholesky_solve};
use planora_sketch::sparse::{Envelope, SparseRows, envelope_cholesky_solve, rcm_order};

/// Einfacher Zufallsgenerator mit festem Startwert (lineare Kongruenz), Werte in [-1, 1).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }

    fn below(&mut self, n: usize) -> usize {
        (((self.next() + 1.0) * 0.5) * n as f64) as usize % n
    }
}

/// Zufällige Bandmatrix 50 × 40: Zeile i hat Einträge um die Spalte `i · 40 / 50`, die Spalten
/// sind gemischt nummeriert, damit die Ordnung etwas zu tun hat.
fn band_rows() -> (usize, Vec<Vec<(usize, f64)>>) {
    let (m, n) = (50, 40);
    let mut rng = Lcg(0x5eed_2026);
    let mut shuffle: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = rng.below(i + 1);
        shuffle.swap(i, j);
    }
    let rows = (0..m)
        .map(|i| {
            let mid = i * n / m;
            (mid.saturating_sub(2)..(mid + 3).min(n))
                .map(|c| (shuffle[c], rng.next()))
                .collect()
        })
        .collect();
    (n, rows)
}

#[test]
fn sparse_gram_equals_dense_gram() {
    let (n, rows) = band_rows();
    let sp = SparseRows::from_rows(n, &rows);
    let perm = rcm_order(&sp);
    let env = Envelope::gram(&sp, &perm);
    let dense = sp.to_dense().gram();
    for i in 0..n {
        for j in 0..n {
            let (a, b) = (env.get(i, j), dense.get(perm[i], perm[j]));
            assert!((a - b).abs() <= 1e-12, "({i}, {j}): {a} gegen {b}");
        }
    }
}

#[test]
fn sparse_rows_multiply_like_dense() {
    let (n, rows) = band_rows();
    let sp = SparseRows::from_rows(n, &rows);
    let dense = sp.to_dense();
    let mut rng = Lcg(7);
    let x: Vec<f64> = (0..n).map(|_| rng.next()).collect();
    let y: Vec<f64> = (0..sp.rows()).map(|_| rng.next()).collect();
    assert_eq!(sp.mul_vec(&x), dense.mul_vec(&x));
    assert_eq!(sp.transpose_mul_vec(&y), dense.transpose_mul_vec(&y));
}

#[test]
fn envelope_cholesky_equals_dense() {
    let (n, rows) = band_rows();
    let sp = SparseRows::from_rows(n, &rows);
    let perm = rcm_order(&sp);
    let mut env = Envelope::gram(&sp, &perm);
    env.add_diagonal(1e-3);
    let mut dense = sp.to_dense().gram();
    for i in 0..n {
        dense.add_to(i, i, 1e-3);
    }
    let mut rng = Lcg(11);
    let b: Vec<f64> = (0..n).map(|_| rng.next()).collect();
    let want = cholesky_solve(&dense, &b).expect("dicht lösbar");
    let b_perm: Vec<f64> = perm.iter().map(|&o| b[o]).collect();
    let got = envelope_cholesky_solve(&env, &b_perm).expect("Hülle lösbar");
    for (i, &o) in perm.iter().enumerate() {
        assert!(
            (got[i] - want[o]).abs() <= 1e-10,
            "Spalte {o}: {} gegen {}",
            got[i],
            want[o]
        );
    }
}

#[test]
fn envelope_cholesky_rejects_indefinite() {
    // Einheitsmatrix minus 2 · I: Diagonale −1.
    let rows: Vec<Vec<(usize, f64)>> = (0..3).map(|i| vec![(i, 1.0)]).collect();
    let sp = SparseRows::from_rows(3, &rows);
    let perm = rcm_order(&sp);
    let mut a = Envelope::gram(&sp, &perm);
    a.add_diagonal(-2.0);
    assert_eq!(a.get(0, 0), -1.0);
    let b = vec![1.0; 3];
    assert!(envelope_cholesky_solve(&a, &b).is_none());
    // NaN in der Matrix wird ebenso abgewiesen.
    let rows = vec![vec![(0, f64::NAN)], vec![(1, 1.0)]];
    let sp = SparseRows::from_rows(2, &rows);
    let a = Envelope::gram(&sp, &rcm_order(&sp));
    assert!(envelope_cholesky_solve(&a, &[1.0, 1.0]).is_none());
}

/// Kette über 200 Variablen in gemischter Nummerierung: je Zeile zwei Nachbarn der Kette.
fn mixed_chain() -> SparseRows {
    let n = 200;
    let mut rng = Lcg(42);
    let mut label: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = rng.below(i + 1);
        label.swap(i, j);
    }
    let rows: Vec<Vec<(usize, f64)>> = (0..n - 1)
        .map(|k| vec![(label[k], 1.0), (label[k + 1], -1.0)])
        .collect();
    SparseRows::from_rows(n, &rows)
}

#[test]
fn rcm_of_chain_has_small_bandwidth() {
    let sp = mixed_chain();
    let perm = rcm_order(&sp);
    let mut inv = vec![usize::MAX; sp.cols];
    for (new, &old) in perm.iter().enumerate() {
        assert_eq!(inv[old], usize::MAX, "Permutation doppelt: {old}");
        inv[old] = new;
    }
    let mut before = 0;
    let mut after = 0;
    for i in 0..sp.rows() {
        let (cols, _) = sp.row(i);
        for &a in cols {
            for &b in cols {
                before = before.max(a.abs_diff(b));
                after = after.max(inv[a].abs_diff(inv[b]));
            }
        }
    }
    assert!(
        before > 4,
        "Die Mischung muss die Bandbreite vergrößern: {before}"
    );
    assert!(after <= 4, "Bandbreite nach RCM: {after}");
}

#[test]
fn rcm_is_deterministic() {
    let sp = mixed_chain();
    assert_eq!(rcm_order(&sp), rcm_order(&sp));
    let (n, rows) = band_rows();
    let band = SparseRows::from_rows(n, &rows);
    assert_eq!(rcm_order(&band), rcm_order(&band));
}

#[test]
fn sparse_rows_merge_duplicate_columns_like_dense() {
    // Doppelte Spalte in einer Zeile: summiert wie `Matrix::add_to`.
    let rows = vec![vec![(1, 0.5), (0, 2.0), (1, 0.25)]];
    let sp = SparseRows::from_rows(2, &rows);
    let mut want = Matrix::zeros(1, 2);
    want.add_to(0, 1, 0.5);
    want.add_to(0, 0, 2.0);
    want.add_to(0, 1, 0.25);
    assert_eq!(sp.to_dense(), want);
}
