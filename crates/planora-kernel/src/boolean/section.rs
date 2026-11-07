//! Ebenenschnitt eines Körpers: die Schnittkurven einer Ebene mit den Flächen eines Körpers,
//! beschnitten auf das Innere der Flächen (Grundlage für „Schneiden“ in der Skizze).

use super::breaks::{bbox, breakpoints, classify_seg, edge_hits, split_at};
use super::seg::Seg;
use super::split::seam_segments;
use super::surfsurf::{SurfaceIntersection, intersect_surfaces, surface_label, surface_name};
use super::uv::{Containment, face_geom, point_in_face};
use super::{face_is_freeform, pipeline_tol};
use crate::geometry::{Curve, Surface};
use crate::ids::Id;
use crate::topology::{Face, Solid};
use planora_i18n::tr;
use planora_math::{DVec3, Frame, Tolerance};
use std::fmt;

/// Ein Stück der Schnittkurve im Inneren einer Fläche: `curve` auf `[t0, t1]` (auch absteigend).
#[derive(Clone, Debug)]
pub struct SectionPiece {
    pub face: Id<Face>,
    pub curve: Curve,
    pub t0: f64,
    pub t1: f64,
}

/// Warum eine Fläche nicht mit der Ebene geschnitten werden kann.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectionError {
    /// Analytische Fläche in einer Lage ohne Rechenweg (etwa schräger Kegel, Torus allgemein);
    /// `surface` ist die Kennung aus `surface_name`, nicht der Anzeigename.
    Unsupported {
        face: Id<Face>,
        surface: &'static str,
    },
    /// Freiformflächen werden (noch) nicht geschnitten.
    Freeform(Id<Face>),
}

impl fmt::Display for SectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            SectionError::Unsupported { surface, .. } => tr!(
                "kernel-section-unsupported",
                surface = surface_label(surface)
            ),
            SectionError::Freeform(_) => tr!("kernel-section-freeform"),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for SectionError {}

/// Dieselbe Punktfolge (Anfang, Mitte und Ende innerhalb `tol.linear`, Richtung egal)?
fn same_pointwise(a: &Seg, b: &Seg, tol: &Tolerance) -> bool {
    let near = |p: DVec3, q: DVec3| p.distance(q) <= tol.linear;
    let ends = (near(a.start(), b.start()) && near(a.end(), b.end()))
        || (near(a.start(), b.end()) && near(a.end(), b.start()));
    ends && near(a.at(0.5), b.at(0.5))
}

/// Schneidet die Ebene den Hüllquader `(lo, hi)`? (Abstände der Ecken mit beiden Vorzeichen,
/// innerhalb der Toleranz.)
fn box_meets_plane((lo, hi): (DVec3, DVec3), plane: &Frame, tol: &Tolerance) -> bool {
    let (mut min, mut max) = (f64::MAX, f64::MIN);
    for i in 0..8 {
        let c = DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let d = plane.z.dot(c - plane.origin);
        min = min.min(d);
        max = max.max(d);
    }
    min <= tol.linear && max >= -tol.linear
}

/// Abstand zur Ebene eines Punkts der Fläche `face` dicht neben `p` auf ihrer Randkante (Tangente
/// `t`), um `eps` ins Flächeninnere versetzt. `None`, wenn kein solcher Punkt im Inneren liegt.
fn side_near(
    solid: &Solid,
    face: Id<Face>,
    p: DVec3,
    t: DVec3,
    eps: f64,
    plane: &Frame,
    tol: &Tolerance,
) -> Option<f64> {
    let geom = face_geom(solid, face);
    let (u, v) = geom.surface.project(p);
    let d = geom.surface.normal(u, v).cross(t).normalize_or_zero();
    [1.0, -1.0].into_iter().find_map(|sgn| {
        let (qu, qv) = geom.surface.project(p + d * (eps * sgn));
        let q = geom.surface.point(qu, qv);
        (point_in_face(&geom, q, tol) == Containment::Inside).then(|| plane.z.dot(q - plane.origin))
    })
}

/// Ein Stück auf dem Rand der Fläche `face` gehört zum Schnitt, wenn es auf einer echten Kante
/// liegt, deren beide Nachbarflächen auf verschiedenen Seiten der Ebene liegen (die Ebene geht
/// durch den Körper). Berührt die Ebene den Körper nur an der Kante, entfällt es.
fn edge_piece_crosses(
    solid: &Solid,
    face: Id<Face>,
    s: &Seg,
    plane: &Frame,
    tol: &Tolerance,
) -> bool {
    let mid = s.at(0.5);
    let f = &solid.faces[face];
    let own = || {
        std::iter::once(&f.outer)
            .chain(f.inner.iter())
            .flat_map(|l| &l.half_edges)
    };
    let Some(edge) = own().map(|he| he.edge).find(|&e| {
        let e = &solid.edges[e];
        Seg::new(e.curve.clone(), e.t0, e.t1).closest(mid).1 <= tol.linear
    }) else {
        return false;
    };
    // Naht: dieselbe Kante zweimal in derselben Fläche, keine Nachbarfläche.
    if own().filter(|he| he.edge == edge).count() > 1 {
        return false;
    }
    let Some(other) = solid.faces.iter().map(|(id, _)| id).find(|&o| {
        o != face && {
            let g = &solid.faces[o];
            std::iter::once(&g.outer)
                .chain(g.inner.iter())
                .flat_map(|l| &l.half_edges)
                .any(|he| he.edge == edge)
        }
    }) else {
        return false;
    };
    let t = s.tangent_at(0.5);
    let eps = (s.length() * 1e-3).max(tol.linear * 100.0);
    match (
        side_near(solid, face, mid, t, eps, plane, tol),
        side_near(solid, other, mid, t, eps, plane, tol),
    ) {
        (Some(a), Some(b)) => a.abs() > tol.linear && b.abs() > tol.linear && a * b < 0.0,
        _ => false,
    }
}

