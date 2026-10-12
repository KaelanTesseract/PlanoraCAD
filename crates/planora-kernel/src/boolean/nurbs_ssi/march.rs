//! Verfolgen der Schnittkurve: Vorhersage längs `g_a × g_b`, Korrektur mit 3-D-Newton,
//! Schrittweite nach Krümmung und Sehnenfehler, Ende am Anfang, am Rand oder an Grenzen;
//! Treffen auf verfolgte Kurven und tangentiale Stellen werden gemeldet.

use std::collections::HashMap;

use super::{
    CellKey, Hints, MAX_CURVE_LENGTH_FACTOR, MAX_MARCH_STEPS, MSG_BRANCH, MSG_LOST, MSG_SINGULAR,
    MSG_TANGENTIAL, PARALLEL_NORMALS, Probe, Ssi, msg_too_long, msg_too_many_steps,
};
use planora_math::DVec3;

/// Zellgröße des Suchrasters für Polylinienstücke (· h_max; muss die Stücklänge abdecken).
pub(super) const CELL_FACTOR: f64 = 4.0;
/// Rückkehr zum Anfang frühestens nach so vielen Punkten.
const RETURN_STEPS: usize = 8;
/// Rückkehr zum Anfang: Abstand unter `RETURN_FACTOR · h` (dann Prüfung „dieselbe Kurve“).
const RETURN_FACTOR: f64 = 1.5;
/// Vorauswahl für das Treffen auf eine verfolgte Kurve: Abstand unter `HIT_FACTOR · h`; ob der
/// Punkt wirklich auf ihr liegt, entscheidet die Korrektur (nahe, andere Kurven sind kein
/// Treffer).
const HIT_FACTOR: f64 = 0.5;
/// Ein Startpunkt wird nur gegen verfolgte Kurven näher als `SKIP_FACTOR · h_max` geprüft.
const SKIP_FACTOR: f64 = 2.0;
/// Nachbarstücke derselben Verfolgung innerhalb dieser Bogenlänge (· h_max) zählen nicht als
/// Treffer.
const NEIGHBOR_ARC: f64 = 3.0;
/// Höchstzahl Newton-Schritte der Korrektur.
const CORRECTOR_STEPS: usize = 20;
/// Ab diesem Schritt gilt ein wieder wachsender Rest als Divergenz.
const CORRECTOR_GRACE: usize = 4;
/// Die Korrektur gilt als gescheitert, wenn ein Schritt weiter als dieses Vielfache der
/// Schrittweite führt.
const CORRECT_REACH: f64 = 2.0;
/// Größte Drehung der Tangente je Schritt (Bogenmaß).
const MAX_TURN: f64 = 0.35;
/// Ein Schritt darf höchstens so viel weiter als die Schrittweite führen.
const STEP_REACH: f64 = 1.5;
/// Sehnenfehler eines Bogens: `Länge · Drehung / CHORD_FACTOR = h² κ / CHORD_FACTOR`.
const CHORD_FACTOR: f64 = 8.0;
/// Größtes Wachstum der Schrittweite je Schritt.
const MAX_GROWTH: f64 = 2.0;
/// Genauigkeit des Randpunkts bei der Bisektion (mm).
const BISECT_TOL: f64 = 1e-9;
/// Höchstzahl Bisektionsschritte.
const BISECT_STEPS: usize = 60;
/// Hat `|g_a × g_b|` längs der Kurve ein örtliches Minimum unter diesem Wert, wird dort genau
/// nach einer tangentialen Stelle (Verzweigungspunkt zweier Kurvenäste) gesucht.
const SINGULAR_SCAN: f64 = 0.05;
/// … und das Minimum deutlich ist: höchstens dieser Anteil des größeren Nachbarwerts. Geht
/// `|g_a × g_b|` an einer tangentialen Stelle linear gegen null, ist das Verhältnis der drei
/// Stichproben um sie höchstens 1/3; Rauschen auf fast gleichen Werten löst keine Suche aus.
const SINGULAR_DIP: f64 = 0.5;
/// Schritte der Suche (Goldener Schnitt) nach dem Minimum von `|g_a × g_b|`.
const GOLDEN_STEPS: usize = 40;
/// Eine geschlossene Schleife, kleiner als dieser Anteil der Diagonale, deren Inneres innerhalb
/// `tol.linear` auf der anderen Fläche liegt, gilt als Berührung (wie bei analytischen Flächen).
const LOOP_TOUCH_MAX: f64 = 1e-2;
/// … wobei der Prüfpunkt mindestens diesen Anteil des Schleifendurchmessers von ihr entfernt ist.
const LOOP_TOUCH_CLEARANCE: f64 = 0.25;

