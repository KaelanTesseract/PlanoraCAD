//! Startpunktsuche: gerichtete Unterteilung der Bézier-Flicken mit sicherem Ausschluss,
//! Normalenkegel-Test gegen Schleifen und mehrere Kurvenäste je Kandidat, Newton im Raum.

use std::f64::consts::{FRAC_PI_2, PI};

use super::{
    H_MIN, Hints, MAX_SEEDS, MAX_SUBDIVISION_DEPTH, MSG_NEAR_TANGENT, MSG_NUMERIC,
    PARALLEL_NORMALS, Probe, Side, Ssi, msg_too_many_seeds,
};
use crate::geometry::Surface;
use crate::nurbs_surface::NurbsSurfaceData;
use planora_math::DVec3;
use planora_math::nurbs::BezierPatch;

/// Ein Flicken gilt als eben genug, wenn sein Ebenheitsmaß höchstens dieser Anteil seiner
/// Diagonale ist.
const SEED_FLAT_FACTOR: f64 = 0.05;
/// Kandidat-Flicken sind klein: Diagonale höchstens dieser Anteil der Gesamtdiagonale.
const SEED_PATCH_FRACTION: f64 = 1.0 / 16.0;
/// Gegen eine gekrümmte analytische Fläche zusätzlich höchstens dieses Vielfache ihres kleinsten
/// Krümmungsradius (Zylinder-, Kegel-, Kugelradius, Rohrradius des Torus).
const SEED_FEATURE_FACTOR: f64 = 1.0;
/// Absolute Untergrenze der Flickenausdehnung (mm), unter der nicht mehr unterteilt wird.
const MIN_PATCH_DIAG: f64 = 1e-4;
/// Höchstzahl geprüfter Flicken bzw. Flickenpaare bei der Unterteilung (Arbeitsgrenze).
const MAX_PATCH_VISITS: usize = 200_000;
/// Höchstzahl Kandidaten-Flicken (jeder liefert höchstens einen Startpunkt).
const MAX_CANDIDATES: usize = 8 * MAX_SEEDS;
/// Höchstzahl Newton-Schritte bei der Suche eines Startpunkts.
const SEED_NEWTON_STEPS: usize = 50;
/// Newton endet vorzeitig, wenn der beste Rest in `STAGNATION_STEPS` Schritten nicht wenigstens
/// um den Faktor `STAGNATION_RATIO` sinkt.
const STAGNATION_STEPS: usize = 6;
const STAGNATION_RATIO: f64 = 0.5;
/// Größter Newton-Schritt bei der Startpunktsuche als Anteil der Gesamtdiagonale.
const SEED_MAX_MOVE: f64 = 0.25;
/// Unterhalb dieser Gram-Determinante `1 − (g_a·g_b)²` (= `|g_a × g_b|²`) wird abwechselnd auf
/// beide Flächen projiziert statt des gemeinsamen Newton-Schritts.
const SEED_DET_MIN: f64 = 1e-20;
/// Ein Grenz-Kandidat ohne Startpunkt zählt als Beinahe-Berührung, wenn der beste Rest höchstens
/// dieses Vielfache von `tol.linear` ist.
const NEAR_CONTACT_FACTOR: f64 = 10.0;
/// Stichproben je Richtung für die Prüfung auf zusammenfallende Flächen (10 · 10 = 100).
const COINCIDENT_SAMPLES: usize = 10;
/// Anteil der Stichproben, die auf der anderen Fläche liegen müssen.
pub(super) const COINCIDENT_SHARE: f64 = 0.9;
/// Liegt mindestens dieser Anteil (aber weniger als `COINCIDENT_SHARE`) auf der anderen Fläche,
/// fallen die Flächen teilweise zusammen. Eine Berührlinie trifft höchstens eine Zeile, Spalte
/// oder Diagonale des 10 · 10-Rasters (unter 20 %), eine flächige Überdeckung ab einem Viertel.
pub(super) const PARTIAL_SHARE: f64 = 0.25;
/// Relative Toleranz, mit der Gewichte als gleich bzw. als Produkt `α_i · β_j` erkannt werden.
const WEIGHT_RATIO: f64 = 1e-12;
/// Normalenkegel an Polen (dort fehlt die sichere Schranke aus den Tangentenkegeln) aus
/// `POLE_CONE_SAMPLES²` Stichproben, erweitert um den größten Winkel benachbarter Stichproben.
const POLE_CONE_SAMPLES: usize = 5;

/// Größte Kandidatendiagonale.
pub(super) fn max_patch(a: &Surface, b: &Surface, diag: f64) -> f64 {
    (SEED_PATCH_FRACTION * diag)
        .min(SEED_FEATURE_FACTOR * feature_size(a))
        .min(SEED_FEATURE_FACTOR * feature_size(b))
}

/// Kleinster Krümmungsradius einer analytischen Fläche (Ebene, Freiformfläche: keiner).
fn feature_size(s: &Surface) -> f64 {
    let r = match s {
        Surface::Cylinder { radius, .. }
        | Surface::Cone { radius, .. }
        | Surface::Sphere { radius, .. } => radius.abs(),
        Surface::Torus { minor, .. } => minor.abs(),
        Surface::Plane { .. } | Surface::Nurbs(_) => f64::INFINITY,
    };
    if r.is_finite() && r > MIN_PATCH_DIAG {
        r
    } else {
        f64::INFINITY
    }
}

/// Flicken mit Unterteilungstiefe je Richtung.
#[derive(Clone)]
pub(super) struct Piece {
    pub(super) p: BezierPatch,
    depth: [usize; 2],
}

/// Knoten der Unterteilung: je Freiformseite ein Flicken.
#[derive(Clone)]
pub(super) struct Node {
    pub(super) a: Option<Piece>,
    pub(super) b: Option<Piece>,
}

