use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Difference,
    Intersection,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum BooleanError {
    #[error("Der Schnitt von {a} mit {b} wird noch nicht unterstützt")]
    UnsupportedIntersection { a: &'static str, b: &'static str },
    #[error("Die Körper berühren sich so, dass die Toleranz keine sichere Entscheidung zulässt")]
    AmbiguousContact,
    #[error("Die Lage eines Flächenstücks zum anderen Körper ließ sich nicht sicher bestimmen")]
    AmbiguousClassification,
    #[error("Das Ergebnis ist nicht geschlossen")]
    NotClosed,
    #[error("Das Ergebnis ist ungültig: {0}")]
    ResultInvalid(String),
    #[error("Das Ergebnis ist leer")]
    EmptyResult,
    #[error("Ein Eingangskörper ist ungültig: {0}")]
    InvalidInput(String),
}
