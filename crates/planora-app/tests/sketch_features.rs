use planora_app::sketch_state::Tool;
use planora_app::state::AppState;
use planora_feature::{FeatureKind, FeatureStatus, SketchPlane, StandardPlane};
use planora_kernel::Surface;
use planora_math::{DVec2, DVec3};

fn volume(s: &AppState, i: usize) -> f64 {
    use planora_mesh::{TessellationOptions, mass_properties, tessellate};
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

fn draw_rectangle(s: &mut AppState, w: f64, h: f64) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, Tool::Rectangle);
    se.click(sk, DVec2::new(0.0, 0.0), 0.5, false).unwrap();
    se.click(sk, DVec2::new(w, h), 0.5, false).unwrap();
}

#[test]
fn a_sketch_becomes_a_feature_and_closing_writes_the_draft_back() {
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    assert_eq!(s.document.feature(id).unwrap().name, "Skizze 1");
    s.open_sketch(id).unwrap();
    assert!(s.sketch_draft.is_some());
    let to_cam = (s.camera.eye() - s.camera.target).normalize();
    assert!(to_cam.z > 0.99, "Draufsicht: {to_cam:?}");
    draw_rectangle(&mut s, 40.0, 30.0);
    assert!(
        s.document.sketch_of(id).unwrap().curves().count() == 0,
        "vor dem Beenden unverändert"
    );
    s.close_sketch();
    assert!(s.sketch_session.is_none() && s.sketch_draft.is_none());
    assert_eq!(s.document.sketch_of(id).unwrap().curves().count(), 4);
}

#[test]
fn extruding_and_editing_the_sketch_updates_the_body() {
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    draw_rectangle(&mut s, 40.0, 30.0);
    s.close_sketch();
    let e = s.extrude_sketch(id, 0, 10.0).unwrap();
    assert_eq!(s.document.feature(e).unwrap().status, FeatureStatus::Ok);
    assert!((volume(&s, 0) - 12000.0).abs() < 1e-3);
    // Skizze wieder öffnen, die rechte obere Ecke verschieben, beenden → Körper folgt
    s.open_sketch(id).unwrap();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let p = sk.points().map(|(i, _)| i).nth(2).unwrap();
        sk.set_point_xy(p, 50.0, 30.0).unwrap();
        let _ = se;
    }
    s.close_sketch();
    assert!(volume(&s, 0) > 12000.0 + 1.0, "{}", volume(&s, 0));
}

#[test]
fn a_sketch_on_a_face_follows_a_resized_body() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let top = s.bodies[0]
        .solid
        .faces
        .iter()
        .find(|(_, f)| {
            matches!(f.surface, Surface::Plane { .. })
                && f.tag.as_ref().is_some_and(|t| t.role == "Deckel")
        })
        .map(|(i, _)| i)
        .unwrap();
    let id = s.new_sketch_on_face(0, top).unwrap();
    let z0 = s.document.sketch_of(id).unwrap().frame().origin.z;
    assert!((z0 - 5.0).abs() < 1e-9);
    s.document
        .update(b, |k| {
            if let FeatureKind::Box { size, .. } = k {
                size.z = 9.0;
            }
        })
        .unwrap();
    s.refresh();
    let z1 = s.document.sketch_of(id).unwrap().frame().origin.z;
    assert!(
        (z1 - 9.0).abs() < 1e-9,
        "die Skizzenebene ist mitgewandert: {z1}"
    );
    // nicht ebene Flächen sind abgelehnt
    s.create_cylinder(2.0, 4.0).unwrap();
    let side = s.bodies[1]
        .solid
        .faces
        .iter()
        .find(|(_, f)| !matches!(f.surface, Surface::Plane { .. }))
        .map(|(i, _)| i)
        .unwrap();
    assert!(s.new_sketch_on_face(1, side).is_err());
}

#[test]
fn copied_edges_are_recorded_and_follow_the_body() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let solid = s.bodies[0].solid.clone();
    let mut done = None;
    for (eid, _) in solid.edges.iter() {
        if s.project_edge_into_sketch(0, eid).is_ok() {
            done = Some(eid);
            break;
        }
    }
    assert!(done.is_some());
    assert_eq!(s.sketch_draft.as_ref().unwrap().copies.len(), 1);
    s.close_sketch();
    s.document
        .update(b, |k| {
            if let FeatureKind::Box { size, .. } = k {
                size.x = 14.0;
                size.y = 24.0;
            }
        })
        .unwrap();
    s.refresh();
    assert_eq!(s.document.feature(id).unwrap().status, FeatureStatus::Ok);
    let sk = s.document.sketch_of(id).unwrap();
    let longest = sk
        .curves()
        .filter_map(|(c, _)| sk.line_length(c))
        .fold(0.0_f64, f64::max);
    assert!(longest > 13.9, "die Kopie ist mitgewachsen: {longest}");
}

#[test]
fn bodies_created_after_a_sketch_cannot_be_projected_into_it() {
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.create_box(DVec3::splat(5.0)).unwrap(); // entsteht NACH der Skizze
    s.open_sketch(id).unwrap();
    let e = s.bodies[0]
        .solid
        .edges
        .iter()
        .next()
        .map(|(i, _)| i)
        .unwrap();
    let err = s.project_edge_into_sketch(0, e).unwrap_err();
    assert!(err.contains("nach der Skizze"), "{err}");
}

#[test]
fn deleting_an_open_sketch_feature_closes_the_session() {
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.delete_feature_with_dependents(id).unwrap();
    assert!(s.sketch_session.is_none() && s.sketch_draft.is_none());
    assert!(s.document.features().is_empty());
}
