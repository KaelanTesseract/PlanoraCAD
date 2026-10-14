//! Umriss-Striche gekrümmter Flächen: Linien, an denen die Fläche von der Blickrichtung aus
//! gesehen umkippt (`n · d = 0`). Exakt für Zylinder, Kegel und Kugel; Torus und Freiform folgen
//! (bis dahin eine benannte Meldung statt einer still falschen Zeichnung).
//!
//! Jeder Umriss wird auf das tatsächliche Flächengebiet beschnitten: Abtastung mit
//! `point_in_face` (Rand zählt als innen), Wechselstellen per Bisektion verfeinert. Wo ein Umriss
//! den Parameterbereich der Fläche nicht verlässt, sind seine Enden aus den Flächenparametern
//! gebaut und fallen so bis auf Rundung mit den Endpunkten der Randkanten zusammen.

use crate::DrawBody;
use crate::basis::Basis;
use crate::error::DrawError;
use crate::limits::{BISECT_TOL, MAX_STROKES, geom_tol};
use crate::stroke::{Stroke, StrokeOrigin};
use planora_exchange::Progress;
use planora_kernel::boolean::{Containment, FaceGeom, face_geom, loop_points, point_in_face};
use planora_kernel::{Curve, Surface};
use planora_math::{DVec3, Frame, TAU, Tolerance};
use std::f64::consts::FRAC_PI_2;

/// Abtastpunkte je Umrissstück für den Zuschnitt auf die Fläche.
const TRIM_SAMPLES: usize = 256;
/// Stützstellen der Vorzeichenwechsel-Suche über einen vollen Umlauf (Kegel).
const ROOT_SAMPLES: usize = 720;
/// Höchstzahl der Halbierungen (Bisektion endet spätestens an der Rechengenauigkeit).
const MAX_BISECT_STEPS: usize = 200;
/// Fenster für das Anlegen an die Randkante, in Abtastschritten zu jeder Seite.
const SNAP_WINDOW: f64 = 2.0;
/// Ein Zuschnittpunkt wird angelegt, wenn die Kante höchstens `SNAP_FACTOR · geom_tol` entfernt
/// ist (wie das Fenster, in dem isect Endpunkte auf Strichen erkennt).
const SNAP_FACTOR: f64 = 16.0;
/// `|n · d|` darunter gilt beim Kegel als null (Blick entlang der Achse bei Steigung 0).
const ZERO_NORMAL_DOT: f64 = 1e-12;
/// Erweiterung des aus den Randschleifen bestimmten Höhenbereichs (relativ).
const V_PAD_REL: f64 = 1e-2;
/// Blick entlang der Zylinderachse: `|a · d| ≥ 1 − AXIAL_EPS`.
const AXIAL_EPS: f64 = 1e-12;
/// Meldung für Flächen, deren Umriss noch nicht berechnet wird.
const UNSUPPORTED: &str = "Umriss von Torus/Freiform folgt";

/// Ein Umrisskandidat vor dem Zuschnitt: Kurve und Parameterbereich; `closed` bei Vollkreisen.
struct Candidate {
    curve: Curve,
    t0: f64,
    t1: f64,
    closed: bool,
}

