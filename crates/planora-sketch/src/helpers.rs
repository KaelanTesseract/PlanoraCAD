//! Zeichenhilfen: Rechteck, Spiegeln, Muster, Körperkanten kopieren, Ebene einer Fläche.
//!
//! Alle Hilfen sind atomar: Bei einem Fehler bleibt die Skizze unverändert.

use crate::constraint::{ConstraintKind, LineOrPoints};
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, PointId, Sketch, finite};
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

/// Eine in die Skizzenebene projizierte Körperkante (Werte in Skizzenkoordinaten).
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
}

fn unsupported(what: &str) -> SketchError {
    SketchError::UnsupportedProjection(what.into())
}

impl Sketch {
    /// Führt `f` aus; bei einem Fehler wird der vorherige Zustand wiederhergestellt.
    fn atomically<T>(
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
    /// Kreise und Bögen.
    fn check_copy_sources(&self, curves: &[CurveId]) -> Result<(), SketchError> {
        if curves.is_empty() {
            return Err(SketchError::InvalidInput("Es ist nichts ausgewählt".into()));
        }
        for (i, &c) in curves.iter().enumerate() {
            self.check_curve(c)?;
            if curves[..i].contains(&c) {
                return Err(SketchError::InvalidInput(
                    "Ein Objekt ist mehrfach ausgewählt".into(),
                ));
            }
            if matches!(
                self.curves[c].kind,
                CurveKind::Ellipse { .. } | CurveKind::Spline { .. }
            ) {
                return Err(SketchError::WrongType(
                    "Nur Linien, Kreise und Bögen können kopiert werden".into(),
                ));
            }
        }
        Ok(())
    }

    /// Achsenparalleles Rechteck mit den gegenüberliegenden Ecken `a` und `b`.
    pub fn add_rectangle(&mut self, a: DVec2, b: DVec2) -> Result<RectIds, SketchError> {
        if !(a.is_finite() && b.is_finite()) {
            return Err(SketchError::InvalidInput(
                "Die Ecken müssen endliche Zahlen sein".into(),
            ));
        }
        let (min, max) = (a.min(b), a.max(b));
        let size = max - min;
        if size.x <= self.tolerance().linear || size.y <= self.tolerance().linear {
            return Err(SketchError::Degenerate(
                "Breite und Höhe des Rechtecks müssen größer als null sein".into(),
            ));
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

    /// Legt eine Kopie von `kind` an, deren Punkte über `map` abgebildet werden. Bögen werden
    /// bei `swap_arc` mit vertauschtem Start und Ende angelegt. Gibt die neue Kurve zurück.
    fn copy_curve(
        &mut self,
        source: CurveId,
        map: &HashMap<PointId, PointId>,
        swap_arc: bool,
    ) -> Result<CurveId, SketchError> {
        let construction = self.curves[source].construction;
        let id = match self.curves[source].kind.clone() {
            CurveKind::Line { a, b } => self.add_line(map[&a], map[&b])?,
            CurveKind::Circle { center, radius } => {
                let r = self.vars[radius];
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
            CurveKind::Ellipse { .. } | CurveKind::Spline { .. } => {
                return Err(SketchError::WrongType(
                    "Nur Linien, Kreise und Bögen können kopiert werden".into(),
                ));
            }
        };
        self.curves[id].construction = construction;
        Ok(id)
    }

    /// Neuer Punkt an `xy` mit der Konstruktionseigenschaft von `like`.
    fn add_point_like(&mut self, like: PointId, xy: DVec2) -> Result<PointId, SketchError> {
        let p = self.add_point(xy.x, xy.y)?;
        self.points[p].construction = self.points[like].construction;
        Ok(p)
    }

    /// Spiegelt Linien, Kreise und Bögen an der Linie `axis`. Jeder Punkt bekommt ein Spiegelbild
    /// mit `Symmetric`-Abhängigkeit (Punkte auf der Achse werden wiederverwendet), Kreise ein
    /// `Equal` zum Original. Bögen werden mit vertauschtem Start und Ende angelegt.
    pub fn add_mirrored(
        &mut self,
        curves: &[CurveId],
        axis: CurveId,
    ) -> Result<Vec<CurveId>, SketchError> {
        self.check_curve(axis)?;
        let CurveKind::Line { a: aa, b: ab } = self.curves[axis].kind else {
            return Err(SketchError::WrongType(
                "Die Spiegelachse muss eine Linie sein".into(),
            ));
        };
        self.check_copy_sources(curves)?;
        if curves.contains(&axis) {
            return Err(SketchError::InvalidInput(
                "Die Spiegelachse kann nicht selbst gespiegelt werden".into(),
            ));
        }
        let (origin, dir) = (self.pos(aa), self.pos(ab) - self.pos(aa));
        if dir.length() <= self.tolerance().linear {
            return Err(SketchError::Degenerate(
                "Die Spiegelachse hat die Länge null".into(),
            ));
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
                let copy = s.copy_curve(c, &map, true)?;
                if matches!(s.curves[c].kind, CurveKind::Circle { .. }) {
                    s.add_constraint(ConstraintKind::Equal(c, copy))?;
                }
                out.push(copy);
            }
            Ok(out)
        })
    }

    /// Lineares Muster: `count` zusätzliche Kopien, Kopie `k` um `k·offset` verschoben.
    pub fn add_linear_pattern(
        &mut self,
        curves: &[CurveId],
        offset: DVec2,
        count: usize,
    ) -> Result<Vec<Vec<CurveId>>, SketchError> {
        if !(1..=MAX_PATTERN_COPIES).contains(&count) {
            return Err(SketchError::InvalidInput(format!(
                "Die Anzahl der Kopien muss zwischen 1 und {MAX_PATTERN_COPIES} liegen"
            )));
        }
        if !offset.is_finite() {
            return Err(SketchError::InvalidInput(
                "Der Versatz muss aus endlichen Zahlen bestehen".into(),
            ));
        }
        let tol = self.tolerance().linear;
        if offset.length() <= tol {
            return Err(SketchError::Degenerate(
                "Der Versatz darf nicht null sein".into(),
            ));
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
                    let copy = s.copy_curve(c, &map, false)?;
                    created.push(s.add_constraint(ConstraintKind::Equal(c, copy))?);
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
    /// Skizze. Unterstützt Geraden (nicht senkrecht zur Skizzenebene) sowie Kreise und Bögen in
    /// Ebenen parallel zur Skizzenebene.
    pub fn add_projected_edge(
        &mut self,
        solid: &Solid,
        edge: Id<Edge>,
    ) -> Result<CurveId, SketchError> {
        let projected = self.project_edge(solid, edge)?;
        self.atomically(|s| {
            let c = match projected {
                Projected::Line(p0, p1) => {
                    let a = s.add_fixed_construction_point(p0)?;
                    let b = s.add_fixed_construction_point(p1)?;
                    s.add_line(a, b)?
                }
                Projected::Circle { center, radius } => {
                    let m = s.add_fixed_construction_point(center)?;
                    let c = s.add_circle(m, radius)?;
                    s.add_constraint(ConstraintKind::Radius(c, radius))?;
                    c
                }
                Projected::Arc { center, start, end } => {
                    let m = s.add_fixed_construction_point(center)?;
                    let a = s.add_fixed_construction_point(start)?;
                    let b = s.add_fixed_construction_point(end)?;
                    s.add_arc(m, a, b)?
                }
                Projected::Ellipse {
                    center,
                    axis,
                    minor,
                } => {
                    let m = s.add_fixed_construction_point(center)?;
                    let e = s.add_ellipse(m, axis, minor)?;
                    s.add_constraint(ConstraintKind::EllipseMajor(e, axis.length()))?;
                    s.add_constraint(ConstraintKind::EllipseMinor(e, minor))?;
                    s.add_constraint(ConstraintKind::EllipseAngle(e, axis.y.atan2(axis.x)))?;
                    e
                }
            };
            s.curves[c].construction = true;
            Ok(c)
        })
    }

    /// Führt die kopierte Körperkante `curve` nach: Die Kante wird wie bei
    /// [`Sketch::add_projected_edge`] projiziert, die vorhandene Kurve bekommt die neue Lage
    /// und Größe, ebenso die dazugehörigen `Fixed`-, `Radius`- und `Ellipse…`-Werte. Die Kurve
    /// behält ihre Id und alle Abhängigkeiten, die der Nutzer daran geknüpft hat. Ändert die
    /// Kante ihre Art oder lässt sie sich nicht projizieren, bleibt die Skizze unverändert.
    pub fn reproject_edge(
        &mut self,
        curve: CurveId,
        solid: &Solid,
        edge: Id<Edge>,
    ) -> Result<(), SketchError> {
        self.check_curve(curve)?;
        let projected = self.project_edge(solid, edge)?;
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
                    finite(radius, "Der Radius")?;
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
                    finite(axis.x, "Die große Halbachse")?;
                    finite(axis.y, "Die große Halbachse")?;
                    finite(minor, "Die kleine Halbachse")?;
                    s.set_point_xy(m, center.x, center.y)?;
                    (s.vars[ax], s.vars[ay], s.vars[mi]) = (axis.x, axis.y, minor);
                    ellipse_values = Some((axis.length(), minor, axis.y.atan2(axis.x)));
                }
                _ => return Err(unsupported("Die Kante hat ihre Art geändert")),
            }
            // Die Fixwerte und Maße, die `add_projected_edge` angelegt hat, mitführen.
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
            Ok(())
        })
    }

