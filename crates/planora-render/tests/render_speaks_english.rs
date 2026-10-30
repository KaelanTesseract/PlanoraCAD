//! Task 12: Die Namen am Navigationswürfel erscheinen unter `with_lang(&english())` auf
//! Englisch; unter Deutsch bleiben sie wie bisher.

use planora_i18n::{english, german, with_lang};
use planora_render::{CubeRegion, ViewFace};

#[test]
fn the_view_cube_speaks_english() {
    with_lang(&english(), || {
        let labels: Vec<String> = ViewFace::ALL.iter().map(|f| f.label()).collect();
        assert_eq!(labels, ["Front", "Back", "Left", "Right", "Top", "Bottom"]);
        assert_eq!(
            CubeRegion::Edge {
                a: ViewFace::Top,
                b: ViewFace::Right
            }
            .name(),
            "Top-Right"
        );
        assert_eq!(
            CubeRegion::Corner {
                x: ViewFace::Right,
                y: ViewFace::Front,
                z: ViewFace::Top
            }
            .name(),
            "Top-Front-Right"
        );
    });
}

#[test]
fn the_view_cube_stays_german_in_german() {
    with_lang(&german(), || {
        let labels: Vec<String> = ViewFace::ALL.iter().map(|f| f.label()).collect();
        assert_eq!(
            labels,
            ["Vorne", "Hinten", "Links", "Rechts", "Oben", "Unten"]
        );
        assert_eq!(CubeRegion::Face(ViewFace::Top).name(), "Oben");
    });
}
