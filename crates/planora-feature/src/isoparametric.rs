//! Isoparametrische Kurve (Skizzen 11b, Spec 6, Entscheidung 4): die Parameterlinie `u = const`
//! oder `v = const` einer Körperfläche als Kurven der Skizze. Die Herkunft ist
//! [`crate::CopySource::Isoparametric`]; gespeichert wird der Anteil 0–1 am Parameterbereich der
//! Berandung ([`iso_range`]), damit die Kurve bei Flächenänderung an derselben
//! verhältnismäßigen Stelle bleibt.
//!
//! Rechnung ([`iso_curves`]):
//! 1. Bereich: Hüllrechteck der Randpolygone in `(u, v)`; deckt die Fläche in einer geschlossenen
//!    Richtung den ganzen Umlauf, gilt der volle Umlauf ab der Naht.
//! 2. Die Linie beim Wert `lo + value · (hi − lo)` läuft über den ganzen Bereich der anderen
//!    Richtung und wird mit den Randpolygonen geschnitten (auch um eine Periode verschoben).
//!    Jeder Schnitt wird auf der echten Randkante nachgeschärft (Halbieren).
//! 3. Abschnitte, deren Mitte in der Fläche oder auf dem Rand liegt, werden Stücke;
//!    aneinanderstoßende Abschnitte bleiben ein Stück. Über die Naht einer geschlossenen Richtung
//!    läuft ein Stück durch: Es wird nur an echten Randschnitten geteilt, nie an der Naht
//!    (Prüf-Schwerpunkt 5).
//! 4. Ausgabe exakt: Ebene → Linie; Zylinder/Kegel `u = const` → Linie, `v = const` →
//!    Bogen/Kreis; Kugel und Torus → Bogen/Kreis; Freiformfläche → fester Spline aus
//!    [`NurbsSurface::iso_curve_u`](planora_math::nurbs::NurbsSurface::iso_curve_u) bzw. `_v`,
//!    auf das Stück beschnitten. Stücke mit Länge ≤ Toleranz entfallen.

use crate::refs::{FaceRef, resolve_face};
use crate::surface_project::{DocView, ProjectError};
use planora_kernel::boolean::{Containment, FaceGeom, face_geom, face_polys, point_in_face_at};
use planora_kernel::{Curve, Solid, Surface};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::helpers::{Included, include_curve};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Richtung der Isokurve: `U` ist die Kurve `u = const`, `V` die Kurve `v = const`
/// (**Annahme**, in `offene-punkte.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsoDirection {
    U,
    V,
}

/// Parameterbereich der Berandung einer Fläche (Spec-Entscheidung 4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsoRange {
    pub u: (f64, f64),
    pub v: (f64, f64),
}

/// Eine Ausdehnung ab diesem Bruchteil der Periode gilt als voller Umlauf.
const FULL_TURN: f64 = 1.0 - 1e-6;
/// Halbe Breite (Bruchteil des Randstücks) der Suche beim Nachschärfen eines Schnitts.
const REFINE_REACH: f64 = 0.05;
/// Abschnitte der Vorzeichensuche beim Nachschärfen.
const REFINE_STEPS: usize = 8;
/// Halbierungen beim Nachschärfen.
const REFINE_BISECTIONS: usize = 60;
/// Größte Abweichung eines nachgeschärften Schnitts vom Polygonpunkt, bezogen auf die 3D-Länge
/// der geschnittenen Polygonkante. Die Randpolygone tasten Bögen alle 5° ab; die Sehnenhöhe ist
/// dann `tan(5°/4) ≈ 0,022` der Sehne. Das Doppelte lässt auch schräge Schnitte (Weg entlang der
/// Linie = Sehnenhöhe / sin Schnittwinkel, bis etwa 25°) zu.
const CHORD_DEVIATION: f64 = 0.05;
/// Abtastpunkte für die Länge eines Stücks.
const LENGTH_SAMPLES: usize = 16;

/// Parameterbereich der Berandung von `face`: Hüllrechteck der Randpolygone in `(u, v)`; ist die
/// Fläche in einer Richtung geschlossen und deckt sie den ganzen Umlauf, gilt der volle Umlauf
/// ab der Naht (Anfang der äußeren Schleife).
pub fn iso_range(face: &FaceGeom) -> IsoRange {
    range_of(face, &aligned_polys(face, face_polys(face)))
}

