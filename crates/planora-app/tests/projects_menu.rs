//! Aufgabe 9: Datei → „Projekte …“ (Strg+O) und „Einstellungen …“ in der ganzen App, die
//! Schließregeln der Fenster (P10), das entfallene Feld „Projekte“ und das geordnete Ende des
//! Miniatur-Arbeiters. Die ganze App ohne Fenster: Rohereignisse je Bild gegen einen egui-Kontext.
//! Bibliotheken und Konfiguration liegen nur in Temp.

mod common;

use eframe::App as _;
use egui::epaint::Shape;
use egui::{Event, FontFamily, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::{
    self, EXIT_WAIT, EXPORT_BUSY, MenuRequest, PROJECTS_ENTRY, PlanoraApp, SETTINGS_ENTRY,
};
use planora_app::dialog::CLOSE_LABEL;
use planora_app::export_dialog::{ExportDialog, ExportFormat};
use planora_app::file_chooser::CHOOSE_FOLDER;
use planora_app::icons::{self, Icon};
use planora_app::import_dialog::ImportDialog;
use planora_app::project_actions;
use planora_app::projects_window::DIRECTORY_BUTTON;
use planora_app::settings_window::{CHANGE_DIRECTORY, NOT_SAVED, RootMessage};
use planora_app::shell::ShellState;
use planora_app::state::{AppState, StandardPlane};
use planora_app::step_import_dialog::StepImportDialog;
use planora_app::thumbnail_cache::{Thumb, ThumbKey, ThumbOutcome};
use planora_app::thumbnail_worker::{DROP_WAIT, ThumbRequest, ThumbWorker};
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{about_dialog, fonts, shell_panels, theme};
use planora_drawing::sheet::SheetModel;
use planora_drawing::{
    DrawError, DrawingFormat, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewKind,
};
use planora_exchange::Progress;
use planora_feature::SketchPlane;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::docstore::DocStore;
use planora_project::library::{Library, NodePath};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const BIG: Vec2 = vec2(1400.0, 900.0);

// ---- Bild ------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
    clip: Rect,
    family: FontFamily,
}

#[derive(Default)]
struct Shot {
    seen: Vec<Seen>,
}

