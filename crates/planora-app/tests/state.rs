use glam::Vec2;
use planora_app::create_dialog::CreateDialog;
use planora_app::state::*;
use planora_math::DVec3;
use planora_render::*;

/// Würfel als Feature neben den vorhandenen Körpern; liefert den Index des neuen Körpers.
fn add_cube(s: &mut AppState, size: f64) -> usize {
    let id = s.create_box(DVec3::splat(size)).unwrap();
    s.body_index(planora_feature::BodyRef {
        feature: id,
        index: 0,
    })
    .unwrap()
}

#[test]
fn new_state_is_empty_and_framing_it_does_not_panic() {
    let mut s = AppState::new();
    assert!(s.bodies.is_empty());
    s.frame_all();
    assert!(
        s.camera
            .view_proj()
            .to_cols_array()
            .iter()
            .all(|v| v.is_finite())
    );
    assert!(
        s.pick(Vec2::new(100.0, 100.0), Vec2::new(800.0, 600.0))
            .is_none()
    );
    assert!(
        !s.scene.line_vertices.is_empty(),
        "das Raster ist auch ohne Körper sichtbar"
    );
    assert!(s.scene.face_vertices.is_empty());
}

#[test]
fn bodies_are_placed_next_to_each_other() {
    let mut s = AppState::new();
    let a = add_cube(&mut s, 10.0);
    let b = add_cube(&mut s, 10.0);
    assert_eq!((a, b), (0, 1));
    assert!(
        s.bodies[1].bounds.0.x > s.bodies[0].bounds.1.x,
        "Körper überlappen sich"
    );
    assert_eq!(
        s.scene.face_vertices.len(),
        s.bodies[0].mesh.positions.len() + s.bodies[1].mesh.positions.len()
    );
}

#[test]
fn adding_a_body_fills_mass_properties_and_bumps_the_scene_version() {
    let mut s = AppState::new();
    let v0 = s.scene_version;
    add_cube(&mut s, 10.0);
    assert!(s.scene_version > v0);
    assert!((s.bodies[0].mass.volume - 1000.0).abs() < 1e-6);
}

#[test]
fn invalid_solids_are_rejected_with_a_message() {
    // Körper entstehen nur noch aus Features: ungültige Maße werden abgelehnt, das Dokument
    // bleibt leer.
    let mut s = AppState::new();
    let err = s.create_box(DVec3::new(1.0, -1.0, 1.0)).unwrap_err();
    assert!(!err.is_empty());
    assert_eq!(s.message.as_deref(), Some(err.as_str()));
    assert!(s.create_cylinder(f64::NAN, 1.0).is_err());
    assert!(s.bodies.is_empty());
    assert!(s.document.features().is_empty());
}

#[test]
fn removing_a_body_clears_references_to_it() {
    let mut s = AppState::new();
    add_cube(&mut s, 10.0);
    add_cube(&mut s, 10.0);
    add_cube(&mut s, 10.0);
    let face = s.bodies[1].mesh.face_ranges[0].face;
    s.set_selection(Some(Pick::Face(FaceRef { body: 1, face })));
    s.set_hover(Some(Pick::Face(FaceRef { body: 1, face })));
    s.delete_feature(s.bodies[1].source.feature).unwrap();
    assert_eq!(s.bodies.len(), 2);
    assert!(s.selection.is_none());
    assert!(s.hover.is_none());
    assert_eq!(s.selected_body, None);

    // Wird ein Körper vor dem ausgewählten entfernt, rückt der Index nach; Auswahl und
    // Hervorhebung bleiben auf dem (weiter sichtbaren) Körper.
    s.set_selection(Some(Pick::Face(FaceRef { body: 1, face })));
    s.set_hover(Some(Pick::Face(FaceRef { body: 1, face })));
    assert_eq!(s.selected_body, Some(1));
    assert_eq!(s.bodies[1].name, "Quader 3");
    s.delete_feature(s.bodies[0].source.feature).unwrap();
    assert_eq!(s.selected_body, Some(0));
    assert_eq!(s.bodies[0].name, "Quader 3");
    assert_eq!(s.hover, Some(Pick::Face(FaceRef { body: 0, face })));
    assert_eq!(s.selection, Some(Pick::Face(FaceRef { body: 0, face })));

    // unbekanntes Feature → Fehler, nichts ändert sich
    assert!(s.delete_feature(planora_feature::FeatureId(999)).is_err());
    assert_eq!(s.bodies.len(), 1);
}

#[test]
fn picking_in_empty_or_tiny_viewports_finds_nothing() {
    let mut s = AppState::new();
    add_cube(&mut s, 10.0);
    for viewport in [
        Vec2::new(0.0, 0.0),
        Vec2::new(0.5, 100.0),
        Vec2::new(100.0, 0.5),
    ] {
        assert!(s.pick(Vec2::ZERO, viewport).is_none(), "{viewport:?}");
        assert!(s.pick(viewport / 2.0, viewport).is_none(), "{viewport:?}");
    }
}

#[test]
fn body_names_stay_unique_after_deleting() {
    let mut s = AppState::new();
    let dialog = CreateDialog::Box {
        x: 10.0,
        y: 10.0,
        z: 10.0,
    };
    dialog.create(&mut s);
    dialog.create(&mut s);
    s.delete_feature(s.bodies[0].source.feature).unwrap();
    dialog.create(&mut s);
    let names: Vec<&str> = s.bodies.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names.len(), 2);
    assert_ne!(names[0], names[1], "{names:?}");
}

#[test]
fn picking_hits_a_face_in_the_middle_of_the_screen() {
    let mut s = AppState::new();
    add_cube(&mut s, 10.0);
    s.camera.set_viewport(800.0, 600.0);
    // In der Standardansicht (isometrisch) liegt die vordere senkrechte Würfelkante genau in der
    // Bildmitte; dort gewinnt zu Recht die Kante. Eine Fläche trifft man neben der Mitte.
    assert!(matches!(
        s.pick(Vec2::new(400.0, 300.0), Vec2::new(800.0, 600.0)),
        Some(Pick::Edge(_))
    ));
    let hit = s.pick(Vec2::new(460.0, 300.0), Vec2::new(800.0, 600.0));
    assert!(matches!(hit, Some(Pick::Face(_))), "{hit:?}");
    assert!(
        s.pick(Vec2::new(2.0, 2.0), Vec2::new(800.0, 600.0))
            .is_none()
    );
}

#[test]
fn hover_and_selection_changes_rebuild_the_scene_only_when_needed() {
    let mut s = AppState::new();
    add_cube(&mut s, 10.0);
    let face = s.bodies[0].mesh.face_ranges[0].face;
    let v = s.scene_version;
    s.set_hover(None);
    assert_eq!(s.scene_version, v);
    s.set_hover(Some(Pick::Face(FaceRef { body: 0, face })));
    assert!(s.scene_version > v);
    let v = s.scene_version;
    s.set_hover(Some(Pick::Face(FaceRef { body: 0, face })));
    assert_eq!(s.scene_version, v);
}

#[test]
fn a_sphere_at_the_dialog_maximum_gives_a_bounded_scene() {
    let mut s = AppState::new();
    s.create_sphere(100_000.0).unwrap();
    assert!(
        s.scene.face_indices.len() < 3_000_000,
        "{}",
        s.scene.face_indices.len()
    );
    assert!(s.scene.face_vertices.len() < 1_000_000);
}
