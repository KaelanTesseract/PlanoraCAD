//! Graphit-Theme: Farben, Schrift, Abstände, Rundungen und Rahmen für Hell und Dunkel an einer
//! Stelle. Dunkel nach dem Designsystem Graphit (`graphit-tokens.json`), Hell als eigener Entwurf
//! mit denselben Regeln (grau zuerst, Farbe nur mit Bedeutung, Rand statt Schatten).

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Visuals, vec2,
};

use crate::fonts;

/// Alle Farben eines Themas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// Hintergrund des 3D-Bereichs und des Fensters.
    pub bg_viewport: Color32,
    /// Fläche schwebender Felder, Leisten und Menüs.
    pub bg_panel: Color32,
    /// Abschnittsköpfe innerhalb eines Felds.
    pub bg_section: Color32,
    /// Suchfeld, kleine Knöpfe, Hover-Fläche, Schalter-Spur.
    pub bg_field: Color32,
    /// Vertiefte Flächen: Zahlenfelder, Befehlszeile.
    pub bg_inset: Color32,
    /// Gewählter, neutraler Zustand (Symbolknopf, Segment, Zeile).
    pub bg_active: Color32,
    /// 1-px-Rand jedes Felds und jeder Box.
    pub border_subtle: Color32,
    /// Gitterlinien des 3D-Bodens.
    pub viewport_grid: Color32,
    /// Jede fünfte Gitterlinie des 3D-Bodens.
    pub viewport_grid_major: Color32,
    /// Bodenachse durch den Ursprung.
    pub ground_axis: Color32,
    /// Werte, Titel, Menüeinträge, aktive Symbole.
    pub text_primary: Color32,
    /// Zeilen und Symbole in Ruhe.
    pub text_secondary: Color32,
    /// Abschnittsbeschriftungen ab 13 px medium (knapp unter 4,5 : 1 erlaubt).
    pub text_tertiary: Color32,
    /// Nur Platzhalter, nie echter Inhalt.
    pub text_muted: Color32,
    /// Einzige gesättigte Fläche: Modus ein, Tastaturfokus, aktive Auswahl.
    pub accent: Color32,
    /// Text auf der Akzentfarbe.
    pub on_accent: Color32,
    /// Hervorhebung der Auswahl im 3D-Bereich.
    pub selection: Color32,
    /// X-Achse.
    pub axis_x: Color32,
    /// Y-Achse.
    pub axis_y: Color32,
    /// Z-Achse.
    pub axis_z: Color32,
    /// Bernstein-Chip (Fläche, Text).
    pub chip_bridge: (Color32, Color32),
    /// Blauer Chip (Fläche, Text).
    pub chip_split: (Color32, Color32),
    /// Grüner Chip (Fläche, Text).
    pub chip_boolean: (Color32, Color32),
    /// Fehlertext.
    pub error: Color32,
    /// Warntext.
    pub warning: Color32,
    /// Erfolg.
    pub ok: Color32,
    /// Markierung (Rückroll-Zeile im Feature-Baum).
    pub marker: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub(crate) const DARK: Palette = Palette {
    bg_viewport: rgb(0x0e0e0e),
    bg_panel: rgb(0x181818),
    bg_section: rgb(0x1a1a1a),
    bg_field: rgb(0x262626),
    bg_inset: rgb(0x0d0d0d),
    bg_active: rgb(0x333333),
    border_subtle: rgb(0x383838),
    viewport_grid: rgb(0x333333),
    viewport_grid_major: rgb(0x4d4d4d),
    ground_axis: rgb(0x38733b),
    text_primary: rgb(0xffffff),
    text_secondary: rgb(0x939393),
    text_tertiary: rgb(0x7d7d7d),
    text_muted: rgb(0x737373),
    accent: rgb(0x006dfd),
    on_accent: rgb(0xffffff),
    selection: rgb(0xf1f05c),
    axis_x: rgb(0xe0303a),
    axis_y: rgb(0x64be62),
    axis_z: rgb(0x3d8bdb),
    chip_bridge: (rgb(0x251f0c), rgb(0xe2ab63)),
    // Graphit-Text #1f6dcc hat nur 3,4 : 1; aufgehellt wie in den Tokens vorgeschlagen.
    chip_split: (rgb(0x0a1d25), rgb(0x4a8fe6)),
    chip_boolean: (rgb(0x0f260c), rgb(0x6ecd62)),
    error: rgb(0xe65a5a),
    warning: rgb(0xdc9628),
    ok: rgb(0x6ecd62),
    marker: rgb(0x5a96e6),
};

