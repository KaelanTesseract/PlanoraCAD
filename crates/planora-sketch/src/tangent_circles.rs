//! Tangentialkreise: zwei Linien mit Zeiger, drei Kurven (Linie, Kreis, Bogen) mit Zeiger.
//!
//! Reine Funktionen: Die Skizze bleibt unverändert, gleiche Eingabe gibt bitgleich dasselbe; so
//! rechnen Vorschau und Klick dasselbe. Nur Linien rechnen analytisch, alles andere der Solver in
//! einer Wegwerfskizze mit festen Kopien der Kurven, damit nichts Vorhandenes rutscht.
//!
//! Eine Linie gilt als unbegrenzt, ein Bogen als ganzer Kreis, genau wie bei `Tangent`.

use crate::constraint::ConstraintKind;
use crate::model::{CurveId, CurveKind, MAX_COORD, Sketch};
use planora_math::{DVec2, Frame, Tolerance};

/// Untere Grenze des Startradius im Solver (mm).
const MIN_START_RADIUS: f64 = 1e-3;
/// Die Wegwerfskizze rechnet um diesen Faktor strenger als die echte Skizze. Der Solver hält auch
/// `Fixed` und `Radius` nur im Rahmen seiner Residuengrenze, die Kopien dürfen also ein wenig
/// wandern. Bei gleicher Toleranz läge das Ergebnis an den echten Kurven knapp über der Grenze
/// (gemessen 1,06e-8 mm bei 1e-8), und `solve` in der echten Skizze schöbe alle freien Kurven nach.
const THROWAWAY_TOLERANCE: f64 = 1e-2;

/// Kurve, die ein Tangentialkreis berühren kann.
#[derive(Clone, Copy)]
enum Shape {
    /// `normal`: Einheitsvektor links der Richtung `a → b`.
    Line {
        a: DVec2,
        b: DVec2,
        normal: DVec2,
    },
    Round {
        center: DVec2,
        radius: f64,
    },
}

impl Shape {
    /// Vorzeichenbehafteter Abstand von der unbegrenzten Linie; für runde Kurven `None`.
    fn signed(&self, p: DVec2) -> Option<f64> {
        match *self {
            Shape::Line { a, normal, .. } => Some(normal.dot(p - a)),
            Shape::Round { .. } => None,
        }
    }

    /// Abstand von der unbegrenzten Linie bzw. vom ganzen Kreis.
    fn distance(&self, p: DVec2) -> f64 {
        match *self {
            Shape::Line { a, normal, .. } => normal.dot(p - a).abs(),
            Shape::Round { center, radius } => (center.distance(p) - radius).abs(),
        }
    }
}

fn shape_of(sk: &Sketch, c: CurveId) -> Option<Shape> {
    let tol = sk.tolerance().linear;
    match sk.curve(c)?.kind {
        CurveKind::Line { a, b } => {
            let (a, b) = (sk.pos(a), sk.pos(b));
            let d = b - a;
            let len = d.length();
            (len > tol).then(|| Shape::Line {
                a,
                b,
                normal: d.perp() / len,
            })
        }
        CurveKind::Circle { center, .. } | CurveKind::Arc { center, .. } => {
            let radius = sk.radius(c)?;
            (radius > tol).then(|| Shape::Round {
                center: sk.pos(center),
                radius,
            })
        }
        _ => None,
    }
}

/// Kann ein Tangentialkreis diese Kurve berühren (lebende Linie, Kreis oder Bogen)?
pub fn is_tangent_candidate(sk: &Sketch, c: CurveId) -> bool {
    sk.curve(c).is_some_and(|d| {
        matches!(
            d.kind,
            CurveKind::Line { .. } | CurveKind::Circle { .. } | CurveKind::Arc { .. }
        )
    })
}

/// Endliche Lage im erlaubten Bereich, Radius über `tol`, und der Kreis berührt alle Linien.
fn acceptable(lines: &[Shape], center: DVec2, radius: f64, tol: f64) -> bool {
    center.is_finite()
        && radius.is_finite()
        && center.x.abs() <= MAX_COORD
        && center.y.abs() <= MAX_COORD
        && radius > tol
        && radius <= MAX_COORD
        && lines
            .iter()
            .all(|l| (l.distance(center) - radius).abs() <= tol)
}

