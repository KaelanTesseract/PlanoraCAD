//! Fenster „Parameter“ (Bearbeiten → Parameter …): die Parametertabelle des Dokuments mit Name,
//! Formel, Wert, Art und Kommentar je Zeile.
//!
//! - Name, Formel und Kommentar wirken bei Enter oder beim Verlassen des Feldes (Plan P9), die
//!   ganze Zeile als **ein** Schritt; eine abgelehnte Änderung lässt den Entwurf mit roter
//!   Meldung stehen, das Dokument bleibt unverändert. Die Art wirkt sofort.
//! - Löschen: ein unbenutzter Parameter wird gelöscht, ein benutzter fragt nach (Meldung mit den
//!   Benutzern, „Durch den Wert ersetzen“ oder „Abbrechen“).
//! - Das Fenster sperrt Rückgängig nicht (Plan P8): Nach Rückgängig, Wiederholen, Zurückladen
//!   oder einem anderen Stand des Dokuments lädt es seine Zeilen neu und verwirft die Entwürfe.
//!   Kennungen (`ParamId`) gelten nur für den Stand, aus dem sie gelesen wurden.
//! - Bei offener Skizze ist alles gesperrt ([`SKETCH_OPEN_LOCK`]), in einer alten Version ebenso
//!   ([`READ_ONLY_HINT`]).

use crate::dialog::{Dialog, DialogSize};
use crate::drawing_browser::READ_ONLY_HINT;
use crate::formula_field::{commit_signal, formula_field_with, unit_value};
use crate::icons::{self, Icon};
use crate::state::{AppState, SKETCH_OPEN_LOCK};
use crate::theme::{self, SPACE_2};
use crate::workspace::DocTab;
use planora_feature::expr::{ParamDef, evaluate_table};
use planora_feature::{FeatureError, ParamEdit, ParamId, ParamUnit, Parameter};

/// Titel des Fensters.
pub const TITLE: &str = "Parameter";
/// Knopf unter der Tabelle.
pub const ADD_ROW: &str = "Zeile hinzufügen";
/// Hinweis am Löschsymbol einer Zeile.
pub const DELETE_ROW: &str = "Zeile löschen";
/// Knopf der Rückfrage bei einem benutzten Parameter.
pub const REPLACE: &str = "Durch den Wert ersetzen";
/// Zweiter Knopf der Rückfrage.
pub const CANCEL: &str = "Abbrechen";
/// Hinweis bei leerer Tabelle.
pub const EMPTY_TABLE: &str = "Noch keine Parameter.";
/// Hinweis in der Statuszeile, wenn ein Bearbeiten-Fenster wegen einer Parameteränderung
/// schließt (seine Felder zeigten sonst alte Werte und Formeln).
pub const DIALOG_CLOSED: &str = "Der Dialog wurde geschlossen, weil sich Parameter geändert haben.";
/// Spaltenköpfe in der Reihenfolge der Tabelle.
pub const COLUMNS: [&str; 5] = ["Name", "Formel", "Wert", "Art", "Kommentar"];

/// Breiten der Spalten in Punkten (Name, Formel samt „fx“ und Wertanzeige, Wert, Art,
/// Kommentar, Löschsymbol); zusammen mit den Abständen und dem Rollbalken passt die Tabelle in
/// die Innenbreite des breiten Fensters (720 px).
const NAME_W: f32 = 110.0;
const FORMULA_CELL_W: f32 = 230.0;
const VALUE_W: f32 = 80.0;
const UNIT_W: f32 = 84.0;
const COMMENT_W: f32 = 100.0;
const DELETE_W: f32 = 24.0;
const WIDTHS: [f32; 5] = [NAME_W, FORMULA_CELL_W, VALUE_W, UNIT_W, COMMENT_W];
/// Breite des Formeltexts im Formelfeld.
const FORMULA_W: f32 = 108.0;
/// Höchste Höhe des Rollbereichs der Tabelle; im niedrigen Fenster weniger.
const TABLE_MAX_H: f32 = 420.0;

/// Spalten mit Text, die eine Zeile übernimmt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Name,
    Formula,
    Comment,
}

