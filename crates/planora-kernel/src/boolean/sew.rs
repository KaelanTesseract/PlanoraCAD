//! Zusammennähen ausgewählter Flächenstücke zu einem `Solid`.

use super::error::BooleanError;
use super::seg::Seg;
use super::select::Piece;
use super::{pipeline_tol, seg_is_freeform};
use crate::geometry::Surface;
use crate::ids::Id;
use crate::topology::{Edge, Face, HalfEdge, Loop, Shell, Solid, Vertex};
use planora_math::{DVec3, Tolerance};
use std::collections::HashMap;

/// Bereits angelegte Kante mit Stützpunkten zum Wiedererkennen und zur Richtungsbestimmung.
struct EdgeRec {
    id: Id<Edge>,
    a: Id<Vertex>,
    b: Id<Vertex>,
    mid: DVec3,
    q1: DVec3,
    q3: DVec3,
    /// Das Stück selbst (Kurvenprobe bei Freiformbeteiligung).
    seg: Seg,
}

struct Sewer<'t> {
    solid: Solid,
    verts: Vec<(DVec3, Id<Vertex>)>,
    edges: Vec<EdgeRec>,
    /// Flächen (Index in der Stückliste), die eine Kante benutzen, je Benutzung ein Eintrag.
    uses: HashMap<Id<Edge>, Vec<usize>>,
    tol: &'t Tolerance,
    /// Freiformbeteiligung (strengere Kantenprobe, siehe `half_edge`).
    freeform: bool,
}

/// Abstand (in Vielfachen der Knotentoleranz), bis zu dem zwei Stücke mit Freiformbeteiligung an
/// Mitte und Viertelpunkten als dieselbe Kante gelten: Zwei Darstellungen derselben Schnittkurve
/// liegen je höchstens `0,8 · NURBS_FIT_TOL` neben der exakten Kurve, also weniger als eine
/// Knotentoleranz (2e-5) auseinander; das Doppelte lässt Spielraum.
const FREEFORM_EDGE_MATCH: f64 = 2.0;

impl Sewer<'_> {
    /// Ecke an `p`; Ecken innerhalb der Toleranz werden verschweißt.
    fn vertex(&mut self, p: DVec3) -> Id<Vertex> {
        if let Some((_, id)) = self
            .verts
            .iter()
            .find(|(q, _)| q.distance(p) <= self.tol.linear * 4.0)
        {
            return *id;
        }
        let id = self.solid.add_vertex(p);
        self.verts.push((p, id));
        id
    }

    /// Findet die gemeinsame Kante zu `seg` oder legt sie an; liefert die Halbkante in Richtung von `seg`.
    fn half_edge(&mut self, seg: &Seg, face: usize) -> HalfEdge {
        let a = self.vertex(seg.start());
        let b = self.vertex(seg.end());
        let (m, q1, q3) = (seg.at(0.5), seg.at(0.25), seg.at(0.75));
        let tol = self.tol.linear * 8.0;
        let freeform = self.freeform;
        let found = self.edges.iter().find(|r| {
            let ends = (r.a == a && r.b == b) || (r.a == b && r.b == a);
            if !freeform {
                return ends && r.mid.distance(m) <= tol;
            }
            // Freiformbeteiligung: `tol.linear` ist hier schon die weitere Knotentoleranz; zwei
            // Stücke sind dieselbe Kante nur, wenn Mitte und Viertelpunkte höchstens
            // `FREEFORM_EDGE_MATCH · tol.linear` neben dem schon angelegten Stück liegen
            // (Abstand zur Kurve, nicht zum Punkt gleichen Bruchteils: die Enden dürfen sich
            // beim Verschweißen längs der Kurve verschieben). Die Mittenprobe allein mit
            // `8 · tol.linear` legte einen flachen Bogen (Pfeilhöhe 1e-4) mit seiner Sehne zusammen.
            let close = self.tol.linear * FREEFORM_EDGE_MATCH;
            ends && [m, q1, q3].iter().all(|q| r.seg.closest(*q).1 <= close)
        });
        if let Some(r) = found {
            let forward =
                r.q1.distance(q1) + r.q3.distance(q3) <= r.q1.distance(q3) + r.q3.distance(q1);
            let id = r.id;
            self.uses.entry(id).or_default().push(face);
            return HalfEdge { edge: id, forward };
        }
        let edge = self.solid.add_edge(seg.curve.clone(), seg.t0, seg.t1, a, b);
        self.edges.push(EdgeRec {
            id: edge,
            a,
            b,
            mid: m,
            q1,
            q3,
            seg: seg.clone(),
        });
        self.uses.entry(edge).or_default().push(face);
        HalfEdge {
            edge,
            forward: true,
        }
    }

    fn loop_of(&mut self, segs: &[Seg], face: usize) -> Loop {
        Loop {
            half_edges: segs.iter().map(|s| self.half_edge(s, face)).collect(),
        }
    }
}

