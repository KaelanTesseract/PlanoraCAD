//! Beschnitt sammeln: Für jede Fläche beider Körper die Schnittsegmente in ihrem Inneren.
//!
//! Jede Schnittkurve zweier Trägerflächen wird an Haltepunkten zerlegt (dort, wo Randkanten der
//! einen Fläche die andere Trägerfläche treffen; dazu Nahtkreuzungen, Pole und Kegelspitzen).
//! Ein Stück gehört zu einer Fläche, wenn es in ihrem Inneren und nicht außerhalb der anderen
//! Fläche liegt.

use super::breaks::{bbox, breakpoints, classify_seg, edge_hits, split_at};
use super::error::BooleanError;
use super::roots::seg_intersections;
use super::seg::Seg;
use super::split::seam_segments;
use super::surfsurf::{SurfaceIntersection, intersect_surfaces, surface_label, surface_name};
use super::uv::{Containment, FaceGeom, check_loops_on_surface, face_geom};
use super::{face_is_freeform, pipeline_tol};
use crate::geometry::{Curve, Surface};
use crate::topology::Solid;
use planora_i18n::tr;
use planora_math::{DVec3, Tolerance};

/// Bruchteile auf einer Randkante, die näher beieinander liegen, gelten als derselbe Teilungspunkt.
const SAME_FRACTION: f64 = 1e-7;

/// Eine Fläche samt der Schnittsegmente, die in ihrem Inneren liegen.
#[derive(Clone, Debug)]
pub struct FaceWork {
    pub geom: FaceGeom,
    pub sections: Vec<Seg>,
}

/// Eine Fläche mit ihren Nahtstücken (für die Lagebestimmung von Punkten auf der Naht).
struct Prepared {
    geom: FaceGeom,
    seams: Vec<Seg>,
    bbox: (DVec3, DVec3),
}

fn overlaps(a: &(DVec3, DVec3), b: &(DVec3, DVec3)) -> bool {
    a.0.x <= b.1.x
        && b.0.x <= a.1.x
        && a.0.y <= b.1.y
        && b.0.y <= a.1.y
        && a.0.z <= b.1.z
        && b.0.z <= a.1.z
}

fn is_closed_curve(c: &Curve) -> bool {
    c.period().is_some()
}

/// Bereits berechnete Kantentreffer (nur bei Freiformbeteiligung): Die Nullstellensuche gegen
/// eine Freiformfläche ist teuer, und jede Kante gehört zu zwei Flächen (die Naht sogar zweimal zu
/// derselben). Schlüssel: Seite und Index der getroffenen Fläche, dazu Enden und Mitte der Kante
/// (Richtung beliebig).
struct HitCache {
    entries: Vec<HitEntry>,
}

struct HitEntry {
    /// (Kante von A?, Index der getroffenen Fläche im anderen Körper)
    key: (bool, usize),
    start: DVec3,
    end: DVec3,
    mid: DVec3,
    hits: Vec<DVec3>,
}

impl HitCache {
    fn hits(
        &mut self,
        key: (bool, usize),
        e: &Seg,
        other: &Surface,
        tol: &Tolerance,
    ) -> Vec<DVec3> {
        let (a, b, m) = (e.start(), e.end(), e.at(0.5));
        let same = |x: DVec3, y: DVec3| x.distance(y) <= tol.linear;
        if let Some(entry) = self.entries.iter().find(|x| {
            x.key == key
                && ((same(x.start, a) && same(x.end, b)) || (same(x.start, b) && same(x.end, a)))
                && x.mid.distance(m) <= tol.linear * 4.0
        }) {
            return entry.hits.clone();
        }
        let hits = edge_hits(e, other, tol);
        self.entries.push(HitEntry {
            key,
            start: a,
            end: b,
            mid: m,
            hits: hits.clone(),
        });
        hits
    }
}

/// Kandidaten für Haltepunkte eines Flächenpaars (unabhängig von der Schnittkurve): Treffer der
/// Randkanten beider Flächen auf der jeweils anderen Trägerfläche, in der Reihenfolge der Kanten.
/// `cache` (nur mit Freiformbeteiligung) mit den Indizes `(i, j)` der Flächen in A und B.
fn pair_hits(
    fa: &FaceGeom,
    fb: &FaceGeom,
    tol: &Tolerance,
    mut cache: Option<(&mut HitCache, usize, usize)>,
) -> Vec<DVec3> {
    let mut out = Vec::new();
    for (own, other, a_edges) in [(fa, fb, true), (fb, fa, false)] {
        for e in own.loops.iter().flatten() {
            match cache.as_mut() {
                Some((c, i, j)) => {
                    // Kanten von A treffen Fläche j von B, Kanten von B Fläche i von A.
                    let key = (a_edges, if a_edges { *j } else { *i });
                    out.extend(c.hits(key, e, &other.surface, tol));
                }
                None => out.extend(edge_hits(e, &other.surface, tol)),
            }
        }
    }
    out
}

