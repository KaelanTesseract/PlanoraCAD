//! Schriften lesen und setzen für Skizzentext (Skizzen 8).
//!
//! Das Crate kennt weder egui noch die Skizze. Es liefert die Schriftliste
//! ([`FontLibrary`]) und die Wahl des passenden Schnitts ([`FaceChoice`]).

mod face;
mod library;

pub use face::{FaceChoice, FaceRef};
pub use library::{FontLibrary, system_font_dirs};

/// Ersatzschrift, wenn die gewünschte Familie fehlt (mitgeliefert, OFL 1.1).
pub const FALLBACK_FAMILY: &str = "Montserrat";
