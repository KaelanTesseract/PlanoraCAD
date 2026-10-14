//! Sichtbarkeit der Linienstücke (Festlegung F9): An jeder Stützstelle stimmen 8 leicht gekippte
//! Prüfstrahlen zum Auge ab. Frei unter irgendeinem Kippstrahl heißt sichtbar (eine Linie genau
//! auf dem Umriss einer Fläche oder in der Ebene einer von der Seite gesehenen Fläche ist nach
//! Zeichenkonvention sichtbar); verdeckt nur, wenn alle brauchbaren Strahlen verdeckt sind.
//! Randtreffer und unsichere Strahlen zählen nicht; ohne brauchbaren Strahl weicht die Stelle
//! aus, danach `AmbiguousVisibility`. Urteilen die Stellen eines Stücks verschieden, wird es
//! halbiert, bis der Wechsel auf `BISECT_TOL` (relativ) genau eingegrenzt ist, oder – wo die
//! Strahlen am Wechsel selbst nur den verdeckenden Rand streifen – auf die Breite dieses
//! Randbands (höchstens `OWN_EPS`), siehe [`classify_range`].

use crate::Visibility;
use crate::basis::Basis;
use crate::error::DrawError;
use crate::limits::{BISECT_TOL, MAX_RAY_TESTS, MAX_SUBDIVISION_DEPTH, own_eps, self_eps};
use crate::raycast::{Caster, HitClass, Ray};
use crate::stroke::Stroke;
use planora_math::DVec3;
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
            let p = self.stroke.point(ta + g * (tb - ta));
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
    };
    // Sicherheitsnetz: Urteile knapp innerhalb der Enden (ohne Ausweichen; unbrauchbar → kein
    // bekanntes Ende), damit ein Wechsel im äußeren Viertel nicht hinter drei gleichen
    // Stützstellen verschwindet.
    let probe = |f: f64| -> Result<Option<Visibility>, DrawError> {
        let p = stroke.point(ta + f * (tb - ta));
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
