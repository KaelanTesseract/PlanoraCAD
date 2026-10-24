//! Eigener Rasterer für die Miniaturen: Einpassen, Verdecken, Schattierung, Farben, PNG.

use planora_app::theme;
use planora_app::thumbnail::{
    MARGIN, MAX_PIXELS, MAX_SIDE, Projector, RENDER_COST_BUDGET, SUPERSAMPLE, THUMB_H, THUMB_SCALE,
    THUMB_W, ThumbColors, ThumbView, decode_png, encode_png, render, render_cost, thumb_colors,
};
use planora_math::DVec3;
use planora_mesh::Mesh;
use planora_render::OrbitCamera;

const W: u32 = THUMB_W * THUMB_SCALE;
const H: u32 = THUMB_H * THUMB_SCALE;

/// Netz aus Punkten und Dreiecken. Die Normalen sind absichtlich falsch (alle gleich): Der
/// Rasterer muss sie aus den Eckpunkten berechnen.
fn mesh(positions: Vec<DVec3>, indices: Vec<u32>) -> Mesh {
    let normals = vec![DVec3::Z; positions.len()];
    Mesh {
        positions,
        normals,
        indices,
        ..Mesh::default()
    }
}

/// Achsparalleler Würfel um `center` mit halber Kantenlänge `h`, je Fläche eigene Punkte.
fn cube(center: DVec3, h: f64) -> Mesh {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let n = DVec3::from_array({
                let mut a = [0.0; 3];
                a[axis] = sign;
                a
            });
            let u = DVec3::from_array({
                let mut a = [0.0; 3];
                a[(axis + 1) % 3] = 1.0;
                a
            });
            let v = n.cross(u);
            let base = positions.len() as u32;
            for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                positions.push(center + (n + u * su + v * sv) * h);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    mesh(positions, indices)
}

/// Quadrat um `center` in der Ebene senkrecht zu `normal` (Achse), halbe Kante `h`.
fn square(center: DVec3, axis: usize, h: f64) -> Mesh {
    let mut u = [0.0; 3];
    u[(axis + 1) % 3] = h;
    let mut v = [0.0; 3];
    v[(axis + 2) % 3] = h;
    let (u, v) = (DVec3::from_array(u), DVec3::from_array(v));
    mesh(
        vec![
            center - u - v,
            center + u - v,
            center + u + v,
            center - u + v,
        ],
        vec![0, 1, 2, 0, 2, 3],
    )
}

fn start() -> ThumbView {
    ThumbView::start()
}

fn colors() -> ThumbColors {
    thumb_colors(true)
}

fn pixel(img: &planora_app::thumbnail::Image, x: i64, y: i64) -> [u8; 4] {
    assert!(x >= 0 && y >= 0 && x < img.width as i64 && y < img.height as i64);
    let i = ((y as u32 * img.width + x as u32) * 4) as usize;
    [
        img.rgba[i],
        img.rgba[i + 1],
        img.rgba[i + 2],
        img.rgba[i + 3],
    ]
}

fn at(img: &planora_app::thumbnail::Image, p: (f32, f32)) -> [u8; 4] {
    pixel(img, p.0.floor() as i64, p.1.floor() as i64)
}

/// Anteil der Pixel, die nicht Hintergrund sind.
fn body_fraction(img: &planora_app::thumbnail::Image, bg: [u8; 4]) -> f64 {
    let n = img.rgba.chunks(4).filter(|p| *p != bg).count();
    n as f64 / (img.width * img.height) as f64
}

/// Blickrichtung zum Betrachter (wie `OrbitCamera::eye` ohne Abstand).
fn eye_dir() -> DVec3 {
    let v = ThumbView::start();
    let (sp, cp) = (v.pitch as f64).sin_cos();
    let (sy, cy) = (v.yaw as f64).sin_cos();
    DVec3::new(cp * cy, cp * sy, sp)
}

#[test]
fn an_empty_scene_is_only_background() {
    let c = colors();
    let img = render(&[], start(), &c, W, H);
    assert!(img.rgba.chunks(4).all(|p| p == c.background));
    assert_eq!((img.width, img.height), (496, 280));
    assert_eq!(img.rgba.len(), 496 * 280 * 4);
    let empty = Mesh::default();
    let img2 = render(&[&empty], start(), &c, W, H);
    assert_eq!(img, img2);
}