/// Ergebnis der Korrektur.
enum Corr {
    Ok(Probe),
    /// Gescheitert, Fußpunkt am offenen Rand: der Punkt liegt jenseits der Freiformfläche.
    Outside,
    /// Normalen (fast) parallel.
    Singular,
    Failed,
}

/// Art der Korrektur ohne Daten (für Wächter in `step`).
enum CorrKind {
    Outside,
    Other,
}

/// Ergebnis eines Schritts.
struct Step {
    probe: Probe,
    t: DVec3,
    dist: f64,
    turn: f64,
    boundary: bool,
}

/// Verfolgte Halbkurve (Polylinie) samt Bogenlänge und Startwerten je Punkt.
pub(super) struct Poly {
    pts: Vec<DVec3>,
    arc: Vec<f64>,
    hints: Vec<Hints>,
    trace: usize,
}

/// Fertiger Zweig.
pub(super) struct Branch {
    pub(super) pts: Vec<DVec3>,
    pub(super) hints: Vec<Hints>,
    pub(super) closed: bool,
}

#[derive(Default)]
struct Budget {
    steps: usize,
    length: f64,
}

impl Ssi<'_> {
    /// 3-D-Newton auf `F_a = 0`, `F_b = 0`, `t · (q − plane) = 0` von `start` aus, bis alle drei
    /// Reste höchstens `eps` betragen.
    fn correct(
        &self,
        start: DVec3,
        plane: DVec3,
        t: DVec3,
        hints: Hints,
        reach: f64,
        eps: f64,
    ) -> Result<Corr, String> {
        let mut q = start;
        let mut hints = hints;
        let mut last: Option<Probe> = None;
        let mut prev_res = f64::INFINITY;
        for k in 0..CORRECTOR_STEPS {
            let Some(pr) = self.probe(q, hints)? else {
                return Ok(Corr::Failed);
            };
            hints = pr.hints();
            last = Some(pr);
            let r2 = t.dot(q - plane);
            let res = pr.residual();
            if res <= eps && r2.abs() <= eps {
                return Ok(Corr::Ok(pr));
            }
            if k >= CORRECTOR_GRACE && res > prev_res {
                break;
            }
            prev_res = res;
            let n = pr.cross();
            if n.length() < PARALLEL_NORMALS {
                return Ok(if pr.on_edge() {
                    Corr::Outside
                } else {
                    Corr::Singular
                });
            }
            let det = t.dot(n);
            if det.abs() < PARALLEL_NORMALS {
                break;
            }
            let (ga, gb) = (pr.ea.g, pr.eb.g);
            let delta = -(gb.cross(t) * pr.ea.f + t.cross(ga) * pr.eb.f + n * r2) / det;
            if !delta.is_finite() {
                return Err(super::MSG_NUMERIC.into());
            }
            if delta.length() > reach {
                break;
            }
            q += delta;
        }
        Ok(match last {
            Some(p) if p.on_edge() => Corr::Outside,
            _ => Corr::Failed,
        })
    }

    /// Ein Schritt von `p` in Richtung `t` mit Schrittweite `h` (wird bei Bedarf halbiert).
    fn step(&self, p: &Probe, t: DVec3, sign: f64, h: &mut f64) -> Result<Step, String> {
        loop {
            let at_min = *h <= self.h_min;
            let pred = p.q + t * *h;
            let corr = self.correct(pred, pred, t, p.hints(), CORRECT_REACH * *h, self.eps)?;
            let corr_kind = match corr {
                Corr::Outside => CorrKind::Outside,
                _ => CorrKind::Other,
            };
            let fail = match corr {
                Corr::Ok(q) => match self.tangent(&q) {
                    None => MSG_TANGENTIAL,
                    Some(tq) => {
                        let tq = tq * sign;
                        let d = q.q - p.q;
                        let dist = d.length();
                        let turn = t.dot(tq).clamp(-1.0, 1.0).acos();
                        let forward = tq.dot(t) > 0.0 && d.dot(t) > 0.0;
                        let good = forward
                            && turn <= MAX_TURN
                            && dist <= STEP_REACH * *h
                            && dist * turn <= CHORD_FACTOR * self.sagitta;
                        if good || (at_min && forward) {
                            return Ok(Step {
                                probe: q,
                                t: tq,
                                dist,
                                turn,
                                boundary: false,
                            });
                        }
                        MSG_LOST
                    }
                },
                // Am Rand: Auch eine gescheiterte (statt „jenseits“ gemeldete) Korrektur endet die
                // Kurve, wenn der vorhergesagte Punkt jenseits des Rands der Freiformfläche liegt.
                Corr::Outside | Corr::Failed
                    if at_min
                        && (matches!(corr_kind, CorrKind::Outside)
                            || self.beyond_edge(pred, p.hints())?) =>
                {
                    let end = self.bisect(p, t, *h)?;
                    let dist = end.q.distance(p.q);
                    return Ok(Step {
                        probe: end,
                        t,
                        dist,
                        turn: 0.0,
                        boundary: true,
                    });
                }
                Corr::Outside | Corr::Failed => MSG_LOST,
                Corr::Singular => MSG_TANGENTIAL,
            };
            if at_min {
                return Err(fail.into());
            }
            *h = (*h * 0.5).max(self.h_min);
        }
    }

    /// Liegt `x` jenseits des Rands einer Freiformseite (Fußpunkt auf einem offenen Rand und
    /// nicht auf der Fläche)?
    fn beyond_edge(&self, x: DVec3, hints: Hints) -> Result<bool, String> {
        for (side, hint) in [(&self.a, hints.a), (&self.b, hints.b)] {
            if side.nurbs.is_none() {
                continue;
            }
            if let Some(e) = self.eval_side(side, x, hint)?
                && e.edge
                && e.f.abs() > self.eps
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Randpunkt zwischen `p` (gültig) und `p + h·t` (jenseits des Rands), auf `BISECT_TOL`.
    /// Gültig heißt hier: Reste höchstens `BISECT_TOL` (jenseits des Rands ist der Abstand zur
    /// Freiformfläche der Abstand zum Rand, der Randpunkt liegt also auf `BISECT_TOL` genau).
    fn bisect(&self, p: &Probe, t: DVec3, h: f64) -> Result<Probe, String> {
        let eps = BISECT_TOL.max(self.res).min(self.eps);
        let (mut lo, mut hi) = (0.0, h);
        let mut best = *p;
        for _ in 0..BISECT_STEPS {
            if hi - lo <= BISECT_TOL {
                break;
            }
            let mid = 0.5 * (lo + hi);
            let pred = p.q + t * mid;
            match self.correct(pred, pred, t, p.hints(), CORRECT_REACH * h, eps)? {
                Corr::Ok(q) => {
                    lo = mid;
                    best = q;
                }
                _ => hi = mid,
            }
        }
        Ok(best)
    }

    /// Kurvenpunkt in der Ebene durch `plane` senkrecht zu `t`, von `start` aus korrigiert;
    /// `scale` ist die Größenordnung des Wegs (Abstand zur Kurve), die Korrektur darf je Schritt
    /// höchstens `CORRECT_REACH · scale` weit gehen.
    pub(super) fn point_in_plane(
        &self,
        start: DVec3,
        plane: DVec3,
        t: DVec3,
        hints: Hints,
        scale: f64,
    ) -> Result<Option<Probe>, String> {
        let reach = CORRECT_REACH * scale.max(start.distance(plane)).max(self.h_min);
        Ok(
            match self.correct(start, plane, t, hints, reach, self.eps)? {
                Corr::Ok(q) => Some(q),
                _ => None,
            },
        )
    }

    /// Liegt `target` auf derselben Schnittkurve wie der Kurvenpunkt `from` (Tangente `t`)?
    /// Korrektur von `from` in die Ebene durch `target` senkrecht zu `t`.
    fn same_curve(
        &self,
        from: DVec3,
        hints: Hints,
        t: DVec3,
        target: DVec3,
    ) -> Result<bool, String> {
        Ok(self
            .point_in_plane(from, target, t, hints, 0.0)?
            .is_some_and(|q| q.q.distance(target) <= self.same))
    }

    fn key(&self, p: DVec3) -> CellKey {
        let c = p / self.cell;
        (c.x.floor() as i64, c.y.floor() as i64, c.z.floor() as i64)
    }

    /// Indizes `(Polylinie, Stück)` in den Rasterzellen um `p`.
    fn near_segments(&self, p: DVec3) -> Vec<(usize, usize)> {
        let (x, y, z) = self.key(p);
        let mut out = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let k = (
                        x.saturating_add(dx),
                        y.saturating_add(dy),
                        z.saturating_add(dz),
                    );
                    if let Some(v) = self.grid.get(&k) {
                        out.extend_from_slice(v);
                    }
                }
            }
        }
        out
    }

    fn push_point(&mut self, pid: usize, q: DVec3, hints: Hints) {
        let poly = &mut self.polys[pid];
        let a = poly.pts[poly.pts.len() - 1];
        let s = poly.arc[poly.arc.len() - 1] + a.distance(q);
        let seg = poly.pts.len() - 1;
        poly.pts.push(q);
        poly.arc.push(s);
        poly.hints.push(hints);
        let key = self.key((a + q) * 0.5);
        self.grid.entry(key).or_default().push((pid, seg));
    }

    /// Je Polylinie das nächste Stück zu `x` innerhalb `radius`, das `keep` zulässt:
    /// `(Polylinie, Stück, Anteil)`, nach Abstand geordnet.
    fn nearest_per_poly(
        &self,
        x: DVec3,
        radius: f64,
        keep: impl Fn(usize, usize) -> bool,
    ) -> Vec<(usize, usize, f64)> {
        let mut best: HashMap<usize, (f64, usize, f64)> = HashMap::new();
        for (k, j) in self.near_segments(x) {
            if !keep(k, j) {
                continue;
            }
            let poly = &self.polys[k];
            let (d, f) = point_segment(x, poly.pts[j], poly.pts[j + 1]);
            if d < radius && best.get(&k).is_none_or(|b| d < b.0) {
                best.insert(k, (d, j, f));
            }
        }
        let mut list: Vec<(usize, (f64, usize, f64))> = best.into_iter().collect();
        list.sort_by(|a, b| a.1.0.total_cmp(&b.1.0).then(a.0.cmp(&b.0)));
        list.into_iter().map(|(k, (_, j, f))| (k, j, f)).collect()
    }

    /// Liegt `x` (auf beiden Flächen) auf der Kurve der Polylinie `k` nahe Stück `j`?
    fn on_poly(&self, k: usize, j: usize, f: f64, x: DVec3) -> Result<bool, String> {
        let poly = &self.polys[k];
        let (a, b) = (poly.pts[j], poly.pts[j + 1]);
        let dir = (b - a).normalize_or_zero();
        let y = a + (b - a) * f;
        if y.distance(x) <= self.same {
            return Ok(true);
        }
        if dir == DVec3::ZERO {
            return Ok(false);
        }
        let hints = if f < 0.5 {
            poly.hints[j]
        } else {
            poly.hints[j + 1]
        };
        self.same_curve(y, hints, dir, x)
    }

    /// Trifft der neue Punkt `q` (Stück `a → q`, Bogenlänge `arc_q`) eine schon verfolgte Kurve?
    /// Vorauswahl über den Strecke–Strecke-Abstand, Entscheidung über die Korrektur.
    fn hits(
        &self,
        pid: usize,
        trace: usize,
        a: DVec3,
        q: DVec3,
        arc_q: f64,
        h: f64,
    ) -> Result<bool, String> {
        let radius = HIT_FACTOR * h;
        let neighbour = |k: usize, j: usize| {
            let poly = &self.polys[k];
            if poly.trace != trace {
                return false;
            }
            let gap = if k == pid {
                arc_q - poly.arc[j + 1]
            } else {
                arc_q + poly.arc[j]
            };
            gap < NEIGHBOR_ARC * self.h_max
        };
        let near = self.nearest_per_poly(q, radius + a.distance(q), |k, j| {
            !neighbour(k, j)
                && segment_distance(a, q, self.polys[k].pts[j], self.polys[k].pts[j + 1]) < radius
        });
        for (k, j, f) in near {
            if self.on_poly(k, j, f, q)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Sucht zwischen den Kurvenpunkten `p0` und `p1` das Minimum von `|g_a × g_b|` (Goldener
    /// Schnitt über Punkte der Kurve in Ebenen senkrecht zur Sehne). Ein Minimum unter
    /// `PARALLEL_NORMALS` (oder eine scheiternde Korrektur) ist eine tangentiale Stelle.
    fn singular_between(&self, p0: &Probe, p1: &Probe) -> Result<bool, String> {
        let chord = p1.q - p0.q;
        let dir = chord.normalize_or_zero();
        if dir == DVec3::ZERO {
            return Ok(false);
        }
        let at = |s: f64| -> Result<Option<f64>, String> {
            let x = p0.q + chord * s;
            let hints = if s < 0.5 { p0.hints() } else { p1.hints() };
            Ok(self
                .point_in_plane(x, x, dir, hints, chord.length())?
                .map(|q| q.cross().length()))
        };
        let g = 0.5 * (5f64.sqrt() - 1.0);
        let (mut lo, mut hi) = (0.0, 1.0);
        let (mut x1, mut x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
        let (Some(mut f1), Some(mut f2)) = (at(x1)?, at(x2)?) else {
            return Ok(true);
        };
        for _ in 0..GOLDEN_STEPS {
            if f1.min(f2) < PARALLEL_NORMALS {
                return Ok(true);
            }
            if f1 < f2 {
                hi = x2;
                x2 = x1;
                f2 = f1;
                x1 = hi - g * (hi - lo);
                let Some(f) = at(x1)? else { return Ok(true) };
                f1 = f;
            } else {
                lo = x1;
                x1 = x2;
                f1 = f2;
                x2 = lo + g * (hi - lo);
                let Some(f) = at(x2)? else { return Ok(true) };
                f2 = f;
            }
        }
        Ok(f1.min(f2) < PARALLEL_NORMALS)
    }

    /// Verfolgt eine Halbkurve vom Startpunkt in Richtung `sign · (g_a × g_b)`. Liefert die
    /// Punkte, ihre Startwerte und ob die Kurve zum Anfang zurückkehrte.
    fn march(
        &mut self,
        seed: &Probe,
        sign: f64,
        trace: usize,
        budget: &mut Budget,
    ) -> Result<(Vec<DVec3>, Vec<Hints>, bool), String> {
        let pid = self.polys.len();
        self.polys.push(Poly {
            pts: vec![seed.q],
            arc: vec![0.0],
            hints: vec![seed.hints()],
            trace,
        });
        let start = seed.q;
        let mut p = *seed;
        let mut t = self.tangent(&p).ok_or(MSG_TANGENTIAL)? * sign;
        let mut h = self.h_max;
        let max_len = MAX_CURVE_LENGTH_FACTOR * self.diag.max(self.h_min);
        // Für die Suche tangentialer Stellen: vorletzter Punkt und die letzten beiden |g_a × g_b|.
        let mut before = *seed;
        let mut sin_before = f64::INFINITY;
        let mut sin_p = p.cross().length();
        loop {
            budget.steps += 1;
            if budget.steps > MAX_MARCH_STEPS {
                return Err(msg_too_many_steps());
            }
            if budget.length > max_len {
                return Err(msg_too_long());
            }
            let st = self.step(&p, t, sign, &mut h)?;
            let q = st.probe;
            let n = self.polys[pid].pts.len();
            // (a) Rückkehr zum Anfang
            if sign > 0.0
                && !st.boundary
                && n >= RETURN_STEPS
                && q.q.distance(start) < RETURN_FACTOR * h
                && self.same_curve(q.q, q.hints(), st.t, start)?
            {
                if st.t.dot(start - q.q) > 0.0 {
                    self.push_point(pid, q.q, q.hints());
                }
                self.push_point(pid, start, seed.hints());
                let poly = &self.polys[pid];
                return Ok((poly.pts.clone(), poly.hints.clone(), true));
            }
            if st.dist > 0.0 {
                // (c) Treffen auf eine andere (oder dieselbe) Kurve
                let arc_q = self.polys[pid].arc[n - 1] + st.dist;
                if self.hits(pid, trace, p.q, q.q, arc_q, h)? {
                    return Err(MSG_BRANCH.into());
                }
                // Tangentiale Stelle zwischen den Punkten (örtliches Minimum von |g_a × g_b|)?
                let sin_q = q.cross().length();
                if sin_p < SINGULAR_SCAN
                    && sin_p <= SINGULAR_DIP * sin_before.max(sin_q)
                    && sin_p < sin_before
                    && sin_p <= sin_q
                    && self.singular_between(&before, &q)?
                {
                    return Err(MSG_SINGULAR.into());
                }
                (before, sin_before, sin_p) = (p, sin_p, sin_q);
                budget.length += st.dist;
                self.push_point(pid, q.q, q.hints());
            }
            // (b) Rand der Freiformfläche
            if st.boundary {
                let poly = &self.polys[pid];
                return Ok((poly.pts.clone(), poly.hints.clone(), false));
            }
            // Schrittweite nach Krümmung (Winkel der letzten beiden Tangenten) und Sehnenfehler.
            let kappa = if st.dist > 0.0 {
                st.turn / st.dist
            } else {
                0.0
            };
            let by_curvature = if kappa > 0.0 {
                (CHORD_FACTOR * self.sagitta / kappa).sqrt()
            } else {
                self.h_max
            };
            h = by_curvature
                .clamp(self.h_min, self.h_max)
                .min(MAX_GROWTH * h);
            p = q;
            t = st.t;
        }
    }

    pub(super) fn trace(&mut self, seed: &Probe, trace: usize) -> Result<Branch, String> {
        let mut budget = Budget::default();
        let (fwd, fh, closed) = self.march(seed, 1.0, trace, &mut budget)?;
        if closed {
            return Ok(Branch {
                pts: fwd,
                hints: fh,
                closed: true,
            });
        }
        let (bwd, bh, _) = self.march(seed, -1.0, trace, &mut budget)?;
        let mut pts: Vec<DVec3> = bwd.into_iter().rev().collect();
        let mut hints: Vec<Hints> = bh.into_iter().rev().collect();
        pts.extend(fwd.into_iter().skip(1));
        hints.extend(fh.into_iter().skip(1));
        Ok(Branch {
            pts,
            hints,
            closed: false,
        })
    }

    /// Liegt der Startpunkt auf einer schon verfolgten Kurve? Je Polylinie, die näher als
    /// `SKIP_FACTOR · h_max` liegt, entscheidet die Korrektur vom nächsten Polylinienpunkt aus
    /// (eine nahe, aber andere Kurve wird nicht übersprungen).
    pub(super) fn on_traced(&self, s: &Probe) -> Result<bool, String> {
        for (k, j, f) in self.nearest_per_poly(s.q, SKIP_FACTOR * self.h_max, |_, _| true) {
            if self.on_poly(k, j, f, s.q)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Ist die geschlossene Schleife eine Berührung innerhalb der Toleranz? Klein (Durchmesser
    /// höchstens `LOOP_TOUCH_MAX · Diagonale`), und der Fußpunkt ihres Schwerpunkts auf der
    /// Freiformseite liegt fern der Schleife und innerhalb `tol.linear` auf der anderen Fläche
    /// (die Flächen weichen im Inneren der Schleife nicht weiter voneinander ab).
    pub(super) fn loop_is_touch(&self, br: &Branch) -> Result<bool, String> {
        let n = br.pts.len().max(1) as f64;
        let c = br.pts.iter().copied().sum::<DVec3>() / n;
        let dia = 2.0 * br.pts.iter().map(|p| p.distance(c)).fold(0.0, f64::max);
        if dia > LOOP_TOUCH_MAX * self.diag {
            return Ok(false);
        }
        let (side, other) = if self.a.nurbs.is_some() {
            (&self.a, &self.b)
        } else {
            (&self.b, &self.a)
        };
        let Some(e) = self.eval_side(side, c, None)? else {
            return Ok(false);
        };
        let s = side.foot_point(c, &e);
        let clear = br
            .pts
            .iter()
            .map(|p| p.distance(s))
            .fold(f64::INFINITY, f64::min);
        if clear < LOOP_TOUCH_CLEARANCE * dia {
            return Ok(false);
        }
        Ok(self
            .eval_side(other, s, None)?
            .is_some_and(|o| o.f.abs() <= self.tol.linear))
    }
}

/// Zweige mit aneinanderliegenden Enden (`< tol`) verbinden; ein offener Zweig, dessen Enden
/// zusammenfallen, wird geschlossen.
pub(super) fn join(branches: Vec<Branch>, tol: f64) -> Vec<Branch> {
    let (mut done, mut open): (Vec<Branch>, Vec<Branch>) =
        branches.into_iter().partition(|b| b.closed);
    'outer: loop {
        for i in 0..open.len() {
            for j in 0..open.len() {
                if i == j {
                    continue;
                }
                let (ei, sj, ej) = (
                    open[i].pts[open[i].pts.len() - 1],
                    open[j].pts[0],
                    open[j].pts[open[j].pts.len() - 1],
                );
                let forward = ei.distance(sj) < tol;
                let backward = !forward && ei.distance(ej) < tol;
                if forward || backward {
                    let mut other = open.remove(j);
                    let i = if j < i { i - 1 } else { i };
                    if backward {
                        other.pts.reverse();
                        other.hints.reverse();
                    }
                    open[i].pts.extend(other.pts.into_iter().skip(1));
                    open[i].hints.extend(other.hints.into_iter().skip(1));
                    continue 'outer;
                }
            }
        }
        break;
    }
    for mut b in open {
        let n = b.pts.len();
        if n > 2 && b.pts[0].distance(b.pts[n - 1]) < tol {
            b.pts[n - 1] = b.pts[0];
            b.closed = true;
        }
        done.push(b);
    }
    done
}

/// Abstand von `p` zum Stück `a → b` und der Anteil des nächsten Punkts.
fn point_segment(p: DVec3, a: DVec3, b: DVec3) -> (f64, f64) {
    let d = b - a;
    let l2 = d.length_squared();
    let f = if l2 > 0.0 {
        ((p - a).dot(d) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((a + d * f).distance(p), f)
}

/// Kleinster Abstand zweier Strecken.
fn segment_distance(p1: DVec3, q1: DVec3, p2: DVec3, q2: DVec3) -> f64 {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    if a <= 0.0 && e <= 0.0 {
        return r.length();
    }
    let (s, t) = if a <= 0.0 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= 0.0 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s = if denom > 0.0 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    (p1 + d1 * s).distance(p2 + d2 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_distance_handles_crossing_parallel_and_degenerate_segments() {
        let d = segment_distance(
            DVec3::new(-1.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(0.0, -1.0, 0.5),
            DVec3::new(0.0, 1.0, 0.5),
        );
        assert!((d - 0.5).abs() < 1e-15);
        let d = segment_distance(
            DVec3::ZERO,
            DVec3::X,
            DVec3::new(2.0, 1.0, 0.0),
            DVec3::new(3.0, 1.0, 0.0),
        );
        assert!((d - 2f64.sqrt()).abs() < 1e-15);
        let d = segment_distance(DVec3::ZERO, DVec3::ZERO, DVec3::Y, DVec3::Y);
        assert!((d - 1.0).abs() < 1e-15);
    }

    #[test]
    fn branches_with_common_ends_are_joined_and_closed() {
        let br = |pts: Vec<DVec3>| Branch {
            hints: vec![Hints::default(); pts.len()],
            pts,
            closed: false,
        };
        let a = br(vec![DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0)]);
        // umgekehrt angehängt: Ende an Ende
        let b = br(vec![DVec3::ZERO, DVec3::Y, DVec3::new(1.0, 1.0, 0.0)]);
        let out = join(vec![a, b], 1e-6);
        assert_eq!(out.len(), 1);
        assert!(out[0].closed);
        assert_eq!(out[0].pts.len(), 5);
        assert_eq!(out[0].pts[0], out[0].pts[4]);
    }
}
