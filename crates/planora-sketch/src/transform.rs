//! Verschieben, Drehen, Skalieren und Kopieren als reine Rechnung, ohne Solver (Entscheidungen 10
//! und 11 des Plans Skizzen 4).
//!
//! Bewegt werden die Punkte der gewählten Kurven und die gewählten Punkte, Kreisradius und
//! Ellipsen-Halbachsen mit. Ein Punkt, den auch eine nicht gewählte Kurve benutzt, wird getrennt:
//! Die fremde Kurve bekommt einen neuen Punkt an der alten Lage und bleibt, wo sie war. Es bleibt,
//! was danach ohne Bewegung erfüllt ist; Horizontal und Vertikal entfallen beim Drehen immer. Eine
//! Ähnlichkeit erhält Zweige und Vorzeichen der Abhängigkeiten, weil sie nicht spiegelt.
//!
//! **Hilfsgeometrie folgt der sichtbaren Auswahl** ([`Sketch::transform_set`]): Konstruktionskurven
//! und -punkte, die nur an der Menge hängen, gehören dazu, sonst bliebe die Form an der alten Lage
//! gebunden. Beim Versatz zieht die Vorlage ihre Rollkreise und damit ihre Kopie mit; wird nur die
//! Kopie bewegt, bleiben Vorlage und Rollkreise stehen und die Kopie hört auf zu folgen.
//!
//! **Text geht als Ganzes** (Skizzen 8, Spec E): Ist ein Teil eines Textes im Rahmen in der Menge
//! (Buchstabe, Rahmenlinie, Ecke), gehören Rahmen und alle Buchstaben dazu; der Text folgt seinen
//! Ecken. Text auf Pfad geht nur mit seinem Pfad (sonst `TextOnPath`). Skalieren multipliziert die
//! Höhe mit dem Maßstab, Kopieren legt einen zweiten Text mit gleichen Werten an.

use crate::constraint::{ConstraintData, ConstraintId, ConstraintKind, LineOrPoints};
use crate::edit::SETTLE_FACTOR;
use crate::error::SketchError;
use crate::fillet::wrap_pi;
use crate::model::{CurveId, CurveKind, HandleCurvature, MAX_COORD, PointId, Sketch, SplineHandle};
use crate::offset::ends_of;
use crate::text::{TextData, TextId, TextPlacement};
use planora_i18n::tr;
use planora_math::{DQuat, DVec2, DVec3, Tolerance};
use std::collections::{HashMap, HashSet};

/// Ähnlichkeit: um `base` drehen und strecken, dann um `offset` verschieben.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Similarity {
    pub base: DVec2,
    pub offset: DVec2,
    /// Drehwinkel in Radiant.
    pub angle: f64,
    /// Maßstab, größer als null.
    pub factor: f64,
}

impl Similarity {
    /// Die Ähnlichkeit, die nichts bewegt, mit dem Bezugspunkt `base`.
    pub fn identity(base: DVec2) -> Similarity {
        Similarity {
            base,
            offset: DVec2::ZERO,
            angle: 0.0,
            factor: 1.0,
        }
    }

    pub fn apply(&self, p: DVec2) -> DVec2 {
        self.base + self.linear(p - self.base) + self.offset
    }

    /// Bewegt die Ähnlichkeit bis auf `tol` nichts? Volle Umdrehungen zählen wie keine.
    pub fn is_identity(&self, tol: f64) -> bool {
        self.offset.length() <= tol
            && wrap_pi(self.angle).abs() <= tol
            && (self.factor - 1.0).abs() <= tol
    }

    /// Eine Richtung im Raum (etwa die Normale eines Textes in 3D) um die z-Achse gedreht, ohne
    /// Maßstab (Skizzen 10).
    pub(crate) fn turn(&self, n: DVec3) -> DVec3 {
        let (sin, cos) = self.angle.sin_cos();
        DVec3::new(n.x * cos - n.y * sin, n.x * sin + n.y * cos, n.z)
    }

    /// Der Anteil ohne Verschiebung, für Vektoren wie die große Halbachse.
    pub(crate) fn linear(&self, v: DVec2) -> DVec2 {
        let (sin, cos) = self.angle.sin_cos();
        DVec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos) * self.factor
    }

    fn is_finite(&self) -> bool {
        self.base.is_finite()
            && self.offset.is_finite()
            && self.angle.is_finite()
            && self.factor.is_finite()
    }
}

/// Starre Bewegung im Raum der Skizze (Skizzen 10, Verschieben aus der Ebene, Spec Entwurf 8): um
/// `pivot` mit `rot` drehen, dann um `shift` verschieben. Ohne z-Anteil und ohne Drehung um X
/// oder Y ist sie eine [`Similarity`] ohne Maßstab ([`Rigid3::planar`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rigid3 {
    pub rot: DQuat,
    pub shift: DVec3,
    pub pivot: DVec3,
}

impl Rigid3 {
    /// Die Bewegung, die nichts bewegt, mit dem Drehpunkt `pivot`.
    pub fn identity(pivot: DVec3) -> Rigid3 {
        Rigid3 {
            rot: DQuat::IDENTITY,
            shift: DVec3::ZERO,
            pivot,
        }
    }

    pub fn apply(&self, p: DVec3) -> DVec3 {
        self.pivot + self.rot * (p - self.pivot) + self.shift
    }

    /// Eine Richtung (Normale, Halbachse) gedreht, ohne Verschiebung.
    pub fn turn(&self, n: DVec3) -> DVec3 {
        self.rot * n
    }

    fn is_finite(&self) -> bool {
        self.rot.is_finite() && self.shift.is_finite() && self.pivot.is_finite()
    }

    /// Dreht sie um mehr als `tol.angular`?
    fn rotates(&self, tol: &Tolerance) -> bool {
        [DVec3::X, DVec3::Y, DVec3::Z]
            .iter()
            .any(|&a| (self.rot * a - a).length() > tol.angular)
    }

    /// Die Bewegung als Ähnlichkeit in der Skizzenebene, wenn sie weder anhebt (|z| bis
    /// `tol.linear`) noch um X oder Y dreht (bis `tol.angular`); sonst `None`. Der Bezugspunkt
    /// ist `pivot` in der Draufsicht, der Winkel die Drehung um Z.
    pub fn planar(&self, tol: &Tolerance) -> Option<Similarity> {
        let rot = self.rot.normalize();
        if self.shift.z.abs() > tol.linear || (rot * DVec3::Z - DVec3::Z).length() > tol.angular {
            return None;
        }
        // Reine Drehung um Z: der Winkel unmittelbar aus dem Quaternion (die Einheit ergibt 0).
        let angle = if rot.x == 0.0 && rot.y == 0.0 {
            2.0 * rot.z.atan2(rot.w)
        } else {
            let x = rot * DVec3::X;
            x.y.atan2(x.x)
        };
        Some(Similarity {
            base: self.pivot.truncate(),
            offset: self.shift.truncate(),
            angle,
            factor: 1.0,
        })
    }
}

/// Was [`Sketch::move_rigid3`] an den Abhängigkeiten geändert hat.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MoveReport {
    /// Entfallene Abhängigkeiten und Maße (2D-only an 3D-Geometrie, nach der Bewegung verletzt,
    /// Tangenten zwischen gekippten Ebenen); interne zählen nicht.
    pub removed_constraints: usize,
    /// 2D-Maße an bewegter Geometrie, die nun in ihrer 3D-Fassung rechnen.
    pub converted_dimensions: usize,
    /// Getrennte Ecken wie [`TransformReport::split`].
    pub split: usize,
    /// Original und Kopie beim Kopieren.
    pub copy: Option<CopyMap>,
}

/// Der Grund, warum eine Bewegung nicht geht, als Fehler mit den Meldungen der Oberfläche.
pub fn block_error(block: TransformBlock) -> SketchError {
    match block {
        TransformBlock::FixedInSet(_) | TransformBlock::BoundToFixed(_) => {
            SketchError::InvalidInput(tr!("sketch-move-fixed"))
        }
        TransformBlock::NothingToDo => SketchError::InvalidInput(tr!("sketch-move-nothing")),
        TransformBlock::Degenerate => SketchError::Degenerate(tr!("sketch-move-degenerate")),
        TransformBlock::PatternInstance => {
            SketchError::InvalidInput(tr!("sketch-move-pattern-instance"))
        }
        TransformBlock::TextOnPath => SketchError::InvalidInput(tr!("sketch-text-path-follows")),
        TransformBlock::NotIn3d => SketchError::InvalidInput(tr!("sketch-err-tool-not-in-3d")),
    }
}

/// Was bewegt wird: Kurven und Punkte. Über [`Sketch::transform_set`] enthält `points` auch die
/// Punkte der Kurven, und die Hilfsgeometrie der Auswahl gehört dazu.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TransformSet {
    pub points: Vec<PointId>,
    pub curves: Vec<CurveId>,
}

/// Warum eine Bewegung nicht geht.
#[derive(Clone, Debug, PartialEq)]
pub enum TransformBlock {
    /// Der Punkt hat ein `Fixed` und würde sich bewegen.
    FixedInSet(PointId),
    /// Diese Lage-Bindung an festgehaltene Geometrie wäre danach nicht mehr erfüllt.
    BoundToFixed(ConstraintId),
    /// Keine Menge, oder die Ähnlichkeit bewegt nichts.
    NothingToDo,
    /// Ungültige Zahlen (Maßstab nicht über null, nicht endlich), oder eine Kurve oder ein Maß
    /// fiele unter die Toleranz oder Koordinaten über die Grenze.
    Degenerate,
    /// In der Menge ist ein Exemplar eines Musters: Seine Lage folgt aus Vorlage und Musterwerten
    /// (Meldung `sketch-move-pattern-instance`). Kopieren geht.
    PatternInstance,
    /// In der Menge ist Text auf einem Pfad, der Pfad aber nicht: Der Text folgt seinem Pfad
    /// (Spec E, Meldung `sketch-text-path-follows`).
    TextOnPath,
    /// Skalieren geht nicht mit 3D-Kurven oder 3D-Punkten in der Menge (Skizzen 10, Meldung
    /// `sketch-err-tool-not-in-3d`); Verschieben und Drehen bleiben, wie sie sind.
    NotIn3d,
}

/// Was eine Bewegung an den Abhängigkeiten geändert hat.
#[derive(Clone, Debug, PartialEq)]
pub struct TransformReport {
    /// Entfallene Abhängigkeiten.
    pub dropped: Vec<ConstraintId>,
    /// Längenmaße, deren Wert mit dem Maßstab multipliziert wurde.
    pub scaled: Vec<ConstraintId>,
    /// So viele bewegte Punkte hat eine fremde Kurve mitbenutzt und sind nun getrennt: Dort ist eine
    /// Ecke aufgegangen.
    pub split: usize,
}

