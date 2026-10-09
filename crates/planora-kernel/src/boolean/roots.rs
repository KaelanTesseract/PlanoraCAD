use super::seg::{Seg, golden_min, sample_count};
use crate::geometry::{Curve, Surface};
use planora_math::{TAU, Tolerance};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Root {
    pub t: f64,
    /// Nur Berührung (kein Vorzeichenwechsel).
    pub touching: bool,
}

#[derive(Clone, Debug)]
pub enum CurveSurface {
    Roots(Vec<Root>),
    /// Die ganze Kurve liegt (innerhalb der Toleranz) auf der Fläche.
    Lies,
}

/// Nullstelle von `g` zwischen `a` und `b` (Vorzeichenwechsel vorausgesetzt) durch Halbieren.
fn bisect(mut a: f64, mut b: f64, g: &impl Fn(f64) -> f64) -> f64 {
    let mut ga = g(a);
    for _ in 0..200 {
        let m = 0.5 * (a + b);
        let gm = g(m);
        if gm == 0.0 || (b - a).abs() < 1e-15 {
            return m;
        }
        if ga * gm < 0.0 {
            b = m;
        } else {
            a = m;
            ga = gm;
        }
    }
    0.5 * (a + b)
}

/// Ein Vollkreis oder eine Vollellipse: Anfang und Ende des Parameterbereichs sind derselbe Punkt.
fn is_closed(curve: &Curve, t0: f64, t1: f64) -> bool {
    matches!(curve, Curve::Circle { .. } | Curve::Ellipse { .. })
        && ((t1 - t0).abs() - TAU).abs() < 1e-9
}

