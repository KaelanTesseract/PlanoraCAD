//! Monitorprüfung und Umrechnung zwischen Punkten und physischen Pixeln.

use super::*;

thread_local! {
    /// Nachgebildete Monitore (physische Pixel) für Tests, je Thread; `None`: echte Prüfung.
    pub(super) static TEST_MONITORS: std::cell::RefCell<Option<Vec<egui::Rect>>> =
        const { std::cell::RefCell::new(None) };
}

/// Setzt für diesen Thread nachgebildete Monitore (für Tests): [`on_screen`] fragt dann nicht
/// das System, sondern prüft, ob der Kopfstreifen einen davon schneidet. So hängen die Tests
/// nicht vom echten Bildschirm des Rechners ab. `None` stellt die echte Prüfung wieder her.
#[doc(hidden)]
pub fn set_test_monitors(monitors: Option<Vec<egui::Rect>>) {
    TEST_MONITORS.with(|m| *m.borrow_mut() = monitors);
}

/// Prüft, ob der Kopfstreifen des Fensters (physische Pixel) auf einem Monitor liegt.
/// `None`, wenn die Plattform das nicht beantworten kann.
pub fn on_screen(rect_px: egui::Rect) -> Option<bool> {
    let head = egui::Rect::from_min_size(rect_px.min, egui::vec2(64.0, 24.0));
    // Wie `MonitorFromRect` mit `MONITOR_DEFAULTTONULL`: Ein Schnitt reicht.
    if let Some(hit) = TEST_MONITORS.with(|m| {
        m.borrow()
            .as_ref()
            .map(|ms| ms.iter().any(|s| s.intersects(head)))
    }) {
        return Some(hit);
    }
    system_on_screen(head)
}

#[cfg(windows)]
pub(super) fn system_on_screen(head: egui::Rect) -> Option<bool> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromRect};
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
pub(super) fn system_on_screen(_head: egui::Rect) -> Option<bool> {
    None
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
