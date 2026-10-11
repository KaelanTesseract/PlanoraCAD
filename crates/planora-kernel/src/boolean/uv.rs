//! Randschleifen einer Fläche im Parameterraum `(u, v)` und Punkt-in-Fläche-Test.
//!
//! Konvention: Eine äußere Schleife läuft um die nach außen zeigende Normale gegen den
//! Uhrzeigersinn. Da die natürliche Normale `∂u × ∂v` ist, läuft sie bei `same_sense` auch in
//! `(u, v)` gegen den Uhrzeigersinn (sonst im Uhrzeigersinn); Löcher laufen jeweils andersherum.

use super::seg::{Seg, curve_sample_count};
use crate::geometry::{Curve, Surface, UvRect};
use crate::ids::Id;
use crate::topology::{Face, FaceTag, Loop, Solid};
use crate::validate::NURBS_EDGE_TOL;
use planora_math::{DVec2, DVec3, TAU, Tolerance, orient2d};

/// Geometrische Beschreibung einer (ggf. beschnittenen) Fläche, unabhängig vom `Solid`.
#[derive(Clone, Debug)]
pub struct FaceGeom {
    pub surface: Surface,
    pub same_sense: bool,
    pub domain: Option<UvRect>,
    /// `loops[0]` ist die äußere Schleife, danach folgen Löcher.
    pub loops: Vec<Vec<Seg>>,
    /// Marke der Quellfläche; wandert über die Flächenstücke in die Ergebnisflächen.
    pub tag: Option<FaceTag>,
}

pub fn face_geom(solid: &Solid, id: Id<Face>) -> FaceGeom {
    let f = &solid.faces[id];
    let to_loop = |l: &Loop| -> Vec<Seg> {
        l.half_edges
            .iter()
            .map(|he| {
                let e = &solid.edges[he.edge];
                let s = Seg::new(e.curve.clone(), e.t0, e.t1);
                if he.forward { s } else { s.reversed() }
            })
            .collect()
    };
    FaceGeom {
        surface: f.surface.clone(),
        same_sense: f.same_sense,
        domain: f.patch,
        loops: std::iter::once(&f.outer)
            .chain(f.inner.iter())
            .map(to_loop)
            .collect(),
        tag: f.tag.clone(),
    }
}

/// Abtastdichte der Bögen in den Randpolygonen: Abschnitte je Vollkreis (alle 5°).
pub(crate) const ARC_SAMPLES_PER_TURN: usize = 72;

/// Zahl der Abtastpunkte eines Stücks: Geraden nur ihr Anfangspunkt, Bögen alle 5°,
/// Freiformkurven nach Bézier-Stücken und Knickwinkel des Kontrollpolygons (ebenfalls 5°).
pub(crate) fn samples(s: &Seg) -> usize {
    match s.curve {
        Curve::Line { .. } => 1,
        _ => curve_sample_count(&s.curve, s.t0, s.t1, ARC_SAMPLES_PER_TURN, 2),
    }
}

/// Abtastpunkte einer Schleife (jedes Stück ohne seinen Endpunkt).
pub fn loop_points(lp: &[Seg]) -> Vec<DVec3> {
    let mut pts = Vec::new();
    for s in lp {
        let n = samples(s);
        for k in 0..n {
            pts.push(s.at(k as f64 / n as f64));
        }
    }
    pts
}

/// Wert `x + k·period`, der `reference` am nächsten liegt.
fn unwrap_near(x: f64, reference: f64, period: f64) -> f64 {
    x + period * ((reference - x) / period).round()
}

/// Kleinster Sprung über einen Pol, bezogen auf die Periode (bei `TAU` genau 1e-9): darunter
/// gilt der Zielwert als schon erreicht, und es folgt ein voller Umlauf.
const POLE_JUMP_MIN_RATIO: f64 = 1e-9 / TAU;

