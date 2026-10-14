//! Fenster „Exportieren“ (Spec §8 des Netz-Austauschs, Spec §6 des STEP-Exports): Format,
//! Qualität und Umfang als gleich hohe Karten über die ganze Breite; das Netz bzw. die Prüfung
//! der Körper entsteht im Arbeitsthread (Fortschritt, Abbrechen); danach Zahl der Dreiecke,
//! geschätzte Größe und Prüfbericht (Netz) bzw. Zahl der Körper und Flächen (STEP); gespeichert
//! wird über die eigene Dateiauswahl (Plan P14) und eine Hilfsdatei.

use crate::browser::ERROR_COLOR;
use crate::cards::card_row;
use crate::file_chooser::{ChooserAction, FileChooser, default_dir};
use crate::jobs::{Job, Poll};
use crate::mesh_body::{WARNING_COLOR, report_text};
use crate::state::{AppState, BodyKind};
use planora_exchange::{
    ExchangeError, ExchangeMesh, ExportBody, Format, MeshReport, MeshTransform, PlacedSource,
    Progress, SolidSource, check_mesh, collect_body, export_file,
};
use planora_feature::BodyRef;
use planora_kernel::Solid;
use planora_mesh::{TessellationOptions, tessellate};
use planora_step::{
    MAX_NAME_CHARS, StepBody, StepError, StepOptions, check_body, display_name, export_step_file,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Abstand des Fensters zum linken und rechten Rand; sonst reicht es über die ganze Breite.
pub const DIALOG_MARGIN: f32 = 16.0;
/// Abstand des Fensters zum oberen Rand (unter Menü und Tab-Leiste).
pub const DIALOG_TOP: f32 = 60.0;
/// Kleinste Breite bei sehr schmalen Fenstern.
pub(crate) const MIN_WIDTH: f32 = 320.0;
/// Breite des Fortschrittsbalkens.
pub(crate) const PROGRESS_WIDTH: f32 = 280.0;
/// So oft wird neu gezeichnet, solange ein Arbeitsthread läuft.
pub(crate) const REPAINT: Duration = Duration::from_millis(100);
/// Bytes je Megabyte (dezimal, wie die Dateiverwaltung).
const BYTES_PER_MB: f64 = 1e6;
/// Darunter zeigt das Fenster „unter 0,1 MB“.
const MIN_SHOWN_MB: f64 = 0.1;
/// Meldung, wenn ein Arbeitsthread ohne Ergebnis endet.
pub const LOST: &str = "Die Berechnung wurde unerwartet beendet.";
/// Warnung bei nicht geschlossenem Netz (Spec E6).
pub const NOT_CLOSED_WARNING: &str = "Die Datei wird trotzdem geschrieben. Manche Programme für den 3D-Druck melden solche Netze als fehlerhaft.";
/// Eintrag des fünften Formats (STEP-Spec §6).
pub const STEP_LABEL: &str = "STEP – für andere CAD-Programme (exakt)";
/// Steht bei STEP an der Stelle der Karte „Qualität“ (STEP-Spec §6).
pub const STEP_EXACT: &str = "Rundungen und Bohrungen bleiben exakt.";
/// Hinweis, wenn das Fenster beim Schreiben einer STEP-Datei nicht geschlossen werden darf.
pub const WRITING_HINT: &str = "Das Schreiben läuft noch. Bitte warten oder erst abbrechen.";
/// So viele Zeichen eines Körpernamens zeigt das Fenster in Meldungen und Listen höchstens.
const MAX_SHOWN_NAME: usize = 40;
/// Meldung, wenn bei STEP kein Körper übrig bleibt.
pub const STEP_NOTHING: &str = "Es gibt keinen Körper, der sich als STEP-Datei ausgeben lässt.";

/// Fenster über die ganze Breite (Rand DIALOG_MARGIN), unter Menü und Tab-Leiste.
pub(crate) fn wide_window(title: &'static str, ctx: &egui::Context) -> egui::Window<'static> {
    let width = (ctx.content_rect().width() - 2.0 * DIALOG_MARGIN).max(MIN_WIDTH);
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, DIALOG_TOP])
        .min_width(width)
        .max_width(width)
}

/// Sehnentoleranz „Grob“ in Millimetern (Spec E5).
const CHORD_COARSE: f64 = 0.1;
/// Sehnentoleranz „Normal“ in Millimetern (Spec E5).
const CHORD_NORMAL: f64 = 0.01;
/// Sehnentoleranz „Fein“ in Millimetern (Spec E5).
const CHORD_FINE: f64 = 0.001;
/// Meldung, wenn sich Auswahl, Qualität oder Modell seit der Berechnung geändert haben.
pub const STALE: &str = "Die Auswahl hat sich geändert, bitte noch einmal speichern.";
/// Meldung, wenn es noch kein fertiges Netz gibt.
pub const NOT_COMPUTED: &str = "Es ist noch nichts berechnet. Bitte warten oder neu berechnen.";

