//! Strahl gegen Flächen: alle Treffer von `origin + s·dir` mit den Flächen aller Körper, je
//! Treffer die Fläche und ob der Punkt im Inneren der beschnittenen Fläche oder auf ihrem Rand
//! liegt.
//!
//! Schnitte: Ebene (linear), Zylinder, Kegel, Kugel (quadratisch im lokalen Rahmen der Fläche),
//! Torus (Vorzeichenwechsel der impliziten Funktion auf 2048 Stützstellen, Bisektion; Berührung
//! ohne Vorzeichenwechsel ist unsicher), Freiform (`ray_hits_checked` des Kernels).
//!
//! Einordnung je Schnittpunkt wie `point_in_face` des Kernels mit
//! `Tolerance { linear: 16·geom_tol(E) }`: Randabstand höchstens `linear` → `Boundary`, sonst
//! Windungszahl der Randpolygone im Parameterraum → `Inside` oder verworfen. Die Randpolygone
//! berechnet [`Scene::new`] einmal je Fläche (`point_in_face` baut sie bei jedem Aufruf neu); der
//! Test `hit_classes_agree_with_point_in_face` prüft die Gleichheit mit dem Kernel.
//!
//! Unsichere Strahlen (`Ray::Unsafe`): Strahl in der Ebene einer Planfläche, Strahl berührt
//! einen Zylinder, Kegel oder eine Kugel (Doppelwurzel) oder liegt in ihrer Mantelfläche,
//! Strahl berührt einen Torus, Freiformschnitt ohne verlässliche Entscheidung. Jede dieser
//! Prüfungen gilt nur für Flächen, deren (aufgeblähte) Hüllbox der Strahl durchläuft.

use crate::DrawBody;
use crate::error::DrawError;
use crate::limits::geom_tol;
use planora_exchange::Progress;
use planora_kernel::boolean::{FaceGeom, Seg, face_geom, loop_points, points_to_uv, winding};
use planora_kernel::{Curve, Surface};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use std::f64::consts::PI;

/// Einordnungs- und Hüllbox-Toleranz in Vielfachen von `geom_tol(E)`.
const TOL_FACTOR: f64 = 16.0;
/// `|n · dir|` darunter: Strahl parallel zur Ebene.
const PLANE_PARALLEL: f64 = 1e-9;
/// Abstand des Strahls von der Ebene (relativ zu `E`), unter dem er in der Ebene liegt.
const IN_PLANE_REL: f64 = 1e-6;
/// Doppelwurzel einer Quadrik: `|b'² − a·c| < TANGENT_DISC_REL · E²`.
const TANGENT_DISC_REL: f64 = 1e-12;
/// `|a|` darunter (Richtung normiert): Strahl parallel zu den Mantellinien.
const QUAD_A_EPS: f64 = 1e-12;
/// `|b'|` darunter (relativ zu `E`): Strahl ohne Querung der Mantelfläche.
const QUAD_B_REL: f64 = 1e-9;
/// Abstand des Strahls von der Mantelfläche (relativ zu `E`), unter dem er in ihr liegt.
const IN_SURFACE_REL: f64 = 1e-6;
/// Stützstellen der Vorzeichenwechsel-Suche am Torus.
const TORUS_SAMPLES: usize = 2048;
/// Berührung des Torus: `min |F| < TORUS_TOUCH_REL · minor²` ohne Vorzeichenwechsel.
const TORUS_TOUCH_REL: f64 = 1e-9;
/// Höchstzahl an Halbierungen (endet früher an der Rechengenauigkeit).
const BISECT_STEPS: usize = 200;
/// Schritte des Goldenen Schnitts beim Suchen des kleinsten `|F|` zwischen zwei Stützstellen.
const GOLDEN_STEPS: usize = 80;

/// Lage eines Treffers auf der beschnittenen Fläche.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitClass {
    Inside,
    Boundary,
}

/// Ein Treffer: Abstand `s` entlang des (normierten) Strahls, Körperindex, Flächenindex (Index
/// in `solid.faces`) und Lage auf der Fläche.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub s: f64,
    pub body: usize,
    pub face: usize,
    pub class: HitClass,
}

