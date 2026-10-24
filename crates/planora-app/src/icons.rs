use crate::state::BodyKind;
use crate::{fonts, theme};
use egui::{Color32, Painter, Pos2, Rect, Stroke, pos2, vec2};

// ---------------------------------------------------------------------------------------------
// Symbole aus der Symbolschrift (Phosphor Light, MIT)
// ---------------------------------------------------------------------------------------------

/// Ein Symbol je Menüpunkt und Leistenschalter. Die Zuordnung zu Phosphor-Namen steht in
/// `TABLE`; die Codepunkte stammen aus `assets/fonts/Phosphor-Light-style.css`, und der Test
/// `glyphs_match_the_phosphor_stylesheet` prüft sie dagegen. Aktionen ohne passendes
/// Phosphor-Symbol stehen unter `HandIcon` bzw. bei den gezeichneten Funktionen weiter unten.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Icon {
    /// Phosphor `file-plus`.
    New,
    /// Phosphor `floppy-disk`.
    Save,
    /// Phosphor `floppy-disk-back`.
    SaveAs,
    /// Phosphor `x`.
    Close,
    /// Phosphor `export`.
    Export,
    /// Phosphor `download-simple`.
    Import,
    /// Phosphor `sign-out`.
    Quit,
    /// Phosphor `info`.
    About,
    /// Phosphor `cube`.
    Box,
    /// Phosphor `cylinder`.
    Cylinder,
    /// Phosphor `sphere`.
    Sphere,
    /// Phosphor `pencil-simple-line`.
    SketchOnPlane,
    /// Phosphor `bounding-box`.
    SketchOnFace,
    /// Phosphor `arrow-line-up`.
    Extrude,
    /// Phosphor `arrow-arc-right`.
    Revolve,
    /// Phosphor `union`.
    Union,
    /// Phosphor `subtract`.
    Subtract,
    /// Phosphor `intersect`.
    Intersect,
    /// Phosphor `arrows-out-cardinal`.
    Move,
    /// Phosphor `skip-forward`.
    ToTimelineEnd,
    /// Phosphor `cube-focus`.
    ViewFace,
    /// Phosphor `cube-transparent`.
    ViewIso,
    /// Phosphor `corners-out`.
    FitAll,
    /// Phosphor `moon`.
    ThemeDark,
    /// Phosphor `sun`.
    ThemeLight,
    /// Phosphor `clock-counter-clockwise`.
    PanelVersions,
    /// Phosphor `eye`.
    Visible,
    /// Phosphor `eye-slash`.
    Hidden,
    /// Phosphor `pencil-ruler`.
    NewDrawing,
    /// Phosphor `file-text`.
    OpenDrawing,
    /// Phosphor `arrow-u-up-left`.
    ToModel,
    /// Phosphor `plus-square`.
    AddView,
    /// Phosphor `magic-wand`.
    SuggestDimensions,
    /// Phosphor `ruler`.
    DimensionTool,
    /// Phosphor `frame-corners`.
    FitSheet,
    /// Phosphor `magnifying-glass-plus`.
    ZoomIn,
    /// Phosphor `magnifying-glass-minus`.
    ZoomOut,
    /// Phosphor `trash`.
    Delete,
    /// Phosphor `cursor`.
    ToolSelect,
    /// Phosphor `dot`.
    ToolPoint,
    /// Phosphor `line-segment`.
    ToolLine,
    /// Phosphor `rectangle`.
    ToolRectangle,
    /// Phosphor `circle`.
    ToolCircle,
    /// Phosphor `bezier-curve`.
    ToolSpline,
    /// Phosphor `copy-simple`.
    ToolProjectEdge,
    /// Phosphor `crosshair-simple`.
    Coincident,
    /// Phosphor `arrows-horizontal`.
    Horizontal,
    /// Phosphor `arrows-vertical`.
    Vertical,
    /// Phosphor `equals`.
    Equal,
    /// Phosphor `lock-simple`.
    Fixed,
    /// Phosphor `align-center-vertical-simple`.
    Symmetric,
    /// Phosphor `arrows-left-right`.
    Distance,
    /// Phosphor `angle`.
    Angle,
    /// Phosphor `circle-dashed`.
    ToggleConstruction,
    /// Phosphor `flip-horizontal`.
    Mirror,
    /// Phosphor `dots-nine`.
    Pattern,
    /// Phosphor `check-circle`.
    EndSketch,
    /// Phosphor `plus`.
    NewTab,
    /// Phosphor `house-simple`.
    Home,
    /// Phosphor `note-pencil`.
    SheetInfo,
    /// Phosphor `number-circle-one`.
    PositionTool,
    /// Phosphor `scissors`.
    SectionTool,
    /// Phosphor `arrow-counter-clockwise`.
    Undo,
    /// Phosphor `arrow-clockwise`.
    Redo,
    /// Phosphor `function`.
    Parameters,
    /// Phosphor `file-arrow-down` (STEP-Körper im Feature-Baum).
    StepBody,
    /// Phosphor `folder` (Ordner in der Dateiauswahl).
    Folder,
    /// Phosphor `file` (Datei in der Dateiauswahl).
    File,
    /// Phosphor `desktop` (Schnellzugriff Desktop).
    Desktop,
    /// Phosphor `image` (Schnellzugriff Bilder).
    Pictures,
    /// Phosphor `hard-drive` (Laufwerk).
    Drive,
    /// Phosphor `star` (Favorit).
    Favorite,
    /// Phosphor `files` (Schnellzugriff Dokumente).
    Documents,
    /// Phosphor `briefcase` (Schnellzugriff Projekte).
    Projects,
    /// Phosphor `caret-left` (eine Ebene hinauf im Pfad des Fensters „Projekte“).
    Back,
    /// Phosphor `caret-right` (Trenner im Pfad des Fensters „Projekte“).
    PathSeparator,
    /// Phosphor `gear-six` (Menüeintrag „Einstellungen …“).
    Settings,
    /// Phosphor `skip-back` (Verlaufsleiste: zum Anfang).
    HistoryStart,
    /// Phosphor `rewind` (Verlaufsleiste: ein Schritt zurück).
    HistoryBack,
    /// Phosphor `play` (Verlaufsleiste: abspielen).
    HistoryPlay,
    /// Phosphor `pause` (Verlaufsleiste: anhalten).
    HistoryPause,
    /// Phosphor `fast-forward` (Verlaufsleiste: ein Schritt vor).
    HistoryForward,
    /// Phosphor `warning` (Verlaufsleiste: Schritt mit Fehler).
    Warning,
}

