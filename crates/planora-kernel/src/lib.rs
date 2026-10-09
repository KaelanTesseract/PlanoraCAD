//! Geometrie-Kernel von PlanoraCAD (B-Rep mit analytischen Flächen).

pub mod boolean;
pub mod error;
pub mod extrude;
pub mod geometry;
pub mod ids;
pub mod kernel;
pub mod primitives;
pub mod profile;
pub mod revolve;
pub mod topology;
mod transform;
pub mod validate;

pub use boolean::{BoolOp, BooleanError};
pub use error::KernelError;
pub use extrude::extrude;
pub use geometry::{Curve, CurveKind, Surface, SurfaceKind, UvRect};
pub use ids::{Arena, Id};
pub use kernel::{AnalyticKernel, Kernel};
pub use profile::{MAX_SAMPLES_PER_ARC, Profile, ProfileLoop, Segment, segment_end};
pub use revolve::revolve;
pub use topology::{Edge, Face, HalfEdge, Loop, Shell, Solid, Vertex};
pub use validate::{ValidationError, ValidationReport, validate};