/// Schnitt der Ebene `plane` mit den Flächen des Körpers (allen oder nur `faces`; Kennungen, die
/// nicht zum Körper gehören, werden übergangen), beschnitten auf das Innere jeder Fläche.
///
/// Die Stücke folgen der Reihenfolge der Flächen im Körper und je Fläche dem Kurvenparameter.
/// Eine Fläche, die ganz in der Ebene liegt, liefert ihre Randkanten. Berührt die Ebene eine
/// Fläche nur (Berührlinie am Zylinder, Berührpunkt an der Kugel), entsteht nichts
/// (**Annahme**: Fusion liefert die Berührlinie nicht). Stücke, die als Punktfolge gleich sind
/// (etwa eine Kante, die zwei Flächen liefern), kommen nur einmal vor; es bleibt das der ersten
/// Fläche. Freiformflächen ergeben [`SectionError::Freeform`], analytische Lagen ohne Rechenweg
/// [`SectionError::Unsupported`].
pub fn section_by_plane(
    solid: &Solid,
    plane: &Frame,
    faces: Option<&[Id<Face>]>,
    tol: &Tolerance,
) -> Result<Vec<SectionPiece>, SectionError> {
    let cut = Surface::Plane { frame: *plane };
    let mut out: Vec<(SectionPiece, Seg)> = Vec::new();
    for (id, _) in solid.faces.iter() {
        if faces.is_some_and(|fs| !fs.contains(&id)) {
            continue;
        }
        let geom = face_geom(solid, id);
        // Freiformflächen nur ablehnen, wenn die Ebene ihren Hüllquader trifft; sonst trägt die
        // Fläche ohnehin nichts bei (etwa eine Rundung weit weg).
        if matches!(geom.surface, Surface::Nurbs(_)) {
            if box_meets_plane(bbox(&geom, tol), plane, tol) {
                return Err(SectionError::Freeform(id));
            }
            continue;
        }
        // Randkanten aus Freiformkurven: Haltepunkte und Lage mit der weiteren Toleranz.
        let ptol = pipeline_tol(tol, face_is_freeform(&geom));
        let mut found: Vec<Seg> = Vec::new();
        match intersect_surfaces(&cut, &geom.surface, tol) {
            SurfaceIntersection::None | SurfaceIntersection::Touch => {}
            SurfaceIntersection::Unsupported => {
                return Err(SectionError::Unsupported {
                    face: id,
                    surface: surface_name(&geom.surface),
                });
            }
            // Nur mit Freiformflächen (oben schon abgewiesen); zur Sicherheit.
            SurfaceIntersection::UnsupportedBecause(_) => return Err(SectionError::Freeform(id)),
            SurfaceIntersection::Coincident => {
                found.extend(
                    geom.loops
                        .iter()
                        .flatten()
                        .filter(|e| e.length() > ptol.linear)
                        .cloned(),
                );
            }
            SurfaceIntersection::Curves(cs) => {
                let boundary: Vec<Seg> = geom.loops.iter().flatten().cloned().collect();
                let seams = seam_segments(&geom, &boundary, &ptol);
                let hits: Vec<_> = boundary
                    .iter()
                    .flat_map(|e| edge_hits(e, &cut, &ptol))
                    .collect();
                for c in cs {
                    let ts = breakpoints(&c, &hits, &[&geom.surface], &ptol);
                    found.extend(split_at(&c, ts, &ptol).into_iter().filter(|s| {
                        match classify_seg(&geom, &seams, s, &ptol) {
                            Containment::Inside => true,
                            // Stück auf einer Kante: nur, wenn die Ebene dort durchgeht.
                            Containment::Boundary => edge_piece_crosses(solid, id, s, plane, &ptol),
                            Containment::Outside => false,
                        }
                    }));
                }
            }
        }
        for s in found {
            if !out.iter().any(|(_, o)| same_pointwise(o, &s, &ptol)) {
                let piece = SectionPiece {
                    face: id,
                    curve: s.curve.clone(),
                    t0: s.t0,
                    t1: s.t1,
                };
                out.push((piece, s));
            }
        }
    }
    Ok(out.into_iter().map(|(p, _)| p).collect())
}

