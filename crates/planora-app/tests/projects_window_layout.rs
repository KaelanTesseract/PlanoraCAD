//! Fenster „Projekte“: Maße bei 800 × 600, 1400 × 900 und 2000 × 1100 mit 0, 1 und 200
//! Kacheln, in Hell und Dunkel. Das Fenster bleibt im Bild und höchstens 1200 px breit, die
//! Fußzeile bleibt sichtbar, alle Kacheln sind gleich groß, und nichts verschiebt sich beim
//! Überfahren, beim Wählen oder wenn Miniaturen eintreffen.

mod common;

use common::{Screen, Seen, find};
use egui::{Event, Modifiers, PointerButton, Rect, pos2, vec2};
use planora_app::dialog::{DialogSize, limits};
use planora_app::project_tiles::{MAX_TILES, TOO_MANY};
use planora_app::projects_window::*;
use planora_app::state::AppState;
use planora_app::theme;
use planora_app::thumbnail_cache::{BuildLimits, thumbnail_for};
use planora_app::thumbnail_worker::{ThumbRequest, ThumbWorker};
use planora_app::workspace::Workspace;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::docstore::DocStore;
use planora_project::library::Library;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SIZES: [(f32, f32); 3] = [(800.0, 600.0), (1400.0, 900.0), (2000.0, 1100.0)];

/// Bibliothek mit `n` Projekten (Namen mit fester Breite der Ziffern).
fn library(n: usize) -> PathBuf {
    let root = common::temp_root("planora-kacheln-mass");
    let lib = Library::open(&root).unwrap();
    for i in 0..n {
        lib.create_project(&format!("Projekt {i:03}")).unwrap();
    }
    root
}

fn workspace(root: &Path, dark: bool) -> Workspace {
    let mut ws = Workspace::new(Some(Library::open(root).unwrap()), Config::default());
    let tmp = common::temp_root("planora-kacheln-mass-tmp");
    fs::create_dir_all(&tmp).unwrap();
    ws.config_path = tmp.join("config.json");
    ws.set_dark(dark);
    ws
}

fn box_doc(root: &Path, project: &str, name: &str) {
    let lib = Library::open(root).unwrap();
    let path = vec![project.to_string(), format!("{name}.pcad")];
    let dir = lib.root().resolve(&path).unwrap();
    let mut s = AppState::new();
    s.create_box(DVec3::new(12.0, 8.0, 5.0)).unwrap();
    drop(DocStore::create(&dir, &s.snapshot(), "").unwrap());
}

struct H {
    screen: Screen,
    w: ProjectsWindow,
    ws: Workspace,
}

impl H {
    fn new(ws: Workspace, width: f32, height: f32) -> H {
        let screen = Screen::new(width, height);
        theme::apply(&screen.ctx, ws.active_tab().state.dark);
        let mut w = ProjectsWindow::default();
        w.open();
        H { screen, w, ws }
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
        let H { screen, w, ws } = self;
        screen.frame(events, &mut |ctx| {
            w.show(ctx, ws);
        })
    }

    fn settle(&mut self, n: usize) -> Vec<Seen> {
        let mut seen = Vec::new();
        for _ in 0..n {
            seen = self.frame(vec![]);
        }
        seen
    }

    fn ready(&mut self) -> Vec<Seen> {
        self.settle(2);
        self.w.wait_for_scan(&self.ws);
        self.settle(4)
    }

    fn window(&self) -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), self.screen.size)
    }

    fn rect(&self) -> Rect {
        self.w.last_rect.expect("Fenster gezeichnet")
    }

    /// Dialog- und Kachelrechtecke.
    fn rects(&self) -> (Rect, Vec<Rect>) {
        (self.rect(), self.w.tile_rects().to_vec())
    }
}

/// Innenbreite eines Dialogs der Außenbreite `outer`.
fn inner(outer: f32) -> f32 {
    outer - theme::panel_frame(true).total_margin().sum().x
}

fn rows(tiles: &[Rect]) -> Vec<Vec<Rect>> {
    let mut rows: Vec<Vec<Rect>> = Vec::new();
    for t in tiles {
        match rows.last_mut() {
            Some(r) if (r[0].top() - t.top()).abs() <= 0.5 => r.push(*t),
            _ => rows.push(vec![*t]),
        }
    }
    rows
}

