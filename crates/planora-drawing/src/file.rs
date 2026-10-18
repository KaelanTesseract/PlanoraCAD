//! Schreiben einer Zeichnung in eine Datei (SVG, DXF oder PDF) über eine Hilfsdatei im Zielordner
//! (wie `planora_exchange::export_file` und `planora_step::export_step_file`). SVG, DXF und PDF
//! teilen [`write_via_temp`] (P14).

use crate::error::DrawError;
use crate::pdf::{PdfMeta, pdf_bytes};
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
    Pdf,
}

impl DrawingFormat {
    /// Dateiendung ohne Punkt.
    pub fn extension(self) -> &'static str {
        match self {
            DrawingFormat::Svg => "svg",
            DrawingFormat::Dxf => "dxf",
            DrawingFormat::Pdf => "pdf",
        }
    }

    /// Name für die Auswahl.
    pub fn label(self) -> &'static str {
        match self {
            DrawingFormat::Svg => "Zeichnung als SVG",
            DrawingFormat::Dxf => "Zeichnung als DXF",
            DrawingFormat::Pdf => "Zeichnung als PDF",
        }
    }
}

/// Der Inhalt der Datei im gewählten Format (PDF: eine Seite, Dokumenteigenschaften leer).
pub fn drawing_to_bytes(model: &SheetModel, format: DrawingFormat) -> Result<Vec<u8>, DrawError> {
    Ok(match format {
        DrawingFormat::Svg => svg_string(model)?.into_bytes(),
        DrawingFormat::Dxf => dxf_string(model)?.into_bytes(),
        DrawingFormat::Pdf => pdf_bytes(&[model], &PdfMeta::default())?,
    })
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
    write_via_temp(path, &bytes, progress)
}

/// Schreibt die Blätter als PDF nach `path` (eine Seite je Blatt, in dieser Reihenfolge), über
/// die Hilfsdatei wie [`export_drawing_file`]. Bei jedem Fehler (auch Abbrechen und ungültigen
/// Zahlen) bleibt eine vorhandene Zieldatei unverändert. Liefert die Dateigröße in Bytes.
pub fn export_pdf_file(
    path: &Path,
    pages: &[&SheetModel],
    meta: &PdfMeta,
    progress: &Progress,
) -> Result<u64, DrawError> {
    progress.check()?;
    let bytes = pdf_bytes(pages, meta)?;
    write_via_temp(path, &bytes, progress)
}

/// Schreibt `bytes` nach `path` (P14): erst in die Hilfsdatei `.<Name>.planora-tmp` im
/// Zielordner, `sync_all`, Abbruch prüfen, dann umbenennen. Bei jedem Fehler wird die Hilfsdatei
/// entfernt und eine vorhandene Zieldatei bleibt unverändert. Liefert die Dateigröße in Bytes.
pub(crate) fn write_via_temp(
    path: &Path,
    bytes: &[u8],
    progress: &Progress,
) -> Result<u64, DrawError> {
    write_via_temp_then(path, bytes, progress, || {})
}

/// Wie [`write_via_temp`]; `before_rename` läuft nach `sync_all` direkt vor der letzten
/// Abbruchprüfung (nur für die Tests des Abbruchs in diesem Augenblick).
fn write_via_temp_then(
    path: &Path,
    bytes: &[u8],
    progress: &Progress,
    before_rename: impl FnOnce(),
) -> Result<u64, DrawError> {
    let tmp = temp_path(path);
    let result = write_to(&tmp, bytes, progress).and_then(|n| {
        before_rename();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "planora-drawing-temp-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn helper_files(d: &Path) -> usize {
        fs::read_dir(d)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with("planora-tmp")
            })
            .count()
    }

    // Abbruch nach dem vollständigen Schreiben, direkt vor dem Umbenennen: das Ziel bleibt alt,
    // die Hilfsdatei ist weg.
    #[test]
    fn cancel_before_rename_keeps_target() {
        let d = dir("vor-umbenennen");
        let path = d.join("z.pdf");
        fs::write(&path, "alt").unwrap();
        let p = Progress::new();
        let bytes = vec![b'x'; 3 * CHUNK_BYTES + 7];
        let e = write_via_temp_then(&path, &bytes, &p, || {
            // Die Hilfsdatei ist vollständig geschrieben.
            assert_eq!(
                fs::metadata(temp_path(&path)).unwrap().len(),
                bytes.len() as u64
            );
            p.cancel();
        })
        .unwrap_err();
        assert!(matches!(e, DrawError::Cancelled), "{e:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "alt");
        assert_eq!(helper_files(&d), 0);
        // Ohne Abbruch: das Ziel hat genau die Bytes, in mehreren Stücken geschrieben.
        let p = Progress::new();
        assert_eq!(
            write_via_temp(&path, &bytes, &p).unwrap(),
            bytes.len() as u64
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(p.fraction(), 1.0);
        assert_eq!(helper_files(&d), 0);
        let _ = fs::remove_dir_all(&d);
    }
}
