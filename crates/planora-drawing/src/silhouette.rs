//! Umriss-Striche gekrümmter Flächen: Linien, an denen die Fläche von der Blickrichtung aus
//! gesehen umkippt (`n · d = 0`). Exakt für Zylinder, Kegel und Kugel (Gerade, Kreis); Torus und
//! Freiformflächen als angepasste Splines (`Curve::Nurbs`, höchstens `SILHOUETTE_TOL` neben dem
//! wahren Umriss).
//!
//! Jeder Umriss wird auf das tatsächliche Flächengebiet beschnitten: Abtastung mit
//! `point_in_face` (Rand zählt als innen), Wechselstellen per Bisektion verfeinert und auf die
//! Randkante gelegt (siehe [`snap_to_edge`]). Wo ein Umriss den Parameterbereich der Fläche nicht
//! verlässt, sind seine Enden aus den Flächenparametern gebaut und fallen so bis auf Rundung mit
//! den Endpunkten der Randkanten zusammen.
//!
//! **Splines (Torus, Freiform):** Der wahre Umriss wird als Weg `t ↦ Punkt` dicht abgetastet,
//! **vor** der Anpassung beschnitten, und jedes Stück wird einzeln angepasst
//! (`NurbsCurve::fit` hält Anfang und Ende fest; reichen die zulässigen Kontrollpunkte nicht, in
//! Teilen mit gemeinsamen Endpunkten). Die Enden eines Stücks an einer Randkante sind
//! damit Punkte **der Kante selbst** (Nächstpunkt aus [`snap_to_edge`]), so dass die
//! Schnittsuche dort teilt. Jede Anpassung wird gegen den dichten Weg geprüft: Seine Sehnen
//! weichen höchstens `SILHOUETTE_TOL/8` vom wahren Umriss ab, die angepasste Kurve höchstens
//! `7/8·SILHOUETTE_TOL` von den Sehnen, zusammen also höchstens `SILHOUETTE_TOL`.
//!
//! - **Torus (Festlegung F5):** parametrisch in `u`: `v₁(u) = atan2(−(d_x cos u + d_y sin u),
//!   d_z)`, `v₂ = v₁ + π` im Rahmen des Torus; Blick in der Ringebene (`|d_z| ≤ 1e-9`): die
//!   Kreise `v = ±π/2` und die zwei Meridiankreise senkrecht zur Blickrichtung (exakte Kreise).
//! - **Freiform:** `g(u, v) = n(u, v)·d` auf einem Raster (64 × 64, je Bézier-Spanne mindestens 8
//!   Teile, höchstens 256 je Richtung), Vorzeichenwechsel an Rasterkanten (Bisektion), Verkettung
//!   nach dem Marching-Squares-Verfahren (Sattelzellen über den Zellmittelwert), über die Naht
//!   geschlossener Richtungen hinweg; Zwischenpunkte per Newton auf `g = 0` in Richtung `∇g`.
//!   Erreicht der Umriss eine Stelle ohne Normale (Pol), ist das
//!   `DrawError::Unsupported("Umriss an einem Pol der Freiformfläche")`.

use crate::DrawBody;
use crate::basis::Basis;
use crate::error::DrawError;
use crate::limits::{BISECT_TOL, MAX_OUTLINE_POINTS, MAX_RAW_STROKES, SILHOUETTE_TOL, geom_tol};
use crate::raycast::Region;
use crate::stroke::{Stroke, StrokeOrigin};
use planora_exchange::Progress;
use planora_kernel::boolean::{Containment, FaceGeom, face_geom, loop_points, point_in_face};
use planora_kernel::{Curve, NurbsSurfaceData, Surface};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DVec2, DVec3, Frame, TAU, Tolerance};
use std::f64::consts::{FRAC_PI_2, PI};

/// Gleichmäßige Abtastpunkte je Umrissstück für den Zuschnitt auf die Fläche (dazu kommen die
/// Stellen aus dem Flächenrand, siehe [`boundary_params`]).
const TRIM_SAMPLES: usize = 256;
/// Höchstzahl der Abtaststellen eines Umrissstücks (gleichmäßige und randgeführte zusammen);
/// darüber `TooComplex`, nie stilles Ausdünnen.
const MAX_TRIM_SAMPLES: usize = 65_536;
/// Mindestzahl der Stützstellen je Randstück für die randgeführten Abtaststellen.
const BOUNDARY_SEG_SAMPLES: usize = 16;
/// Stützstellen je Vollkreis bei gekrümmten Randstücken (alle 5°, wie die Randpolygone).
const BOUNDARY_SAMPLES_PER_TURN: f64 = 72.0;
/// Freiformkanten: Stützstellen je Randstück.
const BOUNDARY_NURBS_SAMPLES: usize = 64;
/// Stützstellen der Vorzeichenwechsel-Suche über einen vollen Umlauf (Kegel).
const ROOT_SAMPLES: usize = 720;
/// Höchstzahl der Halbierungen (Bisektion endet spätestens an der Rechengenauigkeit).
const MAX_BISECT_STEPS: usize = 200;
/// Fenster für das Anlegen an die Randkante, in Abtastschritten zu jeder Seite.
const SNAP_WINDOW: f64 = 2.0;
/// Ein Zuschnittpunkt wird angelegt, wenn die Kante höchstens `SNAP_FACTOR · geom_tol` entfernt
/// ist (wie das Fenster, in dem isect Endpunkte auf Strichen erkennt).
const SNAP_FACTOR: f64 = 16.0;
/// Probestellen im Fenster beim Anlegen an die Randkante (je Randstück; jedes örtliche Minimum
/// wird verfeinert).
const SNAP_PROBES: usize = 64;
/// Reserve für die Krümmung des Umrisses im Fenster (Bogen gegen Sehne).
const SNAP_REACH: f64 = 2.0;
/// `|n · d|` darunter gilt beim Kegel als null (Blick entlang der Achse bei Steigung 0).
const ZERO_NORMAL_DOT: f64 = 1e-12;
/// Erweiterung des aus den Randschleifen bestimmten Höhenbereichs (relativ).
const V_PAD_REL: f64 = 1e-2;
/// Blick entlang der Zylinderachse: `|a · d| ≥ 1 − AXIAL_EPS`.
const AXIAL_EPS: f64 = 1e-12;
/// Einordnungstoleranz der Freiform-Zuschnitte in Vielfachen von `geom_tol` (wie die
/// Strahlprüfung).
const REGION_FACTOR: f64 = 16.0;
/// Torus: `|d_z| ≤ TORUS_FLAT` (Blick in der Ringebene) → Kreise statt Splines.
const TORUS_FLAT: f64 = 1e-9;
/// Torus: Stützstellen je Zweig vor der Verfeinerung (721 Punkte).
const TORUS_SAMPLES: usize = 720;
/// Höchstzahl der Verdopplungen der Stützstellen (Torus) bzw. Halbierungen je Kettenglied
/// (Freiform).
const MAX_DOUBLINGS: usize = 8;
/// Größte Abweichung der Sehnenmitte vom wahren Umriss im dichten Weg (Anteil an
/// `SILHOUETTE_TOL`).
const SAG_SHARE: f64 = 1.0 / 8.0;
/// Größter Abstand der angepassten Kurve von den Sehnen des dichten Wegs (Anteil an
/// `SILHOUETTE_TOL`); zusammen mit `SAG_SHARE` höchstens `SILHOUETTE_TOL`.
const FIT_SHARE: f64 = 7.0 / 8.0;
/// Anpassungsversuche: Schranke der Anpassung `SILHOUETTE_TOL/2`, je Versuch halbiert.
const FIT_ATTEMPTS: usize = 2;
/// Höchstzahl der Halbierungen einer Punktfolge, die sich nicht als Ganzes anpassen lässt.
const FIT_SPLITS: usize = 8;
/// Prüfstellen der angepassten Kurve je Sehne des dichten Wegs.
const FIT_CHECKS_PER_CHORD: usize = 4;
/// Suchfenster (Sehnen zu jeder Seite) beim Prüfen der angepassten Kurve.
const FIT_CHECK_WINDOW: usize = 16;
/// Freiform: Rasterteile je Richtung (mindestens) und je Bézier-Spanne (mindestens), höchstens.
const GRID_MIN: usize = 64;
const GRID_PER_SPAN: usize = 8;
const GRID_MAX: usize = 256;
/// Freiform: Newton-Schritte, Zielgenauigkeit auf `g = 0` und größtes zulässiges `|g|` danach.
const NEWTON_STEPS: usize = 8;
const NEWTON_G: f64 = 1e-12;
const NEWTON_ACCEPT: f64 = 1e-9;
/// Freiform: `|g|` darunter gilt an Rasterpunkten als 0 (Vorzeichen +); sind alle Rasterwerte so
/// klein, wird die Fläche nur von der Seite gesehen und hat keinen Umriss.
const ZERO_G: f64 = 1e-12;
/// Meldung, wenn ein Freiform-Umriss eine Stelle ohne Normale erreicht.
pub const POLE_MESSAGE: &str = "Umriss an einem Pol der Freiformfläche";
/// Meldung, wenn auch das feinste Raster einen Vorzeichenwechsel von g nur zwischen seinen
/// Punkten zeigt (Umrissschleife kleiner als eine Rasterzelle).
pub const HIDDEN_MESSAGE: &str = "Umriss der Freiformfläche feiner als das Suchraster";
/// Meldung, wenn eine Anpassung die Genauigkeit nicht erreicht.
const FIT_MESSAGE: &str = "Umriss lässt sich nicht genau genug annähern";

