//! Constraint-Solver: Gruppen, Levenberg-Marquardt, Freiheitsgrade und Diagnose.
//!
//! Aktiv sind alle lebenden, eingeschalteten und nicht nur anzeigenden Abhängigkeiten
//! (einschließlich der internen). Variablen, die über Abhängigkeiten verbunden sind, bilden eine
//! Gruppe; jede Gruppe wird für sich gelöst. Nur gelöste Gruppen schreiben ihre Werte zurück.
//!
//! Eine Lösung zählt nur, wenn die Geometrie der Gruppe dabei nicht entartet (Spec §4.5):
//! Kreisradius, Bogenradius (Start und Ende), Linienlänge und beide Ellipsen-Halbachsen müssen
//! größer als `tol.linear` sein. Sonst gilt die Gruppe als nicht gelöst und bleibt im letzten
//! Zustand; die Diagnose nennt die Ursache.
//!
//! Zwischenspeicher: Das Ergebnis jeder Gruppe wird mit den Werten ihrer Variablen gemerkt.
//! Stimmen Abhängigkeiten und Werte beim nächsten Lauf überein, wird es übernommen statt neu
//! gerechnet. Das ist exakt dasselbe Ergebnis, denn ein neuer Lauf ab denselben Werten rechnet
//! deterministisch dasselbe (eine gelöste Gruppe ist an ihren Werten schon gelöst). Jede
//! Änderung an Objekten oder Abhängigkeiten leert den Speicher. So rechnet beim Ziehen nur die
//! Gruppe mit dem gezogenen Punkt.

