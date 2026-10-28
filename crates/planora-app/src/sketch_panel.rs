//! Skizzenmodus (Graphit, Etappe C): drei schwebende Felder statt des früheren Fensters
//! „Skizze“. Links die Werkzeugleiste (Symbole, Name und Hinweis beim Überfahren), rechts das
//! Skizzenfeld (Abhängigkeiten, Maßeingabe, Bearbeiten, Liste der Abhängigkeiten), unten das
//! Fußfeld (Hinweis zum Werkzeug, Zustand der Skizze, „Skizze beenden“). Es wird nur gezeichnet
//! und an [`SketchSession`] weitergereicht.

use crate::formula_field::{FX, FormulaField, names_hint};
use crate::icons::{self, Icon, IconSource};
use crate::keymap::{Action, with_hint};
use crate::msg::Msg;
use crate::shell::{self, ShellMode, ShellRects, ShellState};
use crate::shell_panels;
use crate::sketch_state::{ConstraintAction, Item, Severity, SketchSession, Tool, formula_problem};
use crate::sketch_view::{constraint_name, format_number, parse_number, shown_value};
use crate::state::AppState;
use crate::theme;
use egui::RichText;
use planora_feature::{FeatureId, ParamScope, ParamUnit, SketchFeature, constraint_unit};
use planora_i18n::tr;
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintId, ConstraintKind};
use planora_sketch::model::{CurveKind, Sketch};

const _: () = assert!(TOOL_ORDER.len() == crate::shell::STRIP_TOOLS);

/// Werkzeuge der Skizzenleiste in der Reihenfolge der Leiste.
pub const TOOL_ORDER: [Tool; 9] = [
    Tool::Select,
    Tool::Point,
    Tool::Line,
    Tool::Rectangle,
    Tool::Circle,
    Tool::Arc,
    Tool::Ellipse,
    Tool::Spline,
    Tool::ProjectEdge,
];

/// Beschriftung eines Werkzeugs in der aktuellen Sprache.
pub fn tool_label(tool: Tool) -> String {
    match tool {
        Tool::Select => tr!("sketch-tool-select"),
        Tool::Point => tr!("sketch-tool-point"),
        Tool::Line => tr!("action-sketch-line"),
        Tool::Rectangle => tr!("action-sketch-rectangle"),
        Tool::Circle => tr!("action-sketch-circle"),
        Tool::Arc => tr!("sketch-tool-arc"),
        Tool::Ellipse => tr!("sketch-tool-ellipse"),
        Tool::Spline => tr!("sketch-tool-spline"),
        Tool::ProjectEdge => tr!("action-sketch-project-edge"),
    }
}

/// Hinweis zu einem Werkzeug in der aktuellen Sprache.
pub fn tool_hint(tool: Tool) -> String {
    match tool {
        Tool::Select => tr!("sketch-tool-select-hint"),
        Tool::Point => tr!("sketch-tool-point-hint"),
        Tool::Line => tr!("sketch-tool-line-hint"),
        Tool::Rectangle => tr!("sketch-tool-rectangle-hint"),
        Tool::Circle => tr!("sketch-tool-circle-hint"),
        Tool::Arc => tr!("sketch-tool-arc-hint"),
        Tool::Ellipse => tr!("sketch-tool-ellipse-hint"),
        Tool::Spline => tr!("sketch-tool-spline-hint"),
        Tool::ProjectEdge => tr!("sketch-tool-project-edge-hint"),
    }
}

/// Werkzeuge der Skizzenleiste: Werkzeug, Beschriftung, Hinweis (in der aktuellen Sprache).
pub fn tools() -> [(Tool, String, String); 9] {
    TOOL_ORDER.map(|t| (t, tool_label(t), tool_hint(t)))
}

/// Abhängigkeiten, die die Skizzenleiste anbietet.
pub const ACTIONS: [ConstraintAction; 16] = {
    use ConstraintAction::*;
    [
        Coincident,
        Horizontal,
        Vertical,
        Parallel,
        Perpendicular,
        Tangent,
        Equal,
        Fixed,
        Midpoint,
        Symmetric,
        PointOnCurve,
        Distance,
        DistancePointLine,
        Radius,
        Diameter,
        Angle,
    ]
};

