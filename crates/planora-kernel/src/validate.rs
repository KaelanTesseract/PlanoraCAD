use crate::geometry::{Curve, Surface};
use crate::ids::Id;
use crate::topology::{Edge, Face, Solid, Vertex};
use planora_math::Tolerance;
use std::collections::HashSet;
use thiserror::Error;

/// Toleranz (mm), mit der eine Ecke oder Kante auf einer Fläche liegt, wenn Freiformkurven oder
/// -flächen beteiligt sind (Annäherung von Schnittkurven, Spec N4).
pub const NURBS_EDGE_TOL: f64 = 1e-5;

/// Toleranz für „Kante liegt auf Fläche“: `tol.linear`, mit einer Freiformkurve oder -fläche als
/// Beteiligter mindestens [`NURBS_EDGE_TOL`].
pub fn edge_surface_tol(curve: &Curve, surface: &Surface, tol: &Tolerance) -> f64 {
    if matches!(curve, Curve::Nurbs(_)) {
        tol.linear.max(NURBS_EDGE_TOL)
    } else {
        point_surface_tol(surface, tol)
    }
}

/// Toleranz für „Punkt liegt auf Fläche“: `tol.linear`, bei Freiformflächen mindestens
/// [`NURBS_EDGE_TOL`].
pub fn point_surface_tol(surface: &Surface, tol: &Tolerance) -> f64 {
    if matches!(surface, Surface::Nurbs(_)) {
        tol.linear.max(NURBS_EDGE_TOL)
    } else {
        tol.linear
    }
}

/// Jede Kante mit Freiformkurve oder an einer Freiformfläche liegt auf dieser Fläche.
fn check_freeform_edges(solid: &Solid) -> Result<(), ValidationError> {
    let tol = Tolerance::default();
    for (fid, face) in solid.faces.iter() {
        for lp in std::iter::once(&face.outer).chain(face.inner.iter()) {
            for he in &lp.half_edges {
                let e = &solid.edges[he.edge];
                let freeform =
                    matches!(e.curve, Curve::Nurbs(_)) || matches!(face.surface, Surface::Nurbs(_));
                if !freeform {
                    continue;
                }
                let limit = edge_surface_tol(&e.curve, &face.surface, &tol);
                let off = (0..=EDGE_CHECK_SECTIONS).any(|k| {
                    let t = e.t0 + (e.t1 - e.t0) * k as f64 / EDGE_CHECK_SECTIONS as f64;
                    // `implicit` ist nie NaN; ein nicht endlicher Punkt ergibt einen riesigen Wert.
                    face.surface.implicit(e.curve.point(t)).abs() > limit
                });
                if off {
                    return Err(ValidationError::EdgeOffSurface(
                        he.edge.index(),
                        fid.index(),
                    ));
                }
            }
        }
    }
    Ok(())
}

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
    #[error("Die Fläche {0} gehört nicht zu genau einer Schale")]
    ShellMembership(usize),
    #[error("Die Kante {0} liegt nicht auf der Fläche {1} (Freiformkurve oder -fläche beteiligt)")]
    EdgeOffSurface(usize, usize),
}

/// Zahl der Abschnitte, an deren Grenzen eine Kante mit Freiformbeteiligung auf ihrer Fläche
/// geprüft wird (Enden eingeschlossen).
const EDGE_CHECK_SECTIONS: usize = 8;

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
/// sondern in `planora-mesh` über ein positives Volumen geprüft (für Freiformflächen genauso).
///
/// Ist eine Freiformkurve oder -fläche beteiligt, wird zusätzlich geprüft, dass jede Kante auf
/// den Flächen liegt, die sie begrenzt (Abstand höchstens [`edge_surface_tol`], also
/// [`NURBS_EDGE_TOL`]); rein analytische Körper werden wie bisher nur topologisch geprüft.
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

    check_freeform_edges(solid)?;

    // Euler-Poincaré je Schale: V − E + F − R = 2 − 2g (Henkel g); ohne Schalen gilt das Solid
    // als eine Schale.
    let shell_faces: Vec<Vec<Id<Face>>> = if solid.shells.is_empty() {
        vec![solid.faces.iter().map(|(id, _)| id).collect()]
    } else {
        solid.shells.iter().map(|s| s.faces.clone()).collect()
    };
    // Jede Fläche gehört zu genau einer Schale.
    let mut membership = vec![0usize; solid.faces.len()];
    for &fid in shell_faces.iter().flatten() {
        match membership.get_mut(fid.index()) {
            Some(m) => *m += 1,
            None => return Err(ValidationError::ShellMembership(fid.index())),
        }
    }
    if let Some(f) = membership.iter().position(|&m| m != 1) {
        return Err(ValidationError::ShellMembership(f));
    }
    let mut total_genus = 0usize;
    let mut used_all: HashSet<Id<Vertex>> = HashSet::new();
    for faces in &shell_faces {
        let mut edges: HashSet<Id<Edge>> = HashSet::new();
        let mut rings = 0usize;
        for &fid in faces {
            let face = &solid.faces[fid];
            rings += face.inner.len();
            for lp in std::iter::once(&face.outer).chain(face.inner.iter()) {
                for he in &lp.half_edges {
                    edges.insert(he.edge);
                }
            }
        }
        let verts: HashSet<Id<Vertex>> = edges
            .iter()
            .flat_map(|&e| [solid.edges[e].start, solid.edges[e].end])
            .collect();
        used_all.extend(verts.iter().copied());
        let characteristic =
            verts.len() as i64 - edges.len() as i64 + faces.len() as i64 - rings as i64;
        let twice_genus = 2 - characteristic;
        if twice_genus < 0 || twice_genus % 2 != 0 {
            return Err(ValidationError::InvalidEulerCharacteristic(characteristic));
        }
        total_genus += (twice_genus / 2) as usize;
    }

    Ok(ValidationReport {
        vertices: used_all.len(),
        edges: n_edges,
        faces: solid.faces.len(),
        inner_loops,
        genus: total_genus,
    })
}
