use planora_math::nurbs::*;
use planora_math::{DVec3, PI};

fn clamp_knots(p: usize, n: usize) -> Vec<f64> {
    // gleichmäßig geklemmt auf [0, 1]
    let mut k = vec![0.0; p + 1];
    let inner = n - p - 1;
    for i in 1..=inner {
        k.push(i as f64 / (inner + 1) as f64);
    }
    k.extend(std::iter::repeat_n(1.0, p + 1));
    k
}

#[test]
fn basis_functions_form_a_partition_of_unity() {
    let kv = KnotVector::new(3, clamp_knots(3, 7)).unwrap();
    for i in 0..=100 {
        let u = i as f64 / 100.0;
        let span = kv.find_span(u);
        let n = kv.basis(span, u);
        assert_eq!(n.len(), 4);
        assert!((n.iter().sum::<f64>() - 1.0).abs() < 1e-12, "u={u}");
        assert!(n.iter().all(|v| *v >= -1e-15));
    }
}

#[test]
fn basis_derivatives_match_finite_differences() {
    let kv = KnotVector::new(3, clamp_knots(3, 8)).unwrap();
    for &u in &[0.1, 0.33, 0.5, 0.77, 0.9] {
        let h = 1e-6;
        let s = |x: f64| kv.basis(kv.find_span(x), x);
        let d = kv.basis_derivs(kv.find_span(u), u, 2);
        let (a, b) = (s(u + h), s(u - h));
        for j in 0..4 {
            let fd = (a[j] - b[j]) / (2.0 * h);
            assert!(
                (d[1][j] - fd).abs() < 1e-5,
                "u={u} j={j}: {} vs {fd}",
                d[1][j]
            );
        }
        assert_eq!(d.len(), 3);
        // zweite Ableitung: Summe der Zeile ist 0 (Ableitung der Eins)
        assert!(d[1].iter().sum::<f64>().abs() < 1e-9);
        assert!(d[2].iter().sum::<f64>().abs() < 1e-8);
    }
}

#[test]
fn a_rational_quadratic_reproduces_a_quarter_circle_exactly() {
    let w = std::f64::consts::FRAC_1_SQRT_2;
    let c = NurbsCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        vec![
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(1.0, 1.0, 0.0),
            DVec3::new(0.0, 1.0, 0.0),
        ],
        vec![1.0, w, 1.0],
    )
    .unwrap();
    for i in 0..=50 {
        let p = c.point(i as f64 / 50.0);
        assert!((p.length() - 1.0).abs() < 1e-14, "{p:?}");
    }
    let t = c.tangent(0.0);
    assert!(t.normalize().distance(DVec3::Y) < 1e-14);
    let _ = PI;
}

#[test]
fn derivatives_match_finite_differences_for_a_rational_cubic() {
    let c = NurbsCurve::new(
        3,
        clamp_knots(3, 6),
        vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(1.0, 2.0, 0.5),
            DVec3::new(3.0, 3.0, 1.0),
            DVec3::new(4.0, 1.0, 0.0),
            DVec3::new(6.0, 0.0, -1.0),
            DVec3::new(7.0, 2.0, 2.0),
        ],
        vec![1.0, 2.0, 0.5, 1.5, 1.0, 3.0],
    )
    .unwrap();
    let h = 1e-6;
    for &u in &[0.05, 0.3, 0.5, 0.7, 0.95] {
        let d = c.derivs(u, 2);
        let fd1 = (c.point(u + h) - c.point(u - h)) / (2.0 * h);
        let fd2 = (c.point(u + h) - c.point(u) * 2.0 + c.point(u - h)) / (h * h);
        assert!(d[1].distance(fd1) < 1e-4 * d[1].length().max(1.0), "u={u}");
        assert!(d[2].distance(fd2) < 1e-1 * d[2].length().max(1.0), "u={u}");
        assert!(d[0].distance(c.point(u)) < 1e-15);
    }
}

#[test]
fn evaluation_is_total_outside_and_for_bad_parameters() {
    let c = NurbsCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        vec![DVec3::ZERO, DVec3::X, DVec3::new(2.0, 1.0, 0.0)],
        vec![1.0; 3],
    )
    .unwrap();
    for u in [-5.0, 5.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let p = c.point(u);
        assert!(p.is_finite(), "u={u}");
        assert!(c.derivs(u, 2).iter().all(|d| d.is_finite()));
    }
    assert!(c.point(-5.0).distance(c.point(0.0)) < 1e-15);
    assert!(c.point(5.0).distance(c.point(1.0)) < 1e-15);
}

