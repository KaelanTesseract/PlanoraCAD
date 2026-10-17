use glam::{Vec2, Vec3};
use planora_render::*;
use std::collections::HashSet;

fn iso() -> OrbitCamera {
    let mut cam = OrbitCamera::default();
    cam.set_iso_view();
    cam
}

/// Bildpunkt eines Würfelpunkts in Würfeleinheiten (orthografisch, wie `cube_pieces_visible`).
fn screen(cam: &OrbitCamera, p: Vec3) -> Vec2 {
    let v = cam.view().transform_vector3(p);
    Vec2::new(v.x, v.y)
}

#[test]
fn there_are_26_regions_with_unit_normals() {
    let regions = all_regions();
    assert_eq!(all_regions().len(), 26);
    assert_eq!(regions.len(), 26);
    let faces = regions
        .iter()
        .filter(|r| matches!(r, CubeRegion::Face(_)))
        .count();
    let edges = regions
        .iter()
        .filter(|r| matches!(r, CubeRegion::Edge { .. }))
        .count();
    let corners = regions
        .iter()
        .filter(|r| matches!(r, CubeRegion::Corner { .. }))
        .count();
    assert_eq!((faces, edges, corners), (6, 12, 8));
    let unique: HashSet<_> = regions.iter().collect();
    assert_eq!(unique.len(), 26, "keine doppelten Bereiche");
    for (i, r) in regions.iter().enumerate() {
        let n = r.normal();
        assert!((n.length() - 1.0).abs() < 1e-6, "{r:?}: {n:?}");
        for other in &regions[i + 1..] {
            assert!(n.dot(other.normal()) < 0.999, "{r:?} und {other:?}");
        }
    }
}

#[test]
fn names_are_german_and_ordered() {
    assert_eq!(CubeRegion::Face(ViewFace::Front).name(), "Vorne");
    assert_eq!(
        CubeRegion::Edge {
            a: ViewFace::Top,
            b: ViewFace::Right
        }
        .name(),
        "Oben-Rechts"
    );
    assert_eq!(
        CubeRegion::Corner {
            x: ViewFace::Right,
            y: ViewFace::Front,
            z: ViewFace::Top
        }
        .name(),
        "Oben-Vorne-Rechts"
    );
    let names: HashSet<String> = all_regions().iter().map(|r| r.name()).collect();
    assert_eq!(names.len(), 26);
    for n in ["Unten-Hinten", "Vorne-Links", "Unten-Hinten-Links"] {
        assert!(names.contains(n), "{n} fehlt: {names:?}");
    }
}

#[test]
fn front_view_shows_only_the_front_face_and_the_four_edge_strips_and_corners_seen_at_grazing_angle()
{
    let mut cam = OrbitCamera::default();
    cam.set_view(ViewFace::Front);
    assert_eq!(
        current_region(&cam, 2.0),
        Some(CubeRegion::Face(ViewFace::Front))
    );
    let pieces = cube_pieces_visible(&cam, 0.02);
    assert_eq!(pieces.len(), 9, "Fläche, 4 Kanten, 4 Ecken");
    let front = pieces.last().unwrap();
    assert!(front.region.normal().dot(Vec3::NEG_Y) > 0.999);
    assert!(pieces.iter().all(|p| p.region.normal().y < 0.0));
}

#[test]
fn iso_view_current_region_is_the_top_front_right_corner() {
    assert_eq!(
        current_region(&iso(), 2.0),
        Some(CubeRegion::Corner {
            x: ViewFace::Right,
            y: ViewFace::Front,
            z: ViewFace::Top
        })
    );
}

#[test]
fn free_orbit_has_no_current_region() {
    let cam = OrbitCamera {
        yaw: 28.0_f32.to_radians(),
        pitch: 22.0_f32.to_radians(),
        ..OrbitCamera::default()
    };
    assert_eq!(current_region(&cam, 2.0), None);
}

