//! Schreiben in eine Datei über eine Hilfsdatei im Zielordner (Spec §4, wie
//! `planora_exchange::export_file`).

use crate::error::StepError;
use crate::{StepBody, StepOptions, StepReport, write_step};
use planora_exchange::{Progress, temp_path};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

/// Schreibt die Körper als STEP-Datei nach `path`: erst in die Hilfsdatei `.<Name>.planora-tmp`,
/// `sync_all`, dann umbenennen. Bei jedem Fehler (auch Abbrechen und `NothingToExport`) bleibt
/// eine vorhandene Zieldatei unverändert und die Hilfsdatei wird entfernt. Liefert Bericht und
/// Dateigröße in Bytes.
pub fn export_step_file(
    path: &Path,
    bodies: &[StepBody],
    opts: &StepOptions,
    progress: &Progress,
) -> Result<(StepReport, u64), StepError> {
    let tmp = temp_path(path);
    let result = write_to(&tmp, bodies, opts, progress).and_then(|done| {
        fs::rename(&tmp, path)?;
        Ok(done)
    });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_to(
    tmp: &Path,
    bodies: &[StepBody],
    opts: &StepOptions,
    progress: &Progress,
) -> Result<(StepReport, u64), StepError> {
    let mut w = BufWriter::new(File::create(tmp)?);
    let report = write_step(bodies, opts, &mut w, progress)?;
    let file = w
        .into_inner()
        .map_err(|e| StepError::from(e.into_error()))?;
    file.sync_all()?;
    Ok((report, file.metadata()?.len()))
}
