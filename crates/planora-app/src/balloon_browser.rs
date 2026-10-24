//! Abschnitt „Positionen“ im Browser (Teilprojekt 3c-2, Aufgabe 7): unter „Maße“ für die
//! geöffnete Zeichnung je Ansicht eine Gruppenzeile und darunter eine Zeile je Nummernkreis
//! („Pos. 2 – Platte“); Klick wählt den Kreis, das Kontextmenü löscht ihn. Ein Kreis, dessen Teil
//! es nicht mehr gibt, steht rot als „Pos. ? – Teil fehlt“ mit dem Grund als Hinweistext.

use crate::balloon_tool::{self, LOST_TIP};
use crate::browser::{body_indent, error_color, row};
use crate::drawing_browser::{READ_ONLY_HINT, open_drawing, select_drawing};
use crate::drawing_state::WorkArea;
use crate::drawing_view::view_names;
use crate::icons::{paint_balloon_icon, paint_view_icon};
use crate::state::AppState;
use crate::theme;
use planora_drawing::{BalloonId, DrawingId};

/// Text, solange die Zeichnung keinen Nummernkreis hat.
pub const EMPTY_TEXT: &str = "Noch keine Positionsnummern. Das Werkzeug „Position“ setzt welche.";
/// Überschrift des Abschnitts.
pub const SECTION: &str = "Positionen";
/// Hinweistext einer Zeile.
const ROW_TIP: &str = "Klick wählt die Positionsnummer; Rechtsklick zeigt weitere Befehle.";

/// Text einer Zeile: „Pos. 2 – Platte“; ohne Nummer (Teil fehlt) „Pos. ? – Teil fehlt“.
pub fn row_text(number: Option<u32>, name: &str) -> String {
    match number {
        Some(n) => format!("Pos. {n} – {name}"),
        None => "Pos. ? – Teil fehlt".into(),
    }
}

/// Wählt den Kreis im Zeichnungsbereich (die Zeichnung wird dabei geöffnet).
pub fn select_balloon(state: &mut AppState, drawing: DrawingId, b: BalloonId) {
    if state
        .document
        .drawing(drawing)
        .and_then(|d| d.balloon(b))
        .is_none()
    {
        return;
    }
    select_drawing(state, drawing);
    if open_drawing(state, drawing) {
        state.drawing_ui.selected_balloon = Some(b);
        state.drawing_ui.selected_view = None;
        state.drawing_ui.selected_dim = None;
    }
}

struct BalloonRow {
    id: BalloonId,
    text: String,
    lost: bool,
}

struct Group {
    name: String,
    rows: Vec<BalloonRow>,
}

fn groups_of(ctx: &egui::Context, state: &AppState, drawing: DrawingId) -> Vec<Group> {
    let Some(d) = state.document.drawing(drawing) else {
        return Vec::new();
    };
    if d.balloons.is_empty() {
        return Vec::new();
    }
    let numbers = balloon_tool::numbers_cached(ctx, state, drawing);
    let cells = crate::drawing_view::cached_cells(ctx, state);
    let names = view_names(d);
    d.views
        .iter()
        .filter_map(|v| {
            let rows: Vec<BalloonRow> = d
                .balloons
                .iter()
                .filter(|b| b.view == v.id)
                .map(|b| {
                    let n = numbers.get(&b.id).copied().flatten();
                    let name = n
                        .and_then(|n| (n as usize).checked_sub(1))
                        .and_then(|i| cells.get(i))
                        .map_or("", |c| c[1].as_str());
                    BalloonRow {
                        id: b.id,
                        text: row_text(n, name),
                        lost: n.is_none(),
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

enum Action {
    Select(BalloonId),
    Delete(BalloonId),
}

/// Abschnitt „Positionen“ für die geöffnete Zeichnung; ohne geöffnete Zeichnung erscheint
/// nichts. `read_only` (alte Version): Wählen geht, Löschen ist gesperrt.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, read_only: bool) {
    let WorkArea::Drawing(id) = state.drawing_ui.area else {
        return;
    };
    // Auf einem Stücklistenblatt gibt es keine Positionsnummern (das Werkzeug ist dort gesperrt).
    match state.document.drawing(id) {
        Some(d) if !matches!(d.kind, planora_drawing::DrawingKind::PartsList { .. }) => {}
        _ => return,
    }
    ui.add_space(theme::SPACE_3);
    theme::section_header(ui, SECTION, theme::SUB_HEADER_H);
    ui.add_space(theme::SPACE_1);
    let groups = groups_of(ui.ctx(), state, id);
    if groups.is_empty() {
        ui.weak(EMPTY_TEXT);
    }
    let mut action = None;
    for g in &groups {
        row(ui, 0.0, false, &g.name, None, None, None, paint_view_icon);
        for r in &g.rows {
            let b = r.id;
            let selected = state.drawing_ui.selected_balloon == Some(b);
            let color = r.lost.then_some(error_color(ui.visuals().dark_mode));
            let resp = row(
                ui,
                body_indent(),
                selected,
                &r.text,
                color,
                None,
                None,
                paint_balloon_icon,
            );
            let resp = resp.on_hover_text(if r.lost { LOST_TIP } else { ROW_TIP });
            if resp.clicked() {
                action = Some(Action::Select(b));
            }
            resp.context_menu(|ui| {
                if ui
                    .add_enabled(!read_only, egui::Button::new("Löschen"))
                    .on_disabled_hover_text(READ_ONLY_HINT)
                    .clicked()
                {
                    action = Some(Action::Delete(b));
                    ui.close();
                }
            });
        }
    }
    match action {
        Some(Action::Select(b)) => select_balloon(state, id, b),
        // Zweiter Riegel: in einer alten Version wird nichts gelöscht.
        Some(Action::Delete(_)) if read_only => {}
        Some(Action::Delete(b)) => {
            balloon_tool::delete(state, id, b);
        }
        None => {}
    }
}
