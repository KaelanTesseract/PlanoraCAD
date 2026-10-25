//! Fenster für Rückfragen des Arbeitsbereichs. Die Knöpfe rufen nur Methoden des
//! [`Workspace`] auf; Eingabefelder bleiben erhalten, indem der Prompt geklont, bearbeitet und
//! zurückgeschrieben wird.

use crate::dialog::{Dialog, DialogSize};
use crate::keymap::{self, Action, Keymap};
use crate::theme;
use crate::workspace::{After, Prompt, SaveOutcome, Workspace};
use planora_project::docstore::LockMode;
use planora_project::library::{NodeKind, NodePath, TreeNode};

/// Zeigt `ws.prompt` als Fenster und führt die Knöpfe aus. Vorher in jedem Bild
/// [`Workspace::refresh_prompts`] aufrufen, damit offene Autosicherungs-Angebote erscheinen.
pub fn show(ctx: &egui::Context, ws: &mut Workspace) {
    let Some(prompt) = ws.prompt.clone() else {
        return;
    };
    match prompt {
        Prompt::SaveChanges { tab, then } => save_changes(ctx, ws, tab, then),
        Prompt::SaveComment {
            tab,
            then,
            comment,
            branch,
        } => save_comment(ctx, ws, tab, then, comment, branch),
        Prompt::SaveAs {
            tab,
            then,
            parent,
            name,
            comment,
        } => save_as(ctx, ws, tab, then, parent, name, comment),
        Prompt::TakeOver { path, version } => take_over(ctx, ws, path, version),
        Prompt::AutosaveOffer { tab } => autosave_offer(ctx, ws, tab),
        Prompt::Notice(text) => notice(ctx, ws, &text),
    }
}

/// Mittiges, festes Fenster für eine Rückfrage.
fn window(ctx: &egui::Context, key: &str, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    Dialog::new(title, DialogSize::Prompt)
        .id(egui::Id::new(("planora_prompt", key)))
        .fixed()
        .show(ctx, add_contents);
}

fn tab_name(ws: &Workspace, tab: u64) -> String {
    ws.tab_index(tab)
        .map(|i| ws.tabs[i].title())
        .unwrap_or_default()
}

/// Titel der Rückfrage, wenn PlanoraCAD für ein Update neu startet.
pub const SAVE_FOR_UPDATE_TITLE: &str = "Neustart für das Update";

/// Text der Rückfrage beim Neustart für ein Update.
pub fn save_for_update_text(name: &str) -> String {
    format!(
        "Zum Aktualisieren muss PlanoraCAD neu starten. Ungespeicherte Änderungen in „{name}“ speichern?"
    )
}

/// „Änderungen speichern?“; beim Beenden für ein Update ([`Workspace::quit_for_update`]) mit
/// Titel und Text, die das Update nennen. Die Knöpfe sind dieselben.
fn save_changes(ctx: &egui::Context, ws: &mut Workspace, tab: u64, then: After) {
    let name = tab_name(ws, tab);
    let mut choice = None;
    let for_update = ws.quit_for_update && then == After::Quit;
    let (title, text) = if for_update {
        (SAVE_FOR_UPDATE_TITLE, save_for_update_text(&name))
    } else {
        (
            "Änderungen speichern?",
            format!("„{name}“ hat ungespeicherte Änderungen."),
        )
    };
    window(ctx, "save_changes", title, |ui| {
        ui.add(egui::Label::new(text).wrap());
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Speichern").clicked() {
                choice = Some(0);
            }
            if ui.button("Verwerfen").clicked() {
                choice = Some(1);
            }
            if ui.button("Abbrechen").clicked() {
                choice = Some(2);
            }
        });
    });
    match choice {
        Some(0) => ws.save_for_prompt(),
        Some(1) => ws.discard_and_continue(),
        Some(2) => ws.prompt = None,
        _ => {}
    }
}

