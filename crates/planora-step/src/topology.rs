//! Topologie (Spec §3): Körper, Hüllen, Flächen, Schleifen, Kanten und Eckpunkte. Jede Kante
//! und jeder Eckpunkt wird je Körper einmal geschrieben und von allen Flächen gemeinsam benutzt
//! (Nähte: ein `EDGE_CURVE`, zwei `ORIENTED_EDGE` mit entgegengesetzter Richtung).

use crate::entity::{Entities, logical, refs};
use crate::error::StepError;
use crate::geometry::{self, reject};
use planora_kernel::{Edge, Face, Id, Loop, Solid, Vertex};

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

    /// `ADVANCED_FACE`; die äußere Schleife wird `FACE_OUTER_BOUND`, Löcher `FACE_BOUND`.
    fn face(&mut self, e: &mut Entities, id: Id<Face>) -> Result<u64, StepError> {
        let f = &self.solid.faces[id];
        let surface = geometry::surface(e, &f.surface)?;
        let mut bounds = Vec::with_capacity(1 + f.inner.len());
        let outer = self.edge_loop(e, &f.outer)?;
        bounds.push(e.push(&format!("FACE_OUTER_BOUND('',#{outer},.T.)"))?);
        for lp in &f.inner {
            let inner = self.edge_loop(e, lp)?;
            bounds.push(e.push(&format!("FACE_BOUND('',#{inner},.T.)"))?);
        }
        let sense = logical(f.same_sense);
        e.push(&format!(
            "ADVANCED_FACE('',{},#{surface},{sense})",
            refs(&bounds)
        ))
    }

    fn closed_shell(&mut self, e: &mut Entities, faces: &[Id<Face>]) -> Result<u64, StepError> {
        let mut ids = Vec::with_capacity(faces.len());
        for &f in faces {
            ids.push(self.face(e, f)?);
        }
        e.push(&format!("CLOSED_SHELL('',{})", refs(&ids)))
    }
}

/// Schreibt den Körper als `MANIFOLD_SOLID_BREP` mit dem Namen `name` (schon als
/// STEP-Zeichenkette).
pub(crate) fn write_solid(
    e: &mut Entities,
    name: &str,
    solid: &Solid,
) -> Result<WrittenSolid, StepError> {
    let shells = shell_faces(solid);
    if shells.len() != 1 {
        return Err(reject(
            "Körper mit mehreren Schalen werden noch nicht ausgegeben",
        ));
    }
    let mut w = TopoWriter::new(solid);
    let shell = w.closed_shell(e, &shells[0])?;
    let brep = e.push(&format!("MANIFOLD_SOLID_BREP({name},#{shell})"))?;
    Ok(WrittenSolid {
        items: vec![brep],
        faces: shells[0].len(),
        edges: w.edge_count,
    })
}
