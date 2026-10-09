//! 2D-Skizzen mit Constraint-Solver für PlanoraCAD.

pub mod constraint;
pub mod dual;
pub mod error;
pub mod linalg;
pub mod model;

pub use constraint::{ConstraintData, ConstraintId, ConstraintKind, Evaluation, LineOrPoints};
pub use error::SketchError;
pub use model::{CurveData, CurveId, CurveKind, PointData, PointId, Sketch};
