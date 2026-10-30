use bytemuck::{Pod, Zeroable};
use planora_kernel::{Edge, Face, Id};
use planora_mesh::Mesh;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FaceRef {
    pub body: usize,
    pub face: Id<Face>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EdgeRef {
    pub body: usize,
    pub edge: Id<Edge>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pick {
    Face(FaceRef),
    Edge(EdgeRef),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Highlight {
    pub hover: Option<Pick>,
    pub selected: Option<Pick>,
}

/// Farben in sRGB-Werten (0..1); egui zeichnet ohne zusätzliche Gamma-Umrechnung.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub face_base: [f32; 4],
    pub face_hover: [f32; 4],
    pub face_selected: [f32; 4],
    pub edge_base: [f32; 4],
    pub edge_hover: [f32; 4],
    pub edge_selected: [f32; 4],
    pub grid: [f32; 4],
    /// Jede fünfte Rasterlinie (kräftiger als `grid`).
    pub grid_major: [f32; 4],
    pub axis_x: [f32; 4],
    pub axis_y: [f32; 4],
    pub axis_z: [f32; 4],
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            face_base: [0.62, 0.68, 0.78, 1.0],
            face_hover: [0.45, 0.72, 1.0, 1.0],
            face_selected: [1.0, 0.62, 0.20, 1.0],
            edge_base: [0.06, 0.07, 0.09, 1.0],
            edge_hover: [0.30, 0.80, 1.0, 1.0],
            edge_selected: [1.0, 0.55, 0.10, 1.0],
            grid: [0.24, 0.26, 0.30, 1.0],
            grid_major: [0.40, 0.43, 0.49, 1.0],
            axis_x: [0.85, 0.28, 0.28, 1.0],
            axis_y: [0.35, 0.75, 0.35, 1.0],
            axis_z: [0.30, 0.50, 0.95, 1.0],
        }
    }

    pub fn light() -> Self {
        Self {
            face_base: [0.70, 0.76, 0.86, 1.0],
            face_hover: [0.40, 0.65, 0.95, 1.0],
            face_selected: [0.95, 0.55, 0.15, 1.0],
            edge_base: [0.12, 0.13, 0.16, 1.0],
            edge_hover: [0.10, 0.50, 0.85, 1.0],
            edge_selected: [0.90, 0.45, 0.05, 1.0],
            grid: [0.78, 0.80, 0.84, 1.0],
            grid_major: [0.60, 0.63, 0.69, 1.0],
            axis_x: [0.80, 0.20, 0.20, 1.0],
            axis_y: [0.25, 0.65, 0.25, 1.0],
            axis_z: [0.20, 0.40, 0.90, 1.0],
        }
    }

    /// Gedämpfte Farben für den Hintergrund des Skizzen-Modus: Flächen- und Kantenfarben zu
    /// [`DIM_AMOUNT`] in Richtung Mittelgrau gemischt, Raster und Achsen bleiben.
    pub fn dimmed(&self) -> Palette {
        let mix = |c: [f32; 4]| {
            let m = |v: f32| (DIM_GREY + (v - DIM_GREY) * (1.0 - DIM_AMOUNT)).clamp(0.0, 1.0);
            [m(c[0]), m(c[1]), m(c[2]), c[3]]
        };
        Palette {
            face_base: mix(self.face_base),
            face_hover: mix(self.face_hover),
            face_selected: mix(self.face_selected),
            edge_base: mix(self.edge_base),
            edge_hover: mix(self.edge_hover),
            edge_selected: mix(self.edge_selected),
            ..*self
        }
    }
}

/// Grauwert, zu dem `Palette::dimmed` die Farben mischt.
const DIM_GREY: f32 = 0.5;
/// Anteil der Mischung zum Grau in `Palette::dimmed` (60 %).
const DIM_AMOUNT: f32 = 0.6;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FaceVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LineVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

/// Fertig aufbereitete Daten für die Grafikkarte.
#[derive(Clone, Debug, Default)]
pub struct SceneData {
    pub face_vertices: Vec<FaceVertex>,
    pub face_indices: Vec<u32>,
    /// Linienliste: je zwei aufeinanderfolgende Einträge bilden eine Linie.
    pub line_vertices: Vec<LineVertex>,
}

fn p3(p: planora_math::DVec3) -> [f32; 3] {
    [p.x as f32, p.y as f32, p.z as f32]
}

/// Baut die Szene aus allen Körpern. Raster und Achsen gehören nicht dazu: sie folgen der
/// Kamera und kommen je Bild aus [`crate::grid_lines`].
pub fn build_scene(meshes: &[&Mesh], highlight: &Highlight, palette: &Palette) -> SceneData {
    build_scene_marked(meshes, highlight, &[], palette)
}

/// Wie [`build_scene`]; `marked` (Mehrfachauswahl) bekommen die Farbe der Auswahl.
pub fn build_scene_marked(
    meshes: &[&Mesh],
    highlight: &Highlight,
    marked: &[Pick],
    palette: &Palette,
) -> SceneData {
    let mut scene = SceneData::default();

    for (body, mesh) in meshes.iter().enumerate() {
        // Alle Eckpunkte der Szene müssen mit `u32` adressierbar bleiben.
        let fits = scene
            .face_vertices
            .len()
            .checked_add(mesh.positions.len())
            .is_some_and(|n| n <= u32::MAX as usize);
        let (true, Ok(vertex_base)) = (fits, u32::try_from(scene.face_vertices.len())) else {
            eprintln!("Warning: scene too large, body {} is not drawn", body + 1);
            continue;
        };
        scene.face_vertices.extend(
            mesh.positions
                .iter()
                .zip(mesh.normals.iter())
                .map(|(p, n)| FaceVertex {
                    position: p3(*p),
                    normal: p3(*n),
                    color: palette.face_base,
                }),
        );
        for range in &mesh.face_ranges {
            let this = Pick::Face(FaceRef {
                body,
                face: range.face,
            });
            let color = if highlight.selected == Some(this) || marked.contains(&this) {
                palette.face_selected
            } else if highlight.hover == Some(this) {
                palette.face_hover
            } else {
                continue;
            };
            let start = range.first_index as usize;
            let end = start + range.index_count as usize;
            for &i in &mesh.indices[start..end] {
                scene.face_vertices[(vertex_base + i) as usize].color = color;
            }
        }
        scene
            .face_indices
            .extend(mesh.indices.iter().map(|i| i + vertex_base));

        for line in &mesh.edge_lines {
            let this = Pick::Edge(EdgeRef {
                body,
                edge: line.edge,
            });
            let color = if highlight.selected == Some(this) || marked.contains(&this) {
                palette.edge_selected
            } else if highlight.hover == Some(this) {
                palette.edge_hover
            } else {
                palette.edge_base
            };
            for seg in line.points.windows(2) {
                for p in seg {
                    scene.line_vertices.push(LineVertex {
                        position: p3(*p),
                        color,
                    });
                }
            }
        }
    }

    scene
}