/// Original und Kopie der Objekte aus [`Sketch::copy_transformed`].
#[derive(Clone, Debug, PartialEq)]
pub struct CopyMap {
    pub points: HashMap<PointId, PointId>,
    pub curves: HashMap<CurveId, CurveId>,
    pub constraints: HashMap<ConstraintId, ConstraintId>,
    /// Original und Kopie der Texte (Spec E).
    pub texts: HashMap<TextId, TextId>,
}

/// Die Abhängigkeit mit abgebildeten Punkten und Kurven.
fn map_ids(
    kind: &ConstraintKind,
    p: impl Fn(PointId) -> PointId,
    c: impl Fn(CurveId) -> CurveId,
) -> ConstraintKind {
    use ConstraintKind::*;
    let lp = |l: &LineOrPoints| match *l {
        LineOrPoints::Line(x) => LineOrPoints::Line(c(x)),
        LineOrPoints::Points(a, b) => LineOrPoints::Points(p(a), p(b)),
    };
    match kind {
        Coincident(a, b) => Coincident(p(*a), p(*b)),
        PointOnCurve(a, x) => PointOnCurve(p(*a), c(*x)),
        Horizontal(l) => Horizontal(lp(l)),
        Vertical(l) => Vertical(lp(l)),
        AlongZ(l) => AlongZ(lp(l)),
        Parallel(a, b) => Parallel(c(*a), c(*b)),
        Perpendicular(a, b) => Perpendicular(c(*a), c(*b)),
        Tangent(a, b) => Tangent(c(*a), c(*b)),
        Equal(a, b) => Equal(c(*a), c(*b)),
        Fixed(a, x, y) => Fixed(p(*a), *x, *y),
        Fixed3(a, x, y, z) => Fixed3(p(*a), *x, *y, *z),
        Symmetric(a, b, x) => Symmetric(p(*a), p(*b), c(*x)),
        Midpoint(a, x) => Midpoint(p(*a), c(*x)),
        Distance(a, b, v) => Distance(p(*a), p(*b), *v),
        DistancePointLine(a, x, v) => DistancePointLine(p(*a), c(*x), *v),
        Radius(x, v) => Radius(c(*x), *v),
        Diameter(x, v) => Diameter(c(*x), *v),
        Angle(a, b, v) => Angle(c(*a), c(*b), *v),
        ArcRadius(x) => ArcRadius(c(*x)),
        SplineCurvatureHandle(x, a) => SplineCurvatureHandle(c(*x), p(*a)),
        EllipseMajor(x, v) => EllipseMajor(c(*x), *v),
        EllipseMinor(x, v) => EllipseMinor(c(*x), *v),
        EllipseAngle(x, v) => EllipseAngle(c(*x), *v),
        EllipticArcEnds(x) => EllipticArcEnds(c(*x)),
        InPlane(x) => InPlane(c(*x)),
        EllipseAxes3(x, ax, ay, az, m) => EllipseAxes3(c(*x), *ax, *ay, *az, *m),
        Collinear(a, b) => Collinear(c(*a), c(*b)),
        Smooth(a, pa, b, pb) => Smooth(c(*a), p(*pa), c(*b), p(*pb)),
        EndTangent(a, pa, b, pb) => EndTangent(c(*a), p(*pa), c(*b), p(*pb)),
        DistanceX(a, b, v) => DistanceX(p(*a), p(*b), *v),
        DistanceY(a, b, v) => DistanceY(p(*a), p(*b), *v),
        CenterlineDiameter(a, x, v) => CenterlineDiameter(p(*a), c(*x), *v),
        SlotOverall(a, b, x, v) => SlotOverall(p(*a), p(*b), c(*x), *v),
        PatternValue(m, slot, v) => PatternValue(*m, *slot, *v),
    }
}

/// Das Maß nach der Bewegung: Längen mal Faktor, der Ellipsenwinkel plus Drehwinkel; alles andere
/// bleibt, wie es ist, auch der Winkel zweier Linien. Der zweite Wert sagt, ob es ein Längenmaß ist.
fn transformed_value(kind: &ConstraintKind, s: &Similarity) -> (ConstraintKind, bool) {
    use ConstraintKind::*;
    match *kind {
        Distance(_, _, v)
        | DistancePointLine(_, _, v)
        | Radius(_, v)
        | Diameter(_, v)
        | EllipseMajor(_, v)
        | EllipseMinor(_, v)
        | DistanceX(_, _, v)
        | DistanceY(_, _, v)
        | CenterlineDiameter(_, _, v)
        | SlotOverall(_, _, _, v) => (
            kind.with_value(v * s.factor)
                .unwrap_or_else(|| kind.clone()),
            true,
        ),
        EllipseAngle(_, v) => (
            kind.with_value(v + s.angle).unwrap_or_else(|| kind.clone()),
            false,
        ),
        _ => (kind.clone(), false),
    }
}

/// Ist der skalierte Wert eines Längenmaßes noch erlaubt? Ohne die Regel „kleine Halbachse nicht
/// länger als die große“: Beide Halbachsen wachsen um denselben Faktor.
fn length_in_range(kind: &ConstraintKind, tol: f64) -> bool {
    use ConstraintKind::*;
    match *kind {
        DistancePointLine(_, _, v) => v <= MAX_COORD,
        Distance(_, _, v)
        | Radius(_, v)
        | Diameter(_, v)
        | EllipseMajor(_, v)
        | EllipseMinor(_, v)
        | DistanceX(_, _, v)
        | DistanceY(_, _, v)
        | CenterlineDiameter(_, _, v)
        | SlotOverall(_, _, _, v) => v > tol && v <= MAX_COORD,
        _ => true,
    }
}

fn is_position_binding(kind: &ConstraintKind) -> bool {
    use ConstraintKind::*;
    matches!(
        kind,
        Coincident(..) | PointOnCurve(..) | Midpoint(..) | Symmetric(..)
    )
}

/// Bindet an die Achsen der Skizze: waagerecht und senkrecht, auch als Abstand (Skizzen 5).
/// Beim Drehen entfällt sie.
fn is_axis_binding(kind: &ConstraintKind) -> bool {
    matches!(
        kind,
        ConstraintKind::Horizontal(_)
            | ConstraintKind::Vertical(_)
            | ConstraintKind::DistanceX(..)
            | ConstraintKind::DistanceY(..)
    )
}

fn is_active(d: &ConstraintData) -> bool {
    d.enabled && !d.driven
}

/// Die Variablen der Größe einer Kurve: Radius des Kreises, Halbachsen der Ellipse.
fn size_vars(kind: &CurveKind) -> Vec<usize> {
    match *kind {
        CurveKind::Circle { radius, .. } => vec![radius],
        CurveKind::Ellipse {
            ax, ay, az, minor, ..
        }
        | CurveKind::EllipticArc {
            ax, ay, az, minor, ..
        } => [ax, ay, minor].into_iter().chain(az).collect(),
        _ => vec![],
    }
}

/// Die Art mit abgebildeten Punkten und Variablen.
pub(crate) fn map_curve(
    kind: &CurveKind,
    p: impl Fn(PointId) -> PointId,
    v: impl Fn(usize) -> usize,
) -> CurveKind {
    match kind {
        CurveKind::Line { a, b } => CurveKind::Line { a: p(*a), b: p(*b) },
        CurveKind::Circle { center, radius } => CurveKind::Circle {
            center: p(*center),
            radius: v(*radius),
        },
        CurveKind::Arc { center, start, end } => CurveKind::Arc {
            center: p(*center),
            start: p(*start),
            end: p(*end),
        },
        CurveKind::Ellipse {
            center,
            ax,
            ay,
            az,
            minor,
        } => CurveKind::Ellipse {
            center: p(*center),
            ax: v(*ax),
            ay: v(*ay),
            az: az.map(&v),
            minor: v(*minor),
        },
        CurveKind::Spline { points, handles } => CurveKind::Spline {
            points: points.iter().map(|&x| p(x)).collect(),
            handles: handles
                .iter()
                .map(|h| SplineHandle {
                    at: p(h.at),
                    line: h.line,
                    tip: p(h.tip),
                    curvature: match h.curvature {
                        HandleCurvature::Arc { arc, center, other } => HandleCurvature::Arc {
                            arc,
                            center: p(center),
                            other: p(other),
                        },
                        other => other,
                    },
                })
                .collect(),
        },
        CurveKind::ControlSpline {
            points,
            degree,
            knots,
        } => CurveKind::ControlSpline {
            points: points.iter().map(|&x| p(x)).collect(),
            degree: *degree,
            knots: knots.clone(),
        },
        CurveKind::Conic {
            start,
            end,
            apex,
            rho,
        } => CurveKind::Conic {
            start: p(*start),
            end: p(*end),
            apex: p(*apex),
            rho: *rho,
        },
        CurveKind::FixedSpline { start, end, curve } => CurveKind::FixedSpline {
            start: p(*start),
            end: p(*end),
            curve: curve.clone(),
        },
        CurveKind::EllipticArc {
            center,
            ax,
            ay,
            az,
            minor,
            start,
            end,
        } => CurveKind::EllipticArc {
            center: p(*center),
            ax: v(*ax),
            ay: v(*ay),
            az: az.map(&v),
            minor: v(*minor),
            start: p(*start),
            end: p(*end),
        },
    }
}

impl Sketch {
    /// Die Menge zu den gewählten Kurven und Punkten: nur lebende Objekte, jedes einmal, in `points`
    /// auch die Punkte der Kurven. Wiederholt, bis sich nichts mehr ändert, kommt dazu:
    /// - Hilfsgeometrie, deren Bindungen alle in die Menge gehen (`helpers_of`);
    /// - zu einer gewählten Vorlage ihre Rollkreise samt den Stücken der Kopie, die sie tragen
    ///   (`rollers_of`). Die Kopie allein zieht weder Vorlage noch Rollkreis mit;
    /// - zu einem Teil eines Textes der ganze Text (`text_family`).
    pub fn transform_set(&self, curves: &[CurveId], points: &[PointId]) -> TransformSet {
        let mut set = Collected::default();
        for &c in curves {
            if self.check_curve(c).is_ok() {
                set.add_curve(self, c);
            }
        }
        for &p in points {
            if self.check_point(p).is_ok() {
                set.add_point(p);
            }
        }
        if set.out.points.is_empty() {
            return set.out;
        }
        // Exemplare kommen nie als Hilfsgeometrie dazu; gewählt sperren sie die Bewegung.
        let (instance_curves, instance_points) = self.instance_parts();
        loop {
            let before = set.out.points.len() + set.out.curves.len();
            // Griff und Spline gehen zusammen, sonst risse das Trennen den Griff vom Stützpunkt.
            for c in self.handle_family(&set.out.curves) {
                set.add_curve(self, c);
            }
            let (helper_curves, helper_points) = self.helpers_of(&set);
            for c in helper_curves {
                if !instance_curves.contains(&c) {
                    set.add_curve(self, c);
                }
            }
            for p in helper_points {
                if !instance_points.contains(&p) {
                    set.add_point(p);
                }
            }
            for c in self.rollers_of(&set) {
                set.add_curve(self, c);
            }
            for c in self.text_family(&set) {
                set.add_curve(self, c);
            }
            if set.out.points.len() + set.out.curves.len() == before {
                return set.out;
            }
        }
    }

