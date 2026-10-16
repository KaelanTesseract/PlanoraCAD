//! Eigenes Prüfwerkzeug für Markierungen (Maße, Mittellinien): Hüllbox, Pfeile, Linien durch
//! einen Punkt, Abstände. Unabhängig von der Bibliothek geschrieben (nur die Datentypen werden
//! gelesen); Pfeilmaße stehen hier als eigene Zahlen.

use planora_drawing::annotate::{Mark, MarkKind, TextAnchor};
use planora_math::DVec2;

/// Länge eines Pfeils in Blatt-mm (eigene Zahl, nicht die Konstante der Bibliothek).
pub const ARROW_LEN_MM: f64 = 3.5;

pub fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

pub fn close(a: DVec2, b: DVec2, tol: f64) -> bool {
    (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol
}

/// Abstand eines Punkts von der Strecke `a`–`b`.
pub fn dist_point_segment(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    let (qx, qy) = (a.x + t * dx, a.y + t * dy);
    ((p.x - qx).powi(2) + (p.y - qy).powi(2)).sqrt()
}

/// Abstand eines Punkts von der unbegrenzten Geraden durch `a` und `b`.
pub fn dist_point_line(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / len
}

/// Alle Linien `(a, b, Art)`.
pub fn lines(marks: &[Mark]) -> Vec<(DVec2, DVec2, MarkKind)> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Line { a, b, kind } => Some((*a, *b, *kind)),
            _ => None,
        })
        .collect()
}

/// Alle Pfeile `(Spitze, Richtung, Art)`.
pub fn find_arrows(marks: &[Mark]) -> Vec<(DVec2, DVec2, MarkKind)> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Arrow { tip, dir, kind } => Some((*tip, *dir, *kind)),
            _ => None,
        })
        .collect()
}

/// Ein Text als einfache Zeile.
#[derive(Clone, Debug)]
pub struct T {
    pub pos: DVec2,
    pub height: f64,
    pub angle: f64,
    pub anchor: TextAnchor,
    pub text: String,
    pub kind: MarkKind,
}

pub fn texts(marks: &[Mark]) -> Vec<T> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Text {
                pos,
                height,
                angle,
                anchor,
                text,
                kind,
            } => Some(T {
                pos: *pos,
                height: *height,
                angle: *angle,
                anchor: *anchor,
                text: text.clone(),
                kind: *kind,
            }),
            _ => None,
        })
        .collect()
}

/// Alle Bögen `(Mitte, Radius, Anfang, Ende, Art)`.
pub fn arcs(marks: &[Mark]) -> Vec<(DVec2, f64, f64, f64, MarkKind)> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Arc {
                center,
                r,
                start,
                end,
                kind,
            } => Some((*center, *r, *start, *end, *kind)),
            _ => None,
        })
        .collect()
}

/// Linien, die bis auf `tol` durch den Punkt `p` gehen.
pub fn line_through(marks: &[Mark], p: DVec2, tol: f64) -> Vec<(DVec2, DVec2)> {
    lines(marks)
        .into_iter()
        .filter(|(a, b, _)| dist_point_segment(p, *a, *b) <= tol)
        .map(|(a, b, _)| (a, b))
        .collect()
}

/// Gibt es eine Linie `a`–`b` (Richtung egal)?
pub fn has_line(marks: &[Mark], a: DVec2, b: DVec2, tol: f64) -> bool {
    lines(marks).iter().any(|(p, q, _)| {
        (close(*p, a, tol) && close(*q, b, tol)) || (close(*p, b, tol) && close(*q, a, tol))
    })
}

/// Gibt es einen Pfeil mit dieser Spitze und Richtung?
pub fn has_arrow(marks: &[Mark], tip: DVec2, dir: DVec2, tol: f64) -> bool {
    find_arrows(marks)
        .iter()
        .any(|(t, d, _)| close(*t, tip, tol) && close(*d, dir, tol))
}

/// Hüllbox aller Markierungen: Linienenden, Pfeilspitze und -ende, Bogen (Stützstellen) und
/// Textanker.
pub fn bbox_of_marks(marks: &[Mark]) -> (DVec2, DVec2) {
    let mut lo = v(f64::INFINITY, f64::INFINITY);
    let mut hi = v(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut add = |p: DVec2| {
        lo = v(lo.x.min(p.x), lo.y.min(p.y));
        hi = v(hi.x.max(p.x), hi.y.max(p.y));
    };
    for m in marks {
        match m {
            Mark::Line { a, b, .. } => {
                add(*a);
                add(*b);
            }
            Mark::Arrow { tip, dir, .. } => {
                add(*tip);
                add(v(
                    tip.x + dir.x * ARROW_LEN_MM,
                    tip.y + dir.y * ARROW_LEN_MM,
                ));
            }
            Mark::Arc {
                center,
                r,
                start,
                end,
                ..
            } => {
                for k in 0..=64 {
                    let t = start + (end - start) * k as f64 / 64.0;
                    add(v(center.x + r * t.cos(), center.y + r * t.sin()));
                }
            }
            Mark::Text { pos, .. } => add(*pos),
        }
    }
    (lo, hi)
}

/// Alle Zahlen einer Markierung endlich?
pub fn all_finite(marks: &[Mark]) -> bool {
    marks.iter().all(|m| match m {
        Mark::Line { a, b, .. } => a.is_finite() && b.is_finite(),
        Mark::Arrow { tip, dir, .. } => tip.is_finite() && dir.is_finite(),
        Mark::Arc {
            center,
            r,
            start,
            end,
            ..
        } => center.is_finite() && r.is_finite() && start.is_finite() && end.is_finite(),
        Mark::Text {
            pos, height, angle, ..
        } => pos.is_finite() && height.is_finite() && angle.is_finite(),
    })
}

/// Art jeder Markierung.
pub fn kind_of(m: &Mark) -> MarkKind {
    match m {
        Mark::Line { kind, .. }
        | Mark::Arrow { kind, .. }
        | Mark::Arc { kind, .. }
        | Mark::Text { kind, .. } => *kind,
    }
}

/// Prüft die Linien als ungeordnete Menge ungerichteter Strecken: gleich viele, jede erwartete
/// genau einmal vorhanden.
pub fn assert_lines(marks: &[Mark], expected: &[(DVec2, DVec2)], tol: f64) {
    let got = lines(marks);
    assert_eq!(
        got.len(),
        expected.len(),
        "Zahl der Linien: {got:?} statt {expected:?}"
    );
    for (a, b) in expected {
        let n = got
            .iter()
            .filter(|(p, q, _)| {
                (close(*p, *a, tol) && close(*q, *b, tol))
                    || (close(*p, *b, tol) && close(*q, *a, tol))
            })
            .count();
        assert_eq!(n, 1, "Linie {a:?}–{b:?} nicht genau einmal in {got:?}");
    }
}

/// Prüft die Pfeile als ungeordnete Menge `(Spitze, Richtung)`.
pub fn assert_arrows(marks: &[Mark], expected: &[(DVec2, DVec2)], tol: f64) {
    let got = find_arrows(marks);
    assert_eq!(
        got.len(),
        expected.len(),
        "Zahl der Pfeile: {got:?} statt {expected:?}"
    );
    for (tip, dir) in expected {
        assert!(
            has_arrow(marks, *tip, *dir, tol),
            "Pfeil {tip:?} → {dir:?} fehlt in {got:?}"
        );
    }
}
