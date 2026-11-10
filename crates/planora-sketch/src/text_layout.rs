//! Lage der Glyphen eines gesetzten Textes in der Skizze (Skizzen 8, reine Geometrie).
//!
//! Ein [`TextShape`] steht in Einheiten der Höhe mit der Grundlinie der ersten Zeile bei y = 0.
//! Hier wird daraus die Lage in Skizzenkoordinaten (mm): im Rahmen nach Spec B5 und B6, auf
//! einem Pfad nach Spec B7.

use crate::geom::Geom;
use crate::text::{HAlign, VAlign};
use crate::text_shape::{Contour, Glyph, Seg, TextShape};
use planora_math::DVec2;
use std::f64::consts::FRAC_PI_2;

/// Abschnitte der Längentabelle eines Pfades je Vollkreis (Splines ein Viertel davon je
/// Segment, Freiformkurven mit allen Knoten, siehe `Geom::polyline_params`).
const TABLE_STEPS_PER_TURN: usize = 64;
/// Höchstzahl der Schritte (Newton mit Halbieren als Rückfall), wenn zu einer Bogenlänge der
/// Parameter gesucht wird.
const INVERT_STEPS: usize = 60;
/// Ein Vorschub unter diesem Wert (Einheiten der Höhe) gilt als null: ein Akzent.
const ZERO_ADVANCE: f64 = 1e-9;

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

/// Ein Pfad in Leserichtung, nach der Bogenlänge `s` ab dem Anfang der Leserichtung (bei
/// geschlossenen Pfaden dem Bezugspunkt).
struct Walk<'a> {
    geom: &'a Geom,
    /// Parameter am Anfang der Leserichtung; `dir` ist +1 mit dem Parameter, −1 dagegen.
    t0: f64,
    dir: f64,
    /// Parameterspanne der ganzen Leserichtung: `τ ∈ [0, span]`, `t = t0 + dir·τ`.
    span: f64,
    length: f64,
    closed: bool,
    /// Stützstellen `(τ, Bogenlänge)` aufsteigend, für Kurven, deren Länge nicht gleichmäßig mit
    /// dem Parameter wächst; leer bei Linie, Kreis und Bogen.
    table: Vec<(f64, f64)>,
}

