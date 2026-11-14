//! „Auf Fläche projizieren“ (Skizzen 10, Spec 6): Kurven fertiger Skizzen, Körperkanten, Ecken
//! und Konstruktionspunkte werden auf Körperflächen oder Konstruktionsebenen geworfen, nach dem
//! nächsten Punkt oder entlang eines Vektors.
//!
//! Rechnung (Spec 6, Schritte 1–6):
//! 1. Jede Quellkurve wird adaptiv abgetastet (Sehnenfehler ≤ Toleranz, an der Quelle und an
//!    den Treffern), höchstens [`MAX_SURFACE_SAMPLES`] Punkte je Kurve.
//! 2. „Nächster Punkt“: Fußpunkt über `Surface::project`, gültig nur im getrimmten Bereich
//!    (`point_in_face`); es zählt der nächste gültige Treffer über alle Ziele.
//! 3. „Entlang Vektor“: Gerade durch den Abtastpunkt in beide Richtungen, Schnitt über
//!    `curve_surface_roots`; es zählt der nächste gültige Treffer.
//! 4. Wo die Treffer abreißen oder um mehr als 10 × Abtastabstand springen, wird die Kurve
//!    geteilt; die Stelle wird durch Halbieren eingegrenzt.
//! 5. Jedes Stück wird als Spline (Grad 3) durch die Treffer angenähert; die Abweichung wird an
//!    den Mittelpunkten geprüft und bis zu viermal verfeinert.
//! 6. Sonderfälle exakt: Ist das einzige Ziel eine Ebene, wird eine Linie zur Linie, und ein zur
//!    Ebene paralleler Kreis oder Bogen wird senkrecht projiziert zum Kreis bzw. Bogen.
//!
//! Die Rechnung ist rein und hält eine Zeitgrenze ein; wird sie überschritten, gibt es
//! [`ProjectError::TooSlow`] und kein Ergebnis.

use crate::model::{Document, FeatureId, FeatureKind, PlaneId, PointRefId, VertexRef};
use crate::refs::{EdgeRef, FaceRef, resolve_edge, resolve_face};
use planora_i18n::tr;
use planora_kernel::boolean::{
    Containment, CurveSurface, FaceGeom, curve_surface_roots, face_geom, face_polys,
    point_in_face_at,
};
use planora_kernel::{Curve, NurbsSurfaceData, Solid, Surface};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat3, DVec2, DVec3, Frame, Tolerance};
use planora_sketch::helpers::{Included, include_curve};
use planora_sketch::model::CurveId;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

/// Höchstzahl Abtastpunkte je Quellkurve (Spec 6).
pub const MAX_SURFACE_SAMPLES: usize = 2000;
/// Rechenzeit höchstens 2 s (Spec 6, auch beim Neuberechnen, Spec 10).
pub const SURFACE_PROJECT_BUDGET: Duration = Duration::from_secs(2);

/// Gleichmäßige Abschnitte vor dem adaptiven Abtasten.
const START_INTERVALS: usize = 16;
/// Ein Treffer springt, wenn er weiter als dieses Vielfache des Abtastabstands vom vorigen
/// liegt (Spec 6, Schritt 4).
const JUMP_FACTOR: f64 = 10.0;
/// Höchstzahl Verfeinerungen der Annäherung (Spec 6, Schritt 5).
const MAX_REFINE: usize = 4;
/// Höchstzahl Halbierungen beim Eingrenzen einer Abriss- oder Sprungstelle.
const EDGE_BISECTIONS: usize = 60;
/// Höchstzahl Newton-Schritte für einen Strahltreffer auf einer Freiformfläche.
const NEWTON_STEPS: usize = 30;
/// Richtungen gelten als parallel bzw. eine Gerade als parallel zur Ebene unter diesem Wert.
const PARALLEL_EPS: f64 = 1e-9;

/// Projektionsart: nächster Punkt oder entlang eines Vektors (Weltrichtung, beide Richtungen).
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectType {
    ClosestPoint,
    AlongVector(DVec3),
}

/// Quelle der Projektion: Kurve einer früheren Skizze, Körperkante, Körperecke oder
/// Konstruktionspunkt (Platzhalter, siehe [`PointRefId`]).
#[derive(Clone, Debug, PartialEq)]
pub enum CurveSource {
    SketchCurve { sketch: FeatureId, curve: CurveId },
    Edge(EdgeRef),
    Vertex(VertexRef),
    ConstructionPoint(PointRefId),
}

/// Ziel der Projektion: eine Körperfläche (getrimmt) oder eine Grund- bzw. Konstruktionsebene
/// (unbegrenzt).
#[derive(Clone, Debug, PartialEq)]
pub enum FaceTarget {
    Face(FaceRef),
    Plane(PlaneId),
}

