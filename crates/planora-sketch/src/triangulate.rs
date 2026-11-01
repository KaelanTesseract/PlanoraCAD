//! Dreiecke für die Füllung eines Profils (Anzeige „Profil anzeigen“): Ohrenschneiden auf dem
//! abgetasteten Umriss, Löcher über Brücken zum äußeren Rand angeschlossen.
//!
//! Eingabe wie [`crate::ExtractedProfiles::outlines`]: `[0]` äußerer Umriss, danach die Löcher;
//! geschlossen gedacht (ein wiederholter Anfangspunkt am Ende stört nicht). Die Umlaufrichtung
//! wird selbst bestimmt. Die Summe der Dreiecksflächen ist die Fläche des Umrisses ohne die
//! Löcher.

use planora_math::DVec2;

/// Zerlegt die Fläche `outline[0]` ohne die Löcher `outline[1..]` in Dreiecke (gegen den
/// Uhrzeigersinn). Leer, wenn es keinen brauchbaren äußeren Umriss gibt.
pub fn triangulate(outline: &[Vec<DVec2>]) -> Vec<[DVec2; 3]> {
    let Some(outer) = outline.first() else {
        return Vec::new();
    };
    let scale = outline
        .iter()
        .flatten()
        .fold(0.0_f64, |m, p| m.max(p.x.abs()).max(p.y.abs()))
        .max(1.0);
    let eps = 1e-12 * scale * scale;
    let mut poly = cleaned(outer, true, eps);
    if poly.len() < 3 {
        return Vec::new();
    }
    let mut holes: Vec<Vec<DVec2>> = outline[1..]
        .iter()
        .map(|h| cleaned(h, false, eps))
        .filter(|h| h.len() >= 3)
        .collect();
    // Löcher von rechts nach links anschließen: Die Brücke eines Lochs kreuzt dann keine später
    // angeschlossene.
    holes.sort_by(|a, b| max_x(b).total_cmp(&max_x(a)));
    for k in 0..holes.len() {
        let (done, rest) = holes.split_at(k + 1);
        bridge(&mut poly, &done[k], rest, eps);
    }
    ear_clip(&poly, eps)
}

fn max_x(pts: &[DVec2]) -> f64 {
    pts.iter().fold(f64::MIN, |m, p| m.max(p.x))
}

fn area2(pts: &[DVec2]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i].perp_dot(pts[(i + 1) % n])).sum()
}

/// Ohne nicht endliche Punkte, direkte Wiederholungen und den wiederholten Anfangspunkt;
/// gegen den Uhrzeigersinn (`ccw`) oder im Uhrzeigersinn.
fn cleaned(pts: &[DVec2], ccw: bool, eps: f64) -> Vec<DVec2> {
    let same = |a: DVec2, b: DVec2| a.distance_squared(b) <= eps;
    let mut out: Vec<DVec2> = Vec::with_capacity(pts.len());
    for &p in pts.iter().filter(|p| p.is_finite()) {
        if out.last().is_none_or(|&q| !same(p, q)) {
            out.push(p);
        }
    }
    while out.len() > 1 && same(out[0], out[out.len() - 1]) {
        out.pop();
    }
    if (area2(&out) > 0.0) != ccw {
        out.reverse();
    }
    out
}

/// Ausrichtung von `c` zur Geraden `a → b` (> 0: links).
fn orient(a: DVec2, b: DVec2, c: DVec2) -> f64 {
    (b - a).perp_dot(c - a)
}

/// Kreuzen sich die Strecken `p1 p2` und `q1 q2` echt (nicht nur in einem Endpunkt)?
fn crosses(p1: DVec2, p2: DVec2, q1: DVec2, q2: DVec2, eps: f64) -> bool {
    let d1 = orient(q1, q2, p1);
    let d2 = orient(q1, q2, p2);
    let d3 = orient(p1, p2, q1);
    let d4 = orient(p1, p2, q2);
    ((d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps))
        && ((d3 > eps && d4 < -eps) || (d3 < -eps && d4 > eps))
}

/// Liegt `v` auf der Strecke `p m` zwischen den Enden (nicht an einem Ende)?
fn passes_through(p: DVec2, m: DVec2, v: DVec2, eps: f64) -> bool {
    let d = m - p;
    let len2 = d.length_squared();
    if v.distance_squared(p) <= eps || v.distance_squared(m) <= eps || len2 <= eps {
        return false;
    }
    let t = (v - p).dot(d) / len2;
    // Abstand zur Geraden höchstens 1e-9 der Brückenlänge
    orient(p, m, v).abs() <= 1e-9 * len2 && t > 0.0 && t < 1.0
}