/// Schnittpunkte der Kurve auf `[t0, t1]` mit einer Fläche (Nullstellen von `F(c(t))`).
///
/// Jede Nullstelle wird genau einmal gemeldet. `touching` heißt: `g` wechselt dort das
/// Vorzeichen nicht (Berührung). Randstellen innerhalb der Toleranz zählen als ein Schnitt.
/// Bei einer geschlossenen Kurve (Vollkreis) ist die Naht keine Randstelle: sie wird nur
/// einmal gemeldet und nach dem Vorzeichen davor und dahinter eingestuft.
pub fn curve_surface_roots(
    curve: &Curve,
    t0: f64,
    t1: f64,
    surface: &Surface,
    tol: &Tolerance,
) -> CurveSurface {
    if !t0.is_finite() || !t1.is_finite() {
        return CurveSurface::Roots(Vec::new());
    }
    let n = sample_count(t0, t1, 64, 64);
    let g = |t: f64| surface.implicit(curve.point(t));
    let ts: Vec<f64> = (0..=n)
        .map(|i| t0 + (t1 - t0) * i as f64 / n as f64)
        .collect();
    let gs: Vec<f64> = ts.iter().map(|&t| g(t)).collect();
    if gs.iter().all(|v| v.abs() <= tol.linear) {
        return CurveSurface::Lies;
    }
    let closed = is_closed(curve, t0, t1);
    let dir = if t1 >= t0 { 1.0 } else { -1.0 };
    let lin = tol.linear;

    let mut roots: Vec<Root> = Vec::new();
    let add = |roots: &mut Vec<Root>, t: f64, touching: bool| {
        let eps = if touching { 1e-6 } else { 1e-7 };
        if !roots.iter().any(|r| (r.t - t).abs() < eps) {
            roots.push(Root { t, touching });
        }
    };
    // Extremum von g auf [lo, hi] mit Vorzeichen `sgn` (+1: Minimum, -1: Maximum) -> (t, g(t))
    let extremum = |lo: f64, hi: f64, sgn: f64| {
        let (t, v) = golden_min(lo, hi, |t| sgn * g(t));
        (t, sgn * v)
    };
    // Behandelt einen lokalen Extremwert von g in [lo, hi], dessen Nachbarwerte `ga`, `gb`
    // dasselbe Vorzeichen haben: Berührung (|g| <= tol) oder zwei Schnitte (Vorzeichen gedreht).
    // Liefert true, wenn der Bereich als Berührung behandelt wurde.
    let handle_extremum =
        |roots: &mut Vec<Root>, lo: f64, hi: f64, ga: f64, gb: f64, seam: bool| {
            let sgn = if ga + gb >= 0.0 { 1.0 } else { -1.0 };
            let (te, ge) = extremum(lo, hi, sgn);
            if ge.abs() <= lin {
                let mut te = te;
                if seam {
                    // in den Parameterbereich zurückfalten
                    if (te - t0) * dir < 0.0 {
                        te += dir * TAU;
                    }
                    if (te - t1).abs() < 1e-6 {
                        te = t0;
                    }
                }
                add(roots, te, true);
                true
            } else {
                // `ge` hat dasselbe Vorzeichen wie die Nachbarn: kein Schnitt
                if ge * ga > 0.0 || ge * gb > 0.0 {
                    return false;
                }
                for t in [bisect(lo, te, &g), bisect(te, hi, &g)] {
                    let t = if seam && (t - t0) * dir < 0.0 {
                        t + dir * TAU
                    } else {
                        t
                    };
                    add(roots, t, false);
                }
                false
            }
        };

    // Intervalle, die zu einer Berührung gehören, liefern keine eigenen Vorzeichenwechsel.
    let mut skip = vec![false; n];
    // Lokale Extremwerte von g ohne Vorzeichenwechsel an den Stützstellen (Berührung oder
    // zwei dicht benachbarte Schnitte innerhalb eines Abtastintervalls)
    for i in 1..n {
        let (p, c, q) = (gs[i - 1], gs[i], gs[i + 1]);
        let is_ext = (c <= p && c <= q) || (c >= p && c >= q);
        if is_ext && p * q >= 0.0 && handle_extremum(&mut roots, ts[i - 1], ts[i + 1], p, q, false)
        {
            skip[i - 1] = true;
            skip[i] = true;
        }
    }
    // Naht einer geschlossenen Kurve: g ist periodisch, die Naht ist keine Randstelle.
    let mut seam_crossing = false;
    if closed && n >= 2 {
        let (p, q) = (gs[n - 1], gs[1]);
        if p * q >= 0.0 {
            let step = (t1 - t0).abs() / n as f64;
            let lo = t0 - dir * step;
            let hi = t0 + dir * step;
            let (lo, hi) = if lo < hi { (lo, hi) } else { (hi, lo) };
            if handle_extremum(&mut roots, lo, hi, p, q, true) {
                skip[0] = true;
                skip[n - 1] = true;
            }
        } else if gs[0].abs() <= lin {
            seam_crossing = true;
        }
    }

    // Vorzeichenwechsel zwischen Stützstellen
    for i in 0..n {
        if skip[i] {
            continue;
        }
        // Randstelle innerhalb der Toleranz: gilt selbst als der Schnitt (siehe unten)
        if (i == 0 && gs[0].abs() <= lin) || (i == n - 1 && gs[n].abs() <= lin) {
            continue;
        }
        if gs[i] * gs[i + 1] < 0.0 {
            add(&mut roots, bisect(ts[i], ts[i + 1], &g), false);
        } else if gs[i] == 0.0 && i > 0 && gs[i - 1] * gs[i + 1] < 0.0 {
            add(&mut roots, ts[i], false);
        }
    }
    // Randstellen, die innerhalb der Toleranz auf der Fläche liegen (ein einziger Schnitt, auch
    // wenn die Kurve gleich dahinter das Vorzeichen wechselt). Bei geschlossenen Kurven nur,
    // wenn die Naht wirklich durchquert wird; eine Berührung an der Naht steht schon oben.
    if closed {
        if seam_crossing {
            add(&mut roots, ts[0], false);
        }
    } else {
        for &i in &[0usize, n] {
            if gs[i].abs() <= lin {
                add(&mut roots, ts[i], false);
            }
        }
    }
    roots.sort_by(|a, b| a.t.total_cmp(&b.t));
    CurveSurface::Roots(roots)
}

