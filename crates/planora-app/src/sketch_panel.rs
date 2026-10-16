//! Skizzenmodus (Graphit, Etappe C): drei schwebende Felder statt des früheren Fensters
//! „Skizze“. Links die Werkzeugleiste (Symbole, Name und Hinweis beim Überfahren), rechts das
//! Skizzenfeld (Abhängigkeiten, Maßeingabe, Bearbeiten, Liste der Abhängigkeiten), unten das
//! Fußfeld (Hinweis zum Werkzeug, Zustand der Skizze, „Skizze beenden“). Es wird nur gezeichnet
//! und an [`SketchSession`] weitergereicht.

use crate::icons::{self, Icon, IconSource};
use crate::shell::{self, ShellMode, ShellRects, ShellState};
use crate::shell_panels;
use crate::sketch_state::{ConstraintAction, Item, Severity, SketchSession, Tool};
use crate::sketch_view::{constraint_name, format_number, is_angle, parse_number, shown_value};
use crate::state::AppState;
use crate::theme;
use egui::RichText;
use planora_feature::FeatureId;
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintId, ConstraintKind};
use planora_sketch::model::{CurveKind, Sketch};

/// Werkzeuge der Skizzenleiste: Werkzeug, Beschriftung, Hinweis.
pub const TOOLS: [(Tool, &str, &str); 9] = [
    (
        Tool::Select,
        "Auswahl",
        "Klick wählt, Umschalt + Klick ergänzt; Punkte lassen sich ziehen.",
    ),
    (Tool::Point, "Punkt", "Klick setzt einen Punkt."),
    (
        Tool::Line,
        "Linie",
        "Punkte nacheinander anklicken; Eingabe, Doppelklick oder Esc beendet den Linienzug.",
    ),
    (
        Tool::Rectangle,
        "Rechteck",
        "Zwei gegenüberliegende Ecken anklicken.",
    ),
    (
        Tool::Circle,
        "Kreis",
        "Mittelpunkt, dann einen Punkt auf dem Kreis anklicken.",
    ),
    (
        Tool::Arc,
        "Bogen",
        "Mittelpunkt, Start und Ende (gegen den Uhrzeigersinn) anklicken.",
    ),
    (
        Tool::Ellipse,
        "Ellipse",
        "Mittelpunkt, Ende der großen Halbachse, dann die Breite anklicken.",
    ),
    (
        Tool::Spline,
        "Spline",
        "Stützpunkte anklicken; Eingabe oder Doppelklick legt den Spline an.",
    ),
    (
        Tool::ProjectEdge,
        "Körperkante kopieren",
        "Eine Kante eines Körpers anklicken; sie wird als feste Konstruktionslinie übernommen.",
    ),
];

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
/// Breite des Zahlenfelds der Maßeingabe (Text; Einheit daneben, OK/Abbrechen darunter).
const VALUE_WIDTH: f32 = 120.0;
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
    /// Eingabe in Grad (intern Radiant).
    angle: bool,
    focus: bool,
    /// Feld noch ins Bild rollen (kleine Fenster: das Skizzenfeld rollt).
    reveal: bool,
    /// Auswahl beim Öffnen; ändert sie sich, gehört das Feld nicht mehr dazu und schließt.
    selection: Vec<Item>,
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
    /// Eigene Eingabefehler (z. B. keine Zahl).
    error: Option<String>,
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

const NOT_A_NUMBER: &str = "Bitte eine Zahl eingeben";

