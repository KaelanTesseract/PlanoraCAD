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
use crate::drawing_browser::READ_ONLY_HINT;
use crate::state::AppState;
use crate::theme;
use planora_drawing::DrawingId;
use planora_drawing::annotate::dimension_text;
use planora_drawing::dimension::{DimId, DimKind, DimSpec, Tolerance};
use planora_feature::{MAX_DECIMALS, MAX_DIM_TEXT_CHARS, MAX_FIT_CHARS, MAX_TOL_MM};
use std::cell::RefCell;

/// Meldung, wenn das Maß inzwischen fehlt.
const GONE: &str = "Das Maß gibt es nicht mehr.";
/// Hinweis zum eigenen Maßtext.
pub const PLACEHOLDER_HINT: &str = "<> steht für den berechneten Wert";
/// Text neben dem Wert, wenn das Maß keinen Bezug mehr hat.
const LOST_PREFIX: &str = "Das Maß hat seinen Bezug verloren";

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
    pub fn label(self) -> &'static str {
        match self {
            ToleranceKind::None => "Keine",
            ToleranceKind::Symmetric => "Symmetrisch ±",
            ToleranceKind::Limits => "Oben und unten",
            ToleranceKind::LimitSizes => "Grenzmaße",
            ToleranceKind::Fit => "Passung",
        }
    }
}

/// Bezeichnung der Maßart in der Oberfläche.
pub fn kind_label(kind: DimKind) -> &'static str {
    match kind {
        DimKind::Horizontal => "Waagerecht",
        DimKind::Vertical => "Senkrecht",
        DimKind::Aligned => "Schräg",
        DimKind::Diameter => "Durchmesser",
        DimKind::Radius => "Radius",
        DimKind::Angle => "Winkel",
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
        return Err("Bitte eine Zahl eingeben.".into());
    }
    let normal = t.replace('\u{2212}', "-").replace(',', ".");
    let v: f64 = normal
        .parse()
        .map_err(|_| format!("„{t}“ ist keine Zahl."))?;
    if !v.is_finite() {
        return Err(format!("„{t}“ ist keine Zahl."));
    }
    if v.abs() > MAX_TOL_MM {
        return Err(format!("Die Zahl darf höchstens {MAX_TOL_MM} betragen."));
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
        // Leerzeichen am Rand zählen nicht: ein Text nur aus Leerzeichen ist „kein Text“.
        let fit = self.fit.trim();
        let tolerance = match self.kind {
            ToleranceKind::None => Tolerance::None,
            ToleranceKind::Symmetric => {
                let ok = errors.len();
                let v = number(&mut errors, "Abweichung", &self.symmetric);
                if errors.len() == ok && v < 0.0 {
                    errors.push("Abweichung: Die Abweichung darf nicht negativ sein.".into());
                }
                Tolerance::Symmetric(v)
            }
            ToleranceKind::Limits | ToleranceKind::LimitSizes => {
                let (up, low) = if self.kind == ToleranceKind::Limits {
                    ("Oben", "Unten")
                } else {
                    ("Obere Abweichung", "Untere Abweichung")
                };
                let ok = errors.len();
                let upper = number(&mut errors, up, &self.upper);
                let lower = number(&mut errors, low, &self.lower);
                if errors.len() == ok && upper < lower {
                    errors.push(format!(
                        "{up}: Die obere Abweichung darf nicht kleiner als die untere sein."
                    ));
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
                errors.push("Passung: Bitte eine Passung eingeben.".into());
            } else if fit.chars().count() > MAX_FIT_CHARS {
                errors.push(format!(
                    "Passung: Der Text darf höchstens {MAX_FIT_CHARS} Zeichen haben."
                ));
            }
        }
        if self.text.trim().chars().count() > MAX_DIM_TEXT_CHARS {
            errors.push(format!(
                "Maßtext: Der Text darf höchstens {MAX_DIM_TEXT_CHARS} Zeichen haben."
            ));
        }
        if self.decimals > MAX_DECIMALS {
            errors.push(format!(
                "Nachkommastellen: Höchstens {MAX_DECIMALS} sind möglich."
            ));
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
            return Err(READ_ONLY_HINT.into());
        }
        let (decimals, tolerance, text) = self.build().map_err(|e| e.join(" "))?;
        let Some(spec) = state
            .document
            .drawing(self.drawing)
            .and_then(|d| d.dimension(self.dim))
        else {
            return Err(GONE.into());
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
        let response = Dialog::new("Maß bearbeiten", DialogSize::Wide)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut measure_card = |ui: &mut egui::Ui| {
                    ui.strong("Maß");
                    egui::Grid::new("planora-dimension-measure")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Art");
                            ui.label(kind_label(spec.kind));
                            ui.end_row();
                            ui.label("Wert");
                            match &value {
                                Ok(v) => {
                                    ui.label(v);
                                }
                                Err(_) => {
                                    ui.colored_label(error_color(ui.visuals().dark_mode), "?");
                                }
                            }
                            ui.end_row();
                            ui.label("Nachkommastellen");
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
                            note("Nachkommastellen", &r.inner);
                            ui.end_row();
                            ui.label("Maßtext");
                            let r = theme::text_field_enabled(
                                ui,
                                !read_only,
                                egui::TextEdit::singleline(&mut text).desired_width(220.0),
                            );
                            note("Maßtext", &r);
                            ui.end_row();
                        });
                    ui.weak(PLACEHOLDER_HINT);
                    if let Err(reason) = &value {
                        ui.add_space(4.0);
                        ui.colored_label(
                            error_color(ui.visuals().dark_mode),
                            format!("{LOST_PREFIX}: {reason}"),
                        );
                    }
                };
                let mut tolerance_card = |ui: &mut egui::Ui| {
                    ui.strong("Toleranz");
                    egui::Grid::new("planora-dimension-tolerance")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Art");
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
                            note("Toleranzart", &r.inner);
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
                                    field(ui, "Abweichung ±", &mut symmetric);
                                }
                                ToleranceKind::Limits => {
                                    field(ui, "Oben", &mut upper);
                                    field(ui, "Unten", &mut lower);
                                }
                                ToleranceKind::LimitSizes => {
                                    field(ui, "Obere Abweichung", &mut upper);
                                    field(ui, "Untere Abweichung", &mut lower);
                                }
                                ToleranceKind::Fit => {
                                    field(ui, "Passung", &mut fit);
                                }
                            }
                        });
                    if kind == ToleranceKind::LimitSizes {
                        ui.weak("Gezeigt werden Höchstmaß und Mindestmaß.");
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
                    ui.weak(READ_ONLY_HINT);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let r = ui.add_enabled(apply_ok, egui::Button::new("Übernehmen"));
                    note("Übernehmen", &r);
                    if r.clicked() {
                        apply = true;
                    }
                    if ui
                        .button(if read_only { "Schließen" } else { "Abbrechen" })
                        .clicked()
                    {
                        close = true;
                    }
                    let r = ui.add_enabled(
                        !read_only,
                        egui::Button::new(
                            egui::RichText::new("Maß löschen")
                                .color(error_color(ui.visuals().dark_mode)),
                        ),
                    );
                    note("Maß löschen", &r);
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
                Err(e) => state.message = Some(e),
            }
        }
        open && !close
    }
}
