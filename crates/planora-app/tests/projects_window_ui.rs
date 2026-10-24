//! Fenster „Projekte“ im Bild: Kacheln mit Name, Datum und Version, Auswahl, Pfad, Menüs,
//! Tastatur, Knöpfe und Miniaturen (ohne Fenster, Rohereignisse je Bild). Jede Bibliothek liegt
//! unter `common::temp_root`, jede Konfiguration in einem eigenen Temp-Ordner.

mod common;

use common::{Screen, Seen};
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect};
use planora_app::icons::{self, Icon};
use planora_app::project_actions::NO_LIBRARY;
use planora_app::project_tiles::format_changed;
use planora_app::projects_window::*;
use planora_app::settings_window::change_root;
use planora_app::state::AppState;
use planora_app::theme;
use planora_app::thumbnail_cache::{
    BuildLimits, IMPOSSIBLE_TEXT, PREVIEW_JSON, Thumb, ThumbOutcome, thumbnail_for,
};
use planora_app::thumbnail_worker::{BUSY_RETRY, ThumbRequest, ThumbWorker};
use planora_app::workspace::{Prompt, Workspace};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::docstore::{DocStore, peek};
use planora_project::library::{DOCUMENT_FILE, Library, NodePath};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(60);

// ---- Umgebung --------------------------------------------------------------------------------

/// Arbeitsbereich mit leerer Bibliothek in Temp und eigener Konfiguration.
fn workspace() -> Workspace {
    let root = common::temp_root("planora-kacheln");
    let lib = Library::open(&root).unwrap();
    with_config(Workspace::new(Some(lib), Config::default()))
}

fn with_config(mut ws: Workspace) -> Workspace {
    let tmp = common::temp_root("planora-kacheln-tmp");
    fs::create_dir_all(&tmp).unwrap();
    ws.config_path = tmp.join("config.json");
    ws
}

fn lib(ws: &Workspace) -> &Library {
    ws.library.as_ref().unwrap()
}

fn project(ws: &Workspace, name: &str) -> NodePath {
    lib(ws).create_project(name).unwrap()
}

fn folder(ws: &Workspace, parent: &NodePath, name: &str) -> NodePath {
    lib(ws).create_folder(parent, name).unwrap()
}

/// Dokument `name` in `parent` mit dem Stand `state` (ohne Tab); liefert Pfad und Ordner.
fn document(
    ws: &Workspace,
    parent: &NodePath,
    name: &str,
    state: &AppState,
) -> (NodePath, PathBuf) {
    let mut path = parent.clone();
    path.push(format!("{name}.pcad"));
    let dir = lib(ws).root().resolve(&path).unwrap();
    drop(DocStore::create(&dir, &state.snapshot(), "").unwrap());
    (path, dir)
}

/// Dokument mit einem Quader.
fn box_doc(ws: &Workspace, parent: &NodePath, name: &str) -> (NodePath, PathBuf) {
    let mut s = AppState::new();
    s.create_box(DVec3::new(20.0, 10.0, 8.0)).unwrap();
    document(ws, parent, name, &s)
}

/// Dokument ohne Körper.
fn empty_doc(ws: &Workspace, parent: &NodePath, name: &str) -> (NodePath, PathBuf) {
    document(ws, parent, name, &AppState::new())
}

// ---- Bilder ----------------------------------------------------------------------------------

struct Shot {
    seen: Vec<Seen>,
    shapes: Vec<ClippedShape>,
}

impl Shot {
    fn find(&self, text: &str) -> Option<&Seen> {
        self.seen.iter().find(|s| s.text == text)
    }

    fn has(&self, text: &str) -> bool {
        self.find(text).is_some()
    }

    fn at(&self, text: &str) -> Pos2 {
        self.find(text)
            .unwrap_or_else(|| panic!("„{text}“ fehlt: {}", self.texts()))
            .rect
            .center()
    }

    fn texts(&self) -> String {
        self.seen
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// Alle Formen (auch verschachtelte).
    fn flat(&self) -> Vec<&Shape> {
        fn walk<'a>(s: &'a Shape, out: &mut Vec<&'a Shape>) {
            match s {
                Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                other => out.push(other),
            }
        }
        let mut out = Vec::new();
        for c in &self.shapes {
            walk(&c.shape, &mut out);
        }
        out
    }

    /// Farbe des Texts `text` (erste Abschnittsfarbe, sonst die Ersatzfarbe der Form).
    fn color_of(&self, text: &str) -> egui::Color32 {
        for s in self.flat() {
            if let Shape::Text(t) = s
                && t.galley.text() == text
            {
                if let Some(c) = t.override_text_color {
                    return c;
                }
                return t
                    .galley
                    .job
                    .sections
                    .first()
                    .map(|s| s.format.color)
                    .filter(|c| *c != egui::Color32::PLACEHOLDER)
                    .unwrap_or(t.fallback_color);
            }
        }
        panic!("„{text}“ fehlt: {}", self.texts());
    }

