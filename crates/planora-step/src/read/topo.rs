//! Topologie aus STEP (STEP-Import, Aufgabe 3): Schalen, Flächen, Ränder, Kanten und Ecken als
//! exakter Körper des Kernels.
//!
//! Die Richtungsregeln von STEP (ISO 10303-42) und Kernel stimmen überein: Eine Fläche zeigt
//! nach außen mit der Normale ihrer Trägerfläche, wenn `same_sense` wahr ist; ihre äußere
//! Schleife läuft gegen den Uhrzeigersinn um diese Außennormale, Löcher im Uhrzeigersinn; eine
//! Kante läuft von ihrer Anfangs- zu ihrer Endecke. Jede der folgenden Angaben kehrt unabhängig
//! um und wird deshalb einzeln verrechnet:
//! - `ORIENTED_CLOSED_SHELL.orientation` (.F.: alle Flächen der Schale umgekehrt),
//! - `ADVANCED_FACE.same_sense` (und `flipped` der Trägerfläche),
//! - `FACE_BOUND.orientation` (.F.: Schleife rückwärts, jede Halbkante gekippt),
//! - `ORIENTED_EDGE.orientation` (Halbkante vorwärts oder rückwärts),
//! - `EDGE_CURVE.same_sense` (Kante läuft mit oder gegen den Kurvenparameter: `t1 < t0`).
//!
//! Der Aufbau geht über eine Rohform ([`RawBody`]); daraus entsteht der Körper, der dann
//! geprüft wird: `validate`, Umlaufsinn je Fläche, Außenrichtung über das Volumen (negativ →
//! ganz umgedreht), Lage jeder Kante auf ihren Flächen und jeder Ecke auf ihren Kanten.

use crate::read::error::MAX_STEP_FACES;
use crate::read::geom::{Resolver, Skip, SurfaceGeom};
use crate::read::index::StepFile;
use crate::read::units::Units;
use crate::read::value::Entity;
use crate::topology::CLASSIFY_CHORD;
use planora_exchange::Progress;
use planora_kernel::boolean::{face_geom, loop_points, oriented_area, points_to_uv};
use planora_kernel::{
    Curve, Face, FaceTag, HalfEdge, Id, Loop, NURBS_EDGE_TOL, Shell, Solid, Surface, UvRect,
    point_surface_tol, validate,
};
use planora_math::{DVec3, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use std::collections::HashMap;

/// Rolle der Flächenmarke: „STEP-Fläche {n}“, `n` ab 1 in der Reihenfolge der Datei (P7).
pub const STEP_FACE_ROLE: &str = "STEP-Fläche";

/// Zwischenpunkte je Kante, aus denen der Parameterbereich einer Fläche bestimmt wird (dazu
/// beide Ecken).
const EDGE_SAMPLES: usize = 16;

/// Höchstens so viele Stufen von `ORIENTED_EDGE` auf `ORIENTED_EDGE` bzw.
/// `ORIENTED_CLOSED_SHELL` auf `ORIENTED_CLOSED_SHELL` werden verfolgt.
const MAX_ORIENTED_CHAIN: usize = 8;

/// Abstand (Anteil der Periode), bis zu dem ein Parameterbereich noch als ein Umlauf gilt.
const PERIOD_SLACK: f64 = 1e-9;

/// Abstand (relativ), bis zu dem `v` auf einem Pol der Fläche liegt (dort ist `u` beliebig).
const POLE_REL: f64 = 1e-9;

/// Breite (relativ), unter der ein Parameterbereich keine Ausdehnung hat.
const EMPTY_REL: f64 = 1e-12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildOptions {
    /// Toleranz (mm), mit der Kanten auf ihren Flächen und Ecken auf ihren Kanten liegen müssen
    /// (mindestens die Toleranz des Kernels).
    pub weld_tol_mm: f64,
}

/// Ein gebauter Körper samt Zahl seiner Flächen und Warnungen (Aufgabe 7: grobe Datei).
#[derive(Clone, Debug)]
pub struct BuiltSolid {
    pub solid: Solid,
    pub faces: usize,
    pub warnings: Vec<String>,
}

/// Halbkante der Rohform: Index in [`RawBody::edges`] und Laufrichtung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RawHalfEdge {
    pub edge: usize,
    pub forward: bool,
}

