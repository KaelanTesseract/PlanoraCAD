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
            accent(true),
        )
    } else {
        (
            Color32::from_rgb(0xf3, 0xf4, 0xf7),
            Color32::from_rgb(0xff, 0xff, 0xff),
            Color32::from_rgb(0xff, 0xff, 0xff),
            Color32::from_rgb(0xea, 0xec, 0xf1),
            Color32::from_rgb(0xe3, 0xe6, 0xed),
            Color32::from_rgb(0xd5, 0xda, 0xe4),
            accent(false),
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

/// Akzentfarbe (Auswahl, aktive Schaltflächen).
pub fn accent(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x4c, 0x9a, 0xff)
    } else {
        Color32::from_rgb(0x1f, 0x6f, 0xe0)
    }
}

/// Papier einer Zeichnung: auch im dunklen Thema weiß.
pub const PAPER: Color32 = Color32::WHITE;
/// Linien der Ansichten auf dem Papier.
pub const SHEET_INK: Color32 = Color32::from_rgb(0x14, 0x16, 0x1a);
/// Innenrahmen und Blattkante.
pub const SHEET_FRAME: Color32 = Color32::from_rgb(0x50, 0x55, 0x60);
/// Rahmen und Text einer Ansicht, die noch berechnet wird.
pub const SHEET_PENDING: Color32 = Color32::from_rgb(0x8a, 0x90, 0x9c);
/// Maße und Mittellinien auf dem Papier: schwarz wie die Linien der Ansichten, auch im dunklen
/// Thema.
pub const SHEET_DIM: Color32 = SHEET_INK;
/// Maße, die ihren Bezug verloren haben (rot auf dem weißen Papier).
pub const SHEET_LOST: Color32 = Color32::from_rgb(0xd0, 0x1c, 0x1c);
/// Gewähltes Maß auf dem Papier: die Akzentfarbe des hellen Themas, weil das Papier in beiden
/// Themen weiß ist (die hellere Akzentfarbe des dunklen Themas wäre auf Weiß schwach).
pub const SHEET_SELECTED: Color32 = Color32::from_rgb(0x1f, 0x6f, 0xe0);
/// Schatten unter dem Papier.
pub const PAPER_SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 70);

/// Umfeld des Blatts im Zeichnungsbereich.
pub fn drawing_surround(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x2a, 0x2d, 0x34)
    } else {
        Color32::from_rgb(0xd9, 0xdc, 0xe3)
    }
}

/// Hintergrundfarbe des 3D-Viewports.
pub fn viewport_background(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x12, 0x14, 0x18)
    } else {
        Color32::from_rgb(0xf8, 0xf9, 0xfb)
    }
}
