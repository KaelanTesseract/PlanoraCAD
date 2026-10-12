//! Einordnen von Punkten und Flächenstücken relativ zu einem Körper (innen, außen, bündig).
//!
//! Punkte werden mit dem Strahlverfahren eingeordnet: Ein Strahl in einer festen, „unrunden“
//! Richtung zählt die Durchstoßpunkte durch die Flächen des Körpers (ungerade = innen). Trifft
//! der Strahl eine Kante, eine Naht oder berührt er eine Fläche nur, ist das Ergebnis mehrdeutig
//! und es wird die nächste Richtung versucht.

use super::error::BooleanError;
use super::roots::{CurveSurface, curve_surface_roots};
use super::seg::Seg;
use super::seg_is_freeform;
use super::split::Region;
use super::uv::{
    Containment, FaceGeom, face_geom, face_polys, loop_points, point_in_face, point_in_face_at,
    points_to_uv, samples, winding,
};
use crate::geometry::{Curve, Surface};
use crate::ids::Id;
use crate::nurbs_surface::{NurbsSurfaceData, RAY_GRAZING_COS};
use crate::topology::{Edge, Face, Solid};
use crate::validate::point_surface_tol;
use planora_math::{DVec2, DVec3, Tolerance};
use std::cell::OnceCell;

/// Lage eines Flächenstücks relativ zum anderen Körper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Inside,
    Outside,
    /// Auf einer Fläche des anderen Körpers, gleiche Außenrichtung.
    OnSame,
    /// Auf einer Fläche des anderen Körpers, entgegengesetzte Außenrichtung.
    OnOpposite,
}

/// Lage eines Punkts relativ zu einem Körper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Inside,
    Outside,
    OnBoundary,
}

/// Geometrie aller Flächen eines Körpers.
pub fn solid_faces(s: &Solid) -> Vec<FaceGeom> {
    s.faces.iter().map(|(id, _)| face_geom(s, id)).collect()
}

/// Weitere feste Richtungen, die gegen Körper mit Freiformfläche nach den sechs gewöhnlichen
/// versucht werden (dort sind wegen Randband, Polen und Streifschüssen mehr Strahlen mehrdeutig;
/// rein analytische Körper nutzen nur die sechs, unverändert).
fn more_directions() -> [DVec3; 6] {
    [
        DVec3::new(0.3187, 0.8714, -0.3729),
        DVec3::new(-0.8236, -0.4417, -0.3555),
        DVec3::new(0.5521, -0.6639, 0.5043),
        DVec3::new(-0.2771, -0.1913, -0.9416),
        DVec3::new(0.6894, 0.6181, 0.3778),
        DVec3::new(-0.7149, 0.5338, -0.4517),
    ]
    .map(|d| d.normalize())
}

/// Feste, „unrunde“ Strahlrichtungen für das Strahlverfahren.
fn directions() -> [DVec3; 6] {
    [
        DVec3::new(0.7321, 0.4123, 0.5471),
        DVec3::new(-0.3911, 0.8312, 0.3977),
        DVec3::new(0.2214, -0.5731, 0.7893),
        DVec3::new(-0.6123, -0.2987, 0.7311),
        DVec3::new(0.9120, -0.2210, -0.3412),
        DVec3::new(-0.1731, 0.3490, -0.9205),
    ]
    .map(|d| d.normalize())
}

fn outward_normal(g: &FaceGeom, p: DVec3) -> DVec3 {
    let (u, v) = g.surface.project(p);
    g.surface.normal(u, v) * if g.same_sense { 1.0 } else { -1.0 }
}

/// Liegt `p` auf einer Fläche von `other`? Liefert die Außennormale dieser Fläche.
fn on_surface(other: &[FaceGeom], p: DVec3, tol: &Tolerance) -> Option<DVec3> {
    on_surface_face(other, p, tol).map(|g| outward_normal(g, p))
}

/// Die Fläche von `other`, auf der `p` liegt (siehe [`on_surface`]).
fn on_surface_face<'a>(other: &'a [FaceGeom], p: DVec3, tol: &Tolerance) -> Option<&'a FaceGeom> {
    other.iter().find(|g| {
        g.surface.implicit(p).abs() <= point_surface_tol(&g.surface, tol)
            && point_in_face(g, p, tol) != Containment::Outside
    })
}

