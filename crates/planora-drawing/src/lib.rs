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
    let mut splits = isect::split_params(&strokes, &basis, extent, progress)?;

    // Durchstoßpunkte mit Flächen anderer Körper: Dort wechselt die Sichtbarkeit, ohne dass sich
    // im Bild Striche kreuzen (Körper, die ineinander stecken).
    let scene = raycast::Scene::new(bodies, extent, progress)?;
    if bodies.len() > 1 {
        let pairs = Cell::new(0);
        for (s, ts) in strokes.iter().zip(splits.iter_mut()) {
            let extra = scene.pierce_params(s, origin_body(s), &pairs, progress)?;
            if !extra.is_empty() {
                ts.extend(extra);
                *ts = isect::tidy_splits(s, &basis, extent, std::mem::take(ts));
            }
        }
    }

    // Teilstücke zwischen aufeinanderfolgenden Teilparametern. Doppelte Teilstellen an den
    // Umkehrstellen von der Seite gesehener Kreise legt `isect::tidy_splits` zusammen; ein zum
    // Punkt entartetes Teilstück wäre ein Fehler dort (verbliebene winzige Stücke entfernt
    // ohnehin das Zusammenfassen).
    let mut ranges: Vec<(usize, f64, f64)> = Vec::new();
    for (i, (s, ts)) in strokes.iter().zip(&splits).enumerate() {
        progress.check()?;
        let (lo, hi) = (s.t0.min(s.t1), s.t0.max(s.t1));
        let mut params = vec![lo];
        params.extend(ts.iter().copied());
        params.push(hi);
        for w in params.windows(2) {
            debug_assert!(
                !Stroke {
                    t0: w[0],
                    t1: w[1],
                    ..s.clone()
                }
                .is_degenerate_2d(&basis, degenerate),
                "Teilstück [{}, {}] von {:?} ist zum Punkt entartet",
                w[0],
                w[1],
                s.origin
            );
            ranges.push((i, w[0], w[1]));
        }
        if ranges.len() as u64 > MAX_PIECES {
            return Err(DrawError::TooComplex(ranges.len() as u64));
        }
    }

    let edge_faces: Vec<EdgeFaces> = bodies.iter().map(|b| EdgeFaces::new(b.solid)).collect();
    let counter = Cell::new(0);
    progress.set_total(ranges.len() as u64);
    let mut raw: Vec<(usize, f64, f64, Option<Visibility>)> = Vec::with_capacity(ranges.len());
    for (i, a, b) in ranges {
        progress.check()?;
        let s = &strokes[i];
        let body = origin_body(s);
        let own = edge_faces[body].own_faces(s);
        let name = stroke_name(bodies, s);
        // Angepasste Umriss-Splines: Prüfstrahlen vom Punkt des wahren Umrisses.
        let outline = match (&s.origin, &s.curve) {
            (StrokeOrigin::Silhouette { body, face }, planora_kernel::Curve::Nurbs(_)) => bodies
                [*body]
                .solid
                .faces
                .iter()
                .find(|(id, _)| id.index() == *face)
                .map(|(_, f)| f.surface.clone()),
            _ => None,
        };
        let map = |p: planora_math::DVec3| match &outline {
            Some(surface) => silhouette::outline_point(surface, basis.d, p),
            None => p,
        };
        match visibility::classify_range_mapped(
            &scene,
            &basis,
            s,
            &own,
            body,
            (a, b),
            extent,
            &name,
            &counter,
            &map,
        ) {
            Ok(parts) => {
                for (t0, t1, vis) in parts {
                    raw.push((i, t0, t1, Some(vis)));
                }
            }
            // Ein Stück, das im Raum kürzer als OWN_EPS ist und an dem kein Strahl entscheidet
            // (Ecke, an der ein Umriss auf einer Kante endet): Es übernimmt das Urteil des
            // Nachbarstücks desselben Strichs – der Wechsel ist dann auf OWN_EPS genau, wie
            // beim Eingrenzen in `classify_range`.
            Err(DrawError::AmbiguousVisibility(_))
                if s.point(a).distance(s.point(b)) <= limits::own_eps(extent) =>
            {
                raw.push((i, a, b, None));
            }
            Err(e) => return Err(e),
        }
        if raw.len() as u64 > MAX_PIECES {
            return Err(DrawError::TooComplex(raw.len() as u64));
        }
        progress.advance(1);
    }
    let pieces = inherit_short(
        raw,
        |i| stroke_name(bodies, &strokes[i]),
        |i, a, b| strokes[i].point(a).distance(strokes[i].point(b)),
        limits::own_eps(extent),
    )?;

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

/// Stücke ohne eigenes Urteil (`None`) übernehmen das Urteil des vorigen, sonst des nächsten
/// anschließenden Stücks desselben Strichs (die Stücke eines Strichs folgen lückenlos
/// aufeinander). Eine zusammenhängende Folge solcher Stücke darf im Raum zusammen höchstens
/// `limit` (OWN_EPS) lang sein (`len`: Sehnenlänge eines Stücks); längere Folgen und Stücke ohne
/// Nachbarn mit Urteil: `AmbiguousVisibility` mit dem Namen des Strichs.
fn inherit_short(
    mut raw: Vec<(usize, f64, f64, Option<Visibility>)>,
    name: impl Fn(usize) -> String,
    len: impl Fn(usize, f64, f64) -> f64,
    limit: f64,
) -> Result<Vec<Piece>, DrawError> {
    let joined = |a: &(usize, f64, f64, Option<Visibility>),
                  b: &(usize, f64, f64, Option<Visibility>)| {
        a.0 == b.0 && a.2 == b.1
    };
    let mut k = 0;
    while k < raw.len() {
        if raw[k].3.is_some() {
            k += 1;
            continue;
        }
        // Folge [k, m) ohne Urteil, lückenlos auf demselben Strich.
        let mut m = k + 1;
        while m < raw.len() && raw[m].3.is_none() && joined(&raw[m - 1], &raw[m]) {
            m += 1;
        }
        let total: f64 = raw[k..m].iter().map(|r| len(r.0, r.1, r.2)).sum();
        let before = (k > 0 && joined(&raw[k - 1], &raw[k]))
            .then(|| raw[k - 1].3)
            .flatten();
        let after = (m < raw.len() && joined(&raw[m - 1], &raw[m]))
            .then(|| raw[m].3)
            .flatten();
        match before.or(after) {
            Some(vis) if total <= limit => {
                for r in &mut raw[k..m] {
                    r.3 = Some(vis);
                }
            }
            _ => return Err(DrawError::AmbiguousVisibility(name(raw[k].0))),
        }
        k = m;
    }
    raw.into_iter()
        .map(|(stroke, t0, t1, vis)| match vis {
            Some(vis) => Ok(Piece {
                stroke,
                t0,
                t1,
                vis,
            }),
            None => Err(DrawError::AmbiguousVisibility(name(stroke))),
        })
        .collect()
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
