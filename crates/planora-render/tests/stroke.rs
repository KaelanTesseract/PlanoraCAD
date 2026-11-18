//! Skizzenlinien auf der CPU: Breite in Pixeln, Strichelung, Abschneiden, Tiefenversatz.

use glam::{DVec3, Mat4, Vec2, Vec3};
use planora_render::{STROKE_DEPTH_BIAS, StrokeVertex, StrokeView, push_fill, push_stroke};

const SIZE: Vec2 = Vec2::new(1000.0, 500.0);

/// Ansicht, in der NDC x/y direkt Pixel mal 2 / Größe sind (Einheitsmatrix).
fn flat_view() -> StrokeView {
    StrokeView {
        view_proj: Mat4::IDENTITY,
        size_px: SIZE,
        depth_bias: 0.0,
    }
}

/// NDC-Punkt in Pixel (Ursprung Bildmitte).
fn px(v: &StrokeVertex) -> Vec2 {
    Vec2::new(v.ndc[0] * SIZE.x / 2.0, v.ndc[1] * SIZE.y / 2.0)
}

fn ndc_x(px: f64) -> f64 {
    px * 2.0 / f64::from(SIZE.x)
}

#[test]
fn stroke_width_in_pixels() {
    let view = flat_view();
    let mut out = Vec::new();
    // 100 px lang, waagerecht.
    let path = [
        DVec3::new(ndc_x(-50.0), 0.0, 0.5),
        DVec3::new(ndc_x(50.0), 0.0, 0.5),
    ];
    push_stroke(&mut out, &view, &path, 2.0, [1.0; 4], None);
    assert_eq!(out.len(), 6, "ein Viereck = zwei Dreiecke");
    let ys: Vec<f32> = out.iter().map(|v| px(v).y).collect();
    let lo = ys.iter().copied().fold(f32::MAX, f32::min);
    let hi = ys.iter().copied().fold(f32::MIN, f32::max);
    assert!((hi - lo - 2.0).abs() < 1e-3, "Höhe {}", hi - lo);
    let xs: Vec<f32> = out.iter().map(|v| px(v).x).collect();
    let l = xs.iter().copied().fold(f32::MAX, f32::min);
    let h = xs.iter().copied().fold(f32::MIN, f32::max);
    assert!((h - l - 100.0).abs() < 1e-3);
    // Querkoordinate liegt bei -1 und 1.
    assert!(out.iter().all(|v| (v.edge.abs() - 1.0).abs() < 1e-6));
}

/// Fasst die Vierecke einer waagerechten Strichelung zu Strichen (von, bis) in Pixeln.
fn merged_dashes(out: &[StrokeVertex]) -> Vec<(f32, f32)> {
    let mut spans: Vec<(f32, f32)> = out
        .chunks(6)
        .map(|q| {
            let xs: Vec<f32> = q.iter().map(|v| px(v).x).collect();
            (
                xs.iter().copied().fold(f32::MAX, f32::min),
                xs.iter().copied().fold(f32::MIN, f32::max),
            )
        })
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f32, f32)> = Vec::new();
    for s in spans {
        match merged.last_mut() {
            Some(last) if (s.0 - last.1).abs() < 1e-3 => last.1 = s.1,
            _ => merged.push(s),
        }
    }
    merged
}

#[test]
fn dash_continues_across_vertices() {
    let view = flat_view();
    let mut out = Vec::new();
    // Vier Stücke à 5 px.
    let path: Vec<DVec3> = (0..=4)
        .map(|i| DVec3::new(ndc_x(f64::from(i) * 5.0), 0.0, 0.5))
        .collect();
    push_stroke(&mut out, &view, &path, 1.0, [1.0; 4], Some([6.0, 4.0]));
    let dashes = merged_dashes(&out);
    assert_eq!(dashes.len(), 2, "{dashes:?}");
    assert!((dashes[0].0 - 0.0).abs() < 1e-3 && (dashes[0].1 - 6.0).abs() < 1e-3);
    assert!((dashes[1].0 - 10.0).abs() < 1e-3 && (dashes[1].1 - 16.0).abs() < 1e-3);
    // Je Strich über einen Eckpunkt zwei Vierecke, also vier Vierecke insgesamt.
    assert_eq!(out.len(), 4 * 6);
}

fn behind_view() -> StrokeView {
    StrokeView {
        view_proj: glam::camera::rh::proj::directx::perspective(0.8, 2.0, 0.1, 1000.0)
            * glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y),
        size_px: SIZE,
        depth_bias: 0.0,
    }
}

#[test]
fn stroke_clipped_behind_camera() {
    let view = behind_view();
    let mut out = Vec::new();
    // Ein Ende vor der Kamera, das andere weit dahinter (z = 50 liegt hinter dem Auge).
    let path = [DVec3::new(1.0, 0.0, 0.0), DVec3::new(1.0, 0.0, 50.0)];
    push_stroke(&mut out, &view, &path, 2.0, [1.0; 4], None);
    assert!(!out.is_empty());
    for v in &out {
        assert!(v.ndc.iter().all(|c| c.is_finite()), "{:?}", v.ndc);
        assert!((0.0..=1.0 + 1e-4).contains(&v.ndc[2]), "z {}", v.ndc[2]);
    }
    // Gestrichelt ebenso ohne NaN und ohne Strichflut.
    let mut dashed = Vec::new();
    push_stroke(&mut dashed, &view, &path, 2.0, [1.0; 4], Some([6.0, 4.0]));
    assert!(dashed.iter().all(|v| v.ndc.iter().all(|c| c.is_finite())));
    assert!(dashed.len() < 100_000);
    // Beide Enden hinter der Kamera: nichts.
    let mut none = Vec::new();
    let behind = [DVec3::new(0.0, 0.0, 20.0), DVec3::new(1.0, 0.0, 30.0)];
    push_stroke(&mut none, &view, &behind, 2.0, [1.0; 4], Some([6.0, 4.0]));
    assert!(none.is_empty());
}

