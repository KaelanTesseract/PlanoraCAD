//! Fenster „Maß bearbeiten“ (Teilprojekt 12, Aufgabe 13): Art und Wert (nur lesen), Nachkommastellen,
//! Toleranz und eigener Maßtext. Änderungen gehen nur über `Document::update_dimension`, das
//! Löschen über `Document::delete_dimension`. Ungültige Eingaben stehen rot im Fenster und sperren
//! „Übernehmen“; in einer alten Version ist alles gesperrt.
//!
//! Die Grenzen (Nachkommastellen, Länge der Texte, Betrag der Toleranz) sind die der Prüfung im
//! Dokument ([`planora_feature::MAX_DECIMALS`] usw.). „Grenzmaße“ meinen wie im Blatt
//! Abweichungen vom Nennmaß; „Passung“ ist nur Text.

use crate::browser::error_color;
use crate::cards::card_row;
use crate::dialog::{Dialog, DialogSize};
use crate::dimension_browser::{lost_reason, request_delete_dimension};
use crate::dimension_state::resolve_in;
use crate::drawing_browser::read_only_hint;
use crate::state::AppState;
use crate::theme;
use planora_drawing::DrawingId;
use planora_drawing::annotate::dimension_text;
use planora_drawing::dimension::{DimId, DimKind, DimSpec, Tolerance};
use planora_feature::{MAX_DECIMALS, MAX_DIM_TEXT_CHARS, MAX_FIT_CHARS, MAX_TOL_MM};
use planora_i18n::tr;
use std::cell::RefCell;

/// Meldung, wenn das Maß inzwischen fehlt.
fn gone() -> String {
    tr!("dim-gone")
}
/// Hinweis zum eigenen Maßtext.
pub fn placeholder_hint() -> String {
    tr!("dim-placeholder-hint")
}
/// Text neben dem Wert, wenn das Maß keinen Bezug mehr hat.
fn lost_prefix() -> String {
    tr!("dim-lost-prefix")
}

/// Art der Toleranz in der Auswahl.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToleranceKind {
    None,
    Symmetric,
    Limits,
    LimitSizes,
    Fit,
}

impl ToleranceKind {
    /// Reihenfolge der Auswahlliste.
    pub const ALL: [ToleranceKind; 5] = [
        ToleranceKind::None,
        ToleranceKind::Symmetric,
        ToleranceKind::Limits,
        ToleranceKind::LimitSizes,
        ToleranceKind::Fit,
    ];

    /// Bezeichnung in der Oberfläche.
    pub fn label(self) -> String {
        match self {
            ToleranceKind::None => tr!("dim-tol-none"),
            ToleranceKind::Symmetric => tr!("dim-tol-symmetric"),
            ToleranceKind::Limits => tr!("dim-tol-limits"),
            ToleranceKind::LimitSizes => tr!("dim-tol-limit-sizes"),
            ToleranceKind::Fit => tr!("dim-tol-fit"),
        }
    }
}

/// Bezeichnung der Maßart in der Oberfläche.
pub fn kind_label(kind: DimKind) -> String {
    kind_msg(kind).text()
}

/// Wie [`kind_label`], übersetzt erst beim Zeichnen (für Meldungen, die ein Fenster behält).
pub fn kind_msg(kind: DimKind) -> crate::msg::Msg {
    match kind {
        DimKind::Horizontal => crate::msg!("dim-kind-horizontal"),
        DimKind::Vertical => crate::msg!("dim-kind-vertical"),
        DimKind::Aligned => crate::msg!("dim-kind-aligned"),
        DimKind::Diameter => crate::msg!("dim-kind-diameter"),
        DimKind::Radius => crate::msg!("dim-kind-radius"),
        DimKind::Angle => crate::msg!("dim-kind-angle"),
    }
}

/// Zahl als Text mit dem Dezimaltrenner der Sprache (verlustfrei, kürzeste Schreibweise).
fn number_text(v: f64) -> String {
    planora_i18n::localize_decimal(&format!("{v}"))
}

/// Zahl aus einem Eingabefeld: deutsches Komma oder Punkt, `−` oder `-`.
fn parse_number(text: &str) -> Result<f64, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err(tr!("dim-err-enter-number"));
    }
    let normal = t.replace('\u{2212}', "-").replace(',', ".");
    let v: f64 = normal
        .parse()
        .map_err(|_| tr!("dim-err-not-a-number", text = t))?;
    if !v.is_finite() {
        return Err(tr!("dim-err-not-a-number", text = t));
    }
    if v.abs() > MAX_TOL_MM {
        return Err(tr!(
            "dim-err-too-big",
            max = crate::msg::plain_number(MAX_TOL_MM)
        ));
    }
    Ok(v)
}

