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
use planora_math::{DVec2, DVec3, Frame};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_render::{exact_step, visible_step};
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

/// Blick unter `degrees` zur Normalen der Skizzenebene, Ziel im Ursprung.
fn tilt(s: &mut AppState, degrees: f32) {
    let frame = frame_of(s);
    let as_vec3 = |v: planora_math::DVec3| glam::Vec3::new(v.x as f32, v.y as f32, v.z as f32);
    let (sin, cos) = degrees.to_radians().sin_cos();
    s.camera.look_along(
        as_vec3(frame.z) * cos + as_vec3(frame.y) * sin,
        as_vec3(frame.y),
    );
}

/// Lage der Rasterlinien in der Skizze: `(läuft entlang x, Querkoordinate)`, gelesen am
/// Linienpunkt nächst der Bildmitte (dort rechnet der Bildschirm am genauesten zurück).
fn drawn_lines(s: &AppState) -> Vec<(bool, f64)> {
    let frame = frame_of(s);
    let back =
        |p: Pos2| sketch_view::screen_to_sketch(&s.camera, &frame, VIEW, glam::Vec2::new(p.x, p.y));
    grid_paths(s)
        .iter()
        .filter_map(|path| {
            let c = SCREEN.center();
            let near = path
                .iter()
                .min_by(|a, b| a.distance(c).total_cmp(&b.distance(c)))?;
            let at = back(*near)?;
            let other = path.iter().find_map(|p| back(*p).filter(|q| *q != at))?;
            let d = (other - at).abs();
            Some(if d.y < d.x {
                (true, at.y)
            } else {
                (false, at.x)
            })
        })
        .collect()
}

/// Spec-Ziel 5, „am sichtbaren Raster“: Jede Stelle, auf die der Zeiger rastet, trägt eine
/// gezeichnete Linie, auch wenn eine geneigte Ansicht das Raster ausdünnt. Unterhalb des Übergangs
/// (feine Stufe voll da) liegt umgekehrt jede Linie auf einem Vielfachen der Fangstufe; darüber
/// laufen die feinen Linien nach Entscheidung 13 noch schwach aus.
#[test]
fn the_snap_step_is_the_drawn_step_on_a_tilted_view() {
    for plane in [StandardPlane::XY, StandardPlane::XZ] {
        let ctx = context();
        let mut s = sketching(plane, Tool::Line);
        tilt(&mut s, 60.0);
        let mut thinned = 0;
        let mut d = 5.0_f32;
        while d < 5000.0 {
            s.camera.distance = d;
            play(&ctx, &mut s, vec![]);
            let step = s
                .sketch_session
                .as_ref()
                .unwrap()
                .cursor_opts
                .grid_step
                .unwrap();
            let (fine, blend) = exact_step(d, 1.0);
            if blend < 0.5 && step > fine * 1.5 {
                thinned += 1;
            }
            let lines = drawn_lines(&s);
            let on = |v: f64, m: f64| (v - m).abs() < 0.05 * step;
            let mut checked = 0;
            for along_x in [true, false] {
                for m in [-2.0, -1.0, 1.0, 2.0] {
                    // Nur, was im Bild liegt: Die Ansicht zeigt zum Betrachter hin weniger.
                    let at = if along_x {
                        DVec2::new(0.0, m * step)
                    } else {
                        DVec2::new(m * step, 0.0)
                    };
                    if !SCREEN.contains(screen_at(&s, at)) {
                        continue;
                    }
                    checked += 1;
                    assert!(
                        lines.iter().any(|&(a, v)| a == along_x && on(v, m * step)),
                        "{plane:?}, Abstand {d}, Stufe {step}: keine Linie bei {}",
                        m * step
                    );
                }
            }
            assert!(checked >= 2, "{plane:?}, Abstand {d}, Stufe {step}");
            if blend < 0.5 {
                for (_, v) in &lines {
                    assert!(on(*v, (*v / step).round() * step), "{d}, {step}: {v}");
                }
            }
            d *= 1.05;
        }
        assert!(thinned > 0, "{plane:?}: die Ansicht dünnt nie aus");
    }
}

