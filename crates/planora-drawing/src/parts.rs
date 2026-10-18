//! Stückliste: Körperkennung, Zeilen, Überschreibungen der Zeilentexte, Auflösung zu den
//! Zelltexten und die Aufteilung auf Stücklistenblätter. Reine Daten, keine Geometrie (die steht
//! in [`sheet_frame`](crate::sheet_frame) und [`sheet_marks`](crate::sheet_marks)).

use crate::sheet_info::{FieldValue, check_value};
use std::collections::BTreeMap;

/// Kennung eines Körpers über Änderungen am Teil hinweg: der Ursprungs-Körper (Feature und
/// Ergebnis-Index).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyKey {
    pub feature: u64,
    pub index: u32,
}

/// Eigene Texte einer Stücklistenzeile; jede Spalte automatisch oder eigener Text. Die Pos. ist
/// nie überschreibbar.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RowOverride {
    pub name: FieldValue,
    pub qty: FieldValue,
    pub material: FieldValue,
    pub note: FieldValue,
}

impl RowOverride {
    /// Alle Spalten automatisch (dann braucht es den Eintrag nicht).
    pub fn is_auto(&self) -> bool {
        [&self.name, &self.qty, &self.material, &self.note]
            .iter()
            .all(|v| **v == FieldValue::Auto)
    }
}

/// Eine Zeile der Stückliste: gleichnamige Körper zusammengefasst. `keys[0]` ist der Schlüssel
/// der Zeile (für Überschreibungen).
#[derive(Clone, Debug, PartialEq)]
pub struct PartsRow {
    pub pos: u32,
    pub name: String,
    pub qty: u32,
    pub keys: Vec<BodyKey>,
}

/// Die Texte einer Zeile: Pos., Benennung, Menge, Werkstoff, Bemerkung.
pub type PartsCells = [String; 5];

/// Feste Spaltennamen.
pub const PARTS_HEAD: [&str; 5] = ["Pos.", "Benennung", "Menge", "Werkstoff", "Bemerkung"];
pub const MAX_PARTS_NAME_CHARS: usize = 60;
pub const MAX_PARTS_QTY_CHARS: usize = 60;
pub const MAX_PARTS_MATERIAL_CHARS: usize = 60;
pub const MAX_PARTS_NOTE_CHARS: usize = 80;
/// Höchstens so viele Zeilen auf dem Zeichnungsblatt.
pub const PARTS_ON_SHEET: usize = 10;
/// Höchstens so viele Zeilen auf allen Stücklistenblättern zusammen.
pub const MAX_PARTS_LIST_ROWS: usize = 200;
/// Höchstens so viele Nummernkreise je Zeichnung.
pub const MAX_BALLOONS: usize = 200;
/// Höchstens so viele Überschreibungen je Dokument.
pub const MAX_OVERRIDES: usize = 1000;
/// Dokumentart der Stücklistenblätter.
pub const PARTS_DOC_TYPE: &str = "Stückliste";

/// Name des Stücklistenblatts `page` (ab 1): „Stückliste“, „Stückliste (2)“, …
pub fn parts_sheet_name(page: u32) -> String {
    if page <= 1 {
        PARTS_DOC_TYPE.to_string()
    } else {
        format!("{PARTS_DOC_TYPE} ({page})")
    }
}

/// Prüft die eigenen Texte einer Zeile: Längen in Zeichen und Steuerzeichen.
pub fn validate_override(o: &RowOverride) -> Result<(), String> {
    check_value("Die Benennung", &o.name, MAX_PARTS_NAME_CHARS)?;
    check_value("Die Menge", &o.qty, MAX_PARTS_QTY_CHARS)?;
    check_value("Der Werkstoff", &o.material, MAX_PARTS_MATERIAL_CHARS)?;
    check_value("Die Bemerkung", &o.note, MAX_PARTS_NOTE_CHARS)
}

/// Löst die Zeilen mit ihren Überschreibungen (unter dem ersten Schlüssel der Zeile) zu den
/// Zelltexten auf. Automatisch: Benennung = Name, Menge = Zahl, Werkstoff = `material_auto`,
/// Bemerkung leer.
pub fn resolve_parts(
    rows: &[PartsRow],
    overrides: &BTreeMap<BodyKey, RowOverride>,
    material_auto: &str,
) -> Vec<PartsCells> {
    let auto = RowOverride::default();
    rows.iter()
        .map(|r| {
            let o = r
                .keys
                .first()
                .and_then(|k| overrides.get(k))
                .unwrap_or(&auto);
            let qty = r.qty.to_string();
            [
                r.pos.to_string(),
                o.name.resolve(&r.name).to_string(),
                o.qty.resolve(&qty).to_string(),
                o.material.resolve(material_auto).to_string(),
                o.note.resolve("").to_string(),
            ]
        })
        .collect()
}

/// Zahl der Stücklistenblätter für `rows` Zeilen (auf [`MAX_PARTS_LIST_ROWS`] begrenzt) bei
/// `per_page` Zeilen je Blatt; mindestens 1.
pub fn pages_needed(rows: usize, per_page: usize) -> u32 {
    let rows = rows.min(MAX_PARTS_LIST_ROWS);
    (rows.div_ceil(per_page.max(1)) as u32).max(1)
}

/// Die Zeilen des Stücklistenblatts `page` (ab 1); nur die ersten [`MAX_PARTS_LIST_ROWS`]
/// zählen.
pub fn page_rows(cells: &[PartsCells], page: u32, per_page: usize) -> &[PartsCells] {
    let all = &cells[..cells.len().min(MAX_PARTS_LIST_ROWS)];
    let start = (page.max(1) as usize - 1).saturating_mul(per_page);
    let start = start.min(all.len());
    let end = start.saturating_add(per_page).min(all.len());
    &all[start..end]
}
