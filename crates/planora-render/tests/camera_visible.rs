//! Kamera mit sichtbarem Bereich: Die Bildmitte liegt in der Mitte des freien Rechtecks
//! zwischen den Feldern (Verschiebung in normierten Gerätekoordinaten nach der Perspektive).

use glam::{Vec2, Vec3};
use planora_render::OrbitCamera;

const FULL: Vec2 = Vec2::new(1000.0, 600.0);
const VIS_MIN: Vec2 = Vec2::new(250.0, 80.0);
const VIS_SIZE: Vec2 = Vec2::new(500.0, 400.0);

fn cam() -> OrbitCamera {
    OrbitCamera {
        target: Vec3::new(10.0, 20.0, 5.0),
        distance: 100.0,
        yaw: 0.6,
        pitch: 0.4,
        ..OrbitCamera::default()
    }
}

fn visible_cam() -> OrbitCamera {
    let mut c = cam();
    c.set_visible(FULL, (VIS_MIN, VIS_SIZE));
    c
}

/// Einfacher Zufallsgenerator (deterministisch, ohne Abhängigkeit).
fn lcg(seed: &mut u64) -> f32 {
    *seed = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    ((*seed >> 33) as f32) / ((1u64 << 31) as f32)
}

#[test]
fn set_visible_sets_the_fields() {
    let c = visible_cam();
    assert!((c.aspect - FULL.x / FULL.y).abs() < 1e-6);
    assert!((c.visible_aspect - VIS_SIZE.x / VIS_SIZE.y).abs() < 1e-6);
    assert!((c.visible_height_fraction - VIS_SIZE.y / FULL.y).abs() < 1e-6);
    let mut d = c;
    d.clear_visible();
    assert_eq!(d.ndc_offset, Vec2::ZERO);
    assert_eq!(d.visible_aspect, d.aspect);
    assert_eq!(d.visible_height_fraction, 1.0);
}

#[test]
fn target_projects_to_the_centre_of_the_visible_rect() {
    let c = visible_cam();
    let px = c.project(c.target, FULL).unwrap();
    assert!(
        (px - Vec2::new(500.0, 280.0)).length() < 1e-3,
        "{px:?} statt (500, 280)"
    );
}

#[test]
fn ray_and_project_agree_with_offset() {
    let c = visible_cam();
    let mut seed = 7;
    for _ in 0..20 {
        let p = c.target
            + Vec3::new(
                lcg(&mut seed) * 60.0 - 30.0,
                lcg(&mut seed) * 60.0 - 30.0,
                lcg(&mut seed) * 60.0 - 30.0,
            );
        let px = c.project(p, FULL).unwrap();
        let ndc = Vec2::new(px.x / FULL.x * 2.0 - 1.0, 1.0 - px.y / FULL.y * 2.0);
        let (origin, dir) = c.ray(ndc);
        let to_p = p - origin;
        let off_axis = to_p - dir * to_p.dot(dir);
        assert!(off_axis.length() < 1e-4, "{p:?}: Abstand {off_axis:?}");
    }
}

#[test]
fn zero_offset_changes_nothing() {
    let before = cam();
    // Bisherige Rechnung ohne Versatz, bitgleich.
    let (near, far) = (
        (before.distance * 0.01).max(0.001),
        before.distance * 100.0 + 10_000.0,
    );
    let old = glam::camera::rh::proj::directx::perspective(before.fov_y, before.aspect, near, far)
        * before.view();
    assert_eq!(before.view_proj().to_cols_array(), old.to_cols_array());
    let mut c = before;
    c.set_visible(FULL, (VIS_MIN, VIS_SIZE));
    c.aspect = before.aspect;
    c.clear_visible();
    assert_eq!(
        c.view_proj().to_cols_array(),
        before.view_proj().to_cols_array()
    );
    let ndc = before.view_proj().project_point3(before.target);
    assert!(ndc.x.abs() < 1e-4 && ndc.y.abs() < 1e-4);
    // Ohne Versatz folgt das sichtbare Seitenverhältnis dem Fenster.
    let mut d = OrbitCamera::default();
    d.set_viewport(300.0, 100.0);
    assert!((d.visible_aspect - 3.0).abs() < 1e-6);
    // Mit Versatz bleibt das freie Rechteck stehen.
    let mut e = visible_cam();
    e.set_viewport(800.0, 600.0);
    assert!((e.visible_aspect - VIS_SIZE.x / VIS_SIZE.y).abs() < 1e-6);
}

#[test]
fn frame_bounds_fits_the_box_inside_the_visible_rect() {
    let mut c = visible_cam();
    let (min, max) = (Vec3::splat(-50.0), Vec3::splat(50.0));
    c.frame_bounds(min, max);
    let vis_max = VIS_MIN + VIS_SIZE;
    let mut closest = f32::MAX;
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        );
        let px = c.project(corner, FULL).unwrap();
        let margin = (px - VIS_MIN).min(vis_max - px).min_element();
        assert!(margin >= 0.0, "Ecke {corner:?} bei {px:?} außerhalb");
        let rel = ((px - VIS_MIN) / VIS_SIZE)
            .min((vis_max - px) / VIS_SIZE)
            .min_element();
        closest = closest.min(rel);
    }
    assert!(closest <= 0.15, "Würfel zu klein im Bild: Rand {closest}");
}

#[test]
fn pan_scale_is_unchanged_by_the_offset() {
    let mut plain = cam();
    plain.aspect = FULL.x / FULL.y;
    let mut offset = visible_cam();
    plain.pan(30.0, -12.0, FULL.y);
    offset.pan(30.0, -12.0, FULL.y);
    assert!((plain.target - offset.target).length() < 1e-5);
}
