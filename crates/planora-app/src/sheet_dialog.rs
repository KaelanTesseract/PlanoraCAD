//! Fenster „Blattangaben“ (Zeichnungsblatt, Aufgabe 6): die Angaben des Schriftfelds in vier
//! Abschnitten – „Projekt“ (gelten für alle Blätter), „Dieses Blatt“, „Änderungen“ und
//! „Stückliste“ ([`crate::parts_section`]). Das Fenster arbeitet auf Kopien; erst „Übernehmen“
//! schreibt ins Dokument, „Abbrechen“ und das Schließen-Symbol verwerfen. Fehlen einer alten
//! Zeichnung die Angaben, bekommt sie Satz und Projektangaben erst mit „Übernehmen“. Ein
//! Stücklistenblatt hat kein Hinweisfeld: dort fehlt der Schalter für eigene Hinweise.
//!
//! Jedes Feld ist „automatisch“, solange es leer ist: Der automatische Wert steht als Hinweistext
//! im leeren Feld, „Zurücksetzen“ leert ein eigenes Feld wieder.

use crate::browser::error_color;
use crate::dialog::{Dialog, DialogSize};
use crate::drawing_view;
use crate::icons::{self, Icon};
use crate::mesh_body::warning_color;
use crate::msg::Msg;
use crate::parts_section::{self, PartsDraft};
use crate::state::AppState;
use crate::theme;
use planora_drawing::parts::{MAX_OVERRIDES, resolve_parts, validate_override};
use planora_drawing::sheet_frame::TitleField;
use planora_drawing::sheet_info::{
    DASH, DEFAULT_TOLERANCE, FieldValue, MAX_FIELD_CHARS, MAX_NOTE_CHARS, MAX_NOTES,
    MAX_REVISION_CHARS, MAX_REVISIONS, MAX_TITLE_CHARS, NotesValue, ProjectInfo, SheetContext,
    SheetInfo, Stamp, TOO_MANY_REVISIONS, TitleTexts, default_notes, main_scale, resolve_texts,
    revision_letter, validate_project_info, validate_sheet_info,
};
use planora_drawing::sheet_marks::{field_is_cut, note_is_cut, revision_cell_is_cut};
use planora_drawing::{Drawing, DrawingId, DrawingKind, PartsRow};
use planora_i18n::tr;
use std::collections::BTreeMap;

/// Titel des Fensters.
pub fn title() -> String {
    tr!("sheet-title")
}
/// Beschriftung des Knopfs im Werkzeugfeld.
pub fn button() -> String {
    tr!("sheet-button")
}
/// Hinweis am Knopf.
pub fn button_tip() -> String {
    tr!("sheet-button-tip")
}
/// Eintrag im Menü „Zeichnung“.
pub fn menu_entry() -> String {
    tr!("sheet-menu-entry")
}
/// Hinweis am gesperrten Menüeintrag, wenn keine Zeichnung offen ist.
pub fn no_drawing_open() -> String {
    tr!("sheet-no-drawing-open")
}
/// Warnung bei leerem Eigentümer.
pub fn owner_missing() -> String {
    tr!("sheet-owner-missing")
}
/// Warnung bei einem Eintrag, der nicht ganz ins Feld des Schriftfelds passt.
pub fn cut_warning() -> String {
    tr!("sheet-cut-warning")
}
/// Hinweis im Abschnitt „Änderungen“: nur die letzten drei stehen auf dem Blatt.
pub fn revisions_on_sheet() -> String {
    tr!("sheet-revisions-on-sheet")
}
/// Meldung, wenn es die Zeichnung nicht mehr gibt.
fn gone() -> Msg {
    crate::msg!("sheet-gone")
}
/// Hinweis am gesperrten Knopf „Zeile hinzufügen“.
fn notes_full() -> String {
    tr!("sheet-notes-full", max = MAX_NOTES)
}

/// Breite der Textfelder der Angaben.
const FIELD_W: f32 = 280.0;
/// Breite einer Hinweiszeile.
const NOTE_W: f32 = 440.0;
/// Breiten der Spalten der Änderungstabelle; die Beschreibung nimmt den Rest der Zeile, aber
/// mindestens [`REV_TEXT_MIN_W`]. Das Datum hat Platz für „JJJJ-MM-TT“.
const REV_LETTER_W: f32 = 50.0;
const REV_TEXT_MIN_W: f32 = 280.0;
const REV_DATE_W: f32 = 110.0;
const REV_NAME_W: f32 = 140.0;
/// Schmalste Spalte des Namens, wenn der Platz für Beschreibung und Rollbalken knapp wird.
const REV_NAME_MIN_W: f32 = 120.0;

