use crate::boolean::{BoolOp, BooleanError};
use crate::error::KernelError;
use crate::extrude::extrude;
use crate::geometry::{CurveKind, SurfaceKind};
use crate::ids::Id;
use crate::primitives;
use crate::profile::{Profile, ProfileLoop};
use crate::revolve::revolve;
use crate::topology::{Edge, Face, Solid};
use crate::validate::{ValidationError, ValidationReport, validate};
use planora_math::{DMat4, DVec3, Frame, Tolerance};

/// Die kleine Schnittstelle, über die Oberfläche und andere Crates mit dem Kernel sprechen.
pub trait Kernel {
    fn make_box(&self, size: DVec3) -> Result<Solid, KernelError>;
    fn make_cylinder(&self, radius: f64, height: f64) -> Result<Solid, KernelError>;
    fn extrude(&self, profile: &Profile, frame: &Frame, height: f64) -> Result<Solid, KernelError>;
    fn make_sphere(&self, radius: f64) -> Result<Solid, KernelError>;
    fn make_cone(&self, radius: f64, height: f64) -> Result<Solid, KernelError>;
    fn revolve(&self, chain: &ProfileLoop, frame: &Frame) -> Result<Solid, KernelError>;
    fn transform(&self, solid: &Solid, matrix: &DMat4) -> Result<Solid, KernelError>;
    fn validate(&self, solid: &Solid) -> Result<ValidationReport, ValidationError>;
    fn boolean(&self, a: &Solid, b: &Solid, op: BoolOp) -> Result<Solid, BooleanError>;
    /// Art der Trägerfläche einer Fläche; `None` für eine Id außerhalb des Körpers.
    fn face_kind(&self, solid: &Solid, face: Id<Face>) -> Option<SurfaceKind>;
    /// Art der Trägerkurve einer Kante; `None` für eine Id außerhalb des Körpers.
    fn edge_kind(&self, solid: &Solid, edge: Id<Edge>) -> Option<CurveKind>;
    /// Anzahl der Flächen und Kanten.
    fn counts(&self, solid: &Solid) -> (usize, usize);
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

    fn transform(&self, solid: &Solid, matrix: &DMat4) -> Result<Solid, KernelError> {
        solid.transformed(matrix)
    }

    fn validate(&self, solid: &Solid) -> Result<ValidationReport, ValidationError> {
        validate(solid)
    }

    fn boolean(&self, a: &Solid, b: &Solid, op: BoolOp) -> Result<Solid, BooleanError> {
        crate::boolean::boolean(a, b, op, &self.tolerance)
    }

    fn face_kind(&self, solid: &Solid, face: Id<Face>) -> Option<SurfaceKind> {
        solid.faces.get(face).map(|f| f.surface.kind())
    }

    fn edge_kind(&self, solid: &Solid, edge: Id<Edge>) -> Option<CurveKind> {
        solid.edges.get(edge).map(|e| e.curve.kind())
    }

    fn counts(&self, solid: &Solid) -> (usize, usize) {
        (solid.faces.len(), solid.edges.len())
    }
}
