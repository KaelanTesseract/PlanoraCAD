//! STL: binär schreiben; binär und als Text lesen (Spec §3, Plan P4, P5).

use crate::check::weld_exact;
use crate::error::ExchangeError;
use crate::limits::{CHECK_EVERY, MAX_FILE_BYTES, MAX_TRIANGLES, READ_BUFFER, RESERVE_STEP};
use crate::mesh::ExchangeMesh;
use crate::progress::Progress;
use crate::source::ExportBody;
use crate::text::{CountingReader, LineReader, three_numbers};
use planora_i18n::tr;
use planora_math::DVec3;
use std::io::{BufRead, BufReader, Read, Write};

/// Länge des Kopfs einer binären STL-Datei (80 Bytes Text, 4 Bytes Dreieckszahl).
pub const STL_HEADER_BYTES: u64 = 84;
/// Bytes je Dreieck einer binären STL-Datei (Normale, drei Punkte, Attribut).
pub const STL_TRIANGLE_BYTES: u64 = 50;
/// Anfang des Kopftexts geschriebener Dateien (Spec §3); der Rest des Kopfs sind Nullen.
const HEADER_TEXT: &[u8] = b"Planora";
/// UTF-8-Kennung am Anfang mancher Textdateien.
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Schreibt alle Körper als ein binäres STL (Dreiecke aller Körper hintereinander).
pub fn write_stl<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError> {
    let total: usize = bodies.iter().map(|b| b.mesh.triangle_count()).sum();
    if total == 0 {
        return Err(ExchangeError::Empty);
    }
    let count =
        u32::try_from(total).map_err(|_| ExchangeError::TooLargeForFormat { format: "STL" })?;
    let mut header = [0u8; 80];
    header[..HEADER_TEXT.len()].copy_from_slice(HEADER_TEXT);
    w.write_all(&header)?;
    w.write_all(&count.to_le_bytes())?;
    // Normale, drei Punkte, Attribut (bleibt 0)
    let mut record = [0u8; STL_TRIANGLE_BYTES as usize];
    for body in bodies {
        let m = &body.mesh;
        for t in m.indices.as_chunks::<3>().0 {
            let p = t.map(|i| m.positions[i as usize]);
            for (k, v) in [normal(p), p[0], p[1], p[2]].iter().enumerate() {
                for (c, x) in v.iter().enumerate() {
                    let at = 12 * k + 4 * c;
                    record[at..at + 4].copy_from_slice(&x.to_le_bytes());
                }
            }
            w.write_all(&record)?;
        }
    }
    Ok(())
}

/// Einheitsnormale eines Dreiecks aus seinen Punkten (null bei Fläche null).
fn normal(p: [[f32; 3]; 3]) -> [f32; 3] {
    let d = p.map(|q| DVec3::new(f64::from(q[0]), f64::from(q[1]), f64::from(q[2])));
    let n = (d[1] - d[0]).cross(d[2] - d[0]).normalize_or_zero();
    [n.x as f32, n.y as f32, n.z as f32]
}

/// Liest STL, binär oder als Text (Erkennung nach P5). `len` ist die Dateigröße in Bytes (für
/// die Erkennung und die Grenzen); `progress` zählt gelesene Bytes und kann abbrechen. Gleiche
/// Punkte werden verschmolzen (P3).
pub fn read_stl<R: Read>(
    r: R,
    len: u64,
    progress: &Progress,
) -> Result<ExchangeMesh, ExchangeError> {
    if len > MAX_FILE_BYTES {
        return Err(ExchangeError::FileTooLarge { bytes: len });
    }
    progress.set_total(len);
    progress.check()?;
    let mut r = BufReader::with_capacity(READ_BUFFER, CountingReader::new(r, progress));
    let mut head = [0u8; STL_HEADER_BYTES as usize];
    let got = read_up_to(&mut r, &mut head)?;
    if looks_like_text(&head[..got], len) {
        read_text((&head[..got]).chain(r), progress)
    } else if got < head.len() {
        Err(ExchangeError::Truncated)
    } else {
        read_binary(r, &head, len, progress)
    }
}

/// Liest höchstens `buf.len()` Bytes; liefert, wie viele es waren.
fn read_up_to<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<usize, ExchangeError> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(got)
}