/// Bezeichnung eines Felds des Schriftfelds in der Oberfläche und in Meldungen.
pub fn title_field_name(field: TitleField) -> String {
    match field {
        TitleField::Owner => tr!("sheet-field-owner"),
        TitleField::Number => tr!("sheet-field-number"),
        TitleField::Title => tr!("sheet-field-title"),
        TitleField::Subtitle => tr!("sheet-field-subtitle"),
        TitleField::DocType => tr!("sheet-field-doc-type"),
        TitleField::Status => tr!("sheet-field-status"),
        TitleField::Issued => tr!("sheet-field-issued"),
        TitleField::Revision => tr!("sheet-field-revision"),
        TitleField::SheetOf => tr!("sheet-field-sheet-of"),
        TitleField::Format => tr!("sheet-field-format"),
        TitleField::Scale => tr!("sheet-field-scale"),
        TitleField::Creator => tr!("sheet-field-creator"),
        TitleField::Checked => tr!("sheet-field-checked"),
        TitleField::Approved => tr!("sheet-field-approved"),
        TitleField::Material => tr!("sheet-field-material"),
        TitleField::Weight => tr!("sheet-field-weight"),
        TitleField::Language => tr!("sheet-field-language"),
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
    /// Kopie der eigenen Texte der Stückliste und Papierwahl für das Stücklistenblatt.
    pub parts: PartsDraft,
    /// Meldung (zu viele Änderungen, Fehler beim Übernehmen); übersetzt erst beim Zeichnen.
    pub message: Option<Msg>,
    pub last_rect: Option<egui::Rect>,
    /// Zeilen der Stückliste und ob es Stücklistenblätter gibt, je Stand des Dokuments
    /// (Zustand, Änderungszähler) einmal gerechnet.
    parts_cache: Option<PartsCache>,
}

#[derive(Clone, Debug)]
struct PartsCache {
    instance: u64,
    edit: u64,
    rows: Vec<PartsRow>,
    has_sheets: bool,
}

/// Öffnet das Fenster für die Zeichnung `id` (ersetzt ein offenes). Gibt es die Zeichnung nicht,
/// passiert nichts. Das Dokument bleibt dabei unberührt.
pub fn open(state: &mut AppState, id: DrawingId) {
    if state.document.drawing(id).is_none() {
        return;
    }
    // Ist es für dieselbe Zeichnung schon offen, bleibt es (die Eingaben gehen nicht verloren).
    if state.sheet_dialog.as_ref().is_some_and(|d| d.drawing == id) {
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
            parts: PartsDraft::new(state),
            message: None,
            last_rect: None,
            parts_cache: None,
        }
    }

    /// Schreibt Projekt- und Blattangaben ins Dokument, danach die geänderten eigenen Texte der
    /// Stückliste (je Zeile). Gleiche Werte ändern nichts. Alles wird vorher geprüft, damit ein
    /// Fehler nichts halb schreibt.
    pub fn apply(&self, state: &mut AppState) -> Result<(), Msg> {
        validate_project_info(&self.project).map_err(Msg::Raw)?;
        validate_sheet_info(&self.sheet).map_err(Msg::Raw)?;
        for o in self.parts.overrides.values() {
            validate_override(o)
                .map_err(|e| crate::msg!("sheet-parts-error", reason = e.to_string()))?;
        }
        if self.parts.overrides.len() > MAX_OVERRIDES {
            return Err(crate::msg!(
                "sheet-parts-too-many-overrides",
                max = MAX_OVERRIDES
            ));
        }
        if state.document.drawing(self.drawing).is_none() {
            return Err(gone());
        }
        state
            .document
            .set_project_info(self.project.clone())
            .map_err(|e| Msg::Raw(e.to_string()))?;
        state
            .document
            .set_sheet_info(self.drawing, self.sheet.clone())
            .map_err(|e| Msg::Raw(e.to_string()))?;
        // Erst die entfernten Einträge, dann die übrigen: so wird die Grenze nie kurz
        // überschritten.
        let mut changes = self.parts.changes(state.document.parts_overrides());
        changes.sort_by_key(|(_, o)| !o.is_auto());
        for (key, o) in changes {
            state
                .document
                .set_parts_override(key, o)
                .map_err(|e| Msg::Raw(e.to_string()))?;
        }
        Ok(())
    }

    /// Hängt eine Änderung mit dem Datum von heute und dem Namen `creator` an; ab der 24. gibt
    /// es stattdessen die Meldung.
    pub fn add_revision(&mut self, stamp: &Stamp, creator: &str) {
        match self.sheet.add_revision(&stamp.date, creator) {
            Ok(()) => self.message = None,
            Err(m) if m == TOO_MANY_REVISIONS => {
                self.message = Some(crate::msg!("sheet-too-many-revisions", max = MAX_REVISIONS));
            }
            Err(m) => self.message = Some(Msg::Raw(m.to_string())),
        }
    }

    /// Die Texte des Schriftfelds mit allen Feldern automatisch (für die Hinweistexte) und mit
    /// den Eingaben (für die Warnungen). `drawing` ist eine Kopie der Zeichnung; danach trägt
    /// sie den Blattsatz des Entwurfs.
    fn texts(
        &self,
        drawing: &mut Drawing,
        ctx: &SheetContext,
        main: Option<planora_drawing::Scale>,
    ) -> (TitleTexts, TitleTexts) {
        let mut auto_sheet = SheetInfo::new(&Stamp {
            date: self.sheet.created.clone(),
            user: String::new(),
        });
        auto_sheet.revisions = self.sheet.revisions.clone();
        drawing.info = Some(auto_sheet);
        let auto_project = ProjectInfo::new(&Stamp {
            date: String::new(),
            user: self.project.login.clone(),
        });
        let auto = resolve_texts(drawing, Some(&auto_project), ctx, main);
        drawing.info = Some(self.sheet.clone());
        let actual = resolve_texts(drawing, Some(&self.project), ctx, main);
        (auto, actual)
    }

    /// Zeilen der Stückliste und ob es Stücklistenblätter gibt (aus dem Zwischenspeicher, solange
    /// sich das Dokument nicht geändert hat).
    fn parts_rows(&mut self, state: &AppState) -> (Vec<PartsRow>, bool) {
        let edit = state.document.edit_count();
        let fresh = self
            .parts_cache
            .as_ref()
            .is_some_and(|c| c.instance == state.instance && c.edit == edit);
        if !fresh {
            self.parts_cache = Some(PartsCache {
                instance: state.instance,
                edit,
                rows: state.document.parts_rows(),
                has_sheets: !state.document.parts_sheets().is_empty(),
            });
        }
        let c = self.parts_cache.as_ref().expect("eben gesetzt");
        (c.rows.clone(), c.has_sheets)
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll. Es schließt sich auch, wenn
    /// es die Zeichnung nicht mehr gibt.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        // Eine Kopie der Zeichnung je Bild; sie trägt danach den Blattsatz des Entwurfs.
        let Some(mut drawing) = state.document.drawing(self.drawing).cloned() else {
            return false;
        };
        let stamp = Stamp::now();
        let sheet_ctx = drawing_view::context_or_single(state, self.drawing);
        // Maßstab der ersten Ansicht (Grundansicht).
        let main = main_scale(&drawing);
        let (auto, actual) = self.texts(&mut drawing, &sheet_ctx, main);
        let (parts_rows, has_sheets) = self.parts_rows(state);
        // Blattgeometrie dieses Entwurfs (mit seinem Schalter der Stückliste): Zellbreiten,
        // Hinweisfeld und Stückliste für die Warnungen.
        let parts_space = drawing_view::parts_space_for(&drawing, parts_rows.len(), false);
        let layout = drawing_view::layout_from_texts(&drawing, &actual, parts_space);
        // Automatischer Werkstoff: der des Entwurfs der Projektangaben.
        let parts_auto = resolve_parts(
            &parts_rows,
            &BTreeMap::new(),
            self.project.material.resolve(DASH),
        );
        let kind = drawing.kind;
        let parts_sheet = matches!(kind, DrawingKind::PartsList { .. });
        let note_cut = |s: &str| note_is_cut(s, &layout);
        let first_visible = actual
            .revisions
            .len()
            .saturating_sub(planora_drawing::sheet_frame::REV_VISIBLE);
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
        let mut parts_button = false;
        let mut delete_rev: Option<usize> = None;
        let Self {
            project,
            sheet,
            parts,
            message,
            last_rect,
            ..
        } = self;
        let response = Dialog::new(title(), DialogSize::Wide)
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| {
                    let dark = ui.visuals().dark_mode;
                    let warn = |f: TitleField| -> Option<(String, bool)> {
                        if f == TitleField::Owner && actual.get(f).trim().is_empty() {
                            return Some((owner_missing(), true));
                        }
                        field_is_cut(f, actual.get(f), &layout).then(|| (cut_warning(), false))
                    };

                    // ---- Projekt ----
                    theme::section_header(ui, &tr!("sheet-section-project"), theme::SUB_HEADER_H);
                    ui.add_space(theme::SPACE_1);
                    // Eine Bezeichnungsspalte für alle Abschnitte: die Felder beginnen überall gleich.
                    let label_w = label_column_width(ui);
                    egui::Grid::new("planora-sheet-project")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .min_col_width(label_w)
                        .show(ui, |ui| {
                            let rows: [(&mut FieldValue, TitleField); 5] = [
                                (&mut project.owner, TitleField::Owner),
                                (&mut project.creator, TitleField::Creator),
                                (&mut project.checked, TitleField::Checked),
                                (&mut project.approved, TitleField::Approved),
                                (&mut project.material, TitleField::Material),
                            ];
                            for (value, f) in rows {
                                field_row(
                                    ui,
                                    dark,
                                    f,
                                    &title_field_name(f),
                                    value,
                                    auto.get(f),
                                    MAX_FIELD_CHARS,
                                    warn(f),
                                );
                            }
                            field_row(
                                ui,
                                dark,
                                "tolerance",
                                &tr!("sheet-field-tolerance"),
                                &mut project.tolerance,
                                DEFAULT_TOLERANCE,
                                MAX_FIELD_CHARS,
                                None,
                            );
                        });
                    ui.add_space(theme::SPACE_2);
                    ui.label(tr!("sheet-notes-all"));
                    let effective = match &project.notes {
                        NotesValue::Lines(l) => l.clone(),
                        NotesValue::Auto => default_notes(&tolerance),
                    };
                    let mut lines = effective.clone();
                    lines_editor(ui, dark, "project-note", &mut lines, &note_cut);
                    if lines != effective {
                        project.notes = NotesValue::Lines(lines);
                    }
                    if matches!(project.notes, NotesValue::Lines(_))
                        && ui.button(tr!("sheet-notes-reset")).clicked()
                    {
                        project.notes = NotesValue::Auto;
                    }
                    let project_notes = match &project.notes {
                        NotesValue::Lines(l) => l.clone(),
                        NotesValue::Auto => default_notes(&tolerance),
                    };

                    // ---- Dieses Blatt ----
                    ui.add_space(theme::SPACE_3);
                    theme::section_header(ui, &tr!("sheet-section-sheet"), theme::SUB_HEADER_H);
                    ui.add_space(theme::SPACE_1);
                    egui::Grid::new("planora-sheet-own")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .min_col_width(label_w)
                        .show(ui, |ui| {
                            let rows: [(&mut FieldValue, TitleField, usize); 11] = [
                                (&mut sheet.number, TitleField::Number, MAX_FIELD_CHARS),
                                (&mut sheet.title, TitleField::Title, MAX_TITLE_CHARS),
                                (&mut sheet.subtitle, TitleField::Subtitle, MAX_FIELD_CHARS),
                                (&mut sheet.doc_type, TitleField::DocType, MAX_FIELD_CHARS),
                                (&mut sheet.status, TitleField::Status, MAX_FIELD_CHARS),
                                (&mut sheet.issued, TitleField::Issued, MAX_FIELD_CHARS),
                                (&mut sheet.revision, TitleField::Revision, MAX_FIELD_CHARS),
                                (&mut sheet.sheet_of, TitleField::SheetOf, MAX_FIELD_CHARS),
                                (&mut sheet.format, TitleField::Format, MAX_FIELD_CHARS),
                                (&mut sheet.scale, TitleField::Scale, MAX_FIELD_CHARS),
                                (&mut sheet.weight, TitleField::Weight, MAX_FIELD_CHARS),
                            ];
                            for (value, f, max) in rows {
                                field_row(
                                    ui,
                                    dark,
                                    f,
                                    &title_field_name(f),
                                    value,
                                    auto.get(f),
                                    max,
                                    warn(f),
                                );
                            }
                        });
                    // Ein Stücklistenblatt hat kein Hinweisfeld.
                    if !parts_sheet {
                        ui.add_space(theme::SPACE_2);
                        let was_own = matches!(sheet.notes, NotesValue::Lines(_));
                        let mut own = was_own;
                        ui.add(theme::Toggle::new(&mut own, tr!("sheet-notes-own")));
                        if own != was_own {
                            sheet.notes = if own {
                                NotesValue::Lines(project_notes.clone())
                            } else {
                                NotesValue::Auto
                            };
                        }
                        if let NotesValue::Lines(lines) = &mut sheet.notes {
                            lines_editor(ui, dark, "sheet-note", lines, &note_cut);
                        }
                    }

                    // ---- Änderungen ----
                    ui.add_space(theme::SPACE_3);
                    theme::section_header(ui, &tr!("sheet-section-revisions"), theme::SUB_HEADER_H);
                    ui.add_space(theme::SPACE_1);
                    ui.weak(revisions_on_sheet());
                    ui.add_space(theme::SPACE_1);
                    if sheet.revisions.is_empty() {
                        ui.weak(tr!("sheet-no-revisions"));
                    } else {
                        // Feste Spaltenbreiten (Kopf und Zeilen in derselben Breite, also bündig):
                        // Buchstabe, Datum und Name fest, die Beschreibung nimmt den Rest.
                        let gap = ui.spacing().item_spacing.x;
                        let delete_w =
                            text_width(ui, &tr!("action-delete"), egui::TextStyle::Button)
                                + 2.0 * ui.spacing().button_padding.x;
                        // Die Spalte des Buchstabens ist mindestens so breit wie ihr Kopf.
                        let heads = [
                            tr!("sheet-rev-letter"),
                            tr!("sheet-rev-description"),
                            tr!("sheet-rev-date"),
                            tr!("sheet-rev-name"),
                        ];
                        let letter_w =
                            REV_LETTER_W.max(text_width(ui, &heads[0], egui::TextStyle::Body));
                        // Rechts bleibt Platz für den Rollbalken des Fensters, damit „Löschen“ ihn
                        // nicht berührt; reicht es dann nicht für die Beschreibung, wird die Spalte
                        // des Namens schmaler (bis REV_NAME_MIN_W).
                        let scroll = &ui.spacing().scroll;
                        let gutter = scroll.bar_width
                            + scroll.bar_inner_margin
                            + scroll.bar_outer_margin
                            + gap;
                        let row = ui.available_width() - gutter;
                        let fixed = letter_w + REV_DATE_W + delete_w + 4.0 * gap;
                        let name_w =
                            (row - fixed - REV_TEXT_MIN_W).clamp(REV_NAME_MIN_W, REV_NAME_W);
                        let text_w = (row - fixed - name_w).max(REV_TEXT_MIN_W);
                        let widths = [letter_w, text_w, REV_DATE_W, name_w];
                        ui.horizontal(|ui| {
                            for (h, w) in heads.iter().zip(widths) {
                                cell(ui, w, |ui| {
                                    ui.weak(h);
                                });
                            }
                        });
                        for (i, r) in sheet.revisions.iter_mut().enumerate() {
                            let auto_letter =
                                revision_letter(i).map(String::from).unwrap_or_default();
                            let mut letter = match &r.letter {
                                FieldValue::Text(t) => t.clone(),
                                FieldValue::Auto => String::new(),
                            };
                            // Gekürzt wird nur, was im Blatt steht (die letzten sechs).
                            let shown = actual.revisions.get(i).filter(|_| i >= first_visible);
                            let cut_row = shown.is_some_and(|row| {
                                row.iter()
                                    .enumerate()
                                    .any(|(c, text)| revision_cell_is_cut(c, text, &layout))
                            });
                            ui.horizontal(|ui| {
                                // Die Länge steht bei den Zellen der Tabelle im Hinweis am Feld
                                // (neben jeder Zelle wäre die Zeile zu breit).
                                cell(ui, widths[0], |ui| {
                                    theme::text_field(
                                        ui,
                                        egui::TextEdit::singleline(&mut letter)
                                            .id_salt(("planora-rev-letter", i))
                                            .hint_text(auto_letter)
                                            .desired_width(widths[0])
                                            .char_limit(MAX_FIELD_CHARS),
                                    )
                                    .on_hover_text(length_label(&letter, MAX_FIELD_CHARS));
                                });
                                cell(ui, widths[1], |ui| {
                                    theme::text_field(
                                        ui,
                                        egui::TextEdit::singleline(&mut r.description)
                                            .id_salt(("planora-rev-text", i))
                                            .desired_width(widths[1])
                                            .char_limit(MAX_REVISION_CHARS),
                                    )
                                    .on_hover_text(
                                        length_label(&r.description, MAX_REVISION_CHARS),
                                    );
                                });
                                cell(ui, widths[2], |ui| {
                                    theme::text_field(
                                        ui,
                                        egui::TextEdit::singleline(&mut r.date)
                                            .id_salt(("planora-rev-date", i))
                                            .desired_width(widths[2])
                                            .char_limit(MAX_FIELD_CHARS),
                                    )
                                    .on_hover_text(length_label(&r.date, MAX_FIELD_CHARS));
                                });
                                cell(ui, widths[3], |ui| {
                                    theme::text_field(
                                        ui,
                                        egui::TextEdit::singleline(&mut r.name)
                                            .id_salt(("planora-rev-name", i))
                                            .desired_width(widths[3])
                                            .char_limit(MAX_FIELD_CHARS),
                                    )
                                    .on_hover_text(length_label(&r.name, MAX_FIELD_CHARS));
                                });
                                if ui.button(tr!("action-delete")).clicked() {
                                    delete_rev = Some(i);
                                }
                            });
                            r.letter = FieldValue::from_input(&letter);
                            if cut_row {
                                // Unter der Beschreibung dieser Zeile.
                                ui.horizontal(|ui| {
                                    ui.add_space(widths[0] + gap);
                                    ui.scope(|ui| {
                                        ui.set_max_width(widths[1]);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(cut_warning())
                                                    .color(warning_color(dark)),
                                            )
                                            .wrap(),
                                        );
                                    });
                                });
                            }
                        }
                    }
                    ui.add_space(theme::SPACE_1);
                    if ui
                        .add(icons::button(Icon::AddView, tr!("sheet-rev-add")))
                        .clicked()
                    {
                        add_rev = true;
                    }
                    let too_many = is_too_many(message);
                    if let (true, Some(m)) = (too_many, message.as_ref()) {
                        ui.colored_label(error_color(dark), m.text());
                    }

                    // ---- Stückliste ----
                    ui.add_space(theme::SPACE_3);
                    parts_button = parts_section::show_section(
                        ui,
                        dark,
                        kind,
                        sheet,
                        parts,
                        &parts_rows,
                        &parts_auto,
                        &layout,
                        has_sheets,
                    );
                },
                // ---- Knöpfe (Fußzeile: immer zu sehen, auch bei langer Stückliste) ----
                |ui| {
                    let dark = ui.visuals().dark_mode;
                    let too_many = is_too_many(message);
                    if let Some(m) = message.as_ref().filter(|_| !too_many) {
                        ui.colored_label(error_color(dark), m.text());
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button(tr!("tree-apply")).clicked() {
                            apply = true;
                        }
                        if ui.button(tr!("common-cancel")).clicked() {
                            close = true;
                        }
                    });
                },
            );
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
        if parts_button {
            // Handelt sofort; das Fenster bleibt für sein Blatt offen.
            match parts_section::create_or_open(state, self.parts.sheet()) {
                Ok(_) => self.message = None,
                Err(e) => self.message = Some(e),
            }
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

/// Ist die Meldung die der zu vielen Änderungen (steht bei „Änderung hinzufügen“)?
fn is_too_many(message: &Option<Msg>) -> bool {
    matches!(message, Some(Msg::Id("sheet-too-many-revisions", _)))
}

/// Zelle fester Breite `w` (Spalte der Änderungstabelle), Inhalt links und senkrecht mittig.
pub(crate) fn cell(ui: &mut egui::Ui, w: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(w, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(w);
            ui.set_max_width(w);
            add(ui);
        },
    );
}

