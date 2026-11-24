//! Gemerktes Programmfenster: verfolgen während des Laufs (normal, maximiert, minimiert, ohne
//! Angaben), auswählen beim Start (gemerkt vor Update, Monitorprüfung in Pixeln), platzieren
//! in den ersten Bildern (Skalierung des Monitors, auf dem das Fenster steht) und speichern.

mod common;

use egui::{Rect, ViewportCommand, ViewportInfo, pos2, vec2};
use planora_app::main_window::{PLACE_TRIES, Place, at_start, place_step, save, track};
use planora_app::running_version;
use planora_project::config::{Config, MainWindow, PendingUpdate, WindowState};

fn info(min: (f32, f32), size: (f32, f32), maximized: bool, minimized: bool) -> ViewportInfo {
    let inner = Rect::from_min_size(pos2(min.0 + 8.0, min.1 + 31.0), vec2(size.0, size.1));
    let outer = Rect::from_min_max(pos2(min.0, min.1), inner.max + vec2(8.0, 8.0));
    ViewportInfo {
        inner_rect: Some(inner),
        outer_rect: Some(outer),
        maximized: Some(maximized),
        minimized: Some(minimized),
        native_pixels_per_point: Some(1.25),
        ..Default::default()
    }
}

fn normal(x: f32, y: f32, w: f32, h: f32) -> MainWindow {
    MainWindow {
        window: WindowState {
            x: Some(x),
            y: Some(y),
            width: w,
            height: h,
            maximized: false,
        },
        scale: Some(1.25),
    }
}

fn monitors(rects: &[(f32, f32, f32, f32)]) {
    planora_app::detach::set_test_monitors(Some(
        rects
            .iter()
            .map(|&(x, y, w, h)| Rect::from_min_size(pos2(x, y), vec2(w, h)))
            .collect(),
    ));
}

#[test]
fn tracking_keeps_the_normal_bounds_through_maximize_and_minimize() {
    let w = track(None, &info((100.0, 50.0), (1400.0, 900.0), false, false));
    assert_eq!(w, Some(normal(100.0, 50.0, 1400.0, 900.0)));
    // Maximiert: Größe und Lage im normalen Zustand bleiben, nur „maximiert“ kommt dazu.
    let max = track(w, &info((-8.0, -8.0), (1920.0, 1009.0), true, false)).unwrap();
    assert!(max.window.maximized);
    assert_eq!((max.window.x, max.window.width), (Some(100.0), 1400.0));
    // Minimiert (Windows meldet -32000): nichts ändert sich.
    assert_eq!(
        track(
            Some(max),
            &info((-32000.0, -32000.0), (160.0, 28.0), false, true)
        ),
        Some(max)
    );
    // Wieder normal an neuer Stelle.
    let back = track(
        Some(max),
        &info((300.0, 200.0), (1200.0, 800.0), false, false),
    );
    assert_eq!(back, Some(normal(300.0, 200.0, 1200.0, 800.0)));
    // Gleich maximiert gestartet, ohne normalen Stand: der maximierte als Ersatz.
    let first = track(None, &info((-8.0, -8.0), (1920.0, 1009.0), true, false)).unwrap();
    assert!(first.window.maximized);
}

#[test]
fn a_window_left_of_the_main_monitor_keeps_its_position() {
    // Ganz auf einem Monitor links vom Hauptmonitor (negative Lage).
    let w = track(None, &info((-1700.0, 100.0), (1400.0, 900.0), false, false)).unwrap();
    assert_eq!(w.window.x, Some(-1700.0));
    let c = Config {
        main_window: Some(w),
        ..Config::default()
    };
    monitors(&[(-2400.0, 0.0, 2400.0, 1350.0), (0.0, 0.0, 2400.0, 1350.0)]);
    let (_, place) = at_start(&c, &running_version()).unwrap();
    assert_eq!(place.px, Some(pos2(-1700.0 * 1.25, 100.0 * 1.25)));
    planora_app::detach::set_test_monitors(None);
}

#[test]
fn without_positions_the_last_state_stays() {
    // Unter Wayland meldet das System keine Lage: nichts überschreibt den gemerkten Stand.
    let prev = Some(normal(100.0, 50.0, 1400.0, 900.0));
    let mut i = info((0.0, 0.0), (800.0, 600.0), false, false);
    i.outer_rect = None;
    i.inner_rect = None;
    assert_eq!(track(prev, &i), prev);
}

