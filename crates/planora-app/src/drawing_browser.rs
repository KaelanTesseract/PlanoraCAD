//! Abschnitt „Zeichnungen“ im Browser (Teilprojekt 11, Aufgabe 16): je Zeichnung eine Zeile,
//! darunter eingerückt ihre Ansichten; Wählen, Öffnen, Umbenennen und Löschen mit Rückfrage.
//!
//! Stücklistenblätter stehen wie jede Zeichnung in der Liste; umbenennen lassen sie sich nicht,
//! ein Folgeblatt auch nicht einzeln löschen. Seite 1 löschen nimmt alle Blätter der Stückliste
//! mit (Rückfrage nennt die Zahl).
//!
//! Die Zeilen nutzen dieselbe Hilfe wie die Feature-Zeilen (`browser::row`): gleiche Höhe,
//! Schrift und Einrückung, volle Breite. Die Zustandsfunktionen (`request_*`, `apply_*`) sind
//! ohne Oberfläche testbar.

use crate::browser::{body_indent, error_color, row};
use crate::dialog::{Dialog, DialogSize};
use crate::drawing_dialog::{DUPLICATE_NAME, EMPTY_NAME, SKETCH_BLOCKED, paper_label};
use crate::drawing_state::WorkArea;
use crate::drawing_view::view_names;
use crate::icons::{paint_drawing_icon, paint_view_icon};
use crate::state::AppState;
use crate::theme;
use planora_drawing::{Drawing, DrawingId, DrawingKind, Projection, ViewId};
use planora_feature::{FOLLOW_PAGE_DELETE, PARTS_SHEET_RENAME};

/// Text bei fehlenden Zeichnungen.
pub const EMPTY_TEXT: &str =
    "Noch keine Zeichnung. Über „Zeichnung“ im Menü lässt sich eine anlegen.";
/// Hinweis an gesperrten Einträgen in einer alten Version.
pub const READ_ONLY_HINT: &str = "In einer alten Version lässt sich nichts ändern.";
/// Meldung, wenn die Zeichnung oder Ansicht inzwischen fehlt.
const GONE: &str = "Die Zeichnung oder Ansicht gibt es nicht mehr.";

/// Was „Löschen“ in der Rückfrage bedeutet: eine ganze Zeichnung oder eine Ansicht (samt der
/// von ihr abgeleiteten).
#[derive(Clone, Debug, PartialEq)]
pub struct DrawingDeletePlan {
    pub drawing: DrawingId,
    /// `None`: die ganze Zeichnung.
    pub view: Option<ViewId>,
    /// Text der Rückfrage.
    pub text: String,
}

/// Blatt und Projektion für die Zeile, z. B. „A3 quer, Erstwinkel“.
pub fn drawing_detail(d: &Drawing) -> String {
    format!(
        "{} {}, {}",
        paper_label(d.sheet.paper),
        if d.sheet.landscape { "quer" } else { "hoch" },
        match d.projection {
            Projection::First => "Erstwinkel",
            Projection::Third => "Dritter Winkel",
        }
    )
}

// ---- Zustandsfunktionen -----------------------------------------------------------------------

/// Öffnet die Zeichnung im Arbeitsbereich (Auswahl, Zoom und Verschiebung bleiben, wenn es die
/// zuletzt offene war). Im Skizzen-Modus geht es nicht: die Meldung sagt warum.
pub(crate) fn open_drawing(state: &mut AppState, id: DrawingId) -> bool {
    if state.sketch_session.is_some() {
        state.message = Some(SKETCH_BLOCKED.into());
        return false;
    }
    state.drawing_ui.show_drawing(id);
    state.drawing_ui.tick(&state.bodies, &state.document, false);
    true
}

/// Wählt die Zeichnung im Browser; Feature, Körper und 3D-Auswahl sind danach nicht mehr
/// hervorgehoben.
pub(crate) fn select_drawing(state: &mut AppState, id: DrawingId) {
    state.drawing_ui.selected_drawing = Some(id);
    state.selected_feature = None;
    state.selected_body = None;
    state.set_selection(None);
}

/// Räumt auf, nachdem es die Zeichnung `id` nicht mehr gibt: offene Rückfrage und Umbenennen
/// gehen weg, ein offener Zeichnungsbereich fällt auf das Modell zurück.
fn forget(state: &mut AppState, id: DrawingId) {
    if state.drawing_ui.area == WorkArea::Drawing(id) {
        state.drawing_ui.area = WorkArea::Model;
        state.drawing_ui.selected_view = None;
    }
    if state.drawing_ui.selected_drawing == Some(id) {
        state.drawing_ui.selected_drawing = None;
    }
    if state
        .rename_drawing_draft
        .as_ref()
        .is_some_and(|(d, _)| *d == id)
    {
        state.rename_drawing_draft = None;
    }
}