/// Kante der Rohform: Kurve über `t0 → t1` von Ecke `start` nach `end` (Indizes in
/// [`RawBody::vertices`]).
#[derive(Clone, Debug)]
pub(crate) struct RawEdge {
    pub curve: Curve,
    pub t0: f64,
    pub t1: f64,
    pub start: usize,
    pub end: usize,
}

/// Fläche der Rohform; `same_sense` schon mit `flipped` und der Schalenrichtung verrechnet.
#[derive(Clone, Debug)]
pub(crate) struct RawFace {
    pub surface: Surface,
    pub same_sense: bool,
    pub outer: Vec<RawHalfEdge>,
    pub inner: Vec<Vec<RawHalfEdge>>,
    pub patch: Option<UvRect>,
    pub tag: FaceTag,
}

/// Schale der Rohform: Indizes in [`RawBody::faces`].
#[derive(Clone, Debug, Default)]
pub(crate) struct RawShell {
    pub faces: Vec<usize>,
}

/// Rohform eines Körpers: Ecken, Kanten, Flächen, Schalen (Aufgaben 6 und 7 arbeiten darauf).
#[derive(Clone, Debug, Default)]
pub(crate) struct RawBody {
    pub vertices: Vec<DVec3>,
    pub edges: Vec<RawEdge>,
    pub faces: Vec<RawFace>,
    pub shells: Vec<RawShell>,
}

impl RawBody {
    /// Punkte einer Schleife in Laufrichtung: je Halbkante beide Ecken und
    /// [`EDGE_SAMPLES`] Zwischenpunkte.
    fn loop_points(&self, lp: &[RawHalfEdge]) -> Vec<DVec3> {
        let n = EDGE_SAMPLES + 1;
        let mut pts = Vec::with_capacity(lp.len() * (n + 1));
        for he in lp {
            let e = &self.edges[he.edge];
            for k in 0..=n {
                let s = k as f64 / n as f64;
                let s = if he.forward { s } else { 1.0 - s };
                pts.push(e.curve.point(e.t0 + (e.t1 - e.t0) * s));
            }
        }
        pts
    }

    /// Der Körper des Kernels (Ids = Indizes der Rohform).
    fn to_solid(&self) -> Solid {
        let mut solid = Solid::new();
        for p in &self.vertices {
            solid.add_vertex(*p);
        }
        for e in &self.edges {
            solid.add_edge(
                e.curve.clone(),
                e.t0,
                e.t1,
                Id::from_index(e.start),
                Id::from_index(e.end),
            );
        }
        let kernel_loop = |lp: &[RawHalfEdge]| Loop {
            half_edges: lp
                .iter()
                .map(|he| HalfEdge {
                    edge: Id::from_index(he.edge),
                    forward: he.forward,
                })
                .collect(),
        };
        for f in &self.faces {
            solid.add_face(Face {
                surface: f.surface.clone(),
                same_sense: f.same_sense,
                outer: kernel_loop(&f.outer),
                inner: f.inner.iter().map(|l| kernel_loop(l)).collect(),
                patch: f.patch,
                tag: Some(f.tag.clone()),
            });
        }
        solid.shells = self
            .shells
            .iter()
            .map(|s| Shell {
                faces: s.faces.iter().map(|&f| Id::from_index(f)).collect(),
            })
            .collect();
        solid
    }
}

/// Zahl mit Leerzeichen als Tausendertrennung („20 000“).
fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Koordinate für Meldungen: zwei Nachkommastellen, Komma, nie „-0,00“.
fn coord(x: f64) -> String {
    let r = (x * 100.0).round() / 100.0;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r:.2}").replace('.', ",")
}

fn place(p: DVec3) -> String {
    format!("({}; {}; {})", coord(p.x), coord(p.y), coord(p.z))
}

/// Liest die Topologie eines Körpers.
struct TopoReader<'f> {
    file: &'f StepFile,
    res: Resolver<'f>,
    vertex_of: HashMap<u64, usize>,
    edge_of: HashMap<u64, usize>,
    raw: RawBody,
}

impl<'f> TopoReader<'f> {
    fn fail(&self, id: u64, msg: impl Into<String>) -> Skip {
        self.file.fail(id, msg).into()
    }

