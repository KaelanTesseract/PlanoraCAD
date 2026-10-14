//! Fenster „Exportieren“ (Spec §8): Format, Qualität und Umfang als gleich hohe Karten über die
//! ganze Breite; das Netz entsteht im Arbeitsthread (Fortschritt, Abbrechen); danach Zahl der
//! Dreiecke, geschätzte Größe und Prüfbericht mit Warnung (Spec E6); gespeichert wird über die
//! eigene Dateiauswahl (Plan P14) und eine Hilfsdatei (Spec §5).

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

/// Was das Fenster zum Netz zeigt.
#[derive(Debug, PartialEq)]
pub enum ExportStatus {
    Computing(f32),
    Ready {
        triangles: u64,
        bytes: u64,
        report: MeshReport,
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

/// Kurzbeschreibung eines Formats für die Auswahl.
pub fn format_label(f: Format) -> &'static str {
    match f {
        Format::Stl => "STL – für den 3D-Druck",
        Format::Obj => "OBJ – für viele 3D-Programme",
        Format::ThreeMf => "3MF – für den 3D-Druck, mit Namen",
        Format::Glb => "glTF (.glb) – zum Ansehen und Zeigen",
    }
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

type Key = (Quality, Scope, u64, Option<BodyRef>);
type PrepJob = Job<Result<Prepared, String>>;
type PrepResult = Result<Arc<Prepared>, String>;
type WriteJob = Job<Result<u64, ExchangeError>>;

pub struct ExportDialog {
    pub format: Format,
    pub quality: Quality,
    pub scope: Scope,
    prepared: Option<(Key, PrepResult)>,
    job: Option<(Key, PrepJob)>,
    chooser: Option<FileChooser>,
    /// Schlüssel zur Zeit des Öffnens der Dateiauswahl; ändert er sich, schließt sie.
    chooser_key: Option<Key>,
    writing: Option<(PathBuf, WriteJob)>,
    written: Option<Result<(PathBuf, u64), String>>,
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
            format: Format::Stl,
            quality: Quality::Normal,
            scope: if selected {
                Scope::Selected
            } else {
                Scope::Visible
            },
            prepared: None,
            job: None,
            chooser: None,
            chooser_key: None,
            writing: None,
            written: None,
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
        (
            self.quality,
            self.scope,
            state.document.edit_count(),
            selected,
        )
    }

    /// Startet das Vernetzen, wenn sich Qualität, Umfang oder Dokument geändert haben, und holt
    /// fertige Ergebnisse ab. Mit `block` wird gewartet (Tests).
    pub fn tick(&mut self, state: &AppState, block: bool) {
        // Ohne Auswahl gibt es „Gewählter Körper“ nicht mehr (Anzeige und Export stimmen überein).
        if self.scope == Scope::Selected && !Self::has_selection(state) {
            self.scope = Scope::Visible;
        }
        let key = self.key(state);
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
        let wrote = match &self.writing {
            Some((path, job)) => match job.fetch(block) {
                Poll::Pending => None,
                Poll::Ready(r) => Some(r.map(|n| (path.clone(), n)).map_err(|e| e.to_string())),
                Poll::Lost => Some(Err(LOST.to_string())),
            },
            None => None,
        };
        if let Some(w) = wrote {
            self.written = Some(w);
            self.writing = None;
        }
    }

    pub fn status(&self, state: &AppState) -> ExportStatus {
        let key = self.key(state);
        if let Some((k, job)) = &self.job
            && *k == key
        {
            return ExportStatus::Computing(job.fraction());
        }
        match &self.prepared {
            Some((k, Ok(p))) if *k == key => ExportStatus::Ready {
                triangles: p.triangles(),
                bytes: self.format.estimate_bytes(p.triangles(), p.vertices()),
                report: p.report,
            },
            Some((k, Err(m))) if *k == key => ExportStatus::Failed(m.clone()),
            _ => ExportStatus::Computing(0.0),
        }
    }

    pub fn write_state(&self) -> WriteState {
        if self.writing.is_some() {
            return WriteState::Writing;
        }
        match &self.written {
            None => WriteState::Idle,
            Some(Ok((p, n))) => WriteState::Done(p.clone(), *n),
            Some(Err(m)) => WriteState::Failed(m.clone()),
        }
    }

    /// Bricht das Vernetzen ab; das Fenster zeigt „Abgebrochen“, bis neu berechnet wird.
    pub fn cancel(&mut self) {
        if let Some((key, _)) = self.job.take() {
            self.prepared = Some((key, Err(ExchangeError::Cancelled.to_string())));
        }
    }

    /// Verwirft das Ergebnis; das nächste `tick` rechnet neu.
    pub fn recompute(&mut self) {
        self.prepared = None;
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

    /// Schreibt das fertige Netz im Arbeitsthread nach `path` (über eine Hilfsdatei).
    /// Schreibt nur, wenn das Netz noch zum Stand von `state` passt; sonst gibt es eine Meldung.
    pub fn write_to(&mut self, path: PathBuf, state: &AppState) {
        let key = self.key(state);
        let Some((k, Ok(prepared))) = &self.prepared else {
            self.written = Some(Err(NOT_COMPUTED.to_string()));
            return;
        };
        if *k != key {
            self.written = Some(Err(STALE.to_string()));
            return;
        }
        let (prepared, format, target) = (Arc::clone(prepared), self.format, path.clone());
        self.written = None;
        self.writing = Some((
            path,
            Job::spawn(move |_| export_file(&target, format, &prepared.bodies)),
        ));
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
        let has_selection = Self::has_selection(state);
        let before = self.format;
        let (mut format, mut quality, mut scope) = (self.format, self.quality, self.scope);
        let (mut open, mut close, mut cancel, mut recompute, mut begin, mut drop_chooser) =
            (true, false, false, false, false, false);
        let mut chosen: Option<PathBuf> = None;
        let mut cards = Vec::new();
        let chooser = &mut self.chooser;
        let response = wide_window("Exportieren", ctx)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut format_card = |ui: &mut egui::Ui| {
                    ui.strong("Format");
                    for f in Format::ALL {
                        ui.radio_value(&mut format, f, format_label(f));
                    }
                };
                let mut quality_card = |ui: &mut egui::Ui| {
                    ui.strong("Qualität");
                    for q in Quality::ALL {
                        ui.radio_value(&mut quality, q, q.label());
                    }
                    ui.weak("Feiner heißt mehr Dreiecke und größere Dateien.");
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
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(*f)
                                    .desired_width(PROGRESS_WIDTH)
                                    .text("Das Netz wird berechnet …"),
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
                }
                match &write_state {
                    WriteState::Idle => {}
                    WriteState::Writing => {
                        ui.label("Die Datei wird geschrieben …");
                    }
                    WriteState::Done(path, n) => {
                        ui.label(format!(
                            "Fertig: „{}“ geschrieben ({}).",
                            file_name(path),
                            mb_text(*n)
                        ));
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
                        let ready = matches!(status, ExportStatus::Ready { .. })
                            && write_state != WriteState::Writing;
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(ready, egui::Button::new("Exportieren …"))
                                .on_disabled_hover_text(
                                    "Erst möglich, wenn das Netz fertig berechnet ist.",
                                )
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
        if matches!(status, ExportStatus::Computing(_)) || write_state == WriteState::Writing {
            ctx.request_repaint_after(REPAINT);
        }
        if cancel {
            self.cancel();
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
            self.chooser = None;
            self.write_to(path, state);
        }
        open && !close
    }
}
