//! Fenster „Blattangaben“ (Zeichnungsblatt, Aufgabe 6): die Angaben des Schriftfelds in drei
//! Abschnitten – „Projekt“ (gelten für alle Blätter), „Dieses Blatt“ und „Änderungen“. Das Fenster
//! arbeitet auf Kopien; erst „Übernehmen“ schreibt ins Dokument (ein Schritt), „Abbrechen“ und das
//! Schließen-Symbol verwerfen. Fehlen einer alten Zeichnung die Angaben, bekommt sie Satz und
//! Projektangaben erst mit „Übernehmen“.
//!
//! Jedes Feld ist „automatisch“, solange es leer ist: Der automatische Wert steht als Hinweistext
//! im leeren Feld, „Zurücksetzen“ leert ein eigenes Feld wieder.

use crate::browser::error_color;
use crate::dialog::{Dialog, DialogSize};
use crate::drawing_view;
use crate::icons::{self, Icon};
use crate::mesh_body::warning_color;
use crate::state::AppState;
use crate::theme;
use planora_drawing::sheet::SheetModel;
use planora_drawing::sheet_frame::TitleField;
use planora_drawing::sheet_info::{
    DEFAULT_TOLERANCE, FieldValue, MAX_FIELD_CHARS, MAX_NOTE_CHARS, MAX_NOTES, MAX_REVISION_CHARS,
    MAX_TITLE_CHARS, NotesValue, ProjectInfo, SheetContext, SheetInfo, Stamp, TOO_MANY_REVISIONS,
    TitleTexts, default_notes, main_scale, resolve_texts, revision_letter, validate_project_info,
    validate_sheet_info,
};
use planora_drawing::sheet_marks::field_is_cut;
use planora_drawing::{Drawing, DrawingId};

/// Titel des Fensters.
pub const TITLE: &str = "Blattangaben";
/// Beschriftung des Knopfs im Werkzeugfeld.
pub const BUTTON: &str = "Blattangaben";
/// Hinweis am Knopf.
pub const BUTTON_TIP: &str =
    "Titel, Zeichnungsnummer, Änderungen und weitere Angaben des Schriftfelds bearbeiten.";
/// Eintrag im Menü „Zeichnung“.
pub const MENU_ENTRY: &str = "Blattangaben …";
/// Hinweis am gesperrten Menüeintrag, wenn keine Zeichnung offen ist.
pub const NO_DRAWING_OPEN: &str = "Dafür zuerst eine Zeichnung öffnen.";
/// Warnung bei leerem Eigentümer.
pub const OWNER_MISSING: &str = "Bitte den Eigentümer eintragen. Er gehört auf jede Zeichnung.";
/// Warnung bei einem Eintrag, der nicht ganz ins Feld des Schriftfelds passt.
pub const CUT_WARNING: &str = "Passt nicht ganz ins Feld und wird auf dem Blatt gekürzt.";
/// Meldung, wenn es die Zeichnung nicht mehr gibt.
const GONE: &str = "Die Zeichnung gibt es nicht mehr.";
/// Hinweis am gesperrten Knopf „Zeile hinzufügen“.
const NOTES_FULL: &str = "Es sind höchstens 6 Hinweiszeilen möglich.";

/// Breite der Textfelder der Angaben.
const FIELD_W: f32 = 280.0;
/// Breite einer Hinweiszeile.
const NOTE_W: f32 = 440.0;
/// Breiten der Spalten der Änderungstabelle.
const REV_LETTER_W: f32 = 56.0;
const REV_TEXT_W: f32 = 190.0;
const REV_DATE_W: f32 = 92.0;
const REV_NAME_W: f32 = 110.0;

/// Bezeichnung eines Felds des Schriftfelds in der Oberfläche und in Meldungen.
pub fn title_field_name(field: TitleField) -> &'static str {
    match field {
        TitleField::Owner => "Eigentümer",
        TitleField::Number => "Zeichnungsnummer",
        TitleField::Title => "Titel",
        TitleField::Subtitle => "Zusatztitel",
        TitleField::DocType => "Dokumentart",
        TitleField::Status => "Dokumentstatus",
        TitleField::Issued => "Ausgabedatum",
        TitleField::Revision => "Änderungsstand",
        TitleField::SheetOf => "Blatt",
        TitleField::Format => "Format",
        TitleField::Scale => "Maßstab",
        TitleField::Creator => "Erstellt",
        TitleField::Checked => "Geprüft",
        TitleField::Approved => "Freigabe",
        TitleField::Material => "Werkstoff",
        TitleField::Weight => "Gewicht",
        TitleField::Language => "Sprache",
    }
}

