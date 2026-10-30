//! Schreiben einer Zeichnung in eine Datei (SVG, DXF oder PDF) über eine Hilfsdatei im Zielordner
//! (wie `planora_exchange::export_file` und `planora_step::export_step_file`). SVG, DXF und PDF
//! teilen [`write_via_temp`] (P14).

use crate::error::DrawError;
use crate::pdf::{PdfMeta, pdf_bytes};
use crate::sheet::SheetModel;
use crate::{dxf_string, svg_string};
use planora_exchange::{Progress, temp_path};
use planora_i18n::tr;
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

    /// Name für die Auswahl in der aktuellen Sprache.
    pub fn label(self) -> String {
        match self {
            DrawingFormat::Svg => tr!("export-format-svg"),
            DrawingFormat::Dxf => tr!("export-format-dxf"),
            DrawingFormat::Pdf => tr!("export-format-pdf"),
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
    export_drawing_file_then(path, model, format, progress, &mut |_| {})
}

fn export_drawing_file_then(
    path: &Path,
    model: &SheetModel,
    format: DrawingFormat,
    progress: &Progress,
    hook: Hook<'_>,
) -> Result<u64, DrawError> {
    progress.check()?;
    let bytes = drawing_to_bytes(model, format)?;
    write_via_temp(path, &bytes, progress, hook)
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
    export_pdf_file_then(path, pages, meta, progress, &mut |_| {})
}

fn export_pdf_file_then(
    path: &Path,
    pages: &[&SheetModel],
    meta: &PdfMeta,
    progress: &Progress,
    hook: Hook<'_>,
) -> Result<u64, DrawError> {
    progress.check()?;
    let bytes = pdf_bytes(pages, meta)?;
    write_via_temp(path, &bytes, progress, hook)
}

/// Augenblicke beim Schreiben über die Hilfsdatei, an denen die Tests eingreifen (Ablauf prüfen,
/// abbrechen). Außerhalb der Tests tut der Haken nichts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// So viele Stücke stehen in der Hilfsdatei.
    Written(usize),
    /// Nach `sync_all`, vor der letzten Abbruchprüfung und dem Umbenennen.
    BeforeRename,
}

pub(crate) type Hook<'a> = &'a mut dyn FnMut(Step);