/// Seite eines Stücklistenblatts (ab 1), sonst `None`.
fn parts_page(d: &Drawing) -> Option<u32> {
    match d.kind {
        DrawingKind::PartsList { page } => Some(page),
        DrawingKind::Views => None,
    }
}

/// Rückfrage vor dem Löschen der Stückliste mit `pages` Blättern.
pub fn parts_delete_text(pages: usize) -> String {
    if pages <= 1 {
        "Stückliste löschen?".to_string()
    } else {
        format!("Stückliste mit {pages} Blättern löschen?")
    }
}

fn delete_text_drawing(state: &AppState, d: &Drawing) -> String {
    if parts_page(d).is_some() {
        return parts_delete_text(state.document.parts_sheets().len());
    }
    match d.views.len() {
        0 => format!("Zeichnung „{}“ löschen?", d.name),
        1 => format!("Zeichnung „{}“ mit 1 Ansicht löschen?", d.name),
        n => format!("Zeichnung „{}“ mit {n} Ansichten löschen?", d.name),
    }
}

/// Fragt vor dem Löschen einer Zeichnung zurück (die Rückfrage steht in
/// `pending_delete_drawing`).
pub fn request_delete_drawing(state: &mut AppState, id: DrawingId) {
    let Some(d) = state.document.drawing(id) else {
        return;
    };
    if parts_page(d).is_some_and(|p| p > 1) {
        state.message = Some(FOLLOW_PAGE_DELETE.into());
        return;
    }
    let plan = DrawingDeletePlan {
        drawing: id,
        view: None,
        text: delete_text_drawing(state, d),
    };
    state.message = None;
    state.pending_delete_drawing = Some(plan);
}

/// Plant das Löschen einer Ansicht; `None`, wenn es sie nicht gibt. Der Text nennt die
/// abgeleiteten Ansichten und die Zahl der Maße, die mitgelöscht werden; die Rückfrage
/// (`needs_confirmation`) ist nötig, sobald etwas mitgelöscht wird.
fn plan_delete_view(
    state: &AppState,
    drawing: DrawingId,
    view: ViewId,
) -> Option<(DrawingDeletePlan, bool)> {
    let d = state.document.drawing(drawing)?;
    d.view(view)?;
    let names = view_names(d);
    let removed = d.clone().remove_view(view);
    let dependents: Vec<&str> = removed
        .iter()
        .skip(1)
        .filter_map(|v| names.get(v).map(String::as_str))
        .collect();
    let dims = d
        .dimensions
        .iter()
        .filter(|m| removed.contains(&m.view))
        .count();
    let name = names.get(&view).cloned().unwrap_or_default();
    // Eine Schnittansicht reißt immer ihre Schnittlinie in der Ausgangsansicht mit: stets mit
    // Rückfrage.
    if let Some(v) = d.view(view).filter(|v| v.section.is_some()) {
        let source = v
            .parent
            .and_then(|p| names.get(&p))
            .cloned()
            .unwrap_or_default();
        let mut text = format!(
            "Die Schnittansicht „{name}“ wird gelöscht, mit ihr die Schnittlinie in „{source}“."
        );
        let mut extra = Vec::new();
        if !dependents.is_empty() {
            extra.push(format!(
                "Abgeleitete Ansichten ({}) werden mit gelöscht",
                dependents.join(", ")
            ));
        }
        if dims > 0 {
            extra.push(dims_go_along(dims));
        }
        if !extra.is_empty() {
            text.pop();
            text.push_str(&format!(", und {}.", extra.join(", und ")));
        }
        return Some((
            DrawingDeletePlan {
                drawing,
                view: Some(view),
                text,
            },
            true,
        ));
    }
    let text = match (dependents.is_empty(), dims) {
        (true, 0) => format!("Die Ansicht „{name}“ wird gelöscht."),
        (true, n) => format!(
            "Die Ansicht „{name}“ wird gelöscht, und {}.",
            dims_go_along(n)
        ),
        (false, 0) => format!(
            "Die Ansicht „{name}“ hat abgeleitete Ansichten: {}. Sie werden mit gelöscht.",
            dependents.join(", ")
        ),
        (false, n) => format!(
            "Die Ansicht „{name}“ hat abgeleitete Ansichten: {}. Sie werden mit gelöscht, und {}.",
            dependents.join(", "),
            dims_go_along(n)
        ),
    };
    Some((
        DrawingDeletePlan {
            drawing,
            view: Some(view),
            text,
        },
        !dependents.is_empty() || dims > 0,
    ))
}