#[test]
fn the_projector_fits_with_the_margin() {
    assert_eq!(MARGIN, 0.08);
    assert_eq!(SUPERSAMPLE, 2);
    for (name, m) in [
        ("Würfel", cube(DVec3::new(3.0, -2.0, 5.0), 1.0)),
        // Flach und breit: hier begrenzt die Breite, beim Würfel die Höhe.
        ("Platte", {
            let mut s = square(DVec3::ZERO, 2, 1.0);
            for p in &mut s.positions {
                p.x *= 20.0;
                p.y *= 20.0;
            }
            s
        }),
        // Hoch und schmal: hier muss die Höhe begrenzen.
        ("Säule", {
            let mut c = cube(DVec3::ZERO, 1.0);
            for p in &mut c.positions {
                p.z *= 30.0;
            }
            c
        }),
    ] {
        let proj = Projector::fit(&[&m], start(), W, H).expect("endliche Punkte");
        let margin = 0.08 * 280.0;
        let pts: Vec<(f32, f32, f32)> = m.positions.iter().map(|p| proj.project(*p)).collect();
        let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for &(x, y, _) in &pts {
            assert!(
                x >= margin - 0.01 && x <= W as f32 - margin + 0.01,
                "{name}: x = {x}"
            );
            assert!(
                y >= margin - 0.01 && y <= H as f32 - margin + 0.01,
                "{name}: y = {y}"
            );
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
        }
        let touches_x = (x0 - margin).abs() <= 1.0 && (x1 - (W as f32 - margin)).abs() <= 1.0;
        let touches_y = (y0 - margin).abs() <= 1.0 && (y1 - (H as f32 - margin)).abs() <= 1.0;
        assert!(
            touches_x || touches_y,
            "{name}: berührt keinen Rand: {x0}..{x1} × {y0}..{y1}"
        );
    }
    // Oben in der Welt ist oben im Bild; der nähere Punkt hat die größere Tiefe.
    let c = cube(DVec3::ZERO, 1.0);
    let proj = Projector::fit(&[&c], start(), W, H).unwrap();
    let top = proj.project(DVec3::new(0.0, 0.0, 1.0));
    let bottom = proj.project(DVec3::new(0.0, 0.0, -1.0));
    assert!(top.1 < bottom.1, "oben {top:?}, unten {bottom:?}");
    let near = proj.project(eye_dir() * 0.5);
    let far = proj.project(-eye_dir() * 0.5);
    assert!(near.2 > far.2, "nah {near:?}, fern {far:?}");
    // Die Mitte der Punktwolke liegt in der Bildmitte.
    let mid = proj.project(DVec3::ZERO);
    assert!((mid.0 - W as f32 / 2.0).abs() < 0.01 && (mid.1 - H as f32 / 2.0).abs() < 0.01);
}

#[test]
fn corners_land_where_the_projector_says() {
    let c = colors();
    let tri = mesh(
        vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(10.0, 2.0, 1.0),
            DVec3::new(3.0, -1.0, 9.0),
        ],
        vec![0, 1, 2],
    );
    let proj = Projector::fit(&[&tri], start(), W, H).unwrap();
    let img = render(&[&tri], start(), &c, W, H);
    let p: Vec<(f32, f32)> = tri
        .positions
        .iter()
        .map(|q| {
            let (x, y, _) = proj.project(*q);
            (x, y)
        })
        .collect();
    let inner = at(&img, {
        let (x, y) = (
            (p[0].0 + p[1].0 + p[2].0) / 3.0,
            (p[0].1 + p[1].1 + p[2].1) / 3.0,
        );
        (x, y)
    });
    assert_ne!(inner, c.background);
    for i in 0..3 {
        let (a, b, cc) = (p[i], p[(i + 1) % 3], p[(i + 2) % 3]);
        let unit = |d: (f32, f32)| {
            let l = (d.0 * d.0 + d.1 * d.1).sqrt();
            (d.0 / l, d.1 / l)
        };
        let e1 = unit((b.0 - a.0, b.1 - a.1));
        let e2 = unit((cc.0 - a.0, cc.1 - a.1));
        let bis = unit((e1.0 + e2.0, e1.1 + e2.1));
        let half_angle = ((e1.0 * e2.0 + e1.1 * e2.1).clamp(-1.0, 1.0).acos()) / 2.0;
        // 2 px Abstand zu beiden Kanten der Ecke.
        let d_in = 2.0 / half_angle.sin();
        let inside = (a.0 + bis.0 * d_in, a.1 + bis.1 * d_in);
        let outside = (a.0 - bis.0 * 3.0, a.1 - bis.1 * 3.0);
        assert_eq!(at(&img, inside), inner, "Ecke {i} innen {inside:?}");
        assert_eq!(
            at(&img, outside),
            c.background,
            "Ecke {i} außen {outside:?}"
        );
    }
}

#[test]
fn the_nearer_face_wins_in_any_order() {
    let c = colors();
    let e = eye_dir();
    // A liegt näher am Betrachter und steht senkrecht zu x, B weiter hinten senkrecht zu z.
    let a = square(e * 2.0, 0, 1.0);
    let b = square(-e * 2.0, 2, 1.0);
    let ab = render(&[&a, &b], start(), &c, W, H);
    let ba = render(&[&b, &a], start(), &c, W, H);
    assert_eq!(ab, ba);
    // Auch innerhalb eines Netzes entscheidet die Tiefe, nicht die Reihenfolge.
    let merged = |first: &Mesh, second: &Mesh| {
        let mut pos = first.positions.clone();
        pos.extend(second.positions.iter().copied());
        let mut idx = first.indices.clone();
        idx.extend(
            second
                .indices
                .iter()
                .map(|i| i + first.positions.len() as u32),
        );
        mesh(pos, idx)
    };
    let one_ab = render(&[&merged(&a, &b)], start(), &c, W, H);
    let one_ba = render(&[&merged(&b, &a)], start(), &c, W, H);
    assert_eq!(one_ab, one_ba);
    assert_eq!(one_ab, ab);
    let centre = pixel(&ab, W as i64 / 2, H as i64 / 2);
    let only_a = render(&[&a], start(), &c, W, H);
    let only_b = render(&[&b], start(), &c, W, H);
    let shade_a = pixel(&only_a, W as i64 / 2, H as i64 / 2);
    let shade_b = pixel(&only_b, W as i64 / 2, H as i64 / 2);
    assert_ne!(
        shade_a, shade_b,
        "A und B müssen verschieden schattiert sein"
    );
    assert_eq!(centre, shade_a);
}