/// Ergebnis eines Strahls: alle Treffer nach `s` sortiert, oder unsicher mit kurzem Grund.
#[derive(Clone, Debug)]
pub enum Ray {
    Hits(Vec<Hit>),
    Unsafe(&'static str),
}

/// Etwas, das Strahlen prüfen kann (die Sichtprüfung nimmt es als Schnittstelle).
pub trait Caster {
    /// Wie [`Scene::cast`].
    fn cast(&self, origin: DVec3, dir: DVec3, min_s: f64) -> Ray;
}

/// Schnittparameter `s` und, wenn bekannt (Freiform), die Flächenparameter `(u, v)`.
type Candidate = (f64, Option<(f64, f64)>);

/// Eine Fläche, vorbereitet für viele Strahlen.
struct FaceData {
    body: usize,
    face: usize,
    geom: FaceGeom,
    /// Randschleifen im Parameterraum (wie `point_in_face` sie bildet).
    polys: Vec<Vec<DVec2>>,
    /// Hüllbox der Fläche, aufgebläht.
    lo: DVec3,
    hi: DVec3,
}

/// Alle Flächen aller Körper, vorbereitet für viele Strahlen (FaceGeom, Hüllbox, Randpolygone
/// einmal berechnet).
pub struct Scene {
    faces: Vec<FaceData>,
    /// Größter Strahlabstand: Durchmesser der Hüllbox aller Körper plus `extent`.
    reach: f64,
    extent: f64,
    tol: Tolerance,
}

impl Scene {
    /// Bereitet alle Flächen vor. Zählt keine Strahltests (das macht die Sichtprüfung).
    pub fn new(bodies: &[DrawBody], extent: f64, progress: &Progress) -> Result<Scene, DrawError> {
        let extent = if extent.is_finite() {
            extent.max(1e-9)
        } else {
            return Err(DrawError::NonFinite);
        };
        let pad = TOL_FACTOR * geom_tol(extent);
        let plane_pad = pad.max(IN_PLANE_REL * extent);
        let mut faces = Vec::new();
        let mut all_lo = DVec3::splat(f64::INFINITY);
        let mut all_hi = DVec3::splat(f64::NEG_INFINITY);
        for (bi, body) in bodies.iter().enumerate() {
            let solid = body.solid;
            for (fid, _) in solid.faces.iter() {
                progress.check()?;
                let geom = face_geom(solid, fid);
                check_finite(&geom.surface)?;
                let (lo, hi) = face_bbox(&geom);
                if !(lo.is_finite() && hi.is_finite()) {
                    return Err(DrawError::NonFinite);
                }
                all_lo = all_lo.min(lo);
                all_hi = all_hi.max(hi);
                let polys = geom
                    .loops
                    .iter()
                    .map(|lp| points_to_uv(&geom.surface, geom.same_sense, &loop_points(lp)))
                    .collect();
                let p = if matches!(geom.surface, Surface::Plane { .. }) {
                    plane_pad
                } else {
                    pad
                };
                faces.push(FaceData {
                    body: bi,
                    face: fid.index(),
                    geom,
                    polys,
                    lo: lo - DVec3::splat(p),
                    hi: hi + DVec3::splat(p),
                });
            }
        }
        let diameter = if faces.is_empty() {
            0.0
        } else {
            (all_hi - all_lo).length()
        };
        Ok(Scene {
            faces,
            reach: diameter + extent,
            extent,
            tol: Tolerance {
                linear: TOL_FACTOR * geom_tol(extent),
                ..Tolerance::default()
            },
        })
    }