#[test]
fn four_columns_at_1100_and_widths_follow_the_window() {
    let content = Rect::from_min_size(pos2(0.0, 0.0), vec2(2000.0, 1100.0));
    let (outer, _) = limits(content, DialogSize::File);
    assert_eq!(outer, 1100.0);
    assert_eq!(columns_for(inner(1100.0)), 4);
    assert_eq!(columns_for(TILE_MIN_W - 1.0), 1, "mindestens eine Spalte");
    assert_eq!(columns_for(2.0 * TILE_MIN_W + TILE_GAP), 2);
    assert_eq!(columns_for(2.0 * TILE_MIN_W + TILE_GAP - 1.0), 1);
    let root = library(9);
    for (w, h) in SIZES {
        let mut x = H::new(workspace(&root, true), w, h);
        x.ready();
        let (rect, tiles) = x.rects();
        let expected = columns_for(inner(rect.width()));
        let first = &rows(&tiles)[0];
        assert_eq!(first.len(), expected, "{w}×{h}: Spalten");
        if (w, h) == (2000.0, 1100.0) {
            assert_eq!(first.len(), 4);
        }
        // die Reihe füllt die Breite (bis auf das Abrunden der Kachelbreite)
        let row_w = first.last().unwrap().right() - first[0].left();
        assert!(
            inner(rect.width()) - row_w < expected as f32 + 0.5,
            "{w}×{h}: Reihe {row_w} von {}",
            inner(rect.width())
        );
        assert!(first[0].width() >= TILE_MIN_W);
        assert!((first[0].height() - TILE_H).abs() < 0.01);
    }
}

#[test]
fn layout_holds_at_all_sizes_with_0_1_and_200_tiles() {
    for n in [0, 1, 200] {
        let root = library(n);
        for (w, h) in SIZES {
            for dark in [false, true] {
                let what = format!("{w}×{h}, {n} Projekte, dunkel {dark}");
                let mut x = H::new(workspace(&root, dark), w, h);
                let seen = x.ready();
                let (rect, tiles) = x.rects();
                assert!(rect.width() <= 1200.0 + 0.5, "{what}: {rect:?}");
                assert!(x.window().contains_rect(rect), "{what}: {rect:?}");
                let refresh = find(&seen, REFRESH).unwrap_or_else(|| panic!("{what}: Fußzeile"));
                assert!(
                    refresh.clip.contains_rect(refresh.rect) && rect.contains_rect(refresh.rect),
                    "{what}: „{REFRESH}“ {:?} in {:?}",
                    refresh.rect,
                    refresh.clip
                );
                let hint = find(&seen, FOOTER_HINT).unwrap_or_else(|| panic!("{what}: Hinweis"));
                assert!(rect.contains_rect(hint.rect.intersect(hint.clip)), "{what}");
                assert_eq!(tiles.len(), n.min(MAX_TILES), "{what}");
                // Kachelbereich mit fester Höhe (P5), ganz im Fenster
                let grid = x.w.grid_rect().expect("Kachelbereich");
                assert!(
                    (grid.height() - grid_height_for(h)).abs() <= 0.5,
                    "{what}: Kachelbereich {} statt {}",
                    grid.height(),
                    grid_height_for(h)
                );
                assert!(rect.contains_rect(grid), "{what}: {grid:?} in {rect:?}");
                for t in &tiles {
                    assert!(
                        (t.size() - tiles[0].size()).abs().max_elem() <= 0.5,
                        "{what}: Größe {t:?} / {:?}",
                        tiles[0]
                    );
                    assert!(t.right() <= rect.right() + 0.5, "{what}: {t:?}");
                    assert!(t.left() >= rect.left() - 0.5, "{what}: {t:?}");
                }
                for row in rows(&tiles) {
                    for pair in row.windows(2) {
                        assert!(pair[1].left() >= pair[0].right(), "{what}: Überlappung");
                    }
                }
                for s in &seen {
                    let visible = s.rect.intersect(s.clip);
                    if visible.is_positive() {
                        assert!(
                            visible.right() <= rect.right() + 0.5,
                            "{what}: „{}“ ragt rechts hinaus ({:?})",
                            s.text,
                            s.rect
                        );
                    }
                }
                // Rollen im Kachelbereich bewegt das Fenster nicht
                if n == 200 {
                    let grid = x.w.grid_rect().expect("Kachelbereich");
                    let before = x.rect();
                    let H { screen, w, ws } = &mut x;
                    screen.wheel(grid.center(), -600.0, &mut |ctx| {
                        w.show(ctx, ws);
                    });
                    assert_eq!(x.rect(), before, "{what}: Fenster bleibt");
                    let tiles = x.w.tile_rects();
                    assert!(tiles[0].top() < grid.top(), "{what}: gerollt");
                }
            }
        }
    }
}

