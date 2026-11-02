//! Ebene Anordnung der Skizzenkurven (Halbkanten-Struktur) als Grundlage der Profile.
//!
//! Alle Kurven werden an ihren Schnitt- und Berührpunkten (und Splines an ihren
//! Selbstschnitten) in Stücke geteilt. Punkte im Abstand `≤ tol` bilden denselben Knoten
//! (Gitter-Hash). Doppelte Stücke (gleiche Knoten, Mittelpunkte näher als `10·tol`) zählen
//! einmal, das der kleineren Kurve bleibt. Stummel fallen durch wiederholtes Entfernen von
//! Knoten mit Grad 1 weg. An jedem Knoten werden die abgehenden Halbkanten nach Richtung
//! geordnet. Fast gleiche Richtungen (Winkeldifferenz zur ersten der Gruppe `< 1e-2`, etwa bei
//! numerisch gefundenen Berührungen, deren Knoten bis `√(tol/κ)` neben der Berührstelle liegt)
//! ordnet die Richtung zu einem Punkt in gleicher Bogenlänge längs jedes Stücks (ein Viertel des
//! kürzesten Stücks der Gruppe; Stücke kreuzen sich nicht, also gilt dort die echte Reihenfolge),
//! danach die Krümmung (stärker links gekrümmt liegt weiter gegen den Uhrzeigersinn).
//! Die Umläufe mit der Fläche links ergeben mit positiver Fläche die Flächen der Anordnung, mit
//! negativer Fläche den Außenrand je Zusammenhangskomponente. Brücken (Stücke, deren beide
//! Seiten im selben Umlauf liegen, etwa eine Linie von einem inneren Kreis zum Rand) begrenzen
//! keine Fläche und fallen wie Stummel weg; danach wird neu umlaufen.

use crate::geom::Geom;
use crate::intersect::{Prepared, intersect_prepared, self_intersect};
use crate::model::CurveId;
use planora_math::{DVec2, PI, TAU};
use std::collections::HashMap;

/// Abschnitte je Vollkreis der Linienzüge für Flächen und Enthaltensein (Splines 16 je
/// Segment, siehe [`Geom::polyline_params`]).
pub(crate) const STEPS_PER_TURN: usize = 64;
/// Abgangsrichtungen, die sich um weniger unterscheiden, sind fast gleich (dann zählt die
/// Richtung zu einem Punkt weiter längs des Stücks).
const NEAR_ANGLE: f64 = 1e-2;
/// Stellen einer Kurve am selben Knoten, die längs der Kurve höchstens so viele Toleranzen
/// auseinanderliegen, sind dieselbe Teilungsstelle.
const SAME_SPLIT: f64 = 4.0;
/// Doppelte Stücke: Mittelpunkte näher als so viele Toleranzen.
const DUPLICATE_MID: f64 = 10.0;

/// Stück einer Kurve zwischen zwei Knoten, in Kurvenrichtung von `a` (bei `t0`) nach `b`
/// (bei `t1`, `t0 < t1`; bei Kreis und Ellipse `t1 ≤ t0 + 2π`).
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub(crate) curve: CurveId,
    /// Index der Kurve in [`Arrangement::curves`].
    pub(crate) input: usize,
    pub(crate) t0: f64,
    pub(crate) t1: f64,
    pub(crate) a: usize,
    pub(crate) b: usize,
    /// Länge in mm.
    pub(crate) len: f64,
    /// Begrenzt Flächen: weder Doppel eines anderen Stücks noch Stummel oder Brücke.
    pub(crate) active: bool,
}

/// Umlauf mit der Fläche links. Halbkante `2·k` durchläuft Stück `k` vorwärts, `2·k + 1`
/// rückwärts.
#[derive(Clone, Debug)]
pub(crate) struct Cycle {
    pub(crate) half_edges: Vec<usize>,
    /// Linienzug in Umlaufrichtung, geschlossen gedacht.
    pub(crate) polygon: Vec<DVec2>,
    /// Vorzeichenbehaftete Fläche des Linienzugs (positiv = Fläche der Anordnung).
    pub(crate) area: f64,
    /// Zusammenhangskomponente.
    pub(crate) component: usize,
    /// Hüllrechteck des Linienzugs.
    pub(crate) bbox: (DVec2, DVec2),
}

