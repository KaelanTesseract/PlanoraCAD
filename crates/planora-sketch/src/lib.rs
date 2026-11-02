//! 2D-Skizzen mit Constraint-Solver für PlanoraCAD.

pub(crate) mod arrangement;
pub mod constraint;
pub(crate) mod curves;
pub mod data;
/// Interne Hilfe des Solvers (Dual-Zahlen); nur für die Tests öffentlich.
#[doc(hidden)]
pub mod dual;
pub(crate) mod edit;
pub mod error;
pub mod fillet;
pub(crate) mod geom;
pub mod helpers;
pub(crate) mod intersect;
/// Interne lineare Algebra des Solvers; nur für die Tests öffentlich.
#[doc(hidden)]
pub mod linalg;
pub mod model;
pub mod profile;
pub(crate) mod profile_legacy;
pub(crate) mod profile_ref;
pub mod query;
pub mod shapes;
pub mod snap;
pub mod solid;
pub mod solver;
pub mod tangent_circles;
pub mod triangulate;

pub use constraint::{ConstraintData, ConstraintId, ConstraintKind, Evaluation, LineOrPoints};
pub use data::{ConstraintRecord, CurveRecord, PointRecord, SketchData};
pub use edit::TrimPiece;
pub use error::SketchError;
pub use fillet::{
    ChamferIds, ChamferKind, ChamferPlan, CurvePick, DimensionChange, EndMove, FilletIds,
    FilletPlan,
};
pub use helpers::{RectIds, plane_of_face};
pub use model::{CurveData, CurveId, CurveKind, LineStyle, PointData, PointId, Sketch};
pub use profile::{ExtractedProfiles, PieceRef, ProfileRef, RejectedLoop};
pub use profile_ref::LegacyMigration;
pub use shapes::{ArcSlotIds, CenterRectIds, PolygonIds, SlotIds, SlotStyle};
pub use snap::{AlignHit, Snap, SnapHit, SnapIndex};
pub use solid::{extrude_profile, revolve_profile};
pub use solver::{Drag, SolveReport, SolveStatus};