/// Kegel von Richtungen: Achse (Einheitsvektor) und halber Öffnungswinkel.
#[derive(Clone, Copy, Debug)]
pub(super) struct Cone {
    axis: DVec3,
    half: f64,
}

/// Ergebnis des Normalenkegel-Tests.
enum ConeCheck {
    /// Die Normalen können über dem Kandidaten nicht parallel sein.
    Pass,
    /// Weiter unterteilen: Seite (`true` = a) und Richtung (`true` = u).
    Split(bool, bool),
    /// Nicht weiter teilbar, Normalen möglicherweise parallel.
    Limit,
}

/// Ergebnis der Startpunktsuche eines Kandidaten.
pub(super) struct SeedResult {
    pub(super) found: Option<Probe>,
    last: Option<Probe>,
    residual: f64,
}

fn bounds(p: &BezierPatch) -> (DVec3, DVec3) {
    p.bounds()
}

fn diag_of(p: &BezierPatch) -> f64 {
    let (lo, hi) = bounds(p);
    (hi - lo).length()
}

/// Kontrollpunkt und Gewicht `(i, j)` (i in u, j in v).
fn at(p: &BezierPatch, i: usize, j: usize) -> (DVec3, f64) {
    let k = i * (p.degree_v + 1) + j;
    (p.control[k], p.weights[k])
}

/// Halbierung in einer Richtung (de Casteljau in homogenen Koordinaten).
fn split_dir(p: &BezierPatch, along_u: bool) -> [BezierPatch; 2] {
    let (nu, nv) = (p.degree_u + 1, p.degree_v + 1);
    let (lines, len) = if along_u { (nv, nu) } else { (nu, nv) };
    let idx = |line: usize, k: usize| {
        if along_u {
            k * nv + line
        } else {
            line * nv + k
        }
    };
    let mut lo = p.clone();
    let mut hi = p.clone();
    for line in 0..lines {
        let mut h: Vec<(DVec3, f64)> = (0..len)
            .map(|k| {
                let i = idx(line, k);
                (p.control[i] * p.weights[i], p.weights[i])
            })
            .collect();
        let mut left = Vec::with_capacity(len);
        let mut right = vec![(DVec3::ZERO, 0.0); len];
        for r in 0..len {
            left.push(h[0]);
            right[len - 1 - r] = h[len - 1 - r];
            for k in 0..len - 1 - r {
                h[k] = ((h[k].0 + h[k + 1].0) * 0.5, 0.5 * (h[k].1 + h[k + 1].1));
            }
        }
        for k in 0..len {
            let i = idx(line, k);
            let (lp, lw) = left[k];
            let (rp, rw) = right[k];
            lo.control[i] = lp / lw;
            lo.weights[i] = lw;
            hi.control[i] = rp / rw;
            hi.weights[i] = rw;
        }
    }
    if along_u {
        let m = 0.5 * (p.u.0 + p.u.1);
        lo.u = (p.u.0, m);
        hi.u = (m, p.u.1);
    } else {
        let m = 0.5 * (p.v.0 + p.v.1);
        lo.v = (p.v.0, m);
        hi.v = (m, p.v.1);
    }
    [lo, hi]
}

/// Ausdehnung in einer Richtung: längster Kontrollpolygonzug dieser Richtung.
fn extent(p: &BezierPatch, along_u: bool) -> f64 {
    let (nu, nv) = (p.degree_u + 1, p.degree_v + 1);
    let (lines, len) = if along_u { (nv, nu) } else { (nu, nv) };
    (0..lines)
        .map(|line| {
            (1..len)
                .map(|k| {
                    let (a, b) = if along_u {
                        (at(p, k - 1, line).0, at(p, k, line).0)
                    } else {
                        (at(p, line, k - 1).0, at(p, line, k).0)
                    };
                    a.distance(b)
                })
                .sum::<f64>()
        })
        .fold(0.0, f64::max)
}

/// Richtungsvektoren, deren positive Hülle jede Ableitung `S_u` (bzw. `S_v`) des Flickens
/// enthält. Polynomial: Differenzen benachbarter Kontrollpunkte einer Linie; Produktgewichte
/// `w_ij = α_i β_j` (Dreh- und Verschiebeflächen): alle Differenzen `P_l − P_k` (l > k) einer
/// Linie (rationale Bézier-Kurve); sonst alle Differenzen `P_{l,j'} − P_{k,j}` (l > k, beliebige
/// Linien). Nullvektoren (Pole) entfallen, sie tragen nichts bei.
pub(super) fn tangents(p: &BezierPatch, along_u: bool) -> Vec<DVec3> {
    let (nu, nv) = (p.degree_u + 1, p.degree_v + 1);
    let (lines, len) = if along_u { (nv, nu) } else { (nu, nv) };
    let get = |line: usize, k: usize| {
        if along_u {
            at(p, k, line)
        } else {
            at(p, line, k)
        }
    };
    let w0 = p.weights[0];
    let near = |a: f64, b: f64| (a - b).abs() <= WEIGHT_RATIO * a.abs().max(b.abs());
    let equal = p.weights.iter().all(|w| near(*w, w0));
    let product = (0..nu).all(|i| {
        (0..nv).all(|j| near(at(p, i, j).1 * at(p, 0, 0).1, at(p, i, 0).1 * at(p, 0, j).1))
    });
    let scale = p
        .control
        .iter()
        .fold(0.0_f64, |m, q| m.max(q.abs().max_element()));
    let tiny = 16.0 * f64::EPSILON * scale.max(1.0);
    let mut out = Vec::new();
    let mut push = |d: DVec3| {
        if d.length() > tiny {
            out.push(d);
        }
    };
    for line in 0..lines {
        for k in 0..len {
            for l in k + 1..len {
                if equal && l != k + 1 {
                    continue;
                }
                if equal || product {
                    push(get(line, l).0 - get(line, k).0);
                } else {
                    for other in 0..lines {
                        push(get(other, l).0 - get(line, k).0);
                    }
                }
            }
        }
    }
    out
}