#[test]
fn start_prefers_the_saved_window_then_the_update_window() {
    let running = running_version();
    let mut c = Config::default();
    assert_eq!(at_start(&c, &running), None);
    let upd = WindowState {
        x: Some(10.0),
        y: Some(10.0),
        width: 1000.0,
        height: 700.0,
        maximized: true,
    };
    c.update_pending = Some(PendingUpdate {
        from: "0.0.1".into(),
        to: running.to_string(),
        window: Some(upd),
    });
    monitors(&[(0.0, 0.0, 2400.0, 1350.0)]);
    // Nur das Fenster aus dem Update (ältere Versionen merken kein `main_window`).
    let (size, place) = at_start(&c, &running).unwrap();
    assert_eq!((size.width, size.x, size.maximized), (1000.0, None, false));
    assert_eq!(
        place,
        Place {
            px: Some(pos2(10.0, 10.0)),
            maximize: true
        }
    );
    // Gemerktes Fenster geht vor.
    c.main_window = Some(normal(100.0, 50.0, 1400.0, 900.0));
    let (size, place) = at_start(&c, &running).unwrap();
    assert_eq!((size.width, size.height), (1400.0, 900.0));
    assert_eq!(
        place,
        Place {
            px: Some(pos2(125.0, 62.5)),
            maximize: false
        }
    );
    planora_app::detach::set_test_monitors(None);
}

#[test]
fn a_position_off_every_monitor_is_dropped_but_the_size_stays() {
    let running = running_version();
    // Lag auf einem zweiten Monitor rechts (Punkte 2500 bei Skalierung 1,25 → 3125 Pixel).
    let c = Config {
        main_window: Some(normal(2500.0, 100.0, 1400.0, 900.0)),
        ..Config::default()
    };
    monitors(&[(0.0, 0.0, 2400.0, 1350.0), (2400.0, 0.0, 1920.0, 1080.0)]);
    assert_eq!(
        at_start(&c, &running).unwrap().1.px,
        Some(pos2(3125.0, 125.0)),
        "zweiter Monitor da"
    );
    // Zweiter Monitor abgezogen: Lage weg, Größe bleibt.
    monitors(&[(0.0, 0.0, 2400.0, 1350.0)]);
    let (size, place) = at_start(&c, &running).unwrap();
    assert_eq!(place.px, None);
    assert_eq!((size.width, size.height), (1400.0, 900.0));
    planora_app::detach::set_test_monitors(None);
}

#[test]
fn placing_uses_the_scale_of_the_monitor_the_window_is_on() {
    // Gemerkt: 3125 Pixel (zweiter Monitor bei 100 %). Das Fenster entsteht auf dem
    // Hauptmonitor bei 150 %: erst dorthin schieben, mit dessen Skalierung umgerechnet.
    let place = Place {
        px: Some(pos2(3125.0, 125.0)),
        maximize: true,
    };
    let mut i = info((100.0, 100.0), (1400.0, 900.0), false, false);
    i.native_pixels_per_point = Some(1.5);
    let (cmds, done) = place_step(place, &i, 0);
    assert!(!done);
    assert_eq!(
        cmds,
        vec![ViewportCommand::OuterPosition(pos2(
            3125.0 / 1.5,
            125.0 / 1.5
        ))]
    );
    // Auf dem zweiten Monitor (100 %) angekommen: jetzt maximieren, fertig.
    let mut there = info((3125.0, 125.0), (1400.0, 900.0), false, false);
    there.native_pixels_per_point = Some(1.0);
    let (cmds, done) = place_step(place, &there, 1);
    assert!(done);
    assert_eq!(cmds, vec![ViewportCommand::Maximized(true)]);
    // Kommt es nie an, nach PLACE_TRIES Versuchen trotzdem fertig.
    let (_, done) = place_step(place, &i, PLACE_TRIES);
    assert!(done);
    // Ohne Lage und ohne Maximieren: nichts zu tun.
    let (cmds, done) = place_step(
        Place {
            px: None,
            maximize: false,
        },
        &i,
        0,
    );
    assert!(done && cmds.is_empty());
}

#[test]
fn closing_saves_the_window_without_touching_other_settings() {
    let dir = common::temp_root("planora-hauptfenster");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    let c = Config {
        verbose_log: true,
        ..Config::default()
    };
    c.save_to(&path).unwrap();
    let w = normal(100.0, 50.0, 1400.0, 900.0);
    assert!(save(&path, w));
    let back = Config::read_from(&path).unwrap();
    assert_eq!(back.main_window, Some(w));
    assert!(back.verbose_log);
}
