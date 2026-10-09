//! Einordnen von Punkten und Flächenstücken relativ zu einem Körper (innen, außen, bündig).
//!
//! Punkte werden mit dem Strahlverfahren eingeordnet: Ein Strahl in einer festen, „unrunden“
//! Richtung zählt die Durchstoßpunkte durch die Flächen des Körpers (ungerade = innen). Trifft
//! der Strahl eine Kante, eine Naht oder berührt er eine Fläche nur, ist das Ergebnis mehrdeutig
//! und es wird die nächste Richtung versucht.

use super::error::BooleanError;
use super::roots::{CurveSurface, curve_surface_roots};
use super::split::Region;
use super::uv::{
    Containment, FaceGeom, face_geom, loop_points, point_in_face, points_to_uv, winding,
};
use crate::geometry::{Curve, Surface};
use crate::topology::Solid;
use planora_math::{DVec2, DVec3, TAU, Tolerance};

/// Lage eines Flächenstücks relativ zum anderen Körper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Inside,
    Outside,
    /// Auf einer Fläche des anderen Körpers, gleiche Außenrichtung.
    OnSame,
    /// Auf einer Fläche des anderen Körpers, entgegengesetzte Außenrichtung.
    OnOpposite,
}

/// Lage eines Punkts relativ zu einem Körper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Inside,
    Outside,
    OnBoundary,
}

/// Geometrie aller Flächen eines Körpers.
pub fn solid_faces(s: &Solid) -> Vec<FaceGeom> {
    s.faces.iter().map(|(id, _)| face_geom(s, id)).collect()
}

/// Feste, „unrunde“ Strahlrichtungen für das Strahlverfahren.
fn directions() -> [DVec3; 6] {
    [
        DVec3::new(0.7321, 0.4123, 0.5471),
        DVec3::new(-0.3911, 0.8312, 0.3977),
        DVec3::new(0.2214, -0.5731, 0.7893),
        DVec3::new(-0.6123, -0.2987, 0.7311),
        DVec3::new(0.9120, -0.2210, -0.3412),
        DVec3::new(-0.1731, 0.3490, -0.9205),
    ]
    .map(|d| d.normalize())
}

fn outward_normal(g: &FaceGeom, p: DVec3) -> DVec3 {
    let (u, v) = g.surface.project(p);
    g.surface.normal(u, v) * if g.same_sense { 1.0 } else { -1.0 }
}

/// Liegt `p` auf einer Fläche von `other`? Liefert die Außennormale dieser Fläche.
fn on_surface(other: &[FaceGeom], p: DVec3, tol: &Tolerance) -> Option<DVec3> {
    other
        .iter()
        .find(|g| {
            g.surface.implicit(p).abs() <= tol.linear
                && point_in_face(g, p, tol) != Containment::Outside
        })
        .map(|g| outward_normal(g, p))
}

/// Strahllänge, die sicher aus dem Körper hinausreicht (von `p` aus gemessen).
fn reach(other: &[FaceGeom], p: DVec3) -> f64 {
    other
        .iter()
        .flat_map(|g| g.loops.iter().flat_map(|l| loop_points(l)))
        .map(|q| q.distance(p))
        .fold(0.0_f64, f64::max)
        * 2.0
        + 1.0
}

/// Strahlverfahren: `Some(true)` = innen, `Some(false)` = außen, `None` = mehrdeutig
/// (Treffer auf einem Rand oder einer Naht, Berührung, Strahl liegt in einer Fläche).
fn cast(other: &[FaceGeom], p: DVec3, dir: DVec3, reach: f64, tol: &Tolerance) -> Option<bool> {
    let ray = Curve::Line { origin: p, dir };
    let mut hits = 0;
    for g in other {
        match curve_surface_roots(&ray, 0.0, reach, &g.surface, tol) {
            CurveSurface::Lies => return None,
            CurveSurface::Roots(rs) => {
                for r in rs {
                    if r.t <= tol.linear {
                        continue;
                    }
                    let hit = p + dir * r.t;
                    match point_in_face(g, hit, tol) {
                        Containment::Boundary => return None,
                        Containment::Inside => {
                            if r.touching {
                                return None;
                            }
                            hits += 1;
                        }
                        Containment::Outside => {}
                    }
                }
            }
        }
    }
    Some(hits % 2 == 1)
}

