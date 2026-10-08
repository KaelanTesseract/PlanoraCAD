use planora_kernel::*;
use planora_math::DVec3;
use planora_mesh::*;
use planora_render::*;

fn box_mesh() -> Mesh {
    let solid = AnalyticKernel::default()
        .make_box(DVec3::new(2.0, 3.0, 4.0))
        .unwrap();
    tessellate(&solid, &TessellationOptions::default()).unwrap()
}

#[test]
fn scene_copies_all_vertices_and_indices() {
    let mesh = box_mesh();
    let scene = build_scene(&[&mesh], &Highlight::default(), &Palette::dark(), 0.0);
    assert_eq!(scene.face_vertices.len(), mesh.positions.len());
    assert_eq!(scene.face_indices, mesh.indices);
    let expected_line_vertices: usize = mesh
        .edge_lines
        .iter()
        .map(|l| (l.points.len() - 1) * 2)
        .sum();
    assert_eq!(scene.line_vertices.len(), expected_line_vertices);
}

#[test]
fn selected_face_gets_selected_color_and_others_stay_base() {
    let mesh = box_mesh();
    let palette = Palette::dark();
    let face = mesh.face_ranges[0].face;
    let hl = Highlight {
        hover: None,
        selected: Some(Pick::Face(FaceRef { body: 0, face })),
    };
    let scene = build_scene(&[&mesh], &hl, &palette, 0.0);
    let color_of = |range: &FaceRange| {
        let i = mesh.indices[range.first_index as usize] as usize;
        scene.face_vertices[i].color
    };
    assert_eq!(color_of(&mesh.face_ranges[0]), palette.face_selected);
    assert_eq!(color_of(&mesh.face_ranges[1]), palette.face_base);
}

#[test]
fn hovered_edge_gets_hover_color() {
    let mesh = box_mesh();
    let palette = Palette::dark();
    let edge = mesh.edge_lines[0].edge;
    let hl = Highlight {
        hover: Some(Pick::Edge(EdgeRef { body: 0, edge })),
        selected: None,
    };
    let scene = build_scene(&[&mesh], &hl, &palette, 0.0);
    assert_eq!(scene.line_vertices[0].color, palette.edge_hover);
    let other = (mesh.edge_lines[0].points.len() - 1) * 2;
    assert_eq!(scene.line_vertices[other].color, palette.edge_base);
}

#[test]
fn highlight_only_applies_to_its_own_body() {
    let mesh = box_mesh();
    let palette = Palette::dark();
    let face = mesh.face_ranges[0].face;
    let hl = Highlight {
        hover: None,
        selected: Some(Pick::Face(FaceRef { body: 1, face })),
    };
    let scene = build_scene(&[&mesh, &mesh], &hl, &palette, 0.0);
    let n = mesh.positions.len();
    let first = mesh.indices[mesh.face_ranges[0].first_index as usize] as usize;
    assert_eq!(scene.face_vertices[first].color, palette.face_base);
    assert_eq!(scene.face_vertices[n + first].color, palette.face_selected);
    // Indizes des zweiten Körpers sind um die Eckenzahl des ersten verschoben.
    assert_eq!(scene.face_indices.len(), mesh.indices.len() * 2);
    assert_eq!(
        scene.face_indices[mesh.indices.len()],
        mesh.indices[0] + n as u32
    );
}

#[test]
fn grid_adds_lines_and_empty_scene_is_fine() {
    let empty = build_scene(&[], &Highlight::default(), &Palette::dark(), 100.0);
    assert!(empty.face_vertices.is_empty());
    assert!(!empty.line_vertices.is_empty());
    assert_eq!(empty.line_vertices.len() % 2, 0);
    let none = build_scene(&[], &Highlight::default(), &Palette::dark(), 0.0);
    assert!(none.line_vertices.is_empty());
}
