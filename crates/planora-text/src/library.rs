//! Schriftliste: mitgelieferte Schriften und die Schriften aus den Schriftordnern.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use skrifa::FontRef;
use skrifa::raw::FileRef;

use crate::FALLBACK_FAMILY;
use crate::face::{FaceChoice, FaceInfo, FaceRef, pick_face, read_face_info};

// Mitgelieferte Schriften der App, ohne Kopie direkt aus deren Ordner.
const MONTSERRAT_REGULAR: &[u8] =
    include_bytes!("../../planora-app/assets/fonts/Montserrat-Regular.ttf");
const MONTSERRAT_SEMIBOLD: &[u8] =
    include_bytes!("../../planora-app/assets/fonts/Montserrat-SemiBold.ttf");
const ROBOTO_REGULAR: &[u8] = include_bytes!("../../planora-app/assets/fonts/Roboto-Regular.ttf");
const BUNDLED: [&[u8]; 3] = [MONTSERRAT_REGULAR, MONTSERRAT_SEMIBOLD, ROBOTO_REGULAR];

/// Dateiendungen, die als Schrift gelesen werden (ohne Rücksicht auf Groß- und Kleinschreibung).
const FONT_EXTENSIONS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];
/// Tiefe der Unterordner, bis zu der gesucht wird. Schützt vor sehr tiefen Bäumen.
const MAX_DEPTH: usize = 32;
/// Größere Dateien werden nicht gelesen, sondern übersprungen (keine echte Schrift ist so groß).
const MAX_FONT_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Woher die Bytes eines Schnitts kommen.
#[derive(Clone)]
enum Source {
    /// Mitgeliefert, die Bytes liegen schon bereit.
    Bundled(Arc<[u8]>),
    /// Aus einem Schriftordner; die Bytes werden erst bei der Wahl gelesen, damit nicht alle
    /// Systemschriften im Speicher liegen.
    File(PathBuf),
}

#[derive(Clone)]
struct Entry {
    info: FaceInfo,
    source: Source,
    index: u32,
}

impl Entry {
    fn bundled(&self) -> bool {
        matches!(self.source, Source::Bundled(_))
    }
}

/// Alle bekannten Schriften. Wird einmal je Programmlauf aufgebaut.
pub struct FontLibrary {
    entries: Vec<Entry>,
    skipped: Vec<(PathBuf, String)>,
    /// Schon gelesene Dateien, damit eine Datei bei jeder Wahl nicht neu gelesen wird.
    cache: Mutex<HashMap<PathBuf, Arc<[u8]>>>,
}

impl fmt::Debug for FontLibrary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontLibrary")
            .field("faces", &self.entries.len())
            .field("skipped", &self.skipped.len())
            .finish()
    }
}