    /// Ein Rechteck mit 2-px-Rand in `color` genau um `rect`?
    fn border_around(&self, rect: Rect, color: egui::Color32) -> bool {
        self.flat().iter().any(|s| match s {
            Shape::Rect(r) => {
                r.stroke.color == color
                    && (r.stroke.width - 2.0).abs() < 0.01
                    && (r.rect.min - rect.min).length() < 1.0
                    && (r.rect.max - rect.max).length() < 1.0
            }
            _ => false,
        })
    }

    /// Ein Bild (Netz oder Rechteck mit eigener Textur; egui malt Bilder als Rechteck mit
    /// Textur) in `area`?
    fn texture_in(&self, area: Rect) -> bool {
        self.texture_id_in(area).is_some()
    }

    /// Textur des Bilds in `area`.
    fn texture_id_in(&self, area: Rect) -> Option<egui::TextureId> {
        let area = area.expand(0.5);
        let none = egui::TextureId::default();
        self.flat().iter().find_map(|s| match s {
            Shape::Mesh(m)
                if m.texture_id != none
                    && !m.vertices.is_empty()
                    && area.contains_rect(m.calc_bounds()) =>
            {
                Some(m.texture_id)
            }
            Shape::Rect(r) if r.fill_texture_id() != none && area.contains_rect(r.rect) => {
                Some(r.fill_texture_id())
            }
            _ => None,
        })
    }

    /// Eine Linie (etwa die Fortschrittsanzeige) in `area`?
    fn path_in(&self, area: Rect) -> bool {
        self.flat().iter().any(|s| match s {
            Shape::Path(p) => p.points.iter().all(|q| area.contains(*q)),
            _ => false,
        })
    }
}

/// Fenster „Projekte“ mit Arbeitsbereich in einem fensterlosen Kontext.
struct H {
    screen: Screen,
    w: ProjectsWindow,
    ws: Workspace,
    events: Vec<ProjectsEvent>,
}

impl H {
    fn new(ws: Workspace) -> H {
        H::sized(ws, 1400.0, 900.0)
    }

    fn sized(ws: Workspace, width: f32, height: f32) -> H {
        let screen = Screen::new(width, height);
        theme::apply(&screen.ctx, ws.active_tab().state.dark);
        let mut w = ProjectsWindow::default();
        w.open();
        H {
            screen,
            w,
            ws,
            events: Vec::new(),
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> Shot {
        let H {
            screen,
            w,
            ws,
            events: ev,
        } = self;
        let (seen, shapes) = screen.frame_shapes(events, &mut |ctx| ev.push(w.show(ctx, ws)));
        Shot { seen, shapes }
    }

    fn settle(&mut self, n: usize) -> Shot {
        let mut shot = self.frame(vec![]);
        for _ in 1..n {
            shot = self.frame(vec![]);
        }
        shot
    }

    /// Lesen abwarten, dann einige Bilder.
    fn ready(&mut self) -> Shot {
        self.settle(2);
        self.w.wait_for_scan(&self.ws);
        self.settle(4)
    }

    fn button(&mut self, at: Pos2, button: PointerButton, pressed: bool) -> Shot {
        self.frame(vec![Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        }])
    }

    /// Pause länger als die Frist für einen Doppelklick, damit Klicks nicht zusammenfallen.
    fn pause(&mut self) {
        self.settle(8);
    }

    fn click_with(&mut self, at: Pos2, button: PointerButton) -> Shot {
        self.pause();
        self.frame(vec![Event::PointerMoved(at)]);
        self.button(at, button, true);
        self.button(at, button, false);
        self.settle(3)
    }

    fn click(&mut self, at: Pos2) -> Shot {
        self.click_with(at, PointerButton::Primary)
    }

    fn right_click(&mut self, at: Pos2) -> Shot {
        self.click_with(at, PointerButton::Secondary)
    }

    fn double_click(&mut self, at: Pos2) -> Shot {
        self.pause();
        self.frame(vec![Event::PointerMoved(at)]);
        for _ in 0..2 {
            self.button(at, PointerButton::Primary, true);
            self.button(at, PointerButton::Primary, false);
        }
        self.settle(3)
    }

    fn key(&mut self, key: Key) -> Shot {
        let ev = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![ev(true)]);
        self.frame(vec![ev(false)]);
        self.settle(2)
    }