impl Shot {
    fn of(out: &egui::FullOutput) -> Shot {
        fn walk(shape: &Shape, clip: Rect, out: &mut Vec<Seen>) {
            match shape {
                Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
                Shape::Text(t) => out.push(Seen {
                    text: t.galley.text().to_string(),
                    rect: Rect::from_min_size(t.pos, t.galley.size()),
                    clip,
                    family: t
                        .galley
                        .job
                        .sections
                        .first()
                        .map_or(FontFamily::Proportional, |s| {
                            s.format.font_id.family.clone()
                        }),
                }),
                _ => {}
            }
        }
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, c.clip_rect, &mut seen);
        }
        Shot { seen }
    }

    /// Sichtbare Stellen mit genau diesem Text.
    fn all(&self, text: &str) -> Vec<&Seen> {
        self.seen
            .iter()
            .filter(|s| s.text == text && s.clip.contains(s.rect.center()))
            .collect()
    }

    fn has(&self, text: &str) -> bool {
        !self.all(text).is_empty()
    }

    fn has_in(&self, text: &str, area: Rect) -> bool {
        self.all(text).iter().any(|s| area.contains_rect(s.rect))
    }

    fn contains_in(&self, part: &str, area: Rect) -> bool {
        self.seen
            .iter()
            .any(|s| s.text.contains(part) && area.intersects(s.rect))
    }

    fn texts(&self) -> String {
        self.seen
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

// ---- Die ganze App ---------------------------------------------------------------------------

/// Arbeitsbereich mit Bibliothek in Temp und Konfiguration in einem eigenen Temp-Ordner.
fn workspace() -> Workspace {
    let root = common::temp_root("planora-projekte-menue");
    with_config(Workspace::new(
        Some(Library::open(&root).unwrap()),
        Config::default(),
    ))
}

fn with_config(mut ws: Workspace) -> Workspace {
    let tmp = common::temp_root("planora-projekte-menue-cfg");
    fs::create_dir_all(&tmp).unwrap();
    ws.config_path = tmp.join("config.json");
    ws
}

fn lib_root(ws: &Workspace) -> PathBuf {
    ws.library.as_ref().unwrap().root().path().to_path_buf()
}

/// Dokument `name` (ein Quader) in `parent`, ohne Tab.
fn box_doc(ws: &Workspace, parent: &NodePath, name: &str) -> NodePath {
    let mut s = AppState::new();
    s.create_box(DVec3::new(20.0, 10.0, 8.0)).unwrap();
    let mut path = parent.clone();
    path.push(format!("{name}.pcad"));
    let dir = ws.library.as_ref().unwrap().root().resolve(&path).unwrap();
    drop(DocStore::create(&dir, &s.snapshot(), "").unwrap());
    path
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    size: Vec2,
    time: f64,
}

impl Rig {
    fn new(ws: Workspace) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, ws.active_tab().state.dark);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(ws),
            size: BIG,
            time: 0.0,
        };
        rig.settle(3);
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn state(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn projects_open(&mut self) -> bool {
        self.app.windows_mut().1.is_open()
    }

    fn settings_open(&mut self) -> bool {
        self.app.windows_mut().2.is_open()
    }

    fn frame(&mut self, events: Vec<Event>) -> Shot {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let shot = Shot::of(&out);
        out.drop_without_applying_deltas();
        shot
    }

    fn settle(&mut self, n: usize) -> Shot {
        let mut shot = Shot::default();
        for _ in 0..n {
            shot = self.frame(vec![]);
        }
        shot
    }

    /// Pause länger als die Frist eines Doppelklicks, dann ein Bild.
    fn pause(&mut self) -> Shot {
        self.time += 1.0;
        self.settle(3)
    }

    fn button(&mut self, at: Pos2, pressed: bool) -> Shot {
        self.frame(vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }])
    }

    fn click_at(&mut self, at: Pos2) -> Shot {
        self.frame(vec![Event::PointerMoved(at)]);
        self.button(at, true);
        self.button(at, false);
        self.settle(3)
    }

    fn double_click_at(&mut self, at: Pos2) -> Shot {
        self.pause();
        self.frame(vec![Event::PointerMoved(at)]);
        for _ in 0..2 {
            self.button(at, true);
            self.button(at, false);
        }
        self.settle(3)
    }

    /// Klick auf den sichtbaren Text `text` (in `area`, wenn angegeben).
    /// Klick auf `text`, mit `area` auf den untersten Treffer darin (in den Einstellungen steht
    /// die Aktion „Schließen“ der Tastenkürzel über dem Knopf „Schließen“ der Fußzeile).
    fn click_in(&mut self, text: &str, area: Option<Rect>) -> Shot {
        let shot = self.pause();
        let inside: Vec<_> = shot
            .all(text)
            .into_iter()
            .filter(|s| area.is_none_or(|a| a.contains_rect(s.rect)))
            .collect();
        let pick = match area {
            Some(_) => inside
                .into_iter()
                .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top())),
            None => inside.into_iter().next(),
        };
        let at = pick
            .unwrap_or_else(|| panic!("„{text}“ nicht sichtbar: {}", shot.texts()))
            .rect
            .center();
        self.click_at(at)
    }

    fn click(&mut self, text: &str) -> Shot {
        self.click_in(text, None)
    }

    /// Menü `menu` im oberen Feld öffnen und `entry` wählen.
    fn menu(&mut self, menu: &str, entry: &str) -> Shot {
        let top = self.field("oben");
        self.click_in(menu, Some(top));
        self.click(entry)
    }

    fn press(&mut self, key: Key, modifiers: Modifiers) -> Shot {
        let ev = |pressed| Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        };
        self.frame(vec![ev(true)]);
        self.frame(vec![ev(false)]);
        self.settle(2)
    }

    fn ctrl(&mut self, key: Key) -> Shot {
        self.press(key, Modifiers::COMMAND)
    }

    fn field(&self, name: &str) -> Rect {
        shell_panels::field_rects(&self.ctx)
            .into_iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("Feld {name} fehlt"))
            .1
    }

    /// Lesen der Projekte abwarten (höchstens eine Minute), dann einige Bilder.
    fn wait_scan(&mut self) -> Shot {
        self.settle(2);
        let (ws, projects, _) = self.app.windows_mut();
        projects.wait_for_scan(ws);
        self.settle(4)
    }

    /// Wie in der App: nur Bilder zeichnen (kein direktes Lesen), bis Kacheln da sind; höchstens
    /// eine Minute.
    fn frames_until_tiles(&mut self) -> Shot {
        let deadline = Instant::now() + Duration::from_secs(60);
        self.settle(1);
        while self.tile_names().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            self.settle(1);
        }
        self.settle(4)
    }

    fn tile_names(&mut self) -> Vec<String> {
        let p = self.app.windows_mut().1;
        p.tiles().iter().map(|t| t.name.clone()).collect()
    }

    fn tile(&mut self, name: &str) -> Rect {
        let p = self.app.windows_mut().1;
        let i = p
            .tiles()
            .iter()
            .position(|t| t.name == name)
            .unwrap_or_else(|| panic!("Kachel „{name}“ fehlt"));
        p.tile_rects()[i]
    }

    fn settings_rect(&mut self) -> Rect {
        self.app
            .windows_mut()
            .2
            .last_rect
            .expect("Einstellungen gezeichnet")
    }
}

fn with_box(mut ws: Workspace) -> Workspace {
    ws.active_tab_mut()
        .state
        .create_box(DVec3::new(100.0, 60.0, 40.0))
        .unwrap();
    ws
}

/// Zeichnung „Blatt 1“ mit Vorderansicht.
fn add_drawing(s: &mut AppState) -> DrawingId {
    let d = s
        .document
        .add_drawing(
            "Blatt 1",
            Sheet {
                paper: Paper::A4,
                landscape: true,
            },
            Projection::First,
        )
        .unwrap();
    s.document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 148.5, y: 105.0 },
            None,
        )
        .unwrap();
    d
}

// ---- Menü ------------------------------------------------------------------------------------

fn same_row(a: &Seen, b: &Seen) -> bool {
    (a.rect.center().y - b.rect.center().y).abs() < 4.0
}

#[test]
fn the_file_menu_has_projects_with_ctrl_o_and_settings() {
    assert_eq!(PROJECTS_ENTRY, "Projekte …");
    assert_eq!(SETTINGS_ENTRY, "Einstellungen …");
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    let mut ws = workspace();
    let mut shot = Shot::default();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, BIG)),
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                ui.set_width(280.0);
                theme::menu_style(ui.style_mut());
                app::file_menu(ui, &mut ws);
            });
        });
        shot = Shot::of(&out);
        out.drop_without_applying_deltas();
    }
    let projects = shot.all(PROJECTS_ENTRY)[0].clone();
    let keys = shot.all("Strg+O")[0].clone();
    assert!(
        same_row(&projects, &keys),
        "Strg+O in der Zeile von Projekte …"
    );
    assert!(keys.rect.left() > projects.rect.right());
    assert!(!shot.has("Öffnen …"), "kein Öffnen …: {}", shot.texts());
    let settings = shot.all(SETTINGS_ENTRY)[0].clone();
    // „Über PlanoraCAD“ steht im Menü „Hilfe“ (tests/feedback_dialog.rs).
    assert!(
        shot.all(about_dialog::TITLE).is_empty(),
        "kein Über im Menü Datei"
    );
    // Die Symbole links in der Zeile.
    for (row, icon) in [(&projects, Icon::Projects), (&settings, Icon::Settings)] {
        let glyph = icons::glyph(icon).to_string();
        let found = shot
            .seen
            .iter()
            .find(|s| s.family == fonts::icons() && same_row(s, row))
            .unwrap_or_else(|| panic!("kein Symbol bei „{}“", row.text));
        assert_eq!(found.text, glyph, "Symbol bei „{}“", row.text);
        assert!(found.rect.right() <= row.rect.left());
    }
    assert_eq!(icons::glyph(Icon::Settings), '\u{e272}');
}