/// Anteil 0–1 des Punkts `p` (auf der Fläche) am Bereich [`iso_range`] in der Richtung `dir`
/// (bei `U` der u-Wert, bei `V` der v-Wert).
pub fn iso_value_at(face: &FaceGeom, p: DVec3, dir: IsoDirection) -> f64 {
    let range = iso_range(face);
    let (u, v) = face.surface.project(p);
    let (x, (lo, hi), period) = match dir {
        IsoDirection::U => (u, range.u, face.surface.u_period()),
        IsoDirection::V => (v, range.v, face.surface.v_period()),
    };
    let x = match period {
        Some(t) => unwrap_near(x, 0.5 * (lo + hi), t),
        None => x,
    };
    if hi - lo <= 0.0 || !x.is_finite() {
        return 0.0;
    }
    ((x - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// Die Isokurven der Fläche `face` in der Richtung `dir` beim Anteil `value`, in
/// Skizzenkoordinaten von `view.frame`, in Parameterrichtung geordnet. Fehler: Die Fläche ist
/// verloren ([`ProjectError::Lost`]), kein Stück bleibt ([`ProjectError::NoHit`]) oder die Zeit
/// `budget` reicht nicht ([`ProjectError::TooSlow`]).
pub fn iso_curves(
    view: &DocView<'_>,
    face: &FaceRef,
    dir: IsoDirection,
    value: f64,
    tol: &Tolerance,
    budget: Duration,
) -> Result<Vec<Included>, ProjectError> {
    let start = Instant::now();
    let doc = view.doc;
    let visible = doc.visible_at(view.at);
    let solids: Vec<&Solid> = visible.iter().filter_map(|b| doc.body(*b)).collect();
    let found = resolve_face(&solids, face, tol).map_err(|_| ProjectError::Lost)?;
    let solid = solids[found.body];
    solid.faces.get(found.face).ok_or(ProjectError::Lost)?;
    if !value.is_finite() {
        return Err(ProjectError::NoHit);
    }
    let geom = face_geom(solid, found.face);
    let polys = aligned_polys(&geom, face_polys(&geom));
    let range = range_of(&geom, &polys);
    let line = IsoLine::new(&geom, dir, range, value.clamp(0.0, 1.0));
    let over_time = || start.elapsed() > budget;
    if over_time() {
        return Err(ProjectError::TooSlow);
    }
    let spans = line.spans(&polys, tol, &over_time)?;
    let mut out = Vec::new();
    for (t0, t1) in spans {
        if let Some(g) = line.piece(t0, t1, &view.frame, tol) {
            out.push(g);
        }
    }
    if over_time() {
        return Err(ProjectError::TooSlow);
    }
    if out.is_empty() {
        Err(ProjectError::NoHit)
    } else {
        Ok(out)
    }
}

/// Wert `x + k·period`, der `reference` am nächsten liegt.
fn unwrap_near(x: f64, reference: f64, period: f64) -> f64 {
    x + period * ((reference - x) / period).round()
}

/// Abstand `x` auf den kürzesten Weg modulo `period` (ohne Periode unverändert).
fn wrap(x: f64, period: Option<f64>) -> f64 {
    match period {
        Some(t) => x - t * (x / t).round(),
        None => x,
    }
}

/// Hüllrechteck `(lo, hi)` von Punkten (leer: unendlich).
fn bounds(pts: &mut dyn Iterator<Item = DVec2>) -> (DVec2, DVec2) {
    pts.fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(p), hi.max(p)),
    )
}

/// Legt die inneren Schleifen (Löcher) in die Periode der äußeren: Jede Schleife wird in den
/// geschlossenen Richtungen um ganze Perioden so verschoben, dass die Mitte ihres Hüllrechtecks
/// der Mitte der äußeren Hülle am nächsten liegt. Jede Schleife ist für sich ab `project`
/// entfaltet (Winkel in (−π, π]); ohne das Verschieben kann ein Loch eine Periode neben der
/// äußeren Schleife liegen und das Hüllrechteck aufblähen (Korrekturrunde 1, I1).
fn aligned_polys(face: &FaceGeom, mut polys: Vec<Vec<DVec2>>) -> Vec<Vec<DVec2>> {
    let Some(outer) = polys.first() else {
        return polys;
    };
    let (olo, ohi) = bounds(&mut outer.iter().copied());
    if !(olo.is_finite() && ohi.is_finite()) {
        return polys;
    }
    let mid = 0.5 * (olo + ohi);
    let (pu, pv) = (face.surface.u_period(), face.surface.v_period());
    for poly in polys.iter_mut().skip(1) {
        let (lo, hi) = bounds(&mut poly.iter().copied());
        if !(lo.is_finite() && hi.is_finite()) {
            continue;
        }
        let c = 0.5 * (lo + hi);
        let shift = |x: f64, to: f64, period: Option<f64>| {
            period.map_or(0.0, |t| unwrap_near(x, to, t) - x)
        };
        let d = DVec2::new(shift(c.x, mid.x, pu), shift(c.y, mid.y, pv));
        if d != DVec2::ZERO {
            for p in poly.iter_mut() {
                *p += d;
            }
        }
    }
    polys
}

/// Bereich aus den (mit [`aligned_polys`] ausgerichteten) Randpolygonen.
fn range_of(face: &FaceGeom, polys: &[Vec<DVec2>]) -> IsoRange {
    let (mut lo, mut hi) = bounds(&mut polys.iter().flatten().copied());
    if !(lo.is_finite() && hi.is_finite()) {
        let r = face.domain;
        return IsoRange {
            u: r.map_or((0.0, 0.0), |r| (r.u0, r.u1)),
            v: r.map_or((0.0, 0.0), |r| (r.v0, r.v1)),
        };
    }
    // Voller Umlauf ab der Naht: Anfang der äußeren Schleife (ein Loch über der Naht kann
    // auch nach dem Ausrichten etwas über sie hinausragen).
    let (outer_lo, _) = bounds(&mut polys[0].iter().copied());
    let full = |period: Option<f64>, lo: &mut f64, hi: &mut f64, start: f64| {
        if let Some(t) = period
            && *hi - *lo >= t * FULL_TURN
        {
            *lo = if start.is_finite() { start } else { *lo };
            *hi = *lo + t;
        }
    };
    full(face.surface.u_period(), &mut lo.x, &mut hi.x, outer_lo.x);
    full(face.surface.v_period(), &mut lo.y, &mut hi.y, outer_lo.y);
    IsoRange {
        u: (lo.x, hi.x),
        v: (lo.y, hi.y),
    }
}

/// Die Linie `fixed = c` über den Bereich `(a, b)` der anderen Richtung.
struct IsoLine<'a> {
    geom: &'a FaceGeom,
    dir: IsoDirection,
    c: f64,
    a: f64,
    b: f64,
    /// Periode der festen Richtung.
    fixed_period: Option<f64>,
    /// Periode der Laufrichtung, wenn die Fläche darin den vollen Umlauf deckt.
    run_period: Option<f64>,
}

