//! Technische Zeichnungen (Teilprojekt 11): Ansichten exakter Körper auf einem Blatt.
//! Dieses Paket enthält das Datenmodell, die Ansichtsbasis und das Blattlayout.

pub mod basis;
pub mod error;
pub mod layout;
pub mod limits;
pub mod model;

pub use basis::Basis;
pub use error::DrawError;
pub use layout::{GAP_MM, align_dir, view_extent};
pub use model::{
    Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind, ViewSpec,
};
