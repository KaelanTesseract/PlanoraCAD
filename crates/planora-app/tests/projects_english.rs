//! Projekte, Dateien, Versionen und Verlauf auf Englisch (Sprachplan, Task 6): unter
//! `with_lang(&english(), …)` gezeichnet, ohne Fenster. Kein Test ruft `set_language` oder
//! `set_global` (Ruling 4): die Sprache gilt nur für den Thread des Tests.
//!
//! Dazu `stored_labels_keep_their_language` (Review Focus 4): Namen, die beim Anlegen gespeichert
//! werden (Rückgängig-Beschriftungen, vorgeschlagene Namen), bleiben in der Sprache ihres
//! Entstehens.

mod common;

use common::{Seen, assert_english, assert_has, join};
use egui::{Event, Pos2, Rect, vec2};
use planora_app::file_chooser::{self, ChooserAction, ChooserMode, Entry, FileChooser};
use planora_app::history_bar;
use planora_app::places::{PlaceKind, Places};
use planora_app::project_actions::{self, ProjectsUi};
use planora_app::project_tiles::{self, TileKind};
use planora_app::projects_window::{self, ProjectsWindow};
use planora_app::state::{AppState, StandardPlane};
use planora_app::versions_panel::{self, VersionsUi};
use planora_app::workspace::Workspace;
use planora_app::{browser, fonts, theme, undo_labels};
use planora_feature::{FeatureId, SketchPlane};
use planora_i18n::{english, german, with_lang};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::docstore::DocStore;
use planora_project::library::{Library, NodePath};
use std::path::PathBuf;

// ---- Umgebung --------------------------------------------------------------------------------

fn workspace(prefix: &str) -> Workspace {
    let root = common::temp_root(prefix);
    let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let tmp = common::temp_root(&format!("{prefix}-cfg"));
    std::fs::create_dir_all(&tmp).unwrap();
    ws.config_path = tmp.join("config.json");
    ws
}

fn lib(ws: &Workspace) -> &Library {
    ws.library.as_ref().unwrap()
}

/// Dokument `name` in `parent` mit einem Quader (ohne Tab).
fn box_doc(ws: &Workspace, parent: &NodePath, name: &str) -> NodePath {
    let mut s = AppState::new();
    s.create_box(DVec3::new(20.0, 10.0, 8.0)).unwrap();
    let mut path = parent.clone();
    path.push(format!("{name}.pcad"));
    let dir = lib(ws).root().resolve(&path).unwrap();
    drop(DocStore::create(&dir, &s.snapshot(), "").unwrap());
    path
}

fn new_ctx() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    ctx
}

/// Ein Fenster ohne Bildschirm mit eigener Zeit (0,05 s je Bild).
struct Draw {
    ctx: egui::Context,
    size: egui::Vec2,
    time: f64,
}

impl Draw {
    fn new() -> Draw {
        Draw {
            ctx: new_ctx(),
            size: vec2(1920.0, 1080.0),
            time: 0.0,
        }
    }

    fn frame(&mut self, events: Vec<Event>, f: &mut dyn FnMut(&mut egui::Ui)) -> Vec<Seen> {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let out = self.ctx.run_ui(raw, |ui| f(ui));
        let seen = common::collect_seen(&out.shapes);
        out.drop_without_applying_deltas();
        seen
    }

    fn settle(&mut self, n: usize, f: &mut dyn FnMut(&mut egui::Ui)) -> Vec<Seen> {
        let mut seen = Vec::new();
        for _ in 0..n {
            seen = self.frame(vec![], f);
        }
        seen
    }

    fn pointer(&mut self, at: Pos2, button: egui::PointerButton, f: &mut dyn FnMut(&mut egui::Ui)) {
        let ev = |pressed| Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.settle(8, f);
        self.frame(vec![Event::PointerMoved(at)], f);
        self.frame(vec![ev(true)], f);
        self.frame(vec![ev(false)], f);
    }
}

fn center_of(seen: &[Seen], text: &str) -> Pos2 {
    seen.iter()
        .find(|s| s.text == text)
        .unwrap_or_else(|| panic!("„{text}“ fehlt in {}", join(seen)))
        .rect
        .center()
}

// ---- Fenster „Projekte“ ------------------------------------------------------------------------

struct Pw {
    d: Draw,
    w: ProjectsWindow,
    ws: Workspace,
}

impl Pw {
    fn new(ws: Workspace) -> Pw {
        let mut w = ProjectsWindow::default();
        w.open();
        Pw {
            d: Draw::new(),
            w,
            ws,
        }
    }

    fn draw(&mut self, n: usize) -> Vec<Seen> {
        let (w, ws) = (&mut self.w, &mut self.ws);
        self.d.settle(n, &mut |ui| {
            w.show(ui.ctx(), ws);
        })
    }

    fn ready(&mut self) -> Vec<Seen> {
        self.draw(2);
        self.w.wait_for_scan(&self.ws);
        self.draw(4)
    }

    fn click(&mut self, at: Pos2, button: egui::PointerButton) -> Vec<Seen> {
        let (w, ws) = (&mut self.w, &mut self.ws);
        self.d.pointer(at, button, &mut |ui| {
            w.show(ui.ctx(), ws);
        });
        self.draw(3)
    }
}

