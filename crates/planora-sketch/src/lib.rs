//! 2D-Skizzen mit Constraint-Solver für PlanoraCAD.

pub mod constraint;
pub(crate) mod curves;
/// Interne Hilfe des Solvers (Dual-Zahlen); nur für die Tests öffentlich.
#[doc(hidden)]
pub mod dual;
pub mod error;
pub mod helpers;
/// Interne lineare Algebra des Solvers; nur für die Tests öffentlich.
#[doc(hidden)]
pub mod linalg;
pub mod model;
pub mod profile;
pub mod query;
pub mod solid;
pub mod solver;

pub use constraint::{ConstraintData, ConstraintId, ConstraintKind, Evaluation, LineOrPoints};
pub use error::SketchError;
pub use helpers::{RectIds, plane_of_face};
pub use model::{CurveData, CurveId, CurveKind, PointData, PointId, Sketch};
pub use profile::{ExtractedProfiles, RejectedLoop};
pub use solid::{extrude_profile, revolve_profile};
pub use solver::{Drag, SolveReport, SolveStatus};