/// Text, wenn die Datei (nach BOM und Leerraum) mit `solid` beginnt und ihre Größe nicht zur
/// Dreieckszahl im Binärkopf passt (Spec §3).
fn looks_like_text(head: &[u8], len: u64) -> bool {
    let body = head.strip_prefix(&BOM).unwrap_or(head);
    let start = body
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(body.len());
    let body = &body[start..];
    let solid = body.len() >= 5 && body[..5].eq_ignore_ascii_case(b"solid");
    let binary_fits = head.len() == STL_HEADER_BYTES as usize
        && STL_HEADER_BYTES + STL_TRIANGLE_BYTES * claimed(head) == len;
    solid && !binary_fits
}

/// Dreieckszahl aus dem Binärkopf (Bytes 80 bis 83).
fn claimed(head: &[u8]) -> u64 {
    u64::from(u32::from_le_bytes([head[80], head[81], head[82], head[83]]))
}

fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn read_binary<R: Read>(
    mut r: R,
    head: &[u8],
    len: u64,
    progress: &Progress,
) -> Result<ExchangeMesh, ExchangeError> {
    let claimed = claimed(head);
    if claimed > MAX_TRIANGLES as u64 {
        return Err(ExchangeError::TooManyTriangles);
    }
    let possible = len.saturating_sub(STL_HEADER_BYTES) / STL_TRIANGLE_BYTES;
    if claimed > possible {
        return Err(ExchangeError::CountTooLarge { claimed, possible });
    }
    if claimed == 0 {
        return Err(ExchangeError::Empty);
    }
    // Reserviert wird nur ein Schritt; danach wächst der Speicher mit dem Gelesenen (P4).
    let mut points: Vec<[f32; 3]> = Vec::with_capacity(3 * (claimed as usize).min(RESERVE_STEP));
    let mut record = [0u8; STL_TRIANGLE_BYTES as usize];
    for i in 0..claimed {
        if i.is_multiple_of(CHECK_EVERY) {
            progress.check()?;
        }
        r.read_exact(&mut record)?;
        for k in 0..3 {
            let at = 12 + 12 * k;
            let p = [
                f32_at(&record, at),
                f32_at(&record, at + 4),
                f32_at(&record, at + 8),
            ];
            if !p.iter().all(|c| c.is_finite()) {
                return Err(ExchangeError::NotFinite { line: None });
            }
            points.push(p);
        }
    }
    let indices: Vec<u32> = (0..points.len() as u32).collect();
    Ok(weld_exact(&points, &indices))
}

fn read_text<R: BufRead>(r: R, progress: &Progress) -> Result<ExchangeMesh, ExchangeError> {
    let three = tr!("exchange-stl-three-points");
    let mut lines = LineReader::new(r);
    let mut points: Vec<[f32; 3]> = Vec::new();
    let (mut in_facet, mut corners, mut triangles) = (false, 0usize, 0usize);
    while let Some((line, raw)) = lines.next_line()? {
        if (line as u64).is_multiple_of(CHECK_EVERY) {
            progress.check()?;
        }
        let text = String::from_utf8_lossy(raw);
        let mut tokens = text.split_whitespace();
        let Some(word) = tokens.next() else {
            continue;
        };
        match word.to_ascii_lowercase().as_str() {
            "solid" | "endsolid" | "outer" | "endloop" => {}
            "facet" => {
                if in_facet {
                    return Err(ExchangeError::Syntax {
                        line,
                        msg: tr!("exchange-stl-facet-overlap"),
                    });
                }
                in_facet = true;
                corners = 0;
            }
            "vertex" => {
                if !in_facet || corners == 3 {
                    return Err(ExchangeError::Syntax {
                        line,
                        msg: three.clone(),
                    });
                }
                points.push(three_numbers(&mut tokens, line)?);
                corners += 1;
            }
            "endfacet" => {
                if !in_facet || corners != 3 {
                    return Err(ExchangeError::Syntax {
                        line,
                        msg: three.clone(),
                    });
                }
                in_facet = false;
                triangles += 1;
                if triangles > MAX_TRIANGLES {
                    return Err(ExchangeError::TooManyTriangles);
                }
            }
            _ => {
                return Err(ExchangeError::Syntax {
                    line,
                    msg: tr!("exchange-stl-unknown-word"),
                });
            }
        }
    }
    if in_facet {
        return Err(ExchangeError::Truncated);
    }
    if triangles == 0 {
        return Err(ExchangeError::Empty);
    }
    let indices: Vec<u32> = (0..points.len() as u32).collect();
    Ok(weld_exact(&points, &indices))
}
