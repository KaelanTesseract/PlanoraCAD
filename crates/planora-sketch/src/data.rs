//! Rohdaten einer Skizze zum Speichern und Laden. Alle Einträge der Arenen werden in ihrer
//! Reihenfolge übertragen (auch gelöschte), damit Ids nach dem Laden dieselben sind.

use crate::constraint::{ConstraintData, ConstraintId, ConstraintKind};
use crate::error::SketchError;
use crate::model::{
    CurveData, CurveId, CurveKind, MAX_COORD, MAX_SPLINE_POINTS, PointData, PointId, Sketch, finite,
};
use crate::pattern::{
    CircularMode, PatternData, PatternDir, PatternInstance, PatternKind, PatternProblem,
    PatternSlot,
};
use crate::solver::SolveCache;
use planora_i18n::tr;
use planora_kernel::Arena;
use planora_math::{Frame, Tolerance};
use std::collections::HashSet;

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
    pub centerline: bool,
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

/// Ein Muster wie [`PatternData`], mit `alive`: Aufgelöste Muster bleiben in den Rohdaten, damit
/// die Ids der Musterwerte nach dem Laden dieselben sind.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternRecord {
    pub kind: PatternKind,
    pub sources: Vec<CurveId>,
    pub source_points: Vec<PointId>,
    pub values: Vec<ConstraintId>,
    pub instances: Vec<PatternInstance>,
    pub suppressed: Vec<[i32; 2]>,
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
    /// Muster, auch aufgelöste (Skizzen 5); leer in Skizzen ohne Muster.
    pub patterns: Vec<PatternRecord>,
}

fn bad(msg: impl Into<String>) -> SketchError {
    SketchError::InvalidInput(msg.into())
}

impl Sketch {
    /// Arten aller Kurveneinträge in Arena-Reihenfolge, auch gelöschter (wie in
    /// [`Sketch::to_data`]).
    pub fn curve_entries(&self) -> impl Iterator<Item = &CurveKind> {
        self.curves.iter().map(|(_, c)| &c.kind)
    }

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
                    centerline: c.centerline,
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
            patterns: self
                .patterns
                .iter()
                .map(|(_, m)| PatternRecord {
                    kind: m.kind,
                    sources: m.sources.clone(),
                    source_points: m.source_points.clone(),
                    values: m.values.clone(),
                    instances: m.instances.clone(),
                    suppressed: m.suppressed.clone(),
                    alive: m.alive,
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
            return Err(bad(tr!("sketch-err-data-tolerance")));
        }
        for v in [d.frame.origin, d.frame.x, d.frame.y, d.frame.z] {
            if !v.is_finite() || v.abs().max_element() > MAX_COORD {
                return Err(bad(tr!("sketch-err-data-frame")));
            }
        }
        if !d.frame.is_orthonormal(d.tolerance.linear) {
            return Err(bad(tr!("sketch-err-data-frame")));
        }
        for v in &d.vars {
            finite(*v, || tr!("sketch-err-what-sketch-value"))?;
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
                return Err(bad(tr!("sketch-err-data-point-value")));
            }
        }
        for c in &d.curves {
            let pts = c.kind.points();
            if pts.iter().any(|p| !point_ok(*p)) {
                return Err(bad(tr!("sketch-err-data-curve-point")));
            }
            if c.centerline && !(c.construction && matches!(c.kind, CurveKind::Line { .. })) {
                return Err(bad(tr!("sketch-err-data-centerline")));
            }
            match &c.kind {
                CurveKind::Circle { radius, .. } if !var_ok(*radius) => {
                    return Err(bad(tr!("sketch-err-data-circle-value")));
                }
                CurveKind::Ellipse { ax, ay, minor, .. }
                | CurveKind::EllipticArc { ax, ay, minor, .. }
                    if !(var_ok(*ax) && var_ok(*ay) && var_ok(*minor)) =>
                {
                    return Err(bad(tr!("sketch-err-data-ellipse-value")));
                }
                CurveKind::Spline { points }
                    if points.len() < 2 || points.len() > MAX_SPLINE_POINTS =>
                {
                    return Err(bad(tr!("sketch-err-data-spline-count")));
                }
                _ => {}
            }
            if c.alive && pts.iter().any(|p| !point_alive(*p)) {
                return Err(bad(tr!("sketch-err-data-curve-deleted")));
            }
        }
        for c in &d.constraints {
            if !(-1..=1).contains(&c.branch) {
                return Err(bad(tr!("sketch-err-data-constraint-place")));
            }
            if let Some(e) = c.extra
                && !var_ok(e)
            {
                return Err(bad(tr!("sketch-err-data-constraint-value")));
            }
            let pts = c.kind.points();
            let curves = c.kind.curves();
            let pattern_ok = match c.kind {
                ConstraintKind::PatternValue(m, ..) => m.index() < d.patterns.len(),
                _ => true,
            };
            if pts.iter().any(|p| !point_ok(*p))
                || curves.iter().any(|k| !curve_ok(*k))
                || !pattern_ok
            {
                return Err(bad(tr!("sketch-err-data-constraint-object")));
            }
            if !constraint_numbers_ok(&c.kind) {
                return Err(bad(tr!("sketch-err-data-constraint-number")));
            }
            if c.alive
                && (pts.iter().any(|p| !point_alive(*p)) || curves.iter().any(|k| !curve_alive(*k)))
            {
                return Err(bad(tr!("sketch-err-data-constraint-deleted")));
            }
        }
        check_patterns(&d)?;

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
                centerline: c.centerline,
                alive: c.alive,
            });
        }
        let mut constraints = Arena::new();
        for c in d.constraints {
            constraints.push(ConstraintData {
                kind: c.kind,
                branch: c.branch,
                shared_sign: 0,
                driven: c.driven,
                enabled: c.enabled,
                internal: c.internal,
                extra: c.extra,
                alive: c.alive,
            });
        }
        let mut patterns = Arena::new();
        for m in d.patterns {
            patterns.push(PatternData {
                kind: m.kind,
                sources: m.sources,
                source_points: m.source_points,
                values: m.values,
                instances: m.instances,
                suppressed: m.suppressed,
                alive: m.alive,
            });
        }
        let mut s = Sketch::new(d.frame, d.name, d.tolerance);
        s.vars = d.vars;
        s.points = points;
        s.curves = curves;
        s.constraints = constraints;
        s.patterns = patterns;
        s.solve_cache = SolveCache::default();
        s.touch_structure();
        // Typen und Entartungen aller Abhängigkeiten (auch gelöschter) wie bei add_constraint
        // prüfen, sonst könnten spätere Solver-Läufe auf falschen Objekttypen abstürzen.
        for (_, c) in s.constraints.iter() {
            s.check_types(&c.kind)?;
            if c.alive {
                if c.extra.is_none() && s.needs_extra(&c.kind) {
                    return Err(bad(tr!("sketch-err-data-constraint-extra")));
                }
                s.check_constraint_value(&c.kind)?;
            }
        }
        // Keine Abhängigkeit darf Vorlage und eigenes Exemplar eines Musters verbinden, sonst
        // liefe der Löser nie zusammen (wie bei `add_constraint`).
        if s.pattern_cycle(None) {
            return Err(PatternProblem::Cycle.error());
        }
        // Die Tangenten an gemeinsamen Enden bleiben, wie die gespeicherte Geometrie sie zeigt.
        s.refresh_shared_signs(None);
        Ok(s)
    }
}