#[test]
fn three_visible_cube_faces_have_three_shades() {
    for dark in [true, false] {
        let c = thumb_colors(dark);
        let m = cube(DVec3::ZERO, 1.0);
        let proj = Projector::fit(&[&m], start(), W, H).unwrap();
        let img = render(&[&m], start(), &c, W, H);
        // Startblick: von vorne-rechts-oben, also +x, −y und +z sichtbar.
        let faces = [
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(0.0, -1.0, 0.0),
            DVec3::new(0.0, 0.0, 1.0),
        ];
        let shades: Vec<[u8; 4]> = faces
            .iter()
            .map(|f| {
                let (x, y, _) = proj.project(*f);
                at(&img, (x, y))
            })
            .collect();
        for s in &shades {
            assert_ne!(*s, c.background);
            assert_eq!(s[3], 255);
        }
        assert_ne!(shades[0], shades[1]);
        assert_ne!(shades[1], shades[2]);
        assert_ne!(shades[0], shades[2]);
        // Helligkeit 0,35 + 0,65·|n·l| der Körperfarbe: jede Fläche höchstens so hell wie die
        // Körperfarbe und mindestens 35 % davon.
        for s in &shades {
            for k in 0..3 {
                let body = f32::from(c.body[k]);
                assert!(f32::from(s[k]) <= body + 0.5, "{s:?} {:?}", c.body);
                assert!(f32::from(s[k]) >= 0.35 * body - 0.5, "{s:?} {:?}", c.body);
            }
        }
        // Die Deckfläche ist dem Licht (von oben) am nächsten zugewandt: am hellsten.
        let lum = |s: [u8; 4]| u32::from(s[0]) + u32::from(s[1]) + u32::from(s[2]);
        assert!(lum(shades[2]) > lum(shades[0]) && lum(shades[2]) > lum(shades[1]));
    }
}

#[test]
fn both_sides_are_lit_the_same() {
    let c = colors();
    let front = cube(DVec3::ZERO, 1.0);
    let mut back = front.clone();
    for t in back.indices.chunks_mut(3) {
        t.swap(1, 2);
    }
    let a = render(&[&front], start(), &c, W, H);
    let b = render(&[&back], start(), &c, W, H);
    assert_eq!(a, b);
}

#[test]
fn the_order_of_the_corners_changes_nothing() {
    // Jede Kante zählt gleich: ein Dreieck mit vertauschten Ecken deckt genau dieselben Pixel.
    let c = colors();
    let pts = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(10.0, 2.0, 1.0),
        DVec3::new(3.0, -1.0, 9.0),
    ];
    let reference = render(&[&mesh(pts.clone(), vec![0, 1, 2])], start(), &c, W, H);
    for order in [[1, 2, 0], [2, 0, 1], [0, 2, 1], [2, 1, 0], [1, 0, 2]] {
        let img = render(&[&mesh(pts.clone(), order.to_vec())], start(), &c, W, H);
        assert_eq!(img, reference, "{order:?}");
    }
}

#[test]
fn edges_are_smoothed_and_output_is_deterministic() {
    let c = colors();
    let m = cube(DVec3::ZERO, 1.0);
    let img = render(&[&m], start(), &c, W, H);
    assert_eq!(img, render(&[&m], start(), &c, W, H));
    // Unterabtastung: an der Umrisskante gibt es Mischfarben zwischen Hintergrund und Körper.
    let proj = Projector::fit(&[&m], start(), W, H).unwrap();
    let faces: Vec<[u8; 4]> = [DVec3::X, -DVec3::Y, DVec3::Z]
        .iter()
        .map(|f| {
            let (x, y, _) = proj.project(*f);
            at(&img, (x, y))
        })
        .collect();
    let mixed = img
        .rgba
        .chunks(4)
        .filter(|p| *p != c.background && !faces.iter().any(|f| f == p))
        .count();
    assert!(mixed > 50, "nur {mixed} Mischpixel");
}

#[test]
fn the_view_is_the_start_view_of_the_main_window() {
    assert_eq!(
        ThumbView::start(),
        ThumbView {
            yaw: OrbitCamera::default().yaw,
            pitch: OrbitCamera::default().pitch
        }
    );
}

