//! Herkunft aus Skizzen 10 (Spec 5 und 10): eingezogene 3D-Geometrie (Kanten, Flächenränder,
//! Ecken, Ebenen, Achsen, Ursprung, Kurven früherer Skizzen) und Kurven früherer Skizzen als
//! Quelle von Projizieren und Schneiden.
//!
//! Eine Quelle kann mehrere Stücke ergeben (Flächenrand, Ebene, Schnittpunkte). Ihre Kopien
//! bilden eine Gruppe mit derselben Herkunft; die Neuberechnung rechnet die Gruppe zusammen
//! neu. Ergibt sie eine andere Stückzahl, bleibt alles stehen, und die Skizze warnt „Bezug passt
//! nicht mehr“ (siehe `compute_sketch`).
//!
//! Die Rechnung ist rein: Sie liest die sichtbaren Körper, Konstruktionsebenen und die schon
//! berechneten Skizzen vor der Skizze im Verlauf.

use crate::model::{
    CopySource, Document, FeatureKind, IncludeRef, PlaneId, VertexRef, include_plane_half,
};
use crate::planes::StdAxis;
use crate::refs::{EdgeRef, FaceRef, resolve_edge, resolve_face};
use planora_kernel::boolean::{CurveSurface, curve_surface_roots};
use planora_kernel::{Curve, Solid, Surface};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::Sketch;
use planora_sketch::helpers::{Included, include_curve};

/// Ein neu gerechnetes Stück einer Quelle.
#[derive(Clone, Debug)]
pub(crate) enum Piece {
    /// Einbezogene Geometrie in Skizzenkoordinaten (Kurve oder Punkt).
    Include(Included),
    /// Kurvenstück im Raum, das wie eine Kante in die Skizzenebene projiziert wird.
    Project(Curve, f64, f64),
}

/// Was die Rechnung liest: das Dokument, die Stelle der Skizze im Verlauf, die sichtbaren
/// Körper davor, die Skizzenebene und die Toleranz.
pub(crate) struct IncludeCtx<'a> {
    pub(crate) doc: &'a Document,
    pub(crate) idx: usize,
    pub(crate) solids: &'a [&'a Solid],
    pub(crate) frame: &'a Frame,
    pub(crate) tol: &'a Tolerance,
}

/// Die einbezogenen Stücke des Verweises `r` für eine Skizze mit der Ebene `frame` an der Stelle
/// `at` des Verlaufs (sichtbare Körper und Skizzen davor), in Skizzenkoordinaten und in der
/// Reihenfolge der Stücke ([`crate::EdgeCopy::piece`]). Grundlage für das Werkzeug
/// „3D-Geometrie einbeziehen“ (Task 16): dieselbe Rechnung wie die Neuberechnung. `None`, wenn
/// sich der Verweis nicht auflösen lässt.
pub fn included_pieces(
    doc: &Document,
    at: usize,
    frame: &Frame,
    r: &IncludeRef,
) -> Option<Vec<Included>> {
    let at = at.min(doc.features.len());
    let visible = doc.visible_at(at);
    let solids: Vec<&Solid> = visible.iter().filter_map(|b| doc.body(*b)).collect();
    let tol = doc.kernel.tolerance;
    let ctx = IncludeCtx {
        doc,
        idx: at,
        solids: &solids,
        frame,
        tol: &tol,
    };
    include_ref_pieces(&ctx, r).map(|(_, g)| g)
}

/// Die Schnittpunkte der Kurve `curve` der Skizze `sketch` mit der Ebene `frame` einer Skizze an
/// der Stelle `at` des Verlaufs, als Punkte in Skizzenkoordinaten und in der Reihenfolge der
/// Stücke: Grundlage für „Schneiden“ mit einer Skizzenkurve (Task 16), dieselbe Rechnung wie die
/// Neuberechnung (`CopySource::SketchCurve` mit Punkten als Zielen). `None`, wenn die Quelle
/// nicht vor `at` steht, die Kurve fehlt oder in der Ebene liegt.
pub fn sketch_curve_points(
    doc: &Document,
    at: usize,
    frame: &Frame,
    sketch: crate::FeatureId,
    curve: planora_sketch::model::CurveId,
) -> Option<Vec<DVec3>> {
    let at = at.min(doc.features.len());
    let tol = doc.kernel.tolerance;
    let ctx = IncludeCtx {
        doc,
        idx: at,
        solids: &[],
        frame,
        tol: &tol,
    };
    let (_, pieces) =
        source_pieces(&ctx, &CopySource::SketchCurve { sketch, curve }, true, None).ok()?;
    pieces
        .into_iter()
        .map(|p| match p {
            Piece::Include(Included::Point(q)) => Some(q),
            _ => None,
        })
        .collect()
}