/// „1 Maß wird mit gelöscht“ bzw. „n Maße werden mit gelöscht“.
fn dims_go_along(n: usize) -> String {
    if n == 1 {
        "1 Maß wird mit gelöscht".into()
    } else {
        format!("{n} Maße werden mit gelöscht")
    }
}

/// „Löschen“ bei einer Ansicht (Browser und Werkzeugleiste des Zeichnungsbereichs): ohne
/// abgeleitete Ansichten und ohne Maße sofort, sonst mit Rückfrage, die nennt, was mitgelöscht
/// wird.
pub fn request_delete_view(state: &mut AppState, drawing: DrawingId, view: ViewId) {
    let Some((plan, confirm)) = plan_delete_view(state, drawing, view) else {
        return;
    };
    state.message = None;
    if confirm {
        state.pending_delete_drawing = Some(plan);
    } else {
        let _ = apply_delete(state, &plan);
    }
}

/// Führt einen Löschplan aus und schließt die Rückfrage. Gibt es Zeichnung oder Ansicht nicht
/// mehr, passiert nichts außer dem Aufräumen. Haben sich die Folgen für eine Ansicht
/// geändert, wird nichts gelöscht und die Rückfrage zeigt den neuen Text.
pub fn apply_delete(state: &mut AppState, plan: &DrawingDeletePlan) -> Result<(), String> {
    state.pending_delete_drawing = None;
    let id = plan.drawing;
    if state.document.drawing(id).is_none() {
        forget(state, id);
        return Err(GONE.into());
    }
    match plan.view {
        None => {
            let before: Vec<DrawingId> = state.document.drawings().iter().map(|d| d.id).collect();
            state
                .document
                .delete_drawing(id)
                .map_err(|e| e.to_string())?;
            // Seite 1 der Stückliste nimmt ihre Folgeblätter mit: alle Verschwundenen aufräumen.
            for gone in before {
                if state.document.drawing(gone).is_none() {
                    forget(state, gone);
                }
            }
            Ok(())
        }
        Some(view) => {
            let Some((current, _)) = plan_delete_view(state, id, view) else {
                return Err(GONE.into());
            };
            if current.text != plan.text {
                state.pending_delete_drawing = Some(current);
                return Err("Die Folgen des Löschens haben sich geändert; bitte prüfen".into());
            }
            let mut removed = Vec::new();
            state
                .document
                .update_drawing(id, |d| removed = d.remove_view(view))
                .map_err(|e| e.to_string())?;
            // Ansichtskennungen zählen je Zeichnung: nur die Auswahl der offenen Zeichnung, aus
            // der gelöscht wurde, wird geleert.
            if state.drawing_ui.area == WorkArea::Drawing(id)
                && state
                    .drawing_ui
                    .selected_view
                    .is_some_and(|v| removed.contains(&v))
            {
                state.drawing_ui.selected_view = None;
            }
            Ok(())
        }
    }
}

/// Öffnet das Fenster „Umbenennen“ für eine Zeichnung.
pub fn request_rename(state: &mut AppState, id: DrawingId) {
    if let Some(d) = state.document.drawing(id) {
        if parts_page(d).is_some() {
            state.message = Some(PARTS_SHEET_RENAME.into());
            return;
        }
        state.rename_drawing_draft = Some((id, d.name.clone()));
    }
}

/// Warum `name` für die Zeichnung `id` nicht geht: leer oder schon vergeben (ohne Rücksicht auf
/// Groß- und Kleinschreibung; der eigene Name zählt nicht).
pub fn rename_error(state: &AppState, id: DrawingId, name: &str) -> Option<&'static str> {
    let key = name.trim().to_lowercase();
    if key.is_empty() {
        Some(EMPTY_NAME)
    } else if state
        .document
        .drawings()
        .iter()
        .any(|d| d.id != id && d.name.trim().to_lowercase() == key)
    {
        Some(DUPLICATE_NAME)
    } else {
        None
    }
}

