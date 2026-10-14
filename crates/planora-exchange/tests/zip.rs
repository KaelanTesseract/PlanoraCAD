//! ZIP-Schreiber und CRC-32 (Teilprojekt 9, Task 6).

mod common;

use common::{crc32_slow, read_zip};
use planora_exchange::*;
use std::cell::Cell;

#[test]
fn crc32_matches_the_standard_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
    let mut c = Crc32::new();
    c.update(b"1234");
    c.update(b"56789");
    assert_eq!(c.finish(), 0xCBF4_3926);
    let text = b"PlanoraCAD schreibt 3MF";
    assert_eq!(crc32_slow(text), crc32(text));
}

#[test]
fn entries_are_stored_with_crc_and_directory() {
    let mut z = ZipWriter::new(Vec::new());
    z.add_entry("a.txt", &|w| w.write_all(b"Hallo")).unwrap();
    z.add_entry("ordner/b.bin", &|w| {
        for i in 0..1000u32 {
            w.write_all(&i.to_le_bytes())?;
        }
        Ok(())
    })
    .unwrap();
    z.add_entry("leer", &|_| Ok(())).unwrap();
    let bytes = z.finish().unwrap();
    let entries = read_zip(&bytes).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["a.txt", "ordner/b.bin", "leer"]);
    assert_eq!(entries[0].data, b"Hallo");
    assert_eq!(entries[1].data.len(), 4000);
    assert_eq!(&entries[1].data[4..8], &1u32.to_le_bytes());
    assert!(entries[2].data.is_empty());
}

#[test]
fn the_same_content_gives_the_same_file() {
    let make = || {
        let mut z = ZipWriter::new(Vec::new());
        z.add_entry("x", &|w| w.write_all(b"abc")).unwrap();
        z.finish().unwrap()
    };
    assert_eq!(make(), make());
}

#[test]
fn content_that_changes_between_passes_is_refused() {
    let calls = Cell::new(0u8);
    let mut z = ZipWriter::new(Vec::new());
    let e = z
        .add_entry("x", &|w| {
            calls.set(calls.get() + 1);
            w.write_all(&[calls.get()])
        })
        .unwrap_err();
    assert!(matches!(e, ExchangeError::Corrupt(_)), "{e:?}");
}

#[test]
fn an_empty_archive_is_valid() {
    let bytes = ZipWriter::new(Vec::new()).finish().unwrap();
    assert_eq!(bytes.len(), 22);
    assert!(read_zip(&bytes).unwrap().is_empty());
}
