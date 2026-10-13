//! Tessellierung: Körper (B-Rep) → Dreiecksnetz, plus Masseneigenschaften und Netzprüfung.

mod check;
mod discretize;
mod mass;
mod mesh;
mod tessellate;
mod trimmed;
mod volume;

pub use check::{check_face_boundaries, is_watertight};
pub use discretize::{Discretizer, MAX_NURBS_DIVISIONS};
pub use mass::{MassProperties, mass_properties};
pub use mesh::{EdgeLine, FaceRange, Mesh, TessellationOptions};
pub use tessellate::{MeshError, tessellate, tessellate_with};
pub use volume::{blend_volume_plausible, boolean_volume_plausible, curved_area};