/// Alle Einträge von [`Icon`].
pub const ALL: &[Icon] = &[
    Icon::New,
    Icon::Save,
    Icon::SaveAs,
    Icon::Close,
    Icon::Export,
    Icon::Import,
    Icon::Quit,
    Icon::About,
    Icon::Box,
    Icon::Cylinder,
    Icon::Sphere,
    Icon::SketchOnPlane,
    Icon::SketchOnFace,
    Icon::Extrude,
    Icon::Revolve,
    Icon::Union,
    Icon::Subtract,
    Icon::Intersect,
    Icon::Move,
    Icon::ToTimelineEnd,
    Icon::ViewFace,
    Icon::ViewIso,
    Icon::FitAll,
    Icon::ThemeDark,
    Icon::ThemeLight,
    Icon::PanelVersions,
    Icon::Visible,
    Icon::Hidden,
    Icon::NewDrawing,
    Icon::OpenDrawing,
    Icon::ToModel,
    Icon::AddView,
    Icon::SuggestDimensions,
    Icon::DimensionTool,
    Icon::FitSheet,
    Icon::ZoomIn,
    Icon::ZoomOut,
    Icon::Delete,
    Icon::ToolSelect,
    Icon::ToolPoint,
    Icon::ToolLine,
    Icon::ToolRectangle,
    Icon::ToolCircle,
    Icon::ToolSpline,
    Icon::ToolProjectEdge,
    Icon::Coincident,
    Icon::Horizontal,
    Icon::Vertical,
    Icon::Equal,
    Icon::Fixed,
    Icon::Symmetric,
    Icon::Distance,
    Icon::Angle,
    Icon::ToggleConstruction,
    Icon::Mirror,
    Icon::Pattern,
    Icon::EndSketch,
    Icon::NewTab,
    Icon::Home,
    Icon::SheetInfo,
    Icon::PositionTool,
    Icon::SectionTool,
    Icon::Undo,
    Icon::Redo,
    Icon::Parameters,
    Icon::StepBody,
    Icon::Folder,
    Icon::File,
    Icon::Desktop,
    Icon::Pictures,
    Icon::Drive,
    Icon::Favorite,
    Icon::Documents,
    Icon::Projects,
    Icon::Back,
    Icon::PathSeparator,
    Icon::Settings,
    Icon::HistoryStart,
    Icon::HistoryBack,
    Icon::HistoryPlay,
    Icon::HistoryPause,
    Icon::HistoryForward,
    Icon::Warning,
];

