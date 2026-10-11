//! NURBS-Kurven und -Flächen: geprüfte Konstruktoren, Basisfunktionen, rationale Auswertung,
//! Knoteneinfügen, Zerlegung in rationale Bézier-Stücke, exakte Konstruktionen (Kreis, Ellipse,
//! Spline, Extrusion, Drehfläche) und Annäherung von Punktfolgen.

mod bezier;
mod build;
mod curve;
mod dense;
mod error;
mod fit;
mod knots;
mod surface;

pub use bezier::{BezierPatch, BezierSegment};
pub use curve::NurbsCurve;
pub use error::NurbsError;
pub use fit::MAX_FIT_CONTROL_POINTS;
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

/// Summe der Knickwinkel (Bogenmaß) eines Streckenzugs: Winkel zwischen aufeinanderfolgenden
/// Strecken; Strecken der Länge 0 (zusammenfallende Punkte) werden übersprungen. Für ein
/// Kontrollpolygon ist das eine obere Schranke der Gesamtkrümmung der Kurve.
pub fn polygon_turning(points: &[crate::DVec3]) -> f64 {
    let mut total = 0.0;
    let mut prev: Option<crate::DVec3> = None;
    for w in points.windows(2) {
        let d = w[1] - w[0];
        let len = d.length();
        if !(len > 0.0 && len.is_finite()) {
            continue;
        }
        let d = d / len;
        if let Some(p) = prev {
            total += p.dot(d).clamp(-1.0, 1.0).acos();
        }
        prev = Some(d);
    }
    total
}

#[cfg(test)]
mod turning_tests {
    use super::*;
    use crate::{DVec3, PI, TAU};

    #[test]
    fn turning_of_a_square_and_degenerate_polygons() {
        let sq = [
            DVec3::ZERO,
            DVec3::X,
            DVec3::new(1.0, 1.0, 0.0),
            DVec3::Y,
            DVec3::ZERO,
        ];
        assert!((polygon_turning(&sq) - 1.5 * PI).abs() < 1e-12);
        let line = [DVec3::ZERO, DVec3::X, DVec3::X, DVec3::new(2.0, 0.0, 0.0)];
        assert_eq!(polygon_turning(&line), 0.0);
        assert_eq!(polygon_turning(&[]), 0.0);
        let c = NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X, DVec3::Y, 0.0, TAU).unwrap();
        // Kontrollpolygon des Vollkreises: Quadrat, vier Ecken zu je 90°, Seitenmitten 0°.
        assert!((polygon_turning(c.control_points()) - TAU).abs() < 1e-9);
    }

    #[test]
    fn spans_in_counts_overlapping_bezier_pieces() {
        let k =
            KnotVector::new(2, vec![0.0, 0.0, 0.0, 0.25, 0.5, 0.5, 0.75, 1.0, 1.0, 1.0]).unwrap();
        assert_eq!(k.spans_in(0.0, 1.0), 4);
        assert_eq!(k.spans_in(1.0, 0.0), 4);
        assert_eq!(k.spans_in(0.1, 0.3), 2);
        assert_eq!(k.spans_in(0.25, 0.5), 1);
        assert_eq!(k.spans_in(0.3, 0.3), 0);
        assert_eq!(k.spans_in(f64::NAN, 0.3), 4);
    }
}