/// Fenster „Blattangaben“.
#[derive(Clone, Debug)]
pub struct SheetInfoDialog {
    pub drawing: DrawingId,
    /// Kopie der Projektangaben (bei fehlenden mit den Vorgaben).
    pub project: ProjectInfo,
    /// Kopie der Blattangaben (bei fehlenden mit den Vorgaben).
    pub sheet: SheetInfo,
    /// Meldung (zu viele Änderungen, Fehler beim Übernehmen).
    pub message: Option<String>,
    pub last_rect: Option<egui::Rect>,
}

/// Öffnet das Fenster für die Zeichnung `id` (ersetzt ein offenes). Gibt es die Zeichnung nicht,
/// passiert nichts. Das Dokument bleibt dabei unberührt.
pub fn open(state: &mut AppState, id: DrawingId) {
    if state.document.drawing(id).is_none() {
        return;
    }
    state.sheet_dialog = Some(SheetInfoDialog::new(state, id, &Stamp::now()));
}

impl SheetInfoDialog {
    /// Entwurf aus dem Dokument; fehlende Angaben mit den Vorgaben von heute (`stamp`).
    pub fn new(state: &AppState, id: DrawingId, stamp: &Stamp) -> SheetInfoDialog {
        let project = state
            .document
            .project_info()
            .cloned()
            .unwrap_or_else(|| ProjectInfo::new(stamp));
        let sheet = state
            .document
            .drawing(id)
            .and_then(|d| d.info.clone())
            .unwrap_or_else(|| SheetInfo::new(stamp));
        SheetInfoDialog {
            drawing: id,
            project,
            sheet,
            message: None,
            last_rect: None,
        }
    }

    /// Schreibt Projekt- und Blattangaben ins Dokument. Gleiche Werte ändern nichts.
    pub fn apply(&self, state: &mut AppState) -> Result<(), String> {
        validate_project_info(&self.project)?;
        validate_sheet_info(&self.sheet)?;
        if state.document.drawing(self.drawing).is_none() {
            return Err(GONE.into());
        }
        state
            .document
            .set_project_info(self.project.clone())
            .map_err(|e| e.to_string())?;
        state
            .document
            .set_sheet_info(self.drawing, self.sheet.clone())
            .map_err(|e| e.to_string())
    }

    /// Hängt eine Änderung mit dem Datum von heute und dem Namen `creator` an; ab der 24. gibt
    /// es stattdessen die Meldung.
    pub fn add_revision(&mut self, stamp: &Stamp, creator: &str) {
        match self.sheet.add_revision(&stamp.date, creator) {
            Ok(()) => self.message = None,
            Err(m) => self.message = Some(m.to_string()),
        }
    }