impl FontLibrary {
    /// Nur die mitgelieferten Schriften (Montserrat Regular und SemiBold, Roboto Regular).
    pub fn bundled_only() -> FontLibrary {
        let mut entries = Vec::new();
        for bytes in BUNDLED {
            let data: Arc<[u8]> = Arc::from(bytes);
            // Die mitgelieferten Schriften sind geprüft; ein Fehler hieße nur, dass der Schnitt
            // fehlt, nie ein Absturz.
            if let Ok(faces) = read_file_faces(&data) {
                entries.extend(faces.into_iter().map(|(index, info)| Entry {
                    info,
                    source: Source::Bundled(data.clone()),
                    index,
                }));
            }
        }
        FontLibrary {
            entries,
            skipped: Vec::new(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Durchsucht die Ordner samt Unterordnern nach `.ttf`, `.otf`, `.ttc` und `.otc`.
    /// Kaputte Dateien werden übersprungen und in [`FontLibrary::skipped`] vermerkt; fehlende
    /// Ordner werden still übergangen. Die mitgelieferten Schriften sind immer dabei, vorn.
    pub fn scan(dirs: &[PathBuf]) -> FontLibrary {
        let mut lib = FontLibrary::bundled_only();
        let mut files = Vec::new();
        let mut seen_dirs = HashSet::new();
        for dir in dirs {
            collect_font_files(dir, 0, &mut seen_dirs, &mut files);
        }
        // Feste Reihenfolge, unabhängig davon, wie das Dateisystem die Einträge liefert.
        files.sort();
        files.dedup();
        for path in files {
            match read_limited(&path) {
                Ok(bytes) => match read_file_faces(&bytes) {
                    Ok(faces) => {
                        for (index, info) in faces {
                            lib.entries.push(Entry {
                                info,
                                source: Source::File(path.clone()),
                                index,
                            });
                        }
                    }
                    Err(reason) => lib.skipped.push((path, reason)),
                },
                Err(reason) => lib.skipped.push((path, reason)),
            }
        }
        lib
    }

    /// Familien, mitgelieferte zuerst, sonst alphabetisch. Jede Familie steht einmal da,
    /// auch wenn sie in mehreren Dateien oder Schnitten vorkommt.
    pub fn families(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut list: Vec<(bool, String, String)> = Vec::new();
        for entry in &self.entries {
            let key = entry.info.family.to_lowercase();
            if seen.insert(key.clone()) {
                list.push((false, key, entry.info.family.clone()));
            }
        }
        for item in &mut list {
            item.0 = self
                .entries
                .iter()
                .any(|e| e.bundled() && e.info.family.to_lowercase() == item.1);
        }
        // Mitgelieferte vorn (`true` vor `false`), dann nach Namen ohne Groß/Klein.
        list.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        list.into_iter().map(|(_, _, name)| name).collect()
    }

    /// Gibt es die Familie? Groß- und Kleinschreibung zählen nicht.
    pub fn has_family(&self, name: &str) -> bool {
        let key = name.trim().to_lowercase();
        self.entries
            .iter()
            .any(|e| e.info.family.to_lowercase() == key)
    }

    /// Anzahl der bekannten Schnitte (auch doppelte aus mehreren Dateien).
    pub fn face_count(&self) -> usize {
        self.entries.len()
    }

    /// Übersprungene Dateien mit Grund, zum Protokollieren.
    pub fn skipped(&self) -> &[(PathBuf, String)] {
        &self.skipped
    }

    /// Wählt den passendsten Schnitt. Fehlt die Familie (oder lässt sich keine ihrer Dateien
    /// mehr lesen), gilt die Ersatzschrift [`FALLBACK_FAMILY`] mit `substituted`.
    pub fn select(&self, family: &str, bold: bool, italic: bool) -> FaceChoice {
        if let Some(choice) = self.select_in(family, bold, italic) {
            return choice;
        }
        let mut choice = self
            .select_in(FALLBACK_FAMILY, bold, italic)
            .unwrap_or_else(|| last_resort(bold, italic));
        choice.substituted = true;
        choice
    }

    /// Wahl innerhalb einer Familie; ein Schnitt, dessen Datei nicht mehr passt, fällt weg
    /// und die Wahl läuft unter den übrigen weiter.
    fn select_in(&self, family: &str, bold: bool, italic: bool) -> Option<FaceChoice> {
        let key = family.trim().to_lowercase();
        let mut candidates: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| e.info.family.to_lowercase() == key)
            .collect();
        while !candidates.is_empty() {
            let infos: Vec<&FaceInfo> = candidates.iter().map(|e| &e.info).collect();
            let pick = pick_face(&infos, bold, italic)?;
            let entry = candidates[pick.index];
            if let Some(data) = self.load(entry) {
                return Some(FaceChoice {
                    face: FaceRef {
                        data,
                        index: entry.index,
                        wght: pick.wght,
                        family: entry.info.family.clone(),
                        style: entry.info.style.clone(),
                        weight: entry.info.weight,
                        italic: entry.info.italic,
                    },
                    synth_bold: pick.synth_bold,
                    synth_italic: pick.synth_italic,
                    substituted: false,
                });
            }
            candidates.remove(pick.index);
        }
        None
    }

    /// Bytes eines Schnitts. Eine Datei wird nur genommen, wenn sie den Schnitt noch enthält.
    fn load(&self, entry: &Entry) -> Option<Arc<[u8]>> {
        match &entry.source {
            Source::Bundled(data) => Some(data.clone()),
            Source::File(path) => {
                let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
                let data = match cache.get(path) {
                    Some(data) => data.clone(),
                    None => {
                        let data: Arc<[u8]> = Arc::from(read_limited(path).ok()?);
                        cache.insert(path.clone(), data.clone());
                        data
                    }
                };
                drop(cache);
                // Die Datei kann sich seit dem Durchsuchen geändert haben.
                let same = guarded(|| {
                    FontRef::from_index(&data, entry.index)
                        .ok()
                        .and_then(|font| read_face_info(&font).ok())
                        .is_some_and(|info| info == entry.info)
                })
                .unwrap_or(false);
                same.then_some(data)
            }
        }
    }
}

/// Liest eine Schriftdatei, aber nur bis [`MAX_FONT_FILE_BYTES`]. Die Größe kommt aus den
/// Dateiangaben; eine zu große Datei wird gar nicht erst gelesen.
fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
    let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > MAX_FONT_FILE_BYTES {
        return Err(format!(
            "zu groß ({len} Bytes, höchstens {} MB)",
            MAX_FONT_FILE_BYTES / (1024 * 1024)
        ));
    }
    fs::read(path).map_err(|e| e.to_string())
}

/// Montserrat Regular ohne Umweg über die Liste. Nur für den Fall, dass selbst die
/// mitgelieferten Schriften fehlen würden; so endet die Wahl nie ohne Schnitt.
fn last_resort(bold: bool, italic: bool) -> FaceChoice {
    FaceChoice {
        face: FaceRef {
            data: Arc::from(MONTSERRAT_REGULAR),
            index: 0,
            wght: None,
            family: FALLBACK_FAMILY.to_string(),
            style: "Regular".to_string(),
            weight: 400,
            italic: false,
        },
        synth_bold: bold,
        synth_italic: italic,
        substituted: true,
    }
}

/// Führt das Lesen einer fremden Datei so aus, dass auch ein unerwarteter Fehler im Leser
/// nur diese Datei kostet und nie das Programm.
fn guarded<T>(f: impl FnOnce() -> T) -> Option<T> {
    panic::catch_unwind(AssertUnwindSafe(f)).ok()
}

/// Alle brauchbaren Schnitte einer Datei (eine Schrift oder eine Sammlung). Kaputte Einträge
/// einer Sammlung fallen weg; ist kein Eintrag brauchbar, ist die ganze Datei unbrauchbar.
fn read_file_faces(bytes: &[u8]) -> Result<Vec<(u32, FaceInfo)>, String> {
    guarded(|| {
        let file = FileRef::new(bytes).map_err(|e| e.to_string())?;
        let mut faces = Vec::new();
        let mut first_error = None;
        for (index, font) in file.fonts().enumerate() {
            let result = font
                .map_err(|e| e.to_string())
                .and_then(|font| read_face_info(&font));
            match result {
                Ok(info) => faces.push((index as u32, info)),
                Err(err) => {
                    first_error.get_or_insert(err);
                }
            }
        }
        if faces.is_empty() {
            Err(first_error.unwrap_or_else(|| "keine Schrift in der Datei".to_string()))
        } else {
            Ok(faces)
        }
    })
    .unwrap_or_else(|| Err("Fehler beim Lesen".to_string()))
}

/// Sammelt Schriftdateien rekursiv. Verknüpfte Ordner werden nur einmal besucht, damit
/// Schleifen im Dateisystem nicht hängen lassen.
fn collect_font_files(
    dir: &Path,
    depth: usize,
    seen_dirs: &mut HashSet<PathBuf>,
    files: &mut Vec<PathBuf>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let canonical = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !seen_dirs.insert(canonical) {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        // `metadata` folgt Verknüpfungen, so zählen verlinkte Dateien und Ordner mit.
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            collect_font_files(&path, depth + 1, seen_dirs, files);
        } else if meta.is_file() && is_font_file(&path) {
            files.push(fs::canonicalize(&path).unwrap_or(path));
        }
    }
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            FONT_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// Schriftordner des Systems. Windows: `%WINDIR%\Fonts` und
/// `%LOCALAPPDATA%\Microsoft\Windows\Fonts`. Linux: `/usr/share/fonts`,
/// `/usr/local/share/fonts`, `~/.local/share/fonts`, `~/.fonts`.
pub fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        if let Some(windir) = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot"))
        {
            dirs.push(PathBuf::from(windir).join("Fonts"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(
                PathBuf::from(local)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Fonts"),
            );
        }
    }
    #[cfg(not(windows))]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            dirs.push(home.join(".local").join("share").join("fonts"));
            dirs.push(home.join(".fonts"));
        }
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(family: &str, bundled: bool) -> Entry {
        Entry {
            info: FaceInfo {
                family: family.into(),
                style: "Regular".into(),
                weight: 400,
                italic: false,
                wght_axis: None,
            },
            source: if bundled {
                Source::Bundled(Arc::from(&b""[..]))
            } else {
                Source::File(PathBuf::from("x.ttf"))
            },
            index: 0,
        }
    }

