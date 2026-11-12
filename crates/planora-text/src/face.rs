//! Ein Schnitt einer Schrift: Kennzahlen aus der Datei und die gewählte Ausprägung.

use std::fmt;
use std::sync::Arc;

use skrifa::raw::TableProvider;
use skrifa::string::StringId;
use skrifa::{FontRef, MetadataProvider, Tag};

/// Gewicht, ab dem ein Schnitt als fett gilt. Darunter wird Fett nachgebildet.
const BOLD_ENOUGH: f32 = 600.0;
/// Zielgewicht für Fett und für Normal.
const WEIGHT_BOLD: f32 = 700.0;
const WEIGHT_REGULAR: f32 = 400.0;

/// Ein gewählter Schnitt: die Bytes der Datei, der Eintrag darin und die Lage der Achse `wght`.
#[derive(Clone)]
pub struct FaceRef {
    /// Bytes der ganzen Datei (bei einer Sammlung alle Schriften darin).
    pub data: Arc<[u8]>,
    /// Index der Schrift in der Sammlung, sonst 0.
    pub index: u32,
    /// Lage der Achse `wght` bei variablen Schriften, wenn Fett gewählt ist, sonst `None`
    /// (dann gilt die Grundeinstellung der Schrift).
    pub wght: Option<f32>,
    /// Familienname (Name-ID 16, sonst 1).
    pub family: String,
    /// Name des Schnitts (Name-ID 17, sonst 2), z. B. „SemiBold“.
    pub style: String,
    /// Gewicht aus `OS/2` (`usWeightClass`).
    pub weight: u16,
    /// Kursiv aus `OS/2` (`fsSelection` Bit 0).
    pub italic: bool,
}

impl FaceRef {
    /// Öffnet die Schrift zum Lesen. `None` nur, wenn die Bytes nicht mehr passen.
    pub fn font(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(&self.data, self.index).ok()
    }
}

impl fmt::Debug for FaceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Ohne die Bytes selbst, die wären Megabytes lang.
        f.debug_struct("FaceRef")
            .field("bytes", &self.data.len())
            .field("index", &self.index)
            .field("wght", &self.wght)
            .field("family", &self.family)
            .field("style", &self.style)
            .field("weight", &self.weight)
            .field("italic", &self.italic)
            .finish()
    }
}

/// Ergebnis der Schnittwahl.
#[derive(Clone, Debug)]
pub struct FaceChoice {
    pub face: FaceRef,
    /// Fett wird nachgebildet (Umrisse verbreitern), weil kein fetter Schnitt da ist.
    pub synth_bold: bool,
    /// Kursiv wird nachgebildet (Scheren), weil kein kursiver Schnitt da ist.
    pub synth_italic: bool,
    /// Die gewünschte Familie fehlt, es wird die Ersatzschrift benutzt.
    pub substituted: bool,
}

/// Kennzahlen eines Schnitts, wie sie beim Durchsuchen gelesen werden.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FaceInfo {
    pub family: String,
    pub style: String,
    pub weight: u16,
    pub italic: bool,
    /// Bereich der Achse `wght` (kleinster, größter Wert), wenn die Schrift variabel ist.
    pub wght_axis: Option<(f32, f32)>,
}

/// Liest Familie, Schnitt, Gewicht und Kursiv einer Schrift. Fehlt der Name oder hat die
/// Schrift keine Umrisse (weder `glyf` noch `CFF`/`CFF2`), ist sie unbrauchbar. Der Grund ist
/// ein englischer Protokolltext (landet in `SkipReason::Invalid`).
pub(crate) fn read_face_info(font: &FontRef<'_>) -> Result<FaceInfo, String> {
    let family = name_string(font, StringId::TYPOGRAPHIC_FAMILY_NAME)
        .or_else(|| name_string(font, StringId::FAMILY_NAME))
        .ok_or_else(|| "no family name".to_string())?;
    let style = name_string(font, StringId::TYPOGRAPHIC_SUBFAMILY_NAME)
        .or_else(|| name_string(font, StringId::SUBFAMILY_NAME))
        .unwrap_or_else(|| "Regular".to_string());
    if font.outline_glyphs().format().is_none() {
        return Err("no outlines (glyf/CFF)".to_string());
    }
    let (weight, italic) = match font.os2() {
        Ok(os2) => (
            os2.us_weight_class(),
            os2.fs_selection()
                .contains(skrifa::raw::tables::os2::SelectionFlags::ITALIC),
        ),
        // Ohne `OS/2`: wie ein normaler Schnitt.
        Err(_) => (WEIGHT_REGULAR as u16, false),
    };
    let wght_axis = font
        .axes()
        .get_by_tag(Tag::new(b"wght"))
        .map(|axis| (axis.min_value(), axis.max_value()))
        .filter(|(min, max)| min.is_finite() && max.is_finite() && min <= max);
    Ok(FaceInfo {
        family,
        style,
        weight,
        italic,
        wght_axis,
    })
}

