//! Nähte, Pole und Entartungen (STEP-Import, Aufgabe 6): Bereinigung der Rohform
//! ([`RawBody`]), bevor daraus ein Körper des Kernels wird.
//!
//! Der Kernel kennt keine Kanten der Länge null, keinen Pol als Schleife (`VERTEX_LOOP`) und
//! keine periodische Fläche ohne Naht; STEP-Dateien enthalten alles drei. Der Leser
//! - zieht Ecken, die höchstens `weld_tol` neben einem Pol ihrer Fläche liegen (Kugelpol,
//!   Kegelspitze), genau auf den Pol ([`snap_poles`]); die Kanten bekommen dort ihren Parameter
//!   neu. So erkennt der Kernel den Pol (er vergleicht `v` mit 1e-7);
//! - entfernt Kanten, deren Kurve kürzer als `weld_tol` ist ([`drop_degenerate`]; die
//!   `VERTEX_LOOP`s sind schon beim Lesen verworfen), und legt ihre Ecken zusammen;
//! - fügt Zylinder-, Kegel- und Kugelflächen ohne Naht eine Naht hinzu ([`insert_seams`], P9):
//!   zwischen zwei vollen Kreisen, von einem Kreis zum Pol oder (Kugel ohne Rand) von Pol zu Pol;
//!   andere Flächen ohne Naht werden ausgelassen;
//! - prüft, dass jeder Rand, der in die Nähe eines Pols kommt, genau durch ihn läuft
//!   ([`check_poles`]); sonst sähe der Kernel dort keinen Pol und der Körper käme mit falschem
//!   Volumen an.

use crate::read::geom::Skip;
use crate::read::topo::{RawBody, RawEdge, RawHalfEdge, at_line};
use planora_kernel::boolean::points_to_uv;
use planora_kernel::{Curve, Surface};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use std::f64::consts::FRAC_PI_2;

/// Abschnitte, über die die Länge einer Kante gemessen wird.
const LENGTH_SECTIONS: usize = 16;

/// Abstand (relativ), unter dem `v` auf einem Pol liegt; wie `boolean::points_to_uv`.
const POLE_REL: f64 = 1e-7;

fn linear() -> f64 {
    Tolerance::default().linear
}

/// Länge der Kurve einer Kante über [`LENGTH_SECTIONS`] Abschnitte.
pub fn edge_length(e: &RawEdge) -> f64 {
    let n = LENGTH_SECTIONS;
    (0..n)
        .map(|k| {
            let a = e.t0 + (e.t1 - e.t0) * k as f64 / n as f64;
            let b = e.t0 + (e.t1 - e.t0) * (k + 1) as f64 / n as f64;
            e.curve.point(a).distance(e.curve.point(b))
        })
        .sum()
}

/// Pole einer Fläche: `v` und Punkt.
fn pole_points(s: &Surface) -> Vec<(f64, DVec3)> {
    let u = match s {
        Surface::Nurbs(d) => d.surface().domain().0.0,
        _ => 0.0,
    };
    s.pole_vs()
        .into_iter()
        .map(|pv| (pv, s.point(u, pv)))
        .collect()
}

fn at_pole(v: f64, pv: f64) -> bool {
    (v - pv).abs() < POLE_REL * (1.0 + pv.abs())
}

/// Parameter des Punkts `p` auf der Kurve, bei periodischen Kurven dem bisherigen Wert `old` am
/// nächsten.
fn param_near(c: &Curve, p: DVec3, old: f64) -> f64 {
    let t = c.project(p);
    if !t.is_finite() {
        return old;
    }
    match c.period() {
        Some(per) => t + per * ((old - t) / per).round(),
        None => t,
    }
}