/// Die Anordnung: Kurven, Knoten, Stücke und Umläufe.
#[derive(Clone, Debug)]
pub(crate) struct Arrangement {
    pub(crate) curves: Vec<(CurveId, Geom)>,
    pub(crate) nodes: Vec<DVec2>,
    pub(crate) pieces: Vec<Piece>,
    pub(crate) cycles: Vec<Cycle>,
}

impl Arrangement {
    /// Baut die Anordnung der Kurven `curves` (in Reihenfolge ihrer Ids) mit Toleranz `tol`.
    pub(crate) fn build(curves: Vec<(CurveId, Geom)>, tol: f64) -> Arrangement {
        let mut nodes = Nodes::new(tol);
        let n = curves.len();
        let mut events: Vec<Vec<(f64, usize)>> = vec![Vec::new(); n];
        // Enden zuerst: Ihre Lage (Skizzenpunkte) vertritt den Knoten.
        for (i, (_, g)) in curves.iter().enumerate() {
            if !g.periodic() {
                let (lo, hi) = g.domain();
                events[i].push((lo, nodes.get(g.eval(lo).0)));
                events[i].push((hi, nodes.get(g.eval(hi).0)));
            }
        }
        let prepared: Vec<Prepared> = curves.iter().map(|(_, g)| Prepared::new(g)).collect();
        for i in 0..n {
            for j in i + 1..n {
                for (ti, tj) in intersect_prepared(&prepared[i], &prepared[j], tol) {
                    let p = (curves[i].1.eval(ti).0 + curves[j].1.eval(tj).0) * 0.5;
                    let v = nodes.get(p);
                    events[i].push((ti, v));
                    events[j].push((tj, v));
                }
            }
            let g = &curves[i].1;
            for (t1, t2) in self_intersect(g, tol) {
                let v = nodes.get((g.eval(t1).0 + g.eval(t2).0) * 0.5);
                events[i].push((t1, v));
                events[i].push((t2, v));
            }
        }
        // Ungeteilte Kreise und Ellipsen bekommen einen Knoten bei `t = 0`.
        for (i, (_, g)) in curves.iter().enumerate() {
            if g.periodic() && events[i].is_empty() {
                events[i].push((0.0, nodes.get(g.eval(0.0).0)));
            }
        }

        let mut pieces: Vec<Piece> = Vec::new();
        let mut mids: Vec<DVec2> = Vec::new();
        let mut by_nodes: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (i, ev) in events.into_iter().enumerate() {
            let (id, g) = &curves[i];
            for (t0, t1, a, b) in split(g, ev, tol) {
                let len = g.arc_length(t0, t1);
                if !(t1 > t0 && len.is_finite()) || (a == b && len <= SAME_SPLIT * tol) {
                    continue;
                }
                let mid = g.eval(0.5 * (t0 + t1)).0;
                let key = (a.min(b), a.max(b));
                let same = by_nodes.entry(key).or_default();
                let duplicate = same
                    .iter()
                    .any(|&k| mids[k].distance(mid) < DUPLICATE_MID * tol);
                same.push(pieces.len());
                mids.push(mid);
                pieces.push(Piece {
                    curve: *id,
                    input: i,
                    t0,
                    t1,
                    a,
                    b,
                    len,
                    active: !duplicate,
                });
            }
        }
        let nodes = nodes.pos;
        remove_stubs(&mut pieces, nodes.len());

        let mut arr = Arrangement {
            curves,
            nodes,
            pieces,
            cycles: Vec::new(),
        };
        loop {
            arr.cycles = arr.trace();
            // Brücken: beide Halbkanten im selben Umlauf
            let mut cycle_of = vec![usize::MAX; 2 * arr.pieces.len()];
            for (ci, c) in arr.cycles.iter().enumerate() {
                for &h in &c.half_edges {
                    cycle_of[h] = ci;
                }
            }
            let bridges: Vec<usize> = (0..arr.pieces.len())
                .filter(|&k| {
                    arr.pieces[k].active
                        && cycle_of[2 * k] != usize::MAX
                        && cycle_of[2 * k] == cycle_of[2 * k + 1]
                })
                .collect();
            if bridges.is_empty() {
                break;
            }
            for k in bridges {
                arr.pieces[k].active = false;
            }
            remove_stubs(&mut arr.pieces, arr.nodes.len());
        }
        arr
    }

    /// Geometrie der Kurve, zu der Stück `k` gehört.
    pub(crate) fn geom_of(&self, k: usize) -> &Geom {
        &self.curves[self.pieces[k].input].1
    }