impl<'a> Walk<'a> {
    /// `None`, wenn der Pfad keine endliche, positive Länge hat.
    fn new(geom: &'a Geom, closed: bool, clockwise: bool) -> Option<Walk<'a>> {
        let (lo, hi) = geom.domain();
        let span = hi - lo;
        if !(span.is_finite() && span > 0.0) {
            return None;
        }
        let dir = if clockwise { -1.0 } else { 1.0 };
        let t0 = if closed && matches!(geom, Geom::Circle { .. }) {
            // Beim Kreis liegt der Bezugspunkt ganz oben (Spec B7).
            FRAC_PI_2
        } else if clockwise {
            hi
        } else {
            lo
        };
        let mut walk = Walk {
            geom,
            t0,
            dir,
            span,
            length: 0.0,
            closed,
            table: Vec::new(),
        };
        if matches!(
            geom,
            Geom::Line { .. } | Geom::Circle { .. } | Geom::Arc { .. }
        ) {
            walk.length = geom.arc_length(lo, hi);
        } else {
            let ts = geom.polyline_params(lo, hi, TABLE_STEPS_PER_TURN);
            let taus: Vec<f64> = if clockwise {
                ts.iter().rev().map(|t| hi - t).collect()
            } else {
                ts.iter().map(|t| t - lo).collect()
            };
            let mut table = Vec::with_capacity(taus.len());
            let mut sum = 0.0;
            table.push((0.0, 0.0));
            for w in taus.windows(2) {
                sum += geom.arc_length(walk.t(w[0]), walk.t(w[1]));
                table.push((w[1], sum));
            }
            walk.length = sum;
            walk.table = table;
        }
        (walk.length.is_finite() && walk.length > 0.0).then_some(walk)
    }

    fn t(&self, tau: f64) -> f64 {
        self.t0 + self.dir * tau
    }

    /// Der Parameterabstand `τ` zur Bogenlänge `s ∈ [0, length]`.
    fn tau_at(&self, s: f64) -> f64 {
        if self.table.is_empty() {
            return s / self.length * self.span;
        }
        let n = self.table.len();
        let k = self.table.partition_point(|&(_, c)| c <= s).clamp(1, n - 1) - 1;
        let ((a, ca), (b, cb)) = (self.table[k], self.table[k + 1]);
        let want = s - ca;
        if cb <= ca {
            return a;
        }
        let (mut lo, mut hi) = (a, b);
        let mut tau = a + (b - a) * (want / (cb - ca)).clamp(0.0, 1.0);
        let eps = 1e-13 * self.length;
        for _ in 0..INVERT_STEPS {
            let f = self.geom.arc_length(self.t(a), self.t(tau)) - want;
            if f.abs() <= eps {
                break;
            }
            if f > 0.0 {
                hi = tau;
            } else {
                lo = tau;
            }
            let speed = self.geom.eval(self.t(tau)).1.length();
            let next = tau - f / speed;
            // Newton, solange er im Einschluss bleibt (bei NaN nicht), sonst halbieren.
            tau = if next > lo && next < hi {
                next
            } else {
                0.5 * (lo + hi)
            };
            if hi - lo <= f64::EPSILON * self.span {
                break;
            }
        }
        tau
    }

    /// Punkt und Leserichtung (Länge 1) bei der Bogenlänge `s`. Auf offenem Pfad geht es vor dem
    /// Anfang und hinter dem Ende auf der Tangente weiter, auf geschlossenem um die Kurve herum.
    /// `None`, wo die Kurve keine Richtung hat.
    fn at(&self, s: f64) -> Option<(DVec2, DVec2)> {
        let (s, beyond) = if self.closed {
            (s.rem_euclid(self.length), 0.0)
        } else if s < 0.0 {
            (0.0, s)
        } else if s > self.length {
            (self.length, s - self.length)
        } else {
            (s, 0.0)
        };
        let tau = self.tau_at(s);
        let p = self.geom.eval(self.t(tau)).0;
        let u = self.direction(tau)?;
        Some((p + u * beyond, u))
    }

    /// Die Leserichtung bei `τ`; wo die Ableitung verschwindet (etwa aufeinanderliegende
    /// Kontrollpunkte), die Richtung einer kleinen Sehne um `τ`.
    fn direction(&self, tau: f64) -> Option<DVec2> {
        let unit = |v: DVec2| {
            let len = v.length();
            (len.is_finite() && len > 0.0).then(|| v / len)
        };
        let d = self.geom.eval(self.t(tau)).1 * self.dir;
        if d.length() > 1e-9 * self.length / self.span {
            return unit(d);
        }
        let h = 1e-6 * self.span;
        let (a, b) = if self.closed {
            (tau - h, tau + h)
        } else {
            ((tau - h).max(0.0), (tau + h).min(self.span))
        };
        unit(self.geom.eval(self.t(b)).0 - self.geom.eval(self.t(a)).0)
    }
}

/// Eine starre Einheit auf dem Pfad: eine Glyphe mit Vorschub samt den Akzenten (Glyphen ohne
/// Vorschub) über ihr. Anfang und Länge des Vorschubs auf der Zeile, in Einheiten der Höhe.
struct Unit {
    x: f64,
    advance: f64,
}

/// Die Glyphen aller Zeilen hintereinander mit ihrem Ursprung auf dieser einen Zeile, dazu die
/// ganze Breite. Für den Pfad werden die Zeilen schon beim Setzen mit Leerzeichen verbunden
/// ([`crate::text::path_content`]); kommen trotzdem mehrere, stehen sie lückenlos hintereinander.
fn glyph_row(shape: &TextShape) -> (Vec<(DVec2, &Glyph)>, f64) {
    let mut row = Vec::new();
    let mut x = 0.0;
    for line in &shape.lines {
        row.extend(
            line.glyphs
                .iter()
                .map(|g| (g.origin + DVec2::new(x, 0.0), g)),
        );
        x += line.width;
    }
    (row, x)
}

/// Mitte des Umrisses der Glyphe in x (über die Punkte auf der Kurve), relativ zum Ursprung.
fn ink_mid_x(g: &Glyph) -> f64 {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for c in &g.contours {
        for x in std::iter::once(c.start.x).chain(c.segs.iter().map(|s| s.end().x)) {
            lo = lo.min(x);
            hi = hi.max(x);
        }
    }
    if lo <= hi { 0.5 * (lo + hi) } else { 0.0 }
}

/// Die Einheiten und je Glyphe der Zeile die Einheit, zu der sie gehört. Jede Glyphe mit
/// Vorschub ist eine Einheit (auch ein Leerzeichen: Es nimmt Platz ein). Ein Akzent gehört zur
/// Glyphe mit Vorschub davor oder dahinter, je nachdem, über welcher die Mitte seines Umrisses
/// näher liegt; so bleibt er bei jeder Reihenfolge des Setzers auf seinem Buchstaben. Eine
/// Glyphe ohne Vorschub und ohne Umriss entfällt (`None`).
fn units(row: &[(DVec2, &Glyph)]) -> (Vec<Unit>, Vec<Option<usize>>) {
    let mut units = Vec::new();
    let mut base: Vec<Option<usize>> = Vec::with_capacity(row.len());
    for &(o, g) in row {
        if g.advance.abs() > ZERO_ADVANCE {
            base.push(Some(units.len()));
            units.push(Unit {
                x: o.x,
                advance: g.advance,
            });
        } else {
            base.push(None);
        }
    }
    let mut of = base.clone();
    for (i, &(o, g)) in row.iter().enumerate() {
        if base[i].is_some() || g.contours.is_empty() {
            continue;
        }
        let ink = o.x + ink_mid_x(g);
        let off = |u: &Unit| {
            let (a, b) = (u.x.min(u.x + u.advance), u.x.max(u.x + u.advance));
            (a - ink).max(ink - b).max(0.0)
        };
        let before = base[..i].iter().rev().find_map(|u| *u);
        let after = base[i + 1..].iter().find_map(|u| *u);
        let pick = match (before, after) {
            (Some(b), Some(a)) => Some(if off(&units[a]) < off(&units[b]) {
                a
            } else {
                b
            }),
            (Some(u), None) | (None, Some(u)) => Some(u),
            (None, None) => None,
        };
        of[i] = Some(pick.unwrap_or_else(|| {
            units.push(Unit {
                x: o.x,
                advance: 0.0,
            });
            units.len() - 1
        }));
    }
    (units, of)
}

/// Die Konturen des Textes auf dem Pfad `path_geom` in Skizzenkoordinaten, in der Reihenfolge
/// Glyphe, Kontur wie im `TextShape` (Spec B7).
///
/// - Leserichtung: mit dem Parameter, bei Kreis und Bogen (`is_circle_or_arc`) im
///   Uhrzeigersinn. Oberhalb (`above`) heißt links der Leserichtung mit der Grundlinie auf dem
///   Pfad, unterhalb rechts davon mit der Oberlänge auf dem Pfad; lesbar bleibt der Text beide
///   Male.
/// - Jede Glyphe bleibt starr. Sie sitzt mit der Mitte ihres Vorschubs auf der Grundlinie an
///   ihrer Bogenlänge auf dem Pfad, gedreht nach der Tangente dort. Akzente gehen mit ihrem
///   Buchstaben.
/// - Ausrichtung auf offenem Pfad: links am Anfang, zentriert in der Mitte der Länge, rechts am
///   Ende. Auf geschlossenem (`closed`) am Bezugspunkt, beim Kreis ganz oben, sonst am Anfang
///   der Leserichtung: links beginnt dort, zentriert sitzt darüber, rechts endet dort.
/// - Über das Ende hinaus geht es auf offenem Pfad auf der Tangente weiter, auf geschlossenem
///   um die Kurve herum.
/// - Anpassen (`fit`): gleiche Lücken zwischen den Vorschüben vom Anfang bis zum Ende, auf
///   geschlossenem Pfad auch zwischen letzter und erster Glyphe; zu langer Text bekommt
///   negative Lücken. Ausrichtung und Zeichenabstand wirken dann nicht. Eine einzelne Glyphe
///   steht auf offenem Pfad in der Mitte.
/// - Umkehren spiegelt wie im Rahmen (Spec B6): `flip_h` entlang des Pfades an seiner Mitte
///   (offen) bzw. am Bezugspunkt (geschlossen), die Leserichtung kehrt sich um; `flip_v` am
///   Pfad, der Text wechselt die Seite.
///
/// `None`, wenn der Pfad keine Länge hat oder an einer Glyphe keine Richtung: Dann behält der
/// Aufrufer die letzte gute Lage (Review Focus 3).
#[allow(
    clippy::too_many_arguments,
    reason = "Schnittstelle laut Plan (Task 5); jeder Wert ist ein eigenes Feld des Textes"
)]
pub(crate) fn place_path(
    shape: &TextShape,
    height: f64,
    path_geom: &Geom,
    closed: bool,
    is_circle_or_arc: bool,
    above: bool,
    fit: bool,
    h: HAlign,
    flip_h: bool,
    flip_v: bool,
) -> Option<Vec<Contour>> {
    let walk = Walk::new(path_geom, closed, is_circle_or_arc)?;
    let length = walk.length;
    let (row, width) = glyph_row(shape);
    let (units, of) = units(&row);
    // Bogenlänge der Mitte jeder Einheit in Leserichtung.
    let mids: Vec<f64> = if fit {
        let gaps = if closed {
            units.len()
        } else {
            units.len().saturating_sub(1)
        };
        if gaps == 0 {
            units.iter().map(|_| 0.5 * length).collect()
        } else {
            let used: f64 = units.iter().map(|u| u.advance * height).sum();
            let gap = (length - used) / gaps as f64;
            let mut s = 0.0;
            units
                .iter()
                .map(|u| {
                    let a = u.advance * height;
                    let mid = s + 0.5 * a;
                    s += a + gap;
                    mid
                })
                .collect()
        }
    } else {
        let w = width * height;
        let start = match (closed, h) {
            (_, HAlign::Left) => 0.0,
            (false, HAlign::Center) => 0.5 * (length - w),
            (false, HAlign::Right) => length - w,
            (true, HAlign::Center) => -0.5 * w,
            (true, HAlign::Right) => -w,
        };
        units
            .iter()
            .map(|u| start + (u.x + 0.5 * u.advance) * height)
            .collect()
    };
    let frames: Vec<(DVec2, DVec2)> = mids
        .iter()
        .map(|&s| {
            let s = match (flip_h, closed) {
                (false, _) => s,
                (true, true) => -s,
                (true, false) => length - s,
            };
            walk.at(s)
        })
        .collect::<Option<_>>()?;
    let lift = if above { 0.0 } else { -shape.ascender * height };
    let mut out = Vec::with_capacity(row.iter().map(|(_, g)| g.contours.len()).sum());
    for (&(origin, g), unit) in row.iter().zip(&of) {
        let Some(k) = *unit else {
            continue;
        };
        let (p, u) = frames[k];
        let n = u.perp();
        let mid = units[k].x + 0.5 * units[k].advance;
        for c in &g.contours {
            out.push(map_contour(c, |q| {
                let at = origin + q;
                let dx = (at.x - mid) * height;
                let dy = at.y * height + lift;
                let dx = if flip_h { -dx } else { dx };
                let dy = if flip_v { -dy } else { dy };
                p + u * dx + n * dy
            }));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_shape::ShapedLine;

    /// Ein Zeichen: ein Quadrat mit Vorschub 1.
    fn square() -> TextShape {
        let v = DVec2::new;
        TextShape {
            lines: vec![ShapedLine {
                glyphs: vec![Glyph {
                    origin: DVec2::ZERO,
                    advance: 1.0,
                    contours: vec![Contour {
                        start: v(0.0, 0.0),
                        segs: vec![
                            Seg::Line(v(1.0, 0.0)),
                            Seg::Line(v(1.0, 1.0)),
                            Seg::Line(v(0.0, 1.0)),
                            Seg::Line(v(0.0, 0.0)),
                        ],
                    }],
                }],
                width: 1.0,
            }],
            ascender: 0.9,
            descender: -0.2,
            line_height: 1.2,
            substituted: None,
            synthetic: false,
            missing_chars: false,
        }
    }

    fn place(g: &Geom, closed: bool, round: bool) -> Option<Vec<Contour>> {
        place_path(
            &square(),
            5.0,
            g,
            closed,
            round,
            true,
            false,
            HAlign::Center,
            false,
            false,
        )
    }

    #[test]
    fn degenerate_paths_give_none() {
        // Kreis mit Radius 0 (beim Ziehen), Linie mit gleichen Enden, Pfade mit NaN und ∞.
        let c = DVec2::new(3.0, 4.0);
        assert!(place(&Geom::Circle { c, r: 0.0 }, true, true).is_none());
        assert!(place(&Geom::Line { a: c, b: c }, false, false).is_none());
        let nan = DVec2::new(f64::NAN, 0.0);
        assert!(place(&Geom::Line { a: c, b: nan }, false, false).is_none());
        let inf = Geom::Circle {
            c,
            r: f64::INFINITY,
        };
        assert!(place(&inf, true, true).is_none());
        let flat = Geom::Ellipse {
            c,
            a: DVec2::ZERO,
            b: DVec2::ZERO,
        };
        assert!(place(&flat, true, false).is_none());
        // Ein gewöhnlicher Kreis geht.
        assert!(place(&Geom::Circle { c, r: 2.0 }, true, true).is_some());
    }
}
