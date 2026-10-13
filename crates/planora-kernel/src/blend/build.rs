//! Endabschlüsse der Rundungsflächen und Umbau der Nachbarflächen (Spec §4.1, §4.5, R6): Die
//! Nachbarflächen werden bis zu den Berührkurven gekürzt, Abschlussflächen bekommen die
//! Abschlusskurve als neue Kante, alle Flächen gehen als `Piece` in `sew`; danach prüft
//! `validate_with` das Ergebnis.

use super::analytic::{EdgeGeom, VolumeModel};
use super::chain::{CHAIN_ANGLE, leaving_direction};
use super::error::BlendError;
use super::topo::{NEAR_TANGENT_ANGLE, face_name, outward_normal, vertex_faces};
use crate::boolean::{
    Containment, CurveSurface, FaceGeom, Piece, Seg, SurfaceIntersection, curve_surface_roots,
    face_geom, intersect_surfaces, loop_points, oriented_area, pipeline_tol, point_in_face,
    points_to_uv, reverse_loop, seg_intersections, sew,
};
use crate::geometry::{Curve, Surface, UvRect};
use crate::ids::Id;
use crate::topology::{Face, FaceTag, Solid, Vertex};
use crate::validate::validate_with;
use planora_math::{DVec3, PI, Tolerance};
use std::collections::{BTreeMap, HashMap};

/// Vielfaches der (ggf. erweiterten) Längentoleranz, bis zu dem ein Punkt auf einer Kurve liegt
/// oder zwei Punkte zusammenfallen (Abschlusskurven, gekürzte Nachbarkanten, Ecken). Die
/// analytischen Schnitte sind auf etwa 1e-12 genau; der Faktor lässt Spielraum für Newton.
pub const ON_CURVE_FACTOR: f64 = 10.0;
/// Suchfenster für den Schnitt einer Berührkurve mit der Abschlussfläche, in Vielfachen der
/// Reichweite (größter Abstand Kante–Berührpunkt). Schräge Abschlussflächen verschieben den
/// Schnitt höchstens um die Reichweite mal den Kotangens des Schnittwinkels.
pub const END_SEARCH_FACTOR: f64 = 4.0;
/// Kleinster Anteil, den eine gekürzte Nachbarkante von ihrer alten Länge behalten muss; darunter
/// (oder bei Umkehr) ist die Rundung zu groß für diese Fläche. Gleiche Schranke für die
/// Berührkurven zwischen den beiden Abschlüssen (relativ zum Kantenbereich).
const MIN_TRIMMED_FRACTION: f64 = 1e-6;
/// Stützpunkte je Stück beim Bestimmen des Parameterbereichs neuer Flächen (dichter als
/// `loop_points`, damit Bögen den Bereich nicht unterschätzen).
const PATCH_SAMPLES_PER_SEG: usize = 64;

/// Abschluss einer Rundungsfläche an einem Ende.
#[derive(Clone, Debug)]
pub struct EndCut {
    /// Parameter der Berührkurven A und B am Abschluss.
    pub sa: f64,
    pub sb: f64,
    /// Abschlusskurve auf der Rundungsfläche, von A nach B.
    pub curve: Seg,
    /// Abschlussfläche und Ecke, an der die Kante endete (`None` bei Naht, glattem Übergang und
    /// Ecken).
    pub cap: Option<(Id<Face>, Id<Vertex>)>,
}

/// Abschlüsse aller Kanten (Reihenfolge wie die Kanten) und Eckstücke.
#[derive(Clone, Debug, Default)]
pub struct EndPlan {
    pub starts: Vec<EndCut>,
    pub ends: Vec<EndCut>,
    pub corners: Vec<Piece>,
    /// Geschätzte Volumenänderung der Eckstücke (mm³, negativ = Material weg).
    pub corner_volume: f64,
    /// Summe der Beträge der Eckstück-Schätzungen (mm³, für [`super::Blended::volume_scale`]).
    pub corner_scale: f64,
}

/// Längentoleranz des Umbaus: mit Freiformflächen, Freiformkanten oder Hautflächen die weitere
/// Knotentoleranz (`pipeline_tol`), sonst unverändert.
pub fn blend_tol(solid: &Solid, geoms: &[EdgeGeom], tol: &Tolerance) -> Tolerance {
    let freeform = geoms.iter().any(|g| !g.analytic)
        || solid
            .faces
            .iter()
            .any(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        || solid
            .edges
            .iter()
            .any(|(_, e)| matches!(e.curve, Curve::Nurbs(_)));
    pipeline_tol(tol, freeform)
}

/// Parameter des Kurvenpunkts zu `p`; bei periodischen Kurven der Wert, der `near` am nächsten
/// liegt.
pub fn param_near(curve: &Curve, p: DVec3, near: f64) -> f64 {
    let t = curve.project(p);
    match curve.period() {
        Some(per) => t + per * ((near - t) / per).round(),
        None => t,
    }
}

/// Liegt `p` beim Parameter `t` auf der Kurve? NaN gilt als „nicht“.
fn on_curve(curve: &Curve, t: f64, p: DVec3, tol: &Tolerance) -> bool {
    curve.point(t).distance(p) <= ON_CURVE_FACTOR * tol.linear
}

/// Flächen an der Ecke `v`; genau drei werden unterstützt (Abschlüsse, glatte Übergänge, Ecken).
pub(crate) fn three_faces(solid: &Solid, v: Id<Vertex>) -> Result<Vec<Id<Face>>, BlendError> {
    let faces = vertex_faces(solid, v);
    if faces.len() != 3 {
        return Err(BlendError::Corner(format!(
            "An der Ecke treffen {} Flächen zusammen; unterstützt sind drei",
            faces.len()
        )));
    }
    Ok(faces)
}

/// Parameterbreite, die auf der Kurve bei `s` etwa der Länge `len` entspricht.
fn param_window(curve: &Curve, s: f64, len: f64) -> f64 {
    match curve {
        Curve::Line { .. } => len,
        Curve::Circle { radius, .. } => (len / radius).min(PI),
        _ => {
            let speed = curve.tangent(s).length();
            if speed > 0.0 { len / speed } else { len }
        }
    }
}

/// Schnitt der Kurve mit der Fläche, der `s_ref` am nächsten liegt. Liegen zwei verschiedene
/// Schnitte gleich weit (innerhalb `ON_CURVE_FACTOR · tol.linear`) vom Kurvenpunkt bei `s_ref`
/// entfernt (etwa symmetrisch um die Ecke, wenn die Abschlussfläche eine Kantenfläche glatt
/// fortsetzt), ist die Wahl nicht eindeutig: `None` statt eines Zufallstreffers.
fn end_param(
    curve: &Curve,
    s_ref: f64,
    len: f64,
    surface: &Surface,
    tol: &Tolerance,
) -> Option<f64> {
    let w = param_window(curve, s_ref, len);
    let CurveSurface::Roots(rs) = curve_surface_roots(curve, s_ref - w, s_ref + w, surface, tol)
    else {
        return None;
    };
    let mut ts: Vec<f64> = rs.iter().map(|r| r.t).filter(|t| t.is_finite()).collect();
    ts.sort_by(|x, y| (x - s_ref).abs().total_cmp(&(y - s_ref).abs()));
    let best = *ts.first()?;
    let weld = ON_CURVE_FACTOR * tol.linear;
    let reference = curve.point(s_ref);
    let gap = |t: f64| curve.point(t).distance(reference);
    let ambiguous = ts.iter().skip(1).any(|&t| {
        (gap(t) - gap(best)).abs() <= weld && curve.point(t).distance(curve.point(best)) > weld
    });
    (!ambiguous).then_some(best)
}

/// Setzt die dritte Fläche `c` an der Ecke `vp` eine der Kantenflächen glatt fort (Normalen
/// fast parallel, wie `NEAR_TANGENT_ANGLE`)? Dann schneidet die Berührkurve diese Fläche
/// symmetrisch um die Ecke (oder gar nicht); ein Abschluss dort wäre geraten. Benannte Ablehnung;
/// weiter geht es dort nur als glatter Übergang, wenn die anschließende Kante mitgerundet wird
/// (Tangentenkette oder ausdrücklich gewählt, `smooth_pair`).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_transversal_end(
    solid: &Solid,
    g: &EdgeGeom,
    c: Id<Face>,
    vp: DVec3,
) -> Result<(), BlendError> {
    let nc = outward_normal(&solid.faces[c], vp);
    for side in [g.sides.a.face, g.sides.b.face] {
        let ns = outward_normal(&solid.faces[side], vp);
        // NaN (Normale nicht bestimmbar) zählt wie „glatt“.
        if !(nc.cross(ns).length() >= NEAR_TANGENT_ANGLE.sin()) {
            return Err(BlendError::Unsupported(format!(
                "Die Fläche „{}“ setzt an der Ecke die Fläche „{}“ glatt fort; dort lässt sich die Rundung oder Fase nicht abschließen (fortsetzen lässt sie sich nur zusammen mit einer glatt anschließenden Kante, etwa über „Tangentenkette“)",
                face_name(solid, c),
                face_name(solid, side)
            )));
        }
    }
    Ok(())
}

