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
    build_scene_marked,
};
pub use viewcube::{
    CubeFace, CubePiece, CubeRegion, INSET, ISO_VIEW, REGION_COUNT, ViewFace, all_regions,
    cube_faces, cube_hit_test, cube_pieces, current_region, face_text_frame, pieces_hit_test,
};