/// Übernimmt den Namen aus dem Fenster „Umbenennen“. Bei einem ungültigen Namen bleibt der
/// Entwurf (das Fenster) offen.
pub fn apply_rename(state: &mut AppState) -> Result<(), String> {
    let Some((id, name)) = state.rename_drawing_draft.take() else {
        return Ok(());
    };
    if state.document.drawing(id).is_none() {
        return Err(GONE.into());
    }
    if let Some(e) = rename_error(state, id, &name) {
        state.rename_drawing_draft = Some((id, name));
        return Err(e.into());
    }
    match state.document.rename_drawing(id, name.trim()) {
        Ok(()) => {
            state.message = None;
            Ok(())
        }
        Err(e) => {
            state.rename_drawing_draft = Some((id, name));
            Err(e.to_string())
        }
    }
}

fn toggle_hidden(state: &mut AppState, drawing: DrawingId, view: ViewId) {
    let r = state.document.update_drawing(drawing, |d| {
        if let Some(v) = d.views.iter_mut().find(|v| v.id == view) {
            v.show_hidden = !v.show_hidden;
        }
    });
    if let Err(e) = r {
        state.message = Some(e.to_string());
    }
}

// ---- Zeichnen ---------------------------------------------------------------------------------

/// Befehl aus einer Zeile (wird nach dem Zeichnen ausgeführt).
enum Action {
    Select(DrawingId),
    Open(DrawingId),
    SelectView(DrawingId, ViewId),
    Rename(DrawingId),
    DeleteDrawing(DrawingId),
    DeleteView(DrawingId, ViewId),
    ToggleHidden(DrawingId, ViewId),
}

struct ViewRow {
    id: ViewId,
    name: String,
    scale: String,
    show_hidden: bool,
}

struct DrawingRow {
    id: DrawingId,
    name: String,
    detail: String,
    /// Seite eines Stücklistenblatts.
    parts_page: Option<u32>,
    views: Vec<ViewRow>,
}

