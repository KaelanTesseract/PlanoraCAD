//! Skizzenmodus (Graphit, Etappe C): drei schwebende Felder statt des früheren Fensters
//! „Skizze“. Links die Werkzeugleiste (Symbole, Name und Hinweis beim Überfahren), rechts das
//! Skizzenfeld (Abhängigkeiten, Maßeingabe, Bearbeiten, Liste der Abhängigkeiten), unten das
//! Fußfeld (Hinweis zum Werkzeug, Zustand der Skizze, „Skizze beenden“). Es wird nur gezeichnet
//! und an [`SketchSession`] weitergereicht.

use crate::formula_field::{FX, FormulaField, formula_text, names_hint, value_field_text};
use crate::icons::{self, HandIcon, Icon, IconSource};
use crate::keymap::{Action, Keymap, shortcut_hint, with_hint};
use crate::msg::Msg;
use crate::shell::{self, ShellMode, ShellRects, ShellState};
use crate::shell_panels;
use crate::sketch_kinds::ToolKind;
use crate::sketch_state::{
    ConstraintAction, Item, Severity, SketchSession, Tool, TransformStage, formula_problem,
};
use crate::sketch_view::{
    constraint_name_among, format_number, parse_number, round_centers, shown_value, stored_value,
};
use crate::state::AppState;
use crate::theme;
use egui::RichText;
use planora_feature::{FeatureId, ParamScope, ParamUnit, SketchFeature, constraint_unit};
use planora_i18n::{DisplayUnit, current_unit, tr};
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintId, ConstraintKind};
use planora_sketch::model::{CurveKind, Sketch};

const _: () = assert!(TOOL_ORDER.len() == crate::shell::STRIP_TOOLS);

/// Werkzeuge der Skizzenleiste in der Reihenfolge der Leiste.
pub const TOOL_ORDER: [Tool; 18] = [
    Tool::Select,
    Tool::Point,
    Tool::Line,
    Tool::Rectangle,
    Tool::Circle,
    Tool::Arc,
    Tool::Polygon,
    Tool::Slot,
    Tool::Ellipse,
    Tool::Spline,
    Tool::ProjectEdge,
    Tool::Trim,
    Tool::Extend,
    Tool::Break,
    Tool::Fillet,
    Tool::Offset,
    Tool::MoveCopy,
    Tool::Scale,
];

/// Erstes Werkzeug der Gruppe „Ändern“ (Fusion „Ändern“); davor steht in der Leiste eine
/// Trennlinie.
const MODIFY_FIRST: Tool = Tool::Trim;

/// Beschriftung eines Werkzeugs in der aktuellen Sprache.
pub fn tool_label(tool: Tool) -> String {
    match tool {
        Tool::Select => tr!("sketch-tool-select"),
        Tool::Point => tr!("sketch-tool-point"),
        Tool::Line => tr!("action-sketch-line"),
        Tool::Rectangle => tr!("action-sketch-rectangle"),
        Tool::Circle => tr!("action-sketch-circle"),
        Tool::Arc => tr!("sketch-tool-arc"),
        Tool::Polygon => tr!("sketch-tool-polygon"),
        Tool::Slot => tr!("sketch-tool-slot"),
        Tool::Ellipse => tr!("sketch-tool-ellipse"),
        Tool::Spline => tr!("sketch-tool-spline"),
        Tool::ProjectEdge => tr!("action-sketch-project-edge"),
        Tool::Trim => tr!("action-sketch-trim"),
        Tool::Extend => tr!("action-sketch-extend"),
        Tool::Break => tr!("action-sketch-break"),
        Tool::Fillet => tr!("sketch-tool-fillet"),
        Tool::Offset => tr!("action-sketch-offset"),
        Tool::MoveCopy => tr!("action-sketch-move-copy"),
        Tool::Scale => tr!("action-sketch-scale"),
        Tool::Dimension => tr!("action-sketch-dimension"),
        Tool::Pattern => tr!("action-sketch-pattern-rect"),
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
        Tool::Polygon => tr!("sketch-tool-polygon-hint"),
        Tool::Slot => tr!("sketch-tool-slot-hint"),
        Tool::Ellipse => tr!("sketch-tool-ellipse-hint"),
        Tool::Spline => tr!("sketch-tool-spline-hint"),
        Tool::ProjectEdge => tr!("sketch-tool-project-edge-hint"),
        Tool::Trim => tr!("sketch-tool-trim-hint"),
        Tool::Extend => tr!("sketch-tool-extend-hint"),
        Tool::Break => tr!("sketch-tool-break-hint"),
        Tool::Fillet => tr!("sketch-tool-fillet-hint"),
        Tool::Offset => tr!("sketch-tool-offset-hint"),
        Tool::MoveCopy => tr!("sketch-tool-move-copy-hint"),
        Tool::Scale => tr!("sketch-tool-scale-hint"),
        Tool::Dimension => tr!("sketch-tool-dimension-hint"),
        Tool::Pattern => tr!("sketch-tool-pattern-hint"),
    }
}

/// Werkzeuge der Skizzenleiste: Werkzeug, Beschriftung, Hinweis (in der aktuellen Sprache).
pub fn tools() -> [(Tool, String, String); 18] {
    TOOL_ORDER.map(|t| (t, tool_label(t), tool_hint(t)))
}