#[test]
fn the_menu_bar_reports_projects_and_settings() {
    for (entry, want) in [
        (PROJECTS_ENTRY, MenuRequest::Projects),
        (SETTINGS_ENTRY, MenuRequest::Settings),
    ] {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        let mut ws = workspace();
        let mut shell = ShellState::default();
        let mut time = 0.0;
        let mut got = MenuRequest::None;
        let mut run = |events: Vec<Event>, got: &mut MenuRequest| {
            time += 0.05;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, BIG)),
                events,
                time: Some(time),
                ..Default::default()
            };
            let out = ctx.run_ui(raw, |ui| {
                let r = app::menu_bar(ui, &mut ws, &mut shell);
                if r != MenuRequest::None {
                    *got = r;
                }
            });
            let shot = Shot::of(&out);
            out.drop_without_applying_deltas();
            shot
        };
        let click = |at: Pos2| {
            let b = |pressed| Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            vec![vec![Event::PointerMoved(at)], vec![b(true)], vec![b(false)]]
        };
        let shot = run(vec![], &mut got);
        let shot = if shot.has("Datei") {
            shot
        } else {
            run(vec![], &mut got)
        };
        for ev in click(shot.all("Datei")[0].rect.center()) {
            run(ev, &mut got);
        }
        let shot = run(vec![], &mut got);
        assert_eq!(got, MenuRequest::None, "Datei allein meldet nichts");
        for ev in click(shot.all(entry)[0].rect.center()) {
            run(ev, &mut got);
        }
        run(vec![], &mut got);
        assert_eq!(got, want, "{entry}");
    }
}

#[test]
fn ctrl_o_opens_the_projects_window_everywhere() {
    for mode in ["Modell", "Zeichnung", "Skizze"] {
        let mut ws = with_box(workspace());
        let st = &mut ws.active_tab_mut().state;
        let d = add_drawing(st);
        let sk = st
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        match mode {
            "Zeichnung" => ws.active_tab_mut().state.drawing_ui.open(d),
            "Skizze" => ws.active_tab_mut().state.open_sketch(sk).unwrap(),
            _ => {}
        }
        let mut rig = Rig::new(ws);
        rig.settle(3);
        assert!(!rig.projects_open());
        let before = rig.state().snapshot();
        let shot = rig.ctrl(Key::O);
        assert!(rig.projects_open(), "{mode}: Strg+O öffnet Projekte");
        assert!(
            rig.state().snapshot().same_content(&before),
            "{mode}: Dokument unverändert"
        );
        assert!(
            shot.has(planora_app::projects_window::TITLE),
            "{mode}: Fenster gezeichnet"
        );
        // Noch einmal: bleibt offen.
        rig.ctrl(Key::O);
        assert!(rig.projects_open(), "{mode}: bleibt offen");
    }
}

#[test]
fn the_view_menu_has_no_projects_entry_and_the_left_field_no_projects_section() {
    for mode in ["Modell", "Zeichnung"] {
        let mut ws = with_box(workspace());
        if mode == "Zeichnung" {
            let st = &mut ws.active_tab_mut().state;
            let d = add_drawing(st);
            st.drawing_ui.open(d);
        }
        let mut rig = Rig::new(ws);
        let shot = rig.settle(3);
        let left = rig.field("links");
        assert!(
            shot.contains_in("Feature-Baum", left),
            "{mode}: Baum links: {}",
            shot.texts()
        );
        for no in ["Projekte", "Noch keine Projekte"] {
            assert!(!shot.contains_in(no, left), "{mode}: „{no}“ im linken Feld");
        }
        // Menü „Ansicht“: „Versionen“ ja, „Projekte“ nein.
        let top = rig.field("oben");
        let shot = rig.click_in("Ansicht", Some(top));
        let iso = shot.all("Isometrisch")[0].rect;
        let column = Rect::from_min_max(
            egui::pos2(iso.left() - 80.0, top.bottom()),
            egui::pos2(iso.left() + 260.0, rig.size.y),
        );
        assert!(
            shot.has_in("Versionen", column),
            "{mode}: Versionen im Menü: {}",
            shot.texts()
        );
        assert!(
            !shot.contains_in("Projekte", column),
            "{mode}: Projekte im Menü Ansicht"
        );
    }
}

// ---- Einstellungen und Rückweg ----------------------------------------------------------------

#[test]
fn settings_from_the_menu_open_without_way_back() {
    let mut rig = Rig::new(workspace());
    rig.ctrl(Key::O);
    assert!(rig.projects_open());
    let shot = rig.menu("Datei", SETTINGS_ENTRY);
    assert!(rig.settings_open(), "Einstellungen offen");
    assert!(!rig.projects_open(), "Projekte schließt");
    assert!(shot.has(planora_app::settings_window::TITLE));
    let area = rig.settings_rect();
    rig.click_in(CLOSE_LABEL, Some(area));
    assert!(!rig.settings_open(), "Schließen schließt");
    assert!(!rig.projects_open(), "kein Rückweg aus dem Menü");
    rig.settle(3);
    assert!(!rig.projects_open());
}

