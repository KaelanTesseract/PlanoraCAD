//! Geometrie-Kernel von PlanoraCAD (B-Rep mit analytischen Flächen und Freiformflächen).

pub mod boolean;
pub mod error;
pub mod extrude;
pub mod geometry;
pub mod ids;
pub mod kernel;
pub mod nurbs_surface;
pub mod primitives;
pub mod profile;
pub mod revolve;
pub mod topology;
mod transform;
pub mod validate;

pub use boolean::{BoolOp, BooleanError, edge_probe_point, face_probe_point};
pub use error::KernelError;
pub use extrude::extrude;
pub use geometry::{Curve, CurveKind, Surface, SurfaceKind, UvRect, nurbs_divisions};
pub use ids::{Arena, Id};
pub use kernel::{AnalyticKernel, Kernel};
pub use nurbs_surface::NurbsSurfaceData;
pub use profile::{MAX_SAMPLES_PER_ARC, Profile, ProfileLoop, SampledLoop, Segment, segment_end};
pub use revolve::revolve;
pub use topology::{Edge, Face, FaceTag, HalfEdge, Loop, Shell, Solid, Vertex};
pub use validate::{
    NURBS_EDGE_TOL, ValidationError, ValidationReport, edge_surface_tol, point_surface_tol,
    validate, validate_with,
};