/// Strahllänge, die sicher aus dem Körper hinausreicht (von `p` aus gemessen).
fn reach(other: &[FaceGeom], p: DVec3) -> f64 {
    other
        .iter()
        .flat_map(|g| g.loops.iter().flat_map(|l| loop_points(l)))
        .map(|q| q.distance(p))
        .fold(0.0_f64, f64::max)
        * 2.0
        + 1.0
}

/// Sicherheitsfaktor auf den Sehnenpfeil: Die Randpolygone entstehen im Parameterraum, dessen
/// Abbildung den Pfeil verzerren kann.
const SAG_SAFETY: f64 = 2.0;

/// Größter Abstand zwischen einem Randstück und dem Sehnenpolygon, mit dem `point_in_face` es
/// annähert (Bögen: `r·(1 − cos(Δt/2))` bei der Schrittweite `Δt` der Abtastung, für Ellipsen mit
/// der großen Halbachse als obere Schranke; Geraden: 0; Freiformkurven: gemessen, siehe
/// [`nurbs_chord_sag`]).
fn chord_sag(s: &Seg) -> f64 {
    let r = match &s.curve {
        Curve::Line { .. } => return 0.0,
        Curve::Circle { radius, .. } => radius.abs(),
        Curve::Ellipse { major, minor, .. } => major.abs().max(minor.abs()),
        Curve::Nurbs(_) => return nurbs_chord_sag(s),
    };
    let half_step = (s.t1 - s.t0).abs() / samples(s) as f64 / 2.0;
    r * (1.0 - half_step.min(std::f64::consts::PI).cos())
}

/// Probestellen je Sehne bei der Messung des Sehnenpfeils einer Freiformkurve.
const SAG_PROBES_PER_CHORD: usize = 4;

