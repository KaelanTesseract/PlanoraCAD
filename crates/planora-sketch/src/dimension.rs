//! Bemaßen: reine Wahl des Maßes aus Auswahl und Zeiger (Entscheidung 13 des Plans Skizzen 5).
//!
//! [`Sketch::dimension_plan`] liest nur die Skizze: kein neuer Stand, kein neues Objekt. Der Wert
//! im geplanten Maß ist stets der gemessene ([`Sketch::measure`]), nie 0; so lässt das Anlegen
//! die Skizze unbewegt. Die Vorschau und der Klick rufen dieselbe Funktion.

use crate::constraint::ConstraintKind;
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use planora_math::DVec2;

/// Ein gewähltes Objekt: ein Punkt oder eine Kurve mit der Stelle, an der sie gewählt wurde.
/// Die Stelle bei `Curve` nutzt [`Sketch::dimension_plan`] noch nicht; keine Regel braucht sie
/// bisher.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DimPick {
    Point(PointId),
    Curve(CurveId, DVec2),
}

/// Art des geplanten Maßes, wie sie die Vorschau zeigt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimMode {
    /// Abstand längs der Verbindung (auch die Länge einer Linie).
    Aligned,
    Horizontal,
    Vertical,
    PointLine,
    Radius,
    Diameter,
    /// Doppelter Abstand zu einer Mittellinie.
    CenterlineDiameter,
    /// Einfacher Abstand zu einer Mittellinie (umgeschaltet).
    CenterlineRadius,
    Angle,
    Major,
    Minor,
}

/// Ein geplantes Maß: Art mit gemessenem Wert, Anzeigeart und Lage des Texts (der Zeiger).
/// `driven`: Das Maß liest nur Exemplare eines Musters und wird beim Anlegen anzeigend
/// ([`Sketch::add_constraint`], Schlusskorrektur I1).
#[derive(Clone, Debug, PartialEq)]
pub struct DimPlan {
    pub kind: ConstraintKind,
    pub mode: DimMode,
    pub anchor: DVec2,
    pub driven: bool,
}

impl DimPlan {
    /// Umschalter Ø/R wie [`Sketch::toggle_radius_diameter`]: `Radius(c, v) ↔ Diameter(c, 2v)`,
    /// `DistancePointLine(p, L, v) ↔ CenterlineDiameter(p, L, 2v)` (nur an Mittellinien).
    /// Zweimal umgeschaltet ist der Plan bitgleich. `None` für andere Maße.
    pub fn toggled(&self, sk: &Sketch) -> Option<DimPlan> {
        use ConstraintKind::*;
        let kind = crate::constraint::toggled_kind(&self.kind)?;
        let mode = match kind {
            Radius(..) => DimMode::Radius,
            Diameter(..) => DimMode::Diameter,
            CenterlineDiameter(_, l, _) if sk.is_centerline(l) => DimMode::CenterlineDiameter,
            DistancePointLine(..) => DimMode::CenterlineRadius,
            _ => return None,
        };
        Some(DimPlan {
            driven: sk.plans_driven(&kind),
            kind,
            mode,
            anchor: self.anchor,
        })
    }
}

/// Ein gewähltes Objekt, wie es das Bemaßen braucht.
#[derive(Clone, Copy)]
enum Item {
    /// Ein Punkt oder die Mitte eines Kreises oder Bogens.
    Spot(PointId),
    Line(CurveId, PointId, PointId),
}

impl Sketch {
    /// Ist `c` eine lebende Mittellinie?
    fn is_centerline(&self, c: CurveId) -> bool {
        self.curve(c).is_some_and(|d| d.centerline)
    }

    /// Das Maß zu den gewählten Objekten `picks` bei der Zeigerlage `pointer` (Entscheidung 13):
    ///
    /// - Linie allein: Länge nach der Zeigerregel wie zwei Punkte (waagerecht, senkrecht oder
    ///   ausgerichtet, wie Fusion); Kreis: Durchmesser; Bogen: Radius; Ellipse oder Ellipsenbogen: die
    ///   Halbachse, deren Achse dem Zeiger näher liegt (**Annahme**).
    /// - Zwei Punkte (auch Mitten von Kreisen und Bögen): Zeiger in der x-Spanne und außerhalb
    ///   der y-Spanne ⇒ waagerecht (`DistanceX`), in der y-Spanne und außerhalb der x-Spanne ⇒
    ///   senkrecht (`DistanceY`), sonst ausgerichtet (`Distance`) (**Annahme** zur Fusion-Regel).
    ///   Ist der Abstand in x (bzw. y) höchstens `tol.linear`, fällt diese Art aus.
    /// - Punkt oder Kreismitte und Linie: `DistancePointLine`, an einer Mittellinie
    ///   `CenterlineDiameter`.
    /// - Zwei Linien: parallel (innerhalb `10 · tol.angular`) ⇒ `DistancePointLine(L2.a, L1)`,
    ///   mit einer Mittellinie `CenterlineDiameter` vom ersten Punkt der anderen; sonst `Angle`,
    ///   so geordnet, dass der Wert positiv ist. Den Ergänzungswinkel kann der Zeiger nicht
    ///   wählen: `Angle` misst zwischen den gerichteten Linien, `π − θ` wäre nur als
    ///   `Angle(L2, L1, π − θ)` mit dem Messwert `−θ` zu haben (Zeile erfüllt, Anzeige falsch).
    ///
    /// Liest das Maß nur Exemplare eines Musters, ist der Plan `driven`: So legt ihn
    /// [`Sketch::add_constraint`] an (Schlusskorrektur I1).
    ///
    /// Der Wert ist der gemessene; `None`, wenn er null wäre, die Auswahl nicht passt, ein
    /// Objekt nicht lebt oder [`Sketch::add_constraint`] das Maß als Schleife zwischen Vorlage
    /// und eigenem Exemplar eines Musters ablehnen würde. Die Skizze bleibt unberührt.
    pub fn dimension_plan(&self, picks: &[DimPick], pointer: DVec2) -> Option<DimPlan> {
        if !pointer.is_finite() {
            return None;
        }
        let (kind, mode) = match picks {
            [DimPick::Curve(c, _)] => self.single_curve(*c, pointer)?,
            [first, second] => {
                let (a, b) = (self.item(first)?, self.item(second)?);
                self.pair(a, b, pointer)?
            }
            _ => return None,
        };
        let measured = self.measure(&kind)?;
        let small = if matches!(kind, ConstraintKind::Angle(..)) {
            self.tol.angular
        } else {
            self.tol.linear
        };
        if !(measured.is_finite() && measured > small) {
            return None;
        }
        let kind = kind.with_value(measured)?;
        if self.closes_pattern_cycle(&kind) {
            return None;
        }
        Some(DimPlan {
            driven: self.plans_driven(&kind),
            kind,
            mode,
            anchor: pointer,
        })
    }

