//! Nummernkreise (Positionsnummern) als Markierungen auf dem Blatt (Teilprojekt 3c-2).
//!
//! Ein Kreis gehört zu einer Ansicht: sein Anker liegt relativ zur Hüllbox der Ansicht
//! (`ViewLines.bbox`, [0, 1]²) und folgt so Maßstab und Lage der Ansicht; der Versatz zur
//! Kreismitte ist in Blatt-mm. Alle Maße sind A3-Werte mal Faktor `f` des Blatts
//! ([`sheet_scale`]):
//! - Kreis Ø [`BALLOON_D`] ([`PositionPen::Circle`]),
//! - Zahl [`BALLOON_TEXT_H`] mittig, Grundlinie `Mitte.y − h/2`, in `0,8 · Ø` eingepasst
//!   ([`fit_text`]),
//! - Hinweislinie ([`PositionPen::Leader`]) vom Anker bis zum Kreisrand; entfällt, wenn der
//!   Anker im Kreis liegt,
//! - Punkt am Teil als Kreis r [`BALLOON_DOT_R`] (`Leader`).
//!
//! Ein verlorener Kreis (Teil gibt es nicht mehr) hat dieselbe Geometrie in
//! [`MarkKind::Lost`] mit dem Text „?“. Nicht endliche Eingaben ergeben keine Markierungen.

use crate::ViewLines;
use crate::annotate::{Mark, MarkKind, TextAnchor};
use crate::model::{BalloonId, Drawing, ViewId};
use crate::sheet::{SheetModel, SheetTransform, view_transform};
use crate::sheet_frame::{MIN_PEN_MM, sheet_scale};
use crate::sheet_marks::{DXF_TEXT_WIDTH_FACTOR, fit_text, sheet_text_width};
use planora_math::{DVec2, TAU};
use std::collections::BTreeMap;

/// Durchmesser des Kreises (A3).
pub const BALLOON_D: f64 = 8.0;
/// Schrifthöhe der Zahl (A3).
pub const BALLOON_TEXT_H: f64 = 3.5;
/// Radius des Punkts am Teil (A3).
pub const BALLOON_DOT_R: f64 = 0.5;
/// Vorgabe für den Versatz vom Anker zur Kreismitte (A3, schräg rechts oben).
pub const DEFAULT_BALLOON_OFFSET: (f64, f64) = (10.0, 10.0);
/// Anteil des Durchmessers, in den die Zahl eingepasst wird.
const TEXT_FIT: f64 = 0.8;
/// Text eines verlorenen Kreises.
const LOST_TEXT: &str = "?";

/// Linienart eines Nummernkreises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionPen {
    /// Kreis und Zahl.
    Circle,
    /// Hinweislinie und Punkt am Teil.
    Leader,
}

impl PositionPen {
    /// Strichbreite auf A3 in Millimetern.
    pub fn width_mm(self) -> f64 {
        match self {
            PositionPen::Circle => 0.35,
            PositionPen::Leader => 0.25,
        }
    }

    /// Strichbreite beim Faktor `scale` des Blatts, nie unter [`MIN_PEN_MM`].
    pub fn width_at(self, scale: f64) -> f64 {
        (self.width_mm() * scale).max(MIN_PEN_MM)
    }
}

/// Punkt auf dem Blatt zum Anker `anchor` (relativ zur Hüllbox `bbox` der Ansicht).
pub fn anchor_point(
    bbox: ((f64, f64), (f64, f64)),
    xf: &SheetTransform,
    anchor: (f64, f64),
) -> DVec2 {
    let ((x0, y0), (x1, y1)) = bbox;
    let p = DVec2::new(x0 + anchor.0 * (x1 - x0), y0 + anchor.1 * (y1 - y0));
    xf.apply(p)
}

/// Anker (relativ zur Hüllbox `bbox`) des Blattpunkts `at`, je Richtung auf [0, 1] begrenzt.
/// Hat die Hüllbox in einer Richtung keine Ausdehnung oder ist eine Zahl nicht endlich, gilt dort
/// die Mitte (0,5).
pub fn anchor_of(bbox: ((f64, f64), (f64, f64)), xf: &SheetTransform, at: DVec2) -> (f64, f64) {
    let ((x0, y0), (x1, y1)) = bbox;
    let p = (at - DVec2::new(xf.tx, xf.ty)) / xf.scale + xf.bbox_center;
    let rel = |v: f64, lo: f64, hi: f64| {
        let r = (v - lo) / (hi - lo);
        if r.is_finite() {
            r.clamp(0.0, 1.0)
        } else {
            0.5
        }
    };
    (rel(p.x, x0, x1), rel(p.y, y0, y1))
}

