//! Stabile Verweise auf Flächen und Kanten (Rolle + Herkunft, Hinweispunkt als zweite Stufe).
//!
//! Ein Verweis nennt die Marke ([`FaceTag`]) einer Fläche. Gibt es nach einer Neuberechnung
//! mehrere Flächen mit derselben Marke (etwa weil eine Boolesche Operation eine Fläche geteilt
//! hat), entscheidet der Abstand ihres Prüfpunkts zum gespeicherten Hinweispunkt.

use std::collections::BTreeMap;

use crate::error::FeatureError;
use planora_kernel::{Edge, Face, FaceTag, Id, Solid, edge_probe_point, face_probe_point};
use planora_math::{DVec3, Tolerance};

/// Verweis auf eine Fläche über ihre Marke und einen Hinweispunkt.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceRef {
    pub origin: u64,
    pub role: String,
    pub hint: DVec3,
}

impl FaceRef {
    pub fn tag(&self) -> FaceTag {
        FaceTag {
            origin: self.origin,
            role: self.role.clone(),
        }
    }

    /// Verweis auf `face` von `solid`. Die Fläche muss eine Marke mit zugewiesener Herkunft
    /// (`origin != 0`) und einen Prüfpunkt haben.
    pub fn from_face(
        solid: &Solid,
        face: Id<Face>,
        tol: &Tolerance,
    ) -> Result<FaceRef, FeatureError> {
        let tag = assigned_tag(solid, face).ok_or(FeatureError::UntaggedFace)?;
        let hint = face_probe_point(solid, face, tol)
            .ok_or_else(|| FeatureError::InvalidInput("Die Fläche hat keinen Prüfpunkt".into()))?;
        Ok(FaceRef {
            origin: tag.origin,
            role: tag.role.clone(),
            hint,
        })
    }
}

/// Ergebnis von [`resolve_face`]: Körperindex, Fläche und der aktualisierte Hinweispunkt.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedFace {
    pub body: usize,
    pub face: Id<Face>,
    pub hint: DVec3,
}

/// Sucht die Fläche, auf die `r` verweist, in allen `solids`.
///
/// Keine Fläche mit der Marke → [`FeatureError::FaceNotFound`]; mehrere → die mit dem kleinsten
/// Abstand ihres Prüfpunkts zum Hinweispunkt (Gleichstand: kleinerer Körperindex, dann kleinere
/// Flächen-Id). Flächen ohne Prüfpunkt kommen nur zum Zug, wenn keine andere einen hat.
pub fn resolve_face(
    solids: &[&Solid],
    r: &FaceRef,
    tol: &Tolerance,
) -> Result<ResolvedFace, FeatureError> {
    let tag = r.tag();
    // Kandidaten in aufsteigender Reihenfolge (Körper, Fläche); `min_by` behält bei
    // Gleichstand den ersten und erfüllt damit die Gleichstandsregel.
    let candidates: Vec<(usize, Id<Face>, Option<DVec3>)> = solids
        .iter()
        .enumerate()
        .flat_map(|(bi, s)| {
            s.faces_with_tag(&tag)
                .into_iter()
                .map(move |f| (bi, f, face_probe_point(s, f, tol)))
        })
        .collect();
    let best = candidates
        .iter()
        .min_by(|a, b| distance(a.2, r.hint).total_cmp(&distance(b.2, r.hint)))
        .ok_or_else(|| FeatureError::FaceNotFound {
            origin: r.origin,
            role: r.role.clone(),
        })?;
    Ok(ResolvedFace {
        body: best.0,
        face: best.1,
        hint: best.2.unwrap_or(r.hint),
    })
}

/// Verweis auf eine Kante über die beiden angrenzenden Flächen und einen Hinweispunkt.
/// Bei einer Naht (die Kante grenzt zweimal an dieselbe Fläche) verweisen `a` und `b` auf
/// dieselbe Fläche.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeRef {
    pub a: FaceRef,
    pub b: FaceRef,
    pub hint: DVec3,
}