/// Kegel um die Summe der Einheitsvektoren.
fn cone_of(v: &[DVec3]) -> Option<Cone> {
    let sum: DVec3 = v.iter().map(|d| d.normalize()).sum();
    let axis = sum.normalize_or_zero();
    if v.is_empty() || axis == DVec3::ZERO {
        return None;
    }
    let half = v
        .iter()
        .map(|d| d.normalize().dot(axis).clamp(-1.0, 1.0).acos())
        .fold(0.0, f64::max);
    Some(Cone { axis, half })
}

/// Sichere Schranke der Normalenrichtungen `S_u × S_v` aus den Tangentenkegeln (Binet–Cauchy):
/// `cos ∠(n, a_u × a_v) ≥ (cos θ_u cos θ_v − c_uv c_vu) / sin φ`.
fn normal_cone_from(du: &[DVec3], dv: &[DVec3]) -> Option<Cone> {
    let (cu, cv) = (cone_of(du)?, cone_of(dv)?);
    if cu.half >= FRAC_PI_2 || cv.half >= FRAC_PI_2 {
        return None;
    }
    let phi = cu.axis.dot(cv.axis).clamp(-1.0, 1.0).acos();
    let max_cos = |th: f64| {
        let lo = (phi - th).max(0.0);
        let hi = (phi + th).min(PI);
        lo.cos().abs().max(hi.cos().abs())
    };
    let l = (cu.half.cos() * cv.half.cos() - max_cos(cu.half) * max_cos(cv.half)) / phi.sin();
    let axis = cu.axis.cross(cv.axis).normalize_or_zero();
    (l > 0.0 && l.is_finite() && axis != DVec3::ZERO).then(|| Cone {
        axis,
        half: l.min(1.0).acos(),
    })
}

/// Liegen alle Vektoren echt auf einer Seite jeder Ebene senkrecht zu einer Richtung im Kegel?
/// Dann ist `n · S ≠ 0` (festes Vorzeichen) für jede Normale `n` im Kegel und jede positive
/// Kombination `S`. Liefert dazu die Spannweite der Skalarprodukte (für die Wahl der Teilung).
pub(super) fn separates(v: &[DVec3], cone: &Cone) -> (bool, f64) {
    if v.is_empty() || cone.half >= FRAC_PI_2 {
        return (false, 0.0);
    }
    let s = cone.half.sin();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for d in v {
        let x = d.normalize().dot(cone.axis);
        lo = lo.min(x);
        hi = hi.max(x);
    }
    (lo > s || hi < -s, hi - lo)
}

/// Kein Paar paralleler (oder antiparalleler) Richtungen zwischen zwei Kegeln?
fn cones_apart(a: &Cone, b: &Cone) -> bool {
    let s = a.half + b.half;
    s < FRAC_PI_2 && a.axis.dot(b.axis).abs() < s.cos()
}

/// Normalen einer analytischen Fläche an allen ihren Punkten im Quader `[lo, hi]` (sichere
/// Schranke); `None`, wenn sie alle Richtungen umfassen können.
pub(super) fn analytic_cone(s: &Surface, lo: DVec3, hi: DVec3) -> Option<Cone> {
    let c = (lo + hi) * 0.5;
    let r = 0.5 * (hi - lo).length();
    let f = s.frame();
    // Radialrichtung um die Achse: Kegel der Richtungen vom Achspunkt zum Quader (senkrecht zur
    // Achse projiziert; der projizierte Quader liegt in einer Kugel vom Radius `r`).
    let radial = || -> Option<(DVec3, f64, f64, f64)> {
        let v = c - f.origin;
        let h = v.dot(f.z);
        let w = v - f.z * h;
        let d = w.length();
        (d > r).then(|| (w / d, (r / d).asin(), d, h))
    };
    match s {
        Surface::Plane { frame } => Some(Cone {
            axis: frame.z,
            half: 0.0,
        }),
        Surface::Sphere { frame, .. } => {
            let v = c - frame.origin;
            let d = v.length();
            (d > r).then(|| Cone {
                axis: v / d,
                half: (r / d).asin(),
            })
        }
        Surface::Cylinder { .. } => radial().map(|(axis, half, ..)| Cone { axis, half }),
        Surface::Cone { frame, slope, .. } => radial().map(|(m, half, ..)| Cone {
            axis: (m - frame.z * *slope).normalize(),
            half,
        }),
        Surface::Torus { frame, major, .. } => {
            let (m, half_u, d, h) = radial()?;
            // Winkel um das Rohr: Quader in der (ρ − R, h)-Halbebene, ρ ∈ [d − r, d + r].
            let (x0, x1, y0, y1) = (d - r - major, d + r - major, h - r, h + r);
            if x0 <= 0.0 && x1 >= 0.0 && y0 <= 0.0 && y1 >= 0.0 {
                return None;
            }
            let mid = (h).atan2(d - major);
            let half_v = [(x0, y0), (x0, y1), (x1, y0), (x1, y1)]
                .iter()
                .map(|(x, y)| {
                    let a = y.atan2(*x) - mid;
                    (a + PI).rem_euclid(2.0 * PI) - PI
                })
                .fold(0.0_f64, |acc, a| acc.max(a.abs()));
            let half = half_u + half_v;
            (half < FRAC_PI_2).then(|| Cone {
                axis: (m * mid.cos() + frame.z * mid.sin()).normalize(),
                half,
            })
        }
        Surface::Nurbs(_) => None,
    }
}

