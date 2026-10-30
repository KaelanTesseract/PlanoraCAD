//! Schraffur, Schnittlinie und Titel einer Schnittansicht als Markierungen (Teilprojekt 3d).
//!
//! Alle Längen sind Blatt-Millimeter; `f` ist der Blattfaktor [`sheet_scale`] (A3 = 1).
//!
//! - **S6 Lage der Schnittlinie:** in der Quellansicht waagerecht bei Ansichts-y
//!   `offset · ey[a]`, senkrecht bei x `offset · ex[a]` ([`section_line_coord`]); die Länge ist
//!   die Hüllbox der Quellansicht auf dem Blatt plus [`SECTION_OVERHANG_MM`]` · f` an jedem Ende.
//! - **S7 Schraffur:** in Blattkoordinaten nach dem Abbilden, Abstand `spacing · f` unabhängig
//!   vom Ansichtsmaßstab. Die Linienschar eines Körpers hat die Phase `c₀ = xf(0, 0) · n`, alle
//!   Flächen desselben Körpers liegen also auf denselben Linien. Der Winkel wechselt nach dem
//!   Rang des Körpers unter den Körpern mit Schnittfläche: gerade 45°, ungerade 135°. Der Rand
//!   ist ein Vieleck mit höchstens [`HATCH_FLAT_TOL`] Abweichung auf dem Blatt. Höchstens
//!   [`MAX_HATCH_LINES`] Stücke je Ansicht; mehr ergibt die Meldung „Die Schraffur von
//!   „Schnitt A–A“ ist zu fein.“ (ohne Zahl) und keine Schraffur dieser Ansicht.
//! - **S8 Schnittlinie:** Kette ([`SectionPen::Chain`], strichpunktiert), an beiden Enden
//!   [`SECTION_END_MM`]` · f` dick ([`SectionPen::End`]); an jedem Ende ein Schaft von
//!   [`SECTION_SHAFT_MM`]` · f` in Blickrichtung und ein Pfeil, dessen Spitze
//!   `(SECTION_SHAFT_MM + SECTION_ARROW_LEN) · f` vom Ende entfernt liegt und gegen die Blickrichtung zeigt. Die
//!   Blickrichtung `v` der Pfeile ergibt sich aus der Art der Schnittansicht (waagerechte Linie:
//!   nach unten, senkrechte: nach rechts, umgedreht entgegengesetzt). Der Buchstabe steht mittig
//!   hinter der Spitze, Höhe [`DIM_TEXT_H`].
//! - **S9 Titel:** „A–A (1:2)“, Höhe [`SECTION_TITLE_H`]` · f`, mittig, die Grundlinie
//!   [`SECTION_TITLE_GAP`]` · f` plus Texthöhe unter der Unterkante der Schnittansicht.
//!
//! Ansichten ohne Ergebnis, die nicht gezeichnet werden, bekommen nichts; eine nicht gezeichnete
//! Quelle ergibt keine Schnittlinie (Titel und Schraffur bleiben).

use crate::annotate::{
    DIM_TEXT_H, HATCH_FLAT_TOL, Mark, MarkKind, SECTION_ARROW_LEN, SECTION_END_MM,
    SECTION_OVERHANG_MM, SECTION_SHAFT_MM, SECTION_TITLE_GAP, SECTION_TITLE_H, SectionPen,
    TextAnchor,
};
use crate::hatch::{HatchError, MAX_HATCH_LINES, hatch_lines};
use crate::model::{Drawing, Scale, ViewId, ViewSpec, section_line_coord};
use crate::section::{SectionData, loop_polygon};
use crate::sheet::{SheetModel, view_transform};
use crate::sheet_frame::sheet_scale;
use crate::{Basis, ViewLines};
use planora_i18n::tr;
use planora_math::DVec2;
use std::collections::BTreeMap;
use std::f64::consts::FRAC_PI_4;

/// Titel der Schnittansicht, z. B. „A–A (1:2)“.
pub fn section_title(letter: char, scale: Scale) -> String {
    format!("{letter}\u{2013}{letter} ({})", scale.label())
}

/// Name der Schnittansicht in der aktuellen Sprache, z. B. „Schnitt A–A“.
pub fn section_view_name(letter: char) -> String {
    tr!("drawing-view-section", letter = letter.to_string())
}

