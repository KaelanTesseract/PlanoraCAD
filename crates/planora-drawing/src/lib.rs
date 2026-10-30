//! Technische Zeichnungen (Teilprojekt 11): Ansichten exakter Körper auf einem Blatt.
//! Dieses Paket enthält das Datenmodell, die Ansichtsbasis, das Blattlayout und die Berechnung
//! der Linien einer Ansicht mit ihrer Sichtbarkeit.

pub mod annotate;
pub mod balloon;
pub mod basis;
pub mod centerline;
pub mod dimension;
pub mod dxf;
pub mod error;
pub mod file;
pub mod hatch;
pub mod isect;
pub mod layout;
pub mod limits;
pub mod model;
pub mod out2d;
pub mod parts;
pub mod pdf;
pub mod pdf_font;
pub mod raycast;
pub mod section;
pub mod section_marks;
pub mod sheet;
pub mod sheet_frame;
pub mod sheet_info;
pub mod sheet_marks;
pub mod silhouette;
pub mod stroke;
pub mod suggest;
pub mod svg;
pub mod visibility;

pub use basis::Basis;
pub use centerline::{CenterLine, CenterSource, centerlines};
pub use dimension::{Anchor, Measured, measure};
pub use dxf::{dxf_string, write_dxf};
pub use error::DrawError;
pub use file::{DrawingFormat, drawing_to_bytes, export_drawing_file, export_pdf_file};
pub use layout::{GAP_MM, align_dir, view_extent};
pub use model::{
    Balloon, BalloonId, Drawing, DrawingId, DrawingKind, HatchSpec, MAX_HATCH_SPACING,
    MAX_SECTIONS, MIN_HATCH_SHEET_MM, Paper, Placement, Projection, SECTION_LETTERS, Scale,
    SectionSpec, Sheet, ViewId, ViewKey, ViewKind, ViewName, ViewSpec, plane_offset_from_view,
    section_kind, section_line_coord, sections_ok_on,
};
pub use parts::{BodyKey, PartsCells, PartsRow, RowOverride};
pub use pdf::{PdfMeta, pdf_bytes, write_pdf};
pub use section::{SectionData, SectionLines, compute_section};
pub use sheet_frame::{
    CellLabel, PartsSpace, SheetLayout, SheetPen, TitleField, sheet_layout, sheet_layout_with,
};
pub use suggest::{Candidate, CandidateOrigin, dimension_key, suggest};
pub use svg::{svg_string, write_svg};

use limits::{MAX_PIECES, MAX_STROKES, geom_tol};
use planora_exchange::Progress;
use planora_i18n::tr;
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
    /// Mittellinien und Achsenkreuze der Ansicht (Ansichtskoordinaten, mm, ungeskaliert, ohne
    /// Überstand); immer berechnet, in `bbox` nicht enthalten.
    pub centerlines: Vec<CenterLine>,
    /// Teilstellen, die entfielen, weil sie ein zum Punkt entartetes Teilstück abgegrenzt
    /// hätten (Kennzahl für Prüfungen; sollte 0 sein).
    pub dropped_splits: usize,
}

/// Name eines Strichs für Meldungen.
fn stroke_name(bodies: &[DrawBody], s: &Stroke) -> String {
    match s.origin {
        StrokeOrigin::Edge { body, edge } => tr!(
            "drawing-stroke-edge",
            body = bodies[body].name.as_str(),
            n = (edge + 1).to_string()
        ),
        StrokeOrigin::Silhouette { body, face } => tr!(
            "drawing-stroke-silhouette",
            body = bodies[body].name.as_str(),
            n = (face + 1).to_string()
        ),
    }
}

fn origin_body(s: &Stroke) -> usize {
    match s.origin {
        StrokeOrigin::Edge { body, .. } | StrokeOrigin::Silhouette { body, .. } => body,
    }
}

