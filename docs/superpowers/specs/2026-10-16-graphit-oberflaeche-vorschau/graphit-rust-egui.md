# Rust / egui

Use this section when the app's UI is built with **egui / eframe** (the most common choice for a Rust CAD tool with a wgpu viewport). Token names in comments match `tokens.json`. Written against egui 0.31+; on older versions `corner_radius` is called `rounding` and `Margin`/`CornerRadius` take `f32`.

## 1. Fonts

Download Montserrat (Regular, Medium, SemiBold) from Google Fonts (OFL licence) into `assets/fonts/` and embed them.

```rust
use std::sync::Arc;
use egui::{FontData, FontDefinitions, FontFamily};

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("Montserrat-Regular",  &include_bytes!("../assets/fonts/Montserrat-Regular.ttf")[..]),
        ("Montserrat-Medium",   &include_bytes!("../assets/fonts/Montserrat-Medium.ttf")[..]),
        ("Montserrat-SemiBold", &include_bytes!("../assets/fonts/Montserrat-SemiBold.ttf")[..]),
    ] {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "Montserrat-Regular".into());
    fonts.families.insert(FontFamily::Name("medium".into()),   vec!["Montserrat-Medium".into()]);
    fonts.families.insert(FontFamily::Name("semibold".into()), vec!["Montserrat-SemiBold".into()]);
    // Icons: egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Light);
    ctx.set_fonts(fonts);
}
```

## 2. Theme module (`theme.rs`)

```rust
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Visuals, vec2};

// colour tokens
pub const BG_VIEWPORT: Color32   = Color32::from_rgb(0x0e, 0x0e, 0x0e);
pub const BG_PANEL: Color32      = Color32::from_rgb(0x18, 0x18, 0x18);
pub const BG_SECTION: Color32    = Color32::from_rgb(0x1a, 0x1a, 0x1a);
pub const BG_FIELD: Color32      = Color32::from_rgb(0x26, 0x26, 0x26);
pub const BG_INSET: Color32      = Color32::from_rgb(0x0d, 0x0d, 0x0d);
pub const BG_ACTIVE: Color32     = Color32::from_rgb(0x33, 0x33, 0x33);
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(0x38, 0x38, 0x38);
pub const VIEWPORT_GRID: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
pub const GROUND_AXIS: Color32   = Color32::from_rgb(0x38, 0x73, 0x3b);
pub const TEXT_PRIMARY: Color32  = Color32::from_rgb(0xff, 0xff, 0xff);
pub const TEXT_SECONDARY: Color32= Color32::from_rgb(0x93, 0x93, 0x93);
pub const TEXT_TERTIARY: Color32 = Color32::from_rgb(0x7d, 0x7d, 0x7d);
pub const TEXT_MUTED: Color32    = Color32::from_rgb(0x73, 0x73, 0x73);
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x8e);
pub const ACCENT: Color32        = Color32::from_rgb(0x00, 0x6d, 0xfd);
pub const ON_ACCENT: Color32     = Color32::WHITE;
pub const CHIP_BRIDGE: (Color32, Color32)  = (Color32::from_rgb(0x25,0x1f,0x0c), Color32::from_rgb(0xe2,0xab,0x63));
pub const CHIP_SPLIT: (Color32, Color32)   = (Color32::from_rgb(0x0a,0x1d,0x25), Color32::from_rgb(0x1f,0x6d,0xcc));
pub const CHIP_BOOLEAN: (Color32, Color32) = (Color32::from_rgb(0x0f,0x26,0x0c), Color32::from_rgb(0x6e,0xcd,0x62));
pub const BADGE_PRO: (Color32, Color32, Color32) = (Color32::from_rgb(0x30,0x2b,0x00), Color32::from_rgb(0xe6,0xf0,0x98), Color32::from_rgb(0xe9,0xe0,0x73));
pub const SELECTION: Color32 = Color32::from_rgb(0xf1, 0xf0, 0x5c);
pub const AXIS_X: Color32 = Color32::from_rgb(0xe0, 0x30, 0x3a);
pub const AXIS_Y: Color32 = Color32::from_rgb(0x64, 0xbe, 0x62);
pub const AXIS_Z: Color32 = Color32::from_rgb(0x3d, 0x8b, 0xdb);

// spacing / radius / size tokens
pub const SPACE_1: f32 = 4.0;  pub const SPACE_2: f32 = 8.0;  pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0; pub const SPACE_6: f32 = 24.0;
pub const RADIUS_XS: u8 = 4; pub const RADIUS_SM: u8 = 6; pub const RADIUS_MD: u8 = 10; pub const RADIUS_LG: u8 = 14;
pub const PANEL_WIDTH: f32 = 250.0; pub const ROW_H: f32 = 24.0; pub const FIELD_H: f32 = 26.0;
pub const CONTROL: f32 = 32.0; pub const HEADER_H: f32 = 40.0; pub const TOOLBAR_H: f32 = 48.0; pub const DOCK_H: f32 = 56.0;

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.override_text_color = None;
    v.panel_fill = BG_VIEWPORT;
    v.window_fill = BG_PANEL;
    v.window_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    v.window_corner_radius = CornerRadius::same(RADIUS_LG);
    v.menu_corner_radius = CornerRadius::same(RADIUS_LG);
    v.window_shadow = Shadow::NONE;          // outline, not shadow
    v.popup_shadow = Shadow::NONE;
    v.extreme_bg_color = BG_INSET;           // TextEdit / DragValue fill
    v.faint_bg_color = BG_SECTION;
    v.selection.bg_fill = BG_ACTIVE;         // neutral selection, NOT blue
    v.selection.stroke = Stroke::new(1.0, TEXT_PRIMARY);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BG_PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);
    w.inactive.bg_fill = Color32::TRANSPARENT;   w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = Stroke::NONE;         w.inactive.fg_stroke = Stroke::new(1.5, TEXT_SECONDARY);
    w.hovered.bg_fill = BG_FIELD;                w.hovered.weak_bg_fill = BG_FIELD;
    w.hovered.bg_stroke = Stroke::NONE;          w.hovered.fg_stroke = Stroke::new(1.5, TEXT_PRIMARY);
    w.active.bg_fill = BG_ACTIVE;                w.active.weak_bg_fill = BG_ACTIVE;
    w.active.bg_stroke = Stroke::NONE;           w.active.fg_stroke = Stroke::new(1.5, TEXT_PRIMARY);
    w.open = w.active;
    for s in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        s.corner_radius = CornerRadius::same(RADIUS_SM);
        s.expansion = 0.0;
    }
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(SPACE_2, SPACE_1);
        s.spacing.button_padding = vec2(SPACE_2, SPACE_1);
        s.spacing.interact_size = vec2(CONTROL, ROW_H);
        s.spacing.window_margin = Margin::same(SPACE_3 as i8);
        s.spacing.menu_margin = Margin::same(SPACE_4 as i8);
        s.text_styles = [
            (TextStyle::Heading,   FontId::new(13.0, FontFamily::Name("semibold".into()))),
            (TextStyle::Body,      FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Button,    FontId::new(12.0, FontFamily::Name("medium".into()))),
            (TextStyle::Small,     FontId::new(10.0, FontFamily::Name("semibold".into()))),
            (TextStyle::Monospace, FontId::new(11.0, FontFamily::Name("medium".into()))),
            (TextStyle::Name("label".into()), FontId::new(13.0, FontFamily::Name("medium".into()))),
            (TextStyle::Name("value".into()), FontId::new(11.0, FontFamily::Name("medium".into()))),
            (TextStyle::Name("axis".into()),  FontId::new(15.0, FontFamily::Name("medium".into()))),
        ].into();
    });
}

/// The floating-panel frame used by every panel, toolbar and menu.
pub fn panel_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(BG_PANEL)
        .stroke(Stroke::new(1.0, BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(RADIUS_LG))
        .inner_margin(Margin::same(SPACE_3 as i8))
}
```

