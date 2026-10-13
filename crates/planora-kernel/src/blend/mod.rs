//! Rundungen und Fasen (Teilprojekt 8): rollende Kugel, analytische Rundungsflächen, Umbau der
//! Nachbarflächen über `sew` (Spec `2026-10-13-rundungen-und-fasen-design.md`).

pub mod error;
pub mod topo;

pub use error::BlendError;
