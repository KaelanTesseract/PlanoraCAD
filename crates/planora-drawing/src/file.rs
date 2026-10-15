//! Schreiben einer Zeichnung in eine Datei (SVG oder DXF) über eine Hilfsdatei im Zielordner
//! (wie `planora_exchange::export_file` und `planora_step::export_step_file`).

use crate::error::DrawError;
use crate::sheet::SheetModel;
use crate::{dxf_string, svg_string};
use planora_exchange::{Progress, temp_path};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// So viele Bytes werden zwischen zwei Abbruchprüfungen geschrieben.
const CHUNK_BYTES: usize = 64 * 1024;

/// Dateiformat einer Zeichnung.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DrawingFormat {
    Svg,
    Dxf,
}

impl DrawingFormat {
    /// Dateiendung ohne Punkt.
    pub fn extension(self) -> &'static str {
        match self {
            DrawingFormat::Svg => "svg",
            DrawingFormat::Dxf => "dxf",
        }
    }

    /// Name für die Auswahl.
    pub fn label(self) -> &'static str {
        match self {
            DrawingFormat::Svg => "Zeichnung als SVG",
            DrawingFormat::Dxf => "Zeichnung als DXF",
        }
    }
}

/// Der Inhalt der Datei im gewählten Format.
pub fn drawing_to_bytes(model: &SheetModel, format: DrawingFormat) -> Result<Vec<u8>, DrawError> {
    Ok(match format {
        DrawingFormat::Svg => svg_string(model)?,
        DrawingFormat::Dxf => dxf_string(model)?,
    }
    .into_bytes())
}

/// Schreibt die Zeichnung nach `path`: erst in die Hilfsdatei `.<Name>.planora-tmp`,
/// `sync_all`, dann umbenennen. Bei jedem Fehler (auch Abbrechen über `progress` und ungültigen
/// Zahlen im Modell) bleibt eine vorhandene Zieldatei unverändert und die Hilfsdatei wird
/// entfernt. Liefert die Dateigröße in Bytes.
pub fn export_drawing_file(
    path: &Path,
    model: &SheetModel,
    format: DrawingFormat,
    progress: &Progress,
) -> Result<u64, DrawError> {
    progress.check()?;
    let bytes = drawing_to_bytes(model, format)?;
    let tmp = temp_path(path);
    let result = write_to(&tmp, &bytes, progress).and_then(|n| {
        progress.check()?;
        fs::rename(&tmp, path)?;
        Ok(n)
    });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_to(tmp: &Path, bytes: &[u8], progress: &Progress) -> Result<u64, DrawError> {
    let mut file = File::create(tmp)?;
    progress.set_total(bytes.len().div_ceil(CHUNK_BYTES) as u64);
    for chunk in bytes.chunks(CHUNK_BYTES) {
        progress.check()?;
        file.write_all(chunk)?;
        progress.advance(1);
    }
    file.sync_all()?;
    Ok(file.metadata()?.len())
}