    /// Die Texte des Schriftfelds mit allen Feldern automatisch (für die Hinweistexte) und mit
    /// den Eingaben (für die Warnungen).
    fn texts(
        &self,
        drawing: &Drawing,
        ctx: &SheetContext,
        main: Option<planora_drawing::Scale>,
    ) -> (TitleTexts, TitleTexts) {
        let mut auto_sheet = SheetInfo::new(&Stamp {
            date: self.sheet.created.clone(),
            user: String::new(),
        });
        auto_sheet.revisions = self.sheet.revisions.clone();
        let mut auto_drawing = drawing.clone();
        auto_drawing.info = Some(auto_sheet);
        let auto_project = ProjectInfo::new(&Stamp {
            date: String::new(),
            user: self.project.login.clone(),
        });
        let auto = resolve_texts(&auto_drawing, Some(&auto_project), ctx, main);
        let mut own_drawing = drawing.clone();
        own_drawing.info = Some(self.sheet.clone());
        let actual = resolve_texts(&own_drawing, Some(&self.project), ctx, main);
        (auto, actual)
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll. Es schließt sich auch, wenn
    /// es die Zeichnung nicht mehr gibt.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let Some(drawing) = state.document.drawing(self.drawing).cloned() else {
            return false;
        };
        let stamp = Stamp::now();
        let sheet_ctx = drawing_view::sheet_context(state, self.drawing).unwrap_or(SheetContext {
            doc_name: state.document_name.clone(),
            index: 1,
            count: 1,
        });
        // Maßstab der Hauptansicht aus dem zuletzt gezeichneten Blatt (sonst der erste Ansicht).
        let model = drawing_view::last_sheet(ctx).unwrap_or_else(|| {
            std::sync::Arc::new(SheetModel {
                sheet: drawing.sheet,
                frame: None,
                views: Vec::new(),
            })
        });
        let main = main_scale(&drawing, &model);
        let (auto, actual) = self.texts(&drawing, &sheet_ctx, main);
        let creator = self
            .project
            .creator
            .resolve(&self.project.login)
            .to_string();
        let tolerance = self
            .project
            .tolerance
            .resolve(DEFAULT_TOLERANCE)
            .to_string();
        let (mut open, mut close, mut apply, mut add_rev) = (true, false, false, false);
        let mut delete_rev: Option<usize> = None;
        let Self {
            project,
            sheet,
            message,
            last_rect,
            ..
        } = self;
        let response = Dialog::new(TITLE, DialogSize::Wide)
            .open(&mut open)
            .show(ctx, |ui| {
                let dark = ui.visuals().dark_mode;
                let warn = |f: TitleField| -> Option<(&'static str, bool)> {
                    if f == TitleField::Owner && actual.get(f).trim().is_empty() {
                        return Some((OWNER_MISSING, true));
                    }
                    field_is_cut(f, actual.get(f)).then_some((CUT_WARNING, false))
                };

                // ---- Projekt ----
                theme::section_header(ui, "Projekt", theme::SUB_HEADER_H);
                ui.add_space(theme::SPACE_1);
                egui::Grid::new("planora-sheet-project")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        let rows: [(&str, &mut FieldValue, TitleField); 5] = [
                            ("Eigentümer", &mut project.owner, TitleField::Owner),
                            ("Erstellt", &mut project.creator, TitleField::Creator),
                            ("Geprüft", &mut project.checked, TitleField::Checked),
                            ("Freigabe", &mut project.approved, TitleField::Approved),
                            ("Werkstoff", &mut project.material, TitleField::Material),
                        ];
                        for (label, value, f) in rows {
                            field_row(
                                ui,
                                dark,
                                label,
                                value,
                                auto.get(f),
                                MAX_FIELD_CHARS,
                                warn(f),
                            );
                        }
                        field_row(
                            ui,
                            dark,
                            "Allgemeintoleranz",
                            &mut project.tolerance,
                            DEFAULT_TOLERANCE,
                            MAX_FIELD_CHARS,
                            None,
                        );
                    });
                ui.add_space(theme::SPACE_2);
                ui.label("Hinweise für alle Blätter");
                let effective = match &project.notes {
                    NotesValue::Lines(l) => l.clone(),
                    NotesValue::Auto => default_notes(&tolerance),
                };
                let mut lines = effective.clone();
                lines_editor(ui, "project-note", &mut lines);
                if lines != effective {
                    project.notes = NotesValue::Lines(lines);
                }
                if matches!(project.notes, NotesValue::Lines(_))
                    && ui.button("Hinweise zurücksetzen").clicked()
                {
                    project.notes = NotesValue::Auto;
                }
                let project_notes = match &project.notes {
                    NotesValue::Lines(l) => l.clone(),
                    NotesValue::Auto => default_notes(&tolerance),
                };