/// Stück einer Schnittkurve von `qa` nach `qb`; bei geschlossenen Kurven der Bogen, dessen
/// Mitte näher an `near` (der alten Ecke) liegt.
fn pick_arc(cs: &[Curve], qa: DVec3, qb: DVec3, near: DVec3, tol: &Tolerance) -> Option<Seg> {
    for c in cs {
        let ta = param_near(c, qa, 0.0);
        let tb = param_near(c, qb, ta);
        if !(on_curve(c, ta, qa, tol) && on_curve(c, tb, qb, tol)) {
            continue;
        }
        return Some(match c.period() {
            None => Seg::new(c.clone(), ta, tb),
            Some(per) => {
                let alt = if tb > ta { tb - per } else { tb + per };
                let gap = |t1: f64| Seg::new(c.clone(), ta, t1).at(0.5).distance(near);
                if gap(tb) <= gap(alt) {
                    Seg::new(c.clone(), ta, tb)
                } else {
                    Seg::new(c.clone(), ta, alt)
                }
            }
        });
    }
    None
}

/// Abschluss an einer Naht (geschlossene Kante): an beiden Enden derselbe Querschnitt.
pub fn seam_cut(g: &EdgeGeom, at_end: bool) -> EndCut {
    let s = if at_end { g.range.1 } else { g.range.0 };
    EndCut {
        sa: s,
        sb: s,
        curve: g.section_start.clone(),
        cap: None,
    }
}

/// Schnitt der Rundung `g` an ihrem Anfang bzw. Ende mit `surface`: Parameter der
/// Berührkurven und Abschlusskurve von A nach B (der Bogen näher an `near`).
pub fn cut_with(
    g: &EdgeGeom,
    at_end: bool,
    surface: &Surface,
    near: DVec3,
    tol: &Tolerance,
) -> Option<(f64, f64, Seg)> {
    let s_ref = if at_end { g.range.1 } else { g.range.0 };
    let len = END_SEARCH_FACTOR * (g.reach + tol.linear);
    let sa = end_param(&g.contact_a, s_ref, len, surface, tol)?;
    let sb = end_param(&g.contact_b, s_ref, len, surface, tol)?;
    let (qa, qb) = (g.contact_a.point(sa), g.contact_b.point(sb));
    let SurfaceIntersection::Curves(cs) = intersect_surfaces(&g.surface, surface, tol) else {
        return None;
    };
    let curve = pick_arc(&cs, qa, qb, near, tol)?;
    // Schutz gegen entartete Schnitte (etwa sehr steile Kegel): alles endlich, Kurve nicht leer.
    let finite = sa.is_finite()
        && sb.is_finite()
        && curve.t0.is_finite()
        && curve.t1.is_finite()
        && curve.start().is_finite()
        && curve.end().is_finite();
    (finite && curve.start().distance(curve.end()) > tol.linear).then_some((sa, sb, curve))
}

/// Abschluss an der dritten Fläche der Ecke (Spec §4.1 „Enden“). An der Ecke müssen genau drei
/// Flächen zusammentreffen.
pub fn cap_cut(
    solid: &Solid,
    g: &EdgeGeom,
    at_end: bool,
    tol: &Tolerance,
) -> Result<EndCut, BlendError> {
    let e = solid.edges.get(g.edge).ok_or(BlendError::EdgeNotFound)?;
    let v = if at_end { e.end } else { e.start };
    let faces = three_faces(solid, v)?;
    let others: Vec<Id<Face>> = faces
        .iter()
        .copied()
        .filter(|f| *f != g.sides.a.face && *f != g.sides.b.face)
        .collect();
    if others.len() != 1 {
        return Err(BlendError::Corner(
            "Die Kante grenzt an der Ecke nicht an zwei der drei Flächen".into(),
        ));
    }
    let c = others[0];
    let vp = solid.vertices[v].point;
    check_transversal_end(solid, g, c, vp)?;
    let (sa, sb, curve) =
        cut_with(g, at_end, &solid.faces[c].surface, vp, tol).ok_or_else(|| {
            BlendError::Unsupported(format!(
                "Die Rundung lässt sich an der Fläche „{}“ nicht abschließen",
                face_name(solid, c)
            ))
        })?;
    Ok(EndCut {
        sa,
        sb,
        curve,
        cap: Some((c, v)),
    })
}

/// Querschnitt als Abschluss am Anfang bzw. Ende (glatter Übergang, Task 9).
pub(crate) fn section_cut(g: &EdgeGeom, at_end: bool) -> EndCut {
    let s = if at_end { g.range.1 } else { g.range.0 };
    EndCut {
        sa: s,
        sb: s,
        curve: if at_end {
            g.section_end.clone()
        } else {
            g.section_start.clone()
        },
        cap: None,
    }
}