/// Größe der Symbole in der Werkzeugleiste.
const STRIP_ICON: f32 = 20.0;
/// Schmalster Platz, in dem der Werkzeughinweis im Fuß noch gezeigt wird.
const MIN_HINT_W: f32 = 60.0;
/// Breite des Maßfelds (Text; davor bei einer Formel „fx“, dahinter Einheit bzw. „= Wert“,
/// OK/Abbrechen darunter). Mit „fx“ und „= 40,00 mm“ passt es in die Feldbreite 250.
const VALUE_WIDTH: f32 = 100.0;
/// Feste Kennung des Maßfelds: Sie hängt nicht vom Text ab, damit das Feld beim Wechsel von
/// Zahl zu Formel („fx“ erscheint davor) die Tastatur behält.
const VALUE_FIELD_ID: &str = "planora-sketch-dimension-field";
/// Name des Zeichens „fx“ in der Liste der Abhängigkeiten (Bildschirmleser).
pub fn list_fx_label() -> String {
    tr!("sketch-fx-label")
}
/// Höhe der Abhängigkeitsliste, ab der sie rollt.
const LIST_HEIGHT: f32 = 180.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum EditTarget {
    /// Neues Maß aus der Auswahl.
    New(ConstraintAction),
    /// Vorhandenes Maß ändern.
    Existing(ConstraintId),
}

#[derive(Clone, Debug)]
struct Editor {
    target: EditTarget,
    text: String,
    /// Einheit der Eingabe (Winkel in Grad, intern Bogenmaß).
    unit: ParamUnit,
    /// Anzeigendes Maß: nur Zahlen, kein Formelfeld.
    driven: bool,
    focus: bool,
    /// Feld noch ins Bild rollen (kleine Fenster: das Skizzenfeld rollt).
    reveal: bool,
    /// Auswahl beim Öffnen; ändert sie sich, gehört das Feld nicht mehr dazu und schließt.
    selection: Vec<Item>,
    /// Stand von `SketchSession::history_jumps` beim Öffnen; nach Rückgängig/Wiederholen
    /// gehört das Feld zum alten Stand und schließt (auch wenn es das Maß noch gibt).
    jumps: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Mirror,
    Pattern,
}

/// Zustand der Skizzenfelder zwischen den Frames (im egui-Speicher).
#[derive(Clone, Debug)]
struct PanelState {
    sketch: FeatureId,
    editor: Option<Editor>,
    section: Option<Section>,
    pattern_dx: String,
    pattern_dy: String,
    pattern_count: String,
    /// Eigene Eingabefehler (z. B. keine Zahl), als Grund gespeichert und beim Zeichnen übersetzt.
    error: Option<Msg>,
}

impl PanelState {
    fn new(sketch: FeatureId) -> Self {
        Self {
            sketch,
            editor: None,
            section: None,
            pattern_dx: "20".into(),
            pattern_dy: "0".into(),
            pattern_count: "3".into(),
            error: None,
        }
    }
}

fn panel_id() -> egui::Id {
    egui::Id::new("planora-sketch-panel")
}

/// Maßfeld für das bestehende Maß `id`: zeigt die Formel, sonst den Wert (Winkel in Grad,
/// gerundet). `None`, wenn es kein Maß ist.
fn existing_editor(se: &SketchSession, sf: &SketchFeature, id: ConstraintId) -> Option<Editor> {
    let c = sf.sketch.constraint(id)?;
    if !c.kind.is_dimension() {
        return None;
    }
    let unit = constraint_unit(&c.kind);
    let text = match sf.formulas.get(&id).filter(|_| !c.driven) {
        Some(f) => f.clone(),
        None => editor_text(
            shown_value(&sf.sketch, &c.kind, c.driven).unwrap_or(0.0),
            unit,
        ),
    };
    Some(Editor {
        target: EditTarget::Existing(id),
        text,
        unit,
        driven: c.driven,
        focus: true,
        reveal: true,
        selection: se.selection.clone(),
        jumps: se.history_jumps,
    })
}

/// Zahlenfeld für ein neues Maß `action` aus der aktuellen Auswahl (Vorgabe: aktueller Wert).
fn new_editor(se: &SketchSession, sk: &Sketch, action: ConstraintAction) -> Editor {
    let unit = action.unit();
    let value = se.measure_action(sk, action).unwrap_or(0.0);
    Editor {
        target: EditTarget::New(action),
        text: editor_text(value, unit),
        unit,
        driven: false,
        focus: true,
        reveal: true,
        selection: se.selection.clone(),
        jumps: se.history_jumps,
    }
}

fn load_panel(ctx: &egui::Context, index: FeatureId) -> PanelState {
    ctx.data(|d| d.get_temp::<PanelState>(panel_id()))
        .filter(|p| p.sketch == index)
        .unwrap_or_else(|| PanelState::new(index))
}

