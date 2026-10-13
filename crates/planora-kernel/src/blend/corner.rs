//! Ecken im Stil „rollende Kugel“ (Spec §4.3, R7, P6): Zwei gerundete Kanten an einer Ecke
//! treffen sich an der winkelhalbierenden Ebene (Gehrung, nur bei spiegelgleicher Lage, sonst
//! benannt abgelehnt); drei Rundungen mit gleichem Radius an einer Ecke aus drei Ebenen bekommen
//! ein Kugelstück um den Punkt im Abstand r von allen drei Ebenen.
//!
//! Gehrung nur, wenn sie die Schnittmenge der einzeln gerundeten Körper ist (P6): Die dritte,
//! ungerundete Kante an der Ecke muss dieselbe Art (konvex/konkav) haben wie die beiden
//! gerundeten. An einer einspringenden Ecke (zwei konvexe Kanten, dritte konkav) wiche die
//! Gehrung davon ab; das wird benannt abgelehnt. Die Gehrung gilt erst als bewiesen, wenn beide
//! Abschlusskurven auf der Gehrungsebene zusammenfallen und jede an Stützstellen auf der
//! Rundungsfläche der anderen Kante liegt.

use super::analytic::{EdgeGeom, outward_sense};
use super::build::{
    EndCut, ON_CURVE_FACTOR, blend_tol, chain_loop, cut_with, loop_patch, orient_outer, same_cut,
    three_faces,
};
use super::chain::leaving_direction;
use super::error::BlendError;
use super::topo::{edge_angle, edge_sides, face_name, vertex_edges};
use crate::boolean::{Piece, Seg};
use crate::geometry::Surface;
use crate::ids::Id;
use crate::topology::{Face, FaceTag, Solid, Vertex};
use planora_math::{DVec3, Frame, PI, Tolerance};

/// Größter Betrag von `nᵢ · nⱼ`, bei dem drei Flächen als paarweise rechtwinklig gelten
/// (Stufe 2; Task 17 lockert).
pub const ORTHOGONAL_TOL: f64 = 1e-9;
/// Kleinster Betrag der Determinante der drei Normalen: darunter schneiden sich die drei
/// Ebenen nicht sicher in einem Punkt.
const MIN_CORNER_DET: f64 = 1e-6;
/// Spielraum der Pol-Prüfung des Kugelstücks (Baryzentrische Koordinaten).
const POLE_MARGIN: f64 = 1e-6;
/// Stützstellen je Abschlusskurve, an denen geprüft wird, dass sie auf der anderen Rundungsfläche
/// (Gehrung) bzw. auf der Kugel (Kugelstück) liegt.
const CORNER_PROBES: usize = 16;

/// Abschlüsse an einer Ecke: (Index der Kante, Ende statt Anfang?, Abschluss).
pub type CornerCuts = Vec<(usize, bool, EndCut)>;

/// Benannte Ablehnung an der Ecke der Flächen `faces`.
fn at_corner(solid: &Solid, faces: &[Id<Face>], msg: &str) -> BlendError {
    BlendError::Corner(format!("{msg} (Ecke der Flächen {})", names(solid, faces)))
}

