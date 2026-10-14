//! Fehler beim Schreiben von STEP-Dateien (Meldungen deutsch, für die Oberfläche bestimmt).

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StepError {
    #[error("Die Datei lässt sich nicht schreiben: {msg}")]
    Io { msg: String },
    #[error("Eine Zahl ist zu groß oder keine gültige Zahl")]
    NotFinite,
    #[error("„{name}“ wird nicht ausgegeben: {reason}")]
    InvalidBody { name: String, reason: String },
    #[error("Es gibt keinen Körper, der sich als STEP-Datei ausgeben lässt")]
    NothingToExport,
    #[error("Die Datei würde zu groß: mehr als 500 Millionen Einträge")]
    TooManyEntities,
    #[error("Abgebrochen")]
    Cancelled,
}

impl From<std::io::Error> for StepError {
    fn from(e: std::io::Error) -> Self {
        StepError::Io { msg: e.to_string() }
    }
}
