//! Dateien (Plan P9): Formate mit Endung und Größenschätzung, Schreiben über eine Hilfsdatei
//! im Zielordner (Spec §5) und Lesen nach der Endung.

use crate::error::ExchangeError;
use crate::gltf::write_glb;
use crate::limits::MAX_FILE_BYTES;
use crate::mesh::ExchangeMesh;
use crate::obj::{read_obj, write_obj};
use crate::progress::Progress;
use crate::source::ExportBody;
use crate::stl::{STL_HEADER_BYTES, STL_TRIANGLE_BYTES, read_stl, write_stl};
use crate::threemf::write_3mf;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Ausgabeformate (Spec E2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Stl,
    Obj,
    ThreeMf,
    Glb,
}

// Schätzwerte für die Dateigröße (Spec E5, „ca. x MB“), in Bytes je Punkt und je Dreieck.
// Gemessen an Kugel und Zylinder mit sechsstelligen Koordinaten (Stufentest 2) und gerundet;
// die Tests verlangen das Ergebnis innerhalb von Faktor 2.
/// Kopfzeile „# PlanoraCAD“.
const OBJ_BASE: u64 = 13;
/// Zeilen „v …“ und „vn …“ je Punkt.
const OBJ_PER_VERTEX: u64 = 70;
/// Zeile „f a//a b//b c//c“ je Dreieck.
const OBJ_PER_TRIANGLE: u64 = 45;
/// Paketdateien, ZIP-Köpfe und XML-Rahmen.
const THREEMF_BASE: u64 = 1500;
const THREEMF_PER_VERTEX: u64 = 56;
const THREEMF_PER_TRIANGLE: u64 = 50;
/// Köpfe und JSON.
const GLB_BASE: u64 = 1024;
/// Punkt und Normale (24 Bytes), dazu Kopien an harten Kanten.
const GLB_PER_VERTEX: u64 = 36;
/// Drei Punktnummern zu 4 Bytes.
const GLB_PER_TRIANGLE: u64 = 12;

impl Format {
    pub const ALL: [Format; 4] = [Format::Stl, Format::Obj, Format::ThreeMf, Format::Glb];

    /// Dateiendung ohne Punkt.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Stl => "stl",
            Format::Obj => "obj",
            Format::ThreeMf => "3mf",
            Format::Glb => "glb",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Stl => "STL",
            Format::Obj => "OBJ",
            Format::ThreeMf => "3MF",
            Format::Glb => "glTF (.glb)",
        }
    }

    /// Geschätzte Dateigröße in Bytes (STL genau).
    pub fn estimate_bytes(self, triangles: u64, vertices: u64) -> u64 {
        match self {
            Format::Stl => STL_HEADER_BYTES + STL_TRIANGLE_BYTES * triangles,
            Format::Obj => OBJ_BASE + OBJ_PER_VERTEX * vertices + OBJ_PER_TRIANGLE * triangles,
            Format::ThreeMf => {
                THREEMF_BASE + THREEMF_PER_VERTEX * vertices + THREEMF_PER_TRIANGLE * triangles
            }
            Format::Glb => GLB_BASE + GLB_PER_VERTEX * vertices + GLB_PER_TRIANGLE * triangles,
        }
    }

    pub fn write<W: Write>(self, w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError> {
        match self {
            Format::Stl => write_stl(w, bodies),
            Format::Obj => write_obj(w, bodies),
            Format::ThreeMf => write_3mf(w, bodies),
            Format::Glb => write_glb(w, bodies),
        }
    }
}

/// Schreibt `bodies` im Format `format` nach `path`: erst in die Hilfsdatei [`temp_path`],
/// `sync_all`, dann umbenennen. Bei jedem Fehler bleibt eine vorhandene Zieldatei unverändert
/// und die Hilfsdatei wird entfernt (Spec §5). Liefert die Dateigröße in Bytes.
pub fn export_file(
    path: &Path,
    format: Format,
    bodies: &[ExportBody],
) -> Result<u64, ExchangeError> {
    if bodies.iter().all(|b| b.mesh.triangle_count() == 0) {
        return Err(ExchangeError::Empty);
    }
    let tmp = temp_path(path);
    let result = write_to(&tmp, format, bodies).and_then(|len| {
        fs::rename(&tmp, path)
            .map(|()| len)
            .map_err(ExchangeError::from)
    });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_to(tmp: &Path, format: Format, bodies: &[ExportBody]) -> Result<u64, ExchangeError> {
    let mut w = BufWriter::new(File::create(tmp)?);
    format.write(&mut w, bodies)?;
    let file = w
        .into_inner()
        .map_err(|e| ExchangeError::from(e.into_error()))?;
    file.sync_all()?;
    Ok(file.metadata()?.len())
}

/// Hilfsdatei `.<Name>.planora-tmp` im Ordner des Ziels.
pub fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.planora-tmp"))
}

/// Liest ein Netz nach der Dateiendung (`.stl` oder `.obj`, Groß- und Kleinschreibung egal).
pub fn import_file(path: &Path, progress: &Progress) -> Result<ExchangeMesh, ExchangeError> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext != "stl" && ext != "obj" {
        return Err(ExchangeError::UnknownFormat(ext));
    }
    let len = fs::metadata(path)?.len();
    if len > MAX_FILE_BYTES {
        return Err(ExchangeError::FileTooLarge { bytes: len });
    }
    let file = File::open(path)?;
    if ext == "stl" {
        read_stl(file, len, progress)
    } else {
        read_obj(file, len, progress)
    }
}