#[test]
fn projects_window_speaks_english() {
    with_lang(&english(), || {
        // Ohne Projektverzeichnis.
        let mut pw = Pw::new(Workspace::new(None, Config::default()));
        let seen = pw.ready();
        assert_has(
            "ohne Verzeichnis",
            &seen,
            &[
                "Projects",
                "New project",
                "Refresh",
                "Project folder …",
                "No project folder has been chosen yet. Choose the folder for your projects with \"Project folder …\".",
            ],
        );
        assert_english("ohne Verzeichnis", &seen, &[], &[]);

        // Mit Projekten: eines mit Dokument, ein leeres.
        let ws = workspace("planora-projects-english");
        let p = lib(&ws).create_project("P").unwrap();
        let q = lib(&ws).create_project("Leer").unwrap();
        let f = lib(&ws).create_folder(&p, "F").unwrap();
        box_doc(&ws, &p, "Teil");
        let mut pw = Pw::new(ws);
        let seen = pw.ready();
        assert_has(
            "Ebene 1",
            &seen,
            &[
                "Projects",
                "New project",
                "Refresh",
                "Double-click: open · Right-click: more commands",
                "2 projects",
                "No document yet",
                "Version 1 · 1 document",
            ],
        );
        assert!(
            seen.iter().any(|s| s.text.starts_with("Changed ")),
            "{}",
            join(&seen)
        );
        assert_english("Ebene 1", &seen, &[], &[]);

        // Rechtsklick auf ein Projekt: Menü und die Fenster der Aktionen.
        let seen = pw.click(center_of(&seen, "P"), egui::PointerButton::Secondary);
        assert_has("Menü Projekt", &seen, &["Rename …", "Delete …"]);
        assert!(!seen.iter().any(|s| s.text == "Move …"), "{}", join(&seen));
        assert_english("Menü Projekt", &seen, &[], &[]);
        let seen = pw.click(center_of(&seen, "Rename …"), egui::PointerButton::Primary);
        assert_has(
            "Umbenennen",
            &seen,
            &["Rename", "New name for \"P\":", "Cancel"],
        );
        assert_english("Umbenennen", &seen, &[], &[]);
        pw.w.ui.close_dialogs();
        project_actions::request_delete(&mut pw.w.ui, p.clone());
        let seen = pw.draw(3);
        assert_has(
            "Löschen",
            &seen,
            &[
                "Delete",
                "Move \"P\" to the trash?",
                "Move to trash",
                "Cancel",
            ],
        );
        assert_english("Löschen", &seen, &[], &[]);
        pw.w.ui.close_dialogs();
        project_actions::request_move(&mut pw.w.ui, f.clone());
        let seen = pw.draw(3);
        assert_has(
            "Verschieben",
            &seen,
            &["Move", "Move \"P / F\" to:", "Choose target"],
        );
        assert_english("Verschieben", &seen, &[], &[]);
        pw.w.ui.close_dialogs();
        project_actions::request_new_project(&mut pw.w.ui, "");
        let seen = pw.draw(3);
        assert_has("Neues Projekt", &seen, &["New project", "Create", "Cancel"]);
        assert_english("Neues Projekt", &seen, &[], &[]);
        pw.w.ui.close_dialogs();

        // Zweite Ebene: leeres Projekt, Knöpfe, Fenster mit Ort.
        pw.w.go_to(q.clone());
        let seen = pw.ready();
        assert_has(
            "Ebene 2",
            &seen,
            &[
                "New document",
                "New folder",
                "This project is still empty. Create a document or a folder above.",
                "Projects",
            ],
        );
        assert_english("Ebene 2", &seen, &[], &[]);
        project_actions::request_new_document(&mut pw.w.ui, q.clone(), "");
        let seen = pw.draw(3);
        assert_has("Neues Dokument", &seen, &["In \"Leer\":", "Create"]);
        assert_english("Neues Dokument", &seen, &[], &[]);
        pw.w.ui.close_dialogs();
        pw.w.go_to(p.clone());
        let seen = pw.ready();
        assert_has("Ebene 2 mit Inhalt", &seen, &["1 folder · 1 document", "F"]);
        assert_english("Ebene 2 mit Inhalt", &seen, &[], &[]);

        // Lesefehler: das Verzeichnis ist verschwunden.
        let root = lib(&pw.ws).root().path().to_path_buf();
        std::fs::remove_dir_all(&root).unwrap();
        pw.w.ui.invalidate();
        let seen = pw.ready();
        let error = seen
            .iter()
            .find(|s| s.text.starts_with("The project folder cannot be read: "))
            .unwrap_or_else(|| panic!("Lesefehler fehlt in {}", join(&seen)));
        assert!(error.text.ends_with("was not found."), "{}", error.text);
        assert_english("Lesefehler", &seen, &[], &[]);

        // Texte der Kachelzeilen und Menüs ohne Fenster.
        let labels: Vec<String> = projects_window::tile_menu(TileKind::Folder)
            .into_iter()
            .map(projects_window::TileMenu::label)
            .collect();
        assert_eq!(labels, ["Rename …", "Move …", "Delete …"]);
        assert_eq!(project_tiles::documents_text(1), "1 document");
        assert_eq!(project_tiles::documents_text(3), "3 documents");
        assert_eq!(project_tiles::too_many(), "Only the first 500 are shown.");
        assert_eq!(project_tiles::cancelled(), "Cancelled");
        assert_eq!(projects_window::no_bodies(), "No bodies yet");
        assert_eq!(projects_window::too_complex(), "Thumbnail too complex");
        assert_eq!(projects_window::building_text(), "Creating thumbnail …");
        assert_eq!(
            planora_app::thumbnail_cache::impossible_text(),
            "Thumbnail not possible"
        );
        assert_eq!(projects_window::back_hint(), "Up one level");
        assert_eq!(projects_window::reading(), "Reading the projects …");
        assert_eq!(
            projects_window::empty(),
            "No projects yet. Create the first one with \"New project\"."
        );
        assert_eq!(
            projects_window::empty_folder(),
            "This folder is still empty. Create a document or a folder above."
        );
        assert_eq!(
            projects_window::scan_lost(),
            "Reading the projects ended unexpectedly."
        );
        assert_eq!(
            project_actions::read_error_prefix(),
            "The project folder cannot be read: "
        );
        assert_eq!(project_actions::targets_pending(), "Reading the targets …");

        // Hinweise der Aktionen.
        let mut ws = workspace("planora-actions-english");
        let mut st = ProjectsUi::default();
        assert!(!project_actions::delete_node(&mut ws, &mut st, &Vec::new()));
        assert_notice(&ws, "Nothing selected.");
        let p = lib(&ws).create_project("P").unwrap();
        let doc = box_doc(&ws, &p, "Teil");
        assert!(project_actions::rename_node(&mut ws, &mut st, &doc, "Neu.pcad").is_none());
        assert_notice(&ws, "Document names must not end in \".pcad\".");
        ws.prompt = None;
        project_actions::open_node(&mut ws, &doc);
        ws.prompt = None;
        assert!(!project_actions::delete_node(&mut ws, &mut st, &doc));
        assert_notice(&ws, "Please close the affected documents first.");
        let mut none = Workspace::new(None, Config::default());
        let mut st = ProjectsUi::default();
        assert!(project_actions::create_project(&mut none, &mut st, "X").is_none());
        assert_notice(&none, "No project folder has been chosen yet.");
    });
}

