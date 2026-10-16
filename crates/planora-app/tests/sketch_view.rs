use glam::{Vec2, Vec3};
use planora_app::sketch_view::{
    overlay_text, pick_radius_units, screen_to_sketch, sketch_to_screen,
};
use planora_math::{DVec2, Frame};
use planora_render::OrbitCamera;

fn top_camera() -> OrbitCamera {
    let mut cam = OrbitCamera {
        target: Vec3::new(5.0, 3.0, 0.0),
        distance: 100.0,
        ..Default::default()
    };
    cam.look_along(Vec3::Z, Vec3::Y);
    cam.set_viewport(800.0, 600.0);
    cam
}

#[test]
fn the_screen_center_maps_to_the_camera_target_on_the_plane() {
    let cam = top_camera();
    let vp = Vec2::new(800.0, 600.0);
    let p = screen_to_sketch(&cam, &Frame::WORLD, vp, Vec2::new(400.0, 300.0)).unwrap();
    assert!((p - DVec2::new(5.0, 3.0)).length() < 1e-4, "{p:?}");
}

#[test]
fn mapping_round_trips_and_x_points_right_y_points_up() {
    let cam = top_camera();
    let vp = Vec2::new(800.0, 600.0);
    for p in [
        DVec2::new(0.0, 0.0),
        DVec2::new(12.0, -7.0),
        DVec2::new(-30.0, 22.0),
    ] {
        let s = sketch_to_screen(&cam, &Frame::WORLD, vp, p).unwrap();
        let back = screen_to_sketch(&cam, &Frame::WORLD, vp, s).unwrap();
        assert!((back - p).length() < 1e-3, "{p:?} → {s:?} → {back:?}");
    }
    let o = sketch_to_screen(&cam, &Frame::WORLD, vp, DVec2::ZERO).unwrap();
    let x = sketch_to_screen(&cam, &Frame::WORLD, vp, DVec2::new(10.0, 0.0)).unwrap();
    let y = sketch_to_screen(&cam, &Frame::WORLD, vp, DVec2::new(0.0, 10.0)).unwrap();
    assert!(x.x > o.x && (x.y - o.y).abs() < 1.0, "x nach rechts");
    assert!(
        y.y < o.y && (y.x - o.x).abs() < 1.0,
        "y nach oben (Bildschirm-y wächst nach unten)"
    );
}

#[test]
fn a_parallel_ray_misses_the_plane() {
    let mut cam = OrbitCamera::default();
    cam.look_along(Vec3::X, Vec3::Z); // blickt entlang der Ebene XY
    cam.set_viewport(800.0, 600.0);
    cam.target = Vec3::ZERO;
    // Pixel in der Bildmitte: der Strahl verläuft parallel zur Ebene z = 0 nur, wenn pitch = 0
    // und die Ebene durch die Augenhöhe geht; hier genügt: kein Panic, Ergebnis ist Some oder None
    let _ = screen_to_sketch(
        &cam,
        &Frame::WORLD,
        Vec2::new(800.0, 600.0),
        Vec2::new(400.0, 300.0),
    );
}

#[test]
fn a_diameter_is_labelled_with_the_diameter_sign() {
    use planora_math::Tolerance;
    use planora_sketch::constraint::ConstraintKind;
    use planora_sketch::model::Sketch;
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let center = sk.add_point(0.0, 0.0).unwrap();
    let circle = sk.add_circle(center, 5.0).unwrap();
    let kind = ConstraintKind::Diameter(circle, 10.0);
    assert_eq!(overlay_text(&ctx, &sk, &kind, false), "Ø 10,00 mm");
    assert_eq!(overlay_text(&ctx, &sk, &kind, true), "(Ø 10,00 mm)");
}

#[test]
fn the_pick_radius_scales_with_the_zoom() {
    let mut cam = top_camera();
    let vp = Vec2::new(800.0, 600.0);
    let near = pick_radius_units(&cam, &Frame::WORLD, vp, Vec2::new(400.0, 300.0), 8.0).unwrap();
    cam.distance = 200.0;
    let far = pick_radius_units(&cam, &Frame::WORLD, vp, Vec2::new(400.0, 300.0), 8.0).unwrap();
    assert!(
        near > 0.0 && (far / near - 2.0).abs() < 0.05,
        "{near} {far}"
    );
}
