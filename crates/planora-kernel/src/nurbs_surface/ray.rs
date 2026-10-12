//! Strahlschnitt einer NURBS-Fläche (Einordnen per Teststrahl, Spec §6 Punkt 7).
//!
//! Verfahren: Zerlegung in rationale Bézier-Flicken (beim ersten Gebrauch, zwischengespeichert),
//! Strahl–Hüllquader-Test (Slab-Test), Unterteilung bis der Flicken eben genug ist oder die
//! Tiefengrenze erreicht ist, dann Newton auf `S(u, v) − origin − t·dir = 0` mit Startwert aus der
//! Flickenmitte. Ein Treffer zählt nur, wenn `(u, v)` im Bereich des Flickens liegt (der Treffer
//! eines Nachbarflickens wird dort gefunden).

use super::NurbsSurfaceData;
use planora_math::DVec3;
use planora_math::nurbs::BezierPatch;

/// Unterteilen, bis `flatness() ≤ RAY_FLATNESS_RATIO · Diagonale` des Flickens gilt.
const RAY_FLATNESS_RATIO: f64 = 1e-3;
/// Höchste Unterteilungstiefe je Bézier-Flicken.
const RAY_MAX_DEPTH: usize = 12;
/// Höchstzahl untersuchter Flicken je Strahl (Arbeitsgrenze; darüber ist der Strahl unsicher).
const RAY_MAX_VISITS: usize = 60_000;
/// Höchstzahl Newton-Schritte je Blatt.
const RAY_NEWTON_STEPS: usize = 32;
/// Ein Newton-Ergebnis zählt für einen Flicken, wenn `(u, v)` höchstens diesen Bruchteil der
/// Bereichsbreite außerhalb seines Parameterbereichs liegt.
const RAY_RANGE_SLACK: f64 = 1e-9;
/// Newton gilt als konvergiert, wenn der Rest höchstens diesen Bruchteil des Koordinatenmaßes
/// beträgt (weit unter der Modelltoleranz 1e-6 mm bei Teilen bis 1 m).
const RAY_RESIDUAL_RATIO: f64 = 1e-12;
/// Größte Teilungstiefe für Blätter ohne Entscheidung (Newton scheitert oder verlässt den
/// Bereich an einem Pol), bevor der Strahl als unsicher gilt.
const RAY_UNRESOLVED_DEPTH: usize = RAY_MAX_DEPTH + 4;
/// Höchstzahl solcher Nachteilungen je Strahl (ein Strahl genau durch einen Pol trifft alle
/// Keile um ihn; ohne Grenze wüchse die Arbeit mit jeder Stufe um das Vierfache).
const RAY_UNRESOLVED_SPLITS: usize = 64;
/// Neustarts vom Randpunkt, wenn ein Newton-Schritt den Bereich verlässt.
const RAY_RIM_RESTARTS: usize = 3;
/// Abstand des Neustarts vom Pol als Bruchteil des halben v-Bereichs.
const RAY_POLE_NUDGE: f64 = 1e-6;
/// Untergrenze des Koordinatenmaßes für die Restschranke (mm).
const RAY_SCALE_FLOOR: f64 = 1.0;
/// Ein Blatt, in dem der Strahl fast in der Tangentialebene liegt (`|dir · n| <` diesem Wert,
/// Normale in der Blattmitte), ist unsicher: Newton entscheidet dort nicht verlässlich.
pub const RAY_GRAZING_COS: f64 = 0.01;
/// Kleinste Determinante (relativ zum Produkt der Spaltenlängen) des Newton-Systems.
const RAY_SINGULAR_RATIO: f64 = 1e-14;
/// Zuschlag auf die Hüllquader beim Slab-Test (relativ zum Koordinatenmaß), gegen Rundung.
const RAY_BOX_PAD_RATIO: f64 = 1e-12;

/// Ergebnis der Newton-Suche in einem Blatt.
enum Newton {
    /// Konvergiert bei `(t, u, v)`.
    Root(f64, f64, f64),
    /// Der Weg verlässt den Parameterbereich der Fläche (kein Treffer in diesem Blatt).
    Left,
    /// Keine Konvergenz (singuläres System oder Schrittgrenze).
    Failed,
}

/// Schichtprobe: Alle Kontrollpunkte des Flickens (und damit der Flicken, Gewichte positiv)
/// liegen längs `n` im Intervall `[lo, hi]`. Liegt das Stück des Strahls im Hüllquader ganz auf
/// einer Seite dieser Schicht, trifft er den Flicken sicher nicht.
fn slab_excludes(
    q: &BezierPatch,
    n: DVec3,
    origin: DVec3,
    dir: DVec3,
    reach: f64,
    pad: f64,
) -> bool {
    let len = n.length();
    if len.is_nan() || len <= 0.0 {
        return false;
    }
    let n = n / len;
    let (blo, bhi) = q.bounds();
    let Some((ta, tb)) = ray_box_interval(origin, dir, reach, blo, bhi, pad) else {
        return true;
    };
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for c in &q.control {
        let x = n.dot(*c);
        lo = lo.min(x);
        hi = hi.max(x);
    }
    let (lo, hi) = (lo - pad, hi + pad);
    let (sa, sb) = (n.dot(origin + dir * ta), n.dot(origin + dir * tb));
    (sa < lo && sb < lo) || (sa > hi && sb > hi)
}