/// Symbol, Codepunkt in Phosphor-Light.ttf und Phosphor-Name (Quelle: Phosphor-Light-style.css).
const TABLE: &[(Icon, char, &str)] = &[
    (Icon::New, '\u{e236}', "file-plus"),
    (Icon::Save, '\u{e248}', "floppy-disk"),
    (Icon::SaveAs, '\u{eaf4}', "floppy-disk-back"),
    (Icon::Close, '\u{e4f6}', "x"),
    (Icon::Export, '\u{eaf0}', "export"),
    (Icon::Import, '\u{e20c}', "download-simple"),
    (Icon::Quit, '\u{e42a}', "sign-out"),
    (Icon::About, '\u{e2ce}', "info"),
    (Icon::Box, '\u{e1da}', "cube"),
    (Icon::Cylinder, '\u{e8fc}', "cylinder"),
    (Icon::Sphere, '\u{ee66}', "sphere"),
    (Icon::SketchOnPlane, '\u{ebc6}', "pencil-simple-line"),
    (Icon::SketchOnFace, '\u{e6ce}', "bounding-box"),
    (Icon::Extrude, '\u{e066}', "arrow-line-up"),
    (Icon::Revolve, '\u{e016}', "arrow-arc-right"),
    (Icon::Union, '\u{edbe}', "union"),
    (Icon::Subtract, '\u{ebd6}', "subtract"),
    (Icon::Intersect, '\u{e2d2}', "intersect"),
    (Icon::Move, '\u{e0a4}', "arrows-out-cardinal"),
    (Icon::ToTimelineEnd, '\u{e5a6}', "skip-forward"),
    (Icon::ViewFace, '\u{ed0a}', "cube-focus"),
    (Icon::ViewIso, '\u{ec7c}', "cube-transparent"),
    (Icon::FitAll, '\u{e1d0}', "corners-out"),
    (Icon::ThemeDark, '\u{e330}', "moon"),
    (Icon::ThemeLight, '\u{e472}', "sun"),
    (Icon::PanelVersions, '\u{e1a0}', "clock-counter-clockwise"),
    (Icon::Visible, '\u{e220}', "eye"),
    (Icon::Hidden, '\u{e224}', "eye-slash"),
    (Icon::NewDrawing, '\u{e906}', "pencil-ruler"),
    (Icon::OpenDrawing, '\u{e23a}', "file-text"),
    (Icon::ToModel, '\u{e08a}', "arrow-u-up-left"),
    (Icon::AddView, '\u{ed4a}', "plus-square"),
    (Icon::SuggestDimensions, '\u{e6b6}', "magic-wand"),
    (Icon::DimensionTool, '\u{e6b8}', "ruler"),
    (Icon::FitSheet, '\u{e626}', "frame-corners"),
    (Icon::ZoomIn, '\u{e310}', "magnifying-glass-plus"),
    (Icon::ZoomOut, '\u{e30e}', "magnifying-glass-minus"),
    (Icon::Delete, '\u{e4a6}', "trash"),
    (Icon::ToolSelect, '\u{e1dc}', "cursor"),
    (Icon::ToolPoint, '\u{ecde}', "dot"),
    (Icon::ToolLine, '\u{e6d2}', "line-segment"),
    (Icon::ToolRectangle, '\u{e3f0}', "rectangle"),
    (Icon::ToolCircle, '\u{e18a}', "circle"),
    (Icon::ToolSpline, '\u{eb00}', "bezier-curve"),
    (Icon::ToolProjectEdge, '\u{e1cc}', "copy-simple"),
    (Icon::Coincident, '\u{e1d8}', "crosshair-simple"),
    (Icon::Horizontal, '\u{eb06}', "arrows-horizontal"),
    (Icon::Vertical, '\u{eb04}', "arrows-vertical"),
    (Icon::Equal, '\u{e21c}', "equals"),
    (Icon::Fixed, '\u{e308}', "lock-simple"),
    (Icon::Symmetric, '\u{eb10}', "align-center-vertical-simple"),
    (Icon::Distance, '\u{e0a0}', "arrows-left-right"),
    (Icon::Angle, '\u{e7bc}', "angle"),
    (Icon::ToggleConstruction, '\u{e602}', "circle-dashed"),
    (Icon::Mirror, '\u{ed6a}', "flip-horizontal"),
    (Icon::Pattern, '\u{e1fc}', "dots-nine"),
    (Icon::EndSketch, '\u{e184}', "check-circle"),
    (Icon::NewTab, '\u{e3d4}', "plus"),
    (Icon::Home, '\u{e2c6}', "house-simple"),
    (Icon::SheetInfo, '\u{e34c}', "note-pencil"),
    (Icon::PositionTool, '\u{e36a}', "number-circle-one"),
    (Icon::SectionTool, '\u{eae0}', "scissors"),
    (Icon::Undo, '\u{e038}', "arrow-counter-clockwise"),
    (Icon::Redo, '\u{e036}', "arrow-clockwise"),
    (Icon::Parameters, '\u{ebe4}', "function"),
    (Icon::StepBody, '\u{e232}', "file-arrow-down"),
    (Icon::Folder, '\u{e24a}', "folder"),
    (Icon::File, '\u{e230}', "file"),
    (Icon::Desktop, '\u{e560}', "desktop"),
    (Icon::Pictures, '\u{e2ca}', "image"),
    (Icon::Drive, '\u{e29e}', "hard-drive"),
    (Icon::Favorite, '\u{e46a}', "star"),
    (Icon::Documents, '\u{e710}', "files"),
    (Icon::Projects, '\u{e0ee}', "briefcase"),
    (Icon::Back, '\u{e138}', "caret-left"),
    (Icon::PathSeparator, '\u{e13a}', "caret-right"),
    (Icon::Settings, '\u{e272}', "gear-six"),
    (Icon::HistoryStart, '\u{e5a4}', "skip-back"),
    (Icon::HistoryBack, '\u{e6a8}', "rewind"),
    (Icon::HistoryPlay, '\u{e3d0}', "play"),
    (Icon::HistoryPause, '\u{e39e}', "pause"),
    (Icon::HistoryForward, '\u{e6a6}', "fast-forward"),
    (Icon::Warning, '\u{e4e0}', "warning"),
];

fn entry(icon: Icon) -> &'static (Icon, char, &'static str) {
    TABLE
        .iter()
        .find(|(i, ..)| *i == icon)
        .expect("jedes Symbol steht in TABLE")
}

/// Codepunkt des Symbols in der Symbolschrift (`fonts::icons()`).
pub fn glyph(icon: Icon) -> char {
    entry(icon).1
}

/// Name des Symbols bei Phosphor (z. B. `file-plus`).
pub fn phosphor_name(icon: Icon) -> &'static str {
    entry(icon).2
}

/// Das Symbol als Text in der Symbolfamilie.
pub fn text(icon: Icon, size: f32, color: Color32) -> egui::RichText {
    egui::RichText::new(glyph(icon).to_string())
        .family(fonts::icons())
        .size(size)
        .color(color)
}

/// Größe des Symbols in Beschriftungen.
pub const LABEL_ICON_SIZE: f32 = 16.0;
/// Abstand zwischen Symbol und Text in Beschriftungen.
pub const LABEL_GAP: f32 = 8.0;

/// Symbol (16 px) + 8 px Abstand + Text, für Menüpunkte und Knöpfe. Das Symbol hat die Farbe
/// `Color32::PLACEHOLDER`: egui setzt dafür die Textfarbe des Knopfs ein, im Ruhezustand also
/// `text_secondary` des Themes (siehe `theme::apply`), beim Überfahren `text_primary`. So
/// stimmt die Farbe in beiden Themes, ohne dass der Aufrufer sie kennen muss.
pub fn label(icon: Icon, label: &str) -> egui::WidgetText {
    label_colored(icon, label, Color32::PLACEHOLDER)
}

/// Das Symbol als eigener Baustein eines Knopfs (16 px, Farbe folgt dem Knopftext). Anders als
/// [`label`] bleiben Symbol und Text getrennte Texte; der Abstand dazwischen ist der
/// Elementabstand (`item_spacing.x` = 8 px = [`LABEL_GAP`]).
pub fn atom(icon: Icon) -> egui::RichText {
    text(icon, LABEL_ICON_SIZE, Color32::PLACEHOLDER)
}

/// Knopf mit Symbol und Text (Menüeinträge, Leistenschalter).
pub fn button<'a>(icon: Icon, label: impl Into<egui::WidgetText>) -> egui::Button<'a> {
    egui::Button::new((atom(icon), label.into()))
}

