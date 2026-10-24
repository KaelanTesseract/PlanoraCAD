//! Eigener kleiner Rasterer für die Miniaturen im Projekte-Fenster: zeichnet Dreiecksnetze ohne
//! GPU in ein RGBA-Bild. Parallele Projektion im Startblick der Hauptansicht, alle Körper mit Rand
//! eingepasst, beidseitig flach schattiert in der Körperfarbe des Programms, 2 × 2 Unterabtastung
//! gegen Treppenkanten. Die Ausgabe hängt nur von den Eingaben ab (gleiches Bild bei gleichem
//! Netz); unbrauchbare Eingaben (nicht endliche Punkte, Fläche 0, Indizes außerhalb) werden
//! übergangen, nie mit einer Panik beantwortet.

use planora_math::DVec3;
use planora_mesh::Mesh;
use planora_render::OrbitCamera;

use crate::{theme, viewport};

/// Breite der Miniatur auf der Kachel (Punkte).
pub const THUMB_W: u32 = 248;
/// Höhe der Miniatur auf der Kachel (Punkte).
pub const THUMB_H: u32 = 140;
/// Auflösungsfaktor der Datei (496 × 280 Pixel, scharf auf hochauflösenden Bildschirmen).
pub const THUMB_SCALE: u32 = 2;
/// Rand als Anteil der kürzeren Bildseite.
pub const MARGIN: f32 = 0.08;
/// Unterabtastung je Achse und Pixel.
pub const SUPERSAMPLE: u32 = 2;

/// Größte Bildseite in Pixeln; größere Angaben werden darauf begrenzt (Speicher).
const MAX_SIDE: u32 = 4096;
/// Grundhelligkeit jeder Fläche (P7).
const AMBIENT: f64 = 0.35;
/// Anteil des gerichteten Lichts (P7).
const DIFFUSE: f64 = 0.65;
/// Licht im Kameraraum (x rechts, y oben, z zum Betrachter), vor dem Normieren (P7).
const LIGHT: [f64; 3] = [-0.3, 0.5, 1.0];

/// Farben einer Miniatur als RGBA-Werte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThumbColors {
    pub background: [u8; 4],
    pub body: [u8; 4],
}

/// Farben für Hell (`dark = false`) oder Dunkel: Hintergrund aus dem Theme, Körper in der
/// Flächenfarbe der 3D-Ansicht.
pub fn thumb_colors(dark: bool) -> ThumbColors {
    let face = viewport::render_palette(dark).face_base;
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    ThumbColors {
        background: theme::thumbnail_background(dark).to_array(),
        body: [byte(face[0]), byte(face[1]), byte(face[2]), byte(face[3])],
    }
}

/// Blickwinkel der Miniatur (Bogenmaß, Bedeutung wie bei `OrbitCamera`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThumbView {
    pub yaw: f32,
    pub pitch: f32,
}

impl ThumbView {
    /// Derselbe Blick wie beim Start der Hauptansicht.
    pub fn start() -> ThumbView {
        let cam = OrbitCamera::default();
        ThumbView {
            yaw: cam.yaw,
            pitch: cam.pitch,
        }
    }
}

/// RGBA-Bild, 8 Bit je Kanal, Zeile für Zeile von oben.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Parallele Projektion, alle endlichen Punkte eingepasst mit Rand; None ohne endliche Punkte.
#[derive(Clone, Copy, Debug)]
pub struct Projector {
    right: DVec3,
    up: DVec3,
    back: DVec3,
    mid: DVec3,
    half_depth: f64,
    scale: f64,
    centre_x: f64,
    centre_y: f64,
}

/// Kameraachsen (rechts, oben, zum Betrachter) wie `OrbitCamera::view` mit z nach oben.
fn basis(view: ThumbView) -> (DVec3, DVec3, DVec3) {
    let (sp, cp) = f64::from(view.pitch).sin_cos();
    let (sy, cy) = f64::from(view.yaw).sin_cos();
    let back = DVec3::new(cp * cy, cp * sy, sp);
    let right = DVec3::new(-sy, cy, 0.0);
    let up = back.cross(right);
    (right, up, back)
}

impl Projector {
    pub fn fit(meshes: &[&Mesh], view: ThumbView, width: u32, height: u32) -> Option<Projector> {
        let (right, up, back) = basis(view);
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        let mut any = false;
        for p in meshes.iter().flat_map(|m| m.positions.iter()) {
            let c = DVec3::new(p.dot(right), p.dot(up), p.dot(back));
            if !(p.is_finite() && c.is_finite()) {
                continue;
            }
            any = true;
            lo = lo.min(c);
            hi = hi.max(c);
        }
        if !any {
            return None;
        }
        // Mitte und halbe Ausdehnung ohne Überlauf auch bei Koordinaten nahe f64::MAX.
        let mid = lo * 0.5 + hi * 0.5;
        let half = hi * 0.5 - lo * 0.5;
        let (w, h) = (f64::from(width), f64::from(height));
        let margin = f64::from(MARGIN) * w.min(h);
        let fit_axis = |avail: f64, extent: f64| {
            if extent > 0.0 {
                (avail / 2.0).max(0.0) / extent
            } else {
                f64::INFINITY
            }
        };
        let sx = fit_axis(w - 2.0 * margin, half.x);
        let sy = fit_axis(h - 2.0 * margin, half.y);
        let mut scale = sx.min(sy);
        if !scale.is_finite() {
            scale = 1.0;
        }
        Some(Projector {
            right,
            up,
            back,
            mid,
            half_depth: half.z,
            scale,
            centre_x: w / 2.0,
            centre_y: h / 2.0,
        })
    }