/// Review Focus 5: bei offenem Fenster wechseln alle Texte im nächsten Bild; im Zustand steht
/// kein übersetzter Text.
#[test]
fn open_windows_follow_a_language_switch() {
    let ws = workspace("planora-switch-english");
    let p = lib(&ws).create_project("P").unwrap();
    box_doc(&ws, &p, "Teil");
    let mut pw = Pw::new(ws);
    let seen = with_lang(&german(), || pw.ready());
    assert_has(
        "deutsch",
        &seen,
        &["Projekte", "Neues Projekt", "Aktualisieren", "1 Projekt"],
    );
    project_actions::request_delete(&mut pw.w.ui, p.clone());
    let seen = with_lang(&german(), || pw.draw(3));
    assert_has("deutsch", &seen, &["„P“ in den Papierkorb verschieben?"]);
    let seen = with_lang(&english(), || pw.draw(3));
    assert_has(
        "englisch",
        &seen,
        &[
            "Projects",
            "New project",
            "Refresh",
            "1 project",
            "Move to trash",
        ],
    );
    assert_english("englisch", &seen, &[], &[]);
    // Ein Rechtsklick-Menü, das im Deutschen geöffnet wurde, gilt im Englischen weiter.
    pw.w.ui.close_dialogs();
    let at = with_lang(&german(), || pw.draw(2));
    let at = center_of(&at, "P");
    with_lang(&german(), || pw.click(at, egui::PointerButton::Secondary));
    let seen = with_lang(&english(), || pw.draw(3));
    assert_has("Menü", &seen, &["Rename …", "Delete …"]);

    // Ein Lesefehler, der deutsch entstand, wird englisch gezeichnet (gespeichert ist der Grund).
    pw.w.ui.close_dialogs();
    let root = lib(&pw.ws).root().path().to_path_buf();
    std::fs::remove_dir_all(&root).unwrap();
    pw.w.ui.invalidate();
    let seen = with_lang(&german(), || pw.ready());
    assert!(
        seen.iter().any(|s| s
            .text
            .starts_with("Das Projektverzeichnis lässt sich nicht lesen: ")),
        "{}",
        join(&seen)
    );
    let seen = with_lang(&english(), || pw.draw(3));
    let error = seen
        .iter()
        .find(|s| s.text.starts_with("The project folder cannot be read: "))
        .unwrap_or_else(|| panic!("Lesefehler fehlt in {}", join(&seen)));
    assert!(error.text.ends_with("was not found."), "{}", error.text);
}

fn assert_notice(ws: &Workspace, want: &str) {
    match &ws.prompt {
        Some(planora_app::workspace::Prompt::Notice(m)) => {
            assert_eq!(m, want);
            assert!(!common::looks_like_key(m));
        }
        other => panic!("Hinweis „{want}“ erwartet, gefunden: {other:?}"),
    }
}

// ---- Dateiauswahl ----------------------------------------------------------------------------

fn chooser_dir(name: &str) -> PathBuf {
    let d = common::temp_root(name);
    std::fs::create_dir_all(d.join("a")).unwrap();
    std::fs::write(d.join("teil.stl"), b"x").unwrap();
    std::path::absolute(&d).unwrap()
}

struct Fc {
    d: Draw,
    places: Places,
}

impl Fc {
    fn new() -> Fc {
        Fc {
            d: Draw::new(),
            places: Places {
                drives: Some(vec![]),
                ..Places::default()
            },
        }
    }

    fn show(&mut self, c: &mut FileChooser) -> Vec<Seen> {
        let places = &mut self.places;
        self.d.settle(3, &mut |ui| {
            let _ = c.show_with(ui, places);
        })
    }
}