/// Öffnet das Zahlenfeld für das Maß `action` der aktuellen Auswahl, wie die Schaltfläche im
/// Skizzenfeld (auch für Tests ohne Mausklick).
pub fn open_dimension_editor(ctx: &egui::Context, state: &AppState, action: ConstraintAction) {
    let Some(se) = state.sketch_session.as_ref() else {
        return;
    };
    let Some(sk) = state.session_sketch() else {
        return;
    };
    if !action.is_dimension() {
        return;
    }
    let mut panel = load_panel(ctx, se.sketch);
    panel.error = None;
    panel.editor = Some(new_editor(se, sk, action));
    ctx.data_mut(|d| d.insert_temp(panel_id(), panel));
}

/// Öffnet das Maßfeld für das bestehende Maß `id` der offenen Skizze, wie ein Doppelklick
/// in der Liste (auch für Tests ohne Mausklick).
pub fn open_existing_dimension_editor(ctx: &egui::Context, state: &AppState, id: ConstraintId) {
    let (Some(se), Some(sf)) = (state.sketch_session.as_ref(), state.sketch_draft.as_ref()) else {
        return;
    };
    let Some(editor) = existing_editor(se, sf, id) else {
        return;
    };
    let mut panel = load_panel(ctx, se.sketch);
    panel.error = None;
    panel.editor = Some(editor);
    ctx.data_mut(|d| d.insert_temp(panel_id(), panel));
}

/// Ist das Zahlenfeld für ein Maß offen?
pub fn dimension_editor_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<PanelState>(panel_id()))
        .is_some_and(|p| p.editor.is_some())
}

fn strip_buttons_key() -> egui::Id {
    shell_panels::sketch_strip_id().with("buttons")
}

/// Name und Lage der Werkzeugknöpfe der Leiste im letzten Bild (für Tests).
#[doc(hidden)]
pub fn strip_buttons(ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
    ctx.data(|d| d.get_temp::<Vec<(String, egui::Rect)>>(strip_buttons_key()))
        .unwrap_or_default()
}

/// Rechtecke der Skizzenfelder: aus dem Layout der Shell in diesem Bild (die App zeichnet die
/// Felder vorher), sonst – ohne Shell, z. B. in Tests – aus [`shell::layout_in_mode`] über das
/// ganze Fenster.
fn sketch_rects(ctx: &egui::Context) -> ShellRects {
    if let Some(r) = shell_panels::last_layout(ctx).filter(|r| r.strip.is_some()) {
        return r;
    }
    let content = ctx.content_rect();
    let foot = shell_panels::stored(ctx, shell_panels::sketch_foot_width_key()).unwrap_or(0.0);
    shell::layout_in_mode(
        content,
        ShellMode::Sketch,
        &ShellState::default(),
        (content.width(), 0.0),
        foot,
        0.0,
        false,
    )
}

/// Zeigt die Skizzenfelder (Leiste, Skizzenfeld, Fuß), solange der Skizzen-Modus offen ist.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let Some(index) = state.open_sketch_id() else {
        return;
    };
    let rects = sketch_rects(ctx);
    let Some(strip) = rects.strip else {
        return;
    };
    let dark = state.dark;
    let mut panel = load_panel(ctx, index);
    // Formeln der Maße nennen Parameter nur lesend (das Fenster „Parameter“ ist gesperrt).
    let scope = state.document.param_scope();
    let Some((se, sf)) = state.session_draft_mut() else {
        return;
    };
    // Das Zahlenfeld gehört zur Auswahl und zum Stand (vor Rückgängig/Wiederholen), mit dem es
    // geöffnet wurde.
    if panel
        .editor
        .as_ref()
        .is_some_and(|e| e.selection != se.selection || e.jumps != se.history_jumps)
    {
        panel.editor = None;
        panel.error = None;
    }
    strip_field(ctx, strip, dark, se, &mut sf.sketch);
    side_field(ctx, rects.right, dark, se, sf, &mut panel, &scope);
    let close = foot_field(ctx, rects.bottom, dark, se, &mut sf.sketch);
    ctx.data_mut(|d| d.insert_temp(panel_id(), panel));
    if close {
        state.close_sketch();
        ctx.request_repaint();
    }
}