/// Entwurf einer Zeile mit dem Stand, aus dem er gelesen wurde.
#[derive(Clone, Debug)]
struct Row {
    id: ParamId,
    name: String,
    formula: String,
    comment: String,
    base: Parameter,
}

impl Row {
    fn of(p: &Parameter) -> Row {
        Row {
            id: p.id,
            name: p.name.clone(),
            formula: p.formula.clone(),
            comment: p.comment.clone(),
            base: p.clone(),
        }
    }

    /// Zeile aus dem neuen Stand `p`; Spalten, in denen `old` einen eigenen Entwurf hatte
    /// (anders als sein Stand), behalten ihn.
    fn refreshed(p: &Parameter, old: &Row) -> Row {
        let keep = |draft: &str, base: &str, now: &str| {
            if draft != base { draft } else { now }.to_string()
        };
        Row {
            id: p.id,
            name: keep(&old.name, &old.base.name, &p.name),
            formula: keep(&old.formula, &old.base.formula, &p.formula),
            comment: keep(&old.comment, &old.base.comment, &p.comment),
            base: p.clone(),
        }
    }

    fn text_mut(&mut self, column: Column) -> &mut String {
        match column {
            Column::Name => &mut self.name,
            Column::Formula => &mut self.formula,
            Column::Comment => &mut self.comment,
        }
    }
}

/// Was in einem Bild angeklickt oder übernommen wurde (nach dem Zeichnen ausgeführt).
enum Action {
    Commit(ParamId),
    Unit(ParamId, ParamUnit),
    Delete(ParamId),
    Add,
    Replace,
    Cancel,
}

/// Fenster „Parameter“.
pub struct ParameterDialog {
    rows: Vec<Row>,
    error: Option<String>,
    /// Offene Rückfrage beim Löschen: Parameter und Meldung („Breite wird in Quader 1 benutzt.“).
    confirm: Option<(ParamId, String)>,
    /// Stand, aus dem die Zeilen gelesen wurden: Zustand (`AppState::instance`) und
    /// Änderungszähler des Dokuments. Weicht er ab, lädt das Fenster neu.
    seen: (u64, u64),
    /// Rechteck im letzten Bild (für Tests).
    pub last_rect: Option<egui::Rect>,
}

impl ParameterDialog {
    pub fn new(state: &AppState) -> ParameterDialog {
        let mut d = ParameterDialog {
            rows: Vec::new(),
            error: None,
            confirm: None,
            seen: (0, 0),
            last_rect: None,
        };
        d.reload(state);
        d
    }

    /// Liest die Zeilen neu aus dem Dokument; Entwürfe, Meldung und Rückfrage entfallen (nach
    /// Rückgängig, Wiederholen, Zurückladen, anderem Dokument).
    pub fn reload(&mut self, state: &AppState) {
        self.rows = state.document.parameters().iter().map(Row::of).collect();
        self.error = None;
        self.confirm = None;
        self.seen = stand(state);
    }

    /// Nach einer gewöhnlichen Änderung am selben Dokument: Zeilen aus dem neuen Stand, eigene
    /// Entwürfe noch bestehender Zeilen bleiben (außer bei `done`, deren Entwurf gerade
    /// übernommen wurde). Eine Rückfrage zu einem gelöschten Parameter entfällt.
    fn refresh(&mut self, state: &AppState, done: Option<ParamId>) {
        let old = std::mem::take(&mut self.rows);
        self.rows = state
            .document
            .parameters()
            .iter()
            .map(|p| match old.iter().find(|r| r.id == p.id) {
                Some(r) if Some(p.id) != done => Row::refreshed(p, r),
                _ => Row::of(p),
            })
            .collect();
        if self
            .confirm
            .as_ref()
            .is_some_and(|(id, _)| state.document.parameter(*id).is_none())
        {
            self.confirm = None;
        }
        self.seen = stand(state);
    }

    /// Kennungen der Zeilen in Tabellenreihenfolge.
    pub fn row_ids(&self) -> Vec<ParamId> {
        self.rows.iter().map(|r| r.id).collect()
    }

