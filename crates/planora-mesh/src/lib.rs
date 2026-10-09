//! Tessellierung: Körper (B-Rep) → Dreiecksnetz, plus Masseneigenschaften und Netzprüfung.

mod check;
mod discretize;
mod mass;
mod mesh;
mod tessellate;
mod trimmed;
mod volume;

pub use check::is_watertight;
pub use mass::{MassProperties, mass_properties};
pub use mesh::{EdgeLine, FaceRange, Mesh, TessellationOptions};
pub use tessellate::{MeshError, tessellate};
pub use volume::boolean_volume_plausible;
