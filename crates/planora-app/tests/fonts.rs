use planora_app::fonts;

#[test]
fn beide_familien_haben_eine_schrift() {
    let defs = fonts::definitions();
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let names = defs.families.get(&family).expect("Familie vorhanden");
        assert!(!names.is_empty());
        for name in names {
            assert!(!defs.font_data[name].font.is_empty());
        }
    }
}

/// Zeichen außerhalb von ASCII, die in Roboto vorhanden sind (einmal gegen die Zeichentabelle der
/// Schrift geprüft) und in Oberflächentexten vorkommen dürfen. Alles andere erscheint als
/// Kästchen (z. B. ●, ▸, →) und ist verboten.
const ALLOWED: &str = "ÄÖÜäöüß„“–−·×°±²³…ØΔ≈";

/// Jedes Nicht-ASCII-Zeichen in den Quelltexten der Oberfläche (außerhalb von Kommentaren)
/// muss in der Erlaubnisliste stehen. Ausgenommen sind Zeilen mit ASCII-Ersatz in der
/// Skizzenansicht und der gezeichnete Tab-Punkt (`DIRTY_MARK`).
#[test]
fn every_non_ascii_character_in_the_ui_sources_is_known_to_exist_in_the_font() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bad = std::collections::BTreeSet::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let text = std::fs::read_to_string(e.path()).unwrap();
        for line in text.lines() {
            if line.trim_start().starts_with("//")
                || line.contains("symbol(ctx")
                || line.contains("DIRTY_MARK: char")
            {
                continue;
            }
            bad.extend(
                line.chars()
                    .filter(|c| !c.is_ascii() && !ALLOWED.contains(*c)),
            );
        }
    }
    assert!(bad.is_empty(), "Zeichen ohne Glyph in Roboto: {bad:?}");
}
