use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum SketchError {
    #[error("Ungültige Eingabe: {0}")]
    InvalidInput(String),
}