    /// Tippt `text`, ein Zeichen je Bild.
    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.frame(vec![Event::Text(c.to_string())]);
        }
    }

    fn index(&self, name: &str) -> usize {
        self.w
            .tiles()
            .iter()
            .position(|t| t.name == name)
            .unwrap_or_else(|| panic!("Kachel „{name}“ fehlt"))
    }

    /// Rechteck der Kachel `name` im letzten Bild.
    fn tile(&self, name: &str) -> Rect {
        self.w.tile_rects()[self.index(name)]
    }

    /// Bildfläche der Kachel `name` (oberer Teil bis unter die Miniatur).
    fn image_area(&self, name: &str) -> Rect {
        let t = self.tile(name);
        Rect::from_min_max(t.min, egui::pos2(t.max.x, t.min.y + 160.0))
    }
}

fn palette(h: &H) -> &'static theme::Palette {
    theme::palette(h.ws.active_tab().state.dark)
}

// ---- Tests -----------------------------------------------------------------------------------

#[test]
fn tiles_show_name_date_version_and_count() {
    let ws = workspace();
    let p = project(&ws, "Gehäuse");
    let (_, dir) = box_doc(&ws, &p, "Teil");
    let (info, _) = peek(&dir).unwrap();
    let time = info.version(1).unwrap().time.clone();
    let mut h = H::new(ws);
    let shot = h.ready();
    assert!(shot.has("Gehäuse"), "{}", shot.texts());
    let changed = format!("Geändert {}", format_changed(&time).unwrap());
    assert!(shot.has(&changed), "{changed}: {}", shot.texts());
    assert!(shot.has("Version 1 · 1 Dokument"), "{}", shot.texts());
    assert!(shot.has("1 Projekt"), "Fußzeile: {}", shot.texts());
    assert!(shot.has(FOOTER_HINT));
}

#[test]
fn an_empty_library_shows_the_empty_text_and_no_library_shows_the_directory_button() {
    let mut h = H::new(workspace());
    let shot = h.ready();
    assert!(shot.has(EMPTY), "{}", shot.texts());
    assert!(!shot.texts().contains("Wurzel"));

    let mut h = H::new(with_config(Workspace::new(None, Config::default())));
    let shot = h.settle(4);
    assert!(shot.has(NO_LIBRARY), "{}", shot.texts());
    assert!(shot.has(DIRECTORY_BUTTON), "{}", shot.texts());
    assert!(!shot.texts().contains("Wurzel"));
    assert!(!shot.has(EMPTY));
}

#[test]
fn click_selects_with_accent_border_and_double_click_goes_in() {
    let ws = workspace();
    let p = project(&ws, "P");
    project(&ws, "Q");
    folder(&ws, &p, "F");
    let mut h = H::new(ws);
    let shot = h.ready();
    let accent = palette(&h).accent;
    let rect = h.tile("P");
    assert!(!shot.border_around(rect, accent), "noch nichts gewählt");
    let shot = h.click(rect.center());
    assert_eq!(h.w.ui.selected(), Some(&p));
    assert!(
        shot.border_around(h.tile("P"), accent),
        "Akzentrand um die gewählte Kachel"
    );
    assert!(!shot.border_around(h.tile("Q"), accent));
    h.double_click(h.tile("P").center());
    assert_eq!(h.w.location(), &p);
    let shot = h.settle(2);
    assert!(shot.has("F"), "{}", shot.texts());
}

#[test]
fn the_breadcrumb_goes_back() {
    let ws = workspace();
    let p = project(&ws, "P");
    let f = folder(&ws, &p, "F");
    folder(&ws, &f, "G");
    let mut h = H::new(ws);
    h.ready();
    h.double_click(h.tile("P").center());
    h.double_click(h.tile("F").center());
    assert_eq!(h.w.location(), &f);
    let shot = h.settle(2);
    assert!(shot.has("G"));
    let shot = h.click(shot.at(&icons::glyph(Icon::Back).to_string()));
    assert_eq!(h.w.location(), &p);
    h.click(shot.at("F"));
    assert_eq!(
        h.w.location(),
        &p,
        "„F“ ist hier eine Kachel, kein Pfadteil"
    );
    h.double_click(h.tile("F").center());
    let shot = h.settle(2);
    // der Pfadteil „P“ führt eine Ebene hinauf
    let crumb = shot
        .seen
        .iter()
        .filter(|s| s.text == "P")
        .map(|s| s.rect.center())
        .next()
        .unwrap();
    h.click(crumb);
    assert_eq!(h.w.location(), &p);
    let shot = h.settle(2);
    // „Projekte“ steht auch im Titel; der Pfadteil kommt danach
    let crumb = shot
        .seen
        .iter()
        .rfind(|s| s.text == TITLE)
        .unwrap()
        .rect
        .center();
    h.click(crumb);
    assert!(h.w.location().is_empty());
}

