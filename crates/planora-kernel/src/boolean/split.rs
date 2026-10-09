//! Auftrennen einer Fläche entlang von Schnittsegmenten.
//!
//! Algorithmus: ebener Graph aus Randstücken (nur in Flächenrichtung) und Schnittstücken (beide
//! Richtungen). Umläufe entstehen über das Drehsystem an jedem Knoten: Die nächste Halbkante ist
//! die im Uhrzeigersinn erste nach der Gegenkante, gemessen in der Tangentialebene um die
//! Außennormale (an Kugelpolen und der Kegelspitze um `±frame.z`). Positive Umläufe (orientierte
//! Fläche im Parameterraum) sind Außenkonturen, negative Löcher; ein Loch gehört zum kleinsten
//! positiven Umlauf, der es umschließt.
//!
//! Naht periodischer Flächen: Sie ist eine gewöhnliche, doppelt durchlaufene Kante der
//! Randschleife. Schnitte werden dort, wo sie die Naht kreuzen, geteilt. Trennt die Naht danach
//! zwei Umläufe, die in Wahrheit dasselbe Flächenstück sind, werden diese über die Naht
//! zusammengeführt (die Naht liegt dann im Inneren des Stücks); durchläuft ein Umlauf beide
//! Seiten eines Nahtstücks, bleibt die Naht als Kante erhalten.

use super::error::BooleanError;
use super::seg::{Seg, sample_count};
use super::uv::{FaceGeom, loop_points, oriented_area, points_to_uv, winding};
use crate::geometry::Surface;
use planora_math::{DVec2, DVec3, PI, TAU, Tolerance};

/// Ein zusammenhängendes Flächenstück: eine äußere Schleife und beliebig viele Löcher.
#[derive(Clone, Debug)]
pub struct Region {
    pub outer: Vec<Seg>,
    pub holes: Vec<Vec<Seg>>,
}

/// Gerichtete Halbkante des Graphen.
struct He {
    seg: Seg,
    from: usize,
    to: usize,
    /// Gegenläufige Halbkante desselben Schnittstücks (Randstücke haben keine).
    twin: Option<usize>,
}

/// Winkel `x` auf (−π, π] gefaltet.
fn wrap_angle(x: f64) -> f64 {
    x - TAU * (x / TAU).round()
}

/// Teilt Segmente an fremden Knotenpunkten, die in ihrem Inneren liegen.
fn split_at_points(segs: &[Seg], pts: &[DVec3], tol: &Tolerance) -> Vec<Seg> {
    let mut out = Vec::new();
    for s in segs {
        let (a, b) = (s.start(), s.end());
        let mut fs: Vec<f64> = Vec::new();
        for p in pts {
            // nur Punkte im Inneren des Stücks (nicht an seinen Enden)
            if p.distance(a) <= tol.linear || p.distance(b) <= tol.linear {
                continue;
            }
            let (f, d) = s.closest(*p);
            if d <= tol.linear
                && f > 1e-9
                && f < 1.0 - 1e-9
                && !fs.iter().any(|x| (x - f).abs() < 1e-7)
            {
                fs.push(f);
            }
        }
        fs.sort_by(|x, y| x.total_cmp(y));
        let mut prev = 0.0;
        for f in fs.into_iter().chain(std::iter::once(1.0)) {
            out.push(s.sub(prev, f));
            prev = f;
        }
    }
    out
}

/// Nahtstücke einer periodischen Fläche: Randstücke, die die Schleife auch gegenläufig enthält.
fn seam_segments(face: &FaceGeom, boundary: &[Seg], tol: &Tolerance) -> Vec<Seg> {
    if !face.surface.is_periodic() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, s) in boundary.iter().enumerate() {
        let paired = boundary.iter().enumerate().any(|(j, o)| {
            j != i
                && o.start().distance(s.end()) <= tol.linear * 4.0
                && o.end().distance(s.start()) <= tol.linear * 4.0
                && o.at(0.5).distance(s.at(0.5)) <= tol.linear * 4.0
        });
        if paired
            && !out
                .iter()
                .any(|o: &Seg| o.at(0.5).distance(s.at(0.5)) <= tol.linear * 4.0)
        {
            out.push(s.clone());
        }
    }
    out
}