    /// Eintrag `#id`, der einer der Typen `names` sein muss; liefert auch den gefundenen Typ.
    fn expect(&self, id: u64, names: &[&'static str]) -> Result<(Entity, &'static str), Skip> {
        let e = self.file.entity(id)?;
        match names.iter().find(|n| e.is(n)) {
            Some(n) => Ok((e, n)),
            None => Err(self.fail(
                id,
                format!(
                    "Erwartet wird {}, gefunden: {}.",
                    names.join(" oder "),
                    e.names().join(" ")
                ),
            )),
        }
    }

    fn reference(&self, id: u64, e: &Entity, name: &str, i: usize) -> Result<u64, Skip> {
        e.arg(name, i)
            .and_then(|v| v.reference())
            .map_err(|m| self.fail(id, m))
    }

    fn refs(&self, id: u64, e: &Entity, name: &str, i: usize) -> Result<Vec<u64>, Skip> {
        e.arg(name, i)
            .and_then(|v| v.refs())
            .map_err(|m| self.fail(id, m))
    }

    /// Wahrheitswert; `.U.` ist bei Richtungen nicht erlaubt.
    fn flag(&self, id: u64, e: &Entity, name: &str, i: usize) -> Result<bool, Skip> {
        e.arg(name, i)
            .and_then(|v| v.logical())
            .map_err(|m| self.fail(id, m))?
            .ok_or_else(|| self.fail(id, "Eine Richtung ist unbestimmt („.U.“)."))
    }

    /// Flächen einer Schale und ob sie umgekehrt ist (`ORIENTED_CLOSED_SHELL` mit .F.).
    fn shell(&self, id: u64) -> Result<(Vec<u64>, bool), Skip> {
        let mut id = id;
        let mut flipped = false;
        for _ in 0..=MAX_ORIENTED_CHAIN {
            let (e, kind) = self.expect(id, &["CLOSED_SHELL", "ORIENTED_CLOSED_SHELL"])?;
            if kind == "CLOSED_SHELL" {
                return Ok((self.refs(id, &e, kind, 1)?, flipped));
            }
            flipped ^= !self.flag(id, &e, kind, 3)?;
            id = self.reference(id, &e, kind, 2)?;
        }
        Err(self.fail(id, "Die Schalen verweisen zu tief aufeinander."))
    }

    fn vertex(&mut self, id: u64) -> Result<usize, Skip> {
        if let Some(&v) = self.vertex_of.get(&id) {
            return Ok(v);
        }
        let (e, kind) = self.expect(id, &["VERTEX_POINT"])?;
        let p = self.reference(id, &e, kind, 1)?;
        let p = self.res.point(p)?;
        let v = self.raw.vertices.len();
        self.raw.vertices.push(p);
        self.vertex_of.insert(id, v);
        Ok(v)
    }

    fn edge(&mut self, id: u64) -> Result<usize, Skip> {
        if let Some(&k) = self.edge_of.get(&id) {
            return Ok(k);
        }
        let (e, kind) = self.expect(id, &["EDGE_CURVE"])?;
        let start = self.reference(id, &e, kind, 1)?;
        let end = self.reference(id, &e, kind, 2)?;
        let curve = self.reference(id, &e, kind, 3)?;
        let same_sense = self.flag(id, &e, kind, 4)?;
        let start = self.vertex(start)?;
        let end = self.vertex(end)?;
        let curve = self.res.curve(curve)?;
        let (p, q) = (self.raw.vertices[start], self.raw.vertices[end]);
        let (t0, t1) = edge_params(&curve, p, q, same_sense).map_err(|Skip(m)| self.fail(id, m))?;
        let k = self.raw.edges.len();
        self.raw.edges.push(RawEdge {
            curve,
            t0,
            t1,
            start,
            end,
        });
        self.edge_of.insert(id, k);
        Ok(k)
    }

    /// `ORIENTED_EDGE` (auch auf ein weiteres `ORIENTED_EDGE`) als Halbkante.
    fn half_edge(&mut self, id: u64) -> Result<RawHalfEdge, Skip> {
        let mut id = id;
        let mut forward = true;
        for _ in 0..=MAX_ORIENTED_CHAIN {
            let (e, kind) = self.expect(id, &["ORIENTED_EDGE", "EDGE_CURVE"])?;
            if kind == "EDGE_CURVE" {
                return Ok(RawHalfEdge {
                    edge: self.edge(id)?,
                    forward,
                });
            }
            forward ^= !self.flag(id, &e, kind, 4)?;
            id = self.reference(id, &e, kind, 3)?;
        }
        Err(self.fail(id, "Die Kanten verweisen zu tief aufeinander."))
    }

    /// Ein Rand (`FACE_BOUND`/`FACE_OUTER_BOUND`) als Schleife des Kernels; `reverse` kehrt ihn
    /// zusätzlich um (umgekehrte Schale). Liefert auch, ob er als äußerer Rand markiert ist.
    fn bound(&mut self, id: u64, reverse: bool) -> Result<(Vec<RawHalfEdge>, bool), Skip> {
        let (e, kind) = self.expect(id, &["FACE_OUTER_BOUND", "FACE_BOUND"])?;
        let lp = self.reference(id, &e, kind, 1)?;
        let along = self.flag(id, &e, kind, 2)?;
        let le = self.file.entity(lp)?;
        if !le.is("EDGE_LOOP") {
            return Err(self.fail(
                lp,
                format!(
                    "Ein Rand vom Typ {} wird noch nicht unterstützt.",
                    le.names().join(" ")
                ),
            ));
        }
        let list = self.refs(lp, &le, "EDGE_LOOP", 1)?;
        if list.is_empty() {
            return Err(self.fail(lp, "Ein Rand hat keine Kanten."));
        }
        let mut hes = Vec::with_capacity(list.len());
        for oe in list {
            hes.push(self.half_edge(oe)?);
        }
        if along == reverse {
            hes.reverse();
            for he in &mut hes {
                he.forward = !he.forward;
            }
        }
        Ok((hes, kind == "FACE_OUTER_BOUND"))
    }

    /// Eine Fläche mit der Marke `n` (ab 1); `reverse`: Schale umgekehrt.
    fn face(&mut self, id: u64, n: usize, reverse: bool) -> Result<RawFace, Skip> {
        let (e, kind) = self.expect(id, &["ADVANCED_FACE", "FACE_SURFACE"])?;
        let bounds = self.refs(id, &e, kind, 1)?;
        let geometry = self.reference(id, &e, kind, 2)?;
        let same_sense = self.flag(id, &e, kind, 3)?;
        let (surface, flipped) = match self.res.surface(geometry)? {
            SurfaceGeom::Ready { surface, flipped } => (surface, flipped),
            SurfaceGeom::Extrusion { .. } => {
                return Err(Skip(
                    "Eine Strangfläche aus einer Freiformkurve wird noch nicht unterstützt.".into(),
                ));
            }
        };
        if bounds.is_empty() {
            return Err(self.fail(id, "Eine Fläche hat keinen Rand."));
        }
        let mut loops = Vec::with_capacity(bounds.len());
        let mut outer_marked: Option<usize> = None;
        for (i, b) in bounds.iter().enumerate() {
            let (lp, outer) = self.bound(*b, reverse)?;
            if outer {
                if outer_marked.is_some() {
                    return Err(self.fail(id, "Eine Fläche hat mehr als einen äußeren Rand."));
                }
                outer_marked = Some(i);
            }
            loops.push(lp);
        }
        let points: Vec<Vec<DVec3>> = loops.iter().map(|l| self.raw.loop_points(l)).collect();
        let outer = match (outer_marked, &surface) {
            (Some(i), _) => i,
            (None, Surface::Plane { frame }) => {
                // Ohne Kennzeichnung: der Rand mit der größten Fläche in der Ebene.
                let area = |pts: &[DVec3]| {
                    let local: Vec<DVec3> = pts.iter().map(|p| frame.to_local(*p)).collect();
                    let n = local.len();
                    (0..n)
                        .map(|k| {
                            let (a, b) = (local[k], local[(k + 1) % n]);
                            a.x * b.y - b.x * a.y
                        })
                        .sum::<f64>()
                        .abs()
                };
                (0..loops.len())
                    .max_by(|&a, &b| area(&points[a]).total_cmp(&area(&points[b])))
                    .unwrap_or(0)
            }
            (None, _) => 0,
        };
        let patch = if matches!(surface, Surface::Plane { .. }) {
            None
        } else {
            // Die Hülle hängt nicht von der Reihenfolge der Schleifen ab.
            Some(derive_patch(&surface, &points).map_err(|Skip(m)| self.fail(id, m))?)
        };
        let outer_loop = loops[outer].clone();
        let inner = loops
            .into_iter()
            .enumerate()
            .filter(|(i, _)| *i != outer)
            .map(|(_, l)| l)
            .collect();
        Ok(RawFace {
            surface,
            same_sense: same_sense ^ flipped ^ reverse,
            outer: outer_loop,
            inner,
            patch,
            tag: FaceTag {
                origin: 0,
                role: format!("{STEP_FACE_ROLE} {n}"),
            },
        })
    }
}

/// Kantenparameter: `t0` am Anfang, `t1` am Ende. Gerade: `t = (p − origin) · dir` (läuft die
/// Kante gegen die Gerade, ist `t1 < t0` von selbst). Geschlossene Kurven (Kreis, Ellipse,
/// geschlossene Freiformkurve): in Laufrichtung (`same_sense`) entfaltet, mit wahrem Sinn
/// `t1 = t0 + rem(t_end − t0, Periode)`, sonst spiegelbildlich; gleiche Anfangs- und Endecke
/// ergeben einen vollen Umlauf (`t0 ± Periode`). `t1 < t0` ist erlaubt.
pub fn edge_params(
    curve: &Curve,
    start: DVec3,
    end: DVec3,
    same_sense: bool,
) -> Result<(f64, f64), Skip> {
    let t0 = curve.project(start);
    let te = curve.project(end);
    if !(t0.is_finite() && te.is_finite()) {
        return Err(Skip(
            "Die Lage einer Ecke auf ihrer Kante lässt sich nicht bestimmen.".into(),
        ));
    }
    let Some(period) = curve.period() else {
        return Ok((t0, te));
    };
    let span = if start == end {
        period
    } else if same_sense {
        (te - t0).rem_euclid(period)
    } else {
        (t0 - te).rem_euclid(period)
    };
    Ok((t0, if same_sense { t0 + span } else { t0 - span }))
}

/// Parameterbereich einer gekrümmten Fläche aus den Punkten ihrer Schleifen (je Schleife in
/// Laufrichtung, dicht genug, dass zwei Nachbarn nie eine halbe Periode auseinanderliegen).
/// Jeder Punkt wird mit `project_near` (Startwert: der vorige) zurückgerechnet; periodische
/// Richtungen werden entlang der Schleife fortlaufend entfaltet (Sprung > halbe Periode →
/// ±Periode). An Polen (Kugelpol, Kegelspitze) zählt nur `v`. Ergebnis ist die Hülle, in jeder
/// periodischen Richtung um Vielfache der Periode an den Anfang des Bereichs geschoben (analytisch
/// `[0, 2π)`); mehr als eine Periode oder keine Ausdehnung ist kein Bereich.
// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn derive_patch(surface: &Surface, loops: &[Vec<DVec3>]) -> Result<UvRect, Skip> {
    let periods = [surface.u_period(), surface.v_period()];
    let base = match surface {
        Surface::Nurbs(d) => {
            let ((u0, _), (v0, _)) = d.surface().domain();
            [u0, v0]
        }
        _ => [0.0, 0.0],
    };
    let poles = surface.pole_vs();
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for lp in loops {
        let mut hint: Option<(f64, f64)> = None;
        let mut prev: [Option<f64>; 2] = [None, None];
        for p in lp {
            let (u, v) = match hint {
                Some(h) => surface.project_near(*p, h),
                None => surface.project(*p),
            };
            if !(u.is_finite() && v.is_finite()) {
                return Err(Skip(
                    "Ein Randpunkt lässt sich nicht auf seine Fläche zurückrechnen.".into(),
                ));
            }
            hint = Some((u, v));
            let at_pole = poles
                .iter()
                .any(|pv| (v - pv).abs() <= POLE_REL * (1.0 + pv.abs()));
            for (k, mut x) in [u, v].into_iter().enumerate() {
                if k == 0 && at_pole {
                    continue;
                }
                if let (Some(per), Some(q)) = (periods[k], prev[k]) {
                    x += per * ((q - x) / per).round();
                }
                prev[k] = Some(x);
                lo[k] = lo[k].min(x);
                hi[k] = hi[k].max(x);
            }
        }
    }
    // Ohne Ausdehnung (etwa eine Kugel nur aus einer Naht von Pol zu Pol: `u` springt am Pol,
    // die Richtung des Sprungs folgt erst mit Aufgabe 6) ist es kein Bereich.
    let flat = |k: usize| !(hi[k] - lo[k] > EMPTY_REL * (1.0 + lo[k].abs()));
    if flat(0) || flat(1) {
        return Err(Skip(if flat(0) && !poles.is_empty() {
            "Eine Fläche, deren Rand durch einen Pol läuft, wird noch nicht unterstützt.".into()
        } else {
            "Der Parameterbereich einer Fläche lässt sich nicht bestimmen.".into()
        }));
    }
    for k in 0..2 {
        if let Some(per) = periods[k] {
            if hi[k] - lo[k] > per * (1.0 + PERIOD_SLACK) {
                return Err(Skip(
                    "Der Parameterbereich einer Fläche reicht über mehr als einen Umlauf.".into(),
                ));
            }
            let shift = per * ((lo[k] - base[k]) / per).floor();
            lo[k] -= shift;
            hi[k] -= shift;
        }
    }
    Ok(UvRect {
        u0: lo[0],
        u1: hi[0],
        v0: lo[1],
        v1: hi[1],
    })
}

