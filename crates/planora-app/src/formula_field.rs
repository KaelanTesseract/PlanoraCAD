//! Formelfeld: ein Textfeld, das eine Zahl oder eine Formel mit Parameternamen annimmt
//! („Breite / 2“). Bei einer Formel steht links ein klickbares „fx“ (setzt den berechneten Wert
//! ein und löscht damit die Formel), rechts der Wert („= 20,00 mm“), bei einem Fehler darunter
//! die Meldung in der Fehlerfarbe. Eine reine Zahl zeigt rechts wie bisher nur die Einheit.
//!
//! Zuerst gilt die Zahlenregel ([`plain_number`], nimmt auch „1e3“), erst dann die Formel.

use crate::sketch_view::format_number;
use crate::theme;
use planora_feature::expr::{number_text, plain_number};
use planora_feature::{ParamScope, ParamUnit};
use planora_i18n::{localize_decimal, tr};

/// Zahl als Text für ein Feld, das aus einem Zahlenwert vorbelegt wird: verlustfrei wie
/// [`number_text`], aber mit dem Dezimaltrenner der Sprache (wie die Vorgaben der Skizzenfelder).
/// Der Text darf wieder mit Komma oder Punkt gelesen werden. Gespeicherte Formeltexte bleiben,
/// wie sie getippt wurden.
pub fn number_field_text(v: f64) -> String {
    localize_decimal(&number_text(v).replace(',', "."))
}

/// Was im Feld steht.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldInput {
    /// Eine reine Zahl (Komma oder Punkt).
    Number(f64),
    /// Eine Formel (ohne Leerraum am Rand).
    Formula(String),
}

/// Zeichen vor einer Formel.
pub const FX: &str = "fx";
/// Name des Zeichens „fx“ für Bildschirmleser.
pub fn fx_label() -> String {
    tr!("field-fx-label")
}
/// Hinweis zum Zeichen „fx“.
pub fn fx_hint() -> String {
    tr!("field-fx-hint")
}
/// Meldung bei leerem Feld.
pub fn empty() -> String {
    tr!("field-empty")
}
/// So viele Namen nennt [`names_hint`] höchstens.
pub const HINT_NAMES: usize = 8;

/// Leer: `None`; reine Zahl: `Number`; sonst `Formula` (ohne Leerraum am Rand).
pub fn classify(text: &str) -> Option<FieldInput> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    Some(match plain_number(t) {
        Some(v) => FieldInput::Number(v),
        None => FieldInput::Formula(t.to_string()),
    })
}

/// Wert und Formel des Feldes: Zahl ohne Formel, Formel mit ihrem Wert (passend zu `unit`),
/// sonst die Meldung.
pub fn resolve(
    text: &str,
    unit: ParamUnit,
    scope: &ParamScope,
) -> Result<(f64, Option<String>), String> {
    match classify(text) {
        None => Err(empty()),
        Some(FieldInput::Number(v)) => Ok((v, None)),
        Some(FieldInput::Formula(f)) => scope.eval(&f, unit).map(|v| (v, Some(f))),
    }
}

/// Wert mit Einheit, zwei Nachkommastellen: „40,00 mm“, „45,00°“, „1,50“.
pub fn unit_value(v: f64, unit: ParamUnit) -> String {
    let n = format_number(v, 2);
    match unit {
        ParamUnit::Length => format!("{n} mm"),
        ParamUnit::Angle => format!("{n}°"),
        ParamUnit::Number => n,
    }
}

/// Anzeige hinter einer Formel: „= 20,00 mm“, „= 45,00°“, „= 1,50“.
pub fn value_text(v: f64, unit: ParamUnit) -> String {
    format!("= {}", unit_value(v, unit))
}

/// Hinweis mit den Parameternamen („Namen: Breite, Tiefe“), höchstens [`HINT_NAMES`], danach
/// „ …“; `None` ohne Parameter.
pub fn names_hint(scope: &ParamScope) -> Option<String> {
    let names = scope.names();
    if names.is_empty() {
        return None;
    }
    let shown: Vec<&str> = names.iter().take(HINT_NAMES).map(String::as_str).collect();
    let more = if names.len() > HINT_NAMES { " …" } else { "" };
    Some(tr!(
        "field-names",
        list = format!("{}{more}", shown.join(", "))
    ))
}

