//! Sichtbarkeit der Linienstücke (Festlegung F9): An jeder Stützstelle stimmen 8 leicht gekippte
//! Prüfstrahlen zum Auge ab. Frei unter irgendeinem Kippstrahl heißt sichtbar (eine Linie genau
//! auf dem Umriss einer Fläche oder in der Ebene einer von der Seite gesehenen Fläche ist nach
//! Zeichenkonvention sichtbar); verdeckt nur, wenn alle brauchbaren Strahlen verdeckt sind.
//! Randtreffer und unsichere Strahlen zählen nicht; ohne brauchbaren Strahl weicht die Stelle
//! aus, danach `AmbiguousVisibility`. Urteilen die Stellen eines Stücks verschieden, wird es
//! halbiert, bis der Wechsel auf `BISECT_TOL` (relativ) genau eingegrenzt ist, oder – wo die
//! Strahlen am Wechsel selbst nur den verdeckenden Rand streifen – auf die Breite dieses
//! Randbands (höchstens `OWN_EPS`), siehe [`classify_range`]. Zum Schluss fasst
//! [`merge_pieces`] die Stücke zusammen (deckungsgleiche einmal, anschließende vereint, winzige
//! entfernt).

use crate::basis::Basis;
use crate::error::DrawError;
use crate::isect::{Images, tip_params};
use crate::limits::{
    BISECT_TOL, MAX_PAIR_TESTS, MAX_PIECES, MAX_RAY_TESTS, MAX_SUBDIVISION_DEPTH, SILHOUETTE_TOL,
    geom_tol, own_eps, self_eps,
};
use crate::raycast::{Caster, HitClass, Ray};
use crate::stroke::{Stroke, StrokeOrigin};
use crate::{Piece, Visibility};
use planora_exchange::Progress;
use planora_kernel::Curve;
use planora_math::{DVec2, DVec3};
use std::cell::Cell;

/// Kippwinkel der Prüfstrahlen (rad).
pub const TILT: f64 = 1e-5;
/// Zahl der Kipprichtungen, gleichverteilt um die Blickrichtung.
pub const TILT_DIRECTIONS: usize = 8;
/// Stützstellen je Stück (Bruchteil der Stückbreite).
pub const SAMPLE_FRACTIONS: [f64; 3] = [0.25, 0.5, 0.75];
/// Ausweichstellen (Bruchteil der Stückbreite), nacheinander versucht.
pub const SHIFTS: [f64; 6] = [0.0123, -0.0123, 0.037, -0.037, 0.071, -0.071];

/// Stellen knapp innerhalb der Stückenden (Bruchteil der Stückbreite), deren Urteil als
/// bekanntes Ende in die Beurteilung eingeht.
pub const END_PROBE: f64 = 0.01;

/// Urteil eines einzelnen Kippstrahls.
enum RayVote {
    Free,
    Blocked,
    Unusable,
}

/// Zählt einen Strahlwurf; über `MAX_RAY_TESTS` → `TooComplex`.
fn count(counter: &Cell<u64>) -> Result<(), DrawError> {
    let n = counter.get() + 1;
    counter.set(n);
    if n > MAX_RAY_TESTS {
        return Err(DrawError::TooComplex(n));
    }
    Ok(())
}

/// Ein Strahl: Treffer auf eigenen Flächen mit `s ≤ OWN_EPS` entfallen; bleibt ein Randtreffer,
/// ist der Strahl unbrauchbar, sonst verdeckt ihn ein Innentreffer.
fn vote(ray: Ray, own: &[usize], body: usize, own_eps: f64) -> RayVote {
    let Ray::Hits(hits) = ray else {
        return RayVote::Unusable;
    };
    let mut blocked = false;
    for h in hits
        .iter()
        .filter(|h| !(h.body == body && own.contains(&h.face) && h.s <= own_eps))
    {
        match h.class {
            HitClass::Boundary => return RayVote::Unusable,
            HitClass::Inside => blocked = true,
        }
    }
    if blocked {
        RayVote::Blocked
    } else {
        RayVote::Free
    }
}

/// Kippabstimmung an `p`, zählt die Strahlwürfe.
fn sample_counted(
    caster: &dyn Caster,
    basis: &Basis,
    p: DVec3,
    own: &[usize],
    body: usize,
    extent: f64,
    counter: &Cell<u64>,
) -> Result<Option<bool>, DrawError> {
    let eye = basis.eye_dir();
    let min_s = self_eps(extent);
    let own_eps = own_eps(extent);
    let mut blocked = false;
    for k in 0..TILT_DIRECTIONS {
        let phi = k as f64 * std::f64::consts::TAU / TILT_DIRECTIONS as f64;
        let dir = (eye + (basis.ex * phi.cos() + basis.ey * phi.sin()) * TILT).normalize();
        count(counter)?;
        match vote(caster.cast(p, dir, min_s), own, body, own_eps) {
            RayVote::Free => return Ok(Some(true)),
            RayVote::Blocked => blocked = true,
            RayVote::Unusable => {}
        }
    }
    Ok(blocked.then_some(false))
}

