//! Nachgebildete Schnitte: Kursiv durch Scheren, Fett durch Verbreitern der Umrisse.

use planora_math::DVec2;
use planora_sketch::text_shape::{Contour, Seg};

/// Scherung für nachgebildetes Kursiv: `tan 12° ≈ 0,2126`.
pub(crate) const ITALIC_SHEAR: f64 = 0.2126;
/// Verbreitern je Seite für nachgebildetes Fett, in Einheiten der Höhe (2 % der Höhe).
pub(crate) const BOLD_OFFSET: f64 = 0.02;

/// Grenze für FreeType: Nur Ecken, die weniger als etwa 160° umkehren, werden verschoben
/// (`d > -0xF000` in 16.16, also Skalarprodukt der Richtungen über −0,9375).
const MAX_TURN_DOT: f64 = -0.9375;
/// Kürzere Wege gelten als Punkt (doppelter Punkt).
const SAME_POINT: f64 = 1e-12;

/// Verschiebt alle Punkte.
pub(crate) fn translate(contours: &mut [Contour], by: DVec2) {
    for_each_point(contours, |p| *p += by);
}

/// Schert um `k` um die Grundlinie der Zeile: `x += k · (y + dy)`. `dy` ist die Höhe des
/// Glyphen-Ursprungs in der Zeile, damit die ganze Zeile eine einzige Scherung ist.
pub(crate) fn shear(contours: &mut [Contour], k: f64, dy: f64) {
    for_each_point(contours, |p| p.x += k * (p.y + dy));
}

/// Verbreitert die Umrisse um `strength` je Seite nach dem Verfahren von FreeType
/// (`FT_Outline_EmboldenXY`): Jeder Punkt des Kontrollpolygons wandert entlang der
/// gemittelten Normalen seiner beiden Nachbarkanten nach außen, so weit, dass beide Kanten um
/// `strength` versetzt sind. Kurze Kanten an spitzen Innenecken begrenzen den Weg, sehr spitze
/// Ecken (Umkehr über etwa 160°) bleiben stehen. Außen und Innen folgen der Drehrichtung der
/// ganzen Glyphe, Löcher werden also kleiner. Anders als FreeType wird nicht nach rechts oben
/// verschoben; die Umrisse wachsen nach allen Seiten gleich.
pub(crate) fn embolden(contours: &mut [Contour], strength: f64) {
    let area: f64 = contours.iter().map(polygon_area).sum();
    if area.abs() <= SAME_POINT || strength == 0.0 {
        // Keine Drehrichtung (FreeType: `FT_ORIENTATION_NONE`): unverändert.
        return;
    }
    // +1: gegen den Uhrzeigersinn (PostScript), −1: im Uhrzeigersinn (TrueType).
    let sign = area.signum();
    for contour in contours.iter_mut() {
        let pts = polygon(contour);
        let n = pts.len();
        let moved: Vec<DVec2> = (0..n)
            .map(|i| pts[i] + corner_shift(&pts, i, sign, strength))
            .collect();
        write_polygon(contour, &moved);
    }
}

/// Weg eines Punkts beim Verbreitern (FreeType-Formel ohne die Verschiebung um `strength`).
fn corner_shift(pts: &[DVec2], i: usize, sign: f64, strength: f64) -> DVec2 {
    let n = pts.len();
    let here = pts[i];
    // Nächster verschiedener Punkt vorher und nachher; doppelte Punkte wandern mit.
    let prev = (1..n)
        .map(|k| pts[(i + n - k) % n])
        .find(|p| p.distance(here) > SAME_POINT);
    let next = (1..n)
        .map(|k| pts[(i + k) % n])
        .find(|p| p.distance(here) > SAME_POINT);
    let (Some(prev), Some(next)) = (prev, next) else {
        return DVec2::ZERO;
    };
    let (l_in, l_out) = (here.distance(prev), next.distance(here));
    let din = (here - prev) / l_in;
    let dout = (next - here) / l_out;
    let dot = din.dot(dout);
    if dot <= MAX_TURN_DOT {
        return DVec2::ZERO;
    }
    let d = 1.0 + dot;
    // Normale nach außen: rechts der Laufrichtung bei Drehung gegen den Uhrzeigersinn.
    let outward = |u: DVec2| DVec2::new(u.y, -u.x) * sign;
    let sum = outward(din) + outward(dout);
    // Positiv an Innenecken; dort begrenzt die kürzere Kante den Weg.
    let q = -sign * din.perp_dot(dout);
    let l = l_in.min(l_out);
    let factor = if strength * q <= l * d {
        strength / d
    } else {
        l / q
    };
    sum * factor
}

/// Kontrollpolygon einer geschlossenen Kontur ohne den doppelten Endpunkt.
fn polygon(contour: &Contour) -> Vec<DVec2> {
    let mut pts = vec![contour.start];
    for seg in &contour.segs {
        match *seg {
            Seg::Line(p) => pts.push(p),
            Seg::Quad(c, p) => pts.extend([c, p]),
            Seg::Cubic(c1, c2, p) => pts.extend([c1, c2, p]),
        }
    }
    // Das letzte Ende ist der Anfang.
    pts.pop();
    pts
}

