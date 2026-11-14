//! Graphit-Stil der Hülle: Farben, Schriftgrößen, Abstände, schwebende Felder und Rollbalken.
//! Nachgebildet nach `theme.rs` der App (planora-setup hängt nicht von planora-app ab); Werte
//! dort ändern heißt, sie hier mitzuziehen.

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Visuals, vec2,
};
use planora_style::{Palette, fonts, palette};

/// Abstand 4.
pub const SPACE_1: f32 = 4.0;
/// Abstand 8.
pub const SPACE_2: f32 = 8.0;
/// Abstand 12 (Innenabstand der Felder, Abstand der Felder zueinander und zum Fensterrand).
pub const SPACE_3: f32 = 12.0;
/// Abstand 16.
pub const SPACE_4: f32 = 16.0;
/// Rundung 6 (Knöpfe, Eingabefelder).
pub const RADIUS_SM: u8 = 6;
/// Rundung 10 (Karten, Bilder).
pub const RADIUS_MD: u8 = 10;
/// Rundung 14 (schwebende Felder), wie `theme::RADIUS_LG` der App.
pub const RADIUS_LG: u8 = 14;
/// Zeilenhöhe der Schrittleiste (wie die Zeilen im Feature-Baum).
pub const ROW_H: f32 = 28.0;
/// Größe eines Steuerelements.
pub const CONTROL: f32 = 32.0;
/// Deckkraft gesperrter Elemente, wie in der App.
const DISABLED_ALPHA: f32 = 0.75;
/// Dauer der Übergänge, wie in der App.
const ANIMATION_TIME: f32 = 0.12;

/// Schrift der Seitentitel (19 px halbfett, wie `.h1` der Vorschau).
pub fn title_font() -> FontId {
    FontId::new(19.0, fonts::semibold())
}

/// Schrift des Schriftzugs neben dem Logo (15 px halbfett).
pub fn brand_font() -> FontId {
    FontId::new(15.0, fonts::semibold())
}

/// Überschrift eines Felds (13 px halbfett, wie `heading` der App).
pub fn heading_font() -> FontId {
    FontId::new(13.0, fonts::semibold())
}

/// Zeilen der Schrittleiste und Kartentitel (13 px mittel, wie `label` der App).
pub fn label_font() -> FontId {
    FontId::new(13.0, fonts::medium())
}

/// Fließtext (13 px normal).
pub fn body_font() -> FontId {
    FontId::new(13.0, FontFamily::Proportional)
}

/// Kleine Zusätze: Prozent, Bildunterschriften, Kartenhinweise (11 px mittel, wie `value`).
pub fn small_font() -> FontId {
    FontId::new(11.0, fonts::medium())
}

/// Visuals nach dem Muster der App (`theme::visuals`).
fn visuals(dark: bool) -> Visuals {
    let p = palette(dark);
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.override_text_color = None;
    v.panel_fill = p.bg_viewport;
    v.window_fill = p.bg_panel;
    v.window_stroke = Stroke::new(1.0, p.border_subtle);
    v.window_corner_radius = CornerRadius::same(RADIUS_LG);
    v.menu_corner_radius = CornerRadius::same(RADIUS_LG);
    // Rand statt Schatten.
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.extreme_bg_color = p.bg_inset;
    v.faint_bg_color = p.bg_section;
    v.code_bg_color = p.bg_field;
    v.error_fg_color = p.error;
    v.warn_fg_color = p.warning;
    v.hyperlink_color = p.text_primary;
    v.weak_text_color = Some(p.text_tertiary);
    v.selection.bg_fill = p.bg_active;
    v.selection.stroke = Stroke::new(1.0, p.text_primary);
    v.text_cursor.stroke.color = p.accent;
    v.disabled_alpha = DISABLED_ALPHA;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.bg_panel;
    w.noninteractive.weak_bg_fill = p.bg_panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border_subtle);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text_secondary);
    // Abweichend von der App: Knöpfe in Ruhe mit Fläche bg_field und heller Schrift, wie die
    // Knöpfe `.b` der Vorschau (die App hat dort keine Fläche, weil ihre Leisten Symbolknöpfe
    // sind).
    w.inactive.weak_bg_fill = p.bg_field;
    w.inactive.bg_fill = p.bg_field;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border_subtle);
    w.inactive.fg_stroke = Stroke::new(1.5, p.text_primary);
    w.hovered.weak_bg_fill = p.bg_active;
    w.hovered.bg_fill = p.bg_field;
    w.hovered.bg_stroke = Stroke::new(1.0, p.text_tertiary);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text_primary);
    w.active.weak_bg_fill = p.bg_active;
    w.active.bg_fill = p.bg_active;
    w.active.bg_stroke = Stroke::new(1.0, p.text_tertiary);
    w.active.fg_stroke = Stroke::new(1.5, p.text_primary);
    w.open = w.active;
    for s in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        s.corner_radius = CornerRadius::same(RADIUS_SM);
        s.expansion = 0.0;
    }
    v
}