#[test]
fn hit_test_returns_face_edge_and_corner_pieces() {
    let cam = iso();
    let pieces = cube_pieces_visible(&cam, 0.02);
    let tfr = CubeRegion::Corner {
        x: ViewFace::Right,
        y: ViewFace::Front,
        z: ViewFace::Top,
    };
    let corner = pieces
        .iter()
        .find(|p| p.region == tfr)
        .expect("Ecke Oben-Vorne-Rechts sichtbar");
    assert_eq!(corner.corners.len(), 3);
    let centre = corner.corners.iter().copied().sum::<Vec2>() / 3.0;
    assert_eq!(pieces_hit_test(&pieces, centre), Some(corner.region));
    // Feste Punkte für den Flächenanteil 0,6: knapp am Rand des Flächenquadrats „Rechts“ …
    assert_eq!(
        pieces_hit_test(&pieces, screen(&cam, Vec3::new(1.0, -0.55, 0.0))),
        Some(CubeRegion::Face(ViewFace::Right))
    );
    // … mitten auf dem Streifen zwischen „Vorne“ und „Rechts“ …
    assert_eq!(
        pieces_hit_test(&pieces, screen(&cam, Vec3::new(0.8, -0.8, 0.0))),
        Some(CubeRegion::Edge {
            a: ViewFace::Front,
            b: ViewFace::Right
        })
    );
    // … und auf dem Streifen „Oben-Vorne“ knapp vor der Ecke.
    assert_eq!(
        pieces_hit_test(&pieces, screen(&cam, Vec3::new(0.55, -0.8, 0.8))),
        Some(CubeRegion::Edge {
            a: ViewFace::Top,
            b: ViewFace::Front
        })
    );
    assert_eq!(pieces_hit_test(&pieces, Vec2::new(5.0, 5.0)), None);
}

#[test]
fn set_view_region_looks_at_the_region() {
    for r in all_regions() {
        let mut cam = OrbitCamera {
            yaw: 0.3,
            pitch: -0.2,
            ..OrbitCamera::default()
        };
        cam.set_view_region(r);
        assert_eq!(current_region(&cam, 0.5), Some(r), "{}", r.name());
        let to_cam = (cam.eye() - cam.target).normalize();
        assert!(to_cam.dot(r.normal()) > 0.999, "{}: {to_cam:?}", r.name());
        assert!(cam.yaw.is_finite() && cam.pitch.is_finite());
        // Bild-oben bleibt gültig (kein Kippen am Pol).
        let up = cam.view().inverse().y_axis.truncate();
        assert!(up.is_finite() && (up.length() - 1.0).abs() < 1e-3, "{up:?}");
    }
}

#[test]
fn corner_views_use_iso_angles() {
    let mut cam = OrbitCamera::default();
    cam.set_view_region(CubeRegion::Corner {
        x: ViewFace::Right,
        y: ViewFace::Front,
        z: ViewFace::Top,
    });
    assert!((cam.yaw - ISO_VIEW.0).abs() < 1e-4, "{}", cam.yaw);
    assert!((cam.pitch - ISO_VIEW.1).abs() < 1e-4, "{}", cam.pitch);
}

#[test]
fn home_is_iso() {
    let mut cam = OrbitCamera::default();
    cam.home();
    assert_eq!((cam.yaw, cam.pitch), ISO_VIEW);
}

#[test]
fn text_frames_are_right_handed_and_never_mirrored() {
    for face in ViewFace::ALL {
        let n = CubeRegion::Face(face).normal();
        let (right, up) = face_text_frame(face);
        assert!((right.cross(up) - n).length() < 1e-6, "{face:?}");
        let expected_up = match face {
            ViewFace::Top => Vec3::Y,
            ViewFace::Bottom => Vec3::NEG_Y,
            _ => Vec3::Z,
        };
        assert!((up - expected_up).length() < 1e-6, "{face:?}: {up:?}");
        // Von außen frontal angesehen: „rechts“ zeigt im Bild nach rechts, „oben“ nach oben.
        let mut cam = OrbitCamera::default();
        cam.set_view(face);
        let view = cam.view();
        let r = view.transform_vector3(right);
        let u = view.transform_vector3(up);
        assert!(r.x > 0.99, "{face:?}: rechts im Bild {r:?}");
        assert!(u.y > 0.99, "{face:?}: oben im Bild {u:?}");
    }
}