/// Englischer Eintrag der Namenstabelle (sonst der erste), ohne Leerraum am Rand.
fn name_string(font: &FontRef<'_>, id: StringId) -> Option<String> {
    let text: String = font
        .localized_strings(id)
        .english_or_first()?
        .chars()
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Wahl unter den Schnitten einer Familie.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pick {
    /// Index in der übergebenen Liste.
    pub index: usize,
    pub wght: Option<f32>,
    pub synth_bold: bool,
    pub synth_italic: bool,
}

/// Gewicht, das der Schnitt für die Wahl hat. Bei Fett und einer Achse `wght` steht die Achse
/// so nah an 700 wie möglich, sonst gilt das Gewicht aus `OS/2`.
fn effective_weight(info: &FaceInfo, bold: bool) -> (f32, Option<f32>) {
    match info.wght_axis {
        Some((min, max)) if bold => {
            let value = WEIGHT_BOLD.max(min).min(max);
            (value, Some(value))
        }
        _ => (f32::from(info.weight), None),
    }
}

/// Wählt den passendsten Schnitt: zuerst das Gewicht am nächsten an 700 (Fett) bzw. 400,
/// dann die passende Lage (kursiv oder nicht). Bei gleichem Abstand gewinnt bei Fett der
/// schwerere, sonst der leichtere Schnitt, danach der frühere Eintrag.
pub(crate) fn pick_face(faces: &[&FaceInfo], bold: bool, italic: bool) -> Option<Pick> {
    let target = if bold { WEIGHT_BOLD } else { WEIGHT_REGULAR };
    let mut best: Option<(usize, (f32, bool, f32))> = None;
    for (index, info) in faces.iter().enumerate() {
        let (weight, _) = effective_weight(info, bold);
        let distance = (weight - target).abs();
        let style_mismatch = info.italic != italic;
        let tie = if bold { -weight } else { weight };
        let key = (distance, style_mismatch, tie);
        // Gewichte sind endlich, der Vergleich der Tupel ist also vollständig. Nur ein echt
        // besserer Schlüssel ersetzt den früheren Eintrag.
        if best.as_ref().is_none_or(|(_, old)| key < *old) {
            best = Some((index, key));
        }
    }
    let (index, _) = best?;
    let info = faces[index];
    let (weight, wght) = effective_weight(info, bold);
    Some(Pick {
        index,
        wght,
        synth_bold: bold && weight < BOLD_ENOUGH,
        synth_italic: italic && !info.italic,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(weight: u16, italic: bool, axis: Option<(f32, f32)>) -> FaceInfo {
        FaceInfo {
            family: "Probe".into(),
            style: String::new(),
            weight,
            italic,
            wght_axis: axis,
        }
    }

    #[test]
    fn bold_prefers_weight_over_italic() {
        let faces = [
            info(400, false, None),
            info(400, true, None),
            info(700, false, None),
        ];
        let refs: Vec<&FaceInfo> = faces.iter().collect();
        let pick = pick_face(&refs, true, true).unwrap();
        assert_eq!(pick.index, 2);
        assert!(!pick.synth_bold);
        assert!(pick.synth_italic);
    }

    #[test]
    fn italic_face_wins_at_equal_weight() {
        let faces = [info(400, false, None), info(400, true, None)];
        let refs: Vec<&FaceInfo> = faces.iter().collect();
        let pick = pick_face(&refs, false, true).unwrap();
        assert_eq!(pick.index, 1);
        assert!(!pick.synth_italic);
    }

    #[test]
    fn variable_font_sets_wght_axis_when_bold() {
        let faces = [info(400, false, Some((100.0, 900.0)))];
        let refs: Vec<&FaceInfo> = faces.iter().collect();
        let bold = pick_face(&refs, true, false).unwrap();
        assert_eq!(bold.wght, Some(700.0));
        assert!(!bold.synth_bold);
        let regular = pick_face(&refs, false, false).unwrap();
        assert_eq!(regular.wght, None);
    }

    #[test]
    fn light_only_variable_font_still_synthesises_bold() {
        let faces = [info(300, false, Some((100.0, 400.0)))];
        let refs: Vec<&FaceInfo> = faces.iter().collect();
        let pick = pick_face(&refs, true, false).unwrap();
        assert_eq!(pick.wght, Some(400.0));
        assert!(pick.synth_bold);
    }

    #[test]
    fn variable_face_beats_static_regular_for_bold() {
        let faces = [
            info(400, false, None),
            info(400, false, Some((100.0, 900.0))),
        ];
        let refs: Vec<&FaceInfo> = faces.iter().collect();
        let pick = pick_face(&refs, true, false).unwrap();
        assert_eq!(pick.index, 1);
    }

    #[test]
    fn equal_distance_prefers_heavier_for_bold_and_lighter_for_regular() {
        let faces = [info(600, false, None), info(800, false, None)];
        let refs: Vec<&FaceInfo> = faces.iter().collect();
        assert_eq!(pick_face(&refs, true, false).unwrap().index, 1);
        let faces = [info(500, false, None), info(300, false, None)];
        let refs: Vec<&FaceInfo> = faces.iter().collect();
        assert_eq!(pick_face(&refs, false, false).unwrap().index, 1);
    }

    #[test]
    fn empty_list_has_no_pick() {
        assert_eq!(pick_face(&[], true, true), None);
    }
}
