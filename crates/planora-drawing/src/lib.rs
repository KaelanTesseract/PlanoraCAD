//! Technische Zeichnungen (Teilprojekt 11): Ansichten exakter Körper auf einem Blatt.
//! Dieses Paket enthält das Datenmodell, die Ansichtsbasis, das Blattlayout und die Berechnung
//! der Linien einer Ansicht mit ihrer Sichtbarkeit.

pub mod basis;
pub mod error;
pub mod isect;
pub mod layout;
pub mod limits;
pub mod model;
pub mod raycast;
pub mod silhouette;
pub mod stroke;
pub mod visibility;

pub use basis::Basis;
pub use error::DrawError;
pub use layout::{GAP_MM, align_dir, view_extent};
pub use model::{
    Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind, ViewSpec,
};

use limits::{MAX_PIECES, MAX_STROKES, geom_tol};
use planora_exchange::Progress;
use std::cell::Cell;
use stroke::{EdgeFaces, Stroke, StrokeOrigin};

/// Ein Körper der Zeichnung: Name (für Meldungen) und der exakte Körper.
#[derive(Clone, Debug)]
pub struct DrawBody<'a> {
    pub name: String,
    pub solid: &'a planora_kernel::Solid,
}

/// Sichtbarkeit eines Linienstücks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    Hidden,
}

/// Ein Stück `[t0, t1]` des Strichs `stroke` (Index in `ViewLines::strokes`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub stroke: usize,
    pub t0: f64,
    pub t1: f64,
    pub vis: Visibility,
}

/// Linien einer Ansicht: Striche (ohne die zu Punkten entarteten) und ihre Stücke.
#[derive(Clone, Debug)]
pub struct ViewLines {
    pub kind: ViewKind,
    pub basis: Basis,
    pub extent: f64,
    pub strokes: Vec<Stroke>,
    pub pieces: Vec<Piece>,
    /// Hüllbox aller Stücke (sichtbar und verdeckt) in Ansichtskoordinaten, mm, ungeskaliert.
    pub bbox: ((f64, f64), (f64, f64)),
}

/// Name eines Strichs für Meldungen.
fn stroke_name(bodies: &[DrawBody], s: &Stroke) -> String {
    match s.origin {
        StrokeOrigin::Edge { body, edge } => {
            format!("{}, Kante {}", bodies[body].name, edge + 1)
        }
        StrokeOrigin::Silhouette { body, face } => {
            format!("{}, Umriss von Fläche {}", bodies[body].name, face + 1)
        }
    }
}

fn origin_body(s: &Stroke) -> usize {
    match s.origin {
        StrokeOrigin::Edge { body, .. } | StrokeOrigin::Silhouette { body, .. } => body,
    }
}

/// Berechnet die Linien der Ansicht `kind`: Kanten und Umrisse, geteilt an allen Kreuzungen und
/// Anschlüssen, jedes Stück als sichtbar oder verdeckt beurteilt.
pub fn compute_view(
    bodies: &[DrawBody],
    kind: ViewKind,
    progress: &Progress,
) -> Result<ViewLines, DrawError> {
    if bodies.is_empty() {
        return Err(DrawError::NoBodies);
    }
    progress.check()?;
    let basis = Basis::of(kind);
    let extent = stroke::extent_of(bodies);
    if !extent.is_finite() {
        return Err(DrawError::NonFinite);
    }
    let mut strokes = stroke::edge_strokes(bodies, &basis, extent)?;
    progress.check()?;
    strokes.extend(silhouette::silhouette_strokes(
        bodies, &basis, extent, progress,
    )?);
    let degenerate = 16.0 * geom_tol(extent);
    let mut kept = Vec::with_capacity(strokes.len());
    for s in strokes {
        progress.check()?;
        if !s.is_degenerate_2d(&basis, degenerate) {
            kept.push(s);
        }
    }
    let strokes = kept;
    if strokes.len() as u64 > MAX_STROKES {
        return Err(DrawError::TooComplex(strokes.len() as u64));
    }
    let splits = isect::split_params(&strokes, &basis, extent, progress)?;

    // Teilstücke zwischen aufeinanderfolgenden Teilparametern. Teilparameter, die ein in der
    // Ansicht zu einem Punkt entartetes Stück abgrenzen (etwa doppelt gefundene Anschlüsse an
    // der Umkehrstelle eines von der Seite gesehenen Kreises), entfallen wie entartete Striche.
    let mut ranges: Vec<(usize, f64, f64)> = Vec::new();
    for (i, (s, ts)) in strokes.iter().zip(&splits).enumerate() {
        progress.check()?;
        let (lo, hi) = (s.t0.min(s.t1), s.t0.max(s.t1));
        let point_like = |a: f64, b: f64| {
            Stroke {
                t0: a,
                t1: b,
                ..s.clone()
            }
            .is_degenerate_2d(&basis, degenerate)
        };
        let mut params = vec![lo];
        for &t in ts {
            if !point_like(params[params.len() - 1], t) {
                params.push(t);
            }
        }
        if params.len() > 1 && point_like(params[params.len() - 1], hi) {
            params.pop();
        }
        params.push(hi);
        for w in params.windows(2) {
            ranges.push((i, w[0], w[1]));
        }
        if ranges.len() as u64 > MAX_PIECES {
            return Err(DrawError::TooComplex(ranges.len() as u64));
        }
    }

    let scene = raycast::Scene::new(bodies, extent, progress)?;
    let edge_faces: Vec<EdgeFaces> = bodies.iter().map(|b| EdgeFaces::new(b.solid)).collect();
    let counter = Cell::new(0);
    progress.set_total(ranges.len() as u64);
    let mut pieces = Vec::with_capacity(ranges.len());
    for (i, a, b) in ranges {
        progress.check()?;
        let s = &strokes[i];
        let body = origin_body(s);
        let own = edge_faces[body].own_faces(s);
        let name = stroke_name(bodies, s);
        for (t0, t1, vis) in visibility::classify_range(
            &scene, &basis, s, &own, body, a, b, extent, &name, &counter,
        )? {
            pieces.push(Piece {
                stroke: i,
                t0,
                t1,
                vis,
            });
        }
        if pieces.len() as u64 > MAX_PIECES {
            return Err(DrawError::TooComplex(pieces.len() as u64));
        }
        progress.advance(1);
    }

    let bbox = pieces_bbox(&strokes, &pieces, &basis);
    Ok(ViewLines {
        kind,
        basis,
        extent,
        strokes,
        pieces,
        bbox,
    })
}

/// Hüllbox aller Stücke in Ansichtskoordinaten (ohne Rand).
fn pieces_bbox(strokes: &[Stroke], pieces: &[Piece], basis: &Basis) -> ((f64, f64), (f64, f64)) {
    let mut lo = planora_math::DVec2::splat(f64::INFINITY);
    let mut hi = planora_math::DVec2::splat(f64::NEG_INFINITY);
    for p in pieces {
        let part = Stroke {
            t0: p.t0,
            t1: p.t1,
            ..strokes[p.stroke].clone()
        };
        let (a, b) = part.bbox2(basis, 0.0);
        lo = lo.min(a);
        hi = hi.max(b);
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return ((0.0, 0.0), (0.0, 0.0));
    }
    ((lo.x, lo.y), (hi.x, hi.y))
}