/// Warum sich die Stücke einer Herkunft nicht neu rechnen lassen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unfollowed {
    /// Der Bezug ist verloren.
    Lost,
    /// Die Rechnung überschreitet die Zeitgrenze (Spec 10): Die Lage bleibt, die Skizze warnt.
    TooSlow,
}

/// Rechnet die Stücke der Herkunft `source` neu (nur die Arten aus Skizzen 10) und liefert die
/// Herkunft mit nachgeführten Hinweispunkten. `own` ist die Skizze selbst (für die eigene Achse
/// eines Drehprofils). `points` heißt: Die Ziele der Gruppe sind Punkte
/// (bei einer Skizzenkurve als Quelle: Schneiden statt Projizieren). „Auf Fläche projizieren“ und
/// „Schnittkurve“ rechnen mit der Zeitgrenze der Neuberechnung; trifft nichts mehr, sind es null
/// Stücke (also eine andere Stückzahl).
pub(crate) fn source_pieces(
    ctx: &IncludeCtx<'_>,
    source: &CopySource,
    points: bool,
    own: Option<&Sketch>,
) -> Result<(CopySource, Vec<Piece>), Unfollowed> {
    use crate::surface_project::{intersection_curve, project_to_surface, recompute_budget};
    let view = crate::surface_project::DocView::new(ctx.doc, ctx.idx, *ctx.frame);
    let lost = Unfollowed::Lost;
    match source {
        CopySource::Include(r) => {
            let (r, pieces) = include_ref_pieces(ctx, r).ok_or(lost)?;
            Ok((
                CopySource::Include(r),
                pieces.into_iter().map(Piece::Include).collect(),
            ))
        }
        CopySource::SketchCurve { sketch, curve } => {
            let sk = source_sketch(ctx, *sketch).ok_or(lost)?;
            let (c, t0, t1) = sk.curve_world(*curve).ok_or(lost)?;
            let pieces = if points {
                section_points(&c, t0, t1, ctx.frame, ctx.tol)
                    .ok_or(lost)?
                    .into_iter()
                    .map(Piece::Include)
                    .collect()
            } else {
                vec![Piece::Project(c, t0, t1)]
            };
            Ok((source.clone(), pieces))
        }
        // Projektion und Schnittkurve ergeben Kurven (und Punkte aus Ecken); die Art des Ziels
        // prüft `apply_piece`.
        CopySource::OnSurface(p) => surface_pieces(
            source,
            project_to_surface(&view, p, ctx.tol, recompute_budget()),
            || CopySource::OnSurface(p.with_hints(ctx.solids, ctx.tol)),
        ),
        CopySource::Intersection(p) => surface_pieces(
            source,
            intersection_curve(&view, p, ctx.tol, recompute_budget()),
            || CopySource::Intersection(p.with_hints(ctx.solids, ctx.tol)),
        ),
        // Drehprofil: Stücke neu wie bei „Auf Fläche projizieren“. Ergibt die geänderte Quelle
        // keinen Drehkörper mehr (oder liegt die Achse nicht mehr passend), ist das ein
        // verlorener Bezug (Warnung, die Kurven bleiben), kein Fehler.
        CopySource::SpunProfile(p) => {
            use crate::spun_profile::{SpunAxis, SpunError, spun_profile};
            let own_axis = match &p.axis {
                SpunAxis::Own(c) => Some(own_axis(own, *c).ok_or(lost)?),
                SpunAxis::Ref(_) => None,
            };
            match spun_profile(&view, p, own_axis, ctx.tol) {
                Ok(g) => Ok((source.clone(), g.into_iter().map(Piece::Include).collect())),
                Err(SpunError::Nothing) => Ok((source.clone(), Vec::new())),
                Err(SpunError::Lost | SpunError::NotRevolved | SpunError::AxisNotInPlane) => {
                    Err(lost)
                }
            }
        }
        // Die Isokurve rechnet erst ab Task 15; bis dahin bleibt die Lage (Warnung wie bei einem
        // verlorenen Bezug).
        CopySource::Isoparametric { .. } => Err(lost),
        CopySource::Edge(_) | CopySource::Section { .. } | CopySource::Silhouette { .. } => {
            Err(lost)
        }
    }
}

