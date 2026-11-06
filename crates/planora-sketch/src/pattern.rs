//! Muster (Rechteckige und Kreisförmige Anordnung) mit gehaltenen Exemplaren.
//!
//! Ein Muster steht als eigene Art ([`PatternData`]) in der Skizze. Die Lage jedes Exemplars ist
//! eine reine Funktion der Vorlage und der Musterwerte ([`Sketch::pattern_transforms`]). Die
//! Variablen der Exemplare sind für den Löser Konstanten: keine Spalte, kein Freiheitsgrad. Vor
//! und nach jedem Lauf schreibt [`Sketch::refresh_patterns`] sie neu (Entscheidungen 1, 4 und 5 des
//! Plans Skizzen 5). Ein Muster kann die Skizze so nicht überbestimmen.
//!
//! Die Musterwerte (Anzahl, Abstand, Winkel) sind Maße ohne Zeile
//! ([`ConstraintKind::PatternValue`]); so laufen Liste, Formeln und Rückgängig ohne eigenen Weg.

use crate::constraint::{ConstraintId, ConstraintKind};
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, MAX_COORD, PointId, Sketch};
use crate::transform::Similarity;
use planora_i18n::tr;
use planora_kernel::Id;
use planora_math::{DVec2, TAU};
use std::collections::{HashMap, HashSet};

pub type PatternId = Id<PatternData>;

/// Linienzüge der Exemplare je Lage, siehe [`Sketch::pattern_preview`].
pub type PatternPreview = Vec<([i32; 2], Vec<Vec<DVec2>>)>;

/// Höchstzahl der Lagen eines Musters, die Vorlage mitgezählt (Entscheidung 2).
pub const PATTERN_MAX_INSTANCES: usize = 900;
/// Höchstzahl der Kurven aller Exemplare eines Musters.
pub const PATTERN_MAX_CURVES: usize = 3600;
/// So oft löst der Löser höchstens nach, wenn Gruppen Exemplare lesen, deren Lage sich im Lauf
/// geändert hat (Entscheidung 5).
pub(crate) const PATTERN_ROUNDS: usize = 3;

/// Richtung eines Rechteckmusters: die Achsen der Skizze oder eine Linie (folgt ihr).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternDir {
    SketchX,
    SketchY,
    Line(CurveId),
}

/// Feld eines Musterwerts: Anzahl und Abstand je Richtung (Rechteck), Anzahl und Winkel (Kreis).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternSlot {
    Count1,
    Spacing1,
    Count2,
    Spacing2,
    Count,
    Angle,
}

impl PatternSlot {
    /// Eine Anzahl (ganzzahlig, 1 bis [`PATTERN_MAX_INSTANCES`])?
    pub fn is_count(self) -> bool {
        matches!(
            self,
            PatternSlot::Count1 | PatternSlot::Count2 | PatternSlot::Count
        )
    }

    /// Ein Abstand (Länge)?
    pub fn is_spacing(self) -> bool {
        matches!(self, PatternSlot::Spacing1 | PatternSlot::Spacing2)
    }

    pub(crate) fn fits(self, kind: &PatternKind) -> bool {
        match kind {
            PatternKind::Rect { .. } => matches!(
                self,
                PatternSlot::Count1
                    | PatternSlot::Spacing1
                    | PatternSlot::Count2
                    | PatternSlot::Spacing2
            ),
            PatternKind::Circular { mode, .. } => {
                self == PatternSlot::Count
                    || (self == PatternSlot::Angle && *mode != CircularMode::Full)
            }
        }
    }
}

/// Art des Kreismusters: voller Umlauf, Winkel ab der Vorlage, Winkel symmetrisch zur Vorlage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CircularMode {
    Full,
    Angle,
    Symmetric,
}

/// Art eines Musters. `extent`: der Wert ist die Gesamtlänge statt des Abstands; `symmetric`:
/// Exemplare zu beiden Seiten der Vorlage, die Anzahl zählt je Seite einschließlich Vorlage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PatternKind {
    Rect {
        dirs: [PatternDir; 2],
        extent: [bool; 2],
        symmetric: [bool; 2],
    },
    Circular {
        center: PointId,
        mode: CircularMode,
    },
}

