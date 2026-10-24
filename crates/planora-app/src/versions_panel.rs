//! Panel „Versionen“: Versionsbaum des aktiven Dokuments.
//!
//! Die Aktionen (`open_version`, `restore`, …) lassen sich auch ohne Oberfläche aufrufen.
//! Fehler landen als `Prompt::Notice`, nie als Panik. Gespeicherte Versionen werden nur über
//! den beschreibbaren Tab des Dokuments geändert (er hält die Sperre).

use crate::browser::error_color;
use crate::dialog::{Dialog, DialogSize};
use crate::local_time;
use crate::theme;
use crate::workspace::{Prompt, Workspace};
use planora_project::docstore::DocumentInfo;
use planora_project::library::NodePath;

/// Hinweis, wenn zum Ändern kein beschreibbarer Tab des Dokuments offen ist.
const OPEN_EDITABLE: &str = "Zum Ändern das Dokument zuerst bearbeitbar öffnen.";
const NOT_SAVED: &str = "Das Dokument ist noch nicht gespeichert.";

#[derive(Default)]
pub struct VersionsUi {
    /// Kommentar-Entwurf (Fenster offen, wenn `Some`).
    comment_edit: Option<Pending<(u32, String)>>,
    confirm_delete: Option<Pending<Confirm>>,
    /// Versionsbaum einer schreibgeschützten Ansicht ohne beschreibbaren Tab, gemerkt je Tab,
    /// damit die Dateien nicht in jedem Bild gelesen werden. Ungültig nach jeder Aktion und
    /// beim Wechsel des Tabs.
    view_cache: Option<(u64, Option<DocumentInfo>)>,
}

/// Eine offene Rückfrage gehört zu genau einem Tab und Dokument; wechselt der aktive Tab,
/// verfällt sie.
struct Pending<T> {
    tab: u64,
    path: Option<NodePath>,
    what: T,
}

impl<T> Pending<T> {
    fn new(ws: &Workspace, what: T) -> Pending<T> {
        let t = ws.active_tab();
        Pending {
            tab: t.id,
            path: t.path.clone(),
            what,
        }
    }

    fn valid(&self, ws: &Workspace) -> bool {
        let t = ws.active_tab();
        t.id == self.tab && t.path == self.path
    }
}

enum Confirm {
    Version(u32),
    Branch(String),
}

pub struct Row {
    pub number: u32,
    pub branch: String,
    pub depth: usize,
    pub is_tip: bool,
}

/// Anzeigereihenfolge: neueste oben; `depth` = Platz des Zweigs in der Reihenfolge seiner
/// ersten Version (Haupt = 0).
pub fn rows(info: &DocumentInfo) -> Vec<Row> {
    let mut order: Vec<&str> = Vec::new();
    for v in &info.versions {
        if !order.contains(&v.branch.as_str()) {
            order.push(&v.branch);
        }
    }
    let mut out: Vec<Row> = info
        .versions
        .iter()
        .map(|v| Row {
            number: v.number,
            branch: v.branch.clone(),
            depth: order.iter().position(|b| *b == v.branch).unwrap_or(0),
            is_tip: info.tip(&v.branch) == Some(v.number),
        })
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.number));
    out
}

fn notice(ws: &mut Workspace, msg: impl Into<String>) {
    ws.prompt = Some(Prompt::Notice(msg.into()));
}

fn active_path(ws: &Workspace) -> Option<NodePath> {
    ws.active_tab().path.clone()
}

/// Index des beschreibbaren Tabs des aktiven Dokuments (aus einer Ansicht heraus der schon
/// offene beschreibbare Tab); sonst steht ein Hinweis.
fn writable(ws: &mut Workspace) -> Option<usize> {
    let Some(path) = active_path(ws) else {
        notice(ws, NOT_SAVED);
        return None;
    };
    let i = ws.tab_for_path(&path);
    if i.is_none() {
        notice(ws, OPEN_EDITABLE);
    }
    i
}

/// Öffnet Version `n` schreibgeschützt.
pub fn open_version(ws: &mut Workspace, n: u32) {
    let Some(path) = active_path(ws) else {
        return notice(ws, NOT_SAVED);
    };
    if let Err(e) = ws.open_version(&path, n) {
        notice(ws, e.to_string());
    }
}

