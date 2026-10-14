//! Geometrie (Spec §3): Punkte, Richtungen, Koordinatensysteme, Kurven und Flächen.

use crate::entity::Entities;
use crate::error::StepError;
use crate::number::push_real;
use planora_kernel::{Curve, Surface};
use planora_math::{DVec3, Frame};

/// Abweichung, bis zu der ein Koordinatensystem des Kernels als senkrecht und normiert gilt.
/// Die Systeme des Kernels weichen um Rundungsreste (etwa 1e-15) ab; schief gebaute Systeme
/// werden benannt abgelehnt statt falsch geschrieben.
pub const FRAME_TOL: f64 = 1e-9;

/// Ablehnung eines Körpers ohne Namen; [`crate::check_body`] setzt den Namen ein.
pub(crate) fn reject(reason: &str) -> StepError {
    StepError::InvalidBody {
        name: String::new(),
        reason: reason.to_string(),
    }
}

/// Hängt `(x,y,z)` an.
fn push_triple(s: &mut String, v: DVec3) -> Result<(), StepError> {
    s.push('(');
    push_real(s, v.x)?;
    s.push(',');
    push_real(s, v.y)?;
    s.push(',');
    push_real(s, v.z)?;
    s.push(')');
    Ok(())
}

pub(crate) fn point(e: &mut Entities, p: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("CARTESIAN_POINT('',");
    push_triple(&mut s, p)?;
    s.push(')');
    e.push(&s)
}

pub(crate) fn direction(e: &mut Entities, d: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("DIRECTION('',");
    push_triple(&mut s, d)?;
    s.push(')');
    e.push(&s)
}

/// `AXIS2_PLACEMENT_3D` mit Ursprung, z-Achse (`axis`) und x-Achse (`ref_direction`).
pub(crate) fn placement(
    e: &mut Entities,
    origin: DVec3,
    z: DVec3,
    x: DVec3,
) -> Result<u64, StepError> {
    let p = point(e, origin)?;
    let a = direction(e, z)?;
    let r = direction(e, x)?;
    e.push(&format!("AXIS2_PLACEMENT_3D('',#{p},#{a},#{r})"))
}

/// Koordinatensystem des Kernels; schiefe oder nicht normierte Systeme werden abgelehnt.
pub(crate) fn frame_placement(e: &mut Entities, f: &Frame) -> Result<u64, StepError> {
    if !f.is_orthonormal(FRAME_TOL) {
        return Err(reject(
            "Ein Koordinatensystem ist schief oder nicht normiert",
        ));
    }
    placement(e, f.origin, f.z, f.x)
}

/// Eintrag `NAME('',#placement,zahl,…)`.
fn with_numbers(
    e: &mut Entities,
    name: &str,
    placement: u64,
    numbers: &[f64],
) -> Result<u64, StepError> {
    let mut s = format!("{name}('',#{placement}");
    for x in numbers {
        s.push(',');
        push_real(&mut s, *x)?;
    }
    s.push(')');
    e.push(&s)
}

/// Prüft eine Länge, die in STEP positiv sein muss (Radius, Halbachse).
fn positive(x: f64, what: &str) -> Result<f64, StepError> {
    if !x.is_finite() {
        return Err(StepError::NotFinite);
    }
    if x > 0.0 {
        Ok(x)
    } else {
        Err(reject(&format!("{what} ist nicht größer als null")))
    }
}

/// Trägerkurve einer Kante.
pub(crate) fn curve(e: &mut Entities, c: &Curve) -> Result<u64, StepError> {
    match c {
        Curve::Line { origin, dir } => {
            let p = point(e, *origin)?;
            let d = direction(e, *dir)?;
            let v = e.push(&format!("VECTOR('',#{d},1.)"))?;
            e.push(&format!("LINE('',#{p},#{v})"))
        }
        Curve::Circle { frame, radius } => {
            let r = positive(*radius, "Der Radius eines Kreises")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "CIRCLE", a, &[r])
        }
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => {
            let (a, b) = (
                positive(*major, "Eine Halbachse einer Ellipse")?,
                positive(*minor, "Eine Halbachse einer Ellipse")?,
            );
            if b > a {
                // STEP legt die erste Halbachse in die Bezugsrichtung; die längere kommt dorthin
                // (x' = y, y' = −x, derselbe Umlaufsinn um z). Der Parameter verschiebt sich um
                // π/2; die Kante wird über ihre Eckpunkte begrenzt.
                if !frame.is_orthonormal(FRAME_TOL) {
                    return Err(reject(
                        "Ein Koordinatensystem ist schief oder nicht normiert",
                    ));
                }
                let p = placement(e, frame.origin, frame.z, frame.y)?;
                with_numbers(e, "ELLIPSE", p, &[b, a])
            } else {
                let p = frame_placement(e, frame)?;
                with_numbers(e, "ELLIPSE", p, &[a, b])
            }
        }
        Curve::Nurbs(_) => Err(reject("Diese Kantenart wird noch nicht ausgegeben")),
    }
}

/// Trägerfläche einer Fläche.
pub(crate) fn surface(e: &mut Entities, s: &Surface) -> Result<u64, StepError> {
    match s {
        Surface::Plane { frame } => {
            let a = frame_placement(e, frame)?;
            e.push(&format!("PLANE('',#{a})"))
        }
        Surface::Cylinder { frame, radius } => {
            let r = positive(*radius, "Der Radius eines Zylinders")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "CYLINDRICAL_SURFACE", a, &[r])
        }
        Surface::Sphere { frame, radius } => {
            let r = positive(*radius, "Der Radius einer Kugel")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "SPHERICAL_SURFACE", a, &[r])
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            let big = positive(*major, "Der große Radius eines Torus")?;
            let small = positive(*minor, "Der kleine Radius eines Torus")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "TOROIDAL_SURFACE", a, &[big, small])
        }
        Surface::Cone { .. } | Surface::Nurbs(_) => {
            Err(reject("Diese Flächenart wird noch nicht ausgegeben"))
        }
    }
}