#[test]
fn pieces_are_sorted_back_to_front_and_back_faces_culled() {
    let pieces = cube_pieces_visible(&iso(), 0.02);
    assert!(pieces.windows(2).all(|w| w[0].depth <= w[1].depth));
    let regions: HashSet<_> = pieces.iter().map(|p| p.region).collect();
    assert_eq!(regions.len(), pieces.len());
    for f in [ViewFace::Front, ViewFace::Right, ViewFace::Top] {
        assert!(regions.contains(&CubeRegion::Face(f)), "{f:?}");
    }
    for f in [ViewFace::Back, ViewFace::Left, ViewFace::Bottom] {
        assert!(!regions.contains(&CubeRegion::Face(f)), "{f:?}");
    }
    let dir = Vec3::new(1.0, -1.0, 1.0).normalize();
    for p in &pieces {
        assert!(p.region.normal().dot(dir) > 0.02, "{:?}", p.region);
        let n = match p.region {
            CubeRegion::Corner { .. } => 3,
            _ => 4,
        };
        assert_eq!(p.corners.len(), n, "{:?}", p.region);
    }
    let all_facing = all_regions()
        .iter()
        .filter(|r| r.normal().dot(dir) > 0.02)
        .count();
    assert_eq!(pieces.len(), all_facing);
    assert_eq!(
        pieces.last().unwrap().region,
        CubeRegion::Corner {
            x: ViewFace::Right,
            y: ViewFace::Front,
            z: ViewFace::Top
        },
        "die Ecke zur Kamera liegt vorne"
    );
}

#[test]
fn cube_pieces_visible_filters_by_facing() {
    for (yaw, pitch) in [(-0.8_f32, 0.5_f32), (0.3, -1.2), (2.0, 0.05), (-2.9, 1.4)] {
        let cam = OrbitCamera {
            yaw,
            pitch,
            ..OrbitCamera::default()
        };
        let all = cube_pieces_visible(&cam, 0.02);
        for min in [0.1_f32, 0.15, 0.4] {
            let some = cube_pieces_visible(&cam, min);
            let want: Vec<CubeRegion> = all
                .iter()
                .filter(|p| p.depth > min)
                .map(|p| p.region)
                .collect();
            let got: Vec<CubeRegion> = some.iter().map(|p| p.region).collect();
            assert_eq!(got, want, "yaw {yaw}, pitch {pitch}, min {min}");
            assert!(
                some.windows(2).all(|w| w[0].depth <= w[1].depth),
                "hinten zuerst"
            );
        }
    }
}

#[test]
fn invalid_regions_are_detected_and_ignored() {
    use ViewFace::*;
    for r in all_regions() {
        assert!(r.is_valid(), "{}", r.name());
    }
    let invalid = [
        CubeRegion::Edge { a: Top, b: Bottom },
        CubeRegion::Edge { a: Front, b: Front },
        // Falsche Reihenfolge: Z kommt vor Y.
        CubeRegion::Edge { a: Front, b: Top },
        CubeRegion::Edge { a: Right, b: Front },
        CubeRegion::Corner {
            x: Top,
            y: Front,
            z: Top,
        },
        CubeRegion::Corner {
            x: Right,
            y: Left,
            z: Top,
        },
        CubeRegion::Corner {
            x: Right,
            y: Front,
            z: Back,
        },
    ];
    for r in invalid {
        assert!(!r.is_valid(), "{r:?}");
        assert!(r.normal().is_finite(), "{r:?}: {:?}", r.normal());
        let mut cam = OrbitCamera {
            yaw: 0.3,
            pitch: -0.2,
            distance: 7.0,
            ..OrbitCamera::default()
        };
        let before = (cam.yaw, cam.pitch, cam.distance);
        cam.set_view_region(r);
        assert_eq!((cam.yaw, cam.pitch, cam.distance), before, "{r:?}");
    }
}

#[test]
fn nan_camera_has_no_current_region_and_no_pieces_panic() {
    let broken = [
        (f32::NAN, 0.2, 5.0),
        (0.3, f32::NAN, 5.0),
        (0.3, 0.2, f32::NAN),
        (f32::INFINITY, 0.2, 5.0),
        (0.3, f32::NEG_INFINITY, 5.0),
        (0.3, 0.2, f32::INFINITY),
    ];
    for (yaw, pitch, distance) in broken {
        let cam = OrbitCamera {
            yaw,
            pitch,
            distance,
            ..OrbitCamera::default()
        };
        assert_eq!(current_region(&cam, 2.0), None, "{yaw} {pitch} {distance}");
        let pieces = cube_pieces_visible(&cam, 0.02);
        let hit = pieces_hit_test(&pieces, Vec2::ZERO);
        let _ = hit;
    }
}
