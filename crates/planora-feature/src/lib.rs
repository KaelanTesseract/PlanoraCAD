//! Feature-Baum von PlanoraCAD: Datenmodell, stabile Verweise auf Flächen und Kanten,
//! Berechnung und Neuberechnung mit Zwischenspeicher, Änderungsoperationen, Netz-Körper und STEP-Körper.

mod compute;
mod dimensions;
mod drawings;
pub mod error;
pub mod expr;
pub mod formulas;
pub mod georef;
mod meshes;
pub mod model;
mod ops;
pub mod params;
mod parts;
mod parts_sheets;
pub mod recompute;
pub mod refs;
mod snapshot;
mod step_bodies;

pub use compute::MAX_COORD_MM;
pub use dimensions::{
    MAX_DECIMALS, MAX_DIM_TEXT_CHARS, MAX_FIT_CHARS, MAX_HINT_MM, MAX_ROLE_CHARS, MAX_TOL_MM,
    resolve_drawing_dimensions, resolve_drawing_dimensions_keyed, resolve_drawing_dimensions_views,
};
pub use drawings::{
    MAX_DRAWINGS, MAX_NAME_CHARS, MAX_POS_MM, MAX_VIEWS, TOO_MANY_BALLOONS, validate_drawing,
};
pub use error::FeatureError;
pub use expr::{
    MAX_COMMENT_CHARS, MAX_FORMULA_CHARS, MAX_FORMULA_DEPTH, MAX_PARAM_NAME_CHARS, MAX_PARAMETERS,
    ParamUnit,
};
pub use formulas::{
    FieldKey, Formulas, constraint_unit, field_keys, field_label, field_unit, field_value,
    set_field_value,
};
pub use georef::{
    from_edge_ref, from_face_ref, geo_ref_for_edge, geo_ref_for_edge_cached, geo_ref_for_face,
    geo_ref_for_face_cached, resolve_geo, resolve_geo_cached, to_edge_ref, to_face_ref,
};
pub use meshes::VisibleMesh;
pub use model::{
    BlendArt, BodyRef, Document, EdgeCopy, Feature, FeatureId, FeatureKind, FeatureStatus,
    MAX_CHAMFER_ANGLE_DEG, MAX_MESH_SCALE, MESH_LOCKED, MIN_CHAMFER_ANGLE_DEG, MIN_MESH_SCALE,
    MeshId, ROTATION_NORM_TOL, SketchFeature, SketchFormulas, SketchPlane, StandardPlane,
};
pub use params::{ParamEdit, ParamId, ParamScope, Parameter};
pub use parts_sheets::{
    FOLLOW_PAGE_DELETE, PARTS_NAME_TAKEN, PARTS_PAGE_NAME_TAKEN, PARTS_SHEET_EXISTS,
    PARTS_SHEET_RENAME,
};
pub use recompute::{RecomputeReport, VisibleBody};
pub use refs::{
    EdgeRef, FaceRef, RefCache, ResolvedEdge, ResolvedFace, face_probe_calls, first_face,
    resolve_edge, resolve_face,
};
pub use snapshot::{RestoreKind, Snapshot, SnapshotFeature, mesh_validations};
pub use step_bodies::{
    MAX_STEP_TEXT_SUM_BYTES, NewStepBody, StepShape, StepShapeId, StepTextSumLimitGuard,
    limit_step_text_sum_for_tests, step_builds, step_text_megabytes, step_text_sum,
    step_text_sum_limit,
};
