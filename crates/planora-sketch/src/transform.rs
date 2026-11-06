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

use crate::constraint::{ConstraintData, ConstraintId, ConstraintKind, LineOrPoints};
use crate::edit::SETTLE_FACTOR;
use crate::error::SketchError;
use crate::fillet::wrap_pi;
use crate::model::{CurveId, CurveKind, MAX_COORD, PointId, Sketch};
use crate::offset::ends_of;
use planora_i18n::tr;
use planora_math::DVec2;
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
        Parallel(a, b) => Parallel(c(*a), c(*b)),
        Perpendicular(a, b) => Perpendicular(c(*a), c(*b)),
        Tangent(a, b) => Tangent(c(*a), c(*b)),
        Equal(a, b) => Equal(c(*a), c(*b)),
        Fixed(a, x, y) => Fixed(p(*a), *x, *y),
        Symmetric(a, b, x) => Symmetric(p(*a), p(*b), c(*x)),
        Midpoint(a, x) => Midpoint(p(*a), c(*x)),
        Distance(a, b, v) => Distance(p(*a), p(*b), *v),
        DistancePointLine(a, x, v) => DistancePointLine(p(*a), c(*x), *v),
        Radius(x, v) => Radius(c(*x), *v),
        Diameter(x, v) => Diameter(c(*x), *v),
        Angle(a, b, v) => Angle(c(*a), c(*b), *v),
        ArcRadius(x) => ArcRadius(c(*x)),
        EllipseMajor(x, v) => EllipseMajor(c(*x), *v),
        EllipseMinor(x, v) => EllipseMinor(c(*x), *v),
        EllipseAngle(x, v) => EllipseAngle(c(*x), *v),
        EllipticArcEnds(x) => EllipticArcEnds(c(*x)),
        Collinear(a, b) => Collinear(c(*a), c(*b)),
        Smooth(a, pa, b, pb) => Smooth(c(*a), p(*pa), c(*b), p(*pb)),
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
        CurveKind::Ellipse { ax, ay, minor, .. } | CurveKind::EllipticArc { ax, ay, minor, .. } => {
            vec![ax, ay, minor]
        }
        _ => vec![],
    }
}