const LIGHT: Palette = Palette {
    bg_viewport: rgb(0xe8e9ec),
    bg_panel: rgb(0xffffff),
    bg_section: rgb(0xf4f4f6),
    // Hover-Fläche und Aktiv-Fläche müssen sich vom Fensterhintergrund #e8e9ec abheben
    // (≥ 1,1 : 1): beide etwas dunkler als in der Vorschau (dort aktiv #dcdce0).
    bg_field: rgb(0xdadbdf),
    bg_inset: rgb(0xeeeef1),
    bg_active: rgb(0xcfd0d5),
    border_subtle: rgb(0xd4d4d8),
    viewport_grid: rgb(0xd8d9de),
    viewport_grid_major: rgb(0xb4b6bd),
    ground_axis: rgb(0x5aa65e),
    text_primary: rgb(0x151515),
    text_secondary: rgb(0x5f5f66),
    // Vorschau #7a7a82 hätte auf bg_section nur 3,9 : 1; #6e6e76 auf dem Fensterhintergrund
    // bg_viewport (Menü-, Tab- und Statusleiste) nur 4,2 : 1. #66666e erreicht dort 4,7 : 1.
    text_tertiary: rgb(0x66666e),
    text_muted: rgb(0x7a7a82),
    accent: rgb(0x006dfd),
    on_accent: rgb(0xffffff),
    selection: rgb(0xd9a400),
    axis_x: rgb(0xc8202a),
    // Etwas dunkler als der erste Entwurf, damit Linien auf dem hellen Viewport 3 : 1 erreichen.
    axis_y: rgb(0x36903a),
    axis_z: rgb(0x2a72c0),
    chip_bridge: (rgb(0xfbeed9), rgb(0x8a5a13)),
    chip_split: (rgb(0xdbe9fb), rgb(0x1456b0)),
    chip_boolean: (rgb(0xdef3da), rgb(0x2d7a22)),
    error: rgb(0xc62828),
    // Warnung, Erfolg und Markierung stehen auch auf dem Fensterhintergrund bg_viewport #e8e9ec
    // (Menü-, Tab- und Statusleiste): dunkler als im Entwurf, damit es dort 4,5 : 1 sind.
    warning: rgb(0x8c5200),
    ok: rgb(0x2a7020),
    marker: rgb(0x1a5cc4),
};

/// Palette des gewünschten Themas.
pub fn palette(dark: bool) -> &'static Palette {
    if dark { &DARK } else { &LIGHT }
}

/// Abstand 4.
pub const SPACE_1: f32 = 4.0;
/// Abstand 8.
pub const SPACE_2: f32 = 8.0;
/// Abstand 12 (Innenabstand der Felder).
pub const SPACE_3: f32 = 12.0;
/// Abstand 16.
pub const SPACE_4: f32 = 16.0;
/// Abstand 24 (Einzug schwebender Felder vom Fensterrand).
pub const SPACE_6: f32 = 24.0;

/// Rundung 4 (Chips, Tastenkürzel).
pub const RADIUS_XS: u8 = 4;
/// Rundung 6 (Zahlenfelder, Symbolknöpfe).
pub const RADIUS_SM: u8 = 6;
/// Rundung 10 (Abschnittsköpfe, Suchfeld).
pub const RADIUS_MD: u8 = 10;
/// Rundung 14 (schwebende Felder, Menüs).
pub const RADIUS_LG: u8 = 14;

/// Breite der Seitenfelder.
pub const PANEL_WIDTH: f32 = 250.0;
/// Zeilenhöhe.
pub const ROW_H: f32 = 24.0;
/// Höhe eines Zahlenfelds.
pub const FIELD_H: f32 = 26.0;
/// Größe eines Steuerelements (Symbolknopf).
pub const CONTROL: f32 = 32.0;
/// Höhe eines Abschnittskopfs.
pub const HEADER_H: f32 = 40.0;
/// Höhe eines Unterabschnittskopfs (Abschnitt innerhalb eines Bereichs).
pub const SUB_HEADER_H: f32 = 28.0;
/// Höhe der oberen Leiste.
pub const TOOLBAR_H: f32 = 48.0;
/// Höhe des unteren Werkzeugfelds.
pub const DOCK_H: f32 = 56.0;