/// „Auf Fläche projizieren“: Quellen, Zielflächen und Projektionsart (Spec 6).
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceProjection {
    pub sources: Vec<CurveSource>,
    pub faces: Vec<FaceTarget>,
    pub kind: ProjectType,
}

impl SurfaceProjection {
    /// Feste Verweise (Kanten, Flächen, Konstruktionsebenen), siehe
    /// [`crate::CopySource::origins`].
    pub(crate) fn origins(&self, out: &mut Vec<FeatureId>) {
        for s in &self.sources {
            match s {
                CurveSource::Edge(e) | CurveSource::Vertex(VertexRef { edge: e, .. }) => {
                    out.push(FeatureId(e.a.origin));
                    out.push(FeatureId(e.b.origin));
                }
                CurveSource::SketchCurve { .. } | CurveSource::ConstructionPoint(_) => {}
            }
        }
        for f in &self.faces {
            match f {
                FaceTarget::Face(f) => out.push(FeatureId(f.origin)),
                FaceTarget::Plane(PlaneId::Feature(id)) => out.push(*id),
                FaceTarget::Plane(PlaneId::Standard(_)) => {}
            }
        }
    }

    /// Lose Verweise: Quellskizzen und Konstruktionspunkte (siehe
    /// [`crate::CopySource::loose_origins`]).
    pub(crate) fn loose_origins(&self) -> Vec<FeatureId> {
        self.sources
            .iter()
            .filter_map(|s| match s {
                CurveSource::SketchCurve { sketch, .. } => Some(*sketch),
                CurveSource::ConstructionPoint(id) => Some(*id),
                CurveSource::Edge(_) | CurveSource::Vertex(_) => None,
            })
            .collect()
    }
}

/// Warum die Projektion kein Ergebnis hat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectError {
    /// Keine Quelle trifft eine Fläche.
    NoHit,
    /// Die Zeitgrenze wurde überschritten; es gibt kein Ergebnis.
    TooSlow,
    /// Eine Quelle oder ein Ziel lässt sich nicht auflösen.
    Lost,
}

impl ProjectError {
    /// Meldung für die Oberfläche.
    pub fn message(&self) -> String {
        match self {
            ProjectError::NoHit => tr!("feature-surface-no-hit"),
            ProjectError::TooSlow => tr!("feature-surface-too-slow"),
            ProjectError::Lost => tr!("feature-surface-lost"),
        }
    }
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for ProjectError {}

/// Was die Rechnung liest: das Dokument, die Stelle der Skizze im Verlauf (sichtbare Körper,
/// Ebenen und Skizzen davor) und die Ebene der Skizze, in deren Koordinaten das Ergebnis steht.
#[derive(Clone, Copy, Debug)]
pub struct DocView<'a> {
    pub doc: &'a Document,
    pub at: usize,
    pub frame: Frame,
}

impl<'a> DocView<'a> {
    pub fn new(doc: &'a Document, at: usize, frame: Frame) -> Self {
        Self {
            doc,
            at: at.min(doc.features.len()),
            frame,
        }
    }
}

/// Projiziert die Quellen von `spec` auf seine Flächen. Ergebnis in Skizzenkoordinaten von
/// `view.frame`: je Quelle die Stücke in Laufrichtung (Linien, Bögen, Kreise in den Sonderfällen,
/// sonst Splines; Ecken und Punkte als Punkte), Quelle für Quelle. Fehler: eine Quelle oder ein
/// Ziel ist verloren ([`ProjectError::Lost`]), nichts trifft ([`ProjectError::NoHit`]) oder die
/// Zeit `budget` reicht nicht ([`ProjectError::TooSlow`]).
pub fn project_to_surface(
    view: &DocView<'_>,
    spec: &SurfaceProjection,
    tol: &Tolerance,
    budget: Duration,
) -> Result<Vec<Included>, ProjectError> {
    let start = Instant::now();
    let doc = view.doc;
    let visible = doc.visible_at(view.at);
    let solids: Vec<&Solid> = visible.iter().filter_map(|b| doc.body(*b)).collect();
    let dir = match &spec.kind {
        ProjectType::ClosestPoint => None,
        ProjectType::AlongVector(d) => {
            let d = d.normalize_or_zero();
            if !d.is_finite() || d == DVec3::ZERO {
                return Err(ProjectError::NoHit);
            }
            Some(d)
        }
    };
    let targets = spec
        .faces
        .iter()
        .map(|f| resolve_target(view, &solids, f, tol, dir.is_some()))
        .collect::<Option<Vec<_>>>()
        .ok_or(ProjectError::Lost)?;
    let sources = spec
        .sources
        .iter()
        .map(|s| resolve_source(view, &solids, s, tol))
        .collect::<Option<Vec<_>>>()
        .ok_or(ProjectError::Lost)?;
    if targets.is_empty() || sources.is_empty() {
        return Err(ProjectError::NoHit);
    }
    let pr = Projector {
        targets,
        dir,
        tol: *tol,
        start,
        budget,
        frame: view.frame,
        cache: RefCell::new(HashMap::new()),
        used: Cell::new(0),
    };
    pr.check()?;
    let mut out = Vec::new();
    for s in &sources {
        match s {
            Source::Point(q) => {
                if let Some(h) = pr.hit(*q, None)? {
                    out.push(Included::Point(view.frame.to_local(h.p)));
                }
            }
            Source::Curve(c, t0, t1) => {
                if let Some(g) = pr.exact(c, *t0, *t1) {
                    out.extend(g);
                } else {
                    out.extend(pr.project_curve(c, *t0, *t1)?);
                }
            }
        }
    }
    // Auch die letzte Annäherung zählt zur Zeitgrenze: Wer darüber liegt, bekommt nichts.
    pr.check()?;
    if out.is_empty() {
        Err(ProjectError::NoHit)
    } else {
        Ok(out)
    }
}