    /// Projiziert die Körperkante `edge` in die Skizzenebene, ohne die Skizze zu ändern.
    fn project_edge(&self, solid: &Solid, edge: Id<Edge>) -> Result<Projected, SketchError> {
        let e = solid.edges.get(edge).ok_or(SketchError::UnknownObject)?;
        let frame = *self.frame();
        let tol = *self.tolerance();
        let project = |p: DVec3| frame.to_local(p).truncate();
        if !(e.t0.is_finite() && e.t1.is_finite()) || (e.t1 - e.t0).abs() <= tol.angular {
            return Err(SketchError::Degenerate(
                "Die Kante hat keinen Parameterbereich".into(),
            ));
        }
        match &e.curve {
            Curve::Line { .. } => {
                let (p0, p1) = (project(e.curve.point(e.t0)), project(e.curve.point(e.t1)));
                if p0.distance(p1) <= tol.linear {
                    return Err(unsupported(
                        "Die Kante steht senkrecht zur Skizzenebene und wäre nur ein Punkt",
                    ));
                }
                Ok(Projected::Line(p0, p1))
            }
            Curve::Circle {
                frame: cf,
                radius: r,
            } => {
                let full = (e.t1 - e.t0).abs() >= TAU - tol.angular;
                if cf.z.cross(frame.z).length() > tol.angular {
                    if !full {
                        return Err(unsupported(
                            "Kreisbögen, die nicht parallel zur Skizzenebene liegen, werden nicht unterstützt",
                        ));
                    }
                    return self.project_ellipse(cf, *r, *r);
                }
                let center = project(cf.origin);
                if full {
                    Ok(Projected::Circle { center, radius: *r })
                } else {
                    let (p0, p1) = (project(e.curve.point(e.t0)), project(e.curve.point(e.t1)));
                    let ccw = (e.t1 - e.t0).signum() * cf.z.dot(frame.z).signum() > 0.0;
                    let (start, end) = if ccw { (p0, p1) } else { (p1, p0) };
                    Ok(Projected::Arc { center, start, end })
                }
            }
            Curve::Ellipse {
                frame: cf,
                major,
                minor,
            } => {
                if (e.t1 - e.t0).abs() < TAU - tol.angular {
                    return Err(unsupported(
                        "Ellipsenbögen werden nicht unterstützt, nur ganze Ellipsen",
                    ));
                }
                self.project_ellipse(cf, *major, *minor)
            }
        }
    }

    /// Projiziert die ganze Ellipse `cf.origin + major·cos t·cf.x + minor·sin t·cf.y` (Kreise:
    /// `major = minor = r`). Die Projektion ist `c + A·cos t + B·sin t`; ihre Hauptachsen
    /// ergeben sich durch Verschieben des Parameters um
    /// `φ = ½·atan2(2·A·B, |A|² − |B|²)`, die längere wird die große Halbachse.
    fn project_ellipse(
        &self,
        cf: &Frame,
        major: f64,
        minor: f64,
    ) -> Result<Projected, SketchError> {
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
            return Err(unsupported(
                "Die Kurve steht senkrecht zur Skizzenebene und wäre nur eine Strecke",
            ));
        }
        Ok(Projected::Ellipse {
            center,
            axis,
            minor: short,
        })
    }

    /// Neuer Konstruktionspunkt an `xy` mit `Fixed`-Abhängigkeit.
    fn add_fixed_construction_point(&mut self, xy: DVec2) -> Result<PointId, SketchError> {
        let p = self.add_point(xy.x, xy.y)?;
        self.points[p].construction = true;
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
