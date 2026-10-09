use planora_math::{DVec3, Frame};

#[test]
fn world_and_constructed_frames_are_orthonormal() {
    assert!(Frame::WORLD.is_orthonormal(1e-9));
    let f = Frame::from_origin_normal(DVec3::new(1.0, 2.0, 3.0), DVec3::new(1.0, 1.0, 1.0));
    assert!(f.is_orthonormal(1e-9));
}

#[test]
fn broken_frames_are_detected() {
    let mut nan = Frame::WORLD;
    nan.origin.y = f64::NAN;
    let mut inf = Frame::WORLD;
    inf.x = DVec3::new(f64::INFINITY, 0.0, 0.0);
    let mut skew = Frame::WORLD;
    skew.y = DVec3::new(0.1, 1.0, 0.0);
    let mut short = Frame::WORLD;
    short.z = DVec3::new(0.0, 0.0, 0.5);
    let mut left_handed = Frame::WORLD;
    left_handed.z = -DVec3::Z;
    for f in [nan, inf, skew, short, left_handed] {
        assert!(!f.is_orthonormal(1e-9), "{f:?}");
    }
}