/// Kreis an zwei Linien im Sektor des Zeigers (Vorzeichen zu beiden Linien): Die Mitte ist die
/// Projektion des Zeigers auf die Winkelhalbierende, ein gegebener Durchmesser setzt stattdessen
/// den Radius. Bei parallelen Linien ist der Radius der halbe Abstand und die Mitte liegt auf der
/// Mittellinie; ein abweichender Durchmesser wird abgelehnt. Zeiger auf einer der Linien: `None`.
pub fn tangent_circle_two_lines(
    sk: &Sketch,
    l0: CurveId,
    l1: CurveId,
    pointer: DVec2,
    diameter: Option<f64>,
) -> Option<(DVec2, f64)> {
    let tol = sk.tolerance();
    if l0 == l1 || !pointer.is_finite() {
        return None;
    }
    let (s0, s1) = (shape_of(sk, l0)?, shape_of(sk, l1)?);
    let (
        Shape::Line {
            a: a0, normal: n0, ..
        },
        Shape::Line {
            a: a1, normal: n1, ..
        },
    ) = (s0, s1)
    else {
        return None;
    };
    let (d0, d1) = (s0.signed(pointer)?, s1.signed(pointer)?);
    if d0.abs() <= tol.linear || d1.abs() <= tol.linear {
        return None;
    }
    let given = match diameter {
        Some(d) if d.is_finite() && d > 0.0 => Some(d / 2.0),
        Some(_) => return None,
        None => None,
    };
    let (center, radius) = if n0.perp_dot(n1).abs() <= tol.angular {
        let gap = n0.dot(a1 - a0);
        let half = gap.abs() / 2.0;
        if given.is_some_and(|r| (r - half).abs() > tol.linear) {
            return None;
        }
        (pointer - n0 * (d0 - gap / 2.0), half)
    } else {
        // Schnittpunkt der Geraden, Halbierende zeigt in den Sektor des Zeigers.
        let cross = n0.perp_dot(n1);
        let (o0, o1) = (n0.dot(a0), n1.dot(a1));
        let apex = DVec2::new(n1.y * o0 - n0.y * o1, n0.x * o1 - n1.x * o0) / cross;
        let toward = n0 * d0.signum() + n1 * d1.signum();
        let bisector = toward.normalize();
        // Abstand zu den Linien je Einheit entlang der Halbierenden.
        let rise = toward.length() / 2.0;
        match given {
            Some(r) => (apex + bisector * (r / rise), r),
            None => {
                let along = (pointer - apex).dot(bisector);
                (apex + bisector * along, along * rise)
            }
        }
    };
    acceptable(&[s0, s1], center, radius, tol.linear).then_some((center, radius))
}

/// Kreis an drei Kurven (Linie, Kreis, Bogen). Von allen Lösungen gilt die mit dem kleinsten
/// Abstand Zeiger–Kreislinie, bei Gleichstand (≤ `tol.linear`) die mit dem kleinsten Abstand
/// Zeiger–Mitte. `None`, wenn es keine gibt oder eine Kurve nicht taugt.
pub fn tangent_circle_three(
    sk: &Sketch,
    curves: [CurveId; 3],
    pointer: DVec2,
) -> Option<(DVec2, f64)> {
    let [c0, c1, c2] = curves;
    if c0 == c1 || c0 == c2 || c1 == c2 || !pointer.is_finite() {
        return None;
    }
    let shapes = [shape_of(sk, c0)?, shape_of(sk, c1)?, shape_of(sk, c2)?];
    let tol = sk.tolerance();
    let solutions = if shapes.iter().all(|s| matches!(s, Shape::Line { .. })) {
        three_lines(&shapes, tol)
    } else {
        through_solver(&shapes, pointer, tol)
    };
    nearest_to_pointer(&solutions, pointer, tol.linear)
}

/// Alle acht Vorzeichen der Gleichungen `sᵢ·(nᵢ·m − oᵢ) = r`; die Zeile 0 von den anderen
/// abgezogen bleibt ein 2×2-System für die Mitte.
fn three_lines(shapes: &[Shape; 3], tol: &Tolerance) -> Vec<(DVec2, f64)> {
    let row = |s: &Shape| match *s {
        Shape::Line { a, normal, .. } => (normal, normal.dot(a)),
        Shape::Round { .. } => unreachable!("only called with lines"),
    };
    let rows = [row(&shapes[0]), row(&shapes[1]), row(&shapes[2])];
    let mut out: Vec<(DVec2, f64)> = Vec::new();
    for mask in 0..8u8 {
        let sign = |i: u8| if mask & (1 << i) == 0 { 1.0 } else { -1.0 };
        let (s0, s1, s2) = (sign(0), sign(1), sign(2));
        let (n0, o0) = rows[0];
        let (n1, o1) = rows[1];
        let (n2, o2) = rows[2];
        let (u, w) = (n1 * s1 - n0 * s0, n2 * s2 - n0 * s0);
        let det = u.perp_dot(w);
        if det.abs() <= tol.angular {
            continue;
        }
        let (e1, e2) = (s1 * o1 - s0 * o0, s2 * o2 - s0 * o0);
        let center = DVec2::new(e1 * w.y - e2 * u.y, u.x * e2 - w.x * e1) / det;
        let radius = s0 * (n0.dot(center) - o0);
        if acceptable(shapes, center, radius, tol.linear) {
            push_unique(&mut out, center, radius, tol.linear);
        }
    }
    out
}

