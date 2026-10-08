use glam::{Vec2, Vec3};
use planora_render::OrbitCamera;
use std::f32::consts::FRAC_PI_2;

fn cam() -> OrbitCamera {
    OrbitCamera {
        target: Vec3::new(10.0, 20.0, 5.0),
        distance: 100.0,
        yaw: 0.6,
        pitch: 0.4,
        aspect: 1.5,
        ..OrbitCamera::default()
    }
}

#[test]
fn target_projects_to_screen_center() {
    let c = cam();
    let ndc = c.view_proj().project_point3(c.target);
    assert!(ndc.x.abs() < 1e-4 && ndc.y.abs() < 1e-4);
    assert!(ndc.z > 0.0 && ndc.z < 1.0);
}

#[test]
fn center_ray_passes_through_target() {
    let c = cam();
    let (origin, dir) = c.ray(Vec2::ZERO);
    assert!((dir.length() - 1.0).abs() < 1e-4);
    let to_target = c.target - origin;
    let off_axis = to_target - dir * to_target.dot(dir);
    assert!(off_axis.length() < 1e-2, "{off_axis:?}");
}

#[test]
fn project_matches_ray() {
    let c = cam();
    let viewport = Vec2::new(900.0, 600.0);
    let px = c.project(c.target, viewport).unwrap();
    assert!((px - viewport / 2.0).length() < 1e-2, "{px:?}");
}

#[test]
fn points_behind_the_camera_are_not_projected() {
    let c = cam();
    let behind = c.eye() + (c.eye() - c.target);
    assert!(c.project(behind, Vec2::new(800.0, 600.0)).is_none());
}

#[test]
fn orbit_clamps_pitch() {
    let mut c = cam();
    c.orbit(0.0, 1.0e6);
    assert!(c.pitch <= OrbitCamera::MAX_PITCH + 1e-6);
    c.orbit(0.0, -2.0e6);
    assert!(c.pitch >= -OrbitCamera::MAX_PITCH - 1e-6);
    const { assert!(OrbitCamera::MAX_PITCH < FRAC_PI_2) };
}

#[test]
fn zoom_stays_within_bounds() {
    let mut c = cam();
    for _ in 0..1000 {
        c.zoom(500.0);
    }
    assert!(c.distance >= 0.01);
    for _ in 0..1000 {
        c.zoom(-500.0);
    }
    assert!(c.distance <= 1.0e6);
}

#[test]
fn pan_moves_target_parallel_to_the_screen() {
    let mut c = cam();
    let before = c.target;
    c.pan(30.0, -12.0, 600.0);
    let delta = c.target - before;
    assert!(delta.length() > 0.0);
    let view_dir = (c.target - c.eye()).normalize();
    assert!(delta.dot(view_dir).abs() < 1e-3);
}

#[test]
fn zero_sized_viewport_is_ignored() {
    let mut c = cam();
    let aspect = c.aspect;
    c.set_viewport(0.0, 0.0);
    c.set_viewport(100.0, 0.0);
    assert_eq!(c.aspect, aspect);
    c.set_viewport(300.0, 100.0);
    assert!((c.aspect - 3.0).abs() < 1e-6);
    assert!(c.view_proj().to_cols_array().iter().all(|v| v.is_finite()));
}

#[test]
fn frame_bounds_puts_box_in_view() {
    let mut c = OrbitCamera::default();
    c.frame_bounds(Vec3::new(-5.0, -5.0, -5.0), Vec3::new(5.0, 5.0, 5.0));
    assert!(c.target.length() < 1e-5);
    for corner in [
        Vec3::new(-5.0, -5.0, -5.0),
        Vec3::new(5.0, 5.0, 5.0),
        Vec3::new(-5.0, 5.0, -5.0),
    ] {
        let ndc = c.view_proj().project_point3(corner);
        assert!(ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0, "{ndc:?}");
    }
}