/// Ergebnis eines Formelfelds in diesem Bild.
pub struct FieldOutcome {
    /// Antwort des Textfelds.
    pub response: egui::Response,
    /// Enter gedrückt, oder das Feld wurde nach einer Änderung verlassen.
    pub committed: bool,
    /// „fx“ wurde angeklickt; der Text ist jetzt der berechnete Wert.
    pub fx_clicked: bool,
    /// Meldung einer ungültigen Formel (auch wenn sie nicht unter dem Feld steht, siehe
    /// [`FormulaField::error_below`]).
    pub error: Option<String>,
}

/// Formelfeld (siehe Modulbeschreibung) als Baustein:
///
/// ```ignore
/// let o = FormulaField::new(("quader", "breite"), &mut text, ParamUnit::Length)
///     .width(90.0)
///     .enabled(!read_only)
///     .label("Breite")
///     .show(ui, &scope);
/// if o.committed { /* übernehmen */ }
/// ```
///
/// - `id_salt` (Pflicht) macht die Kennung des Textfelds fest: Das Zeichen „fx“ erscheint vor
///   dem Feld, sobald aus einer Zahl eine Formel wird; ohne feste Kennung verlöre das Feld dabei
///   die Tastatur. Mehrere Felder in einem Fenster brauchen verschiedene Werte.
/// - `width`: Breite des Textes (Vorgabe 90 px).
/// - `enabled(false)`: gesperrt (nur lesbar, blass), „fx“ ebenso; nie `committed`.
/// - `error_below(false)`: Die Meldung steht nicht unter dem Feld (gleich hohe Zeilen in einer
///   Tabelle); der Aufrufer zeigt [`FieldOutcome::error`] selbst.
/// - `label`: Name für Bildschirmleser.
/// - `saved`: der übernommene Text (etwa aus dem Dokument). Dann übernimmt das Verlassen des
///   Feldes, sobald der Text davon abweicht, auch wenn die Änderung schon vor dem letzten
///   Fokus getippt wurde (sonst nur nach einer Änderung seit dem Fokus).
///
/// Für Aufgabe 10: Mit `error_below(true)` schiebt die Meldung das Feld darunter um eine
/// Zeile (etwa 49 px statt 26 px) nach unten; in einem Raster Platz vorsehen oder die Meldung
/// (`FieldOutcome::error`) in einer Fußzeile zeigen.
pub struct FormulaField<'a> {
    id_salt: egui::Id,
    text: &'a mut String,
    unit: ParamUnit,
    width: f32,
    enabled: bool,
    error_below: bool,
    label: Option<String>,
    saved: Option<&'a str>,
}

