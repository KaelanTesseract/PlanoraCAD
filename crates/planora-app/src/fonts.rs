//! Schriften der Oberfläche. Alle Dateien liegen unverändert in `assets/fonts` (Herkunft und
//! Prüfsummen in der dortigen README):
//! - Montserrat (Regular, Medium, SemiBold): SIL Open Font License 1.1, Haupttext der Oberfläche.
//! - Roboto: Apache-2.0, Ersatzschrift für Zeichen, die Montserrat nicht hat, und Festbreiten-Text.
//! - Phosphor (Light): MIT, Symbolschrift (private Codepunkte, nur über die Familie `icons`).
//!
//! Die in egui eingebauten Standardschriften sind abgeschaltet.

use egui::{FontData, FontFamily};

const MONTSERRAT_REGULAR: &[u8] = include_bytes!("../assets/fonts/Montserrat-Regular.ttf");
const MONTSERRAT_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Montserrat-Medium.ttf");
const MONTSERRAT_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Montserrat-SemiBold.ttf");
const ROBOTO_REGULAR: &[u8] = include_bytes!("../assets/fonts/Roboto-Regular.ttf");
const PHOSPHOR_LIGHT: &[u8] = include_bytes!("../assets/fonts/Phosphor-Light.ttf");

/// Mittelstarke Schrift (Beschriftungen, Werte).
pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}

/// Halbfette Schrift (Überschriften, Tastenkürzel).
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Symbolschrift (Phosphor Light).
pub fn icons() -> FontFamily {
    FontFamily::Name("icons".into())
}

/// Schriftdefinitionen: Montserrat mit Roboto als Ersatz, Roboto für Festbreiten-Text,
/// Phosphor für Symbole.
pub fn definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::empty();
    for (name, bytes) in [
        ("Montserrat-Regular", MONTSERRAT_REGULAR),
        ("Montserrat-Medium", MONTSERRAT_MEDIUM),
        ("Montserrat-SemiBold", MONTSERRAT_SEMIBOLD),
        ("Roboto", ROBOTO_REGULAR),
        ("Phosphor-Light", PHOSPHOR_LIGHT),
    ] {
        fonts.font_data.insert(
            name.to_owned(),
            std::sync::Arc::new(FontData::from_static(bytes)),
        );
    }
    let families = [
        (
            FontFamily::Proportional,
            &["Montserrat-Regular", "Roboto"][..],
        ),
        (medium(), &["Montserrat-Medium", "Roboto"]),
        (semibold(), &["Montserrat-SemiBold", "Roboto"]),
        (icons(), &["Phosphor-Light"]),
        (FontFamily::Monospace, &["Roboto"]),
    ];
    for (family, names) in families {
        fonts
            .families
            .insert(family, names.iter().map(|n| (*n).to_owned()).collect());
    }
    fonts
}
