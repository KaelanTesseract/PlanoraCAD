use crate::error::KernelError;
use crate::extrude::extrude;
use crate::profile::{Profile, ProfileLoop};
use crate::topology::Solid;
use planora_math::{DVec2, DVec3, Frame, Tolerance};

fn check(dimension: f64, tol: &Tolerance) -> Result<(), KernelError> {
    if dimension > tol.linear {
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