/// Aufgelöstes Ziel.
enum Target {
    /// Getrimmte Fläche mit ihren Randpolygonen (für den Inhaltstest) und, nur für „Entlang
    /// Vektor“, dem Quader des Körpers (begrenzt den Strahl).
    Face {
        geom: Box<FaceGeom>,
        polys: Vec<Vec<DVec2>>,
        bounds: Option<(DVec3, DVec3)>,
    },
    /// Unbegrenzte Ebene.
    Plane(Frame),
}

/// Aufgelöste Quelle: ein Punkt oder ein Kurvenstück im Raum.
enum Source {
    Point(DVec3),
    Curve(Curve, f64, f64),
}

fn resolve_target(
    view: &DocView<'_>,
    solids: &[&Solid],
    f: &FaceTarget,
    tol: &Tolerance,
    along: bool,
) -> Option<Target> {
    match f {
        FaceTarget::Face(r) => {
            let found = resolve_face(solids, r, tol).ok()?;
            let solid = solids[found.body];
            solid.faces.get(found.face)?;
            let geom = face_geom(solid, found.face);
            let polys = face_polys(&geom);
            // Nur für „Entlang Vektor“: Freiformflächen der Quader ihrer Kontrollpunkte (enthält
            // die Fläche), sonst der Quader des Körpernetzes.
            let bounds = match (&geom.surface, along) {
                (_, false) => None,
                (Surface::Nurbs(d), true) => Some(d.control_bounds()),
                (_, true) => {
                    Some(crate::plane_rect::solid_bounds(solid).or_else(|| vertex_bounds(solid))?)
                }
            };
            Some(Target::Face {
                geom: Box::new(geom),
                polys,
                bounds,
            })
        }
        FaceTarget::Plane(PlaneId::Standard(p)) => Some(Target::Plane(p.frame())),
        FaceTarget::Plane(PlaneId::Feature(id)) => {
            let i = view.doc.index_of(*id)?;
            if i >= view.at {
                return None;
            }
            view.doc.plane_frame(*id).map(Target::Plane)
        }
    }
}

/// Quader der Ecken (Ersatz, wenn sich kein Netz bilden lässt).
fn vertex_bounds(solid: &Solid) -> Option<(DVec3, DVec3)> {
    let mut it = solid.vertices.iter().map(|(_, v)| v.point);
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}

fn resolve_source(
    view: &DocView<'_>,
    solids: &[&Solid],
    s: &CurveSource,
    tol: &Tolerance,
) -> Option<Source> {
    match s {
        CurveSource::SketchCurve { sketch, curve } => {
            // Nur Skizzen vor der Skizze im Verlauf (Spec 10).
            let i = view.doc.index_of(*sketch)?;
            if i >= view.at || !matches!(view.doc.features[i].kind, FeatureKind::Sketch(_)) {
                return None;
            }
            let (c, t0, t1) = view.doc.sketch_of(*sketch)?.curve_world(*curve)?;
            Some(Source::Curve(c, t0, t1))
        }
        CurveSource::Edge(e) => {
            let found = resolve_edge(solids, e, tol).ok()?;
            let edge = solids[found.body].edges.get(found.edge)?;
            Some(Source::Curve(edge.curve.clone(), edge.t0, edge.t1))
        }
        CurveSource::Vertex(v) => {
            let found = resolve_edge(solids, &v.edge, tol).ok()?;
            let solid = solids[found.body];
            let e = solid.edges.get(found.edge)?;
            let p = solid
                .vertices
                .get(if v.end { e.end } else { e.start })?
                .point;
            Some(Source::Point(p))
        }
        // Konstruktionspunkte als Features gibt es noch nicht (Platzhalter).
        CurveSource::ConstructionPoint(_) => None,
    }
}

/// Ein Treffer: Punkt, Ziel (Index) und Flächenparameter (Startwert für Nachbarpunkte).
#[derive(Clone, Copy, Debug)]
struct Hit {
    p: DVec3,
    target: usize,
    uv: (f64, f64),
}