/// Ergebnis an einer Stelle: Some(true) = sichtbar, Some(false) = verdeckt, None = unbrauchbar.
pub fn sample_visibility(
    caster: &dyn Caster,
    basis: &Basis,
    p: DVec3,
    own: &[usize],
    body: usize,
    extent: f64,
) -> Option<bool> {
    sample_counted(caster, basis, p, own, body, extent, &Cell::new(0))
        .ok()
        .flatten()
}

/// Gemeinsame Werte einer Beurteilung.
struct Ctx<'a> {
    caster: &'a dyn Caster,
    basis: &'a Basis,
    stroke: &'a Stroke,
    own: &'a [usize],
    body: usize,
    extent: f64,
    name: &'a str,
    counter: &'a Cell<u64>,
    min_width: f64,
    /// Prüfpunkt zu einem Strichpunkt (Spline-Umrisse: Punkt des wahren Umrisses).
    map: &'a dyn Fn(DVec3) -> DVec3,
}

impl Ctx<'_> {
    /// Urteil an der Stelle `f` des Stücks `[ta, tb]`; unbrauchbar → Ausweichstellen.
    fn judge(&self, ta: f64, tb: f64, f: f64) -> Result<Visibility, DrawError> {
        let candidates = std::iter::once(f).chain(
            SHIFTS
                .iter()
                .map(|s| f + s)
                .filter(|g| *g > 0.0 && *g < 1.0),
        );
        for g in candidates {
            let p = (self.map)(self.stroke.point(ta + g * (tb - ta)));
            let v = sample_counted(
                self.caster,
                self.basis,
                p,
                self.own,
                self.body,
                self.extent,
                self.counter,
            )?;
            if let Some(vis) = v {
                let vis = if vis {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
                return Ok(vis);
            }
        }
        Err(DrawError::AmbiguousVisibility(self.name.to_string()))
    }

    /// Innerhalb der Bisektion: Urteile an beiden Enden, wenn beide bekannt sind und das Stück
    /// im Raum kürzer als `OWN_EPS` ist.
    fn bracket(
        &self,
        ta: f64,
        tb: f64,
        left: Option<Visibility>,
        right: Option<Visibility>,
    ) -> Option<(Visibility, Visibility)> {
        let (l, r) = (left?, right?);
        (self.stroke.point(ta).distance(self.stroke.point(tb)) <= own_eps(self.extent))
            .then_some((l, r))
    }

    /// Beurteilt `[ta, tb]`. `left`/`right`: bekannte Urteile genau an den Enden (Mittelstellen
    /// der übergeordneten Stücke), damit ein Wechsel zwischen einer Stützstelle und dem Ende
    /// nicht verloren geht.
    fn range(
        &self,
        ta: f64,
        tb: f64,
        depth: u32,
        left: Option<Visibility>,
        right: Option<Visibility>,
        out: &mut Vec<(f64, f64, Visibility)>,
    ) -> Result<(), DrawError> {
        let mut v = [Visibility::Visible; 3];
        for (k, f) in SAMPLE_FRACTIONS.iter().enumerate() {
            let vis = match self.judge(ta, tb, *f) {
                Ok(x) => x,
                Err(DrawError::AmbiguousVisibility(name)) => {
                    // Am Wechsel selbst (beide Enden bekannt, Breite unter OWN_EPS) liegen alle
                    // Strahlen im Randband: Der Wechsel ist auf diese Breite eingegrenzt.
                    let Some((l, r)) = self.bracket(ta, tb, left, right) else {
                        return Err(DrawError::AmbiguousVisibility(name));
                    };
                    if l == r {
                        out.push((ta, tb, l));
                    } else {
                        let tm = 0.5 * (ta + tb);
                        out.push((ta, tm, l));
                        out.push((tm, tb, r));
                    }
                    return Ok(());
                }
                Err(e) => return Err(e),
            };
            v[k] = vis;
        }
        let uniform = v.iter().all(|x| *x == v[0])
            && left.is_none_or(|l| l == v[0])
            && right.is_none_or(|r| r == v[0]);
        if uniform {
            out.push((ta, tb, v[0]));
            return Ok(());
        }
        if tb - ta < self.min_width || depth >= MAX_SUBDIVISION_DEPTH {
            out.push((ta, tb, v[1]));
            return Ok(());
        }
        let tm = 0.5 * (ta + tb);
        // Urteil der Mittelstelle gilt als bekanntes Ende beider Hälften. Ist sie ausgewichen,
        // liegt sie bis zu 0,071 Stückbreiten neben `tm`; liegt ein Wechsel genau dazwischen,
        // urteilt eine Hälfte uneinheitlich und wird weiter halbiert – das kostet nur Schritte,
        // der Wechsel bleibt auf die Breite der Ausweichstelle genau.
        let mid = Some(v[1]);
        self.range(ta, tm, depth + 1, left, mid, out)?;
        self.range(tm, tb, depth + 1, mid, right, out)
    }
}