    /// Alle Treffer von `origin + s·dir` mit `min_s < s ≤ reach` (dir wird normiert, `s` ist
    /// also eine Länge). Treffer auf dem Rand einer Fläche heißen `Boundary`.
    /// `Err(AmbiguousVisibility)` wird NICHT hier erzeugt; unsichere Strahlen liefern
    /// `Ray::Unsafe`.
    pub fn cast(&self, origin: DVec3, dir: DVec3, min_s: f64) -> Ray {
        let len = dir.length();
        if !(origin.is_finite() && len.is_finite() && len > 0.0 && !min_s.is_nan()) {
            return Ray::Unsafe("ungültiger Strahl");
        }
        let d = dir / len;
        let mut out = Vec::new();
        if min_s >= self.reach {
            return Ray::Hits(out);
        }
        for f in &self.faces {
            let Some((sa, sb)) = ray_box(origin, d, min_s, self.reach, f.lo, f.hi) else {
                continue;
            };
            let cands = match self.face_hits(f, origin, d, min_s, (sa, sb)) {
                Ok(c) => c,
                Err(why) => return Ray::Unsafe(why),
            };
            for (s, uv) in cands {
                if !(s.is_finite() && s > min_s && s <= self.reach) {
                    continue;
                }
                let p = origin + d * s;
                let uv = uv.unwrap_or_else(|| f.geom.surface.project(p));
                if let Some(class) = self.classify(f, p, uv) {
                    out.push(Hit {
                        s,
                        body: f.body,
                        face: f.face,
                        class,
                    });
                }
            }
        }
        out.sort_by(|a, b| a.s.total_cmp(&b.s));
        Ray::Hits(out)
    }

    /// Schnittparameter (und, wenn bekannt, Flächenparameter) des Strahls mit der Trägerfläche;
    /// `Err` mit Grund, wenn der Strahl unsicher ist.
    fn face_hits(
        &self,
        f: &FaceData,
        o: DVec3,
        d: DVec3,
        min_s: f64,
        (sa, sb): (f64, f64),
    ) -> Result<Vec<Candidate>, &'static str> {
        let e = self.extent;
        let roots = match &f.geom.surface {
            Surface::Plane { frame } => {
                let nd = frame.z.dot(d);
                let dist = frame.z.dot(o - frame.origin);
                if nd.abs() < PLANE_PARALLEL {
                    if dist.abs() < IN_PLANE_REL * e {
                        return Err("Strahl liegt in einer ebenen Fläche");
                    }
                    return Ok(Vec::new());
                }
                vec![-dist / nd]
            }
            Surface::Cylinder { frame, radius } => {
                let (lo, ld) = local(frame, o, d);
                let a = ld.x * ld.x + ld.y * ld.y;
                let b = lo.x * ld.x + lo.y * ld.y;
                let c = lo.x * lo.x + lo.y * lo.y - radius * radius;
                quadratic(a, b, c, e, min_s, self.reach, || {
                    f.geom.surface.implicit(o).abs()
                })?
            }
            Surface::Cone {
                frame,
                radius,
                slope,
            } => {
                let (lo, ld) = local(frame, o, d);
                let k = *slope;
                let w = radius + k * lo.z;
                let a = ld.x * ld.x + ld.y * ld.y - k * k * ld.z * ld.z;
                let b = lo.x * ld.x + lo.y * ld.y - k * w * ld.z;
                let c = lo.x * lo.x + lo.y * lo.y - w * w;
                // Wurzeln auf dem Gegenast des Doppelkegels (Radius < 0, jenseits der Spitze)
                // haben eine Höhe außerhalb der Fläche und fallen bei der Einordnung heraus.
                quadratic(a, b, c, e, min_s, self.reach, || {
                    f.geom.surface.implicit(o).abs()
                })?
            }
            Surface::Sphere { frame, radius } => {
                let oc = o - frame.origin;
                quadratic(
                    1.0,
                    oc.dot(d),
                    oc.length_squared() - radius * radius,
                    e,
                    min_s,
                    self.reach,
                    || f.geom.surface.implicit(o).abs(),
                )?
            }
            Surface::Torus { minor, .. } => {
                let lo_s = sa.max(min_s);
                let hi_s = sb.min(self.reach);
                torus_roots(&f.geom.surface, *minor, o, d, lo_s, hi_s, min_s)?
            }
            Surface::Nurbs(n) => {
                let start = min_s;
                let Some(h) = n.ray_hits_checked(o + d * start, d, self.reach - start) else {
                    return Err("Freiformfläche nicht sicher entscheidbar");
                };
                // Rückgabe des Kernels: (t, u, v), t entlang des verschobenen Strahls.
                return Ok(h
                    .into_iter()
                    .map(|(t, u, v)| (t + start, Some((u, v))))
                    .collect());
            }
        };
        Ok(roots.into_iter().map(|s| (s, None)).collect())
    }

