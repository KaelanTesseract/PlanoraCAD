//! Skizzenraster und Fang (Skizzen 2, Task 6): Raster-Fang beim Zeichnen, Raster in der
//! Skizzenebene, Schalter im Skizzenfeld, gemerkt in `config.json`. Ohne Fenster bei 1920 × 1080;
//! Konfiguration nur in eigenen Temp-Ordnern.

mod common;

use egui::epaint::Shape;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, pos2};
use planora_app::keymap::Keymap;
use planora_app::sketch_state::{Snap, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_panel, sketch_view};
use planora_feature::SketchPlane;
use planora_i18n::{DisplayUnit, with_unit};
use planora_math::{DVec2, Frame};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_render::visible_step;
use std::fs;
use std::path::PathBuf;

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(1920.0, 1080.0),
};
const VIEW: glam::Vec2 = glam::Vec2::new(1920.0, 1080.0);

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn raw(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    }
}

/// Offene, leere Skizze auf `plane` im Abstand 150 (Raster 10 mm), Werkzeug `tool`.
fn sketching(plane: StandardPlane, tool: Tool) -> AppState {
    let mut s = AppState::new();
    let id = s.add_sketch(SketchPlane::Standard(plane)).unwrap();
    s.open_sketch(id).unwrap();
    s.camera.distance = 150.0;
    s.camera.target = glam::Vec3::ZERO;
    s.camera.set_viewport(VIEW.x, VIEW.y);
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, tool);
    s
}

fn frame_of(s: &AppState) -> Frame {
    *s.session_sketch().unwrap().frame()
}

fn screen_at(s: &AppState, p: DVec2) -> Pos2 {
    let v = sketch_view::sketch_to_screen(&s.camera, &frame_of(s), VIEW, p).unwrap();
    pos2(v.x, v.y)
}

fn play(ctx: &egui::Context, s: &mut AppState, frames: Vec<Vec<Event>>) {
    for events in std::iter::once(vec![]).chain(frames) {
        ctx.run_ui(raw(events), |ui| {
            sketch_view::show_overlay(ui, SCREEN, s);
        })
        .drop_without_applying_deltas();
    }
}

fn click(ctx: &egui::Context, s: &mut AppState, at: DVec2, modifiers: Modifiers) {
    let p = screen_at(s, at);
    let button = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    };
    play(
        ctx,
        s,
        vec![
            vec![Event::PointerMoved(p)],
            vec![button(true)],
            vec![button(false)],
            vec![],
        ],
    );
}

fn newest_point(s: &AppState) -> DVec2 {
    let sk = s.session_sketch().unwrap();
    let p = sk.points().map(|(p, _)| p).last().unwrap();
    sk.point_xy(p).unwrap()
}

fn is_multiple(v: f64, step: f64) -> bool {
    let q = v / step;
    (q - q.round()).abs() <= 1e-12
}

// ---- Raster-Fang ------------------------------------------------------------------------------

#[test]
fn a_free_click_lands_on_the_grid() {
    let ctx = context();
    let mut s = sketching(StandardPlane::XY, Tool::Point);
    play(&ctx, &mut s, vec![]);
    click(&ctx, &mut s, DVec2::new(23.4, 16.7), Modifiers::NONE);
    assert_eq!(newest_point(&s), DVec2::new(20.0, 20.0));
}

#[test]
fn the_cursor_shows_the_grid_snap_before_the_click() {
    let ctx = context();
    let mut s = sketching(StandardPlane::XY, Tool::Line);
    play(&ctx, &mut s, vec![]);
    let p = screen_at(&s, DVec2::new(23.4, 16.7));
    play(&ctx, &mut s, vec![vec![Event::PointerMoved(p)]]);
    let cur = s
        .sketch_session
        .as_ref()
        .unwrap()
        .draw_cursor
        .clone()
        .unwrap();
    assert_eq!(cur.snap, Snap::Grid);
    assert_eq!(cur.at, DVec2::new(20.0, 20.0));
}