/// Abhängigkeiten, die die Skizzenleiste anbietet.
pub const ACTIONS: [ConstraintAction; 19] = {
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
        // Skizzen 5
        Concentric,
        Collinear,
        Smooth,
        Distance,
        DistancePointLine,
        Radius,
        Diameter,
        Angle,
    ]
};

/// Die Aktion der Tastenbelegung zu einem Knopf der Abhängigkeiten, soweit es eine gibt.
fn bindable_action(action: ConstraintAction) -> Option<Action> {
    match action {
        ConstraintAction::Concentric => Some(Action::SketchConcentric),
        ConstraintAction::Collinear => Some(Action::SketchCollinear),
        ConstraintAction::Smooth => Some(Action::SketchSmooth),
        _ => None,
    }
}

/// Größe der Symbole in der Werkzeugleiste.
const STRIP_ICON: f32 = 20.0;
/// So lange (Sekunden) gedrückt, öffnet ein Knopf mit Arten seine Liste.
const FLYOUT_HOLD_SECS: f64 = 0.4;
/// Abstand der Prüfung, ob das Halten lange genug war (ohne Eingabe zeichnet egui sonst nicht neu).
const FLYOUT_HOLD_REPAINT_SECS: f32 = 0.05;
/// Kantenlänge des Dreiecks auf einem Knopf mit Arten.
const FLYOUT_TRIANGLE: f32 = 5.0;
/// Abstand des Dreiecks von der Ecke des Knopfs.
const FLYOUT_TRIANGLE_INSET: f32 = 2.0;
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

#[derive(Clone, Debug, PartialEq)]
enum EditTarget {
    /// Neues Maß aus der Auswahl.
    New(ConstraintAction),
    /// Vorhandenes Maß ändern.
    Existing(ConstraintId),
    /// Von der Skizzenbemaßung gesetztes, noch nicht angelegtes Maß (Skizzen 5); gehört zu
    /// `SketchSession::dim.plan`, Esc legt nichts an.
    Planned(ConstraintKind),
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
    /// Vorbelegter Text und der Wert (Skizzeneinheit), aus dem er stammt: Wird in einer
    /// anderen Anzeigeeinheit als mm ein Längenmaß unverändert bestätigt, bleibt das Maß, wie es
    /// ist (kein Rundungsverlust durch Hin- und Zurückrechnen).
    shown: String,
    value: f64,
    /// Anzeigeeinheit beim Öffnen; nach dem Umstellen schließt das Feld.
    display: DisplayUnit,
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
    /// Anzeigeeinheit der Versatztexte; nach dem Umstellen stehen wieder die Vorgaben da.
    pattern_unit: DisplayUnit,
    /// Eigene Eingabefehler (z. B. keine Zahl), als Grund gespeichert und beim Zeichnen übersetzt.
    error: Option<Msg>,
}

impl PanelState {
    fn new(sketch: FeatureId) -> Self {
        Self {
            sketch,
            editor: None,
            section: None,
            pattern_dx: pattern_default(),
            pattern_dy: "0".into(),
            pattern_count: "3".into(),
            pattern_unit: current_unit(),
            error: None,
        }
    }
}

/// Vorgabe des Versatzes x eines Musters in mm.
const PATTERN_DX_MM: f64 = 20.0;

/// Vorgabe des Versatzes x als Feldtext in der Anzeigeeinheit („20“, in Zoll „0,787402“).
fn pattern_default() -> String {
    value_field_text(PATTERN_DX_MM, ParamUnit::Length)
}

/// Versatz eines Musters in mm aus den Texten der Felder x und y (Anzeigeeinheit). Eine
/// unveränderte Vorgabe (x: 20 mm, y: 0) gilt genau. `None` je Feld ohne gültige Zahl.
#[doc(hidden)]
pub fn pattern_offsets(dx: &str, dy: &str) -> (Option<f64>, Option<f64>) {
    let read = |text: &str, default_mm: f64| {
        if text.trim() == value_field_text(default_mm, ParamUnit::Length) {
            return Some(default_mm);
        }
        parse_number(text).map(|v| current_unit().from_display(v))
    };
    (read(dx, PATTERN_DX_MM), read(dy, 0.0))
}

fn panel_id() -> egui::Id {
    egui::Id::new("planora-sketch-panel")
}

/// Maßfeld für das bestehende Maß `id`: zeigt die Formel, sonst den Wert (Winkel in Grad,
/// gerundet). `None`, wenn es kein Maß ist.
fn existing_editor(
    se: &SketchSession,
    sf: &SketchFeature,
    id: ConstraintId,
    scope: &ParamScope,
) -> Option<Editor> {
    let c = sf.sketch.constraint(id)?;
    if !c.kind.is_dimension() {
        return None;
    }
    let unit = constraint_unit(&c.kind);
    let value = shown_value(&sf.sketch, &c.kind, c.driven).unwrap_or(0.0);
    let text = match sf.formulas.get(&id).filter(|_| !c.driven) {
        Some(f) => formula_text(f, unit, scope),
        None => editor_text(value, unit),
    };
    Some(Editor {
        target: EditTarget::Existing(id),
        shown: text.clone(),
        text,
        unit,
        driven: c.driven,
        focus: true,
        reveal: true,
        selection: se.selection.clone(),
        jumps: se.history_jumps,
        value,
        display: current_unit(),
    })
}