/// Sprung über einen Pol: von `u_at` in Richtung `dir` (±1) bis zum nächsten Wert, der
/// `u_target` modulo der Periode entspricht (mindestens ein Stück, höchstens ein voller Umlauf).
/// Ohne Periode in `u` (offene Freiformfläche mit Pol) direkt zum Zielwert.
fn pole_jump(u_at: f64, u_target: f64, dir: f64, period: Option<f64>) -> f64 {
    let Some(period) = period else {
        return u_target;
    };
    let mut delta = ((u_target - u_at) * dir).rem_euclid(period);
    if delta < POLE_JUMP_MIN_RATIO * period {
        delta = period;
    }
    u_at + dir * delta
}

/// Parameter aller Punkte. Analytische Flächen: `project`. Freiformflächen: `project_near` mit
/// dem vorigen Ergebnis als Startwert (fortlaufende Punkte einer Schleife); liegt der so
/// gefundene Flächenpunkt weiter als [`NURBS_EDGE_TOL`] vom Eingabepunkt, entscheidet die
/// globale Suche (`project`). Ergebnisse im Parameterbereich (gefaltet).
fn project_all(surface: &Surface, pts: &[DVec3]) -> Vec<(f64, f64)> {
    if !matches!(surface, Surface::Nurbs(_)) {
        return pts.iter().map(|p| surface.project(*p)).collect();
    }
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for p in pts {
        let uv = match out.last() {
            Some(&hint) => {
                let (u, v) = surface.project_near(*p, hint);
                if surface.point(u, v).distance(*p) <= NURBS_EDGE_TOL {
                    (u, v)
                } else {
                    surface.project(*p)
                }
            }
            None => surface.project(*p),
        };
        out.push(uv);
    }
    out
}

/// Polygon im Parameterraum. Bei periodischen Flächen wird `u` (beim Torus und bei in `v`
/// geschlossenen Freiformflächen auch `v`) entlang der Schleife stetig aufgerollt, mit der
/// Periode der Fläche (`u_period`/`v_period`); Pole (Kugelpole, Kegelspitze, zusammengefallene
/// Kontrollpunktreihen) werden als Strecke entlang `v = const` eingefügt.
///
/// Richtung der Polstrecke: Die Fläche liegt links der Laufrichtung, wenn die Schleife in `(u, v)`
/// gegen den Uhrzeigersinn läuft (`same_sense`). Wer von unten (kleineres `v`) auf einen Pol zuläuft,
/// hat die Fläche also bei kleinerem `u` und läuft am Pol in Richtung abnehmender `u` weiter; von
/// oben kommend in Richtung zunehmender `u`. Bei `!same_sense` ist es jeweils umgekehrt.
///
/// `points_to_uv_indexed` liefert zu jedem Polygonpunkt den Index des zugehörigen Eingabepunkts
/// (`None` für die eingefügten Polpunkte); `points_to_uv` gibt nur die Polygonpunkte zurück.
pub fn points_to_uv_indexed(
    surface: &Surface,
    same_sense: bool,
    pts: &[DVec3],
) -> Vec<(DVec2, Option<usize>)> {
    let poles = surface.pole_vs();
    if !surface.is_periodic() && poles.is_empty() {
        return project_all(surface, pts)
            .into_iter()
            .enumerate()
            .map(|(i, (u, v))| (DVec2::new(u, v), Some(i)))
            .collect();
    }
    if pts.is_empty() {
        return Vec::new();
    }
    let s = if same_sense { 1.0 } else { -1.0 };
    let u_period = surface.u_period();
    let v_period = surface.v_period();
    let pole_of = |v: f64| {
        poles
            .iter()
            .copied()
            .find(|pv| (v - pv).abs() < 1e-7 * (1.0 + pv.abs()))
    };
    // Laufrichtung in `u` am Pol: von unten kommend −s, von oben kommend +s.
    let pole_dir = |v_before: f64, v_pole: f64| if v_before < v_pole { -s } else { s };

    // Schleife so drehen, dass sie nicht an einem Pol beginnt.
    let proj = project_all(surface, pts);
    let start = proj
        .iter()
        .position(|(_, v)| pole_of(*v).is_none())
        .unwrap_or(0);

    let mut out: Vec<(DVec2, Option<usize>)> = Vec::with_capacity(pts.len() + 4);
    let mut prev: Option<DVec2> = None;
    // Offener Polbesuch: (v des Pols, v vor dem Pol, u vor dem Pol)
    let mut pole: Option<(f64, f64, f64)> = None;
    for i in (0..pts.len()).map(|k| (start + k) % pts.len()) {
        let (u0, v0) = proj[i];
        if let Some(pv) = pole_of(v0) {
            if pole.is_none() {
                let before = prev.unwrap_or(DVec2::new(0.0, 0.0));
                pole = Some((pv, before.y, before.x));
            }
            continue;
        }
        let (mut u, v) = match prev {
            Some(p) => (
                u_period.map_or(u0, |t| unwrap_near(u0, p.x, t)),
                v_period.map_or(v0, |t| unwrap_near(v0, p.y, t)),
            ),
            None => (u0, v0),
        };
        if let Some((vp, v_before, u_at)) = pole.take() {
            let u_new = pole_jump(u_at, u0, pole_dir(v_before, vp), u_period);
            out.push((DVec2::new(u_at, vp), None));
            out.push((DVec2::new(u_new, vp), None));
            u = u_new;
        }
        out.push((DVec2::new(u, v), Some(i)));
        prev = Some(DVec2::new(u, v));
    }
    if let Some((vp, v_before, u_at)) = pole {
        // Schleife endet am Pol: über den Pol zum Anfangspunkt zurück.
        let first_u = out.first().map(|p| p.0.x).unwrap_or(0.0);
        out.push((DVec2::new(u_at, vp), None));
        out.push((
            DVec2::new(
                pole_jump(u_at, first_u, pole_dir(v_before, vp), u_period),
                vp,
            ),
            None,
        ));
    }
    out
}