/// Prüft die Muster der Rohdaten. Indizes vorhanden, auch bei aufgelösten Mustern. Bei lebenden
/// Mustern alles, was Löser und Nachführung voraussetzen: Vorlagen, Richtungslinien, Mitte und
/// Exemplare leben; die Vorlagenpunkte sind die Punkte der Vorlagen; jedes Exemplar hat so viele
/// Punkte und Kurven wie die Vorlage, an einer eigenen Lage außerhalb der Abwahl; die Werte sind
/// lebende Musterwerte genau dieses Musters, je Feld einer, alle Felder der Art; kein Exemplar
/// ist zugleich Vorlage, Richtungslinie oder Mitte. Jeder lebende Musterwert gehört zu einem
/// lebenden Muster, das ihn führt.
fn check_patterns(d: &SketchData) -> Result<(), SketchError> {
    let (np, nc, nk) = (d.points.len(), d.curves.len(), d.constraints.len());
    for m in &d.patterns {
        let refs_ok = match m.kind {
            PatternKind::Rect { dirs, .. } => dirs
                .iter()
                .all(|dir| !matches!(dir, PatternDir::Line(l) if l.index() >= nc)),
            PatternKind::Circular { center, .. } => center.index() < np,
        };
        let points_ok = m
            .source_points
            .iter()
            .chain(m.instances.iter().flat_map(|i| &i.points))
            .all(|p| p.index() < np);
        let curves_ok = m
            .sources
            .iter()
            .chain(m.instances.iter().flat_map(|i| &i.curves))
            .all(|c| c.index() < nc);
        if !(refs_ok && points_ok && curves_ok && m.values.iter().all(|c| c.index() < nk)) {
            return Err(bad(tr!("sketch-err-data-pattern-object")));
        }
    }

    let deleted = || bad(tr!("sketch-err-data-pattern-deleted"));
    let instance = || bad(tr!("sketch-err-data-pattern-instance"));
    let value = || bad(tr!("sketch-err-data-pattern-value"));
    let point_alive = |p: PointId| d.points[p.index()].alive;
    let curve_alive = |c: CurveId| d.curves[c.index()].alive;
    // Teile der Exemplare lebender Muster, danach gegen Vorlagen, Richtungen und Mitten geprüft.
    let mut inst_curves: HashSet<CurveId> = HashSet::new();
    let mut inst_points: HashSet<PointId> = HashSet::new();
    for (k, m) in d.patterns.iter().enumerate().filter(|(_, m)| m.alive) {
        // Vorlagen: lebend, ohne Doppelte; ihre Punkte genau `source_points`.
        let mut own_points: Vec<PointId> = Vec::new();
        for (i, &c) in m.sources.iter().enumerate() {
            if m.sources[..i].contains(&c) {
                return Err(instance());
            }
            if !curve_alive(c) {
                return Err(deleted());
            }
            for p in d.curves[c.index()].kind.points() {
                if !own_points.contains(&p) {
                    own_points.push(p);
                }
            }
        }
        if m.sources.is_empty() || own_points != m.source_points {
            return Err(instance());
        }
        match m.kind {
            PatternKind::Rect { dirs, .. } => {
                for dir in dirs {
                    if let PatternDir::Line(l) = dir
                        && !(curve_alive(l)
                            && matches!(d.curves[l.index()].kind, CurveKind::Line { .. }))
                    {
                        return Err(deleted());
                    }
                }
            }
            PatternKind::Circular { center, .. } => {
                if !point_alive(center) {
                    return Err(deleted());
                }
            }
        }
        // Exemplare: so viele Punkte und Kurven wie die Vorlage, lebend, je eine eigene Lage.
        let mut lattice: HashSet<[i32; 2]> = HashSet::new();
        for inst in &m.instances {
            if inst.points.len() != m.source_points.len()
                || inst.curves.len() != m.sources.len()
                || inst.at == [0, 0]
                || m.suppressed.contains(&inst.at)
                || !lattice.insert(inst.at)
            {
                return Err(instance());
            }
            if !(inst.points.iter().all(|&p| point_alive(p))
                && inst.curves.iter().all(|&c| curve_alive(c)))
            {
                return Err(deleted());
            }
            if !(inst.curves.iter().all(|&c| inst_curves.insert(c))
                && inst.points.iter().all(|&p| inst_points.insert(p)))
            {
                return Err(instance());
            }
        }
        // Werte: lebende Musterwerte dieses Musters, je Feld einer, alle Felder der Art.
        let mut slots: Vec<PatternSlot> = Vec::new();
        for &c in &m.values {
            let r = &d.constraints[c.index()];
            match r.kind {
                ConstraintKind::PatternValue(owner, slot, _)
                    if owner.index() == k && r.alive && !slots.contains(&slot) =>
                {
                    slots.push(slot);
                }
                _ => return Err(value()),
            }
        }
        let wanted: &[PatternSlot] = match m.kind {
            PatternKind::Rect { .. } => &[
                PatternSlot::Count1,
                PatternSlot::Spacing1,
                PatternSlot::Count2,
                PatternSlot::Spacing2,
            ],
            PatternKind::Circular {
                mode: CircularMode::Full,
                ..
            } => &[PatternSlot::Count],
            PatternKind::Circular { .. } => &[PatternSlot::Count, PatternSlot::Angle],
        };
        if slots.len() != wanted.len() || !wanted.iter().all(|w| slots.contains(w)) {
            return Err(value());
        }
    }
    for m in d.patterns.iter().filter(|m| m.alive) {
        let used = match m.kind {
            PatternKind::Rect { dirs, .. } => dirs
                .iter()
                .any(|dir| matches!(dir, PatternDir::Line(l) if inst_curves.contains(l))),
            PatternKind::Circular { center, .. } => inst_points.contains(&center),
        };
        if used
            || m.sources.iter().any(|c| inst_curves.contains(c))
            || m.source_points.iter().any(|p| inst_points.contains(p))
        {
            return Err(PatternProblem::InstanceAsSource.error());
        }
    }
    for (i, c) in d.constraints.iter().enumerate() {
        if let ConstraintKind::PatternValue(m, ..) = c.kind
            && c.alive
        {
            let owner = &d.patterns[m.index()];
            if !(owner.alive && owner.values.iter().any(|v| v.index() == i)) {
                return Err(value());
            }
        }
    }
    Ok(())
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
        | EllipseAngle(_, v)
        | DistanceX(_, _, v)
        | DistanceY(_, _, v)
        | CenterlineDiameter(_, _, v)
        | SlotOverall(_, _, _, v)
        | PatternValue(_, _, v) => ok(*v),
        _ => true,
    }
}
