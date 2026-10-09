//! Boolesche Operationen (Vereinigen, Abziehen, Schneiden) für Körper mit analytischen Flächen.

mod classify;
mod error;
mod imprint;
mod roots;
mod seg;
mod select;
mod sew;
mod split;
mod surfsurf;
mod uv;

pub use classify::{Class, Location, classify_region, solid_contains, solid_faces};
pub use error::{BoolOp, BooleanError};
pub use imprint::{FaceWork, imprint};
pub use roots::{CurveSurface, Root, curve_surface_roots, seg_intersections};
pub use seg::{Seg, reverse_loop};
pub use select::{Piece, Side, select_piece};
pub use sew::sew;
pub use split::{Region, split_face};
pub use surfsurf::{SurfaceIntersection, intersect_surfaces, surface_name};
pub use uv::{
    Containment, FaceGeom, face_geom, loop_points, oriented_area, point_in_face, points_to_uv,
    points_to_uv_indexed, winding,
};

use crate::topology::Solid;
use crate::validate::{ValidationError, validate};
use planora_math::Tolerance;

/// Boolesche Operation `a op b`. Das Ergebnis besteht immer den Validator; sonst ein benannter Fehler.
pub fn boolean(a: &Solid, b: &Solid, op: BoolOp, tol: &Tolerance) -> Result<Solid, BooleanError> {
    validate(a).map_err(|e| BooleanError::InvalidInput(format!("Körper A: {e}")))?;
    validate(b).map_err(|e| BooleanError::InvalidInput(format!("Körper B: {e}")))?;

    let (work_a, work_b) = imprint(a, b, tol)?;
    let faces_a = solid_faces(a);
    let faces_b = solid_faces(b);

    let mut pieces: Vec<Piece> = Vec::new();
    // Liegt irgendein Flächenstück im Inneren des anderen Körpers, überlappen sich die Körper.
    let mut interiors_overlap = false;
    for (side, works, other) in [(Side::A, &work_a, &faces_b), (Side::B, &work_b, &faces_a)] {
        for w in works {
            for region in split_face(&w.geom, &w.sections, tol)? {
                let class = classify_region(&w.geom, &region, other, tol)?;
                interiors_overlap |= class == Class::Inside;
                if let Some(p) = select_piece(op, side, class, &w.geom, region) {
                    pieces.push(p);
                }
            }
        }
    }
    if pieces.is_empty() {
        return Err(BooleanError::EmptyResult);
    }
    let solid = sew(pieces, tol)?;
    validate(&solid).map_err(|e| result_error(e, interiors_overlap))?;
    Ok(solid)
}

/// Fehler für ein Ergebnis, das den Validator nicht besteht.
///
/// Kriterium für den Berührungsfall: Eine Kante, die mehr als zweimal benutzt wird, entsteht,
/// wenn sich die Körper nur an einer Kante berühren (Spec-Nicht-Ziel). Das wird nur dann als
/// `AmbiguousContact` gemeldet, wenn kein Flächenstück eines Körpers im Inneren des anderen
/// liegt (`interiors_overlap == false`): Dann haben die Körper kein gemeinsames Volumen, und es
/// gibt nichts zu verschneiden. Überlappen sie sich, bleibt es `ResultInvalid`, damit echte
/// Fehler etwa beim Zusammennähen sichtbar bleiben.
fn result_error(e: ValidationError, interiors_overlap: bool) -> BooleanError {
    match e {
        ValidationError::EdgeUseCount(_, n) if n > 2 && !interiors_overlap => {
            BooleanError::AmbiguousContact
        }
        e => BooleanError::ResultInvalid(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_use_count_is_contact_only_without_interior_overlap() {
        let multi = ValidationError::EdgeUseCount(5, 4);
        assert_eq!(
            result_error(multi.clone(), false),
            BooleanError::AmbiguousContact
        );
        // Überlappen sich die Körper, ist eine mehrfach benutzte Kante ein echter Fehler
        // (z. B. beim Zusammennähen) und darf nicht als Berührung verdeckt werden.
        assert!(matches!(
            result_error(multi, true),
            BooleanError::ResultInvalid(m) if m.contains("4-mal")
        ));
        assert!(matches!(
            result_error(ValidationError::EdgeUseCount(5, 1), false),
            BooleanError::ResultInvalid(_)
        ));
        assert!(matches!(
            result_error(ValidationError::OpenLoop(1), false),
            BooleanError::ResultInvalid(_)
        ));
    }
}