    /// Entwurf einer Spalte (wie im Feld).
    pub fn text(&self, id: ParamId, column: Column) -> Option<&str> {
        let r = self.rows.iter().find(|r| r.id == id)?;
        Some(match column {
            Column::Name => &r.name,
            Column::Formula => &r.formula,
            Column::Comment => &r.comment,
        })
    }

    /// Setzt den Entwurf einer Spalte (wie Tippen; wirkt erst mit [`ParameterDialog::commit`]).
    pub fn set_text(&mut self, id: ParamId, column: Column, text: &str) {
        if let Some(r) = self.rows.iter_mut().find(|r| r.id == id) {
            *r.text_mut(column) = text.to_string();
        }
    }

    /// Übernimmt die ganze Zeile (Name, Formel, Kommentar) mit **einer** Änderung. Abgelehnt:
    /// Meldung im Fenster, Entwurf bleibt, Dokument unverändert.
    pub fn commit(&mut self, id: ParamId, state: &mut AppState) -> Result<(), String> {
        let Some(row) = self.rows.iter().find(|r| r.id == id) else {
            return Err(self.gone(state));
        };
        let Some(unit) = state.document.parameter(id).map(|p| p.unit) else {
            return Err(self.gone(state));
        };
        let edit = ParamEdit {
            name: row.name.trim().to_string(),
            formula: row.formula.trim().to_string(),
            unit,
            comment: row.comment.clone(),
        };
        let result = state.edit_parameter(id, edit);
        self.finish(state, result, Some(id))
    }

    /// Ändert die Art einer Zeile sofort (Name, Formel und Kommentar wie im Dokument).
    pub fn set_unit(
        &mut self,
        id: ParamId,
        unit: ParamUnit,
        state: &mut AppState,
    ) -> Result<(), String> {
        let Some(p) = state.document.parameter(id).cloned() else {
            return Err(self.gone(state));
        };
        let edit = ParamEdit {
            name: p.name,
            formula: p.formula,
            unit,
            comment: p.comment,
        };
        let result = state.edit_parameter(id, edit);
        self.finish(state, result, None)
    }

    /// „Zeile hinzufügen“: neue Zeile mit Vorgaben (Plan P13).
    pub fn add_row(&mut self, state: &mut AppState) -> Result<ParamId, String> {
        let result = state.add_parameter();
        let id = result.as_ref().ok().copied();
        self.finish(state, result.map(|_| ()), None)?;
        Ok(id.expect("gelungen"))
    }

    /// Löschsymbol einer Zeile: Nennt keine Formel den Parameter, wird er gelöscht; sonst öffnet
    /// die Rückfrage ([`ParameterDialog::confirm_text`]). Bei offener Skizze gesperrt.
    pub fn request_delete(&mut self, id: ParamId, state: &mut AppState) -> Result<(), String> {
        let Some(p) = state.document.parameter(id) else {
            return Err(self.gone(state));
        };
        let users = state.document.parameter_users(id);
        if state.sketch_session.is_none() && !users.is_empty() {
            let text = FeatureError::ParameterInUse {
                name: p.name.clone(),
                users,
            }
            .to_string();
            self.error = None;
            self.confirm = Some((id, text));
            return Ok(());
        }
        let result = state.delete_parameter(id);
        self.finish(state, result, None)
    }

    /// Meldung der offenen Rückfrage.
    pub fn confirm_text(&self) -> Option<&str> {
        self.confirm.as_ref().map(|(_, t)| t.as_str())
    }

    /// „Durch den Wert ersetzen“ in der Rückfrage (ein Schritt; abgelehnt etwa, solange ein
    /// Benutzer einen Fehler hat: Meldung im Fenster).
    pub fn replace_confirmed(&mut self, state: &mut AppState) -> Result<(), String> {
        let Some((id, _)) = self.confirm.take() else {
            return Ok(());
        };
        let result = state.replace_parameter(id);
        self.finish(state, result, None)
    }

    /// „Abbrechen“ in der Rückfrage: nichts ändert sich.
    pub fn cancel_delete(&mut self) {
        self.confirm = None;
    }

