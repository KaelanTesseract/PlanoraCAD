use egui::Color32;
use planora_style::{DARK, LIGHT, fonts, palette};

#[test]
fn palette_values_are_unchanged() {
    assert_eq!(DARK.bg_viewport, Color32::from_rgb(0x0e, 0x0e, 0x0e));
    assert_eq!(DARK.bg_panel, Color32::from_rgb(0x18, 0x18, 0x18));
    assert_eq!(DARK.accent, Color32::from_rgb(0x00, 0x6d, 0xfd));
    assert_eq!(LIGHT.bg_viewport, Color32::from_rgb(0xe8, 0xe9, 0xec));
    assert_eq!(LIGHT.bg_panel, Color32::from_rgb(0xff, 0xff, 0xff));
    assert_eq!(LIGHT.text_primary, Color32::from_rgb(0x15, 0x15, 0x15));
}

#[test]
fn palette_selects_theme() {
    assert_eq!(palette(true), &DARK);
    assert_eq!(palette(false), &LIGHT);
}

#[test]
fn fonts_have_three_families() {
    let defs = fonts::definitions();
    for family in [fonts::medium(), fonts::semibold(), fonts::icons()] {
        assert!(defs.families.contains_key(&family), "{family:?} fehlt");
    }
}
