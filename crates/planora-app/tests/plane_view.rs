//! Skizzen 6, Task 11: Konstruktionsebenen erscheinen in der 3D-Ansicht als halbdurchsichtige
//! orange Rechtecke und lassen sich anklicken; Eckpunkte nur bei der Punktwahl (ohne Fenster,
//! 1920×1080).

use egui::{Rect, pos2};
use glam::{Vec2, Vec3};
use planora_app::sketch_view;
use planora_app::state::{AppState, PickFilter, StandardPlane};
use planora_feature::planes::{PlaneBase, PlaneDef, PlaneFeature};
use planora_feature::{FaceRef, FeatureId, FeatureKind, FeatureStatus};
use planora_math::{DVec3, Tolerance};
use planora_render::{OrbitCamera, Pick};
use planora_sketch::plane_of_face;

const VIEWPORT: Vec2 = Vec2::new(1920.0, 1080.0);

/// Quader mit Kante `size` im Ursprung und eine Versatzebene `z` über XY.
fn box_and_plane(size: f64, z: f64) -> (AppState, FeatureId) {
    let mut s = AppState::new();
    s.document
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::splat(size),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let plane = s
        .document
        .add_feature(
            "Ebene",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Offset {
                    base: PlaneBase::Standard(StandardPlane::XY),
                    distance: z,
                },
            }),
        )
        .unwrap();
    s.refresh();
    (s, plane)
}

/// Kamera schräg von oben auf `target`, Bild 1920×1080.
fn look_from_above(s: &mut AppState, target: Vec3) {
    s.camera = OrbitCamera {
        target,
        distance: 200.0,
        pitch: 1.2,
        ..OrbitCamera::default()
    };
    s.camera.set_viewport(VIEWPORT.x, VIEWPORT.y);
}

fn screen(s: &AppState, p: Vec3) -> Vec2 {
    s.camera.project(p, VIEWPORT).unwrap()
}

#[test]
fn plane_wins_when_in_front() {
    // Ebene 30 mm über dem 20-mm-Quader: Von oben liegt sie vor der Deckfläche.
    let (mut s, plane) = box_and_plane(20.0, 30.0);
    look_from_above(&mut s, Vec3::new(10.0, 10.0, 10.0));
    let at = screen(&s, Vec3::new(10.0, 10.0, 30.0));
    assert_eq!(s.pick(at, VIEWPORT), Some(Pick::Plane(plane)));
    // Neben dem Quader, aber im Rechteck der Ebene: ebenfalls die Ebene.
    let beside = screen(&s, Vec3::new(-15.0, 10.0, 30.0));
    assert_eq!(s.pick(beside, VIEWPORT), Some(Pick::Plane(plane)));
    // Klick wählt die Ebene und ihr Feature (wie im Browser), keinen Körper.
    s.set_selection(s.pick(at, VIEWPORT));
    assert_eq!(s.selection, Some(Pick::Plane(plane)));
    assert_eq!(s.selected_feature, Some(plane));
    assert_eq!(s.selected_body, None);
    // Projizieren und Schneiden im Skizzen-Modus sehen keine Ebenen.
    assert!(!matches!(
        s.pick_with(at, VIEWPORT, PickFilter::GEOMETRY),
        Some(Pick::Plane(_))
    ));
}

#[test]
fn nearer_face_wins_over_plane() {
    // Ebene mitten im Quader: Die Deckfläche liegt näher an der Kamera.
    let (mut s, plane) = box_and_plane(20.0, 10.0);
    look_from_above(&mut s, Vec3::new(10.0, 10.0, 10.0));
    let at = screen(&s, Vec3::new(10.0, 10.0, 20.0));
    assert!(
        matches!(s.pick(at, VIEWPORT), Some(Pick::Face(_))),
        "{:?}",
        s.pick(at, VIEWPORT)
    );
    // Außerhalb des Quaders trifft der Strahl nur die Ebene.
    let beside = screen(&s, Vec3::new(-15.0, 10.0, 10.0));
    assert_eq!(s.pick(beside, VIEWPORT), Some(Pick::Plane(plane)));
}

#[test]
fn edge_in_front_of_plane_wins() {
    let (mut s, _) = box_and_plane(20.0, 10.0);
    look_from_above(&mut s, Vec3::new(10.0, 10.0, 10.0));
    // Mitte der oberen Kante y = 0 (z = 20) liegt vor der Ebene.
    let at = screen(&s, Vec3::new(10.0, 0.0, 20.0));
    assert!(
        matches!(s.pick(at, VIEWPORT), Some(Pick::Edge(_))),
        "{:?}",
        s.pick(at, VIEWPORT)
    );
}

