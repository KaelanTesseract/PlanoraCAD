//! OBJ: Text schreiben (`o`, `v`, `vn`, `f a//n`); lesen mit Dreiecken und Vielecken (Spec §3,
//! Plan P6).

use crate::check::weld_exact;
use crate::error::ExchangeError;
use crate::limits::{CHECK_EVERY, MAX_FILE_BYTES, MAX_TRIANGLES, MAX_VERTICES, READ_BUFFER};
use crate::mesh::ExchangeMesh;
use crate::progress::Progress;
use crate::source::ExportBody;
use crate::text::{CountingReader, LineReader, three_numbers};
use planora_math::DVec3;
use std::cmp::Ordering;
use std::io::{BufReader, Read, Write};

/// Schreibt alle Körper als OBJ: je Körper `o Name`, Punkte, Normalen je Punkt und Dreiecke
/// (Nummern über alle Körper fortlaufend, ab 1).
pub fn write_obj<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError> {
    if bodies.iter().all(|b| b.mesh.triangle_count() == 0) {
        return Err(ExchangeError::Empty);
    }
    writeln!(w, "# PlanoraCAD")?;
    let mut base: u64 = 1;
    for body in bodies.iter().filter(|b| b.mesh.triangle_count() > 0) {
        let m = &body.mesh;
        writeln!(w, "o {}", object_name(&body.name))?;
        for p in &m.positions {
            writeln!(w, "v {} {} {}", p[0], p[1], p[2])?;
        }
        for n in vertex_normals(m) {
            writeln!(w, "vn {} {} {}", n[0], n[1], n[2])?;
        }
        for t in m.indices.as_chunks::<3>().0 {
            let [a, b, c] = t.map(|i| base + u64::from(i));
            writeln!(w, "f {a}//{a} {b}//{b} {c}//{c}")?;
        }
        base += m.positions.len() as u64;
    }
    Ok(())
}

/// Name auf einer Zeile: Steuerzeichen werden Leerzeichen; leer → „Körper“.
fn object_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        "Körper".into()
    } else {
        clean.to_string()
    }
}

/// Normale je Punkt: Summe der Flächennormalen der anliegenden Dreiecke (nach Fläche gewichtet),
/// normiert; ohne Fläche (0, 0, 1).
fn vertex_normals(m: &ExchangeMesh) -> Vec<[f32; 3]> {
    let mut sum = vec![DVec3::ZERO; m.positions.len()];
    for t in m.indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| m.point(i as usize));
        let n = (b - a).cross(c - a);
        for &i in t {
            sum[i as usize] += n;
        }
    }
    sum.into_iter()
        .map(|n| {
            let u = n.try_normalize().unwrap_or(DVec3::Z);
            [u.x as f32, u.y as f32, u.z as f32]
        })
        .collect()
}

/// Liest OBJ: `v` und `f` (Ecken `v`, `v/vt`, `v//vn`, `v/vt/vn`, auch negative Nummern);
/// alles Übrige wird übergangen. Gleiche Punkte werden verschmolzen (P3).
pub fn read_obj<R: Read>(
    r: R,
    len: u64,
    progress: &Progress,
) -> Result<ExchangeMesh, ExchangeError> {
    if len > MAX_FILE_BYTES {
        return Err(ExchangeError::FileTooLarge { bytes: len });
    }
    progress.set_total(len);
    progress.check()?;
    let mut lines = LineReader::new(BufReader::with_capacity(
        READ_BUFFER,
        CountingReader::new(r, progress),
    ));
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut corners: Vec<u32> = Vec::new();
    while let Some((line, raw)) = lines.next_line()? {
        if (line as u64).is_multiple_of(CHECK_EVERY) {
            progress.check()?;
        }
        let text = String::from_utf8_lossy(raw);
        let mut tokens = text.split_whitespace();
        match tokens.next() {
            Some("v") => {
                if positions.len() == MAX_VERTICES {
                    return Err(ExchangeError::TooManyVertices);
                }
                positions.push(three_numbers(&mut tokens, line)?);
            }
            Some("f") => {
                corners.clear();
                for token in tokens {
                    corners.push(corner(token, positions.len(), line)?);
                }
                if corners.len() < 3 {
                    return Err(ExchangeError::Syntax {
                        line,
                        msg: "Eine Fläche braucht mindestens drei Ecken",
                    });
                }
                triangulate(&positions, &corners, line, &mut indices)?;
                if indices.len() / 3 > MAX_TRIANGLES {
                    return Err(ExchangeError::TooManyTriangles);
                }
            }
            // Kommentare, Gruppen, Materialien, Glättung, Texturkoordinaten, Normalen und alles
            // Übrige werden übergangen (Spec §3).
            _ => {}
        }
    }
    if indices.is_empty() {
        return Err(ExchangeError::Empty);
    }
    Ok(weld_exact(&positions, &indices))
}

