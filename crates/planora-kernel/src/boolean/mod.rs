//! Boolesche Operationen (Vereinigen, Abziehen, Schneiden) für Körper mit analytischen Flächen.

mod classify;
mod error;
mod imprint;
mod nurbs_ssi;
mod roots;
mod seg;
mod select;
mod sew;
mod split;
mod surfsurf;
mod uv;

pub use classify::{
    Class, Location, classify_region, edge_probe_point, face_probe_point, solid_contains,
    solid_faces,
};
pub use error::{BoolOp, BooleanError};
pub use imprint::{FaceWork, imprint};
pub use nurbs_ssi::{
    MAX_CURVE_LENGTH_FACTOR, MAX_MARCH_STEPS, MAX_SEEDS, MAX_SUBDIVISION_DEPTH, NURBS_FIT_TOL,
    SsiOutcome, intersect_nurbs,
};
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

use crate::geometry::{Curve, Surface};
use crate::topology::Solid;
use crate::validate::{NURBS_EDGE_TOL, ValidationError, validate};
use planora_math::{DVec3, Frame, Tolerance};

/// Längentoleranz für Knoten, Teilungen und das Verschweißen von Ecken, sobald Freiformkurven
/// oder -flächen beteiligt sind. Schnittkurven des numerischen Rechners liegen nur bis
/// `0,8 · NURBS_FIT_TOL` auf beiden Flächen; ihr Haltepunkt auf einer Randkante (etwa der Naht)
/// kann deshalb bis etwa `√2 · 0,8 · NURBS_FIT_TOL ≈ 1,1e-5` neben dem exakten Kantenpunkt liegen.
/// Die doppelte Kantentoleranz deckt das mit Abstand ab und ist klein gegen jedes Bauteilmaß.
pub(crate) const NURBS_NODE_TOL: f64 = 2.0 * NURBS_EDGE_TOL;

/// Toleranz für Pipeline-Schritte mit Freiformbeteiligung: `tol.linear` mindestens
/// [`NURBS_NODE_TOL`]; ohne Freiformbeteiligung unverändert `tol` (analytische Wege bitgleich).
pub(crate) fn pipeline_tol(tol: &Tolerance, freeform: bool) -> Tolerance {
    if freeform {
        Tolerance {
            linear: tol.linear.max(NURBS_NODE_TOL),
            angular: tol.angular,
        }
    } else {
        *tol
    }
}

/// Ist an dieser Fläche eine Freiformfläche oder -kurve beteiligt (Fläche oder eine Randkante)?
pub(crate) fn face_is_freeform(g: &FaceGeom) -> bool {
    matches!(g.surface, Surface::Nurbs(_)) || g.loops.iter().flatten().any(seg_is_freeform)
}

pub(crate) fn seg_is_freeform(s: &Seg) -> bool {
    matches!(s.curve, Curve::Nurbs(_))
}

/// Liegen `t0` und `t1` im Parameterbereich einer Freiformkurve? Bei geschlossenen Kurven darf
/// ein Stück über die Naht reichen (höchstens eine Periode jenseits des Bereichs). Andere Kurven
/// haben keinen festen Bereich.
fn nurbs_params_ok(c: &Curve, t0: f64, t1: f64) -> bool {
    let Some((a, b)) = c.domain() else {
        return true;
    };
    let slack = c.period().unwrap_or(0.0);
    [t0, t1].iter().all(|t| (a - slack..=b + slack).contains(t))
}

/// Eingangsprüfung: nicht leer, alle Zahlen endlich, Freiformkanten im Parameterbereich, danach
/// der Validator.
fn check_input(s: &Solid) -> Result<(), String> {
    if s.faces.is_empty() {
        return Err("leerer Körper (keine Flächen)".into());
    }
    let finite3 = |p: DVec3| p.is_finite();
    let frame_ok = |f: &Frame| finite3(f.origin) && finite3(f.x) && finite3(f.y) && finite3(f.z);
    if s.vertices.iter().any(|(_, v)| !finite3(v.point)) {
        return Err("Eckpunkt mit ungültiger Koordinate (NaN oder unendlich)".into());
    }
    for (_, e) in s.edges.iter() {
        let curve_ok = match &e.curve {
            Curve::Line { origin, dir } => finite3(*origin) && finite3(*dir),
            Curve::Circle { frame, radius } => frame_ok(frame) && radius.is_finite(),
            Curve::Ellipse {
                frame,
                major,
                minor,
            } => frame_ok(frame) && major.is_finite() && minor.is_finite(),
            // Endlich durch den geprüften Konstruktor.
            Curve::Nurbs(_) => true,
        };
        if !(curve_ok && e.t0.is_finite() && e.t1.is_finite()) {
            return Err("Kante mit ungültigem Wert (NaN oder unendlich)".into());
        }
        if !nurbs_params_ok(&e.curve, e.t0, e.t1) {
            return Err("Kante einer Freiformkurve außerhalb ihres Parameterbereichs".into());
        }
    }
    validate(s).map_err(|e| e.to_string())?;
    Ok(())
}

/// Boolesche Operation `a op b`. Das Ergebnis besteht immer den Validator; sonst ein benannter Fehler.
pub fn boolean(a: &Solid, b: &Solid, op: BoolOp, tol: &Tolerance) -> Result<Solid, BooleanError> {
    check_input(a).map_err(|e| BooleanError::InvalidInput(format!("Körper A: {e}")))?;
    check_input(b).map_err(|e| BooleanError::InvalidInput(format!("Körper B: {e}")))?;

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
    if has_freeform(a) || has_freeform(b) {
        check_freeform_result(&solid, tol)?;
    }
    Ok(solid)
}

