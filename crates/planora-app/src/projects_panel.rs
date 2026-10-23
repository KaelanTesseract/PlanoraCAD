//! Panel „Projekte“: Baum Projekt → Ordner → Dokument.
//!
//! Die Aktionen (`open_node`, `create_project`, …) lassen sich auch ohne Oberfläche aufrufen.
//! Fehler landen als `Prompt::Notice`, nie als Panik. Pfade kommen nur aus dem Baum der
//! Bibliothek und werden von ihr geprüft.

use crate::browser::error_color;
use crate::dialog::{Dialog, DialogSize};
use crate::file_chooser::display_path;
use crate::theme;
use crate::workspace::{Prompt, Workspace};
use planora_project::ProjectError;
use planora_project::config::Config;
use planora_project::docstore::LockMode;
use planora_project::library::{Library, NodeKind, NodePath, TreeNode};
use std::path::Path;

/// Hinweis ohne gewählte Wurzel.
const NO_LIBRARY: &str = "Es ist noch kein Projektordner gewählt. Gib unten einen Ordner ein, in dem deine Projekte liegen sollen.";
/// Ablehnung, solange betroffene Dokumente in Tabs offen sind.
const CLOSE_FIRST: &str = "Bitte zuerst die betroffenen Dokumente schließen.";
/// Meldung, wenn eine Aktion keine Bibliothek hat.
const NO_ROOT: &str = "Es ist noch kein Projektordner gewählt.";

#[derive(Default)]
pub struct ProjectsUi {
    /// Zwischengespeicherter Baum (oder der Lesefehler); `None` = beim nächsten Zeichnen lesen.
    tree: Option<Result<Vec<TreeNode>, String>>,
    selected: Option<NodePath>,
    /// Eingabe „Neues Projekt“ (offen, wenn `Some`).
    new_project: Option<String>,
    /// Fenster „Neuer Ordner“: Ort und Name.
    new_folder: Option<(NodePath, String)>,
    /// Fenster „Umbenennen“: Knoten und neuer Name.
    rename: Option<(NodePath, String)>,
    /// Fenster „Verschieben“: Knoten und gewähltes Ziel.
    moving: Option<(NodePath, Option<NodePath>)>,
    /// Rückfrage „Löschen“.
    confirm_delete: Option<NodePath>,
    /// Eingabe „Wurzel wählen“ (offen, wenn `Some`).
    root_text: Option<String>,
    /// Letzte gesehene `Workspace::library_revision`.
    seen_revision: u64,
}

impl ProjectsUi {
    /// Der Baum wird beim nächsten Zeichnen neu gelesen.
    pub fn invalidate(&mut self) {
        self.tree = None;
    }

    pub fn selected(&self) -> Option<&NodePath> {
        self.selected.as_ref()
    }
}

fn notice(ws: &mut Workspace, msg: impl Into<String>) {
    ws.prompt = Some(Prompt::Notice(msg.into()));
}

/// Liegt `p` in `prefix` (oder ist es)? Ohne Unterscheidung der Groß-/Kleinschreibung, damit
/// auf Windows kein offenes Dokument übersehen wird (im Zweifel wird eher abgelehnt).
fn is_under(p: &NodePath, prefix: &NodePath) -> bool {
    p.len() >= prefix.len()
        && p.iter()
            .zip(prefix)
            .all(|(a, b)| a.to_lowercase() == b.to_lowercase())
}

/// Ist ein Dokument in oder unter `path` in einem Tab offen (auch als Ansicht)?
fn open_tabs_under(ws: &Workspace, path: &NodePath) -> bool {
    ws.tabs
        .iter()
        .any(|t| t.path.as_ref().is_some_and(|p| is_under(p, path)))
}

/// Prüfung vor Umbenennen, Verschieben und Löschen; `false` = abgelehnt (Hinweis steht).
fn may_change(ws: &mut Workspace, path: &NodePath) -> bool {
    if path.is_empty() {
        notice(ws, "Nichts ausgewählt.");
        false
    } else if open_tabs_under(ws, path) {
        notice(ws, CLOSE_FIRST);
        false
    } else {
        true
    }
}

/// Öffnet ein Dokument in einem Tab. Ist es gesperrt, fragt `Prompt::TakeOver` nach.
pub fn open_node(ws: &mut Workspace, path: &NodePath) {
    match ws.open_document(path, LockMode::Acquire) {
        Ok(r) => {
            if !r.notes.is_empty() {
                let text = r.notes.join("\n");
                if r.autosave_offered {
                    // Das Angebot hat Vorrang; die Hinweise stehen dann in der Statuszeile
                    // des Tabs (wie nach dem Übernehmen einer Sperre).
                    ws.tabs[r.tab].state.message = Some(text);
                } else {
                    notice(ws, text);
                }
            }
        }
        Err(ProjectError::Locked { .. }) => {
            ws.prompt = Some(Prompt::TakeOver {
                path: path.clone(),
                version: None,
            })
        }
        Err(e) => notice(ws, e.to_string()),
    }
}

