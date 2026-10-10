use planora_app::state::*;
use planora_feature::FeatureKind;
use planora_kernel::boolean::BoolOp;
use planora_math::DVec3;

fn state_with_box_and_cylinder() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_cylinder(2.0, 14.0).unwrap();
    s
}

/// Quader mit eigenem Namen genau an `origin` (ohne Platzieren neben den anderen).
fn box_at(s: &mut AppState, name: &str, size: DVec3, origin: DVec3) {
    s.document
        .add_feature(name, FeatureKind::Box { size, origin })
        .unwrap();
    s.refresh();
}

#[test]
fn translating_a_body_moves_bounds_and_scene() {
    let mut s = state_with_box_and_cylinder();
    let before = s.bodies[1].bounds;
    let v = s.scene_version;
    s.move_body(1, DVec3::new(1.0, 2.0, 3.0)).unwrap();
    let after = s.bodies[1].bounds;
    assert!((after.0 - before.0 - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-6);
    assert!(s.scene_version > v);
    assert!(s.move_body(9, DVec3::ZERO).is_err());
}

#[test]
fn boolean_replaces_both_bodies_with_the_result() {
    let mut s = state_with_box_and_cylinder();
    // Zylinder mittig durch den Quader legen
    let b = s.bodies[0].bounds;
    let c = s.bodies[1].bounds;
    let target = DVec3::new(5.0, 5.0, -2.0) + b.0;
    s.move_body(1, target - c.0 - DVec3::new(2.0, 2.0, 0.0))
        .unwrap();
    let f = s.add_boolean(0, 1, BoolOp::Difference).unwrap();
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(s.bodies[0].source.feature, f);
    assert_eq!(s.selected_body, Some(0));
    let vol = s.bodies[0].mass.volume;
    assert!(
        (vol - (1000.0 - std::f64::consts::PI * 4.0 * 10.0)).abs() / 1000.0 < 0.01,
        "{vol}"
    );
    assert!(s.bodies[0].name.contains("Abzug") || s.bodies[0].name.contains("Quader"));
}

#[test]
fn boolean_errors_are_german_messages_and_keep_the_bodies() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let features = s.document.features().len();
    // die Körper liegen nebeneinander: Schneiden ist leer
    let err = s.add_boolean(0, 1, BoolOp::Intersection).unwrap_err();
    assert!(err.to_lowercase().contains("leer"), "{err}");
    assert_eq!(s.bodies.len(), 2);
    assert_eq!(
        s.document.features().len(),
        features,
        "Feature wieder entfernt"
    );
    assert!(
        s.add_boolean(0, 0, BoolOp::Union).is_err(),
        "A und B müssen verschieden sein"
    );
    assert!(s.add_boolean(0, 7, BoolOp::Union).is_err());
}