/// Lage eines Stücks zur Fläche (siehe [`classify_seg`]).
fn classify(face: &Prepared, seg: &Seg, tol: &Tolerance) -> Containment {
    classify_seg(&face.geom, &face.seams, seg, tol)
}

/// Zerlegt die Schnittkurve an den Haltepunkten und liefert die Stücke samt Lage in beiden
/// Flächen. Stücke kürzer als die Toleranz entfallen.
fn pieces(
    c: &Curve,
    ts: Vec<f64>,
    fa: &Prepared,
    fb: &Prepared,
    tol: &Tolerance,
) -> Vec<(Seg, Containment, Containment)> {
    split_at(c, ts, tol)
        .into_iter()
        .map(|seg| {
            let (ca, cb) = (classify(fa, &seg, tol), classify(fb, &seg, tol));
            (seg, ca, cb)
        })
        .collect()
}

/// Ist das Stück eine geschlossene Kurve (Vollkreis/Vollellipse, Anfang = Ende)?
fn is_closed_seg(s: &Seg, tol: &Tolerance) -> bool {
    is_closed_curve(&s.curve) && s.start().distance(s.end()) <= tol.linear
}

/// Randstücke von `own`, die im Inneren von `target` liegen (beide auf derselben Trägerfläche).
///
/// Nähte von `own` sind keine Beschnittkanten und liefern nichts. Geschlossene Randkanten
/// (Vollkreise) werden nur an echten Schnittpunkten geteilt, nicht an ihrem willkürlichen
/// Anfangspunkt: so entsteht dasselbe Stück wie über den Kurvenschnitt mit der Nachbarfläche.
fn coincident_sections(own: &Prepared, target: &Prepared, tol: &Tolerance) -> Vec<Seg> {
    let is_seam = |e: &Seg| {
        own.seams
            .iter()
            .any(|m| m.closest(e.at(0.5)).1 <= tol.linear * 4.0)
    };
    let mut out = Vec::new();
    for e in own.geom.loops.iter().flatten().filter(|e| !is_seam(e)) {
        let closed = is_closed_seg(e, tol);
        let mut fs: Vec<f64> = Vec::new();
        for t in target.geom.loops.iter().flatten() {
            for (fe, _) in seg_intersections(e, t, tol) {
                // bei geschlossenen Kanten ist 1 dasselbe wie 0
                fs.push(if closed && fe >= 1.0 - SAME_FRACTION {
                    0.0
                } else {
                    fe
                });
            }
        }
        if !closed {
            fs.extend([0.0, 1.0]);
        }
        fs.sort_by(|x, y| x.total_cmp(y));
        fs.dedup_by(|a, b| (*a - *b).abs() < SAME_FRACTION);
        let mut spans: Vec<(f64, f64)> = fs.windows(2).map(|w| (w[0], w[1])).collect();
        if closed {
            match (fs.first(), fs.last()) {
                (Some(&first), Some(&last)) => spans.push((last, first + 1.0)),
                _ => spans.push((0.0, 1.0)),
            }
        }
        for (f0, f1) in spans {
            let piece = e.sub(f0, f1);
            if piece.length() <= tol.linear {
                continue;
            }
            if classify(target, &piece, tol) == Containment::Inside {
                out.push(piece);
            }
        }
    }
    out
}

/// Dasselbe Stück (gleiche Enden in beliebiger Richtung, gleiche Mitte)?
fn same_piece(a: &Seg, b: &Seg, tol: &Tolerance) -> bool {
    let ends = (a.start().distance(b.start()) <= tol.linear
        && a.end().distance(b.end()) <= tol.linear)
        || (a.start().distance(b.end()) <= tol.linear && a.end().distance(b.start()) <= tol.linear);
    ends && a.at(0.5).distance(b.at(0.5)) <= tol.linear * 4.0
}

fn push_unique(sections: &mut Vec<Seg>, s: Seg, tol: &Tolerance) {
    if !sections.iter().any(|o| same_piece(o, &s, tol)) {
        sections.push(s);
    }
}

/// Flächen samt Nähten und Hüllquader. Freiformflächen, deren Randkanten nicht auf ihnen liegen,
/// ergeben eine benannte Meldung.
fn prepare(solid: &Solid, tol: &Tolerance) -> Result<Vec<Prepared>, BooleanError> {
    solid
        .faces
        .iter()
        .map(|(id, _)| {
            let geom = face_geom(solid, id);
            check_loops_on_surface(&geom, tol)?;
            let boundary: Vec<Seg> = geom.loops.iter().flatten().cloned().collect();
            let seams = seam_segments(
                &geom,
                &boundary,
                &pipeline_tol(tol, face_is_freeform(&geom)),
            );
            let bbox = bbox(&geom, tol);
            Ok(Prepared { geom, seams, bbox })
        })
        .collect()
}

