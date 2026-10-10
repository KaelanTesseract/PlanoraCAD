use planora_feature::*;
use planora_kernel::*;
use planora_math::{DMat4, DVec3, Tolerance};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn tol() -> Tolerance {
    Tolerance::default()
}

fn tagged_box(size: DVec3, origin: u64) -> Solid {
    let mut s = k().make_box(size).unwrap();
    s.assign_origin(origin);
    s
}

fn face_with_role(s: &Solid, role: &str) -> Id<Face> {
    s.faces
        .iter()
        .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == role))
        .map(|(i, _)| i)
        .unwrap()
}

#[test]
fn a_face_reference_is_found_again_after_the_body_moved() {
    let a = tagged_box(DVec3::new(4.0, 6.0, 8.0), 5);
    let r = FaceRef::from_face(&a, face_with_role(&a, "Deckel"), &tol()).unwrap();
    assert_eq!((r.origin, r.role.as_str()), (5, "Deckel"));
    let moved = k()
        .transform(&a, &DMat4::from_translation(DVec3::new(10.0, 0.0, 0.0)))
        .unwrap();
    let hit = resolve_face(&[&moved], &r, &tol()).unwrap();
    assert_eq!(hit.body, 0);
    assert_eq!(moved.faces[hit.face].tag.as_ref().unwrap().role, "Deckel");
    assert!(
        hit.hint.x > 5.0,
        "der Hinweispunkt ist mitgewandert: {:?}",
        hit.hint
    );
}

#[test]
fn missing_untagged_and_unassigned_faces_are_errors() {
    let a = tagged_box(DVec3::splat(3.0), 5);
    let r = FaceRef::from_face(&a, face_with_role(&a, "Deckel"), &tol()).unwrap();
    let other = tagged_box(DVec3::splat(3.0), 6);
    assert!(matches!(
        resolve_face(&[&other], &r, &tol()),
        Err(FeatureError::FaceNotFound { origin: 5, .. })
    ));
    assert!(matches!(
        resolve_face(&[], &r, &tol()),
        Err(FeatureError::FaceNotFound { .. })
    ));
    let unassigned = k().make_box(DVec3::splat(3.0)).unwrap(); // origin == 0
    assert!(matches!(
        FaceRef::from_face(&unassigned, face_with_role(&unassigned, "Deckel"), &tol()),
        Err(FeatureError::UntaggedFace)
    ));
    assert!(matches!(
        FaceRef::from_face(&a, Id::from_index(99), &tol()),
        Err(FeatureError::UntaggedFace | FeatureError::InvalidInput(_))
    ));
}

#[test]
fn the_right_body_is_chosen_among_several() {
    let a = tagged_box(DVec3::splat(2.0), 1);
    let b = tagged_box(DVec3::splat(2.0), 2);
    let r = FaceRef::from_face(&b, face_with_role(&b, "Boden"), &tol()).unwrap();
    let hit = resolve_face(&[&a, &b], &r, &tol()).unwrap();
    assert_eq!(hit.body, 1);
}

#[test]
fn a_split_face_is_resolved_by_its_hint() {
    let mut plate = k().make_box(DVec3::new(20.0, 20.0, 5.0)).unwrap();
    plate.assign_origin(1);
    let mut groove = k().make_box(DVec3::new(4.0, 30.0, 2.0)).unwrap();
    groove = k()
        .transform(
            &groove,
            &DMat4::from_translation(DVec3::new(8.0, -5.0, 3.0)),
        )
        .unwrap();
    groove.assign_origin(2);
    let cut = k().boolean(&plate, &groove, BoolOp::Difference).unwrap();
    let tag = FaceTag {
        origin: 1,
        role: "Deckel".into(),
    };
    let pieces = cut.faces_with_tag(&tag);
    assert_eq!(pieces.len(), 2);
    let probes: Vec<DVec3> = pieces
        .iter()
        .map(|&f| face_probe_point(&cut, f, &tol()).unwrap())
        .collect();
    for (i, p) in probes.iter().enumerate() {
        let r = FaceRef {
            origin: 1,
            role: "Deckel".into(),
            hint: *p + DVec3::new(0.01, 0.0, 0.0),
        };
        let hit = resolve_face(&[&cut], &r, &tol()).unwrap();
        assert_eq!(hit.face, pieces[i], "Hinweis {i}");
    }
}

#[test]
fn edge_references_use_the_two_adjacent_faces() {
    let a = tagged_box(DVec3::new(4.0, 6.0, 8.0), 3);
    let (ea, _) = a
        .edges
        .iter()
        .find(|(id, _)| {
            let r = EdgeRef::from_edge(&a, *id, &tol());
            r.is_ok_and(|r| {
                let roles = [r.a.role.as_str(), r.b.role.as_str()];
                roles.contains(&"Deckel") && roles.contains(&"Wand 1")
            })
        })
        .expect("die Kante zwischen Deckel und Wand 1");
    let r = EdgeRef::from_edge(&a, ea, &tol()).unwrap();
    let moved = k()
        .transform(&a, &DMat4::from_translation(DVec3::new(0.0, 0.0, 7.0)))
        .unwrap();
    let hit = resolve_edge(&[&moved], &r, &tol()).unwrap();
    let p = edge_probe_point(&moved, hit.edge).unwrap();
    let p0 = edge_probe_point(&a, ea).unwrap();
    assert!((p - p0 - DVec3::new(0.0, 0.0, 7.0)).length() < 1e-9);
}

#[test]
fn seam_edges_reference_the_same_face_twice() {
    let mut c = k().make_cylinder(3.0, 5.0).unwrap();
    c.assign_origin(4);
    let seam = c
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, Curve::Line { .. }))
        .map(|(i, _)| i)
        .expect("Naht");
    let r = EdgeRef::from_edge(&c, seam, &tol()).unwrap();
    assert_eq!(r.a.tag(), r.b.tag());
    let hit = resolve_edge(&[&c], &r, &tol()).unwrap();
    assert_eq!(hit.edge, seam);
    let missing = tagged_box(DVec3::splat(1.0), 9);
    assert!(matches!(
        resolve_edge(&[&missing], &r, &tol()),
        Err(FeatureError::EdgeNotFound { .. })
    ));
}

#[test]
fn feature_kinds_report_their_dependencies_and_labels() {
    let a = FeatureId(3);
    let b = FeatureId(5);
    let k1 = FeatureKind::Boolean {
        target: BodyRef {
            feature: b,
            index: 0,
        },
        tool: BodyRef {
            feature: a,
            index: 0,
        },
        op: BoolOp::Union,
    };
    assert_eq!(k1.dependencies(), vec![a, b]);
    assert_eq!(k1.label(), "Boolesche Operation");
    let k2 = FeatureKind::Box {
        size: DVec3::ONE,
        origin: DVec3::ZERO,
    };
    assert!(k2.dependencies().is_empty());
    let f = StandardPlane::XZ.frame();
    assert!((f.x.cross(f.y) - f.z).length() < 1e-12);
    assert!((f.z - DVec3::new(0.0, -1.0, 0.0)).length() < 1e-12);
}

#[test]
fn a_new_document_is_empty() {
    let d = Document::new();
    assert!(d.features().is_empty());
    assert_eq!(d.marker(), 0);
    assert!(d.feature(FeatureId(1)).is_none());
}