#[test]
fn degenerate_and_non_finite_input_does_not_panic() {
    let c = colors();
    let plain = cube(DVec3::ZERO, 1.0);
    let plain_img = render(&[&plain], start(), &c, W, H);
    // Würfel mit nicht endlichen Punkten, Index außerhalb, Fläche 0 und unvollständigem Dreieck.
    let mut bad = plain.clone();
    let n = bad.positions.len() as u32;
    bad.positions.push(DVec3::new(f64::NAN, 0.0, 0.0));
    bad.positions.push(DVec3::new(f64::INFINITY, 1.0, 1.0));
    bad.positions.push(DVec3::new(0.0, f64::NEG_INFINITY, 0.0));
    bad.indices.extend([
        n,
        0,
        1,
        0,
        n + 1,
        2,
        n + 2,
        3,
        4,
        0,
        1,
        99_999,
        0,
        0,
        0,
        0,
        1,
    ]);
    let bad_img = render(&[&bad], start(), &c, W, H);
    assert_eq!(
        bad_img, plain_img,
        "nicht endliche Punkte dürfen nichts ändern"
    );
    // Nur NaN: Hintergrund und kein Projektor.
    let nan = mesh(
        vec![
            DVec3::splat(f64::NAN),
            DVec3::new(f64::NAN, 1.0, 2.0),
            DVec3::ONE * f64::INFINITY,
        ],
        vec![0, 1, 2],
    );
    assert!(Projector::fit(&[&nan], start(), W, H).is_none());
    let img = render(&[&nan], start(), &c, W, H);
    assert!(img.rgba.chunks(4).all(|p| p == c.background));
    // Riesige Koordinaten neben kleinen.
    let mut huge = plain.clone();
    let k = huge.positions.len() as u32;
    huge.positions.push(DVec3::new(1e300, -1e300, 1e300));
    huge.positions.push(DVec3::new(-1e308, 1e308, -1e308));
    huge.positions.push(DVec3::new(1e308, 1e308, 1e308));
    huge.indices.extend([k, k + 1, k + 2, 0, 1, k]);
    let img = render(&[&huge], start(), &c, W, H);
    assert_eq!(img.rgba.len(), 496 * 280 * 4);
    // Ein einzelner Punkt, ein Strich, Dreiecke der Fläche 0.
    let point = mesh(vec![DVec3::ONE; 3], vec![0, 1, 2]);
    let line = mesh(vec![DVec3::ZERO, DVec3::X, DVec3::X * 2.0], vec![0, 1, 2]);
    for m in [&point, &line] {
        assert!(Projector::fit(&[m], start(), W, H).is_some());
        let img = render(&[m], start(), &c, W, H);
        assert!(img.rgba.chunks(4).all(|p| p == c.background));
    }
    // Sehr kleine und leere Bildgrößen.
    for (w, h) in [(0, 0), (1, 1), (0, 5), (3, 0), (2, 1)] {
        let img = render(&[&plain], start(), &c, w, h);
        assert_eq!(img.rgba.len(), (img.width * img.height * 4) as usize);
        assert_eq!((img.width, img.height), (w, h));
    }
    // Nicht endliche Blickwinkel.
    let odd = ThumbView {
        yaw: f32::NAN,
        pitch: f32::INFINITY,
    };
    let img = render(&[&plain], odd, &c, W, H);
    assert_eq!(img.rgba.len(), 496 * 280 * 4);
}

#[test]
fn huge_and_tiny_bodies_both_fill_the_image() {
    let c = colors();
    let reference = body_fraction(
        &render(&[&cube(DVec3::ZERO, 1.0)], start(), &c, W, H),
        c.background,
    );
    assert!(reference > 0.2, "{reference}");
    for (h, centre) in [
        (1e-6, DVec3::new(1e-6, 2e-6, -3e-6)),
        (1e6, DVec3::new(5e6, -1e6, 2e6)),
    ] {
        let img = render(&[&cube(centre, h)], start(), &c, W, H);
        let f = body_fraction(&img, c.background);
        assert!(
            (f - reference).abs() <= 0.02,
            "h = {h}: {f} statt {reference}"
        );
    }
    // Sehr dünner Körper (Platte 100 × 100 × 0,001) bleibt sichtbar.
    let mut thin = cube(DVec3::ZERO, 1.0);
    for p in &mut thin.positions {
        p.x *= 50.0;
        p.y *= 50.0;
        p.z *= 0.0005;
    }
    let f = body_fraction(&render(&[&thin], start(), &c, W, H), c.background);
    assert!(f > 0.1, "dünne Platte: {f}");
}

#[test]
fn colours_follow_the_theme() {
    for dark in [true, false] {
        let c = thumb_colors(dark);
        assert_eq!(c.background, theme::thumbnail_background(dark).to_array());
        assert_eq!(
            theme::thumbnail_background(dark),
            theme::palette(dark).bg_inset
        );
        let face = planora_app::viewport::render_palette(dark).face_base;
        let expected = face.map(|v| (v * 255.0).round() as u8);
        assert_eq!(c.body, expected);
        let img = render(&[&cube(DVec3::ZERO, 1.0)], start(), &c, W, H);
        assert_eq!(pixel(&img, 0, 0), c.background);
    }
    assert_ne!(thumb_colors(true), thumb_colors(false));
    assert_ne!(
        thumb_colors(true).background,
        thumb_colors(false).background
    );
    assert_ne!(thumb_colors(true).body, thumb_colors(false).body);
}