/// Ein Exemplar an der Lage `at`: Punkte parallel zu `PatternData::source_points`, Kurven
/// parallel zu `PatternData::sources`.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternInstance {
    pub at: [i32; 2],
    pub points: Vec<PointId>,
    pub curves: Vec<CurveId>,
}

/// Ein Muster in der Skizze.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternData {
    pub kind: PatternKind,
    pub sources: Vec<CurveId>,
    /// Alle Punkte der Vorlagen, ohne Doppelte, in der Reihenfolge der Kurven.
    pub source_points: Vec<PointId>,
    /// Die Musterwerte (`ConstraintKind::PatternValue`).
    pub values: Vec<ConstraintId>,
    /// Ohne Vorlage und ohne abgewählte Lagen.
    pub instances: Vec<PatternInstance>,
    pub suppressed: Vec<[i32; 2]>,
    pub(crate) alive: bool,
}

/// Was ein Muster werden soll. `values`: die Abstände (oder Gesamtlängen) je Richtung, beim
/// Kreismuster `[Winkel, 0]`; `counts[1]` zählt beim Kreismuster nicht.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternPlan {
    pub kind: PatternKind,
    pub sources: Vec<CurveId>,
    pub counts: [u32; 2],
    pub values: [f64; 2],
    pub suppressed: Vec<[i32; 2]>,
}

/// Warum ein Muster nicht geht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternProblem {
    /// Keine, doppelte oder gelöschte Vorlagen, oder Anzahl 0.
    Empty,
    /// Mehr als [`PATTERN_MAX_INSTANCES`] Lagen oder [`PATTERN_MAX_CURVES`] Kurven.
    TooMany,
    /// Abstand oder Winkel null, nicht endlich oder zu groß; Schritt null.
    ZeroStep,
    /// Richtungslinie oder Mitte fehlt oder ist entartet, oder beide Richtungen sind parallel.
    BadDirection,
    /// Ein Exemplar soll Vorlage, Richtungslinie oder Mitte sein.
    InstanceAsSource,
    /// Eine Abhängigkeit verbindet Vorlage, Richtung oder Mitte mit einem eigenen Exemplar.
    Cycle,
}

impl PatternProblem {
    /// Meldung für die Oberfläche.
    pub fn message(self) -> String {
        match self {
            PatternProblem::Empty => tr!("sketch-pattern-empty"),
            PatternProblem::TooMany => tr!(
                "sketch-pattern-too-many",
                max = PATTERN_MAX_INSTANCES.to_string(),
                curves = PATTERN_MAX_CURVES.to_string()
            ),
            PatternProblem::ZeroStep => tr!("sketch-pattern-zero-step"),
            PatternProblem::BadDirection => tr!("sketch-pattern-direction"),
            PatternProblem::InstanceAsSource => tr!("sketch-pattern-instance-source"),
            PatternProblem::Cycle => tr!("sketch-pattern-cycle"),
        }
    }

    /// Der Fehler, mit dem [`Sketch::add_pattern`] und [`Sketch::add_constraint`] ablehnen.
    pub fn error(self) -> SketchError {
        match self {
            PatternProblem::ZeroStep | PatternProblem::BadDirection => {
                SketchError::Degenerate(self.message())
            }
            _ => SketchError::InvalidInput(self.message()),
        }
    }
}