use crate::constraint::ConstraintId;
use crate::error::SketchError;
use crate::linalg::{Matrix, analyze_rows, cholesky_solve};
use crate::model::{CurveId, CurveKind, PointId, Sketch};
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
/// Nullraumanteil, ab dem eine Variable als frei gilt.
const FREE_COMPONENT: f64 = 1e-6;
/// Versuche mit steigender Dämpfung innerhalb eines Schritts.
const DAMPING_TRIES: usize = 30;

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
    vars: Vec<usize>,
    /// Bitmuster der Variablenwerte nach dem Lauf (gelöst: die übernommene Lösung; nicht
    /// gelöst: der unveränderte Start).
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
        (hit.vars == g.vars
            && hit
                .vars
                .iter()
                .zip(&hit.values)
                .all(|(&v, &bits)| x[v].to_bits() == bits))
        .then_some(&hit.outcome)
    }

    fn insert(&mut self, g: &Group, x: &[f64], outcome: Outcome) {
        self.groups.insert(
            g.constraints.clone(),
            CachedGroup {
                vars: g.vars.clone(),
                values: g.vars.iter().map(|&v| x[v].to_bits()).collect(),
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
    /// Löst alle aktiven Abhängigkeiten. Nicht lösbare Gruppen bleiben unverändert.
    pub fn solve(&mut self) -> SolveReport {
        self.solve_with_soft(&[])
    }

    /// Baut Residuen und Jacobi-Matrix der Gruppe an der Stelle `x`, ohne die Abhängigkeit `skip`.
    fn assemble(
        &self,
        g: &Group,
        col: &HashMap<usize, usize>,
        x: &[f64],
        soft: &[Soft],
        skip: Option<ConstraintId>,
    ) -> Assembled {
        let mut rows: Vec<(f64, Vec<(usize, f64)>)> = Vec::new();
        let mut owner = Vec::new();
        for (ci, &id) in g.constraints.iter().enumerate() {
            if Some(id) == skip {
                continue;
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
                rows.push((res.v, entries));
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
        skip: Option<ConstraintId>,
        tol_r: f64,
    ) -> Lm {
        let mut a = self.assemble(g, col, x, soft, skip);
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
                    let t = self.assemble(g, col, &trial, soft, skip);
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

    /// Rang, Redundanz und freie Variablen einer gelösten Gruppe an der Stelle `x`.
    fn analyze(&self, g: &Group, col: &HashMap<usize, usize>, x: &[f64]) -> Analysis {
        let a = self.assemble(g, col, x, &[], None);
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
    /// Linienlänge und beide Halbachsen einer Ellipse müssen größer als `tol.linear` sein (NaN
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
            CurveKind::Ellipse { ax, ay, minor, .. } => {
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
        let at_start = self.assemble(g, col, start, &[], None);
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
                    .levenberg_marquardt(g, col, &mut x, &[], Some(id), tol_r)
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
        let a = self.assemble(g, col, failed, &[], None);
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
        let n = self.vars.len();
        let mut parent: Vec<usize> = (0..n).collect();
        let mut in_constraint = vec![false; n];
        // Abhängigkeiten in Anlagereihenfolge, jeweils mit ihren Variablen.
        let mut active: Vec<(ConstraintId, Vec<usize>)> = Vec::new();
        for (id, c) in self.constraints.iter() {
            if !(c.alive && c.enabled && !c.driven) {
                continue;
            }
            let Some(ev) = self.evaluate(id, &self.vars) else {
                continue;
            };
            if ev.locals.is_empty() {
                continue;
            }
            for w in ev.locals.windows(2) {
                let (a, b) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
                parent[a] = b;
            }
            for &v in &ev.locals {
                in_constraint[v] = true;
            }
            active.push((id, ev.locals));
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
                });
                groups.len() - 1
            });
            groups[gi].vars.push(v);
        }
        for (id, locals) in active {
            let gi = index[&find(&mut parent, locals[0])];
            groups[gi].constraints.push(id);
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
            for v in vars.into_iter().filter(|&v| in_constraint[v]) {
                let gi = index[&find(&mut parent, v)];
                if !seen.contains(&gi) {
                    seen.push(gi);
                    groups[gi].curves.push(cid);
                }
            }
        }
        (groups, in_constraint)
    }

    /// Löst mit weichen Zusatzbedingungen (zum Ziehen). Harte Abhängigkeiten werden danach
    /// ohne die weichen exakt nachgelöst („Politur“).
    pub(crate) fn solve_with_soft(&mut self, soft: &[Soft]) -> SolveReport {
        let tol_r = RESIDUAL_FRACTION * self.tolerance().linear;
        let (groups, in_constraint) = self.build_groups();

        let mut status = SolveStatus::Solved;
        let mut redundant = Vec::new();
        let mut conflicting = Vec::new();
        let mut free_vars: HashSet<usize> = HashSet::new();
        let mut dof = 0;
        let mut iterations = 0;
        let mut group_solves = 0;
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
            let mut lm = self.levenberg_marquardt(g, &col, &mut x, &group_soft, None, tol_r);
            let mut used = lm.iterations;
            if !group_soft.is_empty() {
                // Politur: Die weichen Bedingungen liefern nur eine Näherung. Von dort aus wird
                // ohne sie exakt nachgelöst. Landet ein Spline-Parameter dabei auf der
                // Fortsetzung, wird er aufs Ende gesetzt und erneut poliert. Gelingt das nicht,
                // wird ohne Zug vom Start gelöst.
                lm = self.levenberg_marquardt(g, &col, &mut x, &[], None, tol_r);
                used += lm.iterations;
                if lm.converged && !self.acceptable(g, &lm, &x) {
                    self.clamp_spline_parameters(g, &mut x);
                    lm = self.levenberg_marquardt(g, &col, &mut x, &[], None, tol_r);
                    used += lm.iterations;
                }
                if !self.acceptable(g, &lm, &x) {
                    x = start.clone();
                    lm = self.levenberg_marquardt(g, &col, &mut x, &[], None, tol_r);
                    used += lm.iterations;
                }
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

        // Lebende Variablen, die in keiner aktiven Abhängigkeit vorkommen, sind frei.
        for v in self.live_variables() {
            if !in_constraint[v] && free_vars.insert(v) {
                dof += 1;
            }
        }
        let free_points: Vec<PointId> = self
            .points()
            .filter(|(_, p)| free_vars.contains(&p.x) || free_vars.contains(&p.y))
            .map(|(i, _)| i)
            .collect();
        let free_curves: Vec<CurveId> = self
            .curves()
            .filter(|(_, c)| {
                own_vars(&c.kind).iter().any(|v| free_vars.contains(v))
                    || c.kind.points().iter().any(|p| free_points.contains(p))
            })
            .map(|(i, _)| i)
            .collect();
        SolveReport {
            status,
            dof,
            redundant,
            conflicting,
            free_points,
            free_curves,
            iterations,
            group_solves,
        }
    }

    /// Zieht Punkte zu Zielpositionen; harte Abhängigkeiten bleiben erfüllt. Der Solver startet
    /// vom aktuellen Zustand, die gewählte Lage mehrdeutiger Abhängigkeiten (`branch`) bleibt
    /// unverändert, deshalb klappt nichts in die Spiegellage um. Nicht lösbare Gruppen bleiben
    /// unverändert (siehe `solve`).
    pub fn drag(&mut self, drags: &[Drag]) -> Result<SolveReport, SketchError> {
        for d in drags {
            self.check_point(d.point)?;
            if !d.target.is_finite() {
                return Err(SketchError::InvalidInput(
                    "Das Ziel muss eine endliche Position sein".into(),
                ));
            }
        }
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

    /// Wahr, wenn keine aktive Abhängigkeit (lebend, eingeschaltet, nicht nur anzeigend, auch
    /// interne) den Punkt direkt oder über eine Kurve benutzt, die ihn enthält.
    fn point_unconstrained(&self, p: PointId) -> bool {
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
fn own_vars(kind: &CurveKind) -> Vec<usize> {
    match kind {
        CurveKind::Circle { radius, .. } => vec![*radius],
        CurveKind::Ellipse { ax, ay, minor, .. } => vec![*ax, *ay, *minor],
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
    }
}
