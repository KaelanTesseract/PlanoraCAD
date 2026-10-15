//! Mittellinien und Achsenkreuze einer Ansicht, aus dem Modell berechnet (Teilprojekt 12).
//!
//! Regeln (nur in Normansichten, deren Blickrichtung eine Weltachse ist; in der Isometrie stehen
//! Achsen und Kreise schräg, dort gibt es keine Mittellinien):
//! - Je **Kreiskante** (voller Kreis oder Bogen) mit `|Normale·d| ≥ 1 − 1e-9`: Achsenkreuz am
//!   projizierten Mittelpunkt, waagrecht und senkrecht, Halblänge = Radius.
//! - Je **Zylinder-/Kegelfläche** mit `|Achse·d| ≤ 1e-9`: eine Linie entlang der projizierten
//!   Achse zwischen den Enden der Fläche (Höhenbereich ihrer Randkanten längs der Achse).
//! - Je **Kugel**: Achsenkreuz am projizierten Mittelpunkt, Halblänge = Radius.
//! - Je **Torus** mit `|Achse·d| ≥ 1 − 1e-9`: Achsenkreuz, Halblänge = großer + kleiner Radius.
//!
//! Kreuze mit derselben Mitte (konzentrische Kreise, übereinanderliegende Kreise, Kugel und
//! Kreis) ergeben **ein** Kreuz mit der größten Halblänge; Achsenlinien auf derselben Geraden,
//! die sich überlappen oder berühren, werden zu einer Linie vereint. Toleranz:
//! `geom_tol(max(Ausdehnung, Lage))` – bei weit vom Ursprung entfernten Körpern bestimmt die
//! Rechengenauigkeit der Lage die kleinste unterscheidbare Strecke.
//!
//! Nähte (beide Seiten derselben Fläche) sind keine Kanten und liefern kein Kreuz. Linien,
//! Ellipsen und Freiformkurven liefern nichts. Kaputte Zahlen (NaN, unendlich, Radius ≤ 0)
//! werden übergangen, nie gezeichnet.
//!
//! Die Linien sind in Ansichtskoordinaten (mm, ungeskaliert) und **ohne** Überstand; den
//! Überstand in Blatt-mm addiert erst der Aufbau des Blatts.

use crate::DrawBody;
use crate::basis::Basis;
use crate::error::DrawError;
use crate::limits::{MAX_PAIR_TESTS, geom_tol};
use planora_exchange::Progress;
use planora_kernel::{Curve, Solid, Surface};
use planora_math::{DVec2, DVec3};

/// Grenze für „parallel zur Blickrichtung“: `|n·d| ≥ 1 − PARALLEL_EPS`.
const PARALLEL_EPS: f64 = 1e-9;
/// Grenze für „quer zur Blickrichtung“: `|a·d| ≤ PERP_EPS`.
const PERP_EPS: f64 = 1e-9;
/// Probepunkte je Randkante für den Höhenbereich einer Fläche (dazu die Enden und bei Kreisen
/// und Ellipsen die exakten Extremstellen).
const RANGE_SAMPLES: usize = 64;

/// Eine Mittellinie in Ansichtskoordinaten (mm, ungeskaliert), ohne Überstand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CenterLine {
    pub a: DVec2,
    pub b: DVec2,
    pub source: CenterSource,
}

/// Herkunft einer Mittellinie: Körperindex und Kanten- bzw. Flächenindex (in `solid.edges`
/// bzw. `solid.faces`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CenterSource {
    Circle { body: usize, edge: usize },
    Axis { body: usize, face: usize },
    Sphere { body: usize, face: usize },
    Torus { body: usize, face: usize },
}

/// Ein Achsenkreuz vor dem Zusammenfassen.
#[derive(Clone, Copy, Debug)]
struct Cross {
    c: DVec2,
    half: f64,
    source: CenterSource,
}

/// Eine Achsenlinie vor dem Zusammenfassen.
#[derive(Clone, Copy, Debug)]
struct AxisLine {
    a: DVec2,
    b: DVec2,
    source: CenterSource,
}

