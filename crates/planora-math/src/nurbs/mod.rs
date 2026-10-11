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
/// Höchstzahl Bézier-Flicken bei der Zerlegung einer Fläche (begrenzt Speicher und Rechenzeit;
/// bei Grad 9 × 9 etwa 65 536 · 100 Kontrollpunkte).
pub const MAX_BEZIER_PATCHES: usize = 65_536;
/// Höchstzahl Kontrollpunkte einer Fläche insgesamt.
pub const MAX_CONTROL_TOTAL: usize = 262_144;
/// Größter erlaubter Koordinatenbetrag in Millimetern.
pub const MAX_COORD: f64 = 1e9;
/// Kleinstes erlaubtes Gewicht (muss echt positiv sein).
pub const MIN_WEIGHT: f64 = 1e-6;
/// Größtes erlaubtes Gewicht.
pub const MAX_WEIGHT: f64 = 1e6;
/// Größter erlaubter Betrag eines Knotens und größte Länge des Parameterbereichs
/// (verhindert Überlauf bei Knotendifferenzen).
pub const MAX_KNOT_MAGNITUDE: f64 = 1e9;
/// Kleinster erlaubter Abstand zweier verschiedener Knoten, bezogen auf die Länge des
/// Parameterbereichs (verhindert unendliche Ableitungen und beim Spiegeln verschmelzende Knoten).
pub const MIN_KNOT_GAP_RATIO: f64 = 1e-9;
/// Kleinste erlaubte Länge des Parameterbereichs (mit `MIN_KNOT_GAP_RATIO` ergibt das absolute
/// Knotenabstände von mindestens 1e-15; Auswertung und Ableitungen bleiben endlich).
pub const MIN_KNOT_DOMAIN: f64 = 1e-6;
/// Höchste Ableitungsordnung, die Kurven und Basisfunktionen liefern (Ordnungen darüber
/// werden darauf begrenzt; Ableitungen über dem Grad sind ohnehin 0).
pub const MAX_DERIV_ORDER: usize = MAX_DEGREE;