/// Zahlenfeld für ein neues Maß `action` aus der aktuellen Auswahl (Vorgabe: aktueller Wert).
fn new_editor(se: &SketchSession, sk: &Sketch, action: ConstraintAction) -> Editor {
    let unit = action.unit();
    let value = se.measure_action(sk, action).unwrap_or(0.0);
    let text = editor_text(value, unit);
    Editor {
        target: EditTarget::New(action),
        shown: text.clone(),
        text,
        unit,
        driven: false,
        focus: true,
        reveal: true,
        selection: se.selection.clone(),
        jumps: se.history_jumps,
        value,
        display: current_unit(),
    }
}

/// Maßfeld für das von der Skizzenbemaßung gesetzte Maß `kind` (Vorgabe: der gemessene Wert).
fn planned_editor(se: &SketchSession, kind: &ConstraintKind) -> Editor {
    let unit = constraint_unit(kind);
    let value = stored_value(kind).unwrap_or(0.0);
    let text = editor_text(value, unit);
    Editor {
        target: EditTarget::Planned(kind.clone()),
        shown: text.clone(),
        text,
        unit,
        driven: false,
        focus: true,
        reveal: true,
        selection: se.selection.clone(),
        jumps: se.history_jumps,
        value,
        display: current_unit(),
    }
}