/// Wählbarer Knopf mit Symbol und Text (Werkzeug, Tab, Schalter einer Leiste).
pub fn selectable<'a>(
    selected: bool,
    icon: Icon,
    label: impl Into<egui::WidgetText>,
) -> egui::Button<'a> {
    egui::Button::selectable(selected, (atom(icon), label.into()))
}

/// Menüeintrag mit Symbol beliebiger Herkunft: Schriftsymbole als Text, gezeichnete Symbole in
/// einem 16-px-Feld vor dem Text (gleiche Lage wie bei [`button`]).
pub fn source_entry(
    ui: &mut egui::Ui,
    source: IconSource,
    label: &str,
    enabled: bool,
) -> egui::Response {
    if let IconSource::Font(icon) = source {
        return theme::entry(ui, enabled, button(icon, label));
    }
    let id = egui::Id::new(("planora-icon", label));
    // Gesperrt zeichnet der Maler blass, auch das Symbol.
    ui.add_enabled(enabled, |ui: &mut egui::Ui| {
        let result = egui::Button::new((
            egui::Atom::custom(id, vec2(LABEL_ICON_SIZE, LABEL_ICON_SIZE)),
            label,
        ))
        .atom_ui(ui);
        if let Some(rect) = result.rect(id) {
            let active = result.response.hovered();
            paint_source(
                ui.painter(),
                rect,
                source,
                stroke_color(ui.visuals().dark_mode, active),
            );
        }
        theme::focus_ring(ui, &result.response);
        result.response
    })
}

/// Wie [`label`], mit fester Symbolfarbe (z. B. `theme::palette(dark).text_secondary`).
pub fn label_colored(icon: Icon, label: &str, color: Color32) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &glyph(icon).to_string(),
        0.0,
        egui::TextFormat::simple(egui::FontId::new(LABEL_ICON_SIZE, fonts::icons()), color),
    );
    job.append(
        label,
        LABEL_GAP,
        egui::TextFormat::simple(
            egui::FontId::new(12.0, fonts::medium()),
            Color32::PLACEHOLDER,
        ),
    );
    job.into()
}

// ---------------------------------------------------------------------------------------------
// Gezeichnete Symbole
// ---------------------------------------------------------------------------------------------

/// Strichbreite aller gezeichneten Symbole.
pub const STROKE_WIDTH: f32 = 1.5;

/// Strichfarbe der gezeichneten Symbole: `text_secondary` in Ruhe, `text_primary` wenn aktiv.
pub fn stroke_color(dark: bool, active: bool) -> Color32 {
    let p = theme::palette(dark);
    if active {
        p.text_primary
    } else {
        p.text_secondary
    }
}

/// Strich der gezeichneten Symbole in Themenfarbe.
pub fn stroke(dark: bool, active: bool) -> Stroke {
    Stroke::new(STROKE_WIDTH, stroke_color(dark, active))
}

/// Linie mit runden Enden (egui kennt keine Linienkappen: an beiden Enden ein kleiner Kreis).
fn seg(painter: &Painter, points: [Pos2; 2], stroke: Stroke) {
    painter.line_segment(points, stroke);
    for p in points {
        painter.circle_filled(p, stroke.width / 2.0, stroke.color);
    }
}

/// Zeichnet ein kleines Körpersymbol in `rect`.
pub fn paint_body_icon(painter: &Painter, rect: Rect, kind: BodyKind, color: Color32) {
    let stroke = Stroke::new(STROKE_WIDTH, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    match kind {
        BodyKind::Box => {
            let front =
                Rect::from_center_size(c + vec2(-r * 0.15, r * 0.15), vec2(r * 1.4, r * 1.4));
            let off = vec2(r * 0.4, -r * 0.4);
            painter.rect_stroke(front, 1.0, stroke, egui::StrokeKind::Inside);
            seg(painter, [front.left_top(), front.left_top() + off], stroke);
            seg(
                painter,
                [front.right_top(), front.right_top() + off],
                stroke,
            );
            seg(
                painter,
                [front.right_bottom(), front.right_bottom() + off],
                stroke,
            );
            seg(
                painter,
                [front.left_top() + off, front.right_top() + off],
                stroke,
            );
            seg(
                painter,
                [front.right_top() + off, front.right_bottom() + off],
                stroke,
            );
        }
        BodyKind::Cylinder | BodyKind::Extrusion => {
            let top = c + vec2(0.0, -r * 0.7);
            let bottom = c + vec2(0.0, r * 0.7);
            let rx = r * 0.8;
            let ry = r * 0.3;
            painter.add(egui::Shape::ellipse_stroke(top, vec2(rx, ry), stroke));
            painter.add(egui::Shape::ellipse_stroke(bottom, vec2(rx, ry), stroke));
            seg(
                painter,
                [pos2(c.x - rx, top.y), pos2(c.x - rx, bottom.y)],
                stroke,
            );
            seg(
                painter,
                [pos2(c.x + rx, top.y), pos2(c.x + rx, bottom.y)],
                stroke,
            );
        }
        BodyKind::Sphere | BodyKind::Revolution => {
            painter.circle_stroke(c, r, stroke);
            painter.add(egui::Shape::ellipse_stroke(c, vec2(r, r * 0.35), stroke));
        }
        BodyKind::Cone => {
            let apex: Pos2 = c + vec2(0.0, -r);
            let base_y = c.y + r * 0.7;
            let rx = r * 0.85;
            seg(painter, [apex, pos2(c.x - rx, base_y)], stroke);
            seg(painter, [apex, pos2(c.x + rx, base_y)], stroke);
            painter.add(egui::Shape::ellipse_stroke(
                pos2(c.x, base_y),
                vec2(rx, r * 0.28),
                stroke,
            ));
        }
        BodyKind::Combined => {
            // zwei überlappende Quadrate: aus zwei Körpern entstanden
            let side = vec2(r * 1.2, r * 1.2);
            let back = Rect::from_center_size(c + vec2(r * 0.3, -r * 0.3), side);
            let front = Rect::from_center_size(c + vec2(-r * 0.3, r * 0.3), side);
            painter.rect_stroke(back, 1.0, stroke, egui::StrokeKind::Inside);
            painter.rect_stroke(front, 1.0, stroke, egui::StrokeKind::Inside);
        }
        BodyKind::Mesh => {
            // Dreiecksnetz: ein großes Dreieck mit eingeschriebenem Dreieck
            let a = c + vec2(0.0, -r);
            let b = c + vec2(-r, r * 0.8);
            let d = c + vec2(r, r * 0.8);
            painter.add(egui::Shape::closed_line(vec![a, b, d], stroke));
            painter.add(egui::Shape::closed_line(
                vec![a.lerp(b, 0.5), b.lerp(d, 0.5), d.lerp(a, 0.5)],
                stroke,
            ));
        }
        BodyKind::Step => {
            // Aus der Symbolschrift (Phosphor `file-arrow-down`), in der Größe des Feldes.
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                glyph(Icon::StepBody),
                egui::FontId::new(rect.height().min(rect.width()), fonts::icons()),
                color,
            );
        }
    }
}

