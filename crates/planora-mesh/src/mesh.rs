use planora_kernel::{Edge, Face, Id};
use planora_math::DVec3;

/// Dreiecksnetz eines Körpers. Jede Fläche besitzt eigene Eckpunkte (kein Teilen über Flächen
/// hinweg), damit Normalen und Zuordnung zur Fläche eindeutig bleiben.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<DVec3>,
    pub normals: Vec<DVec3>,
    pub indices: Vec<u32>,
    pub face_ranges: Vec<FaceRange>,
    pub edge_lines: Vec<EdgeLine>,
    /// Flächen, auf denen die Sehnentoleranz nicht überall garantiert ist, weil eine Grenze der
    /// Verfeinerung (Rundenzahl, Punktzahl) erreicht wurde. Das Netz ist trotzdem geschlossen und
    /// brauchbar, nur gröber als verlangt; leer im Normalfall.
    pub inexact_faces: Vec<Id<Face>>,
}

/// Indexbereich `first_index .. first_index + index_count` gehört zu `face`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceRange {
    pub face: Id<Face>,
    pub first_index: u32,
    pub index_count: u32,
}

/// Linienzug einer Kante für die Darstellung.
#[derive(Clone, Debug)]
pub struct EdgeLine {
    pub edge: Id<Edge>,
    pub points: Vec<DVec3>,
}

#[derive(Clone, Copy, Debug)]
pub struct TessellationOptions {
    /// Größte erlaubte Abweichung der Sehne vom Kreisbogen in Millimetern.
    pub chord_tolerance: f64,
}

impl Default for TessellationOptions {
    fn default() -> Self {
        Self {
            chord_tolerance: 0.01,
        }
    }
}