/// Punkt und Richtung (Welt) der Linie `c` der eigenen Skizze `own` als Achse eines Drehprofils;
/// `None`, wenn die Kurve fehlt (gelöscht) oder keine Linie ist (Prüfauftrag aus Task 7).
fn own_axis(own: Option<&Sketch>, c: planora_sketch::CurveId) -> Option<(DVec3, DVec3)> {
    let sk = own?;
    if !matches!(
        sk.curve(c)?.kind,
        planora_sketch::model::CurveKind::Line { .. }
    ) {
        return None;
    }
    match sk.curve_world(c)? {
        (Curve::Line { origin, dir }, _, _) => Some((origin, dir)),
        _ => None,
    }
}

/// Stücke aus „Auf Fläche projizieren“ oder „Schnittkurve“: bei Erfolg mit nachgeführten
/// Hinweispunkten (`refreshed`); trifft nichts mehr, null Stücke; zu lange gerechnet oder Bezug
/// verloren als Grund.
fn surface_pieces(
    source: &CopySource,
    got: Result<Vec<Included>, crate::surface_project::ProjectError>,
    refreshed: impl FnOnce() -> CopySource,
) -> Result<(CopySource, Vec<Piece>), Unfollowed> {
    use crate::surface_project::ProjectError;
    match got {
        Ok(g) => Ok((refreshed(), g.into_iter().map(Piece::Include).collect())),
        Err(ProjectError::NoHit) => Ok((source.clone(), Vec::new())),
        Err(ProjectError::TooSlow) => Err(Unfollowed::TooSlow),
        Err(ProjectError::Lost) => Err(Unfollowed::Lost),
    }
}

/// Die berechnete Skizze `id`, wenn sie vor der Skizze im Verlauf steht (Spec 10); sonst ist der
/// Bezug verloren.
fn source_sketch<'a>(ctx: &IncludeCtx<'a>, id: crate::FeatureId) -> Option<&'a Sketch> {
    let i = ctx.doc.index_of(id)?;
    if i >= ctx.idx || !matches!(ctx.doc.features[i].kind, FeatureKind::Sketch(_)) {
        return None;
    }
    ctx.doc.sketch_of(id)
}

/// Schnittpunkte der Kurve `c(t0 … t1)` mit der Skizzenebene, nach dem Parameter geordnet, als
/// Punkte in Skizzenkoordinaten. Liegt die Kurve in der Ebene, ist das kein Schnitt (`None`).
fn section_points(
    c: &Curve,
    t0: f64,
    t1: f64,
    frame: &Frame,
    tol: &Tolerance,
) -> Option<Vec<Included>> {
    let plane = Surface::Plane { frame: *frame };
    match curve_surface_roots(c, t0, t1, &plane, tol) {
        CurveSurface::Roots(roots) => {
            let mut ts: Vec<f64> = roots.iter().map(|r| r.t).collect();
            ts.sort_by(f64::total_cmp);
            Some(
                ts.into_iter()
                    .map(|t| {
                        let mut p = frame.to_local(c.point(t));
                        // Der Schnittpunkt liegt in der Ebene; Rundungsreste fallen weg.
                        p.z = 0.0;
                        Included::Point(p)
                    })
                    .collect(),
            )
        }
        CurveSurface::Lies => None,
    }
}