/// Ein Abtastpunkt: Parameter, Punkt der Quelle und (falls getroffen) Treffer.
#[derive(Clone, Copy, Debug)]
struct Sample {
    t: f64,
    q: DVec3,
    h: Option<Hit>,
}

impl Sample {
    fn hp(&self) -> Option<DVec3> {
        self.h.map(|h| h.p)
    }
}

/// Die eigentliche Rechnung mit Zielen, Richtung, Toleranz und Zeitgrenze.
struct Projector {
    targets: Vec<Target>,
    dir: Option<DVec3>,
    tol: Tolerance,
    start: Instant,
    budget: Duration,
    frame: Frame,
    /// Schon gerechnete Abtastpunkte der laufenden Quellkurve, nach den Bits des Parameters
    /// (Mittelpunkte aus dem Abtasten werden bei der Prüfung der Annäherung wieder gebraucht).
    cache: RefCell<HashMap<u64, Sample>>,
    /// Abtastpunkte der laufenden Quellkurve in den Stücken (höchstens [`MAX_SURFACE_SAMPLES`]).
    used: Cell<usize>,
}

/// Liegt der Punkt im Rechenbereich (endlich, Beträge höchstens [`crate::MAX_COORD_MM`])?
/// Treffer weit draußen (Richtung fast parallel zur Ebene) zählen wie kein Treffer.
fn in_range(p: DVec3) -> bool {
    p.is_finite() && p.abs().max_element() <= crate::MAX_COORD_MM
}

/// Ergebnis der Annäherung eines Stücks.
enum Fitted {
    /// Fertig (oder `None`: das Stück fällt zu einem Punkt zusammen).
    Done(Option<Included>),
    /// Die Toleranz ist mit diesem Stück nicht zu halten: dichter teilen.
    Split(Vec<Sample>),
}

/// Abstand von `p` zur Strecke `a`–`b`.
fn seg_distance(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let ab = b - a;
    let l2 = ab.length_squared();
    if l2 <= 0.0 {
        return p.distance(a);
    }
    let s = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    p.distance(a + ab * s)
}

/// Parameterbereich, in dem die Gerade `q + s·d` den (etwas vergrößerten) Quader schneidet.
fn ray_box(q: DVec3, d: DVec3, lo: DVec3, hi: DVec3, margin: f64) -> Option<(f64, f64)> {
    let lo = lo - DVec3::splat(margin);
    let hi = hi + DVec3::splat(margin);
    let (mut s0, mut s1) = (f64::NEG_INFINITY, f64::INFINITY);
    for k in 0..3 {
        if d[k].abs() < PARALLEL_EPS {
            if q[k] < lo[k] || q[k] > hi[k] {
                return None;
            }
            continue;
        }
        let a = (lo[k] - q[k]) / d[k];
        let b = (hi[k] - q[k]) / d[k];
        s0 = s0.max(a.min(b));
        s1 = s1.min(a.max(b));
    }
    (s0.is_finite() && s1.is_finite() && s0 < s1).then_some((s0, s1))
}

/// Newton auf `S(u, v) = q + s·d` von `uv` aus (Freiformfläche, Startwert eines Nachbarpunkts):
/// Parameter und Strahlparameter `s`, wenn das Verfahren auf ein Hundertstel der Toleranz
/// genau konvergiert; sonst `None` (dann rechnet der Aufrufer den vollen Schnitt).
fn ray_newton(
    d: &NurbsSurfaceData,
    q: DVec3,
    dir: DVec3,
    uv: (f64, f64),
    lin: f64,
) -> Option<((f64, f64), f64)> {
    let ((u0, u1), (v0, v1)) = d.surface().domain();
    let (mut u, mut v) = uv;
    let mut s = (d.point(u, v) - q).dot(dir);
    for _ in 0..NEWTON_STEPS {
        let (wu, wv) = d.wrap(u, v);
        let (cu, cv) = (wu.clamp(u0, u1), wv.clamp(v0, v1));
        let der = d.surface().derivs(cu, cv, 1);
        let f = der[0][0] - q - dir * s;
        if !f.is_finite() {
            return None;
        }
        if f.length() <= 0.01 * lin {
            return Some(((cu, cv), s));
        }
        let m = DMat3::from_cols(der[1][0], der[0][1], -dir);
        let det = m.determinant();
        if !det.is_finite() || det.abs() < 1e-300 {
            return None;
        }
        let x = m.inverse() * (-f);
        if !x.is_finite() {
            return None;
        }
        (u, v, s) = (cu + x.x, cv + x.y, s + x.z);
    }
    None
}

impl Projector {
    fn check(&self) -> Result<(), ProjectError> {
        if self.start.elapsed() > self.budget {
            Err(ProjectError::TooSlow)
        } else {
            Ok(())
        }
    }

