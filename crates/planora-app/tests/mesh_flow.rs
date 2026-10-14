//! Stufentest 3: Netz einlesen, als Netz-Körper bewegen, speichern (Format 3), laden und
//! wieder exportieren (Teilprojekt 9, Task 14).

use planora_app::state::*;
use planora_exchange::*;
use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::{DQuat, DVec3, PI};
use planora_mesh::{TessellationOptions, tessellate};
use planora_project::{decode_version, encode_version};
use std::sync::Arc;

/// serde_json liest f64 nur bis auf 1 ULP genau zurück (ohne „float_roundtrip“); so weit dürfen
/// Volumen und Drehung nach Speichern und Laden abweichen.
const ROUNDTRIP_REL: f64 = 1e-12;
const ROUNDTRIP_ROT: f64 = 1e-12;

#[test]
fn a_mesh_travels_from_file_to_document_and_back() {
    let dir = std::env::temp_dir().join(format!("planora-flow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let k = AnalyticKernel::default();
    let cylinder = k.make_cylinder(5.0, 10.0).unwrap();
    let mesh = tessellate(
        &cylinder,
        &TessellationOptions {
            chord_tolerance: 0.01,
        },
    )
    .unwrap();
    let body = collect_body(&SolidSource {
        name: "Zylinder",
        mesh: &mesh,
    })
    .unwrap();
    let path = dir.join("zylinder.stl");
    export_file(&path, Format::Stl, std::slice::from_ref(&body)).unwrap();
    let read = import_file(&path, &Progress::new()).unwrap();
    assert_eq!(read.triangle_count(), body.mesh.triangle_count());

    let mut s = AppState::new();
    s.import_mesh("Zylinder", Arc::new(read), 1.0).unwrap();
    let i = s.selected_body.unwrap();
    let v0 = s.bodies[i].mass.volume;
    let area = s.bodies[i].mass.surface_area;
    let exact = PI * 25.0 * 10.0;
    assert!((v0 - exact).abs() <= 0.01 * area, "{v0} / {exact}");

    let t = MeshTransform {
        offset: DVec3::new(30.0, 0.0, 0.0),
        rotation: DQuat::from_rotation_x(0.5),
        scale: 2.0,
    };
    s.set_mesh_transform(i, t).unwrap();
    let v1 = s.bodies[s.selected_body.unwrap()].mass.volume;
    assert!((v1 - 8.0 * v0).abs() < 1e-6 * v1, "{v1} / {v0}");

    // Speichern und Laden (Format 3)
    let text = encode_version(&s.snapshot()).unwrap();
    assert!(text.contains("\"format\": 3"));
    let back = AppState::from_snapshot(decode_version(&text, "x.json").unwrap()).unwrap();
    let j = back
        .bodies
        .iter()
        .position(|b| b.kind == BodyKind::Mesh)
        .unwrap();
    assert!(
        (back.bodies[j].mass.volume - v1).abs() <= ROUNDTRIP_REL * v1,
        "{} / {v1}",
        back.bodies[j].mass.volume
    );
    let bt = back.mesh_transform(j).unwrap();
    assert_eq!((bt.offset, bt.scale), (t.offset, t.scale));
    assert!(
        bt.rotation.abs_diff_eq(t.rotation, ROUNDTRIP_ROT),
        "{:?} / {:?}",
        bt.rotation,
        t.rotation
    );

    // Wieder exportieren: das Netz mit seiner Lage
    let (m, t2) = back
        .document
        .mesh_body(back.bodies[j].source.feature)
        .unwrap();
    let again = collect_body(&PlacedSource {
        name: "Zylinder",
        mesh: m,
        transform: t2,
    })
    .unwrap();
    let path2 = dir.join("wieder.obj");
    export_file(&path2, Format::Obj, std::slice::from_ref(&again)).unwrap();
    let reread = import_file(&path2, &Progress::new()).unwrap();
    let v2 = reread.mass(&MeshTransform::IDENTITY).volume;
    assert!((v2 - v1).abs() < 1e-4 * v1, "{v2} / {v1}");
    assert_eq!(check_mesh(&reread).closed, check_mesh(&again.mesh).closed);
    let _ = std::fs::remove_dir_all(&dir);
}
