//! Schriftliste und Schnittwahl. Die Tests benutzen nur die mitgelieferten Schriften und
//! eigene Ordner, nie die Schriften des Systems.

use std::fs;
use std::path::{Path, PathBuf};

use planora_text::{FALLBACK_FAMILY, FontLibrary};

const ROBOTO: &[u8] = include_bytes!("../../planora-app/assets/fonts/Roboto-Regular.ttf");
const MONTSERRAT_SEMIBOLD: &[u8] =
    include_bytes!("../../planora-app/assets/fonts/Montserrat-SemiBold.ttf");

/// Eigener Ordner je Test, wird am Ende wieder gelöscht.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("planora-text-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, rel: &str, bytes: &[u8]) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn be_u16(data: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([data[at], data[at + 1]])
}

fn be_u32(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

/// Baut eine Schriftsammlung (`.ttc`) aus einzelnen Schriften. In einer Sammlung zählen die
/// Tabellen-Versätze vom Dateianfang, darum werden sie je Schrift verschoben.
fn build_ttc(fonts: &[&[u8]]) -> Vec<u8> {
    let header_len = 12 + 4 * fonts.len();
    let mut out = Vec::new();
    out.extend_from_slice(b"ttcf");
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&(fonts.len() as u32).to_be_bytes());
    out.resize(header_len, 0);
    for (i, font) in fonts.iter().enumerate() {
        while out.len() % 4 != 0 {
            out.push(0);
        }
        let base = out.len();
        out[12 + 4 * i..16 + 4 * i].copy_from_slice(&(base as u32).to_be_bytes());
        let mut copy = font.to_vec();
        let num_tables = be_u16(&copy, 4) as usize;
        for t in 0..num_tables {
            let at = 12 + 16 * t + 8;
            let shifted = be_u32(&copy, at) + base as u32;
            copy[at..at + 4].copy_from_slice(&shifted.to_be_bytes());
        }
        out.extend_from_slice(&copy);
    }
    out
}

/// Benennt eine Tabelle im Inhaltsverzeichnis um, sodass die Schrift sie nicht mehr hat.
fn rename_table(font: &[u8], tag: &[u8; 4], new_tag: &[u8; 4]) -> Vec<u8> {
    let mut copy = font.to_vec();
    let num_tables = be_u16(&copy, 4) as usize;
    let mut found = false;
    for t in 0..num_tables {
        let at = 12 + 16 * t;
        if &copy[at..at + 4] == tag {
            copy[at..at + 4].copy_from_slice(new_tag);
            found = true;
        }
    }
    assert!(found, "Tabelle fehlt in der Testschrift");
    copy
}

#[test]
fn bundled_has_montserrat_and_roboto() {
    let lib = FontLibrary::bundled_only();
    let families = lib.families();
    assert!(families.len() >= 2, "{families:?}");
    assert_eq!(
        &families[..2],
        &["Montserrat".to_string(), "Roboto".to_string()]
    );
    assert_eq!(FALLBACK_FAMILY, "Montserrat");
}

#[test]
fn scan_reads_ttf_and_ttc_in_subfolders() {
    let dir = TempDir::new("ttf-ttc");
    dir.write("a/x.ttf", ROBOTO);
    dir.write("b/c/Y.TTC", &build_ttc(&[ROBOTO, MONTSERRAT_SEMIBOLD]));
    let bundled = FontLibrary::bundled_only();
    let lib = FontLibrary::scan(&[dir.path().to_path_buf()]);
    // Jede Familie genau einmal, obwohl Roboto jetzt dreimal da ist.
    assert_eq!(lib.families(), bundled.families());
    // Drei Schnitte dazu: Roboto aus der .ttf, Roboto und Montserrat SemiBold aus der .ttc.
    assert_eq!(lib.face_count(), bundled.face_count() + 3);
    assert!(lib.skipped().is_empty(), "{:?}", lib.skipped());
}

#[test]
fn scan_skips_broken_files() {
    let dir = TempDir::new("broken");
    dir.write("cut.ttf", &ROBOTO[..100]);
    dir.write("leer.otf", &[]);
    dir.write(
        "tief/unsinn.ttc",
        b"ttcf das ist keine Schriftsammlung, nur Unsinn",
    );
    let bundled = FontLibrary::bundled_only();
    let lib = FontLibrary::scan(&[dir.path().to_path_buf()]);
    assert_eq!(lib.families(), bundled.families());
    assert_eq!(lib.face_count(), bundled.face_count());
    assert_eq!(lib.skipped().len(), 3, "{:?}", lib.skipped());
}

#[test]
fn scan_skips_font_without_outlines() {
    let dir = TempDir::new("no-outlines");
    dir.write("ohne-glyf.ttf", &rename_table(ROBOTO, b"glyf", b"xxxx"));
    let bundled = FontLibrary::bundled_only();
    let lib = FontLibrary::scan(&[dir.path().to_path_buf()]);
    assert_eq!(lib.face_count(), bundled.face_count());
    assert_eq!(lib.skipped().len(), 1, "{:?}", lib.skipped());
}