#[test]
fn the_directory_button_goes_to_settings_and_back() {
    let mut rig = Rig::new(workspace());
    rig.ctrl(Key::O);
    rig.wait_scan();
    rig.click(DIRECTORY_BUTTON);
    assert!(
        rig.settings_open(),
        "Projektverzeichnis … öffnet Einstellungen"
    );
    assert!(!rig.projects_open(), "Projekte schließt dabei");
    let area = rig.settings_rect();
    rig.click_in(CLOSE_LABEL, Some(area));
    assert!(!rig.settings_open());
    assert!(rig.projects_open(), "Rückweg nach Projekte");
    // Der Rückweg gilt nur einmal: Projekte schließen, Einstellungen aus dem Menü, schließen.
    let shot = rig.settle(2);
    assert!(shot.has(planora_app::projects_window::TITLE));
    rig.menu("Datei", SETTINGS_ENTRY);
    let area = rig.settings_rect();
    rig.click_in(CLOSE_LABEL, Some(area));
    assert!(!rig.projects_open(), "aus dem Menü kein Rückweg");
}

#[test]
fn a_favorite_added_in_the_settings_of_the_app_is_saved() {
    let ws = workspace();
    let root = lib_root(&ws);
    fs::create_dir_all(root.join("Teile")).unwrap();
    let cfg = ws.config_path.clone();
    let mut rig = Rig::new(ws);
    rig.menu("Datei", SETTINGS_ENTRY);
    rig.click(CHANGE_DIRECTORY);
    // Rechtsklick auf den Ordner, dann „Zu Favoriten hinzufügen“.
    let shot = rig.pause();
    let at = shot.all("Teile")[0].rect.center();
    rig.frame(vec![Event::PointerMoved(at)]);
    for pressed in [true, false] {
        rig.frame(vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        }]);
    }
    rig.settle(3);
    rig.click(planora_app::places::ADD_FAVORITE);
    let disk = Config::read_from(&cfg).unwrap();
    assert_eq!(disk.favorites.len(), 1, "{disk:?}");
    assert_eq!(
        fs::canonicalize(&disk.favorites[0]).unwrap(),
        fs::canonicalize(root.join("Teile")).unwrap()
    );
}

#[test]
fn after_a_directory_change_the_window_shows_the_new_projects() {
    change_directory_and_come_back(false);
}

/// Ließ sich die Einstellung nicht speichern, gilt der Wechsel trotzdem (Warnung `NOT_SAVED`):
/// das Fenster zeigt danach ebenso die neuen Projekte.
#[test]
fn a_directory_change_that_could_not_be_saved_still_shows_the_new_projects() {
    change_directory_and_come_back(true);
}

fn change_directory_and_come_back(blocked: bool) {
    let mut ws = workspace();
    if blocked {
        // Der Ordner der Konfiguration ist eine Datei: Schreiben ist unmöglich.
        let blocker = ws.config_path.with_file_name("blockiert");
        fs::write(&blocker, b"x").unwrap();
        ws.config_path = blocker.join("config.json");
    }
    ws.library.as_ref().unwrap().create_project("A").unwrap();
    let second = lib_root(&ws).join("Zweit");
    Library::open(&second).unwrap().create_project("B").unwrap();
    let mut rig = Rig::new(ws);
    rig.ctrl(Key::O);
    rig.frames_until_tiles();
    assert!(rig.tile_names().contains(&"A".to_string()));
    rig.click(DIRECTORY_BUTTON);
    rig.click(CHANGE_DIRECTORY);
    rig.app
        .windows_mut()
        .2
        .chooser_mut()
        .expect("Auswahl offen")
        .mark("Zweit");
    rig.click(CHOOSE_FOLDER);
    assert!(planora_app::places::same_dir(
        &lib_root(rig.ws()),
        &std::fs::canonicalize(&second).unwrap()
    ));
    let message = rig.app.windows_mut().2.message().cloned();
    if blocked {
        assert_eq!(
            message,
            Some(RootMessage::Warning(NOT_SAVED.into())),
            "Warnung, Wechsel gilt trotzdem"
        );
    } else {
        assert_eq!(message, None);
    }
    let area = rig.settings_rect();
    rig.click_in(CLOSE_LABEL, Some(area));
    assert!(rig.projects_open(), "Rückweg");
    let shot = rig.frames_until_tiles();
    assert_eq!(rig.tile_names(), ["B"]);
    assert!(shot.has("B"), "{}", shot.texts());
    assert!(!shot.has("A"), "keine alte Kachel: {}", shot.texts());
}

// ---- Sperre bei offener Rückfrage (Prüfrunde 1) ----------------------------------------------

/// Projekte mit „P“ offen und gelesen.
fn rig_with_project() -> (Rig, NodePath) {
    let ws = workspace();
    let p = ws.library.as_ref().unwrap().create_project("P").unwrap();
    let mut rig = Rig::new(ws);
    rig.ctrl(Key::O);
    rig.wait_scan();
    (rig, p)
}

/// Klick auf `text` im Fenster Projekte (nicht in einer Rückfrage).
fn click_in_projects(rig: &mut Rig, text: &str) {
    let open = rig.projects_open();
    let area = rig
        .app
        .windows_mut()
        .1
        .last_rect
        .unwrap_or_else(|| panic!("Projekte gezeichnet (offen: {open})"));
    rig.click_in(text, Some(area));
}