/// Liegt `m` vom Punkt `v` (Vorgänger `a`, Nachfolger `c`, gegen den Uhrzeigersinn) aus im
/// Inneren der Ecke?
fn in_cone(a: DVec2, v: DVec2, c: DVec2, m: DVec2) -> bool {
    if orient(a, v, c) >= 0.0 {
        orient(a, v, m) > 0.0 && orient(v, c, m) > 0.0
    } else {
        orient(a, v, m) > 0.0 || orient(v, c, m) > 0.0
    }
}

/// Schließt das Loch `hole` (im Uhrzeigersinn) über eine Brücke vom rechtesten Lochpunkt zum
/// nächsten sichtbaren Punkt von `poly` an; `others` sind die noch nicht angeschlossenen Löcher.
fn bridge(poly: &mut Vec<DVec2>, hole: &[DVec2], others: &[Vec<DVec2>], eps: f64) {
    let mi = (0..hole.len())
        .max_by(|&a, &b| hole[a].x.total_cmp(&hole[b].x))
        .unwrap_or(0);
    let m = hole[mi];
    let n = poly.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        poly[a]
            .distance_squared(m)
            .total_cmp(&poly[b].distance_squared(m))
    });
    let edges_of = |pts: &[DVec2]| -> Vec<(DVec2, DVec2)> {
        (0..pts.len())
            .map(|i| (pts[i], pts[(i + 1) % pts.len()]))
            .collect()
    };
    let mut edges = edges_of(poly);
    edges.extend(edges_of(hole));
    for o in others {
        edges.extend(edges_of(o));
    }
    let h = hole.len();
    let (hole_prev, hole_next) = (hole[(mi + h - 1) % h], hole[(mi + 1) % h]);
    let vertices = || poly.iter().chain(hole).chain(others.iter().flatten());
    // Die Brücke `p → m` muss an beiden Enden in die Fläche zeigen (an `m` aus dem Loch heraus;
    // das Loch läuft im Uhrzeigersinn, die Fläche liegt also wie beim äußeren Rand links), darf
    // keinen Rand kreuzen und durch keinen anderen Punkt laufen (dort könnte sie unbemerkt in ein
    // Loch oder aus der Fläche treten).
    let visible = |pi: usize| {
        let p = poly[pi];
        in_cone(poly[(pi + n - 1) % n], p, poly[(pi + 1) % n], m)
            && in_cone(hole_prev, m, hole_next, p)
            && !edges.iter().any(|&(a, b)| crosses(p, m, a, b, eps))
            && !vertices().any(|&v| passes_through(p, m, v, eps))
    };
    let pi = order
        .iter()
        .copied()
        .find(|&i| visible(i))
        .unwrap_or(order[0]);
    let mut merged = Vec::with_capacity(n + hole.len() + 2);
    merged.extend_from_slice(&poly[..=pi]);
    merged.extend_from_slice(&hole[mi..]);
    merged.extend_from_slice(&hole[..=mi]);
    merged.extend_from_slice(&poly[pi..]);
    *poly = merged;
}

/// Liegt `p` im Dreieck `a b c` (gegen den Uhrzeigersinn) oder auf seinem Rand?
fn in_triangle(a: DVec2, b: DVec2, c: DVec2, p: DVec2, eps: f64) -> bool {
    orient(a, b, p) >= -eps && orient(b, c, p) >= -eps && orient(c, a, p) >= -eps
}

/// Ohrenschneiden auf einem Umlauf gegen den Uhrzeigersinn (Brücken: doppelte Punkte erlaubt).
fn ear_clip(poly: &[DVec2], eps: f64) -> Vec<[DVec2; 3]> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n.saturating_sub(2));
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut left = n;
    let mut i = 0;
    // Versuche seit dem letzten geschnittenen Ohr; nach einer vollen Runde ohne Ohr wird die
    // nächste konvexe Ecke trotzdem geschnitten (Entartung), damit es sicher endet.
    let mut misses = 0;
    while left > 3 {
        let (a, b, c) = (prev[i], i, next[i]);
        let (pa, pb, pc) = (poly[a], poly[b], poly[c]);
        let turn = orient(pa, pb, pc);
        let flat = turn.abs() <= eps;
        let ear = !flat
            && turn > 0.0
            && (misses > left
                || !{
                    let mut k = next[c];
                    let mut blocked = false;
                    while k != a {
                        let p = poly[k];
                        let at_corner = [pa, pb, pc].iter().any(|q| q.distance_squared(p) <= eps);
                        if !at_corner && in_triangle(pa, pb, pc, p, eps) {
                            blocked = true;
                            break;
                        }
                        k = next[k];
                    }
                    blocked
                });
        if flat || ear || misses > 2 * left {
            if ear {
                out.push([pa, pb, pc]);
            }
            next[a] = c;
            prev[c] = a;
            left -= 1;
            misses = 0;
            i = c;
        } else {
            misses += 1;
            i = next[i];
        }
    }
    let (a, b, c) = (prev[i], i, next[i]);
    if orient(poly[a], poly[b], poly[c]) > eps {
        out.push([poly[a], poly[b], poly[c]]);
    }
    out
}