/// Markierungen, die zu einer Ansicht des Blatts hinzukommen.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionMarks {
    pub view: ViewId,
    pub marks: Vec<Mark>,
}

fn finite(p: DVec2) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn mark_finite(m: &Mark) -> bool {
    match m {
        Mark::Line { a, b, .. } => finite(*a) && finite(*b),
        Mark::Arrow { tip, dir, .. } => finite(*tip) && finite(*dir),
        Mark::Arc { center, r, .. } => finite(*center) && r.is_finite(),
        Mark::Text { pos, height, .. } => finite(*pos) && height.is_finite(),
    }
}

fn line(a: DVec2, b: DVec2, pen: SectionPen) -> Mark {
    Mark::Line {
        a,
        b,
        kind: MarkKind::SectionLine(pen),
    }
}

/// Schraffur, Schnittlinie und Titel aller Schnittansichten der Zeichnung. Je Schnittansicht
/// mit Ergebnis höchstens zwei Einträge: Schraffur und Titel (bei der Schnittansicht) und die
/// Schnittlinie (bei der Quellansicht). Die zweite Liste enthält je Ansicht mit zu feiner
/// Schraffur eine Meldung.
pub fn section_marks(
    drawing: &Drawing,
    results: &BTreeMap<ViewId, &ViewLines>,
    sections: &BTreeMap<ViewId, &SectionData>,
) -> (Vec<SectionMarks>, Vec<String>) {
    let f = sheet_scale(drawing.sheet);
    let mut out = Vec::new();
    let mut errors = Vec::new();
    if !f.is_finite() || f <= 0.0 {
        return (out, errors);
    }
    for spec in &drawing.views {
        let Some(sec) = &spec.section else { continue };
        let Some(lines) = results.get(&spec.id) else {
            continue;
        };
        let Some(xf) = view_transform(drawing, spec.id, results) else {
            continue;
        };
        let mut own: Vec<Mark> = Vec::new();
        if sec.hatch.on
            && let Some(data) = sections.get(&spec.id)
        {
            match hatch_marks(data, lines, &xf, spec, sec.hatch.spacing * f) {
                Ok(m) => own.extend(m),
                Err(HatchError::TooMany(_)) => errors.push(tr!(
                    "drawing-err-hatch-too-fine",
                    name = section_view_name(sec.letter)
                )),
                Err(HatchError::BadInput) => errors.push(tr!(
                    "drawing-err-hatch-failed",
                    name = section_view_name(sec.letter)
                )),
            }
        }
        own.push(title_mark(sec.letter, spec.scale, lines, &xf, f));
        own.retain(mark_finite);
        if !own.is_empty() {
            out.push(SectionMarks {
                view: spec.id,
                marks: own,
            });
        }
        if let Some(parent) = spec.parent
            && let Some(src) = line_marks(drawing, spec, parent, results, f)
            && !src.is_empty()
        {
            out.push(SectionMarks {
                view: parent,
                marks: src,
            });
        }
    }
    (out, errors)
}

/// Hängt die Markierungen an die Ansichten des Blatts. Unbekannte Ansichten werden übergangen.
pub fn add_section_marks(model: &mut SheetModel, list: &[SectionMarks]) {
    for entry in list {
        if let Some(v) = model.views.iter_mut().find(|v| v.id == entry.view) {
            v.marks.extend(entry.marks.iter().cloned());
        }
    }
}

/// Schraffurlinien aller Körper mit Schnittfläche.
fn hatch_marks(
    data: &SectionData,
    lines: &ViewLines,
    xf: &crate::sheet::SheetTransform,
    spec: &ViewSpec,
    spacing: f64,
) -> Result<Vec<Mark>, HatchError> {
    let tol = HATCH_FLAT_TOL / spec.scale.factor();
    let mut bodies: Vec<usize> = data.regions.iter().map(|r| r.body).collect();
    bodies.sort_unstable();
    bodies.dedup();
    let origin = xf.apply(DVec2::new(0.0, 0.0));
    let mut marks = Vec::new();
    let mut total = 0usize;
    for (rank, body) in bodies.iter().enumerate() {
        let angle = if rank % 2 == 0 {
            FRAC_PI_4
        } else {
            3.0 * FRAC_PI_4
        };
        for region in data.regions.iter().filter(|r| r.body == *body) {
            let polys: Vec<Vec<DVec2>> = region
                .loops
                .iter()
                .map(|lp| {
                    loop_polygon(lp, &lines.basis, tol)
                        .into_iter()
                        .map(|p| xf.apply(p))
                        .collect()
                })
                .collect();
            let pieces = hatch_lines(
                &polys,
                angle,
                spacing,
                origin,
                MAX_HATCH_LINES.saturating_sub(total),
            )?;
            total += pieces.len();
            marks.extend(pieces.into_iter().map(|(a, b)| Mark::Line {
                a,
                b,
                kind: MarkKind::Hatch,
            }));
        }
    }
    Ok(marks)
}