/// Mittellinien aller Körper für die Ansicht `basis` (siehe Modulbeschreibung). Reihenfolge:
/// erst die Kreuze (je zwei Linien, waagrecht und senkrecht), dann die Achsenlinien, jeweils in
/// der Reihenfolge ihres ersten Auftretens (Körper, Kanten, Flächen).
pub fn centerlines(
    bodies: &[DrawBody],
    basis: &Basis,
    extent: f64,
    progress: &Progress,
) -> Result<Vec<CenterLine>, DrawError> {
    progress.check()?;
    let d = basis.d;
    if !(d.is_finite() && extent.is_finite()) {
        return Err(DrawError::NonFinite);
    }
    // Nur Normansichten (Blick entlang einer Weltachse).
    if d.abs().max_element() < 1.0 - PARALLEL_EPS {
        return Ok(Vec::new());
    }
    let mut crosses = Vec::new();
    let mut axes = Vec::new();
    for (bi, body) in bodies.iter().enumerate() {
        collect_body(bi, body.solid, basis, &mut crosses, &mut axes, progress)?;
    }
    // Toleranz aus Ausdehnung und Lage (Rechengenauigkeit weit draußen).
    let mut reach: f64 = 0.0;
    for c in &crosses {
        reach = reach.max(c.c.abs().max_element() + c.half);
    }
    for l in &axes {
        reach = reach
            .max(l.a.abs().max_element())
            .max(l.b.abs().max_element());
    }
    let tol = geom_tol(extent.max(reach));
    crosses.retain(|c| c.half > tol);
    axes.retain(|l| l.a.distance(l.b) > tol);
    let pairs = std::cell::Cell::new(0u64);
    let mut out = Vec::new();
    for c in merge_crosses(crosses, tol, &pairs, progress)? {
        out.push(CenterLine {
            a: c.c - DVec2::new(c.half, 0.0),
            b: c.c + DVec2::new(c.half, 0.0),
            source: c.source,
        });
        out.push(CenterLine {
            a: c.c - DVec2::new(0.0, c.half),
            b: c.c + DVec2::new(0.0, c.half),
            source: c.source,
        });
    }
    for l in merge_axes(axes, tol, &pairs, progress)? {
        out.push(CenterLine {
            a: l.a,
            b: l.b,
            source: l.source,
        });
    }
    Ok(out)
}

/// Normierter Vektor oder `None` (Länge 0, NaN, unendlich).
fn unit(v: DVec3) -> Option<DVec3> {
    let l = v.length();
    (l.is_finite() && l > 0.0).then(|| v / l)
}