/// WCAG-Kontrast zweier deckender Farben, von 1,0 (gleich) bis 21,0 (Schwarz auf Weiß).
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    fn channel(c: u8) -> f32 {
        let c = f32::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    fn luminance(c: Color32) -> f32 {
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Deckkraft gesperrter Elemente (`Visuals::disabled_alpha`).
pub const DISABLED_ALPHA: f32 = 0.75;

/// Dauer der Übergänge (Aufklappen, Hover-Wechsel), 120 ms.
pub const ANIMATION_TIME: f32 = 0.12;

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
    // Verweise in normaler Textfarbe (die Akzentfarbe hätte als Text zu wenig Kontrast).
    v.hyperlink_color = p.text_primary;
    // Schwache Texte (Hinweise) in text_tertiary statt halb durchsichtig (das unterschritte den
    // Kontrast auf den Flächen).
    v.weak_text_color = Some(p.text_tertiary);
    // Neutrale Auswahl, kein Blau.
    v.selection.bg_fill = p.bg_active;
    v.selection.stroke = Stroke::new(1.0, p.text_primary);
    v.text_cursor.stroke.color = p.accent;
    // Gesperrte Elemente blasser, aber lesbar: Nebentext erreicht so in beiden Themen 3 : 1 auf
    // Fenster, Feld, Abschnitt und Eingabe (egui-Vorgabe 0,5 ergäbe nur etwa 2 : 1).
    v.disabled_alpha = DISABLED_ALPHA;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.bg_panel;
    w.noninteractive.weak_bg_fill = p.bg_panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border_subtle);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text_secondary);
    // Ruhe: Knöpfe ohne Fläche (`weak_bg_fill`), aber mit 1-px-Rand, damit Knöpfe, Zahlenfelder,
    // Auswahllisten und Textfelder sichtbar bleiben (Symbolknöpfe der Leisten schalten den
    // Rahmen selbst ab). Kästchen, Optionsfelder und Schieberspur (`bg_fill`) behalten die
    // Spur-Fläche bg_field. Hover: Rand border_subtle → text_tertiary (Graphit-Zustandstabelle).
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_fill = p.bg_field;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border_subtle);
    w.inactive.fg_stroke = Stroke::new(1.5, p.text_secondary);
    w.hovered.weak_bg_fill = p.bg_field;
    w.hovered.bg_fill = p.bg_field;
    w.hovered.bg_stroke = Stroke::new(1.0, p.text_tertiary);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text_primary);
    w.active.weak_bg_fill = p.bg_active;
    w.active.bg_fill = p.bg_active;
    w.active.bg_stroke = Stroke::NONE;
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

/// Schrift `heading` (13 px halbfett): Abschnittsköpfe, Fenstertitel.
pub fn heading_font() -> FontId {
    FontId::new(13.0, fonts::semibold())
}

/// Schrift `label` (13 px mittel): Zeilen im Browser und in den Bereichen.
pub fn label_font() -> FontId {
    FontId::new(13.0, fonts::medium())
}

/// Schrift `value` (11 px mittel): Werte, Statuswörter, Zusätze.
pub fn value_font() -> FontId {
    FontId::new(11.0, fonts::medium())
}

/// Schrift `kbd` (10 px halbfett): Tastenkürzel.
pub fn kbd_font() -> FontId {
    FontId::new(10.0, fonts::semibold())
}

fn text_styles() -> std::collections::BTreeMap<TextStyle, FontId> {
    let named = |n: &str| TextStyle::Name(n.into());
    [
        (TextStyle::Heading, heading_font()),
        (TextStyle::Body, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(12.0, fonts::medium())),
        (TextStyle::Small, FontId::new(10.0, fonts::semibold())),
        (
            TextStyle::Monospace,
            FontId::new(11.0, FontFamily::Monospace),
        ),
        (named("label"), label_font()),
        (named("value"), value_font()),
        (named("kbd"), kbd_font()),
        (named("axis"), FontId::new(15.0, fonts::medium())),
    ]
    .into()
}