/// Werkzeugleiste: die neun Werkzeuge als Symbolknöpfe untereinander. Name (mit Taste, soweit
/// das Werkzeug ein Kürzel hat) und Hinweis stehen im Hinweistext, der Name (für
/// Bildschirmleser) in der Knopfbeschreibung. Ein Klick wählt wie die Taste
/// ([`SketchSession::choose_tool`]).
fn strip_field(
    ctx: &egui::Context,
    rect: egui::Rect,
    dark: bool,
    se: &mut SketchSession,
    sk: &mut Sketch,
) {
    let frame = theme::panel_frame(dark).inner_margin(egui::Margin::same(
        (shell::STRIP_PAD - theme::panel_frame(dark).stroke.width) as i8,
    ));
    let inner = shell_panels::inner_size(&frame, rect.size());
    let mut seen: Vec<(String, egui::Rect)> = Vec::new();
    // Die Tastenbelegung dieses Bilds (für die Taste im Hinweistext).
    let km = crate::keymap::mouse::current(ctx);
    shell_panels::field(
        ctx,
        shell_panels::sketch_strip_id(),
        rect.min,
        frame,
        |ui| {
            ui.set_width(inner.x);
            ui.set_height(inner.y);
            // Reicht die Höhe nicht (sehr niedrige Fenster), rollt die Leiste.
            egui::ScrollArea::vertical()
                .id_salt("planora-field-sketch-strip-scroll")
                .max_height(inner.y)
                .auto_shrink([false, true])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = shell::STRIP_SPACING;
                    for (tool, label, hint) in tools() {
                        let selected = se.tool == tool;
                        // Name mit Taste („Linie (L)“), ohne Eintrag in der Tabelle nur der
                        // Name.
                        let title = match Action::for_sketch_tool(tool) {
                            Some(a) => with_hint(&km, a, &label),
                            None => label.clone(),
                        };
                        let r =
                            strip_button(ui, icons::sketch_tool_icon(tool), &label, selected, dark)
                                .on_hover_ui(|ui| {
                                    ui.strong(&title);
                                    ui.label(&hint);
                                });
                        if r.clicked() {
                            se.choose_tool(sk, tool);
                        }
                        seen.push((label, r.rect));
                    }
                });
        },
    );
    ctx.data_mut(|d| d.insert_temp(strip_buttons_key(), seen));
}

/// Knopf der Werkzeugleiste: 36 × 36, nur Symbol; gewählt mit neutraler Fläche `bg_active`,
/// beim Überfahren `bg_field`.
fn strip_button(
    ui: &mut egui::Ui,
    source: IconSource,
    label: &str,
    selected: bool,
    dark: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::Vec2::splat(shell::STRIP_BUTTON), egui::Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label)
    });
    if ui.is_rect_visible(rect) {
        let p = theme::palette(dark);
        let hovered = response.hovered();
        let fill = if selected {
            Some(p.bg_active)
        } else if hovered {
            Some(p.bg_field)
        } else {
            None
        };
        if let Some(fill) = fill {
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(theme::RADIUS_SM), fill);
        }
        let color = if selected || hovered {
            p.text_primary
        } else {
            p.text_secondary
        };
        let icon = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(STRIP_ICON));
        icons::paint_source(ui.painter(), icon, source, color);
    }
    theme::focus_ring(ui, &response);
    response
}

/// Skizzenfeld rechts (Spaltenbreite 250): Inhalt in einer senkrechten ScrollArea, höchstens so
/// hoch wie die Spalte.
fn side_field(
    ctx: &egui::Context,
    rect: egui::Rect,
    dark: bool,
    se: &mut SketchSession,
    sf: &mut SketchFeature,
    panel: &mut PanelState,
    scope: &ParamScope,
) {
    let frame = theme::panel_frame(dark);
    let inner = shell_panels::inner_size(&frame, rect.size());
    let margin = f32::from(frame.inner_margin.left);
    shell_panels::field(ctx, shell_panels::sketch_side_id(), rect.min, frame, |ui| {
        shell_panels::side_column(ui, inner, margin, |ui| {
            let area = egui::ScrollArea::vertical()
                .id_salt("planora-field-sketch-side-scroll")
                .max_height(inner.y)
                .auto_shrink([false, true]);
            theme::scroll_area(ui, area, |ui| side_contents(ui, se, sf, panel, scope));
        });
    });
}

