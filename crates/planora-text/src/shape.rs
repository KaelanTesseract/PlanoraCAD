//! Setzen mit `harfrust`: Zeilen, Glyphen und ihre Lage, Ersatz für fehlende Zeichen,
//! Zeichenabstand und nachgebildete Schnitte. Ergebnis ist ein [`TextShape`] in Einheiten der
//! Höhe (Geviert = 1).

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

use harfrust::{
    BufferFlags, Direction, ShapeOptions, Shaper, ShaperData, ShaperInstance, UnicodeBuffer,
};
use planora_math::DVec2;
use planora_sketch::text_shape::{
    Contour, Glyph, MAX_TEXT_CHARS, MAX_TEXT_CURVES, ShapedLine, TextShape, TextShapeError,
    TextShaper, TextStyle,
};
use skrifa::instance::{Location, LocationRef};
use skrifa::outline::OutlineGlyphCollection;
use skrifa::raw::TableProvider;
use skrifa::{FontRef, MetadataProvider};

use crate::outline::{glyph_contours, tidy};
use crate::synth::{BOLD_STRENGTH, ITALIC_SHEAR, embolden, shear};
use crate::{FALLBACK_FAMILY, FaceChoice, FontLibrary};

/// Schrift für Zeichen, die der gewählten Schrift fehlen (mitgeliefert, Apache-2.0).
const GLYPH_FALLBACK_FAMILY: &str = "Roboto";
/// Ein Tabulator wird wie vier Leerzeichen gesetzt (**Annahme**, siehe `offene-punkte.md`).
const TAB_AS_SPACES: &str = "    ";
/// Glyphen-Id für „Zeichen fehlt in der Schrift“ (`.notdef`).
const NOTDEF: u32 = 0;

impl TextShaper for FontLibrary {
    fn shape(&self, content: &str, style: &TextStyle) -> Result<TextShape, TextShapeError> {
        if content.chars().count() > MAX_TEXT_CHARS {
            return Err(TextShapeError::TooLong);
        }
        if content.chars().all(char::is_whitespace) {
            return Err(TextShapeError::Empty);
        }
        let spacing = sanitize_spacing(style.spacing);
        let primary = self.select(&style.font, style.bold, style.italic);
        let fallback = self.select(GLYPH_FALLBACK_FAMILY, style.bold, style.italic);
        let mut substituted = primary.substituted;
        let result = match shape_guarded(&primary, &fallback, content, spacing) {
            Some(result) => result,
            None => {
                // Die Schrift lässt sich nicht setzen (kaputte Datei): wie eine fehlende Familie.
                substituted = true;
                let replacement = self.select(FALLBACK_FAMILY, style.bold, style.italic);
                shape_guarded(&replacement, &fallback, content, spacing)
                    .unwrap_or_else(|| Ok(unset_shape(content)))
            }
        };
        result.map(|mut shape| {
            shape.substituted = substituted.then(|| style.font.clone());
            shape
        })
    }
}

/// Zeichenabstand im erlaubten Bereich; Unsinn (NaN, unendlich) gilt als 0.
fn sanitize_spacing(spacing: f64) -> f64 {
    if spacing.is_finite() {
        spacing.clamp(0.0, 1000.0)
    } else {
        0.0
    }
}

/// Setzt so, dass ein unerwarteter Fehler im Setzer oder Zeichner nur diesen Versuch kostet.
/// `None`, wenn die Schrift sich nicht öffnen lässt oder der Setzer abbricht.
fn shape_guarded(
    primary: &FaceChoice,
    fallback: &FaceChoice,
    content: &str,
    spacing: f64,
) -> Option<Result<TextShape, TextShapeError>> {
    panic::catch_unwind(AssertUnwindSafe(|| {
        shape_with(primary, fallback, content, spacing)
    }))
    .ok()
    .flatten()
}

/// Nur für den Fall, dass selbst die mitgelieferte Schrift nicht zu setzen wäre: leere Zeilen
/// mit Hinweis „Zeichen fehlen“, ungefähre Kennzahlen.
fn unset_shape(content: &str) -> TextShape {
    let lines = split_lines(content)
        .iter()
        .map(|_| ShapedLine {
            glyphs: Vec::new(),
            width: 0.0,
        })
        .collect();
    TextShape {
        lines,
        ascender: 0.8,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: true,
    }
}

/// Zeilen des Inhalts: Enter bricht die Zeile (auch `\r\n` und `\r`), Tabulatoren werden
/// Leerzeichen.
fn split_lines(content: &str) -> Vec<String> {
    content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(|line| line.replace('\t', TAB_AS_SPACES))
        .collect()
}