#[test]
fn hostile_data_is_rejected_by_the_constructor() {
    let k = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let p = vec![DVec3::ZERO, DVec3::X, DVec3::Y];
    let ok = |deg: usize, k: Vec<f64>, p: Vec<DVec3>, w: Vec<f64>| NurbsCurve::new(deg, k, p, w);
    assert!(ok(2, k.clone(), p.clone(), vec![1.0; 3]).is_ok());
    assert!(ok(0, k.clone(), p.clone(), vec![1.0; 3]).is_err(), "Grad 0");
    assert!(
        ok(MAX_DEGREE + 1, k.clone(), p.clone(), vec![1.0; 3]).is_err(),
        "Grad zu groß"
    );
    assert!(
        ok(2, k.clone(), p.clone(), vec![1.0, 0.0, 1.0]).is_err(),
        "Gewicht 0"
    );
    assert!(
        ok(2, k.clone(), p.clone(), vec![1.0, -1.0, 1.0]).is_err(),
        "Gewicht negativ"
    );
    assert!(ok(2, k.clone(), p.clone(), vec![1.0, f64::NAN, 1.0]).is_err());
    assert!(
        ok(2, k.clone(), p.clone(), vec![1.0, 1e7, 1.0]).is_err(),
        "Gewicht zu groß"
    );
    assert!(
        ok(2, k.clone(), p.clone(), vec![1.0; 2]).is_err(),
        "Gewichtszahl"
    );
    assert!(
        ok(2, vec![0.0, 0.0, 0.0, 1.0, 1.0], p.clone(), vec![1.0; 3]).is_err(),
        "Knotenzahl"
    );
    assert!(
        ok(
            2,
            vec![0.0, 0.0, 0.0, 0.5, 0.2, 1.0],
            p.clone(),
            vec![1.0; 3]
        )
        .is_err(),
        "Knoten fallend"
    );
    assert!(
        ok(
            2,
            vec![0.0, 0.0, f64::NAN, 1.0, 1.0, 1.0],
            p.clone(),
            vec![1.0; 3]
        )
        .is_err()
    );
    assert!(
        ok(
            2,
            vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            p.clone(),
            vec![1.0; 3]
        )
        .is_err(),
        "Bereich leer"
    );
    assert!(
        ok(
            2,
            vec![0.0, 0.1, 0.2, 0.8, 0.9, 1.0],
            p.clone(),
            vec![1.0; 3]
        )
        .is_err(),
        "nicht geklemmt"
    );
    let far = vec![DVec3::ZERO, DVec3::X, DVec3::new(0.0, 2e9, 0.0)];
    assert!(
        ok(2, k.clone(), far, vec![1.0; 3]).is_err(),
        "Koordinate zu groß"
    );
    let nan = vec![DVec3::ZERO, DVec3::new(f64::NAN, 0.0, 0.0), DVec3::Y];
    assert!(ok(2, k.clone(), nan, vec![1.0; 3]).is_err());
    assert!(
        ok(2, k, vec![DVec3::ZERO, DVec3::X], vec![1.0; 2]).is_err(),
        "zu wenige Punkte"
    );
}

#[test]
fn reversal_keeps_the_shape() {
    let c = NurbsCurve::new(
        3,
        clamp_knots(3, 5),
        vec![
            DVec3::ZERO,
            DVec3::new(1.0, 2.0, 0.0),
            DVec3::new(2.0, -1.0, 1.0),
            DVec3::new(3.0, 1.0, 0.0),
            DVec3::new(4.0, 0.0, 0.0),
        ],
        vec![1.0, 2.0, 1.0, 0.5, 1.0],
    )
    .unwrap();
    let r = c.reversed();
    let (a, b) = c.domain();
    let (ra, rb) = r.domain();
    for i in 0..=20 {
        let f = i as f64 / 20.0;
        let p = c.point(a + (b - a) * f);
        let q = r.point(rb - (rb - ra) * f);
        assert!(p.distance(q) < 1e-12);
    }
}
