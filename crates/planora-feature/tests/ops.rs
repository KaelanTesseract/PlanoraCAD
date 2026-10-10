use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::DVec3;

fn boxed(d: &mut Document, name: &str, size: f64) -> FeatureId {
    d.add_feature(
        name,
        FeatureKind::Box {
            size: DVec3::splat(size),
            origin: DVec3::ZERO,
        },
    )
    .unwrap()
}

#[test]
fn update_changes_parameters_and_recomputes_only_the_followers() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let b = boxed(&mut d, "B", 4.0);
    let u = d
        .add_feature(
            "U",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: a,
                    index: 0,
                },
                tool: BodyRef {
                    feature: b,
                    index: 0,
                },
                op: BoolOp::Union,
            },
        )
        .unwrap();
    // B ändern → B und U neu, A nicht
    let before = d.recompute();
    assert!(before.computed.is_empty());
    d.update(b, |k| {
        if let FeatureKind::Box { size, .. } = k {
            *size = DVec3::splat(6.0);
        }
    })
    .unwrap();
    // recompute lief schon in update; ein weiterer Lauf ändert nichts mehr
    assert!(d.recompute().computed.is_empty());
    assert_eq!(d.feature(u).unwrap().status, FeatureStatus::Ok);
}

#[test]
fn update_count_of_recomputed_features_is_exactly_the_followers() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let b = boxed(&mut d, "B", 4.0);
    let _c = boxed(&mut d, "C", 4.0);
    let u = d
        .add_feature(
            "U",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: a,
                    index: 0,
                },
                tool: BodyRef {
                    feature: b,
                    index: 0,
                },
                op: BoolOp::Union,
            },
        )
        .unwrap();
    let report = d
        .update_with_report(b, |k| {
            if let FeatureKind::Box { size, .. } = k {
                *size = DVec3::splat(5.0);
            }
        })
        .unwrap();
    let mut ids = report.computed.clone();
    ids.sort();
    assert_eq!(ids, vec![b, u], "nur B und das davon abhängige U");
    assert_eq!(report.reused.len(), 2, "A und C");
}

#[test]
fn update_rejects_variant_changes_and_forward_references_without_touching_the_document() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let m = d
        .add_feature(
            "M",
            FeatureKind::Move {
                body: BodyRef {
                    feature: a,
                    index: 0,
                },
                delta: DVec3::X,
            },
        )
        .unwrap();
    let r = d.update(a, |k| {
        *k = FeatureKind::Sphere {
            radius: 1.0,
            origin: DVec3::ZERO,
        }
    });
    assert!(matches!(r, Err(FeatureError::InvalidInput(_))));
    let r = d.update(a, |k| {
        if let FeatureKind::Box { size, .. } = k {
            *size = DVec3::splat(2.0);
        }
    });
    assert!(r.is_ok());
    let r = d.update(m, |k| {
        if let FeatureKind::Move { body, .. } = k {
            body.feature = FeatureId(99);
        }
    });
    assert!(matches!(r, Err(FeatureError::UnknownFeature)));
    let r = d.update(a, |_| {});
    assert!(r.is_ok());
    assert!(d.update(FeatureId(77), |_| {}).is_err());
}

#[test]
fn rename_and_dependents_and_delete_with_consequences() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let b = boxed(&mut d, "B", 4.0);
    let u = d
        .add_feature(
            "U",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: a,
                    index: 0,
                },
                tool: BodyRef {
                    feature: b,
                    index: 0,
                },
                op: BoolOp::Union,
            },
        )
        .unwrap();
    let m = d
        .add_feature(
            "M",
            FeatureKind::Move {
                body: BodyRef {
                    feature: u,
                    index: 0,
                },
                delta: DVec3::X,
            },
        )
        .unwrap();
    d.rename(a, "Grundplatte").unwrap();
    assert_eq!(d.feature(a).unwrap().name, "Grundplatte");
    assert!(d.rename(a, "").is_err());
    assert_eq!(d.dependents(a), vec![u, m]);
    assert_eq!(d.dependents(m), Vec::<FeatureId>::new());
    match d.delete(a) {
        Err(FeatureError::HasDependents(ids)) => assert_eq!(ids, vec![u.0, m.0]),
        other => panic!("{other:?}"),
    }
    assert_eq!(d.features().len(), 4, "unverändert");
    d.delete(m).unwrap();
    let removed = d.delete_with_dependents(a).unwrap();
    assert_eq!(removed, vec![a, u]);
    assert_eq!(d.features().len(), 1);
    assert_eq!(d.marker(), 1);
    d.check_invariants().unwrap();
    assert_eq!(d.visible_bodies().len(), 1);
}

#[test]
fn inserting_at_the_marker_only_allows_earlier_references() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let b = boxed(&mut d, "B", 4.0);
    d.set_marker(1).unwrap();
    let r = d.add_feature(
        "M",
        FeatureKind::Move {
            body: BodyRef {
                feature: b,
                index: 0,
            },
            delta: DVec3::X,
        },
    );
    assert!(matches!(r, Err(FeatureError::ForwardReference)), "{r:?}");
    let m = d
        .add_feature(
            "M",
            FeatureKind::Move {
                body: BodyRef {
                    feature: a,
                    index: 0,
                },
                delta: DVec3::X,
            },
        )
        .unwrap();
    assert_eq!(d.index_of(m), Some(1));
    d.check_invariants().unwrap();
}