/// Setzt Farben (Hell und Dunkel), Textstile, Rundungen und Abstände und wählt das Thema.
pub fn apply(ctx: &egui::Context, dark: bool) {
    ctx.set_visuals_of(egui::Theme::Dark, visuals(true));
    ctx.set_visuals_of(egui::Theme::Light, visuals(false));
    ctx.set_theme(if dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
    ctx.all_styles_mut(|style| {
        style.text_styles = text_styles();
        style.animation_time = ANIMATION_TIME;
        let s = &mut style.spacing;
        s.item_spacing = vec2(SPACE_2, SPACE_1);
        s.button_padding = vec2(SPACE_2, SPACE_1);
        s.interact_size = vec2(CONTROL, ROW_H);
        s.window_margin = Margin::same(SPACE_3 as i8);
        s.menu_margin = Margin::same(SPACE_4 as i8);
        s.scroll = scroll_style();
    });
}

/// Rollbalken: schwebend über dem Inhalt (belegen keinen Platz), in Ruhe unsichtbar. Über der
/// Fläche erscheint ein schmaler, halb deckender Griff ohne Spur, über dem Griff wird er etwas
/// breiter. Die Farben setzt [`scroll_area`] aus der Palette ([`scroll_handle_colors`]).
pub fn scroll_style() -> egui::style::ScrollStyle {
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

/// Farben des Rollbalken-Griffs `(in Ruhe über der Fläche, beim Überfahren oder Ziehen)`.
pub fn scroll_handle_colors(dark: bool) -> (Color32, Color32) {
    let p = palette(dark);
    (p.text_tertiary, p.text_secondary)
}

/// Zeigt `area` mit Griff in den Farben aus [`scroll_handle_colors`] (egui nimmt die Schriftfarbe
/// der Knöpfe, beim Überfahren wäre das text_primary, also ein weißer Balken im dunklen Thema).
/// Der Inhalt behält den Stil des Aufrufers.
pub fn scroll_area<R>(
    ui: &mut egui::Ui,
    area: egui::ScrollArea,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let saved = ui.style().clone();
    let (rest, hover) = scroll_handle_colors(ui.visuals().dark_mode);
    let w = &mut ui.style_mut().visuals.widgets;
    w.inactive.fg_stroke.color = rest;
    w.hovered.fg_stroke.color = hover;
    w.active.fg_stroke.color = hover;
    let content_style = saved.clone();
    let out = area.show(ui, |ui| {
        ui.set_style(content_style);
        add_contents(ui)
    });
    ui.set_style(saved);
    out
}

/// Wie [`scroll_area`], zeichnet aber nur die sichtbaren von `total_rows` gleich hohen Zeilen
/// (`row_height` ohne Zeilenabstand; `ScrollArea::show_rows`).
pub fn scroll_area_rows<R>(
    ui: &mut egui::Ui,
    area: egui::ScrollArea,
    row_height: f32,
    total_rows: usize,
    add_contents: impl FnOnce(&mut egui::Ui, std::ops::Range<usize>) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let saved = ui.style().clone();
    let (rest, hover) = scroll_handle_colors(ui.visuals().dark_mode);
    let w = &mut ui.style_mut().visuals.widgets;
    w.inactive.fg_stroke.color = rest;
    w.hovered.fg_stroke.color = hover;
    w.active.fg_stroke.color = hover;
    let content_style = saved.clone();
    let out = area.show_rows(ui, row_height, total_rows, |ui, range| {
        ui.set_style(content_style);
        add_contents(ui, range)
    });
    ui.set_style(saved);
    out
}

/// Rahmen schwebender Felder, Leisten und Menüs: bg_panel, 1 px Rand, Rundung 14, kein Schatten.
pub fn panel_frame(dark: bool) -> egui::Frame {
    let p = palette(dark);
    egui::Frame::new()
        .fill(p.bg_panel)
        .stroke(Stroke::new(1.0, p.border_subtle))
        .corner_radius(CornerRadius::same(RADIUS_LG))
        .inner_margin(Margin::same(SPACE_3 as i8))
}

/// Abschnittskopf innerhalb eines Felds.
pub fn section_frame(dark: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(palette(dark).bg_section)
        .corner_radius(CornerRadius::same(RADIUS_MD))
        .inner_margin(Margin::symmetric(SPACE_2 as i8, SPACE_1 as i8))
}

/// Vertiefte Fläche (Zahlenfeld, Befehlszeile).
pub fn inset_frame(dark: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(palette(dark).bg_inset)
        .corner_radius(CornerRadius::same(RADIUS_SM))
        .inner_margin(Margin::symmetric(SPACE_2 as i8, SPACE_1 as i8))
}

/// Fläche eines Chips: beim Überfahren ein Stück, beim Drücken stärker zur Schriftfarbe `ink`
/// hin gemischt (beide aus den Chip-Farben der Palette).
pub fn chip_fill(fill: Color32, ink: Color32, hovered: bool, pressed: bool) -> Color32 {
    if pressed {
        fill.lerp_to_gamma(ink, 0.3)
    } else if hovered {
        fill.lerp_to_gamma(ink, 0.15)
    } else {
        fill
    }
}

/// Tastaturfokus: zeichnet 2 px in Akzentfarbe außen um ein Element, das gerade den Fokus hat.
/// Nach dem Element aufrufen, z. B. `theme::focus_ring(ui, &ui.text_edit_singleline(..))`.
pub fn focus_ring(ui: &egui::Ui, response: &egui::Response) {
    if !response.has_focus() {
        return;
    }
    let dark = ui.visuals().dark_mode;
    let radius = ui.visuals().widgets.inactive.corner_radius;
    ui.painter().rect_stroke(
        response.rect.expand(1.0),
        radius,
        Stroke::new(2.0, accent(dark)),
        egui::StrokeKind::Outside,
    );
}

/// Menüeintrag oder randloser Knopf (gesperrt, wenn nicht `enabled`) mit sichtbarem
/// Tastaturfokus: ohne Rand zeigt sich der Fokus nur als [`focus_ring`].
pub fn entry(ui: &mut egui::Ui, enabled: bool, widget: impl egui::Widget) -> egui::Response {
    let response = ui.add_enabled(enabled, widget);
    focus_ring(ui, &response);
    response
}

/// Schwebendes Feld an fester Stelle `pos`: `Order::Middle` (Dialoge aus `Dialog` liegen
/// darüber), nicht verschiebbar, ohne Einblenden. Das Feld nimmt Zeigereingaben an, damit Klicks,
/// Ziehen und Mausrad auf ihm den 3D-Bereich darunter nicht erreichen.
pub fn field_area(id: egui::Id, pos: egui::Pos2) -> egui::Area {
    egui::Area::new(id)
        .order(egui::Order::Middle)
        .movable(false)
        .fixed_pos(pos)
        .fade_in(false)
}

/// Abschnittskopf über die ganze Breite: Fläche aus [`section_frame`], Titel als Überschrift in
/// `text_primary`. `height` ist [`HEADER_H`] (Kopf eines Bereichs) oder [`SUB_HEADER_H`].
pub fn section_header(ui: &mut egui::Ui, title: &str, height: f32) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let frame = section_frame(dark);
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), egui::Sense::hover());
    // Selbst gezeichnet: die Überschrift für Bildschirmleser als Beschriftung melden.
    let enabled = ui.is_enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, title));
    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, frame.corner_radius, frame.fill);
        let galley = ui.painter().layout_no_wrap(
            title.to_owned(),
            heading_font(),
            palette(dark).text_primary,
        );
        let pos = egui::pos2(
            rect.left() + f32::from(frame.inner_margin.left),
            rect.center().y - galley.size().y / 2.0,
        );
        ui.painter()
            .with_clip_rect(rect.intersect(ui.clip_rect()))
            .galley(pos, galley, palette(dark).text_primary);
    }
    response
}

