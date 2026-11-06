//! Constraint-Solver: Gruppen, Levenberg-Marquardt, Freiheitsgrade und Diagnose.
//!
//! Aktiv sind alle lebenden, eingeschalteten und nicht nur anzeigenden Abhängigkeiten
//! (einschließlich der internen). Variablen, die über Abhängigkeiten verbunden sind, bilden eine
//! Gruppe; jede Gruppe wird für sich gelöst. Nur gelöste Gruppen schreiben ihre Werte zurück.
//!
//! Eine Lösung zählt nur, wenn die Geometrie der Gruppe dabei nicht entartet (Spec §4.5):
//! Kreisradius, Bogenradius (Start und Ende), Linienlänge und beide Halbachsen von Ellipsen und
//! Ellipsenbögen müssen größer als `tol.linear` sein. Sonst gilt die Gruppe als nicht gelöst und
//! bleibt im letzten Zustand; die Diagnose nennt die Ursache.
//!
//! Zwischenspeicher: Das Ergebnis jeder Gruppe wird mit den Werten aller Variablen gemerkt,
//! die es bestimmen: ihre eigenen und sämtliche Variablen ihrer Kurven, auch solche außerhalb
//! der Gruppe (die Prüfung auf Entartung liest etwa das freie Ende einer Linie oder einen
//! Punkt einer anderen Gruppe). Stimmen Abhängigkeiten und Werte beim nächsten Lauf überein, wird es übernommen statt neu
//! gerechnet. Das ist exakt dasselbe Ergebnis, denn ein neuer Lauf ab denselben Werten rechnet
//! deterministisch dasselbe (eine gelöste Gruppe ist an ihren Werten schon gelöst). Jede
//! Änderung an Objekten oder Abhängigkeiten leert den Speicher. So rechnet beim Ziehen nur die
//! Gruppe mit dem gezogenen Punkt.

use crate::constraint::{ConstraintId, ConstraintKind, SHARED_END_TANGENT_DRIFT};
use crate::error::SketchError;
use crate::linalg::{Matrix, analyze_rows, cholesky_solve};
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use crate::pattern::{PATTERN_ROUNDS, PatternId};
use planora_i18n::tr;
use planora_math::DVec2;
use std::collections::{HashMap, HashSet};

/// Höchstzahl nutzererzeugter Abhängigkeiten einer Gruppe, für die die Widerspruchsdiagnose
/// je Abhängigkeit einen eigenen Lauf rechnet (Entscheidung D4). Jeder Lauf kostet so viel wie
/// ein Lösen der Gruppe; bei mehr Abhängigkeiten meldet der Solver „nicht lösbar“ mit den
/// Abhängigkeiten, deren Residuum am Ende des gescheiterten Laufs zu groß ist.
const MAX_DIAGNOSE_RUNS: usize = 48;

/// Höchstzahl der Levenberg-Marquardt-Schritte je Lauf.
const MAX_ITERATIONS: usize = 100;
/// Anteil von `tol.linear`, bis zu dem alle Residuen sinken müssen (1e-8 mm bei Standardtoleranz).
const RESIDUAL_FRACTION: f64 = 1e-2;
/// Relative Schwelle der Rangbestimmung (Länge des Restvektors / ursprüngliche Zeilenlänge).
const RANK_REL_TOL: f64 = 1e-7;
/// Anfangsdämpfung (Marquardt-Parameter).
const INITIAL_LAMBDA: f64 = 1e-3;
/// Untere Grenze der Dämpfung (fast reines Gauß-Newton).
const MIN_LAMBDA: f64 = 1e-12;
/// Obere Grenze der Dämpfung; darüber gilt der Schritt als gescheitert.
const MAX_LAMBDA: f64 = 1e12;
/// Faktor, um den die Dämpfung nach einem guten Schritt fällt bzw. nach einem schlechten steigt.
const LAMBDA_FACTOR: f64 = 10.0;
/// Untere Schranke für den Maßstab der Dämpfung (Jacobi-Matrix aus lauter Nullen).
///
/// Gedämpft wird mit `λ · s · I`, `s = max(max_k (JᵀJ)_kk, MIN_DIAG)`. Alle Skizzenvariablen
/// haben dieselbe Einheit (mm), deshalb dämpft der Solver euklidisch und nicht spaltenweise
/// (Marquardt): Spaltenweise Skalierung macht schwach gekoppelte Richtungen (etwa das Drehen
/// eines Punktes um einen festen Punkt) „billig“ und erzeugt riesige Querschritte, mit denen
/// der Solver schon bei einfachen Startlagen nicht konvergiert.
const MIN_DIAG: f64 = 1e-9;
/// Relative Schrittweite bzw. Kostensenkung, unter der ein Schritt als Stillstand gilt.
const STALL_REL: f64 = 1e-14;
/// So viele Schritte für den ganzen Weg nimmt [`Sketch::solve_in_steps`] höchstens.
const MAX_VALUE_STEPS: f64 = 128.0;
/// Nullraumanteil, ab dem eine Variable als frei gilt.
const FREE_COMPONENT: f64 = 1e-6;
/// Versuche mit steigender Dämpfung innerhalb eines Schritts.
const DAMPING_TRIES: usize = 30;
/// So oft setzt [`Sketch::solve_holding`] die gehaltenen Punkte zurück und löst nach.
const HOLD_ROUNDS: usize = 8;
/// Um so viel genauer zieht [`Sketch::solve_holding`] zum Schluss nach.
const HOLD_POLISH: f64 = 1e-2;
/// So oft halbiert [`Sketch::solve_holding_in_steps`] den Schritt höchstens.
const HOLD_HALVINGS: usize = 6;

/// Ergebnis eines Solver-Laufs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolveStatus {
    /// Alle Gruppen sind gelöst.
    Solved,
    /// Mindestens eine Gruppe ließ sich nicht lösen, ohne dass ein Widerspruch gefunden wurde.
    NotConverged,
    /// Mindestens eine Gruppe enthält sich widersprechende Abhängigkeiten.
    Conflicting,
}

/// Bericht eines Solver-Laufs.
#[derive(Clone, Debug)]
pub struct SolveReport {
    /// Beim Ziehen: Eine Gruppe, die gezogene Werte benutzt, ließ sich nicht lösen. Fremde
    /// Gruppen, die schon vorher scheiterten, zählen nicht.
    pub drag_failed: bool,
    pub status: SolveStatus,
    /// Verbleibende Freiheitsgrade (nur gelöste Gruppen und freie Variablen).
    pub dof: usize,
    /// Überzählige (aber erfüllte) nutzererzeugte Abhängigkeiten, jeweils die später angelegte.
    pub redundant: Vec<ConstraintId>,
    /// Abhängigkeiten, die einen Widerspruch verursachen.
    pub conflicting: Vec<ConstraintId>,
    pub free_points: Vec<PointId>,
    pub free_curves: Vec<CurveId>,
    /// Höchste Schrittzahl über alle Gruppen.
    pub iterations: usize,
    /// Zahl der Gruppen, die in diesem Lauf tatsächlich gerechnet wurden; die übrigen kamen
    /// unverändert aus dem Zwischenspeicher (siehe Moduldoku).
    pub group_solves: usize,
}

/// Weiche Zusatzbedingung `weight·(x[var] − target)` (zum Ziehen).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Soft {
    pub var: usize,
    pub target: f64,
    pub weight: f64,
}