#[test]
fn a_prompt_locks_every_button_of_the_projects_window() {
    let (mut rig, p) = rig_with_project();
    // Strg+S: „Speichern unter“ (Rückfrage) bei offenem Fenster Projekte.
    rig.ctrl(Key::S);
    assert!(matches!(rig.ws().prompt, Some(Prompt::SaveAs { .. })));
    let generation = rig.app.windows_mut().1.ui.generation();
    click_in_projects(&mut rig, DIRECTORY_BUTTON);
    assert!(!rig.settings_open(), "Projektverzeichnis … öffnet nichts");
    assert!(rig.projects_open());
    click_in_projects(&mut rig, planora_app::projects_window::NEW_PROJECT);
    click_in_projects(&mut rig, planora_app::projects_window::REFRESH);
    assert!(
        !project_actions::dialog_open(&rig.app.windows_mut().1.ui),
        "Neues Projekt öffnet nichts"
    );
    assert_eq!(
        rig.app.windows_mut().1.ui.generation(),
        generation,
        "Aktualisieren wirkt nicht"
    );
    // Tiefer: Pfad, Neues Dokument, Neuer Ordner.
    rig.ws().prompt = None;
    rig.app.windows_mut().1.go_to(p.clone());
    rig.settle(3);
    rig.ctrl(Key::S);
    for text in [
        planora_app::projects_window::NEW_DOCUMENT,
        planora_app::projects_window::NEW_FOLDER,
        planora_app::projects_window::TITLE,
    ] {
        click_in_projects(&mut rig, text);
    }
    assert!(!project_actions::dialog_open(&rig.app.windows_mut().1.ui));
    assert_eq!(rig.app.windows_mut().1.location(), &p, "Pfad gesperrt");
    // Nach der Antwort geht alles wieder.
    rig.ws().prompt = None;
    rig.settle(2);
    click_in_projects(&mut rig, planora_app::projects_window::NEW_FOLDER);
    assert!(project_actions::dialog_open(&rig.app.windows_mut().1.ui));
}

#[test]
fn the_directory_cannot_be_changed_while_a_prompt_is_open() {
    let ws = workspace();
    let p = ws.library.as_ref().unwrap().create_project("P").unwrap();
    let root = lib_root(&ws);
    fs::create_dir_all(root.join("Neu")).unwrap();
    let mut rig = Rig::new(ws);
    // RV1: Projekte offen → Strg+S → „Projektverzeichnis …“ öffnet die Einstellungen nicht.
    rig.ctrl(Key::O);
    rig.wait_scan();
    rig.ctrl(Key::S);
    click_in_projects(&mut rig, DIRECTORY_BUTTON);
    assert!(!rig.settings_open(), "keine Einstellungen bei Rückfrage");
    // Auch direkt nicht.
    {
        let (ws, pr, s) = rig.app.windows_mut();
        app::open_settings(ws, pr, s, true);
    }
    assert!(!rig.settings_open(), "open_settings bei Rückfrage");
    rig.ws().prompt = None;
    rig.settle(2);

    // RV3: Einstellungen offen, Auswahl läuft; dann „Dokument gesperrt“ (Übernahme) als
    // Rückfrage: die Wahl ist gesperrt, bis die Rückfrage beantwortet ist.
    rig.menu("Datei", SETTINGS_ENTRY);
    rig.click(CHANGE_DIRECTORY);
    rig.app
        .windows_mut()
        .2
        .chooser_mut()
        .expect("Auswahl offen")
        .mark("Neu");
    let mut doc = p.clone();
    doc.push("Teil.pcad".into());
    rig.ws().prompt = Some(Prompt::TakeOver {
        path: doc,
        version: None,
    });
    rig.settle(2);
    let area = rig.settings_rect();
    rig.click_in(CHOOSE_FOLDER, Some(area));
    assert_eq!(lib_root(rig.ws()), root, "Verzeichnis bleibt");
    assert!(
        rig.app.windows_mut().2.chooser_mut().is_some(),
        "Auswahl bleibt offen, nichts gewählt"
    );
    // Mit Strg+S (Speichern unter) ebenso; „Projektverzeichnis ändern …“ geht dann auch nicht.
    rig.ws().prompt = None;
    rig.settle(2);
    rig.app.windows_mut().2.open(false);
    rig.settle(2);
    rig.ctrl(Key::S);
    assert!(rig.ws().prompt.is_some());
    let area = rig.settings_rect();
    rig.click_in(CHANGE_DIRECTORY, Some(area));
    assert!(
        rig.app.windows_mut().2.chooser_mut().is_none(),
        "„ändern“ gesperrt"
    );
    // Beantwortet: wieder frei.
    rig.ws().prompt = None;
    rig.settle(2);
    rig.click(CHANGE_DIRECTORY);
    assert!(rig.app.windows_mut().2.chooser_mut().is_some());
}

#[test]
fn the_settings_entry_is_disabled_while_a_prompt_is_open() {
    for prompt in [false, true] {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        let mut ws = workspace();
        if prompt {
            ws.prompt = Some(Prompt::Notice("x".into()));
        }
        let mut got = MenuRequest::None;
        let mut time = 0.0;
        let mut run = |events: Vec<Event>, got: &mut MenuRequest| {
            time += 0.05;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, BIG)),
                events,
                time: Some(time),
                ..Default::default()
            };
            let out = ctx.run_ui(raw, |ui| {
                let r = app::file_menu(ui, &mut ws);
                if r != MenuRequest::None {
                    *got = r;
                }
            });
            let shot = Shot::of(&out);
            out.drop_without_applying_deltas();
            shot
        };
        run(vec![], &mut got);
        let shot = run(vec![], &mut got);
        let at = shot.all(SETTINGS_ENTRY)[0].rect.center();
        let b = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for ev in [
            vec![Event::PointerMoved(at)],
            vec![b(true)],
            vec![b(false)],
            vec![],
        ] {
            run(ev, &mut got);
        }
        let want = if prompt {
            MenuRequest::None
        } else {
            MenuRequest::Settings
        };
        assert_eq!(got, want, "Rückfrage offen: {prompt}");
    }
}

