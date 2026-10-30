//! Nähte, Pole und Entartungen (STEP-Import, Aufgabe 6): Bereinigung der Rohform
//! ([`RawBody`]), bevor daraus ein Körper des Kernels wird.
//!
//! Der Kernel kennt keine Kanten der Länge null, keinen Pol als Schleife (`VERTEX_LOOP`) und
//! keine periodische Fläche ohne Naht; STEP-Dateien enthalten alles drei. Der Leser
//! - zieht Ecken, die höchstens `weld_tol` neben einem Pol ihrer Fläche liegen (Kugelpol,
//!   Kegelspitze), genau auf den Pol ([`snap_poles`]); die Kanten bekommen dort ihren Parameter
//!   neu. So erkennt der Kernel den Pol (er vergleicht `v` mit 1e-7);
//! - entfernt Kanten, deren Kurve kürzer als `weld_tol` ist ([`drop_degenerate`]), und legt ihre
//!   Ecken zusammen; `VERTEX_LOOP`s sind beim Lesen keine Ränder geworden, ihre Punkte stehen als
//!   Pole der Fläche (`pole_marks`) bereit;
//! - fügt Zylinder-, Kegel- und Kugelflächen ohne Naht eine Naht hinzu ([`insert_seams`], P9):
//!   zwischen zwei vollen Kreisen, von einem Kreis zum Pol, (Kugel ohne Rand oder mit beiden
//!   Polen zwischen ihren Löchern) von Pol zu Pol oder (Kugelfläche, deren Rand einen Pol
//!   berührt und um die Kugel herumläuft) vom inneren zum berührten Pol, immer mit Abstand zum
//!   übrigen Rand; andere Flächen ohne Naht werden ausgelassen. Wo nur der Umlaufsinn die Seite
//!   einer Kugelfläche bestimmt (kein Pol genannt, oder ein Pol liegt im Rand), wird das
//!   vermerkt (`side_guessed`);
//! - prüft, dass jeder Rand, der in die Nähe eines Pols kommt, genau durch ihn läuft
//!   ([`check_poles`]); sonst sähe der Kernel dort keinen Pol und der Körper käme mit falschem
//!   Volumen an.

