//! Profil-Extraktion: geschlossene Ketten aus Linien/Bögen und Kreise werden zu Kernel-Profilen.
//!
//! Regeln: Es zählen lebende Kurven, die keine Konstruktionsgeometrie sind. Endpunkte im Abstand
//! `≤ tol.linear` bilden denselben Knoten. Kreise sind eigene Schleifen, Linien und Bögen bilden
//! einen Graphen, dessen Zusammenhangskomponenten je ein einfacher Kreis sein müssen. Ellipsen
//! und Splines werden (noch) abgelehnt. Schleifen, die sich selbst oder andere schneiden oder
//! berühren, werden abgelehnt; die übrigen werden nach Schachtelungstiefe zu äußeren Schleifen
//! (gerade Tiefe) und Löchern (ungerade Tiefe) sortiert.

use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, Sketch};
use planora_kernel::{Profile, ProfileLoop, Segment, segment_end};
use planora_math::{DVec2, TAU, orient2d};

/// Meldung für Bögen, deren Endpunkte nicht auf demselben Kreis um die Mitte liegen.
const ARC_OFF_CIRCLE: &str =
    "Ein Bogenende liegt nicht auf dem Bogenkreis; die Skizze zuerst lösen";

/// Abtastschritte je Vollkreis für Schnitt- und Enthaltensein-Prüfungen (wie im Kernel).
const CHECK_STEPS: usize = 64;

/// Eine Schleife (oder Kurvengruppe), die nicht zu einem Profil werden kann, mit Grund.
#[derive(Clone, Debug, PartialEq)]
pub struct RejectedLoop {
    pub curves: Vec<CurveId>,
    pub reason: SketchError,
}

/// Ergebnis von [`Sketch::profiles`].
#[derive(Clone, Debug, Default)]
pub struct ExtractedProfiles {
    pub profiles: Vec<Profile>,
    pub rejected: Vec<RejectedLoop>,
    /// Kurven der äußeren Schleife von `profiles[i]` in Durchlaufreihenfolge.
    pub curve_loops: Vec<Vec<CurveId>>,
}

/// Geschlossene Schleife, die alle Topologieprüfungen bestanden hat.
struct Candidate {
    curves: Vec<CurveId>,
    lp: ProfileLoop,
    /// Abgetastete Punkte, geschlossen gedacht (letzter Punkt ≠ erster).
    samples: Vec<DVec2>,
    bbox: (DVec2, DVec2),
}

/// Kante des Kurvengraphen: Kurve von Knoten `a` nach Knoten `b` (in Kurvenrichtung).
struct Edge {
    curve: CurveId,
    a: usize,
    b: usize,
}

