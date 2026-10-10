//! Dateiformat, Projektverwaltung und Versionsspeicher von PlanoraCAD (ohne Oberfläche).
pub mod convert;
pub mod error;
pub mod format;

pub use convert::{FORMAT_VERSION, decode_version, encode_version};
pub use error::{ProjectError, Result};
