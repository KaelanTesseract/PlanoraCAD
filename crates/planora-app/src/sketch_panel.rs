//! Schwebendes Fenster „Skizze“: Werkzeuge, Abhängigkeiten, Bearbeiten, Liste der
//! Abhängigkeiten und Statuszeile. Es wird nur gezeichnet und an [`SketchSession`] weitergereicht.

use crate::sketch_state::{ConstraintAction, Item, Severity, SketchSession, Tool};
use crate::sketch_view::{constraint_name, format_number, is_angle, parse_number, shown_value};
use crate::state::AppState;
use egui::{Color32, RichText};
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintId, ConstraintKind};
use planora_sketch::model::{CurveKind, Sketch};

const TOOLS: [(Tool, &str, &str); 9] = [
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

const ACTIONS: [ConstraintAction; 16] = {
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

const ERROR: Color32 = Color32::from_rgb(230, 90, 90);
const WARNING: Color32 = Color32::from_rgb(230, 160, 60);
const OK: Color32 = Color32::from_rgb(90, 190, 110);

/// Breite des Fensters „Skizze“.
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Mirror,
    Pattern,
}

/// Zustand des Fensters zwischen den Frames (im egui-Speicher).
#[derive(Clone, Debug)]
struct PanelState {
    sketch: usize,
    editor: Option<Editor>,
    section: Option<Section>,
    pattern_dx: String,
    pattern_dy: String,
    pattern_count: String,
    /// Eigene Eingabefehler (z. B. keine Zahl).
    error: Option<String>,
}

impl PanelState {
    fn new(sketch: usize) -> Self {
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

/// Zeigt das Fenster „Skizze“, solange der Skizzen-Modus offen ist.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let Some(index) = state.sketch_session.as_ref().map(|s| s.sketch) else {
        return;
    };
    if index >= state.sketches.len() {
        return;
    }
    let mut panel = ctx
        .data(|d| d.get_temp::<PanelState>(panel_id()))
        .filter(|p| p.sketch == index)
        .unwrap_or_else(|| PanelState::new(index));
    let mut close = false;
    egui::Window::new("Skizze")
        .id(panel_id().with("window"))
        .default_pos(egui::pos2(330.0, 90.0))
        // Feste Breite: Das Zahlenfeld oder die Liste sollen die Schaltflächen nicht umbrechen.
        .min_width(PANEL_WIDTH)
        .max_width(PANEL_WIDTH)
        .resizable(false)
        .show(ctx, |ui| {
            let AppState {
                sketches,
                sketch_session,
                ..
            } = &mut *state;
            let (Some(se), Some(sk)) = (sketch_session.as_mut(), sketches.get_mut(index)) else {
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
    ui.strong(&sk.name);
    ui.add_space(2.0);

    // Werkzeuge
    ui.weak("Werkzeuge");
    ui.horizontal_wrapped(|ui| {
        for (tool, label, _) in TOOLS {
            if ui.selectable_label(se.tool == tool, label).clicked() && se.tool != tool {
                se.set_tool(tool);
            }
        }
    });
    if let Some((_, _, hint)) = TOOLS.iter().find(|(t, ..)| *t == se.tool) {
        ui.label(RichText::new(*hint).small().weak());
    }
    ui.separator();

    // Abhängigkeiten
    ui.weak("Abhängigkeiten");
    let available = se.available_actions(sk);
    ui.horizontal_wrapped(|ui| {
        for action in ACTIONS {
            let enabled = available.contains(&action);
            let label = if action.is_dimension() {
                format!("{} …", action.label())
            } else {
                action.label().to_string()
            };
            if ui
                .add_enabled(enabled, egui::Button::new(label).small())
                .on_disabled_hover_text("Passt nicht zur aktuellen Auswahl.")
                .clicked()
            {
                panel.error = None;
                if action.is_dimension() {
                    let angle = action == ConstraintAction::Angle;
                    let value = se.measure_action(sk, action).unwrap_or(0.0);
                    panel.editor = Some(Editor {
                        target: EditTarget::New(action),
                        text: editor_text(value, angle),
                        angle,
                        focus: true,
                    });
                } else {
                    let _ = se.apply(sk, action, None);
                }
            }
        }
    });
    value_editor(ui, se, sk, panel);
    if let Some(m) = panel.error.as_ref().or(se.message.as_ref()) {
        ui.colored_label(ERROR, m);
    }
    ui.separator();

    // Bearbeiten
    ui.weak("Bearbeiten");
    let curves = se
        .selection
        .iter()
        .filter(|i| matches!(i, Item::Curve(_)))
        .count();
    let axis = mirror_axis(se, sk);
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                curves > 0,
                egui::Button::new("Konstruktion umschalten").small(),
            )
            .on_disabled_hover_text("Zuerst Kurven auswählen.")
            .clicked()
        {
            se.toggle_construction(sk);
        }
        if ui
            .add_enabled(
                axis.is_some() && curves >= 2,
                egui::Button::new("Spiegeln …").small(),
            )
            .on_disabled_hover_text(
                "Kurven und zuletzt die Linie auswählen, an der gespiegelt wird.",
            )
            .clicked()
        {
            panel.section = Some(Section::Mirror);
        }
        if ui
            .add_enabled(curves > 0, egui::Button::new("Muster …").small())
            .on_disabled_hover_text("Zuerst Kurven auswählen.")
            .clicked()
        {
            panel.section = Some(Section::Pattern);
        }
        if ui
            .add_enabled(
                !se.selection.is_empty(),
                egui::Button::new("Löschen").small(),
            )
            .on_disabled_hover_text("Es ist nichts ausgewählt.")
            .clicked()
        {
            se.delete_selection(sk);
        }
    });
    sections(ui, se, sk, panel);
    ui.checkbox(&mut se.auto_constrain, "Automatisch waagerecht/senkrecht");
    ui.separator();

    // Liste der Abhängigkeiten
    ui.weak("Abhängigkeiten der Skizze");
    constraint_list(ui, se, sk, panel);
    ui.separator();

    let close = ui
        .button(RichText::new("Skizze beenden").strong())
        .clicked();

    // Statuszeile
    let (text, severity) = se.status(sk);
    let color = match severity {
        Severity::Info if se.last_report.as_ref().is_some_and(|r| r.dof == 0) => OK,
        Severity::Info => ui.visuals().text_color(),
        Severity::Warning => WARNING,
        Severity::Error => ERROR,
    };
    ui.colored_label(color, text);
    close
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
        let r = ui.add(egui::TextEdit::singleline(&mut editor.text).desired_width(70.0));
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
                ui.add(egui::TextEdit::singleline(&mut panel.pattern_dx).desired_width(50.0));
                ui.label("y");
                ui.add(egui::TextEdit::singleline(&mut panel.pattern_dy).desired_width(50.0));
                ui.label("mm");
            });
            ui.horizontal(|ui| {
                ui.label("Anzahl Kopien");
                ui.add(egui::TextEdit::singleline(&mut panel.pattern_count).desired_width(40.0));
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
                    text = text.color(ERROR);
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