#[test]
fn unsupported_pairs_report_a_clear_message() {
    // Ohne Drehen gibt es keinen gekippten Zylinder mehr; ein Kegel, dessen Achse parallel
    // neben der des Zylinders liegt, ergibt ebenfalls ein nicht unterstütztes Flächenpaar.
    let mut s = AppState::new();
    s.document
        .add_feature(
            "A",
            FeatureKind::Cylinder {
                radius: 2.0,
                height: 10.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    s.document
        .add_feature(
            "B",
            FeatureKind::Cone {
                radius: 2.0,
                height: 4.0,
                origin: DVec3::new(2.0, 0.0, 3.0),
            },
        )
        .unwrap();
    s.refresh();
    assert_eq!(s.bodies.len(), 2);
    let err = s.add_boolean(0, 1, BoolOp::Union).unwrap_err();
    assert!(err.contains("nicht unterstützt"), "{err}");
}

// Zusätzliche Tests (über den Brief hinaus): Reihenfolge, Auswahl, unveränderter Zustand bei
// Fehlern, Platzierung „in place“ und Voreinstellungen des Dialogs.

#[test]
fn a_feature_with_an_explicit_origin_keeps_the_position() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let id = s
        .add_feature(
            None,
            FeatureKind::Box {
                size: DVec3::splat(4.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let i = s
        .body_index(planora_feature::BodyRef {
            feature: id,
            index: 0,
        })
        .unwrap();
    assert_eq!(i, 1);
    assert!(
        s.bodies[1].bounds.0.length() < 1e-9,
        "{:?}",
        s.bodies[1].bounds
    );
    assert!((s.bodies[1].bounds.1 - DVec3::splat(4.0)).length() < 1e-9);
}

#[test]
fn boolean_result_takes_the_lower_index_and_is_selected() {
    let mut s = AppState::new();
    box_at(&mut s, "Erster", DVec3::splat(10.0), DVec3::ZERO);
    box_at(&mut s, "Zweiter", DVec3::splat(10.0), DVec3::ZERO);
    box_at(&mut s, "Dritter", DVec3::new(10.0, 10.0, 4.0), DVec3::ZERO);
    // „Dritter“ (Index 2) liegt im unteren Teil von „Zweiter“ (Index 1); A = 2, B = 1.
    s.move_body(2, DVec3::new(0.0, 0.0, -2.0)).unwrap();
    let f = s.add_boolean(2, 1, BoolOp::Intersection).unwrap();
    assert_eq!(s.bodies[1].source.feature, f);
    assert_eq!(s.bodies.len(), 2);
    assert_eq!(s.bodies[0].name, "Erster");
    assert_eq!(s.bodies[1].name, "Schnitt 1");
    assert_eq!(s.bodies[1].kind, BodyKind::Combined);
    assert_eq!(s.selected_body, Some(1));
    assert!(s.selection.is_none() && s.hover.is_none());
    assert!(
        (s.bodies[1].mass.volume - 200.0).abs() < 1e-6,
        "{}",
        s.bodies[1].mass.volume
    );
    // das Ergebnis bleibt an der Stelle der Eingangskörper
    assert!(
        s.bodies[1].bounds.0.length() < 1e-9,
        "{:?}",
        s.bodies[1].bounds
    );
}

#[test]
fn a_failed_boolean_leaves_the_document_unchanged() {
    let mut s = state_with_box_and_cylinder();
    s.selected_body = Some(1);
    let version = s.scene_version;
    let names: Vec<String> = s.bodies.iter().map(|b| b.name.clone()).collect();
    let features = s.document.features().len();
    assert!(s.add_boolean(0, 1, BoolOp::Intersection).is_err());
    assert_eq!(s.document.features().len(), features);
    assert_eq!(
        s.bodies.iter().map(|b| b.name.clone()).collect::<Vec<_>>(),
        names
    );
    assert_eq!(s.selected_body, Some(1));
    assert_eq!(s.scene_version, version);
}

#[test]
fn translate_rejects_non_finite_offsets() {
    let mut s = state_with_box_and_cylinder();
    let before = s.bodies[0].bounds;
    assert!(s.move_body(0, DVec3::new(f64::NAN, 0.0, 0.0)).is_err());
    assert_eq!(s.bodies[0].bounds, before);
}

#[test]
fn boolean_dialog_defaults_to_selection_and_another_body() {
    use planora_app::boolean_dialog::BooleanDialog;
    let mut s = state_with_box_and_cylinder();
    let d = BooleanDialog::new(&s);
    assert_eq!((d.a, d.b, d.op), (0, 1, BoolOp::Difference));
    s.selected_body = Some(1);
    let d = BooleanDialog::with_op(&s, BoolOp::Union);
    assert_eq!((d.a, d.b, d.op), (1, 0, BoolOp::Union));
}

#[test]
fn boolean_dialog_stays_open_on_error_and_closes_on_success() {
    use planora_app::boolean_dialog::BooleanDialog;
    let mut s = state_with_box_and_cylinder();

    // A == B: „Ausführen“ ist gesperrt; ein Ausführen würde den Dialog offen lassen
    let mut d = BooleanDialog::with_op(&s, BoolOp::Union);
    d.b = d.a;
    assert!(!d.can_run());
    assert!(d.execute(&mut s), "bei A = B bleibt der Dialog offen");
    assert!(d.error().is_some_and(|m| m.contains("verschiedene")));
    assert_eq!(s.bodies.len(), 2);

    // Getrennte Körper schneiden: leeres Ergebnis → Fehlermeldung im Dialog, Dialog bleibt offen
    let mut d = BooleanDialog::with_op(&s, BoolOp::Intersection);
    assert!(d.can_run());
    assert!(
        d.execute(&mut s),
        "bei einem Fehler bleibt der Dialog offen"
    );
    assert!(
        d.error().is_some_and(|m| m.contains("leer")),
        "{:?}",
        d.error()
    );
    assert_eq!(s.message.as_deref(), d.error());
    assert_eq!(
        s.bodies.len(),
        2,
        "bei einem Fehler bleibt alles unverändert"
    );

    // Zylinder durch den Quader legen und abziehen: Erfolg schließt den Dialog
    let b = s.bodies[0].bounds;
    let c = s.bodies[1].bounds;
    let target = DVec3::new(5.0, 5.0, -2.0) + b.0;
    s.move_body(1, target - c.0 - DVec3::new(2.0, 2.0, 0.0))
        .unwrap();
    // Eine andere Auswahl (Operation oder Körper) macht die alte Meldung hinfällig
    d.op = BoolOp::Difference;
    assert!(
        d.error().is_none(),
        "Meldung gilt nur für die Auswahl, die sie erzeugt hat"
    );
    let mut e = BooleanDialog::with_op(&s, BoolOp::Union);
    e.b = e.a;
    e.execute(&mut s);
    e.b = 1 - e.a;
    assert!(
        e.error().is_none(),
        "nach Wechsel von Körper B keine alte Meldung"
    );
    assert!(!d.execute(&mut s), "nach Erfolg schließt der Dialog");
    assert!(d.error().is_none());
    assert!(s.message.is_none());
    assert_eq!(s.bodies.len(), 1);
}