/// Inhalt des Skizzenfelds: Name, Abhängigkeiten mit Maßeingabe, Bearbeiten, Liste.
fn side_contents(
    ui: &mut egui::Ui,
    se: &mut SketchSession,
    sf: &mut SketchFeature,
    panel: &mut PanelState,
    scope: &ParamScope,
) {
    ui.strong(&sf.sketch.name);
    ui.add_space(2.0);

    // Abhängigkeiten
    theme::section_header(ui, &tr!("sketch-section-constraints"), theme::SUB_HEADER_H);
    let sk = &mut sf.sketch;
    let available = se.available_actions(sk);
    ui.horizontal_wrapped(|ui| {
        for action in ACTIONS {
            let enabled = available.contains(&action);
            let label = if action.is_dimension() {
                format!("{} …", action.label())
            } else {
                action.label().to_string()
            };
            if tool_button(ui, icons::sketch_action_icon(action), &label, enabled)
                .on_disabled_hover_text(tr!("sketch-not-fitting"))
                .clicked()
            {
                panel.error = None;
                if action.is_dimension() {
                    panel.editor = Some(new_editor(se, sk, action));
                } else {
                    let _ = se.apply(sk, action, None);
                }
            }
        }
    });
    let live = value_editor(ui, se, sf, panel, scope);
    // Meldungen: die eigenen und die der Sitzung (ohne die der gerade getippten Formel, die
    // schon unter dem Feld steht); sonst bei einer Skizze mit Formeln, die sich nicht lösen
    // lässt, ihr Fehler (Plan P12).
    let problem = formula_problem(sf, se.last_report.as_ref());
    if let Some(m) = panel
        .error
        .as_ref()
        .or(se.message.as_ref())
        .map(Msg::text)
        .filter(|m| live.as_deref() != Some(m.as_str()))
        .or(problem.map(|p| p.text()))
    {
        ui.colored_label(theme::palette(ui.visuals().dark_mode).error, m);
    }
    let sk = &mut sf.sketch;
    ui.separator();

    // Bearbeiten
    theme::section_header(ui, &tr!("sketch-section-edit"), theme::SUB_HEADER_H);
    let curves = se
        .selection
        .iter()
        .filter(|i| matches!(i, Item::Curve(_)))
        .count();
    let axis = mirror_axis(se, sk);
    // Die Taste steht im Hinweistext („Konstruktion umschalten (X)“).
    let construction = construction();
    let construction_tip = with_hint(
        &crate::keymap::mouse::current(ui.ctx()),
        Action::SketchConstruction,
        &construction,
    );
    ui.horizontal_wrapped(|ui| {
        if tool_button(
            ui,
            IconSource::Font(Icon::ToggleConstruction),
            &construction,
            se.can_toggle_construction(),
        )
        .on_hover_text(construction_tip)
        .on_disabled_hover_text(tr!("sketch-select-curves-first"))
        .clicked()
        {
            se.toggle_construction_if_possible(sk);
        }
        if tool_button(
            ui,
            IconSource::Font(Icon::Mirror),
            &tr!("sketch-mirror-button"),
            axis.is_some() && curves >= 2,
        )
        .on_disabled_hover_text(tr!("sketch-mirror-disabled"))
        .clicked()
        {
            panel.section = Some(Section::Mirror);
        }
        if tool_button(
            ui,
            IconSource::Font(Icon::Pattern),
            &tr!("sketch-pattern-button"),
            curves > 0,
        )
        .on_disabled_hover_text(tr!("sketch-select-curves-first"))
        .clicked()
        {
            panel.section = Some(Section::Pattern);
        }
        if tool_button(
            ui,
            IconSource::Font(Icon::Delete),
            &tr!("action-delete"),
            !se.selection.is_empty(),
        )
        .on_disabled_hover_text(tr!("sketch-delete-disabled"))
        .clicked()
        {
            se.delete_selection(sk);
        }
    });
    sections(ui, se, sk, panel);
    // In einer Zeile wäre die Beschriftung breiter als das Feld (250): zweizeilig; die Erklärung
    // steht im Hinweistext.
    ui.add(theme::Toggle::new(
        &mut se.auto_constrain,
        auto_axis_label(),
    ))
    .on_hover_text(auto_axis_hint());
    ui.separator();

    // Liste der Abhängigkeiten
    theme::section_header(ui, &tr!("sketch-section-list"), theme::SUB_HEADER_H);
    constraint_list(ui, se, sf, panel);
}

