// Der Test setzt das Kameraziel bewusst nach `default()`, wie in der Vorgabe.
#![allow(clippy::field_reassign_with_default)]

use glam::{Vec3, Vec4Swizzles};
use planora_render::{OrbitCamera, Palette};

#[test]
fn looking_along_a_normal_puts_the_camera_there() {
    let mut cam = OrbitCamera::default();
    cam.target = Vec3::new(1.0, 2.0, 3.0);
    for dir in [Vec3::X, -Vec3::Y, Vec3::new(1.0, 1.0, 1.0).normalize()] {
        cam.look_along(dir, Vec3::Z);
        let to_cam = (cam.eye() - cam.target).normalize();
        assert!(to_cam.dot(dir) > 0.999, "{dir:?} → {to_cam:?}");
    }
}

#[test]
fn top_view_keeps_x_right_and_y_up() {
    let mut cam = OrbitCamera::default();
    cam.look_along(Vec3::Z, Vec3::Y);
    let view = cam.view();
    let x = (view * Vec3::X.extend(0.0)).xyz();
    let y = (view * Vec3::Y.extend(0.0)).xyz();
    assert!(x.x > 0.99 && x.y.abs() < 0.05, "x zeigt nach rechts: {x:?}");
    assert!(y.y > 0.99 && y.x.abs() < 0.05, "y zeigt nach oben: {y:?}");
}

#[test]
fn dimmed_palette_stays_in_range_and_differs() {
    let p = Palette::dark();
    let d = p.dimmed();
    assert_ne!(p.face_base, d.face_base);
    assert_eq!(p.grid, d.grid);
    for c in [
        d.face_base,
        d.face_hover,
        d.face_selected,
        d.edge_base,
        d.edge_hover,
        d.edge_selected,
    ] {
        assert!(c.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}

#[test]
fn bottom_view_keeps_the_up_hint_on_screen() {
    let mut cam = OrbitCamera::default();
    cam.look_along(-Vec3::Z, Vec3::Y);
    let to_cam = (cam.eye() - cam.target).normalize();
    assert!(to_cam.z < -0.99);
    let y = (cam.view() * Vec3::Y.extend(0.0)).xyz();
    assert!(y.y > 0.99, "y zeigt nach oben: {y:?}");
    let before = cam;
    cam.look_along(Vec3::ZERO, Vec3::Y);
    assert_eq!(cam, before, "Nullrichtung ändert nichts");
}
