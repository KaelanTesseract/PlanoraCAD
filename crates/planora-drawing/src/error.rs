//! Fehler der Zeichnungserzeugung (deutsche Meldungen).

use planora_exchange::ExchangeError;

/// Fehler beim Berechnen oder Schreiben einer Zeichnung.
#[derive(Debug, thiserror::Error)]
pub enum DrawError {
    #[error("Es gibt keinen Körper, der sich zeichnen lässt.")]
    NoBodies,
    #[error("Alle Körper sind Netze; eine Zeichnung braucht exakte Körper.")]
    MeshBodiesOnly,
    #[error(
        "Bei „{0}“ lässt sich nicht sicher sagen, ob die Linie verdeckt ist. Eine andere Ansicht oder ein anderer Maßstab hilft nicht; bitte melden Sie den Körper."
    )]
    AmbiguousVisibility(String),
    #[error("Der Körper ist für diese Ansicht zu aufwendig ({0} Teilstücke).")]
    TooComplex(u64),
    #[error("Ein Wert im Körper ist keine gültige Zahl.")]
    NonFinite,
    #[error("Abgebrochen.")]
    Cancelled,
    #[error("Die Datei lässt sich nicht schreiben: {0}")]
    Io(#[from] std::io::Error),
    #[error("Der Körper ist fehlerhaft: {0}")]
    InvalidBody(String),
    #[error("Das wird nicht unterstützt: {0}")]
    Unsupported(String),
}

impl From<ExchangeError> for DrawError {
    fn from(e: ExchangeError) -> Self {
        match e {
            ExchangeError::Cancelled => DrawError::Cancelled,
            other => DrawError::Unsupported(other.to_string()),
        }
    }
}
