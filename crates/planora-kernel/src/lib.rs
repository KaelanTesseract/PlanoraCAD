//! Geometrie-Kernel von PlanoraCAD (B-Rep mit analytischen Flächen).

pub mod error;
pub mod geometry;
pub mod ids;
pub mod topology;

pub use error::KernelError;
pub use geometry::{Curve, Surface, UvRect};
pub use ids::{Arena, Id};
pub use topology::{Edge, Face, HalfEdge, Loop, Shell, Solid, Vertex};