    #[test]
    fn families_bundled_first_then_alphabetical_without_case() {
        let lib = FontLibrary {
            entries: vec![
                entry("Roboto", true),
                entry("Montserrat", true),
                entry("zeta", false),
                entry("Arial", false),
                entry("ROBOTO", false),
                entry("beta", false),
                entry("arial", false),
            ],
            skipped: Vec::new(),
            cache: Mutex::new(HashMap::new()),
        };
        assert_eq!(
            lib.families(),
            ["Montserrat", "Roboto", "Arial", "beta", "zeta"]
        );
    }

    /// Sammlung aus Roboto und Montserrat SemiBold; die Versätze zählen vom Dateianfang.
    fn build_ttc(fonts: &[&[u8]]) -> Vec<u8> {
        let be16 = |d: &[u8], at: usize| u16::from_be_bytes([d[at], d[at + 1]]) as usize;
        let be32 =
            |d: &[u8], at: usize| u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]);
        let mut out = Vec::new();
        out.extend_from_slice(b"ttcf");
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        out.extend_from_slice(&(fonts.len() as u32).to_be_bytes());
        out.resize(12 + 4 * fonts.len(), 0);
        for (i, font) in fonts.iter().enumerate() {
            while out.len() % 4 != 0 {
                out.push(0);
            }
            let base = out.len() as u32;
            out[12 + 4 * i..16 + 4 * i].copy_from_slice(&base.to_be_bytes());
            let mut copy = font.to_vec();
            for t in 0..be16(&copy, 4) {
                let at = 12 + 16 * t + 8;
                let shifted = be32(&copy, at) + base;
                copy[at..at + 4].copy_from_slice(&shifted.to_be_bytes());
            }
            out.extend_from_slice(&copy);
        }
        out
    }

    /// Bibliothek nur mit den Schnitten aus dem Ordner, ohne die mitgelieferten.
    fn scanned_only(dir: &Path) -> FontLibrary {
        let mut lib = FontLibrary::scan(&[dir.to_path_buf()]);
        lib.entries.retain(|e| !e.bundled());
        lib
    }

    #[test]
    fn select_reads_face_from_collection_file_and_survives_changes() {
        let dir = std::env::temp_dir().join(format!("planora-text-load-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sammlung.ttc");
        fs::write(&file, build_ttc(&[ROBOTO_REGULAR, MONTSERRAT_SEMIBOLD])).unwrap();

        let lib = scanned_only(&dir);
        assert_eq!(lib.face_count(), 2);
        let choice = lib.select("Montserrat", true, false);
        assert!(!choice.substituted);
        assert_eq!(choice.face.index, 1);
        assert_eq!(choice.face.style, "SemiBold");
        assert!(choice.face.font().is_some());

        // Datei nach dem Durchsuchen ausgetauscht: Eintrag 1 gibt es nicht mehr.
        let lib = scanned_only(&dir);
        fs::write(&file, ROBOTO_REGULAR).unwrap();
        let choice = lib.select("Montserrat", true, false);
        assert!(choice.substituted);
        assert_eq!(choice.face.family, FALLBACK_FAMILY);

        // Datei nach dem Durchsuchen gelöscht.
        fs::write(&file, build_ttc(&[ROBOTO_REGULAR, MONTSERRAT_SEMIBOLD])).unwrap();
        let lib = scanned_only(&dir);
        fs::remove_file(&file).unwrap();
        let choice = lib.select("Roboto", false, false);
        assert!(choice.substituted);
        assert!(choice.face.font().is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn font_extensions_ignore_case() {
        assert!(is_font_file(Path::new("a/ARIAL.TTF")));
        assert!(is_font_file(Path::new("b.otc")));
        assert!(!is_font_file(Path::new("c.fon")));
        assert!(!is_font_file(Path::new("ttf")));
    }
}
