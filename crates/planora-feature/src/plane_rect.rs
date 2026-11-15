//! Das Rechteck einer Konstruktionsebene (Skizzen 6) als gemeinsame Regel für die Anzeige der
//! App und für „3D-Geometrie einbeziehen“ (Skizzen 10, Spec 5, Fix-Runde Task 16): Mitte und
//! halbe Kantenlänge.
//!
//! - **Mitte:** Bei einer Ebene auf einer Fläche (Versatz von einer Fläche, Tangentialebene) die
//!   Mitte des Hüllquaders der Fläche, senkrecht in die Ebene gelegt; sonst der Fußpunkt des
//!   Ursprungs ([`plane_rect_center`], [`outline_anchor`]).
//! - **Größe:** das Größere aus 25 mm und 0,6 × Diagonale der Hüllquader der Körper
//!   ([`plane_half_for_bounds`]). Die App misst an den Netzen aller eingeblendeten Körper;
//!   Einbeziehen und Neuberechnung messen reproduzierbar an den Netzen der Körper **vor** der
//!   Skizze ([`solid_bounds`], feste Sehnentoleranz). Spätere Körper zählen dort nicht mit
//!   (bekannte Abweichung zur Anzeige).

use crate::planes::{PlaneBase, PlaneDef};
use crate::refs::FaceRef;
use planora_kernel::{Face, Id, Solid};
use planora_math::{DVec3, Frame};
use planora_mesh::{Mesh, TessellationOptions, tessellate};

/// Kleinste halbe Kantenlänge in mm.
pub const PLANE_MIN_HALF_MM: f64 = 25.0;
/// Halbe Kantenlänge je mm Diagonale der Körper.
pub const PLANE_HALF_PER_DIAGONAL: f64 = 0.6;

/// Halbe Kantenlänge aus den Hüllquadern `bounds` (unteres, oberes Eck); leere oder ungültige
/// Quader zählen nicht.
pub fn plane_half_for_bounds(bounds: impl IntoIterator<Item = (DVec3, DVec3)>) -> f64 {
    let diagonal = bounds
        .into_iter()
        .filter(|(lo, hi)| lo.is_finite() && hi.is_finite() && lo.cmple(*hi).all())
        .reduce(|(lo, hi), (a, b)| (lo.min(a), hi.max(b)))
        .map_or(0.0, |(lo, hi)| (hi - lo).length());
    let half = (PLANE_HALF_PER_DIAGONAL * diagonal).max(PLANE_MIN_HALF_MM);
    if half.is_finite() {
        half
    } else {
        PLANE_MIN_HALF_MM
    }
}

/// Mitte des Rechtecks der Ebene `frame`: der Anker `anchor` (ohne Anker der Ursprung)
/// senkrecht in die Ebene gelegt.
pub fn plane_rect_center(frame: &Frame, anchor: Option<DVec3>) -> DVec3 {
    let a = anchor.unwrap_or(DVec3::ZERO);
    a - frame.z * (a - frame.origin).dot(frame.z)
}

/// Anker aus den Punkten des Umrisses einer Fläche: die Mitte ihres Hüllquaders.
pub fn outline_anchor(points: &[DVec3]) -> Option<DVec3> {
    let first = *points.first()?;
    let (lo, hi) = points
        .iter()
        .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    Some((lo + hi) * 0.5)
}

/// Die Fläche, auf der eine Ebene mit der Definition `def` sitzt (Versatz von einer Fläche,
/// Tangentialebene); sonst `None`.
pub fn plane_base_face(def: &PlaneDef) -> Option<&FaceRef> {
    match def {
        PlaneDef::Offset {
            base: PlaneBase::Face(r),
            ..
        }
        | PlaneDef::Tangent { face: r, .. } => Some(r),
        _ => None,
    }
}

/// Netz eines Körpers mit der festen Sehnentoleranz der Dokumentrechnung (unabhängig von der
/// Einstellung der Anzeige, damit die Neuberechnung reproduzierbar ist).
fn fixed_mesh(solid: &Solid) -> Option<Mesh> {
    tessellate(solid, &TessellationOptions::default()).ok()
}

/// Hüllquader des Netzes eines Körpers (feste Sehnentoleranz); `None` ohne Netz.
pub fn solid_bounds(solid: &Solid) -> Option<(DVec3, DVec3)> {
    mesh_bounds(&fixed_mesh(solid)?)
}

fn mesh_bounds(mesh: &Mesh) -> Option<(DVec3, DVec3)> {
    let first = *mesh.positions.first()?;
    Some(
        mesh.positions
            .iter()
            .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
    )
}

/// Die Punkte der Dreiecke der Fläche `face` im Netz `mesh` (Umriss für [`outline_anchor`]).
pub fn mesh_face_points(mesh: &Mesh, face: Id<Face>) -> Option<Vec<DVec3>> {
    let range = mesh.face_ranges.iter().find(|fr| fr.face == face)?;
    let first = range.first_index as usize;
    let indices = mesh
        .indices
        .get(first..first + range.index_count as usize)?;
    let pts: Vec<DVec3> = indices
        .iter()
        .filter_map(|&i| mesh.positions.get(i as usize).copied())
        .collect();
    (!pts.is_empty()).then_some(pts)
}

/// Die Punkte der Fläche `face` des Körpers `solid` im Netz mit fester Sehnentoleranz.
pub(crate) fn solid_face_points(solid: &Solid, face: Id<Face>) -> Option<Vec<DVec3>> {
    mesh_face_points(&fixed_mesh(solid)?, face)
}