/// Fallen zwei Abschlusskurven zusammen (Enden in beliebiger Reihenfolge, Mitte)? NaN gilt als
/// „nicht“.
pub(crate) fn same_cut(x: &Seg, y: &Seg, weld: f64) -> bool {
    let (a, b, c, d) = (x.start(), x.end(), y.start(), y.end());
    ((a.distance(c) <= weld && b.distance(d) <= weld)
        || (a.distance(d) <= weld && b.distance(c) <= weld))
        && x.at(0.5).distance(y.at(0.5)) <= weld
}

/// Glatter Übergang an `v` zwischen den Enden `(i, ei)` und `(j, ej)`: beide Rundungsflächen
/// enden an ihrem Querschnitt, der derselbe sein muss. `None`, wenn die Kanten dort nicht glatt
/// anschließen (Richtungen nicht innerhalb `CHAIN_ANGLE` entgegengesetzt).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn smooth_pair(
    solid: &Solid,
    geoms: &[EdgeGeom],
    v: Id<Vertex>,
    (i, ei): (usize, bool),
    (j, ej): (usize, bool),
    tol: &Tolerance,
) -> Result<Option<(EndCut, EndCut)>, BlendError> {
    let (gi, gj) = (&geoms[i], &geoms[j]);
    let (di, dj) = (
        leaving_direction(solid, gi.edge, v),
        leaving_direction(solid, gj.edge, v),
    );
    // NaN oder Nullrichtung zählt als „nicht glatt“.
    if !(di.dot(dj) <= -CHAIN_ANGLE.cos()) {
        return Ok(None);
    }
    three_faces(solid, v)?;
    if gi.convex != gj.convex {
        return Err(BlendError::Unsupported(
            "Am glatten Übergang wechselt die Kante zwischen konvex und konkav".into(),
        ));
    }
    let (ci, cj) = (section_cut(gi, ei), section_cut(gj, ej));
    let weld = ON_CURVE_FACTOR * blend_tol(solid, geoms, tol).linear;
    if !same_cut(&ci.curve, &cj.curve, weld) {
        return Err(BlendError::Unsupported(
            "Die Rundung lässt sich am glatten Übergang nicht fortsetzen".into(),
        ));
    }
    Ok(Some((ci, cj)))
}

/// Zwei gerundete Kanten an einer Ecke: glatter Übergang, sonst Gehrung (Task 11).
fn vertex_pair(
    solid: &Solid,
    geoms: &[EdgeGeom],
    v: Id<Vertex>,
    x: (usize, bool),
    y: (usize, bool),
    tol: &Tolerance,
) -> Result<Vec<(usize, bool, EndCut)>, BlendError> {
    if let Some((cx, cy)) = smooth_pair(solid, geoms, v, x, y, tol)? {
        return Ok(vec![(x.0, x.1, cx), (y.0, y.1, cy)]);
    }
    super::corner::miter_pair(solid, geoms, v, x, y, tol)
}

/// Drei gerundete Kanten an einer Ecke: Kugelstück (Task 11); mehr: benannt abgelehnt.
fn corner_many(
    solid: &Solid,
    geoms: &[EdgeGeom],
    v: Id<Vertex>,
    list: &[(usize, bool)],
    plan: &mut EndPlan,
    tol: &Tolerance,
) -> Result<Vec<(usize, bool, EndCut)>, BlendError> {
    let index = plan.corners.len() + 1;
    let (cuts, piece, volume) = super::corner::corner_three(solid, geoms, v, list, index, tol)?;
    plan.corners.push(piece);
    plan.corner_volume += volume;
    plan.corner_scale += volume.abs();
    Ok(cuts)
}

/// Abschlüsse aller Kanten, je Ecke entschieden: geschlossene Kanten an ihrer Naht, eine
/// gerundete Kante an der Ecke → dritte Fläche, zwei → glatter Übergang (oder Ecke), drei → Ecke.
/// Die Ecken werden nach ihrer Id abgearbeitet (deterministisch).
pub fn end_cuts(solid: &Solid, geoms: &[EdgeGeom], tol: &Tolerance) -> Result<EndPlan, BlendError> {
    let n = geoms.len();
    let mut starts: Vec<Option<EndCut>> = vec![None; n];
    let mut ends: Vec<Option<EndCut>> = vec![None; n];
    let mut at: BTreeMap<Id<Vertex>, Vec<(usize, bool)>> = BTreeMap::new();
    for (i, g) in geoms.iter().enumerate() {
        if g.closed {
            starts[i] = Some(seam_cut(g, false));
            ends[i] = Some(seam_cut(g, true));
            continue;
        }
        let e = solid.edges.get(g.edge).ok_or(BlendError::EdgeNotFound)?;
        at.entry(e.start).or_default().push((i, false));
        at.entry(e.end).or_default().push((i, true));
    }
    let mut plan = EndPlan::default();
    for (v, list) in at {
        let cuts = match list.as_slice() {
            [(i, end)] => vec![(*i, *end, cap_cut(solid, &geoms[*i], *end, tol)?)],
            [x, y] => vertex_pair(solid, geoms, v, *x, *y, tol)?,
            _ => corner_many(solid, geoms, v, &list, &mut plan, tol)?,
        };
        for (i, end, cut) in cuts {
            if end {
                ends[i] = Some(cut);
            } else {
                starts[i] = Some(cut);
            }
        }
    }
    let missing = || BlendError::ResultInvalid("Ein Abschluss der Rundung fehlt".into());
    plan.starts = starts
        .into_iter()
        .map(|c| c.ok_or_else(missing))
        .collect::<Result<_, _>>()?;
    plan.ends = ends
        .into_iter()
        .map(|c| c.ok_or_else(missing))
        .collect::<Result<_, _>>()?;
    Ok(plan)
}

/// Kürzt das Stück am Ende (`at_end`) bzw. Anfang bis `p`. Liegt `p` nicht auf der Kurve:
/// `Unsupported`; bliebe vom Stück nichts übrig (oder kehrte es sich um): `TooLarge`.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn retarget(
    seg: &Seg,
    p: DVec3,
    at_end: bool,
    face: &str,
    tol: &Tolerance,
) -> Result<Seg, BlendError> {
    let old = if at_end { seg.t1 } else { seg.t0 };
    let t = param_near(&seg.curve, p, old);
    if !on_curve(&seg.curve, t, p, tol) {
        return Err(BlendError::Unsupported(format!(
            "Eine Kante der Fläche „{face}“ passt nicht zur Rundung"
        )));
    }
    let (t0, t1) = if at_end { (seg.t0, t) } else { (t, seg.t1) };
    let before = seg.t1 - seg.t0;
    let out = Seg::new(seg.curve.clone(), t0, t1);
    // Auch ein Rest unter der Verschweißweite fiele in `sew` zu einem Punkt zusammen.
    if !((t1 - t0) * before.signum() > MIN_TRIMMED_FRACTION * before.abs()
        && out.length() > ON_CURVE_FACTOR * tol.linear)
    {
        return Err(BlendError::TooLarge {
            face: face.to_string(),
        });
    }
    Ok(out)
}