    /// Konstruktionskurven und -punkte außerhalb der Menge, deren aktive Abhängigkeiten nur in die
    /// Menge oder zu ebensolcher Hilfsgeometrie gehen, die keine fremde Kurve benutzt, die kein
    /// `Fixed` tragen und die über gemeinsame Punkte oder Abhängigkeiten an der Menge hängen.
    /// Dazu zählt der freie Mittelpunkt eines Langlochs, ein einfacher Punkt nur mit `Midpoint`.
    fn helpers_of(&self, set: &Collected) -> (Vec<CurveId>, Vec<PointId>) {
        let mut curves: HashSet<CurveId> = self
            .curves()
            .filter(|(c, d)| d.construction && !set.curves.contains(c))
            .map(|(c, _)| c)
            .collect();
        let active: Vec<&ConstraintData> = self
            .constraints()
            .map(|(_, d)| d)
            .filter(|d| is_active(d))
            .collect();
        let mut users: HashMap<PointId, Vec<CurveId>> = HashMap::new();
        for (c, d) in self.curves() {
            for p in d.kind.points() {
                users.entry(p).or_default().push(c);
            }
        }
        let mut bound: HashMap<PointId, Vec<&ConstraintKind>> = HashMap::new();
        for d in &active {
            for p in d.kind.points() {
                bound.entry(p).or_default().push(&d.kind);
            }
        }
        let mut points: HashSet<PointId> = self
            .points()
            .filter(|&(p, d)| {
                let used = users.get(&p);
                let by_helper = used.is_some_and(|u| u.iter().any(|c| curves.contains(c)));
                let at_middle = used.is_none()
                    && bound.get(&p).is_some_and(|b| {
                        b.iter().all(|k| matches!(k, ConstraintKind::Midpoint(..)))
                    });
                !set.points.contains(&p) && (d.construction || by_helper || at_middle)
            })
            .map(|(p, _)| p)
            .collect();
        for d in &active {
            if let ConstraintKind::Fixed(p, ..) | ConstraintKind::Fixed3(p, ..) = d.kind {
                points.remove(&p);
            }
        }
        loop {
            let before = curves.len() + points.len();
            curves.retain(|&c| {
                self.curves[c]
                    .kind
                    .points()
                    .iter()
                    .all(|p| set.points.contains(p) || points.contains(p))
            });
            points.retain(|p| {
                users.get(p).is_none_or(|u| {
                    u.iter()
                        .all(|c| set.curves.contains(c) || curves.contains(c))
                })
            });
            for d in &active {
                let (ps, cs) = (d.kind.points(), d.kind.curves());
                let inside = ps
                    .iter()
                    .all(|p| set.points.contains(p) || points.contains(p))
                    && cs
                        .iter()
                        .all(|c| set.curves.contains(c) || curves.contains(c));
                if !inside {
                    for p in &ps {
                        points.remove(p);
                    }
                    for c in &cs {
                        curves.remove(c);
                    }
                }
            }
            if curves.len() + points.len() == before {
                break;
            }
        }
        // Nur, was an der Menge hängt: Ein freier Konstruktionskreis daneben bleibt.
        let mut reached_curves = HashSet::new();
        let mut reached_points = HashSet::new();
        loop {
            let before = reached_curves.len() + reached_points.len();
            for &c in &curves {
                let kind_points = self.curves[c].kind.points();
                if kind_points
                    .iter()
                    .any(|p| set.points.contains(p) || reached_points.contains(p))
                {
                    reached_curves.insert(c);
                }
                if reached_curves.contains(&c) {
                    reached_points.extend(kind_points.into_iter().filter(|p| points.contains(p)));
                }
            }
            for d in &active {
                let (ps, cs) = (d.kind.points(), d.kind.curves());
                let linked = ps
                    .iter()
                    .any(|p| set.points.contains(p) || reached_points.contains(p))
                    || cs
                        .iter()
                        .any(|c| set.curves.contains(c) || reached_curves.contains(c));
                if linked {
                    reached_points.extend(ps.into_iter().filter(|p| points.contains(p)));
                    reached_curves.extend(cs.into_iter().filter(|c| curves.contains(c)));
                }
            }
            if reached_curves.len() + reached_points.len() == before {
                break;
            }
        }
        let helper_curves = self
            .curves()
            .map(|(c, _)| c)
            .filter(|c| reached_curves.contains(c))
            .collect();
        let helper_points = self
            .points()
            .map(|(p, _)| p)
            .filter(|p| reached_points.contains(p))
            .collect();
        (helper_curves, helper_points)
    }

    /// Die noch fehlenden Kurven der Texte, von denen ein Teil in der Menge ist (Spec E): beim Text
    /// im Rahmen die Rahmenlinien und alle Buchstaben, sobald ein Buchstabe, eine Rahmenlinie, eine
    /// Ecke oder ein erzeugter Punkt dabei ist (auch als Punkt einer anderen Kurve, sonst risse das
    /// Trennen die Ecke vom Rahmen). Beim Text auf Pfad alle Buchstaben, sobald einer dabei ist;
    /// der Pfad kommt nicht von selbst dazu (siehe `TransformBlock::TextOnPath`).
    fn text_family(&self, set: &Collected) -> Vec<CurveId> {
        let mut out = Vec::new();
        for (_, d) in self.texts() {
            let frame = match d.placement {
                TextPlacement::Box { lines, corners, .. } => Some((lines, corners)),
                TextPlacement::Path { .. } => None,
            };
            let touched = d.curves.iter().any(|c| set.curves.contains(c))
                || d.points.iter().any(|p| set.points.contains(p))
                || frame.is_some_and(|(lines, corners)| {
                    lines.iter().any(|c| set.curves.contains(c))
                        || corners.iter().any(|p| set.points.contains(p))
                });
            if !touched {
                continue;
            }
            let lines: &[CurveId] = match &frame {
                Some((lines, _)) => lines,
                None => &[],
            };
            out.extend(
                lines
                    .iter()
                    .chain(&d.curves)
                    .filter(|c| !set.curves.contains(c) && self.curves[**c].alive),
            );
        }
        out
    }

    /// Die Texte, die mit der Menge gehen: deren Buchstaben oder Rahmenlinien darin liegen (über
    /// `transform_set` dann alle ihre Kurven).
    fn texts_in_set(&self, set: &TransformSet) -> Vec<TextId> {
        if self.texts.is_empty() {
            return Vec::new();
        }
        let curves: HashSet<CurveId> = set.curves.iter().copied().collect();
        self.texts()
            .filter(|(_, d)| {
                d.curves.iter().any(|c| curves.contains(c))
                    || matches!(d.placement, TextPlacement::Box { lines, .. }
                        if lines.iter().any(|c| curves.contains(c)))
            })
            .map(|(t, _)| t)
            .collect()
    }

    /// Ist unter `texts` ein Text auf Pfad, dessen Pfad nicht in der Menge ist? Er folgt seinem
    /// Pfad und geht nicht allein (Spec E).
    fn text_off_its_path(&self, set: &TransformSet, texts: &[TextId]) -> bool {
        texts.iter().any(|&t| {
            matches!(self.texts[t].placement, TextPlacement::Path { path, .. }
                if !set.curves.contains(&path))
        })
    }

    /// Die Kurven, die an einem Konstruktionskreis `roller` tangential hängen.
    fn touching_roller(&self, roller: CurveId) -> Vec<CurveId> {
        self.constraints()
            .filter(|(_, d)| is_active(d))
            .filter_map(|(_, d)| match d.kind {
                ConstraintKind::Tangent(x, y) if x == roller => Some(y),
                ConstraintKind::Tangent(x, y) if y == roller => Some(x),
                _ => None,
            })
            .collect()
    }

    /// Rollkreise außerhalb der Menge, die auf einer gewählten Vorlage sitzen, und die Stücke der
    /// Kopie, die sie tragen (siehe `rolls_on_source`).
    fn rollers_of(&self, set: &Collected) -> Vec<CurveId> {
        let mut out = Vec::new();
        for (roller, d) in self.curves() {
            let CurveKind::Circle { center, .. } = d.kind else {
                continue;
            };
            if !d.construction || set.curves.contains(&roller) {
                continue;
            }
            let on_set_curve = set
                .curves
                .iter()
                .any(|&q| self.curves[q].kind.points().contains(&center))
                || self.constraints().any(|(_, k)| {
                    is_active(k)
                        && matches!(k.kind, ConstraintKind::PointOnCurve(p, q)
                            if p == center && set.curves.contains(&q))
                });
            if !on_set_curve {
                continue;
            }
            let touched = self.touching_roller(roller);
            if touched.is_empty() || !self.rolls_on_source(roller, &touched) {
                continue;
            }
            out.push(roller);
            out.extend(self.copy_run(
                &touched,
                |c| !set.curves.contains(&c),
                |q| set.curves.contains(&q),
            ));
        }
        out
    }

    /// Die Stücke einer Kopie ab `start`, über gemeinsame Enden: Tangente oder `Equal` zum Nachbarn
    /// oder sonst alles, was `continues_copy` nennt. Ein Nachbar, der nur über `Parallel` zu seiner
    /// Vorlage als Stück erkennbar ist (scharfe Fuge, eigener Lauf), zählt nur, wenn `source` für
    /// diese Vorlage gilt. Nur Kurven, für die `allowed` gilt; Rollkreise und Querlinien nie.
    fn copy_run(
        &self,
        start: &[CurveId],
        allowed: impl Fn(CurveId) -> bool,
        source: impl Fn(CurveId) -> bool,
    ) -> Vec<CurveId> {
        let mut run: Vec<CurveId> = start.to_vec();
        let mut next = 0;
        while let Some(&x) = run.get(next) {
            next += 1;
            let ends = ends_of(&self.curves[x].kind);
            for (c, d) in self.curves() {
                if run.contains(&c)
                    || !allowed(c)
                    || matches!(d.kind, CurveKind::Circle { .. })
                    || self.is_cross_line(c)
                    || !ends_of(&d.kind).iter().any(|p| ends.contains(p))
                    || !self.continues_copy(x, c)
                {
                    continue;
                }
                let joined = self.constraints().any(|(_, k)| {
                    is_active(k)
                        && matches!(k.kind, ConstraintKind::Tangent(a, b) | ConstraintKind::Equal(a, b)
                            if (a, b) == (x, c) || (a, b) == (c, x))
                });
                let foreign_source = self.constraints().any(|(_, k)| {
                    is_active(k)
                        && matches!(k.kind, ConstraintKind::Parallel(q, y) if y == c && !source(q))
                });
                if joined || !foreign_source {
                    run.push(c);
                }
            }
        }
        run
    }