#[test]
fn file_chooser_speaks_english() {
    // Das Anlegen in Deutsch, das Zeichnen in Englisch: bekannte Ordner und Meldungen (gespeichert
    // wird der Grund, `ChooserError`) folgen der Sprache des Bildes (Review Focus 5).
    let dir = chooser_dir("planora-chooser-english");
    let mut save = with_lang(&german(), || {
        FileChooser::save(dir.clone(), String::new(), "stl")
    });
    with_lang(&english(), || {
        let mut fc = Fc::new();
        fc.places.project_root = Some(dir.clone());
        let seen = fc.show(&mut save);
        assert_has(
            "Speichern",
            &seen,
            &["Up one folder", "File name", "Save", "Cancel", "Projects"],
        );
        assert_english("Speichern", &seen, &[], &[]);
        // Bekannte Ordner tragen die englischen Namen, auch wenn die Auswahl deutsch angelegt wurde.
        for b in save.place_buttons(&fc.places, &[]) {
            let want = match b.place.kind {
                PlaceKind::Documents => "Documents",
                PlaceKind::Downloads => "Downloads",
                PlaceKind::Desktop => "Desktop",
                PlaceKind::Pictures => "Pictures",
                PlaceKind::Project => "Projects",
                _ => continue,
            };
            assert_eq!(b.place.label, want, "{:?}", b.place);
        }

        // Die Meldung wird deutsch ausgelöst und englisch gezeichnet: gespeichert ist der Grund.
        save.file_name = "a?b".into();
        with_lang(&german(), || {
            assert_eq!(save.confirm(), ChooserAction::None)
        });
        assert_eq!(
            save.error_text().as_deref(),
            Some(
                "Please enter a valid file name (without special characters such as : ? * or slashes)."
            )
        );
        assert_eq!(
            with_lang(&german(), || save.error_text()).as_deref(),
            Some(
                "Bitte einen gültigen Dateinamen eingeben (ohne Sonderzeichen wie : ? * oder Schrägstriche)."
            )
        );
        let seen = fc.show(&mut save);
        assert_has(
            "Name",
            &seen,
            &[
                "Please enter a valid file name (without special characters such as : ? * or slashes).",
            ],
        );
        assert_english("Name", &seen, &[], &[]);
        save.file_name = "teil".into();
        assert_eq!(save.confirm(), ChooserAction::None);
        assert!(save.asks_to_replace());
        let seen = fc.show(&mut save);
        assert_has(
            "Ersetzen",
            &seen,
            &[
                "This file already exists. Click again to replace it.",
                "Replace",
            ],
        );
        assert_english("Ersetzen", &seen, &[], &[]);

        // Weitere Gründe: Favorit fehlt, Ordner nicht lesbar.
        save.error = Some(file_chooser::ChooserError::FavoriteMissing);
        assert!(
            save.error_text()
                .unwrap()
                .starts_with("This folder does not exist.")
        );
        save.error = Some(file_chooser::ChooserError::Io(std::io::ErrorKind::NotFound));
        assert_eq!(
            save.error_text().as_deref(),
            Some("The folder does not exist.")
        );

        // Der Ordner aus der Eingabezeile fehlt.
        save.dir_text = dir.join("gibt-es-nicht").display().to_string();
        assert!(!save.go_to_text());
        let seen = fc.show(&mut save);
        assert_has("Ordner fehlt", &seen, &["This folder does not exist."]);
        assert_english("Ordner fehlt", &seen, &[], &[]);

        // Öffnen: keine Datei gewählt, keine passenden Dateien.
        let mut open = FileChooser::open(dir.join("a"), vec!["step"]);
        let seen = fc.show(&mut open);
        assert_has(
            "Öffnen",
            &seen,
            &["Open", "File name", "There are no matching files here."],
        );
        assert_english("Öffnen", &seen, &[], &[]);
        assert_eq!(open.confirm(), ChooserAction::None);
        let seen = fc.show(&mut open);
        assert_has(
            "Öffnen ohne Datei",
            &seen,
            &["Please choose an existing file."],
        );

        // Ordner-Modus.
        let mut folder = FileChooser::folder(dir.clone());
        let seen = fc.show(&mut folder);
        assert_has("Ordner", &seen, &["Choose this folder", "Cancel", "a"]);
        assert!(
            seen.iter().any(|s| s.text.starts_with("Chosen: ")),
            "{}",
            join(&seen)
        );
        assert_english("Ordner", &seen, &[], &[]);
        folder.enter("a");
        let seen = fc.show(&mut folder);
        assert_has("Leerer Ordner", &seen, &["There are no subfolders here."]);
        folder.mark("nichts");
        assert_eq!(
            file_chooser::no_folder(),
            "Please choose an existing folder."
        );

        // Hinweis beim Überfahren eines Ordners und der Rest der Texte.
        let e = Entry {
            name: "a".into(),
            is_dir: true,
        };
        assert_eq!(
            file_chooser::entry_hover(&e).as_deref(),
            Some("Double-click to open")
        );
        assert_eq!(folder.mode, ChooserMode::Folder);
    });
}

// ---- Versionen -------------------------------------------------------------------------------

fn ws_with_versions(prefix: &str, versions: usize) -> Workspace {
    let mut w = workspace(prefix);
    let p = lib(&w).create_project("P").unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    for i in 1..versions {
        w.active_tab_mut()
            .state
            .create_box(DVec3::splat(2.0 + i as f64))
            .unwrap();
        w.save_active(&format!("Stand {}", i + 1), None).unwrap();
    }
    w
}

