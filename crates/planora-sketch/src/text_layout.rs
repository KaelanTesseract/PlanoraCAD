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
/// Gauß-Legendre mit fünf Stellen auf `[−1, 1]`: Stelle und Gewicht (exakt bis Grad 9).
const GAUSS_5: [(f64, f64); 5] = [
    (-0.906_179_845_938_664, 0.236_926_885_056_189_08),
    (-0.538_469_310_105_683_1, 0.478_628_670_499_366_47),
    (0.0, 0.568_888_888_888_888_9),
    (0.538_469_310_105_683_1, 0.478_628_670_499_366_47),
    (0.906_179_845_938_664, 0.236_926_885_056_189_08),
];

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
///
/// Eine reine Funktion aus Geometrie, `closed` und `clockwise`: Der Text merkt sie sich zur
/// Laufzeit ([`Walk::fits`]) und baut die Längentabelle nur neu, wenn sich der Pfad ändert.
/// Sonst kostete jedes Lösen zwei Tabellen, bei einer projizierten Freiformkurve mit vielen
/// Knoten zigtausend Auswertungen.
#[derive(Clone)]
pub(crate) struct Walk {
    geom: Geom,
    clockwise: bool,
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

impl std::fmt::Debug for Walk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Ohne Tabelle: Sie hat bis zu 65537 Einträge.
        f.debug_struct("Walk")
            .field("length", &self.length)
            .field("closed", &self.closed)
            .field("clockwise", &self.clockwise)
            .field("table", &self.table.len())
            .finish()
    }
}

impl Walk {
    /// `None`, wenn der Pfad keine endliche, positive Länge hat.
    pub(crate) fn new(geom: Geom, closed: bool, clockwise: bool) -> Option<Walk> {
        let (lo, hi) = geom.domain();
        let span = hi - lo;
        if !(span.is_finite() && span > 0.0) {
            return None;
        }
        let dir = if clockwise { -1.0 } else { 1.0 };
        // Bezugspunkt geschlossener Pfade: bei Kreis und Ellipse ganz oben (Spec B7, Ruling 5b
        // für die Ellipse), sonst der Anfang der Leserichtung. Kreis und Ellipse sind periodisch,
        // ihr Parameter darf also über den Bereich hinaus laufen.
        let t0 = match &geom {
            Geom::Circle { .. } if closed => FRAC_PI_2,
            // y = c.y + a.y·cos t + b.y·sin t ist am größten bei t = atan2(b.y, a.y).
            Geom::Ellipse { a, b, .. } if closed => b.y.atan2(a.y),
            _ if clockwise => hi,
            _ => lo,
        };
        let mut walk = Walk {
            geom,
            clockwise,
            t0,
            dir,
            span,
            length: 0.0,
            closed,
            table: Vec::new(),
        };
        if matches!(
            walk.geom,
            Geom::Line { .. } | Geom::Circle { .. } | Geom::Arc { .. }
        ) {
            walk.length = walk.geom.arc_length(lo, hi);
        } else {
            // Abgetastet wird von `t0` in Leserichtung über die ganze Spanne, auch über die Naht
            // einer Ellipse hinweg (Bezugspunkt oben); so passen Tabelle und `t(τ)` zusammen.
            let ts = walk
                .geom
                .polyline_params(walk.t(0.0), walk.t(span), TABLE_STEPS_PER_TURN);
            let taus: Vec<f64> = ts
                .iter()
                .map(|t| ((t - t0) * dir).clamp(0.0, span))
                .collect();
            let mut table = Vec::with_capacity(taus.len());
            let mut sum = 0.0;
            table.push((0.0, 0.0));
            for w in taus.windows(2) {
                sum += walk.length_between(w[0], w[1]);
                table.push((w[1], sum));
            }
            walk.length = sum;
            walk.table = table;
        }
        (walk.length.is_finite() && walk.length > 0.0).then_some(walk)
    }

    /// Gilt dieser Weg für den Pfad `geom` mit `closed` und `clockwise`? Dann ist er derselbe,
    /// den [`Walk::new`] bauen würde.
    pub(crate) fn fits(&self, geom: &Geom, closed: bool, clockwise: bool) -> bool {
        self.closed == closed && self.clockwise == clockwise && self.geom == *geom
    }

    fn t(&self, tau: f64) -> f64 {
        self.t0 + self.dir * tau
    }

