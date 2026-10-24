//! Panel „Projekte“: Baum Projekt → Ordner → Dokument (bis es dem Fenster „Projekte“ weicht).
//!
//! Die Aktionen und ihre Fenster stehen in [`crate::project_actions`]; das Panel zeichnet nur
//! den Baum, die Eingabe „Neues Projekt“ und die Wahl des Projektordners.

use crate::browser::error_color;
use crate::file_chooser::display_path;
pub use crate::project_actions::ProjectsUi;
use crate::project_actions::{self as actions, READ_ERROR_PREFIX, create_project};
use crate::theme;
use crate::workspace::{Prompt, Workspace};
use planora_project::config::Config;
use planora_project::library::{NodeKind, NodePath, TreeNode};
use std::path::Path;

/// Hinweis des Panels ohne gewählte Wurzel (das Panel hat das Eingabefeld darunter).
const NO_LIBRARY: &str = "Es ist noch kein Projektordner gewählt. Gib unten einen Ordner ein, in dem deine Projekte liegen sollen.";

/// Wunsch aus dem Baum, ausgeführt nach dem Zeichnen (der Baum ist dabei geliehen).
enum Action {
    Select(NodePath),
    Open(NodePath),
    NewFolder(NodePath),
    Rename(NodePath, String),
    Move(NodePath),
    Delete(NodePath),
}

/// Eingaben des Panels („Neues Projekt“, „Wurzel wählen“) im Speicher des Kontexts: sie gehören
/// nur zum Panel, nicht zu den Aktionen.
fn field(ui: &egui::Ui, key: &str) -> Option<String> {
    ui.data(|d| d.get_temp::<Option<String>>(field_id(key)))
        .flatten()
}

fn set_field(ui: &egui::Ui, key: &str, text: Option<String>) {
    ui.data_mut(|d| d.insert_temp(field_id(key), text));
}

fn field_id(key: &str) -> egui::Id {
    egui::Id::new(("planora_projects_panel_field", key.to_string()))
}

/// Zeichnet das Panel. Der Baum wird nur gelesen, wenn er ungültig ist (nach einer Aktion oder
/// „Aktualisieren“); solange eine Rückfrage offen ist, ist das Panel gesperrt.
pub fn show(ui: &mut egui::Ui, ws: &mut Workspace, st: &mut ProjectsUi) {
    let enabled = ws.prompt.is_none();
    ui.add_enabled_ui(enabled, |ui| show_enabled(ui, ws, st));
}

fn show_enabled(ui: &mut egui::Ui, ws: &mut Workspace, st: &mut ProjectsUi) {
    st.observe(ws);
    let Some(root) = ws.library.as_ref().map(|l| l.root().path().to_path_buf()) else {
        ui.label(NO_LIBRARY);
        ui.add_space(4.0);
        let mut text = field(ui, "root").unwrap_or_else(|| {
            ws.config
                .root
                .clone()
                .unwrap_or_else(Config::default_root)
                .display()
                .to_string()
        });
        if root_field(ui, ws, &mut text, false) != RootField::Open {
            set_field(ui, "root", None);
            st.select(None);
            st.invalidate();
        } else {
            set_field(ui, "root", Some(text));
        }
        return;
    };

    ui.small(display_path(&root));
    ui.horizontal_wrapped(|ui| {
        if ui.button("Neues Projekt").clicked() {
            set_field(ui, "project", Some(String::new()));
        }
        if ui.button("Aktualisieren").clicked() {
            st.invalidate();
        }
        if ui.button("Wurzel wählen …").clicked() {
            set_field(ui, "root", Some(display_path(&root)));
        }
    });
    if let Some(mut name) = field(ui, "project") {
        let mut done = false;
        let mut create = false;
        // Umbrechende Zeile wie bei der Wurzel: das Feld nimmt die freie Breite der Karte, die
        // Knöpfe rücken in die nächste Zeile (in einer festen Zeile ragten sie aus der Karte
        // und machten den Inhalt darunter breiter als die Karte).
        ui.horizontal_wrapped(|ui| {
            let r = theme::text_field(
                ui,
                egui::TextEdit::singleline(&mut name)
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
        if create && let Some(p) = create_project(ws, st, &name) {
            st.select(Some(p));
            done = true;
        }
        set_field(ui, "project", (!done).then_some(name));
    }
    if let Some(mut text) = field(ui, "root") {
        match root_field(ui, ws, &mut text, true) {
            RootField::Open => set_field(ui, "root", Some(text)),
            RootField::Cancelled => set_field(ui, "root", None),
            RootField::Chosen => {
                set_field(ui, "root", None);
                st.select(None);
                st.invalidate();
            }
        }
    }
    ui.separator();

    if st.is_stale(ws) {
        let tree = ws
            .library
            .as_ref()
            .map(|l| l.tree().map_err(|e| e.to_string()));
        if let Some(tree) = tree {
            st.set_tree(tree, ws.library_revision);
        }
    }
    let mut actions = Vec::new();
    match st.tree() {
        Some(Ok(nodes)) if nodes.is_empty() => {
            ui.weak("Noch keine Projekte. Lege mit „Neues Projekt“ das erste an.");
        }
        Some(Ok(nodes)) => {
            for n in nodes {
                draw_node(ui, ws, st.selected(), n, &mut actions);
            }
        }
        Some(Err(e)) => {
            ui.colored_label(
                error_color(ui.visuals().dark_mode),
                format!("{READ_ERROR_PREFIX}{e}"),
            );
        }
        None => {}
    }
    for a in actions {
        match a {
            Action::Select(p) => st.select(Some(p)),
            Action::Open(p) => {
                st.select(Some(p.clone()));
                actions::open_node(ws, &p);
            }
            Action::NewFolder(p) => actions::request_new_folder(st, p, ""),
            Action::Rename(p, name) => actions::request_rename(st, p, &name),
            Action::Move(p) => actions::request_move(st, p),
            Action::Delete(p) => actions::request_delete(st, p),
        }
    }
    // die Fenster sind eigene Ebenen: nur ohne offene Rückfrage des Programms
    actions::show_dialogs(ui.ctx(), ws, st);
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
                ws.prompt = Some(Prompt::Notice("Bitte einen Ordner angeben.".into()));
            } else {
                match ws.choose_root(Path::new(t)) {
                    Ok(()) => {
                        ws.persist_config();
                        out = RootField::Chosen;
                    }
                    Err(e) => ws.prompt = Some(Prompt::Notice(e)),
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
