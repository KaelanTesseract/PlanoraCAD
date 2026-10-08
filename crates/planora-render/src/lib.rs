//! Darstellung und Auswahl: Kamera, Picking, Szenendaten. GPU-Anbindung in `gpu`.

mod camera;
pub mod gpu;
mod pick;
mod scene;
mod viewcube;

pub use camera::OrbitCamera;
pub use pick::{EdgeHit, FaceHit, pick_edge, pick_face};
pub use scene::{
    EdgeRef, FaceRef, FaceVertex, Highlight, LineVertex, Palette, Pick, SceneData, build_scene,
};
pub use viewcube::{CubeFace, ISO_VIEW, ViewFace, cube_faces, cube_hit_test};
