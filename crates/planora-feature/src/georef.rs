//! Bezüge von Maßen: Umwandlung zwischen den Verweisen des Modells ([`EdgeRef`], [`FaceRef`])
//! und den Verweisdaten der Zeichnung ([`GeoRefData`]) sowie die Auflösung eines Bezugs gegen
//! die exakten Körper zu Modellgeometrie ([`Geo3`]).
//!
//! Die Auflösungsregeln stimmen mit denen des Maßvorschlags (`planora_drawing::suggest`)
//! überein, damit Schlüssel vorhandener und vorgeschlagener Maße zusammenpassen: Kante mit
//! Gerade → `Line` zwischen ihren Eckpunkten, Kreiskante → `Circle` aus dem Kreisrahmen mit dem
//! Bogen der Kante ([`circle_edge_arc`], bei einem vollen Kreis keiner),
//! Zylinderfläche → `Axis` mit der normierten Achsrichtung und dem Bereich der Randkanten
//! ([`axial_range`]), Kugelfläche → `Sphere`.

use crate::error::FeatureError;
use crate::refs::{
    EdgeRef, FaceRef, RefCache, distance, edge_candidates_cached, face_candidates_cached,
};
use planora_drawing::centerline::axial_range;
use planora_drawing::dimension::{DimError, EndSel, Geo3, GeoRef, GeoRefData, TagData};
use planora_drawing::suggest::circle_edge_arc;
use planora_i18n::tr;
use planora_kernel::{Curve, Edge, Face, Id, Solid, Surface};
use planora_math::{DVec3, Tolerance};

fn edge_kind() -> String {
    tr!("feature-geo-edge-kind")
}
fn face_kind() -> String {
    tr!("feature-geo-face-kind")
}
fn no_center() -> String {
    tr!("feature-geo-no-center")
}
fn no_range() -> String {
    tr!("feature-geo-no-range")
}
fn mismatch() -> String {
    tr!("feature-geo-mismatch")
}
fn broken() -> String {
    tr!("feature-geo-broken")
}
/// Grund, wenn die Fläche eines Bezugs fehlt (ohne innere Nummern oder Rollennamen).
pub fn face_gone() -> String {
    tr!("feature-geo-face-gone")
}
/// Grund, wenn die Kante eines Bezugs fehlt (ohne innere Nummern oder Rollennamen).
pub fn edge_gone() -> String {
    tr!("feature-geo-edge-gone")
}

fn lost(text: String) -> DimError {
    DimError::Lost(text)
}

fn tag_data(r: &FaceRef) -> TagData {
    TagData {
        origin: r.origin,
        role: r.role.clone(),
        hint: r.hint,
    }
}

fn face_ref(t: &TagData) -> FaceRef {
    FaceRef {
        origin: t.origin,
        role: t.role.clone(),
        hint: t.hint,
    }
}

/// Verweisdaten eines Kantenverweises (verlustfrei, siehe [`to_edge_ref`]).
pub fn from_edge_ref(r: &EdgeRef) -> GeoRefData {
    GeoRefData::Edge {
        a: tag_data(&r.a),
        b: tag_data(&r.b),
        hint: r.hint,
    }
}

/// Verweisdaten eines Flächenverweises (verlustfrei, siehe [`to_face_ref`]).
pub fn from_face_ref(r: &FaceRef) -> GeoRefData {
    GeoRefData::Face(tag_data(r))
}

/// Kantenverweis aus Verweisdaten; `None` bei Flächendaten.
pub fn to_edge_ref(d: &GeoRefData) -> Option<EdgeRef> {
    match d {
        GeoRefData::Edge { a, b, hint } => Some(EdgeRef {
            a: face_ref(a),
            b: face_ref(b),
            hint: *hint,
        }),
        GeoRefData::Face(_) => None,
    }
}

/// Flächenverweis aus Verweisdaten; `None` bei Kantendaten.
pub fn to_face_ref(d: &GeoRefData) -> Option<FaceRef> {
    match d {
        GeoRefData::Face(t) => Some(face_ref(t)),
        GeoRefData::Edge { .. } => None,
    }
}