/// Gemerkte Ergebnisse je Gruppe, Schlüssel: die Abhängigkeiten der Gruppe.
#[derive(Clone, Debug, Default)]
pub(crate) struct SolveCache {
    groups: HashMap<Vec<ConstraintId>, CachedGroup>,
}

#[derive(Clone, Debug)]
struct CachedGroup {
    /// `Group::reads` beim Lauf.
    reads: Vec<usize>,
    /// Bitmuster der Werte dieser Variablen nach dem Lauf (gelöst: die übernommene Lösung;
    /// nicht gelöst: der unveränderte Start).
    values: Vec<u64>,
    outcome: Outcome,
}

#[derive(Clone, Debug)]
enum Outcome {
    Solved {
        dof: usize,
        redundant: Vec<ConstraintId>,
        free_vars: Vec<usize>,
    },
    Failed {
        conflict: bool,
        found: Vec<ConstraintId>,
        iterations: usize,
    },
}

impl SolveCache {
    fn lookup(&self, g: &Group, x: &[f64]) -> Option<&Outcome> {
        let hit = self.groups.get(&g.constraints)?;
        (hit.reads == g.reads
            && hit
                .reads
                .iter()
                .zip(&hit.values)
                .all(|(&v, &bits)| x[v].to_bits() == bits))
        .then_some(&hit.outcome)
    }

    fn insert(&mut self, g: &Group, x: &[f64], outcome: Outcome) {
        self.groups.insert(
            g.constraints.clone(),
            CachedGroup {
                reads: g.reads.clone(),
                values: g.reads.iter().map(|&v| x[v].to_bits()).collect(),
                outcome,
            },
        );
    }
}

/// Gewicht der weichen Zugbedingung. Klein gegen die harten Residuen (Gewicht 1), damit die
/// Abhängigkeiten im Zuglauf gewinnen und die Politur nur wenig korrigieren muss; groß genug,
/// damit der Zug die freien Freiheitsgrade in wenigen Schritten bewegt (Kosten des Zugs bei
/// 10 mm Abstand: 1e-2, deutlich über der Stillstandsschwelle).
const DRAG_WEIGHT: f64 = 1e-2;

/// Zug eines Punktes zu einer Zielposition.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub point: PointId,
    pub target: DVec2,
}

/// Zusammenhangskomponente aus Variablen und Abhängigkeiten.
struct Group {
    vars: Vec<usize>,
    constraints: Vec<ConstraintId>,
    /// Lebende Kurven mit mindestens einer Variablen in der Gruppe.
    curves: Vec<CurveId>,
    /// Alle Variablen, die das Ergebnis der Gruppe bestimmen: `vars` und sämtliche Variablen
    /// der Kurven in `curves` (die Prüfung auf Entartung liest auch Punkte außerhalb der
    /// Gruppe, etwa das freie Ende einer Linie oder einen Punkt einer anderen Gruppe).
    /// Aufsteigend, ohne Doppelte. Schlüssel des Zwischenspeichers.
    reads: Vec<usize>,
    /// Variablen von Musterexemplaren, die die Abhängigkeiten der Gruppe lesen: für den Löser
    /// Konstanten (keine Spalte), aber Teil von `reads`. Aufsteigend, ohne Doppelte.
    pattern_reads: Vec<usize>,
}

/// Residuen und Jacobi-Matrix einer Gruppe.
struct Assembled {
    r: Vec<f64>,
    j: Matrix,
    /// Die ersten `hard_rows` Zeilen stammen aus Abhängigkeiten, der Rest aus weichen Bedingungen.
    hard_rows: usize,
    /// Zu jeder harten Zeile: Index der Abhängigkeit in `Group::constraints`.
    owner: Vec<usize>,
}

impl Assembled {
    /// Größter Betrag der harten Residuen (NaN bleibt erhalten).
    fn hard_inf(&self) -> f64 {
        self.r[..self.hard_rows].iter().fold(0.0_f64, |m, v| {
            if v.is_nan() || m.is_nan() {
                f64::NAN
            } else {
                m.max(v.abs())
            }
        })
    }
}

/// Welche Zeilen in die Rechnung eingehen.
#[derive(Clone, Copy)]
enum Rows<'a> {
    All,
    /// Ohne diese Abhängigkeit (Suche nach der Ursache eines Widerspruchs).
    Without(ConstraintId),
    /// Diese Tangenten zielen auf ihren Restwert statt auf null
    /// ([`Sketch::solve_keeping_tangent_drift`]).
    Keeping(&'a [(ConstraintId, f64)]),
}

struct Lm {
    converged: bool,
    iterations: usize,
}

struct Analysis {
    dof: usize,
    redundant: Vec<ConstraintId>,
    free_vars: Vec<usize>,
}

