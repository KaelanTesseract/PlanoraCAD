use planora_i18n::tr;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Difference,
    Intersection,
}

/// Fehler einer Booleschen Operation. Die Art des Fehlers steht in der Variante, nicht im Text
/// (der je nach Sprache wechselt): `TooExpensive` und `FreeformLimit` sind die Grenzen der
/// Freiform-Wege.
#[derive(Debug, Clone, PartialEq)]
pub enum BooleanError {
    /// Kennungen der Flächenarten (siehe `surface_name`), nicht ihre Anzeigenamen.
    UnsupportedIntersection {
        a: &'static str,
        b: &'static str,
    },
    /// Nicht lösbare Lage mit benannter Ursache; der Text steht schon in der Sprache der Meldung.
    Unsupported(String),
    /// Wie `Unsupported`, aber aus einem Weg, den es nur mit Freiformflächen gibt (Schnittrechner,
    /// Einordnen gegen Freiformflächen, dünne Gebilde, Kante neben der Freiformfläche).
    FreeformLimit(String),
    /// Arbeitsgrenze für Freiformflächen erreicht (die Meldung enthält schon einen Vorschlag).
    TooExpensive,
    AmbiguousContact,
    AmbiguousClassification,
    NotClosed,
    ResultInvalid(String),
    EmptyResult,
    InvalidInput(String),
}

impl fmt::Display for BooleanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            BooleanError::UnsupportedIntersection { a, b } => tr!(
                "kernel-boolean-unsupported-intersection",
                a = super::surface_label(a),
                b = super::surface_label(b)
            ),
            BooleanError::Unsupported(msg) | BooleanError::FreeformLimit(msg) => msg.clone(),
            BooleanError::TooExpensive => super::budget::too_expensive(),
            BooleanError::AmbiguousContact => tr!("kernel-boolean-ambiguous-contact"),
            BooleanError::AmbiguousClassification => tr!("kernel-boolean-ambiguous-classification"),
            BooleanError::NotClosed => tr!("kernel-boolean-not-closed"),
            BooleanError::ResultInvalid(m) => tr!("kernel-boolean-result-invalid", msg = m),
            BooleanError::EmptyResult => tr!("kernel-boolean-empty-result"),
            BooleanError::InvalidInput(m) => tr!("kernel-boolean-invalid-input", msg = m),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for BooleanError {}

/// Name der Freiformfläche in Meldungen (wie `surface_name`).
const FREEFORM_NAME: &str = "Freiformfläche";

impl BooleanError {
    /// Enthält die Meldung schon einen Vorschlag des Benutzers („…; bitte …“)? Das entscheidet
    /// die Fehlerart, nicht der Wortlaut (der je nach Sprache wechselt): bisher nur die
    /// Arbeitsgrenze für Freiformflächen.
    pub fn has_user_hint(&self) -> bool {
        matches!(self, BooleanError::TooExpensive)
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
            BooleanError::TooExpensive | BooleanError::FreeformLimit(_) => true,
            _ => false,
        }
    }
}
