//! Gemerktes Programmfenster (wie Fusion): Beim sauberen Schließen kommen Größe, Lage,
//! „maximiert“ und die Skalierung des Monitors in die Konfiguration (`main_window`); der nächste
//! Start öffnet wieder so. Bei maximiertem Fenster bleiben Größe und Lage des normalen Zustands
//! gemerkt, damit das Fenster beim Verkleinern seine alte Größe bekommt.
//!
//! Die Lage wird nicht über den Fenster-Builder gesetzt: winit rechnet sie mit der Skalierung
//! des Monitors um, auf dem das neue Fenster entsteht, und bei Monitoren mit verschiedener
//! Skalierung landete es daneben. Stattdessen schiebt das erste Bild das Fenster auf die
//! gemerkte Stelle in physischen Pixeln ([`place_step`]) und maximiert es erst danach.

use std::path::Path;

use planora_project::config::{Config, MainWindow, WindowState};
use planora_update::Version;

/// Höhe des Kopfstreifens, dessen Lage auf einem Monitor liegen muss (Punkte).
const HEAD: f32 = 24.0;
/// So oft wird die Lage beim Start höchstens nachgeschoben (ein Monitorwechsel ändert die
/// Skalierung, dann stimmt die erste Umrechnung nicht).
pub const PLACE_TRIES: u8 = 3;
/// Abweichung in physischen Pixeln, die beim Platzieren noch als „angekommen“ gilt.
const PLACE_SLACK_PX: f32 = 2.0;

/// Stand des Fensters nach diesem Bild ohne „maximiert“: Lage der Außenkante, Größe der
/// Innenfläche (Punkte), mit der Skalierung des Monitors. `None` ohne Rechtecke (etwa unter
/// Wayland, das keine Lage meldet) oder bei unbrauchbaren Werten.
fn current(info: &egui::ViewportInfo) -> Option<MainWindow> {
    let inner = info.inner_rect?;
    let outer = info.outer_rect?;
    let window = WindowState {
        x: Some(outer.min.x),
        y: Some(outer.min.y),
        width: inner.width(),
        height: inner.height(),
        maximized: false,
    }
    .sanitized_any_monitor()?;
    Some(MainWindow {
        window,
        scale: info.native_pixels_per_point,
    })
}

/// Nächster gemerkter Stand nach dem Bild mit `info`: minimiert oder ohne Angaben →
/// unverändert; maximiert → der bisherige normale Stand mit „maximiert“ (ohne bisherigen der
/// maximierte als Ersatz); sonst der gemeldete Stand.
pub fn track(prev: Option<MainWindow>, info: &egui::ViewportInfo) -> Option<MainWindow> {
    if info.minimized == Some(true) {
        return prev;
    }
    let Some(now) = current(info) else {
        return prev;
    };
    if info.maximized == Some(true) {
        let mut base = prev.unwrap_or(now);
        base.window.maximized = true;
        return Some(base);
    }
    Some(now)
}

/// Wohin das erste Bild das Fenster schiebt: linke obere Ecke der Außenkante in physischen
/// Pixeln (fehlt sie, setzt das System das Fenster) und ob es danach maximiert wird.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub px: Option<egui::Pos2>,
    pub maximize: bool,
}

/// Fenster für den Start: `WindowState` für den Fenster-Builder (nur die Größe; Lage und
/// „maximiert“ setzt [`place_step`]) und die Platzierung. Zuerst das gemerkte Fenster, sonst
/// nach einem Update das der alten Version (ältere Versionen merken kein `main_window`). Liegt
/// der Kopfstreifen des gemerkten auf keinem Monitor mehr (Monitor abgezogen), fällt die Lage
/// weg; die Größe bleibt.
pub fn at_start(config: &Config, running: &Version) -> Option<(WindowState, Place)> {
    let (w, scale) = match config.main_window {
        Some(m) => (m.window, m.scale.unwrap_or(1.0)),
        None => (
            crate::update_install::window_for_start(config, running)?.sanitized()?,
            1.0,
        ),
    };
    let px = match (w.x, w.y) {
        (Some(x), Some(y)) => {
            let at = egui::pos2(x * scale, y * scale);
            let head = egui::Rect::from_min_size(at, egui::vec2(w.width * scale, HEAD * scale));
            (crate::detach::on_screen(head) != Some(false)).then_some(at)
        }
        _ => None,
    };
    let size_only = WindowState {
        x: None,
        y: None,
        maximized: false,
        ..w
    };
    Some((
        size_only,
        Place {
            px,
            maximize: w.maximized,
        },
    ))
}

/// Ein Schritt des Platzierens im Bild mit `info`: die Befehle an das Fenster und ob die
/// Platzierung damit erledigt ist. Liegt das Fenster nicht an der Stelle (Pixel), wird es mit
/// der Skalierung des Monitors, auf dem es jetzt steht, dorthin geschoben, höchstens
/// [`PLACE_TRIES`]-mal; danach (oder ohne Lage) wird es, falls gewollt, maximiert.
pub fn place_step(
    place: Place,
    info: &egui::ViewportInfo,
    tries: u8,
) -> (Vec<egui::ViewportCommand>, bool) {
    if let (Some(px), Some(ppp), Some(outer)) =
        (place.px, info.native_pixels_per_point, info.outer_rect)
        && ppp > 0.0
        && tries < PLACE_TRIES
        && (outer.min.to_vec2() * ppp - px.to_vec2()).length() > PLACE_SLACK_PX
    {
        return (
            vec![egui::ViewportCommand::OuterPosition(egui::pos2(
                px.x / ppp,
                px.y / ppp,
            ))],
            false,
        );
    }
    let cmds = if place.maximize {
        vec![egui::ViewportCommand::Maximized(true)]
    } else {
        Vec::new()
    };
    (cmds, true)
}

/// Schreibt nur das gemerkte Fenster in die Konfigurationsdatei; `true`, wenn geschrieben
/// wurde. Ein Fehler lässt die Datei unangetastet.
pub fn save(config_path: &Path, w: MainWindow) -> bool {
    Config::update_at(config_path, |c| c.main_window = Some(w)).is_ok()
}