impl<'a> IsoLine<'a> {
    fn new(geom: &'a FaceGeom, dir: IsoDirection, range: IsoRange, value: f64) -> Self {
        let s = &geom.surface;
        let (fixed, run, fixed_period, period) = match dir {
            IsoDirection::U => (range.u, range.v, s.u_period(), s.v_period()),
            IsoDirection::V => (range.v, range.u, s.v_period(), s.u_period()),
        };
        let run_period = period.filter(|t| run.1 - run.0 >= t * FULL_TURN);
        IsoLine {
            geom,
            dir,
            c: fixed.0 + value * (fixed.1 - fixed.0),
            a: run.0,
            b: run.1,
            fixed_period,
            run_period,
        }
    }

    /// `(u, v)` des Linienpunkts beim Laufparameter `t`.
    fn uv(&self, t: f64) -> (f64, f64) {
        match self.dir {
            IsoDirection::U => (self.c, t),
            IsoDirection::V => (t, self.c),
        }
    }

    /// (feste, laufende) Koordinate eines Punkts in `(u, v)`.
    fn split(&self, p: DVec2) -> (f64, f64) {
        match self.dir {
            IsoDirection::U => (p.x, p.y),
            IsoDirection::V => (p.y, p.x),
        }
    }

    /// Abschnitte `(t0, t1)` der Linie in der Fläche, in Laufrichtung.
    fn spans(
        &self,
        polys: &[Vec<DVec2>],
        tol: &Tolerance,
        over_time: &dyn Fn() -> bool,
    ) -> Result<Vec<(f64, f64)>, ProjectError> {
        let (a, b) = (self.a, self.b);
        let eps = 1e-12 * (b - a).abs().max(1.0);
        // (Polygonwert, nachgeschärfter Wert) je Schnitt, nach dem Polygonwert geordnet;
        // gleiche Polygonwerte (gemeinsame Ecken zweier Kanten) zählen einmal.
        let mut found: Vec<(f64, f64)> = Vec::new();
        for (t, chord) in self.crossings(polys) {
            if over_time() {
                return Err(ProjectError::TooSlow);
            }
            found.push((t, self.refine(t, chord, tol)));
        }
        found.sort_by(|x, y| x.0.total_cmp(&y.0));
        found.dedup_by(|x, y| (x.0 - y.0).abs() <= eps);
        // Das Nachschärfen darf getrennte Schnitte nicht zusammenlegen oder vertauschen (sonst
        // verschmölzen Stücke über ein Loch): Dann gelten für beide die Polygonwerte.
        let mut refined: Vec<bool> = vec![true; found.len()];
        for i in 1..found.len() {
            if found[i].1 - found[i - 1].1 <= eps {
                refined[i] = false;
                refined[i - 1] = false;
            }
        }
        let mut cuts = vec![a, b];
        for (&(raw, sharp), ok) in found.iter().zip(&refined) {
            let t = if *ok { sharp } else { raw };
            if t > a && t < b {
                cuts.push(t);
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|x, y| (*x - *y).abs() <= eps);
        let mut spans: Vec<(f64, f64)> = Vec::new();
        for w in cuts.windows(2) {
            let (t0, t1) = (w[0], w[1]);
            if t1 - t0 <= eps {
                continue;
            }
            let uv = self.uv(0.5 * (t0 + t1));
            let p = self.geom.surface.point(uv.0, uv.1);
            if point_in_face_at(self.geom, polys, p, uv, tol) == Containment::Outside {
                continue;
            }
            // Aneinanderstoßende Abschnitte bleiben ein Stück.
            match spans.last_mut() {
                Some(last) if (last.1 - t0).abs() <= eps => last.1 = t1,
                _ => spans.push((t0, t1)),
            }
        }
        // Über die Naht läuft ein Stück durch: das letzte (bis zum Bereichsende) und das erste
        // (ab dem Bereichsanfang) sind eins.
        if let Some(t) = self.run_period
            && spans.len() > 1
            && (spans[0].0 - a).abs() <= eps
            && (spans[spans.len() - 1].1 - b).abs() <= eps
        {
            let first = spans.remove(0);
            if let Some(last) = spans.last_mut() {
                last.1 = first.1 + t;
            }
        }
        Ok(spans)
    }

    /// Schnitte der Linie mit den Randpolygonen (auch um eine Periode verschoben) als
    /// Laufparameter, ungeordnet, je mit der 3D-Länge der geschnittenen Polygonkante.
    fn crossings(&self, polys: &[Vec<DVec2>]) -> Vec<(f64, f64)> {
        let s = &self.geom.surface;
        let shifts = |period: Option<f64>| match period {
            Some(t) => vec![-t, 0.0, t],
            None => vec![0.0],
        };
        let (us, vs) = (shifts(s.u_period()), shifts(s.v_period()));
        let mut out = Vec::new();
        for poly in polys {
            let n = poly.len();
            for i in 0..n {
                let (p, q) = (poly[i], poly[(i + 1) % n]);
                for du in &us {
                    for dv in &vs {
                        let d = DVec2::new(*du, *dv);
                        let ((pf, pr), (qf, qr)) = (self.split(p + d), self.split(q + d));
                        let (sp, sq) = (pf - self.c, qf - self.c);
                        let chord = || {
                            let (a, b) = (p + d, q + d);
                            s.point(a.x, a.y).distance(s.point(b.x, b.y))
                        };
                        if sp == 0.0 && sq == 0.0 {
                            // Randstück entlang der Linie: beide Enden.
                            out.push((pr, chord()));
                            out.push((qr, chord()));
                        } else if (sp <= 0.0 && sq >= 0.0) || (sp >= 0.0 && sq <= 0.0) {
                            out.push((pr + (qr - pr) * sp / (sp - sq), chord()));
                        }
                    }
                }
            }
        }
        out.retain(|(t, _)| t.is_finite() && *t >= self.a && *t <= self.b);
        out
    }

    /// Schärft den Schnitt beim Laufparameter `t` (aus dem Randpolygon) auf der nächsten echten
    /// Randkante nach: Halbieren über die feste Koordinate des Fußpunkts. Findet sich in der Nähe
    /// kein Vorzeichenwechsel oder liegt der neue Punkt weiter vom Polygonpunkt ab, als die
    /// Abtastung der geschnittenen Polygonkante (3D-Länge `chord`) abweichen kann, bleibt `t`.
    fn refine(&self, t: f64, chord: f64, tol: &Tolerance) -> f64 {
        let s = &self.geom.surface;
        let (u, v) = self.uv(t);
        let x = s.point(u, v);
        let Some((seg, (f0, _))) = self
            .geom
            .loops
            .iter()
            .flatten()
            .map(|seg| (seg, seg.closest(x)))
            .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
        else {
            return t;
        };
        let g = |f: f64| {
            let (pu, pv) = s.project(seg.at(f));
            let (fixed, _) = self.split(DVec2::new(pu, pv));
            wrap(fixed - self.c, self.fixed_period)
        };
        let (lo, hi) = ((f0 - REFINE_REACH).max(0.0), (f0 + REFINE_REACH).min(1.0));
        // Vorzeichenwechsel, der `f0` am nächsten liegt.
        let mut best: Option<(f64, f64)> = None;
        let mut prev = (lo, g(lo));
        for k in 1..=REFINE_STEPS {
            let f = lo + (hi - lo) * k as f64 / REFINE_STEPS as f64;
            let gf = g(f);
            if prev.1 == 0.0 || prev.1 * gf < 0.0 || gf == 0.0 {
                let mid = 0.5 * (prev.0 + f);
                if best.is_none_or(|(a, b)| (mid - f0).abs() < (0.5 * (a + b) - f0).abs()) {
                    best = Some((prev.0, f));
                }
            }
            prev = (f, gf);
        }
        let Some((mut fa, mut fb)) = best else {
            return t;
        };
        let mut ga = g(fa);
        for _ in 0..REFINE_BISECTIONS {
            let m = 0.5 * (fa + fb);
            let gm = g(m);
            if gm == 0.0 {
                fa = m;
                fb = m;
                break;
            }
            if (ga < 0.0) == (gm < 0.0) {
                fa = m;
                ga = gm;
            } else {
                fb = m;
            }
        }
        let (pu, pv) = s.project(seg.at(0.5 * (fa + fb)));
        let (_, run) = self.split(DVec2::new(pu, pv));
        let period = match self.dir {
            IsoDirection::U => s.v_period(),
            IsoDirection::V => s.u_period(),
        };
        let run = period.map_or(run, |p| unwrap_near(run, t, p));
        // Nur ein naher Treffer zählt (sonst war es eine andere Kante): höchstens die
        // Sehnenabweichung der Abtastung (Korrekturrunde 1, M1).
        if !run.is_finite() {
            return t;
        }
        let (ru, rv) = self.uv(run);
        let gap = s.point(ru, rv).distance(x);
        if gap <= (chord * CHORD_DEVIATION).max(tol.linear) {
            run
        } else {
            t
        }
    }

    /// Das Stück `t0 … t1` als Kurve in Skizzenkoordinaten; `None`, wenn es kürzer als die
    /// Toleranz ist (Pol, Kegelspitze) oder sich nicht bilden lässt.
    fn piece(&self, t0: f64, t1: f64, frame: &Frame, tol: &Tolerance) -> Option<Included> {
        let (curve, scale, (t0, t1)) = self.curve(t0, t1)?;
        let (p0, p1) = (t0 * scale, t1 * scale);
        let mut len = 0.0;
        let mut prev = curve.point(p0);
        for k in 1..=LENGTH_SAMPLES {
            let p = curve.point(p0 + (p1 - p0) * k as f64 / LENGTH_SAMPLES as f64);
            len += p.distance(prev);
            prev = p;
        }
        if len <= tol.linear {
            return None;
        }
        include_curve(frame, &curve, p0, p1).ok()
    }

    /// Kernkurve der Linie: Kurve, Maßstab (Kurvenparameter = Maßstab · Laufparameter) und der
    /// (bei Freiformflächen in den Bereich gebrachte) Laufbereich.
    fn curve(&self, t0: f64, t1: f64) -> Option<(Curve, f64, (f64, f64))> {
        let (c, u_dir) = (self.c, self.dir == IsoDirection::U);
        let circle = |origin: DVec3, x: DVec3, y: DVec3, radius: f64| {
            // Negativer Radius: dieselben Punkte mit gedrehten Achsen.
            let (x, y) = if radius < 0.0 { (-x, -y) } else { (x, y) };
            (radius.abs() > 0.0).then(|| Curve::Circle {
                frame: Frame {
                    origin,
                    x,
                    y,
                    z: x.cross(y),
                },
                radius: radius.abs(),
            })
        };
        let radial = |f: &Frame, u: f64| f.x * u.cos() + f.y * u.sin();
        let plain = |curve: Option<Curve>| curve.map(|k| (k, 1.0, (t0, t1)));
        match &self.geom.surface {
            Surface::Plane { frame: f } => plain(Some(if u_dir {
                Curve::Line {
                    origin: f.origin + f.x * c,
                    dir: f.y,
                }
            } else {
                Curve::Line {
                    origin: f.origin + f.y * c,
                    dir: f.x,
                }
            })),
            Surface::Cylinder { frame: f, radius } => plain(if u_dir {
                Some(Curve::Line {
                    origin: f.origin + radial(f, c) * *radius,
                    dir: f.z,
                })
            } else {
                circle(f.origin + f.z * c, f.x, f.y, *radius)
            }),
            Surface::Cone {
                frame: f,
                radius,
                slope,
            } => {
                if u_dir {
                    let d = radial(f, c) * *slope + f.z;
                    let len = d.length();
                    let line = Curve::Line {
                        origin: f.origin + radial(f, c) * *radius,
                        dir: d / len,
                    };
                    Some((line, len, (t0, t1)))
                } else {
                    plain(circle(f.origin + f.z * c, f.x, f.y, radius + slope * c))
                }
            }
            Surface::Sphere { frame: f, radius } => plain(if u_dir {
                circle(f.origin, radial(f, c), f.z, *radius)
            } else {
                circle(
                    f.origin + f.z * (radius * c.sin()),
                    f.x,
                    f.y,
                    radius * c.cos(),
                )
            }),
            Surface::Torus {
                frame: f,
                major,
                minor,
            } => plain(if u_dir {
                circle(f.origin + radial(f, c) * *major, radial(f, c), f.z, *minor)
            } else {
                circle(
                    f.origin + f.z * (minor * c.sin()),
                    f.x,
                    f.y,
                    major + minor * c.cos(),
                )
            }),
            Surface::Nurbs(d) => {
                let s = d.surface();
                let ((u0, u1), (v0, v1)) = s.domain();
                let fold = |x: f64, (lo, hi): (f64, f64), period: Option<f64>| match period {
                    Some(p) if x < lo || x > hi => lo + (x - lo).rem_euclid(p),
                    _ => x.clamp(lo, hi),
                };
                let su = self.geom.surface.u_period();
                let sv = self.geom.surface.v_period();
                let (fixed, run, run_p) = if u_dir {
                    (fold(c, (u0, u1), su), (v0, v1), sv)
                } else {
                    (fold(c, (v0, v1), sv), (u0, u1), su)
                };
                let iso = if u_dir {
                    s.iso_curve_u(fixed)
                } else {
                    s.iso_curve_v(fixed)
                }
                .ok()?;
                // Ein offener Laufbereich wird auf den Bereich begrenzt; ein geschlossener wird
                // beim Beschneiden gefaltet (auch über die Naht).
                let range = match run_p {
                    Some(_) => (t0, t1),
                    None => (t0.clamp(run.0, run.1), t1.clamp(run.0, run.1)),
                };
                Some((Curve::Nurbs(Arc::new(iso)), 1.0, range))
            }
        }
    }
}
