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
//!
//! Eine Gruppe, die ohne Zug schon am Start erfüllt ist (etwa jeder Punkt mit `Fixed` nach dem
//! Auflösen eines Textes), wird ohne Schritt und ohne Kopie des Variablenvektors erkannt; das
//! Ergebnis ist dasselbe wie mit dem vollen Lauf, der dort vor dem ersten Schritt abbräche.

use crate::constraint::{ConstraintId, ConstraintKind, SHARED_END_TANGENT_DRIFT};
use crate::error::SketchError;
use crate::linalg::{Matrix, analyze_rows, cholesky_solve};
use crate::model::{CurveId, CurveKind, PointId, Sketch};
use crate::pattern::{PATTERN_ROUNDS, PatternId};
use crate::sparse::{
    Envelope, SparseRows, analyze_rows_sparse, envelope_cholesky_solve, rcm_order,
};
use planora_i18n::tr;
use planora_math::{DVec2, DVec3};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Zeit eines gemessenen Lösungslaufs, aufgeteilt nach den Teilen der Rechnung (nur zum Messen,
/// siehe [`Sketch::solve_timed`]). `normal` ist `JᵀJ` samt Gradient, `rank` die Rangbestimmung.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SolveTimings {
    /// Aufbau von Residuen und Jacobi-Matrix.
    pub assemble: Duration,
    /// Normalgleichungen: `JᵀJ` und Gradient.
    pub normal: Duration,
    /// Cholesky-Zerlegung und Lösen des Schritts.
    pub cholesky: Duration,
    /// Rang, Redundanz und freie Variablen.
    pub rank: Duration,
    /// Der ganze Aufruf.
    pub total: Duration,
    /// Zahl der Gruppen, die auf dem dünnen Weg gerechnet wurden (Skizzen 11a).
    pub sparse_groups: usize,
}

thread_local! {
    /// Zähler des laufenden `*_timed`-Aufrufs. Ohne einen solchen ist er `None`; dann läuft kein
    /// `Instant::now()`, der normale Weg kostet nur das Lesen dieses Wertes.
    static TIMINGS: Cell<Option<SolveTimings>> = const { Cell::new(None) };
}

/// Setzt den Zähler beim Verlassen (auch bei einem Panic) wieder auf `None`.
struct TimingsGuard;

impl Drop for TimingsGuard {
    fn drop(&mut self) {
        TIMINGS.with(|c| c.set(None));
    }
}

/// Führt `f` aus und rechnet die Zeit dem Teil `part` zu, wenn gerade gemessen wird.
#[inline]
fn timed<T>(part: fn(&mut SolveTimings) -> &mut Duration, f: impl FnOnce() -> T) -> T {
    if TIMINGS.with(|c| c.get().is_none()) {
        return f();
    }
    let start = Instant::now();
    let out = f();
    let spent = start.elapsed();
    TIMINGS.with(|c| {
        if let Some(mut t) = c.get() {
            *part(&mut t) += spent;
            c.set(Some(t));
        }
    });
    out
}

/// Zählt eine dünn gerechnete Gruppe, wenn gerade gemessen wird.
fn count_sparse_group() {
    TIMINGS.with(|c| {
        if let Some(mut t) = c.get() {
            t.sparse_groups += 1;
            c.set(Some(t));
        }
    });
}

/// Misst den Aufruf `f` und liefert dessen Ergebnis mit der Aufteilung der Zeit.
fn measure<T>(f: impl FnOnce() -> T) -> (T, SolveTimings) {
    TIMINGS.with(|c| c.set(Some(SolveTimings::default())));
    let _guard = TimingsGuard;
    let start = Instant::now();
    let out = f();
    let total = start.elapsed();
    let mut t = TIMINGS.with(|c| c.get()).unwrap_or_default();
    t.total = total;
    (out, t)
}

/// Höchstzahl nutzererzeugter Abhängigkeiten einer Gruppe, für die die Widerspruchsdiagnose
/// je Abhängigkeit einen eigenen Lauf rechnet (Entscheidung D4). Jeder Lauf kostet so viel wie
/// ein Lösen der Gruppe; bei mehr Abhängigkeiten meldet der Solver „nicht lösbar“ mit den
/// Abhängigkeiten, deren Residuum am Ende des gescheiterten Laufs zu groß ist.
const MAX_DIAGNOSE_RUNS: usize = 48;

/// Zeilen einer Gruppe und die Abhängigkeit (Index in `Group::constraints`) je Zeile.
type RowsPart = (Vec<(f64, Vec<(usize, f64)>)>, Vec<usize>);

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
/// Zug auf dem dünnen Weg: Senkt ein Schritt die Kosten um höchstens diesen Anteil und sind die
/// harten Residuen erfüllt, endet die weiche Phase (Ruling T3-3).
const SOFT_STALL_RATIO: f64 = 0.5;
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
        /// Analyse vom Zugbeginn übernommen (dünner Weg), nicht an diesen Werten gerechnet.
        carried: bool,
    },
    Failed {
        conflict: bool,
        found: Vec<ConstraintId>,
        iterations: usize,
    },
}

impl SolveCache {
    /// Ist nichts gemerkt? Nur für Tests.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Leerer Speicher mit Platz für `n` Gruppen.
    fn with_capacity(n: usize) -> Self {
        Self {
            groups: HashMap::with_capacity(n),
        }
    }

    /// Nimmt den Eintrag der Gruppe heraus, wenn er zu ihren Lesevariablen und deren Werten in
    /// `x` passt. Ein Eintrag, der nicht passt, ist ohnehin überholt (der alte Speicher wird
    /// nach dem Lauf verworfen). Gerufen nur für Gruppen ohne Zug: Ein Eintrag mit übernommener
    /// Analyse (`carried`) passt dann nie, die Analyse wird an der neuen Lage neu gerechnet.
    fn take(&mut self, g: &Group, x: &[f64]) -> Option<CachedGroup> {
        let hit = self.groups.remove(&g.constraints)?;
        let carried = matches!(hit.outcome, Outcome::Solved { carried: true, .. });
        (!carried
            && hit.reads == g.reads
            && hit
                .reads
                .iter()
                .zip(&hit.values)
                .all(|(&v, &bits)| x[v].to_bits() == bits))
        .then_some(hit)
    }