/// Schließt eine Schleife nach dem Ersetzen: Nachbarn ersetzter Stücke werden bis an deren Enden
/// gekürzt; zwei ersetzte Nachbarn müssen sich schon treffen.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn reconnect(
    segs: &mut [Seg],
    replaced: &[bool],
    face: &str,
    tol: &Tolerance,
) -> Result<(), BlendError> {
    let n = segs.len();
    if n < 2 {
        return Ok(());
    }
    for k in 0..n {
        let prev = (k + n - 1) % n;
        match (replaced[prev], replaced[k]) {
            (false, true) => {
                let p = segs[k].start();
                segs[prev] = retarget(&segs[prev], p, true, face, tol)?;
            }
            (true, false) => {
                let p = segs[prev].end();
                segs[k] = retarget(&segs[k], p, false, face, tol)?;
            }
            (true, true) => {
                // NaN gilt als „treffen sich nicht“.
                let gap = segs[prev].end().distance(segs[k].start());
                if !(gap <= ON_CURVE_FACTOR * tol.linear) {
                    return Err(BlendError::Corner(format!(
                        "Die Rundungen treffen sich auf der Fläche „{face}“ nicht"
                    )));
                }
            }
            (false, false) => {}
        }
    }
    Ok(())
}

/// Setzt die Abschlusskurve `cap` an der Ecke `vp` in eine Schleife der Abschlussfläche ein und
/// kürzt die beiden Kanten an der Ecke bis zu ihren Enden.
///
/// Periodische Abschlussflächen (Zylinder, Kegel …): Die Ecke wird im Raum gesucht, die Naht der
/// Fläche spielt dafür keine Rolle. Ist aber eine der beiden Kanten an der Ecke selbst die Naht,
/// wird nur diese eine Verwendung gekürzt, die zweite nicht; dann sollte `sew` nicht schließen
/// oder `validate_with` ablehnen (`ResultInvalid`). Dieser Fall ist nicht getestet.
fn insert_cap(
    loops: &mut [Vec<Seg>],
    vp: DVec3,
    cap: &Seg,
    face: &str,
    tol: &Tolerance,
) -> Result<(), BlendError> {
    let weld = ON_CURVE_FACTOR * tol.linear;
    let on = |s: &Seg, p: DVec3, at_end: bool| {
        let t = param_near(&s.curve, p, if at_end { s.t1 } else { s.t0 });
        on_curve(&s.curve, t, p, tol)
    };
    for lp in loops.iter_mut() {
        let n = lp.len();
        for j in 0..n {
            let k = (j + 1) % n;
            if lp[j].end().distance(vp) > weld || lp[k].start().distance(vp) > weld {
                continue;
            }
            let oriented = if on(&lp[j], cap.start(), true) && on(&lp[k], cap.end(), false) {
                cap.clone()
            } else if on(&lp[j], cap.end(), true) && on(&lp[k], cap.start(), false) {
                cap.reversed()
            } else {
                return Err(BlendError::Unsupported(format!(
                    "Die Abschlusskurve passt nicht zur Fläche „{face}“"
                )));
            };
            lp[j] = retarget(&lp[j], oriented.start(), true, face, tol)?;
            lp[k] = retarget(&lp[k], oriented.end(), false, face, tol)?;
            lp.insert(j + 1, oriented);
            return Ok(());
        }
    }
    Err(BlendError::ResultInvalid(format!(
        "Die Ecke der Fläche „{face}“ wurde nicht gefunden"
    )))
}

/// Dreht eine Schleife so, dass sie Außenkontur der Fläche ist (gegen den Uhrzeigersinn um die
/// Außennormale).
pub fn orient_outer(surface: &Surface, same_sense: bool, segs: Vec<Seg>) -> Vec<Seg> {
    let poly = points_to_uv(surface, same_sense, &loop_points(&segs));
    if oriented_area(&poly, same_sense) < 0.0 {
        reverse_loop(&segs)
    } else {
        segs
    }
}

/// Dichte Punkte einer Schleife (für den Parameterbereich).
fn dense_points(lp: &[Seg]) -> Vec<DVec3> {
    let mut pts = Vec::with_capacity(lp.len() * PATCH_SAMPLES_PER_SEG);
    for s in lp {
        for i in 0..PATCH_SAMPLES_PER_SEG {
            pts.push(s.at(i as f64 / PATCH_SAMPLES_PER_SEG as f64));
        }
    }
    pts
}

/// Parameterbereich einer gekrümmten Fläche aus ihrer Schleife (Ebenen: keiner).
pub fn loop_patch(surface: &Surface, same_sense: bool, lp: &[Seg]) -> Option<UvRect> {
    if matches!(surface, Surface::Plane { .. }) {
        return None;
    }
    let poly = points_to_uv(surface, same_sense, &dense_points(lp));
    let mut r = UvRect {
        u0: f64::INFINITY,
        u1: f64::NEG_INFINITY,
        v0: f64::INFINITY,
        v1: f64::NEG_INFINITY,
    };
    for p in &poly {
        r.u0 = r.u0.min(p.x);
        r.u1 = r.u1.max(p.x);
        r.v0 = r.v0.min(p.y);
        r.v1 = r.v1.max(p.y);
    }
    poly.first().map(|_| r)
}

/// Alter Parameterbereich, in nicht geschlossenen Richtungen bis zu den neuen Schleifen erweitert
/// (P10).
fn grow_patch(
    surface: &Surface,
    same_sense: bool,
    old: Option<UvRect>,
    loops: &[Vec<Seg>],
) -> Option<UvRect> {
    let mut r = old?;
    for lp in loops {
        if let Some(p) = loop_patch(surface, same_sense, lp) {
            if surface.u_period().is_none() {
                r.u0 = r.u0.min(p.u0);
                r.u1 = r.u1.max(p.u1);
            }
            if surface.v_period().is_none() {
                r.v0 = r.v0.min(p.v0);
                r.v1 = r.v1.max(p.v1);
            }
        }
    }
    Some(r)
}

/// Ordnet lose Stücke zu einer geschlossenen Schleife (Stücke werden bei Bedarf umgedreht);
/// `None`, wenn sie sich nicht schließen.
pub fn chain_loop(mut segs: Vec<Seg>, weld: f64) -> Option<Vec<Seg>> {
    if segs.is_empty() {
        return None;
    }
    let mut out = vec![segs.remove(0)];
    while !segs.is_empty() {
        let end = out.last()?.end();
        let i = segs
            .iter()
            .position(|s| s.start().distance(end) <= weld || s.end().distance(end) <= weld)?;
        let s = segs.remove(i);
        out.push(if s.start().distance(end) <= weld {
            s
        } else {
            s.reversed()
        });
    }
    (out.last()?.end().distance(out[0].start()) <= weld).then_some(out)
}

