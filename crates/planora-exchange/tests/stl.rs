//! STL schreiben und lesen, Erkennung Text/Binär, feindliche Dateien (Teilprojekt 9, Task 3).

use planora_exchange::*;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn body(name: &str, mesh: ExchangeMesh) -> ExportBody {
    ExportBody {
        name: name.into(),
        mesh,
    }
}

fn write(bodies: &[ExportBody]) -> Vec<u8> {
    let mut out = Vec::new();
    write_stl(&mut out, bodies).unwrap();
    out
}

fn read(bytes: &[u8]) -> Result<ExchangeMesh, ExchangeError> {
    read_stl(bytes, bytes.len() as u64, &Progress::new())
}

fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

#[test]
fn binary_layout_matches_the_spec() {
    let bytes = write(&[body("Würfel", cube())]);
    assert_eq!(
        bytes.len() as u64,
        STL_HEADER_BYTES + STL_TRIANGLE_BYTES * 12
    );
    assert_eq!(&bytes[..7], b"Planora");
    assert!(bytes[7..80].iter().all(|&b| b == 0));
    assert_eq!(u32::from_le_bytes(bytes[80..84].try_into().unwrap()), 12);
    // erstes Dreieck (0, 2, 1): Normale −z, zweiter Punkt (1, 1, 0), Attribut 0
    assert_eq!(
        [f32_at(&bytes, 84), f32_at(&bytes, 88), f32_at(&bytes, 92)],
        [0.0, 0.0, -1.0]
    );
    assert_eq!(
        [
            f32_at(&bytes, 108),
            f32_at(&bytes, 112),
            f32_at(&bytes, 116)
        ],
        [1.0, 1.0, 0.0]
    );
    assert_eq!(&bytes[132..134], &[0, 0]);
}

#[test]
fn several_bodies_go_into_one_file() {
    let mut moved = cube();
    for p in &mut moved.positions {
        p[0] += 5.0;
    }
    let bytes = write(&[body("A", cube()), body("B", moved)]);
    assert_eq!(u32::from_le_bytes(bytes[80..84].try_into().unwrap()), 24);
    let back = read(&bytes).unwrap();
    assert_eq!((back.triangle_count(), back.positions.len()), (24, 16));
}

#[test]
fn binary_round_trip_keeps_shape_and_closedness() {
    let back = read(&write(&[body("Würfel", cube())])).unwrap();
    assert_eq!((back.triangle_count(), back.positions.len()), (12, 8));
    let r = check_mesh(&back);
    assert!(r.closed, "{r:?}");
    let v = back.mass(&MeshTransform::IDENTITY);
    assert!((v.volume - 1.0).abs() < 1e-12 && (v.surface_area - 6.0).abs() < 1e-12);
}

#[test]
fn writing_nothing_is_an_error() {
    let mut out = Vec::new();
    assert_eq!(write_stl(&mut out, &[]), Err(ExchangeError::Empty));
    assert_eq!(
        write_stl(&mut out, &[body("leer", ExchangeMesh::default())]),
        Err(ExchangeError::Empty)
    );
}

/// Würfel als STL-Text mit dem Zeilenende `newline`.
fn ascii_cube(newline: &str) -> String {
    let c = cube();
    let mut s = String::from("solid würfel");
    s.push_str(newline);
    for t in c.indices.as_chunks::<3>().0 {
        for line in ["  facet normal 0 0 0", "    outer loop"] {
            s.push_str(line);
            s.push_str(newline);
        }
        for &i in t {
            let p = c.positions[i as usize];
            s.push_str(&format!("      vertex {} {} {}", p[0], p[1], p[2]));
            s.push_str(newline);
        }
        for line in ["    endloop", "  endfacet"] {
            s.push_str(line);
            s.push_str(newline);
        }
    }
    s.push_str("endsolid würfel");
    s.push_str(newline);
    s
}

#[test]
fn ascii_files_are_read_with_all_line_endings_and_a_bom() {
    for nl in ["\n", "\r\n", "\r"] {
        let text = ascii_cube(nl);
        let m = read(text.as_bytes()).unwrap();
        assert_eq!((m.triangle_count(), m.positions.len()), (12, 8), "{nl:?}");
        assert!(check_mesh(&m).closed);
        let with_bom = format!("\u{feff}{text}");
        assert_eq!(read(with_bom.as_bytes()).unwrap().triangle_count(), 12);
        let upper = text.replace("solid", "SOLID");
        assert_eq!(read(upper.as_bytes()).unwrap().triangle_count(), 12);
    }
}

