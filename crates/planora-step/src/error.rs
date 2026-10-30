//! Fehler beim Schreiben von STEP-Dateien (Meldungen deutsch, für die Oberfläche bestimmt).

use planora_i18n::tr;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepError {
    Io { msg: String },
    NotFinite,
    InvalidBody { name: String, reason: String },
    NothingToExport,
    TooManyEntities,
    Cancelled,
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            StepError::Io { msg } => tr!("step-write-io", msg = msg),
            StepError::NotFinite => tr!("step-not-finite"),
            StepError::InvalidBody { name, reason } => {
                tr!("step-invalid-body", name = name, reason = reason)
            }
            StepError::NothingToExport => tr!("step-nothing-to-export"),
            StepError::TooManyEntities => tr!("step-too-many-entities-out"),
            StepError::Cancelled => tr!("step-cancelled"),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for StepError {}

impl From<std::io::Error> for StepError {
    fn from(e: std::io::Error) -> Self {
        StepError::Io { msg: e.to_string() }
    }
}
