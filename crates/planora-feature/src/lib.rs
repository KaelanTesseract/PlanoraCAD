//! Feature-Baum von PlanoraCAD: Datenmodell, stabile Verweise auf Flächen und Kanten,
//! Berechnung und Neuberechnung mit Zwischenspeicher sowie Änderungsoperationen.

mod compute;
pub mod error;
pub mod model;
mod ops;
pub mod recompute;
pub mod refs;
mod snapshot;

pub use compute::MAX_COORD_MM;
pub use error::FeatureError;
pub use model::{
    BodyRef, Document, EdgeCopy, Feature, FeatureId, FeatureKind, FeatureStatus, SketchFeature,
    SketchPlane, StandardPlane,
};
pub use recompute::{RecomputeReport, VisibleBody};
pub use refs::{EdgeRef, FaceRef, ResolvedEdge, ResolvedFace, resolve_edge, resolve_face};
pub use snapshot::{Snapshot, SnapshotFeature};
