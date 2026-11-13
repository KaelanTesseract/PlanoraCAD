//! Abgedockte Fenster: Plätze, Zustand je Platz, Monitorprüfung und Umrechnung zwischen
//! Punkten (egui) und physischen Pixeln (config.json).

use planora_project::config::{Config, DetachedWindow};
use std::collections::BTreeMap;
use std::path::Path;

/// Ein Platz, der sich als eigenes Fenster abdocken lässt.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Place {
    Tree,
    Right,
    SketchSide,
    Dialog(&'static str),
}

/// Alle Dialogschlüssel (wird mit den Dialogen in Aufgabe 5 gefüllt).
pub const DIALOG_KEYS: &[&str] = &[];

impl Place {
    /// Schlüssel in `config.json` (`detached`).
    pub fn key(self) -> String {
        match self {
            Place::Tree => "tree".to_string(),
            Place::Right => "right".to_string(),
            Place::SketchSide => "sketch_side".to_string(),
            Place::Dialog(k) => format!("dialog:{k}"),
        }
    }
}

/// Welche Plätze abgedockt sind und wo; `dirty`, wenn seit dem letzten Speichern etwas
/// geändert wurde.
#[derive(Clone, Debug, Default)]
pub struct DetachState {
    windows: BTreeMap<String, DetachedWindow>,
    dirty: bool,
}

impl DetachState {
    pub fn from_config(c: &Config) -> Self {
        DetachState {
            windows: c.detached.clone(),
            dirty: false,
        }
    }

    pub fn get(&self, p: Place) -> Option<DetachedWindow> {
        self.windows.get(&p.key()).copied()
    }

    /// Dockt den Platz wieder an.
    pub fn undock(&mut self, p: Place) {
        if self.windows.remove(&p.key()).is_some() {
            self.dirty = true;
        }
    }

    /// Merkt Lage und Größe des abgedockten Platzes.
    pub fn set(&mut self, p: Place, w: DetachedWindow) {
        if self.windows.insert(p.key(), w) != Some(w) {
            self.dirty = true;
        }
    }

    /// `true`, wenn seit dem letzten Aufruf etwas geändert wurde (setzt zurück).
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Schreibt nur das Feld `detached` in die Konfigurationsdatei; `true`, wenn geschrieben
    /// wurde. Ein Fehler lässt die Datei unangetastet und bricht nichts ab.
    pub fn save(&self, config_path: &Path) -> bool {
        let windows = self.windows.clone();
        Config::update_at(config_path, |c| c.detached = windows).is_ok()
    }
}

/// Prüft, ob der Kopfstreifen des Fensters (physische Pixel) auf einem Monitor liegt.
/// `None`, wenn die Plattform das nicht beantworten kann.
#[cfg(windows)]
pub fn on_screen(rect_px: egui::Rect) -> Option<bool> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromRect};
    let head = egui::Rect::from_min_size(rect_px.min, egui::vec2(64.0, 24.0));
    let r = RECT {
        left: head.min.x.round() as i32,
        top: head.min.y.round() as i32,
        right: head.max.x.round() as i32,
        bottom: head.max.y.round() as i32,
    };
    // SAFETY: `r` ist ein gültiger Zeiger auf ein lokales RECT.
    let monitor = unsafe { MonitorFromRect(&r, MONITOR_DEFAULTTONULL) };
    Some(!monitor.is_null())
}

#[cfg(not(windows))]
pub fn on_screen(_rect_px: egui::Rect) -> Option<bool> {
    None
}

/// Das gespeicherte Fenster, falls es abgedockt starten soll: `None`, wenn nichts gespeichert
/// ist oder der Monitor fehlt (dann bleibt der Platz angedockt).
pub fn should_detach(
    saved: Option<DetachedWindow>,
    on_screen: impl Fn(egui::Rect) -> Option<bool>,
) -> Option<DetachedWindow> {
    let w = saved?;
    let rect = egui::Rect::from_min_size(egui::pos2(w.x, w.y), egui::vec2(w.width, w.height));
    (on_screen(rect) != Some(false)).then_some(w)
}

/// Rechteck in Punkten nach physischen Pixeln.
pub fn to_physical(rect_points: egui::Rect, pixels_per_point: f32) -> DetachedWindow {
    DetachedWindow {
        x: rect_points.min.x * pixels_per_point,
        y: rect_points.min.y * pixels_per_point,
        width: rect_points.width() * pixels_per_point,
        height: rect_points.height() * pixels_per_point,
    }
}

/// Physische Pixel nach Punkten.
pub fn to_points(w: DetachedWindow, pixels_per_point: f32) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(w.x / pixels_per_point, w.y / pixels_per_point),
        egui::vec2(w.width / pixels_per_point, w.height / pixels_per_point),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win() -> DetachedWindow {
        DetachedWindow {
            x: -1500.0,
            y: 40.0,
            width: 400.0,
            height: 600.0,
        }
    }

    #[test]
    fn place_keys() {
        assert_eq!(Place::Tree.key(), "tree");
        assert_eq!(Place::Right.key(), "right");
        assert_eq!(Place::SketchSide.key(), "sketch_side");
        assert_eq!(Place::Dialog("blend").key(), "dialog:blend");
    }

    #[test]
    fn should_detach_missing_monitor_keeps_docked() {
        assert_eq!(should_detach(Some(win()), |_| Some(false)), None);
        assert_eq!(should_detach(None, |_| Some(true)), None);
    }

    #[test]
    fn should_detach_unknown_platform_trusts_saved() {
        assert_eq!(should_detach(Some(win()), |_| None), Some(win()));
        assert_eq!(should_detach(Some(win()), |_| Some(true)), Some(win()));
    }

    #[test]
    fn physical_round_trip_125() {
        let r = egui::Rect::from_min_size(egui::pos2(-120.3, 33.7), egui::vec2(320.0, 480.5));
        let back = to_points(to_physical(r, 1.25), 1.25);
        assert!((back.min - r.min).length() < 0.01);
        assert!((back.max - r.max).length() < 0.01);
    }

    #[test]
    fn state_undock_marks_dirty() {
        let mut s = DetachState::default();
        assert!(!s.take_dirty());
        s.set(Place::Tree, win());
        assert_eq!(s.get(Place::Tree), Some(win()));
        assert!(s.take_dirty());
        assert!(!s.take_dirty());
        s.set(Place::Tree, win());
        assert!(!s.take_dirty(), "gleicher Wert ist keine Änderung");
        s.undock(Place::Tree);
        assert_eq!(s.get(Place::Tree), None);
        assert!(s.take_dirty());
        s.undock(Place::Tree);
        assert!(!s.take_dirty());
    }

    #[test]
    fn save_writes_only_detached_field() {
        let dir = std::env::temp_dir().join(format!("planora-detach-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let mut s = DetachState::default();
        s.set(Place::Right, win());
        assert!(s.save(&path));
        let c = Config::load_from(&path);
        assert_eq!(DetachState::from_config(&c).get(Place::Right), Some(win()));
        std::fs::remove_dir_all(&dir).ok();
    }
}
