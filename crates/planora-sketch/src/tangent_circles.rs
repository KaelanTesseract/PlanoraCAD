//! Tangentialkreise: zwei Linien mit Zeiger, drei Kurven (Linie, Kreis, Bogen) mit Zeiger.
//!
//! Reine Funktionen und durchweg analytisch: Die Skizze bleibt unverändert, gleiche Eingabe gibt
//! bitgleich dasselbe; so rechnen Vorschau und Klick dasselbe.
//!
//! Eine Linie gilt als unbegrenzt, ein Bogen als ganzer Kreis, genau wie bei `Tangent`.

use crate::model::{CurveId, CurveKind, MAX_COORD, Sketch};
use planora_math::{DVec2, Tolerance};

/// Anteil von `tol.linear`, bis zu dem eine Lösung die Kurven treffen muss; der Solver rechnet
/// erst darüber und schöbe sonst freie Kurven in der echten Skizze nach.
const HELD_RESIDUAL: f64 = 1e-2;
/// Betrag der Determinante der drei Berührgleichungen (Zeilen etwa Länge 1), bis zu dem eine
/// Lösung als nicht bestimmt gilt.
const MIN_DETERMINANT: f64 = 1e-7;

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
    nearest_to_pointer(&all_solutions(&shapes, tol), pointer, tol.linear)
}

/// Alle Kreise an den drei Kurven, nach Art des Apollonios: je Kurve ein Vorzeichen `s`.
/// Eine Linie gibt `n·m − d = s·r`, ein Kreis `|m − o|² = (r + s·ρ)²`. Mit `q = |m|² − r²` sind das
/// drei lineare Gleichungen in `(x, y, r, q)`, ihre Lösungen eine Gerade; darauf ergibt
/// `q = |m|² − r²` eine quadratische Gleichung. Gerechnet wird um die Mitte der Kurven und in
/// deren Größe, damit Lage und Maßstab die Genauigkeit nicht drücken.
fn all_solutions(shapes: &[Shape; 3], tol: &Tolerance) -> Vec<(DVec2, f64)> {
    let (origin, scale) = frame_of(shapes);
    let mut out = Vec::new();
    for mask in 0..8u8 {
        let signs = [0, 1, 2].map(|i| if mask & (1 << i) == 0 { 1.0 } else { -1.0 });
        let rows = [0, 1, 2].map(|i| equation(&shapes[i], signs[i], origin, scale));
        let Some((point, dir)) = solution_line(rows, tol.angular) else {
            continue;
        };
        let at = |t: f64| std::array::from_fn::<f64, 4, _>(|k| point[k] + t * dir[k]);
        // |m|² − r² − q = 0 entlang der Geraden.
        let form = |u: &[f64; 4], w: &[f64; 4]| u[0] * w[0] + u[1] * w[1] - u[2] * w[2];
        let a = form(&dir, &dir);
        let b = 2.0 * form(&point, &dir) - dir[3];
        let c = form(&point, &point) - point[3];
        for t in quadratic_roots(a, b, c) {
            let [x, y, r, _] = at(t);
            let (center, radius) = (origin + DVec2::new(x, y) * scale, r * scale);
            if touches_all(shapes, signs, center, radius, tol.linear) {
                push_unique(&mut out, center, radius, tol.linear);
            }
        }
    }
    out
}

/// Bezugspunkt und Maßstab: Schwerpunkt der Kurven, größte vorkommende Länge.
fn frame_of(shapes: &[Shape; 3]) -> (DVec2, f64) {
    let reference = |s: &Shape| match *s {
        Shape::Line { a, b, .. } => (a + b) / 2.0,
        Shape::Round { center, .. } => center,
    };
    let origin = shapes.iter().map(reference).sum::<DVec2>() / 3.0;
    let scale = shapes
        .iter()
        .map(|s| match *s {
            Shape::Line { a, normal, .. } => normal.dot(a - origin).abs(),
            Shape::Round { center, radius } => center.distance(origin).max(radius),
        })
        .fold(0.0, f64::max);
    (origin, if scale > 0.0 { scale } else { 1.0 })
}

