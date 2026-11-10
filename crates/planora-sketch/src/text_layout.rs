//! Lage der Glyphen eines gesetzten Textes in der Skizze (Skizzen 8, reine Geometrie).
//!
//! Ein [`TextShape`] steht in Einheiten der Höhe mit der Grundlinie der ersten Zeile bei y = 0.
//! Hier wird daraus die Lage in Skizzenkoordinaten (mm): im Rahmen nach Spec B5 und B6.

use crate::text::{HAlign, VAlign};
use crate::text_shape::{Contour, Seg, TextShape};
use planora_math::DVec2;

/// Bildet jeden Punkt einer Kontur mit `f` ab.
pub(crate) fn map_contour(c: &Contour, f: impl Fn(DVec2) -> DVec2) -> Contour {
    Contour {
        start: f(c.start),
        segs: c
            .segs
            .iter()
            .map(|s| match *s {
                Seg::Line(p) => Seg::Line(f(p)),
                Seg::Quad(a, p) => Seg::Quad(f(a), f(p)),
                Seg::Cubic(a, b, p) => Seg::Cubic(f(a), f(b), f(p)),
            })
            .collect(),
    }
}

/// Lage der Grundlinie der ersten Zeile über der Grundlinie des Rahmens (mm), bei der Höhe
/// `height` und dem Abstand `box_h` von Grund- und Gegenlinie. Bezug oben ist die Oberlänge der
/// ersten Zeile, unten die Unterlänge der letzten Zeile, Mitte die Mitte dazwischen (Spec B5,
/// **Annahme**).
fn first_baseline(shape: &TextShape, height: f64, box_h: f64, v: VAlign) -> f64 {
    let below = shape.lines.len().saturating_sub(1) as f64 * shape.line_height * height;
    // Ober- und Unterkante des Textblocks, gemessen ab der ersten Grundlinie.
    let top = shape.ascender * height;
    let bottom = shape.descender * height - below;
    match v {
        VAlign::Bottom => -bottom,
        VAlign::Top => box_h - top,
        VAlign::Middle => 0.5 * box_h - 0.5 * (top + bottom),
    }
}

/// Die Konturen des Textes im Rahmen mit den Ecken `corners` (Ecke 0 → 1 ist die Grundlinie,
/// Ecke 3 liegt über Ecke 0), in Skizzenkoordinaten und in der Reihenfolge Zeile, Glyphe, Kontur.
///
/// - Die Grundlinie gibt die Richtung des Textes; „oben“ steht senkrecht dazu, links davon. Der
///   Abstand der Gegenlinie zählt vorzeichenbehaftet in diese Richtung.
/// - Waagerecht: jede Zeile mit ihrer Breite links, mittig oder rechts an der Grundlinie.
/// - Senkrecht: siehe [`first_baseline`]. Der Text darf über den Rahmen hinausragen.
/// - Umkehren spiegelt an der senkrechten (`flip_h`) bzw. waagerechten (`flip_v`) Mittelachse
///   des Rahmens (Spec B6).
///
/// Eine Grundlinie der Länge null gibt die Richtung der x-Achse; der Aufrufer behält in dem Fall
/// besser die letzte gute Lage.
pub fn place_box(
    shape: &TextShape,
    height: f64,
    corners: [DVec2; 4],
    h: HAlign,
    v: VAlign,
    flip_h: bool,
    flip_v: bool,
) -> Vec<Contour> {
    let base = corners[1] - corners[0];
    let width = base.length();
    let u = if width > 0.0 && width.is_finite() {
        base / width
    } else {
        DVec2::X
    };
    let n = u.perp();
    let box_h = (corners[3] - corners[0]).dot(n);
    let first = first_baseline(shape, height, box_h, v);
    let to_sketch = |p: DVec2| {
        let x = if flip_h { width - p.x } else { p.x };
        let y = if flip_v { box_h - p.y } else { p.y };
        corners[0] + u * x + n * y
    };
    let mut out = Vec::with_capacity(
        shape
            .lines
            .iter()
            .flat_map(|l| &l.glyphs)
            .map(|g| g.contours.len())
            .sum(),
    );
    for (i, line) in shape.lines.iter().enumerate() {
        let line_w = line.width * height;
        let x0 = match h {
            HAlign::Left => 0.0,
            HAlign::Center => 0.5 * (width - line_w),
            HAlign::Right => width - line_w,
        };
        let y0 = first - i as f64 * shape.line_height * height;
        for g in &line.glyphs {
            let origin = DVec2::new(x0, y0) + g.origin * height;
            for c in &g.contours {
                out.push(map_contour(c, |q| to_sketch(origin + q * height)));
            }
        }
    }
    out
}
