use planora_app::state::{AppState, BodyKind};
use planora_feature::{FeatureKind, FeatureStatus};
use planora_kernel::BoolOp;
use planora_math::DVec3;
use planora_mesh::{TessellationOptions, mass_properties, tessellate};

fn volume(s: &AppState, i: usize) -> f64 {
    mass_properties(
        &tessellate(
            &s.bodies[i].solid,
            &TessellationOptions {
                chord_tolerance: 1e-3,
            },
        )
        .unwrap(),
    )
    .volume
}

#[test]
fn primitives_become_features_and_stand_side_by_side() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(10.0)).unwrap();
    let b = s.create_box(DVec3::splat(10.0)).unwrap();
    assert_ne!(a, b);
    assert_eq!(s.document.features().len(), 2);
    assert_eq!(s.bodies.len(), 2);
    assert_eq!(s.bodies[0].name, "Quader 1");
    assert_eq!(s.bodies[1].name, "Quader 2");
    assert!(
        s.bodies[1].bounds.0.x > s.bodies[0].bounds.1.x,
        "nebeneinander"
    );
    assert_eq!(s.bodies[0].kind, BodyKind::Box);
    s.create_cylinder(2.0, 5.0).unwrap();
    s.create_sphere(3.0).unwrap();
    s.create_cone(2.0, 4.0).unwrap();
    assert_eq!(s.bodies.len(), 5);
    assert_eq!(s.bodies[2].name, "Zylinder 1");
}

#[test]
fn a_boolean_replaces_its_inputs_and_errors_leave_the_document_alone() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_cylinder(2.0, 14.0).unwrap();
    // Zylinder in den Quader schieben. Der Plan hatte (-7, 5, -2); das setzt voraus, dass der
    // Zylinder ohne Abstand am Quader steht. Mit den 10 mm Abstand der Vorgabe (Mitte bei
    // x = 10 + 10 + 2 = 22) führt (-17, 5, -2) ihn in die Mitte des Quaders (Testtippfehler).
    s.move_body(1, DVec3::new(-17.0, 5.0, -2.0)).unwrap();
    let features_before = s.document.features().len();
    assert!(s.add_boolean(0, 0, BoolOp::Union).is_err());
    assert!(s.add_boolean(0, 9, BoolOp::Union).is_err());
    assert_eq!(s.document.features().len(), features_before);
    let f = s.add_boolean(0, 1, BoolOp::Difference).unwrap();
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(s.selected_body, Some(0));
    assert_eq!(s.bodies[0].source.feature, f);
    assert!((volume(&s, 0) - (1000.0 - std::f64::consts::PI * 4.0 * 10.0)).abs() < 0.6);
    assert_eq!(s.bodies[0].kind, BodyKind::Combined);
}

#[test]
fn moving_a_body_edits_the_last_move_feature_instead_of_stacking_new_ones() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    let x0 = s.bodies[0].bounds.0.x;
    s.move_body(0, DVec3::new(5.0, 0.0, 0.0)).unwrap();
    assert_eq!(s.document.features().len(), 2);
    s.move_body(0, DVec3::new(2.0, 0.0, 0.0)).unwrap();
    assert_eq!(s.document.features().len(), 2, "kein weiteres Feature");
    assert!((s.bodies[0].bounds.0.x - (x0 + 7.0)).abs() < 1e-6);
    match &s.document.features()[1].kind {
        FeatureKind::Move { delta, .. } => assert!((delta.x - 7.0).abs() < 1e-9),
        k => panic!("{k:?}"),
    }
    assert!(s.move_body(0, DVec3::new(f64::NAN, 0.0, 0.0)).is_err());
    assert!(s.move_body(5, DVec3::X).is_err());
    assert_eq!(s.document.features().len(), 2);
}

#[test]
fn moving_after_a_foreign_feature_read_the_body_adds_a_new_move() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    s.move_body(0, DVec3::X).unwrap(); // Move für Körper 0 (Feature 3)
    let _ = s.add_boolean(0, 1, BoolOp::Union); // liest beide (ob erfolgreich oder nicht)
    let n = s.document.features().len();
    s.move_body(0, DVec3::Y).unwrap();
    assert!(
        s.document.features().len() >= n,
        "ein weiteres Feature entsteht oder der letzte Move wird erweitert, nie beides verloren"
    );
}

#[test]
fn editing_the_document_and_refreshing_updates_the_bodies_and_keeps_the_selection() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::splat(4.0)).unwrap();
    s.selected_body = Some(0);
    s.document
        .update(b, |k| {
            if let FeatureKind::Box { size, .. } = k {
                *size = DVec3::splat(6.0);
            }
        })
        .unwrap();
    s.refresh();
    assert!((volume(&s, 0) - 216.0).abs() < 1e-6);
    assert_eq!(s.selected_body, Some(0));
}

#[test]
fn deleting_features_removes_their_bodies_and_reports_dependents() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    let _ = s.add_boolean(0, 1, BoolOp::Union);
    let e = s.delete_feature(a).unwrap_err();
    assert!(e.contains("Quader 1") || e.contains("Boolesche"), "{e}");
    let removed = s.delete_feature_with_dependents(a).unwrap();
    assert!(!removed.is_empty());
    assert!(s.selected_body.is_none() || s.selected_body.unwrap() < s.bodies.len());
    for f in s.document.features() {
        assert!(!matches!(f.status, FeatureStatus::Error(_)), "{f:?}");
    }
}

#[test]
fn rename_goes_through_the_feature() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    s.rename_feature(a, "Grundplatte").unwrap();
    assert_eq!(s.bodies[0].name, "Grundplatte");
    assert!(s.rename_feature(a, "  ").is_err());
}
