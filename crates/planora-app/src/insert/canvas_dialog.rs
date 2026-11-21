//! Fenster „Leinwand“ (Skizzen 9, Spec C): Bild wählen (PNG, JPG, TIFF; die Endung muss zum
//! Inhalt passen), im Hintergrund lesen, dann Ebene oder ebene Fläche, Deckkraft (Vorgabe 50),
//! Durch Objekte anzeigen und Auswählbar (Vorgabe an), X- und Y-Abstand, Z-Winkel, Skalierung XY
//! oder getrennt X/Y, Horizontal und Vertikal spiegeln. Die Felder folgen dem SVG-Fenster: Y
//! positiv nach unten, Z-Winkel positiv im Uhrzeigersinn. Startgröße 1 px = 1 mm. Griffe
//! ([`crate::sketch_grips`]) verschieben, drehen und skalieren die Vorschau.
//!
//! „OK“ legt die Leinwand an der Verlaufsmarke an (ein Rückgängig-Schritt); beim Bearbeiten
//! einer vorhandenen Leinwand ist es ebenfalls ein Schritt. Abbrechen, Esc und ✕ hinterlassen
//! nichts. Solange eine Skizze offen ist, gibt es das Fenster nicht („Erst die Skizze beenden“).
//! „Renderfähig“ entfällt (kein Render-Bereich).

use super::pick::{InsertTarget, TargetPick};
use super::{BackgroundRead, DialogAction, ReadSpec};
use crate::browser::error_color;
use crate::canvas::{CanvasPreview, GPU_MAX_EDGE};
use crate::cards::card_row;
use crate::dialog::{Dialog, DialogSize};
use crate::export_dialog::{PROGRESS_WIDTH, REPAINT, file_name};
use crate::file_chooser::{ChooserAction, FileChooser};
use crate::formula_field::{FormulaField, eval_shown, value_field_text};
use crate::msg::Msg;
use crate::places::Places;
use crate::state::{AppState, InsertGrips};
use crate::theme;
use planora_exchange::{ExchangeError, MAX_IMAGE_BYTES, decode_rgba, probe_image};
use planora_feature::{
    CanvasBase, CanvasFeature, CanvasImage, FeatureId, FeatureKind, ImageId, ParamUnit,
    SketchPlane, image_hash,
};
use planora_i18n::tr;
use planora_math::{DVec2, Frame};
use planora_render::CanvasTexture;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Breite der Zahlenfelder.
const FIELD_WIDTH: f32 = 120.0;
/// Einheit je Feld: X-Abstand, Y-Abstand, Z-Winkel, Skalierung X (oder XY), Skalierung Y.
const UNITS: [ParamUnit; 5] = [
    ParamUnit::Length,
    ParamUnit::Length,
    ParamUnit::Angle,
    ParamUnit::Number,
    ParamUnit::Number,
];
/// Endungen der Bilddateien.
const EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "tif", "tiff"];