/// Breite eines Texts in der Schrift `style` (ohne Umbruch).
fn text_width(ui: &egui::Ui, text: &str, style: egui::TextStyle) -> f32 {
    egui::WidgetText::from(text)
        .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, style)
        .size()
        .x
}

/// Gemeinsame Breite der Bezeichnungsspalte aller Abschnitte: die längste Bezeichnung.
pub(crate) fn label_column_width(ui: &egui::Ui) -> f32 {
    labels()
        .iter()
        .chain(parts_section::labels().iter())
        .map(|l| text_width(ui, l, egui::TextStyle::Body))
        .fold(0.0, f32::max)
}

/// Alle Bezeichnungen der Felder in „Projekt“ und „Dieses Blatt“.
fn labels() -> Vec<String> {
    let mut all: Vec<String> = [
        TitleField::Owner,
        TitleField::Creator,
        TitleField::Checked,
        TitleField::Approved,
        TitleField::Material,
    ]
    .into_iter()
    .map(title_field_name)
    .collect();
    all.push(tr!("sheet-field-tolerance"));
    all.extend(
        [
            TitleField::Number,
            TitleField::Title,
            TitleField::Subtitle,
            TitleField::DocType,
            TitleField::Status,
            TitleField::Issued,
            TitleField::Revision,
            TitleField::SheetOf,
            TitleField::Format,
            TitleField::Scale,
            TitleField::Weight,
        ]
        .into_iter()
        .map(title_field_name),
    );
    all
}

