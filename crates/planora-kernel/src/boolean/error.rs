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
    /// Nicht lösbare Lage mit benannter Ursache (etwa vom numerischen Schnittrechner).
    #[error("{0}")]
    Unsupported(String),
    #[error(
        "Die Körper berühren sich nur an einer Kante oder tangential oder liegen zu knapp beieinander; diese Lage lässt sich nicht sicher auswerten"
    )]
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

/// Name der Freiformfläche in Meldungen (wie `surface_name`).
const FREEFORM_NAME: &str = "Freiformfläche";

impl BooleanError {
    /// Enthält die Meldung schon einen Vorschlag des Benutzers („…; bitte …“)? Das entscheidet
    /// die Fehlerart, nicht der Wortlaut (der je nach Sprache wechselt): bisher nur die
    /// Arbeitsgrenze für Freiformflächen.
    pub fn has_user_hint(&self) -> bool {
        matches!(self, BooleanError::Unsupported(msg) if msg == super::budget::TOO_EXPENSIVE)
    }

    /// Stammt die Meldung aus einem Weg, den es nur mit Freiformflächen gibt (Schnittrechner,
    /// Einordnen gegen Freiformflächen, dünne Gebilde, Arbeitsgrenze, Kante neben der
    /// Freiformfläche)? Grenzen analytischer Wege (etwa Ebene gegen Kegel) zählen nicht, auch wenn
    /// ein Körper Freiformflächen hat. `AmbiguousContact` und `AmbiguousClassification` entstehen
    /// in der gemeinsamen Pipeline; ihre Ursache lässt sich nicht zuordnen, sie zählen nicht.
    pub fn is_freeform_limit(&self) -> bool {
        match self {
            BooleanError::UnsupportedIntersection { a, b } => {
                *a == FREEFORM_NAME || *b == FREEFORM_NAME
            }
            BooleanError::Unsupported(msg) => {
                [
                    super::budget::TOO_EXPENSIVE,
                    super::classify::FREEFORM_GRAZING,
                    super::classify::FREEFORM_RAYS_DISAGREE,
                    super::classify::OBLIQUE_ON_FREEFORM,
                    super::THIN_FEATURE,
                    super::uv::EDGE_OFF_FREEFORM,
                ]
                .contains(&msg.as_str())
                    || (msg.starts_with("Der Schnitt von ") && msg.contains(FREEFORM_NAME))
            }
            _ => false,
        }
    }
}
