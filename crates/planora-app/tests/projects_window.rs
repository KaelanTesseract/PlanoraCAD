//! Fenster „Projekte“: Zustand und Navigation (ohne Zeichnen der Kacheln). Gelesen wird im
//! Hintergrund; Aktionen laufen über `project_actions`. Jede Bibliothek liegt unter
//! `common::temp_root`, jede Konfiguration in einem eigenen Temp-Ordner.

mod common;

use planora_app::project_actions::{self, ProjectsUi};
use planora_app::project_tiles::{Tile, TileKind};
use planora_app::projects_window::*;
use planora_app::settings_window::change_root;
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{fonts, testing::collect_texts};
use planora_project::config::Config;
use planora_project::docstore::peek;
use planora_project::library::{Library, NodePath};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const CLOSE_FIRST: &str = "Bitte zuerst die betroffenen Dokumente schließen.";

struct Env {
    ws: Workspace,
    tmp: PathBuf,
}

fn env() -> Env {
    let root = common::temp_root("planora-projekte-fenster");
    let lib = Library::open(&root).unwrap();
    let tmp = common::temp_root("planora-projekte-fenster-tmp");
    fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(Some(lib), Config::default());
    ws.config_path = tmp.join("config.json");
    Env { ws, tmp }
}

fn lib_root(ws: &Workspace) -> PathBuf {
    ws.library.as_ref().unwrap().root().path().to_path_buf()
}

fn project(ws: &Workspace, name: &str) -> NodePath {
    ws.library.as_ref().unwrap().create_project(name).unwrap()
}

fn folder(ws: &Workspace, parent: &NodePath, name: &str) -> NodePath {
    ws.library
        .as_ref()
        .unwrap()
        .create_folder(parent, name)
        .unwrap()
}

fn doc_dir(ws: &Workspace, path: &NodePath) -> PathBuf {
    ws.library.as_ref().unwrap().document_dir(path).unwrap()
}

/// Geöffnetes Fenster mit fertig gelesenem Stand.
fn opened(ws: &Workspace) -> ProjectsWindow {
    let mut w = ProjectsWindow::default();
    w.open();
    w.wait_for_scan(ws);
    w
}

fn names(w: &ProjectsWindow) -> Vec<String> {
    w.tiles().iter().map(|t| t.name.clone()).collect()
}

fn tile(w: &ProjectsWindow, name: &str) -> Tile {
    w.tiles()
        .iter()
        .find(|t| t.name == name)
        .cloned()
        .unwrap_or_else(|| panic!("Kachel „{name}“ fehlt: {:?}", names(w)))
}

fn notice(ws: &Workspace) -> String {
    match &ws.prompt {
        Some(Prompt::Notice(m)) => m.clone(),
        other => panic!("Hinweis erwartet, war {other:?}"),
    }
}

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

