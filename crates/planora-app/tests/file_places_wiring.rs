//! Dateiauswahl, Verdrahtung in der App: Die Fenster des aktiven Tabs bekommen die gemeinsamen
//! Schnellzugriffe, ein Favorit aus dem Kontextmenü landet in der Konfigurationsdatei des
//! Arbeitsbereichs (hier immer ein Temp-Ordner), eine unlesbare Datei bleibt stehen. Dazu das
//! Neuzeichnen während der Laufwerkssuche, die Hervorhebung im Knopfmodell und die Ordnerfarbe.

use planora_app::app::show_file_dialogs;
use planora_app::export_dialog::ExportDialog;
use planora_app::file_chooser::*;
use planora_app::import_dialog::ImportDialog;
use planora_app::places::*;
use planora_app::step_import_dialog::StepImportDialog;
use planora_app::theme;
use planora_app::workspace::Workspace;
use planora_math::DVec3;
use planora_project::config::Config;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-verdraht-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn workspace_at(file: &Path, config: Config) -> Workspace {
    let mut ws = Workspace::new(None, config);
    ws.config_path = file.to_path_buf();
    ws
}

/// Bilder der Dateifenster des Arbeitsbereichs (wie in der App).
struct AppDriver {
    ctx: egui::Context,
    time: f64,
}

impl AppDriver {
    fn new() -> AppDriver {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        AppDriver { ctx, time: 0.0 }
    }

