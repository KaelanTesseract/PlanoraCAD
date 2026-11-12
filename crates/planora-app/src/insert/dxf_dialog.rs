//! Fenster „DXF einfügen“ (Skizzen 9, Spec C): Datei wählen, im Hintergrund lesen, dann Ebene,
//! Einheiten (mm, cm, m, in, ft), Einfügemodus („Eine Skizze“ oder „Eine Skizze pro Layer“),
//! X- und Y-Abstand, Layerliste und Kontrollpunktrahmen. Die Vorschau liegt als Linien in der
//! Ansicht. „OK“ ist ein Rückgängig-Schritt: im Dokument (neue Skizzen) oder in der offenen
//! Skizze („DXF einfügen“). Abbrechen, Esc und ✕ hinterlassen nichts.

use super::pick::{InsertTarget, TargetPick};
use super::{BackgroundRead, DialogAction, UNITS, display_unit, skipped_summary, unit_label};
use crate::browser::error_color;
use crate::cards::card_row;
use crate::dialog::{Dialog, DialogSize};
use crate::export_dialog::{PROGRESS_WIDTH, REPAINT, file_name};
use crate::file_chooser::{ChooserAction, FileChooser};
use crate::formula_field::{FormulaField, eval_shown, value_field_text};
use crate::mesh_body::warning_color;
use crate::msg::Msg;
use crate::places::Places;
use crate::state::{AppState, InsertPreview};
use crate::theme;
use planora_exchange::import2d::{Import2d, Skipped, Unit2d};
use planora_exchange::read_dxf;
use planora_feature::{FeatureId, ParamUnit};
use planora_i18n::tr;
use planora_math::{DVec2, Frame};
use planora_sketch::import::{InsertOptions, InsertReport, Placement, preview_polylines};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Höchste Höhe der Layerliste, darüber rollt sie.
const LAYER_LIST_HEIGHT: f32 = 180.0;
/// Breite der Abstandsfelder.
const OFFSET_WIDTH: f32 = 120.0;
/// Abweichung der Vorschau-Linien, bezogen auf die Diagonale der Datei.
const PREVIEW_TOL_REL: f64 = 1e-3;

/// Gelesene Datei.
pub struct DxfLoaded {
    pub path: PathBuf,
    pub import: Arc<Import2d>,
}

/// Was die Vorschau zuletzt berechnet hat (sie wird nur bei einer Änderung neu berechnet).
#[derive(Clone, Debug, PartialEq)]
struct PreviewKey {
    run: u64,
    units: Unit2d,
    offset: DVec2,
    layers: Vec<bool>,
    frame: Frame,
}

pub struct DxfDialog {
    chooser: FileChooser,
    choosing: bool,
    reading: Option<BackgroundRead<Import2d>>,
    /// Nummer des Laufs, aus dem `loaded` stammt.
    loaded_run: u64,
    pub loaded: Option<DxfLoaded>,
    pub units: Unit2d,
    /// „Eine Skizze“ (sonst „Eine Skizze pro Layer“); bei offener Skizze immer eine.
    pub single: bool,
    /// X- und Y-Abstand in mm (Y positiv nach oben).
    pub offset: DVec2,
    offset_text: [String; 2],
    /// Häkchen je Layer (gleiche Stelle wie in `Import2d::layers`).
    pub layers: Vec<bool>,
    /// Kontrollpunktrahmen.
    pub frame: bool,
    pub target: TargetPick,
    /// Meldung im Fenster; wird beim Zeichnen übersetzt.
    pub error: Option<Msg>,
    preview_key: Option<PreviewKey>,
    /// Rechteck des Fensters im letzten Bild (Tests der vollen Breite).
    pub last_rect: Option<egui::Rect>,
    /// Rechtecke der Karten im letzten Bild (Tests der gleichen Höhe).
    pub last_cards: Vec<egui::Rect>,
}