/// Fußfeld: rechts „Skizze beenden“, davor der Zustand der Skizze (Farbe nach Schweregrad und
/// Freiheitsgraden), links der Hinweis zum Werkzeug (gekürzt, ganz beim Überfahren). `true`, wenn
/// „Skizze beenden“ gedrückt wurde. Merkt sich Wunsch- und Mindestbreite für die Geometrie.
fn foot_field(
    ctx: &egui::Context,
    rect: egui::Rect,
    dark: bool,
    se: &mut SketchSession,
    sk: &mut Sketch,
) -> bool {
    let frame = shell_panels::bar_frame(dark, shell::BOTTOM_H);
    let inner = shell_panels::inner_size(&frame, rect.size());
    let margin = frame.total_margin().sum().x;
    let p = theme::palette(dark);
    let (text, severity) = se.status(sk);
    let color = match severity {
        Severity::Info if se.last_report.as_ref().is_some_and(|r| r.dof == 0) => p.ok,
        Severity::Info => p.text_secondary,
        Severity::Warning => p.warning,
        Severity::Error => p.error,
    };
    let hint = tool_hint(se.tool);
    let mut close = false;
    shell_panels::field(ctx, shell_panels::sketch_foot_id(), rect.min, frame, |ui| {
        ui.set_width(inner.x);
        ui.set_height(inner.y);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let right = ui.cursor().right();
            let end = ui.add(icons::button(
                Icon::EndSketch,
                RichText::new(tr!("sketch-end")).strong(),
            ));
            close = end.clicked();
            let gap = ui.spacing().item_spacing.x;
            let sep = ui.separator().rect.width() + gap;
            let status = ui.colored_label(color, &text);
            let fixed = right - status.rect.left();
            let galley = ui.painter().layout_no_wrap(
                hint.clone(),
                egui::TextStyle::Body.resolve(ui.style()),
                p.text_secondary,
            );
            // Der Hinweis nimmt den Rest (gekürzt, ganz beim Überfahren); ist kaum Platz, entfällt
            // er (er steht auch im Hinweistext der Leiste).
            if ui.available_width() - sep > MIN_HINT_W {
                ui.separator();
                ui.add(egui::Label::new(RichText::new(&hint).color(p.text_secondary)).truncate());
            }
            // Wunsch: alles in einer Zeile; mindestens: Zustand und Knopf.
            let wanted = fixed + gap + sep + galley.size().x + margin;
            shell_panels::remember_width(ui, shell_panels::sketch_foot_min_key(), fixed + margin);
            // Auf gerade Breite aufgerundet: so liegt das mittige Feld auf ganzen Pixeln.
            let even = (wanted / 2.0).ceil() * 2.0;
            shell_panels::remember_width(ui, shell_panels::sketch_foot_width_key(), even);
        });
    });
    close
}

/// Leistenknopf mit Symbol (Schrift oder gezeichnet) und Text, ohne Rand; gesperrt blass.
fn tool_button(
    ui: &mut egui::Ui,
    source: IconSource,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    // `add_enabled` statt `add_enabled_ui`: ohne eigenen Bereich bricht die Zeile richtig um.
    ui.add_enabled(enabled, |ui: &mut egui::Ui| {
        icons::source_button(ui, source, label, dark, false)
    })
}

/// Vorgabetext des Zahlenfelds (Winkel in Grad, eine Nachkommastelle).
fn editor_text(value: f64, unit: ParamUnit) -> String {
    if unit == ParamUnit::Angle {
        format_number(value.to_degrees(), 1)
    } else {
        format_number(value, 2)
    }
}

/// Zuletzt ausgewählte Linie (Spiegelachse).
fn mirror_axis(se: &SketchSession, sk: &Sketch) -> Option<planora_sketch::model::CurveId> {
    se.selection.iter().rev().find_map(|i| match i {
        Item::Curve(c) if matches!(sk.curve(*c)?.kind, CurveKind::Line { .. }) => Some(*c),
        _ => None,
    })
}

