//! Nullreste an Nähten und Polen: Kernel-Körper im Ursprung müssen nach dem Umwandeln
//! geschlossen sein, eingelesene Netze bleiben exakt (Spec §4 und §12, Korrekturwelle K1).

use planora_exchange::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{AnalyticKernel, BlendEdge, BlendKind, BlendRequest, Kernel, Solid};
use planora_mesh::{TessellationOptions, tessellate};

/// Kugel, Zylinder, Kegel und Zylinder mit gerundeter Kante, jeweils im Ursprung.
fn cases() -> Vec<(&'static str, Solid)> {
    let k = AnalyticKernel::default();
    let cylinder = k.make_cylinder(4.0, 10.0).unwrap();
    let edge = cylinder
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            edge_uses(&cylinder, id)
                .iter()
                .any(|u| face_name(&cylinder, u.face) == "Deckel")
                && edge_uses(&cylinder, id).len() == 2
                && edge_uses(&cylinder, id)
                    .iter()
                    .any(|u| face_name(&cylinder, u.face) != "Deckel")
        })
        .unwrap();
    let rounded = k
        .blend(
            &cylinder,
            &BlendRequest {
                edges: vec![BlendEdge {
                    edge,
                    first_face: None,
                }],
                kind: BlendKind::Fillet { radius: 1.0 },
                tangent_chain: false,
            },
        )
        .unwrap()
        .solid;
    vec![
        ("Kugel", k.make_sphere(20.0).unwrap()),
        ("Zylinder", cylinder),
        ("Kegel", k.make_cone(5.0, 12.0).unwrap()),
        ("Gerundeter Zylinder", rounded),
    ]
}

fn open_edges_after_files(body: &ExportBody) -> [usize; 2] {
    let mut stl = Vec::new();
    write_stl(&mut stl, std::slice::from_ref(body)).unwrap();
    let mut obj = Vec::new();
    write_obj(&mut obj, std::slice::from_ref(body)).unwrap();
    let p = Progress::new();
    [
        check_mesh(&read_stl(&stl[..], stl.len() as u64, &p).unwrap()).open_edges,
        check_mesh(&read_obj(&obj[..], obj.len() as u64, &p).unwrap()).open_edges,
    ]
}

#[test]
fn kernel_bodies_at_the_origin_are_closed_at_every_quality() {
    for (name, solid) in cases() {
        // Grob, Normal, Fein (Spec E5)
        for chord in [0.1, 0.01, 0.001] {
            let mesh = tessellate(
                &solid,
                &TessellationOptions {
                    chord_tolerance: chord,
                },
            )
            .unwrap();
            let body = collect_body(&SolidSource { name, mesh: &mesh }).unwrap();
            let report = check_mesh(&body.mesh);
            assert!(
                report.closed,
                "{name} {chord}: {} offene Kanten",
                report.open_edges
            );
            assert_eq!(
                open_edges_after_files(&body),
                [0, 0],
                "{name} {chord}: nach STL und OBJ"
            );
        }
    }
}

#[test]
fn imported_meshes_stay_exact() {
    // Zwei Punkte im Abstand 1e-12 sind verschieden und bleiben es (Spec §4).
    let mesh = ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1e-12, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        indices: vec![0, 2, 3, 1, 2, 3],
    };
    let body = collect_body(&PlacedSource {
        name: "Import",
        mesh: &mesh,
        transform: MeshTransform::IDENTITY,
    })
    .unwrap();
    assert_eq!(body.mesh.vertex_count(), 4);
    // `weld_exact` ordnet die Punkte neu; es kommt nur darauf an, dass beide da sind.
    assert!(body.mesh.positions.contains(&[1e-12, 0.0, 0.0]));
    assert!(body.mesh.positions.contains(&[0.0, 0.0, 0.0]));
}
