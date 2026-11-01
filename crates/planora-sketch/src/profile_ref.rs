//! Profil-Verweise: gespeicherte Profile nach einer Skizzenänderung wiederfinden, mehrere
//! gewählte Flächen zu Profilen zusammenfassen (nebeneinanderliegende Flächen ergeben einen
//! Körper, wie in Fusion) und alte Profilnummern (bisherige Extraktion) umrechnen.
//!
//! Wiederfinden (Entscheidung 4 des Plans Skizzen 1): Maß `J = M / (L_R + L_F − M)` über die
//! Stücke des äußeren Rands; `L_R`, `L_F` sind die Randlängen von Verweis und Fläche in mm, `M`
//! die Länge der Überdeckung von Stücken derselben Kurve mit der Fläche auf derselben Seite.
//! Gefunden ist die Fläche mit dem besten `J`, wenn `J ≥ MATCH_MIN` und der Abstand zum
//! zweitbesten mindestens `TIE_EPS` beträgt; bei Gleichstand entscheidet dasselbe Maß über alle
//! Stücke einschließlich der Löcher, sonst ist der Verweis verloren.
//!
//! Übergang (Entscheidung 5): Zum alten Profil `n` gehören alle heutigen Flächen, deren
//! Innenpunkt in seiner äußeren Schleife und außerhalb seiner Löcher liegt.

use crate::arrangement::{
    Arrangement, STEPS_PER_TURN, polygon_area, polygon_centroid, polygon_contains,
};
use crate::geom::Geom;
use crate::model::Sketch;
use crate::profile::{
    CHECK_STEPS, ExtractedProfiles, LoopBuilder, PieceRef, ProfileRef, Run, normalized,
};
use planora_kernel::{Profile, ProfileLoop, SampledLoop};
use planora_math::{DVec2, TAU};
use std::collections::{HashMap, HashSet};

/// Kleinstes Maß `J`, ab dem ein Verweis als gefunden gilt.
pub(crate) const MATCH_MIN: f64 = 0.5;
/// Mindestabstand des besten `J` zum zweitbesten.
pub(crate) const TIE_EPS: f64 = 1e-6;
/// Parameter zweier Stücke derselben Kurve, die so nah beieinanderliegen, schließen aneinander
/// an (sie stammen aus derselben Teilung, sind also praktisch gleich).
const SAME_PARAM: f64 = 1e-9;

impl ExtractedProfiles {
    /// Index des Profils, auf das der Verweis `r` heute zeigt (siehe Modulbeschreibung);
    /// `None`, wenn er verloren ist.
    pub fn find(&self, r: &ProfileRef) -> Option<usize> {
        let angular = self.angle_based();
        let outer: Vec<f64> = self
            .refs
            .iter()
            .map(|f| measure(&r.outer, &f.outer, &angular))
            .collect();
        let best = outer.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        // `measure` ist endlich; ohne Flächen bleibt `best` −∞
        if best < MATCH_MIN {
            return None;
        }
        let tied: Vec<usize> = (0..outer.len())
            .filter(|&i| best - outer[i] < TIE_EPS)
            .collect();
        if let &[i] = tied.as_slice() {
            return Some(i);
        }
        // Gleichstand: dasselbe Maß über alle Stücke einschließlich der Löcher
        let all =
            |p: &ProfileRef| -> Vec<PieceRef> { p.outer.iter().chain(&p.holes).cloned().collect() };
        let mine = all(r);
        let full: Vec<(usize, f64)> = tied
            .iter()
            .map(|&i| (i, measure(&mine, &all(&self.refs[i]), &angular)))
            .collect();
        let best = full.iter().map(|f| f.1).fold(f64::NEG_INFINITY, f64::max);
        match full
            .iter()
            .filter(|f| best - f.1 < TIE_EPS)
            .collect::<Vec<_>>()
            .as_slice()
        {
            &[f] => Some(f.0),
            _ => None,
        }
    }