    /// Querlinie eines Versatzes: Konstruktionslinie mit `Perpendicular` oder einem Punkt darauf.
    fn is_cross_line(&self, c: CurveId) -> bool {
        let d = &self.curves[c];
        matches!(d.kind, CurveKind::Line { .. })
            && d.construction
            && self.constraints().any(|(_, k)| {
                is_active(k)
                    && matches!(k.kind,
                        ConstraintKind::Perpendicular(_, x) | ConstraintKind::PointOnCurve(_, x)
                            if x == c)
            })
    }

    /// Wird nur die Kopie bewegt, hört sie auf zu folgen: Ihre Bindungen an Rollkreis, Vorlage und
    /// Querlinien entfallen, auch wenn sie nach dieser Bewegung zufällig noch gelten.
    fn severed_copy_bindings(&self, scope: &Scope) -> HashSet<ConstraintId> {
        let mut severed = HashSet::new();
        for (roller, d) in self.curves() {
            if !d.construction
                || !matches!(d.kind, CurveKind::Circle { .. })
                || scope.curves.contains(&roller)
            {
                continue;
            }
            let touched = self.touching_roller(roller);
            let moving: Vec<CurveId> = touched
                .iter()
                .copied()
                .filter(|c| scope.curves.contains(c))
                .collect();
            if moving.is_empty() || !self.rolls_on_source(roller, &touched) {
                continue;
            }
            let run = self.copy_run(&moving, |c| scope.curves.contains(&c), |_| true);
            let run_points: HashSet<PointId> = run
                .iter()
                .flat_map(|&c| self.curves[c].kind.points())
                .collect();
            for (id, k) in self.constraints().filter(|(_, k)| is_active(k)) {
                let cut = match k.kind {
                    ConstraintKind::Tangent(a, b) => {
                        (a == roller && moving.contains(&b)) || (b == roller && moving.contains(&a))
                    }
                    ConstraintKind::Parallel(q, y) => {
                        run.contains(&y) && !scope.curves.contains(&q)
                    }
                    ConstraintKind::PointOnCurve(p, line) => {
                        run_points.contains(&p)
                            && !scope.curves.contains(&line)
                            && self.is_cross_line(line)
                    }
                    _ => false,
                };
                if cut {
                    severed.insert(id);
                }
            }
        }
        severed
    }

    /// Alle Kurven und freien Punkte ohne feste Geometrie, also ohne Achshilfslinie, Kantenkopien
    /// und festen Ursprung, und ohne Exemplare von Mustern (sie folgen ihrer Vorlage) und ohne Text
    /// auf einem Pfad, der stehen bleibt.
    pub fn whole_sketch_set(&self) -> TransformSet {
        let held = self.held_points();
        let (instances, _) = self.instance_parts();
        let mut curves: Vec<CurveId> = self
            .curves()
            .map(|(c, _)| c)
            .filter(|&c| !self.is_held_curve(c, &held) && !instances.contains(&c))
            .collect();
        // Text auf einem Pfad, der stehen bleibt, bleibt bei ihm (Spec E).
        let moving: HashSet<CurveId> = curves.iter().copied().collect();
        let staying: HashSet<CurveId> = self
            .texts()
            .filter(|(_, d)| {
                matches!(d.placement, TextPlacement::Path { path, .. } if !moving.contains(&path))
            })
            .flat_map(|(_, d)| d.curves.iter().copied())
            .collect();
        curves.retain(|c| !staying.contains(c));
        let used: HashSet<PointId> = self.curves().flat_map(|(_, d)| d.kind.points()).collect();
        let points: Vec<PointId> = self
            .points()
            .map(|(p, _)| p)
            .filter(|p| !used.contains(p) && !held.contains(p))
            .collect();
        self.transform_set(&curves, &points)
    }

    /// Würde die Ähnlichkeit eine 3D-Kurve oder einen 3D-Punkt der Menge skalieren?
    fn scales_3d(&self, set: &TransformSet, s: &Similarity) -> bool {
        (s.factor - 1.0).abs() > self.tol.linear
            && (set.curves.iter().any(|&c| self.is_3d_curve(c))
                || set.points.iter().any(|&p| self.is_3d_point(p)))
    }

    /// Geht die Bewegung? Dieselbe Rechnung wie [`Sketch::apply_transform`].
    pub fn check_transform(
        &self,
        set: &TransformSet,
        s: &Similarity,
    ) -> Result<(), TransformBlock> {
        self.transformed(set, s).map(|_| ())
    }

    /// Bewegt die Menge; die Skizze ändert sich nur bei `Ok`. Kein Solver.
    pub fn apply_transform(
        &mut self,
        set: &TransformSet,
        s: &Similarity,
    ) -> Result<TransformReport, TransformBlock> {
        let (moved, report) = self.transformed(set, s)?;
        *self = moved;
        Ok(report)
    }

    /// Linienzüge der Kurven der Menge nach der Bewegung, in der Reihenfolge der Menge; leer, wenn
    /// die Bewegung nicht geht. Dieselbe Rechnung wie [`Sketch::apply_transform`].
    pub fn transform_preview(
        &self,
        set: &TransformSet,
        s: &Similarity,
        steps_per_turn: usize,
    ) -> Vec<Vec<DVec2>> {
        let Ok((moved, _)) = self.transformed(set, s) else {
            return Vec::new();
        };
        self.transform_set(&set.curves, &set.points)
            .curves
            .iter()
            .filter_map(|&c| moved.curve_polyline(c, steps_per_turn))
            .collect()
    }

