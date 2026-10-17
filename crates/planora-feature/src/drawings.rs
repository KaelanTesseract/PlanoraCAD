//! Zeichnungen im Dokument: Verwalten, Prüfen und Übernehmen aus einem Snapshot.

use crate::error::FeatureError;
use crate::model::Document;
use planora_drawing::{
    Drawing, DrawingId, Placement, Projection, Scale, Sheet, ViewId, ViewKind, align_dir,
};
use std::collections::HashSet;

/// Höchstzahl der Zeichnungen in einem Dokument.
pub const MAX_DRAWINGS: usize = 1000;
/// Höchstzahl der Ansichten einer Zeichnung.
pub const MAX_VIEWS: usize = 64;
/// Höchstzahl der Zeichen eines Zeichnungsnamens.
pub const MAX_NAME_CHARS: usize = 200;
/// Größter Betrag einer Lageangabe auf dem Blatt in Millimetern.
pub const MAX_POS_MM: f64 = 1.0e5;

fn bad(m: impl Into<String>) -> FeatureError {
    FeatureError::InvalidInput(m.into())
}

fn unknown() -> FeatureError {
    bad("Die Zeichnung gibt es nicht")
}

fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Prüft eine einzelne Zeichnung (Name, Ansichten, Verweise, Zahlen, Maße).
pub fn validate_drawing(d: &Drawing) -> Result<(), String> {
    if d.name.trim().is_empty() {
        return Err("Die Zeichnung braucht einen Namen".into());
    }
    if d.name.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "Der Name der Zeichnung ist zu lang (höchstens {MAX_NAME_CHARS} Zeichen)"
        ));
    }
    if d.views.len() > MAX_VIEWS {
        return Err(format!(
            "Eine Zeichnung darf höchstens {MAX_VIEWS} Ansichten haben"
        ));
    }
    if d.next_view == 0 || d.next_view == u64::MAX {
        return Err("Der Ansichtszähler der Zeichnung ist ungültig".into());
    }
    let ok = |x: f64| x.is_finite() && x.abs() <= MAX_POS_MM;
    let in_range = |s: u32| (1..=1000).contains(&s);
    let mut seen: Vec<ViewId> = Vec::with_capacity(d.views.len());
    for v in &d.views {
        if v.id.0 == 0 || v.id.0 >= d.next_view {
            return Err("Eine Ansicht hat eine ungültige Kennung".into());
        }
        if seen.contains(&v.id) {
            return Err("Eine Ansichtskennung kommt doppelt vor".into());
        }
        if !in_range(v.scale.num) || !in_range(v.scale.den) {
            return Err("Der Maßstab muss zwischen 1:1000 und 1000:1 liegen".into());
        }
        match (v.placement, v.parent) {
            (Placement::Free { x, y }, None) => {
                if !ok(x) || !ok(y) {
                    return Err("Die Lage einer Ansicht liegt außerhalb des Blatts".into());
                }
            }
            (Placement::Free { .. }, Some(_)) => {
                return Err("Eine abgeleitete Ansicht muss an ihrer Ausrichtung hängen".into());
            }
            (Placement::Aligned { .. }, None) => {
                return Err(
                    "Eine Ansicht ohne Ausgangsansicht kann nicht ausgerichtet sein".into(),
                );
            }
            (Placement::Aligned { distance }, Some(p)) => {
                if !ok(distance) {
                    return Err("Der Abstand einer Ansicht liegt außerhalb des Blatts".into());
                }
                // Die Ausgangsansicht muss früher in der Liste stehen: das schließt Zyklen aus.
                if !seen.contains(&p) {
                    return Err(
                        "Die Ausgangsansicht muss vor der abgeleiteten Ansicht stehen".into(),
                    );
                }
                let parent = d
                    .views
                    .iter()
                    .find(|w| w.id == p)
                    .ok_or("Die Ausgangsansicht gibt es nicht")?;
                if !parent.kind.is_normal() {
                    return Err("Aus einer isometrischen Ansicht lässt sich nichts ableiten".into());
                }
                if align_dir(d.projection, parent.kind, v.kind).is_none() {
                    return Err("Diese Ansicht lässt sich nicht ableiten".into());
                }
            }
        }
        seen.push(v.id);
    }
    if let Some(info) = &d.info {
        planora_drawing::sheet_info::validate_sheet_info(info)?;
    }
    crate::dimensions::validate_dimensions(d)
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

    fn name_taken(&self, name: &str, except: Option<DrawingId>) -> bool {
        let key = name_key(name);
        self.drawings
            .iter()
            .any(|d| Some(d.id) != except && name_key(&d.name) == key)
    }

    fn check_drawing(&self, d: &Drawing) -> Result<(), FeatureError> {
        validate_drawing(d).map_err(bad)?;
        if self.name_taken(&d.name, Some(d.id)) {
            return Err(bad("Eine Zeichnung mit diesem Namen gibt es schon"));
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
        if self.drawings.len() >= MAX_DRAWINGS {
            return Err(bad(format!(
                "Ein Dokument darf höchstens {MAX_DRAWINGS} Zeichnungen haben"
            )));
        }
        if self.next_drawing == u64::MAX {
            return Err(bad("Der Zeichnungszähler ist erschöpft"));
        }
        let id = DrawingId(self.next_drawing);
        let d = Drawing::new(id, name.trim().to_string(), sheet, projection);
        self.check_drawing(&d)?;
        self.next_drawing += 1;
        self.drawings.push(d);
        self.drawing_edits += 1;
        Ok(id)
    }

    /// Benennt eine Zeichnung um.
    pub fn rename_drawing(&mut self, id: DrawingId, name: &str) -> Result<(), FeatureError> {
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
        self.drawings.remove(idx);
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
            return Err(bad("Die Kennung einer Zeichnung lässt sich nicht ändern"));
        }
        self.check_drawing(&copy)?;
        if copy != self.drawings[idx] {
            self.drawings[idx] = copy;
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

    /// Übernimmt Zeichnungen aus einem Snapshot (mit Prüfung); der Änderungszähler bleibt 0.
    pub(crate) fn set_drawings(
        &mut self,
        drawings: Vec<Drawing>,
        next_drawing: u64,
    ) -> Result<(), FeatureError> {
        if next_drawing == 0 || next_drawing == u64::MAX {
            return Err(bad("Der Zeichnungszähler ist ungültig"));
        }
        if drawings.len() > MAX_DRAWINGS {
            return Err(bad(format!(
                "Ein Dokument darf höchstens {MAX_DRAWINGS} Zeichnungen haben"
            )));
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for d in &drawings {
            if d.id.0 == 0 || d.id.0 >= next_drawing {
                return Err(bad("Eine Zeichnung hat eine ungültige Kennung"));
            }
            if !ids.insert(d.id) {
                return Err(bad("Eine Zeichnungskennung kommt doppelt vor"));
            }
            validate_drawing(d).map_err(|e| bad(format!("Zeichnung „{}“: {e}", d.name)))?;
            if !names.insert(name_key(&d.name)) {
                return Err(bad("Ein Zeichnungsname kommt doppelt vor"));
            }
        }
        self.drawings = drawings;
        self.next_drawing = next_drawing;
        Ok(())
    }
}