    /// Analyse eines gelösten Eintrags mit den Abhängigkeiten `constraints` und denselben
    /// Lesevariablen `reads`, ohne Blick auf die Werte (Zug auf dem dünnen Weg: Rang und freie
    /// Teile gelten vom Zugbeginn an weiter). Andere Lesevariablen heißen andere Spalten; dann
    /// passen die gemerkten freien Variablen nicht.
    fn analysis_for(&self, constraints: &[ConstraintId], reads: &[usize]) -> Option<Analysis> {
        let hit = self.groups.get(constraints)?;
        if hit.reads != reads {
            return None;
        }
        match &hit.outcome {
            Outcome::Solved {
                dof,
                redundant,
                free_vars,
                ..
            } => Some(Analysis {
                dof: *dof,
                redundant: redundant.clone(),
                free_vars: free_vars.clone(),
            }),
            Outcome::Failed { .. } => None,
        }
    }

    /// Übernimmt einen passenden Eintrag aus [`take`](Self::take) unverändert: Lesevariablen
    /// und Werte stimmen ja überein.
    fn keep(&mut self, constraints: Vec<ConstraintId>, hit: CachedGroup) {
        self.groups.insert(constraints, hit);
    }

    /// Merkt das Ergebnis der Gruppe an den Werten `x`; die Gruppe wird dabei verbraucht
    /// (Schlüssel und Lesevariablen wandern ohne Kopie in den Speicher).
    fn insert(&mut self, g: Group, x: &[f64], outcome: Outcome) {
        let values = g.reads.iter().map(|&v| x[v].to_bits()).collect();
        self.groups.insert(
            g.constraints,
            CachedGroup {
                reads: g.reads,
                values,
                outcome,
            },
        );
    }
}

impl SolveReport {
    /// Ist dieser Bericht schlechter als `before`, der Bericht zur Skizze vor einer Änderung? Gelöst
    /// ist nie schlechter. Sonst: Die Skizze war vorher gelöst, oder jetzt widersprechen sich
    /// Abhängigkeiten, die es vorher nicht taten, oder sie wurde von widersprüchlich zu nicht lösbar.
    /// Ein Widerspruch an anderer Stelle, den es schon gab, zählt nicht.
    pub fn is_worse_than(&self, before: &SolveReport) -> bool {
        if self.status == SolveStatus::Solved {
            return false;
        }
        before.status == SolveStatus::Solved
            || self
                .conflicting
                .iter()
                .any(|c| !before.conflicting.contains(c))
            || (before.status == SolveStatus::Conflicting
                && self.status == SolveStatus::NotConverged)
    }
}

/// Gewicht der weichen Zugbedingung. Klein gegen die harten Residuen (Gewicht 1), damit die
/// Abhängigkeiten im Zuglauf gewinnen und die Politur nur wenig korrigieren muss; groß genug,
/// damit der Zug die freien Freiheitsgrade in wenigen Schritten bewegt (Kosten des Zugs bei
/// 10 mm Abstand: 1e-2, deutlich über der Stillstandsschwelle).
const DRAG_WEIGHT: f64 = 1e-2;

/// Zug eines Punktes zu einer Zielposition (Skizzen 10: im Raum).
///
/// - `plane: None` heißt Skizzenebene, wie bisher: Es ziehen nur x und y des Ziels, z des Ziels
///   zählt nicht, und ein 3D-Punkt behält seine Höhe frei (er wandert parallel zur Skizzenebene).
/// - `plane: Some((punkt, normale))` ist die Zugebene (aktive Triadenebene oder die Ebene parallel
///   zum Bildschirm durch den gefassten Punkt): Das Ziel wird auf diese Ebene geworfen, und ein
///   3D-Punkt zieht in allen drei Koordinaten dorthin. Ein 2D-Punkt zieht nur in x und y.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub point: PointId,
    pub target: DVec3,
    pub plane: Option<(DVec3, DVec3)>,
}

impl Drag {
    /// Das Ziel, auf die Zugebene geworfen (ohne Ebene unverändert). Ein 2D-Punkt (`is_3d` falsch)
    /// bleibt in der Skizzenebene und nimmt x und y des Ziels, eine Zugebene gilt für ihn nicht.
    fn aim(&self, is_3d: bool) -> DVec3 {
        match self.plane {
            Some((o, n)) if is_3d => {
                let n = n.normalize();
                self.target - n * (self.target - o).dot(n)
            }
            _ => self.target,
        }
    }
}

/// Zusammenhangskomponente aus Variablen und Abhängigkeiten.
struct Group {
    /// Variablen der Gruppe, aufsteigend; die Stelle ist die Spalte (siehe [`Group::column`]).
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
    /// Dünner Weg (Skizzen 11a): Ordnung der Spalten nach RCM (neu → alt), einmal je
    /// `build_groups_holding` aus den Variablen der Abhängigkeiten. `None`: dichter Weg.
    order: Option<Vec<usize>>,
}

impl Group {
    /// Spalte der Variablen `v` in der Jacobi-Matrix der Gruppe: ihr Platz in `vars`
    /// (aufsteigend, siehe `build_groups_holding`).
    fn column(&self, v: usize) -> Option<usize> {
        self.vars.binary_search(&v).ok()
    }
}

/// Jacobi-Matrix einer Gruppe: dicht (kleine Gruppen) oder als dünne Zeilen.
enum Jacobian {
    Dense(Matrix),
    Sparse(SparseRows),
}

