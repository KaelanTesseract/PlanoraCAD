//! Nachbarschaft einer Kante (Spec §3.1, §4.1): angrenzende Flächen und ihre Lage in deren
//! Schleifen, Kantenwinkel, konvex/konkav, Kanten und Flächen an einer Ecke.

use super::error::BlendError;
use crate::ids::Id;
use crate::topology::{Edge, Face, Solid, Vertex};
use planora_math::DVec3;
use std::f64::consts::PI;

/// Ist `|nA × nB|` kleiner als der Sinus dieses Winkels (2°), gehen die Nachbarflächen fast
/// tangential ineinander über (Kantenwinkel nahe 180°) oder bilden eine Schneide (nahe 0°).
/// Die Kugel hätte dort keine sichere Lage (Spec §3.4).
pub const NEAR_TANGENT_ANGLE: f64 = 2.0 * PI / 180.0;

/// Mindestlänge der normierten Tangente (sonst null oder NaN; das gilt als fast tangential).
const MIN_TANGENT_LEN: f64 = 0.5;

/// Stellen (Bruchteile des Kantenbereichs), an denen Kantenwinkel und Konvexität geprüft werden;
/// die mittlere liefert den gemeldeten Winkel.
const ANGLE_PROBES: [f64; 3] = [0.25, 0.5, 0.75];

/// Verwendung einer Kante in einer Schleife einer Fläche.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeUse {
    pub face: Id<Face>,
    /// 0 = äußere Schleife, `k + 1` = Loch `k`.
    pub loop_index: usize,
    /// Stelle der Halbkante in der Schleife.
    pub position: usize,
    /// Läuft die Halbkante in Kantenrichtung (`start → end`)?
    pub forward: bool,
}

/// Die beiden Seiten einer Kante: Fläche 1 (`a`) und Fläche 2 (`b`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeSides {
    pub a: EdgeUse,
    pub b: EdgeUse,
}

/// Kantenwinkel an der Kantenmitte.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeAngle {
    /// Konvex: Das Material bildet an der Kante einen Winkel unter 180° (Rundung trägt ab).
    pub convex: bool,
    /// Innenwinkel des Materials (Bogenmaß; unter π konvex, über π konkav).
    pub interior: f64,
}

/// Alle Verwendungen der Kante, nach Fläche, Schleife und Stelle geordnet.
pub fn edge_uses(solid: &Solid, edge: Id<Edge>) -> Vec<EdgeUse> {
    let mut out = Vec::new();
    for (fid, f) in solid.faces.iter() {
        for (li, lp) in std::iter::once(&f.outer).chain(f.inner.iter()).enumerate() {
            for (position, he) in lp.half_edges.iter().enumerate() {
                if he.edge == edge {
                    out.push(EdgeUse {
                        face: fid,
                        loop_index: li,
                        position,
                        forward: he.forward,
                    });
                }
            }
        }
    }
    out
}

/// Die beiden Seiten der Kante. `first_face` legt Fläche 1 fest (sonst die erste Verwendung).
/// Fehler: Kante nicht im Körper, nicht genau zwei Verwendungen, Naht (beide Seiten dieselbe
/// Fläche) oder `first_face` grenzt nicht an die Kante.
pub fn edge_sides(
    solid: &Solid,
    edge: Id<Edge>,
    first_face: Option<Id<Face>>,
) -> Result<EdgeSides, BlendError> {
    if solid.edges.get(edge).is_none() {
        return Err(BlendError::EdgeNotFound);
    }
    let uses = edge_uses(solid, edge);
    if uses.len() != 2 {
        return Err(BlendError::InvalidInput(format!(
            "Die Kante wird {}-mal statt zweimal verwendet",
            uses.len()
        )));
    }
    if uses[0].face == uses[1].face {
        return Err(BlendError::SeamEdge);
    }
    let (a, b) = match first_face {
        Some(f) if uses[1].face == f => (uses[1], uses[0]),
        Some(f) if uses[0].face != f => {
            return Err(BlendError::InvalidInput(
                "Die gewählte erste Fläche grenzt nicht an die Kante".into(),
            ));
        }
        _ => (uses[0], uses[1]),
    };
    Ok(EdgeSides { a, b })
}

/// Außennormale der Fläche an einem Punkt auf ihr.
pub fn outward_normal(face: &Face, p: DVec3) -> DVec3 {
    let (u, v) = face.surface.project(p);
    let n = face.surface.normal(u, v);
    if face.same_sense { n } else { -n }
}

/// Einheitstangente der Kante in Kantenrichtung (`start → end`) beim Kurvenparameter `t`.
fn edge_tangent(e: &Edge, t: f64) -> DVec3 {
    (e.curve.tangent(t) * (e.t1 - e.t0).signum()).normalize_or_zero()
}

