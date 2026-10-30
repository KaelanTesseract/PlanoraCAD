//! Netz-Körper im Dokument: Sichtbarkeit, gemeinsame Netze, Lage, Unterdrücken, Löschen,
//! Sperre, Import ganz oder gar nicht (Teilprojekt 9, Task 10).

use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::{DQuat, DVec3, Frame, Tolerance};
use planora_sketch::Sketch;
use std::sync::Arc;

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> Arc<ExchangeMesh> {
    Arc::new(ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    })
}

fn at(x: f64) -> MeshTransform {
    MeshTransform {
        offset: DVec3::new(x, 0.0, 0.0),
        ..MeshTransform::IDENTITY
    }
}

/// Änderung an einem Feature-Parameter (Clippy: Typ zu verschachtelt).
type Edit = fn(&mut FeatureKind);

fn status(d: &Document, id: FeatureId) -> FeatureStatus {
    d.feature(id).unwrap().status.clone()
}

fn mesh_id(d: &Document, id: FeatureId) -> MeshId {
    match &d.feature(id).unwrap().kind {
        FeatureKind::MeshBody { mesh, .. } => *mesh,
        other => panic!("kein Netz-Körper: {other:?}"),
    }
}

#[test]
fn a_mesh_body_is_visible_with_its_transform() {
    let mut d = Document::new();
    let id = d.add_mesh_body("Netz 1", tetra(), at(20.0)).unwrap();
    assert_eq!(status(&d, id), FeatureStatus::Ok);
    assert!(d.visible_bodies().is_empty(), "kein Kernel-Körper");
    let v = d.visible_meshes();
    assert_eq!(v.len(), 1);
    assert_eq!(
        v[0].body,
        BodyRef {
            feature: id,
            index: 0
        }
    );
    assert_eq!(v[0].name, "Netz 1");
    assert_eq!(v[0].transform, at(20.0));
    assert_eq!(v[0].mesh_id, mesh_id(&d, id));
    let kind = &d.feature(id).unwrap().kind;
    assert_eq!(kind.label(), "Netz-Körper");
    assert!(kind.dependencies().is_empty());
    assert_eq!(kind.mesh_transform(), Some(at(20.0)));
    assert_eq!(d.mesh_count(), 1);
    d.check_invariants().unwrap();
}

#[test]
fn several_features_can_share_one_mesh() {
    let mut d = Document::new();
    let a = d.add_mesh_body("A", tetra(), at(0.0)).unwrap();
    let mid = mesh_id(&d, a);
    let shared = Arc::clone(d.mesh(mid).unwrap());
    let b = d
        .add_feature(
            "B",
            FeatureKind::MeshBody {
                mesh: mid,
                offset: DVec3::new(30.0, 0.0, 0.0),
                rotation: DQuat::IDENTITY,
                scale: 2.0,
            },
        )
        .unwrap();
    assert_eq!(status(&d, b), FeatureStatus::Ok);
    assert_eq!(d.mesh_count(), 1);
    assert!(Arc::ptr_eq(d.mesh_body(b).unwrap().0, &shared));
    assert_eq!(d.visible_meshes().len(), 2);
    assert_eq!(d.snapshot().meshes.len(), 1);
    // Löschen von A: Netz bleibt (B benutzt es); Löschen von B: Netz entfällt
    d.delete(a).unwrap();
    assert_eq!(d.mesh_count(), 1);
    d.delete(b).unwrap();
    assert_eq!(d.mesh_count(), 0);
    d.check_invariants().unwrap();
}