/// Ergebnis des Lesens im Hintergrund: die Bytes der Datei, ihre Prüfsumme und das schon für die
/// GPU entschlüsselte Bild (höchstens 4096 px Kante).
pub struct ImageBytes {
    pub bytes: Arc<[u8]>,
    pub hash: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

/// Leser der Bilddateien: Grenze [`MAX_IMAGE_BYTES`]. Prüft Format und Kantenlänge
/// ([`probe_image`] mit dem Format des Inhalts) und entschlüsselt einmal für die Ansicht; die
/// Endung prüft das Fenster danach gegen den Inhalt.
pub fn image_spec() -> ReadSpec<ImageBytes> {
    ReadSpec {
        max_bytes: MAX_IMAGE_BYTES,
        too_large: ExchangeError::ImageTooLarge,
        read: read_image,
        broken: ExchangeError::ImageBroken,
    }
}

fn read_image(bytes: &[u8], cancelled: &dyn Fn() -> bool) -> Result<ImageBytes, ExchangeError> {
    // Ohne passende Endung prüft `probe_image` nur, ob es überhaupt ein Bild ist.
    let ext = ["png", "jpg", "tif"]
        .into_iter()
        .find(|e| {
            !matches!(
                probe_image(e, bytes),
                Err(ExchangeError::ImageWrongExtension)
            )
        })
        .unwrap_or("png");
    probe_image(ext, bytes)?;
    if cancelled() {
        return Err(ExchangeError::Cancelled);
    }
    let (width, height, rgba) = decode_rgba(bytes, GPU_MAX_EDGE)?;
    Ok(ImageBytes {
        bytes: Arc::from(bytes),
        hash: image_hash(bytes),
        width,
        height,
        rgba: Arc::from(rgba),
    })
}

/// Gelesenes Bild.
pub struct CanvasLoaded {
    pub path: Option<PathBuf>,
    pub image: Arc<CanvasImage>,
    /// Für die Ansicht schon entschlüsselt (beim Bearbeiten `None`: der Zwischenspeicher hat es).
    texture: Option<CanvasTexture>,
}

pub struct CanvasDialog {
    /// Bearbeitete Leinwand; `None` beim Einfügen.
    pub edit: Option<FeatureId>,
    chooser: FileChooser,
    choosing: bool,
    reading: Option<BackgroundRead<ImageBytes>>,
    pub loaded: Option<CanvasLoaded>,
    /// Deckkraft in Prozent (0..=100).
    pub opacity: u8,
    pub display_through: bool,
    pub selectable: bool,
    /// X- und Y-Abstand in mm; Y positiv nach unten, wie im SVG-Fenster.
    pub offset: DVec2,
    /// Z-Winkel in Grad, positiv im Uhrzeigersinn.
    pub angle_deg: f64,
    /// Skalierung XY (Seitenverhältnis bleibt): `Some` mit dem Wert des Felds, das X und Y
    /// gemeinsam ändert; `None`, wenn X und Y getrennt eingegeben werden.
    pub scale_xy: Option<f64>,
    /// Maßstab je Achse (1 = 1 px auf 1 mm).
    pub scale: DVec2,
    pub flip_h: bool,
    pub flip_v: bool,
    /// Texte der Felder X-Abstand, Y-Abstand, Z-Winkel, Skalierung X (oder XY), Skalierung Y.
    pub texts: [String; 5],
    shown: [String; 5],
    shown_values: [f64; 5],
    pub target: TargetPick,
    /// Meldung im Fenster; wird beim Zeichnen übersetzt.
    pub error: Option<Msg>,
    /// Rechteck des Fensters im letzten Bild.
    pub last_rect: Option<egui::Rect>,
    /// Rechtecke der Karten im letzten Bild.
    pub last_cards: Vec<egui::Rect>,
}

/// Träger einer Leinwand aus der gewählten Ebene.
pub fn base_of(plane: &SketchPlane) -> CanvasBase {
    match plane {
        SketchPlane::Standard(p) => CanvasBase::Standard(*p),
        SketchPlane::OnFace(r) => CanvasBase::Face(r.clone()),
        SketchPlane::OnPlane(id) => CanvasBase::Plane(*id),
    }
}

fn plane_of(base: &CanvasBase) -> SketchPlane {
    match base {
        CanvasBase::Standard(p) => SketchPlane::Standard(*p),
        CanvasBase::Face(r) => SketchPlane::OnFace(r.clone()),
        CanvasBase::Plane(id) => SketchPlane::OnPlane(*id),
    }
}

/// Koordinatensystem des Trägers aus der berechneten Lage `placed` der Leinwand `c` (Umkehrung
/// von [`CanvasFeature::placed_frame`]); `None` bei Maßstab 0.
fn base_frame(c: &CanvasFeature, placed: &Frame) -> Option<Frame> {
    let sign = |flip: bool| if flip { -1.0 } else { 1.0 };
    let sx = c.scale.x * sign(c.flip_h);
    let sy = c.scale.y * sign(c.flip_v);
    if sx == 0.0 || sy == 0.0 {
        return None;
    }
    let (s, co) = c.angle.sin_cos();
    let u = placed.x / sx;
    let w = placed.y / sy;
    let x = u * co - w * s;
    let y = u * s + w * co;
    Some(Frame {
        origin: placed.origin - x * c.offset.x - y * c.offset.y,
        x,
        y,
        z: placed.z,
    })
}

impl CanvasDialog {
    fn blank(dir: PathBuf, target: TargetPick) -> CanvasDialog {
        // Vorgaben wie in Fusion (Deckkraft 50, Durch Objekte anzeigen und Auswählbar an,
        // 1 px = 1 mm) aus dem Modell.
        let d = CanvasFeature::new(
            ImageId(0),
            CanvasBase::Standard(planora_feature::StandardPlane::XY),
        );
        let mut dialog = CanvasDialog {
            edit: None,
            chooser: FileChooser::open(dir, EXTENSIONS.to_vec()),
            choosing: true,
            reading: None,
            loaded: None,
            opacity: d.opacity,
            display_through: d.display_through,
            selectable: d.selectable,
            offset: DVec2::ZERO,
            angle_deg: 0.0,
            scale_xy: Some(d.scale.x),
            scale: d.scale,
            flip_h: false,
            flip_v: false,
            texts: Default::default(),
            shown: Default::default(),
            shown_values: [f64::NAN; 5],
            target,
            error: None,
            last_rect: None,
            last_cards: Vec::new(),
        };
        dialog.refresh_texts();
        dialog
    }