/// Punktnummer einer Ecke; negative Nummern zählen vom zuletzt gelesenen Punkt zurück. Nur schon
/// gelesene Punkte sind gültig (P6).
fn corner(token: &str, count: usize, line: usize) -> Result<u32, ExchangeError> {
    let first = token.split('/').next().unwrap_or_default();
    let i: i64 = first.parse().map_err(|_| ExchangeError::Syntax {
        line,
        msg: "Eine Ecke hat keine gültige Punktnummer",
    })?;
    let count = count as i64;
    let index = match i.cmp(&0) {
        Ordering::Greater => i - 1,
        Ordering::Less => count + i,
        Ordering::Equal => -1,
    };
    if !(0..count).contains(&index) {
        return Err(ExchangeError::IndexOutOfRange { line });
    }
    Ok(index as u32)
}

fn point(p: [f32; 3]) -> DVec3 {
    DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
}

/// Höchstzahl der Ecken eines nicht konvexen Vielecks. `earcut` rechnet quadratisch und lässt
/// sich nicht abbrechen; bei 5000 Ecken bleibt die Zerlegung im Debug-Build deutlich unter einer
/// Sekunde. Konvexe Vielecke (Fächer) sind nur durch die Zeilenlänge begrenzt.
const MAX_EARCUT_CORNERS: usize = 5_000;

/// Zerlegt ein Vieleck in Dreiecke (P6): konvex → Fächer, sonst `earcutr` mit angeglichener
/// Orientierung.
fn triangulate(
    positions: &[[f32; 3]],
    corners: &[u32],
    line: usize,
    out: &mut Vec<u32>,
) -> Result<(), ExchangeError> {
    let n = corners.len();
    if n == 3 {
        out.extend_from_slice(corners);
        return Ok(());
    }
    let pts: Vec<DVec3> = corners
        .iter()
        .map(|&i| point(positions[i as usize]))
        .collect();
    let normal = newell(&pts);
    if is_convex(&pts, normal) {
        for w in corners[1..].windows(2) {
            out.extend_from_slice(&[corners[0], w[0], w[1]]);
        }
        return Ok(());
    }
    if n > MAX_EARCUT_CORNERS {
        return Err(ExchangeError::Polygon { line });
    }
    let flat = project(&pts, normal);
    let tris = earcutr::earcut(&flat, &[], 2).map_err(|_| ExchangeError::Polygon { line })?;
    if tris.len() != 3 * (n - 2) {
        return Err(ExchangeError::Polygon { line });
    }
    let at = |k: usize| (flat[2 * k], flat[2 * k + 1]);
    let polygon: f64 = (0..n)
        .map(|i| {
            let ((x0, y0), (x1, y1)) = (at(i), at((i + 1) % n));
            x0 * y1 - x1 * y0
        })
        .sum();
    for t in tris.as_chunks::<3>().0 {
        let [a, b, c] = *t;
        let ((ax, ay), (bx, by), (cx, cy)) = (at(a), at(b), at(c));
        let signed = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        if signed * polygon >= 0.0 {
            out.extend_from_slice(&[corners[a], corners[b], corners[c]]);
        } else {
            out.extend_from_slice(&[corners[a], corners[c], corners[b]]);
        }
    }
    Ok(())
}

/// Normale nach Newell (Länge = doppelte Fläche des Vielecks).
fn newell(pts: &[DVec3]) -> DVec3 {
    let n = pts.len();
    pts.iter()
        .enumerate()
        .map(|(i, p)| p.cross(pts[(i + 1) % n]))
        .sum()
}

/// Konvex (gerade Ecken erlaubt), wenn jede Ecke im Sinn der Normalen abbiegt; ohne Normale nie.
fn is_convex(pts: &[DVec3], normal: DVec3) -> bool {
    if normal == DVec3::ZERO {
        return false;
    }
    let n = pts.len();
    (0..n).all(|i| {
        let (a, b, c) = (pts[i], pts[(i + 1) % n], pts[(i + 2) % n]);
        (b - a).cross(c - b).dot(normal) >= 0.0
    })
}

/// Punkte in die Ebene senkrecht zur größten Komponente der Normalen (für `earcut`).
fn project(pts: &[DVec3], normal: DVec3) -> Vec<f64> {
    let a = normal.abs();
    let axes: fn(DVec3) -> [f64; 2] = if a.z >= a.x && a.z >= a.y {
        |p| [p.x, p.y]
    } else if a.x >= a.y {
        |p| [p.y, p.z]
    } else {
        |p| [p.z, p.x]
    };
    pts.iter().flat_map(|&p| axes(p)).collect()
}
