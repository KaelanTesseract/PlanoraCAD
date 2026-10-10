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

// ---- Aus der Prüfung von Task 8 (Task 9) ------------------------------------------------------

#[test]
fn an_untagged_planar_face_gives_a_german_error() {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let top = s.bodies[0]
        .solid
        .faces
        .iter()
        .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == "Deckel"))
        .map(|(i, _)| i)
        .unwrap();
    s.bodies[0].solid.faces[top].tag = None;
    let n = s.document.features().len();
    let e = s.new_sketch_on_face(0, top).unwrap_err();
    assert!(e.contains("Auf dieser Fläche geht keine Skizze"), "{e}");
    assert!(e.contains("keinem Feature zugeordnet"), "{e}");
    assert_eq!(s.document.features().len(), n, "Dokument unverändert");
}

#[test]
fn a_sketch_on_a_face_stores_the_face_frame_in_its_definition() {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let top = s.bodies[0]
        .solid
        .faces
        .iter()
        .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == "Deckel"))
        .map(|(i, _)| i)
        .unwrap();
    let id = s.new_sketch_on_face(0, top).unwrap();
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Sketch(def) => {
            let f = def.sketch.frame();
            assert!((f.origin.z - 5.0).abs() < 1e-9, "{f:?}");
            assert!(f.z.z > 0.99, "{f:?}");
        }
        k => panic!("{k:?}"),
    }
}

#[test]
fn switching_to_another_sketch_writes_the_edited_draft_back() {
    let mut s = AppState::new();
    let a = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    let b = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    let cam = s.camera;
    s.open_sketch(a).unwrap();
    draw_rectangle(&mut s, 4.0, 3.0);
    s.open_sketch(b).unwrap();
    assert_eq!(s.open_sketch_id(), Some(b));
    assert_eq!(s.document.sketch_of(a).unwrap().curves().count(), 4);
    assert_eq!(s.document.sketch_of(b).unwrap().curves().count(), 0);
    s.close_sketch();
    assert_eq!(
        s.camera.target, cam.target,
        "Kamera von vor der ersten Skizze"
    );
}

#[test]
fn a_body_moved_after_the_sketch_names_the_move_as_a_cause() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(5.0)).unwrap();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.move_body(0, DVec3::X).unwrap(); // das Verschieben steht hinter der Skizze
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
    assert!(err.contains("Verschieben"), "{err}");
}

#[test]
fn a_copied_edge_keeps_its_length_relation_after_a_resize() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let solid = s.bodies[0].solid.clone();
    assert!(
        solid
            .edges
            .iter()
            .any(|(e, _)| s.project_edge_into_sketch(0, e).is_ok())
    );
    s.close_sketch();
    let copy_length = |s: &AppState| -> f64 {
        let FeatureKind::Sketch(def) = &s.document.feature(id).unwrap().kind else {
            panic!("Skizze");
        };
        assert_eq!(def.copies.len(), 1);
        s.document
            .sketch_of(id)
            .unwrap()
            .line_length(def.copies[0].curve)
            .unwrap()
    };
    let before = copy_length(&s);
    s.document
        .update(b, |k| {
            if let FeatureKind::Box { size, .. } = k {
                size.x = 14.0;
                size.y = 27.0;
            }
        })
        .unwrap();
    s.refresh();
    let after = copy_length(&s);
    // Eine Kante in x (10 mm) wird 14 mm lang, eine in y (20 mm) 27 mm.
    let expected = if (before - 10.0).abs() < 1e-6 {
        14.0
    } else {
        assert!((before - 20.0).abs() < 1e-6, "{before}");
        27.0
    };
    assert!(
        (after - expected).abs() < 1e-6,
        "vorher {before}, nachher {after}"
    );
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

#[test]
fn a_rejected_draft_keeps_the_sketch_open() {
    // Eine Kantenkopie im Entwurf, die auf ein späteres Feature verweist, lehnt das Dokument
    // ab (Verweis nach vorn): Die Skizze bleibt offen, der Entwurf erhalten.
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.create_box(DVec3::splat(4.0)).unwrap(); // entsteht nach der Skizze
    s.open_sketch(id).unwrap();
    let solid = s.bodies[0].solid.clone();
    let edge = solid
        .edges
        .iter()
        .find_map(|(e, _)| planora_feature::EdgeRef::from_edge(&solid, e, &s.kernel.tolerance).ok())
        .expect("Kantenverweis");
    let curve = {
        let sk = s.session_sketch_mut().unwrap().1;
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(1.0, 0.0).unwrap();
        sk.add_line(a, b).unwrap()
    };
    s.sketch_draft
        .as_mut()
        .unwrap()
        .copies
        .push(planora_feature::EdgeCopy { curve, edge });
    let e = s.try_close_sketch().unwrap_err();
    assert!(e.contains("nicht übernehmen"), "{e}");
    assert_eq!(s.open_sketch_id(), Some(id), "Skizze bleibt offen");
    assert_eq!(s.sketch_draft.as_ref().unwrap().copies.len(), 1);
    assert_eq!(s.message.as_deref(), Some(e.as_str()));
    // Auch Dokument-Änderungen von außen werden dann abgelehnt.
    assert!(s.move_body(0, DVec3::X).is_err());
    assert_eq!(s.document.features().len(), 2);
}
