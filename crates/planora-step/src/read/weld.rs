//! Zusammenziehen, Paaren und Nachziehen (STEP-Import, Aufgabe 7) auf der Rohform
//! ([`RawBody`]).
//!
//! Viele Programme schreiben jede Fläche mit eigenen Ecken (`VERTEX_POINT`) und Kanten
//! (`EDGE_CURVE`), die nur bis zur Genauigkeit der Datei zusammenpassen. Der Leser
//! - zieht Ecken zusammen, die höchstens `weld_tol` auseinanderliegen ([`weld_points`]: je
//!   Punkt der erste Vertreter in Eingabereihenfolge, nicht transitiv; der Punkt des Vertreters
//!   bleibt, nichts wird gemittelt), je Schale getrennt (sich berührende Stücke bleiben getrennt);
//! - paart Kanten mit gleichem Eckenpaar (beide Richtungen), deren Viertel- und Mittelpunkte
//!   höchstens `weld_tol` auseinanderliegen: eine Kante des Kernels mit Kurve und Parametern der
//!   ersten; die Richtung der zweiten folgt aus dem Vergleich der Tangenten ([`weld_body`]);
//! - verlangt danach, dass jede Kante genau zweimal benutzt wird ([`check_open_edges`]); sonst
//!   wird der Körper mit der Lücke zur nächsten offenen Kante oder als offene Kante ausgelassen.
//!   Lücken werden nie geschlossen;
//! - zieht Ecken, die nur bis zur Genauigkeit auf ihren Flächen liegen, genau auf den Schnitt
//!   ihrer analytischen Flächen und Kanten (P16, [`tighten_vertices`]): Der Kernel rechnet mit
//!   1e-6 und lehnt Abziehen, Rundung und Schnitt schon bei Ecken 1e-5 neben ihren Kanten ab.
//!
//! Nachbarn werden über ein Raster ([`HashGrid`]) bzw. über das Eckenpaar gesucht, nie paarweise
//! über alle; der Aufwand wächst linear mit der Zahl der Ecken und Kanten.

use crate::read::geom::Skip;
use crate::read::topo::{RawBody, place};
use planora_kernel::{Curve, Surface};
use planora_math::{DMat3, DVec3, Tolerance};
use std::collections::HashMap;

/// Raster mit Zellgröße `cell`; Nachbarsuche in 27 Zellen (der Suchabstand darf die Zellgröße
/// nicht übersteigen).
pub struct HashGrid {
    cell: f64,
    cells: HashMap<[i64; 3], Vec<(DVec3, usize)>>,
}

impl HashGrid {
    /// Leeres Raster; eine nicht positive oder nicht endliche Zellgröße gilt als 1.
    pub fn new(cell: f64) -> HashGrid {
        HashGrid {
            cell: if cell > 0.0 && cell.is_finite() {
                cell
            } else {
                1.0
            },
            cells: HashMap::new(),
        }
    }

    fn key(&self, p: DVec3) -> [i64; 3] {
        // `as` sättigt bei sehr großen Werten; solche Punkte landen dann in einer Randzelle.
        [p.x, p.y, p.z].map(|c| (c / self.cell).floor() as i64)
    }

    /// Trägt den Punkt `p` mit der Nummer `index` ein (nicht endliche Punkte werden übergangen).
    pub fn insert(&mut self, p: DVec3, index: usize) {
        if p.is_finite() {
            let k = self.key(p);
            self.cells.entry(k).or_default().push((p, index));
        }
    }