/// Fenster „Maß bearbeiten“.
#[derive(Clone, Debug)]
pub struct DimensionDialog {
    pub drawing: DrawingId,
    pub dim: DimId,
    /// Alte Version: alles gesperrt.
    pub read_only: bool,
    pub decimals: u8,
    pub kind: ToleranceKind,
    /// Abweichung bei „Symmetrisch ±“.
    pub symmetric: String,
    /// Obere Abweichung („Oben und unten“, „Grenzmaße“).
    pub upper: String,
    /// Untere Abweichung („Oben und unten“, „Grenzmaße“).
    pub lower: String,
    /// Text der Passung.
    pub fit: String,
    /// Eigener Maßtext (leer: der berechnete Wert).
    pub text: String,
    /// Bedienelemente des letzten Bildes: Bezeichnung und ob sie bedienbar waren.
    pub inputs: Vec<(String, bool)>,
    pub last_rect: Option<egui::Rect>,
    pub last_cards: Vec<egui::Rect>,
}

/// Öffnet das Fenster „Maß bearbeiten“ für ein Maß (ersetzt ein offenes). Gibt es das Maß nicht,
/// passiert nichts.
pub fn open(state: &mut AppState, drawing: DrawingId, dim: DimId, read_only: bool) {
    let Some(spec) = state
        .document
        .drawing(drawing)
        .and_then(|d| d.dimension(dim))
    else {
        return;
    };
    state.dimension_dialog = Some(DimensionDialog::new(drawing, spec, read_only));
}

impl DimensionDialog {
    /// Entwurf aus dem Maß.
    pub fn new(drawing: DrawingId, spec: &DimSpec, read_only: bool) -> DimensionDialog {
        let zero = || "0".to_string();
        let mut d = DimensionDialog {
            drawing,
            dim: spec.id,
            read_only,
            decimals: spec.decimals.min(MAX_DECIMALS),
            kind: ToleranceKind::None,
            symmetric: zero(),
            upper: zero(),
            lower: zero(),
            fit: String::new(),
            text: spec.text_override.clone().unwrap_or_default(),
            inputs: Vec::new(),
            last_rect: None,
            last_cards: Vec::new(),
        };
        match &spec.tolerance {
            Tolerance::None => {}
            Tolerance::Symmetric(v) => {
                d.kind = ToleranceKind::Symmetric;
                d.symmetric = number_text(*v);
            }
            Tolerance::Limits { upper, lower } => {
                d.kind = ToleranceKind::Limits;
                d.upper = number_text(*upper);
                d.lower = number_text(*lower);
            }
            Tolerance::LimitSizes { upper, lower } => {
                d.kind = ToleranceKind::LimitSizes;
                d.upper = number_text(*upper);
                d.lower = number_text(*lower);
            }
            Tolerance::Fit(s) => {
                d.kind = ToleranceKind::Fit;
                d.fit = s.clone();
            }
        }
        d
    }