/// Zahlenfeld für ein neues Maß `action` aus der aktuellen Auswahl (Vorgabe: aktueller Wert).
fn new_editor(se: &SketchSession, sk: &Sketch, action: ConstraintAction) -> Editor {
    let angle = action == ConstraintAction::Angle;
    let value = se.measure_action(sk, action).unwrap_or(0.0);
    Editor {
        target: EditTarget::New(action),
        text: editor_text(value, angle),
        angle,
        focus: true,
        reveal: true,
        selection: se.selection.clone(),
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
pub fn strip_buttons(ctx: &egui::Context) -> Vec<(&'static str, egui::Rect)> {
    ctx.data(|d| d.get_temp::<Vec<(&'static str, egui::Rect)>>(strip_buttons_key()))
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
    let Some((se, sk)) = state.session_sketch_mut() else {
        return;
    };
    // Das Zahlenfeld gehört zur Auswahl, mit der es geöffnet wurde.
    if panel
        .editor
        .as_ref()
        .is_some_and(|e| e.selection != se.selection)
    {
        panel.editor = None;
        panel.error = None;
    }
    strip_field(ctx, strip, dark, se, sk);
    side_field(ctx, rects.right, dark, se, sk, &mut panel);
    let close = foot_field(ctx, rects.bottom, dark, se, sk);
    ctx.data_mut(|d| d.insert_temp(panel_id(), panel));
    if close {
        state.close_sketch();
    }
}

/// Werkzeugleiste: die neun Werkzeuge als Symbolknöpfe untereinander. Name und Hinweis stehen im
/// Hinweistext und (für Bildschirmleser) in der Knopfbeschreibung.
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
    let mut seen: Vec<(&'static str, egui::Rect)> = Vec::new();
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
                    for (tool, label, hint) in TOOLS {
                        let selected = se.tool == tool;
                        let r =
                            strip_button(ui, icons::sketch_tool_icon(tool), label, selected, dark)
                                .on_hover_ui(|ui| {
                                    ui.strong(label);
                                    ui.label(hint);
                                });
                        if r.clicked() && !selected {
                            se.set_tool(sk, tool);
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
    sk: &mut Sketch,
    panel: &mut PanelState,
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
            theme::scroll_area(ui, area, |ui| side_contents(ui, se, sk, panel));
        });
    });
}

/// Inhalt des Skizzenfelds: Name, Abhängigkeiten mit Maßeingabe, Bearbeiten, Liste.
fn side_contents(
    ui: &mut egui::Ui,
    se: &mut SketchSession,
    sk: &mut Sketch,
    panel: &mut PanelState,
) {
    ui.strong(&sk.name);
    ui.add_space(2.0);

    // Abhängigkeiten
    theme::section_header(ui, "Abhängigkeiten", theme::SUB_HEADER_H);
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
                .on_disabled_hover_text("Passt nicht zur aktuellen Auswahl.")
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
    value_editor(ui, se, sk, panel);
    if let Some(m) = panel.error.as_ref().or(se.message.as_ref()) {
        ui.colored_label(theme::palette(ui.visuals().dark_mode).error, m);
    }
    ui.separator();

    // Bearbeiten
    theme::section_header(ui, "Bearbeiten", theme::SUB_HEADER_H);
    let curves = se
        .selection
        .iter()
        .filter(|i| matches!(i, Item::Curve(_)))
        .count();
    let axis = mirror_axis(se, sk);
    ui.horizontal_wrapped(|ui| {
        if tool_button(
            ui,
            IconSource::Font(Icon::ToggleConstruction),
            "Konstruktion umschalten",
            curves > 0,
        )
        .on_disabled_hover_text("Zuerst Kurven auswählen.")
        .clicked()
        {
            se.toggle_construction(sk);
        }
        if tool_button(
            ui,
            IconSource::Font(Icon::Mirror),
            "Spiegeln …",
            axis.is_some() && curves >= 2,
        )
        .on_disabled_hover_text("Kurven und zuletzt die Linie auswählen, an der gespiegelt wird.")
        .clicked()
        {
            panel.section = Some(Section::Mirror);
        }
        if tool_button(ui, IconSource::Font(Icon::Pattern), "Muster …", curves > 0)
            .on_disabled_hover_text("Zuerst Kurven auswählen.")
            .clicked()
        {
            panel.section = Some(Section::Pattern);
        }
        if tool_button(
            ui,
            IconSource::Font(Icon::Delete),
            "Löschen",
            !se.selection.is_empty(),
        )
        .on_disabled_hover_text("Es ist nichts ausgewählt.")
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
    .on_hover_text(AUTO_AXIS_HINT);
    ui.separator();

    // Liste der Abhängigkeiten
    theme::section_header(ui, "Abhängigkeiten der Skizze", theme::SUB_HEADER_H);
    constraint_list(ui, se, sk, panel);
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
    let hint = TOOLS
        .iter()
        .find(|(t, ..)| *t == se.tool)
        .map_or("", |(_, _, h)| *h);
    let mut close = false;
    shell_panels::field(ctx, shell_panels::sketch_foot_id(), rect.min, frame, |ui| {
        ui.set_width(inner.x);
        ui.set_height(inner.y);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let right = ui.cursor().right();
            let end = ui.add(icons::button(
                Icon::EndSketch,
                RichText::new("Skizze beenden").strong(),
            ));
            close = end.clicked();
            let gap = ui.spacing().item_spacing.x;
            let sep = ui.separator().rect.width() + gap;
            let status = ui.colored_label(color, &text);
            let fixed = right - status.rect.left();
            let galley = ui.painter().layout_no_wrap(
                hint.to_owned(),
                egui::TextStyle::Body.resolve(ui.style()),
                p.text_secondary,
            );
            // Der Hinweis nimmt den Rest (gekürzt, ganz beim Überfahren); ist kaum Platz, entfällt
            // er (er steht auch im Hinweistext der Leiste).
            if ui.available_width() - sep > MIN_HINT_W {
                ui.separator();
                ui.add(egui::Label::new(RichText::new(hint).color(p.text_secondary)).truncate());
            }
            // Wunsch: alles in einer Zeile; mindestens: Zustand und Knopf.
            let wanted = fixed + gap + sep + galley.size().x + margin;
            shell_panels::remember_width(ui, shell_panels::sketch_foot_min_key(), fixed + margin);
            shell_panels::remember_width(ui, shell_panels::sketch_foot_width_key(), wanted.ceil());
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

/// Vorgabetext des Zahlenfelds (Winkel in Grad).
fn editor_text(value: f64, angle: bool) -> String {
    if angle {
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

/// Zahlenfeld für ein Maß mit „OK“ (oder Eingabe) und „Abbrechen“.
fn value_editor(
    ui: &mut egui::Ui,
    se: &mut SketchSession,
    sk: &mut Sketch,
    panel: &mut PanelState,
) {
    let Some(editor) = panel.editor.as_mut() else {
        return;
    };
    let title = match editor.target {
        EditTarget::New(a) => a.label().to_string(),
        EditTarget::Existing(id) => match sk.constraint(id) {
            Some(c) => crate::sketch_view::kind_label(&c.kind).to_string(),
            None => {
                panel.editor = None;
                return;
            }
        },
    };
    let unit = if editor.angle { "°" } else { "mm" };
    let (mut ok, mut cancel) = (false, false);
    // Untereinander, damit es in die Feldbreite 250 passt: Name, Zahl mit Einheit, Knöpfe.
    let top = ui.cursor().top();
    ui.label(format!("{title}:"));
    ui.horizontal(|ui| {
        let r = theme::text_field(
            ui,
            egui::TextEdit::singleline(&mut editor.text).desired_width(VALUE_WIDTH),
        );
        if editor.focus {
            r.request_focus();
            editor.focus = false;
        }
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            ok = true;
        }
        ui.label(unit);
    });
    ui.horizontal(|ui| {
        if ui.small_button("OK").clicked() {
            ok = true;
        }
        if ui.small_button("Abbrechen").clicked() {
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
        return;
    }
    if !ok {
        return;
    }
    let Some(v) = parse_number(&editor.text) else {
        panel.error = Some(NOT_A_NUMBER.into());
        return;
    };
    let v = if editor.angle { v.to_radians() } else { v };
    panel.error = None;
    let done = match editor.target {
        EditTarget::New(action) => se.apply(sk, action, Some(v)).is_ok(),
        EditTarget::Existing(id) => se.set_dimension_value(sk, id, v).is_ok(),
    };
    if done {
        panel.editor = None;
    }
}

/// Eingaben für „Spiegeln …“ und „Muster …“.
fn sections(ui: &mut egui::Ui, se: &mut SketchSession, sk: &mut Sketch, panel: &mut PanelState) {
    match panel.section {
        Some(Section::Mirror) => {
            let axis = mirror_axis(se, sk);
            ui.label("Achse: die zuletzt ausgewählte Linie.");
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(axis.is_some(), egui::Button::new("Spiegeln").small())
                    .clicked()
                    && let Some(axis) = axis
                    && se.mirror_selection(sk, axis).is_ok()
                {
                    panel.section = None;
                }
                if ui.small_button("Abbrechen").clicked() {
                    panel.section = None;
                }
            });
        }
        Some(Section::Pattern) => {
            // Je Wert eine Zeile, damit alles in die Feldbreite 250 passt.
            for (label, text) in [
                ("Versatz x", &mut panel.pattern_dx),
                ("Versatz y", &mut panel.pattern_dy),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    theme::text_field(ui, egui::TextEdit::singleline(text).desired_width(50.0));
                    ui.label("mm");
                });
            }
            ui.horizontal(|ui| {
                ui.label("Anzahl Kopien");
                theme::text_field(
                    ui,
                    egui::TextEdit::singleline(&mut panel.pattern_count).desired_width(40.0),
                );
            });
            ui.horizontal(|ui| {
                if ui.small_button("Erzeugen").clicked() {
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
                        _ => panel.error = Some(NOT_A_NUMBER.into()),
                    }
                }
                if ui.small_button("Abbrechen").clicked() {
                    panel.section = None;
                    panel.error = None;
                }
            });
        }
        None => {}
    }
}

/// Liste der Abhängigkeiten: Klick wählt, Doppelklick ändert ein Maß, „anzeigend“ schaltet ein
/// Maß auf nur anzeigend, das Löschsymbol entfernt die Abhängigkeit.
fn constraint_list(
    ui: &mut egui::Ui,
    se: &mut SketchSession,
    sk: &mut Sketch,
    panel: &mut PanelState,
) {
    let rows: Vec<(ConstraintId, ConstraintKind, bool)> = sk
        .constraints()
        .map(|(id, c)| (id, c.kind.clone(), c.driven))
        .collect();
    if rows.is_empty() {
        ui.label(RichText::new("Noch keine Abhängigkeiten.").small().weak());
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
                let mut text = RichText::new(constraint_name(sk, &kind, driven));
                if bad.contains(&id) {
                    text = text.color(theme::palette(ui.visuals().dark_mode).error);
                }
                let mut delete = false;
                let mut name = None;
                ui.horizontal(|ui| {
                    // Löschen als Symbol rechts, damit der Name die Feldbreite behält.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dark = ui.visuals().dark_mode;
                        delete = icons::source_icon_button(
                            ui,
                            IconSource::Font(Icon::Delete),
                            DELETE_ROW,
                            dark,
                        )
                        .on_hover_text(DELETE_ROW)
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
                    if r.double_clicked() {
                        let angle = is_angle(&kind);
                        let value = shown_value(sk, &kind, driven).unwrap_or(0.0);
                        panel.error = None;
                        panel.editor = Some(Editor {
                            target: EditTarget::Existing(id),
                            text: editor_text(value, angle),
                            angle,
                            focus: true,
                            reveal: true,
                            selection: se.selection.clone(),
                        });
                    }
                    let mut d = driven;
                    if ui
                        .checkbox(&mut d, "anzeigend")
                        .on_hover_text("Nur anzeigendes Maß (bestimmt nichts)")
                        .changed()
                    {
                        let _ = se.set_driven(sk, id, d);
                    }
                }
                if delete {
                    se.selection = vec![item];
                    se.delete_selection(sk);
                }
            }
        },
    );
}

/// Beschriftung des Schalters [`AUTO_AXIS`] in zwei Zeilen („erkennen“ in der zweiten).
pub fn auto_axis_label() -> String {
    AUTO_AXIS.replacen(
        ' ', "
", 1,
    )
}

/// Erklärung zum Schalter [`AUTO_AXIS`] (Hinweistext).
pub const AUTO_AXIS_HINT: &str = "Fast waagerechte oder senkrechte Linien bekommen beim Zeichnen von selbst die passende Abhängigkeit.";

/// Schalter für automatisch waagerechte/senkrechte Linien.
pub const AUTO_AXIS: &str = "Waagerecht/senkrecht erkennen";

/// Beschreibung des Löschknopfs einer Zeile der Liste.
const DELETE_ROW: &str = "Abhängigkeit löschen";