/// Die Stücke eines Einbeziehen-Verweises mit nachgeführten Hinweispunkten.
fn include_ref_pieces(ctx: &IncludeCtx<'_>, r: &IncludeRef) -> Option<(IncludeRef, Vec<Included>)> {
    let local = |p: DVec3| ctx.frame.to_local(p);
    match r {
        IncludeRef::Edge(e) => {
            let (e, solid, edge) = edge_of(ctx, e)?;
            let edge = &solid.edges[edge];
            let g = include_curve(ctx.frame, &edge.curve, edge.t0, edge.t1).ok()?;
            Some((IncludeRef::Edge(e), vec![g]))
        }
        IncludeRef::FaceBoundary(f) => {
            let found = resolve_face(ctx.solids, f, ctx.tol).ok()?;
            let solid = ctx.solids[found.body];
            let face = solid.faces.get(found.face)?;
            let mut seen = Vec::new();
            let mut pieces = Vec::new();
            for he in face
                .outer
                .half_edges
                .iter()
                .chain(face.inner.iter().flat_map(|l| l.half_edges.iter()))
            {
                // Eine Naht läuft zweimal durch die Schleife, sie zählt einmal.
                if seen.contains(&he.edge) {
                    continue;
                }
                seen.push(he.edge);
                let edge = solid.edges.get(he.edge)?;
                // Entartete Kanten (etwa an Polen) ergeben kein Stück.
                if let Ok(g) = include_curve(ctx.frame, &edge.curve, edge.t0, edge.t1) {
                    pieces.push(g);
                }
            }
            if pieces.is_empty() {
                return None;
            }
            let f = FaceRef {
                hint: found.hint,
                ..f.clone()
            };
            Some((IncludeRef::FaceBoundary(f), pieces))
        }
        IncludeRef::Vertex(v) => {
            let (edge_ref, solid, edge) = edge_of(ctx, &v.edge)?;
            let e = &solid.edges[edge];
            let p = solid
                .vertices
                .get(if v.end { e.end } else { e.start })?
                .point;
            Some((
                IncludeRef::Vertex(VertexRef {
                    edge: edge_ref,
                    end: v.end,
                }),
                vec![Included::Point(local(p))],
            ))
        }
        IncludeRef::Plane(id) => {
            let pf = match id {
                PlaneId::Standard(p) => p.frame(),
                PlaneId::Feature(f) => {
                    let i = ctx.doc.index_of(*f)?;
                    if i >= ctx.idx {
                        return None;
                    }
                    ctx.doc.plane_frame(*f)?
                }
            };
            let h = include_plane_half(ctx.solids);
            // Mitte nach derselben Regel wie die Anzeige: auf einer Fläche deren Mitte, sonst der
            // Fußpunkt des Ursprungs.
            let anchor = match id {
                PlaneId::Feature(f) => plane_anchor(ctx, *f),
                PlaneId::Standard(_) => None,
            };
            let c = crate::plane_rect::plane_rect_center(&pf, anchor);
            let corner = |sx: f64, sy: f64| local(c + pf.x * (sx * h) + pf.y * (sy * h));
            let ring = [
                corner(-1.0, -1.0),
                corner(1.0, -1.0),
                corner(1.0, 1.0),
                corner(-1.0, 1.0),
            ];
            let lines = (0..4)
                .map(|k| Included::Line(ring[k], ring[(k + 1) % 4]))
                .collect();
            Some((r.clone(), lines))
        }
        IncludeRef::Axis(a) => {
            let dir = match a {
                StdAxis::X => DVec3::X,
                StdAxis::Y => DVec3::Y,
                StdAxis::Z => DVec3::Z,
            };
            let h = include_plane_half(ctx.solids);
            Some((
                r.clone(),
                vec![Included::Line(local(-dir * h), local(dir * h))],
            ))
        }
        // Konstruktionspunkte als Features gibt es noch nicht (Platzhalter).
        IncludeRef::ConstructionPoint(_) => None,
        IncludeRef::Origin => Some((r.clone(), vec![Included::Point(local(DVec3::ZERO))])),
        IncludeRef::SketchCurve { sketch, curve } => {
            let sk = source_sketch(ctx, *sketch)?;
            let (c, t0, t1) = sk.curve_world(*curve)?;
            let g = include_curve(ctx.frame, &c, t0, t1).ok()?;
            Some((r.clone(), vec![g]))
        }
    }
}

/// Anker des Rechtecks der Konstruktionsebene `f` (siehe [`crate::plane_rect`]): die Mitte ihrer
/// Basisfläche unter den Körpern vor der Skizze; `None` ohne Basisfläche oder wenn sie sich nicht
/// auflösen lässt.
fn plane_anchor(ctx: &IncludeCtx<'_>, f: crate::FeatureId) -> Option<DVec3> {
    let FeatureKind::Plane(pf) = &ctx.doc.feature(f)?.kind else {
        return None;
    };
    let r = crate::plane_rect::plane_base_face(&pf.def)?;
    let found = resolve_face(ctx.solids, r, ctx.tol).ok()?;
    let pts = crate::plane_rect::solid_face_points(ctx.solids[found.body], found.face)?;
    crate::plane_rect::outline_anchor(&pts)
}

/// Löst die Kante `e` auf: Verweis mit neuem Hinweispunkt, Körper und Kante.
fn edge_of<'a>(
    ctx: &IncludeCtx<'a>,
    e: &EdgeRef,
) -> Option<(EdgeRef, &'a Solid, planora_kernel::Id<planora_kernel::Edge>)> {
    let found = resolve_edge(ctx.solids, e, ctx.tol).ok()?;
    let solid = ctx.solids[found.body];
    solid.edges.get(found.edge)?;
    Some((
        EdgeRef {
            hint: found.hint,
            ..e.clone()
        },
        solid,
        found.edge,
    ))
}