/// Zieht jede Ecke, die höchstens `tol` neben einem Pol einer ihrer Flächen liegt, genau auf
/// den Pol; die Kanten an ihr bekommen dort ihren Parameter neu.
// Der negierte Vergleich lehnt absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn snap_poles(raw: &mut RawBody, tol: f64) {
    if !(tol > 0.0) {
        return;
    }
    let mut target: Vec<Option<DVec3>> = vec![None; raw.vertices.len()];
    for f in &raw.faces {
        let poles = pole_points(&f.surface);
        if poles.is_empty() {
            continue;
        }
        for he in f.loops.iter().flatten() {
            let e = &raw.edges[he.edge];
            for v in [e.start, e.end] {
                let p = raw.vertices[v];
                if let Some((_, q)) = poles.iter().find(|(_, q)| q.distance(p) <= tol) {
                    target[v] = Some(*q);
                }
            }
        }
    }
    for (v, q) in target.iter().enumerate() {
        if let Some(q) = q {
            raw.vertices[v] = *q;
        }
    }
    for e in &mut raw.edges {
        if let Some(q) = target[e.start] {
            e.t0 = param_near(&e.curve, q, e.t0);
        }
        if let Some(q) = target[e.end] {
            e.t1 = param_near(&e.curve, q, e.t1);
        }
    }
}

/// Entfernt Kanten, deren Kurve kürzer als `tol` ist (mindestens die Toleranz des Kernels;
/// Länge über [`LENGTH_SECTIONS`] Abschnitte, nicht über den Abstand der Ecken: ein voller Kreis
/// hat gleiche Ecken und bleibt), aus allen Schleifen und legt ihre beiden Ecken zusammen.
/// Schleifen, die danach leer sind, fallen weg (wie `VERTEX_LOOP`s). Schließt sich eine Schleife
/// danach nicht mehr (eine lange Kurve zwischen zwei gleichen Ecken), ist das „Eine Kante hat die
/// Länge null.“ mit der Zeile der Kante. Die Kanten selbst bleiben bis zum Aufräumen stehen.
pub fn drop_degenerate(raw: &mut RawBody, tol: f64) -> Result<(), Skip> {
    let tol = tol.max(linear());
    let degenerate: Vec<bool> = raw.edges.iter().map(|e| edge_length(e) < tol).collect();
    if !degenerate.contains(&true) {
        return Ok(());
    }
    // Ecken der entarteten Kanten zusammenlegen (Ende auf Anfang).
    let mut rep: Vec<usize> = (0..raw.vertices.len()).collect();
    let find = |rep: &[usize], mut v: usize| {
        while rep[v] != v {
            v = rep[v];
        }
        v
    };
    for (k, e) in raw.edges.iter().enumerate() {
        if degenerate[k] {
            let (a, b) = (find(&rep, e.start), find(&rep, e.end));
            if a != b {
                rep[b] = a;
            }
        }
    }
    for e in &mut raw.edges {
        e.start = find(&rep, e.start);
        e.end = find(&rep, e.end);
    }
    let edges = &raw.edges;
    let start = |he: &RawHalfEdge| {
        let e = &edges[he.edge];
        if he.forward { e.start } else { e.end }
    };
    let end = |he: &RawHalfEdge| {
        let e = &edges[he.edge];
        if he.forward { e.end } else { e.start }
    };
    for f in &mut raw.faces {
        let mut kept: Vec<Vec<RawHalfEdge>> = Vec::with_capacity(f.loops.len());
        let mut marked = None;
        for (i, lp) in f.loops.iter().enumerate() {
            let dropped = lp.iter().find(|he| degenerate[he.edge]).map(|he| he.edge);
            let rest: Vec<RawHalfEdge> = lp
                .iter()
                .copied()
                .filter(|he| !degenerate[he.edge])
                .collect();
            if rest.is_empty() {
                continue;
            }
            if let Some(k) = dropped {
                let n = rest.len();
                if (0..n).any(|j| end(&rest[j]) != start(&rest[(j + 1) % n])) {
                    return Err(at_line(edges[k].line, "Eine Kante hat die Länge null."));
                }
            }
            if f.marked_outer == Some(i) {
                marked = Some(kept.len());
            }
            kept.push(rest);
        }
        f.loops = kept;
        f.marked_outer = marked;
    }
    Ok(())
}

