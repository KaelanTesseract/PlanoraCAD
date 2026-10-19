//! Schraffur-Zuschnitt (Teilprojekt 3d, Schnittansichten): Linienscharen gegen Flächen mit
//! Löchern zuschneiden. Rein 2D.
//!
//! Die Linien liegen auf `n·p = n·origin + k·spacing` (`n` = Normale zur Linienrichtung).
//! Gerade-ungerade-Regel über alle Kanten aller Vielecke; eine Kante zählt halboffen auf der
//! Normalenkoordinate, so dass Linien durch Ecken keine Doppelzählung erzeugen.

use planora_math::DVec2;

/// Obergrenze für die Zahl der Schraffurlinien und -stücke.
pub const MAX_HATCH_LINES: usize = 20_000;

/// Stücke kürzer als dieser Wert entfallen.
const MIN_PIECE: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq)]
pub enum HatchError {
    /// Abstand ≤ 0 oder nicht endlich, Winkel oder Koordinaten nicht endlich.
    BadInput,
    /// Mehr Linien (oder Stücke) als erlaubt; die Zahl ist die ermittelte Anzahl (> `max`).
    TooMany(usize),
}

/// Gerade-ungerade-Regel über alle Kanten aller Vielecke (geschlossen, Reihenfolge und Umlaufsinn egal).
/// Schnitt mit einer Kante zählt halboffen auf der Normalenkoordinate (s0 ≤ c < s1 oder s1 ≤ c < s0);
/// Stücke kürzer als 1e-9 entfallen; Ergebnis je Linie nach Lage sortiert, Linien nach k aufsteigend.
pub fn hatch_lines(
    polys: &[Vec<DVec2>],
    angle: f64,
    spacing: f64,
    origin: DVec2,
    max: usize,
) -> Result<Vec<(DVec2, DVec2)>, HatchError> {
    if !(spacing.is_finite() && spacing > 0.0 && angle.is_finite())
        || !(origin.x.is_finite() && origin.y.is_finite())
    {
        return Err(HatchError::BadInput);
    }
    let polys: Vec<&Vec<DVec2>> = polys.iter().filter(|p| p.len() >= 3).collect();
    if polys.is_empty() {
        return Ok(Vec::new());
    }
    if polys
        .iter()
        .any(|p| p.iter().any(|q| !(q.x.is_finite() && q.y.is_finite())))
    {
        return Err(HatchError::BadInput);
    }
    let d = DVec2::new(angle.cos(), angle.sin());
    let n = DVec2::new(-angle.sin(), angle.cos());
    let c0 = n.dot(origin);
    let (mut smin, mut smax) = (f64::INFINITY, f64::NEG_INFINITY);
    for q in polys.iter().flat_map(|p| p.iter()) {
        let s = n.dot(*q);
        smin = smin.min(s);
        smax = smax.max(s);
    }
    let k_lo = ((smin - c0) / spacing).ceil();
    let k_hi = ((smax - c0) / spacing).floor();
    if k_hi < k_lo {
        return Ok(Vec::new());
    }
    // Zählung zuerst: zu viele Linien werden abgelehnt, bevor etwas gebaut wird.
    let count = k_hi - k_lo + 1.0;
    if count > max as f64 {
        let n_lines = if count >= usize::MAX as f64 {
            usize::MAX
        } else {
            count as usize
        };
        return Err(HatchError::TooMany(n_lines));
    }
    let (k_lo, k_hi) = (k_lo as i64, k_hi as i64);

    let mut out: Vec<(DVec2, DVec2)> = Vec::new();
    let mut ts: Vec<f64> = Vec::new();
    for k in k_lo..=k_hi {
        let c = c0 + k as f64 * spacing;
        ts.clear();
        for poly in &polys {
            let m = poly.len();
            for i in 0..m {
                let (a, b) = (poly[i], poly[(i + 1) % m]);
                let (s0, s1) = (n.dot(a), n.dot(b));
                let hit = (s0 <= c && c < s1) || (s1 <= c && c < s0);
                if hit {
                    let (t0, t1) = (d.dot(a), d.dot(b));
                    ts.push(t0 + (c - s0) / (s1 - s0) * (t1 - t0));
                }
            }
        }
        ts.sort_by(|x, y| x.total_cmp(y));
        for pair in ts.chunks(2).filter(|p| p.len() == 2) {
            if pair[1] - pair[0] < MIN_PIECE {
                continue;
            }
            if out.len() >= max {
                return Err(HatchError::TooMany(out.len() + 1));
            }
            out.push((n * c + d * pair[0], n * c + d * pair[1]));
        }
    }
    Ok(out)
}
