//! Drehprofil (Skizzen 11b, Spec 5): Auswahl, Achse und Einstellungen als Herkunft
//! [`crate::CopySource::SpunProfile`] und die Rechnung (Task 13).
//!
//! Jeder Punkt der Auswahl wird um die Achse in die Halbebene der Skizze gedreht: Die Lage
//! entlang der Achse bleibt, der Abstand zur Achse bleibt. Für Körper ist das Ergebnis der
//! äußere Umriss im Meridianschnitt; das geht in dieser Fassung nur für Drehkörper um die Achse
//! (Spec-Entscheidung 6, ganz oder gar nicht).

use crate::model::{BodyRef, FeatureId, FeatureKind};
use crate::planes::AxisRef;
use crate::refs::resolve_edge;
use crate::surface_project::{CurveSource, DocView, Source, fit, resolve_source};
use crate::{EdgeRef, VertexRef};
use planora_i18n::tr;
use planora_kernel::{Curve, Solid, Surface, section_by_plane};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::CurveId;
use planora_sketch::helpers::{Included, include_curve};

/// Was gedreht wird: ganze Körper oder bestimmte Elemente (Kanten, Ecken, Skizzenkurven).
#[derive(Clone, Debug, PartialEq)]
pub enum SpunSelection {
    Bodies(Vec<BodyRef>),
    Elements(Vec<CurveSource>),
}

/// Drehachse: eine Linie der eigenen Skizze oder ein Bezug (Ursprungsachse, gerade Kante,
/// Linie einer früheren Skizze).
#[derive(Clone, Debug, PartialEq)]
pub enum SpunAxis {
    Own(CurveId),
    Ref(AxisRef),
}

/// „Drehprofil“: Auswahl, Achse, Toleranz (mm) für angenäherte Kurven und „Umkehren“ (Ergebnis
/// auf der anderen Seite der Achse).
#[derive(Clone, Debug, PartialEq)]
pub struct SpunSpec {
    pub selection: SpunSelection,
    pub axis: SpunAxis,
    pub tolerance: f64,
    pub flip: bool,
}

fn edge_origins(e: &EdgeRef, out: &mut Vec<FeatureId>) {
    out.push(FeatureId(e.a.origin));
    out.push(FeatureId(e.b.origin));
}

impl SpunSpec {
    /// Feste Verweise: Körper-Features, Kanten und Ecken der Auswahl, die Achsenkante (siehe
    /// [`crate::CopySource::origins`]).
    pub(crate) fn origins(&self, out: &mut Vec<FeatureId>) {
        match &self.selection {
            SpunSelection::Bodies(bodies) => out.extend(bodies.iter().map(|b| b.feature)),
            SpunSelection::Elements(items) => {
                for s in items {
                    match s {
                        CurveSource::Edge(e) | CurveSource::Vertex(VertexRef { edge: e, .. }) => {
                            edge_origins(e, out)
                        }
                        CurveSource::SketchCurve { .. } | CurveSource::ConstructionPoint(_) => {}
                    }
                }
            }
        }
        if let SpunAxis::Ref(AxisRef::Edge(e)) = &self.axis {
            edge_origins(e, out);
        }
    }

    /// Lose Verweise: Quellskizzen der Elemente und der Achse, Konstruktionspunkte (siehe
    /// [`crate::CopySource::loose_origins`]).
    pub(crate) fn loose_origins(&self) -> Vec<FeatureId> {
        let mut out = Vec::new();
        if let SpunSelection::Elements(items) = &self.selection {
            for s in items {
                match s {
                    CurveSource::SketchCurve { sketch, .. } => out.push(*sketch),
                    CurveSource::ConstructionPoint(id) => out.push(*id),
                    CurveSource::Edge(_) | CurveSource::Vertex(_) => {}
                }
            }
        }
        if let SpunAxis::Ref(AxisRef::SketchLine { sketch, .. }) = &self.axis {
            out.push(*sketch);
        }
        out
    }
}