/// Liest die Rohform der Volumen-Einträge `items`.
fn read_raw(
    file: &StepFile,
    units: Units,
    items: &[u64],
    progress: &Progress,
) -> Result<RawBody, Skip> {
    let mut r = TopoReader {
        file,
        res: Resolver::new(file, units),
        vertex_of: HashMap::new(),
        edge_of: HashMap::new(),
        raw: RawBody::default(),
    };
    let Some(&first) = items.first() else {
        return Err(Skip("Der Körper hat keine Flächen.".into()));
    };
    if items.len() > 1 {
        // Mehrere Stücke in einem Körper: Aufgabe 6.
        return Err(Skip(
            "Ein Körper aus mehreren Stücken wird noch nicht unterstützt.".into(),
        ));
    }
    let (e, kind) = r.expect(first, &["MANIFOLD_SOLID_BREP", "BREP_WITH_VOIDS"])?;
    if kind == "BREP_WITH_VOIDS" {
        // Hohlräume: Aufgabe 6.
        return Err(Skip(
            "Ein Körper mit Hohlräumen wird noch nicht unterstützt.".into(),
        ));
    }
    let outer = r.reference(first, &e, kind, 1)?;
    let (faces, reverse) = r.shell(outer)?;
    if faces.len() > file.limits().max_faces {
        return Err(Skip(format!(
            "Der Körper hat mehr als {} Flächen.",
            grouped(MAX_STEP_FACES)
        )));
    }
    let mut shell = RawShell::default();
    for (i, f) in faces.into_iter().enumerate() {
        if progress.is_cancelled() {
            return Err(Skip("Abgebrochen".into()));
        }
        let face = r.face(f, i + 1, reverse)?;
        shell.faces.push(r.raw.faces.len());
        r.raw.faces.push(face);
        progress.advance(1);
    }
    r.raw.shells.push(shell);
    Ok(r.raw)
}

