//! Topologie (Spec §3): Körper, Hüllen, Flächen, Schleifen, Kanten und Eckpunkte. Jede Kante
//! und jeder Eckpunkt wird je Körper einmal geschrieben und von allen Flächen gemeinsam benutzt
//! (Nähte: ein `EDGE_CURVE`, zwei `ORIENTED_EDGE` mit entgegengesetzter Richtung).

use crate::entity::{Entities, logical, refs};
use crate::error::StepError;
use crate::geometry::{self, reject};
use planora_i18n::tr;
use planora_kernel::{Edge, Face, Id, Loop, Solid, Vertex};
use planora_math::DVec3;
use planora_mesh::{TessellationOptions, tessellate};

/// Ergebnis eines geschriebenen Körpers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WrittenSolid {
    /// Darstellungselemente für `ADVANCED_BREP_SHAPE_REPRESENTATION`.
    pub items: Vec<u64>,
    pub faces: usize,
    pub edges: usize,
}

/// Verweisen alle Schleifen, Kanten und Schalen auf vorhandene Teile? Sonst würde die Prüfung
/// des Kernels beim Zugriff abbrechen.
pub(crate) fn ids_in_range(solid: &Solid) -> bool {
    let (nv, ne, nf) = (solid.vertices.len(), solid.edges.len(), solid.faces.len());
    let edges_ok = solid
        .edges
        .iter()
        .all(|(_, e)| e.start.index() < nv && e.end.index() < nv);
    let loops_ok = solid.faces.iter().all(|(_, f)| {
        std::iter::once(&f.outer)
            .chain(f.inner.iter())
            .all(|lp| lp.half_edges.iter().all(|he| he.edge.index() < ne))
    });
    let shells_ok = solid
        .shells
        .iter()
        .all(|s| s.faces.iter().all(|f| f.index() < nf));
    edges_ok && loops_ok && shells_ok
}

/// Flächen der Schalen (ohne Schalenliste: alle Flächen als eine Schale, wie im Kernel).
pub(crate) fn shell_faces(solid: &Solid) -> Vec<Vec<Id<Face>>> {
    if solid.shells.is_empty() {
        vec![solid.faces.iter().map(|(id, _)| id).collect()]
    } else {
        solid.shells.iter().map(|s| s.faces.clone()).collect()
    }
}

/// Schreibt Teile eines Körpers; merkt sich schon geschriebene Kanten und Eckpunkte.
struct TopoWriter<'s> {
    solid: &'s Solid,
    vertices: Vec<Option<u64>>,
    edges: Vec<Option<u64>>,
    edge_count: usize,
}

