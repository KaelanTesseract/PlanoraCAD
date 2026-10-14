//! glTF 2.0 als `.glb` schreiben (Spec §3, Plan P8): ein Puffer, je Körper ein Mesh mit
//! `POSITION`, `NORMAL` und `indices` (u32), ein Knoten je Mesh; Meter (= mm · 0,001).

use crate::error::ExchangeError;
use crate::normals::{SMOOTH_ANGLE_DEG, Shaded, shade};
use crate::source::ExportBody;
use std::fmt::Write as _;
use std::io::Write;

/// Millimeter → Meter (glTF rechnet in Metern, Spec §2).
pub const GLTF_SCALE: f64 = 0.001;
/// „glTF“ als Zahl (little endian).
const GLB_MAGIC: u32 = 0x4654_6C67;
const GLB_VERSION: u32 = 2;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;
/// Länge des Dateikopfs und eines Blockkopfs in Bytes.
const GLB_HEADER: u64 = 12;
const CHUNK_HEADER: u64 = 8;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;

/// Schreibt alle Körper mit Dreiecken als `.glb`; Körper ohne Dreiecke entfallen.
pub fn write_glb<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError> {
    let parts: Vec<(&str, Shaded)> = bodies
        .iter()
        .filter(|b| b.mesh.triangle_count() > 0)
        .map(|b| (b.name.as_str(), in_metres(shade(&b.mesh, SMOOTH_ANGLE_DEG))))
        .collect();
    if parts.is_empty() {
        return Err(ExchangeError::Empty);
    }
    let too_large = || ExchangeError::TooLargeForFormat { format: "glTF" };
    let (mut views, mut accessors, mut meshes, mut nodes) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut offset: u64 = 0;
    for (k, (name, s)) in parts.iter().enumerate() {
        let v = s.positions.len();
        let count = s.indices.len();
        let vbytes = 12 * v as u64;
        let ibytes = 4 * count as u64;
        let normals_at = offset + vbytes;
        let indices_at = offset + 2 * vbytes;
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{vbytes},"target":{ARRAY_BUFFER}}}"#
        ));
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{normals_at},"byteLength":{vbytes},"target":{ARRAY_BUFFER}}}"#
        ));
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{indices_at},"byteLength":{ibytes},"target":{ELEMENT_ARRAY_BUFFER}}}"#
        ));
        offset += 2 * vbytes + ibytes;
        let (a, b, c) = (3 * k, 3 * k + 1, 3 * k + 2);
        let (lo, hi) = bounds(&s.positions);
        accessors.push(format!(
            r#"{{"bufferView":{a},"componentType":{FLOAT},"count":{v},"type":"VEC3","min":[{},{},{}],"max":[{},{},{}]}}"#,
            lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]
        ));
        accessors.push(format!(
            r#"{{"bufferView":{b},"componentType":{FLOAT},"count":{v},"type":"VEC3"}}"#
        ));
        accessors.push(format!(
            r#"{{"bufferView":{c},"componentType":{UNSIGNED_INT},"count":{count},"type":"SCALAR"}}"#
        ));
        let name = json_string(name);
        meshes.push(format!(
            r#"{{"name":{name},"primitives":[{{"attributes":{{"POSITION":{a},"NORMAL":{b}}},"indices":{c},"mode":4}}]}}"#
        ));
        nodes.push(format!(r#"{{"mesh":{k},"name":{name}}}"#));
    }
    let scene: Vec<String> = (0..parts.len()).map(|k| k.to_string()).collect();
    let json = format!(
        r#"{{"asset":{{"version":"2.0","generator":"PlanoraCAD"}},"scene":0,"scenes":[{{"nodes":[{}]}}],"nodes":[{}],"meshes":[{}],"accessors":[{}],"bufferViews":[{}],"buffers":[{{"byteLength":{offset}}}]}}"#,
        scene.join(","),
        nodes.join(","),
        meshes.join(","),
        accessors.join(","),
        views.join(",")
    );
    let mut json = json.into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let json_len = u32::try_from(json.len()).map_err(|_| too_large())?;
    let bin_len = u32::try_from(offset).map_err(|_| too_large())?;
    let total = u32::try_from(GLB_HEADER + 2 * CHUNK_HEADER + json.len() as u64 + offset)
        .map_err(|_| too_large())?;
    w.write_all(&GLB_MAGIC.to_le_bytes())?;
    w.write_all(&GLB_VERSION.to_le_bytes())?;
    w.write_all(&total.to_le_bytes())?;
    w.write_all(&json_len.to_le_bytes())?;
    w.write_all(&CHUNK_JSON.to_le_bytes())?;
    w.write_all(&json)?;
    w.write_all(&bin_len.to_le_bytes())?;
    w.write_all(&CHUNK_BIN.to_le_bytes())?;
    for (_, s) in &parts {
        for p in s.positions.iter().chain(&s.normals) {
            for c in p {
                w.write_all(&c.to_le_bytes())?;
            }
        }
        for i in &s.indices {
            w.write_all(&i.to_le_bytes())?;
        }
    }
    Ok(())
}

/// Punkte in Meter (Normalen bleiben).
fn in_metres(mut s: Shaded) -> Shaded {
    for p in &mut s.positions {
        *p = p.map(|c| (f64::from(c) * GLTF_SCALE) as f32);
    }
    s
}

/// Kleinste und größte Koordinate je Achse der geschriebenen Werte.
fn bounds(points: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in points {
        for ((l, h), &c) in lo.iter_mut().zip(hi.iter_mut()).zip(p) {
            *l = l.min(c);
            *h = h.max(c);
        }
    }
    (lo, hi)
}

/// JSON-Zeichenkette mit Anführungszeichen; Steuerzeichen als `\u00XX`.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