#[test]
fn scan_skips_files_over_64_mb_without_reading() {
    let dir = TempDir::new("huge");
    dir.write("klein.ttf", ROBOTO);
    // Nur die Länge setzen, nichts schreiben: Die Datei ist 64 MB plus ein Byte groß.
    let huge = dir.path().join("riesig.ttf");
    fs::File::create(&huge)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    let bundled = FontLibrary::bundled_only();
    let lib = FontLibrary::scan(&[dir.path().to_path_buf()]);
    assert_eq!(lib.face_count(), bundled.face_count() + 1);
    assert_eq!(lib.skipped().len(), 1, "{:?}", lib.skipped());
    let (path, reason) = &lib.skipped()[0];
    assert!(path.ends_with("riesig.ttf"), "{path:?}");
    assert!(reason.contains("64 MB"), "{reason}");
}

#[test]
fn scan_ignores_missing_dirs_and_other_files() {
    let dir = TempDir::new("other");
    dir.write("liesmich.txt", b"keine Schrift");
    dir.write("roboto.ttf.bak", ROBOTO);
    let bundled = FontLibrary::bundled_only();
    let lib = FontLibrary::scan(&[dir.path().to_path_buf(), dir.path().join("gibt-es-nicht")]);
    assert_eq!(lib.face_count(), bundled.face_count());
    assert!(lib.skipped().is_empty(), "{:?}", lib.skipped());
}

#[test]
fn select_missing_family_falls_back() {
    let lib = FontLibrary::bundled_only();
    let choice = lib.select("Gibtsnicht", false, false);
    assert!(choice.substituted);
    assert_eq!(choice.face.family, "Montserrat");
    assert_eq!(choice.face.weight, 400);
    assert!(!choice.synth_bold && !choice.synth_italic);
}

#[test]
fn select_missing_family_bold_italic_uses_semibold_and_synthetic_italic() {
    let lib = FontLibrary::bundled_only();
    let choice = lib.select("Gibtsnicht", true, true);
    assert!(choice.substituted);
    assert_eq!(choice.face.family, "Montserrat");
    assert_eq!(choice.face.style, "SemiBold");
    assert!(!choice.synth_bold);
    assert!(choice.synth_italic);
}

#[test]
fn select_bold_uses_semibold_for_montserrat() {
    let lib = FontLibrary::bundled_only();
    let choice = lib.select("Montserrat", true, false);
    assert!(!choice.substituted);
    assert_eq!(choice.face.family, "Montserrat");
    assert_eq!(choice.face.style, "SemiBold");
    assert_eq!(choice.face.weight, 600);
    assert!(!choice.synth_bold);
    assert!(!choice.synth_italic);
    assert_eq!(choice.face.wght, None);
}

#[test]
fn select_regular_montserrat_is_regular() {
    let lib = FontLibrary::bundled_only();
    let choice = lib.select("Montserrat", false, false);
    assert_eq!(choice.face.style, "Regular");
    assert_eq!(choice.face.weight, 400);
    assert!(!choice.substituted && !choice.synth_bold && !choice.synth_italic);
}

#[test]
fn select_italic_without_face_is_synthetic() {
    let lib = FontLibrary::bundled_only();
    let choice = lib.select("Roboto", false, true);
    assert!(!choice.substituted);
    assert_eq!(choice.face.family, "Roboto");
    assert!(choice.synth_italic);
    assert!(!choice.synth_bold);
}

#[test]
fn select_bold_without_face_is_synthetic() {
    let lib = FontLibrary::bundled_only();
    let choice = lib.select("Roboto", true, false);
    assert!(!choice.substituted);
    assert!(choice.synth_bold);
    assert!(!choice.synth_italic);
}

#[test]
fn family_match_ignores_case() {
    let lib = FontLibrary::bundled_only();
    assert!(lib.has_family("montserrat"));
    assert!(lib.has_family("ROBOTO"));
    assert!(!lib.has_family("Gibtsnicht"));
    let choice = lib.select("roboto", false, false);
    assert!(!choice.substituted);
    assert_eq!(choice.face.family, "Roboto");
}

#[test]
fn face_ref_bytes_open_as_font() {
    let dir = TempDir::new("face-bytes");
    dir.write("sammlung.ttc", &build_ttc(&[ROBOTO, MONTSERRAT_SEMIBOLD]));
    let lib = FontLibrary::scan(&[dir.path().to_path_buf()]);
    // Montserrat SemiBold gibt es mitgeliefert und als Eintrag 1 der Sammlung; beide müssen
    // sich öffnen lassen.
    let choice = lib.select("Montserrat", true, false);
    assert!(choice.face.font().is_some());
    let roboto = lib.select("Roboto", false, false);
    assert!(roboto.face.font().is_some());
}

#[cfg(windows)]
#[test]
fn system_font_dirs_on_windows() {
    let dirs = planora_text::system_font_dirs();
    if let Ok(windir) = std::env::var("WINDIR") {
        assert!(
            dirs.contains(&PathBuf::from(windir).join("Fonts")),
            "{dirs:?}"
        );
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let user = PathBuf::from(local)
            .join("Microsoft")
            .join("Windows")
            .join("Fonts");
        assert!(dirs.contains(&user), "{dirs:?}");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn system_font_dirs_on_linux() {
    let dirs = planora_text::system_font_dirs();
    assert!(
        dirs.contains(&PathBuf::from("/usr/share/fonts")),
        "{dirs:?}"
    );
    assert!(
        dirs.contains(&PathBuf::from("/usr/local/share/fonts")),
        "{dirs:?}"
    );
}