/// Warum ein Drehprofil nichts ergibt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpunError {
    /// Die Achse liegt weder in der Skizzenebene noch parallel zu ihr.
    AxisNotInPlane,
    /// Ein gewählter Körper ist kein Drehkörper um die Achse (Abweichung, Spec 5).
    NotRevolved,
    /// Achse, Körper oder Element sind nicht mehr vorhanden.
    Lost,
    /// Die Auswahl ergibt keine Geometrie.
    Nothing,
}

impl SpunError {
    /// Meldung in der aktuellen Sprache (die ersten beiden wörtlich aus der Spec).
    pub fn message(self) -> String {
        match self {
            SpunError::AxisNotInPlane => tr!("sketch-spun-axis-off-plane"),
            SpunError::NotRevolved => tr!("sketch-spun-not-revolved"),
            SpunError::Lost => tr!("sketch-spun-lost"),
            SpunError::Nothing => tr!("sketch-spun-nothing"),
        }
    }
}

impl std::fmt::Display for SpunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for SpunError {}

/// Grenze für „Achse in der Skizzenebene oder parallel“ zwischen Einheitsvektoren.
const REL: f64 = 1e-9;
/// Kleinste Winkelgrenze für „koaxial“, wie `COAXIAL_ANGLE` in `planora-kernel/src/blend/
/// analytic.rs` (dort nicht öffentlich).
const COAXIAL_ANGLE: f64 = 1e-9;
/// Abtastpunkte je Kurvenstück für die Nullstellen beim Beschneiden auf die Halbebene.
const CLIP_SAMPLES: usize = 64;
/// Anfangs- und Höchstzahl Abtastpunkte für eine angenäherte Bildkurve.
const SPLINE_SAMPLES: (usize, usize) = (65, 1025);

/// Die Abbildung der Drehung: Achse (Punkt `a`, Einheitsrichtung `d`), Fußpunkt `a2` der Achse
/// in der Skizzenebene und Richtung `n` der Halbebene (in der Ebene, senkrecht zur Achse).
struct Spin {
    a: DVec3,
    a2: DVec3,
    d: DVec3,
    n: DVec3,
    frame: Frame,
}

impl Spin {
    /// Lage entlang der Achse und Abstand zur Achse.
    fn sr(&self, p: DVec3) -> (f64, f64) {
        let v = p - self.a;
        let s = v.dot(self.d);
        (s, (v - self.d * s).length())
    }

    /// Bild in Skizzenkoordinaten.
    fn image(&self, p: DVec3) -> DVec3 {
        let (s, r) = self.sr(p);
        let mut q = self.frame.to_local(self.a2 + self.d * s + self.n * r);
        q.z = 0.0;
        q
    }

    /// Signierter Abstand zur Achse in Richtung der Halbebene (nur in der Meridianebene sinnvoll).
    fn side(&self, p: DVec3) -> f64 {
        (p - self.a).dot(self.n)
    }
}

