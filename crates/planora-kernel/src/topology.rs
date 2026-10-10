use crate::geometry::{Curve, Surface, UvRect};
use crate::ids::{Arena, Id};
use planora_math::DVec3;

#[derive(Clone, Debug)]
pub struct Vertex {
    pub point: DVec3,
}

/// Kante: `curve` über den Parameterbereich `t0 → t1`, von `start` nach `end`.
#[derive(Clone, Debug)]
pub struct Edge {
    pub curve: Curve,
    pub t0: f64,
    pub t1: f64,
    pub start: Id<Vertex>,
    pub end: Id<Vertex>,
}

/// Durchlaufrichtung einer Kante innerhalb einer Schleife.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HalfEdge {
    pub edge: Id<Edge>,
    pub forward: bool,
}

/// Geschlossene Folge von Halbkanten. Eine äußere Schleife läuft gegen den Uhrzeigersinn,
/// gesehen von außen auf die Fläche (Rechte-Hand-Regel um die nach außen zeigende Normale).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Loop {
    pub half_edges: Vec<HalfEdge>,
}

impl Loop {
    pub fn reversed(&self) -> Loop {
        Loop {
            half_edges: self
                .half_edges
                .iter()
                .rev()
                .map(|he| HalfEdge {
                    edge: he.edge,
                    forward: !he.forward,
                })
                .collect(),
        }
    }
}

/// Fläche. Die nach außen zeigende Normale ist die natürliche Flächennormale,
/// wenn `same_sense` wahr ist, sonst deren Gegenteil.
///
/// Ebene Flächen haben `patch == None` und werden über ihre Schleifen begrenzt.
/// Gekrümmte Flächen müssen einen `patch` (Parameterbereich) haben.
#[derive(Clone, Debug)]
pub struct Face {
    pub surface: Surface,
    pub same_sense: bool,
    pub outer: Loop,
    pub inner: Vec<Loop>,
    pub patch: Option<UvRect>,
    /// Beschreibende Marke (Herkunft und Rolle). Fließt weder in die Geometrie noch in den
    /// Validator ein.
    pub tag: Option<FaceTag>,
}

/// Beschreibende Marke einer Fläche (siehe Spec Feature-Baum §4). `origin == 0`: noch keinem
/// Feature zugewiesen.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FaceTag {
    pub origin: u64,
    pub role: String,
}

#[derive(Clone, Debug, Default)]
pub struct Shell {
    pub faces: Vec<Id<Face>>,
}

#[derive(Clone, Debug, Default)]
pub struct Solid {
    pub vertices: Arena<Vertex>,
    pub edges: Arena<Edge>,
    pub faces: Arena<Face>,
    pub shells: Vec<Shell>,
}

impl Solid {
    /// Weist allen noch nicht zugewiesenen Marken (`origin == 0`) die Herkunft `origin` zu.
    /// Flächen ohne Marke und bereits zugewiesene Marken bleiben unverändert.
    pub fn assign_origin(&mut self, origin: u64) {
        let ids: Vec<Id<Face>> = self.faces.iter().map(|(id, _)| id).collect();
        for id in ids {
            if let Some(tag) = self.faces[id].tag.as_mut()
                && tag.origin == 0
            {
                tag.origin = origin;
            }
        }
    }

    /// Alle Flächen mit genau dieser Marke, aufsteigend nach Id.
    pub fn faces_with_tag(&self, tag: &FaceTag) -> Vec<Id<Face>> {
        self.faces
            .iter()
            .filter(|(_, f)| f.tag.as_ref() == Some(tag))
            .map(|(id, _)| id)
            .collect()
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_vertex(&mut self, point: DVec3) -> Id<Vertex> {
        self.vertices.push(Vertex { point })
    }

    pub fn add_edge(
        &mut self,
        curve: Curve,
        t0: f64,
        t1: f64,
        start: Id<Vertex>,
        end: Id<Vertex>,
    ) -> Id<Edge> {
        self.edges.push(Edge {
            curve,
            t0,
            t1,
            start,
            end,
        })
    }

    pub fn add_face(&mut self, face: Face) -> Id<Face> {
        self.faces.push(face)
    }

    /// Fasst alle Flächen in einer einzigen Schale zusammen.
    pub fn seal(&mut self) {
        self.shells = vec![Shell {
            faces: self.faces.iter().map(|(id, _)| id).collect(),
        }];
    }

    pub fn half_edge_start(&self, he: HalfEdge) -> Id<Vertex> {
        let e = &self.edges[he.edge];
        if he.forward { e.start } else { e.end }
    }

    pub fn half_edge_end(&self, he: HalfEdge) -> Id<Vertex> {
        let e = &self.edges[he.edge];
        if he.forward { e.end } else { e.start }
    }
}
