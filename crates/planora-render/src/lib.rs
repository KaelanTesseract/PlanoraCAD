//! Darstellung und Auswahl: Kamera, Picking, Szenendaten. GPU-Anbindung in `gpu`.

mod camera;
pub mod gpu;
mod grid;
mod pick;
mod scene;
mod viewcube;

pub use camera::OrbitCamera;
pub use grid::{grid_lines, grid_radius, grid_step};
pub use pick::{EdgeHit, FaceHit, pick_edge, pick_face};
pub use scene::{
    EdgeRef, FaceRef, FaceVertex, Highlight, LineVertex, Palette, Pick, SceneData, build_scene,
    build_scene_marked,
};
pub use viewcube::{
    CubePiece, CubeRegion, INSET, ISO_VIEW, ViewFace, all_regions, cube_pieces_visible,
    current_region, face_text_frame, pieces_hit_test,
};
