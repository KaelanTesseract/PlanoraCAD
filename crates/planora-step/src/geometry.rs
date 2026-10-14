//! Geometrie (Spec §3): Punkte, Richtungen, Koordinatensysteme, Kurven und Flächen.

use crate::entity::Entities;
use crate::error::StepError;
use crate::number::push_real;
use planora_kernel::{Curve, Surface, UvRect};
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

/// Kegel (Spec §3): Der Kernel beschreibt den Radius an der Höhe `v` als `radius + slope · v`;
/// STEP verlangt am Ursprung einen Radius ≥ 0 und einen halben Öffnungswinkel in (0, π/2). Die
/// Lage wird deshalb auf die Mitte des v-Bereichs der Fläche verschoben (dort ist der Radius der
/// Fläche positiv) und bei fallendem Radius (`slope < 0`) die Achse umgekehrt; die natürliche
/// Normale (von der Achse weg) bleibt dieselbe. `slope = 0` ist ein Zylinder.
fn cone(
    e: &mut Entities,
    frame: &Frame,
    radius: f64,
    slope: f64,
    patch: Option<UvRect>,
) -> Result<u64, StepError> {
    if !(radius.is_finite() && slope.is_finite()) {
        return Err(StepError::NotFinite);
    }
    if !frame.is_orthonormal(FRAME_TOL) {
        return Err(reject(
            "Ein Koordinatensystem ist schief oder nicht normiert",
        ));
    }
    let Some(p) = patch else {
        return Err(reject("Eine Kegelfläche hat keinen Parameterbereich"));
    };
    let v_mid = 0.5 * (p.v0 + p.v1);
    if !v_mid.is_finite() {
        return Err(StepError::NotFinite);
    }
    let rho = radius + slope * v_mid;
    if rho.is_nan() || rho <= 0.0 {
        return Err(reject("Eine Kegelfläche liegt jenseits ihrer Spitze"));
    }
    let origin = frame.origin + frame.z * v_mid;
    if slope == 0.0 {
        let a = placement(e, origin, frame.z, frame.x)?;
        return with_numbers(e, "CYLINDRICAL_SURFACE", a, &[rho]);
    }
    let z = if slope > 0.0 { frame.z } else { -frame.z };
    let a = placement(e, origin, z, frame.x)?;
    with_numbers(e, "CONICAL_SURFACE", a, &[rho, slope.abs().atan()])
}

/// Trägerfläche einer Fläche; `patch` ist der Parameterbereich der Fläche (für den Kegel).
pub(crate) fn surface(
    e: &mut Entities,
    s: &Surface,
    patch: Option<UvRect>,
) -> Result<u64, StepError> {
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
            if small >= big {
                // Ein Spindel- oder Hornring (kleiner Radius ≥ großer) ist in STEP keine
                // gewöhnliche Torusfläche; eine Sonderfläche ist hier nicht überprüft.
                return Err(reject(
                    "Eine Torusfläche mit kleinem Radius größer oder gleich dem großen lässt sich noch nicht als STEP ausgeben.",
                ));
            }
            let a = frame_placement(e, frame)?;
            with_numbers(e, "TOROIDAL_SURFACE", a, &[big, small])
        }
        Surface::Cone {
            frame,
            radius,
            slope,
        } => cone(e, frame, *radius, *slope, patch),
        Surface::Nurbs(_) => Err(reject("Diese Flächenart wird noch nicht ausgegeben")),
    }
}