fn rows_of(state: &AppState) -> Vec<DrawingRow> {
    state
        .document
        .drawings()
        .iter()
        .map(|d| {
            let names = view_names(d);
            DrawingRow {
                id: d.id,
                name: d.name.clone(),
                detail: drawing_detail(d),
                parts_page: parts_page(d),
                views: d
                    .views
                    .iter()
                    .map(|v| ViewRow {
                        id: v.id,
                        name: names.get(&v.id).cloned().unwrap_or_default(),
                        scale: v.scale.label(),
                        show_hidden: v.show_hidden,
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Abschnitt „Zeichnungen“ unter dem Feature-Baum samt Fenstern „Umbenennen“ und Rückfrage.
/// `read_only` (alte Version): Wählen und Öffnen gehen, Ändern ist gesperrt.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, read_only: bool) {
    theme::section_header(ui, "Zeichnungen", theme::SUB_HEADER_H);
    ui.add_space(theme::SPACE_1);
    let rows = rows_of(state);
    if rows.is_empty() {
        ui.weak(EMPTY_TEXT);
    }
    let open = match state.drawing_ui.area {
        WorkArea::Drawing(id) => Some(id),
        WorkArea::Model => None,
    };
    let mut action = None;
    for r in &rows {
        let id = r.id;
        let resp = row(
            ui,
            0.0,
            0.0,
            state.drawing_ui.selected_drawing == Some(id),
            &r.name,
            None,
            None,
            Some(&r.detail),
            paint_drawing_icon,
        )
        .on_hover_text("Doppelklick öffnet die Zeichnung; Rechtsklick zeigt weitere Befehle.");
        if resp.double_clicked() {
            action = Some(Action::Open(id));
        } else if resp.clicked() {
            action = Some(Action::Select(id));
        }
        resp.context_menu(|ui| {
            if ui.button("Öffnen").clicked() {
                action = Some(Action::Open(id));
                ui.close();
            }
            let rename_why = if read_only {
                READ_ONLY_HINT
            } else {
                PARTS_SHEET_RENAME
            };
            if ui
                .add_enabled(
                    !read_only && r.parts_page.is_none(),
                    egui::Button::new("Umbenennen"),
                )
                .on_disabled_hover_text(rename_why)
                .clicked()
            {
                action = Some(Action::Rename(id));
                ui.close();
            }
            ui.separator();
            let follow = r.parts_page.is_some_and(|p| p > 1);
            let delete_why = if read_only {
                READ_ONLY_HINT
            } else {
                FOLLOW_PAGE_DELETE
            };
            if ui
                .add_enabled(!read_only && !follow, egui::Button::new("Löschen"))
                .on_disabled_hover_text(delete_why)
                .clicked()
            {
                action = Some(Action::DeleteDrawing(id));
                ui.close();
            }
        });
        for v in &r.views {
            let vid = v.id;
            let selected = open == Some(id) && state.drawing_ui.selected_view == Some(vid);
            let resp = row(
                ui,
                body_indent(),
                0.0,
                selected,
                &v.name,
                None,
                None,
                Some(&v.scale),
                paint_view_icon,
            )
            .on_hover_text("Klick öffnet die Zeichnung und wählt die Ansicht.");
            if resp.clicked() {
                action = Some(Action::SelectView(id, vid));
            }
            resp.context_menu(|ui| {
                if ui
                    .add_enabled(!read_only, egui::Button::new("Löschen"))
                    .on_disabled_hover_text(READ_ONLY_HINT)
                    .clicked()
                {
                    action = Some(Action::DeleteView(id, vid));
                    ui.close();
                }
                let toggle = if v.show_hidden {
                    "Verdeckte Kanten ausblenden"
                } else {
                    "Verdeckte Kanten zeigen"
                };
                if ui
                    .add_enabled(!read_only, egui::Button::new(toggle))
                    .on_disabled_hover_text(READ_ONLY_HINT)
                    .clicked()
                {
                    action = Some(Action::ToggleHidden(id, vid));
                    ui.close();
                }
            });
        }
    }
    if let Some(a) = action {
        run(state, a, read_only);
    }
    rename_window(ui.ctx(), state);
    delete_window(ui.ctx(), state);
}

/// Führt einen Befehl aus einer Zeile aus. Änderungen sind in einer alten Version gesperrt
/// (die Schaltflächen sind es schon; das hier ist der zweite Riegel).
fn run(state: &mut AppState, action: Action, read_only: bool) {
    match action {
        Action::Select(id) => select_drawing(state, id),
        Action::Open(id) => {
            select_drawing(state, id);
            open_drawing(state, id);
        }
        Action::SelectView(id, view) => {
            select_drawing(state, id);
            if open_drawing(state, id) {
                state.drawing_ui.selected_view = Some(view);
            }
        }
        _ if read_only => {}
        Action::Rename(id) => request_rename(state, id),
        Action::DeleteDrawing(id) => request_delete_drawing(state, id),
        Action::DeleteView(id, view) => request_delete_view(state, id, view),
        Action::ToggleHidden(id, view) => toggle_hidden(state, id, view),
    }
}

/// Kleines Fenster „Umbenennen“ mit Textfeld und rotem Fehlertext.
fn rename_window(ctx: &egui::Context, state: &mut AppState) {
    let Some((id, mut name)) = state.rename_drawing_draft.clone() else {
        return;
    };
    if state.document.drawing(id).is_none() {
        state.rename_drawing_draft = None;
        return;
    }
    let error = rename_error(state, id, &name);
    let (mut apply, mut cancel) = (false, false);
    let mut open = true;
    Dialog::new("Umbenennen", DialogSize::Prompt)
        .id(egui::Id::new("planora-rename-drawing"))
        .open(&mut open)
        .show(ctx, |ui| {
            let r = theme::text_field(ui, egui::TextEdit::singleline(&mut name));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                apply = true;
            }
            if let Some(e) = error {
                ui.colored_label(error_color(ui.visuals().dark_mode), e);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(error.is_none(), egui::Button::new("Übernehmen"))
                    .clicked()
                {
                    apply = true;
                }
                if ui.button("Abbrechen").clicked() {
                    cancel = true;
                }
            });
        });
    if let Some(draft) = state.rename_drawing_draft.as_mut() {
        draft.1 = name;
    }
    if apply && error.is_none() {
        let _ = apply_rename(state);
    } else if cancel || !open {
        state.rename_drawing_draft = None;
    }
}

/// Rückfrage vor dem Löschen einer Zeichnung oder einer Ansicht mit abgeleiteten Ansichten oder
/// Maßen.
fn delete_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(plan) = state.pending_delete_drawing.clone() else {
        return;
    };
    let (mut confirm, mut cancel) = (false, false);
    let mut open = true;
    Dialog::new("Rückfrage", DialogSize::Prompt)
        .id(egui::Id::new("planora-delete-drawing"))
        .open(&mut open)
        .show(ctx, |ui| {
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
        let _ = apply_delete(state, &plan);
    } else if cancel || !open {
        state.pending_delete_drawing = None;
    }
}
