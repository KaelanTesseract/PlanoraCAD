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
use super::roots::seg_intersections;
use super::seg::{Seg, curve_sample_count};
use super::uv::{
    FaceGeom, check_segs_on_surface, loop_points, oriented_area, points_to_uv, winding,
};
use super::{face_is_freeform, pipeline_tol, seg_is_freeform};
use crate::geometry::Surface;
use planora_math::{DVec2, DVec3, TAU, Tolerance};

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

/// `x` auf eine Periode um 0 gefaltet (bei `TAU`: Winkel auf (−π, π]).
fn wrap_period(x: f64, period: f64) -> f64 {
    x - period * (x / period).round()
}

/// Punkt eines Pols `v = pv`: analytische Flächen bei `u = 0`, Freiformflächen am Anfang ihres
/// u-Bereichs (die u-Linie ist dort ein Punkt, jeder u-Wert liefert ihn).
pub(super) fn pole_point(s: &Surface, pv: f64) -> DVec3 {
    match s {
        Surface::Nurbs(d) => d.point(d.surface().domain().0.0, pv),
        _ => s.point(0.0, pv),
    }
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
pub(super) fn seam_segments(face: &FaceGeom, boundary: &[Seg], tol: &Tolerance) -> Vec<Seg> {
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

/// Punkte, an denen das Schnittstück `s` die Naht `seam` kreuzt.
///
/// Analytische Flächen: die Naht ist eine Linie `u = const` (Winkelgrenze), wie bisher. Bei
/// Freiformflächen ist sie eine Parameterlinie `u = u0` (in u geschlossen) oder `v = v0` (in v
/// geschlossen); welche, zeigt die Lage ihrer beiden Enden im Parameterraum. Gefaltet wird mit der
/// Periode der Fläche; die Abtastung folgt bei Freiformkurven deren Gestalt.
fn seam_crossings(surface: &Surface, s: &Seg, seam: &Seg, tol: &Tolerance) -> Vec<DVec3> {
    // (Koordinate 0 = u, 1 = v; Periode)
    let (coord, period) = match surface {
        Surface::Nurbs(_) => {
            let (a, b) = (surface.project(seam.start()), surface.project(seam.end()));
            let along_v = (a.0 - b.0).abs() <= (a.1 - b.1).abs();
            match (along_v, surface.u_period(), surface.v_period()) {
                (true, Some(p), _) => (0, p),
                (false, _, Some(p)) => (1, p),
                (_, Some(p), _) => (0, p),
                (_, _, Some(p)) => (1, p),
                _ => return Vec::new(),
            }
        }
        _ => (0, TAU),
    };
    let pick = |uv: (f64, f64)| if coord == 0 { uv.0 } else { uv.1 };
    let c_seam = pick(surface.project(seam.at(0.5)));
    let g = |f: f64| wrap_period(pick(surface.project(s.at(f))) - c_seam, period);
    let n = curve_sample_count(&s.curve, s.t0, s.t1, 720, 64);
    let mut out = Vec::new();
    let mut push = |p: DVec3| {
        if seam.closest(p).1 <= tol.linear {
            out.push(p);
        }
    };
    let fs: Vec<f64> = (0..=n).map(|i| i as f64 / n as f64).collect();
    let gs: Vec<f64> = fs.iter().map(|f| g(*f)).collect();
    // Stützstellen, die (räumlich) auf der Naht liegen: Abschnitte, in denen der Schnitt längs der
    // Naht verläuft, sind keine Kreuzungen (ihre Enden sind Kurvenenden oder Pole und damit
    // bereits Knoten).
    let on: Vec<bool> = fs
        .iter()
        .map(|f| seam.closest(s.at(*f)).1 <= tol.linear * 4.0)
        .collect();
    for i in 0..=n {
        let along = (i > 0 && on[i - 1]) || (i < n && on[i + 1]);
        if gs[i].abs() < 1e-12 && !along {
            push(s.at(fs[i]));
        }
    }
    for i in 0..n {
        let (g0, g1) = (gs[i], gs[i + 1]);
        // echter Vorzeichenwechsel, nicht der Sprung bei ±π und nicht Rauschen längs der Naht
        if g0 * g1 < 0.0 && (g0 - g1).abs() < 0.5 * period && !(on[i] && on[i + 1]) {
            let (mut lo, mut hi) = (fs[i], fs[i + 1]);
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

/// Liegt das Stück ganz auf einer Naht?
fn on_seam(seams: &[Seg], s: &Seg, tol: &Tolerance) -> bool {
    seams.iter().any(|m| {
        [s.start(), s.at(0.5), s.end()]
            .iter()
            .all(|p| m.closest(*p).1 <= tol.linear * 4.0)
    })
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
        .find(|pv| pole_point(&face.surface, *pv).distance(p) <= tol.linear * 4.0);
    let n = match pole {
        // Freiformfläche: die Normale am Pol (dort aus einem leicht versetzten Punkt, siehe
        // `NurbsSurfaceData::normal`); der Rahmen der Freiformfläche hat keine Achse.
        Some(pv) if matches!(face.surface, Surface::Nurbs(_)) => {
            let (u, _) = face.surface.project(p);
            face.surface.normal(u, pv) * sgn
        }
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

/// Sind `a` und `b` dasselbe Stück (gleiche Enden, Mitte innerhalb `4 · tol.linear`)?
///
/// Mit Freiformbeteiligung (`freeform`, `tol` ist dann die Knotentoleranz) strenger: Mitte und
/// Viertelpunkte von `b` liegen höchstens `tol.linear` neben `a` (Abstand zur Kurve). Die
/// Mittenprobe mit `4 · tol.linear` (8e-5) legte sonst einen flachen Bogen mit seiner Sehne
/// zusammen (Übergabe Task 9: Quader `[3 − d, 13] × [−5, 5]²` gegen einen Freiformzylinder r = 3,
/// d bis 5e-5): Der Bogen fehlte in der einen Fläche, während die Nachbarfläche ihn behielt.
/// Darunter (Pfeilhöhe bis zur Knotentoleranz) gilt ein Stück als Randstück; dass das Ergebnis
/// dann zusammenpasst, prüft `check_freeform_result` nach dem Vernähen.
fn same_piece(a: &Seg, b: &Seg, tol: &Tolerance, freeform: bool) -> bool {
    let ends = (a.start().distance(b.start()) <= tol.linear
        && a.end().distance(b.end()) <= tol.linear)
        || (a.start().distance(b.end()) <= tol.linear && a.end().distance(b.start()) <= tol.linear);
    if !freeform {
        return ends && a.at(0.5).distance(b.at(0.5)) <= tol.linear * 4.0;
    }
    ends && [0.25, 0.5, 0.75]
        .iter()
        .all(|f| a.closest(b.at(*f)).1 <= tol.linear)
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
///
/// Freiformbeteiligung (Fläche, Randkante oder Schnitt): Rand und Schnitte müssen auf der Fläche
/// liegen (sonst [`BooleanError::Unsupported`]); Knoten und Teilungen nutzen die Toleranz
/// [`pipeline_tol`](super::pipeline_tol).
pub fn split_face(
    face: &FaceGeom,
    sections: &[Seg],
    tol: &Tolerance,
) -> Result<Vec<Region>, BooleanError> {
    check_segs_on_surface(
        &face.surface,
        face.loops.iter().flatten().chain(sections),
        tol,
    )?;
    let freeform = face_is_freeform(face) || sections.iter().any(seg_is_freeform);
    let ptol = pipeline_tol(tol, freeform);
    let tol = &ptol;
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
    let seams = seam_segments(face, &boundary, tol);
    for seam in &seams {
        for s in sections {
            all_pts.extend(seam_crossings(&face.surface, s, seam, tol));
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

    // Doppelte Schnittsegmente und solche, die mit einem Randstück zusammenfallen, entfallen.
    // Fällt ein Schnitt mit einem Nahtstück zusammen, ist die Naht dort ein echter Schnitt: Das
    // Nahtstück wird markiert und später nicht über die Naht hinweg zusammengeführt.
    let mut unique: Vec<Seg> = Vec::new();
    let mut cut_seam: Vec<bool> = vec![false; boundary.len()];
    for s in secs {
        let mut on_boundary = false;
        for (bi, b) in boundary.iter().enumerate() {
            if same_piece(b, &s, tol, freeform) {
                on_boundary = true;
                cut_seam[bi] = true;
            }
        }
        if !on_boundary && !unique.iter().any(|u| same_piece(u, &s, tol, freeform)) {
            unique.push(s);
        }
    }
    // Zusammenführbar sind nur Nahtstücke ohne Schnitt (gewöhnliche Randstücke nie).
    let mergeable: Vec<bool> = boundary
        .iter()
        .zip(&cut_seam)
        .map(|(b, &cut)| !cut && on_seam(&seams, b, tol))
        .collect();

    // Schnitte, die sich im Inneren kreuzen, hätte der Aufrufer teilen müssen. Mit
    // Freiformbeteiligung fallen gemeinsame Enden zweier Stücke nur bis auf die Knotentoleranz
    // zusammen; ein Treffer dort ist das gemeinsame Ende, keine Kreuzung im Inneren: Zählt nur,
    // wer weiter als der Knotenabstand von den Enden beider Stücke liegt. Analytisch unverändert.
    let node_gap = tol.linear * 4.0;
    for (i, a) in unique.iter().enumerate() {
        for b in &unique[i + 1..] {
            let inner = |f: f64| f > 1e-6 && f < 1.0 - 1e-6;
            let inner_at = |s: &Seg, f: f64| {
                inner(f)
                    && (!freeform || {
                        let p = s.at(f);
                        p.distance(s.start()) > node_gap && p.distance(s.end()) > node_gap
                    })
            };
            if seg_intersections(a, b, tol)
                .iter()
                .any(|&(fa, fb)| inner_at(a, fa) && inner_at(b, fb))
            {
                return Err(BooleanError::AmbiguousContact);
            }
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
            if removed[i] || removed[j] || !mergeable[i] || !mergeable[j] {
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
    let (u_period, v_period) = (face.surface.u_period(), face.surface.v_period());
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
            || u_period.is_some_and(|t| (last.x - first.x).abs() > 0.5 * t)
            || v_period.is_some_and(|t| (last.y - first.y).abs() > 0.5 * t)
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

    let shifts = |period: Option<f64>| match period {
        Some(t) => vec![-t, 0.0, t],
        None => vec![0.0],
    };
    let (u_shifts, v_shifts) = (shifts(u_period), shifts(v_period));
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