/// Länge eines Eintrags als „n/max“ (Zeichen).
pub(crate) fn length_label(text: &str, max: usize) -> String {
    format!("{}/{max}", text.chars().count())
}

/// Eine Zeile des Fensters: Bezeichnung, Textfeld (mit dem automatischen Wert als Hinweistext),
/// Länge „n/max“, „Zurücksetzen“ bei eigenem Text und darunter die Warnung des Felds.
#[allow(clippy::too_many_arguments)]
fn field_row(
    ui: &mut egui::Ui,
    dark: bool,
    key: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    value: &mut FieldValue,
    auto: &str,
    max: usize,
    warn: Option<(String, bool)>,
) {
    field_row_with(
        ui,
        dark,
        ("planora-sheet-field", key),
        label,
        value,
        auto,
        max,
        warn,
    );
}

/// Wie [`field_row`] mit eigener Kennung des Textfelds `id` (für gleich benannte Felder, etwa
/// die Spalten mehrerer Stücklistenzeilen).
#[allow(clippy::too_many_arguments)]
pub(crate) fn field_row_with(
    ui: &mut egui::Ui,
    dark: bool,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    value: &mut FieldValue,
    auto: &str,
    max: usize,
    warn: Option<(String, bool)>,
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
                    .id_salt(id)
                    .hint_text(auto)
                    .desired_width(FIELD_W)
                    .char_limit(max),
            );
            ui.weak(length_label(&text, max));
            if ui
                .add_visible(custom, egui::Button::new(tr!("keys-reset")))
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
            ui.colored_label(color, &message);
        }
    });
    *value = FieldValue::from_input(&text);
    ui.end_row();
}

