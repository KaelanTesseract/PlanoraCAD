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

/// Jedes Nicht-ASCII-Zeichen in den Quelltexten der Oberfläche (Texte und Markierungen) muss in
/// der mitgelieferten Schrift vorhanden sein, sonst erscheinen Kästchen.
#[test]
fn every_non_ascii_character_in_the_ui_sources_exists_in_the_font() {
    let defs = fonts::definitions();
    let data = &defs.font_data["Roboto"].font;
    let face = ttf_parser::Face::parse(data, 0).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut missing = std::collections::BTreeSet::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let text = std::fs::read_to_string(e.path()).unwrap();
        for line in text.lines() {
            // Kommentare zählen nicht; "symbol(" hat eine ASCII-Ersatzschreibung, die gemalte
            // Markierung der Tabs ist ein Kreis (DIRTY_MARK steht nur im Text für Tests).
            if line.trim_start().starts_with("//")
                || line.contains("symbol(ctx")
                || line.contains("DIRTY_MARK: char")
            {
                continue;
            }
            for c in line.chars().filter(|c| !c.is_ascii()) {
                if face.glyph_index(c).is_none() {
                    missing.insert(c);
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "Zeichen fehlen in der Schrift: {missing:?}"
    );
}