#[test]
fn png_roundtrip_and_strict_decode() {
    let c = colors();
    let img = render(&[&cube(DVec3::ZERO, 1.0)], start(), &c, W, H);
    let bytes = encode_png(&img).expect("PNG schreiben");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(decode_png(&bytes, W, H).expect("PNG lesen"), img);
    // Falsche Größe.
    assert!(decode_png(&bytes, W - 1, H).is_err());
    assert!(decode_png(&bytes, W, H + 1).is_err());
    assert!(decode_png(&bytes, H, W).is_err());
    // Abgeschnitten, leer, Unsinn.
    assert!(decode_png(&bytes[..bytes.len() / 2], W, H).is_err());
    assert!(decode_png(&bytes[..bytes.len() - 13], W, H).is_err());
    assert!(decode_png(&[], W, H).is_err());
    assert!(decode_png(b"kein PNG", W, H).is_err());
    // Jedes andere Format der richtigen Größe wird abgelehnt, auch solche mit 4 Byte je Pixel
    // (Grau + Alpha in 16 Bit), die dieselbe Datenmenge wie RGBA mit 8 Bit haben.
    use png::{BitDepth, ColorType};
    for (color, depth, bytes_per_px) in [
        (ColorType::Grayscale, BitDepth::Eight, 1),
        (ColorType::Grayscale, BitDepth::Sixteen, 2),
        (ColorType::GrayscaleAlpha, BitDepth::Eight, 2),
        (ColorType::GrayscaleAlpha, BitDepth::Sixteen, 4),
        (ColorType::Rgb, BitDepth::Eight, 3),
        (ColorType::Rgba, BitDepth::Sixteen, 8),
        (ColorType::Indexed, BitDepth::Eight, 1),
    ] {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, W, H);
            enc.set_color(color);
            enc.set_depth(depth);
            if color == ColorType::Indexed {
                enc.set_palette((0..=255u8).flat_map(|v| [v, v, v]).collect::<Vec<u8>>());
            }
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&vec![128u8; (W * H * bytes_per_px) as usize])
                .unwrap();
        }
        assert!(
            decode_png(&out, W, H).is_err(),
            "{color:?} {depth:?} wurde angenommen"
        );
    }
    // Ein Bild mit falscher Puffergröße lässt sich nicht schreiben.
    let broken = planora_app::thumbnail::Image {
        width: 4,
        height: 4,
        rgba: vec![0; 10],
    };
    assert!(encode_png(&broken).is_err());
}

/// Kamera der Hauptansicht im Startblick: Weltrichtung → Kameraraum (x rechts, y oben, z zum
/// Betrachter), unabhängig vom Rasterer aus `OrbitCamera::view` gerechnet.
fn view_dir(d: DVec3) -> DVec3 {
    let v = OrbitCamera::default()
        .view()
        .transform_vector3(glam::Vec3::new(d.x as f32, d.y as f32, d.z as f32));
    DVec3::new(f64::from(v.x), f64::from(v.y), f64::from(v.z))
}

/// Nur Punkte, keine Dreiecke: legt das Einpassen fest, ohne etwas zu zeichnen.
fn frame(h: f64) -> Mesh {
    let mut c = cube(DVec3::ZERO, h);
    c.indices.clear();
    c
}

/// Rechts- und Oben-Richtung des Startblicks in Weltkoordinaten (Bildebene).
fn screen_axes() -> (DVec3, DVec3) {
    let yaw = f64::from(start().yaw);
    let right = DVec3::new(-yaw.sin(), yaw.cos(), 0.0);
    (right, eye_dir().cross(right))
}

#[test]
fn the_image_is_not_mirrored_or_upside_down() {
    // Kleine Würfel auf den Achsen, eingepasst in einen Rahmen um den Ursprung: Wo sie im
    // gezeichneten Bild landen, hängt nicht vom Projektor des Tests ab.
    let c = colors();
    let (cx, cy) = (W / 2, H / 2);
    for (name, p) in [
        ("+X rechts", DVec3::new(0.9, 0.0, 0.0)),
        ("-Y links", DVec3::new(0.0, -0.9, 0.0)),
        ("+Z oben", DVec3::new(0.0, 0.0, 0.9)),
    ] {
        let img = render(&[&frame(1.0), &cube(p, 0.05)], start(), &c, W, H);
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for y in 0..H {
            for x in 0..W {
                if pixel(&img, x.into(), y.into()) != c.background {
                    xs.push(x);
                    ys.push(y);
                }
            }
        }
        assert!(!xs.is_empty(), "{name}: nichts gezeichnet");
        let ok = match name {
            "+X rechts" => xs.iter().all(|&x| x > cx + 20),
            "-Y links" => xs.iter().all(|&x| x + 20 < cx),
            _ => ys.iter().all(|&y| y + 20 < cy),
        };
        assert!(
            ok,
            "{name}: x {:?}..{:?}, y {:?}..{:?}",
            xs.iter().min(),
            xs.iter().max(),
            ys.iter().min(),
            ys.iter().max()
        );
    }
    // Abbildung = Sicht der Hauptkamera: Bild-x = s · Sicht-x, Bild-y = −s · Sicht-y (s > 0),
    // Tiefe gleichsinnig mit Sicht-z.
    let cubem = cube(DVec3::ZERO, 1.0);
    let proj = Projector::fit(&[&cubem], start(), W, H).unwrap();
    let o = proj.project(DVec3::ZERO);
    let rel = |p: DVec3| {
        let q = proj.project(p);
        (
            f64::from(q.0 - o.0),
            f64::from(q.1 - o.1),
            f64::from(q.2 - o.2),
        )
    };
    let s = rel(DVec3::X).0 / view_dir(DVec3::X).x;
    assert!(s > 1.0, "Maßstab {s}");
    for p in [
        DVec3::X,
        DVec3::Y,
        DVec3::Z,
        DVec3::new(1.0, 2.0, 3.0),
        DVec3::new(-2.0, 1.0, 0.5),
    ] {
        let (ix, iy, iz) = rel(p);
        let v = view_dir(p);
        assert!(
            (ix - s * v.x).abs() < 1e-3 * s,
            "x {p}: {ix} statt {}",
            s * v.x
        );
        assert!(
            (iy + s * v.y).abs() < 1e-3 * s,
            "y {p}: {iy} statt {}",
            -s * v.y
        );
        if v.z.abs() > 1e-3 {
            assert_eq!(iz > 0.0, v.z > 0.0, "Tiefe {p}");
        }
    }
}