#[test]
fn transform_suppress_and_delete_follow_the_document() {
    let mut d = Document::new();
    let id = d
        .add_mesh_body("N", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    d.update(id, |k| {
        if let FeatureKind::MeshBody {
            scale, rotation, ..
        } = k
        {
            *scale = 3.0;
            *rotation = DQuat::from_rotation_y(1.0);
        }
    })
    .unwrap();
    let meshes = d.visible_meshes();
    let vol = meshes[0].mesh.mass(&meshes[0].transform).volume;
    assert!((vol - 27.0 * 1000.0 / 6.0).abs() < 1e-6, "{vol}");
    d.set_suppressed(id, true).unwrap();
    assert!(d.visible_meshes().is_empty());
    assert_eq!(status(&d, id), FeatureStatus::Suppressed);
    d.set_suppressed(id, false).unwrap();
    assert_eq!(d.visible_meshes().len(), 1);
    d.delete(id).unwrap();
    assert!(d.visible_meshes().is_empty());
    assert_eq!(d.mesh_count(), 0);
    d.check_invariants().unwrap();
}

#[test]
fn bad_parameters_become_named_error_states() {
    let mut d = Document::new();
    let id = d
        .add_mesh_body("N", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    let cases: [(&str, Edit); 6] = [
        ("Maßstab", |k| {
            if let FeatureKind::MeshBody { scale, .. } = k {
                *scale = 0.0;
            }
        }),
        ("Maßstab", |k| {
            if let FeatureKind::MeshBody { scale, .. } = k {
                *scale = 2e6;
            }
        }),
        ("Maßstab", |k| {
            if let FeatureKind::MeshBody { scale, .. } = k {
                *scale = f64::NAN;
            }
        }),
        ("Drehung", |k| {
            if let FeatureKind::MeshBody { rotation, .. } = k {
                *rotation = DQuat::from_xyzw(0.0, 0.0, 0.0, 2.0);
            }
        }),
        ("Drehung", |k| {
            if let FeatureKind::MeshBody { rotation, .. } = k {
                *rotation = DQuat::from_xyzw(f64::NAN, 0.0, 0.0, 1.0);
            }
        }),
        ("außerhalb", |k| {
            if let FeatureKind::MeshBody { offset, .. } = k {
                *offset = DVec3::new(2e9, 0.0, 0.0);
            }
        }),
    ];
    for (word, edit) in cases {
        d.update(id, edit).unwrap();
        match status(&d, id) {
            FeatureStatus::Error(e) => assert!(e.contains(word), "{e}"),
            s => panic!("{s:?}"),
        }
        assert!(d.visible_meshes().is_empty());
        d.check_invariants().unwrap();
        d.update(id, |k| {
            if let FeatureKind::MeshBody {
                scale,
                rotation,
                offset,
                ..
            } = k
            {
                *scale = 1.0;
                *rotation = DQuat::IDENTITY;
                *offset = DVec3::ZERO;
            }
        })
        .unwrap();
        assert_eq!(status(&d, id), FeatureStatus::Ok);
    }
    // Lage im Bereich, aber eine Ecke liegt bei 1e9 + 10
    d.update(id, |k| {
        if let FeatureKind::MeshBody { offset, .. } = k {
            *offset = DVec3::new(MAX_COORD_MM, 0.0, 0.0);
        }
    })
    .unwrap();
    assert!(matches!(status(&d, id), FeatureStatus::Error(e) if e.contains("außerhalb")));
}

#[test]
fn import_is_all_or_nothing() {
    let mut d = Document::new();
    d.add_feature(
        "Q",
        FeatureKind::Box {
            size: DVec3::ONE,
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let before = (
        d.features().len(),
        d.edit_count(),
        d.mesh_count(),
        d.snapshot().next_id,
    );
    let far = MeshTransform {
        offset: DVec3::new(2e9, 0.0, 0.0),
        ..MeshTransform::IDENTITY
    };
    assert!(matches!(
        d.add_mesh_body("N", tetra(), far),
        Err(FeatureError::OutOfRange(_))
    ));
    let edge = MeshTransform {
        offset: DVec3::new(MAX_COORD_MM, 0.0, 0.0),
        ..MeshTransform::IDENTITY
    };
    assert!(matches!(
        d.add_mesh_body("N", tetra(), edge),
        Err(FeatureError::OutOfRange(_))
    ));
    let tiny = MeshTransform {
        scale: 1e-9,
        ..MeshTransform::IDENTITY
    };
    assert!(matches!(
        d.add_mesh_body("N", tetra(), tiny),
        Err(FeatureError::InvalidInput(_))
    ));
    let broken = Arc::new(ExchangeMesh {
        positions: vec![[0.0; 3]],
        indices: vec![0, 0, 5],
    });
    assert!(matches!(
        d.add_mesh_body("N", broken, MeshTransform::IDENTITY),
        Err(FeatureError::InvalidInput(_))
    ));
    assert_eq!(
        (
            d.features().len(),
            d.edit_count(),
            d.mesh_count(),
            d.snapshot().next_id
        ),
        before
    );
    d.check_invariants().unwrap();
}

#[test]
fn mesh_bodies_cannot_be_edited_by_other_features() {
    let mut d = Document::new();
    let q = d
        .add_feature(
            "Q",
            FeatureKind::Box {
                size: DVec3::splat(5.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let n = d
        .add_mesh_body("N", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    let (qb, nb) = (
        BodyRef {
            feature: q,
            index: 0,
        },
        BodyRef {
            feature: n,
            index: 0,
        },
    );
    let locked = FeatureError::MeshBodyLocked.to_string();
    assert_eq!(locked, mesh_locked());
    let face = FaceRef {
        origin: n.0,
        role: "Netz".into(),
        hint: DVec3::ZERO,
    };
    let edge = EdgeRef {
        a: face.clone(),
        b: face.clone(),
        hint: DVec3::ZERO,
    };
    let kinds = [
        FeatureKind::Boolean {
            target: qb,
            tool: nb,
            op: BoolOp::Union,
        },
        FeatureKind::Boolean {
            target: nb,
            tool: qb,
            op: BoolOp::Difference,
        },
        FeatureKind::Move {
            body: nb,
            delta: DVec3::X,
        },
        FeatureKind::Verrunden {
            body: nb,
            edges: vec![edge.clone()],
            art: BlendArt::Rundung { radius: 1.0 },
            tangentenkette: false,
        },
        FeatureKind::Sketch(SketchFeature {
            sketch: Sketch::new(Frame::WORLD, "S", Tolerance::default()),
            plane: SketchPlane::OnFace(face),
            copies: vec![],
            formulas: Default::default(),
        }),
    ];
    for kind in kinds {
        let id = d.add_feature("X", kind).unwrap();
        assert_eq!(status(&d, id), FeatureStatus::Error(locked.clone()));
        d.check_invariants().unwrap();
        d.delete(id).unwrap();
    }
    assert_eq!(status(&d, n), FeatureStatus::Ok);
    assert_eq!(status(&d, q), FeatureStatus::Ok);
}

#[test]
fn snapshots_carry_only_used_meshes_and_are_checked_on_load() {
    let mut d = Document::new();
    let a = d.add_mesh_body("A", tetra(), at(0.0)).unwrap();
    let unused = d.add_mesh(tetra()).unwrap();
    let s = d.snapshot();
    assert_eq!(s.meshes.len(), 1);
    assert!(!s.meshes.contains_key(&unused));
    let mut back = Document::from_snapshot(s.clone(), 0.05).unwrap();
    assert_eq!(back.visible_meshes()[0].body.feature, a);
    back.check_invariants().unwrap();
    let next = back.add_mesh(tetra()).unwrap();
    assert!(next.0 > mesh_id(&d, a).0, "Kennungen laufen weiter");
    // fehlendes Netz: Verweis ungültig
    let mut missing = s.clone();
    missing.meshes.clear();
    assert!(matches!(
        Document::from_snapshot(missing, 0.05),
        Err(FeatureError::MeshMissing(_))
    ));
    // kaputtes Netz
    let id = *s.meshes.keys().next().unwrap();
    let mut broken = s.clone();
    broken.meshes.insert(
        id,
        Arc::new(ExchangeMesh {
            positions: vec![[f32::NAN; 3]; 3],
            indices: vec![0, 1, 2],
        }),
    );
    assert!(matches!(
        Document::from_snapshot(broken, 0.05),
        Err(FeatureError::InvalidInput(_))
    ));
    // Kennung 0
    let mut zero = s;
    let m = zero.meshes.remove(&id).unwrap();
    zero.meshes.insert(MeshId(0), m);
    assert!(matches!(
        Document::from_snapshot(zero, 0.05),
        Err(FeatureError::InvalidInput(_))
    ));
}