impl<'s> TopoWriter<'s> {
    fn new(solid: &'s Solid) -> TopoWriter<'s> {
        TopoWriter {
            solid,
            vertices: vec![None; solid.vertices.len()],
            edges: vec![None; solid.edges.len()],
            edge_count: 0,
        }
    }

    fn vertex(&mut self, e: &mut Entities, id: Id<Vertex>) -> Result<u64, StepError> {
        if let Some(n) = self.vertices[id.index()] {
            return Ok(n);
        }
        let p = geometry::point(e, self.solid.vertices[id].point)?;
        let n = e.push(&format!("VERTEX_POINT('',#{p})"))?;
        self.vertices[id.index()] = Some(n);
        Ok(n)
    }

    /// `EDGE_CURVE` vom Anfangs- zum Endpunkt; läuft die Kante gegen den Kurvenparameter
    /// (`t1 < t0`), ist `same_sense` falsch.
    fn edge(&mut self, e: &mut Entities, id: Id<Edge>) -> Result<u64, StepError> {
        if let Some(n) = self.edges[id.index()] {
            return Ok(n);
        }
        let edge = &self.solid.edges[id];
        let start = self.vertex(e, edge.start)?;
        let end = self.vertex(e, edge.end)?;
        let curve = geometry::curve(e, &edge.curve)?;
        let sense = logical(edge.t1 >= edge.t0);
        let n = e.push(&format!("EDGE_CURVE('',#{start},#{end},#{curve},{sense})"))?;
        self.edges[id.index()] = Some(n);
        self.edge_count += 1;
        Ok(n)
    }

    fn edge_loop(&mut self, e: &mut Entities, lp: &Loop) -> Result<u64, StepError> {
        let mut oriented = Vec::with_capacity(lp.half_edges.len());
        for he in &lp.half_edges {
            let ec = self.edge(e, he.edge)?;
            let dir = logical(he.forward);
            oriented.push(e.push(&format!("ORIENTED_EDGE('',*,*,#{ec},{dir})"))?);
        }
        e.push(&format!("EDGE_LOOP('',{})", refs(&oriented)))
    }

    /// `ADVANCED_FACE`; die äußere Schleife wird `FACE_OUTER_BOUND`, Löcher `FACE_BOUND`. Mit
    /// `reversed` wird die Fläche umgekehrt geschrieben (`same_sense` und die Richtung der
    /// Ränder gekippt), für die Schalen von Hohlräumen.
    fn face(&mut self, e: &mut Entities, id: Id<Face>, reversed: bool) -> Result<u64, StepError> {
        let f = &self.solid.faces[id];
        let surface = geometry::surface(e, &f.surface, f.patch)?;
        let along = logical(!reversed);
        let mut bounds = Vec::with_capacity(1 + f.inner.len());
        let outer = self.edge_loop(e, &f.outer)?;
        bounds.push(e.push(&format!("FACE_OUTER_BOUND('',#{outer},{along})"))?);
        for lp in &f.inner {
            let inner = self.edge_loop(e, lp)?;
            bounds.push(e.push(&format!("FACE_BOUND('',#{inner},{along})"))?);
        }
        let sense = logical(f.same_sense != reversed);
        e.push(&format!(
            "ADVANCED_FACE('',{},#{surface},{sense})",
            refs(&bounds)
        ))
    }

    fn closed_shell(
        &mut self,
        e: &mut Entities,
        faces: &[Id<Face>],
        reversed: bool,
    ) -> Result<u64, StepError> {
        let mut ids = Vec::with_capacity(faces.len());
        for &f in faces {
            ids.push(self.face(e, f, reversed)?);
        }
        e.push(&format!("CLOSED_SHELL('',{})", refs(&ids)))
    }
}

/// Ein zusammenhängendes Stück: äußere Schale und die Hohlräume darin (Indizes in
/// [`shell_faces`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lump {
    pub outer: usize,
    pub voids: Vec<usize>,
}

/// Sehnentoleranz (mm) des Netzes, mit dem die Schalen eines Körpers mit mehreren Schalen nach
/// dem Vorzeichen ihres Volumens unterschieden werden (grob genügt: es zählen nur das Vorzeichen
/// und ob ein Punkt innen liegt).
pub const CLASSIFY_CHORD: f64 = 0.05;
/// Ab diesem Wert der Umlaufzahl liegt ein Punkt innerhalb einer Schale (innen 1, außen 0).
const INSIDE_WINDING: f64 = 0.5;