/// Löst aus zwei Startlagen: Mitte am Zeiger, Radius = kleinster und größter Abstand zu den Kurven.
fn through_solver(shapes: &[Shape; 3], pointer: DVec2, tol: &Tolerance) -> Vec<(DVec2, f64)> {
    let dist = shapes.map(|s| s.distance(pointer));
    let least = dist[0].min(dist[1]).min(dist[2]).max(MIN_START_RADIUS);
    let most = dist[0].max(dist[1]).max(dist[2]).max(MIN_START_RADIUS);
    let mut out = Vec::new();
    for start in [least, most] {
        if let Some((center, radius)) = solve_from(shapes, pointer, start, tol) {
            push_unique(&mut out, center, radius, tol.linear);
        }
    }
    out
}

/// Ein Lauf in der Wegwerfskizze. Zählt nur, wenn der Kreis voll bestimmt ist und nicht mit einer
/// der vorgegebenen Kurven zusammenfällt.
fn solve_from(
    shapes: &[Shape; 3],
    center: DVec2,
    radius: f64,
    tol: &Tolerance,
) -> Option<(DVec2, f64)> {
    let strict = Tolerance {
        linear: tol.linear * THROWAWAY_TOLERANCE,
        ..*tol
    };
    let mut t = Sketch::new(Frame::WORLD, "", strict);
    let mut copies = Vec::with_capacity(3);
    for s in shapes {
        let copy = match *s {
            Shape::Line { a, b, .. } => {
                let (pa, pb) = (t.add_point(a.x, a.y).ok()?, t.add_point(b.x, b.y).ok()?);
                t.add_constraint(ConstraintKind::Fixed(pa, a.x, a.y)).ok()?;
                t.add_constraint(ConstraintKind::Fixed(pb, b.x, b.y)).ok()?;
                t.add_line(pa, pb).ok()?
            }
            Shape::Round { center, radius } => {
                let m = t.add_point(center.x, center.y).ok()?;
                t.add_constraint(ConstraintKind::Fixed(m, center.x, center.y))
                    .ok()?;
                let c = t.add_circle(m, radius).ok()?;
                t.add_constraint(ConstraintKind::Radius(c, radius)).ok()?;
                c
            }
        };
        copies.push(copy);
    }
    let m = t.add_point(center.x, center.y).ok()?;
    let k = t.add_circle(m, radius).ok()?;
    for copy in copies {
        t.add_constraint(ConstraintKind::Tangent(copy, k)).ok()?;
    }
    let report = t.solve();
    if report.status != crate::solver::SolveStatus::Solved
        || report.dof != 0
        || !report.redundant.is_empty()
    {
        return None;
    }
    let (center, radius) = (t.point_xy(m).ok()?, t.radius(k)?);
    let is_given = shapes.iter().any(|s| match *s {
        Shape::Round {
            center: c,
            radius: r,
        } => c.distance(center) <= tol.linear && (r - radius).abs() <= tol.linear,
        Shape::Line { .. } => false,
    });
    (!is_given && acceptable(&[], center, radius, tol.linear)).then_some((center, radius))
}

fn push_unique(out: &mut Vec<(DVec2, f64)>, center: DVec2, radius: f64, tol: f64) {
    let known = out
        .iter()
        .any(|(c, r)| c.distance(center) <= tol && (r - radius).abs() <= tol);
    if !known {
        out.push((center, radius));
    }
}

fn nearest_to_pointer(
    solutions: &[(DVec2, f64)],
    pointer: DVec2,
    tol: f64,
) -> Option<(DVec2, f64)> {
    let rim = |&(c, r): &(DVec2, f64)| (c.distance(pointer) - r).abs();
    let best_rim = solutions.iter().map(rim).min_by(f64::total_cmp)?;
    solutions
        .iter()
        .filter(|s| rim(s) <= best_rim + tol)
        .min_by(|a, b| a.0.distance(pointer).total_cmp(&b.0.distance(pointer)))
        .copied()
}