/// Qualität des Netzes (Spec E5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quality {
    Coarse,
    Normal,
    Fine,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Coarse, Quality::Normal, Quality::Fine];

    /// Sehnentoleranz in Millimetern.
    pub fn chord_tolerance(self) -> f64 {
        match self {
            Quality::Coarse => CHORD_COARSE,
            Quality::Normal => CHORD_NORMAL,
            Quality::Fine => CHORD_FINE,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Quality::Coarse => "Grob (bis 0,1 mm Abweichung)",
            Quality::Normal => "Normal (bis 0,01 mm)",
            Quality::Fine => "Fein (bis 0,001 mm)",
        }
    }
}

/// Umfang (Spec E4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Selected,
    Visible,
}

/// Format im Fenster: eines der vier Netzformate oder STEP (exakte Körper).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    Mesh(Format),
    Step,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 5] = [
        ExportFormat::Mesh(Format::Stl),
        ExportFormat::Mesh(Format::Obj),
        ExportFormat::Mesh(Format::ThreeMf),
        ExportFormat::Mesh(Format::Glb),
        ExportFormat::Step,
    ];

    /// Dateiendung ohne Punkt.
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Mesh(f) => f.extension(),
            ExportFormat::Step => "step",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Mesh(f) => format_label(f),
            ExportFormat::Step => STEP_LABEL,
        }
    }

    pub fn is_step(self) -> bool {
        self == ExportFormat::Step
    }
}

/// Ein Körper für den Arbeitsthread (Kopie, damit das Dokument frei bleibt).
#[derive(Clone, Debug)]
pub enum ExportSource {
    Solid(Solid),
    Mesh(Arc<ExchangeMesh>, MeshTransform),
}

#[derive(Clone, Debug)]
pub struct ExportItem {
    pub name: String,
    pub source: ExportSource,
}

/// Fertig vernetzte Körper mit gemeinsamem Bericht.
#[derive(Debug)]
pub struct Prepared {
    pub bodies: Vec<ExportBody>,
    pub report: MeshReport,
}

impl Prepared {
    pub fn triangles(&self) -> u64 {
        self.bodies
            .iter()
            .map(|b| b.mesh.triangle_count() as u64)
            .sum()
    }

    pub fn vertices(&self) -> u64 {
        self.bodies
            .iter()
            .map(|b| b.mesh.positions.len() as u64)
            .sum()
    }
}

/// Für STEP geprüfte Körper (STEP-Spec §4, §6, E3).
#[derive(Debug)]
pub struct StepPrepared {
    /// Körper, die die Prüfung bestanden haben: Name und exakter Körper.
    pub bodies: Vec<(String, Solid)>,
    /// Flächen aller geprüften Körper.
    pub faces: usize,
    /// Abgelehnte Körper mit Grund („„Name“ wird nicht ausgegeben: …“).
    pub rejected: Vec<String>,
    /// Netz-Körper, die STEP nicht enthält (E3).
    pub skipped_meshes: Vec<String>,
    /// Körper, deren Name in der Datei auf `MAX_NAME_CHARS` Zeichen gekürzt wird (kurz gezeigt).
    pub shortened: Vec<String>,
}

/// Was das Fenster zum Ergebnis zeigt.
#[derive(Debug, PartialEq)]
pub enum ExportStatus {
    Computing(f32),
    Ready {
        triangles: u64,
        bytes: u64,
        report: MeshReport,
    },
    /// STEP: Zahl der Körper und Flächen, abgelehnte Körper, ausgelassene Netz-Körper.
    StepReady {
        bodies: usize,
        faces: usize,
        rejected: Vec<String>,
        skipped_meshes: Vec<String>,
    },
    Failed(String),
}

/// Stand des Schreibens der Datei.
#[derive(Debug, PartialEq)]
pub enum WriteState {
    Idle,
    Writing,
    Done(PathBuf, u64),
    Failed(String),
}

