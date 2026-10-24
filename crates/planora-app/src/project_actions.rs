//! Aktionen auf Projekte, Ordner und Dokumente und ihre Fenster („Neues Projekt“, „Neues
//! Dokument“, „Neuer Ordner“, „Umbenennen“, „Verschieben“, „Löschen“). Das Fenster „Projekte“
//! und (bis es entfällt) das Feld „Projekte“ rufen sie auf.
//!
//! Die Aktionen (`open_node`, `create_project`, …) lassen sich auch ohne Oberfläche aufrufen.
//! Fehler landen als `Prompt::Notice`, nie als Panik. Pfade kommen nur aus dem Baum der
//! Bibliothek und werden von ihr geprüft. Wechselt das Projektverzeichnis (oder ändert sich die
//! Bibliothek sonst über den Arbeitsbereich), schließen die offenen Fenster und die Auswahl
//! verfällt: ihre Pfade gehören zum alten Stand.

use crate::dialog::{Dialog, DialogSize};
use crate::theme;
use crate::workspace::{Prompt, Workspace};
use planora_project::ProjectError;
use planora_project::docstore::LockMode;
use planora_project::library::{Library, NodeKind, NodePath, TreeNode};

/// Hinweis ohne gewähltes Projektverzeichnis.
pub const NO_LIBRARY: &str = "Es ist noch kein Projektverzeichnis gewählt. Wähle mit „Projektverzeichnis …“ den Ordner, in dem deine Projekte liegen sollen.";
/// Anfang der Meldung, wenn sich der Projektordner nicht lesen lässt (danach der Grund).
pub const READ_ERROR_PREFIX: &str = "Der Projektordner lässt sich nicht lesen: ";
/// Ablehnung, solange betroffene Dokumente in Tabs offen sind.
const CLOSE_FIRST: &str = "Bitte zuerst die betroffenen Dokumente schließen.";
/// Meldung, wenn eine Aktion keine Bibliothek hat.
const NO_ROOT: &str = "Es ist noch kein Projektordner gewählt.";

#[derive(Default)]
pub struct ProjectsUi {
    /// Zwischengespeicherter Baum (oder der Lesefehler); `None` = neu lesen.
    tree: Option<Result<Vec<TreeNode>, String>>,
    selected: Option<NodePath>,
    /// Fenster „Neues Projekt“: Name.
    new_project: Option<String>,
    /// Fenster „Neues Dokument“: Ort und Name.
    new_document: Option<(NodePath, String)>,
    /// Fenster „Neuer Ordner“: Ort und Name.
    new_folder: Option<(NodePath, String)>,
    /// Fenster „Umbenennen“: Knoten und neuer Name.
    rename: Option<(NodePath, String)>,
    /// Fenster „Verschieben“: Knoten und gewähltes Ziel.
    moving: Option<(NodePath, Option<NodePath>)>,
    /// Rückfrage „Löschen“.
    confirm_delete: Option<NodePath>,
    /// `Workspace::library_revision`, zu der `tree` gelesen wurde.
    tree_revision: Option<u64>,
    /// Zuletzt bemerkte `Workspace::library_revision` (`None` = noch nie hingesehen); Fenster
    /// und Auswahl gehören zu diesem Stand.
    seen_revision: Option<u64>,
    /// Zählt [`ProjectsUi::invalidate`]: ein Lesen, das vorher begann, ist überholt.
    generation: u64,
}

impl ProjectsUi {
    /// Der Baum wird neu gelesen (beim nächsten Zeichnen bzw. vom Fenster im Hintergrund).
    pub fn invalidate(&mut self) {
        self.tree = None;
        self.generation += 1;
    }

    pub fn selected(&self) -> Option<&NodePath> {
        self.selected.as_ref()
    }

    pub fn select(&mut self, path: Option<NodePath>) {
        self.selected = path;
    }

    /// Fehlt der Baum oder hat sich die Bibliothek seit dem Lesen über den Arbeitsbereich
    /// geändert (`library_revision`)?
    pub fn is_stale(&self, ws: &Workspace) -> bool {
        self.tree.is_none() || self.tree_revision != Some(ws.library_revision)
    }

    /// Neuer Baum (oder Lesefehler), gelesen beim Stand `revision`.
    pub fn set_tree(&mut self, tree: Result<Vec<TreeNode>, String>, revision: u64) {
        self.tree = Some(tree);
        self.tree_revision = Some(revision);
    }

    /// Der zuletzt gelesene Baum (oder der Lesefehler).
    pub fn tree(&self) -> Option<&Result<Vec<TreeNode>, String>> {
        self.tree.as_ref()
    }

    /// Schließt alle Fenster der Aktionen (ohne etwas auszuführen).
    pub fn close_dialogs(&mut self) {
        self.new_project = None;
        self.new_document = None;
        self.new_folder = None;
        self.rename = None;
        self.moving = None;
        self.confirm_delete = None;
    }

    /// Bemerkt eine Änderung der Bibliothek über den Arbeitsbereich (Speichern unter anderswo,
    /// neues Projektverzeichnis): Fenster schließen, Auswahl verwerfen, denn deren Pfade gehören
    /// zum alten Stand (ein anderes Verzeichnis kann gleichnamige Projekte haben). Neu gelesen
    /// wird über [`ProjectsUi::is_stale`]. `true`, wenn sich etwas geändert hatte.
    pub fn observe(&mut self, ws: &Workspace) -> bool {
        let now = ws.library_revision;
        match self.seen_revision {
            Some(seen) if seen == now => false,
            Some(_) => {
                self.seen_revision = Some(now);
                self.close_dialogs();
                self.selected = None;
                true
            }
            None => {
                self.seen_revision = Some(now);
                false
            }
        }
    }