/// Stil der Menüeinträge: ohne Rand und Fläche in Ruhe, `bg_field` beim Überfahren (egui-Menüstil),
/// aber mit Graphit-Innenabstand 8/4 statt 2/0.
pub fn menu_style(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.button_padding = vec2(SPACE_2, SPACE_1);
}

/// Stil für Symbol- und Leistenknöpfe (Tab-Leiste, Werkzeugleisten): wie [`menu_style`] ohne Rand
/// in Ruhe und beim Überfahren; Abstände bleiben.
pub fn frameless(style: &mut egui::Style) {
    let w = &mut style.visuals.widgets;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = Stroke::NONE;
    w.hovered.bg_stroke = Stroke::NONE;
    w.active.bg_stroke = Stroke::NONE;
    w.open.bg_stroke = Stroke::NONE;
}

/// Wert (Zahl, Name, Ergebnis) im Textstil `value` und in `text_primary`.
pub fn value(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).font(value_font()).strong()
}

/// Einzeiliges Textfeld im Graphit-Stil: mindestens 26 px hoch, Text senkrecht mittig, sonst
/// wie [`field`].
pub fn text_field(ui: &mut egui::Ui, edit: egui::TextEdit<'_>) -> egui::Response {
    text_field_enabled(ui, true, edit)
}