/// Kantenwinkel und konvex/konkav, geprüft an drei Stellen. Konvex heißt
/// `(nA × nB) · T_A > 0` mit `T_A` = Laufrichtung der Halbkante in Fläche A (Schleifen laufen
/// gegen den Uhrzeigersinn um die Außennormale). Fast tangentiale Flächen oder eine Schneide
/// ergeben `NearlyTangent`, ein Wechsel zwischen konvex und konkav `Unsupported`.
/// `sides` muss von `edge_sides` für dieselbe Kante stammen (sonst Panik bei fremden Ids).
pub fn edge_angle(
    solid: &Solid,
    edge: Id<Edge>,
    sides: &EdgeSides,
) -> Result<EdgeAngle, BlendError> {
    let e = &solid.edges[edge];
    let (fa, fb) = (&solid.faces[sides.a.face], &solid.faces[sides.b.face]);
    let mut first: Option<bool> = None;
    let mut middle: Option<EdgeAngle> = None;
    for f in ANGLE_PROBES {
        let t = e.t0 + (e.t1 - e.t0) * f;
        let p = e.curve.point(t);
        let tangent = edge_tangent(e, t);
        let ta = if sides.a.forward { tangent } else { -tangent };
        let (na, nb) = (outward_normal(fa, p), outward_normal(fb, p));
        let cross = na.cross(nb);
        let phi = na.dot(nb).clamp(-1.0, 1.0).acos();
        let convex = cross.dot(ta) > 0.0;
        let interior = if convex { PI - phi } else { PI + phi };
        // NaN (Tangente null, Normale null) zählt wie „fast tangential“.
        let sharp = cross.length() >= NEAR_TANGENT_ANGLE.sin() && ta.length() > MIN_TANGENT_LEN;
        if !sharp {
            return Err(BlendError::NearlyTangent {
                angle_deg: interior.to_degrees(),
            });
        }
        match first {
            None => first = Some(convex),
            Some(c) if c != convex => {
                return Err(BlendError::Unsupported(
                    "Die Kante wechselt zwischen konvex und konkav; das wird nicht unterstützt"
                        .into(),
                ));
            }
            Some(_) => {}
        }
        if f == 0.5 {
            middle = Some(EdgeAngle { convex, interior });
        }
    }
    middle.ok_or(BlendError::SpineLost)
}

/// Richtungen von der Kante in Fläche A bzw. B hinein (senkrecht zur Kante, in der Tangentialebene
/// der Fläche) beim Kurvenparameter `t`. `sides` muss von `edge_sides` für dieselbe Kante stammen.
/// Das Innere einer Fläche liegt links ihrer Schleife,
/// also in Richtung `n × T`; Fläche B durchläuft die Kante entgegengesetzt.
pub fn inward_directions(
    solid: &Solid,
    edge: Id<Edge>,
    sides: &EdgeSides,
    t: f64,
) -> (DVec3, DVec3) {
    let e = &solid.edges[edge];
    let p = e.curve.point(t);
    let tangent = edge_tangent(e, t);
    let ta = if sides.a.forward { tangent } else { -tangent };
    let na = outward_normal(&solid.faces[sides.a.face], p);
    let nb = outward_normal(&solid.faces[sides.b.face], p);
    (
        na.cross(ta).normalize_or_zero(),
        nb.cross(-ta).normalize_or_zero(),
    )
}

/// Kanten mit `v` als Anfang oder Ende (aufsteigend nach Id).
pub fn vertex_edges(solid: &Solid, v: Id<Vertex>) -> Vec<Id<Edge>> {
    solid
        .edges
        .iter()
        .filter(|(_, e)| e.start == v || e.end == v)
        .map(|(id, _)| id)
        .collect()
}

/// Flächen, deren Schleifen durch `v` laufen (aufsteigend, ohne Doppelte).
pub fn vertex_faces(solid: &Solid, v: Id<Vertex>) -> Vec<Id<Face>> {
    solid
        .faces
        .iter()
        .filter(|(_, f)| {
            std::iter::once(&f.outer)
                .chain(f.inner.iter())
                .flat_map(|lp| lp.half_edges.iter())
                .any(|he| solid.half_edge_start(*he) == v)
        })
        .map(|(id, _)| id)
        .collect()
}

/// Name einer Fläche für Meldungen: Rolle der Marke, sonst „Fläche n“.
pub fn face_name(solid: &Solid, face: Id<Face>) -> String {
    solid
        .faces
        .get(face)
        .and_then(|f| f.tag.as_ref())
        .map(|t| t.role.clone())
        .unwrap_or_else(|| format!("Fläche {}", face.index() + 1))
}
