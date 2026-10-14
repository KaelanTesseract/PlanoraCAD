//! Stufentest 2: alle Formate über Dateien, Größenschätzung, Hilfsdatei und Endungen
//! (Teilprojekt 9, Task 9).

mod common;

use planora_exchange::*;
use planora_kernel::{AnalyticKernel, Kernel};
use planora_mesh::{TessellationOptions, tessellate};
use std::fs;
use std::path::{Path, PathBuf};

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

fn cube_body() -> ExportBody {
    ExportBody {
        name: "Würfel".into(),
        mesh: cube(),
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-exchange-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn every_format_writes_a_readable_file_near_its_estimate() {
    let k = AnalyticKernel::default();
    let solids = [
        ("Kugel", k.make_sphere(6.0).unwrap()),
        ("Zylinder", k.make_cylinder(3.0, 8.0).unwrap()),
    ];
    let bodies: Vec<ExportBody> = solids
        .iter()
        .map(|(name, s)| {
            let m = tessellate(
                s,
                &TessellationOptions {
                    chord_tolerance: 0.01,
                },
            )
            .unwrap();
            collect_body(&SolidSource { name, mesh: &m }).unwrap()
        })
        .collect();
    let t: u64 = bodies.iter().map(|b| b.mesh.triangle_count() as u64).sum();
    let v: u64 = bodies.iter().map(|b| b.mesh.positions.len() as u64).sum();
    let dir = temp_dir("formate");
    for format in Format::ALL {
        let path = dir.join(format!("körper.{}", format.extension()));
        let len = export_file(&path, format, &bodies).unwrap();
        assert_eq!(len, fs::metadata(&path).unwrap().len());
        assert!(!temp_path(&path).exists(), "Hilfsdatei entfernt");
        let estimate = format.estimate_bytes(t, v);
        assert!(
            len as f64 >= 0.5 * estimate as f64 && len as f64 <= 2.0 * estimate as f64,
            "{format:?}: {len} Bytes, geschätzt {estimate}"
        );
        let bytes = fs::read(&path).unwrap();
        match format {
            Format::Stl | Format::Obj => {
                let m = import_file(&path, &Progress::new()).unwrap();
                assert_eq!(m.triangle_count() as u64, t);
            }
            Format::ThreeMf => {
                let entries = common::read_zip(&bytes).unwrap();
                let xml =
                    common::parse_xml(std::str::from_utf8(&entries[2].data).unwrap()).unwrap();
                assert_eq!(
                    xml.iter().filter(|x| x.name == "triangle").count() as u64,
                    t
                );
                assert_eq!(xml.iter().filter(|x| x.name == "vertex").count() as u64, v);
            }
            Format::Glb => {
                assert_eq!(&bytes[..4], b"glTF");
                let total = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
                assert_eq!(total as usize, bytes.len());
            }
        }
    }
    assert_eq!(Format::Stl.estimate_bytes(12, 8), 84 + 50 * 12);
    let labels: Vec<&str> = Format::ALL.iter().map(|f| f.label()).collect();
    assert_eq!(labels, ["STL", "OBJ", "3MF", "glTF (.glb)"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_export_keeps_the_old_file_and_leaves_no_helper() {
    let dir = temp_dir("fehler");
    let path = dir.join("alt.stl");
    fs::write(&path, b"alter Inhalt").unwrap();
    assert_eq!(
        export_file(&path, Format::Stl, &[]),
        Err(ExchangeError::Empty)
    );
    let empty = [ExportBody {
        name: "leer".into(),
        mesh: ExchangeMesh::default(),
    }];
    for format in Format::ALL {
        assert_eq!(
            export_file(&path, format, &empty),
            Err(ExchangeError::Empty)
        );
    }
    assert_eq!(fs::read(&path).unwrap(), b"alter Inhalt");
    assert!(!temp_path(&path).exists());
    // Ordner gibt es nicht: Dateifehler, nichts angelegt
    let missing = dir.join("fehlt").join("x.stl");
    assert!(matches!(
        export_file(&missing, Format::Stl, &[cube_body()]),
        Err(ExchangeError::Io { .. })
    ));
    // Ziel ist ein Ordner: Umbenennen scheitert, Hilfsdatei entfernt, Ordner bleibt
    let target_dir = dir.join("ordner.stl");
    fs::create_dir_all(&target_dir).unwrap();
    assert!(export_file(&target_dir, Format::Stl, &[cube_body()]).is_err());
    assert!(target_dir.is_dir());
    assert!(!temp_path(&target_dir).exists());
    // Überschreiben einer vorhandenen Datei klappt
    export_file(&path, Format::Stl, &[cube_body()]).unwrap();
    assert_eq!(fs::read(&path).unwrap().len(), 84 + 50 * 12);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_endings_and_missing_files_are_named() {
    assert_eq!(
        import_file(Path::new("x.step"), &Progress::new()),
        Err(ExchangeError::UnknownFormat("step".into()))
    );
    assert!(matches!(
        import_file(Path::new("gibt-es-nicht.stl"), &Progress::new()),
        Err(ExchangeError::Io { .. })
    ));
    let dir = temp_dir("endung");
    let path = dir.join("W.STL");
    export_file(&path, Format::Stl, &[cube_body()]).unwrap();
    assert_eq!(
        import_file(&path, &Progress::new())
            .unwrap()
            .triangle_count(),
        12
    );
    let obj = dir.join("W.Obj");
    export_file(&obj, Format::Obj, &[cube_body()]).unwrap();
    assert!(check_mesh(&import_file(&obj, &Progress::new()).unwrap()).closed);
    assert_eq!(
        temp_path(&path).file_name().unwrap().to_string_lossy(),
        ".W.STL.planora-tmp"
    );
    let _ = fs::remove_dir_all(&dir);
}
