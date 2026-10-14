//! Grundlegende Mathematik für PlanoraCAD: Vektoren, Koordinatensysteme, Toleranzen.

mod frame;
pub mod nurbs;
mod predicates;
mod radius;
mod tolerance;

pub use frame::Frame;
pub use glam::{DMat3, DMat4, DQuat, DVec2, DVec3, EulerRot};
pub use predicates::orient2d;
pub use radius::{RadiusLaw, RadiusLawError};
pub use std::f64::consts::{PI, TAU};
pub use tolerance::Tolerance;
