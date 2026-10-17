//! Hervorhebung mehrerer Kanten (Teilprojekt 8, Task 10).

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
fn marked_edges_get_the_selected_colour() {
    let mesh = box_mesh();
    let palette = Palette::dark();
    let (e0, e1) = (mesh.edge_lines[0].edge, mesh.edge_lines[1].edge);
    let marked = [
        Pick::Edge(EdgeRef { body: 0, edge: e0 }),
        Pick::Edge(EdgeRef { body: 0, edge: e1 }),
    ];
    let scene = build_scene_marked(&[&mesh], &Highlight::default(), &marked, &palette);
    let first = |i: usize| -> usize {
        mesh.edge_lines[..i]
            .iter()
            .map(|l| (l.points.len() - 1) * 2)
            .sum()
    };
    assert_eq!(scene.line_vertices[first(0)].color, palette.edge_selected);
    assert_eq!(scene.line_vertices[first(1)].color, palette.edge_selected);
    assert_eq!(scene.line_vertices[first(2)].color, palette.edge_base);
    // Ohne Markierung gleich wie bisher.
    let plain = build_scene(&[&mesh], &Highlight::default(), &palette);
    let unmarked = build_scene_marked(&[&mesh], &Highlight::default(), &[], &palette);
    let colours = |s: &SceneData| s.line_vertices.iter().map(|v| v.color).collect::<Vec<_>>();
    assert_eq!(colours(&plain), colours(&unmarked));
}