    /// Nummern aller eingetragenen Punkte höchstens `tol` (≤ Zellgröße) neben `p`, aufsteigend.
    pub fn near(&self, p: DVec3, tol: f64) -> Vec<usize> {
        let mut out = Vec::new();
        if !p.is_finite() {
            return out;
        }
        let [x, y, z] = self.key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let k = [
                        x.saturating_add(dx),
                        y.saturating_add(dy),
                        z.saturating_add(dz),
                    ];
                    if let Some(list) = self.cells.get(&k) {
                        out.extend(
                            list.iter()
                                .filter(|(q, _)| q.distance(p) <= tol)
                                .map(|(_, i)| *i),
                        );
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Je Punkt der Vertreter: der Index des ersten Punkts (in Eingabereihenfolge), der selbst
/// Vertreter ist und höchstens `tol` entfernt liegt, sonst der Punkt selbst. Nicht transitiv:
/// In einer Kette a–b–c (je knapp unter `tol`) wird b zu a, c bleibt (a ist zu weit).
pub fn weld_points(points: &[DVec3], tol: f64) -> Vec<usize> {
    if !(tol > 0.0 && tol.is_finite()) {
        return (0..points.len()).collect();
    }
    let mut grid = HashGrid::new(tol);
    points
        .iter()
        .enumerate()
        .map(|(i, p)| match grid.near(*p, tol).first() {
            Some(&r) => r,
            None => {
                grid.insert(*p, i);
                i
            }
        })
        .collect()
}

/// Zahl für Meldungen: höchstens vier gültige Stellen, Komma, ohne überflüssige Nullen
/// („0,001“, „0,001235“, „12,5“).
pub(crate) fn short_number(x: f64) -> String {
    if !x.is_finite() || x == 0.0 {
        return "0".into();
    }
    let digits = (3 - x.abs().log10().floor() as i32).max(0) as usize;
    let s = format!("{x:.digits$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    s.replace('.', ",")
}

/// Warnungstext für grobe Dateien.
pub fn coarse_warning(tol_mm: f64) -> String {
    format!(
        "Genauigkeit der Datei {} mm: Rechnen mit diesem Körper kann scheitern.",
        short_number(tol_mm)
    )
}

/// Viertel- und Mittelpunkt, an denen gepaarte Kanten verglichen werden.
const PAIR_SAMPLES: [f64; 3] = [0.25, 0.5, 0.75];

/// Punkt der Kante `k` beim Anteil `s` ihres Parameterbereichs.
fn edge_at(raw: &RawBody, k: usize, s: f64) -> DVec3 {
    let e = &raw.edges[k];
    e.curve.point(e.t0 + (e.t1 - e.t0) * s)
}

/// Tangente der Kante `k` in Laufrichtung (von `start` nach `end`) beim Anteil `s`.
fn edge_dir(raw: &RawBody, k: usize, s: f64) -> DVec3 {
    let e = &raw.edges[k];
    let t = e.curve.tangent(e.t0 + (e.t1 - e.t0) * s);
    if e.t1 < e.t0 { -t } else { t }
}

/// Läuft Kante `b` gegen Kante `a` (Vergleich der Tangenten in der Mitte)?
fn opposed(raw: &RawBody, a: usize, b: usize) -> bool {
    edge_dir(raw, a, 0.5).dot(edge_dir(raw, b, 0.5)) < 0.0
}

/// Schale (Index), in der Ecke bzw. Kante zuerst benutzt wird.
fn first_shells(raw: &RawBody) -> (Vec<Option<usize>>, Vec<Option<usize>>) {
    let mut vs = vec![None; raw.vertices.len()];
    let mut es = vec![None; raw.edges.len()];
    for (s, shell) in raw.shells.iter().enumerate() {
        for &f in &shell.faces {
            for he in raw.faces[f].loops.iter().flatten() {
                es[he.edge].get_or_insert(s);
                let e = &raw.edges[he.edge];
                vs[e.start].get_or_insert(s);
                vs[e.end].get_or_insert(s);
            }
        }
    }
    (vs, es)
}

/// Zieht Ecken zusammen ([`weld_points`] mit `tol`, je Schale) und paart Kanten: gleiches
/// Eckenpaar (beide Richtungen) und Viertel- und Mittelpunkte höchstens `tol` auseinander →
/// eine Kante (Kurve und Parameter der ersten; Halbkanten der zweiten zeigen auf die erste, bei
/// gegenläufigen Tangenten mit gekippter Richtung). Danach werden ungenutzte Ecken und Kanten
/// entfernt. Kein Lückenschließen: Was weiter auseinanderliegt, bleibt getrennt.
pub fn weld_body(raw: &mut RawBody, tol: f64) {
    let tol = tol.max(Tolerance::default().linear);
    let (vertex_shell, edge_shell) = first_shells(raw);
    // Ecken je Schale zusammenziehen.
    let mut vmap: Vec<usize> = (0..raw.vertices.len()).collect();
    let mut by_shell: Vec<Vec<usize>> = vec![Vec::new(); raw.shells.len()];
    for (v, s) in vertex_shell.iter().enumerate() {
        if let Some(s) = s {
            by_shell[*s].push(v);
        }
    }
    for members in by_shell {
        let points: Vec<DVec3> = members.iter().map(|&v| raw.vertices[v]).collect();
        for (i, r) in weld_points(&points, tol).into_iter().enumerate() {
            vmap[members[i]] = members[r];
        }
    }
    for e in &mut raw.edges {
        e.start = vmap[e.start];
        e.end = vmap[e.end];
    }
    // Kanten paaren: Kandidaten über das Eckenpaar.
    let mut by_pair: HashMap<(usize, usize, usize), Vec<usize>> = HashMap::new();
    let mut emap: Vec<(usize, bool)> = (0..raw.edges.len()).map(|k| (k, false)).collect();
    for k in 0..raw.edges.len() {
        let Some(s) = edge_shell[k] else { continue };
        let e = &raw.edges[k];
        let key = (s, e.start.min(e.end), e.start.max(e.end));
        let found = by_pair.get(&key).and_then(|cands| {
            cands.iter().find_map(|&r| {
                let flip = opposed(raw, r, k);
                PAIR_SAMPLES
                    .iter()
                    .all(|&t| {
                        let q = edge_at(raw, k, if flip { 1.0 - t } else { t });
                        edge_at(raw, r, t).distance(q) <= tol
                    })
                    .then_some((r, flip))
            })
        });
        match found {
            Some(m) => emap[k] = m,
            None => by_pair.entry(key).or_default().push(k),
        }
    }
    for f in &mut raw.faces {
        for he in f.loops.iter_mut().flatten() {
            let (r, flip) = emap[he.edge];
            he.edge = r;
            he.forward ^= flip;
        }
    }
    raw.compact();
}

/// Kleinster Suchabstand (mm) für die Partnerkante einer offenen Kante.
const MIN_GAP_SEARCH: f64 = 0.01;
/// Suchabstand für die Partnerkante in Vielfachen der Genauigkeit.
const GAP_SEARCH_TOLS: f64 = 100.0;
/// Punkte (Anteile), an denen die Lücke zwischen zwei Kanten gemessen wird.
const GAP_SAMPLES: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

/// Offene Kanten (genau einmal benutzt) in der Reihenfolge der Rohform.
fn open_edges(raw: &RawBody) -> Vec<usize> {
    let mut uses = vec![0usize; raw.edges.len()];
    for he in raw.faces.iter().flat_map(|f| f.loops.iter().flatten()) {
        uses[he.edge] += 1;
    }
    (0..raw.edges.len()).filter(|&k| uses[k] == 1).collect()
}

/// Lücke von der offenen Kante `k` zur nächsten anderen offenen Kante (Mittelpunkte höchstens
/// `radius` auseinander, die nächste zählt): größter Abstand gleichlaufender Punkte beider Kanten
/// (Enden, Viertel, Mitte). `None` ohne Partner.
fn gap_of(raw: &RawBody, grid: &HashGrid, k: usize, radius: f64) -> Option<f64> {
    let mid = edge_at(raw, k, 0.5);
    let partner = grid
        .near(mid, radius)
        .into_iter()
        .filter(|&j| j != k)
        .min_by(|&a, &b| {
            edge_at(raw, a, 0.5)
                .distance(mid)
                .total_cmp(&edge_at(raw, b, 0.5).distance(mid))
        })?;
    let flip = opposed(raw, k, partner);
    Some(
        GAP_SAMPLES
            .iter()
            .map(|&t| {
                let q = edge_at(raw, partner, if flip { 1.0 - t } else { t });
                edge_at(raw, k, t).distance(q)
            })
            .fold(0.0, f64::max),
    )
}

/// Prüft die offenen Kanten; `gaps_only`: nur Lücken melden.
fn open_edge_check(raw: &RawBody, tol: f64, gaps_only: bool) -> Result<(), Skip> {
    let open = open_edges(raw);
    if open.is_empty() {
        return Ok(());
    }
    let radius = (GAP_SEARCH_TOLS * tol).max(MIN_GAP_SEARCH);
    let mut grid = HashGrid::new(radius);
    for &k in &open {
        grid.insert(edge_at(raw, k, 0.5), k);
    }
    for &k in &open {
        if let Some(gap) = gap_of(raw, &grid, k, radius) {
            return Err(Skip(format!(
                "Lücke von {} mm an der Kante bei {}.",
                short_number(gap),
                place(edge_at(raw, k, 0.5))
            )));
        }
        if !gaps_only {
            return Err(Skip(format!(
                "Der Körper ist ungültig: Die Kante bei {} gehört nur zu einer Fläche.",
                place(edge_at(raw, k, 0.5))
            )));
        }
    }
    Ok(())
}

/// Lücken zwischen offenen Kanten (früh, gleich nach dem Paaren und dem Entfernen entarteter
/// Kanten): Die erste nur einmal benutzte Kante (in der Reihenfolge der Rohform), neben der eine
/// andere offene Kante liegt (Mittelpunkte höchstens `max(100 · tol, 0,01 mm)` auseinander; die
/// nächste zählt), ergibt „Lücke von {d} mm an der Kante bei (…).“ mit dem größten Abstand
/// gleichlaufender Punkte beider Kanten (Enden, Viertel, Mitte; `d` mit höchstens vier gültigen
/// Stellen). Offene Kanten ohne Partner meldet erst [`check_open_edges`]: Davor sagen das Einfügen
/// der Nähte (P9) und das Fertigstellen der Flächen („keinen Rand“) genauer, was fehlt.
pub fn check_gaps(raw: &RawBody, tol: f64) -> Result<(), Skip> {
    open_edge_check(raw, tol, true)
}

/// Jede Kante wird genau zweimal benutzt: Die erste nur einmal benutzte Kante ergibt eine Lücke
/// wie bei [`check_gaps`] oder, ohne Partner, „Der Körper ist ungültig: Die Kante bei (…) gehört
/// nur zu einer Fläche.“ Öfter als zweimal benutzte Kanten meldet `validate`.
pub fn check_open_edges(raw: &RawBody, tol: f64) -> Result<(), Skip> {
    open_edge_check(raw, tol, false)
}

/// Größter Rest (relativ `1 + |p|`), bis zu dem eine Ecke als genau auf ihren Flächen gilt.
const TIGHT_REL: f64 = 1e-9;
/// Höchstzahl der Gauß-Newton-Schritte je Ecke.
const TIGHTEN_STEPS: usize = 30;
/// Schritt (relativ `1 + |p|`), unter dem Gauß-Newton fertig ist.
const STEP_DONE: f64 = 1e-15;
/// Schrittweite (relativ `1 + |p|`) der Ableitungen der Flächenfunktionen.
const GRADIENT_STEP: f64 = 1e-6;

/// Eine Bedingung an eine Ecke: Wert 0 auf Fläche bzw. Kurve.
enum Condition<'a> {
    /// Abstandsfunktion einer analytischen Fläche (`Surface::implicit`).
    Surface(&'a Surface),
    /// Abstand zu einer Geraden, Kreis- oder Ellipsenkurve (drei Komponenten des Lots).
    Curve(&'a Curve),
}

/// Werte und Gradienten aller Bedingungen an der Stelle `p`.
fn residuals(conds: &[Condition], p: DVec3) -> Vec<(f64, DVec3)> {
    let h = GRADIENT_STEP * (1.0 + p.length());
    let axes = [DVec3::X, DVec3::Y, DVec3::Z];
    let mut out = Vec::new();
    for c in conds {
        match c {
            Condition::Surface(s) => {
                let g = axes.map(|a| (s.implicit(p + a * h) - s.implicit(p - a * h)) / (2.0 * h));
                out.push((s.implicit(p), DVec3::from_array(g)));
            }
            Condition::Curve(c) => {
                let off = |q: DVec3| q - c.point(c.project(q));
                let r = off(p);
                let d = axes.map(|a| (off(p + a * h) - off(p - a * h)) / (2.0 * h));
                for k in 0..3 {
                    out.push((r[k], DVec3::new(d[0][k], d[1][k], d[2][k])));
                }
            }
        }
    }
    out
}

fn largest(res: &[(f64, DVec3)]) -> f64 {
    res.iter().map(|(r, _)| r.abs()).fold(0.0, f64::max)
}

/// Gauß-Newton (kleinste Quadrate, gedämpft, kleinste Verschiebung in unbestimmten Richtungen)
/// von `p0` auf den gemeinsamen Punkt der Bedingungen. `Some` nur, wenn der größte Rest danach
/// höchstens `TIGHT_REL · (1 + |p|)` ist und die Ecke höchstens `tol` wandert.
// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn solve(conds: &[Condition], p0: DVec3, tol: f64) -> Option<DVec3> {
    let mut p = p0;
    // Bis der Schritt verschwindet (nicht nur bis zur Schranke des Rests): so genau, wie die
    // Bedingungen es hergeben.
    for _ in 0..TIGHTEN_STEPS {
        let res = residuals(conds, p);
        let mut jtj = DMat3::ZERO;
        let mut jtr = DVec3::ZERO;
        for (r, g) in &res {
            jtj += DMat3::from_cols(*g * g.x, *g * g.y, *g * g.z);
            jtr += *g * *r;
        }
        let trace = jtj.x_axis.x + jtj.y_axis.y + jtj.z_axis.z;
        let damped = jtj + DMat3::from_diagonal(DVec3::splat(1e-12 * trace.max(1e-300)));
        let step = -(damped.inverse() * jtr);
        if !step.is_finite() {
            return None;
        }
        p += step;
        if !(p.distance(p0) <= 10.0 * tol) {
            return None;
        }
        if step.length() <= STEP_DONE * (1.0 + p.length()) {
            break;
        }
    }
    let res = residuals(conds, p);
    (largest(&res) <= TIGHT_REL * (1.0 + p.length()) && p.distance(p0) <= tol).then_some(p)
}

/// Analytische Fläche (Ebene, Zylinder, Kegel, Kugel, Torus)?
fn analytic(s: &Surface) -> bool {
    !matches!(s, Surface::Nurbs(_))
}

/// Parameter von `p` auf der Kurve, bei periodischen Kurven dem bisherigen Wert `old` am
/// nächsten.
fn param_near(c: &Curve, p: DVec3, old: f64) -> f64 {
    let t = c.project(p);
    if !t.is_finite() {
        return old;
    }
    match c.period() {
        Some(per) => t + per * ((old - t) / per).round(),
        None => t,
    }
}

/// Nachziehen der Ecken (P16): Jede Ecke, deren Flächen alle analytisch sind (mindestens zwei
/// Flächen, keine an einem Pol) und deren Kanten Geraden, Kreise oder Ellipsen sind, wird genau
/// auf den Schnitt ihrer Flächen und Kanten gezogen (Gauß-Newton auf den Abstandsfunktionen,
/// kleinste Verschiebung in unbestimmten Richtungen). Passen die Kanten nicht genau dazu (grobe
/// Kurven), zählen nur die Flächen. Angenommen wird nur eine Verschiebung bis `tol` mit größtem
/// Rest bis 1e-9 · (1 + |p|); sonst bleibt die Ecke, wo sie ist. Ecken, die schon so genau liegen,
/// bleiben unberührt. Danach werden Geraden durch ihre neuen Ecken gelegt (wenn beide höchstens
/// `tol` neben der alten Geraden liegen) und die Parameter der übrigen Kanten neu bestimmt.
pub fn tighten_vertices(raw: &mut RawBody, tol: f64) {
    let n = raw.vertices.len();
    let mut faces_at: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut edges_at: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, f) in raw.faces.iter().enumerate() {
        for he in f.loops.iter().flatten() {
            let e = &raw.edges[he.edge];
            for v in [e.start, e.end] {
                if !faces_at[v].contains(&i) {
                    faces_at[v].push(i);
                }
                if !edges_at[v].contains(&he.edge) {
                    edges_at[v].push(he.edge);
                }
            }
        }
    }
    let linear = Tolerance::default().linear;
    let mut moved: Vec<Option<DVec3>> = vec![None; n];
    for v in 0..n {
        let faces = &faces_at[v];
        if faces.len() < 2 {
            continue;
        }
        let p0 = raw.vertices[v];
        let surfaces: Vec<&Surface> = faces.iter().map(|&f| &raw.faces[f].surface).collect();
        if !surfaces.iter().all(|s| analytic(s)) {
            continue;
        }
        // Pole (Kugelpol, Kegelspitze) zieht schon `seam::snap_poles` genau.
        let on_pole = surfaces.iter().any(|s| {
            s.pole_vs()
                .iter()
                .any(|&pv| s.point(0.0, pv).distance(p0) <= tol.max(linear))
        });
        if on_pole {
            continue;
        }
        let curves: Vec<&Curve> = edges_at[v].iter().map(|&k| &raw.edges[k].curve).collect();
        if curves.iter().any(|c| matches!(c, Curve::Nurbs(_))) {
            continue;
        }
        let mut conds: Vec<Condition> = surfaces.iter().map(|s| Condition::Surface(s)).collect();
        conds.extend(curves.iter().map(|c| Condition::Curve(c)));
        if largest(&residuals(&conds, p0)) <= TIGHT_REL * (1.0 + p0.length()) {
            continue;
        }
        let p = solve(&conds, p0, tol).or_else(|| {
            let only_surfaces: Vec<Condition> =
                surfaces.iter().map(|s| Condition::Surface(s)).collect();
            solve(&only_surfaces, p0, tol)
        });
        if let Some(p) = p {
            moved[v] = Some(p);
        }
    }
    for e in &mut raw.edges {
        let (a, b) = (moved[e.start], moved[e.end]);
        if a.is_none() && b.is_none() {
            continue;
        }
        let (pa, pb) = (
            a.unwrap_or(raw.vertices[e.start]),
            b.unwrap_or(raw.vertices[e.end]),
        );
        if let Curve::Line { origin, dir } = e.curve {
            let off = |p: DVec3| {
                let d = p - origin;
                (d - dir * d.dot(dir)).length()
            };
            let len = pa.distance(pb);
            if len > linear && off(pa) <= tol && off(pb) <= tol {
                let along = pb - pa;
                let sign = if along.dot(dir) < 0.0 { -1.0 } else { 1.0 };
                let new_dir = along / len * sign;
                e.curve = Curve::Line {
                    origin: pa,
                    dir: new_dir,
                };
                e.t0 = 0.0;
                e.t1 = sign * len;
                continue;
            }
        }
        e.t0 = param_near(&e.curve, pa, e.t0);
        e.t1 = param_near(&e.curve, pb, e.t1);
    }
    for (v, p) in moved.into_iter().enumerate() {
        if let Some(p) = p {
            raw.vertices[v] = p;
        }
    }
}