#[test]
fn hover_and_arriving_thumbnails_move_nothing() {
    let root = library(0);
    let lib = Library::open(&root).unwrap();
    for i in 0..6 {
        lib.create_project(&format!("P{i}")).unwrap();
        box_doc(&root, &format!("P{i}"), "Teil");
    }
    let gate = Arc::new(Mutex::new(()));
    let g = gate.clone();
    let mut x = H::new(workspace(&root, true), 1400.0, 900.0);
    x.w.set_worker(ThumbWorker::start_with(move |r: &ThumbRequest| {
        let _held = g.lock().unwrap_or_else(|e| e.into_inner());
        thumbnail_for(&r.dir, &r.key, &BuildLimits::default())
    }));
    let held = gate.lock().unwrap();
    x.ready();
    let before = x.rects();
    let first = before.1[0];
    x.frame(vec![Event::PointerMoved(first.center())]);
    x.settle(3);
    assert_eq!(x.rects(), before, "Überfahren");
    let at = first.center();
    for pressed in [true, false] {
        x.frame(vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }]);
    }
    x.settle(3);
    assert!(x.w.ui.selected().is_some());
    assert_eq!(x.rects(), before, "Wählen");
    drop(held);
    assert!(x.w.wait_for_thumbnails(Duration::from_secs(60)));
    x.settle(4);
    assert_eq!(x.rects(), before, "Miniaturen eingetroffen");
}

#[test]
fn long_names_stay_inside_their_tile() {
    let name: String = "Gehäuse für den langen Namen mit sehr vielen Zeichen "
        .chars()
        .cycle()
        .take(80)
        .collect::<String>()
        .trim_end()
        .to_string();
    let root = library(0);
    Library::open(&root).unwrap().create_project(&name).unwrap();
    for (w, h) in SIZES {
        let mut x = H::new(workspace(&root, true), w, h);
        let H { screen, w: win, ws } = &mut x;
        let mut shapes = Vec::new();
        for _ in 0..4 {
            shapes = screen
                .frame_shapes(vec![], &mut |ctx| {
                    win.show(ctx, ws);
                })
                .1;
        }
        win.wait_for_scan(ws);
        for _ in 0..4 {
            shapes = screen
                .frame_shapes(vec![], &mut |ctx| {
                    win.show(ctx, ws);
                })
                .1;
        }
        let tile = x.w.tile_rects()[0];
        let mut found = false;
        for c in &shapes {
            let mut stack = vec![&c.shape];
            while let Some(s) = stack.pop() {
                match s {
                    egui::Shape::Vec(v) => stack.extend(v.iter()),
                    egui::Shape::Text(t) if t.galley.text() == name => {
                        found = true;
                        let r = Rect::from_min_size(t.pos, t.galley.size());
                        assert!(r.right() < tile.right(), "{w}×{h}: {r:?} in {tile:?}");
                        assert!(r.left() > tile.left());
                        assert!(t.galley.elided, "gekürzt");
                        let last = t.galley.rows[0].row.glyphs.last().map(|g| g.chr);
                        assert_eq!(last, Some('…'), "endet mit „…“");
                    }
                    _ => {}
                }
            }
        }
        assert!(found, "{w}×{h}: Name gezeichnet");
    }
}

#[test]
fn the_window_does_not_jump_when_tiles_arrive() {
    let root = library(0);
    let lib = Library::open(&root).unwrap();
    for i in 0..12 {
        lib.create_project(&format!("P{i:02}")).unwrap();
        box_doc(&root, &format!("P{i:02}"), "Teil");
    }
    for (w, h) in SIZES {
        let mut x = H::new(workspace(&root, true), w, h);
        x.w.set_scan_delay(Duration::from_millis(30));
        x.settle(3);
        assert!(x.w.scanning(), "liest noch");
        assert!(x.w.tile_rects().is_empty());
        let before = x.rect();
        x.w.wait_for_scan(&x.ws);
        x.settle(4);
        assert_eq!(x.w.tile_rects().len(), 12);
        assert_eq!(
            x.rect(),
            before,
            "{w}×{h}: Fenster gleich groß und an derselben Stelle"
        );
    }
}

