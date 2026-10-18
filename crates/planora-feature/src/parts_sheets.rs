//! Stücklistenblätter als echte Zeichnungen: Seite 1 legt der Nutzer an, die Folgeblätter
//! entstehen und verschwinden mit der Zeilenzahl (nie beim Laden).

use crate::error::FeatureError;
use crate::model::Document;
use planora_drawing::parts::{pages_needed, parts_sheet_name};
use planora_drawing::sheet_frame::parts_rows_per_page;
use planora_drawing::sheet_info::{
    NotesValue, ProjectInfo, SheetInfo, Stamp, validate_project_info,
};
use planora_drawing::{Drawing, DrawingId, DrawingKind, Projection, Sheet};

pub const PARTS_SHEET_EXISTS: &str = "Es gibt schon ein Stücklistenblatt.";
pub const PARTS_NAME_TAKEN: &str = "Es gibt schon eine Zeichnung mit dem Namen „Stückliste“.";
pub const FOLLOW_PAGE_DELETE: &str = "Ein Folgeblatt der Stückliste lässt sich nicht einzeln löschen. Es verschwindet von selbst, wenn die Zeilen auf weniger Blätter passen.";
pub const PARTS_PAGE_NAME_TAKEN: &str = "Es gibt schon eine Zeichnung mit einem Namen wie „Stückliste (2)“. Bitte benenne sie zuerst um, damit die Folgeblätter der Stückliste Platz haben.";
pub const PARTS_SHEET_RENAME: &str = "Stücklistenblätter lassen sich nicht umbenennen.";

/// So viele Seitennamen („Stückliste (2)“ …) sind für Stücklistenblätter reserviert, solange es
/// welche gibt: 200 Zeilen auf dem kleinsten Blatt (26 Zeilen) ergeben höchstens 8 Seiten.
const RESERVED_PAGES: u32 = 8;

fn bad(m: impl Into<String>) -> FeatureError {
    FeatureError::InvalidInput(m.into())
}

pub(crate) fn page_of(d: &Drawing) -> Option<u32> {
    match d.kind {
        DrawingKind::PartsList { page } => Some(page),
        DrawingKind::Views => None,
    }
}

impl Document {
    /// Die Stücklistenblätter, nach Seitennummer.
    pub fn parts_sheets(&self) -> Vec<DrawingId> {
        let mut v: Vec<(u32, DrawingId)> = self
            .drawings
            .iter()
            .filter_map(|d| page_of(d).map(|p| (p, d.id)))
            .collect();
        v.sort();
        v.into_iter().map(|(_, id)| id).collect()
    }

    /// Ob der Name zu den Seitennamen gehört, die Stücklistenblättern vorbehalten sind (nur
    /// wirksam, solange es Stücklistenblätter gibt).
    pub(crate) fn reserved_for_parts(&self, name: &str) -> bool {
        let key = name.trim().to_lowercase();
        self.drawings.iter().any(|d| page_of(d).is_some())
            && (1..=RESERVED_PAGES).any(|p| parts_sheet_name(p).to_lowercase() == key)
    }