/// Ein Umrisskandidat vor dem Zuschnitt: Kurve und Parameterbereich; `closed` bei Vollkreisen.
struct Candidate {
    curve: Curve,
    t0: f64,
    t1: f64,
    closed: bool,
}

/// Umriss-Striche aller Flächen aller Körper für die Blickrichtung `basis.d`.
///
/// Ebenen haben keinen Umriss. Zylinder, Kegel und Kugel liefern Geraden und Kreise, Torus und
/// Freiformflächen angepasste Splines (siehe Modulbeschreibung). `StrokeOrigin::Silhouette` trägt
/// den Index der Fläche in `solid.faces`.
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
            // Aufwand je Fläche (die Striche insgesamt begrenzen MAX_RAW_STROKES und MAX_STROKES).
            let mut work = Work::default();
            let surface = &face.surface;
            if matches!(surface, Surface::Plane { .. }) {
                continue;
            }
            check_finite(surface)?;
            let fg = face_geom(solid, fid);
            // Lage auf der beschnittenen Fläche: analytische Flächen wie der Kernel; Freiform-
            // flächen über die einmal vorbereiteten Randpolygone (der Kernel baut sie je Frage neu
            // und projiziert dabei jeden Randpunkt auf die Fläche).
            let region = matches!(surface, Surface::Nurbs(_))
                .then(|| Region::new(fg.clone(), REGION_FACTOR * tol.linear));
            let in_face = |p: DVec3| match &region {
                Some(r) => r
                    .classify(REGION_FACTOR * tol.linear, p, surface.project(p))
                    .is_some(),
                None => point_in_face(&fg, p, &tol) != Containment::Outside,
            };
            let origin = StrokeOrigin::Silhouette {
                body: bi,
                face: fid.index(),
            };
            let mut push = |curve: Curve, t0: f64, t1: f64| -> Result<(), DrawError> {
                out.push(Stroke {
                    curve,
                    t0,
                    t1,
                    origin: origin.clone(),
                });
                if out.len() as u64 > MAX_RAW_STROKES {
                    return Err(DrawError::TooComplex(out.len() as u64));
                }
                Ok(())
            };
            let candidates = match surface {
                Surface::Cylinder { frame, radius } => {
                    cylinder_lines(&fg, frame, *radius, d, &tol)?
                }
                Surface::Cone { .. } => cone_lines(&fg, d, &tol)?,
                Surface::Sphere { frame, radius } => vec![great_circle(frame, *radius, d)],
                Surface::Torus {
                    frame,
                    major,
                    minor,
                } => {
                    let dl = local_dir(frame, d);
                    if dl.z.abs() <= TORUS_FLAT {
                        torus_circles(frame, *major, *minor, dl)
                    } else {
                        for k in 0..2 {
                            let path = torus_branch(frame, *major, *minor, dl, k);
                            for (c, t0, t1) in
                                spline_pieces(&fg, &path, &in_face, &tol, &mut work, progress)?
                            {
                                push(c, t0, t1)?;
                            }
                        }
                        Vec::new()
                    }
                }
                Surface::Nurbs(data) => {
                    for chain in outline_chains(data, d, &mut work, progress)? {
                        let path = chain.path(data, d);
                        for (c, t0, t1) in
                            spline_pieces(&fg, &path, &in_face, &tol, &mut work, progress)?
                        {
                            push(c, t0, t1)?;
                        }
                    }
                    Vec::new()
                }
                Surface::Plane { .. } => Vec::new(),
            };
            for c in candidates {
                progress.check()?;
                let point = |t: f64| c.curve.point(t);
                let path = Path {
                    point: &point,
                    t0: c.t0,
                    t1: c.t1,
                    closed: c.closed,
                };
                let proj = match &c.curve {
                    Curve::Line { origin, dir } => Proj::Line {
                        origin: *origin,
                        dir: *dir,
                    },
                    Curve::Circle { frame, .. } => Proj::Circle(*frame),
                    _ => unreachable!("Umrisskandidaten sind Geraden und Kreise"),
                };
                for r in trim_to_face(
                    &fg,
                    &path,
                    &proj,
                    &in_face,
                    &tol,
                    SNAP_FACTOR * tol.linear,
                    progress,
                )? {
                    push(c.curve.clone(), r.a, r.b)?;
                }
            }
        }
    }
    Ok(out)
}

/// Zähler für den Rechenaufwand der Spline-Umrisse einer Fläche (Rasterpunkte und dichte
/// Punkte): über `MAX_OUTLINE_POINTS` → `TooComplex`.
#[derive(Default)]
struct Work {
    points: u64,
}

impl Work {
    fn add(&mut self, n: usize) -> Result<(), DrawError> {
        self.points += n as u64;
        if self.points > MAX_OUTLINE_POINTS {
            return Err(DrawError::TooComplex(self.points));
        }
        Ok(())
    }
}

/// Blickrichtung im Rahmen der Fläche.
fn local_dir(frame: &Frame, d: DVec3) -> DVec3 {
    DVec3::new(d.dot(frame.x), d.dot(frame.y), d.dot(frame.z))
}

/// Ein Umrissweg: Punkt zum Parameter `t ∈ [t0, t1]`; `closed`: periodisch mit Periode
/// `t1 − t0` (Anfang = Ende).
struct Path<'a> {
    point: &'a dyn Fn(f64) -> DVec3,
    t0: f64,
    t1: f64,
    closed: bool,
}

/// Ein Spline-Umrissweg mit eigenem Speicher für die Punktfunktion.
struct OwnedPath {
    point: Box<dyn Fn(f64) -> DVec3>,
    t0: f64,
    t1: f64,
    closed: bool,
    /// Startstützstellen (aufsteigend, `t0` … `t1`) für die Verfeinerung.
    knots: Vec<f64>,
    /// Freiform: wird gesetzt, wenn ein Wegpunkt nicht auf den Umriss fand (Newton).
    failed: Option<std::rc::Rc<std::cell::Cell<bool>>>,
}

impl OwnedPath {
    fn as_path(&self) -> Path<'_> {
        Path {
            point: &*self.point,
            t0: self.t0,
            t1: self.t1,
            closed: self.closed,
        }
    }
}

/// Torus-Zweig `k` (0: `v₁`, 1: `v₂ = v₁ + π`) als geschlossener Weg über `u ∈ [0, 2π]`.
fn torus_branch(frame: &Frame, major: f64, minor: f64, dl: DVec3, k: usize) -> OwnedPath {
    let f = *frame;
    let shift = k as f64 * PI;
    let point = move |u: f64| {
        let v = (-(dl.x * u.cos() + dl.y * u.sin())).atan2(dl.z) + shift;
        torus_point(&f, major, minor, u, v)
    };
    OwnedPath {
        point: Box::new(point),
        t0: 0.0,
        t1: TAU,
        closed: true,
        knots: (0..=TORUS_SAMPLES)
            .map(|i| TAU * i as f64 / TORUS_SAMPLES as f64)
            .collect(),
        failed: None,
    }
}

/// Torusoberfläche `P(u, v) = o + x (R + r cos v) cos u + y (R + r cos v) sin u + z r sin v`.
fn torus_point(f: &Frame, major: f64, minor: f64, u: f64, v: f64) -> DVec3 {
    let rho = major + minor * v.cos();
    f.origin + f.x * (rho * u.cos()) + f.y * (rho * u.sin()) + f.z * (minor * v.sin())
}

/// Torus, Blick in der Ringebene: `cos v · (d_x cos u + d_y sin u) = 0` → die Kreise `v = ±π/2`
/// (Radius `R`, Mitte `o ± r·z`) und die Meridiankreise bei `u = atan2(d_y, d_x) ± π/2` (Radius
/// `r`, Mitte `o + R·n_u`, Ebene durch die Achse).
fn torus_circles(frame: &Frame, major: f64, minor: f64, dl: DVec3) -> Vec<Candidate> {
    let full = |curve: Curve| Candidate {
        curve,
        t0: 0.0,
        t1: TAU,
        closed: true,
    };
    let mut out = Vec::new();
    for s in [1.0, -1.0] {
        out.push(full(Curve::Circle {
            frame: Frame {
                origin: frame.origin + frame.z * (s * minor),
                ..*frame
            },
            radius: major,
        }));
    }
    let phi = dl.y.atan2(dl.x);
    for um in [phi + FRAC_PI_2, phi - FRAC_PI_2] {
        let nu = frame.x * um.cos() + frame.y * um.sin();
        out.push(full(Curve::Circle {
            frame: Frame {
                origin: frame.origin + nu * major,
                x: nu,
                y: frame.z,
                z: nu.cross(frame.z),
            },
            radius: minor,
        }));
    }
    out
}