#[test]
fn more_than_500_projects_show_the_hint() {
    let root = library(MAX_TILES + 1);
    let mut x = H::new(workspace(&root, true), 1400.0, 900.0);
    let seen = x.ready();
    let hint = find(&seen, TOO_MANY).expect("Hinweis");
    assert!(
        hint.clip.contains_rect(hint.rect),
        "sichtbar: {:?}",
        hint.rect
    );
    assert_eq!(x.w.tile_rects().len(), MAX_TILES);
    let footer = format!("{MAX_TILES} von {} Projekten", MAX_TILES + 1);
    assert!(find(&seen, &footer).is_some(), "Fußzeile „{footer}“");
    let root = library(MAX_TILES);
    let mut x = H::new(workspace(&root, true), 1400.0, 900.0);
    let seen = x.ready();
    assert!(find(&seen, TOO_MANY).is_none());
    assert!(find(&seen, &format!("{MAX_TILES} Projekte")).is_some());
}

#[test]
fn a_deep_path_never_covers_the_buttons() {
    let project_name = "Gehäuse für die Kundenanlage Nord-West 1"; // 40 Zeichen
    assert_eq!(project_name.chars().count(), 40);
    let root = library(0);
    let lib = Library::open(&root).unwrap();
    let mut at = lib.create_project(project_name).unwrap();
    for i in 0..5 {
        let name = format!("Unterordner Nummer {i}"); // 20 Zeichen
        assert_eq!(name.chars().count(), 20);
        at = lib.create_folder(&at, &name).unwrap();
    }
    let last = at.last().unwrap().clone();
    for (w, h) in SIZES {
        let what = format!("{w}×{h}");
        let mut x = H::new(workspace(&root, true), w, h);
        x.ready();
        let level_one = x.rect();
        x.w.go_to(at.clone());
        let seen = x.settle(4);
        let rect = x.rect();
        assert_eq!(
            rect, level_one,
            "{what}: gleiche Fenstergröße auf jeder Ebene"
        );
        let grid = x.w.grid_rect().unwrap();
        // alle Texte über dem Kachelbereich: im Fenster, nicht übereinander
        let header: Vec<&Seen> = seen
            .iter()
            .filter(|s| s.rect.bottom() <= grid.top() && s.rect.top() > rect.top())
            .collect();
        for s in &header {
            let visible = s.rect.intersect(s.clip);
            assert!(
                rect.contains_rect(visible),
                "{what}: „{}“ {:?} ragt aus {rect:?}",
                s.text,
                s.rect
            );
        }
        for (i, a) in header.iter().enumerate() {
            for b in &header[i + 1..] {
                let overlap = a.rect.intersect(a.clip).intersect(b.rect.intersect(b.clip));
                assert!(
                    !(overlap.width() > 0.5 && overlap.height() > 0.5),
                    "{what}: „{}“ {:?} überdeckt „{}“ {:?}",
                    a.text,
                    a.rect,
                    b.text,
                    b.rect
                );
            }
        }
        for button in [NEW_DOCUMENT, NEW_FOLDER] {
            let b = find(&seen, button).unwrap_or_else(|| panic!("{what}: {button}"));
            assert!(
                b.clip.contains_rect(b.rect),
                "{what}: {button} abgeschnitten"
            );
        }
        assert!(
            find(&seen, &last).is_some(),
            "{what}: der letzte Teil „{last}“ steht immer da"
        );
        if w < 2000.0 {
            assert!(
                find(&seen, CRUMB_ELLIPSIS).is_some(),
                "{what}: vordere Teile als „…“"
            );
        }
    }
}

#[test]
fn the_primary_button_keeps_its_size() {
    for dark in [false, true] {
        let mut screen = Screen::new(400.0, 200.0);
        theme::apply(&screen.ctx, dark);
        let rect = std::cell::Cell::new(None);
        let mut draw = |ctx: &egui::Context| {
            egui::Area::new(egui::Id::new("knopf"))
                .fixed_pos(pos2(20.0, 20.0))
                .show(ctx, |ui| {
                    rect.set(Some(
                        ui.add(theme::primary_button("Neues Projekt", dark)).rect,
                    ));
                });
        };
        screen.settle(2, &mut draw);
        let rest = rect.get().unwrap();
        screen.frame(vec![Event::PointerMoved(rest.center())], &mut draw);
        screen.settle(2, &mut draw);
        let hovered = rect.get().unwrap();
        screen.frame(
            vec![Event::PointerButton {
                pos: rest.center(),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
            &mut draw,
        );
        screen.settle(1, &mut draw);
        let pressed = rect.get().unwrap();
        assert_eq!(rest, hovered, "Überfahren");
        assert_eq!(rest, pressed, "Drücken");
        let p = theme::palette(dark);
        assert_ne!(p.accent, p.bg_panel);
    }
}