    /// Anfangs- und Endknoten der Halbkante `h`.
    pub(crate) fn ends(&self, h: usize) -> (usize, usize) {
        let p = &self.pieces[h / 2];
        if h.is_multiple_of(2) {
            (p.a, p.b)
        } else {
            (p.b, p.a)
        }
    }

    /// Ordnet die Halbkanten an jedem Knoten und läuft alle Umläufe ab.
    fn trace(&self) -> Vec<Cycle> {
        let he_count = 2 * self.pieces.len();
        let mut around: Vec<Vec<(f64, f64, usize)>> = vec![Vec::new(); self.nodes.len()];
        for (k, p) in self.pieces.iter().enumerate() {
            if !p.active {
                continue;
            }
            for h in [2 * k, 2 * k + 1] {
                let (angle, curvature) = self.departure(h);
                around[self.ends(h).0].push((angle, curvature, h));
            }
        }
        // Lage jeder abgehenden Halbkante in der Ordnung gegen den Uhrzeigersinn um ihren Knoten.
        let mut slot = vec![usize::MAX; he_count];
        let mut order: Vec<Vec<usize>> = Vec::with_capacity(self.nodes.len());
        for list in around {
            let sorted = self.sort_around(list);
            for (i, &h) in sorted.iter().enumerate() {
                slot[h] = i;
            }
            order.push(sorted);
        }
        // Nächste Halbkante: am Endknoten die Halbkante unmittelbar im Uhrzeigersinn vor der
        // Gegenrichtung (so bleibt die Fläche links).
        let next = |h: usize| -> usize {
            let v = self.ends(h).1;
            let list = &order[v];
            let i = slot[h ^ 1];
            list[(i + list.len() - 1) % list.len()]
        };

        let component = self.components();
        let polylines: Vec<Vec<DVec2>> = (0..self.pieces.len())
            .map(|k| {
                let p = &self.pieces[k];
                if p.active {
                    self.geom_of(k).polyline(p.t0, p.t1, STEPS_PER_TURN)
                } else {
                    Vec::new()
                }
            })
            .collect();
        let mut seen = vec![false; he_count];
        let mut cycles = Vec::new();
        for start in 0..he_count {
            if seen[start] || !self.pieces[start / 2].active {
                continue;
            }
            let mut half_edges = Vec::new();
            let mut h = start;
            let mut closed = false;
            for _ in 0..=he_count {
                if seen[h] {
                    closed = h == start;
                    break;
                }
                seen[h] = true;
                half_edges.push(h);
                h = next(h);
            }
            if !closed {
                // Kann bei einer gültigen Ordnung nicht auftreten; zur Sicherheit verworfen.
                continue;
            }
            let mut polygon = Vec::new();
            for &h in &half_edges {
                let pl = &polylines[h / 2];
                if h.is_multiple_of(2) {
                    polygon.extend_from_slice(&pl[..pl.len() - 1]);
                } else {
                    polygon.extend(pl[1..].iter().rev());
                }
            }
            let area = polygon_area(&polygon);
            let bbox = polygon
                .iter()
                .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(*p), hi.max(*p))
                });
            cycles.push(Cycle {
                component: component[self.ends(start).0],
                half_edges,
                polygon,
                area,
                bbox,
            });
        }
        cycles
    }

    /// Abgangswinkel in `[0, 2π)` und vorzeichenbehaftete Krümmung in Laufrichtung der
    /// Halbkante `h` an ihrem Anfangsknoten.
    fn departure(&self, h: usize) -> (f64, f64) {
        let p = &self.pieces[h / 2];
        let g = self.geom_of(h / 2);
        let (t, sign) = if h.is_multiple_of(2) {
            (p.t0, 1.0)
        } else {
            (p.t1, -1.0)
        };
        let (at, d) = g.eval(t);
        let mut dir = d * sign;
        if !(dir.is_finite() && dir.length() > 0.0) {
            // verschwindende Ableitung: Richtung zu einem nahen Punkt des Stücks
            dir = g.eval(t + sign * 1e-6 * (p.t1 - p.t0)).0 - at;
        }
        let angle = dir.y.atan2(dir.x).rem_euclid(TAU);
        let angle = if angle >= TAU { 0.0 } else { angle };
        (angle, g.curvature(t) * sign)
    }

    /// Ordnet die abgehenden Halbkanten `(Winkel, Krümmung, Halbkante)` eines Knotens gegen den
    /// Uhrzeigersinn: nach Winkel; Gruppen fast gleicher Winkel (zur ersten der Gruppe
    /// `< NEAR_ANGLE`) nach der Richtung zum Punkt in gleicher Bogenlänge, dann nach Krümmung.
    fn sort_around(&self, mut list: Vec<(f64, f64, usize)>) -> Vec<usize> {
        let n = list.len();
        if n == 0 {
            return Vec::new();
        }
        list.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.2.cmp(&y.2)));
        // Beginn hinter der größten Lücke (auch über den Umlauf), damit keine Gruppe über 2π
        // reicht.
        let gap = |i: usize| {
            let (a, b) = (list[i].0, list[(i + 1) % n].0);
            if i + 1 == n { b + TAU - a } else { b - a }
        };
        let widest = (0..n)
            .max_by(|&i, &j| gap(i).total_cmp(&gap(j)))
            .unwrap_or(0);
        list.rotate_left((widest + 1) % n);
        let mut out = Vec::with_capacity(n);
        let mut i = 0;
        while i < n {
            let mut j = i + 1;
            while j < n && (list[j].0 - list[i].0).rem_euclid(TAU) < NEAR_ANGLE {
                j += 1;
            }
            if j == i + 1 {
                out.push(list[i].2);
            } else {
                let reference = list[i].0;
                let s = list[i..j]
                    .iter()
                    .map(|e| self.pieces[e.2 / 2].len)
                    .fold(f64::INFINITY, f64::min)
                    / 4.0;
                let mut group: Vec<(f64, f64, usize)> = list[i..j]
                    .iter()
                    .map(|&(_, k, h)| (self.secant_angle(h, s, reference), k, h))
                    .collect();
                group.sort_by(|x, y| {
                    x.0.total_cmp(&y.0)
                        .then(x.1.total_cmp(&y.1))
                        .then(x.2.cmp(&y.2))
                });
                out.extend(group.into_iter().map(|(_, _, h)| h));
            }
            i = j;
        }
        out
    }

    /// Winkel (in `(−π, π]`, relativ zu `reference`) der Richtung vom Anfang der Halbkante `h`
    /// zu ihrem Punkt nach etwa der Bogenlänge `s` (Parameter anteilig zur Stücklänge).
    fn secant_angle(&self, h: usize, s: f64, reference: f64) -> f64 {
        let p = &self.pieces[h / 2];
        let g = self.geom_of(h / 2);
        let frac = if p.len > 0.0 {
            (s / p.len).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let span = p.t1 - p.t0;
        let (from, to) = if h.is_multiple_of(2) {
            (p.t0, p.t0 + frac * span)
        } else {
            (p.t1, p.t1 - frac * span)
        };
        let v = g.eval(to).0 - g.eval(from).0;
        let a = v.y.atan2(v.x) - reference;
        let a = (a + PI).rem_euclid(TAU) - PI;
        if a.is_finite() { a } else { 0.0 }
    }

    /// Zusammenhangskomponente je Knoten (über die aktiven Stücke).
    fn components(&self) -> Vec<usize> {
        let mut parent: Vec<usize> = (0..self.nodes.len()).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        for p in self.pieces.iter().filter(|p| p.active) {
            let (ra, rb) = (find(&mut parent, p.a), find(&mut parent, p.b));
            if ra != rb {
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
        (0..self.nodes.len())
            .map(|v| find(&mut parent, v))
            .collect()
    }
}

/// Knoten mit Zusammenlegen im Abstand `≤ tol` über ein Gitter mit Zellweite `tol`.
pub(crate) struct Nodes {
    tol: f64,
    pub(crate) pos: Vec<DVec2>,
    grid: HashMap<(i64, i64), Vec<usize>>,
}

impl Nodes {
    pub(crate) fn new(tol: f64) -> Nodes {
        Nodes {
            tol: if tol > 0.0 { tol } else { f64::MIN_POSITIVE },
            pos: Vec::new(),
            grid: HashMap::new(),
        }
    }

    fn cell(&self, p: DVec2) -> (i64, i64) {
        // `as` sättigt bei sehr großen und nicht endlichen Werten.
        (
            (p.x / self.tol).floor() as i64,
            (p.y / self.tol).floor() as i64,
        )
    }

    /// Knoten bei `p`: der älteste vorhandene im Abstand `≤ tol`, sonst ein neuer.
    pub(crate) fn get(&mut self, p: DVec2) -> usize {
        let (cx, cy) = self.cell(p);
        let mut best: Option<usize> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                let key = (cx.saturating_add(dx), cy.saturating_add(dy));
                for &i in self.grid.get(&key).into_iter().flatten() {
                    if self.pos[i].distance(p) <= self.tol && best.is_none_or(|b| i < b) {
                        best = Some(i);
                    }
                }
            }
        }
        best.unwrap_or_else(|| {
            let i = self.pos.len();
            self.pos.push(p);
            self.grid.entry((cx, cy)).or_default().push(i);
            i
        })
    }
}

