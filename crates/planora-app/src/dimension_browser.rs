//! Abschnitt „Maße“ im Browser (Teilprojekt 12, Aufgabe 13): unter „Zeichnungen“ für die
//! geöffnete Zeichnung je Ansicht eine Gruppenzeile und darunter eine Zeile je Maß; Wählen,
//! „Maß bearbeiten“ öffnen und Löschen (mit Rückfrage bei Maßketten).
//!
//! Die Zeilen nutzen dieselbe Hilfe wie die Feature-Zeilen (`browser::row`): gleiche Höhe,
//! Schrift und Einrückung, volle Breite. Der Text einer Zeile kommt aus demselben Code wie der
//! Text auf dem Blatt ([`planora_drawing::annotate::dimension_text`]). Ein Maß ohne Bezug zeigt
//! nie einen Wert, sondern ein rotes `?` mit dem Grund als Hinweistext.

use crate::browser::{body_indent, error_color, row};
use crate::dimension_dialog;
use crate::dimension_state::resolve_in;
use crate::drawing_browser::{READ_ONLY_HINT, open_drawing, select_drawing};
use crate::drawing_state::WorkArea;
use crate::drawing_view::view_names;
use crate::icons::{paint_dimension_icon, paint_view_icon};
use crate::state::AppState;
use planora_drawing::DrawingId;
use planora_drawing::annotate::dimension_text;
use planora_drawing::dimension::{DimError, DimId, DimKind, DimSpec, Measured};

/// Text, solange die Zeichnung kein Maß hat.
pub const EMPTY_TEXT: &str =
    "Noch keine Maße. Mit dem Werkzeug „Maß“ oder „Maße vorschlagen“ lassen sich welche anlegen.";
/// Rückfrage vor dem Löschen eines Maßes, das zu einer Kette gehört.
pub const CHAIN_ASK: &str = "Dieses Maß gehört zu einer Kette; die anderen Maße bleiben";
/// Meldung, wenn Zeichnung oder Maß inzwischen fehlen.
const GONE: &str = "Das Maß gibt es nicht mehr.";
/// Hinweis, wenn für ein Maß kein Messergebnis vorliegt.
const NO_RESULT: &str = "Das Maß lässt sich nicht berechnen.";

/// Rückfrage „Maß löschen“.
#[derive(Clone, Debug, PartialEq)]
pub struct DimDeletePlan {
    pub drawing: DrawingId,
    pub dim: DimId,
    /// Text der Rückfrage.
    pub text: String,
}

/// Text einer Zeile: Maßtext wie auf dem Blatt (Vorsatz, Wert, Toleranz; gestapelte Werte als
/// „oben / unten“), bei fehlendem Bezug nur `?`.
pub fn row_text(spec: &DimSpec, result: &Result<Measured, DimError>) -> String {
    let Ok(m) = result else {
        return "?".into();
    };
    let t = dimension_text(spec, m);
    let mut parts = Vec::new();
    if !t.main.is_empty() {
        parts.push(t.main);
    }
    if let Some((up, lo)) = t.stack {
        parts.push(format!("{up} / {lo}"));
    }
    parts.join(" ")
}

/// Grund, warum ein Maß keinen Wert hat (`None`, wenn es gemessen ist).
pub fn lost_reason(result: Option<&Result<Measured, DimError>>) -> Option<String> {
    match result {
        Some(Ok(_)) => None,
        Some(Err(e)) => Some(e.to_string()),
        None => Some(NO_RESULT.into()),
    }
}

// ---- Zustandsfunktionen -----------------------------------------------------------------------

/// Wählt das Maß im Zeichnungsbereich (die Zeichnung wird dabei geöffnet).
pub fn select_dimension(state: &mut AppState, drawing: DrawingId, dim: DimId) {
    if state
        .document
        .drawing(drawing)
        .and_then(|d| d.dimension(dim))
        .is_none()
    {
        return;
    }
    select_drawing(state, drawing);
    if open_drawing(state, drawing) {
        state.drawing_ui.selected_dim = Some(dim);
        state.drawing_ui.selected_view = None;
    }
}

/// Löscht ein Maß; bei einem Kettenglied erst nach der Rückfrage (`pending_delete_dimension`).
pub fn request_delete_dimension(
    state: &mut AppState,
    drawing: DrawingId,
    dim: DimId,
) -> Result<(), String> {
    let Some(spec) = state
        .document
        .drawing(drawing)
        .and_then(|d| d.dimension(dim))
    else {
        return Err(GONE.into());
    };
    if spec.chain.is_some() {
        state.message = None;
        state.pending_delete_dimension = Some(DimDeletePlan {
            drawing,
            dim,
            text: CHAIN_ASK.into(),
        });
        return Ok(());
    }
    apply_delete_dimension(state, drawing, dim)
}

/// Löscht das Maß wirklich, räumt Auswahl, Rückfrage und Fenster „Maß bearbeiten“ auf.
pub fn apply_delete_dimension(
    state: &mut AppState,
    drawing: DrawingId,
    dim: DimId,
) -> Result<(), String> {
    state.pending_delete_dimension = None;
    state
        .document
        .delete_dimension(drawing, dim)
        .map_err(|e| e.to_string())?;
    if state.drawing_ui.area == WorkArea::Drawing(drawing)
        && state.drawing_ui.selected_dim == Some(dim)
    {
        state.drawing_ui.selected_dim = None;
    }
    if state
        .dimension_dialog
        .as_ref()
        .is_some_and(|d| d.drawing == drawing && d.dim == dim)
    {
        state.dimension_dialog = None;
    }
    Ok(())
}

