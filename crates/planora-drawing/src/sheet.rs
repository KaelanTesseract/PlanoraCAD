//! Blattmodell: die Linien aller berechneten Ansichten in Blattkoordinaten (mm, y nach oben,
//! Ursprung unten links) mit Maßstab, Lage und Innenrahmen.

use crate::ViewLines;
use crate::Visibility;
use crate::layout::{align_dir, view_extent};
use crate::model::{Drawing, Placement, Sheet, ViewId, ViewKind};
use crate::out2d::{Out2d, try_piece_to_out};
use planora_math::DVec2;
use std::collections::BTreeMap;

/// Rand zwischen Blattkante und Innenrahmen in mm.
pub const FRAME_MARGIN_MM: f64 = 10.0;

/// Abbildung von Ansichtskoordinaten auf das Blatt: `Blatt = scale·(p − bbox_center) + (tx, ty)`.
#[derive(Clone, Copy, Debug)]
pub struct SheetTransform {
    pub scale: f64,
    pub tx: f64,
    pub ty: f64,
    pub bbox_center: DVec2,
}

impl SheetTransform {
    /// Bildet einen Punkt der Ansicht auf das Blatt ab.
    pub fn apply(&self, p: DVec2) -> DVec2 {
        (p - self.bbox_center) * self.scale + DVec2::new(self.tx, self.ty)
    }
}

/// Eine Linie auf dem Blatt.
#[derive(Clone, Debug)]
pub struct SheetLine {
    pub vis: Visibility,
    pub curve: Out2d,
}

/// Eine Ansicht auf dem Blatt.
#[derive(Clone, Debug)]
pub struct SheetView {
    pub id: ViewId,
    pub name: String,
    pub lines: Vec<SheetLine>,
    /// Breite und Höhe auf dem Blatt in mm.
    pub extent_mm: (f64, f64),
}

/// Alle Linien eines Blatts.
#[derive(Clone, Debug)]
pub struct SheetModel {
    pub sheet: Sheet,
    /// Innenrahmen `(min, max)`, 10 mm vom Blattrand.
    pub frame: (DVec2, DVec2),
    pub views: Vec<SheetView>,
}

/// Mitte der Ansicht auf dem Blatt: frei platziert `(x, y)`, abgeleitet Mitte des Elternbildes
/// plus `distance · align_dir`. `None`, wenn die Ansicht unbekannt ist, das Elternbild fehlt oder
/// nicht berechnet ist, die Ansicht nicht abgeleitet werden kann oder eine Zahl nicht endlich ist.
pub fn view_center(
    drawing: &Drawing,
    view: ViewId,
    results: &BTreeMap<ViewId, &ViewLines>,
) -> Option<DVec2> {
    center_rec(drawing, view, results, drawing.views.len())
}

fn center_rec(
    drawing: &Drawing,
    view: ViewId,
    results: &BTreeMap<ViewId, &ViewLines>,
    depth: usize,
) -> Option<DVec2> {
    let spec = drawing.view(view)?;
    let c = match spec.placement {
        Placement::Free { x, y } => DVec2::new(x, y),
        Placement::Aligned { distance } => {
            // Tiefe begrenzt: ein Kreis in den Elternverweisen (fremde Datei) ergibt `None`.
            let depth = depth.checked_sub(1)?;
            let parent = drawing.view(spec.parent?)?;
            if !results.contains_key(&parent.id) {
                return None;
            }
            let pc = center_rec(drawing, parent.id, results, depth)?;
            let (dx, dy) = align_dir(drawing.projection, parent.kind, spec.kind)?;
            pc + DVec2::new(dx, dy) * distance
        }
    };
    c.is_finite().then_some(c)
}

/// Baut das Blattmodell: je Ansicht mit Ergebnis die Stücke (verdeckte nur bei `show_hidden`)
/// als Ausgabekurven. Ansichten ohne Ergebnis, ohne berechenbare Lage, mit ungültigem Maßstab
/// oder nicht endlicher Hüllbox fehlen in `views`; Stücke mit nicht endlichen Zahlen entfallen.
/// Namen: `ViewKind::label`, bei mehrfach gleicher Art ` 2`, ` 3` (in Reihenfolge der Ansichten
/// der Zeichnung, auch wenn eine frühere Ansicht gerade nicht berechnet ist).
pub fn build_sheet(drawing: &Drawing, results: &BTreeMap<ViewId, &ViewLines>) -> SheetModel {
    let (w, h) = drawing.sheet.size_mm();
    let m = FRAME_MARGIN_MM;
    let mut counts: BTreeMap<u8, usize> = BTreeMap::new();
    let mut views = Vec::new();
    for spec in &drawing.views {
        let n = counts.entry(kind_index(spec.kind)).or_insert(0);
        *n += 1;
        let name = if *n == 1 {
            spec.kind.label().to_string()
        } else {
            format!("{} {}", spec.kind.label(), n)
        };
        let Some(lines) = results.get(&spec.id) else {
            continue;
        };
        let Some(center) = view_center(drawing, spec.id, results) else {
            continue;
        };
        if !spec.scale.is_valid() {
            continue;
        }
        let ((x0, y0), (x1, y1)) = lines.bbox;
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
            continue;
        }
        let xf = SheetTransform {
            scale: spec.scale.factor(),
            tx: center.x,
            ty: center.y,
            bbox_center: DVec2::new(0.5 * (x0 + x1), 0.5 * (y0 + y1)),
        };
        let mut out = Vec::new();
        for p in &lines.pieces {
            if p.vis == Visibility::Hidden && !spec.show_hidden {
                continue;
            }
            let Some(stroke) = lines.strokes.get(p.stroke) else {
                continue;
            };
            if let Some(curve) = try_piece_to_out(stroke, &lines.basis, p.t0, p.t1, &xf) {
                out.push(SheetLine { vis: p.vis, curve });
            }
        }
        views.push(SheetView {
            id: spec.id,
            name,
            lines: out,
            extent_mm: view_extent(lines.bbox, spec.scale),
        });
    }
    SheetModel {
        sheet: drawing.sheet,
        frame: (DVec2::new(m, m), DVec2::new(w - m, h - m)),
        views,
    }
}

fn kind_index(k: ViewKind) -> u8 {
    ViewKind::ALL.iter().position(|&x| x == k).unwrap_or(0) as u8
}