impl<'a> FormulaField<'a> {
    pub fn new(
        id_salt: impl std::hash::Hash + std::fmt::Debug,
        text: &'a mut String,
        unit: ParamUnit,
    ) -> Self {
        FormulaField {
            id_salt: egui::Id::new(id_salt),
            text,
            unit,
            width: 90.0,
            enabled: true,
            error_below: true,
            label: None,
            saved: None,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn error_below(mut self, below: bool) -> Self {
        self.error_below = below;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn saved(mut self, saved: &'a str) -> Self {
        self.saved = Some(saved);
        self
    }

    /// Zeigt das Feld; die Formel wird mit `scope` ausgewertet.
    pub fn show(self, ui: &mut egui::Ui, scope: &ParamScope) -> FieldOutcome {
        let unit = self.unit;
        self.show_with(ui, &|f| scope.eval(f, unit))
    }

    /// Wie [`FormulaField::show`], die Formel wertet aber `eval` aus (etwa mit einer
    /// Probetabelle im Fenster „Parameter“, damit eine Zeile sich nicht mit ihrem alten Wert
    /// selbst berechnet).
    pub fn show_with(
        self,
        ui: &mut egui::Ui,
        eval: &dyn Fn(&str) -> Result<f64, String>,
    ) -> FieldOutcome {
        let FormulaField {
            id_salt,
            text,
            unit,
            width,
            enabled,
            error_below,
            label,
            saved,
        } = self;
        let dark = ui.visuals().dark_mode;
        let result = match classify(text) {
            Some(FieldInput::Formula(f)) => Some(eval(&f)),
            _ => None,
        };
        let mut fx_clicked = false;
        let inner = ui.vertical(|ui| {
            let row = ui.horizontal(|ui| {
                if result.is_some() {
                    let valid = matches!(result, Some(Ok(_)));
                    // Nur per Mausklick: kein Halt für Tab und Umschalt+Tab, also auch nie
                    // Enter oder Leertaste auf einem „fx“ mit der Tastatur (die Formel ginge
                    // sonst mit einem Tastendruck verloren). Bildschirmleser finden es weiter
                    // unter seinem Namen.
                    let fx = ui
                        .add_enabled(
                            enabled && valid,
                            egui::Button::new(egui::RichText::new(FX).italics())
                                .frame(false)
                                .sense(egui::Sense::CLICK),
                        )
                        .on_hover_text(fx_hint())
                        .on_disabled_hover_text(fx_hint());
                    fx.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            enabled && valid,
                            fx_label(),
                        )
                    });
                    if fx.clicked()
                        && let Some(Ok(v)) = result
                    {
                        *text = number_field_text(v);
                        fx_clicked = true;
                    }
                }
                let response = theme::text_field_enabled(
                    ui,
                    enabled,
                    egui::TextEdit::singleline(text)
                        .id_salt(id_salt)
                        .desired_width(width),
                );
                if let Some(l) = &label {
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, enabled, l)
                    });
                }
                match &result {
                    Some(Ok(v)) => {
                        let shown = value_text(*v, unit);
                        // Große Werte werden gekürzt; egui zeigt dann beim Überfahren den
                        // ganzen Text als Hinweis.
                        ui.add(egui::Label::new(theme::value(shown)).truncate());
                    }
                    Some(Err(_)) => {}
                    None => {
                        if !unit.suffix().is_empty() {
                            ui.label(unit.suffix());
                        }
                    }
                }
                response
            });
            if error_below && let Some(Err(e)) = &result {
                ui.colored_label(theme::palette(dark).error, e);
            }
            row.inner
        });
        let response = inner.inner;
        let committed = enabled
            && match saved {
                Some(s) => leave_signal(ui, &response, text.as_str() != s),
                None => commit_signal(ui, &response),
            };
        FieldOutcome {
            response,
            committed,
            fx_clicked,
            error: match result {
                Some(Err(e)) => Some(e),
                _ => None,
            },
        }
    }
}

/// Kurzform von [`FormulaField`]: Feld mit fester Kennung `id_salt` und Textbreite `width`,
/// Formel mit `scope` ausgewertet, Meldung unter dem Feld.
pub fn formula_field(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    unit: ParamUnit,
    scope: &ParamScope,
    width: f32,
) -> FieldOutcome {
    FormulaField::new(id_salt, text, unit)
        .width(width)
        .show(ui, scope)
}

/// Übernahme eines Textfelds, dessen Entwurf mit einem übernommenen Stand verglichen wird:
/// Enter, oder das Feld wurde verlassen und `differs` (Entwurf weicht vom Stand ab).
pub fn leave_signal(ui: &egui::Ui, response: &egui::Response, differs: bool) -> bool {
    response.lost_focus() && (differs || ui.input(|i| i.key_pressed(egui::Key::Enter)))
}

/// Übernahme eines Textfelds: Enter, oder das Feld wurde nach einer Änderung verlassen (Plan
/// P9). Merkt sich dazu im egui-Speicher, ob sich der Text seit dem Fokus geändert hat.
pub fn commit_signal(ui: &egui::Ui, response: &egui::Response) -> bool {
    let key = response.id.with("formel-geaendert");
    if response.gained_focus() {
        ui.data_mut(|d| d.remove::<bool>(key));
    }
    if response.changed() {
        ui.data_mut(|d| d.insert_temp(key, true));
    }
    if !response.lost_focus() {
        return false;
    }
    let changed = ui.data_mut(|d| d.remove_temp::<bool>(key)).unwrap_or(false);
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    enter || changed
}