/// Teilungsstellen einer Kurve zu Stücken `(t0, t1, a, b)`: Parameter in den Bereich gebracht
/// und sortiert; Stellen am selben Knoten, die längs der Kurve höchstens `SAME_SPLIT·tol`
/// auseinanderliegen, zählen einmal (die Enden offener Kurven behalten ihren Parameter).
pub(crate) fn split(g: &Geom, ev: Vec<(f64, usize)>, tol: f64) -> Vec<(f64, f64, usize, usize)> {
    let (_, hi) = g.domain();
    let mut ev: Vec<(f64, usize)> = ev
        .into_iter()
        .filter(|(t, _)| t.is_finite())
        .map(|(t, v)| (g.normalize(t), v))
        .collect();
    ev.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
    let mut out: Vec<(f64, usize)> = Vec::with_capacity(ev.len());
    for (t, v) in ev {
        if let Some(last) = out.last_mut()
            && last.1 == v
            && g.arc_length(last.0, t) <= SAME_SPLIT * tol
        {
            if !g.periodic() && t == hi {
                last.0 = hi;
            }
            continue;
        }
        out.push((t, v));
    }
    if g.periodic()
        && out.len() > 1
        && out[out.len() - 1].1 == out[0].1
        && g.arc_length(out[out.len() - 1].0, out[0].0 + TAU) <= SAME_SPLIT * tol
    {
        out.pop();
    }
    let mut pieces: Vec<(f64, f64, usize, usize)> = out
        .windows(2)
        .map(|w| (w[0].0, w[1].0, w[0].1, w[1].1))
        .collect();
    if g.periodic()
        && let (Some(first), Some(last)) = (out.first(), out.last())
    {
        pieces.push((last.0, first.0 + TAU, last.1, first.1));
    }
    pieces
}