    /// Zähler der Aufrufe von [`ProjectsUi::invalidate`].
    #[doc(hidden)]
    pub fn generation(&self) -> u64 {
        self.generation
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

/// Öffnet ein Dokument in einem Tab (ein offenes wird nur nach vorn geholt). Ist es gesperrt,
/// fragt `Prompt::TakeOver` nach.
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

/// Legt in `parent` ein leeres Dokument an ([`Workspace::create_document`]), öffnet es in einem
/// Tab und wählt es. Fehler (Name, „gibt es schon“) erscheinen als Hinweis; dann ändert sich
/// nichts.
pub fn create_document(
    ws: &mut Workspace,
    st: &mut ProjectsUi,
    parent: &NodePath,
    name: &str,
) -> Option<NodePath> {
    let before = ws.library_revision;
    let r = ws.create_document(parent, name);
    st.invalidate();
    let path = r.map_err(|e| notice(ws, e)).ok()?;
    // die eigene Änderung schließt weder Fenster noch Auswahl (anders als eine fremde)
    if st.seen_revision == Some(before) {
        st.seen_revision = Some(ws.library_revision);
    }
    open_node(ws, &path);
    st.selected = Some(path.clone());
    Some(path)
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

/// Verschiebt den Knoten in den Papierkorb des Projektverzeichnisses.
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

// ---- Fenster ---------------------------------------------------------------------------

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

/// Ziele zum Verschieben von `path` aus dem zuletzt gelesenen Baum.
#[doc(hidden)]
pub fn move_targets(st: &ProjectsUi, path: &NodePath) -> Vec<NodePath> {
    let mut out = Vec::new();
    if let Some(Ok(nodes)) = &st.tree {
        containers(nodes, path, &mut out);
    }
    out
}

fn label(path: &NodePath) -> String {
    path.join(" / ")
}

fn dialog_window(title: &str, key: &str) -> Dialog<'static> {
    Dialog::new(title, DialogSize::Prompt)
        .id(egui::Id::new(("planora_projects_dialog", key.to_string())))
        .fixed()
}

/// Öffnet das Fenster „Neues Projekt“ mit dem Namen `name`.
#[doc(hidden)]
pub fn request_new_project(st: &mut ProjectsUi, name: &str) {
    st.new_project = Some(name.to_string());
}

/// Öffnet das Fenster „Neues Dokument“ in `parent` mit dem Namen `name`.
#[doc(hidden)]
pub fn request_new_document(st: &mut ProjectsUi, parent: NodePath, name: &str) {
    st.new_document = Some((parent, name.to_string()));
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

/// Ob eines der Fenster offen ist.
pub fn dialog_open(st: &ProjectsUi) -> bool {
    st.new_project.is_some()
        || st.new_document.is_some()
        || st.new_folder.is_some()
        || st.rename.is_some()
        || st.moving.is_some()
        || st.confirm_delete.is_some()
}

/// Textfeld eines Fensters; `true`, wenn darin Enter gedrückt wurde.
fn name_field(ui: &mut egui::Ui, name: &mut String) -> bool {
    let r = theme::text_field(ui, egui::TextEdit::singleline(name));
    r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

/// Fenster mit Text, Namensfeld, „Anlegen“ (auch Enter) und „Abbrechen“; liefert
/// (anlegen, abbrechen).
fn name_window(
    ctx: &egui::Context,
    title: &str,
    key: &str,
    text: Option<String>,
    name: &mut String,
) -> (bool, bool) {
    let (mut ok, mut cancel) = (false, false);
    dialog_window(title, key).show(ctx, |ui| {
        if let Some(t) = &text {
            ui.label(t);
        }
        let enter = name_field(ui, name);
        ui.horizontal(|ui| {
            ok = ui.button("Anlegen").clicked() || enter;
            cancel = ui.button("Abbrechen").clicked();
        });
    });
    (ok, cancel)
}

/// Fenster „Neues Projekt“, „Neues Dokument“, „Neuer Ordner“, „Umbenennen“, „Verschieben“ und
/// „Löschen“. Sie sind eigene Ebenen und werden nur gezeichnet, solange keine Rückfrage des
/// Programms offen ist. Hat sich die Bibliothek über den Arbeitsbereich geändert, schließen sie
/// vorher ([`ProjectsUi::observe`]).
pub fn show_dialogs(ctx: &egui::Context, ws: &mut Workspace, st: &mut ProjectsUi) {
    st.observe(ws);
    if ws.prompt.is_some() {
        return;
    }
    if let Some(mut name) = st.new_project.take() {
        let (ok, cancel) = name_window(ctx, "Neues Projekt", "project", None, &mut name);
        let done = cancel
            || (ok
                && create_project(ws, st, &name)
                    .inspect(|p| st.selected = Some(p.clone()))
                    .is_some());
        if !done {
            st.new_project = Some(name);
        }
    }
    if let Some((parent, mut name)) = st.new_document.take() {
        let text = format!("In „{}“:", label(&parent));
        let (ok, cancel) = name_window(ctx, "Neues Dokument", "document", Some(text), &mut name);
        let done = cancel || (ok && create_document(ws, st, &parent, &name).is_some());
        if !done {
            st.new_document = Some((parent, name));
        }
    }
    if let Some((parent, mut name)) = st.new_folder.take() {
        let text = format!("In „{}“:", label(&parent));
        let (ok, cancel) = name_window(ctx, "Neuer Ordner", "folder", Some(text), &mut name);
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
        let targets = move_targets(st, &path);
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
        let out = c.run_ui(egui::RawInput::default(), |ui| {
            show_dialogs(ui.ctx(), ws, st)
        });
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
        let _ = std::fs::remove_dir_all(&root);
    }
}
