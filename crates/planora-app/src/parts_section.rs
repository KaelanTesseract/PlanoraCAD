//! Abschnitt „Stückliste“ im Fenster „Blattangaben“ (Teilprojekt 3c-2, Aufgabe 6): der Schalter
//! „Stückliste auf diesem Blatt“, je Zeile die eigenen Texte (Benennung, Menge, Werkstoff,
//! Bemerkung; die Pos. zählt das Programm) und darunter Papier und Knopf für das
//! Stücklistenblatt.
//!
//! Wie die übrigen Abschnitte arbeitet er auf Kopien: der Schalter steht im Blattsatz des
//! Fensters, die eigenen Texte in [`PartsDraft`]; erst „Übernehmen“ schreibt sie. Nur der Knopf
//! „Stücklistenblatt erzeugen/öffnen“ handelt sofort ([`create_or_open`]).

use crate::drawing_browser::open_drawing;
use crate::drawing_dialog::{PAPERS, drawing_menu_blocked, paper_label};
use crate::mesh_body::warning_color;
use crate::sheet_dialog::{CUT_WARNING, field_row_with, label_column_width};
use crate::state::AppState;
use crate::theme;
use planora_drawing::parts::{
    MAX_PARTS_LIST_ROWS, MAX_PARTS_MATERIAL_CHARS, MAX_PARTS_NAME_CHARS, MAX_PARTS_NOTE_CHARS,
    MAX_PARTS_QTY_CHARS, PARTS_ON_SHEET,
};
use planora_drawing::sheet_info::{FieldValue, SheetInfo, Stamp};
use planora_drawing::sheet_marks::parts_cell_is_cut;
use planora_drawing::{
    BodyKey, DrawingId, DrawingKind, Paper, PartsCells, PartsRow, RowOverride, Sheet, SheetLayout,
};
use std::collections::BTreeMap;

/// Überschrift des Abschnitts.
pub const SECTION: &str = "Stückliste";
/// Schalter der Stückliste auf dem Zeichnungsblatt.
pub const SWITCH: &str = "Stückliste auf diesem Blatt";
/// Hinweis bei mehr Zeilen, als auf das Zeichnungsblatt passen.
pub const TOO_MANY_ON_SHEET: &str =
    "Weitere Zeilen passen nicht aufs Blatt. Dafür gibt es das Stücklistenblatt.";
/// Hinweis bei mehr Zeilen, als auf die Stücklistenblätter passen.
pub const TOO_MANY_ROWS: &str = "Mehr als 200 Zeilen passen nicht auf die Stücklistenblätter.";
/// Text ohne Zeilen.
pub const NO_PARTS: &str = "Es gibt noch keine Teile.";
/// Knopf, solange es kein Stücklistenblatt gibt.
pub const CREATE: &str = "Stücklistenblatt erzeugen";
/// Knopf, wenn es das Stücklistenblatt schon gibt.
pub const OPEN: &str = "Stücklistenblatt öffnen";

/// Kopie der eigenen Zeilentexte und die Papierwahl für ein neues Stücklistenblatt.
#[derive(Clone, Debug)]
pub struct PartsDraft {
    pub overrides: BTreeMap<BodyKey, RowOverride>,
    pub paper: Paper,
    pub landscape: bool,
}

impl PartsDraft {
    /// Kopie der eigenen Texte des Dokuments; Papier A4 hoch.
    pub fn new(state: &AppState) -> PartsDraft {
        PartsDraft {
            overrides: state.document.parts_overrides().clone(),
            paper: Paper::A4,
            landscape: false,
        }
    }

    /// Blatt für ein neues Stücklistenblatt.
    pub fn sheet(&self) -> Sheet {
        Sheet {
            paper: self.paper,
            landscape: self.landscape,
        }
    }

    /// Die Schlüssel, deren Texte sich gegenüber `doc` unterscheiden, mit dem neuen Stand (ganz
    /// automatisch, wenn der Eintrag im Entwurf fehlt).
    pub fn changes(&self, doc: &BTreeMap<BodyKey, RowOverride>) -> Vec<(BodyKey, RowOverride)> {
        let mut keys: Vec<BodyKey> = self.overrides.keys().chain(doc.keys()).copied().collect();
        keys.sort();
        keys.dedup();
        keys.into_iter()
            .filter_map(|k| {
                let new = self.overrides.get(&k).cloned().unwrap_or_default();
                let old = doc.get(&k).cloned().unwrap_or_default();
                (new != old).then_some((k, new))
            })
            .collect()
    }
}

/// Legt das Stücklistenblatt (Seite 1 und die nötigen Folgeblätter) auf `sheet` an oder öffnet,
/// wenn es schon eines gibt, dessen Seite 1; danach steht es im Arbeitsbereich. Im Skizzen-Modus
/// geht es nicht.
pub fn create_or_open(state: &mut AppState, sheet: Sheet) -> Result<DrawingId, String> {
    if let Some(why) = drawing_menu_blocked(state) {
        return Err(why.into());
    }
    let id = match state.document.parts_sheets().first() {
        Some(id) => *id,
        None => state
            .document
            .add_parts_sheets(sheet, &Stamp::now())
            .map_err(|e| e.to_string())?,
    };
    if !open_drawing(state, id) {
        return Err(state.message.clone().unwrap_or_default());
    }
    Ok(id)
}