fn lib(ws: &mut Workspace) -> Option<&Library> {
    if ws.library.is_none() {
        notice(ws, NO_ROOT);
    }
    ws.library.as_ref()
}

/// Ergebnis einer Bibliotheksaktion: Baum ungültig machen, Fehler als Hinweis.
fn finish<T>(ws: &mut Workspace, st: &mut ProjectsUi, r: planora_project::Result<T>) -> Option<T> {
    st.invalidate();
    r.map_err(|e| notice(ws, e.to_string())).ok()
}

pub fn create_project(ws: &mut Workspace, st: &mut ProjectsUi, name: &str) -> Option<NodePath> {
    let r = lib(ws)?.create_project(name);
    finish(ws, st, r)
}

pub fn create_folder(
    ws: &mut Workspace,
    st: &mut ProjectsUi,
    parent: &NodePath,
    name: &str,
) -> Option<NodePath> {
    let r = lib(ws)?.create_folder(parent, name);
    finish(ws, st, r)
}

pub fn rename_node(
    ws: &mut Workspace,
    st: &mut ProjectsUi,
    path: &NodePath,
    name: &str,
) -> Option<NodePath> {
    if !may_change(ws, path) {
        return None;
    }
    let r = lib(ws)?.rename(path, name);
    let to = finish(ws, st, r)?;
    if st.selected.as_ref() == Some(path) {
        st.selected = Some(to.clone());
    }
    Some(to)
}

pub fn move_node(
    ws: &mut Workspace,
    st: &mut ProjectsUi,
    path: &NodePath,
    to: &NodePath,
) -> Option<NodePath> {
    if !may_change(ws, path) {
        return None;
    }
    let r = lib(ws)?.move_node(path, to);
    let new = finish(ws, st, r)?;
    if st.selected.as_ref() == Some(path) {
        st.selected = Some(new.clone());
    }
    Some(new)
}

/// Verschiebt den Knoten in den Papierkorb der Wurzel.
pub fn delete_node(ws: &mut Workspace, st: &mut ProjectsUi, path: &NodePath) -> bool {
    if !may_change(ws, path) {
        return false;
    }
    let Some(l) = lib(ws) else { return false };
    let r = l.delete(path);
    let ok = finish(ws, st, r).is_some();
    if ok && st.selected.as_ref().is_some_and(|s| is_under(s, path)) {
        st.selected = None;
    }
    ok
}

// ---- Oberfläche ------------------------------------------------------------------------

/// Wunsch aus dem Baum, ausgeführt nach dem Zeichnen (der Baum ist dabei geliehen).
enum Action {
    Select(NodePath),
    Open(NodePath),
    NewFolder(NodePath),
    Rename(NodePath, String),
    Move(NodePath),
    Delete(NodePath),
}

/// Zeichnet das Panel. Der Baum wird nur gelesen, wenn er ungültig ist (nach einer Aktion oder
/// „Aktualisieren“); solange eine Rückfrage offen ist, ist das Panel gesperrt.
pub fn show(ui: &mut egui::Ui, ws: &mut Workspace, st: &mut ProjectsUi) {
    let enabled = ws.prompt.is_none();
    ui.add_enabled_ui(enabled, |ui| show_enabled(ui, ws, st));
}

