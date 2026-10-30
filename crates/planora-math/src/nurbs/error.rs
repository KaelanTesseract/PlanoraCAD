//! Fehler der NURBS-Konstruktoren und -Rechnungen (Meldungen in der Sprache des Programms).

use planora_i18n::tr;
use std::fmt;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NurbsError {
    Degree(usize),
    Knots(String),
    Control(String),
    Weights(String),
    Fit(String),
    Input(String),
    TooLarge(String),
}

impl fmt::Display for NurbsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            NurbsError::Degree(d) => tr!(
                "math-err-degree",
                degree = d.to_string(),
                max = crate::nurbs::MAX_DEGREE.to_string()
            ),
            NurbsError::Knots(d) => tr!("math-err-knots", detail = d.as_str()),
            NurbsError::Control(d) => tr!("math-err-control", detail = d.as_str()),
            NurbsError::Weights(d) => tr!("math-err-weights", detail = d.as_str()),
            NurbsError::Fit(d) => tr!("math-err-fit", detail = d.as_str()),
            NurbsError::Input(d) => tr!("math-err-input", detail = d.as_str()),
            NurbsError::TooLarge(d) => tr!("math-err-too-large", detail = d.as_str()),
        };
        f.write_str(&text)
    }
}
