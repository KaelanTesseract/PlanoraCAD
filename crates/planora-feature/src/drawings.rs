//! Zeichnungen im Dokument: Verwalten, Prüfen und Übernehmen aus einem Snapshot.

use crate::error::FeatureError;
use crate::model::Document;
use crate::parts_sheets::{follow_page_delete, page_of, parts_sheet_rename};
use planora_drawing::basis::Basis;
use planora_drawing::model::{Balloon, BalloonId};
use planora_drawing::parts::{BodyKey, MAX_BALLOONS};
use planora_drawing::sheet_info::{ProjectInfo, SheetInfo, Stamp, validate_project_info};
use planora_drawing::{
    Drawing, DrawingId, DrawingKind, MAX_SECTIONS, Placement, Projection, SECTION_LETTERS, Scale,
    Sheet, ViewId, ViewKind, align_dir, sections_ok_on,
};
use planora_i18n::tr;
use std::collections::{BTreeMap, HashSet};

/// Höchstzahl der Zeichnungen in einem Dokument.
pub const MAX_DRAWINGS: usize = 1000;
/// Höchstzahl der Ansichten einer Zeichnung.
pub const MAX_VIEWS: usize = 64;
/// Höchstzahl der Zeichen eines Zeichnungsnamens.
pub const MAX_NAME_CHARS: usize = 200;
/// Größter Betrag einer Lageangabe auf dem Blatt in Millimetern.
pub const MAX_POS_MM: f64 = 1.0e5;
/// Meldung, wenn eine Zeichnung schon die Höchstzahl an Nummernkreisen hat.
pub fn too_many_balloons() -> String {
    tr!("feature-balloons-max")
}

fn bad(m: impl Into<String>) -> FeatureError {
    FeatureError::InvalidInput(m.into())
}

fn unknown() -> FeatureError {
    bad(tr!("feature-drawing-unknown"))
}

fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Prüft eine einzelne Zeichnung (Name, Ansichten, Verweise, Zahlen, Maße).
pub fn validate_drawing(d: &Drawing) -> Result<(), String> {
    if d.name.trim().is_empty() {
        return Err(tr!("feature-drawing-name-needed"));
    }
    if d.name.chars().count() > MAX_NAME_CHARS {
        return Err(tr!(
            "feature-drawing-name-long",
            max = MAX_NAME_CHARS.to_string()
        ));
    }
    match d.kind {
        DrawingKind::Views => {}
        DrawingKind::PartsList { page } => {
            if page == 0 {
                return Err(tr!("feature-parts-page-number"));
            }
            if !d.views.is_empty() || !d.dimensions.is_empty() || !d.balloons.is_empty() {
                return Err(tr!("feature-parts-sheet-empty"));
            }
        }
    }
    if d.views.len() > MAX_VIEWS {
        return Err(tr!(
            "feature-drawing-views-max",
            max = MAX_VIEWS.to_string()
        ));
    }
    if d.next_view == 0 || d.next_view == u64::MAX {
        return Err(tr!("feature-view-counter-invalid"));
    }
    if d.views.iter().filter(|v| v.section.is_some()).count() > MAX_SECTIONS {
        return Err(tr!("section-full", max = MAX_SECTIONS.to_string()));
    }
    let ok = |x: f64| x.is_finite() && x.abs() <= MAX_POS_MM;
    let in_range = |s: u32| (1..=1000).contains(&s);
    let mut seen: Vec<ViewId> = Vec::with_capacity(d.views.len());
    for v in &d.views {
        if v.id.0 == 0 || v.id.0 >= d.next_view {
            return Err(tr!("feature-view-id-invalid"));
        }
        if seen.contains(&v.id) {
            return Err(tr!("feature-view-id-twice"));
        }
        if !in_range(v.scale.num) || !in_range(v.scale.den) {
            return Err(tr!("feature-view-scale"));
        }
        if v.section.is_some()
            && !matches!(
                (v.placement, v.parent),
                (Placement::Aligned { .. }, Some(_))
            )
        {
            return Err(not_straight());
        }
        match (v.placement, v.parent) {
            (Placement::Free { x, y }, None) => {
                if !ok(x) || !ok(y) {
                    return Err(tr!("feature-view-position"));
                }
            }
            (Placement::Free { .. }, Some(_)) => {
                return Err(tr!("feature-view-derived-aligned"));
            }
            (Placement::Aligned { .. }, None) => {
                return Err(tr!("feature-view-no-parent-aligned"));
            }
            (Placement::Aligned { distance }, Some(p)) => {
                if !ok(distance) {
                    return Err(tr!("feature-view-offset"));
                }
                // Die Ausgangsansicht muss früher in der Liste stehen: das schließt Zyklen aus.
                if !seen.contains(&p) {
                    return Err(tr!("feature-view-parent-first"));
                }
                let parent = d
                    .views
                    .iter()
                    .find(|w| w.id == p)
                    .ok_or_else(|| tr!("feature-view-parent-unknown"))?;
                if let Some(sec) = &v.section {
                    validate_section(d, v, sec, parent, &seen)?;
                }
                if !parent.kind.is_normal() {
                    return Err(tr!("feature-view-isometric-derive"));
                }
                if align_dir(d.projection, parent.kind, v.kind).is_none() {
                    return Err(tr!("feature-view-not-derivable"));
                }
            }
        }
        seen.push(v.id);
    }
    sections_ok_on(d, d.sheet)?;
    validate_balloons(d)?;
    if let Some(info) = &d.info {
        planora_drawing::sheet_info::validate_sheet_info(info)?;
    }
    crate::dimensions::validate_dimensions(d)
}