impl DxfDialog {
    /// Neues Fenster im Ordner `dir`; Ziel nach dem Zustand (offene Skizze, Auswahl).
    pub fn new(dir: PathBuf, state: &AppState) -> DxfDialog {
        DxfDialog {
            chooser: FileChooser::open(dir, vec!["dxf"]),
            choosing: true,
            reading: None,
            loaded_run: 0,
            loaded: None,
            units: display_unit(),
            single: true,
            offset: DVec2::ZERO,
            offset_text: [
                value_field_text(0.0, ParamUnit::Length),
                value_field_text(0.0, ParamUnit::Length),
            ],
            layers: Vec::new(),
            frame: false,
            target: TargetPick::new(state),
            error: None,
            preview_key: None,
            last_rect: None,
            last_cards: Vec::new(),
        }
    }

    /// Fügt in die offene Skizze ein (dann ist „pro Layer“ gesperrt und das Wählfeld entfällt)?
    pub fn into_open(&self) -> bool {
        self.target.into_open()
    }

    /// Liest `path` im Hintergrund; ein noch laufendes Lesen wird abgebrochen.
    pub fn load(&mut self, path: PathBuf) {
        if let Some(r) = self.reading.as_mut() {
            r.cancel();
        }
        self.error = None;
        self.reading = Some(BackgroundRead::start(path, read_dxf));
    }

    pub fn is_loading(&self) -> bool {
        self.reading.is_some()
    }

    pub fn progress(&self) -> f32 {
        self.reading.as_ref().map_or(0.0, BackgroundRead::progress)
    }

    /// Bricht das Lesen ab; sein Ergebnis kommt nie an.
    pub fn cancel(&mut self) {
        if let Some(mut r) = self.reading.take() {
            r.cancel();
            self.error = Some(crate::msg!("projects-cancelled"));
        }
    }

    /// Die Meldung im Fenster in der aktuellen Sprache.
    pub fn error_text(&self) -> Option<String> {
        self.error.as_ref().map(Msg::text)
    }

    /// Holt das Ergebnis des Lesens ab; mit `block` wird gewartet (Tests).
    pub fn tick(&mut self, block: bool) {
        let Some(r) = self.reading.as_mut() else {
            return;
        };
        let result = if block { r.wait() } else { r.poll() };
        if result.is_none() && r.is_running() {
            return;
        }
        let Some(r) = self.reading.take() else {
            return;
        };
        match result {
            Some(Ok(import)) => {
                self.units = import.units.unwrap_or_else(display_unit);
                self.layers = vec![true; import.layers.len()];
                self.loaded_run = r.run();
                self.loaded = Some(DxfLoaded {
                    path: r.path().to_path_buf(),
                    import: Arc::new(import),
                });
                self.preview_key = None;
                self.choosing = false;
                self.error = None;
            }
            Some(Err(e)) => self.error = Some(Msg::Raw(e.to_string())),
            None => {}
        }
    }

    /// Lage der Geometrie: Maßstab der Einheit, dann der Abstand.
    pub fn placement(&self) -> Placement {
        Placement {
            scale: DVec2::splat(self.units.mm()),
            offset: self.offset,
            ..Placement::default()
        }
    }

    fn options(&self, layers: Vec<bool>) -> InsertOptions {
        InsertOptions {
            fixed: false,
            show_frame: self.frame,
            layers,
        }
    }

    /// „OK“: fügt ein (ein Rückgängig-Schritt). Danach steht der Hinweis zum Übersprungenen in
    /// `state.info`. Bei einem Fehler bleibt alles unverändert und die Meldung steht im Fenster.
    pub fn insert(&mut self, state: &mut AppState) -> Result<(), String> {
        match self.try_insert(state) {
            Ok(reports) => {
                self.error = None;
                state.message = None;
                let file = self
                    .loaded
                    .as_ref()
                    .map(|l| l.import.skipped)
                    .unwrap_or_default();
                state.info = hint(&file, &reports);
                state.info_outlives_step = state.info.is_some();
                Ok(())
            }
            Err(e) => {
                let text = e.text();
                self.error = Some(e);
                Err(text)
            }
        }
    }

