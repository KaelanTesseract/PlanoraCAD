//! Rohdaten einer Skizze zum Speichern und Laden. Alle Einträge der Arenen werden in ihrer
//! Reihenfolge übertragen (auch gelöschte), damit Ids nach dem Laden dieselben sind.

use crate::constraint::{ConstraintData, ConstraintKind};
use crate::error::SketchError;
use crate::model::{
    CurveData, CurveId, CurveKind, MAX_COORD, MAX_SPLINE_POINTS, PointData, PointId, Sketch, finite,
};
use crate::solver::SolveCache;
use planora_kernel::Arena;
use planora_math::{Frame, Tolerance};

#[derive(Clone, Debug, PartialEq)]
pub struct PointRecord {
    pub x: usize,
    pub y: usize,
    pub construction: bool,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveRecord {
    pub kind: CurveKind,
    pub construction: bool,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstraintRecord {
    pub kind: ConstraintKind,
    pub branch: i8,
    pub driven: bool,
    pub enabled: bool,
    pub internal: bool,
    pub extra: Option<usize>,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SketchData {
    pub name: String,
    pub frame: Frame,
    pub tolerance: Tolerance,
    pub vars: Vec<f64>,
    pub points: Vec<PointRecord>,
    pub curves: Vec<CurveRecord>,
    pub constraints: Vec<ConstraintRecord>,
}

fn bad(msg: impl Into<String>) -> SketchError {
    SketchError::InvalidInput(msg.into())
}

impl Sketch {
    pub fn to_data(&self) -> SketchData {
        SketchData {
            name: self.name.clone(),
            frame: *self.frame(),
            tolerance: *self.tolerance(),
            vars: self.vars.clone(),
            points: self
                .points
                .iter()
                .map(|(_, p)| PointRecord {
                    x: p.x,
                    y: p.y,
                    construction: p.construction,
                    alive: p.alive,
                })
                .collect(),
            curves: self
                .curves
                .iter()
                .map(|(_, c)| CurveRecord {
                    kind: c.kind.clone(),
                    construction: c.construction,
                    alive: c.alive,
                })
                .collect(),
            constraints: self
                .constraints
                .iter()
                .map(|(_, c)| ConstraintRecord {
                    kind: c.kind.clone(),
                    branch: c.branch,
                    driven: c.driven,
                    enabled: c.enabled,
                    internal: c.internal,
                    extra: c.extra,
                    alive: c.alive,
                })
                .collect(),
        }
    }

    /// Baut eine Skizze aus Rohdaten auf. Prüft alles (Zahlen endlich und im Bereich, Indizes
    /// vorhanden, lebende Objekte verweisen nur auf lebende); löst und rechnet nichts.
    pub fn from_data(d: SketchData) -> Result<Sketch, SketchError> {
        if !(d.tolerance.linear.is_finite()
            && d.tolerance.linear > 0.0
            && d.tolerance.angular.is_finite()
            && d.tolerance.angular > 0.0)
        {
            return Err(bad("Die Toleranz der Skizze ist ungültig"));
        }
        for v in [d.frame.origin, d.frame.x, d.frame.y, d.frame.z] {
            if !v.is_finite() || v.abs().max_element() > MAX_COORD {
                return Err(bad("Das Koordinatensystem der Skizze ist ungültig"));
            }
        }
        if !d.frame.is_orthonormal(d.tolerance.linear) {
            return Err(bad("Das Koordinatensystem der Skizze ist ungültig"));
        }
        for v in &d.vars {
            finite(*v, "Ein Wert der Skizze")?;
        }
        let nv = d.vars.len();
        let var_ok = |i: usize| i < nv;
        let np = d.points.len();
        let nc = d.curves.len();
        let point_ok = |p: PointId| p.index() < np;
        let curve_ok = |c: CurveId| c.index() < nc;
        let point_alive = |p: PointId| d.points[p.index()].alive;
        let curve_alive = |c: CurveId| d.curves[c.index()].alive;

        for p in &d.points {
            if !var_ok(p.x) || !var_ok(p.y) {
                return Err(bad("Ein Punkt verweist auf einen fehlenden Wert"));
            }
        }
        for c in &d.curves {
            let pts = c.kind.points();
            if pts.iter().any(|p| !point_ok(*p)) {
                return Err(bad("Eine Kurve verweist auf einen fehlenden Punkt"));
            }
            match &c.kind {
                CurveKind::Circle { radius, .. } if !var_ok(*radius) => {
                    return Err(bad("Ein Kreis verweist auf einen fehlenden Wert"));
                }
                CurveKind::Ellipse { ax, ay, minor, .. }
                    if !(var_ok(*ax) && var_ok(*ay) && var_ok(*minor)) =>
                {
                    return Err(bad("Eine Ellipse verweist auf einen fehlenden Wert"));
                }
                CurveKind::Spline { points }
                    if points.len() < 2 || points.len() > MAX_SPLINE_POINTS =>
                {
                    return Err(bad("Ein Spline hat eine ungültige Zahl von Stützpunkten"));
                }
                _ => {}
            }
            if c.alive && pts.iter().any(|p| !point_alive(*p)) {
                return Err(bad("Eine Kurve verweist auf einen gelöschten Punkt"));
            }
        }
        for c in &d.constraints {
            if !(-1..=1).contains(&c.branch) {
                return Err(bad("Ungültige Lage einer Abhängigkeit"));
            }
            if let Some(e) = c.extra
                && !var_ok(e)
            {
                return Err(bad("Eine Abhängigkeit verweist auf einen fehlenden Wert"));
            }
            let pts = c.kind.points();
            let curves = c.kind.curves();
            if pts.iter().any(|p| !point_ok(*p)) || curves.iter().any(|k| !curve_ok(*k)) {
                return Err(bad("Eine Abhängigkeit verweist auf ein fehlendes Objekt"));
            }
            if !constraint_numbers_ok(&c.kind) {
                return Err(bad("Eine Abhängigkeit hat einen ungültigen Zahlenwert"));
            }
            if c.alive
                && (pts.iter().any(|p| !point_alive(*p)) || curves.iter().any(|k| !curve_alive(*k)))
            {
                return Err(bad("Eine Abhängigkeit verweist auf ein gelöschtes Objekt"));
            }
        }

        let mut points = Arena::new();
        for p in d.points {
            points.push(PointData {
                x: p.x,
                y: p.y,
                construction: p.construction,
                alive: p.alive,
            });
        }
        let mut curves = Arena::new();
        for c in d.curves {
            curves.push(CurveData {
                kind: c.kind,
                construction: c.construction,
                alive: c.alive,
            });
        }
        let mut constraints = Arena::new();
        for c in d.constraints {
            constraints.push(ConstraintData {
                kind: c.kind,
                branch: c.branch,
                driven: c.driven,
                enabled: c.enabled,
                internal: c.internal,
                extra: c.extra,
                alive: c.alive,
            });
        }
        let mut s = Sketch::new(d.frame, d.name, d.tolerance);
        s.vars = d.vars;
        s.points = points;
        s.curves = curves;
        s.constraints = constraints;
        s.solve_cache = SolveCache::default();
        s.touch_structure();
        // Typen und Entartungen aller Abhängigkeiten (auch gelöschter) wie bei add_constraint
        // prüfen, sonst könnten spätere Solver-Läufe auf falschen Objekttypen abstürzen.
        for (_, c) in s.constraints.iter() {
            s.check_types(&c.kind)?;
            if c.alive {
                s.check_constraint_value(&c.kind)?;
            }
        }
        Ok(s)
    }
}

/// Alle Zahlen einer Abhängigkeit endlich und im Bereich (Winkel und Maße).
fn constraint_numbers_ok(k: &ConstraintKind) -> bool {
    use ConstraintKind::*;
    let ok = |v: f64| v.is_finite() && v.abs() <= MAX_COORD;
    match k {
        Fixed(_, x, y) => ok(*x) && ok(*y),
        Distance(_, _, v)
        | DistancePointLine(_, _, v)
        | Radius(_, v)
        | Diameter(_, v)
        | Angle(_, _, v)
        | EllipseMajor(_, v)
        | EllipseMinor(_, v)
        | EllipseAngle(_, v) => ok(*v),
        _ => true,
    }
}
