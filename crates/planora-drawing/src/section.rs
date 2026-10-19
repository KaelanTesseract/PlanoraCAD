//! Schnitt eines Körpers an einer Ebene (Teilprojekt 3d, Schnittansichten), Teil 1: der
//! Kernel-Schnitt. Der Kernel bleibt unverändert; benutzt werden nur `make_box`,
//! `Solid::transformed`, `assign_origin` und `boolean`.
//!
//! Festlegungen:
//! - **S1 Ebene:** `SectionPlane { kind, offset }`. `kind` ist die Normansicht, `d = Basis::of(kind).d`
//!   ist ± eine Weltachse `a`; die Ebene ist `p[a] = offset`. Behalten wird
//!   `d[a]·(p[a] − offset) ≥ 0` (Material hinter der Ebene, vom Betrachter aus).
//! - **S2 Schneidekörper:** ein Quader um die Hüllbox des Körpers, der von der Ebene in Richtung
//!   `−d` reicht. Er bekommt die Herkunft `section_origin(body) = u64::MAX − body`; Schnittflächen
//!   erkennt man an dieser Marke (nicht an ihrer Lage). Trägt ein Eingangskörper schon eine Marke
//!   in diesem Bereich, ist das ein Fehler.
//! - **S3 Ganz vor/hinter, Tangentialfall:** Liegt die Hüllbox ganz auf der behaltenen (oder
//!   entfernten) Seite, einschließlich Berührung, braucht es keinen Kernel. Liegt eine Ecke näher
//!   als 1e-4 mm an der Ebene, wird vor dem Schneiden um `SECTION_NUDGE_MM` zur behaltenen Seite
//!   verschoben. Scheitert `boolean` sonst, folgt ein zweiter Versuch mit Verschiebung. Meldet
//!   die Ansicht `AmbiguousVisibility` (Ebene haarscharf neben einer Bohrungsachse), rechnet
//!   [`compute_section`] einmal mit verschobener Ebene, wenn dieselben Körper Schnittflächen haben.
//!   „Ganz auf einer Seite“ entscheidet die sichere Hüllbox [`solid_bounds`].
//! - **Abtastgrenzen der Randschleifen:** Kreis- und Ellipsenkanten höchstens 65 536 Stücke,
//!   Freiformkanten höchstens 2049 Punkte (Verdopplung ab 64 Stücken, Halt vor 4096). Wird eine
//!   Grenze erreicht, wird das Vieleck gröber als `tol`, ohne Meldung; praktisch unerreichbar
//!   (Kreis erst ab Radius/`tol` > 8,7e8, Freiform nur bei extrem welliger Kante).

use crate::basis::Basis;
use crate::error::DrawError;
use crate::model::ViewKind;
use crate::stroke::{Stroke, StrokeOrigin};
use crate::{DrawBody, ViewLines};
use planora_exchange::Progress;
use planora_kernel::boolean::boolean;
use planora_kernel::primitives::make_box;
use planora_kernel::{BoolOp, BooleanError, Curve, Face, Id, Loop, Solid, Surface};
use planora_math::{DMat4, DVec2, DVec3, Tolerance};

/// Kleinste Herkunftsmarke, die für Schnittflächen reserviert ist.
pub const SECTION_ORIGIN_MIN: u64 = u64::MAX - 65_535;
/// Verschiebung der Ebene in Millimetern, wenn sie genau auf einer Fläche oder Kante liegt.
pub const SECTION_NUDGE_MM: f64 = 0.001;
/// Eine Ecke näher als dieser Abstand an der Ebene löst die Verschiebung aus.
const NEAR_MM: f64 = 1e-4;

/// Herkunftsmarke der Schnittflächen des Körpers mit dem Index `body`.
pub fn section_origin(body: usize) -> u64 {
    u64::MAX - body as u64
}

/// Schnittebene: Normansicht und Lage der Ebene auf der Weltachse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectionPlane {
    pub kind: ViewKind,
    pub offset: f64,
}