/// Schnittpunkte zweier Kurvenstücke als Bruchteile `(f_a, f_b)`. Überlappen die Stücke
/// auf einer Strecke, werden die Enden der Überlappung geliefert.
///
/// Es wird in beide Richtungen abgetastet (a gegen b und b gegen a), damit auch ein kleines
/// Stück gegen ein sehr langes nicht zwischen den Stützstellen hindurchrutscht.
pub fn seg_intersections(a: &Seg, b: &Seg, tol: &Tolerance) -> Vec<(f64, f64)> {
    if ![a.t0, a.t1, b.t0, b.t1].iter().all(|v| v.is_finite()) {
        return Vec::new();
    }
    let mut out = one_way(a, b, tol);
    for (fb, fa) in one_way(b, a, tol) {
        let p = a.at(fa);
        if !out
            .iter()
            .any(|&(x, _)| a.at(x).distance(p) < 10.0 * tol.linear)
        {
            out.push((fa, fb));
        }
    }
    // Naht eines Vollkreises nicht doppelt melden (Anfang und Ende sind derselbe Punkt).
    if is_closed(&a.curve, a.t0, a.t1) {
        out.sort_by(|x, y| x.0.total_cmp(&y.0));
        if let (Some(first), Some(last)) = (out.first(), out.last())
            && out.len() > 1
            && a.at(first.0).distance(a.at(last.0)) < 10.0 * tol.linear
        {
            out.pop();
        }
    }
    out.sort_by(|x, y| x.0.total_cmp(&y.0));
    out
}

/// Abtastung von `a` gegen `b`.
fn one_way(a: &Seg, b: &Seg, tol: &Tolerance) -> Vec<(f64, f64)> {
    let near = tol.linear;
    // Punktförmiges Stück: nur prüfen, ob es auf `b` liegt.
    if a.t0 == a.t1 {
        let (fb, d) = b.closest(a.start());
        return if d <= near {
            vec![(0.0, fb)]
        } else {
            Vec::new()
        };
    }
    let n = sample_count(a.t0, a.t1, 64, 64);
    let dist = |f: f64| b.closest(a.at(f)).1;
    let ds: Vec<f64> = (0..=n).map(|i| dist(i as f64 / n as f64)).collect();
    let reach = (a.length() / n as f64) * 2.0 + near;
    let mut out: Vec<(f64, f64)> = Vec::new();
    let push = |out: &mut Vec<(f64, f64)>, fa: f64| {
        let fb = b.closest(a.at(fa)).0;
        if !out.iter().any(|(x, _)| (x - fa).abs() < 1e-6) {
            out.push((fa, fb));
        }
    };

    // Überlappungsstrecken: Läufe von Stützstellen, die der anderen Kurve näher als tol liegen
    let mut covered = vec![false; n + 1];
    let mut i = 0;
    while i <= n {
        if ds[i] <= near {
            let mut j = i;
            while j < n && ds[j + 1] <= near {
                j += 1;
            }
            if j - i >= 2 {
                // Ende des Laufs verfeinern (Übergang zu > tol suchen)
                let lo = if i > 0 {
                    bisect_dist(&dist, (i - 1) as f64 / n as f64, i as f64 / n as f64, near)
                } else {
                    0.0
                };
                let hi = if j < n {
                    bisect_dist(&dist, (j + 1) as f64 / n as f64, j as f64 / n as f64, near)
                } else {
                    1.0
                };
                push(&mut out, lo);
                push(&mut out, hi);
                covered[i..=j].fill(true);
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    // einzelne Schnittpunkte: lokale Minima des Abstands (Überlappungen sind schon erfasst)
    for i in 0..=n {
        if covered[i] {
            continue;
        }
        let prev = if i > 0 { ds[i - 1] } else { f64::MAX };
        let next = if i < n { ds[i + 1] } else { f64::MAX };
        if ds[i] <= prev && ds[i] <= next && ds[i] < reach {
            let lo = if i > 0 {
                (i - 1) as f64 / n as f64
            } else {
                0.0
            };
            let hi = if i < n {
                (i + 1) as f64 / n as f64
            } else {
                1.0
            };
            let (fa, m) = golden_min(lo, hi, dist);
            if m <= near && !out.iter().any(|(x, _)| (x - fa).abs() < 1e-4) {
                push(&mut out, fa);
            }
        }
    }
    out
}

/// Findet den Übergang des Abstands durch `level` zwischen `inside` (≤ level) und `outside` (> level).
fn bisect_dist(dist: &impl Fn(f64) -> f64, outside: f64, inside: f64, level: f64) -> f64 {
    let (mut a, mut b) = (outside, inside);
    for _ in 0..60 {
        let m = 0.5 * (a + b);
        if dist(m) <= level {
            b = m;
        } else {
            a = m;
        }
    }
    b
}