fn versions_frame(d: &mut Draw, ws: &mut Workspace, st: &mut VersionsUi) -> Vec<Seen> {
    d.settle(3, &mut |ui| {
        egui::CentralPanel::default().show(ui, |ui| versions_panel::show(ui, ws, st));
    })
}

#[test]
fn versions_panel_speaks_english() {
    with_lang(&english(), || {
        let mut d = Draw::new();
        // Ungespeichert.
        let mut ws = Workspace::new(None, Config::default());
        let mut st = VersionsUi::default();
        let seen = versions_frame(&mut d, &mut ws, &mut st);
        assert_has(
            "ungespeichert",
            &seen,
            &["This document has not been saved yet."],
        );
        assert_english("ungespeichert", &seen, &[], &[]);

        // Liste mit ungespeicherten Änderungen.
        let mut ws = ws_with_versions("planora-versions-english", 3);
        ws.active_tab_mut()
            .state
            .create_box(DVec3::splat(9.0))
            .unwrap();
        let seen = versions_frame(&mut d, &mut ws, &mut st);
        assert_has("Liste", &seen, &["Versions", "Changes (not saved)"]);
        assert_english("Liste", &seen, &[], &["v1", "v2", "v3"]);

        // Kontextmenü einer Version.
        let row = seen
            .iter()
            .find(|s| s.text.contains("v3"))
            .unwrap_or_else(|| panic!("Zeile v3 fehlt in {}", join(&seen)))
            .rect
            .center();
        let mut f = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| versions_panel::show(ui, &mut ws, &mut st));
        };
        d.pointer(row, egui::PointerButton::Secondary, &mut f);
        let seen = d.settle(3, &mut f);
        assert_has(
            "Menü",
            &seen,
            &[
                "Open",
                "Restore",
                "Branch off",
                "Edit comment …",
                "Delete …",
                "Delete branch …",
                "Switch to this branch",
            ],
        );
        assert_english("Menü", &seen, &[], &["v1", "v2", "v3"]);

        // Fenster „Kommentar ändern“ und die Rückfrage.
        let mut ws = ws_with_versions("planora-versions-english2", 3);
        let mut st = VersionsUi::default();
        versions_panel::request_comment(&mut ws, &mut st, 2, "Stand 2");
        let seen = versions_frame(&mut d, &mut ws, &mut st);
        assert_has(
            "Kommentar",
            &seen,
            &["Edit comment", "Comment on v2", "Apply", "Cancel"],
        );
        assert_english("Kommentar", &seen, &[], &["v1", "v2", "v3"]);
        let mut st = VersionsUi::default();
        versions_panel::request_delete_version(&mut ws, &mut st, 2);
        let seen = versions_frame(&mut d, &mut ws, &mut st);
        assert_has(
            "Löschen",
            &seen,
            &["Delete", "Delete version v2 permanently?", "Cancel"],
        );
        assert_english("Löschen", &seen, &[], &["v1", "v2", "v3"]);
        let mut st = VersionsUi::default();
        versions_panel::request_delete_branch(&mut ws, &mut st, "Haupt");
        let seen = versions_frame(&mut d, &mut ws, &mut st);
        assert_has(
            "Zweig löschen",
            &seen,
            &["Delete branch \"Haupt\" with all its versions permanently?"],
        );

        // Alte Version: Hinweis und Knöpfe.
        let mut ws = ws_with_versions("planora-versions-english3", 2);
        let mut st = VersionsUi::default();
        versions_panel::open_version(&mut ws, 1);
        let seen = versions_frame(&mut d, &mut ws, &mut st);
        assert_has(
            "alte Version",
            &seen,
            &["You are viewing an old version.", "Restore", "Branch off"],
        );
        assert_english("alte Version", &seen, &[], &["v1", "v2"]);

        // Hinweise, wenn nichts bearbeitbar ist.
        let mut ws = Workspace::new(None, Config::default());
        versions_panel::set_comment(&mut ws, 1, "x");
        assert_notice(&ws, "The document has not been saved yet.");
        let mut ws = ws_with_versions("planora-versions-english4", 1);
        // Nur die Ansicht bleibt offen: sie hat keinen beschreibbaren Tab.
        versions_panel::open_version(&mut ws, 1);
        ws.close_tab(0);
        ws.prompt = None;
        versions_panel::set_comment(&mut ws, 1, "x");
        assert_notice(
            &ws,
            "To change anything, open the document for editing first.",
        );
    });
}

// ---- Workspace-Meldungen ---------------------------------------------------------------------

