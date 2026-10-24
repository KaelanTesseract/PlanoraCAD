//! Eigener Rasterer für die Miniaturen: Einpassen, Verdecken, Schattierung, Farben, PNG.

use planora_app::theme;
use planora_app::thumbnail::{
    MARGIN, Projector, SUPERSAMPLE, THUMB_H, THUMB_SCALE, THUMB_W, ThumbColors, ThumbView,
    decode_png, encode_png, render, thumb_colors,
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
    // Graustufen- und RGB-PNG der richtigen Größe werden abgelehnt.
    for color in [png::ColorType::Grayscale, png::ColorType::Rgb] {
        let ch = if color == png::ColorType::Rgb { 3 } else { 1 };
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, W, H);
            enc.set_color(color);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&vec![128u8; (W * H * ch) as usize])
                .unwrap();
        }
        assert!(decode_png(&out, W, H).is_err(), "{color:?}");
    }
    // Ein Bild mit falscher Puffergröße lässt sich nicht schreiben.
    let broken = planora_app::thumbnail::Image {
        width: 4,
        height: 4,
        rgba: vec![0; 10],
    };
    assert!(encode_png(&broken).is_err());
}

#[test]
fn two_hundred_thousand_triangles_render_in_reasonable_time() {
    // Kugel aus 200 000 Dreiecken (P8: Obergrenze der Miniatur).
    let (rings, segs) = (250u32, 400u32);
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
    assert_eq!(idx.len() / 3, 200_000);
    let m = mesh(pos, idx);
    let c = colors();
    let t = std::time::Instant::now();
    let img = render(&[&m], start(), &c, W, H);
    let ms = t.elapsed().as_millis();
    eprintln!("200 000 Dreiecke: {ms} ms");
    assert!(body_fraction(&img, c.background) > 0.2);
    assert!(ms < 10_000, "{ms} ms");
    // Ungünstigster Fall: 200 000 lange schmale Schrägstreifen über das ganze Bild.
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
    let slivers = mesh(pos, idx);
    let t = std::time::Instant::now();
    let img = render(&[&slivers], start(), &c, W, H);
    let ms = t.elapsed().as_millis();
    eprintln!("200 000 schmale Streifen: {ms} ms");
    assert!(body_fraction(&img, c.background) > 0.05);
    assert!(ms < 20_000, "{ms} ms");
}
