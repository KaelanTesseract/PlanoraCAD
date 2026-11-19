//! Schleifenprüfung der Muster (Entscheidung 6 der Skizzen 5, Härtung nach Skizzen 5).
//!
//! Exemplarvariablen sind für den Löser Konstanten. Eine Zusammenhangsgruppe, die Exemplare von
//! Muster P liest und einen Eingang (Vorlage, Richtungslinie oder Mitte) von Muster Q enthält,
//! gibt eine Kante P → Q: Bewegt sich P, muss Q nachgeführt werden. Ein Muster auf einem Kreis
//! dieser Kanten liefe im Nachführen nie zusammen, ob der Kreis über ein Muster geht (Vorlage liest
//! eigenes Exemplar) oder über mehrere (A liest B, B liest A).
//!
//! Die Gruppen dafür brauchen weder Kurven noch Ordnung des Lösers. [`CycleGroups`] hält sie als
//! Vereinigung über die Variablen; `add_constraint` führt sie für die neue Abhängigkeit fort, statt
//! sie neu zu bauen (sonst im schlimmsten Fall quadratisch über viele Abhängigkeiten).

use crate::model::Sketch;
use crate::pattern::PatternId;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Zusammenhangsgruppen der aktiven Abhängigkeiten für die Schleifenprüfung, zum Stand
/// `revision` der Skizze.
#[derive(Clone, Debug, Default)]
pub(crate) struct CycleGroups {
    revision: u64,
    /// Exemplarvariablen: verbinden nichts, werden nur gelesen.
    held: HashSet<usize>,
    parent: Vec<usize>,
    in_constraint: Vec<bool>,
    /// Je Wurzel: Muster mit einem Eingang in der Gruppe.
    inputs: HashMap<usize, Vec<PatternId>>,
    /// Je Wurzel: Muster, deren Exemplare die Gruppe liest.
    reads: HashMap<usize, Vec<PatternId>>,
    /// Muster je Eingangsvariable und je Exemplarvariable.
    input_of: HashMap<usize, Vec<PatternId>>,
    output_of: HashMap<usize, Vec<PatternId>>,
    /// Kanten P → Q mit der Zahl der Gruppen, die sie geben.
    edges: HashMap<(PatternId, PatternId), usize>,
}

fn add_unique(to: &mut Vec<PatternId>, from: &[PatternId]) {
    for &p in from {
        if !to.contains(&p) {
            to.push(p);
        }
    }
}

impl CycleGroups {
    /// Baut die Gruppen aus allen lebenden, aktiven, treibenden Abhängigkeiten.
    fn build(sk: &Sketch) -> CycleGroups {
        let n = sk.vars.len();
        let mut g = CycleGroups {
            revision: sk.revision(),
            held: sk.pattern_vars(),
            parent: (0..n).collect(),
            in_constraint: vec![false; n],
            ..CycleGroups::default()
        };
        for (id, _) in sk.patterns() {
            let (inputs, outputs) = sk.pattern_io(id);
            for v in inputs {
                g.input_of.entry(v).or_default().push(id);
            }
            for v in outputs {
                g.output_of.entry(v).or_default().push(id);
            }
        }
        for (id, c) in sk.constraints.iter() {
            if !(c.alive && c.enabled && !c.driven) {
                continue;
            }
            if let Some(all) = sk.constraint_locals(id, &sk.vars) {
                g.add_locals(&all);
            }
        }
        g
    }

    fn root(&self, mut v: usize) -> usize {
        while self.parent[v] != v {
            v = self.parent[v];
        }
        v
    }

    fn find(&mut self, v: usize) -> usize {
        crate::solver::find(&mut self.parent, v)
    }

    /// Kanten der Gruppe mit diesen Eingängen und gelesenen Mustern um `by` ändern.
    fn count_edges(&mut self, root: usize, by: isize) {
        let (Some(ins), Some(reads)) = (self.inputs.get(&root), self.reads.get(&root)) else {
            return;
        };
        for &p in reads {
            for &q in ins {
                let e = self.edges.entry((p, q)).or_insert(0);
                *e = e.saturating_add_signed(by);
                if *e == 0 {
                    self.edges.remove(&(p, q));
                }
            }
        }
    }

