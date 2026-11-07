//! Zeichenhilfen: Rechteck, Spiegeln, Muster, Körperkanten und Kurven projizieren, Ebene einer
//! Fläche.
//!
//! Alle Hilfen sind atomar: Bei einem Fehler bleibt die Skizze unverändert.

use crate::constraint::{ConstraintKind, LineOrPoints};
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, PointId, Sketch, finite};
use planora_i18n::tr;
use planora_kernel::{Curve, Edge, Face, Id, Solid, Surface};
use planora_math::{DVec2, DVec3, Frame, TAU};
use std::collections::HashMap;
use std::collections::hash_map::Entry;

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
#[derive(Clone, Copy, Debug)]
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

    /// Prüft die Quellkurven für Spiegeln und Muster: vorhanden, ohne Doppelte, nur Linien,
    /// Kreise, Bögen und Ellipsenbögen.
    fn check_copy_sources(&self, curves: &[CurveId]) -> Result<(), SketchError> {
        if curves.is_empty() {
            return Err(SketchError::InvalidInput(tr!("sketch-nothing-selected")));
        }
        for (i, &c) in curves.iter().enumerate() {
            self.check_curve(c)?;
            if curves[..i].contains(&c) {
                return Err(SketchError::InvalidInput(tr!("sketch-err-object-twice")));
            }
            if matches!(
                self.curves[c].kind,
                CurveKind::Ellipse { .. } | CurveKind::Spline { .. }
            ) {
                return Err(SketchError::WrongType(tr!("sketch-err-copy-kinds")));
            }
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
    /// gespiegelt. Nur Muster (`CopyHow::Pattern`) kopieren auch Ellipsen und Splines; Radius und
    /// Halbachsen nehmen den Maßstab, die große Halbachse die Drehung der Lage mit. Gibt die neue
    /// Kurve zurück.
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
            } if matches!(how, CopyHow::Pattern(_)) => {
                let CopyHow::Pattern(t) = how else {
                    unreachable!()
                };
                let major = t.linear(DVec2::new(self.vars[ax], self.vars[ay]));
                self.add_ellipse(map[&center], major, self.vars[minor] * factor)?
            }
            CurveKind::Spline { points } if matches!(how, CopyHow::Pattern(_)) => {
                let mapped: Vec<PointId> = points.iter().map(|p| map[p]).collect();
                self.add_spline(&mapped)?
            }
            CurveKind::Ellipse { .. } | CurveKind::Spline { .. } => {
                return Err(SketchError::WrongType(tr!("sketch-err-copy-kinds")));
            }
        };
        self.curves[id].construction = construction;
        self.curves[id].centerline = centerline;
        Ok(id)
    }

    /// Neuer Punkt an `xy` mit der Konstruktionseigenschaft von `like`.
    pub(crate) fn add_point_like(
        &mut self,
        like: PointId,
        xy: DVec2,
    ) -> Result<PointId, SketchError> {
        let p = self.add_point(xy.x, xy.y)?;
        self.points[p].construction = self.points[like].construction;
        Ok(p)
    }

    /// Spiegelt Linien, Kreise, Bögen und Ellipsenbögen an der Linie `axis`. Jeder Punkt bekommt
    /// ein Spiegelbild mit `Symmetric`-Abhängigkeit (Punkte auf der Achse werden
    /// wiederverwendet), Kreise ein `Equal` zum Original. Bögen und Ellipsenbögen werden mit
    /// vertauschtem Start und Ende angelegt; die Halbachsen eines gespiegelten Ellipsenbogens
    /// sind nicht an das Original gebunden.
    pub fn add_mirrored(
        &mut self,
        curves: &[CurveId],
        axis: CurveId,
    ) -> Result<Vec<CurveId>, SketchError> {
        self.check_curve(axis)?;
        let CurveKind::Line { a: aa, b: ab } = self.curves[axis].kind else {
            return Err(SketchError::WrongType(tr!("sketch-err-mirror-line")));
        };
        self.check_copy_sources(curves)?;
        if curves.contains(&axis) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-mirror-self")));
        }
        let (origin, dir) = (self.pos(aa), self.pos(ab) - self.pos(aa));
        if dir.length() <= self.tolerance().linear {
            return Err(SketchError::Degenerate(tr!("sketch-err-mirror-zero")));
        }
        let d = dir.normalize();
        self.atomically(|s| {
            let tol = s.tolerance().linear;
            let mut map: HashMap<PointId, PointId> = HashMap::new();
            let mut out = Vec::with_capacity(curves.len());
            for &c in curves {
                for p in s.curves[c].kind.points() {
                    if map.contains_key(&p) {
                        continue;
                    }
                    let xy = s.pos(p);
                    let on_axis = (xy - origin).perp_dot(d).abs() <= tol;
                    if on_axis {
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
                out.push(copy);
            }
            Ok(out)
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
        self.check_copy_sources(curves)?;
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
                let mut group = Vec::with_capacity(curves.len());
                for &c in curves {
                    for p in s.curves[c].kind.points() {
                        if let Entry::Vacant(slot) = map.entry(p) {
                            slot.insert(s.add_point_like(p, s.pos(p) + shift)?);
                        }
                    }
                    let copy = s.copy_curve(c, &map, CopyHow::Shift)?;
                    // `Equal` gibt es nur für Linien, Kreise und Bögen.
                    if !matches!(s.curves[c].kind, CurveKind::EllipticArc { .. }) {
                        created.push(s.add_constraint(ConstraintKind::Equal(c, copy))?);
                    }
                    if matches!(s.curves[c].kind, CurveKind::Line { .. }) {
                        created.push(s.add_constraint(ConstraintKind::Parallel(c, copy))?);
                    }
                    group.push(copy);
                }
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
    /// als feste Geometrie an: Linie, Kreis, Bogen, Ellipse oder Ellipsenbogen (schräger
    /// Kreisbogen, Teilstück einer Ellipse). Die Punkte bekommen `Fixed`-Werte, Kreise einen
    /// `Radius`, Ellipsen und Ellipsenbögen `EllipseMajor`, `EllipseMinor` und `EllipseAngle`.
    /// Mit `construction: false` sind Kurve und Punkte normale Geometrie (verknüpfte Projektion,
    /// bildet Profile). Freiformkurven und Stücke, die nur ein Punkt oder eine Strecke wären,
    /// werden abgelehnt; die Skizze bleibt dann unverändert.
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
                        minor: mi,
                        start: a,
                        end: b,
                    });
                    s.add_ellipse_values(e, axis, minor)?;
                    s.push_internal(ConstraintKind::EllipticArcEnds(e));
                    e
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

    /// Gibt der Kurve `curve` die neue Lage `projected` (siehe [`Sketch::reproject_curve`]).
    fn reproject(&mut self, curve: CurveId, projected: Projected) -> Result<(), SketchError> {
        self.atomically(|s| {
            let kind = s.curves[curve].kind.clone();
            // Neue Maße, die in den Abhängigkeiten nachgeführt werden.
            let mut radius_value = None;
            let mut ellipse_values = None;
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
                    },
                ) => {
                    ellipse_values = Some(s.set_ellipse_axes((ax, ay, mi), axis, minor)?);
                    s.set_point_xy(m, center.x, center.y)?;
                    s.set_point_xy(a, start.x, start.y)?;
                    s.set_point_xy(b, end.x, end.y)?;
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
    /// senkrecht zur Skizzenebene. Andere Kurven liefern `false`.
    pub fn projects_to_point(&self, curve: &Curve, t0: f64, t1: f64) -> bool {
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
            } => self.project_ellipse_piece(curve, (cf, *major, *minor), t0, t1, full),
            Curve::Nurbs(_) => Err(unsupported(tr!("sketch-err-freeform-edge"))),
        }
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
