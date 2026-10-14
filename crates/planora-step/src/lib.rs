//! STEP-Export (Teilprojekt 10): exakte Körper des Kernels als STEP-Datei (AP214,
//! `AUTOMOTIVE_DESIGN`). Das Textformat ist von Hand geschrieben, ohne neue Abhängigkeit.

pub mod entity;
pub mod error;
pub mod number;
pub mod text;

pub use entity::{Entities, MAX_ENTITIES, WRAP_COLUMN, logical, refs};
pub use error::StepError;
pub use number::{push_real, real};
pub use text::{MAX_NAME_CHARS, StepText, step_string};
