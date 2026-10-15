//! Feature-Baum von PlanoraCAD: Datenmodell, stabile Verweise auf Flächen und Kanten,
//! Berechnung und Neuberechnung mit Zwischenspeicher, Änderungsoperationen und Netz-Körper.

mod compute;
mod dimensions;
mod drawings;
pub mod error;
pub mod georef;
mod meshes;
pub mod model;
mod ops;
pub mod recompute;
pub mod refs;
mod snapshot;

pub use compute::MAX_COORD_MM;
pub use dimensions::{MAX_DECIMALS, MAX_DIM_TEXT_CHARS, MAX_FIT_CHARS, MAX_TOL_MM};
pub use drawings::{MAX_DRAWINGS, MAX_NAME_CHARS, MAX_POS_MM, MAX_VIEWS, validate_drawing};
pub use error::FeatureError;
pub use georef::{
    from_edge_ref, from_face_ref, geo_ref_for_edge, geo_ref_for_face, resolve_geo, to_edge_ref,
    to_face_ref,
};
pub use meshes::VisibleMesh;
pub use model::{
    BlendArt, BodyRef, Document, EdgeCopy, Feature, FeatureId, FeatureKind, FeatureStatus,
    MAX_MESH_SCALE, MESH_LOCKED, MIN_MESH_SCALE, MeshId, ROTATION_NORM_TOL, SketchFeature,
    SketchPlane, StandardPlane,
};
pub use recompute::{RecomputeReport, VisibleBody};
pub use refs::{
    EdgeRef, FaceRef, ResolvedEdge, ResolvedFace, first_face, resolve_edge, resolve_face,
};
pub use snapshot::{Snapshot, SnapshotFeature};