/// Wie [`text_field`], gesperrt (nur lesbar, blass), wenn `enabled` falsch ist.
pub fn text_field_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    edit: egui::TextEdit<'_>,
) -> egui::Response {
    field_enabled(
        ui,
        enabled,
        edit.min_size(vec2(0.0, FIELD_H))
            .vertical_align(egui::Align::Center),
    )
}

/// Feld im Graphit-Stil: Zahlenfelder (`DragValue`) 26 px hoch mit mittigem Wert, vertiefte
/// Fläche `bg_inset`, 2-px-Fokusring in Akzentfarbe. Textfelder über [`text_field`], das die
/// Höhe von 26 px am Feld selbst setzt (egui misst Textfelder nicht an `interact_size`).
pub fn field(ui: &mut egui::Ui, widget: impl egui::Widget) -> egui::Response {
    field_enabled(ui, true, widget)
}

fn field_enabled(ui: &mut egui::Ui, enabled: bool, widget: impl egui::Widget) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let inset = palette(dark).bg_inset;
    // Stil nur für dieses Feld ändern, ohne eigenen Bereich (der bräche in umbrechenden Zeilen
    // nicht um).
    let saved = ui.style().clone();
    let s = ui.style_mut();
    s.spacing.interact_size.y = FIELD_H;
    s.visuals.widgets.inactive.weak_bg_fill = inset;
    s.visuals.extreme_bg_color = inset;
    let response = ui.add_enabled(enabled, widget);
    ui.set_style(saved);
    focus_ring(ui, &response);
    response
}

/// Schalter (Umschalter) statt Kästchen: Spur `bg_field`, bei „an“ Akzent mit weißem Knopf.
pub struct Toggle<'a> {
    on: &'a mut bool,
    label: egui::WidgetText,
}

impl<'a> Toggle<'a> {
    /// Schalter für `on` mit Beschriftung rechts daneben.
    pub fn new(on: &'a mut bool, label: impl Into<egui::WidgetText>) -> Self {
        Toggle {
            on,
            label: label.into(),
        }
    }
}

/// Breite und Höhe der Schalterspur.
const TOGGLE_TRACK: egui::Vec2 = vec2(28.0, 16.0);

impl egui::Widget for Toggle<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let Toggle { on, label } = self;
        let gap = ui.spacing().item_spacing.x;
        let galley = label.into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Button,
        );
        let size = vec2(
            TOGGLE_TRACK.x + gap + galley.size().x,
            galley.size().y.max(ui.spacing().interact_size.y),
        );
        let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Checkbox,
                ui.is_enabled(),
                *on,
                galley.text(),
            )
        });
        if ui.is_rect_visible(rect) {
            let p = palette(ui.visuals().dark_mode);
            let visuals = ui.style().interact(&response);
            let track = egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.center().y - TOGGLE_TRACK.y / 2.0),
                TOGGLE_TRACK,
            );
            let radius = TOGGLE_TRACK.y / 2.0;
            let (fill, stroke, knob, knob_x) = if *on {
                (p.accent, Stroke::NONE, p.on_accent, track.right() - radius)
            } else {
                (
                    p.bg_field,
                    Stroke::new(1.0, p.border_subtle),
                    visuals.text_color(),
                    track.left() + radius,
                )
            };
            let painter = ui.painter();
            painter.rect(track, radius, fill, stroke, egui::StrokeKind::Inside);
            painter.circle_filled(egui::pos2(knob_x, track.center().y), radius - 3.0, knob);
            let text_pos = egui::pos2(track.right() + gap, rect.center().y - galley.size().y / 2.0);
            painter.galley(text_pos, galley, visuals.text_color());
        }
        focus_ring(ui, &response);
        response
    }
}