    /// Bogenlänge zwischen `τ = a` und `τ = b`, die in einem Abschnitt der Längentabelle liegen.
    /// Bei Freiformkurven liegt jeder Abschnitt in einem Bézier-Stück (die Tabelle enthält alle
    /// Knoten), dort ist die Geschwindigkeit glatt: Gauß-Legendre mit fünf Stellen genügt. Die
    /// allgemeine Länge rechnete je Abschnitt 33 Auswertungen, bei einem Pfad mit vielen Knoten
    /// in jedem Bild zigtausendfach. Andere Kurven wie [`Geom::arc_length`].
    fn length_between(&self, a: f64, b: f64) -> f64 {
        if !matches!(self.geom, Geom::Nurbs { .. }) {
            return self.geom.arc_length(self.t(a), self.t(b));
        }
        let (ta, tb) = (self.t(a), self.t(b));
        let (mid, half) = (0.5 * (ta + tb), 0.5 * (tb - ta).abs());
        if !(half.is_finite() && half > 0.0) {
            return 0.0;
        }
        GAUSS_5
            .iter()
            .map(|&(x, w)| w * self.geom.eval(mid + half * x).1.length())
            .sum::<f64>()
            * half
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
            let f = self.length_between(a, tau) - want;
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
            // Über die Naht herum statt aus dem Bereich hinaus (eine geschlossene Freiformkurve
            // liefe sonst auf der Verlängerung ihres ersten bzw. letzten Stücks).
            (
                (tau - h).rem_euclid(self.span),
                (tau + h).rem_euclid(self.span),
            )
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

/// Die Konturen des Textes auf dem Pfad `walk` (siehe [`Walk::new`]) in Skizzenkoordinaten, in
/// der Reihenfolge Glyphe, Kontur wie im `TextShape` (Spec B7).
///
/// - Leserichtung: mit dem Parameter, bei Kreis, Bogen, Ellipse und Ellipsenbogen (`clockwise`,
///   Ruling 5b) im Uhrzeigersinn. Oberhalb (`above`) heißt links der Leserichtung mit der
///   Grundlinie auf dem Pfad, unterhalb rechts davon mit der Oberlänge auf dem Pfad; lesbar
///   bleibt der Text beide Male.
/// - Jede Glyphe bleibt starr. Sie sitzt mit der Mitte ihres Vorschubs auf der Grundlinie an
///   ihrer Bogenlänge auf dem Pfad, gedreht nach der Tangente dort. Akzente gehen mit ihrem
///   Buchstaben.
/// - Ausrichtung auf offenem Pfad: links am Anfang, zentriert in der Mitte der Länge, rechts am
///   Ende. Auf geschlossenem (`closed`) am Bezugspunkt, bei Kreis und Ellipse ganz oben, sonst
///   am Anfang der Leserichtung: links beginnt dort, zentriert sitzt darüber, rechts endet dort.
/// - Über das Ende hinaus geht es auf offenem Pfad auf der Tangente weiter, auf geschlossenem
///   um die Kurve herum.
/// - Anpassen (`fit`): gleiche Lücken zwischen den Vorschüben vom Anfang bis zum Ende, auf
///   geschlossenem Pfad auch zwischen letzter und erster Glyphe; zu langer Text bekommt
///   negative Lücken. Ausrichtung und Zeichenabstand wirken dann nicht. Eine einzelne Glyphe
///   steht auf offenem Pfad in der Mitte.
/// - Umkehren (Spec B6, Ruling 5a/5d): `flip_h` kehrt die Leserichtung um, die Buchstaben bleiben
///   lesbar (nicht spiegelverkehrt). Gelesen wird dann vom Ende zum Anfang (offen) bzw. am
///   Bezugspunkt in die andere Richtung herum (geschlossen); oberhalb und unterhalb gelten
///   weiter zur Leserichtung, der Text steht also auf der anderen Seite des Pfades (auf dem
///   Kreis: gegen den Uhrzeigersinn, unten lesbar). `flip_v` wechselt die Seite (Spiegelung am
///   Pfad).
///
/// Den Inhalt setzt der Aufrufer mit verbundenen Zeilen ([`crate::text::path_content`]), das
/// Ergebnis hat also höchstens eine Zeile.
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
    walk: &Walk,
    above: bool,
    fit: bool,
    h: HAlign,
    flip_h: bool,
    flip_v: bool,
) -> Option<Vec<Contour>> {
    debug_assert!(
        shape.lines.len() <= 1,
        "Text auf Pfad mit `path_content` setzen (Zeilen verbunden)"
    );
    let (length, closed) = (walk.length, walk.closed);
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
    // Umgekehrte Leserichtung: dieselbe Bogenlänge von der anderen Seite gemessen, die
    // Leserichtung entgegen; links davon ist dann die andere Seite des Pfades.
    let frames: Vec<(DVec2, DVec2)> = mids
        .iter()
        .map(|&s| match (flip_h, closed) {
            (false, _) => walk.at(s),
            (true, true) => walk.at(-s).map(|(p, u)| (p, -u)),
            (true, false) => walk.at(length - s).map(|(p, u)| (p, -u)),
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
        let walk = Walk::new(g.clone(), closed, round)?;
        place_path(
            &square(),
            5.0,
            &walk,
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

    /// Die gemerkte Längentabelle legt den Text bitgleich wie eine frisch gebaute; ein bewegter
    /// Pfad mit gleich vielen Knoten baut sie neu.
    #[test]
    fn cached_walk_is_bit_identical_and_rebuilt_when_the_path_moves() {
        use crate::model::Sketch;
        use crate::text::TextParams;
        use crate::text_shape::TextStyle;
        use planora_math::{Frame, Tolerance};
        use std::sync::Arc;

        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let pts: Vec<_> = [
            (0.0, 0.0),
            (10.0, 8.0),
            (20.0, -4.0),
            (30.0, 6.0),
            (40.0, 0.0),
        ]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
        let path = s.add_control_spline(&pts, 3).unwrap();
        let shape = Arc::new(square());
        let params = TextParams {
            content: "x".into(),
            style: TextStyle {
                font: "Test".into(),
                bold: false,
                italic: false,
                spacing: 0.0,
            },
            height: 5.0,
            flip_h: false,
            flip_v: false,
        };
        let t = s
            .add_text_path(path, params, true, false, HAlign::Center, shape.clone())
            .unwrap();
        let fresh_place = |s: &Sketch| {
            let walk = Walk::new(s.geom(path).unwrap(), false, false).unwrap();
            let placed = place_path(
                &shape,
                5.0,
                &walk,
                true,
                false,
                HAlign::Center,
                false,
                false,
            )
            .unwrap();
            (walk, placed)
        };
        let starts = |s: &Sketch| -> Vec<u64> {
            let d = &s.texts[t];
            d.points
                .iter()
                .flat_map(|&p| {
                    let q = s.pos(p);
                    [q.x.to_bits(), q.y.to_bits()]
                })
                .collect()
        };
        s.solve();
        let cached = s.texts[t].walk.0.clone().expect("Tabelle gemerkt");
        let (fresh, placed) = fresh_place(&s);
        assert_eq!(cached.table, fresh.table);
        assert_eq!(cached.length.to_bits(), fresh.length.to_bits());
        let again = place_path(
            &shape,
            5.0,
            &cached,
            true,
            false,
            HAlign::Center,
            false,
            false,
        )
        .unwrap();
        assert_eq!(again, placed, "bitgleich mit frischer Tabelle");
        let want = [placed[0].start.x.to_bits(), placed[0].start.y.to_bits()];
        assert_eq!(
            starts(&s)[..2],
            want,
            "die Skizze steht an der frischen Lage"
        );
        // Gleicher Pfad: dieselbe Tabelle bleibt.
        let before = starts(&s);
        s.solve();
        assert!(Arc::ptr_eq(&cached, s.texts[t].walk.0.as_ref().unwrap()));
        assert_eq!(starts(&s), before);

        // Kontrollpunkt bewegt: gleich viele Knoten, andere Geometrie, neue Tabelle.
        s.set_point_xy(pts[2], 20.0, 3.0).unwrap();
        s.solve();
        let moved = s.texts[t].walk.0.clone().unwrap();
        let geom = s.geom(path).unwrap();
        assert!(!Arc::ptr_eq(&cached, &moved));
        assert!(!cached.fits(&geom, false, false));
        assert!(moved.fits(&geom, false, false));
        // Die Zahl der Abschnitte folgt der Krümmung, die Knoten bleiben dieselben.
        assert_ne!(cached.table, moved.table);
        let (fresh, placed) = fresh_place(&s);
        assert_eq!(moved.table, fresh.table);
        let want = [placed[0].start.x.to_bits(), placed[0].start.y.to_bits()];
        assert_eq!(starts(&s)[..2], want, "der Text folgt dem bewegten Pfad");
    }
}
