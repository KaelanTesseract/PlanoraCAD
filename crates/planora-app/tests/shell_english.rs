//! Hauptfenster, Menüs, Rückfragen und Tastenkürzel auf Englisch (Sprachplan, Task 4): unter
//! `with_lang(&english(), …)` gezeichnet, ohne Fenster. Kein Test ruft `set_language` oder
//! `set_global` (Ruling 4): die Sprache gilt nur für den Thread des Tests.

mod common;

use common::{Seen, assert_has, join, looks_like_key};
use eframe::App as _;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::{self, CreateKind, PlanoraApp};
use planora_app::keymap::settings_ui::{self, CaptureState};
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, text};
use planora_app::workspace::{After, Prompt, Workspace};
use planora_app::{commands, dialog, fonts, places, shell_panels, theme};
use planora_i18n::{english, german, with_lang};
use planora_project::config::Config;
use planora_project::library::Library;

const BIG: Vec2 = vec2(1920.0, 1080.0);

/// Texte, die zu Dateien späterer Tasks gehören und hier noch deutsch sind (Task 12: Würfelseiten).
/// Sie werden nur von der Deutsch-Prüfung ausgenommen, nicht von den Erwartungen.
const LATER_TASKS: [&str; 1] = ["Würfel"];

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn new() -> Rig {
        let root = common::temp_root("planora-shell-english");
        let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        ws.config_path = root.join("config.json");
        Rig::with(ws)
    }

    fn with(ws: Workspace) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, ws.active_tab().state.dark);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(ws),
            time: 0.0,
        };
        rig.settle();
        rig
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, BIG)),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let seen = common::collect_seen(&out.shapes);
        out.drop_without_applying_deltas();
        seen
    }

    fn settle(&mut self) -> Vec<Seen> {
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen = self.frame(vec![]);
        }
        seen
    }

    fn click_at(&mut self, at: Pos2) -> Vec<Seen> {
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.settle()
    }

    /// Klick auf den Menütitel `title` im oberen Feld.
    fn open_menu(&mut self, title: &str) -> Vec<Seen> {
        let top = shell_panels::field_rects(&self.ctx)
            .into_iter()
            .find(|(n, _)| *n == "oben")
            .expect("oberes Feld")
            .1;
        let seen = self.settle();
        let at = seen
            .iter()
            .find(|s| s.text == title && top.contains_rect(s.rect))
            .unwrap_or_else(|| panic!("Menü „{title}“ fehlt: {}", join(&seen)))
            .rect
            .center();
        self.click_at(at)
    }
}

/// Kein deutscher Buchstabe und keine Schlüssel-Id in den Texten (Zeilen späterer Tasks
/// ausgenommen).
fn assert_english(what: &str, seen: &[Seen]) {
    common::assert_english(what, seen, &[], &LATER_TASKS);
}

