//! Fehler, Grenzen, Netztyp, Lage und Fortschritt (Teilprojekt 9, Task 1).

use planora_exchange::*;
use planora_math::{DQuat, DVec3};

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    }
}

#[test]
fn limits_match_the_spec() {
    assert_eq!(MAX_TRIANGLES, 50_000_000);
    assert_eq!(MAX_FILE_BYTES, 8 * 1024 * 1024 * 1024);
    assert_eq!(MAX_VERTICES, 150_000_000);
    assert_eq!(MAX_LINE_BYTES, 1 << 20);
    assert_eq!(RESERVE_STEP, 65_536);
}

#[test]
fn error_texts_are_german_and_name_the_line() {
    assert_eq!(
        ExchangeError::BadNumber { line: 7 }.to_string(),
        "Zeile 7: Die Zahl lässt sich nicht lesen"
    );
    assert!(
        ExchangeError::NotFinite { line: Some(3) }
            .to_string()
            .starts_with("Zeile 3: ")
    );
    assert!(
        !ExchangeError::NotFinite { line: None }
            .to_string()
            .contains("Zeile")
    );
    assert_eq!(
        ExchangeError::Syntax {
            line: 12,
            msg: "Unbekanntes Wort".to_string()
        }
        .to_string(),
        "Zeile 12: Unbekanntes Wort"
    );
    assert!(
        ExchangeError::IndexOutOfRange { line: 4 }
            .to_string()
            .starts_with("Zeile 4: ")
    );
    assert!(
        ExchangeError::Polygon { line: 9 }
            .to_string()
            .starts_with("Zeile 9: ")
    );
    let big = ExchangeError::FileTooLarge {
        bytes: MAX_FILE_BYTES + 1,
    }
    .to_string();
    assert!(big.contains("8 GB"), "{big}");
    let count = ExchangeError::CountTooLarge {
        claimed: 1000,
        possible: 2,
    }
    .to_string();
    assert!(count.contains("1000") && count.contains('2'), "{count}");
    assert!(
        ExchangeError::TooManyTriangles
            .to_string()
            .contains("50 Millionen")
    );
    assert!(
        ExchangeError::TooLargeForFormat { format: "3MF" }
            .to_string()
            .contains("3MF")
    );
    assert!(
        ExchangeError::UnknownFormat("step".into())
            .to_string()
            .contains("„step“")
    );
    assert_eq!(ExchangeError::Cancelled.to_string(), "Abgebrochen");
    let eof: ExchangeError = std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into();
    assert_eq!(eof, ExchangeError::Truncated);
    let other: ExchangeError = std::io::Error::other("kaputt").into();
    assert!(matches!(other, ExchangeError::Io { .. }));
}

#[test]
fn validation_rejects_broken_meshes_by_name() {
    tetra().validate().unwrap();
    assert_eq!(
        ExchangeMesh::default().validate(),
        Err(ExchangeError::Empty)
    );
    let mut m = tetra();
    m.indices.pop();
    assert!(matches!(m.validate(), Err(ExchangeError::Corrupt(_))));
    let mut m = tetra();
    m.indices[0] = 4;
    assert!(matches!(m.validate(), Err(ExchangeError::Corrupt(_))));
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut m = tetra();
        m.positions[1][2] = bad;
        assert_eq!(m.validate(), Err(ExchangeError::NotFinite { line: None }));
    }
}

#[test]
fn the_transform_scales_then_rotates_then_moves() {
    let t = MeshTransform {
        offset: DVec3::new(1.0, 2.0, 3.0),
        rotation: DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2),
        scale: 2.0,
    };
    let p = t.apply(DVec3::X);
    assert!((p - DVec3::new(1.0, 4.0, 3.0)).length() < 1e-12, "{p}");
    assert_eq!(MeshTransform::default(), MeshTransform::IDENTITY);
    assert_eq!(
        MeshTransform::IDENTITY.apply(DVec3::new(5.0, -1.0, 2.0)),
        DVec3::new(5.0, -1.0, 2.0)
    );
}

#[test]
fn mass_follows_the_transform() {
    let m = tetra();
    let v = m.mass(&MeshTransform::IDENTITY);
    assert!((v.volume - 1000.0 / 6.0).abs() < 1e-9, "{}", v.volume);
    let t = MeshTransform {
        offset: DVec3::new(100.0, 0.0, 0.0),
        rotation: DQuat::from_rotation_x(0.3),
        scale: 2.0,
    };
    let w = m.mass(&t);
    assert!((w.volume - 8.0 * 1000.0 / 6.0).abs() < 1e-6, "{}", w.volume);
    let placed = m.placed(&t);
    assert_eq!(placed.positions.len(), 4);
    assert_eq!(placed.indices, m.indices);
    assert!((placed.positions[0] - DVec3::new(100.0, 0.0, 0.0)).length() < 1e-12);
    assert_eq!(m.point(1), DVec3::new(10.0, 0.0, 0.0));
    assert_eq!((m.vertex_count(), m.triangle_count()), (4, 4));
}

#[test]
fn progress_reports_fraction_and_cancel() {
    let p = Progress::new();
    assert_eq!(p.fraction(), 0.0);
    p.set_total(200);
    p.advance(50);
    assert!((p.fraction() - 0.25).abs() < 1e-6);
    p.advance(500);
    assert_eq!(p.fraction(), 1.0);
    assert_eq!(p.check(), Ok(()));
    assert!(!p.is_cancelled());
    p.cancel();
    assert!(p.is_cancelled());
    assert_eq!(p.check(), Err(ExchangeError::Cancelled));
}