/// Teilt die Stücke einer Schleife an fremden Ecken, die in ihrem Inneren liegen (T-Stöße: die
/// Nachbarfläche wurde dort geteilt, diese nicht). Punkte im Schweißabstand der Enden zählen nicht.
fn split_at_vertices(lp: &[Seg], pts: &[DVec3], tol: &Tolerance) -> Vec<Seg> {
    let weld = tol.linear * 4.0;
    let mut out = Vec::with_capacity(lp.len());
    for s in lp {
        let (a, b) = (s.start(), s.end());
        let mut cuts: Vec<(f64, DVec3)> = Vec::new();
        for &p in pts {
            if p.distance(a) <= weld || p.distance(b) <= weld {
                continue;
            }
            let (f, d) = s.closest(p);
            if d <= tol.linear && !cuts.iter().any(|(_, q)| q.distance(p) <= weld) {
                cuts.push((f, p));
            }
        }
        cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut prev = 0.0;
        for f in cuts.into_iter().map(|(f, _)| f).chain(std::iter::once(1.0)) {
            out.push(s.sub(prev, f));
            prev = f;
        }
    }
    out
}

fn find(parent: &mut [usize], x: usize) -> usize {
    let mut r = x;
    while parent[r] != r {
        r = parent[r];
    }
    let mut c = x;
    while parent[c] != r {
        let n = parent[c];
        parent[c] = r;
        c = n;
    }
    r
}

/// Näht Flächenstücke zu einem `Solid` zusammen: Ecken und Kanten werden verschweißt, die Schalen
/// ergeben sich aus dem Zusammenhang über gemeinsame Kanten (Union-Find).
///
/// Mit Freiformflächen oder -kurven gilt für das Verschweißen die Toleranz
/// [`pipeline_tol`](super::pipeline_tol): Schnittkurven liegen nur auf `NURBS_FIT_TOL` genau.
pub fn sew(pieces: Vec<Piece>, tol: &Tolerance) -> Result<Solid, BooleanError> {
    let freeform = pieces.iter().any(|p| {
        matches!(p.surface, Surface::Nurbs(_))
            || std::iter::once(&p.outer)
                .chain(p.holes.iter())
                .flatten()
                .any(seg_is_freeform)
    });
    let ptol = pipeline_tol(tol, freeform);
    let tol = &ptol;
    let mut s = Sewer {
        solid: Solid::new(),
        verts: Vec::new(),
        edges: Vec::new(),
        uses: HashMap::new(),
        tol,
        freeform,
    };
    let corners: Vec<DVec3> = pieces
        .iter()
        .flat_map(|p| std::iter::once(&p.outer).chain(p.holes.iter()))
        .flatten()
        .map(Seg::start)
        .collect();
    let mut face_ids: Vec<Id<Face>> = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        if p.outer.is_empty() || p.holes.iter().any(|h| h.is_empty()) {
            return Err(BooleanError::NotClosed);
        }
        let outer = split_at_vertices(&p.outer, &corners, tol);
        let outer = s.loop_of(&outer, i);
        let inner: Vec<Loop> = p
            .holes
            .iter()
            .map(|h| {
                let h = split_at_vertices(h, &corners, tol);
                s.loop_of(&h, i)
            })
            .collect();
        let patch = if matches!(p.surface, Surface::Plane { .. }) {
            None
        } else {
            p.domain
        };
        face_ids.push(s.solid.add_face(Face {
            surface: p.surface.clone(),
            same_sense: p.same_sense,
            outer,
            inner,
            patch,
            tag: p.tag.clone(),
        }));
    }

    // Jede Kante muss von mindestens zwei Halbkanten benutzt werden (genau zwei und die
    // Richtungen prüft der Validator).
    let mut parent: Vec<usize> = (0..pieces.len()).collect();
    for faces in s.uses.values() {
        if faces.len() < 2 {
            return Err(BooleanError::NotClosed);
        }
        let first = faces[0];
        for &f in &faces[1..] {
            let (ra, rb) = (find(&mut parent, first), find(&mut parent, f));
            parent[ra] = rb;
        }
    }
    let mut groups: HashMap<usize, Vec<Id<Face>>> = HashMap::new();
    for (i, id) in face_ids.iter().enumerate() {
        groups.entry(find(&mut parent, i)).or_default().push(*id);
    }
    // Reihenfolge der Schalen: nach der ersten Fläche (deterministisch, unabhängig vom HashMap).
    let mut shells: Vec<Vec<Id<Face>>> = groups.into_values().collect();
    shells.sort_by_key(|faces| faces[0].index());
    s.solid.shells = shells.into_iter().map(|faces| Shell { faces }).collect();
    Ok(s.solid)
}