    /// Legt Seite 1 der Stückliste an (Erstwinkelprojektion, ohne Hinweisfeld) und gleicht die
    /// Folgeblätter ab. Zählt als eine Änderung.
    pub fn add_parts_sheets(
        &mut self,
        sheet: Sheet,
        stamp: &Stamp,
    ) -> Result<DrawingId, FeatureError> {
        if self.drawings.iter().any(|d| page_of(d).is_some()) {
            return Err(bad(PARTS_SHEET_EXISTS));
        }
        let name = parts_sheet_name(1);
        if self.name_taken(&name, None) {
            return Err(bad(PARTS_NAME_TAKEN));
        }
        if (2..=RESERVED_PAGES).any(|p| self.name_taken(&parts_sheet_name(p), None)) {
            return Err(bad(PARTS_PAGE_NAME_TAKEN));
        }
        let project = match self.project_info {
            Some(_) => None,
            None => {
                let p = ProjectInfo::new(stamp);
                validate_project_info(&p).map_err(bad)?;
                Some(p)
            }
        };
        if self.drawings.len() >= crate::drawings::MAX_DRAWINGS {
            return Err(bad(format!(
                "Ein Dokument darf höchstens {} Zeichnungen haben",
                crate::drawings::MAX_DRAWINGS
            )));
        }
        if self.next_drawing == u64::MAX {
            return Err(bad("Der Zeichnungszähler ist erschöpft"));
        }
        let id = DrawingId(self.next_drawing);
        let mut d = Drawing::new(id, name, sheet, Projection::First);
        d.info = Some(page_info(&stamp.date));
        d.kind = DrawingKind::PartsList { page: 1 };
        self.check_drawing(&d)?;
        self.next_drawing += 1;
        self.drawings.push(d);
        if project.is_some() {
            self.project_info = project;
        }
        self.sync_parts_sheets_inner();
        self.drawing_edits += 1;
        Ok(id)
    }

    /// Gleicht die Zahl der Stücklistenblätter an die Zeilen an: fehlende Folgeblätter entstehen
    /// direkt hinter dem letzten Stücklistenblatt, überzählige verschwinden von hinten; das
    /// Blattformat von Seite 1 gilt für alle. Gibt `true` zurück, wenn sich etwas geändert hat
    /// (das zählt als eine Änderung).
    pub fn sync_parts_sheets(&mut self) -> bool {
        let changed = self.sync_parts_sheets_inner();
        if changed {
            self.drawing_edits += 1;
        }
        changed
    }

    /// Wie [`Document::sync_parts_sheets`], ohne den Änderungszähler (für Vorgänge, die selbst
    /// als eine Änderung zählen).
    pub(crate) fn sync_parts_sheets_inner(&mut self) -> bool {
        let ids = self.parts_sheets();
        let Some(first) = ids
            .first()
            .and_then(|id| self.drawing(*id))
            .filter(|d| page_of(d) == Some(1))
            .cloned()
        else {
            return false;
        };
        let mut changed = false;
        // Blattformat und Projektion von Seite 1 gelten für alle Folgeblätter.
        for d in self.drawings.iter_mut() {
            if page_of(d).is_some_and(|p| p > 1)
                && (d.sheet != first.sheet || d.projection != first.projection)
            {
                d.sheet = first.sheet;
                d.projection = first.projection;
                changed = true;
            }
        }
        let rows = self.parts_rows().len();
        let wanted = pages_needed(rows, parts_rows_per_page(first.sheet));
        let have = ids.len() as u32;
        // Überzählige von hinten entfernen.
        if have > wanted {
            let drop: Vec<DrawingId> = ids[wanted as usize..].to_vec();
            self.drawings.retain(|d| !drop.contains(&d.id));
            changed = true;
        }
        // Fehlende hinter dem letzten Stücklistenblatt einfügen.
        let created = first
            .info
            .as_ref()
            .map_or_else(String::new, |i| i.created.clone());
        for page in (have.min(wanted) + 1)..=wanted {
            let name = parts_sheet_name(page);
            if self.drawings.len() >= crate::drawings::MAX_DRAWINGS
                || self.next_drawing == u64::MAX
                || self.name_taken(&name, None)
            {
                break;
            }
            let id = DrawingId(self.next_drawing);
            let mut d = Drawing::new(id, name, first.sheet, first.projection);
            d.info = Some(page_info(&created));
            d.kind = DrawingKind::PartsList { page };
            let at = self
                .drawings
                .iter()
                .rposition(|x| page_of(x).is_some())
                .map_or(self.drawings.len(), |i| i + 1);
            self.next_drawing += 1;
            self.drawings.insert(at, d);
            changed = true;
        }
        changed
    }
}

fn page_info(created: &str) -> SheetInfo {
    let mut i = SheetInfo::new(&Stamp {
        date: created.to_string(),
        user: String::new(),
    });
    i.notes = NotesValue::Lines(Vec::new());
    i
}