#[test]
fn geometry_snap_goes_first() {
    let ctx = context();
    let mut s = sketching(StandardPlane::XY, Tool::Point);
    let at = DVec2::new(23.4, 16.7);
    let (_, sk) = s.session_sketch_mut().unwrap();
    sk.add_point(at.x, at.y).unwrap();
    let before = sk.points().count();
    play(&ctx, &mut s, vec![]);
    let p = screen_at(&s, at);
    play(&ctx, &mut s, vec![vec![Event::PointerMoved(p)]]);
    let se = s.sketch_session.as_ref().unwrap();
    assert!(
        matches!(se.draw_cursor.as_ref().unwrap().snap, Snap::Point(_)),
        "{:?}",
        se.draw_cursor
    );
    assert_eq!(se.draw_cursor.as_ref().unwrap().at, at);
    click(&ctx, &mut s, at, Modifiers::NONE);
    assert_eq!(s.session_sketch().unwrap().points().count(), before);
}

#[test]
fn alignment_and_grid_mix_per_axis() {
    let ctx = context();
    let mut s = sketching(StandardPlane::XY, Tool::Point);
    let (_, sk) = s.session_sketch_mut().unwrap();
    sk.add_point(33.3, 47.0).unwrap();
    play(&ctx, &mut s, vec![]);
    click(&ctx, &mut s, DVec2::new(33.4, 16.7), Modifiers::NONE);
    assert_eq!(newest_point(&s), DVec2::new(33.3, 20.0));
}

#[test]
fn ctrl_switches_the_grid_snap_off() {
    let ctx = context();
    let mut s = sketching(StandardPlane::XY, Tool::Point);
    play(&ctx, &mut s, vec![]);
    let at = DVec2::new(23.4, 16.7);
    click(&ctx, &mut s, at, Modifiers::CTRL);
    let got = newest_point(&s);
    assert!(got.distance(at) < 0.2, "{got}");
}

#[test]
fn switching_snap_or_grid_off_means_no_grid_snap() {
    for (grid, snap) in [(true, false), (false, true), (false, false)] {
        let ctx = context(); // sonst zählt der nächste Klick an derselben Stelle als Doppelklick
        let mut s = sketching(StandardPlane::XY, Tool::Point);
        s.sketch_show_grid = grid;
        s.sketch_snap_grid = snap;
        play(&ctx, &mut s, vec![]);
        let at = DVec2::new(23.4, 16.7);
        click(&ctx, &mut s, at, Modifiers::NONE);
        let got = newest_point(&s);
        assert!(got.distance(at) < 0.2, "{grid} {snap}: {got}");
    }
}

/// Review-Focus 5: In Zoll liegt der gefangene Punkt ohne f32-Fehler auf dem Vielfachen.
#[test]
fn inch_snap_is_an_exact_multiple() {
    with_unit(DisplayUnit::Zoll, || {
        for (distance, step) in [(40.0_f32, 2.54), (150.0, 25.4), (150.0 * 25.4, 254.0)] {
            let ctx = context();
            let mut s = sketching(StandardPlane::XY, Tool::Point);
            s.camera.distance = distance;
            let got = visible_step(distance, 25.4);
            assert!((got - step).abs() < 1e-9, "{distance}: {got}");
            play(&ctx, &mut s, vec![]);
            let at = DVec2::new(3.3 * step, 2.1 * step);
            click(&ctx, &mut s, at, Modifiers::NONE);
            let p = newest_point(&s);
            assert!(is_multiple(p.x, got), "{distance}: {} / {got}", p.x);
            assert!(is_multiple(p.y, got), "{distance}: {} / {got}", p.y);
            assert!((p.x - 3.0 * step).abs() < 1e-9, "{distance}: {p}");
            assert!((p.y - 2.0 * step).abs() < 1e-9, "{distance}: {p}");
        }
    });
}

// ---- Raster in der Skizzenebene ---------------------------------------------------------------

/// Die Linien des Rasters als Punktfolgen auf dem Bildschirm.
fn grid_paths(s: &AppState) -> Vec<Vec<Pos2>> {
    sketch_view::sketch_grid_shapes(s, SCREEN)
        .into_iter()
        .map(|shape| match shape {
            Shape::Path(p) => p.points,
            other => panic!("{other:?}"),
        })
        .collect()
}

