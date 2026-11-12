//! 2D-Skizzen mit Constraint-Solver für PlanoraCAD.

pub(crate) mod arrangement;
pub mod constraint;
pub(crate) mod curves;
pub mod data;
pub mod dimension;
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
pub mod offset;
pub mod pattern;
pub mod profile;
pub(crate) mod profile_legacy;
pub(crate) mod profile_ref;
pub mod query;
pub mod shapes;
pub mod snap;
pub mod solid;
pub mod solver;
pub mod spline_handles;
pub mod tangent_circles;
pub mod text;
pub mod text_layout;
pub(crate) mod text_profile;
pub(crate) mod text_refresh;
pub mod text_shape;
pub mod transform;
pub mod triangulate;

pub use constraint::{ConstraintData, ConstraintId, ConstraintKind, Evaluation, LineOrPoints};
pub use curves::{
    conic_apex_ok, conic_nurbs, conic_rho_at, conic_rho_ok, conic_shoulder, control_spline_nurbs,
};
pub use data::{ConstraintRecord, CurveRecord, PatternRecord, PointRecord, SketchData, TextRecord};
pub use dimension::{DimMode, DimPick, DimPlan};
pub use edit::TrimPiece;
pub use error::SketchError;
pub use fillet::{
    ChamferIds, ChamferKind, ChamferPlan, CurvePick, DimensionChange, EndMove, FilletIds,
    FilletPlan,
};
pub use helpers::{RectIds, plane_of_face};
pub use model::{
    CurveData, CurveId, CurveKind, HandleCurvature, LineStyle, PointData, PointId, Sketch,
    SplineHandle,
};
pub use pattern::{
    CircularMode, PatternData, PatternDir, PatternId, PatternInstance, PatternKind, PatternPlan,
    PatternProblem, PatternSlot,
};
pub use profile::{ExtractedProfiles, PieceRef, ProfileRef, RejectedLoop};
pub use profile_ref::LegacyMigration;
pub use shapes::{ArcSlotIds, CenterRectIds, PolygonIds, SlotIds, SlotStyle};
pub use snap::{AlignHit, Snap, SnapHit, SnapIndex};
pub use solid::{extrude_profile, revolve_profile};
pub use solver::{Drag, SolveReport, SolveStatus};
pub use spline_handles::{NodeCurvature, NodeHandle, Nodes, hermite_nodes};
pub use text::{
    HAlign, ReshapeReport, ShapedCache, TextData, TextId, TextKindEdit, TextParams, TextPlacement,
    VAlign,
};
pub use transform::{CopyMap, Similarity, TransformBlock, TransformReport, TransformSet};