fn show_enabled(ui: &mut egui::Ui, ws: &mut Workspace, st: &mut ProjectsUi) {
    if st.seen_revision != ws.library_revision {
        st.seen_revision = ws.library_revision;
        st.invalidate();
    }
    let Some(root) = ws.library.as_ref().map(|l| l.root().path().to_path_buf()) else {
        ui.label(NO_LIBRARY);
        ui.add_space(4.0);
        let text = st.root_text.get_or_insert_with(|| {
            ws.config
                .root
                .clone()
                .unwrap_or_else(Config::default_root)
                .display()
                .to_string()
        });
        if root_field(ui, ws, text, false) != RootField::Open {
            st.root_text = None;
            st.selected = None;
            st.invalidate();
        }
        return;
    };

    ui.small(display_path(&root));
    ui.horizontal_wrapped(|ui| {
        if ui.button("Neues Projekt").clicked() {
            st.new_project = Some(String::new());
        }
        if ui.button("Aktualisieren").clicked() {
            st.invalidate();
        }
        if ui.button("Wurzel wählen …").clicked() {
            st.root_text = Some(display_path(&root));
        }
    });
    if let Some(name) = &mut st.new_project {
        let mut done = false;
        let mut create = false;
        // Umbrechende Zeile wie bei der Wurzel: das Feld nimmt die freie Breite der Karte, die
        // Knöpfe rücken in die nächste Zeile (in einer festen Zeile ragten sie aus der Karte
        // und machten den Inhalt darunter breiter als die Karte).
        ui.horizontal_wrapped(|ui| {
            let r = theme::text_field(
                ui,
                egui::TextEdit::singleline(name)
                    .hint_text("Projektname")
                    .desired_width(ui.available_width()),
            );
            if ui.button("Anlegen").clicked()
                || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                create = true;
            }
            if ui.button("Abbrechen").clicked() {
                done = true;
            }
        });
        if create {
            let name = name.clone();
            if let Some(p) = create_project(ws, st, &name) {
                st.selected = Some(p);
                done = true;
            }
        }
        if done {
            st.new_project = None;
        }
    }
    if let Some(mut text) = st.root_text.take() {
        match root_field(ui, ws, &mut text, true) {
            RootField::Open => st.root_text = Some(text),
            RootField::Cancelled => {}
            RootField::Chosen => {
                st.selected = None;
                st.invalidate();
            }
        }
    }
    ui.separator();

    if st.tree.is_none() {
        st.tree = ws
            .library
            .as_ref()
            .map(|l| l.tree().map_err(|e| e.to_string()));
    }
    let mut actions = Vec::new();
    match &st.tree {
        Some(Ok(nodes)) if nodes.is_empty() => {
            ui.weak("Noch keine Projekte. Lege mit „Neues Projekt“ das erste an.");
        }
        Some(Ok(nodes)) => {
            for n in nodes {
                draw_node(ui, ws, st.selected.as_ref(), n, &mut actions);
            }
        }
        Some(Err(e)) => {
            ui.colored_label(
                error_color(ui.visuals().dark_mode),
                format!("Der Projektordner lässt sich nicht lesen: {e}"),
            );
        }
        None => {}
    }
    for a in actions {
        match a {
            Action::Select(p) => st.selected = Some(p),
            Action::Open(p) => {
                st.selected = Some(p.clone());
                open_node(ws, &p);
            }
            Action::NewFolder(p) => st.new_folder = Some((p, String::new())),
            Action::Rename(p, name) => st.rename = Some((p, name)),
            Action::Move(p) => st.moving = Some((p, None)),
            Action::Delete(p) => st.confirm_delete = Some(p),
        }
    }
    // die Fenster sind eigene Ebenen: nur zeichnen, solange keine Rückfrage des Programms offen ist
    if ws.prompt.is_none() {
        dialogs(ui.ctx(), ws, st);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootField {
    Open,
    Chosen,
    Cancelled,
}

/// Eingabezeile für die Wurzel. Bei Erfolg wird sie gemerkt (Konfiguration); ein Fehler
/// erscheint als Hinweis. Solange gespeicherte Dokumente offen sind, wird die Wurzel nicht
/// gewechselt: ihre Pfade gehören zur bisherigen Wurzel.
fn root_field(
    ui: &mut egui::Ui,
    ws: &mut Workspace,
    text: &mut String,
    cancellable: bool,
) -> RootField {
    let mut out = RootField::Open;
    ui.horizontal_wrapped(|ui| {
        theme::text_field(
            ui,
            egui::TextEdit::singleline(text).hint_text("Projektordner"),
        );
        if ui.button("Übernehmen").clicked() {
            let t = text.trim();
            if t.is_empty() {
                notice(ws, "Bitte einen Ordner angeben.");
            } else {
                match ws.choose_root(Path::new(t)) {
                    Ok(()) => {
                        ws.persist_config();
                        out = RootField::Chosen;
                    }
                    Err(e) => notice(ws, e),
                }
            }
        }
        if cancellable && ui.button("Abbrechen").clicked() {
            out = RootField::Cancelled;
        }
    });
    out
}

fn draw_node(
    ui: &mut egui::Ui,
    ws: &Workspace,
    selected: Option<&NodePath>,
    node: &TreeNode,
    actions: &mut Vec<Action>,
) {
    let is_selected = selected == Some(&node.path);
    match node.kind {
        NodeKind::Document => {
            let mut text = egui::RichText::new(&node.name);
            if ws.tab_for_path(&node.path).is_some() {
                text = text.strong();
            }
            let r = ui.selectable_label(is_selected, text);
            if r.double_clicked() {
                actions.push(Action::Open(node.path.clone()));
            } else if r.clicked() {
                actions.push(Action::Select(node.path.clone()));
            }
            r.context_menu(|ui| {
                if ui.button("Öffnen").clicked() {
                    actions.push(Action::Open(node.path.clone()));
                    ui.close();
                }
                common_menu(ui, node, actions);
            });
        }
        NodeKind::Project | NodeKind::Folder => {
            let mut text = egui::RichText::new(&node.name);
            if is_selected {
                text = text.color(ui.visuals().selection.stroke.color);
            }
            if node.kind == NodeKind::Project {
                text = text.strong();
            }
            let r = egui::CollapsingHeader::new(text)
                .id_salt(("planora_projects", &node.path))
                .default_open(true)
                .show(ui, |ui| {
                    for c in &node.children {
                        draw_node(ui, ws, selected, c, actions);
                    }
                    if node.children.is_empty() {
                        ui.weak("leer");
                    }
                });
            let h = r.header_response;
            if h.clicked() {
                actions.push(Action::Select(node.path.clone()));
            }
            h.context_menu(|ui| {
                if ui.button("Neuer Ordner …").clicked() {
                    actions.push(Action::NewFolder(node.path.clone()));
                    ui.close();
                }
                common_menu(ui, node, actions);
            });
        }
    }
}

/// Umbenennen, Verschieben (nicht für Projekte) und Löschen.
fn common_menu(ui: &mut egui::Ui, node: &TreeNode, actions: &mut Vec<Action>) {
    if ui.button("Umbenennen …").clicked() {
        actions.push(Action::Rename(node.path.clone(), node.name.clone()));
        ui.close();
    }
    if node.kind != NodeKind::Project && ui.button("Verschieben …").clicked() {
        actions.push(Action::Move(node.path.clone()));
        ui.close();
    }
    if ui.button("Löschen …").clicked() {
        actions.push(Action::Delete(node.path.clone()));
        ui.close();
    }
}

/// Alle Projekte und Ordner außer `except` und seinen Nachfahren (Ziele zum Verschieben).
fn containers(nodes: &[TreeNode], except: &NodePath, out: &mut Vec<NodePath>) {
    for n in nodes {
        if n.kind == NodeKind::Document || is_under(&n.path, except) {
            continue;
        }
        out.push(n.path.clone());
        containers(&n.children, except, out);
    }
}

fn label(path: &NodePath) -> String {
    path.join(" / ")
}

fn dialog_window(title: &str, key: &str) -> Dialog<'static> {
    Dialog::new(title, DialogSize::Prompt)
        .id(egui::Id::new(("planora_projects_dialog", key.to_string())))
        .fixed()
}