pub fn points_to_uv(surface: &Surface, same_sense: bool, pts: &[DVec3]) -> Vec<DVec2> {
    points_to_uv_indexed(surface, same_sense, pts)
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

/// Vorzeichenbehaftete Fläche des Polygons, positiv für Außenkonturen
/// (gegen den Uhrzeigersinn bei `same_sense`, sonst im Uhrzeigersinn).
pub fn oriented_area(poly: &[DVec2], same_sense: bool) -> f64 {
    let n = poly.len();
    let a: f64 = (0..n)
        .map(|i| poly[i].perp_dot(poly[(i + 1) % n]))
        .sum::<f64>()
        / 2.0;
    if same_sense { a } else { -a }
}

/// Windungszahl des Polygons um `q` (exakte Orientierungstests).
pub fn winding(poly: &[DVec2], q: DVec2) -> i32 {
    let n = poly.len();
    let mut w = 0;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if a.y <= q.y {
            if b.y > q.y && orient2d(a, b, q) > 0.0 {
                w += 1;
            }
        } else if b.y <= q.y && orient2d(a, b, q) < 0.0 {
            w -= 1;
        }
    }
    w
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Containment {
    Inside,
    Boundary,
    Outside,
}

/// Liegt `p` (auf der Fläche) innerhalb der beschnittenen Fläche, auf ihrem Rand oder außerhalb?
pub fn point_in_face(f: &FaceGeom, p: DVec3, tol: &Tolerance) -> Containment {
    if f.loops
        .iter()
        .flatten()
        .any(|s| s.closest(p).1 <= tol.linear)
    {
        return Containment::Boundary;
    }
    let (u, v) = f.surface.project(p);
    let shifts = |period: Option<f64>| match period {
        Some(t) => vec![-t, 0.0, t],
        None => vec![0.0],
    };
    let u_shifts = shifts(f.surface.u_period());
    let v_shifts = shifts(f.surface.v_period());
    let mut total = 0;
    for lp in &f.loops {
        let poly = points_to_uv(&f.surface, f.same_sense, &loop_points(lp));
        for du in &u_shifts {
            for dv in &v_shifts {
                total += winding(&poly, DVec2::new(u + du, v + dv));
            }
        }
    }
    let s = if f.same_sense { 1 } else { -1 };
    if total * s >= 1 {
        Containment::Inside
    } else {
        Containment::Outside
    }
}