/// Das Maßfeld folgt dem gesetzten Maß der Skizzenbemaßung: Ist eins gesetzt, öffnet es (auch
/// nach dem Umschalten Ø/R mit dem neuen Wert); ist es fort (Esc, anderes Werkzeug, neuer Klick,
/// Rückgängig), schließt ein Feld dafür.
fn follow_planned(panel: &mut PanelState, se: &SketchSession) {
    let planned = se.dim.plan.as_ref().map(|p| &p.kind);
    let open = match panel.editor.as_ref().map(|e| &e.target) {
        Some(EditTarget::Planned(k)) => Some(k),
        _ => None,
    };
    match (planned, open) {
        (Some(k), Some(o)) if k == o => {}
        (Some(k), _) => {
            panel.error = None;
            panel.editor = Some(planned_editor(se, k));
        }
        (None, Some(_)) => {
            panel.editor = None;
            panel.error = None;
        }
        (None, None) => {}
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
    let scope = state.document.param_scope();
    let Some(editor) = existing_editor(se, sf, id, &scope) else {
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

fn look_at_key() -> egui::Id {
    shell_panels::sketch_side_id().with("look-at")
}

/// Lage des Knopfs „Blick auf“ in der Palettenzeile im letzten Bild (für Tests).
#[doc(hidden)]
pub fn look_at_button(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data(|d| d.get_temp::<egui::Rect>(look_at_key()))
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

/// Hat das Werkzeug eine Liste von Arten (und darum ein Dreieck auf dem Knopf)?
pub fn has_flyout(tool: Tool) -> bool {
    !ToolKind::of(tool).is_empty()
}

fn flyout_id(tool: Tool) -> egui::Id {
    shell_panels::sketch_strip_id()
        .with("flyout")
        .with(tool as usize)
}

/// Ist eine Liste von Arten offen? Die Ansicht läuft im Bild vor der Leiste und liest das, damit
/// Esc, Eingabe und ein Drücken in die Zeichnung nur die Liste treffen.
pub fn flyout_open(ctx: &egui::Context) -> bool {
    TOOL_ORDER
        .iter()
        .any(|t| has_flyout(*t) && egui::Popup::is_id_open(ctx, flyout_id(*t)))
}

fn close_flyouts(ctx: &egui::Context) {
    for tool in TOOL_ORDER.into_iter().filter(|t| has_flyout(*t)) {
        egui::Popup::close_id(ctx, flyout_id(tool));
    }
}

fn flyout_entries_key() -> egui::Id {
    shell_panels::sketch_strip_id().with("flyout-entries")
}

/// Werkzeug und Art beim Öffnen der Liste; ändert eine Taste sie, schließt die Liste.
fn flyout_origin_key() -> egui::Id {
    shell_panels::sketch_strip_id().with("flyout-origin")
}

/// Die Liste ging durch Halten auf: Das Loslassen danach ist kein Klick auf den Knopf.
fn flyout_held_key() -> egui::Id {
    shell_panels::sketch_strip_id().with("flyout-held")
}

/// Name und Lage der Einträge der offenen Liste im letzten Bild (für Tests).
#[doc(hidden)]
pub fn flyout_entries(ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
    ctx.data(|d| d.get_temp::<Vec<(String, egui::Rect)>>(flyout_entries_key()))
        .unwrap_or_default()
}

/// Titel im Hinweistext der Leiste: Bei Formen mit Arten Name und Taste der zuletzt benutzten Art,
/// sonst Name und, falls belegt, Taste des Werkzeugs.
pub fn strip_title(km: &Keymap, se: &SketchSession, tool: Tool) -> String {
    if let Some(kind) = se.kind_of(tool) {
        return with_hint(km, kind.action(), &kind.label());
    }
    let label = tool_label(tool);
    match Action::for_sketch_tool(tool) {
        Some(a) => with_hint(km, a, &label),
        None => label,
    }
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
    let mut show_profiles = state.show_sketch_profiles;
    let mut palette = SketchPalette {
        grid: state.sketch_show_grid,
        snap: state.sketch_snap_grid,
        changed: false,
        look_at: false,
    };
    let Some((se, sf)) = state.session_draft_mut() else {
        return;
    };
    // Das Zahlenfeld gehört zur Auswahl und zum Stand (vor Rückgängig/Wiederholen), mit dem es
    // geöffnet wurde.
    if panel.editor.as_ref().is_some_and(|e| {
        e.selection != se.selection || e.jumps != se.history_jumps || e.display != current_unit()
    }) {
        panel.editor = None;
        panel.error = None;
    }
    follow_planned(&mut panel, se);
    // Anzeigeeinheit umgestellt: Die Versatztexte stünden in der alten Einheit.
    if panel.pattern_unit != current_unit() {
        panel.pattern_dx = pattern_default();
        panel.pattern_dy = "0".into();
        panel.pattern_unit = current_unit();
    }
    strip_field(ctx, strip, dark, se, &mut sf.sketch);
    let side = SideInput {
        panel: &mut panel,
        scope: &scope,
        show_profiles: &mut show_profiles,
        palette: &mut palette,
    };
    side_field(ctx, rects.right, dark, se, sf, side);
    let close = foot_field(ctx, rects.bottom, dark, se, &mut sf.sketch);
    state.show_sketch_profiles = show_profiles;
    state.sketch_show_grid = palette.grid;
    state.sketch_snap_grid = palette.snap;
    state.sketch_grid_changed |= palette.changed;
    if palette.look_at {
        state.look_at_sketch();
    }
    ctx.data_mut(|d| d.insert_temp(panel_id(), panel));
    if close {
        state.close_sketch();
        ctx.request_repaint();
    }
}

/// Werkzeugleiste: die Werkzeuge als Symbolknöpfe untereinander, vor Stutzen, Dehnen und
/// Brechen (Fusion „Ändern“) eine Trennlinie. Name (mit Taste, soweit
/// das Werkzeug ein Kürzel hat) und Hinweis stehen im Hinweistext, der Name (für
/// Bildschirmleser) in der Knopfbeschreibung. Ein Klick wählt wie die Taste
/// ([`SketchSession::choose_tool`]). Formen mit Arten tragen ein Dreieck; rechter Klick oder Halten
/// öffnet ihre Liste, siehe [`flyout_gesture`].
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
    let mut entries: Vec<(String, egui::Rect)> = Vec::new();
    // Die Tastenbelegung dieses Bilds (für die Taste im Hinweistext).
    let km = crate::keymap::mouse::current(ctx);
    // Eine Taste hat Werkzeug oder Art gewechselt: Die Liste gehört zum alten Stand.
    let origin = (se.tool, se.kind_of(se.tool));
    if flyout_open(ctx) && ctx.data(|d| d.get_temp(flyout_origin_key())) != Some(origin) {
        close_flyouts(ctx);
    }
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
                        if tool == MODIFY_FIRST {
                            strip_separator(ui, dark);
                        }
                        let selected = se.tool == tool;
                        // Name mit Taste („Linie (L)“), ohne Eintrag in der Tabelle nur der
                        // Name; bei Formen die zuletzt benutzte Art.
                        let title = strip_title(&km, se, tool);
                        let hint = se.kind_of(tool).map_or(hint, ToolKind::hint);
                        let flyout = has_flyout(tool);
                        let source = icons::sketch_tool_icon(tool);
                        let r = strip_button(ui, source, &label, selected, flyout, dark)
                            .on_hover_ui(|ui| {
                                ui.strong(&title);
                                if tool.is_modify() {
                                    ui.label(tr!("sketch-strip-modify", tool = label.clone()));
                                }
                                ui.label(&hint);
                            });
                        if flyout {
                            flyout_gesture(ctx, &r, tool, origin);
                        }
                        let held = ctx.data(|d| d.get_temp(flyout_held_key())).unwrap_or(false);
                        if r.clicked() && !held {
                            se.choose_tool(sk, tool);
                            egui::Popup::close_id(ctx, flyout_id(tool));
                        }
                        if flyout && let Some(kind) = flyout_list(&r, tool, &km, &mut entries) {
                            se.choose_kind(sk, kind);
                        }
                        seen.push((label, r.rect));
                    }
                });
        },
    );
    if !ctx.input(|i| i.pointer.any_down()) {
        ctx.data_mut(|d| d.remove_temp::<bool>(flyout_held_key()));
    }
    ctx.data_mut(|d| {
        d.insert_temp(strip_buttons_key(), seen);
        d.insert_temp(flyout_entries_key(), entries);
    });
}

