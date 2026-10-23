//! Fenster „Einstellungen“ mit dem Abschnitt „Projekte → Projektverzeichnis“: Wechsel des
//! Projektverzeichnisses über die Dateiauswahl im Ordner-Modus, Sperre bei offenen gespeicherten
//! Dokumenten, Meldungen, Schließen mit Rückweg und Platz bei 800 × 600. Jede Konfiguration
//! liegt im Temp-Ordner, jede Bibliothek unter `common::temp_root`.

mod common;

use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use planora_app::file_chooser::{CHOOSE_FOLDER, display_path};
use planora_app::icons::{self, Icon};
use planora_app::places::{self, Places};
use planora_app::settings_window::*;
use planora_app::theme;
use planora_app::workspace::Workspace;
use planora_project::config::Config;
use planora_project::library::Library;
use std::fs;
use std::path::{Path, PathBuf};

/// Arbeitsbereich mit Bibliothek in Temp, Konfiguration in einem eigenen Temp-Ordner.
struct Env {
    ws: Workspace,
    /// Wie übergeben (nicht kanonisch).
    given: PathBuf,
    /// Kanonischer Pfad der Bibliothek.
    root: PathBuf,
    /// Eigener Temp-Ordner (Konfiguration, andere Verzeichnisse).
    tmp: PathBuf,
    cfg: PathBuf,
}

fn env() -> Env {
    let given = common::temp_root("planora-einstellungen");
    let lib = Library::open(&given).unwrap();
    let root = lib.root().path().to_path_buf();
    let tmp = common::temp_root("planora-einstellungen-tmp");
    fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(Some(lib), Config::default());
    let cfg = tmp.join("config.json");
    ws.config_path = cfg.clone();
    Env {
        ws,
        given,
        root,
        tmp,
        cfg,
    }
}

fn lib_root(ws: &Workspace) -> PathBuf {
    ws.library.as_ref().unwrap().root().path().to_path_buf()
}

/// Legt im aktuellen Tab ein gespeichertes Dokument „P/Teil“ an (Tab bleibt offen).
fn save_document(ws: &mut Workspace) {
    let p = ws.library.as_ref().unwrap().create_project("P").unwrap();
    ws.save_as(&p, "Teil", "").unwrap();
}

fn write_config(cfg: &Path, root: &Path, favorites: Vec<PathBuf>) {
    Config {
        root: Some(root.to_path_buf()),
        favorites,
    }
    .save_to(cfg)
    .unwrap();
}

// --- change_root ---------------------------------------------------------------------------

#[test]
fn changing_the_directory_with_open_documents_is_refused_and_writes_nothing() {
    let mut e = env();
    save_document(&mut e.ws);
    let fav = e.tmp.join("Fav");
    write_config(&e.cfg, &e.root, vec![fav]);
    let before = fs::read(&e.cfg).unwrap();
    let rev = e.ws.library_revision;
    let other = e.tmp.join("Anderes");
    assert_eq!(
        change_root(&mut e.ws, &other),
        Err(RootMessage::Warning(
            "Bitte zuerst alle gespeicherten Dokumente schließen, bevor der Projektordner wechselt."
                .into()
        ))
    );
    assert_eq!(
        fs::read(&e.cfg).unwrap(),
        before,
        "Datei Byte für Byte gleich"
    );
    assert_eq!(lib_root(&e.ws), e.root);
    assert_eq!(e.ws.library_revision, rev);
    assert!(!other.exists(), "nichts angelegt");
}

#[test]
fn a_new_directory_keeps_favorites_and_only_changes_root() {
    let mut e = env();
    e.ws.library.as_ref().unwrap().create_project("P").unwrap();
    let fav = e.tmp.join("Fav");
    write_config(&e.cfg, &e.root, vec![fav.clone()]);
    let rev = e.ws.library_revision;
    let other = e.tmp.join("Anderes");
    assert_eq!(change_root(&mut e.ws, &other), Ok(()));
    let disk = Config::read_from(&e.cfg).unwrap();
    assert_eq!(disk.root, Some(other.clone()));
    assert_eq!(disk.favorites, vec![fav]);
    assert_eq!(e.ws.library_revision, rev + 1);
    assert!(places::same_dir(
        &lib_root(&e.ws),
        &fs::canonicalize(&other).unwrap()
    ));
    assert!(
        e.root.join("P").join("projekt.json").is_file(),
        "altes Projekt bleibt, wo es ist"
    );
    assert!(!other.join("P").exists(), "nichts kopiert");
}