#[test]
fn plane_rect_size_and_center() {
    // Kleiner Quader: mindestens 25 mm halbe Kante; Mitte = Fußpunkt des Ursprungs.
    let (s, plane) = box_and_plane(20.0, 30.0);
    let r = s.plane_rect(plane).unwrap();
    assert!((r.half.x - 25.0).abs() < 1e-9 && (r.half.y - 25.0).abs() < 1e-9);
    assert!((r.frame.origin - DVec3::new(0.0, 0.0, 30.0)).length() < 1e-9);
    // Großer Quader: 0,6 × Diagonale.
    let (s, plane) = box_and_plane(100.0, 30.0);
    let r = s.plane_rect(plane).unwrap();
    let want = 0.6 * (3.0_f64).sqrt() * 100.0;
    assert!((r.half.x - want).abs() < 1e-6, "{}", r.half.x);
}

#[test]
fn plane_on_face_is_centered_on_the_face() {
    // Quader 40 × 20 × 10 bei x = 100; Versatzebene 5 mm über der Deckfläche.
    let mut s = AppState::new();
    s.document
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(40.0, 20.0, 10.0),
                origin: DVec3::new(100.0, 0.0, 0.0),
            },
        )
        .unwrap();
    s.refresh();
    let solid = &s.bodies[0].solid;
    let (top, _) = solid
        .faces
        .iter()
        .find(|(f, _)| plane_of_face(solid, *f).is_some_and(|fr| fr.z.z > 0.999))
        .unwrap();
    let r = FaceRef::from_face(solid, top, &Tolerance::default()).unwrap();
    let plane = s
        .document
        .add_feature(
            "Ebene",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Offset {
                    base: PlaneBase::Face(r),
                    distance: 5.0,
                },
            }),
        )
        .unwrap();
    s.refresh();
    let rect = s.plane_rect(plane).unwrap();
    assert!(
        (rect.frame.origin - DVec3::new(120.0, 10.0, 15.0)).length() < 1e-6,
        "{:?}",
        rect.frame.origin
    );
}

#[test]
fn suppressed_plane_not_drawn() {
    let (mut s, plane) = box_and_plane(20.0, 30.0);
    look_from_above(&mut s, Vec3::new(10.0, 10.0, 10.0));
    let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(VIEWPORT.x, VIEWPORT.y));
    assert!(s.plane_rect(plane).is_some());
    assert_eq!(sketch_view::plane_shapes(&s, rect).len(), 1);
    let at = screen(&s, Vec3::new(-15.0, 10.0, 30.0));

    // Unterdrückt: nicht gezeichnet, nicht anklickbar.
    s.document.set_suppressed(plane, true).unwrap();
    s.refresh();
    assert!(s.plane_rect(plane).is_none());
    assert!(sketch_view::plane_shapes(&s, rect).is_empty());
    assert_eq!(s.pick(at, VIEWPORT), None);
    s.document.set_suppressed(plane, false).unwrap();
    s.refresh();
    assert_eq!(sketch_view::plane_shapes(&s, rect).len(), 1);

    // Zurückgerollt (Marker vor der Ebene): ebenfalls nicht.
    s.document.set_marker(1).unwrap();
    s.refresh();
    assert!(s.plane_rect(plane).is_none());
    assert!(sketch_view::plane_shapes(&s, rect).is_empty());
}

#[test]
fn failed_plane_not_drawn() {
    // Mittelebene aus zweimal derselben Ebene: Fehler.
    let mut s = AppState::new();
    let plane = s
        .document
        .add_feature(
            "Ebene",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Mid {
                    a: PlaneBase::Standard(StandardPlane::XY),
                    b: PlaneBase::Standard(StandardPlane::XY),
                },
            }),
        )
        .unwrap();
    s.refresh();
    assert!(matches!(
        s.document.feature(plane).unwrap().status,
        FeatureStatus::Error(_)
    ));
    assert!(s.plane_rect(plane).is_none());
}

#[test]
fn vertex_pick_only_in_point_mode() {
    let (mut s, _) = box_and_plane(20.0, -50.0);
    look_from_above(&mut s, Vec3::new(10.0, 10.0, 10.0));
    let corner = Vec3::new(20.0, 0.0, 20.0);
    let at = screen(&s, corner) + Vec2::new(3.0, -2.0);
    // Normale Auswahl: wie bisher keine Eckpunkte.
    assert!(!matches!(s.pick(at, VIEWPORT), Some(Pick::Vertex { .. })));
    let Some(Pick::Vertex { body, edge, end }) = s.pick_with(at, VIEWPORT, PickFilter::POINTS)
    else {
        panic!(
            "Ecke erwartet: {:?}",
            s.pick_with(at, VIEWPORT, PickFilter::POINTS)
        );
    };
    assert_eq!(body, 0);
    let solid = &s.bodies[0].solid;
    let e = &solid.edges[edge];
    let p = solid.vertices[if end { e.end } else { e.start }].point;
    assert!((p - corner.as_dvec3()).length() < 1e-9, "{p:?}");
}
