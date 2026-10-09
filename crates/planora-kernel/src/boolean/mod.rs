//! Boolesche Operationen (Vereinigen, Abziehen, Schneiden) für Körper mit analytischen Flächen.

mod classify;
mod error;
mod imprint;
mod roots;
mod seg;
mod split;
mod surfsurf;
mod uv;

pub use classify::{Class, Location, classify_region, solid_contains, solid_faces};
pub use error::{BoolOp, BooleanError};
pub use imprint::{FaceWork, imprint};
pub use roots::{CurveSurface, Root, curve_surface_roots, seg_intersections};
pub use seg::{Seg, reverse_loop};
pub use split::{Region, split_face};
pub use surfsurf::{SurfaceIntersection, intersect_surfaces, surface_name};
pub use uv::{
    Containment, FaceGeom, face_geom, loop_points, oriented_area, point_in_face, points_to_uv,
    points_to_uv_indexed, winding,
};