#[test]
fn the_same_directory_changes_nothing() {
    let mut e = env();
    // Mit offenem Dokument: der gleiche Ordner ist kein Wechsel, also auch keine Sperre.
    save_document(&mut e.ws);
    let rev = e.ws.library_revision;
    for same in [e.given.clone(), e.root.clone(), current_directory(&e.ws)] {
        assert_eq!(change_root(&mut e.ws, &same), Ok(()), "{same:?}");
    }
    assert!(!e.cfg.exists(), "config.json nicht angelegt");
    assert_eq!(e.ws.library_revision, rev);
    assert_eq!(lib_root(&e.ws), e.root);
}

#[test]
fn an_unusable_directory_gives_an_error_and_keeps_the_old_one() {
    let mut e = env();
    write_config(&e.cfg, &e.root, vec![e.tmp.join("Fav")]);
    let before = fs::read(&e.cfg).unwrap();
    let file = e.tmp.join("datei.txt");
    fs::write(&file, b"x").unwrap();
    let rev = e.ws.library_revision;
    assert!(matches!(
        change_root(&mut e.ws, &file),
        Err(RootMessage::Error(_))
    ));
    assert_eq!(lib_root(&e.ws), e.root);
    assert_eq!(e.ws.library_revision, rev);
    assert_eq!(fs::read(&e.cfg).unwrap(), before);
}

#[test]
fn an_unreadable_config_is_not_overwritten_by_favorites_but_root_is_chosen() {
    let mut e = env();
    fs::write(&e.cfg, b"{kaputt").unwrap();
    let other = e.tmp.join("Anderes");
    assert_eq!(change_root(&mut e.ws, &other), Ok(()));
    let disk = Config::read_from(&e.cfg).expect("danach gültig");
    assert_eq!(disk.root, Some(other.clone()));
    assert!(disk.favorites.is_empty());
}

#[test]
fn the_current_directory_falls_back_to_the_config_and_its_default() {
    let mut e = env();
    assert!(places::same_dir(&current_directory(&e.ws), &e.given));
    assert!(
        !current_directory(&e.ws)
            .to_string_lossy()
            .starts_with(r"\\?\"),
        "ohne Windows-Präfix"
    );
    e.ws.library = None;
    e.ws.config.root = Some(e.tmp.join("Gewählt"));
    assert_eq!(current_directory(&e.ws), e.tmp.join("Gewählt"));
    e.ws.config.root = None;
    assert_eq!(current_directory(&e.ws), Config::default_root());
}

// --- Fenster ------------------------------------------------------------------------------

/// Ein gezeichneter Text mit Rechteck, Ausschnitt und Farbe.
struct Shot {
    text: String,
    rect: Rect,
    clip: Rect,
    color: Color32,
}

fn walk(shape: &egui::epaint::Shape, clip: Rect, out: &mut Vec<Shot>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
        egui::epaint::Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut color = section.map_or(t.fallback_color, |s| s.format.color);
            if color == Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            out.push(Shot {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                clip,
                color,
            })
        }
        _ => {}
    }
}

/// Das Fenster ohne Bildschirm, mit Arbeitsbereich und Schnellzugriffen.
struct App {
    ctx: egui::Context,
    size: egui::Vec2,
    time: f64,
    w: SettingsWindow,
    e: Env,
    places: Places,
    /// Letztes Ereignis ungleich `None`.
    last: SettingsEvent,
}