/// Eine Schrift, bereit zum Setzen und Zeichnen.
struct Face<'a> {
    font: FontRef<'a>,
    outlines: OutlineGlyphCollection<'a>,
    location: Location,
    /// `1 / units per em`: rechnet Schrifteinheiten in Einheiten der Höhe um.
    scale: f64,
    synth_bold: bool,
    synth_italic: bool,
}

impl<'a> Face<'a> {
    fn new(choice: &'a FaceChoice) -> Option<Self> {
        let font = choice.face.font()?;
        let upem = font.head().ok()?.units_per_em();
        if upem == 0 {
            return None;
        }
        let location = match choice.face.wght {
            Some(wght) => font.axes().location([("wght", wght)]),
            None => Location::default(),
        };
        Some(Face {
            outlines: font.outline_glyphs(),
            font,
            location,
            scale: 1.0 / f64::from(upem),
            synth_bold: choice.synth_bold,
            synth_italic: choice.synth_italic,
        })
    }

    /// Lage im Variationsraum für den Setzer; `None` bei der Grundeinstellung.
    fn instance(&self) -> Option<ShaperInstance> {
        let coords = self.location.coords();
        (!coords.is_empty())
            .then(|| ShaperInstance::from_coords(&self.font, coords.iter().copied()))
    }

    /// Ober-, Unterlänge und Zeilenabstand aus `hhea`, sonst aus `OS/2`, in Einheiten der Höhe.
    fn line_metrics(&self) -> (f64, f64, f64) {
        let from = |asc: i16, desc: i16, gap: i16| {
            (asc > desc).then(|| {
                let (asc, desc) = (f64::from(asc) * self.scale, f64::from(desc) * self.scale);
                let gap = f64::from(gap.max(0)) * self.scale;
                (asc, desc, gap)
            })
        };
        let hhea = self.font.hhea().ok().and_then(|h| {
            from(
                h.ascender().to_i16(),
                h.descender().to_i16(),
                h.line_gap().to_i16(),
            )
        });
        let os2 = || {
            self.font.os2().ok().and_then(|o| {
                from(
                    o.s_typo_ascender(),
                    o.s_typo_descender(),
                    o.s_typo_line_gap(),
                )
            })
        };
        hhea.or_else(os2).unwrap_or((0.8, -0.2, 0.0))
    }
}

/// Eine gesetzte Glyphe vor dem Zusammenbau, in Einheiten der Höhe.
#[derive(Clone, Copy, Debug)]
struct Placed {
    /// 0: gewählte Schrift, 1: Ersatz für fehlende Zeichen.
    face: usize,
    gid: u32,
    /// Byte-Anfang des Clusters in der Zeile.
    cluster: u32,
    advance: f64,
    offset: DVec2,
}

/// Setzt `text` mit einem Setzer. `direction` wird vorgegeben (Ersatz mitten in einer Zeile),
/// sonst aus dem Text geraten. `cluster_base` verschiebt die Cluster auf die Lage in der Zeile.
fn run_shaper(
    shaper: &Shaper<'_>,
    text: &str,
    direction: Option<Direction>,
    face: usize,
    scale: f64,
    cluster_base: u32,
) -> (Vec<Placed>, Direction) {
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    if let Some(direction) = direction {
        buffer.set_direction(direction);
    } else {
        buffer.set_flags(BufferFlags::BEGINNING_OF_TEXT | BufferFlags::END_OF_TEXT);
    }
    buffer.guess_segment_properties();
    let direction = buffer.direction();
    let output = shaper.shape(buffer, ShapeOptions::new());
    let placed = output
        .glyph_infos()
        .iter()
        .zip(output.glyph_positions())
        .map(|(info, pos)| Placed {
            face,
            gid: info.glyph_id,
            cluster: info.cluster + cluster_base,
            advance: f64::from(pos.x_advance) * scale,
            offset: DVec2::new(f64::from(pos.x_offset), f64::from(pos.y_offset)) * scale,
        })
        .collect();
    (placed, direction)
}

