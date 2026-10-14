//! 2D-Schnitte der Striche und Teilpunkte.
//!
//! Ein Strich wird in der Ansicht als ebene Kurve `p(t) = (P(t)·ex, P(t)·ey)` betrachtet.
//! [`stroke_cross`] findet die Kreuzungen zweier Striche: geschlossen für Gerade–Gerade und
//! Gerade–Kreis/Ellipse, sonst durch Zerlegen beider Parameterbereiche und Newton.
//! [`split_params`] sammelt daraus je Strich die inneren Teilparameter: echte (transversale)
//! Kreuzungen und die Stellen, an denen Endpunkte anderer Striche auf dem Strich liegen.
//!
//! Periodische Parameter (Kreis, Ellipse, geschlossene Freiformkurve): Wurzeln werden in den
//! Bereich des Strichs verschoben; zwei Wurzeln, deren Parameter sich um eine Periode
//! unterscheiden, sind derselbe Punkt und werden einmal gemeldet.
//!
//! Gerechnet wird relativ zu einem Bezugspunkt nahe der Geometrie (siehe `Ev`), damit die
//! Rundung von der Ausdehnung `E` abhängt und nicht vom Abstand zum Ursprung – so bleibt
//! `geom_tol(E)` auch für weit verschobene Körper erreichbar.

use crate::basis::Basis;
use crate::error::DrawError;
use crate::limits::{BISECT_TOL, MAX_PAIR_TESTS, MAX_STROKES, MAX_SUBDIVISION_DEPTH, geom_tol};
use crate::stroke::Stroke;
use planora_exchange::Progress;
use planora_kernel::Curve;
use planora_math::{DVec2, DVec3};
use std::cell::Cell;

/// Vielfaches von `geom_tol`, bis zu dem zwei Punkte als derselbe gelten (Duplikate, Endpunkt
/// auf Strich, Annahme einer Newton-Wurzel).
const NEAR_FACTOR: f64 = 16.0;
/// Zerlegung endet, wenn beide Abschnitte schmaler als dieser Anteil der Strichbreite sind.
const LEAF_REL: f64 = 1e-3;
/// Höchstzahl der Newton-Schritte je Startpaar.
const NEWTON_STEPS: usize = 40;
/// Halbierungen der Schrittweite, wenn ein Newton-Schritt den Abstand vergrößert.
const NEWTON_BACKTRACK: usize = 8;
/// Schwelle für `transversal`: `|da × db| > TRANSVERSAL_REL · |da| · |db|`.
const TRANSVERSAL_REL: f64 = 1e-6;
/// Unterhalb dieses Sinus gelten zwei Geraden als parallel (keine Wurzel).
const PARALLEL_REL: f64 = 1e-12;
/// Unterhalb dieses Sinus löst Newton über die gedämpften Normalgleichungen statt direkt.
const NEWTON_DIRECT_REL: f64 = 1e-8;
/// Newton bricht bei `|F| < geom_tol` nur ab, wenn `|da × db| > WELL_POSED_REL·|da|·|db|`. Nahe
/// einer Berührung erfüllt ein ganzes Stück der Breite `√(tol/κ)` schon `|F| < tol`, und der
/// Winkel dort (bis `√(2κ·tol)`, mit `tol ≥ 1e-9` also bis ~1e-5) läge über der Schwelle für
/// `transversal`; dort wird bis zur Rechengenauigkeit weiter iteriert (bis kein Schritt mehr
/// verbessert).
const WELL_POSED_REL: f64 = 1e-3;
/// Dämpfung der Normalgleichungen relativ zur Spur von `JᵀJ`.
const LM_DAMPING: f64 = 1e-12;
/// Probestellen für die Hüllbox eines Abschnitts.
const CELL_SAMPLES: usize = 9;
/// Abschnitte für die Suche des Fußpunkts eines Endpunkts.
const FOOT_SAMPLES: usize = 128;
/// Schritte des Goldenen Schnitts bei der Fußpunktsuche.
const GOLDEN_STEPS: usize = 100;
/// Abbruchprüfung je so vielen Paartests.
const CHECK_EVERY: u64 = 1024;