/// Schreibt `bytes` nach `path` (P14): erst in die Hilfsdatei `.<Name>.planora-tmp` im
/// Zielordner, `sync_all`, Abbruch prüfen, dann umbenennen. Bei jedem Fehler wird die Hilfsdatei
/// entfernt und eine vorhandene Zieldatei bleibt unverändert. Liefert die Dateigröße in Bytes.
/// `hook` läuft nach jedem geschriebenen Stück und vor dem Umbenennen (nur die Tests greifen
/// dort ein; sonst `&mut |_| {}`).
pub(crate) fn write_via_temp(
    path: &Path,
    bytes: &[u8],
    progress: &Progress,
    hook: Hook<'_>,
) -> Result<u64, DrawError> {
    let tmp = temp_path(path);
    let result = write_to(&tmp, bytes, progress, hook).and_then(|n| {
        hook(Step::BeforeRename);
        progress.check()?;
        fs::rename(&tmp, path)?;
        Ok(n)
    });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_to(
    tmp: &Path,
    bytes: &[u8],
    progress: &Progress,
    hook: Hook<'_>,
) -> Result<u64, DrawError> {
    let mut file = File::create(tmp)?;
    progress.set_total(bytes.len().div_ceil(CHUNK_BYTES) as u64);
    for (i, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
        progress.check()?;
        file.write_all(chunk)?;
        progress.advance(1);
        hook(Step::Written(i + 1));
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
        let e = write_via_temp(&path, &bytes, &p, &mut |step| {
            if step == Step::BeforeRename {
                // Die Hilfsdatei ist vollständig geschrieben.
                assert_eq!(
                    fs::metadata(temp_path(&path)).unwrap().len(),
                    bytes.len() as u64
                );
                p.cancel();
            }
        })
        .unwrap_err();
        assert!(matches!(e, DrawError::Cancelled), "{e:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "alt");
        assert_eq!(helper_files(&d), 0);
        // Ohne Abbruch: das Ziel hat genau die Bytes, in mehreren Stücken geschrieben.
        let p = Progress::new();
        assert_eq!(
            write_via_temp(&path, &bytes, &p, &mut |_| {}).unwrap(),
            bytes.len() as u64
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(p.fraction(), 1.0);
        assert_eq!(helper_files(&d), 0);
        let _ = fs::remove_dir_all(&d);
    }

    /// Blatt mit so vielen Linien, dass jede Ausgabe mehrere Stücke von `CHUNK_BYTES` braucht.
    fn big_sheet(shift: f64) -> SheetModel {
        use crate::Visibility;
        use crate::model::{Paper, Sheet, ViewId};
        use crate::out2d::Out2d;
        use crate::sheet::{SheetLine, SheetView};
        use crate::stroke::StrokeOrigin;
        use planora_math::DVec2;
        let lines = (0..2500)
            .map(|i| {
                let y = 10.0 + 0.1 * f64::from(i) + shift;
                SheetLine {
                    vis: if i % 3 == 0 {
                        Visibility::Hidden
                    } else {
                        Visibility::Visible
                    },
                    curve: Out2d::Line {
                        a: DVec2::new(10.123_456, y),
                        b: DVec2::new(300.654_321, y + 0.37),
                    },
                    origin: StrokeOrigin::Edge { body: 0, edge: 0 },
                }
            })
            .collect();
        SheetModel {
            sheet: Sheet {
                paper: Paper::A3,
                landscape: true,
            },
            frame: None,
            views: vec![SheetView {
                id: ViewId(1),
                name: "Vorne".into(),
                lines,
                extent_mm: (0.0, 0.0),
                marks: Vec::new(),
            }],
        }
    }

    /// Ein Schreibweg mit Haken.
    type Writer<'a> = &'a dyn Fn(&Path, &Progress, Hook<'_>) -> Result<u64, DrawError>;

    /// Prüft den Weg über die Hilfsdatei: jedes Stück landet erst in der Hilfsdatei, das Ziel
    /// bleibt bis zum Umbenennen alt, danach genau `want`, Fortschritt 1; Abbruch während des
    /// Schreibens oder vor dem Umbenennen lässt das alte Ziel und keine Hilfsdatei zurück.
    fn check_writer(name: &str, write: Writer<'_>, want: &[u8]) {
        let chunks = want.len().div_ceil(CHUNK_BYTES);
        assert!(chunks >= 2, "{name}: {} Bytes", want.len());
        let d = dir(name);
        let path = d.join("ziel");
        let tmp = temp_path(&path);

        fs::write(&path, "alt").unwrap();
        let p = Progress::new();
        let mut steps = Vec::new();
        let n = write(&path, &p, &mut |step| {
            let len = match step {
                Step::Written(k) => (k * CHUNK_BYTES).min(want.len()),
                Step::BeforeRename => want.len(),
            };
            assert!(
                fs::read(&tmp).unwrap() == want[..len],
                "{name}: Hilfsdatei bei {step:?}"
            );
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                "alt",
                "{name}: {step:?}"
            );
            steps.push(step);
        })
        .unwrap();
        let mut expect: Vec<Step> = (1..=chunks).map(Step::Written).collect();
        expect.push(Step::BeforeRename);
        assert_eq!(steps, expect, "{name}");
        assert_eq!(n, want.len() as u64);
        assert!(fs::read(&path).unwrap() == want, "{name}: Inhalt");
        assert_eq!(p.fraction(), 1.0, "{name}");
        assert_eq!(helper_files(&d), 0, "{name}");

        for at in [Step::Written(1), Step::BeforeRename] {
            fs::write(&path, "alt").unwrap();
            let p = Progress::new();
            let mut seen = Vec::new();
            let e = write(&path, &p, &mut |step| {
                seen.push(step);
                if step == at {
                    p.cancel();
                }
            })
            .unwrap_err();
            assert!(matches!(e, DrawError::Cancelled), "{name} {at:?}: {e:?}");
            assert_eq!(seen.last(), Some(&at), "{name}: nach dem Abbruch weiter");
            assert_eq!(fs::read_to_string(&path).unwrap(), "alt", "{name} {at:?}");
            assert_eq!(helper_files(&d), 0, "{name} {at:?}");
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn pdf_and_drawing_files_go_through_the_helper() {
        let (a, b, c) = (big_sheet(0.0), big_sheet(0.05), big_sheet(0.02));
        let pages = [&a, &b, &c];
        let meta = PdfMeta {
            title: "Drei".into(),
            ..PdfMeta::default()
        };
        check_writer(
            "pdf-seiten",
            &|path, p, hook| export_pdf_file_then(path, &pages, &meta, p, hook),
            &pdf_bytes(&pages, &meta).unwrap(),
        );
        for format in [DrawingFormat::Pdf, DrawingFormat::Svg, DrawingFormat::Dxf] {
            check_writer(
                &format!("zeichnung-{}", format.extension()),
                &|path, p, hook| export_drawing_file_then(path, &a, format, p, hook),
                &drawing_to_bytes(&a, format).unwrap(),
            );
        }
    }
}