/// Läuft jeder Rand, der höchstens `tol` (mindestens die Toleranz des Kernels) an einen Pol
/// seiner Fläche herankommt, genau durch ihn (`v` wie im Kernel auf 1e-7 am Pol)? Sonst ließe sich
/// der Rand dort nicht in den Parameterraum abbilden (der Kernel sähe keinen Pol, `u` wäre
/// zufällig) und der Körper käme falsch an.
pub fn check_poles(raw: &RawBody, tol: f64) -> Result<(), Skip> {
    let near = tol.max(linear());
    for f in &raw.faces {
        let poles = pole_points(&f.surface);
        if poles.is_empty() {
            continue;
        }
        for lp in &f.loops {
            for p in raw.loop_points(lp) {
                for (pv, q) in &poles {
                    if q.distance(p) <= near && !at_pole(f.surface.project(p).1, *pv) {
                        return Err(Skip(format!(
                            "Der Rand der Fläche „{}“ läuft nicht genau durch einen Pol.",
                            f.tag.role
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Name der Flächenart für die Meldung nach P9.
fn kind_name(s: &Surface) -> &'static str {
    match s {
        Surface::Plane { .. } => "Ebene",
        Surface::Cylinder { .. } => "Zylinder",
        Surface::Cone { .. } => "Kegel",
        Surface::Sphere { .. } => "Kugel",
        Surface::Torus { .. } => "Torus",
        Surface::Nurbs(_) => "Freiformfläche",
    }
}

/// Fügt Flächen ohne Naht eine Naht hinzu (P9). Eine Fläche hat keine Naht, wenn sie keinen Rand
/// hat oder Ränder, die sich um ihre Achse winden (im Parameterraum nach der Regel des Kernels
/// aufgerollt: Anfang und Ende liegen eine Periode auseinander). Nur bei Zylinder, Kegel und
/// Kugel wird eingefügt:
/// - zwei Kreisschleifen (die untere läuft in Richtung der Außennormale gegen den Uhrzeigersinn,
///   die obere andersherum): Naht entlang der Mantellinie (Kugel: Meridian) beim Winkel `u_s` der
///   ersten Ecke der ersten Kreisschleife; hat die andere Schleife dort keine Ecke, wird ihre
///   Kante bei `u_s` geteilt (in allen Schleifen, die sie benutzen);
/// - eine Kreisschleife und auf ihrer Seite ein Pol (Kugelkappe, Kegel mit Spitze): Naht vom
///   Kreis zum Pol, der Pol wird eine Ecke;
/// - Kugel ohne Rand: Naht im Meridian bei `u = 0` von Pol zu Pol.
///
/// Die neue Schleife hat die Form des Kernels `[Kreis, Naht, Kreis, Naht rückwärts]` (ohne den
/// fehlenden Kreis an einem Pol) und wird der äußere Rand. Passen die Ränder, die sich um die
/// Achse winden, nicht dazu (etwa ein Kreis falsch herum, ein Rand durch einen Pol, der den
/// anderen nicht erreicht): „Der Rand der Fläche „…“ schließt sich nicht: Eine Kante läuft falsch
/// herum.“ (wie die Prüfung der Schleifen). Andere Flächenarten ohne Naht (Ränder gegenläufig um
/// die Achse oder kein Rand): „Fläche vom Typ {Typ} ohne Naht wird noch nicht unterstützt.“;
/// Flächen ohne Rand, die keine Kugel sind: „Eine Fläche hat keinen Rand.“
pub fn insert_seams(raw: &mut RawBody, tol: f64) -> Result<(), Skip> {
    let tol = tol.max(linear());
    for i in 0..raw.faces.len() {
        insert_seam(raw, i, tol)?;
    }
    Ok(())
}

/// Windungszahl eines aufgerollten Rands in Richtung `k` (Perioden zwischen Anfang und Ende).
fn winding(poly: &[DVec2], k: usize, period: Option<f64>) -> i64 {
    let (Some(per), Some(a), Some(b)) = (period, poly.first(), poly.last()) else {
        return 0;
    };
    let (a, b) = if k == 0 { (a.x, b.x) } else { (a.y, b.y) };
    ((b - a) / per).round() as i64
}

fn insert_seam(raw: &mut RawBody, i: usize, tol: f64) -> Result<(), Skip> {
    let f = &raw.faces[i];
    let surface = f.surface.clone();
    let periods = [surface.u_period(), surface.v_period()];
    let no_rim = || at_line(f.line, "Eine Fläche hat keinen Rand.");
    let no_seam = || {
        Skip(format!(
            "Fläche vom Typ {} ohne Naht wird noch nicht unterstützt.",
            kind_name(&surface)
        ))
    };
    // Ränder, die sich um die Achse winden, aber nicht zu einer Fläche ohne Naht passen (etwa
    // ein Kreis falsch herum): derselbe Grund wie bei der Prüfung der Schleifen.
    let not_closing = || {
        Skip(format!(
            "Der Rand der Fläche „{}“ schließt sich nicht: Eine Kante läuft falsch herum.",
            f.tag.role
        ))
    };
    if periods.iter().all(Option::is_none) {
        return if f.loops.is_empty() {
            Err(no_rim())
        } else {
            Ok(())
        };
    }
    let polys: Vec<Vec<DVec2>> = f
        .loops
        .iter()
        .map(|lp| points_to_uv(&surface, f.same_sense, &raw.loop_points(lp)))
        .collect();
    let windings: Vec<[i64; 2]> = polys
        .iter()
        .map(|p| [winding(p, 0, periods[0]), winding(p, 1, periods[1])])
        .collect();
    let winding_loops: Vec<usize> = (0..windings.len())
        .filter(|&k| windings[k] != [0, 0])
        .collect();
    if !f.loops.is_empty() && winding_loops.is_empty() {
        return Ok(());
    }
    // Ein Rand ohne Naht ist ein Kreis um die Achse; einer, der durch einen Pol läuft und sich
    // trotzdem windet, erreicht den anderen Pol nicht.
    // Schließt er sich mit dem umgekehrten Sinn der Fläche, liegt es am Sinn.
    let poles = surface.pole_vs();
    for &k in &winding_loops {
        if polys[k]
            .iter()
            .any(|q| poles.iter().any(|pv| at_pole(q.y, *pv)))
        {
            let other = points_to_uv(&surface, !f.same_sense, &raw.loop_points(&f.loops[k]));
            return Err(if winding(&other, 0, periods[0]) == 0 {
                Skip(format!(
                    "Die Fläche „{}“ ist verdreht: Rand und Außenseite passen nicht zusammen.",
                    f.tag.role
                ))
            } else {
                not_closing()
            });
        }
    }
    if !matches!(
        surface,
        Surface::Cylinder { .. } | Surface::Cone { .. } | Surface::Sphere { .. }
    ) {
        if f.loops.is_empty() {
            return Err(no_seam());
        }
        let sum = windings
            .iter()
            .fold([0, 0], |s, w| [s[0] + w[0], s[1] + w[1]]);
        // Gegenläufige Ränder um die Achse: eine heile Fläche ohne Naht. Sonst ein Fehler in
        // den Rändern.
        return Err(if sum == [0, 0] {
            no_seam()
        } else {
            not_closing()
        });
    }
    let s: i64 = if f.same_sense { 1 } else { -1 };
    let line = f.line;
    match winding_loops[..] {
        [] => match surface {
            Surface::Sphere { .. } => {
                let south = push_vertex(raw, surface.point(0.0, -FRAC_PI_2));
                let north = push_vertex(raw, surface.point(0.0, FRAC_PI_2));
                let seam = push_seam(raw, &surface, 0.0, south, north, line)?;
                set_seam_loop(raw, i, &[], vec![fwd(seam), bwd(seam)]);
                Ok(())
            }
            _ => Err(no_rim()),
        },
        [a] => {
            // Fläche auf der linken Seite des Rands: oberhalb, wenn er um die Außennormale
            // gegen den Uhrzeigersinn läuft.
            let above = windings[a][0] * s > 0;
            let va = raw.he_start(f.loops[a][0]);
            let (u_s, v_a) = surface.project(raw.vertices[va]);
            let pole = surface
                .pole_vs()
                .into_iter()
                .filter(|&pv| if above { pv > v_a } else { pv < v_a })
                .min_by(|x, y| (x - v_a).abs().total_cmp(&(y - v_a).abs()));
            let Some(pv) = pole else {
                return Err(not_closing());
            };
            let Some(rim) = rotated(raw, &f.loops[a], va) else {
                return Err(not_closing());
            };
            let vp = push_vertex(raw, surface.point(u_s, pv));
            let seam = push_seam(raw, &surface, u_s, va, vp, line)?;
            let mut lp = rim;
            lp.extend([fwd(seam), bwd(seam)]);
            set_seam_loop(raw, i, &[a], lp);
            Ok(())
        }
        [a, b] => {
            if windings[a][1] != 0 || windings[b][1] != 0 {
                return Err(not_closing());
            }
            let level = |p: &[DVec2]| p.iter().map(|q| q.y).sum::<f64>() / p.len() as f64;
            let (lower, upper) = if level(&polys[a]) < level(&polys[b]) {
                (a, b)
            } else {
                (b, a)
            };
            if windings[lower][0] != s || windings[upper][0] != -s {
                return Err(not_closing());
            }
            let va = raw.he_start(f.loops[a][0]);
            let (u_s, _) = surface.project(raw.vertices[va]);
            let vb = vertex_at(raw, i, b, u_s, tol)?;
            let f = &raw.faces[i];
            let (Some(rim_a), Some(rim_b)) =
                (rotated(raw, &f.loops[a], va), rotated(raw, &f.loops[b], vb))
            else {
                return Err(Skip(format!(
                    "Der Fläche „{}“ lässt sich keine Naht hinzufügen.",
                    f.tag.role
                )));
            };
            let seam = push_seam(raw, &surface, u_s, va, vb, line)?;
            let mut lp = rim_a;
            lp.push(fwd(seam));
            lp.extend(rim_b);
            lp.push(bwd(seam));
            set_seam_loop(raw, i, &[a, b], lp);
            Ok(())
        }
        _ => Err(not_closing()),
    }
}

fn fwd(edge: usize) -> RawHalfEdge {
    RawHalfEdge {
        edge,
        forward: true,
    }
}

fn bwd(edge: usize) -> RawHalfEdge {
    RawHalfEdge {
        edge,
        forward: false,
    }
}

fn push_vertex(raw: &mut RawBody, p: DVec3) -> usize {
    raw.vertices.push(p);
    raw.vertices.len() - 1
}

/// Schleife `lp` so gedreht, dass sie an der Ecke `v` beginnt.
fn rotated(raw: &RawBody, lp: &[RawHalfEdge], v: usize) -> Option<Vec<RawHalfEdge>> {
    let k = lp.iter().position(|he| raw.he_start(*he) == v)?;
    Some(lp[k..].iter().chain(&lp[..k]).copied().collect())
}

/// Ersetzt die Schleifen `replaced` der Fläche `i` durch `lp` als äußeren Rand.
fn set_seam_loop(raw: &mut RawBody, i: usize, replaced: &[usize], lp: Vec<RawHalfEdge>) {
    let f = &mut raw.faces[i];
    let rest: Vec<Vec<RawHalfEdge>> = std::mem::take(&mut f.loops)
        .into_iter()
        .enumerate()
        .filter(|(k, _)| !replaced.contains(k))
        .map(|(_, l)| l)
        .collect();
    f.loops = std::iter::once(lp).chain(rest).collect();
    f.marked_outer = Some(0);
}

/// Neue Nahtkante von Ecke `a` nach Ecke `b` beim Winkel `u_s`: Zylinder und Kegel eine Gerade
/// (Mantellinie), Kugel ein Kreis im Meridian (wie `revolve.rs` ihn baut: Ebene durch die Achse,
/// Parameter = Breite).
// Der negierte Vergleich lehnt absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn push_seam(
    raw: &mut RawBody,
    surface: &Surface,
    u_s: f64,
    a: usize,
    b: usize,
    line: usize,
) -> Result<usize, Skip> {
    let (pa, pb) = (raw.vertices[a], raw.vertices[b]);
    let (curve, t0, t1) = match surface {
        Surface::Sphere { frame, radius } => {
            let radial = frame.x * u_s.cos() + frame.y * u_s.sin();
            let curve = Curve::Circle {
                frame: Frame {
                    origin: frame.origin,
                    x: radial,
                    y: frame.z,
                    z: radial.cross(frame.z),
                },
                radius: *radius,
            };
            (curve, surface.project(pa).1, surface.project(pb).1)
        }
        _ => {
            let d = pb - pa;
            let len = d.length();
            if !(len > linear()) {
                return Err(at_line(line, "Eine Naht hätte die Länge null."));
            }
            (
                Curve::Line {
                    origin: pa,
                    dir: d / len,
                },
                0.0,
                len,
            )
        }
    };
    raw.edges.push(RawEdge {
        curve,
        t0,
        t1,
        start: a,
        end: b,
        line: 0,
    });
    Ok(raw.edges.len() - 1)
}

/// Ecke der Schleife `b` der Fläche `i` beim Winkel `u_s`: eine vorhandene (höchstens `tol`
/// daneben) oder eine neue, für die die Kante an dieser Stelle geteilt wird.
fn vertex_at(raw: &mut RawBody, i: usize, b: usize, u_s: f64, tol: f64) -> Result<usize, Skip> {
    let f = &raw.faces[i];
    let surface = f.surface.clone();
    let lp = f.loops[b].clone();
    let (_, level) = surface.project(raw.vertices[raw.he_start(lp[0])]);
    let target = surface.point(u_s, level);
    for he in &lp {
        let v = raw.he_start(*he);
        if raw.vertices[v].distance(target) <= tol {
            return Ok(v);
        }
    }
    for he in &lp {
        let e = &raw.edges[he.edge];
        let (lo, hi) = (e.t0.min(e.t1), e.t0.max(e.t1));
        let mut t = e.curve.project(target);
        if let Some(per) = e.curve.period() {
            t += per * ((lo - t) / per).ceil();
        }
        if t > lo && t < hi && e.curve.point(t).distance(target) <= tol {
            return Ok(split_edge(raw, he.edge, t));
        }
    }
    Err(Skip(format!(
        "Der Fläche „{}“ lässt sich keine Naht hinzufügen.",
        f.tag.role
    )))
}

/// Teilt die Kante `k` beim Parameter `t` (neue Ecke auf der Kurve); jede Schleife, die sie
/// benutzt, bekommt statt ihrer die beiden Teile in ihrer Laufrichtung.
fn split_edge(raw: &mut RawBody, k: usize, t: f64) -> usize {
    let nv = push_vertex(raw, raw.edges[k].curve.point(t));
    let mut second = raw.edges[k].clone();
    second.t0 = t;
    second.start = nv;
    raw.edges[k].t1 = t;
    raw.edges[k].end = nv;
    raw.edges.push(second);
    let k2 = raw.edges.len() - 1;
    for f in &mut raw.faces {
        for lp in &mut f.loops {
            if !lp.iter().any(|he| he.edge == k) {
                continue;
            }
            let mut out = Vec::with_capacity(lp.len() + 1);
            for he in lp.iter() {
                match (he.edge == k, he.forward) {
                    (true, true) => out.extend([fwd(k), fwd(k2)]),
                    (true, false) => out.extend([bwd(k2), bwd(k)]),
                    _ => out.push(*he),
                }
            }
            *lp = out;
        }
    }
    nv
}
