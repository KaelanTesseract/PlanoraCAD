//! Neutrale 2D-Geometrie, die die DXF- und SVG-Leser liefern (Skizzen 9). Die Typen kennen
//! weder Skizzen noch Dateiformate; die Umwandlung in Skizzenelemente geschieht später.

use planora_math::DVec2;
use std::f64::consts::{PI, TAU};

/// Zeicheneinheit einer Datei: alle Längeneinheiten, die `$INSUNITS` in DXF nennen kann (die
/// ersten fünf stehen im Dialog immer zur Wahl, die anderen nur, wenn die Datei sie nennt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit2d {
    Millimeter,
    Centimeter,
    Meter,
    Inch,
    Foot,
    Mile,
    Kilometer,
    Microinch,
    Mil,
    Yard,
    Angstrom,
    Nanometer,
    Micron,
    Decimeter,
    Decameter,
    Hectometer,
    Gigameter,
    AstronomicalUnit,
    LightYear,
    Parsec,
    UsSurveyFoot,
    UsSurveyInch,
    UsSurveyYard,
    UsSurveyMile,
}

impl Unit2d {
    /// Millimeter je Einheit.
    pub fn mm(self) -> f64 {
        match self {
            Unit2d::Millimeter => 1.0,
            Unit2d::Centimeter => 10.0,
            Unit2d::Meter => 1000.0,
            Unit2d::Inch => 25.4,
            Unit2d::Foot => 304.8,
            Unit2d::Mile => 1_609_344.0,
            Unit2d::Kilometer => 1e6,
            Unit2d::Microinch => 2.54e-5,
            Unit2d::Mil => 0.0254,
            Unit2d::Yard => 914.4,
            Unit2d::Angstrom => 1e-7,
            Unit2d::Nanometer => 1e-6,
            Unit2d::Micron => 1e-3,
            Unit2d::Decimeter => 100.0,
            Unit2d::Decameter => 1e4,
            Unit2d::Hectometer => 1e5,
            Unit2d::Gigameter => 1e12,
            Unit2d::AstronomicalUnit => 1.495_978_707e14,
            Unit2d::LightYear => 9.460_730_472_580_8e18,
            Unit2d::Parsec => 3.085_677_581_491_367e19,
            Unit2d::UsSurveyFoot => 1_200_000.0 / 3937.0,
            Unit2d::UsSurveyInch => 100_000.0 / 3937.0,
            Unit2d::UsSurveyYard => 3_600_000.0 / 3937.0,
            Unit2d::UsSurveyMile => 6_336_000_000.0 / 3937.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign2d {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign2d {
    Bottom,
    Middle,
    Top,
}

/// Ein Text mit Schrift, Höhe und Lage.
#[derive(Debug, Clone, PartialEq)]
pub struct Text2d {
    pub text: String,
    pub font: String,
    pub height: f64,
    pub pos: DVec2,
    /// Drehwinkel in Radiant.
    pub angle: f64,
    pub halign: HAlign2d,
    pub valign: VAlign2d,
}

/// Ein gelesenes 2D-Element.
#[derive(Debug, Clone, PartialEq)]
pub enum Element2d {
    Point(DVec2),
    Line(DVec2, DVec2),
    Circle {
        c: DVec2,
        r: f64,
    },
    /// Winkel in Radiant, gegen den Uhrzeigersinn von `start` nach `end`.
    Arc {
        c: DVec2,
        r: f64,
        start: f64,
        end: f64,
    },
    /// `major` ist der Vektor der großen Halbachse, `ratio` das Verhältnis klein zu groß.
    /// Voll: `start = 0`, `end = TAU`.
    Ellipse {
        c: DVec2,
        major: DVec2,
        ratio: f64,
        start: f64,
        end: f64,
    },
    Quad([DVec2; 3]),
    Cubic([DVec2; 4]),
    Nurbs {
        degree: usize,
        knots: Vec<f64>,
        points: Vec<DVec2>,
        weights: Option<Vec<f64>>,
    },
    FitSpline(Vec<DVec2>),
    Text(Text2d),
}

/// Eine Ebene (Layer) mit ihren Elementen.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer2d {
    pub name: String,
    pub elements: Vec<Element2d>,
}

/// Zähler der übersprungenen Inhalte, für den Hinweis nach dem Import.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Skipped {
    pub unknown: usize,
    pub three_d: usize,
    pub dimensions: usize,
    pub hatches: usize,
    pub images: usize,
    pub texts: usize,
    pub degenerate: usize,
}

/// Ergebnis eines DXF- oder SVG-Lesers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Import2d {
    pub layers: Vec<Layer2d>,
    pub units: Option<Unit2d>,
    pub skipped: Skipped,
}

impl Import2d {
    /// Anzahl aller Elemente über alle Layer.
    pub fn element_count(&self) -> usize {
        self.layers.iter().map(|l| l.elements.len()).sum()
    }

    /// Hüllquader (min, max) aller Elemente; `None`, wenn es keine gibt.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let mut b: Option<(DVec2, DVec2)> = None;
        for e in self.layers.iter().flat_map(|l| &l.elements) {
            element_bounds(e, &mut |p| {
                b = Some(match b {
                    None => (p, p),
                    Some((lo, hi)) => (lo.min(p), hi.max(p)),
                });
            });
        }
        b
    }
}

