//! Schriften der Oberfläche: Roboto (Apache-2.0), unverändert mitgeliefert.
//! Die in egui eingebauten Standardschriften sind abgeschaltet.

const ROBOTO_REGULAR: &[u8] = include_bytes!("../assets/fonts/Roboto-Regular.ttf");

/// Schriftdefinitionen: Roboto für proportionalen und für Festbreiten-Text.
pub fn definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::empty();
    fonts.font_data.insert(
        "Roboto".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(ROBOTO_REGULAR)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("Roboto".to_owned());
    }
    fonts
}
