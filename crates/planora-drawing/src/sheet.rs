//! Blattmodell: die Linien aller berechneten Ansichten in Blattkoordinaten (mm, y nach oben,
//! Ursprung unten links) mit Maßstab, Lage und Innenrahmen.

use crate::ViewLines;
use crate::Visibility;
use crate::annotate::{
    DimEntry, GroupBase, Mark, ResolvedDims, group_base, layout_centerline, layout_dimension_in,
    layout_lost_in,
};
use crate::dimension::{ChainMode, Measured};
use crate::layout::{align_dir, view_extent};
use crate::model::ViewSpec;
use crate::model::{Drawing, Placement, Sheet, ViewId, ViewKind};
use crate::out2d::{Out2d, piece_to_outs};
use crate::stroke::StrokeOrigin;
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
    /// Herkunft des Strichs, aus dem die Linie stammt (Kante oder Umriss).
    pub origin: StrokeOrigin,
}

/// Eine Ansicht auf dem Blatt.
#[derive(Clone, Debug)]
pub struct SheetView {
    pub id: ViewId,
    pub name: String,
    pub lines: Vec<SheetLine>,
    /// Breite und Höhe auf dem Blatt in mm.
    pub extent_mm: (f64, f64),
    /// Markierungen der Ansicht in Blattkoordinaten: Mittellinien (nur bei
    /// `ViewSpec.centerlines`), Maße und verlorene Maße.
    pub marks: Vec<Mark>,
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
            // Ein Elternbild, das `build_sheet` auslässt, positioniert kein Kind.
            if !is_drawable(parent, results) {
                return None;
            }
            let pc = center_rec(drawing, parent.id, results, depth)?;
            let (dx, dy) = align_dir(drawing.projection, parent.kind, spec.kind)?;
            pc + DVec2::new(dx, dy) * distance
        }
    };
    c.is_finite().then_some(c)
}

/// Kann die Ansicht gezeichnet werden: Ergebnis vorhanden, gültiger Maßstab, endliche Hüllbox?
/// Gemeinsame Regel von `build_sheet` (Ansicht erscheint) und `view_center` (Elternbild
/// positioniert Kinder).
fn is_drawable(spec: &ViewSpec, results: &BTreeMap<ViewId, &ViewLines>) -> bool {
    let Some(lines) = results.get(&spec.id) else {
        return false;
    };
    let ((x0, y0), (x1, y1)) = lines.bbox;
    spec.scale.is_valid() && [x0, y0, x1, y1].iter().all(|v| v.is_finite())
}

/// Baut das Blattmodell: je Ansicht mit Ergebnis die Stücke (verdeckte nur bei `show_hidden`)
/// als Ausgabekurven. Ansichten ohne Ergebnis, ohne berechenbare Lage, mit ungültigem Maßstab
/// oder nicht endlicher Hüllbox (oder ohne zeichenbares Elternbild) fehlen in `views`; Stücke mit nicht endlichen Zahlen entfallen.
/// Namen: `ViewKind::label`, bei mehrfach gleicher Art ` 2`, ` 3` (in Reihenfolge der Ansichten
/// der Zeichnung, auch wenn eine frühere Ansicht gerade nicht berechnet ist).
pub fn build_sheet(drawing: &Drawing, results: &BTreeMap<ViewId, &ViewLines>) -> SheetModel {
    build_sheet_with(drawing, results, &ResolvedDims::default())
}