use crate::read::geom::Skip;
use crate::read::topo::{RawBody, RawEdge, RawHalfEdge, at_line};
use planora_i18n::tr;
use planora_kernel::blend::topo::role_label;
use planora_kernel::boolean::{oriented_area, points_to_uv};
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
pub(crate) fn edge_length(e: &RawEdge) -> f64 {
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
pub(crate) fn snap_poles(raw: &mut RawBody, tol: f64) {
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

/// Teilt jede Kante einer Fläche mit Polen, die mitten durch einen Pol läuft (etwa ein Großkreis
/// mit seiner Ecke am Äquator oder an einem Pol), genau dort: Der Pol wird eine Ecke. Sonst
/// läge er zwischen zwei Abtastpunkten, und u spränge dort um eine halbe Periode.
pub(crate) fn split_at_poles(raw: &mut RawBody, tol: f64) {
    let near = tol.max(linear());
    // Je Durchlauf alle Kanten einmal absuchen und je Kante höchstens eine Teilung sammeln
    // (eine Kante durch beide Pole bekommt die zweite im nächsten Durchlauf); so lange, bis
    // nichts mehr zu teilen ist. Jede Teilung setzt eine Ecke auf einen Pol, die Zahl der
    // Durchläufe ist also durch die Pole je Kante begrenzt (die Schranke nur zur Sicherheit).
    for _ in 0..16 {
        let mut found: Vec<(usize, f64, DVec3)> = Vec::new();
        let mut done = vec![false; raw.edges.len()];
        for f in &raw.faces {
            let poles = pole_points(&f.surface);
            if poles.is_empty() {
                continue;
            }
            for he in f.loops.iter().flatten() {
                if done[he.edge] {
                    continue;
                }
                let e = &raw.edges[he.edge];
                for (_, q) in &poles {
                    if raw.vertices[e.start].distance(*q) <= near
                        || raw.vertices[e.end].distance(*q) <= near
                    {
                        continue;
                    }
                    let (lo, hi) = (e.t0.min(e.t1), e.t0.max(e.t1));
                    let mut t = e.curve.project(*q);
                    if let Some(per) = e.curve.period() {
                        t += per * ((lo - t) / per).ceil();
                    }
                    if t > lo && t < hi && e.curve.point(t).distance(*q) <= near {
                        found.push((he.edge, t, *q));
                        done[he.edge] = true;
                        break;
                    }
                }
            }
        }
        if found.is_empty() {
            return;
        }
        for (k, t, q) in found {
            let v = split_edge(raw, k, t);
            raw.vertices[v] = q;
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
                    return Err(at_line(edges[k].line, tr!("step-r-edge-zero")));
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
pub(crate) fn check_poles(raw: &RawBody, tol: f64) -> Result<(), Skip> {
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
                        return Err(Skip(tr!(
                            "step-r-rim-not-through-pole",
                            role = role_label(&f.tag.role)
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Name der Flächenart für die Meldung nach P9.
fn kind_name(s: &Surface) -> String {
    match s {
        Surface::Plane { .. } => tr!("kernel-surface-plane"),
        Surface::Cylinder { .. } => tr!("kernel-surface-cylinder"),
        Surface::Cone { .. } => tr!("kernel-surface-cone"),
        Surface::Sphere { .. } => tr!("kernel-surface-sphere"),
        Surface::Torus { .. } => tr!("kernel-surface-torus"),
        Surface::Nurbs(_) => tr!("kernel-surface-freeform"),
    }
}

/// Fügt Flächen ohne Naht eine Naht hinzu (P9). Eine Fläche hat keine Naht, wenn sie keinen Rand
/// hat oder Ränder, die sich um ihre Achse winden (im Parameterraum nach der Regel des Kernels
/// aufgerollt: Anfang und Ende liegen eine Periode auseinander). Nur bei Zylinder, Kegel und
/// Kugel wird eingefügt:
/// - zwei Kreisschleifen (die untere läuft in Richtung der Außennormale gegen den Uhrzeigersinn,
///   die obere andersherum): Naht entlang der Mantellinie (Kugel: Meridian) beim Winkel `u_s`;
/// - eine Kreisschleife und auf ihrer Seite ein Pol (Kugelkappe, Kegel mit Spitze): Naht vom
///   Kreis zum Pol, der Pol wird eine Ecke. Nennt die Datei Pole (`VERTEX_LOOP`), muss es einer
///   davon sein (sonst „verdreht“); ohne sie ist die Seite einer Kugelkappe nur geraten
///   (`side_guessed`, siehe `build_solid`);
/// - Kugel ohne Rand: Naht im Meridian bei `u = 0` von Pol zu Pol.
///
/// `u_s` hält Abstand zu allem anderen Rand der Fläche (`free_angle`: Löcher, Mantellinien
/// von Aussparungen, Kreuzungen); die Kreisschleifen werden dort von der Mantellinie geschnitten
/// (`vertex_on`) und, wo keine Ecke liegt, geteilt (in allen Schleifen, die die Kante
/// benutzen). Gibt es keinen solchen Winkel: „Der Fläche „…“ lässt sich keine Naht hinzufügen.“
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
    let role = f.tag.role.clone();
    let line = f.line;
    let same_sense = f.same_sense;
    let periods = [surface.u_period(), surface.v_period()];
    let no_rim = || at_line(line, tr!("step-r-face-no-rim"));
    let no_seam = || Skip(tr!("step-r-surface-no-seam", type = kind_name(&surface)));
    // Ränder, die sich um die Achse winden, aber nicht zu einer Fläche ohne Naht passen (etwa
    // ein Kreis falsch herum): derselbe Grund wie bei der Prüfung der Schleifen.
    let not_closing = || Skip(tr!("step-r-rim-not-closing", role = role_label(&role)));
    let twisted = || Skip(tr!("step-r-face-twisted", role = role_label(&role)));
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
        .map(|lp| points_to_uv(&surface, same_sense, &raw.loop_points(lp)))
        .collect();
    // Windung in u: für Ränder ohne Pol aus der verfeinerten Spur (nahe einem Pol springt u
    // zwischen den groben Abtastpunkten, das Aufrollen wäre dort mehrdeutig).
    let pole_vs = surface.pole_vs();
    let windings: Vec<[i64; 2]> = polys
        .iter()
        .zip(&f.loops)
        .map(|(p, lp)| {
            let through_pole = p.iter().any(|q| pole_vs.iter().any(|pv| at_pole(q.y, *pv)));
            let wu = match periods[0] {
                Some(per) if !through_pole && !pole_vs.is_empty() => {
                    let tr = u_track(raw, &surface, lp);
                    match (tr.first(), tr.last()) {
                        (Some(a), Some(b)) => ((b.2 - a.2) / per).round() as i64,
                        _ => 0,
                    }
                }
                _ => winding(p, 0, periods[0]),
            };
            [wu, winding(p, 1, periods[1])]
        })
        .collect();
    let winding_loops: Vec<usize> = (0..windings.len())
        .filter(|&k| windings[k] != [0, 0])
        .collect();
    if !f.loops.is_empty() && winding_loops.is_empty() {
        return sphere_piece(raw, i, &polys, tol);
    }
    // Ein Rand ohne Naht ist ein Kreis um die Achse; einer, der durch einen Pol läuft und sich
    // trotzdem windet, läuft um die Kugel herum: Die Fläche enthält den anderen Pol im Inneren
    // (Kugel ohne Achtel) und bekommt eine Naht vom berührten zum inneren Pol. Sonst erreicht er
    // den anderen Pol nicht. Schließt er sich mit dem umgekehrten Sinn der Fläche, liegt es am
    // Sinn.
    let poles = surface.pole_vs();
    for &k in &winding_loops {
        if polys[k]
            .iter()
            .any(|q| poles.iter().any(|pv| at_pole(q.y, *pv)))
        {
            let mut seen = std::collections::HashSet::new();
            let has_seam = f.loops.iter().flatten().any(|he| !seen.insert(he.edge));
            if matches!(surface, Surface::Sphere { .. })
                && winding_loops.len() == 1
                && windings[k][0].abs() == 1
                && windings[k][1] == 0
                && !has_seam
            {
                return sphere_around_pole(raw, i, k, tol);
            }
            let other = points_to_uv(&surface, !same_sense, &raw.loop_points(&f.loops[k]));
            return Err(if winding(&other, 0, periods[0]) == 0 {
                twisted()
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
        // Gegenläufige Ränder um die Achse, oder ein Rand um die Achse und ein Pol (Kappe einer
        // Freiform-Drehfläche): eine heile Fläche ohne Naht. Sonst ein Fehler in den Rändern.
        let cap = winding_loops.len() == 1 && sum[1] == 0 && !poles.is_empty();
        return Err(if sum == [0, 0] || cap {
            no_seam()
        } else {
            not_closing()
        });
    }
    let s: i64 = if same_sense { 1 } else { -1 };
    let marks = f.pole_marks.clone();
    let holes: Vec<usize> = (0..f.loops.len())
        .filter(|k| !winding_loops.contains(k))
        .collect();
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
            let level = polys[a].iter().map(|q| q.y).sum::<f64>() / polys[a].len() as f64;
            let pole = poles
                .iter()
                .copied()
                .filter(|&pv| if above { pv > level } else { pv < level })
                .min_by(|x, y| (x - level).abs().total_cmp(&(y - level).abs()));
            let Some(pv) = pole else {
                return Err(not_closing());
            };
            // Nennt die Datei Pole (`VERTEX_LOOP`), muss die Naht zu einem davon laufen; sonst
            // ist die Seite nur aus dem Umlaufsinn gewählt (bei der Kugel zweideutig).
            let pole_point = surface.point(0.0, pv);
            let near = tol.max(linear());
            if !marks.is_empty() && !marks.iter().any(|m| m.distance(pole_point) <= near) {
                return Err(twisted());
            }
            let guessed = marks.is_empty() && matches!(surface, Surface::Sphere { .. });
            let Some(u_s) = free_angle(raw, i, &[a], &holes, tol) else {
                return Err(no_room(&role));
            };
            let va = vertex_on(raw, i, a, u_s, tol).ok_or_else(|| no_room(&role))?;
            let rim = rotated(raw, &raw.faces[i].loops[a], va).ok_or_else(|| no_room(&role))?;
            let vp = push_vertex(raw, surface.point(u_s, pv));
            let seam = push_seam(raw, &surface, u_s, va, vp, line)?;
            let mut lp = rim;
            lp.extend([fwd(seam), bwd(seam)]);
            set_seam_loop(raw, i, &[a], lp);
            raw.faces[i].side_guessed = guessed;
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
            let Some(u_s) = free_angle(raw, i, &[a, b], &holes, tol) else {
                return Err(no_room(&role));
            };
            let va = vertex_on(raw, i, a, u_s, tol).ok_or_else(|| no_room(&role))?;
            let vb = vertex_on(raw, i, b, u_s, tol).ok_or_else(|| no_room(&role))?;
            let f = &raw.faces[i];
            let (Some(rim_a), Some(rim_b)) =
                (rotated(raw, &f.loops[a], va), rotated(raw, &f.loops[b], vb))
            else {
                return Err(no_room(&role));
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

/// Kugelfläche ohne windenden Rand: Hat sie eine Naht (eine Kante zweimal) oder einen Rand durch
/// einen Pol, ist alles bestimmt. Sonst ist sie ein Stück ohne Naht und ohne Pol im Rand: Laufen
/// alle Ränder als Löcher (im Uhrzeigersinn um die Außennormale), enthält die Fläche beide Pole
/// der Kugel und bekommt eine Naht von Pol zu Pol mit Abstand zu den Löchern; sonst ist sie ein
/// gewöhnliches Stück ohne Pol. In beiden Fällen folgt ihre Seite nur aus dem Umlaufsinn
/// (`side_guessed`), außer die Datei nennt die Pole (`VERTEX_LOOP`): Dann müssen sie in der
/// Fläche liegen, sonst ist sie verdreht.
fn sphere_piece(raw: &mut RawBody, i: usize, polys: &[Vec<DVec2>], tol: f64) -> Result<(), Skip> {
    let f = &raw.faces[i];
    let Surface::Sphere { .. } = f.surface else {
        return Ok(());
    };
    let surface = f.surface.clone();
    let poles = surface.pole_vs();
    let touches = polys
        .iter()
        .flatten()
        .any(|q| poles.iter().any(|pv| at_pole(q.y, *pv)));
    let mut seen = std::collections::HashSet::new();
    let has_seam = f.loops.iter().flatten().any(|he| !seen.insert(he.edge));
    if has_seam {
        return Ok(());
    }
    let twisted = || Skip(tr!("step-r-face-twisted", role = role_label(&f.tag.role)));
    let near = tol.max(linear());
    let pole_points = pole_points(&surface);
    if touches {
        // Ein Pol im Rand entscheidet die Seite nicht: Auch die Gegenfläche ist darstellbar
        // (Keil, Halbkugel mit der Achse in der Schnittfläche; beim Achtel die Kugel ohne
        // Achtel mit einer Naht zum inneren Pol). Die Seite folgt nur aus dem Umlaufsinn. Ein
        // Pol, den der Rand nicht berührt, liegt außerhalb (der Rand windet sich nicht um
        // ihn): Eine Marke dort oder anderswo ist verdreht; eine Marke am berührten Pol sagt
        // nichts.
        for m in &f.pole_marks {
            let at = pole_points.iter().position(|(_, q)| q.distance(*m) <= near);
            let touched =
                at.is_some_and(|j| polys.iter().flatten().any(|q| at_pole(q.y, poles[j])));
            if !touched {
                return Err(twisted());
            }
        }
        raw.faces[i].side_guessed = true;
        return Ok(());
    }
    let marks_at_poles = f
        .pole_marks
        .iter()
        .all(|m| pole_points.iter().any(|(_, q)| q.distance(*m) <= near));
    let all_holes = polys.iter().all(|p| oriented_area(p, f.same_sense) < 0.0);
    let guessed = f.pole_marks.is_empty();
    if !all_holes {
        // Gewöhnliches Stück: Ein genannter Pol läge nicht in ihm.
        if !guessed {
            return Err(twisted());
        }
        raw.faces[i].side_guessed = true;
        return Ok(());
    }
    if !marks_at_poles {
        return Err(twisted());
    }
    let role = f.tag.role.clone();
    let line = f.line;
    let holes: Vec<usize> = (0..f.loops.len()).collect();
    let u_s = free_angle(raw, i, &[], &holes, tol).ok_or_else(|| no_room(&role))?;
    let south = push_vertex(raw, surface.point(u_s, -FRAC_PI_2));
    let north = push_vertex(raw, surface.point(u_s, FRAC_PI_2));
    let seam = push_seam(raw, &surface, u_s, south, north, line)?;
    set_seam_loop(raw, i, &[], vec![fwd(seam), bwd(seam)]);
    raw.faces[i].side_guessed = guessed;
    Ok(())
}

fn no_room(role: &str) -> Skip {
    Skip(tr!("step-r-no-room-seam", role = role_label(role)))
}

/// Anteile der Sprungstrecke am Pol, an denen die Naht zum inneren Pol versucht wird.
const AROUND_POLE_FRACTIONS: [f64; 7] = [0.5, 0.25, 0.75, 0.125, 0.375, 0.625, 0.875];

/// Kugelfläche, deren Rand `k` genau einen Pol berührt und sich einmal um die Achse windet: Sie
/// enthält den anderen Pol im Inneren (Kugel ohne Achtel). Sie bekommt eine Naht vom inneren
/// zum berührten Pol, in der Lücke, die der Rand am berührten Pol lässt (die Sprungstrecke im
/// Parameterraum nach der Regel des Kernels), mit Abstand zum übrigen Rand und zu Löchern. Die
/// Schleife hat dann die Form `[Naht, Rand ab dem Pol, Naht rückwärts]`. Marken: eine am inneren
/// Pol bestätigt die Seite, eine am berührten Pol sagt nichts, jede andere ist verdreht; ohne
/// Bestätigung folgt die Seite nur aus dem Umlaufsinn (`side_guessed`).
fn sphere_around_pole(raw: &mut RawBody, i: usize, k: usize, tol: f64) -> Result<(), Skip> {
    let f = &raw.faces[i];
    let surface = f.surface.clone();
    let role = f.tag.role.clone();
    let line = f.line;
    let near = tol.max(linear());
    let per = surface.u_period().unwrap_or(std::f64::consts::TAU);
    let not_closing = || Skip(tr!("step-r-rim-not-closing", role = role_label(&role)));
    let twisted = || Skip(tr!("step-r-face-twisted", role = role_label(&role)));
    let poles = pole_points(&surface);
    let lp = f.loops[k].clone();
    // Der berührte Pol ist eine Ecke der Schleife (nach dem Teilen an den Polen), der andere nicht.
    let at_vertex = |q: DVec3| {
        lp.iter()
            .position(|he| raw.vertices[raw.he_start(*he)].distance(q) <= near)
    };
    let touched: Vec<Option<usize>> = poles.iter().map(|(_, q)| at_vertex(*q)).collect();
    let (Some(t), Some(inner)) = (
        (0..poles.len()).find(|&j| touched[j].is_some()),
        (0..poles.len()).find(|&j| touched[j].is_none()),
    ) else {
        return Err(not_closing());
    };
    let mut confirmed = false;
    for m in &f.pole_marks {
        if m.distance(poles[inner].1) <= near {
            confirmed = true;
        } else if m.distance(poles[t].1) > near {
            return Err(twisted());
        }
    }
    let j = touched[t].expect("touched pole is a corner");
    let vt = raw.he_start(lp[j]);
    let rim: Vec<RawHalfEdge> = lp[j..].iter().chain(&lp[..j]).copied().collect();
    // Sprungstrecke am berührten Pol: die letzten beiden Polpunkte des Polygons (die Schleife
    // beginnt und endet dort).
    let poly = points_to_uv(&surface, f.same_sense, &raw.loop_points(&rim));
    let pv = poles[t].0;
    let jump = poly
        .windows(2)
        .rev()
        .find(|w| at_pole(w[0].y, pv) && at_pole(w[1].y, pv))
        .map(|w| (w[0].x, w[1].x - w[0].x));
    let Some((u_a, delta)) = jump else {
        return Err(not_closing());
    };
    // Löcher (die übrigen Schleifen) mit Abstand; der Rand selbst darf die Mantellinie außer
    // am Pol nicht kreuzen und ihr nicht nahe kommen.
    let holes: Vec<usize> = (0..f.loops.len()).filter(|&h| h != k).collect();
    let (hole_ranges, hole_margin) = hole_ranges(raw, i, &holes, tol, per);
    let margin = SEAM_CLEARANCE * per;
    let wrap = |x: f64| x.rem_euclid(per);
    let track = u_track(raw, &surface, &rim);
    let pole_pts: Vec<DVec3> = poles.iter().map(|(_, q)| *q).collect();
    let runs: Vec<Vec<(usize, f64, f64)>> = track
        .split(|&(h, s, _)| {
            let p = he_point(raw, rim[h], s);
            pole_pts.iter().any(|q| q.distance(p) <= near)
        })
        .filter(|r| !r.is_empty())
        .map(|r| r.to_vec())
        .collect();
    let clear = |u_s: f64| {
        for &(lo, hi) in &hole_ranges {
            if hi - lo >= per - 2.0 * hole_margin
                || wrap(u_s - (lo - hole_margin)) <= hi - lo + 2.0 * hole_margin
            {
                return false;
            }
        }
        runs.iter().all(|r| {
            crossings(r, u_s, per).is_empty()
                && r.iter().all(|t| {
                    let d = wrap(t.2 - u_s);
                    d.min(per - d) >= margin
                })
        })
    };
    let Some(u_s) = AROUND_POLE_FRACTIONS
        .iter()
        .map(|frac| wrap(u_a + delta * frac))
        .find(|&u_s| clear(u_s))
    else {
        return Err(no_room(&role));
    };
    let vi = push_vertex(raw, surface.point(u_s, poles[inner].0));
    let seam = push_seam(raw, &surface, u_s, vi, vt, line)?;
    let mut lp = vec![fwd(seam)];
    lp.extend(rim);
    lp.push(bwd(seam));
    set_seam_loop(raw, i, &[k], lp);
    raw.faces[i].side_guessed = !confirmed;
    Ok(())
}

/// Genaue Bereiche der Löcher `holes` der Fläche `i` in u und der Abstand, den eine Naht von
/// ihnen halten muss: mindestens [`HOLE_CLEARANCE_TOLS`] Genauigkeiten der Datei als Winkel am
/// kleinsten Abstand zur Achse, höchstens [`MAX_HOLE_CLEARANCE`] der Periode.
fn hole_ranges(
    raw: &RawBody,
    i: usize,
    holes: &[usize],
    tol: f64,
    per: f64,
) -> (Vec<(f64, f64)>, f64) {
    let f = &raw.faces[i];
    let surface = &f.surface;
    let mut hole_margin = SEAM_CLEARANCE * per;
    let ranges: Vec<(f64, f64)> = holes
        .iter()
        .map(|&k| {
            let lp = &f.loops[k];
            let rho = raw
                .loop_points(lp)
                .iter()
                .map(|p| axis_distance(surface, *p))
                .fold(f64::INFINITY, f64::min);
            if rho > 0.0 {
                hole_margin = hole_margin.max(HOLE_CLEARANCE_TOLS * tol / rho);
            }
            u_extent(raw, surface, f.same_sense, lp, per)
        })
        .collect();
    (ranges, hole_margin.min(MAX_HOLE_CLEARANCE * per))
}

/// Abstand (Anteil der Periode), den die Naht von anderem Rand der Fläche halten muss.
const SEAM_CLEARANCE: f64 = 1e-6;

/// Abstand der Naht von einem Loch: mindestens so viele Genauigkeiten der Datei (als Winkel am
/// kleinsten Abstand des Lochs zur Achse), höchstens [`MAX_HOLE_CLEARANCE`] der Periode.
const HOLE_CLEARANCE_TOLS: f64 = 10.0;
const MAX_HOLE_CLEARANCE: f64 = 0.01;

/// Abstand eines Punkts von der Achse einer Dreh-Fläche (Zylinder, Kegel, Kugel).
fn axis_distance(surface: &Surface, p: DVec3) -> f64 {
    let f = surface.frame();
    let d = p - f.origin;
    (d - f.z * d.dot(f.z)).length()
}

/// Bereich einer Schleife in u (fortlaufend aufgerollt), mit genau gesuchten Extremwerten: Ein
/// Kreis auf einer Kugel hat sein größtes u mitten auf der Kante, Abtastpunkte allein lägen
/// darunter. Läuft die Schleife durch einen Pol, gilt die Regel des Kernels (`points_to_uv`).
fn u_extent(
    raw: &RawBody,
    surface: &Surface,
    same_sense: bool,
    lp: &[RawHalfEdge],
    per: f64,
) -> (f64, f64) {
    let poles = surface.pole_vs();
    let poly = points_to_uv(surface, same_sense, &raw.loop_points(lp));
    if poly
        .iter()
        .any(|q| poles.iter().any(|pv| at_pole(q.y, *pv)))
    {
        return poly
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| {
                (lo.min(q.x), hi.max(q.x))
            });
    }
    let track = u_track(raw, surface, lp);
    let refine = |j: usize, sign: f64| -> f64 {
        let (he_i, s, u0) = track[j];
        let he = lp[he_i];
        // Nachbarn auf derselben Halbkante begrenzen die Suche.
        let a0 = track[..j]
            .iter()
            .rev()
            .find(|t| t.0 == he_i && t.1 < s)
            .map_or(s, |t| t.1);
        let b0 = track[j + 1..]
            .iter()
            .find(|t| t.0 == he_i && t.1 > s)
            .map_or(s, |t| t.1);
        let (mut a, mut b) = (a0, b0);
        let u_at = |x: f64| {
            let u = surface.project(he_point(raw, he, x)).0;
            sign * (u + per * ((u0 - u) / per).round())
        };
        for _ in 0..80 {
            let m1 = a + (b - a) / 3.0;
            let m2 = b - (b - a) / 3.0;
            if u_at(m1) < u_at(m2) {
                a = m1;
            } else {
                b = m2;
            }
        }
        sign * u_at(0.5 * (a + b))
    };
    let (jmin, jmax) = track.iter().enumerate().fold((0, 0), |(lo, hi), (k, t)| {
        (
            if t.2 < track[lo].2 { k } else { lo },
            if t.2 > track[hi].2 { k } else { hi },
        )
    });
    let lo = refine(jmin, -1.0).min(track[jmin].2);
    let hi = refine(jmax, 1.0).max(track[jmax].2);
    (lo, hi)
}

/// Punkte einer Schleife mit ihrer Stelle (Index der Halbkante, Anteil entlang ihr) und `u`
/// entlang der Schleife fortlaufend entfaltet (Schleifen ohne Pol).
fn u_track(raw: &RawBody, surface: &Surface, lp: &[RawHalfEdge]) -> Vec<(usize, f64, f64)> {
    let n = EDGE_STEPS;
    let per = surface.u_period().unwrap_or(f64::INFINITY);
    // Abstand zweier Werte modulo der Periode (−½ … ½ Periode).
    let step = |a: f64, b: f64| {
        if per.is_finite() {
            (b - a) - per * ((b - a) / per).round()
        } else {
            b - a
        }
    };
    let mut out: Vec<(usize, f64, f64)> = Vec::with_capacity(lp.len() * (n + 1));
    for (j, he) in lp.iter().enumerate() {
        let u_of = |s: f64| surface.project(he_point(raw, *he, s)).0;
        // Gleichmäßig, dann dort verfeinert, wo u zwischen Nachbarn stark springt (nahe einem
        // Pol ändert sich u schnell; dort wäre das Aufrollen sonst mehrdeutig).
        let mut pts: Vec<(f64, f64)> = (0..=n)
            .map(|k| {
                let s = k as f64 / n as f64;
                (s, u_of(s))
            })
            .collect();
        loop {
            let mut refined = Vec::with_capacity(pts.len() * 2);
            let mut changed = false;
            refined.push(pts[0]);
            for w in pts.windows(2) {
                let (a, b) = (w[0], w[1]);
                if step(a.1, b.1).abs() > MAX_U_STEP
                    && b.0 - a.0 > 1e-9
                    && pts.len() < MAX_EDGE_SAMPLES
                {
                    let m = 0.5 * (a.0 + b.0);
                    refined.push((m, u_of(m)));
                    changed = true;
                }
                refined.push(b);
            }
            pts = refined;
            if !changed {
                break;
            }
        }
        for (s, mut u) in pts {
            if let Some(&(_, _, prev)) = out.last() {
                u += per * ((prev - u) / per).round();
            }
            out.push((j, s, u));
        }
    }
    out
}

/// Punkte einer Schleife in Laufrichtung, dort verfeinert, wo u schnell läuft (wie
/// [`u_track`]); für den Parameterbereich von Flächen mit Polen.
pub(crate) fn fine_loop_points(raw: &RawBody, surface: &Surface, lp: &[RawHalfEdge]) -> Vec<DVec3> {
    u_track(raw, surface, lp)
        .into_iter()
        .map(|(j, s, _)| he_point(raw, lp[j], s))
        .collect()
}

/// Größter Sprung von u zwischen zwei Punkten der Spur, bevor nachverfeinert wird.
const MAX_U_STEP: f64 = 0.25;

/// Höchstzahl der Punkte je Halbkante in [`u_track`].
const MAX_EDGE_SAMPLES: usize = 4096;

/// Zwischenpunkte je Halbkante für [`u_track`].
const EDGE_STEPS: usize = 24;

/// Punkt der Halbkante beim Anteil `s` (0 am Anfang, 1 am Ende).
fn he_point(raw: &RawBody, he: RawHalfEdge, s: f64) -> DVec3 {
    let e = &raw.edges[he.edge];
    let s = if he.forward { s } else { 1.0 - s };
    e.curve.point(e.t0 + (e.t1 - e.t0) * s)
}

/// Abschnitte der Spur, die `u = t` (bis auf Vielfache der Periode) kreuzen, halboffen gezählt.
fn crossings(track: &[(usize, f64, f64)], t: f64, per: f64) -> Vec<(usize, f64)> {
    let mut out = Vec::new();
    for w in track.windows(2) {
        let (a, b) = (w[0].2, w[1].2);
        let k = ((a.min(b) - t) / per).ceil();
        let mut x = t + k * per;
        while x <= a.max(b) {
            if (a <= x && x < b) || (b <= x && x < a) {
                out.push((w[0].0, x));
            }
            x += per;
        }
    }
    out
}

/// Ein Winkel für die Naht, der Abstand zu allem anderen Rand der Fläche hält: Jede der
/// Kreisschleifen `rims` kreuzt die Mantellinie dort genau einmal, und nicht entlang einer
/// Kante (Mantellinien von Aussparungen und Fenstern); kein Loch (`holes`) reicht an ihn heran.
/// Zuerst die Winkel der Ecken der ersten Kreisschleife (so verbindet die Naht bei den meisten
/// Exporten vorhandene Ecken), dann die Mitten der größten freien Lücken.
fn free_angle(raw: &RawBody, i: usize, rims: &[usize], holes: &[usize], tol: f64) -> Option<f64> {
    let f = &raw.faces[i];
    let surface = &f.surface;
    let per = surface.u_period()?;
    let margin = SEAM_CLEARANCE * per;
    let tracks: Vec<Vec<(usize, f64, f64)>> = rims
        .iter()
        .map(|&k| u_track(raw, surface, &f.loops[k]))
        .collect();
    // Genaue Bereiche der Löcher in u (Extremwerte auf den Kanten gesucht) und ein Abstand von
    // mindestens einigen Genauigkeiten der Datei, als Winkel am kleinsten Abstand zur Achse.
    let (hole_ranges, hole_margin) = hole_ranges(raw, i, holes, tol, per);
    let wrap = |x: f64| x.rem_euclid(per);
    let near = |x: f64, t: f64| {
        let d = wrap(x - t);
        d.min(per - d) < margin
    };
    let clear = |t: f64| {
        for &(lo, hi) in &hole_ranges {
            if hi - lo >= per - 2.0 * hole_margin {
                return false;
            }
            if wrap(t - (lo - hole_margin)) <= hi - lo + 2.0 * hole_margin {
                return false;
            }
        }
        tracks.iter().all(|tr| {
            crossings(tr, t, per).len() == 1
                && !tr.windows(2).any(|w| {
                    near(w[0].2, t)
                        && near(w[1].2, t)
                        && (w[0].2 - w[1].2).abs() < margin
                        && w[0].0 == w[1].0
                })
        })
    };
    // Kandidaten: Ecken der ersten Kreisschleife, dann Lückenmitten.
    let mut candidates: Vec<f64> = tracks
        .first()
        .map(|tr| {
            tr.iter()
                .filter(|(_, s, _)| *s == 0.0)
                .map(|t| wrap(t.2))
                .collect()
        })
        .unwrap_or_default();
    // Grenzen alles Belegten (Ecken aller Schleifen, Ränder der Löcher); die Mitten der Lücken
    // dazwischen, größte zuerst. Mitten in einem Loch fallen bei der Prüfung heraus.
    let mut marks: Vec<f64> = hole_ranges
        .iter()
        .flat_map(|&(lo, hi)| [wrap(lo), wrap(hi)])
        .collect();
    for lp in &f.loops {
        for he in lp {
            marks.push(wrap(surface.project(raw.vertices[raw.he_start(*he)]).0));
        }
    }
    marks.sort_by(f64::total_cmp);
    let mut gaps: Vec<(f64, f64)> = (0..marks.len())
        .map(|k| {
            let a = marks[k];
            let b = if k + 1 < marks.len() {
                marks[k + 1]
            } else {
                marks[0] + per
            };
            (b - a, 0.5 * (a + b))
        })
        .collect();
    gaps.sort_by(|a, b| b.0.total_cmp(&a.0));
    candidates.extend(gaps.iter().map(|g| wrap(g.1)));
    candidates.into_iter().find(|&t| clear(t))
}

/// Ecke der Schleife `k` der Fläche `i` beim Winkel `u_s`: Die Mantellinie schneidet die
/// Schleife dort (genau einmal, siehe [`free_angle`]); liegt der Schnittpunkt höchstens `tol`
/// neben einer Ecke, ist es diese, sonst wird die Kante dort geteilt.
fn vertex_on(raw: &mut RawBody, i: usize, k: usize, u_s: f64, tol: f64) -> Option<usize> {
    let surface = raw.faces[i].surface.clone();
    let per = surface.u_period()?;
    let lp = raw.faces[i].loops[k].clone();
    let track = u_track(raw, &surface, &lp);
    let &[(j, target)] = &crossings(&track, u_s, per)[..] else {
        return None;
    };
    // Bereich des Anteils auf der Halbkante j, in dem `u` die Zielstelle kreuzt.
    let pos = track.windows(2).position(|w| {
        w[0].0 == j && {
            let (a, b) = (w[0].2, w[1].2);
            (a <= target && target < b) || (b <= target && target < a)
        }
    })?;
    let (s0, u0) = (track[pos].1, track[pos].2);
    let (s1, _) = (track[pos + 1].1, track[pos + 1].2);
    let he = lp[j];
    let u_at = |s: f64| {
        let u = surface.project(he_point(raw, he, s)).0;
        u + per * ((u0 - u) / per).round()
    };
    let rising = u_at(s1) > u_at(s0);
    let (mut lo, mut hi) = (s0, s1);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if (u_at(mid) < target) == rising {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let s = 0.5 * (lo + hi);
    let p = he_point(raw, he, s);
    let (vs, ve) = (raw.he_start(he), raw.he_end(he));
    for v in [vs, ve] {
        if raw.vertices[v].distance(p) <= tol {
            return Some(v);
        }
    }
    let e = &raw.edges[he.edge];
    let fe = if he.forward { s } else { 1.0 - s };
    let t = e.t0 + (e.t1 - e.t0) * fe;
    Some(split_edge(raw, he.edge, t))
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
                return Err(at_line(line, tr!("step-r-seam-zero")));
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