    fn try_insert(&self, state: &mut AppState) -> Result<Vec<InsertReport>, Msg> {
        let Some(l) = &self.loaded else {
            return Err(crate::msg!("import-choose-file"));
        };
        if !self.layers.iter().any(|c| *c) {
            return Err(crate::msg!("insert-no-layer"));
        }
        let Some(target) = self.target.target() else {
            return Err(crate::msg!("insert-target-empty"));
        };
        let place = self.placement();
        let lib = state
            .fonts()
            .cloned()
            .unwrap_or_else(|| Arc::new(planora_text::FontLibrary::bundled_only()));
        let shaper = crate::fonts_library::Guarded(&lib);
        match target {
            InsertTarget::OpenSketch => {
                // Die Skizze muss noch die vom Öffnen sein.
                if state.open_sketch_id() != self.target.open_sketch() {
                    return Err(crate::msg!("state-sketch-not-found"));
                }
                let Some((se, sk)) = state.session_sketch_mut() else {
                    return Err(crate::msg!("state-sketch-not-found"));
                };
                let report = sk
                    .insert_geometry(
                        &l.import,
                        &place,
                        &self.options(self.layers.clone()),
                        &shaper,
                    )
                    .map_err(|e| Msg::Raw(e.to_string()))?;
                se.last_report = Some(sk.clone().solve());
                se.edit_label = Some(crate::msg!("insert-dxf-undo"));
                Ok(vec![report])
            }
            InsertTarget::Plane(plane) => {
                let Some(frame) = self.target.frame(state) else {
                    return Err(crate::msg!("insert-target-empty"));
                };
                // Je Skizze: Name (ohne: „Skizze N“) und welche Layer.
                let jobs: Vec<(Option<&str>, Vec<bool>)> = if self.single {
                    vec![(None, self.layers.clone())]
                } else {
                    l.import
                        .layers
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| self.layers.get(*i).copied().unwrap_or(true))
                        .map(|(i, layer)| {
                            let only = (0..l.import.layers.len()).map(|j| j == i).collect();
                            (Some(layer.name.as_str()), only)
                        })
                        .collect()
                };
                let mut added: Vec<FeatureId> = Vec::new();
                let mut reports = Vec::new();
                for (name, layers) in jobs {
                    let opt = self.options(layers);
                    let r = state.add_filled_sketch(plane.clone(), frame, name, |sk| {
                        sk.insert_geometry(&l.import, &place, &opt, &shaper)
                            .map_err(|e| e.to_string())
                    });
                    match r {
                        Ok((id, report)) => {
                            added.push(id);
                            reports.push(report);
                        }
                        Err(e) => {
                            // Alles oder nichts: schon angelegte Skizzen gehen wieder.
                            state.remove_features(&added);
                            return Err(Msg::Raw(e));
                        }
                    }
                }
                Ok(reports)
            }
        }
    }

    /// Rechnet die Vorschau neu, wenn sich etwas geändert hat, und legt sie in den Zustand.
    fn update_preview(&mut self, state: &mut AppState) {
        let (Some(l), Some(frame)) = (&self.loaded, self.target.frame(state)) else {
            self.preview_key = None;
            state.insert_preview = None;
            return;
        };
        let key = PreviewKey {
            run: self.loaded_run,
            units: self.units,
            offset: self.offset,
            layers: self.layers.clone(),
            frame,
        };
        if self.preview_key.as_ref() == Some(&key) && state.insert_preview.is_some() {
            return;
        }
        let place = self.placement();
        let diag = l
            .import
            .bounds()
            .map_or(1.0, |(lo, hi)| (hi - lo).length() * self.units.mm());
        let tol = (diag * PREVIEW_TOL_REL).max(1e-6);
        let lines = preview_polylines(&l.import, &place, &self.layers, tol);
        state.insert_preview = Some(InsertPreview { frame, lines });
        self.preview_key = Some(key);
    }

    /// Räumt Wahlmodus und Vorschau ab (Fenster geschlossen, auf jedem Weg).
    pub fn finish(state: &mut AppState) {
        TargetPick::finish(state);
        state.insert_preview = None;
    }

    /// Zeichnet das Fenster. `places` sind die gemeinsamen Schnellzugriffe der Dateiauswahl.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        last_dir: &mut Option<PathBuf>,
        places: &mut Places,
    ) -> DialogAction {
        // Eine Skizze, die seit dem Öffnen geöffnet oder beendet wurde: das Fenster gilt nicht
        // mehr (Werkzeugwechsel hinterlässt nichts).
        if state.open_sketch_id() != self.target.open_sketch() {
            DxfDialog::finish(state);
            return DialogAction::Closed;
        }
        self.tick(false);
        self.target.sync(state);
        let (mut open, mut close, mut cancel_read, mut ok, mut other, mut back) =
            (true, false, false, false, false, false);
        let mut chosen = None;
        let mut cards = Vec::new();
        let loading = self.is_loading().then(|| self.progress());
        let choosing = self.choosing || self.loaded.is_none();
        let error = self.error_text();
        let pick_error = self.target.error.as_ref().map(Msg::text);
        let into_open = self.into_open();
        let has_loaded = self.loaded.is_some();
        let scope = state.document.param_scope();
        let ready = has_loaded && self.target.target().is_some() && self.layers.iter().any(|c| *c);
        let chooser = RefCell::new(&mut self.chooser);
        let (mut body_action, mut footer_action) = (ChooserAction::None, ChooserAction::None);
        let (mut units, mut single, mut frame) = (self.units, self.single, self.frame);
        let mut layers = self.layers.clone();
        let mut texts = self.offset_text.clone();
        let mut field_errors: Vec<String> = Vec::new();
        let target = &mut self.target;
        let loaded = &self.loaded;
        let response = Dialog::new(tr!("insert-dxf-title"), DialogSize::File)
            .center_on_open()
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| {
                    let dark = ui.visuals().dark_mode;
                    if choosing {
                        ui.strong(tr!("insert-dxf-choose"));
                        body_action = chooser.borrow_mut().show_body(ui, places);
                    }
                    if let Some(f) = loading {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(f)
                                    .desired_width(PROGRESS_WIDTH)
                                    .text(tr!("import-reading")),
                            );
                            if ui.button(tr!("common-cancel")).clicked() {
                                cancel_read = true;
                            }
                        });
                    }
                    for e in error.iter().chain(&pick_error) {
                        ui.colored_label(error_color(dark), e);
                    }
                    let (Some(l), false) = (loaded, choosing) else {
                        return;
                    };
                    ui.separator();
                    let mut file_card = |ui: &mut egui::Ui| {
                        ui.strong(tr!("insert-file"));
                        ui.add(egui::Label::new(file_name(&l.path)).wrap());
                        ui.add_space(theme::SPACE_2);
                        ui.strong(tr!("insert-target"));
                        target.show(ui, state);
                        ui.add_space(theme::SPACE_2);
                        ui.strong(tr!("insert-units"));
                        ui.horizontal_wrapped(|ui| {
                            for u in UNITS {
                                ui.radio_value(&mut units, u, unit_label(u));
                            }
                        });
                        if l.import.units.is_none() {
                            ui.colored_label(warning_color(dark), tr!("insert-dxf-no-unit"));
                        }
                        ui.add_space(theme::SPACE_2);
                        ui.strong(tr!("insert-mode"));
                        ui.horizontal_wrapped(|ui| {
                            ui.radio_value(&mut single, true, tr!("insert-mode-single"));
                            ui.add_enabled_ui(!into_open, |ui| {
                                ui.radio_value(&mut single, false, tr!("insert-mode-per-layer"))
                                    .on_disabled_hover_text(tr!("insert-mode-open-sketch"));
                            });
                        });
                    };
                    let mut place_card = |ui: &mut egui::Ui| {
                        egui::Grid::new("insert-dxf-offsets")
                            .num_columns(2)
                            .show(ui, |ui| {
                                for (i, label) in [tr!("insert-offset-x"), tr!("insert-offset-y")]
                                    .into_iter()
                                    .enumerate()
                                {
                                    ui.label(label.as_str());
                                    let out = FormulaField::new(
                                        ("insert-dxf-offset", i),
                                        &mut texts[i],
                                        ParamUnit::Length,
                                    )
                                    .width(OFFSET_WIDTH)
                                    .error_below(false)
                                    .label(label.as_str())
                                    .show(ui, &scope);
                                    if let Some(e) = out.error {
                                        field_errors.push(tr!(
                                            "field-error",
                                            label = label.as_str(),
                                            error = e
                                        ));
                                    }
                                    ui.end_row();
                                }
                            });
                        ui.add_space(theme::SPACE_2);
                        ui.strong(tr!("insert-layers"));
                        egui::ScrollArea::vertical()
                            .id_salt("insert-dxf-layers")
                            .max_height(LAYER_LIST_HEIGHT)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                for (i, layer) in l.import.layers.iter().enumerate() {
                                    if let Some(c) = layers.get_mut(i) {
                                        ui.checkbox(c, layer.name.as_str());
                                    }
                                }
                            });
                        ui.add_space(theme::SPACE_2);
                        ui.checkbox(&mut frame, tr!("insert-frame"));
                    };
                    let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                        &mut [&mut file_card, &mut place_card];
                    cards = card_row(ui, "insert-dxf", row);
                    for e in &field_errors {
                        ui.colored_label(error_color(dark), e);
                    }
                },
                |ui| {
                    if choosing {
                        footer_action = chooser.borrow_mut().show_footer(ui);
                        return;
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(ready, egui::Button::new(tr!("common-ok")))
                            .clicked()
                        {
                            ok = true;
                        }
                        if ui.button(tr!("insert-other-file")).clicked() {
                            other = true;
                        }
                        if ui.button(tr!("common-cancel")).clicked() {
                            close = true;
                        }
                    });
                },
            );
        for action in [body_action, footer_action] {
            match action {
                ChooserAction::Chosen(p) => chosen = Some(p),
                // Mit gelesener Datei zurück zu den Einstellungen, sonst Fenster schließen.
                ChooserAction::Cancel if has_loaded => back = true,
                ChooserAction::Cancel => close = true,
                ChooserAction::None => {}
            }
        }
        self.last_rect = response.map(|r| r.response.rect);
        self.last_cards = cards;
        if has_loaded && !choosing {
            self.units = units;
            self.single = single || into_open;
            self.frame = frame;
            self.layers = layers;
            for (i, t) in texts.iter().enumerate() {
                if let Ok(v) = eval_shown(t, ParamUnit::Length, &scope) {
                    self.offset[i] = v;
                }
            }
            self.offset_text = texts;
        }
        if loading.is_some() {
            ctx.request_repaint_after(REPAINT);
        }
        if cancel_read {
            self.cancel();
        }
        if other {
            self.choosing = true;
        }
        if back {
            self.choosing = false;
        }
        if let Some(p) = chosen {
            *last_dir = p.parent().map(Path::to_path_buf);
            self.load(p);
        }
        // Esc schließt (wie „Abbrechen“), solange kein Feld tippt.
        if !crate::viewport::typing(ctx) && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if ok && field_errors.is_empty() && self.insert(state).is_ok() {
            DxfDialog::finish(state);
            return DialogAction::Inserted;
        }
        if !open || close {
            DxfDialog::finish(state);
            return DialogAction::Closed;
        }
        self.update_preview(state);
        DialogAction::Open
    }
}

/// Hinweis nach dem Einfügen: Übersprungenes (alle Skizzen zusammen) und ersetzte Schriften.
/// `file` ist das, was die Datei selbst übersprang; jeder Bericht enthält es, dazu das beim
/// Einfügen Entartete.
fn hint(file: &Skipped, reports: &[InsertReport]) -> Option<String> {
    let mut skipped = *file;
    let mut fonts: Vec<String> = Vec::new();
    for r in reports {
        skipped.degenerate += r.skipped.degenerate.saturating_sub(file.degenerate);
        for f in &r.missing_fonts {
            if !fonts.contains(f) {
                fonts.push(f.clone());
            }
        }
    }
    let mut parts: Vec<String> = skipped_summary(&skipped)
        .map(|m| m.text())
        .into_iter()
        .collect();
    parts.extend(
        fonts
            .iter()
            .map(|f| tr!("sketch-text-hint-substituted", font = f.clone())),
    );
    (!parts.is_empty()).then(|| parts.join(" "))
}