#[test]
fn shell_speaks_english() {
    with_lang(&english(), || {
        // Menüleiste, Statuszeile, Befehlsfeld.
        let mut rig = Rig::new();
        rig.app.shell_mut().set_versions_visible(true);
        let seen = rig.settle();
        assert_has(
            "Hauptfenster",
            &seen,
            &[
                "File",
                "Create",
                "Drawing",
                "Edit",
                "View",
                "Help",
                "0 bodies",
                "Union",
                "Subtract",
                "Intersect",
                "Properties",
                "Versions",
            ],
        );
        assert_english("Hauptfenster", &seen);

        // Jedes Menü.
        let seen = rig.open_menu("File");
        assert_has(
            "Datei",
            &seen,
            &[
                "New",
                "Projects …",
                "Save",
                "Save as …",
                "Export …",
                "Import mesh …",
                "Close",
                "Settings …",
                "Quit",
                "Ctrl+N",
                "Ctrl+S",
            ],
        );
        assert_english("Datei", &seen);

        let mut rig = Rig::new();
        let seen = rig.open_menu("Create");
        assert_has(
            "Erzeugen",
            &seen,
            &[
                "Box …",
                "Cylinder …",
                "Sphere …",
                "Cone …",
                "Sketch on XY plane",
                "Sketch on XZ plane",
                "Sketch on YZ plane",
                "Sketch on selected face",
                "Extrude sketch …",
                "Revolve sketch …",
                "E",
            ],
        );
        assert_english("Erzeugen", &seen);

        let mut rig = Rig::new();
        let seen = rig.open_menu("Edit");
        assert_has(
            "Bearbeiten",
            &seen,
            &[
                "Undo",
                "Redo",
                "Parameters …",
                "Union …",
                "Subtract …",
                "Intersect …",
                "Fillet …",
                "Chamfer …",
                "To end of timeline",
                "Ctrl+Z",
                "Ctrl+Y",
            ],
        );
        assert_english("Bearbeiten", &seen);

        let mut rig = Rig::new();
        let seen = rig.open_menu("View");
        assert_has(
            "Ansicht",
            &seen,
            &["Isometric", "Fit all", "Keyboard shortcuts …", "Versions"],
        );
        assert!(
            seen.iter()
                .any(|s| s.text == "Light theme" || s.text == "Dark theme"),
            "Design-Umschalter: {}",
            join(&seen)
        );
        assert_english("Ansicht", &seen);

        let mut rig = Rig::new();
        let seen = rig.open_menu("Help");
        assert_has("Hilfe", &seen, &["Feedback …", "Common problems"]);
        assert_english("Hilfe", &seen);

        // Zeichnungsmenü: nur der Titel gehört zu diesem Task (Inhalt: Task 9).
        let mut rig = Rig::new();
        let seen = rig.open_menu("Drawing");
        assert!(!seen.is_empty());

        // Körperzahl in der Mehrzahl, Warnbanner einer alten Version.
        let mut rig = Rig::new();
        rig.app
            .ws_mut()
            .active_tab_mut()
            .state
            .create_box(glam::DVec3::splat(5.0))
            .unwrap();
        let seen = rig.settle();
        assert_has("ein Körper", &seen, &["1 body"]);
        rig.app
            .ws_mut()
            .active_tab_mut()
            .state
            .create_box(glam::DVec3::splat(7.0))
            .unwrap();
        assert_has("zwei Körper", &rig.settle(), &["2 bodies"]);
        {
            let tab = rig.app.ws_mut().active_tab_mut();
            tab.read_only = true;
            tab.based_on = Some(3);
        }
        let seen = rig.settle();
        assert_has(
            "Banner",
            &seen,
            &[
                "Old version v3 – read-only. In the \"Versions\" area you can restore it or branch from it.",
            ],
        );
        assert_english("Banner", &seen);
    });
}

type PromptCase<'a> = (&'a str, Box<dyn Fn(u64) -> Prompt>, Vec<&'a str>);