pub fn restore(ws: &mut Workspace, n: u32) {
    let i = ws.active;
    if let Err(e) = ws.restore_version(i, n) {
        notice(ws, e);
    }
}

/// Öffnet Version `n` als Ansicht und macht sie bearbeitbar (neuer Zweig beim Speichern).
pub fn branch_from(ws: &mut Workspace, n: u32) {
    let Some(path) = active_path(ws) else {
        return notice(ws, NOT_SAVED);
    };
    let view = match ws.open_version(&path, n) {
        Ok(i) => i,
        Err(e) => return notice(ws, e.to_string()),
    };
    if let Err(e) = ws.make_editable(view) {
        notice(ws, e);
    }
}

pub fn set_comment(ws: &mut Workspace, n: u32, text: &str) {
    let Some(i) = writable(ws) else { return };
    if let Some(store) = ws.tabs[i].store.as_mut()
        && let Err(e) = store.set_comment(n, text)
    {
        notice(ws, e.to_string());
    }
}

pub fn delete_version(ws: &mut Workspace, n: u32) {
    let Some(i) = writable(ws) else { return };
    if let Err(e) = ws.delete_version(i, n) {
        notice(ws, e);
    }
}

pub fn delete_branch(ws: &mut Workspace, name: &str) {
    let Some(i) = writable(ws) else { return };
    if let Err(e) = ws.delete_branch(i, name) {
        notice(ws, e);
    }
}

/// Öffnet die Rückfrage „Löschen“ für Version `n`; ohne beschreibbaren Tab steht stattdessen
/// ein Hinweis.
pub fn request_delete_version(ws: &mut Workspace, st: &mut VersionsUi, n: u32) {
    if writable(ws).is_some() {
        st.confirm_delete = Some(Pending::new(ws, Confirm::Version(n)));
    }
}

/// Öffnet die Rückfrage „Zweig löschen“.
pub fn request_delete_branch(ws: &mut Workspace, st: &mut VersionsUi, name: &str) {
    if writable(ws).is_some() {
        st.confirm_delete = Some(Pending::new(ws, Confirm::Branch(name.to_string())));
    }
}

/// Öffnet das Fenster „Kommentar ändern“.
pub fn request_comment(ws: &mut Workspace, st: &mut VersionsUi, n: u32, text: &str) {
    if writable(ws).is_some() {
        st.comment_edit = Some(Pending::new(ws, (n, text.to_string())));
    }
}

/// Führt die offene Löschrückfrage aus, wenn sie noch zum aktiven Tab gehört.
pub fn confirm_pending_delete(ws: &mut Workspace, st: &mut VersionsUi) {
    st.view_cache = None;
    let Some(p) = st.confirm_delete.take() else {
        return;
    };
    if !p.valid(ws) {
        return;
    }
    match &p.what {
        Confirm::Version(n) => delete_version(ws, *n),
        Confirm::Branch(b) => delete_branch(ws, b),
    }
}

/// Gibt es eine offene Rückfrage oder ein offenes Kommentarfenster?
pub fn has_pending(st: &VersionsUi) -> bool {
    st.confirm_delete.is_some() || st.comment_edit.is_some()
}

/// Verwirft offene Rückfragen, die nicht zum aktiven Tab gehören.
fn drop_stale(ws: &Workspace, st: &mut VersionsUi) {
    if st.confirm_delete.as_ref().is_some_and(|p| !p.valid(ws)) {
        st.confirm_delete = None;
    }
    if st.comment_edit.as_ref().is_some_and(|p| !p.valid(ws)) {
        st.comment_edit = None;
    }
}

/// Setzt den aktiven Zweig und öffnet dessen Spitze als Ansicht (der beschreibbare Tab behält
/// seinen Arbeitsstand).
pub fn switch_branch(ws: &mut Workspace, name: &str) {
    let Some(i) = writable(ws) else { return };
    let Some(store) = ws.tabs[i].store.as_mut() else {
        return;
    };
    if let Err(e) = store.set_active_branch(name) {
        return notice(ws, e.to_string());
    }
    let tip = store.info().tip(name);
    // die Kachel zeigt die Spitze des aktiven Zweigs: das Fenster Projekte liest neu
    ws.content_revision += 1;
    let Some(tip) = tip else {
        return;
    };
    ws.activate(i);
    open_version(ws, tip);
}

