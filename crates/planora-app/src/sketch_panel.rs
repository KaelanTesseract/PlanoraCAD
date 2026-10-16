//! Schwebendes Fenster „Skizze“: Werkzeuge, Abhängigkeiten, Bearbeiten, Liste der
//! Abhängigkeiten und Statuszeile. Es wird nur gezeichnet und an [`SketchSession`] weitergereicht.

use crate::icons::{self, Icon, IconSource};
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

/// Breite des Fensters „Skizze“. Bleibt 300 statt der Graphit-Feldbreite 250: das Zahlenfeld
/// eines Maßes (Name, Feld, Einheit, „OK“, „Abbrechen“) passt in 250 nicht in eine Zeile.
const PANEL_WIDTH: f32 = 300.0;
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
    /// Auswahl beim Öffnen; ändert sie sich, gehört das Feld nicht mehr dazu und schließt.
    selection: Vec<Item>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Mirror,
    Pattern,
}

/// Zustand des Fensters zwischen den Frames (im egui-Speicher).
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
        selection: se.selection.clone(),
    }
}

fn load_panel(ctx: &egui::Context, index: FeatureId) -> PanelState {
    ctx.data(|d| d.get_temp::<PanelState>(panel_id()))
        .filter(|p| p.sketch == index)
        .unwrap_or_else(|| PanelState::new(index))
}

/// Öffnet das Zahlenfeld für das Maß `action` der aktuellen Auswahl, wie die Schaltfläche im
/// Fenster (auch für Tests ohne Mausklick).
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

/// Zeigt das Fenster „Skizze“, solange der Skizzen-Modus offen ist.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let Some(index) = state.open_sketch_id() else {
        return;
    };
    if state.session_sketch().is_none() {
        return;
    }
    let mut panel = load_panel(ctx, index);
    let mut close = false;
    theme::window(ctx, "Skizze")
        .id(panel_id().with("window"))
        // Startposition rechts neben dem linken Feld.
        .default_pos(egui::pos2(
            crate::shell::SHELL_INSET + theme::PANEL_WIDTH + theme::SPACE_6,
            90.0,
        ))
        // Feste Breite: Das Zahlenfeld oder die Liste sollen die Schaltflächen nicht umbrechen.
        .min_width(PANEL_WIDTH)
        .max_width(PANEL_WIDTH)
        .resizable(false)
        .show(ctx, |ui| {
            let Some((se, sk)) = state.session_sketch_mut() else {
                return;
            };
            close = contents(ui, se, sk, &mut panel);
        });
    ctx.data_mut(|d| d.insert_temp(panel_id(), panel));
    if close {
        state.close_sketch();
    }
}

/// Inhalt des Fensters; `true`, wenn „Skizze beenden“ gedrückt wurde.
fn contents(
    ui: &mut egui::Ui,
    se: &mut SketchSession,
    sk: &mut Sketch,
    panel: &mut PanelState,
) -> bool {
    // Das Zahlenfeld gehört zur Auswahl, mit der es geöffnet wurde.
    if panel
        .editor
        .as_ref()
        .is_some_and(|e| e.selection != se.selection)
    {
        panel.editor = None;
        panel.error = None;
    }
    let dark = ui.visuals().dark_mode;
    ui.strong(&sk.name);
    ui.add_space(2.0);

    // Werkzeuge
    theme::section_header(ui, "Werkzeuge", theme::SUB_HEADER_H);
    ui.horizontal_wrapped(|ui| {
        for (tool, label, _) in TOOLS {
            let selected = se.tool == tool;
            let source = icons::sketch_tool_icon(tool);
            if icons::source_button(ui, source, label, dark, selected).clicked() && !selected {
                se.set_tool(sk, tool);
            }
        }
    });
    if let Some((_, _, hint)) = TOOLS.iter().find(|(t, ..)| *t == se.tool) {
        ui.label(RichText::new(*hint).small().weak());
    }
    ui.separator();

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
    ui.add(theme::Toggle::new(
        &mut se.auto_constrain,
        "Automatisch waagerecht/senkrecht",
    ));
    ui.separator();

    // Liste der Abhängigkeiten
    theme::section_header(ui, "Abhängigkeiten der Skizze", theme::SUB_HEADER_H);
    constraint_list(ui, se, sk, panel);
    ui.separator();

    let close = ui
        .add(icons::button(
            Icon::EndSketch,
            RichText::new("Skizze beenden").strong(),
        ))
        .clicked();

    // Statuszeile
    let (text, severity) = se.status(sk);
    let p = theme::palette(ui.visuals().dark_mode);
    let color = match severity {
        Severity::Info if se.last_report.as_ref().is_some_and(|r| r.dof == 0) => p.ok,
        Severity::Info => p.text_secondary,
        Severity::Warning => p.warning,
        Severity::Error => p.error,
    };
    ui.colored_label(color, text);
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
    ui.horizontal(|ui| {
        ui.label(format!("{title}:"));
        let r = theme::text_field(
            ui,
            egui::TextEdit::singleline(&mut editor.text).desired_width(70.0),
        );
        if editor.focus {
            r.request_focus();
            editor.focus = false;
        }
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            ok = true;
        }
        ui.label(unit);
        if ui.small_button("OK").clicked() {
            ok = true;
        }
        if ui.small_button("Abbrechen").clicked() {
            cancel = true;
        }
    });
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
            ui.horizontal(|ui| {
                ui.label("Versatz x");
                theme::text_field(
                    ui,
                    egui::TextEdit::singleline(&mut panel.pattern_dx).desired_width(50.0),
                );
                ui.label("y");
                theme::text_field(
                    ui,
                    egui::TextEdit::singleline(&mut panel.pattern_dy).desired_width(50.0),
                );
                ui.label("mm");
            });
            ui.horizontal(|ui| {
                ui.label("Anzahl Kopien");
                theme::text_field(
                    ui,
                    egui::TextEdit::singleline(&mut panel.pattern_count).desired_width(40.0),
                );
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
/// Maß auf nur anzeigend, „Löschen“ entfernt die Abhängigkeit.
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
    egui::ScrollArea::vertical()
        .max_height(LIST_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (id, kind, driven) in rows {
                let item = Item::Constraint(id);
                let selected = se.selection.contains(&item);
                let mut text = RichText::new(constraint_name(sk, &kind, driven));
                if bad.contains(&id) {
                    text = text.color(theme::palette(ui.visuals().dark_mode).error);
                }
                ui.horizontal(|ui| {
                    let r = ui.selectable_label(selected, text);
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
                    if ui.small_button("Löschen").clicked() {
                        se.selection = vec![item];
                        se.delete_selection(sk);
                    }
                });
            }
        });
}