/// Hinweiszeilen: je Zeile ein Textfeld mit „Zeile löschen“, darunter „Zeile hinzufügen“ (bis
/// sechs Zeilen).
fn lines_editor(
    ui: &mut egui::Ui,
    dark: bool,
    id: &str,
    lines: &mut Vec<String>,
    is_cut: &dyn Fn(&str) -> bool,
) {
    let mut delete = None;
    for (i, line) in lines.iter_mut().enumerate() {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                theme::text_field(
                    ui,
                    egui::TextEdit::singleline(line)
                        .id_salt(("planora-sheet-note", id, i))
                        .desired_width(NOTE_W)
                        .char_limit(MAX_NOTE_CHARS),
                );
                ui.weak(length_label(line, MAX_NOTE_CHARS));
                if ui.button(tr!("sheet-line-delete")).clicked() {
                    delete = Some(i);
                }
            });
            if is_cut(line) {
                ui.colored_label(warning_color(dark), cut_warning());
            }
        });
    }
    if let Some(i) = delete {
        lines.remove(i);
    }
    let full = lines.len() >= MAX_NOTES;
    if ui
        .add_enabled(!full, egui::Button::new(tr!("sheet-line-add")))
        .on_disabled_hover_text(notes_full())
        .clicked()
    {
        lines.push(String::new());
    }
}
