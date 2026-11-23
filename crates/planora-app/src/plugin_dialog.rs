//! Beschriebene Dialoge der Plug-ins (Plug-ins Etappe 1, Task 6): Das Plug-in beschreibt den
//! Dialog (Titel, Felder), das Programm zeichnet ihn im Dialog-Rahmen der Befehle
//! ([`crate::dialog::Dialog`], mittig im Hauptfenster, nicht abdockbar) und meldet, was der
//! Nutzer tut. Titel und Beschriftungen sind Text-Schlüssel des Plug-ins und werden in jedem
//! Bild neu übersetzt (ein Sprachwechsel bei offenem Dialog wirkt sofort).
//!
//! Felder: `label` nur Text, `text` einzeiliges Feld, `number` Zahlenfeld (Komma oder Punkt,
//! angezeigt mit dem Trenner der Sprache; Einheiten erst in Etappe 2), `toggle` ein Haken. Jede
//! Änderung meldet `changed`, „OK“ meldet `ok` mit allen Werten, „Abbrechen“, Esc und das
//! Schließen-Symbol melden `cancel`; nach `ok` und `cancel` ist der Dialog zu.

use crate::dialog::{Dialog as Frame, DialogSize, escape_pressed};
use crate::theme;
use planora_i18n::tr;
use planora_plugin_host::runtime::{Dialog, DialogEvent, FieldKind, FieldValue};
use std::cell::Cell;
use std::collections::BTreeMap;

/// Ein offener Dialog eines Plug-ins mit den Werten seiner Felder.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenPluginDialog {
    /// Kennung des Plug-ins.
    pub plugin: String,
    /// Die Beschreibung aus `open-dialog`.
    pub spec: Dialog,
    /// Aktueller Wert je Feld-id (`label`-Felder haben keinen).
    pub values: BTreeMap<String, FieldValue>,
    /// Getippter Text der Zahlenfelder je Feld-id (auch halbfertig, etwa „1,“); der Wert in
    /// [`OpenPluginDialog::values`] ist die letzte gültige Zahl.
    pub number_texts: BTreeMap<String, String>,
}

/// Anfangswert eines Felds aus seiner Beschreibung.
fn initial(kind: &FieldKind) -> Option<FieldValue> {
    match kind {
        FieldKind::Label => None,
        FieldKind::Text(s) => Some(FieldValue::Text(s.clone())),
        FieldKind::Number(v) => Some(FieldValue::Number(*v)),
        FieldKind::Toggle(b) => Some(FieldValue::Toggle(*b)),
    }
}

/// Eine Zahl für das Zahlenfeld: so kurz wie möglich, mit dem Trenner der Sprache.
pub fn format_number(v: f64) -> String {
    planora_i18n::localize_decimal(&format!("{v}"))
}

impl OpenPluginDialog {
    /// Frisch geöffnet: die Werte aus der Beschreibung.
    pub fn new(plugin: impl Into<String>, spec: Dialog) -> OpenPluginDialog {
        let mut d = OpenPluginDialog {
            plugin: plugin.into(),
            spec: Dialog {
                id: spec.id.clone(),
                title_key: String::new(),
                fields: Vec::new(),
            },
            values: BTreeMap::new(),
            number_texts: BTreeMap::new(),
        };
        d.reopen(spec);
        d
    }

    /// `open-dialog` mit derselben id: Die Felder kommen aus `spec`; ein Feld mit derselben id
    /// und derselben Art behält seinen Wert, alle übrigen Werte fallen weg.
    pub fn reopen(&mut self, spec: Dialog) {
        let mut values = BTreeMap::new();
        let mut texts = BTreeMap::new();
        for f in &spec.fields {
            let Some(fresh) = initial(&f.kind) else {
                continue;
            };
            let value = match self.values.remove(&f.id) {
                Some(old) if std::mem::discriminant(&old) == std::mem::discriminant(&fresh) => {
                    if let Some(t) = self.number_texts.remove(&f.id) {
                        texts.insert(f.id.clone(), t);
                    }
                    old
                }
                _ => fresh,
            };
            values.insert(f.id.clone(), value);
        }
        self.spec = spec;
        self.values = values;
        self.number_texts = texts;
    }

    /// Alle Werte in der Reihenfolge der Felder (für `ok`).
    pub fn all_values(&self) -> Vec<(String, FieldValue)> {
        self.spec
            .fields
            .iter()
            .filter_map(|f| self.values.get(&f.id).map(|v| (f.id.clone(), v.clone())))
            .collect()
    }

