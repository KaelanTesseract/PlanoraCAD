//! Fehler der Zeichnungserzeugung (Meldungen in der aktuellen Sprache).

use planora_exchange::ExchangeError;
use planora_i18n::tr;

/// Fehler beim Berechnen oder Schreiben einer Zeichnung. Der Text entsteht in der aktuellen
/// Sprache ([`planora_i18n::current`]).
#[derive(Debug, thiserror::Error)]
#[error("{}", self.text())]
pub enum DrawError {
    NoBodies,
    MeshBodiesOnly,
    AmbiguousVisibility(String),
    /// Eine Arbeitsgrenze ist überschritten; die Zahl kann Striche, Teilstücke, Paartests,
    /// Sichtprüfungen oder Umrisspunkte zählen.
    TooComplex(u64),
    SectionMisses,
    SectionFailed(String),
    NonFinite,
    Cancelled,
    Io(#[from] std::io::Error),
    InvalidBody(String),
    Unsupported(String),
}

impl DrawError {
    /// Die Meldung in der aktuellen Sprache.
    fn text(&self) -> String {
        match self {
            DrawError::NoBodies => tr!("drawing-no-bodies"),
            DrawError::MeshBodiesOnly => tr!("drawing-mesh-bodies-only"),
            DrawError::AmbiguousVisibility(what) => {
                tr!("drawing-err-ambiguous", what = what.as_str())
            }
            DrawError::TooComplex(n) => tr!("drawing-err-too-complex", n = n.to_string()),
            DrawError::SectionMisses => tr!("drawing-err-section-misses"),
            DrawError::SectionFailed(what) => {
                tr!("drawing-err-section-failed", what = what.as_str())
            }
            DrawError::NonFinite => tr!("drawing-err-non-finite"),
            DrawError::Cancelled => tr!("drawing-err-cancelled"),
            DrawError::Io(e) => io_text(e),
            DrawError::InvalidBody(detail) => {
                tr!("drawing-err-invalid-body", detail = detail.as_str())
            }
            DrawError::Unsupported(detail) => {
                tr!("drawing-err-unsupported", detail = detail.as_str())
            }
        }
    }
}

/// Meldung zu einem Schreibfehler; Windows-Fehler 32/33 (Datei gesperrt) bekommen einen Hinweis.
fn io_text(e: &std::io::Error) -> String {
    let mut t = tr!("drawing-err-io", msg = e.to_string());
    if matches!(e.raw_os_error(), Some(32 | 33)) {
        t.push(' ');
        t.push_str(&tr!("drawing-err-io-locked"));
    }
    t
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
        assert_eq!(
            DrawError::SectionMisses.to_string(),
            "Die Schnittebene trifft den Körper nicht."
        );
        assert_eq!(
            DrawError::SectionFailed("Körper 2".into()).to_string(),
            "Bei „Körper 2“ lässt sich der Schnitt nicht berechnen."
        );
    }
}
