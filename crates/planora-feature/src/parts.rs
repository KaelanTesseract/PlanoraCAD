//! Stückliste im Dokument: stabile Körperkennung, Zeilen (gleichnamige Körper zusammengefasst),
//! Zelltexte und die Texte, die der Nutzer pro Zeile überschrieben hat.

use crate::error::FeatureError;
use crate::model::{BodyRef, Document, FeatureKind};
use planora_drawing::parts::{
    BodyKey, MAX_OVERRIDES, PartsCells, PartsRow, RowOverride, resolve_parts, validate_override,
};
use planora_drawing::sheet_info::DASH;
use planora_i18n::tr;
use std::collections::BTreeMap;

fn bad(m: impl Into<String>) -> FeatureError {
    FeatureError::InvalidInput(m.into())
}

/// Prüft die Überschreibungen aus einem Snapshot (Anzahl und Texte).
pub(crate) fn check_overrides(o: &BTreeMap<BodyKey, RowOverride>) -> Result<(), FeatureError> {
    if o.len() > MAX_OVERRIDES {
        return Err(bad(tr!(
            "sheet-parts-too-many-overrides",
            max = MAX_OVERRIDES.to_string()
        )));
    }
    for v in o.values() {
        validate_override(v)
            .map_err(|e| bad(tr!("feature-parts-prefix", detail = e.to_string())))?;
    }
    Ok(())
}

impl Document {
    /// Folgt einer Kette von Verschiebungen und Verrundungen bis zu dem Körper, den ein anderes
    /// Feature erzeugt hat (Ursprung für Benennung und Art).
    pub fn naming_origin(&self, mut r: BodyRef) -> BodyRef {
        // Verweise zeigen nur nach vorn: spätestens nach so vielen Schritten ist die Kette zu Ende.
        for _ in 0..self.features().len() {
            match self.feature(r.feature).map(|f| &f.kind) {
                Some(FeatureKind::Move { body, .. } | FeatureKind::Verrunden { body, .. }) => {
                    r = *body
                }
                _ => break,
            }
        }
        r
    }

    /// Anzeigename eines Körpers: der Name des Features, das ihn ursprünglich erzeugt hat (mit
    /// laufender Nummer, wenn es mehrere Körper erzeugt).
    pub fn body_display_name(&self, r: BodyRef) -> Option<String> {
        let r = self.naming_origin(r);
        let f = self.feature(r.feature)?;
        let several = r.index > 0
            || self
                .body(BodyRef {
                    feature: r.feature,
                    index: 1,
                })
                .is_some();
        Some(if several {
            format!("{} {}", f.name, r.index + 1)
        } else {
            f.name.clone()
        })
    }

    /// Kennung des Körpers über Änderungen am Teil hinweg: Verschieben und Verrunden führen zum
    /// bearbeiteten Körper, eine Boolesche Operation (Ergebnis 0) zu ihrem Ziel; weitere
    /// Ergebnisse einer Booleschen Operation sind eigene Körper.
    pub fn body_key(&self, mut r: BodyRef) -> BodyKey {
        for _ in 0..self.features().len() {
            match self.feature(r.feature).map(|f| &f.kind) {
                Some(FeatureKind::Move { body, .. } | FeatureKind::Verrunden { body, .. }) => {
                    r = *body
                }
                Some(FeatureKind::Boolean { target, .. }) if r.index == 0 => r = *target,
                _ => break,
            }
        }
        BodyKey {
            feature: r.feature.0,
            index: u32::try_from(r.index).unwrap_or(u32::MAX),
        }
    }

    /// Die Zeilen der Stückliste: sichtbare Körper (ohne Netz-Körper und Unterdrückte), nach
    /// Anzeigename in der Reihenfolge der ersten Fundstelle zusammengefasst.
    pub fn parts_rows(&self) -> Vec<PartsRow> {
        let mut rows: Vec<PartsRow> = Vec::new();
        for v in self.visible_bodies() {
            let name = self.body_display_name(v.body).unwrap_or(v.name);
            let key = self.body_key(v.body);
            match rows.iter_mut().find(|r| r.name == name) {
                Some(r) => {
                    r.qty += 1;
                    r.keys.push(key);
                }
                None => rows.push(PartsRow {
                    pos: rows.len() as u32 + 1,
                    name,
                    qty: 1,
                    keys: vec![key],
                }),
            }
        }
        rows
    }

    /// Texte der Stückliste (Pos., Benennung, Menge, Werkstoff, Bemerkung) mit den
    /// Überschreibungen; Werkstoff automatisch aus den Projektangaben.
    pub fn parts_cells(&self) -> Vec<PartsCells> {
        let material = self
            .project_info()
            .map_or(DASH, |p| p.material.resolve(DASH));
        resolve_parts(&self.parts_rows(), &self.parts_overrides, material)
    }

    /// Pos. der Zeile, die den Körper `key` enthält.
    pub fn row_of(&self, key: BodyKey) -> Option<u32> {
        self.parts_rows()
            .into_iter()
            .find(|r| r.keys.contains(&key))
            .map(|r| r.pos)
    }

    /// Eigene Zeilentexte, nach Körperkennung des ersten Körpers der Zeile.
    pub fn parts_overrides(&self) -> &BTreeMap<BodyKey, RowOverride> {
        &self.parts_overrides
    }

    /// Setzt die eigenen Texte der Zeile mit dem Schlüssel `key` (mit Prüfung). Ganz
    /// automatische Texte entfernen den Eintrag. Der Änderungszähler steigt nur bei einer
    /// Änderung.
    pub fn set_parts_override(&mut self, key: BodyKey, o: RowOverride) -> Result<(), FeatureError> {
        validate_override(&o)
            .map_err(|e| bad(tr!("feature-parts-prefix", detail = e.to_string())))?;
        if o.is_auto() {
            if self.parts_overrides.remove(&key).is_some() {
                self.drawing_edits += 1;
            }
            return Ok(());
        }
        if self.parts_overrides.get(&key) == Some(&o) {
            return Ok(());
        }
        if !self.parts_overrides.contains_key(&key) && self.parts_overrides.len() >= MAX_OVERRIDES {
            return Err(bad(tr!(
                "sheet-parts-too-many-overrides",
                max = MAX_OVERRIDES.to_string()
            )));
        }
        self.parts_overrides.insert(key, o);
        self.drawing_edits += 1;
        Ok(())
    }
}
