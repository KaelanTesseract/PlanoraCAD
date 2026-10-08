use crate::error::KernelError;
use crate::extrude::extrude;
use crate::profile::{Profile, ProfileLoop, Segment};
use crate::revolve::revolve;
use crate::topology::Solid;
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};

fn check(dimension: f64, tol: &Tolerance) -> Result<(), KernelError> {
    if dimension.is_finite() && dimension > tol.linear {
        Ok(())
    } else {
        Err(KernelError::InvalidDimension(dimension))
    }
}

/// Quader mit einer Ecke im Ursprung, ausgedehnt in +x, +y, +z.
pub fn make_box(size: DVec3, tol: &Tolerance) -> Result<Solid, KernelError> {
    check(size.x, tol)?;
    check(size.y, tol)?;
    check(size.z, tol)?;
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(size.x, size.y)),
        holes: vec![],
    };
    extrude(&profile, &Frame::WORLD, size.z, tol)
}

/// Zylinder um die z-Achse, Boden im Ursprung.
pub fn make_cylinder(radius: f64, height: f64, tol: &Tolerance) -> Result<Solid, KernelError> {
    check(radius, tol)?;
    check(height, tol)?;
    let profile = Profile {
        outer: ProfileLoop::circle(DVec2::ZERO, radius),
        holes: vec![],
    };
    extrude(&profile, &Frame::WORLD, height, tol)
}

/// Kugel um den Ursprung (Pole auf der z-Achse).
pub fn make_sphere(radius: f64, tol: &Tolerance) -> Result<Solid, KernelError> {
    check(radius, tol)?;
    let chain = ProfileLoop {
        start: DVec2::new(0.0, -radius),
        segments: vec![Segment::Arc {
            center: DVec2::ZERO,
            sweep: PI,
        }],
    };
    revolve(&chain, &Frame::WORLD, tol)
}

/// Kegel um die z-Achse: Grundkreis (Radius `radius`) im Ursprung, Spitze bei `z = height`.
pub fn make_cone(radius: f64, height: f64, tol: &Tolerance) -> Result<Solid, KernelError> {
    check(radius, tol)?;
    check(height, tol)?;
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(radius, 0.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, height),
            },
        ],
    };
    revolve(&chain, &Frame::WORLD, tol)
}
