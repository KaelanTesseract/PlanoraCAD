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
//! Der Aufbau geht über eine Rohform ([`RawBody`]); sie wird bereinigt (Zusammenziehen, Paaren
//! und Nachziehen der Ecken: Aufgabe 7, [`crate::read::weld`]; Pole, entartete Kanten, fehlende
//! Nähte: Aufgabe 6, [`crate::read::seam`]), dann entsteht daraus der Körper,
//! der geprüft wird: `validate`, je Fläche das Schließen jeder Schleife im Parameterraum und ihr
//! Umlaufsinn (außen gegen, Löcher im Uhrzeigersinn), Außenrichtung je Schale über ihr Volumen
//! (äußere Schalen positiv, Hohlräume negativ, sonst umgedreht), Lage jeder Kante auf ihren
//! Flächen und jeder Ecke auf ihren Kanten.

use crate::read::error::MAX_STEP_FACES;
use crate::read::geom::{Resolver, Skip, SurfaceGeom};
use crate::read::index::StepFile;
use crate::read::seam;
use crate::read::spline::{close_curve, finish_cone, finish_extrusion_with_tol};
use crate::read::units::Units;
use crate::read::value::Entity;
use crate::read::weld;
use crate::topology::CLASSIFY_CHORD;
use planora_exchange::Progress;
use planora_kernel::boolean::{face_geom, loop_points, oriented_area, points_to_uv};
use planora_kernel::{
    Curve, Face, FaceTag, HalfEdge, Id, Loop, NURBS_EDGE_TOL, Shell, Solid, Surface, UvRect,
    ValidationError, point_surface_tol, validate,
};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_mesh::{MeshError, TessellationOptions, tessellate};
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

/// Höchstens so viel (Anteil der Periode) darf ein Bereich grober Dateien über eine Periode
/// reichen, bevor er gekappt wird (bei 1e-3 mm Genauigkeit genügt das ab etwa 0,3 mm Radius).
const MAX_PERIOD_SLACK: f64 = 1e-3;

/// Schritt (Anteil der Periode) für die Länge je Parametereinheit in [`derive_patch_sensed`].
const SPEED_STEP: f64 = 1e-6;

/// Abstand (relativ), unter dem `v` auf einem Pol der Fläche liegt (dort ist `u` beliebig);
/// derselbe Wert wie in `planora_kernel::boolean::points_to_uv`.
const POLE_REL: f64 = 1e-7;

/// Grund aus [`derive_patch_sensed`], wenn ein Rand am Pol zur falschen Seite springt (Rand und
/// Sinn der Fläche passen nicht zusammen); [`finish_face`] nennt dazu die Fläche.
const TWISTED_AT_POLE: &str = "Rand und Außenseite passen am Pol nicht zusammen.";

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
pub struct RawHalfEdge {
    pub edge: usize,
    pub forward: bool,
}

/// Kante der Rohform: Kurve über `t0 → t1` von Ecke `start` nach `end` (Indizes in
/// [`RawBody::vertices`]); `line` ist die Zeile ihres Eintrags für Meldungen (0: vom Leser
/// eingefügt, etwa eine Naht).
#[derive(Clone, Debug)]
pub struct RawEdge {
    pub curve: Curve,
    pub t0: f64,
    pub t1: f64,
    pub start: usize,
    pub end: usize,
    pub line: usize,
}

/// Fläche der Rohform; `same_sense` schon mit `flipped` und der Schalenrichtung verrechnet.
/// `loops` sind die Ränder (ohne `VERTEX_LOOP`); nach dem Fertigstellen ist der erste der äußere,
/// vorher zeigt `marked_outer` auf den in der Datei als äußerer gekennzeichneten. `line` ist die
/// Zeile der Fläche für Meldungen.
#[derive(Clone, Debug)]
pub struct RawFace {
    pub surface: Surface,
    pub same_sense: bool,
    pub loops: Vec<Vec<RawHalfEdge>>,
    pub marked_outer: Option<usize>,
    pub patch: Option<UvRect>,
    pub tag: FaceTag,
    pub line: usize,
    /// Pole, die die Datei als `VERTEX_LOOP` nennt (Punkte); eine eingefügte Naht muss zu einem
    /// davon laufen.
    pub pole_marks: Vec<DVec3>,
    /// Die Seite der Fläche folgt nur aus dem Umlaufsinn ihres Rands (Kugelkappe ohne Naht und
    /// ohne Pol); ein solcher Körper darf nicht umgedreht werden.
    pub side_guessed: bool,
}

/// Schale der Rohform: Indizes in [`RawBody::faces`]; `void`: Hohlraum (`BREP_WITH_VOIDS`).
#[derive(Clone, Debug, Default)]
pub struct RawShell {
    pub faces: Vec<usize>,
    pub void: bool,
    /// Bei Hohlräumen: Index der äußeren Schale, in der sie liegen.
    pub owner: Option<usize>,
}

/// Rohform eines Körpers: Ecken, Kanten, Flächen, Schalen (Aufgaben 6 und 7 arbeiten darauf).
#[derive(Clone, Debug, Default)]
pub struct RawBody {
    pub vertices: Vec<DVec3>,
    pub edges: Vec<RawEdge>,
    pub faces: Vec<RawFace>,
    pub shells: Vec<RawShell>,
}

/// Meldung mit der Zeile davor („Zeile n: …“), wenn sie bekannt ist.
pub(crate) fn at_line(line: usize, msg: impl Into<String>) -> Skip {
    let msg = msg.into();
    if line > 0 {
        Skip(format!("Zeile {line}: {msg}"))
    } else {
        Skip(msg)
    }
}

impl RawBody {
    /// Anfangsecke einer Halbkante.
    pub fn he_start(&self, he: RawHalfEdge) -> usize {
        let e = &self.edges[he.edge];
        if he.forward { e.start } else { e.end }
    }

    /// Endecke einer Halbkante.
    pub fn he_end(&self, he: RawHalfEdge) -> usize {
        let e = &self.edges[he.edge];
        if he.forward { e.end } else { e.start }
    }