/// Raumwinkel des Dreiecks `a, b, c`, von `p` aus gesehen (Formel von Van Oosterom und
/// Strackee).
fn solid_angle(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> f64 {
    let (a, b, c) = (a - p, b - p, c - p);
    let (la, lb, lc) = (a.length(), b.length(), c.length());
    let det = a.dot(b.cross(c));
    let div = la * lb * lc + a.dot(b) * lc + a.dot(c) * lb + b.dot(c) * la;
    2.0 * det.atan2(div)
}

/// Teilt die Schalen eines Körpers in Stücke (Spec §3, Volumenvorzeichen): Schalen mit positivem
/// Volumen sind äußere Schalen, mit negativem Hohlräume. Jeder Hohlraum gehört zur kleinsten
/// äußeren Schale, in der sein erster Eckpunkt liegt (Umlaufzahl des Netzes). Ein Körper mit
/// einer Schale braucht kein Netz.
pub(crate) fn lumps(solid: &Solid) -> Result<Vec<Lump>, StepError> {
    let shells = shell_faces(solid);
    if shells.len() == 1 {
        return Ok(vec![Lump {
            outer: 0,
            voids: Vec::new(),
        }]);
    }
    let mesh = tessellate(
        solid,
        &TessellationOptions {
            chord_tolerance: CLASSIFY_CHORD,
        },
    )
    .map_err(|e| reject(tr!("step-w-shells-ambiguous", e = e.to_string())))?;
    let mut shell_of = vec![usize::MAX; solid.faces.len()];
    for (k, faces) in shells.iter().enumerate() {
        for f in faces {
            shell_of[f.index()] = k;
        }
    }
    let (lo, hi) = mesh
        .positions
        .iter()
        .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        });
    let o = 0.5 * (lo + hi);
    let mut volume = vec![0.0; shells.len()];
    let mut triangles: Vec<Vec<[DVec3; 3]>> = vec![Vec::new(); shells.len()];
    for r in &mesh.face_ranges {
        let k = shell_of[r.face.index()];
        let start = r.first_index as usize;
        let end = start + r.index_count as usize;
        for t in mesh.indices[start..end].as_chunks::<3>().0 {
            let p = t.map(|i| mesh.positions[i as usize]);
            volume[k] += (p[0] - o).dot((p[1] - o).cross(p[2] - o)) / 6.0;
            triangles[k].push(p);
        }
    }
    if volume.iter().any(|v| !v.is_finite() || *v == 0.0) {
        return Err(reject(tr!("step-w-shell-no-volume")));
    }
    let mut out: Vec<Lump> = (0..shells.len())
        .filter(|&k| volume[k] > 0.0)
        .map(|outer| Lump {
            outer,
            voids: Vec::new(),
        })
        .collect();
    for void in (0..shells.len()).filter(|&k| volume[k] < 0.0) {
        let he = solid.faces[shells[void][0]].outer.half_edges[0];
        let p = solid.vertices[solid.half_edge_start(he)].point;
        let inside = |k: usize| {
            let w: f64 = triangles[k]
                .iter()
                .map(|t| solid_angle(p, t[0], t[1], t[2]))
                .sum();
            w / (4.0 * std::f64::consts::PI) > INSIDE_WINDING
        };
        let owner = (0..out.len())
            .filter(|&i| inside(out[i].outer))
            .min_by(|&a, &b| volume[out[a].outer].total_cmp(&volume[out[b].outer]))
            .ok_or_else(|| reject(tr!("step-w-cavity-nowhere")))?;
        out[owner].voids.push(void);
    }
    Ok(out)
}

/// Schreibt den Körper (Spec §3): je Stück ein `MANIFOLD_SOLID_BREP`, mit Hohlräumen ein
/// `BREP_WITH_VOIDS` (Hohlräume als `ORIENTED_CLOSED_SHELL` mit `.F.`; ihre Schale wird dafür
/// umgekehrt geschrieben). `name` ist schon eine STEP-Zeichenkette.
pub(crate) fn write_solid(
    e: &mut Entities,
    name: &str,
    solid: &Solid,
) -> Result<WrittenSolid, StepError> {
    let shells = shell_faces(solid);
    // ISO 10303-42: eine Hülle braucht mindestens eine Fläche.
    if shells.is_empty() || shells.iter().any(Vec::is_empty) {
        return Err(reject(tr!("step-w-body-no-faces")));
    }
    let mut w = TopoWriter::new(solid);
    let mut items = Vec::new();
    for lump in lumps(solid)? {
        let outer = w.closed_shell(e, &shells[lump.outer], false)?;
        if lump.voids.is_empty() {
            items.push(e.push(&format!("MANIFOLD_SOLID_BREP({name},#{outer})"))?);
            continue;
        }
        let mut voids = Vec::with_capacity(lump.voids.len());
        for &v in &lump.voids {
            let shell = w.closed_shell(e, &shells[v], true)?;
            voids.push(e.push(&format!("ORIENTED_CLOSED_SHELL('',*,#{shell},.F.)"))?);
        }
        items.push(e.push(&format!(
            "BREP_WITH_VOIDS({name},#{outer},{})",
            refs(&voids)
        ))?);
    }
    Ok(WrittenSolid {
        items,
        faces: shells.iter().map(Vec::len).sum(),
        edges: w.edge_count,
    })
}