fn not_straight() -> String {
    tr!("section-not-from-here")
}

/// Prüft eine Schnittansicht `v` gegen ihre Ausgangsansicht `parent` und die Buchstaben der
/// Ansichten davor (`seen`).
fn validate_section(
    d: &Drawing,
    v: &planora_drawing::ViewSpec,
    sec: &planora_drawing::SectionSpec,
    parent: &planora_drawing::ViewSpec,
    seen: &[ViewId],
) -> Result<(), String> {
    if !parent.kind.is_normal() || parent.section.is_some() {
        return Err(not_straight());
    }
    if !v.kind.is_normal() || Basis::of(v.kind).d.dot(Basis::of(parent.kind).d).abs() > 1e-9 {
        return Err(tr!("feature-section-plane"));
    }
    if v.scale != parent.scale {
        return Err(tr!("feature-section-scale"));
    }
    let twice = d
        .views
        .iter()
        .filter(|w| seen.contains(&w.id))
        .any(|w| w.section.is_some_and(|s| s.letter == sec.letter));
    if !SECTION_LETTERS.contains(sec.letter) || twice {
        return Err(tr!("feature-section-letter"));
    }
    if !sec.offset.is_finite() || sec.offset.abs() > MAX_POS_MM {
        return Err(tr!("feature-section-offset"));
    }
    Ok(())
}

/// Prüft die Nummernkreise: Anzahl, Kennungen, Ansicht, Anker und Versatz.
fn validate_balloons(d: &Drawing) -> Result<(), String> {
    if d.balloons.len() > MAX_BALLOONS {
        return Err(too_many_balloons());
    }
    if d.next_balloon == 0 || d.next_balloon == u64::MAX {
        return Err(tr!("feature-balloon-counter"));
    }
    let mut ids = HashSet::with_capacity(d.balloons.len());
    for b in &d.balloons {
        if b.id.0 == 0 || b.id.0 >= d.next_balloon {
            return Err(tr!("feature-balloon-id-invalid"));
        }
        if !ids.insert(b.id) {
            return Err(tr!("feature-balloon-id-twice"));
        }
        if d.view(b.view).is_none() {
            return Err(tr!("feature-balloon-no-view"));
        }
        let unit = |x: f64| x.is_finite() && (0.0..=1.0).contains(&x);
        if !unit(b.anchor.0) || !unit(b.anchor.1) {
            return Err(tr!("feature-balloon-target"));
        }
        let (dx, dy) = b.offset;
        if !(dx.is_finite() && dy.is_finite() && dx.hypot(dy) <= MAX_POS_MM) {
            return Err(tr!("feature-balloon-position"));
        }
    }
    Ok(())
}