/// Öffnet die Liste des Knopfs `r` bei rechtem Klick oder nach [`FLYOUT_HOLD_SECS`] Halten mit der
/// linken Taste. Das Loslassen nach dem Halten wählt nichts ([`flyout_held_key`]).
fn flyout_gesture(
    ctx: &egui::Context,
    r: &egui::Response,
    tool: Tool,
    origin: (Tool, Option<ToolKind>),
) {
    let mut open = r.secondary_clicked();
    let holding = r.is_pointer_button_down_on()
        && r.contains_pointer()
        && ctx.input(|i| i.pointer.primary_down());
    if holding {
        ctx.request_repaint_after_secs(FLYOUT_HOLD_REPAINT_SECS);
        let long_enough = ctx.input(|i| {
            i.pointer
                .press_start_time()
                .is_some_and(|t| i.time - t >= FLYOUT_HOLD_SECS)
        });
        if long_enough {
            ctx.data_mut(|d| d.insert_temp(flyout_held_key(), true));
            open = true;
        }
    }
    if open {
        egui::Popup::open_id(ctx, flyout_id(tool));
        ctx.data_mut(|d| d.insert_temp(flyout_origin_key(), origin));
    }
}

/// Zeigt die Liste des Knopfs `r`, wenn sie offen ist: alle Arten mit Namen und Kürzel, rechts
/// neben dem Knopf. Sie schließt bei einem Eintrag, bei Esc (macht `Popup` selbst) und bei einem
/// Drücken außerhalb von Liste und Knopf; `CloseOnClick` taugt nicht, das Loslassen nach dem Halten
/// schlösse sie sofort. Liefert die gewählte Art.
fn flyout_list(
    r: &egui::Response,
    tool: Tool,
    km: &Keymap,
    entries: &mut Vec<(String, egui::Rect)>,
) -> Option<ToolKind> {
    let id = flyout_id(tool);
    let mut picked = None;
    let shown = egui::Popup::menu(r)
        .id(id)
        .open_memory(None)
        .align(egui::RectAlign::RIGHT_START)
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .show(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            for kind in ToolKind::of(tool) {
                let keys = shortcut_hint(km, kind.action());
                let source = icons::sketch_kind_icon(*kind);
                let label = kind.label();
                let entry = icons::source_entry_keys(ui, source, &label, &keys, true)
                    .on_hover_text(kind.hint());
                if entry.clicked() {
                    picked = Some(*kind);
                    ui.close();
                }
                entries.push((label, entry.rect));
            }
        })?;
    let outside = r.ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !shown.response.rect.contains(p) && !r.rect.contains(p))
    });
    if outside {
        egui::Popup::close_id(&r.ctx, id);
    }
    picked
}

/// Trennlinie der Leiste: [`shell::STRIP_SEPARATOR_H`] hoch (zusätzlich zum Abstand der
/// Knöpfe), mittig ein waagerechter Strich in der Randfarbe.
fn strip_separator(ui: &mut egui::Ui, dark: bool) {
    let top = ui.cursor().top() - shell::STRIP_SPACING;
    ui.add_space(shell::STRIP_SEPARATOR_H);
    let y = (top + ui.cursor().top()) / 2.0;
    let x = ui.max_rect().x_range();
    ui.painter().hline(
        x.min + 4.0..=x.max - 4.0,
        y,
        egui::Stroke::new(1.0, theme::palette(dark).border_subtle),
    );
}

/// Knopf der Werkzeugleiste: 36 × 36, nur Symbol; gewählt mit neutraler Fläche `bg_active`,
/// beim Überfahren `bg_field`. `flyout`: unten rechts ein gefülltes Dreieck in der Farbe des Symbols.
fn strip_button(
    ui: &mut egui::Ui,
    source: IconSource,
    label: &str,
    selected: bool,
    flyout: bool,
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
        if flyout {
            let corner = rect.right_bottom() - egui::Vec2::splat(FLYOUT_TRIANGLE_INSET);
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    corner,
                    corner - egui::vec2(FLYOUT_TRIANGLE, 0.0),
                    corner - egui::vec2(0.0, FLYOUT_TRIANGLE),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
    }
    theme::focus_ring(ui, &response);
    response
}

/// Schalter „Skizzenraster“ und „Fang“ (wie Fusions Skizzenpalette). `changed`: einer wurde
/// betätigt.
struct SketchPalette {
    grid: bool,
    snap: bool,
    changed: bool,
    /// Der Knopf „Blick auf“ wurde gedrückt.
    look_at: bool,
}

/// Was das Skizzenfeld außer Sitzung und Entwurf braucht.
struct SideInput<'a> {
    panel: &'a mut PanelState,
    scope: &'a ParamScope,
    /// Schalter „Profil anzeigen“ ([`AppState::show_sketch_profiles`]).
    show_profiles: &'a mut bool,
    palette: &'a mut SketchPalette,
}

/// Skizzenfeld rechts (Spaltenbreite 250): Inhalt in einer senkrechten ScrollArea, höchstens so
/// hoch wie die Spalte.
fn side_field(
    ctx: &egui::Context,
    rect: egui::Rect,
    dark: bool,
    se: &mut SketchSession,
    sf: &mut SketchFeature,
    side: SideInput,
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
            theme::scroll_area(ui, area, |ui| side_contents(ui, se, sf, side));
        });
    });
}