#[test]
fn workspace_messages_speak_english() {
    with_lang(&english(), || {
        use planora_app::workspace as w;
        assert_eq!(w::nothing_to_undo(), "There is nothing to undo.");
        assert_eq!(w::nothing_to_redo(), "There is nothing to redo.");
        assert_eq!(w::export_writing(), "This is not possible while exporting.");
        assert_eq!(w::no_root(), "No project folder has been chosen yet.");
        assert_eq!(
            w::document_name_suffix(),
            "Document names must not end in \".pcad\"."
        );

        // Vorgeschlagener Name eines neuen Tabs: in der Sprache des Anlegens.
        let mut ws = workspace("planora-workspace-english");
        let i = ws.new_tab();
        assert_eq!(ws.tabs[i].name, "Untitled 2");
        assert_eq!(with_lang(&german(), || ws.new_tab()), i + 1);
        assert_eq!(ws.tabs[i + 1].name, "Unbenannt 3");

        // Fehlermeldungen.
        assert_eq!(ws.make_editable(99), Err("No such tab.".to_string()));
        assert_eq!(ws.restore_version(99, 1), Err("No such tab.".to_string()));
        let p = lib(&ws).create_project("P").unwrap();
        ws.active = 0;
        assert_eq!(
            ws.save_as(&Vec::new(), "Teil", ""),
            Err("Please choose a project or a folder as the target.".to_string())
        );
        assert_eq!(
            ws.save_as(&p, "Teil.pcad", ""),
            Err("Document names must not end in \".pcad\".".to_string())
        );
        assert_eq!(
            ws.create_document(&Vec::new(), "T"),
            Err("Please choose a project or a folder as the target.".to_string())
        );
        ws.save_as(&p, "Teil", "").unwrap();
        ws.active_tab_mut()
            .state
            .create_box(DVec3::splat(3.0))
            .unwrap();
        assert_eq!(
            ws.restore_version(ws.active, 1),
            Err("There are unsaved changes. Save or discard them first.".to_string())
        );
        let path = ws.active_tab().path.clone().unwrap();
        ws.save_active("", None).unwrap();
        let view = ws.open_version(&path, 1).unwrap();
        assert_eq!(
            ws.make_editable(view),
            Err(
                "The document is already open in another tab. Continue working there or close the tab."
                    .to_string()
            )
        );
        assert_eq!(
            ws.save_active("", None),
            Err("This version is read-only. Branch off or restore it first.".to_string())
        );
        let mut none = Workspace::new(None, Config::default());
        assert_eq!(
            none.save_as(&Vec::new(), "T", ""),
            Err("No project folder has been chosen yet.".to_string())
        );
    });
}

// ---- Verlaufsleiste und Feature-Baum ---------------------------------------------------------

fn body_of(s: &AppState, feature: FeatureId) -> usize {
    s.bodies
        .iter()
        .position(|b| b.source.feature == feature)
        .expect("Körper sichtbar")
}

/// Skizze, Extrusion, Verschieben: eine Kette aus drei Schritten.
fn chain_model() -> (AppState, [FeatureId; 3]) {
    let mut s = AppState::new();
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::ZERO, DVec2::new(5.0, 10.0))
        .unwrap();
    s.close_sketch();
    let e = s.extrude_sketch(sk, 0, 2.0).unwrap();
    s.move_body(body_of(&s, e), DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    (s, [sk, e, m])
}

fn bar_frame(d: &mut Draw, s: &mut AppState, events: Vec<Event>) -> Vec<Seen> {
    d.frame(events, &mut |ui| history_bar::show(ui.ctx(), &mut *s))
}

fn bar_settle(d: &mut Draw, s: &mut AppState, n: usize) -> Vec<Seen> {
    let mut seen = Vec::new();
    for _ in 0..n {
        seen = bar_frame(d, s, vec![]);
    }
    seen
}