#[test]
fn right_click_menus_offer_the_spec_entries() {
    let ws = workspace();
    let p = project(&ws, "P");
    folder(&ws, &p, "F");
    empty_doc(&ws, &p, "D");
    let mut h = H::new(ws);
    h.ready();
    let shot = h.right_click(h.tile("P").center());
    assert!(shot.has(RENAME) && shot.has(DELETE), "{}", shot.texts());
    assert!(!shot.has(MOVE), "Projekte lassen sich nicht verschieben");
    h.key(Key::Escape);
    h.double_click(h.tile("P").center());
    h.settle(2);
    for name in ["F", "D"] {
        let shot = h.right_click(h.tile(name).center());
        for entry in [RENAME, MOVE, DELETE] {
            assert!(shot.has(entry), "{name}: {entry} fehlt: {}", shot.texts());
        }
        h.key(Key::Escape);
    }
    // ein Eintrag öffnet das Fenster der Aktion
    let shot = h.right_click(h.tile("F").center());
    let shot = h.click(shot.at(RENAME));
    assert!(shot.has("Neuer Name für „P / F“:"), "{}", shot.texts());
}

#[test]
fn tab_reaches_every_tile_and_enter_opens_it() {
    let ws = workspace();
    let p = project(&ws, "P");
    let (a, _) = empty_doc(&ws, &p, "A");
    let (b, _) = empty_doc(&ws, &p, "B");
    folder(&ws, &p, "C");
    let mut h = H::new(ws);
    h.ready();
    h.w.go_to(p.clone());
    h.settle(3);
    let target = h.index("B");
    let mut reached = Vec::new();
    for _ in 0..40 {
        h.key(Key::Tab);
        if let Some(i) = h.w.focused_tile() {
            if !reached.contains(&i) {
                reached.push(i);
            }
            if i == target {
                break;
            }
        }
    }
    assert_eq!(
        h.w.focused_tile(),
        Some(target),
        "Tab erreicht „B“: {reached:?}"
    );
    assert!(
        reached.contains(&h.index("A")),
        "„A“ vorher erreicht: {reached:?}"
    );
    let tabs = h.ws.tabs.len();
    let shot = h.settle(1);
    let accent = palette(&h).accent;
    let ring = h.tile("B").expand(1.0);
    assert!(
        shot.flat().iter().any(|s| matches!(s, Shape::Rect(r)
            if r.stroke.color == accent && (r.rect.min - ring.min).length() < 1.5)),
        "Fokusring um „B“"
    );
    h.key(Key::Enter);
    assert!(h.ws.tab_for_path(&b).is_some(), "„B“ geöffnet");
    assert!(h.ws.tab_for_path(&a).is_none(), "nur „B“");
    assert!(h.ws.tabs.len() <= tabs + 1);
    assert!(h.w.is_open(), "das Fenster bleibt offen");
}

#[test]
fn new_project_button_creates_and_selects() {
    let mut h = H::new(workspace());
    let shot = h.ready();
    h.click(shot.at(NEW_PROJECT));
    let shot = h.settle(2);
    assert!(
        shot.has("Anlegen"),
        "Fenster „Neues Projekt“: {}",
        shot.texts()
    );
    h.type_text("Neu");
    h.key(Key::Enter);
    h.w.wait_for_scan(&h.ws);
    let shot = h.settle(3);
    assert!(shot.has("Neu"), "{}", shot.texts());
    assert_eq!(h.w.ui.selected(), Some(&vec!["Neu".to_string()]));
    assert!(shot.border_around(h.tile("Neu"), palette(&h).accent));
}

#[test]
fn new_document_button_creates_opens_and_shows_the_tile() {
    let ws = workspace();
    let p = project(&ws, "P");
    let mut h = H::new(ws);
    let shot = h.ready();
    assert!(!shot.has(NEW_DOCUMENT), "nur in einem Projekt");
    h.double_click(h.tile("P").center());
    let shot = h.settle(2);
    h.click(shot.at(NEW_DOCUMENT));
    h.settle(2);
    h.type_text("Teil");
    h.key(Key::Enter);
    let path: NodePath = vec!["P".into(), "Teil.pcad".into()];
    assert!(h.ws.tab_for_path(&path).is_some(), "Tab geöffnet");
    h.w.wait_for_scan(&h.ws);
    let shot = h.settle(3);
    assert!(
        shot.has("Teil") && shot.has("Version 1"),
        "{}",
        shot.texts()
    );
    assert_eq!(h.w.location(), &p);
}