    pub fn project(&self, p: DVec3) -> (f32, f32, f32) {
        let (x, y, z) = self.project_f64(p);
        (x as f32, y as f32, z as f32)
    }

    /// Wie [`Projector::project`] in voller Genauigkeit; Tiefe in −1 … 1 über alle Punkte.
    fn project_f64(&self, p: DVec3) -> (f64, f64, f64) {
        let x = self.centre_x + (p.dot(self.right) - self.mid.x) * self.scale;
        let y = self.centre_y - (p.dot(self.up) - self.mid.y) * self.scale;
        let z = if self.half_depth > 0.0 {
            (p.dot(self.back) - self.mid.z) / self.half_depth
        } else {
            0.0
        };
        (x, y, z)
    }

    /// Richtung `d` (Welt) im Kameraraum.
    fn camera_dir(&self, d: DVec3) -> DVec3 {
        DVec3::new(d.dot(self.right), d.dot(self.up), d.dot(self.back))
    }
}

/// Einheitsnormale des Dreiecks aus seinen Eckpunkten; None bei Fläche 0 oder Überlauf.
fn face_normal(a: DVec3, b: DVec3, c: DVec3) -> Option<DVec3> {
    // Kanten vor dem Kreuzprodukt normieren, damit auch riesige Koordinaten nicht überlaufen.
    let scaled = |e: DVec3| {
        let m = e.abs().max_element();
        (m > 0.0 && m.is_finite()).then(|| e / m)
    };
    let n = scaled(b - a)?.cross(scaled(c - a)?);
    let len = n.length();
    (len > 0.0 && len.is_finite()).then(|| n / len)
}