/// Jede Rasterlinie liegt in der Skizzenebene auf einem Vielfachen der Stufe `step`, auch von ihren
/// Punkten auf dem Bildschirm zurückgerechnet. Ein Raster auf einer anderen Ebene träfe das nicht.
fn assert_lines_in_plane(s: &AppState, step: f64) {
    let frame = frame_of(s);
    let paths = grid_paths(s);
    let (mut along_x, mut along_y) = (0, 0);
    for path in &paths {
        assert!(path.len() >= 2);
        let local: Vec<DVec2> = path
            .iter()
            .map(|p| {
                sketch_view::screen_to_sketch(&s.camera, &frame, VIEW, glam::Vec2::new(p.x, p.y))
                    .unwrap()
            })
            .collect();
        let spread = |f: fn(&DVec2) -> f64| {
            let (lo, hi) = local
                .iter()
                .map(f)
                .fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v), h.max(v)));
            hi - lo
        };
        // Der Bildschirm rundet auf f32: bis 5 % der Stufe gelten als getroffen.
        let on = |v: f64| ((v / step) - (v / step).round()).abs() < 0.05;
        if spread(|p| p.y) < spread(|p| p.x) {
            along_x += 1;
            for p in &local {
                assert!(on(p.y), "{p}");
            }
        } else {
            along_y += 1;
            for p in &local {
                assert!(on(p.x), "{p}");
            }
        }
    }
    assert!(along_x >= 2 && along_y >= 2, "{along_x} {along_y}");
}

#[test]
fn the_grid_lies_in_the_sketch_plane_also_on_xz() {
    for plane in [StandardPlane::XY, StandardPlane::XZ, StandardPlane::YZ] {
        let s = sketching(plane, Tool::Select);
        assert_lines_in_plane(&s, 10.0);
    }
}

#[test]
fn the_grid_follows_a_tilted_view_in_the_plane() {
    let mut s = sketching(StandardPlane::XZ, Tool::Select);
    s.camera.orbit(60.0, 40.0);
    assert_lines_in_plane(&s, 10.0);
}

#[test]
fn the_line_count_stays_bounded_over_the_zoom_range() {
    for plane in [StandardPlane::XY, StandardPlane::XZ] {
        for tilt in [0.0, 80.0] {
            let mut s = sketching(plane, Tool::Select);
            s.camera.orbit(tilt, tilt);
            let mut d = 0.5_f32;
            let mut most = 0;
            while d < 1.0e6 {
                s.camera.distance = d;
                let n = sketch_view::sketch_grid_shapes(&s, SCREEN).len();
                assert!(n <= 200, "{plane:?}, Neigung {tilt}, Abstand {d}: {n}");
                most = most.max(n);
                d *= 1.01;
            }
            assert!(most > 30, "{most}");
        }
    }
}

#[test]
fn the_grid_is_gone_when_switched_off_and_drawn_by_the_overlay_when_on() {
    let ctx = context();
    let paths = |s: &mut AppState| {
        let mut n = 0;
        for _ in 0..2 {
            let out = ctx.run_ui(raw(vec![]), |ui| {
                sketch_view::show_overlay(ui, SCREEN, s);
            });
            n = out
                .shapes
                .iter()
                .filter(|c| matches!(c.shape, Shape::Path(_)))
                .count();
            out.drop_without_applying_deltas();
        }
        n
    };
    let mut s = sketching(StandardPlane::XZ, Tool::Select);
    let on = paths(&mut s);
    let lines = sketch_view::sketch_grid_shapes(&s, SCREEN).len();
    assert!(lines > 10, "{lines}");
    s.sketch_show_grid = false;
    assert!(sketch_view::sketch_grid_shapes(&s, SCREEN).is_empty());
    assert_eq!(paths(&mut s), on - lines);
    // „Fang“ allein schaltet das Raster nicht ab.
    s.sketch_show_grid = true;
    s.sketch_snap_grid = false;
    assert_eq!(sketch_view::sketch_grid_shapes(&s, SCREEN).len(), lines);
}