/// Umriss-Striche aller Flächen aller Körper für die Blickrichtung `basis.d`.
///
/// Ebenen haben keinen Umriss. Torus- und Freiformflächen liefern vorerst
/// `DrawError::Unsupported`. `StrokeOrigin::Silhouette` trägt den Index der Fläche in
/// `solid.faces`.
pub fn silhouette_strokes(
    bodies: &[DrawBody],
    basis: &Basis,
    extent: f64,
    progress: &Progress,
) -> Result<Vec<Stroke>, DrawError> {
    let tol = Tolerance {
        linear: geom_tol(extent),
        ..Tolerance::default()
    };
    let d = basis.d;
    let mut out = Vec::new();
    for (bi, body) in bodies.iter().enumerate() {
        let solid = body.solid;
        for (fid, face) in solid.faces.iter() {
            progress.check()?;
            let surface = &face.surface;
            if matches!(surface, Surface::Plane { .. }) {
                continue;
            }
            if matches!(surface, Surface::Torus { .. } | Surface::Nurbs(_)) {
                return Err(DrawError::Unsupported(UNSUPPORTED.into()));
            }
            check_finite(surface)?;
            let fg = face_geom(solid, fid);
            let candidates = match surface {
                Surface::Cylinder { frame, radius } => {
                    cylinder_lines(&fg, frame, *radius, d, &tol)?
                }
                Surface::Cone { .. } => cone_lines(&fg, d, &tol)?,
                Surface::Sphere { frame, radius } => vec![great_circle(frame, *radius, d)],
                _ => Vec::new(),
            };
            for c in candidates {
                progress.check()?;
                for (t0, t1) in trim_to_face(&fg, &c, &tol, progress)? {
                    out.push(Stroke {
                        curve: c.curve.clone(),
                        t0,
                        t1,
                        origin: StrokeOrigin::Silhouette {
                            body: bi,
                            face: fid.index(),
                        },
                    });
                    if out.len() as u64 > MAX_STROKES {
                        return Err(DrawError::TooComplex(out.len() as u64));
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Rahmen und Maße der Fläche sind endliche Zahlen.
fn check_finite(surface: &Surface) -> Result<(), DrawError> {
    let f = surface.frame();
    let frame_ok = f.origin.is_finite() && f.x.is_finite() && f.y.is_finite() && f.z.is_finite();
    let dims_ok = match surface {
        Surface::Cylinder { radius, .. } | Surface::Sphere { radius, .. } => radius.is_finite(),
        Surface::Cone { radius, slope, .. } => radius.is_finite() && slope.is_finite(),
        _ => true,
    };
    if frame_ok && dims_ok {
        Ok(())
    } else {
        Err(DrawError::NonFinite)
    }
}

/// Höhenbereich `[v0, v1]` der Fläche: aus dem Parameterbereich, sonst aus den `v`-Werten der
/// Randschleifen, dann um `V_PAD_REL·(v1 − v0) + tol` erweitert, damit die Enden aus Zuschnitt
/// und Anlegen an die Kante entstehen und nie aus abgetasteten Extremwerten.
fn v_range(fg: &FaceGeom, tol: &Tolerance) -> Result<(f64, f64), DrawError> {
    let (v0, v1) = match fg.domain {
        Some(r) => (r.v0.min(r.v1), r.v0.max(r.v1)),
        None => {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for lp in &fg.loops {
                for p in loop_points(lp) {
                    let (_, v) = fg.surface.project(p);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
            let pad = V_PAD_REL * (hi - lo) + tol.linear;
            (lo - pad, hi + pad)
        }
    };
    if v0.is_finite() && v1.is_finite() {
        Ok((v0, v1))
    } else {
        Err(DrawError::NonFinite)
    }
}

/// Zylinder: Mantellinien bei `u = atan2(d·y, d·x) ± π/2` (dort ist die Normale
/// `cos u·x + sin u·y` senkrecht zu `d`), Parameter = Höhe `v`. Blick entlang der Achse: keine.
fn cylinder_lines(
    fg: &FaceGeom,
    frame: &Frame,
    radius: f64,
    d: DVec3,
    tol: &Tolerance,
) -> Result<Vec<Candidate>, DrawError> {
    let a = frame.z;
    if a.dot(d).abs() >= 1.0 - AXIAL_EPS {
        return Ok(Vec::new());
    }
    let (v0, v1) = v_range(fg, tol)?;
    let phi = d.dot(frame.y).atan2(d.dot(frame.x));
    let lines = [phi + FRAC_PI_2, phi - FRAC_PI_2]
        .into_iter()
        .map(|u| {
            let n = frame.x * u.cos() + frame.y * u.sin();
            Candidate {
                curve: Curve::Line {
                    origin: frame.origin + n * radius,
                    dir: a,
                },
                t0: v0,
                t1: v1,
                closed: false,
            }
        })
        .collect();
    Ok(lines)
}

/// Kegel: Winkel `u` mit `normal(u, v)·d = 0` (Kernelnormale; sie hängt bei Kegeln nicht von `v`
/// ab und wird in der Mitte des Höhenbereichs ausgewertet, also nicht an der Spitze), gefunden
/// über Vorzeichenwechsel an `ROOT_SAMPLES` Stützstellen und Bisektion. Mantellinie von
/// `P(u, v0)` nach `P(u, v1)`, Parameter = Länge.
fn cone_lines(fg: &FaceGeom, d: DVec3, tol: &Tolerance) -> Result<Vec<Candidate>, DrawError> {
    let (v0, v1) = v_range(fg, tol)?;
    let vm = 0.5 * (v0 + v1);
    let s = &fg.surface;
    let f = |u: f64| s.normal(u, vm).dot(d);
    let us: Vec<f64> = (0..ROOT_SAMPLES)
        .map(|i| TAU * i as f64 / ROOT_SAMPLES as f64)
        .collect();
    let fs: Vec<f64> = us.iter().map(|&u| f(u)).collect();
    if fs.iter().any(|x| !x.is_finite()) {
        return Err(DrawError::NonFinite);
    }
    // Steigung 0 und Blick entlang der Achse: n·d ≡ 0, die Fläche erscheint als Kreis (wie beim
    // Zylinder), es gibt keine Umrisslinie.
    if fs.iter().all(|x| x.abs() <= ZERO_NORMAL_DOT) {
        return Ok(Vec::new());
    }
    let mut roots = Vec::new();
    for i in 0..ROOT_SAMPLES {
        let (ua, fa) = (us[i], fs[i]);
        let ub = if i + 1 == ROOT_SAMPLES {
            TAU
        } else {
            us[i + 1]
        };
        let fb = fs[(i + 1) % ROOT_SAMPLES];
        if fa == 0.0 {
            roots.push(ua);
        } else if fa * fb < 0.0 {
            roots.push(bisect_root(&f, ua, fa, ub));
        }
    }
    let mut out = Vec::new();
    for u in roots {
        let p0 = s.point(u, v0);
        let p1 = s.point(u, v1);
        let len = p0.distance(p1);
        if !len.is_finite() {
            return Err(DrawError::NonFinite);
        }
        if len <= tol.linear {
            continue;
        }
        out.push(Candidate {
            curve: Curve::Line {
                origin: p0,
                dir: (p1 - p0) / len,
            },
            t0: 0.0,
            t1: len,
            closed: false,
        });
    }
    Ok(out)
}

/// Nullstelle von `f` in `[lo, hi]` mit `f(lo) = f_lo` und Vorzeichenwechsel, bis zur
/// Rechengenauigkeit halbiert.
fn bisect_root(f: &impl Fn(f64) -> f64, mut lo: f64, mut f_lo: f64, mut hi: f64) -> f64 {
    for _ in 0..MAX_BISECT_STEPS {
        let mid = 0.5 * (lo + hi);
        if mid <= lo || mid >= hi {
            break;
        }
        let fm = f(mid);
        if fm == 0.0 {
            return mid;
        }
        if (fm > 0.0) == (f_lo > 0.0) {
            lo = mid;
            f_lo = fm;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Kugel: Großkreis senkrecht zu `d` (Rahmen `e1 ⟂ d`, `e2 = d × e1`, `z = d`).
fn great_circle(frame: &Frame, radius: f64, d: DVec3) -> Candidate {
    let helper = if d.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let e1 = (helper - d * helper.dot(d)).normalize();
    let e2 = d.cross(e1);
    Candidate {
        curve: Curve::Circle {
            frame: Frame {
                origin: frame.origin,
                x: e1,
                y: e2,
                z: d,
            },
            radius,
        },
        t0: 0.0,
        t1: TAU,
        closed: true,
    }
}

/// Teilbereiche von `c`, die in der Fläche liegen (`Inside` oder `Boundary`). Abtastung an
/// `TRIM_SAMPLES` Stellen, Wechselstellen per Bisektion (mindestens auf `BISECT_TOL` der
/// Parameterbreite, weiter bis zur Rechengenauigkeit, damit die Enden auf dem Rand liegen).
/// Vollkreise: ganz innen bleibt `[t0, t1]`; sonst Bögen, die über `t1` hinausreichen dürfen.
fn trim_to_face(
    fg: &FaceGeom,
    c: &Candidate,
    tol: &Tolerance,
    progress: &Progress,
) -> Result<Vec<(f64, f64)>, DrawError> {
    let w = c.t1 - c.t0;
    let inside = |t: f64| point_in_face(fg, c.curve.point(t), tol) != Containment::Outside;
    // Offene Kurven: Stellen 0..N−1 einschließlich beider Enden; Vollkreise: N Stellen im Umlauf.
    let steps = if c.closed {
        TRIM_SAMPLES
    } else {
        TRIM_SAMPLES - 1
    };
    let t_at = |i: usize| {
        if !c.closed && i == steps {
            c.t1
        } else {
            c.t0 + w * i as f64 / steps as f64
        }
    };
    let flags: Vec<bool> = (0..TRIM_SAMPLES).map(|i| inside(t_at(i))).collect();
    if flags.iter().all(|&f| f) {
        return Ok(vec![(c.t0, c.t1)]);
    }
    if flags.iter().all(|&f| !f) {
        return Ok(Vec::new());
    }
    // Folge der Stellen (Parameter, innen?). Vollkreise: ab einer äußeren Stelle einmal herum,
    // so dass jeder innere Abschnitt beidseitig von äußeren Stellen begrenzt ist.
    let seq: Vec<(f64, bool)> = if c.closed {
        let k = flags.iter().position(|&f| !f).unwrap_or(0);
        (0..=TRIM_SAMPLES)
            .map(|m| {
                (
                    c.t0 + w * (k + m) as f64 / TRIM_SAMPLES as f64,
                    flags[(k + m) % TRIM_SAMPLES],
                )
            })
            .collect()
    } else {
        (0..TRIM_SAMPLES).map(|i| (t_at(i), flags[i])).collect()
    };
    let refine = |t_in: f64, t_out: f64| {
        let (mut a, mut b) = (t_in, t_out);
        for _ in 0..MAX_BISECT_STEPS {
            let mid = 0.5 * (a + b);
            if mid == a || mid == b {
                break;
            }
            if inside(mid) {
                a = mid;
            } else {
                b = mid;
            }
        }
        debug_assert!((b - a).abs() <= BISECT_TOL * w.abs());
        snap_to_edge(
            fg,
            &c.curve,
            a,
            SNAP_WINDOW * w.abs() / steps as f64,
            tol.linear,
        )
        .map_or(a, |t| if c.closed { t } else { t.clamp(c.t0, c.t1) })
    };
    let mut out = Vec::new();
    let mut start: Option<f64> = None;
    for j in 0..seq.len() {
        let (t, is_in) = seq[j];
        if is_in && start.is_none() {
            start = Some(if j == 0 { t } else { refine(t, seq[j - 1].0) });
        }
        let ends_here = is_in && (j + 1 == seq.len() || !seq[j + 1].1);
        if ends_here {
            progress.check()?;
            let a = start.take().unwrap_or(t);
            let b = if j + 1 == seq.len() {
                t
            } else {
                refine(t, seq[j + 1].0)
            };
            // Bögen, die ganz jenseits des Umlaufs beginnen, um eine Periode zurückschieben.
            let (a, b) = if c.closed && a >= c.t1 {
                (a - w, b - w)
            } else {
                (a, b)
            };
            let mid = c.curve.point(0.5 * (a + b));
            let (pa, pb) = (c.curve.point(a), c.curve.point(b));
            if pa.distance(mid).max(mid.distance(pb)) > tol.linear {
                out.push((a, b));
            }
        }
    }
    Ok(out)
}

/// Legt einen Zuschnittpunkt `t_star` genau auf die Randkante, die der Umriss dort kreuzt.
///
/// `point_in_face` meldet `Boundary` nur bis `tol` neben einer Kante; sonst entscheidet das
/// Randpolygon im Parameterraum (5°-Abtastung), und die Bisektion endet auf dessen Sehne – bei
/// im `(u, v)`-Raum gekrümmten Kanten (Ellipse eines schrägen Schnitts) bis ≈ 1e-3·r neben der
/// wahren Kante. Deshalb wird je Randstück der Abstand `|P(t) − Kante|` im Fenster
/// `[t_star − h, t_star + h]` minimiert (Goldener Schnitt) und das kleinste Minimum genommen,
/// wenn es höchstens `SNAP_FACTOR · tol` beträgt. Randstücke, an denen der Umriss schon am
/// Fensterrand anliegt (er läuft entlang einer Kante, etwa einer Naht), zählen nicht: Dort ist
/// der Bisektionspunkt bereits auf `tol` genau. Ohne Treffer (schleifender Schnitt): `None`.
fn snap_to_edge(fg: &FaceGeom, curve: &Curve, t_star: f64, h: f64, tol: f64) -> Option<f64> {
    let limit = SNAP_FACTOR * tol;
    let (lo, hi) = (t_star - h, t_star + h);
    let mut best: Option<(f64, f64)> = None;
    for seg in fg.loops.iter().flatten() {
        let dist = |t: f64| seg.closest(curve.point(t)).1;
        if dist(lo) <= limit || dist(hi) <= limit {
            continue;
        }
        let (t, d) = golden_min(lo, hi, dist);
        if d <= limit && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((t, d));
        }
    }
    best.map(|(t, _)| t)
}

/// Minimum einer unimodalen Funktion auf `[a, b]` (Goldener Schnitt bis zur Rechengenauigkeit):
/// `(Stelle, Wert)`.
fn golden_min(mut a: f64, mut b: f64, f: impl Fn(f64) -> f64) -> (f64, f64) {
    let g = 0.5 * (5f64.sqrt() - 1.0);
    let mut x1 = b - g * (b - a);
    let mut x2 = a + g * (b - a);
    let (mut f1, mut f2) = (f(x1), f(x2));
    for _ in 0..MAX_BISECT_STEPS {
        if !(a < x1 && x1 < x2 && x2 < b) {
            break;
        }
        if f1 <= f2 {
            b = x2;
            x2 = x1;
            f2 = f1;
            x1 = b - g * (b - a);
            f1 = f(x1);
        } else {
            a = x1;
            x1 = x2;
            f1 = f2;
            x2 = a + g * (b - a);
            f2 = f(x2);
        }
    }
    if f1 <= f2 { (x1, f1) } else { (x2, f2) }
}
