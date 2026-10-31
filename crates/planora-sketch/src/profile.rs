//! Profil-Extraktion: geschlossene Ketten aus Linien, Bögen und Splines sowie Kreise und
//! Ellipsen werden zu Kernel-Profilen.
//!
//! Regeln: Es zählen lebende Kurven, die keine Konstruktionsgeometrie sind. Endpunkte im Abstand
//! `≤ tol.linear` bilden denselben Knoten. Kreise und Ellipsen sind eigene Schleifen (Ellipsen als
//! ein geschlossenes Freiformsegment), Linien, Bögen und Splines bilden einen Graphen, dessen
//! Zusammenhangskomponenten je ein einfacher Kreis sein müssen. Splines werden zu natürlichen
//! kubischen Freiformkurven mit derselben Parametrisierung wie in der Skizze (Stützpunkt `i` bei
//! `t = i`); ein Spline, dessen Enden zusammenfallen, ist allein eine Schleife. Schleifen, die
//! sich selbst oder andere schneiden oder
//! berühren, werden abgelehnt; die übrigen werden nach Schachtelungstiefe zu äußeren Schleifen
//! (gerade Tiefe) und Löchern (ungerade Tiefe) sortiert.
//!
//! „Berühren“ heißt: näher als `tol.linear` plus die Sehnenabweichung der Abtastung
//! (`planora_kernel::SampledLoop`, dieselbe Prüfung wie im Kernel). Ein tangential an eine
//! Kante gelegtes Loch wird so in jedem Maßstab abgelehnt; sehr knappe echte Abstände (unter
//! etwa 0,12 % des Radius beteiligter Bögen) gelten ebenfalls als Berührung.

use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, Sketch};
use planora_i18n::tr;
use planora_kernel::{Profile, ProfileLoop, SampledLoop, Segment, segment_end};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, TAU, Tolerance};
use std::sync::Arc;

/// Meldung für Bögen, deren Endpunkte nicht auf demselben Kreis um die Mitte liegen.
fn arc_off_circle() -> String {
    tr!("sketch-err-arc-off-circle")
}