    /// Legt eine bewegte Kopie der Menge an: Kurven samt Stil, jede innere Abhängigkeit mit
    /// abgebildeten Ids und ihrem `driven` und `enabled`; `Fixed` nicht, die Kopie ist frei, und
    /// keine Bindung ans Original. Die Zweige setzt `add_constraint` an der Kopie neu. Horizontal,
    /// vertikal und was sonst an der Kopie nicht gilt, entfällt. Zu jedem Text der Menge entsteht
    /// ein zweiter (`copy_texts`); Text auf Pfad nur mit seinem Pfad (`sketch-text-path-follows`).
    /// Bei einem Fehler bleibt die Skizze unverändert.
    pub fn copy_transformed(
        &mut self,
        set: &TransformSet,
        s: &Similarity,
    ) -> Result<CopyMap, SketchError> {
        if !s.is_finite() {
            return Err(SketchError::Degenerate(tr!("sketch-err-value-finite")));
        }
        if s.factor <= 0.0 {
            return Err(SketchError::Degenerate(tr!(
                "sketch-err-dimension-positive"
            )));
        }
        let set = self.transform_set(&set.curves, &set.points);
        if set.points.is_empty() {
            return Err(SketchError::InvalidInput(tr!("sketch-nothing-selected")));
        }
        if self.scales_3d(&set, s) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-tool-not-in-3d")));
        }
        let texts = self.texts_in_set(&set);
        if self.text_off_its_path(&set, &texts) {
            return Err(SketchError::InvalidInput(tr!("sketch-text-path-follows")));
        }
        let s = &self.settled(&set, s);
        self.atomically(|sk| {
            let mut points = HashMap::new();
            for &p in &set.points {
                points.insert(p, sk.add_point_like(p, s.apply(sk.pos(p)))?);
            }
            let mut sizes = HashMap::new();
            let mut curves = HashMap::new();
            for &c in &set.curves {
                curves.insert(c, sk.copy_curve_transformed(c, &points, &mut sizes, s)?);
            }
            for &c in &set.curves {
                sk.carry_handles(c, curves[&c], &points, &curves);
            }
            let constraints = sk.copy_inner_constraints(&points, &curves, s)?;
            let texts = sk.copy_texts(&texts, &points, &curves, s)?;
            Ok(CopyMap {
                points,
                curves,
                constraints,
                texts,
            })
        })
    }

    /// Verschiebt oder kopiert die Menge starr im Raum (Skizzen 10, Spec Entwurf 8). Ohne z-Anteil
    /// und ohne Drehung um X oder Y ([`Rigid3::planar`]) ist das genau
    /// [`Sketch::apply_transform`] bzw. [`Sketch::copy_transformed`]. Sonst:
    ///
    /// - Bewegte Punkte werden 3D-Punkte (beim Drehen auch alle Punkte der Kreise, Bögen und
    ///   Ellipsen der Menge), und Kreise, Bögen, Ellipsen und Text bekommen ihre gedrehte Normale.
    ///   Linien und Splines folgen ihren Punkten. Was 3D ist, bleibt 3D, auch zurück in der Ebene.
    /// - Abhängigkeiten, die es an 3D-Geometrie nicht gibt, entfallen (wie beim Einbeziehen, auch
    ///   waagerechte und senkrechte Maße), ebenso, was danach nicht mehr gilt und nach außen bindet
    ///   oder vorher galt, und Tangenten zwischen nun gekippten Ebenen. Sie zählen in
    ///   `removed_constraints`. Abstand, Radius, Durchmesser und Winkel rechnen danach in ihrer
    ///   3D-Fassung (`converted_dimensions`).
    /// - Kopieren legt erst eine Kopie an Ort und Stelle an (mit den inneren Abhängigkeiten) und
    ///   bewegt dann sie; das Original bleibt, wie es ist.
    ///
    /// Kein Solver. Bei einem Fehler bleibt die Skizze unverändert.
    pub fn move_rigid3(
        &mut self,
        set: &TransformSet,
        m: &Rigid3,
        copy: bool,
    ) -> Result<MoveReport, SketchError> {
        if !m.is_finite() || m.rot.length() <= f64::EPSILON {
            return Err(SketchError::Degenerate(tr!("sketch-err-value-finite")));
        }
        if let Some(s) = m.planar(&self.tol) {
            if copy {
                let map = self.copy_transformed(set, &s)?;
                return Ok(MoveReport {
                    copy: Some(map),
                    ..MoveReport::default()
                });
            }
            let report = self.apply_transform(set, &s).map_err(block_error)?;
            return Ok(MoveReport {
                removed_constraints: report.dropped.len(),
                split: report.split,
                ..MoveReport::default()
            });
        }
        let m = Rigid3 {
            rot: m.rot.normalize(),
            ..*m
        };
        let mut out = self.clone();
        let mut copied = None;
        let set = if copy {
            let full = self.transform_set(&set.curves, &set.points);
            let map = out.copy_transformed(&full, &Similarity::identity(m.pivot.truncate()))?;
            let moving = TransformSet {
                points: full
                    .points
                    .iter()
                    .filter_map(|p| map.points.get(p))
                    .copied()
                    .collect(),
                curves: full
                    .curves
                    .iter()
                    .filter_map(|c| map.curves.get(c))
                    .copied()
                    .collect(),
            };
            copied = Some(map);
            moving
        } else {
            set.clone()
        };
        let mut report = out.move_3d(&set, &m)?;
        report.copy = copied;
        *self = out;
        Ok(report)
    }

    /// Der Hauptteil von [`Sketch::move_rigid3`] außerhalb der Ebene, auf einer Kopie der Skizze
    /// (die Aufrufer übernehmen sie nur bei `Ok`).
    fn move_3d(&mut self, set: &TransformSet, m: &Rigid3) -> Result<MoveReport, SketchError> {
        use ConstraintKind::*;
        let fixed = || SketchError::InvalidInput(tr!("sketch-move-fixed"));
        let far = || SketchError::Degenerate(tr!("sketch-move-degenerate"));
        let set = self.transform_set(&set.curves, &set.points);
        if set.points.is_empty() {
            return Err(SketchError::InvalidInput(tr!("sketch-nothing-selected")));
        }
        let (instance_curves, instance_points) = self.instance_parts();
        if set.curves.iter().any(|c| instance_curves.contains(c))
            || set.points.iter().any(|p| instance_points.contains(p))
        {
            return Err(block_error(TransformBlock::PatternInstance));
        }
        let texts = self.texts_in_set(&set);
        // Text auf einem Pfad bleibt in der Skizzenebene (Task 13).
        if texts
            .iter()
            .any(|&t| matches!(self.texts[t].placement, TextPlacement::Path { .. }))
        {
            return Err(SketchError::InvalidInput(tr!("sketch-err-text-path-3d")));
        }
        let glyph_points: HashSet<PointId> = texts
            .iter()
            .flat_map(|&t| self.texts[t].points.iter().copied())
            .collect();
        let glyphs = self.glyph_curves();
        let rotating = m.rotates(&self.tol);
        let held = self.held_points();
        let planar: Vec<CurveId> = set
            .curves
            .iter()
            .copied()
            .filter(|&c| {
                !glyphs.contains(&c) && crate::constraint3d::is_planar_kind(&self.curves[c].kind)
            })
            .collect();
        // Feste Kurven drehen ihre Ebene nicht: festgehaltene und einbezogene (deren Halbachsen
        // `EllipseAxes3` hält).
        let pinned_axes = |c: CurveId| {
            self.internal_constraints()
                .any(|(_, d)| matches!(d.kind, EllipseAxes3(x, ..) if x == c))
        };
        for &c in &set.curves {
            if matches!(self.curves[c].kind, CurveKind::FixedSpline { .. })
                || (rotating
                    && planar.contains(&c)
                    && (self.is_held_curve(c, &held) || pinned_axes(c)))
            {
                return Err(fixed());
            }
        }
        let mut targets = Vec::with_capacity(set.points.len());
        for &p in &set.points {
            if glyph_points.contains(&p) {
                continue;
            }
            let (from, to) = (self.pos3(p), m.apply(self.pos3(p)));
            if held.contains(&p) {
                if from.distance(to) > self.tol.linear {
                    return Err(fixed());
                }
            } else if from != to {
                if !to.is_finite() || to.abs().max_element() > MAX_COORD {
                    return Err(far());
                }
                targets.push((p, to));
            }
        }
        // Die Buchstabenpunkte gehen starr mit (Gesamtprüfung I2): Das Neusetzen schreibt sie
        // danach an dieselbe Stelle; ein Text ohne gesetzte Form (Schrift fehlt) oder ohne Lage
        // bleibt so trotzdem ganz in seiner Ebene.
        let mut glyph_targets = Vec::with_capacity(glyph_points.len());
        for &p in &glyph_points {
            let to = m.apply(self.pos3(p));
            if !to.is_finite() || to.abs().max_element() > MAX_COORD {
                return Err(far());
            }
            glyph_targets.push((p, to));
        }
        if targets.is_empty() && !(rotating && !(planar.is_empty() && texts.is_empty())) {
            return Err(block_error(TransformBlock::NothingToDo));
        }
        let scope = Scope {
            points: set.points.iter().copied().collect(),
            curves: set.curves.iter().copied().collect(),
            moved: targets.iter().map(|t| t.0).collect(),
        };
        let severed = self.severed_copy_bindings(&scope);
        let held_before: HashSet<ConstraintId> = self
            .constraints()
            .filter(|&(id, d)| scope.touches(&d.kind) && self.satisfied(id, SETTLE_FACTOR))
            .map(|(id, _)| id)
            .collect();
        // Maße mit 3D-Fassung, die bisher in der Ebene rechnen.
        let flat_dims: Vec<ConstraintId> = self
            .constraints()
            .filter(|(_, d)| {
                matches!(d.kind, Distance(..) | Radius(..) | Diameter(..) | Angle(..))
                    && scope.touches(&d.kind)
                    && !self.constraint_is_3d(&d.kind)
            })
            .map(|(id, _)| id)
            .collect();
        let mut report = MoveReport {
            split: self.split_from_outsiders(&scope).map_err(block_error)?,
            ..MoveReport::default()
        };
        // Anheben: die bewegten Punkte, beim Drehen auch die Punkte der ebenen Kurven (sie
        // bekommen eine eigene Ebene). Die Punkte der Buchstaben folgen unten starr.
        let mut seeds: Vec<PointId> = targets.iter().map(|t| t.0).collect();
        if rotating {
            for &c in &planar {
                seeds.extend(self.curves[c].kind.points());
            }
            for &t in &texts {
                if let TextPlacement::Box { corners, .. } = self.texts[t].placement {
                    seeds.extend(corners);
                }
            }
        }
        seeds.retain(|p| !glyph_points.contains(p));
        report.removed_constraints += self.lift_for_include(&seeds, None)?;
        for &(p, to) in &targets {
            let d = &self.points[p];
            let (x, y, z) = (d.x, d.y, d.z.ok_or_else(far)?);
            self.vars[x] = to.x;
            self.vars[y] = to.y;
            self.vars[z] = to.z;
        }
        for &(p, to) in &glyph_targets {
            self.promote_to_3d(p);
            let d = &self.points[p];
            let (x, y, z) = (d.x, d.y, d.z.ok_or_else(far)?);
            self.vars[x] = to.x;
            self.vars[y] = to.y;
            self.vars[z] = to.z;
        }
        if rotating {
            self.turn_planes(&planar, m);
        }
        self.move_texts3(&texts, m, rotating);
        self.refresh_patterns();
        self.refresh_texts();
        // Die Abhängigkeiten an Bewegtem; nur anzeigende und ausgeschaltete bleiben.
        let touched: Vec<ConstraintId> = self
            .constraints()
            .filter(|(_, d)| scope.touches(&d.kind))
            .map(|(id, _)| id)
            .collect();
        for id in touched {
            let kind = self.constraints[id].kind.clone();
            let inner = scope.holds_all(&kind);
            if !inner
                && self.needs_extra(&kind)
                && let (Some(var), Some(t)) = (self.constraints[id].extra, self.extra_start(&kind))
            {
                self.vars[var] = t;
            }
            if !is_active(&self.constraints[id]) {
                continue;
            }
            if severed.contains(&id) || self.tangent_planes_apart(&kind) {
                self.drop_constraint(id);
                report.removed_constraints += 1;
                continue;
            }
            if self.satisfied(id, SETTLE_FACTOR) {
                continue;
            }
            if is_position_binding(&kind) && self.binds_to_held(&kind, &held) {
                return Err(fixed());
            }
            if inner && !held_before.contains(&id) {
                continue;
            }
            self.drop_constraint(id);
            report.removed_constraints += 1;
        }
        report.converted_dimensions = flat_dims
            .iter()
            .filter(|&&id| {
                self.constraint(id)
                    .is_some_and(|d| self.constraint_is_3d(&d.kind))
            })
            .count();
        self.touch_structure();
        Ok(report)
    }

    /// Dreht die Ebenen der (angehobenen) Kreise, Bögen und Ellipsen `curves` mit `m`: Normale
    /// und große Halbachse (geteilte Halbachsen nur einmal). Ohne eigene Normale galt die
    /// Skizzennormale; Bögen und Ellipsen bekommen dann ihr `InPlane`, Ellipsen ihr `az`.
    fn turn_planes(&mut self, curves: &[CurveId], m: &Rigid3) {
        let mut axes: HashMap<usize, usize> = HashMap::new();
        for &c in curves {
            let n = self.curves[c].normal.unwrap_or(DVec3::Z);
            let had_plane = self.curves[c].normal.is_some();
            let mut kind = self.curves[c].kind.clone();
            if let CurveKind::Ellipse { ax, ay, az, .. }
            | CurveKind::EllipticArc { ax, ay, az, .. } = &mut kind
            {
                match axes.get(ax) {
                    Some(&z) => *az = Some(z),
                    None => {
                        let a = DVec3::new(
                            self.vars[*ax],
                            self.vars[*ay],
                            az.map_or(0.0, |i| self.vars[i]),
                        );
                        let b = m.turn(a);
                        let z = match *az {
                            Some(i) => i,
                            None => self.new_var(0.0),
                        };
                        self.vars[*ax] = b.x;
                        self.vars[*ay] = b.y;
                        self.vars[z] = b.z;
                        *az = Some(z);
                        axes.insert(*ax, z);
                    }
                }
            }
            if kind != self.curves[c].kind {
                self.set_curve_kind(c, kind);
            }
            self.curves[c].normal = Some(m.turn(n).normalize());
            let wants_plane = !matches!(self.curves[c].kind, CurveKind::Circle { .. });
            let has_plane = self
                .internal_constraints()
                .any(|(_, d)| d.kind == ConstraintKind::InPlane(c));
            if wants_plane && !had_plane && !has_plane {
                self.push_internal(ConstraintKind::InPlane(c));
            }
        }
        self.touch_structure();
    }

    /// Die Texte `texts` nach einer Bewegung im Raum: Ein Text in der Skizzenebene bekommt die
    /// Skizzennormale als eigene Ebene samt `InPlane` an den Rahmenlinien (wie
    /// [`Sketch::add_text_box3`]), beim Drehen dreht die Normale mit. Ein internes Waagerecht,
    /// Senkrecht oder Entlang Z des Rahmens, das danach nicht mehr gilt, entfällt (wie beim
    /// Drehen in der Ebene).
    fn move_texts3(&mut self, texts: &[TextId], m: &Rigid3, rotating: bool) {
        for &t in texts {
            let TextPlacement::Box { lines, .. } = self.texts[t].placement else {
                continue;
            };
            let old = self.texts[t].normal;
            let n = old.unwrap_or(DVec3::Z);
            let n = if rotating { m.turn(n).normalize() } else { n };
            self.texts[t].normal = Some(n);
            if old.is_none() {
                for &line in &lines[..3] {
                    self.push_internal(ConstraintKind::InPlane(line));
                }
            }
            let level: Vec<ConstraintId> = self
                .internal_constraints()
                .filter(|(_, k)| {
                    matches!(k.kind, ConstraintKind::Horizontal(LineOrPoints::Line(l))
                        | ConstraintKind::Vertical(LineOrPoints::Line(l))
                        | ConstraintKind::AlongZ(LineOrPoints::Line(l)) if lines.contains(&l))
                })
                .map(|(id, _)| id)
                .collect();
            for id in level {
                if !self.satisfied(id, SETTLE_FACTOR) {
                    self.drop_constraint(id);
                }
            }
            self.touch_structure();
        }
    }

    /// Eine Kopie der Kurve `c` auf den abgebildeten Punkten. `sizes` bildet die Variablen der
    /// Größe ab, damit Bögen derselben Ellipse auch in der Kopie dieselben Halbachsen teilen.
    fn copy_curve_transformed(
        &mut self,
        c: CurveId,
        points: &HashMap<PointId, PointId>,
        sizes: &mut HashMap<usize, usize>,
        s: &Similarity,
    ) -> Result<CurveId, SketchError> {
        let (construction, centerline) = (self.curves[c].construction, self.curves[c].centerline);
        let comb = self.curves[c].comb;
        // Eine Kurve in 3D behält ihre Ebene, um z mitgedreht (Skizzen 10).
        let normal = self.curves[c].normal.map(|n| s.turn(n));
        let id = match self.curves[c].kind.clone() {
            CurveKind::Line { a, b } => self.add_line(points[&a], points[&b])?,
            CurveKind::Circle { center, radius } => {
                let id = self.add_circle(points[&center], self.vars[radius] * s.factor)?;
                self.curves[id].normal = normal;
                id
            }
            CurveKind::Arc { center, start, end } => match normal {
                Some(n) => self.add_arc3(points[&center], points[&start], points[&end], n)?,
                None => self.add_arc(points[&center], points[&start], points[&end])?,
            },
            CurveKind::Spline { points: list, .. } => {
                let mapped: Vec<PointId> = list.iter().map(|p| points[p]).collect();
                self.add_spline(&mapped)?
            }
            CurveKind::ControlSpline {
                points: list,
                degree,
                knots,
            } => {
                let mapped: Vec<PointId> = list.iter().map(|p| points[p]).collect();
                self.add_control_spline_as(&mapped, degree, knots)?
            }
            CurveKind::Conic {
                start,
                end,
                apex,
                rho,
            } => self.add_conic_as(points[&start], points[&end], points[&apex], rho)?,
            CurveKind::FixedSpline { .. } => {
                return Err(SketchError::WrongType(tr!("sketch-fixed-spline-no-copy")));
            }
            ref kind @ (CurveKind::Ellipse {
                ax, ay, az, minor, ..
            }
            | CurveKind::EllipticArc {
                ax, ay, az, minor, ..
            }) => {
                if !sizes.contains_key(&ax) {
                    let (major, small) = self.scaled_axes(ax, ay, minor, s)?;
                    sizes.insert(ax, self.new_var(major.x));
                    sizes.insert(ay, self.new_var(major.y));
                    sizes.insert(minor, self.new_var(small));
                }
                // Ohne Normale entsteht die Kopie in der Skizzenebene, also ohne `az`; mit Normale
                // behält sie ihren z-Anteil (Drehen um z ändert ihn nicht, Skalieren geht in 3D
                // nicht).
                let mut flat = kind.clone();
                match az {
                    Some(z) if normal.is_some() => {
                        let value = self.vars[z];
                        sizes.entry(z).or_insert_with(|| self.new_var(value));
                    }
                    _ => {
                        if let CurveKind::Ellipse { az, .. } | CurveKind::EllipticArc { az, .. } =
                            &mut flat
                        {
                            *az = None;
                        }
                    }
                }
                let mapped = map_curve(&flat, |p| points[&p], |v| sizes[&v]);
                let id = match mapped {
                    CurveKind::EllipticArc {
                        center,
                        ax,
                        ay,
                        az,
                        minor,
                        start,
                        end,
                    } => {
                        self.add_elliptic_arc_vars(center, (ax, ay, az), minor, start, end)?
                            .0
                    }
                    other => self.push_curve(other),
                };
                if normal.is_some() {
                    self.curves[id].normal = normal;
                    self.push_internal(ConstraintKind::InPlane(id));
                }
                id
            }
        };
        self.curves[id].construction = construction;
        self.curves[id].centerline = centerline;
        self.curves[id].comb = comb;
        Ok(id)
    }

    /// Legt zu jedem Text in `texts` einen zweiten an (Spec E): gleiche Werte, die Höhe mal Maßstab,
    /// die Lage auf den Kopien von Rahmen bzw. Pfad, die kopierten Buchstaben als seine Kurven und
    /// Punkte in derselben Reihenfolge und dieselbe gesetzte Form. So steht die Kopie sofort da,
    /// wo das nächste Neusetzen sie hinschriebe, auch bei einem noch ungesetzten Text. Der kopierte
    /// Rahmen bekommt die internen Abhängigkeiten des Originals, das Waagerecht der Grundlinie nur
    /// ohne Drehung.
    fn copy_texts(
        &mut self,
        texts: &[TextId],
        points: &HashMap<PointId, PointId>,
        curves: &HashMap<CurveId, CurveId>,
        s: &Similarity,
    ) -> Result<HashMap<TextId, TextId>, SketchError> {
        let turning = wrap_pi(s.angle).abs() > self.tol.angular;
        let point = |p: &PointId| points.get(p).copied().ok_or(SketchError::UnknownObject);
        let curve = |c: &CurveId| curves.get(c).copied().ok_or(SketchError::UnknownObject);
        let mut out = HashMap::new();
        for &t in texts {
            let d = self.texts[t].clone();
            let height = d.height * s.factor;
            if !(height.is_finite() && height > 0.0) {
                return Err(SketchError::InvalidInput(tr!("sketch-text-height")));
            }
            let placement = match d.placement {
                TextPlacement::Box {
                    lines,
                    corners,
                    h,
                    v,
                } => {
                    let new_lines = [
                        curve(&lines[0])?,
                        curve(&lines[1])?,
                        curve(&lines[2])?,
                        curve(&lines[3])?,
                    ];
                    let new_corners = [
                        point(&corners[0])?,
                        point(&corners[1])?,
                        point(&corners[2])?,
                        point(&corners[3])?,
                    ];
                    let internal: Vec<ConstraintKind> = self
                        .internal_constraints()
                        .map(|(_, k)| &k.kind)
                        .filter(|k| {
                            let cs = k.curves();
                            !cs.is_empty()
                                && cs.iter().all(|c| lines.contains(c))
                                && k.points().iter().all(|p| corners.contains(p))
                                && !(turning && is_axis_binding(k))
                        })
                        .cloned()
                        .collect();
                    for kind in internal {
                        let mapped = map_ids(&kind, |p| points[&p], |c| curves[&c]);
                        self.push_internal(mapped);
                    }
                    TextPlacement::Box {
                        lines: new_lines,
                        corners: new_corners,
                        h,
                        v,
                    }
                }
                TextPlacement::Path {
                    path,
                    above,
                    fit,
                    h,
                } => TextPlacement::Path {
                    path: curve(&path)?,
                    above,
                    fit,
                    h,
                },
            };
            let copy = TextData {
                height,
                placement,
                normal: d.normal.map(|n| s.turn(n)),
                curves: d.curves.iter().map(curve).collect::<Result<_, _>>()?,
                points: d.points.iter().map(point).collect::<Result<_, _>>()?,
                ..d
            };
            out.insert(t, self.texts.push(copy));
        }
        if !out.is_empty() {
            self.touch_structure();
        }
        Ok(out)
    }

    /// Die Halbachsen nach der Bewegung, geprüft. Die Rollen dürfen vertauscht sein (siehe
    /// `check_axis_pair`), darum nicht über `check_ellipse_axes`.
    fn scaled_axes(
        &self,
        ax: usize,
        ay: usize,
        minor: usize,
        s: &Similarity,
    ) -> Result<(DVec2, f64), SketchError> {
        let major = s.linear(DVec2::new(self.vars[ax], self.vars[ay]));
        let small = self.vars[minor] * s.factor;
        crate::model::finite(major.x, || tr!("sketch-err-what-major"))?;
        crate::model::finite(major.y, || tr!("sketch-err-what-major"))?;
        crate::model::finite(small, || tr!("sketch-err-what-minor"))?;
        if major.length() <= self.tol.linear {
            return Err(SketchError::Degenerate(tr!("sketch-major-axis-positive")));
        }
        if small <= self.tol.linear {
            return Err(SketchError::Degenerate(tr!("sketch-err-minor-range")));
        }
        Ok((major, small))
    }

    /// Legt die Abhängigkeiten an, deren Bezüge ganz in der Menge liegen, außer `Fixed`.
    fn copy_inner_constraints(
        &mut self,
        points: &HashMap<PointId, PointId>,
        curves: &HashMap<CurveId, CurveId>,
        s: &Similarity,
    ) -> Result<HashMap<ConstraintId, ConstraintId>, SketchError> {
        let inner: Vec<(ConstraintId, ConstraintData, bool)> = self
            .constraints()
            .filter(|(_, d)| {
                // Musterwerte gehören zum Muster, nicht zur Geometrie: Kopien sind lose.
                !matches!(
                    d.kind,
                    ConstraintKind::Fixed(..)
                        | ConstraintKind::Fixed3(..)
                        | ConstraintKind::PatternValue(..)
                ) && d.kind.points().iter().all(|p| points.contains_key(p))
                    && d.kind.curves().iter().all(|c| curves.contains_key(c))
            })
            .map(|(id, d)| (id, d.clone(), self.satisfied(id, SETTLE_FACTOR)))
            .collect();
        let mut map = HashMap::new();
        for (id, d, held_before) in inner {
            let (value, _) = transformed_value(&d.kind, s);
            let kind = map_ids(&value, |p| points[&p], |c| curves[&c]);
            let new = self.add_constraint_supplement(kind, d.supplement)?;
            if let (Some(from), Some(to)) = (d.extra, self.constraints[new].extra) {
                self.vars[to] = self.vars[from];
            }
            self.constraints[new].driven = d.driven;
            self.constraints[new].enabled = d.enabled;
            let gone = is_active(&d)
                && !self.satisfied(new, SETTLE_FACTOR)
                && (held_before || is_axis_binding(&d.kind));
            if gone {
                self.remove_constraint(new)?;
            } else {
                map.insert(id, new);
            }
        }
        self.touch_structure();
        Ok(map)
    }

    /// Die Ähnlichkeit ohne Anteile, die nichts bewirken dürfen. Eine Abhängigkeit gilt bis
    /// `SETTLE_FACTOR` mal der Toleranz als erfüllt und bleibt; das Lösen danach zöge die Punkte um
    /// diesen Rest zurück, und das Ergebnis wiche von der Vorschau ab. Darum zählt ein Winkel unter
    /// dieser Grenze als null, gemessen als Winkel und als Weg des fernsten Punkts (eine Ellipse dreht
    /// auch um ihre Mitte, darum zählt ihre große Halbachse dazu), und eine Verschiebung ebenso.
    fn settled(&self, set: &TransformSet, s: &Similarity) -> Similarity {
        let limit = SETTLE_FACTOR * self.tol.linear;
        let lever = set
            .points
            .iter()
            .map(|&p| self.pos(p).distance(s.base))
            .fold(0.0, f64::max);
        let axis = set
            .curves
            .iter()
            .map(|&c| match self.curves[c].kind {
                CurveKind::Ellipse { ax, ay, .. } | CurveKind::EllipticArc { ax, ay, .. } => {
                    DVec2::new(self.vars[ax], self.vars[ay]).length()
                }
                // Ein Kreis mit eigener Ebene (Skizzen 10) dreht um seine Mitte mit.
                CurveKind::Circle { radius, .. } if self.curves[c].normal.is_some() => {
                    self.vars[radius]
                }
                _ => 0.0,
            })
            .fold(0.0, f64::max);
        let lever = lever + axis;
        let turn = wrap_pi(s.angle).abs();
        let mut out = *s;
        if turn <= SETTLE_FACTOR * self.tol.angular || turn * lever * s.factor <= limit {
            out.angle = 0.0;
        }
        if s.offset.length() <= limit {
            out.offset = DVec2::ZERO;
        }
        out
    }

    /// Die eine Rechnung hinter Prüfung, Vorschau und Ergebnis: die bewegte Skizze als Kopie.
    fn transformed(
        &self,
        set: &TransformSet,
        s: &Similarity,
    ) -> Result<(Sketch, TransformReport), TransformBlock> {
        if !s.is_finite() || s.factor <= 0.0 {
            return Err(TransformBlock::Degenerate);
        }
        let set = self.transform_set(&set.curves, &set.points);
        let (instance_curves, instance_points) = self.instance_parts();
        if set.curves.iter().any(|c| instance_curves.contains(c))
            || set.points.iter().any(|p| instance_points.contains(p))
        {
            return Err(TransformBlock::PatternInstance);
        }
        if self.scales_3d(&set, s) {
            return Err(TransformBlock::NotIn3d);
        }
        let texts = self.texts_in_set(&set);
        if self.text_off_its_path(&set, &texts) {
            return Err(TransformBlock::TextOnPath);
        }
        let s = &self.settled(&set, s);
        if set.points.is_empty() || s.is_identity(self.tol.linear) {
            return Err(TransformBlock::NothingToDo);
        }
        let held = self.held_points();
        let turning = wrap_pi(s.angle).abs() > self.tol.angular;
        let stretching = s.factor != 1.0;
        // Eine feste Kurve behält auch Größe und Lage ihrer Achsen, wenn die Mitte stehen bleibt.
        for &c in &set.curves {
            // Der feste Spline bleibt immer stehen, auch ohne `Fixed` an seinen Enden oder wenn sein
            // einziger Punkt die Mitte des Maßstabs ist.
            if let CurveKind::FixedSpline { start, .. } = self.curves[c].kind {
                return Err(TransformBlock::FixedInSet(start));
            }
            if !self.is_held_curve(c, &held) {
                continue;
            }
            match self.curves[c].kind {
                CurveKind::Circle { center, .. } if stretching => {
                    return Err(TransformBlock::FixedInSet(center));
                }
                CurveKind::Ellipse { center, .. } | CurveKind::EllipticArc { center, .. }
                    if stretching || turning =>
                {
                    return Err(TransformBlock::FixedInSet(center));
                }
                _ => {}
            }
        }
        let mut targets = Vec::with_capacity(set.points.len());
        for &p in &set.points {
            let (from, to) = (self.pos(p), s.apply(self.pos(p)));
            if held.contains(&p) {
                if from.distance(to) > self.tol.linear {
                    return Err(TransformBlock::FixedInSet(p));
                }
            } else if from != to {
                if to.abs().max_element() > MAX_COORD {
                    return Err(TransformBlock::Degenerate);
                }
                targets.push((p, to));
            }
        }
        // Eine Kurve mit eigener Ebene (Skizzen 10) dreht beim Drehen ihre Normale, auch wenn ihre
        // Mitte stehen bleibt.
        let resizes = set.curves.iter().any(|&c| match self.curves[c].kind {
            CurveKind::Circle { .. } => stretching || (turning && self.curves[c].normal.is_some()),
            CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => stretching || turning,
            _ => false,
        });
        if targets.is_empty() && !resizes {
            return Err(TransformBlock::NothingToDo);
        }
        let scope = Scope {
            points: set.points.iter().copied().collect(),
            curves: set.curves.iter().copied().collect(),
            moved: targets.iter().map(|t| t.0).collect(),
        };
        let severed = self.severed_copy_bindings(&scope);
        let held_before: HashSet<ConstraintId> = self
            .constraints()
            .filter(|&(id, d)| scope.touches(&d.kind) && self.satisfied(id, SETTLE_FACTOR))
            .map(|(id, _)| id)
            .collect();

        let mut out = self.clone();
        let split = out.split_from_outsiders(&scope)?;
        out.write_positions(&set.curves, &targets, s)?;
        // Kurven in 3D (Skizzen 10): Ihre Ebene dreht mit, die Normale um z.
        if turning {
            for &c in &set.curves {
                if let Some(n) = out.curves[c].normal {
                    out.curves[c].normal = Some(s.turn(n));
                }
            }
        }
        // Muster: Abstände mit dem Maßstab, wenn die ganze Vorlage samt Richtungslinien mitgeht;
        // dann die Exemplare an die neue Lage, damit Bindungen an ihnen richtig geprüft werden.
        let mut scaled_spacings = Vec::new();
        if stretching {
            let held_line = |l: CurveId| self.is_held_curve(l, &held);
            scaled_spacings = out
                .scale_pattern_spacings(&scope.curves, held_line, s.factor)
                .ok_or(TransformBlock::Degenerate)?;
        }
        out.refresh_patterns();
        // Text: Höhe mit dem Maßstab, das interne Waagerecht des Rahmens entfällt beim Drehen
        // (sonst drehte das Lösen ihn zurück); dann die Buchstaben an die neue Lage.
        out.move_texts(&texts, s, turning)?;
        out.refresh_texts();
        let mut report = out.settle_after_transform(&scope, &held, &held_before, &severed, s)?;
        report.scaled.extend(scaled_spacings);
        report.split = split;
        // Buchstaben sind erzeugt, nicht gezeichnet: Ein winziges Stück einer Schrift sperrt die
        // Bewegung nicht. Der Rahmen zählt wie jede Linie.
        let glyphs = self.glyph_curves();
        if set
            .curves
            .iter()
            .any(|&c| !glyphs.contains(&c) && out.collapsed(c))
        {
            return Err(TransformBlock::Degenerate);
        }
        out.touch_structure();
        Ok((out, report))
    }

    /// Die Texte `texts` nach der Bewegung mit dem Maßstab von `s`: Höhe mal Maßstab; beim Drehen
    /// entfällt das interne Waagerecht der Grundlinie, wie bei [`Sketch::set_text_angle`].
    fn move_texts(
        &mut self,
        texts: &[TextId],
        s: &Similarity,
        turning: bool,
    ) -> Result<(), TransformBlock> {
        for &t in texts {
            let height = self.texts[t].height * s.factor;
            if !(height.is_finite() && height > 0.0) {
                return Err(TransformBlock::Degenerate);
            }
            self.texts[t].height = height;
            // Ein Text in 3D: Die Ebene dreht mit (die internen `InPlane` lesen die Normale).
            if let Some(n) = self.texts[t].normal {
                self.texts[t].normal = Some(s.turn(n));
            }
            let TextPlacement::Box { lines, .. } = self.texts[t].placement else {
                continue;
            };
            if !turning {
                continue;
            }
            let level: Vec<ConstraintId> = self
                .internal_constraints()
                .filter(|(_, k)| {
                    matches!(k.kind, ConstraintKind::Horizontal(LineOrPoints::Line(l))
                        | ConstraintKind::Vertical(LineOrPoints::Line(l)) if l == lines[0])
                })
                .map(|(id, _)| id)
                .collect();
            for id in level {
                self.drop_constraint(id);
            }
        }
        Ok(())
    }

    /// Fremde Kurven an bewegten Punkten oder geteilten Halbachsen bekommen eigene an der alten
    /// Lage. Eine Abhängigkeit, die an einem getrennten Punkt und sonst nur an Fremdem hängt, gehört
    /// zur fremden Seite und wandert mit auf den neuen Punkt. Liefert die Zahl der getrennten Punkte.
    fn split_from_outsiders(&mut self, scope: &Scope) -> Result<usize, TransformBlock> {
        let mut shared_vars = HashSet::new();
        for &c in &scope.curves {
            shared_vars.extend(size_vars(&self.curves[c].kind));
        }
        let outsiders: Vec<CurveId> = self
            .curves()
            .map(|(c, _)| c)
            .filter(|c| !scope.curves.contains(c))
            .collect();
        let mut new_points: HashMap<PointId, PointId> = HashMap::new();
        let mut new_vars: HashMap<usize, usize> = HashMap::new();
        for c in outsiders {
            let kind = self.curves[c].kind.clone();
            for p in kind.points() {
                if scope.moved.contains(&p) && !new_points.contains_key(&p) {
                    let copy = self
                        .add_point_like(p, self.pos(p))
                        .map_err(|_| TransformBlock::Degenerate)?;
                    new_points.insert(p, copy);
                }
            }
            for v in size_vars(&kind) {
                if shared_vars.contains(&v) && !new_vars.contains_key(&v) {
                    new_vars.insert(v, self.new_var(self.vars[v]));
                }
            }
            let mapped = map_curve(
                &kind,
                |p| new_points.get(&p).copied().unwrap_or(p),
                |v| new_vars.get(&v).copied().unwrap_or(v),
            );
            if mapped != kind {
                self.set_curve_kind(c, mapped);
            }
        }
        if new_points.is_empty() {
            return Ok(0);
        }
        let rehung: Vec<(ConstraintId, ConstraintKind)> = self
            .constraints()
            .filter_map(|(id, d)| {
                let (points, curves) = (d.kind.points(), d.kind.curves());
                let total = points.len() + curves.len();
                let inside = points.iter().filter(|p| scope.points.contains(p)).count()
                    + curves.iter().filter(|c| scope.curves.contains(c)).count();
                let split = points.iter().any(|p| new_points.contains_key(p));
                (split && total >= 2 && inside == 1).then(|| {
                    let kind =
                        map_ids(&d.kind, |p| new_points.get(&p).copied().unwrap_or(p), |c| c);
                    (id, kind)
                })
            })
            .collect();
        for (id, kind) in rehung {
            self.constraints[id].kind = kind;
        }
        Ok(new_points.len())
    }

    /// Schreibt die neuen Lagen, Radien und Halbachsen. Geteilte Halbachsen nur einmal.
    fn write_positions(
        &mut self,
        curves: &[CurveId],
        targets: &[(PointId, DVec2)],
        s: &Similarity,
    ) -> Result<(), TransformBlock> {
        for &(p, to) in targets {
            let (x, y) = (self.points[p].x, self.points[p].y);
            self.vars[x] = to.x;
            self.vars[y] = to.y;
        }
        let mut done = HashSet::new();
        for &c in curves {
            match self.curves[c].kind {
                CurveKind::Circle { radius, .. } if done.insert(radius) => {
                    self.vars[radius] *= s.factor;
                    if self.vars[radius] > MAX_COORD {
                        return Err(TransformBlock::Degenerate);
                    }
                }
                CurveKind::Ellipse { ax, ay, minor, .. }
                | CurveKind::EllipticArc { ax, ay, minor, .. }
                    if done.insert(ax) =>
                {
                    let major = s.linear(DVec2::new(self.vars[ax], self.vars[ay]));
                    self.vars[ax] = major.x;
                    self.vars[ay] = major.y;
                    self.vars[minor] *= s.factor;
                    if major.length() > MAX_COORD || self.vars[minor] > MAX_COORD {
                        return Err(TransformBlock::Degenerate);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Passt die Maße an und entscheidet über jede Abhängigkeit an Bewegtem. Nur anzeigende und
    /// ausgeschaltete bewegen nichts und bleiben.
    fn settle_after_transform(
        &mut self,
        scope: &Scope,
        held: &HashSet<PointId>,
        held_before: &HashSet<ConstraintId>,
        severed: &HashSet<ConstraintId>,
        s: &Similarity,
    ) -> Result<TransformReport, TransformBlock> {
        let turning = wrap_pi(s.angle).abs() > self.tol.angular;
        let stretching = s.factor != 1.0;
        let mut report = TransformReport {
            dropped: Vec::new(),
            scaled: Vec::new(),
            split: 0,
        };
        let touched: Vec<ConstraintId> = self
            .constraints()
            .filter(|(_, d)| scope.touches(&d.kind))
            .map(|(id, _)| id)
            .collect();
        for id in touched {
            let kind = self.constraints[id].kind.clone();
            let inner = scope.holds_all(&kind);
            if inner {
                let (value, is_length) = transformed_value(&kind, s);
                if is_length && stretching {
                    if !length_in_range(&value, self.tol.linear) {
                        return Err(TransformBlock::Degenerate);
                    }
                    report.scaled.push(id);
                }
                self.constraints[id].kind = value;
            } else if self.needs_extra(&kind)
                && let (Some(var), Some(t)) = (self.constraints[id].extra, self.extra_start(&kind))
            {
                // Nur eine Seite hat sich bewegt: Die Stelle auf der Kurve ist neu zu suchen.
                self.vars[var] = t;
            }
            if !is_active(&self.constraints[id]) {
                continue;
            }
            // Eine Tangente zweier Bögen ohne gemeinsames Ende gibt es nur in parallelen Ebenen
            // (Skizzen 10); dreht eine Normale weg, entfällt sie.
            if severed.contains(&id)
                || (turning && is_axis_binding(&kind))
                || self.tangent_planes_apart(&kind)
            {
                self.drop_constraint(id);
                report.dropped.push(id);
                continue;
            }
            if self.satisfied(id, SETTLE_FACTOR) {
                continue;
            }
            if is_position_binding(&kind) && self.binds_to_held(&kind, held) {
                return Err(TransformBlock::BoundToFixed(id));
            }
            // Was schon vorher nicht galt, hat die Bewegung nicht gebrochen.
            if inner && !held_before.contains(&id) {
                continue;
            }
            self.drop_constraint(id);
            report.dropped.push(id);
        }
        Ok(report)
    }

    /// Punkte mit einem wirksamen `Fixed`.
    fn held_points(&self) -> HashSet<PointId> {
        self.constraints()
            .filter(|(_, d)| is_active(d))
            .filter_map(|(_, d)| match d.kind {
                ConstraintKind::Fixed(p, ..) | ConstraintKind::Fixed3(p, ..) => Some(p),
                _ => None,
            })
            .collect()
    }

    /// Festgehaltene Kurve: Alle Punkte haben `Fixed`, und auch die Größe liegt über Maße fest
    /// (Achshilfslinie, Kantenkopie). Ein Kreis mit fester Mitte und freiem Radius zählt nicht.
    fn is_held_curve(&self, c: CurveId, held: &HashSet<PointId>) -> bool {
        let kind = &self.curves[c].kind;
        if matches!(kind, CurveKind::FixedSpline { .. }) {
            return true;
        }
        if !kind.points().iter().all(|p| held.contains(p)) {
            return false;
        }
        let has = |test: fn(&ConstraintKind, CurveId) -> bool| {
            self.constraints()
                .any(|(_, d)| is_active(d) && test(&d.kind, c))
        };
        match kind {
            CurveKind::Circle { .. } => has(
                |k, c| matches!(*k, ConstraintKind::Radius(x, _) | ConstraintKind::Diameter(x, _) if x == c),
            ),
            CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. } => {
                has(|k, c| matches!(*k, ConstraintKind::EllipseMajor(x, _) if x == c))
            }
            _ => true,
        }
    }

    /// Hängt die Abhängigkeit an festgehaltener Geometrie?
    fn binds_to_held(&self, kind: &ConstraintKind, held: &HashSet<PointId>) -> bool {
        kind.points().iter().any(|p| held.contains(p))
            || kind.curves().iter().any(|&c| self.is_held_curve(c, held))
    }

    /// Ist die Kurve unter die Toleranz geschrumpft?
    fn collapsed(&self, c: CurveId) -> bool {
        let tol = self.tol.linear;
        // Im Raum gemessen: Eine Linie entlang Z ist in der Draufsicht kurz, aber nicht entartet.
        let close = |a: PointId, b: PointId| self.pos3(a).distance(self.pos3(b)) <= tol;
        match self.curves[c].kind {
            CurveKind::Line { a, b } => close(a, b),
            CurveKind::Circle { radius, .. } => self.vars[radius] <= tol,
            CurveKind::Arc { center, start, end } => close(center, start) || close(center, end),
            CurveKind::Ellipse { ax, ay, minor, .. } => self.axes_collapsed(ax, ay, minor),
            CurveKind::EllipticArc {
                center,
                ax,
                ay,
                minor,
                start,
                end,
                ..
            } => self.axes_collapsed(ax, ay, minor) || close(center, start) || close(center, end),
            CurveKind::Spline { ref points, .. } => points.windows(2).any(|w| close(w[0], w[1])),
            // Kontrollpunkte dürfen zusammenfallen; entartet ist erst die ganze Kurve in einem Punkt.
            CurveKind::ControlSpline { ref points, .. } => {
                points.windows(2).all(|w| close(w[0], w[1]))
            }
            CurveKind::Conic {
                start, end, apex, ..
            } => close(start, apex) && close(apex, end),
            // Bleibt, wie sie ist; bewegen geht ohnehin nicht.
            CurveKind::FixedSpline { .. } => false,
        }
    }

    fn axes_collapsed(&self, ax: usize, ay: usize, minor: usize) -> bool {
        let tol = self.tol.linear;
        DVec2::new(self.vars[ax], self.vars[ay]).length() <= tol || self.vars[minor] <= tol
    }
}

/// Die Menge im Aufbau: in Wahlreihenfolge, jedes Objekt einmal.
#[derive(Default)]
struct Collected {
    out: TransformSet,
    points: HashSet<PointId>,
    curves: HashSet<CurveId>,
}

impl Collected {
    fn add_curve(&mut self, sk: &Sketch, c: CurveId) {
        if self.curves.insert(c) {
            self.out.curves.push(c);
            for p in sk.curves[c].kind.points() {
                self.add_point(p);
            }
        }
    }

    fn add_point(&mut self, p: PointId) {
        if self.points.insert(p) {
            self.out.points.push(p);
        }
    }
}

/// Die Menge als Nachschlagetabellen.
struct Scope {
    points: HashSet<PointId>,
    curves: HashSet<CurveId>,
    /// Die Punkte, die sich wirklich bewegen.
    moved: HashSet<PointId>,
}

impl Scope {
    /// Hängt die Abhängigkeit an etwas, das sich bewegt?
    fn touches(&self, kind: &ConstraintKind) -> bool {
        kind.points().iter().any(|p| self.moved.contains(p))
            || kind.curves().iter().any(|c| self.curves.contains(c))
    }

    /// Liegen alle Bezüge in der Menge?
    fn holds_all(&self, kind: &ConstraintKind) -> bool {
        kind.points().iter().all(|p| self.points.contains(p))
            && kind.curves().iter().all(|c| self.curves.contains(c))
    }
}
