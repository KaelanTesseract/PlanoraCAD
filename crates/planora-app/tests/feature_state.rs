use planora_app::state::{AppState, BodyKind};
use planora_feature::{BodyRef, FeatureKind, FeatureStatus};
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

/// Deckfläche (ebene Fläche mit Normale +z ganz oben) des Körpers `i`.
fn top_face(s: &AppState, i: usize) -> planora_kernel::Id<planora_kernel::Face> {
    let solid = &s.bodies[i].solid;
    let top_z = s.bodies[i].bounds.1.z;
    solid
        .faces
        .iter()
        .find(|(id, _)| {
            planora_sketch::plane_of_face(solid, *id)
                .is_some_and(|f| f.z.z > 0.5 && (f.origin.z - top_z).abs() < 1e-9)
        })
        .map(|(id, _)| id)
        .expect("Deckfläche")
}

#[test]
fn moving_after_a_foreign_feature_read_the_body_adds_a_new_move() {
    // Quader → Verschieben → Skizze auf seiner Deckfläche → Extrusion. Die Skizze liest den
    // verschobenen Körper (über die Flächenmarke des Quaders); ein weiteres Verschieben darf das
    // alte Verschieben deshalb nicht erweitern, sonst wanderten Skizze und Extrusion mit.
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 10.0, 4.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    let i = s.new_sketch_on_face(0, top_face(&s, 0)).unwrap();
    let frame = *s.document.sketch_of(i).unwrap().frame();
    let local = |p: DVec3| frame.to_local(p).truncate();
    let (a, b) = (
        local(DVec3::new(3.0, 3.0, 4.0)),
        local(DVec3::new(6.0, 6.0, 4.0)),
    );
    s.open_sketch(i).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(a.min(b), a.max(b))
        .unwrap();
    s.close_sketch();
    let extrusion = BodyRef {
        feature: s.extrude_sketch(i, 0, 2.0).unwrap(),
        index: 0,
    };
    let e = s.body_index(extrusion).unwrap();
    let extrusion_bounds = s.bodies[e].bounds;
    let box_bounds = s.bodies[0].bounds;
    let n = s.document.features().len();
    s.move_body(0, DVec3::Y).unwrap();
    assert_eq!(s.document.features().len(), n + 1, "ein neues Verschieben");
    let ei = s.body_index(extrusion).expect("Extrusion sichtbar");
    assert_eq!(
        s.bodies[ei].bounds, extrusion_bounds,
        "Extrusion bleibt liegen"
    );
    let moved = s.bodies.iter().position(|b| b.source != extrusion).unwrap();
    assert!((s.bodies[moved].bounds.0 - (box_bounds.0 + DVec3::Y)).length() < 1e-9);
    match &s.document.features()[1].kind {
        FeatureKind::Move { delta, .. } => assert!((*delta - DVec3::X).length() < 1e-12),
        k => panic!("{k:?}"),
    }
}

#[test]
fn moving_again_extends_the_move_when_only_unrelated_features_follow() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    // ein Quader, der den verschobenen Körper nicht liest
    s.create_box(DVec3::splat(2.0)).unwrap();
    let n = s.document.features().len();
    s.move_body(0, DVec3::Y).unwrap();
    assert_eq!(s.document.features().len(), n, "kein neues Feature");
    match &s.document.features()[1].kind {
        FeatureKind::Move { delta, .. } => {
            assert!((*delta - DVec3::new(1.0, 1.0, 0.0)).length() < 1e-12)
        }
        k => panic!("{k:?}"),
    }
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

// Zusätzliche Tests (über den Brief hinaus).

#[test]
fn a_moved_body_keeps_its_name_kind_and_selection() {
    use planora_render::{FaceRef, Pick};
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    let face = s.bodies[0].mesh.face_ranges[0].face;
    s.set_selection(Some(Pick::Face(FaceRef { body: 0, face })));
    s.move_body(0, DVec3::new(0.0, 0.0, 3.0)).unwrap();
    assert_eq!(
        s.bodies[0].name, "Quader 1",
        "Name des erzeugenden Features"
    );
    assert_eq!(s.bodies[0].kind, BodyKind::Box);
    assert_eq!(
        s.selected_body,
        Some(0),
        "Auswahl folgt dem verschobenen Körper"
    );
    assert_eq!(s.selection, Some(Pick::Face(FaceRef { body: 0, face })));
    assert_eq!(s.bodies[1].name, "Zylinder 1", "Reihenfolge bleibt");
    // Umbenennen trifft das erzeugende Feature, nicht das Verschieben
    let id = s.naming_feature(0).unwrap();
    s.rename_feature(id, "Platte").unwrap();
    assert_eq!(s.bodies[0].name, "Platte");
    assert_eq!(s.document.feature(id).unwrap().name, "Platte");
}

