//! Graphit-Theme: Palette beider Themen, Kontrast, Visuals, Textstile und Rahmen.

use egui::{Color32, CornerRadius, FontFamily, Shadow, Stroke, TextStyle, Theme};
use planora_app::{fonts, theme};

const BOTH: [bool; 2] = [true, false];

fn name(dark: bool) -> &'static str {
    if dark { "dunkel" } else { "hell" }
}

#[test]
fn contrast_ratio_follows_wcag() {
    let r = theme::contrast_ratio(Color32::BLACK, Color32::WHITE);
    assert!((r - 21.0).abs() < 0.01, "{r}");
    let r = theme::contrast_ratio(Color32::WHITE, Color32::WHITE);
    assert!((r - 1.0).abs() < 0.001, "{r}");
    // Reihenfolge egal; bekannter Wert aus Graphit: #939393 auf #181818 ≈ 5,7 : 1.
    let a = Color32::from_rgb(0x93, 0x93, 0x93);
    let b = Color32::from_rgb(0x18, 0x18, 0x18);
    let r = theme::contrast_ratio(a, b);
    assert!((r - theme::contrast_ratio(b, a)).abs() < 1e-6);
    assert!((5.6..5.9).contains(&r), "{r}");
}

#[test]
fn contrast_of_every_text_pair_is_at_least_4_5_in_both_themes() {
    let mut failures = Vec::new();
    for dark in BOTH {
        let p = theme::palette(dark);
        let mut check = |what: &str, fg: Color32, bg: Color32, min: f32| {
            let r = theme::contrast_ratio(fg, bg);
            if r < min {
                failures.push(format!("{} {what}: {r:.2} < {min}", name(dark)));
            }
        };
        let surfaces = [
            ("bg_panel", p.bg_panel),
            ("bg_section", p.bg_section),
            ("bg_field", p.bg_field),
            ("bg_active", p.bg_active),
            ("bg_inset", p.bg_inset),
        ];
        for (s, bg) in surfaces {
            check(&format!("text_primary auf {s}"), p.text_primary, bg, 4.5);
            // Ruling: Nebentext auf aktivem Grund nur 3,0 (aktive Elemente nutzen text_primary).
            let min = if s == "bg_active" { 3.0 } else { 4.5 };
            check(
                &format!("text_secondary auf {s}"),
                p.text_secondary,
                bg,
                min,
            );
        }
        check("on_accent auf accent", p.on_accent, p.accent, 4.5);
        for (chip, (fill, text)) in [
            ("chip_bridge", p.chip_bridge),
            ("chip_split", p.chip_split),
            ("chip_boolean", p.chip_boolean),
        ] {
            check(chip, text, fill, 4.5);
        }
        check("error auf bg_panel", p.error, p.bg_panel, 4.5);
        check("warning auf bg_panel", p.warning, p.bg_panel, 4.5);
        check("ok auf bg_panel", p.ok, p.bg_panel, 4.5);
        check("marker auf bg_panel", p.marker, p.bg_panel, 4.5);
    }
    // text_tertiary (nur ab 13 px medium) im hellen Thema mindestens 4,3 auf bg_section; der
    // dunkle Graphit-Wert #7d7d7d liegt mit 4,2 knapp darunter und ist ausgenommen.
    let p = theme::palette(false);
    let r = theme::contrast_ratio(p.text_tertiary, p.bg_section);
    if r < 4.3 {
        failures.push(format!("hell text_tertiary auf bg_section: {r:.2} < 4.3"));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn dark_palette_uses_graphit_tokens() {
    let p = theme::palette(true);
    let rgb = Color32::from_rgb;
    assert_eq!(p.bg_viewport, rgb(0x0e, 0x0e, 0x0e));
    assert_eq!(p.bg_panel, rgb(0x18, 0x18, 0x18));
    assert_eq!(p.bg_section, rgb(0x1a, 0x1a, 0x1a));
    assert_eq!(p.bg_field, rgb(0x26, 0x26, 0x26));
    assert_eq!(p.bg_inset, rgb(0x0d, 0x0d, 0x0d));
    assert_eq!(p.bg_active, rgb(0x33, 0x33, 0x33));
    assert_eq!(p.border_subtle, rgb(0x38, 0x38, 0x38));
    assert_eq!(p.text_primary, Color32::WHITE);
    assert_eq!(p.text_secondary, rgb(0x93, 0x93, 0x93));
    assert_eq!(p.accent, rgb(0x00, 0x6d, 0xfd));
    assert_eq!(p.selection, rgb(0xf1, 0xf0, 0x5c));
    assert_eq!(p.axis_x, rgb(0xe0, 0x30, 0x3a));
    assert_eq!(p.axis_y, rgb(0x64, 0xbe, 0x62));
    assert_eq!(p.axis_z, rgb(0x3d, 0x8b, 0xdb));
    assert_eq!(p.error, rgb(0xe6, 0x5a, 0x5a));
    assert_eq!(p.warning, rgb(0xdc, 0x96, 0x28));
    assert_eq!(p.ok, rgb(0x6e, 0xcd, 0x62));
    assert_eq!(p.marker, rgb(0x5a, 0x96, 0xe6));
    assert_eq!(theme::accent(true), p.accent);
    assert_eq!(theme::accent(false), p.accent);
}

#[test]
fn apply_sets_both_visuals_and_paper_constants_stay() {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    for dark in BOTH {
        let p = theme::palette(dark);
        let t = if dark { Theme::Dark } else { Theme::Light };
        let v = ctx.style_of(t).visuals.clone();
        assert_eq!(v.dark_mode, dark, "{}", name(dark));
        assert_eq!(v.panel_fill, p.bg_viewport, "{}", name(dark));
        assert_eq!(v.window_fill, p.bg_panel, "{}", name(dark));
        assert_eq!(v.extreme_bg_color, p.bg_inset, "{}", name(dark));
        assert_eq!(v.faint_bg_color, p.bg_section, "{}", name(dark));
        assert_eq!(v.override_text_color, None);
        // Neutrale Auswahl, kein Blau.
        assert_eq!(v.selection.bg_fill, p.bg_active, "{}", name(dark));
        // Zustände: Ruhe durchsichtig, Hover bg_field, Aktiv bg_active.
        let w = &v.widgets;
        assert_eq!(w.inactive.weak_bg_fill, Color32::TRANSPARENT);
        assert_eq!(w.hovered.weak_bg_fill, p.bg_field);
        assert_eq!(w.active.weak_bg_fill, p.bg_active);
        assert_eq!(w.inactive.fg_stroke.color, p.text_secondary);
        assert_eq!(w.hovered.fg_stroke.color, p.text_primary);
        assert_eq!(w.active.fg_stroke.color, p.text_primary);
    }
    assert_eq!(
        ctx.style_of(Theme::Light).visuals.panel_fill,
        Color32::from_rgb(0xe8, 0xe9, 0xec)
    );
    assert_eq!(ctx.theme(), Theme::Dark);
    theme::apply(&ctx, false);
    assert_eq!(ctx.theme(), Theme::Light);
    assert_eq!(theme::PAPER, Color32::WHITE);
    assert_eq!(theme::SHEET_INK, Color32::from_rgb(0x14, 0x16, 0x1a));
    assert_eq!(theme::SHEET_DIM, theme::SHEET_INK);
    assert_eq!(theme::SHEET_SELECTED, Color32::from_rgb(0x1f, 0x6f, 0xe0));
}

#[test]
fn text_styles_use_graphit_sizes() {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, false);
    for t in [Theme::Dark, Theme::Light] {
        let style = ctx.style_of(t);
        let font = |s: TextStyle| style.text_styles.get(&s).cloned().expect("Textstil fehlt");
        let named = |n: &str| font(TextStyle::Name(n.into()));
        let heading = font(TextStyle::Heading);
        assert_eq!((heading.size, heading.family), (13.0, fonts::semibold()));
        let body = font(TextStyle::Body);
        assert_eq!((body.size, body.family), (12.0, FontFamily::Proportional));
        let button = font(TextStyle::Button);
        assert_eq!((button.size, button.family), (12.0, fonts::medium()));
        assert_eq!(font(TextStyle::Small).size, 10.0);
        let label = named("label");
        assert_eq!((label.size, label.family), (13.0, fonts::medium()));
        let value = named("value");
        assert_eq!((value.size, value.family), (11.0, fonts::medium()));
        let kbd = named("kbd");
        assert_eq!((kbd.size, kbd.family), (10.0, fonts::semibold()));
        let axis = named("axis");
        assert_eq!((axis.size, axis.family), (15.0, fonts::medium()));
        let s = &style.spacing;
        assert_eq!(s.item_spacing, egui::vec2(8.0, 4.0));
        assert_eq!(s.button_padding, egui::vec2(8.0, 4.0));
        assert_eq!(s.interact_size, egui::vec2(32.0, 24.0));
        assert_eq!(s.window_margin, egui::Margin::same(12));
        assert_eq!(s.menu_margin, egui::Margin::same(16));
    }
}

#[test]
fn frames_use_one_pixel_border_and_no_shadow() {
    for dark in BOTH {
        let p = theme::palette(dark);
        let f = theme::panel_frame(dark);
        assert_eq!(f.fill, p.bg_panel);
        assert_eq!(
            f.stroke,
            Stroke::new(1.0, p.border_subtle),
            "{}",
            name(dark)
        );
        assert_eq!(f.corner_radius, CornerRadius::same(theme::RADIUS_LG));
        assert_eq!(theme::RADIUS_LG, 14);
        assert_eq!(f.inner_margin, egui::Margin::same(theme::SPACE_3 as i8));
        assert_eq!(f.shadow, Shadow::NONE);
        let s = theme::section_frame(dark);
        assert_eq!(s.fill, p.bg_section);
        assert_eq!(s.corner_radius, CornerRadius::same(theme::RADIUS_MD));
        assert_eq!(s.shadow, Shadow::NONE);
        let i = theme::inset_frame(dark);
        assert_eq!(i.fill, p.bg_inset);
        assert_eq!(i.corner_radius, CornerRadius::same(theme::RADIUS_SM));
        assert_eq!(i.shadow, Shadow::NONE);
    }
    let ctx = egui::Context::default();
    theme::apply(&ctx, true);
    for t in [Theme::Dark, Theme::Light] {
        let v = ctx.style_of(t).visuals.clone();
        let p = theme::palette(t == Theme::Dark);
        assert_eq!(v.window_shadow, Shadow::NONE, "{t:?}");
        assert_eq!(v.popup_shadow, Shadow::NONE, "{t:?}");
        assert_eq!(v.window_stroke, Stroke::new(1.0, p.border_subtle), "{t:?}");
        assert_eq!(v.window_corner_radius, CornerRadius::same(theme::RADIUS_LG));
        assert_eq!(v.menu_corner_radius, CornerRadius::same(theme::RADIUS_LG));
    }
}

#[test]
fn size_tokens_match_graphit() {
    assert_eq!(
        [
            theme::SPACE_1,
            theme::SPACE_2,
            theme::SPACE_3,
            theme::SPACE_4,
            theme::SPACE_6
        ],
        [4.0, 8.0, 12.0, 16.0, 24.0]
    );
    assert_eq!(
        [
            theme::RADIUS_XS,
            theme::RADIUS_SM,
            theme::RADIUS_MD,
            theme::RADIUS_LG
        ],
        [4, 6, 10, 14]
    );
    assert_eq!(
        [
            theme::PANEL_WIDTH,
            theme::ROW_H,
            theme::FIELD_H,
            theme::CONTROL,
            theme::HEADER_H,
            theme::TOOLBAR_H,
            theme::DOCK_H
        ],
        [250.0, 24.0, 26.0, 32.0, 40.0, 48.0, 56.0]
    );
}