    /// Der nächste gültige Treffer des Punkts `q` über alle Ziele (Spec 6, Schritte 2 und 3).
    /// `hint` ist der Treffer eines Nachbarpunkts (Startwert auf Freiformflächen).
    fn hit(&self, q: DVec3, hint: Option<Hit>) -> Result<Option<Hit>, ProjectError> {
        self.check()?;
        if !q.is_finite() {
            return Ok(None);
        }
        let mut best: Option<(f64, Hit)> = None;
        let mut take = |dist: f64, h: Hit| {
            if in_range(h.p) && best.is_none_or(|(b, _)| dist < b) {
                best = Some((dist, h));
            }
        };
        for (k, t) in self.targets.iter().enumerate() {
            let near = hint.filter(|h| h.target == k).map(|h| h.uv);
            match (t, self.dir) {
                (Target::Plane(f), None) => {
                    let p = q - f.z * (q - f.origin).dot(f.z);
                    take(
                        q.distance(p),
                        Hit {
                            p,
                            target: k,
                            uv: (0.0, 0.0),
                        },
                    );
                }
                (Target::Plane(f), Some(d)) => {
                    let dn = d.dot(f.z);
                    if dn.abs() < PARALLEL_EPS {
                        continue;
                    }
                    let s = (f.origin - q).dot(f.z) / dn;
                    take(
                        s.abs(),
                        Hit {
                            p: q + d * s,
                            target: k,
                            uv: (0.0, 0.0),
                        },
                    );
                }
                (Target::Face { geom, polys, .. }, None) => {
                    let uv = match near {
                        Some(n) => geom.surface.project_near(q, n),
                        None => geom.surface.project(q),
                    };
                    let p = geom.surface.point(uv.0, uv.1);
                    if p.is_finite()
                        && point_in_face_at(geom, polys, p, uv, &self.tol) != Containment::Outside
                    {
                        take(q.distance(p), Hit { p, target: k, uv });
                    }
                }
                (
                    Target::Face {
                        geom,
                        polys,
                        bounds,
                    },
                    Some(d),
                ) => {
                    let inside = |p: DVec3, uv: (f64, f64)| {
                        point_in_face_at(geom, polys, p, uv, &self.tol) != Containment::Outside
                    };
                    // Freiformfläche mit Startwert: zuerst Newton.
                    if let (Surface::Nurbs(nd), Some(n)) = (&geom.surface, near)
                        && let Some((uv, s)) = ray_newton(nd, q, d, n, self.tol.linear)
                    {
                        let p = q + d * s;
                        if inside(p, uv) {
                            take(s.abs(), Hit { p, target: k, uv });
                            continue;
                        }
                    }
                    let Some((lo, hi)) = *bounds else {
                        continue;
                    };
                    let margin = 1e-3 * lo.distance(hi) + 10.0 * self.tol.linear;
                    let Some((s0, s1)) = ray_box(q, d, lo, hi, margin) else {
                        continue;
                    };
                    let line = Curve::Line { origin: q, dir: d };
                    let CurveSurface::Roots(roots) =
                        curve_surface_roots(&line, s0, s1, &geom.surface, &self.tol)
                    else {
                        // Die Gerade liegt in der Fläche: kein eindeutiger Treffer.
                        continue;
                    };
                    for r in roots {
                        let p = q + d * r.t;
                        let uv = geom.surface.project(p);
                        if inside(p, uv) {
                            take(r.t.abs(), Hit { p, target: k, uv });
                        }
                    }
                }
            }
        }
        Ok(best.map(|(_, h)| h))
    }

    /// Hängen die Treffer `h0`, `h1` der Quellpunkte `q0`, `q1` zusammen (kein Sprung über
    /// 10 × Abtastabstand)?
    fn continuous(&self, h0: DVec3, h1: DVec3, q0: DVec3, q1: DVec3) -> bool {
        h0.distance(h1) <= JUMP_FACTOR * q0.distance(q1) + 10.0 * self.tol.linear
    }

    /// Hängen die Abtastpunkte `a` und `b` zusammen (beide getroffen, kein Sprung)?
    fn linked(&self, a: &Sample, b: &Sample) -> bool {
        match (a.hp(), b.hp()) {
            (Some(ha), Some(hb)) => self.continuous(ha, hb, a.q, b.q),
            _ => false,
        }
    }

    /// Abtastpunkt bei `t` (aus dem Zwischenspeicher, wenn schon gerechnet).
    fn sample(&self, c: &Curve, t: f64, hint: Option<Hit>) -> Result<Sample, ProjectError> {
        if let Some(s) = self.cache.borrow().get(&t.to_bits()) {
            return Ok(*s);
        }
        let q = c.point(t);
        let s = Sample {
            t,
            q,
            h: self.hit(q, hint)?,
        };
        self.cache.borrow_mut().insert(t.to_bits(), s);
        Ok(s)
    }

