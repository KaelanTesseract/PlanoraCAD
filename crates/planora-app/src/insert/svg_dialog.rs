//! Fenster „SVG einfügen“ (Skizzen 9, Spec C): Datei wählen, im Hintergrund lesen, dann Ebene,
//! X- und Y-Abstand, Z-Winkel, Horizontal und Vertikal spiegeln, Skalierung und
//! Kontrollpunktrahmen. Die Felder folgen Fusion: Y-Abstand positiv nach unten, Z-Winkel positiv
//! im Uhrzeigersinn. Die Vorschau liegt als Linien in der Ansicht, am Anfang mit der
//! Hüllquader-Mitte auf dem Ursprung; Griffe darauf ([`crate::sketch_grips`]) verschieben, drehen
//! und skalieren, Feld und Griff bleiben in beiden Richtungen gleich. „OK“ fügt fest ein (alle
//! Punkte `Fixed`, wie Fusion), in einem Rückgängig-Schritt: in die offene Skizze oder in eine
//! neue Skizze. Abbrechen, Esc und ✕ hinterlassen nichts.

use super::pick::{InsertTarget, TargetPick};
use super::{BackgroundRead, DialogAction, svg_spec};
use crate::browser::error_color;
use crate::cards::card_row;
use crate::dialog::{Dialog, DialogSize};
use crate::export_dialog::{PROGRESS_WIDTH, REPAINT, file_name};
use crate::file_chooser::{ChooserAction, FileChooser};
use crate::formula_field::{FormulaField, eval_shown, value_field_text};
use crate::msg::Msg;
use crate::places::Places;
use crate::state::{AppState, InsertGrips, InsertPreview};
use crate::theme;
use planora_exchange::import2d::Import2d;
use planora_feature::ParamUnit;
use planora_i18n::tr;
use planora_math::{DVec2, Frame};
use planora_sketch::import::{InsertOptions, Placement, preview_polylines};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Breite der Zahlenfelder.
const FIELD_WIDTH: f32 = 120.0;
/// Abweichung der Vorschau-Linien, bezogen auf die Diagonale der Datei.
const PREVIEW_TOL_REL: f64 = 1e-3;
/// Einheit je Feld: X-Abstand, Y-Abstand, Z-Winkel, Skalierung.
const UNITS: [ParamUnit; 4] = [
    ParamUnit::Length,
    ParamUnit::Length,
    ParamUnit::Angle,
    ParamUnit::Number,
];

/// Gelesene Datei.
pub struct SvgLoaded {
    pub path: PathBuf,
    pub import: Arc<Import2d>,
}

pub struct SvgDialog {
    chooser: FileChooser,
    choosing: bool,
    reading: Option<BackgroundRead<Import2d>>,
    /// Nummer des Laufs, aus dem `loaded` stammt.
    loaded_run: u64,
    pub loaded: Option<SvgLoaded>,
    /// X- und Y-Abstand in mm; Y positiv nach unten, wie Fusion.
    pub offset: DVec2,
    /// Z-Winkel in Grad, positiv im Uhrzeigersinn, wie Fusion.
    pub angle_deg: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    /// Skalierung als Faktor (1 = Größe der Datei in mm).
    pub scale: f64,
    /// Kontrollpunktrahmen.
    pub frame: bool,
    /// Texte der Felder X-Abstand, Y-Abstand, Z-Winkel und Skalierung.
    pub texts: [String; 4],
    /// Zuletzt aus den Werten geschriebene oder übernommene Texte: Ein anderer Text ist eine
    /// Eingabe.
    shown: [String; 4],
    /// Werte zu `shown`: Ein anderer Wert (Griff, Code) schreibt den Text neu.
    shown_values: [f64; 4],
    pub target: TargetPick,
    /// Meldung im Fenster; wird beim Zeichnen übersetzt.
    pub error: Option<Msg>,
    preview_key: Option<(u64, Placement, Frame)>,
    /// Rechteck des Fensters im letzten Bild (Tests der vollen Breite).
    pub last_rect: Option<egui::Rect>,
    /// Rechtecke der Karten im letzten Bild (Tests der gleichen Höhe).
    pub last_cards: Vec<egui::Rect>,
}