fn invalid(e: impl std::fmt::Display) -> Skip {
    Skip(format!("Der Körper ist ungültig: {e}"))
}

/// Dreht jede Fläche um: `same_sense` gekippt, Schleifen rückwärts.
fn turn_inside_out(solid: &mut Solid) {
    let ids: Vec<Id<Face>> = solid.faces.iter().map(|(id, _)| id).collect();
    for id in ids {
        let f = &mut solid.faces[id];
        f.same_sense = !f.same_sense;
        f.outer = f.outer.reversed();
        f.inner = f.inner.iter().map(Loop::reversed).collect();
    }
}

/// Läuft die äußere Schleife jeder Fläche gegen den Uhrzeigersinn um ihre Außennormale? Sonst
/// passen `same_sense` und Ränder einer Fläche nicht zusammen (das Volumen sähe das nicht).
// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_face_turns(solid: &Solid) -> Result<(), Skip> {
    for (id, face) in solid.faces.iter() {
        let g = face_geom(solid, id);
        let pts = loop_points(&g.loops[0]);
        let poly = points_to_uv(&g.surface, g.same_sense, &pts);
        let area = oriented_area(&poly, g.same_sense);
        if !(area > 0.0) {
            let role = face.tag.as_ref().map_or("", |t| t.role.as_str());
            return Err(Skip(format!(
                "Die Fläche „{role}“ ist verdreht: Rand und Außenseite passen nicht zusammen."
            )));
        }
    }
    Ok(())
}