/// Berechnet die Linien der Ansicht `kind`: Kanten und Umrisse, geteilt an allen Kreuzungen und
/// Anschlüssen, jedes Stück als sichtbar oder verdeckt beurteilt, dann zusammengefasst
/// (`visibility::merge_pieces`: anschließende Stücke gleicher Sichtbarkeit vereint,
/// deckungsgleiche Stücke einmal – sichtbar, wenn eines sichtbar ist –, winzige Stücke entfernt).
pub fn compute_view(
    bodies: &[DrawBody],
    kind: ViewKind,
    progress: &Progress,
) -> Result<ViewLines, DrawError> {
    let mut v = compute_view_unmerged(bodies, kind, progress)?;
    v.pieces = visibility::merge_pieces(
        &v.strokes,
        std::mem::take(&mut v.pieces),
        &v.basis,
        v.extent,
        progress,
    )?;
    v.bbox = pieces_bbox(&v.strokes, &v.pieces, &v.basis);
    Ok(v)
}

/// Wie [`compute_view`], aber ohne das Zusammenfassen: die Stücke so, wie Teilen und
/// Sichtprüfung sie liefern (lückenlos je Strich). Für Prüfungen der Einzelschritte.
pub fn compute_view_unmerged(
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
    // Erst sammeln (Speichergrenze MAX_RAW_STROKES), dann die zum Punkt entarteten verwerfen;
    // MAX_STROKES gilt für die verbleibenden.
    let mut strokes = stroke::edge_strokes(bodies, &basis, extent)?;
    progress.check()?;
    strokes.extend(silhouette::silhouette_strokes(
        bodies, &basis, extent, progress,
    )?);
    if strokes.len() as u64 > limits::MAX_RAW_STROKES {
        return Err(DrawError::TooComplex(strokes.len() as u64));
    }
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
    // Ein Paarzähler für Strichpaare und Durchstoßpaare (gemeinsame Grenze MAX_PAIR_TESTS).
    let pairs = Cell::new(0);
    let mut splits = isect::split_params_counted(&strokes, &basis, extent, &pairs, progress)?;

    // Durchstoßpunkte mit Flächen anderer Körper: Dort wechselt die Sichtbarkeit, ohne dass sich
    // im Bild Striche kreuzen (Körper, die ineinander stecken).
    let scene = raycast::Scene::new(bodies, extent, progress)?;
    if bodies.len() > 1 {
        for (s, ts) in strokes.iter().zip(splits.iter_mut()) {
            let extra = scene.pierce_params(s, origin_body(s), &pairs, progress)?;
            if !extra.is_empty() {
                ts.extend(extra);
                *ts = isect::tidy_splits(s, &basis, extent, std::mem::take(ts));
            }
        }
    }

    // Teilstücke zwischen aufeinanderfolgenden Teilparametern. Doppelte Teilstellen an den
    // Umkehrstellen von der Seite gesehener Kreise legt `isect::tidy_splits` zusammen. Bleibt
    // dennoch eine Teilstelle, die ein zum Punkt entartetes Teilstück abgrenzt (fast von der
    // Seite gesehene Kegelschnitte, kleine Halbachse knapp über der Einrastgrenze), entfällt sie
    // ohne Meldung und wird in `ViewLines::dropped_splits` gezählt.
    let mut ranges: Vec<(usize, f64, f64)> = Vec::new();
    let mut dropped_splits = 0;
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
            if point_like(params[params.len() - 1], t) {
                dropped_splits += 1;
            } else {
                params.push(t);
            }
        }
        if params.len() > 1 && point_like(params[params.len() - 1], hi) {
            params.pop();
            dropped_splits += 1;
        }
        params.push(hi);
        for w in params.windows(2) {
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
                if arc_len(s, a, b) <= limits::own_eps(extent) =>
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
        |i, a, b| arc_len(&strokes[i], a, b),
        limits::own_eps(extent),
    )?;

    let bbox = pieces_bbox(&strokes, &pieces, &basis);
    let centerlines = centerline::centerlines(bodies, &basis, extent, progress)?;
    Ok(ViewLines {
        centerlines,
        dropped_splits,
        kind,
        basis,
        extent,
        strokes,
        pieces,
        bbox,
    })
}

/// Abschnitte für die Bogenlänge eines Stücks im Raum.
const ARC_SAMPLES: usize = 16;