#[test]
fn thumbnails_arrive_without_blocking_and_show_as_texture() {
    let ws = workspace();
    let p = project(&ws, "P");
    let (_, dir) = box_doc(&ws, &p, "Teil");
    let mut h = H::new(ws);
    h.ready();
    assert!(h.w.wait_for_thumbnails(WAIT), "Miniatur fertig");
    assert!(matches!(h.w.thumb_state(&dir), Some(Thumb::Image(_))));
    let shot = h.settle(3);
    assert!(shot.texture_in(h.image_area("P")), "Bild in der Kachel");
    // auch im Projekt, auf der Dokumentkachel
    h.double_click(h.tile("P").center());
    h.settle(2);
    assert!(h.w.wait_for_thumbnails(WAIT));
    let shot = h.settle(3);
    assert!(
        shot.texture_in(h.image_area("Teil")),
        "Bild in der Dokumentkachel"
    );
}

/// Arbeiter, der vor jedem Auftrag auf `gate` wartet und Aufträge zählt.
fn gated_worker(gate: Arc<Mutex<()>>, count: Arc<AtomicUsize>) -> ThumbWorker {
    ThumbWorker::start_with(move |r: &ThumbRequest| {
        count.fetch_add(1, Ordering::SeqCst);
        let _g = gate.lock().unwrap_or_else(|e| e.into_inner());
        thumbnail_for(&r.dir, &r.key, &BuildLimits::default())
    })
}

#[test]
fn the_frame_never_waits_for_a_slow_thumbnail() {
    let ws = workspace();
    let p = project(&ws, "P");
    let (_, dir) = box_doc(&ws, &p, "Teil");
    let gate = Arc::new(Mutex::new(()));
    let count = Arc::new(AtomicUsize::new(0));
    let mut h = H::new(ws);
    h.w.set_worker(gated_worker(gate.clone(), count.clone()));
    let held = gate.lock().unwrap();
    h.ready();
    let mut slowest = Duration::ZERO;
    let mut shot = h.settle(1);
    for _ in 0..5 {
        let t0 = Instant::now();
        shot = h.frame(vec![]);
        slowest = slowest.max(t0.elapsed());
    }
    assert!(
        slowest < Duration::from_millis(50),
        "Bild dauerte {slowest:?}"
    );
    assert!(h.w.thumb_state(&dir).is_none(), "noch nicht fertig");
    assert!(
        shot.path_in(h.image_area("P")),
        "ruhige Fortschrittsanzeige"
    );
    drop(held);
    assert!(h.w.wait_for_thumbnails(WAIT));
    let shot = h.settle(3);
    assert!(shot.texture_in(h.image_area("P")));
    assert!(!shot.path_in(h.image_area("P")), "Anzeige weg");
}

#[test]
fn placeholders_name_no_bodies_and_too_complex() {
    let ws = workspace();
    let p = project(&ws, "Leer");
    empty_doc(&ws, &p, "Teil");
    let q = project(&ws, "Schwer");
    box_doc(&ws, &q, "Teil");
    let r = project(&ws, "Kaputt");
    box_doc(&ws, &r, "Teil");
    project(&ws, "Ohne");
    let mut h = H::new(ws);
    h.w.set_worker(ThumbWorker::start_with(|r: &ThumbRequest| {
        let name = r.dir.to_string_lossy().into_owned();
        let fake = |thumb| ThumbOutcome {
            thumb,
            stored: true,
            from_cache: false,
        };
        if name.contains("Schwer") {
            fake(Thumb::TooComplex)
        } else if name.contains("Kaputt") {
            fake(Thumb::Impossible)
        } else {
            thumbnail_for(&r.dir, &r.key, &BuildLimits::default())
        }
    }));
    h.ready();
    assert!(h.w.wait_for_thumbnails(WAIT));
    let shot = h.settle(3);
    assert!(
        shot.has("Noch keine Körper · 1 Dokument"),
        "{}",
        shot.texts()
    );
    let inside = |shot: &Shot, text: &str, area: Rect| {
        shot.seen
            .iter()
            .any(|s| s.text == text && area.contains_rect(s.rect))
    };
    assert!(
        inside(&shot, NO_BODIES, h.image_area("Leer")),
        "{}",
        shot.texts()
    );
    assert!(inside(&shot, TOO_COMPLEX, h.image_area("Schwer")));
    assert!(inside(&shot, IMPOSSIBLE_TEXT, h.image_area("Kaputt")));
    assert!(
        inside(&shot, NO_BODIES, h.image_area("Ohne")),
        "Projekt ohne Dokument"
    );
    assert!(shot.has("Noch kein Dokument"));
}

