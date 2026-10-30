//! Netz-Körper in der Oberfläche: Maße, Lage, Sperre, Vorschau, Auswahl (Teilprojekt 9,
//! Task 12).

use planora_app::mesh_body::*;
use planora_app::state::*;
use planora_exchange::{ExchangeMesh, MeshReport, MeshTransform};
use planora_feature::{BlendArt, mesh_locked};
use planora_kernel::Id;
use planora_kernel::boolean::BoolOp;
use planora_math::{DQuat, DVec3};
use planora_render::{FaceRef, Pick};
use std::sync::Arc;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> Arc<ExchangeMesh> {
    Arc::new(ExchangeMesh {
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
    })
}

/// Quader 10 mm und der Würfel als Netz-Körper in Zentimetern (gewählt).
fn with_mesh() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.import_mesh("Würfel", cube(), 10.0).unwrap();
    s
}

#[test]
fn an_imported_mesh_becomes_a_selected_body_with_its_measures() {
    let s = with_mesh();
    assert_eq!(s.bodies.len(), 2);
    let i = s.selected_body.unwrap();
    let b = &s.bodies[i];
    assert_eq!((b.kind, b.name.as_str()), (BodyKind::Mesh, "Würfel"));
    assert!((b.mass.volume - 1000.0).abs() < 1e-9, "{}", b.mass.volume);
    assert_eq!(b.bounds, (DVec3::ZERO, DVec3::splat(10.0)));
    assert!(b.mesh_report.unwrap().closed);
    assert!(!b.preview);
    assert_eq!(
        b.mesh.positions.len(),
        24,
        "harte Kanten: drei Punkte je Ecke"
    );
    assert_eq!(b.mesh.normals.len(), 24);
    assert_eq!(b.mesh.face_ranges.len(), 1);
    assert!(b.mesh.edge_lines.is_empty(), "keine Kantenlinien");
    assert!(s.is_mesh(i) && !s.is_mesh(1 - i));
    assert_eq!(s.solid_body_count(), 1);
    assert!(s.selected_is_mesh());
    s.document.check_invariants().unwrap();
}

#[test]
fn moving_rotating_and_scaling_change_the_mesh_body_itself() {
    let mut s = with_mesh();
    let count = s.document.features().len();
    let i = s.selected_body.unwrap();
    s.move_body(i, DVec3::new(5.0, 0.0, 0.0)).unwrap();
    assert_eq!(s.document.features().len(), count, "kein neues Verschieben");
    let i = s.selected_body.unwrap();
    assert_eq!(
        s.mesh_transform(i).unwrap().offset,
        DVec3::new(5.0, 0.0, 0.0)
    );
    let t = MeshTransform {
        offset: DVec3::new(5.0, 0.0, 0.0),
        rotation: rotation_from_degrees(DVec3::new(0.0, 0.0, 90.0)),
        scale: 20.0,
    };
    s.set_mesh_transform(i, t).unwrap();
    let b = &s.bodies[s.selected_body.unwrap()];
    assert!((b.mass.volume - 8000.0).abs() < 1e-6, "{}", b.mass.volume);
    assert!(
        (b.bounds.0.x + 15.0).abs() < 1e-9 && (b.bounds.1.y - 20.0).abs() < 1e-9,
        "{:?}",
        b.bounds
    );
    let d = euler_degrees(t.rotation);
    assert!((d - DVec3::new(0.0, 0.0, 90.0)).length() < 1e-9, "{d}");
    // ungültiger Maßstab: abgelehnt, Lage bleibt
    let bad = MeshTransform { scale: 0.0, ..t };
    assert!(s.set_mesh_transform(s.selected_body.unwrap(), bad).is_err());
    assert_eq!(s.mesh_transform(s.selected_body.unwrap()), Some(t));
    assert!(s.message.as_deref().unwrap().contains("Maßstab"));
    // kein Netz-Körper
    let q = 1 - s.selected_body.unwrap();
    assert!(s.set_mesh_transform(q, t).is_err());
    assert_eq!(s.mesh_transform(q), None);
}