/// Das Drehprofil `spec` in der Skizze von `view` (Ergebnis in Skizzenkoordinaten, alle z = 0).
/// `own_axis` ist bei [`SpunAxis::Own`] Punkt und Richtung der Achslinie aus der gespeicherten
/// Skizze in Weltkoordinaten (`None` = verloren). Bestimmte Elemente gehen immer; Körper nur,
/// wenn jede Fläche eine Ebene senkrecht zur Achse oder eine zur Achse koaxiale Drehfläche ist.
pub fn spun_profile(
    view: &DocView<'_>,
    spec: &SpunSpec,
    own_axis: Option<(DVec3, DVec3)>,
    tol: &Tolerance,
) -> Result<Vec<Included>, SpunError> {
    let doc = view.doc;
    let visible = doc.visible_at(view.at);
    let solids: Vec<&Solid> = visible.iter().filter_map(|b| doc.body(*b)).collect();
    let (a, d) = resolve_spin_axis(view, &spec.axis, own_axis, &solids, tol)?;
    let frame = view.frame;
    if d.dot(frame.z).abs() > REL {
        return Err(SpunError::AxisNotInPlane);
    }
    let a2 = a - frame.z * (a - frame.origin).dot(frame.z);
    let n = frame.z.cross(d).normalize();
    let spin = Spin {
        a,
        a2,
        d,
        n: if spec.flip { -n } else { n },
        frame,
    };
    let out = match &spec.selection {
        SpunSelection::Bodies(bodies) => {
            let mut chosen = Vec::new();
            for b in bodies {
                if !visible.contains(b) {
                    return Err(SpunError::Lost);
                }
                chosen.push(doc.body(*b).ok_or(SpunError::Lost)?);
            }
            if !chosen.iter().all(|s| revolved(s, a, d, tol)) {
                return Err(SpunError::NotRevolved);
            }
            body_outline(&spin, &chosen, tol)?
        }
        SpunSelection::Elements(items) => {
            let mut out = Vec::new();
            for s in items {
                match resolve_source(view, &solids, s, tol).ok_or(SpunError::Lost)? {
                    Source::Point(p) => out.push(Included::Point(spin.image(p))),
                    Source::Curve(c, t0, t1) => {
                        element_curve(&spin, &c, t0, t1, spec.tolerance, tol, &mut out)
                    }
                }
            }
            out
        }
    };
    if out.is_empty() {
        Err(SpunError::Nothing)
    } else {
        Ok(out)
    }
}

/// Punkt und Einheitsrichtung der Achse. Eine Linie einer Skizze gilt nur, wenn die Skizze vor
/// der Skizze im Verlauf steht (Spec 10); eine Kante muss gerade sein.
fn resolve_spin_axis(
    view: &DocView<'_>,
    axis: &SpunAxis,
    own_axis: Option<(DVec3, DVec3)>,
    solids: &[&Solid],
    tol: &Tolerance,
) -> Result<(DVec3, DVec3), SpunError> {
    let lost = SpunError::Lost;
    let (a, d) = match axis {
        SpunAxis::Own(_) => own_axis.ok_or(lost)?,
        SpunAxis::Ref(AxisRef::Edge(e)) => {
            let found = resolve_edge(solids, e, tol).map_err(|_| lost)?;
            let solid = solids[found.body];
            let edge = solid.edges.get(found.edge).ok_or(lost)?;
            let Curve::Line { .. } = edge.curve else {
                return Err(lost);
            };
            let p0 = solid.vertices.get(edge.start).ok_or(lost)?.point;
            let p1 = solid.vertices.get(edge.end).ok_or(lost)?.point;
            (p0, p1 - p0)
        }
        SpunAxis::Ref(r) => {
            if let AxisRef::SketchLine { sketch, .. } = r {
                let i = view.doc.index_of(*sketch).ok_or(lost)?;
                if i >= view.at || !matches!(view.doc.features[i].kind, FeatureKind::Sketch(_)) {
                    return Err(lost);
                }
            }
            let (a, d, _) =
                crate::planes::resolve_axis(view.doc, r, solids, tol).map_err(|_| lost)?;
            (a, d)
        }
    };
    let d = d.normalize_or_zero();
    if !(a.is_finite() && d.is_finite()) || d == DVec3::ZERO {
        return Err(lost);
    }
    Ok((a, d))
}

/// Ist jede Fläche eine Ebene senkrecht zur Achse oder eine zur Achse koaxiale Zylinder-, Kegel-,
/// Kugel- oder Torusfläche (Rahmen-z parallel zur Achse, Ursprung auf der Achse)? Grenzen wie
/// `revolves_about` im Kernel: Abstand von der Achse höchstens `tol.linear`, Richtungsabweichung
/// über die Körpergröße höchstens `tol.linear` (mindestens [`COAXIAL_ANGLE`]). So geht auch eine
/// Achse aus einer Skizzenlinie mit dem Rest des Lösers (Korrektur I1).
fn revolved(solid: &Solid, a: DVec3, d: DVec3, tol: &Tolerance) -> bool {
    let size = crate::plane_rect::solid_bounds(solid)
        .map(|(lo, hi)| lo.distance(hi))
        .unwrap_or(0.0)
        .max(
            solid
                .faces
                .iter()
                .map(|(_, f)| reach(&f.surface))
                .fold(0.0, f64::max),
        )
        .max(1.0);
    let lin = tol.linear;
    let angle = (lin / size).max(COAXIAL_ANGLE);
    let parallel = |z: DVec3| z.cross(d).length() <= angle;
    let on_axis = |p: DVec3| {
        let v = p - a;
        (v - d * v.dot(d)).length() <= lin
    };
    solid.faces.iter().all(|(_, f)| match &f.surface {
        Surface::Plane { frame } => parallel(frame.z),
        Surface::Cylinder { frame, .. }
        | Surface::Cone { frame, .. }
        | Surface::Torus { frame, .. } => parallel(frame.z) && on_axis(frame.origin),
        Surface::Sphere { frame, .. } => on_axis(frame.origin),
        Surface::Nurbs(_) => false,
    })
}

