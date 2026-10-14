//! Austausch von Dreiecksnetzen (Teilprojekt 9): STL, OBJ, 3MF und glTF schreiben, STL und OBJ
//! lesen, Netze prüfen. Keine neue Abhängigkeit: ZIP und CRC-32 sind selbst geschrieben,
//! Vielecke zerlegt das schon vorhandene `earcutr`.

mod check;
pub mod error;
mod gltf;
pub mod limits;
mod mesh;
mod normals;
mod obj;
mod progress;
mod source;
mod stl;
mod text;
mod threemf;
mod zip;

pub use check::{MeshReport, check_mesh, weld_exact};
pub use error::ExchangeError;
pub use gltf::{GLTF_SCALE, write_glb};
pub use limits::{
    CHECK_EVERY, MAX_FILE_BYTES, MAX_LINE_BYTES, MAX_TRIANGLES, MAX_VERTICES, READ_BUFFER,
    RESERVE_STEP,
};
pub use mesh::{ExchangeMesh, MeshTransform};
pub use normals::{SMOOTH_ANGLE_DEG, Shaded, shade};
pub use obj::{read_obj, write_obj};
pub use progress::Progress;
pub use source::{ExportBody, PlacedSource, SolidSource, TriSource, collect_body};
pub use stl::{STL_HEADER_BYTES, STL_TRIANGLE_BYTES, read_stl, write_stl};
pub use threemf::write_3mf;
pub use zip::{Content, Crc32, ZipWriter, crc32};