    /// Ändert Eingänge oder gelesene Muster der Gruppe `root` und hält die Kanten mit.
    fn retag(&mut self, root: usize, inputs: &[PatternId], reads: &[PatternId]) {
        if inputs.is_empty() && reads.is_empty() {
            return;
        }
        self.count_edges(root, -1);
        if !inputs.is_empty() {
            add_unique(self.inputs.entry(root).or_default(), inputs);
        }
        if !reads.is_empty() {
            add_unique(self.reads.entry(root).or_default(), reads);
        }
        self.count_edges(root, 1);
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        self.count_edges(ra, -1);
        self.count_edges(rb, -1);
        self.parent[ra] = rb;
        if let Some(ins) = self.inputs.remove(&ra) {
            add_unique(self.inputs.entry(rb).or_default(), &ins);
        }
        if let Some(reads) = self.reads.remove(&ra) {
            add_unique(self.reads.entry(rb).or_default(), &reads);
        }
        self.count_edges(rb, 1);
    }

    /// Nimmt eine Abhängigkeit mit den Variablen `all` auf (wie `build_groups_holding`: gehaltene
    /// Variablen verbinden nichts; liest sie nur Gehaltenes, entfällt sie).
    fn add_locals(&mut self, all: &[usize]) {
        let (locals, read): (Vec<usize>, Vec<usize>) =
            all.iter().partition(|v| !self.held.contains(v));
        let Some(&first) = locals.first() else {
            return;
        };
        for &v in &locals {
            if !self.in_constraint[v] {
                self.in_constraint[v] = true;
                if let Some(ins) = self.input_of.get(&v).cloned() {
                    let r = self.find(v);
                    self.retag(r, &ins, &[]);
                }
            }
        }
        for w in locals.windows(2) {
            self.union(w[0], w[1]);
        }
        let mut reads: Vec<PatternId> = Vec::new();
        for h in &read {
            if let Some(ps) = self.output_of.get(h) {
                add_unique(&mut reads, ps);
            }
        }
        let r = self.find(first);
        self.retag(r, &[], &reads);
    }

    /// Erreicht `from` über die Kanten `edges` das Muster `to` (auch in null Schritten)?
    fn reaches(
        edges: &HashMap<(PatternId, PatternId), usize>,
        from: PatternId,
        to: PatternId,
    ) -> bool {
        let mut seen = vec![from];
        let mut stack = vec![from];
        while let Some(p) = stack.pop() {
            if p == to {
                return true;
            }
            for &(a, b) in edges.keys() {
                if a == p && !seen.contains(&b) {
                    seen.push(b);
                    stack.push(b);
                }
            }
        }
        false
    }

    /// Alle Muster auf einem Kreis der Kanten, nach Id geordnet.
    fn cyclic(&self) -> Vec<PatternId> {
        let mut out: Vec<PatternId> = Vec::new();
        for &(p, q) in self.edges.keys() {
            if !out.contains(&p) && Self::reaches(&self.edges, q, p) {
                out.push(p);
            }
        }
        out.sort();
        out
    }