/// Punkte (Welt) der Rasterlinie durch den Skizzenursprung, die entlang `along_x` läuft.
fn zero_line(s: &AppState, along_x: bool) -> Option<Vec<planora_math::DVec3>> {
    let frame = frame_of(s);
    grid_paths(s).iter().find_map(|path| {
        let local: Vec<DVec2> = path
            .iter()
            .filter_map(|p| {
                sketch_view::screen_to_sketch(&s.camera, &frame, VIEW, glam::Vec2::new(p.x, p.y))
            })
            .collect();
        let (a, b) = (local.first()?, local.last()?);
        let (across, spread) = if along_x {
            (a.y.abs().max(b.y.abs()), (a.x - b.x).abs())
        } else {
            (a.x.abs().max(b.x.abs()), (a.y - b.y).abs())
        };
        (across < 0.5 && spread > 1.0).then(|| local.iter().map(|p| frame.point_2d(*p)).collect())
    })
}

#[test]
fn the_zero_lines_are_left_out_only_where_a_world_axis_lies() {
    // XY: x- und y-Achse der Welt liegen beide ganz in den Nulllinien.
    let s = sketching(StandardPlane::XY, Tool::Select);
    assert!(zero_line(&s, true).is_none());
    assert!(zero_line(&s, false).is_none());

    // XZ und YZ: Die Achse in der Ebene liegt ganz da; die z-Achse läuft nur von 0 nach oben, also
    // bleibt die untere Hälfte der anderen Nulllinie.
    for plane in [StandardPlane::XZ, StandardPlane::YZ] {
        let s = sketching(plane, Tool::Select);
        let frame = frame_of(&s);
        let z_along_x = frame.x.z.abs() > 0.5;
        assert!(zero_line(&s, !z_along_x).is_none(), "{plane:?}");
        let line = zero_line(&s, z_along_x).unwrap_or_else(|| panic!("{plane:?}: fehlt"));
        let lowest = line.iter().map(|p| p.z).fold(f64::MAX, f64::min);
        let highest = line.iter().map(|p| p.z).fold(f64::MIN, f64::max);
        assert!(lowest < -10.0, "{plane:?}: {lowest}");
        assert!(highest < 0.5, "{plane:?}: oben liegt die Achse, {highest}");
    }
}

#[test]
fn a_general_plane_through_the_world_origin_keeps_both_zero_lines() {
    let mut s = sketching(StandardPlane::XY, Tool::Select);
    let tilted = Frame::from_origin_normal(DVec3::ZERO, DVec3::new(1.0, 1.0, 1.0));
    let (_, sk) = s.session_sketch_mut().unwrap();
    sk.set_frame(tilted).unwrap();
    tilt(&mut s, 0.0);
    assert_lines_in_plane(&s, 10.0);
    for along_x in [true, false] {
        let line = zero_line(&s, along_x).expect("Nulllinie");
        // Sie läuft beidseits des Ursprungs: Keine Hälfte ist weggelassen.
        let dir = if along_x { tilted.x } else { tilted.y };
        let along: Vec<f64> = line.iter().map(|p| p.dot(dir)).collect();
        assert!(along.iter().any(|t| *t < -10.0), "{along:?}");
        assert!(along.iter().any(|t| *t > 10.0), "{along:?}");
    }
}

#[test]
fn a_plane_off_the_world_origin_keeps_both_zero_lines() {
    let mut s = sketching(StandardPlane::XY, Tool::Select);
    let (_, sk) = s.session_sketch_mut().unwrap();
    sk.set_frame(Frame::from_origin_normal(
        DVec3::new(0.0, 0.0, 5.0),
        DVec3::Z,
    ))
    .unwrap();
    s.camera.target = glam::Vec3::new(0.0, 0.0, 5.0);
    assert!(zero_line(&s, true).is_some());
    assert!(zero_line(&s, false).is_some());
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
