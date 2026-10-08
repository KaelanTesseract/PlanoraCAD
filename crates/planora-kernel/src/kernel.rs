use crate::error::KernelError;
use crate::extrude::extrude;
use crate::primitives;
use crate::profile::{Profile, ProfileLoop};
use crate::revolve::revolve;
use crate::topology::Solid;
use crate::validate::{ValidationError, ValidationReport, validate};
use planora_math::{DVec3, Frame, Tolerance};

/// Die kleine Schnittstelle, über die Oberfläche und andere Crates mit dem Kernel sprechen.
pub trait Kernel {
    fn make_box(&self, size: DVec3) -> Result<Solid, KernelError>;
    fn make_cylinder(&self, radius: f64, height: f64) -> Result<Solid, KernelError>;
    fn extrude(&self, profile: &Profile, frame: &Frame, height: f64) -> Result<Solid, KernelError>;
    fn make_sphere(&self, radius: f64) -> Result<Solid, KernelError>;
    fn make_cone(&self, radius: f64, height: f64) -> Result<Solid, KernelError>;
    fn revolve(&self, chain: &ProfileLoop, frame: &Frame) -> Result<Solid, KernelError>;
    fn validate(&self, solid: &Solid) -> Result<ValidationReport, ValidationError>;
}

/// Kernel mit analytischen Flächen (Ebene, Zylinder, Kegel, Kugel, Torus).
#[derive(Clone, Copy, Debug, Default)]
pub struct AnalyticKernel {
    pub tolerance: Tolerance,
}

impl Kernel for AnalyticKernel {
    fn make_box(&self, size: DVec3) -> Result<Solid, KernelError> {
        primitives::make_box(size, &self.tolerance)
    }

    fn make_cylinder(&self, radius: f64, height: f64) -> Result<Solid, KernelError> {
        primitives::make_cylinder(radius, height, &self.tolerance)
    }

    fn extrude(&self, profile: &Profile, frame: &Frame, height: f64) -> Result<Solid, KernelError> {
        extrude(profile, frame, height, &self.tolerance)
    }

    fn make_sphere(&self, radius: f64) -> Result<Solid, KernelError> {
        primitives::make_sphere(radius, &self.tolerance)
    }

    fn make_cone(&self, radius: f64, height: f64) -> Result<Solid, KernelError> {
        primitives::make_cone(radius, height, &self.tolerance)
    }

    fn revolve(&self, chain: &ProfileLoop, frame: &Frame) -> Result<Solid, KernelError> {
        revolve(chain, frame, &self.tolerance)
    }

    fn validate(&self, solid: &Solid) -> Result<ValidationReport, ValidationError> {
        validate(solid)
    }
}
