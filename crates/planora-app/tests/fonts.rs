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
