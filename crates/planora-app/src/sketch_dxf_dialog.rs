//! „Als DXF speichern“ einer Skizze (Skizzen 11b, Spec 2), wie Fusions „Save As DXF“: Rechtsklick
//! auf eine Skizze im Browser öffnet den Dialog mit der Einheit und den Häkchen „Punkte“,
//! „Projizierte Geometrie“ und „Konstruktionsgeometrie“; OK führt zur Dateiauswahl (Endung
//! „.dxf“, Dateiname = Skizzenname). Gerechnet und geschrieben wird mit
//! [`planora_feature::sketch_dxf`] und [`planora_drawing::plain_dxf`].
//!
//! Das Speichern ändert das Dokument nicht: kein Schritt, kein neuer Zähler. Schlägt es fehl,
//! entsteht keine Datei (erst das fertige Ergebnis wird geschrieben).

use crate::browser::error_color;
use crate::dialog::{Dialog, DialogSize};
use crate::file_chooser::{ChooserAction, FileChooser, default_dir};
use crate::places::Places;
use crate::state::AppState;
use crate::surface_project_dialog::{DialogOutcome, keys};
use planora_drawing::plain_dxf::{DxfUnit, write_plain_dxf};
use planora_feature::{FeatureId, FeatureKind, SketchDxfOptions, sketch_dxf};
use planora_i18n::{DisplayUnit, current_unit, tr};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// Der Dialog „Als DXF speichern“ einer Skizze.
pub struct SketchDxfDialog {
    pub sketch: FeatureId,
    pub options: SketchDxfOptions,
    /// Nach OK: die Dateiauswahl (vorher stehen die Optionen im Fenster).
    chooser: Option<FileChooser>,
    /// Meldung des letzten Versuchs (Fehler beim Rechnen oder Schreiben).
    error: Option<String>,
}

/// Name der Einheit im Dialog.
fn unit_label(u: DxfUnit) -> String {
    match u {
        DxfUnit::Millimeter => tr!("sketch-dxf-unit-mm"),
        DxfUnit::Centimeter => tr!("sketch-dxf-unit-cm"),
        DxfUnit::Meter => tr!("sketch-dxf-unit-m"),
        DxfUnit::Inch => tr!("sketch-dxf-unit-inch"),
        DxfUnit::Foot => tr!("sketch-dxf-unit-foot"),
    }
}

/// Dateiname aus dem Skizzennamen: verbotene Zeichen werden zu „_“.
fn file_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "\\/:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    if s.trim().is_empty() {
        tr!("sketch-dxf-default-name")
    } else {
        s
    }
}

/// Schreibt die Skizze `sketch` als DXF nach `path`. Bei einem Fehler (3D-Skizze, nichts übrig,
/// Schreibfehler) entsteht keine Datei; der Text ist die Meldung für die Oberfläche.
pub fn save_sketch_dxf(
    state: &AppState,
    sketch: FeatureId,
    o: &SketchDxfOptions,
    path: &Path,
) -> Result<(), String> {
    let model = sketch_dxf(&state.document, sketch, o).map_err(|e| e.message())?;
    let mut bytes = Vec::new();
    write_plain_dxf(&model, &mut bytes).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| {
        tr!(
            "sketch-dxf-save-failed",
            reason = crate::file_chooser::io_message(e.kind()).as_str()
        )
    })
}

impl SketchDxfDialog {
    /// Vorgaben: Einheit des Dokuments, Punkte und Projiziertes an, Konstruktion aus (Annahme).
    pub fn new(_state: &AppState, sketch: FeatureId) -> SketchDxfDialog {
        let unit = match current_unit() {
            DisplayUnit::Millimeter => DxfUnit::Millimeter,
            DisplayUnit::Zoll => DxfUnit::Inch,
        };
        SketchDxfDialog {
            sketch,
            options: SketchDxfOptions {
                unit,
                points: true,
                projected: true,
                construction: false,
            },
            chooser: None,
            error: None,
        }
    }

    /// Prüft die Optionen und öffnet die Dateiauswahl (ein Fehler bleibt als Meldung stehen).
    fn begin_save(&mut self, state: &AppState, dir: PathBuf) {
        match sketch_dxf(&state.document, self.sketch, &self.options) {
            Err(e) => self.error = Some(e.message()),
            Ok(_) => {
                self.error = None;
                let name = state
                    .document
                    .feature(self.sketch)
                    .map(|f| f.name.clone())
                    .unwrap_or_default();
                self.chooser = Some(FileChooser::save(dir, file_stem(&name), "dxf"));
            }
        }
    }

