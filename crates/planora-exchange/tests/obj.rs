//! OBJ schreiben und lesen, Vielecke, feindliche Dateien (Teilprojekt 9, Task 4).

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

fn write(bodies: &[ExportBody]) -> String {
    let mut out = Vec::new();
    write_obj(&mut out, bodies).unwrap();
    String::from_utf8(out).unwrap()
}

fn read(text: &str) -> Result<ExchangeMesh, ExchangeError> {
    read_obj(text.as_bytes(), text.len() as u64, &Progress::new())
}

/// Summe der z-Komponenten der Flächennormalen (= doppelte Fläche in der xy-Ebene) und ob
/// alle Dreiecke nach +z zeigen.
fn signed_area_xy(m: &ExchangeMesh) -> (f64, bool) {
    let mut area = 0.0;
    let mut all_up = true;
    for t in m.indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| m.point(i as usize));
        let z = (b - a).cross(c - a).z;
        area += z / 2.0;
        all_up &= z > 0.0;
    }
    (area, all_up)
}

#[test]
fn the_written_text_has_objects_points_normals_and_faces() {
    let text = write(&[body("Würfel 1", cube())]);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "# PlanoraCAD");
    assert_eq!(lines[1], "o Würfel 1");
    assert_eq!(lines[2], "v 0 0 0");
    assert_eq!(lines.iter().filter(|l| l.starts_with("v ")).count(), 8);
    assert_eq!(lines.iter().filter(|l| l.starts_with("vn ")).count(), 8);
    assert_eq!(lines.iter().filter(|l| l.starts_with("f ")).count(), 12);
    assert!(lines.contains(&"f 1//1 3//3 2//2"), "{text}");
    // Ecke (1, 1, 1): gemittelte Normale zeigt schräg nach außen
    let vn = lines
        .iter()
        .filter(|l| l.starts_with("vn "))
        .nth(6)
        .unwrap();
    let n: Vec<f32> = vn[3..].split(' ').map(|x| x.parse().unwrap()).collect();
    assert!(
        n.iter().all(|&c| (c - 1.0 / 3f32.sqrt()).abs() < 1e-6),
        "{vn}"
    );
}

#[test]
fn several_bodies_continue_the_numbering_and_names_stay_on_one_line() {
    let text = write(&[body("A", cube()), body("B\nzwei", cube())]);
    assert!(
        text.contains("o A\n") && text.contains("o B zwei\n"),
        "{text}"
    );
    assert!(text.contains("f 9//9 11//11 10//10"));
    assert_eq!(read(&text).unwrap().triangle_count(), 24);
    let mut out = Vec::new();
    assert_eq!(write_obj(&mut out, &[]), Err(ExchangeError::Empty));
}

#[test]
fn round_trip_is_exact() {
    let mut c = cube();
    for p in &mut c.positions {
        p[0] = p[0] * 0.1 + 1234.5678;
        p[2] *= -3.3;
    }
    let back = read(&write(&[body("x", c.clone())])).unwrap();
    assert_eq!(back, weld_exact(&c.positions, &c.indices));
    assert!(check_mesh(&back).closed);
}

#[test]
fn all_corner_forms_and_negative_indices_are_read() {
    let text = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvn 0 0 1\ng gruppe\ns 1\nusemtl stahl\n\
                f 1 2 3\nf 1/1 3/1 4/1\nf -4//1 -2//1 -1//1\nf 1/1/1 2/1/1 3/1/1\n";
    let m = read(text).unwrap();
    assert_eq!(m.indices, vec![0, 1, 2, 0, 2, 3, 0, 2, 3, 0, 1, 2]);
}

