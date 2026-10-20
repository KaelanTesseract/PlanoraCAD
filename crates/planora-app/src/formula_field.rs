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
/// Hinweis zum Zeichen „fx“.
pub const FX_HINT: &str = "Formel. Klicken setzt den berechneten Wert ein und löscht die Formel.";
/// Meldung bei leerem Feld.
pub const EMPTY: &str = "Bitte eine Zahl oder Formel eingeben.";
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
        None => Err(EMPTY.into()),
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
    Some(format!("Namen: {}{more}", shown.join(", ")))
}

/// Ergebnis eines Formelfelds in diesem Bild.
pub struct FieldOutcome {
    /// Antwort des Textfelds.
    pub response: egui::Response,
    /// Enter gedrückt, oder das Feld wurde nach einer Änderung verlassen.
    pub committed: bool,
    /// „fx“ wurde angeklickt; der Text ist jetzt der berechnete Wert.
    pub fx_clicked: bool,
}

/// Formelfeld (siehe Modulbeschreibung) mit Textbreite `width`; die Formel wird mit `scope`
/// ausgewertet.
pub fn formula_field(
    ui: &mut egui::Ui,
    text: &mut String,
    unit: ParamUnit,
    scope: &ParamScope,
    width: f32,
) -> FieldOutcome {
    formula_field_with(ui, text, unit, &|f| scope.eval(f, unit), width)
}

/// Wie [`formula_field`], die Formel wertet aber `eval` aus (etwa mit einer Probetabelle im
/// Fenster „Parameter“, damit eine Zeile sich nicht mit ihrem alten Wert selbst berechnet).
pub fn formula_field_with(
    ui: &mut egui::Ui,
    text: &mut String,
    unit: ParamUnit,
    eval: &dyn Fn(&str) -> Result<f64, String>,
    width: f32,
) -> FieldOutcome {
    let dark = ui.visuals().dark_mode;
    let input = classify(text);
    let result = match &input {
        Some(FieldInput::Formula(f)) => Some(eval(f)),
        _ => None,
    };
    let mut fx_clicked = false;
    let inner = ui.vertical(|ui| {
        let row = ui.horizontal(|ui| {
            if result.is_some() {
                let valid = matches!(result, Some(Ok(_)));
                let fx = ui
                    .add_enabled(
                        valid,
                        egui::Button::new(egui::RichText::new(FX).italics()).frame(false),
                    )
                    .on_hover_text(FX_HINT)
                    .on_disabled_hover_text(FX_HINT);
                theme::focus_ring(ui, &fx);
                if fx.clicked()
                    && let Some(Ok(v)) = result
                {
                    *text = number_text(v);
                    fx_clicked = true;
                }
            }
            let response =
                theme::text_field(ui, egui::TextEdit::singleline(text).desired_width(width));
            match &result {
                Some(Ok(v)) => {
                    ui.label(theme::value(value_text(*v, unit)));
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
        if let Some(Err(e)) = &result {
            ui.colored_label(theme::palette(dark).error, e);
        }
        row.inner
    });
    let response = inner.inner;
    let committed = commit_signal(ui, &response);
    FieldOutcome {
        response,
        committed,
        fx_clicked,
    }
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