/// Symbol einer Zeile im Feature-Baum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureIcon {
    Sketch,
    Body(BodyKind),
    Move,
    Blend,
    Chamfer,
}

impl FeatureIcon {
    /// Symbol für Verrunden (`chamfer == false`) oder Fase (Menü, Befehlsfeld, Baumzeile).
    pub fn for_blend(chamfer: bool) -> FeatureIcon {
        if chamfer {
            FeatureIcon::Chamfer
        } else {
            FeatureIcon::Blend
        }
    }

    /// Symbol zur Art eines Features.
    pub fn of(kind: &planora_feature::FeatureKind) -> FeatureIcon {
        use planora_feature::FeatureKind as K;
        match kind {
            K::Sketch(_) => FeatureIcon::Sketch,
            K::Box { .. } => FeatureIcon::Body(BodyKind::Box),
            K::Cylinder { .. } => FeatureIcon::Body(BodyKind::Cylinder),
            K::Sphere { .. } => FeatureIcon::Body(BodyKind::Sphere),
            K::Cone { .. } => FeatureIcon::Body(BodyKind::Cone),
            K::Extrude { .. } => FeatureIcon::Body(BodyKind::Extrusion),
            K::Revolve { .. } => FeatureIcon::Body(BodyKind::Revolution),
            K::Boolean { .. } => FeatureIcon::Body(BodyKind::Combined),
            K::Move { .. } => FeatureIcon::Move,
            K::Verrunden { art, .. } => FeatureIcon::for_blend(art.is_chamfer()),
            K::MeshBody { .. } => FeatureIcon::Body(BodyKind::Mesh),
            K::StepBody { .. } => FeatureIcon::Body(BodyKind::Step),
        }
    }
}