/// Berührkurve von `s0` nach `s1` brauchbar? Sie muss in Kantenrichtung (Vorzeichen von
/// `range`) laufen und mehr als `MIN_TRIMMED_FRACTION` des Kantenbereichs behalten; sonst
/// überschneiden sich die Abschlüsse (Kante zu kurz für das Maß).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn contact_span_ok(range: (f64, f64), s0: f64, s1: f64) -> bool {
    let full = range.1 - range.0;
    (s1 - s0) * full.signum() > MIN_TRIMMED_FRACTION * full.abs()
}

/// Meldung für Löcher und Flächen, die in den Bereich der Rundung reichen.
fn in_blend_area(what: &str) -> BlendError {
    BlendError::Unsupported(format!(
        "{what} liegt im Bereich der Rundung; bitte ein kleineres Maß wählen"
    ))
}

/// Liegen alle Löcher einer umgebauten Fläche noch ganz innerhalb ihrer neuen Außenschleife, ohne
/// sie oder einander zu berühren? Sonst bliebe ein Loch (samt Wand) über der Rundung stehen oder
/// würde von der Berührkurve geschnitten: benannte Ablehnung statt eines falschen Körpers.
fn check_holes(
    surface: &Surface,
    same_sense: bool,
    loops: &[Vec<Seg>],
    face: &str,
    tol: &Tolerance,
) -> Result<(), BlendError> {
    let Some((outer, holes)) = loops.split_first() else {
        return Ok(());
    };
    let refuse = || in_blend_area(&format!("Ein Loch der Fläche „{face}“"));
    let shape = FaceGeom {
        surface: surface.clone(),
        same_sense,
        domain: None,
        loops: vec![outer.clone()],
        tag: None,
    };
    for (k, hole) in holes.iter().enumerate() {
        let inside = loop_points(hole)
            .into_iter()
            .chain(hole.iter().map(|s| s.at(0.5)))
            .all(|p| point_in_face(&shape, p, tol) == Containment::Inside);
        if !inside {
            return Err(refuse());
        }
        for other in std::iter::once(outer).chain(holes[k + 1..].iter()) {
            for a in hole {
                if other
                    .iter()
                    .any(|b| !seg_intersections(a, b, tol).is_empty())
                {
                    return Err(refuse());
                }
            }
        }
    }
    Ok(())
}

/// Kreuzt oder berührt ein neues oder gekürztes Stück einer umgebauten Fläche (Berührkurve,
/// Abschlusskurve, gekürzte Nachbarkante) irgendein anderes Stück ihrer Schleifen außerhalb einer
/// gemeinsamen Ecke? Dann liefe die Berührkurve etwa durch eine Aussparung der Fläche (Sackloch,
/// das die Wand anschneidet) und die Schleife durchdränge sich selbst: benannte Ablehnung statt
/// eines Körpers, der `validate` besteht, sich aber nicht vernetzen lässt. Genau über
/// `seg_intersections` (nicht über Stichproben); erlaubt sind nur Treffer an Punkten, die Ende
/// beider Stücke sind. Überlappungen zählen als Treffer. Unveränderte Stücke (gleiche Enden wie im
/// Eingangskörper) werden nur gegen veränderte geprüft; Hüllquader filtern sicher vor.
fn check_crossings(
    original: &[Vec<Seg>],
    loops: &[Vec<Seg>],
    face: &str,
    tol: &Tolerance,
) -> Result<(), BlendError> {
    let weld = ON_CURVE_FACTOR * tol.linear;
    let same = |a: &Seg, b: &Seg| a.start() == b.start() && a.end() == b.end();
    let segs: Vec<&Seg> = loops.iter().flatten().collect();
    let changed: Vec<bool> = segs
        .iter()
        .map(|s| !original.iter().flatten().any(|o| same(o, s)))
        .collect();
    let boxes: Vec<Option<Aabb>> = segs.iter().map(|s| seg_box(s)).collect();
    let ends = |s: &Seg| [s.start(), s.end()];
    for i in 0..segs.len() {
        if !changed[i] {
            continue;
        }
        for j in 0..segs.len() {
            if i == j || (changed[j] && j < i) {
                continue;
            }
            if let (Some(a), Some(b)) = (&boxes[i], &boxes[j])
                && apart(a, b, weld)
            {
                continue;
            }
            let (a, b) = (segs[i], segs[j]);
            let shared = |p: DVec3| {
                ends(a).iter().any(|q| q.distance(p) <= weld)
                    && ends(b).iter().any(|q| q.distance(p) <= weld)
            };
            if seg_intersections(a, b, tol)
                .iter()
                .any(|&(fa, _)| !shared(a.at(fa)))
            {
                return Err(in_blend_area(&format!(
                    "Eine Aussparung oder Kante der Fläche „{face}“"
                )));
            }
        }
    }
    Ok(())
}

/// Achsenparalleler Quader (kleinste, größte Ecke).
type Aabb = (DVec3, DVec3);

/// Komponentenweise Schranke für `|a cos t + b sin t|` über alle `t`: `sqrt(a_i² + b_i²)`.
fn swing(a: DVec3, b: DVec3) -> DVec3 {
    (a * a + b * b).map(f64::sqrt)
}

/// Sicherer Hüllquader eines Kurvenstücks: Gerade über ihre Enden, Kreis und Ellipse ganz
/// (unabhängig vom Parameterbereich). `None` bei Freiformkurven (dann kein Vorfilter).
fn seg_box(seg: &Seg) -> Option<Aabb> {
    let around = |c: DVec3, h: DVec3| Some((c - h, c + h));
    match &seg.curve {
        Curve::Line { .. } => {
            let (a, b) = (seg.start(), seg.end());
            Some((a.min(b), a.max(b)))
        }
        Curve::Circle { frame, radius } => {
            around(frame.origin, swing(frame.x, frame.y) * radius.abs())
        }
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => around(frame.origin, swing(frame.x * *major, frame.y * *minor)),
        Curve::Nurbs(_) => None,
    }
}