    /// Neues Fenster „Leinwand“ im Ordner `dir`; Ziel nach der Auswahl.
    pub fn new(dir: PathBuf, state: &AppState) -> CanvasDialog {
        CanvasDialog::blank(dir, TargetPick::new(state))
    }

    /// Fenster „Leinwand bearbeiten“ mit den Werten der Leinwand `id`; `None` für etwas anderes.
    pub fn edit(dir: PathBuf, state: &AppState, id: FeatureId) -> Option<CanvasDialog> {
        let FeatureKind::Canvas(c) = &state.document.feature(id)?.kind else {
            return None;
        };
        let image = state.document.image(c.image)?.clone();
        let frame = state
            .document
            .canvas_frame(id)
            .and_then(|f| base_frame(c, &f))
            .or(c.last_base);
        let target = match frame {
            Some(f) => TargetPick::with_plane(state, plane_of(&c.base), f),
            None => TargetPick::new(state),
        };
        let mut d = CanvasDialog::blank(dir, target);
        d.edit = Some(id);
        d.choosing = false;
        d.loaded = Some(CanvasLoaded {
            path: None,
            image,
            texture: None,
        });
        d.opacity = c.opacity;
        d.display_through = c.display_through;
        d.selectable = c.selectable;
        d.offset = DVec2::new(c.offset.x, -c.offset.y);
        d.angle_deg = -c.angle.to_degrees();
        d.scale = c.scale;
        d.scale_xy = (c.scale.x == c.scale.y).then_some(c.scale.x);
        d.flip_h = c.flip_h;
        d.flip_v = c.flip_v;
        d.refresh_texts();
        Some(d)
    }

    /// Liest `path` im Hintergrund; ein noch laufendes Lesen wird abgebrochen.
    pub fn load(&mut self, path: PathBuf) {
        if let Some(r) = self.reading.as_mut() {
            r.cancel();
        }
        self.error = None;
        self.reading = Some(BackgroundRead::start(path, image_spec()));
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

    pub fn error_text(&self) -> Option<String> {
        self.error.as_ref().map(Msg::text)
    }

    /// Holt das Ergebnis des Lesens ab; mit `block` wird gewartet (Tests). Passt die Endung
    /// nicht zum Inhalt, bleibt das Fenster mit der Meldung offen.
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
        let path = r.path().to_path_buf();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_string();
        match result.map(|res| {
            res.and_then(|b| {
                super::guarded(ExchangeError::ImageBroken, || probe_image(&ext, &b.bytes))
                    .map(|i| (b, i))
            })
        }) {
            Some(Ok((b, info))) => {
                let image = Arc::new(CanvasImage {
                    bytes: b.bytes,
                    format: info.format,
                    width: info.width,
                    height: info.height,
                    hash: b.hash,
                });
                self.loaded = Some(CanvasLoaded {
                    path: Some(path),
                    image,
                    texture: Some(CanvasTexture {
                        key: b.hash,
                        width: b.width,
                        height: b.height,
                        rgba: b.rgba,
                    }),
                });
                self.choosing = false;
                self.error = None;
            }
            Some(Err(e)) => self.error = Some(Msg::Raw(e.to_string())),
            None => {}
        }
    }

    /// Versatz in der Ebene (Y nach oben).
    fn grip_offset(&self) -> DVec2 {
        DVec2::new(self.offset.x, -self.offset.y)
    }

