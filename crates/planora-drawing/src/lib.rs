//! Technische Zeichnungen (Teilprojekt 11): Ansichten exakter Körper auf einem Blatt.
//! Dieses Paket enthält das Datenmodell, die Ansichtsbasis und das Blattlayout.

pub mod basis;
pub mod error;
pub mod isect;
pub mod layout;
pub mod limits;
pub mod model;
pub mod silhouette;
pub mod stroke;

pub use basis::Basis;
pub use error::DrawError;
pub use layout::{GAP_MM, align_dir, view_extent};
pub use model::{
    Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind, ViewSpec,
};

/// Ein Körper der Zeichnung: Name (für Meldungen) und der exakte Körper.
#[derive(Clone, Debug)]
pub struct DrawBody<'a> {
    pub name: String,
    pub solid: &'a planora_kernel::Solid,
}
