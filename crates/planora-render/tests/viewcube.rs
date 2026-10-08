use glam::Vec2;
use planora_render::*;

fn visible(cam: &OrbitCamera) -> Vec<ViewFace> {
    let mut v: Vec<_> = cube_faces(cam).iter().map(|f| f.face).collect();
    v.sort_by_key(|f| format!("{f:?}"));
    v
}

#[test]
fn front_view_shows_only_the_front_face() {
    let mut cam = OrbitCamera::default();
    cam.set_view(ViewFace::Front);
    assert_eq!(visible(&cam), vec![ViewFace::Front]);
}

#[test]
fn top_view_shows_only_the_top_face() {
    let mut cam = OrbitCamera::default();
    cam.set_view(ViewFace::Top);
    assert_eq!(visible(&cam), vec![ViewFace::Top]);
}

#[test]
fn isometric_view_shows_three_faces() {
    let mut cam = OrbitCamera::default();
    cam.set_iso_view();
    assert_eq!(
        visible(&cam),
        vec![ViewFace::Front, ViewFace::Right, ViewFace::Top]
    );
}

#[test]
fn clicking_the_center_of_the_front_face_selects_it() {
    let mut cam = OrbitCamera::default();
    cam.set_view(ViewFace::Front);
    let faces = cube_faces(&cam);
    assert_eq!(cube_hit_test(&faces, Vec2::ZERO), Some(ViewFace::Front));
    assert_eq!(cube_hit_test(&faces, Vec2::new(5.0, 5.0)), None);
}

#[test]
fn every_standard_view_looks_at_the_expected_side() {
    // Die Kamera steht auf der Seite der Fläche, die sie ansieht.
    let expectations = [
        (ViewFace::Front, glam::Vec3::NEG_Y),
        (ViewFace::Back, glam::Vec3::Y),
        (ViewFace::Right, glam::Vec3::X),
        (ViewFace::Left, glam::Vec3::NEG_X),
        (ViewFace::Top, glam::Vec3::Z),
        (ViewFace::Bottom, glam::Vec3::NEG_Z),
    ];
    for (face, side) in expectations {
        let mut cam = OrbitCamera::default();
        cam.set_view(face);
        let dir = (cam.eye() - cam.target).normalize();
        assert!(dir.dot(side) > 0.99, "{face:?}: {dir:?}");
    }
}