    /// Mittelpunkt zwischen `a` und `b`, mit dem Treffer eines Endes als Startwert.
    fn mid(&self, c: &Curve, a: &Sample, b: &Sample) -> Result<Sample, ProjectError> {
        self.sample(c, 0.5 * (a.t + b.t), a.h.or(b.h))
    }

    /// Sonderfälle exakt (Spec 6, Schritt 6): einziges Ziel eine Ebene; eine Linie wird zur Linie,
    /// ein zur Ebene paralleler Kreis oder Bogen bei senkrechter Projektion zum Kreis bzw. Bogen.
    /// `None`, wenn kein Sonderfall vorliegt.
    fn exact(&self, c: &Curve, t0: f64, t1: f64) -> Option<Vec<Included>> {
        let [Target::Plane(f)] = self.targets.as_slice() else {
            return None;
        };
        let d = self.dir.unwrap_or(f.z);
        let dn = d.dot(f.z);
        if dn.abs() < PARALLEL_EPS {
            // Parallel zur Ebene trifft nichts.
            return Some(Vec::new());
        }
        let proj = |p: DVec3| p + d * ((f.origin - p).dot(f.z) / dn);
        let local = |p: DVec3| self.frame.to_local(p);
        match c {
            Curve::Line { .. } => {
                let (a, b) = (proj(c.point(t0)), proj(c.point(t1)));
                if !(in_range(a) && in_range(b)) {
                    return Some(Vec::new());
                }
                if a.distance(b) <= self.tol.linear {
                    // Entlang der eigenen Richtung projiziert: ein Punkt.
                    return Some(vec![Included::Point(local(a))]);
                }
                Some(vec![Included::Line(local(a), local(b))])
            }
            Curve::Circle { frame, radius } => {
                let perpendicular = d.cross(f.z).length() < PARALLEL_EPS;
                let parallel = frame.z.cross(f.z).length() < PARALLEL_EPS;
                if !(perpendicular && parallel) {
                    return None;
                }
                if !in_range(proj(frame.origin)) {
                    return Some(Vec::new());
                }
                let moved = Curve::Circle {
                    frame: Frame {
                        origin: proj(frame.origin),
                        ..*frame
                    },
                    radius: *radius,
                };
                include_curve(&self.frame, &moved, t0, t1)
                    .ok()
                    .map(|g| vec![g])
            }
            _ => None,
        }
    }

    /// Allgemeiner Weg (Spec 6, Schritte 1, 4 und 5) für das Kurvenstück `c(t0 … t1)`.
    fn project_curve(&self, c: &Curve, t0: f64, t1: f64) -> Result<Vec<Included>, ProjectError> {
        self.cache.borrow_mut().clear();
        let samples = self.adaptive(c, t0, t1)?;
        self.used.set(samples.len());
        let closed = c
            .period()
            .is_some_and(|p| ((t1 - t0).abs() - p).abs() < 1e-9)
            || c.point(t0).distance(c.point(t1)) <= self.tol.linear;
        let pieces = self.pieces(c, &samples, closed)?;
        let mut out = Vec::new();
        for p in pieces {
            out.extend(self.fit_piece(c, p)?);
        }
        Ok(out)
    }

    /// Adaptives Abtasten: Abschnitte werden halbiert, solange die Sehne an der Quelle oder an
    /// den Treffern mehr als die Toleranz abweicht oder in der Mitte ein Treffer auftaucht oder
    /// fehlt. Abschnitte mit nur einem getroffenen Ende bleiben (die Stelle grenzt
    /// [`Projector::pieces`] ein). Höchstens [`MAX_SURFACE_SAMPLES`] Punkte.
    fn adaptive(&self, c: &Curve, t0: f64, t1: f64) -> Result<Vec<Sample>, ProjectError> {
        let mut s: Vec<Sample> = Vec::with_capacity(START_INTERVALS + 1);
        for i in 0..=START_INTERVALS {
            let hint = s.last().and_then(|p: &Sample| p.h);
            s.push(self.sample(c, t0 + (t1 - t0) * i as f64 / START_INTERVALS as f64, hint)?);
        }
        // `open[i]`: Abschnitt `s[i]`–`s[i + 1]` ist noch zu prüfen.
        let mut open = vec![true; s.len() - 1];
        let lin = self.tol.linear;
        while open.iter().any(|o| *o) && s.len() < MAX_SURFACE_SAMPLES {
            let mut ns = vec![s[0]];
            let mut no = Vec::new();
            for i in 0..s.len() - 1 {
                let (a, b) = (s[i], s[i + 1]);
                if !open[i] || ns.len() + (s.len() - i) >= MAX_SURFACE_SAMPLES {
                    ns.push(b);
                    no.push(false);
                    continue;
                }
                let m = self.mid(c, &a, &b)?;
                let split = seg_distance(m.q, a.q, b.q) > lin
                    || match (a.hp(), m.hp(), b.hp()) {
                        (Some(ha), Some(hm), Some(hb)) => {
                            self.continuous(ha, hm, a.q, m.q)
                                && self.continuous(hm, hb, m.q, b.q)
                                && seg_distance(hm, ha, hb) > lin
                        }
                        (Some(_), None, Some(_)) | (None, Some(_), None) => true,
                        _ => false,
                    };
                if split {
                    ns.push(m);
                    ns.push(b);
                    no.push(true);
                    no.push(true);
                } else {
                    ns.push(b);
                    no.push(false);
                }
            }
            s = ns;
            open = no;
        }
        Ok(s)
    }