/// Vernetzt die Körper (läuft im Arbeitsthread); bricht zwischen den Körpern ab, wenn
/// gewünscht (Plan P15).
pub fn prepare(
    items: &[ExportItem],
    quality: Quality,
    progress: &Progress,
) -> Result<Prepared, String> {
    progress.set_total(items.len() as u64);
    let mut bodies = Vec::with_capacity(items.len());
    for item in items {
        progress.check().map_err(|e| e.to_string())?;
        let body = match &item.source {
            ExportSource::Solid(s) => {
                let options = TessellationOptions {
                    chord_tolerance: quality.chord_tolerance(),
                };
                let mesh = tessellate(s, &options)
                    .map_err(|e| format!("„{}“ lässt sich nicht vernetzen: {e}", item.name))?;
                collect_body(&SolidSource {
                    name: &item.name,
                    mesh: &mesh,
                })
            }
            ExportSource::Mesh(m, t) => collect_body(&PlacedSource {
                name: &item.name,
                mesh: m,
                transform: *t,
            }),
        }
        .map_err(|e| format!("„{}“: {e}", item.name))?;
        bodies.push(body);
        progress.advance(1);
    }
    let reports: Vec<MeshReport> = bodies.iter().map(|b| check_mesh(&b.mesh)).collect();
    Ok(Prepared {
        report: MeshReport::combine(&reports),
        bodies,
    })
}

/// Prüft die Körper für STEP (läuft im Arbeitsthread): Netz-Körper werden ausgelassen und
/// genannt, jeder exakte Körper durchläuft `check_body`; abgelehnte stehen mit Grund in der
/// Liste. Bricht zwischen den Körpern und in großen Körpern ab, wenn gewünscht.
pub fn prepare_step(items: &[ExportItem], progress: &Progress) -> Result<StepPrepared, String> {
    progress.set_total(items.len() as u64);
    let mut out = StepPrepared {
        bodies: Vec::new(),
        faces: 0,
        rejected: Vec::new(),
        skipped_meshes: Vec::new(),
        shortened: Vec::new(),
    };
    for (index, item) in items.iter().enumerate() {
        progress.check().map_err(|e| e.to_string())?;
        // Leere Namen werden „Körper n“, damit Meldungen nie „„“ lauten; lange Namen erscheinen
        // in Meldungen und Listen gekürzt.
        let name = display_name(&item.name, index);
        let shown = shorten_for_display(&name);
        match &item.source {
            ExportSource::Mesh(..) => out.skipped_meshes.push(shown),
            ExportSource::Solid(s) => {
                if name.chars().count() > MAX_NAME_CHARS {
                    out.shortened.push(shown.clone());
                }
                let body = StepBody {
                    name: shown,
                    solid: s,
                };
                match check_body(&body, &StepOptions::new("x.step"), progress) {
                    Ok(stats) => {
                        out.faces += stats.faces;
                        out.bodies.push((name, s.clone()));
                    }
                    Err(e @ StepError::InvalidBody { .. }) => out.rejected.push(e.to_string()),
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
        progress.advance(1);
    }
    Ok(out)
}

/// Kürzt einen Namen für Meldungen auf `MAX_SHOWN_NAME` Zeichen (mit „…“).
pub fn shorten_for_display(name: &str) -> String {
    if name.chars().count() > MAX_SHOWN_NAME {
        let head: String = name.chars().take(MAX_SHOWN_NAME).collect();
        format!("{head}…")
    } else {
        name.to_string()
    }
}

/// Hinweis auf Namen, die in der Datei gekürzt wurden.
pub fn shortened_text(names: &[String]) -> String {
    let list: Vec<String> = names.iter().map(|n| format!("„{n}“")).collect();
    format!(
        "Name gekürzt (höchstens {MAX_NAME_CHARS} Zeichen in der Datei): {}",
        list.join(", ")
    )
}

/// Endung `.stp` wird bei STEP zu `.step` (statt `x.stp.step`).
pub fn normalize_target(format: ExportFormat, path: PathBuf) -> PathBuf {
    let is_stp = path
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("stp"));
    if format.is_step() && is_stp {
        path.with_extension(ExportFormat::Step.extension())
    } else {
        path
    }
}

/// Hinweis am gesperrten Knopf „Exportieren …“: bei STEP ohne Körper der Grund, sonst Warten.
pub fn disabled_hint(status: &ExportStatus) -> &'static str {
    match status {
        ExportStatus::StepReady { bodies: 0, .. } => STEP_NOTHING,
        _ => "Erst möglich, wenn die Berechnung fertig ist.",
    }
}

/// Schreibt geprüfte Körper als STEP-Datei nach `path` (Hilfsdatei, Fortschritt, Abbrechen);
/// liefert die Dateigröße.
pub fn write_step_bodies(
    path: &Path,
    prepared: &StepPrepared,
    progress: &Progress,
) -> Result<u64, StepError> {
    let bodies: Vec<StepBody> = prepared
        .bodies
        .iter()
        .map(|(name, solid)| StepBody {
            name: name.clone(),
            solid,
        })
        .collect();
    let opts = StepOptions::new(&file_name(path));
    export_step_file(path, &bodies, &opts, progress).map(|(_, bytes)| bytes)
}

/// Kurzbeschreibung eines Netzformats für die Auswahl.
pub fn format_label(f: Format) -> &'static str {
    match f {
        Format::Stl => "STL – für den 3D-Druck",
        Format::Obj => "OBJ – für viele 3D-Programme",
        Format::ThreeMf => "3MF – für den 3D-Druck, mit Namen",
        Format::Glb => "glTF (.glb) – zum Ansehen und Zeigen",
    }
}