    /// Winkel in der Ebene (Radiant, gegen den Uhrzeigersinn).
    fn grip_angle(&self) -> f64 {
        -self.angle_deg.to_radians()
    }

    /// Die Leinwand nach den Feldern auf dem Träger `base` (Bild noch ohne Kennung).
    pub fn feature(&self, base: CanvasBase) -> CanvasFeature {
        let mut f = CanvasFeature::new(ImageId(0), base);
        f.offset = self.grip_offset();
        f.angle = self.grip_angle();
        f.scale = self.scale;
        f.flip_h = self.flip_h;
        f.flip_v = self.flip_v;
        f.opacity = self.opacity.min(100);
        f.display_through = self.display_through;
        f.selectable = self.selectable;
        f
    }

    /// Übernimmt die Lage eines Griffzugs (Ebene: Y nach oben, gegen den Uhrzeigersinn) in die
    /// Felder. Der Faktor gilt für X; Y folgt im selben Verhältnis (das Seitenverhältnis bleibt).
    pub fn set_from_grips(&mut self, offset: DVec2, angle: f64, factor: f64) {
        self.offset = DVec2::new(offset.x, -offset.y);
        self.angle_deg = -angle.to_degrees();
        if factor.is_finite() && factor > 0.0 && self.scale.x > 0.0 {
            self.scale *= factor / self.scale.x;
        }
        if self.scale_xy.is_some() {
            self.scale_xy = Some(self.scale.x);
        }
        self.refresh_texts();
    }

    /// Schaltet zwischen Skalierung XY (gemeinsam) und getrennt X/Y um.
    pub fn set_scale_locked(&mut self, locked: bool) {
        self.scale_xy = locked.then_some(self.scale.x);
        // Die Felder heißen danach anders: ihre Texte neu schreiben.
        self.shown_values[3] = f64::NAN;
        self.shown_values[4] = f64::NAN;
        self.refresh_texts();
    }

    fn values(&self) -> [f64; 5] {
        [
            self.offset.x,
            self.offset.y,
            self.angle_deg,
            self.scale.x,
            self.scale.y,
        ]
    }

    fn set_value(&mut self, i: usize, v: f64) {
        match i {
            0 => self.offset.x = v,
            1 => self.offset.y = v,
            2 => self.angle_deg = v,
            3 if self.scale_xy.is_some() => {
                // XY: beide Achsen im selben Verhältnis.
                if self.scale.x > 0.0 {
                    self.scale *= v / self.scale.x;
                } else {
                    self.scale = DVec2::splat(v);
                }
                self.scale_xy = Some(self.scale.x);
            }
            3 => self.scale.x = v,
            _ => self.scale.y = v,
        }
    }

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

    /// Felder, die gerade zu sehen sind (bei XY fehlt „Skalierung Y“).
    fn field_count(&self) -> usize {
        if self.scale_xy.is_some() { 4 } else { 5 }
    }

    fn take_texts(&mut self, scope: &planora_feature::ParamScope) -> Vec<String> {
        let labels = self.field_labels();
        let mut errors = Vec::new();
        for i in 0..self.field_count() {
            if self.texts[i] == self.shown[i] {
                continue;
            }
            let result = eval_shown(&self.texts[i], UNITS[i], scope).and_then(|v| {
                if i >= 3 && !(v.is_finite() && v > 0.0) {
                    Err(tr!("insert-scale-positive"))
                } else {
                    Ok(v)
                }
            });
            match result {
                Ok(v) => {
                    self.set_value(i, v);
                    self.shown[i] = self.texts[i].clone();
                    self.shown_values[i] = self.values()[i];
                    // XY ändert auch Y: dessen Text folgt.
                    self.refresh_texts();
                }
                Err(e) => errors.push(tr!("field-error", label = labels[i].as_str(), error = e)),
            }
        }
        errors
    }

    /// Namen der Felder.
    pub fn field_labels(&self) -> Vec<String> {
        let mut v = vec![
            tr!("insert-offset-x"),
            tr!("insert-offset-y"),
            tr!("insert-angle-z"),
        ];
        if self.scale_xy.is_some() {
            v.push(tr!("insert-canvas-scale-xy"));
        } else {
            v.push(tr!("insert-canvas-scale-x"));
            v.push(tr!("insert-canvas-scale-y"));
        }
        v
    }

