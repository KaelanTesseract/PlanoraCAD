//! STEP-Export und -Import: exakte Körper des Kernels als STEP-Datei (AP214,
//! `AUTOMOTIVE_DESIGN`, Teilprojekt 10) und ein eigener Leser für STEP-Dateien (`read`).
//! Das Textformat ist von Hand geschrieben und gelesen, ohne neue Abhängigkeit.

pub mod entity;
pub mod error;
mod file;
mod geometry;
pub mod number;
mod product;
pub mod read;
pub mod text;
mod topology;

pub use entity::{Entities, MAX_ENTITIES, WRAP_COLUMN, logical, refs};
pub use error::StepError;
pub use file::export_step_file;
pub use geometry::FRAME_TOL;
pub use number::{push_real, real};
pub use product::{SCHEMA, SYSTEM, utc_timestamp};
pub use read::error::{
    MAX_STEP_BODIES, MAX_STEP_ENTITIES, MAX_STEP_FACES, MAX_STEP_FILE_BYTES, ReadLimits,
    StepReadError,
};
pub use read::import::{
    ImportOptions, ImportedBody, MAX_STEP_TEXT_BYTES, NORMAL_FILE_NAME, NormalBody, StepImport,
    UNNAMED_BODY, check_normal_header, import_bodies, normalize, read_normal_body,
};
pub use read::index::{StepFile, StepHeader, read_step_file};
pub use read::units::{COARSE_TOL_MM, DEFAULT_FILE_TOL_MM, LengthUnit};
pub use text::{MAX_NAME_CHARS, StepText, step_string};

use planora_exchange::Progress;
use planora_kernel::{Curve, NURBS_EDGE_TOL, Solid, Surface, validate_with};
use planora_math::Tolerance;
use std::io::Write;
use std::time::SystemTime;

/// Ein Körper für die Ausgabe: Name aus dem Feature-Baum und der exakte Körper des Kernels.
#[derive(Clone, Debug)]
pub struct StepBody<'a> {
    pub name: String,
    pub solid: &'a Solid,
}

/// Einstellungen der Ausgabe.
#[derive(Clone, Debug, PartialEq)]
pub struct StepOptions {
    /// Dateiname für den Kopf (`FILE_NAME`), ohne Ordner.
    pub file_name: String,
    /// Zeitpunkt für den Kopf.
    pub time: SystemTime,
    /// Toleranz des Kernels; daraus folgt die Unsicherheit im geometrischen Kontext.
    pub tolerance: Tolerance,
}

impl StepOptions {
    /// Jetziger Zeitpunkt, Toleranz des Kernels.
    pub fn new(file_name: &str) -> StepOptions {
        StepOptions {
            file_name: file_name.to_string(),
            time: SystemTime::now(),
            tolerance: Tolerance::default(),
        }
    }
}

/// Bericht über eine geschriebene Datei (Spec §2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepReport {
    /// Zahl der geschriebenen Körper (Teile).
    pub bodies: usize,
    pub faces: usize,
    pub edges: usize,
    /// Zahl der Einträge `#n` in der Datei.
    pub entities: u64,
    /// Abgelehnte Körper, je ein [`StepError::InvalidBody`].
    pub rejected: Vec<StepError>,
    /// Namen, die auf [`MAX_NAME_CHARS`] Zeichen gekürzt wurden.
    pub shortened_names: Vec<String>,
}

/// Kennzahlen eines geprüften Körpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyStats {
    pub faces: usize,
    pub edges: usize,
    /// Zahl der Einträge, die der Körper ohne Teil-Einträge braucht.
    pub entities: u64,
}

/// Anzeigename: leere Namen werden „Körper n“ (n ab 1).
pub fn display_name(name: &str, index: usize) -> String {
    if name.trim().is_empty() {
        format!("Körper {}", index + 1)
    } else {
        name.to_string()
    }
}