/// Namen der Flächen für Meldungen: „A“, „B“ und „C“.
fn names(solid: &Solid, faces: &[Id<Face>]) -> String {
    let n: Vec<String> = faces
        .iter()
        .map(|f| format!("„{}“", face_name(solid, *f)))
        .collect();
    match n.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} und {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

/// Liegt die Kurve an allen Stützstellen höchstens `weld` neben der Fläche? NaN gilt als „nicht“.
fn lies_on(seg: &Seg, surface: &Surface, weld: f64) -> bool {
    (0..=CORNER_PROBES).all(|k| {
        let p = seg.at(k as f64 / CORNER_PROBES as f64);
        surface.implicit(p).abs() <= weld
    })
}

/// Gehrung: zwei gerundete Kanten an einer Ecke mit drei Flächen, die dritte Kante bleibt.
pub fn miter_pair(
    solid: &Solid,
    geoms: &[EdgeGeom],
    v: Id<Vertex>,
    (i, ei): (usize, bool),
    (j, ej): (usize, bool),
    tol: &Tolerance,
) -> Result<CornerCuts, BlendError> {
    let (gi, gj) = (&geoms[i], &geoms[j]);
    let faces = three_faces(solid, v)?;
    if gi.convex != gj.convex {
        return Err(at_corner(
            solid,
            &faces,
            "An der Ecke treffen eine konvexe und eine konkave Kante zusammen; das wird nicht unterstützt",
        ));
    }
    if !(gi.analytic && gj.analytic) {
        return Err(at_corner(
            solid,
            &faces,
            "Ecken mit Freiform-Rundungsflächen werden noch nicht unterstützt",
        ));
    }
    let fi = [gi.sides.a.face, gi.sides.b.face];
    let fj = [gj.sides.a.face, gj.sides.b.face];
    if fj.iter().filter(|f| fi.contains(f)).count() != 1
        || fi.iter().chain(&fj).any(|f| !faces.contains(f))
    {
        return Err(at_corner(
            solid,
            &faces,
            "Die beiden Kanten haben keine gemeinsame Fläche",
        ));
    }
    // Die dritte, ungerundete Kante an der Ecke (zwischen den beiden nicht gemeinsamen Flächen).
    let third: Vec<_> = vertex_edges(solid, v)
        .into_iter()
        .filter(|&e| e != gi.edge && e != gj.edge)
        .collect();
    let [third] = third.as_slice() else {
        return Err(BlendError::Corner(format!(
            "An der Ecke der Flächen {} treffen {} weitere Kanten zusammen; unterstützt ist eine",
            names(solid, &faces),
            third.len()
        )));
    };
    let convex_third = edge_sides(solid, *third, None)
        .and_then(|s| edge_angle(solid, *third, &s))
        .map_err(|_| {
            BlendError::Corner(format!(
                "Die dritte Kante an der Ecke der Flächen {} ist keine scharfe Kante; dort lässt sich keine Gehrung bilden",
                names(solid, &faces)
            ))
        })?
        .convex;
    if convex_third != gi.convex {
        let (rounded, other) = if gi.convex {
            ("konvexe", "konkav")
        } else {
            ("konkave", "konvex")
        };
        return Err(BlendError::Corner(format!(
            "Zwei {rounded} Rundungen an einer Ecke, deren dritte Kante {other} ist (Flächen {}), werden noch nicht unterstützt",
            names(solid, &faces)
        )));
    }
    let vp = solid.vertices[v].point;
    let n = (leaving_direction(solid, gi.edge, v) - leaving_direction(solid, gj.edge, v))
        .normalize_or_zero();
    if !n.is_finite() || n == DVec3::ZERO {
        return Err(at_corner(
            solid,
            &faces,
            "Die beiden Kanten laufen in dieselbe Richtung",
        ));
    }
    let plane = Surface::Plane {
        frame: Frame::from_origin_normal(vp, n),
    };
    let cut = |g: &EdgeGeom, end: bool| {
        cut_with(g, end, &plane, vp, tol)
            .map(|(sa, sb, curve)| EndCut {
                sa,
                sb,
                curve,
                cap: None,
            })
            .ok_or_else(|| {
                BlendError::Corner(format!(
                    "Die Rundungen lassen sich an der Ecke der Flächen {} nicht zusammenführen",
                    names(solid, &faces)
                ))
            })
    };
    let (ci, cj) = (cut(gi, ei)?, cut(gj, ej)?);
    let weld = ON_CURVE_FACTOR * blend_tol(solid, geoms, tol).linear;
    if !(same_cut(&ci.curve, &cj.curve, weld)
        && lies_on(&ci.curve, &gj.surface, weld)
        && lies_on(&cj.curve, &gi.surface, weld))
    {
        return Err(BlendError::Corner(format!(
            "Die Rundungen treffen sich an der Ecke der Flächen {} nicht auf der winkelhalbierenden Ebene (verschiedene Maße oder Winkel); das wird nicht unterstützt",
            names(solid, &faces)
        )));
    }
    Ok(vec![(i, ei, ci), (j, ej, cj)])
}

/// Punkt im Abstand `r` innerhalb aller drei Ebenen durch `vp` mit den Außennormalen `n`:
/// Lösung von `nₖ · S = nₖ · vp − r` (Cramer); `None`, wenn die Ebenen fast parallel sind.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn ball_center(n: [DVec3; 3], vp: DVec3, r: f64) -> Option<DVec3> {
    let det = n[0].dot(n[1].cross(n[2]));
    if !(det.abs() > MIN_CORNER_DET) {
        return None;
    }
    let d: Vec<f64> = n.iter().map(|x| x.dot(vp) - r).collect();
    let s = (n[1].cross(n[2]) * d[0] + n[2].cross(n[0]) * d[1] + n[0].cross(n[1]) * d[2]) / det;
    s.is_finite().then_some(s)
}

