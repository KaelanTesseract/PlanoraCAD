use planora_kernel::boolean::BoolOp;

/// Prüft, ob das Ergebnisvolumen einer Booleschen Operation in den möglichen Grenzen liegt
/// (Spielraum `rel_tol` relativ zur Summe der Eingangsvolumen).
pub fn boolean_volume_plausible(
    op: BoolOp,
    v_a: f64,
    v_b: f64,
    v_result: f64,
    rel_tol: f64,
) -> bool {
    let eps = rel_tol * (v_a + v_b).max(1e-12);
    // auch NaN ablehnen
    if v_result.is_nan() || v_result <= -eps {
        return false;
    }
    match op {
        BoolOp::Union => v_result >= v_a.max(v_b) - eps && v_result <= v_a + v_b + eps,
        BoolOp::Difference => v_result >= v_a - v_b - eps && v_result <= v_a + eps,
        BoolOp::Intersection => v_result <= v_a.min(v_b) + eps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_per_operation() {
        let ok = |op, v| boolean_volume_plausible(op, 10.0, 5.0, v, 0.01);
        assert!(ok(BoolOp::Union, 12.0));
        assert!(!ok(BoolOp::Union, 16.0));
        assert!(ok(BoolOp::Difference, 5.0));
        assert!(!ok(BoolOp::Difference, 4.0));
        assert!(ok(BoolOp::Intersection, 0.0));
        assert!(!ok(BoolOp::Intersection, -1.0));
        assert!(!ok(BoolOp::Union, f64::NAN));
    }
}
