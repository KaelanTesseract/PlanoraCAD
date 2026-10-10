use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::DVec3;

fn sample() -> Document {
    let mut d = Document::new();
    let a = d
        .add_feature(
            "A",
            FeatureKind::Box {
                size: DVec3::new(10.0, 10.0, 10.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let b = d
        .add_feature(
            "B",
            FeatureKind::Cylinder {
                radius: 3.0,
                height: 12.0,
                origin: DVec3::new(5.0, 5.0, -1.0),
            },
        )
        .unwrap();
    d.add_feature(
        "C",
        FeatureKind::Boolean {
            target: BodyRef {
                feature: a,
                index: 0,
            },
            tool: BodyRef {
                feature: b,
                index: 0,
            },
            op: BoolOp::Difference,
        },
    )
    .unwrap();
    d
}

fn shape(d: &Document) -> Vec<(String, bool, Option<usize>)> {
    d.features()
        .iter()
        .map(|f| (f.name.clone(), f.suppressed, None))
        .collect()
}

#[test]
fn a_snapshot_rebuilds_an_equivalent_document() {
    let mut d = sample();
    let idx = d.features()[1].id;
    d.set_suppressed(idx, true).unwrap();
    d.set_marker(3).unwrap();
    let s = d.snapshot();
    let e = Document::from_snapshot(s, 0.05).unwrap();
    assert_eq!(shape(&d), shape(&e));
    assert_eq!(d.marker(), e.marker());
    assert_eq!(e.edit_count(), 0);
    e.check_invariants().unwrap();
    for (x, y) in d.features().iter().zip(e.features()) {
        assert_eq!(x.id, y.id);
        assert_eq!(x.status, y.status);
    }
    // neue Ids setzen die alte Zählung fort
    let mut e = e;
    let id = e
        .add_feature(
            "D",
            FeatureKind::Box {
                size: DVec3::ONE,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    assert!(d.features().iter().all(|f| f.id != id));
}

#[test]
fn every_public_edit_raises_the_edit_count() {
    let mut d = sample();
    let a = d.features()[0].id;
    let mut last = d.edit_count();
    let mut bumped = |d: &Document, what: &str| {
        let now = d.edit_count();
        assert!(now > last, "{what} muss den Zähler erhöhen");
        last = now;
    };
    d.rename(a, "A2").unwrap();
    bumped(&d, "rename");
    d.update(a, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.x = 11.0
        }
    })
    .unwrap();
    bumped(&d, "update");
    d.set_suppressed(a, true).unwrap();
    bumped(&d, "set_suppressed");
    d.set_marker(1).unwrap();
    bumped(&d, "set_marker");
    d.set_marker(3).unwrap();
    d.add_feature(
        "N",
        FeatureKind::Box {
            size: DVec3::ONE,
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    bumped(&d, "add_feature");
    let n = d.features().last().unwrap().id;
    d.delete(n).unwrap();
    bumped(&d, "delete");
    let c = d.features()[2].id;
    d.delete_with_dependents(c).unwrap();
    bumped(&d, "delete_with_dependents");
    // keine Änderung → kein Anstieg
    let before = d.edit_count();
    d.set_marker(d.marker()).unwrap();
    d.recompute();
    assert_eq!(d.edit_count(), before);
}

fn snap_with(f: impl FnOnce(&mut Snapshot)) -> FeatureError {
    let mut s = sample().snapshot();
    f(&mut s);
    Document::from_snapshot(s, 0.05).unwrap_err()
}

#[test]
fn inconsistent_snapshots_are_rejected() {
    snap_with(|s| s.features[1].id = s.features[0].id);
    snap_with(|s| s.features[0].id = FeatureId(0));
    snap_with(|s| s.next_id = 2);
    snap_with(|s| s.marker = 99);
    snap_with(|s| s.features[0].name = "  ".into());
    // Boolesche Operation verweist auf ein späteres Feature
    snap_with(|s| s.features.swap(0, 2));
    snap_with(|s| {
        if let FeatureKind::Boolean { target, .. } = &mut s.features[2].kind {
            target.feature = FeatureId(77);
        }
    });
}

#[test]
fn a_snapshot_with_huge_numbers_loads_with_an_error_status_not_a_panic() {
    let mut s = sample().snapshot();
    s.features[0].kind = FeatureKind::Box {
        size: DVec3::splat(1e300),
        origin: DVec3::ZERO,
    };
    let d = Document::from_snapshot(s, 0.05).unwrap();
    assert!(matches!(d.features()[0].status, FeatureStatus::Error(_)));
    d.check_invariants().unwrap();
}
