use planora_sketch::linalg::*;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn matrix_basics() {
    let m = Matrix::from_rows(&[vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]);
    assert_eq!((m.rows(), m.cols()), (2, 3));
    assert_eq!(m.get(1, 2), 6.0);
    assert_eq!(m.mul_vec(&[1.0, 0.0, -1.0]), vec![-2.0, -2.0]);
    assert_eq!(m.transpose_mul_vec(&[1.0, 1.0]), vec![5.0, 7.0, 9.0]);
    let g = m.gram();
    assert_eq!((g.rows(), g.cols()), (3, 3));
    assert_eq!(g.get(0, 0), 17.0);
    assert_eq!(g.get(0, 1), g.get(1, 0));
    let mut z = Matrix::zeros(2, 2);
    z.set(0, 1, 2.0);
    z.add_to(0, 1, 3.0);
    assert_eq!(z.get(0, 1), 5.0);
    assert_eq!(Matrix::identity(3).get(2, 2), 1.0);
}

#[test]
fn cholesky_solves_a_positive_definite_system() {
    let a = Matrix::from_rows(&[
        vec![4.0, 1.0, 0.0],
        vec![1.0, 3.0, 1.0],
        vec![0.0, 1.0, 2.0],
    ]);
    let x = [1.0, 2.0, 3.0];
    let b = a.mul_vec(&x);
    let got = cholesky_solve(&a, &b).unwrap();
    for (g, w) in got.iter().zip(x) {
        assert!(close(*g, w), "{got:?}");
    }
}

#[test]
fn cholesky_rejects_indefinite_and_non_finite() {
    let indefinite = Matrix::from_rows(&[vec![1.0, 2.0], vec![2.0, 1.0]]);
    assert!(cholesky_solve(&indefinite, &[1.0, 1.0]).is_none());
    let nan = Matrix::from_rows(&[vec![f64::NAN, 0.0], vec![0.0, 1.0]]);
    assert!(cholesky_solve(&nan, &[1.0, 1.0]).is_none());
    let singular = Matrix::from_rows(&[vec![1.0, 1.0], vec![1.0, 1.0]]);
    assert!(cholesky_solve(&singular, &[1.0, 1.0]).is_none());
}

#[test]
fn later_duplicate_rows_are_the_dependent_ones() {
    let a = Matrix::from_rows(&[vec![1.0, 2.0], vec![2.0, 4.0]]);
    let r = analyze_rows(&a, 1e-9);
    assert_eq!(r.independent, vec![true, false]);
    assert_eq!(r.rank, 1);
    assert_eq!(r.null_space.len(), 1);
    // Nullvektor steht senkrecht auf (1, 2)
    let v = &r.null_space[0];
    assert!(close(v[0] + 2.0 * v[1], 0.0));
    assert!(close(v.iter().map(|x| x * x).sum::<f64>(), 1.0));
}

#[test]
fn combination_of_earlier_rows_is_dependent() {
    let a = Matrix::from_rows(&[
        vec![1.0, 0.0, 0.0],
        vec![0.0, 1.0, 0.0],
        vec![1.0, 1.0, 0.0],
    ]);
    let r = analyze_rows(&a, 1e-9);
    assert_eq!(r.independent, vec![true, true, false]);
    assert_eq!(r.rank, 2);
    assert_eq!(r.null_space.len(), 1);
    assert!(close(r.null_space[0][2].abs(), 1.0));
}

#[test]
fn zero_rows_are_dependent_and_scaling_does_not_matter() {
    let a = Matrix::from_rows(&[vec![0.0, 0.0], vec![1e-8, 0.0], vec![0.0, 1e8]]);
    let r = analyze_rows(&a, 1e-9);
    assert_eq!(r.independent, vec![false, true, true]);
    assert_eq!(r.rank, 2);
    assert!(r.null_space.is_empty());
}

#[test]
fn no_rows_means_the_whole_space_is_free() {
    let r = analyze_rows(&Matrix::zeros(0, 3), 1e-9);
    assert_eq!(r.rank, 0);
    assert_eq!(r.null_space.len(), 3);
}

#[test]
fn null_space_is_orthonormal_and_annihilated() {
    let a = Matrix::from_rows(&[
        vec![1.0, 2.0, 0.5, -1.0, 0.0, 3.0],
        vec![0.0, 1.0, 1.0, 2.0, -2.0, 0.0],
        vec![3.0, 0.0, -1.0, 1.0, 1.0, 1.0],
    ]);
    let r = analyze_rows(&a, 1e-9);
    assert_eq!(r.rank, 3);
    assert_eq!(r.null_space.len(), 3);
    for (i, v) in r.null_space.iter().enumerate() {
        for rv in a.mul_vec(v) {
            assert!(rv.abs() < 1e-9, "A·v = {rv}");
        }
        for (j, w) in r.null_space.iter().enumerate() {
            let d: f64 = v.iter().zip(w).map(|(x, y)| x * y).sum();
            assert!(close(d, if i == j { 1.0 } else { 0.0 }));
        }
    }
}