    /// Grenzt die Stelle zwischen dem guten Abtastpunkt `good` und dem schlechten `bad` (kein
    /// Treffer oder Sprung) ein und liefert den letzten guten Punkt nahe an `bad`.
    fn edge(&self, c: &Curve, good: Sample, bad: Sample) -> Result<Sample, ProjectError> {
        let (mut g, mut b) = (good, bad);
        for _ in 0..EDGE_BISECTIONS {
            if g.q.distance(b.q) <= 0.5 * self.tol.linear || self.used.get() >= MAX_SURFACE_SAMPLES
            {
                break;
            }
            let m = self.sample(c, 0.5 * (g.t + b.t), g.h)?;
            if self.linked(&g, &m) {
                g = m;
            } else {
                b = m;
            }
        }
        Ok(g)
    }

    /// Teilt die Abtastpunkte an Abrissen und Sprüngen in Stücke (Spec 6, Schritt 4); die Enden
    /// werden eingegrenzt. Bei einer geschlossenen Quelle werden das erste und das letzte Stück
    /// über die Naht verbunden.
    fn pieces(
        &self,
        c: &Curve,
        s: &[Sample],
        closed: bool,
    ) -> Result<Vec<Vec<Sample>>, ProjectError> {
        let mut pieces: Vec<Vec<Sample>> = Vec::new();
        let mut cur: Vec<Sample> = Vec::new();
        for i in 0..s.len() {
            if s[i].h.is_none() {
                continue;
            }
            if cur.is_empty() && i > 0 {
                // Anfang eines Stücks: zur vorigen Stelle hin eingrenzen.
                let e = self.edge(c, s[i], s[i - 1])?;
                if e.t != s[i].t {
                    cur.push(e);
                    self.used.set(self.used.get() + 1);
                }
            }
            cur.push(s[i]);
            if i + 1 == s.len() || !self.linked(&s[i], &s[i + 1]) {
                if i + 1 < s.len() {
                    let e = self.edge(c, s[i], s[i + 1])?;
                    if e.t != s[i].t {
                        cur.push(e);
                        self.used.set(self.used.get() + 1);
                    }
                }
                pieces.push(std::mem::take(&mut cur));
            }
        }
        // Über die Naht: Das letzte Stück endet am Ende, das erste beginnt am Anfang.
        if closed && pieces.len() > 1 {
            let first_at_start = pieces[0][0].t == s[0].t;
            let last_at_end = pieces
                .last()
                .and_then(|p| p.last())
                .is_some_and(|l| l.t == s[s.len() - 1].t);
            if first_at_start && last_at_end {
                let mut joined = pieces.pop().unwrap_or_default();
                let head = std::mem::take(&mut pieces[0]);
                // Der erste Punkt des Kopfstücks ist derselbe wie der letzte des Schwanzes.
                joined.extend(head.into_iter().skip(1));
                pieces[0] = joined;
            }
        }
        Ok(pieces)
    }

    /// Nähert ein Stück als Spline (Grad 3) an (siehe [`Projector::fit_refined`]). Lässt sich die
    /// Toleranz nicht halten, wird das Stück in der Mitte geteilt und jede Hälfte für sich
    /// angenähert; die Schranke wird nie gelockert. Ist auch das nicht mehr möglich (Stück aus
    /// zwei Punkten, Deckel der Abtastpunkte erreicht), gibt es [`ProjectError::TooSlow`].
    fn fit_piece(&self, c: &Curve, p: Vec<Sample>) -> Result<Vec<Included>, ProjectError> {
        match self.fit_refined(c, p)? {
            Fitted::Done(g) => Ok(g.into_iter().collect()),
            Fitted::Split(p) => {
                if p.len() < 3 {
                    return Err(ProjectError::TooSlow);
                }
                let k = p.len() / 2;
                let mut out = self.fit_piece(c, p[..=k].to_vec())?;
                out.extend(self.fit_piece(c, p[k..].to_vec())?);
                Ok(out)
            }
        }
    }

