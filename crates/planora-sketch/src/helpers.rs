//! Zeichenhilfen: Rechteck, Spiegeln, Muster, Körperkanten und Kurven projizieren, Ebene einer
//! Fläche.
//!
//! Alle Hilfen sind atomar: Bei einem Fehler bleibt die Skizze unverändert.

use crate::constraint::{ConstraintKind, LineOrPoints};
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, PointId, Sketch, finite};
use planora_i18n::tr;
use planora_kernel::{Curve, Edge, Face, Id, Solid, Surface};
use planora_math::nurbs::{NurbsCurve, NurbsError};
use planora_math::{DVec2, DVec3, Frame, PI, TAU};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

/// Höchstzahl zusätzlicher Kopien eines Musters.
const MAX_PATTERN_COPIES: usize = 100;

/// Ids eines Rechtecks: Ecken unten links, unten rechts, oben rechts, oben links; Linien
/// unten, rechts, oben, links.
#[derive(Clone, Debug, PartialEq)]
pub struct RectIds {
    pub points: [PointId; 4],
    pub lines: [CurveId; 4],
}

/// Spiegelt `p` an der Geraden durch `a` mit Richtung `d` (normiert).
fn reflect(p: DVec2, a: DVec2, d: DVec2) -> DVec2 {
    let v = p - a;
    a + d * (2.0 * v.dot(d)) - v
}

/// Wie projizierte Geometrie angelegt wird (siehe [`Sketch::add_projected_curve`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectStyle {
    /// Kurve und Punkte als Konstruktion (alte Kantenkopie); sonst normale, profilfähige
    /// Geometrie (verknüpfte Projektion). Die `Fixed`-Werte gibt es in beiden Fällen.
    pub construction: bool,
}

/// Ein in die Skizzenebene projiziertes Kurvenstück (Werte in Skizzenkoordinaten).
#[derive(Clone, Debug)]
enum Projected {
    Line(DVec2, DVec2),
    Circle {
        center: DVec2,
        radius: f64,
    },
    /// Gegen den Uhrzeigersinn von `start` nach `end`.
    Arc {
        center: DVec2,
        start: DVec2,
        end: DVec2,
    },
    /// Große Halbachse als Vektor, kleine Halbachse als Länge.
    Ellipse {
        center: DVec2,
        axis: DVec2,
        minor: f64,
    },
    /// Ellipsenbogen wie `Ellipse`, gegen den Uhrzeigersinn von `start` nach `end`.
    EllipticArc {
        center: DVec2,
        axis: DVec2,
        minor: f64,
        start: DVec2,
        end: DVec2,
    },
    /// Freiformkurve in Skizzenkoordinaten (`z = 0`). `start` gleich `end` heißt geschlossen.
    Spline {
        start: DVec2,
        end: DVec2,
        curve: NurbsCurve,
    },
}

/// Was von einem projizierten Freiformstück übrig bleibt (Spec F2).
enum FlatNurbs {
    /// Alle Kontrollpunkte auf einer Stelle.
    Point,
    /// Alle Kontrollpunkte auf der Geraden durch `origin` mit der Richtung `dir` (normiert).
    Line {
        origin: DVec2,
        dir: DVec2,
    },
    Free,
}

/// Abschnitte je Bézier-Stück, in denen die Ausdehnung eines hochkant stehenden Stücks gesucht wird.
const EDGE_ON_STEPS_PER_SPAN: usize = 32;
/// Drittelungen, mit denen eine gefundene Spitze nachgeschärft wird.
const EDGE_ON_REFINE: usize = 60;

/// Gemeinsamer Fehler, wenn sich eine Freiformkante nicht zerlegen lässt.
fn freeform_failed(_: NurbsError) -> SketchError {
    unsupported(tr!("sketch-err-freeform-edge"))
}

/// Das Stück `from … to` (`from < to`) von `c`. Bei einer geschlossenen Kurve dürfen die Parameter
/// den Bereich verlassen und werden gefaltet wie im Kernel; ein Stück über die Naht wird zusammengesetzt.
/// Der Parameter bleibt der von `from … to`.
fn nurbs_range(c: &NurbsCurve, from: f64, to: f64, tol: f64) -> Result<NurbsCurve, SketchError> {
    let (a, b) = c.domain();
    let width = b - a;
    let eps = width * 1e-12;
    if from >= a - eps && to <= b + eps {
        return c.sub_curve(from.max(a), to.min(b)).map_err(freeform_failed);
    }
    if !c.is_closed(tol) || to - from > width + eps {
        return Err(SketchError::Degenerate(tr!("sketch-err-edge-no-range")));
    }
    let shift = (from - a).div_euclid(width) * width;
    let (u0, u1) = (from - shift, to - shift);
    if b - u0 <= eps {
        let part = c.sub_curve(a, u1 - b + a).map_err(freeform_failed)?;
        return shifted(&part, shift + width);
    }
    if u1 <= b + eps {
        let part = c.sub_curve(u0, u1.min(b)).map_err(freeform_failed)?;
        return shifted(&part, shift);
    }
    let first = c.sub_curve(u0, b).map_err(freeform_failed)?;
    if u1 - b <= eps {
        return shifted(&first, shift);
    }
    let second = c.sub_curve(a, u1 - width).map_err(freeform_failed)?;
    let second = shifted(&second, width)?;
    let joined = joined_at_seam(&first, &second)?;
    shifted(&joined, shift)
}

/// `c` mit um `by` verschobenem Parameter.
fn shifted(c: &NurbsCurve, by: f64) -> Result<NurbsCurve, SketchError> {
    NurbsCurve::new(
        c.degree(),
        c.knots().knots().iter().map(|k| k + by).collect(),
        c.control_points().to_vec(),
        c.weights().to_vec(),
    )
    .map_err(freeform_failed)
}

/// Hängt `next` an `first`: Sein erster Kontrollpunkt fällt mit dem letzten von `first` zusammen, der
/// Parameter läuft durch, der Knoten dort hat die Vielfachheit des Grades. Die homogene Teilung erlaubt,
/// alle Gewichte von `next` mit einem Faktor anzugleichen; die Kurve ändert sich dadurch nicht.
fn joined_at_seam(first: &NurbsCurve, next: &NurbsCurve) -> Result<NurbsCurve, SketchError> {
    let p = first.degree();
    let (ka, kb) = (first.knots().knots(), next.knots().knots());
    let mut knots = ka[..ka.len() - 1].to_vec();
    knots.extend_from_slice(&kb[p + 1..]);
    let mut control = first.control_points().to_vec();
    control.extend_from_slice(&next.control_points()[1..]);
    let scale = first.weights()[first.weights().len() - 1] / next.weights()[0];
    let mut weights = first.weights().to_vec();
    weights.extend(next.weights()[1..].iter().map(|w| w * scale));
    NurbsCurve::new(p, knots, control, weights).map_err(freeform_failed)
}