/// Strahlverfahren mit allen festen Richtungen, bis eine eindeutig ist.
fn cast_all(other: &[FaceGeom], p: DVec3, tol: &Tolerance) -> Result<bool, BooleanError> {
    let reach = reach(other, p);
    directions()
        .into_iter()
        .find_map(|d| cast(other, p, d, reach, tol))
        .ok_or(BooleanError::AmbiguousClassification)
}

/// Lage eines Punkts relativ zu einem Körper (gegeben durch seine Flächen).
pub fn solid_contains(
    other: &[FaceGeom],
    p: DVec3,
    tol: &Tolerance,
) -> Result<Location, BooleanError> {
    if !p.is_finite() {
        return Err(BooleanError::InvalidInput(
            "Der Punkt hat keine endlichen Koordinaten".into(),
        ));
    }
    if on_surface(other, p, tol).is_some() {
        return Ok(Location::OnBoundary);
    }
    Ok(if cast_all(other, p, tol)? {
        Location::Inside
    } else {
        Location::Outside
    })
}

/// Verschiebungen um eine Periode, mit denen ein Punkt gegen aufgerollte Polygone geprüft wird.
fn shifts(surface: &Surface) -> Vec<DVec2> {
    let us: &[f64] = if surface.is_periodic() {
        &[-TAU, 0.0, TAU]
    } else {
        &[0.0]
    };
    let vs: &[f64] = if matches!(surface, Surface::Torus { .. }) {
        &[-TAU, 0.0, TAU]
    } else {
        &[0.0]
    };
    us.iter()
        .flat_map(|&du| vs.iter().map(move |&dv| DVec2::new(du, dv)))
        .collect()
}

/// Ein Punkt im Inneren der Region mit möglichst großem Abstand zum Rand (auch zu Nähten, die
/// keine echten Kanten sind: dort wären Randtests mehrdeutig).
fn interior_point(face: &FaceGeom, region: &Region, tol: &Tolerance) -> Option<DVec3> {
    let outer = points_to_uv(&face.surface, face.same_sense, &loop_points(&region.outer));
    let holes: Vec<Vec<DVec2>> = region
        .holes
        .iter()
        .map(|h| points_to_uv(&face.surface, face.same_sense, &loop_points(h)))
        .collect();
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for p in &outer {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return None;
    }
    let shifts = shifts(&face.surface);
    let sgn = if face.same_sense { 1 } else { -1 };
    let n = 24;
    let mut best: Option<(DVec3, f64)> = None;
    for i in 0..n {
        for j in 0..n {
            let uv = DVec2::new(
                lo.x + (hi.x - lo.x) * (i as f64 + 0.5) / n as f64,
                lo.y + (hi.y - lo.y) * (j as f64 + 0.5) / n as f64,
            );
            // Löcher können um eine Periode versetzt aufgerollt sein.
            let w = winding(&outer, uv)
                + holes
                    .iter()
                    .flat_map(|h| shifts.iter().map(move |s| winding(h, uv + *s)))
                    .sum::<i32>();
            if w * sgn != 1 {
                continue;
            }
            let q = face.surface.point(uv.x, uv.y);
            let d = region
                .outer
                .iter()
                .chain(region.holes.iter().flatten())
                .map(|s| s.closest(q).1)
                .fold(f64::MAX, f64::min);
            if best.is_none_or(|(_, bd)| d > bd) {
                best = Some((q, d));
            }
        }
    }
    best.filter(|(_, d)| *d > tol.linear * 2.0).map(|(q, _)| q)
}

/// Ordnet ein Flächenstück relativ zum anderen Körper ein.
pub fn classify_region(
    face: &FaceGeom,
    region: &Region,
    other: &[FaceGeom],
    tol: &Tolerance,
) -> Result<Class, BooleanError> {
    let q = interior_point(face, region, tol).ok_or(BooleanError::AmbiguousClassification)?;
    let n = outward_normal(face, q);
    if let Some(n2) = on_surface(other, q, tol) {
        return Ok(if n.dot(n2) > 0.0 {
            Class::OnSame
        } else {
            Class::OnOpposite
        });
    }
    Ok(if cast_all(other, q, tol)? {
        Class::Inside
    } else {
        Class::Outside
    })
}