                // ---- Dieses Blatt ----
                ui.add_space(theme::SPACE_3);
                theme::section_header(ui, "Dieses Blatt", theme::SUB_HEADER_H);
                ui.add_space(theme::SPACE_1);
                egui::Grid::new("planora-sheet-own")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        let rows: [(&str, &mut FieldValue, TitleField, usize); 11] = [
                            (
                                "Zeichnungsnummer",
                                &mut sheet.number,
                                TitleField::Number,
                                MAX_FIELD_CHARS,
                            ),
                            (
                                "Titel",
                                &mut sheet.title,
                                TitleField::Title,
                                MAX_TITLE_CHARS,
                            ),
                            (
                                "Zusatztitel",
                                &mut sheet.subtitle,
                                TitleField::Subtitle,
                                MAX_FIELD_CHARS,
                            ),
                            (
                                "Dokumentart",
                                &mut sheet.doc_type,
                                TitleField::DocType,
                                MAX_FIELD_CHARS,
                            ),
                            (
                                "Dokumentstatus",
                                &mut sheet.status,
                                TitleField::Status,
                                MAX_FIELD_CHARS,
                            ),
                            (
                                "Ausgabedatum",
                                &mut sheet.issued,
                                TitleField::Issued,
                                MAX_FIELD_CHARS,
                            ),
                            (
                                "Änderungsstand",
                                &mut sheet.revision,
                                TitleField::Revision,
                                MAX_FIELD_CHARS,
                            ),
                            (
                                "Blatt",
                                &mut sheet.sheet_of,
                                TitleField::SheetOf,
                                MAX_FIELD_CHARS,
                            ),
                            (
                                "Format",
                                &mut sheet.format,
                                TitleField::Format,
                                MAX_FIELD_CHARS,
                            ),
                            (
                                "Maßstab",
                                &mut sheet.scale,
                                TitleField::Scale,
                                MAX_FIELD_CHARS,
                            ),
                            (
                                "Gewicht",
                                &mut sheet.weight,
                                TitleField::Weight,
                                MAX_FIELD_CHARS,
                            ),
                        ];
                        for (label, value, f, max) in rows {
                            field_row(ui, dark, label, value, auto.get(f), max, warn(f));
                        }
                    });
                ui.add_space(theme::SPACE_2);
                let was_own = matches!(sheet.notes, NotesValue::Lines(_));
                let mut own = was_own;
                ui.add(theme::Toggle::new(
                    &mut own,
                    "Eigene Hinweise für dieses Blatt",
                ));
                if own != was_own {
                    sheet.notes = if own {
                        NotesValue::Lines(project_notes.clone())
                    } else {
                        NotesValue::Auto
                    };
                }
                if let NotesValue::Lines(lines) = &mut sheet.notes {
                    lines_editor(ui, "sheet-note", lines);
                }

                // ---- Änderungen ----
                ui.add_space(theme::SPACE_3);
                theme::section_header(ui, "Änderungen", theme::SUB_HEADER_H);
                ui.add_space(theme::SPACE_1);
                if sheet.revisions.is_empty() {
                    ui.weak("Noch keine Änderungen.");
                } else {
                    egui::Grid::new("planora-sheet-revisions")
                        .num_columns(5)
                        .spacing([8.0, 8.0])
                        .show(ui, |ui| {
                            for h in ["Buchstabe", "Beschreibung", "Datum", "Name"] {
                                ui.weak(h);
                            }
                            ui.label("");
                            ui.end_row();
                            for (i, r) in sheet.revisions.iter_mut().enumerate() {
                                let auto_letter =
                                    revision_letter(i).map(String::from).unwrap_or_default();
                                let mut letter = match &r.letter {
                                    FieldValue::Text(t) => t.clone(),
                                    FieldValue::Auto => String::new(),
                                };
                                theme::text_field(
                                    ui,
                                    egui::TextEdit::singleline(&mut letter)
                                        .id_salt(("planora-rev-letter", i))
                                        .hint_text(auto_letter)
                                        .desired_width(REV_LETTER_W)
                                        .char_limit(MAX_FIELD_CHARS),
                                );
                                r.letter = FieldValue::from_input(&letter);
                                theme::text_field(
                                    ui,
                                    egui::TextEdit::singleline(&mut r.description)
                                        .id_salt(("planora-rev-text", i))
                                        .desired_width(REV_TEXT_W)
                                        .char_limit(MAX_REVISION_CHARS),
                                );
                                theme::text_field(
                                    ui,
                                    egui::TextEdit::singleline(&mut r.date)
                                        .id_salt(("planora-rev-date", i))
                                        .desired_width(REV_DATE_W)
                                        .char_limit(MAX_FIELD_CHARS),
                                );
                                theme::text_field(
                                    ui,
                                    egui::TextEdit::singleline(&mut r.name)
                                        .id_salt(("planora-rev-name", i))
                                        .desired_width(REV_NAME_W)
                                        .char_limit(MAX_FIELD_CHARS),
                                );
                                if ui.button("Löschen").clicked() {
                                    delete_rev = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                }
                ui.add_space(theme::SPACE_1);
                if ui
                    .add(icons::button(Icon::AddView, "Änderung hinzufügen"))
                    .clicked()
                {
                    add_rev = true;
                }
                let too_many = message.as_deref() == Some(TOO_MANY_REVISIONS);
                if too_many {
                    ui.colored_label(error_color(dark), TOO_MANY_REVISIONS);
                }

                // ---- Knöpfe ----
                ui.add_space(theme::SPACE_2);
                if let Some(m) = message.as_deref().filter(|_| !too_many) {
                    ui.colored_label(error_color(dark), m);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Übernehmen").clicked() {
                        apply = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        close = true;
                    }
                });
            });
        *last_rect = response.map(|r| r.response.rect);
        if let Some(i) = delete_rev
            && i < self.sheet.revisions.len()
        {
            self.sheet.revisions.remove(i);
            self.message = None;
        }
        if add_rev {
            self.add_revision(&stamp, &creator);
        }
        if apply {
            match self.apply(state) {
                Ok(()) => return false,
                Err(e) => self.message = Some(e),
            }
        }
        open && !close
    }
}