#[test]
fn deleting_a_moved_body_removes_its_moves_but_a_combined_input_is_protected() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.move_body(1, DVec3::Z).unwrap();
    s.delete_body(1).unwrap();
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(
        s.document.features().len(),
        1,
        "Quader und Verschieben entfernt"
    );

    s.create_box(DVec3::splat(4.0)).unwrap();
    s.move_body(1, DVec3::new(-12.0, 1.0, 1.0)).unwrap(); // überlappt Quader 1
    s.add_boolean(0, 1, BoolOp::Union).unwrap();
    let n = s.document.features().len();
    // Der Körper ist das Ergebnis der Vereinigung: Löschen nimmt nur sie heraus.
    s.delete_body(0).unwrap();
    assert_eq!(s.document.features().len(), n - 1);
    assert_eq!(s.bodies.len(), 2, "die Eingangskörper sind wieder sichtbar");
    // Ein Quader, auf dem eine Vereinigung aufbaut, wird nicht ohne Rückfrage gelöscht.
    s.add_boolean(0, 1, BoolOp::Union).unwrap();
    let a = s.document.features()[0].id;
    let e = s.delete_feature(a).unwrap_err();
    // Seit Task 9 heißen Boolesche Features nach ihrer Art („Boolesche Operation N“).
    assert!(e.contains("Boolesche Operation"), "{e}");
    assert_eq!(s.message.as_deref(), Some(e.as_str()));
}

#[test]
fn creating_a_primitive_with_invalid_sizes_leaves_the_document_alone() {
    let mut s = AppState::new();
    assert!(s.create_box(DVec3::new(1.0, 0.0, 1.0)).is_err());
    assert!(s.create_sphere(f64::INFINITY).is_err());
    assert!(s.create_cone(1.0, -2.0).is_err());
    assert!(s.document.features().is_empty());
    // ein Feature mit Fehler über die allgemeine Schnittstelle bleibt stehen, mit Meldung
    let id = s
        .add_feature(
            None,
            FeatureKind::Cylinder {
                radius: -1.0,
                height: 1.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    assert!(matches!(
        s.document.feature(id).unwrap().status,
        FeatureStatus::Error(_)
    ));
    assert!(s.message.is_some());
    assert!(s.bodies.is_empty());
}

// ---- R8: Verschieben und Features hinter dem Marker, Rücknahme bei riesiger Verschiebung ----

#[test]
fn a_reader_behind_the_marker_also_prevents_extending_the_move() {
    // Quader → Verschieben → Skizze auf der Deckfläche; der Marker steht vor der Skizze. Würde
    // das alte Verschieben erweitert, wanderte die Skizze beim Vorrücken des Markers mit.
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 10.0, 4.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    s.new_sketch_on_face(0, top_face(&s, 0)).unwrap();
    s.document.set_marker(2).unwrap();
    s.refresh();
    let n = s.document.features().len();
    s.move_body(0, DVec3::Y).unwrap();
    assert_eq!(s.document.features().len(), n + 1, "ein neues Verschieben");
    match &s.document.features()[1].kind {
        FeatureKind::Move { delta, .. } => assert!((*delta - DVec3::X).length() < 1e-12),
        k => panic!("{k:?}"),
    }
}

#[test]
fn a_huge_move_is_rejected() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    let bounds = s.bodies[0].bounds;
    let n = s.document.features().len();
    assert!(s.move_body(0, DVec3::splat(f64::MAX)).is_err());
    assert!(s.message.is_some());
    assert_eq!(s.document.features().len(), n, "kein neues Feature");
    match &s.document.features()[1].kind {
        FeatureKind::Move { delta, .. } => assert_eq!(*delta, DVec3::X, "alte Verschiebung"),
        k => panic!("{k:?}"),
    }
    assert_eq!(s.bodies[0].bounds, bounds, "Körper unverändert");
    assert_eq!(
        s.document
            .feature(s.bodies[0].source.feature)
            .unwrap()
            .status,
        FeatureStatus::Ok
    );
}