/// Erwartete Farbe einer Fläche mit Weltnormale `n`: Körperfarbe · (0,35 + 0,65·|n·l|), n im
/// Kameraraum der Hauptkamera, l = norm(−0,3; 0,5; 1).
fn expected_shade(c: &ThumbColors, n: DVec3) -> [f64; 3] {
    let l = DVec3::new(-0.3, 0.5, 1.0).normalize();
    let k = 0.35 + 0.65 * view_dir(n).normalize().dot(l).abs();
    [0, 1, 2].map(|i| f64::from(c.body[i]) * k)
}

fn assert_shade(name: &str, got: [u8; 4], want: [f64; 3]) {
    for i in 0..3 {
        assert!(
            (f64::from(got[i]) - want[i]).abs() <= 1.0,
            "{name}: {got:?} statt {want:?}"
        );
    }
}

#[test]
fn known_faces_get_the_expected_brightness() {
    for dark in [true, false] {
        let c = thumb_colors(dark);
        // Quadrat senkrecht zum Blick: |n·l| = 1/|l| ≈ 0,8575.
        let (right, up) = screen_axes();
        let sq = mesh(
            vec![-right - up, right - up, right + up, -right + up],
            vec![0, 1, 2, 0, 2, 3],
        );
        let img = render(&[&sq], start(), &c, W, H);
        let want = expected_shade(&c, eye_dir());
        let direct = 0.35 + 0.65 / DVec3::new(-0.3, 0.5, 1.0).length();
        for (w, b) in want.iter().zip(c.body) {
            assert!((w - f64::from(b) * direct).abs() < 0.01);
        }
        assert_shade("Quadrat", pixel(&img, W as i64 / 2, H as i64 / 2), want);
        // Die drei sichtbaren Würfelflächen.
        let m = cube(DVec3::ZERO, 1.0);
        let proj = Projector::fit(&[&m], start(), W, H).unwrap();
        let img = render(&[&m], start(), &c, W, H);
        for (name, n) in [("+x", DVec3::X), ("-y", -DVec3::Y), ("+z", DVec3::Z)] {
            let (x, y, _) = proj.project(n);
            assert_shade(name, at(&img, (x, y)), expected_shade(&c, n));
        }
    }
}

fn crack_colors() -> ThumbColors {
    ThumbColors {
        background: [10, 20, 30, 0],
        body: colors().body,
    }
}