/// Hinweis auf ausgelassene Netz-Körper (STEP-Spec E3).
pub fn skipped_text(names: &[String]) -> String {
    let list: Vec<String> = names.iter().map(|n| format!("„{n}“")).collect();
    format!(
        "Netz-Körper werden als STEP nicht ausgegeben: {}. Für sie bitte STL, OBJ, 3MF oder glTF wählen.",
        list.join(", ")
    )
}

/// Zahl mit Leerzeichen als Tausendertrenner („12 345“).
pub fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Größe in Megabyte mit Komma; unter 0,1 MB „unter 0,1 MB“.
pub fn mb_text(bytes: u64) -> String {
    let mb = bytes as f64 / BYTES_PER_MB;
    if mb < MIN_SHOWN_MB {
        "unter 0,1 MB".into()
    } else {
        format!("{mb:.1} MB").replace('.', ",")
    }
}

/// Geschätzte Größe: „ca. 1,2 MB“ (sehr kleine Dateien „unter 0,1 MB“).
pub fn estimate_text(bytes: u64) -> String {
    let t = mb_text(bytes);
    if t.starts_with("unter") {
        t
    } else {
        format!("ca. {t}")
    }
}

/// Dateiname ohne Ordner.
pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Stand, zu dem ein Ergebnis gehört: Qualität (`None` bei STEP), Umfang, Änderungszahl des
/// Dokuments, gewählter Körper.
type Key = (Option<Quality>, Scope, u64, Option<BodyRef>);
type PrepJob = Job<Result<Prepared, String>>;
type PrepResult = Result<Arc<Prepared>, String>;
type StepJob = Job<Result<StepPrepared, String>>;
type StepResult = Result<Arc<StepPrepared>, String>;
type WriteJob = Job<Result<u64, ExchangeError>>;
type StepWriteJob = Job<Result<u64, StepError>>;

pub struct ExportDialog {
    pub format: ExportFormat,
    pub quality: Quality,
    pub scope: Scope,
    prepared: Option<(Key, PrepResult)>,
    job: Option<(Key, PrepJob)>,
    step_prepared: Option<(Key, StepResult)>,
    step_job: Option<(Key, StepJob)>,
    chooser: Option<FileChooser>,
    /// Schlüssel und Format zur Zeit des Öffnens der Dateiauswahl; ändern sie sich, schließt sie.
    chooser_key: Option<Key>,
    writing: Option<(PathBuf, WriteJob)>,
    step_writing: Option<(PathBuf, StepWriteJob)>,
    written: Option<Result<(PathBuf, u64), String>>,
    /// Format, mit dem zuletzt geschrieben wurde; die Meldung gehört nur zu diesem Format.
    write_format: ExportFormat,
    written_format: Option<ExportFormat>,
    /// Schließen wurde abgewiesen, weil eine STEP-Datei geschrieben wird (zeigt den Hinweis).
    close_denied: bool,
    /// Rechteck des Fensters im letzten Bild (Tests der vollen Breite).
    pub last_rect: Option<egui::Rect>,
    /// Rechtecke der Karten im letzten Bild (Tests der gleichen Höhe).
    pub last_cards: Vec<egui::Rect>,
}

impl ExportDialog {
    /// Voreinstellung: STL, Normal; Umfang „gewählter Körper“, wenn einer gewählt ist.
    pub fn new(state: &AppState) -> ExportDialog {
        let selected = state.selected_body.is_some_and(|i| i < state.bodies.len());
        ExportDialog {
            format: ExportFormat::Mesh(Format::Stl),
            quality: Quality::Normal,
            scope: if selected {
                Scope::Selected
            } else {
                Scope::Visible
            },
            prepared: None,
            job: None,
            step_prepared: None,
            step_job: None,
            chooser: None,
            chooser_key: None,
            writing: None,
            step_writing: None,
            written: None,
            write_format: ExportFormat::Mesh(Format::Stl),
            written_format: None,
            close_denied: false,
            last_rect: None,
            last_cards: Vec::new(),
        }
    }

    fn has_selection(state: &AppState) -> bool {
        state.selected_body.is_some_and(|i| i < state.bodies.len())
    }

    /// Indizes der Körper im Umfang (Spec E4: der gewählte, sonst alle sichtbaren).
    fn chosen(&self, state: &AppState) -> Vec<usize> {
        match (self.scope, state.selected_body) {
            (Scope::Selected, Some(i)) if i < state.bodies.len() => vec![i],
            _ => (0..state.bodies.len()).collect(),
        }
    }