    /// Die Eingaben als Werte, oder alle Fehler als Sätze.
    #[allow(clippy::type_complexity)]
    pub fn build(&self) -> Result<(u8, Tolerance, Option<String>), Vec<String>> {
        let mut errors = Vec::new();
        let number = |errors: &mut Vec<String>, label: &str, text: &str| match parse_number(text) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!("{label}: {e}"));
                0.0
            }
        };
        let deviation = tr!("dim-deviation");
        // Leerzeichen am Rand zählen nicht: ein Text nur aus Leerzeichen ist „kein Text“.
        let fit = self.fit.trim();
        let tolerance = match self.kind {
            ToleranceKind::None => Tolerance::None,
            ToleranceKind::Symmetric => {
                let ok = errors.len();
                let v = number(&mut errors, &deviation, &self.symmetric);
                if errors.len() == ok && v < 0.0 {
                    errors.push(tr!("dim-err-negative"));
                }
                Tolerance::Symmetric(v)
            }
            ToleranceKind::Limits | ToleranceKind::LimitSizes => {
                let (up, low) = if self.kind == ToleranceKind::Limits {
                    (tr!("dim-upper"), tr!("dim-lower"))
                } else {
                    (tr!("dim-upper-deviation"), tr!("dim-lower-deviation"))
                };
                let ok = errors.len();
                let upper = number(&mut errors, &up, &self.upper);
                let lower = number(&mut errors, &low, &self.lower);
                if errors.len() == ok && upper < lower {
                    errors.push(tr!("dim-err-limits-order", label = up));
                }
                if self.kind == ToleranceKind::Limits {
                    Tolerance::Limits { upper, lower }
                } else {
                    Tolerance::LimitSizes { upper, lower }
                }
            }
            ToleranceKind::Fit => Tolerance::Fit(fit.to_string()),
        };
        if self.kind == ToleranceKind::Fit {
            if fit.is_empty() {
                errors.push(tr!("dim-err-fit-empty"));
            } else if fit.chars().count() > MAX_FIT_CHARS {
                errors.push(tr!("dim-err-fit-long", max = MAX_FIT_CHARS));
            }
        }
        if self.text.trim().chars().count() > MAX_DIM_TEXT_CHARS {
            errors.push(tr!("dim-err-text-long", max = MAX_DIM_TEXT_CHARS));
        }
        if self.decimals > MAX_DECIMALS {
            errors.push(tr!("dim-err-decimals", max = usize::from(MAX_DECIMALS)));
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let text = self.text.trim();
        let text = (!text.is_empty()).then(|| text.to_string());
        Ok((self.decimals, tolerance, text))
    }

    /// Alle Fehler der Eingaben (leer, wenn „Übernehmen“ geht).
    pub fn errors(&self) -> Vec<String> {
        self.build().err().unwrap_or_default()
    }

    /// Übernimmt die Eingaben ins Dokument (ein Schritt). Ändert sich nichts, bleibt das
    /// Dokument unberührt.
    pub fn apply(&self, state: &mut AppState) -> Result<(), String> {
        if self.read_only {
            return Err(read_only_hint());
        }
        let (decimals, tolerance, text) = self.build().map_err(|e| e.join(" "))?;
        let Some(spec) = state
            .document
            .drawing(self.drawing)
            .and_then(|d| d.dimension(self.dim))
        else {
            return Err(gone());
        };
        if spec.decimals == decimals && spec.tolerance == tolerance && spec.text_override == text {
            return Ok(());
        }
        state
            .document
            .update_dimension(self.drawing, self.dim, |s| {
                s.decimals = decimals;
                s.tolerance = tolerance;
                s.text_override = text;
            })
            .map_err(|e| e.to_string())
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll. Es schließt sich auch, wenn
    /// es das Maß nicht mehr gibt.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let Some(spec) = state
            .document
            .drawing(self.drawing)
            .and_then(|d| d.dimension(self.dim))
            .cloned()
        else {
            return false;
        };
        // Wert nur, wenn das Maß gerade gemessen ist; sonst der Grund (nie ein alter Wert).
        let dims = resolve_in(state, self.drawing);
        let result = dims
            .by_view
            .get(&spec.view)
            .and_then(|list| list.iter().find(|(s, _)| s.id == self.dim))
            .map(|(_, r)| r);
        let value: Result<String, String> = match (lost_reason(result), result) {
            (None, Some(Ok(m))) => {
                let mut plain = spec.clone();
                plain.decimals = self.decimals;
                plain.text_override = None;
                plain.tolerance = Tolerance::None;
                Ok(dimension_text(&plain, m).main)
            }
            (reason, _) => Err(reason.unwrap_or_default()),
        };
        let errors = self.errors();
        let read_only = self.read_only;
        let inputs: RefCell<Vec<(String, bool)>> = RefCell::new(Vec::new());
        let note = |label: &str, r: &egui::Response| {
            inputs.borrow_mut().push((label.to_string(), r.enabled()));
        };
        let (mut decimals, mut kind) = (self.decimals, self.kind);
        let (mut symmetric, mut upper, mut lower) = (
            self.symmetric.clone(),
            self.upper.clone(),
            self.lower.clone(),
        );
        let (mut fit, mut text) = (self.fit.clone(), self.text.clone());
        let (mut open, mut close, mut apply, mut delete) = (true, false, false, false);
        let mut cards = Vec::new();
        let apply_ok = !read_only && errors.is_empty();
        let response = Dialog::new(tr!("dim-edit-title"), DialogSize::Wide)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut measure_card = |ui: &mut egui::Ui| {
                    ui.strong(tr!("dim-card-measure"));
                    egui::Grid::new("planora-dimension-measure")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(tr!("dim-kind"));
                            ui.label(kind_label(spec.kind));
                            ui.end_row();
                            ui.label(tr!("dim-value"));
                            match &value {
                                Ok(v) => {
                                    ui.label(v);
                                }
                                Err(_) => {
                                    ui.colored_label(error_color(ui.visuals().dark_mode), "?");
                                }
                            }
                            ui.end_row();
                            let decimals_label = tr!("dim-decimals");
                            ui.label(&decimals_label);
                            let r = ui.add_enabled_ui(!read_only, |ui| {
                                egui::ComboBox::from_id_salt("planora-dimension-decimals")
                                    .selected_text(decimals.to_string())
                                    .show_ui(ui, |ui| {
                                        for n in 0..=MAX_DECIMALS {
                                            ui.selectable_value(&mut decimals, n, n.to_string());
                                        }
                                    })
                                    .response
                            });
                            note(&decimals_label, &r.inner);
                            ui.end_row();
                            let text_label = tr!("dim-text");
                            ui.label(&text_label);
                            let r = theme::text_field_enabled(
                                ui,
                                !read_only,
                                egui::TextEdit::singleline(&mut text).desired_width(220.0),
                            );
                            note(&text_label, &r);
                            ui.end_row();
                        });
                    ui.weak(placeholder_hint());
                    if let Err(reason) = &value {
                        ui.add_space(4.0);
                        ui.colored_label(
                            error_color(ui.visuals().dark_mode),
                            format!("{}: {reason}", lost_prefix()),
                        );
                    }
                };
                let mut tolerance_card = |ui: &mut egui::Ui| {
                    ui.strong(tr!("dim-card-tolerance"));
                    egui::Grid::new("planora-dimension-tolerance")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(tr!("dim-kind"));
                            let r = ui.add_enabled_ui(!read_only, |ui| {
                                egui::ComboBox::from_id_salt("planora-dimension-tolerance-kind")
                                    .selected_text(kind.label())
                                    .show_ui(ui, |ui| {
                                        for k in ToleranceKind::ALL {
                                            ui.selectable_value(&mut kind, k, k.label());
                                        }
                                    })
                                    .response
                            });
                            note(&tr!("dim-tolerance-kind"), &r.inner);
                            ui.end_row();
                            let field = |ui: &mut egui::Ui, label: &str, value: &mut String| {
                                ui.label(label);
                                let r = theme::text_field_enabled(
                                    ui,
                                    !read_only,
                                    egui::TextEdit::singleline(value).desired_width(120.0),
                                );
                                note(label, &r);
                                ui.end_row();
                            };
                            match kind {
                                ToleranceKind::None => {}
                                ToleranceKind::Symmetric => {
                                    field(ui, &tr!("dim-deviation-pm"), &mut symmetric);
                                }
                                ToleranceKind::Limits => {
                                    field(ui, &tr!("dim-upper"), &mut upper);
                                    field(ui, &tr!("dim-lower"), &mut lower);
                                }
                                ToleranceKind::LimitSizes => {
                                    field(ui, &tr!("dim-upper-deviation"), &mut upper);
                                    field(ui, &tr!("dim-lower-deviation"), &mut lower);
                                }
                                ToleranceKind::Fit => {
                                    field(ui, &tr!("dim-tol-fit"), &mut fit);
                                }
                            }
                        });
                    if kind == ToleranceKind::LimitSizes {
                        ui.weak(tr!("dim-limit-sizes-note"));
                    }
                };
                let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                    &mut [&mut measure_card, &mut tolerance_card];
                cards = card_row(ui, "dimension-edit", row);
                ui.add_space(8.0);
                for e in &errors {
                    ui.colored_label(error_color(ui.visuals().dark_mode), e);
                }
                if read_only {
                    ui.weak(read_only_hint());
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let apply_label = tr!("tree-apply");
                    let r = ui.add_enabled(apply_ok, egui::Button::new(&apply_label));
                    note(&apply_label, &r);
                    if r.clicked() {
                        apply = true;
                    }
                    if ui
                        .button(if read_only {
                            tr!("dialog-close")
                        } else {
                            tr!("common-cancel")
                        })
                        .clicked()
                    {
                        close = true;
                    }
                    let delete_label = tr!("dim-delete");
                    let r = ui.add_enabled(
                        !read_only,
                        egui::Button::new(
                            egui::RichText::new(&delete_label)
                                .color(error_color(ui.visuals().dark_mode)),
                        ),
                    );
                    note(&delete_label, &r);
                    if r.clicked() {
                        delete = true;
                    }
                });
            });
        self.inputs = inputs.into_inner();
        (self.decimals, self.kind) = (decimals, kind);
        (self.symmetric, self.upper, self.lower) = (symmetric, upper, lower);
        (self.fit, self.text) = (fit, text);
        self.last_rect = response.map(|r| r.response.rect);
        self.last_cards = cards;
        if apply && apply_ok {
            match self.apply(state) {
                Ok(()) => return false,
                Err(e) => state.message = Some(e),
            }
        }
        if delete && !read_only {
            match request_delete_dimension(state, self.drawing, self.dim) {
                Ok(()) if state.pending_delete_dimension.is_none() => return false,
                Ok(()) => {}
                Err(e) => state.message = Some(e.text()),
            }
        }
        open && !close
    }
}