impl SvgDialog {
    /// Neues Fenster im Ordner `dir`; Ziel nach dem Zustand (offene Skizze, Auswahl).
    pub fn new(dir: PathBuf, state: &AppState) -> SvgDialog {
        let mut d = SvgDialog {
            chooser: FileChooser::open(dir, vec!["svg", "svgz"]),
            choosing: true,
            reading: None,
            loaded_run: 0,
            loaded: None,
            offset: DVec2::ZERO,
            angle_deg: 0.0,
            flip_h: false,
            flip_v: false,
            scale: 1.0,
            frame: false,
            texts: Default::default(),
            shown: Default::default(),
            shown_values: [f64::NAN; 4],
            target: TargetPick::new(state),
            error: None,
            preview_key: None,
            last_rect: None,
            last_cards: Vec::new(),
        };
        d.refresh_texts();
        d
    }

    /// Fügt in die offene Skizze ein (dann entfällt das Wählfeld)?
    pub fn into_open(&self) -> bool {
        self.target.into_open()
    }

    /// Liest `path` im Hintergrund; ein noch laufendes Lesen wird abgebrochen.
    pub fn load(&mut self, path: PathBuf) {
        if let Some(r) = self.reading.as_mut() {
            r.cancel();
        }
        self.error = None;
        self.reading = Some(BackgroundRead::start(path, svg_spec()));
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
                self.loaded_run = r.run();
                self.loaded = Some(SvgLoaded {
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

    /// Lage der Geometrie mit dem Hüllquader `bounds` der Datei: Mitte auf den Ursprung, dann
    /// spiegeln, skalieren, drehen und verschieben. Die Felder (Y nach unten, Winkel im
    /// Uhrzeigersinn) werden hier in die Skizze (Y nach oben, gegen den Uhrzeigersinn) gedreht.
    pub fn placement(&self, bounds: (DVec2, DVec2)) -> Placement {
        let sign = |flip: bool| if flip { -1.0 } else { 1.0 };
        Placement {
            scale: DVec2::new(sign(self.flip_h), sign(self.flip_v)) * self.scale,
            angle: self.grip_angle(),
            offset: self.grip_offset(),
            pivot: (bounds.0 + bounds.1) * 0.5,
        }
    }

    /// Lage mit dem Hüllquader der gelesenen Datei (ohne Datei oder Geometrie um den Ursprung).
    pub fn current_placement(&self) -> Placement {
        let bounds = self
            .loaded
            .as_ref()
            .and_then(|l| l.import.bounds())
            .unwrap_or((DVec2::ZERO, DVec2::ZERO));
        self.placement(bounds)
    }

    /// Mitte der Vorschau in der Skizze (Y nach oben).
    fn grip_offset(&self) -> DVec2 {
        DVec2::new(self.offset.x, -self.offset.y)
    }

    /// Winkel in der Skizze (Radiant, gegen den Uhrzeigersinn).
    fn grip_angle(&self) -> f64 {
        -self.angle_deg.to_radians()
    }

    /// Übernimmt die Lage eines Griffzugs (Skizze: Y nach oben, gegen den Uhrzeigersinn) in die
    /// Felder.
    pub fn set_from_grips(&mut self, offset: DVec2, angle: f64, factor: f64) {
        self.offset = DVec2::new(offset.x, -offset.y);
        self.angle_deg = -angle.to_degrees();
        self.scale = factor;
        self.refresh_texts();
    }

    fn values(&self) -> [f64; 4] {
        [self.offset.x, self.offset.y, self.angle_deg, self.scale]
    }

    fn set_value(&mut self, i: usize, v: f64) {
        match i {
            0 => self.offset.x = v,
            1 => self.offset.y = v,
            2 => self.angle_deg = v,
            _ => self.scale = v,
        }
    }

    /// Schreibt die Texte der Felder neu, deren Wert sich seit dem letzten Text geändert hat.
    fn refresh_texts(&mut self) {
        for (i, v) in self.values().into_iter().enumerate() {
            if v.to_bits() != self.shown_values[i].to_bits() {
                let t = value_field_text(v, UNITS[i]);
                self.texts[i] = t.clone();
                self.shown[i] = t;
                self.shown_values[i] = v;
            }
        }
    }

    /// Übernimmt geänderte Texte in die Werte; Fehler als Text je Feld.
    fn take_texts(&mut self, scope: &planora_feature::ParamScope) -> Vec<String> {
        let labels = field_labels();
        let mut errors = Vec::new();
        for i in 0..4 {
            if self.texts[i] == self.shown[i] {
                continue;
            }
            let result = eval_shown(&self.texts[i], UNITS[i], scope).and_then(|v| {
                if i == 3 && !(v.is_finite() && v > 0.0) {
                    Err(tr!("insert-scale-positive"))
                } else {
                    Ok(v)
                }
            });
            match result {
                Ok(v) => {
                    self.set_value(i, v);
                    self.shown[i] = self.texts[i].clone();
                    self.shown_values[i] = v;
                }
                Err(e) => errors.push(tr!("field-error", label = labels[i].as_str(), error = e)),
            }
        }
        errors
    }

    fn options(&self) -> InsertOptions {
        InsertOptions {
            fixed: true,
            show_frame: self.frame,
            layers: Vec::new(),
        }
    }

    /// „OK“: fügt ein (ein Rückgängig-Schritt). Danach steht der Hinweis zum Übersprungenen in
    /// `state.info`. Bei einem Fehler bleibt alles unverändert und die Meldung steht im Fenster.
    pub fn insert(&mut self, state: &mut AppState) -> Result<(), String> {
        match self.try_insert(state) {
            Ok(report) => {
                self.error = None;
                state.message = None;
                let file = self
                    .loaded
                    .as_ref()
                    .map(|l| l.import.skipped)
                    .unwrap_or_default();
                let mut parts: Vec<String> = super::dxf_dialog::hint(&file, &[report])
                    .into_iter()
                    .collect();
                if file.texts > 0 {
                    parts.push(tr!("insert-svg-text-hint"));
                }
                state.info = (!parts.is_empty()).then(|| parts.join(" "));
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

    fn try_insert(
        &self,
        state: &mut AppState,
    ) -> Result<planora_sketch::import::InsertReport, Msg> {
        let Some(l) = &self.loaded else {
            return Err(crate::msg!("import-choose-file"));
        };
        if !(self.scale.is_finite() && self.scale > 0.0) {
            return Err(crate::msg!("insert-scale-positive"));
        }
        let Some(target) = self.target.target() else {
            return Err(crate::msg!("insert-target-empty"));
        };
        let place = self.current_placement();
        let opt = self.options();
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
                    .insert_geometry(&l.import, &place, &opt, &shaper)
                    .map_err(|e| Msg::Raw(e.to_string()))?;
                se.last_report = Some(sk.clone().solve());
                se.edit_label = Some(crate::msg!("insert-svg-undo"));
                Ok(report)
            }
            InsertTarget::Plane(plane) => {
                let Some(frame) = self.target.frame(state) else {
                    return Err(crate::msg!("insert-target-empty"));
                };
                state
                    .add_filled_sketch(plane, frame, None, |sk| {
                        sk.insert_geometry(&l.import, &place, &opt, &shaper)
                            .map_err(|e| e.to_string())
                    })
                    .map(|(_, report)| report)
                    .map_err(Msg::Raw)
            }
        }
    }

    /// Rechnet die Vorschau neu, wenn sich etwas geändert hat, legt sie in den Zustand und
    /// schreibt die Griffe (ein laufender Zug bleibt).
    fn update_preview(&mut self, state: &mut AppState) {
        let (Some(l), Some(frame)) = (&self.loaded, self.target.frame(state)) else {
            self.preview_key = None;
            state.insert_preview = None;
            state.insert_grips = None;
            return;
        };
        let place = self.current_placement();
        let (hot, drag) = state
            .insert_grips
            .as_ref()
            .filter(|g| g.frame == frame)
            .map_or((None, None), |g| (g.hot, g.drag));
        state.insert_grips = Some(InsertGrips {
            frame,
            offset: self.grip_offset(),
            angle: self.grip_angle(),
            factor: self.scale,
            hot,
            drag,
            moved: false,
            esc_taken: false,
        });
        let key = (self.loaded_run, place, frame);
        if self.preview_key.as_ref() == Some(&key) && state.insert_preview.is_some() {
            return;
        }
        let diag = l
            .import
            .bounds()
            .map_or(1.0, |(lo, hi)| (hi - lo).length() * self.scale);
        let tol = (diag * PREVIEW_TOL_REL).max(1e-6);
        let lines = preview_polylines(&l.import, &place, &[], tol);
        state.insert_preview = Some(InsertPreview { frame, lines });
        self.preview_key = Some(key);
    }

    /// Räumt Wahlmodus, Vorschau und Griffe ab (Fenster geschlossen, auf jedem Weg).
    pub fn finish(state: &mut AppState) {
        TargetPick::finish(state);
        state.insert_preview = None;
        state.insert_grips = None;
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
            SvgDialog::finish(state);
            return DialogAction::Closed;
        }
        self.tick(false);
        self.target.sync(state);
        // Hat Esc in diesem Bild nur den Griffzug abgebrochen? (Die Ansicht läuft vorher.)
        let esc_taken = state.insert_grips.as_ref().is_some_and(|g| g.esc_taken);
        // Griff → Feld: ein Zug in der Ansicht seit dem letzten Bild.
        if let Some(g) = state.insert_grips.as_mut()
            && g.moved
        {
            g.moved = false;
            let (o, a, f) = (g.offset, g.angle, g.factor);
            self.set_from_grips(o, a, f);
        }
        self.refresh_texts();
        let (mut open, mut close, mut cancel_read, mut ok, mut other, mut back) =
            (true, false, false, false, false, false);
        let mut chosen = None;
        let mut cards = Vec::new();
        let loading = self.is_loading().then(|| self.progress());
        let choosing = self.choosing || self.loaded.is_none();
        let error = self.error_text();
        let pick_error = self.target.error.as_ref().map(Msg::text);
        let has_loaded = self.loaded.is_some();
        let scope = state.document.param_scope();
        let empty = self
            .loaded
            .as_ref()
            .and_then(|l| super::nothing_usable(&l.import));
        let ready = has_loaded && empty.is_none() && self.target.target().is_some();
        let chooser = RefCell::new(&mut self.chooser);
        let (mut body_action, mut footer_action) = (ChooserAction::None, ChooserAction::None);
        let (mut flip_h, mut flip_v, mut frame) = (self.flip_h, self.flip_v, self.frame);
        let mut texts = self.texts.clone();
        let mut field_errors: Vec<String> = Vec::new();
        let target = &mut self.target;
        let loaded = &self.loaded;
        let labels = field_labels();
        let response = Dialog::new(tr!("insert-svg-title"), DialogSize::File)
            .dock_key("insert_svg")
            .center_on_open()
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| {
                    // Abgedockt läuft der Inhalt mehrmals je Aufruf (Durchgänge des Kindfensters): neu sammeln.
                    field_errors.clear();
                    let dark = ui.visuals().dark_mode;
                    if choosing {
                        ui.strong(tr!("insert-svg-choose"));
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
                        ui.weak(tr!("insert-svg-grips-hint"));
                        if let Some(why) = &empty {
                            ui.colored_label(error_color(dark), why);
                        }
                    };
                    let mut place_card = |ui: &mut egui::Ui| {
                        egui::Grid::new("insert-svg-fields")
                            .num_columns(2)
                            .show(ui, |ui| {
                                for (i, label) in labels.iter().enumerate() {
                                    ui.label(label.as_str());
                                    let out = FormulaField::new(
                                        ("insert-svg-field", i),
                                        &mut texts[i],
                                        UNITS[i],
                                    )
                                    .width(FIELD_WIDTH)
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
                        ui.checkbox(&mut flip_h, tr!("insert-flip-h"));
                        ui.checkbox(&mut flip_v, tr!("insert-flip-v"));
                        ui.checkbox(&mut frame, tr!("insert-frame"));
                    };
                    let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                        &mut [&mut file_card, &mut place_card];
                    cards = card_row(ui, "insert-svg", row);
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
            self.flip_h = flip_h;
            self.flip_v = flip_v;
            self.frame = frame;
            self.texts = texts;
        }
        // Feld → Griff: geänderte Texte werden Werte, die Griffe folgen unten.
        let take_errors = self.take_texts(&scope);
        for e in take_errors {
            if !field_errors.contains(&e) {
                field_errors.push(e);
            }
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
        // Esc schließt (wie „Abbrechen“), solange kein Feld tippt und Esc nicht eben nur einen
        // Griffzug abgebrochen hat.
        if !esc_taken
            && !crate::viewport::typing(ctx)
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            // Verbraucht: Esc wirkt nur hier, nicht auch noch anderswo.
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            close = true;
        }
        if ok && field_errors.is_empty() && self.insert(state).is_ok() {
            SvgDialog::finish(state);
            return DialogAction::Inserted;
        }
        if !open || close {
            SvgDialog::finish(state);
            return DialogAction::Closed;
        }
        self.update_preview(state);
        DialogAction::Open
    }
}

/// Namen der Felder X-Abstand, Y-Abstand, Z-Winkel und Skalierung.
fn field_labels() -> [String; 4] {
    [
        tr!("insert-offset-x"),
        tr!("insert-offset-y"),
        tr!("insert-angle-z"),
        tr!("insert-scale"),
    ]
}