    /// Schlösse eine Abhängigkeit mit den Variablen `all` einen Kreis, der über ihre Gruppe geht?
    /// Ein Kreis anderswo in der Skizze (etwa aus einer alten Datei) zählt nicht.
    fn closes_cycle(&self, all: &[usize]) -> bool {
        let (locals, read): (Vec<usize>, Vec<usize>) = all
            .iter()
            .filter(|&&v| v < self.parent.len())
            .partition(|v| !self.held.contains(v));
        if locals.is_empty() {
            return false;
        }
        let mut roots: Vec<usize> = locals.iter().map(|&v| self.root(v)).collect();
        roots.sort_unstable();
        roots.dedup();
        let mut ins: Vec<PatternId> = Vec::new();
        let mut reads: Vec<PatternId> = Vec::new();
        for r in &roots {
            add_unique(&mut ins, self.inputs.get(r).map_or(&[], Vec::as_slice));
            add_unique(&mut reads, self.reads.get(r).map_or(&[], Vec::as_slice));
        }
        for v in &locals {
            if !self.in_constraint[*v] {
                add_unique(&mut ins, self.input_of.get(v).map_or(&[], Vec::as_slice));
            }
        }
        for h in &read {
            add_unique(&mut reads, self.output_of.get(h).map_or(&[], Vec::as_slice));
        }
        if ins.is_empty() || reads.is_empty() {
            return false;
        }
        // Die Kanten mit der vereinigten Gruppe statt der einzelnen.
        let mut edges = self.edges.clone();
        for &r in &roots {
            let (Some(ri), Some(rr)) = (self.inputs.get(&r), self.reads.get(&r)) else {
                continue;
            };
            for &p in rr {
                for &q in ri {
                    if let Some(e) = edges.get_mut(&(p, q)) {
                        *e -= 1;
                        if *e == 0 {
                            edges.remove(&(p, q));
                        }
                    }
                }
            }
        }
        for &p in &reads {
            for &q in &ins {
                *edges.entry((p, q)).or_insert(0) += 1;
            }
        }
        reads
            .iter()
            .any(|&p| ins.iter().any(|&q| Self::reaches(&edges, q, p)))
    }
}

impl Sketch {
    /// Die Gruppen der Schleifenprüfung zum jetzigen Stand: aus dem Zwischenspeicher oder neu.
    fn cycle_groups(&self) -> Arc<CycleGroups> {
        match &self.cycle_groups {
            Some(g) if g.revision == self.revision() => Arc::clone(g),
            _ => Arc::new(CycleGroups::build(self)),
        }
    }

    /// Legt die Gruppen der Schleifenprüfung für den jetzigen Stand an, falls sie fehlen (vor dem
    /// Anlegen einer Abhängigkeit, die sie danach mit [`Sketch::follow_cycle_groups`] fortführt).
    pub(crate) fn ensure_cycle_groups(&mut self) {
        if self.patterns().next().is_none() {
            return;
        }
        let g = self.cycle_groups();
        self.cycle_groups = Some(g);
    }

    /// Nach dem Anlegen der Abhängigkeit `id` (ohne andere Änderung seit dem Stand `before`): die
    /// Gruppen mit ihr fortführen statt neu bauen. `lifted`: Das Anlegen hat Punkte in den Raum
    /// gehoben; dann gelten die Gruppen nicht mehr.
    pub(crate) fn follow_cycle_groups(
        &mut self,
        before: u64,
        id: crate::constraint::ConstraintId,
        lifted: bool,
    ) {
        let Some(mut g) = self.cycle_groups.take() else {
            return;
        };
        if lifted || g.revision != before {
            return;
        }
        let groups = Arc::make_mut(&mut g);
        let n = self.vars.len();
        let start = groups.parent.len();
        groups.parent.extend(start..n);
        groups.in_constraint.resize(n, false);
        let c = &self.constraints[id];
        if c.alive
            && c.enabled
            && !c.driven
            && let Some(all) = self.constraint_locals(id, &self.vars)
        {
            groups.add_locals(&all);
        }
        groups.revision = self.revision();
        self.cycle_groups = Some(g);
    }

    /// Schlösse die Abhängigkeit mit den Variablen `locals` (noch nicht angelegt oder gerade
    /// angelegt) eine neue Schleife zwischen Vorlage und eigenem Exemplar, auch über weitere
    /// Muster? Geprüft werden nur Kreise durch die Gruppe, in der sie liegt: Eine Schleife anderswo
    /// in der Skizze (etwa aus einer alten Datei) sperrt fremde Befehle nicht.
    pub(crate) fn pattern_cycle(&self, locals: &[usize]) -> bool {
        if self.patterns().next().is_none() {
            return false;
        }
        self.cycle_groups().closes_cycle(locals)
    }

    /// Alle lebenden Muster in einer Schleife, auch über mehrere Muster (beim Laden, vor und nach
    /// Befehlen, die Muster ändern oder auflösen).
    pub(crate) fn cyclic_patterns(&self) -> Vec<PatternId> {
        if self.patterns().next().is_none() {
            return Vec::new();
        }
        self.cycle_groups().cyclic()
    }
}