    /// Indizes der Profile zu den Verweisen `refs` (in derselben Reihenfolge); `Err(i)`, wenn
    /// Verweis `i` verloren ist oder auf dieselbe Fläche zeigt wie ein früherer.
    pub fn resolve(&self, refs: &[ProfileRef]) -> Result<Vec<usize>, usize> {
        let mut out = Vec::with_capacity(refs.len());
        for (i, r) in refs.iter().enumerate() {
            match self.find(r) {
                Some(f) if !out.contains(&f) => out.push(f),
                _ => return Err(i),
            }
        }
        Ok(out)
    }

    /// Fasst die gewählten Profile `sel` zusammen: Profile, die gemeinsame Kanten haben, werden
    /// zu einem Profil, dessen Rand die Halbkanten der gewählten Flächen bilden, deren
    /// Gegenstück zu keiner gewählten Fläche gehört; Stücke derselben Kurve, die aneinander
    /// anschließen, werden wieder ein Segment. Jedes Loch gehört zur kleinsten umschließenden
    /// äußeren Schleife. Eine einzelne Fläche ergibt genau `profiles[i]`. Reihenfolge nach dem
    /// kleinsten Index je Gruppe; ungültige und doppelte Indizes entfallen.
    pub fn merged(&self, sel: &[usize]) -> Vec<Profile> {
        let mut sel: Vec<usize> = sel
            .iter()
            .copied()
            .filter(|&i| i < self.profiles.len())
            .collect();
        sel.sort_unstable();
        sel.dedup();
        let single = |i: &usize| self.profiles[*i].clone();
        let Some(arr) = self
            .arrangement
            .as_deref()
            .filter(|_| self.faces.len() == self.profiles.len())
        else {
            return sel.iter().map(single).collect();
        };

        // Fläche jeder Halbkante der gewählten Profile
        let mut owner = vec![usize::MAX; 2 * arr.pieces.len()];
        for &i in &sel {
            for c in self.faces[i].cycles() {
                for &h in &arr.cycles[c].half_edges {
                    owner[h] = i;
                }
            }
        }
        // Zusammenhangsgruppen über gemeinsame Halbkanten
        let mut parent: Vec<usize> = (0..self.profiles.len()).collect();
        fn root(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        for &i in &sel {
            for c in self.faces[i].cycles() {
                for &h in &arr.cycles[c].half_edges {
                    let j = owner[h ^ 1];
                    if j != usize::MAX {
                        let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                        parent[a.max(b)] = a.min(b);
                    }
                }
            }
        }
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut group_of: HashMap<usize, usize> = HashMap::new();
        for &i in &sel {
            let r = root(&mut parent, i);
            let g = *group_of.entry(r).or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
            groups[g].push(i);
        }

        let mut out = Vec::new();
        for g in groups {
            if g.len() == 1 {
                out.push(single(&g[0]));
                continue;
            }
            match self.merge_group(arr, &g, &owner) {
                Some(profiles) => out.extend(profiles),
                // Kann bei gültigen Flächen nicht auftreten; zur Sicherheit einzeln.
                None => out.extend(g.iter().map(single)),
            }
        }
        out
    }

    /// Profile einer Gruppe zusammenhängender Flächen (siehe [`ExtractedProfiles::merged`]).
    fn merge_group(
        &self,
        arr: &Arrangement,
        group: &[usize],
        owner: &[usize],
    ) -> Option<Vec<Profile>> {
        let inside = |h: usize| owner[h] != usize::MAX && group.contains(&owner[h]);
        let boundary = |h: usize| inside(h) && !inside(h ^ 1);
        // nächste Halbkante im Umlauf
        let mut next: HashMap<usize, usize> = HashMap::new();
        for &i in group {
            for c in self.faces[i].cycles() {
                let hes = &arr.cycles[c].half_edges;
                for (k, &h) in hes.iter().enumerate() {
                    next.insert(h, hes[(k + 1) % hes.len()]);
                }
            }
        }
        let mut starts: Vec<usize> = next.keys().copied().filter(|&h| boundary(h)).collect();
        starts.sort_unstable();
        // Randumläufe: am Endknoten weiter um den Knoten, bis wieder eine Randhalbkante kommt
        // (innere Halbkanten werden über ihr Gegenstück übersprungen).
        let mut seen: HashSet<usize> = HashSet::new();
        let mut loops: Vec<Vec<usize>> = Vec::new();
        for &s in &starts {
            if seen.contains(&s) {
                continue;
            }
            let mut lp = Vec::new();
            let mut h = s;
            while seen.insert(h) {
                lp.push(h);
                let mut g = *next.get(&h)?;
                let mut steps = 0;
                while !boundary(g) {
                    g = *next.get(&(g ^ 1))?;
                    steps += 1;
                    if steps > next.len() {
                        return None;
                    }
                }
                h = g;
            }
            if h != s {
                return None;
            }
            loops.push(lp);
        }

        let builder = LoopBuilder {
            tol: self.tol,
            arr,
            splines: &self.splines,
        };
        let mut outers: Vec<(Vec<DVec2>, f64, ProfileLoop)> = Vec::new();
        let mut holes: Vec<(DVec2, ProfileLoop)> = Vec::new();
        for lp in loops {
            let runs = joined_runs(arr, &lp);
            let polygon = polygon_of(arr, &runs);
            let area = polygon_area(&polygon);
            let built = builder.build(&oriented(arr, runs)).ok()?;
            if area > 0.0 {
                outers.push((polygon, area, built));
            } else {
                holes.push((*polygon.first()?, built));
            }
        }
        let mut profiles: Vec<Profile> = outers
            .iter()
            .map(|o| Profile {
                outer: o.2.clone(),
                holes: Vec::new(),
            })
            .collect();
        for (p, lp) in holes {
            let host = (0..outers.len())
                .filter(|&o| polygon_contains(&outers[o].0, p))
                .min_by(|&a, &b| outers[a].1.total_cmp(&outers[b].1))?;
            profiles[host].holes.push(lp);
        }
        Some(profiles)
    }

    /// Kurven (Index), deren normierter Parameter ein Winkel ist (Kreis, Bogen, Ellipse,
    /// Ellipsenbogen): Ihre Stücke werden modulo 1 verglichen.
    fn angle_based(&self) -> HashMap<usize, bool> {
        self.arrangement
            .iter()
            .flat_map(|arr| arr.curves.iter())
            .map(|(id, g)| {
                let angle = !matches!(g, Geom::Line { .. } | Geom::Spline { .. });
                (id.index(), angle)
            })
            .collect()
    }
}

/// Maß `J = M / (L_R + L_F − M)` der Stücke `r` (Verweis) und `f` (Fläche).
fn measure(r: &[PieceRef], f: &[PieceRef], angular: &HashMap<usize, bool>) -> f64 {
    let m: f64 = r
        .iter()
        .map(|a| f.iter().map(|b| overlap(a, b, angular)).sum::<f64>())
        .sum();
    let total = r.iter().map(|p| p.len).sum::<f64>() + f.iter().map(|p| p.len).sum::<f64>();
    let d = total - m;
    if m.is_finite() && d > 0.0 {
        (m / d).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Länge der Überdeckung zweier Stücke in mm (0 bei anderer Kurve oder anderer Seite); der
/// Anteil der Überdeckung am Parameterbereich zählt mit der Länge des kürzeren Anteils.
fn overlap(a: &PieceRef, b: &PieceRef, angular: &HashMap<usize, bool>) -> f64 {
    if a.curve != b.curve || a.left != b.left {
        return 0.0;
    }
    let (sa, sb) = (a.u1 - a.u0, b.u1 - b.u0);
    if !(sa > 0.0 && sb > 0.0) {
        return 0.0;
    }
    let periodic = angular.get(&a.curve.index()).copied().unwrap_or(false);
    let shifts: &[f64] = if periodic {
        &[-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0]
    } else {
        &[0.0]
    };
    let ov: f64 = shifts
        .iter()
        .map(|k| (a.u1.min(b.u1 + k) - a.u0.max(b.u0 + k)).max(0.0))
        .sum();
    (ov / sa * a.len).min(ov / sb * b.len)
}

/// Stücke eines Randumlaufs; aneinander anschließende Stücke derselben Kurve werden eins. Ein
/// Umlauf aus einer einzigen ganzen Kreis- oder Ellipsenkurve beginnt bei Parameter 0 (wie die
/// ungeteilte Kurve).
fn joined_runs(arr: &Arrangement, hes: &[usize]) -> Vec<Run> {
    let runs: Vec<Run> = hes.iter().map(|&h| Run::of(arr, h)).collect();
    let n = runs.len();
    let Some(start) = (0..n).find(|&i| !joins(arr, &runs[(i + n - 1) % n], &runs[i])) else {
        let Some(mut all) = runs.first().copied() else {
            return Vec::new();
        };
        for &r in &runs[1..] {
            all = join(all, r);
        }
        if arr.curves[all.input].1.periodic() && all.t1 - all.t0 >= TAU - SAME_PARAM {
            all.t0 = 0.0;
            all.t1 = TAU;
        }
        return vec![all];
    };
    let mut out: Vec<Run> = Vec::with_capacity(n);
    for k in 0..n {
        let r = runs[(start + k) % n];
        match out.last_mut() {
            Some(last) if joins(arr, last, &r) => *last = join(*last, r),
            _ => out.push(r),
        }
    }
    out
}

/// Schließt `b` in Laufrichtung an `a` an (gleiche Kurve, gleiche Richtung, gleicher Parameter
/// am gemeinsamen Knoten, zusammen höchstens ein Umlauf)?
fn joins(arr: &Arrangement, a: &Run, b: &Run) -> bool {
    if a.input != b.input || a.forward != b.forward || a.to != b.from {
        return false;
    }
    let g = &arr.curves[a.input].1;
    let (x, y) = if a.forward {
        (a.t1, b.t0)
    } else {
        (b.t1, a.t0)
    };
    if g.periodic() {
        let d = (x - y).rem_euclid(TAU);
        (d <= SAME_PARAM || d >= TAU - SAME_PARAM)
            && (a.t1 - a.t0) + (b.t1 - b.t0) <= TAU + SAME_PARAM
    } else {
        (x - y).abs() <= SAME_PARAM
    }
}

/// `a` und das anschließende `b` als ein Stück.
fn join(a: Run, b: Run) -> Run {
    let span = b.t1 - b.t0;
    if a.forward {
        Run {
            t1: a.t1 + span,
            to: b.to,
            ..a
        }
    } else {
        Run {
            t0: a.t0 - span,
            to: b.to,
            ..a
        }
    }
}

/// Linienzug eines Umlaufs (geschlossen gedacht).
fn polygon_of(arr: &Arrangement, runs: &[Run]) -> Vec<DVec2> {
    let mut polygon = Vec::new();
    for r in runs {
        let pl = arr.curves[r.input].1.polyline(r.t0, r.t1, STEPS_PER_TURN);
        if pl.is_empty() {
            continue;
        }
        if r.forward {
            polygon.extend_from_slice(&pl[..pl.len() - 1]);
        } else {
            polygon.extend(pl[1..].iter().rev());
        }
    }
    polygon
}

/// Umlauf so gedreht (und wenn nötig umgekehrt), dass er beim Stück mit dem kleinsten
/// (Kurven-Index, `u0`) beginnt und es vorwärts durchläuft (wie die einzelnen Flächen).
fn oriented(arr: &Arrangement, mut runs: Vec<Run>) -> Vec<Run> {
    let rank = |r: &Run| {
        let (id, g) = &arr.curves[r.input];
        (id.index(), normalized(g, r.t0))
    };
    let Some(first) = (0..runs.len()).min_by(|&x, &y| {
        let (a, b) = (rank(&runs[x]), rank(&runs[y]));
        a.0.cmp(&b.0).then(a.1.total_cmp(&b.1))
    }) else {
        return runs;
    };
    let mut at = first;
    if !runs[first].forward {
        runs.reverse();
        for r in &mut runs {
            r.forward = !r.forward;
            std::mem::swap(&mut r.from, &mut r.to);
        }
        at = runs.len() - 1 - first;
    }
    runs.rotate_left(at);
    runs
}

/// Innenpunkt einer Fläche aus ihrer Abtastung (`[0]` äußerer Umriss, danach die Löcher):
/// waagerechte Gerade in Höhe des Schwerpunkts des äußeren Umrisses, Schnitte mit allen
/// Umrissen (gerade/ungerade), Mitte des breitesten Innenintervalls.
fn interior_point(outline: &[Vec<DVec2>]) -> Option<DVec2> {
    let y = polygon_centroid(outline.first()?).y;
    let mut xs: Vec<f64> = Vec::new();
    for poly in outline {
        let n = poly.len();
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            if (a.y > y) != (b.y > y) {
                xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
            }
        }
    }
    xs.sort_by(f64::total_cmp);
    xs.as_chunks::<2>()
        .0
        .iter()
        .max_by(|p, q| (p[1] - p[0]).total_cmp(&(q[1] - q[0])))
        .map(|&[a, b]| DVec2::new(0.5 * (a + b), y))
}

/// Verweise der heutigen Flächen (`now`) zum alten Profil `n` (`legacy`), siehe
/// [`Sketch::migrate_profile_index`].
fn migrate(
    sketch: &Sketch,
    legacy: &ExtractedProfiles,
    now: &ExtractedProfiles,
    n: usize,
) -> Option<Vec<ProfileRef>> {
    let old = legacy.profiles.get(n)?;
    let tol = sketch.tolerance();
    let outer = SampledLoop::new(&old.outer, CHECK_STEPS, tol);
    let holes: Vec<SampledLoop> = old
        .holes
        .iter()
        .map(|h| SampledLoop::new(h, CHECK_STEPS, tol))
        .collect();
    let refs: Vec<ProfileRef> = now
        .outlines
        .iter()
        .zip(&now.refs)
        .filter(|(o, _)| {
            interior_point(o)
                .is_some_and(|p| outer.contains(p) && !holes.iter().any(|h| h.contains(p)))
        })
        .map(|(_, r)| r.clone())
        .collect();
    (!refs.is_empty()).then_some(refs)
}

impl Sketch {
    /// Verweise auf die heutigen Flächen, die das alte Profil `n` (bisherige Extraktion,
    /// [`Sketch::legacy_profiles`]) bedeckt: alle Flächen, deren Innenpunkt in seiner äußeren
    /// Schleife und außerhalb seiner Löcher liegt. Zusammengefasst mit
    /// [`ExtractedProfiles::merged`] ergeben sie denselben Körper wie früher. `None`, wenn es
    /// das alte Profil nicht gibt oder keine Fläche darin liegt.
    pub fn migrate_profile_index(&self, n: usize) -> Option<Vec<ProfileRef>> {
        migrate(self, &self.legacy_profiles(), &self.profiles(), n)
    }

    /// Alte Profilnummer `n`, deren Umrechnung ([`Sketch::migrate_profile_index`]) genau die
    /// Verweise `refs` ergibt (Reihenfolge egal); `None`, wenn es keine gibt.
    pub fn legacy_index_of(&self, refs: &[ProfileRef]) -> Option<usize> {
        let (legacy, now) = (self.legacy_profiles(), self.profiles());
        (0..legacy.profiles.len()).find(|&n| {
            migrate(self, &legacy, &now, n).is_some_and(|m| {
                m.len() == refs.len()
                    && m.iter().all(|x| refs.contains(x))
                    && refs.iter().all(|x| m.contains(x))
            })
        })
    }
}