#[test]
fn a_convex_quad_becomes_a_fan_and_a_concave_polygon_is_cut_by_ear_clipping() {
    let quad = read("v 0 0 0\nv 2 0 0\nv 2 1 0\nv 0 1 0\nf 1 2 3 4\n").unwrap();
    assert_eq!(quad.indices, vec![0, 1, 2, 0, 2, 3]);
    // U-Form gegen den Uhrzeigersinn (Fläche 3 · 2 − 1 · 1 = 5); ein Fächer von Punkt 1 aus
    // ergäbe falsch herum liegende Dreiecke.
    let u = read(
        "v 0 0 0\nv 3 0 0\nv 3 2 0\nv 2 2 0\nv 2 1 0\nv 1 1 0\nv 1 2 0\nv 0 2 0\nf 1 2 3 4 5 6 7 8\n",
    )
    .unwrap();
    assert_eq!(u.triangle_count(), 6);
    let (area, all_up) = signed_area_xy(&u);
    assert!((area - 5.0).abs() < 1e-12, "Fläche {area}");
    assert!(all_up, "jedes Dreieck behält die Orientierung des Vielecks");
    // dieselbe U-Form im Uhrzeigersinn: alle Dreiecke zeigen nach −z
    let cw = read(
        "v 0 0 0\nv 0 2 0\nv 1 2 0\nv 1 1 0\nv 2 1 0\nv 2 2 0\nv 3 2 0\nv 3 0 0\nf 1 2 3 4 5 6 7 8\n",
    )
    .unwrap();
    let (area, _) = signed_area_xy(&cw);
    assert!((area + 5.0).abs() < 1e-12, "Fläche {area}");
}

#[test]
fn hostile_obj_files_get_named_errors() {
    let cases: [(&str, ExchangeError); 9] = [
        (
            "v 0 0 0\nv 1 0 0\nf 1 2 3\n",
            ExchangeError::IndexOutOfRange { line: 3 },
        ),
        (
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 0\n",
            ExchangeError::IndexOutOfRange { line: 4 },
        ),
        (
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -4 2 3\n",
            ExchangeError::IndexOutOfRange { line: 4 },
        ),
        (
            "f 1 2 3\nv 0 0 0\nv 1 0 0\nv 0 1 0\n",
            ExchangeError::IndexOutOfRange { line: 1 },
        ),
        (
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 99999999999999999999\n",
            ExchangeError::Syntax {
                line: 4,
                msg: "Eine Ecke hat keine gültige Punktnummer",
            },
        ),
        ("v 0 0 1e400\n", ExchangeError::NotFinite { line: Some(1) }),
        ("v 0 0 nan\n", ExchangeError::NotFinite { line: Some(1) }),
        ("v 0 0 x\n", ExchangeError::BadNumber { line: 1 }),
        (
            "v 0 0 0\nv 1 0 0\nf 1 2\n",
            ExchangeError::Syntax {
                line: 3,
                msg: "Eine Fläche braucht mindestens drei Ecken",
            },
        ),
    ];
    for (text, e) in cases {
        assert_eq!(read(text), Err(e), "{text:?}");
    }
    assert_eq!(
        read("# nur ein Kommentar\nv 0 0 0\n"),
        Err(ExchangeError::Empty)
    );
    assert_eq!(
        read_obj(&b""[..], MAX_FILE_BYTES + 1, &Progress::new()),
        Err(ExchangeError::FileTooLarge {
            bytes: MAX_FILE_BYTES + 1
        })
    );
    // Vieleck ohne Fläche (alle Punkte auf einer Geraden): benannt
    assert_eq!(
        read("v 0 0 0\nv 1 0 0\nv 2 0 0\nv 1 0 0\nf 1 2 3 4\n"),
        Err(ExchangeError::Polygon { line: 5 })
    );
}

#[test]
fn line_endings_bom_tabs_and_foreign_comment_bytes_are_fine() {
    let base = "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\nf 1 3 2\nf 1 2 4\nf 1 4 3\nf 2 3 4\n";
    for nl in ["\n", "\r\n", "\r"] {
        let text =
            format!("\u{feff}# Kommentar\n{}", base.replace('\n', nl)).replace("v 1", "v\t1");
        let m = read(&text).unwrap();
        assert_eq!(m.triangle_count(), 4, "{nl:?}");
        assert!(check_mesh(&m).closed);
    }
    let latin1 = b"# Gr\xfc\xdfe\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n".to_vec();
    assert_eq!(
        read_obj(&latin1[..], latin1.len() as u64, &Progress::new())
            .unwrap()
            .triangle_count(),
        1
    );
}

#[test]
fn reading_stops_when_cancelled() {
    let p = Progress::new();
    p.cancel();
    let text = write(&[body("x", cube())]);
    assert_eq!(
        read_obj(text.as_bytes(), text.len() as u64, &p),
        Err(ExchangeError::Cancelled)
    );
}