fn text_styles() -> std::collections::BTreeMap<TextStyle, FontId> {
    [
        (TextStyle::Heading, title_font()),
        (TextStyle::Body, body_font()),
        (TextStyle::Button, FontId::new(12.0, fonts::medium())),
        (TextStyle::Small, small_font()),
        (
            TextStyle::Monospace,
            FontId::new(11.0, FontFamily::Monospace),
        ),
    ]
    .into()
}

/// Schriften, Farben (Hell und Dunkel), Textstile und Abstände einmal setzen.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts::definitions());
    ctx.set_visuals_of(egui::Theme::Dark, visuals(true));
    ctx.set_visuals_of(egui::Theme::Light, visuals(false));
    ctx.all_styles_mut(|style| {
        style.text_styles = text_styles();
        style.animation_time = ANIMATION_TIME;
        let s = &mut style.spacing;
        s.item_spacing = vec2(SPACE_2, SPACE_1);
        // Knöpfe der Hülle etwas größer als die Symbolknöpfe der App (wie `.b` der Vorschau).
        s.button_padding = vec2(14.0, 6.0);
        s.interact_size = vec2(CONTROL, 24.0);
        s.scroll = scroll_style();
    });
}

/// Dunkel, wenn Windows dunkel meldet oder gar nichts meldet.
pub fn is_dark(system: Option<egui::Theme>) -> bool {
    system != Some(egui::Theme::Light)
}

/// Wählt das Thema (nur bei einer Änderung aufrufen).
pub fn set_theme(ctx: &egui::Context, dark: bool) {
    ctx.set_theme(if dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
}

/// Schwebendes Feld wie `theme::panel_frame` der App: Fläche `bg_panel`, 1-px-Rand
/// `border_subtle`, Rundung 14, Innenabstand 12, kein Schatten (Rand statt Schatten).
pub fn panel_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.bg_panel)
        .stroke(Stroke::new(1.0, p.border_subtle))
        .corner_radius(CornerRadius::same(RADIUS_LG))
        .inner_margin(Margin::same(SPACE_3 as i8))
}

/// Rollbalken wie `theme::scroll_style` der App: schwebend, in Ruhe unsichtbar, schmaler Griff.
fn scroll_style() -> egui::style::ScrollStyle {
    egui::style::ScrollStyle {
        floating: true,
        bar_width: 8.0,
        floating_width: 6.0,
        floating_allocated_width: 0.0,
        handle_min_length: 24.0,
        bar_inner_margin: SPACE_1,
        bar_outer_margin: 2.0,
        foreground_color: true,
        dormant_background_opacity: 0.0,
        active_background_opacity: 0.0,
        interact_background_opacity: 0.0,
        dormant_handle_opacity: 0.0,
        active_handle_opacity: 0.6,
        interact_handle_opacity: 1.0,
        ..egui::style::ScrollStyle::floating()
    }
}

/// Zeigt `area` mit Griff in `text_tertiary` (Ruhe) und `text_secondary` (Überfahren), wie
/// `theme::scroll_area` der App.
pub fn scroll_area<R>(
    ui: &mut egui::Ui,
    area: egui::ScrollArea,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let saved = ui.style().clone();
    let p = palette(ui.visuals().dark_mode);
    let w = &mut ui.style_mut().visuals.widgets;
    w.inactive.fg_stroke.color = p.text_tertiary;
    w.hovered.fg_stroke.color = p.text_secondary;
    w.active.fg_stroke.color = p.text_secondary;
    let content_style = saved.clone();
    let out = area.show(ui, |ui| {
        ui.set_style(content_style);
        add_contents(ui)
    });
    ui.set_style(saved);
    out.inner
}

/// Hauptknopf: Fläche `accent`, Schrift `on_accent` (wie `theme::primary_button` der App).
pub fn primary_button(text: String, p: &Palette) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(p.on_accent))
        .fill(p.accent)
        .stroke(Stroke::new(1.0, p.accent))
}

/// Fläche einer Karte: in Ruhe `bg_section`, gewählt `accent_subtle` (wie `theme::card_fill`).
pub fn card_fill(p: &Palette, selected: bool) -> Color32 {
    if selected {
        p.accent_subtle
    } else {
        p.bg_section
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_system_theme_means_dark() {
        assert!(is_dark(None));
        assert!(is_dark(Some(egui::Theme::Dark)));
        assert!(!is_dark(Some(egui::Theme::Light)));
    }

    #[test]
    fn panel_frame_matches_app_values() {
        let p = palette(true);
        let f = panel_frame(p);
        assert_eq!(f.fill, p.bg_panel);
        assert_eq!(f.stroke, Stroke::new(1.0, p.border_subtle));
        assert_eq!(f.corner_radius, CornerRadius::same(14));
        assert_eq!(f.inner_margin, Margin::same(12));
        assert_eq!(f.shadow, Shadow::NONE);
    }
}