/// Grobe Ausdehnung einer Fläche (für die relative Toleranz, auch ohne Netz).
fn reach(s: &Surface) -> f64 {
    let o = s.frame().origin.length();
    o + match s {
        Surface::Cylinder { radius, .. } | Surface::Sphere { radius, .. } => *radius,
        Surface::Cone { radius, .. } => radius.abs(),
        Surface::Torus { major, minor, .. } => major + minor,
        Surface::Plane { .. } | Surface::Nurbs(_) => 0.0,
    }
}

/// Ein Kurvenstück `curve(t0 … t1)` des Meridianschnitts.
struct Arm {
    curve: Curve,
    t0: f64,
    t1: f64,
}

impl Arm {
    fn at(&self, f: f64) -> DVec3 {
        self.curve.point(self.t0 + (self.t1 - self.t0) * f)
    }
}

/// Äußerer Umriss der Drehkörper: Meridianschnitt, auf die Halbebene beschnitten, ohne Stücke
/// auf der Achse und ohne Schleifen, die ganz in einer anderen liegen.
fn body_outline(
    spin: &Spin,
    solids: &[&Solid],
    tol: &Tolerance,
) -> Result<Vec<Included>, SpunError> {
    let plane = Frame {
        origin: spin.a,
        x: spin.d,
        y: spin.n,
        z: spin.d.cross(spin.n),
    };
    let lin = tol.linear.max(1e-9);
    let mut arms: Vec<Arm> = Vec::new();
    for solid in solids {
        let pieces =
            section_by_plane(solid, &plane, None, tol).map_err(|_| SpunError::NotRevolved)?;
        for p in pieces {
            clip_half(spin, p.curve, p.t0, p.t1, lin, &mut arms);
        }
    }
    let chains = chain(&arms, lin * 10.0);
    // Schleifen als Vielecke in (s, r); offene über die Gerade zwischen ihren Enden geschlossen.
    let polys: Vec<Vec<DVec2>> = chains
        .iter()
        .map(|c| {
            c.iter()
                .flat_map(|&(i, fwd)| {
                    (0..16).map(move |k| {
                        let f = k as f64 / 16.0;
                        (i, if fwd { f } else { 1.0 - f })
                    })
                })
                .map(|(i, f)| {
                    let (s, r) = spin.sr(arms[i].at(f));
                    DVec2::new(s, r)
                })
                .collect()
        })
        .collect();
    let probe = |c: &[(usize, bool)]| {
        let (s, r) = spin.sr(arms[c[0].0].at(0.5));
        DVec2::new(s, r)
    };
    // Bild = Verschiebung der Meridianebene in die Skizzenebene (beide parallel).
    let shifted = Frame {
        origin: spin.frame.origin - (spin.a2 - spin.a),
        ..spin.frame
    };
    let mut out = Vec::new();
    for (ci, c) in chains.iter().enumerate() {
        let inside = polys
            .iter()
            .enumerate()
            .any(|(pi, poly)| pi != ci && contains(poly, probe(c)));
        if inside {
            continue;
        }
        for &(i, _) in c {
            let arm = &arms[i];
            if let Ok(mut g) = include_curve(&shifted, &arm.curve, arm.t0, arm.t1) {
                flatten(&mut g);
                out.push(g);
            }
        }
    }
    Ok(merge(out, lin * 10.0))
}