/// Verweisdaten für die Kante `edge` von `solid` (wie [`EdgeRef::from_edge`]).
pub fn geo_ref_for_edge(
    solid: &Solid,
    edge: Id<Edge>,
    tol: &Tolerance,
) -> Result<GeoRefData, FeatureError> {
    geo_ref_for_edge_cached(solid, edge, tol, &RefCache::new())
}

/// Wie [`geo_ref_for_edge`], mit Prüfpunkten aus `cache` (gleiches Ergebnis).
pub fn geo_ref_for_edge_cached<'a>(
    solid: &'a Solid,
    edge: Id<Edge>,
    tol: &Tolerance,
    cache: &RefCache<'a>,
) -> Result<GeoRefData, FeatureError> {
    Ok(from_edge_ref(&EdgeRef::from_edge_cached(
        solid, edge, tol, cache,
    )?))
}

/// Verweisdaten für die Fläche `face` von `solid` (wie [`FaceRef::from_face`]).
pub fn geo_ref_for_face(
    solid: &Solid,
    face: Id<Face>,
    tol: &Tolerance,
) -> Result<GeoRefData, FeatureError> {
    geo_ref_for_face_cached(solid, face, tol, &RefCache::new())
}

/// Wie [`geo_ref_for_face`], mit Prüfpunkten aus `cache` (gleiches Ergebnis).
pub fn geo_ref_for_face_cached<'a>(
    solid: &'a Solid,
    face: Id<Face>,
    tol: &Tolerance,
    cache: &RefCache<'a>,
) -> Result<GeoRefData, FeatureError> {
    Ok(from_face_ref(&FaceRef::from_face_cached(
        solid, face, tol, cache,
    )?))
}

/// Wählt unter den Kandidaten `(Körper, Element, Prüfpunkt)` den mit dem kleinsten Abstand des
/// Prüfpunkts zum Hinweis. Liegt ein zweiter bis auf `tol.linear` gleich nah (auch: beide ohne
/// Prüfpunkt), ist der Bezug mehrdeutig. Keine Kandidaten → `Ok(None)`.
fn pick<T: Copy>(
    candidates: &[(usize, T, Option<DVec3>)],
    hint: DVec3,
    tol: &Tolerance,
) -> Result<Option<(usize, T)>, DimError> {
    let mut ranked: Vec<(f64, usize, T)> = candidates
        .iter()
        .map(|&(b, x, p)| (distance(p, hint), b, x))
        .collect();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
    match ranked.as_slice() {
        [] => Ok(None),
        [(_, b, x)] => Ok(Some((*b, *x))),
        [(d0, b, x), (d1, ..), ..] => {
            let tie = if d0.is_finite() {
                d1 - d0 <= tol.linear
            } else {
                true
            };
            if tie {
                Err(DimError::Ambiguous)
            } else {
                Ok(Some((*b, *x)))
            }
        }
    }
}

/// Sucht die Kante der Verweisdaten `d` (müssen Kantendaten sein).
fn find_edge<'a>(
    solids: &[&'a Solid],
    d: &GeoRefData,
    tol: &Tolerance,
    cache: &RefCache<'a>,
) -> Result<(&'a Solid, &'a Edge), DimError> {
    let r = to_edge_ref(d).ok_or_else(|| lost(mismatch()))?;
    let candidates = edge_candidates_cached(solids, &r, cache);
    let (body, edge) = pick(&candidates, r.hint, tol)?.ok_or_else(|| lost(edge_gone()))?;
    let solid = solids[body];
    let e = solid.edges.get(edge).ok_or_else(|| lost(broken()))?;
    Ok((solid, e))
}

fn vertex(solid: &Solid, v: Id<planora_kernel::Vertex>) -> Result<DVec3, DimError> {
    solid
        .vertices
        .get(v)
        .map(|v| v.point)
        .ok_or_else(|| lost(broken()))
}