impl EdgeRef {
    /// Verweis auf `edge` von `solid` über die ersten beiden Flächen-Verwendungen der Kante.
    /// Weniger als zwei Verwendungen oder eine unmarkierte Fläche → [`FeatureError::UntaggedEdge`].
    pub fn from_edge(
        solid: &Solid,
        edge: Id<Edge>,
        tol: &Tolerance,
    ) -> Result<EdgeRef, FeatureError> {
        let uses = edge_uses(solid);
        let faces = uses
            .get(&edge)
            .filter(|u| u.len() >= 2)
            .ok_or(FeatureError::UntaggedEdge)?;
        let hint = edge_probe_point(solid, edge)
            .ok_or_else(|| FeatureError::InvalidInput("Die Kante hat keinen Prüfpunkt".into()))?;
        // Hat eine angrenzende Fläche keinen eigenen Prüfpunkt (sehr schmale Fläche), dient der
        // Kantenpunkt als ihr Hinweis; er liegt auf dem Rand der Fläche.
        let face_ref = |face: Id<Face>| -> Result<FaceRef, FeatureError> {
            let tag = assigned_tag(solid, face).ok_or(FeatureError::UntaggedEdge)?;
            Ok(FaceRef {
                origin: tag.origin,
                role: tag.role.clone(),
                hint: face_probe_point(solid, face, tol).unwrap_or(hint),
            })
        };
        Ok(EdgeRef {
            a: face_ref(faces[0])?,
            b: face_ref(faces[1])?,
            hint,
        })
    }
}

/// Ergebnis von [`resolve_edge`]: Körperindex, Kante und der aktualisierte Hinweispunkt.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedEdge {
    pub body: usize,
    pub edge: Id<Edge>,
    pub hint: DVec3,
}

/// Sucht die Kante, auf die `r` verweist: Kandidaten sind Kanten, die von einer Fläche mit der
/// Marke von `a` und (in einer anderen Verwendung) von einer mit der Marke von `b` benutzt
/// werden. Mehrere → kleinster Abstand des Kantenpunkts zum Hinweispunkt (Gleichstand:
/// kleinerer Körperindex, dann kleinere Kanten-Id); keine → [`FeatureError::EdgeNotFound`].
pub fn resolve_edge(
    solids: &[&Solid],
    r: &EdgeRef,
    _tol: &Tolerance,
) -> Result<ResolvedEdge, FeatureError> {
    let (ta, tb) = (r.a.tag(), r.b.tag());
    let mut candidates: Vec<(usize, Id<Edge>, Option<DVec3>)> = Vec::new();
    for (bi, s) in solids.iter().enumerate() {
        let has = |f: Id<Face>, t: &FaceTag| s.faces.get(f).and_then(|f| f.tag.as_ref()) == Some(t);
        for (edge, faces) in edge_uses(s) {
            let matches = faces.iter().enumerate().any(|(i, &fa)| {
                has(fa, &ta)
                    && faces
                        .iter()
                        .enumerate()
                        .any(|(j, &fb)| i != j && has(fb, &tb))
            });
            if matches {
                candidates.push((bi, edge, edge_probe_point(s, edge)));
            }
        }
    }
    let best = candidates
        .iter()
        .min_by(|a, b| distance(a.2, r.hint).total_cmp(&distance(b.2, r.hint)))
        .ok_or_else(|| FeatureError::EdgeNotFound {
            a: r.a.role.clone(),
            b: r.b.role.clone(),
        })?;
    Ok(ResolvedEdge {
        body: best.0,
        edge: best.1,
        hint: best.2.unwrap_or(r.hint),
    })
}

/// Marke der Fläche, falls vorhanden und einem Feature zugewiesen (`origin != 0`).
fn assigned_tag(solid: &Solid, face: Id<Face>) -> Option<&FaceTag> {
    solid
        .faces
        .get(face)?
        .tag
        .as_ref()
        .filter(|t| t.origin != 0)
}

/// Abstand eines (womöglich fehlenden) Prüfpunkts zum Hinweis; ohne Prüfpunkt unendlich.
fn distance(probe: Option<DVec3>, hint: DVec3) -> f64 {
    probe.map_or(f64::INFINITY, |p| p.distance(hint))
}

/// Für jede Kante die Flächen, deren Schleifen sie benutzen, in der Reihenfolge
/// (Fläche aufsteigend, äußere vor inneren Schleifen, Halbkanten-Reihenfolge). Benutzt eine
/// Fläche die Kante zweimal (Naht), steht sie zweimal in der Liste.
fn edge_uses(solid: &Solid) -> BTreeMap<Id<Edge>, Vec<Id<Face>>> {
    let mut uses: BTreeMap<Id<Edge>, Vec<Id<Face>>> = BTreeMap::new();
    for (fid, face) in solid.faces.iter() {
        for lp in std::iter::once(&face.outer).chain(face.inner.iter()) {
            for he in &lp.half_edges {
                uses.entry(he.edge).or_default().push(fid);
            }
        }
    }
    uses
}