// ---- Zeichnen ---------------------------------------------------------------------------------

enum Action {
    Select(DimId),
    Edit(DimId),
    Delete(DimId),
}

struct DimRow {
    id: DimId,
    kind: DimKind,
    text: String,
    /// Grund, wenn das Maß keinen Wert hat.
    lost: Option<String>,
}

struct Group {
    name: String,
    rows: Vec<DimRow>,
}

fn groups_of(state: &mut AppState, drawing: DrawingId) -> Vec<Group> {
    let dims = resolve_in(state, drawing);
    let Some(d) = state.document.drawing(drawing) else {
        return Vec::new();
    };
    let names = view_names(d);
    d.views
        .iter()
        .filter_map(|v| {
            let entries = dims.by_view.get(&v.id);
            let rows: Vec<DimRow> = d
                .dimensions
                .iter()
                .filter(|m| m.view == v.id)
                .map(|m| {
                    let result = entries
                        .and_then(|list| list.iter().find(|(s, _)| s.id == m.id))
                        .map(|(_, r)| r);
                    DimRow {
                        id: m.id,
                        kind: m.kind,
                        text: result.map_or_else(|| "?".into(), |r| row_text(m, r)),
                        lost: lost_reason(result),
                    }
                })
                .collect();
            (!rows.is_empty()).then(|| Group {
                name: names.get(&v.id).cloned().unwrap_or_default(),
                rows,
            })
        })
        .collect()
}

/// Abschnitt „Maße“ für die geöffnete Zeichnung samt Rückfrage. Ohne geöffnete Zeichnung
/// erscheint nichts. `read_only` (alte Version): Wählen und Ansehen gehen, Ändern ist gesperrt.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, read_only: bool) {
    if let WorkArea::Drawing(id) = state.drawing_ui.area
        && state.document.drawing(id).is_some()
    {
        section(ui, state, id, read_only);
    }
    delete_window(ui.ctx(), state);
}

fn section(ui: &mut egui::Ui, state: &mut AppState, id: DrawingId, read_only: bool) {
    ui.add_space(10.0);
    ui.heading("Maße");
    ui.add_space(4.0);
    let groups = groups_of(state, id);
    if groups.is_empty() {
        ui.weak(EMPTY_TEXT);
    }
    let mut action = None;
    for g in &groups {
        row(
            ui,
            0.0,
            0.0,
            false,
            &g.name,
            None,
            None,
            None,
            paint_view_icon,
        );
        for r in &g.rows {
            let dim = r.id;
            let selected = state.drawing_ui.selected_dim == Some(dim);
            let color = r
                .lost
                .is_some()
                .then_some(error_color(ui.visuals().dark_mode));
            let resp = row(
                ui,
                body_indent(),
                0.0,
                selected,
                &r.text,
                color,
                None,
                None,
                paint_dimension_icon(r.kind),
            );
            let resp = match &r.lost {
                Some(reason) => resp.on_hover_text(reason),
                None => resp.on_hover_text(
                    "Klick wählt das Maß; Doppelklick öffnet „Maß bearbeiten“; Rechtsklick zeigt weitere Befehle.",
                ),
            };
            if resp.double_clicked() {
                action = Some(Action::Edit(dim));
            } else if resp.clicked() {
                action = Some(Action::Select(dim));
            }
            resp.context_menu(|ui| {
                if ui
                    .button(if read_only { "Ansehen" } else { "Bearbeiten" })
                    .clicked()
                {
                    action = Some(Action::Edit(dim));
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(!read_only, egui::Button::new("Löschen"))
                    .on_disabled_hover_text(READ_ONLY_HINT)
                    .clicked()
                {
                    action = Some(Action::Delete(dim));
                    ui.close();
                }
            });
        }
    }
    if let Some(a) = action {
        run(state, id, a, read_only);
    }
}

/// Führt einen Befehl aus einer Zeile aus. Ändern ist in einer alten Version gesperrt (die
/// Einträge sind es schon; das hier ist der zweite Riegel).
fn run(state: &mut AppState, drawing: DrawingId, action: Action, read_only: bool) {
    match action {
        Action::Select(dim) => select_dimension(state, drawing, dim),
        Action::Edit(dim) => {
            select_dimension(state, drawing, dim);
            dimension_dialog::open(state, drawing, dim, read_only);
        }
        Action::Delete(_) if read_only => {}
        Action::Delete(dim) => {
            if let Err(e) = request_delete_dimension(state, drawing, dim) {
                state.message = Some(e);
            }
        }
    }
}

/// Rückfrage vor dem Löschen eines Kettenglieds.
fn delete_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(plan) = state.pending_delete_dimension.clone() else {
        return;
    };
    // Gibt es Zeichnung oder Maß nicht mehr, ist die Rückfrage hinfällig.
    if state
        .document
        .drawing(plan.drawing)
        .and_then(|d| d.dimension(plan.dim))
        .is_none()
    {
        state.pending_delete_dimension = None;
        return;
    }
    let (mut confirm, mut cancel) = (false, false);
    let mut open = true;
    egui::Window::new("Rückfrage")
        .id(egui::Id::new("planora-delete-dimension"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.label(&plan.text);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button(
                        egui::RichText::new("Löschen").color(error_color(ui.visuals().dark_mode)),
                    )
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button("Abbrechen").clicked() {
                    cancel = true;
                }
            });
        });
    if confirm {
        if let Err(e) = apply_delete_dimension(state, plan.drawing, plan.dim) {
            state.message = Some(e);
        }
    } else if cancel || !open {
        state.pending_delete_dimension = None;
    }
}