fn save_comment(
    ctx: &egui::Context,
    ws: &mut Workspace,
    tab: u64,
    then: Option<After>,
    mut comment: String,
    mut branch: Option<String>,
) {
    let mut done = None;
    window(ctx, "save_comment", "Version speichern", |ui| {
        if let Some(b) = branch.as_mut() {
            ui.label(
                "Diese Version ist nicht die neueste ihres Zweigs. Der Stand wird in einem neuen Zweig gespeichert.",
            );
            ui.horizontal(|ui| {
                ui.label("Zweig:");
                theme::text_field(ui, egui::TextEdit::singleline(b));
            });
        }
        ui.horizontal(|ui| {
            ui.label("Kommentar:");
            theme::text_field(ui, egui::TextEdit::singleline(&mut comment));
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let ok = branch.as_ref().is_none_or(|b| !b.trim().is_empty());
            if ui.add_enabled(ok, egui::Button::new("Speichern")).clicked() {
                done = Some(true);
            }
            if ui.button("Abbrechen").clicked() {
                done = Some(false);
            }
        });
    });
    match done {
        Some(true) => {
            ws.prompt = None;
            let Some(i) = ws.tab_index(tab) else { return };
            ws.activate(i);
            match ws.save_active(&comment, branch.as_deref().map(str::trim)) {
                Ok(SaveOutcome::Saved(_)) => ws.finish_after(tab, then),
                // Stand ist nicht die Spitze: Zweignamen nachfragen
                Ok(SaveOutcome::NeedsBranch) => {
                    ws.prompt = Some(Prompt::SaveComment {
                        tab,
                        then,
                        comment,
                        branch: Some(String::new()),
                    });
                }
                Ok(SaveOutcome::NeedsLocation) => {
                    let name = ws.tabs[i].name.clone();
                    ws.prompt = Some(Prompt::SaveAs {
                        tab,
                        then,
                        parent: Vec::new(),
                        name,
                        comment,
                    });
                }
                Err(e) => ws.prompt = Some(Prompt::Notice(e)),
            }
        }
        Some(false) => ws.prompt = None,
        None => {
            ws.prompt = Some(Prompt::SaveComment {
                tab,
                then,
                comment,
                branch,
            })
        }
    }
}

/// Aufklappbarer Baum der Projekte und Ordner; ein Klick wählt das Ziel.
fn container_tree(ui: &mut egui::Ui, nodes: &[TreeNode], selected: &mut NodePath) {
    for node in nodes {
        if node.kind == NodeKind::Document {
            continue;
        }
        let is_selected = *selected == node.path;
        let has_children = node.children.iter().any(|c| c.kind != NodeKind::Document);
        let label = node.name.clone();
        if has_children {
            let id = ui.make_persistent_id(("speichern_unter", &node.path));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                .show_header(ui, |ui| {
                    if ui.selectable_label(is_selected, label).clicked() {
                        *selected = node.path.clone();
                    }
                })
                .body(|ui| container_tree(ui, &node.children, selected));
        } else {
            ui.horizontal(|ui| {
                // gleiche Einrückung wie Knoten mit Aufklapp-Pfeil
                ui.add_space(ui.spacing().indent);
                if ui.selectable_label(is_selected, label).clicked() {
                    *selected = node.path.clone();
                }
            });
        }
    }
}

/// Hinweis in „Speichern unter“, wenn es noch kein Projekt gibt: nennt die Taste für Projekte
/// aus der Tastenbelegung, ohne Taste den Weg über das Menü „Datei“.
pub fn no_project_hint(km: &Keymap) -> String {
    let keys = keymap::shortcut_hint(km, Action::Projects);
    let open = if keys.is_empty() {
        format!("im Menü „Datei“ unter „{}“", crate::app::PROJECTS_ENTRY)
    } else {
        format!("mit {keys}")
    };
    format!(
        "Noch kein Projekt vorhanden. Bitte mit „Abbrechen“ schließen, dann {open} das Fenster Projekte öffnen und ein Projekt anlegen."
    )
}