    /// Lage von `p` (Parameter `uv`) auf der beschnittenen Fläche, wie `point_in_face` des
    /// Kernels mit vorab berechneten Randpolygonen; `None` für außerhalb.
    fn classify(&self, f: &FaceData, p: DVec3, (u, v): (f64, f64)) -> Option<HitClass> {
        if f.geom
            .loops
            .iter()
            .flatten()
            .any(|s| s.closest(p).1 <= self.tol.linear)
        {
            return Some(HitClass::Boundary);
        }
        let shifts = |period: Option<f64>| match period {
            Some(t) => vec![-t, 0.0, t],
            None => vec![0.0],
        };
        let us = shifts(f.geom.surface.u_period());
        let vs = shifts(f.geom.surface.v_period());
        let mut total = 0;
        for poly in &f.polys {
            for du in &us {
                for dv in &vs {
                    total += winding(poly, DVec2::new(u + du, v + dv));
                }
            }
        }
        let sense = if f.geom.same_sense { 1 } else { -1 };
        (total * sense >= 1).then_some(HitClass::Inside)
    }
}

impl Caster for Scene {
    fn cast(&self, origin: DVec3, dir: DVec3, min_s: f64) -> Ray {
        Scene::cast(self, origin, dir, min_s)
    }
}

/// Strahlpunkt und Richtung im lokalen Rahmen.
fn local(frame: &Frame, o: DVec3, d: DVec3) -> (DVec3, DVec3) {
    (
        frame.to_local(o),
        DVec3::new(d.dot(frame.x), d.dot(frame.y), d.dot(frame.z)),
    )
}

/// Wurzeln von `a·s² + 2b·s + c = 0` (Richtung normiert). Doppelwurzel (`|b² − a·c| <
/// 1e-12·E²`) bei `min_s < s ≤ reach` und Strahl in der Mantelfläche (`a ≈ 0`, `b ≈ 0`,
/// Abstand `dist()` < 1e-6·E) sind unsicher.
fn quadratic(
    a: f64,
    b: f64,
    c: f64,
    e: f64,
    min_s: f64,
    reach: f64,
    dist: impl Fn() -> f64,
) -> Result<Vec<f64>, &'static str> {
    if a.abs() < QUAD_A_EPS {
        if b.abs() < QUAD_B_REL * e {
            if dist() < IN_SURFACE_REL * e {
                return Err("Strahl liegt in einer gekrümmten Fläche");
            }
            return Ok(Vec::new());
        }
        // Die zweite Wurzel liegt bei |s| ≳ 2·|b|/|a|, weit hinter `reach`.
        return Ok(vec![-c / (2.0 * b)]);
    }
    let disc = b * b - a * c;
    if disc.abs() < TANGENT_DISC_REL * e * e {
        let s0 = -b / a;
        if s0 > min_s && s0 <= reach {
            return Err("Strahl streift eine gekrümmte Fläche");
        }
        return Ok(Vec::new());
    }
    if disc < 0.0 {
        return Ok(Vec::new());
    }
    let q = -(b + b.signum() * disc.sqrt());
    if q == 0.0 {
        return Ok(vec![0.0]);
    }
    Ok(vec![q / a, c / q])
}

