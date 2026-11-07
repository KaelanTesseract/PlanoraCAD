//! Haltepunkte auf Schnittkurven: Treffer der Randkanten mit einer Trägerfläche, Zerlegen der
//! Kurve an diesen Punkten und Lage der Stücke zu einer Fläche. Gemeinsam genutzt vom Beschnitt
//! der Booleschen Operationen (`imprint.rs`) und vom Ebenenschnitt (`section.rs`).

use super::roots::{CurveSurface, curve_surface_roots};
use super::seg::Seg;
use super::split::pole_point;
use super::uv::{Containment, FaceGeom, point_in_face};
use crate::geometry::{Curve, Surface};
use crate::validate::edge_surface_tol;
use planora_math::{DVec3, Tolerance};

/// Kleinster Parameterabstand zweier Haltepunkte, der noch ein eigenes Kurvenstück ergibt.
const MIN_PARAM_SPAN: f64 = 1e-9;

/// Anfang des Parameterbereichs einer geschlossenen Kurve (Kreis, Ellipse: 0).
fn period_start(c: &Curve) -> f64 {
    c.domain().map_or(0.0, |(a, _)| a)
}

/// Parameter von `p` auf der Kurve, falls `p` (innerhalb der Toleranz) auf ihr liegt.
/// Bei geschlossenen Kurven in `[Anfang, Anfang + Periode)` gefaltet.
fn param_on_curve(c: &Curve, p: DVec3, tol: &Tolerance) -> Option<f64> {
    let t = c.project(p);
    let t = match c.period() {
        Some(period) => {
            let a = period_start(c);
            a + (t - a).rem_euclid(period)
        }
        None => t,
    };
    (c.point(t).distance(p) <= tol.linear * 10.0).then_some(t)
}

/// Punkte, an denen die Randkante `e` die Trägerfläche `other` trifft: Nullstellen, Kantenenden
/// innerhalb der Toleranz auf der Fläche und bei ganz aufliegenden (nicht geschlossenen) Kanten
/// ihre Enden.
pub(super) fn edge_hits(e: &Seg, other: &Surface, tol: &Tolerance) -> Vec<DVec3> {
    let mut out = Vec::new();
    match curve_surface_roots(&e.curve, e.t0, e.t1, other, tol) {
        CurveSurface::Roots(rs) => {
            for r in rs {
                out.push(e.curve.point(r.t));
            }
            // Ein Kantenende knapp innerhalb der Toleranz kann der Nullstellensuche
            // entgehen, wenn die Kante gleich dahinter die Fläche verlässt.
            for p in [e.start(), e.end()] {
                if other.implicit(p).abs() <= edge_surface_tol(&e.curve, other, tol) {
                    out.push(p);
                }
            }
        }
        // Der Anfangspunkt einer geschlossenen Kante ist kein Teilungspunkt.
        CurveSurface::Lies if e.start().distance(e.end()) <= tol.linear => {}
        CurveSurface::Lies => {
            out.push(e.start());
            out.push(e.end());
        }
    }
    out
}

/// Haltepunkte der Schnittkurve `c`: die Kantentreffer `hits` (auch Nahtkanten: so wird die
/// Kurve an der Naht geteilt), dazu Pole und Kegelspitzen der Trägerflächen `surfaces`, durch die
/// die Kurve läuft. Punkte, die nicht auf der Kurve liegen, entfallen; nahe Punkte zählen einmal.
pub(super) fn breakpoints(
    c: &Curve,
    hits: &[DVec3],
    surfaces: &[&Surface],
    tol: &Tolerance,
) -> Vec<f64> {
    let mut ts: Vec<f64> = Vec::new();
    let mut pts: Vec<DVec3> = Vec::new();
    let mut add = |p: DVec3| {
        if let Some(t) = param_on_curve(c, p, tol) {
            let q = c.point(t);
            if !pts.iter().any(|x| x.distance(q) <= tol.linear) {
                pts.push(q);
                ts.push(t);
            }
        }
    };
    for p in hits {
        add(*p);
    }
    for s in surfaces {
        for pv in s.pole_vs() {
            add(pole_point(s, pv));
        }
    }
    // Offene Freiformkurven (Äste des numerischen Schnittrechners) enden auf dem Rand einer
    // Fläche oder dort, wo der nächste Teil desselben Asts beginnt: Ihre Enden sind Haltepunkte,
    // sonst gingen die Stücke zwischen dem letzten Haltepunkt und dem Kurvenende verloren.
    if let (Curve::Nurbs(_), None, Some((a, b))) = (c, c.period(), c.domain()) {
        for t in [a, b] {
            let q = c.point(t);
            if !pts.iter().any(|x| x.distance(q) <= tol.linear) {
                pts.push(q);
                ts.push(t);
            }
        }
    }
    ts
}

/// Zerlegt die Kurve an den Haltepunkten `ts` in Stücke (geschlossene Kurven ringsum, ohne
/// Haltepunkt als ganze Kurve; offene Kurven nur zwischen zwei Haltepunkten). Stücke kürzer als
/// die Toleranz entfallen.
pub(super) fn split_at(c: &Curve, mut ts: Vec<f64>, tol: &Tolerance) -> Vec<Seg> {
    ts.sort_by(|x, y| x.total_cmp(y));
    let mut intervals: Vec<(f64, f64)> = Vec::new();
    if let Some(period) = c.period() {
        if ts.is_empty() {
            let a = period_start(c);
            intervals.push((a, a + period));
        } else {
            for w in ts.windows(2) {
                intervals.push((w[0], w[1]));
            }
            intervals.push((ts[ts.len() - 1], ts[0] + period));
        }
    } else {
        for w in ts.windows(2) {
            intervals.push((w[0], w[1]));
        }
    }
    intervals
        .into_iter()
        .filter(|(a, b)| (b - a).abs() > MIN_PARAM_SPAN)
        .map(|(a, b)| Seg::new(c.clone(), a, b))
        .filter(|seg| seg.length() > tol.linear)
        .collect()
}

/// Lage eines Stücks zur Fläche `geom` mit den Nahtstücken `seams`. Liegt die Mitte zufällig auf
/// dem Rand (etwa auf der Naht), werden weitere Punkte des Stücks befragt. Liegt das ganze Stück
/// auf einer Naht (und nicht zugleich auf einem echten Randstück), gilt es als innen: die Naht ist
/// Inneres der Fläche.
pub(super) fn classify_seg(
    geom: &FaceGeom,
    seams: &[Seg],
    seg: &Seg,
    tol: &Tolerance,
) -> Containment {
    for f in [0.5, 0.381_966, 0.618_034, 0.25, 0.75] {
        let c = point_in_face(geom, seg.at(f), tol);
        if c != Containment::Boundary {
            return c;
        }
    }
    let mid = seg.at(0.5);
    // Nahterkennung mit 4·tol wie in `split_face` (Nahtstücke werden dort so gepaart); der Test
    // gegen echte Randstücke nutzt tol wie `point_in_face`, damit beide Urteile zusammenpassen.
    let is_seam = |s: &Seg| {
        seams
            .iter()
            .any(|m| m.closest(s.at(0.5)).1 <= tol.linear * 4.0)
    };
    let on_seam = seams.iter().any(|m| m.closest(mid).1 <= tol.linear * 4.0);
    let on_real_edge = geom
        .loops
        .iter()
        .flatten()
        .filter(|s| !is_seam(s))
        .any(|s| s.closest(mid).1 <= tol.linear);
    if on_seam && !on_real_edge {
        Containment::Inside
    } else {
        Containment::Boundary
    }
}