fn save_as(
    ctx: &egui::Context,
    ws: &mut Workspace,
    tab: u64,
    then: Option<After>,
    mut parent: NodePath,
    mut name: String,
    mut comment: String,
) {
    let tree = ws.library.as_ref().map(|l| l.tree());
    let no_project_hint = no_project_hint(&ws.keymap);
    let mut done = None;
    window(ctx, "save_as", "Speichern unter", |ui| {
        match &tree {
            None => {
                // Bei offener Rückfrage sind Projekte und Einstellungen gesperrt: erst schließen.
                // (Kein „→“: das Zeichen fehlt in den Schriften.)
                ui.label("Bitte mit „Abbrechen“ schließen, dann im Menü „Datei“ unter „Einstellungen …“ das Projektverzeichnis wählen.");
            }
            Some(Err(e)) => {
                ui.label(format!("Die Projekte lassen sich nicht lesen: {e}"));
            }
            Some(Ok(nodes)) => {
                ui.label("Ziel (Projekt oder Ordner):");
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    let area = egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .auto_shrink([false, true]);
                    theme::scroll_area(ui, area, |ui| {
                        if nodes.iter().all(|n| n.kind == NodeKind::Document) {
                            ui.weak(&no_project_hint);
                        }
                        container_tree(ui, nodes, &mut parent);
                    });
                });
            }
        }
        egui::Grid::new("speichern_unter_felder")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Name:");
                theme::text_field(ui, egui::TextEdit::singleline(&mut name));
                ui.end_row();
                ui.label("Kommentar:");
                theme::text_field(ui, egui::TextEdit::singleline(&mut comment));
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let ok = matches!(tree, Some(Ok(_))) && !parent.is_empty() && !name.trim().is_empty();
            if ui
                .add_enabled(ok, egui::Button::new("Speichern"))
                .on_disabled_hover_text("Ziel wählen und einen Namen eingeben.")
                .clicked()
            {
                done = Some(true);
            }
            if ui.button("Abbrechen").clicked() {
                done = Some(false);
            }
        });
    });
    match done {
        Some(true) => {
            ws.prompt = None;
            let Some(i) = ws.tab_index(tab) else { return };
            ws.activate(i);
            match ws.save_as(&parent, name.trim(), &comment) {
                Ok(_) => ws.finish_after(tab, then),
                Err(e) => ws.prompt = Some(Prompt::Notice(e)),
            }
        }
        Some(false) => ws.prompt = None,
        None => {
            ws.prompt = Some(Prompt::SaveAs {
                tab,
                then,
                parent,
                name,
                comment,
            })
        }
    }
}

fn take_over(ctx: &egui::Context, ws: &mut Workspace, path: NodePath, version: Option<u32>) {
    let mut done = None;
    window(ctx, "take_over", "Dokument gesperrt", |ui| {
        ui.label(
            "Das Dokument wird gerade von einem anderen Programm benutzt (gesperrt). Falls das Programm abgestürzt ist, kannst du die Sperre übernehmen.",
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Sperre übernehmen").clicked() {
                done = Some(true);
            }
            if ui.button("Abbrechen").clicked() {
                done = Some(false);
            }
        });
    });
    match done {
        Some(true) => {
            ws.prompt = None;
            match ws.open_document(&path, LockMode::TakeOver) {
                Ok(r) => {
                    let mut notes = r.notes;
                    if let Some(n) = version
                        && let Err(e) = ws.open_version(&path, n)
                    {
                        notes.push(e.to_string());
                    }
                    // Ein Autosicherungs-Angebot hat Vorrang; die Hinweise stehen dann in der
                    // Statuszeile des Tabs.
                    if !notes.is_empty() {
                        let text = notes.join("\n");
                        if ws.prompt.is_none() {
                            ws.prompt = Some(Prompt::Notice(text));
                        } else {
                            ws.active_tab_mut().state.message = Some(text);
                        }
                    }
                }
                Err(e) => ws.prompt = Some(Prompt::Notice(e.to_string())),
            }
        }
        Some(false) => ws.prompt = None,
        None => {}
    }
}

fn autosave_offer(ctx: &egui::Context, ws: &mut Workspace, tab: u64) {
    let name = tab_name(ws, tab);
    let mut choice = None;
    window(ctx, "autosave_offer", "Autosicherung gefunden", |ui| {
        ui.strong(name);
        ui.label(
            "Für dieses Dokument gibt es eine Autosicherung, die neuer ist als der gespeicherte Stand (zum Beispiel nach einem Absturz).",
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button("Wiederherstellen")
                .on_hover_text("Den Stand der Autosicherung laden (gilt danach als ungespeichert).")
                .clicked()
            {
                choice = Some(true);
            }
            if ui
                .button("Verwerfen")
                .on_hover_text("Die Autosicherung löschen und den gespeicherten Stand behalten.")
                .clicked()
            {
                choice = Some(false);
            }
        });
    });
    match choice {
        Some(true) => ws.apply_autosave(tab),
        Some(false) => ws.discard_autosave(tab),
        None => {}
    }
}

fn notice(ctx: &egui::Context, ws: &mut Workspace, text: &str) {
    let mut ok = false;
    window(ctx, "notice", "Hinweis", |ui| {
        ui.label(text);
        ui.add_space(6.0);
        if ui.button("OK").clicked() {
            ok = true;
        }
    });
    if ok {
        ws.prompt = None;
    }
}