/// Beschneidet `curve(t0 … t1)` auf die Halbebene `side ≥ 0`; Stücke auf der Achse fallen weg.
fn clip_half(spin: &Spin, curve: Curve, t0: f64, t1: f64, lin: f64, out: &mut Vec<Arm>) {
    let y = |t: f64| spin.side(curve.point(t));
    let ts: Vec<f64> = (0..=CLIP_SAMPLES)
        .map(|k| t0 + (t1 - t0) * k as f64 / CLIP_SAMPLES as f64)
        .collect();
    // Schnittstellen mit der Achse (Vorzeichenwechsel, per Halbierung verfeinert).
    let mut cuts = vec![t0];
    for w in ts.windows(2) {
        let (mut lo, mut hi) = (w[0], w[1]);
        let (ylo, yhi) = (y(lo), y(hi));
        if ylo.abs() <= lin || yhi.abs() <= lin || ylo.signum() == yhi.signum() {
            if yhi.abs() <= lin && w[1] != t1 {
                cuts.push(w[1]);
            }
            continue;
        }
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if y(mid).signum() == ylo.signum() {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        cuts.push(0.5 * (lo + hi));
    }
    cuts.push(t1);
    cuts.dedup_by(|b, a| (*b - *a).abs() <= f64::EPSILON * (1.0 + a.abs()));
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if (b - a).abs() <= 1e-12 * (1.0 + a.abs()) {
            continue;
        }
        // Ganz auf der Achse oder auf der anderen Seite: weg.
        let mids = [0.25, 0.5, 0.75].map(|f| y(a + (b - a) * f));
        if mids.iter().all(|m| m.abs() <= lin) || mids.iter().any(|m| *m < -lin) {
            continue;
        }
        out.push(Arm {
            curve: curve.clone(),
            t0: a,
            t1: b,
        });
    }
}

/// Verkettet die Stücke über gemeinsame Enden zu Schleifen (Stückindex, vorwärts?).
fn chain(arms: &[Arm], gap: f64) -> Vec<Vec<(usize, bool)>> {
    let ends: Vec<(DVec3, DVec3)> = arms.iter().map(|a| (a.at(0.0), a.at(1.0))).collect();
    let mut used = vec![false; arms.len()];
    let mut out = Vec::new();
    for start in 0..arms.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut c = vec![(start, true)];
        // Erst vorwärts vom Ende, dann rückwärts vom Anfang verlängern.
        for back in [false, true] {
            loop {
                let tip = if back {
                    let (i, f) = c[0];
                    if f { ends[i].0 } else { ends[i].1 }
                } else {
                    let (i, f) = c[c.len() - 1];
                    if f { ends[i].1 } else { ends[i].0 }
                };
                let next = (0..arms.len()).filter(|&j| !used[j]).find_map(|j| {
                    if ends[j].0.distance(tip) <= gap {
                        Some((j, !back))
                    } else if ends[j].1.distance(tip) <= gap {
                        Some((j, back))
                    } else {
                        None
                    }
                });
                let Some((j, fwd)) = next else { break };
                used[j] = true;
                if back {
                    c.insert(0, (j, fwd));
                } else {
                    c.push((j, fwd));
                }
            }
        }
        out.push(c);
    }
    out
}

/// Liegt `p` im Vieleck `poly` (gerade/ungerade-Regel)?
fn contains(poly: &[DVec2], p: DVec2) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Rundungsreste aus der Ebene nehmen (alle z = 0).
fn flatten(g: &mut Included) {
    match g {
        Included::Point(p) => p.z = 0.0,
        Included::Line(a, b) => {
            a.z = 0.0;
            b.z = 0.0;
        }
        Included::Circle { c, n, .. } | Included::Ellipse { c, n, .. } => {
            c.z = 0.0;
            *n = DVec3::Z;
        }
        Included::Arc { c, start, end, n } => {
            for p in [c, start, end] {
                p.z = 0.0;
            }
            *n = DVec3::Z;
        }
        Included::EllipticArc {
            c, start, end, n, ..
        } => {
            for p in [c, start, end] {
                p.z = 0.0;
            }
            *n = DVec3::Z;
        }
        Included::Spline { .. } => {}
    }
}