/// Kantenfunktion: positiv, wenn `p` links der Kante `a → b` liegt (im Bild mit y nach unten).
fn edge(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> f64 {
    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
}

/// Erste gültige Abtastzeile/-spalte für den Bereich `lo..=hi` (Abtastmitten bei `i + 0.5`).
fn sample_range(lo: f64, hi: f64, count: usize) -> Option<(usize, usize)> {
    if count == 0 {
        return None;
    }
    let first = (lo - 0.5).ceil().max(0.0);
    let last = (hi - 0.5).floor().min((count - 1) as f64);
    (first <= last).then_some((first as usize, last as usize))
}

/// Abtastspalten `lo..=hi` innerhalb von `x0..=x1`, in denen die Zeile `py` alle Kanten auf der
/// inneren Seite hat, mit einer Spalte Reserve je Seite; None, wenn die Zeile das Dreieck verfehlt.
fn row_span(
    edges: [((f64, f64), (f64, f64)); 3],
    py: f64,
    x0: usize,
    x1: usize,
) -> Option<(usize, usize)> {
    let (mut lo, mut hi) = (x0 as f64, x1 as f64);
    for (a, b) in edges {
        // edge(a, b, (px, py)) = k·px + m
        let k = -(b.1 - a.1);
        let m = (b.0 - a.0) * (py - a.1) + (b.1 - a.1) * a.0;
        if k == 0.0 {
            if m < 0.0 {
                return None;
            }
            continue;
        }
        let root = -m / k - 0.5;
        if k > 0.0 {
            lo = lo.max(root.ceil() - 1.0);
        } else {
            hi = hi.min(root.floor() + 1.0);
        }
    }
    (lo <= hi).then_some((lo as usize, hi as usize))
}

/// Zeichnet die Netze in ein Bild `width × height` (jede Seite höchstens 4096).
pub fn render(
    meshes: &[&Mesh],
    view: ThumbView,
    colors: &ThumbColors,
    width: u32,
    height: u32,
) -> Image {
    let (width, height) = (width.min(MAX_SIDE), height.min(MAX_SIDE));
    let pixels = width as usize * height as usize;
    let ss = SUPERSAMPLE as usize;
    let per_pixel = ss * ss;
    let mut rgba = Vec::with_capacity(pixels * 4);
    let Some(proj) = Projector::fit(meshes, view, width, height).filter(|_| pixels > 0) else {
        for _ in 0..pixels {
            rgba.extend_from_slice(&colors.background);
        }
        return Image {
            width,
            height,
            rgba,
        };
    };
    // Abtastpunkte zeilenweise je Pixel gruppiert: Pixel (x, y) besitzt `per_pixel` Einträge ab
    // `(y * width + x) * per_pixel`, Abtastung (sx, sy) darin an `sy * ss + sx`.
    let mut depth = vec![f32::NEG_INFINITY; pixels * per_pixel];
    let mut colour = vec![colors.background; pixels * per_pixel];
    let (sw, sh) = (width as usize * ss, height as usize * ss);
    let s = f64::from(SUPERSAMPLE);
    let light = DVec3::from_array(LIGHT).normalize();
    for mesh in meshes {
        for tri in mesh.indices.as_chunks::<3>().0 {
            let corner = |i: u32| mesh.positions.get(i as usize).copied();
            let (Some(pa), Some(pb), Some(pc)) = (corner(tri[0]), corner(tri[1]), corner(tri[2]))
            else {
                continue;
            };
            let Some(n) = face_normal(pa, pb, pc) else {
                continue;
            };
            let n = proj.camera_dir(n);
            let shade = AMBIENT + DIFFUSE * n.dot(light).abs();
            let lit = |k: usize| {
                (f64::from(colors.body[k]) * shade)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            let fill = [lit(0), lit(1), lit(2), colors.body[3]];
            // Eckpunkte im Abtastraster (eine Einheit je Abtastpunkt).
            let to_samples = |p: DVec3| {
                let (x, y, z) = proj.project_f64(p);
                (x * s, y * s, z)
            };
            let (a, mut b, mut c) = (to_samples(pa), to_samples(pb), to_samples(pc));
            if ![a.0, a.1, a.2, b.0, b.1, b.2, c.0, c.1, c.2]
                .iter()
                .all(|v| v.is_finite())
            {
                continue;
            }
            let mut area = edge((a.0, a.1), (b.0, b.1), (c.0, c.1));
            if !area.is_finite() || area == 0.0 {
                continue;
            }
            if area < 0.0 {
                std::mem::swap(&mut b, &mut c);
                area = -area;
            }
            let (Some((x0, x1)), Some((y0, y1))) = (
                sample_range(a.0.min(b.0).min(c.0), a.0.max(b.0).max(c.0), sw),
                sample_range(a.1.min(b.1).min(c.1), a.1.max(b.1).max(c.1), sh),
            ) else {
                continue;
            };
            let (a2, b2, c2) = ((a.0, a.1), (b.0, b.1), (c.0, c.1));
            for j in y0..=y1 {
                let py = j as f64 + 0.5;
                // Nur die Spanne der Zeile innerhalb aller drei Kanten (lange schmale Dreiecke
                // kosten so nicht ihr ganzes Begrenzungsrechteck); je ein Abtastpunkt Reserve,
                // die Kantenfunktionen unten entscheiden genau.
                let Some((lo, hi)) = row_span([(b2, c2), (c2, a2), (a2, b2)], py, x0, x1) else {
                    continue;
                };
                let row = (j / ss) * width as usize;
                let sub_y = (j % ss) * ss;
                for i in lo..=hi {
                    let p = (i as f64 + 0.5, py);
                    let w0 = edge(b2, c2, p);
                    let w1 = edge(c2, a2, p);
                    let w2 = edge(a2, b2, p);
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let z = ((w0 * a.2 + w1 * b.2 + w2 * c.2) / area) as f32;
                    let k = (row + i / ss) * per_pixel + sub_y + i % ss;
                    if z > depth[k] {
                        depth[k] = z;
                        colour[k] = fill;
                    }
                }
            }
        }
    }
    for px in 0..pixels {
        let base = px * per_pixel;
        let samples = &colour[base..base + per_pixel];
        let count = samples.len() as u32;
        for k in 0..4 {
            let sum: u32 = samples.iter().map(|c| u32::from(c[k])).sum();
            rgba.push(((sum + count / 2) / count) as u8);
        }
    }
    Image {
        width,
        height,
        rgba,
    }
}

/// Schreibt das Bild als PNG (RGBA, 8 Bit).
pub fn encode_png(img: &Image) -> Result<Vec<u8>, String> {
    let expected = u64::from(img.width) * u64::from(img.height) * 4;
    if img.rgba.len() as u64 != expected {
        return Err(format!(
            "Bildpuffer hat {} statt {expected} Bytes.",
            img.rgba.len()
        ));
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width, img.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(&img.rgba)
            .map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Nur RGBA8 in genau `width × height` (sonst Fehler); keine anderen Formate.
pub fn decode_png(bytes: &[u8], width: u32, height: u32) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let info = reader.info();
    if info.width != width || info.height != height {
        return Err(format!(
            "Bild hat {} × {} statt {width} × {height} Pixel.",
            info.width, info.height
        ));
    }
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("Bild ist nicht RGBA mit 8 Bit.".to_string());
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| "Bild ist zu groß.".to_string())?;
    let mut buf = vec![0; size];
    let frame = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(frame.buffer_size());
    let expected = width as usize * height as usize * 4;
    // Größe und Format stehen oben schon fest (aus dem Kopf); hier nur die gelesene Datenmenge.
    if buf.len() != expected {
        return Err("Bilddaten passen nicht zur Größe.".to_string());
    }
    Ok(Image {
        width,
        height,
        rgba: buf,
    })
}
