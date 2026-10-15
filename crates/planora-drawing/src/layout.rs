//! Blattlayout: Abstand und Ausrichtung abgeleiteter Ansichten.

use crate::basis::Basis;
use crate::model::{Projection, Scale, ViewKind};

/// Abstand zwischen Ansichten in Millimetern.
pub const GAP_MM: f64 = 20.0;

/// Einheitsrichtung auf dem Blatt, in der die abgeleitete Ansicht `child` vom Elternbild
/// `parent` liegt. `None`, wenn die Ansicht nicht abgeleitet werden kann (isometrisch).
pub fn align_dir(projection: Projection, parent: ViewKind, child: ViewKind) -> Option<(f64, f64)> {
    if child == ViewKind::Isometric {
        return None;
    }
    let pb = Basis::of(parent);
    let e = Basis::of(child).eye_dir();
    let (a, b) = (e.dot(pb.ex), e.dot(pb.ey));
    if a * a + b * b < 1e-9 {
        return Some((1.0, 0.0));
    }
    let (a, b) = match projection {
        Projection::First => (-a, -b),
        Projection::Third => (a, b),
    };
    Some(if a.abs() >= b.abs() {
        (if a > 0.0 { 1.0 } else { -1.0 }, 0.0)
    } else {
        (0.0, if b > 0.0 { 1.0 } else { -1.0 })
    })
}

/// Breite und Höhe einer Ansicht auf dem Blatt in Millimetern.
pub fn view_extent(lines_bbox: ((f64, f64), (f64, f64)), scale: Scale) -> (f64, f64) {
    let ((x0, y0), (x1, y1)) = lines_bbox;
    let f = scale.factor();
    ((x1 - x0) * f, (y1 - y0) * f)
}