/// Ob der Winkel `a` im gegen den Uhrzeigersinn laufenden Bereich start..end liegt.
fn angle_in_range(a: f64, start: f64, end: f64) -> bool {
    let span = (end - start).rem_euclid(TAU);
    if span == 0.0 && end != start {
        return true; // volle Umdrehung
    }
    (a - start).rem_euclid(TAU) <= span
}

/// Meldet Punkte, die die Hülle des Elements bestimmen.
fn element_bounds(e: &Element2d, f: &mut dyn FnMut(DVec2)) {
    match e {
        Element2d::Point(p) => f(*p),
        Element2d::Line(a, b) => {
            f(*a);
            f(*b);
        }
        Element2d::Circle { c, r } => {
            f(*c - DVec2::splat(*r));
            f(*c + DVec2::splat(*r));
        }
        Element2d::Arc { c, r, start, end } => {
            let at = |a: f64| *c + DVec2::new(a.cos(), a.sin()) * *r;
            f(at(*start));
            f(at(*end));
            for k in 0..4 {
                let a = k as f64 * PI / 2.0;
                if angle_in_range(a, *start, *end) {
                    f(at(a));
                }
            }
        }
        Element2d::Ellipse {
            c,
            major,
            ratio,
            start,
            end,
        } => {
            let minor = DVec2::new(-major.y, major.x) * *ratio;
            let at = |t: f64| *c + *major * t.cos() + minor * t.sin();
            f(at(*start));
            f(at(*end));
            // Extrema der Parameterform: x'(t) = 0 bzw. y'(t) = 0.
            let tx = minor.x.atan2(major.x);
            let ty = minor.y.atan2(major.y);
            for base in [tx, ty] {
                for t in [base, base + PI] {
                    if angle_in_range(t, *start, *end) {
                        f(at(t));
                    }
                }
            }
        }
        // Bézier- und Spline-Kurven liegen in der konvexen Hülle ihrer Kontrollpunkte.
        Element2d::Quad(p) => p.iter().for_each(|q| f(*q)),
        Element2d::Cubic(p) => p.iter().for_each(|q| f(*q)),
        Element2d::Nurbs { points, .. } => points.iter().for_each(|q| f(*q)),
        Element2d::FitSpline(p) => p.iter().for_each(|q| f(*q)),
        // Bei Text zählt für die Hülle nur die Lage `pos`, nicht die Ausdehnung.
        Element2d::Text(t) => f(t.pos),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(e: Element2d) -> Import2d {
        Import2d {
            layers: vec![Layer2d {
                name: "0".into(),
                elements: vec![e],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn bounds_cover_arc_extremes() {
        let imp = one(Element2d::Arc {
            c: DVec2::ZERO,
            r: 2.0,
            start: 0.0,
            end: PI,
        });
        let (lo, hi) = imp.bounds().unwrap();
        assert!((hi.y - 2.0).abs() < 1e-12, "hi.y = {}", hi.y);
        assert!((lo.x + 2.0).abs() < 1e-12 && (hi.x - 2.0).abs() < 1e-12);
        assert!(lo.y.abs() < 1e-12);
    }

    #[test]
    fn bounds_cover_ellipse_extremes() {
        let imp = one(Element2d::Ellipse {
            c: DVec2::ZERO,
            major: DVec2::new(3.0, 0.0),
            ratio: 0.5,
            start: 0.0,
            end: TAU,
        });
        let (lo, hi) = imp.bounds().unwrap();
        assert!((hi.x - 3.0).abs() < 1e-12 && (lo.y + 1.5).abs() < 1e-12);
    }

    #[test]
    fn bounds_cover_rotated_ellipse_extremes() {
        let imp = one(Element2d::Ellipse {
            c: DVec2::ZERO,
            major: DVec2::new(1.0, 1.0),
            ratio: 1.0,
            start: 0.0,
            end: TAU,
        });
        let (lo, hi) = imp.bounds().unwrap();
        let r = 2f64.sqrt();
        assert!((hi.x - r).abs() < 1e-12 && (lo.x + r).abs() < 1e-12);
        assert!((hi.y - r).abs() < 1e-12 && (lo.y + r).abs() < 1e-12);
    }

    #[test]
    fn bounds_cover_rotated_partial_ellipse() {
        // x = cos t - sin t, y = cos t + sin t; t in 0..PI/2: x von 1 bis -1, y bis sqrt(2) bei PI/4.
        let imp = one(Element2d::Ellipse {
            c: DVec2::ZERO,
            major: DVec2::new(1.0, 1.0),
            ratio: 1.0,
            start: 0.0,
            end: PI / 2.0,
        });
        let (lo, hi) = imp.bounds().unwrap();
        assert!((hi.y - 2f64.sqrt()).abs() < 1e-12, "hi.y = {}", hi.y);
        assert!((hi.x - 1.0).abs() < 1e-12 && (lo.x + 1.0).abs() < 1e-12);
        assert!((lo.y - 1.0).abs() < 1e-12, "lo.y = {}", lo.y);
    }

    #[test]
    fn empty_import_has_no_bounds() {
        assert!(Import2d::default().bounds().is_none());
        assert_eq!(Import2d::default().element_count(), 0);
    }

    #[test]
    fn unit_mm_factors() {
        assert_eq!(Unit2d::Foot.mm(), 304.8);
        assert_eq!(Unit2d::Inch.mm(), 25.4);
        assert_eq!(Unit2d::Millimeter.mm(), 1.0);
        assert_eq!(Unit2d::Centimeter.mm(), 10.0);
        assert_eq!(Unit2d::Meter.mm(), 1000.0);
    }
}