#[test]
fn prompts_speak_english() {
    with_lang(&english(), || {
        let cases: Vec<PromptCase> = vec![
            (
                "Änderungen speichern",
                Box::new(|tab| Prompt::SaveChanges {
                    tab,
                    then: After::CloseTab,
                }),
                vec!["Save changes?", "Save", "Discard", "Cancel"],
            ),
            (
                "Neustart für das Update",
                Box::new(|tab| Prompt::SaveChanges {
                    tab,
                    then: After::Quit,
                }),
                vec!["Restart for the update", "Save", "Discard", "Cancel"],
            ),
            (
                "Version speichern",
                Box::new(|tab| Prompt::SaveComment {
                    tab,
                    then: None,
                    comment: String::new(),
                    branch: Some(String::new()),
                }),
                vec![
                    "Save version",
                    "Branch:",
                    "Comment:",
                    "This version is not the latest of its branch. The state is saved in a new branch.",
                ],
            ),
            (
                "Speichern unter",
                Box::new(|tab| Prompt::SaveAs {
                    tab,
                    then: None,
                    parent: Vec::new(),
                    name: String::new(),
                    comment: String::new(),
                }),
                vec![
                    "Save as",
                    "Name:",
                    "Comment:",
                    "Target (project or folder):",
                ],
            ),
            (
                "Autosicherung",
                Box::new(|tab| Prompt::AutosaveOffer { tab }),
                vec!["Autosave found", "Restore", "Discard"],
            ),
            (
                "Hinweis",
                Box::new(|_| Prompt::Notice("Something happened".into())),
                vec!["Notice", "OK", "Something happened"],
            ),
        ];
        for (what, make, wanted) in cases {
            let mut rig = Rig::new();
            let tab = rig.app.ws_mut().active_tab().id;
            // Beim Beenden für ein Update nennt die Rückfrage das Update.
            rig.app.ws_mut().quit_for_update = what == "Neustart für das Update";
            rig.app.ws_mut().prompt = Some(make(tab));
            let seen = rig.settle();
            assert_has(what, &seen, &wanted);
            assert_english(what, &seen);
            if what == "Neustart für das Update" {
                assert!(
                    seen.iter().any(|s| s.text.starts_with(
                        "PlanoraCAD has to restart to update. Save the unsaved changes in \""
                    )),
                    "Text zum Update fehlt in {}",
                    join(&seen)
                );
                assert!(!seen.iter().any(|s| s.text == "Save changes?"));
            }
            if what == "Speichern unter" {
                // Ohne Projekt steht der Hinweis dabei.
                assert!(
                    seen.iter()
                        .any(|s| s.text.starts_with("There is no project yet. Please close")),
                    "Hinweis ohne Projekt fehlt in {}",
                    join(&seen)
                );
            }
        }
        // Ohne Projektverzeichnis.
        let mut ws = Workspace::new(None, Config::default());
        ws.config_path = common::temp_root("planora-shell-english-nolib").join("config.json");
        let mut rig = Rig::with(ws);
        let tab = rig.app.ws_mut().active_tab().id;
        rig.app.ws_mut().prompt = Some(Prompt::SaveAs {
            tab,
            then: None,
            parent: Vec::new(),
            name: String::new(),
            comment: String::new(),
        });
        let seen = rig.settle();
        assert_has(
            "Speichern unter ohne Verzeichnis",
            &seen,
            &[
                "Save as",
                "Please close this with \"Cancel\", then choose the project folder in the \"File\" menu under \"Settings …\".",
            ],
        );
        assert_english("Speichern unter ohne Verzeichnis", &seen);
        // Gesperrtes Dokument.
        let mut rig = Rig::new();
        rig.app.ws_mut().prompt = Some(Prompt::TakeOver {
            path: Vec::new(),
            version: None,
        });
        let seen = rig.settle();
        assert_has(
            "Gesperrtes Dokument",
            &seen,
            &[
                "Document locked",
                "The document is currently in use by another program (locked). If that program has crashed, you can take over the lock.",
                "Take over lock",
                "Cancel",
            ],
        );
        assert_english("Gesperrtes Dokument", &seen);
    });
}