/// Löst `[a b c] · x = r` (Spalten `a`, `b`, `c`) nach Cramer; `None` bei (fast) singulärem System.
fn solve3(a: DVec3, b: DVec3, c: DVec3, r: DVec3) -> Option<[f64; 3]> {
    let det = a.dot(b.cross(c));
    let size = a.length() * b.length() * c.length();
    if !det.is_finite() || det.abs() <= RAY_SINGULAR_RATIO * size {
        return None;
    }
    Some([
        r.dot(b.cross(c)) / det,
        a.dot(r.cross(c)) / det,
        a.dot(b.cross(r)) / det,
    ])
}

/// Schneidet der Strahl `origin + t·dir`, `t ∈ [0, reach]`, den Quader `[lo − pad, hi + pad]`?
fn ray_box(origin: DVec3, dir: DVec3, reach: f64, lo: DVec3, hi: DVec3, pad: f64) -> bool {
    ray_box_interval(origin, dir, reach, lo, hi, pad).is_some()
}

/// Parameterintervall `[t0, t1] ⊂ [0, reach]` des Strahls im Quader `[lo − pad, hi + pad]`.
fn ray_box_interval(
    origin: DVec3,
    dir: DVec3,
    reach: f64,
    lo: DVec3,
    hi: DVec3,
    pad: f64,
) -> Option<(f64, f64)> {
    let (lo, hi) = (lo - DVec3::splat(pad), hi + DVec3::splat(pad));
    let (mut t0, mut t1) = (0.0_f64, reach);
    for k in 0..3 {
        let (o, d) = (origin[k], dir[k]);
        if d == 0.0 {
            if o < lo[k] || o > hi[k] {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo[k] - o) / d, (hi[k] - o) / d);
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, t1))
}

impl NurbsSurfaceData {
    /// Bézier-Flicken der Fläche (beim ersten Gebrauch berechnet; `None`, wenn die Zerlegung die
    /// Grenze `MAX_BEZIER_PATCHES` überschreitet).
    fn bezier(&self) -> Option<&[BezierPatch]> {
        self.bezier
            .get_or_init(|| self.surface.bezier_patches().ok())
            .as_deref()
    }

    /// Schnittpunkte des Strahls `origin + t·dir` (t ≥ 0, t ≤ reach) mit der Fläche: (t, u, v),
    /// nach t sortiert, Duplikate entfernt.
    ///
    /// Verfahren: Bézier-Flicken, Strahl–Hüllquader (Slab-Test), Unterteilung bis
    /// `flatness() ≤ 1e-3·Diagonale` (des Flickens) oder Tiefe 12, Newton auf
    /// `S(u,v) − origin − t·dir = 0` mit Startwert aus dem Flickenmittelpunkt, Prüfung u, v im
    /// Flickenbereich ± 1e-9 (relativ zur Bereichsbreite).
    ///
    /// Unsichere Blätter (Strahl fast tangential, Newton ohne Konvergenz, Arbeitsgrenze) werden
    /// hier übergangen; wer das unterscheiden muss, nimmt [`Self::ray_hits_checked`].
    pub fn ray_hits(&self, origin: DVec3, dir: DVec3, reach: f64) -> Vec<(f64, f64, f64)> {
        self.ray_hits_impl(origin, dir, reach, false)
            .unwrap_or_default()
    }

    /// Wie [`Self::ray_hits`], aber `None`, sobald ein Blatt keine verlässliche Entscheidung
    /// erlaubt: Strahl fast in der Tangentialebene eines Blatts, das er durchquert
    /// (`|dir · n| <` [`RAY_GRAZING_COS`]), Newton ohne Konvergenz, Newton verlässt den
    /// Bereich in einem Blatt an einem Pol, Arbeitsgrenze überschritten,
    /// Zerlegung zu groß oder nicht endliche Eingaben. Der Aufrufer wählt dann einen anderen
    /// Strahl, statt zu raten.
    pub fn ray_hits_checked(
        &self,
        origin: DVec3,
        dir: DVec3,
        reach: f64,
    ) -> Option<Vec<(f64, f64, f64)>> {
        self.ray_hits_impl(origin, dir, reach, true)
    }

