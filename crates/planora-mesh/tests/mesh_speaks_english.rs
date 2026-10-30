//! Task 12: Die Fehler der Vernetzung erscheinen unter `with_lang(&english())` auf Englisch;
//! unter Deutsch bleiben sie wie bisher.

use planora_i18n::{english, german, with_lang};
use planora_mesh::MeshError;

fn all_errors() -> Vec<MeshError> {
    vec![
        MeshError::Triangulation(3),
        MeshError::MissingPatch(4),
        MeshError::TooLarge,
        MeshError::OffSurface(5),
    ]
}

#[test]
fn every_mesh_error_speaks_english() {
    with_lang(&english(), || {
        for e in all_errors() {
            let text = e.to_string();
            assert!(!text.is_empty());
            assert!(!text.chars().any(|c| "äöüßÄÖÜ„“".contains(c)), "{text}");
            let looks_like_key = !text.contains(' ')
                && text.contains('-')
                && text
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            assert!(!looks_like_key, "{text}");
        }
    });
}

#[test]
fn german_texts_stay_as_they_were() {
    with_lang(&german(), || {
        assert_eq!(
            MeshError::Triangulation(3).to_string(),
            "Fläche 3: Triangulation fehlgeschlagen"
        );
        assert_eq!(
            MeshError::TooLarge.to_string(),
            "Netz zu groß: mehr als 2³² Eckpunkte oder Indizes"
        );
    });
}