/// Markierungen eines Nummernkreises mit dem Anker `at` (Blatt-mm), dem Versatz `offset` zur
/// Kreismitte und der Nummer `number` (`None`: verloren) beim Faktor `scale` des Blatts.
/// Nicht endliche Eingaben (oder `scale ≤ 0`) ergeben keine Markierungen.
pub fn layout_balloon(at: DVec2, offset: (f64, f64), number: Option<u32>, scale: f64) -> Vec<Mark> {
    let off = DVec2::new(offset.0, offset.1);
    if !(at.is_finite() && off.is_finite() && scale.is_finite() && scale > 0.0) {
        return Vec::new();
    }
    let centre = at + off;
    let r = 0.5 * BALLOON_D * scale;
    let (circle, leader, text) = match number {
        Some(n) => (
            MarkKind::Position(PositionPen::Circle),
            MarkKind::Position(PositionPen::Leader),
            n.to_string(),
        ),
        None => (MarkKind::Lost, MarkKind::Lost, LOST_TEXT.to_string()),
    };
    let full = |center: DVec2, r: f64, kind: MarkKind| Mark::Arc {
        center,
        r,
        start: 0.0,
        end: TAU,
        kind,
    };
    let mut marks = vec![full(centre, r, circle)];
    let len = off.length();
    if len > r {
        marks.push(Mark::Line {
            a: at,
            b: centre - off * (r / len),
            kind: leader,
        });
    }
    marks.push(full(at, BALLOON_DOT_R * scale, leader));
    let fitted = fit_text(&text, BALLOON_TEXT_H * scale, TEXT_FIT * BALLOON_D * scale);
    marks.push(Mark::Text {
        pos: DVec2::new(centre.x, centre.y - 0.5 * fitted.height),
        height: fitted.height,
        angle: 0.0,
        anchor: TextAnchor::Middle,
        text: fitted.text,
        kind: circle,
    });
    marks
}

/// Die Zahl eines Nummernkreises für DXF: CAD-Schriften laufen bis zu
/// [`DXF_TEXT_WIDTH_FACTOR`]-mal breiter als die Schätzung, deshalb wird ein Text der Art
/// `Position(Circle)` so weit verkleinert, dass `Schätzung · Zuschlag ≤ 0,8 · Ø · scale` gilt
/// (ohne Untergrenze, nie gekürzt), und bleibt dabei senkrecht mittig. Alle anderen
/// Markierungen bleiben unverändert. `scale` ist der Faktor des Blatts.
pub fn balloon_text_dxf(m: &Mark, scale: f64) -> Mark {
    let Mark::Text {
        pos,
        height,
        text,
        kind: MarkKind::Position(PositionPen::Circle),
        ..
    } = m
    else {
        return m.clone();
    };
    let unit = sheet_text_width(text, 1.0) * DXF_TEXT_WIDTH_FACTOR;
    let room = TEXT_FIT * BALLOON_D * scale;
    if !(unit > 0.0 && room.is_finite() && unit * height > room) {
        return m.clone();
    }
    let h = room / unit;
    let mut out = m.clone();
    if let Mark::Text {
        pos: p, height: hh, ..
    } = &mut out
    {
        *p = DVec2::new(pos.x, pos.y + 0.5 * (height - h));
        *hh = h;
    }
    out
}

/// Die Markierungen eines Nummernkreises auf dem Blatt (für Auswahl und Ziehen der Oberfläche).
#[derive(Clone, Debug, PartialEq)]
pub struct BalloonMarks {
    pub view: ViewId,
    pub id: BalloonId,
    /// Kreismitte auf dem Blatt.
    pub centre: DVec2,
    /// Kreisradius auf dem Blatt.
    pub radius: f64,
    pub marks: Vec<Mark>,
}

/// Die Nummernkreise der Zeichnung in ihrer Reihenfolge, je mit Nummer aus `numbers` (fehlt sie
/// oder ist sie `None`, ist der Kreis verloren). Kreise von Ansichten, die nicht gezeichnet
/// werden, und Kreise mit nicht endlichen Zahlen fehlen.
pub fn balloon_marks(
    drawing: &Drawing,
    results: &BTreeMap<ViewId, &ViewLines>,
    numbers: &BTreeMap<BalloonId, Option<u32>>,
) -> Vec<BalloonMarks> {
    let scale = sheet_scale(drawing.sheet);
    let mut out = Vec::new();
    for b in &drawing.balloons {
        let (Some(xf), Some(lines)) = (
            view_transform(drawing, b.view, results),
            results.get(&b.view),
        ) else {
            continue;
        };
        let at = anchor_point(lines.bbox, &xf, b.anchor);
        let number = numbers.get(&b.id).copied().flatten();
        let marks = layout_balloon(at, b.offset, number, scale);
        if marks.is_empty() {
            continue;
        }
        out.push(BalloonMarks {
            view: b.view,
            id: b.id,
            centre: at + DVec2::new(b.offset.0, b.offset.1),
            radius: 0.5 * BALLOON_D * scale,
            marks,
        });
    }
    out
}

/// Hängt die Markierungen der Kreise an die Markierungen ihrer Ansicht im Blattmodell an
/// (Ansichten, die im Modell fehlen, gehen leer aus).
pub fn add_balloon_marks(model: &mut SheetModel, list: &[BalloonMarks]) {
    for bm in list {
        if let Some(v) = model.views.iter_mut().find(|v| v.id == bm.view) {
            v.marks.extend(bm.marks.iter().cloned());
        }
    }
}
