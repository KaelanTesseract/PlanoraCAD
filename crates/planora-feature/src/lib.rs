//! Feature-Baum von PlanoraCAD: Datenmodell und stabile Verweise auf Flächen und Kanten.

pub mod error;
pub mod model;
pub mod refs;

pub use error::FeatureError;
pub use model::{
    BodyRef, Document, EdgeCopy, Feature, FeatureId, FeatureKind, FeatureStatus, SketchFeature,
    SketchPlane, StandardPlane,
};
pub use refs::{EdgeRef, FaceRef, ResolvedEdge, ResolvedFace, resolve_edge, resolve_face};