/// Hat der Körper eine Freiformfläche oder -kante?
fn has_freeform(s: &Solid) -> bool {
    s.faces
        .iter()
        .any(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        || s.edges
            .iter()
            .any(|(_, e)| matches!(e.curve, Curve::Nurbs(_)))
}

/// Zahl der Abschnitte, an deren Grenzen [`check_freeform_result`] jede Kante prüft.
const RESULT_CHECK_SECTIONS: usize = 8;

/// Meldung, wenn das vernähte Ergebnis einer Operation mit Freiformflächen eine Kante neben ihrer
/// Fläche oder neben ihren Ecken hat.
pub(crate) const THIN_FEATURE: &str = "Ein Gebilde ist dünner als die Toleranz für Freiformflächen; das Ergebnis lässt sich nicht sicher bilden";

/// `x ≤ limit`; NaN ist nie innerhalb. Eigene Funktion, weil die Prüfungen „nicht innerhalb“
/// brauchen und clippy `!(x <= limit)` (gewollt: NaN zählt als außerhalb) ablehnt.
fn within(x: f64, limit: f64) -> bool {
    x <= limit
}

/// Analytische Kanten auf analytischen Flächen: erlaubter Abstand in Vielfachen von `tol.linear`.
/// Analytische Kurven auf analytischen Flächen sind exakt; liegt eine Kante weiter daneben, hat
/// das Verschweißen mit der Knotentoleranz ein dünneres Gebilde zusammengelegt. Mit dem Faktor 4
/// (wie die Mittenprobe der analytischen Pipeline) blieb ein Deckel 1,5e-6 bis 3,9e-6 unter einer
/// Mantelkante ein „gültiger“ Körper, dessen Netz nicht schließt.
const ANALYTIC_EDGE_FACTOR: f64 = 1.0;

/// Geometrische Prüfung des Ergebnisses, wenn Freiformflächen beteiligt sind, zusätzlich zu
/// `validate` (das Kanten mit Freiformbeteiligung schon mit [`NURBS_EDGE_TOL`] prüft): Analytische
/// Kanten auf analytischen Flächen liegen an `RESULT_CHECK_SECTIONS + 1` Stellen höchstens
/// `ANALYTIC_EDGE_FACTOR · tol.linear` (= `tol.linear`) neben jeder Fläche, die sie benutzt (exakte Kurven), und
/// die Kurvenenden jeder Kante liegen höchstens [`NURBS_NODE_TOL`] neben ihren Ecken. Pipeline
/// und Verschweißen arbeiten mit der weiteren Knotentoleranz; ein Gebilde dünner als diese (etwa
/// ein Deckel 1e-5 unter einer Mantelkante) würde sonst still zu einer Fläche verschmolzen, deren
/// Kanten neben ihr liegen. Rein analytische Operationen laufen nicht hierüber (unverändert).
fn check_freeform_result(s: &Solid, tol: &Tolerance) -> Result<(), BooleanError> {
    let limit = ANALYTIC_EDGE_FACTOR * tol.linear;
    let fail = || BooleanError::Unsupported(THIN_FEATURE.into());
    for (_, e) in s.edges.iter() {
        let (p0, p1) = (e.curve.point(e.t0), e.curve.point(e.t1));
        if !within(p0.distance(s.vertices[e.start].point), NURBS_NODE_TOL)
            || !within(p1.distance(s.vertices[e.end].point), NURBS_NODE_TOL)
        {
            return Err(fail());
        }
    }
    for (_, face) in s.faces.iter() {
        for lp in std::iter::once(&face.outer).chain(face.inner.iter()) {
            for he in &lp.half_edges {
                let e = &s.edges[he.edge];
                // Kanten mit Freiformbeteiligung hat `validate` schon so geprüft.
                if matches!(e.curve, Curve::Nurbs(_)) || matches!(face.surface, Surface::Nurbs(_)) {
                    continue;
                }
                let off = (0..=RESULT_CHECK_SECTIONS).any(|k| {
                    let t = e.t0 + (e.t1 - e.t0) * k as f64 / RESULT_CHECK_SECTIONS as f64;
                    // auch NaN gilt als „neben der Fläche“
                    !within(face.surface.implicit(e.curve.point(t)).abs(), limit)
                });
                if off {
                    return Err(fail());
                }
            }
        }
    }
    Ok(())
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
    use planora_math::TAU;
    use planora_math::nurbs::NurbsCurve;

    #[test]
    fn nurbs_edges_must_stay_in_their_parameter_range() {
        let arc = Curve::nurbs(
            NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X, DVec3::Y, 0.0, 1.0).unwrap(),
        );
        assert!(nurbs_params_ok(&arc, 0.0, 1.0));
        assert!(nurbs_params_ok(&arc, 0.7, 0.2));
        assert!(!nurbs_params_ok(&arc, 0.0, 1.5));
        assert!(!nurbs_params_ok(&arc, -0.1, 0.5));
        // geschlossen: ein Stück darf über die Naht reichen, aber nicht beliebig weit
        let circle = Curve::nurbs(
            NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X, DVec3::Y, 0.0, TAU).unwrap(),
        );
        assert!(nurbs_params_ok(&circle, 0.8, 1.3));
        assert!(!nurbs_params_ok(&circle, 0.0, 2.5));
        // analytische Kurven haben keinen festen Bereich
        let line = Curve::Line {
            origin: DVec3::ZERO,
            dir: DVec3::X,
        };
        assert!(nurbs_params_ok(&line, -1e6, 1e6));
    }

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
