use crate::geometry::Surface;
use crate::topology::Solid;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    #[error("Kante {0} wird {1}-mal verwendet (erwartet: 2)")]
    EdgeUseCount(usize, usize),
    #[error("Kante {0} wird nicht genau einmal vorwärts und einmal rückwärts durchlaufen")]
    EdgeOrientation(usize),
    #[error("Eine Schleife der Fläche {0} ist nicht geschlossen")]
    OpenLoop(usize),
    #[error("Eine Schleife der Fläche {0} ist leer")]
    EmptyLoop(usize),
    #[error("Die gekrümmte Fläche {0} hat keinen Parameterbereich")]
    MissingPatch(usize),
    #[error("Die Euler-Poincaré-Formel liefert kein ganzzahliges Geschlecht (V-E+F-R = {0})")]
    InvalidEulerCharacteristic(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// Zahl der Ecken, die von mindestens einer Kante benutzt werden.
    pub vertices: usize,
    pub edges: usize,
    pub faces: usize,
    /// Zahl der inneren Schleifen (Löcher in Flächen).
    pub inner_loops: usize,
    /// Zahl der „Henkel“ (Torus = 1).
    pub genus: usize,
}

/// Prüft Topologie und Orientierung. Dass die Normalen nach außen zeigen, wird nicht hier,
/// sondern in `planora-mesh` über ein positives Volumen geprüft.
pub fn validate(solid: &Solid) -> Result<ValidationReport, ValidationError> {
    let n_edges = solid.edges.len();
    let mut forward = vec![0usize; n_edges];
    let mut backward = vec![0usize; n_edges];
    let mut inner_loops = 0;

    for (fid, face) in solid.faces.iter() {
        if !matches!(face.surface, Surface::Plane { .. }) && face.patch.is_none() {
            return Err(ValidationError::MissingPatch(fid.index()));
        }
        inner_loops += face.inner.len();
        for lp in std::iter::once(&face.outer).chain(face.inner.iter()) {
            if lp.half_edges.is_empty() {
                return Err(ValidationError::EmptyLoop(fid.index()));
            }
            for (i, he) in lp.half_edges.iter().enumerate() {
                if he.forward {
                    forward[he.edge.index()] += 1;
                } else {
                    backward[he.edge.index()] += 1;
                }
                let next = lp.half_edges[(i + 1) % lp.half_edges.len()];
                if solid.half_edge_end(*he) != solid.half_edge_start(next) {
                    return Err(ValidationError::OpenLoop(fid.index()));
                }
            }
        }
    }

    for e in 0..n_edges {
        let total = forward[e] + backward[e];
        if total != 2 {
            return Err(ValidationError::EdgeUseCount(e, total));
        }
        if forward[e] != 1 || backward[e] != 1 {
            return Err(ValidationError::EdgeOrientation(e));
        }
    }

    let used: HashSet<_> = solid
        .edges
        .iter()
        .flat_map(|(_, e)| [e.start, e.end])
        .collect();
    let characteristic =
        used.len() as i64 - n_edges as i64 + solid.faces.len() as i64 - inner_loops as i64;
    let twice_genus = 2 - characteristic;
    if twice_genus < 0 || twice_genus % 2 != 0 {
        return Err(ValidationError::InvalidEulerCharacteristic(characteristic));
    }

    Ok(ValidationReport {
        vertices: used.len(),
        edges: n_edges,
        faces: solid.faces.len(),
        inner_loops,
        genus: (twice_genus / 2) as usize,
    })
}
