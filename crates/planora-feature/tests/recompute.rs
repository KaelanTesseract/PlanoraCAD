use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::DVec3;

fn boxed(d: &mut Document, name: &str) -> FeatureId {
    d.add_feature(
        name,
        FeatureKind::Box {
            size: DVec3::splat(4.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap()
}

#[test]
fn a_second_run_without_changes_computes_nothing() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let b = boxed(&mut d, "B");
    d.add_feature(
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
    let report = d.recompute();
    assert!(report.computed.is_empty(), "{:?}", report.computed);
    assert_eq!(report.reused.len(), 3);
}

#[test]
fn errors_skip_dependents_but_not_independent_features() {
    let mut d = Document::new();
    let bad = d.add_feature(
        "Zu groß",
        FeatureKind::Cylinder {
            radius: -1.0,
            height: 2.0,
            origin: DVec3::ZERO,
        },
    );
    // der Kernel lehnt den Zylinder ab; das Feature wird angelegt und trägt den Fehler
    let bad = bad.unwrap();
    assert!(matches!(
        d.feature(bad).unwrap().status,
        FeatureStatus::Error(_)
    ));
    let ok = boxed(&mut d, "Quader");
    let dep = d
        .add_feature(
            "Verschieben",
            FeatureKind::Move {
                body: BodyRef {
                    feature: bad,
                    index: 0,
                },
                delta: DVec3::X,
            },
        )
        .unwrap();
    assert_eq!(d.feature(ok).unwrap().status, FeatureStatus::Ok);
    assert!(matches!(
        d.feature(dep).unwrap().status,
        FeatureStatus::Skipped(_)
    ));
    assert_eq!(d.visible_bodies().len(), 1, "nur der Quader");
}

#[test]
fn suppressing_a_feature_skips_its_dependents() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let b = boxed(&mut d, "B");
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
    d.set_suppressed(a, true).unwrap();
    assert_eq!(d.feature(a).unwrap().status, FeatureStatus::Suppressed);
    assert!(
        matches!(&d.feature(u).unwrap().status, FeatureStatus::Skipped(m) if m.contains("unterdrückt")),
        "{:?}",
        d.feature(u).unwrap().status
    );
    let names: Vec<String> = d.visible_bodies().into_iter().map(|v| v.name).collect();
    assert_eq!(names, ["B"]);
    d.set_suppressed(a, false).unwrap();
    assert_eq!(d.feature(u).unwrap().status, FeatureStatus::Ok);
    assert_eq!(d.visible_bodies().len(), 1);
}

#[test]
fn a_boolean_with_a_consumed_body_reports_it() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let b = boxed(&mut d, "B");
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
    let again = d
        .add_feature(
            "V",
            FeatureKind::Move {
                body: BodyRef {
                    feature: a,
                    index: 0,
                },
                delta: DVec3::X,
            },
        )
        .unwrap();
    assert!(
        matches!(&d.feature(again).unwrap().status, FeatureStatus::Error(m) if m.contains("nicht")),
        "{:?}",
        d.feature(again).unwrap().status
    );
    assert_eq!(d.feature(u).unwrap().status, FeatureStatus::Ok);
}

#[test]
fn the_marker_hides_later_features_and_inserts_at_its_position() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let b = boxed(&mut d, "B");
    d.set_marker(1).unwrap();
    assert_eq!(d.feature(b).unwrap().status, FeatureStatus::NotComputed);
    assert_eq!(d.visible_bodies().len(), 1);
    let c = boxed(&mut d, "C");
    assert_eq!(d.index_of(c), Some(1), "wird am Marker eingefügt");
    assert_eq!(d.index_of(b), Some(2));
    assert_eq!(d.marker(), 2);
    assert_eq!(d.feature(a).unwrap().status, FeatureStatus::Ok);
    assert!(d.set_marker(99).is_err());
    d.set_marker(3).unwrap();
    assert_eq!(d.visible_bodies().len(), 3);
}