/// Baryzentrische Koordinaten von `d` in der Basis `n` (alle ≥ 0: `d` liegt im Kegel der drei
/// Normalen, also im Kugeldreieck der Ecke).
fn inside_cone(n: [DVec3; 3], d: DVec3) -> bool {
    let det = n[0].dot(n[1].cross(n[2]));
    let a = d.dot(n[1].cross(n[2])) / det;
    let b = n[0].dot(d.cross(n[2])) / det;
    let c = n[0].dot(n[1].cross(d)) / det;
    a > -POLE_MARGIN && b > -POLE_MARGIN && c > -POLE_MARGIN
}

/// Drei Rundungen mit gleichem Radius an einer Ecke aus drei Ebenen: jede Rundung endet an der
/// Ebene durch den Kugelmittelpunkt senkrecht zu ihrer Kante; dazwischen ein Kugelstück
/// („Ecke `index`“). Liefert die Abschlüsse, das Stück und seine geschätzte Volumenänderung.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn corner_three(
    solid: &Solid,
    geoms: &[EdgeGeom],
    v: Id<Vertex>,
    list: &[(usize, bool)],
    index: usize,
    tol: &Tolerance,
) -> Result<(CornerCuts, Piece, f64), BlendError> {
    if list.len() != 3 {
        return Err(BlendError::Corner(format!(
            "An der Ecke treffen {} gerundete Kanten zusammen; unterstützt sind höchstens drei",
            list.len()
        )));
    }
    let faces = three_faces(solid, v)?;
    let mut radius: Option<f64> = None;
    let mut pairs: Vec<[Id<Face>; 2]> = Vec::with_capacity(3);
    for &(i, _) in list {
        let g = &geoms[i];
        let r = match g.surface {
            Surface::Cylinder { radius, .. } if g.analytic => radius,
            Surface::Plane { .. } if g.analytic => {
                return Err(at_corner(
                    solid,
                    &faces,
                    "Ecken mit drei Fasen werden nicht unterstützt",
                ));
            }
            _ => {
                return Err(BlendError::Corner(format!(
                    "Nur gerade Kanten zwischen Ebenen bekommen ein Kugelstück (Kante zwischen {})",
                    names(solid, &[g.sides.a.face, g.sides.b.face])
                )));
            }
        };
        if !g.convex {
            return Err(BlendError::Corner(format!(
                "Ecken mit konkaven Kanten werden noch nicht unterstützt (Flächen {})",
                names(solid, &faces)
            )));
        }
        match radius {
            Some(r0) if !((r0 - r).abs() <= tol.linear) => {
                return Err(BlendError::Corner(format!(
                    "Die Radien der Kanten an der Ecke der Flächen {} sind verschieden",
                    names(solid, &faces)
                )));
            }
            _ => radius = Some(r),
        }
        let mut pair = [g.sides.a.face, g.sides.b.face];
        pair.sort();
        if !pair.iter().all(|f| faces.contains(f)) || pairs.contains(&pair) {
            return Err(BlendError::Corner(format!(
                "Die Kanten an der Ecke der Flächen {} liegen nicht je zwischen zwei der drei Flächen",
                names(solid, &faces)
            )));
        }
        pairs.push(pair);
    }
    let r = radius.ok_or_else(|| at_corner(solid, &faces, "An der Ecke fehlt eine Rundung"))?;
    let mut n = [DVec3::ZERO; 3];
    for (k, f) in faces.iter().enumerate() {
        let face = &solid.faces[*f];
        let Surface::Plane { frame } = &face.surface else {
            return Err(BlendError::Corner(format!(
                "Nur Ecken aus drei Ebenen bekommen ein Kugelstück; die Fläche „{}“ ist keine Ebene",
                face_name(solid, *f)
            )));
        };
        n[k] = if face.same_sense { frame.z } else { -frame.z };
    }
    if (0..3).any(|a| !(n[a].dot(n[(a + 1) % 3]).abs() <= ORTHOGONAL_TOL)) {
        return Err(BlendError::Corner(format!(
            "Nur rechtwinklige Ecken bekommen bisher ein Kugelstück (Flächen {})",
            names(solid, &faces)
        )));
    }
    let vp = solid.vertices[v].point;
    let center = ball_center(n, vp, r).ok_or_else(|| {
        at_corner(
            solid,
            &faces,
            "Die drei Flächen schneiden sich nicht in einem Punkt",
        )
    })?;
    let weld = ON_CURVE_FACTOR * blend_tol(solid, geoms, tol).linear;
    let sphere_miss = || {
        BlendError::Corner(format!(
            "Die Rundungen an der Ecke der Flächen {} treffen die Kugel nicht",
            names(solid, &faces)
        ))
    };
    let mut cuts = Vec::with_capacity(3);
    for &(i, end) in list {
        let g = &geoms[i];
        let plane = Surface::Plane {
            frame: Frame::from_origin_normal(center, leaving_direction(solid, g.edge, v)),
        };
        let (sa, sb, curve) = cut_with(g, end, &plane, vp, tol).ok_or_else(|| {
            BlendError::Corner(format!(
                "Die Rundung zwischen {} lässt sich an der Kugelecke nicht abschließen",
                names(solid, &[g.sides.a.face, g.sides.b.face])
            ))
        })?;
        // Der Abschluss muss ein Bogen der Kugel sein (Kugelmitte auf der Rundungsachse).
        let on_sphere = (0..=CORNER_PROBES).all(|k| {
            let p = curve.at(k as f64 / CORNER_PROBES as f64);
            (p.distance(center) - r).abs() <= weld
        });
        if !on_sphere {
            return Err(sphere_miss());
        }
        cuts.push((
            i,
            end,
            EndCut {
                sa,
                sb,
                curve,
                cap: None,
            },
        ));
    }
    // Kugel so legen, dass beide Pole außerhalb des Kugeldreiecks liegen.
    let z = [n[0] - n[1], n[1] - n[2], n[2] - n[0]]
        .into_iter()
        .map(|d| d.normalize())
        .find(|d| !inside_cone(n, *d) && !inside_cone(n, -*d))
        .ok_or_else(|| {
            at_corner(
                solid,
                &faces,
                "Für das Kugelstück gibt es keine Lage ohne Pol",
            )
        })?;
    let mid = (n[0] + n[1] + n[2]).normalize();
    let x = (mid - z * mid.dot(z)).normalize();
    let frame = Frame {
        origin: center,
        x,
        y: z.cross(x),
        z,
    };
    let surface = Surface::Sphere { frame, radius: r };
    let segs = chain_loop(cuts.iter().map(|(_, _, c)| c.curve.clone()).collect(), weld)
        .ok_or_else(sphere_miss)?;
    let m = center + mid * r;
    let same_sense = outward_sense(mid, m, vp, true);
    let outer = orient_outer(&surface, same_sense, segs);
    let piece = Piece {
        domain: loop_patch(&surface, same_sense, &outer),
        surface,
        same_sense,
        outer,
        holes: Vec::new(),
        tag: Some(FaceTag {
            origin: 0,
            role: format!("Ecke {index}"),
        }),
    };
    // Rechtwinklige Ecke: Würfel r³ minus Kugelachtel.
    let volume = -(1.0 - PI / 6.0) * r * r * r;
    Ok((cuts, piece, volume))
}