/// Punkte, an denen das Schnittstück `s` die Naht `seam` (eine Linie `u = const`) kreuzt.
fn seam_crossings(surface: &Surface, s: &Seg, seam: &Seg, tol: &Tolerance) -> Vec<DVec3> {
    let u_seam = surface.project(seam.at(0.5)).0;
    let g = |f: f64| wrap_angle(surface.project(s.at(f)).0 - u_seam);
    let n = sample_count(s.t0, s.t1, 720, 64);
    let mut out = Vec::new();
    let mut push = |p: DVec3| {
        if seam.closest(p).1 <= tol.linear * 4.0 {
            out.push(p);
        }
    };
    let gs: Vec<f64> = (0..=n).map(|i| g(i as f64 / n as f64)).collect();
    for (i, gi) in gs.iter().enumerate() {
        if gi.abs() < 1e-12 {
            push(s.at(i as f64 / n as f64));
        }
    }
    for i in 0..n {
        let (g0, g1) = (gs[i], gs[i + 1]);
        // echter Vorzeichenwechsel, nicht der Sprung bei ±π
        if g0 * g1 < 0.0 && (g0 - g1).abs() < PI {
            let (mut lo, mut hi) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let mut glo = g0;
            for _ in 0..80 {
                let mid = 0.5 * (lo + hi);
                let gm = g(mid);
                if (gm < 0.0) == (glo < 0.0) {
                    lo = mid;
                    glo = gm;
                } else {
                    hi = mid;
                }
            }
            push(s.at(0.5 * (lo + hi)));
        }
    }
    out
}

fn node_of(nodes: &mut Vec<DVec3>, p: DVec3, tol: &Tolerance) -> usize {
    if let Some(i) = nodes.iter().position(|q| q.distance(p) <= tol.linear * 4.0) {
        return i;
    }
    nodes.push(p);
    nodes.len() - 1
}

/// Außennormale der Fläche am Knoten und eine Bezugsrichtung `e1` in der Tangentialebene.
/// An Polen (Kugelpole, Kegelspitze) ist die Normale `±frame.z`: Dort ordnen sich die
/// abgehenden Kurven nach ihrem Winkel um die Achse.
fn node_frame(face: &FaceGeom, p: DVec3, tol: &Tolerance) -> (DVec3, DVec3) {
    let sgn = if face.same_sense { 1.0 } else { -1.0 };
    let pole = face
        .surface
        .pole_vs()
        .into_iter()
        .find(|pv| face.surface.point(0.0, *pv).distance(p) <= tol.linear * 4.0);
    let n = match pole {
        Some(pv) => {
            let z = face.surface.frame().z;
            let side = if face.surface.normal(0.0, pv).dot(z) >= 0.0 {
                1.0
            } else {
                -1.0
            };
            z * (side * sgn)
        }
        None => {
            let (u, v) = face.surface.project(p);
            face.surface.normal(u, v) * sgn
        }
    };
    (n, n.any_orthonormal_vector())
}

fn angle(n: DVec3, e1: DVec3, t: DVec3) -> f64 {
    let e2 = n.cross(e1);
    t.dot(e2).atan2(t.dot(e1))
}