/// Sehnenpfeil einer Freiformkurve: größter Abstand von Kurvenpunkten zwischen den Abtaststellen
/// (je Sehne `SAG_PROBES_PER_CHORD` innere Proben) zur jeweiligen Sehne. Eine Messung, keine
/// strenge Schranke; `SAG_SAFETY` deckt den Rest ab.
fn nurbs_chord_sag(s: &Seg) -> f64 {
    let n = samples(s);
    let mut sag = 0.0_f64;
    for k in 0..n {
        let (f0, f1) = (k as f64 / n as f64, (k + 1) as f64 / n as f64);
        let (a, b) = (s.at(f0), s.at(f1));
        let ab = b - a;
        let len2 = ab.length_squared();
        for j in 1..=SAG_PROBES_PER_CHORD {
            let q = s.at(f0 + (f1 - f0) * j as f64 / (SAG_PROBES_PER_CHORD + 1) as f64);
            let t = if len2 > 0.0 {
                ((q - a).dot(ab) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            sag = sag.max(q.distance(a + ab * t));
        }
    }
    sag
}

/// Liegt `p` so nah am Rand der Fläche, dass das Randpolygon von `point_in_face` hier nicht
/// verlässlich zwischen innen und außen unterscheidet (Toleranz plus Sehnenpfeil)?
fn near_boundary(g: &FaceGeom, p: DVec3, tol: &Tolerance) -> bool {
    g.loops
        .iter()
        .flatten()
        .any(|s| s.closest(p).1 <= tol.linear + SAG_SAFETY * chord_sag(s))
}

/// Breite des Bands um die Randkanten (und Pole) einer Freiformfläche, als Bruchteil der
/// Strahllänge und zugleich der Diagonale der Fläche (das Kleinere zählt): Ein Treffer darin ist
/// unsicher (Randpolygone im Parameterraum, deren Sehnenpfeil bei 5°-Abtastung unter
/// `1e-3 ·` Krümmungsradius bleibt, und die Rückrechnung), es wird die nächste Richtung versucht.
/// Nur auf die Strahllänge bezogen wuchs das Band mit dem ganzen Körper: Bei einer Platte von
/// 1000 mm mit einer Bohrung r = 2 lag jeder Treffer auf der Bohrung im Band.
const FREEFORM_BOUNDARY_BAND: f64 = 0.01;

/// Untergrenze des Bands (mm): die Knotentoleranz der Pipeline mit Freiformbeteiligung.
const FREEFORM_BAND_FLOOR: f64 = super::NURBS_NODE_TOL;

/// Je Freiformfläche des anderen Körpers einmal je Einordnung berechnet: Randpolygone im
/// Parameterraum und die Diagonale des Hüllquaders ihrer Randpunkte.
struct FreeformFace {
    polys: Vec<Vec<DVec2>>,
    size: f64,
}

impl FreeformFace {
    fn new(g: &FaceGeom) -> Self {
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for q in g.loops.iter().flat_map(|l| loop_points(l)) {
            lo = lo.min(q);
            hi = hi.max(q);
        }
        let size = if lo.x <= hi.x {
            (hi - lo).length()
        } else {
            0.0
        };
        FreeformFace {
            polys: face_polys(g),
            size,
        }
    }

    /// Bandbreite für einen Strahl der Länge `reach`.
    fn band(&self, reach: f64) -> f64 {
        (FREEFORM_BOUNDARY_BAND * reach.min(self.size)).max(FREEFORM_BAND_FLOOR)
    }
}

/// Liegt der Treffer `hit` auf der Freiformfläche `g` im Band `band` um eine Randkante?
fn near_freeform_boundary(g: &FaceGeom, hit: DVec3, band: f64) -> bool {
    g.loops.iter().flatten().any(|s| s.closest(hit).1 <= band)
}

/// Läuft der Strahl `p + t·dir` im Band `band` an einem Pol vorbei, und zwar vorwärts
/// (nächste Annäherung bei `0 < t ≤ reach`)? Ein Treffer am Pol kann der Strahlrechnung entgehen
/// (siehe `ray_hits_checked`); dann soll das Band greifen, auch wenn kein Treffer gefunden wurde.
/// Ein Strahl, der von einem Startpunkt im Band vom Pol weg läuft, zählt nicht (Runde 1 begrenzte
/// `t` auf `[0, reach]`: dann galt von einem Punkt nahe am Pol jede Richtung als ungeeignet).
fn passes_a_pole(d: &NurbsSurfaceData, p: DVec3, dir: DVec3, reach: f64, band: f64) -> bool {
    let u0 = d.surface().domain().0.0;
    d.poles_v().iter().any(|pv| {
        let q = d.point(u0, *pv);
        let t = (q - p).dot(dir);
        t > 0.0 && t <= reach && q.distance(p + dir * t) <= band
    })
}

/// Strahlverfahren: `Some(true)` = innen, `Some(false)` = außen, `None` = mehrdeutig
/// (Treffer auf oder dicht an einem Rand oder einer Naht, Berührung, Strahl liegt in einer Fläche).
///
/// Freiformflächen: Strahlschnitt über [`NurbsSurfaceData::ray_hits_checked`]; unsichere Blätter,
/// Streifschüsse (`|dir · n| <` [`RAY_GRAZING_COS`]), ein Strahl im Band an einem Pol vorbei und
/// Treffer im Randband [`near_freeform_boundary`] machen den Strahl mehrdeutig. `cache` hält je
/// Freiformfläche Randpolygone und Größe (beim ersten Gebrauch berechnet).
fn cast(
    other: &[FaceGeom],
    cache: &[OnceCell<FreeformFace>],
    p: DVec3,
    dir: DVec3,
    reach: f64,
    tol: &Tolerance,
) -> Option<bool> {
    let ray = Curve::Line { origin: p, dir };
    let mut hits = 0;
    for (i, g) in other.iter().enumerate() {
        if let Surface::Nurbs(d) = &g.surface {
            let info = cache[i].get_or_init(|| FreeformFace::new(g));
            let band = info.band(reach);
            if passes_a_pole(d, p, dir, reach, band) {
                return None;
            }
            for (t, u, v) in d.ray_hits_checked(p, dir, reach)? {
                let hit = p + dir * t;
                if t <= tol.linear {
                    // Der Startpunkt liegt auf der Fläche, ohne dass `on_surface` ihn erkannt
                    // hat (etwa dicht an einem Pol, wo die Rückrechnung von `u` unsicher ist):
                    // Liegt der Treffer in der Fläche, ist die Zählung unsicher; außerhalb der
                    // beschnittenen Fläche zählt er wie bisher nicht.
                    if point_in_face_at(g, &info.polys, hit, (u, v), tol) != Containment::Outside {
                        return None;
                    }
                    continue;
                }
                let n = d.normal(u, v);
                if n == DVec3::ZERO || (dir.dot(n) / n.length()).abs() < RAY_GRAZING_COS {
                    return None;
                }
                if near_freeform_boundary(g, hit, band) {
                    return None;
                }
                match point_in_face_at(g, &info.polys, hit, (u, v), tol) {
                    Containment::Boundary => return None,
                    Containment::Inside => hits += 1,
                    Containment::Outside => {}
                }
            }
            continue;
        }
        match curve_surface_roots(&ray, 0.0, reach, &g.surface, tol) {
            CurveSurface::Lies => return None,
            CurveSurface::Roots(rs) => {
                for r in rs {
                    if r.t <= tol.linear {
                        continue;
                    }
                    let hit = p + dir * r.t;
                    // Treffer im Pfeilband eines Randbogens: innen/außen unsicher, nächste Richtung.
                    if near_boundary(g, hit, tol) {
                        return None;
                    }
                    match point_in_face(g, hit, tol) {
                        Containment::Boundary => return None,
                        Containment::Inside => {
                            if r.touching {
                                return None;
                            }
                            hits += 1;
                        }
                        Containment::Outside => {}
                    }
                }
            }
        }
    }
    Some(hits % 2 == 1)
}

/// Meldung, wenn gegen einen Körper mit Freiformfläche keine zwei Strahlrichtungen eindeutig sind.
pub(crate) const FREEFORM_GRAZING: &str =
    "Die Lage des Flächenstücks zum Körper ist nicht eindeutig bestimmbar (Streifschuss)";

/// Meldung, wenn sich zwei eindeutige Strahlrichtungen gegen einen Körper mit Freiformfläche
/// widersprechen (ein Treffer wurde übersehen oder doppelt gezählt).
pub(crate) const FREEFORM_RAYS_DISAGREE: &str = "Die Lage des Flächenstücks zum Körper ist nicht eindeutig bestimmbar (die Teststrahlen widersprechen sich)";

/// Strahlverfahren mit allen festen Richtungen, bis eine eindeutig ist.
///
/// Hat der Körper eine Freiformfläche, werden nach den sechs gewöhnlichen noch sechs weitere
/// Richtungen versucht, und **zwei** eindeutige Richtungen müssen dasselbe ergeben
/// (Gegenprobe gegen einen übersehenen Treffer der numerischen Strahlrechnung); widersprechen sie
/// sich: [`FREEFORM_RAYS_DISAGREE`]; gibt es keine zwei eindeutigen: [`FREEFORM_GRAZING`].
/// Rein analytische Körper: unverändert die erste eindeutige Richtung.
fn cast_all(other: &[FaceGeom], p: DVec3, tol: &Tolerance) -> Result<bool, BooleanError> {
    let reach = reach(other, p);
    let cache: Vec<OnceCell<FreeformFace>> = other.iter().map(|_| OnceCell::new()).collect();
    if !other.iter().any(|g| matches!(g.surface, Surface::Nurbs(_))) {
        return directions()
            .into_iter()
            .find_map(|d| cast(other, &cache, p, d, reach, tol))
            .ok_or(BooleanError::AmbiguousClassification);
    }
    let mut first: Option<bool> = None;
    for d in directions().into_iter().chain(more_directions()) {
        let Some(inside) = cast(other, &cache, p, d, reach, tol) else {
            continue;
        };
        match first {
            None => first = Some(inside),
            Some(f) if f == inside => return Ok(inside),
            Some(_) => return Err(BooleanError::Unsupported(FREEFORM_RAYS_DISAGREE.into())),
        }
    }
    Err(BooleanError::Unsupported(FREEFORM_GRAZING.into()))
}

/// Lage eines Punkts relativ zu einem Körper (gegeben durch seine Flächen).
pub fn solid_contains(
    other: &[FaceGeom],
    p: DVec3,
    tol: &Tolerance,
) -> Result<Location, BooleanError> {
    if !p.is_finite() {
        return Err(BooleanError::InvalidInput(
            "Der Punkt hat keine endlichen Koordinaten".into(),
        ));
    }
    if on_surface(other, p, tol).is_some() {
        return Ok(Location::OnBoundary);
    }
    Ok(if cast_all(other, p, tol)? {
        Location::Inside
    } else {
        Location::Outside
    })
}

/// Verschiebungen um eine Periode, mit denen ein Punkt gegen aufgerollte Polygone geprüft wird.
fn shifts(surface: &Surface) -> Vec<DVec2> {
    let around = |period: Option<f64>| match period {
        Some(t) => vec![-t, 0.0, t],
        None => vec![0.0],
    };
    let (us, vs) = (around(surface.u_period()), around(surface.v_period()));
    us.iter()
        .flat_map(|&du| vs.iter().map(move |&dv| DVec2::new(du, dv)))
        .collect()
}

/// Ein Punkt im Inneren der Region mit möglichst großem Abstand zum Rand (auch zu Nähten, die
/// keine echten Kanten sind: dort wären Randtests mehrdeutig).
fn interior_point(face: &FaceGeom, region: &Region, tol: &Tolerance) -> Option<DVec3> {
    let outer = points_to_uv(&face.surface, face.same_sense, &loop_points(&region.outer));
    let holes: Vec<Vec<DVec2>> = region
        .holes
        .iter()
        .map(|h| points_to_uv(&face.surface, face.same_sense, &loop_points(h)))
        .collect();
    if outer.len() < 3 {
        return None;
    }
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for p in &outer {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let shifts = shifts(&face.surface);
    let sgn = if face.same_sense { 1 } else { -1 };
    let segs = || region.outer.iter().chain(region.holes.iter().flatten());
    // Mit Freiformkurven im Rand ist `Seg::closest` teuer (Abtastung plus Goldener Schnitt je
    // Rasterpunkt): Dann wird der Abstand zum Rand über einen feinen Streckenzug geschätzt und
    // nur für den gewählten Punkt genau nachgerechnet. Ohne Freiformkurven unverändert.
    let polyline: Option<Vec<(DVec3, DVec3)>> = segs().any(seg_is_freeform).then(|| {
        segs()
            .flat_map(|s| {
                let n = samples(s) * INTERIOR_POLYLINE_REFINE;
                (0..n).map(move |k| (s.at(k as f64 / n as f64), s.at((k + 1) as f64 / n as f64)))
            })
            .collect()
    });
    let boundary_distance = |q: DVec3| match &polyline {
        Some(lines) => lines
            .iter()
            .map(|(a, b)| segment_distance(q, *a, *b))
            .fold(f64::MAX, f64::min),
        None => segs().map(|s| s.closest(q).1).fold(f64::MAX, f64::min),
    };
    let n = 24;
    let mut best: Option<(DVec3, f64)> = None;
    for i in 0..n {
        for j in 0..n {
            let uv = DVec2::new(
                lo.x + (hi.x - lo.x) * (i as f64 + 0.5) / n as f64,
                lo.y + (hi.y - lo.y) * (j as f64 + 0.5) / n as f64,
            );
            // Löcher können um eine Periode versetzt aufgerollt sein.
            let w = winding(&outer, uv)
                + holes
                    .iter()
                    .flat_map(|h| shifts.iter().map(move |s| winding(h, uv + *s)))
                    .sum::<i32>();
            if w * sgn != 1 {
                continue;
            }
            let q = face.surface.point(uv.x, uv.y);
            let d = boundary_distance(q);
            if best.is_none_or(|(_, bd)| d > bd) {
                best = Some((q, d));
            }
        }
    }
    if polyline.is_some() {
        // genauer Abstand des gewählten Punkts
        best = best.map(|(q, _)| (q, segs().map(|s| s.closest(q).1).fold(f64::MAX, f64::min)));
    }
    best.filter(|(_, d)| *d > tol.linear * 2.0).map(|(q, _)| q)
}

/// Verfeinerung des Streckenzugs, mit dem `interior_point` den Randabstand bei Freiformkurven
/// schätzt (Vielfaches der gewöhnlichen Abtastung `samples`).
const INTERIOR_POLYLINE_REFINE: usize = 4;

/// Abstand von `q` zur Strecke `ab`.
fn segment_distance(q: DVec3, a: DVec3, b: DVec3) -> f64 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 {
        ((q - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    q.distance(a + ab * t)
}

/// Ein Punkt im Inneren der Fläche `face` (nicht auf ihrem Rand und nicht auf einer Naht), mit
/// derselben Suche wie bei der Einordnung der Flächenstücke. `None` bei unbekannter Id oder wenn
/// kein Punkt gefunden wird.
///
/// Das Flächenstück ist die ganze Fläche: äußere Schleife und Löcher direkt aus der Fläche. Das
/// ist genau das, was `split_face` ohne Schnitte liefert, umgeht aber dessen Graphaufbau, der für
/// den Prüfpunkt nichts beiträgt. Nähte periodischer Flächen sind gewöhnliche Stücke der äußeren
/// Schleife; `interior_point` hält deshalb auch zu ihnen Abstand.
pub fn face_probe_point(solid: &Solid, face: Id<Face>, tol: &Tolerance) -> Option<DVec3> {
    solid.faces.get(face)?;
    let geom = face_geom(solid, face);
    let mut loops = geom.loops.iter().cloned();
    let region = Region {
        outer: loops.next()?,
        holes: loops.collect(),
    };
    interior_point(&geom, &region, tol)
}

/// Ein Punkt auf der Kante `edge`: Kurvenpunkt in der Mitte ihres Parameterbereichs.
/// `None` bei unbekannter Id.
pub fn edge_probe_point(solid: &Solid, edge: Id<Edge>) -> Option<DVec3> {
    let e = solid.edges.get(edge)?;
    Some(e.curve.point(0.5 * (e.t0 + e.t1)))
}

/// Kleinster Betrag des Kosinus zwischen den Normalen eines Flächenstücks und der Fläche, auf der
/// es (mit Freiformbeteiligung) liegen soll; deckungsgleiche Flächen haben `|cos| ≈ 1`.
const ON_SURFACE_MIN_COS: f64 = 0.5;

/// Meldung, wenn ein Flächenstück eine Freiformfläche nur schräg berührt (siehe
/// [`classify_region`]).
pub(crate) const OBLIQUE_ON_FREEFORM: &str = "Ein Flächenstück liegt schräg auf einer Freiformfläche des anderen Körpers; die Lage lässt sich nicht sicher bestimmen";

/// Ordnet ein Flächenstück relativ zum anderen Körper ein.
pub fn classify_region(
    face: &FaceGeom,
    region: &Region,
    other: &[FaceGeom],
    tol: &Tolerance,
) -> Result<Class, BooleanError> {
    let q = interior_point(face, region, tol).ok_or(BooleanError::AmbiguousClassification)?;
    let n = outward_normal(face, q);
    if let Some(g) = on_surface_face(other, q, tol) {
        let n2 = outward_normal(g, q);
        // Mit Freiformbeteiligung reicht die weitere Toleranz `NURBS_EDGE_TOL` für „liegt auf“:
        // Ein Stück, das die andere Fläche dort nur schräg trifft, ist nicht deckungsgleich (ein
        // fehlender Schnitt oder ein Gebilde unter der Toleranz); OnSame/OnOpposite wären geraten.
        let freeform =
            matches!(face.surface, Surface::Nurbs(_)) || matches!(g.surface, Surface::Nurbs(_));
        if freeform && n.dot(n2).abs() < ON_SURFACE_MIN_COS {
            return Err(BooleanError::Unsupported(OBLIQUE_ON_FREEFORM.into()));
        }
        return Ok(if n.dot(n2) > 0.0 {
            Class::OnSame
        } else {
            Class::OnOpposite
        });
    }
    Ok(if cast_all(other, q, tol)? {
        Class::Inside
    } else {
        Class::Outside
    })
}
