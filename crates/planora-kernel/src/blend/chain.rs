//! Kantenketten (Spec §4.2, R8): glatt anschließende Kanten, Tangentenkette und Gruppierung der
//! gewählten Kanten in Ketten mit Längen (für den variablen Radius, Task 14).

use super::error::BlendError;
use super::topo::{edge_angle, edge_sides, edge_uses, vertex_edges};
use super::{BlendEdge, BlendRequest, MAX_BLEND_EDGES};
use crate::boolean::Seg;
use crate::ids::Id;
use crate::topology::{Edge, Face, Solid, Vertex};
use planora_math::{DVec3, PI};
use std::collections::VecDeque;

/// Größter Winkel zwischen den Richtungen zweier Kanten an ihrer gemeinsamen Ecke (0,5°), bei
/// dem sie als glatt anschließend gelten. Tangentiale Übergänge aus Skizzen sind auf 1e-9 genau.
pub const CHAIN_ANGLE: f64 = 0.5 * PI / 180.0;

/// Gewählte Kante mit ihrer Lage in einer Kette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChainedEdge {
    pub edge: BlendEdge,
    /// Nummer der Kette (aufeinanderfolgende, glatt verbundene gewählte Kanten).
    pub chain: usize,
    /// Abstand des Kantenanfangs in Kettenrichtung vom Kettenanfang (mm).
    pub offset: f64,
    /// Länge der Kante (mm).
    pub length: f64,
    /// Läuft die Kante gegen die Kettenrichtung?
    pub reversed: bool,
    /// Länge der ganzen Kette (mm).
    pub chain_length: f64,
}

