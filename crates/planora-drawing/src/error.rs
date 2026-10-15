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
        "Bei „{0}“ lässt sich nicht sicher sagen, ob die Linie verdeckt ist. Ein anderer Maßstab hilft nicht; eine andere Ansicht kann gehen."
    )]
    AmbiguousVisibility(String),
    /// Eine Arbeitsgrenze ist überschritten; die Zahl kann Striche, Teilstücke, Paartests,
    /// Sichtprüfungen oder Umrisspunkte zählen.
    #[error("Der Körper ist für diese Ansicht zu aufwendig ({0} Arbeitsschritte).")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_name_what_helps_and_stay_neutral() {
        assert_eq!(
            DrawError::AmbiguousVisibility("Kante 3".into()).to_string(),
            "Bei „Kante 3“ lässt sich nicht sicher sagen, ob die Linie verdeckt ist. Ein anderer Maßstab hilft nicht; eine andere Ansicht kann gehen."
        );
        assert_eq!(
            DrawError::TooComplex(123).to_string(),
            "Der Körper ist für diese Ansicht zu aufwendig (123 Arbeitsschritte)."
        );
    }
}
