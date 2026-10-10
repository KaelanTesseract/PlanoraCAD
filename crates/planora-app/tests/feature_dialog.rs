use planora_app::feature_dialog::FeatureDialog;
use planora_app::state::AppState;
use planora_feature::{FeatureKind, FeatureStatus};
use planora_math::DVec3;

#[test]
fn the_dialog_edits_parameters_and_the_bodies_follow() {
    let mut s = AppState::new();
    let id = s.create_box(DVec3::splat(4.0)).unwrap();
    let mut d = FeatureDialog::new(&s, id).expect("Dialog für Quader");
    d.set_box_size(DVec3::new(5.0, 6.0, 7.0));
    d.apply(&mut s).unwrap();
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Box { size, .. } => assert_eq!(*size, DVec3::new(5.0, 6.0, 7.0)),
        k => panic!("{k:?}"),
    }
    assert!((s.bodies[0].bounds.1.z - s.bodies[0].bounds.0.z - 7.0).abs() < 1e-6);
}

#[test]
fn invalid_values_become_an_error_status_not_a_panic() {
    let mut s = AppState::new();
    let id = s.create_box(DVec3::splat(4.0)).unwrap();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_box_size(DVec3::new(-1.0, 6.0, 7.0));
    let r = d.apply(&mut s);
    assert!(
        r.is_err()
            || matches!(
                s.document.feature(id).unwrap().status,
                FeatureStatus::Error(_)
            )
    );
    assert!(s.bodies.is_empty() || r.is_err());
}

#[test]
fn sketches_have_no_dialog_and_unknown_ids_none() {
    let mut s = AppState::new();
    let sk = s
        .add_sketch(planora_feature::SketchPlane::Standard(
            planora_feature::StandardPlane::XY,
        ))
        .unwrap();
    assert!(FeatureDialog::new(&s, sk).is_none());
    assert!(FeatureDialog::new(&s, planora_feature::FeatureId(99)).is_none());
}

// ---- Zusätzliche Tests (über den Brief hinaus) ----------------------------------------------

#[test]
fn numbers_are_read_with_comma_or_dot_and_bad_text_is_rejected() {
    let mut s = AppState::new();
    let id = s.create_cylinder(1.0, 4.0).unwrap();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    assert!(d.set_text("Radius", "2,5"));
    assert!(d.set_text("Höhe", "6.0"));
    d.apply(&mut s).unwrap();
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Cylinder { radius, height, .. } => {
            assert_eq!((*radius, *height), (2.5, 6.0));
        }
        k => panic!("{k:?}"),
    }
    assert!(d.set_text("Radius", "zwei"));
    let e = d.apply(&mut s).unwrap_err();
    assert!(e.contains("Radius"), "{e}");
    assert!(!d.set_text("Gibt es nicht", "1"));
}

#[test]
fn editing_an_extrusion_changes_its_height_and_the_body_follows() {
    use planora_feature::{SketchPlane, StandardPlane};
    use planora_math::DVec2;
    let mut s = AppState::new();
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::ZERO, DVec2::new(4.0, 3.0))
        .unwrap();
    s.close_sketch();
    let e = s.extrude_sketch(sk, 0, 2.0).unwrap();
    let mut d = FeatureDialog::new(&s, e).unwrap();
    assert!(d.set_text("Höhe", "5"));
    d.apply(&mut s).unwrap();
    let (lo, hi) = s.bodies[0].bounds;
    assert!((hi.z - lo.z - 5.0).abs() < 1e-6, "{lo:?} {hi:?}");
}

#[test]
fn the_boolean_dialog_offers_only_bodies_visible_before_the_feature() {
    use planora_kernel::BoolOp;
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_cylinder(2.0, 14.0).unwrap();
    s.move_body(1, DVec3::new(-17.0, 5.0, -2.0)).unwrap();
    let b = s.add_boolean(0, 1, BoolOp::Union).unwrap();
    s.create_box(DVec3::splat(1.0)).unwrap(); // entsteht danach: keine Auswahl
    let mut d = FeatureDialog::new(&s, b).unwrap();
    assert_eq!(d.body_choices(&s).len(), 2);
    d.set_boolean_op(BoolOp::Difference);
    d.apply(&mut s).unwrap();
    match &s.document.feature(b).unwrap().kind {
        FeatureKind::Boolean { op, .. } => assert_eq!(*op, BoolOp::Difference),
        k => panic!("{k:?}"),
    }
    assert_eq!(s.document.feature(b).unwrap().status, FeatureStatus::Ok);
}