/// Einstellungen des aktiven Werkzeugs: beim Versatz der Haken „Kette“, beim Verschieben der Haken
/// „Kopie erstellen“, „Bezugspunkt setzen“, OK und Abbrechen.
fn tool_options(ui: &mut egui::Ui, se: &mut SketchSession, sf: &mut SketchFeature) {
    if se.tool == Tool::Offset {
        ui.checkbox(&mut se.offset_chain, tr!("sketch-offset-chain"));
    }
    if se.runs_pattern() {
        pattern_options(ui, se, sf);
    }
    if se.runs_transform() {
        if se.tool == Tool::Scale {
            // Der Maßstab kopiert nicht; er sagt, was Eingabe jetzt tut.
            match se.transform.stage {
                TransformStage::Picking if !se.selection.is_empty() => {}
                TransformStage::Picking => {
                    ui.label(RichText::new(tr!("sketch-scale-hint")).small().weak());
                }
                TransformStage::SetBase => {
                    ui.label(RichText::new(tr!("sketch-scale-base")).small().weak());
                }
                TransformStage::Active => {}
            }
        } else {
            ui.checkbox(&mut se.transform.copy, tr!("sketch-move-copy"));
        }
        let active = se.transform.stage == TransformStage::Active;
        if ui
            .add_enabled(active, egui::Button::new(tr!("sketch-move-set-base")))
            .clicked()
        {
            se.begin_set_base();
        }
        ui.horizontal(|ui| {
            if ui.button(tr!("common-ok")).clicked() {
                // Fehler stehen in der Meldung der Sitzung.
                let _ = se.confirm_transform(sf);
            }
            if ui.button(tr!("common-cancel")).clicked() {
                se.cancel(&mut sf.sketch);
            }
        });
    }
}

/// Einstellungen der Anordnung (Skizzen 5): je Richtung Art (Achse oder Linie, „Linie wählen“),
/// „Gesamtlänge“ und „Symmetrisch“; beim Kreismuster „Voll“, „Winkel“, „Symmetrisch“; OK und
/// Abbrechen.
fn pattern_options(ui: &mut egui::Ui, se: &mut SketchSession, sf: &mut SketchFeature) {
    use planora_sketch::pattern::{CircularMode, PatternDir, PatternKind};
    let active = se.pattern.stage == TransformStage::Active;
    if let Some(kind) = se.pattern.plan.as_ref().map(|p| p.kind).filter(|_| active) {
        match kind {
            PatternKind::Rect {
                dirs,
                mut extent,
                mut symmetric,
            } => {
                let mut pick = None;
                for k in 0..2 {
                    let name = match dirs[k] {
                        PatternDir::SketchX => tr!("sketch-pattern-axis-x"),
                        PatternDir::SketchY => tr!("sketch-pattern-axis-y"),
                        PatternDir::Line(_) => tr!("sketch-pattern-line"),
                        PatternDir::Perpendicular => tr!("sketch-pattern-perpendicular"),
                    };
                    let n = (k + 1).to_string();
                    ui.label(format!(
                        "{}: {name}",
                        tr!("sketch-pattern-direction-n", n = n)
                    ));
                    ui.horizontal_wrapped(|ui| {
                        let picking =
                            se.pattern.picking_direction && se.pattern.direction_slot == k;
                        if ui
                            .selectable_label(picking, tr!("sketch-pattern-pick-line"))
                            .on_hover_text(tr!("sketch-pattern-pick-line-hint"))
                            .clicked()
                        {
                            pick = Some(k);
                        }
                        ui.checkbox(&mut extent[k], tr!("sketch-pattern-extent"))
                            .on_hover_text(tr!("sketch-pattern-extent-hint"));
                        ui.checkbox(&mut symmetric[k], tr!("sketch-pattern-symmetric"))
                            .on_hover_text(tr!("sketch-pattern-symmetric-hint"));
                    });
                }
                if let Some(plan) = se.pattern.plan.as_mut() {
                    plan.kind = PatternKind::Rect {
                        dirs,
                        extent,
                        symmetric,
                    };
                }
                if let Some(k) = pick {
                    se.pick_pattern_direction(k);
                }
            }
            PatternKind::Circular { center, mode } => {
                let mut m = mode;
                ui.horizontal(|ui| {
                    ui.radio_value(&mut m, CircularMode::Full, tr!("sketch-pattern-full"));
                    ui.radio_value(
                        &mut m,
                        CircularMode::Angle,
                        tr!("sketch-pattern-angle-mode"),
                    );
                    ui.radio_value(
                        &mut m,
                        CircularMode::Symmetric,
                        tr!("sketch-pattern-symmetric"),
                    );
                });
                if m != mode
                    && let Some(plan) = se.pattern.plan.as_mut()
                {
                    plan.kind = PatternKind::Circular { center, mode: m };
                }
            }
        }
    }
    ui.horizontal(|ui| {
        if ui.button(tr!("common-ok")).clicked() {
            // Fehler stehen in der Meldung der Sitzung.
            let _ = se.confirm_pattern(sf);
        }
        if ui.button(tr!("common-cancel")).clicked() {
            se.cancel(&mut sf.sketch);
        }
    });
}