impl Document {
    /// Alle Zeichnungen in der Reihenfolge ihrer Anlage.
    pub fn drawings(&self) -> &[Drawing] {
        &self.drawings
    }

    /// Sucht eine Zeichnung.
    pub fn drawing(&self, id: DrawingId) -> Option<&Drawing> {
        self.drawings.iter().find(|d| d.id == id)
    }

    pub(crate) fn name_taken(&self, name: &str, except: Option<DrawingId>) -> bool {
        let key = name_key(name);
        self.drawings
            .iter()
            .any(|d| Some(d.id) != except && name_key(&d.name) == key)
    }

    pub(crate) fn check_drawing(&self, d: &Drawing) -> Result<(), FeatureError> {
        validate_drawing(d).map_err(bad)?;
        // Die Vorbehaltsnamen gelten nur, wenn der Name sich ändert: eine Zeichnung, die ihn
        // schon trug, bleibt änderbar.
        let renamed = self
            .drawing(d.id)
            .is_none_or(|old| name_key(&old.name) != name_key(&d.name));
        if self.name_taken(&d.name, Some(d.id))
            || (renamed && page_of(d).is_none() && self.reserved_for_parts(&d.name))
        {
            return Err(bad(tr!("feature-drawing-name-taken")));
        }
        Ok(())
    }

    /// Legt eine leere Zeichnung an.
    pub fn add_drawing(
        &mut self,
        name: &str,
        sheet: Sheet,
        projection: Projection,
    ) -> Result<DrawingId, FeatureError> {
        self.add_drawing_with(name, sheet, projection, None, None)
    }

    /// Legt eine leere Zeichnung mit Blattangaben an (Datum und Benutzer des Stempels). Gibt es
    /// noch keine Projektangaben, entstehen sie mit; das zählt zusammen als eine Änderung.
    pub fn add_drawing_stamped(
        &mut self,
        name: &str,
        sheet: Sheet,
        projection: Projection,
        stamp: &Stamp,
    ) -> Result<DrawingId, FeatureError> {
        let project = match self.project_info {
            Some(_) => None,
            None => {
                let p = ProjectInfo::new(stamp);
                validate_project_info(&p).map_err(bad)?;
                Some(p)
            }
        };
        // F4: Die Vorgabe des Schalters wird einmal beim Anlegen festgelegt.
        let mut info = SheetInfo::new(stamp);
        info.parts_list = self.visible_bodies().len() > 1;
        self.add_drawing_with(name, sheet, projection, Some(info), project)
    }

    fn add_drawing_with(
        &mut self,
        name: &str,
        sheet: Sheet,
        projection: Projection,
        info: Option<SheetInfo>,
        project: Option<ProjectInfo>,
    ) -> Result<DrawingId, FeatureError> {
        if self.drawings.len() >= MAX_DRAWINGS {
            return Err(bad(tr!(
                "feature-max-drawings",
                max = MAX_DRAWINGS.to_string()
            )));
        }
        if self.next_drawing == u64::MAX {
            return Err(bad(tr!("feature-drawing-counter-used")));
        }
        let id = DrawingId(self.next_drawing);
        let mut d = Drawing::new(id, name.trim().to_string(), sheet, projection);
        d.info = info;
        self.check_drawing(&d)?;
        self.next_drawing += 1;
        self.drawings.push(d);
        if project.is_some() {
            self.project_info = project;
        }
        self.drawing_edits += 1;
        Ok(id)
    }

    /// Die Projektangaben für das Schriftfeld, falls schon vorhanden.
    pub fn project_info(&self) -> Option<&ProjectInfo> {
        self.project_info.as_ref()
    }