#[test]
fn unreadable_documents_are_shown_in_warning_colour() {
    let ws = workspace();
    let p = project(&ws, "P");
    let (_, dir) = empty_doc(&ws, &p, "Teil");
    fs::write(dir.join(DOCUMENT_FILE), b"kaputt").unwrap();
    let mut h = H::new(ws);
    let shot = h.ready();
    let warning = palette(&h).warning;
    assert_eq!(shot.color_of("Dokumente nicht lesbar"), warning);
    h.double_click(h.tile("P").center());
    let shot = h.settle(2);
    assert_eq!(shot.color_of("Dokument nicht lesbar"), warning);
    assert_ne!(shot.color_of("Teil"), warning, "nur die Angabe");
}

#[test]
fn refresh_rereads_external_changes() {
    let ws = workspace();
    let root = lib(&ws).root().path().to_path_buf();
    let mut h = H::new(ws);
    h.ready();
    Library::open(&root)
        .unwrap()
        .create_project("Außen")
        .unwrap();
    let shot = h.settle(4);
    assert!(!shot.has("Außen"), "ohne Aktualisieren nicht neu gelesen");
    h.click(shot.at(REFRESH));
    h.w.wait_for_scan(&h.ws);
    let shot = h.settle(3);
    assert!(shot.has("Außen"), "{}", shot.texts());
}

#[test]
fn a_prompt_disables_the_tiles() {
    let ws = workspace();
    project(&ws, "P");
    let mut h = H::new(ws);
    h.ready();
    h.ws.prompt = Some(Prompt::Notice("Hinweis".into()));
    h.click(h.tile("P").center());
    h.double_click(h.tile("P").center());
    assert!(h.w.location().is_empty(), "gesperrt");
    assert_eq!(h.w.ui.selected(), None);
    h.ws.prompt = None;
    h.double_click(h.tile("P").center());
    assert_eq!(h.w.location(), &vec!["P".to_string()]);
}

#[test]
fn the_directory_button_asks_for_the_settings() {
    let mut h = H::new(workspace());
    let shot = h.ready();
    h.click(shot.at(DIRECTORY_BUTTON));
    assert!(
        h.events.contains(&ProjectsEvent::OpenSettings),
        "{:?}",
        h.events
    );
}

#[test]
fn closing_stops_pending_thumbnails() {
    let ws = workspace();
    for i in 0..5 {
        let p = project(&ws, &format!("P{i}"));
        box_doc(&ws, &p, "Teil");
    }
    let gate = Arc::new(Mutex::new(()));
    let count = Arc::new(AtomicUsize::new(0));
    let mut h = H::new(ws);
    h.w.set_worker(gated_worker(gate.clone(), count.clone()));
    let held = gate.lock().unwrap();
    let shot = h.ready();
    let close = icons::glyph(Icon::Close).to_string();
    h.click(shot.at(&close));
    assert!(h.events.contains(&ProjectsEvent::Closed), "{:?}", h.events);
    assert!(!h.w.is_open());
    drop(held);
    let worker = h.w.worker().expect("Arbeiter");
    assert!(worker.wait_idle(WAIT), "Warteschlange leer");
    let started = count.load(Ordering::SeqCst);
    assert!(started <= 1, "nur der laufende Auftrag, nicht {started}");
}

#[test]
fn a_failed_store_is_not_retried_in_this_session() {
    let ws = workspace();
    let p = project(&ws, "P");
    let (_, dir) = box_doc(&ws, &p, "Teil");
    fs::create_dir_all(dir.join(PREVIEW_JSON)).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let mut h = H::new(ws);
    h.w.set_worker(gated_worker(Arc::new(Mutex::new(())), count.clone()));
    let shot = h.ready();
    assert!(h.w.wait_for_thumbnails(WAIT));
    h.settle(20);
    h.click(shot.at(REFRESH));
    h.w.wait_for_scan(&h.ws);
    h.settle(5);
    assert!(h.w.wait_for_thumbnails(WAIT));
    let shot = h.settle(2);
    assert_eq!(count.load(Ordering::SeqCst), 1, "genau ein Auftrag");
    assert!(matches!(h.w.thumb_state(&dir), Some(Thumb::Image(_))));
    assert!(
        shot.texture_in(h.image_area("P")),
        "Miniatur aus dem Speicher"
    );
    assert!(dir.join(PREVIEW_JSON).is_dir(), "nichts geschrieben");
}

fn preview_json(dir: &Path) -> String {
    fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap_or_default()
}