/// Dichte Stützstellen des Wegs: Ausgangsstellen `path.knots`, jedes Glied gleichmäßig in
/// `2^k` Teile geteilt (`k ≤ MAX_DOUBLINGS`), bis die Sehnenmitten höchstens
/// `SAG_SHARE · SILHOUETTE_TOL` vom Weg abweichen (sonst benannte Meldung). Liefert
/// `(t, Punkt)`.
fn densify(
    path: &OwnedPath,
    work: &mut Work,
    progress: &Progress,
) -> Result<Vec<(f64, DVec3)>, DrawError> {
    let limit = SAG_SHARE * SILHOUETTE_TOL;
    let p = &path.point;
    let mut out: Vec<(f64, DVec3)> = Vec::new();
    for w in path.knots.windows(2) {
        progress.check()?;
        let (a, b) = (w[0], w[1]);
        let mut n = 1usize;
        let mut pts: Vec<(f64, DVec3)>;
        let mut doublings = 0;
        loop {
            pts = (0..=n)
                .map(|i| {
                    let t = if i == n {
                        b
                    } else {
                        a + (b - a) * i as f64 / n as f64
                    };
                    (t, p(t))
                })
                .collect();
            let sag = pts
                .windows(2)
                .map(|q| p(0.5 * (q[0].0 + q[1].0)).distance(0.5 * (q[0].1 + q[1].1)))
                .fold(0.0, f64::max);
            if sag.is_nan() {
                return Err(DrawError::NonFinite);
            }
            if sag <= limit {
                break;
            }
            if doublings == MAX_DOUBLINGS {
                // Ohne die Sehnenschranke wäre die Prüfung der Anpassung nicht mehr streng.
                return Err(DrawError::Unsupported(format!(
                    "{FIT_MESSAGE} (Umriss zu stark gekrümmt)"
                )));
            }
            n *= 2;
            doublings += 1;
        }
        work.add(pts.len())?;
        if out.is_empty() {
            out.extend(pts);
        } else {
            out.extend(pts.into_iter().skip(1));
        }
    }
    Ok(out)
}

/// Spline-Stücke eines Umrisswegs: dicht abtasten, auf die Fläche beschneiden, jedes Stück
/// anpassen. Liefert `(Kurve, t0, t1)` (Parameterbereich der angepassten Kurve).
fn spline_pieces(
    fg: &FaceGeom,
    path: &OwnedPath,
    in_face: &dyn Fn(DVec3) -> bool,
    tol: &Tolerance,
    work: &mut Work,
    progress: &Progress,
) -> Result<Vec<(Curve, f64, f64)>, DrawError> {
    let dense = densify(path, work, progress)?;
    let limit = (SNAP_FACTOR * tol.linear).max(SPLINE_SNAP_SHARE * SILHOUETTE_TOL);
    let ranges = trim_to_face(
        fg,
        &path.as_path(),
        &Proj::Dense(&dense),
        in_face,
        tol,
        limit,
        progress,
    )?;
    // Freiform: Fand ein Wegpunkt nicht auf den Umriss (Newton), ist die Kurve dort geraten.
    // Geprüft nach jeder Auswertung des Wegs: auch die Endpunkte der Bereiche unten werten ihn
    // noch einmal aus.
    let failed = || {
        if path.failed.as_ref().is_some_and(|f| f.get()) {
            Err(DrawError::Unsupported(format!(
                "{FIT_MESSAGE} (Punkt nicht auf dem Umriss)"
            )))
        } else {
            Ok(())
        }
    };
    failed()?;
    let period = path.t1 - path.t0;
    let mut out = Vec::new();
    for r in ranges {
        progress.check()?;
        let gap = BISECT_TOL * period;
        let whole = path.closed && r.b - r.a >= period - gap;
        let mut pts: Vec<DVec3> = Vec::new();
        if whole {
            pts.extend(dense.iter().map(|x| x.1));
            if let Some(&first) = pts.first() {
                // Geschlossen: das letzte Stück endet genau am Anfang.
                let n = pts.len();
                pts[n - 1] = first;
            }
        } else {
            pts.push(r.ends[0].unwrap_or_else(|| (path.point)(r.a)));
            // Stützstellen echt zwischen a und b (bei geschlossenen Wegen auch jenseits t1).
            let laps: &[f64] = if path.closed { &[0.0, 1.0] } else { &[0.0] };
            for lap in laps {
                for &(t, q) in &dense {
                    let t = t + lap * period;
                    if t > r.a + gap && t < r.b - gap {
                        pts.push(q);
                    }
                }
            }
            pts.push(r.ends[1].unwrap_or_else(|| (path.point)(r.b)));
        }
        failed()?;
        for c in fit_checked(&pts, tol, 0, progress)? {
            let (t0, t1) = c.domain();
            out.push((Curve::nurbs(c), t0, t1));
        }
    }
    Ok(out)
}

/// Passt die Punkte an (`NurbsCurve::fit`, Schranke `SILHOUETTE_TOL/2`, je Versuch halbiert) und
/// prüft die Kurve gegen den Streckenzug der Punkte (höchstens `FIT_SHARE · SILHOUETTE_TOL`).
/// Gelingt das nicht (lange Umrisse brauchten mehr als die zulässigen Kontrollpunkte), wird die
/// Punktfolge in der Mitte geteilt (gemeinsamer Punkt) und jede Hälfte für sich angepasst,
/// höchstens `FIT_SPLITS` Stufen tief. Zu kurze Stücke (Länge ≤ `tol`) entfallen.
fn fit_checked(
    pts: &[DVec3],
    tol: &Tolerance,
    depth: usize,
    progress: &Progress,
) -> Result<Vec<NurbsCurve>, DrawError> {
    progress.check()?;
    let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    if !total.is_finite() {
        return Err(DrawError::NonFinite);
    }
    if pts.len() < 2 || total <= tol.linear {
        return Ok(Vec::new());
    }
    let mut ftol = 0.5 * SILHOUETTE_TOL;
    let mut why = String::new();
    for _ in 0..FIT_ATTEMPTS {
        match NurbsCurve::fit(pts, ftol) {
            Ok(c) if follows(&c, pts, FIT_SHARE * SILHOUETTE_TOL) => return Ok(vec![c]),
            Ok(_) => why = "Prüfung zwischen den Stützstellen".into(),
            Err(e) => why = e.to_string(),
        }
        ftol *= 0.5;
    }
    if depth < FIT_SPLITS && pts.len() >= 4 {
        let m = pts.len() / 2;
        let mut out = fit_checked(&pts[..=m], tol, depth + 1, progress)?;
        out.extend(fit_checked(&pts[m..], tol, depth + 1, progress)?);
        return Ok(out);
    }
    Err(DrawError::Unsupported(format!("{FIT_MESSAGE} ({why})")))
}

/// Liegt die Kurve überall höchstens `limit` neben dem Streckenzug `pts`? Geprüft an
/// `FIT_CHECKS_PER_CHORD` Stellen je Sehne; gesucht wird in einem Fenster um die Sehne mit
/// demselben Längenanteil (Parameter der Anpassung = Sehnenlängenanteil).
fn follows(c: &NurbsCurve, pts: &[DVec3], limit: f64) -> bool {
    let mut cum = Vec::with_capacity(pts.len());
    let mut s = 0.0;
    cum.push(0.0);
    for w in pts.windows(2) {
        s += w[0].distance(w[1]);
        cum.push(s);
    }
    let total = s;
    let (a, b) = c.domain();
    let n = pts.len() - 1;
    let m = FIT_CHECKS_PER_CHORD * n;
    for j in 0..=m {
        let f = j as f64 / m as f64;
        let q = c.point(a + (b - a) * f);
        let k = cum
            .partition_point(|&x| x <= f * total)
            .saturating_sub(1)
            .min(n - 1);
        let lo = k.saturating_sub(FIT_CHECK_WINDOW);
        let hi = (k + FIT_CHECK_WINDOW).min(n - 1);
        let near = (lo..=hi).any(|i| seg_dist3(q, pts[i], pts[i + 1]) <= limit);
        if !near {
            return false;
        }
    }
    true
}