#[test]
fn keys_speak_english() {
    with_lang(&english(), || {
        // Übersicht.
        let mut rig = Rig::new();
        rig.app.overview_mut().open_focused();
        let seen = rig.settle();
        assert_has(
            "Übersicht",
            &seen,
            &[
                "Keyboard shortcuts",
                "Everywhere",
                "Model",
                "Sketch",
                "Drawing",
                "Change in settings",
                "Close",
                "New document",
                "Ctrl+N",
                "Ctrl+Shift+E",
                "Mouse wheel",
                "Drag middle button",
                "Delete",
                "Space",
            ],
        );
        assert_english("Übersicht", &seen);
        rig.app.overview_mut().filter = "zzzz".into();
        assert_has("Suche", &rig.settle(), &["No shortcuts found."]);

        // Einstellungen: Abschnitt „Keyboard shortcuts“.
        let root = common::temp_root("planora-keys-english");
        let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        ws.config_path = root.join("config.json");
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        let mut state = CaptureState::default();
        state.capturing = Some((Action::Save, 0));
        let mut seen = Vec::new();
        for i in 0..4 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, BIG)),
                time: Some(f64::from(i) * 0.05),
                ..Default::default()
            };
            let out = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default()
                    .show(ui, |ui| settings_ui::show(ui, &mut ws, &mut state));
            });
            seen = common::collect_seen(&out.shapes);
            out.drop_without_applying_deltas();
        }
        assert_has(
            "Einstellungen",
            &seen,
            &[
                "Keyboard shortcuts",
                "Search: action, area or key",
                "Choose \"Change\" and press the new key. Esc cancels.",
                "Action",
                "Area",
                "Binding",
                "Reset all",
                "Press a key …",
                "Change",
                "Remove",
                "Add",
            ],
        );
        assert_english("Einstellungen", &seen);

        // Hinweise nach Doppelbelegung, Tauschen, Zurücksetzen.
        let ctrl_s = Action::Save.default_bindings()[0];
        assert_eq!(
            settings_ui::conflict_text(&ctrl_s, Action::Save),
            "Ctrl+S is already used by \"Save\"."
        );
        assert_eq!(
            settings_ui::swapped_text(&ctrl_s, Action::Save),
            "Ctrl+S no longer applies to \"Save\"."
        );
        assert_eq!(
            settings_ui::taken_text(&ctrl_s, Action::Save),
            "Ctrl+S is a default again and no longer applies to \"Save\"."
        );

        // Tastennamen: keiner deutsch, keiner eine Schlüssel-Id; gespeichert wird deutsch.
        let mut checked = 0;
        for &key in egui::Key::ALL {
            if text::is_modifier_key(key) {
                continue;
            }
            let chord = KeyChord {
                key,
                ctrl: true,
                shift: true,
                alt: true,
            };
            let shown = text::chord_text(&chord);
            assert!(
                !shown.chars().any(|c| "äöüßÄÖÜ„".contains(c)),
                "Taste {key:?}: „{shown}“"
            );
            assert!(!looks_like_key(&shown), "Taste {key:?}: „{shown}“");
            assert!(shown.starts_with("Ctrl+Shift+Alt+"), "{shown}");
            let b = Binding::Key(chord);
            let stored = text::stored_text(&b);
            assert!(stored.starts_with("Strg+Umschalt+Alt+"), "{stored}");
            assert_eq!(text::parse_binding(&stored), Some(b), "{stored}");
            assert_eq!(
                with_lang(&german(), || text::binding_text(&b)),
                stored,
                "Deutsch: Anzeige = gespeicherter Text"
            );
            checked += 1;
        }
        assert!(checked > 100, "{checked}");

        // Die Konfiguration bleibt deutsch, auch bei englischer Oberfläche.
        let mut km = Keymap::defaults();
        km.set(
            Action::Save,
            0,
            Binding::Key(KeyChord {
                key: egui::Key::K,
                ctrl: true,
                shift: false,
                alt: false,
            }),
        );
        assert_eq!(km.text(Action::Save), "Ctrl+K");
        assert_eq!(
            km.to_overrides().get("datei.speichern"),
            Some(&vec!["Strg+K".to_string()])
        );
        // Hinweise beim Laden einer Datei.
        let mut file = std::collections::BTreeMap::new();
        file.insert("gibt.es.nicht".to_string(), vec!["Strg+K".to_string()]);
        file.insert("datei.neu".to_string(), vec!["Quatsch".to_string()]);
        let (_, notes) = Keymap::from_overrides(&file);
        assert!(
            notes.contains(&"Unknown action \"gibt.es.nicht\" skipped.".to_string()),
            "{notes:?}"
        );
        assert!(
            notes
                .contains(&"Invalid binding \"Quatsch\" for \"New document\" skipped.".to_string()),
            "{notes:?}"
        );
        let fixed = Keymap::fixed_key_text(
            Action::SketchLine,
            &Binding::Key(KeyChord {
                key: egui::Key::Delete,
                ctrl: false,
                shift: false,
                alt: false,
            }),
        );
        assert_eq!(
            fixed.as_deref(),
            Some("\"Delete\" is permanently assigned in the sketch.")
        );
        assert_eq!(Area::All.label(), "Everywhere");

        // Alle Aktionen haben einen englischen Namen.
        for &a in Action::ALL {
            let label = a.label();
            assert!(
                !label.is_empty() && !looks_like_key(&label),
                "{a:?}: {label}"
            );
            assert!(!label.chars().any(|c| "äöüßÄÖÜ„".contains(c)), "{label}");
        }
    });
}

