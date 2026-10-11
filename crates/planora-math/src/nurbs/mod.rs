//! NURBS-Kurven und -Flächen: geprüfte Konstruktoren, Basisfunktionen, rationale Auswertung,
//! Knoteneinfügen und Zerlegung in rationale Bézier-Stücke.

mod bezier;
mod curve;
mod error;
mod knots;
mod surface;

pub use bezier::{BezierPatch, BezierSegment};
pub use curve::NurbsCurve;
pub use error::NurbsError;
pub use knots::KnotVector;
pub use surface::{MAX_SURFACE_DERIV, NurbsSurface};

/// Größter erlaubter Grad (Schutz vor Rechenaufwand und Zahlenüberlauf).
pub const MAX_DEGREE: usize = 9;
/// Höchstzahl Kontrollpunkte je Richtung.
pub const MAX_CONTROL_POINTS: usize = 4096;
/// Höchstzahl Kontrollpunkte einer Fläche insgesamt.
pub const MAX_CONTROL_TOTAL: usize = 262_144;
/// Größter erlaubter Koordinatenbetrag in Millimetern.
pub const MAX_COORD: f64 = 1e9;
/// Kleinstes erlaubtes Gewicht (muss echt positiv sein).
pub const MIN_WEIGHT: f64 = 1e-6;
/// Größtes erlaubtes Gewicht.
pub const MAX_WEIGHT: f64 = 1e6;