## 3. Layout recipe

The 3D view is the whole window; panels are `egui::Area`s anchored to the edges, drawn on top.

```rust
fn ui(&mut self, ctx: &egui::Context) {
    // full-window viewport (render the model with an egui_wgpu::Callback inside)
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(theme::BG_VIEWPORT))
        .show(ctx, |ui| self.viewport.show(ui));

    use egui::{Align2, Area, Id, vec2};
    let inset = theme::SPACE_6;
    Area::new(Id::new("outliner")).anchor(Align2::LEFT_TOP, vec2(inset, inset))
        .show(ctx, |ui| theme::panel_frame().show(ui, |ui| { ui.set_width(theme::PANEL_WIDTH - 2.0 * theme::SPACE_3); self.outliner(ui) }));
    Area::new(Id::new("topbar")).anchor(Align2::CENTER_TOP, vec2(0.0, inset))
        .show(ctx, |ui| theme::panel_frame().inner_margin(egui::Margin::same(8)).show(ui, |ui| self.top_bar(ui)));
    Area::new(Id::new("modes")).anchor(Align2::RIGHT_TOP, vec2(-inset, inset))
        .show(ctx, |ui| theme::panel_frame().inner_margin(egui::Margin::same(8)).show(ui, |ui| self.mode_switcher(ui)));
    Area::new(Id::new("properties")).anchor(Align2::RIGHT_TOP, vec2(-inset, inset + theme::TOOLBAR_H + theme::SPACE_3))
        .show(ctx, |ui| theme::panel_frame().show(ui, |ui| { ui.set_width(theme::PANEL_WIDTH - 2.0 * theme::SPACE_3); self.properties(ui) }));
    Area::new(Id::new("dock")).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -inset))
        .show(ctx, |ui| self.command_bar_and_dock(ui));
    Area::new(Id::new("readout")).anchor(Align2::LEFT_BOTTOM, vec2(inset, -inset))
        .show(ctx, |ui| self.coords_and_snaps(ui));
}
```

## 4. Rules for the implementation

- Do not use `SidePanel` / `TopBottomPanel` for the panels: they dock edge-to-edge and break the floating look. Use `Area` + `panel_frame()`.
- Paint the floor grid yourself in the viewport: 1px lines in `VIEWPORT_GRID`, the X ground line in `GROUND_AXIS`, fading out with distance.
- Icon buttons: `ui.add(egui::Button::new(icon).min_size(vec2(CONTROL, CONTROL)).selected(is_active))` — `selected` gives the `BG_ACTIVE` fill.
- Numeric fields: `egui::DragValue` wrapped in a `Frame` with `BG_INSET` fill, `RADIUS_SM`, height `FIELD_H`, value centred, unit suffix " cm".
- Operation chips, Kbd badges and the Pro badge are plain `Frame`s with a `RichText` label — no hover state.
- Keyboard focus: draw a 2px `ACCENT` stroke around the focused widget (`response.has_focus()`).

If the app uses **iced** or **Slint** instead, the tokens stay the same: map them into an iced `Theme`/`Palette` plus custom `container::Style`s, or into a Slint `global Theme { … }` with one property per token.