    /// Kopien der Körper im Umfang für den Arbeitsthread.
    pub fn items(&self, state: &AppState) -> Vec<ExportItem> {
        self.chosen(state)
            .into_iter()
            .filter_map(|i| {
                let b = &state.bodies[i];
                let source = if b.kind == BodyKind::Mesh {
                    let (mesh, t) = state.document.mesh_body(b.source.feature)?;
                    ExportSource::Mesh(Arc::clone(mesh), t)
                } else {
                    ExportSource::Solid(b.solid.clone())
                };
                Some(ExportItem {
                    name: b.name.clone(),
                    source,
                })
            })
            .collect()
    }

    fn key(&self, state: &AppState) -> Key {
        let selected = match self.scope {
            Scope::Selected => state
                .selected_body
                .and_then(|i| state.bodies.get(i))
                .map(|b| b.source),
            Scope::Visible => None,
        };
        let quality = (!self.format.is_step()).then_some(self.quality);
        (quality, self.scope, state.document.edit_count(), selected)
    }

    /// Startet das Vernetzen bzw. die STEP-Prüfung, wenn sich Format, Qualität, Umfang oder
    /// Dokument geändert haben, und holt fertige Ergebnisse ab. Mit `block` wird gewartet
    /// (Tests).
    pub fn tick(&mut self, state: &AppState, block: bool) {
        // Ohne Auswahl gibt es „Gewählter Körper“ nicht mehr (Anzeige und Export stimmen überein).
        if self.scope == Scope::Selected && !Self::has_selection(state) {
            self.scope = Scope::Visible;
        }
        let key = self.key(state);
        if self.format.is_step() {
            let fresh = self.step_prepared.as_ref().is_some_and(|(k, _)| *k == key);
            let running = self.step_job.as_ref().is_some_and(|(k, _)| *k == key);
            if !fresh && !running {
                let items = self.items(state);
                self.step_job = Some((key, Job::spawn(move |p| prepare_step(&items, p))));
            }
            let finished = match &self.step_job {
                Some((k, job)) => match job.fetch(block) {
                    Poll::Pending => None,
                    Poll::Ready(r) => Some((*k, r.map(Arc::new))),
                    Poll::Lost => Some((*k, Err(LOST.to_string()))),
                },
                None => None,
            };
            if let Some(done) = finished {
                self.step_prepared = Some(done);
                self.step_job = None;
            }
        } else {
            let fresh = self.prepared.as_ref().is_some_and(|(k, _)| *k == key);
            let running = self.job.as_ref().is_some_and(|(k, _)| *k == key);
            if !fresh && !running {
                let items = self.items(state);
                let quality = self.quality;
                // Ein alter Auftrag wird dabei fallengelassen und bricht ab.
                self.job = Some((key, Job::spawn(move |p| prepare(&items, quality, p))));
            }
            let finished = match &self.job {
                Some((k, job)) => match job.fetch(block) {
                    Poll::Pending => None,
                    Poll::Ready(r) => Some((*k, r.map(Arc::new))),
                    Poll::Lost => Some((*k, Err(LOST.to_string()))),
                },
                None => None,
            };
            if let Some(done) = finished {
                self.prepared = Some(done);
                self.job = None;
            }
        }
        let wrote = match &self.writing {
            Some((path, job)) => match job.fetch(block) {
                Poll::Pending => None,
                Poll::Ready(r) => Some(r.map(|n| (path.clone(), n)).map_err(|e| e.to_string())),
                Poll::Lost => Some(Err(LOST.to_string())),
            },
            None => None,
        };
        if let Some(w) = wrote {
            self.set_written(w);
            self.writing = None;
        }
        let wrote = match &self.step_writing {
            Some((path, job)) => match job.fetch(block) {
                Poll::Pending => None,
                Poll::Ready(r) => Some(r.map(|n| (path.clone(), n)).map_err(|e| e.to_string())),
                Poll::Lost => Some(Err(LOST.to_string())),
            },
            None => None,
        };
        if let Some(w) = wrote {
            self.set_written(w);
            self.step_writing = None;
        }
        if self.step_writing.is_none() {
            self.close_denied = false;
        }
    }

    /// Läuft gerade das Schreiben einer STEP-Datei? Dann darf das Fenster nicht verworfen
    /// werden: Der Arbeitsthread bräche ab und die alte Datei bliebe ohne Meldung liegen.
    pub fn is_writing_step(&self) -> bool {
        self.step_writing.is_some()
    }

    /// Fragt, ob das Fenster geschlossen werden darf; beim Schreiben einer STEP-Datei nicht
    /// (dann steht im Fenster der Hinweis `WRITING_HINT`).
    pub fn request_close(&mut self) -> bool {
        if self.is_writing_step() {
            self.close_denied = true;
            false
        } else {
            true
        }
    }