/// Berechnet für jede Fläche beider Körper die Schnittsegmente in ihrem Inneren.
///
/// Die Listen folgen der Reihenfolge der Flächen im jeweiligen Körper. Ein Flächenpaar, dessen
/// Schnitt nicht unterstützt wird und dessen Hüllquader sich überlappen, liefert
/// [`BooleanError::UnsupportedIntersection`] (analytische Paare ohne Rechenweg) bzw.
/// [`BooleanError::Unsupported`] mit der Ursache des numerischen Schnittrechners. Ist eine
/// Freiformfläche oder -kurve beteiligt, gilt für Haltepunkte, Stücke und Lage die Toleranz
/// [`pipeline_tol`] (Schnittkurven liegen nur auf `NURBS_FIT_TOL` genau), und eine Randkante
/// neben ihrer Freiformfläche ergibt [`BooleanError::Unsupported`]. Doppelte Schnittstücke
/// einer Fläche (etwa von einer zusammenfallenden Fläche und einer anstoßenden Seitenfläche)
/// werden nur einmal geführt.
pub fn imprint(
    a: &Solid,
    b: &Solid,
    tol: &Tolerance,
) -> Result<(Vec<FaceWork>, Vec<FaceWork>), BooleanError> {
    let pa = prepare(a, tol)?;
    let pb = prepare(b, tol)?;
    let mut sa: Vec<Vec<Seg>> = vec![Vec::new(); pa.len()];
    let mut sb: Vec<Vec<Seg>> = vec![Vec::new(); pb.len()];
    let mut cache = HitCache {
        entries: Vec::new(),
    };

    for (i, fa) in pa.iter().enumerate() {
        for (j, fb) in pb.iter().enumerate() {
            if !overlaps(&fa.bbox, &fb.bbox) {
                continue;
            }
            super::budget::check()?;
            let (ga, gb) = (&fa.geom, &fb.geom);
            let unsupported = || BooleanError::UnsupportedIntersection {
                a: surface_name(&ga.surface),
                b: surface_name(&gb.surface),
            };
            // Der Schnitt selbst rechnet mit der Modelltoleranz; Haltepunkte, Stücke und ihre Lage
            // bei Freiformbeteiligung mit der weiteren Toleranz.
            let freeform = face_is_freeform(ga) || face_is_freeform(gb);
            let ptol = pipeline_tol(tol, freeform);
            let tol_p = &ptol;
            match intersect_surfaces(&ga.surface, &gb.surface, tol) {
                SurfaceIntersection::None | SurfaceIntersection::Touch => {}
                SurfaceIntersection::Unsupported => return Err(unsupported()),
                SurfaceIntersection::UnsupportedBecause(msg) => {
                    let text = tr!(
                        "kernel-boolean-intersection-because",
                        a = surface_label(surface_name(&ga.surface)),
                        b = surface_label(surface_name(&gb.surface)),
                        msg = msg
                    );
                    // Nur Wege mit Freiformflächen kommen hierher (der Schnittrechner braucht
                    // mindestens eine); sie gelten als Freiform-Grenze. Der Zweig `Unsupported`
                    // ist nur Absicherung und wird nie erreicht.
                    return Err(if freeform {
                        BooleanError::FreeformLimit(text)
                    } else {
                        BooleanError::Unsupported(text)
                    });
                }
                SurfaceIntersection::Coincident => {
                    for s in coincident_sections(fb, fa, tol_p) {
                        push_unique(&mut sa[i], s, tol_p);
                    }
                    for s in coincident_sections(fa, fb, tol_p) {
                        push_unique(&mut sb[j], s, tol_p);
                    }
                }
                SurfaceIntersection::Curves(cs) => {
                    super::budget::check()?;
                    let cache = freeform.then_some((&mut cache, i, j));
                    let hits = pair_hits(ga, gb, tol_p, cache);
                    for c in cs {
                        let ts = breakpoints(&c, &hits, &[&ga.surface, &gb.surface], tol_p);
                        for (seg, ca, cb) in pieces(&c, ts, fa, fb, tol_p) {
                            if ca == Containment::Inside && cb != Containment::Outside {
                                push_unique(&mut sa[i], seg.clone(), tol_p);
                            }
                            if cb == Containment::Inside && ca != Containment::Outside {
                                push_unique(&mut sb[j], seg, tol_p);
                            }
                        }
                    }
                }
            }
        }
    }
    let work = |p: Vec<Prepared>, s: Vec<Vec<Seg>>| -> Vec<FaceWork> {
        p.into_iter()
            .zip(s)
            .map(|(p, sections)| FaceWork {
                geom: p.geom,
                sections,
            })
            .collect()
    };
    Ok((work(pa, sa), work(pb, sb)))
}
