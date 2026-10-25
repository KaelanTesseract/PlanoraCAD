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
        ..Config::default()
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
fn an_unreadable_config_is_replaced_with_the_chosen_directory_and_the_favorites_in_memory() {
    let mut e = env();
    fs::write(&e.cfg, b"{kaputt").unwrap();
    let fav = e.tmp.join("Im Speicher");
    e.ws.config.favorites = vec![fav.clone()];
    let other = e.tmp.join("Anderes");
    assert_eq!(change_root(&mut e.ws, &other), Ok(()));
    let disk = Config::read_from(&e.cfg).expect("danach gültig");
    assert_eq!(disk.root, Some(other.clone()));
    assert_eq!(disk.favorites, vec![fav], "Stand im Speicher geschrieben");
}

#[test]
fn a_project_or_document_folder_is_not_accepted() {
    let mut e = env();
    write_config(&e.cfg, &e.root, vec![e.tmp.join("Fav")]);
    let before = fs::read(&e.cfg).unwrap();
    let p = e.ws.library.as_ref().unwrap().create_project("P").unwrap();
    e.ws.save_as(&p, "Teil", "").unwrap();
    // Das Dokument wieder schließen (neuer leerer Tab), damit nichts gesperrt ist.
    e.ws.new_tab();
    e.ws.close_tab(0);
    assert!(!e.ws.has_saved_documents());
    let rev = e.ws.library_revision;
    let bare = e.tmp.join("Kopie.PCAD");
    fs::create_dir_all(&bare).unwrap();
    let doc_inside = e.tmp.join("Doku");
    fs::create_dir_all(&doc_inside).unwrap();
    fs::write(doc_inside.join("dokument.json"), b"{}").unwrap();
    for target in [
        e.root.join("P"),
        e.root.join("P").join("Teil.pcad"),
        bare,
        doc_inside,
    ] {
        assert_eq!(
            change_root(&mut e.ws, &target),
            Err(RootMessage::Error(NESTED.into())),
            "{target:?}"
        );
    }
    assert_eq!(
        NESTED,
        "Das ist selbst schon ein Projekt oder Dokument. Bitte den Ordner wählen, der deine Projekte enthält."
    );
    assert_eq!(lib_root(&e.ws), e.root);
    assert_eq!(e.ws.library_revision, rev);
    assert_eq!(fs::read(&e.cfg).unwrap(), before);
}

#[test]
fn an_unreadable_target_gives_a_plain_error_and_keeps_the_old_one() {
    let mut e = env();
    write_config(&e.cfg, &e.root, vec![]);
    let before = fs::read(&e.cfg).unwrap();
    // Eine Datei lässt sich nicht als Ordner lesen.
    let file = e.tmp.join("datei.txt");
    fs::write(&file, b"x").unwrap();
    let Err(RootMessage::Error(text)) = change_root(&mut e.ws, &file) else {
        panic!("Fehler erwartet");
    };
    assert!(text.starts_with(UNREADABLE), "{text}");
    assert!(text.len() > UNREADABLE.len(), "mit Grund: {text}");
    assert!(
        !text.contains("os error"),
        "ohne Text des Betriebssystems: {text}"
    );
    assert_eq!(UNREADABLE, "Das Verzeichnis lässt sich nicht lesen.");
    assert_eq!(lib_root(&e.ws), e.root);
    assert_eq!(fs::read(&e.cfg).unwrap(), before);
}

#[test]
fn a_failed_save_switches_for_this_session_with_a_warning() {
    let mut e = env();
    // Der Ordner der Konfiguration ist eine Datei: Schreiben ist unmöglich.
    let blocker = e.tmp.join("blockiert");
    fs::write(&blocker, b"x").unwrap();
    e.ws.config_path = blocker.join("config.json");
    let rev = e.ws.library_revision;
    let other = e.tmp.join("Anderes");
    assert_eq!(
        change_root(&mut e.ws, &other),
        Err(RootMessage::Warning(NOT_SAVED.into()))
    );
    assert_eq!(
        NOT_SAVED,
        "Das neue Projektverzeichnis gilt nur bis zum Programmende: die Einstellung konnte nicht gespeichert werden."
    );
    assert!(places::same_dir(
        &lib_root(&e.ws),
        &fs::canonicalize(&other).unwrap()
    ));
    assert_eq!(e.ws.config.root, Some(other.clone()));
    assert_eq!(e.ws.library_revision, rev + 1);
    assert_eq!(fs::read(&blocker).unwrap(), b"x");
}