/// Beurteilt das Stück [ta, tb] des Strichs; teilt per Bisektion, wenn die Stellen verschieden urteilen.
///
/// Stützstellen bei `SAMPLE_FRACTIONS`, unbrauchbare weichen auf `SHIFTS` aus, sonst
/// `AmbiguousVisibility(name)`. Ein Teilstück gilt nur als einheitlich, wenn auch die bekannten
/// Urteile an seinen Enden übereinstimmen (Mittelstelle des übergeordneten Stücks; auf oberster
/// Ebene die Stellen bei `END_PROBE` und `1 − END_PROBE`). Genauigkeit des Wechsels:
/// `BISECT_TOL · param_len`, außer wo die Stellen am Wechsel selbst unbrauchbar sind (alle
/// Strahlen im Randband des verdeckenden Rands, Breite etwa `16·geom_tol + TILT · Abstand zum
/// Verdecker`): Ist ein Teilstück dann an beiden Enden bekannt und kürzer
/// als `OWN_EPS`, endet die Eingrenzung in seiner Mitte – der Wechsel ist auf die Breite dieses
/// Bands (höchstens `OWN_EPS`) genau.
/// Benachbarte Ergebnisse gleicher Sichtbarkeit werden zusammengefasst; `counter` zählt die
/// Strahlwürfe (über `MAX_RAY_TESTS` → `TooComplex`).
#[allow(clippy::too_many_arguments)]
pub fn classify_range(
    caster: &dyn Caster,
    basis: &Basis,
    stroke: &Stroke,
    own: &[usize],
    body: usize,
    ta: f64,
    tb: f64,
    extent: f64,
    name: &str,
    counter: &Cell<u64>,
) -> Result<Vec<(f64, f64, Visibility)>, DrawError> {
    classify_range_mapped(
        caster,
        basis,
        stroke,
        own,
        body,
        (ta, tb),
        extent,
        name,
        counter,
        &|p| p,
    )
}

/// Wie [`classify_range`], die Prüfstrahlen starten aber bei `map(Strichpunkt)`: Angepasste
/// Umriss-Splines (Torus, Freiform) liegen bis `SILHOUETTE_TOL` neben dem wahren Umriss, also
/// auch bis dahin **in** ihrer Fläche; Strahlen von dort träfen die eigene Fläche weit hinter
/// `OWN_EPS` (bei Krümmungsradius `ρ` und Tiefe `δ` nach etwa `√(2ρδ)`). `compute_view` übergibt
/// für sie den Punkt des wahren Umrisses (`silhouette::outline_point`).
#[allow(clippy::too_many_arguments)]
pub fn classify_range_mapped(
    caster: &dyn Caster,
    basis: &Basis,
    stroke: &Stroke,
    own: &[usize],
    body: usize,
    (ta, tb): (f64, f64),
    extent: f64,
    name: &str,
    counter: &Cell<u64>,
    map: &dyn Fn(DVec3) -> DVec3,
) -> Result<Vec<(f64, f64, Visibility)>, DrawError> {
    let (ta, tb) = (ta.min(tb), ta.max(tb));
    let ctx = Ctx {
        caster,
        basis,
        stroke,
        own,
        body,
        extent,
        name,
        counter,
        min_width: BISECT_TOL * stroke.param_len(),
        map,
    };
    // Sicherheitsnetz: Urteile knapp innerhalb der Enden (ohne Ausweichen; unbrauchbar → kein
    // bekanntes Ende), damit ein Wechsel im äußeren Viertel nicht hinter drei gleichen
    // Stützstellen verschwindet.
    let probe = |f: f64| -> Result<Option<Visibility>, DrawError> {
        let p = map(stroke.point(ta + f * (tb - ta)));
        Ok(
            sample_counted(caster, basis, p, own, body, extent, counter)?.map(|v| {
                if v {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                }
            }),
        )
    };
    let (left, right) = (probe(END_PROBE)?, probe(1.0 - END_PROBE)?);
    let mut raw = Vec::new();
    ctx.range(ta, tb, 0, left, right, &mut raw)?;
    let mut out: Vec<(f64, f64, Visibility)> = Vec::with_capacity(raw.len());
    for (a, b, vis) in raw {
        match out.last_mut() {
            Some(last) if last.2 == vis => last.1 = b,
            _ => out.push((a, b, vis)),
        }
    }
    Ok(out)
}

/// Vielfaches von `geom_tol`, bis zu dem zwei Linien im Bild als dieselbe gelten (Überdeckung
/// beim Zusammenfassen; auch die Dicke, bis zu der ein Kegelschnitt als von der Seite gesehen
/// gilt). Größer als die Toleranz der Teilstellen (16·geom_tol): Silhouetten und Kanten, die
/// geometrisch zusammenfallen, liegen nach Rundung im Kernel bis etwa 1e-10·E auseinander.
pub const COINCIDENT_FACTOR: f64 = 1024.0;
/// Vielfaches von `geom_tol`: Stücke mit kürzerer Bildlänge entfallen.
pub const TINY_FACTOR: f64 = 64.0;
/// Abschnitte für die Bildlänge eines Stücks.
const LENGTH_SAMPLES: usize = 16;
/// Abbruchprüfung je so vielen Stücken bzw. Paaren.
const MERGE_CHECK_EVERY: usize = 1024;
/// Prüfstellen (Bruchteil eines Teilbereichs), die alle beim anderen Stück liegen müssen, damit
/// der Teilbereich als überdeckt gilt: beide Enden, ¼, ½, ¾ und die Stellen des Goldenen
/// Schnitts (eine Kurve, die das andere Stück genau an den Vierteln kreuzt und an den Enden
/// berührt, etwa eine Welle mit zwei Perioden auf einer Geraden, gilt so nicht als bedeckt).
const COVER_FRACTIONS: [f64; 7] = [0.0, 0.25, 0.381_966, 0.5, 0.618_034, 0.75, 1.0];
/// Rastergröße (je Bildachse) für die Vorauswahl der schon behaltenen Stücke.
const MERGE_GRID: usize = 64;