    /// Meldung unter der Tabelle (zuletzt abgelehnte Änderung).
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Nach einer Aktion: gelungen → Zeilen aus dem neuen Stand (Umbenennen schreibt auch andere
    /// Formeln um; der Entwurf von `done` ist übernommen), Meldung weg; abgelehnt → Meldung,
    /// Entwürfe bleiben.
    fn finish(
        &mut self,
        state: &AppState,
        result: Result<(), String>,
        done: Option<ParamId>,
    ) -> Result<(), String> {
        match result {
            Ok(()) => {
                self.refresh(state, done);
                self.error = None;
                Ok(())
            }
            Err(e) => {
                self.error = Some(e.clone());
                Err(e)
            }
        }
    }

    /// Die Zeile gibt es im Dokument nicht mehr: neu laden.
    fn gone(&mut self, state: &AppState) -> String {
        self.reload(state);
        let e = "Den Parameter gibt es nicht mehr.".to_string();
        self.error = Some(e.clone());
        e
    }

    /// Zeigt das Fenster und führt aus, was darin angeklickt oder übernommen wurde; `true`,
    /// solange es offen bleiben soll. `read_only`: alte Version.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState, read_only: bool) -> bool {
        let (open, actions) = self.draw(ctx, state, read_only);
        self.apply(actions, state);
        open
    }

    /// Zeichnet das Fenster; liefert, ob es offen bleibt, und die Aktionen dieses Bildes (bei
    /// Sperre keine).
    fn draw(
        &mut self,
        ctx: &egui::Context,
        state: &AppState,
        read_only: bool,
    ) -> (bool, Vec<Action>) {
        // Anderer Zustand (Datei neu geladen, anderes Dokument): Kennungen gelten nicht mehr.
        // Gewöhnliche Änderung am selben Dokument: Entwürfe bleiben.
        let now = stand(state);
        if self.seen.0 != now.0 {
            self.reload(state);
        } else if self.seen.1 != now.1 {
            self.refresh(state, None);
        }
        let lock = if read_only {
            Some(READ_ONLY_HINT)
        } else if state.sketch_session.is_some() {
            Some(SKETCH_OPEN_LOCK)
        } else {
            None
        };
        let mut open = true;
        let mut actions = Vec::new();
        let mut footer_actions = Vec::new();
        let confirm = self.confirm.as_ref().map(|(_, t)| t.clone());
        let error = self.error.clone();
        let table_h = (ctx.content_rect().height() - 260.0).clamp(120.0, TABLE_MAX_H);
        let shown = Dialog::new(TITLE, DialogSize::Wide)
            .id(egui::Id::new("parameter_dialog"))
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| {
                    if let Some(hint) = lock {
                        ui.label(
                            egui::RichText::new(hint)
                                .color(theme::palette(ui.visuals().dark_mode).warning),
                        );
                        ui.add_space(SPACE_2);
                    }
                    ui.add_enabled_ui(lock.is_none(), |ui| {
                        self.table(ui, state, table_h, &mut actions);
                    });
                },
                |ui| {
                    footer(
                        ui,
                        lock.is_none(),
                        confirm.as_deref(),
                        error.as_deref(),
                        &mut footer_actions,
                    )
                },
            );
        self.last_rect = shown.map(|r| r.response.rect);
        actions.append(&mut footer_actions);
        if lock.is_some() {
            actions.clear();
        }
        (open, actions)
    }

    /// Führt die Aktionen eines Bildes aus; Meldungen stehen danach im Fenster (`error`).
    fn apply(&mut self, actions: Vec<Action>, state: &mut AppState) {
        for a in actions {
            let _ = match a {
                Action::Commit(id) => self.commit(id, state),
                Action::Unit(id, unit) => self.set_unit(id, unit, state),
                Action::Delete(id) => self.request_delete(id, state),
                Action::Add => self.add_row(state).map(|_| ()),
                Action::Replace => self.replace_confirmed(state),
                Action::Cancel => {
                    self.cancel_delete();
                    Ok(())
                }
            };
        }
    }

    /// Kopfzeile und Zeilen in einem Rollbereich.
    fn table(&mut self, ui: &mut egui::Ui, state: &AppState, max_h: f32, out: &mut Vec<Action>) {
        if self.rows.is_empty() {
            ui.weak(EMPTY_TABLE);
            return;
        }
        let dark = ui.visuals().dark_mode;
        let params = state.document.parameters();
        theme::scroll_area(
            ui,
            egui::ScrollArea::vertical()
                .id_salt("parameter_tabelle")
                .max_height(max_h)
                .auto_shrink([false, true]),
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(SPACE_2, SPACE_2);
                ui.horizontal(|ui| {
                    for (head, w) in COLUMNS.iter().zip(WIDTHS) {
                        cell(
                            ui,
                            w,
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new(*head)
                                        .font(theme::label_font())
                                        .color(theme::palette(dark).text_tertiary),
                                );
                            },
                        );
                    }
                });
                for (i, row) in self.rows.iter_mut().enumerate() {
                    let Some(p) = params.get(i).filter(|p| p.id == row.id) else {
                        continue;
                    };
                    ui.horizontal_top(|ui| row_ui(ui, row, p, i, state, out));
                }
            },
        );
    }
}