/// Entfernt wiederholt aktive Stücke an Knoten mit Grad 1 (Schleifen am Knoten zählen 2).
fn remove_stubs(pieces: &mut [Piece], node_count: usize) {
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    let mut degree = vec![0usize; node_count];
    for (k, p) in pieces.iter().enumerate().filter(|(_, p)| p.active) {
        incident[p.a].push(k);
        incident[p.b].push(k);
        degree[p.a] += 1;
        degree[p.b] += 1;
    }
    let mut stack: Vec<usize> = (0..node_count).filter(|&v| degree[v] == 1).collect();
    while let Some(v) = stack.pop() {
        if degree[v] != 1 {
            continue;
        }
        let Some(&k) = incident[v].iter().find(|&&k| pieces[k].active) else {
            continue;
        };
        pieces[k].active = false;
        let (a, b) = (pieces[k].a, pieces[k].b);
        degree[a] -= 1;
        degree[b] -= 1;
        let other = if a == v { b } else { a };
        if degree[other] == 1 {
            stack.push(other);
        }
    }
}

/// Vorzeichenbehaftete Fläche eines geschlossen gedachten Linienzugs.
pub(crate) fn polygon_area(pts: &[DVec2]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| pts[i].perp_dot(pts[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

/// Flächenschwerpunkt eines geschlossen gedachten Linienzugs (Mittel der Punkte, wenn er keine
/// Fläche hat).
pub(crate) fn polygon_centroid(pts: &[DVec2]) -> DVec2 {
    let n = pts.len();
    let (mut a, mut c) = (0.0, DVec2::ZERO);
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        let w = p.perp_dot(q);
        a += w;
        c += (p + q) * w;
    }
    if a.abs() > 0.0 && c.is_finite() {
        c / (3.0 * a)
    } else {
        pts.iter().copied().sum::<DVec2>() / n.max(1) as f64
    }
}

/// Liegt `p` im Inneren des geschlossen gedachten Linienzugs (gerade/ungerade Schnittzahl)?
pub(crate) fn polygon_contains(pts: &[DVec2], p: DVec2) -> bool {
    let n = pts.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                inside = !inside;
            }
        }
    }
    inside
}