/// Abstand des Punkts `q` von der Strecke `a b`.
fn seg_dist3(q: DVec3, a: DVec3, b: DVec3) -> f64 {
    let ab = b - a;
    let l2 = ab.length_squared();
    let t = if l2 > 0.0 {
        ((q - a).dot(ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    q.distance(a + ab * t)
}

/// Anteil von `SILHOUETTE_TOL`, bis zu dem ein Spline-Umrissende auf die Randkante gelegt wird
/// (der wahre Umriss liegt auf der Fläche, die Kante nur auf deren Toleranz).
const SPLINE_SNAP_SHARE: f64 = 0.25;

// ---------------------------------------------------------------------------------------------
// Freiform: Raster, Marching Squares, Ketten.

/// Eine verkettete Umrisskurve im Parameterraum (fortlaufend, über Nähte entfaltet).
struct Chain {
    uv: Vec<DVec2>,
    closed: bool,
}

/// `g = n·d` und `∇g` bei `(u, v)` (gefaltet); `None` ohne Normale.
fn g_grad(sf: &NurbsSurface, data: &NurbsSurfaceData, d: DVec3, uv: DVec2) -> Option<(f64, DVec2)> {
    let (u, v) = data.wrap(uv.x, uv.y);
    let dv = sf.derivs(u, v, 2);
    let (su, sv) = (dv[1][0], dv[0][1]);
    let n = su.cross(sv);
    let len = n.length();
    if sf.normal(u, v) == DVec3::ZERO || len.is_nan() || len <= 0.0 || !len.is_finite() {
        return None;
    }
    let nh = n / len;
    let nu = dv[2][0].cross(sv) + su.cross(dv[1][1]);
    let nv = dv[1][1].cross(sv) + su.cross(dv[0][2]);
    let gu = (nu - nh * nh.dot(nu)).dot(d) / len;
    let gv = (nv - nh * nh.dot(nv)).dot(d) / len;
    Some((nh.dot(d), DVec2::new(gu, gv)))
}

/// `g = n·d` bei `(u, v)` mit der Normale von `NurbsSurface::normal` (`None` am Pol).
fn g_at(sf: &NurbsSurface, data: &NurbsSurfaceData, d: DVec3, u: f64, v: f64) -> Option<f64> {
    let (u, v) = data.wrap(u, v);
    let n = sf.normal(u, v);
    (n != DVec3::ZERO).then(|| n.dot(d))
}

/// Newton auf `g = 0` in Richtung `∇g` (höchstens `NEWTON_STEPS`, bis `|g| < NEWTON_G`);
/// nicht geschlossene Richtungen bleiben im Bereich. Liefert den besten Punkt, `None`, wenn dort
/// nicht `|g| ≤ NEWTON_ACCEPT` gilt (der Punkt läge neben dem Umriss).
fn newton_outline(data: &NurbsSurfaceData, d: DVec3, uv0: DVec2) -> Option<DVec2> {
    let sf = data.surface();
    let ((u0, u1), (v0, v1)) = sf.domain();
    let keep = |p: DVec2| {
        DVec2::new(
            if data.is_periodic_u() {
                p.x
            } else {
                p.x.clamp(u0, u1)
            },
            if data.is_periodic_v() {
                p.y
            } else {
                p.y.clamp(v0, v1)
            },
        )
    };
    let mut uv = keep(uv0);
    let mut best = (f64::INFINITY, uv);
    for _ in 0..=NEWTON_STEPS {
        let Some((g, grad)) = g_grad(sf, data, d, uv) else {
            break;
        };
        if g.abs() < best.0 {
            best = (g.abs(), uv);
        }
        let gg = grad.length_squared();
        if g.abs() < NEWTON_G || gg.is_nan() || gg <= 0.0 || !gg.is_finite() {
            break;
        }
        uv = keep(uv - grad * (g / gg));
    }
    (best.0 <= NEWTON_ACCEPT).then_some(best.1)
}

/// Rasterlinien einer Richtung: je Bézier-Spanne gleich viele Teile, insgesamt mindestens
/// `GRID_MIN`, je Spanne mindestens `GRID_PER_SPAN`, höchstens `GRID_MAX` (bei mehr als 32
/// Spannen also weniger als 8 je Spanne); auf Stufe `level` je Spanne `2^level`-mal so viele,
/// ebenfalls höchstens `GRID_MAX`. Liefert die Linien und ob die Obergrenze erreicht ist.
fn grid_lines(knots: &[f64], lo: f64, hi: f64, level: u32) -> Result<(Vec<f64>, bool), DrawError> {
    let mut breaks: Vec<f64> = vec![lo];
    for &k in knots {
        if k > lo && k < hi && breaks.last().is_some_and(|&b| k > b) {
            breaks.push(k);
        }
    }
    breaks.push(hi);
    let spans = breaks.len() - 1;
    if spans > GRID_MAX {
        return Err(DrawError::TooComplex(spans as u64));
    }
    let cap = (GRID_MAX / spans).max(1);
    let base = GRID_PER_SPAN.max(GRID_MIN.div_ceil(spans)).min(cap);
    let per = base.saturating_mul(1 << level.min(16)).min(cap);
    let mut out = Vec::with_capacity(spans * per + 1);
    for w in breaks.windows(2) {
        for i in 0..per {
            out.push(w[0] + (w[1] - w[0]) * i as f64 / per as f64);
        }
    }
    out.push(hi);
    Ok((out, per == cap))
}
/// Kennung einer Rasterkante: waagerecht (in u) bei `(i, j)` → `(i, j) – (i+1, j)`, senkrecht
/// (in v) → `(i, j) – (i, j+1)`; Indizes geschlossener Richtungen gefaltet.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum EdgeKey {
    H(usize, usize),
    V(usize, usize),
}

/// Versteckter Vorzeichenwechsel: Hat der Mittelpunkt einer Rasterkante, deren Enden gleiches
/// Vorzeichen haben, das andere Vorzeichen (die Kante wird zweimal gequert), oder die Mitte einer
/// Zelle mit vier gleichen Ecken (eine Schleife innerhalb der Zelle), würde Marching Squares
/// den Umriss dort verlieren. Rasterpunkte ohne Normale (Pol) bleiben außen vor.
fn hidden_change(
    us: &[f64],
    vs: &[f64],
    gv: &[Option<f64>],
    g: &dyn Fn(f64, f64) -> Result<Option<f64>, DrawError>,
    work: &mut Work,
    progress: &Progress,
) -> Result<bool, DrawError> {
    let (nu, nv) = (us.len() - 1, vs.len() - 1);
    let at = |i: usize, j: usize| gv[i * (nv + 1) + j];
    let pos = |x: f64| x >= 0.0;
    let differs = |a: Option<f64>, b: Option<f64>, u: f64, v: f64| -> Result<bool, DrawError> {
        let (Some(a), Some(b)) = (a, b) else {
            return Ok(false);
        };
        if pos(a) != pos(b) {
            return Ok(false);
        }
        Ok(g(u, v)?.is_some_and(|m| pos(m) != pos(a)))
    };
    work.add(3 * (nu + 1) * (nv + 1))?;
    for i in 0..=nu {
        progress.check()?;
        for j in 0..=nv {
            if i < nu && differs(at(i, j), at(i + 1, j), 0.5 * (us[i] + us[i + 1]), vs[j])? {
                return Ok(true);
            }
            if j < nv && differs(at(i, j), at(i, j + 1), us[i], 0.5 * (vs[j] + vs[j + 1]))? {
                return Ok(true);
            }
            if i < nu && j < nv {
                let c = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
                if c.iter().all(|x| x.is_some())
                    && c.iter()
                        .flatten()
                        .all(|x| pos(*x) == pos(c[0].unwrap_or(0.0)))
                    && differs(
                        c[0],
                        c[2],
                        0.5 * (us[i] + us[i + 1]),
                        0.5 * (vs[j] + vs[j + 1]),
                    )?
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
/// Umrissketten einer Freiformfläche (siehe Modulbeschreibung).
fn outline_chains(
    data: &NurbsSurfaceData,
    d: DVec3,
    work: &mut Work,
    progress: &Progress,
) -> Result<Vec<Chain>, DrawError> {
    let sf = data.surface();
    let ((u0, u1), (v0, v1)) = sf.domain();
    let (pu, pv) = (data.is_periodic_u(), data.is_periodic_v());
    // Werte wie an den Rasterpunkten: |g| ≤ ZERO_G gilt als 0 (Vorzeichen +).
    let g_snap = |u: f64, v: f64| -> Result<Option<f64>, DrawError> {
        match g_at(sf, data, d, u, v) {
            Some(g) if !g.is_finite() => Err(DrawError::NonFinite),
            Some(g) => Ok(Some(if g.abs() <= ZERO_G { 0.0 } else { g })),
            None => Ok(None),
        }
    };
    let pos = |g: f64| g >= 0.0;
    // Raster verfeinern, bis kein Vorzeichenwechsel zwischen den Rasterpunkten versteckt ist.
    let mut level = 0;
    let (us, vs, gv) = loop {
        let (us, cap_u) = grid_lines(sf.knots_u().knots(), u0, u1, level)?;
        let (vs, cap_v) = grid_lines(sf.knots_v().knots(), v0, v1, level)?;
        let (nu, nv) = (us.len() - 1, vs.len() - 1);
        work.add((nu + 1) * (nv + 1))?;
        // Werte an den Rasterpunkten (geschlossene Richtungen: letzte Reihe = erste Reihe).
        let mut gv: Vec<Option<f64>> = vec![None; (nu + 1) * (nv + 1)];
        let at = |i: usize, j: usize| i * (nv + 1) + j;
        for i in 0..=nu {
            progress.check()?;
            for j in 0..=nv {
                let fi = if pu && i == nu { 0 } else { i };
                let fj = if pv && j == nv { 0 } else { j };
                gv[at(i, j)] = if (fi, fj) != (i, j) {
                    gv[at(fi, fj)]
                } else {
                    g_snap(us[i], vs[j])?
                };
            }
        }
        if !hidden_change(&us, &vs, &gv, &g_snap, work, progress)? {
            break (us, vs, gv);
        }
        if cap_u && cap_v {
            return Err(DrawError::Unsupported(HIDDEN_MESSAGE.into()));
        }
        level += 1;
    };
    if gv.iter().flatten().all(|g| *g == 0.0) {
        return Ok(Vec::new());
    }
    let (nu, nv) = (us.len() - 1, vs.len() - 1);
    let at = |i: usize, j: usize| i * (nv + 1) + j;
    let fold_i = |i: usize| if pu && i == nu { 0 } else { i };
    let fold_j = |j: usize| if pv && j == nv { 0 } else { j };
    let mut nodes: Vec<DVec2> = Vec::new();
    let mut index: std::collections::HashMap<EdgeKey, usize> = std::collections::HashMap::new();
    let mut links: Vec<Vec<usize>> = Vec::new();
    // Nullstelle auf einer Rasterkante (Bisektion bis zur Rechengenauigkeit).
    let mut crossing = |key: EdgeKey,
                        nodes: &mut Vec<DVec2>,
                        links: &mut Vec<Vec<usize>>|
     -> Result<usize, DrawError> {
        if let Some(&k) = index.get(&key) {
            return Ok(k);
        }
        let (a, b) = match key {
            EdgeKey::H(i, j) => (DVec2::new(us[i], vs[j]), DVec2::new(us[i + 1], vs[j])),
            EdgeKey::V(i, j) => (DVec2::new(us[i], vs[j]), DVec2::new(us[i], vs[j + 1])),
        };
        let g = |p: DVec2| -> Result<f64, DrawError> {
            g_snap(p.x, p.y)?.ok_or(DrawError::Unsupported(POLE_MESSAGE.into()))
        };
        let (mut lo, mut hi) = (a, b);
        let ga = g(lo)?;
        for _ in 0..MAX_BISECT_STEPS {
            let mid = 0.5 * (lo + hi);
            if mid == lo || mid == hi {
                break;
            }
            let gm = g(mid)?;
            if pos(gm) == pos(ga) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let p = 0.5 * (lo + hi);
        nodes.push(p);
        links.push(Vec::new());
        index.insert(key, nodes.len() - 1);
        Ok(nodes.len() - 1)
    };
    for i in 0..nu {
        progress.check()?;
        for j in 0..nv {
            let corners = [
                gv[at(i, j)],
                gv[at(i + 1, j)],
                gv[at(i + 1, j + 1)],
                gv[at(i, j + 1)],
            ];
            let defined: Vec<f64> = corners.iter().flatten().copied().collect();
            if defined.len() < 4 {
                if defined.iter().any(|&g| pos(g)) && defined.iter().any(|&g| !pos(g)) {
                    return Err(DrawError::Unsupported(POLE_MESSAGE.into()));
                }
                continue;
            }
            let s: Vec<bool> = defined.iter().map(|&g| pos(g)).collect();
            let keys = [
                EdgeKey::H(i, fold_j(j)),
                EdgeKey::V(fold_i(i + 1), j),
                EdgeKey::H(i, fold_j(j + 1)),
                EdgeKey::V(fold_i(i), j),
            ];
            // Kante e verbindet die Ecken (e, e+1 mod 4) – Ecken in der Reihenfolge c0..c3.
            let cuts: Vec<usize> = (0..4).filter(|&e| s[e] != s[(e + 1) % 4]).collect();
            let pairs: Vec<(usize, usize)> = match cuts.len() {
                0 => continue,
                2 => vec![(cuts[0], cuts[1])],
                4 => {
                    let mid = DVec2::new(0.5 * (us[i] + us[i + 1]), 0.5 * (vs[j] + vs[j + 1]));
                    let gc =
                        g_snap(mid.x, mid.y)?.ok_or(DrawError::Unsupported(POLE_MESSAGE.into()))?;
                    if pos(gc) == s[0] {
                        // c0 und c2 hängen über die Mitte zusammen: c1 und c3 abgetrennt.
                        vec![(0, 1), (2, 3)]
                    } else {
                        vec![(3, 0), (1, 2)]
                    }
                }
                _ => unreachable!("Vorzeichenwechsel im Viereck sind gerade"),
            };
            for (ea, eb) in pairs {
                let ka = crossing(keys[ea], &mut nodes, &mut links)?;
                let kb = crossing(keys[eb], &mut nodes, &mut links)?;
                links[ka].push(kb);
                links[kb].push(ka);
            }
        }
    }
    work.add(nodes.len())?;
    // Ketten: zuerst offene (ab Knoten mit einem Nachbarn), dann geschlossene.
    let mut seen = vec![false; nodes.len()];
    let unwrap = |prev: DVec2, mut q: DVec2| {
        if pu {
            let p = u1 - u0;
            q.x += ((prev.x - q.x) / p).round() * p;
        }
        if pv {
            let p = v1 - v0;
            q.y += ((prev.y - q.y) / p).round() * p;
        }
        q
    };
    let mut chains = Vec::new();
    let starts: Vec<usize> = (0..nodes.len())
        .filter(|&k| links[k].len() == 1)
        .chain(0..nodes.len())
        .collect();
    for start in starts {
        if seen[start] || links[start].is_empty() {
            continue;
        }
        progress.check()?;
        let mut uv = vec![nodes[start]];
        seen[start] = true;
        let (mut prev, mut cur) = (start, links[start][0]);
        let mut closed = false;
        loop {
            let last = *uv.last().expect("Kette nicht leer");
            if cur == start {
                // Geschlossen: zurück am Anfang (entfaltet).
                uv.push(unwrap(last, nodes[start]));
                closed = true;
                break;
            }
            uv.push(unwrap(last, nodes[cur]));
            seen[cur] = true;
            match links[cur].iter().copied().find(|&k| k != prev) {
                Some(n) if n == start || !seen[n] => {
                    prev = cur;
                    cur = n;
                }
                _ => break,
            }
        }
        chains.push(Chain { uv, closed });
    }
    Ok(chains)
}

impl Chain {
    /// Weg über die Kettenglieder: `t ∈ [0, n−1]`; zwischen den Gliedern linear im
    /// Parameterraum, dann per Newton auf `g = 0`. Ganzzahlige `t` sind die Glieder selbst.
    fn path(&self, data: &std::sync::Arc<NurbsSurfaceData>, d: DVec3) -> OwnedPath {
        let uv = self.uv.clone();
        let n = uv.len() - 1;
        let data = data.clone();
        let closed = self.closed;
        let failed = std::rc::Rc::new(std::cell::Cell::new(false));
        let flag = failed.clone();
        let point = move |t: f64| {
            let t = if closed && (t < 0.0 || t > n as f64) {
                t.rem_euclid(n as f64)
            } else {
                t.clamp(0.0, n as f64)
            };
            let i = (t.floor() as usize).min(n - 1);
            let f = t - i as f64;
            let q = if f == 0.0 {
                uv[i]
            } else if f == 1.0 {
                uv[i + 1]
            } else {
                let start = uv[i].lerp(uv[i + 1], f);
                newton_outline(&data, d, start).unwrap_or_else(|| {
                    flag.set(true);
                    start
                })
            };
            data.point(q.x, q.y)
        };
        OwnedPath {
            point: Box::new(point),
            t0: 0.0,
            t1: n as f64,
            closed,
            knots: (0..=n).map(|i| i as f64).collect(),
            failed: Some(failed),
        }
    }
}

/// Punkt des wahren Umrisses der Fläche nahe `p` (für die Sichtprüfung der Spline-Umrisse, die
/// bis `SILHOUETTE_TOL` neben ihm liegen): Torus über `u` des Punkts und den näheren Zweig,
/// Freiform über den Fußpunkt auf der Fläche und Newton auf `g = 0`. Andere Flächen: `p`.
pub fn outline_point(surface: &Surface, d: DVec3, p: DVec3) -> DVec3 {
    match surface {
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            let dl = local_dir(frame, d);
            if dl.z.abs() <= TORUS_FLAT {
                return p;
            }
            let l = frame.to_local(p);
            let u = l.y.atan2(l.x);
            let v = l.z.atan2((l.x * l.x + l.y * l.y).sqrt() - major);
            let v1 = (-(dl.x * u.cos() + dl.y * u.sin())).atan2(dl.z);
            let ang = |a: f64| {
                let r = (a - v).rem_euclid(TAU);
                r.min(TAU - r)
            };
            let vb = if ang(v1) <= ang(v1 + PI) { v1 } else { v1 + PI };
            torus_point(frame, *major, *minor, u, vb)
        }
        Surface::Nurbs(data) => {
            let (u, v) = data.project(p);
            match newton_outline(data, d, DVec2::new(u, v)) {
                Some(q) => {
                    let s = data.point(q.x, q.y);
                    if s.is_finite() { s } else { p }
                }
                None => p,
            }
        }
        _ => p,
    }
}

/// Rahmen und Maße der Fläche sind endliche Zahlen.
fn check_finite(surface: &Surface) -> Result<(), DrawError> {
    let f = surface.frame();
    let frame_ok = f.origin.is_finite() && f.x.is_finite() && f.y.is_finite() && f.z.is_finite();
    let dims_ok = match surface {
        Surface::Cylinder { radius, .. } | Surface::Sphere { radius, .. } => radius.is_finite(),
        Surface::Cone { radius, slope, .. } => radius.is_finite() && slope.is_finite(),
        Surface::Torus { major, minor, .. } => major.is_finite() && minor.is_finite(),
        Surface::Nurbs(n) => {
            let sf = n.surface();
            let (nu, nv) = sf.counts();
            (0..nu).all(|i| (0..nv).all(|j| sf.control_at(i, j).is_finite()))
        }
        Surface::Plane { .. } => true,
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

/// Nächster Wegpunkt zu einem Raumpunkt, je nach Art des Wegs.
enum Proj<'a> {
    /// Gerade `origin + t·dir` (`dir` normiert).
    Line { origin: DVec3, dir: DVec3 },
    /// Kreis im Rahmen (Parameter = Winkel um `z` ab `x`).
    Circle(Frame),
    /// Dichter Streckenzug `(t, Punkt)` des Wegs (Spline-Umrisse).
    Dense(&'a [(f64, DVec3)]),
}

impl Proj<'_> {
    /// Größte Abweichung des nächsten Wegpunkts vom wahren nächsten Punkt (Streckenzug: die
    /// Sehnenabweichung des dichten Wegs).
    fn slack(&self) -> f64 {
        match self {
            Proj::Dense(_) => SILHOUETTE_TOL,
            _ => 0.0,
        }
    }

    /// Parameter des Wegpunkts nächst `q` im Bereich des Wegs (geschlossene Wege: in
    /// `[t0, t1)`); `None`, wenn es keinen gibt (Punkt auf der Kreisachse, leerer Streckenzug).
    fn nearest(&self, c: &Path, q: DVec3) -> Option<f64> {
        let w = c.t1 - c.t0;
        let wrap = |t: f64| {
            if c.closed && w > 0.0 {
                c.t0 + (t - c.t0).rem_euclid(w)
            } else {
                t.clamp(c.t0.min(c.t1), c.t0.max(c.t1))
            }
        };
        match self {
            Proj::Line { origin, dir } => Some(wrap((q - *origin).dot(*dir))),
            Proj::Circle(frame) => {
                let l = frame.to_local(q);
                if l.x == 0.0 && l.y == 0.0 {
                    return None;
                }
                let a = l.y.atan2(l.x);
                if c.closed {
                    Some(wrap(a))
                } else {
                    let t = c.t0 + (a - c.t0).rem_euclid(TAU);
                    (t <= c.t1).then_some(t)
                }
            }
            Proj::Dense(pts) => {
                let mut best: Option<(f64, f64)> = None;
                for s in pts.windows(2) {
                    let (ta, a) = s[0];
                    let (tb, b) = s[1];
                    let ab = b - a;
                    let l2 = ab.length_squared();
                    let f = if l2 > 0.0 {
                        ((q - a).dot(ab) / l2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let dist = q.distance(a + ab * f);
                    if best.is_none_or(|(_, bd)| dist < bd) {
                        best = Some((ta + (tb - ta) * f, dist));
                    }
                }
                best.map(|(t, _)| wrap(t))
            }
        }
    }
}

/// Stützstellen eines Randstücks für die randgeführten Abtaststellen: Geraden
/// `BOUNDARY_SEG_SAMPLES`, Kreise und Ellipsen alle 5° (mindestens `BOUNDARY_SEG_SAMPLES`),
/// Freiformkanten `BOUNDARY_NURBS_SAMPLES`.
fn boundary_seg_samples(seg: &planora_kernel::boolean::Seg) -> usize {
    match seg.curve {
        Curve::Line { .. } => BOUNDARY_SEG_SAMPLES,
        Curve::Circle { .. } | Curve::Ellipse { .. } => {
            let n = ((seg.t1 - seg.t0).abs() * BOUNDARY_SAMPLES_PER_TURN / TAU).ceil();
            if n.is_finite() {
                (n as usize).clamp(BOUNDARY_SEG_SAMPLES, MAX_TRIM_SAMPLES)
            } else {
                BOUNDARY_SEG_SAMPLES
            }
        }
        Curve::Nurbs(_) => BOUNDARY_NURBS_SAMPLES,
    }
}

/// Vielfaches der größten Sehne eines Randstücks, bis zu dem ein Randpunkt als „am Weg“ gilt
/// (Bogen zu Sehne bei 5°-Schritten ≈ 1,0003; Freiformkanten mit Reserve).
const BOUNDARY_REACH: f64 = 1.5;

/// Randgeführte Abtaststellen eines Wegs: Jedes Randstück der Fläche wird dicht abgetastet, und
/// jeder Randpunkt, der dem Weg näher kommt als der Abstand benachbarter Randpunkte (also jeder
/// Punkt beiderseits einer Stelle, an der der Rand den Weg kreuzt oder berührt), liefert den
/// Parameter seines nächsten Wegpunkts. So bekommt jede Aussparung und jeder Flächenstreifen,
/// den der Weg quert, Abtaststellen an seinen Rändern – unabhängig davon, wie schmal er gegen den
/// gleichmäßigen Abstand ist. Mehr als `MAX_TRIM_SAMPLES` Stellen → `TooComplex`.
fn boundary_params(
    fg: &FaceGeom,
    c: &Path,
    proj: &Proj,
    tol: &Tolerance,
    progress: &Progress,
) -> Result<Vec<f64>, DrawError> {
    let mut out = Vec::new();
    for seg in fg.loops.iter().flatten() {
        progress.check()?;
        let n = boundary_seg_samples(seg);
        let pts: Vec<DVec3> = (0..=n).map(|i| seg.at(i as f64 / n as f64)).collect();
        let chord = pts
            .windows(2)
            .map(|p| p[0].distance(p[1]))
            .fold(0.0, f64::max);
        if !chord.is_finite() {
            return Err(DrawError::NonFinite);
        }
        let reach = BOUNDARY_REACH * chord + proj.slack() + tol.linear;
        for q in pts {
            if let Some(t) = proj.nearest(c, q)
                && (c.point)(t).distance(q) <= reach
            {
                out.push(t);
            }
        }
        if out.len() > MAX_TRIM_SAMPLES {
            return Err(DrawError::TooComplex(out.len() as u64));
        }
    }
    Ok(out)
}

/// Ein beschnittener Teilbereich `[a, b]` eines Wegs; `ends`: die Punkte der Randkante, auf die
/// seine Enden gelegt wurden (`None`, wo kein Rand anliegt oder der Bereich am Wegende endet).
struct Trimmed {
    a: f64,
    b: f64,
    ends: [Option<DVec3>; 2],
}

/// Teilbereiche des Wegs, die in der Fläche liegen (`Inside` oder `Boundary`).
///
/// Abtaststellen: `TRIM_SAMPLES` gleichmäßige Stellen, dazu die randgeführten Stellen aus
/// [`boundary_params`] und je eine Stelle mitten zwischen einer randgeführten Stelle und ihren
/// Nachbarn (die Randpunkte selbst zählen als innen; die Mitte liegt in einer schmalen
/// Aussparung). Wechselstellen per Bisektion (mindestens auf `BISECT_TOL` der Parameterbreite,
/// weiter bis zur Rechengenauigkeit), dann auf die Randkante gelegt ([`snap_to_edge`] mit der
/// Grenze `snap_limit`). Geschlossene Wege: ganz innen bleibt `[t0, t1]`; sonst Bögen, die über
/// `t1` hinausreichen dürfen.
fn trim_to_face(
    fg: &FaceGeom,
    c: &Path,
    proj: &Proj,
    in_face: &dyn Fn(DVec3) -> bool,
    tol: &Tolerance,
    snap_limit: f64,
    progress: &Progress,
) -> Result<Vec<Trimmed>, DrawError> {
    let w = c.t1 - c.t0;
    let point = c.point;
    let inside = |t: f64| in_face(point(t));
    // Gleichmäßig: offene Wege 0..N−1 einschließlich beider Enden; geschlossene N Stellen.
    let steps = if c.closed {
        TRIM_SAMPLES
    } else {
        TRIM_SAMPLES - 1
    };
    let mut ts: Vec<(f64, bool)> = (0..TRIM_SAMPLES)
        .map(|i| {
            let t = if !c.closed && i == steps {
                c.t1
            } else {
                c.t0 + w * i as f64 / steps as f64
            };
            (t, false)
        })
        .collect();
    ts.extend(
        boundary_params(fg, c, proj, tol, progress)?
            .into_iter()
            .map(|t| (t, true)),
    );
    let sort_dedup = |ts: &mut Vec<(f64, bool)>| {
        ts.sort_by(|a, b| a.0.total_cmp(&b.0));
        ts.dedup_by(|later, kept| {
            if later.0 == kept.0 {
                kept.1 |= later.1;
                true
            } else {
                false
            }
        });
    };
    sort_dedup(&mut ts);
    // Mitten neben randgeführten Stellen (geschlossen auch über das Ende hinweg).
    let n = ts.len();
    let mut mids = Vec::new();
    for i in 0..n {
        let (a, fa) = ts[i];
        let (b, fb) = if i + 1 < n {
            ts[i + 1]
        } else if c.closed {
            (ts[0].0 + w, ts[0].1)
        } else {
            continue;
        };
        if fa || fb {
            let m = 0.5 * (a + b);
            mids.push((if c.closed && m >= c.t1 { m - w } else { m }, false));
        }
    }
    ts.extend(mids);
    sort_dedup(&mut ts);
    if ts.len() > MAX_TRIM_SAMPLES {
        return Err(DrawError::TooComplex(ts.len() as u64));
    }
    let mut flags: Vec<bool> = Vec::with_capacity(ts.len());
    for (i, &(t, _)) in ts.iter().enumerate() {
        if i % 32 == 0 {
            progress.check()?;
        }
        flags.push(inside(t));
    }
    if flags.iter().all(|&f| f) {
        return Ok(vec![Trimmed {
            a: c.t0,
            b: c.t1,
            ends: [None, None],
        }]);
    }
    if flags.iter().all(|&f| !f) {
        return Ok(Vec::new());
    }
    // Folge der Stellen (Parameter, innen?). Geschlossene Wege: ab einer äußeren Stelle einmal
    // herum, so dass jeder innere Abschnitt beidseitig von äußeren Stellen begrenzt ist.
    let n = ts.len();
    let seq: Vec<(f64, bool)> = if c.closed {
        let k = flags.iter().position(|&f| !f).unwrap_or(0);
        (0..=n)
            .map(|m| {
                let i = (k + m) % n;
                let lap = ((k + m) / n) as f64;
                (ts[i].0 + lap * w, flags[i])
            })
            .collect()
    } else {
        ts.iter().zip(&flags).map(|(&(t, _), &f)| (t, f)).collect()
    };
    let spacing = w.abs() / steps as f64;
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
        match snap_to_edge(fg, point, a, SNAP_WINDOW * spacing, snap_limit) {
            Some((t, q)) => (if c.closed { t } else { t.clamp(c.t0, c.t1) }, Some(q)),
            None => (a, None),
        }
    };
    let mut out = Vec::new();
    let mut start: Option<(f64, Option<DVec3>)> = None;
    for j in 0..seq.len() {
        let (t, is_in) = seq[j];
        if is_in && start.is_none() {
            start = Some(if j == 0 {
                (t, None)
            } else {
                refine(t, seq[j - 1].0)
            });
        }
        let ends_here = is_in && (j + 1 == seq.len() || !seq[j + 1].1);
        if ends_here {
            progress.check()?;
            let (a, qa) = start.take().unwrap_or((t, None));
            let (b, qb) = if j + 1 == seq.len() {
                (t, None)
            } else {
                refine(t, seq[j + 1].0)
            };
            // Bögen, die ganz jenseits des Umlaufs beginnen, um eine Periode zurückschieben.
            let (a, b) = if c.closed && a >= c.t1 {
                (a - w, b - w)
            } else {
                (a, b)
            };
            let mid = point(0.5 * (a + b));
            let (pa, pb) = (point(a), point(b));
            if pa.distance(mid).max(mid.distance(pb)) > tol.linear {
                out.push(Trimmed {
                    a,
                    b,
                    ends: [qa, qb],
                });
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
/// `[t_star − h, t_star + h]` minimiert (Goldener Schnitt); unter den Minima, die höchstens
/// `limit` betragen, gilt das mit der Stelle **nächst `t_star`** (im Fenster kann bei einer
/// schmalen Aussparung auch deren gegenüberliegende Kante liegen). Grenze `limit`: exakte
/// Kurven `SNAP_FACTOR · tol`; Spline-Umrisse, deren wahrer Weg auf der Fläche liegt, die Kante
/// aber nur auf deren Toleranz, bis `SPLINE_SNAP_SHARE · SILHOUETTE_TOL`. Randstücke, an denen der Umriss schon am Fensterrand
/// anliegt (er läuft entlang einer Kante, etwa einer Naht), zählen nicht: Dort ist der
/// Bisektionspunkt bereits genau. Jedes Randstück wird im Fenster an `SNAP_PROBES` Stellen
/// abgetastet, jedes örtliche Minimum einzeln verfeinert (ein kleines Loch kreuzt den Umriss im
/// Fenster zweimal). Liefert die Stelle auf dem Umriss und den nächsten Punkt der
/// Kante (für Splines das Ende selbst); ohne Treffer (schleifender Schnitt): `None`.
fn snap_to_edge(
    fg: &FaceGeom,
    point: &dyn Fn(f64) -> DVec3,
    t_star: f64,
    h: f64,
    limit: f64,
) -> Option<(f64, DVec3)> {
    let (lo, hi) = (t_star - h, t_star + h);
    let p_star = point(t_star);
    // Reichweite des Fensters im Raum (Sehnen mit Reserve für die Krümmung): Randstücke, die
    // weiter weg sind, kann der Umriss im Fenster nicht erreichen.
    let reach = SNAP_REACH * point(lo).distance(p_star).max(point(hi).distance(p_star)) + limit;
    let probes: Vec<f64> = (0..=SNAP_PROBES)
        .map(|i| lo + (hi - lo) * i as f64 / SNAP_PROBES as f64)
        .collect();
    let mut best: Option<(f64, f64, &planora_kernel::boolean::Seg)> = None;
    for seg in fg.loops.iter().flatten() {
        let dist = |t: f64| seg.closest(point(t)).1;
        if dist(t_star) > reach {
            continue;
        }
        let ds: Vec<f64> = probes.iter().map(|&t| dist(t)).collect();
        if ds[0] <= limit || ds[SNAP_PROBES] <= limit {
            continue;
        }
        // Jedes örtliche Minimum der Probe einzeln verfeinern: Eine Randschleife (kleines Loch)
        // kann den Umriss im Fenster zweimal kreuzen.
        for i in 0..=SNAP_PROBES {
            let left = if i == 0 { f64::INFINITY } else { ds[i - 1] };
            let right = if i == SNAP_PROBES {
                f64::INFINITY
            } else {
                ds[i + 1]
            };
            if !(ds[i] <= left && ds[i] <= right) {
                continue;
            }
            let a = probes[i.saturating_sub(1)];
            let b = probes[(i + 1).min(SNAP_PROBES)];
            let (t, d) = golden_min(a, b, dist);
            if d <= limit && best.is_none_or(|(bt, _, _)| (t - t_star).abs() < (bt - t_star).abs())
            {
                best = Some((t, d, seg));
            }
        }
    }
    best.map(|(t, _, seg)| {
        let (f, _) = seg.closest(point(t));
        (t, seg.at(f))
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_wavy_outline_is_fitted_in_parts() {
        // 150 Wellen (Amplitude 0,05 mm, Wellenlänge 0,6 mm): eine einzige Anpassung bräuchte
        // mehr als die zulässigen 512 Kontrollpunkte; geteilt hält jedes Stück die Genauigkeit,
        // und die Stücke schließen lückenlos aneinander an.
        let wave = |x: f64| DVec3::new(x, 0.05 * (x * std::f64::consts::TAU / 0.6).sin(), 0.0);
        let n = 30_000;
        let len = 150.0 * 0.6;
        let pts: Vec<DVec3> = (0..=n).map(|i| wave(len * i as f64 / n as f64)).collect();
        let tol = Tolerance {
            linear: geom_tol(len),
            ..Tolerance::default()
        };
        let parts = fit_checked(&pts, &tol, 0, &Progress::new()).unwrap();
        assert!(parts.len() > 1, "{}", parts.len());
        for w in parts.windows(2) {
            assert_eq!(w[0].point(w[0].domain().1), w[1].point(w[1].domain().0));
        }
        assert_eq!(parts[0].point(parts[0].domain().0), pts[0]);
        let last = &parts[parts.len() - 1];
        assert_eq!(last.point(last.domain().1), pts[n]);
        // Eigene Prüfung: jeder Kurvenpunkt liegt höchstens SILHOUETTE_TOL neben der Welle
        // (Abstand zur Welle über die Senkrechte: |y − 0,05·sin(…x)| begrenzt ihn nach oben).
        for c in &parts {
            let (a, b) = c.domain();
            for i in 0..=2000 {
                let p = c.point(a + (b - a) * i as f64 / 2000.0);
                assert!((p.y - wave(p.x).y).abs() <= SILHOUETTE_TOL, "{p:?}");
            }
        }
    }

    /// Kubische B-Spline-Fläche (6 × 6 Kontrollpunkte, Knoten 0, ⅓, ⅔, 1) über [0, 10]²:
    /// geneigte Ebene z = a·y mit einer flachen, schiefen Beule (Kontrollpunkthöhen
    /// bx[i] · by[j]); am Rand verschwinden die Ableitungen der Beule.
    fn bump_patch(a: f64) -> NurbsSurfaceData {
        bump_patch_with(a, 0.6, 1.0)
    }

    fn bump_patch_with(a: f64, x3: f64, y3: f64) -> NurbsSurfaceData {
        let (bx, by) = ([0.0, 0.0, 1.0, x3, 0.0, 0.0], [0.0, 0.0, 1.0, y3, 0.0, 0.0]);
        let mut control = Vec::new();
        for (i, hx) in bx.iter().enumerate() {
            for (j, hy) in by.iter().enumerate() {
                let (x, y) = (10.0 * i as f64 / 5.0, 10.0 * j as f64 / 5.0);
                control.push(DVec3::new(x, y, a * y + hx * hy));
            }
        }
        let k = vec![0.0, 0.0, 0.0, 0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0, 1.0, 1.0, 1.0];
        let s = NurbsSurface::new(3, 3, k.clone(), k, 6, 6, control, vec![1.0; 36]).unwrap();
        NurbsSurfaceData::new(s, false, false).unwrap()
    }
    /// Größtes g = n·Y (eigene Normale aus den Ableitungen) und seine Stelle.
    fn g_max(data: &NurbsSurfaceData) -> (f64, DVec2) {
        let g = |u: f64, v: f64| {
            let dv = data.surface().derivs(u, v, 1);
            dv[1][0].cross(dv[0][1]).normalize().y
        };
        let mut best = (f64::NEG_INFINITY, DVec2::ZERO);
        for i in 0..=100 {
            for j in 0..=100 {
                let (u, v) = (i as f64 / 100.0, j as f64 / 100.0);
                let x = g(u, v);
                if x > best.0 {
                    best = (x, DVec2::new(u, v));
                }
            }
        }
        // Verfeinern: Koordinatensuche mit kleiner werdender Schrittweite.
        let mut h = 1.0 / 100.0;
        while h > 1e-12 {
            let mut moved = false;
            for dd in [DVec2::X, -DVec2::X, DVec2::Y, -DVec2::Y] {
                let q = (best.1 + dd * h).clamp(DVec2::ZERO, DVec2::ONE);
                let x = g(q.x, q.y);
                if x > best.0 {
                    best = (x, q);
                    moved = true;
                }
            }
            if !moved {
                h *= 0.5;
            }
        }
        best
    }

    #[test]
    fn an_outline_loop_inside_one_grid_cell_is_found_or_named() {
        // Geneigte Fläche mit flacher Beule, von vorne (d = +Y): g = n·Y ist überall negativ,
        // außer in einer kleinen Insel um die steilste Stelle der Beule (eigene Rechnung: g_max).
        // Die Neigung a wird so gewählt (Bisektion), dass die Insel in u und v nur etwa 0,7
        // Rasterzellen breit ist und kein Rasterpunkt in ihr liegt – Marching Squares allein sähe
        // sie nicht. Erwartet: die Umrissschleife wird gefunden (Verfeinerung) oder benannt.
        let cell = 1.0 / 66.0; // 3 Spannen zu je 22 Teilen
        let gy = |data: &NurbsSurfaceData, q: DVec2| {
            let dv = data.surface().derivs(q.x, q.y, 1);
            dv[1][0].cross(dv[0][1]).normalize().y
        };
        // Breite der Insel längs einer Achse durch die Stelle des Maximums.
        let width = |data: &NurbsSurfaceData, at: DVec2, axis: DVec2| {
            let mut w = 0.0;
            for s in [1.0, -1.0] {
                let (mut a, mut b) = (0.0, 0.2);
                for _ in 0..80 {
                    let m = 0.5 * (a + b);
                    if gy(data, at + axis * (s * m)) > 0.0 {
                        a = m;
                    } else {
                        b = m;
                    }
                }
                w += a;
            }
            w
        };
        let (mut lo, mut hi) = (0.2, 0.3);
        let mut chosen = None;
        for _ in 0..60 {
            let a = 0.5 * (lo + hi);
            let data = bump_patch_with(a, 0.85, 0.875);
            let (gm, at) = g_max(&data);
            if gm <= 0.0 {
                hi = a;
                continue;
            }
            let w = width(&data, at, DVec2::X).min(width(&data, at, DVec2::Y));
            if w > 0.7 * cell {
                lo = a;
            } else {
                hi = a;
            }
            chosen = Some((a, at));
        }
        let (a, at) = chosen.unwrap();
        let data = bump_patch_with(a, 0.85, 0.875);
        let (wu, wv) = (width(&data, at, DVec2::X), width(&data, at, DVec2::Y));
        assert!(wu < 1.5 * cell && wv < 1.5 * cell, "Insel {wu} × {wv}");
        // Kein Rasterpunkt der ersten Stufe liegt in der Insel.
        let (us, _) = grid_lines(data.surface().knots_u().knots(), 0.0, 1.0, 0).unwrap();
        let (vs, _) = grid_lines(data.surface().knots_v().knots(), 0.0, 1.0, 0).unwrap();
        assert_eq!(us.len(), 67);
        for &u in &us {
            for &v in &vs {
                assert!(
                    gy(&data, DVec2::new(u, v)) < 0.0,
                    "Rasterpunkt ({u}, {v}) in der Insel"
                );
            }
        }
        let mut work = Work::default();
        match outline_chains(&data, DVec3::Y, &mut work, &Progress::new()) {
            Ok(chains) => {
                assert_eq!(chains.len(), 1, "eine Schleife");
                let c = &chains[0];
                assert!(c.closed);
                for q in &c.uv {
                    assert!(
                        q.distance(at) < 1.5 * cell,
                        "{q:?} fern der Insel bei {at:?}"
                    );
                }
            }
            Err(DrawError::Unsupported(m)) => assert_eq!(m, HIDDEN_MESSAGE),
            Err(e) => panic!("{e:?}"),
        }
    }

    #[test]
    fn a_failed_outline_point_at_a_range_end_is_reported() {
        // Der Weg wird an den Bereichsenden für die Anpassung noch einmal ausgewertet. Scheitert
        // Newton erst dort (hier: die dritte Auswertung bei t = 0 meldet den Fehler), darf die
        // Kurve nicht trotzdem angepasst werden.
        let solid = planora_kernel::primitives::make_box(DVec3::ONE, &Tolerance::default())
            .expect("Würfel");
        let fid = solid.faces.iter().next().map(|(id, _)| id).expect("Fläche");
        let fg = face_geom(&solid, fid);
        let failed = std::rc::Rc::new(std::cell::Cell::new(false));
        let flag = failed.clone();
        let calls = std::cell::Cell::new(0);
        let path = OwnedPath {
            point: Box::new(move |t: f64| {
                if t == 0.0 {
                    calls.set(calls.get() + 1);
                    if calls.get() == 3 {
                        flag.set(true);
                    }
                }
                DVec3::new(t, 0.0, 0.0)
            }),
            t0: 0.0,
            t1: 1.0,
            closed: false,
            knots: vec![0.0, 1.0],
            failed: Some(failed),
        };
        let tol = Tolerance {
            linear: geom_tol(1.0),
            ..Tolerance::default()
        };
        let r = spline_pieces(
            &fg,
            &path,
            &|_| true,
            &tol,
            &mut Work::default(),
            &Progress::new(),
        );
        assert!(
            matches!(&r, Err(DrawError::Unsupported(m)) if m.contains("nicht auf dem Umriss")),
            "{:?}",
            r.as_ref().map(|v| v.len())
        );
    }

    #[test]
    fn newton_off_the_outline_is_reported_not_used() {
        // Ebene Fläche (ohne Beule, a = 0,3): g = n·Y ist überall gleich und ungleich null, ∇g = 0.
        // Newton kann keinen Umrisspunkt finden; das Ergebnis muss „keiner“ sein statt eines
        // Punkts neben dem Umriss.
        let flat = {
            let mut control = Vec::new();
            for i in 0..4 {
                for j in 0..4 {
                    let (x, y) = (10.0 * i as f64 / 3.0, 10.0 * j as f64 / 3.0);
                    control.push(DVec3::new(x, y, 0.3 * y));
                }
            }
            let k = vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
            let s = NurbsSurface::new(3, 3, k.clone(), k, 4, 4, control, vec![1.0; 16]).unwrap();
            NurbsSurfaceData::new(s, false, false).unwrap()
        };
        assert_eq!(newton_outline(&flat, DVec3::Y, DVec2::new(0.5, 0.5)), None);
        // Auf der Beulenfläche konvergiert er dagegen von einem Punkt nahe der Insel aus.
        let bump = bump_patch(0.2);
        let (_, at) = g_max(&bump);
        // Start dicht neben dem Rand der Insel (eigene Bisektion längs u).
        let gy = |q: DVec2| {
            let dv = bump.surface().derivs(q.x, q.y, 1);
            dv[1][0].cross(dv[0][1]).normalize().y
        };
        let (mut lo, mut hi) = (0.0, 0.3);
        for _ in 0..60 {
            let m = 0.5 * (lo + hi);
            if gy(at + DVec2::new(m, 0.0)) > 0.0 {
                lo = m;
            } else {
                hi = m;
            }
        }
        let start = at + DVec2::new(lo + 1e-3, 0.0);
        let q = newton_outline(&bump, DVec3::Y, start).expect("Umrisspunkt");
        let n = bump.surface().normal(q.x, q.y);
        assert!(n.y.abs() <= NEWTON_ACCEPT);
    }
}