/// Zeile `[x, y, r, q, rechte Seite]` im Rahmen aus [`frame_of`], Koeffizienten auf Länge 1.
fn equation(shape: &Shape, s: f64, origin: DVec2, scale: f64) -> [f64; 5] {
    let row = match *shape {
        Shape::Line { a, normal, .. } => {
            [normal.x, normal.y, -s, 0.0, normal.dot(a - origin) / scale]
        }
        Shape::Round { center, radius } => {
            let (m, rho) = ((center - origin) / scale, radius / scale);
            [
                -2.0 * m.x,
                -2.0 * m.y,
                -2.0 * s * rho,
                1.0,
                rho * rho - m.dot(m),
            ]
        }
    };
    let len = row[..4].iter().map(|v| v * v).sum::<f64>().sqrt();
    row.map(|v| v / len)
}

/// Lösungsgerade `Punkt + t·Richtung` von drei Gleichungen in vier Unbekannten, Gauß-Jordan mit
/// voller Pivotsuche. `None` bei Rang unter 3 (Pivot ≤ `min_pivot`): Dann gibt es keine oder keine
/// bestimmte Lösung, etwa bei parallelen Linien mit gleichem Vorzeichen oder gleichen Kreisen.
fn solution_line(mut m: [[f64; 5]; 3], min_pivot: f64) -> Option<([f64; 4], [f64; 4])> {
    let mut columns = [0, 1, 2, 3];
    for k in 0..3 {
        let (mut row, mut col, mut best) = (k, k, -1.0);
        for (i, line) in m.iter().enumerate().skip(k) {
            for (j, &c) in columns.iter().enumerate().skip(k) {
                if line[c].abs() > best {
                    (row, col, best) = (i, j, line[c].abs());
                }
            }
        }
        if best <= min_pivot {
            return None;
        }
        m.swap(k, row);
        columns.swap(k, col);
        let pivot_line = m[k].map(|v| v / m[k][columns[k]]);
        m[k] = pivot_line;
        for (i, line) in m.iter_mut().enumerate() {
            if i != k {
                let f = line[columns[k]];
                for (v, p) in line.iter_mut().zip(pivot_line) {
                    *v -= f * p;
                }
            }
        }
    }
    let free = columns[3];
    let (mut point, mut dir) = ([0.0; 4], [0.0; 4]);
    dir[free] = 1.0;
    for k in 0..3 {
        point[columns[k]] = m[k][4];
        dir[columns[k]] = -m[k][free];
    }
    Some((point, dir))
}

/// Nullstellen von `a·t² + b·t + c`, in der stabilen Form; nicht endliche Werte bei `a = 0` oder
/// doppelter Null fallen später in [`touches_all`] heraus.
fn quadratic_roots(a: f64, b: f64, c: f64) -> Vec<f64> {
    let disc = b * b - 4.0 * a * c;
    if disc.is_nan() || disc < 0.0 {
        return Vec::new();
    }
    let w = -(b + disc.sqrt().copysign(b)) / 2.0;
    vec![w / a, c / w]
}

/// Berührt der Kreis alle drei Kurven mit diesen Vorzeichen so genau, dass der Solver der echten
/// Skizze nichts mehr zu tun hat, und ist er dabei bestimmt? Eine vorgegebene Kurve selbst zählt
/// nicht, auch keine Lösung, an der die Gleichungen abhängig werden (doppelte Nullstelle): Der
/// Solver sähe dort eine überzählige Abhängigkeit.
fn touches_all(shapes: &[Shape; 3], signs: [f64; 3], center: DVec2, radius: f64, tol: f64) -> bool {
    if !acceptable(&[], center, radius, tol) {
        return false;
    }
    let mut gradient = [[0.0; 3]; 3];
    for (i, shape) in shapes.iter().enumerate() {
        let s = signs[i];
        let residual = match *shape {
            Shape::Line { a, normal, .. } => {
                gradient[i] = [normal.x, normal.y, -s];
                normal.dot(center - a) - s * radius
            }
            Shape::Round {
                center: o,
                radius: rho,
            } => {
                let to = center - o;
                let dist = to.length();
                if dist <= tol {
                    return false;
                }
                let reach = radius + s * rho;
                gradient[i] = [to.x / dist, to.y / dist, -reach.signum()];
                dist - reach.abs()
            }
        };
        if residual.abs() > HELD_RESIDUAL * tol {
            return false;
        }
    }
    let [u, v, w] = gradient;
    let det = u[0] * (v[1] * w[2] - v[2] * w[1]) - u[1] * (v[0] * w[2] - v[2] * w[0])
        + u[2] * (v[0] * w[1] - v[1] * w[0]);
    det.abs() > MIN_DETERMINANT
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