/// Wie [`build_sheet`], dazu je Ansicht die Markierungen (`SheetView.marks`): die Mittellinien
/// der Ansicht (`ViewLines.centerlines`, nur bei `ViewSpec.centerlines`, mit Überstand nach
/// [`layout_centerline`]) und die Maße aus `dims.by_view` (gemessene nach
/// [`layout_dimension_in`], fehlgeschlagene als verlorene Maße nach [`layout_lost`] mit der
/// letzten bekannten Lage aus `dims.last_known` und dem Fehlertext als Grund). Gruppen
/// (`ChainSpec.group`) teilen ihren Bezug quer zur Messrichtung ([`group_base`]); Bezugsmaße
/// stapeln um `order · DIM_STEP`. Linien und Ausdehnung sind dieselben wie ohne Markierungen.
/// Maße von Ansichten, die nicht gezeichnet werden, entfallen.
pub fn build_sheet_with(
    drawing: &Drawing,
    results: &BTreeMap<ViewId, &ViewLines>,
    dims: &ResolvedDims,
) -> SheetModel {
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
        if !is_drawable(spec, results) {
            continue;
        }
        let Some(lines) = results.get(&spec.id) else {
            continue;
        };
        let Some(center) = view_center(drawing, spec.id, results) else {
            continue;
        };
        let ((x0, y0), (x1, y1)) = lines.bbox;
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
            for curve in piece_to_outs(stroke, &lines.basis, p.t0, p.t1, &xf) {
                out.push(SheetLine {
                    vis: p.vis,
                    curve,
                    origin: stroke.origin.clone(),
                });
            }
        }
        let mut marks = Vec::new();
        if spec.centerlines {
            for c in &lines.centerlines {
                marks.extend(layout_centerline(c, &xf));
            }
        }
        if let Some(list) = dims.by_view.get(&spec.id) {
            marks.extend(view_dimension_marks(list, dims, &xf));
        }
        views.push(SheetView {
            id: spec.id,
            name,
            lines: out,
            extent_mm: view_extent(lines.bbox, spec.scale),
            marks,
        });
    }
    SheetModel {
        sheet: drawing.sheet,
        frame: (DVec2::new(m, m), DVec2::new(w - m, h - m)),
        views,
    }
}

/// Markierungen der Maße einer Ansicht (Regeln siehe [`build_sheet_with`]).
fn view_dimension_marks(list: &[DimEntry], dims: &ResolvedDims, xf: &SheetTransform) -> Vec<Mark> {
    let mut members: BTreeMap<u64, Vec<&Measured>> = BTreeMap::new();
    for (s, r) in list {
        if let (Some(c), Ok(m)) = (s.chain, r) {
            members.entry(c.group).or_default().push(m);
        }
    }
    let bases: BTreeMap<u64, GroupBase> = members
        .iter()
        .filter_map(|(g, ms)| group_base(ms, xf).map(|b| (*g, b)))
        .collect();
    let mut marks = Vec::new();
    // Bereits gezeichnete Linien je Gruppe: Hilfslinien an gemeinsamen Punkten einer Kette
    // erscheinen nur einmal.
    let mut drawn: BTreeMap<u64, Vec<Mark>> = BTreeMap::new();
    for (s, r) in list {
        let base = s.chain.and_then(|c| bases.get(&c.group));
        let level = match s.chain {
            Some(c) if c.mode == ChainMode::Baseline => c.order as usize,
            _ => 0,
        };
        let new = match r {
            Ok(m) => layout_dimension_in(s, m, xf, level, base),
            Err(e) => layout_lost_in(s, dims.last_known.get(&s.id), xf, &e.to_string(), base),
        };
        match s.chain {
            Some(c) => {
                let seen = drawn.entry(c.group).or_default();
                for mk in new {
                    if matches!(mk, Mark::Line { .. }) && seen.iter().any(|o| same_line(o, &mk)) {
                        continue;
                    }
                    seen.push(mk.clone());
                    marks.push(mk);
                }
            }
            None => marks.extend(new),
        }
    }
    marks
}

/// Gleiche Linie (Art gleich, Enden bis auf 1e-9 gleich, Richtung egal)?
fn same_line(a: &Mark, b: &Mark) -> bool {
    const EPS: f64 = 1e-9;
    let near = |p: DVec2, q: DVec2| (p.x - q.x).abs() <= EPS && (p.y - q.y).abs() <= EPS;
    match (a, b) {
        (
            Mark::Line {
                a: p,
                b: q,
                kind: k,
            },
            Mark::Line {
                a: p2,
                b: q2,
                kind: k2,
            },
        ) => k == k2 && ((near(*p, *p2) && near(*q, *q2)) || (near(*p, *q2) && near(*q, *p2))),
        _ => false,
    }
}

fn kind_index(k: ViewKind) -> u8 {
    ViewKind::ALL.iter().position(|&x| x == k).unwrap_or(0) as u8
}
