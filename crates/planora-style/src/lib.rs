//! Gemeinsame Farben und Schriften (Graphit) für die App und den Installer. Dunkel nach dem
//! Designsystem Graphit (`graphit-tokens.json`), Hell als eigener Entwurf mit denselben Regeln
//! (grau zuerst, Farbe nur mit Bedeutung, Rand statt Schatten).

use egui::Color32;

pub mod fonts;

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
    /// Leicht getönte Fläche der Auswahl (gewählte Karte): etwa 10 % `accent` über `bg_section`.
    pub accent_subtle: Color32,
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
    /// Markierung (Marker der Verlaufsleiste).
    pub marker: Color32,
}

pub const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const DARK: Palette = Palette {
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
    accent_subtle: rgb(0x172231),
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

pub const LIGHT: Palette = Palette {
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
    accent_subtle: rgb(0xdce6f7),
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