/// Vorzeichenbehaftetes Volumen aus einem groben Netz.
fn volume(solid: &Solid) -> Result<f64, Skip> {
    let mesh = tessellate(
        solid,
        &TessellationOptions {
            chord_tolerance: CLASSIFY_CHORD,
        },
    )
    .map_err(|e| Skip(format!("Der Körper lässt sich nicht vernetzen: {e}")))?;
    let v = mass_properties(&mesh).volume;
    if !v.is_finite() || v == 0.0 {
        return Err(Skip("Der Körper hat kein Volumen.".into()));
    }
    Ok(v)
}

/// Jede Kante liegt mit ihrer Mitte auf ihren Flächen, jede Ecke auf ihren Kanten.
// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_edges(solid: &Solid, weld_tol: f64) -> Result<(), Skip> {
    let tol = Tolerance::default();
    let mut faces_of: Vec<Vec<Id<Face>>> = vec![Vec::new(); solid.edges.len()];
    for (fid, f) in solid.faces.iter() {
        for lp in std::iter::once(&f.outer).chain(f.inner.iter()) {
            for he in &lp.half_edges {
                faces_of[he.edge.index()].push(fid);
            }
        }
    }
    for (eid, e) in solid.edges.iter() {
        let mid = e.curve.point(0.5 * (e.t0 + e.t1));
        for &fid in &faces_of[eid.index()] {
            let s = &solid.faces[fid].surface;
            let limit = weld_tol.max(point_surface_tol(s, &tol));
            if !(s.implicit(mid).abs() <= limit) {
                return Err(Skip(format!(
                    "Die Kante bei {} liegt nicht auf ihren Flächen.",
                    place(mid)
                )));
            }
        }
    }
    for (_, e) in solid.edges.iter() {
        let limit = if matches!(e.curve, Curve::Nurbs(_)) {
            weld_tol.max(NURBS_EDGE_TOL)
        } else {
            weld_tol.max(tol.linear)
        };
        for (t, v) in [(e.t0, e.start), (e.t1, e.end)] {
            let p = solid.vertices[v].point;
            if !(e.curve.point(t).distance(p) <= limit) {
                return Err(Skip(format!(
                    "Die Ecke bei {} liegt nicht auf ihrer Kante.",
                    place(p)
                )));
            }
        }
    }
    Ok(())
}

