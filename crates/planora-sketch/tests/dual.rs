use planora_sketch::dual::Dual;

/// f(x, y) = sqrt(x² + y²) / (x + 3) − x·y
fn f(x: &Dual, y: &Dual) -> Dual {
    let r = (x * x + y * y).sqrt();
    &r / &(x + &Dual::constant(3.0)) - x * y
}

#[test]
fn gradient_matches_finite_differences() {
    let (x0, y0) = (1.3, -0.7);
    let x = Dual::variable(x0, 0, 2);
    let y = Dual::variable(y0, 1, 2);
    let r = f(&x, &y);
    let h = 1e-6;
    let value = |a: f64, b: f64| f(&Dual::constant(a), &Dual::constant(b)).v;
    let dx = (value(x0 + h, y0) - value(x0 - h, y0)) / (2.0 * h);
    let dy = (value(x0, y0 + h) - value(x0, y0 - h)) / (2.0 * h);
    assert!((r.v - value(x0, y0)).abs() < 1e-12);
    assert!((r.grad(0) - dx).abs() < 1e-7, "{} vs {dx}", r.grad(0));
    assert!((r.grad(1) - dy).abs() < 1e-7, "{} vs {dy}", r.grad(1));
}

#[test]
fn constants_have_zero_gradient_and_mix_with_variables() {
    let c = Dual::constant(2.0);
    assert_eq!(c.grad(0), 0.0);
    assert_eq!(c.grad(7), 0.0);
    let x = Dual::variable(5.0, 2, 3);
    let s = &x * &c + 1.0;
    assert_eq!(s.v, 11.0);
    assert_eq!(s.grad(2), 2.0);
    assert_eq!(s.grad(0), 0.0);
    let n = -s;
    assert_eq!(n.v, -11.0);
    assert_eq!(n.grad(2), -2.0);
}

#[test]
fn sqrt_at_zero_has_a_finite_gradient() {
    let x = Dual::variable(0.0, 0, 1);
    let r = x.sqrt();
    assert_eq!(r.v, 0.0);
    assert!(r.grad(0).is_finite());
    let neg = Dual::variable(-1e-30, 0, 1).sqrt();
    assert!(neg.v.is_finite() && neg.grad(0).is_finite());
}

#[test]
fn sin_and_cos_carry_the_chain_rule() {
    let x = Dual::variable(0.8, 0, 2);
    let y = Dual::variable(-1.1, 1, 2);
    let u = &x * &y;
    let s = u.sin();
    let c = u.cos();
    assert!((s.v - (0.8_f64 * -1.1).sin()).abs() < 1e-15);
    assert!((c.v - (0.8_f64 * -1.1).cos()).abs() < 1e-15);
    // d sin(xy)/dx = cos(xy)·y, d cos(xy)/dy = −sin(xy)·x
    assert!((s.grad(0) - c.v * -1.1).abs() < 1e-15);
    assert!((c.grad(1) + s.v * 0.8).abs() < 1e-15);
    assert_eq!(Dual::constant(1.0).sin().grad(0), 0.0);
}
