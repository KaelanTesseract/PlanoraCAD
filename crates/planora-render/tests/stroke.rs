//! Skizzenlinien auf der CPU: Breite in Pixeln, Strichelung, Abschneiden, Tiefenversatz.

use glam::{DVec3, Mat4, Vec2, Vec3};
use planora_render::{
    FACE_SLOPE_SCALE, STROKE_DEPTH_BIAS, StrokeVertex, StrokeView, push_fill, push_stroke,
    push_stroke_phased,
};

const SIZE: Vec2 = Vec2::new(1000.0, 500.0);

/// Ansicht, in der NDC x/y direkt Pixel mal 2 / Größe sind (Einheitsmatrix).
fn flat_view() -> StrokeView {
    StrokeView {
        view_proj: Mat4::IDENTITY,
        size_px: SIZE,
        depth_bias: 0.0,
        pixels_per_point: 1.0,
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

/// Strichpunkt (Mittellinie, Skizzen 12): zwei versetzte Muster ergeben 12 Strich, 3 Lücke,
/// 2 Strich, 3 Lücke wie egui `dashed_line_with_offset(&[12, 2], &[3, 3])`.
#[test]
fn phased_dashes_build_a_centerline() {
    let view = flat_view();
    let mut out = Vec::new();
    let path = [
        DVec3::new(ndc_x(0.0), 0.0, 0.5),
        DVec3::new(ndc_x(40.0), 0.0, 0.5),
    ];
    push_stroke_phased(
        &mut out,
        &view,
        &path,
        1.0,
        [1.0; 4],
        Some([12.0, 8.0]),
        0.0,
    );
    push_stroke_phased(
        &mut out,
        &view,
        &path,
        1.0,
        [1.0; 4],
        Some([2.0, 18.0]),
        5.0,
    );
    let dashes = merged_dashes(&out);
    let want = [(0.0, 12.0), (15.0, 17.0), (20.0, 32.0), (35.0, 37.0)];
    assert_eq!(dashes.len(), want.len(), "{dashes:?}");
    for (d, w) in dashes.iter().zip(want) {
        assert!(
            (d.0 - w.0).abs() < 1e-3 && (d.1 - w.1).abs() < 1e-3,
            "{dashes:?}"
        );
    }
}

fn behind_view() -> StrokeView {
    StrokeView {
        view_proj: glam::camera::rh::proj::directx::perspective(0.8, 2.0, 0.1, 1000.0)
            * glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y),
        size_px: SIZE,
        depth_bias: 0.0,
        pixels_per_point: 1.0,
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

fn projection(distance: f32) -> Mat4 {
    // Wie `OrbitCamera::near_far`: near = 1 % des Abstands, far = 100 * Abstand + 10000.
    let near = (distance * 0.01).max(0.001);
    let far = distance * 100.0 + 10_000.0;
    glam::camera::rh::proj::directx::perspective(0.8, 2.0, near, far)
}

/// Kamera im Bahnabstand `distance` über der Ebene z = 0, `elevation_deg` = Winkel der
/// Blickrichtung zur Ebene.
fn camera(distance: f32, elevation_deg: f32) -> Mat4 {
    let el = elevation_deg.to_radians();
    let eye = Vec3::new(el.cos(), 0.0, el.sin()) * distance;
    projection(distance) * glam::camera::rh::view::look_at_mat4(eye, Vec3::ZERO, Vec3::Z)
}

/// Kamera mit Bahnabstand `orbit`, die senkrecht auf die Ebene z = 0 schaut, das Auge im
/// Abstand `eye_dist` (Fläche in anderer Entfernung als das Ziel).
fn camera_at(orbit: f32, eye_dist: f32) -> Mat4 {
    projection(orbit)
        * glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, eye_dist), Vec3::ZERO, Vec3::Y)
}

#[test]
fn stroke_bias_covers_half_width_at_grazing_angles() {
    let width = 1.2_f32;
    let half = width / 2.0;
    for distance in [1.0_f32, 100.0, 10_000.0] {
        // 85, 87, 88 und 89 Grad zur Ebenennormalen.
        for elevation in [5.0_f32, 3.0, 2.0, 1.0] {
            let view_proj = camera(distance, elevation);
            let view = StrokeView {
                view_proj,
                size_px: SIZE,
                depth_bias: STROKE_DEPTH_BIAS,
                pixels_per_point: 1.0,
            };
            let d = f64::from(distance);
            let mut out = Vec::new();
            let path = [
                DVec3::new(0.0, -0.2 * d, 0.0),
                DVec3::new(0.0, 0.2 * d, 0.0),
            ];
            push_stroke(&mut out, &view, &path, width, [1.0; 4], None);
            let face_z = depth_of(view_proj, Vec3::ZERO);
            let stroke_z = out.iter().map(|v| v.ndc[2]).fold(f32::MAX, f32::min);
            let bias = face_z - stroke_z;
            assert!(
                bias > 0.0,
                "Linie muss vor der Fläche liegen: d = {distance}"
            );

            // Tiefengefälle der Fläche je Pixel (größte Richtung = quer zur Linie).
            let h = 1e-3 * distance;
            let project = |p: Vec3| {
                let c = view_proj * p.extend(1.0);
                (Vec2::new(c.x, c.y) / c.w * SIZE * 0.5, c.z / c.w)
            };
            let (p0, z0) = project(Vec3::ZERO);
            let (p1, z1) = project(Vec3::new(h, 0.0, 0.0));
            let slope = (z1 - z0).abs() / (p1 - p0).length();
            assert!(slope > 0.0);

            // Seitenpixel der Linie liegen bis `half` Pixel von der Mitte; dort kann die
            // Fläche um `half * slope` näher an der Kamera liegen als in der Mitte. Der
            // Versatz allein reicht bei streifendem Blick nicht (das ist so gewollt), mit dem
            // Hardware-Bias der Flächen (slope_scale mal Gefälle) ist es gedeckt.
            let worst = half * slope;
            assert!(
                bias + FACE_SLOPE_SCALE * slope >= worst,
                "d = {distance}, {elevation} Grad: Versatz {bias} + Gefälle {slope} reichen nicht"
            );
            // Der Hardware-Bias allein trägt mit Reserve (mindestens das Doppelte des
            // Bedarfs), damit der Versatz nicht allein die Last tragen muss.
            assert!(
                FACE_SLOPE_SCALE * slope >= 2.0 * worst * 0.99,
                "d = {distance}"
            );
        }
    }
}

#[test]
fn stroke_bias_keeps_coplanar_line_in_front() {
    // Der Versatz muss die f32-Genauigkeit der Tiefe klar übersteigen.
    for distance in [1.0_f32, 100.0, 10_000.0] {
        let view_proj = camera(distance, 5.0);
        let view = StrokeView {
            view_proj,
            size_px: SIZE,
            depth_bias: STROKE_DEPTH_BIAS,
            pixels_per_point: 1.0,
        };
        let d = f64::from(distance);
        let mut out = Vec::new();
        let path = [
            DVec3::new(0.0, -0.2 * d, 0.0),
            DVec3::new(0.0, 0.2 * d, 0.0),
        ];
        push_stroke(&mut out, &view, &path, 1.2, [1.0; 4], None);
        let face_z = depth_of(view_proj, Vec3::ZERO);
        for v in &out {
            assert!(
                face_z - v.ndc[2] > 20.0 * f32::EPSILON,
                "d = {distance}: {} gegen {face_z}",
                v.ndc[2]
            );
        }
    }
}

#[test]
fn stroke_bias_does_not_reveal_hidden_lines() {
    // (Bahnabstand, Abstand der Fläche vom Auge, Tiefe der Linie hinter der Fläche).
    let cases = [
        (100.0_f32, 100.0_f32, 1.0_f32), // 1 mm hinter der Fläche bei 100 mm
        (100.0, 30.0, 0.3),              // nah: 1 % des Abstands
        (100.0, 300.0, 1.0),             // dreifacher Abstand, dünne Wand von 1 % des Bahnabstands
        (10_000.0, 30_000.0, 100.0),
    ];
    for (orbit, eye_dist, hidden) in cases {
        let view_proj = camera_at(orbit, eye_dist);
        let view = StrokeView {
            view_proj,
            size_px: SIZE,
            depth_bias: STROKE_DEPTH_BIAS,
            pixels_per_point: 1.0,
        };
        let face_z = depth_of(view_proj, Vec3::ZERO);
        let mut out = Vec::new();
        let y = f64::from(hidden);
        let path = [DVec3::new(-5.0, 0.0, -y), DVec3::new(5.0, 0.0, -y)];
        push_stroke(&mut out, &view, &path, 1.0, [1.0; 4], None);
        assert!(!out.is_empty());
        for v in &out {
            assert!(
                v.ndc[2] > face_z,
                "Linie {hidden} hinter der Fläche scheint durch (Bahn {orbit}, Auge {eye_dist}): {} <= {face_z}",
                v.ndc[2]
            );
        }
    }
}

#[test]
fn dash_terminates_on_long_segments_with_offset_phase() {
    // Früher hing die Schleife durch f32-Rundung: [4.8, 2.4] auf langer Strecke.
    for (pattern, leads) in [
        ([4.8_f32, 2.4_f32], [0.0_f32, 1.3, 37.7]),
        ([6.0, 4.0], [0.5, 3.3, 123.45]),
    ] {
        for lead in leads {
            let view = flat_view();
            let mut out = Vec::new();
            // Erst ein Stück der Länge `lead` (verschiebt die Phase), dann 800 px.
            let path = [
                DVec3::new(ndc_x(-400.0 - f64::from(lead)), 0.0, 0.5),
                DVec3::new(ndc_x(-400.0), 0.0, 0.5),
                DVec3::new(ndc_x(400.0), 0.0, 0.5),
            ];
            push_stroke(&mut out, &view, &path, 1.0, [1.0; 4], Some(pattern));
            let period = pattern[0] + pattern[1];
            let expected = (800.0 + lead) / period;
            let n = merged_dashes(&out).len() as f32;
            assert!(
                (n - expected).abs() <= 2.0,
                "{pattern:?} lead {lead}: {n} Striche, erwartet {expected}"
            );
        }
    }
}

#[test]
fn dash_stays_dashed_on_huge_segments_clipped_to_viewport() {
    let size = Vec2::new(1920.0, 1080.0);
    let view = StrokeView {
        size_px: size,
        ..flat_view()
    };
    let mut out = Vec::new();
    // 200 000 px lang, quert das Bild.
    let half = 100_000.0 * 2.0 / 1920.0;
    let path = [DVec3::new(-half, 0.0, 0.5), DVec3::new(half, 0.0, 0.5)];
    push_stroke(&mut out, &view, &path, 1.0, [1.0; 4], Some([6.0, 4.0]));
    assert!(!out.is_empty());
    assert!(out.len() < 6 * 1000, "zu viele Striche: {}", out.len());
    let px_x = |v: &StrokeVertex| v.ndc[0] * size.x / 2.0;
    let mut spans: Vec<(f32, f32)> = out
        .chunks(6)
        .map(|q| {
            let xs: Vec<f32> = q.iter().map(px_x).collect();
            (
                xs.iter().copied().fold(f32::MAX, f32::min),
                xs.iter().copied().fold(f32::MIN, f32::max),
            )
        })
        .filter(|(a, b)| *a > -900.0 && *b < 900.0)
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(spans.len() > 150, "nur {} Striche im Bild", spans.len());
    // Innerhalb des Bildes sind es Striche mit Lücken: je 6 px, Anfänge im Abstand 10 px.
    for s in &spans {
        assert!((s.1 - s.0 - 6.0).abs() < 0.05, "{s:?}");
    }
    for w in spans.windows(2) {
        assert!((w[1].0 - w[0].0 - 10.0).abs() < 0.05, "{w:?}");
    }
    // Die Phase läuft über den abgeschnittenen Teil weiter: -100000 ist ein Vielfaches von 10.
    let rem = (spans[0].0 + 100_000.0).rem_euclid(10.0);
    assert!(!(0.05..=9.95).contains(&rem), "Phase verschoben: {rem}");
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