enum Action {
    Open(u32),
    Restore(u32),
    Branch(u32),
    Comment(u32, String),
    AskDelete(u32),
    AskDeleteBranch(String),
    Switch(String),
}

/// Versionsbaum des aktiven Tabs: bei beschreibbaren Tabs und Ansichten mit offenem
/// beschreibbaren Tab aus dem Speicher; bei einer reinen Ansicht einmal gelesen und je Tab
/// gemerkt (`Err` = lässt sich nicht lesen).
fn current_info(ws: &Workspace, st: &mut VersionsUi) -> Option<Result<DocumentInfo, ()>> {
    let tab = ws.active_tab();
    let path = tab.path.as_ref()?;
    let live = if tab.store.is_some() {
        Some(tab)
    } else {
        ws.tab_for_path(path).map(|i| &ws.tabs[i])
    };
    if let Some(info) = live.and_then(|t| t.store.as_ref()).map(|s| s.info()) {
        // der Stand der Ansicht kann sich geändert haben, solange sie nicht angezeigt wurde
        st.view_cache = None;
        return Some(Ok(info.clone()));
    }
    if st.view_cache.as_ref().map(|c| c.0) != Some(tab.id) {
        st.view_cache = Some((tab.id, tab.version_info()));
    }
    Some(st.view_cache.as_ref().and_then(|c| c.1.clone()).ok_or(()))
}

/// Zeichnet das Panel; solange eine Rückfrage offen ist, ist es gesperrt.
pub fn show(ui: &mut egui::Ui, ws: &mut Workspace, st: &mut VersionsUi) {
    show_with_header(ui, ws, st, true);
}

/// Wie [`show`]; ohne `header` fehlt der Abschnittskopf „Versionen“ (den zeigt dann der
/// Aufrufer, z. B. als Umschalter).
pub fn show_with_header(ui: &mut egui::Ui, ws: &mut Workspace, st: &mut VersionsUi, header: bool) {
    drop_stale(ws, st);
    let enabled = ws.prompt.is_none();
    ui.add_enabled_ui(enabled, |ui| show_enabled(ui, ws, st, header));
}