/// Eine Kreuzung zweier Striche: Parameter auf a und b, Punkt, und `transversal` (false = Berührung).
#[derive(Clone, Copy, Debug)]
pub struct Cross {
    pub ta: f64,
    pub tb: f64,
    pub p: DVec2,
    pub transversal: bool,
}

/// Gemeinsamer Rechenzustand: Basis, Toleranz, Kostenzähler und (optional) Fortschritt.
struct Ctx<'a> {
    basis: &'a Basis,
    /// Projektion des Bezugspunkts (lokale 2D-Punkte + `shift` = Blattpunkte).
    shift: DVec2,
    tol: f64,
    pairs: &'a Cell<u64>,
    progress: Option<&'a Progress>,
}

impl Ctx<'_> {
    /// Abstand, bis zu dem zwei Punkte als derselbe gelten.
    fn near(&self) -> f64 {
        NEAR_FACTOR * self.tol
    }

    /// Zählt einen Paartest; Überlauf → `TooComplex`, gelegentlich Abbruchprüfung.
    fn count(&self) -> Result<(), DrawError> {
        let n = self.pairs.get() + 1;
        self.pairs.set(n);
        if n > MAX_PAIR_TESTS {
            return Err(DrawError::TooComplex(n));
        }
        if n.is_multiple_of(CHECK_EVERY)
            && let Some(p) = self.progress
        {
            p.check()?;
        }
        Ok(())
    }
}

/// Projizierter Strich, ausgewertet relativ zum Bezugspunkt `r` (3D): Gerade und
/// Kegelschnitt in geschlossener Form mit `origin − r`, Freiformkurven als um `−r` verschobene
/// Kopie. Ableitungen sind dieselben wie die des Strichs.
struct Ev<'a> {
    s: &'a Stroke,
    kind: Kind,
}

enum Kind {
    Line { o: DVec2, d: DVec2 },
    Conic { c: DVec2, u: DVec2, v: DVec2 },
    Free { curve: Curve, ex: DVec3, ey: DVec3 },
}

fn proj(basis: &Basis, v: DVec3) -> DVec2 {
    DVec2::new(v.dot(basis.ex), v.dot(basis.ey))
}