/// Fasst Linien auf einer Geraden und Bögen auf demselben Kreis, die aneinanderstoßen, zusammen
/// (der Schnitt teilt sie an Flächengrenzen und Nähten).
fn merge(mut g: Vec<Included>, gap: f64) -> Vec<Included> {
    let near = |a: DVec3, b: DVec3| a.distance(b) <= gap;
    loop {
        let mut joined = None;
        'outer: for i in 0..g.len() {
            for j in 0..g.len() {
                if i == j {
                    continue;
                }
                let m = match (&g[i], &g[j]) {
                    (Included::Line(a0, a1), Included::Line(b0, b1)) => {
                        // Gemeinsames Ende und gleiche Richtung.
                        let (p, q, r) = if near(*a1, *b0) {
                            (*a0, *a1, *b1)
                        } else if near(*a1, *b1) {
                            (*a0, *a1, *b0)
                        } else {
                            continue;
                        };
                        let (u, v) = ((q - p).normalize_or_zero(), (r - q).normalize_or_zero());
                        if u.cross(v).length() > 1e-9 || u.dot(v) <= 0.0 {
                            continue;
                        }
                        Included::Line(p, r)
                    }
                    (
                        Included::Arc {
                            c: c0,
                            start: s0,
                            end: e0,
                            n: n0,
                        },
                        Included::Arc {
                            c: c1,
                            start: s1,
                            end: e1,
                            n: n1,
                        },
                    ) => {
                        if !near(*c0, *c1)
                            || (c0.distance(*s0) - c1.distance(*s1)).abs() > gap
                            || n0.dot(*n1) < 1.0 - 1e-9
                            || !near(*e0, *s1)
                            || near(*s0, *e1)
                        {
                            continue;
                        }
                        Included::Arc {
                            c: *c0,
                            start: *s0,
                            end: *e1,
                            n: *n0,
                        }
                    }
                    _ => continue,
                };
                joined = Some((i, j, m));
                break 'outer;
            }
        }
        let Some((i, j, m)) = joined else { return g };
        g[i] = m;
        g.remove(j);
    }
}