fn same_piece(a: &Seg, b: &Seg, tol: &Tolerance) -> bool {
    let ends = (a.start().distance(b.start()) <= tol.linear
        && a.end().distance(b.end()) <= tol.linear)
        || (a.start().distance(b.end()) <= tol.linear && a.end().distance(b.start()) <= tol.linear);
    ends && a.at(0.5).distance(b.at(0.5)) <= tol.linear * 4.0
}

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// Teilt eine Fläche entlang von Schnittsegmenten. Algorithmus: ebener Graph aus Randstücken
/// (nur in Flächenrichtung) und Schnittstücken (beide Richtungen); Umläufe werden über das
/// Drehsystem an jedem Knoten gebildet (nächste Halbkante = im Uhrzeigersinn erste nach der
/// Gegenkante). Positive Umläufe sind Außenkonturen, negative Löcher.
pub fn split_face(
    face: &FaceGeom,
    sections: &[Seg],
    tol: &Tolerance,
) -> Result<Vec<Region>, BooleanError> {
    let finite = |s: &Seg| {
        s.t0.is_finite()
            && s.t1.is_finite()
            && [s.start(), s.at(0.5), s.end()]
                .iter()
                .all(|p| p.is_finite())
    };
    if !sections.iter().all(finite) {
        return Err(BooleanError::InvalidInput(
            "Schnittkurve mit ungültigen Zahlenwerten".into(),
        ));
    }

    let boundary: Vec<Seg> = face.loops.iter().flatten().cloned().collect();
    let mut all_pts: Vec<DVec3> = boundary
        .iter()
        .chain(sections.iter())
        .flat_map(|s| [s.start(), s.end()])
        .collect();
    // Schnitte, die die Naht kreuzen, bekommen dort einen Knoten (Naht und Schnitt werden geteilt).
    for seam in seam_segments(face, &boundary, tol) {
        for s in sections {
            all_pts.extend(seam_crossings(&face.surface, s, &seam, tol));
        }
    }
    let degenerate = |s: &Seg| {
        s.start().distance(s.end()) <= tol.linear * 4.0
            && s.at(0.5).distance(s.start()) <= tol.linear * 4.0
    };
    let boundary: Vec<Seg> = split_at_points(&boundary, &all_pts, tol)
        .into_iter()
        .filter(|s| !degenerate(s))
        .collect();
    let secs: Vec<Seg> = split_at_points(sections, &all_pts, tol)
        .into_iter()
        .filter(|s| !degenerate(s))
        .collect();

    // doppelte Schnittsegmente und solche, die mit einem Randstück zusammenfallen, entfallen
    let mut unique: Vec<Seg> = Vec::new();
    for s in secs {
        if !unique.iter().any(|u| same_piece(u, &s, tol))
            && !boundary.iter().any(|b| same_piece(b, &s, tol))
        {
            unique.push(s);
        }
    }

    let mut nodes: Vec<DVec3> = Vec::new();
    let mut hes: Vec<He> = Vec::new();
    for s in &boundary {
        let (from, to) = (
            node_of(&mut nodes, s.start(), tol),
            node_of(&mut nodes, s.end(), tol),
        );
        hes.push(He {
            seg: s.clone(),
            from,
            to,
            twin: None,
        });
    }
    for s in &unique {
        let (from, to) = (
            node_of(&mut nodes, s.start(), tol),
            node_of(&mut nodes, s.end(), tol),
        );
        let i = hes.len();
        hes.push(He {
            seg: s.clone(),
            from,
            to,
            twin: Some(i + 1),
        });
        hes.push(He {
            seg: s.reversed(),
            from: to,
            to: from,
            twin: Some(i),
        });
    }
    let mut out_of: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (i, h) in hes.iter().enumerate() {
        out_of[h.from].push(i);
    }
    let frames: Vec<(DVec3, DVec3)> = nodes.iter().map(|p| node_frame(face, *p, tol)).collect();

    // Nachfolger jeder Halbkante über das Drehsystem
    let mut next = vec![usize::MAX; hes.len()];
    for (h, he) in hes.iter().enumerate() {
        let (n, e1) = frames[he.to];
        let ang_twin = angle(n, e1, -he.seg.tangent_at(1.0));
        let mut best: Option<(usize, f64)> = None;
        let mut tie = false;
        for &o in &out_of[he.to] {
            let ang_o = angle(n, e1, hes[o].seg.tangent_at(0.0));
            let mut d = (ang_twin - ang_o).rem_euclid(TAU);
            // gleiche Richtung wie die Gegenkante: erst nach einer vollen Drehung
            if !(1e-9..=TAU - 1e-9).contains(&d) {
                d = TAU;
            }
            match best {
                None => best = Some((o, d)),
                Some((_, bd)) => {
                    if (d - bd).abs() < 1e-9 {
                        tie = true;
                    } else if d < bd {
                        best = Some((o, d));
                        tie = false;
                    }
                }
            }
        }
        match best {
            // Rückkehr über das eigene Schnittstück: freies Ende im Inneren der Fläche
            Some((o, _)) if !tie && he.twin != Some(o) => next[h] = o,
            _ => return Err(BooleanError::AmbiguousContact),
        }
    }
    let mut prev = vec![usize::MAX; hes.len()];
    for (h, &nx) in next.iter().enumerate() {
        if prev[nx] != usize::MAX {
            return Err(BooleanError::AmbiguousContact);
        }
        prev[nx] = h;
    }

    // Umläufe kennzeichnen
    let mut cycle = vec![usize::MAX; hes.len()];
    let mut n_cycles = 0;
    for start in 0..hes.len() {
        if cycle[start] != usize::MAX {
            continue;
        }
        let mut h = start;
        loop {
            cycle[h] = n_cycles;
            h = next[h];
            if h == start {
                break;
            }
            if cycle[h] != usize::MAX {
                return Err(BooleanError::AmbiguousContact);
            }
        }
        n_cycles += 1;
    }

    // Nahtstücke, deren Seiten in verschiedenen Umläufen liegen, trennen nichts: Kante entfernen
    // und die beiden Umläufe zusammenführen.
    let mut removed = vec![false; hes.len()];
    let mut parent: Vec<usize> = (0..n_cycles).collect();
    let nb = boundary.len();
    for i in 0..nb {
        for j in (i + 1)..nb {
            if removed[i] || removed[j] {
                continue;
            }
            let (a, b) = (&hes[i], &hes[j]);
            if a.from != b.to
                || a.to != b.from
                || a.seg.at(0.5).distance(b.seg.at(0.5)) > tol.linear * 4.0
            {
                continue;
            }
            let (ci, cj) = (find(&mut parent, cycle[i]), find(&mut parent, cycle[j]));
            if ci == cj {
                continue;
            }
            let (pi, pj) = (prev[i], prev[j]);
            let (ni, nj) = (next[i], next[j]);
            next[pi] = nj;
            prev[nj] = pi;
            next[pj] = ni;
            prev[ni] = pj;
            removed[i] = true;
            removed[j] = true;
            parent[ci] = cj;
        }
    }

    // Freie Enden, die erst durch das Entfernen der Naht entstehen (Schnitt endet auf der Naht)
    let mut degree = vec![0usize; nodes.len()];
    for (h, he) in hes.iter().enumerate() {
        if removed[h] || he.twin.is_some_and(|t| t < h) {
            continue;
        }
        degree[he.from] += 1;
        degree[he.to] += 1;
    }
    if degree.contains(&1) {
        return Err(BooleanError::AmbiguousContact);
    }

    // Umläufe
    let mut visited = removed.clone();
    let mut walks: Vec<Vec<Seg>> = Vec::new();
    for start in 0..hes.len() {
        if visited[start] {
            continue;
        }
        let mut walk = Vec::new();
        let mut h = start;
        loop {
            visited[h] = true;
            walk.push(hes[h].seg.clone());
            h = next[h];
            if h == start {
                break;
            }
            if visited[h] || walk.len() > hes.len() {
                return Err(BooleanError::AmbiguousContact);
            }
        }
        walks.push(walk);
    }

    // Umläufe klassifizieren: positive = Außenkontur, negative = Loch
    let wrap_v = matches!(face.surface, Surface::Torus { .. });
    let polys: Vec<Vec<DVec2>> = walks
        .iter()
        .map(|w| points_to_uv(&face.surface, face.same_sense, &loop_points(w)))
        .collect();
    for p in &polys {
        // Ein Umlauf, der die Fläche ohne Nahtkante umrundet, wäre im Parameterraum nicht
        // geschlossen (Sprung um 2π zwischen letztem und erstem Punkt).
        let (Some(first), Some(last)) = (p.first(), p.last()) else {
            return Err(BooleanError::AmbiguousContact);
        };
        if p.len() < 2
            || (face.surface.is_periodic() && (last.x - first.x).abs() > PI)
            || (wrap_v && (last.y - first.y).abs() > PI)
        {
            return Err(BooleanError::AmbiguousContact);
        }
    }
    let areas: Vec<f64> = polys
        .iter()
        .map(|p| oriented_area(p, face.same_sense))
        .collect();
    let positive: Vec<usize> = (0..walks.len()).filter(|&i| areas[i] > 0.0).collect();
    if positive.is_empty() {
        return Err(BooleanError::AmbiguousContact);
    }
    let mut regions: Vec<Region> = positive
        .iter()
        .map(|&i| Region {
            outer: walks[i].clone(),
            holes: Vec::new(),
        })
        .collect();

    let periodic: &[f64] = &[-TAU, 0.0, TAU];
    let u_shifts: &[f64] = if face.surface.is_periodic() {
        periodic
    } else {
        &[0.0]
    };
    let v_shifts: &[f64] = if wrap_v { periodic } else { &[0.0] };
    let sgn = if face.same_sense { 1 } else { -1 };
    for i in (0..walks.len()).filter(|&i| areas[i] <= 0.0) {
        let candidates: Vec<DVec3> = loop_points(&walks[i])
            .into_iter()
            .chain(walks[i].iter().map(|s| s.at(0.5)))
            .collect();
        let mut owner: Option<(usize, f64)> = None;
        for (ri, &pi) in positive.iter().enumerate() {
            // Bezugspunkt des Lochs, der nicht auf dem Rand des Kandidaten liegt; fällt das Loch
            // ganz mit dem Kandidaten zusammen (dessen Gegenumlauf), scheidet er aus.
            let Some(q) = candidates.iter().find(|q| {
                walks[pi]
                    .iter()
                    .all(|s| s.closest(**q).1 > tol.linear * 10.0)
            }) else {
                continue;
            };
            let (u, v) = face.surface.project(*q);
            let poly = &polys[pi];
            let w: i32 = u_shifts
                .iter()
                .flat_map(|du| {
                    v_shifts
                        .iter()
                        .map(move |dv| winding(poly, DVec2::new(u + du, v + dv)))
                })
                .sum();
            if w * sgn >= 1 && owner.is_none_or(|(_, a)| areas[pi] < a) {
                owner = Some((ri, areas[pi]));
            }
        }
        match owner {
            Some((ri, _)) => regions[ri].holes.push(walks[i].clone()),
            None => return Err(BooleanError::AmbiguousContact),
        }
    }
    Ok(regions)
}