fn secondary(at: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn history_bar_speaks_english() {
    with_lang(&english(), || {
        let (mut s, [sk, e, m]) = chain_model();
        s.select_body(Some(m));
        let mut d = Draw::new();
        let seen = bar_settle(&mut d, &mut s, 4);
        assert_has("Leiste", &seen, &["Step 3 of 3"]);
        assert_english("Leiste", &seen, &[], &[]);
        let rects = history_bar::bar_rects(&d.ctx);
        let names: Vec<&str> = rects.buttons.iter().map(|b| b.0.as_str()).collect();
        for want in [
            "Play",
            "To the start",
            "One step back",
            "One step forward",
            "To the end",
        ] {
            assert!(names.contains(&want), "{want} fehlt in {names:?}");
        }

        // Hinweis beim Überfahren eines Schritts.
        let at = rects.steps[2].1.center();
        bar_frame(&mut d, &mut s, vec![Event::PointerMoved(at)]);
        let seen = bar_settle(&mut d, &mut s, 30);
        assert_has("Hinweis", &seen, &["Double-click opens the settings."]);
        // Die Maße im Hinweis kommen aus den Eigenschaften (Task 7).
        assert_english("Hinweis", &seen, &[], &["Körper:"]);
        let at = rects.steps[0].1.center();
        bar_frame(&mut d, &mut s, vec![Event::PointerMoved(at)]);
        let seen = bar_settle(&mut d, &mut s, 30);
        assert_has("Hinweis Skizze", &seen, &["Double-click opens the sketch."]);

        // Kontextmenü eines Schritts.
        bar_frame(&mut d, &mut s, vec![secondary(at, true)]);
        bar_frame(&mut d, &mut s, vec![secondary(at, false)]);
        let seen = bar_settle(&mut d, &mut s, 3);
        assert_has("Menü", &seen, &["Edit", "Rename", "Suppress", "Delete"]);
        assert_english("Menü", &seen, &[], &[]);

        // Zurückgesetzt hinter dem Marker.
        let mut s = chain_model().0;
        s.select_body(Some(m));
        history_bar::move_marker(&mut s, m, 1).unwrap();
        let mut d = Draw::new();
        let seen = bar_settle(&mut d, &mut s, 4);
        assert_has("Marker", &seen, &["Step 1 of 3"]);
        let rects = history_bar::bar_rects(&d.ctx);
        let at = rects.steps[2].1.center();
        bar_frame(&mut d, &mut s, vec![Event::PointerMoved(at)]);
        let seen = bar_settle(&mut d, &mut s, 30);
        assert_has("zurückgesetzt", &seen, &["Rolled back"]);
        assert_english("zurückgesetzt", &seen, &[], &["Körper:"]);
        assert_eq!(
            history_bar::lock_hint(),
            "This body is rolled back. Jump to the end of the history first."
        );
        assert_eq!(
            history_bar::move_marker(&mut s, m, 99),
            Err("This position does not exist in the history".to_string())
        );
        assert_eq!(
            history_bar::move_marker(&mut s, FeatureId(9999), 0),
            Err("The body no longer exists".to_string())
        );

        // Der ganze Verlauf ist zurückgesetzt.
        let mut s = chain_model().0;
        s.select_body(Some(m));
        browser::roll_back_to(&mut s, sk).unwrap();
        let mut d = Draw::new();
        let seen = bar_settle(&mut d, &mut s, 4);
        assert_has(
            "global",
            &seen,
            &["Activate all", "Entire history rolled back"],
        );
        assert_english("global", &seen, &[], &[]);
        assert_eq!(
            history_bar::move_marker(&mut s, e, 2),
            Err(
                "The whole history is rolled back. Activate it first with \"Activate all\"."
                    .to_string()
            )
        );

        // Ohne gewählten Körper.
        let mut s = AppState::new();
        let mut d = Draw::new();
        let seen = bar_settle(&mut d, &mut s, 4);
        assert_has("kein Körper", &seen, &["Choose a body"]);
        assert_english("kein Körper", &seen, &[], &[]);
    });
}

/// Der Ordner „Zeichnungen“ gehört zu Task 9 und ist hier noch deutsch.
const DRAWINGS_LATER: [&str; 1] = ["Zeichnung"];

fn tree_frame(d: &mut Draw, s: &mut AppState, events: Vec<Event>) -> Vec<Seen> {
    d.frame(events, &mut |ui| {
        egui::CentralPanel::default().show(ui, |ui| browser::show(ui, &mut *s));
    })
}

fn tree_settle(d: &mut Draw, s: &mut AppState, n: usize) -> Vec<Seen> {
    let mut seen = Vec::new();
    for _ in 0..n {
        seen = tree_frame(d, s, vec![]);
    }
    seen
}

#[test]
fn feature_tree_speaks_english() {
    with_lang(&english(), || {
        // Leeres Dokument.
        let mut s = AppState::new();
        let mut d = Draw::new();
        let seen = tree_settle(&mut d, &mut s, 3);
        assert_has(
            "leer",
            &seen,
            &[
                "Feature tree",
                "Bodies",
                "Sketches",
                "Drawings",
                "Origin",
                "No bodies yet. Use \"Create\" in the menu to add a body or a sketch.",
                "No sketch yet. Use \"Create\", then \"Sketch\" to add one.",
            ],
        );
        assert_english("leer", &seen, &[], &DRAWINGS_LATER);
        s.tree_folders.set_open(browser::Folder::Origin, true);
        let seen = tree_settle(&mut d, &mut s, 3);
        assert_has("Ursprung", &seen, &["XY plane", "XZ plane", "YZ plane"]);
        assert_english("Ursprung", &seen, &[], &DRAWINGS_LATER);

        // Mit Körper: Kontextmenü einer Skizze.
        let (mut s, [sk, _, m]) = chain_model();
        let mut d = Draw::new();
        let seen = tree_settle(&mut d, &mut s, 3);
        assert_english("Körper", &seen, &[], &DRAWINGS_LATER);
        let name = s.document.feature(sk).unwrap().name.clone();
        let at = seen
            .iter()
            .find(|t| t.text == name)
            .unwrap_or_else(|| panic!("Skizze „{name}“ fehlt in {}", join(&seen)))
            .rect
            .center();
        tree_frame(&mut d, &mut s, vec![Event::PointerMoved(at)]);
        tree_frame(&mut d, &mut s, vec![secondary(at, true)]);
        tree_frame(&mut d, &mut s, vec![secondary(at, false)]);
        let seen = tree_settle(&mut d, &mut s, 3);
        assert_has(
            "Menü Skizze",
            &seen,
            &[
                "Edit",
                "Rename",
                "Suppress",
                "Extrude …",
                "Revolve …",
                "Delete",
            ],
        );
        assert_english("Menü Skizze", &seen, &[], &DRAWINGS_LATER);

        // Zurückgesetzter Körper.
        let mut s = chain_model().0;
        history_bar::move_marker(&mut s, m, 1).unwrap();
        let mut d = Draw::new();
        let seen = tree_settle(&mut d, &mut s, 3);
        assert_has("zurückgesetzt", &seen, &["rolled back"]);
        assert_english("zurückgesetzt", &seen, &[], &DRAWINGS_LATER);

        // Umbenennen und Rückfrage beim Löschen mit Folgen.
        let (mut s, [sk, ..]) = chain_model();
        let mut d = Draw::new();
        s.rename_draft = Some((sk, "S".to_string()));
        let seen = tree_settle(&mut d, &mut s, 3);
        assert_has("Umbenennen", &seen, &["Rename", "Apply", "Cancel"]);
        assert_english("Umbenennen", &seen, &[], &DRAWINGS_LATER);
        s.rename_draft = None;
        s.pending_delete = Some(browser::delete_plan(&s, sk));
        let seen = tree_settle(&mut d, &mut s, 3);
        assert_has("Rückfrage", &seen, &["Confirm", "Delete", "Cancel"]);
        assert_english("Rückfrage", &seen, &[], &DRAWINGS_LATER);
        let text = browser::delete_plan(&s, sk).text();
        assert!(
            text.contains("feature(s) that depend on it will be deleted:"),
            "{text}"
        );
        // Löschen mit Hinweis zu Skizzen auf Flächen.
        let note = browser::DeletePlan {
            target: sk,
            dependents: Vec::new(),
            names: Vec::new(),
            target_name: "Box".into(),
            note: Some(vec!["S1".into(), "S2".into()]),
        }
        .text();
        assert!(
            note.starts_with("\"Box\" will be deleted.\n\nNote: A sketch on a face"),
            "{note}"
        );
        assert!(
            note.ends_with(
                "It is therefore kept and then lies on the face of that body: \"S1\", \"S2\""
            ),
            "{note}"
        );
        assert_eq!(
            browser::apply_delete(
                &mut s,
                &browser::DeletePlan {
                    target: FeatureId(9999),
                    dependents: Vec::new(),
                    names: Vec::new(),
                    target_name: String::new(),
                    note: None,
                }
            ),
            Err("The feature no longer exists".to_string())
        );
    });
}

// ---- Rückgängig-Namen ------------------------------------------------------------------------

#[test]
fn undo_names_speak_english() {
    with_lang(&english(), || {
        assert_eq!(undo_labels::fallback(), "Change");
        let mut s = AppState::new();
        let before = s.snapshot();
        s.create_box(DVec3::splat(4.0)).unwrap();
        assert_eq!(
            undo_labels::describe_change(&before, &s.snapshot()),
            "Create box"
        );
    });
}

/// Review Focus 4: was beim Anlegen gespeichert wird, bleibt in der Sprache seines Entstehens.
#[test]
fn stored_labels_keep_their_language() {
    let mut ws = workspace("planora-stored-labels");
    // Deutsch angelegt: Schritt und vorgeschlagener Name.
    let first = with_lang(&german(), || {
        let first = ws.new_tab();
        ws.active_tab_mut()
            .state
            .create_box(DVec3::splat(4.0))
            .unwrap();
        ws.observe_history(false);
        first
    });
    assert_eq!(ws.tabs[first].name, "Unbenannt 2");
    with_lang(&english(), || {
        // Die gespeicherte Beschriftung bleibt deutsch, die Anzeige läuft ohne Fehler.
        assert_eq!(
            ws.active_tab().undo_label().as_deref(),
            Some("Quader anlegen")
        );
        assert_eq!(
            planora_app::app::undo_menu_text(ws.active_tab().undo_label().as_deref()),
            "Undo: Quader anlegen"
        );
        let mut d = Draw::new();
        let seen = d.settle(3, &mut |ui| {
            planora_app::app::edit_menu(ui, &mut ws);
        });
        assert_has("Menü", &seen, &["Undo: Quader anlegen", "Redo"]);
        for s in &seen {
            assert!(!common::looks_like_key(&s.text), "{}", join(&seen));
        }
        // Der Name des Tabs bleibt, wie er angelegt wurde; ein neuer entsteht englisch.
        assert_eq!(ws.tabs[first].name, "Unbenannt 2");
        let i = ws.new_tab();
        assert_eq!(ws.tabs[i].name, "Untitled 3");
        ws.activate(first);

        // Ein neuer Schritt entsteht englisch, der alte bleibt deutsch.
        ws.active_tab_mut()
            .state
            .create_box(DVec3::splat(6.0))
            .unwrap();
        ws.observe_history(false);
        assert_eq!(ws.active_tab().undo_label().as_deref(), Some("Create box"));
        ws.active_tab_mut().undo().unwrap();
        assert_eq!(ws.active_tab().redo_label().as_deref(), Some("Create box"));
        assert_eq!(
            ws.active_tab().undo_label().as_deref(),
            Some("Quader anlegen")
        );
        ws.active_tab_mut().redo().unwrap();
        assert_eq!(ws.active_tab().undo_label().as_deref(), Some("Create box"));
    });
    // Zurück auf Deutsch: der englische Schritt bleibt englisch.
    with_lang(&german(), || {
        assert_eq!(ws.active_tab().undo_label().as_deref(), Some("Create box"));
        let text = planora_app::app::undo_menu_text(ws.active_tab().undo_label().as_deref());
        assert_eq!(text, "Rückgängig: Create box");
    });

    // Skizzenverlauf: gespeicherte Schritte behalten ihre Sprache.
    let mut s = AppState::new();
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    with_lang(&german(), || {
        s.session_sketch_mut()
            .unwrap()
            .1
            .add_rectangle(DVec2::ZERO, DVec2::new(5.0, 3.0))
            .unwrap();
        s.observe_sketch(false);
    });
    with_lang(&english(), || {
        assert_eq!(s.sketch_undo_label().as_deref(), Some("Rechteck zeichnen"));
        s.session_sketch_mut()
            .unwrap()
            .1
            .add_rectangle(DVec2::new(8.0, 0.0), DVec2::new(12.0, 3.0))
            .unwrap();
        // Eine noch offene Änderung wird beim Anzeigen benannt, in der Sprache des Bildes ...
        assert_eq!(s.sketch_undo_label().as_deref(), Some("Draw rectangle"));
        s.observe_sketch(false);
        // ... und als Schritt gespeichert.
        assert_eq!(s.sketch_undo_label().as_deref(), Some("Draw rectangle"));
    });
    with_lang(&german(), || {
        assert_eq!(s.sketch_undo_label().as_deref(), Some("Draw rectangle"));
    });
}
