//! Gemerktes Programmfenster (wie Fusion): Beim sauberen Schließen kommen Größe, Lage,
//! „maximiert“ und die Skalierung des Monitors in die Konfiguration (`main_window`); der nächste
//! Start öffnet wieder so. Bei maximiertem Fenster bleiben Größe und Lage des normalen Zustands
//! gemerkt, damit das Fenster beim Verkleinern seine alte Größe bekommt. Nach einem Update gilt
//! zuerst das Fenster aus dem Update ([`crate::update_install::window_for_start`]).

use std::path::Path;

use planora_project::config::{Config, MainWindow, WindowState};
use planora_update::Version;

/// Höhe des Kopfstreifens, dessen Lage auf einem Monitor liegen muss (Punkte).
const HEAD: f32 = 24.0;

/// Nächster gemerkter Stand nach dem Bild mit `info`: minimiert → unverändert; maximiert →
/// der bisherige normale Stand mit „maximiert“ (ohne bisherigen der maximierte als Ersatz);
/// sonst der gemeldete Stand mit der Skalierung des Monitors.
pub fn track(prev: Option<MainWindow>, info: &egui::ViewportInfo) -> Option<MainWindow> {
    if info.minimized == Some(true) {
        return prev;
    }
    if info.maximized == Some(true) {
        return match prev {
            Some(mut p) => {
                p.window.maximized = true;
                Some(p)
            }
            None => crate::update_install::window_state(info).map(|window| MainWindow {
                window,
                scale: info.native_pixels_per_point,
            }),
        };
    }
    crate::update_install::window_state(info).map(|mut window| {
        window.maximized = false;
        MainWindow {
            window,
            scale: info.native_pixels_per_point,
        }
    })
}

/// Fenster für den Start: nach einem Update das der alten Version, sonst das gemerkte. Liegt
/// dessen Kopfstreifen auf keinem Monitor mehr (Monitor abgezogen), fällt die Lage weg und das
/// System setzt das Fenster; die Größe bleibt.
pub fn at_start(config: &Config, running: &Version) -> Option<WindowState> {
    if let Some(w) = crate::update_install::window_for_start(config, running) {
        return Some(w);
    }
    let saved = config.main_window?;
    let mut w = saved.window;
    if let (Some(x), Some(y)) = (w.x, w.y) {
        let s = saved.scale.unwrap_or(1.0);
        let head_px =
            egui::Rect::from_min_size(egui::pos2(x * s, y * s), egui::vec2(w.width * s, HEAD * s));
        if crate::detach::on_screen(head_px) == Some(false) {
            w.x = None;
            w.y = None;
        }
    }
    Some(w)
}

/// Schreibt nur das gemerkte Fenster in die Konfigurationsdatei; `true`, wenn geschrieben
/// wurde. Ein Fehler lässt die Datei unangetastet.
pub fn save(config_path: &Path, w: MainWindow) -> bool {
    Config::update_at(config_path, |c| c.main_window = Some(w)).is_ok()
}
