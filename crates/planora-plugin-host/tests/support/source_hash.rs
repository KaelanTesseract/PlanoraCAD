//! Der Quell-Hash eines Beispiel-Plug-ins: SHA-256 über die sortierten Pfade und Inhalte aller
//! Dateien unter `plugins/<name>/` ohne `target/`. Wird vom Paket-Werkzeug (`examples/pack.rs`) und
//! vom Test `sources_match_packages` gemeinsam benutzt, damit beide gleich rechnen.
//!
//! Zeilenenden (`\r`) werden nicht mitgezählt, damit Windows und Linux denselben Hash ergeben.

use sha2::{Digest, Sha256};
use std::io;
use std::path::Path;

fn collect(
    dir: &Path,
    prefix: &str,
    out: &mut Vec<(String, std::path::PathBuf)>,
) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if prefix.is_empty() && name == "target" {
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if entry.file_type()?.is_dir() {
            collect(&entry.path(), &rel, out)?;
        } else {
            out.push((rel, entry.path()));
        }
    }
    Ok(())
}

/// Der Hash als kleingeschriebene Hexzeichen.
pub fn source_hash(dir: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    collect(dir, "", &mut files)?;
    files.sort();
    let mut h = Sha256::new();
    for (rel, path) in files {
        let bytes: Vec<u8> = std::fs::read(path)?
            .into_iter()
            .filter(|b| *b != b'\r')
            .collect();
        h.update(rel.as_bytes());
        h.update([0]);
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(&bytes);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