/// Schreibt das Polygon in derselben Reihenfolge zurück; das letzte Ende ist wieder der Anfang.
fn write_polygon(contour: &mut Contour, pts: &[DVec2]) {
    let Some(&start) = pts.first() else {
        return;
    };
    contour.start = start;
    let mut k = 1;
    let mut next = || {
        let p = pts.get(k).copied().unwrap_or(start);
        k += 1;
        p
    };
    for seg in &mut contour.segs {
        *seg = match *seg {
            Seg::Line(_) => Seg::Line(next()),
            Seg::Quad(..) => {
                let c = next();
                Seg::Quad(c, next())
            }
            Seg::Cubic(..) => {
                let c1 = next();
                let c2 = next();
                Seg::Cubic(c1, c2, next())
            }
        };
    }
}

/// Fläche des Kontrollpolygons mit Vorzeichen (positiv gegen den Uhrzeigersinn).
fn polygon_area(contour: &Contour) -> f64 {
    let pts = polygon(contour);
    let n = pts.len();
    (0..n)
        .map(|i| pts[i].perp_dot(pts[(i + 1) % n]))
        .sum::<f64>()
        * 0.5
}

fn for_each_point(contours: &mut [Contour], mut f: impl FnMut(&mut DVec2)) {
    for contour in contours {
        f(&mut contour.start);
        for seg in &mut contour.segs {
            match seg {
                Seg::Line(p) => f(p),
                Seg::Quad(c, p) => {
                    f(c);
                    f(p);
                }
                Seg::Cubic(c1, c2, p) => {
                    f(c1);
                    f(c2);
                    f(p);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, x1: f64, y1: f64, ccw: bool) -> Contour {
        let mut corners = [
            DVec2::new(x0, y0),
            DVec2::new(x1, y0),
            DVec2::new(x1, y1),
            DVec2::new(x0, y1),
        ];
        if !ccw {
            corners.reverse();
        }
        let start = corners[0];
        let mut segs: Vec<Seg> = corners[1..].iter().map(|p| Seg::Line(*p)).collect();
        segs.push(Seg::Line(start));
        Contour { start, segs }
    }

    fn bounds(contour: &Contour) -> (DVec2, DVec2) {
        let pts = polygon(contour);
        let min = pts
            .iter()
            .fold(DVec2::splat(f64::INFINITY), |a, p| a.min(*p));
        let max = pts
            .iter()
            .fold(DVec2::splat(f64::NEG_INFINITY), |a, p| a.max(*p));
        (min, max)
    }

    #[test]
    fn embolden_grows_outer_and_shrinks_hole_in_both_orientations() {
        for ccw in [true, false] {
            // Außen in der einen Richtung, das Loch in der anderen, wie in jeder Schrift.
            let mut ring = [
                square(0.0, 0.0, 1.0, 1.0, ccw),
                square(0.3, 0.3, 0.7, 0.7, !ccw),
            ];
            embolden(&mut ring, 0.05);
            let (min, max) = bounds(&ring[0]);
            assert!(
                (min - DVec2::splat(-0.05)).length() < 1e-12,
                "{ccw}: {min:?}"
            );
            assert!(
                (max - DVec2::splat(1.05)).length() < 1e-12,
                "{ccw}: {max:?}"
            );
            let (min, max) = bounds(&ring[1]);
            assert!(
                (min - DVec2::splat(0.35)).length() < 1e-12,
                "{ccw}: {min:?}"
            );
            assert!(
                (max - DVec2::splat(0.65)).length() < 1e-12,
                "{ccw}: {max:?}"
            );
            // Geschlossen wie vorher.
            for contour in &ring {
                assert_eq!(contour.segs.last().unwrap().end(), contour.start);
            }
        }
    }

    #[test]
    fn embolden_moves_duplicate_points_together() {
        let mut contour = square(0.0, 0.0, 1.0, 1.0, true);
        contour.segs.insert(0, Seg::Line(DVec2::ZERO));
        let mut list = [contour];
        embolden(&mut list, 0.1);
        let pts = polygon(&list[0]);
        assert_eq!(pts[0], pts[1]);
        assert!((pts[0] - DVec2::new(-0.1, -0.1)).length() < 1e-12);
    }

    #[test]
    fn shear_uses_line_baseline() {
        let mut list = [square(0.0, 0.0, 1.0, 1.0, true)];
        shear(&mut list, ITALIC_SHEAR, 0.5);
        let pts = polygon(&list[0]);
        assert!((pts[0].x - ITALIC_SHEAR * 0.5).abs() < 1e-15);
        assert!((pts[2].x - (1.0 + ITALIC_SHEAR * 1.5)).abs() < 1e-15);
    }
}