    pub fn status(&self, state: &AppState) -> ExportStatus {
        let key = self.key(state);
        if self.format.is_step() {
            if let Some((k, job)) = &self.step_job
                && *k == key
            {
                return ExportStatus::Computing(job.fraction());
            }
            return match &self.step_prepared {
                Some((k, Ok(p))) if *k == key => ExportStatus::StepReady {
                    bodies: p.bodies.len(),
                    faces: p.faces,
                    rejected: p.rejected.clone(),
                    skipped_meshes: p.skipped_meshes.clone(),
                },
                Some((k, Err(m))) if *k == key => ExportStatus::Failed(m.clone()),
                _ => ExportStatus::Computing(0.0),
            };
        }
        if let Some((k, job)) = &self.job
            && *k == key
        {
            return ExportStatus::Computing(job.fraction());
        }
        match (&self.prepared, self.format) {
            (Some((k, Ok(p))), ExportFormat::Mesh(f)) if *k == key => ExportStatus::Ready {
                triangles: p.triangles(),
                bytes: f.estimate_bytes(p.triangles(), p.vertices()),
                report: p.report,
            },
            (Some((k, Err(m))), _) if *k == key => ExportStatus::Failed(m.clone()),
            _ => ExportStatus::Computing(0.0),
        }
    }

    /// Merkt sich das Ergebnis des Schreibens samt Format, mit dem geschrieben wurde.
    fn set_written(&mut self, w: Result<(PathBuf, u64), String>) {
        self.written = Some(w);
        self.written_format = Some(self.write_format);
    }

    /// Namen, die in der geprüften STEP-Datei gekürzt werden (leer ohne gültiges Ergebnis).
    pub fn shortened_names(&self, state: &AppState) -> Vec<String> {
        match &self.step_prepared {
            Some((k, Ok(p))) if *k == self.key(state) => p.shortened.clone(),
            _ => Vec::new(),
        }
    }

    pub fn write_state(&self) -> WriteState {
        if self.writing.is_some() || self.step_writing.is_some() {
            return WriteState::Writing;
        }
        if self.written_format != Some(self.format) {
            return WriteState::Idle;
        }
        match &self.written {
            None => WriteState::Idle,
            Some(Ok((p, n))) => WriteState::Done(p.clone(), *n),
            Some(Err(m)) => WriteState::Failed(m.clone()),
        }
    }

    /// Bricht das Vernetzen bzw. die Prüfung ab; das Fenster zeigt „Abgebrochen“, bis neu
    /// berechnet wird.
    pub fn cancel(&mut self) {
        // Nur der Auftrag des gewählten Formats; der andere läuft im Hintergrund weiter.
        if self.format.is_step() {
            if let Some((key, _)) = self.step_job.take() {
                self.step_prepared = Some((key, Err(StepError::Cancelled.to_string())));
            }
        } else if let Some((key, _)) = self.job.take() {
            self.prepared = Some((key, Err(ExchangeError::Cancelled.to_string())));
        }
    }

    /// Erledigter Anteil beim Schreiben einer STEP-Datei (Netzformate: 0).
    pub fn write_progress(&self) -> f32 {
        self.step_writing
            .as_ref()
            .map_or(0.0, |(_, job)| job.fraction())
    }

    /// Bricht das Schreiben einer STEP-Datei ab; die Zieldatei bleibt unverändert.
    pub fn cancel_write(&mut self) {
        if let Some((_, job)) = &self.step_writing {
            job.cancel();
        }
    }

    /// Verwirft das Ergebnis des gewählten Formats; das nächste `tick` rechnet neu. Das Ergebnis
    /// des anderen Formats bleibt (Wechsel zurück rechnet nicht neu).
    pub fn recompute(&mut self) {
        if self.format.is_step() {
            self.step_prepared = None;
        } else {
            self.prepared = None;
        }
    }

