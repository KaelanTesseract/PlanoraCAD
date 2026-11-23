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
use planora_kernel::{Curve, NURBS_EDGE_TOL};
use planora_math::{DVec2, DVec3};
use std::cell::Cell;

/// Kippwinkel der Prüfstrahlen (rad).
pub const TILT: f64 = 1e-5;
/// Kippwinkel des Rückfalls an Freiformkanten, wenn alle Strahlen mit [`TILT`] unbrauchbar sind
/// (sie streifen auf ganzer Länge den Rand einer Fläche, deren Rand nur genähert ist; C15b).
pub const TILT_FALLBACK: f64 = 1e-3;
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
    /// Unbrauchbar wegen eines Randtreffers auf einer fremden Fläche (weder eigene noch
    /// Nachbarfläche): Die Stelle liegt im Bild auf einer Randkante davor.
    Grazing,
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

/// Ein Strahl: Treffer auf eigenen Flächen mit `s ≤ OWN_EPS` entfallen, ebenso Randtreffer auf
/// Nachbarflächen über Freiformkanten mit `s ≤ near_eps` ([`NEIGHBOR_FLAG`]); bleibt ein
/// Randtreffer, ist der Strahl unbrauchbar, sonst verdeckt ihn ein Innentreffer.
fn vote(ray: Ray, own: &[usize], body: usize, own_eps: f64, near_eps: f64) -> RayVote {
    let Ray::Hits(hits) = ray else {
        return RayVote::Unusable;
    };
    let mut blocked = false;
    // Freiformkante: Sie liegt nur auf `NURBS_EDGE_TOL` genau auf ihren Flächen; ein flach
    // streifender Strahl träfe eine eigene Fläche erst hinter `OWN_EPS` (C15b).
    let freeform = own
        .iter()
        .any(|f| *f == FREEFORM_STROKE || f & NEIGHBOR_FLAG != 0);
    let own_eps = if own.contains(&FREEFORM_STROKE) {
        own_eps.max(near_eps)
    } else {
        own_eps
    };
    for h in hits.iter().filter(|h| {
        !(h.body == body && own.contains(&h.face) && h.s <= own_eps)
            && !(h.body == body
                && own.contains(&(h.face | NEIGHBOR_FLAG))
                && matches!(h.class, HitClass::Boundary)
                && h.s <= near_eps)
    }) {
        match h.class {
            HitClass::Boundary => {
                let mine = h.body == body
                    && (own.contains(&h.face) || own.contains(&(h.face | NEIGHBOR_FLAG)));
                return if mine {
                    RayVote::Unusable
                } else {
                    RayVote::Grazing
                };
            }
            // An Freiformkanten (C15b): Ein Innentreffer vor jedem Randtreffer verdeckt die Stelle,
            // was dahinter kommt, ist ohne Belang (die Treffer sind nach `s` sortiert).
            HitClass::Inside if freeform => return RayVote::Blocked,
            HitClass::Inside => blocked = true,
        }
    }
    if blocked {
        RayVote::Blocked
    } else {
        RayVote::Free
    }
}

/// Marke in der Liste der eigenen Flächen: Der Strich ist eine Freiformkante (C15b).
pub const FREEFORM_STROKE: usize = usize::MAX;

/// Kennung einer Nachbarfläche in der Liste der eigenen Flächen eines Strichs (Index plus diese
/// Marke): eine Fläche, die über eine Freiformkante an einer eigenen Fläche hängt (C15b).
pub const NEIGHBOR_FLAG: usize = 1 << 40;

/// Bis zu diesem Abstand zählen Randtreffer auf Nachbarflächen ([`NEIGHBOR_FLAG`]) nicht:
/// Endet ein Umriss oder eine Kante im Bild tangential auf einer Freiformkante, liegt deren
/// Berührpunkt nur auf `NURBS_EDGE_TOL` genau, und die Prüfstrahlen streifen den Rand der
/// Nachbarfläche auf einer Länge von etwa `√(2·R·NURBS_EDGE_TOL)` (C15b). Innentreffer zählen.
fn near_eps(extent: f64) -> f64 {
    (2.0 * extent.max(1e-9) * planora_kernel::NURBS_EDGE_TOL).sqrt()
}

