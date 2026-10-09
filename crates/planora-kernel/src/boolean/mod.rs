//! Boolesche Operationen (Vereinigen, Abziehen, Schneiden) für Körper mit analytischen Flächen.

mod error;
mod roots;
mod seg;
mod surfsurf;

pub use error::{BoolOp, BooleanError};
pub use roots::{CurveSurface, Root, curve_surface_roots, seg_intersections};
pub use seg::{Seg, reverse_loop};
pub use surfsurf::{SurfaceIntersection, intersect_surfaces, surface_name};