/// Meldung für Splines, deren Kurvenenden nicht an den Knoten der Schleife liegen.
fn spline_off_node() -> String {
    tr!("sketch-err-spline-off-node")
}

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
    /// Abtastung mit Fehlerschranke je Sehne (gemeinsame Prüfung mit dem Kernel).
    sampled: SampledLoop,
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

        // Kreise und Ellipsen sind eigene Schleifen; Linien, Bögen und Splines bilden den
        // Graphen (Splines hängen mit ihren Endpunkten daran).
        let mut edges: Vec<Edge> = Vec::new();
        for (id, c) in self.curves().filter(|(_, c)| !c.construction) {
            match &c.kind {
                CurveKind::Circle { center, radius } => {
                    let r = self.vars[*radius];
                    if !(r.is_finite() && r > tol) {
                        out.rejected.push(RejectedLoop {
                            curves: vec![id],
                            reason: SketchError::Degenerate(tr!("sketch-radius-positive")),
                        });
                        continue;
                    }
                    let lp = ProfileLoop::circle(self.pos(*center), r);
                    candidates.push(Candidate::new(vec![id], lp, self.tolerance()));
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
                CurveKind::Ellipse { .. } => match self.ellipse_loop(id) {
                    Ok(lp) => candidates.push(Candidate::new(vec![id], lp, self.tolerance())),
                    Err(reason) => out.rejected.push(RejectedLoop {
                        curves: vec![id],
                        reason,
                    }),
                },
                CurveKind::Spline { points } => match (points.first(), points.last()) {
                    (Some(first), Some(last)) if points.len() >= 2 => {
                        let (na, nb) = (node(self.pos(*first)), node(self.pos(*last)));
                        edges.push(Edge {
                            curve: id,
                            a: na,
                            b: nb,
                        });
                    }
                    // Kann bei einer gültigen Skizze nicht auftreten (`add_spline` und das Laden
                    // verlangen zwei Punkte); zur Sicherheit benannt statt einer Panik.
                    _ => out.rejected.push(RejectedLoop {
                        curves: vec![id],
                        reason: SketchError::Degenerate(tr!("sketch-err-spline-two-points")),
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
                    candidates.push(Candidate::new(order, lp, self.tolerance()));
                }
                Err(reason) => out.rejected.push(RejectedLoop { curves, reason }),
            }
        }

        // Selbstschnitt und Schnitt zwischen Schleifen.
        let n = candidates.len();
        let mut bad = vec![false; n];
        for i in 0..n {
            if candidates[i].sampled.touches_itself(tol) {
                bad[i] = true;
            }
            for j in i + 1..n {
                if candidates[i].sampled.touches(&candidates[j].sampled, tol) {
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
                    reason: SketchError::Degenerate(tr!("sketch-err-loop-no-area")),
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
                            && candidates[i]
                                .sampled
                                .boxes_near(&candidates[j].sampled, tol)
                            && candidates[j]
                                .sampled
                                .contains(candidates[i].sampled.points[0])
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
        let is_spline =
            |e: usize| matches!(self.curves[edges[e].curve].kind, CurveKind::Spline { .. });
        // Ein Spline, dessen Enden zusammenfallen, ist allein eine geschlossene Schleife (ein
        // geschlossenes Freiformsegment wie eine Vollellipse). Hängen an diesem Knoten weitere
        // Kurven, ist das keine einfache Schleife.
        if let Some(&e) = comp.iter().find(|&&e| edges[e].a == edges[e].b)
            && !(comp.len() == 1 && is_spline(e))
        {
            let msg = match self.curves[edges[e].curve].kind {
                CurveKind::Arc { .. } => tr!("sketch-err-arc-closed"),
                CurveKind::Spline { .. } => {
                    tr!("sketch-err-spline-closed-branch")
                }
                _ => tr!("sketch-err-line-zero-length"),
            };
            return Err(SketchError::Degenerate(msg));
        }
        let mut degree = vec![0usize; reps.len()];
        for &e in comp {
            degree[edges[e].a] += 1;
            degree[edges[e].b] += 1;
        }
        // Verzweigungen zuerst: Ein Stummel an einer Ecke hat Grad 1 *und* 3 und ist mehrdeutig,
        // nicht „offen“.
        if degree.iter().any(|&d| d >= 3) {
            return Err(SketchError::AmbiguousTopology(tr!("sketch-err-t-junction")));
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
            return Err(SketchError::AmbiguousTopology(tr!(
                "sketch-err-loops-in-group"
            )));
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
            let seg = match &self.curves[edge.curve].kind {
                CurveKind::Spline { points } => self.spline_segment(points, forward)?,
                &CurveKind::Arc { center, .. } => {
                    // Der Bogen läuft gegen den Uhrzeigersinn vom Knoten `a` (Start) zu `b` (Ende).
                    let c = self.pos(center);
                    let (ra, rb) = ((reps[edge.a] - c).length(), (reps[edge.b] - c).length());
                    if (ra - rb).abs() > tol.linear {
                        return Err(SketchError::Degenerate(arc_off_circle()));
                    }
                    let angle = |p: DVec2| (p - c).y.atan2((p - c).x);
                    let phi = (angle(reps[edge.b]) - angle(reps[edge.a])).rem_euclid(TAU);
                    // Der Kernel verlangt tol.angular < |sweep| ≤ 2π; ein Bogen zwischen zwei
                    // verschiedenen Knoten, der (fast) null oder eine volle Drehung überstreicht,
                    // ist entartet.
                    if phi <= tol.angular || phi >= TAU - tol.angular {
                        return Err(SketchError::Degenerate(tr!("sketch-err-arc-no-angle")));
                    }
                    Segment::Arc {
                        center: c,
                        sweep: if forward { phi } else { -phi },
                    }
                }
                _ => Segment::Line { to: target },
            };
            let off_node = if let Segment::Nurbs { curve } = &seg {
                // Der Kernel verlangt, dass ein Freiformsegment am Ende des vorigen beginnt.
                let p = curve.point(curve.domain().0);
                if at.distance(DVec2::new(p.x, p.y)) > tol.linear {
                    return Err(SketchError::Degenerate(spline_off_node()));
                }
                spline_off_node()
            } else {
                arc_off_circle()
            };
            at = segment_end(at, &seg);
            if at.distance(target) > tol.linear {
                return Err(SketchError::Degenerate(off_node));
            }
            segments.push(seg);
        }
        let lp = ProfileLoop { start, segments };
        if !lp.is_closed(tol) {
            // Durch die Prüfung je Segment eigentlich ausgeschlossen; zur Sicherheit.
            return Err(SketchError::Degenerate(arc_off_circle()));
        }
        Ok((order.iter().map(|&(e, _)| edges[e].curve).collect(), lp))
    }
}

impl Sketch {
    /// Schleife aus einer Ellipse der Skizze: ein geschlossenes Freiformsegment
    /// `c + a·cos t + b·sin t` mit `b = minor · (−ay, ax)/|a|` (gegen den Uhrzeigersinn, wie
    /// [`Sketch::ellipse_point`]).
    fn ellipse_loop(&self, id: CurveId) -> Result<ProfileLoop, SketchError> {
        let CurveKind::Ellipse {
            center,
            ax,
            ay,
            minor,
        } = self.curves[id].kind
        else {
            return Err(SketchError::WrongType(tr!("sketch-err-not-ellipse")));
        };
        let tol = self.tolerance().linear;
        let a = DVec2::new(self.vars[ax], self.vars[ay]);
        let minor = self.vars[minor];
        let len = a.length();
        if !(len.is_finite() && len > tol) {
            return Err(SketchError::Degenerate(tr!(
                "sketch-err-ellipse-major-small",
                tol = planora_i18n::len_arg(tol, tol.to_string()),
                unit = planora_i18n::unit_symbol()
            )));
        }
        if !(minor.is_finite() && minor > tol) {
            return Err(SketchError::Degenerate(tr!(
                "sketch-err-ellipse-minor-small",
                tol = planora_i18n::len_arg(tol, tol.to_string()),
                unit = planora_i18n::unit_symbol()
            )));
        }
        let b = a.perp() / len * minor;
        let c = self.pos(center);
        let curve = NurbsCurve::elliptic_arc(
            DVec3::new(c.x, c.y, 0.0),
            DVec3::new(a.x, a.y, 0.0),
            DVec3::new(b.x, b.y, 0.0),
            0.0,
            TAU,
        )
        .map_err(|e| {
            SketchError::Degenerate(tr!("sketch-err-ellipse-invalid", detail = e.to_string()))
        })?;
        ProfileLoop::nurbs_loop(curve).map_err(|e| SketchError::Kernel(e.to_string()))
    }

    /// Freiformsegment eines Splines: natürlicher kubischer Spline durch die Stützpunkte mit
    /// den Parametern `0, 1, …, n − 1` (wie [`Sketch::spline_point`]); umgekehrt, wenn die
    /// Schleife ihn gegen seine Richtung durchläuft.
    fn spline_segment(
        &self,
        points: &[crate::model::PointId],
        forward: bool,
    ) -> Result<Segment, SketchError> {
        let tol = self.tolerance().linear;
        let xy: Vec<DVec2> = points.iter().map(|&p| self.pos(p)).collect();
        // `add_spline` prüft das beim Anlegen; das Lösen kann Punkte danach zusammenlegen.
        if xy.windows(2).any(|w| w[0].distance(w[1]) <= tol) {
            return Err(SketchError::Degenerate(tr!("sketch-err-spline-neighbours")));
        }
        let pts: Vec<DVec3> = xy.iter().map(|p| DVec3::new(p.x, p.y, 0.0)).collect();
        let params: Vec<f64> = (0..pts.len()).map(|i| i as f64).collect();
        let curve = NurbsCurve::natural_cubic(&pts, &params).map_err(|e| {
            SketchError::Degenerate(tr!("sketch-err-spline-invalid", detail = e.to_string()))
        })?;
        let seg = Segment::Nurbs {
            curve: Arc::new(curve),
        };
        if forward {
            return Ok(seg);
        }
        // Umkehr wie im Kernel (gespiegelte Knoten, bei Rundungsproblemen negierte Knoten).
        let single = ProfileLoop {
            start: xy[0],
            segments: vec![seg],
        };
        let reversed = single
            .try_reversed()
            .map_err(|e| SketchError::Kernel(e.to_string()))?;
        reversed
            .segments
            .into_iter()
            .next()
            .ok_or_else(|| SketchError::Degenerate(tr!("sketch-err-spline-reverse")))
    }
}

impl Candidate {
    fn new(curves: Vec<CurveId>, lp: ProfileLoop, tol: &Tolerance) -> Candidate {
        let sampled = SampledLoop::new(&lp, CHECK_STEPS, tol);
        Candidate {
            curves,
            lp,
            sampled,
        }
    }
}