/// Die Punkte der Kurven `curves` ohne Doppelte, in der Reihenfolge der Kurven.
fn points_of(sk: &Sketch, curves: &[CurveId]) -> Vec<PointId> {
    let mut out: Vec<PointId> = Vec::new();
    for &c in curves {
        for p in sk.curves[c].kind.points() {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Die Umkehrung von `t`: `t.apply(inverse(t, p)) = p`.
fn inverse_apply(t: &Similarity, p: DVec2) -> DVec2 {
    let v = (p - t.base - t.offset) / t.factor;
    let (sin, cos) = (-t.angle).sin_cos();
    t.base + DVec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

impl Sketch {
    /// Ein lebendes Muster.
    pub fn pattern(&self, id: PatternId) -> Option<&PatternData> {
        self.patterns.get(id).filter(|d| d.alive)
    }

    /// Alle lebenden Muster.
    pub fn patterns(&self) -> impl Iterator<Item = (PatternId, &PatternData)> {
        self.patterns.iter().filter(|(_, d)| d.alive)
    }

    pub fn pattern_count(&self) -> usize {
        self.patterns().count()
    }

    /// Das Muster, zu dem die Kurve gehört: als Vorlage (`None`) oder als Exemplar an der Lage.
    pub fn pattern_of_curve(&self, c: CurveId) -> Option<(PatternId, Option<[i32; 2]>)> {
        self.patterns().find_map(|(id, d)| {
            if d.sources.contains(&c) {
                return Some((id, None));
            }
            d.instances
                .iter()
                .find(|i| i.curves.contains(&c))
                .map(|i| (id, Some(i.at)))
        })
    }

    /// Lagen der Exemplare: je Lage die Ähnlichkeit, die die Vorlage dorthin bringt. Ohne die
    /// Vorlage und ohne abgewählte Lagen. Rein; dieselbe Rechnung für Vorschau, Anlegen und
    /// Nachführen.
    pub fn pattern_transforms(
        &self,
        plan: &PatternPlan,
    ) -> Result<Vec<([i32; 2], Similarity)>, PatternProblem> {
        self.check_pattern_refs(plan)?;
        let mut all = self.pattern_lattice(plan)?;
        all.retain(|(at, _)| !plan.suppressed.contains(at));
        Ok(all)
    }

    /// Linienzüge der Exemplare je Lage (wie [`Sketch::curve_polyline`]), aus derselben Rechnung
    /// wie [`Sketch::add_pattern`]: Die Exemplare werden auf einer Kopie angelegt. Fehler wie
    /// `add_pattern`: ein [`PatternProblem`] als [`PatternProblem::error`], sonst der Fehler beim
    /// Anlegen eines Exemplars (etwa Koordinaten über `MAX_COORD`).
    pub fn pattern_preview(
        &self,
        plan: &PatternPlan,
        steps_per_turn: usize,
    ) -> Result<PatternPreview, SketchError> {
        let transforms = self
            .pattern_transforms(plan)
            .map_err(PatternProblem::error)?;
        let mut s = self.clone();
        let source_points = points_of(&s, &plan.sources);
        let mut out = Vec::with_capacity(transforms.len());
        for (at, t) in &transforms {
            let inst = s.add_instance(&plan.sources, &source_points, *at, t)?;
            let lines = inst
                .curves
                .iter()
                .filter_map(|&c| s.curve_polyline(c, steps_per_turn))
                .collect();
            out.push((*at, lines));
        }
        Ok(out)
    }

    /// Legt das Muster an: Kopien an den Lagen aus [`Sketch::pattern_transforms`], je Wert ein
    /// `PatternValue` (bei „Voll“ ohne Winkel; Anzahl und Abstand der zweiten Richtung auch bei
    /// Anzahl 1), dann einmal lösen. Bei einem Fehler bleibt die Skizze unverändert.
    pub fn add_pattern(&mut self, plan: &PatternPlan) -> Result<PatternId, SketchError> {
        let transforms = self
            .pattern_transforms(plan)
            .map_err(PatternProblem::error)?;
        let lattice: Vec<[i32; 2]> = self
            .pattern_lattice(plan)
            .map_err(PatternProblem::error)?
            .into_iter()
            .map(|(at, _)| at)
            .collect();
        self.atomically(|s| {
            let source_points = points_of(s, &plan.sources);
            let mut instances = Vec::with_capacity(transforms.len());
            for (at, t) in &transforms {
                instances.push(s.add_instance(&plan.sources, &source_points, *at, t)?);
            }
            let mut suppressed: Vec<[i32; 2]> = Vec::new();
            for at in &plan.suppressed {
                if lattice.contains(at) && !suppressed.contains(at) {
                    suppressed.push(*at);
                }
            }
            let id = s.patterns.push(PatternData {
                kind: plan.kind,
                sources: plan.sources.clone(),
                source_points,
                values: Vec::new(),
                instances,
                suppressed,
                alive: true,
            });
            let count = |n: u32| f64::from(n);
            let slots: Vec<(PatternSlot, f64)> = match plan.kind {
                PatternKind::Rect { .. } => vec![
                    (PatternSlot::Count1, count(plan.counts[0])),
                    (PatternSlot::Spacing1, plan.values[0]),
                    (PatternSlot::Count2, count(plan.counts[1])),
                    (PatternSlot::Spacing2, plan.values[1]),
                ],
                PatternKind::Circular {
                    mode: CircularMode::Full,
                    ..
                } => vec![(PatternSlot::Count, count(plan.counts[0]))],
                PatternKind::Circular { .. } => vec![
                    (PatternSlot::Count, count(plan.counts[0])),
                    (PatternSlot::Angle, plan.values[0]),
                ],
            };
            for (slot, v) in slots {
                let c = s.push_pattern_value(ConstraintKind::PatternValue(id, slot, v))?;
                s.patterns[id].values.push(c);
            }
            s.touch_structure();
            s.solve();
            Ok(id)
        })
    }

    /// Der gespeicherte Wert eines lebenden Musters im Feld `slot`.
    pub(crate) fn pattern_value(&self, id: PatternId, slot: PatternSlot) -> Option<f64> {
        self.pattern(id)?
            .values
            .iter()
            .find_map(|&c| match self.constraints.get(c)?.kind {
                ConstraintKind::PatternValue(_, x, v) if x == slot => Some(v),
                _ => None,
            })
    }

    /// Der Plan eines lebenden Musters aus seinen gespeicherten Werten.
    pub(crate) fn pattern_plan(&self, id: PatternId) -> Option<PatternPlan> {
        let d = self.pattern(id)?;
        let value = |slot: PatternSlot| self.pattern_value(id, slot);
        let count = |v: f64| {
            if v.is_finite() && v >= 0.0 {
                v.round().min(u32::MAX as f64) as u32
            } else {
                0
            }
        };
        let (counts, values) = match d.kind {
            PatternKind::Rect { .. } => (
                [
                    count(value(PatternSlot::Count1)?),
                    count(value(PatternSlot::Count2)?),
                ],
                [value(PatternSlot::Spacing1)?, value(PatternSlot::Spacing2)?],
            ),
            PatternKind::Circular { mode, .. } => (
                [count(value(PatternSlot::Count)?), 1],
                [
                    if mode == CircularMode::Full {
                        0.0
                    } else {
                        value(PatternSlot::Angle)?
                    },
                    0.0,
                ],
            ),
        };
        Some(PatternPlan {
            kind: d.kind,
            sources: d.sources.clone(),
            counts,
            values,
            suppressed: d.suppressed.clone(),
        })
    }

    /// Schreibt die Variablen aller Exemplare aus der aktuellen Vorlage und den Musterwerten neu.
    /// Exemplare, Vorlagen und Punkte, die nicht mehr leben oder deren Art nicht mehr passt,
    /// bleiben, wie sie sind; ebenso Muster, deren Lage gerade nicht geht. `true`, wenn sich ein
    /// Wert geändert hat (dann neuer Stand).
    pub(crate) fn refresh_patterns(&mut self) -> bool {
        let ids: Vec<PatternId> = self.patterns().map(|(id, _)| id).collect();
        let mut writes: Vec<(usize, f64)> = Vec::new();
        for id in ids {
            let Some(plan) = self.pattern_plan(id) else {
                continue;
            };
            let Ok(lattice) = self.pattern_lattice(&plan) else {
                continue;
            };
            let lattice: HashMap<[i32; 2], Similarity> = lattice.into_iter().collect();
            let d = &self.patterns[id];
            for inst in &d.instances {
                let Some(t) = lattice.get(&inst.at) else {
                    continue;
                };
                for (&q, &p) in inst.points.iter().zip(&d.source_points) {
                    if !(self.points[q].alive && self.points[p].alive) {
                        continue;
                    }
                    let to = t.apply(self.pos(p));
                    writes.push((self.points[q].x, to.x));
                    writes.push((self.points[q].y, to.y));
                }
                for (&c, &src) in inst.curves.iter().zip(&d.sources) {
                    if !(self.curves[c].alive && self.curves[src].alive) {
                        continue;
                    }
                    match (&self.curves[src].kind, &self.curves[c].kind) {
                        (
                            CurveKind::Circle { radius: from, .. },
                            CurveKind::Circle { radius: to, .. },
                        ) => writes.push((*to, self.vars[*from] * t.factor)),
                        (
                            CurveKind::Ellipse {
                                ax, ay, minor: mi, ..
                            }
                            | CurveKind::EllipticArc {
                                ax, ay, minor: mi, ..
                            },
                            CurveKind::Ellipse {
                                ax: bx,
                                ay: by,
                                minor: mj,
                                ..
                            }
                            | CurveKind::EllipticArc {
                                ax: bx,
                                ay: by,
                                minor: mj,
                                ..
                            },
                        ) => {
                            let a = t.linear(DVec2::new(self.vars[*ax], self.vars[*ay]));
                            writes.push((*bx, a.x));
                            writes.push((*by, a.y));
                            writes.push((*mj, self.vars[*mi] * t.factor));
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut changed = false;
        for (v, x) in writes {
            if self.vars[v].to_bits() != x.to_bits() {
                self.vars[v] = x;
                changed = true;
            }
        }
        if changed {
            self.touch_values();
        }
        changed
    }

    /// Die Variablen aller lebenden Exemplare (Punkte, Radien, Halbachsen): für den Löser
    /// Konstanten.
    pub(crate) fn pattern_vars(&self) -> HashSet<usize> {
        let mut out = HashSet::new();
        for (_, d) in self.patterns() {
            for inst in &d.instances {
                for &q in &inst.points {
                    if self.points[q].alive {
                        out.extend([self.points[q].x, self.points[q].y]);
                    }
                }
                for &c in &inst.curves {
                    if self.curves[c].alive {
                        out.extend(crate::solver::own_vars(&self.curves[c].kind));
                    }
                }
            }
        }
        out
    }

    /// Eingänge und Ausgänge eines lebenden Musters als Variablen: Vorlagen (Punkte und eigene
    /// Variablen), Richtungslinien und Mitte; die Variablen seiner Exemplare.
    pub(crate) fn pattern_io(&self, id: PatternId) -> (HashSet<usize>, HashSet<usize>) {
        let d = &self.patterns[id];
        let mut inputs = HashSet::new();
        let point = |p: PointId, set: &mut HashSet<usize>| {
            set.extend([self.points[p].x, self.points[p].y]);
        };
        for &p in &d.source_points {
            point(p, &mut inputs);
        }
        for &c in &d.sources {
            inputs.extend(crate::solver::own_vars(&self.curves[c].kind));
        }
        match d.kind {
            PatternKind::Rect { dirs, .. } => {
                for dir in dirs {
                    if let PatternDir::Line(l) = dir {
                        for p in self.curves[l].kind.points() {
                            point(p, &mut inputs);
                        }
                    }
                }
            }
            PatternKind::Circular { center, .. } => point(center, &mut inputs),
        }
        let mut outputs = HashSet::new();
        for inst in &d.instances {
            for &q in &inst.points {
                point(q, &mut outputs);
            }
            for &c in &inst.curves {
                outputs.extend(crate::solver::own_vars(&self.curves[c].kind));
            }
        }
        (inputs, outputs)
    }

    /// Ist `p` Punkt eines Exemplars? Dann sein Vorlagenpunkt und die Lage des Exemplars.
    pub(crate) fn instance_source_point(&self, p: PointId) -> Option<(PointId, Similarity)> {
        let (id, inst, k) = self.patterns().find_map(|(id, d)| {
            d.instances.iter().find_map(|inst| {
                inst.points
                    .iter()
                    .position(|&q| q == p)
                    .map(|k| (id, inst.at, k))
            })
        })?;
        let plan = self.pattern_plan(id)?;
        let (_, t) = self
            .pattern_lattice(&plan)
            .ok()?
            .into_iter()
            .find(|(at, _)| *at == inst)?;
        Some((self.patterns[id].source_points[k], t))
    }

    /// Ist `c` Kurve eines Exemplars? Dann ihre Vorlage.
    pub(crate) fn instance_source_curve(&self, c: CurveId) -> Option<CurveId> {
        self.patterns().find_map(|(_, d)| {
            d.instances.iter().find_map(|inst| {
                inst.curves
                    .iter()
                    .position(|&x| x == c)
                    .map(|k| d.sources[k])
            })
        })
    }

    /// Ziel für die Vorlage, wenn ein Exemplarpunkt nach `target` gezogen wird: die Umkehrung
    /// der Lage des Exemplars (Zug am Exemplar bewegt die Vorlage, **Annahme**).
    pub(crate) fn instance_drag_target(
        &self,
        p: PointId,
        target: DVec2,
    ) -> Option<(PointId, DVec2)> {
        // Ist der Vorlagenpunkt gelöscht, zieht der Zug den Punkt selbst.
        let (source, t) = self
            .instance_source_point(p)
            .filter(|(s, _)| self.points[*s].alive)?;
        Some((source, inverse_apply(&t, target)))
    }

    /// Vorlagen vorhanden, lebend, ohne Doppelte; kein Exemplar als Vorlage, Richtungslinie oder
    /// Mitte.
    fn check_pattern_refs(&self, plan: &PatternPlan) -> Result<(), PatternProblem> {
        if plan.sources.is_empty() {
            return Err(PatternProblem::Empty);
        }
        for (i, &c) in plan.sources.iter().enumerate() {
            if self.check_curve(c).is_err() || plan.sources[..i].contains(&c) {
                return Err(PatternProblem::Empty);
            }
        }
        let mut curves: HashSet<CurveId> = HashSet::new();
        let mut points: HashSet<PointId> = HashSet::new();
        for (_, d) in self.patterns() {
            for inst in &d.instances {
                curves.extend(inst.curves.iter().copied());
                points.extend(inst.points.iter().copied());
            }
        }
        let instance_curve = plan.sources.iter().any(|c| curves.contains(c))
            || points_of(self, &plan.sources)
                .iter()
                .any(|p| points.contains(p));
        let instance_ref = match plan.kind {
            PatternKind::Rect { dirs, .. } => dirs
                .iter()
                .any(|d| matches!(d, PatternDir::Line(l) if curves.contains(l))),
            PatternKind::Circular { center, .. } => points.contains(&center),
        };
        if instance_curve || instance_ref {
            return Err(PatternProblem::InstanceAsSource);
        }
        Ok(())
    }

    /// Einheitsvektor einer Richtung zum Zeitpunkt der Rechnung.
    fn pattern_dir(&self, dir: PatternDir) -> Result<DVec2, PatternProblem> {
        match dir {
            PatternDir::SketchX => Ok(DVec2::X),
            PatternDir::SketchY => Ok(DVec2::Y),
            PatternDir::Line(l) => {
                let Some(CurveKind::Line { a, b }) = self.curve(l).map(|c| &c.kind) else {
                    return Err(PatternProblem::BadDirection);
                };
                let d = self.pos(*b) - self.pos(*a);
                let len = d.length();
                if len.is_finite() && len > self.tol.linear {
                    Ok(d / len)
                } else {
                    Err(PatternProblem::BadDirection)
                }
            }
        }
    }

    /// Alle Lagen außer der Vorlage, auch abgewählte (Entscheidung 4). Prüft Anzahl, Werte und
    /// Richtungen, nicht die Vorlagen.
    pub(crate) fn pattern_lattice(
        &self,
        plan: &PatternPlan,
    ) -> Result<Vec<([i32; 2], Similarity)>, PatternProblem> {
        let tol = self.tol;
        let count = |n: u32| -> Result<i32, PatternProblem> {
            if n == 0 {
                Err(PatternProblem::Empty)
            } else if n as usize > PATTERN_MAX_INSTANCES {
                Err(PatternProblem::TooMany)
            } else {
                Ok(n as i32)
            }
        };
        let too_many = |positions: usize| {
            positions > PATTERN_MAX_INSTANCES
                || (positions - 1) * plan.sources.len() > PATTERN_MAX_CURVES
        };
        let mut out = Vec::new();
        match plan.kind {
            PatternKind::Rect {
                dirs,
                extent,
                symmetric,
            } => {
                let n = [count(plan.counts[0])?, count(plan.counts[1])?];
                let lo = [0, 1].map(|k| if symmetric[k] { -(n[k] - 1) } else { 0 });
                let hi = [n[0] - 1, n[1] - 1];
                let positions = (0..2)
                    .map(|k| (hi[k] - lo[k] + 1) as usize)
                    .product::<usize>();
                if too_many(positions) {
                    return Err(PatternProblem::TooMany);
                }
                let mut step = [DVec2::ZERO; 2];
                let mut unit = [DVec2::ZERO; 2];
                for k in 0..2 {
                    let v = plan.values[k];
                    if !(v.is_finite() && v.abs() > tol.linear && v.abs() <= MAX_COORD) {
                        return Err(PatternProblem::ZeroStep);
                    }
                    let s = if extent[k] {
                        if n[k] > 1 {
                            v / f64::from(n[k] - 1)
                        } else {
                            0.0
                        }
                    } else {
                        v
                    };
                    if n[k] > 1 && s.abs() <= tol.linear {
                        return Err(PatternProblem::ZeroStep);
                    }
                    unit[k] = self.pattern_dir(dirs[k])?;
                    step[k] = unit[k] * s;
                }
                if hi[0] > lo[0] && hi[1] > lo[1] && unit[0].perp_dot(unit[1]).abs() <= tol.angular
                {
                    return Err(PatternProblem::BadDirection);
                }
                for j in lo[1]..=hi[1] {
                    for i in lo[0]..=hi[0] {
                        if (i, j) == (0, 0) {
                            continue;
                        }
                        let mut t = Similarity::identity(DVec2::ZERO);
                        t.offset = step[0] * f64::from(i) + step[1] * f64::from(j);
                        out.push(([i, j], t));
                    }
                }
            }
            PatternKind::Circular { center, mode } => {
                if self.check_point(center).is_err() {
                    return Err(PatternProblem::BadDirection);
                }
                let n = count(plan.counts[0])?;
                let symmetric = mode == CircularMode::Symmetric;
                let positions = if symmetric { 2 * n - 1 } else { n } as usize;
                if too_many(positions) {
                    return Err(PatternProblem::TooMany);
                }
                let step = if mode == CircularMode::Full {
                    TAU / f64::from(n)
                } else {
                    let theta = plan.values[0];
                    if !(theta.is_finite()
                        && theta.abs() > tol.angular
                        && theta.abs() <= TAU + tol.angular)
                    {
                        return Err(PatternProblem::ZeroStep);
                    }
                    // Ein voller Umlauf gilt wie „Voll“, sonst läge das letzte Exemplar auf der
                    // Vorlage (Annahme); symmetrisch dann gleichmäßig über alle 2n − 1 Lagen.
                    let full = theta.abs() >= TAU - tol.angular;
                    match (full, symmetric) {
                        (true, false) => theta.signum() * TAU / f64::from(n),
                        (true, true) => theta.signum() * TAU / f64::from(2 * n - 1),
                        (false, _) if n == 1 => 0.0,
                        (false, false) => theta / f64::from(n - 1),
                        (false, true) => 0.5 * theta / f64::from(n - 1),
                    }
                };
                if n > 1 && step.abs() <= tol.angular {
                    return Err(PatternProblem::ZeroStep);
                }
                let c = self.pos(center);
                let lo = if symmetric { -(n - 1) } else { 0 };
                for k in lo..n {
                    if k == 0 {
                        continue;
                    }
                    let mut t = Similarity::identity(c);
                    t.angle = step * f64::from(k);
                    out.push(([k, 0], t));
                }
            }
        }
        Ok(out)
    }

    /// Legt ein Exemplar an der Lage `t` an: neue Punkte zu `source_points`, Kopien der Kurven
    /// `sources` mit Stil.
    fn add_instance(
        &mut self,
        sources: &[CurveId],
        source_points: &[PointId],
        at: [i32; 2],
        t: &Similarity,
    ) -> Result<PatternInstance, SketchError> {
        let mut map = HashMap::with_capacity(source_points.len());
        let mut points = Vec::with_capacity(source_points.len());
        for &p in source_points {
            let q = self.add_point_like(p, t.apply(self.pos(p)))?;
            map.insert(p, q);
            points.push(q);
        }
        let mut curves = Vec::with_capacity(sources.len());
        for &c in sources {
            curves.push(self.copy_curve(c, &map, crate::helpers::CopyHow::Pattern(*t))?);
        }
        Ok(PatternInstance { at, points, curves })
    }
}