/// Einordnung nach den Kontrollpunkten (die Kurve liegt in ihrer konvexen Hülle).
fn flat_nurbs(c: &NurbsCurve, tol: f64) -> FlatNurbs {
    let pts: Vec<DVec2> = c.control_points().iter().map(|q| q.truncate()).collect();
    let (mut far, mut pair) = (0.0, (pts[0], pts[0]));
    for (i, &p) in pts.iter().enumerate() {
        for &q in &pts[i + 1..] {
            let d = p.distance(q);
            if d > far {
                (far, pair) = (d, (p, q));
            }
        }
    }
    if far <= tol {
        return FlatNurbs::Point;
    }
    let dir = (pair.1 - pair.0) / far;
    if pts.iter().all(|p| (*p - pair.0).perp_dot(dir).abs() <= tol) {
        FlatNurbs::Line {
            origin: pair.0,
            dir,
        }
    } else {
        FlatNurbs::Free
    }
}

/// Die wirkliche Ausdehnung des auf die Gerade projizierten Stücks, abgetastet und an den Spitzen
/// nachgeschärft; die Linie läuft vom Anfang des Stücks in Richtung seines Endes.
fn edge_on_line(c: &NurbsCurve, origin: DVec2, dir: DVec2) -> (DVec2, DVec2) {
    let (lo, hi) = c.domain();
    let along = |t: f64| (c.point(t).truncate() - origin).dot(dir);
    let n = c.knots().spans_in(lo, hi).max(1) * EDGE_ON_STEPS_PER_SPAN;
    let at = |k: usize| lo + (hi - lo) * k as f64 / n as f64;
    let (mut low, mut high) = (0, 0);
    for k in 1..=n {
        if along(at(k)) < along(at(low)) {
            low = k;
        }
        if along(at(k)) > along(at(high)) {
            high = k;
        }
    }
    let refined = |k: usize, sign: f64| {
        let (mut u, mut v) = (at(k.saturating_sub(1)), at((k + 1).min(n)));
        for _ in 0..EDGE_ON_REFINE {
            let (a, b) = (u + (v - u) / 3.0, v - (v - u) / 3.0);
            if sign * along(a) < sign * along(b) {
                u = a;
            } else {
                v = b;
            }
        }
        sign * (sign * along(0.5 * (u + v))).max(sign * along(at(k)))
    };
    let (min, max) = (refined(low, -1.0), refined(high, 1.0));
    let (p0, p1) = (origin + dir * min, origin + dir * max);
    if along(lo) <= along(hi) {
        (p0, p1)
    } else {
        (p1, p0)
    }
}

/// Wie [`Sketch::copy_curve`] kopiert: verschoben (lineares Muster), gespiegelt an einer Achse mit
/// normierter Richtung, oder an die Lage eines Exemplars (Muster aus `pattern.rs`).
#[derive(Clone, Copy, Debug)]
pub(crate) enum CopyHow {
    Shift,
    Mirror(DVec2),
    Pattern(crate::transform::Similarity),
}

fn unsupported(what: String) -> SketchError {
    SketchError::UnsupportedProjection(what)
}

impl Sketch {
    /// Führt `f` aus; bei einem Fehler wird der vorherige Zustand wiederhergestellt.
    pub(crate) fn atomically<T>(
        &mut self,
        f: impl FnOnce(&mut Sketch) -> Result<T, SketchError>,
    ) -> Result<T, SketchError> {
        let backup = self.clone();
        let result = f(self);
        if result.is_err() {
            *self = backup;
        }
        result
    }

    /// Prüft die Quellkurven für Spiegeln und Muster: vorhanden, ohne Doppelte, keine ganze
    /// Ellipse (Skizzen 7: Splines beider Arten gehen jetzt), kein fester Spline, keine erzeugte
    /// Kurve eines Textes (Spec E, Meldung `text_message`). Rahmenlinien allein werden wie Linien
    /// kopiert, ein Text entsteht dabei nicht (**Annahme**, Spec E).
    fn check_copy_sources(
        &self,
        curves: &[CurveId],
        text_message: impl FnOnce() -> String,
    ) -> Result<(), SketchError> {
        if curves.is_empty() {
            return Err(SketchError::InvalidInput(tr!("sketch-nothing-selected")));
        }
        for (i, &c) in curves.iter().enumerate() {
            self.check_curve(c)?;
            if curves[..i].contains(&c) {
                return Err(SketchError::InvalidInput(tr!("sketch-err-object-twice")));
            }
            if matches!(self.curves[c].kind, CurveKind::Ellipse { .. }) {
                return Err(SketchError::WrongType(tr!("sketch-err-copy-kinds")));
            }
            self.refuse_fixed_spline(c)?;
        }
        self.refuse_text_curves(curves, text_message)
    }

    /// Ein fester Spline ändert sich nicht und hat keine Kopie (Spec E); alle anderen Kurven gehen.
    pub(crate) fn refuse_fixed_spline(&self, c: CurveId) -> Result<(), SketchError> {
        if matches!(self.curves[c].kind, CurveKind::FixedSpline { .. }) {
            return Err(SketchError::WrongType(tr!("sketch-fixed-spline-no-copy")));
        }
        Ok(())
    }