    /// Steht in einem Zahlenfeld ein Text, der keine Zahl ist? (Dann ist „OK“ gesperrt.)
    pub fn has_invalid_number(&self) -> bool {
        self.number_texts
            .values()
            .any(|t| planora_i18n::parse_num(t).is_none())
    }

    /// Die Id des Fensters (sprachunabhängig, je Plug-in und Dialog).
    pub fn window_id(&self) -> egui::Id {
        egui::Id::unique(("planora_plugin_dialog", &self.plugin, &self.spec.id))
    }
}

/// Zeichnet den Dialog `d` und liefert, was der Nutzer in diesem Bild getan hat. `texts`
/// übersetzt einen Text-Schlüssel des Plug-ins in die aktuelle Sprache. Nach `ok` und `cancel`
/// schließt der Aufrufer den Dialog.
pub fn show(
    ctx: &egui::Context,
    d: &mut OpenPluginDialog,
    texts: &dyn Fn(&str) -> String,
) -> Option<DialogEvent> {
    let mut open = true;
    let mut event: Option<DialogEvent> = None;
    let mut ok = false;
    // Beide Teile (Inhalt und Fußzeile) brauchen sie: Zellen statt zweier `&mut`.
    let cancel = Cell::new(false);
    let invalid = Cell::new(d.has_invalid_number());
    let id = d.window_id();
    let title = texts(&d.spec.title_key);
    Frame::new(title, DialogSize::Normal)
        .id(id)
        .open(&mut open)
        .show_with_footer(
            ctx,
            |ui| {
                let mut bad = false;
                let fields = d.spec.fields.clone();
                egui::Grid::new(id.with("felder"))
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        for f in &fields {
                            ui.label(texts(&f.label_key));
                            if let Some(changed) = field(ui, f.id.as_str(), d, &mut bad) {
                                event = Some(DialogEvent::Changed((f.id.clone(), changed)));
                            }
                            ui.end_row();
                        }
                    });
                invalid.set(bad);
                if escape_pressed(ui, true) {
                    cancel.set(true);
                }
            },
            |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!invalid.get(), egui::Button::new(tr!("common-ok")))
                        .clicked()
                    {
                        ok = true;
                    }
                    if ui.button(tr!("common-cancel")).clicked() {
                        cancel.set(true);
                    }
                });
            },
        );
    if ok {
        Some(DialogEvent::Ok(d.all_values()))
    } else if cancel.get() || !open {
        Some(DialogEvent::Cancel)
    } else {
        event
    }
}

/// Das Eingabeelement des Felds `id` (rechte Spalte); `Some(neuer Wert)` nach einer Änderung.
/// Ein Zahlenfeld mit ungültigem Text setzt `invalid` (dann ist „OK“ gesperrt).
fn field(
    ui: &mut egui::Ui,
    id: &str,
    d: &mut OpenPluginDialog,
    invalid: &mut bool,
) -> Option<FieldValue> {
    let width = ui.available_width().max(160.0);
    match d.values.get_mut(id) {
        None => {
            // `label`: nur die Beschriftung links.
            ui.label("");
            None
        }
        Some(FieldValue::Text(t)) => {
            let r = theme::text_field(ui, egui::TextEdit::singleline(t).desired_width(width));
            r.changed().then(|| FieldValue::Text(t.clone()))
        }
        Some(FieldValue::Toggle(b)) => {
            let r = ui.checkbox(b, "");
            r.changed().then_some(FieldValue::Toggle(*b))
        }
        Some(FieldValue::Number(v)) => {
            let text = d
                .number_texts
                .entry(id.to_string())
                .or_insert_with(|| format_number(*v));
            let r = theme::text_field(ui, egui::TextEdit::singleline(text).desired_width(width));
            let parsed = planora_i18n::parse_num(text);
            if parsed.is_none() {
                *invalid = true;
            }
            let mut out = None;
            if r.changed()
                && let Some(n) = parsed
                && n != *v
            {
                *v = n;
                out = Some(FieldValue::Number(n));
            }
            // Ohne Eingabefokus steht die Zahl mit dem Trenner der aktuellen Sprache da.
            if !r.has_focus() && parsed == Some(*v) {
                *text = format_number(*v);
            }
            out
        }
    }
}