#[test]
fn several_solid_blocks_are_merged() {
    let one = ascii_cube("\n");
    let m = read(format!("{one}{one}").as_bytes()).unwrap();
    assert_eq!((m.triangle_count(), m.positions.len()), (24, 8));
}

#[test]
fn a_binary_file_whose_header_starts_with_solid_is_read_as_binary() {
    let mut bytes = write(&[body("Würfel", cube())]);
    bytes[..7].copy_from_slice(b"solid x");
    assert_eq!(read(&bytes).unwrap().triangle_count(), 12);
}

#[test]
fn hostile_binary_files_get_named_errors() {
    let good = write(&[body("Würfel", cube())]);
    // abgeschnitten, Größe ehrlich: Kopfzahl größer als die Datei hergibt
    let cut = &good[..good.len() - 10];
    assert_eq!(
        read(cut),
        Err(ExchangeError::CountTooLarge {
            claimed: 12,
            possible: 11
        })
    );
    // abgeschnitten, Größenangabe falsch: endet zu früh
    assert_eq!(
        read_stl(cut, good.len() as u64, &Progress::new()),
        Err(ExchangeError::Truncated)
    );
    // kürzer als der Kopf
    assert_eq!(read(&good[..50]), Err(ExchangeError::Truncated));
    // Kopfzahl über der Grenze
    let mut many = good.clone();
    many[80..84].copy_from_slice(&(MAX_TRIANGLES as u32 + 1).to_le_bytes());
    assert_eq!(read(&many), Err(ExchangeError::TooManyTriangles));
    // Datei über 8 GB
    assert_eq!(
        read_stl(&good[..], MAX_FILE_BYTES + 1, &Progress::new()),
        Err(ExchangeError::FileTooLarge {
            bytes: MAX_FILE_BYTES + 1
        })
    );
    // NaN und unendlich im ersten Punkt
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut b = good.clone();
        b[96..100].copy_from_slice(&bad.to_le_bytes());
        assert_eq!(read(&b), Err(ExchangeError::NotFinite { line: None }));
    }
    // keine Dreiecke
    let mut empty = good[..84].to_vec();
    empty[80..84].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(read(&empty), Err(ExchangeError::Empty));
}

#[test]
fn hostile_text_files_get_named_errors_with_line_numbers() {
    let three = "Ein Dreieck braucht genau drei Punkte";
    let cases: [(&str, ExchangeError); 8] = [
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nendloop\nendfacet\nendsolid\n",
            ExchangeError::Syntax {
                line: 7,
                msg: three,
            },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nvertex 1 1 0\n",
            ExchangeError::Syntax {
                line: 7,
                msg: three,
            },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 nan\n",
            ExchangeError::NotFinite { line: Some(4) },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 1e39\n",
            ExchangeError::NotFinite { line: Some(4) },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 zwei 0\n",
            ExchangeError::BadNumber { line: 4 },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0\n",
            ExchangeError::Syntax {
                line: 4,
                msg: "Es fehlt eine Zahl",
            },
        ),
        (
            "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\n",
            ExchangeError::Truncated,
        ),
        (
            "solid x\r\nhallo welt\r\n",
            ExchangeError::Syntax {
                line: 2,
                msg: "Unbekanntes Wort in der STL-Datei",
            },
        ),
    ];
    for (text, expected) in cases {
        assert_eq!(read(text.as_bytes()), Err(expected), "{text:?}");
    }
    assert_eq!(
        read(b"solid leer\nendsolid leer\n"),
        Err(ExchangeError::Empty)
    );
}

#[test]
fn an_endless_line_is_refused() {
    let mut text = b"solid x\n".to_vec();
    text.extend(std::iter::repeat_n(b'7', MAX_LINE_BYTES + 10));
    assert_eq!(
        read(&text),
        Err(ExchangeError::Syntax {
            line: 2,
            msg: "Die Zeile ist zu lang"
        })
    );
}

#[test]
fn reading_stops_when_cancelled_and_counts_its_bytes() {
    let bytes = write(&[body("Würfel", cube())]);
    let p = Progress::new();
    p.cancel();
    assert_eq!(
        read_stl(&bytes[..], bytes.len() as u64, &p),
        Err(ExchangeError::Cancelled)
    );
    let p = Progress::new();
    read_stl(&bytes[..], bytes.len() as u64, &p).unwrap();
    assert_eq!(p.fraction(), 1.0);
}