    /// Annäherung mit der Schranke `tol.linear` und Prüfung an den Mittelpunkten; wo die
    /// Abweichung darüber liegt, wird verfeinert (höchstens [`MAX_REFINE`]-mal, solange der
    /// Deckel [`MAX_SURFACE_SAMPLES`] es zulässt). Bleibt eine Abweichung über der Toleranz oder
    /// scheitert die Annäherung, ist das Stück zu teilen.
    fn fit_refined(&self, c: &Curve, mut p: Vec<Sample>) -> Result<Fitted, ProjectError> {
        let lin = self.tol.linear;
        for round in 0..=MAX_REFINE {
            self.check()?;
            let pts: Vec<DVec3> = p
                .iter()
                .filter_map(|s| s.hp().map(|h| self.frame.to_local(h)))
                .collect();
            let length: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
            if pts.len() < 2 || length <= lin {
                return Ok(Fitted::Done(None));
            }
            let fitted = fit(&pts, lin);
            // Die Annäherung selbst kann teuer sein: Zeitgrenze gleich danach prüfen.
            self.check()?;
            let Some(curve) = fitted else {
                return Ok(Fitted::Split(p));
            };
            let mut np = vec![p[0]];
            let (mut bad, mut added) = (false, false);
            for w in p.windows(2) {
                let (a, b) = (w[0], w[1]);
                if self.linked(&a, &b) {
                    let m = self.mid(c, &a, &b)?;
                    if self.linked(&a, &m)
                        && self.linked(&m, &b)
                        && m.hp()
                            .is_some_and(|h| curve.distance_to(self.frame.to_local(h)) > lin)
                    {
                        bad = true;
                        if round < MAX_REFINE && self.used.get() < MAX_SURFACE_SAMPLES {
                            np.push(m);
                            self.used.set(self.used.get() + 1);
                            added = true;
                        }
                    }
                }
                np.push(b);
            }
            self.check()?;
            if !bad {
                return Ok(Fitted::Done(Some(Included::Spline { curve })));
            }
            if !added {
                return Ok(Fitted::Split(np));
            }
            p = np;
        }
        Ok(Fitted::Split(p))
    }
}

thread_local! {
    /// Künstliche Verzögerung jeder Annäherung (nur für Tests, siehe [`slow_fit_for_tests`]).
    static FIT_DELAY: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

/// Verlangsamt im aufrufenden Thread jede Annäherung um `delay`, bis der Wächter fällt (nur für
/// Tests: belegt, dass die Zeitgrenze auch die Annäherung erfasst).
#[doc(hidden)]
#[must_use]
pub fn slow_fit_for_tests(delay: Duration) -> SlowFitGuard {
    let previous = FIT_DELAY.with(|c| c.replace(delay));
    SlowFitGuard { previous }
}

/// Stellt beim Fallen die vorige Verzögerung wieder her.
#[doc(hidden)]
pub struct SlowFitGuard {
    previous: Duration,
}

impl Drop for SlowFitGuard {
    fn drop(&mut self) {
        FIT_DELAY.with(|c| c.set(self.previous));
    }
}

/// Annäherung mit der Schranke `lin` (nie gelockert).
fn fit(pts: &[DVec3], lin: f64) -> Option<NurbsCurve> {
    let delay = FIT_DELAY.with(Cell::get);
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
    NurbsCurve::fit(pts, lin).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fix-Runde 1: Eine Quelle mit Knick (Polygonzug Grad 1) wird nicht mit gelockerter
    /// Schranke angenähert, sondern dichter geteilt; jede Abweichung bleibt ≤ Toleranz.
    #[test]
    fn kink_is_split_not_loosened() {
        let tol = Tolerance {
            linear: 1e-6,
            ..Tolerance::default()
        };
        let pts = [
            DVec3::new(0.0, 0.0, 1.0),
            DVec3::new(5.0, 5.0, 1.0),
            DVec3::new(10.0, 0.0, 1.0),
        ];
        let v =
            NurbsCurve::new(1, vec![0.0, 0.0, 0.5, 1.0, 1.0], pts.to_vec(), vec![1.0; 3]).unwrap();
        let c = Curve::nurbs(v);
        let pr = Projector {
            targets: vec![Target::Plane(Frame::WORLD)],
            dir: None,
            tol,
            start: Instant::now(),
            budget: Duration::from_secs(60),
            frame: Frame::WORLD,
            cache: RefCell::new(HashMap::new()),
            used: Cell::new(0),
        };
        let got = pr.project_curve(&c, 0.0, 1.0).unwrap();
        assert!(!got.is_empty());
        let flat = |p: DVec3| DVec3::new(p.x, p.y, 0.0);
        let (a, m, b) = (flat(pts[0]), flat(pts[1]), flat(pts[2]));
        for g in &got {
            let Included::Spline { curve } = g else {
                panic!("{g:?}");
            };
            let (t0, t1) = curve.domain();
            for i in 0..=400 {
                let p = curve.point(t0 + (t1 - t0) * i as f64 / 400.0);
                let gap = seg_distance(p, a, m).min(seg_distance(p, m, b));
                assert!(gap <= tol.linear, "Abweichung {gap} bei {p:?}");
            }
        }
    }
}
