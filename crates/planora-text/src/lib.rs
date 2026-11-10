//! Schriften lesen und setzen für Skizzentext (Skizzen 8).
//!
//! Das Crate kennt kein egui. Es liefert die Schriftliste ([`FontLibrary`]), die Wahl des
//! passenden Schnitts ([`FaceChoice`]) und setzt Text: `FontLibrary` setzt
//! `planora_sketch::text_shape::TextShaper` um und liefert ein `TextShape`. Von der Skizze kennt
//! es nur diese reinen Datentypen (Abweichung von Spec A10, siehe `offene-punkte.md`).

mod face;
mod library;
mod outline;
mod shape;
mod synth;

pub use face::{FaceChoice, FaceRef};
pub use library::{FontLibrary, SkipReason, system_font_dirs};

/// Ersatzschrift, wenn die gewünschte Familie fehlt (mitgeliefert, OFL 1.1).
pub const FALLBACK_FAMILY: &str = "Montserrat";