/// Urteil der Kippabstimmung an einer Stelle.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Sample {
    /// Sichtbar (`true`) oder verdeckt.
    Seen(bool),
    /// Freiformkante, jeder Strahl streift den Rand einer fremden Fläche davor (C15b): Die
    /// Stelle liegt im Bild auf deren Randkante, etwa eine steile Schnittkurve auf einer Wand,
    /// die man von der Kante sieht, unter dem Randkreis der Deckfläche. Die vordere Kante wird
    /// dort gezeichnet; ist sie verdeckt, ist es die Stelle dahinter auch. Gilt als verdeckt,
    /// wenn keine Ausweichstelle entscheidet (das Zusammenfassen nimmt das verdeckte Stück unter
    /// einer sichtbaren weg).
    Grazing,
    Unknown,
}

/// Kippabstimmung an `p` (verdeckt auch bei [`Sample::Grazing`]), zählt die Strahlwürfe.
fn sample_counted(
    caster: &dyn Caster,
    basis: &Basis,
    p: DVec3,
    own: &[usize],
    body: usize,
    extent: f64,
    counter: &Cell<u64>,
) -> Result<Option<bool>, DrawError> {
    Ok(
        match sample_raw(caster, basis, p, own, body, extent, counter)? {
            Sample::Seen(v) => Some(v),
            Sample::Grazing => Some(false),
            Sample::Unknown => None,
        },
    )
}

