use planora_math::*;

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-12, "{a} != {b}");
}

fn close3(a: DVec3, b: DVec3) {
    assert!(a.distance(b) < 1e-12, "{a:?} != {b:?}");
}

#[test]
fn default_tolerance_values() {
    let t = Tolerance::default();
    close(t.linear, 1e-6);
    close(t.angular, 1e-9);
}

#[test]
fn tolerance_compares_points() {
    let t = Tolerance::default();
    assert!(t.points_equal(DVec3::ZERO, DVec3::new(5e-7, 0.0, 0.0)));
    assert!(!t.points_equal(DVec3::ZERO, DVec3::new(2e-6, 0.0, 0.0)));
}

#[test]
fn frame_from_normal_is_orthonormal_and_right_handed() {
    let normals = [
        DVec3::X,
        DVec3::Y,
        DVec3::Z,
        -DVec3::Z,
        DVec3::new(1.0, 2.0, 3.0),
    ];
    for n in normals {
        let f = Frame::from_origin_normal(DVec3::new(1.0, 1.0, 1.0), n);
        close(f.x.length(), 1.0);
        close(f.y.length(), 1.0);
        close(f.z.length(), 1.0);
        close(f.x.dot(f.y), 0.0);
        close(f.x.dot(f.z), 0.0);
        close(f.y.dot(f.z), 0.0);
        close3(f.x.cross(f.y), f.z);
        close3(f.z, n.normalize());
    }
}

#[test]
fn frame_local_roundtrip() {
    let f = Frame::from_origin_normal(DVec3::new(1.0, 2.0, 3.0), DVec3::new(0.3, -0.2, 0.9));
    let local = DVec3::new(0.5, -1.5, 2.0);
    let p = f.point(local.x, local.y, local.z);
    close3(f.to_local(p), local);
}

#[test]
fn frame_point_2d_uses_x_and_y_axes() {
    let f = Frame::WORLD;
    close3(f.point_2d(DVec2::new(2.0, 3.0)), DVec3::new(2.0, 3.0, 0.0));
}

#[test]
fn rotation_keeps_frame_right_handed() {
    let m = DMat4::from_translation(DVec3::new(1.0, 0.0, 0.0)) * DMat4::from_rotation_z(0.7);
    let f = Frame::WORLD.transformed(&m);
    close3(f.x.cross(f.y), f.z);
    close3(f.origin, m.transform_point3(DVec3::ZERO));
}

#[test]
fn mirror_keeps_frame_right_handed() {
    let m = DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0));
    let f = Frame::WORLD.transformed(&m);
    close3(f.x.cross(f.y), f.z);
}

#[test]
fn orient2d_sign_matches_winding() {
    let a = DVec2::new(0.0, 0.0);
    let b = DVec2::new(1.0, 0.0);
    assert!(orient2d(a, b, DVec2::new(0.0, 1.0)) > 0.0);
    assert!(orient2d(a, b, DVec2::new(0.0, -1.0)) < 0.0);
    assert_eq!(orient2d(a, b, DVec2::new(2.0, 0.0)), 0.0);
}