/// Akzentfarbe (Modus eingeschaltet, Tastaturfokus, aktive Auswahl); in beiden Themen gleich.
pub fn accent(dark: bool) -> Color32 {
    palette(dark).accent
}

/// Papier einer Zeichnung: auch im dunklen Thema weiß.
pub const PAPER: Color32 = Color32::WHITE;
/// Linien der Ansichten auf dem Papier.
pub const SHEET_INK: Color32 = Color32::from_rgb(0x14, 0x16, 0x1a);
/// Blattkante.
pub const SHEET_FRAME: Color32 = Color32::from_rgb(0x50, 0x55, 0x60);
/// Rahmen und Text einer Ansicht, die noch berechnet wird.
pub const SHEET_PENDING: Color32 = Color32::from_rgb(0x8a, 0x90, 0x9c);
/// Maße und Mittellinien auf dem Papier: schwarz wie die Linien der Ansichten, auch im dunklen
/// Thema.
pub const SHEET_DIM: Color32 = SHEET_INK;
/// Maße, die ihren Bezug verloren haben (rot auf dem weißen Papier).
pub const SHEET_LOST: Color32 = Color32::from_rgb(0xd0, 0x1c, 0x1c);
/// Gewähltes Maß auf dem Papier: ein kräftiges Blau, das auf Weiß gut lesbar ist (das Papier ist
/// in beiden Themen weiß).
pub const SHEET_SELECTED: Color32 = Color32::from_rgb(0x1f, 0x6f, 0xe0);
/// Leerer Fortschrittsbalken einer Ansicht auf dem Papier.
pub const SHEET_PROGRESS_TRACK: Color32 = Color32::from_gray(225);
/// Schatten unter dem Papier.
pub const PAPER_SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 70);

/// Umfeld des Blatts im Zeichnungsbereich: ein ruhiges Neutral, von dem sich das weiße Papier
/// abhebt.
pub fn drawing_surround(dark: bool) -> Color32 {
    let p = palette(dark);
    if dark { p.bg_section } else { p.bg_active }
}

/// Hintergrundfarbe des 3D-Viewports.
pub fn viewport_background(dark: bool) -> Color32 {
    palette(dark).bg_viewport
}

/// Farben der Skizzen-Ansicht (Entwurf und fertige Skizzen in der 3D-Ansicht).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SketchColors {
    /// Frei bewegliche Geometrie.
    pub free: Color32,
    /// Voll bestimmte Geometrie.
    pub determined: Color32,
    /// Gewählte Geometrie.
    pub selected: Color32,
    /// Hilfsgeometrie.
    pub construction: Color32,
    /// Kurven fertiger Skizzen in der 3D-Ansicht.
    pub finished: Color32,
    /// Widersprüchliche oder fehlerhafte Geometrie.
    pub error: Color32,
    /// Maßtext.
    pub text: Color32,
    /// Fläche hinter dem Maßtext (halbtransparent).
    pub label_bg: Color32,
    /// Vorschau beim Zeichnen.
    pub preview: Color32,
    /// X-Achse der Skizze.
    pub axis_x: Color32,
    /// Y-Achse der Skizze.
    pub axis_y: Color32,
}