/// Kippabstimmung an `p`, zählt die Strahlwürfe.
fn sample_raw(
    caster: &dyn Caster,
    basis: &Basis,
    p: DVec3,
    own: &[usize],
    body: usize,
    extent: f64,
    counter: &Cell<u64>,
) -> Result<Sample, DrawError> {
    let eye = basis.eye_dir();
    let min_s = self_eps(extent);
    let own_eps = own_eps(extent);
    // Striche an Freiformkanten (eigene oder Nachbarfläche über eine Freiformkante, C15b):
    // Sind alle Strahlen mit `TILT` unbrauchbar, folgt ein Satz mit `TILT_FALLBACK`.
    let freeform = own
        .iter()
        .any(|f| *f == FREEFORM_STROKE || f & NEIGHBOR_FLAG != 0);
    let tilts: &[f64] = if freeform {
        &[TILT, TILT_FALLBACK]
    } else {
        &[TILT]
    };
    let mut blocked = false;
    let mut all_grazing = true;
    for &tilt in tilts {
        for k in 0..TILT_DIRECTIONS {
            let phi = k as f64 * std::f64::consts::TAU / TILT_DIRECTIONS as f64;
            let dir = (eye + (basis.ex * phi.cos() + basis.ey * phi.sin()) * tilt).normalize();
            count(counter)?;
            let ray = caster.cast(p, dir, min_s);
            match vote(ray, own, body, own_eps, near_eps(extent)) {
                RayVote::Free => return Ok(Sample::Seen(true)),
                RayVote::Blocked => blocked = true,
                RayVote::Unusable => all_grazing = false,
                RayVote::Grazing => {}
            }
        }
        if blocked {
            break;
        }
    }
    Ok(if blocked {
        Sample::Seen(false)
    } else if freeform && all_grazing {
        Sample::Grazing
    } else {
        Sample::Unknown
    })
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
    /// Urteil an der Stelle `f` des Stücks `[ta, tb]`; unbrauchbar → Ausweichstellen. Streift
    /// an `f` jeder Strahl ([`Sample::Grazing`]) und entscheidet keine Ausweichstelle: verdeckt.
    fn judge(&self, ta: f64, tb: f64, f: f64) -> Result<Visibility, DrawError> {
        let candidates = std::iter::once(f).chain(
            SHIFTS
                .iter()
                .map(|s| f + s)
                .filter(|g| *g > 0.0 && *g < 1.0),
        );
        let mut grazing = false;
        for (k, g) in candidates.enumerate() {
            let p = (self.map)(self.stroke.point(ta + g * (tb - ta)));
            match sample_raw(
                self.caster,
                self.basis,
                p,
                self.own,
                self.body,
                self.extent,
                self.counter,
            )? {
                Sample::Seen(true) => return Ok(Visibility::Visible),
                Sample::Seen(false) => return Ok(Visibility::Hidden),
                Sample::Grazing if k == 0 => grazing = true,
                _ => {}
            }
        }
        if grazing {
            return Ok(Visibility::Hidden);
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
    // Sicherheitsnetz: Urteile knapp innerhalb der Enden (ohne Ausweichen; unbrauchbar oder
    // streifend → kein bekanntes Ende), damit ein Wechsel im äußeren Viertel nicht hinter drei
    // gleichen Stützstellen verschwindet.
    let probe = |f: f64| -> Result<Option<Visibility>, DrawError> {
        let p = map(stroke.point(ta + f * (tb - ta)));
        Ok(
            match sample_raw(caster, basis, p, own, body, extent, counter)? {
                Sample::Seen(true) => Some(Visibility::Visible),
                Sample::Seen(false) => Some(Visibility::Hidden),
                Sample::Grazing | Sample::Unknown => None,
            },
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
/// Vielfaches von `NURBS_EDGE_TOL`: In Ansichten mit Freiformkanten entfallen Stücke mit kürzerer
/// Bildlänge (C15b): 1e-4 mm wie `SILHOUETTE_TOL`. Reste an Umkehrstellen, an denen zwei Hälften
/// einer Schnittkurve zusammenlaufen, sind im Bild bis etwa 7e-5 mm lang.
pub const FREEFORM_TINY_FACTOR: f64 = 10.0;
/// Abschnitte für die Bildlänge eines Stücks.
const LENGTH_SAMPLES: usize = 16;
/// Abbruchprüfung je so vielen Stücken bzw. Paaren.
const MERGE_CHECK_EVERY: usize = 1024;
/// Prüfstellen (Bruchteil eines Teilbereichs), die alle beim anderen Stück liegen müssen, damit
/// der Teilbereich als überdeckt gilt: beide Enden, ¼, ½, ¾ und die Stellen des Goldenen
/// Schnitts (eine Kurve, die das andere Stück genau an den Vierteln kreuzt und an den Enden
/// berührt, etwa eine Welle mit zwei Perioden auf einer Geraden, gilt so nicht als bedeckt).
const COVER_FRACTIONS: [f64; 7] = [0.0, 0.25, 0.381_966, 0.5, 0.618_034, 0.75, 1.0];
/// Proben je offenem Bereich, an denen bei Freiformkanten der Übergang „im Gleichheitsabstand /
/// nicht“ gesucht wird (C15b), und Halbierungsschritte für seine Lage.
const COVER_SCAN: usize = 32;
const COVER_BISECT: usize = 40;
/// Anteil, um den die Abdeckungsprobe bei Freiformpaaren von den Enden eines Teilbereichs abrückt.
const COVER_INSET: f64 = 1e-3;
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
    // Mit Freiformkanten (C15b): Stücke kürzer als ihre Genauigkeit sind Rechenreste an Ecken
    // (etwa am Ende eines Bogens, der auf eine Freiformkante trifft).
    let freeform = strokes
        .iter()
        .any(|s| matches!(s.curve, Curve::Nurbs(_)) && !is_fitted(s));
    let tiny = if freeform {
        (TINY_FACTOR * tol).max(FREEFORM_TINY_FACTOR * NURBS_EDGE_TOL)
    } else {
        TINY_FACTOR * tol
    };
    let pieces = drop_tiny(strokes, pieces, basis, tiny, progress)?;
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

/// Proben je Stück für [`flat_spline_tips`].
const FLAT_TIP_SAMPLES: usize = 64;

/// Umkehrstellen des Bilds einer Freiformkante (C15): Stellen, an denen die Bildgeschwindigkeit
/// auf null fällt und das Bild auf sich selbst zurückläuft – etwa eine ebene Schnittkurve, deren
/// Ebene die Blickrichtung enthält, oder eine Kante auf einer von der Seite gesehenen Zylinder-
/// wand. Dort läge das Bild sonst zweimal über sich selbst (wie bei [`tip_params`] für Kreise).
/// Gesucht an den Proben, an denen die Bildrichtung umschlägt; die Stelle der kleinsten
/// Bildgeschwindigkeit dazwischen auf die Rechengenauigkeit. Echt innerhalb `(a, b)`.
fn flat_spline_tips(s: &Stroke, basis: &Basis, _thin: f64, a: f64, b: f64) -> Vec<f64> {
    if !matches!(s.curve, Curve::Nurbs(_)) || b <= a {
        return Vec::new();
    }
    let n = FLAT_TIP_SAMPLES;
    let ts: Vec<f64> = (0..=n).map(|i| a + (b - a) * i as f64 / n as f64).collect();
    let ps: Vec<DVec2> = ts.iter().map(|&t| s.point2(basis, t)).collect();
    let h = 1e-7 * (b - a);
    let speed = |t: f64| (s.point2(basis, t + h) - s.point2(basis, t - h)).length() / (2.0 * h);
    let mut out = Vec::new();
    for i in 1..n {
        // Die Bildrichtung schlägt um: Die Sehnen vor und nach der Probe zeigen gegeneinander.
        let (d0, d1) = (ps[i] - ps[i - 1], ps[i + 1] - ps[i]);
        if d0.dot(d1) < 0.0 {
            let t = golden_extreme(&|t| -speed(t), ts[i - 1], ts[i + 1]);
            if t > a && t < b {
                out.push(t);
            }
        }
    }
    out
}

/// Stelle des größten Werts von `f` auf `[lo, hi]` (Goldener Schnitt; `f` dort eingipflig).
fn golden_extreme(f: &dyn Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    const R: f64 = 0.618_033_988_749_894_8;
    let mut x1 = hi - R * (hi - lo);
    let mut x2 = lo + R * (hi - lo);
    let (mut f1, mut f2) = (f(x1), f(x2));
    for _ in 0..80 {
        if f1 < f2 {
            lo = x1;
            x1 = x2;
            f1 = f2;
            x2 = lo + R * (hi - lo);
            f2 = f(x2);
        } else {
            hi = x2;
            x2 = x1;
            f2 = f1;
            x1 = hi - R * (hi - lo);
            f1 = f(x1);
        }
    }
    if f1 >= f2 { x1 } else { x2 }
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
        let mut tips = tip_params(&strokes[p.stroke], basis, thin, p.t0, p.t1);
        tips.extend(flat_spline_tips(
            &strokes[p.stroke],
            basis,
            thin + 2.0 * SILHOUETTE_TOL,
            p.t0,
            p.t1,
        ));
        tips.sort_by(f64::total_cmp);
        for t in tips {
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
    // Freiformkanten (etwa Schnittkurven in allgemeiner Lage, C15) liegen nur auf
    // `NURBS_EDGE_TOL` genau auf ihren Flächen: Sie tragen `2·NURBS_EDGE_TOL` bei (Klasse 1).
    let tol_unit = geom_tol(extent);
    // Eine Freiformkante liegt bis `NURBS_EDGE_TOL` neben jeder ihrer Flächen; zwei Hälften
    // derselben Schnittkurve, die im Bild zusammenfallen, also bis `2·NURBS_EDGE_TOL` auseinander.
    let extra = [0.0, 2.0 * NURBS_EDGE_TOL, SILHOUETTE_TOL];
    let class_of = |st: &Stroke| -> usize {
        if is_fitted(st) {
            2
        } else if matches!(st.curve, Curve::Nurbs(_)) {
            1
        } else {
            0
        }
    };
    let pair_images: Vec<Vec<Images>> = (0..3)
        .map(|a| {
            (0..3)
                .map(|b| Images::new(basis, r, extent, (extra[a] + extra[b] + near) / tol_unit))
                .collect()
        })
        .collect();
    // Wie `pair_images`, aber ohne den Zuschlag angepasster Umrisse (Suche der Übergänge an
    // Freiformkanten, C15b).
    let scan_images: Vec<Vec<Images>> = (0..3)
        .map(|a| {
            (0..3)
                .map(|b| {
                    let e = |c: usize| if c == 2 { 0.0 } else { extra[c] };
                    Images::new(basis, r, extent, (e(a) + e(b) + near) / tol_unit)
                })
                .collect()
        })
        .collect();
    let fitted: Vec<bool> = pieces
        .iter()
        .map(|p| is_fitted(&strokes[p.stroke]))
        .collect();
    let class: Vec<usize> = pieces
        .iter()
        .map(|p| class_of(&strokes[p.stroke]))
        .collect();
    let pad = |c: usize| near + 2.0 * extra[c];
    let mut boxes: Vec<(DVec2, DVec2)> = Vec::with_capacity(pieces.len());
    let mut depth: Vec<f64> = Vec::with_capacity(pieces.len());
    for (k, p) in pieces.iter().enumerate() {
        if k % MERGE_CHECK_EVERY == 0 {
            progress.check()?;
        }
        let (lo, hi) = piece_stroke(strokes, p).bbox2(basis, pad(class[k]));
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
            let q_class = class_of(qs);
            let (qlo, qhi) = qs.bbox2(basis, pad(q_class));
            let images = if class[idx] == 0 && q_class == 0 {
                &images
            } else {
                &pair_images[class[idx]][q_class]
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
            // Freiformkante (C15b): Sie kann das andere Stück im Bild berühren und dort ein Stück
            // weit innerhalb des Gleichheitsabstands laufen (Schnittkurve tangential am Umriss);
            // dann auch an den Stellen teilen, an denen sie diesen Abstand verlässt.
            // Nur echte Freiformkanten (Klasse 1), nicht angepasste Umrisse.
            // Gesucht wird mit dem Gleichheitsabstand ohne den Zuschlag angepasster Umrisse
            // (`scan_images`): Sonst fiele die ganze Berührzone einer Schnittkante am Umriss bis
            // `SILHOUETTE_TOL` weg, und dort fehlte eine Linie, die neben der anderen liegt.
            let freeform_pair = class[idx] == 1 || q_class == 1;
            let scan = &scan_images[class[idx]][q_class];
            let mut scan_cuts: Vec<f64> = Vec::new();
            if freeform_pair {
                let near_at = |t: f64| -> Result<bool, DrawError> {
                    Ok(!scan.feet(qs, scan.point(&ps, t)?)?.is_empty())
                };
                for &(a, b) in &open {
                    let ts: Vec<f64> = (0..=COVER_SCAN)
                        .map(|i| a + (b - a) * i as f64 / COVER_SCAN as f64)
                        .collect();
                    let mut prev = near_at(ts[0])?;
                    for w in ts.windows(2) {
                        let now = near_at(w[1])?;
                        if now != prev {
                            let (mut lo, mut hi) = (w[0], w[1]);
                            for _ in 0..COVER_BISECT {
                                let mid = 0.5 * (lo + hi);
                                if near_at(mid)? == prev {
                                    lo = mid;
                                } else {
                                    hi = mid;
                                }
                            }
                            scan_cuts.push(0.5 * (lo + hi));
                        }
                        prev = now;
                    }
                }
            }
            cuts.extend(&scan_cuts);
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
                    // An einer Übergangsstelle (oben per Halbierung gesucht) liegt der Abstand
                    // genau auf der Grenze; dort knapp innerhalb des Teilbereichs prüfen.
                    // Ein Teilbereich an einer solchen Stelle gilt nur mit dem engeren Abstand
                    // als bedeckt.
                    let inset = if freeform_pair { COVER_INSET } else { 0.0 };
                    let at_scan = scan_cuts.contains(&x) || scan_cuts.contains(&y);
                    let test = if at_scan { scan } else { images };
                    let mut covered = true;
                    for f in COVER_FRACTIONS {
                        let f = inset + (1.0 - 2.0 * inset) * f;
                        let pt = test.point(&ps, x + f * (y - x))?;
                        if test.feet(qs, pt)?.is_empty() {
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
            let (lo, hi) = st.bbox2(basis, pad(class[idx]));
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

    fn hit(s: f64, face: usize, class: HitClass) -> crate::raycast::Hit {
        crate::raycast::Hit {
            s,
            body: 0,
            face,
            class,
        }
    }

    /// C15b: Randtreffer auf einer Nachbarfläche über eine Freiformkante zählen nahe dem Start
    /// nicht; weiter weg, auf anderen Flächen oder im Inneren zählen sie.
    #[test]
    fn near_rim_hits_on_freeform_neighbours_are_ignored() {
        let own = [3, 5 | NEIGHBOR_FLAG];
        let (own_eps, near) = (1e-4, 0.02);
        let v = |h: Vec<crate::raycast::Hit>| vote(Ray::Hits(h), &own, 0, own_eps, near);
        assert!(matches!(
            v(vec![hit(0.001, 5, HitClass::Boundary)]),
            RayVote::Free
        ));
        assert!(matches!(
            v(vec![hit(0.05, 5, HitClass::Boundary)]),
            RayVote::Unusable
        ));
        assert!(matches!(
            v(vec![hit(0.001, 6, HitClass::Boundary)]),
            RayVote::Grazing
        ));
        assert!(matches!(
            v(vec![hit(0.001, 5, HitClass::Inside)]),
            RayVote::Blocked
        ));
        // Eigene Flächen wie bisher bis OWN_EPS.
        assert!(matches!(
            v(vec![hit(5e-5, 3, HitClass::Boundary)]),
            RayVote::Free
        ));
        assert!(matches!(
            v(vec![hit(0.001, 3, HitClass::Inside)]),
            RayVote::Blocked
        ));
        // Eine Freiformkante: Treffer auf eigenen Flächen zählen bis `near_eps` nicht.
        let own_ff = [3, FREEFORM_STROKE];
        let w = |h: Vec<crate::raycast::Hit>| vote(Ray::Hits(h), &own_ff, 0, own_eps, near);
        assert!(matches!(
            w(vec![hit(0.001, 3, HitClass::Inside)]),
            RayVote::Free
        ));
        assert!(matches!(
            w(vec![hit(0.05, 3, HitClass::Inside)]),
            RayVote::Blocked
        ));
        // An Freiformkanten entscheidet ein Innentreffer vor einem Randtreffer; sonst bleibt
        // der Strahl unbrauchbar wie bisher.
        let both = || {
            vec![
                hit(0.1, 9, HitClass::Inside),
                hit(0.2, 0, HitClass::Boundary),
            ]
        };
        assert!(matches!(w(both()), RayVote::Blocked));
        assert!(matches!(
            vote(Ray::Hits(both()), &[3], 0, own_eps, near),
            RayVote::Grazing
        ));
    }

    /// Gibt für jeden Strahl dieselben Treffer zurück.
    struct Fixed(Vec<crate::raycast::Hit>);
    impl Caster for Fixed {
        fn cast(&self, _: DVec3, _: DVec3, _: f64) -> Ray {
            Ray::Hits(self.0.clone())
        }
    }

    /// C15b: Streift an einer Freiformkante jeder Strahl den Rand einer fremden Fläche davor,
    /// liegt die Stelle im Bild auf deren Randkante: verdeckt (die vordere Kante wird dort
    /// gezeichnet). Bei genauen Kanten und bei Randtreffern auf eigenen oder Nachbarflächen
    /// bleibt die Stelle unentschieden.
    #[test]
    fn grazing_a_front_rim_hides_a_freeform_stroke() {
        let basis = Basis::of(ViewKind::Top);
        let n = Cell::new(0);
        let at = |hits: Vec<crate::raycast::Hit>, own: &[usize]| {
            sample_counted(&Fixed(hits), &basis, DVec3::ZERO, own, 0, 10.0, &n).unwrap()
        };
        let rim = || vec![hit(0.01, 13, HitClass::Boundary)];
        assert_eq!(at(rim(), &[1, 5, FREEFORM_STROKE]), Some(false));
        assert_eq!(at(rim(), &[1, 5 | NEIGHBOR_FLAG]), Some(false));
        assert_eq!(at(rim(), &[1, 5]), None);
        let own_rim = || vec![hit(0.5, 5, HitClass::Boundary)];
        assert_eq!(at(own_rim(), &[1, 5, FREEFORM_STROKE]), None);
    }

    /// Streift bis `x < 1.01` (fremder Rand bei `s` = 0,01), dahinter frei.
    struct Split;
    impl Caster for Split {
        fn cast(&self, origin: DVec3, _: DVec3, _: f64) -> Ray {
            if origin.x < 1.01 {
                Ray::Hits(vec![hit(0.01, 13, HitClass::Boundary)])
            } else {
                Ray::Hits(Vec::new())
            }
        }
    }

    /// Prüferfund C15b: Die Streifregel greift erst, wenn auch die Ausweichstellen nichts
    /// entscheiden; entscheidet keine, bleibt sie (verdeckt).
    #[test]
    fn grazing_yields_to_a_decided_shift() {
        let basis = Basis::of(ViewKind::Top);
        let n = Cell::new(0);
        let st = line(0.0, 1);
        let own = [1, FREEFORM_STROKE];
        let id = |p: DVec3| p;
        let ctx = |caster: &'static dyn Caster| Ctx {
            caster,
            basis: &basis,
            stroke: &st,
            own: &own,
            body: 0,
            extent: 10.0,
            name: "S",
            counter: &n,
            min_width: 1e-9,
            map: &id,
        };
        // Stelle x = 1,0 streift, die Ausweichstelle x ≈ 1,025 ist frei.
        assert_eq!(
            ctx(&Split).judge(0.0, 2.0, 0.5).unwrap(),
            Visibility::Visible
        );
        // Überall streifend: verdeckt.
        static ALL: Fixed2 = Fixed2;
        assert_eq!(ctx(&ALL).judge(0.0, 2.0, 0.5).unwrap(), Visibility::Hidden);
    }

    struct Fixed2;
    impl Caster for Fixed2 {
        fn cast(&self, _: DVec3, _: DVec3, _: f64) -> Ray {
            Ray::Hits(vec![hit(0.01, 13, HitClass::Boundary)])
        }
    }

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