/// Schnitte des Strahls mit dem Torus für `s ∈ [lo, hi]`: Vorzeichenwechsel von
/// `F(s) = implicit(o + s·d)` auf `TORUS_SAMPLES` Stützstellen, Bisektion. Zwischen zwei
/// Stützstellen gleichen Vorzeichens, zwischen denen `|F|` die Null erreichen könnte (`F` ist
/// ein Abstand, ändert sich also höchstens um die Schrittweite), sucht der Goldene Schnitt das
/// kleinste `|F|`: Wechselt dort das Vorzeichen, sind es zwei Schnitte; bleibt `min |F|` unter
/// `1e-9·minor²`, berührt der Strahl den Torus (unsicher).
fn torus_roots(
    surf: &Surface,
    minor: f64,
    o: DVec3,
    d: DVec3,
    lo: f64,
    hi: f64,
    min_s: f64,
) -> Result<Vec<f64>, &'static str> {
    if hi <= lo {
        return Ok(Vec::new());
    }
    let f = |s: f64| surf.implicit(o + d * s);
    let touch = TORUS_TOUCH_REL * minor * minor;
    let n = TORUS_SAMPLES;
    let h = (hi - lo) / n as f64;
    let ss: Vec<f64> = (0..=n).map(|i| lo + h * i as f64).collect();
    let fs: Vec<f64> = ss.iter().map(|&s| f(s)).collect();
    let mut roots = Vec::new();
    for i in 0..=n {
        if fs[i] == 0.0 {
            // Stützstelle genau auf der Fläche: Berührung oder Querung?
            let before = if i > 0 { fs[i - 1] } else { fs[i] };
            let after = if i < n { fs[i + 1] } else { fs[i] };
            if before * after >= 0.0 && ss[i] > min_s {
                return Err("Strahl berührt Torus");
            }
            roots.push(ss[i]);
            continue;
        }
        if i == n {
            break;
        }
        let (a, b) = (fs[i], fs[i + 1]);
        if b == 0.0 {
            continue;
        }
        if a * b < 0.0 {
            roots.push(bisect(&f, ss[i], ss[i + 1], a));
            continue;
        }
        // Gleiches Vorzeichen: kann |F| dazwischen null werden?
        if (a.abs() + b.abs() - h) / 2.0 > touch {
            continue;
        }
        let sign = a.signum();
        let g = |s: f64| sign * f(s);
        let (sm, gm) = golden_min(&g, ss[i], ss[i + 1]);
        if gm.abs() < touch {
            if sm > min_s {
                return Err("Strahl berührt Torus");
            }
            continue;
        }
        if gm < 0.0 {
            roots.push(bisect(&f, ss[i], sm, a));
            roots.push(bisect(&f, sm, ss[i + 1], sign * gm));
        }
    }
    Ok(roots)
}

/// Nullstelle von `f` in `[a, b]` mit `f(a)` vom Vorzeichen `fa`, `f(b)` vom anderen.
fn bisect(f: &impl Fn(f64) -> f64, mut a: f64, mut b: f64, fa: f64) -> f64 {
    for _ in 0..BISECT_STEPS {
        let m = 0.5 * (a + b);
        if m <= a.min(b) || m >= a.max(b) {
            break;
        }
        let fm = f(m);
        if fm == 0.0 {
            return m;
        }
        if (fm < 0.0) == (fa < 0.0) {
            a = m;
        } else {
            b = m;
        }
    }
    0.5 * (a + b)
}

/// Kleinster Wert von `g` in `[a, b]` (Goldener Schnitt), mit den Randwerten verglichen.
fn golden_min(g: &impl Fn(f64) -> f64, a: f64, b: f64) -> (f64, f64) {
    let r = 0.5 * (5f64.sqrt() - 1.0);
    let (mut a, mut b) = (a, b);
    let mut x1 = b - r * (b - a);
    let mut x2 = a + r * (b - a);
    let (mut g1, mut g2) = (g(x1), g(x2));
    for _ in 0..GOLDEN_STEPS {
        if g1 < g2 {
            b = x2;
            x2 = x1;
            g2 = g1;
            x1 = b - r * (b - a);
            g1 = g(x1);
        } else {
            a = x1;
            x1 = x2;
            g1 = g2;
            x2 = a + r * (b - a);
            g2 = g(x2);
        }
    }
    let m = 0.5 * (a + b);
    [(m, g(m)), (x1, g1), (x2, g2)]
        .into_iter()
        .min_by(|p, q| p.1.total_cmp(&q.1))
        .expect("drei Werte")
}