    /// Öffnet die Dateiauswahl im Ordner `dir` mit dem Namen des Körpers (bei mehreren
    /// „Körper“); Zeichen, die in Dateinamen nicht erlaubt sind, werden „_“.
    pub fn begin_save(&mut self, dir: PathBuf, state: &AppState) {
        let names: Vec<&str> = self
            .chosen(state)
            .into_iter()
            .filter_map(|i| state.bodies.get(i).map(|b| b.name.as_str()))
            .collect();
        let stem = match names.as_slice() {
            [one] => (*one).to_string(),
            _ => "Körper".to_string(),
        };
        let stem: String = stem
            .chars()
            .map(|c| {
                if c.is_control() || "\\/:*?\"<>|".contains(c) {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        self.chooser_key = Some(self.key(state));
        self.chooser = Some(FileChooser::save(dir, stem, self.format.extension()));
    }

    pub fn chooser(&self) -> Option<&FileChooser> {
        self.chooser.as_ref()
    }

    /// Schreibt das fertige Ergebnis im Arbeitsthread nach `path` (über eine Hilfsdatei).
    /// Schreibt nur, wenn das Ergebnis noch zum Stand von `state` passt; sonst gibt es eine
    /// Meldung.
    pub fn write_to(&mut self, path: PathBuf, state: &AppState) {
        let key = self.key(state);
        self.write_format = self.format;
        let path = normalize_target(self.format, path);
        match self.format {
            ExportFormat::Step => {
                let Some((k, Ok(prepared))) = &self.step_prepared else {
                    self.set_written(Err(NOT_COMPUTED.to_string()));
                    return;
                };
                if *k != key {
                    self.set_written(Err(STALE.to_string()));
                    return;
                }
                if prepared.bodies.is_empty() {
                    self.set_written(Err(StepError::NothingToExport.to_string()));
                    return;
                }
                let (prepared, target) = (Arc::clone(prepared), path.clone());
                self.written = None;
                self.step_writing = Some((
                    path,
                    Job::spawn(move |p| write_step_bodies(&target, &prepared, p)),
                ));
            }
            ExportFormat::Mesh(format) => {
                let Some((k, Ok(prepared))) = &self.prepared else {
                    self.set_written(Err(NOT_COMPUTED.to_string()));
                    return;
                };
                if *k != key {
                    self.set_written(Err(STALE.to_string()));
                    return;
                }
                let (prepared, target) = (Arc::clone(prepared), path.clone());
                self.written = None;
                self.writing = Some((
                    path,
                    Job::spawn(move |_| export_file(&target, format, &prepared.bodies)),
                ));
            }
        }
    }

    /// Übernimmt die in der Dateiauswahl gewählte Datei. Wurde bei STEP `.stp` eingegeben und
    /// gibt es die Datei mit `.step` schon, fragt die Auswahl noch einmal nach dem Ersetzen.
    pub fn accept_chosen(&mut self, path: PathBuf, state: &AppState) {
        let target = normalize_target(self.format, path.clone());
        if target != path
            && target.exists()
            && let Some(c) = self.chooser.as_mut()
        {
            c.select(&file_name(&target));
            c.confirm();
            return;
        }
        self.chooser = None;
        self.chooser_key = None;
        self.write_to(target, state);
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll. `last_dir` ist der zuletzt
    /// benutzte Ordner der Dateiauswahl.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        last_dir: &mut Option<PathBuf>,
    ) -> bool {
        self.tick(state, false);
        let status = self.status(state);
        let write_state = self.write_state();
        let write_progress = self.write_progress();
        let denied = self.close_denied;
        let writing_step = self.is_writing_step();
        let shortened = if matches!(write_state, WriteState::Done(..)) && self.format.is_step() {
            self.shortened_names(state)
        } else {
            Vec::new()
        };
        let has_selection = Self::has_selection(state);
        let before = self.format;
        let (mut format, mut quality, mut scope) = (self.format, self.quality, self.scope);
        let (mut open, mut close, mut cancel, mut recompute, mut begin, mut drop_chooser) =
            (true, false, false, false, false, false);
        let mut cancel_write = false;
        let mut chosen: Option<PathBuf> = None;
        let mut cards = Vec::new();
        let chooser = &mut self.chooser;
        let response = wide_window("Exportieren", ctx)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut format_card = |ui: &mut egui::Ui| {
                    ui.strong("Format");
                    for f in ExportFormat::ALL {
                        ui.radio_value(&mut format, f, f.label());
                    }
                };
                let mut quality_card = |ui: &mut egui::Ui| {
                    if before.is_step() {
                        ui.label(STEP_EXACT);
                    } else {
                        ui.strong("Qualität");
                        for q in Quality::ALL {
                            ui.radio_value(&mut quality, q, q.label());
                        }
                        ui.weak("Feiner heißt mehr Dreiecke und größere Dateien.");
                    }
                };
                let mut scope_card = |ui: &mut egui::Ui| {
                    ui.strong("Umfang");
                    let selected = ui
                        .add_enabled(
                            has_selection,
                            egui::RadioButton::new(scope == Scope::Selected, "Gewählter Körper"),
                        )
                        .on_disabled_hover_text("Dafür zuerst einen Körper wählen.");
                    if selected.clicked() {
                        scope = Scope::Selected;
                    }
                    ui.radio_value(&mut scope, Scope::Visible, "Alle sichtbaren Körper");
                    ui.weak("Unterdrückte Schritte zählen nicht.");
                };
                let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                    &mut [&mut format_card, &mut quality_card, &mut scope_card];
                cards = card_row(ui, "export", row);
                ui.add_space(8.0);
                match &status {
                    ExportStatus::Computing(f) => {
                        let text = if before.is_step() {
                            "Die Körper werden geprüft …"
                        } else {
                            "Das Netz wird berechnet …"
                        };
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(*f)
                                    .desired_width(PROGRESS_WIDTH)
                                    .text(text),
                            );
                            if ui.button("Abbrechen").clicked() {
                                cancel = true;
                            }
                        });
                    }
                    ExportStatus::Failed(m) => {
                        ui.colored_label(ERROR_COLOR, m);
                        if ui.button("Neu berechnen").clicked() {
                            recompute = true;
                        }
                    }
                    ExportStatus::Ready {
                        triangles,
                        bytes,
                        report,
                    } => {
                        ui.label(format!(
                            "{} Dreiecke, {}",
                            group_digits(*triangles),
                            estimate_text(*bytes)
                        ));
                        if report.closed {
                            ui.label(report_text(report));
                        } else {
                            ui.colored_label(WARNING_COLOR, report_text(report));
                            ui.colored_label(WARNING_COLOR, NOT_CLOSED_WARNING);
                        }
                    }
                    ExportStatus::StepReady {
                        bodies,
                        faces,
                        rejected,
                        skipped_meshes,
                    } => {
                        if *bodies == 0 {
                            ui.colored_label(WARNING_COLOR, STEP_NOTHING);
                        } else {
                            ui.label(format!(
                                "{} Körper, {} Flächen",
                                group_digits(*bodies as u64),
                                group_digits(*faces as u64)
                            ));
                        }
                        for r in rejected {
                            ui.colored_label(WARNING_COLOR, r);
                        }
                        if !skipped_meshes.is_empty() {
                            ui.colored_label(WARNING_COLOR, skipped_text(skipped_meshes));
                        }
                    }
                }
                match &write_state {
                    WriteState::Idle => {}
                    WriteState::Writing => {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(write_progress)
                                    .desired_width(PROGRESS_WIDTH)
                                    .text("Die Datei wird geschrieben …"),
                            );
                            if writing_step && ui.button("Abbrechen").clicked() {
                                cancel_write = true;
                            }
                        });
                        if denied {
                            ui.colored_label(WARNING_COLOR, WRITING_HINT);
                        }
                    }
                    WriteState::Done(path, n) => {
                        ui.label(format!(
                            "Fertig: „{}“ geschrieben ({}).",
                            file_name(path),
                            mb_text(*n)
                        ));
                        if !shortened.is_empty() {
                            ui.colored_label(WARNING_COLOR, shortened_text(&shortened));
                        }
                    }
                    WriteState::Failed(m) => {
                        ui.colored_label(ERROR_COLOR, format!("Nicht geschrieben: {m}"));
                    }
                }
                ui.separator();
                match chooser.as_mut() {
                    Some(c) => {
                        ui.strong("Speichern unter");
                        match c.show(ui) {
                            ChooserAction::Chosen(p) => chosen = Some(p),
                            ChooserAction::Cancel => drop_chooser = true,
                            ChooserAction::None => {}
                        }
                    }
                    None => {
                        let ready = match &status {
                            ExportStatus::Ready { .. } => true,
                            ExportStatus::StepReady { bodies, .. } => *bodies > 0,
                            _ => false,
                        } && write_state != WriteState::Writing;
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(ready, egui::Button::new("Exportieren …"))
                                .on_disabled_hover_text(disabled_hint(&status))
                                .clicked()
                            {
                                begin = true;
                            }
                            if ui.button("Schließen").clicked() {
                                close = true;
                            }
                        });
                    }
                }
            });
        let want_close = !open || close;
        let alive = !want_close || !self.request_close();
        self.last_rect = response.map(|r| r.response.rect);
        self.last_cards = cards;
        (self.format, self.quality, self.scope) = (format, quality, scope);
        let stale_chooser = self.chooser_key.is_some_and(|k| k != self.key(state));
        if self.format != before || drop_chooser || stale_chooser {
            self.chooser = None;
            self.chooser_key = None;
        }
        if self.format != before || stale_chooser {
            // Die Meldung „Fertig“ gehörte zum alten Format oder Stand.
            self.written = None;
        }
        let busy = matches!(status, ExportStatus::Computing(_))
            || self.write_state() == WriteState::Writing;
        if busy {
            ctx.request_repaint_after(REPAINT);
        }
        if cancel {
            self.cancel();
        }
        if cancel_write {
            self.cancel_write();
        }
        if recompute {
            self.recompute();
        }
        if begin {
            let dir = last_dir.clone().unwrap_or_else(default_dir);
            self.begin_save(dir, state);
        }
        if let Some(path) = chosen {
            *last_dir = path.parent().map(Path::to_path_buf);
            self.accept_chosen(path, state);
        }
        alive
    }
}
