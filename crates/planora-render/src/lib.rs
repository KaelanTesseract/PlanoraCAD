//! Darstellung und Auswahl: Kamera, Picking, Szenendaten. GPU-Anbindung in `gpu`.

mod camera;
mod canvas;
pub mod gpu;
mod grid;
mod pick;
mod scene;
mod stroke;
mod viewcube;

pub use camera::OrbitCamera;
pub use canvas::{CanvasDraw, CanvasTexture, CanvasVertex, canvas_hit, quad_vertices};
pub use grid::{
    axis_half_length, axis_lines, exact_step, grid_lines, grid_radius, grid_step, line_weight,
};
pub use pick::{EdgeHit, FaceHit, PlaneRect, Ray, pick_edge, pick_face, pick_plane, pick_vertex};
pub use scene::{
    EdgeRef, FaceRef, FaceVertex, Highlight, LineVertex, Palette, Pick, SceneData, build_scene,
    build_scene_marked,
};
pub use stroke::{
    FACE_SLOPE_SCALE, STROKE_DEPTH_BIAS, StrokeVertex, StrokeView, push_fill, push_stroke,
};
pub use viewcube::{
    CubePiece, CubeRegion, INSET, ISO_VIEW, ViewFace, all_regions, cube_pieces_visible,
    current_region, face_text_frame, pieces_hit_test,
};
