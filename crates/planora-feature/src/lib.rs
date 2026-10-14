//! Feature-Baum von PlanoraCAD: Datenmodell, stabile Verweise auf Flächen und Kanten,
//! Berechnung und Neuberechnung mit Zwischenspeicher, Änderungsoperationen und Netz-Körper.

mod compute;
pub mod error;
mod meshes;
pub mod model;
mod ops;
pub mod recompute;
pub mod refs;
mod snapshot;

pub use compute::MAX_COORD_MM;
pub use error::FeatureError;
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