#[test]
fn mesh_bodies_refuse_booleans_blends_and_sketches() {
    let mut s = with_mesh();
    let m = s.selected_body.unwrap();
    let q = 1 - m;
    let count = s.document.features().len();
    for (a, b) in [(q, m), (m, q)] {
        assert_eq!(
            s.add_boolean(a, b, BoolOp::Union),
            Err(mesh_locked().to_string())
        );
    }
    assert_eq!(
        s.add_blend(m, &[], BlendArt::Rundung { radius: 1.0 }, false),
        Err(mesh_locked().to_string())
    );
    assert_eq!(
        s.new_sketch_on_face(m, Id::from_index(0)),
        Err(mesh_locked().to_string())
    );
    assert_eq!(s.document.features().len(), count);
    assert_eq!(s.message.as_deref(), Some(mesh_locked().as_str()));
}

#[test]
fn very_large_meshes_are_shown_as_a_preview() {
    let mut s = AppState::new();
    s.display_triangles = 5;
    s.import_mesh("Groß", cube(), 1.0).unwrap();
    let b = &s.bodies[0];
    assert!(b.preview);
    assert_eq!(b.mesh.indices.len() / 3, 4, "jedes dritte Dreieck: 12 / 3");
    assert!(
        (b.mass.volume - 1.0).abs() < 1e-12,
        "Maße aus dem ganzen Netz"
    );
    assert_eq!(b.mesh_report.unwrap().triangles, 12);
    assert!(s.mesh_note().unwrap().contains(&preview_note()));
    assert_eq!(DISPLAY_TRIANGLES, 5_000_000);
    assert_eq!(AppState::new().display_triangles, DISPLAY_TRIANGLES);
}

#[test]
fn import_with_an_empty_name_counts_and_a_failed_import_changes_nothing() {
    let mut s = AppState::new();
    s.import_mesh("  ", cube(), 1.0).unwrap();
    assert_eq!(s.bodies[0].name, "Netz-Körper 1");
    let before = (s.document.features().len(), s.document.edit_count());
    assert!(
        s.import_mesh("Weit", cube(), 1e9).is_err(),
        "Maßstab über 1e6"
    );
    assert_eq!(
        (s.document.features().len(), s.document.edit_count()),
        before
    );
    s.import_mesh("", cube(), 1.0).unwrap();
    assert!(s.bodies.iter().any(|b| b.name == "Netz-Körper 2"));
}

#[test]
fn selection_survives_a_refresh_and_the_state_reloads() {
    let mut s = with_mesh();
    let m = s.selected_body.unwrap();
    s.set_selection(Some(Pick::Face(FaceRef {
        body: m,
        face: Id::from_index(0),
    })));
    s.refresh();
    assert!(s.selection.is_some(), "Auswahl auf dem Netz-Körper bleibt");
    let back = AppState::from_snapshot(s.snapshot()).unwrap();
    assert_eq!(back.bodies.len(), 2);
    assert!(back.bodies.iter().any(|b| b.kind == BodyKind::Mesh));
}

#[test]
fn report_texts_are_plain_german() {
    let closed = MeshReport {
        triangles: 12,
        vertices: 8,
        closed: true,
        ..MeshReport::default()
    };
    assert_eq!(report_text(&closed), "Das Netz ist geschlossen.");
    let open = MeshReport {
        open_edges: 1,
        flipped_edges: 2,
        degenerate_triangles: 1,
        ..MeshReport::default()
    };
    assert_eq!(
        report_text(&open),
        "Das Netz ist nicht ganz geschlossen: 1 offene Kante, 2 Kanten mit falsch herum \
         liegenden Dreiecken. 1 Dreieck hat keine Fläche."
    );
}

#[test]
fn tiny_angles_show_as_positive_zero() {
    // Rundungsreste der Euler-Zerlegung dürfen nie „-0 °“ ergeben.
    let tiny = DQuat::from_rotation_z(-1e-15);
    let d = euler_degrees(tiny);
    assert!(
        d.x.is_sign_positive() && d.y.is_sign_positive() && d.z.is_sign_positive(),
        "{d}"
    );
    assert_eq!(d, DVec3::ZERO);
    let d = euler_degrees(rotation_from_degrees(DVec3::new(0.0, 0.0, -90.0)));
    assert!((d.z + 90.0).abs() < 1e-9, "{d}");
}