    fn ray_hits_impl(
        &self,
        origin: DVec3,
        dir: DVec3,
        reach: f64,
        strict: bool,
    ) -> Option<Vec<(f64, f64, f64)>> {
        if !origin.is_finite() || !dir.is_finite() || !reach.is_finite() || reach < 0.0 {
            return None;
        }
        let dir_len = dir.length();
        if dir_len == 0.0 {
            return None;
        }
        let patches = self.bezier()?;
        let (blo, bhi) = self.control_bounds();
        let scale = blo
            .abs()
            .max(bhi.abs())
            .max(origin.abs())
            .max_element()
            .max((bhi - blo).length())
            .max(RAY_SCALE_FLOOR);
        let pad = RAY_BOX_PAD_RATIO * scale;
        let mut stack: Vec<(BezierPatch, usize)> = patches
            .iter()
            .filter(|q| {
                let (lo, hi) = q.bounds();
                ray_box(origin, dir, reach, lo, hi, pad)
            })
            .map(|q| (q.clone(), 0))
            .collect();
        let mut visits = 0usize;
        let mut unresolved_splits = 0usize;
        let mut hits: Vec<(f64, f64, f64)> = Vec::new();
        while let Some((q, depth)) = stack.pop() {
            visits += 1;
            if visits > RAY_MAX_VISITS {
                return if strict {
                    None
                } else {
                    Some(sorted(hits, dir_len, scale))
                };
            }
            let (lo, hi) = q.bounds();
            let flat = q.flatness() <= RAY_FLATNESS_RATIO * (hi - lo).length();
            if !flat && depth < RAY_MAX_DEPTH {
                for c in q.subdivide() {
                    let (lo, hi) = c.bounds();
                    if ray_box(origin, dir, reach, lo, hi, pad) {
                        stack.push((c, depth + 1));
                    }
                }
                continue;
            }
            // Blatt. Ein Blatt, das der Strahl sicher nicht trifft (Schichtprobe), braucht
            // keine Rechnung; sonst ist ein fast tangentialer Strahl unsicher.
            let (uc, vc) = (0.5 * (q.u.0 + q.u.1), 0.5 * (q.v.0 + q.v.1));
            let n = self.normal(uc, vc);
            if slab_excludes(&q, n, origin, dir, reach, pad) {
                continue;
            }
            if strict && (n == DVec3::ZERO || (dir.dot(n) / dir_len).abs() < RAY_GRAZING_COS) {
                return None;
            }
            let unresolved = match self.ray_newton(origin, dir, uc, vc, scale) {
                Newton::Root(t, u, v) => {
                    if t >= 0.0 && t <= reach && self.in_patch(&q, u, v) {
                        hits.push((t, u, v));
                    }
                    false
                }
                // An einem Pol ist die Parametrisierung entartet (`S_u = 0`): Newton kann über
                // den Rand hinausschießen, obwohl der Treffer im Blatt liegt (Strahl dicht am
                // Pol). Dort ist „verlassen“ kein Nachweis für „kein Treffer“. An gewöhnlichen
                // Rändern setzt Newton vom Randpunkt neu an (`ray_newton`); die Regel für alle
                // Ränder (Runde 1) machte Strahlen an Rändern und Nähten unnötig oft mehrdeutig.
                Newton::Left => self.touches_pole(&q),
                Newton::Failed => true,
            };
            if unresolved && strict {
                // Weiter teilen (bis `RAY_UNRESOLVED_DEPTH`): kleinere Blätter scheiden über die
                // Schichtprobe aus oder lassen Newton konvergieren; sonst ist der Strahl unsicher.
                unresolved_splits += 1;
                if depth >= RAY_UNRESOLVED_DEPTH || unresolved_splits > RAY_UNRESOLVED_SPLITS {
                    return None;
                }
                for c in q.subdivide() {
                    let (lo, hi) = c.bounds();
                    if ray_box(origin, dir, reach, lo, hi, pad) {
                        stack.push((c, depth + 1));
                    }
                }
            }
        }
        Some(sorted(hits, dir_len, scale))
    }

    /// Berührt der Flicken einen Pol (zusammengefallene Kontrollpunktreihe bei `v0` oder `v1`)?
    fn touches_pole(&self, q: &BezierPatch) -> bool {
        // Die Flickengrenzen am Bereichsrand sind exakt die Knotenwerte (Halbieren lässt die
        // äußeren Grenzen unverändert), ebenso die Polwerte.
        self.poles_v.iter().any(|pv| q.v.0 == *pv || q.v.1 == *pv)
    }