/// Maßfeld mit „OK“ (oder Eingabe) und „Abbrechen“: ein Formelfeld (Zahl oder Formel mit
/// Parameternamen; bei einer Formel „fx“ davor und „= Wert“ dahinter), bei anzeigenden Maßen
/// ein reines Zahlenfeld. Übernommen wird nur mit „OK“ oder Eingabe (Plan P9). Die Meldung
/// einer gerade getippten fehlerhaften Formel steht unter dem Feld (über den Knöpfen, damit sie
/// mit ihnen im Bild ist); solange sie ansteht, ist „OK“ gesperrt. Liefert diese Meldung.
fn value_editor(
    ui: &mut egui::Ui,
    se: &mut SketchSession,
    sf: &mut SketchFeature,
    panel: &mut PanelState,
    scope: &ParamScope,
) -> Option<String> {
    let editor = panel.editor.as_mut()?;
    let title = match editor.target {
        EditTarget::New(a) => a.label(),
        EditTarget::Existing(id) => match sf.sketch.constraint(id) {
            Some(c) => crate::sketch_view::kind_label(&c.kind),
            None => {
                panel.editor = None;
                return None;
            }
        },
    };
    let (mut ok, mut cancel) = (false, false);
    let mut live = None;
    // Untereinander, damit es in die Feldbreite 250 passt: Name, Feld mit Einheit bzw. Wert,
    // Namen der Parameter, Knöpfe.
    let top = ui.cursor().top();
    ui.label(format!("{title}:"));
    let response = if editor.driven {
        ui.horizontal(|ui| {
            let r = theme::text_field(
                ui,
                egui::TextEdit::singleline(&mut editor.text)
                    .id_salt(VALUE_FIELD_ID)
                    .desired_width(VALUE_WIDTH),
            );
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, &title));
            ui.label(editor.unit.suffix());
            r
        })
        .inner
    } else {
        let outcome = FormulaField::new(VALUE_FIELD_ID, &mut editor.text, editor.unit)
            .width(VALUE_WIDTH)
            .label(title.clone())
            .show(ui, scope);
        live = outcome.error;
        if let Some(hint) = names_hint(scope) {
            ui.label(RichText::new(hint).small().weak());
        }
        outcome.response
    };
    if editor.focus {
        response.request_focus();
        editor.focus = false;
    }
    // Eine Meldung zur vorigen Eingabe gilt nach einer Änderung des Textes nicht mehr.
    if response.changed() {
        se.message = None;
        panel.error = None;
    }
    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        ok = true;
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(live.is_none(), egui::Button::new(tr!("common-ok")).small())
            .on_disabled_hover_text(tr!("sketch-formula-has-error"))
            .clicked()
        {
            ok = true;
        }
        if ui.small_button(tr!("common-cancel")).clicked() {
            cancel = true;
        }
    });
    // Frisch geöffnet: Name, Zahl und Knöpfe ins Bild rollen (in kleinen Fenstern liegt das
    // Zahlenfeld sonst unter dem sichtbaren Teil des Skizzenfelds).
    if editor.reveal {
        let block = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
        ui.scroll_to_rect(block, None);
        if !ui.ctx().will_discard() {
            editor.reveal = false;
        }
    }
    if cancel {
        panel.editor = None;
        panel.error = None;
        return None;
    }
    if !ok {
        return live;
    }
    panel.error = None;
    let done = match editor.target {
        EditTarget::New(action) => se
            .apply_dimension_input(sf, action, &editor.text, scope)
            .is_ok(),
        EditTarget::Existing(id) => se.set_dimension_input(sf, id, &editor.text, scope).is_ok(),
    };
    if done {
        panel.editor = None;
    }
    live
}

/// Eingaben für „Spiegeln …“ und „Muster …“.
fn sections(ui: &mut egui::Ui, se: &mut SketchSession, sk: &mut Sketch, panel: &mut PanelState) {
    match panel.section {
        Some(Section::Mirror) => {
            let axis = mirror_axis(se, sk);
            ui.label(tr!("sketch-mirror-axis"));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        axis.is_some(),
                        egui::Button::new(tr!("sketch-mirror-do")).small(),
                    )
                    .clicked()
                    && let Some(axis) = axis
                    && se.mirror_selection(sk, axis).is_ok()
                {
                    panel.section = None;
                }
                if ui.small_button(tr!("common-cancel")).clicked() {
                    panel.section = None;
                }
            });
        }
        Some(Section::Pattern) => {
            // Je Wert eine Zeile, damit alles in die Feldbreite 250 passt.
            for (label, text) in [
                (tr!("sketch-offset-x"), &mut panel.pattern_dx),
                (tr!("sketch-offset-y"), &mut panel.pattern_dy),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    theme::text_field(ui, egui::TextEdit::singleline(text).desired_width(50.0));
                    ui.label("mm");
                });
            }
            ui.horizontal(|ui| {
                ui.label(tr!("sketch-pattern-count"));
                theme::text_field(
                    ui,
                    egui::TextEdit::singleline(&mut panel.pattern_count).desired_width(40.0),
                );
            });
            ui.horizontal(|ui| {
                if ui.small_button(tr!("create-create")).clicked() {
                    let dx = parse_number(&panel.pattern_dx);
                    let dy = parse_number(&panel.pattern_dy);
                    let count = panel.pattern_count.trim().parse::<usize>().ok();
                    match (dx, dy, count) {
                        (Some(dx), Some(dy), Some(n)) => {
                            panel.error = None;
                            if se.pattern_selection(sk, DVec2::new(dx, dy), n).is_ok() {
                                panel.section = None;
                            }
                        }
                        _ => panel.error = Some(crate::msg!("sketch-enter-number")),
                    }
                }
                if ui.small_button(tr!("common-cancel")).clicked() {
                    panel.section = None;
                    panel.error = None;
                }
            });
        }
        None => {}
    }
}