/// Löst einen Bezug gegen die exakten Körper auf. Fehler als [`DimError`]: nicht gefunden oder
/// nicht bemaßbar → `Lost` mit Grund in der Sprache des Programms; mehrere gleich passende Kandidaten (gleicher
/// Abstand zum Hinweispunkt, siehe Modulbeschreibung) → `Ambiguous`.
///
/// Regeln: `Edge` mit gerader Kurve → `Line` zwischen Anfangs- und Endpunkt der Kante, mit
/// Kreis (voll oder Bogen) → `Circle { Mitte, Normale des Kreisrahmens, Radius, Bogen }` (Bogen
/// nach [`circle_edge_arc`]: Anfang, Mitte und Ende der Kante; `None` bei vollem Kreis, auch aus
/// mehreren Kanten), Ellipse und
/// Freiformkurve → `Lost`; `EdgeEnd` → `Point` (Anfangs- bzw. Endpunkt; ein Vollkreis hat nur
/// den Anfangspunkt); `Center` → `Point(Mitte)` nur für Kreise und Bögen; `Face` mit
/// Zylinderfläche → `Axis` (Ursprung und normierte Achse des Rahmens, Radius, Bereich der
/// Randkanten entlang der Achse), Kugel → `Sphere`, andere Flächen → `Lost`.
pub fn resolve_geo(solids: &[&Solid], r: &GeoRef, tol: &Tolerance) -> Result<Geo3, DimError> {
    resolve_geo_cached(solids, r, tol, &RefCache::new())
}

/// Wie [`resolve_geo`], mit Prüfpunkten, Kantenverwendungen und Achsbereichen aus `cache`
/// (gleiches Ergebnis). Ein Durchgang über viele Maße (Auflösen, Vorschlag, Werkzeug) berechnet
/// so die Prüfpunkte aller Kandidatenflächen und den Achsbereich einer Zylinderfläche nur
/// einmal.
pub fn resolve_geo_cached<'a>(
    solids: &[&'a Solid],
    r: &GeoRef,
    tol: &Tolerance,
    cache: &RefCache<'a>,
) -> Result<Geo3, DimError> {
    match r {
        GeoRef::Edge(d) => {
            let (solid, e) = find_edge(solids, d, tol, cache)?;
            match &e.curve {
                Curve::Line { .. } => Ok(Geo3::Line {
                    a: vertex(solid, e.start)?,
                    b: vertex(solid, e.end)?,
                }),
                Curve::Circle { frame, radius } => Ok(Geo3::Circle {
                    center: frame.origin,
                    normal: frame.z,
                    radius: *radius,
                    arc: circle_edge_arc(solid, e),
                }),
                _ => Err(lost(edge_kind())),
            }
        }
        GeoRef::EdgeEnd { edge, end } => {
            let (solid, e) = find_edge(solids, edge, tol, cache)?;
            // Ein Vollkreis beginnt und endet im selben Eckpunkt: Ende = Anfang.
            let v = match end {
                EndSel::Start => e.start,
                EndSel::End => e.end,
            };
            Ok(Geo3::Point(vertex(solid, v)?))
        }
        GeoRef::Center(d) => {
            let (_, e) = find_edge(solids, d, tol, cache)?;
            match &e.curve {
                Curve::Circle { frame, .. } => Ok(Geo3::Point(frame.origin)),
                _ => Err(lost(no_center())),
            }
        }
        GeoRef::Face(d) => {
            let fr = to_face_ref(d).ok_or_else(|| lost(mismatch()))?;
            let candidates = face_candidates_cached(solids, &fr, tol, cache);
            let (body, face) = pick(&candidates, fr.hint, tol)?.ok_or_else(|| lost(face_gone()))?;
            let solid = solids[body];
            let f = solid.faces.get(face).ok_or_else(|| lost(broken()))?;
            match &f.surface {
                Surface::Cylinder { frame, radius } => {
                    let z = frame.z.normalize_or_zero();
                    let key = (solid as *const Solid, face);
                    let cached = cache.axial.borrow().get(&key).copied();
                    let range = match cached {
                        Some(r) => r,
                        None => {
                            let r = axial_range(solid, f, frame.origin, z);
                            cache.axial.borrow_mut().insert(key, r);
                            r
                        }
                    };
                    let (t0, t1) = range.ok_or_else(|| lost(no_range()))?;
                    Ok(Geo3::Axis {
                        origin: frame.origin,
                        dir: z,
                        radius: *radius,
                        t0,
                        t1,
                    })
                }
                Surface::Sphere { frame, radius } => Ok(Geo3::Sphere {
                    center: frame.origin,
                    radius: *radius,
                }),
                _ => Err(lost(face_kind())),
            }
        }
    }
}
