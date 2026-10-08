use egui::{Color32, CornerRadius, Visuals};

fn base(dark: bool) -> Visuals {
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let radius = CornerRadius::same(6);
    let (panel, window, extreme, faint, inactive, hovered, accent) = if dark {
        (
            Color32::from_rgb(0x1e, 0x21, 0x27),
            Color32::from_rgb(0x25, 0x28, 0x30),
            Color32::from_rgb(0x15, 0x17, 0x1b),
            Color32::from_rgb(0x26, 0x29, 0x31),
            Color32::from_rgb(0x2c, 0x30, 0x39),
            Color32::from_rgb(0x38, 0x3d, 0x49),
            Color32::from_rgb(0x4c, 0x9a, 0xff),
        )
    } else {
        (
            Color32::from_rgb(0xf3, 0xf4, 0xf7),
            Color32::from_rgb(0xff, 0xff, 0xff),
            Color32::from_rgb(0xff, 0xff, 0xff),
            Color32::from_rgb(0xea, 0xec, 0xf1),
            Color32::from_rgb(0xe3, 0xe6, 0xed),
            Color32::from_rgb(0xd5, 0xda, 0xe4),
            Color32::from_rgb(0x1f, 0x6f, 0xe0),
        )
    };
    v.panel_fill = panel;
    v.window_fill = window;
    v.extreme_bg_color = extreme;
    v.faint_bg_color = faint;
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = radius;
    v.widgets.inactive.weak_bg_fill = inactive;
    v.widgets.inactive.bg_fill = inactive;
    v.widgets.hovered.weak_bg_fill = hovered;
    v.widgets.hovered.bg_fill = hovered;
    v.widgets.active.weak_bg_fill = accent;
    v.widgets.active.bg_fill = accent;
    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.inactive.corner_radius = radius;
    v.widgets.hovered.corner_radius = radius;
    v.widgets.active.corner_radius = radius;
    v.selection.bg_fill = accent.linear_multiply(0.55);
    v.hyperlink_color = accent;
    v
}

/// Setzt Farben, Rundungen und Abstände für das gewünschte Theme.
pub fn apply(ctx: &egui::Context, dark: bool) {
    ctx.set_visuals_of(egui::Theme::Dark, base(true));
    ctx.set_visuals_of(egui::Theme::Light, base(false));
    ctx.set_theme(if dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.menu_margin = egui::Margin::same(6);
    });
}

/// Hintergrundfarbe des 3D-Viewports.
pub fn viewport_background(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x12, 0x14, 0x18)
    } else {
        Color32::from_rgb(0xf8, 0xf9, 0xfb)
    }
}