/// Ersetzt jede Folge von Clustern mit fehlenden Zeichen durch denselben Text, gesetzt mit dem
/// Ersatz-Setzer. Was auch dort fehlt, bleibt `.notdef` (Platz leer, Hinweis).
fn fill_missing(
    placed: Vec<Placed>,
    line: &str,
    direction: Direction,
    shaper: &Shaper<'_>,
    scale: f64,
) -> Vec<Placed> {
    let missing: HashSet<u32> = placed
        .iter()
        .filter(|g| g.gid == NOTDEF)
        .map(|g| g.cluster)
        .collect();
    let mut starts: Vec<u32> = placed.iter().map(|g| g.cluster).collect();
    starts.sort_unstable();
    starts.dedup();
    // Ende eines Clusters im Text: der Anfang des nächsten, sonst das Zeilenende.
    let end_of = |cluster: u32| {
        starts
            .iter()
            .copied()
            .find(|&s| s > cluster)
            .unwrap_or(line.len() as u32)
    };
    let mut out = Vec::with_capacity(placed.len());
    let mut i = 0;
    while i < placed.len() {
        if !missing.contains(&placed[i].cluster) {
            out.push(placed[i]);
            i += 1;
            continue;
        }
        let mut j = i;
        while j < placed.len() && missing.contains(&placed[j].cluster) {
            j += 1;
        }
        let run = &placed[i..j];
        let lo = run.iter().map(|g| g.cluster).min().unwrap_or(0);
        let hi = run.iter().map(|g| g.cluster).max().unwrap_or(0);
        match line.get(lo as usize..end_of(hi) as usize) {
            Some(text) if !text.is_empty() => {
                let (filled, _) = run_shaper(shaper, text, Some(direction), 1, scale, lo);
                out.extend(filled);
            }
            _ => out.extend_from_slice(run),
        }
        i = j;
    }
    out
}

/// Umrisse je (Schrift, Glyphe), schon fett nachgebildet; `None`, wenn sie sich nicht
/// zeichnen lässt.
type OutlineCache = HashMap<(usize, u32), Option<Arc<Vec<Contour>>>>;