#[test]
fn a_theme_change_requests_new_thumbnails() {
    let ws = workspace();
    let p = project(&ws, "P");
    let (_, dir) = box_doc(&ws, &p, "Teil");
    let mut h = H::new(ws);
    assert!(h.ws.active_tab().state.dark);
    h.ready();
    assert!(h.w.wait_for_thumbnails(WAIT));
    assert!(
        preview_json(&dir).contains("\"dark\":true"),
        "{}",
        preview_json(&dir)
    );
    let shot = h.settle(2);
    let dark_texture = shot.texture_id_in(h.image_area("P")).expect("dunkles Bild");
    h.ws.set_dark(false);
    theme::apply(&h.screen.ctx, false);
    h.settle(3);
    assert!(h.w.wait_for_thumbnails(WAIT));
    let shot = h.settle(3);
    assert!(
        preview_json(&dir).contains("\"dark\":false"),
        "{}",
        preview_json(&dir)
    );
    assert!(matches!(h.w.thumb_state(&dir), Some(Thumb::Image(_))));
    let light_texture = shot.texture_id_in(h.image_area("P")).expect("helles Bild");
    assert_ne!(light_texture, dark_texture, "neue Textur für das neue Bild");
}

#[test]
fn a_result_for_an_old_key_is_dropped() {
    // Während das dunkle Bild entsteht, wechselt das Design: das dunkle Ergebnis gilt nicht.
    let ws = workspace();
    let p = project(&ws, "P");
    let (_, dir) = box_doc(&ws, &p, "Teil");
    // je ein Riegel für das dunkle und das helle Bild
    let dark_gate = Arc::new(Mutex::new(()));
    let light_gate = Arc::new(Mutex::new(()));
    let count = Arc::new(AtomicUsize::new(0));
    let (dg, lg, c) = (dark_gate.clone(), light_gate.clone(), count.clone());
    let mut h = H::new(ws);
    h.w.set_worker(ThumbWorker::start_with(move |r: &ThumbRequest| {
        c.fetch_add(1, Ordering::SeqCst);
        let gate = if r.key.dark { &dg } else { &lg };
        let _g = gate.lock().unwrap_or_else(|e| e.into_inner());
        thumbnail_for(&r.dir, &r.key, &BuildLimits::default())
    }));
    let dark_held = dark_gate.lock().unwrap();
    let light_held = light_gate.lock().unwrap();
    h.ready();
    let wait_for = |h: &mut H, n: usize| {
        let deadline = Instant::now() + WAIT;
        while count.load(Ordering::SeqCst) < n && Instant::now() < deadline {
            h.frame(vec![]);
        }
        assert_eq!(count.load(Ordering::SeqCst), n);
    };
    wait_for(&mut h, 1);
    h.ws.set_dark(false);
    theme::apply(&h.screen.ctx, false);
    h.settle(3);
    drop(dark_held);
    // das dunkle Ergebnis kommt an, das helle ist in Arbeit
    wait_for(&mut h, 2);
    h.settle(3);
    assert!(h.w.thumb_state(&dir).is_none(), "altes Ergebnis übernommen");
    drop(light_held);
    assert!(h.w.wait_for_thumbnails(WAIT));
    h.settle(2);
    assert!(matches!(h.w.thumb_state(&dir), Some(Thumb::Image(_))));
    assert!(
        preview_json(&dir).contains("\"dark\":false"),
        "{}",
        preview_json(&dir)
    );
}

#[test]
fn a_busy_document_is_asked_again_later() {
    // Eine andere Instanz baut gerade: nicht sofort, aber nach BUSY_RETRY erneut anfragen.
    let ws = workspace();
    let p = project(&ws, "P");
    let (_, dir) = box_doc(&ws, &p, "Teil");
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let mut h = H::new(ws);
    h.w.set_worker(ThumbWorker::start_with(move |r: &ThumbRequest| {
        if c.fetch_add(1, Ordering::SeqCst) == 0 {
            ThumbOutcome {
                thumb: Thumb::Busy,
                stored: false,
                from_cache: false,
            }
        } else {
            thumbnail_for(&r.dir, &r.key, &BuildLimits::default())
        }
    }));
    h.ready();
    let start = Instant::now();
    let deadline = start + WAIT;
    while !matches!(h.w.thumb_state(&dir), Some(Thumb::Busy)) && Instant::now() < deadline {
        h.frame(vec![]);
    }
    assert!(matches!(h.w.thumb_state(&dir), Some(Thumb::Busy)));
    h.settle(5);
    assert_eq!(count.load(Ordering::SeqCst), 1, "nicht sofort erneut");
    while count.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        h.frame(vec![]);
    }
    assert_eq!(
        count.load(Ordering::SeqCst),
        2,
        "nach der Wartezeit erneut angefragt"
    );
    assert!(start.elapsed() >= BUSY_RETRY - Duration::from_millis(100));
    assert!(h.w.wait_for_thumbnails(WAIT));
    assert!(matches!(h.w.thumb_state(&dir), Some(Thumb::Image(_))));
}