#[test]
fn persisting_reports_whether_it_was_written() {
    let mut e = env();
    e.ws.config.root = Some(e.tmp.join("A"));
    assert!(e.ws.try_persist_config());
    assert_eq!(
        Config::read_from(&e.cfg).unwrap().root,
        Some(e.tmp.join("A"))
    );
    let blocker = e.tmp.join("blockiert");
    fs::write(&blocker, b"x").unwrap();
    e.ws.config_path = blocker.join("config.json");
    assert!(!e.ws.try_persist_config());
    // Die alte Form bleibt still.
    e.ws.persist_config();
}

#[test]
fn errors_and_warnings_have_their_own_colours() {
    for dark in [false, true] {
        let p = theme::palette(dark);
        assert_eq!(RootMessage::Error("x".into()).color(dark), p.error);
        assert_eq!(RootMessage::Warning("x".into()).color(dark), p.warning);
        assert_ne!(p.error, p.warning);
    }
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
    /// Wie in der App über `show_in` (Schnellzugriffe aus dem Arbeitsbereich, Speichern danach).
    via_show_in: bool,
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
            via_show_in: false,
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
        let via_show_in = self.via_show_in;
        let out = self.ctx.run_ui(raw, |ui| {
            let ev = if via_show_in {
                w.show_in(ui.ctx(), ws)
            } else {
                w.show(ui.ctx(), ws, places)
            };
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

    /// Klick auf den sichtbaren Text `text` (vorher eine Pause, damit kein Doppelklick entsteht);
    /// bei mehreren der unterste (die Aktion „Schließen“ der Tastenkürzel steht über dem Knopf
    /// „Schließen“ der Fußzeile).
    fn click(&mut self, text: &str) {
        self.click_with(text, PointerButton::Primary);
    }

    fn click_with(&mut self, text: &str, which: PointerButton) {
        self.time += 1.0;
        let seen = self.settle(4);
        let s = seen
            .iter()
            .filter(|s| s.text == text && s.clip.contains(s.rect.center()))
            .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .unwrap_or_else(|| panic!("„{text}“ nicht sichtbar"));
        let at = s.rect.center();
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: which,
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
fn a_later_successful_change_clears_an_earlier_warning() {
    let mut e = env();
    save_document(&mut e.ws);
    fs::create_dir_all(e.given.join("Neu")).unwrap();
    let mut a = App::new(e, true, 1400.0, 900.0);
    refuse(&mut a);
    assert!(matches!(a.w.message(), Some(RootMessage::Warning(_))));
    // Dokument schließen, dann noch einmal wählen (ohne neues `open`).
    a.e.ws.new_tab();
    a.e.ws.close_tab(0);
    a.click(CHANGE_DIRECTORY);
    a.w.chooser_mut().unwrap().mark("Neu");
    a.click(CHOOSE_FOLDER);
    assert_eq!(a.w.message(), None, "alte Warnung weg");
    assert_eq!(lib_root(&a.e.ws), a.e.root.join("Neu"));
    let seen = a.settle(4);
    assert!(!texts(&seen).iter().any(|t| t.starts_with("Bitte zuerst")));
}

#[test]
fn a_favorite_added_in_the_settings_chooser_is_written_to_the_config_file() {
    let e = env();
    fs::create_dir_all(e.given.join("Teile")).unwrap();
    write_config(&e.cfg, &e.root, vec![]);
    let mut a = App::new(e, true, 1400.0, 900.0);
    a.via_show_in = true;
    a.w.open(false);
    a.click(CHANGE_DIRECTORY);
    a.click_with("Teile", PointerButton::Secondary);
    a.click(places::ADD_FAVORITE);
    let disk = Config::read_from(&a.e.cfg).unwrap();
    assert_eq!(disk.favorites.len(), 1, "{disk:?}");
    assert!(places::same_dir(
        &disk.favorites[0],
        &a.e.given.join("Teile")
    ));
    assert_eq!(disk.root, Some(a.e.root.clone()), "Verzeichnis bleibt");
    assert_eq!(a.e.ws.config.favorites, disk.favorites);
    assert!(a.w.chooser_mut().is_some(), "Auswahl bleibt offen");
    // `show_in` gibt den Rückweg weiter.
    a.w.open(true);
    a.click("Schließen");
    assert_eq!(
        a.last,
        SettingsEvent::Closed {
            return_to_projects: true
        }
    );
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

// --- Darstellung -------------------------------------------------------------------------------

/// Angezeigter Wert des Reglers: Zahl und gleich dahinter „ %“ (egui zeichnet beide getrennt),
/// unter der Beschriftung. Liefert Wert und Rechteck der Zahl.
fn shown_value(seen: &[Shot]) -> (u8, Rect) {
    let label = seen
        .iter()
        .find(|s| s.text == TRANSPARENCY_LABEL)
        .expect("Beschriftung sichtbar");
    let number = seen
        .iter()
        .find(|s| s.rect.top() > label.rect.top() && s.text.parse::<u8>().is_ok())
        .expect("Zahl sichtbar");
    let unit = seen
        .iter()
        .find(|s| s.text.trim() == "%")
        .expect("„%“ sichtbar");
    assert!(
        (unit.rect.left() - number.rect.right()).abs() < 2.0
            && (unit.rect.center().y - number.rect.center().y).abs() < 2.0,
        "„%“ direkt hinter der Zahl"
    );
    assert_eq!(unit.text, " %", "mit Leerzeichen: „40 %“");
    (number.text.parse().unwrap(), number.rect)
}

/// Klickt in die Schieberspur des Reglers: `at` = 0,0 (linkes Ende) bis 1,0 (rechtes Ende).
/// Die Spur steht unter der Beschriftung und endet vor der Zahl.
fn click_slider(a: &mut App, at: f32) {
    a.time += 1.0;
    let seen = a.settle(4);
    let label = seen
        .iter()
        .find(|s| s.text == TRANSPARENCY_LABEL)
        .expect("Beschriftung sichtbar");
    let (_, value) = shown_value(&seen);
    let value = Shot {
        text: String::new(),
        rect: value,
        clip: value,
        color: Color32::TRANSPARENT,
    };
    let left = label.rect.left();
    let right = left + SLIDER_WIDTH;
    assert!(right < value.rect.left(), "Zahl rechts neben der Spur");
    let pos = pos2(left + (right - left) * at, value.rect.center().y);
    let button = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    a.frame(vec![Event::PointerMoved(pos)]);
    a.frame(vec![button(true)]);
    a.frame(vec![button(false)]);
    a.settle(3);
}

#[test]
fn the_appearance_section_stands_above_projects() {
    for dark in [false, true] {
        let e = env();
        let mut a = App::new(e, dark, 1400.0, 900.0);
        a.w.open(false);
        let seen = a.settle(6);
        let find = |t: &str| {
            seen.iter()
                .find(|s| s.text == t)
                .unwrap_or_else(|| panic!("„{t}“ fehlt: {:?}", texts(&seen)))
                .rect
        };
        assert_eq!(SECTION_APPEARANCE, "Darstellung");
        assert_eq!(TRANSPARENCY_LABEL, "Durchsichtigkeit der Panels");
        let appearance = find(SECTION_APPEARANCE);
        let label = find(TRANSPARENCY_LABEL);
        let (shown, value) = shown_value(&seen);
        assert_eq!(shown, 40, "Voreinstellung 40 %");
        let projects = find(SECTION_PROJECTS);
        assert!(
            appearance.bottom() <= label.top(),
            "Überschrift über dem Regler"
        );
        assert!(
            value.bottom() <= projects.top(),
            "Darstellung über Projekte"
        );
        assert!(appearance.bottom() <= projects.top());
    }
}

#[test]
fn moving_the_slider_applies_at_once_and_saves() {
    let e = env();
    write_config(&e.cfg, &e.root, vec![e.tmp.join("Fav")]);
    let mut a = App::new(e, true, 1400.0, 900.0);
    a.w.open(false);
    click_slider(&mut a, 1.0);
    let v = a.e.ws.panel_transparency();
    assert!(v >= 76, "rechtes Ende: {v}");
    let disk = Config::read_from(&a.e.cfg).unwrap();
    assert_eq!(disk.panel_transparency, v, "gespeichert");
    assert_eq!(
        disk.favorites,
        vec![a.e.tmp.join("Fav")],
        "Favoriten bleiben"
    );
    assert_eq!(disk.root, Some(a.e.root.clone()), "Verzeichnis bleibt");
    assert_eq!(a.w.appearance_message(), None);
    let seen = a.settle(3);
    assert_eq!(shown_value(&seen).0, v, "Zahl neu");
    click_slider(&mut a, 0.0);
    assert!(a.e.ws.panel_transparency() <= 4);
    assert_eq!(
        Config::read_from(&a.e.cfg).unwrap().panel_transparency,
        a.e.ws.panel_transparency()
    );
}

#[test]
fn a_failed_save_keeps_the_transparency_for_the_session_with_a_hint() {
    for dark in [false, true] {
        let mut e = env();
        let blocker = e.tmp.join("blockiert");
        fs::write(&blocker, b"x").unwrap();
        e.ws.config_path = blocker.join("config.json");
        let mut a = App::new(e, dark, 1400.0, 900.0);
        a.w.open(false);
        click_slider(&mut a, 1.0);
        assert!(a.e.ws.panel_transparency() >= 76, "gilt trotzdem");
        assert_eq!(fs::read(&blocker).unwrap(), b"x");
        assert_eq!(
            a.w.appearance_message(),
            Some(&RootMessage::Warning(TRANSPARENCY_NOT_SAVED.into()))
        );
        assert_eq!(
            TRANSPARENCY_NOT_SAVED,
            "Die Durchsichtigkeit gilt nur bis zum Programmende: die Einstellung konnte nicht gespeichert werden."
        );
        let seen = a.settle(3);
        let shot = seen
            .iter()
            .find(|s| s.text == TRANSPARENCY_NOT_SAVED)
            .expect("Hinweis gezeichnet");
        assert_eq!(shot.color, theme::palette(dark).warning);
        // Neu öffnen leert den Hinweis.
        a.w.close();
        a.w.open(false);
        assert_eq!(a.w.appearance_message(), None);
    }
}

#[test]
fn the_slider_is_locked_while_a_prompt_is_open() {
    let e = env();
    let mut a = App::new(e, true, 1400.0, 900.0);
    a.e.ws.prompt = Some(planora_app::workspace::Prompt::Notice("Hinweis".into()));
    a.w.open(false);
    click_slider(&mut a, 1.0);
    assert_eq!(a.e.ws.panel_transparency(), 40);
    assert!(!a.e.cfg.exists());
}

#[test]
fn an_unsaved_slider_value_survives_closing_from_outside() {
    let e = env();
    write_config(&e.cfg, &e.root, vec![]);
    let mut a = App::new(e, true, 1400.0, 900.0);
    a.w.open(false);
    a.settle(2);
    // Wie beim Ziehen: der Wert gilt schon, gespeichert ist er noch nicht.
    a.w.preview_transparency(&mut a.e.ws, 66);
    assert_eq!(a.e.ws.panel_transparency(), 66);
    assert_eq!(Config::read_from(&a.e.cfg).unwrap().panel_transparency, 40);
    assert!(a.e.ws.panel_transparency_pending, "vorgemerkt");
    // Von außen geschlossen (ohne Arbeitsbereich): der Wert bleibt vorgemerkt …
    a.w.close();
    assert_eq!(a.e.ws.panel_transparency(), 66);
    assert!(a.e.ws.panel_transparency_pending);
    // … und kommt beim nächsten gelungenen Schreiben in die Datei.
    assert!(a.e.ws.try_persist_config());
    assert_eq!(Config::read_from(&a.e.cfg).unwrap().panel_transparency, 66);
    assert!(!a.e.ws.panel_transparency_pending);
}

#[test]
fn closing_the_window_saves_an_unsaved_slider_value() {
    let e = env();
    write_config(&e.cfg, &e.root, vec![]);
    let mut a = App::new(e, true, 1400.0, 900.0);
    a.w.open(false);
    a.settle(2);
    a.w.preview_transparency(&mut a.e.ws, 55);
    a.click("Schließen");
    assert!(!a.w.is_open());
    assert_eq!(Config::read_from(&a.e.cfg).unwrap().panel_transparency, 55);
    assert!(!a.e.ws.panel_transparency_pending);
}
