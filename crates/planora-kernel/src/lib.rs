//! Geometrie-Kernel von PlanoraCAD (B-Rep mit analytischen Flächen).

pub mod error;
pub mod extrude;
pub mod geometry;
pub mod ids;
pub mod kernel;
pub mod primitives;
pub mod profile;
pub mod revolve;
pub mod topology;
pub mod validate;

pub use error::KernelError;
pub use extrude::extrude;
pub use geometry::{Curve, Surface, UvRect};
pub use ids::{Arena, Id};
pub use kernel::{AnalyticKernel, Kernel};
pub use profile::{Profile, ProfileLoop, Segment, segment_end};
pub use revolve::revolve;
pub use topology::{Edge, Face, HalfEdge, Loop, Shell, Solid, Vertex};
pub use validate::{ValidationError, ValidationReport, validate};