/// Eine Zelle fester Breite `w` (mindestens Feldhöhe hoch), damit die Spalten untereinander
/// stehen.
fn cell<R>(
    ui: &mut egui::Ui,
    w: f32,
    layout: egui::Layout,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.allocate_ui_with_layout(egui::vec2(w, theme::FIELD_H), layout, |ui| {
        ui.set_min_size(egui::vec2(w, theme::FIELD_H));
        ui.set_max_width(w);
        add(ui)
    })
    .inner
}

/// Rückfrage, „Zeile hinzufügen“ und die Meldung; bleibt unter dem rollenden Inhalt stehen.
fn footer(
    ui: &mut egui::Ui,
    enabled: bool,
    confirm: Option<&str>,
    error: Option<&str>,
    out: &mut Vec<Action>,
) {
    let dark = ui.visuals().dark_mode;
    ui.add_space(SPACE_2);
    if let Some(text) = confirm {
        ui.label(text);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(enabled, egui::Button::new(REPLACE))
                .clicked()
            {
                out.push(Action::Replace);
            }
            if ui.add_enabled(enabled, egui::Button::new(CANCEL)).clicked() {
                out.push(Action::Cancel);
            }
        });
        ui.add_space(SPACE_2);
    }
    if ui
        .add_enabled(enabled, icons::button(Icon::NewTab, ADD_ROW))
        .clicked()
    {
        out.push(Action::Add);
    }
    if let Some(e) = error {
        ui.add_space(SPACE_2);
        ui.colored_label(theme::palette(dark).error, e);
    }
}

/// Zeigt das Fenster „Parameter“ des Tabs (falls offen). Eine Parameteränderung ist ein eigener
/// Schritt: Eine noch offene Änderung (etwa ein Ziehen, während das Feld die Tastatur hielt)
/// wird vorher ein Schritt, danach folgt [`DocTab::after_parameter_change`].
pub fn show_in_tab(ctx: &egui::Context, tab: &mut DocTab) {
    let Some(mut dialog) = tab.parameter_dialog.take() else {
        return;
    };
    let (open, actions) = dialog.draw(ctx, &tab.state, tab.read_only);
    if !actions.is_empty() {
        tab.observe_history(false);
        let before = tab.state.document.edit_count();
        dialog.apply(actions, &mut tab.state);
        if tab.state.document.edit_count() != before {
            tab.after_parameter_change();
        }
    }
    if open {
        tab.parameter_dialog = Some(dialog);
    }
}

/// Bearbeiten → Parameter …: öffnet das Fenster (ein offenes bleibt, wie es ist).
pub fn open(tab: &mut DocTab) {
    if tab.parameter_dialog.is_none() {
        tab.parameter_dialog = Some(ParameterDialog::new(&tab.state));
    }
}

/// Stand des Dokuments, aus dem die Zeilen stammen.
fn stand(state: &AppState) -> (u64, u64) {
    (state.instance, state.document.edit_count())
}