/// Bild eines Kurvenstücks aus „Bestimmte Elemente“: Gerade in einer Ebene mit der Achse →
/// Linie (an der Achse geteilt), koaxialer Kreis → Punkt, sonst Spline innerhalb `limit`.
fn element_curve(
    spin: &Spin,
    c: &Curve,
    t0: f64,
    t1: f64,
    limit: f64,
    tol: &Tolerance,
    out: &mut Vec<Included>,
) {
    let lin = tol.linear.max(1e-9);
    match c {
        Curve::Line { origin, dir } => {
            let w = |p: DVec3| {
                let v = p - spin.a;
                v - spin.d * v.dot(spin.d)
            };
            let (w0, w1) = (w(*origin), w(*origin + *dir) - w(*origin));
            // Eine Ebene mit der Achse: Die Querteile sind parallel (oder eins verschwindet).
            if w0.cross(w1).length() <= lin * (1.0 + w1.length()) {
                let mut ts = vec![t0];
                let ww = w1.length_squared();
                if ww > 0.0 {
                    let t = -w0.dot(w1) / ww;
                    let inner = (t - t0) * (t - t1) < 0.0;
                    if inner && (w0 + w1 * t).length() <= lin {
                        ts.push(t);
                    }
                }
                ts.push(t1);
                for p in ts.windows(2) {
                    let (a, b) = (spin.image(c.point(p[0])), spin.image(c.point(p[1])));
                    if a.distance(b) > lin {
                        out.push(Included::Line(a, b));
                    }
                }
                return;
            }
        }
        Curve::Circle { frame, .. } => {
            let v = frame.origin - spin.a;
            let off = (v - spin.d * v.dot(spin.d)).length();
            let angle = (lin / frame.origin.distance(spin.a).max(1.0)).max(COAXIAL_ANGLE);
            if frame.z.cross(spin.d).length() <= angle && off <= lin {
                out.push(Included::Point(spin.image(c.point(t0))));
                return;
            }
        }
        _ => {}
    }
    // Wo die Kurve die Achse berührt oder kreuzt, hat das Bild einen Knick (r = 0): dort teilen,
    // jedes Teil für sich annähern (Korrektur M1).
    let r = |t: f64| spin.sr(c.point(t)).1;
    let ts: Vec<f64> = (0..=CLIP_SAMPLES)
        .map(|k| t0 + (t1 - t0) * k as f64 / CLIP_SAMPLES as f64)
        .collect();
    let mut cuts = vec![t0];
    for k in 1..CLIP_SAMPLES {
        let (r0, r1, r2) = (r(ts[k - 1]), r(ts[k]), r(ts[k + 1]));
        if r1 > r0 || r1 > r2 {
            continue;
        }
        // Minimum per Goldenem Schnitt eingrenzen.
        let (mut lo, mut hi) = (ts[k - 1], ts[k + 1]);
        let g = 0.5 * (5f64.sqrt() - 1.0);
        for _ in 0..80 {
            let (x1, x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
            if r(x1) < r(x2) {
                hi = x2;
            } else {
                lo = x1;
            }
        }
        let t = 0.5 * (lo + hi);
        if r(t) <= 10.0 * lin && (t - t0).abs() > 1e-9 && (t1 - t).abs() > 1e-9 {
            cuts.push(t);
        }
    }
    cuts.push(t1);
    cuts.dedup_by(|b, a| (*b - *a).abs() <= 1e-9);
    for w in cuts.windows(2) {
        out.push(spline_image(spin, c, w[0], w[1], limit, lin));
    }
}

/// Abgetastet, abgebildet und mit der Schranke `limit` angenähert (fester Spline). Prüft die
/// Mitten zwischen den Abtastpunkten und verdichtet, bis die Schranke auch dort gilt. Gelingt
/// das nicht, ist der Streckenzug durch die dichtesten Abtastpunkte der Spline (nie weglassen).
fn spline_image(spin: &Spin, c: &Curve, t0: f64, t1: f64, limit: f64, lin: f64) -> Included {
    let limit = if limit.is_finite() && limit > 0.0 {
        limit
    } else {
        lin
    };
    let mut n = SPLINE_SAMPLES.0;
    loop {
        let at = |k: f64| spin.image(c.point(t0 + (t1 - t0) * k / (n - 1) as f64));
        let pts: Vec<DVec3> = (0..n).map(|k| at(k as f64)).collect();
        let first = pts[0];
        if pts.iter().all(|p| p.distance(first) <= lin) {
            // Bild fällt auf einen Punkt zusammen.
            return Included::Point(first);
        }
        if let Some(curve) = fit(&pts, limit * 0.5)
            && (0..n - 1).all(|k| curve.distance_to(at(k as f64 + 0.5)) <= limit)
        {
            return Included::Spline { curve };
        }
        if n >= SPLINE_SAMPLES.1 {
            return polyline(pts);
        }
        n = 2 * n - 1;
    }
}

/// Streckenzug (Grad 1) durch die Punkte als Spline; zusammenfallende Nachbarn fallen weg.
fn polyline(mut pts: Vec<DVec3>) -> Included {
    pts.dedup_by(|b, a| a.distance(*b) <= 1e-12);
    let first = pts[0];
    if pts.len() < 2 {
        return Included::Point(first);
    }
    let n = pts.len();
    let mut knots = vec![0.0];
    knots.extend((0..n).map(|i| i as f64 / (n - 1) as f64));
    knots.push(1.0);
    match NurbsCurve::new(1, knots, pts, vec![1.0; n]) {
        Ok(curve) => Included::Spline { curve },
        Err(_) => Included::Point(first),
    }
}
