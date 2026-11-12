//! Fuzz-artige Tests des DXF-Lesers: abgeschnittene und zufällig veränderte Dateien dürfen
//! nie abstürzen. Erlaubt ist nur `Ok` oder `Err`.

use planora_exchange::read_dxf;
use std::path::PathBuf;

const NEVER: &dyn Fn() -> bool = &|| false;

/// Alle Testdateien aus `tests/data/dxf`, nach Name sortiert.
fn samples() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/dxf");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "dxf"))
        .collect();
    files.sort();
    assert!(files.len() >= 3, "Testdateien fehlen in {dir:?}");
    files
        .into_iter()
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect()
}

/// Einfacher Zufallszahlengenerator (xorshift) mit festem Startwert.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn truncated_files_never_panic() {
    for (_, bytes) in samples() {
        for i in 0..50 {
            let cut = bytes.len() * i / 50;
            // Nur Ok oder Err; ein Panic ließe den Test scheitern.
            let _ = read_dxf(&bytes[..cut], NEVER);
        }
    }
}

#[test]
fn byte_flips_never_panic() {
    let files = samples();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for n in 0..500 {
        let (_, original) = &files[n % files.len()];
        let mut bytes = original.clone();
        for _ in 0..1 + rng.next() % 8 {
            let at = (rng.next() % bytes.len() as u64) as usize;
            bytes[at] = rng.next() as u8;
        }
        let _ = read_dxf(&bytes, NEVER);
    }
}

#[test]
fn binary_variants_never_panic() {
    // Auch die Binärform, aus den Textdateien erzeugt, wird abgeschnitten und verändert.
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for (_, text) in samples() {
        let Ok(drawing) = dxf::Drawing::load(&mut std::io::Cursor::new(&text)) else {
            continue;
        };
        let mut binary = Vec::new();
        drawing.save_binary(&mut binary).unwrap();
        for i in 0..50 {
            let _ = read_dxf(&binary[..binary.len() * i / 50], NEVER);
        }
        for _ in 0..100 {
            let mut bytes = binary.clone();
            for _ in 0..1 + rng.next() % 4 {
                let at = (rng.next() % bytes.len() as u64) as usize;
                bytes[at] = rng.next() as u8;
            }
            let _ = read_dxf(&bytes, NEVER);
        }
    }
}