/// Skizzenfarben des Themas. Linien erreichen auf dem Viewport mindestens 3 : 1. Dunkel: Auswahl,
/// Achsen und Fehler stammen aus der Palette. Hell: das Auswahlgelb der Palette (#d9a400) ist auf
/// dem hellen Viewport zu schwach und wird für Linien zu einem dunkleren Bernstein.
pub fn sketch_colors(dark: bool) -> SketchColors {
    let p = palette(dark);
    if dark {
        SketchColors {
            free: rgb(0x4c9aff),
            determined: p.ok,
            selected: p.selection,
            construction: p.text_secondary,
            finished: rgb(0x7fb4ff),
            error: p.error,
            text: p.text_primary,
            label_bg: with_alpha(p.bg_panel, 0xd8),
            preview: rgb(0xa8ccff),
            axis_x: p.axis_x,
            axis_y: p.axis_y,
        }
    } else {
        SketchColors {
            free: p.accent,
            determined: p.ok,
            selected: rgb(0xa86a00),
            construction: p.text_tertiary,
            finished: p.chip_split.1,
            error: p.error,
            text: p.text_primary,
            label_bg: with_alpha(p.bg_panel, 0xe0),
            preview: p.axis_z,
            axis_x: p.axis_x,
            axis_y: p.axis_y,
        }
    }
}

/// Farben des Navigationswürfels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubeColors {
    /// Flächen, leicht in der Achsenfarbe getönt: `[X (Rechts/Links), Y (Vorne/Hinten),
    /// Z (Oben/Unten)]`.
    pub face: [Color32; 3],
    /// Kantenstreifen (neutral, dunkler als die Flächen).
    pub edge: Color32,
    /// Eckendreiecke (neutral, am dunkelsten).
    pub corner: Color32,
    /// Umrisslinien aller Bereiche.
    pub line: Color32,
    /// Bereich der aktuellen Ansicht.
    pub highlight: Color32,
    /// Bereich der aktuellen Ansicht unter dem Mauszeiger.
    pub highlight_hover: Color32,
    /// Beschriftung auf den Flächen und Name der Ansicht.
    pub text: Color32,
    /// Beschriftung auf der hervorgehobenen Fläche.
    pub text_on_highlight: Color32,
    /// Home-Knopf in Ruhe.
    pub home_fill: Color32,
    /// Home-Knopf unter dem Mauszeiger.
    pub home_hover: Color32,
}

/// Mischt `a` und `b` im sRGB-Raum: `t` = 0 ergibt `a`, `t` = 1 ergibt `b` (deckend).
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Dunkelt `c` um den Anteil `t` ab (alle Kanäle skaliert).
fn darken(c: Color32, t: f32) -> Color32 {
    let m = |x: u8| (f32::from(x) * (1.0 - t)).round() as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

/// Farben des Navigationswürfels: Flächen = `bg_field` mit 28 % Achsenfarbe, Kanten
/// `bg_section`, Ecken `bg_inset`, aktuelle Ansicht in der Akzentfarbe. Hell genauso; die
/// Kontraste prüft der Test `colours_in_both_themes` (tests/view_cube.rs).
pub fn cube_colors(dark: bool) -> CubeColors {
    let p = palette(dark);
    let tint = |axis: Color32| mix(p.bg_field, axis, 0.28);
    CubeColors {
        face: [tint(p.axis_x), tint(p.axis_y), tint(p.axis_z)],
        edge: p.bg_section,
        corner: p.bg_inset,
        line: p.text_tertiary,
        highlight: p.accent,
        // Etwas dunkler als der Akzent: Weiß darauf bleibt über 4,5 : 1 (nur beim Überfahren).
        highlight_hover: darken(p.accent, 0.12),
        text: p.text_primary,
        text_on_highlight: p.on_accent,
        home_fill: p.bg_field,
        home_hover: p.bg_active,
    }
}

/// Füllung eines Würfelbereichs unter dem Mauszeiger: heller als `fill`, die Beschriftung
/// (`CubeColors::text`) bleibt lesbar (dunkel 15 % zu Weiß, hell 45 % zu Weiß).
pub fn cube_hover_fill(dark: bool, fill: Color32) -> Color32 {
    let white = palette(false).bg_panel;
    mix(fill, white, if dark { 0.15 } else { 0.45 })
}

/// Dieselbe Farbe mit anderem Alpha (nicht vormultipliziert).
fn with_alpha(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

/// Heller (für die Hervorhebung unter dem Mauszeiger).
pub fn lighten(c: Color32) -> Color32 {
    let mix = |v: u8| v.saturating_add(((255 - u16::from(v)) * 2 / 5) as u8);
    Color32::from_rgba_unmultiplied(mix(c.r()), mix(c.g()), mix(c.b()), c.a())
}
