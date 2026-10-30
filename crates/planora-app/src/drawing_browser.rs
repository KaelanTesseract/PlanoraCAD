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
use crate::drawing_dialog::{duplicate_name, empty_name, paper_label, sketch_blocked};
use crate::drawing_state::WorkArea;
use crate::drawing_view::{view_name_msgs, view_names};
use crate::icons::{paint_drawing_icon, paint_view_icon};
use crate::msg::{Msg, MsgArg};
use crate::state::AppState;
use crate::theme;
use planora_drawing::{Drawing, DrawingId, DrawingKind, Projection, ViewId};
use planora_i18n::tr;

/// Text bei fehlenden Zeichnungen.
pub fn empty_text() -> String {
    tr!("drawing-empty")
}
/// Hinweis an gesperrten Einträgen in einer alten Version.
pub fn read_only_hint() -> String {
    tr!("drawing-read-only")
}
/// Meldung, wenn die Zeichnung oder Ansicht inzwischen fehlt.
fn gone() -> Msg {
    crate::msg!("drawing-gone-view")
}
/// Hinweis, dass ein Folgeblatt der Stückliste nicht einzeln gelöscht wird.
pub fn follow_page_delete() -> Msg {
    crate::msg!("drawing-follow-page-delete")
}
/// Hinweis, dass sich Stücklistenblätter nicht umbenennen lassen.
pub fn parts_sheet_rename() -> Msg {
    crate::msg!("drawing-parts-sheet-rename")
}

/// Was „Löschen“ in der Rückfrage bedeutet: eine ganze Zeichnung oder eine Ansicht (samt der
/// von ihr abgeleiteten).
#[derive(Clone, Debug, PartialEq)]
pub struct DrawingDeletePlan {
    pub drawing: DrawingId,
    /// `None`: die ganze Zeichnung.
    pub view: Option<ViewId>,
    /// Text der Rückfrage (übersetzt erst beim Zeichnen).
    pub text: Msg,
}

/// Blatt und Projektion für die Zeile, z. B. „A3 quer, Erstwinkel“.
pub fn drawing_detail(d: &Drawing) -> String {
    tr!(
        "drawing-detail",
        paper = paper_label(d.sheet.paper),
        orientation = if d.sheet.landscape {
            tr!("drawing-landscape")
        } else {
            tr!("drawing-portrait")
        },
        projection = match d.projection {
            Projection::First => tr!("drawing-angle-first"),
            Projection::Third => tr!("drawing-angle-third"),
        }
    )
}

// ---- Zustandsfunktionen -----------------------------------------------------------------------