/// Titel unter der Schnittansicht.
fn title_mark(
    letter: char,
    scale: Scale,
    lines: &ViewLines,
    xf: &crate::sheet::SheetTransform,
    f: f64,
) -> Mark {
    let ((x0, y0), (x1, _)) = lines.bbox;
    let bottom = xf.apply(DVec2::new(0.5 * (x0 + x1), y0));
    let h = SECTION_TITLE_H * f;
    Mark::Text {
        pos: DVec2::new(bottom.x, bottom.y - SECTION_TITLE_GAP * f - h),
        height: h,
        angle: 0.0,
        anchor: TextAnchor::Middle,
        text: section_title(letter, scale),
        kind: MarkKind::SectionLine(SectionPen::Chain),
        fit_width: None,
    }
}

/// Schnittlinie mit Endstücken, Schäften, Pfeilen und Buchstaben in der Quellansicht.
fn line_marks(
    drawing: &Drawing,
    spec: &ViewSpec,
    parent: ViewId,
    results: &BTreeMap<ViewId, &ViewLines>,
    f: f64,
) -> Option<Vec<Mark>> {
    let sec = spec.section.as_ref()?;
    let src = drawing.view(parent)?;
    let src_lines = results.get(&parent)?;
    let xf = view_transform(drawing, parent, results)?;
    let (horizontal, coord) = section_line_coord(src.kind, &sec.plane(spec.kind))?;
    let ((x0, y0), (x1, y1)) = src_lines.bbox;
    let (pa, pb) = if horizontal {
        (DVec2::new(x0, coord), DVec2::new(x1, coord))
    } else {
        (DVec2::new(coord, y0), DVec2::new(coord, y1))
    };
    let (a, b) = (xf.apply(pa), xf.apply(pb));
    let u = if horizontal {
        DVec2::new(1.0, 0.0)
    } else {
        DVec2::new(0.0, 1.0)
    };
    // Blickrichtung auf dem Blatt (S5): aus der Blickrichtung der Schnittansicht.
    let pbasis = Basis::of(src.kind);
    let kd = Basis::of(spec.kind).d;
    let v = DVec2::new(kd.dot(pbasis.ex), kd.dot(pbasis.ey));
    let v = if v.length() > 0.5 {
        DVec2::new(v.x.round(), v.y.round())
    } else {
        return None;
    };
    let (a, b) = (
        a - u * (SECTION_OVERHANG_MM * f),
        b + u * (SECTION_OVERHANG_MM * f),
    );
    let total = (b - a).dot(u);
    if total.is_nan() || total <= 0.0 {
        return None;
    }
    let end = (SECTION_END_MM * f).min(0.5 * total);
    let mut marks = vec![
        line(a, a + u * end, SectionPen::End),
        line(b - u * end, b, SectionPen::End),
    ];
    if total > 2.0 * end {
        marks.push(line(a + u * end, b - u * end, SectionPen::Chain));
    }
    for e in [a, b] {
        marks.push(line(e, e + v * (SECTION_SHAFT_MM * f), SectionPen::Chain));
        let tip = e + v * ((SECTION_SHAFT_MM + SECTION_ARROW_LEN) * f);
        marks.push(Mark::Arrow {
            tip,
            dir: DVec2::new(-v.x, -v.y),
            kind: MarkKind::SectionLine(SectionPen::Chain),
        });
        let centre = tip + v * (DIM_TEXT_H / 2.0 + f);
        marks.push(Mark::Text {
            pos: DVec2::new(centre.x, centre.y - DIM_TEXT_H / 2.0),
            height: DIM_TEXT_H,
            angle: 0.0,
            anchor: TextAnchor::Middle,
            text: sec.letter.to_string(),
            kind: MarkKind::SectionLine(SectionPen::Chain),
            fit_width: None,
        });
    }
    marks.retain(mark_finite);
    Some(marks)
}