/// Öffnet das Fenster „Neuer Ordner“ in `parent` mit dem Namen `name` (für Tests).
#[doc(hidden)]
pub fn request_new_folder(st: &mut ProjectsUi, parent: NodePath, name: &str) {
    st.new_folder = Some((parent, name.to_string()));
}

/// Öffnet das Fenster „Umbenennen“ für `path` (für Tests).
#[doc(hidden)]
pub fn request_rename(st: &mut ProjectsUi, path: NodePath, name: &str) {
    st.rename = Some((path, name.to_string()));
}

/// Öffnet das Fenster „Verschieben“ für `path` (für Tests).
#[doc(hidden)]
pub fn request_move(st: &mut ProjectsUi, path: NodePath) {
    st.moving = Some((path, None));
}

/// Öffnet die Rückfrage „Löschen“ für `path` (für Tests).
#[doc(hidden)]
pub fn request_delete(st: &mut ProjectsUi, path: NodePath) {
    st.confirm_delete = Some(path);
}

/// Ob eines der vier Fenster offen ist (für Tests).
#[doc(hidden)]
pub fn dialog_open(st: &ProjectsUi) -> bool {
    st.new_folder.is_some()
        || st.rename.is_some()
        || st.moving.is_some()
        || st.confirm_delete.is_some()
}