impl App {
    fn new(e: Env, dark: bool, w: f32, h: f32) -> App {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        theme::apply(&ctx, dark);
        let places = Places {
            drives: Some(vec![]),
            ..e.ws.places()
        };
        App {
            ctx,
            size: vec2(w, h),
            time: 0.0,
            w: SettingsWindow::default(),
            e,
            places,
            last: SettingsEvent::None,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Shot> {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let (w, ws, places, last) = (
            &mut self.w,
            &mut self.e.ws,
            &mut self.places,
            &mut self.last,
        );
        let out = self.ctx.run_ui(raw, |ui| {
            let ev = w.show(ui.ctx(), ws, places);
            if ev != SettingsEvent::None {
                *last = ev;
            }
        });
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, c.clip_rect, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    fn settle(&mut self, n: usize) -> Vec<Shot> {
        let mut seen = Vec::new();
        for _ in 0..n {
            seen = self.frame(vec![]);
        }
        seen
    }

    /// Klick auf den sichtbaren Text `text` (vorher eine Pause, damit kein Doppelklick entsteht).
    fn click(&mut self, text: &str) {
        self.time += 1.0;
        let seen = self.settle(4);
        let s = seen
            .iter()
            .find(|s| s.text == text && s.clip.contains(s.rect.center()))
            .unwrap_or_else(|| panic!("„{text}“ nicht sichtbar"));
        let at = s.rect.center();
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.settle(3);
    }
}

fn texts(seen: &[Shot]) -> Vec<&str> {
    seen.iter().map(|s| s.text.as_str()).collect()
}

fn close_glyph() -> String {
    icons::glyph(Icon::Close).to_string()
}

#[test]
fn the_window_shows_section_path_button_and_hint() {
    let e = env();
    let path = display_path(&current_directory(&e.ws));
    let mut a = App::new(e, true, 1400.0, 900.0);
    a.w.open(false);
    assert!(a.w.is_open());
    let seen = a.settle(6);
    let t = texts(&seen);
    for wanted in [
        TITLE,
        SECTION_PROJECTS,
        DIRECTORY_LABEL,
        path.as_str(),
        CHANGE_DIRECTORY,
        KEEP_HINT,
        "Schließen",
    ] {
        assert!(t.contains(&wanted), "„{wanted}“ fehlt: {t:?}");
    }
    assert!(!t.iter().any(|s| s.contains("Wurzel")), "{t:?}");
    // Auch beim Wählen nirgends „Wurzel“.
    a.click(CHANGE_DIRECTORY);
    assert!(a.w.chooser_mut().is_some(), "Knopf öffnet die Auswahl");
    let seen = a.settle(4);
    let t = texts(&seen);
    assert!(t.contains(&CHOOSE_HEADING), "{t:?}");
    assert!(t.contains(&CHOOSE_FOLDER), "{t:?}");
    assert!(!t.iter().any(|s| s.contains("Wurzel")), "{t:?}");
}

#[test]
fn cancel_changes_nothing() {
    let e = env();
    fs::create_dir_all(e.given.join("Neu")).unwrap();
    let mut a = App::new(e, true, 1400.0, 900.0);
    a.w.open(false);
    a.click(CHANGE_DIRECTORY);
    a.w.chooser_mut().unwrap().mark("Neu");
    assert_eq!(a.w.chooser_mut().unwrap().marked(), Some("Neu"));
    a.click("Abbrechen");
    assert!(a.w.chooser_mut().is_none(), "Auswahl zu");
    assert!(a.w.is_open(), "Fenster bleibt offen");
    assert_eq!(lib_root(&a.e.ws), a.e.root);
    assert!(!a.e.cfg.exists(), "config.json fehlt weiter");
    assert_eq!(a.w.message(), None);
}

#[test]
fn choosing_a_marked_folder_switches_and_closes_the_chooser() {
    let e = env();
    fs::create_dir_all(e.given.join("Neu")).unwrap();
    let mut a = App::new(e, true, 1400.0, 900.0);
    a.w.open(false);
    a.click(CHANGE_DIRECTORY);
    a.w.chooser_mut().unwrap().mark("Neu");
    a.click(CHOOSE_FOLDER);
    assert_eq!(lib_root(&a.e.ws), a.e.root.join("Neu"));
    assert_eq!(a.w.message(), None);
    assert!(a.w.chooser_mut().is_none());
    let disk = Config::read_from(&a.e.cfg).unwrap();
    assert!(places::same_dir(
        disk.root.as_deref().unwrap(),
        &a.e.given.join("Neu")
    ));
    // Der neue Pfad wird angezeigt.
    let seen = a.settle(4);
    let shown = display_path(&a.e.root.join("Neu"));
    assert!(texts(&seen).contains(&shown.as_str()), "{shown}");
}

#[test]
fn the_chooser_starts_in_the_current_directory() {
    let mut e = env();
    let mut w = SettingsWindow::default();
    w.open(false);
    w.start_choosing(&e.ws);
    assert!(places::same_dir(w.chooser_mut().unwrap().dir(), &e.given));
    // Ohne Bibliothek: der gewählte Ordner, fehlt er, sein Elternordner.
    e.ws.library = None;
    e.ws.config.root = Some(e.tmp.join("fehlt"));
    w.start_choosing(&e.ws);
    assert!(places::same_dir(w.chooser_mut().unwrap().dir(), &e.tmp));
    fs::create_dir_all(e.tmp.join("da")).unwrap();
    e.ws.config.root = Some(e.tmp.join("da"));
    w.start_choosing(&e.ws);
    assert!(places::same_dir(
        w.chooser_mut().unwrap().dir(),
        &e.tmp.join("da")
    ));
}

/// Wählt mit offenem Dokument den Ordner „Neu“: wird abgelehnt.
fn refuse(a: &mut App) {
    a.w.open(false);
    a.click(CHANGE_DIRECTORY);
    a.w.chooser_mut().unwrap().mark("Neu");
    a.click(CHOOSE_FOLDER);
}

#[test]
fn a_refusal_is_shown_in_warning_colour() {
    for dark in [false, true] {
        let mut e = env();
        save_document(&mut e.ws);
        fs::create_dir_all(e.given.join("Neu")).unwrap();
        let mut a = App::new(e, dark, 1400.0, 900.0);
        refuse(&mut a);
        let Some(RootMessage::Warning(text)) = a.w.message().cloned() else {
            panic!("Warnung erwartet: {:?}", a.w.message());
        };
        assert!(
            a.w.chooser_mut().is_none(),
            "Auswahl schließt auch bei Ablehnung"
        );
        assert_eq!(lib_root(&a.e.ws), a.e.root);
        let seen = a.settle(4);
        let shot = seen
            .iter()
            .find(|s| s.text == text)
            .unwrap_or_else(|| panic!("Meldung nicht gezeichnet ({dark})"));
        assert_eq!(shot.color, theme::palette(dark).warning, "dunkel: {dark}");
    }
}

#[test]
fn reopening_clears_message_and_chooser() {
    let mut e = env();
    save_document(&mut e.ws);
    fs::create_dir_all(e.given.join("Neu")).unwrap();
    let mut a = App::new(e, true, 1400.0, 900.0);
    refuse(&mut a);
    assert!(a.w.message().is_some());
    a.w.start_choosing(&a.e.ws);
    a.w.close();
    assert!(!a.w.is_open());
    a.w.open(false);
    assert_eq!(a.w.message(), None);
    assert!(a.w.chooser_mut().is_none());
}

#[test]
fn closing_reports_the_way_back_to_projects() {
    let e = env();
    let mut a = App::new(e, true, 1400.0, 900.0);
    a.w.open(true);
    a.click("Schließen");
    assert_eq!(
        a.last,
        SettingsEvent::Closed {
            return_to_projects: true
        }
    );
    assert!(!a.w.is_open());

    a.last = SettingsEvent::None;
    a.w.open(true);
    a.click(&close_glyph());
    assert_eq!(
        a.last,
        SettingsEvent::Closed {
            return_to_projects: true
        }
    );
    assert!(!a.w.is_open());

    a.last = SettingsEvent::None;
    a.w.open(false);
    a.click("Schließen");
    assert_eq!(
        a.last,
        SettingsEvent::Closed {
            return_to_projects: false
        }
    );

    // ✕ auch während des Wählens.
    a.last = SettingsEvent::None;
    a.w.open(true);
    a.click(CHANGE_DIRECTORY);
    a.click(&close_glyph());
    assert_eq!(
        a.last,
        SettingsEvent::Closed {
            return_to_projects: true
        }
    );
    assert!(!a.w.is_open());
    assert!(a.w.chooser_mut().is_none());
}

#[test]
fn the_window_fits_800x600_while_choosing() {
    for long in [false, true] {
        let e = env();
        for i in 0..60 {
            fs::create_dir_all(e.given.join(format!("Ordner {i:02}"))).unwrap();
        }
        let favorites: Vec<PathBuf> = (0..24)
            .map(|i| {
                if long {
                    e.tmp.join(format!(
                        "Ein sehr langer Ordnername für die Kundenprojekte aus dem Jahr {i} mit vielen Unterordnern"
                    ))
                } else {
                    e.tmp.join(format!("Fav {i}"))
                }
            })
            .collect();
        for f in &favorites {
            fs::create_dir_all(f).unwrap();
        }
        let mut a = App::new(e, true, 800.0, 600.0);
        a.places.favorites = favorites;
        a.w.open(false);
        a.w.start_choosing(&a.e.ws);
        let seen = a.settle(8);
        let rect = a.w.last_rect.expect("Fenster gezeichnet");
        let window = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        assert!(rect.width() <= 720.0 + 1.0, "lang {long}: {rect:?}");
        assert!(window.contains_rect(rect), "lang {long}: {rect:?}");
        for s in &seen {
            if s.clip.intersects(s.rect) {
                assert!(
                    s.rect.left() >= -0.5 && s.rect.right() <= 800.5,
                    "lang {long}: „{}“ bei {:?}",
                    s.text,
                    s.rect
                );
            }
        }
        for button in [CHOOSE_FOLDER, "Abbrechen"] {
            let b = seen
                .iter()
                .find(|s| s.text == button)
                .unwrap_or_else(|| panic!("lang {long}: „{button}“ fehlt"));
            assert!(
                b.clip.contains_rect(b.rect) && rect.contains_rect(b.rect),
                "lang {long}: „{button}“ {:?} in {:?}",
                b.rect,
                b.clip
            );
        }
    }
}
