use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum SketchError {
    #[error("Ungültige Eingabe: {0}")]
    InvalidInput(String),
    #[error("Das Objekt gibt es in dieser Skizze nicht")]
    UnknownObject,
    #[error("Entartete Geometrie: {0}")]
    Degenerate(String),
    #[error("Falscher Objekttyp: {0}")]
    WrongType(String),
}