/// Textfeld eines Fensters; `true`, wenn darin Enter gedrückt wurde.
fn name_field(ui: &mut egui::Ui, name: &mut String) -> bool {
    let r = theme::text_field(ui, egui::TextEdit::singleline(name));
    r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

/// Fenster „Neuer Ordner“, „Umbenennen“, „Verschieben“ und „Löschen“.
fn dialogs(ctx: &egui::Context, ws: &mut Workspace, st: &mut ProjectsUi) {
    if let Some((parent, mut name)) = st.new_folder.take() {
        let (mut ok, mut cancel) = (false, false);
        dialog_window("Neuer Ordner", "folder").show(ctx, |ui| {
            ui.label(format!("In „{}“:", label(&parent)));
            let enter = name_field(ui, &mut name);
            ui.horizontal(|ui| {
                ok = ui.button("Anlegen").clicked() || enter;
                cancel = ui.button("Abbrechen").clicked();
            });
        });
        let done = cancel
            || (ok
                && create_folder(ws, st, &parent, &name)
                    .inspect(|p| st.selected = Some(p.clone()))
                    .is_some());
        if !done {
            st.new_folder = Some((parent, name));
        }
    }
    if let Some((path, mut name)) = st.rename.take() {
        let (mut ok, mut cancel) = (false, false);
        dialog_window("Umbenennen", "rename").show(ctx, |ui| {
            ui.label(format!("Neuer Name für „{}“:", label(&path)));
            let enter = name_field(ui, &mut name);
            ui.horizontal(|ui| {
                ok = ui.button("Umbenennen").clicked() || enter;
                cancel = ui.button("Abbrechen").clicked();
            });
        });
        let done = cancel || (ok && rename_node(ws, st, &path, &name).is_some());
        if !done {
            st.rename = Some((path, name));
        }
    }
    if let Some((path, mut target)) = st.moving.take() {
        let mut targets = Vec::new();
        if let Some(Ok(nodes)) = &st.tree {
            containers(nodes, &path, &mut targets);
        }
        let (mut ok, mut cancel) = (false, false);
        dialog_window("Verschieben", "move").show(ctx, |ui| {
            ui.label(format!("„{}“ verschieben nach:", label(&path)));
            egui::ComboBox::from_id_salt("planora_projects_move_target")
                .selected_text(
                    target
                        .as_ref()
                        .map(label)
                        .unwrap_or_else(|| "Ziel wählen".into()),
                )
                .show_ui(ui, |ui| {
                    for t in &targets {
                        ui.selectable_value(&mut target, Some(t.clone()), label(t));
                    }
                });
            ui.horizontal(|ui| {
                ok = ui
                    .add_enabled(target.is_some(), egui::Button::new("Verschieben"))
                    .clicked();
                cancel = ui.button("Abbrechen").clicked();
            });
        });
        let done = cancel
            || (ok
                && target
                    .as_ref()
                    .is_some_and(|to| move_node(ws, st, &path, to).is_some()));
        if !done {
            st.moving = Some((path, target));
        }
    }
    if let Some(path) = st.confirm_delete.take() {
        let (mut ok, mut cancel) = (false, false);
        dialog_window("Löschen", "delete").show(ctx, |ui| {
            ui.label(format!("„{}“ in den Papierkorb verschieben?", label(&path)));
            ui.horizontal(|ui| {
                ok = ui.button("In den Papierkorb").clicked();
                cancel = ui.button("Abbrechen").clicked();
            });
        });
        // Auch nach einem Fehler schließt die Rückfrage; der Hinweis erklärt den Grund.
        if ok {
            delete_node(ws, st, &path);
        } else if !cancel {
            st.confirm_delete = Some(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fonts, testing::collect_texts};
    use planora_project::config::Config;
    use planora_project::fsutil::new_id;

    fn frame(c: &egui::Context, ws: &mut Workspace, st: &mut ProjectsUi) -> String {
        let out = c.run_ui(egui::RawInput::default(), |ui| show(ui, ws, st));
        let t = collect_texts(&out);
        out.drop_without_applying_deltas();
        t
    }

    #[test]
    fn the_dialog_windows_are_not_drawn_while_a_prompt_is_open() {
        let c = egui::Context::default();
        c.set_fonts(fonts::definitions());
        let root = std::env::temp_dir().join(format!("planora-pp-unit-{}", new_id()));
        let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        let mut st = ProjectsUi::default();
        let p = create_project(&mut ws, &mut st, "P").unwrap();
        st.new_folder = Some((p, String::new()));
        ws.prompt = Some(Prompt::Notice("Hinweis".into()));
        let t = frame(&c, &mut ws, &mut st);
        assert!(!t.contains("Anlegen"), "{t}");
        assert!(st.new_folder.is_some(), "der Zustand bleibt erhalten");
        ws.prompt = None;
        frame(&c, &mut ws, &mut st);
        let t = frame(&c, &mut ws, &mut st);
        assert!(t.contains("Anlegen"), "{t}");
    }
}