/// Sicherer Hüllquader der Rundungsfläche der Kante `g` mit den Abschlüssen `start` und `end`,
/// wo er sich ohne Annahmen über Abschlusskurven angeben lässt (sonst `None`, kein Vorfilter):
/// - Torus (Rundung an einer Kreiskante): der ganze Torus.
/// - Prisma (gerade Kante) mit Querschnitten an beiden Enden (glatter Übergang): die Fläche ist der
///   zwischen ihnen verschobene Querschnitt, liegt also im Quader beider Querschnitte.
/// - Drehfläche einer Fase (Kegel, Zylinder, Ebene um die Achse) mit Querschnitten oder Naht an
///   beiden Enden: der volle Umlauf des Querschnitts um die Achse (`frame.z`, Ursprung auf ihr);
///   Höhe und Abstand von der Achse sind längs der Querschnittsstrecke an ihren Enden extrem.
fn blend_box(g: &EdgeGeom, start: &EndCut, end: &EndCut) -> Option<Aabb> {
    let sections = start.cap.is_none() && end.cap.is_none();
    match (&g.surface, g.volume) {
        (
            Surface::Torus {
                frame,
                major,
                minor,
            },
            _,
        ) => {
            let h =
                swing(frame.x, frame.y) * (major.abs() + minor.abs()) + frame.z.abs() * minor.abs();
            Some((frame.origin - h, frame.origin + h))
        }
        (_, VolumeModel::Prism { .. }) if sections => {
            let (a0, a1) = seg_box(&start.curve)?;
            let (b0, b1) = seg_box(&end.curve)?;
            Some((a0.min(b0), a1.max(b1)))
        }
        (
            Surface::Cone { frame, .. }
            | Surface::Cylinder { frame, .. }
            | Surface::Plane { frame },
            VolumeModel::Pappus { .. },
        ) if sections && matches!(start.curve.curve, Curve::Line { .. }) => {
            let (o, z) = (frame.origin, frame.z);
            let (pa, pb) = (start.curve.start(), start.curve.end());
            let height = |p: DVec3| (p - o).dot(z);
            let rho = |p: DVec3| (p - o - z * height(p)).length();
            let (h0, h1) = (height(pa).min(height(pb)), height(pa).max(height(pb)));
            let r = rho(pa).max(rho(pb));
            let (lo, hi) = ((z * h0).min(z * h1), (z * h0).max(z * h1));
            let side = swing(frame.x, frame.y) * r;
            Some((o + lo - side, o + hi + side))
        }
        _ => None,
    }
}

/// Sicherer Hüllquader eines Eckstücks: Kugelstücke liegen in der ganzen Kugel; sonst keiner.
fn corner_box(piece: &Piece) -> Option<Aabb> {
    match &piece.surface {
        Surface::Sphere { frame, radius } => {
            let h = DVec3::splat(radius.abs());
            Some((frame.origin - h, frame.origin + h))
        }
        _ => None,
    }
}

/// Liegen die Quader mehr als `margin` auseinander? NaN gilt als „nicht“.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn apart(a: &Aabb, b: &Aabb, margin: f64) -> bool {
    let gap = (b.0 - a.1).max(a.0 - b.1);
    gap.x > margin || gap.y > margin || gap.z > margin
}

/// Reicht eine Kante der umgebauten Fläche `piece` ins Innere der neuen Rundungsfläche `blend`
/// (etwa die Wand eines Lochs oder einer Tasche im abgetragenen Bereich)? Treffer auf dem Rand der
/// Rundungsfläche (gekürzte Nachbarkanten, Berühr- und Abschlusskurven) zählen nicht.
///
/// Vorfilter: Ist `bound` ein sicherer Hüllquader der Rundungsfläche und liegt der sichere
/// Hüllquader eines Kurvenstücks weiter als die Verschweißweite davon entfernt, kann kein Punkt des
/// Stücks in der Fläche liegen; die Nullstellensuche entfällt dann.
fn check_piercing(
    piece: &Piece,
    blend: &Piece,
    bound: Option<&Aabb>,
    face: &str,
    tol: &Tolerance,
) -> Result<(), BlendError> {
    let margin = ON_CURVE_FACTOR * tol.linear;
    let shape = FaceGeom {
        surface: blend.surface.clone(),
        same_sense: blend.same_sense,
        domain: blend.domain,
        loops: vec![blend.outer.clone()],
        tag: None,
    };
    // Treffer im Inneren zählen immer; Treffer auf dem Rand der Rundungsfläche nur dann nicht,
    // wenn sie an einem Ende des Kurvenstücks liegen (gemeinsame Ecke). Ein Stück, das den Rand
    // mitten auf seinem Lauf berührt (etwa die Kante einer Aussparung, die die Berührkurve
    // tangential trifft), gilt als Durchdringung.
    let hit_at = |seg: &Seg, p: DVec3| match point_in_face(&shape, p, tol) {
        Containment::Inside => true,
        Containment::Boundary => seg.start().distance(p) > margin && seg.end().distance(p) > margin,
        Containment::Outside => false,
    };
    let inside = |p: DVec3| point_in_face(&shape, p, tol) == Containment::Inside;
    for seg in std::iter::once(&piece.outer)
        .chain(piece.holes.iter())
        .flatten()
    {
        if let (Some(b), Some(sb)) = (bound, seg_box(seg))
            && apart(b, &sb, margin)
        {
            continue;
        }
        let (lo, hi) = (seg.t0.min(seg.t1), seg.t0.max(seg.t1));
        let hit = match curve_surface_roots(&seg.curve, lo, hi, &blend.surface, tol) {
            CurveSurface::Roots(rs) => rs.iter().any(|r| hit_at(seg, seg.curve.point(r.t))),
            CurveSurface::Lies => inside(seg.at(0.5)),
        };
        if hit {
            return Err(in_blend_area(&format!("Die Fläche „{face}“")));
        }
    }
    Ok(())
}

/// Meldung für Körper mit mehreren Schalen (Hohlraum oder getrennte Teile).
pub const CAVITY: &str = "Der Körper hat mehrere Schalen (etwa einen Hohlraum); Rundungen und Fasen an solchen Körpern werden noch nicht unterstützt";

/// Hat der Körper mehr als eine Schale mit Flächen (Hohlraum oder getrennte Teile)? Eine solche
/// Schale berührt keine Fläche der Kante; ob sie im abgetragenen Bereich liegt oder die neue
/// Fläche schneidet, prüfen weder `check_holes` noch `check_piercing` sicher. Deshalb vorerst
/// immer abgelehnt (auch Hohlräume weit weg von der Kante). Keine Schalenliste gilt wie bei
/// `validate` als eine Schale. `blend` prüft das schon vor der Geometrie (billig zuerst).
pub(crate) fn has_cavity(solid: &Solid) -> bool {
    solid.shells.iter().filter(|s| !s.faces.is_empty()).count() > 1
}