/// Fasst die Stücke einer Ansicht zusammen (Spec §4.4, Festlegung F6, Review Focus 1):
///
/// 1. **Anschließende Stücke** desselben Strichs mit gleicher Sichtbarkeit (`t1` des einen =
///    `t0` des nächsten; bei geschlossenen Strichen auch Ende an Anfang, das Stück reicht dann
///    über die Naht hinaus) werden vereint.
/// 2. **Überdeckungen** ([`trim_covered`]): Die Stücke werden nach Rang behandelt – sichtbar vor
///    verdeckt, dann das dem Auge nähere (Tiefe der Stückmitte), dann die Reihenfolge (Strich,
///    `t0`); eine strenge Ordnung. Von jedem Stück entfällt der Teil, den schon behaltene
///    (ranghöhere) Stücke im Bild bedecken: Ein Teilbereich zwischen den Fußpunkten der Enden
///    der anderen Stücke gilt als bedeckt, wenn seine Enden und seine Stellen bei ¼, ½, ¾ und
///    an den Stellen des Goldenen Schnitts (`COVER_FRACTIONS`) höchstens
///    `COINCIDENT_FACTOR·geom_tol` neben dem Bild eines solchen Stücks liegen (Abstand geometrisch, unabhängig
///    von Kurvenart und Parametrisierung – auch Bögen von der Seite gesehener Kreise auf
///    geraden Kanten). So liegt keine Stelle des Bilds unter zwei Stücken: deckungsgleiche
///    Linien einmal, keine gestrichelten Reste unter durchgezogenen.
/// 3. **Winzige Stücke** (Bildlänge unter `64·geom_tol`) entfallen; ihr Bereich geht an das
///    vorige anschließende Stück desselben Strichs, sonst an das nächste; danach noch einmal
///    Schritt 1.
///
/// Ergebnis nach Strich und `t0` sortiert (deterministisch). Mehr als `MAX_PIECES` Stücke oder
/// mehr als `MAX_PAIR_TESTS` geprüfte Paare → `TooComplex`.
pub fn merge_pieces(
    strokes: &[Stroke],
    pieces: Vec<Piece>,
    basis: &Basis,
    extent: f64,
    progress: &Progress,
) -> Result<Vec<Piece>, DrawError> {
    if pieces.len() as u64 > MAX_PIECES {
        return Err(DrawError::TooComplex(pieces.len() as u64));
    }
    let tol = geom_tol(extent);
    let pieces = join_adjacent(strokes, sorted(pieces), progress)?;
    let pieces = trim_covered(strokes, pieces, basis, extent, progress)?;
    let pieces = drop_tiny(strokes, pieces, basis, TINY_FACTOR * tol, progress)?;
    join_adjacent(strokes, pieces, progress)
}

/// Nach Strich, dann `t0` sortiert.
fn sorted(mut pieces: Vec<Piece>) -> Vec<Piece> {
    pieces.sort_by(|a, b| a.stroke.cmp(&b.stroke).then(a.t0.total_cmp(&b.t0)));
    pieces
}

/// Periode eines geschlossenen Strichs (läuft über eine ganze Periode), sonst `None`.
fn closed_period(s: &Stroke) -> Option<f64> {
    s.curve
        .period()
        .filter(|p| *p > 0.0 && (s.param_len() - p).abs() <= BISECT_TOL * p)
}

/// Schritt 2: anschließende Stücke gleicher Sichtbarkeit vereinen (auch über die Naht).
fn join_adjacent(
    strokes: &[Stroke],
    pieces: Vec<Piece>,
    progress: &Progress,
) -> Result<Vec<Piece>, DrawError> {
    let mut out: Vec<Piece> = Vec::with_capacity(pieces.len());
    for (k, p) in sorted(pieces).into_iter().enumerate() {
        if k % MERGE_CHECK_EVERY == 0 {
            progress.check()?;
        }
        match out.last_mut() {
            Some(l) if l.stroke == p.stroke && l.vis == p.vis && l.t1 == p.t0 => l.t1 = p.t1,
            _ => out.push(p),
        }
    }
    // Über die Naht: Das letzte Stück eines geschlossenen Strichs endet am Bereichsende, das
    // erste beginnt am Bereichsanfang.
    let mut k = 0;
    while k < out.len() {
        let stroke = out[k].stroke;
        let mut m = k;
        while m + 1 < out.len() && out[m + 1].stroke == stroke {
            m += 1;
        }
        let s = &strokes[stroke];
        if let Some(period) = closed_period(s) {
            let (lo, hi) = (s.t0.min(s.t1), s.t0.max(s.t1));
            if m > k && out[m].t1 == hi && out[k].t0 == lo && out[m].vis == out[k].vis {
                out[m].t1 = out[k].t1 + period;
                out.remove(k);
                m -= 1;
            }
        }
        k = m + 1;
    }
    Ok(sorted(out))
}

