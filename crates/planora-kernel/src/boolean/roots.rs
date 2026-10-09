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
/// Bei einer geschlossenen Kurve (Vollkreis) wird die Naht nur einmal gemeldet.
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
    let step = (t1 - t0).abs() / n as f64;

    let mut roots: Vec<Root> = Vec::new();
    let add = |roots: &mut Vec<Root>, t: f64, touching: bool| {
        if !roots.iter().any(|r| (r.t - t).abs() < 1e-7) {
            roots.push(Root { t, touching });
        }
    };

    // Vorzeichenwechsel
    for i in 0..n {
        if gs[i] * gs[i + 1] < 0.0 {
            add(&mut roots, bisect(ts[i], ts[i + 1], &g), false);
        } else if gs[i] == 0.0 && i > 0 && gs[i - 1] * gs[i + 1] < 0.0 {
            add(&mut roots, ts[i], false);
        }
    }
    // Endpunkte, die innerhalb der Toleranz auf der Fläche liegen
    for &i in &[0usize, n] {
        if gs[i].abs() <= tol.linear {
            add(&mut roots, ts[i], false);
        }
    }
    // Berührungen: lokale Minima von |g| ohne Vorzeichenwechsel
    for i in 1..n {
        let (p, c, q) = (gs[i - 1].abs(), gs[i].abs(), gs[i + 1].abs());
        if c <= p && c <= q && gs[i - 1] * gs[i + 1] >= 0.0 {
            let (t, m) = golden_min(ts[i - 1], ts[i + 1], |t| g(t).abs());
            if m <= tol.linear && !roots.iter().any(|r| (r.t - t).abs() < 1e-6) {
                add(&mut roots, t, true);
            }
        }
    }
    roots.sort_by(|a, b| a.t.total_cmp(&b.t));

    // Zwei dicht benachbarte Vorzeichenwechsel, zwischen denen |g| nie über die Toleranz
    // hinausgeht, sind in Wahrheit eine Berührung (Rundungsrauschen am Scheitel).
    let mut merged: Vec<Root> = Vec::with_capacity(roots.len());
    let mut i = 0;
    while i < roots.len() {
        if i + 1 < roots.len() {
            let (a, b) = (roots[i], roots[i + 1]);
            if !a.touching && !b.touching && b.t - a.t <= 2.0 * step {
                let (tm, neg) = golden_min(a.t, b.t, |t| -g(t).abs());
                if -neg <= tol.linear {
                    merged.push(Root {
                        t: tm,
                        touching: true,
                    });
                    i += 2;
                    continue;
                }
            }
        }
        merged.push(roots[i]);
        i += 1;
    }
    let mut roots = merged;

    // Naht einer geschlossenen Kurve nicht doppelt melden.
    if is_closed(curve, t0, t1) && roots.len() > 1 {
        let (lo, hi) = (t0.min(t1), t0.max(t1));
        let first = roots[0].t;
        let last = roots[roots.len() - 1].t;
        if (first - lo).abs() < 1e-9 && (last - hi).abs() < 1e-9 {
            roots.pop();
        }
    }
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