/// Zeichnet die Fenster der Aktionen zwei Bilder lang; liefert die Texte des zweiten.
fn dialog_texts(c: &egui::Context, ws: &mut Workspace, st: &mut ProjectsUi) -> String {
    let mut t = String::new();
    for _ in 0..2 {
        let out = c.run_ui(egui::RawInput::default(), |ui| {
            project_actions::show_dialogs(ui.ctx(), ws, st)
        });
        t = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    t
}

fn version_files(dir: &Path) -> Vec<Vec<u8>> {
    let mut files: Vec<_> = fs::read_dir(dir.join("versionen"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    files.into_iter().map(|p| fs::read(p).unwrap()).collect()
}

#[test]
fn opening_reads_the_projects_in_the_background() {
    let mut e = env();
    // in gemischter Reihenfolge angelegt, je mit einem Dokument
    for i in (0..50).rev().step_by(2).chain((0..50).step_by(2)) {
        let p = project(&e.ws, &format!("P{i:02}"));
        e.ws.save_as(&p, "Teil", "").unwrap();
    }
    let mut w = ProjectsWindow::default();
    w.set_scan_delay(Duration::from_millis(10));
    assert!(!w.is_open());
    w.open();
    assert!(w.is_open());
    let start = Instant::now();
    w.update(&e.ws);
    let took = start.elapsed();
    assert!(took < Duration::from_millis(200), "update wartete {took:?}");
    assert!(w.scan_state().is_none(), "noch nicht gelesen");
    assert!(w.tiles().is_empty());
    w.wait_for_scan(&e.ws);
    assert!(matches!(w.scan_state(), Some(Ok(_))));
    let expected: Vec<String> = (0..50).map(|i| format!("P{i:02}")).collect();
    assert_eq!(names(&w), expected);
    assert!(w.tiles().iter().all(|t| t.kind == TileKind::Project));
    // fertig gelesen: weitere Bilder lesen nicht neu
    for _ in 0..3 {
        w.update(&e.ws);
        assert!(!w.scanning());
    }
}

#[test]
fn double_click_on_a_project_goes_in_and_back_returns() {
    let mut e = env();
    let p = project(&e.ws, "P");
    let f = folder(&e.ws, &p, "F");
    e.ws.save_as(&p, "Teil", "").unwrap();
    let mut w = opened(&e.ws);
    assert!(w.location().is_empty());
    w.activate(&mut e.ws, &p);
    assert_eq!(w.location(), &p);
    assert_eq!(names(&w), ["F", "Teil"]);
    w.activate(&mut e.ws, &f);
    assert_eq!(w.location(), &f);
    assert!(w.tiles().is_empty());
    w.back();
    assert_eq!(w.location(), &p);
    w.back();
    assert!(w.location().is_empty());
    assert_eq!(names(&w), ["P"]);
    w.back();
    assert!(w.location().is_empty(), "oben bleibt oben");
    // Pfadteile
    w.go_to(f.clone());
    assert_eq!(w.location(), &f);
    w.go_to(Vec::new());
    assert_eq!(names(&w), ["P"]);
    assert!(e.ws.prompt.is_none());
}

#[test]
fn activating_a_document_opens_a_tab_and_keeps_the_window_open() {
    let mut e = env();
    let p = project(&e.ws, "P");
    let doc = e.ws.save_as(&p, "Teil", "").unwrap();
    e.ws.close_tab(0);
    assert!(e.ws.tabs.iter().all(|t| t.path.is_none()));
    let mut w = opened(&e.ws);
    w.go_to(p.clone());
    w.activate(&mut e.ws, &doc);
    assert!(w.is_open(), "das Fenster bleibt offen");
    assert_eq!(e.ws.active_tab().path.as_ref(), Some(&doc));
    assert_eq!(w.ui.selected(), Some(&doc));
    assert_eq!(w.location(), &p, "ein Dokument ist kein Ort");
    assert!(e.ws.prompt.is_none(), "{:?}", e.ws.prompt);
}

#[test]
fn activating_an_open_document_only_brings_it_to_the_front() {
    let mut e = env();
    let p = project(&e.ws, "P");
    let doc = e.ws.save_as(&p, "Teil", "").unwrap();
    e.ws.new_tab();
    assert_eq!((e.ws.tabs.len(), e.ws.active), (2, 1));
    let mut w = opened(&e.ws);
    w.go_to(p);
    w.activate(&mut e.ws, &doc);
    assert_eq!(e.ws.tabs.len(), 2);
    assert_eq!(e.ws.active, 0, "der Tab des Dokuments ist vorn");
    assert!(e.ws.prompt.is_none(), "{:?}", e.ws.prompt);
}

#[test]
fn every_panel_action_is_reachable_from_tiles() {
    let c = ctx();
    let mut e = env();
    let p = project(&e.ws, "P");
    let q = project(&e.ws, "Q");
    let f = folder(&e.ws, &p, "F");
    let doc = e.ws.save_as(&p, "Teil", "").unwrap();
    e.ws.close_tab(0);
    assert_eq!(
        tile_menu(TileKind::Project),
        ["Umbenennen …", "Löschen …"],
        "Projekte lassen sich nicht verschieben"
    );
    for kind in [TileKind::Folder, TileKind::Document] {
        assert_eq!(
            tile_menu(kind),
            ["Umbenennen …", "Verschieben …", "Löschen …"]
        );
    }
    let mut w = opened(&e.ws);
    // erste Ebene: Projekt
    let tp = tile(&w, "P");
    for (entry, title) in [
        ("Umbenennen …", "Neuer Name für „P“:"),
        ("Löschen …", "„P“ in den Papierkorb verschieben?"),
    ] {
        w.apply_menu(&tp, entry);
        assert!(project_actions::dialog_open(&w.ui), "{entry}");
        let t = dialog_texts(&c, &mut e.ws, &mut w.ui);
        assert!(t.contains(title), "{entry}: {t}");
        w.ui.close_dialogs();
        assert!(!project_actions::dialog_open(&w.ui));
    }
    w.apply_menu(&tp, "Verschieben …");
    assert!(
        !project_actions::dialog_open(&w.ui),
        "kein Verschieben für Projekte"
    );
    // zweite Ebene: Ordner und Dokument
    w.activate(&mut e.ws, &p);
    for name in ["F", "Teil"] {
        let t = tile(&w, name);
        for entry in tile_menu(t.kind) {
            w.apply_menu(&t, &entry);
            assert!(project_actions::dialog_open(&w.ui), "{name}: {entry}");
            let text = dialog_texts(&c, &mut e.ws, &mut w.ui);
            let shown = match entry.as_str() {
                "Umbenennen …" => format!("Neuer Name für „{}“:", t.path.join(" / ")),
                "Verschieben …" => format!("„{}“ verschieben nach:", t.path.join(" / ")),
                _ => format!("„{}“ in den Papierkorb verschieben?", t.path.join(" / ")),
            };
            assert!(text.contains(&shown), "{name}: {entry}: {text}");
            w.ui.close_dialogs();
        }
    }
    // Ziele zum Verschieben kommen aus dem gelesenen Baum
    assert_eq!(
        project_actions::move_targets(&w.ui, &doc),
        vec![p.clone(), f.clone(), q.clone()]
    );
    assert_eq!(project_actions::move_targets(&w.ui, &f), vec![p.clone(), q]);
    // Knöpfe
    w.new_project();
    assert!(project_actions::dialog_open(&w.ui));
    let t = dialog_texts(&c, &mut e.ws, &mut w.ui);
    assert!(t.contains("Neues Projekt") && t.contains("Anlegen"), "{t}");
    w.ui.close_dialogs();
    w.new_document();
    assert!(project_actions::dialog_open(&w.ui));
    let t = dialog_texts(&c, &mut e.ws, &mut w.ui);
    assert!(t.contains("Neues Dokument") && t.contains("In „P“:"), "{t}");
    w.ui.close_dialogs();
    w.new_folder();
    assert!(project_actions::dialog_open(&w.ui));
    let t = dialog_texts(&c, &mut e.ws, &mut w.ui);
    assert!(t.contains("Neuer Ordner") && t.contains("In „P“:"), "{t}");
    w.ui.close_dialogs();
    assert!(e.ws.prompt.is_none(), "{:?}", e.ws.prompt);
}

#[test]
fn open_documents_lock_rename_move_and_delete() {
    let mut e = env();
    let p = project(&e.ws, "P");
    let f = folder(&e.ws, &p, "F");
    let doc = e.ws.save_as(&p, "Teil", "").unwrap();
    let dir = doc_dir(&e.ws, &doc);
    let mut w = opened(&e.ws);
    for path in [&doc, &p] {
        assert!(project_actions::rename_node(&mut e.ws, &mut w.ui, path, "Neu").is_none());
        assert_eq!(notice(&e.ws), CLOSE_FIRST);
        e.ws.prompt = None;
        assert!(project_actions::move_node(&mut e.ws, &mut w.ui, path, &f).is_none());
        assert_eq!(notice(&e.ws), CLOSE_FIRST);
        e.ws.prompt = None;
        assert!(!project_actions::delete_node(&mut e.ws, &mut w.ui, path));
        assert_eq!(notice(&e.ws), CLOSE_FIRST);
        e.ws.prompt = None;
    }
    // über die Kachel und die Rückfrage
    w.go_to(p.clone());
    let t = tile(&w, "Teil");
    w.apply_menu(&t, "Löschen …");
    let mut s = common::Screen::new(1000.0, 700.0);
    let mut draw = |c: &egui::Context| project_actions::show_dialogs(c, &mut e.ws, &mut w.ui);
    let seen = s.settle(3, &mut draw);
    let at = common::find(&seen, "In den Papierkorb")
        .expect("Rückfrage")
        .rect
        .center();
    s.click(at, &mut draw);
    drop(draw);
    assert_eq!(notice(&e.ws), CLOSE_FIRST);
    assert!(dir.join("dokument.json").is_file(), "nichts gelöscht");
    assert!(lib_root(&e.ws).join("P").join("F").is_dir());
}

#[test]
fn new_document_creates_version_1_opens_it_and_selects_the_tile() {
    let mut e = env();
    let p = project(&e.ws, "P");
    // ein geänderter, ungespeicherter Tab bleibt, wie er ist
    e.ws.active_tab_mut()
        .state
        .create_box(planora_math::DVec3::splat(5.0))
        .unwrap();
    let first_id = e.ws.tabs[0].id;
    let mut w = opened(&e.ws);
    w.go_to(p.clone());
    let rev = e.ws.library_revision;
    let path = project_actions::create_document(&mut e.ws, &mut w.ui, &p, "Neu").unwrap();
    assert_eq!(path, vec!["P".to_string(), "Neu.pcad".to_string()]);
    assert_eq!(e.ws.library_revision, rev + 1);
    assert!(e.ws.prompt.is_none(), "{:?}", e.ws.prompt);
    // Version 1, leerer Stand, Kommentar leer
    let dir = doc_dir(&e.ws, &path);
    let (info, _) = peek(&dir).unwrap();
    assert_eq!(info.versions.len(), 1);
    assert_eq!(info.versions[0].number, 1);
    assert_eq!(info.versions[0].comment, "");
    assert_eq!(info.tip(&info.active_branch), Some(1));
    // gleicher Inhalt wie das erste Speichern eines neuen Tabs
    let mut reference = env();
    reference.ws.new_tab();
    let rp = project(&reference.ws, "R");
    let r = reference.ws.save_as(&rp, "Ref", "").unwrap();
    assert_eq!(
        version_files(&dir),
        version_files(&doc_dir(&reference.ws, &r))
    );
    // geöffnet, vorn, gewählt; der alte Tab unverändert
    assert_eq!(e.ws.tabs.len(), 2);
    assert_eq!(e.ws.active_tab().path.as_ref(), Some(&path));
    assert!(!e.ws.active_tab().is_dirty());
    assert_eq!(e.ws.tabs[0].id, first_id);
    assert!(e.ws.tabs[0].is_dirty() && e.ws.tabs[0].path.is_none());
    assert_eq!(w.ui.selected(), Some(&path));
    // die Kachel erscheint, die Auswahl bleibt
    w.wait_for_scan(&e.ws);
    assert_eq!(tile(&w, "Neu").kind, TileKind::Document);
    assert_eq!(w.ui.selected(), Some(&path));
    assert!(w.is_open());
}

#[test]
fn new_document_names_follow_the_folder_rules() {
    let mut e = env();
    let p = project(&e.ws, "P");
    e.ws.save_as(&p, "Teil", "").unwrap();
    let mut w = opened(&e.ws);
    let root = lib_root(&e.ws);
    let tree = || e_tree(&root);
    let before = tree();
    let rev = e.ws.library_revision;
    for bad in ["..", "a/b", "", "Teil", "x.pcad", "X.PCAD"] {
        let tabs = e.ws.tabs.len();
        assert!(
            project_actions::create_document(&mut e.ws, &mut w.ui, &p, bad).is_none(),
            "{bad}"
        );
        assert!(!notice(&e.ws).is_empty(), "{bad}");
        e.ws.prompt = None;
        assert_eq!(e.ws.tabs.len(), tabs, "{bad}");
        assert_eq!(e.ws.library_revision, rev, "{bad}");
        assert_eq!(tree(), before, "{bad}");
    }
    // in einem Dokument lässt sich nichts anlegen
    let doc = vec!["P".to_string(), "Teil.pcad".to_string()];
    assert!(project_actions::create_document(&mut e.ws, &mut w.ui, &doc, "X").is_none());
    e.ws.prompt = None;
    // über das Fenster: der Fehler erscheint, das Fenster bleibt mit dem Namen offen
    project_actions::request_new_document(&mut w.ui, p.clone(), "a/b");
    let mut s = common::Screen::new(1000.0, 700.0);
    let mut draw = |c: &egui::Context| project_actions::show_dialogs(c, &mut e.ws, &mut w.ui);
    let seen = s.settle(3, &mut draw);
    let at = common::find(&seen, "Anlegen")
        .expect("Anlegen")
        .rect
        .center();
    s.click(at, &mut draw);
    drop(draw);
    assert!(!notice(&e.ws).is_empty());
    assert!(project_actions::dialog_open(&w.ui), "das Fenster bleibt");
    assert_eq!(tree(), before);
}

/// Alle Einträge unter `root` (relativ, sortiert).
fn e_tree(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            out.push(p.strip_prefix(base).unwrap().to_path_buf());
            if p.is_dir() {
                walk(&p, base, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn a_save_as_elsewhere_shows_up_after_the_revision_changes() {
    let mut e = env();
    let p = project(&e.ws, "P");
    let mut w = opened(&e.ws);
    w.go_to(p.clone());
    assert!(w.tiles().is_empty());
    e.ws.save_as(&p, "Neu", "").unwrap();
    w.update(&e.ws);
    w.wait_for_scan(&e.ws);
    assert_eq!(names(&w), ["Neu"]);
    // auf der Platte von außen angelegt: erst nach „Aktualisieren“
    folder(&e.ws, &p, "Außen");
    w.update(&e.ws);
    w.wait_for_scan(&e.ws);
    assert_eq!(names(&w), ["Neu"], "kein Neulesen ohne Anlass");
    w.ui.invalidate();
    w.wait_for_scan(&e.ws);
    assert_eq!(names(&w), ["Außen", "Neu"]);
}

#[test]
fn a_vanished_location_falls_back_to_the_parent() {
    let mut e = env();
    let p = project(&e.ws, "P");
    let f = folder(&e.ws, &p, "F");
    let g = folder(&e.ws, &f, "G");
    e.ws.save_as(&p, "Teil", "").unwrap();
    let mut w = opened(&e.ws);
    w.go_to(g.clone());
    assert_eq!(w.location(), &g);
    fs::remove_dir_all(lib_root(&e.ws).join("P").join("F")).unwrap();
    w.ui.invalidate();
    w.wait_for_scan(&e.ws);
    assert_eq!(w.location(), &p);
    assert_eq!(names(&w), ["Teil"]);
    // ganz verschwunden: oben
    w.go_to(p.clone());
    e.ws.close_tab(0);
    fs::remove_dir_all(lib_root(&e.ws).join("P")).unwrap();
    w.ui.invalidate();
    w.wait_for_scan(&e.ws);
    assert!(w.location().is_empty());
    assert!(w.tiles().is_empty());
    // ein Ort, den es nicht gibt
    w.go_to(vec!["Gibt es nicht".into()]);
    assert!(w.location().is_empty());
}

/// Zweite Bibliothek „B“ mit einem Projekt „P“ (anderer Inhalt als in der ersten).
fn second_library(e: &Env) -> PathBuf {
    let b = e.tmp.join("B");
    let lib = Library::open(&b).unwrap();
    let p = lib.create_project("P").unwrap();
    lib.create_folder(&p, "Nur in B").unwrap();
    lib.create_project("Zweites").unwrap();
    b
}

#[test]
fn changing_the_directory_resets_location_and_selection() {
    let mut e = env();
    let p = project(&e.ws, "P");
    let doc = e.ws.save_as(&p, "Teil", "").unwrap();
    e.ws.close_tab(0);
    let b = second_library(&e);
    let b_root = Library::open(&b).unwrap().root().path().to_path_buf();
    let mut w = opened(&e.ws);
    w.go_to(p.clone());
    w.ui.select(Some(doc.clone()));
    let _ = change_root(&mut e.ws, &b);
    assert_eq!(lib_root(&e.ws), b_root, "gewechselt");
    w.update(&e.ws);
    assert!(w.location().is_empty());
    assert!(w.ui.selected().is_none());
    assert!(
        w.tiles().is_empty(),
        "keine Kacheln des alten Verzeichnisses"
    );
    w.wait_for_scan(&e.ws);
    assert_eq!(names(&w), ["P", "Zweites"]);
    w.activate(&mut e.ws, &p);
    assert_eq!(names(&w), ["Nur in B"]);
}

#[test]
fn a_directory_change_drops_pending_project_windows() {
    let c = ctx();
    let mut e = env();
    let p = project(&e.ws, "P");
    let f = folder(&e.ws, &p, "F");
    let b = second_library(&e);
    let mut w = opened(&e.ws);
    type Request = Box<dyn Fn(&mut ProjectsUi)>;
    let requests: Vec<(&str, Request)> = vec![
        (
            "Löschen",
            Box::new(|st| project_actions::request_delete(st, vec!["P".into()])),
        ),
        (
            "Umbenennen",
            Box::new(|st| project_actions::request_rename(st, vec!["P".into()], "X")),
        ),
        (
            "Verschieben",
            Box::new(|st| project_actions::request_move(st, vec!["P".into(), "F".into()])),
        ),
        (
            "Neuer Ordner",
            Box::new(|st| project_actions::request_new_folder(st, vec!["P".into()], "X")),
        ),
        (
            "Neues Dokument",
            Box::new(|st| project_actions::request_new_document(st, vec!["P".into()], "X")),
        ),
        (
            "Neues Projekt",
            Box::new(|st| project_actions::request_new_project(st, "X")),
        ),
    ];
    let first = lib_root(&e.ws);
    let mut roots = [
        first.clone(),
        Library::open(&b).unwrap().root().path().to_path_buf(),
    ];
    for (name, request) in &requests {
        request(&mut w.ui);
        w.ui.select(Some(p.clone()));
        assert!(project_actions::dialog_open(&w.ui), "{name}");
        // Wechsel (die Warnung „nicht gespeichert“ hieße trotzdem: gewechselt)
        let _ = change_root(&mut e.ws, &roots[1]);
        assert_eq!(lib_root(&e.ws), roots[1], "{name}");
        // ohne `update`: auch das Zeichnen zeigt das alte Fenster nicht mehr
        let t = dialog_texts(&c, &mut e.ws, &mut w.ui);
        assert!(!t.contains("Abbrechen"), "{name}: {t}");
        assert!(!project_actions::dialog_open(&w.ui), "{name}");
        assert!(w.ui.selected().is_none(), "{name}");
        w.update(&e.ws);
        assert!(!project_actions::dialog_open(&w.ui), "{name}");
        w.wait_for_scan(&e.ws);
        roots.swap(0, 1);
    }
    // nichts wurde gelöscht, umbenannt, verschoben oder angelegt
    assert!(first.join("P").join("F").is_dir());
    assert!(b.join("P").join("Nur in B").is_dir());
    assert!(!first.join("X").exists() && !b.join("X").exists());
    assert!(!b.join("P").join("X").exists() && !first.join("P").join("X").exists());
    assert!(!b.join("P").join("X.pcad").exists());
    let _ = f;
    // ebenso über `update` allein (Fenster mit Baum)
    project_actions::request_delete(&mut w.ui, vec!["P".into()]);
    e.ws.choose_root(&roots[1]).unwrap();
    w.update(&e.ws);
    assert!(!project_actions::dialog_open(&w.ui));
}

#[test]
fn without_a_library_there_is_no_scan() {
    let tmp = common::temp_root("planora-projekte-fenster-ohne");
    fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(None, Config::default());
    ws.config_path = tmp.join("config.json");
    let mut w = ProjectsWindow::default();
    w.open();
    w.update(&ws);
    assert!(w.scan_state().is_none());
    assert!(!w.scanning());
    let start = Instant::now();
    w.wait_for_scan(&ws);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(w.scan_state().is_none());
    assert!(w.tiles().is_empty());
    assert!(project_actions::no_library().contains("Projektverzeichnis …"));
}

#[test]
fn the_new_project_and_new_document_windows_are_not_drawn_under_a_prompt() {
    let c = ctx();
    let mut e = env();
    let p = project(&e.ws, "P");
    let mut w = opened(&e.ws);
    for which in 0..2 {
        if which == 0 {
            w.new_project();
        } else {
            w.go_to(p.clone());
            w.new_document();
        }
        e.ws.prompt = Some(Prompt::Notice("Hinweis".into()));
        let t = dialog_texts(&c, &mut e.ws, &mut w.ui);
        assert!(!t.contains("Anlegen"), "{which}: {t}");
        assert!(
            project_actions::dialog_open(&w.ui),
            "der Zustand bleibt erhalten"
        );
        e.ws.prompt = None;
        let t = dialog_texts(&c, &mut e.ws, &mut w.ui);
        assert!(t.contains("Anlegen"), "{which}: {t}");
        w.ui.close_dialogs();
    }
}

/// Bibliothek mit `n` Projekten je einem Dokument (damit das Lesen über `set_scan_delay` dauert).
fn with_documents(e: &mut Env, n: usize) {
    for i in 0..n {
        let p = project(&e.ws, &format!("P{i}"));
        e.ws.save_as(&p, "Teil", "").unwrap();
    }
    e.ws.close_tab(0);
}

#[test]
fn an_invalidate_during_a_running_scan_is_not_lost() {
    let mut e = env();
    with_documents(&mut e, 6);
    let mut w = ProjectsWindow::default();
    w.set_scan_delay(Duration::from_millis(80));
    w.open();
    w.update(&e.ws);
    assert!(w.scanning());
    // der Baum ist gelesen, die Dokumente noch nicht: jetzt ändert sich die Platte
    std::thread::sleep(Duration::from_millis(120));
    project(&e.ws, "Außen");
    w.ui.invalidate();
    w.update(&e.ws);
    assert!(w.scanning());
    w.wait_for_scan(&e.ws);
    assert!(
        names(&w).contains(&"Außen".to_string()),
        "das alte Lesen gilt nicht als aktuell: {:?}",
        names(&w)
    );
    assert!(!w.ui.is_stale(&e.ws));
}

#[test]
fn close_cancels_the_running_scan() {
    let mut e = env();
    with_documents(&mut e, 6);
    let mut w = ProjectsWindow::default();
    w.set_scan_delay(Duration::from_millis(80));
    w.open();
    w.update(&e.ws);
    assert!(w.scanning());
    w.close();
    assert!(!w.is_open());
    assert!(!w.scanning(), "Schließen bricht das Lesen ab");
    w.update(&e.ws);
    assert!(!w.scanning(), "ein geschlossenes Fenster liest nicht");
    assert!(w.scan_state().is_none());
}

#[test]
fn open_documents_lock_their_paths_regardless_of_case() {
    let mut e = env();
    let p = project(&e.ws, "P");
    let doc = e.ws.save_as(&p, "Teil", "").unwrap();
    let dir = doc_dir(&e.ws, &doc);
    let mut w = opened(&e.ws);
    let lower_project = vec!["p".to_string()];
    let lower_doc = vec!["p".to_string(), "teil.pcad".to_string()];
    for path in [&lower_project, &lower_doc] {
        assert!(
            !project_actions::delete_node(&mut e.ws, &mut w.ui, path),
            "{path:?}"
        );
        assert_eq!(notice(&e.ws), CLOSE_FIRST, "{path:?}");
        e.ws.prompt = None;
        assert!(project_actions::rename_node(&mut e.ws, &mut w.ui, path, "Neu").is_none());
        assert_eq!(notice(&e.ws), CLOSE_FIRST, "{path:?}");
        e.ws.prompt = None;
    }
    assert!(dir.join("dokument.json").is_file(), "nichts gelöscht");
}

#[test]
fn deleting_drops_a_selection_below_but_keeps_one_elsewhere() {
    let mut e = env();
    let p = project(&e.ws, "P");
    let f = folder(&e.ws, &p, "F");
    let g = folder(&e.ws, &f, "G");
    let other = folder(&e.ws, &p, "Anderes");
    let mut w = opened(&e.ws);
    w.ui.select(Some(g.clone()));
    assert!(project_actions::delete_node(&mut e.ws, &mut w.ui, &f));
    assert!(
        w.ui.selected().is_none(),
        "die Auswahl lag im gelöschten Ordner"
    );
    w.ui.select(Some(other.clone()));
    let q = folder(&e.ws, &p, "Q");
    assert!(project_actions::delete_node(&mut e.ws, &mut w.ui, &q));
    assert_eq!(w.ui.selected(), Some(&other), "andere Auswahl bleibt");
}

#[test]
fn document_names_ending_in_the_suffix_are_refused() {
    let mut e = env();
    let p = project(&e.ws, "P");
    let mut w = opened(&e.ws);
    for bad in ["x.pcad", "X.PCAD", "Teil.Pcad"] {
        assert!(project_actions::create_document(&mut e.ws, &mut w.ui, &p, bad).is_none());
        assert_eq!(
            notice(&e.ws),
            planora_app::workspace::document_name_suffix()
        );
        e.ws.prompt = None;
    }
    assert!(fs::read_dir(lib_root(&e.ws).join("P")).unwrap().all(|x| {
        !x.unwrap()
            .file_name()
            .to_string_lossy()
            .to_lowercase()
            .ends_with(".pcad")
    }));
}

#[test]
fn the_move_window_says_when_the_targets_are_still_being_read() {
    let c = ctx();
    let mut e = env();
    let p = project(&e.ws, "P");
    let doc = e.ws.save_as(&p, "Teil", "").unwrap();
    e.ws.close_tab(0);
    // noch nichts gelesen
    let mut st = ProjectsUi::default();
    project_actions::request_move(&mut st, doc.clone());
    let t = dialog_texts(&c, &mut e.ws, &mut st);
    assert!(t.contains(&project_actions::targets_pending()), "{t}");
    // gelesen: kein Hinweis
    let mut w = opened(&e.ws);
    project_actions::request_move(&mut w.ui, doc);
    let t = dialog_texts(&c, &mut e.ws, &mut w.ui);
    assert!(!t.contains(&project_actions::targets_pending()), "{t}");
    assert!(t.contains("verschieben nach:"), "{t}");
}