impl SectionPlane {
    /// Achse `a` der Ebene und Vorzeichen `d[a]` der Blickrichtung; `None` für die
    /// isometrische Ansicht oder einen nicht endlichen Versatz.
    pub fn axis(&self) -> Option<(usize, f64)> {
        if self.kind == ViewKind::Isometric || !self.offset.is_finite() {
            return None;
        }
        let d = Basis::of(self.kind).d;
        (0..3).find_map(|a| (d[a].abs() > 0.5).then(|| (a, d[a].signum())))
    }
}

/// Ergebnis des Schnitts eines Körpers.
#[derive(Clone, Debug)]
pub enum CutOutcome {
    /// Der Körper liegt ganz auf der behaltenen Seite.
    Kept,
    /// Der Körper liegt ganz auf der entfernten Seite (oder bleibt nichts übrig).
    Removed,
    /// Der geschnittene Körper; `nudged`: die Ebene wurde um `SECTION_NUDGE_MM` verschoben.
    Cut { solid: Solid, nudged: bool },
}

/// Hüllbox des Körpers, die ihn sicher ganz enthält; `None` bei leerem Körper oder nicht
/// endlichen Werten.
///
/// Sie entscheidet, ob ein Körper ohne Kernel ganz behalten oder ganz entfernt wird. Eine zu
/// kleine Box (früher: 64 Stützpunkte je Kante, bei Kreisen bis etwa 1,2e-3 · Radius zu klein)
/// ließe einen knapp angeschnittenen großen Körper still verschwinden oder ungeschnitten. Daher:
/// Geraden über ihre Enden, Kreis- und Ellipsenbögen genau (Enden und Scheitel im Bereich),
/// Freiformkanten über ihre Kontrollpunkte (enthalten die Kurve sicher); dazu Kugel, Torus und
/// Freiformflächen, deren äußerste Punkte im Innern der Fläche liegen können, über eine sichere
/// Box. Ebene, Zylinder und Kegel erreichen ihr Äußerstes immer am Rand.
pub fn solid_bounds(solid: &Solid) -> Option<(DVec3, DVec3)> {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    let mut add = |a: DVec3, b: DVec3| {
        lo = lo.min(a);
        hi = hi.max(b);
    };
    for (_, v) in solid.vertices.iter() {
        add(v.point, v.point);
    }
    for (_, e) in solid.edges.iter() {
        let (a, b) = curve_bounds(&e.curve, e.t0, e.t1);
        add(a, b);
    }
    for (_, f) in solid.faces.iter() {
        if let Some((a, b)) = surface_bounds(&f.surface) {
            add(a, b);
        }
    }
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// Sichere Box eines Kurvenstücks `t0 … t1` (siehe [`solid_bounds`]).
fn curve_bounds(curve: &Curve, t0: f64, t1: f64) -> (DVec3, DVec3) {
    let (p0, p1) = (curve.point(t0), curve.point(t1));
    let (mut lo, mut hi) = (p0.min(p1), p0.max(p1));
    let (frame, ax, ay) = match curve {
        Curve::Line { .. } => return (lo, hi),
        Curve::Circle { frame, radius } => (frame, *radius, *radius),
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => (frame, *major, *minor),
        Curve::Nurbs(c) => {
            for p in c.control_points() {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
            return (lo, hi);
        }
    };
    // Koordinate k: o + ax·x[k]·cos t + ay·y[k]·sin t = o + A·cos(t − φ); Scheitel bei t = φ + mπ.
    let (tmin, tmax) = (t0.min(t1), t0.max(t1));
    for k in 0..3 {
        let (c, s) = (ax * frame.x[k], ay * frame.y[k]);
        let amp = c.hypot(s);
        if tmax - tmin >= std::f64::consts::TAU {
            lo[k] = lo[k].min(frame.origin[k] - amp);
            hi[k] = hi[k].max(frame.origin[k] + amp);
            continue;
        }
        let phi = s.atan2(c);
        let first = ((tmin - phi) / std::f64::consts::PI).ceil() as i64;
        let last = ((tmax - phi) / std::f64::consts::PI).floor() as i64;
        for m in first..=last.min(first + 2) {
            let v = frame.origin[k] + amp * (m as f64 * std::f64::consts::PI).cos();
            lo[k] = lo[k].min(v);
            hi[k] = hi[k].max(v);
        }
    }
    (lo, hi)
}

/// Sichere Box für Flächen, deren äußerste Punkte im Innern liegen können; sonst `None`.
fn surface_bounds(surface: &Surface) -> Option<(DVec3, DVec3)> {
    match surface {
        Surface::Sphere { frame, radius } => {
            let r = DVec3::splat(radius.abs());
            Some((frame.origin - r, frame.origin + r))
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            // Ausdehnung längs Achse k: Ring R·√(1 − z[k]²) plus Rohr r (für den ganzen Torus genau).
            let mut ext = DVec3::ZERO;
            for k in 0..3 {
                ext[k] =
                    major.abs() * (1.0 - frame.z[k] * frame.z[k]).max(0.0).sqrt() + minor.abs();
            }
            Some((frame.origin - ext, frame.origin + ext))
        }
        Surface::Nurbs(d) => Some(d.control_bounds()),
        Surface::Plane { .. } | Surface::Cylinder { .. } | Surface::Cone { .. } => None,
    }
}

/// Flächen mit Herkunft ab `SECTION_ORIGIN_MIN`, aufsteigend nach Id.
pub fn cut_faces(solid: &Solid) -> Vec<Id<Face>> {
    solid
        .faces
        .iter()
        .filter(|(_, f)| {
            f.tag
                .as_ref()
                .is_some_and(|t| t.origin >= SECTION_ORIGIN_MIN)
        })
        .map(|(id, _)| id)
        .collect()
}

/// Schneidet `solid` (der `body`-te exakte Körper) an `plane` und behält die Seite nach S1.
pub fn cut_body(
    solid: &Solid,
    body: usize,
    plane: &SectionPlane,
    tol: &Tolerance,
) -> Result<CutOutcome, String> {
    let (a, da) = plane
        .axis()
        .ok_or_else(|| "keine gültige Schnittebene".to_string())?;
    if body as u64 > u64::MAX - SECTION_ORIGIN_MIN {
        return Err("zu viele Körper für die Schnittmarken".into());
    }
    if !cut_faces(solid).is_empty() {
        return Err("der Körper trägt schon eine Schnittmarke".into());
    }
    let (lo, hi) =
        solid_bounds(solid).ok_or_else(|| "leerer oder ungültiger Körper".to_string())?;

    let side = |off: f64| classify(lo[a], hi[a], da, off);
    match side(plane.offset) {
        Side::Kept => return Ok(CutOutcome::Kept),
        Side::Removed => return Ok(CutOutcome::Removed),
        Side::Cut => {}
    }

    let near = solid
        .vertices
        .iter()
        .any(|(_, v)| (v.point[a] - plane.offset).abs() < NEAR_MM);
    let mut nudged = near;
    let mut offset = plane.offset;
    if nudged {
        offset += da * SECTION_NUDGE_MM;
    }
    loop {
        match side(offset) {
            Side::Kept => return Ok(CutOutcome::Kept),
            Side::Removed => return Ok(CutOutcome::Removed),
            Side::Cut => {}
        }
        let cutter = cutter_box(lo, hi, a, da, offset, body)?;
        match boolean(solid, &cutter, BoolOp::Difference, tol) {
            Ok(out) => return Ok(CutOutcome::Cut { solid: out, nudged }),
            Err(BooleanError::EmptyResult) => return Ok(CutOutcome::Removed),
            Err(BooleanError::InvalidInput(m)) => return Err(m),
            Err(e) => {
                if nudged {
                    return Err(e.to_string());
                }
                nudged = true;
                offset = plane.offset + da * SECTION_NUDGE_MM;
            }
        }
    }
}

enum Side {
    Kept,
    Removed,
    Cut,
}

/// Ordnet die Spanne `[lo, hi]` auf der Achse der Ebene ein (Berührung zählt als ganz auf
/// einer Seite).
fn classify(lo: f64, hi: f64, da: f64, offset: f64) -> Side {
    let (smin, smax) = if da > 0.0 {
        (lo - offset, hi - offset)
    } else {
        (offset - hi, offset - lo)
    };
    if smin >= 0.0 {
        Side::Kept
    } else if smax <= 0.0 {
        Side::Removed
    } else {
        Side::Cut
    }
}

/// Schneidequader S2: quer zur Achse um die Mitte der Hüllbox, längs der Achse von der Ebene in
/// Richtung `−d`.
fn cutter_box(
    lo: DVec3,
    hi: DVec3,
    a: usize,
    da: f64,
    offset: f64,
    body: usize,
) -> Result<Solid, String> {
    let center = (lo + hi) * 0.5;
    let size = hi - lo;
    let mut dims = DVec3::ZERO;
    let mut min = DVec3::ZERO;
    for k in 0..3 {
        if k == a {
            let len = 2.0 * size[k].max(1.0) + (offset - center[k]).abs();
            dims[k] = len;
            min[k] = if da > 0.0 { offset - len } else { offset };
        } else {
            let half = size[k].max(1.0);
            dims[k] = 2.0 * half;
            min[k] = center[k] - half;
        }
    }
    let mut b = make_box(dims, &Tolerance::default())
        .map_err(|e| e.to_string())?
        .transformed(&DMat4::from_translation(min))
        .map_err(|e| e.to_string())?;
    b.assign_origin(section_origin(body));
    Ok(b)
}

/// Abtasttoleranz, wenn der übergebene Wert nicht brauchbar ist.
const FALLBACK_TOL: f64 = 1e-6;
/// Höchstzahl der Stücke je Kreis- oder Ellipsenkante (Schutz vor unendlichen Schleifen).
const MAX_ARC_PIECES: usize = 1 << 16;
/// Anfangszahl der Stücke einer Freiformkante.
const FREEFORM_START: usize = 64;
/// Höchstzahl der Punkte je Freiformkante.
const FREEFORM_MAX_POINTS: usize = 4096;

/// Eine Schnittfläche eines Körpers mit ihren Randschleifen (äußere zuerst, dann innere).
#[derive(Clone, Debug)]
pub struct SectionRegion {
    pub body: usize,
    /// Je Schleife die Kanten in Laufrichtung.
    pub loops: Vec<Vec<Stroke>>,
}

fn loop_strokes(solid: &Solid, lp: &Loop, body: usize) -> Vec<Stroke> {
    lp.half_edges
        .iter()
        .map(|he| {
            let e = &solid.edges[he.edge];
            let (t0, t1) = if he.forward {
                (e.t0, e.t1)
            } else {
                (e.t1, e.t0)
            };
            Stroke {
                curve: e.curve.clone(),
                t0,
                t1,
                origin: StrokeOrigin::Edge {
                    body,
                    edge: he.edge.index(),
                },
            }
        })
        .collect()
}

/// Eine Region je Schnittfläche des geschnittenen Körpers `cut` (`body` ist sein Index unter
/// den exakten Körpern).
pub fn section_regions(cut: &Solid, body: usize) -> Vec<SectionRegion> {
    cut_faces(cut)
        .into_iter()
        .map(|fid| {
            let face = &cut.faces[fid];
            let loops = std::iter::once(&face.outer)
                .chain(face.inner.iter())
                .map(|l| loop_strokes(cut, l, body))
                .collect();
            SectionRegion { body, loops }
        })
        .collect()
}

/// Vieleck einer Schleife in Ansichtskoordinaten, ohne doppelten Schlusspunkt. `tol` (mm) ist
/// der größte Abstand der Sehnen von der Kurve; ungültige Werte zählen wie 1e-6.
pub fn loop_polygon(lp: &[Stroke], basis: &Basis, tol: f64) -> Vec<DVec2> {
    let tol = if tol.is_finite() && tol > 0.0 {
        tol
    } else {
        FALLBACK_TOL
    };
    let mut out = Vec::new();
    for s in lp {
        let ts = sample_params(s, tol);
        // Der letzte Punkt ist der erste der nächsten Kante.
        for &t in &ts[..ts.len() - 1] {
            out.push(s.point2(basis, t));
        }
    }
    out
}

/// Parameter der Abtastung einer Kante, einschließlich beider Enden (mindestens 2 Werte).
fn sample_params(s: &Stroke, tol: f64) -> Vec<f64> {
    let span = s.t1 - s.t0;
    let at = |n: usize| -> Vec<f64> {
        (0..=n)
            .map(|i| {
                if i == n {
                    s.t1
                } else {
                    s.t0 + span * i as f64 / n as f64
                }
            })
            .collect()
    };
    match &s.curve {
        Curve::Line { .. } => at(1),
        Curve::Circle { radius, .. } => at(arc_pieces(span.abs(), radius.abs(), tol)),
        Curve::Ellipse { major, minor, .. } => {
            at(arc_pieces(span.abs(), major.abs().max(minor.abs()), tol))
        }
        Curve::Nurbs(_) => {
            let mut n = FREEFORM_START;
            loop {
                let ts = at(n);
                if n * 2 >= FREEFORM_MAX_POINTS || chords_within(s, &ts, tol) {
                    return ts;
                }
                n *= 2;
            }
        }
    }
}

/// Stückzahl für Kreis und Ellipse nach Sehnenabstand `tol` beim größten Radius.
fn arc_pieces(span: f64, r_max: f64, tol: f64) -> usize {
    if !(span.is_finite() && r_max.is_finite()) || r_max <= 0.0 {
        return 1;
    }
    let step = 2.0 * (1.0 - tol / r_max).clamp(-1.0, 1.0).acos();
    if step.is_nan() || step <= 0.0 {
        return MAX_ARC_PIECES;
    }
    let n = (span / step).ceil();
    if n.is_finite() {
        (n as usize).clamp(1, MAX_ARC_PIECES)
    } else {
        MAX_ARC_PIECES
    }
}

/// Liegt der Kurvenpunkt in der Mitte jedes Stücks höchstens `tol` von dessen Sehne?
fn chords_within(s: &Stroke, ts: &[f64], tol: f64) -> bool {
    ts.windows(2).all(|w| {
        let (a, b) = (s.point(w[0]), s.point(w[1]));
        let m = s.point(0.5 * (w[0] + w[1]));
        let ab = b - a;
        let len2 = ab.length_squared();
        let u = if len2 > 0.0 {
            ((m - a).dot(ab) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (m - (a + ab * u)).length() <= tol
    })
}

/// Vorzeichenbehaftete Fläche (Gaußsche Trapezformel); gegen den Uhrzeigersinn positiv.
pub fn polygon_area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (p, q) = (poly[i], poly[(i + 1) % n]);
            p.x * q.y - q.x * p.y
        })
        .sum::<f64>()
        / 2.0
}

/// Ansichtsrechnung (gleiche Form wie `compute_view`; die App und Tests dürfen eine Ersatzrechnung
/// einsetzen).
pub type ViewFn = fn(&[DrawBody], ViewKind, &Progress) -> Result<ViewLines, DrawError>;

/// Die geschnittenen Körper und ihre Schnittflächen.
#[derive(Clone, Debug)]
pub struct SectionData {
    /// `body` = ursprünglicher Index.
    pub regions: Vec<SectionRegion>,
    /// Geschnittene bzw. behaltene Körper, Reihenfolge = Körper der Ansicht.
    pub solids: Vec<Solid>,
    /// `solids[k]` stammt von `bodies[source[k]]`.
    pub source: Vec<usize>,
    /// Die Ebene wurde bei mindestens einem Körper um `SECTION_NUDGE_MM` verschoben.
    pub nudged: bool,
}

/// Linien der Schnittansicht mit den Daten der Schnittflächen.
pub struct SectionLines {
    pub lines: ViewLines,
    pub data: SectionData,
}

/// Schneidet jeden Körper an `plane` (S4) und rechnet die Ansicht `plane.kind` mit `view` aus
/// den behaltenen Körpern. Fortschritt: ein Schritt je Körper vor der Ansichtsrechnung.
pub fn compute_section(
    bodies: &[DrawBody],
    plane: &SectionPlane,
    view: ViewFn,
    progress: &Progress,
) -> Result<SectionLines, DrawError> {
    if bodies.is_empty() {
        return Err(DrawError::NoBodies);
    }
    if !plane.offset.is_finite() {
        return Err(DrawError::NonFinite);
    }
    if plane.axis().is_none() {
        return Err(DrawError::Unsupported(
            "Ein Schnitt braucht eine der sechs Normansichten.".into(),
        ));
    }
    progress.set_total(bodies.len() as u64);
    let data = cut_all(bodies, plane, progress)?;
    match view_of(bodies, &data, plane.kind, view, progress) {
        Ok(lines) => Ok(SectionLines { lines, data }),
        // Knapp neben einer Bohrungsachse (etwa 5e-5 bis 1e-3 mm) liegen Kante der Schnittfläche
        // und Umriss der Bohrung so dicht, dass die Ansicht nicht sicher entscheiden kann, was
        // verdeckt ist. Dann einmal mit der um `SECTION_NUDGE_MM` zur behaltenen Seite
        // verschobenen Ebene rechnen (wie im Tangentialfall S3). Das gilt nur, wenn dabei
        // dieselben Körper Schnittflächen haben (sonst fiele ein knapp angeschnittener Körper
        // still weg); sonst und wenn auch das nicht geht, bleibt die erste Meldung.
        Err(first @ DrawError::AmbiguousVisibility(_)) => {
            let (_, da) = plane.axis().expect("oben geprüft");
            let shifted = SectionPlane {
                kind: plane.kind,
                offset: plane.offset + da * SECTION_NUDGE_MM,
            };
            let mut again = cut_all(bodies, &shifted, progress)?;
            if again.source != data.source || cut_bodies_of(&again) != cut_bodies_of(&data) {
                return Err(first);
            }
            match view_of(bodies, &again, plane.kind, view, progress) {
                Ok(lines) => {
                    again.nudged = true;
                    Ok(SectionLines { lines, data: again })
                }
                Err(DrawError::AmbiguousVisibility(_)) => Err(first),
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// Indizes der Körper mit Schnittfläche, aufsteigend ohne Doppelte.
fn cut_bodies_of(data: &SectionData) -> Vec<usize> {
    let mut v: Vec<usize> = data.regions.iter().map(|g| g.body).collect();
    v.dedup();
    v.sort_unstable();
    v.dedup();
    v
}

/// Ansicht der geschnittenen bzw. behaltenen Körper.
fn view_of(
    bodies: &[DrawBody],
    data: &SectionData,
    kind: ViewKind,
    view: ViewFn,
    progress: &Progress,
) -> Result<ViewLines, DrawError> {
    let cut_bodies: Vec<DrawBody> = data
        .solids
        .iter()
        .zip(&data.source)
        .map(|(s, &i)| DrawBody {
            name: bodies[i].name.clone(),
            solid: s,
        })
        .collect();
    view(&cut_bodies, kind, progress)
}

/// Schneidet alle Körper an genau dieser Ebene (S4).
fn cut_all(
    bodies: &[DrawBody],
    plane: &SectionPlane,
    progress: &Progress,
) -> Result<SectionData, DrawError> {
    let tol = Tolerance::default();
    let mut solids = Vec::new();
    let mut source = Vec::new();
    let mut regions = Vec::new();
    let mut nudged = false;
    for (i, b) in bodies.iter().enumerate() {
        progress.check()?;
        let out = cut_body(b.solid, i, plane, &tol)
            .map_err(|_| DrawError::SectionFailed(b.name.clone()))?;
        match out {
            CutOutcome::Kept => {
                solids.push(b.solid.clone());
                source.push(i);
            }
            CutOutcome::Removed => {}
            CutOutcome::Cut { solid, nudged: n } => {
                nudged |= n;
                regions.extend(section_regions(&solid, i));
                solids.push(solid);
                source.push(i);
            }
        }
        progress.advance(1);
    }
    progress.check()?;
    if regions.is_empty() {
        return Err(DrawError::SectionMisses);
    }
    Ok(SectionData {
        regions,
        solids,
        source,
        nudged,
    })
}