/// Prüft einen Körper (Spec §4): Verweise, `validate` (mit der Toleranz aus `opts`) und ein
/// Probelauf des Schreibens ohne Ausgabe. Jede Ablehnung ist ein [`StepError::InvalidBody`] mit
/// dem Namen des Körpers; `Cancelled` und `TooManyEntities` werden durchgereicht.
pub fn check_body(
    body: &StepBody,
    opts: &StepOptions,
    progress: &Progress,
) -> Result<BodyStats, StepError> {
    let named = |reason: String| StepError::InvalidBody {
        name: body.name.clone(),
        reason,
    };
    if !topology::ids_in_range(body.solid) {
        return Err(named(
            "Der Körper verweist auf Teile, die es nicht gibt".into(),
        ));
    }
    validate_with(body.solid, &opts.tolerance).map_err(|e| named(e.to_string()))?;
    let mut sink = std::io::sink();
    let mut e = Entities::probe(&mut sink, progress);
    match topology::write_solid(&mut e, "''", body.solid) {
        Ok(w) => Ok(BodyStats {
            faces: w.faces,
            edges: w.edges,
            entities: e.count(),
        }),
        Err(StepError::InvalidBody { reason, .. }) => Err(named(reason)),
        Err(StepError::NotFinite) => Err(named(StepError::NotFinite.to_string())),
        Err(other) => Err(other),
    }
}

/// Hat der Körper Freiformkurven oder -flächen?
fn has_freeform(solid: &Solid) -> bool {
    solid
        .edges
        .iter()
        .any(|(_, e)| matches!(e.curve, Curve::Nurbs(_)))
        || solid
            .faces
            .iter()
            .any(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
}

/// Unsicherheit für den geometrischen Kontext: die Längentoleranz des Kernels, bei Körpern mit
/// Freiformkurven oder -flächen mindestens [`NURBS_EDGE_TOL`] (so weit dürfen dort Kanten von
/// ihren Flächen abweichen).
fn uncertainty(solids: &[&Solid], tol: &Tolerance) -> f64 {
    if solids.iter().any(|s| has_freeform(s)) {
        tol.linear.max(NURBS_EDGE_TOL)
    } else {
        tol.linear
    }
}

/// Schreibt die Körper als STEP-Datei nach `out` (Spec §2): eine Datei, je Körper ein Teil mit
/// dem Namen aus dem Feature-Baum. Körper, die die Prüfung nicht bestehen, werden ausgelassen
/// und stehen im Bericht; bleibt keiner übrig, ist das `NothingToExport` (dann wurde nichts
/// geschrieben). `progress` zählt die Einträge und bricht ab, wenn gewünscht.
pub fn write_step(
    bodies: &[StepBody],
    opts: &StepOptions,
    out: &mut dyn Write,
    progress: &Progress,
) -> Result<StepReport, StepError> {
    let mut report = StepReport::default();
    let mut accepted: Vec<(String, &Solid, BodyStats)> = Vec::new();
    for (i, b) in bodies.iter().enumerate() {
        if progress.is_cancelled() {
            return Err(StepError::Cancelled);
        }
        let name = display_name(&b.name, i);
        let checked = StepBody {
            name: name.clone(),
            solid: b.solid,
        };
        match check_body(&checked, opts, progress) {
            Ok(stats) => accepted.push((name, b.solid, stats)),
            Err(e @ StepError::InvalidBody { .. }) => report.rejected.push(e),
            Err(e) => return Err(e),
        }
    }
    if accepted.is_empty() {
        return Err(StepError::NothingToExport);
    }
    let fixed = product::CONTEXT_ENTITIES + product::PART_ENTITIES * accepted.len() as u64;
    progress.set_total(fixed + accepted.iter().map(|a| a.2.entities).sum::<u64>());
    let solids: Vec<&Solid> = accepted.iter().map(|a| a.1).collect();
    let mut e = Entities::new(out, progress);
    product::header(&mut e, &opts.file_name, opts.time)?;
    let ctx = product::context(&mut e, uncertainty(&solids, &opts.tolerance))?;
    for (name, solid, _) in &accepted {
        let text = step_string(name);
        if text.shortened {
            report.shortened_names.push(name.clone());
        }
        let w = topology::write_solid(&mut e, &text.text, solid)?;
        product::product(&mut e, &ctx, &text.text, &w.items)?;
        report.bodies += 1;
        report.faces += w.faces;
        report.edges += w.edges;
    }
    product::trailer(&mut e)?;
    report.entities = e.count();
    Ok(report)
}