#[test]
fn a_writing_export_explains_why_nothing_opens() {
    let mut ws = with_box(workspace());
    let d = add_drawing(&mut ws.active_tab_mut().state);
    let mut rig = Rig::new(ws);
    let dir = common::temp_root("planora-projekte-menue-busy");
    fs::create_dir_all(&dir).unwrap();
    {
        let s = &rig.app.ws_mut().active_tab().state;
        let mut e = ExportDialog::new(s);
        e.format = ExportFormat::Drawing(DrawingFormat::Svg);
        e.drawing = Some(d);
        e.tick(s, true);
        e.set_writer(slow_writer);
        e.write_to(dir.join("z.svg"), s);
        rig.ws().active_tab_mut().export_dialog = Some(e);
    }
    rig.settle(2);
    let shot = rig.ctrl(Key::O);
    assert!(!rig.projects_open());
    assert_eq!(rig.state().message.as_deref(), Some(EXPORT_BUSY));
    assert_eq!(EXPORT_BUSY, "Bitte erst den Export beenden.");
    let shot = if shot.has(EXPORT_BUSY) {
        shot
    } else {
        rig.settle(2)
    };
    assert!(
        shot.has(EXPORT_BUSY),
        "in der Statuszeile: {}",
        shot.texts()
    );
    rig.state().message = None;
    rig.menu("Datei", SETTINGS_ENTRY);
    assert!(!rig.settings_open());
    assert_eq!(rig.state().message.as_deref(), Some(EXPORT_BUSY));
    rig.ws().active_tab_mut().export_dialog = None;
}

// ---- Schließregeln (P10) ---------------------------------------------------------------------

