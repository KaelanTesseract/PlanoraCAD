//! 2D-Skizzen mit Constraint-Solver für PlanoraCAD.

pub mod constraint;
pub mod curves;
pub mod dual;
pub mod error;
pub mod helpers;
pub mod linalg;
pub mod model;
pub mod profile;
pub mod solver;

pub use constraint::{ConstraintData, ConstraintId, ConstraintKind, Evaluation, LineOrPoints};
pub use error::SketchError;
pub use helpers::{RectIds, plane_of_face};
pub use model::{CurveData, CurveId, CurveKind, PointData, PointId, Sketch};
pub use profile::{ExtractedProfiles, RejectedLoop};
pub use solver::{Drag, Soft, SolveReport, SolveStatus};