// ---- Schalter und Konfiguration ---------------------------------------------------------------

struct Env {
    ws: Workspace,
    tmp: PathBuf,
    cfg: PathBuf,
}

fn env(prefix: &str) -> Env {
    let tmp = common::temp_root(prefix);
    fs::create_dir_all(&tmp).unwrap();
    let cfg = tmp.join("config.json");
    let library = Library::open(&tmp.join("Lib")).unwrap();
    let mut ws = Workspace::new(Some(library), Config::default());
    ws.config_path = cfg.clone();
    Env { ws, tmp, cfg }
}

fn find_text(shapes: &[egui::epaint::ClippedShape], text: &str) -> Option<Pos2> {
    common::collect_seen(shapes)
        .into_iter()
        .find(|s| s.text == text)
        .map(|s| s.rect.center())
}

/// Ein Klick auf den Schalter `label` im Skizzenfeld.
fn click_switch(ctx: &egui::Context, s: &mut AppState, label: &str) {
    let mut shapes = Vec::new();
    for _ in 0..3 {
        let out = ctx.run_ui(raw(vec![]), |ui| sketch_panel::show(ui.ctx(), s));
        shapes = out.shapes.clone();
        out.drop_without_applying_deltas();
    }
    let at = find_text(&shapes, label).unwrap_or_else(|| panic!("„{label}“ fehlt"));
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for events in [
        vec![Event::PointerMoved(at)],
        vec![button(true)],
        vec![button(false)],
    ] {
        ctx.run_ui(raw(events), |ui| sketch_panel::show(ui.ctx(), s))
            .drop_without_applying_deltas();
    }
}

fn open_sketch(ws: &mut Workspace) {
    let s = &mut ws.active_tab_mut().state;
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
}

#[test]
fn the_switches_write_the_config_and_every_tab_follows() {
    let mut e = env("planora-sketch-grid-schalter");
    let ctx = context();
    e.ws.new_tab(); // zweiter Tab, schon vor dem Umschalten da
    e.ws.activate(0);
    open_sketch(&mut e.ws);

    click_switch(&ctx, &mut e.ws.active_tab_mut().state, "Skizzenraster");
    {
        let s = &e.ws.active_tab().state;
        assert!(!s.sketch_show_grid);
        assert!(s.sketch_grid_changed);
    }
    e.ws.sync_sketch_grid();
    assert!(!e.ws.active_tab().state.sketch_grid_changed);
    let text = fs::read_to_string(&e.cfg).unwrap();
    assert!(text.contains("\"sketch_grid\": false"), "{text}");
    assert!(!text.contains("sketch_snap"), "{text}");

    click_switch(&ctx, &mut e.ws.active_tab_mut().state, "Fang");
    e.ws.sync_sketch_grid();
    let disk = Config::read_from(&e.cfg).unwrap();
    assert!(!disk.sketch_grid);
    assert!(!disk.sketch_snap);

    // Der andere Tab ist schon umgestellt, auch ohne dass er aktiv war.
    e.ws.activate(1);
    assert!(!e.ws.active_tab().state.sketch_show_grid);
    assert!(!e.ws.active_tab().state.sketch_snap_grid);

    // Ein neuer Tab übernimmt die Konfiguration, sobald er aktiv ist.
    e.ws.new_tab();
    e.ws.sync_sketch_grid();
    assert!(!e.ws.active_tab().state.sketch_show_grid);
    assert!(!e.ws.active_tab().state.sketch_snap_grid);

    // Ein neuer Arbeitsbereich liest sie aus der Datei.
    let mut next = Workspace::new(None, Config::load_from(&e.cfg));
    next.config_path = e.cfg.clone();
    next.sync_sketch_grid();
    assert!(!next.active_tab().state.sketch_show_grid);
    assert!(!next.active_tab().state.sketch_snap_grid);
}

