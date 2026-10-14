//! Zahlen im STEP-Format (REAL, Spec §4): kürzeste Darstellung, die verlustfrei zurückgelesen
//! wird (höchstens 17 gültige Stellen), immer mit Punkt, Exponent mit großem `E`.

use crate::error::StepError;

/// Hängt `x` als STEP-REAL an `out` an: `1.`, `0.5`, `-2.25`, `1.E-7`, `1.5E21`; `-0` wird `0.`.
/// Nicht endliche Werte ergeben `NotFinite`, `out` bleibt dann unverändert.
pub fn push_real(out: &mut String, x: f64) -> Result<(), StepError> {
    if !x.is_finite() {
        return Err(StepError::NotFinite);
    }
    // `-0.0 == 0.0`: beide werden `0.`.
    let x = if x == 0.0 { 0.0 } else { x };
    // `{:?}` ist die kürzeste Darstellung, die beim Lesen wieder genau `x` ergibt.
    let s = format!("{x:?}");
    match s.split_once('e') {
        Some((mantissa, exponent)) => {
            out.push_str(mantissa);
            if !mantissa.contains('.') {
                out.push('.');
            }
            out.push('E');
            out.push_str(exponent);
        }
        None => match s.strip_suffix(".0") {
            Some(whole) => {
                out.push_str(whole);
                out.push('.');
            }
            None => out.push_str(&s),
        },
    }
    Ok(())
}

/// `x` als STEP-REAL (siehe [`push_real`]).
pub fn real(x: f64) -> Result<String, StepError> {
    let mut s = String::new();
    push_real(&mut s, x)?;
    Ok(s)
}