/// Ungekürzte Umrisskurven der Trägerfläche `surface` in Blickrichtung `view` (Einheitsvektor)
/// samt einer Ebene, die sie alle enthält (für die Treffer der Randkanten). `Ok(None)`: kein
/// Umriss (Ebene; Zylinder und Kegel mit Achse in Blickrichtung). `Err(())`: Lage ohne Rechenweg.
fn silhouette_curves(
    surface: &Surface,
    view: DVec3,
    tol: &Tolerance,
) -> Result<Option<(Vec<Curve>, Frame)>, ()> {
    let along_axis = |axis: DVec3| axis.cross(view).length() <= tol.angular;
    let across_axis = |axis: DVec3| axis.dot(view).abs() <= tol.angular;
    match surface {
        Surface::Plane { .. } => Ok(None),
        Surface::Cylinder { frame, radius } => {
            if along_axis(frame.z) {
                return Ok(None);
            }
            if !across_axis(frame.z) {
                return Err(());
            }
            // Die beiden Mantellinien bei ±90° zur Blickrichtung liegen in der Ebene durch die
            // Achse mit der Normalen `view`.
            let d = frame.z.cross(view).normalize();
            let lines = [d, -d]
                .into_iter()
                .map(|s| Curve::Line {
                    origin: frame.origin + s * *radius,
                    dir: frame.z,
                })
                .collect();
            Ok(Some((lines, Frame::from_origin_normal(frame.origin, view))))
        }
        Surface::Cone {
            frame,
            radius,
            slope,
        } => {
            if along_axis(frame.z) {
                return Ok(None);
            }
            if !across_axis(frame.z) {
                return Err(());
            }
            // Mantellinien `origin + s·(radius + slope·v) + z·v` in der Ebene durch die Achse mit
            // der Normalen `view`; sie laufen durch die Spitze (dort teilt `breakpoints`).
            let d = frame.z.cross(view).normalize();
            let lines = [d, -d]
                .into_iter()
                .map(|s| Curve::Line {
                    origin: frame.origin + s * *radius,
                    dir: (frame.z + s * *slope).normalize(),
                })
                .collect();
            Ok(Some((lines, Frame::from_origin_normal(frame.origin, view))))
        }
        Surface::Sphere { frame, radius } => {
            // Großkreis senkrecht zur Blickrichtung.
            let plane = Frame::from_origin_normal(frame.origin, view);
            let circle = Curve::Circle {
                frame: plane,
                radius: *radius,
            };
            Ok(Some((vec![circle], plane)))
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            if !along_axis(frame.z) {
                return Err(());
            }
            // Außen- und Innenkreis in der Mittelebene (der Innenkreis nur, wenn es ihn gibt).
            let circles = [major + minor, major - minor]
                .into_iter()
                .filter(|r| *r > tol.linear)
                .map(|radius| Curve::Circle {
                    frame: *frame,
                    radius,
                })
                .collect();
            Ok(Some((circles, *frame)))
        }
        Surface::Nurbs(_) => Err(()),
    }
}

/// Umriss (Silhouette) der Fläche `face` in Blickrichtung `view` (Einheitsvektor, Normale der
/// Skizzenebene): die Linien, an denen die Fläche von vorn nach hinten umschlägt, beschnitten auf
/// das Innere der Fläche wie bei [`section_by_plane`].
///
/// Analytisch: Zylinder und Kegel mit Achse quer zur Blickrichtung ergeben die beiden Mantellinien
/// (beim Kegel bis zur Spitze), die Kugel den Großkreis senkrecht zur Blickrichtung, ein Torus mit
/// Achse in Blickrichtung Außen- und Innenkreis in der Mittelebene. Ebenen und Zylinder/Kegel mit
/// Achse in Blickrichtung haben keinen Umriss (leeres Ergebnis). Jede andere Lage ergibt
/// [`SectionError::Unsupported`], Freiformflächen [`SectionError::Freeform`].
///
/// Die Stücke folgen den Umrisskurven in fester Reihenfolge und je Kurve dem Parameter. Ein Stück,
/// das auf einer echten Randkante liegt, entfällt (die Kante selbst lässt sich projizieren);
/// eines auf der Naht bleibt. Eine Kennung, die nicht zum Körper gehört, ergibt nichts.
pub fn silhouette(
    solid: &Solid,
    face: Id<Face>,
    view: DVec3,
    tol: &Tolerance,
) -> Result<Vec<SectionPiece>, SectionError> {
    if solid.faces.get(face).is_none() {
        return Ok(Vec::new());
    }
    let geom = face_geom(solid, face);
    if matches!(geom.surface, Surface::Nurbs(_)) {
        return Err(SectionError::Freeform(face));
    }
    let view = view.normalize_or_zero();
    if view == DVec3::ZERO {
        return Ok(Vec::new());
    }
    let Some((curves, plane)) =
        silhouette_curves(&geom.surface, view, tol).map_err(|()| SectionError::Unsupported {
            face,
            surface: surface_name(&geom.surface),
        })?
    else {
        return Ok(Vec::new());
    };
    let ptol = pipeline_tol(tol, face_is_freeform(&geom));
    let cut = Surface::Plane { frame: plane };
    let boundary: Vec<Seg> = geom.loops.iter().flatten().cloned().collect();
    let seams = seam_segments(&geom, &boundary, &ptol);
    let hits: Vec<_> = boundary
        .iter()
        .flat_map(|e| edge_hits(e, &cut, &ptol))
        .collect();
    let mut out = Vec::new();
    for c in curves {
        let ts = breakpoints(&c, &hits, &[&geom.surface], &ptol);
        let kept: Vec<(f64, f64)> = split_at(&c, ts, &ptol)
            .into_iter()
            .filter(|s| classify_seg(&geom, &seams, s, &ptol) == Containment::Inside)
            .map(|s| (s.t0, s.t1))
            .collect();
        out.extend(
            join_adjacent(kept, c.period())
                .into_iter()
                .map(|(t0, t1)| SectionPiece {
                    face,
                    curve: c.clone(),
                    t0,
                    t1,
                }),
        );
    }
    Ok(out)
}