    /// Entfernt die Projektangaben wieder (etwa wenn das Anlegen einer Zeichnung scheitert, das
    /// sie miterzeugt hat). Der Änderungszähler steigt nur, wenn es welche gab.
    pub fn clear_project_info(&mut self) {
        if self.project_info.take().is_some() {
            self.drawing_edits += 1;
        }
    }

    /// Setzt die Projektangaben (mit Prüfung). Der Änderungszähler steigt nur bei einer Änderung.
    pub fn set_project_info(&mut self, p: ProjectInfo) -> Result<(), FeatureError> {
        validate_project_info(&p).map_err(bad)?;
        if self.project_info.as_ref() != Some(&p) {
            self.project_info = Some(p);
            self.drawing_edits += 1;
        }
        Ok(())
    }

    /// Setzt die Blattangaben einer Zeichnung (mit Prüfung).
    pub fn set_sheet_info(&mut self, id: DrawingId, i: SheetInfo) -> Result<(), FeatureError> {
        self.update_drawing(id, |d| d.info = Some(i))
    }

    /// Benennt eine Zeichnung um.
    pub fn rename_drawing(&mut self, id: DrawingId, name: &str) -> Result<(), FeatureError> {
        if self.drawing(id).is_some_and(|d| page_of(d).is_some()) {
            return Err(bad(parts_sheet_rename()));
        }
        let name = name.trim().to_string();
        self.update_drawing(id, |d| d.name = name)
    }

    /// Löscht eine Zeichnung.
    pub fn delete_drawing(&mut self, id: DrawingId) -> Result<(), FeatureError> {
        let idx = self
            .drawings
            .iter()
            .position(|d| d.id == id)
            .ok_or_else(unknown)?;
        match page_of(&self.drawings[idx]) {
            Some(page) if page > 1 => return Err(bad(follow_page_delete())),
            // Seite 1 nimmt alle Folgeblätter mit.
            Some(_) => self.drawings.retain(|d| page_of(d).is_none()),
            None => {
                self.drawings.remove(idx);
            }
        }
        self.drawing_edits += 1;
        Ok(())
    }

    /// Ändert eine Zeichnung auf einer Kopie und prüft das Ergebnis; bei Fehler bleibt das
    /// Dokument unverändert. Der Änderungszähler steigt nur, wenn sich etwas geändert hat.
    pub fn update_drawing(
        &mut self,
        id: DrawingId,
        f: impl FnOnce(&mut Drawing),
    ) -> Result<(), FeatureError> {
        let idx = self
            .drawings
            .iter()
            .position(|d| d.id == id)
            .ok_or_else(unknown)?;
        let mut copy = self.drawings[idx].clone();
        f(&mut copy);
        if copy.id != id {
            return Err(bad(tr!("feature-drawing-id-fixed")));
        }
        let old = &self.drawings[idx];
        if copy.kind != old.kind {
            return Err(bad(tr!("feature-drawing-kind-fixed")));
        }
        if page_of(old).is_some() && copy.name != old.name {
            return Err(bad(parts_sheet_rename()));
        }
        self.check_drawing(&copy)?;
        if copy != self.drawings[idx] {
            let parts = page_of(&copy).is_some();
            let before = if parts {
                self.drawings.clone()
            } else {
                Vec::new()
            };
            self.drawings[idx] = copy;
            if parts {
                self.sync_parts_sheets_inner();
                // Gleicht der Abgleich die Änderung wieder aus (Folgeblatt), bleibt alles wie es war.
                if self.drawings == before {
                    return Ok(());
                }
            }
            self.drawing_edits += 1;
        }
        Ok(())
    }

    /// Fügt einer Zeichnung eine Ansicht hinzu und prüft das Ergebnis.
    pub fn add_view(
        &mut self,
        id: DrawingId,
        kind: ViewKind,
        scale: Scale,
        placement: Placement,
        parent: Option<ViewId>,
    ) -> Result<ViewId, FeatureError> {
        let mut new_id = None;
        self.update_drawing(id, |d| {
            new_id = Some(d.add_view(kind, scale, placement, parent));
        })?;
        new_id.ok_or_else(unknown)
    }