#[test]
fn a_sync_without_a_change_writes_nothing() {
    let mut e = env("planora-sketch-grid-still");
    e.ws.sync_sketch_grid();
    assert!(!e.cfg.exists(), "kein Schreiben ohne Änderung");
    assert!(e.ws.active_tab().state.sketch_show_grid);
    assert!(e.ws.active_tab().state.sketch_snap_grid);
}

#[test]
fn switching_back_on_leaves_the_file_without_the_fields() {
    let mut e = env("planora-sketch-grid-zurueck");
    assert!(e.ws.set_sketch_grid(false, false));
    assert!(e.ws.set_sketch_grid(true, true));
    let text = fs::read_to_string(&e.cfg).unwrap();
    assert!(!text.contains("sketch_grid"), "{text}");
    assert!(!text.contains("sketch_snap"), "{text}");
}

#[test]
fn a_failed_write_rides_along_on_every_write_path() {
    for path in [
        "verzeichnis",
        "durchsicht",
        "tasten",
        "favoriten",
        "update",
        "design",
        "sprache",
        "einheit",
    ] {
        let mut e = env(&format!("planora-sketch-grid-mit-{path}"));
        let blocker = e.tmp.join("blockiert");
        fs::write(&blocker, b"x").unwrap();
        e.ws.config_path = blocker.join("config.json");
        e.ws.active_tab_mut().state.sketch_show_grid = false;
        e.ws.active_tab_mut().state.sketch_grid_changed = true;
        e.ws.sync_sketch_grid();
        assert!(e.ws.sketch_grid_pending, "{path}");
        assert!(
            !e.ws.active_tab().state.sketch_show_grid,
            "gilt für die Sitzung"
        );
        // Der Stand im Speicher hält den Schalter auch beim nächsten Bild.
        e.ws.sync_sketch_grid();
        assert!(!e.ws.active_tab().state.sketch_show_grid, "{path}");
        assert_eq!(fs::read(&blocker).unwrap(), b"x");
        e.ws.config_path = e.cfg.clone();
        let written = match path {
            "verzeichnis" => e.ws.try_persist_config(),
            "durchsicht" => e.ws.set_panel_transparency(20),
            "tasten" => e.ws.set_keymap(Keymap::default()),
            "update" => e.ws.set_update_check(false),
            "design" => e.ws.set_theme(false),
            "sprache" => e.ws.set_language(&planora_i18n::german()),
            "einheit" => e.ws.set_length_unit(DisplayUnit::Millimeter),
            _ => {
                let mut places = e.ws.places();
                places.add_favorite(&e.tmp.join("Teile")).unwrap();
                e.ws.save_places(places)
            }
        };
        assert!(written, "{path}");
        assert!(!e.ws.sketch_grid_pending, "{path}");
        assert!(!Config::read_from(&e.cfg).unwrap().sketch_grid, "{path}");
        // Danach nicht mehr vorgemerkt: eine Änderung der Datei von außen bleibt.
        Config::update_at(&e.cfg, |c| c.sketch_grid = true).unwrap();
        assert!(e.ws.set_panel_transparency(30), "{path}");
        assert!(Config::read_from(&e.cfg).unwrap().sketch_grid, "{path}");
    }
}

#[test]
fn writing_the_switches_carries_the_other_pending_settings() {
    let mut e = env("planora-sketch-grid-andere");
    let blocker = e.tmp.join("blockiert");
    fs::write(&blocker, b"x").unwrap();
    e.ws.config_path = blocker.join("config.json");
    assert!(!e.ws.set_theme(false));
    assert!(!e.ws.set_length_unit(DisplayUnit::Millimeter));
    assert!(!e.ws.set_panel_transparency(25));
    e.ws.config_path = e.cfg.clone();
    assert!(e.ws.set_sketch_grid(true, false));
    let disk = Config::read_from(&e.cfg).unwrap();
    assert!(!disk.sketch_snap);
    assert_eq!(disk.theme.as_deref(), Some("hell"));
    assert_eq!(disk.length_unit.as_deref(), Some("mm"));
    assert_eq!(disk.panel_transparency, 25);
    assert!(!e.ws.theme_pending);
    assert!(!e.ws.length_unit_pending);
    assert!(!e.ws.panel_transparency_pending);
}