#[test]
fn commands_and_places_speak_english() {
    with_lang(&english(), || {
        assert_eq!(
            commands::need_sketch(),
            "Choose a sketch with a closed profile first."
        );
        assert_eq!(commands::need_body(), "Choose a body first.");
        assert_eq!(commands::need_selection(), "Choose a body or step first.");
        assert_eq!(dialog::close_label(), "Close");
        assert_eq!(app::projects_entry(), "Projects …");
        assert_eq!(app::undo_menu_text(Some("Box")), "Undo: Box");
        assert_eq!(app::redo_menu_text(None), "Redo");
        assert_eq!(CreateKind::Revolution.label(), "Revolve sketch …");
        let root = common::temp_root("planora-cmd-english");
        let ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        assert_eq!(
            app::create_blocked(&ws, CreateKind::Extrusion).as_deref(),
            Some("Create a sketch first.")
        );
        // Schnellzugriffe.
        let labels: Vec<String> =
            places::known_places(Some(std::path::Path::new("C:/Users/x")), &|_| true)
                .into_iter()
                .map(|p| p.label)
                .collect();
        assert_eq!(labels, ["Documents", "Downloads", "Desktop", "Pictures"]);
        assert_eq!(places::project_label(), "Projects");
        assert_eq!(
            places::favorites_full(),
            "There are already 24 favorites. Please remove one first."
        );
        assert_eq!(places::add_favorite_label(), "Add to favorites");
        assert_eq!(places::remove_favorite_label(), "Remove from favorites");
        assert!(places::favorite_missing().starts_with("This folder does not exist."));
        // Statuszeilen-Hinweis und Ansicht-Würfel.
        assert!(shell_panels::sketch_keys_hint().starts_with("Esc cancels"));
        assert_eq!(planora_i18n::tr!("view-home-label"), "Home");
        assert_eq!(
            planora_i18n::tr!("view-home-hint"),
            "Home: default view (isometric)"
        );
        assert_eq!(
            planora_i18n::tr!("view-cube-hover", name = "Front"),
            "View: Front"
        );
        // Tab-Leiste.
        assert_eq!(planora_i18n::tr!("tab-close"), "Close");
    });
}

#[test]
fn every_menu_text_id_exists_in_both_languages() {
    // Die Aktionsnamen und Tastennamen stehen in Tabellen im Code (nicht als `tr!("…")`); die
    // Vollständigkeitsprüfung der Sprachdateien sieht sie nicht, deshalb hier: in beiden
    // Sprachen ein Text, der keine Id ist.
    for lang in [german(), english()] {
        with_lang(&lang, || {
            for &a in Action::ALL {
                let label = a.label();
                assert!(!looks_like_key(&label), "{}: {a:?} = {label}", lang.id());
            }
            for &key in egui::Key::ALL {
                if text::is_modifier_key(key) {
                    continue;
                }
                let name = text::chord_text(&KeyChord {
                    key,
                    ctrl: false,
                    shift: false,
                    alt: false,
                });
                assert!(!looks_like_key(&name), "{}: {key:?} = {name}", lang.id());
            }
        });
    }
}