    /// Setzt einen Nummernkreis an den Körper `body` in der Ansicht `view` (Anker auf [0, 1]
    /// begrenzt, Versatz in Blatt-mm). Zählt als eine Änderung; bei Fehler bleibt das Dokument
    /// unverändert.
    pub fn add_balloon(
        &mut self,
        drawing: DrawingId,
        view: ViewId,
        body: BodyKey,
        anchor: (f64, f64),
        offset: (f64, f64),
    ) -> Result<BalloonId, FeatureError> {
        let d = self.drawing(drawing).ok_or_else(unknown)?;
        if d.balloons.len() >= MAX_BALLOONS {
            return Err(bad(too_many_balloons()));
        }
        if d.view(view).is_none() {
            return Err(bad(tr!("feature-view-unknown")));
        }
        if !(anchor.0.is_finite() && anchor.1.is_finite()) {
            return Err(bad(tr!("feature-balloon-place")));
        }
        let anchor = (anchor.0.clamp(0.0, 1.0), anchor.1.clamp(0.0, 1.0));
        let mut new_id = None;
        self.update_drawing(drawing, |d| {
            new_id = Some(d.add_balloon(Balloon {
                id: BalloonId(0),
                view,
                body,
                anchor,
                offset,
            }));
        })?;
        new_id.ok_or_else(unknown)
    }

    /// Nummer (Pos. der Stücklistenzeile) jedes Nummernkreises der Zeichnung; `None`, wenn es
    /// den Körper nicht mehr gibt (verloren). Unbekannte Zeichnung: leer.
    pub fn balloon_numbers(&self, drawing: DrawingId) -> BTreeMap<BalloonId, Option<u32>> {
        let Some(d) = self.drawing(drawing) else {
            return BTreeMap::new();
        };
        let rows = self.parts_rows();
        d.balloons
            .iter()
            .map(|b| {
                let pos = rows
                    .iter()
                    .find(|r| r.keys.contains(&b.body))
                    .map(|r| r.pos);
                (b.id, pos)
            })
            .collect()
    }

    /// Übernimmt Zeichnungen aus einem Snapshot (mit Prüfung); der Änderungszähler bleibt 0.
    pub(crate) fn set_drawings(
        &mut self,
        drawings: Vec<Drawing>,
        next_drawing: u64,
    ) -> Result<(), FeatureError> {
        if next_drawing == 0 || next_drawing == u64::MAX {
            return Err(bad(tr!("feature-drawing-counter-invalid")));
        }
        if drawings.len() > MAX_DRAWINGS {
            return Err(bad(tr!(
                "feature-max-drawings",
                max = MAX_DRAWINGS.to_string()
            )));
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for d in &drawings {
            if d.id.0 == 0 || d.id.0 >= next_drawing {
                return Err(bad(tr!("feature-drawing-id-invalid")));
            }
            if !ids.insert(d.id) {
                return Err(bad(tr!("feature-drawing-id-twice")));
            }
            validate_drawing(d).map_err(|e| {
                bad(tr!(
                    "feature-drawing-prefix",
                    name = d.name.to_string(),
                    detail = e.to_string()
                ))
            })?;
            if !names.insert(name_key(&d.name)) {
                return Err(bad(tr!("feature-drawing-name-twice")));
            }
        }
        let mut pages: Vec<u32> = drawings.iter().filter_map(page_of).collect();
        pages.sort_unstable();
        if pages.iter().enumerate().any(|(i, p)| *p != i as u32 + 1) {
            return Err(bad(tr!("feature-parts-pages-gap")));
        }
        // Stücklistenblätter erkennt man an ihrer Art (`DrawingKind::PartsList`); ihr Name
        // entstand in der Sprache beim Anlegen und ist danach Benutzerdaten. Beim Laden zählen
        // nur eindeutige Namen (oben) und lückenlose Seiten, nicht der Wortlaut: sonst ließe sich
        // ein Dokument nicht öffnen, dessen Sprachdatei fehlt oder sich geändert hat.
        self.drawings = drawings;
        self.next_drawing = next_drawing;
        Ok(())
    }
}