/// Inhalt des Skizzenfelds: Name, Abhängigkeiten mit Maßeingabe, Bearbeiten, Liste.
fn side_contents(
    ui: &mut egui::Ui,
    se: &mut SketchSession,
    sf: &mut SketchFeature,
    side: SideInput,
) {
    let SideInput {
        panel,
        scope,
        show_profiles,
        palette,
    } = side;
    // Name, rechts daneben der Schalter „Profil anzeigen“ (wie Fusions Skizzenpalette
    // „Profil“: geschlossene Flächen hell eingefärbt); in einer Zeile, damit die Liste darunter
    // nicht nach unten rutscht.
    ui.horizontal(|ui| {
        ui.strong(&sf.sketch.name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.checkbox(show_profiles, tr!("sketch-show-profile"));
        });
    });
    // Skizzenraster und Fang in einer eigenen Zeile unter dem Namen.
    ui.horizontal(|ui| {
        let grid = ui
            .checkbox(&mut palette.grid, tr!("sketch-palette-grid"))
            .on_hover_text(tr!("sketch-palette-grid-hint"));
        let snap = ui
            .checkbox(&mut palette.snap, tr!("sketch-palette-snap"))
            .on_hover_text(tr!("sketch-palette-snap-hint"));
        palette.changed |= grid.changed() || snap.changed();
        // Nur das Symbol (die Spalte ist schmal); Name und Hinweis im Hinweistext.
        let label = tr!("sketch-look-at");
        let look = icons::source_icon_button(
            ui,
            IconSource::Hand(HandIcon::LookAt),
            &label,
            ui.visuals().dark_mode,
        )
        .on_hover_text(format!("{label}\n{}", tr!("sketch-look-at-hint")));
        ui.data_mut(|d| d.insert_temp(look_at_key(), look.rect));
        palette.look_at |= look.clicked();
    });
    tool_options(ui, se, sf);
    ui.add_space(2.0);

    // Abhängigkeiten
    theme::section_header(ui, &tr!("sketch-section-constraints"), theme::SUB_HEADER_H);
    let sk = &mut sf.sketch;
    let available = se.available_actions(sk);
    let km = crate::keymap::mouse::current(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        for action in ACTIONS {
            let enabled = available.contains(&action);
            let label = if action.is_dimension() {
                format!("{} …", action.label())
            } else {
                action.label().to_string()
            };
            let mut button = tool_button(ui, icons::sketch_action_icon(action), &label, enabled);
            // Belegbare Abhängigkeiten (Skizzen 5) nennen ihre Taste im Hinweistext.
            if let Some(bound) = bindable_action(action)
                && !shortcut_hint(&km, bound).is_empty()
            {
                button = button.on_hover_text(with_hint(&km, bound, &label));
            }
            if button
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
    let centerline = centerline();
    let centerline_tip = with_hint(
        &crate::keymap::mouse::current(ui.ctx()),
        Action::SketchCenterline,
        &centerline,
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
            IconSource::Hand(HandIcon::Centerline),
            &centerline,
            se.can_toggle_centerline(sk),
        )
        .on_hover_text(centerline_tip)
        .on_disabled_hover_text(tr!("sketch-select-lines-first"))
        .clicked()
        {
            se.toggle_centerline_if_possible(sk);
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
            // In der Anordnung gehört die Auswahl der Vorlage (wie Entf in der Ansicht).
            !se.selection.is_empty() && !se.runs_pattern(),
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
    constraint_list(ui, se, sf, panel, scope);
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
    let hint = se.active_hint();
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

/// Vorgabetext des Zahlenfelds (Winkel in Grad, eine Nachkommastelle; Längen in mm mit zwei
/// Nachkommastellen, in einer anderen Anzeigeeinheit umgerechnet wie die Formelfelder).
fn editor_text(value: f64, unit: ParamUnit) -> String {
    if unit == ParamUnit::Number {
        // Die Anzahl eines Musters: ganzzahlig, ohne Einheit.
        format_number(value, 0)
    } else if unit == ParamUnit::Angle {
        format_number(value.to_degrees(), 1)
    } else if current_unit() == DisplayUnit::Millimeter {
        format_number(value, 2)
    } else {
        value_field_text(value, unit)
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
    let title = match &editor.target {
        EditTarget::New(a) => a.label(),
        EditTarget::Existing(id) => match sf.sketch.constraint(*id) {
            Some(c) => crate::sketch_view::kind_label(&c.kind),
            None => {
                panel.editor = None;
                return None;
            }
        },
        EditTarget::Planned(k) => crate::sketch_view::kind_label(k),
    };
    let planned = matches!(editor.target, EditTarget::Planned(_));
    let toggles = match &editor.target {
        EditTarget::Planned(k) => crate::sketch_dimension::toggles(&sf.sketch, k),
        _ => false,
    };
    // Gesetztes Maß: Tab schaltet Ø/R um und lässt dem Feld die Tastatur (egui hat aus Tab schon
    // eine Fokusrichtung gemacht).
    let mut toggle =
        toggles && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
    if toggle {
        ui.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
    }
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
    // Esc im Feld eines gesetzten Maßes: nichts anlegen; die Ansicht sieht die Taste nicht mehr.
    if planned && response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        cancel = true;
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
        if toggles
            && ui
                .small_button(toggle_label())
                .on_hover_text(tr!("sketch-toggle-radius-diameter-hint"))
                .clicked()
        {
            toggle = true;
        }
    });
    if toggle && se.toggle_dimension_preview(&sf.sketch) {
        // Das Feld folgt dem umgeschalteten Maß mit seinem Wert; eine getippte Zahl bleibt
        // (wie Fusion die Eingabe behält, **Annahme**).
        let typed = editor.text.trim() != editor.shown.trim();
        if let Some(plan) = &se.dim.plan {
            let text = std::mem::take(&mut editor.text);
            *editor = planned_editor(se, &plan.kind);
            if typed {
                editor.text = text;
            }
        }
        se.dim.text = typed.then(|| editor.text.clone());
        return live;
    }
    // Der getippte Text gilt auch für einen Klick neben das Feld.
    if planned {
        se.dim.text = (editor.text.trim() != editor.shown.trim()).then(|| editor.text.clone());
    }
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
        if planned {
            se.cancel_planned();
        }
        panel.editor = None;
        panel.error = None;
        return None;
    }
    if !ok {
        return live;
    }
    panel.error = None;
    // Unverändert bestätigtes Längenmaß in einer anderen Anzeigeeinheit als mm: Der Text ist
    // gerundet; das Maß bleibt (bestehend) bzw. bekommt den gemessenen Wert (neu).
    let kept = editor.unit == ParamUnit::Length
        && current_unit() != DisplayUnit::Millimeter
        && editor.text.trim() == editor.shown.trim();
    let done = match (editor.target.clone(), kept) {
        (EditTarget::New(action), true) => {
            se.apply_dimension_value(sf, action, editor.value).is_ok()
        }
        (EditTarget::Existing(_), true) => true,
        (EditTarget::New(action), false) => se
            .apply_dimension_input(sf, action, &editor.text, scope)
            .is_ok(),
        (EditTarget::Existing(id), false) => {
            se.set_dimension_input(sf, id, &editor.text, scope).is_ok()
        }
        (EditTarget::Planned(kind), true) => se.apply_planned_value(sf, kind, editor.value).is_ok(),
        (EditTarget::Planned(kind), false) => {
            se.apply_planned(sf, kind, &editor.text, scope).is_ok()
        }
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
                    ui.label(current_unit().symbol());
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
                    // Versatz in der Anzeigeeinheit, die Skizze rechnet in mm.
                    let (dx, dy) = pattern_offsets(&panel.pattern_dx, &panel.pattern_dy);
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
    scope: &ParamScope,
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
    let centers = round_centers(sk);
    theme::scroll_area(
        ui,
        egui::ScrollArea::vertical()
            .max_height(LIST_HEIGHT)
            .auto_shrink([false, true]),
        |ui| {
            for (id, kind, driven) in rows {
                let item = Item::Constraint(id);
                let selected = se.selection.contains(&item);
                let mut text =
                    RichText::new(constraint_name_among(&sf.sketch, &centers, &kind, driven));
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
                    // Die Liste wählt ohne Klicklage auf einer Kurve.
                    se.drop_curve_picks();
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
                        && formula_row(ui, &formula_text(f, constraint_unit(&kind), scope))
                            .clicked()
                    {
                        se.drop_curve_picks();
                        se.selection = vec![item];
                        open = true;
                    }
                    if open && let Some(editor) = existing_editor(se, sf, id, scope) {
                        panel.error = None;
                        panel.editor = Some(editor);
                    }
                    let mut d = driven;
                    let mut flip = false;
                    let toggles = crate::sketch_dimension::toggles(&sf.sketch, &kind);
                    // Ein Musterwert lässt sich nicht anzeigend schalten (Entscheidung 3).
                    let pattern_value = matches!(kind, ConstraintKind::PatternValue(..));
                    let changed = ui
                        .horizontal(|ui| {
                            if pattern_value {
                                return false;
                            }
                            let r = ui
                                .checkbox(&mut d, tr!("sketch-driven"))
                                .on_hover_text(tr!("sketch-driven-hint"));
                            // Radius und Durchmesser umschalten (auch zur Mittellinie).
                            flip = toggles
                                && ui
                                    .small_button(toggle_label())
                                    .on_hover_text(tr!("sketch-toggle-radius-diameter-hint"))
                                    .clicked();
                            r.changed()
                        })
                        .inner;
                    if changed && se.set_driven(&mut sf.sketch, id, d).is_ok() {
                        // Ein anzeigendes Maß bestimmt nichts und hat keine Formel.
                        sf.prune_formulas();
                    }
                    if flip {
                        let _ = se.toggle_radius_diameter(sf, id);
                    }
                }
                if delete && !se.runs_pattern() {
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

/// Beschriftung des Knopfs, der ein Maß zwischen Radius und Durchmesser umschaltet („Ø/R“).
pub fn toggle_label() -> String {
    tr!("sketch-toggle-radius-diameter")
}

/// Beschriftung des Schalters [`auto_axis`] in zwei Zeilen (nach dem ersten Wort umbrochen).
pub fn auto_axis_label() -> String {
    auto_axis().replacen(' ', "\n", 1)
}

/// Erklärung zum Schalter [`auto_axis`] (Hinweistext).
pub fn auto_axis_hint() -> String {
    tr!("sketch-auto-axis-hint")
}

/// Schalter für die automatischen Abhängigkeiten beim Zeichnen.
pub fn auto_axis() -> String {
    tr!("sketch-auto-axis")
}

/// Beschriftung des Knopfs „Mittellinie“ (gleich dem Namen seiner Aktion).
pub fn centerline() -> String {
    Action::SketchCenterline.label()
}

/// Beschriftung des Knopfs „Konstruktion umschalten“ (gleich dem Namen seiner Aktion).
pub fn construction() -> String {
    Action::SketchConstruction.label()
}