/// Baut aus den Volumen-Einträgen `items` (`MANIFOLD_SOLID_BREP`; `BREP_WITH_VOIDS` und mehrere
/// Stücke folgen in Aufgabe 6) einen geprüften Körper. Mehr als `file.limits().max_faces`
/// Flächen, ungültige Topologie, verdrehte Flächen, Kanten neben ihren Flächen oder Ecken neben
/// ihren Kanten ergeben ein [`Skip`] mit Grund; ein nach innen gekehrter Körper wird umgedreht.
/// `progress` zählt je Fläche eins weiter; Abbruch ergibt `Skip("Abgebrochen")`.
pub fn build_solid(
    file: &StepFile,
    units: Units,
    items: &[u64],
    opts: &BuildOptions,
    progress: &Progress,
) -> Result<BuiltSolid, Skip> {
    let raw = read_raw(file, units, items, progress)?;
    let mut solid = raw.to_solid();
    // Erst prüfen, dann vernetzen: Das Netz für das Volumen setzt gültige Topologie voraus. Die
    // Prüfung am Ende (nach dem Umdrehen) verlangt der Ablauf des Plans; sie ist billig.
    validate(&solid).map_err(invalid)?;
    check_face_turns(&solid)?;
    if volume(&solid)? < 0.0 {
        turn_inside_out(&mut solid);
    }
    check_edges(&solid, opts.weld_tol_mm)?;
    validate(&solid).map_err(invalid)?;
    Ok(BuiltSolid {
        faces: solid.faces.len(),
        solid,
        warnings: Vec::new(),
    })
}