/// Fügt aufeinanderfolgende Parameterbereiche einer Kurve zusammen, die lückenlos aneinander
/// stoßen (Haltepunkte an Polen und Nähten teilen sonst etwa den Großkreis der Kugel), bei
/// geschlossenen Kurven auch das letzte Stück mit dem ersten über die Periode hinweg.
/// `ranges` ist aufsteigend geordnet, wie [`split_at`] sie liefert.
fn join_adjacent(ranges: Vec<(f64, f64)>, period: Option<f64>) -> Vec<(f64, f64)> {
    const GAP: f64 = 1e-9;
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (a, b) in ranges {
        match out.last_mut() {
            Some(last) if (a - last.1).abs() <= GAP => last.1 = b,
            _ => out.push((a, b)),
        }
    }
    if let (Some(p), true) = (period, out.len() > 1) {
        let (first, last) = (out[0], out[out.len() - 1]);
        if (first.0 + p - last.1).abs() <= GAP {
            out.pop();
            out[0] = (last.0, first.1 + p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extrude::extrude;
    use crate::geometry::{Curve, Surface};
    use crate::primitives::{make_box, make_cone, make_cylinder, make_sphere};
    use crate::profile::{Profile, ProfileLoop, Segment};
    use crate::revolve::revolve;
    use crate::topology::Solid;
    use planora_math::nurbs::{NurbsCurve, NurbsSurface};
    use planora_math::{DVec2, DVec3, Frame, PI, TAU, Tolerance};

    fn tol() -> Tolerance {
        Tolerance::default()
    }

    fn plane(origin: DVec3, normal: DVec3) -> Frame {
        Frame::from_origin_normal(origin, normal)
    }

    fn seg(p: &SectionPiece) -> Seg {
        Seg::new(p.curve.clone(), p.t0, p.t1)
    }

    fn total_length(ps: &[SectionPiece]) -> f64 {
        ps.iter().map(|p| seg(p).length()).sum()
    }

    /// Keine zwei Stücke sind dieselbe Punktfolge.
    fn assert_no_duplicates(ps: &[SectionPiece]) {
        for (i, a) in ps.iter().enumerate() {
            for b in &ps[i + 1..] {
                assert!(
                    !same_pointwise(&seg(a), &seg(b), &tol()),
                    "doppeltes Stück: {a:?} / {b:?}"
                );
            }
        }
    }

    fn face_with(solid: &Solid, f: impl Fn(&Surface) -> bool) -> Id<Face> {
        solid
            .faces
            .iter()
            .find(|(_, x)| f(&x.surface))
            .map(|(id, _)| id)
            .unwrap()
    }

    #[test]
    fn box_cut_by_mid_plane_gives_four_lines() {
        let b = make_box(DVec3::new(10.0, 20.0, 30.0), &tol()).unwrap();
        let ps = section_by_plane(
            &b,
            &plane(DVec3::new(0.0, 0.0, 15.0), DVec3::Z),
            None,
            &tol(),
        )
        .unwrap();
        assert_eq!(ps.len(), 4);
        assert!(ps.iter().all(|p| matches!(p.curve, Curve::Line { .. })));
        assert!((total_length(&ps) - 60.0).abs() < 1e-9);
        for p in &ps {
            let s = seg(p);
            assert!((s.start().z - 15.0).abs() < 1e-9 && (s.end().z - 15.0).abs() < 1e-9);
        }
    }

    #[test]
    fn box_cut_through_top_face_gives_its_boundary() {
        let b = make_box(DVec3::new(10.0, 20.0, 30.0), &tol()).unwrap();
        let ps = section_by_plane(
            &b,
            &plane(DVec3::new(0.0, 0.0, 30.0), DVec3::Z),
            None,
            &tol(),
        )
        .unwrap();
        assert_eq!(ps.len(), 4);
        assert_no_duplicates(&ps);
        assert!((total_length(&ps) - 60.0).abs() < 1e-9);
    }

    #[test]
    fn cylinder_cut_oblique_gives_ellipse_arcs_inside() {
        let c = make_cylinder(5.0, 20.0, &tol()).unwrap();
        let a = 30f64.to_radians();
        let n = DVec3::new(0.0, a.sin(), a.cos());
        let ps = section_by_plane(&c, &plane(DVec3::new(0.0, 0.0, 10.0), n), None, &tol()).unwrap();
        assert_eq!(ps.len(), 1, "{ps:?}");
        let Curve::Ellipse { major, minor, .. } = ps[0].curve else {
            panic!("keine Ellipse: {:?}", ps[0].curve);
        };
        assert!((major - 5.0 / a.cos()).abs() < 1e-9);
        assert!((minor - 5.0).abs() < 1e-9);
        let s = seg(&ps[0]);
        // geschlossen: Anfang = Ende, eine volle Periode
        assert!(s.start().distance(s.end()) <= tol().linear);
        assert!(((ps[0].t1 - ps[0].t0).abs() - TAU).abs() < 1e-9);
        for i in 0..=64 {
            let p = s.at(i as f64 / 64.0);
            let r = (p.x * p.x + p.y * p.y).sqrt();
            assert!(r <= 5.0 + tol().linear, "außerhalb des Mantels: {p}");
            assert!(p.z >= -tol().linear && p.z <= 20.0 + tol().linear, "{p}");
        }
    }

    #[test]
    fn cylinder_cut_along_axis_gives_two_lines_of_height() {
        let c = make_cylinder(5.0, 20.0, &tol()).unwrap();
        let ps = section_by_plane(
            &c,
            &plane(DVec3::new(2.0, 0.0, 0.0), DVec3::X),
            None,
            &tol(),
        )
        .unwrap();
        let long: Vec<Seg> = ps
            .iter()
            .map(seg)
            .filter(|s| (s.length() - 20.0).abs() < 1e-9)
            .collect();
        assert_eq!(long.len(), 2, "{ps:?}");
        let gap = long[0].at(0.5).distance(long[1].at(0.5));
        assert!((gap - 2.0 * 21f64.sqrt()).abs() < 1e-9);
        // dazu die beiden Sehnen auf den Deckeln
        assert_eq!(ps.len(), 4);
        assert_no_duplicates(&ps);
    }

    /// Die Ebene y = 0 enthält die Naht des Mantels (bei x = 5): Die Mantellinie auf der Naht
    /// ist Inneres der Fläche und bleibt erhalten (nur die Mitte zu befragen, verlöre sie).
    #[test]
    fn cylinder_cut_through_seam_keeps_seam_line() {
        let c = make_cylinder(5.0, 20.0, &tol()).unwrap();
        let ps = section_by_plane(&c, &plane(DVec3::ZERO, DVec3::Y), None, &tol()).unwrap();
        let long: Vec<Seg> = ps
            .iter()
            .map(seg)
            .filter(|s| (s.length() - 20.0).abs() < 1e-9)
            .collect();
        assert_eq!(long.len(), 2, "{ps:?}");
        assert!((long[0].at(0.5).distance(long[1].at(0.5)) - 10.0).abs() < 1e-9);
        assert_eq!(ps.len(), 4, "{ps:?}");
    }

    #[test]
    fn tangent_plane_gives_no_zero_length_pieces() {
        let t = tol();
        let c = make_cylinder(5.0, 20.0, &t).unwrap();
        let ps =
            section_by_plane(&c, &plane(DVec3::new(5.0, 0.0, 0.0), DVec3::X), None, &t).unwrap();
        assert!(ps.iter().all(|p| seg(p).length() > t.linear), "{ps:?}");
        // dichte Folge x = −6 … 6 in 0,01
        for i in 0..=1200 {
            let x = -6.0 + i as f64 * 0.01;
            let ps = section_by_plane(&c, &plane(DVec3::new(x, 0.0, 0.0), DVec3::X), None, &t)
                .unwrap_or_else(|e| panic!("x = {x}: {e}"));
            assert!(
                ps.iter().all(|p| seg(p).length() > t.linear),
                "x = {x}: Stück ohne Länge"
            );
            if x.abs() < 5.0 - t.linear {
                // zwei Mantellinien und zwei Sehnen auf den Deckeln
                assert_eq!(ps.len(), 4, "x = {x}: {ps:?}");
            } else if x.abs() > 5.0 + t.linear {
                assert!(ps.is_empty(), "x = {x}: {ps:?}");
            }
        }
    }

    #[test]
    fn plane_through_box_edge_gives_that_edge_once() {
        let b = make_box(DVec3::new(10.0, 20.0, 30.0), &tol()).unwrap();
        let ps = section_by_plane(&b, &plane(DVec3::ZERO, DVec3::X), None, &tol()).unwrap();
        // Die Fläche x = 0 liegt ganz in der Ebene: genau ihre vier Randkanten.
        assert_eq!(ps.len(), 4, "{ps:?}");
        assert_no_duplicates(&ps);
        let face = face_with(&b, |s| match s {
            Surface::Plane { frame } => {
                frame.z.cross(DVec3::X).length() < 1e-9 && frame.origin.x.abs() < 1e-9
            }
            _ => false,
        });
        assert!(ps.iter().all(|p| p.face == face));
        for p in &ps {
            let s = seg(p);
            assert!(s.start().x.abs() < 1e-9 && s.end().x.abs() < 1e-9);
        }
        assert!((total_length(&ps) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn sphere_cut_gives_circle() {
        let s = make_sphere(5.0, &tol()).unwrap();
        let ps = section_by_plane(
            &s,
            &plane(DVec3::new(0.0, 0.0, 3.0), DVec3::Z),
            None,
            &tol(),
        )
        .unwrap();
        assert_eq!(ps.len(), 1, "{ps:?}");
        let Curve::Circle { radius, .. } = ps[0].curve else {
            panic!("kein Kreis: {:?}", ps[0].curve);
        };
        assert!((radius - 4.0).abs() < 1e-9);
        // ganzer Kreis (die Länge aus `Seg::length` ist nur ein Streckenzug)
        assert!(((ps[0].t1 - ps[0].t0).abs() - TAU).abs() < 1e-9);
    }

    #[test]
    fn oblique_cone_is_unsupported() {
        let c = make_cone(5.0, 10.0, &tol()).unwrap();
        let a = 30f64.to_radians();
        let n = DVec3::new(0.0, a.sin(), a.cos());
        let err =
            section_by_plane(&c, &plane(DVec3::new(0.0, 0.0, 5.0), n), None, &tol()).unwrap_err();
        let cone = face_with(&c, |s| matches!(s, Surface::Cone { .. }));
        assert_eq!(
            err,
            SectionError::Unsupported {
                face: cone,
                surface: "Kegel"
            }
        );
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn nurbs_face_is_freeform_error() {
        let t = tol();
        let mut c = make_cylinder(5.0, 20.0, &t).unwrap();
        let line = NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![DVec3::new(5.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 20.0)],
            vec![1.0; 2],
        )
        .unwrap();
        let nurbs = Surface::nurbs(
            NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
            true,
            false,
        )
        .unwrap();
        let side = face_with(&c, |s| matches!(s, Surface::Cylinder { .. }));
        c.faces[side].surface = nurbs;
        let cut = plane(DVec3::new(2.0, 0.0, 0.0), DVec3::X);
        let err = section_by_plane(&c, &cut, None, &t).unwrap_err();
        assert_eq!(err, SectionError::Freeform(side));
        assert!(!err.to_string().is_empty());
        // Nur ein ebener Deckel gewählt: kein Fehler, seine Sehne.
        let cap = face_with(&c, |s| matches!(s, Surface::Plane { .. }));
        let ps = section_by_plane(&c, &cut, Some(&[cap]), &t).unwrap();
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].face, cap);
    }

    /// Fix-Runde 1: Die Ebene x = y geht durch zwei gegenüberliegende senkrechte Kanten des
    /// Quaders: Beide Kanten gehören zum Schnitt, die Kontur ist geschlossen.
    #[test]
    fn diagonal_plane_through_box_edges_gives_closed_contour() {
        let t = tol();
        let b = make_box(DVec3::new(10.0, 10.0, 30.0), &t).unwrap();
        let n = DVec3::new(1.0, -1.0, 0.0).normalize();
        let ps = section_by_plane(&b, &plane(DVec3::ZERO, n), None, &t).unwrap();
        assert_eq!(ps.len(), 4, "{ps:?}");
        assert_no_duplicates(&ps);
        // geschlossen: jeder Endpunkt kommt genau zweimal vor
        let ends: Vec<DVec3> = ps
            .iter()
            .flat_map(|p| {
                let s = seg(p);
                [s.start(), s.end()]
            })
            .collect();
        for e in &ends {
            let k = ends.iter().filter(|x| x.distance(*e) <= t.linear).count();
            assert_eq!(k, 2, "offenes Ende bei {e}: {ps:?}");
        }
        let verticals = ps
            .iter()
            .map(seg)
            .filter(|s| (s.length() - 30.0).abs() < 1e-9)
            .count();
        assert_eq!(verticals, 2);
    }

    /// Fix-Runde 1: Die Ebene x + y = 0 berührt den Quader nur an der Kante x = y = 0: kein Stück,
    /// kein Fehler.
    #[test]
    fn plane_touching_box_edge_gives_nothing() {
        let t = tol();
        let b = make_box(DVec3::new(10.0, 10.0, 30.0), &t).unwrap();
        let n = DVec3::new(1.0, 1.0, 0.0).normalize();
        let ps = section_by_plane(&b, &plane(DVec3::ZERO, n), None, &t).unwrap();
        assert!(ps.is_empty(), "{ps:?}");
    }

    /// Fix-Runde 1: Eine Freiformfläche abseits der Ebene stört nicht; wird sie geschnitten, gibt
    /// es den Freiform-Fehler.
    #[test]
    fn nurbs_face_away_from_plane_is_skipped() {
        let t = tol();
        let mut b = make_box(DVec3::new(10.0, 20.0, 30.0), &t).unwrap();
        let top = face_with(&b, |s| match s {
            Surface::Plane { frame } => {
                (frame.origin.z - 30.0).abs() < 1e-9 && frame.z.z.abs() > 0.5
            }
            _ => false,
        });
        let edge = NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![DVec3::new(0.0, 0.0, 30.0), DVec3::new(10.0, 0.0, 30.0)],
            vec![1.0; 2],
        )
        .unwrap();
        let s = NurbsSurface::extrude(&edge, DVec3::new(0.0, 20.0, 0.0)).unwrap();
        b.faces[top].surface = Surface::nurbs(s, false, false).unwrap();
        let ps =
            section_by_plane(&b, &plane(DVec3::new(0.0, 0.0, 15.0), DVec3::Z), None, &t).unwrap();
        assert_eq!(ps.len(), 4, "{ps:?}");
        assert!((total_length(&ps) - 60.0).abs() < 1e-9);
        let err = section_by_plane(&b, &plane(DVec3::new(0.0, 0.0, 30.0), DVec3::Z), None, &t)
            .unwrap_err();
        assert_eq!(err, SectionError::Freeform(top));
    }

    // --- Silhouette (Skizzen 6, Task 2) ---

    /// Halber Zylinder r = 5, h = 20: Mantel nur 0…180° (y ≥ 0), Achse z.
    fn half_cylinder(t: &Tolerance) -> Solid {
        let profile = Profile {
            outer: ProfileLoop {
                start: DVec2::new(5.0, 0.0),
                segments: vec![
                    Segment::Arc {
                        center: DVec2::ZERO,
                        sweep: PI,
                    },
                    Segment::Line {
                        to: DVec2::new(5.0, 0.0),
                    },
                ],
            },
            holes: vec![],
        };
        extrude(&profile, &Frame::WORLD, 20.0, t).unwrap()
    }

    /// Drehkörper um z mit einem Torusstück (R = 5, r = 2), das außen (R + r) und innen (R − r)
    /// über die Mittelebene z = 0 reicht.
    fn torus_solid(t: &Tolerance) -> Solid {
        let chain = ProfileLoop {
            start: DVec2::new(0.0, -2.0),
            segments: vec![
                Segment::Line {
                    to: DVec2::new(5.0, -2.0),
                },
                Segment::Arc {
                    center: DVec2::new(5.0, 0.0),
                    sweep: 300f64.to_radians(),
                },
                Segment::Line {
                    to: DVec2::new(0.0, -1.0),
                },
            ],
        };
        revolve(&chain, &Frame::WORLD, t).unwrap()
    }

    #[test]
    fn cylinder_side_view_gives_two_lines_of_height() {
        let t = tol();
        let c = make_cylinder(5.0, 20.0, &t).unwrap();
        let side = face_with(&c, |s| matches!(s, Surface::Cylinder { .. }));
        let ps = silhouette(&c, side, DVec3::Y, &t).unwrap();
        assert_eq!(ps.len(), 2, "{ps:?}");
        let mut xs: Vec<f64> = ps
            .iter()
            .map(|p| {
                assert!(matches!(p.curve, Curve::Line { .. }), "{p:?}");
                assert_eq!(p.face, side);
                let s = seg(p);
                assert!((s.length() - 20.0).abs() < 1e-9, "{p:?}");
                assert!(s.start().y.abs() < 1e-9 && s.end().y.abs() < 1e-9);
                assert!((s.start().x - s.end().x).abs() < 1e-9);
                s.start().x
            })
            .collect();
        xs.sort_by(|a, b| a.total_cmp(b));
        assert!(
            (xs[0] + 5.0).abs() < 1e-9 && (xs[1] - 5.0).abs() < 1e-9,
            "{xs:?}"
        );
    }

    #[test]
    fn half_cylinder_gives_one_line() {
        let t = tol();
        let c = half_cylinder(&t);
        let side = face_with(&c, |s| matches!(s, Surface::Cylinder { .. }));
        // Blick entlang x: Umrisslinien bei y = ±5, nur y = +5 liegt auf dem Mantel.
        let ps = silhouette(&c, side, DVec3::X, &t).unwrap();
        assert_eq!(ps.len(), 1, "{ps:?}");
        let s = seg(&ps[0]);
        assert!((s.length() - 20.0).abs() < 1e-9);
        assert!(
            (s.at(0.5) - DVec3::new(0.0, 5.0, 10.0)).length() < 1e-9,
            "{ps:?}"
        );
    }

    #[test]
    fn sphere_gives_full_circle() {
        let t = tol();
        let s = make_sphere(5.0, &t).unwrap();
        let face = face_with(&s, |x| matches!(x, Surface::Sphere { .. }));
        for view in [DVec3::Z, DVec3::X, DVec3::new(1.0, 2.0, 3.0).normalize()] {
            let ps = silhouette(&s, face, view, &t).unwrap();
            assert_eq!(ps.len(), 1, "{view}: {ps:?}");
            let Curve::Circle { frame, radius } = ps[0].curve else {
                panic!("kein Kreis: {:?}", ps[0].curve);
            };
            assert!((radius - 5.0).abs() < 1e-9);
            assert!(frame.origin.length() < 1e-9);
            assert!(frame.z.cross(view).length() < 1e-9, "{view}: {:?}", frame.z);
            assert!(((ps[0].t1 - ps[0].t0).abs() - TAU).abs() < 1e-9, "{ps:?}");
        }
    }

    #[test]
    fn torus_top_view_gives_two_circles() {
        let t = tol();
        let s = torus_solid(&t);
        let face = face_with(&s, |x| matches!(x, Surface::Torus { .. }));
        let ps = silhouette(&s, face, DVec3::Z, &t).unwrap();
        assert_eq!(ps.len(), 2, "{ps:?}");
        let radii: Vec<f64> = ps
            .iter()
            .map(|p| {
                let Curve::Circle { frame, radius } = p.curve else {
                    panic!("kein Kreis: {:?}", p.curve);
                };
                assert!(frame.origin.length() < 1e-9, "{p:?}");
                assert!(frame.z.cross(DVec3::Z).length() < 1e-9);
                assert!(((p.t1 - p.t0).abs() - TAU).abs() < 1e-9, "{p:?}");
                radius
            })
            .collect();
        // feste Reihenfolge: erst außen (R + r), dann innen (R − r)
        assert!(
            (radii[0] - 7.0).abs() < 1e-9 && (radii[1] - 3.0).abs() < 1e-9,
            "{radii:?}"
        );
        // Blick quer zur Achse: kein Rechenweg
        let err = silhouette(&s, face, DVec3::X, &t).unwrap_err();
        assert_eq!(
            err,
            SectionError::Unsupported {
                face,
                surface: "Torus"
            }
        );
    }

    #[test]
    fn cylinder_view_along_axis_is_empty() {
        let t = tol();
        let c = make_cylinder(5.0, 20.0, &t).unwrap();
        for (id, _) in c.faces.iter() {
            assert!(silhouette(&c, id, DVec3::Z, &t).unwrap().is_empty());
            assert!(silhouette(&c, id, -DVec3::Z, &t).unwrap().is_empty());
        }
    }

    #[test]
    fn oblique_cylinder_is_unsupported() {
        let t = tol();
        let c = make_cylinder(5.0, 20.0, &t).unwrap();
        let side = face_with(&c, |s| matches!(s, Surface::Cylinder { .. }));
        let a = 30f64.to_radians();
        let err = silhouette(&c, side, DVec3::new(0.0, a.sin(), a.cos()), &t).unwrap_err();
        assert_eq!(
            err,
            SectionError::Unsupported {
                face: side,
                surface: "Zylinder"
            }
        );
    }

    /// Kegel r = 5, h = 10 von der Seite: zwei Mantellinien vom Grundkreis bis zur Spitze.
    #[test]
    fn cone_side_view_gives_two_generators_to_apex() {
        let t = tol();
        let c = make_cone(5.0, 10.0, &t).unwrap();
        let cone = face_with(&c, |s| matches!(s, Surface::Cone { .. }));
        let ps = silhouette(&c, cone, DVec3::Y, &t).unwrap();
        assert_eq!(ps.len(), 2, "{ps:?}");
        let apex = DVec3::new(0.0, 0.0, 10.0);
        let mut feet: Vec<f64> = ps
            .iter()
            .map(|p| {
                let s = seg(p);
                assert!((s.length() - 125f64.sqrt()).abs() < 1e-9, "{p:?}");
                let (a, b) = (s.start(), s.end());
                let (foot, top) = if a.z < b.z { (a, b) } else { (b, a) };
                assert!(top.distance(apex) < 1e-9, "{p:?}");
                assert!(foot.z.abs() < 1e-9 && foot.y.abs() < 1e-9);
                foot.x
            })
            .collect();
        feet.sort_by(|a, b| a.total_cmp(b));
        assert!((feet[0] + 5.0).abs() < 1e-9 && (feet[1] - 5.0).abs() < 1e-9);
        // Blick entlang der Achse: leer; schräg: kein Rechenweg
        assert!(silhouette(&c, cone, DVec3::Z, &t).unwrap().is_empty());
        let err = silhouette(&c, cone, DVec3::new(1.0, 0.0, 1.0).normalize(), &t).unwrap_err();
        assert_eq!(
            err,
            SectionError::Unsupported {
                face: cone,
                surface: "Kegel"
            }
        );
    }

    #[test]
    fn planar_face_has_no_silhouette_and_nurbs_is_freeform() {
        let t = tol();
        let mut c = make_cylinder(5.0, 20.0, &t).unwrap();
        let cap = face_with(&c, |s| matches!(s, Surface::Plane { .. }));
        assert!(silhouette(&c, cap, DVec3::Y, &t).unwrap().is_empty());
        let line = NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![DVec3::new(5.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 20.0)],
            vec![1.0; 2],
        )
        .unwrap();
        let side = face_with(&c, |s| matches!(s, Surface::Cylinder { .. }));
        c.faces[side].surface = Surface::nurbs(
            NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
            true,
            false,
        )
        .unwrap();
        assert_eq!(
            silhouette(&c, side, DVec3::Y, &t).unwrap_err(),
            SectionError::Freeform(side)
        );
    }
}