/// Liegt der Flicken an einem Pol der Fläche?
fn at_pole(d: &NurbsSurfaceData, p: &BezierPatch) -> bool {
    d.poles_v().iter().any(|pv| p.v.0 == *pv || p.v.1 == *pv)
}

/// Normalenkegel eines Flickens: sicher aus den Tangentenkegeln; an Polen (dort umfassen die
/// Tangentenkegel alle Richtungen) aus Stichproben der Normalen, erweitert um den größten Winkel
/// benachbarter Stichproben (Schätzung, keine sichere Schranke).
pub(super) fn patch_cone(
    d: &NurbsSurfaceData,
    p: &BezierPatch,
    du: &[DVec3],
    dv: &[DVec3],
) -> Option<Cone> {
    if let Some(c) = normal_cone_from(du, dv) {
        return Some(c);
    }
    if !at_pole(d, p) {
        return None;
    }
    let n = POLE_CONE_SAMPLES;
    let mut normals = Vec::with_capacity(n * n);
    for i in 0..n {
        for j in 0..n {
            let u = p.u.0 + (p.u.1 - p.u.0) * i as f64 / (n - 1) as f64;
            let v = p.v.0 + (p.v.1 - p.v.0) * j as f64 / (n - 1) as f64;
            normals.push(d.normal(u, v));
        }
    }
    if normals.iter().any(|x| *x == DVec3::ZERO || !x.is_finite()) {
        return None;
    }
    let angle = |a: DVec3, b: DVec3| a.dot(b).clamp(-1.0, 1.0).acos();
    let mut step = 0.0_f64;
    for i in 0..n {
        for j in 0..n {
            let x = normals[i * n + j];
            if i + 1 < n {
                step = step.max(angle(x, normals[(i + 1) * n + j]));
            }
            if j + 1 < n {
                step = step.max(angle(x, normals[i * n + j + 1]));
            }
        }
    }
    let cone = cone_of(&normals)?;
    let half = cone.half + step;
    (half < FRAC_PI_2).then_some(Cone {
        axis: cone.axis,
        half,
    })
}

/// Kann ein Stück mit diesen Kontrollpunkten (es liegt in ihrer konvexen Hülle, alle Gewichte
/// positiv) die analytische Fläche `other` nicht treffen? Sicher: Lipschitz-Schranke über dem
/// Hüllquader, Ebene: alle Kontrollpunkte echt auf einer Seite; Kugel, Zylinder, Kegel: `F` ist
/// konvex (Abstand zu Punkt bzw. Achse), also ist die Hülle innen, wenn alle Kontrollpunkte innen
/// sind; außen ist `F` mindestens die lineare Stützfunktion längs der Radialrichtung der
/// Quadermitte, deren Minimum über der Hülle an einem Kontrollpunkt liegt.
pub(super) fn excluded_points(other: &Surface, control: &[DVec3], lin: f64) -> bool {
    let (lo, hi) = control.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(l, h), q| (l.min(*q), h.max(*q)),
    );
    let c = (lo + hi) * 0.5;
    let r = 0.5 * (hi - lo).length();
    let f = other.implicit(c);
    if f.is_finite() && f.abs() > r + lin {
        return true;
    }
    let fs: Vec<f64> = control.iter().map(|q| other.implicit(*q)).collect();
    match other {
        // linear: alle Kontrollpunkte echt auf einer Seite
        Surface::Plane { .. } => fs.iter().all(|f| *f > lin) || fs.iter().all(|f| *f < -lin),
        Surface::Sphere { frame, radius } => {
            let m = (c - frame.origin).normalize_or_zero();
            fs.iter().all(|f| *f < -lin)
                || (m != DVec3::ZERO
                    && control
                        .iter()
                        .all(|q| m.dot(*q - frame.origin) - radius > lin))
        }
        Surface::Cylinder { frame, radius } => {
            let w = c - frame.origin;
            let m = (w - frame.z * w.dot(frame.z)).normalize_or_zero();
            fs.iter().all(|f| *f < -lin)
                || (m != DVec3::ZERO
                    && control
                        .iter()
                        .all(|q| m.dot(*q - frame.origin) - radius > lin))
        }
        Surface::Cone {
            frame,
            radius,
            slope,
        } => {
            let w = c - frame.origin;
            let m = (w - frame.z * w.dot(frame.z)).normalize_or_zero();
            let k = (1.0 + slope * slope).sqrt();
            fs.iter().all(|f| *f < -lin)
                || (m != DVec3::ZERO
                    && control.iter().all(|q| {
                        let d = *q - frame.origin;
                        (m.dot(d) - (radius + slope * d.dot(frame.z))) / k > lin
                    }))
        }
        Surface::Torus { .. } | Surface::Nurbs(_) => false,
    }
}