/// Umriss einer Fläche, der als angepasster Spline vorliegt (Torus, Freiform): nur auf
/// `SILHOUETTE_TOL` genau, im Gegensatz zu Kanten und den genauen Umrissen von Zylinder, Kegel
/// und Kugel.
fn is_fitted(s: &Stroke) -> bool {
    matches!(s.origin, StrokeOrigin::Silhouette { .. }) && matches!(s.curve, Curve::Nurbs(_))
}

/// Rang eines Stücks: sichtbar vor verdeckt, dann genaue vor angepassten Strichen (liegt ein
/// angepasster Umriss auf einer genauen Kurve, bleibt die genaue), dann kleinere Tiefe (näher am
/// Auge), dann der Index (nach Strich und `t0` sortiert) – streng und übergangsfest
/// (lexikographisch).
fn rank_order(pieces: &[Piece], depth: &[f64], fitted: &[bool]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..pieces.len()).collect();
    let key = |v: Visibility| if v == Visibility::Visible { 0 } else { 1 };
    order.sort_by(|&i, &j| {
        key(pieces[i].vis)
            .cmp(&key(pieces[j].vis))
            .then(fitted[i].cmp(&fitted[j]))
            .then(depth[i].total_cmp(&depth[j]))
            .then(i.cmp(&j))
    });
    order
}

/// Ein Stück als eigener Strich (Parameterbereich des Stücks).
fn piece_stroke(strokes: &[Stroke], p: &Piece) -> Stroke {
    Stroke {
        t0: p.t0,
        t1: p.t1,
        ..strokes[p.stroke].clone()
    }
}

/// Raster über dem Bild für die Vorauswahl behaltener Stücke (nach ihrer Hüllbox).
struct MergeGrid {
    lo: DVec2,
    cell: DVec2,
    cells: Vec<Vec<usize>>,
}

impl MergeGrid {
    fn new(lo: DVec2, hi: DVec2) -> MergeGrid {
        let cell = ((hi - lo) / MERGE_GRID as f64).max(DVec2::splat(1e-300));
        MergeGrid {
            lo,
            cell,
            cells: vec![Vec::new(); MERGE_GRID * MERGE_GRID],
        }
    }

    fn range(&self, lo: DVec2, hi: DVec2) -> (usize, usize, usize, usize) {
        let c = |x: f64| {
            if x.is_nan() {
                0
            } else {
                (x.floor().max(0.0) as usize).min(MERGE_GRID - 1)
            }
        };
        let a = (lo - self.lo) / self.cell;
        let b = (hi - self.lo) / self.cell;
        (c(a.x), c(a.y), c(b.x), c(b.y))
    }

    fn insert(&mut self, k: usize, lo: DVec2, hi: DVec2) {
        let (i0, j0, i1, j1) = self.range(lo, hi);
        for i in i0..=i1 {
            for j in j0..=j1 {
                self.cells[i * MERGE_GRID + j].push(k);
            }
        }
    }