#[test]
fn visible_tiles_are_built_first() {
    let ws = workspace();
    for i in 0..24 {
        let p = project(&ws, &format!("P{i:02}"));
        box_doc(&ws, &p, "Teil");
    }
    let gate = Arc::new(Mutex::new(()));
    let order = Arc::new(Mutex::new(Vec::<String>::new()));
    let (g, o) = (gate.clone(), order.clone());
    let mut h = H::sized(ws, 1400.0, 900.0);
    h.w.set_worker(ThumbWorker::start_with(move |r: &ThumbRequest| {
        let project = r
            .dir
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        o.lock().unwrap().push(project);
        let _g = g.lock().unwrap_or_else(|e| e.into_inner());
        thumbnail_for(&r.dir, &r.key, &BuildLimits::default())
    }));
    let held = gate.lock().unwrap();
    h.ready();
    let deadline = Instant::now() + WAIT;
    while order.lock().unwrap().is_empty() && Instant::now() < deadline {
        h.frame(vec![]);
    }
    // ganz nach unten rollen
    let grid = h.w.grid_rect().expect("Kachelbereich");
    let H { screen, w, ws, .. } = &mut h;
    screen.wheel(grid.center(), -10_000.0, &mut |ctx| {
        w.show(ctx, ws);
    });
    h.settle(2);
    let visible: Vec<String> =
        h.w.tiles()
            .iter()
            .zip(h.w.tile_rects())
            .filter(|(_, r)| r.intersects(grid))
            .map(|(t, _)| t.name.clone())
            .collect();
    assert!(
        !visible.contains(&"P00".to_string()),
        "gerollt: {visible:?}"
    );
    drop(held);
    assert!(h.w.wait_for_thumbnails(WAIT));
    let order = order.lock().unwrap().clone();
    assert_eq!(order.len(), 24, "{order:?}");
    // nach dem laufenden Auftrag kommen die sichtbaren Kacheln
    for name in &order[1..=visible.len()] {
        assert!(
            visible.contains(name),
            "{name} vor den sichtbaren: {order:?} / {visible:?}"
        );
    }
}

#[test]
fn the_first_frame_after_a_directory_change_draws_no_old_tiles() {
    let ws = workspace();
    project(&ws, "Alt");
    let other = common::temp_root("planora-kacheln-anders");
    Library::open(&other)
        .unwrap()
        .create_project("Neu")
        .unwrap();
    let mut h = H::new(ws);
    let shot = h.ready();
    assert!(shot.has("Alt"));
    change_root(&mut h.ws, &other).unwrap();
    let shot = h.frame(vec![]);
    assert!(
        !shot.has("Alt"),
        "alte Kacheln im ersten Bild: {}",
        shot.texts()
    );
    h.w.wait_for_scan(&h.ws);
    let shot = h.settle(3);
    assert!(shot.has("Neu"), "{}", shot.texts());
}

#[test]
fn tiles_are_computed_once_per_scan_not_per_frame() {
    let ws = workspace();
    for i in 0..3 {
        project(&ws, &format!("P{i}"));
    }
    let mut h = H::new(ws);
    h.ready();
    let n = h.w.tile_refreshes();
    h.settle(10);
    h.frame(vec![Event::PointerMoved(h.tile("P1").center())]);
    h.settle(3);
    assert_eq!(h.w.tile_refreshes(), n, "keine neue Berechnung je Bild");
    h.w.go_to(vec!["P1".into()]);
    assert_eq!(h.w.tile_refreshes(), n + 1);
}

#[test]
fn thumbnails_belong_to_the_document_folder_not_the_path() {
    // Zwei Verzeichnisse mit gleichnamigem Projekt und Dokument: nach dem Wechsel kein altes Bild.
    let ws = workspace();
    let p = project(&ws, "P");
    let (_, old_dir) = box_doc(&ws, &p, "Teil");
    let other = common::temp_root("planora-kacheln-zweites");
    let mut other_ws = with_config(Workspace::new(
        Some(Library::open(&other).unwrap()),
        Config::default(),
    ));
    let q = project(&other_ws, "P");
    let (_, new_dir) = empty_doc(&other_ws, &q, "Teil");
    other_ws.library = None;
    let mut h = H::new(ws);
    h.ready();
    assert!(h.w.wait_for_thumbnails(WAIT));
    assert!(matches!(h.w.thumb_state(&old_dir), Some(Thumb::Image(_))));
    change_root(&mut h.ws, &other).unwrap();
    h.settle(2);
    h.w.wait_for_scan(&h.ws);
    h.settle(3);
    assert!(h.w.wait_for_thumbnails(WAIT));
    let shot = h.settle(3);
    assert!(matches!(h.w.thumb_state(&new_dir), Some(Thumb::NoBodies)));
    assert!(
        !shot.texture_in(h.image_area("P")),
        "kein Bild aus dem alten Verzeichnis"
    );
    assert!(shot.has(NO_BODIES), "{}", shot.texts());
}