impl Group {
    /// Die RCM-Ordnung des dünnen Wegs. Dünne Zeilen baut `assemble` nur, wenn `order` gesetzt
    /// ist; wer eine [`Jacobian::Sparse`] der Gruppe hat, hat also auch die Ordnung.
    fn sparse_order(&self) -> &[usize] {
        self.order
            .as_deref()
            .expect("dünne Jacobi-Matrix nur mit RCM-Ordnung der Gruppe")
    }
}

/// Normalgleichungen eines Schritts: `JᵀJ` dicht oder als Hülle in RCM-Ordnung.
enum Normal<'a> {
    Dense(Matrix),
    Sparse(Envelope, &'a [usize]),
}

impl Normal<'_> {
    /// Maßstab der Dämpfung, siehe `MIN_DIAG`.
    fn scale(&self) -> f64 {
        match self {
            Normal::Dense(gram) => (0..gram.rows())
                .map(|i| gram.get(i, i))
                .fold(MIN_DIAG, f64::max),
            Normal::Sparse(env, _) => env.max_diagonal().max(MIN_DIAG),
        }
    }

    /// Löst `(JᵀJ + damping · I) · step = rhs`; der Schritt steht in der Spaltenordnung der
    /// Gruppe.
    fn solve(&self, damping: f64, rhs: &[f64]) -> Option<Vec<f64>> {
        match self {
            Normal::Dense(gram) => {
                let mut m = gram.clone();
                for i in 0..m.rows() {
                    m.add_to(i, i, damping);
                }
                timed(|t| &mut t.cholesky, || cholesky_solve(&m, rhs))
            }
            Normal::Sparse(env, order) => {
                let mut m = env.clone();
                m.add_diagonal(damping);
                let b: Vec<f64> = order.iter().map(|&old| rhs[old]).collect();
                let y = timed(|t| &mut t.cholesky, || envelope_cholesky_solve(&m, &b))?;
                let mut step = vec![0.0; y.len()];
                for (&old, v) in order.iter().zip(y) {
                    step[old] = v;
                }
                Some(step)
            }
        }
    }
}

/// Residuen und Jacobi-Matrix einer Gruppe.
struct Assembled {
    r: Vec<f64>,
    j: Jacobian,
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

impl Analysis {
    /// Gleiche Freiheitsgrade, überzählige Abhängigkeiten und freie Variablen (Prüfung des
    /// Kurzwegs [`Ready::FixedPoint`]).
    fn same_as(&self, other: &Analysis) -> bool {
        self.dof == other.dof
            && self.redundant == other.redundant
            && self.free_vars == other.free_vars
    }
}

/// Eine Gruppe ohne Zug, die schon am Start erfüllt ist: Levenberg-Marquardt bräche vor dem
/// ersten Schritt ab.
enum Ready {
    /// Genau ein `Fixed` an genau dem Punkt der Gruppe (jeder Punkt nach dem Auflösen eines
    /// Textes): Seine zwei Zeilen tragen je eine Eins in einer eigenen Spalte. Rang 2, also kein
    /// Freiheitsgrad, nichts überzählig, nichts frei, ohne dass die Zeilen gebaut werden.
    FixedPoint,
    /// Sonst: die Zeilen am Start, sie dienen auch der Analyse.
    Assembled(Assembled),
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