/// Liste der Abhängigkeiten: Klick wählt, Doppelklick ändert ein Maß, „anzeigend“ schaltet ein
/// Maß auf nur anzeigend, das Löschsymbol entfernt die Abhängigkeit. Maße mit Formel tragen
/// darunter „fx“ und die Formel; „fx“ öffnet das Maßfeld. Der Name zeigt weiter den Wert.
fn constraint_list(
    ui: &mut egui::Ui,
    se: &mut SketchSession,
    sf: &mut SketchFeature,
    panel: &mut PanelState,
) {
    let sk = &sf.sketch;
    let rows: Vec<(ConstraintId, ConstraintKind, bool)> = sk
        .constraints()
        .map(|(id, c)| (id, c.kind.clone(), c.driven))
        .collect();
    if rows.is_empty() {
        ui.label(RichText::new(tr!("sketch-list-empty")).small().weak());
        return;
    }
    let bad: Vec<ConstraintId> = se
        .last_report
        .as_ref()
        .map(|r| r.conflicting.iter().chain(&r.redundant).copied().collect())
        .unwrap_or_default();
    let shift = ui.input(|i| i.modifiers.shift);
    theme::scroll_area(
        ui,
        egui::ScrollArea::vertical()
            .max_height(LIST_HEIGHT)
            .auto_shrink([false, true]),
        |ui| {
            for (id, kind, driven) in rows {
                let item = Item::Constraint(id);
                let selected = se.selection.contains(&item);
                let mut text = RichText::new(constraint_name(&sf.sketch, &kind, driven));
                if bad.contains(&id) {
                    text = text.color(theme::palette(ui.visuals().dark_mode).error);
                }
                let mut delete = false;
                let mut name = None;
                ui.horizontal(|ui| {
                    // Löschen als Symbol rechts, damit der Name die Feldbreite behält.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dark = ui.visuals().dark_mode;
                        let delete_row = tr!("sketch-delete-row");
                        delete = icons::source_icon_button(
                            ui,
                            IconSource::Font(Icon::Delete),
                            &delete_row,
                            dark,
                        )
                        .on_hover_text(&delete_row)
                        .clicked();
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            name = Some(ui.selectable_label(selected, text));
                        });
                    });
                });
                let r = name.expect("Zeile gezeichnet");
                if r.clicked() {
                    if shift {
                        if selected {
                            se.selection.retain(|i| *i != item);
                        } else {
                            se.selection.push(item);
                        }
                    } else {
                        se.selection = vec![item];
                    }
                }
                if kind.is_dimension() {
                    let mut open = r.double_clicked();
                    if let Some(f) = sf.formulas.get(&id).filter(|_| !driven)
                        && formula_row(ui, f).clicked()
                    {
                        se.selection = vec![item];
                        open = true;
                    }
                    if open && let Some(editor) = existing_editor(se, sf, id) {
                        panel.error = None;
                        panel.editor = Some(editor);
                    }
                    let mut d = driven;
                    if ui
                        .checkbox(&mut d, tr!("sketch-driven"))
                        .on_hover_text(tr!("sketch-driven-hint"))
                        .changed()
                        && se.set_driven(&mut sf.sketch, id, d).is_ok()
                    {
                        // Ein anzeigendes Maß bestimmt nichts und hat keine Formel.
                        sf.prune_formulas();
                    }
                }
                if delete {
                    se.selection = vec![item];
                    se.delete_selection(&mut sf.sketch);
                }
            }
        },
    );
}

/// Zeile unter einem Maß mit Formel: „fx“ (öffnet das Maßfeld) und die Formel (gekürzt,
/// ganz beim Überfahren). Liefert die Antwort von „fx“.
fn formula_row(ui: &mut egui::Ui, formula: &str) -> egui::Response {
    let p = theme::palette(ui.visuals().dark_mode);
    ui.horizontal(|ui| {
        let fx = ui
            .add(egui::Button::new(RichText::new(FX).italics()).frame(false))
            .on_hover_text(tr!("sketch-fx-hint"));
        let fx_label = list_fx_label();
        fx.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &fx_label)
        });
        theme::focus_ring(ui, &fx);
        ui.add(egui::Label::new(RichText::new(formula).color(p.text_secondary)).truncate());
        fx
    })
    .inner
}

/// Beschriftung des Schalters [`auto_axis`] in zwei Zeilen (nach dem ersten Wort umbrochen).
pub fn auto_axis_label() -> String {
    auto_axis().replacen(' ', "\n", 1)
}

/// Erklärung zum Schalter [`auto_axis`] (Hinweistext).
pub fn auto_axis_hint() -> String {
    tr!("sketch-auto-axis-hint")
}

/// Schalter für automatisch waagerechte/senkrechte Linien.
pub fn auto_axis() -> String {
    tr!("sketch-auto-axis")
}

/// Beschriftung des Knopfs „Konstruktion umschalten“ (gleich dem Namen seiner Aktion).
pub fn construction() -> String {
    Action::SketchConstruction.label()
}
