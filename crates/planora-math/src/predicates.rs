use glam::DVec2;
use robust::Coord;

/// Exakter Orientierungstest: > 0, wenn `a → b → c` gegen den Uhrzeigersinn läuft,
/// < 0 im Uhrzeigersinn, 0 bei kollinearen Punkten.
pub fn orient2d(a: DVec2, b: DVec2, c: DVec2) -> f64 {
    robust::orient2d(
        Coord { x: a.x, y: a.y },
        Coord { x: b.x, y: b.y },
        Coord { x: c.x, y: c.y },
    )
}