/// Parameterintervall des Strahls `o + s·d`, `s ∈ [s0, s1]`, in der Box `[lo, hi]`.
fn ray_box(o: DVec3, d: DVec3, s0: f64, s1: f64, lo: DVec3, hi: DVec3) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (s0, s1);
    for k in 0..3 {
        if d[k] == 0.0 {
            if o[k] < lo[k] || o[k] > hi[k] {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo[k] - o[k]) / d[k], (hi[k] - o[k]) / d[k]);
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, t1))
}

/// Hüllbox der Fläche (nicht aufgebläht). Ebene, Zylinder, Kegel sind Regelflächen: Jede
/// Mantellinie durch einen Flächenpunkt verlässt die Fläche zu beiden Seiten über den Rand, der
/// Punkt liegt also zwischen zwei Randpunkten – die Hüllbox des Randes enthält die Fläche. Kugel
/// und Torus: Hüllbox der ganzen Fläche. Freiform: Hüllbox der Kontrollpunkte.
fn face_bbox(g: &FaceGeom) -> (DVec3, DVec3) {
    match &g.surface {
        Surface::Plane { .. } | Surface::Cylinder { .. } | Surface::Cone { .. } => {
            let mut lo = DVec3::splat(f64::INFINITY);
            let mut hi = DVec3::splat(f64::NEG_INFINITY);
            for s in g.loops.iter().flatten() {
                let (a, b) = seg_bbox(s);
                lo = lo.min(a);
                hi = hi.max(b);
            }
            (lo, hi)
        }
        Surface::Sphere { frame, radius } => (
            frame.origin - DVec3::splat(*radius),
            frame.origin + DVec3::splat(*radius),
        ),
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            let z = frame.z.normalize();
            let half = DVec3::new(
                major * (1.0 - z.x * z.x).max(0.0).sqrt() + minor,
                major * (1.0 - z.y * z.y).max(0.0).sqrt() + minor,
                major * (1.0 - z.z * z.z).max(0.0).sqrt() + minor,
            );
            (frame.origin - half, frame.origin + half)
        }
        Surface::Nurbs(n) => n.control_bounds(),
    }
}

/// Hüllbox eines Kurvenstücks: Geraden ihre Enden, Kreis und Ellipse dazu die Achsextrema im
/// Bereich, Freiformkurven ihre Kontrollpunkte.
fn seg_bbox(s: &Seg) -> (DVec3, DVec3) {
    let (mut lo, mut hi) = (s.start().min(s.end()), s.start().max(s.end()));
    let conic = match &s.curve {
        Curve::Circle { frame, radius } => Some((frame.x * *radius, frame.y * *radius)),
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => Some((frame.x * *major, frame.y * *minor)),
        _ => None,
    };
    if let Some((ax, ay)) = conic {
        let (a, b) = (s.t0.min(s.t1), s.t0.max(s.t1));
        for k in 0..3 {
            let t_star = ay[k].atan2(ax[k]);
            let k0 = ((a - t_star) / PI).ceil() as i64;
            let k1 = ((b - t_star) / PI).floor() as i64;
            for m in k0..=k1 {
                let p = s.curve.point(t_star + m as f64 * PI);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    if let Curve::Nurbs(c) = &s.curve {
        for &p in c.control_points() {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    (lo, hi)
}

/// Alle Zahlen der Trägerfläche endlich?
fn check_finite(s: &Surface) -> Result<(), DrawError> {
    let fr = s.frame();
    let frame_ok =
        fr.origin.is_finite() && fr.x.is_finite() && fr.y.is_finite() && fr.z.is_finite();
    let nums_ok = match s {
        Surface::Plane { .. } | Surface::Nurbs(_) => true,
        Surface::Cylinder { radius, .. } | Surface::Sphere { radius, .. } => radius.is_finite(),
        Surface::Cone { radius, slope, .. } => radius.is_finite() && slope.is_finite(),
        Surface::Torus { major, minor, .. } => major.is_finite() && minor.is_finite(),
    };
    if frame_ok && nums_ok {
        Ok(())
    } else {
        Err(DrawError::NonFinite)
    }
}