    /// Behaltene Stücke, deren Rasterzellen die Box berühren (ohne Wiederholung, aufsteigend).
    fn query(&self, lo: DVec2, hi: DVec2) -> Vec<usize> {
        let (i0, j0, i1, j1) = self.range(lo, hi);
        let mut out = Vec::new();
        for i in i0..=i1 {
            for j in j0..=j1 {
                out.extend_from_slice(&self.cells[i * MERGE_GRID + j]);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Schritt 2 von [`merge_pieces`]: entfernt von jedem Stück die Teile, die ranghöhere,
/// schon behaltene Stücke im Bild bedecken (siehe dort).
fn trim_covered(
    strokes: &[Stroke],
    pieces: Vec<Piece>,
    basis: &Basis,
    extent: f64,
    progress: &Progress,
) -> Result<Vec<Piece>, DrawError> {
    if pieces.is_empty() {
        return Ok(pieces);
    }
    // Stücke von der Seite gesehener Kreise an ihren Umkehrstellen teilen: Ein Stück über eine
    // Umkehrstelle läge im Bild zweimal über sich selbst.
    let thin = COINCIDENT_FACTOR * geom_tol(extent);
    let mut split = Vec::with_capacity(pieces.len());
    for p in pieces {
        let mut a = p.t0;
        for t in tip_params(&strokes[p.stroke], basis, thin, p.t0, p.t1) {
            split.push(Piece { t0: a, t1: t, ..p });
            a = t;
        }
        split.push(Piece { t0: a, ..p });
    }
    let pieces = split;
    let r = strokes[pieces[0].stroke].point(pieces[0].t0);
    let images = Images::new(basis, r, extent, COINCIDENT_FACTOR);
    let near = images.near();
    // Paare mit einem angepassten Umriss: Gleichheitsabstand `SILHOUETTE_TOL + near` (angepasst
    // gegen genau) bzw. `2·SILHOUETTE_TOL + near` (zwei angepasste); genaue Paare behalten `near`.
    let tol_unit = geom_tol(extent);
    let wide_one = Images::new(basis, r, extent, (SILHOUETTE_TOL + near) / tol_unit);
    let wide_two = Images::new(basis, r, extent, (2.0 * SILHOUETTE_TOL + near) / tol_unit);
    let fitted: Vec<bool> = pieces
        .iter()
        .map(|p| is_fitted(&strokes[p.stroke]))
        .collect();
    let pad = |f: bool| if f { wide_two.near() } else { near };
    let mut boxes: Vec<(DVec2, DVec2)> = Vec::with_capacity(pieces.len());
    let mut depth: Vec<f64> = Vec::with_capacity(pieces.len());
    for (k, p) in pieces.iter().enumerate() {
        if k % MERGE_CHECK_EVERY == 0 {
            progress.check()?;
        }
        let (lo, hi) = piece_stroke(strokes, p).bbox2(basis, pad(fitted[k]));
        if !(lo.is_finite() && hi.is_finite()) {
            return Err(DrawError::NonFinite);
        }
        boxes.push((lo, hi));
        let s = &strokes[p.stroke];
        depth.push(basis.depth(s.point(0.5 * (p.t0 + p.t1))));
    }
    let (glo, ghi) = boxes.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(a, b), (lo, hi)| (a.min(*lo), b.max(*hi)),
    );
    let mut grid = MergeGrid::new(glo, ghi);
    // Behaltene Stücke: (Stück als Strich, Hüllbox).
    let mut kept: Vec<(Piece, Stroke)> = Vec::with_capacity(pieces.len());
    let mut pairs: u64 = 0;
    for idx in rank_order(&pieces, &depth, &fitted) {
        progress.check()?;
        let p = pieces[idx];
        let ps = piece_stroke(strokes, &p);
        let (plo, phi) = boxes[idx];
        let mut open: Vec<(f64, f64)> = vec![(p.t0, p.t1)];
        for q in grid.query(plo, phi) {
            pairs += 1;
            if pairs > MAX_PAIR_TESTS {
                return Err(DrawError::TooComplex(pairs));
            }
            if pairs.is_multiple_of(MERGE_CHECK_EVERY as u64) {
                progress.check()?;
            }
            let qs = &kept[q].1;
            let q_fitted = is_fitted(qs);
            let (qlo, qhi) = qs.bbox2(basis, pad(q_fitted));
            let images = match (fitted[idx], q_fitted) {
                (false, false) => &images,
                (true, true) => &wide_two,
                _ => &wide_one,
            };
            if qlo.x > phi.x || plo.x > qhi.x || qlo.y > phi.y || plo.y > qhi.y {
                continue;
            }
            // Teilstellen: Fußpunkte der Enden des anderen Stücks auf diesem. Ein gemeinsamer
            // Abschnitt zweier Stücke derselben Bildkurve endet an einem ihrer Enden.
            let mut cuts = images.feet(&ps, images.point(qs, qs.t0)?)?;
            cuts.extend(images.feet(&ps, images.point(qs, qs.t1)?)?);
            let ends_near = !images.feet(qs, images.point(&ps, p.t0)?)?.is_empty()
                || !images.feet(qs, images.point(&ps, p.t1)?)?.is_empty();
            if cuts.is_empty() && !ends_near {
                continue;
            }
            cuts.sort_by(f64::total_cmp);
            let mut next: Vec<(f64, f64)> = Vec::with_capacity(open.len() + 2);
            for &(a, b) in &open {
                let mut bounds = vec![a];
                bounds.extend(cuts.iter().copied().filter(|&t| t > a && t < b));
                bounds.push(b);
                // Je Teilbereich: Some(bedeckt?) oder None (kaum länger als der
                // Gleichheitsabstand: liegt nur an einem Punkt beim anderen Stück – Kreuzung,
                // Anschluss, zwei Fußpunkte desselben Endes; er folgt seinen Nachbarn).
                let mut state: Vec<Option<bool>> = Vec::with_capacity(bounds.len());
                for w in bounds.windows(2) {
                    let (x, y) = (w[0], w[1]);
                    let part = Piece { t0: x, t1: y, ..p };
                    if image_len(strokes, &part, basis) <= 4.0 * images.near() {
                        state.push(None);
                        continue;
                    }
                    let mut covered = true;
                    for f in COVER_FRACTIONS {
                        let pt = images.point(&ps, x + f * (y - x))?;
                        if images.feet(qs, pt)?.is_empty() {
                            covered = false;
                            break;
                        }
                    }
                    state.push(Some(covered));
                }
                // Kurze Teilbereiche gelten als bedeckt, wenn ein langer Nachbar bedeckt ist und
                // keiner frei (am Rand einer Überdeckung), sonst als frei.
                let n = state.len();
                for i in 0..n {
                    if state[i].is_some() {
                        continue;
                    }
                    let left = state[..i].iter().rev().find_map(|x| *x);
                    let right = state[i + 1..].iter().find_map(|x| *x);
                    let covered = left.unwrap_or(true)
                        && right.unwrap_or(true)
                        && (left.is_some() || right.is_some());
                    state[i] = Some(covered);
                }
                for (w, st) in bounds.windows(2).zip(&state) {
                    if *st != Some(true) {
                        let (x, y) = (w[0], w[1]);
                        match next.last_mut() {
                            Some(l) if l.1 == x => l.1 = y,
                            _ => next.push((x, y)),
                        }
                    }
                }
            }
            open = next;
            if open.is_empty() {
                break;
            }
        }
        for (a, b) in open {
            let piece = Piece { t0: a, t1: b, ..p };
            let st = piece_stroke(strokes, &piece);
            let (lo, hi) = st.bbox2(basis, pad(fitted[idx]));
            grid.insert(kept.len(), lo, hi);
            kept.push((piece, st));
        }
    }
    Ok(sorted(kept.into_iter().map(|(p, _)| p).collect()))
}

/// Bildlänge eines Stücks: Summe der Sehnen über `LENGTH_SAMPLES` Abschnitte.
fn image_len(strokes: &[Stroke], p: &Piece, basis: &Basis) -> f64 {
    let s = &strokes[p.stroke];
    let mut prev = s.point2(basis, p.t0);
    let mut total = 0.0;
    for k in 1..=LENGTH_SAMPLES {
        let q = s.point2(
            basis,
            p.t0 + (p.t1 - p.t0) * k as f64 / LENGTH_SAMPLES as f64,
        );
        total += q.distance(prev);
        prev = q;
    }
    total
}

/// Schritt 3: Winzige Stücke entfallen, ihr Bereich geht an das vorige, sonst das nächste
/// anschließende Stück desselben Strichs.
fn drop_tiny(
    strokes: &[Stroke],
    pieces: Vec<Piece>,
    basis: &Basis,
    tiny: f64,
    progress: &Progress,
) -> Result<Vec<Piece>, DrawError> {
    let mut out: Vec<Piece> = Vec::with_capacity(pieces.len());
    let mut carry: Option<Piece> = None;
    for (k, mut p) in sorted(pieces).into_iter().enumerate() {
        if k % MERGE_CHECK_EVERY == 0 {
            progress.check()?;
        }
        if let Some(c) = carry.take()
            && c.stroke == p.stroke
            && c.t1 == p.t0
        {
            p.t0 = c.t0;
        }
        if image_len(strokes, &p, basis) < tiny {
            match out.last_mut() {
                Some(l) if l.stroke == p.stroke && l.t1 == p.t0 => l.t1 = p.t1,
                _ => carry = Some(p),
            }
            continue;
        }
        out.push(p);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ViewKind;
    use crate::stroke::StrokeOrigin;
    use planora_kernel::Curve;

    fn line(y: f64, edge: usize) -> Stroke {
        Stroke {
            curve: Curve::Line {
                origin: DVec3::new(0.0, y, 0.0),
                dir: DVec3::X,
            },
            t0: 0.0,
            t1: 2.0,
            origin: StrokeOrigin::Edge { body: 0, edge },
        }
    }

    fn piece(stroke: usize, t0: f64, t1: f64, vis: Visibility) -> Piece {
        Piece {
            stroke,
            t0,
            t1,
            vis,
        }
    }

    fn merge(strokes: &[Stroke], pieces: Vec<Piece>) -> Vec<Piece> {
        merge_pieces(
            strokes,
            pieces,
            &Basis::of(ViewKind::Top),
            2.0,
            &Progress::new(),
        )
        .unwrap()
    }

    #[test]
    fn tiny_pieces_go_to_their_neighbour() {
        use Visibility::{Hidden, Visible};
        // geom_tol(2) = 1e-9 (Untergrenze): winzig heißt kürzer als 6,4e-8.
        let s = [line(0.0, 0)];
        // Verdecktes Stück von 1e-8 zwischen zwei sichtbaren: entfällt, alles ein Stück.
        let out = merge(
            &s,
            vec![
                piece(0, 0.0, 1.0, Visible),
                piece(0, 1.0, 1.0 + 1e-8, Hidden),
                piece(0, 1.0 + 1e-8, 2.0, Visible),
            ],
        );
        assert_eq!(out, vec![piece(0, 0.0, 2.0, Visible)]);
        // Am Anfang ohne Vorgänger: an den Nachfolger.
        let out = merge(
            &s,
            vec![piece(0, 0.0, 1e-8, Hidden), piece(0, 1e-8, 2.0, Visible)],
        );
        assert_eq!(out, vec![piece(0, 0.0, 2.0, Visible)]);
        // 1e-7 ist nicht winzig.
        let out = merge(
            &s,
            vec![piece(0, 0.0, 1e-7, Hidden), piece(0, 1e-7, 2.0, Visible)],
        );
        assert_eq!(out.len(), 2);
        // Ein winziger ganzer Strich ohne Nachbarn entfällt.
        let short = Stroke {
            t1: 1e-8,
            ..line(5.0, 1)
        };
        let out = merge(
            &[line(0.0, 0), short],
            vec![piece(0, 0.0, 2.0, Visible), piece(1, 0.0, 1e-8, Visible)],
        );
        assert_eq!(out, vec![piece(0, 0.0, 2.0, Visible)]);
    }

    #[test]
    fn coincident_pieces_keep_the_visible_one_in_either_direction() {
        use Visibility::{Hidden, Visible};
        // Gegenläufige deckungsgleiche Gerade (andere Tiefe): eines bleibt, das sichtbare.
        let back = Stroke {
            curve: Curve::Line {
                origin: DVec3::new(2.0, 0.0, -3.0),
                dir: -DVec3::X,
            },
            ..line(0.0, 1)
        };
        let s = [line(0.0, 0), back];
        let out = merge(
            &s,
            vec![piece(0, 0.0, 2.0, Hidden), piece(1, 0.0, 2.0, Visible)],
        );
        assert_eq!(out, vec![piece(1, 0.0, 2.0, Visible)]);
        // Gleich sichtbar: das dem Auge nähere (Draufsicht: größeres z) …
        let out = merge(
            &s,
            vec![piece(0, 0.0, 2.0, Hidden), piece(1, 0.0, 2.0, Hidden)],
        );
        assert_eq!(out, vec![piece(0, 0.0, 2.0, Hidden)]);
        let front = Stroke {
            curve: Curve::Line {
                origin: DVec3::new(2.0, 0.0, 3.0),
                dir: -DVec3::X,
            },
            ..line(0.0, 2)
        };
        let out = merge(
            &[line(0.0, 0), front.clone()],
            vec![piece(0, 0.0, 2.0, Visible), piece(1, 0.0, 2.0, Visible)],
        );
        assert_eq!(out, vec![piece(1, 0.0, 2.0, Visible)]);
        // … in gleicher Tiefe das erste.
        let out = merge(
            &[line(0.0, 0), line(0.0, 1)],
            vec![piece(0, 0.0, 2.0, Visible), piece(1, 0.0, 2.0, Visible)],
        );
        assert_eq!(out, vec![piece(0, 0.0, 2.0, Visible)]);
        // Ein verdecktes Stück ganz unter einem sichtbaren: entfällt.
        let out = merge(
            &s,
            vec![piece(0, 0.0, 1.0, Hidden), piece(1, 0.0, 2.0, Visible)],
        );
        assert_eq!(out, vec![piece(1, 0.0, 2.0, Visible)]);
        // Teilüberdeckung: das verdeckte Stück wird am Fußpunkt des sichtbaren Endes gekürzt
        // (Rückwärtsgerade x = 2 − t: das sichtbare Stück deckt x ∈ [0; 1,5]).
        let out = merge(
            &s,
            vec![piece(0, 0.0, 2.0, Hidden), piece(1, 0.5, 2.0, Visible)],
        );
        assert_eq!(out.len(), 2, "{out:?}");
        assert_eq!(out[0].stroke, 0);
        assert!(
            (out[0].t0 - 1.5).abs() < 1e-12 && out[0].t1 == 2.0,
            "{out:?}"
        );
    }

    #[test]
    fn the_rank_is_a_strict_order() {
        use Visibility::Visible;
        // Drei deckungsgleiche sichtbare Geraden in den Tiefen 0, 0,6·near und 1,2·near mit den
        // Indizes 2, 1, 0 (Draufsicht: Tiefe = −z). Eine Regel „Tiefe nur über near, sonst
        // Index“ bildete einen Kreis (jede verliert einmal, alle drei entfielen); die strenge
        // Ordnung behält die nächste.
        let near = COINCIDENT_FACTOR * geom_tol(2.0);
        let at = |z: f64, edge: usize| Stroke {
            curve: Curve::Line {
                origin: DVec3::new(0.0, 0.0, z),
                dir: DVec3::X,
            },
            ..line(0.0, edge)
        };
        let s = [at(-1.2 * near, 0), at(-0.6 * near, 1), at(0.0, 2)];
        let out = merge(
            &s,
            vec![
                piece(0, 0.0, 2.0, Visible),
                piece(1, 0.0, 2.0, Visible),
                piece(2, 0.0, 2.0, Visible),
            ],
        );
        assert_eq!(out, vec![piece(2, 0.0, 2.0, Visible)]);
        let order = rank_order(
            &[
                piece(0, 0.0, 2.0, Visible),
                piece(1, 0.0, 2.0, Visible),
                piece(2, 0.0, 2.0, Visible),
            ],
            &[1.2 * near, 0.6 * near, 0.0],
            &[false; 3],
        );
        assert_eq!(order, vec![2, 1, 0]);
    }

    #[test]
    fn a_side_on_arc_on_a_straight_edge_is_drawn_once() {
        use Visibility::{Hidden, Visible};
        // Draufsicht: Kreis r = 1 in der XZ-Ebene (von der Seite, Bild die Strecke
        // x ∈ [−1, 1], y = 0) und eine Gerade y = 0 von x = −1 bis 1 in der Tiefe 5 (weiter weg).
        // Die Bogenhälfte [0, π] und die Gerade decken einander: eine Linie bleibt (die nähere).
        // Ein verdeckter Bogen unter der sichtbaren Geraden entfällt ebenso.
        let arc = Stroke {
            curve: Curve::Circle {
                frame: planora_math::Frame {
                    origin: DVec3::ZERO,
                    x: DVec3::X,
                    y: DVec3::Z,
                    z: -DVec3::Y,
                },
                radius: 1.0,
            },
            t0: 0.0,
            t1: std::f64::consts::TAU,
            origin: StrokeOrigin::Edge { body: 0, edge: 0 },
        };
        let far = Stroke {
            curve: Curve::Line {
                origin: DVec3::new(-1.0, 0.0, -5.0),
                dir: DVec3::X,
            },
            ..line(0.0, 1)
        };
        let pi = std::f64::consts::PI;
        let out = merge(
            &[arc.clone(), far.clone()],
            vec![piece(0, 0.0, pi, Visible), piece(1, 0.0, 2.0, Visible)],
        );
        assert_eq!(out, vec![piece(0, 0.0, pi, Visible)]);
        let out = merge(
            &[arc, far],
            vec![piece(0, 0.0, pi, Hidden), piece(1, 0.0, 2.0, Visible)],
        );
        assert_eq!(out, vec![piece(1, 0.0, 2.0, Visible)]);
    }
}
