//! Netzprüfung, Verschmelzen und Quellen (Teilprojekt 9, Task 2).

use planora_exchange::*;
use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::{DQuat, DVec3};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

#[test]
fn a_closed_cube_is_closed() {
    assert_eq!(
        check_mesh(&cube()),
        MeshReport {
            triangles: 12,
            vertices: 8,
            open_edges: 0,
            non_manifold_edges: 0,
            flipped_edges: 0,
            degenerate_triangles: 0,
            closed: true,
        }
    );
}

#[test]
fn a_missing_triangle_opens_three_edges() {
    let mut m = cube();
    m.indices.truncate(33);
    let r = check_mesh(&m);
    assert_eq!((r.triangles, r.open_edges, r.closed), (11, 3, false));
}

#[test]
fn a_flipped_triangle_is_reported_as_not_closed() {
    let mut m = cube();
    m.indices.swap(1, 2); // erstes Dreieck (0, 2, 1) → (0, 1, 2)
    let r = check_mesh(&m);
    assert_eq!((r.flipped_edges, r.open_edges), (3, 0));
    assert!(!r.closed);
}

#[test]
fn a_fin_is_non_manifold() {
    let mut m = cube();
    m.positions.push([0.5, -1.0, 0.0]); // Punkt 8
    m.indices.extend_from_slice(&[0, 1, 8]); // drittes Dreieck an der Kante 0–1
    let r = check_mesh(&m);
    assert_eq!((r.non_manifold_edges, r.open_edges), (1, 2));
    assert!(!r.closed);
}

#[test]
fn degenerate_triangles_are_counted_not_removed() {
    let mut m = cube();
    m.indices.extend_from_slice(&[0, 0, 1]); // doppelter Punkt
    m.positions.push([0.5, 0.0, 0.0]); // Punkt 8 auf der Kante 0–1
    m.indices.extend_from_slice(&[0, 8, 1]); // Fläche genau null
    let r = check_mesh(&m);
    assert_eq!((r.degenerate_triangles, r.triangles), (2, 14));
    assert!(!r.closed);
}

#[test]
fn an_empty_mesh_is_not_closed() {
    assert!(!check_mesh(&ExchangeMesh::default()).closed);
    let only_collapsed = ExchangeMesh {
        positions: vec![[0.0; 3]],
        indices: vec![0, 0, 0],
    };
    assert!(!check_mesh(&only_collapsed).closed);
}

#[test]
fn reports_combine() {
    let a = check_mesh(&cube());
    let mut open = cube();
    open.indices.truncate(33);
    let b = check_mesh(&open);
    let c = MeshReport::combine(&[a, b]);
    assert_eq!(
        (c.triangles, c.vertices, c.open_edges, c.closed),
        (23, 16, 3, false)
    );
    assert!(MeshReport::combine(&[a, a]).closed);
    assert!(!MeshReport::combine(&[]).closed);
}

#[test]
fn weld_merges_only_bit_identical_points() {
    // Zwei Dreiecke mit getrennten Punkten wie in STL; ein Punkt weicht um ein Bit ab, −0 ist +0.
    let x = 1.0f32;
    let x_next = f32::from_bits(x.to_bits() + 1);
    let positions = [
        [0.0, 0.0, 0.0],
        [x, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [x, 0.0, 0.0],
        [-0.0, 0.0, 0.0],
        [x_next, 1.0, 0.0],
        [9.0, 9.0, 9.0],
    ];
    let w = weld_exact(&positions, &[0, 1, 2, 3, 4, 5]);
    assert_eq!(
        w.positions,
        vec![
            [0.0, 0.0, 0.0],
            [x, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [x_next, 1.0, 0.0]
        ]
    );
    assert_eq!(w.indices, vec![0, 1, 2, 1, 0, 3]);
}

#[test]
fn a_triangle_soup_cube_becomes_closed_after_welding() {
    let c = cube();
    let soup: Vec<[f32; 3]> = c.indices.iter().map(|&i| c.positions[i as usize]).collect();
    let idx: Vec<u32> = (0..soup.len() as u32).collect();
    let unwelded = ExchangeMesh {
        positions: soup.clone(),
        indices: idx.clone(),
    };
    assert!(!check_mesh(&unwelded).closed);
    let w = weld_exact(&soup, &idx);
    assert_eq!(w.positions.len(), 8);
    assert!(check_mesh(&w).closed);
}

#[test]
fn a_kernel_box_becomes_a_closed_export_body() {
    let k = AnalyticKernel::default();
    let solid = k.make_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let mesh = tessellate(&solid, &TessellationOptions::default()).unwrap();
    let body = collect_body(&SolidSource {
        name: "Quader 1",
        mesh: &mesh,
    })
    .unwrap();
    assert_eq!(body.name, "Quader 1");
    assert_eq!(body.mesh.triangle_count(), mesh.indices.len() / 3);
    assert!(
        body.mesh.positions.len() < mesh.positions.len(),
        "Flächen teilen ihre Ecken"
    );
    let r = check_mesh(&body.mesh);
    assert!(r.closed, "{r:?}");
    let v = body.mesh.mass(&MeshTransform::IDENTITY).volume;
    assert!((v - 6000.0).abs() < 1e-3, "{v}");
    assert!((v - mass_properties(&mesh).volume).abs() < 1e-3);
}

#[test]
fn a_placed_source_applies_the_transform() {
    let c = cube();
    let t = MeshTransform {
        offset: DVec3::new(5.0, 0.0, 0.0),
        rotation: DQuat::IDENTITY,
        scale: 3.0,
    };
    let body = collect_body(&PlacedSource {
        name: "Netz",
        mesh: &c,
        transform: t,
    })
    .unwrap();
    let v = body.mesh.mass(&MeshTransform::IDENTITY).volume;
    assert!((v - 27.0).abs() < 1e-4, "{v}");
    let min_x = body
        .mesh
        .positions
        .iter()
        .map(|p| p[0])
        .fold(f32::MAX, f32::min);
    assert_eq!(min_x, 5.0);
}

#[test]
fn unrepresentable_points_and_bad_indices_are_named() {
    let c = cube();
    let huge = MeshTransform {
        offset: DVec3::new(1e300, 0.0, 0.0),
        ..MeshTransform::IDENTITY
    };
    assert_eq!(
        collect_body(&PlacedSource {
            name: "x",
            mesh: &c,
            transform: huge
        }),
        Err(ExchangeError::NotFinite { line: None })
    );
    let mut bad = cube();
    bad.indices[5] = 99;
    assert!(matches!(
        collect_body(&PlacedSource {
            name: "x",
            mesh: &bad,
            transform: MeshTransform::IDENTITY
        }),
        Err(ExchangeError::Corrupt(_))
    ));
}