impl ChainedEdge {
    /// Anteil der Kettenlänge am Kantenpunkt im Abstand `s` vom Kantenanfang (Kantenrichtung).
    pub fn fraction(&self, s: f64) -> f64 {
        let along = if self.reversed { self.length - s } else { s };
        if self.chain_length > 0.0 {
            ((self.offset + along) / self.chain_length).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Richtung, in der `edge` die Ecke `v` verlässt (Einheitsvektor; null bei unbekannter Kante
/// oder entarteter Tangente).
pub fn leaving_direction(solid: &Solid, edge: Id<Edge>, v: Id<Vertex>) -> DVec3 {
    let Some(e) = solid.edges.get(edge) else {
        return DVec3::ZERO;
    };
    let sign = (e.t1 - e.t0).signum();
    if e.start == v {
        (e.curve.tangent(e.t0) * sign).normalize_or_zero()
    } else {
        (-e.curve.tangent(e.t1) * sign).normalize_or_zero()
    }
}

fn is_seam(solid: &Solid, edge: Id<Edge>) -> bool {
    let u = edge_uses(solid, edge);
    u.len() == 2 && u[0].face == u[1].face
}

/// Kante zwischen zwei Flächen, die tangential ineinander übergehen (Kantenwinkel nahe 180°,
/// etwa zwei nicht zusammengefasste Stücke derselben Ebene nach einer Vereinigung)? Sie ist
/// keine echte Kante: Eine Kette läuft nicht über sie weiter (Review Stufe 2, M1).
fn is_flat(solid: &Solid, edge: Id<Edge>) -> bool {
    /// Grenze zwischen Schneide (Kantenwinkel nahe 0°) und glattem Übergang (nahe 180°).
    const RIGHT_ANGLE_DEG: f64 = 90.0;
    edge_sides(solid, edge, None).is_ok_and(|sides| {
        matches!(
            edge_angle(solid, edge, &sides),
            Err(BlendError::NearlyTangent { angle_deg }) if angle_deg > RIGHT_ANGLE_DEG
        )
    })
}

/// Geschlossene Kante (Anfang = Ende)? Sie läuft nur in sich selbst weiter.
fn is_loop(solid: &Solid, edge: Id<Edge>) -> bool {
    solid.edges.get(edge).is_none_or(|e| e.start == e.end)
}

/// Die Kante, die an `v` glatt an `edge` anschließt, wenn es genau eine gibt (Nähte und Kanten
/// zwischen tangential ineinander übergehenden Flächen zählen nicht).
pub fn smooth_neighbour(solid: &Solid, edge: Id<Edge>, v: Id<Vertex>) -> Option<Id<Edge>> {
    if is_loop(solid, edge) {
        return None;
    }
    let d = leaving_direction(solid, edge, v);
    let found: Vec<Id<Edge>> = vertex_edges(solid, v)
        .into_iter()
        .filter(|&o| o != edge && !is_seam(solid, o) && !is_loop(solid, o))
        .filter(|&o| leaving_direction(solid, o, v).dot(d) <= -CHAIN_ANGLE.cos())
        .filter(|&o| !is_flat(solid, o))
        .collect();
    (found.len() == 1).then(|| found[0])
}

/// Die andere Ecke der Kante.
fn other_end(solid: &Solid, edge: Id<Edge>, v: Id<Vertex>) -> Id<Vertex> {
    let e = &solid.edges[edge];
    if e.start == v { e.end } else { e.start }
}

/// Tangentenkette ab `edge`, höchstens `limit` Kanten (danach abgebrochen).
fn walk_chain(solid: &Solid, edge: Id<Edge>, limit: usize) -> Vec<Id<Edge>> {
    let Some(e) = solid.edges.get(edge) else {
        return Vec::new();
    };
    let mut forward = vec![edge];
    let (mut cur, mut v) = (edge, e.end);
    while forward.len() < limit {
        let Some(next) = smooth_neighbour(solid, cur, v) else {
            break;
        };
        if forward.contains(&next) {
            break;
        }
        forward.push(next);
        v = other_end(solid, next, v);
        cur = next;
    }
    let mut back: Vec<Id<Edge>> = Vec::new();
    let (mut cur, mut v) = (edge, e.start);
    while forward.len() + back.len() < limit {
        let Some(prev) = smooth_neighbour(solid, cur, v) else {
            break;
        };
        if forward.contains(&prev) || back.contains(&prev) {
            break;
        }
        back.push(prev);
        v = other_end(solid, prev, v);
        cur = prev;
    }
    back.reverse();
    back.extend(forward);
    back
}

/// Tangentenkette ab `edge`: die Kante und alle über glatte Ecken anschließenden, in Laufrichtung
/// geordnet (bei geschlossenen Ketten beginnend bei `edge`). Leer bei unbekannter Kante.
pub fn tangent_chain(solid: &Solid, edge: Id<Edge>) -> Vec<Id<Edge>> {
    walk_chain(solid, edge, usize::MAX)
}

fn edge_length(solid: &Solid, edge: Id<Edge>) -> f64 {
    let e = &solid.edges[edge];
    Seg::new(e.curve.clone(), e.t0, e.t1).length()
}

fn faces_of(solid: &Solid, edge: Id<Edge>) -> Vec<Id<Face>> {
    edge_uses(solid, edge).iter().map(|u| u.face).collect()
}

/// Fläche 1 eines Kettenmitglieds aus der seines Vorgängers in der Kette: dieselbe Fläche, wenn
/// sie an das Mitglied grenzt, sonst die Fläche des Mitglieds, die nicht an den Vorgänger grenzt
/// (die glatte Fortsetzung, etwa gerade Wand → Halbzylinder). `None`, wenn das nicht eindeutig ist.
fn continued_face(
    solid: &Solid,
    prev: Id<Edge>,
    prev_face: Option<Id<Face>>,
    member: Id<Edge>,
) -> Option<Id<Face>> {
    let f = prev_face?;
    let own = faces_of(solid, member);
    if own.contains(&f) {
        return Some(f);
    }
    let shared = faces_of(solid, prev);
    let fresh: Vec<Id<Face>> = own.into_iter().filter(|g| !shared.contains(g)).collect();
    (fresh.len() == 1).then(|| fresh[0])
}

/// Fläche 1 für alle Glieder der Kette `members`, ausgehend von der gewählten Kante an Stelle
/// `p`. Deren Angabe gilt unverändert (auch eine falsche – sie wird später benannt abgelehnt);
/// ohne Angabe setzen die Glieder die Fläche fort, die `edge_sides` für sie wählt (erste
/// Verwendung). So liegen Fasenmaße längs der ganzen Kette auf derselben Seite.
fn first_faces(
    solid: &Solid,
    members: &[Id<Edge>],
    p: usize,
    first: Option<Id<Face>>,
) -> Vec<Option<Id<Face>>> {
    let mut faces = vec![None; members.len()];
    faces[p] = first.or_else(|| faces_of(solid, members[p]).first().copied());
    for k in p + 1..members.len() {
        faces[k] = continued_face(solid, members[k - 1], faces[k - 1], members[k]);
    }
    for k in (0..p).rev() {
        faces[k] = continued_face(solid, members[k + 1], faces[k + 1], members[k]);
    }
    faces[p] = first;
    faces
}

/// Gewählte Kanten ohne Doppelte, bei eingeschalteter Tangentenkette mit ihren glatt
/// anschließenden Kanten (P9: je gewählte Kante ihre Kette in Laufrichtung, schon enthaltene
/// Kanten entfallen), gruppiert in Ketten. Die gewählte Kante behält ihre Fläche 1; ihre
/// Kettenmitglieder bekommen die Fläche, die diese glatt fortsetzt. Die Höchstzahl
/// [`MAX_BLEND_EDGES`] gilt nach Ausdehnung und ohne Doppelte.
pub fn select_edges(solid: &Solid, r: &BlendRequest) -> Result<Vec<ChainedEdge>, BlendError> {
    let too_many =
        || BlendError::InvalidParameter(format!("Zu viele Kanten (höchstens {MAX_BLEND_EDGES})"));
    let mut picked: Vec<BlendEdge> = Vec::new();
    for be in &r.edges {
        if solid.edges.get(be.edge).is_none() {
            return Err(BlendError::EdgeNotFound);
        }
        if picked.iter().any(|p| p.edge == be.edge) {
            continue;
        }
        let members = if r.tangent_chain {
            // Eine Kante mehr als erlaubt genügt, um „zu viele“ zu erkennen.
            walk_chain(solid, be.edge, MAX_BLEND_EDGES + 1)
        } else {
            vec![be.edge]
        };
        let p = members.iter().position(|&e| e == be.edge).unwrap_or(0);
        let faces = first_faces(solid, &members, p, be.first_face);
        for (e, first_face) in members.into_iter().zip(faces) {
            if !picked.iter().any(|q| q.edge == e) {
                picked.push(BlendEdge {
                    edge: e,
                    first_face,
                });
            }
        }
        if picked.len() > MAX_BLEND_EDGES {
            return Err(too_many());
        }
    }
    group(solid, &picked)
}

/// Gruppiert die gewählten Kanten in Läufe glatt aufeinanderfolgender gewählter Kanten (auch über
/// den Anfang einer geschlossenen Kette hinweg) und misst sie. Reihenfolge der Ausgabe wie
/// `picked`; Ketten nach ihrer ersten Kante in `picked` nummeriert.
fn group(solid: &Solid, picked: &[BlendEdge]) -> Result<Vec<ChainedEdge>, BlendError> {
    let index = |e: Id<Edge>| picked.iter().position(|p| p.edge == e);
    let mut out: Vec<Option<ChainedEdge>> = vec![None; picked.len()];
    let mut chain = 0;
    for i in 0..picked.len() {
        if out[i].is_some() {
            continue;
        }
        let start = picked[i].edge;
        // (Kante, gegen die Laufrichtung?) in Laufrichtung der Kante `start`.
        let mut run: VecDeque<(Id<Edge>, bool)> = VecDeque::from([(start, false)]);
        let free = |e: Id<Edge>, run: &VecDeque<(Id<Edge>, bool)>| {
            index(e).is_some_and(|j| out[j].is_none()) && !run.iter().any(|(x, _)| *x == e)
        };
        let (mut cur, mut v) = (start, solid.edges[start].end);
        while let Some(next) = smooth_neighbour(solid, cur, v).filter(|&n| free(n, &run)) {
            run.push_back((next, solid.edges[next].start != v));
            v = other_end(solid, next, v);
            cur = next;
        }
        let (mut cur, mut v) = (start, solid.edges[start].start);
        while let Some(prev) = smooth_neighbour(solid, cur, v).filter(|&n| free(n, &run)) {
            run.push_front((prev, solid.edges[prev].end != v));
            v = other_end(solid, prev, v);
            cur = prev;
        }
        let lengths: Vec<f64> = run.iter().map(|(e, _)| edge_length(solid, *e)).collect();
        let total: f64 = lengths.iter().sum();
        let mut offset = 0.0;
        for (&(e, reversed), &length) in run.iter().zip(&lengths) {
            let j = index(e).ok_or_else(|| {
                BlendError::ResultInvalid("Eine Kante der Kette fehlt in der Auswahl".into())
            })?;
            out[j] = Some(ChainedEdge {
                edge: picked[j],
                chain,
                offset,
                length,
                reversed,
                chain_length: total,
            });
            offset += length;
        }
        chain += 1;
    }
    Ok(out.into_iter().flatten().collect())
}