/// Tiefe (NDC z) eines Punktes mit reiner `view_proj`, wie die Flächen sie schreiben.
fn depth_of(view_proj: Mat4, p: Vec3) -> f32 {
    let c = view_proj * p.extend(1.0);
    c.z / c.w
}

/// Kamera wie `OrbitCamera::near_far`: near = 1 % des Abstands, far = 100 * Abstand + 10000.
/// `elevation_deg` ist der Winkel der Blickrichtung zur Ebene z = 0.
fn camera(distance: f32, elevation_deg: f32) -> Mat4 {
    let near = (distance * 0.01).max(0.001);
    let far = distance * 100.0 + 10_000.0;
    let el = elevation_deg.to_radians();
    let eye = Vec3::new(el.cos(), 0.0, el.sin()) * distance;
    let proj = glam::camera::rh::proj::directx::perspective(0.8, 16.0 / 9.0, near, far);
    proj * glam::camera::rh::view::look_at_mat4(eye, Vec3::ZERO, Vec3::Z)
}

#[test]
fn stroke_bias_keeps_coplanar_line_in_front() {
    // Der Versatz muss die f32-Genauigkeit der Tiefe klar übersteigen.
    const { assert!(STROKE_DEPTH_BIAS > 20.0 * f32::EPSILON) };
    for distance in [1.0_f32, 10.0, 100.0, 1000.0, 10_000.0] {
        // 85° zur Ebenennormalen (Normale = z) = 5° über der Ebene.
        let view_proj = camera(distance, 5.0);
        let view = StrokeView {
            view_proj,
            size_px: SIZE,
            depth_bias: STROKE_DEPTH_BIAS,
        };
        let mut out = Vec::new();
        let d = f64::from(distance);
        // Linie in der Ebene z = 0, quer zur Blickrichtung (y), also gleiche Tiefe entlang.
        let path = [
            DVec3::new(0.0, -0.2 * d, 0.0),
            DVec3::new(0.0, 0.2 * d, 0.0),
        ];
        push_stroke(&mut out, &view, &path, 1.2, [1.0; 4], None);
        assert!(!out.is_empty());
        let face_z = depth_of(view_proj, Vec3::ZERO);
        let line_z = depth_of(view_proj, Vec3::ZERO) - STROKE_DEPTH_BIAS;
        assert!(line_z < face_z, "d = {distance}");
        // Die Tiefe der erzeugten Eckpunkte entspricht der Flächentiefe minus Versatz
        // (Mitte der Linie liegt bei x = 0, dort hat die Fläche face_z).
        let mid = out.iter().map(|v| v.ndc[2]).fold(f32::MAX, f32::min);
        assert!(mid < face_z, "d = {distance}: {mid} >= {face_z}");
    }
}

#[test]
fn stroke_bias_does_not_reveal_hidden_lines() {
    // 1 mm hinter der Fläche bei 100 mm Abstand, und 1 % des Abstands bei allen anderen.
    for (distance, hidden) in [
        (100.0_f32, 1.0_f32),
        (10.0, 0.1),
        (1000.0, 10.0),
        (10_000.0, 100.0),
    ] {
        let view_proj = camera(distance, 89.0);
        let face_z = depth_of(view_proj, Vec3::ZERO);
        let behind_z = depth_of(view_proj, Vec3::new(0.0, 0.0, -hidden));
        assert!(
            behind_z - STROKE_DEPTH_BIAS > face_z,
            "Linie {hidden} hinter der Fläche scheint durch bei d = {distance}"
        );
    }
}

#[test]
fn fill_triangles_projected() {
    let view = flat_view();
    let mut out = Vec::new();
    let tri = [
        DVec3::new(0.0, 0.0, 0.25),
        DVec3::new(0.5, 0.0, 0.25),
        DVec3::new(0.0, 0.5, 0.25),
    ];
    push_fill(&mut out, &view, &[tri], [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(out.len(), 3);
    assert_eq!(out[1].ndc, [0.5, 0.0, 0.25]);
    assert_eq!(out[2].ndc, [0.0, 0.5, 0.25]);
    assert!(
        out.iter()
            .all(|v| v.edge == 0.0 && v.color == [0.1, 0.2, 0.3, 0.4])
    );
    // Dreieck komplett hinter der Kamera fällt weg.
    let mut none = Vec::new();
    let behind = [
        DVec3::new(0.0, 0.0, 20.0),
        DVec3::new(1.0, 0.0, 20.0),
        DVec3::new(0.0, 1.0, 20.0),
    ];
    push_fill(&mut none, &behind_view(), &[behind], [1.0; 4]);
    assert!(none.is_empty());
}
