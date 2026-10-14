//! Kompakte Netzdaten für das Dokument (Spec §7, Teilprojekt 9, Task 11).

use planora_exchange::*;

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
fn bytes_round_trip() {
    let m = tetra();
    let b = m.to_bytes();
    assert_eq!(b.len(), 8 + 12 * 4 + 12 * 4);
    assert_eq!(&b[..8], &[4, 0, 0, 0, 4, 0, 0, 0]);
    assert_eq!(&b[8 + 12..8 + 16], &10.0f32.to_le_bytes());
    assert_eq!(ExchangeMesh::from_bytes(&b).unwrap(), m);
    assert_eq!(
        ExchangeMesh::byte_len_from_header(&b[..8]),
        Ok(b.len() as u64)
    );
}

#[test]
fn damaged_bytes_are_named() {
    let good = tetra().to_bytes();
    assert!(matches!(
        ExchangeMesh::from_bytes(&good[..7]),
        Err(ExchangeError::Corrupt(_))
    ));
    assert!(matches!(
        ExchangeMesh::from_bytes(&good[..good.len() - 4]),
        Err(ExchangeError::Corrupt(_))
    ));
    let mut more = good.clone();
    more[4] = 5;
    assert!(matches!(
        ExchangeMesh::from_bytes(&more),
        Err(ExchangeError::Corrupt(_))
    ));
    let mut huge = good.clone();
    huge[4..8].copy_from_slice(&(MAX_TRIANGLES as u32 + 1).to_le_bytes());
    assert_eq!(
        ExchangeMesh::from_bytes(&huge),
        Err(ExchangeError::TooManyTriangles)
    );
    let mut many = good.clone();
    many[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        ExchangeMesh::from_bytes(&many),
        Err(ExchangeError::TooManyVertices)
    );
    let mut nan = good.clone();
    nan[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
    assert_eq!(
        ExchangeMesh::from_bytes(&nan),
        Err(ExchangeError::NotFinite { line: None })
    );
    let mut index = good.clone();
    let at = 8 + 48;
    index[at..at + 4].copy_from_slice(&9u32.to_le_bytes());
    assert!(matches!(
        ExchangeMesh::from_bytes(&index),
        Err(ExchangeError::Corrupt(_))
    ));
    let none = [0u8; 8];
    assert_eq!(ExchangeMesh::from_bytes(&none), Err(ExchangeError::Empty));
}