    fn frame(&mut self, ws: &mut Workspace, events: Vec<egui::Event>) -> Vec<(String, egui::Pos2)> {
        self.time += 0.05;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let out = self.ctx.run_ui(input, |ui| show_file_dialogs(ui.ctx(), ws));
        fn walk(s: &egui::epaint::Shape, out: &mut Vec<(String, egui::Pos2)>) {
            match s {
                egui::epaint::Shape::Text(t) => {
                    out.push((t.galley.text().to_string(), t.pos + t.galley.size() / 2.0))
                }
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    fn click(&mut self, ws: &mut Workspace, at: egui::Pos2, button: egui::PointerButton) {
        let ev = |pressed| egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(ws, vec![egui::Event::PointerMoved(at)]);
        self.frame(ws, vec![ev(true)]);
        self.frame(ws, vec![ev(false)]);
        self.frame(ws, vec![]);
    }
}

fn pos_of(seen: &[(String, egui::Pos2)], text: &str) -> Option<egui::Pos2> {
    seen.iter().find(|(t, _)| t == text).map(|(_, p)| *p)
}

/// Rechtsklick auf die Zeile `row`, dann Klick auf `item` im Kontextmenü.
fn context_click(drv: &mut AppDriver, ws: &mut Workspace, row: &str, item: &str) {
    drv.frame(ws, vec![]);
    let seen = drv.frame(ws, vec![]);
    let at = pos_of(&seen, row).unwrap_or_else(|| panic!("„{row}“: {seen:?}"));
    drv.click(ws, at, egui::PointerButton::Secondary);
    let seen = drv.frame(ws, vec![]);
    let at = pos_of(&seen, item).unwrap_or_else(|| panic!("„{item}“: {seen:?}"));
    drv.click(ws, at, egui::PointerButton::Primary);
}

#[test]
fn a_favorite_added_in_the_import_window_is_written_to_the_config_file() {
    let d = temp_dir("import");
    fs::create_dir_all(d.join("Teile")).unwrap();
    let file = d.join("cfg").join("config.json");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, r#"{ "root": "D:/Projekte" }"#).unwrap();
    let mut ws = workspace_at(&file, Config::load_from(&file));
    ws.active_tab_mut().import_dialog = Some(ImportDialog::new(d.clone()));
    let mut drv = AppDriver::new();
    context_click(&mut drv, &mut ws, "Teile", ADD_FAVORITE);
    let disk = Config::load_from(&file);
    assert_eq!(disk.favorites, [d.join("Teile")], "gespeichert");
    assert_eq!(
        disk.root,
        Some(PathBuf::from("D:/Projekte")),
        "Wurzel bleibt"
    );
    assert_eq!(ws.config.favorites, [d.join("Teile")]);
    // Und wieder entfernen.
    context_click(&mut drv, &mut ws, "Teile", REMOVE_FAVORITE);
    assert!(Config::load_from(&file).favorites.is_empty());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn an_unreadable_config_file_is_left_alone_by_the_app() {
    let d = temp_dir("unlesbar");
    fs::create_dir_all(d.join("Teile")).unwrap();
    let file = d.join("config.json");
    fs::write(&file, "{ kaputt").unwrap();
    let mut ws = workspace_at(&file, Config::load_from(&file));
    ws.active_tab_mut().step_import_dialog = Some(StepImportDialog::new(d.clone()));
    let mut drv = AppDriver::new();
    context_click(&mut drv, &mut ws, "Teile", ADD_FAVORITE);
    assert_eq!(fs::read_to_string(&file).unwrap(), "{ kaputt");
    assert_eq!(
        ws.config.favorites,
        [d.join("Teile")],
        "gilt bis Programmende"
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_export_window_shows_the_shared_favorites() {
    let d = temp_dir("export");
    let file = d.join("config.json");
    let mut ws = workspace_at(
        &file,
        Config {
            root: Some(d.join("Projekte")),
            favorites: vec![d.join("Lieblingsordner")],
            ..Config::default()
        },
    );
    fs::create_dir_all(d.join("Projekte")).unwrap();
    {
        let tab = ws.active_tab_mut();
        tab.state.create_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
        let mut dialog = ExportDialog::new(&tab.state);
        dialog.tick(&tab.state, true);
        dialog.begin_save(d.clone(), &tab.state);
        tab.export_dialog = Some(dialog);
    }
    let mut drv = AppDriver::new();
    drv.frame(&mut ws, vec![]);
    let seen = drv.frame(&mut ws, vec![]);
    assert!(pos_of(&seen, "Lieblingsordner").is_some(), "{seen:?}");
    assert!(pos_of(&seen, PROJECT_LABEL).is_some(), "{seen:?}");
    assert!(!file.exists(), "ohne Änderung nichts geschrieben");
    let _ = fs::remove_dir_all(&d);
}

// --- Neuzeichnen während der Laufwerkssuche ---------------------------------------------------

#[test]
fn repainting_stops_when_the_scan_is_done_or_hangs_too_long() {
    let (release, wait) = mpsc::channel::<()>();
    let scan = DriveScan::start(move |sink| {
        sink.report(PathBuf::from("C:\\"));
        let _ = wait.recv_timeout(Duration::from_secs(10));
    });
    let now = Instant::now();
    assert!(scan.wants_repaint_at(now), "Suche läuft");
    assert!(
        !scan.wants_repaint_at(now + DRIVE_WAIT + Duration::from_secs(1)),
        "ein hängendes Laufwerk hält das Neuzeichnen nicht ewig an"
    );
    assert!(DRIVE_WAIT <= Duration::from_secs(5));
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !scan.is_done() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!scan.wants_repaint_at(Instant::now()), "fertig");
    assert_eq!(scan.known(), [PathBuf::from("C:\\")]);
}

// --- Knopfmodell -------------------------------------------------------------------------------

#[test]
fn the_button_model_highlights_the_current_folder_and_gives_favorites_a_menu() {
    let d = temp_dir("knoepfe");
    fs::create_dir_all(d.join("a")).unwrap();
    let mut c = FileChooser::open(d.join("a"), vec!["stl"]);
    let shared = Places {
        favorites: vec![d.join("a"), d.clone()],
        ..Places::default()
    };
    let drives = [PathBuf::from("Q:\\")];
    let buttons = c.place_buttons(&shared, &drives);
    let get = |p: &Path| buttons.iter().find(|b| b.place.path == p).unwrap();
    assert!(get(&d.join("a")).current);
    assert!(!get(&d).current);
    assert!(!get(Path::new("Q:\\")).current);
    assert_eq!(
        get(&d).menu,
        Some(RowMenu::RemoveFavorite(d.clone())),
        "Favoriten lassen sich entfernen"
    );
    assert_eq!(get(Path::new("Q:\\")).menu, None);
    // Nach einem Wechsel wandert die Hervorhebung.
    c.up();
    let buttons = c.place_buttons(&shared, &drives);
    let current: Vec<&Path> = buttons
        .iter()
        .filter(|b| b.current)
        .map(|b| b.place.path.as_path())
        .collect();
    assert_eq!(current, [d.as_path()]);
    let _ = fs::remove_dir_all(&d);
}

// --- Farbe und Symbole --------------------------------------------------------------------------

#[test]
fn the_folder_symbol_uses_the_light_accent_of_the_theme() {
    for dark in [false, true] {
        let p = theme::palette(dark);
        let c = EntryIcon::Folder.color(dark);
        assert_eq!(c, p.marker, "Markierungsfarbe (dunkel: {dark})");
        assert!(
            c.b() > c.r() && c.b() > c.g(),
            "ein Blau wie der Akzent: {c:?}"
        );
        for grey in [p.text_primary, p.text_secondary, p.text_tertiary] {
            assert_ne!(c, grey);
        }
        assert_eq!(
            EntryIcon::File.color(dark),
            p.text_secondary,
            "ruhiges Grau"
        );
        assert!(theme::contrast_ratio(c, p.bg_panel) >= 4.5);
    }
}

#[test]
fn documents_and_projects_have_their_own_symbols() {
    use planora_app::icons::{self, Icon};
    assert_eq!(PlaceKind::Documents.icon(), Icon::Documents);
    assert_eq!(PlaceKind::Project.icon(), Icon::Projects);
    assert_eq!(icons::phosphor_name(Icon::Documents), "files");
    assert_eq!(icons::phosphor_name(Icon::Projects), "briefcase");
    // Der Projektordner sieht anders aus als die Ordnerzeilen und die Grundansicht (Home).
    for other in [Icon::Folder, Icon::Home] {
        assert_ne!(icons::glyph(Icon::Projects), icons::glyph(other));
        assert_ne!(icons::glyph(Icon::Documents), icons::glyph(other));
    }
}