fn sumsq(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum()
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

impl Sketch {
    /// Führt die Maße `targets` (Abhängigkeit, alter Wert, neuer Wert) gemeinsam in Schritten von der
    /// aktuellen Lage zum Ziel, wenn ein direktes Lösen gescheitert ist: Ein großer Sprung überwiegt
    /// in den Kosten die Winkel der Tangenten am gemeinsamen Endpunkt, sie laufen an ihre Grenze bei
    /// ±π, und dort bleibt der Löser hängen. Beginnt mit dem halben Weg; was `accept` nicht annimmt,
    /// halbiert den Schritt, bis auf [`MAX_VALUE_STEPS`] Schritte für den ganzen Weg. `None`, wenn es
    /// nicht ans Ziel kommt; sonst die Skizze am Ziel und ihr letzter Bericht.
    pub fn solve_in_steps(
        &self,
        targets: &[(ConstraintId, f64, f64)],
        accept: impl Fn(&Sketch, &SolveReport) -> bool,
    ) -> Option<(Sketch, SolveReport)> {
        let mut current = self.clone();
        let (mut at, mut step) = (0.0_f64, 0.5_f64);
        while step >= 1.0 / MAX_VALUE_STEPS {
            let last = 1.0 - at <= step;
            let next = if last { 1.0 } else { at + step };
            let mut trial = current.clone();
            for &(id, from, to) in targets {
                let value = if last { to } else { from + (to - from) * next };
                trial.set_constraint_value(id, value).ok()?;
            }
            let found = trial.solve();
            if !accept(&trial, &found) {
                step /= 2.0;
                continue;
            }
            if last {
                return Some((trial, found));
            }
            (current, at) = (trial, next);
        }
        None
    }

    /// Löst alle aktiven Abhängigkeiten. Nicht lösbare Gruppen bleiben unverändert.
    pub fn solve(&mut self) -> SolveReport {
        self.solve_with_soft(&[])
    }

    /// Löst bei festgehaltenen Punkten `hold`, ohne dass sie sich auch nur im letzten Bit bewegen:
    /// auf einer Kopie mit vorübergehendem `Fixed`. Gelöst: Die Werte werden übernommen, `true`.
    /// Sonst `false`, und die Skizze bleibt, wie sie ist.
    ///
    /// Die Schranke des Lösers reicht hier nicht: Ein Winkelfehler von 1e-8 ist an einer Linie von
    /// 150 mm schon 1,5e-6 mm Abstand. Darum wird genauer nachgezogen; gelingt das nicht, gilt der
    /// Lauf als gescheitert, statt ein ungenaues Ergebnis als gelöst zu melden.
    pub fn solve_holding(&mut self, hold: &[PointId]) -> bool {
        let mut trial = self.clone();
        let mut pins = Vec::with_capacity(hold.len());
        for &p in hold {
            let Ok(at) = self.point_xy(p) else {
                return false;
            };
            match trial.add_constraint(ConstraintKind::Fixed(p, at.x, at.y)) {
                Ok(id) => pins.push(id),
                Err(_) => return false,
            }
        }
        let pinned: Vec<(usize, f64)> = self
            .held_variables(hold)
            .into_iter()
            .map(|v| (v, self.vars[v]))
            .collect();
        if !trial.settle_holding(&pinned) {
            return false;
        }
        trial.tol.linear *= HOLD_POLISH;
        trial.drop_rows_between_held(&pinned, &pins);
        trial.touch_structure();
        if !trial.settle_holding(&pinned) {
            return false;
        }
        self.vars = trial.vars;
        self.touch_values();
        true
    }

    /// Wie [`solve_holding`](Self::solve_holding) für das Maß `dim`, das schon den neuen Wert `to`
    /// trägt und vorher `from` hatte: Ein großer Sprung lässt den Löser nah an der Grenze scheitern,
    /// in kleineren Schritten bleibt die Lage. Halbiert höchstens [`HOLD_HALVINGS`]-mal.
    pub fn solve_holding_in_steps(
        &mut self,
        dim: ConstraintId,
        hold: &[PointId],
        from: f64,
        to: f64,
    ) -> bool {
        if self.solve_holding(hold) {
            return true;
        }
        let mut trial = self.clone();
        if !trial.hold_by_halves(dim, hold, from, to, HOLD_HALVINGS) {
            return false;
        }
        self.vars = trial.vars;
        self.touch_values();
        true
    }

    /// Geht von `from` nach `to`; `self` steht bei `from`-Lage, das Maß danach auf `to`.
    fn hold_by_halves(
        &mut self,
        dim: ConstraintId,
        hold: &[PointId],
        from: f64,
        to: f64,
        depth: usize,
    ) -> bool {
        if self.set_constraint_value(dim, to).is_err() {
            return false;
        }
        if self.solve_holding(hold) {
            return true;
        }
        if depth == 0 {
            return false;
        }
        let mid = 0.5 * (from + to);
        self.hold_by_halves(dim, hold, from, mid, depth - 1)
            && self.hold_by_halves(dim, hold, mid, to, depth - 1)
    }

    /// Schaltet Abhängigkeiten ab, die nur gehaltene Werte lesen: Sie sind schon im Rahmen der
    /// gewöhnlichen Schranke erfüllt, und bewegen darf sie niemand mehr. Die Zeilen `pins` bleiben.
    fn drop_rows_between_held(&mut self, pinned: &[(usize, f64)], pins: &[ConstraintId]) {
        let held: HashSet<usize> = pinned.iter().map(|p| p.0).collect();
        let idle: Vec<ConstraintId> = self
            .constraints
            .iter()
            .filter(|(id, c)| c.alive && c.enabled && !pins.contains(id))
            .map(|(id, _)| id)
            .filter(|&id| {
                self.evaluate(id, &self.vars)
                    .is_some_and(|ev| ev.locals.iter().all(|v| held.contains(v)))
            })
            .collect();
        for id in idle {
            self.constraints[id].enabled = false;
        }
    }

    /// Löst, setzt `pinned` zurück und löst nach, bis sich nichts mehr rührt: Der Löser kommt nur
    /// bis auf seine Schranke an die festen Werte heran. `false`, wenn es nicht gelingt.
    fn settle_holding(&mut self, pinned: &[(usize, f64)]) -> bool {
        for _ in 0..HOLD_ROUNDS {
            let report = self.solve();
            if report.status != SolveStatus::Solved {
                return false;
            }
            if report.iterations == 0 {
                return true;
            }
            for &(v, value) in pinned {
                self.vars[v] = value;
            }
        }
        false
    }

    /// Die Variablen von `hold` und die Radien der Kreise, die durch sie schon feststehen (Mitte
    /// und ein Punkt mit `PointOnCurve` in `hold`).
    fn held_variables(&self, hold: &[PointId]) -> Vec<usize> {
        let mut vars: Vec<usize> = Vec::with_capacity(hold.len() * 2);
        for &p in hold {
            vars.extend([self.points[p].x, self.points[p].y]);
        }
        for (_, d) in self.constraints().filter(|(_, d)| d.enabled) {
            if let ConstraintKind::PointOnCurve(p, c) = d.kind
                && hold.contains(&p)
                && let CurveKind::Circle { center, radius } = self.curves[c].kind
                && hold.contains(&center)
            {
                vars.push(radius);
            }
        }
        vars
    }

    /// Baut Residuen und Jacobi-Matrix der Gruppe an der Stelle `x`, je nach `which` (alle, ohne eine Abhängigkeit, oder mit Restwerten für Tangenten).
    fn assemble(
        &self,
        g: &Group,
        col: &HashMap<usize, usize>,
        x: &[f64],
        soft: &[Soft],
        which: Rows,
    ) -> Assembled {
        let mut rows: Vec<(f64, Vec<(usize, f64)>)> = Vec::new();
        let mut owner = Vec::new();
        for (ci, &id) in g.constraints.iter().enumerate() {
            let mut target = 0.0;
            match which {
                Rows::Without(skip) if skip == id => continue,
                Rows::Keeping(kept) => {
                    if let Some(&(_, rest)) = kept.iter().find(|(c, _)| *c == id) {
                        target = rest;
                    }
                }
                _ => {}
            }
            // Die Gruppe enthält nur lebende Abhängigkeiten; `None` kann nicht auftreten.
            let Some(ev) = self.evaluate(id, x) else {
                continue;
            };
            for res in &ev.residuals {
                let entries = ev
                    .locals
                    .iter()
                    .enumerate()
                    .filter_map(|(k, v)| col.get(v).map(|&c| (c, res.grad(k))))
                    .collect();
                rows.push((res.v - target, entries));
                owner.push(ci);
            }
        }
        let hard_rows = rows.len();
        for s in soft {
            if let Some(&c) = col.get(&s.var) {
                rows.push((s.weight * (x[s.var] - s.target), vec![(c, s.weight)]));
            }
        }
        let mut j = Matrix::zeros(rows.len(), g.vars.len());
        let mut r = Vec::with_capacity(rows.len());
        for (i, (value, entries)) in rows.iter().enumerate() {
            r.push(*value);
            for (c, d) in entries {
                j.add_to(i, *c, *d);
            }
        }
        Assembled {
            r,
            j,
            hard_rows,
            owner,
        }
    }

    /// Levenberg-Marquardt mit einheitlicher Dämpfung `λ · max_k (JᵀJ)_kk · I` (siehe
    /// `MIN_DIAG`). Ändert `x` nur durch Schritte, die die Kosten senken.
    fn levenberg_marquardt(
        &self,
        g: &Group,
        col: &HashMap<usize, usize>,
        x: &mut [f64],
        soft: &[Soft],
        which: Rows,
        tol_r: f64,
    ) -> Lm {
        let mut a = self.assemble(g, col, x, soft, which);
        let mut cost = sumsq(&a.r);
        let mut lambda = INITIAL_LAMBDA;
        let mut iterations = 0;
        for _ in 0..MAX_ITERATIONS {
            if soft.is_empty() && a.hard_inf() <= tol_r {
                break;
            }
            if !cost.is_finite() {
                break;
            }
            iterations += 1;
            let gradient = a.j.transpose_mul_vec(&a.r);
            let gram = a.j.gram();
            let rhs: Vec<f64> = gradient.iter().map(|v| -v).collect();
            // Maßstab der Dämpfung, siehe `MIN_DIAG`.
            let scale = (0..gram.rows())
                .map(|i| gram.get(i, i))
                .fold(MIN_DIAG, f64::max);
            let mut accepted = false;
            let mut stalled = false;
            for _ in 0..DAMPING_TRIES {
                let mut m = gram.clone();
                for i in 0..m.rows() {
                    m.add_to(i, i, lambda * scale);
                }
                if let Some(step) = cholesky_solve(&m, &rhs) {
                    let mut trial = x.to_vec();
                    for (c, &v) in g.vars.iter().enumerate() {
                        trial[v] += step[c];
                    }
                    let t = self.assemble(g, col, &trial, soft, which);
                    let trial_cost = sumsq(&t.r);
                    if trial_cost.is_finite() && trial_cost < cost {
                        let size = g.vars.iter().map(|&v| x[v].abs()).fold(1.0_f64, f64::max);
                        let step_len = step.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
                        stalled =
                            step_len <= STALL_REL * size || (cost - trial_cost) <= STALL_REL * cost;
                        x.copy_from_slice(&trial);
                        a = t;
                        cost = trial_cost;
                        lambda = (lambda / LAMBDA_FACTOR).max(MIN_LAMBDA);
                        accepted = true;
                        break;
                    }
                }
                lambda *= LAMBDA_FACTOR;
                if lambda > MAX_LAMBDA {
                    break;
                }
            }
            if !accepted || stalled {
                break;
            }
        }
        let h = a.hard_inf();
        Lm {
            converged: h.is_finite() && h <= tol_r,
            iterations,
        }
    }

    /// Scheitert eine Gruppe, in der eine Tangente am gemeinsamen Endpunkt schon im Rahmen von
    /// [`SHARED_END_TANGENT_DRIFT`] danebenliegt (etwa in einer alten Skizze, deren Mitte fest
    /// ist), wird sie noch einmal gelöst: Diese Tangenten behalten ihre Abweichung, statt sie
    /// abbauen zu müssen; sie halten ihre Kurven weiter. Exakte Tangenten gelten unverändert
    /// streng, ein neuer Widerspruch zu ihnen bleibt also einer.
    fn solve_keeping_tangent_drift(
        &self,
        g: &Group,
        col: &HashMap<usize, usize>,
        start: &[f64],
        soft: &[Soft],
        tol_r: f64,
    ) -> Option<(Vec<f64>, Lm)> {
        let kept: Vec<(ConstraintId, f64)> = g
            .constraints
            .iter()
            .filter_map(|&id| {
                let drift = self.shared_end_tangent_drift(id, start)?;
                let rest = self.evaluate(id, start)?.residuals.first()?.v;
                (rest != 0.0 && drift <= SHARED_END_TANGENT_DRIFT).then_some((id, rest))
            })
            .collect();
        if kept.is_empty() {
            return None;
        }
        let which = Rows::Keeping(&kept);
        let mut x = start.to_vec();
        let mut iterations = 0;
        if !soft.is_empty() {
            iterations += self
                .levenberg_marquardt(g, col, &mut x, soft, which, tol_r)
                .iterations;
        }
        let mut lm = self.levenberg_marquardt(g, col, &mut x, &[], which, tol_r);
        lm.iterations += iterations;
        let within = kept.iter().all(|&(id, _)| {
            self.shared_end_tangent_drift(id, &x)
                .is_some_and(|d| d <= SHARED_END_TANGENT_DRIFT)
        });
        (lm.converged && within).then_some((x, lm))
    }

    /// Rang, Redundanz und freie Variablen einer gelösten Gruppe an der Stelle `x`.
    fn analyze(&self, g: &Group, col: &HashMap<usize, usize>, x: &[f64]) -> Analysis {
        let a = self.assemble(g, col, x, &[], Rows::All);
        let space = analyze_rows(&a.j, RANK_REL_TOL);
        let mut redundant = Vec::new();
        for (row, independent) in space.independent.iter().enumerate() {
            let id = g.constraints[a.owner[row]];
            if !independent && !self.constraints[id].internal && !redundant.contains(&id) {
                redundant.push(id);
            }
        }
        let free_vars = g
            .vars
            .iter()
            .enumerate()
            .filter(|(c, _)| {
                space
                    .null_space
                    .iter()
                    .any(|v| v[*c].abs() > FREE_COMPONENT)
            })
            .map(|(_, &v)| v)
            .collect();
        Analysis {
            dof: g.vars.len().saturating_sub(space.rank),
            redundant,
            free_vars,
        }
    }

    /// Abhängigkeiten der Gruppe, deren Spline-Parameter in `x` außerhalb von `[0, n − 1]`
    /// liegt (Spielraum siehe `Sketch::spline_parameter`). Dort läge die Lösung auf der nicht
    /// gezeichneten Fortsetzung des End-Segments; sie gilt nicht als Lösung.
    fn spline_parameters_outside(&self, g: &Group, x: &[f64]) -> Vec<ConstraintId> {
        g.constraints
            .iter()
            .copied()
            .filter(|&id| {
                self.spline_parameter(id, x)
                    .is_some_and(|sp| !sp.contains(x[sp.var]))
            })
            .collect()
    }

    /// Setzt Spline-Parameter der Gruppe, die außerhalb liegen, auf das nächste Ende.
    fn clamp_spline_parameters(&self, g: &Group, x: &mut [f64]) {
        for &id in &g.constraints {
            if let Some(sp) = self.spline_parameter(id, x) {
                let t = x[sp.var];
                if t.is_finite() {
                    x[sp.var] = t.clamp(0.0, sp.end);
                }
            }
        }
    }

    /// Ist eine der Kurven `curves` an der Stelle `x` entartet? Kreis- und Bogenradius,
    /// Linienlänge und beide Halbachsen einer Ellipse oder eines Ellipsenbogens müssen größer als `tol.linear` sein (NaN
    /// zählt als entartet). Ein negativer Kreisradius (gespiegelter Kreis, etwa bei einer
    /// Tangente durch die Linie hindurch) ist ebenfalls entartet.
    fn degenerate(&self, curves: &[CurveId], x: &[f64]) -> bool {
        let tol = self.tolerance().linear;
        let at = |p: PointId| DVec2::new(x[self.points[p].x], x[self.points[p].y]);
        let ok = |v: f64| v > tol;
        curves.iter().any(|&c| match &self.curves[c].kind {
            CurveKind::Line { a, b } => !ok(at(*a).distance(at(*b))),
            CurveKind::Circle { radius, .. } => !ok(x[*radius]),
            CurveKind::Arc { center, start, end } => {
                let m = at(*center);
                !(ok(at(*start).distance(m)) && ok(at(*end).distance(m)))
            }
            CurveKind::Ellipse { ax, ay, minor, .. }
            | CurveKind::EllipticArc { ax, ay, minor, .. } => {
                !(ok(DVec2::new(x[*ax], x[*ay]).length()) && ok(x[*minor]))
            }
            CurveKind::Spline { .. } => false,
        })
    }

    /// Konvergiert, alle Spline-Parameter im Bereich und keine Kurve der Gruppe entartet.
    fn acceptable(&self, g: &Group, lm: &Lm, x: &[f64]) -> bool {
        lm.converged
            && self.spline_parameters_outside(g, x).is_empty()
            && !self.degenerate(&g.curves, x)
    }

    /// Bestimmt, welche einzelne Abhängigkeit einer nicht lösbaren Gruppe den Widerspruch
    /// verursacht (Entscheidung D4). Leer, wenn auch der Rückfall nichts findet.
    ///
    /// Ohne `full` (beim Ziehen für die gezogene Gruppe) und bei mehr als
    /// [`MAX_DIAGNOSE_RUNS`] Abhängigkeiten entfällt die teure Suche; gemeldet werden dann nur
    /// die Abhängigkeiten mit zu großem Residuum, der Status ist „nicht lösbar“.
    #[allow(clippy::too_many_arguments)]
    fn diagnose(
        &self,
        g: &Group,
        col: &HashMap<usize, usize>,
        start: &[f64],
        failed: &[f64],
        tol_r: f64,
        full: bool,
    ) -> (Vec<ConstraintId>, bool) {
        // Erfüllt schon der Start alles und ist dort nichts entartet (etwa beim Ziehen), ist
        // keine Abhängigkeit schuld.
        let at_start = self.assemble(g, col, start, &[], Rows::All);
        if at_start.hard_inf() <= tol_r
            && self.spline_parameters_outside(g, start).is_empty()
            && !self.degenerate(&g.curves, start)
        {
            return (Vec::new(), false);
        }
        let user = g
            .constraints
            .iter()
            .filter(|&&id| !self.constraints[id].internal)
            .count();
        if full && user <= MAX_DIAGNOSE_RUNS {
            let mut candidates = Vec::new();
            for &id in &g.constraints {
                if self.constraints[id].internal {
                    continue;
                }
                let mut x = start.to_vec();
                if self
                    .levenberg_marquardt(g, col, &mut x, &[], Rows::Without(id), tol_r)
                    .converged
                    && self
                        .spline_parameters_outside(g, &x)
                        .iter()
                        .all(|&o| o == id)
                    && !self.degenerate(&g.curves, &x)
                {
                    candidates.push(id);
                }
            }
            if !candidates.is_empty() {
                return (candidates, true);
            }
        }
        let a = self.assemble(g, col, failed, &[], Rows::All);
        let mut big = Vec::new();
        for (row, v) in a.r[..a.hard_rows].iter().enumerate() {
            let id = g.constraints[a.owner[row]];
            // `!(|v| <= tol_r)` zählt auch NaN-Residuen als verletzt.
            let violated = v.is_nan() || v.abs() > tol_r;
            if violated && !self.constraints[id].internal && !big.contains(&id) {
                big.push(id);
            }
        }
        (big, false)
    }

    /// Bildet die Gruppen der aktiven Abhängigkeiten. Liefert auch, welche Variablen in einer
    /// aktiven Abhängigkeit vorkommen.
    fn build_groups(&self) -> (Vec<Group>, Vec<bool>) {
        self.build_groups_holding(&self.pattern_vars(), None)
    }

    /// Wie [`build_groups`](Self::build_groups); die Variablen `held` (Musterexemplare) sind
    /// Konstanten: Sie verbinden nichts und zählen nicht als Variablen einer Gruppe, landen aber
    /// in `reads` und `pattern_reads`. Abhängigkeiten, die nur sie lesen, entfallen. `extra`:
    /// Variablen einer noch nicht angelegten Abhängigkeit, die mit verbindet (ohne Eintrag in
    /// `Group::constraints`).
    fn build_groups_holding(
        &self,
        held: &HashSet<usize>,
        extra: Option<&[usize]>,
    ) -> (Vec<Group>, Vec<bool>) {
        let n = self.vars.len();
        let mut parent: Vec<usize> = (0..n).collect();
        let mut in_constraint = vec![false; n];
        // Abhängigkeiten in Anlagereihenfolge, jeweils mit ihren Variablen und den gelesenen
        // Exemplarvariablen.
        let mut active: Vec<(Option<ConstraintId>, Vec<usize>, Vec<usize>)> = Vec::new();
        let existing = self
            .constraints
            .iter()
            .filter(|(_, c)| c.alive && c.enabled && !c.driven)
            .filter_map(|(id, _)| Some((Some(id), self.evaluate(id, &self.vars)?.locals)));
        for (id, all) in existing.chain(extra.map(|e| (None, e.to_vec()))) {
            let (locals, read): (Vec<usize>, Vec<usize>) =
                all.into_iter().partition(|v| !held.contains(v));
            if locals.is_empty() {
                continue;
            }
            for w in locals.windows(2) {
                let (a, b) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
                parent[a] = b;
            }
            for &v in &locals {
                in_constraint[v] = true;
            }
            active.push((id, locals, read));
        }
        let mut groups: Vec<Group> = Vec::new();
        let mut index: HashMap<usize, usize> = HashMap::new();
        for v in (0..n).filter(|v| in_constraint[*v]) {
            let root = find(&mut parent, v);
            let gi = *index.entry(root).or_insert_with(|| {
                groups.push(Group {
                    vars: Vec::new(),
                    constraints: Vec::new(),
                    curves: Vec::new(),
                    reads: Vec::new(),
                    pattern_reads: Vec::new(),
                });
                groups.len() - 1
            });
            groups[gi].vars.push(v);
        }
        for (id, locals, read) in active {
            let gi = index[&find(&mut parent, locals[0])];
            groups[gi].constraints.extend(id);
            groups[gi].pattern_reads.extend(read);
        }
        // Kurven, deren Variablen (Punkte oder eigene) in einer Gruppe liegen, gehören zu ihr
        // (für die Prüfung auf Entartung).
        for (cid, c) in self.curves() {
            let mut vars: Vec<usize> = c
                .kind
                .points()
                .iter()
                .flat_map(|&p| [self.points[p].x, self.points[p].y])
                .collect();
            vars.extend(own_vars(&c.kind));
            let mut seen: Vec<usize> = Vec::new();
            for &v in vars.iter().filter(|&&v| in_constraint[v]) {
                let gi = index[&find(&mut parent, v)];
                if !seen.contains(&gi) {
                    seen.push(gi);
                    groups[gi].curves.push(cid);
                    groups[gi].reads.extend(vars.iter().copied());
                }
            }
        }
        for g in &mut groups {
            g.pattern_reads.sort_unstable();
            g.pattern_reads.dedup();
            g.reads.extend(g.vars.iter().copied());
            g.reads.extend(g.pattern_reads.iter().copied());
            g.reads.sort_unstable();
            g.reads.dedup();
        }
        (groups, in_constraint)
    }

    /// Die lebenden Muster in einer Schleife: Eine Gruppe enthält eine Eingangsvariable (Vorlage,
    /// Richtungslinie oder Mitte) und liest eine Exemplarvariable desselben Musters. Dann liefe das
    /// Nachführen nie zusammen (Entscheidung 6). `extra`: die Variablen einer Abhängigkeit, die
    /// erst angelegt werden soll; `touching`: nur Gruppen, die eine dieser Variablen enthalten
    /// (`None`: alle Gruppen).
    fn patterns_in_cycles(
        &self,
        extra: Option<&[usize]>,
        touching: Option<&[usize]>,
    ) -> Vec<PatternId> {
        let held = self.pattern_vars();
        if held.is_empty() {
            return Vec::new();
        }
        let (groups, _) = self.build_groups_holding(&held, extra);
        let io: Vec<(PatternId, HashSet<usize>, HashSet<usize>)> = self
            .patterns()
            .map(|(id, _)| {
                let (inputs, outputs) = self.pattern_io(id);
                (id, inputs, outputs)
            })
            .collect();
        let touching: Option<HashSet<usize>> = touching.map(|t| t.iter().copied().collect());
        let mut out: Vec<PatternId> = Vec::new();
        for g in groups.iter().filter(|g| !g.pattern_reads.is_empty()) {
            if let Some(t) = &touching
                && !g.vars.iter().any(|v| t.contains(v))
            {
                continue;
            }
            for (id, inputs, outputs) in &io {
                if !out.contains(id)
                    && g.pattern_reads.iter().any(|v| outputs.contains(v))
                    && g.vars.iter().any(|v| inputs.contains(v))
                {
                    out.push(*id);
                }
            }
        }
        out
    }

    /// Schlösse die Abhängigkeit mit den Variablen `locals` (noch nicht angelegt oder gerade
    /// angelegt) eine neue Schleife zwischen Vorlage und eigenem Exemplar? Geprüft werden nur die
    /// Gruppen, in denen sie liegt: Eine Schleife anderswo in der Skizze (etwa aus einer alten
    /// Datei) sperrt fremde Befehle nicht.
    pub(crate) fn pattern_cycle(&self, locals: &[usize]) -> bool {
        !self
            .patterns_in_cycles(Some(locals), Some(locals))
            .is_empty()
    }

    /// Alle lebenden Muster in einer Schleife (beim Laden, vor und nach Befehlen, die Muster
    /// ändern oder auflösen).
    pub(crate) fn cyclic_patterns(&self) -> Vec<PatternId> {
        self.patterns_in_cycles(None, None)
    }

    /// Löst mit weichen Zusatzbedingungen (zum Ziehen). Harte Abhängigkeiten werden danach
    /// ohne die weichen exakt nachgelöst („Politur“).
    ///
    /// Mit Mustern (Entscheidung 5): Vor dem Lauf werden die Exemplare aus der Vorlage
    /// nachgeführt, danach erneut. Hat sich dabei ein Exemplar bewegt und liest eine Gruppe
    /// Exemplare, folgt noch ein Lauf, insgesamt höchstens `max(PATTERN_ROUNDS, Muster + 1)`:
    /// Jede Runde bringt in einer Kette ohne Schleife mindestens ein weiteres Glied zur Ruhe, so
    /// reicht das für jede solche Kette (Schlusskorrektur I2). Die Exemplare stehen am Ende immer
    /// an der Lage der gelösten Vorlage. Geht die Lage eines Musters nicht, ist ein „gelöst“
    /// „nicht lösbar“: Seine Exemplare stünden sonst still an der alten Lage.
    pub(crate) fn solve_with_soft(&mut self, soft: &[Soft]) -> SolveReport {
        if self.patterns().next().is_none() {
            return self.solve_round(soft, &HashSet::new()).0;
        }
        self.refresh_patterns();
        let held = self.pattern_vars();
        let max_rounds = PATTERN_ROUNDS.max(self.patterns().count() + 1);
        let (mut report, mut reads) = self.solve_round(soft, &held);
        let mut rounds = 1;
        loop {
            let refresh = self.refresh_patterns();
            if refresh.failed && report.status == SolveStatus::Solved {
                report.status = SolveStatus::NotConverged;
            }
            if !(refresh.moved && reads) {
                break;
            }
            if rounds >= max_rounds {
                // Die letzte Nachführung hat Exemplare bewegt, die eine Gruppe liest (etwa eine
                // Schleife über mehrere Muster, die die Prüfung nicht kennt): Deren Lösung ist
                // überholt.
                self.flag_stale_pattern_reads(&held, &mut report);
                break;
            }
            let (next, again) = self.solve_round(soft, &held);
            rounds += 1;
            reads = again;
            report = SolveReport {
                drag_failed: report.drag_failed || next.drag_failed,
                iterations: report.iterations.max(next.iterations),
                group_solves: report.group_solves + next.group_solves,
                ..next
            };
        }
        report
    }

    /// Prüft die Gruppen, die Exemplare lesen, an den aktuellen Werten: Verletzte Abhängigkeiten
    /// kommen nach `conflicting`, und ein „gelöst“ wird „nicht lösbar“ (kein Widerspruch: der
    /// nächste Lauf führt weiter).
    fn flag_stale_pattern_reads(&self, held: &HashSet<usize>, report: &mut SolveReport) {
        let tol_r = RESIDUAL_FRACTION * self.tolerance().linear;
        let (groups, _) = self.build_groups_holding(held, None);
        let mut violated = false;
        for g in groups.iter().filter(|g| !g.pattern_reads.is_empty()) {
            for &id in &g.constraints {
                let Some(ev) = self.evaluate(id, &self.vars) else {
                    continue;
                };
                // `!(|r| <= tol_r)` zählt auch NaN als verletzt.
                if ev.residuals.iter().all(|r| r.v.abs() <= tol_r) {
                    continue;
                }
                violated = true;
                if !self.constraints[id].internal && !report.conflicting.contains(&id) {
                    report.conflicting.push(id);
                }
            }
        }
        if violated && report.status == SolveStatus::Solved {
            report.status = SolveStatus::NotConverged;
        }
    }

    /// Ein Lauf über alle Gruppen; `held` sind die Exemplarvariablen (Konstanten). Der zweite
    /// Wert sagt, ob eine Gruppe Exemplare liest.
    fn solve_round(&mut self, soft: &[Soft], held: &HashSet<usize>) -> (SolveReport, bool) {
        let tol_r = RESIDUAL_FRACTION * self.tolerance().linear;
        let (groups, in_constraint) = self.build_groups_holding(held, None);
        let reads_patterns = groups.iter().any(|g| !g.pattern_reads.is_empty());

        let mut status = SolveStatus::Solved;
        let mut redundant = Vec::new();
        let mut conflicting = Vec::new();
        let mut free_vars: HashSet<usize> = HashSet::new();
        let mut dof = 0;
        let mut iterations = 0;
        let mut group_solves = 0;
        let mut drag_failed = false;
        let touched = |g: &Group| soft.iter().any(|s| g.reads.binary_search(&s.var).is_ok());
        let start = self.vars.clone();
        let mut committed = self.vars.clone();
        let old_cache = std::mem::take(&mut self.solve_cache);
        let mut cache = SolveCache::default();
        for g in &groups {
            let col: HashMap<usize, usize> =
                g.vars.iter().enumerate().map(|(c, v)| (*v, c)).collect();
            // Nur die weichen Bedingungen auf Variablen dieser Gruppe zählen. Gruppen ohne sie
            // werden genau wie von `solve` gelöst (kein Zug- und Politurlauf).
            let group_soft: Vec<Soft> = soft
                .iter()
                .filter(|s| col.contains_key(&s.var))
                .copied()
                .collect();
            // Unverändert seit dem letzten Lauf: Ergebnis übernehmen (siehe Moduldoku). Eine
            // gelöste Gruppe ist an ihren Werten schon gelöst, ein neuer Lauf bräuchte keinen
            // Schritt; eine gescheiterte rechnete ab demselben Start dasselbe.
            if group_soft.is_empty()
                && let Some(outcome) = old_cache.lookup(g, &start)
            {
                match outcome {
                    Outcome::Solved {
                        dof: d,
                        redundant: red,
                        free_vars: fv,
                    } => {
                        dof += d;
                        free_vars.extend(fv.iter().copied());
                        for id in red {
                            if !redundant.contains(id) {
                                redundant.push(*id);
                            }
                        }
                    }
                    Outcome::Failed {
                        conflict,
                        found,
                        iterations: used,
                    } => {
                        drag_failed |= touched(g);
                        iterations = iterations.max(*used);
                        if *conflict {
                            status = SolveStatus::Conflicting;
                        } else if status == SolveStatus::Solved {
                            status = SolveStatus::NotConverged;
                        }
                        for id in found {
                            if !conflicting.contains(id) {
                                conflicting.push(*id);
                            }
                        }
                    }
                }
                cache.insert(g, &start, outcome.clone());
                continue;
            }
            group_solves += 1;
            let mut x = start.clone();
            let mut lm = self.levenberg_marquardt(g, &col, &mut x, &group_soft, Rows::All, tol_r);
            let mut used = lm.iterations;
            if !group_soft.is_empty() {
                // Politur: Die weichen Bedingungen liefern nur eine Näherung. Von dort aus wird
                // ohne sie exakt nachgelöst. Landet ein Spline-Parameter dabei auf der
                // Fortsetzung, wird er aufs Ende gesetzt und erneut poliert. Gelingt das nicht,
                // wird ohne Zug vom Start gelöst.
                lm = self.levenberg_marquardt(g, &col, &mut x, &[], Rows::All, tol_r);
                used += lm.iterations;
                if lm.converged && !self.acceptable(g, &lm, &x) {
                    self.clamp_spline_parameters(g, &mut x);
                    lm = self.levenberg_marquardt(g, &col, &mut x, &[], Rows::All, tol_r);
                    used += lm.iterations;
                }
                if !self.acceptable(g, &lm, &x) {
                    x = start.clone();
                    lm = self.levenberg_marquardt(g, &col, &mut x, &[], Rows::All, tol_r);
                    used += lm.iterations;
                }
            }
            if !lm.converged
                && let Some((y, retry)) =
                    self.solve_keeping_tangent_drift(g, &col, &start, &group_soft, tol_r)
            {
                used += retry.iterations;
                x = y;
                lm = retry;
            }
            iterations = iterations.max(used);
            // Spline-Parameter außerhalb des gezeichneten Bereichs: nicht gelöst.
            let outside = if lm.converged {
                self.spline_parameters_outside(g, &x)
            } else {
                Vec::new()
            };
            if lm.converged && outside.is_empty() && !self.degenerate(&g.curves, &x) {
                let analysis = self.analyze(g, &col, &x);
                dof += analysis.dof;
                free_vars.extend(analysis.free_vars.iter().copied());
                for &id in &analysis.redundant {
                    if !redundant.contains(&id) {
                        redundant.push(id);
                    }
                }
                for &v in &g.vars {
                    committed[v] = x[v];
                }
                cache.insert(
                    g,
                    &x,
                    Outcome::Solved {
                        dof: analysis.dof,
                        redundant: analysis.redundant,
                        free_vars: analysis.free_vars,
                    },
                );
            } else {
                // Beim Ziehen sucht die Diagnose für die gezogene Gruppe nicht nach der
                // Ursache; das holt das volle Lösen am Ende des Ziehens nach.
                drag_failed |= touched(g);
                let full = group_soft.is_empty();
                let (found, conflict) = self.diagnose(g, &col, &start, &x, tol_r, full);
                // Widerspruch gewinnt gegen „nicht konvergiert“.
                if conflict {
                    status = SolveStatus::Conflicting;
                } else if status == SolveStatus::Solved {
                    status = SolveStatus::NotConverged;
                }
                let mut blamed = Vec::new();
                for id in found.into_iter().chain(outside) {
                    if !blamed.contains(&id) {
                        blamed.push(id);
                    }
                    if !conflicting.contains(&id) {
                        conflicting.push(id);
                    }
                }
                if full {
                    cache.insert(
                        g,
                        &start,
                        Outcome::Failed {
                            conflict,
                            found: blamed,
                            iterations: used,
                        },
                    );
                }
            }
        }
        if committed
            .iter()
            .zip(&start)
            .any(|(a, b)| a.to_bits() != b.to_bits())
        {
            self.touch_values();
        }
        self.vars = committed;
        self.solve_cache = cache;

        // Lebende Variablen, die in keiner aktiven Abhängigkeit vorkommen, sind frei;
        // Exemplarvariablen nicht (sie hält das Muster).
        for v in self.live_variables() {
            if !in_constraint[v] && !held.contains(&v) && free_vars.insert(v) {
                dof += 1;
            }
        }
        let mut free_points: Vec<PointId> = self
            .points()
            .filter(|(_, p)| free_vars.contains(&p.x) || free_vars.contains(&p.y))
            .map(|(i, _)| i)
            .collect();
        // Ein Exemplarpunkt gilt als frei, wenn sein Vorlagenpunkt frei ist (nur für die Farbe).
        let mut free_set: HashSet<PointId> = free_points.iter().copied().collect();
        let mut instance_curves: Vec<(CurveId, CurveId)> = Vec::new();
        for (_, d) in self.patterns() {
            for inst in &d.instances {
                for (&q, p) in inst.points.iter().zip(&d.source_points) {
                    if self.points[q].alive && free_set.contains(p) && free_set.insert(q) {
                        free_points.push(q);
                    }
                }
                instance_curves.extend(inst.curves.iter().copied().zip(d.sources.iter().copied()));
            }
        }
        let mut free_curves: Vec<CurveId> = self
            .curves()
            .filter(|(_, c)| {
                own_vars(&c.kind).iter().any(|v| free_vars.contains(v))
                    || c.kind.points().iter().any(|p| free_set.contains(p))
            })
            .map(|(i, _)| i)
            .collect();
        let free_sources: HashSet<CurveId> = free_curves.iter().copied().collect();
        for (copy, source) in instance_curves {
            if self.curves[copy].alive
                && free_sources.contains(&source)
                && !free_sources.contains(&copy)
            {
                free_curves.push(copy);
            }
        }
        (
            SolveReport {
                drag_failed,
                status,
                dof,
                redundant,
                conflicting,
                free_points,
                free_curves,
                iterations,
                group_solves,
            },
            reads_patterns,
        )
    }

    /// Zieht Punkte zu Zielpositionen; harte Abhängigkeiten bleiben erfüllt. Der Solver startet
    /// vom aktuellen Zustand, die gewählte Lage mehrdeutiger Abhängigkeiten (`branch`) bleibt
    /// unverändert, deshalb klappt nichts in die Spiegellage um. Nicht lösbare Gruppen bleiben
    /// unverändert (siehe `solve`).
    ///
    /// Zug an einem Musterexemplar zieht dessen Vorlagenpunkt an die entsprechende Stelle (die
    /// Umkehrung der Lage des Exemplars, **Annahme**).
    pub fn drag(&mut self, drags: &[Drag]) -> Result<SolveReport, SketchError> {
        for d in drags {
            self.check_point(d.point)?;
            if !d.target.is_finite() {
                return Err(SketchError::InvalidInput(tr!("sketch-err-target-finite")));
            }
            crate::model::finite(d.target.x, || tr!("sketch-err-what-target-x"))?;
            crate::model::finite(d.target.y, || tr!("sketch-err-what-target-y"))?;
        }
        let mapped: Vec<Drag>;
        let drags = if self.patterns().next().is_some() {
            mapped = drags
                .iter()
                .map(|d| match self.instance_drag_target(d.point, d.target) {
                    Some((point, target)) if target.is_finite() => Drag { point, target },
                    _ => *d,
                })
                .collect();
            &mapped[..]
        } else {
            drags
        };
        let mut soft = Vec::with_capacity(2 * drags.len());
        for d in drags {
            let (xv, yv) = (self.points[d.point].x, self.points[d.point].y);
            soft.push(Soft {
                var: xv,
                target: d.target.x,
                weight: DRAG_WEIGHT,
            });
            soft.push(Soft {
                var: yv,
                target: d.target.y,
                weight: DRAG_WEIGHT,
            });
        }
        // Punkte ohne jede Abhängigkeit folgen dem Zug direkt.
        for d in drags {
            if self.point_unconstrained(d.point) {
                self.set_point_xy(d.point, d.target.x, d.target.y)?;
            }
        }
        Ok(self.solve_with_soft(&soft))
    }

    /// Zieht den Radius eines Kreises zu `target` (größer als die Toleranz); harte Abhängigkeiten
    /// bleiben erfüllt. Liest keine aktive Abhängigkeit den Radius, folgt er direkt.
    /// Am Kreis eines Musterexemplars zieht das den Kreis der Vorlage.
    pub fn drag_radius(&mut self, c: CurveId, target: f64) -> Result<SolveReport, SketchError> {
        self.check_curve(c)?;
        let c = self.instance_source_curve(c).unwrap_or(c);
        let CurveKind::Circle { radius, .. } = self.curves[c].kind else {
            return Err(SketchError::InvalidInput(tr!(
                "sketch-err-only-whole-circles"
            )));
        };
        crate::model::finite(target, || tr!("sketch-err-what-radius"))?;
        if target <= self.tolerance().linear {
            return Err(SketchError::Degenerate(tr!("sketch-radius-positive")));
        }
        if !self.build_groups().1[radius] {
            self.vars[radius] = target;
            self.touch_values();
        }
        Ok(self.solve_with_soft(&[Soft {
            var: radius,
            target,
            weight: DRAG_WEIGHT,
        }]))
    }

    /// Wahr, wenn keine aktive Abhängigkeit (lebend, eingeschaltet, nicht nur anzeigend, auch
    /// interne) den Punkt direkt oder über eine Kurve benutzt, die ihn enthält.
    fn point_unconstrained(&self, p: PointId) -> bool {
        // Ein Exemplarpunkt hält das Muster; mit gelöschter Vorlage zählt er wie jeder Punkt.
        if self.instance_drag_target(p, DVec2::ZERO).is_some() {
            return false;
        }
        !self
            .constraints
            .iter()
            .filter(|(_, c)| c.alive && c.enabled && !c.driven)
            .any(|(_, c)| {
                c.kind.points().contains(&p)
                    || c.kind
                        .curves()
                        .iter()
                        .any(|&cu| self.curves[cu].kind.points().contains(&p))
            })
    }

    /// Alle Variablen lebender Punkte und Kurven (ohne Doppelte).
    fn live_variables(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.points().flat_map(|(_, p)| [p.x, p.y]).collect();
        for (_, c) in self.curves() {
            v.extend(own_vars(&c.kind));
        }
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// Variablen, die zu einer Kurve gehören und nicht Teil eines Punktes sind.
pub(crate) fn own_vars(kind: &CurveKind) -> Vec<usize> {
    match kind {
        CurveKind::Circle { radius, .. } => vec![*radius],
        CurveKind::Ellipse { ax, ay, minor, .. } | CurveKind::EllipticArc { ax, ay, minor, .. } => {
            vec![*ax, *ay, *minor]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::ConstraintKind as K;
    use planora_math::{Frame, Tolerance};

    #[test]
    fn soft_pull_moves_free_points_and_keeps_hard_constraints_exact() {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let a = s.add_point(0.0, 0.0).unwrap();
        let b = s.add_point(10.0, 0.0).unwrap();
        s.add_line(a, b).unwrap();
        s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
        // b wird nach (0, 20) gezogen; erreichbar ist nur der Kreis mit Radius 10.
        let (bx, by) = (s.points[b].x, s.points[b].y);
        let soft = [
            Soft {
                var: bx,
                target: 0.0,
                weight: 1.0,
            },
            Soft {
                var: by,
                target: 20.0,
                weight: 1.0,
            },
        ];
        let r = s.solve_with_soft(&soft);
        assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
        let p = s.point_xy(b).unwrap();
        assert!((p.length() - 10.0).abs() < 1e-8, "{p:?}");
        assert!(p.y > 9.0, "b folgt dem Zug: {p:?}");
    }

    #[test]
    fn degenerate_ellipses_lines_and_negative_circles_are_detected() {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let m = s.add_point(0.0, 0.0).unwrap();
        let e = s.add_ellipse(m, DVec2::new(3.0, 0.0), 1.0).unwrap();
        let c = s.add_circle(m, 2.0).unwrap();
        let b = s.add_point(1.0, 0.0).unwrap();
        let l = s.add_line(m, b).unwrap();
        let CurveKind::Ellipse { ax, minor, .. } = s.curves[e].kind else {
            unreachable!()
        };
        let CurveKind::Circle { radius, .. } = s.curves[c].kind else {
            unreachable!()
        };
        assert!(!s.degenerate(&[e, c, l], &s.vars));
        let mut x = s.vars.clone();
        x[minor] = 0.0;
        assert!(s.degenerate(&[e], &x));
        let mut x = s.vars.clone();
        x[ax] = 0.0;
        assert!(s.degenerate(&[e], &x));
        let mut x = s.vars.clone();
        x[radius] = -2.0;
        assert!(s.degenerate(&[c], &x));
        x[radius] = f64::NAN;
        assert!(s.degenerate(&[c], &x));
        let mut x = s.vars.clone();
        x[s.points[b].x] = 0.0;
        assert!(s.degenerate(&[l], &x));
        let p = s.add_point(0.0, 1.0).unwrap();
        let q = s.add_point(-3.0, 0.0).unwrap();
        let arc = s
            .add_elliptic_arc(m, DVec2::new(3.0, 0.0), 1.0, p, q)
            .unwrap();
        let CurveKind::EllipticArc { ay, minor, .. } = s.curves[arc].kind else {
            unreachable!()
        };
        assert!(!s.degenerate(&[arc], &s.vars));
        let mut x = s.vars.clone();
        x[minor] = 0.0;
        assert!(s.degenerate(&[arc], &x));
        let mut x = s.vars.clone();
        x[ay] = f64::NAN;
        assert!(s.degenerate(&[arc], &x));
    }
}