impl Ssi<'_> {
    /// Anteil der Stichproben (10 · 10 im Parameterbereich) der Freiformseite `s`, die innerhalb
    /// `tol.linear` auf `o` liegen. Analytische Seiten sind unbeschränkt und werden nicht
    /// abgetastet (`None`; „gegenseitig“ prüft dann nur die Freiformseite).
    pub(super) fn share_on(&self, s: &Side, o: &Side) -> Option<f64> {
        self.share_within(s, o, self.tol.linear)
    }

    /// Wie `share_on` mit Abstandsschranke `dist`.
    pub(super) fn share_within(&self, s: &Side, o: &Side, dist: f64) -> Option<f64> {
        let d = s.nurbs?;
        let ((u0, u1), (v0, v1)) = d.surface().domain();
        let n = COINCIDENT_SAMPLES;
        let mut on = 0usize;
        for i in 0..n {
            for j in 0..n {
                let u = u0 + (u1 - u0) * (i as f64 + 0.5) / n as f64;
                let v = v0 + (v1 - v0) * (j as f64 + 0.5) / n as f64;
                let f = o.surf.implicit(d.point(u, v));
                if f.is_finite() && f.abs() <= dist {
                    on += 1;
                }
            }
        }
        Some(on as f64 / (n * n) as f64)
    }

    fn count_visit(&mut self) -> Result<(), String> {
        self.visits += 1;
        if self.visits > MAX_PATCH_VISITS {
            return Err(format!(
                "Zu viele Schnittbereiche (mehr als {MAX_PATCH_VISITS} geprüfte Flicken)"
            ));
        }
        Ok(())
    }

    /// Kann der Flicken die analytische Fläche `other` nicht treffen (siehe `excluded_points`)?
    fn excluded_by(&self, other: &Side, p: &BezierPatch) -> bool {
        excluded_points(other.surf, &p.control, self.tol.linear)
    }

    /// Zwei Freiform-Flicken können sich nicht treffen: Hüllquader getrennt oder die
    /// Kontrollpunkte liegen längs einer Normalenachse in getrennten Schichten (konvexe Hülle).
    fn pair_excluded(&self, pa: &Piece, pb: &Piece) -> bool {
        let (la, ha) = bounds(&pa.p);
        let (lb, hb) = bounds(&pb.p);
        let m = DVec3::splat(self.tol.linear);
        if !((la - m).cmple(hb).all() && (lb - m).cmple(ha).all()) {
            return true;
        }
        let axes = [(&self.a, pa), (&self.b, pb)].map(|(side, piece)| {
            side.nurbs.and_then(|d| {
                let p = &piece.p;
                patch_cone(d, p, &tangents(p, true), &tangents(p, false)).map(|c| c.axis)
            })
        });
        let slab = |axis: DVec3, p: &BezierPatch| {
            p.control
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), q| {
                    let x = q.dot(axis);
                    (l.min(x), h.max(x))
                })
        };
        axes.into_iter().flatten().any(|axis| {
            let (a0, a1) = slab(axis, &pa.p);
            let (b0, b1) = slab(axis, &pb.p);
            b0 > a1 + self.tol.linear || a0 > b1 + self.tol.linear
        })
    }

    fn keep(&self, n: &Node) -> bool {
        match (&n.a, &n.b) {
            (Some(p), None) => !self.excluded_by(&self.b, &p.p),
            (None, Some(p)) => !self.excluded_by(&self.a, &p.p),
            (Some(p), Some(q)) => !self.pair_excluded(p, q),
            (None, None) => false,
        }
    }

    /// Ist die Richtung noch teilbar?
    fn can_split(piece: &Piece, along_u: bool) -> bool {
        let k = if along_u { 0 } else { 1 };
        piece.depth[k] < MAX_SUBDIVISION_DEPTH && extent(&piece.p, along_u) >= MIN_PATCH_DIAG
    }

    /// Muss ein Flicken wegen Größe oder Wölbung geteilt werden? Seite und Richtung (die längere).
    fn size_split(&self, n: &Node) -> Option<(bool, bool)> {
        let mut best: Option<(f64, bool, bool)> = None;
        for (is_a, piece) in [(true, &n.a), (false, &n.b)] {
            let Some(piece) = piece else { continue };
            let d = diag_of(&piece.p);
            if d <= self.max_patch && piece.p.flatness() <= SEED_FLAT_FACTOR * d {
                continue;
            }
            let (eu, ev) = (extent(&piece.p, true), extent(&piece.p, false));
            let dir = match (Self::can_split(piece, true), Self::can_split(piece, false)) {
                (true, true) => eu >= ev,
                (true, false) => true,
                (false, true) => false,
                (false, false) => continue,
            };
            if best.is_none_or(|b| d > b.0) {
                best = Some((d, is_a, dir));
            }
        }
        best.map(|(_, s, d)| (s, d))
    }

    /// Normalenkegel-Test: Können die Normalen beider Seiten über dem Kandidaten parallel sein?
    fn cone_check(&self, n: &Node) -> ConeCheck {
        struct Info<'p> {
            piece: &'p Piece,
            du: Vec<DVec3>,
            dv: Vec<DVec3>,
            cone: Option<Cone>,
        }
        fn info<'p>(side: &Side, piece: &'p Option<Piece>) -> Option<Info<'p>> {
            let piece = piece.as_ref()?;
            let d = side.nurbs?;
            let (du, dv) = (tangents(&piece.p, true), tangents(&piece.p, false));
            let cone = patch_cone(d, &piece.p, &du, &dv);
            Some(Info {
                piece,
                du,
                dv,
                cone,
            })
        }
        let ia = info(&self.a, &n.a);
        let ib = info(&self.b, &n.b);
        // Normalenkegel der anderen Seite: Freiform aus ihrem Flicken, analytisch über dem
        // Hüllquader des Freiform-Flickens.
        let other_cone = |own: &Info, other: &Option<Info>, other_side: &Side| match other {
            Some(o) => o.cone,
            None => {
                let (lo, hi) = bounds(&own.piece.p);
                analytic_cone(other_side.surf, lo, hi)
            }
        };
        // Teilungsoptionen
        #[derive(Clone, Copy)]
        struct Opt {
            score: f64,
            side_a: bool,
            along_u: bool,
            splittable: bool,
        }
        let mut options: Vec<Opt> = Vec::new();
        for (is_a, own, other, other_side) in
            [(true, &ia, &ib, &self.b), (false, &ib, &ia, &self.a)]
        {
            let Some(own) = own else { continue };
            let oc = other_cone(own, other, other_side);
            for (along_u, d) in [(true, &own.du), (false, &own.dv)] {
                let spread = match &oc {
                    Some(c) => {
                        let (ok, spread) = separates(d, c);
                        if ok {
                            return ConeCheck::Pass;
                        }
                        spread
                    }
                    None => 0.0,
                };
                if let (Some(c), Some(mine)) = (&oc, &own.cone)
                    && cones_apart(mine, c)
                {
                    return ConeCheck::Pass;
                }
                // Bewertung: Spannweite der Skalarprodukte (Teilen dieser Richtung trennt die
                // Vorzeichen) plus Kegelweite der anderen Seite, anteilig zur Ausdehnung in dieser
                // Richtung (Teilen der langen Richtung verkleinert den Hüllquader und verengt so
                // den Kegel; ohne Kegel zählt nur die Ausdehnung).
                let gamma = oc.map_or(FRAC_PI_2, |c| c.half.min(FRAC_PI_2));
                let (eu, ev) = (extent(&own.piece.p, true), extent(&own.piece.p, false));
                let ext = if along_u { eu } else { ev } / eu.max(ev).max(f64::MIN_POSITIVE);
                options.push(Opt {
                    score: spread + gamma * ext,
                    side_a: is_a,
                    along_u,
                    splittable: Self::can_split(own.piece, along_u),
                });
            }
        }
        // Die beste noch mögliche Teilung (auch die andere Richtung verkleinert den Hüllquader,
        // verengt so den Kegel der anderen Seite und ermöglicht den Ausschluss); erst wenn
        // nichts mehr teilbar ist, ist der Kandidat an der Grenze.
        match options
            .iter()
            .filter(|o| o.splittable)
            .max_by(|x, y| x.score.total_cmp(&y.score))
        {
            Some(o) => ConeCheck::Split(o.side_a, o.along_u),
            None => ConeCheck::Limit,
        }
    }

    /// Die beiden Hälften eines Knotens (geteilt wird der Flicken der Seite `side_a`; fehlt er,
    /// gibt es nichts zu teilen).
    fn split_node(n: &Node, side_a: bool, along_u: bool) -> Vec<Node> {
        let k = if along_u { 0 } else { 1 };
        let piece = if side_a { &n.a } else { &n.b };
        let Some(piece) = piece.as_ref() else {
            return Vec::new();
        };
        let halves = split_dir(&piece.p, along_u);
        halves
            .into_iter()
            .map(|p| {
                let mut depth = piece.depth;
                depth[k] += 1;
                let child = Some(Piece { p, depth });
                Node {
                    a: if side_a { child.clone() } else { n.a.clone() },
                    b: if side_a { n.b.clone() } else { child },
                }
            })
            .collect()
    }

    /// Startpunktsuche und Verfolgung, verschränkt: Ein Startpunkt auf einer schon verfolgten
    /// Kurve zählt nicht als eigener Startpunkt (siehe `handle_seed`).
    pub(super) fn search(&mut self) -> Result<(), String> {
        let patches = |s: &Side| -> Result<Option<Vec<BezierPatch>>, String> {
            s.nurbs
                .map(|d| d.surface().bezier_patches())
                .transpose()
                .map_err(|e| format!("Die Freiformfläche lässt sich nicht zerlegen: {e}"))
        };
        let (pa, pb) = (patches(&self.a)?, patches(&self.b)?);
        let piece = |p: &BezierPatch| Piece {
            p: p.clone(),
            depth: [0, 0],
        };
        let roots: Vec<(Option<usize>, Option<usize>)> = match (&pa, &pb) {
            (Some(x), None) => (0..x.len()).map(|i| (Some(i), None)).collect(),
            (None, Some(y)) => (0..y.len()).map(|j| (None, Some(j))).collect(),
            (Some(x), Some(y)) => {
                if x.len().saturating_mul(y.len()) > MAX_PATCH_VISITS {
                    return Err(format!(
                        "Zu viele Schnittbereiche (mehr als {MAX_PATCH_VISITS} Flickenpaare)"
                    ));
                }
                (0..x.len())
                    .flat_map(|i| (0..y.len()).map(move |j| (Some(i), Some(j))))
                    .collect()
            }
            (None, None) => Vec::new(),
        };
        let mut stack: Vec<Node> = Vec::new();
        for (i, j) in roots {
            stack.push(Node {
                a: i.and_then(|i| pa.as_ref().map(|v| piece(&v[i]))),
                b: j.and_then(|j| pb.as_ref().map(|v| piece(&v[j]))),
            });
            while let Some(n) = stack.pop() {
                self.count_visit()?;
                if !self.keep(&n) {
                    continue;
                }
                if let Some((side, dir)) = self.size_split(&n) {
                    stack.extend(Self::split_node(&n, side, dir).into_iter().rev());
                    continue;
                }
                match self.cone_check(&n) {
                    ConeCheck::Pass => self.candidate(&n, false)?,
                    // Normalen dort möglicherweise parallel, aber an dieser Stelle ist schon eine
                    // Berührung eingeordnet: nicht weiter teilen.
                    ConeCheck::Split(..) if self.touch_inside(&n) => {}
                    ConeCheck::Split(side, dir) => {
                        stack.extend(Self::split_node(&n, side, dir).into_iter().rev())
                    }
                    ConeCheck::Limit => self.candidate(&n, true)?,
                }
            }
        }
        Ok(())
    }

    /// Liegt ein eingeordneter Berührpunkt im (um seine Größe vergrößerten) Hüllquader des
    /// Knotens?
    fn touch_inside(&self, n: &Node) -> bool {
        if self.touches.is_empty() {
            return false;
        }
        let (_, _, lo, hi) = self.cand_start(n);
        let m = DVec3::splat((hi - lo).length());
        self.touches
            .iter()
            .any(|t| (lo - m).cmple(t.q).all() && t.q.cmple(hi + m).all())
    }

    /// Startpunkt eines Kandidaten (Mittel der Flickenmitten auf den Freiformseiten), Startwerte
    /// und Hüllquader.
    fn cand_start(&self, n: &Node) -> (DVec3, Hints, DVec3, DVec3) {
        let mut pts = Vec::new();
        let mut hints = Hints::default();
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for (piece, side, hint) in [(&n.a, &self.a, &mut hints.a), (&n.b, &self.b, &mut hints.b)] {
            if let (Some(p), Some(d)) = (piece, side.nurbs) {
                let p = &p.p;
                let uv = (0.5 * (p.u.0 + p.u.1), 0.5 * (p.v.0 + p.v.1));
                pts.push(d.point(uv.0, uv.1));
                *hint = Some(uv);
                let (l, h) = bounds(p);
                lo = lo.min(l);
                hi = hi.max(h);
            }
        }
        let start = pts.iter().copied().sum::<DVec3>() / pts.len().max(1) as f64;
        (start, hints, lo, hi)
    }

    /// Ein Kandidat. Angenommen (Normalen getrennt): Startpunkte von seinem Rand
    /// (`edge_seeds`, vollständig, siehe dort). Grenz-Kandidat (Normalen dort möglicherweise
    /// parallel, nicht weiter teilbar): Startpunkt aus der Mitte; nur ein Berührpunkt ist
    /// zulässig, sonst „nahezu tangential“ – nie wird ein Kandidat stillschweigend verworfen.
    fn candidate(&mut self, n: &Node, limit: bool) -> Result<(), String> {
        let (start, hints, lo, hi) = self.cand_start(n);
        let size = (hi - lo).length();
        self.candidates += 1;
        if self.candidates > MAX_CANDIDATES {
            return Err(format!(
                "Zu viele Schnittbereiche (mehr als {MAX_CANDIDATES} Kandidaten-Flicken)"
            ));
        }
        if !limit {
            return self.edge_seeds(n);
        }
        // Grenz-Kandidaten an einer schon eingeordneten Berührung (etwa längs einer Berührlinie
        // oder eines gemeinsamen Rands) brauchen keinen eigenen Startpunkt.
        let m = DVec3::splat(size);
        let reach = self.max_patch.min(self.diag) + size;
        let center = (lo + hi) * 0.5;
        if self.touches.iter().any(|t| {
            (lo - m).cmple(t.q).all() && t.q.cmple(hi + m).all() || t.q.distance(center) <= reach
        }) {
            self.candidates -= 1;
            return Ok(());
        }
        let r = self.refine_seed(start, hints)?;
        match r.found {
            Some(p) if p.cross().length() < PARALLEL_NORMALS => self.handle_seed(p),
            Some(p) => self.limit_transversal(p),
            None if r.residual <= NEAR_CONTACT_FACTOR * self.tol.linear => match r.last {
                Some(p) => self.handle_touch(p),
                None => Err(MSG_NEAR_TANGENT.into()),
            },
            None => Err(MSG_NEAR_TANGENT.into()),
        }
    }

    /// Neuer Startpunkt: Duplikate und Punkte auf verfolgten Kurven entfallen; sonst gezählt und
    /// verfolgt (bzw. als Berührpunkt eingeordnet).
    pub(super) fn handle_seed(&mut self, s: Probe) -> Result<(), String> {
        if self.starts.iter().any(|q| q.distance(s.q) <= self.same) {
            return Ok(());
        }
        let transversal = s.cross().length() >= PARALLEL_NORMALS;
        if transversal && self.on_traced(&s)? {
            return Ok(());
        }
        if !transversal {
            return self.handle_touch(s);
        }
        self.count_start(s.q)?;
        let br = self.trace(&s, self.starts.len())?;
        let len: f64 = br.pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        if len <= self.tol.linear || (br.closed && self.loop_is_touch(&br)?) {
            // Nur ein Punkt (etwa an einer Ecke) oder eine Schleife innerhalb der Toleranz: wie
            // eine Berührung.
            self.check_touch(&s)?;
            self.touches.push(s);
        } else {
            self.branches.push(br);
        }
        Ok(())
    }

    /// Grenz-Kandidat mit einem Schnittpunkt, an dem die Normalen nicht parallel sind: Eine
    /// kleine geschlossene Schleife innerhalb der Toleranz ist eine Berührung (wie bei
    /// analytischen Flächen), alles andere ist nicht sicher bestimmbar (es könnten weitere Äste
    /// in der Nähe der tangentialen Stelle liegen).
    fn limit_transversal(&mut self, p: Probe) -> Result<(), String> {
        if self.starts.iter().any(|q| q.distance(p.q) <= self.same) {
            return Ok(());
        }
        if self.on_traced(&p)? {
            return Err(MSG_NEAR_TANGENT.into());
        }
        self.count_start(p.q)?;
        let br = self.trace(&p, self.starts.len())?;
        if br.closed && self.loop_is_touch(&br)? {
            self.check_touch(&p)?;
            self.touches.push(p);
            return Ok(());
        }
        Err(MSG_NEAR_TANGENT.into())
    }

    /// Berührpunkt: Punkte nahe einem schon eingeordneten (innerhalb der Kandidatengröße, etwa
    /// längs einer Berührlinie) zählen nicht erneut.
    fn handle_touch(&mut self, s: Probe) -> Result<(), String> {
        if self
            .touches
            .iter()
            .any(|q| q.q.distance(s.q) <= self.max_patch.min(self.diag))
            || self.starts.iter().any(|q| q.distance(s.q) <= self.same)
        {
            return Ok(());
        }
        self.count_start(s.q)?;
        self.check_touch(&s)?;
        self.touches.push(s);
        Ok(())
    }

    fn count_start(&mut self, q: DVec3) -> Result<(), String> {
        self.starts.push(q);
        if self.starts.len() > MAX_SEEDS {
            return Err(msg_too_many_seeds());
        }
        Ok(())
    }

    /// Newton im Raum auf beide Flächen (kleinster Schritt, der beide linearisierten Gleichungen
    /// erfüllt; bei fast parallelen Normalen abwechselnde Projektion). Endet bei Konvergenz,
    /// Stillstand oder nach `SEED_NEWTON_STEPS`.
    pub(super) fn refine_seed(&self, start: DVec3, hints: Hints) -> Result<SeedResult, String> {
        let mut q = start;
        let mut hints = hints;
        let max_move = SEED_MAX_MOVE * self.diag.max(H_MIN);
        let mut history: Vec<f64> = Vec::new();
        let mut last: Option<Probe> = None;
        let mut best = f64::INFINITY;
        for k in 0..SEED_NEWTON_STEPS {
            let Some(pr) = self.probe(q, hints)? else {
                break;
            };
            hints = pr.hints();
            let res = pr.residual();
            if res < best {
                best = res;
                last = Some(pr);
            }
            history.push(best);
            if k >= STAGNATION_STEPS && best > STAGNATION_RATIO * history[k - STAGNATION_STEPS] {
                break;
            }
            let (fa, fb, ga, gb) = (pr.ea.f, pr.eb.f, pr.ea.g, pr.eb.g);
            let c = ga.dot(gb);
            let det = 1.0 - c * c;
            let delta = if det > SEED_DET_MIN {
                let al = (-fa + c * fb) / det;
                let be = (-fb + c * fa) / det;
                ga * al + gb * be
            } else {
                let q1 = q - ga * fa;
                match self.eval_side(&self.b, q1, hints.b)? {
                    Some(e) => q1 - e.g * e.f - q,
                    None => break,
                }
            };
            if !delta.is_finite() {
                return Err(MSG_NUMERIC.into());
            }
            let len = delta.length();
            if res <= self.eps && len <= self.eps {
                return Ok(SeedResult {
                    found: Some(pr),
                    last: Some(pr),
                    residual: res,
                });
            }
            q += if len > max_move {
                delta * (max_move / len)
            } else {
                delta
            };
        }
        if let Some(pr) = self.probe(q, hints)?
            && pr.residual() < best
        {
            best = pr.residual();
            last = Some(pr);
        }
        Ok(SeedResult {
            found: last.filter(|p| p.residual() <= self.eps),
            last,
            residual: best,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch() -> BezierPatch {
        // quadratisch × linear, rationale Gewichte (kein Produkt)
        BezierPatch {
            u: (0.0, 1.0),
            v: (0.0, 1.0),
            degree_u: 2,
            degree_v: 1,
            control: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(0.0, 1.0, 0.2),
                DVec3::new(1.0, 0.0, 1.0),
                DVec3::new(1.0, 1.0, 0.5),
                DVec3::new(2.0, 0.0, 0.0),
                DVec3::new(2.0, 1.0, 0.3),
            ],
            weights: vec![1.0, 2.0, 0.5, 1.0, 1.0, 3.0],
        }
    }

    fn eval(p: &BezierPatch, u: f64, v: f64) -> DVec3 {
        let b = |n: usize, i: usize, t: f64| {
            let c = (1..=i).fold(1.0, |c, k| c * (n + 1 - k) as f64 / k as f64);
            c * t.powi(i as i32) * (1.0 - t).powi((n - i) as i32)
        };
        let (mut num, mut den) = (DVec3::ZERO, 0.0);
        for i in 0..=p.degree_u {
            for j in 0..=p.degree_v {
                let (q, w) = at(p, i, j);
                let f = b(p.degree_u, i, u) * b(p.degree_v, j, v) * w;
                num += q * f;
                den += f;
            }
        }
        num / den
    }

    #[test]
    fn directional_split_keeps_the_surface() {
        let p = patch();
        for along_u in [true, false] {
            let [lo, hi] = split_dir(&p, along_u);
            for (s, t) in [(0.3, 0.7), (0.9, 0.1)] {
                let (a, b) = if along_u { (0.5 * s, t) } else { (s, 0.5 * t) };
                assert!(eval(&lo, s, t).distance(eval(&p, a, b)) < 1e-12);
                let (a, b) = if along_u {
                    (0.5 + 0.5 * s, t)
                } else {
                    (s, 0.5 + 0.5 * t)
                };
                assert!(eval(&hi, s, t).distance(eval(&p, a, b)) < 1e-12);
            }
        }
    }

    #[test]
    fn tangent_and_normal_cones_contain_the_true_directions() {
        let p = patch();
        let (du, dv) = (tangents(&p, true), tangents(&p, false));
        let cn = normal_cone_from(&du, &dv);
        let h = 1e-6;
        for i in 0..=10 {
            for j in 0..=10 {
                let (u, v) = (i as f64 / 10.0, j as f64 / 10.0);
                let su =
                    (eval(&p, (u + h).min(1.0), v) - eval(&p, (u - h).max(0.0), v)).normalize();
                let sv =
                    (eval(&p, u, (v + h).min(1.0)) - eval(&p, u, (v - h).max(0.0))).normalize();
                // in den Kegeln der Differenzvektoren (positive Hülle ⊂ Kegel)
                let cu = cone_of(&du).unwrap();
                let cv = cone_of(&dv).unwrap();
                assert!(su.dot(cu.axis).acos() <= cu.half + 1e-6);
                assert!(sv.dot(cv.axis).acos() <= cv.half + 1e-6);
                if let Some(c) = cn {
                    let n = su.cross(sv).normalize();
                    assert!(n.dot(c.axis).acos() <= c.half + 1e-6);
                }
            }
        }
    }
}
