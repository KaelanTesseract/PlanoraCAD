//! 2D-Skizzen mit Constraint-Solver für PlanoraCAD.

pub mod constraint;
pub mod dual;
pub mod error;
pub mod linalg;
pub mod model;
pub mod solver;

pub use constraint::{ConstraintData, ConstraintId, ConstraintKind, Evaluation, LineOrPoints};
pub use error::SketchError;
pub use model::{CurveData, CurveId, CurveKind, PointData, PointId, Sketch};
pub use solver::{Drag, Soft, SolveReport, SolveStatus};