/// Eine Zeile: Name, Formel (Formelfeld), Wert, Art, Kommentar, Löschsymbol.
fn row_ui(
    ui: &mut egui::Ui,
    row: &mut Row,
    p: &Parameter,
    index: usize,
    state: &AppState,
    out: &mut Vec<Action>,
) {
    let id = row.id;
    let center = egui::Layout::left_to_right(egui::Align::Center);
    let name = cell(ui, NAME_W, center, |ui| {
        theme::text_field(
            ui,
            egui::TextEdit::singleline(&mut row.name)
                .id_salt(("parameter_name", id.0))
                .desired_width(NAME_W),
        )
    });
    if commit_signal(ui, &name) {
        out.push(Action::Commit(id));
    }
    // Die Formel der Zeile rechnet mit einer Probetabelle: die übrigen Zeilen wie im Dokument,
    // diese mit dem Entwurf (sonst nähme sie ihren alten Wert, und ein Selbstbezug sähe gültig
    // aus). Unverändert: der Wert aus dem Dokument.
    let committed = state.document.parameter_value(id);
    let eval = |text: &str| -> Result<f64, String> {
        if text == p.formula
            && let Some(v) = committed
        {
            return Ok(v);
        }
        trial_value(state.document.parameters(), index, text)
    };
    let top = egui::Layout::top_down(egui::Align::Min);
    let field = cell(ui, FORMULA_CELL_W, top, |ui| {
        ui.push_id(("parameter_formel", id.0), |ui| {
            formula_field_with(ui, &mut row.formula, p.unit, &eval, FORMULA_W)
        })
        .inner
    });
    if field.committed || field.fx_clicked {
        out.push(Action::Commit(id));
    }
    cell(ui, VALUE_W, center, |ui| {
        if let Some(v) = committed {
            ui.label(theme::value(unit_value(v, p.unit)));
        }
    });
    let mut unit = p.unit;
    cell(ui, UNIT_W, center, |ui| {
        egui::ComboBox::from_id_salt(("parameter_art", id.0))
            .selected_text(unit.label())
            .width(UNIT_W - SPACE_2)
            .show_ui(ui, |ui| {
                for u in ParamUnit::ALL {
                    ui.selectable_value(&mut unit, u, u.label());
                }
            });
    });
    if unit != p.unit {
        out.push(Action::Unit(id, unit));
    }
    let comment = cell(ui, COMMENT_W, center, |ui| {
        theme::text_field(
            ui,
            egui::TextEdit::singleline(&mut row.comment)
                .id_salt(("parameter_kommentar", id.0))
                .desired_width(COMMENT_W),
        )
    });
    if commit_signal(ui, &comment) {
        out.push(Action::Commit(id));
    }
    let delete = cell(ui, DELETE_W, center, |ui| {
        ui.add(
            egui::Button::new(icons::text(
                Icon::Delete,
                icons::LABEL_ICON_SIZE,
                egui::Color32::PLACEHOLDER,
            ))
            .frame(false),
        )
    })
    .on_hover_text(DELETE_ROW);
    // Das Symbol ist ein Zeichen der Symbolschrift; Bildschirmleser hören „Zeile löschen“.
    delete.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, DELETE_ROW));
    theme::focus_ring(ui, &delete);
    if delete.clicked() {
        out.push(Action::Delete(id));
    }
}

/// Wert der Zeile `index` mit dem Formelentwurf `text` (übrige Zeilen wie im Dokument). Fehler
/// einer anderen Zeile nennen deren Namen.
fn trial_value(params: &[Parameter], index: usize, text: &str) -> Result<f64, String> {
    let defs: Vec<ParamDef<'_>> = params
        .iter()
        .enumerate()
        .map(|(i, p)| ParamDef {
            name: &p.name,
            formula: if i == index { text } else { &p.formula },
            unit: p.unit,
        })
        .collect();
    match evaluate_table(&defs) {
        Ok(values) => Ok(values[index]),
        Err(e) if e.index == index => Err(e.message),
        Err(e) => match params.get(e.index) {
            Some(other) if !e.message.contains(&format!("„{}“", other.name)) => {
                Err(format!("Parameter „{}“: {}", other.name, e.message))
            }
            _ => Err(e.message),
        },
    }
}