    /// Punkte einer Schleife in Laufrichtung: je Halbkante beide Ecken und
    /// [`EDGE_SAMPLES`] Zwischenpunkte.
    pub fn loop_points(&self, lp: &[RawHalfEdge]) -> Vec<DVec3> {
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
                outer: f
                    .loops
                    .first()
                    .map_or(Loop { half_edges: vec![] }, |l| kernel_loop(l)),
                inner: f.loops.iter().skip(1).map(|l| kernel_loop(l)).collect(),
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

    /// Entfernt Kanten, die keine Schleife mehr benutzt, und Ecken ohne Kante (nach dem
    /// Entfernen entarteter Kanten und dem Teilen für Nähte); die Indizes werden neu vergeben,
    /// die Reihenfolge bleibt.
    pub fn compact(&mut self) {
        let mut used = vec![false; self.edges.len()];
        for f in &self.faces {
            for he in f.loops.iter().flatten() {
                used[he.edge] = true;
            }
        }
        let mut edge_map = vec![usize::MAX; self.edges.len()];
        let mut edges = Vec::with_capacity(self.edges.len());
        for (k, e) in std::mem::take(&mut self.edges).into_iter().enumerate() {
            if used[k] {
                edge_map[k] = edges.len();
                edges.push(e);
            }
        }
        let mut vertex_map = vec![usize::MAX; self.vertices.len()];
        let mut vertices = Vec::with_capacity(self.vertices.len());
        for e in &mut edges {
            for v in [&mut e.start, &mut e.end] {
                if vertex_map[*v] == usize::MAX {
                    vertex_map[*v] = vertices.len();
                    vertices.push(self.vertices[*v]);
                }
                *v = vertex_map[*v];
            }
        }
        self.vertices = vertices;
        self.edges = edges;
        for f in &mut self.faces {
            for he in f.loops.iter_mut().flatten() {
                he.edge = edge_map[he.edge];
            }
        }
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

pub(crate) fn place(p: DVec3) -> String {
    format!("({}; {}; {})", coord(p.x), coord(p.y), coord(p.z))
}

/// Liest die Topologie eines Körpers.
struct TopoReader<'f> {
    file: &'f StepFile,
    /// Genauigkeit der Datei (mm) für den Parameterbereich grober Nähte.
    tol: f64,
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
        let mut curve = self.res.curve(curve)?;
        let (p, q) = (self.raw.vertices[start], self.raw.vertices[end]);
        let linear = Tolerance::default().linear;
        // Eine Freiformkurve mit einer Ecke an beiden Enden, die sich nur bis zur Genauigkeit
        // der Datei schließt, wird genau geschlossen (beide Enden auf ihre Mitte); liegen ihre
        // Enden weiter auseinander, liegt die Ecke nicht auf beiden Enden.
        if let Curve::Nurbs(c) = &curve
            && curve.period().is_none()
            && p.distance(q) <= linear
        {
            let (a, b) = c.domain();
            let gap = c.point(a).distance(c.point(b));
            if let Some(closed) = close_curve(c, self.tol) {
                curve = Curve::nurbs(closed);
            } else if gap > self.tol.max(linear) {
                return Err(Skip(format!(
                    "Die Ecke bei {} liegt nicht auf ihrer Kante.",
                    place(p)
                )));
            }
        }
        // Eine offene Kurve zwischen zusammenfallenden Ecken hat die Länge null (geschlossene
        // Kurven ergeben dort einen vollen Umlauf); sie wird mit den übrigen entarteten Kanten
        // entfernt (`seam::drop_degenerate`), erst danach fällt sie gegebenenfalls auf.
        let (t0, t1) = edge_params(&curve, p, q, same_sense).map_err(|Skip(m)| self.fail(id, m))?;
        let k = self.raw.edges.len();
        self.raw.edges.push(RawEdge {
            curve,
            t0,
            t1,
            start,
            end,
            line: self.file.line(id).unwrap_or(0),
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
    /// Ein `VERTEX_LOOP` (Pol, Kegelspitze) ist kein Rand für den Kernel: sein Punkt als Pol.
    fn bound(&mut self, id: u64, reverse: bool) -> Result<Bound, Skip> {
        let (e, kind) = self.expect(id, &["FACE_OUTER_BOUND", "FACE_BOUND"])?;
        let lp = self.reference(id, &e, kind, 1)?;
        let along = self.flag(id, &e, kind, 2)?;
        let le = self.file.entity(lp)?;
        if le.is("VERTEX_LOOP") {
            let v = self.reference(lp, &le, "VERTEX_LOOP", 1)?;
            let (ve, kind) = self.expect(v, &["VERTEX_POINT"])?;
            let p = self.reference(v, &ve, kind, 1)?;
            return Ok(Bound::Pole(self.res.point(p)?));
        }
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
        Ok(Bound::Loop(hes, kind == "FACE_OUTER_BOUND"))
    }

    /// Eine Fläche mit der Marke `n` (ab 1); `reverse`: Schale umgekehrt. Äußerer Rand und
    /// Parameterbereich folgen erst nach dem Bereinigen ([`finish_face`]); `VERTEX_LOOP`s werden
    /// verworfen, eine Fläche ohne Rand kommt bis zum Einfügen der Nähte durch.
    fn face(&mut self, id: u64, n: usize, reverse: bool) -> Result<RawFace, Skip> {
        let (e, kind) = self.expect(id, &["ADVANCED_FACE", "FACE_SURFACE"])?;
        let bounds = self.refs(id, &e, kind, 1)?;
        let geometry = self.reference(id, &e, kind, 2)?;
        let same_sense = self.flag(id, &e, kind, 3)?;
        let geom = self.res.surface(geometry)?;
        let mut loops = Vec::with_capacity(bounds.len());
        let mut marked_outer: Option<usize> = None;
        let mut pole_marks = Vec::new();
        for b in &bounds {
            let (lp, outer) = match self.bound(*b, reverse)? {
                Bound::Loop(lp, outer) => (lp, outer),
                Bound::Pole(p) => {
                    pole_marks.push(p);
                    continue;
                }
            };
            if outer {
                if marked_outer.is_some() {
                    return Err(self.fail(id, "Eine Fläche hat mehr als einen äußeren Rand."));
                }
                marked_outer = Some(loops.len());
            }
            loops.push(lp);
        }
        // Eine Strangfläche aus einer Freiformkurve bekommt ihren Bereich aus den Schleifen (nie
        // gekippt: ihre natürliche Normale ist die von STEP), ein Kegel aus einer Geraden seine
        // Hälfte (gemessen).
        let (surface, flipped) = match geom {
            SurfaceGeom::Ready { surface, flipped } => (surface, flipped),
            SurfaceGeom::Extrusion { curve, dir } => {
                let pts: Vec<Vec<DVec3>> = loops.iter().map(|l| self.raw.loop_points(l)).collect();
                let surface = finish_extrusion_with_tol(&curve, dir, &pts, self.tol)
                    .map_err(|Skip(m)| self.fail(geometry, m))?;
                (surface, false)
            }
            SurfaceGeom::LineCone {
                line_origin,
                dir,
                axis_origin,
                axis,
            } => {
                let pts: Vec<Vec<DVec3>> = loops.iter().map(|l| self.raw.loop_points(l)).collect();
                finish_cone(line_origin, dir, axis_origin, axis, &pts)
                    .map_err(|Skip(m)| self.fail(geometry, m))?
            }
        };
        Ok(RawFace {
            surface,
            same_sense: same_sense ^ flipped ^ reverse,
            loops,
            marked_outer,
            patch: None,
            tag: FaceTag {
                origin: 0,
                role: format!("{STEP_FACE_ROLE} {n}"),
            },
            line: self.file.line(id).unwrap_or(0),
            pole_marks,
            side_guessed: false,
        })
    }
}

/// Ein Rand einer Fläche: Schleife (mit Kennzeichen „äußerer Rand“) oder Pol (`VERTEX_LOOP`).
enum Bound {
    Loop(Vec<RawHalfEdge>, bool),
    Pole(DVec3),
}

/// Stellt die Fläche `i` fertig: äußerer Rand (gekennzeichnet; sonst bei Ebenen der mit der
/// größten Fläche, bei gekrümmten Flächen der, der am deutlichsten gegen den Uhrzeigersinn um die
/// Außennormale läuft) als erste Schleife, Parameterbereich gekrümmter Flächen aus allen
/// Schleifen (äußerer Rand zuerst, Pole nach der Regel des Kernels). Ohne Schleife: „Eine Fläche
/// hat keinen Rand.“
pub(crate) fn finish_face(raw: &mut RawBody, i: usize, tol: f64) -> Result<(), Skip> {
    let f = &raw.faces[i];
    if f.loops.is_empty() {
        return Err(at_line(f.line, "Eine Fläche hat keinen Rand."));
    }
    let surface = &f.surface;
    let sense = f.same_sense;
    let plane = matches!(surface, Surface::Plane { .. });
    // Punkte der Schleifen nur, wo sie gebraucht werden: Wahl des äußeren Rands ohne
    // Kennzeichnung und Parameterbereich gekrümmter Flächen.
    let points: Vec<Vec<DVec3>> = if plane && f.marked_outer.is_some() {
        Vec::new()
    } else {
        // Auf Flächen mit Polen dichter, wo u schnell läuft (Rand nahe einem Pol).
        if surface.pole_vs().is_empty() {
            f.loops.iter().map(|l| raw.loop_points(l)).collect()
        } else {
            f.loops
                .iter()
                .map(|l| seam::fine_loop_points(raw, surface, l))
                .collect()
        }
    };
    let outer = match (f.marked_outer, surface) {
        (Some(k), Surface::Plane { .. }) => k,
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
            (0..points.len())
                .max_by(|&a, &b| area(&points[a]).total_cmp(&area(&points[b])))
                .unwrap_or(0)
        }
        (marked, _) => {
            // Gekrümmt: der Rand, der am deutlichsten gegen den Uhrzeigersinn um die
            // Außennormale läuft (Löcher laufen andersherum). Eine Kennzeichnung „außen“ gilt
            // nur, wenn ihr Rand auch so läuft: Auf geschlossenen Flächen (Kugelband ohne Pol)
            // ist „außen“ in STEP nicht eindeutig.
            let turn = |pts: &[DVec3]| oriented_area(&points_to_uv(surface, sense, pts), sense);
            match marked {
                Some(k) if turn(&points[k]) > 0.0 => k,
                _ => (0..points.len())
                    .max_by(|&a, &b| turn(&points[a]).total_cmp(&turn(&points[b])))
                    .unwrap_or(0),
            }
        }
    };
    let patch = if plane {
        None
    } else {
        // Äußerer Rand zuerst: weitere Schleifen werden neben ihm entfaltet.
        let mut ordered = Vec::with_capacity(points.len());
        ordered.extend(points.get(outer).cloned());
        ordered.extend(
            points
                .iter()
                .enumerate()
                .filter(|(k, _)| *k != outer)
                .map(|(_, p)| p.clone()),
        );
        Some(
            derive_patch_sensed(surface, sense, &ordered, tol).map_err(|Skip(m)| {
                if m == TWISTED_AT_POLE {
                    Skip(format!(
                        "Die Fläche „{}“ ist verdreht: Rand und Außenseite passen nicht zusammen.",
                        f.tag.role
                    ))
                } else {
                    at_line(f.line, m)
                }
            })?,
        )
    };
    let f = &mut raw.faces[i];
    if outer < f.loops.len() {
        let first = f.loops.remove(outer);
        f.loops.insert(0, first);
    }
    f.marked_outer = Some(0);
    f.patch = patch;
    Ok(())
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
/// Laufrichtung, dicht genug, dass zwei Nachbarn nie eine halbe Periode auseinanderliegen; der
/// äußere Rand zuerst) und dem Sinn der Fläche. Jeder Punkt wird mit `project_near` (Startwert:
/// der vorige) zurückgerechnet; periodische Richtungen werden entlang der Schleife fortlaufend
/// entfaltet (Sprung > halbe Periode → ±Periode), jede weitere Schleife beginnt neben dem schon
/// bestimmten Bereich. Läuft eine Schleife durch einen Pol (Kugelpol, Kegelspitze; `v` am Pol
/// relativ [`POLE_REL`] wie im Kernel oder der Punkt höchstens `tol_mm` vom Polpunkt), folgt `u`
/// der Regel des Kernels (`boolean::points_to_uv`: Sprung über den Pol in die Richtung, auf der
/// die Fläche liegt). Ergebnis ist die Hülle, in jeder periodischen Richtung um Vielfache der
/// Periode an den Anfang des Bereichs geschoben (analytisch `[0, 2π)`). Für Dateien mit der
/// Genauigkeit `tol_mm`: Bis `2 · tol_mm / Geschwindigkeit` (kleinste Länge je Parameter an den
/// Randpunkten, Punkte näher als `10 · tol_mm` an einem Pol ausgenommen; mindestens
/// [`PERIOD_SLACK`], höchstens [`MAX_PERIOD_SLACK`] der Periode) über eine Periode wird der
/// Bereich auf genau eine Periode gekappt; darüber oder ohne Ausdehnung ist er kein Bereich.
pub fn derive_patch_sensed(
    surface: &Surface,
    same_sense: bool,
    loops: &[Vec<DVec3>],
    tol_mm: f64,
) -> Result<UvRect, Skip> {
    derive_patch_impl(surface, same_sense, loops, tol_mm)
}

/// Randpunkte im Parameterraum: `u`, `v` (entfaltet), ob `u` für den Bereich zählt und ob der
/// Punkt auf einem Pol liegt.
type Track = Vec<(f64, f64, bool, bool)>;

// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn derive_patch_impl(
    surface: &Surface,
    sense: bool,
    loops: &[Vec<DVec3>],
    tol_mm: f64,
) -> Result<UvRect, Skip> {
    let periods = [surface.u_period(), surface.v_period()];
    let base = match surface {
        Surface::Nurbs(d) => {
            let ((u0, _), (v0, _)) = d.surface().domain();
            [u0, v0]
        }
        _ => [0.0, 0.0],
    };
    let poles = surface.pole_vs();
    let pole_points: Vec<DVec3> = poles.iter().map(|&pv| surface.point(base[0], pv)).collect();
    let param_pole = |v: f64| {
        poles
            .iter()
            .any(|pv| (v - pv).abs() < POLE_REL * (1.0 + pv.abs()))
    };
    let geo_pole = |p: DVec3| {
        pole_points
            .iter()
            .copied()
            .find(|q| tol_mm > 0.0 && q.distance(p) <= tol_mm)
    };
    let fold = |k: usize, x: f64| match periods[k] {
        Some(per) => base[k] + (x - base[k]).rem_euclid(per),
        None => x,
    };
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    // Kleinste Länge je Parametereinheit an den Randpunkten, je periodischer Richtung.
    let mut speed = [f64::INFINITY; 2];
    let mut through_pole = false;
    for lp in loops {
        // Jede weitere Schleife beginnt neben dem schon bestimmten Bereich (Loch nahe der
        // Naht): ihr erster Wert wird um Vielfache der Periode an dessen Mitte gelegt.
        let start: [Option<f64>; 2] =
            [0, 1].map(|k| (lo[k] <= hi[k]).then(|| 0.5 * (lo[k] + hi[k])));
        let mut track: Track = Vec::with_capacity(lp.len());
        let mut hint: Option<(f64, f64)> = None;
        let mut prev = start;
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
            let at_pole = param_pole(v) || geo_pole(*p).is_some();
            let mut x = [u, v];
            for k in 0..2 {
                if k == 0 && at_pole {
                    continue;
                }
                if let (Some(per), Some(q)) = (periods[k], prev[k]) {
                    x[k] += per * ((q - x[k]) / per).round();
                }
                prev[k] = Some(x[k]);
            }
            track.push((x[0], x[1], !at_pole, at_pole));
        }
        if track.iter().any(|t| t.3) {
            // Durch einen Pol: `u` nach der Regel des Kernels (Polpunkte genau auf den Pol).
            through_pole = true;
            let pts: Vec<DVec3> = lp.iter().map(|p| geo_pole(*p).unwrap_or(*p)).collect();
            track = points_to_uv(surface, sense, &pts)
                .into_iter()
                .map(|q| (q.x, q.y, true, param_pole(q.y)))
                .collect();
            for k in 0..2 {
                let first = track
                    .iter()
                    .find(|t| k == 1 || !t.3)
                    .map(|t| if k == 0 { t.0 } else { t.1 });
                if let (Some(per), Some(q), Some(f)) = (periods[k], start[k], first) {
                    let shift = per * ((q - f) / per).round();
                    for t in &mut track {
                        if k == 0 {
                            t.0 += shift;
                        } else {
                            t.1 += shift;
                        }
                    }
                }
            }
        }
        for &(u, v, count_u, at_pole) in &track {
            for (k, x) in [u, v].into_iter().enumerate() {
                if k == 0 && !count_u {
                    continue;
                }
                if let Some(per) = periods[k].filter(|_| tol_mm > 0.0 && !at_pole) {
                    let (uf, vf) = (fold(0, u), fold(1, v));
                    let h = SPEED_STEP * per;
                    let q = if k == 0 {
                        surface.point(uf + h, vf)
                    } else {
                        surface.point(uf, vf + h)
                    };
                    let s = q.distance(surface.point(uf, vf)) / h;
                    if s.is_finite() && s > 10.0 * tol_mm {
                        speed[k] = speed[k].min(s);
                    }
                }
                lo[k] = lo[k].min(x);
                hi[k] = hi[k].max(x);
            }
        }
    }
    // Ohne Ausdehnung ist es kein Bereich.
    let flat = |k: usize| !(hi[k] - lo[k] > EMPTY_REL * (1.0 + lo[k].abs()));
    if flat(0) || flat(1) {
        return Err(Skip(
            "Der Parameterbereich einer Fläche lässt sich nicht bestimmen.".into(),
        ));
    }
    for k in 0..2 {
        if let Some(per) = periods[k] {
            let slack = (2.0 * tol_mm / speed[k]).clamp(PERIOD_SLACK * per, MAX_PERIOD_SLACK * per);
            if !(hi[k] - lo[k] <= per + slack) {
                // Über einen Pol hinweg heißt das: Der Sprung am Pol läuft zur falschen Seite,
                // Rand und Sinn der Fläche passen nicht zusammen.
                return Err(Skip(if through_pole && k == 0 {
                    TWISTED_AT_POLE.into()
                } else {
                    "Der Parameterbereich einer Fläche reicht über mehr als einen Umlauf.".into()
                }));
            }
            if hi[k] - lo[k] > per {
                hi[k] = lo[k] + per;
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

/// Liest die Rohform der Volumen-Einträge `items` (`MANIFOLD_SOLID_BREP`, `BREP_WITH_VOIDS`):
/// je äußere Schale und je Hohlraum eine Schale (`void`), alle in einem Körper. Die Richtung der
/// Schalen (`ORIENTED_CLOSED_SHELL`) ist schon in die Flächen verrechnet; Marken „STEP-Fläche n“
/// laufen über alle Schalen. Noch nicht bereinigt (Pole, entartete Kanten, Nähte) und ohne
/// äußeren Rand und Parameterbereich (siehe [`build_solid`]).
pub fn read_raw(
    file: &StepFile,
    units: Units,
    items: &[u64],
    tol: f64,
    progress: &Progress,
) -> Result<RawBody, Skip> {
    let mut r = TopoReader {
        file,
        tol,
        res: Resolver::new(file, units),
        vertex_of: HashMap::new(),
        edge_of: HashMap::new(),
        raw: RawBody::default(),
    };
    if items.is_empty() {
        return Err(Skip("Der Körper hat keine Flächen.".into()));
    }
    // Schalen: (Flächen, umgekehrt, äußere Schale des Hohlraums).
    let mut shells: Vec<(Vec<u64>, bool, Option<usize>)> = Vec::new();
    for &item in items {
        let (e, kind) = r.expect(item, &["MANIFOLD_SOLID_BREP", "BREP_WITH_VOIDS"])?;
        let outer = r.reference(item, &e, kind, 1)?;
        let (faces, reverse) = r.shell(outer)?;
        let owner = shells.len();
        shells.push((faces, reverse, None));
        if kind == "BREP_WITH_VOIDS" {
            for void in r.refs(item, &e, kind, 2)? {
                let (faces, reverse) = r.shell(void)?;
                shells.push((faces, reverse, Some(owner)));
            }
        }
    }
    if shells.iter().any(|(faces, ..)| faces.is_empty()) {
        return Err(Skip("Der Körper hat keine Flächen.".into()));
    }
    let total: usize = shells.iter().map(|(faces, ..)| faces.len()).sum();
    if total > file.limits().max_faces {
        return Err(Skip(format!(
            "Der Körper hat mehr als {} Flächen.",
            grouped(MAX_STEP_FACES)
        )));
    }
    for (faces, reverse, owner) in shells {
        let mut shell = RawShell {
            faces: Vec::with_capacity(faces.len()),
            void: owner.is_some(),
            owner,
        };
        for f in faces {
            if progress.is_cancelled() {
                return Err(Skip("Abgebrochen".into()));
            }
            let n = r.raw.faces.len() + 1;
            let face = r.face(f, n, reverse)?;
            shell.faces.push(r.raw.faces.len());
            r.raw.faces.push(face);
            progress.advance(1);
        }
        r.raw.shells.push(shell);
    }
    Ok(r.raw)
}

/// Name der Fläche mit dem Kernel-Index `i` für Meldungen („„STEP-Fläche n““).
fn face_name(solid: &Solid, i: usize) -> String {
    match solid
        .faces
        .get(Id::from_index(i))
        .and_then(|f| f.tag.as_ref())
    {
        Some(t) => format!("„{}“", t.role),
        None => "einer Fläche".into(),
    }
}

/// Ort der Kante mit dem Kernel-Index `i` (Mitte) für Meldungen.
fn edge_place(solid: &Solid, i: usize) -> String {
    match solid.edges.get(Id::from_index(i)) {
        Some(e) => place(e.curve.point(0.5 * (e.t0 + e.t1))),
        None => "(?)".into(),
    }
}

/// „Der Körper ist ungültig: …“ mit Flächenmarken und Kantenorten statt Kernel-Nummern.
fn invalid(solid: &Solid, e: &ValidationError) -> Skip {
    use ValidationError as V;
    let why = match *e {
        V::EdgeUseCount(k, 1) => {
            format!(
                "Die Kante bei {} gehört nur zu einer Fläche.",
                edge_place(solid, k)
            )
        }
        V::EdgeUseCount(k, n) => format!(
            "Die Kante bei {} gehört zu {n} Flächen statt zu zwei.",
            edge_place(solid, k)
        ),
        V::EdgeOrientation(k) => format!(
            "Die Kante bei {} wird von beiden Flächen in derselben Richtung durchlaufen.",
            edge_place(solid, k)
        ),
        V::OpenLoop(f) => format!(
            "Ein Rand der Fläche {} ist nicht geschlossen.",
            face_name(solid, f)
        ),
        V::EmptyLoop(f) => format!("Ein Rand der Fläche {} ist leer.", face_name(solid, f)),
        V::MissingPatch(f) => format!(
            "Die Fläche {} hat keinen Parameterbereich.",
            face_name(solid, f)
        ),
        V::InvalidEulerCharacteristic(_) => {
            "Die Flächen schließen den Körper nicht richtig ab.".into()
        }
        V::ShellMembership(f) => format!(
            "Die Fläche {} gehört nicht zu genau einer Schale.",
            face_name(solid, f)
        ),
        V::EdgeOffSurface(k, f) => format!(
            "Die Kante bei {} liegt nicht auf der Fläche {}.",
            edge_place(solid, k),
            face_name(solid, f)
        ),
        V::HoleAcrossSeam(f) => format!(
            "Ein Loch der Fläche {} reicht über ihre Naht.",
            face_name(solid, f)
        ),
    };
    Skip(format!("Der Körper ist ungültig: {why}"))
}

/// Liegt der Wert eines Polygons im Parameterraum um ganze Perioden neben dem Anfang? Dann
/// windet sich die Schleife um die Achse statt sich zu schließen.
fn winds(poly: &[DVec2], axis: usize, period: f64) -> bool {
    let (Some(a), Some(b)) = (poly.first(), poly.last()) else {
        return false;
    };
    let (a, b) = if axis == 0 { (a.x, b.x) } else { (a.y, b.y) };
    ((b - a) / period).round() != 0.0
}

/// Ränder jeder Fläche im Parameterraum: Auf periodischen Flächen schließt sich jede Schleife
/// (ein Kreisbogen mit falschem `same_sense` läuft sonst um die Achse); dann läuft der äußere
/// Rand gegen den Uhrzeigersinn um die Außennormale und jedes Loch im Uhrzeigersinn. Sonst
/// passen `same_sense` und Ränder einer Fläche nicht zusammen (das Volumen sähe das nicht).
// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_face_loops(solid: &Solid) -> Result<(), Skip> {
    let polys: Vec<(Id<Face>, Vec<Vec<DVec2>>)> = solid
        .faces
        .iter()
        .map(|(id, _)| {
            let g = face_geom(solid, id);
            let polys = g
                .loops
                .iter()
                .map(|lp| points_to_uv(&g.surface, g.same_sense, &loop_points(lp)))
                .collect();
            (id, polys)
        })
        .collect();
    // Erst das Schließen (die genauere Ursache), dann der Umlaufsinn.
    for (id, loops) in &polys {
        let s = &solid.faces[*id].surface;
        let periods = [s.u_period(), s.v_period()];
        for poly in loops {
            for (axis, period) in periods.iter().enumerate() {
                if period.is_some_and(|p| winds(poly, axis, p)) {
                    return Err(Skip(format!(
                        "Der Rand der Fläche {} schließt sich nicht: Eine Kante läuft falsch herum.",
                        face_name(solid, id.index())
                    )));
                }
            }
        }
    }
    for (id, loops) in &polys {
        let sense = solid.faces[*id].same_sense;
        for (k, poly) in loops.iter().enumerate() {
            let area = oriented_area(poly, sense);
            let ok = if k == 0 { area > 0.0 } else { area < 0.0 };
            if !ok {
                return Err(Skip(format!(
                    "Die Fläche {} ist verdreht: Rand und Außenseite passen nicht zusammen.",
                    face_name(solid, id.index())
                )));
            }
        }
    }
    Ok(())
}

/// Dreht die Flächen `ids` um: `same_sense` gekippt, Schleifen rückwärts.
fn turn_inside_out(solid: &mut Solid, ids: &[Id<Face>]) {
    for &id in ids {
        let f = &mut solid.faces[id];
        f.same_sense = !f.same_sense;
        f.outer = f.outer.reversed();
        f.inner = f.inner.iter().map(Loop::reversed).collect();
    }
}

/// Dreiecke je Schale (Reihenfolge von `solid.shells`).
type ShellTriangles = Vec<Vec<[DVec3; 3]>>;

/// Je Dreieck einer Schale (wie [`ShellTriangles`]) der Index seiner Fläche.
type ShellTriangleFaces = Vec<Vec<usize>>;

/// Grobes Netz je Schale: Volumen mit Vorzeichen, Dreiecke, Flächen und die benutzte Sehne.
struct ShellMesh {
    volumes: Vec<f64>,
    triangles: ShellTriangles,
    faces: ShellTriangleFaces,
    chord: f64,
}

/// Sehne des groben Netzes als Anteil der Körpergröße (Hülldiagonale), höchstens
/// [`CLASSIFY_CHORD`].
const CHORD_REL: f64 = 1e-3;

/// Unter diesem Anteil der Summe aller Tetraederbeträge (Bezugspunkt: Mitte der Schale) hat
/// eine Schale kein Volumen: Zwei gegengleiche Flächen am selben Ort heben sich bis auf
/// Rundungsrauschen auf, das Vorzeichen wäre Zufall.
const VOLUME_REL: f64 = 1e-6;

/// Vorzeichenbehaftetes Volumen je Schale aus einem groben Netz (Reihenfolge von
/// `solid.shells`), die Dreiecke je Schale und je Dreieck seine Fläche. Eine Schale, deren
/// Volumen unter [`VOLUME_REL`] der Summe der Tetraederbeträge liegt, hat kein Volumen.
// Der negierte Vergleich lehnt absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn shell_volumes(solid: &Solid) -> Result<ShellMesh, Skip> {
    // Sehne höchstens [`CLASSIFY_CHORD`], bei kleinen Körpern [`CHORD_REL`] ihrer Größe: Sonst
    // schnitten sich die Netze dünner Wände, und die Probe sähe bei winzigen Körpern nichts.
    let (lo, hi) = bounds_of(solid.vertices.iter().map(|(_, v)| &v.point));
    let chord = CLASSIFY_CHORD
        .min(CHORD_REL * lo.distance(hi))
        .max(Tolerance::default().linear);
    let mesh = tessellate(
        solid,
        &TessellationOptions {
            chord_tolerance: chord,
        },
    )
    .map_err(|e| {
        Skip(match e {
            MeshError::Triangulation(f) | MeshError::MissingPatch(f) | MeshError::OffSurface(f) => {
                format!(
                    "Die Fläche {} lässt sich nicht in Dreiecke zerlegen.",
                    face_name(solid, f)
                )
            }
            MeshError::TooLarge => "Der Körper ist zu groß für ein Dreiecksnetz.".into(),
        })
    })?;
    let mut shell_of = vec![0usize; solid.faces.len()];
    for (k, s) in solid.shells.iter().enumerate() {
        for f in &s.faces {
            shell_of[f.index()] = k;
        }
    }
    let shells = solid.shells.len().max(1);
    let mut triangles: ShellTriangles = vec![Vec::new(); shells];
    let mut faces: ShellTriangleFaces = vec![Vec::new(); shells];
    for r in &mesh.face_ranges {
        let k = shell_of[r.face.index()];
        let start = r.first_index as usize;
        let end = start + r.index_count as usize;
        for t in mesh.indices[start..end].as_chunks::<3>().0 {
            triangles[k].push(t.map(|i| mesh.positions[i as usize]));
            faces[k].push(r.face.index());
        }
    }
    // Bezugspunkt je Schale in ihrer Mitte (kleinere Rundungsfehler als der Ursprung); die
    // Summe der Beträge sagt, ob das Ergebnis mehr als Rundungsrauschen ist.
    let mut volumes = vec![0.0; shells];
    for (k, tris) in triangles.iter().enumerate() {
        let (lo, hi) = bounds_of(tris.iter().flatten());
        let o = 0.5 * (lo + hi);
        let (mut sum, mut abs) = (0.0, 0.0);
        for t in tris {
            let [a, b, c] = t.map(|p| p - o);
            let v = a.dot(b.cross(c)) / 6.0;
            sum += v;
            abs += v.abs();
        }
        if !(sum.is_finite() && sum.abs() > VOLUME_REL * abs) {
            return Err(Skip("Der Körper hat kein Volumen.".into()));
        }
        volumes[k] = sum;
    }
    Ok(ShellMesh {
        volumes,
        triangles,
        faces,
        chord,
    })
}

/// Hüllquader von Punkten.
fn bounds_of<'a>(points: impl IntoIterator<Item = &'a DVec3>) -> (DVec3, DVec3) {
    points
        .into_iter()
        .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        })
}

/// Liegt `q` im um `d` vergrößerten Quader?
fn in_box(q: DVec3, (lo, hi): (DVec3, DVec3), d: f64) -> bool {
    q.cmpge(lo - DVec3::splat(d)).all() && q.cmple(hi + DVec3::splat(d)).all()
}

/// Überschneiden sich die um `d` vergrößerten Quader?
fn boxes_meet(a: (DVec3, DVec3), b: (DVec3, DVec3), d: f64) -> bool {
    (a.0 - DVec3::splat(d)).cmple(b.1).all() && (b.0 - DVec3::splat(d)).cmple(a.1).all()
}

/// Nächster Punkt des Dreiecks `t` zu `q` (Ericson, „Real-Time Collision Detection“, 5.1.5).
fn closest_on_triangle(q: DVec3, t: &[DVec3; 3]) -> DVec3 {
    let [a, b, c] = *t;
    let (ab, ac, ap) = (b - a, c - a, q - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = q - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = q - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w;
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Dreieck mit der Normale, die aus dem Material heraus zeigt, und seiner Fläche.
struct Facet {
    t: [DVec3; 3],
    n: DVec3,
    face: usize,
}

/// Höchstens so viele Zellen je Achse im Raster über die Dreiecke eines Stücks.
const GRID_CELLS: f64 = 48.0;

/// Wenigstens so viele Zellen je Achse.
const MIN_GRID_CELLS: f64 = 4.0;

/// Drei feste, zueinander schiefe Strahlrichtungen für die Umlaufzahl; keine liegt in einer
/// Achsen- oder Diagonalebene, damit ein Strahl nicht entlang von Kanten oder Ecken der
/// üblichen Netze läuft.
const RAY_DIRS: [[f64; 3]; 3] = [
    [0.3317, 0.7213, 0.6083],
    [-0.6271, 0.2117, 0.7496],
    [0.5407, -0.6329, 0.5541],
];

/// Gleichmäßiges Raster über Dreiecke (je Zelle die Dreiecke, deren Hüllquader sie trifft) für
/// die Suche nach dem nächsten Dreieck und für Strahlen durch das Netz (Umlaufzahl).
struct FacetGrid {
    lo: DVec3,
    cell: f64,
    dims: [usize; 3],
    cells: Vec<Vec<u32>>,
    facets: Vec<Facet>,
    /// Je leerer Zelle: liegt sie innen? Keine Fläche schneidet eine leere Zelle (jedes Dreieck
    /// steht in allen Zellen, die sein Hüllquader trifft), deshalb gilt das für die ganze Zelle
    /// und für benachbarte leere Zellen gleich.
    inside: Vec<Option<bool>>,
    /// Je Dreieck die Nummer des Strahls, der es zuletzt geprüft hat (ein Dreieck steht in
    /// mehreren Zellen und darf je Strahl nur einmal zählen).
    stamp: std::cell::RefCell<Vec<u32>>,
    rays: std::cell::Cell<u32>,
}

/// Schnitt des Strahls `o + t·d` (t > 0) mit dem Dreieck (Möller–Trumbore).
fn ray_triangle(o: DVec3, d: DVec3, t: &[DVec3; 3]) -> Option<f64> {
    let [a, b, c] = *t;
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() <= 1e-300 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let tt = e2.dot(q) * inv;
    (tt > 0.0).then_some(tt)
}

impl FacetGrid {
    fn new(facets: Vec<Facet>) -> FacetGrid {
        let (lo, hi) = bounds_of(facets.iter().flat_map(|f| f.t.iter()));
        let extent = (hi - lo).max(DVec3::splat(1e-9));
        // Zellenzahl nach der Zahl der Dreiecke (etwa ein Dreieck je Zelle auf der Oberfläche),
        // damit kleine Stücke kein großes Raster bekommen.
        let per_axis = (facets.len() as f64)
            .cbrt()
            .clamp(MIN_GRID_CELLS, GRID_CELLS);
        let cell = (extent.max_element() / per_axis).max(1e-9);
        let dims = [extent.x, extent.y, extent.z].map(|e| ((e / cell).ceil() as usize).max(1));
        let mut cells = vec![Vec::new(); dims[0] * dims[1] * dims[2]];
        let index = |p: DVec3| -> [usize; 3] {
            let c = ((p - lo) / cell).floor();
            let mut out = [0usize; 3];
            for (k, x) in [c.x, c.y, c.z].into_iter().enumerate() {
                out[k] = x.clamp(0.0, (dims[k] - 1) as f64) as usize;
            }
            out
        };
        for (k, f) in facets.iter().enumerate() {
            let (a, b) = bounds_of(f.t.iter());
            let (ia, ib) = (index(a), index(b));
            for x in ia[0]..=ib[0] {
                for y in ia[1]..=ib[1] {
                    for z in ia[2]..=ib[2] {
                        cells[(x * dims[1] + y) * dims[2] + z].push(k as u32);
                    }
                }
            }
        }
        let n = facets.len();
        let mut grid = FacetGrid {
            lo,
            cell,
            dims,
            cells,
            facets,
            inside: Vec::new(),
            stamp: std::cell::RefCell::new(vec![0; n]),
            rays: std::cell::Cell::new(0),
        };
        grid.classify();
        grid
    }

    fn idx(&self, c: [usize; 3]) -> usize {
        (c[0] * self.dims[1] + c[1]) * self.dims[2] + c[2]
    }

    /// Leere Zellen: je zusammenhängendem Gebiet einmal über die Umlaufzahl in der Mitte der
    /// ersten Zelle bestimmt (ersatzweise über das nächste Dreieck), dann über Nachbarn
    /// (6 Richtungen) weitergegeben.
    fn classify(&mut self) {
        let n = self.cells.len();
        let mut inside: Vec<Option<bool>> = vec![None; n];
        let [dx, dy, dz] = self.dims;
        for start in 0..n {
            if inside[start].is_some() || !self.cells[start].is_empty() {
                continue;
            }
            let c = [start / (dy * dz), (start / dz) % dy, start % dz];
            let centre = self.lo
                + (DVec3::new(c[0] as f64, c[1] as f64, c[2] as f64) + DVec3::splat(0.5))
                    * self.cell;
            let label = match self.winding(centre) {
                Some(w) => w > 0,
                None => self
                    .nearest(centre)
                    .is_some_and(|(_, p, k)| (centre - p).dot(self.facets[k].n) < 0.0),
            };
            inside[start] = Some(label);
            let mut stack = vec![c];
            while let Some([x, y, z]) = stack.pop() {
                let next = [
                    (x > 0).then(|| [x - 1, y, z]),
                    (x + 1 < dx).then(|| [x + 1, y, z]),
                    (y > 0).then(|| [x, y - 1, z]),
                    (y + 1 < dy).then(|| [x, y + 1, z]),
                    (z > 0).then(|| [x, y, z - 1]),
                    (z + 1 < dz).then(|| [x, y, z + 1]),
                ];
                for m in next.into_iter().flatten() {
                    let k = self.idx(m);
                    if inside[k].is_none() && self.cells[k].is_empty() {
                        inside[k] = Some(label);
                        stack.push(m);
                    }
                }
            }
        }
        self.inside = inside;
    }

    /// Zelle des Punkts (auch außerhalb des Rasters, dann jenseits der Grenzen).
    fn cell_of(&self, q: DVec3) -> [i64; 3] {
        let c = ((q - self.lo) / self.cell).floor();
        [c.x as i64, c.y as i64, c.z as i64]
    }

    fn in_grid(&self, c: [i64; 3]) -> bool {
        c.iter()
            .zip(self.dims)
            .all(|(x, d)| *x >= 0 && *x < d as i64)
    }

    /// Nächstes Dreieck zu `q`: Abstand, nächster Punkt, Index. Ringweise um die Zelle von `q`,
    /// bis kein weiterer Ring näher sein kann.
    fn nearest(&self, q: DVec3) -> Option<(f64, DVec3, usize)> {
        let c = ((q - self.lo) / self.cell).floor();
        let c = [c.x, c.y, c.z]
            .iter()
            .zip(self.dims)
            .map(|(x, d)| (*x).clamp(0.0, (d - 1) as f64) as i64)
            .collect::<Vec<_>>();
        let max_r = *self.dims.iter().max()? as i64;
        let mut best: Option<(f64, DVec3, usize)> = None;
        for r in 0..=max_r {
            for x in (c[0] - r)..=(c[0] + r) {
                for y in (c[1] - r)..=(c[1] + r) {
                    for z in (c[2] - r)..=(c[2] + r) {
                        let ring = (x - c[0]).abs().max((y - c[1]).abs()).max((z - c[2]).abs());
                        if ring != r || !self.in_grid([x, y, z]) {
                            continue;
                        }
                        let idx =
                            (x as usize * self.dims[1] + y as usize) * self.dims[2] + z as usize;
                        for &k in &self.cells[idx] {
                            let p = closest_on_triangle(q, &self.facets[k as usize].t);
                            let d = p.distance(q);
                            if best.is_none_or(|b| d < b.0) {
                                best = Some((d, p, k as usize));
                            }
                        }
                    }
                }
            }
            if best.is_some_and(|b| b.0 <= r as f64 * self.cell) {
                break;
            }
        }
        best
    }

    /// Dreiecke (Index, Abstand), die höchstens `r` von `q` entfernt liegen.
    fn within(&self, q: DVec3, r: f64) -> Vec<(usize, f64)> {
        // Nur die Zellen, die die Kugel mit Radius `r` um `q` trifft.
        let mut a = self.cell_of(q - DVec3::splat(r));
        let mut b = self.cell_of(q + DVec3::splat(r));
        for k in 0..3 {
            a[k] = a[k].max(0);
            b[k] = b[k].min(self.dims[k] as i64 - 1);
        }
        self.rays.set(self.rays.get().wrapping_add(1));
        let mark = self.rays.get();
        let mut stamp = self.stamp.borrow_mut();
        let mut out = Vec::new();
        for x in a[0]..=b[0] {
            for y in a[1]..=b[1] {
                for z in a[2]..=b[2] {
                    if !self.in_grid([x, y, z]) {
                        continue;
                    }
                    let idx = self.idx([x as usize, y as usize, z as usize]);
                    for &k in &self.cells[idx] {
                        let k = k as usize;
                        if stamp[k] == mark {
                            continue;
                        }
                        stamp[k] = mark;
                        let d = closest_on_triangle(q, &self.facets[k].t).distance(q);
                        if d <= r {
                            out.push((k, d));
                        }
                    }
                }
            }
        }
        out
    }

    /// Vorzeichenbehaftete Zahl der Dreiecke, die der Strahl `p + t·d` (t > 0) durchstößt
    /// (+1 aus dem Material heraus, −1 hinein): für ein geschlossenes Netz die Umlaufzahl von
    /// `p`. Der Strahl läuft Zelle für Zelle durch das Raster (Amanatides–Woo); jedes Dreieck
    /// zählt nur einmal.
    fn ray_count(&self, p: DVec3, d: DVec3) -> i64 {
        // Eintritt in das Raster, falls `p` außerhalb liegt.
        let hi = self.lo
            + DVec3::new(
                self.dims[0] as f64,
                self.dims[1] as f64,
                self.dims[2] as f64,
            ) * self.cell;
        let (mut t_in, mut t_out) = (0.0_f64, f64::INFINITY);
        for k in 0..3 {
            let (o, dk, l, h) = (p[k], d[k], self.lo[k], hi[k]);
            if dk.abs() < 1e-300 {
                if o < l || o > h {
                    return 0;
                }
                continue;
            }
            let (a, b) = ((l - o) / dk, (h - o) / dk);
            t_in = t_in.max(a.min(b));
            t_out = t_out.min(a.max(b));
        }
        if t_in > t_out {
            return 0;
        }
        let start = if t_in > 0.0 {
            p + d * (t_in + 1e-9 * self.cell)
        } else {
            p
        };
        let mut c = self.cell_of(start);
        for (x, d) in c.iter_mut().zip(self.dims) {
            *x = (*x).clamp(0, d as i64 - 1);
        }
        let step: [i64; 3] = [0, 1, 2].map(|k| if d[k] > 0.0 { 1 } else { -1 });
        let mut t_max = [f64::INFINITY; 3];
        let mut t_delta = [f64::INFINITY; 3];
        for k in 0..3 {
            if d[k].abs() > 1e-300 {
                let edge = self.lo[k] + (c[k] + i64::from(d[k] > 0.0)) as f64 * self.cell;
                t_max[k] = (edge - start[k]) / d[k];
                t_delta[k] = self.cell / d[k].abs();
            }
        }
        self.rays.set(self.rays.get().wrapping_add(1));
        let mark = self.rays.get();
        let mut stamp = self.stamp.borrow_mut();
        let mut count = 0;
        loop {
            let idx = self.idx([c[0] as usize, c[1] as usize, c[2] as usize]);
            for &k in &self.cells[idx] {
                let k = k as usize;
                if stamp[k] == mark {
                    continue;
                }
                stamp[k] = mark;
                let f = &self.facets[k];
                if ray_triangle(p, d, &f.t).is_some() {
                    count += if d.dot(f.n) > 0.0 { 1 } else { -1 };
                }
            }
            let k = (0..3)
                .min_by(|&a, &b| t_max[a].total_cmp(&t_max[b]))
                .expect("drei Achsen");
            c[k] += step[k];
            if !self.in_grid(c) {
                break;
            }
            t_max[k] += t_delta[k];
        }
        count
    }

    /// Umlaufzahl von `q` aus zwei Strahlen, bei Uneinigkeit (ein Strahl traf eine Kante oder
    /// Ecke des Netzes) entscheidet ein dritter; `None`, wenn sich keine zwei einig sind.
    fn winding(&self, q: DVec3) -> Option<i64> {
        let ray = |k: usize| self.ray_count(q, DVec3::from_array(RAY_DIRS[k]).normalize());
        let (a, b) = (ray(0), ray(1));
        if a == b {
            return Some(a);
        }
        let c = ray(2);
        (c == a || c == b).then_some(c)
    }

    /// Liegt `q` innen (Umlaufzahl > 0) und weiter als `depth` von den Dreiecken? `None`: näher
    /// als `depth` (Berühren, Netzfehler) oder unentscheidbar – zählt nicht.
    fn deep_inside(&self, q: DVec3, depth: f64) -> Option<bool> {
        let c = self.cell_of(q);
        if self.in_grid(c) {
            let k = self.idx([c[0] as usize, c[1] as usize, c[2] as usize]);
            if let Some(label) = self.inside.get(k).copied().flatten() {
                // Leere Zelle: innen oder außen steht fest; zählt nur, wenn kein Dreieck näher
                // als `depth` liegt (in den Zellen bis `depth` um sie herum).
                return self.within(q, depth).is_empty().then_some(label);
            }
        }
        let (d, _, _) = self.nearest(q)?;
        if d <= depth {
            return None;
        }
        self.winding(q).map(|w| w > 0)
    }
}

/// Dreiecke der Schale mit Normalen aus dem Material heraus (`volume`: ihr Volumen mit
/// Vorzeichen; `flip`: Normalen umkehren, für das Material um einen Hohlraum) und ihren Flächen.
fn facets(triangles: &[[DVec3; 3]], faces: &[usize], volume: f64, flip: bool) -> Vec<Facet> {
    let s = volume.signum() * if flip { -1.0 } else { 1.0 };
    triangles
        .iter()
        .zip(faces)
        .filter_map(|(t, face)| {
            let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
            (n != DVec3::ZERO).then(|| Facet {
                t: *t,
                n: n * s,
                face: *face,
            })
        })
        .collect()
}

/// Höchstens so viele Probepunkte (Schwerpunkte von Dreiecken) je Schale.
const SHELL_PROBES: usize = 512;

/// Schwerpunkte gleichmäßig verteilter Dreiecke der Schale (höchstens [`SHELL_PROBES`]), ein
/// kleines Stück ins Innere der Schale gerückt (`volume`: ihr Volumen mit Vorzeichen, gibt an,
/// wohin die Dreiecke zeigen).
fn probes(triangles: &[[DVec3; 3]], volume: f64) -> Vec<DVec3> {
    let (lo, hi) = bounds_of(triangles.iter().flatten());
    let inset = 1e-6 * (1.0 + lo.distance(hi));
    let step = triangles.len().div_ceil(SHELL_PROBES).max(1);
    triangles
        .iter()
        .step_by(step)
        .filter_map(|t| {
            let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
            (n != DVec3::ZERO).then(|| (t[0] + t[1] + t[2]) / 3.0 - n * (inset * volume.signum()))
        })
        .collect()
}

/// Sind zwei Trägerflächen dieselbe Fläche (Lage bis `tol`, Richtungen bis [`SAME_DIR`])?
/// Freiformflächen nur, wenn sie dieselben Daten teilen.
fn same_surface(a: &Surface, b: &Surface, tol: f64) -> bool {
    let parallel = |u: DVec3, v: DVec3| u.dot(v).abs() >= SAME_DIR;
    let close = |x: f64, y: f64| (x - y).abs() <= tol;
    // Abstand des Punkts `p` von der Geraden durch `o` in Richtung `z`.
    let off_axis = |p: DVec3, o: DVec3, z: DVec3| {
        let d = p - o;
        (d - z * d.dot(z)).length()
    };
    match (a, b) {
        (Surface::Plane { frame: f }, Surface::Plane { frame: g }) => {
            parallel(f.z, g.z) && (g.origin - f.origin).dot(f.z).abs() <= tol
        }
        (
            Surface::Cylinder {
                frame: f,
                radius: r,
            },
            Surface::Cylinder {
                frame: g,
                radius: q,
            },
        ) => parallel(f.z, g.z) && off_axis(g.origin, f.origin, f.z) <= tol && close(*r, *q),
        (
            Surface::Cone {
                frame: f,
                radius: r,
                slope: s,
            },
            Surface::Cone {
                frame: g,
                radius: q,
                slope: t,
            },
        ) => {
            // Gleiche Spitze und gleicher halber Öffnungswinkel (die Achse darf umgekehrt sein).
            let apex = |fr: &Frame, radius: f64, slope: f64| fr.origin - fr.z * (radius / slope);
            s.abs() > 1e-12
                && t.abs() > 1e-12
                && parallel(f.z, g.z)
                && apex(f, *r, *s).distance(apex(g, *q, *t)) <= tol
                && (s.abs() - t.abs()).abs() <= 1e-9 * (1.0 + s.abs())
        }
        (
            Surface::Sphere {
                frame: f,
                radius: r,
            },
            Surface::Sphere {
                frame: g,
                radius: q,
            },
        ) => f.origin.distance(g.origin) <= tol && close(*r, *q),
        (
            Surface::Torus {
                frame: f,
                major: r,
                minor: s,
            },
            Surface::Torus {
                frame: g,
                major: q,
                minor: t,
            },
        ) => {
            parallel(f.z, g.z)
                && f.origin.distance(g.origin) <= tol
                && close(*r, *q)
                && close(*s, *t)
        }
        (Surface::Nurbs(d), Surface::Nurbs(e)) => std::sync::Arc::ptr_eq(d, e),
        _ => false,
    }
}

/// Skalarprodukt zweier Einheitsrichtungen, ab dem sie als gleich gelten ([`same_surface`]).
const SAME_DIR: f64 = 1.0 - 1e-9;

/// Höchstens so viele gleichmäßig gewählte Dreiecke je Stück prüft [`check_face_sides`]
/// (dazu je Fläche ihr größtes Dreieck).
const SIDE_PROBES: usize = 64;

/// Normalen, deren Skalarprodukt darüber liegt, gelten als gleich gerichtet (etwa 30°):
/// Nachbardreiecke derselben gekrümmten Fläche und Flächen, die auf derselben Trägerfläche
/// übereinanderliegen.
const PARALLEL: f64 = 0.87;

/// Prüft jedes Stück (äußere Schale mit ihren Hohlräumen) unabhängig vom Aufbau: Dicht hinter
/// jedem Dreieck (gegen die Außennormale) muss die Umlaufzahl 1 sein, dicht davor 0. Doppelt
/// belegte oder gegengleich übereinanderliegende Flächen, Flächen auf der falschen Seite und
/// Ränder, die um die falsche Hälfte laufen, fallen so auf. Der Abstand der Probepunkte ist
/// `depth` (zwei Sehnen des Netzes plus Genauigkeit), kleiner nur, wo ein anders gerichtetes
/// Dreieck einer anderen Trägerfläche näher liegt (dünne Stege, Fasen, dünne Wände: dann die
/// Hälfte dieses Abstands). Ist eine Wand dünner als zwei Sehnen (`chord`; die Netze könnten
/// sich schneiden), entscheidet das Dreieck nicht; Dreiecke auf derselben Trägerfläche (bis
/// `tol` genau) sind keine Wand, sondern eine doppelte Belegung, und zählen mit vollem Abstand.
/// Geprüft werden je Fläche ihr größtes Dreieck und je Stück bis [`SIDE_PROBES`] weitere.
// Der negierte Vergleich lehnt absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_face_sides(
    grid: &FacetGrid,
    depth: f64,
    chord: f64,
    tol: f64,
    solid: &Solid,
) -> Result<(), Skip> {
    let area = |t: &[DVec3; 3]| (t[1] - t[0]).cross(t[2] - t[0]).length();
    let coincident = |a: usize, b: usize| {
        same_surface(
            &solid.faces[Id::from_index(a)].surface,
            &solid.faces[Id::from_index(b)].surface,
            tol.max(Tolerance::default().linear),
        )
    };
    let mut largest: HashMap<usize, (f64, usize)> = HashMap::new();
    for (k, f) in grid.facets.iter().enumerate() {
        let a = area(&f.t);
        let e = largest.entry(f.face).or_insert((a, k));
        if a > e.0 {
            *e = (a, k);
        }
    }
    let step = grid.facets.len().div_ceil(SIDE_PROBES).max(1);
    let mut chosen: Vec<usize> = largest.values().map(|(_, k)| *k).collect();
    chosen.extend((0..grid.facets.len()).step_by(step));
    chosen.sort_unstable();
    chosen.dedup();
    for k in chosen {
        let f = &grid.facets[k];
        let c = (f.t[0] + f.t[1] + f.t[2]) / 3.0;
        let mut d_other = f64::INFINITY;
        let mut d_wall = f64::INFINITY;
        for (j, d) in grid.within(c, depth) {
            if j == k {
                continue;
            }
            let other = &grid.facets[j];
            let dot = other.n.dot(f.n);
            if dot > PARALLEL {
                continue;
            }
            if dot < -PARALLEL {
                // Gegengleich auf derselben Trägerfläche: doppelte Belegung, die Probe zählt.
                if other.face != f.face && coincident(other.face, f.face) {
                    continue;
                }
                d_wall = d_wall.min(d);
            } else {
                d_other = d_other.min(d);
            }
        }
        // Dünne Wand: Die Probe rückt auf die halbe Wandstärke heran. Ist die Wand dünner als
        // zwei Sehnen, könnten sich die Netze schneiden; dann entscheidet das Dreieck nicht.
        if d_wall <= 2.0 * chord {
            continue;
        }
        let h = depth.min(0.5 * d_other).min(0.5 * d_wall);
        if !(h > 0.0) {
            continue;
        }
        let (Some(inside), Some(outside)) = (grid.winding(c - f.n * h), grid.winding(c + f.n * h))
        else {
            continue;
        };
        if inside != 1 || outside != 0 {
            return Err(Skip(format!(
                "Die Fläche {} liegt nicht richtig im Körper: Innen und außen passen an ihr \
                 nicht zusammen.",
                face_name(solid, f.face)
            )));
        }
    }
    Ok(())
}

/// Lage der Schalen zueinander, über die groben Netze: Ein Hohlraum liegt in seiner äußeren
/// Schale, Stücke überschneiden sich nicht. Ein Stück ist seine äußere Schale ohne seine
/// Hohlräume (`pieces`: je äußerer Schale ihr Raster; ein Stück im Hohlraum eines anderen ist
/// erlaubt). Ein Probepunkt (Schwerpunkt eines Netzdreiecks) zählt erst, wenn er weiter als
/// `depth` (zwei Sehnen des Netzes plus Genauigkeit der Datei) von den Dreiecken des anderen
/// Stücks entfernt liegt; ob innen oder außen, sagt die Umlaufzahl (Strahlen durch das Raster).
/// Berührende Stücke (Stift in Bohrung) und Netzfehler bleiben erlaubt. Nur Paare, deren
/// Hüllquader sich treffen, werden verglichen.
fn check_shells(
    triangles: &ShellTriangles,
    faces: &ShellTriangleFaces,
    volumes: &[f64],
    owners: &[Option<usize>],
    pieces: &[Option<FacetGrid>],
    depth: f64,
) -> Result<(), Skip> {
    let boxes: Vec<(DVec3, DVec3)> = triangles
        .iter()
        .map(|t| bounds_of(t.iter().flatten()))
        .collect();
    let probes: Vec<Vec<DVec3>> = triangles
        .iter()
        .zip(volumes)
        .map(|(t, v)| probes(t, *v))
        .collect();
    let lumps: Vec<usize> = (0..owners.len()).filter(|k| owners[*k].is_none()).collect();
    let region = |k: usize| FacetGrid::new(facets(&triangles[k], &faces[k], volumes[k], false));
    let outside = || Skip("Ein Hohlraum liegt nicht ganz im Körper.".into());
    // Hohlräume: ganz in ihrer äußeren Schale, die äußere Schale nicht im Hohlraum.
    for (v, owner) in owners.iter().enumerate() {
        let Some(o) = *owner else {
            continue;
        };
        let outer = region(o);
        if probes[v]
            .iter()
            .any(|q| outer.deep_inside(*q, depth) == Some(false))
        {
            return Err(outside());
        }
        let cavity = region(v);
        if probes[o]
            .iter()
            .any(|q| in_box(*q, boxes[v], depth) && cavity.deep_inside(*q, depth) == Some(true))
        {
            return Err(outside());
        }
    }
    // Stücke: kein Punkt eines Stücks tief im Material eines anderen.
    for &b in &lumps {
        let Some(grid) = &pieces[b] else {
            continue;
        };
        for &a in &lumps {
            if a == b || !boxes_meet(boxes[a], boxes[b], depth) {
                continue;
            }
            if probes[a]
                .iter()
                .any(|q| in_box(*q, boxes[b], depth) && grid.deep_inside(*q, depth) == Some(true))
            {
                return Err(Skip("Zwei Stücke des Körpers überschneiden sich.".into()));
            }
        }
    }
    Ok(())
}

/// Außenrichtung je Schale über ihr Volumen: äußere Schalen positiv, Hohlräume (Eigentümer in
/// `owners`, Reihenfolge von `solid.shells`) negativ; sonst wird die Schale umgedreht – außer sie
/// enthält eine Fläche, deren Seite nur geraten ist (`guessed`, je Fläche): Dann ist der Körper
/// zweideutig. Danach je Stück (äußere Schale mit Hohlräumen) die Probe, ob innen und außen an
/// jeder Fläche stimmen ([`check_face_sides`]), und die Lage der Schalen zueinander
/// ([`check_shells`]), beides mit der Tiefe aus zwei Sehnen des Netzes und der Genauigkeit `tol`.
fn orient_shells(
    solid: &mut Solid,
    owners: &[Option<usize>],
    guessed: &[bool],
    tol: f64,
) -> Result<(), Skip> {
    let ShellMesh {
        volumes,
        triangles,
        faces,
        chord,
    } = shell_volumes(solid)?;
    for (k, &v) in volumes.iter().enumerate() {
        let void = owners.get(k).is_some_and(Option::is_some);
        if (v > 0.0) == void {
            let ids = solid.shells[k].faces.clone();
            if let Some(f) = ids.iter().find(|f| guessed.get(f.index()) == Some(&true)) {
                return Err(Skip(format!(
                    "Die Seite der Fläche {} ist nicht eindeutig: Sie hat weder Naht noch Pol, und \
                     der Körper müsste umgedreht werden.",
                    face_name(solid, f.index())
                )));
            }
            turn_inside_out(solid, &ids);
        }
    }
    let depth = 2.0 * chord + tol;
    // Je Stück ein Raster über äußere Schale und Hohlräume, Normalen aus dem Material heraus.
    let pieces: Vec<Option<FacetGrid>> = (0..owners.len())
        .map(|b| {
            owners.get(b).copied().flatten().is_none().then(|| {
                let mut f = facets(&triangles[b], &faces[b], volumes[b], false);
                for v in (0..owners.len()).filter(|v| owners[*v] == Some(b)) {
                    f.extend(facets(&triangles[v], &faces[v], volumes[v], true));
                }
                FacetGrid::new(f)
            })
        })
        .collect();
    // Erst die Lage der Schalen (die genauere Meldung), dann die allgemeine Probe.
    check_shells(&triangles, &faces, &volumes, owners, &pieces, depth)?;
    for grid in pieces.iter().flatten() {
        check_face_sides(grid, depth, chord, tol, solid)?;
    }
    Ok(())
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

/// Baut aus den Volumen-Einträgen `items` (`MANIFOLD_SOLID_BREP`, `BREP_WITH_VOIDS`; mehrere
/// ergeben mehrere Schalen eines Körpers) einen geprüften Körper. Ablauf: Rohform lesen, Ecken
/// zusammenziehen und Kanten paaren ([`crate::read::weld`]), Ecken auf Pole ziehen, entartete
/// Kanten entfernen, Lücken zwischen offenen Kanten melden, Ecken auf den Schnitt ihrer Flächen
/// nachziehen (P16), fehlende Nähte einfügen ([`crate::read::seam`]), Pole prüfen, je Fläche
/// äußerer Rand und Parameterbereich, ungenutzte Kanten und Ecken entfernen, jede Kante genau
/// zweimal benutzt; dann `validate`, Schleifen je Fläche, Außenrichtung je Schale über ihr Volumen,
/// Lage der Kanten und Ecken, `validate`. Mehr als `file.limits().max_faces` Flächen, ungültige
/// Topologie, verdrehte Flächen, Kanten neben ihren Flächen oder Ecken neben ihren Kanten ergeben
/// ein [`Skip`] mit Grund. Grobe Dateien (`Units::is_coarse`) bekommen die Warnung
/// [`weld::coarse_warning`]. `progress` zählt je Fläche eins weiter; Abbruch ergibt
/// `Skip("Abgebrochen")`.
pub fn build_solid(
    file: &StepFile,
    units: Units,
    items: &[u64],
    opts: &BuildOptions,
    progress: &Progress,
) -> Result<BuiltSolid, Skip> {
    let tol = opts.weld_tol_mm;
    let mut raw = read_raw(file, units, items, tol, progress)?;
    weld::weld_body(&mut raw, tol);
    seam::snap_poles(&mut raw, tol);
    seam::split_at_poles(&mut raw, tol);
    seam::drop_degenerate(&mut raw, tol)?;
    weld::check_gaps(&raw, tol)?;
    weld::tighten_vertices(&mut raw, tol);
    seam::insert_seams(&mut raw, tol)?;
    seam::check_poles(&raw, tol)?;
    for i in 0..raw.faces.len() {
        finish_face(&mut raw, i, tol)?;
    }
    raw.compact();
    weld::check_open_edges(&raw, tol)?;
    let owners: Vec<Option<usize>> = raw.shells.iter().map(|s| s.owner).collect();
    let guessed: Vec<bool> = raw.faces.iter().map(|f| f.side_guessed).collect();
    let mut solid = raw.to_solid();
    // Erst prüfen, dann vernetzen: Das Netz für das Volumen setzt gültige Topologie voraus. Die
    // Prüfung am Ende (nach dem Umdrehen) verlangt der Ablauf des Plans; sie ist billig.
    validate(&solid).map_err(|e| invalid(&solid, &e))?;
    check_face_loops(&solid)?;
    orient_shells(&mut solid, &owners, &guessed, tol)?;
    check_edges(&solid, tol)?;
    validate(&solid).map_err(|e| invalid(&solid, &e))?;
    let mut warnings = Vec::new();
    if units.is_coarse() {
        warnings.push(weld::coarse_warning(tol));
    }
    Ok(BuiltSolid {
        faces: solid.faces.len(),
        solid,
        warnings,
    })
}
