//! Gemerktes Programmfenster: verfolgen während des Laufs (normal, maximiert, minimiert),
//! auswählen beim Start (Update zuerst, Monitorprüfung) und beim Schließen speichern.

mod common;

use egui::{Rect, ViewportInfo, pos2, vec2};
use planora_app::main_window::{at_start, save, track};
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
fn start_prefers_the_update_window_then_the_saved_one() {
    let running = running_version();
    let mut c = Config::default();
    assert_eq!(at_start(&c, &running), None);
    c.main_window = Some(normal(100.0, 50.0, 1400.0, 900.0));
    planora_app::detach::set_test_monitors(Some(vec![Rect::from_min_size(
        pos2(0.0, 0.0),
        vec2(2400.0, 1350.0),
    )]));
    assert_eq!(
        at_start(&c, &running),
        Some(normal(100.0, 50.0, 1400.0, 900.0).window)
    );
    let upd = WindowState {
        x: Some(10.0),
        y: Some(10.0),
        width: 1000.0,
        height: 700.0,
        maximized: false,
    };
    c.update_pending = Some(PendingUpdate {
        from: "0.0.1".into(),
        to: running.to_string(),
        window: Some(upd),
    });
    assert_eq!(at_start(&c, &running), Some(upd), "Update zuerst");
    planora_app::detach::set_test_monitors(None);
}

#[test]
fn a_position_off_every_monitor_is_dropped_but_the_size_stays() {
    let running = running_version();
    let mut c = Config::default();
    // Lag auf einem zweiten Monitor rechts (Punkte 2500 bei Skalierung 1,25 → 3125 Pixel).
    c.main_window = Some(normal(2500.0, 100.0, 1400.0, 900.0));
    planora_app::detach::set_test_monitors(Some(vec![
        Rect::from_min_size(pos2(0.0, 0.0), vec2(2400.0, 1350.0)),
        Rect::from_min_size(pos2(2400.0, 0.0), vec2(1920.0, 1080.0)),
    ]));
    assert_eq!(
        at_start(&c, &running).unwrap().x,
        Some(2500.0),
        "zweiter Monitor da"
    );
    // Zweiter Monitor abgezogen: Lage weg, Größe bleibt.
    planora_app::detach::set_test_monitors(Some(vec![Rect::from_min_size(
        pos2(0.0, 0.0),
        vec2(2400.0, 1350.0),
    )]));
    let w = at_start(&c, &running).unwrap();
    assert_eq!((w.x, w.y), (None, None));
    assert_eq!((w.width, w.height), (1400.0, 900.0));
    planora_app::detach::set_test_monitors(None);
}

#[test]
fn closing_saves_the_window_without_touching_other_settings() {
    let dir = common::temp_root("planora-hauptfenster");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    let mut c = Config::default();
    c.verbose_log = true;
    c.save_to(&path).unwrap();
    let w = normal(100.0, 50.0, 1400.0, 900.0);
    assert!(save(&path, w));
    let back = Config::read_from(&path).unwrap();
    assert_eq!(back.main_window, Some(w));
    assert!(back.verbose_log);
}