/// Umrisse einer Glyphe relativ zu ihrem Ursprung, mit nachgebildetem Fett und Kursiv.
fn glyph_outline(
    faces: &[Face<'_>],
    glyph: &Placed,
    cache: &mut OutlineCache,
) -> Option<Vec<Contour>> {
    let face = &faces[glyph.face];
    let base = cache
        .entry((glyph.face, glyph.gid))
        .or_insert_with(|| {
            let mut contours = glyph_contours(
                &face.outlines,
                LocationRef::from(&face.location),
                glyph.gid,
                face.scale,
            )
            .ok()?;
            if face.synth_bold {
                // Wie FreeType: 0,02 breiter und höher, linker und unterer Rand bleiben; der
                // Vorschub wächst in `assemble` um dieselbe Stärke.
                embolden(&mut contours, BOLD_STRENGTH);
                contours = contours
                    .into_iter()
                    .filter_map(|c| tidy(c.start, c.segs))
                    .collect();
            }
            Some(Arc::new(contours))
        })
        .clone()?;
    let mut contours = Vec::clone(&base);
    if face.synth_italic {
        shear(&mut contours, ITALIC_SHEAR, glyph.offset.y);
    }
    Some(contours)
}

/// Baut eine Zeile: Ursprünge mit Zeichenabstand und nachgebildetem Fett, Umrisse, Breite.
///
/// Der Zeichenabstand gilt je Cluster (ein Zeichen samt Akzenten): Alle Glyphen eines
/// Clusters behalten ihre Lage zueinander, sonst rutschte ein Akzent vom Buchstaben weg. Jeder
/// Cluster außer dem letzten der Zeile wird um `spacing %` seines Vorschubs länger.
fn assemble(
    placed: &[Placed],
    faces: &[Face<'_>],
    spacing: f64,
    cache: &mut OutlineCache,
    curves: &mut usize,
    missing: &mut bool,
) -> Result<ShapedLine, TextShapeError> {
    let factor = spacing / 100.0;
    let mut glyphs = Vec::with_capacity(placed.len());
    let mut pen = 0.0;
    let mut extra = 0.0;
    let mut i = 0;
    while i < placed.len() {
        let cluster = placed[i].cluster;
        let mut j = i;
        while j < placed.len() && placed[j].cluster == cluster {
            j += 1;
        }
        let mut cluster_advance = 0.0;
        let mut cluster_bold = 0.0;
        for glyph in &placed[i..j] {
            let face = &faces[glyph.face];
            let origin = DVec2::new(pen + extra + glyph.offset.x, glyph.offset.y);
            // `FT_GlyphSlot_Embolden`: Der Vorschub wächst um die Stärke.
            let bold = if face.synth_bold && glyph.advance != 0.0 {
                BOLD_STRENGTH
            } else {
                0.0
            };
            let contours = if glyph.gid == NOTDEF {
                // Fehlt auch im Ersatz: Platz bleibt leer, kein Kasten.
                *missing = true;
                Vec::new()
            } else {
                glyph_outline(faces, glyph, cache).unwrap_or_else(|| {
                    *missing = true;
                    Vec::new()
                })
            };
            *curves += contours.iter().map(|c| c.segs.len()).sum::<usize>();
            if *curves > MAX_TEXT_CURVES {
                return Err(TextShapeError::TooManyCurves);
            }
            glyphs.push(Glyph {
                origin,
                advance: glyph.advance + bold,
                contours,
            });
            pen += glyph.advance;
            cluster_advance += glyph.advance;
            cluster_bold += bold;
        }
        extra += cluster_bold;
        if j < placed.len() {
            extra += factor * cluster_advance;
        }
        i = j;
    }
    Ok(ShapedLine {
        glyphs,
        width: pen + extra,
    })
}

/// Setzt den ganzen Inhalt. `None`, wenn sich die gewählte Schrift nicht öffnen lässt.
fn shape_with(
    primary: &FaceChoice,
    fallback: &FaceChoice,
    content: &str,
    spacing: f64,
) -> Option<Result<TextShape, TextShapeError>> {
    let main = Face::new(primary)?;
    let same = Arc::ptr_eq(&primary.face.data, &fallback.face.data)
        && primary.face.index == fallback.face.index
        && primary.face.wght == fallback.face.wght;
    let spare = if same { None } else { Face::new(fallback) };

    let main_data = ShaperData::new(&main.font);
    let main_instance = main.instance();
    let main_shaper = main_data
        .shaper(&main.font)
        .instance(main_instance.as_ref())
        .build();
    // Der Ersatz wird erst vorbereitet, wenn ein Zeichen fehlt.
    let spare_data: OnceCell<ShaperData> = OnceCell::new();
    let spare_instance = spare.as_ref().and_then(Face::instance);

    let mut faces = vec![main];
    let mut spare_used = false;
    let mut placed_lines = Vec::new();
    for line in split_lines(content) {
        if line.is_empty() {
            placed_lines.push(Vec::new());
            continue;
        }
        let (mut placed, direction) = run_shaper(&main_shaper, &line, None, 0, faces[0].scale, 0);
        if let Some(spare) = &spare
            && placed.iter().any(|g| g.gid == NOTDEF)
        {
            let data = spare_data.get_or_init(|| ShaperData::new(&spare.font));
            let shaper = data
                .shaper(&spare.font)
                .instance(spare_instance.as_ref())
                .build();
            placed = fill_missing(placed, &line, direction, &shaper, spare.scale);
            spare_used |= placed.iter().any(|g| g.face == 1);
        }
        placed_lines.push(placed);
    }
    if let Some(spare) = spare {
        faces.push(spare);
    }

    let mut cache = OutlineCache::new();
    let mut curves = 0;
    let mut missing = false;
    let mut lines = Vec::with_capacity(placed_lines.len());
    for placed in &placed_lines {
        match assemble(
            placed,
            &faces,
            spacing,
            &mut cache,
            &mut curves,
            &mut missing,
        ) {
            Ok(line) => lines.push(line),
            Err(err) => return Some(Err(err)),
        }
    }
    let (ascender, descender, gap) = faces[0].line_metrics();
    let synthetic = primary.synth_bold
        || primary.synth_italic
        || (spare_used && (fallback.synth_bold || fallback.synth_italic));
    Some(Ok(TextShape {
        lines,
        ascender,
        descender,
        line_height: ascender - descender + gap,
        substituted: None,
        synthetic,
        missing_chars: missing,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_face_is_not_shaped() {
        let lib = FontLibrary::bundled_only();
        let fallback = lib.select(GLYPH_FALLBACK_FAMILY, false, false);
        let mut broken = lib.select("Montserrat", false, false);
        broken.face.data = Arc::from(&b"keine Schrift"[..]);
        assert!(shape_with(&broken, &fallback, "a", 0.0).is_none());
        assert!(shape_guarded(&broken, &fallback, "a", 0.0).is_none());
    }

    #[test]
    fn spacing_outside_range_is_clamped() {
        assert_eq!(sanitize_spacing(f64::NAN), 0.0);
        assert_eq!(sanitize_spacing(f64::INFINITY), 0.0);
        assert_eq!(sanitize_spacing(-5.0), 0.0);
        assert_eq!(sanitize_spacing(5000.0), 1000.0);
        assert_eq!(sanitize_spacing(12.5), 12.5);
    }

    #[test]
    fn lines_split_on_every_line_break_and_tabs_become_spaces() {
        assert_eq!(split_lines("a\r\nb\rc\nd\te"), ["a", "b", "c", "d    e"]);
        assert_eq!(split_lines("x\n"), ["x", ""]);
    }
}
