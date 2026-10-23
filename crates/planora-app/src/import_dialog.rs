//! Fenster „Netz importieren“ (Spec §8): Datei wählen (STL, OBJ), im Arbeitsthread lesen
//! (Fortschritt, Abbrechen), Prüfbericht, Name und Einheit der Datei, „Importieren“. Das
//! Dokument ändert sich erst beim Importieren und nur bei Erfolg (Spec §5). Ein gelesenes Netz
//! gehört zu dem Dokumentstand, bei dem die Datei gewählt wurde; hat sich das Dokument seither
//! geändert, wird nichts übernommen.

use crate::browser::error_color;
use crate::cards::card_row;
use crate::dialog::{Dialog, DialogSize};
use crate::export_dialog::{LOST, PROGRESS_WIDTH, REPAINT, file_name, group_digits};
use crate::file_chooser::{ChooserAction, FileChooser};
use crate::jobs::{Job, Poll};
use crate::mesh_body::{report_text, warning_color};
use crate::places::Places;
use crate::state::AppState;
use crate::theme;
use planora_exchange::{ExchangeError, ExchangeMesh, MeshReport, check_mesh, import_file};
use planora_feature::FeatureId;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Hinweis bei nicht geschlossenem Netz.
pub const OPEN_MESH_HINT: &str = "Der Netz-Körper lässt sich trotzdem anlegen und anzeigen; Volumen und Oberfläche stimmen dann nur ungefähr.";
/// Meldung, wenn sich das Dokument seit der Dateiwahl geändert hat.
pub const STALE: &str =
    "Das Dokument hat sich seit dem Lesen der Datei geändert. Bitte die Datei noch einmal wählen.";

/// Einheit der Quelldatei (Spec §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Millimeter,
    Zentimeter,
    Meter,
    Zoll,
}

impl Unit {
    pub const ALL: [Unit; 4] = [Unit::Millimeter, Unit::Zentimeter, Unit::Meter, Unit::Zoll];

    /// Millimeter je Einheit der Datei (Maßstab des Netz-Körpers).
    pub fn factor(self) -> f64 {
        match self {
            Unit::Millimeter => 1.0,
            Unit::Zentimeter => 10.0,
            Unit::Meter => 1000.0,
            Unit::Zoll => 25.4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Unit::Millimeter => "Millimeter",
            Unit::Zentimeter => "Zentimeter",
            Unit::Meter => "Meter",
            Unit::Zoll => "Zoll",
        }
    }
}

/// Gelesene und geprüfte Datei.
pub struct Loaded {
    pub path: PathBuf,
    pub mesh: Arc<ExchangeMesh>,
    pub report: MeshReport,
    /// Änderungszähler des Dokuments bei der Dateiwahl; `None`, wenn das Lesen ohne Dokument
    /// angestoßen wurde (dann entfällt der Vergleich).
    pub edits: Option<u64>,
}

type ReadJob = Job<Result<(ExchangeMesh, MeshReport), ExchangeError>>;

/// Laufendes Lesen: Datei, Dokumentstand bei der Dateiwahl, Auftrag.
struct Reading {
    path: PathBuf,
    edits: Option<u64>,
    job: ReadJob,
}

pub struct ImportDialog {
    chooser: FileChooser,
    reading: Option<Reading>,
    pub loaded: Option<Loaded>,
    pub name: String,
    pub unit: Unit,
    pub error: Option<String>,
    /// Rechteck des Fensters im letzten Bild (Tests der vollen Breite).
    pub last_rect: Option<egui::Rect>,
    /// Rechtecke der Karten im letzten Bild (Tests der gleichen Höhe).
    pub last_cards: Vec<egui::Rect>,
}

/// Dateiname ohne Endung als Vorschlag für den Namen.
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().trim().to_string())
        .unwrap_or_default()
}

impl ImportDialog {
    pub fn new(dir: PathBuf) -> ImportDialog {
        ImportDialog {
            chooser: FileChooser::open(dir, vec!["stl", "obj"]),
            reading: None,
            loaded: None,
            name: String::new(),
            unit: Unit::Millimeter,
            error: None,
            last_rect: None,
            last_cards: Vec::new(),
        }
    }

    /// Liest `path` im Arbeitsthread und prüft das Netz (ohne Bindung an ein Dokument).
    pub fn load(&mut self, path: PathBuf) {
        self.start(path, None);
    }

    /// Wie [`ImportDialog::load`], das Ergebnis gilt aber nur für den Dokumentstand `edits`.
    pub fn load_at(&mut self, path: PathBuf, edits: u64) {
        self.start(path, Some(edits));
    }

    fn start(&mut self, path: PathBuf, edits: Option<u64>) {
        self.error = None;
        self.loaded = None;
        let source = path.clone();
        // Ein noch laufendes Lesen wird hier fallengelassen und damit abgebrochen.
        self.reading = Some(Reading {
            path,
            edits,
            job: Job::spawn(move |progress| {
                let mesh = import_file(&source, progress)?;
                let report = check_mesh(&mesh);
                Ok((mesh, report))
            }),
        });
    }

    pub fn is_loading(&self) -> bool {
        self.reading.is_some()
    }

    pub fn progress(&self) -> f32 {
        self.reading.as_ref().map_or(0.0, |r| r.job.fraction())
    }