/// Einträge Verrunden und Fase für Menü und Befehlsfeld: Beschriftung, Symbol, `chamfer`.
pub fn blend_entries() -> [(&'static str, IconSource, bool); 2] {
    [("Verrunden …", false), ("Fase …", true)].map(|(label, chamfer)| {
        (
            label,
            IconSource::Feature(FeatureIcon::for_blend(chamfer)),
            chamfer,
        )
    })
}

/// Zeichnet das Symbol eines Features in `rect`.
pub fn paint_feature_icon(painter: &Painter, rect: Rect, icon: FeatureIcon, color: Color32) {
    match icon {
        FeatureIcon::Sketch => paint_sketch_icon(painter, rect, color),
        FeatureIcon::Body(kind) => paint_body_icon(painter, rect, kind, color),
        FeatureIcon::Move => paint_move_icon(painter, rect, color),
        FeatureIcon::Blend => paint_blend_icon(painter, rect, color),
        FeatureIcon::Chamfer => paint_chamfer_icon(painter, rect, color),
    }
}

/// Verschieben: ein Pfeil nach rechts oben mit Spitze.
fn paint_move_icon(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(STROKE_WIDTH, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let from = c + vec2(-r, r);
    let to = c + vec2(r, -r);
    seg(painter, [from, to], stroke);
    seg(painter, [to, to + vec2(-r * 0.7, 0.0)], stroke);
    seg(painter, [to, to + vec2(0.0, r * 0.7)], stroke);
}

/// Verrunden: Winkel mit abgerundeter Ecke (die Fase hat ein eigenes Symbol).
fn paint_blend_icon(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(STROKE_WIDTH, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let corner = c + vec2(-r, r);
    let bend = r * 0.9;
    let mut pts: Vec<Pos2> = vec![c + vec2(-r, -r), corner + vec2(0.0, -bend)];
    for i in 1..8 {
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / 8.0;
        pts.push(corner + vec2(bend * (1.0 - a.cos()), -bend * (1.0 - a.sin())));
    }
    pts.push(corner + vec2(bend, 0.0));
    pts.push(c + vec2(r, r));
    painter.add(egui::Shape::line(pts, stroke));
}

/// Fase: Winkel mit gerade abgeschrägter Ecke.
fn paint_chamfer_icon(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(STROKE_WIDTH, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let corner = c + vec2(-r, r);
    let bend = r * 0.9;
    let pts: Vec<Pos2> = vec![
        c + vec2(-r, -r),
        corner + vec2(0.0, -bend),
        corner + vec2(bend, 0.0),
        c + vec2(r, r),
    ];
    painter.add(egui::Shape::line(pts, stroke));
}

/// Zeichnet das Skizzensymbol in `rect`: ein Rechteck mit Eckpunkten und ein Kreis.
pub fn paint_sketch_icon(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(STROKE_WIDTH, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let frame = Rect::from_center_size(c + vec2(-r * 0.2, r * 0.2), vec2(r * 1.4, r * 1.1));
    painter.rect_stroke(frame, 0.0, stroke, egui::StrokeKind::Middle);
    for corner in [
        frame.left_top(),
        frame.right_top(),
        frame.left_bottom(),
        frame.right_bottom(),
    ] {
        painter.circle_filled(corner, 1.8, color);
    }
    painter.circle_stroke(
        frame.right_top() + vec2(-r * 0.1, r * 0.1),
        r * 0.45,
        stroke,
    );
}

/// Zeichnungssymbol: ein Blatt mit Innenrahmen und Schriftfeld.
pub fn paint_drawing_icon(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(STROKE_WIDTH, color);
    let thin = Stroke::new(1.0, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let sheet = Rect::from_center_size(c, vec2(r * 1.7, r * 1.3));
    painter.rect_stroke(sheet, 1.0, stroke, egui::StrokeKind::Middle);
    let inner = sheet.shrink(r * 0.22);
    painter.rect_stroke(inner, 0.0, thin, egui::StrokeKind::Middle);
    let block = Rect::from_min_max(
        pos2(inner.right() - r * 0.6, inner.bottom() - r * 0.3),
        inner.right_bottom(),
    );
    painter.rect_stroke(block, 0.0, thin, egui::StrokeKind::Middle);
}

/// Ansichtssymbol: ein kleines Rechteck mit Mittellinien.
pub fn paint_view_icon(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(STROKE_WIDTH, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.3;
    let frame = Rect::from_center_size(c, vec2(r * 2.0, r * 1.6));
    painter.rect_stroke(frame, 1.0, stroke, egui::StrokeKind::Middle);
    seg(
        painter,
        [
            pos2(c.x, frame.top() - 2.0),
            pos2(c.x, frame.bottom() + 2.0),
        ],
        Stroke::new(1.0, color),
    );
    seg(
        painter,
        [
            pos2(frame.left() - 2.0, c.y),
            pos2(frame.right() + 2.0, c.y),
        ],
        Stroke::new(1.0, color),
    );
}

/// Symbol einer Positionsnummer für die Zeilen im Abschnitt „Positionen“: Kreis mit
/// Hinweislinie und Punkt.
pub fn paint_balloon_icon(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(STROKE_WIDTH, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.22;
    let centre = c + vec2(r * 0.6, -r * 0.6);
    painter.circle_stroke(centre, r, stroke);
    let dot = c + vec2(-r * 1.5, r * 1.5);
    let dir = (centre - dot).normalized();
    seg(painter, [dot, centre - dir * r], Stroke::new(1.0, color));
    painter.circle_filled(dot, 1.5, color);
}

/// Symbol einer Maßart für die Zeilen im Abschnitt „Maße“: Maßlinie mit Pfeilspitzen
/// (waagerecht, senkrecht, schräg), Kreis mit Strich (Durchmesser), Kreisbogen mit Strecke
/// (Radius), zwei Strahlen mit Bogen (Winkel).
pub fn paint_dimension_icon(
    kind: planora_drawing::dimension::DimKind,
) -> impl FnOnce(&Painter, Rect, Color32) {
    use planora_drawing::dimension::DimKind;
    move |painter, rect, color| {
        let stroke = Stroke::new(STROKE_WIDTH, color);
        let thin = Stroke::new(1.0, color);
        let c = rect.center();
        let r = rect.width().min(rect.height()) * 0.36;
        // Maßlinie von `a` nach `b` mit kleinen Pfeilspitzen an beiden Enden.
        let dim_line = |a: Pos2, b: Pos2| {
            seg(painter, [a, b], stroke);
            let d = (b - a).normalized();
            let n = vec2(-d.y, d.x) * 2.5;
            for (tip, back) in [(a, d), (b, -d)] {
                let base = tip + back * 4.0;
                seg(painter, [tip, base + n], stroke);
                seg(painter, [tip, base - n], stroke);
            }
        };
        match kind {
            DimKind::Horizontal => {
                dim_line(c + vec2(-r, 0.0), c + vec2(r, 0.0));
                seg(
                    painter,
                    [c + vec2(-r, -r * 0.8), c + vec2(-r, r * 0.8)],
                    thin,
                );
                seg(painter, [c + vec2(r, -r * 0.8), c + vec2(r, r * 0.8)], thin);
            }
            DimKind::Vertical => {
                dim_line(c + vec2(0.0, -r), c + vec2(0.0, r));
                seg(
                    painter,
                    [c + vec2(-r * 0.8, -r), c + vec2(r * 0.8, -r)],
                    thin,
                );
                seg(painter, [c + vec2(-r * 0.8, r), c + vec2(r * 0.8, r)], thin);
            }
            DimKind::Aligned => {
                dim_line(c + vec2(-r, r * 0.7), c + vec2(r, -r * 0.7));
            }
            DimKind::Diameter => {
                painter.circle_stroke(c, r, stroke);
                seg(
                    painter,
                    [c + vec2(-r, r) * 1.15, c + vec2(r, -r) * 1.15],
                    thin,
                );
            }
            DimKind::Radius => {
                painter.circle_stroke(c, r, thin);
                seg(painter, [c, c + vec2(r, -r) * 0.99], stroke);
            }
            DimKind::Angle => {
                let o = c + vec2(-r, r * 0.8);
                seg(painter, [o, o + vec2(r * 2.0, 0.0)], stroke);
                seg(painter, [o, o + vec2(r * 1.6, -r * 1.4)], stroke);
                let pts: Vec<Pos2> = (0..=6_i32)
                    .map(|i| {
                        let t = -0.72 * (i as f32) / 6.0;
                        o + vec2(t.cos(), t.sin()) * r
                    })
                    .collect();
                painter.add(egui::Shape::line(pts, thin));
            }
        }
    }
}

/// Gezeichnete Symbole für Aktionen, zu denen Phosphor kein passendes Symbol hat (geprüft in
/// `Phosphor-Light-style.css`: es gibt weder Bogen, Ellipse, Parallel, Senkrecht, Tangente,
/// Mittelpunkt noch Punkt-auf-Kurve oder Punkt-Linie-Abstand). Radius, Durchmesser und Winkel
/// zeichnet weiter [`paint_dimension_icon`], Verrunden [`FeatureIcon::Blend`], Fase [`FeatureIcon::Chamfer`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HandIcon {
    Arc,
    Ellipse,
    Parallel,
    Perpendicular,
    Tangent,
    Midpoint,
    PointOnCurve,
    DistancePointLine,
}

/// Alle gezeichneten Symbole aus [`HandIcon`].
pub const HAND_DRAWN: &[HandIcon] = &[
    HandIcon::Arc,
    HandIcon::Ellipse,
    HandIcon::Parallel,
    HandIcon::Perpendicular,
    HandIcon::Tangent,
    HandIcon::Midpoint,
    HandIcon::PointOnCurve,
    HandIcon::DistancePointLine,
];

/// Zeichnet ein [`HandIcon`] in `rect` (Strichbreite 1,5, runde Enden).
pub fn paint_hand_icon(painter: &Painter, rect: Rect, icon: HandIcon, color: Color32) {
    let stroke = Stroke::new(STROKE_WIDTH, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let arc = |center: Pos2, radius: f32, from: f32, to: f32| {
        let pts: Vec<Pos2> = (0..=12_i32)
            .map(|i| {
                let a = from + (to - from) * (i as f32) / 12.0;
                center + vec2(a.cos(), -a.sin()) * radius
            })
            .collect();
        painter.add(egui::Shape::line(pts, stroke));
    };
    match icon {
        HandIcon::Arc => arc(
            c + vec2(0.0, r * 0.4),
            r * 1.1,
            0.4,
            std::f32::consts::PI - 0.4,
        ),
        HandIcon::Ellipse => {
            painter.add(egui::Shape::ellipse_stroke(
                c,
                vec2(r * 1.1, r * 0.65),
                stroke,
            ));
        }
        HandIcon::Parallel => {
            seg(
                painter,
                [c + vec2(-r, r * 0.9), c + vec2(r * 0.3, -r * 0.9)],
                stroke,
            );
            seg(
                painter,
                [c + vec2(-r * 0.2, r * 0.9), c + vec2(r * 1.1, -r * 0.9)],
                stroke,
            );
        }
        HandIcon::Perpendicular => {
            seg(painter, [c + vec2(-r, r), c + vec2(r, r)], stroke);
            seg(painter, [c + vec2(0.0, r), c + vec2(0.0, -r)], stroke);
            let m = r * 0.4;
            let corner = c + vec2(0.0, r);
            seg(
                painter,
                [corner + vec2(m, -m), corner + vec2(m, 0.0)],
                stroke,
            );
            seg(
                painter,
                [corner + vec2(m, -m), corner + vec2(0.0, -m)],
                stroke,
            );
        }
        HandIcon::Tangent => {
            painter.circle_stroke(c + vec2(0.0, -r * 0.2), r * 0.7, stroke);
            seg(
                painter,
                [c + vec2(-r, r * 0.5), c + vec2(r, r * 0.5)],
                stroke,
            );
        }
        HandIcon::Midpoint => {
            seg(painter, [c + vec2(-r, 0.0), c + vec2(r, 0.0)], stroke);
            painter.circle_filled(c, 2.2, color);
            seg(
                painter,
                [c + vec2(0.0, -r * 0.5), c + vec2(0.0, r * 0.5)],
                stroke,
            );
        }
        HandIcon::PointOnCurve => {
            arc(
                c + vec2(0.0, r * 0.6),
                r * 1.3,
                0.5,
                std::f32::consts::PI - 0.5,
            );
            painter.circle_filled(c + vec2(0.0, r * 0.6 - r * 1.3), 2.2, color);
        }
        HandIcon::DistancePointLine => {
            seg(
                painter,
                [c + vec2(-r, r * 0.8), c + vec2(r, r * 0.8)],
                stroke,
            );
            painter.circle_filled(c + vec2(0.0, -r * 0.8), 2.2, color);
            seg(
                painter,
                [c + vec2(0.0, -r * 0.5), c + vec2(0.0, r * 0.8)],
                stroke,
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Einheitlicher Zugriff: Symbolschrift oder gezeichnet
// ---------------------------------------------------------------------------------------------

/// Woher ein Symbol kommt: aus der Symbolschrift oder von einer Zeichenfunktion.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconSource {
    Font(Icon),
    Hand(HandIcon),
    Feature(FeatureIcon),
    Body(BodyKind),
    Dimension(planora_drawing::dimension::DimKind),
}

/// Symbol zu einer Körperart (Menü „Erzeugen“, Feature-Baum): wo Phosphor ein passendes Symbol
/// hat, die Schrift; sonst (Kegel, Netz, Kombination, ...) das Symbol des Feature-Baums.
pub fn body_icon(kind: BodyKind) -> IconSource {
    match kind {
        BodyKind::Box => IconSource::Font(Icon::Box),
        BodyKind::Cylinder => IconSource::Font(Icon::Cylinder),
        BodyKind::Sphere => IconSource::Font(Icon::Sphere),
        BodyKind::Extrusion => IconSource::Font(Icon::Extrude),
        BodyKind::Revolution => IconSource::Font(Icon::Revolve),
        BodyKind::Combined => IconSource::Font(Icon::Union),
        BodyKind::Step => IconSource::Font(Icon::StepBody),
        BodyKind::Cone | BodyKind::Mesh => IconSource::Body(kind),
    }
}

/// Symbol eines Skizzenwerkzeugs (`sketch_panel::TOOLS`).
pub fn sketch_tool_icon(tool: crate::sketch_state::Tool) -> IconSource {
    use crate::sketch_state::Tool as T;
    match tool {
        T::Select => IconSource::Font(Icon::ToolSelect),
        T::Point => IconSource::Font(Icon::ToolPoint),
        T::Line => IconSource::Font(Icon::ToolLine),
        T::Rectangle => IconSource::Font(Icon::ToolRectangle),
        T::Circle => IconSource::Font(Icon::ToolCircle),
        T::Arc => IconSource::Hand(HandIcon::Arc),
        T::Ellipse => IconSource::Hand(HandIcon::Ellipse),
        T::Spline => IconSource::Font(Icon::ToolSpline),
        T::ProjectEdge => IconSource::Font(Icon::ToolProjectEdge),
    }
}

/// Symbol einer Abhängigkeit (`sketch_panel::ACTIONS`).
pub fn sketch_action_icon(action: crate::sketch_state::ConstraintAction) -> IconSource {
    use crate::sketch_state::ConstraintAction as A;
    use planora_drawing::dimension::DimKind;
    match action {
        A::Coincident => IconSource::Font(Icon::Coincident),
        A::Horizontal => IconSource::Font(Icon::Horizontal),
        A::Vertical => IconSource::Font(Icon::Vertical),
        A::Parallel => IconSource::Hand(HandIcon::Parallel),
        A::Perpendicular => IconSource::Hand(HandIcon::Perpendicular),
        A::Tangent => IconSource::Hand(HandIcon::Tangent),
        A::Equal => IconSource::Font(Icon::Equal),
        A::Fixed => IconSource::Font(Icon::Fixed),
        A::Midpoint => IconSource::Hand(HandIcon::Midpoint),
        A::Symmetric => IconSource::Font(Icon::Symmetric),
        A::PointOnCurve => IconSource::Hand(HandIcon::PointOnCurve),
        A::Distance => IconSource::Font(Icon::Distance),
        A::DistancePointLine => IconSource::Hand(HandIcon::DistancePointLine),
        A::Radius => IconSource::Dimension(DimKind::Radius),
        A::Diameter => IconSource::Dimension(DimKind::Diameter),
        A::Angle => IconSource::Font(Icon::Angle),
    }
}

/// Zeichnet ein Symbol beliebiger Herkunft in `rect` (Symbolschrift: Glyph mittig).
pub fn paint_source(painter: &Painter, rect: Rect, source: IconSource, color: Color32) {
    match source {
        IconSource::Font(icon) => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                glyph(icon),
                egui::FontId::new(rect.height().min(rect.width()), fonts::icons()),
                color,
            );
        }
        IconSource::Hand(h) => paint_hand_icon(painter, rect, h, color),
        IconSource::Feature(f) => paint_feature_icon(painter, rect, f, color),
        IconSource::Body(k) => paint_body_icon(painter, rect, k, color),
        IconSource::Dimension(k) => paint_dimension_icon(k)(painter, rect, color),
    }
}

/// Knopf mit Symbol (16 px) + 8 px Abstand + Text, für Symbole jeder Herkunft (Menüpunkte,
/// Leistenschalter). `selected` zeichnet den Knopf als aktiv (`bg_active`, `text_primary`).
pub fn source_button(
    ui: &mut egui::Ui,
    source: IconSource,
    text: &str,
    dark: bool,
    selected: bool,
) -> egui::Response {
    source_button_impl(ui, source, text, true, dark, selected)
}

/// Knopf nur mit Symbol (16 px); `label` benennt ihn für Bildschirmleser und gehört als
/// Hinweistext an die Antwort (`on_hover_text`).
pub fn source_icon_button(
    ui: &mut egui::Ui,
    source: IconSource,
    label: &str,
    dark: bool,
) -> egui::Response {
    source_button_impl(ui, source, label, false, dark, false)
}

fn source_button_impl(
    ui: &mut egui::Ui,
    source: IconSource,
    text: &str,
    show_text: bool,
    dark: bool,
    selected: bool,
) -> egui::Response {
    let pad = ui.spacing().button_padding;
    let galley = egui::WidgetText::from(egui::RichText::new(if show_text { text } else { "" }))
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Button,
        );
    let gap = if show_text { LABEL_GAP } else { 0.0 };
    let size = vec2(
        pad.x * 2.0 + LABEL_ICON_SIZE + gap + galley.size().x,
        (LABEL_ICON_SIZE.max(galley.size().y) + pad.y * 2.0).max(ui.spacing().interact_size.y),
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    // Selbst gezeichnet: Art, Beschriftung und Zustand für Bildschirmleser selbst melden.
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, text)
    });
    if ui.is_rect_visible(rect) {
        let p = theme::palette(dark);
        let visuals = ui.style().interact_selectable(&response, selected);
        // Gesperrte Knöpfe bekommen beim Überfahren keine Fläche (nur den Hinweis).
        let hovered = response.hovered() && ui.is_enabled();
        let active = selected || hovered;
        if active {
            ui.painter()
                .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
        }
        let color = if active {
            p.text_primary
        } else {
            p.text_secondary
        };
        let icon_rect = Rect::from_center_size(
            pos2(rect.left() + pad.x + LABEL_ICON_SIZE / 2.0, rect.center().y),
            vec2(LABEL_ICON_SIZE, LABEL_ICON_SIZE),
        );
        paint_source(ui.painter(), icon_rect, source, color);
        let text_pos = pos2(
            rect.left() + pad.x + LABEL_ICON_SIZE + LABEL_GAP,
            rect.center().y - galley.size().y / 2.0,
        );
        ui.painter().galley(text_pos, galley, color);
    }
    // Ohne Rand: der Tastaturfokus zeigt sich als Ring.
    theme::focus_ring(ui, &response);
    response
}