/// Bogenlänge des Stücks `[a, b]` im Raum: Summe der Sehnen über `ARC_SAMPLES` Abschnitte (von
/// unten an die Bogenlänge heran; bei kurzen Stücken bis auf Rundung genau). Eine einzelne Sehne
/// wäre bei einem fast geschlossenen Bogen beliebig kurz.
fn arc_len(s: &Stroke, a: f64, b: f64) -> f64 {
    let mut prev = s.point(a);
    let mut total = 0.0;
    for k in 1..=ARC_SAMPLES {
        let q = s.point(a + (b - a) * k as f64 / ARC_SAMPLES as f64);
        total += q.distance(prev);
        prev = q;
    }
    total
}

/// Stücke ohne eigenes Urteil (`None`) übernehmen das Urteil des vorigen, sonst des nächsten
/// anschließenden Stücks desselben Strichs (die Stücke eines Strichs folgen lückenlos
/// aufeinander). Eine zusammenhängende Folge solcher Stücke darf im Raum zusammen höchstens
/// `limit` (OWN_EPS) lang sein (`len`: Bogenlänge eines Stücks, [`arc_len`]); längere Folgen und
/// Stücke ohne Nachbarn mit Urteil: `AmbiguousVisibility` mit dem Namen des Strichs.
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

#[cfg(test)]
mod tests {
    use super::*;
    use planora_kernel::Curve;
    use planora_math::{DVec3, Frame};

    fn circle_stroke() -> Stroke {
        Stroke {
            curve: Curve::Circle {
                frame: Frame::WORLD,
                radius: 1.0,
            },
            t0: 0.0,
            t1: std::f64::consts::TAU,
            origin: StrokeOrigin::Edge { body: 0, edge: 0 },
        }
    }

    #[test]
    fn arc_length_is_measured_along_the_curve() {
        // Fast geschlossener Bogen (Radius 1, 2π − 1e-3): Sehne 1e-3, Bogen ≈ 2π.
        let s = circle_stroke();
        let b = std::f64::consts::TAU - 1e-3;
        assert!(s.point(0.0).distance(s.point(b)) < 1.01e-3);
        let l = arc_len(&s, 0.0, b);
        assert!((l - b).abs() < 0.03 * b, "{l}");
        // Gerade: genau.
        let line = Stroke {
            curve: Curve::Line {
                origin: DVec3::ZERO,
                dir: DVec3::X,
            },
            t0: 0.0,
            t1: 3.0,
            ..s
        };
        assert!((arc_len(&line, 0.5, 2.5) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_short_piece_prefers_the_verdict_of_its_predecessor() {
        use Visibility::{Hidden, Visible};
        let len = |_: usize, a: f64, b: f64| (b - a).abs();
        let name = |i: usize| format!("S{i}");
        // Vorgänger sichtbar, Nachfolger verdeckt: das kurze Stück wird sichtbar.
        let raw = vec![
            (0, 0.0, 1.0, Some(Visible)),
            (0, 1.0, 1.1, None),
            (0, 1.1, 2.0, Some(Hidden)),
        ];
        let p = inherit_short(raw, name, len, 0.5).unwrap();
        assert_eq!(p[1].vis, Visible);
        // Ohne Vorgänger: der Nachfolger.
        let raw = vec![(0, 0.0, 0.1, None), (0, 0.1, 2.0, Some(Hidden))];
        let p = inherit_short(raw, name, len, 0.5).unwrap();
        assert_eq!(p[0].vis, Hidden);
        // Ein anderer Strich davor zählt nicht als Vorgänger.
        let raw = vec![
            (0, 0.0, 1.0, Some(Visible)),
            (1, 1.0, 1.1, None),
            (1, 1.1, 2.0, Some(Hidden)),
        ];
        let p = inherit_short(raw, name, len, 0.5).unwrap();
        assert_eq!(p[1].vis, Hidden);
        // Zu lang (Bogen statt Sehne gemessen): benannte Meldung.
        let raw = vec![(0, 0.0, 1.0, Some(Visible)), (0, 1.0, 2.0, None)];
        assert!(matches!(
            inherit_short(raw, name, len, 0.5),
            Err(DrawError::AmbiguousVisibility(n)) if n == "S0"
        ));
    }
}