/// Positive, endliche Zahl?
fn positive(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

/// Kreuze und Achsenlinien eines Körpers.
fn collect_body(
    bi: usize,
    solid: &Solid,
    basis: &Basis,
    crosses: &mut Vec<Cross>,
    axes: &mut Vec<AxisLine>,
    progress: &Progress,
) -> Result<(), DrawError> {
    let d = basis.d;
    let uses = crate::stroke::edge_faces(solid);
    for (eid, e) in solid.edges.iter() {
        progress.check()?;
        let ei = eid.index();
        // Naht: alle Halbkanten in derselben Fläche.
        if let Some(f) = uses.get(ei)
            && f.iter().all(|&x| x == f[0])
        {
            continue;
        }
        let Curve::Circle { frame, radius } = &e.curve else {
            continue;
        };
        let Some(n) = unit(frame.z) else { continue };
        if !(positive(*radius) && frame.origin.is_finite()) {
            continue;
        }
        if n.dot(d).abs() >= 1.0 - PARALLEL_EPS {
            crosses.push(Cross {
                c: basis.project(frame.origin),
                half: *radius,
                source: CenterSource::Circle { body: bi, edge: ei },
            });
        }
    }
    for (fid, face) in solid.faces.iter() {
        progress.check()?;
        let fi = fid.index();
        match &face.surface {
            Surface::Cylinder { frame, radius } => {
                if positive(*radius) {
                    axis_line(bi, fi, solid, face, frame, basis, axes);
                }
            }
            Surface::Cone {
                frame,
                radius,
                slope,
            } => {
                if radius.is_finite() && slope.is_finite() {
                    axis_line(bi, fi, solid, face, frame, basis, axes);
                }
            }
            Surface::Sphere { frame, radius } => {
                if positive(*radius) && frame.origin.is_finite() {
                    crosses.push(Cross {
                        c: basis.project(frame.origin),
                        half: *radius,
                        source: CenterSource::Sphere { body: bi, face: fi },
                    });
                }
            }
            Surface::Torus {
                frame,
                major,
                minor,
            } => {
                let half = major + minor;
                if let Some(z) = unit(frame.z)
                    && positive(half)
                    && frame.origin.is_finite()
                    && z.dot(d).abs() >= 1.0 - PARALLEL_EPS
                {
                    crosses.push(Cross {
                        c: basis.project(frame.origin),
                        half,
                        source: CenterSource::Torus { body: bi, face: fi },
                    });
                }
            }
            Surface::Plane { .. } | Surface::Nurbs(_) => {}
        }
    }
    Ok(())
}

/// Achsenlinie einer Zylinder- oder Kegelfläche, wenn die Achse quer zur Blickrichtung steht:
/// zwischen dem kleinsten und größten Achsabstand `(p − origin)·z` der Randkanten der Fläche.
fn axis_line(
    bi: usize,
    fi: usize,
    solid: &Solid,
    face: &planora_kernel::Face,
    frame: &planora_math::Frame,
    basis: &Basis,
    axes: &mut Vec<AxisLine>,
) {
    let Some(z) = unit(frame.z) else { return };
    if !frame.origin.is_finite() || z.dot(basis.d).abs() > PERP_EPS {
        return;
    }
    let Some((lo, hi)) = axial_range(solid, face, frame.origin, z) else {
        return;
    };
    let a = basis.project(frame.origin + z * lo);
    let b = basis.project(frame.origin + z * hi);
    if a.is_finite() && b.is_finite() {
        axes.push(AxisLine {
            a,
            b,
            source: CenterSource::Axis { body: bi, face: fi },
        });
    }
}

/// Kleinster und größter Wert `(p − origin)·z` über die Randkanten der Fläche (Enden, Probe-
/// punkte, bei Kreisen und Ellipsen die exakten Extremstellen). `None` ohne endliche Werte.
/// Auch der Bereich einer Zylinderfläche als Maßbezug (`suggest`).
pub(crate) fn axial_range(
    solid: &Solid,
    face: &planora_kernel::Face,
    origin: DVec3,
    z: DVec3,
) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut add = |p: DVec3| {
        let s = (p - origin).dot(z);
        if s.is_finite() {
            lo = lo.min(s);
            hi = hi.max(s);
        }
    };
    for lp in std::iter::once(&face.outer).chain(face.inner.iter()) {
        for he in &lp.half_edges {
            let Some(e) = solid.edges.get(he.edge) else {
                continue;
            };
            let (t0, t1) = (e.t0.min(e.t1), e.t0.max(e.t1));
            if !(t0.is_finite() && t1.is_finite()) {
                continue;
            }
            match &e.curve {
                Curve::Line { .. } => {
                    add(e.curve.point(t0));
                    add(e.curve.point(t1));
                }
                c => {
                    for i in 0..=RANGE_SAMPLES {
                        add(c.point(t0 + (t1 - t0) * i as f64 / RANGE_SAMPLES as f64));
                    }
                    // Kreis/Ellipse: (p − o)·z = A cos t + B sin t + C, Extrema bei
                    // t = atan2(B, A) + k·π.
                    let conic = match c {
                        Curve::Circle { frame, radius } => {
                            Some((frame.x * *radius, frame.y * *radius))
                        }
                        Curve::Ellipse {
                            frame,
                            major,
                            minor,
                        } => Some((frame.x * *major, frame.y * *minor)),
                        _ => None,
                    };
                    if let Some((ax, ay)) = conic {
                        let t_star = ay.dot(z).atan2(ax.dot(z));
                        if t_star.is_finite() {
                            let pi = std::f64::consts::PI;
                            let k0 = ((t0 - t_star) / pi).ceil();
                            let k1 = ((t1 - t_star) / pi).floor();
                            // Höchstens wenige Umläufe; mehr wäre eine kaputte Kante.
                            if k0.is_finite() && k1.is_finite() && k1 - k0 <= 8.0 {
                                let mut k = k0;
                                while k <= k1 {
                                    add(c.point(t_star + k * pi));
                                    k += 1.0;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// Zählt Paartests gegen `MAX_PAIR_TESTS`.
fn count_pair(pairs: &std::cell::Cell<u64>) -> Result<(), DrawError> {
    let n = pairs.get() + 1;
    pairs.set(n);
    if n > MAX_PAIR_TESTS {
        return Err(DrawError::TooComplex(n));
    }
    Ok(())
}

/// Einfache Vereinigungs-Suche über Indizes.
fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    // Der kleinere Index bleibt Vertreter (Reihenfolge des ersten Auftretens).
    if ra < rb {
        parent[rb] = ra;
    } else if rb < ra {
        parent[ra] = rb;
    }
}

/// Kreuze mit derselben Mitte (Abstand je Koordinate ≤ `tol`) zu einem zusammenfassen:
/// Halblänge und Herkunft vom größten (bei Gleichstand vom ersten).
fn merge_crosses(
    crosses: Vec<Cross>,
    tol: f64,
    pairs: &std::cell::Cell<u64>,
    progress: &Progress,
) -> Result<Vec<Cross>, DrawError> {
    let n = crosses.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| crosses[i].c.x.total_cmp(&crosses[j].c.x));
    for (k, &i) in order.iter().enumerate() {
        progress.check()?;
        for &j in &order[k + 1..] {
            if crosses[j].c.x - crosses[i].c.x > tol {
                break;
            }
            count_pair(pairs)?;
            if (crosses[j].c.y - crosses[i].c.y).abs() <= tol {
                union(&mut parent, i, j);
            }
        }
    }
    let mut best: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        let r = find(&mut parent, i);
        match best[r] {
            Some(b) if crosses[b].half >= crosses[i].half => {}
            _ => best[r] = Some(i),
        }
    }
    Ok((0..n)
        .filter(|&i| find(&mut parent, i) == i)
        .map(|r| {
            let b = best[r].unwrap_or(r);
            Cross {
                c: crosses[r].c,
                half: crosses[b].half,
                source: crosses[b].source,
            }
        })
        .collect())
}

/// Achsenlinien auf derselben Geraden (beide Enden der einen höchstens `tol` neben der Geraden
/// der anderen), die sich überlappen oder berühren, zu einer vereinen: die äußersten Enden der
/// Gruppe, Herkunft der ersten.
fn merge_axes(
    axes: Vec<AxisLine>,
    tol: f64,
    pairs: &std::cell::Cell<u64>,
    progress: &Progress,
) -> Result<Vec<AxisLine>, DrawError> {
    let n = axes.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        progress.check()?;
        let (a, b) = (axes[i].a, axes[i].b);
        let len = a.distance(b);
        let u = (b - a) / len;
        for (j, other) in axes.iter().enumerate().skip(i + 1) {
            count_pair(pairs)?;
            let (p, q) = (other.a, other.b);
            let off = |x: DVec2| (x - a).perp_dot(u).abs();
            if off(p) > tol || off(q) > tol {
                continue;
            }
            let (s0, s1) = ((p - a).dot(u), (q - a).dot(u));
            if s0.max(s1) >= -tol && s0.min(s1) <= len + tol {
                union(&mut parent, i, j);
            }
        }
    }
    let mut out = Vec::new();
    for r in 0..n {
        if find(&mut parent, r) != r {
            continue;
        }
        let (a, b) = (axes[r].a, axes[r].b);
        let u = (b - a) / a.distance(b);
        // Äußerste Enden der Gruppe (ursprüngliche Punkte, keine neu berechneten).
        let (mut lo, mut lo_p) = (0.0, a);
        let (mut hi, mut hi_p) = (a.distance(b), b);
        for (j, other) in axes.iter().enumerate().skip(r) {
            if find(&mut parent, j) != r {
                continue;
            }
            for x in [other.a, other.b] {
                let s = (x - a).dot(u);
                if s < lo {
                    (lo, lo_p) = (s, x);
                }
                if s > hi {
                    (hi, hi_p) = (s, x);
                }
            }
        }
        out.push(AxisLine {
            a: lo_p,
            b: hi_p,
            source: axes[r].source,
        });
    }
    Ok(out)
}