/// Die Art mit abgebildeten Punkten und Variablen.
fn map_curve(
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
            minor,
        } => CurveKind::Ellipse {
            center: p(*center),
            ax: v(*ax),
            ay: v(*ay),
            minor: v(*minor),
        },
        CurveKind::Spline { points } => CurveKind::Spline {
            points: points.iter().map(|&x| p(x)).collect(),
        },
        CurveKind::EllipticArc {
            center,
            ax,
            ay,
            minor,
            start,
            end,
        } => CurveKind::EllipticArc {
            center: p(*center),
            ax: v(*ax),
            ay: v(*ay),
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
    ///   (`rollers_of`). Die Kopie allein zieht weder Vorlage noch Rollkreis mit.
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
        loop {
            let before = set.out.points.len() + set.out.curves.len();
            let (helper_curves, helper_points) = self.helpers_of(&set);
            for c in helper_curves {
                set.add_curve(self, c);
            }
            for p in helper_points {
                set.add_point(p);
            }
            for c in self.rollers_of(&set) {
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
            if let ConstraintKind::Fixed(p, ..) = d.kind {
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
    /// und festen Ursprung.
    pub fn whole_sketch_set(&self) -> TransformSet {
        let held = self.held_points();
        let curves: Vec<CurveId> = self
            .curves()
            .map(|(c, _)| c)
            .filter(|&c| !self.is_held_curve(c, &held))
            .collect();
        let used: HashSet<PointId> = self.curves().flat_map(|(_, d)| d.kind.points()).collect();
        let points: Vec<PointId> = self
            .points()
            .map(|(p, _)| p)
            .filter(|p| !used.contains(p) && !held.contains(p))
            .collect();
        self.transform_set(&curves, &points)
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
    /// vertikal und was sonst an der Kopie nicht gilt, entfällt. Bei einem Fehler bleibt die Skizze
    /// unverändert.
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
            let constraints = sk.copy_inner_constraints(&points, &curves, s)?;
            Ok(CopyMap {
                points,
                curves,
                constraints,
            })
        })
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
        let id = match self.curves[c].kind.clone() {
            CurveKind::Line { a, b } => self.add_line(points[&a], points[&b])?,
            CurveKind::Circle { center, radius } => {
                self.add_circle(points[&center], self.vars[radius] * s.factor)?
            }
            CurveKind::Arc { center, start, end } => {
                self.add_arc(points[&center], points[&start], points[&end])?
            }
            CurveKind::Spline { points: list } => {
                let mapped: Vec<PointId> = list.iter().map(|p| points[p]).collect();
                self.add_spline(&mapped)?
            }
            ref kind @ (CurveKind::Ellipse { ax, ay, minor, .. }
            | CurveKind::EllipticArc { ax, ay, minor, .. }) => {
                if !sizes.contains_key(&ax) {
                    let (major, small) = self.scaled_axes(ax, ay, minor, s)?;
                    sizes.insert(ax, self.new_var(major.x));
                    sizes.insert(ay, self.new_var(major.y));
                    sizes.insert(minor, self.new_var(small));
                }
                let mapped = map_curve(kind, |p| points[&p], |v| sizes[&v]);
                match mapped {
                    CurveKind::EllipticArc {
                        center,
                        ax,
                        ay,
                        minor,
                        start,
                        end,
                    } => self.add_elliptic_arc_vars(center, ax, ay, minor, start, end),
                    other => self.push_curve(other),
                }
            }
        };
        self.curves[id].construction = construction;
        self.curves[id].centerline = centerline;
        Ok(id)
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
                    ConstraintKind::Fixed(..) | ConstraintKind::PatternValue(..)
                ) && d.kind.points().iter().all(|p| points.contains_key(p))
                    && d.kind.curves().iter().all(|c| curves.contains_key(c))
            })
            .map(|(id, d)| (id, d.clone(), self.satisfied(id, SETTLE_FACTOR)))
            .collect();
        let mut map = HashMap::new();
        for (id, d, held_before) in inner {
            let (value, _) = transformed_value(&d.kind, s);
            let kind = map_ids(&value, |p| points[&p], |c| curves[&c]);
            let new = self.add_constraint(kind)?;
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
        let s = &self.settled(&set, s);
        if set.points.is_empty() || s.is_identity(self.tol.linear) {
            return Err(TransformBlock::NothingToDo);
        }
        let held = self.held_points();
        let turning = wrap_pi(s.angle).abs() > self.tol.angular;
        let stretching = s.factor != 1.0;
        // Eine feste Kurve behält auch Größe und Lage ihrer Achsen, wenn die Mitte stehen bleibt.
        for &c in &set.curves {
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
        let resizes = set.curves.iter().any(|&c| match self.curves[c].kind {
            CurveKind::Circle { .. } => stretching,
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
        let mut report = out.settle_after_transform(&scope, &held, &held_before, &severed, s)?;
        report.split = split;
        if set.curves.iter().any(|&c| out.collapsed(c)) {
            return Err(TransformBlock::Degenerate);
        }
        out.touch_structure();
        Ok((out, report))
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
            if severed.contains(&id) || (turning && is_axis_binding(&kind)) {
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
                ConstraintKind::Fixed(p, ..) => Some(p),
                _ => None,
            })
            .collect()
    }

    /// Festgehaltene Kurve: Alle Punkte haben `Fixed`, und auch die Größe liegt über Maße fest
    /// (Achshilfslinie, Kantenkopie). Ein Kreis mit fester Mitte und freiem Radius zählt nicht.
    fn is_held_curve(&self, c: CurveId, held: &HashSet<PointId>) -> bool {
        let kind = &self.curves[c].kind;
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
        let close = |a: PointId, b: PointId| self.pos(a).distance(self.pos(b)) <= tol;
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
            } => self.axes_collapsed(ax, ay, minor) || close(center, start) || close(center, end),
            CurveKind::Spline { ref points } => points.windows(2).any(|w| close(w[0], w[1])),
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
