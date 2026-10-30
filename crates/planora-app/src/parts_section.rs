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
use crate::icons::{self, Icon};
use crate::mesh_body::warning_color;
use crate::msg::Msg;
use crate::sheet_dialog::{cell, cut_warning, length_label};
use crate::state::AppState;
use crate::theme;
use planora_drawing::parts::{
    MAX_PARTS_LIST_ROWS, MAX_PARTS_MATERIAL_CHARS, MAX_PARTS_NAME_CHARS, MAX_PARTS_NOTE_CHARS,
    MAX_PARTS_QTY_CHARS, PARTS_HEAD, PARTS_ON_SHEET,
};
use planora_drawing::sheet_info::{FieldValue, SheetInfo, Stamp};
use planora_drawing::sheet_marks::parts_cell_is_cut;
use planora_drawing::{
    BodyKey, DrawingId, DrawingKind, Paper, PartsCells, PartsRow, RowOverride, Sheet, SheetLayout,
};
use planora_i18n::tr;
use std::collections::BTreeMap;

/// Überschrift des Abschnitts.
pub fn section() -> String {
    tr!("parts-section")
}
/// Schalter der Stückliste auf dem Zeichnungsblatt.
pub fn switch() -> String {
    tr!("parts-switch")
}
/// Hinweis bei mehr Zeilen, als auf das Zeichnungsblatt passen.
pub fn too_many_on_sheet() -> String {
    tr!("parts-too-many-on-sheet")
}
/// Hinweis bei mehr Zeilen, als auf die Stücklistenblätter passen.
pub fn too_many_rows() -> String {
    tr!("parts-too-many-rows", max = MAX_PARTS_LIST_ROWS)
}
/// Text ohne Zeilen.
pub fn no_parts() -> String {
    tr!("parts-none")
}
/// Knopf, solange es kein Stücklistenblatt gibt.
pub fn create() -> String {
    tr!("parts-create")
}
/// Knopf, wenn es das Stücklistenblatt schon gibt.
pub fn open() -> String {
    tr!("parts-open")
}

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
pub fn create_or_open(state: &mut AppState, sheet: Sheet) -> Result<DrawingId, Msg> {
    if let Some(why) = drawing_menu_blocked(state) {
        return Err(why);
    }
    let id = match state.document.parts_sheets().first() {
        Some(id) => *id,
        None => state
            .document
            .add_parts_sheets(sheet, &Stamp::now())
            .map_err(|e| Msg::Raw(e.to_string()))?,
    };
    if !open_drawing(state, id) {
        return Err(Msg::Raw(state.message.clone().unwrap_or_default()));
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
pub(crate) fn labels() -> [String; 4] {
    [
        tr!("parts-col-name"),
        tr!("parts-col-qty"),
        tr!("parts-col-material"),
        tr!("parts-col-note"),
    ]
}

/// Hinweis am Knopf, der die eigenen Texte einer Zeile entfernt.
pub fn reset_tip() -> String {
    tr!("keys-reset")
}

/// Breite der Spalte Pos. (nur Text).
const POS_W: f32 = 40.0;
/// Breite der Spalte Menge.
const QTY_W: f32 = 64.0;
/// Breite des Knopfs „Zurücksetzen“ am Zeilenende.
const RESET_W: f32 = 28.0;
/// Anteile von Benennung, Werkstoff und Bemerkung an der übrigen Breite (wie auf dem Blatt).
const SHARES: [f32; 3] = [78.0, 45.0, 30.0];

/// Spaltenbreiten der Zeilen (Pos., Benennung, Menge, Werkstoff, Bemerkung, Knopf) für die
/// Zeilenbreite `row_w` mit dem Abstand `gap` zwischen den Spalten.
fn column_widths(row_w: f32, gap: f32) -> [f32; 6] {
    let rest = (row_w - POS_W - QTY_W - RESET_W - 5.0 * gap).max(150.0);
    let total: f32 = SHARES.iter().sum();
    [
        POS_W,
        rest * SHARES[0] / total,
        QTY_W,
        rest * SHARES[1] / total,
        rest * SHARES[2] / total,
        RESET_W,
    ]
}

/// Zeichnet den Abschnitt. `kind` ist die Art der Zeichnung des Fensters (der Schalter gehört nur
/// zu Zeichnungen mit Ansichten), `info` ihr Blattsatz (Entwurf), `rows` die Zeilen der
/// Stückliste, `auto` deren automatische Texte, `layout` die Blattgeometrie dieses Entwurfs (für
/// die Warnung „gekürzt“), `has_sheets` ob es schon ein Stücklistenblatt gibt. Liefert `true`,
/// wenn der Knopf „Stücklistenblatt erzeugen/öffnen“ geklickt wurde.
///
/// Oben stehen Schalter, Papierwahl und Knopf, darunter die Zeilen als Tabelle (eine Zeile je
/// Stücklistenzeile, die Länge „n/max“ im Hinweis am Feld). Zeilen außerhalb des sichtbaren
/// Bereichs werden nur vermessen, nicht gezeichnet.
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
    theme::section_header(ui, &section(), theme::SUB_HEADER_H);
    ui.add_space(theme::SPACE_1);
    let views = matches!(kind, DrawingKind::Views);
    if views {
        ui.add(theme::Toggle::new(&mut info.parts_list, switch()));
    }
    // Stücklistenblatt: Papierwahl (nur zum Anlegen) und Knopf.
    let mut clicked = false;
    ui.horizontal(|ui| {
        if !has_sheets {
            ui.label(tr!("drawing-new-paper"));
            egui::ComboBox::from_id_salt("planora-parts-paper")
                .selected_text(paper_label(draft.paper))
                .show_ui(ui, |ui| {
                    for p in PAPERS {
                        ui.selectable_value(&mut draft.paper, p, paper_label(p));
                    }
                });
            ui.radio_value(&mut draft.landscape, false, tr!("drawing-portrait"));
            ui.radio_value(&mut draft.landscape, true, tr!("drawing-landscape"));
        }
        if ui
            .button(if has_sheets { open() } else { create() })
            .clicked()
        {
            clicked = true;
        }
    });
    let warn = |ui: &mut egui::Ui, text: &str| {
        ui.add(egui::Label::new(egui::RichText::new(text).color(warning_color(dark))).wrap());
    };
    if rows.is_empty() {
        ui.weak(no_parts());
        return clicked;
    }
    if views && info.parts_list && rows.len() > PARTS_ON_SHEET {
        warn(ui, &too_many_on_sheet());
    }
    if rows.len() > MAX_PARTS_LIST_ROWS {
        warn(ui, &too_many_rows());
    }
    ui.add_space(theme::SPACE_1);

    // Spalten wie die Änderungstabelle: feste Breiten, Kopf bündig mit den Feldern, rechts Platz
    // für den Rollbalken des Fensters.
    let gap = ui.spacing().item_spacing.x;
    let scroll = &ui.spacing().scroll;
    let gutter = scroll.bar_width + scroll.bar_inner_margin + scroll.bar_outer_margin + gap;
    let widths = column_widths(ui.available_width() - gutter, gap);
    let row_w: f32 = widths.iter().sum::<f32>() + 5.0 * gap;
    ui.horizontal(|ui| {
        for (c, head) in PARTS_HEAD.iter().enumerate() {
            cell(ui, widths[c], |ui| {
                ui.weak(*head);
            });
        }
    });
    // Gekürzt wird nur, was in einer Tabelle dieses Blatts steht.
    let cut =
        |col: usize, text: &str| layout.parts.is_some() && parts_cell_is_cut(col, text, layout);
    let row_h = theme::FIELD_H.max(ui.spacing().interact_size.y) + ui.spacing().item_spacing.y;
    let warn_h = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
    for (i, (row, auto)) in rows.iter().zip(auto).take(MAX_PARTS_LIST_ROWS).enumerate() {
        let Some(key) = row.keys.first().copied() else {
            continue;
        };
        let mut o = draft.overrides.get(&key).cloned().unwrap_or_default();
        let shown = |o: &RowOverride, c: usize| -> String {
            let v = [&o.name, &o.qty, &o.material, &o.note][c];
            v.resolve(&auto[c + 1]).to_string()
        };
        let is_cut = |o: &RowOverride| (0..4).any(|c| cut(c + 1, &shown(o, c)));
        // Unsichtbare Zeilen nur vermessen.
        let h = row_h + if is_cut(&o) { warn_h } else { 0.0 };
        let probe = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(row_w, h));
        if !ui.is_rect_visible(probe) {
            ui.allocate_space(egui::vec2(row_w, h - ui.spacing().item_spacing.y));
            continue;
        }
        let mut reset = false;
        ui.horizontal(|ui| {
            // Die Pos. zählt das Programm: nur Text, kein Feld.
            cell(ui, widths[0], |ui| {
                ui.label(row.pos.to_string());
            });
            let values: [&mut FieldValue; 4] =
                [&mut o.name, &mut o.qty, &mut o.material, &mut o.note];
            for (c, value) in values.into_iter().enumerate() {
                let mut text = match value {
                    FieldValue::Text(t) => t.clone(),
                    FieldValue::Auto => String::new(),
                };
                cell(ui, widths[c + 1], |ui| {
                    theme::text_field(
                        ui,
                        egui::TextEdit::singleline(&mut text)
                            .id_salt(("planora-parts-cell", i, c))
                            .hint_text(auto[c + 1].as_str())
                            .desired_width(widths[c + 1])
                            .char_limit(LIMITS[c]),
                    )
                    .on_hover_text(length_label(&text, LIMITS[c]));
                });
                *value = FieldValue::from_input(&text);
            }
            let custom = !o.is_auto();
            cell(ui, widths[5], |ui| {
                if ui
                    .add_visible(custom, egui::Button::new(icons::atom(Icon::Close)))
                    .on_hover_text(reset_tip())
                    .clicked()
                {
                    reset = true;
                }
            });
        });
        if reset {
            o = RowOverride::default();
        }
        if is_cut(&o) {
            // Unter den Feldern dieser Zeile.
            ui.horizontal(|ui| {
                ui.add_space(widths[0] + gap);
                ui.scope(|ui| {
                    ui.set_max_width(row_w - widths[0] - gap);
                    warn(ui, &cut_warning());
                });
            });
        }
        if o.is_auto() {
            draft.overrides.remove(&key);
        } else {
            draft.overrides.insert(key, o);
        }
    }
    clicked
}