    /// Achsenparalleles Rechteck mit den gegenüberliegenden Ecken `a` und `b`.
    pub fn add_rectangle(&mut self, a: DVec2, b: DVec2) -> Result<RectIds, SketchError> {
        if !(a.is_finite() && b.is_finite()) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-corners-finite")));
        }
        let (min, max) = (a.min(b), a.max(b));
        let size = max - min;
        if size.x <= self.tolerance().linear || size.y <= self.tolerance().linear {
            return Err(SketchError::Degenerate(tr!("sketch-err-rect-size")));
        }
        self.atomically(|s| {
            let corners = [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)];
            let mut points = [Id::from_index(0); 4];
            for (slot, c) in points.iter_mut().zip(corners) {
                *slot = s.add_point(c.x, c.y)?;
            }
            let mut lines = [Id::from_index(0); 4];
            for (i, slot) in lines.iter_mut().enumerate() {
                *slot = s.add_line(points[i], points[(i + 1) % 4])?;
            }
            s.add_constraint(ConstraintKind::Horizontal(LineOrPoints::Line(lines[0])))?;
            s.add_constraint(ConstraintKind::Vertical(LineOrPoints::Line(lines[1])))?;
            s.add_constraint(ConstraintKind::Horizontal(LineOrPoints::Line(lines[2])))?;
            s.add_constraint(ConstraintKind::Vertical(LineOrPoints::Line(lines[3])))?;
            Ok(RectIds { points, lines })
        })
    }

    /// Legt eine Kopie der Kurve `source` an, deren Punkte über `map` abgebildet werden; Stil wie
    /// die Vorlage. Beim Spiegeln (`CopyHow::Mirror`) werden Bögen und Ellipsenbögen mit
    /// vertauschtem Start und Ende angelegt und die große Halbachse eines Ellipsenbogens
    /// gespiegelt; Splines behalten die Reihenfolge ihrer Punkte, ein Spline mit Kontrollpunkten
    /// auch Grad und Knoten. Nur Muster (`CopyHow::Pattern`) kopieren auch ganze Ellipsen; Radius
    /// und Halbachsen nehmen den Maßstab, die große Halbachse die Drehung der Lage mit. Gibt die
    /// neue Kurve zurück.
    pub(crate) fn copy_curve(
        &mut self,
        source: CurveId,
        map: &HashMap<PointId, PointId>,
        how: CopyHow,
    ) -> Result<CurveId, SketchError> {
        let swap_arc = matches!(how, CopyHow::Mirror(_));
        let factor = match how {
            CopyHow::Pattern(t) => t.factor,
            _ => 1.0,
        };
        let construction = self.curves[source].construction;
        let centerline = self.curves[source].centerline;
        let id = match self.curves[source].kind.clone() {
            CurveKind::Line { a, b } => self.add_line(map[&a], map[&b])?,
            CurveKind::Circle { center, radius } => {
                let r = self.vars[radius] * factor;
                self.add_circle(map[&center], r)?
            }
            CurveKind::Arc { center, start, end } => {
                let (s, e) = if swap_arc {
                    (map[&end], map[&start])
                } else {
                    (map[&start], map[&end])
                };
                self.add_arc(map[&center], s, e)?
            }
            CurveKind::EllipticArc {
                center,
                ax,
                ay,
                minor,
                start,
                end,
                ..
            } => {
                let a = DVec2::new(self.vars[ax], self.vars[ay]);
                // Spiegeln kehrt den Umlaufsinn um; die gespiegelte Ellipse ist dieselbe Menge
                // wie die mit gespiegelter großer Halbachse und gleicher kleiner Halbachse.
                let (major, s, e) = match how {
                    CopyHow::Mirror(d) => (reflect(a, DVec2::ZERO, d), map[&end], map[&start]),
                    CopyHow::Shift => (a, map[&start], map[&end]),
                    CopyHow::Pattern(t) => (t.linear(a), map[&start], map[&end]),
                };
                self.add_elliptic_arc(map[&center], major, self.vars[minor] * factor, s, e)?
            }
            CurveKind::Ellipse {
                center,
                ax,
                ay,
                minor,
                ..
            } if matches!(how, CopyHow::Pattern(_)) => {
                let CopyHow::Pattern(t) = how else {
                    unreachable!()
                };
                let major = t.linear(DVec2::new(self.vars[ax], self.vars[ay]));
                self.add_ellipse(map[&center], major, self.vars[minor] * factor)?
            }
            CurveKind::Spline { points, .. } => {
                let mapped: Vec<PointId> = points.iter().map(|p| map[p]).collect();
                self.add_spline(&mapped)?
            }
            CurveKind::ControlSpline {
                points,
                degree,
                knots,
            } => {
                let mapped: Vec<PointId> = points.iter().map(|p| map[p]).collect();
                self.add_control_spline_as(&mapped, degree, knots)?
            }
            CurveKind::Conic {
                start,
                end,
                apex,
                rho,
            } => self.add_conic_as(map[&start], map[&end], map[&apex], rho)?,
            CurveKind::FixedSpline { .. } => {
                return Err(SketchError::WrongType(tr!("sketch-fixed-spline-no-copy")));
            }
            CurveKind::Ellipse { .. } => {
                return Err(SketchError::WrongType(tr!("sketch-err-copy-kinds")));
            }
        };
        self.curves[id].construction = construction;
        self.curves[id].centerline = centerline;
        Ok(id)
    }

    /// Neuer Punkt an `xy` mit der Konstruktionseigenschaft von `like`. Ist `like` ein
    /// 3D-Punkt, wird auch der neue einer, mit dem z von `like` (Kopieren, Muster, Spiegeln).
    pub(crate) fn add_point_like(
        &mut self,
        like: PointId,
        xy: DVec2,
    ) -> Result<PointId, SketchError> {
        let p = match self.points[like].z {
            Some(zi) => self.add_point3(xy.extend(self.vars[zi]))?,
            None => self.add_point(xy.x, xy.y)?,
        };
        self.points[p].construction = self.points[like].construction;
        Ok(p)
    }

    /// Spiegelt Linien, Kreise, Bögen, Ellipsenbögen und Splines an der Linie `axis`. Jeder Punkt bekommt
    /// ein Spiegelbild mit `Symmetric`-Abhängigkeit (Punkte auf der Achse werden
    /// wiederverwendet), Kreise ein `Equal` zum Original. Bögen und Ellipsenbögen werden mit
    /// vertauschtem Start und Ende angelegt; die Halbachsen eines gespiegelten Ellipsenbogens
    /// sind nicht an das Original gebunden.
    ///
    /// Griffe (Skizzen 7) gehen mit dem Spline und umgekehrt, geliefert werden nur die Kopien von
    /// `curves`. `at` und `tip` bekommen `Symmetric`, zwei Krümmungsbögen `Equal`. Mitte und freies Ende
    /// eines Bogens bekommen nichts und immer einen eigenen Punkt: Den Rest halten seine internen
    /// Zeilen, jede weitere wäre überzählig.
    pub fn add_mirrored(
        &mut self,
        curves: &[CurveId],
        axis: CurveId,
    ) -> Result<Vec<CurveId>, SketchError> {
        self.check_curve(axis)?;
        let CurveKind::Line { a: aa, b: ab } = self.curves[axis].kind else {
            return Err(SketchError::WrongType(tr!("sketch-err-mirror-line")));
        };
        self.check_copy_sources(curves, || tr!("sketch-text-no-mirror"))?;
        // Auch an einer Buchstabenkurve spiegelt nichts: Die Spiegelung bände an sie.
        self.refuse_text_curves(&[axis], || tr!("sketch-text-no-mirror"))?;
        let all = self.with_handle_family(curves);
        if all.contains(&axis) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-mirror-self")));
        }
        let (origin, dir) = (self.pos(aa), self.pos(ab) - self.pos(aa));
        if dir.length() <= self.tolerance().linear {
            return Err(SketchError::Degenerate(tr!("sketch-err-mirror-zero")));
        }
        let d = dir.normalize();
        self.atomically(|s| {
            let tol = s.tolerance().linear;
            let bows = s.handle_arcs(&all);
            let mut map: HashMap<PointId, PointId> = HashMap::new();
            let mut copies: HashMap<CurveId, CurveId> = HashMap::new();
            for &c in &all {
                for p in s.curves[c].kind.points() {
                    if map.contains_key(&p) {
                        continue;
                    }
                    let xy = s.pos(p);
                    let on_axis = (xy - origin).perp_dot(d).abs() <= tol;
                    if bows.iter().any(|b| b.1 == p || b.2 == p) {
                        map.insert(p, s.add_point_like(p, reflect(xy, origin, d))?);
                    } else if on_axis {
                        map.insert(p, p);
                    } else {
                        let q = s.add_point_like(p, reflect(xy, origin, d))?;
                        s.add_constraint(ConstraintKind::Symmetric(p, q, axis))?;
                        map.insert(p, q);
                    }
                }
                let copy = s.copy_curve(c, &map, CopyHow::Mirror(d))?;
                if matches!(s.curves[c].kind, CurveKind::Circle { .. }) {
                    s.add_constraint(ConstraintKind::Equal(c, copy))?;
                }
                copies.insert(c, copy);
            }
            for &c in &all {
                s.carry_handles(c, copies[&c], &map, &copies);
            }
            for &(arc, ..) in &bows {
                s.add_constraint(ConstraintKind::Equal(arc, copies[&arc]))?;
            }
            Ok(curves.iter().map(|c| copies[c]).collect())
        })
    }

    /// Lineares Muster: `count` zusätzliche Kopien, Kopie `k` um `k·offset` verschoben.
    /// Kopien von Ellipsenbögen sind nicht über `Equal` an das Original gebunden.
    pub fn add_linear_pattern(
        &mut self,
        curves: &[CurveId],
        offset: DVec2,
        count: usize,
    ) -> Result<Vec<Vec<CurveId>>, SketchError> {
        if !(1..=MAX_PATTERN_COPIES).contains(&count) {
            return Err(SketchError::InvalidInput(tr!(
                "sketch-err-copies-range",
                max = MAX_PATTERN_COPIES.to_string()
            )));
        }
        if !offset.is_finite() {
            return Err(SketchError::InvalidInput(tr!("sketch-err-offset-finite")));
        }
        let tol = self.tolerance().linear;
        if offset.length() <= tol {
            return Err(SketchError::Degenerate(tr!("sketch-err-offset-zero")));
        }
        self.check_copy_sources(curves, || tr!("sketch-text-no-pattern"))?;
        let all = self.with_handle_family(curves);
        // Griffe binden nichts an die Vorlage, so wenig wie der Spline, zu dem sie gehören (**Annahme**).
        let parts = self.handle_curves();
        let first = self.curves[curves[0]].kind.points()[0];
        let horizontal = offset.y.abs() <= tol;
        let vertical = offset.x.abs() <= tol;
        self.atomically(|s| {
            let mut out = Vec::with_capacity(count);
            // Vom Muster angelegte Abhängigkeiten (zum Aufräumen der überzähligen, siehe unten).
            let mut created = Vec::new();
            for k in 1..=count {
                let shift = offset * k as f64;
                let mut map: HashMap<PointId, PointId> = HashMap::new();
                let mut copies: HashMap<CurveId, CurveId> = HashMap::new();
                for &c in &all {
                    for p in s.curves[c].kind.points() {
                        if let Entry::Vacant(slot) = map.entry(p) {
                            slot.insert(s.add_point_like(p, s.pos(p) + shift)?);
                        }
                    }
                    let copy = s.copy_curve(c, &map, CopyHow::Shift)?;
                    copies.insert(c, copy);
                    if parts.contains(&c) {
                        continue;
                    }
                    // `Equal` gibt es nur für Linien, Kreise und Bögen.
                    if !matches!(
                        s.curves[c].kind,
                        CurveKind::EllipticArc { .. }
                            | CurveKind::Spline { .. }
                            | CurveKind::ControlSpline { .. }
                            | CurveKind::Conic { .. }
                    ) {
                        created.push(s.add_constraint(ConstraintKind::Equal(c, copy))?);
                    }
                    if matches!(s.curves[c].kind, CurveKind::Line { .. }) {
                        created.push(s.add_constraint(ConstraintKind::Parallel(c, copy))?);
                    }
                }
                for &c in &all {
                    s.carry_handles(c, copies[&c], &map, &copies);
                }
                let group: Vec<CurveId> = curves.iter().map(|c| copies[c]).collect();
                let moved = map[&first];
                created.push(s.add_constraint(ConstraintKind::Distance(
                    first,
                    moved,
                    shift.length(),
                ))?);
                if horizontal {
                    created.push(s.add_constraint(ConstraintKind::Horizontal(
                        LineOrPoints::Points(first, moved),
                    ))?);
                } else if vertical {
                    created.push(s.add_constraint(ConstraintKind::Vertical(
                        LineOrPoints::Points(first, moved),
                    ))?);
                }
                out.push(group);
            }
            // Bei geschlossenen Formen folgen Gleichheit und Parallelität der letzten Kante aus
            // den übrigen (je Kopie zwei überzählige Abhängigkeiten bei einem Rechteck). Die vom
            // Muster angelegten überzähligen werden wieder entfernt; die Form bleibt dieselbe,
            // weil jede überzählige Abhängigkeit aus den verbleibenden folgt. Jede dieser
            // Abhängigkeiten hat genau eine Zeile, die Rangprüfung meldet also genau die
            // entbehrlichen.
            let report = s.solve();
            for id in report.redundant {
                if created.contains(&id) {
                    s.remove_constraint(id)?;
                }
            }
            Ok(out)
        })
    }

    /// Kopiert eine Körperkante orthogonal projiziert als feste Konstruktionsgeometrie in die
    /// Skizze. Unterstützt Geraden (nicht senkrecht zur Skizzenebene), Kreise und Bögen in
    /// Ebenen parallel zur Skizzenebene sowie ganze schräge Kreise und ganze Ellipsen.
    /// Hülle um [`Sketch::add_projected_curve`] mit `construction: true`.
    pub fn add_projected_edge(
        &mut self,
        solid: &Solid,
        edge: Id<Edge>,
    ) -> Result<CurveId, SketchError> {
        let projected = self.project_edge(solid, edge)?;
        self.add_projected(projected, ProjectStyle { construction: true })
    }

    /// Projiziert das Kurvenstück `curve(t0 … t1)` orthogonal in die Skizzenebene und legt es
    /// als feste Geometrie an: Linie, Kreis, Bogen, Ellipse, Ellipsenbogen (schräger Kreisbogen,
    /// Teilstück einer Ellipse) oder fester Spline (Freiformkurve). Die Punkte bekommen
    /// `Fixed`-Werte, Kreise einen `Radius`, Ellipsen und Ellipsenbögen `EllipseMajor`,
    /// `EllipseMinor` und `EllipseAngle`. Mit `construction: false` sind Kurve und Punkte normale
    /// Geometrie (verknüpfte Projektion, bildet Profile). Stücke, die nur ein Punkt wären, werden
    /// abgelehnt; die Skizze bleibt dann unverändert.
    pub fn add_projected_curve(
        &mut self,
        curve: &Curve,
        t0: f64,
        t1: f64,
        style: ProjectStyle,
    ) -> Result<CurveId, SketchError> {
        let projected = self.project_curve(curve, t0, t1)?;
        self.add_projected(projected, style)
    }

    /// Legt eine projizierte Kurve an (siehe [`Sketch::add_projected_curve`]).
    fn add_projected(
        &mut self,
        projected: Projected,
        style: ProjectStyle,
    ) -> Result<CurveId, SketchError> {
        let construction = style.construction;
        self.atomically(|s| {
            let c = match projected {
                Projected::Line(p0, p1) => {
                    let a = s.add_fixed_point(p0, construction)?;
                    let b = s.add_fixed_point(p1, construction)?;
                    s.add_line(a, b)?
                }
                Projected::Circle { center, radius } => {
                    let m = s.add_fixed_point(center, construction)?;
                    let c = s.add_circle(m, radius)?;
                    s.add_constraint(ConstraintKind::Radius(c, radius))?;
                    c
                }
                Projected::Arc { center, start, end } => {
                    let m = s.add_fixed_point(center, construction)?;
                    let a = s.add_fixed_point(start, construction)?;
                    let b = s.add_fixed_point(end, construction)?;
                    s.add_arc(m, a, b)?
                }
                Projected::Ellipse {
                    center,
                    axis,
                    minor,
                } => {
                    let m = s.add_fixed_point(center, construction)?;
                    let e = s.add_ellipse(m, axis, minor)?;
                    s.add_ellipse_values(e, axis, minor)?;
                    e
                }
                Projected::EllipticArc {
                    center,
                    axis,
                    minor,
                    start,
                    end,
                } => {
                    let m = s.add_fixed_point(center, construction)?;
                    let a = s.add_fixed_point(start, construction)?;
                    let b = s.add_fixed_point(end, construction)?;
                    // Wie `add_elliptic_arc`, aber die interne Abhängigkeit `EllipticArcEnds`
                    // kommt nach den Ellipsen-Werten: Mitte, Halbachsen und Enden sind damit
                    // schon bestimmt, und die Rangprüfung nennt nur die (stille) interne
                    // Abhängigkeit überzählig statt `EllipseMajor`/`EllipseMinor`. Die Prüfung
                    // auf Musterschleifen entfällt: Alle Variablen sind neu und gehören zu
                    // keinem Muster.
                    s.check_elliptic_arc(m, axis, minor, a, b)?;
                    let (ax, ay, mi) = (s.new_var(axis.x), s.new_var(axis.y), s.new_var(minor));
                    let e = s.push_curve(CurveKind::EllipticArc {
                        center: m,
                        ax,
                        ay,
                        az: None,
                        minor: mi,
                        start: a,
                        end: b,
                    });
                    s.add_ellipse_values(e, axis, minor)?;
                    s.push_internal(ConstraintKind::EllipticArcEnds(e));
                    e
                }
                Projected::Spline { start, end, curve } => {
                    let a = s.add_fixed_point(start, construction)?;
                    // Geschlossene Kante: ein fester Punkt.
                    let b = if start == end {
                        a
                    } else {
                        s.add_fixed_point(end, construction)?
                    };
                    s.push_curve(CurveKind::FixedSpline {
                        start: a,
                        end: b,
                        curve: Arc::new(curve),
                    })
                }
            };
            s.curves[c].construction = construction;
            Ok(c)
        })
    }

    /// `EllipseMajor`, `EllipseMinor` und `EllipseAngle` einer projizierten Ellipse.
    fn add_ellipse_values(
        &mut self,
        e: CurveId,
        axis: DVec2,
        minor: f64,
    ) -> Result<(), SketchError> {
        self.add_constraint(ConstraintKind::EllipseMajor(e, axis.length()))?;
        self.add_constraint(ConstraintKind::EllipseMinor(e, minor))?;
        self.add_constraint(ConstraintKind::EllipseAngle(e, axis.y.atan2(axis.x)))?;
        Ok(())
    }

    /// Führt die kopierte Körperkante `curve` nach: Die Kante wird wie bei
    /// [`Sketch::add_projected_edge`] projiziert, sonst wie [`Sketch::reproject_curve`].
    pub fn reproject_edge(
        &mut self,
        curve: CurveId,
        solid: &Solid,
        edge: Id<Edge>,
    ) -> Result<(), SketchError> {
        self.check_curve(curve)?;
        let projected = self.project_edge(solid, edge)?;
        self.reproject(curve, projected)
    }

    /// Führt die projizierte Kurve `id` nach: Das Stück `curve(t0 … t1)` wird wie bei
    /// [`Sketch::add_projected_curve`] projiziert, die vorhandene Kurve bekommt die neue Lage
    /// und Größe, ebenso die dazugehörigen `Fixed`-, `Radius`- und `Ellipse…`-Werte. Die Kurve
    /// behält ihre Id, ihren Stil und alle Abhängigkeiten, die der Nutzer daran geknüpft hat.
    /// Ändert das Stück seine Art (Fehler `sketch-err-edge-type-changed`) oder lässt es sich
    /// nicht projizieren, bleibt die Skizze unverändert.
    pub fn reproject_curve(
        &mut self,
        id: CurveId,
        curve: &Curve,
        t0: f64,
        t1: f64,
    ) -> Result<(), SketchError> {
        self.check_curve(id)?;
        let projected = self.project_curve(curve, t0, t1)?;
        self.reproject(id, projected)
    }

    /// Gibt der projizierten Kurve `id` Lage und Werte derselben Kurve aus `from` (etwa dem
    /// zuletzt berechneten Stand der Skizze), in Skizzenkoordinaten; die zugehörigen `Fixed`-,
    /// `Radius`- und `Ellipse…`-Werte werden mitgeführt wie bei [`Sketch::reproject_curve`].
    /// Fehlt die Kurve in `from` oder hat sie dort eine andere Art, bleibt die Skizze
    /// unverändert und es gibt einen Fehler.
    pub fn restore_projected_curve(
        &mut self,
        id: CurveId,
        from: &Sketch,
    ) -> Result<(), SketchError> {
        self.check_curve(id)?;
        from.check_curve(id)?;
        let projected = match from.curves[id].kind {
            CurveKind::FixedSpline {
                start,
                end,
                ref curve,
            } => Projected::Spline {
                start: from.pos(start),
                end: from.pos(end),
                curve: (**curve).clone(),
            },
            CurveKind::Line { a, b } => Projected::Line(from.pos(a), from.pos(b)),
            CurveKind::Circle { center, radius } => Projected::Circle {
                center: from.pos(center),
                radius: from.vars[radius],
            },
            CurveKind::Arc { center, start, end } => Projected::Arc {
                center: from.pos(center),
                start: from.pos(start),
                end: from.pos(end),
            },
            CurveKind::Ellipse {
                center,
                ax,
                ay,
                minor,
                ..
            } => Projected::Ellipse {
                center: from.pos(center),
                axis: DVec2::new(from.vars[ax], from.vars[ay]),
                minor: from.vars[minor],
            },
            CurveKind::EllipticArc {
                center,
                ax,
                ay,
                minor,
                start,
                end,
                ..
            } => Projected::EllipticArc {
                center: from.pos(center),
                axis: DVec2::new(from.vars[ax], from.vars[ay]),
                minor: from.vars[minor],
                start: from.pos(start),
                end: from.pos(end),
            },
            CurveKind::Spline { .. }
            | CurveKind::ControlSpline { .. }
            | CurveKind::Conic { .. } => {
                return Err(unsupported(tr!("sketch-err-edge-type-changed")));
            }
        };
        self.reproject(id, projected)
    }

    /// Gibt der Kurve `curve` die neue Lage `projected` (siehe [`Sketch::reproject_curve`]).
    fn reproject(&mut self, curve: CurveId, projected: Projected) -> Result<(), SketchError> {
        self.atomically(|s| {
            let kind = s.curves[curve].kind.clone();
            // Neue Maße, die in den Abhängigkeiten nachgeführt werden.
            let mut radius_value = None;
            let mut ellipse_values = None;
            let mut new_shape = false;
            match (projected, kind) {
                (Projected::Line(p0, p1), CurveKind::Line { a, b }) => {
                    s.set_point_xy(a, p0.x, p0.y)?;
                    s.set_point_xy(b, p1.x, p1.y)?;
                }
                (
                    Projected::Circle { center, radius },
                    CurveKind::Circle {
                        center: m,
                        radius: r,
                    },
                ) => {
                    finite(radius, || tr!("sketch-err-what-radius"))?;
                    s.set_point_xy(m, center.x, center.y)?;
                    s.vars[r] = radius;
                    radius_value = Some(radius);
                }
                (
                    Projected::Arc { center, start, end },
                    CurveKind::Arc {
                        center: m,
                        start: a,
                        end: b,
                    },
                ) => {
                    s.set_point_xy(m, center.x, center.y)?;
                    s.set_point_xy(a, start.x, start.y)?;
                    s.set_point_xy(b, end.x, end.y)?;
                }
                (
                    Projected::Ellipse {
                        center,
                        axis,
                        minor,
                    },
                    CurveKind::Ellipse {
                        center: m,
                        ax,
                        ay,
                        minor: mi,
                        ..
                    },
                ) => {
                    ellipse_values = Some(s.set_ellipse_axes((ax, ay, mi), axis, minor)?);
                    s.set_point_xy(m, center.x, center.y)?;
                }
                (
                    Projected::EllipticArc {
                        center,
                        axis,
                        minor,
                        start,
                        end,
                    },
                    CurveKind::EllipticArc {
                        center: m,
                        ax,
                        ay,
                        minor: mi,
                        start: a,
                        end: b,
                        ..
                    },
                ) => {
                    ellipse_values = Some(s.set_ellipse_axes((ax, ay, mi), axis, minor)?);
                    s.set_point_xy(m, center.x, center.y)?;
                    s.set_point_xy(a, start.x, start.y)?;
                    s.set_point_xy(b, end.x, end.y)?;
                }
                // Andere Knoten oder ein anderer Grad sind keine Artänderung. Wird aus der offenen
                // Kante eine geschlossene oder umgekehrt, passen die Punkte nicht mehr: Das zählt als
                // Artänderung (**Annahme**).
                (
                    Projected::Spline {
                        start,
                        end,
                        curve: new,
                    },
                    CurveKind::FixedSpline {
                        start: a, end: b, ..
                    },
                ) if (start == end) == (a == b) => {
                    s.set_point_xy(a, start.x, start.y)?;
                    s.set_point_xy(b, end.x, end.y)?;
                    s.curves[curve].kind = CurveKind::FixedSpline {
                        start: a,
                        end: b,
                        curve: Arc::new(new),
                    };
                    // Der neue Bereich passt nicht mehr zu den alten Parametern.
                    new_shape = true;
                }
                _ => return Err(unsupported(tr!("sketch-err-edge-type-changed"))),
            }
            // Die Fixwerte und Maße, die `add_projected` angelegt hat, mitführen.
            let points = s.curves[curve].kind.points();
            let ids: Vec<_> = s
                .constraints
                .iter()
                .filter(|(_, c)| c.alive)
                .map(|(id, _)| id)
                .collect();
            for id in ids {
                let new_kind = match s.constraints[id].kind {
                    ConstraintKind::Fixed(p, ..) if points.contains(&p) => {
                        let xy = s.pos(p);
                        Some(ConstraintKind::Fixed(p, xy.x, xy.y))
                    }
                    ConstraintKind::Fixed3(p, ..) if points.contains(&p) => {
                        let at = s.pos3(p);
                        Some(ConstraintKind::Fixed3(p, at.x, at.y, at.z))
                    }
                    ConstraintKind::Radius(c, _) if c == curve => {
                        radius_value.map(|r| ConstraintKind::Radius(c, r))
                    }
                    ConstraintKind::EllipseMajor(c, _) if c == curve => {
                        ellipse_values.map(|(l, ..)| ConstraintKind::EllipseMajor(c, l))
                    }
                    ConstraintKind::EllipseMinor(c, _) if c == curve => {
                        ellipse_values.map(|(_, m, _)| ConstraintKind::EllipseMinor(c, m))
                    }
                    ConstraintKind::EllipseAngle(c, _) if c == curve => {
                        ellipse_values.map(|(.., a)| ConstraintKind::EllipseAngle(c, a))
                    }
                    _ => None,
                };
                if let Some(k) = new_kind {
                    s.constraints[id].kind = k;
                }
            }
            s.touch_structure();
            s.refresh_shared_signs(Some(curve));
            if new_shape {
                s.reseed_extras(curve);
            }
            Ok(())
        })
    }

    /// Setzt die Halbachsen-Variablen `(ax, ay, minor)` einer Ellipse oder eines
    /// Ellipsenbogens; liefert die neuen Werte für `EllipseMajor`, `EllipseMinor` und
    /// `EllipseAngle`.
    fn set_ellipse_axes(
        &mut self,
        (ax, ay, mi): (usize, usize, usize),
        axis: DVec2,
        minor: f64,
    ) -> Result<(f64, f64, f64), SketchError> {
        finite(axis.x, || tr!("sketch-err-what-major"))?;
        finite(axis.y, || tr!("sketch-err-what-major"))?;
        finite(minor, || tr!("sketch-err-what-minor"))?;
        (self.vars[ax], self.vars[ay], self.vars[mi]) = (axis.x, axis.y, minor);
        Ok((axis.length(), minor, axis.y.atan2(axis.x)))
    }

    /// Ob das Kurvenstück `curve(t0 … t1)` in der Skizzenebene nur ein Punkt wäre: eine Gerade
    /// senkrecht zur Skizzenebene oder eine Freiformkurve, deren Kontrollpunkte alle auf eine Stelle
    /// fallen. Andere Kurven liefern `false`.
    pub fn projects_to_point(&self, curve: &Curve, t0: f64, t1: f64) -> bool {
        if let Curve::Nurbs(c) = curve {
            return self.project_nurbs(c, t0, t1).is_ok_and(|piece| {
                matches!(
                    flat_nurbs(&piece, self.tolerance().linear),
                    FlatNurbs::Point
                )
            });
        }
        let Curve::Line { .. } = curve else {
            return false;
        };
        let frame = self.frame();
        let project = |t: f64| frame.to_local(curve.point(t)).truncate();
        project(t0).distance(project(t1)) <= self.tolerance().linear
    }

    /// Projiziert die Körperkante `edge` in die Skizzenebene, ohne die Skizze zu ändern.
    /// Die Kantenkopie lehnt Teilstücke schräger Kreise und Teilstücke von Ellipsen weiter ab
    /// (wie vor Skizzen 6); alles andere wie [`Sketch::project_curve`].
    fn project_edge(&self, solid: &Solid, edge: Id<Edge>) -> Result<Projected, SketchError> {
        let e = solid.edges.get(edge).ok_or(SketchError::UnknownObject)?;
        let tol = *self.tolerance();
        if !(e.t0.is_finite() && e.t1.is_finite()) || (e.t1 - e.t0).abs() <= tol.angular {
            return Err(SketchError::Degenerate(tr!("sketch-err-edge-no-range")));
        }
        let full = (e.t1 - e.t0).abs() >= TAU - tol.angular;
        match &e.curve {
            Curve::Circle { frame: cf, .. }
                if !full && cf.z.cross(self.frame().z).length() > tol.angular =>
            {
                Err(unsupported(tr!("sketch-err-arc-not-parallel")))
            }
            Curve::Ellipse { .. } if !full => Err(unsupported(tr!("sketch-err-ellipse-arc"))),
            // Hochkant stehende Kreise und Ellipsen lehnt die alte Kantenkopie weiter ab; erst die
            // neue Projektion macht sie zu Linien (Skizzen 6).
            Curve::Circle { frame: cf, .. } | Curve::Ellipse { frame: cf, .. }
                if cf.z.dot(self.frame().z).abs() <= tol.angular =>
            {
                Err(unsupported(tr!("sketch-err-curve-perpendicular")))
            }
            _ => self.project_curve(&e.curve, e.t0, e.t1),
        }
    }

    /// Projiziert das Kurvenstück `curve(t0 … t1)` in die Skizzenebene, ohne die Skizze zu
    /// ändern.
    fn project_curve(&self, curve: &Curve, t0: f64, t1: f64) -> Result<Projected, SketchError> {
        let frame = *self.frame();
        let tol = *self.tolerance();
        let project = |p: DVec3| frame.to_local(p).truncate();
        if !(t0.is_finite() && t1.is_finite()) || (t1 - t0).abs() <= tol.angular {
            return Err(SketchError::Degenerate(tr!("sketch-err-edge-no-range")));
        }
        let full = (t1 - t0).abs() >= TAU - tol.angular;
        match curve {
            Curve::Line { .. } => {
                let (p0, p1) = (project(curve.point(t0)), project(curve.point(t1)));
                if p0.distance(p1) <= tol.linear {
                    return Err(unsupported(tr!("sketch-err-edge-perpendicular")));
                }
                Ok(Projected::Line(p0, p1))
            }
            Curve::Circle {
                frame: cf,
                radius: r,
            } => {
                if cf.z.dot(frame.z).abs() <= tol.angular {
                    return self.project_edge_on(curve, (cf, *r, *r), t0, t1, full);
                }
                if cf.z.cross(frame.z).length() > tol.angular {
                    return self.project_ellipse_piece(curve, (cf, *r, *r), t0, t1, full);
                }
                let center = project(cf.origin);
                if full {
                    Ok(Projected::Circle { center, radius: *r })
                } else {
                    let (p0, p1) = (project(curve.point(t0)), project(curve.point(t1)));
                    let ccw = (t1 - t0).signum() * cf.z.dot(frame.z).signum() > 0.0;
                    let (start, end) = if ccw { (p0, p1) } else { (p1, p0) };
                    Ok(Projected::Arc { center, start, end })
                }
            }
            Curve::Ellipse {
                frame: cf,
                major,
                minor,
            } if cf.z.dot(frame.z).abs() <= tol.angular => {
                self.project_edge_on(curve, (cf, *major, *minor), t0, t1, full)
            }
            Curve::Ellipse {
                frame: cf,
                major,
                minor,
            } => self.project_ellipse_piece(curve, (cf, *major, *minor), t0, t1, full),
            Curve::Nurbs(c) => {
                let piece = self.project_nurbs(c, t0, t1)?;
                match flat_nurbs(&piece, tol.linear) {
                    FlatNurbs::Point => Err(unsupported(tr!("sketch-err-curve-perpendicular"))),
                    FlatNurbs::Line { origin, dir } => {
                        let (p0, p1) = edge_on_line(&piece, origin, dir);
                        if p0.distance(p1) <= tol.linear {
                            return Err(unsupported(tr!("sketch-err-curve-perpendicular")));
                        }
                        Ok(Projected::Line(p0, p1))
                    }
                    FlatNurbs::Free => {
                        let (lo, hi) = piece.domain();
                        let start = piece.point(lo).truncate();
                        let end = piece.point(hi).truncate();
                        Ok(Projected::Spline {
                            start,
                            // Geschlossen heißt in der Skizze: genau ein Punkt.
                            end: if start.distance(end) <= tol.linear {
                                start
                            } else {
                                end
                            },
                            curve: piece,
                        })
                    }
                }
            }
        }
    }

    /// Das Stück `t0 … t1` der Freiformkurve `c` in Skizzenkoordinaten (`z = 0`), absteigend umgedreht:
    /// Kontrollpunkte abgebildet, Gewichte und Knoten unverändert. Das ist exakt, auch rational, denn
    /// die Abbildung ist affin.
    fn project_nurbs(&self, c: &NurbsCurve, t0: f64, t1: f64) -> Result<NurbsCurve, SketchError> {
        let frame = *self.frame();
        if !(t0.is_finite() && t1.is_finite()) || t0 == t1 {
            return Err(SketchError::Degenerate(tr!("sketch-err-edge-no-range")));
        }
        let piece = nurbs_range(c, t0.min(t1), t0.max(t1), self.tolerance().linear)?;
        let piece = if t1 < t0 {
            piece.reversed().map_err(freeform_failed)?
        } else {
            piece
        };
        piece
            .transformed(|p| frame.to_local(p).truncate().extend(0.0))
            .map_err(freeform_failed)
    }

    /// Projiziert ein Kreis- oder Ellipsenstück, dessen Ebene senkrecht auf der Skizzenebene
    /// steht (wie Fusion): Es wird eine Linie. Die Projektion `c + A·cos t + B·sin t` liegt
    /// dann auf einer Geraden durch `c`; die Linie reicht vom kleinsten bis zum größten Wert
    /// entlang dieser Geraden, beim ganzen Kreis zwischen den beiden äußersten Punkten, beim
    /// Bogen über seine Enden und jede Spitze innerhalb seines Bereichs.
    fn project_edge_on(
        &self,
        curve: &Curve,
        (cf, major, minor): (&Frame, f64, f64),
        t0: f64,
        t1: f64,
        full: bool,
    ) -> Result<Projected, SketchError> {
        let frame = *self.frame();
        let tol = *self.tolerance();
        let in_plane = |v: DVec3| DVec2::new(v.dot(frame.x), v.dot(frame.y));
        let center = frame.to_local(cf.origin).truncate();
        let big_a = in_plane(cf.x) * major;
        let big_b = in_plane(cf.y) * minor;
        let dir = if big_a.length() >= big_b.length() {
            big_a
        } else {
            big_b
        };
        let u = dir.normalize_or_zero();
        if u == DVec2::ZERO {
            return Err(unsupported(tr!("sketch-err-curve-perpendicular")));
        }
        let (a, b) = (big_a.dot(u), big_b.dot(u));
        // Wert entlang der Geraden bei `t` (über den Punkt der Kurve, damit Parameter und Lage
        // genau zur Kante passen).
        let along = |t: f64| (frame.to_local(curve.point(t)).truncate() - center).dot(u);
        let (lo, hi) = if full {
            let amp = a.hypot(b);
            (-amp, amp)
        } else {
            let (from, to) = (t0.min(t1), t0.max(t1));
            let mut vals = vec![along(t0), along(t1)];
            // Spitzen bei `t = atan2(b, a) + k·π`.
            let base = b.atan2(a);
            let k0 = ((from - base) / PI).floor() as i64;
            let k1 = ((to - base) / PI).ceil() as i64;
            for k in k0..=k1 {
                let t = base + k as f64 * PI;
                if t > from && t < to {
                    vals.push(along(t));
                }
            }
            let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            (lo, hi)
        };
        if !(lo.is_finite() && hi.is_finite()) || hi - lo <= tol.linear {
            return Err(unsupported(tr!("sketch-err-curve-perpendicular")));
        }
        Ok(Projected::Line(center + u * lo, center + u * hi))
    }

    /// Projiziert das Stück `t0 … t1` der Ellipse `curve` mit `(Rahmen, große, kleine
    /// Halbachse)` = `shape` (Kreise: beide Halbachsen `r`): ganz (`full`) als Ellipse, sonst
    /// als Ellipsenbogen.
    fn project_ellipse_piece(
        &self,
        curve: &Curve,
        (cf, major, minor): (&Frame, f64, f64),
        t0: f64,
        t1: f64,
        full: bool,
    ) -> Result<Projected, SketchError> {
        let (center, axis, short, turn) = self.project_ellipse(cf, major, minor)?;
        if full {
            return Ok(Projected::Ellipse {
                center,
                axis,
                minor: short,
            });
        }
        let frame = *self.frame();
        let project = |t: f64| frame.to_local(curve.point(t)).truncate();
        let (p0, p1) = (project(t0), project(t1));
        // Gegen den Uhrzeigersinn von `p0` nach `p1`, wenn Umlaufsinn und `t1 − t0` dasselbe
        // Vorzeichen haben.
        let ccw = (t1 - t0).signum() * turn.signum() > 0.0;
        let (start, end) = if ccw { (p0, p1) } else { (p1, p0) };
        Ok(Projected::EllipticArc {
            center,
            axis,
            minor: short,
            start,
            end,
        })
    }

    /// Projiziert die ganze Ellipse `cf.origin + major·cos t·cf.x + minor·sin t·cf.y` (Kreise:
    /// `major = minor = r`). Die Projektion ist `c + A·cos t + B·sin t`; ihre Hauptachsen
    /// ergeben sich durch Verschieben des Parameters um
    /// `φ = ½·atan2(2·A·B, |A|² − |B|²)`, die längere wird die große Halbachse. Liefert Mitte,
    /// große Halbachse (Vektor), kleine Halbachse und den Umlaufsinn `A × B` (positiv: gegen
    /// den Uhrzeigersinn bei wachsendem `t`).
    fn project_ellipse(
        &self,
        cf: &Frame,
        major: f64,
        minor: f64,
    ) -> Result<(DVec2, DVec2, f64, f64), SketchError> {
        let frame = *self.frame();
        let tol = *self.tolerance();
        let center = frame.to_local(cf.origin).truncate();
        let in_plane = |v: DVec3| DVec2::new(v.dot(frame.x), v.dot(frame.y));
        let big_a = in_plane(cf.x) * major;
        let big_b = in_plane(cf.y) * minor;
        let phi =
            0.5 * (2.0 * big_a.dot(big_b)).atan2(big_a.length_squared() - big_b.length_squared());
        let (s, c) = phi.sin_cos();
        let a = big_a * c + big_b * s;
        let b = -big_a * s + big_b * c;
        let (axis, short) = if b.length() > a.length() {
            (b, a.length())
        } else {
            (a, b.length())
        };
        if !(axis.is_finite() && short.is_finite()) || short <= tol.linear {
            return Err(unsupported(tr!("sketch-err-curve-perpendicular")));
        }
        Ok((center, axis, short, big_a.perp_dot(big_b)))
    }

    /// Neuer Punkt an `xy` mit `Fixed`-Abhängigkeit, wahlweise als Konstruktionspunkt.
    fn add_fixed_point(&mut self, xy: DVec2, construction: bool) -> Result<PointId, SketchError> {
        let p = self.add_point(xy.x, xy.y)?;
        self.points[p].construction = construction;
        self.add_constraint(ConstraintKind::Fixed(p, xy.x, xy.y))?;
        Ok(p)
    }
}

/// Ebene einer ebenen Fläche: Ursprung der Flächenebene, `z` = nach außen zeigende Normale.
/// `None` für gekrümmte Flächen oder unbekannte Ids.
pub fn plane_of_face(solid: &Solid, face: Id<Face>) -> Option<Frame> {
    let f = solid.faces.get(face)?;
    let Surface::Plane { frame } = &f.surface else {
        return None;
    };
    let z = if f.same_sense { frame.z } else { -frame.z };
    let x = frame.x;
    Some(Frame {
        origin: frame.origin,
        x,
        y: z.cross(x),
        z,
    })
}