    /// Das Maß einer einzeln gewählten Kurve (Wert noch 0).
    fn single_curve(&self, c: CurveId, pointer: DVec2) -> Option<(ConstraintKind, DimMode)> {
        use ConstraintKind::*;
        Some(match self.curve(c)?.kind {
            CurveKind::Line { a, b } => self.two_points(a, b, pointer)?,
            CurveKind::Circle { .. } => (Diameter(c, 0.0), DimMode::Diameter),
            CurveKind::Arc { .. } => (Radius(c, 0.0), DimMode::Radius),
            CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => {
                let (m, major, _) = self.ellipse_axes(c)?;
                let dir = major.try_normalize()?;
                let d = pointer - m;
                // Abstand zur großen Achse gegen Abstand zur kleinen Achse.
                if dir.perp_dot(d).abs() <= dir.dot(d).abs() {
                    (EllipseMajor(c, 0.0), DimMode::Major)
                } else {
                    (EllipseMinor(c, 0.0), DimMode::Minor)
                }
            }
            CurveKind::Spline { .. } => return None,
        })
    }

    /// Ein gewähltes Objekt als Punkt (auch Kreis- und Bogenmitte) oder Linie.
    fn item(&self, pick: &DimPick) -> Option<Item> {
        match *pick {
            DimPick::Point(p) => {
                self.check_point(p).ok()?;
                Some(Item::Spot(p))
            }
            DimPick::Curve(c, _) => match self.curve(c)?.kind {
                CurveKind::Line { a, b } => Some(Item::Line(c, a, b)),
                CurveKind::Circle { center, .. } | CurveKind::Arc { center, .. } => {
                    Some(Item::Spot(center))
                }
                _ => None,
            },
        }
    }

    /// Das Maß zwischen zwei gewählten Objekten (Wert noch 0).
    fn pair(&self, a: Item, b: Item, pointer: DVec2) -> Option<(ConstraintKind, DimMode)> {
        use ConstraintKind::*;
        let to_line = |p: PointId, l: CurveId| {
            if self.is_centerline(l) {
                (CenterlineDiameter(p, l, 0.0), DimMode::CenterlineDiameter)
            } else {
                (DistancePointLine(p, l, 0.0), DimMode::PointLine)
            }
        };
        Some(match (a, b) {
            (Item::Spot(p), Item::Spot(q)) => self.two_points(p, q, pointer)?,
            (Item::Spot(p), Item::Line(l, ..)) | (Item::Line(l, ..), Item::Spot(p)) => {
                to_line(p, l)
            }
            (Item::Line(l1, a1, b1), Item::Line(l2, a2, b2)) => {
                if l1 == l2 {
                    return None;
                }
                let d1 = (self.pos(b1) - self.pos(a1)).try_normalize()?;
                let d2 = (self.pos(b2) - self.pos(a2)).try_normalize()?;
                if d1.perp_dot(d2).abs() <= 10.0 * self.tol.angular {
                    if self.is_centerline(l1) {
                        to_line(a2, l1)
                    } else if self.is_centerline(l2) {
                        to_line(a1, l2)
                    } else {
                        (DistancePointLine(a2, l1, 0.0), DimMode::PointLine)
                    }
                } else if d1.perp_dot(d2) > 0.0 {
                    (Angle(l1, l2, 0.0), DimMode::Angle)
                } else {
                    (Angle(l2, l1, 0.0), DimMode::Angle)
                }
            }
        })
    }

    /// Zeigerregel zwischen zwei Punkten: waagerecht, senkrecht oder ausgerichtet. `None` für
    /// denselben Punkt zweimal.
    fn two_points(
        &self,
        p: PointId,
        q: PointId,
        pointer: DVec2,
    ) -> Option<(ConstraintKind, DimMode)> {
        use ConstraintKind::*;
        if p == q {
            return None;
        }
        let (a, b) = (self.pos(p), self.pos(q));
        let within = |v: f64, s: f64, t: f64| s.min(t) <= v && v <= s.max(t);
        let in_x = within(pointer.x, a.x, b.x);
        let in_y = within(pointer.y, a.y, b.y);
        let tol = self.tol.linear;
        Some(if in_x && !in_y && (b.x - a.x).abs() > tol {
            (DistanceX(p, q, 0.0), DimMode::Horizontal)
        } else if in_y && !in_x && (b.y - a.y).abs() > tol {
            (DistanceY(p, q, 0.0), DimMode::Vertical)
        } else {
            (Distance(p, q, 0.0), DimMode::Aligned)
        })
    }
}