impl<'a> Ev<'a> {
    fn new(s: &'a Stroke, basis: &Basis, r: DVec3) -> Result<Ev<'a>, DrawError> {
        let kind = match &s.curve {
            Curve::Line { origin, dir } => Kind::Line {
                o: proj(basis, *origin - r),
                d: proj(basis, *dir),
            },
            Curve::Circle { frame, radius } => Kind::Conic {
                c: proj(basis, frame.origin - r),
                u: proj(basis, frame.x * *radius),
                v: proj(basis, frame.y * *radius),
            },
            Curve::Ellipse {
                frame,
                major,
                minor,
            } => Kind::Conic {
                c: proj(basis, frame.origin - r),
                u: proj(basis, frame.x * *major),
                v: proj(basis, frame.y * *minor),
            },
            Curve::Nurbs(c) => Kind::Free {
                curve: Curve::nurbs(
                    c.transformed(|p| p - r)
                        .map_err(|e| DrawError::InvalidBody(e.to_string()))?,
                ),
                ex: basis.ex,
                ey: basis.ey,
            },
        };
        Ok(Ev { s, kind })
    }

    /// Lokaler 2D-Punkt bei `t`.
    fn p(&self, t: f64) -> DVec2 {
        match &self.kind {
            Kind::Line { o, d } => *o + *d * t,
            Kind::Conic { c, u, v } => *c + *u * t.cos() + *v * t.sin(),
            Kind::Free { curve, ex, ey } => {
                let q = curve.point(t);
                DVec2::new(q.dot(*ex), q.dot(*ey))
            }
        }
    }

    /// Ableitung des 2D-Punkts nach `t`.
    fn d(&self, t: f64) -> DVec2 {
        match &self.kind {
            Kind::Line { d, .. } => *d,
            Kind::Conic { u, v, .. } => *v * t.cos() - *u * t.sin(),
            Kind::Free { curve, ex, ey } => {
                let q = curve.tangent(t);
                DVec2::new(q.dot(*ex), q.dot(*ey))
            }
        }
    }
}

/// `x > limit`; `false`, wenn eine Seite keine Zahl ist (NaN gilt nie als groß genug).
fn exceeds(x: f64, limit: f64) -> bool {
    x > limit
}

/// Parameterbereich aufsteigend.
fn range(s: &Stroke) -> (f64, f64) {
    (s.t0.min(s.t1), s.t0.max(s.t1))
}

/// Zulässige Überschreitung des Bereichs (`BISECT_TOL` relativ zur Breite).
fn slack(s: &Stroke) -> f64 {
    BISECT_TOL * s.param_len()
}

/// Abstand zweier Parameter; bei periodischen Kurven modulo Periode.
fn param_dist(s: &Stroke, a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    match s.curve.period() {
        Some(p) if p > 0.0 && p.is_finite() => {
            let r = d.rem_euclid(p);
            r.min(p - r)
        }
        _ => d,
    }
}

/// Läuft der Strich über eine ganze Periode (Vollkreis, Vollellipse, ganze geschlossene
/// Freiformkurve)? Dann ist sein Bereichsrand nur die Naht, kein Endpunkt.
fn is_closed(s: &Stroke) -> bool {
    s.curve
        .period()
        .is_some_and(|p| (s.param_len() - p).abs() <= BISECT_TOL * p)
}

/// Endpunkt-Parameter eines Strichs für die Endpunkt-Teilung (keine bei geschlossenen).
fn ends(s: &Stroke) -> Vec<f64> {
    if is_closed(s) {
        vec![]
    } else {
        vec![s.t0, s.t1]
    }
}

fn finite(s: &Stroke) -> bool {
    s.t0.is_finite() && s.t1.is_finite()
}

/// Kreuzungen zweier Striche in der Ansicht. Wurzeln gelten im Bereich ± `BISECT_TOL`·Breite und
/// werden auf den Bereich begrenzt; jede Kreuzung wird genau einmal gemeldet (auch an der Naht
/// periodischer Parameter). Kollineare oder deckungsgleiche Stücke haben keine transversalen
/// Wurzeln; ihre Überdeckung regeln die Endpunkte (siehe [`split_params`]).
pub fn stroke_cross(
    a: &Stroke,
    b: &Stroke,
    basis: &Basis,
    extent: f64,
) -> Result<Vec<Cross>, DrawError> {
    if !(finite(a) && finite(b)) {
        return Err(DrawError::NonFinite);
    }
    let r = a.point(a.t0);
    let pairs = Cell::new(0);
    let ctx = Ctx {
        basis,
        shift: basis.project(r),
        tol: geom_tol(extent),
        pairs: &pairs,
        progress: None,
    };
    cross_in(&ctx, &Ev::new(a, basis, r)?, &Ev::new(b, basis, r)?)
}

fn cross_in(ctx: &Ctx, a: &Ev, b: &Ev) -> Result<Vec<Cross>, DrawError> {
    let roots = match (&a.kind, &b.kind) {
        (Kind::Line { .. }, Kind::Line { .. }) => line_line(a, b),
        (Kind::Line { .. }, Kind::Conic { .. }) => line_conic(ctx, a, b),
        (Kind::Conic { .. }, Kind::Line { .. }) => line_conic(ctx, b, a)
            .into_iter()
            .map(|(t, s)| (s, t))
            .collect(),
        _ => generic(ctx, a, b)?,
    };
    Ok(finish(ctx, a, b, roots))
}

/// Gerade–Gerade geschlossen. Parallel (auch kollinear) und entartet: keine Wurzel.
fn line_line(a: &Ev, b: &Ev) -> Vec<(f64, f64)> {
    let (oa, da) = (a.p(0.0), a.d(0.0));
    let (ob, db) = (b.p(0.0), b.d(0.0));
    let cr = da.perp_dot(db);
    if !exceeds(cr.abs(), PARALLEL_REL * da.length() * db.length()) {
        return vec![];
    }
    let w = ob - oa;
    let s = w.perp_dot(db) / cr;
    let t = w.perp_dot(da) / cr;
    match (in_range(a.s, s), in_range(b.s, t)) {
        (Some(s), Some(t)) => vec![(s, t)],
        _ => vec![],
    }
}

/// `t` im Bereich ± Spielraum? Dann auf den Bereich begrenzt.
fn in_range(s: &Stroke, t: f64) -> Option<f64> {
    let (lo, hi) = range(s);
    let e = slack(s);
    (t >= lo - e && t <= hi + e).then(|| t.clamp(lo, hi))
}

/// Alle Darstellungen `θ + k·2π` im Bereich des Strichs (± Spielraum), auf ihn begrenzt.
fn periodic_in_range(s: &Stroke, theta: f64) -> Vec<f64> {
    let (lo, hi) = range(s);
    let e = slack(s);
    let p = std::f64::consts::TAU;
    let k0 = ((lo - e - theta) / p).ceil() as i64;
    let k1 = ((hi + e - theta) / p).floor() as i64;
    (k0..=k1)
        .map(|k| (theta + k as f64 * p).clamp(lo, hi))
        .collect()
}

/// Gerade `l` gegen Kreis/Ellipse `c`: `n·(c + u cos θ + v sin θ − o) = 0` mit der
/// Einheitsnormale `n` der Geraden, also `A cos θ + B sin θ + K = 0`, gelöst über
/// `θ = atan2(B, A) ± acos(−K / R)`, `R = hypot(A, B)`. Liegt `|K|` höchstens `geom_tol` über `R`,
/// ist es eine Berührung (doppelte Wurzel). `R ≈ 0`: der Kegelschnitt ist von der Seite gesehen
/// eine zur Geraden parallele Strecke – keine Wurzel.
fn line_conic(ctx: &Ctx, l: &Ev, c: &Ev) -> Vec<(f64, f64)> {
    let Kind::Conic { c: cc, u, v } = c.kind else {
        return vec![];
    };
    let (o, d) = (l.p(0.0), l.d(0.0));
    let dl = d.length();
    if !exceeds(dl * l.s.param_len(), ctx.tol) {
        return vec![];
    }
    let n = d.perp() / dl;
    let (ka, kb, kk) = (n.dot(u), n.dot(v), n.dot(cc - o));
    let r = ka.hypot(kb);
    if !exceeds(r, ctx.tol) || kk.abs() > r + ctx.tol {
        return vec![];
    }
    let phi = kb.atan2(ka);
    let delta = (-kk / r).clamp(-1.0, 1.0).acos();
    let mut out = Vec::new();
    for theta in [phi + delta, phi - delta] {
        for t in periodic_in_range(c.s, theta) {
            let q = c.p(t);
            if let Some(s) = in_range(l.s, (q - o).dot(d) / (dl * dl)) {
                out.push((s, t));
            }
        }
    }
    out
}

/// Abschnitt eines Strichs mit seiner (gepolsterten) 2D-Hüllbox.
#[derive(Clone, Copy)]
struct Piece {
    lo: f64,
    hi: f64,
    min: DVec2,
    max: DVec2,
}

impl Piece {
    fn overlaps(&self, o: &Piece) -> bool {
        self.min.x <= o.max.x
            && o.min.x <= self.max.x
            && self.min.y <= o.max.y
            && o.min.y <= self.max.y
    }
}

/// Hüllbox des Abschnitts `lo…hi` aus `CELL_SAMPLES` Probestellen. Gepolstert um die größte
/// zweite Differenz der Probe (≈ achtfache Ausbauchung zwischen zwei Probestellen) und den
/// Gleichheitsabstand.
fn piece(ctx: &Ctx, s: &Ev, lo: f64, hi: f64) -> Piece {
    let n = CELL_SAMPLES - 1;
    let pts: Vec<DVec2> = (0..=n)
        .map(|i| s.p(lo + (hi - lo) * i as f64 / n as f64))
        .collect();
    let mut min = DVec2::splat(f64::INFINITY);
    let mut max = DVec2::splat(f64::NEG_INFINITY);
    for p in &pts {
        min = min.min(*p);
        max = max.max(*p);
    }
    let bulge = pts
        .windows(3)
        .map(|w| (w[0] - 2.0 * w[1] + w[2]).length())
        .fold(0.0, f64::max);
    let pad = DVec2::splat(bulge + ctx.near());
    Piece {
        lo,
        hi,
        min: min - pad,
        max: max + pad,
    }
}

fn halves(ctx: &Ctx, s: &Ev, p: &Piece) -> [Piece; 2] {
    let m = 0.5 * (p.lo + p.hi);
    [piece(ctx, s, p.lo, m), piece(ctx, s, m, p.hi)]
}

/// Alle übrigen Paare: Zerlegung beider Bereiche, Newton je überlebendem Abschnittspaar.
fn generic(ctx: &Ctx, a: &Ev, b: &Ev) -> Result<Vec<(f64, f64)>, DrawError> {
    let (alo, ahi) = range(a.s);
    let (blo, bhi) = range(b.s);
    // Ganzer Strich: die konservative Hüllbox des Strichs (exakte Kegelschnitt-Extrema,
    // Kontrollpunkte bei Freiformkurven) zusammen mit der Probe.
    let top = |s: &Ev, lo: f64, hi: f64| {
        let p = piece(ctx, s, lo, hi);
        let (bmin, bmax) = s.s.bbox2(ctx.basis, ctx.near());
        Piece {
            lo,
            hi,
            min: p.min.min(bmin - ctx.shift),
            max: p.max.max(bmax - ctx.shift),
        }
    };
    let pa = top(a, alo, ahi);
    let pb = top(b, blo, bhi);
    let leaf = (LEAF_REL * (ahi - alo), LEAF_REL * (bhi - blo));
    let mut leaves = Vec::new();
    subdivide(ctx, a, pa, b, pb, leaf, 0, &mut leaves)?;
    let mut roots = Vec::new();
    for (s, t) in leaves {
        if let Some(r) = newton(ctx, a, b, s, t) {
            roots.push(r);
        }
    }
    Ok(roots)
}

#[allow(clippy::too_many_arguments)]
fn subdivide(
    ctx: &Ctx,
    a: &Ev,
    pa: Piece,
    b: &Ev,
    pb: Piece,
    leaf: (f64, f64),
    depth: u32,
    out: &mut Vec<(f64, f64)>,
) -> Result<(), DrawError> {
    ctx.count()?;
    if !pa.overlaps(&pb) {
        return Ok(());
    }
    let a_small = pa.hi - pa.lo < leaf.0;
    let b_small = pb.hi - pb.lo < leaf.1;
    if (a_small && b_small) || depth >= MAX_SUBDIVISION_DEPTH {
        out.push((0.5 * (pa.lo + pa.hi), 0.5 * (pb.lo + pb.hi)));
        return Ok(());
    }
    let sa: Vec<Piece> = if a_small {
        vec![pa]
    } else {
        halves(ctx, a, &pa).to_vec()
    };
    let sb: Vec<Piece> = if b_small {
        vec![pb]
    } else {
        halves(ctx, b, &pb).to_vec()
    };
    for x in &sa {
        for y in &sb {
            subdivide(ctx, a, *x, b, *y, leaf, depth + 1, out)?;
        }
    }
    Ok(())
}

/// Newton auf `F(s, t) = pa(s) − pb(t)` mit `J = [pa'(s), −pb'(t)]`; bei fast parallelen
/// Ableitungen (Berührung) über gedämpfte Normalgleichungen. Iterierte bleiben im Bereich;
/// höchstens `NEWTON_STEPS` Schritte, Abbruch bei `|F| < geom_tol` (gut gestellt) bzw. wenn kein
/// Schritt `|F|` mehr verkleinert (nahe Berührung). Angenommen, wenn `|F| ≤ 16·geom_tol`.
fn newton(ctx: &Ctx, a: &Ev, b: &Ev, s0: f64, t0: f64) -> Option<(f64, f64)> {
    let (alo, ahi) = range(a.s);
    let (blo, bhi) = range(b.s);
    let f_at = |s: f64, t: f64| a.p(s) - b.p(t);
    let (mut s, mut t) = (s0, t0);
    let mut f = f_at(s, t);
    for _ in 0..NEWTON_STEPS {
        let fl = f.length();
        let da = a.d(s);
        let db = b.d(t);
        let cr = da.perp_dot(db);
        let well_posed = cr.abs() > WELL_POSED_REL * da.length() * db.length();
        if !fl.is_finite() || fl == 0.0 || (fl < ctx.tol && well_posed) {
            break;
        }
        let (ds, dt) = if cr.abs() > NEWTON_DIRECT_REL * da.length() * db.length() {
            // ds·da − dt·db = −F
            (-f.perp_dot(db) / cr, da.perp_dot(f) / cr)
        } else {
            let (j11, j12, j22) = (da.dot(da), -da.dot(db), db.dot(db));
            let (g1, g2) = (da.dot(f), -db.dot(f));
            let lam = LM_DAMPING * (j11 + j22);
            let det = (j11 + lam) * (j22 + lam) - j12 * j12;
            if !exceeds(det, 0.0) {
                break;
            }
            (
                -((j22 + lam) * g1 - j12 * g2) / det,
                -((j11 + lam) * g2 - j12 * g1) / det,
            )
        };
        if !(ds.is_finite() && dt.is_finite()) {
            break;
        }
        let mut step = 1.0;
        let mut best = None;
        for _ in 0..=NEWTON_BACKTRACK {
            let ns = (s + step * ds).clamp(alo, ahi);
            let nt = (t + step * dt).clamp(blo, bhi);
            let nf = f_at(ns, nt);
            if nf.length() < fl {
                best = Some((ns, nt, nf));
                break;
            }
            step *= 0.5;
        }
        match best {
            Some((ns, nt, nf)) => {
                s = ns;
                t = nt;
                f = nf;
            }
            None => break,
        }
    }
    (f.length() <= ctx.near()).then_some((s, t))
}

/// Wurzeln zu Kreuzungen: Punkt, `transversal`, Vereinigung der Duplikate. Duplikate sind
/// Wurzeln mit 2D-Abstand `< 16·geom_tol`, deren Parameter auf beiden Strichen (modulo Periode)
/// innerhalb der Zerlegungsauflösung liegen – so bleiben zwei Kreuzungen in einem
/// Selbstschnittpunkt der Projektion getrennt. Berührungswurzeln (nicht transversal) derselben
/// Stelle werden auch ohne 2D-Nähe vereinigt (Newton trifft sie nur auf `√tol` genau).
fn finish(ctx: &Ctx, a: &Ev, b: &Ev, roots: Vec<(f64, f64)>) -> Vec<Cross> {
    let (wa, wb) = (a.s.param_len(), b.s.param_len());
    let mut out: Vec<(Cross, f64)> = Vec::new();
    for (s, t) in roots {
        let (qa, qb) = (a.p(s), b.p(t));
        let err = qa.distance(qb);
        let (da, db) = (a.d(s), b.d(t));
        let transversal = da.perp_dot(db).abs() > TRANSVERSAL_REL * da.length() * db.length();
        let c = Cross {
            ta: s,
            tb: t,
            p: 0.5 * (qa + qb),
            transversal,
        };
        let same = |e: &Cross| {
            let params = param_dist(a.s, e.ta, s) <= LEAF_REL * wa
                && param_dist(b.s, e.tb, t) <= LEAF_REL * wb;
            params && (e.p.distance(c.p) < ctx.near() || !(e.transversal || transversal))
        };
        match out.iter_mut().find(|(e, _)| same(e)) {
            Some(slot) => {
                if err < slot.1 {
                    *slot = (c, err);
                }
            }
            None => out.push((c, err)),
        }
    }
    let mut v: Vec<Cross> = out
        .into_iter()
        .map(|(c, _)| Cross {
            p: c.p + ctx.shift,
            ..c
        })
        .collect();
    v.sort_by(|x, y| x.ta.total_cmp(&y.ta).then(x.tb.total_cmp(&y.tb)));
    v
}

/// Parameter auf `s`, an denen der Punkt `q` (2D) höchstens `near` vom Strich entfernt liegt
/// (Fußpunkte). Gerade geschlossen; sonst Probe und Goldener Schnitt um jedes örtliche Minimum.
fn foot_params(ctx: &Ctx, ev: &Ev, q: DVec2) -> Vec<f64> {
    let near = ctx.near();
    let s = ev.s;
    let (lo, hi) = range(s);
    if let Kind::Line { o, d } = ev.kind {
        let dd = d.dot(d);
        if !exceeds(dd.sqrt() * s.param_len(), ctx.tol) {
            return vec![];
        }
        return in_range(s, (q - o).dot(d) / dd)
            .filter(|&t| ev.p(t).distance(q) <= near)
            .into_iter()
            .collect();
    }
    let n = FOOT_SAMPLES;
    let ts: Vec<f64> = (0..=n)
        .map(|i| lo + (hi - lo) * i as f64 / n as f64)
        .collect();
    let pts: Vec<DVec2> = ts.iter().map(|&t| ev.p(t)).collect();
    let dist: Vec<f64> = pts.iter().map(|p| p.distance(q)).collect();
    let mut out: Vec<f64> = Vec::new();
    for i in 0..=n {
        let left = if i > 0 { dist[i - 1] } else { f64::INFINITY };
        let right = if i < n { dist[i + 1] } else { f64::INFINITY };
        if exceeds(dist[i], left.min(right)) || dist[i].is_nan() {
            continue;
        }
        // Zwischen zwei Probestellen weicht die Kurve höchstens um etwa die Sehnenlänge ab.
        let reach = [i.wrapping_sub(1), i]
            .iter()
            .filter(|&&k| k < n)
            .map(|&k| pts[k].distance(pts[k + 1]))
            .fold(0.0, f64::max);
        if dist[i] > 2.0 * reach + near {
            continue;
        }
        let (mut x0, mut x1) = (ts[i.saturating_sub(1)], ts[(i + 1).min(n)]);
        let g = |t: f64| ev.p(t).distance_squared(q);
        let r = 0.5 * (5f64.sqrt() - 1.0);
        let (mut c, mut d) = (x1 - r * (x1 - x0), x0 + r * (x1 - x0));
        let (mut gc, mut gd) = (g(c), g(d));
        for _ in 0..GOLDEN_STEPS {
            if gc <= gd {
                x1 = d;
                d = c;
                gd = gc;
                c = x1 - r * (x1 - x0);
                gc = g(c);
            } else {
                x0 = c;
                c = d;
                gc = gd;
                d = x0 + r * (x1 - x0);
                gd = g(d);
            }
        }
        let t = 0.5 * (x0 + x1);
        // Ränder des Bereichs mitprüfen (Minimum genau am Ende).
        let t = [t, ts[i]]
            .into_iter()
            .min_by(|&x, &y| g(x).total_cmp(&g(y)))
            .unwrap_or(t);
        if g(t).sqrt() <= near && !out.iter().any(|&u| (u - t).abs() <= slack(s)) {
            out.push(t);
        }
    }
    out
}

/// Für jeden Strich die sortierten, eindeutigen inneren Teilparameter (t0 < t < t1, Abstand ≥ BISECT_TOL·Breite voneinander und vom Rand).
///
/// Gesammelt werden (1) alle transversalen Kreuzungen mit anderen Strichen (Vorauswahl über die
/// nach `min.x` sortierten Hüllboxen) und (2) die Parameter, an denen Endpunkte anderer Striche
/// höchstens `16·geom_tol` vom Strich entfernt liegen (Überdeckung, T-Anschlüsse). Der
/// Bereichsrand ist nie ein Teilpunkt; bei geschlossenen Strichen (ganze Periode) ist er nur die
/// Naht und zählt auch nicht als Endpunkt für andere Striche. Berührungen teilen nicht. Mehr als `MAX_STROKES` Striche oder `MAX_PAIR_TESTS` Paartests → `TooComplex`.
pub fn split_params(
    strokes: &[Stroke],
    basis: &Basis,
    extent: f64,
    progress: &Progress,
) -> Result<Vec<Vec<f64>>, DrawError> {
    let n = strokes.len();
    if n as u64 > MAX_STROKES {
        return Err(DrawError::TooComplex(n as u64));
    }
    progress.check()?;
    if strokes.iter().any(|s| !finite(s)) {
        return Err(DrawError::NonFinite);
    }
    // Bezugspunkt: Anfang des ersten Strichs (alle Striche liegen innerhalb der Ausdehnung).
    let r = strokes.first().map_or(DVec3::ZERO, |s| s.point(s.t0));
    let pairs = Cell::new(0);
    let ctx = Ctx {
        basis,
        shift: basis.project(r),
        tol: geom_tol(extent),
        pairs: &pairs,
        progress: Some(progress),
    };
    let mut evs = Vec::with_capacity(n);
    for s in strokes {
        progress.check()?;
        evs.push(Ev::new(s, basis, r)?);
    }
    let mut boxes = Vec::with_capacity(n);
    for s in strokes {
        progress.check()?;
        let (lo, hi) = s.bbox2(basis, ctx.near());
        if !(lo.is_finite() && hi.is_finite()) {
            return Err(DrawError::NonFinite);
        }
        boxes.push((lo, hi));
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| boxes[i].0.x.total_cmp(&boxes[j].0.x));
    let mut cand: Vec<Vec<f64>> = vec![Vec::new(); n];
    for (k, &i) in order.iter().enumerate() {
        progress.check()?;
        for &j in &order[k + 1..] {
            if boxes[j].0.x > boxes[i].1.x {
                break;
            }
            ctx.count()?;
            if boxes[j].0.y > boxes[i].1.y || boxes[i].0.y > boxes[j].1.y {
                continue;
            }
            let (a, b) = (&evs[i], &evs[j]);
            for c in cross_in(&ctx, a, b)? {
                if c.transversal {
                    cand[i].push(c.ta);
                    cand[j].push(c.tb);
                }
            }
            for t in ends(b.s) {
                cand[i].extend(foot_params(&ctx, a, b.p(t)));
            }
            for t in ends(a.s) {
                cand[j].extend(foot_params(&ctx, b, a.p(t)));
            }
        }
    }
    Ok(strokes
        .iter()
        .zip(cand)
        .map(|(s, ts)| interior_unique(s, ts))
        .collect())
}

/// Innere, sortierte, eindeutige Parameter: Abstand ≥ `BISECT_TOL`·Breite vom Rand und
/// voneinander.
pub(crate) fn interior_unique(s: &Stroke, mut ts: Vec<f64>) -> Vec<f64> {
    let (lo, hi) = range(s);
    let e = slack(s);
    ts.retain(|&t| t >= lo + e && t <= hi - e);
    ts.sort_by(f64::total_cmp);
    let mut out: Vec<f64> = Vec::with_capacity(ts.len());
    for t in ts {
        if out.last().is_none_or(|&l| t - l >= e) {
            out.push(t);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ViewKind;
    use planora_math::DVec3;

    fn line(o: DVec3, d: DVec3, i: usize) -> Stroke {
        Stroke {
            curve: Curve::Line {
                origin: o,
                dir: d.normalize(),
            },
            t0: 0.0,
            t1: 20.0,
            origin: crate::stroke::StrokeOrigin::Edge { body: 0, edge: i },
        }
    }

    #[test]
    fn pair_counter_overflow_reports_too_complex() {
        let basis = Basis::of(ViewKind::Front);
        let pairs = Cell::new(MAX_PAIR_TESTS);
        let ctx = Ctx {
            basis: &basis,
            shift: DVec2::ZERO,
            tol: geom_tol(20.0),
            pairs: &pairs,
            progress: None,
        };
        let a = line(DVec3::ZERO, DVec3::new(1.0, 0.0, 1.0), 0);
        let circ = Stroke {
            curve: Curve::Circle {
                frame: planora_math::Frame {
                    origin: DVec3::ZERO,
                    x: DVec3::X,
                    y: DVec3::Z,
                    z: -DVec3::Y,
                },
                radius: 5.0,
            },
            t0: 0.0,
            t1: std::f64::consts::TAU,
            origin: crate::stroke::StrokeOrigin::Edge { body: 0, edge: 1 },
        };
        let ev = |s| Ev::new(s, &basis, DVec3::ZERO).unwrap();
        let err = cross_in(&ctx, &ev(&circ), &ev(&circ)).unwrap_err();
        assert!(matches!(err, DrawError::TooComplex(n) if n == MAX_PAIR_TESTS + 1));
        // Schnellfälle zählen keine Zerlegung.
        assert!(cross_in(&ctx, &ev(&a), &ev(&circ)).is_ok());
    }
}