#[test]
fn the_dialog_draws_without_input_and_stays_open() {
    let mut s = AppState::new();
    let id = s.create_box(DVec3::splat(4.0)).unwrap();
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    let mut d = FeatureDialog::new(&s, id).unwrap();
    let mut text = String::new();
    for _ in 0..3 {
        let mut open = false;
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            open = d.show(ui.ctx(), &mut s);
        });
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
        assert!(open);
    }
    for must in ["Breite", "Übernehmen", "Abbrechen", "Quader 1"] {
        assert!(text.contains(must), "„{must}“ fehlt:\n{text}");
    }
}

// ---- Fix-Runde 1 ------------------------------------------------------------------------------

#[test]
fn a_dialog_on_a_changed_feature_refuses_and_reloads() {
    let mut s = AppState::new();
    let id = s.create_box(DVec3::splat(4.0)).unwrap();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    // Das Feature ändert sich, während der Dialog offen ist.
    s.document
        .update(id, |k| {
            if let FeatureKind::Box { size, .. } = k {
                size.z = 9.0;
            }
        })
        .unwrap();
    s.refresh();
    d.set_box_size(DVec3::new(5.0, 6.0, 7.0));
    let e = d.apply(&mut s).unwrap_err();
    assert!(e.contains("inzwischen geändert"), "{e}");
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Box { size, .. } => assert_eq!(size.z, 9.0, "nicht überschrieben"),
        k => panic!("{k:?}"),
    }
    assert!(d.reload(&s));
    d.set_text("Breite", "5");
    d.apply(&mut s).unwrap();
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Box { size, .. } => assert_eq!(*size, DVec3::new(5.0, 4.0, 9.0)),
        k => panic!("{k:?}"),
    }
}

#[test]
fn the_move_dialog_edits_the_delta() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    let x0 = s.bodies[0].bounds.0.x;
    s.move_body(0, DVec3::X).unwrap();
    let m = s.bodies[0].source.feature;
    let mut d = FeatureDialog::new(&s, m).unwrap();
    assert!(d.set_text("Δx", "2,5"));
    assert!(d.set_text("Δz", "-1"));
    d.apply(&mut s).unwrap();
    match &s.document.feature(m).unwrap().kind {
        FeatureKind::Move { delta, .. } => assert_eq!(*delta, DVec3::new(2.5, 0.0, -1.0)),
        k => panic!("{k:?}"),
    }
    assert!((s.bodies[0].bounds.0.x - (x0 + 2.5)).abs() < 1e-9);
    assert!(!d.set_text("Höhe", "1"), "kein Höhenfeld beim Verschieben");
}

#[test]
fn the_revolve_dialog_offers_the_axis_and_applies() {
    use planora_feature::{SketchPlane, StandardPlane};
    use planora_math::DVec2;
    let mut s = AppState::new();
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    let axis = {
        let sketch = s.session_sketch_mut().unwrap().1;
        sketch
            .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(5.0, 10.0))
            .unwrap();
        let a = sketch.add_point(0.0, -2.0).unwrap();
        let b = sketch.add_point(0.0, 12.0).unwrap();
        let axis = sketch.add_line(a, b).unwrap();
        sketch.set_construction(axis, true).unwrap();
        axis
    };
    s.close_sketch();
    let r = s.revolve_sketch(sk, 0, axis).unwrap();
    let mut d = FeatureDialog::new(&s, r).expect("Dialog für Rotation");
    assert!(!d.set_text("Höhe", "1"), "keine Zahlenfelder");
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            d.show(ui.ctx(), &mut s);
        });
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    for must in ["Skizze 1", "Profil 1", "Drehachse", "Konstruktionslinie 1"] {
        assert!(text.contains(must), "„{must}“ fehlt:\n{text}");
    }
    d.apply(&mut s).unwrap();
    assert_eq!(s.document.feature(r).unwrap().status, FeatureStatus::Ok);
    assert_eq!(s.bodies.len(), 1);
}