/// Eine Zeile des Fensters: Bezeichnung, Textfeld (mit dem automatischen Wert als Hinweistext),
/// Länge „n/max“, „Zurücksetzen“ bei eigenem Text und darunter die Warnung des Felds.
fn field_row(
    ui: &mut egui::Ui,
    dark: bool,
    label: &str,
    value: &mut FieldValue,
    auto: &str,
    max: usize,
    warn: Option<(&'static str, bool)>,
) {
    ui.label(label);
    let mut text = match value {
        FieldValue::Text(t) => t.clone(),
        FieldValue::Auto => String::new(),
    };
    let custom = matches!(value, FieldValue::Text(_));
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            theme::text_field(
                ui,
                egui::TextEdit::singleline(&mut text)
                    .id_salt(("planora-sheet-field", label))
                    .hint_text(auto)
                    .desired_width(FIELD_W)
                    .char_limit(max),
            );
            let n = text.chars().count();
            if n > 0 {
                ui.weak(format!("{n}/{max}"));
            }
            if ui
                .add_visible(custom, egui::Button::new("Zurücksetzen"))
                .clicked()
            {
                text.clear();
            }
        });
        if let Some((message, error)) = warn {
            let color = if error {
                error_color(dark)
            } else {
                warning_color(dark)
            };
            ui.colored_label(color, message);
        }
    });
    *value = FieldValue::from_input(&text);
    ui.end_row();
}

/// Hinweiszeilen: je Zeile ein Textfeld mit „Zeile löschen“, darunter „Zeile hinzufügen“ (bis
/// sechs Zeilen).
fn lines_editor(ui: &mut egui::Ui, id: &str, lines: &mut Vec<String>) {
    let mut delete = None;
    for (i, line) in lines.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            theme::text_field(
                ui,
                egui::TextEdit::singleline(line)
                    .id_salt(("planora-sheet-note", id, i))
                    .desired_width(NOTE_W)
                    .char_limit(MAX_NOTE_CHARS),
            );
            if ui.button("Zeile löschen").clicked() {
                delete = Some(i);
            }
        });
    }
    if let Some(i) = delete {
        lines.remove(i);
    }
    let full = lines.len() >= MAX_NOTES;
    if ui
        .add_enabled(!full, egui::Button::new("Zeile hinzufügen"))
        .on_disabled_hover_text(NOTES_FULL)
        .clicked()
    {
        lines.push(String::new());
    }
}