/// Öffnet die Zeichnung im Arbeitsbereich (Auswahl, Zoom und Verschiebung bleiben, wenn es die
/// zuletzt offene war). Im Skizzen-Modus geht es nicht: die Meldung sagt warum.
pub(crate) fn open_drawing(state: &mut AppState, id: DrawingId) -> bool {
    if state.sketch_session.is_some() {
        state.message = Some(sketch_blocked().text());
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
pub fn parts_delete_text(pages: usize) -> Msg {
    if pages <= 1 {
        crate::msg!("drawing-del-parts")
    } else {
        crate::msg!("drawing-del-parts-pages", n = pages)
    }
}

fn delete_text_drawing(state: &AppState, d: &Drawing) -> Msg {
    if parts_page(d).is_some() {
        return parts_delete_text(state.document.parts_sheets().len());
    }
    crate::msg!(
        "drawing-del-ask",
        name = d.name.as_str(),
        views = d.views.len()
    )
}

/// Fragt vor dem Löschen einer Zeichnung zurück (die Rückfrage steht in
/// `pending_delete_drawing`).
pub fn request_delete_drawing(state: &mut AppState, id: DrawingId) {
    let Some(d) = state.document.drawing(id) else {
        return;
    };
    if parts_page(d).is_some_and(|p| p > 1) {
        state.message = Some(follow_page_delete().text());
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
    let names = view_name_msgs(d);
    let removed = d.clone().remove_view(view);
    let dependents: Vec<MsgArg> = removed
        .iter()
        .skip(1)
        .filter_map(|v| names.get(v).cloned().map(MsgArg::from))
        .collect();
    let dims = d
        .dimensions
        .iter()
        .filter(|m| removed.contains(&m.view))
        .count();
    let name = MsgArg::from(names.get(&view).cloned().unwrap_or(Msg::Raw(String::new())));
    // Eine Schnittansicht reißt immer ihre Schnittlinie in der Ausgangsansicht mit: stets mit
    // Rückfrage.
    if let Some(v) = d.view(view).filter(|v| v.section.is_some()) {
        let source = MsgArg::from(
            v.parent
                .and_then(|p| names.get(&p))
                .cloned()
                .unwrap_or(Msg::Raw(String::new())),
        );
        // „Mit ihr verschwindet die Schnittlinie in „Vorne“.“ bzw. „… verschwinden die
        // Schnittlinie in „Vorne“, die abgeleiteten Ansichten (…) und 2 Maße.“
        let line = crate::msg!("drawing-del-section-line", source = source);
        let derived = (!dependents.is_empty()).then(|| {
            crate::msg!(
                "drawing-del-derived-views",
                names = MsgArg::List(dependents.clone())
            )
        });
        let dim_item = (dims > 0).then(|| crate::msg!("drawing-del-dims", n = dims));
        let (count, list) = match (derived, dim_item) {
            (None, None) => (1usize, crate::msg!("drawing-del-list-1", a = line)),
            (Some(b), None) | (None, Some(b)) => {
                (2, crate::msg!("drawing-del-list-2", a = line, b = b))
            }
            (Some(b), Some(c)) => (3, crate::msg!("drawing-del-list-3", a = line, b = b, c = c)),
        };
        let text = crate::msg!(
            "drawing-del-section",
            name = name,
            count = count,
            list = list
        );
        return Some((
            DrawingDeletePlan {
                drawing,
                view: Some(view),
                text,
            },
            true,
        ));
    }
    let list = MsgArg::List(dependents.clone());
    let text = match (dependents.is_empty(), dims) {
        (true, 0) => crate::msg!("drawing-del-view", name = name),
        (true, n) => crate::msg!(
            "drawing-del-view-dims",
            name = name,
            dims = dims_go_along(n)
        ),
        (false, 0) => crate::msg!("drawing-del-view-derived", name = name, names = list),
        (false, n) => crate::msg!(
            "drawing-del-view-derived-dims",
            name = name,
            names = list,
            dims = dims_go_along(n)
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
fn dims_go_along(n: usize) -> Msg {
    crate::msg!("drawing-del-dims-along", n = n)
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
pub fn apply_delete(state: &mut AppState, plan: &DrawingDeletePlan) -> Result<(), Msg> {
    state.pending_delete_drawing = None;
    let id = plan.drawing;
    if state.document.drawing(id).is_none() {
        forget(state, id);
        return Err(gone());
    }
    match plan.view {
        None => {
            let before: Vec<DrawingId> = state.document.drawings().iter().map(|d| d.id).collect();
            state
                .document
                .delete_drawing(id)
                .map_err(|e| Msg::Raw(e.to_string()))?;
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
                return Err(gone());
            };
            if current.text != plan.text {
                state.pending_delete_drawing = Some(current);
                return Err(crate::msg!("drawing-del-changed"));
            }
            let mut removed = Vec::new();
            state
                .document
                .update_drawing(id, |d| removed = d.remove_view(view))
                .map_err(|e| Msg::Raw(e.to_string()))?;
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
            state.message = Some(parts_sheet_rename().text());
            return;
        }
        state.rename_drawing_draft = Some((id, d.name.clone()));
    }
}

/// Warum `name` für die Zeichnung `id` nicht geht: leer oder schon vergeben (ohne Rücksicht auf
/// Groß- und Kleinschreibung; der eigene Name zählt nicht).
pub fn rename_error(state: &AppState, id: DrawingId, name: &str) -> Option<Msg> {
    let key = name.trim().to_lowercase();
    if key.is_empty() {
        Some(empty_name())
    } else if state
        .document
        .drawings()
        .iter()
        .any(|d| d.id != id && d.name.trim().to_lowercase() == key)
    {
        Some(duplicate_name())
    } else {
        None
    }
}

/// Übernimmt den Namen aus dem Fenster „Umbenennen“. Bei einem ungültigen Namen bleibt der
/// Entwurf (das Fenster) offen.
pub fn apply_rename(state: &mut AppState) -> Result<(), Msg> {
    let Some((id, name)) = state.rename_drawing_draft.take() else {
        return Ok(());
    };
    if state.document.drawing(id).is_none() {
        return Err(gone());
    }
    if let Some(e) = rename_error(state, id, &name) {
        state.rename_drawing_draft = Some((id, name));
        return Err(e);
    }
    match state.document.rename_drawing(id, name.trim()) {
        Ok(()) => {
            state.message = None;
            Ok(())
        }
        Err(e) => {
            state.rename_drawing_draft = Some((id, name));
            Err(Msg::Raw(e.to_string()))
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

/// Inhalt des Ordners „Zeichnungen“ im Feature-Baum (den Ordnerkopf zeichnet
/// [`crate::browser`], die Fenster [`windows`]). `read_only` (alte Version): Wählen und Öffnen
/// gehen, Ändern ist gesperrt.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, read_only: bool) {
    let rows = rows_of(state);
    if rows.is_empty() {
        ui.weak(empty_text());
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
            state.drawing_ui.selected_drawing == Some(id),
            &r.name,
            None,
            None,
            Some(&r.detail),
            paint_drawing_icon,
        )
        .on_hover_text(tr!("drawing-row-tip"));
        if resp.double_clicked() {
            action = Some(Action::Open(id));
        } else if resp.clicked() {
            action = Some(Action::Select(id));
        }
        resp.context_menu(|ui| {
            if ui.button(tr!("chooser-open")).clicked() {
                action = Some(Action::Open(id));
                ui.close();
            }
            let rename_why = if read_only {
                read_only_hint()
            } else {
                parts_sheet_rename().text()
            };
            if ui
                .add_enabled(
                    !read_only && r.parts_page.is_none(),
                    egui::Button::new(tr!("tree-rename")),
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
                read_only_hint()
            } else {
                follow_page_delete().text()
            };
            if ui
                .add_enabled(
                    !read_only && !follow,
                    egui::Button::new(tr!("action-delete")),
                )
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
                selected,
                &v.name,
                None,
                None,
                Some(&v.scale),
                paint_view_icon,
            )
            .on_hover_text(tr!("drawing-view-row-tip"));
            if resp.clicked() {
                action = Some(Action::SelectView(id, vid));
            }
            resp.context_menu(|ui| {
                if ui
                    .add_enabled(!read_only, egui::Button::new(tr!("action-delete")))
                    .on_disabled_hover_text(read_only_hint())
                    .clicked()
                {
                    action = Some(Action::DeleteView(id, vid));
                    ui.close();
                }
                let toggle = if v.show_hidden {
                    tr!("drawing-hide-hidden")
                } else {
                    tr!("drawing-show-hidden")
                };
                if ui
                    .add_enabled(!read_only, egui::Button::new(toggle))
                    .on_disabled_hover_text(read_only_hint())
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
}

/// Fenster „Umbenennen“ und Rückfrage beim Löschen; jedes Bild, auch bei zugeklapptem Ordner
/// (die Rückfrage kommt auch aus der Zeichnungsansicht).
pub(crate) fn windows(ctx: &egui::Context, state: &mut AppState) {
    rename_window(ctx, state);
    delete_window(ctx, state);
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
    let error = rename_error(state, id, &name).map(|m| m.text());
    let (mut apply, mut cancel) = (false, false);
    let mut open = true;
    Dialog::new(tr!("tree-rename"), DialogSize::Prompt)
        .id(egui::Id::new("planora-rename-drawing"))
        .open(&mut open)
        .show(ctx, |ui| {
            let r = theme::text_field(ui, egui::TextEdit::singleline(&mut name));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                apply = true;
            }
            if let Some(e) = &error {
                ui.colored_label(error_color(ui.visuals().dark_mode), e);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(error.is_none(), egui::Button::new(tr!("tree-apply")))
                    .clicked()
                {
                    apply = true;
                }
                if ui.button(tr!("common-cancel")).clicked() {
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
    Dialog::new(tr!("tree-confirm-title"), DialogSize::Prompt)
        .id(egui::Id::new("planora-delete-drawing"))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(plan.text.text());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button(
                        egui::RichText::new(tr!("action-delete"))
                            .color(error_color(ui.visuals().dark_mode)),
                    )
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button(tr!("common-cancel")).clicked() {
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