/// Grenze der Zeichen je Spalte (Benennung, Menge, Werkstoff, Bemerkung).
const LIMITS: [usize; 4] = [
    MAX_PARTS_NAME_CHARS,
    MAX_PARTS_QTY_CHARS,
    MAX_PARTS_MATERIAL_CHARS,
    MAX_PARTS_NOTE_CHARS,
];

/// Bezeichnungen der Spalten im Fenster.
pub(crate) const LABELS: [&str; 4] = ["Benennung", "Menge", "Werkstoff", "Bemerkung"];

/// Zeichnet den Abschnitt. `kind` ist die Art der Zeichnung des Fensters (der Schalter gehört nur
/// zu Zeichnungen mit Ansichten), `info` ihr Blattsatz (Entwurf), `rows` die Zeilen der
/// Stückliste, `auto` deren automatische Texte, `layout` die Blattgeometrie dieses Entwurfs (für
/// die Warnung „gekürzt“), `has_sheets` ob es schon ein Stücklistenblatt gibt. Liefert `true`,
/// wenn der Knopf „Stücklistenblatt erzeugen/öffnen“ geklickt wurde.
#[allow(clippy::too_many_arguments)]
pub fn show_section(
    ui: &mut egui::Ui,
    dark: bool,
    kind: DrawingKind,
    info: &mut SheetInfo,
    draft: &mut PartsDraft,
    rows: &[PartsRow],
    auto: &[PartsCells],
    layout: &SheetLayout,
    has_sheets: bool,
) -> bool {
    theme::section_header(ui, SECTION, theme::SUB_HEADER_H);
    ui.add_space(theme::SPACE_1);
    let views = matches!(kind, DrawingKind::Views);
    if views {
        ui.add(theme::Toggle::new(&mut info.parts_list, SWITCH));
    }
    let warn = |ui: &mut egui::Ui, text: &str| {
        ui.add(egui::Label::new(egui::RichText::new(text).color(warning_color(dark))).wrap());
    };
    if rows.is_empty() {
        ui.weak(NO_PARTS);
    }
    if views && info.parts_list && rows.len() > PARTS_ON_SHEET {
        warn(ui, TOO_MANY_ON_SHEET);
    }
    if rows.len() > MAX_PARTS_LIST_ROWS {
        warn(ui, TOO_MANY_ROWS);
    }
    let label_w = label_column_width(ui);
    // Gekürzt wird nur, was in einer Tabelle dieses Blatts steht.
    let cut =
        |col: usize, text: &str| layout.parts.is_some() && parts_cell_is_cut(col, text, layout);
    for (i, (row, auto)) in rows.iter().zip(auto).take(MAX_PARTS_LIST_ROWS).enumerate() {
        let Some(key) = row.keys.first().copied() else {
            continue;
        };
        ui.add_space(theme::SPACE_2);
        // Die Pos. zählt das Programm: nur Text, kein Feld.
        ui.strong(format!("Pos. {}", row.pos));
        let mut o = draft.overrides.get(&key).cloned().unwrap_or_default();
        egui::Grid::new(("planora-parts-row", i))
            .num_columns(2)
            .spacing([12.0, 8.0])
            .min_col_width(label_w)
            .show(ui, |ui| {
                let values: [&mut FieldValue; 4] =
                    [&mut o.name, &mut o.qty, &mut o.material, &mut o.note];
                for (c, value) in values.into_iter().enumerate() {
                    let shown = value.resolve(&auto[c + 1]).to_string();
                    let w = cut(c + 1, &shown).then_some((CUT_WARNING, false));
                    field_row_with(
                        ui,
                        dark,
                        ("planora-parts-cell", i, c),
                        LABELS[c],
                        value,
                        &auto[c + 1],
                        LIMITS[c],
                        w,
                    );
                }
            });
        if o.is_auto() {
            draft.overrides.remove(&key);
        } else {
            draft.overrides.insert(key, o);
        }
    }

    // Stücklistenblatt: Papierwahl (nur zum Anlegen) und Knopf.
    ui.add_space(theme::SPACE_2);
    let mut clicked = false;
    ui.horizontal(|ui| {
        if !has_sheets {
            ui.label("Papier");
            egui::ComboBox::from_id_salt("planora-parts-paper")
                .selected_text(paper_label(draft.paper))
                .show_ui(ui, |ui| {
                    for p in PAPERS {
                        ui.selectable_value(&mut draft.paper, p, paper_label(p));
                    }
                });
            ui.radio_value(&mut draft.landscape, false, "hoch");
            ui.radio_value(&mut draft.landscape, true, "quer");
        }
        if ui.button(if has_sheets { OPEN } else { CREATE }).clicked() {
            clicked = true;
        }
    });
    clicked
}