    /// „OK“: legt die Leinwand an (oder ändert die bearbeitete), ein Rückgängig-Schritt. Bei
    /// einem Fehler bleibt alles unverändert und die Meldung steht im Fenster.
    pub fn insert(&mut self, state: &mut AppState) -> Result<FeatureId, String> {
        match self.try_insert(state) {
            Ok(id) => {
                self.error = None;
                Ok(id)
            }
            Err(e) => {
                let text = e.text();
                self.error = Some(e);
                Err(text)
            }
        }
    }

    fn try_insert(&self, state: &mut AppState) -> Result<FeatureId, Msg> {
        let Some(l) = &self.loaded else {
            return Err(crate::msg!("insert-canvas-choose-file"));
        };
        if !(self.scale.x.is_finite()
            && self.scale.x > 0.0
            && self.scale.y.is_finite()
            && self.scale.y > 0.0)
        {
            return Err(crate::msg!("insert-scale-positive"));
        }
        if let Some(why) = state.canvas_lock() {
            return Err(Msg::Raw(why));
        }
        let Some(InsertTarget::Plane(plane)) = self.target.target() else {
            return Err(crate::msg!("insert-target-empty"));
        };
        let f = self.feature(base_of(&plane));
        match self.edit {
            Some(id) => state.update_canvas(id, f).map(|()| id).map_err(Msg::Raw),
            None => state.insert_canvas((*l.image).clone(), f).map_err(Msg::Raw),
        }
    }

    /// Legt Vorschau und Griffe in den Zustand (ein laufender Zug bleibt).
    fn update_preview(&mut self, state: &mut AppState) {
        let (Some(l), Some(frame), Some(InsertTarget::Plane(plane))) = (
            &mut self.loaded,
            self.target.frame(state),
            self.target.target(),
        ) else {
            state.canvas_preview = None;
            state.insert_grips = None;
            return;
        };
        if let Some(t) = l.texture.take()
            && !state.canvas_textures.contains(t.key)
        {
            state.canvas_textures.insert(t);
        }
        let image = l.image.clone();
        let (hot, drag) = state
            .insert_grips
            .as_ref()
            .filter(|g| g.frame == frame)
            .map_or((None, None), |g| (g.hot, g.drag));
        state.insert_grips = Some(InsertGrips {
            frame,
            offset: self.grip_offset(),
            angle: self.grip_angle(),
            factor: self.scale.x,
            hot,
            drag,
            moved: false,
            esc_taken: false,
        });
        state.canvas_preview = Some(CanvasPreview {
            image,
            feature: self.feature(base_of(&plane)),
            base: frame,
            edit: self.edit,
        });
    }