fn show_enabled(ui: &mut egui::Ui, ws: &mut Workspace, st: &mut VersionsUi, header: bool) {
    let info = match current_info(ws, st) {
        None => {
            ui.weak("Dieses Dokument ist noch nicht gespeichert.");
            return;
        }
        Some(Err(())) => {
            ui.colored_label(
                error_color(ui.visuals().dark_mode),
                "Die Versionen lassen sich nicht lesen.",
            );
            return;
        }
        Some(Ok(info)) => info,
    };
    if header {
        theme::section_header(ui, "Versionen", theme::HEADER_H);
    }
    let (read_only, dirty, based_on) = {
        let t = ws.active_tab();
        (t.read_only, t.is_dirty(), t.based_on)
    };
    let can_write = active_path(ws).and_then(|p| ws.tab_for_path(&p)).is_some();
    let mut actions = Vec::new();
    if read_only {
        ui.label("Du siehst eine alte Version.");
        if let Some(n) = based_on {
            ui.horizontal(|ui| {
                if ui.button("Wiederherstellen").clicked() {
                    actions.push(Action::Restore(n));
                }
                if ui.button("Abzweigen").clicked() {
                    actions.push(Action::Branch(n));
                }
            });
        }
    }
    if dirty {
        ui.label("Änderungen (nicht gespeichert)");
    }
    ui.separator();
    for r in rows(&info) {
        let Some(v) = info.version(r.number) else {
            continue;
        };
        let mark = if based_on == Some(r.number) {
            "* "
        } else if r.is_tip && r.depth > 0 {
            "> "
        } else {
            ""
        };
        let text = format!(
            "{}{mark}v{}  {}  {}  {}",
            "  ".repeat(r.depth),
            r.number,
            r.branch,
            local_time::local_time_text(&v.time),
            v.comment
        );
        let resp = ui.selectable_label(based_on == Some(r.number), text);
        if resp.double_clicked() {
            actions.push(Action::Open(r.number));
        }
        resp.context_menu(|ui| {
            if ui.button("Öffnen").clicked() {
                actions.push(Action::Open(r.number));
                ui.close();
            }
            if ui.button("Wiederherstellen").clicked() {
                actions.push(Action::Restore(r.number));
                ui.close();
            }
            if ui.button("Abzweigen").clicked() {
                actions.push(Action::Branch(r.number));
                ui.close();
            }
            let w = |ui: &mut egui::Ui, label: &str| {
                ui.add_enabled(can_write, egui::Button::new(label))
                    .on_disabled_hover_text(OPEN_EDITABLE)
                    .clicked()
            };
            if w(ui, "Kommentar ändern …") {
                actions.push(Action::Comment(r.number, v.comment.clone()));
                ui.close();
            }
            if w(ui, "Löschen …") {
                actions.push(Action::AskDelete(r.number));
                ui.close();
            }
            if r.is_tip {
                if w(ui, "Zweig löschen …") {
                    actions.push(Action::AskDeleteBranch(r.branch.clone()));
                    ui.close();
                }
                if w(ui, "Zu diesem Zweig wechseln") {
                    actions.push(Action::Switch(r.branch.clone()));
                    ui.close();
                }
            }
        });
    }
    let acted = !actions.is_empty();
    for a in actions {
        match a {
            Action::Open(n) => open_version(ws, n),
            Action::Restore(n) => restore(ws, n),
            Action::Branch(n) => branch_from(ws, n),
            Action::Comment(n, text) => request_comment(ws, st, n, &text),
            Action::AskDelete(n) => request_delete_version(ws, st, n),
            Action::AskDeleteBranch(b) => request_delete_branch(ws, st, &b),
            Action::Switch(name) => switch_branch(ws, &name),
        }
    }
    if acted {
        st.view_cache = None;
    }
    // die Fenster sind eigene Ebenen: nur zeichnen, solange keine Rückfrage des Programms offen ist
    if ws.prompt.is_none() {
        dialogs(ui.ctx(), ws, st);
    }
}

/// Fenster „Kommentar ändern“ und Rückfrage „Löschen“.
fn dialogs(ctx: &egui::Context, ws: &mut Workspace, st: &mut VersionsUi) {
    if let Some(mut p) = st.comment_edit.take() {
        let mut keep = true;
        let mut apply = false;
        Dialog::new("Kommentar ändern", DialogSize::Prompt)
            .fixed()
            .show(ctx, |ui| {
                ui.label(format!("Kommentar zu v{}", p.what.0));
                let r = theme::text_field(ui, egui::TextEdit::singleline(&mut p.what.1));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.button("Übernehmen").clicked() || enter {
                        apply = true;
                        keep = false;
                    }
                    if ui.button("Abbrechen").clicked() {
                        keep = false;
                    }
                });
            });
        if apply {
            let (n, text) = p.what.clone();
            set_comment(ws, n, &text);
            st.view_cache = None;
        } else if keep {
            st.comment_edit = Some(p);
        }
    }
    if let Some(p) = st.confirm_delete.take() {
        let question = match &p.what {
            Confirm::Version(n) => format!("Version v{n} endgültig löschen?"),
            Confirm::Branch(b) => {
                format!("Zweig „{b}“ mit allen seinen Versionen endgültig löschen?")
            }
        };
        let mut answer = None;
        Dialog::new("Löschen", DialogSize::Prompt)
            .fixed()
            .show(ctx, |ui| {
                ui.label(question);
                ui.horizontal(|ui| {
                    if ui.button("Löschen").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Abbrechen").clicked() {
                        answer = Some(false);
                    }
                });
            });
        match answer {
            Some(true) => {
                st.confirm_delete = Some(p);
                confirm_pending_delete(ws, st);
            }
            Some(false) => {}
            None => st.confirm_delete = Some(p),
        }
    }
}