    /// Wie [`solve`](Self::solve), mit der Aufteilung der Zeit (nur zum Messen). Das Ergebnis ist
    /// bitgleich zu dem von `solve`.
    #[doc(hidden)]
    pub fn solve_timed(&mut self) -> (SolveReport, SolveTimings) {
        measure(|| self.solve())
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
            let Ok(at) = self.point_xyz(p) else {
                return false;
            };
            // Ein 3D-Punkt steht auch in z fest (Skizzen 10).
            let pin = if self.is_3d_point(p) {
                ConstraintKind::Fixed3(p, at.x, at.y, at.z)
            } else {
                ConstraintKind::Fixed(p, at.x, at.y)
            };
            match trial.add_constraint(pin) {
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

    /// Wie [`solve_holding`](Self::solve_holding), aber nur für die Zusammenhangsgruppen, zu denen
    /// die Punkte `hold` gehören: Alle anderen Gruppen bleiben, wie sie sind, und ihr Scheitern
    /// zählt nicht (etwa ein Widerspruch an ganz anderer Stelle der Skizze). `false`, wenn eine
    /// der eigenen Gruppen sich bei festen `hold` nicht lösen lässt; dann bleibt die Skizze, wie
    /// sie ist.
    pub(crate) fn solve_holding_connected(&mut self, hold: &[PointId]) -> bool {
        let own: HashSet<usize> = hold
            .iter()
            .flat_map(|&p| {
                [self.points[p].x, self.points[p].y]
                    .into_iter()
                    .chain(self.points[p].z)
            })
            .collect();
        let (groups, _) = self.build_groups();
        let keep: HashSet<ConstraintId> = groups
            .iter()
            .filter(|g| g.vars.iter().any(|v| own.contains(v)))
            .flat_map(|g| g.constraints.iter().copied())
            .collect();
        let other: Vec<ConstraintId> = self
            .constraints
            .iter()
            .filter(|(id, c)| c.alive && c.enabled && !c.driven && !keep.contains(id))
            .map(|(id, _)| id)
            .collect();
        let mut trial = self.clone();
        for id in other {
            trial.constraints[id].enabled = false;
        }
        trial.touch_structure();
        if !trial.solve_holding(hold) {
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
            vars.extend(self.points[p].z);
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
    fn assemble(&self, g: &Group, x: &[f64], soft: &[Soft], which: Rows) -> Assembled {
        timed(
            |t| &mut t.assemble,
            || self.assemble_rows(g, x, soft, which),
        )
    }

    /// Der Aufbau selbst, siehe [`assemble`](Self::assemble).
    fn assemble_rows(&self, g: &Group, x: &[f64], soft: &[Soft], which: Rows) -> Assembled {
        let (mut rows, owner) = self.constraint_rows(g, x, which);
        let hard_rows = rows.len();
        for s in soft {
            if let Some(c) = g.column(s.var) {
                rows.push((s.weight * (x[s.var] - s.target), vec![(c, s.weight)]));
            }
        }
        let (r, j) = if g.order.is_some() {
            // Dünner Weg: dieselben Einträge als dünne Zeilen.
            let (r, entries): (Vec<f64>, Vec<Vec<(usize, f64)>>) = rows.into_iter().unzip();
            let j = SparseRows::from_rows(g.vars.len(), &entries);
            (r, Jacobian::Sparse(j))
        } else {
            let mut j = Matrix::zeros(rows.len(), g.vars.len());
            let mut r = Vec::with_capacity(rows.len());
            for (i, (value, entries)) in rows.iter().enumerate() {
                r.push(*value);
                for (c, d) in entries {
                    j.add_to(i, *c, *d);
                }
            }
            (r, Jacobian::Dense(j))
        };
        Assembled {
            r,
            j,
            hard_rows,
            owner,
        }
    }

    /// Zeilen der Abhängigkeiten der Gruppe an der Stelle `x`, je Zeile Wert und Einträge
    /// `(Spalte, Ableitung)`, dazu der Index der Abhängigkeit in `g.constraints`.
    fn constraint_rows(&self, g: &Group, x: &[f64], which: Rows) -> RowsPart {
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
            let cols: Vec<Option<usize>> = ev.locals.iter().map(|v| g.column(*v)).collect();
            for res in &ev.residuals {
                let entries = cols
                    .iter()
                    .enumerate()
                    .filter_map(|(k, c)| c.map(|c| (c, res.grad(k))))
                    .collect();
                rows.push((res.v - target, entries));
                owner.push(ci);
            }
        }
        (rows, owner)
    }

    /// Levenberg-Marquardt mit einheitlicher Dämpfung `λ · max_k (JᵀJ)_kk · I` (siehe
    /// `MIN_DIAG`). Ändert `x` nur durch Schritte, die die Kosten senken.
    fn levenberg_marquardt(
        &self,
        g: &Group,
        x: &mut [f64],
        soft: &[Soft],
        which: Rows,
        tol_r: f64,
    ) -> Lm {
        let a = self.assemble(g, x, soft, which);
        self.levenberg_marquardt_from(g, x, soft, which, tol_r, a)
    }

    /// Wie [`levenberg_marquardt`](Self::levenberg_marquardt) mit den schon gebauten Zeilen `a`
    /// an der Stelle `x` (mit denselben `soft` und `which`).
    fn levenberg_marquardt_from(
        &self,
        g: &Group,
        x: &mut [f64],
        soft: &[Soft],
        which: Rows,
        tol_r: f64,
        mut a: Assembled,
    ) -> Lm {
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
            let (gradient, normal) = timed(
                |t| &mut t.normal,
                || match &a.j {
                    Jacobian::Dense(j) => (j.transpose_mul_vec(&a.r), Normal::Dense(j.gram())),
                    Jacobian::Sparse(j) => {
                        let order = g.sparse_order();
                        let env = Envelope::gram(j, order);
                        (j.transpose_mul_vec(&a.r), Normal::Sparse(env, order))
                    }
                },
            );
            let rhs: Vec<f64> = gradient.iter().map(|v| -v).collect();
            // Maßstab der Dämpfung, siehe `MIN_DIAG`.
            let scale = normal.scale();
            let mut accepted = false;
            let mut stalled = false;
            for _ in 0..DAMPING_TRIES {
                if let Some(step) = normal.solve(lambda * scale, &rhs) {
                    let mut trial = x.to_vec();
                    for (c, &v) in g.vars.iter().enumerate() {
                        trial[v] += step[c];
                    }
                    let t = self.assemble(g, &trial, soft, which);
                    let trial_cost = sumsq(&t.r);
                    if trial_cost.is_finite() && trial_cost < cost {
                        let size = g.vars.iter().map(|&v| x[v].abs()).fold(1.0_f64, f64::max);
                        let step_len = step.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
                        stalled =
                            step_len <= STALL_REL * size || (cost - trial_cost) <= STALL_REL * cost;
                        // Zug auf dem dünnen Weg (Ruling T3-3): Sind die harten Residuen schon
                        // erfüllt und sinken die Kosten je Schritt nur noch langsam, ist das
                        // Zugziel erreicht; weitere Schritte wandern nur entlang freier
                        // Richtungen (unsichtbar, ~1e-6 mm). Der dichte Weg rechnet wie bisher.
                        if !soft.is_empty()
                            && g.order.is_some()
                            && (cost - trial_cost) <= SOFT_STALL_RATIO * cost
                            && t.hard_inf() <= tol_r
                        {
                            stalled = true;
                        }
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
                .levenberg_marquardt(g, &mut x, soft, which, tol_r)
                .iterations;
        }
        let mut lm = self.levenberg_marquardt(g, &mut x, &[], which, tol_r);
        lm.iterations += iterations;
        let within = kept.iter().all(|&(id, _)| {
            self.shared_end_tangent_drift(id, &x)
                .is_some_and(|d| d <= SHARED_END_TANGENT_DRIFT)
        });
        (lm.converged && within).then_some((x, lm))
    }

    /// Rang, Redundanz und freie Variablen einer gelösten Gruppe an der Stelle `x`.
    fn analyze(&self, g: &Group, x: &[f64]) -> Analysis {
        self.analyze_assembled(g, &self.assemble(g, x, &[], Rows::All))
    }

    /// Wie [`analyze`](Self::analyze) mit den schon gebauten Zeilen `a` (alle Abhängigkeiten,
    /// ohne weiche Bedingungen) an der gelösten Stelle.
    fn analyze_assembled(&self, g: &Group, a: &Assembled) -> Analysis {
        let (independent, rank, free) = timed(
            |t| &mut t.rank,
            || match &a.j {
                Jacobian::Dense(j) => {
                    let space = analyze_rows(j, RANK_REL_TOL);
                    let free: Vec<bool> = (0..g.vars.len())
                        .map(|c| space.null_space.iter().any(|v| v[c].abs() > FREE_COMPONENT))
                        .collect();
                    (space.independent, space.rank, free)
                }
                Jacobian::Sparse(j) => {
                    let space =
                        analyze_rows_sparse(j, g.sparse_order(), RANK_REL_TOL, FREE_COMPONENT);
                    (space.independent, space.rank, space.free)
                }
            },
        );
        let mut redundant = Vec::new();
        for (row, independent) in independent.iter().enumerate() {
            let id = g.constraints[a.owner[row]];
            if !independent && !self.constraints[id].internal && !redundant.contains(&id) {
                redundant.push(id);
            }
        }
        let free_vars = g
            .vars
            .iter()
            .zip(&free)
            .filter(|(_, f)| **f)
            .map(|(&v, _)| v)
            .collect();
        Analysis {
            dof: g.vars.len().saturating_sub(rank),
            redundant,
            free_vars,
        }
    }

    /// Abhängigkeiten der Gruppe, deren Spline-Parameter in `x` außerhalb des Kurvenbereichs
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
                    x[sp.var] = t.clamp(sp.lo, sp.hi);
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
        // Im Raum (Skizzen 10); ein 2D-Punkt hat z = 0, also dieselben Abstände wie bisher.
        let at = |p: PointId| {
            let d = &self.points[p];
            DVec3::new(x[d.x], x[d.y], d.z.map_or(0.0, |i| x[i]))
        };
        let ok = |v: f64| v > tol;
        curves.iter().any(|&c| match &self.curves[c].kind {
            CurveKind::Line { a, b } => !ok(at(*a).distance(at(*b))),
            CurveKind::Circle { radius, .. } => !ok(x[*radius]),
            CurveKind::Arc { center, start, end } => {
                let m = at(*center);
                !(ok(at(*start).distance(m)) && ok(at(*end).distance(m)))
            }
            CurveKind::Ellipse {
                ax, ay, az, minor, ..
            }
            | CurveKind::EllipticArc {
                ax, ay, az, minor, ..
            } => {
                let z = az.map_or(0.0, |i| x[i]);
                !(ok(DVec3::new(x[*ax], x[*ay], z).length()) && ok(x[*minor]))
            }
            CurveKind::Spline { .. }
            | CurveKind::ControlSpline { .. }
            | CurveKind::Conic { .. }
            | CurveKind::FixedSpline { .. } => false,
        })
    }

    /// Besteht die Gruppe aus genau einem `Fixed` an genau ihrem Punkt, und liegt der Punkt in
    /// `x` schon im Rahmen von `tol_r` am Ziel? Dieselbe Rechnung wie die Residuen von `Fixed`
    /// (`x − Ziel`) und [`Assembled::hard_inf`] (NaN ist nicht erfüllt).
    fn fixed_point_ready(&self, g: &Group, x: &[f64], tol_r: f64) -> bool {
        let [id] = g.constraints[..] else {
            return false;
        };
        let ConstraintKind::Fixed(p, fx, fy) = self.constraints[id].kind else {
            return false;
        };
        let (vx, vy) = (self.points[p].x, self.points[p].y);
        let ready = g.vars.len() == 2
            && g.column(vx).is_some()
            && g.column(vy).is_some()
            && (x[vx] - fx).abs() <= tol_r
            && (x[vy] - fy).abs() <= tol_r;
        debug_assert!(
            !ready || self.assemble(g, x, &[], Rows::All).hard_inf() <= tol_r,
            "fixed-point shortcut differs from the full path"
        );
        ready
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
        start: &[f64],
        failed: &[f64],
        tol_r: f64,
        full: bool,
    ) -> (Vec<ConstraintId>, bool) {
        // Erfüllt schon der Start alles und ist dort nichts entartet (etwa beim Ziehen), ist
        // keine Abhängigkeit schuld.
        let at_start = self.assemble(g, start, &[], Rows::All);
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
                    .levenberg_marquardt(g, &mut x, &[], Rows::Without(id), tol_r)
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
        let a = self.assemble(g, failed, &[], Rows::All);
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
        let mut held = self.pattern_vars();
        held.extend(self.text_vars());
        self.build_groups_holding(&held, None)
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
            .filter_map(|(id, _)| Some((Some(id), self.constraint_locals(id, &self.vars)?)));
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
        // Gruppe je Wurzel der Vereinigung (`usize::MAX`: noch keine).
        let mut index: Vec<usize> = vec![usize::MAX; n];
        for v in (0..n).filter(|v| in_constraint[*v]) {
            let root = find(&mut parent, v);
            if index[root] == usize::MAX {
                groups.push(Group {
                    vars: Vec::new(),
                    constraints: Vec::new(),
                    curves: Vec::new(),
                    reads: Vec::new(),
                    pattern_reads: Vec::new(),
                    order: None,
                });
                index[root] = groups.len() - 1;
            }
            groups[index[root]].vars.push(v);
        }
        // Dünner Weg: Struktur von `JᵀJ` aus den Variablen der Abhängigkeiten (je Abhängigkeit
        // eine Zeile über alle ihre Variablen, eine Obermenge der echten Zeilen).
        let sparse: Vec<bool> = groups
            .iter()
            .map(|g| self.solver_path.sparse_for(g.vars.len()))
            .collect();
        let mut pattern: Vec<Vec<Vec<(usize, f64)>>> = vec![Vec::new(); groups.len()];
        for (id, locals, read) in active {
            let gi = index[find(&mut parent, locals[0])];
            if sparse[gi] {
                let g = &groups[gi];
                let row = locals
                    .iter()
                    .filter_map(|&v| g.column(v).map(|c| (c, 1.0)))
                    .collect();
                pattern[gi].push(row);
            }
            groups[gi].constraints.extend(id);
            groups[gi].pattern_reads.extend(read);
        }
        for ((g, rows), sparse) in groups.iter_mut().zip(&pattern).zip(&sparse) {
            if *sparse {
                g.order = Some(rcm_order(&SparseRows::from_rows(g.vars.len(), rows)));
            }
        }
        // Kurven, deren Variablen (Punkte oder eigene) in einer Gruppe liegen, gehören zu ihr
        // (für die Prüfung auf Entartung).
        for (cid, c) in self.curves() {
            let mut vars: Vec<usize> = c
                .kind
                .points()
                .iter()
                .flat_map(|&p| {
                    let d = &self.points[p];
                    [d.x, d.y].into_iter().chain(d.z)
                })
                .collect();
            vars.extend(own_vars(&c.kind));
            let mut seen: Vec<usize> = Vec::new();
            for &v in vars.iter().filter(|&&v| in_constraint[v]) {
                let gi = index[find(&mut parent, v)];
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
        self.solve_with_soft_holding(soft, &HashSet::new())
    }

    /// Wie [`solve_with_soft`](Self::solve_with_soft); die Variablen `extra` sind wie Exemplare
    /// Konstanten des Laufs.
    fn solve_with_soft_holding(&mut self, soft: &[Soft], extra: &HashSet<usize>) -> SolveReport {
        let texts = self.texts().next().is_some();
        if self.patterns().next().is_none() {
            if !texts {
                return self.solve_round(soft, extra, extra).0;
            }
            // Texte (Skizzen 8): Die erzeugten Punkte sind Konstanten wie Musterexemplare. Nichts
            // liest sie (Spec E), also genügt ein Lauf zwischen zwei Nachführungen.
            self.refresh_texts();
            let mut held = self.text_vars();
            held.extend(extra.iter().copied());
            let report = self.solve_round(soft, &held, extra).0;
            self.refresh_texts();
            return report;
        }
        self.refresh_patterns();
        self.refresh_texts();
        let mut held = self.pattern_vars();
        held.extend(self.text_vars());
        held.extend(extra.iter().copied());
        let max_rounds = PATTERN_ROUNDS.max(self.patterns().count() + 1);
        let (mut report, mut reads) = self.solve_round(soft, &held, extra);
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
            let (next, again) = self.solve_round(soft, &held, extra);
            rounds += 1;
            reads = again;
            report = SolveReport {
                drag_failed: report.drag_failed || next.drag_failed,
                iterations: report.iterations.max(next.iterations),
                group_solves: report.group_solves + next.group_solves,
                ..next
            };
        }
        // Texte folgen dem gelösten Rahmen, auch einem, der an Exemplaren hängt.
        if texts {
            self.refresh_texts();
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
    ///
    /// `extra`: Teil von `held`, der nur für diesen Lauf fest steht (anders als Musterexemplare, die
    /// in jedem Lauf fest stehen). Gruppen, die so eine Variable lesen, gehen nicht über den
    /// Zwischenspeicher: Ihre Freiheitsgrade gelten nur unter dem Halten, und der Schlüssel kennt
    /// das Halten nicht. Das volle Lösen danach bekäme sonst diese Zahlen.
    fn solve_round(
        &mut self,
        soft: &[Soft],
        held: &HashSet<usize>,
        extra: &HashSet<usize>,
    ) -> (SolveReport, bool) {
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
        let only_this_run = |g: &Group| g.reads.iter().any(|v| extra.contains(v));
        let start = self.vars.clone();
        let mut committed = self.vars.clone();
        let mut old_cache = std::mem::take(&mut self.solve_cache);
        let mut cache = SolveCache::with_capacity(groups.len());
        for g in groups {
            // Nur die weichen Bedingungen auf Variablen dieser Gruppe zählen. Gruppen ohne sie
            // werden genau wie von `solve` gelöst (kein Zug- und Politurlauf).
            let group_soft: Vec<Soft> = soft
                .iter()
                .filter(|s| g.column(s.var).is_some())
                .copied()
                .collect();
            // Unverändert seit dem letzten Lauf: Ergebnis übernehmen (siehe Moduldoku). Eine
            // gelöste Gruppe ist an ihren Werten schon gelöst, ein neuer Lauf bräuchte keinen
            // Schritt; eine gescheiterte rechnete ab demselben Start dasselbe.
            if group_soft.is_empty()
                && !only_this_run(&g)
                && let Some(hit) = old_cache.take(&g, &start)
            {
                match &hit.outcome {
                    Outcome::Solved {
                        dof: d,
                        redundant: red,
                        free_vars: fv,
                        ..
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
                        drag_failed |= touched(&g);
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
                cache.keep(g.constraints, hit);
                continue;
            }
            group_solves += 1;
            if g.order.is_some() {
                count_sparse_group();
            }
            // Schon am Start erfüllt und ohne Zug (etwa jeder Punkt mit `Fixed` nach dem
            // Auflösen eines Textes): Levenberg-Marquardt bräche vor dem ersten Schritt ab, mit
            // 0 Schritten, konvergiert und `x` gleich dem Start. Das wird hier ohne Kopie des
            // Variablenvektors festgestellt, und die Zeilen an dieser Stelle dienen auch der
            // Analyse. Das Ergebnis ist dasselbe wie mit dem vollen Lauf.
            // Nicht erfüllt: Die Zeilen am Start sind dieselben, mit denen Levenberg-Marquardt
            // beginnt (ohne Zug, alle Abhängigkeiten), sie werden nicht noch einmal gebaut.
            let mut start_rows: Option<Assembled> = None;
            let ready = if !group_soft.is_empty() {
                None
            } else if self.fixed_point_ready(&g, &start, tol_r) {
                Some(Ready::FixedPoint)
            } else {
                let a = self.assemble(&g, &start, &[], Rows::All);
                if a.hard_inf() <= tol_r {
                    Some(Ready::Assembled(a))
                } else {
                    start_rows = Some(a);
                    None
                }
            };
            let mut moved: Option<Vec<f64>> = None;
            let (lm, used) = if ready.is_some() {
                let lm = Lm {
                    converged: true,
                    iterations: 0,
                };
                (lm, 0)
            } else {
                let mut x = start.clone();
                let mut lm = match start_rows {
                    Some(a) => self.levenberg_marquardt_from(&g, &mut x, &[], Rows::All, tol_r, a),
                    None => self.levenberg_marquardt(&g, &mut x, &group_soft, Rows::All, tol_r),
                };
                let mut used = lm.iterations;
                if !group_soft.is_empty() {
                    // Politur: Die weichen Bedingungen liefern nur eine Näherung. Von dort aus
                    // wird ohne sie exakt nachgelöst. Landet ein Spline-Parameter dabei auf der
                    // Fortsetzung, wird er aufs Ende gesetzt und erneut poliert. Gelingt das
                    // nicht, wird ohne Zug vom Start gelöst.
                    lm = self.levenberg_marquardt(&g, &mut x, &[], Rows::All, tol_r);
                    used += lm.iterations;
                    if lm.converged && !self.acceptable(&g, &lm, &x) {
                        self.clamp_spline_parameters(&g, &mut x);
                        lm = self.levenberg_marquardt(&g, &mut x, &[], Rows::All, tol_r);
                        used += lm.iterations;
                    }
                    if !self.acceptable(&g, &lm, &x) {
                        x = start.clone();
                        lm = self.levenberg_marquardt(&g, &mut x, &[], Rows::All, tol_r);
                        used += lm.iterations;
                    }
                }
                if !lm.converged
                    && let Some((y, retry)) =
                        self.solve_keeping_tangent_drift(&g, &start, &group_soft, tol_r)
                {
                    used += retry.iterations;
                    x = y;
                    lm = retry;
                }
                moved = Some(x);
                (lm, used)
            };
            let x: &[f64] = moved.as_deref().unwrap_or(&start);
            iterations = iterations.max(used);
            // Spline-Parameter außerhalb des gezeichneten Bereichs: nicht gelöst.
            let outside = if lm.converged {
                self.spline_parameters_outside(&g, x)
            } else {
                Vec::new()
            };
            if lm.converged && outside.is_empty() && !self.degenerate(&g.curves, x) {
                // Zug auf dem dünnen Weg (Ruling F7): Rang und freie Teile vom Zugbeginn
                // übernehmen, statt sie in jedem Zugschritt neu zu rechnen; das Lösen beim
                // Loslassen rechnet sie neu (siehe `SolveCache::take`). Gruppen, die nur für
                // diesen Lauf gehaltene Werte lesen, rechnen wie bisher.
                let carried = !group_soft.is_empty() && g.order.is_some() && !only_this_run(&g);
                let analysis = match &ready {
                    Some(Ready::FixedPoint) => {
                        let analysis = Analysis {
                            dof: 0,
                            redundant: Vec::new(),
                            free_vars: Vec::new(),
                        };
                        debug_assert!(
                            analysis.same_as(&self.analyze(&g, x)),
                            "fixed-point shortcut differs from the full path"
                        );
                        analysis
                    }
                    Some(Ready::Assembled(a)) => self.analyze_assembled(&g, a),
                    None if carried => old_cache
                        .analysis_for(&g.constraints, &g.reads)
                        .unwrap_or_else(|| self.analyze(&g, x)),
                    None => self.analyze(&g, x),
                };
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
                if !only_this_run(&g) {
                    cache.insert(
                        g,
                        x,
                        Outcome::Solved {
                            dof: analysis.dof,
                            redundant: analysis.redundant,
                            free_vars: analysis.free_vars,
                            carried,
                        },
                    );
                }
            } else {
                // Beim Ziehen sucht die Diagnose für die gezogene Gruppe nicht nach der
                // Ursache; das holt das volle Lösen am Ende des Ziehens nach.
                drag_failed |= touched(&g);
                let full = group_soft.is_empty();
                let (found, conflict) = self.diagnose(&g, &start, x, tol_r, full);
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
                if full && !only_this_run(&g) {
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
        // Exemplarvariablen nicht (sie hält das Muster), ebenso erzeugte Punkte eines Textes.
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
        self.drag_holding(drags, &[])
    }

    /// Wie [`drag`](Self::drag), mit der Aufteilung der Zeit (nur zum Messen).
    #[doc(hidden)]
    pub fn drag_timed(
        &mut self,
        drags: &[Drag],
    ) -> Result<(SolveReport, SolveTimings), SketchError> {
        let (r, t) = measure(|| self.drag(drags));
        Ok((r?, t))
    }

    /// Wie [`drag`](Self::drag), aber die Punkte `hold` stehen für den Löser fest wie Exemplare eines
    /// Musters: Ein Griffzug nimmt den Stützpunkt nicht mit. Weiche Ziele allein täten das, sobald das
    /// Ziel nicht genau erreichbar ist, denn der Löser verteilt den Rest auf alle Ziele. Geht der Zug
    /// nur mit einem bewegten `hold`, scheitert die Gruppe wie jeder andere Zug.
    pub fn drag_holding(
        &mut self,
        drags: &[Drag],
        hold: &[PointId],
    ) -> Result<SolveReport, SketchError> {
        for &p in hold {
            self.check_point(p)?;
        }
        for d in drags {
            self.check_point(d.point)?;
            if !d.target.is_finite() {
                return Err(SketchError::InvalidInput(tr!("sketch-err-target-finite")));
            }
            crate::model::finite(d.target.x, || tr!("sketch-err-what-target-x"))?;
            crate::model::finite(d.target.y, || tr!("sketch-err-what-target-y"))?;
            if let Some((o, n)) = d.plane {
                // Eine Zugebene braucht einen endlichen Punkt und eine normierbare Normale.
                if !(o.is_finite() && n.is_finite() && n.length() > 0.0) {
                    return Err(SketchError::InvalidInput(tr!("sketch-err-drag-plane")));
                }
                crate::model::finite(d.target.z, || tr!("sketch-err-what-target-z"))?;
                let aim = d.aim(true);
                for v in [aim.x, aim.y, aim.z] {
                    crate::model::finite(v, || tr!("sketch-err-what-target-z"))?;
                }
            }
        }
        let mapped: Vec<Drag>;
        let drags = if self.patterns().next().is_some() {
            mapped = drags
                .iter()
                .map(
                    |d| match self.instance_drag_target(d.point, d.target.truncate()) {
                        Some((point, target)) if target.is_finite() => Drag {
                            point,
                            target: target.extend(d.target.z),
                            plane: d.plane,
                        },
                        _ => *d,
                    },
                )
                .collect();
            &mapped[..]
        } else {
            drags
        };
        // Zieht auch z: ein 3D-Punkt mit Zugebene.
        let with_z = |s: &Sketch, d: &Drag| d.plane.is_some() && s.points[d.point].z.is_some();
        let mut soft = Vec::with_capacity(2 * drags.len());
        for d in drags {
            let (xv, yv) = (self.points[d.point].x, self.points[d.point].y);
            let aim = d.aim(self.points[d.point].z.is_some());
            soft.push(Soft {
                var: xv,
                target: aim.x,
                weight: DRAG_WEIGHT,
            });
            soft.push(Soft {
                var: yv,
                target: aim.y,
                weight: DRAG_WEIGHT,
            });
            if with_z(self, d)
                && let Some(zv) = self.points[d.point].z
            {
                soft.push(Soft {
                    var: zv,
                    target: aim.z,
                    weight: DRAG_WEIGHT,
                });
            }
        }
        // Punkte ohne jede Abhängigkeit folgen dem Zug direkt. Ebenso Punkte, die keine aktive Zeile
        // liest, obwohl eine Abhängigkeit an ihrer Kurve hängt: Der Löser sähe sie in keiner Gruppe,
        // und das weiche Ziel bliebe ohne Wirkung. So etwa die Spitze eines inneren Griffs, wenn ein
        // Krümmungsgriff am Ende desselben Splines nur das Ende liest (Handtest Skizzen 7).
        // Erzeugte Punkte eines Textes folgen nie dem Zug; sie hält der Text (Skizzen 8).
        let mut unread: Option<Vec<bool>> = None;
        for d in drags {
            let free = !self.is_text_point(d.point)
                && (self.point_unconstrained(d.point) || {
                    let read = unread.get_or_insert_with(|| self.build_groups().1);
                    let (xv, yv) = (self.points[d.point].x, self.points[d.point].y);
                    self.instance_drag_target(d.point, DVec2::ZERO).is_none()
                        && !read[xv]
                        && !read[yv]
                        && self.points[d.point].z.is_none_or(|zv| !read[zv])
                });
            if free {
                let aim = d.aim(self.points[d.point].z.is_some());
                if with_z(self, d) {
                    self.set_point_xyz(d.point, aim)?;
                } else {
                    self.set_point_xy(d.point, aim.x, aim.y)?;
                }
            }
        }
        let mut held = HashSet::new();
        for &p in hold {
            // An einem Exemplar steht die Vorlage fest, wie beim Zug selbst.
            let p = self
                .instance_drag_target(p, self.pos(p))
                .map_or(p, |(source, _)| source);
            held.extend([self.points[p].x, self.points[p].y]);
            held.extend(self.points[p].z);
        }
        Ok(self.solve_with_soft_holding(&soft, &held))
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
        // Einen erzeugten Punkt eines Textes hält der Text (Skizzen 8).
        if self.instance_drag_target(p, DVec2::ZERO).is_some() || self.is_text_point(p) {
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
        let mut v: Vec<usize> = self
            .points()
            .flat_map(|(_, p)| [p.x, p.y].into_iter().chain(p.z))
            .collect();
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
        CurveKind::Ellipse {
            ax, ay, az, minor, ..
        }
        | CurveKind::EllipticArc {
            ax, ay, az, minor, ..
        } => [*ax, *ay, *minor].into_iter().chain(*az).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::ConstraintKind as K;
    use planora_math::{Frame, Tolerance};

    #[test]
    fn analysis_for_needs_the_same_reads() {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let a = s.add_point(0.0, 0.0).unwrap();
        let b = s.add_point(3.0, 0.0).unwrap();
        let id = s.add_constraint(K::Distance(a, b, 3.0)).unwrap();
        let mut cache = SolveCache::default();
        cache.groups.insert(
            vec![id],
            CachedGroup {
                reads: vec![0, 1, 2, 3],
                values: vec![0; 4],
                outcome: Outcome::Solved {
                    dof: 3,
                    redundant: Vec::new(),
                    free_vars: vec![0, 1, 2, 3],
                    carried: false,
                },
            },
        );
        let hit = cache
            .analysis_for(&[id], &[0, 1, 2, 3])
            .map(|a| a.free_vars);
        assert_eq!(hit, Some(vec![0, 1, 2, 3]));
        // Dieselben Abhängigkeiten, andere Lesevariablen: nichts übernehmen.
        assert!(cache.analysis_for(&[id], &[0, 1, 2, 4]).is_none());
        assert!(cache.analysis_for(&[id], &[0, 1, 2]).is_none());
    }

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