    /// Zeichnet den Dialog. `last_dir` ist der zuletzt benutzte Ordner der Dateiauswahl,
    /// `places` sind deren gemeinsame Schnellzugriffe.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        last_dir: &mut Option<PathBuf>,
        places: &mut Places,
    ) -> DialogOutcome {
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        let mut chosen: Option<PathBuf> = None;
        let mut drop_chooser = false;
        let mut o = self.options;
        let error = self.error.clone();
        let in_chooser = self.chooser.is_some();
        let chooser = RefCell::new(&mut self.chooser);
        let places = RefCell::new(places);
        let (mut body_action, mut footer_action) = (ChooserAction::None, ChooserAction::None);
        Dialog::new(tr!("sketch-dxf-title"), DialogSize::File)
            .dock_key("sketch_dxf")
            .id(egui::Id::unique("sketch_dxf_dialog"))
            .center_on_open()
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| {
                    if let Some(m) = &error {
                        ui.colored_label(error_color(ui.visuals().dark_mode), m);
                    }
                    if let Some(c) = chooser.borrow_mut().as_mut() {
                        body_action = c.show_body(ui, &mut places.borrow_mut());
                        return;
                    }
                    ui.horizontal(|ui| {
                        ui.label(tr!("sketch-dxf-unit"));
                        egui::ComboBox::from_id_salt("sketch_dxf_unit")
                            .selected_text(unit_label(o.unit))
                            .show_ui(ui, |ui| {
                                for u in [
                                    DxfUnit::Millimeter,
                                    DxfUnit::Centimeter,
                                    DxfUnit::Meter,
                                    DxfUnit::Inch,
                                    DxfUnit::Foot,
                                ] {
                                    ui.selectable_value(&mut o.unit, u, unit_label(u));
                                }
                            });
                    });
                    ui.checkbox(&mut o.points, tr!("sketch-dxf-points"));
                    ui.checkbox(&mut o.projected, tr!("sketch-dxf-projected"));
                    ui.checkbox(&mut o.construction, tr!("sketch-dxf-construction"));
                },
                |ui| {
                    if let Some(c) = chooser.borrow_mut().as_mut() {
                        footer_action = c.show_footer(ui);
                        return;
                    }
                    ui.horizontal(|ui| {
                        if ui.button(tr!("common-ok")).clicked() {
                            ok = true;
                        }
                        if ui.button(tr!("common-cancel")).clicked() {
                            cancel = true;
                        }
                    });
                    let (enter, esc) = keys(ui, true);
                    ok |= enter;
                    cancel |= esc;
                },
            );
        for a in [body_action, footer_action] {
            match a {
                ChooserAction::Chosen(p) => chosen = Some(p),
                ChooserAction::Cancel => drop_chooser = true,
                ChooserAction::None => {}
            }
        }
        if !in_chooser {
            self.options = o;
        }
        if !open || drop_chooser || (cancel && !in_chooser) {
            return DialogOutcome::Cancelled;
        }
        if ok && !in_chooser {
            let dir = last_dir.clone().unwrap_or_else(default_dir);
            self.begin_save(state, dir);
        }
        if let Some(path) = chosen {
            *last_dir = path.parent().map(Path::to_path_buf);
            match save_sketch_dxf(state, self.sketch, &self.options, &path) {
                Ok(()) => return DialogOutcome::Applied,
                Err(m) => {
                    // Zurück zu den Optionen, die Meldung bleibt stehen.
                    self.chooser = None;
                    self.error = Some(m);
                }
            }
        }
        DialogOutcome::Open
    }
}

/// Zeigt den offenen Dialog (Browser → Skizze → „Als DXF speichern“). Er schließt, wenn die
/// Skizze verschwindet. Das Dokument bleibt unberührt.
pub fn show_open(
    ctx: &egui::Context,
    state: &mut AppState,
    last_dir: &mut Option<PathBuf>,
    places: &mut Places,
) -> Option<DialogOutcome> {
    let mut d = state.sketch_dxf_dialog.take()?;
    if !state
        .document
        .feature(d.sketch)
        .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_)))
    {
        return Some(DialogOutcome::Cancelled);
    }
    let out = d.show(ctx, state, last_dir, places);
    if out == DialogOutcome::Open && state.sketch_dxf_dialog.is_none() {
        state.sketch_dxf_dialog = Some(d);
    }
    Some(out)
}

impl AppState {
    /// Öffnet „Als DXF speichern“ für die Skizze `sketch` (nur für Skizzen, sonst `false`).
    pub fn open_sketch_dxf(&mut self, sketch: FeatureId) -> bool {
        if !self
            .document
            .feature(sketch)
            .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        {
            return false;
        }
        self.sketch_dxf_dialog = Some(SketchDxfDialog::new(self, sketch));
        true
    }
}