#[test]
fn no_cracks_between_neighbouring_triangles() {
    let c = crack_colors();
    // 1) Gitter, dessen Ecken und Kanten genau auf Abtastmitten liegen: Blick entlang −x
    //    (yaw = pitch = 0, Bild-x = Welt-y, Bild-y = −Welt-z) und Maßstab genau 0,25 px je
    //    Einheit (Ausdehnung = 4 × halbe nutzbare Breite). Ungerade Koordinaten → Abtastmitten.
    let view = ThumbView {
        yaw: 0.0,
        pitch: 0.0,
    };
    let margin = f64::from(MARGIN) * 100.0;
    let hi = (100.0 - 2.0 * margin) / 2.0 * 4.0;
    let mut pos = vec![DVec3::new(0.0, -hi, -hi), DVec3::new(0.0, hi, hi)];
    let ys: Vec<f64> = (-50..=50).map(|k| f64::from(2 * k + 1)).collect();
    let zs: Vec<f64> = (-30..=30).map(|k| f64::from(2 * k + 1)).collect();
    let first = pos.len() as u32;
    for &z in &zs {
        for &y in &ys {
            pos.push(DVec3::new(0.0, y, z));
        }
    }
    let nx = ys.len() as u32;
    let mut idx = Vec::new();
    for r in 0..zs.len() as u32 - 1 {
        for q in 0..nx - 1 {
            let a = first + r * nx + q;
            idx.extend([a, a + 1, a + nx + 1, a, a + nx + 1, a + nx]);
        }
    }
    let grid = mesh(pos, idx);
    let proj = Projector::fit(&[&grid], view, 100, 100).unwrap();
    // Ecke (y, z) = (−99, −59) liegt genau auf der Abtastmitte (50,5; 129,5).
    let corner = proj.project(DVec3::new(0.0, ys[0], zs[0]));
    assert_eq!(
        (corner.0 * 2.0, corner.1 * 2.0),
        (50.5, 129.5),
        "Ausrichtung"
    );
    let img = render(&[&grid], view, &c, 100, 100);
    for y in 36..=63 {
        for x in 26..=73 {
            assert_eq!(pixel(&img, x, y)[3], 255, "Riss bei ({x}, {y})");
        }
    }

    // 2) Offenes, gestörtes, nicht ebenes Gitter 60 × 60 in der Bildebene des Startblicks.
    let (right, up) = screen_axes();
    let back = eye_dir();
    let mut seed = 12345u64;
    let mut rnd = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 11) as f64 / (1u64 << 53) as f64) - 0.5
    };
    let n = 60u32;
    let mut pos = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            let border = i == 0 || j == 0 || i == n || j == n;
            let (mut u, mut v) = (f64::from(i) - 30.0, (f64::from(j) - 30.0) * 0.5);
            if !border {
                u += rnd() * 0.6;
                v += rnd() * 0.3;
            }
            let d = (u * 0.3).sin() * (v * 0.5).cos() * 3.0 + rnd();
            pos.push(right * u + up * v + back * d);
        }
    }
    let mut idx = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let a = j * (n + 1) + i;
            let b = a + n + 1;
            idx.extend([a, a + 1, b + 1, a, b + 1, b]);
        }
    }
    let grid = mesh(pos, idx);
    let proj = Projector::fit(&[&grid], start(), W, H).unwrap();
    let p0 = proj.project(right * -30.0 + up * -15.0);
    let p1 = proj.project(right * 30.0 + up * 15.0);
    let (x0, x1) = (p0.0.min(p1.0) + 2.0, p0.0.max(p1.0) - 2.0);
    let (y0, y1) = (p0.1.min(p1.1) + 2.0, p0.1.max(p1.1) - 2.0);
    let img = render(&[&grid], start(), &c, W, H);
    let mut checked = 0;
    for y in 0..H {
        for x in 0..W {
            let (fx, fy) = (x as f32, y as f32);
            if fx >= x0 && fx + 1.0 <= x1 && fy >= y0 && fy + 1.0 <= y1 {
                assert_eq!(
                    pixel(&img, x.into(), y.into())[3],
                    255,
                    "Riss bei ({x}, {y})"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 50_000, "{checked}");

    // 3) Fächer aus 20 000 Splittern um einen Mittelpunkt (nicht ebene Scheibe).
    let k = 20_000u32;
    let mut pos = vec![DVec3::ZERO];
    for s in 0..k {
        let t = std::f64::consts::TAU * f64::from(s) / f64::from(k);
        pos.push(right * (10.0 * t.cos()) + up * (10.0 * t.sin()) + back * (0.5 * (7.0 * t).sin()));
    }
    let idx: Vec<u32> = (0..k).flat_map(|s| [0, 1 + s, 1 + (s + 1) % k]).collect();
    let fan = mesh(pos, idx);
    let proj = Projector::fit(&[&fan], start(), W, H).unwrap();
    let centre = proj.project(DVec3::ZERO);
    let rim = proj.project(right * 10.0);
    let r = (rim.0 - centre.0).hypot(rim.1 - centre.1) - 2.0;
    let img = render(&[&fan], start(), &c, W, H);
    let mut checked = 0;
    for y in 0..H {
        for x in 0..W {
            let inside = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
                .iter()
                .all(|(dx, dy)| (x as f32 + dx - centre.0).hypot(y as f32 + dy - centre.1) < r);
            if inside {
                assert_eq!(
                    pixel(&img, x.into(), y.into())[3],
                    255,
                    "Riss bei ({x}, {y})"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 30_000, "{checked}");
}

#[test]
fn image_size_is_capped() {
    assert_eq!(MAX_SIDE, 1024);
    assert!(u64::from(W) * u64::from(H) <= MAX_PIXELS);
    assert!(MAX_PIXELS < u64::from(MAX_SIDE) * u64::from(MAX_SIDE));
    let c = colors();
    let m = cube(DVec3::ZERO, 1.0);
    let capped_h = (MAX_PIXELS / 1024) as u32;
    for (w, h, want) in [
        (5000, 10, (1024, 10)),
        (10, 5000, (10, 1024)),
        (1024, 1024, (1024, capped_h)),
        (u32::MAX, u32::MAX, (1024, capped_h)),
        (W, H, (W, H)),
    ] {
        let img = render(&[&m], start(), &c, w, h);
        assert_eq!((img.width, img.height), want, "{w} × {h}");
        assert!(u64::from(img.width) * u64::from(img.height) <= MAX_PIXELS);
        assert_eq!(img.rgba.len(), (img.width * img.height * 4) as usize);
        assert!(render_cost(&[&m], start(), w, h) > 0);
    }
}

fn sphere(rings: u32, segs: u32) -> Mesh {
    let mut pos = Vec::new();
    for i in 0..=rings {
        let th = std::f64::consts::PI * f64::from(i) / f64::from(rings);
        for j in 0..segs {
            let ph = std::f64::consts::TAU * f64::from(j) / f64::from(segs);
            pos.push(DVec3::new(th.sin() * ph.cos(), th.sin() * ph.sin(), th.cos()) * 10.0);
        }
    }
    let mut idx = Vec::new();
    for i in 0..rings {
        for j in 0..segs {
            let a = i * segs + j;
            let b = i * segs + (j + 1) % segs;
            idx.extend([a, a + segs, b, b, a + segs, b + segs]);
        }
    }
    mesh(pos, idx)
}

/// Fein zerlegter Stab (Länge 1000, Radius 5) mit `n` Segmenten: 2·n bildlange Dreiecke.
fn rod(n: u32) -> Mesh {
    let mut pos = Vec::new();
    for k in 0..n {
        let a = std::f64::consts::TAU * f64::from(k) / f64::from(n);
        pos.push(DVec3::new(-500.0, 5.0 * a.cos(), 5.0 * a.sin()));
        pos.push(DVec3::new(500.0, 5.0 * a.cos(), 5.0 * a.sin()));
    }
    let mut idx = Vec::new();
    for k in 0..n {
        let (a, b) = (2 * k, 2 * ((k + 1) % n));
        idx.extend([a, a + 1, b, b, a + 1, b + 1]);
    }
    mesh(pos, idx)
}

#[test]
fn render_cost_is_zero_when_nothing_is_drawn() {
    let empty = Mesh::default();
    let nan = mesh(vec![DVec3::splat(f64::NAN); 3], vec![0, 1, 2]);
    let point = mesh(vec![DVec3::ONE; 3], vec![0, 1, 2]);
    let line = mesh(vec![DVec3::ZERO, DVec3::X, DVec3::X * 2.0], vec![0, 1, 2]);
    let outside = mesh(vec![DVec3::ZERO, DVec3::X, DVec3::Z], vec![0, 1, 7]);
    assert_eq!(render_cost(&[], start(), W, H), 0);
    for m in [&empty, &nan, &point, &line, &outside, &frame(1.0)] {
        assert_eq!(render_cost(&[m], start(), W, H), 0);
    }
    let c = cube(DVec3::ZERO, 1.0);
    assert_eq!(render_cost(&[&c], start(), 0, H), 0);
    assert_eq!(render_cost(&[&c], start(), W, 0), 0);
    assert!(render_cost(&[&c], start(), W, H) > 0);
}

#[test]
fn render_cost_of_a_known_triangle() {
    // Blick entlang −x, Bild 100 × 100, Rand 8 px: Das Dreieck füllt 84 × 84 px, also 168
    // Abtastzeilen, und deckt halb so viele Abtastpunkte wie sein Rechteck (168² / 2 = 14 112).
    // Kosten = Zeilen + Abtastpunkte / 10 = 168 + 1411.
    let view = ThumbView {
        yaw: 0.0,
        pitch: 0.0,
    };
    let t = mesh(
        vec![
            DVec3::new(0.0, -1.0, -1.0),
            DVec3::new(0.0, 1.0, -1.0),
            DVec3::new(0.0, -1.0, 1.0),
        ],
        vec![0, 1, 2],
    );
    assert_eq!(render_cost(&[&t], view, 100, 100), 168 + 1411);
    // Doppelt gezeichnet kostet doppelt, ein zweites Netz zählt mit.
    let mut twice = t.clone();
    twice.indices.extend([2, 1, 0]);
    assert_eq!(render_cost(&[&twice], view, 100, 100), 2 * (168 + 1411));
    assert_eq!(render_cost(&[&t, &t], view, 100, 100), 2 * (168 + 1411));
}

#[test]
fn render_cost_flags_the_pathological_cases_and_the_budget_is_fast() {
    assert_eq!(RENDER_COST_BUDGET, 20_000_000);
    let c = colors();
    // Kugel aus 200 000 Dreiecken: weit unter dem Budget, schnell.
    let ball = sphere(250, 400);
    assert_eq!(ball.indices.len() / 3, 200_000);
    let cost_ball = render_cost(&[&ball], start(), W, H);
    let t = std::time::Instant::now();
    let img = render(&[&ball], start(), &c, W, H);
    let ms_ball = t.elapsed().as_millis();
    assert!(body_fraction(&img, c.background) > 0.2);
    // Stab mit 100 000 Segmenten (200 000 bildlange Dreiecke): über dem Budget.
    let cost_rod = render_cost(&[&rod(100_000)], start(), W, H);
    // 200 000 Splitter, die über die ganze Bildhöhe laufen: über dem Budget.
    let mut pos = Vec::new();
    let mut idx = Vec::new();
    for i in 0..200_000u32 {
        let x = -50.0 + f64::from(i) * 0.0005;
        let b = pos.len() as u32;
        pos.extend([
            DVec3::new(x, 0.0, -50.0),
            DVec3::new(x + 100.0, 0.0, 50.0),
            DVec3::new(x + 100.0005, 0.0, 50.0),
        ]);
        idx.extend([b, b + 1, b + 2]);
    }
    let cost_slivers = render_cost(&[&mesh(pos, idx)], start(), W, H);
    // Stab so fein, dass er gerade noch ins Budget passt: muss in vertretbarer Zeit fertig sein.
    let n_fit = (100_000.0 * RENDER_COST_BUDGET as f64 / cost_rod as f64 * 0.95) as u32;
    let fit = rod(n_fit);
    let cost_fit = render_cost(&[&fit], start(), W, H);
    let t = std::time::Instant::now();
    let img = render(&[&fit], start(), &c, W, H);
    let ms_fit = t.elapsed().as_millis();
    eprintln!(
        "Kosten: Kugel {cost_ball} ({ms_ball} ms), Stab 100 000 {cost_rod}, Splitter {cost_slivers}, Stab {n_fit} {cost_fit} ({ms_fit} ms)"
    );
    assert!(cost_ball * 10 < RENDER_COST_BUDGET, "{cost_ball}");
    assert!(cost_rod > RENDER_COST_BUDGET, "{cost_rod}");
    assert!(cost_slivers > RENDER_COST_BUDGET, "{cost_slivers}");
    assert!(cost_fit <= RENDER_COST_BUDGET, "{cost_fit}");
    assert!(body_fraction(&img, c.background) > 0.005);
    assert!(ms_ball < 10_000, "{ms_ball} ms");
    assert!(ms_fit < 10_000, "{ms_fit} ms");
}