/// Baut den neuen Körper: alle Flächen mit ersetzten und gekürzten Kanten, Abschlusskurven in den
/// Abschlussflächen, die Rundungsflächen (Rollen `roles`) und die Eckstücke des Plans; danach
/// `sew` und `validate_with`.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn assemble(
    solid: &Solid,
    geoms: &[EdgeGeom],
    plan: &EndPlan,
    roles: &[String],
    tol: &Tolerance,
) -> Result<Solid, BlendError> {
    if plan.starts.len() != geoms.len()
        || plan.ends.len() != geoms.len()
        || roles.len() != geoms.len()
    {
        return Err(BlendError::ResultInvalid(
            "Kanten, Abschlüsse und Flächennamen passen nicht zusammen".into(),
        ));
    }
    if has_cavity(solid) {
        return Err(BlendError::Unsupported(CAVITY.into()));
    }
    let btol = blend_tol(solid, geoms, tol);
    let weld = ON_CURVE_FACTOR * btol.linear;
    for cut in plan.starts.iter().chain(&plan.ends) {
        // NaN gilt als zu kurz.
        if !(cut.curve.length() > weld) {
            return Err(BlendError::InvalidParameter(format!(
                "Die Abschlusskurve ist nicht länger als {} mm; das Maß ist zu klein",
                super::show_num(weld)
            )));
        }
    }
    let mut repl: HashMap<(Id<Face>, usize, usize), Seg> = HashMap::new();
    for (i, g) in geoms.iter().enumerate() {
        let (s, e) = (&plan.starts[i], &plan.ends[i]);
        for (u, curve, s0, s1) in [
            (g.sides.a, &g.contact_a, s.sa, e.sa),
            (g.sides.b, &g.contact_b, s.sb, e.sb),
        ] {
            let seg = Seg::new(curve.clone(), s0, s1);
            if !(contact_span_ok(g.range, s0, s1) && seg.length() > weld) {
                return Err(BlendError::TooLarge {
                    face: face_name(solid, u.face),
                });
            }
            repl.insert(
                (u.face, u.loop_index, u.position),
                if u.forward { seg } else { seg.reversed() },
            );
        }
    }
    let mut caps: HashMap<Id<Face>, Vec<(Id<Vertex>, Seg)>> = HashMap::new();
    for cut in plan.starts.iter().chain(&plan.ends) {
        if let Some((f, v)) = cut.cap {
            caps.entry(f).or_default().push((v, cut.curve.clone()));
        }
    }
    let mut pieces: Vec<Piece> = Vec::with_capacity(solid.faces.len() + geoms.len());
    let mut names: Vec<String> = Vec::with_capacity(solid.faces.len());
    for (fid, face) in solid.faces.iter() {
        let g = face_geom(solid, fid);
        let name = face_name(solid, fid);
        let mut loops: Vec<Vec<Seg>> = Vec::with_capacity(g.loops.len());
        for (li, lp) in g.loops.iter().enumerate() {
            let replaced: Vec<bool> = (0..lp.len())
                .map(|k| repl.contains_key(&(fid, li, k)))
                .collect();
            let mut segs: Vec<Seg> = lp
                .iter()
                .enumerate()
                .map(|(k, s)| {
                    repl.get(&(fid, li, k))
                        .cloned()
                        .unwrap_or_else(|| s.clone())
                })
                .collect();
            reconnect(&mut segs, &replaced, &name, &btol)?;
            loops.push(segs);
        }
        for (v, cap) in caps.get(&fid).map(Vec::as_slice).unwrap_or(&[]) {
            let vp = solid
                .vertices
                .get(*v)
                .ok_or_else(|| {
                    BlendError::ResultInvalid(format!(
                        "Die Ecke der Fläche „{name}“ wurde nicht gefunden"
                    ))
                })?
                .point;
            insert_cap(&mut loops, vp, cap, &name, &btol)?;
        }
        let touched = caps.contains_key(&fid) || repl.keys().any(|(f, _, _)| *f == fid);
        if touched {
            check_crossings(&g.loops, &loops, &name, &btol)?;
            check_holes(&face.surface, face.same_sense, &loops, &name, &btol)?;
        }
        let domain = grow_patch(&face.surface, face.same_sense, face.patch, &loops);
        let mut it = loops.into_iter();
        let outer = it.next().unwrap_or_default();
        pieces.push(Piece {
            surface: face.surface.clone(),
            same_sense: face.same_sense,
            domain,
            outer,
            holes: it.collect(),
            tag: face.tag.clone(),
        });
        names.push(name);
    }
    let kept = pieces.len();
    for (i, g) in geoms.iter().enumerate() {
        let (s, e) = (&plan.starts[i], &plan.ends[i]);
        let segs = vec![
            Seg::new(g.contact_a.clone(), s.sa, e.sa),
            e.curve.clone(),
            Seg::new(g.contact_b.clone(), e.sb, s.sb),
            s.curve.reversed(),
        ];
        let outer = orient_outer(&g.surface, g.same_sense, segs);
        pieces.push(Piece {
            surface: g.surface.clone(),
            same_sense: g.same_sense,
            domain: loop_patch(&g.surface, g.same_sense, &outer),
            outer,
            holes: Vec::new(),
            tag: Some(FaceTag {
                origin: 0,
                role: roles[i].clone(),
            }),
        });
    }
    for (i, blend) in pieces[kept..].iter().enumerate() {
        let bound = blend_box(&geoms[i], &plan.starts[i], &plan.ends[i]);
        for (piece, name) in pieces[..kept].iter().zip(&names) {
            check_piercing(piece, blend, bound.as_ref(), name, &btol)?;
        }
    }
    // Eckstücke ebenso: Keine Kante des Eingangskörpers (Loch- oder Taschenwand) darf ins Innere
    // eines Kugelstücks reichen.
    for blend in &plan.corners {
        let bound = corner_box(blend);
        for (piece, name) in pieces[..kept].iter().zip(&names) {
            check_piercing(piece, blend, bound.as_ref(), name, &btol)?;
        }
    }
    pieces.extend(plan.corners.iter().cloned());
    let result = sew(pieces, tol).map_err(|e| BlendError::ResultInvalid(e.to_string()))?;
    validate_with(&result, tol).map_err(|e| BlendError::ResultInvalid(e.to_string()))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blend::analytic::SectionKind;
    use crate::blend::chain::select_edges;
    use crate::blend::section::ChamferSpec;
    use crate::blend::{BlendKind, BlendRequest};
    use crate::{AnalyticKernel, Kernel, Profile, ProfileLoop, Segment};
    use planora_math::{DVec2, Frame};

    /// Rasterweite der Stichproben je Richtung.
    const GRID: usize = 40;
    /// Spielraum der Enthaltenseins-Prüfung (Rundung der Auswertung).
    const SLACK: f64 = 1e-9;

    fn stadium() -> Solid {
        let profile = Profile {
            outer: ProfileLoop {
                start: DVec2::ZERO,
                segments: vec![
                    Segment::Line {
                        to: DVec2::new(10.0, 0.0),
                    },
                    Segment::Arc {
                        center: DVec2::new(10.0, 2.0),
                        sweep: PI,
                    },
                    Segment::Line {
                        to: DVec2::new(0.0, 4.0),
                    },
                    Segment::Arc {
                        center: DVec2::new(0.0, 2.0),
                        sweep: PI,
                    },
                ],
            },
            holes: vec![],
        };
        AnalyticKernel::default()
            .extrude(&profile, &Frame::WORLD, 3.0)
            .unwrap()
    }

    fn inside_box(b: &Aabb, p: DVec3) -> bool {
        p.cmpge(b.0 - SLACK).all() && p.cmple(b.1 + SLACK).all()
    }

    /// Geometrien und Abschlüsse der Tangentenkette ab `edge`.
    fn chain_plan(
        s: &Solid,
        edge: Id<crate::topology::Edge>,
        kind: SectionKind,
        bk: BlendKind,
    ) -> (Vec<EdgeGeom>, EndPlan) {
        let tol = Tolerance::default();
        let r = BlendRequest {
            edges: vec![crate::blend::BlendEdge {
                edge,
                first_face: None,
            }],
            kind: bk,
            tangent_chain: true,
        };
        let geoms: Vec<EdgeGeom> = select_edges(s, &r)
            .unwrap()
            .iter()
            .map(|c| super::super::edge_geom(s, &c.edge, kind, &tol).unwrap())
            .collect();
        let plan = end_cuts(s, &geoms, &tol).unwrap();
        (geoms, plan)
    }

    /// Prüft für jede Rundungsfläche mit Hüllquader, dass Stichproben der Fläche darin liegen;
    /// gibt die Zahl der Flächen mit Quader zurück.
    fn check_boxes(geoms: &[EdgeGeom], plan: &EndPlan) -> usize {
        let tol = Tolerance::default();
        let mut boxed = 0;
        for (i, g) in geoms.iter().enumerate() {
            let (s, e) = (&plan.starts[i], &plan.ends[i]);
            let Some(b) = blend_box(g, s, e) else {
                continue;
            };
            boxed += 1;
            let segs = vec![
                Seg::new(g.contact_a.clone(), s.sa, e.sa),
                e.curve.clone(),
                Seg::new(g.contact_b.clone(), e.sb, s.sb),
                s.curve.reversed(),
            ];
            let outer = orient_outer(&g.surface, g.same_sense, segs);
            for p in dense_points(&outer) {
                assert!(inside_box(&b, p), "Rand {i}: {p} nicht in {b:?}");
            }
            // Querschnitte zwischen den Abschlüssen (Prisma: verschoben, Drehfläche: gedreht).
            let mut hits = 0;
            if let Some(dom) = loop_patch(&g.surface, g.same_sense, &outer) {
                let shape = FaceGeom {
                    surface: g.surface.clone(),
                    same_sense: g.same_sense,
                    domain: Some(dom),
                    loops: vec![outer.clone()],
                    tag: None,
                };
                for a in 0..=GRID {
                    for c in 0..=GRID {
                        let u = dom.u0 + (dom.u1 - dom.u0) * a as f64 / GRID as f64;
                        let v = dom.v0 + (dom.v1 - dom.v0) * c as f64 / GRID as f64;
                        let p = g.surface.point(u, v);
                        if point_in_face(&shape, p, &tol) == Containment::Inside {
                            hits += 1;
                            assert!(inside_box(&b, p), "Fläche {i}: {p} nicht in {b:?}");
                        }
                    }
                }
            } else {
                for a in 0..=GRID {
                    for c in 0..=GRID {
                        let (f, t) = (a as f64 / GRID as f64, c as f64 / GRID as f64);
                        let p = s.curve.at(t).lerp(e.curve.at(t), f);
                        hits += 1;
                        assert!(inside_box(&b, p), "Ebene {i}: {p} nicht in {b:?}");
                    }
                }
            }
            assert!(hits > 0, "Fläche {i} ohne Stichprobe");
        }
        boxed
    }

    fn top_edge(s: &Solid) -> Id<crate::topology::Edge> {
        s.edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| {
                let e = &s.edges[id];
                matches!(e.curve, Curve::Line { .. })
                    && s.vertices[e.start].point.z > 2.5
                    && s.vertices[e.end].point.z > 2.5
            })
            .unwrap()
    }

    #[test]
    fn blend_boxes_contain_their_faces() {
        let s = stadium();
        let e = top_edge(&s);
        let fillet = (
            SectionKind::Fillet { radius: 0.5 },
            BlendKind::Fillet { radius: 0.5 },
        );
        let chamfer = (
            SectionKind::Chamfer(ChamferSpec::Distances { a: 0.3, b: 0.6 }),
            BlendKind::ChamferTwo {
                distance1: 0.3,
                distance2: 0.6,
            },
        );
        for (kind, bk) in [fillet, chamfer] {
            let (geoms, plan) = chain_plan(&s, e, kind, bk);
            assert_eq!(geoms.len(), 4);
            // Alle vier Flächen enden an Querschnitten: alle bekommen einen Quader.
            assert_eq!(check_boxes(&geoms, &plan), 4, "{kind:?}");
        }
        // Zylinderdeckel: Torus bzw. Kegel mit Naht.
        let cyl = AnalyticKernel::default().make_cylinder(5.0, 8.0).unwrap();
        let rim = cyl
            .edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| matches!(cyl.edges[id].curve, Curve::Circle { .. }))
            .unwrap();
        for (kind, bk) in [
            (
                SectionKind::Fillet { radius: 1.0 },
                BlendKind::Fillet { radius: 1.0 },
            ),
            (
                SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 0.5 }),
                BlendKind::ChamferTwo {
                    distance1: 1.0,
                    distance2: 0.5,
                },
            ),
        ] {
            let (geoms, plan) = chain_plan(&cyl, rim, kind, bk);
            assert_eq!(check_boxes(&geoms, &plan), 1, "{kind:?}");
        }
        // Würfelkante mit Abschlussflächen: kein Quader (Abschlusskurven ohne sichere Hülle).
        let cube = AnalyticKernel::default()
            .make_box(DVec3::splat(10.0))
            .unwrap();
        let (geoms, plan) = chain_plan(
            &cube,
            top_edge(&cube),
            SectionKind::Fillet { radius: 1.0 },
            BlendKind::Fillet { radius: 1.0 },
        );
        assert_eq!(check_boxes(&geoms, &plan), 0);
    }

    #[test]
    fn segment_boxes_contain_their_curves() {
        let frame = Frame {
            origin: DVec3::new(1.0, -2.0, 3.0),
            x: DVec3::new(0.6, 0.8, 0.0),
            y: DVec3::new(0.0, 0.0, 1.0),
            z: DVec3::new(0.8, -0.6, 0.0),
        };
        let segs = [
            Seg::new(
                Curve::Line {
                    origin: DVec3::new(1.0, 2.0, 3.0),
                    dir: DVec3::new(0.0, 0.6, -0.8),
                },
                -2.0,
                5.0,
            ),
            Seg::new(Curve::Circle { frame, radius: 2.5 }, 0.3, 2.0),
            Seg::new(
                Curve::Ellipse {
                    frame,
                    major: 3.0,
                    minor: 1.0,
                },
                -1.0,
                4.0,
            ),
        ];
        for seg in &segs {
            let b = seg_box(seg).unwrap();
            for k in 0..=200 {
                let p = seg.at(k as f64 / 200.0);
                assert!(inside_box(&b, p), "{p} nicht in {b:?}");
            }
        }
        assert!(apart(
            &(DVec3::ZERO, DVec3::ONE),
            &(DVec3::splat(2.0), DVec3::splat(3.0)),
            0.5
        ));
        assert!(!apart(
            &(DVec3::ZERO, DVec3::ONE),
            &(DVec3::splat(1.2), DVec3::splat(3.0)),
            0.5
        ));
        let nan = DVec3::splat(f64::NAN);
        assert!(!apart(&(nan, nan), &(DVec3::ZERO, DVec3::ONE), 0.5));
    }
}