    /// Liegt `(u, v)` im Bereich des Flickens (± `RAY_RANGE_SLACK` der Bereichsbreite)? In
    /// geschlossenen Richtungen auch um eine Periode versetzt.
    fn in_patch(&self, q: &BezierPatch, u: f64, v: f64) -> bool {
        let ((u0, u1), (v0, v1)) = self.surface.domain();
        let inside = |x: f64, (a, b): (f64, f64), w: f64, periodic: bool| {
            let s = RAY_RANGE_SLACK * w;
            let shifts: &[f64] = if periodic { &[0.0, -1.0, 1.0] } else { &[0.0] };
            shifts.iter().any(|k| {
                let y = x + k * w;
                y >= a - s && y <= b + s
            })
        };
        inside(u, q.u, u1 - u0, self.periodic_u) && inside(v, q.v, v1 - v0, self.periodic_v)
    }

    /// Newton auf `S(u, v) − origin − t·dir = 0` von `(u, v)` aus (t aus der Projektion des
    /// Flächenpunkts auf den Strahl).
    fn ray_newton(&self, origin: DVec3, dir: DVec3, u: f64, v: f64, scale: f64) -> Newton {
        let ((u0, u1), (v0, v1)) = self.surface.domain();
        let (mut u, mut v) = (u, v);
        let p = self.surface.point(u, v);
        let mut t = (p - origin).dot(dir) / dir.length_squared();
        let limit = RAY_RESIDUAL_RATIO * scale;
        let outside = |x: f64, a: f64, b: f64, periodic: bool| !periodic && (x < a || x > b);
        let mut restarts = 0;
        for _ in 0..RAY_NEWTON_STEPS {
            let d = self.surface.derivs(u, v, 1);
            let r = d[0][0] - origin - dir * t;
            if !r.is_finite() {
                return Newton::Failed;
            }
            let Some([du, dv, dt]) = solve3(d[1][0], d[0][1], -dir, -r) else {
                return Newton::Failed;
            };
            let (nu, nv) = (u + du, v + dv);
            if outside(nu, u0, u1, self.periodic_u) || outside(nv, v0, v1, self.periodic_v) {
                // Ein Schritt knapp über den Rand kann am Rand selbst enden: dort prüfen.
                let (cu, cv) = (nu.clamp(u0, u1), nv.clamp(v0, v1));
                let s = self.surface.point(cu, cv);
                let tt = (s - origin).dot(dir) / dir.length_squared();
                if (s - origin - dir * tt).length() <= limit {
                    return Newton::Root(tt, cu, cv);
                }
                // Projiziertes Newton: vom Randpunkt aus weiter. Liegt der Treffer dicht am
                // Rand, schießt der erste Schritt aus der Blattmitte (lineares Modell eines
                // gekrümmten Blatts) leicht über den Rand hinaus; vom Rand aus konvergiert
                // Newton dann zum Treffer (Review Runde 2: Punkte 1e-5 neben der Kante einer
                // Spline-Wand verloren so ihren Treffer). Erst wenn auch die Neustarts den
                // Bereich verlassen, gilt das Blatt als ohne Treffer.
                if restarts == RAY_RIM_RESTARTS {
                    return Newton::Left;
                }
                restarts += 1;
                // An einem Pol ist das System singulär (`S_u = 0`): ein Stück neben dem Pol
                // neu ansetzen.
                let cv = match self.poles_v.iter().find(|pv| **pv == cv) {
                    Some(pv) => pv + (0.5 * (v0 + v1) - pv) * RAY_POLE_NUDGE,
                    None => cv,
                };
                (u, v, t) = (cu, cv, tt);
                continue;
            }
            (u, v) = self.wrap(nu, nv);
            t += dt;
            if r.length() <= limit {
                // Rest schon klein: der letzte Schritt war nur noch Feinarbeit.
                let s = self.surface.point(u, v);
                if (s - origin - dir * t).length() <= limit {
                    return Newton::Root(t, u, v);
                }
            }
        }
        let s = self.surface.point(u, v);
        if (s - origin - dir * t).length() <= limit {
            Newton::Root(t, u, v)
        } else {
            Newton::Failed
        }
    }
}

/// Nach `t` sortiert; Treffer, deren Punkte auf dem Strahl höchstens `RAY_RESIDUAL_RATIO ·
/// 1000 · Maß` auseinander liegen (derselbe Treffer aus zwei Nachbarflicken), nur einmal.
fn sorted(mut hits: Vec<(f64, f64, f64)>, dir_len: f64, scale: f64) -> Vec<(f64, f64, f64)> {
    /// Abstand gleicher Treffer, als Vielfaches der Newton-Restschranke.
    const DEDUP_FACTOR: f64 = 1000.0;
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    let gap = DEDUP_FACTOR * RAY_RESIDUAL_RATIO * scale / dir_len;
    let mut out: Vec<(f64, f64, f64)> = Vec::with_capacity(hits.len());
    for h in hits {
        if out.last().is_none_or(|l| h.0 - l.0 > gap) {
            out.push(h);
        }
    }
    out
}
