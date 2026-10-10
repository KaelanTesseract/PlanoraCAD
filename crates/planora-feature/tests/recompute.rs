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
    let report = d.set_suppressed_with_report(a, true).unwrap();
    assert!(report.computed.is_empty(), "{:?}", report.computed);
    assert_eq!(report.reused, vec![b], "A unterdrückt, U ausgesetzt");
    assert_eq!(d.feature(a).unwrap().status, FeatureStatus::Suppressed);
    assert!(
        matches!(&d.feature(u).unwrap().status, FeatureStatus::Skipped(m) if m.contains("unterdrückt")),
        "{:?}",
        d.feature(u).unwrap().status
    );
    let names: Vec<String> = d.visible_bodies().into_iter().map(|v| v.name).collect();
    assert_eq!(names, ["B"]);
    let report = d.set_suppressed_with_report(a, false).unwrap();
    assert_eq!(report.computed, vec![a, u]);
    assert_eq!(report.reused, vec![b]);
    assert_eq!(d.feature(u).unwrap().status, FeatureStatus::Ok);
    assert_eq!(d.visible_bodies().len(), 1);
}

#[test]
fn setting_the_current_suppression_state_changes_nothing() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let report = d.set_suppressed_with_report(a, false).unwrap();
    assert!(
        report.computed.is_empty() && report.reused.is_empty(),
        "{report:?}"
    );
    d.set_suppressed(a, true).unwrap();
    let report = d.set_suppressed_with_report(a, true).unwrap();
    assert!(
        report.computed.is_empty() && report.reused.is_empty(),
        "{report:?}"
    );
    assert_eq!(d.feature(a).unwrap().status, FeatureStatus::Suppressed);
    assert!(d.set_suppressed(FeatureId(99), true).is_err());
}

#[test]
fn suppressing_an_unrelated_box_recomputes_only_readers_of_the_visible_bodies() {
    use planora_math::{DVec2, Frame, Tolerance};
    use planora_sketch::model::Sketch;
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let b = boxed(&mut d, "B");
    let c = d
        .add_feature(
            "C",
            FeatureKind::Box {
                size: DVec3::ONE,
                origin: DVec3::new(20.0, 0.0, 0.0),
            },
        )
        .unwrap();
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
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    sk.add_rectangle(DVec2::new(30.0, 0.0), DVec2::new(35.0, 5.0))
        .unwrap();
    let s = d
        .add_feature(
            "S",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
            }),
        )
        .unwrap();
    let e = d
        .add_feature(
            "E",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 2.0,
            },
        )
        .unwrap();
    for id in [a, b, c, u, s, e] {
        assert_eq!(d.feature(id).unwrap().status, FeatureStatus::Ok, "{id:?}");
    }
    let report = d.set_suppressed_with_report(c, true).unwrap();
    assert_eq!(
        report.computed,
        vec![u],
        "die Boolesche Operation liest die sichtbaren Körper"
    );
    assert_eq!(
        report.reused,
        vec![a, b, s, e],
        "die Extrusion liest nur ihre Skizze"
    );
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

fn body(feature: FeatureId) -> BodyRef {
    BodyRef { feature, index: 0 }
}

#[test]
fn visible_bodies_before_shows_the_inputs_before_a_boolean_and_only_the_result_after_it() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let b = d
        .add_feature(
            "B",
            FeatureKind::Box {
                size: DVec3::splat(4.0),
                origin: DVec3::new(2.0, 0.0, 0.0),
            },
        )
        .unwrap();
    let u = d
        .add_feature(
            "U",
            FeatureKind::Boolean {
                target: body(a),
                tool: body(b),
                op: BoolOp::Union,
            },
        )
        .unwrap();
    let c = boxed(&mut d, "C");
    let before_u: Vec<BodyRef> = d.visible_bodies_before(u).iter().map(|v| v.body).collect();
    assert_eq!(before_u, vec![body(a), body(b)]);
    let before_c: Vec<BodyRef> = d.visible_bodies_before(c).iter().map(|v| v.body).collect();
    assert_eq!(before_c, vec![body(u)]);
    assert!(d.visible_bodies_before(a).is_empty());
    assert!(
        d.visible_bodies_before(FeatureId(999)).is_empty(),
        "unbekannt"
    );
    assert_eq!(d.visible_bodies_before(c)[0].name, "U");
}

#[test]
fn results_of_moves_and_booleans_take_the_place_of_their_inputs() {
    let mut d = Document::new();
    let a = boxed(&mut d, "A");
    let b = boxed(&mut d, "B");
    let c = boxed(&mut d, "C");
    let m = d
        .add_feature(
            "M",
            FeatureKind::Move {
                body: body(a),
                delta: DVec3::X,
            },
        )
        .unwrap();
    let order: Vec<BodyRef> = d.visible_bodies().iter().map(|v| v.body).collect();
    assert_eq!(
        order,
        vec![body(m), body(b), body(c)],
        "Move bleibt an der Stelle"
    );
    // Ziel hinten, Werkzeug vorn: das Ergebnis steht an der vorderen Stelle
    let u = d
        .add_feature(
            "U",
            FeatureKind::Boolean {
                target: body(c),
                tool: body(b),
                op: BoolOp::Union,
            },
        )
        .unwrap();
    assert_eq!(d.feature(u).unwrap().status, FeatureStatus::Ok);
    let order: Vec<BodyRef> = d.visible_bodies().iter().map(|v| v.body).collect();
    assert_eq!(order, vec![body(m), body(u)]);
}

#[test]
fn a_failed_feature_keeps_its_error_without_recomputing_until_its_input_changes() {
    let mut d = Document::new();
    let z = d
        .add_feature(
            "Z",
            FeatureKind::Cylinder {
                radius: -1.0,
                height: 4.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    assert!(matches!(
        d.feature(z).unwrap().status,
        FeatureStatus::Error(_)
    ));
    let again = d.recompute();
    assert!(!again.computed.contains(&z), "{:?}", again.computed);
    assert!(
        again.reused.contains(&z),
        "übernommener Fehler steht in reused"
    );
    assert!(matches!(
        d.feature(z).unwrap().status,
        FeatureStatus::Error(_)
    ));
    let fixed = d
        .update_with_report(z, |k| {
            if let FeatureKind::Cylinder { radius, .. } = k {
                *radius = 1.0;
            }
        })
        .unwrap();
    assert_eq!(d.feature(z).unwrap().status, FeatureStatus::Ok);
    assert!(fixed.computed.contains(&z), "{:?}", fixed.computed);
    d.check_invariants().unwrap();
}