    /// Räumt Wahlmodus, Vorschau und Griffe ab (Fenster geschlossen, auf jedem Weg).
    pub fn finish(state: &mut AppState) {
        TargetPick::finish(state);
        state.canvas_preview = None;
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
        // Eine Skizze wurde geöffnet oder die bearbeitete Leinwand ist weg: Das Fenster gilt
        // nicht mehr (Werkzeugwechsel hinterlässt nichts).
        if state.canvas_lock().is_some() || self.edit.is_some_and(|id| !state.is_canvas(id)) {
            CanvasDialog::finish(state);
            return DialogAction::Closed;
        }
        self.tick(false);
        self.target.sync(state);
        let esc_taken = state.insert_grips.as_ref().is_some_and(|g| g.esc_taken);
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
        // Ein Feld, das mit Enter verlassen wird, übernimmt (wie Fusion).
        let field_enter = std::cell::Cell::new(false);
        let error = self.error_text();
        let pick_error = self.target.error.as_ref().map(Msg::text);
        let has_loaded = self.loaded.is_some();
        let editing = self.edit.is_some();
        let scope = state.document.param_scope();
        let ready = has_loaded && self.target.target().is_some();
        let labels = self.field_labels();
        let chooser = RefCell::new(&mut self.chooser);
        let (mut body_action, mut footer_action) = (ChooserAction::None, ChooserAction::None);
        let (mut flip_h, mut flip_v) = (self.flip_h, self.flip_v);
        let (mut through, mut selectable, mut opacity) =
            (self.display_through, self.selectable, self.opacity);
        let mut locked = self.scale_xy.is_some();
        let mut texts = self.texts.clone();
        let mut field_errors: Vec<String> = Vec::new();
        let target = &mut self.target;
        let loaded = &self.loaded;
        let title = if editing {
            tr!("insert-canvas-edit-title")
        } else {
            tr!("insert-canvas-title")
        };
        let response = Dialog::new(title, DialogSize::File)
            .dock_key("canvas")
            .center_on_open()
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| {
                    // Abgedockt läuft der Inhalt mehrmals je Aufruf (Durchgänge des Kindfensters): neu sammeln.
                    field_errors.clear();
                    let dark = ui.visuals().dark_mode;
                    if choosing {
                        ui.strong(tr!("insert-canvas-choose"));
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
                        ui.strong(tr!("insert-canvas-image"));
                        let name = l.path.as_deref().map_or_else(
                            || tr!("insert-canvas-size", w = l.image.width, h = l.image.height),
                            file_name,
                        );
                        ui.add(egui::Label::new(name).wrap());
                        ui.add_space(theme::SPACE_2);
                        ui.strong(tr!("insert-target"));
                        target.show(ui, state);
                        ui.add_space(theme::SPACE_2);
                        ui.label(tr!("insert-canvas-opacity"));
                        ui.add(egui::Slider::new(&mut opacity, 0..=100).suffix(" %"));
                        ui.checkbox(&mut through, tr!("insert-canvas-display-through"));
                        ui.checkbox(&mut selectable, tr!("insert-canvas-selectable"));
                        ui.add_space(theme::SPACE_2);
                        ui.weak(tr!("insert-svg-grips-hint"));
                    };
                    let mut place_card = |ui: &mut egui::Ui| {
                        egui::Grid::new("insert-canvas-fields")
                            .num_columns(2)
                            .show(ui, |ui| {
                                for (i, label) in labels.iter().enumerate() {
                                    ui.label(label.as_str());
                                    let out = FormulaField::new(
                                        ("insert-canvas-field", i),
                                        &mut texts[i],
                                        UNITS[i],
                                    )
                                    .width(FIELD_WIDTH)
                                    .error_below(false)
                                    .label(label.as_str())
                                    .show(ui, &scope);
                                    if crate::dialog::left_with_enter(ui, &out.response) {
                                        field_enter.set(true);
                                    }
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
                        ui.checkbox(&mut locked, tr!("insert-canvas-keep-aspect"));
                        ui.add_space(theme::SPACE_2);
                        ui.checkbox(&mut flip_h, tr!("insert-flip-h"));
                        ui.checkbox(&mut flip_v, tr!("insert-flip-v"));
                    };
                    let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                        &mut [&mut file_card, &mut place_card];
                    cards = card_row(ui, "insert-canvas", row);
                    for e in &field_errors {
                        ui.colored_label(error_color(dark), e);
                    }
                },
                |ui| {
                    // Esc schließt (wie „Abbrechen“), solange kein Feld tippt und Esc nicht eben
                    // nur einen Griffzug abgebrochen hat; im Fenster des Dialogs gelesen
                    // (abgedockt auch Esc im Hauptfenster).
                    if !esc_taken && crate::dialog::escape_pressed(ui, false) {
                        close = true;
                    }
                    if choosing {
                        footer_action = chooser.borrow_mut().show_footer(ui);
                        return;
                    }
                    ok |= crate::dialog::enter_confirms(ui, field_enter.get(), ready);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(ready, egui::Button::new(tr!("common-ok")))
                            .clicked()
                        {
                            ok = true;
                        }
                        if !editing && ui.button(tr!("insert-other-file")).clicked() {
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
            self.display_through = through;
            self.selectable = selectable;
            self.opacity = opacity.min(100);
            self.texts = texts;
            if locked != self.scale_xy.is_some() {
                self.set_scale_locked(locked);
            }
        }
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
        if ok && field_errors.is_empty() && self.insert(state).is_ok() {
            CanvasDialog::finish(state);
            return DialogAction::Inserted;
        }
        if !open || close {
            CanvasDialog::finish(state);
            return DialogAction::Closed;
        }
        self.update_preview(state);
        DialogAction::Open
    }
}
