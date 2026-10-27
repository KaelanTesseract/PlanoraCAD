//! Eigene kleine ZIP-Schreibfunktion (gespeichert, ohne Kompression) für die Projektdatei in
//! der Rückmeldung. Geprüft mit einem eigenen Leser und unter Windows zusätzlich mit
//! `Expand-Archive` aus Windows PowerShell.

mod common;

use planora_app::zip_store;
use std::path::Path;

fn project(base: &Path) -> std::path::PathBuf {
    let doc = base.join("Ölwanne Maße.pcad");
    std::fs::create_dir_all(doc.join("versionen")).unwrap();
    std::fs::create_dir_all(doc.join("leer")).unwrap();
    std::fs::write(doc.join("dokument.json"), "{\"ä\":1}").unwrap();
    std::fs::write(
        doc.join("versionen").join("1 Größe.json"),
        vec![7u8; 70_000],
    )
    .unwrap();
    std::fs::write(doc.join(".autosave.json"), "ungespeichert").unwrap();
    doc
}

#[test]
fn zip_holds_the_folder_without_autosave_with_utf8_names() {
    let base = common::temp_root("planora-zip");
    std::fs::create_dir_all(&base).unwrap();
    let doc = project(&base);
    let out = base.join("Ölwanne Maße.pcad.zip");
    zip_store::write_project_zip(&doc, &out).expect("ZIP geschrieben");
    let bytes = std::fs::read(&out).unwrap();
    let entries = common::zip_entries(&bytes);
    let mut names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "Ölwanne Maße.pcad/",
            "Ölwanne Maße.pcad/dokument.json",
            "Ölwanne Maße.pcad/leer/",
            "Ölwanne Maße.pcad/versionen/",
            "Ölwanne Maße.pcad/versionen/1 Größe.json",
        ]
    );
    for e in &entries {
        assert_eq!(e.method, 0, "gespeichert: {}", e.name);
        assert_ne!(e.flags & 0x0800, 0, "UTF-8-Merker: {}", e.name);
    }
    let big = entries
        .iter()
        .find(|e| e.name.ends_with("1 Größe.json"))
        .unwrap();
    assert_eq!(big.data, vec![7u8; 70_000]);
}

#[test]
fn failed_zip_leaves_no_half_file() {
    let base = common::temp_root("planora-zip-fehler");
    std::fs::create_dir_all(&base).unwrap();
    let out = base.join("x.pcad.zip");
    assert!(zip_store::write_project_zip(&base.join("fehlt.pcad"), &out).is_err());
    assert!(!out.exists());
}

#[cfg(windows)]
#[test]
fn windows_expand_archive_reads_it() {
    let base = common::temp_root("planora-zip-windows");
    std::fs::create_dir_all(&base).unwrap();
    let doc = project(&base);
    let out = base.join("Ölwanne Maße.pcad.zip");
    zip_store::write_project_zip(&doc, &out).unwrap();
    let dest = base.join("entpackt");
    // Pfade über Umgebungsvariablen, damit Umlaute nicht durch die Befehlszeile müssen.
    let status = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Expand-Archive -LiteralPath $env:PLANORA_ZIP -DestinationPath $env:PLANORA_DEST -ErrorAction Stop",
        ])
        .env("PLANORA_ZIP", &out)
        .env("PLANORA_DEST", &dest)
        .status()
        .expect("powershell startet");
    assert!(status.success());
    let root = dest.join("Ölwanne Maße.pcad");
    assert_eq!(
        std::fs::read_to_string(root.join("dokument.json")).unwrap(),
        "{\"ä\":1}"
    );
    assert_eq!(
        std::fs::read(root.join("versionen").join("1 Größe.json")).unwrap(),
        vec![7u8; 70_000]
    );
    assert!(root.join("leer").is_dir());
    assert!(!root.join(".autosave.json").exists());
}