impl Sketch {
    /// Zerlegt die Skizze in extrudierbare Profile und abgelehnte Schleifen mit Begründung.
    ///
    /// Die Reihenfolge ist deterministisch: Profile nach kleinster Kurven-Id ihrer äußeren
    /// Schleife, Löcher ebenso.
    pub fn profiles(&self) -> ExtractedProfiles {
        let tol = self.tolerance().linear;
        let mut out = ExtractedProfiles::default();
        let mut candidates: Vec<Candidate> = Vec::new();

        // Knoten: Endpunkte im Abstand <= tol fallen zusammen (erster Punkt ist Vertreter).
        let mut reps: Vec<DVec2> = Vec::new();
        let mut node = |pos: DVec2| -> usize {
            reps.iter()
                .position(|r| r.distance(pos) <= tol)
                .unwrap_or_else(|| {
                    reps.push(pos);
                    reps.len() - 1
                })
        };

        // Kreise sind eigene Schleifen; Ellipsen werden abgelehnt; Linien, Bögen und Splines
        // bilden den Graphen (Splines hängen mit ihren Endpunkten daran).
        let mut edges: Vec<Edge> = Vec::new();
        for (id, c) in self.curves().filter(|(_, c)| !c.construction) {
            match &c.kind {
                CurveKind::Circle { center, radius } => {
                    let r = self.vars[*radius];
                    if !(r.is_finite() && r > tol) {
                        out.rejected.push(RejectedLoop {
                            curves: vec![id],
                            reason: SketchError::Degenerate(
                                "Der Radius muss größer als null sein".into(),
                            ),
                        });
                        continue;
                    }
                    let lp = ProfileLoop::circle(self.pos(*center), r);
                    candidates.push(Candidate::new(vec![id], lp, tol));
                }
                CurveKind::Line { a, b }
                | CurveKind::Arc {
                    start: a, end: b, ..
                } => {
                    let (na, nb) = (node(self.pos(*a)), node(self.pos(*b)));
                    edges.push(Edge {
                        curve: id,
                        a: na,
                        b: nb,
                    });
                }
                CurveKind::Ellipse { .. } => out.rejected.push(RejectedLoop {
                    curves: vec![id],
                    reason: SketchError::UnsupportedForSolid("Ellipse".into()),
                }),
                CurveKind::Spline { points } => match (points.first(), points.last()) {
                    (Some(first), Some(last)) if points.len() >= 2 => {
                        let (na, nb) = (node(self.pos(*first)), node(self.pos(*last)));
                        edges.push(Edge {
                            curve: id,
                            a: na,
                            b: nb,
                        });
                    }
                    _ => out.rejected.push(RejectedLoop {
                        curves: vec![id],
                        reason: SketchError::UnsupportedForSolid("Spline".into()),
                    }),
                },
            }
        }
        let node_count = reps.len();

        // Zusammenhangskomponenten (Union-Find über die Knoten).
        let mut parent: Vec<usize> = (0..node_count).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        for e in &edges {
            let (ra, rb) = (find(&mut parent, e.a), find(&mut parent, e.b));
            if ra != rb {
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
        // Komponenten in Reihenfolge ihrer ersten Kante (Kanten liegen nach Kurven-Id sortiert).
        let mut comp_index: Vec<Option<usize>> = vec![None; node_count];
        let mut comps: Vec<Vec<usize>> = Vec::new();
        for (ei, e) in edges.iter().enumerate() {
            let root = find(&mut parent, e.a);
            let ci = *comp_index[root].get_or_insert_with(|| {
                comps.push(Vec::new());
                comps.len() - 1
            });
            comps[ci].push(ei);
        }

        for comp in comps {
            let curves: Vec<CurveId> = comp.iter().map(|&e| edges[e].curve).collect();
            match self.trace_loop(&comp, &edges, &reps) {
                Ok(lp_curves) => {
                    let (order, lp) = lp_curves;
                    candidates.push(Candidate::new(order, lp, tol));
                }
                Err(reason) => out.rejected.push(RejectedLoop { curves, reason }),
            }
        }

        // Selbstschnitt und Schnitt zwischen Schleifen.
        let n = candidates.len();
        let mut bad = vec![false; n];
        for i in 0..n {
            if polyline_self_touches(&candidates[i].samples) {
                bad[i] = true;
            }
            for j in i + 1..n {
                if boxes_overlap(candidates[i].bbox, candidates[j].bbox)
                    && polylines_touch(&candidates[i].samples, &candidates[j].samples)
                {
                    bad[i] = true;
                    bad[j] = true;
                }
            }
        }
        let mut good: Vec<usize> = Vec::new();
        for (i, c) in candidates.iter().enumerate() {
            if bad[i] {
                out.rejected.push(RejectedLoop {
                    curves: c.curves.clone(),
                    reason: SketchError::SelfIntersecting,
                });
            } else if c.lp.signed_area().abs() <= tol * tol {
                // Zwei Linien zwischen denselben Knoten (Hin- und Rückweg) oder kollineare
                // Punkte schließen keine Fläche ein. Erst nach der Schnittprüfung, weil sich bei
                // einer Achterschleife die Teilflächen aufheben. Schwelle: Quadrat der Kante tol.
                out.rejected.push(RejectedLoop {
                    curves: c.curves.clone(),
                    reason: SketchError::Degenerate("Die Schleife umschließt keine Fläche".into()),
                });
            } else {
                good.push(i);
            }
        }

        // Schachtelung: Tiefe = Zahl der umschließenden Schleifen. Da sich die übrigen
        // Schleifen nicht berühren, entscheidet ein beliebiger Abtastpunkt.
        good.sort_by_key(|&i| candidates[i].curves.iter().map(|c| c.index()).min());
        let containers: Vec<Vec<usize>> = good
            .iter()
            .map(|&i| {
                good.iter()
                    .copied()
                    .filter(|&j| {
                        j != i
                            && boxes_overlap(candidates[i].bbox, candidates[j].bbox)
                            && point_in_polygon(candidates[i].samples[0], &candidates[j].samples)
                    })
                    .collect()
            })
            .collect();
        let mut profile_of: Vec<Option<usize>> = vec![None; n];
        for (k, &i) in good.iter().enumerate() {
            if containers[k].len().is_multiple_of(2) {
                profile_of[i] = Some(out.profiles.len());
                out.profiles.push(Profile {
                    outer: candidates[i].lp.clone(),
                    holes: Vec::new(),
                });
                out.curve_loops.push(candidates[i].curves.clone());
            }
        }
        let area = |i: usize| candidates[i].lp.signed_area().abs();
        for (k, &i) in good.iter().enumerate() {
            if !containers[k].len().is_multiple_of(2) {
                // Unmittelbar umschließende Schleife = kleinste Fläche unter den Umschließenden;
                // sie hat gerade Tiefe und ist damit eine äußere Schleife.
                let direct = containers[k]
                    .iter()
                    .copied()
                    .min_by(|&a, &b| area(a).total_cmp(&area(b)));
                match direct.and_then(|p| profile_of[p]) {
                    Some(pi) => out.profiles[pi].holes.push(candidates[i].lp.clone()),
                    None => out.rejected.push(RejectedLoop {
                        curves: candidates[i].curves.clone(),
                        reason: SketchError::AmbiguousNesting,
                    }),
                }
            }
        }
        out
    }

    /// Prüft eine Zusammenhangskomponente des Kurvengraphen und baut daraus die Schleife.
    /// Liefert die Kurven in Durchlaufreihenfolge und die Kernel-Schleife.
    fn trace_loop(
        &self,
        comp: &[usize],
        edges: &[Edge],
        reps: &[DVec2],
    ) -> Result<(Vec<CurveId>, ProfileLoop), SketchError> {
        let tol = self.tolerance();
        if comp
            .iter()
            .any(|&e| matches!(self.curves[edges[e].curve].kind, CurveKind::Spline { .. }))
        {
            return Err(SketchError::UnsupportedForSolid("Spline".into()));
        }
        if let Some(&e) = comp.iter().find(|&&e| edges[e].a == edges[e].b) {
            let msg = match self.curves[edges[e].curve].kind {
                CurveKind::Arc { .. } => "Ein Bogen beginnt und endet im selben Punkt",
                _ => "Eine Linie hat die Länge null",
            };
            return Err(SketchError::Degenerate(msg.into()));
        }
        let mut degree = vec![0usize; reps.len()];
        for &e in comp {
            degree[edges[e].a] += 1;
            degree[edges[e].b] += 1;
        }
        // Verzweigungen zuerst: Ein Stummel an einer Ecke hat Grad 1 *und* 3 und ist mehrdeutig,
        // nicht „offen“.
        if degree.iter().any(|&d| d >= 3) {
            return Err(SketchError::AmbiguousTopology("T-Verzweigung".into()));
        }
        if degree.contains(&1) {
            return Err(SketchError::OpenLoop);
        }

        // Kreis durchlaufen: Alle Knoten haben Grad 2, die Komponente ist zusammenhängend,
        // also ein einziger Kreis über alle Kanten.
        let first = comp[0];
        let mut used = vec![false; edges.len()];
        used[first] = true;
        let mut order: Vec<(usize, bool)> = vec![(first, true)];
        let mut at_node = edges[first].b;
        while at_node != edges[first].a {
            let next = comp
                .iter()
                .copied()
                .find(|&e| !used[e] && (edges[e].a == at_node || edges[e].b == at_node))
                .ok_or(SketchError::OpenLoop)?;
            used[next] = true;
            let forward = edges[next].a == at_node;
            order.push((next, forward));
            at_node = if forward {
                edges[next].b
            } else {
                edges[next].a
            };
        }
        if order.len() != comp.len() {
            // Kann bei Grad 2 überall nicht auftreten; zur Sicherheit statt einer Panik.
            return Err(SketchError::AmbiguousTopology(
                "mehrere Schleifen in einer Gruppe".into(),
            ));
        }

        let start = reps[edges[first].a];
        let mut segments = Vec::with_capacity(order.len());
        // Laufender Endpunkt, wie der Kernel ihn berechnet: Linien springen auf ihr absolutes
        // Ziel, Bögen drehen den vorigen Endpunkt. Jedes Segmentende muss deshalb einzeln
        // mit seinem Zielknoten übereinstimmen, sonst wiche das Profil still von der Skizze ab.
        let mut at = start;
        for &(e, forward) in &order {
            let edge = &edges[e];
            let target = reps[if forward { edge.b } else { edge.a }];
            let seg = match self.curves[edge.curve].kind {
                CurveKind::Arc { center, .. } => {
                    // Der Bogen läuft gegen den Uhrzeigersinn vom Knoten `a` (Start) zu `b` (Ende).
                    let c = self.pos(center);
                    let (ra, rb) = ((reps[edge.a] - c).length(), (reps[edge.b] - c).length());
                    if (ra - rb).abs() > tol.linear {
                        return Err(SketchError::Degenerate(ARC_OFF_CIRCLE.into()));
                    }
                    let angle = |p: DVec2| (p - c).y.atan2((p - c).x);
                    let phi = (angle(reps[edge.b]) - angle(reps[edge.a])).rem_euclid(TAU);
                    // Der Kernel verlangt tol.angular < |sweep| ≤ 2π; ein Bogen zwischen zwei
                    // verschiedenen Knoten, der (fast) null oder eine volle Drehung überstreicht,
                    // ist entartet.
                    if phi <= tol.angular || phi >= TAU - tol.angular {
                        return Err(SketchError::Degenerate(
                            "Ein Bogen hat (fast) keinen Öffnungswinkel".into(),
                        ));
                    }
                    Segment::Arc {
                        center: c,
                        sweep: if forward { phi } else { -phi },
                    }
                }
                _ => Segment::Line { to: target },
            };
            at = segment_end(at, &seg);
            if at.distance(target) > tol.linear {
                return Err(SketchError::Degenerate(ARC_OFF_CIRCLE.into()));
            }
            segments.push(seg);
        }
        let lp = ProfileLoop { start, segments };
        if !lp.is_closed(tol) {
            // Durch die Prüfung je Segment eigentlich ausgeschlossen; zur Sicherheit.
            return Err(SketchError::Degenerate(ARC_OFF_CIRCLE.into()));
        }
        Ok((order.iter().map(|&(e, _)| edges[e].curve).collect(), lp))
    }
}

impl Candidate {
    fn new(curves: Vec<CurveId>, lp: ProfileLoop, tol: f64) -> Candidate {
        let mut samples = lp.sample(CHECK_STEPS);
        // Die Abtastung einer geschlossenen Schleife endet am Startpunkt; der doppelte
        // Punkt entfällt, die Linienzüge gelten im Folgenden als geschlossen.
        if samples.len() > 1 && samples[samples.len() - 1].distance(samples[0]) <= tol {
            samples.pop();
        }
        let bbox = samples.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        Candidate {
            curves,
            lp,
            samples,
            bbox,
        }
    }
}

/// Überlappen oder berühren sich zwei Hüllrechtecke (Rand zählt mit)?
fn boxes_overlap(a: (DVec2, DVec2), b: (DVec2, DVec2)) -> bool {
    a.0.x <= b.1.x && b.0.x <= a.1.x && a.0.y <= b.1.y && b.0.y <= a.1.y
}

/// Segmente eines geschlossen gedachten Linienzugs (letzter Punkt zurück zum ersten).
fn closed_segments(pts: &[DVec2]) -> impl Iterator<Item = (DVec2, DVec2)> + '_ {
    let n = pts.len();
    (0..n).map(move |i| (pts[i], pts[(i + 1) % n]))
}

/// Liegt `p` (kollinear zu `a`–`b`) im achsparallelen Bereich der Strecke?
fn within_range(a: DVec2, b: DVec2, p: DVec2) -> bool {
    p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
}

/// Haben `a` und `b` streng entgegengesetzte Vorzeichen?
fn opposite(a: f64, b: f64) -> bool {
    (a > 0.0 && b < 0.0) || (a < 0.0 && b > 0.0)
}

/// Schneiden oder berühren sich die Strecken `p`–`q` und `r`–`s`? Exakter Vorzeichentest.
fn segments_touch(p: DVec2, q: DVec2, r: DVec2, s: DVec2) -> bool {
    let d1 = orient2d(p, q, r);
    let d2 = orient2d(p, q, s);
    let d3 = orient2d(r, s, p);
    let d4 = orient2d(r, s, q);
    if opposite(d1, d2) && opposite(d3, d4) {
        return true;
    }
    (d1 == 0.0 && within_range(p, q, r))
        || (d2 == 0.0 && within_range(p, q, s))
        || (d3 == 0.0 && within_range(r, s, p))
        || (d4 == 0.0 && within_range(r, s, q))
}

/// Schneidet oder berührt irgendein Segment von `a` irgendeines von `b` (beide geschlossen)?
fn polylines_touch(a: &[DVec2], b: &[DVec2]) -> bool {
    closed_segments(a).any(|(p, q)| closed_segments(b).any(|(r, s)| segments_touch(p, q, r, s)))
}

/// Berühren sich nicht benachbarte Segmente desselben geschlossenen Linienzugs?
fn polyline_self_touches(pts: &[DVec2]) -> bool {
    let n = pts.len();
    if n < 4 {
        // Mit höchstens drei Segmenten ist jedes Segmentpaar benachbart.
        return false;
    }
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue; // benachbart über den Schluss
            }
            if segments_touch(pts[i], pts[i + 1], pts[j], pts[(j + 1) % n]) {
                return true;
            }
        }
    }
    false
}

/// Strahlverfahren (gerade/ungerade Anzahl Schnitte); das Polygon gilt als geschlossen.
fn point_in_polygon(p: DVec2, poly: &[DVec2]) -> bool {
    let mut inside = false;
    for (a, b) in closed_segments(poly) {
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}