    /// Bricht das Lesen ab (der Arbeitsthread endet beim nächsten Prüfen).
    pub fn cancel(&mut self) {
        if self.reading.take().is_some() {
            self.error = Some(ExchangeError::Cancelled.to_string());
        }
    }

    /// Holt das Ergebnis des Lesens ab; mit `block` wird gewartet (Tests).
    pub fn tick(&mut self, block: bool) {
        let done = match &self.reading {
            Some(r) => match r.job.fetch(block) {
                Poll::Pending => None,
                Poll::Ready(res) => Some(res.map_err(|e| e.to_string())),
                Poll::Lost => Some(Err(LOST.to_string())),
            },
            None => None,
        };
        let Some(result) = done else { return };
        // Nach `Ready` oder `Lost` liefert der Auftrag nichts mehr: verwerfen.
        let Some(Reading { path, edits, .. }) = self.reading.take() else {
            return;
        };
        match result {
            Ok((mesh, report)) => {
                self.name = file_stem(&path);
                self.loaded = Some(Loaded {
                    path,
                    mesh: Arc::new(mesh),
                    report,
                    edits,
                });
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Ist das gelesene Netz noch zum Dokument passend? Sonst wird es verworfen.
    fn drop_if_stale(&mut self, state: &AppState) {
        if let Some(l) = &self.loaded
            && l.edits.is_some_and(|e| e != state.document.edit_count())
        {
            self.loaded = None;
            self.error = Some(STALE.into());
        }
    }

    /// Legt den Netz-Körper an (Name, Einheit); das Dokument ändert sich nur bei Erfolg.
    pub fn import(&self, state: &mut AppState) -> Result<FeatureId, String> {
        let Some(l) = &self.loaded else {
            return Err("Bitte zuerst eine Datei wählen.".into());
        };
        if l.edits.is_some_and(|e| e != state.document.edit_count()) {
            return Err(STALE.into());
        }
        state.import_mesh(&self.name, Arc::clone(&l.mesh), self.unit.factor())
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll.
    ///
    /// `places` sind die gemeinsamen Schnellzugriffe der Dateiauswahl (Favoriten,
    /// Projektordner); geänderte Favoriten stehen danach darin.
    pub fn show_in(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        last_dir: &mut Option<PathBuf>,
        places: &mut Places,
    ) -> bool {
        self.tick(false);
        self.drop_if_stale(state);
        let (mut open, mut close, mut cancel, mut import) = (true, false, false, false);
        let mut chosen = None;
        let mut cards = Vec::new();
        let (mut name, mut unit) = (self.name.clone(), self.unit);
        let loading = self.is_loading().then(|| self.progress());
        let response = Dialog::new("Netz importieren", DialogSize::Wide)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.strong("Datei wählen (STL oder OBJ)");
                match self.chooser.show_with(ui, places) {
                    ChooserAction::Chosen(p) => chosen = Some(p),
                    ChooserAction::Cancel => close = true,
                    ChooserAction::None => {}
                }
                if let Some(f) = loading {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::ProgressBar::new(f)
                                .desired_width(PROGRESS_WIDTH)
                                .text("Die Datei wird gelesen …"),
                        );
                        if ui.button("Abbrechen").clicked() {
                            cancel = true;
                        }
                    });
                }
                if let Some(e) = &self.error {
                    ui.colored_label(error_color(ui.visuals().dark_mode), e);
                }
                if let Some(l) = &self.loaded {
                    ui.separator();
                    let mut report_card = |ui: &mut egui::Ui| {
                        ui.strong("Prüfung");
                        ui.label(format!("Datei: {}", file_name(&l.path)));
                        ui.label(format!(
                            "{} Dreiecke, {} Punkte",
                            group_digits(l.report.triangles as u64),
                            group_digits(l.report.vertices as u64)
                        ));
                        if l.report.closed {
                            ui.label(report_text(&l.report));
                        } else {
                            ui.colored_label(
                                warning_color(ui.visuals().dark_mode),
                                report_text(&l.report),
                            );
                            ui.colored_label(warning_color(ui.visuals().dark_mode), OPEN_MESH_HINT);
                        }
                    };
                    let mut name_card = |ui: &mut egui::Ui| {
                        ui.strong("Name");
                        theme::text_field(
                            ui,
                            egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY),
                        );
                        ui.add_space(4.0);
                        ui.strong("Einheit der Datei");
                        ui.horizontal_wrapped(|ui| {
                            for u in Unit::ALL {
                                ui.radio_value(&mut unit, u, u.label());
                            }
                        });
                    };
                    let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                        &mut [&mut report_card, &mut name_card];
                    cards = card_row(ui, "import", row);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Importieren").clicked() {
                            import = true;
                        }
                        if ui.button("Schließen").clicked() {
                            close = true;
                        }
                    });
                } else if ui.button("Schließen").clicked() {
                    close = true;
                }
            });
        self.last_rect = response.map(|r| r.response.rect);
        self.last_cards = cards;
        (self.name, self.unit) = (name, unit);
        if loading.is_some() {
            ctx.request_repaint_after(REPAINT);
        }
        if cancel {
            self.cancel();
        }
        if let Some(p) = chosen {
            *last_dir = p.parent().map(Path::to_path_buf);
            self.load_at(p, state.document.edit_count());
        }
        if import {
            match self.import(state) {
                Ok(_) => return false,
                Err(e) => self.error = Some(e),
            }
        }
        open && !close
    }
}