fn slow_writer(_: &Path, _: &SheetModel, _: DrawingFormat, p: &Progress) -> Result<u64, DrawError> {
    loop {
        p.check()?;
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn opening_projects_closes_import_windows_but_never_a_writing_export() {
    let mut ws = with_box(workspace());
    let d = add_drawing(&mut ws.active_tab_mut().state);
    let mut rig = Rig::new(ws);
    let dir = common::temp_root("planora-projekte-menue-import");
    fs::create_dir_all(&dir).unwrap();

    rig.ws().active_tab_mut().import_dialog = Some(ImportDialog::new(dir.clone()));
    rig.settle(2);
    rig.ctrl(Key::O);
    assert!(rig.ws().active_tab().import_dialog.is_none(), "Import zu");
    assert!(rig.projects_open(), "Projekte offen");

    rig.app.windows_mut().1.close();
    rig.ws().active_tab_mut().step_import_dialog = Some(StepImportDialog::new(dir.clone()));
    rig.settle(2);
    rig.ctrl(Key::O);
    assert!(
        rig.ws().active_tab().step_import_dialog.is_none(),
        "STEP zu"
    );
    assert!(rig.projects_open());

    // Ein ruhendes Exportfenster schließt ebenso.
    rig.app.windows_mut().1.close();
    let e = ExportDialog::new(&rig.ws().active_tab().state);
    rig.ws().active_tab_mut().export_dialog = Some(e);
    rig.settle(2);
    rig.ctrl(Key::O);
    assert!(rig.ws().active_tab().export_dialog.is_none(), "Export zu");
    assert!(rig.projects_open());

    // Schreibt der Export, bleibt er; Projekte öffnet dann nicht.
    rig.app.windows_mut().1.close();
    {
        let s = &rig.app.ws_mut().active_tab().state;
        let mut e = ExportDialog::new(s);
        e.format = ExportFormat::Drawing(DrawingFormat::Svg);
        e.drawing = Some(d);
        e.tick(s, true);
        e.set_writer(slow_writer);
        e.write_to(dir.join("z.svg"), s);
        assert!(e.is_writing_file());
        rig.ws().active_tab_mut().export_dialog = Some(e);
    }
    rig.settle(2);
    rig.ctrl(Key::O);
    assert!(
        rig.ws()
            .active_tab()
            .export_dialog
            .as_ref()
            .is_some_and(|e| e.is_writing_file()),
        "schreibender Export bleibt"
    );
    assert!(!rig.projects_open(), "Projekte bleibt zu");
    // Ebenso die Einstellungen aus dem Menü.
    rig.menu("Datei", SETTINGS_ENTRY);
    assert!(!rig.settings_open(), "Einstellungen bleiben zu");
    rig.ws().active_tab_mut().export_dialog = None;
}

#[test]
fn a_file_window_of_the_active_tab_closes_projects_and_settings() {
    let mut rig = Rig::new(with_box(workspace()));
    rig.ctrl(Key::O);
    assert!(rig.projects_open());
    rig.menu("Datei", "Netz importieren …");
    assert!(
        rig.ws().active_tab().import_dialog.is_some(),
        "Import offen"
    );
    assert!(!rig.projects_open(), "Import schließt Projekte");
    rig.ws().active_tab_mut().import_dialog = None;

    // Tab mit offenem STEP-Import im Hintergrund.
    let dir = common::temp_root("planora-projekte-menue-step");
    fs::create_dir_all(&dir).unwrap();
    let i = rig.ws().new_tab();
    rig.ws().tabs[i].step_import_dialog = Some(StepImportDialog::new(dir));
    rig.ws().activate(0);
    rig.settle(2);
    for window in ["Projekte", "Einstellungen"] {
        if window == "Projekte" {
            rig.ctrl(Key::O);
            assert!(rig.projects_open());
        } else {
            rig.menu("Datei", SETTINGS_ENTRY);
            assert!(rig.settings_open());
        }
        rig.ws().activate(i);
        rig.settle(2);
        assert!(
            !rig.projects_open(),
            "{window}: Tabwechsel schließt Projekte"
        );
        assert!(
            !rig.settings_open(),
            "{window}: Tabwechsel schließt Einstellungen"
        );
        assert!(rig.ws().active_tab().step_import_dialog.is_some());
        rig.ws().activate(0);
        rig.settle(2);
    }
}

// ---- Dokumente, Speichern unter, Rückgängig ---------------------------------------------------

#[test]
fn opening_a_document_from_the_window_switches_the_tab() {
    let ws = workspace();
    let p = ws.library.as_ref().unwrap().create_project("P").unwrap();
    let doc = box_doc(&ws, &p, "Teil");
    let mut rig = Rig::new(ws);
    rig.ctrl(Key::O);
    rig.wait_scan();
    let at = rig.tile("P").center();
    rig.double_click_at(at);
    assert_eq!(rig.app.windows_mut().1.location(), &p, "im Projekt");
    rig.settle(2);
    let at = rig.tile("Teil").center();
    rig.double_click_at(at);
    let ws = rig.ws();
    let i = ws.tab_for_path(&doc).expect("Tab des Dokuments");
    assert_eq!(ws.active, i, "der Tab ist vorn");
    assert_eq!(
        ws.active_tab().state.bodies.len(),
        1,
        "der Quader des Dokuments"
    );
    assert!(rig.projects_open(), "Projekte bleibt offen");
    // Ein zweiter Tab vorn, dann noch einmal Doppelklick: der Tab des Dokuments kommt nach vorn.
    rig.ws().new_tab();
    rig.settle(2);
    let at = rig.tile("Teil").center();
    rig.double_click_at(at);
    let ws = rig.ws();
    assert_eq!(ws.active, ws.tab_for_path(&doc).unwrap(), "wieder vorn");
    assert!(rig.projects_open());
}

#[test]
fn save_as_points_to_the_projects_window() {
    let mut rig = Rig::new(with_config(Workspace::new(None, Config::default())));
    rig.ctrl(Key::S);
    assert!(matches!(rig.ws().prompt, Some(Prompt::SaveAs { .. })));
    let shot = rig.settle(3);
    assert!(
        shot.has(
            "Bitte mit „Abbrechen“ schließen, dann im Menü „Datei“ unter „Einstellungen …“ das Projektverzeichnis wählen."
        ),
        "{}",
        shot.texts()
    );
    // Der Rat führt zum Ziel: Abbrechen, dann Datei → Einstellungen ….
    rig.click("Abbrechen");
    assert!(rig.ws().prompt.is_none());
    rig.menu("Datei", SETTINGS_ENTRY);
    assert!(rig.settings_open(), "Einstellungen öffnen");

    // Bibliothek ohne Projekt.
    let mut rig = Rig::new(workspace());
    rig.ctrl(Key::S);
    let shot = rig.settle(3);
    assert!(
        shot.has(
            "Noch kein Projekt vorhanden. Bitte mit „Abbrechen“ schließen, dann mit Strg+O das Fenster Projekte öffnen und ein Projekt anlegen."
        ),
        "{}",
        shot.texts()
    );
    rig.click("Abbrechen");
    rig.ctrl(Key::O);
    rig.wait_scan();
    click_in_projects(&mut rig, planora_app::projects_window::NEW_PROJECT);
    assert!(
        project_actions::dialog_open(&rig.app.windows_mut().1.ui),
        "Neues Projekt öffnet"
    );
}

/// Prüfrunde M2: die neuen Texte duzen und sagen „Projektverzeichnis“; die Meldung von
/// `choose_root` bleibt wörtlich (Vorgabe des Plans).
#[test]
fn new_texts_say_du_and_projektverzeichnis() {
    assert_eq!(
        planora_app::settings_window::KEEP_HINT,
        "Deine Projekte bleiben, wo sie sind. Im neuen Verzeichnis erscheinen nur die Projekte, die dort liegen."
    );
    assert_eq!(
        project_actions::READ_ERROR_PREFIX,
        "Das Projektverzeichnis lässt sich nicht lesen: "
    );
    assert_eq!(
        planora_app::workspace::ROOT_LOCKED,
        "Bitte zuerst alle gespeicherten Dokumente schließen, bevor der Projektordner wechselt."
    );
}

/// Prüfrunde M4: Ein Dokument lässt sich nicht in einen Namen auf „.pcad“ umbenennen (gleich
/// welche Schreibweise), wie bei „Neues Dokument“ und „Speichern unter“.
#[test]
fn renaming_a_document_to_a_pcad_name_is_refused() {
    let mut ws = workspace();
    let p = ws.library.as_ref().unwrap().create_project("P").unwrap();
    let doc = box_doc(&ws, &p, "Teil");
    let mut st = project_actions::ProjectsUi::default();
    for bad in ["Neu.pcad", "Neu.PCAD", " Neu.Pcad "] {
        assert!(
            project_actions::rename_node(&mut ws, &mut st, &doc, bad).is_none(),
            "{bad}"
        );
        assert!(
            matches!(&ws.prompt, Some(Prompt::Notice(t))
                if t == planora_app::workspace::DOCUMENT_NAME_SUFFIX),
            "{bad}: Hinweis wie beim Anlegen"
        );
        ws.prompt = None;
    }
    let tree = ws.library.as_ref().unwrap().tree().unwrap();
    let names: Vec<_> = tree[0]
        .children
        .iter()
        .map(|n| n.path.last().cloned().unwrap_or_default())
        .collect();
    assert_eq!(names, ["Teil.pcad"], "nichts umbenannt");
    // Ein gewöhnlicher Name geht.
    let to = project_actions::rename_node(&mut ws, &mut st, &doc, "Neu").expect("umbenannt");
    assert_eq!(to.last().map(String::as_str), Some("Neu.pcad"));
    // Ordner und Projekte prüft weiter die Bibliothek (ihre eigene Meldung).
    let f = ws.library.as_ref().unwrap().create_folder(&p, "F").unwrap();
    assert!(project_actions::rename_node(&mut ws, &mut st, &f, "G").is_some());
}

fn bodies(rig: &mut Rig) -> usize {
    rig.state().bodies.len()
}

#[test]
fn undo_is_blocked_by_project_dialogs_but_not_by_the_windows() {
    let ws = workspace();
    let p = ws.library.as_ref().unwrap().create_project("P").unwrap();
    let mut rig = Rig::new(ws);
    rig.state().create_box(DVec3::splat(10.0)).unwrap();
    rig.settle(3);
    assert_eq!(bodies(&mut rig), 1);
    rig.ctrl(Key::O);
    rig.wait_scan();
    rig.ctrl(Key::Z);
    assert_eq!(bodies(&mut rig), 0, "Projekte offen: Strg+Z wirkt");
    rig.ctrl(Key::Y);
    assert_eq!(bodies(&mut rig), 1, "Strg+Y wirkt");
    // Rückfrage „Löschen …“ des Fensters offen: gesperrt.
    project_actions::request_delete(&mut rig.app.windows_mut().1.ui, p);
    rig.settle(2);
    assert!(project_actions::dialog_open(&rig.app.windows_mut().1.ui));
    rig.ctrl(Key::Z);
    assert_eq!(bodies(&mut rig), 1, "Rückfrage offen: Strg+Z gesperrt");
    rig.app.windows_mut().1.ui.close_dialogs();
    rig.settle(2);
    rig.ctrl(Key::Z);
    assert_eq!(bodies(&mut rig), 0, "danach wieder frei");
}

#[test]
fn undo_is_blocked_while_the_settings_window_is_open() {
    let mut rig = Rig::new(workspace());
    rig.state().create_box(DVec3::splat(10.0)).unwrap();
    rig.settle(3);
    rig.menu("Datei", SETTINGS_ENTRY);
    assert!(rig.settings_open());
    rig.ctrl(Key::Z);
    assert_eq!(bodies(&mut rig), 1, "Einstellungen offen: Strg+Z gesperrt");
    rig.app.windows_mut().2.close();
    rig.settle(2);
    rig.ctrl(Key::Z);
    assert_eq!(bodies(&mut rig), 0, "danach wieder frei");
}

#[test]
fn closing_projects_closes_its_dialogs() {
    let ws = workspace();
    let p = ws.library.as_ref().unwrap().create_project("P").unwrap();
    let mut rig = Rig::new(ws);
    rig.ctrl(Key::O);
    rig.wait_scan();
    project_actions::request_delete(&mut rig.app.windows_mut().1.ui, p);
    rig.settle(2);
    // Ein Dateifenster des Tabs schließt Projekte samt Rückfrage (sonst bliebe Rückgängig gesperrt).
    rig.menu("Datei", "Netz importieren …");
    assert!(!rig.projects_open());
    assert!(!project_actions::dialog_open(&rig.app.windows_mut().1.ui));
}

// ---- Ende des Programms ----------------------------------------------------------------------

fn request(dir: &Path) -> ThumbRequest {
    ThumbRequest {
        dir: dir.to_path_buf(),
        key: ThumbKey::new("x", 1, "Haupt", true),
    }
}

/// Arbeiter, dessen Aufbau `busy` dauert.
fn slow_worker(busy: Duration) -> ThumbWorker {
    ThumbWorker::start_with(move |_: &ThumbRequest| {
        std::thread::sleep(busy);
        ThumbOutcome {
            thumb: Thumb::NoBodies,
            stored: false,
            from_cache: false,
        }
    })
}

/// App mit eingesetztem Arbeiter, der gerade einen Auftrag baut.
fn app_with_running_worker(busy: Duration) -> PlanoraApp {
    let mut app = PlanoraApp::for_tests(workspace());
    let dir = common::temp_root("planora-projekte-menue-ende");
    fs::create_dir_all(&dir).unwrap();
    let projects = app.windows_mut().1;
    projects.set_worker(slow_worker(busy));
    let w = projects.worker().unwrap();
    w.set_queue(vec![request(&dir)], vec![]);
    let deadline = Instant::now() + Duration::from_secs(10);
    while w.running().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(w.running().is_some(), "Auftrag läuft");
    app
}

#[test]
fn exiting_the_app_shuts_the_thumbnail_worker_down() {
    assert!(EXIT_WAIT > DROP_WAIT);
    let mut app = app_with_running_worker(Duration::from_millis(300));
    let t0 = Instant::now();
    app.on_exit();
    let took = t0.elapsed();
    let w = app.windows_mut().1.worker().unwrap();
    assert!(!w.alive(), "der Arbeiter ist beendet ({took:?})");
    assert!(took <= EXIT_WAIT + Duration::from_millis(500), "{took:?}");
    // Nach dem Ende nimmt er nichts mehr an.
    w.set_queue(vec![request(Path::new("x"))], vec![]);
    assert!(w.is_idle());
}

#[test]
fn dropping_the_app_waits_only_briefly_for_a_running_thumbnail() {
    let app = app_with_running_worker(Duration::from_secs(3));
    let t0 = Instant::now();
    drop(app);
    let took = t0.elapsed();
    assert!(
        took < DROP_WAIT + Duration::from_millis(450),
        "Fallenlassen dauerte {took:?}"
    );
}
