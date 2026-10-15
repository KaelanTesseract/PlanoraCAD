//! Strahl gegen Flächen: alle Treffer von `origin + s·dir` mit den Flächen aller Körper, je
//! Treffer die Fläche und ob der Punkt im Inneren der beschnittenen Fläche oder auf ihrem Rand
//! liegt.
//!
//! Schnitte: Ebene (linear), Zylinder, Kegel, Kugel (quadratisch im lokalen Rahmen der Fläche),
//! Torus (Vorzeichenwechsel der impliziten Funktion auf 2048 Stützstellen, Bisektion; Berührung
//! ohne Vorzeichenwechsel ist unsicher, außer der Strahl beginnt auf dem Torus und entfernt sich
//! von ihm), Freiform (`ray_hits_checked` des Kernels; verweigert er einen fast tangentialen
//! Strahl, entscheidet eine eigene Unterteilung der Bézier-Flicken mit strengen Ausschlüssen, siehe
//! `nurbs_hits`).
//!
//! Einordnung je Schnittpunkt mit `tol = 16·geom_tol(E)`: Randabstand höchstens `tol` →
//! `Boundary`. Fern von gekrümmten Randkurven wie `point_in_face` des Kernels: Windungszahl der
//! Randpolygone im Parameterraum → `Inside` oder verworfen (die Polygone berechnet
//! [`Scene::new`] einmal je Fläche). Die Polygone tasten Bögen nur alle 5° ab (Freiformkurven
//! nach ihrer Struktur, Geraden nur am Anfang) und weichen vom wahren Rand ab; je Randstück
//! misst [`Scene::new`] diese Abweichung (im Raum und über die Fläche gehoben) und legt ein
//! Band von `max(2 · Abweichung, 2e-3 · Radius bei Kreis/Ellipse) + tol` um das Stück. Im Band
//! entscheidet der nächste Randpunkt `q` exakt: Innenseite ist links der Laufrichtung `τ` um die Außennormale `n` (äußere Schleifen
//! laufen gegen den Uhrzeigersinn, Löcher andersherum), also `(p − q)·(n × τ) > tol` → innen,
//! `< −tol` → außen, sonst Rand. Liegt `q` an einer Ecke (Ende eines Randstücks), gilt der Punkt
//! als `Boundary` (vorsichtig; die Sichtprüfung verwirft solche Strahlen). Nähte (Randstücke,
//! die die Fläche zweimal gegenläufig enthält) sind kein Materialrand und zählen dabei nicht.
//!
//! Unsichere Strahlen (`Ray::Unsafe`): Strahl in der Ebene einer Planfläche, Strahl berührt
//! einen Zylinder, Kegel oder eine Kugel (Doppelwurzel) oder liegt in ihrer Mantelfläche,
//! Strahl berührt einen Torus, Freiformschnitt ohne verlässliche Entscheidung. Jede dieser
//! Prüfungen gilt nur für Flächen, deren (aufgeblähte) Hüllbox der Strahl durchläuft.

use crate::DrawBody;
use crate::error::DrawError;
use crate::limits::geom_tol;
use planora_exchange::Progress;
use planora_kernel::boolean::{
    CurveSurface, FaceGeom, Seg, curve_surface_roots, face_geom, loop_points, points_to_uv,
    points_to_uv_indexed, winding,
};
use planora_kernel::{Curve, Surface};
use planora_math::nurbs::BezierPatch;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use std::f64::consts::PI;

/// Einordnungs- und Hüllbox-Toleranz in Vielfachen von `geom_tol(E)`.
const TOL_FACTOR: f64 = 16.0;
/// `|n · dir|` darunter: Strahl parallel zur Ebene.
const PLANE_PARALLEL: f64 = 1e-9;
/// Abstand des Strahls von der Ebene (relativ zu `E`), unter dem er in der Ebene liegt.
const IN_PLANE_REL: f64 = 1e-6;
/// Doppelwurzel einer Quadrik: halber Wurzelabstand `√|b'² − a·c| / |a| < TANGENT_SEP_REL · E`
/// (für `a = 1` dasselbe wie `|b'² − a·c| < 1e-12·E²`).
const TANGENT_SEP_REL: f64 = 1e-6;
/// Mindestband um Kreise und Ellipsen (relativ zu ihrem größten Maß), in dem der nächste
/// Randpunkt statt des Polygons entscheidet: `2e-3 ≥ 2·(1 − cos 2,5°)` (Abtastung alle 5°).
const RIM_BAND_REL: f64 = 2e-3;
/// Vergleichsstellen je Polygonkante beim Messen der Polygonabweichung (Teilungen).
const SAG_SUBSAMPLES: usize = 8;
/// Sicherheitsfaktor auf die gemessene Polygonabweichung.
const SAG_FACTOR: f64 = 2.0;
/// Bruchteil am Ende eines Randstücks, ab dem der nächste Randpunkt als Ecke gilt.
const CORNER_FRACTION: f64 = 1e-6;
/// `|a|` darunter (Richtung normiert): Strahl parallel zu den Mantellinien.
const QUAD_A_EPS: f64 = 1e-12;
/// `|b'|` darunter (relativ zu `E`): Strahl ohne Querung der Mantelfläche.
const QUAD_B_REL: f64 = 1e-9;
/// Abstand des Strahls von der Mantelfläche (relativ zu `E`), unter dem er in ihr liegt.
const IN_SURFACE_REL: f64 = 1e-6;
/// Stützstellen der früheren Vorzeichenwechsel-Suche am Torus (nur noch Vergleich im Test).
#[cfg(test)]
const TORUS_SAMPLES: usize = 2048;
/// Berührung des Torus: `min |F| < max(TORUS_TOUCH_REL · minor, tol)` ohne Vorzeichenwechsel
/// (`F` ist ein Abstand, die Schwelle also eine Länge).
const TORUS_TOUCH_REL: f64 = 1e-9;
/// Freiform, eigene Flickenunterteilung (wo der Kernel verweigert): kleinste Flickendiagonale
/// (relativ zu E), größte Tiefe, Höchstzahl untersuchter Flicken, Ebenheit (relativ zur Diagonale)
/// und Streifgrenze wie im Kernel, Rundungspolster, Newton-Schritte und -Restschranke (relativ
/// zum Koordinatenmaß), Abstand gleicher Treffer.
const IMPLICIT_LEAF_REL: f64 = 1e-6;
const PATCH_MAX_DEPTH: usize = 40;
const PATCH_MAX_VISITS: usize = 20_000;
const PATCH_FLATNESS: f64 = 1e-3;
const PATCH_GRAZING_COS: f64 = 0.01;
const PATCH_PAD_REL: f64 = 1e-12;
const PATCH_NEWTON_STEPS: usize = 32;
const PATCH_RESIDUAL_REL: f64 = 1e-12;
const PATCH_DEDUP_REL: f64 = 1e-9;
/// Eine Berührung muss `|F|` unter diesen Anteil des Werts am Anfang des Suchbereichs bringen
/// (eine echte Berührung bringt `|F|` auf null; ein Strahl, der auf der Fläche beginnt und sich
/// von ihr entfernt, bleibt darüber).
const START_DIP: f64 = 0.5;
/// Höchstzahl an Halbierungen (endet früher an der Rechengenauigkeit).
const BISECT_STEPS: usize = 200;
/// Schritte des Goldenen Schnitts beim Suchen des kleinsten `|F|` zwischen zwei Stützstellen.
const GOLDEN_STEPS: usize = 80;

/// Lage eines Treffers auf der beschnittenen Fläche.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitClass {
    Inside,
    Boundary,
}

/// Ein Treffer: Abstand `s` entlang des (normierten) Strahls, Körperindex, Flächenindex (Index
/// in `solid.faces`) und Lage auf der Fläche.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub s: f64,
    pub body: usize,
    pub face: usize,
    pub class: HitClass,
}

/// Ergebnis eines Strahls: alle Treffer nach `s` sortiert, oder unsicher mit kurzem Grund.
#[derive(Clone, Debug)]
pub enum Ray {
    Hits(Vec<Hit>),
    Unsafe(&'static str),
}

/// Etwas, das Strahlen prüfen kann (die Sichtprüfung nimmt es als Schnittstelle).
pub trait Caster {
    /// Wie [`Scene::cast`].
    fn cast(&self, origin: DVec3, dir: DVec3, min_s: f64) -> Ray;
}

/// Schnittparameter `s` und, wenn bekannt (Freiform), die Flächenparameter `(u, v)`.
type Candidate = (f64, Option<(f64, f64)>);
/// Berührstelle der Flickenteilung: Strahlparameter und Diagonale des Flickens.
type Touch = (f64, f64);
/// Lagefragen an eine Freiformfläche: liegt ein Punkt auf der beschnittenen Fläche; liegt er
/// höchstens `tol` + Größe von ihrem Rand entfernt.
type FaceTests<'a> = (&'a dyn Fn(DVec3) -> bool, &'a dyn Fn(DVec3, f64) -> bool);

/// Eine Fläche, vorbereitet für viele Strahlen.
struct FaceData {
    body: usize,
    face: usize,
    region: Region,
    /// Freiform: Bézier-Flicken (beim ersten Bedarf zerlegt; None, wenn zu viele).
    patches: std::cell::OnceCell<Option<Vec<BezierPatch>>>,
    /// Hüllbox der Fläche, aufgebläht.
    lo: DVec3,
    hi: DVec3,
}

/// Beschnittene Fläche, vorbereitet für viele Lagefragen: Randpolygone im Parameterraum und
/// Randbänder einmal berechnet (siehe Modulbeschreibung). Auch der Zuschnitt der
/// Freiform-Umrisse nutzt sie.
pub(crate) struct Region {
    pub(crate) geom: FaceGeom,
    /// Randschleifen im Parameterraum (wie `point_in_face` sie bildet).
    polys: Vec<Vec<DVec2>>,
    /// Je Randstück (in der Reihenfolge `loops.iter().flatten()`) seine Art.
    segs: Vec<SegInfo>,
    /// Hüllbox je Randstück (gleiche Reihenfolge) und je Randpolygon (im Parameterraum): Bei
    /// Flächen mit vielen Löchern prüft die Einordnung nur Randstücke und Polygone in der Nähe.
    seg_boxes: Vec<(DVec3, DVec3)>,
    poly_boxes: Vec<(DVec2, DVec2)>,
    /// Größte Bandbreite aller Randstücke (mindestens die Einordnungstoleranz): Randstücke, deren
    /// Hüllbox weiter entfernt liegt, können weder Rand noch nächster Randpunkt im Band sein.
    reach: f64,
}

impl Region {
    /// Bereitet die Fläche vor; `tol` ist die Einordnungstoleranz (Randbänder).
    pub(crate) fn new(geom: FaceGeom, tol: f64) -> Region {
        let polys: Vec<Vec<DVec2>> = geom
            .loops
            .iter()
            .map(|lp| points_to_uv(&geom.surface, geom.same_sense, &loop_points(lp)))
            .collect();
        let segs = seg_infos(&geom, tol);
        let seg_boxes = geom.loops.iter().flatten().map(seg_bbox).collect();
        let poly_boxes = polys
            .iter()
            .map(|poly| {
                poly.iter().fold(
                    (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
                    |(lo, hi), q| (lo.min(*q), hi.max(*q)),
                )
            })
            .collect();
        let reach = segs.iter().map(|s| s.band).fold(tol, f64::max);
        Region {
            geom,
            polys,
            segs,
            seg_boxes,
            poly_boxes,
            reach,
        }
    }
}

/// Art eines Randstücks für die exakte Einordnung nahe am Rand.
#[derive(Clone, Copy, Debug)]
struct SegInfo {
    /// Naht: die Fläche enthält dasselbe Stück auch gegenläufig.
    seam: bool,
    /// Breite des Bands um das Stück, in dem der nächste Randpunkt statt des Polygons
    /// entscheidet (0: Polygon exakt, etwa Geraden auf Ebenen).
    band: f64,
    /// Geschlossen (Anfang = Ende): seine Enden sind keine Ecke.
    closed: bool,
}

/// Größtes Maß eines Kegelschnitts (Kreisradius, große Halbachse); sonst 0.
fn conic_size(c: &Curve) -> f64 {
    match c {
        Curve::Circle { radius, .. } => radius.abs(),
        Curve::Ellipse { major, minor, .. } => major.abs().max(minor.abs()),
        _ => 0.0,
    }
}

/// Gemessene Abweichung des Randpolygons vom wahren Randstück, in den Raum gehoben: Das
/// Stück wird wie im Kernel abgetastet (`loop_points` auf das Stück allein: Bruchteile `k/n`),
/// die Punkte wie im Kernel in `(u, v)` umgerechnet (`points_to_uv_indexed`). Zwischen zwei
/// Abtastpunkten wird an `SAG_SUBSAMPLES − 1` Stellen der wahre Kurvenpunkt mit dem
/// Flächenpunkt der Sehne im Parameterraum verglichen (das ist die Kante des Polygons, mit dem
/// die Windungszahl rechnet, in den Raum gehoben); fehlt ein Parameter (Pol), mit der Sehne im
/// Raum. So erhalten auch Geraden ein Band, die im Parameterraum gekrümmt sind (Geraden auf
/// Freiformflächen, die keine Parameterlinien sind).
fn polygon_sag(surface: &Surface, same_sense: bool, s: &Seg) -> f64 {
    let mut pts = loop_points(std::slice::from_ref(s));
    let n = pts.len();
    if n == 0 {
        return 0.0;
    }
    pts.push(s.end());
    let mut uv: Vec<Option<DVec2>> = vec![None; n + 1];
    for (q, idx) in points_to_uv_indexed(surface, same_sense, &pts) {
        if let Some(i) = idx {
            uv[i] = Some(q);
        }
    }
    let mut sag: f64 = 0.0;
    for k in 0..n {
        for j in 1..SAG_SUBSAMPLES {
            let w = j as f64 / SAG_SUBSAMPLES as f64;
            let c = s.at((k as f64 + w) / n as f64);
            // Polygonkante über die Fläche gehoben; an Polen (kein uv) ersatzweise die Sehne.
            let lifted = match (uv[k], uv[k + 1]) {
                (Some(a), Some(b)) => {
                    let m = a.lerp(b, w);
                    Some(surface.point(m.x, m.y)).filter(|p| p.is_finite())
                }
                _ => None,
            };
            let edge = lifted.unwrap_or_else(|| pts[k].lerp(pts[k + 1], w));
            sag = sag.max(c.distance(edge));
        }
    }
    if sag.is_finite() { sag } else { f64::INFINITY }
}

/// Art aller Randstücke der Fläche: Naht, geschlossen, Bandbreite
/// `max(SAG_FACTOR · gemessene Abweichung, 2e-3 · Maß bei Kreis/Ellipse) + tol`
/// (0, wenn das Polygon das Stück genau wiedergibt).
fn seg_infos(g: &FaceGeom, tol: f64) -> Vec<SegInfo> {
    let all: Vec<&Seg> = g.loops.iter().flatten().collect();
    let ends: Vec<(DVec3, DVec3, DVec3)> = all
        .iter()
        .map(|s| (s.start(), s.at(0.5), s.end()))
        .collect();
    all.iter()
        .enumerate()
        .map(|(i, s)| {
            let (a, m, b) = ends[i];
            let seam = ends.iter().enumerate().any(|(j, &(a2, m2, b2))| {
                j != i && a.distance(b2) <= tol && b.distance(a2) <= tol && m.distance(m2) <= tol
            });
            let band = if seam {
                0.0
            } else {
                let w = (SAG_FACTOR * polygon_sag(&g.surface, g.same_sense, s))
                    .max(RIM_BAND_REL * conic_size(&s.curve));
                // Unterhalb der Toleranz gibt das Polygon das Stück genau genug wieder.
                if w > tol { w + tol } else { 0.0 }
            };
            SegInfo {
                seam,
                band,
                closed: a.distance(b) <= tol,
            }
        })
        .collect()
}
/// Alle Flächen aller Körper, vorbereitet für viele Strahlen (FaceGeom, Hüllbox, Randpolygone
/// einmal berechnet).
pub struct Scene {
    faces: Vec<FaceData>,
    /// Größter Strahlabstand, gemessen ab dem Strahlursprung: Durchmesser der Hüllbox aller
    /// Körper plus `extent`. Reicht durch die ganze Szene, solange der Ursprung höchstens
    /// `extent` außerhalb ihrer Hüllbox liegt (die Sichtprüfung startet auf Strichen, also in
    /// der Hüllbox); in Debug-Bauten geprüft.
    reach: f64,
    /// Hüllbox aller Flächen (nicht aufgebläht).
    scene_lo: DVec3,
    scene_hi: DVec3,
    extent: f64,
    tol: Tolerance,
}

impl Scene {
    /// Bereitet alle Flächen vor. Zählt keine Strahltests (das macht die Sichtprüfung).
    pub fn new(bodies: &[DrawBody], extent: f64, progress: &Progress) -> Result<Scene, DrawError> {
        let extent = if extent.is_finite() {
            extent.max(1e-9)
        } else {
            return Err(DrawError::NonFinite);
        };
        let pad = TOL_FACTOR * geom_tol(extent);
        let plane_pad = pad.max(IN_PLANE_REL * extent);
        let mut faces = Vec::new();
        let mut all_lo = DVec3::splat(f64::INFINITY);
        let mut all_hi = DVec3::splat(f64::NEG_INFINITY);
        for (bi, body) in bodies.iter().enumerate() {
            let solid = body.solid;
            for (fid, _) in solid.faces.iter() {
                progress.check()?;
                let geom = face_geom(solid, fid);
                check_finite(&geom.surface)?;
                let (lo, hi) = face_bbox(&geom);
                if !(lo.is_finite() && hi.is_finite()) {
                    return Err(DrawError::NonFinite);
                }
                all_lo = all_lo.min(lo);
                all_hi = all_hi.max(hi);
                let p = if matches!(geom.surface, Surface::Plane { .. }) {
                    plane_pad
                } else {
                    pad
                };
                faces.push(FaceData {
                    body: bi,
                    face: fid.index(),
                    region: Region::new(geom, pad),
                    patches: std::cell::OnceCell::new(),
                    lo: lo - DVec3::splat(p),
                    hi: hi + DVec3::splat(p),
                });
            }
        }
        let diameter = if faces.is_empty() {
            0.0
        } else {
            (all_hi - all_lo).length()
        };
        Ok(Scene {
            faces,
            reach: diameter + extent,
            scene_lo: all_lo,
            scene_hi: all_hi,
            extent,
            tol: Tolerance {
                linear: TOL_FACTOR * geom_tol(extent),
                ..Tolerance::default()
            },
        })
    }

    /// Alle Treffer von `origin + s·dir` mit `min_s < s ≤ reach` (dir wird normiert, `s` ist
    /// also eine Länge). Treffer auf dem Rand einer Fläche heißen `Boundary`.
    /// `Err(AmbiguousVisibility)` wird NICHT hier erzeugt; unsichere Strahlen liefern
    /// `Ray::Unsafe`.
    pub fn cast(&self, origin: DVec3, dir: DVec3, min_s: f64) -> Ray {
        let len = dir.length();
        if !(origin.is_finite() && len.is_finite() && len > 0.0 && !min_s.is_nan()) {
            return Ray::Unsafe("ungültiger Strahl");
        }
        let d = dir / len;
        debug_assert!(
            self.faces.is_empty()
                || (origin
                    .cmpge(self.scene_lo - DVec3::splat(self.extent))
                    .all()
                    && origin
                        .cmple(self.scene_hi + DVec3::splat(self.extent))
                        .all()),
            "Strahlursprung weiter als `extent` außerhalb der Szene: `reach` reicht nicht"
        );
        let mut out = Vec::new();
        if min_s >= self.reach {
            return Ray::Hits(out);
        }
        for f in &self.faces {
            let Some((sa, sb)) = ray_box(origin, d, min_s, self.reach, f.lo, f.hi) else {
                continue;
            };
            let cands = match self.face_hits(f, origin, d, min_s, (sa, sb)) {
                Ok(c) => c,
                Err(why) => return Ray::Unsafe(why),
            };
            for (s, uv) in cands {
                if !(s.is_finite() && s > min_s && s <= self.reach) {
                    continue;
                }
                let p = origin + d * s;
                let uv = uv.unwrap_or_else(|| f.region.geom.surface.project(p));
                if let Some(class) = self.classify(f, p, uv) {
                    out.push(Hit {
                        s,
                        body: f.body,
                        face: f.face,
                        class,
                    });
                }
            }
        }
        out.sort_by(|a, b| a.s.total_cmp(&b.s));
        Ray::Hits(out)
    }

    /// Schnittparameter (und, wenn bekannt, Flächenparameter) des Strahls mit der Trägerfläche;
    /// `Err` mit Grund, wenn der Strahl unsicher ist.
    fn face_hits(
        &self,
        f: &FaceData,
        o: DVec3,
        d: DVec3,
        min_s: f64,
        (sa, sb): (f64, f64),
    ) -> Result<Vec<Candidate>, &'static str> {
        let e = self.extent;
        let roots = match &f.region.geom.surface {
            Surface::Plane { frame } => {
                let nd = frame.z.dot(d);
                let dist = frame.z.dot(o - frame.origin);
                if nd.abs() < PLANE_PARALLEL {
                    if dist.abs() < IN_PLANE_REL * e {
                        return Err("Strahl liegt in einer ebenen Fläche");
                    }
                    return Ok(Vec::new());
                }
                vec![-dist / nd]
            }
            Surface::Cylinder { frame, radius } => {
                let (lo, ld) = local(frame, o, d);
                let a = ld.x * ld.x + ld.y * ld.y;
                let b = lo.x * ld.x + lo.y * ld.y;
                let c = lo.x * lo.x + lo.y * lo.y - radius * radius;
                quadratic(a, b, c, e, min_s, self.reach, || {
                    f.region.geom.surface.implicit(o).abs()
                })?
            }
            Surface::Cone {
                frame,
                radius,
                slope,
            } => {
                let (lo, ld) = local(frame, o, d);
                let k = *slope;
                let w = radius + k * lo.z;
                let a = ld.x * ld.x + ld.y * ld.y - k * k * ld.z * ld.z;
                let b = lo.x * ld.x + lo.y * ld.y - k * w * ld.z;
                let c = lo.x * lo.x + lo.y * lo.y - w * w;
                // Wurzeln auf dem Gegenast des Doppelkegels (Radius < 0, jenseits der Spitze)
                // haben eine Höhe außerhalb der Fläche und fallen bei der Einordnung heraus.
                quadratic(a, b, c, e, min_s, self.reach, || {
                    f.region.geom.surface.implicit(o).abs()
                })?
            }
            Surface::Sphere { frame, radius } => {
                let oc = o - frame.origin;
                quadratic(
                    1.0,
                    oc.dot(d),
                    oc.length_squared() - radius * radius,
                    e,
                    min_s,
                    self.reach,
                    || f.region.geom.surface.implicit(o).abs(),
                )?
            }
            Surface::Torus { minor, .. } => {
                let lo_s = sa.max(min_s);
                let hi_s = sb.min(self.reach);
                let touch = (TORUS_TOUCH_REL * minor).max(self.tol.linear);
                torus_roots(&f.region.geom.surface, touch, o, d, lo_s, hi_s, min_s)?
            }
            Surface::Nurbs(n) => {
                // Ab dem Eintritt in die Hüllbox der Fläche (Treffer mit s ≤ min_s zählen nie).
                let start = sa.max(min_s);
                let end = sb.min(self.reach);
                if end < start {
                    return Ok(Vec::new());
                }
                let on_face = |p: DVec3| {
                    let uv = f.region.geom.surface.project(p);
                    self.classify(f, p, uv).is_some()
                };
                let Some(patches) = f
                    .patches
                    .get_or_init(|| n.surface().bezier_patches().ok())
                    .as_deref()
                else {
                    return Err("Freiformfläche nicht sicher entscheidbar");
                };
                // Abstand zum Rand der Fläche (Randkurven einschließlich Nähte, also auch zum
                // Rand des Parameterbereichs, soweit er Rand der Fläche ist).
                let near_rim =
                    |p: DVec3, size: f64| f.region.rim_distance(p) <= self.tol.linear + size;
                return nurbs_hits(
                    n,
                    patches,
                    o,
                    d,
                    (start, end),
                    e,
                    min_s,
                    (&on_face, &near_rim),
                );
            }
        };
        Ok(roots.into_iter().map(|s| (s, None)).collect())
    }

    /// Lage von `p` (Parameter `uv`) auf der beschnittenen Fläche `f` (siehe
    /// [`Region::classify`]).
    fn classify(&self, f: &FaceData, p: DVec3, uv: (f64, f64)) -> Option<HitClass> {
        f.region.classify(self.tol.linear, p, uv)
    }
}

impl Region {
    /// Lage von `p` (Parameter `uv`) auf der beschnittenen Fläche; `None` für außerhalb.
    /// Randabstand ≤ `tol` → Rand; im Band um gekrümmte Randkurven der nächste Randpunkt
    /// (exakt), sonst die Windungszahl der Randpolygone wie `point_in_face` des Kernels.
    pub(crate) fn classify(&self, tol: f64, p: DVec3, (u, v): (f64, f64)) -> Option<HitClass> {
        // Nächster Punkt auf einem Randstück, das keine Naht ist: (Abstand, Stück, Bruchteil).
        let mut nearest: Option<(f64, &Seg, SegInfo, f64)> = None;
        let mut in_band = false;
        let reach = self.reach.max(tol);
        for ((s, info), (lo, hi)) in self
            .geom
            .loops
            .iter()
            .flatten()
            .zip(&self.segs)
            .zip(&self.seg_boxes)
        {
            // Hüllbox weiter als jedes Band entfernt: weder Rand noch nächster Randpunkt im Band
            // (NaN-Boxen werden nicht übersprungen).
            if p.clamp(*lo, *hi).distance(p) > reach {
                continue;
            }
            let (fr, dist) = s.closest(p);
            if dist <= tol {
                return Some(HitClass::Boundary);
            }
            if info.seam {
                continue;
            }
            in_band |= dist <= info.band;
            if nearest.is_none_or(|n| dist < n.0) {
                nearest = Some((dist, s, *info, fr));
            }
        }
        if in_band && let Some((_, seg, info, fr)) = nearest {
            return self.side_of_rim(tol, p, seg, info, fr);
        }
        let shifts = |period: Option<f64>| match period {
            Some(t) => vec![-t, 0.0, t],
            None => vec![0.0],
        };
        let us = shifts(self.geom.surface.u_period());
        let vs = shifts(self.geom.surface.v_period());
        let mut total = 0;
        for (poly, (lo, hi)) in self.polys.iter().zip(&self.poly_boxes) {
            for du in &us {
                for dv in &vs {
                    let q = DVec2::new(u + du, v + dv);
                    // Außerhalb der Hüllbox des Polygons ist seine Windungszahl null.
                    if q.cmplt(*lo).any() || q.cmpgt(*hi).any() {
                        continue;
                    }
                    total += winding(poly, q);
                }
            }
        }
        let sense = if self.geom.same_sense { 1 } else { -1 };
        (total * sense >= 1).then_some(HitClass::Inside)
    }

    /// Kleinster Abstand von `p` zu einem Randstück der Fläche (Nähte eingeschlossen).
    pub(crate) fn rim_distance(&self, p: DVec3) -> f64 {
        let mut best = f64::INFINITY;
        for (s, (lo, hi)) in self.geom.loops.iter().flatten().zip(&self.seg_boxes) {
            if p.clamp(*lo, *hi).distance(p) >= best {
                continue;
            }
            best = best.min(s.closest(p).1);
        }
        best
    }

    /// Exakte Seite von `p` gegenüber dem nächsten Randpunkt `q = seg(fr)`: Die Fläche liegt
    /// links der Laufrichtung `τ` um die Außennormale `n`, die Innenseite zeigt also nach
    /// `n × τ`. An einer Ecke (Ende eines nicht geschlossenen Stücks) ist das nicht
    /// entscheidbar → Rand.
    fn side_of_rim(
        &self,
        tol: f64,
        p: DVec3,
        seg: &Seg,
        info: SegInfo,
        fr: f64,
    ) -> Option<HitClass> {
        if !info.closed && !(CORNER_FRACTION..=1.0 - CORNER_FRACTION).contains(&fr) {
            return Some(HitClass::Boundary);
        }
        let q = seg.at(fr);
        let (qu, qv) = self.geom.surface.project(q);
        let n = self.geom.surface.normal(qu, qv);
        let n = if self.geom.same_sense { n } else { -n };
        let inward = n.cross(seg.tangent_at(fr)).normalize_or_zero();
        if inward == DVec3::ZERO || !inward.is_finite() {
            return Some(HitClass::Boundary);
        }
        let side = (p - q).dot(inward);
        if side > tol {
            Some(HitClass::Inside)
        } else if side < -tol {
            None
        } else {
            Some(HitClass::Boundary)
        }
    }
}

/// Probepunkte eines Strichs für seine Hüllbox beim Durchstoßen.
const PIERCE_BOX_SAMPLES: usize = 65;

impl Scene {
    /// Parameter, an denen der Strich die beschnittenen Flächen **anderer** Körper als `body`
    /// durchstößt (oder berührt): Dort kann die Sichtbarkeit wechseln, ohne dass sich im Bild
    /// zwei Striche kreuzen (Körper, die ineinander stecken). Nullstellen der impliziten
    /// Flächenfunktion längs der Kurve (`curve_surface_roots` des Kernels), eingeordnet wie die
    /// Strahltreffer (innen oder Rand zählt). Liegt der Strich ganz in der Trägerfläche
    /// (gemeinsame Fläche), entsteht kein Teilpunkt. Vorauswahl über die Hüllboxen; jedes
    /// geprüfte Paar Strich–Fläche zählt `pairs` (über `MAX_PAIR_TESTS` → `TooComplex`).
    pub fn pierce_params(
        &self,
        stroke: &crate::stroke::Stroke,
        body: usize,
        pairs: &std::cell::Cell<u64>,
        progress: &Progress,
    ) -> Result<Vec<f64>, DrawError> {
        progress.check()?;
        let (t0, t1) = (stroke.t0.min(stroke.t1), stroke.t0.max(stroke.t1));
        let pts: Vec<DVec3> = (0..PIERCE_BOX_SAMPLES)
            .map(|i| stroke.point(t0 + (t1 - t0) * i as f64 / (PIERCE_BOX_SAMPLES - 1) as f64))
            .collect();
        // Zwischen zwei Probepunkten weicht die Kurve höchstens um etwa den Punktabstand ab.
        let gap = pts
            .windows(2)
            .map(|w| w[0].distance(w[1]))
            .fold(0.0, f64::max);
        let pad = DVec3::splat(gap + self.tol.linear);
        let lo = pts
            .iter()
            .fold(DVec3::splat(f64::INFINITY), |a, &b| a.min(b))
            - pad;
        let hi = pts
            .iter()
            .fold(DVec3::splat(f64::NEG_INFINITY), |a, &b| a.max(b))
            + pad;
        if !(lo.is_finite() && hi.is_finite()) {
            return Err(DrawError::NonFinite);
        }
        let mut out = Vec::new();
        for f in self.faces.iter().filter(|f| f.body != body) {
            if (f.lo.cmpgt(hi) | f.hi.cmplt(lo)).any() {
                continue;
            }
            let n = pairs.get() + 1;
            pairs.set(n);
            if n > crate::limits::MAX_PAIR_TESTS {
                return Err(DrawError::TooComplex(n));
            }
            progress.check()?;
            let roots =
                match curve_surface_roots(&stroke.curve, t0, t1, &f.region.geom.surface, &self.tol)
                {
                    CurveSurface::Roots(r) => r,
                    CurveSurface::Lies => continue,
                };
            for r in roots {
                let p = stroke.point(r.t);
                let uv = f.region.geom.surface.project(p);
                if self.classify(f, p, uv).is_some() {
                    out.push(r.t);
                }
            }
        }
        Ok(out)
    }
}

impl Caster for Scene {
    fn cast(&self, origin: DVec3, dir: DVec3, min_s: f64) -> Ray {
        Scene::cast(self, origin, dir, min_s)
    }
}

/// Strahlpunkt und Richtung im lokalen Rahmen.
fn local(frame: &Frame, o: DVec3, d: DVec3) -> (DVec3, DVec3) {
    (
        frame.to_local(o),
        DVec3::new(d.dot(frame.x), d.dot(frame.y), d.dot(frame.z)),
    )
}

/// Wurzeln von `a·s² + 2b·s + c = 0` (Richtung normiert). Doppelwurzel (halber Wurzelabstand
/// `√|b² − a·c| / |a| < 1e-6·E`), deren Mitte oder eine Wurzel in `(min_s, reach]` liegt, und
/// Strahl in der Mantelfläche (`a ≈ 0`, `b ≈ 0`, Abstand `dist()` < 1e-6·E) sind unsicher.
fn quadratic(
    a: f64,
    b: f64,
    c: f64,
    e: f64,
    min_s: f64,
    reach: f64,
    dist: impl Fn() -> f64,
) -> Result<Vec<f64>, &'static str> {
    if a.abs() < QUAD_A_EPS {
        if b.abs() < QUAD_B_REL * e {
            if dist() < IN_SURFACE_REL * e {
                return Err("Strahl liegt in einer gekrümmten Fläche");
            }
            return Ok(Vec::new());
        }
        // Die zweite Wurzel liegt bei |s| ≳ 2·|b|/|a|, weit hinter `reach`.
        return Ok(vec![-c / (2.0 * b)]);
    }
    let disc = b * b - a * c;
    // Doppelwurzel gemessen am Wurzelabstand, nicht an `disc` selbst: Bei kleinem `a` (Strahl
    // fast parallel zu den Mantellinien) liegen die Wurzeln trotz winziger `disc` weit
    // auseinander und sind gewöhnliche Schnitte.
    let half_sep = disc.abs().sqrt() / a.abs();
    if half_sep < TANGENT_SEP_REL * e {
        let s0 = -b / a;
        let in_range = |s: f64| s > min_s && s <= reach;
        if in_range(s0) || (disc > 0.0 && (in_range(s0 - half_sep) || in_range(s0 + half_sep))) {
            return Err("Strahl streift eine gekrümmte Fläche");
        }
        return Ok(Vec::new());
    }
    if disc < 0.0 {
        return Ok(Vec::new());
    }
    let q = -(b + b.signum() * disc.sqrt());
    if q == 0.0 {
        return Ok(vec![0.0]);
    }
    Ok(vec![q / a, c / q])
}

/// Schnitte des Strahls mit dem Torus für `s ∈ [lo, hi]` über das Viertgradpolynom des Strahls.
///
/// Im lokalen Rahmen des Torus ist `F(s) = implicit(o + s·d)` genau dort null, wo
/// `Q(s) = (|p|² + R² − r²)² − 4R²(p_x² + p_y²)` null ist (`Q = G·H` mit
/// `G = (ρ − R)² + z² − r²`, dem Vorzeichen von `F`, und `H = (ρ + R)² + z² − r²`). Zwischen
/// zwei Nullstellen der Ableitung `Q'` ist `Q` monoton, hat also höchstens eine Nullstelle.
/// Bezogen auf den Strahlpunkt `σ = 0` nächst der Torusmitte ist `Q'` ein reduziertes
/// Kubikpolynom `σ³ + pσ + q` (geschlossen gelöst, mit Newton nachgeschärft). Seine Wurzeln im
/// Bereich teilen `[lo, hi]` in Stücke; ein Vorzeichenwechsel von `F` an den Stückenden ergibt
/// einen Schnitt (Bisektion auf `F`, wie bisher bis zur Rechengenauigkeit).
///
/// An jeder Wurzel von `Q'`, an der `|F|` kleiner ist als an den Nachbarteilstellen (Annäherung
/// an die Fläche ohne Vorzeichenwechsel), sucht der Goldene Schnitt zwischen den Nachbarstellen
/// das kleinste `|F|` (gegen Rundung der Wurzel): Wechselt dort das Vorzeichen, sind es zwei
/// Schnitte; bleibt `min |F|` unter `touch` (eine Länge) und deutlich unter den Werten an den
/// Nachbarstellen (`START_DIP`), berührt der Strahl den Torus (unsicher, wenn hinter `min_s`).
/// Ein Strahl, der auf dem Torus beginnt und sich entfernt, hat dort kein solches Minimum.
/// Ergebnis wie die frühere Abtastung an 2048 Stellen (Prüfung im Modultest), aber mit wenigen
/// Dutzend statt Tausenden Auswertungen.
fn torus_roots(
    surf: &Surface,
    touch: f64,
    o: DVec3,
    d: DVec3,
    lo: f64,
    hi: f64,
    min_s: f64,
) -> Result<Vec<f64>, &'static str> {
    if hi <= lo {
        return Ok(Vec::new());
    }
    let Surface::Torus {
        frame,
        major,
        minor,
    } = surf
    else {
        return Ok(Vec::new());
    };
    let f = |s: f64| surf.implicit(o + d * s);
    let mut xs = vec![lo];
    xs.extend(
        torus_critical_params(frame, *major, *minor, o, d)
            .into_iter()
            .filter(|&s| s > lo && s < hi),
    );
    xs.push(hi);
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    let fs: Vec<f64> = xs.iter().map(|&s| f(s)).collect();
    let n = xs.len() - 1;
    let mut roots = Vec::new();
    for i in 0..=n {
        if fs[i] == 0.0 {
            // Teilstelle genau auf der Fläche: Berührung oder Querung?
            let before = if i > 0 { fs[i - 1] } else { fs[i] };
            let after = if i < n { fs[i + 1] } else { fs[i] };
            if before * after >= 0.0 && xs[i] > min_s {
                return Err("Strahl berührt Torus");
            }
            roots.push(xs[i]);
        }
    }
    // Annäherungen an die Fläche: innere Teilstelle `j`, deren Nachbarn gleiches Vorzeichen
    // haben und an der `F` (in dieser Richtung) kleiner ist als an beiden. Bleibt das kleinste
    // `|F|` dort unter `touch`, ist es eine Berührung – auch wenn `F` dabei knapp das Vorzeichen
    // wechselt (zwei Schnitte im Abstand der Rundung sind nicht verlässlich zu trennen). Die
    // beiden Teilstücke daneben liefern dann keine Schnitte.
    let mut done = vec![false; n];
    for j in 1..n {
        let (a, m, b) = (fs[j - 1], fs[j], fs[j + 1]);
        if a * b <= 0.0 || m == 0.0 {
            continue;
        }
        let sign = a.signum();
        if sign * m >= (sign * a).min(sign * b) {
            continue;
        }
        let g = |s: f64| sign * f(s);
        let (mut sm, mut gm) = golden_min(&g, xs[j - 1], xs[j + 1]);
        if sign * m < gm {
            (sm, gm) = (xs[j], sign * m);
        }
        if gm.abs() < touch && gm.abs() < START_DIP * a.abs().min(b.abs()) {
            if sm > min_s {
                return Err("Strahl berührt Torus");
            }
            done[j - 1] = true;
            done[j] = true;
            continue;
        }
        if gm < 0.0 && sign * m > 0.0 {
            // Die Rundung der Teilstelle verdeckt zwei Schnitte daneben.
            roots.push(bisect(&f, xs[j - 1], sm, a));
            roots.push(bisect(&f, sm, xs[j + 1], sign * gm));
            done[j - 1] = true;
            done[j] = true;
        }
    }
    for i in 0..n {
        if !done[i] && fs[i] * fs[i + 1] < 0.0 {
            roots.push(bisect(&f, xs[i], xs[i + 1], fs[i]));
        }
    }
    roots.sort_by(f64::total_cmp);
    Ok(roots)
}

/// Strahlparameter `s`, an denen die Ableitung des Viertgradpolynoms `Q` von Strahl und Torus
/// verschwindet (höchstens drei; siehe [`torus_roots`]).
fn torus_critical_params(frame: &Frame, major: f64, minor: f64, o: DVec3, d: DVec3) -> Vec<f64> {
    let (lo, ld) = local(frame, o, d);
    let dd = ld.length_squared();
    if !(dd > 0.0 && dd.is_finite()) {
        return Vec::new();
    }
    // Bezugspunkt nächst der Mitte: p = p0 + σ·ld mit p0 · ld = 0, also |p|² = |p0|² + σ²·dd.
    let sc = -lo.dot(ld) / dd;
    let p0 = lo + ld * sc;
    let r2 = major * major;
    let k = p0.length_squared() + r2 - minor * minor;
    let b = p0.x * ld.x + p0.y * ld.y;
    let c = ld.x * ld.x + ld.y * ld.y;
    // Q(σ) = dd²σ⁴ + 2(k·dd − 2R²c)σ² − 8R²bσ + k² − 4R²|p0_xy|²,
    // Q'(σ)/(4dd²) = σ³ + pσ + q.
    let p = (k * dd - 2.0 * r2 * c) / (dd * dd);
    let q = -2.0 * r2 * b / (dd * dd);
    depressed_cubic_roots(p, q)
        .into_iter()
        .map(|x| sc + x)
        .filter(|s| s.is_finite())
        .collect()
}

/// Reelle Wurzeln von `x³ + p·x + q` (Cardano bzw. trigonometrisch), je mit Newton
/// nachgeschärft.
fn depressed_cubic_roots(p: f64, q: f64) -> Vec<f64> {
    if !(p.is_finite() && q.is_finite()) {
        return Vec::new();
    }
    let h = 0.5 * q;
    let t = p / 3.0;
    let disc = h * h + t * t * t;
    let mut out: Vec<f64> = if disc > 0.0 {
        // Eine reelle Wurzel; Vorzeichen so, dass sich nichts aufhebt.
        let a = -(h.signum() * (h.abs() + disc.sqrt())).cbrt();
        let a = if h == 0.0 { disc.sqrt().cbrt() } else { a };
        vec![if a == 0.0 { 0.0 } else { a - t / a }]
    } else if t == 0.0 {
        vec![0.0]
    } else {
        // Drei reelle Wurzeln (p < 0).
        let m = 2.0 * (-t).sqrt();
        let arg = (3.0 * q / (p * m)).clamp(-1.0, 1.0);
        let phi = arg.acos() / 3.0;
        (0..3)
            .map(|k| m * (phi - 2.0 * PI * k as f64 / 3.0).cos())
            .collect()
    };
    for x in &mut out {
        for _ in 0..4 {
            let fx = (*x * *x + p) * *x + q;
            let dx = 3.0 * *x * *x + p;
            if dx == 0.0 || !fx.is_finite() {
                break;
            }
            let nx = *x - fx / dx;
            if !nx.is_finite() || ((nx * nx + p) * nx + q).abs() >= fx.abs() {
                break;
            }
            *x = nx;
        }
    }
    out
}

/// Frühere Torus-Schnittsuche (Abtastung an `TORUS_SAMPLES` Stellen), als Vergleich im
/// Modultest: Vorzeichenwechsel von `F(s) = implicit(o + s·d)` auf `TORUS_SAMPLES` Stützstellen,
/// Bisektion. Zwischen zwei Stützstellen gleichen Vorzeichens, zwischen denen `|F|` die Null
/// erreichen könnte (`F` ist ein Abstand, ändert sich also höchstens um die Schrittweite), sucht
/// der Goldene Schnitt das kleinste `|F|`: Wechselt dort das Vorzeichen, sind es zwei Schnitte;
/// bleibt `min |F|` unter `touch` (eine Länge), berührt der Strahl den Torus (unsicher).
#[cfg(test)]
fn torus_roots_sampled(
    surf: &Surface,
    touch: f64,
    o: DVec3,
    d: DVec3,
    lo: f64,
    hi: f64,
    min_s: f64,
) -> Result<Vec<f64>, &'static str> {
    if hi <= lo {
        return Ok(Vec::new());
    }
    let f = |s: f64| surf.implicit(o + d * s);
    let n = TORUS_SAMPLES;
    let h = (hi - lo) / n as f64;
    let ss: Vec<f64> = (0..=n).map(|i| lo + h * i as f64).collect();
    let fs: Vec<f64> = ss.iter().map(|&s| f(s)).collect();
    let mut roots = Vec::new();
    for i in 0..=n {
        if fs[i] == 0.0 {
            // Stützstelle genau auf der Fläche: Berührung oder Querung?
            let before = if i > 0 { fs[i - 1] } else { fs[i] };
            let after = if i < n { fs[i + 1] } else { fs[i] };
            if before * after >= 0.0 && ss[i] > min_s {
                return Err("Strahl berührt Torus");
            }
            roots.push(ss[i]);
            continue;
        }
        if i == n {
            break;
        }
        let (a, b) = (fs[i], fs[i + 1]);
        if b == 0.0 {
            continue;
        }
        if a * b < 0.0 {
            roots.push(bisect(&f, ss[i], ss[i + 1], a));
            continue;
        }
        // Gleiches Vorzeichen: kann |F| dazwischen null werden?
        if (a.abs() + b.abs() - h) / 2.0 > touch {
            continue;
        }
        let sign = a.signum();
        let g = |s: f64| sign * f(s);
        let (sm, gm) = golden_min(&g, ss[i], ss[i + 1]);
        // Berührung nur, wo |F| unter `touch` **und** deutlich unter beide Endwerte des
        // Stützintervalls fällt: Ein Prüfstrahl, der auf dem Torus beginnt (von einer Linie auf
        // der Fläche) und sich von ihr entfernt, bleibt anfangs unter `touch`, ohne sie zu
        // berühren; eine echte Berührung bringt |F| auf null.
        if gm.abs() < touch && gm.abs() < START_DIP * a.abs().min(b.abs()) {
            if sm > min_s {
                return Err("Strahl berührt Torus");
            }
            continue;
        }
        if gm < 0.0 {
            roots.push(bisect(&f, ss[i], sm, a));
            roots.push(bisect(&f, sm, ss[i + 1], sign * gm));
        }
    }
    Ok(roots)
}

/// Schnitte des Strahls `o + s·d`, `s ∈ [start, end]`, mit der Freiformfläche: zuerst
/// `ray_hits_checked` des Kernels. Der Kernel verweigert jeden Strahl, der ein Flächenstück fast
/// tangential durchläuft (`RAY_GRAZING_COS`) – etwa Prüfstrahlen vom Umriss der Fläche oder von
/// einer Kante, an der eine Freiformwand tangential ansetzt. Dann entscheidet
/// [`patch_hits`] (Unterteilung der Bézier-Flicken mit strengen Ausschlüssen). Bleiben dort
/// Stellen unentschieden (der Strahl verläuft auf weniger als `IMPLICIT_LEAF_REL · E` an der
/// Fläche entlang), ist das eine Berührung: auf der beschnittenen Fläche (`on_face`) hinter
/// `min_s` macht sie den Strahl unsicher – außer im Stück bis `OWN_EPS` hinter dem Anfang eines
/// Strahls, der auf der Fläche selbst beginnt (dort liegt nur seine eigene Tangentialstelle;
/// Treffer auf eigenen Flächen bis `OWN_EPS` verdecken nach F4 ohnehin nicht).
///
/// Eine solche Stelle **neben** der beschnittenen Fläche zählt nur dann nicht, wenn sie weiter als
/// `tol` + Flickengröße von ihrem Rand entfernt liegt (`near_rim`): Newton im Flicken hat keinen
/// Neustart am Rand und keine Polbehandlung wie der Kernel; ein echter Treffer knapp innerhalb
/// des Rands könnte so als Berührung knapp außerhalb erscheinen. Nahe am Rand ist der Strahl
/// daher unsicher, statt den Treffer zu raten.
#[allow(clippy::too_many_arguments)]
fn nurbs_hits(
    n: &planora_kernel::NurbsSurfaceData,
    patches: &[BezierPatch],
    o: DVec3,
    d: DVec3,
    (start, end): (f64, f64),
    e: f64,
    min_s: f64,
    (on_face, near_rim): FaceTests,
) -> Result<Vec<Candidate>, &'static str> {
    if let Some(h) = n.ray_hits_checked(o + d * start, d, end - start) {
        return Ok(h
            .into_iter()
            .map(|(t, u, v)| (t + start, Some((u, v))))
            .collect());
    }
    let (hits, touches) = patch_hits(n, patches, o, d, (start, end), IMPLICIT_LEAF_REL * e)?;
    let on_surface = n.signed_distance(o).abs() <= IN_SURFACE_REL * e;
    let skip = start + crate::limits::own_eps(e);
    for (t, size) in touches {
        if on_surface && t <= skip {
            continue;
        }
        if t > min_s {
            let p = o + d * t;
            if on_face(p) {
                return Err("Strahl streift eine Freiformfläche");
            }
            if near_rim(p, size) {
                return Err("Strahl streift den Rand einer Freiformfläche");
            }
        }
    }
    Ok(hits)
}

/// Schnitte des Strahls mit den Bézier-Flicken der Fläche durch Unterteilung (Gewichte positiv:
/// jeder Flicken liegt in der Hülle seiner Kontrollpunkte). Ein Flicken fällt weg, wenn der
/// Strahl seinen Hüllquader verfehlt oder ganz auf einer Seite seiner Schicht längs der Normale
/// in der Flickenmitte bleibt (streng). Ebene, nicht streifende Flicken (wie im Kernel:
/// Ebenheit ≤ 1e-3 · Diagonale, `|d·n| ≥ RAY_GRAZING_COS`) entscheidet Newton (Treffer im
/// Flicken oder keiner); streifende werden – anders als im Kernel – weiter geteilt, bis die
/// Diagonale `leaf` unterschreitet. Dort liefert Newton einen Treffer, sonst ist die Stelle
/// eine Berührung (Rückgabe: Strahlparameter und Flickendiagonale). Mehr als
/// `PATCH_MAX_VISITS` Flicken → unsicher.
fn patch_hits(
    n: &planora_kernel::NurbsSurfaceData,
    patches: &[BezierPatch],
    o: DVec3,
    d: DVec3,
    (start, end): (f64, f64),
    leaf: f64,
) -> Result<(Vec<Candidate>, Vec<Touch>), &'static str> {
    const UNSAFE: &str = "Freiformfläche nicht sicher entscheidbar";
    let sf = n.surface();
    let scale = patches
        .iter()
        .map(|q| {
            let (lo, hi) = q.bounds();
            lo.abs().max(hi.abs()).max_element()
        })
        .fold(o.abs().max_element(), f64::max)
        .max(1.0);
    let pad = PATCH_PAD_REL * scale;
    let mut stack: Vec<(BezierPatch, usize)> = patches.iter().map(|q| (q.clone(), 0)).collect();
    let mut visits = 0usize;
    let mut hits: Vec<Candidate> = Vec::new();
    let mut touches = Vec::new();
    while let Some((q, depth)) = stack.pop() {
        visits += 1;
        if visits > PATCH_MAX_VISITS {
            return Err(UNSAFE);
        }
        let (lo, hi) = q.bounds();
        let Some((t0, t1)) = ray_box(
            o,
            d,
            start,
            end,
            lo - DVec3::splat(pad),
            hi + DVec3::splat(pad),
        ) else {
            continue;
        };
        let (uc, vc) = (0.5 * (q.u.0 + q.u.1), 0.5 * (q.v.0 + q.v.1));
        let mut nrm = sf.normal(uc, vc);
        if nrm == DVec3::ZERO {
            // Pol oder entartete Mitte: Normale der Eckdiagonalen (jede Richtung ist zulässig,
            // die Schicht bleibt eine strenge Schranke).
            let c = &q.control;
            let (nu, nv) = (q.degree_u + 1, q.degree_v + 1);
            let a = c[nu * nv - 1] - c[0];
            let b = c[nv - 1] - c[(nu - 1) * nv];
            nrm = a.cross(b).normalize_or_zero();
        }
        if nrm != DVec3::ZERO {
            let (mut slo, mut shi) = (f64::INFINITY, f64::NEG_INFINITY);
            for c in &q.control {
                let x = nrm.dot(*c);
                slo = slo.min(x);
                shi = shi.max(x);
            }
            let (sa, sb) = (nrm.dot(o + d * t0), nrm.dot(o + d * t1));
            if (sa < slo - pad && sb < slo - pad) || (sa > shi + pad && sb > shi + pad) {
                continue;
            }
        }
        let diag = (hi - lo).length();
        let small = diag <= leaf || depth >= PATCH_MAX_DEPTH;
        let flat = q.flatness() <= PATCH_FLATNESS * diag;
        let grazing = nrm == DVec3::ZERO || nrm.dot(d).abs() < PATCH_GRAZING_COS;
        if small || (flat && !grazing) {
            match leaf_newton(sf, n, &q, o, d, (uc, vc), scale) {
                Some((t, u, v)) => {
                    if t >= start && t <= end {
                        hits.push((t, Some((u, v))));
                    }
                    continue;
                }
                None if small => {
                    touches.push(((0.5 * (lo + hi) - o).dot(d).clamp(start, end), diag));
                    continue;
                }
                // Ebener, nicht streifender Flicken ohne Konvergenz: weiter teilen.
                None => {}
            }
        }
        for c in q.subdivide() {
            stack.push((c, depth + 1));
        }
    }
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    let gap = PATCH_DEDUP_REL * scale;
    let mut out: Vec<Candidate> = Vec::with_capacity(hits.len());
    for h in hits {
        if out.last().is_none_or(|l| h.0 - l.0 > gap) {
            out.push(h);
        }
    }
    Ok((out, touches))
}

/// Newton auf `S(u, v) − o − t·d = 0` vom Flickenmittelpunkt aus; `Some((t, u, v))`, wenn er
/// konvergiert (Rest ≤ `PATCH_RESIDUAL_REL · Maß`) und `(u, v)` im Flicken liegt (± 1e-9 der
/// Bereichsbreite, geschlossene Richtungen auch um eine Periode versetzt).
fn leaf_newton(
    sf: &planora_math::nurbs::NurbsSurface,
    n: &planora_kernel::NurbsSurfaceData,
    q: &BezierPatch,
    o: DVec3,
    d: DVec3,
    (u0, v0): (f64, f64),
    scale: f64,
) -> Option<(f64, f64, f64)> {
    let limit = PATCH_RESIDUAL_REL * scale;
    let (mut u, mut v) = (u0, v0);
    let mut t = (sf.point(u, v) - o).dot(d);
    for _ in 0..PATCH_NEWTON_STEPS {
        let (wu, wv) = n.wrap(u, v);
        let dv = sf.derivs(wu, wv, 1);
        let r = dv[0][0] - o - d * t;
        if !r.is_finite() {
            return None;
        }
        if r.length() <= limit {
            break;
        }
        let (a, b, c) = (dv[1][0], dv[0][1], -d);
        let det = a.dot(b.cross(c));
        if !det.is_finite() || det.abs() <= 1e-14 * a.length() * b.length() {
            return None;
        }
        let rhs = -r;
        u += rhs.dot(b.cross(c)) / det;
        v += a.dot(rhs.cross(c)) / det;
        t += a.dot(b.cross(rhs)) / det;
    }
    let (wu, wv) = n.wrap(u, v);
    if (sf.point(wu, wv) - o - d * t).length() > limit {
        return None;
    }
    let ((du0, du1), (dv0, dv1)) = sf.domain();
    let inside = |x: f64, (a, b): (f64, f64), w: f64, periodic: bool| {
        let s = 1e-9 * w;
        let shifts: &[f64] = if periodic { &[0.0, -1.0, 1.0] } else { &[0.0] };
        shifts.iter().any(|k| {
            let y = x + k * w;
            y >= a - s && y <= b + s
        })
    };
    (inside(u, q.u, du1 - du0, n.is_periodic_u()) && inside(v, q.v, dv1 - dv0, n.is_periodic_v()))
        .then_some((t, wu, wv))
}
/// Nullstelle von `f` in `[a, b]` mit `f(a)` vom Vorzeichen `fa`, `f(b)` vom anderen.
fn bisect(f: &impl Fn(f64) -> f64, mut a: f64, mut b: f64, fa: f64) -> f64 {
    for _ in 0..BISECT_STEPS {
        let m = 0.5 * (a + b);
        if m <= a.min(b) || m >= a.max(b) {
            break;
        }
        let fm = f(m);
        if fm == 0.0 {
            return m;
        }
        if (fm < 0.0) == (fa < 0.0) {
            a = m;
        } else {
            b = m;
        }
    }
    0.5 * (a + b)
}

/// Kleinster Wert von `g` in `[a, b]` (Goldener Schnitt), mit den Randwerten verglichen.
fn golden_min(g: &impl Fn(f64) -> f64, a: f64, b: f64) -> (f64, f64) {
    let r = 0.5 * (5f64.sqrt() - 1.0);
    let (mut a, mut b) = (a, b);
    let mut x1 = b - r * (b - a);
    let mut x2 = a + r * (b - a);
    let (mut g1, mut g2) = (g(x1), g(x2));
    for _ in 0..GOLDEN_STEPS {
        if g1 < g2 {
            b = x2;
            x2 = x1;
            g2 = g1;
            x1 = b - r * (b - a);
            g1 = g(x1);
        } else {
            a = x1;
            x1 = x2;
            g1 = g2;
            x2 = a + r * (b - a);
            g2 = g(x2);
        }
    }
    let m = 0.5 * (a + b);
    [(m, g(m)), (x1, g1), (x2, g2)]
        .into_iter()
        .min_by(|p, q| p.1.total_cmp(&q.1))
        .expect("drei Werte")
}

/// Parameterintervall des Strahls `o + s·d`, `s ∈ [s0, s1]`, in der Box `[lo, hi]`.
fn ray_box(o: DVec3, d: DVec3, s0: f64, s1: f64, lo: DVec3, hi: DVec3) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (s0, s1);
    for k in 0..3 {
        if d[k] == 0.0 {
            if o[k] < lo[k] || o[k] > hi[k] {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo[k] - o[k]) / d[k], (hi[k] - o[k]) / d[k]);
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, t1))
}

/// Hüllbox der Fläche (nicht aufgebläht). Ebene, Zylinder, Kegel sind Regelflächen: Jede
/// Mantellinie durch einen Flächenpunkt verlässt die Fläche zu beiden Seiten über den Rand, der
/// Punkt liegt also zwischen zwei Randpunkten – die Hüllbox des Randes enthält die Fläche. Kugel
/// und Torus: Hüllbox der ganzen Fläche. Freiform: Hüllbox der Kontrollpunkte.
fn face_bbox(g: &FaceGeom) -> (DVec3, DVec3) {
    match &g.surface {
        Surface::Plane { .. } | Surface::Cylinder { .. } | Surface::Cone { .. } => {
            let mut lo = DVec3::splat(f64::INFINITY);
            let mut hi = DVec3::splat(f64::NEG_INFINITY);
            for s in g.loops.iter().flatten() {
                let (a, b) = seg_bbox(s);
                lo = lo.min(a);
                hi = hi.max(b);
            }
            (lo, hi)
        }
        Surface::Sphere { frame, radius } => (
            frame.origin - DVec3::splat(*radius),
            frame.origin + DVec3::splat(*radius),
        ),
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            let z = frame.z.normalize();
            let half = DVec3::new(
                major * (1.0 - z.x * z.x).max(0.0).sqrt() + minor,
                major * (1.0 - z.y * z.y).max(0.0).sqrt() + minor,
                major * (1.0 - z.z * z.z).max(0.0).sqrt() + minor,
            );
            (frame.origin - half, frame.origin + half)
        }
        Surface::Nurbs(n) => n.control_bounds(),
    }
}

/// Hüllbox eines Kurvenstücks: Geraden ihre Enden, Kreis und Ellipse dazu die Achsextrema im
/// Bereich, Freiformkurven ihre Kontrollpunkte.
fn seg_bbox(s: &Seg) -> (DVec3, DVec3) {
    let (mut lo, mut hi) = (s.start().min(s.end()), s.start().max(s.end()));
    let conic = match &s.curve {
        Curve::Circle { frame, radius } => Some((frame.x * *radius, frame.y * *radius)),
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => Some((frame.x * *major, frame.y * *minor)),
        _ => None,
    };
    if let Some((ax, ay)) = conic {
        let (a, b) = (s.t0.min(s.t1), s.t0.max(s.t1));
        for k in 0..3 {
            let t_star = ay[k].atan2(ax[k]);
            let k0 = ((a - t_star) / PI).ceil() as i64;
            let k1 = ((b - t_star) / PI).floor() as i64;
            for m in k0..=k1 {
                let p = s.curve.point(t_star + m as f64 * PI);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    if let Curve::Nurbs(c) = &s.curve {
        for &p in c.control_points() {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    (lo, hi)
}

/// Alle Zahlen der Trägerfläche endlich?
fn check_finite(s: &Surface) -> Result<(), DrawError> {
    let fr = s.frame();
    let frame_ok =
        fr.origin.is_finite() && fr.x.is_finite() && fr.y.is_finite() && fr.z.is_finite();
    let nums_ok = match s {
        Surface::Plane { .. } => true,
        Surface::Nurbs(n) => {
            let sf = n.surface();
            let (nu, nv) = sf.counts();
            (0..nu).all(|i| {
                (0..nv).all(|j| sf.control_at(i, j).is_finite() && sf.weight_at(i, j).is_finite())
            })
        }
        Surface::Cylinder { radius, .. } | Surface::Sphere { radius, .. } => radius.is_finite(),
        Surface::Cone { radius, slope, .. } => radius.is_finite() && slope.is_finite(),
        Surface::Torus { major, minor, .. } => major.is_finite() && minor.is_finite(),
    };
    if frame_ok && nums_ok {
        Ok(())
    } else {
        Err(DrawError::NonFinite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kleiner eigener Zufallszahlengenerator (xorshift64*), Werte in [0, 1).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
        }
        fn range(&mut self, a: f64, b: f64) -> f64 {
            a + (b - a) * self.next()
        }
        fn dir(&mut self) -> DVec3 {
            loop {
                let v = DVec3::new(
                    self.range(-1.0, 1.0),
                    self.range(-1.0, 1.0),
                    self.range(-1.0, 1.0),
                );
                if v.length() > 0.1 && v.length() <= 1.0 {
                    return v.normalize();
                }
            }
        }
    }

    fn tori() -> Vec<Surface> {
        let tilted = {
            let z = DVec3::new(0.3, -0.2, 1.0).normalize();
            let x = z.cross(DVec3::Y).normalize();
            Frame {
                origin: DVec3::new(1.5, -2.0, 0.5),
                x,
                y: z.cross(x),
                z,
            }
        };
        vec![
            Surface::Torus {
                frame: Frame::WORLD,
                major: 10.0,
                minor: 3.0,
            },
            Surface::Torus {
                frame: tilted,
                major: 7.0,
                minor: 0.5,
            },
            Surface::Torus {
                frame: Frame::WORLD,
                major: 4.0,
                minor: 3.5,
            },
        ]
    }

    /// Vergleich mit der früheren Abtastung: gleiche Treffer (auf 1e-9 genau) und gleiche
    /// Berührungsmeldungen – für Zufallsstrahlen, Strahlen knapp an Berührung (Höhe ±minor ± δ,
    /// Abstand zum Rohrkreis r ± δ) und Strahlen, die auf dem Torus beginnen.
    #[test]
    fn closed_form_torus_hits_agree_with_the_sampled_search() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let e = 30.0;
        let touch_tol = TOL_FACTOR * geom_tol(e);
        let (mut rays, mut hits, mut touches, mut lucky) = (0, 0, 0, 0);
        for surf in tori() {
            let Surface::Torus {
                frame,
                major,
                minor,
            } = &surf
            else {
                unreachable!()
            };
            let touch = (TORUS_TOUCH_REL * minor).max(touch_tol);
            let mut cases: Vec<(DVec3, DVec3, f64)> = Vec::new();
            for _ in 0..3000 {
                let o = DVec3::new(
                    rng.range(-20.0, 20.0),
                    rng.range(-20.0, 20.0),
                    rng.range(-20.0, 20.0),
                );
                cases.push((o, rng.dir(), 0.0));
            }
            // Fast berührende Strahlen: waagrecht (lokal) in der Höhe z = ±minor + δ.
            for k in 0..600 {
                let delta = [0.0, 1e-12, -1e-12, 1e-9, -1e-9, 1e-7, -1e-6, 1e-4][k % 8];
                let z = if k % 2 == 0 { *minor } else { -*minor } + delta;
                let a = rng.range(0.0, TAU_F);
                let off = rng.range(-major - minor, major + minor);
                let dl = DVec3::new(a.cos(), a.sin(), 0.0);
                let ol = DVec3::new(-a.sin(), a.cos(), 0.0) * off - dl * 30.0 + DVec3::Z * z;
                cases.push((
                    frame.point(ol.x, ol.y, ol.z),
                    frame.x * dl.x + frame.y * dl.y + frame.z * dl.z,
                    0.0,
                ));
            }
            // Strahlen vom Torus aus (Umriss- und Kantenpunkte), leicht gekippt.
            for _ in 0..600 {
                let (u, v) = (rng.range(0.0, TAU_F), rng.range(0.0, TAU_F));
                let p = surf.point(u, v);
                let n = surf.normal(u, v);
                let t = n.cross(rng.dir()).normalize();
                let dir = (t + n * rng.range(-1e-5, 1e-5)).normalize();
                cases.push((p, dir, 1e-7 * e));
            }
            for (o, d, min_s) in cases {
                rays += 1;
                let (lo, hi) = (min_s, 60.0);
                let new = torus_roots(&surf, touch, o, d, lo, hi, min_s);
                let old = torus_roots_sampled(&surf, touch, o, d, lo, hi, min_s);
                match (&new, &old) {
                    (Ok(a), Ok(b)) => {
                        assert_eq!(a.len(), b.len(), "{o:?} {d:?}: {a:?} gegen {b:?}");
                        for (x, y) in a.iter().zip(b) {
                            assert!((x - y).abs() <= 1e-9 * e, "{o:?} {d:?}: {a:?} gegen {b:?}");
                        }
                        hits += a.len();
                    }
                    (Err(_), Err(_)) => touches += 1,
                    // Die Abtastung fand zwei Schnitte einer Annäherung mit min |F| < touch nur,
                    // wenn zufällig eine Stützstelle zwischen sie fiel (sonst: Berührung). Die
                    // geschlossene Form meldet hier immer die Berührung.
                    (Err(_), Ok(b))
                        if b.windows(2).any(|w| {
                            let g = |s: f64| surf.implicit(o + d * s).abs();
                            w[1] - w[0] < 1e-2 && golden_min(&g, w[0], w[1]).1 < touch
                        }) =>
                    {
                        lucky += 1
                    }
                    _ => panic!("{o:?} {d:?} (min_s {min_s}): neu {new:?}, früher {old:?}"),
                }
            }
        }
        eprintln!(
            "{rays} Strahlen, {hits} Treffer, {touches} Berührungen, {lucky} Zufallstreffer der Abtastung"
        );
        assert!(touches > 100 && hits > 1000, "{touches} {hits}");
        assert!(lucky * 100 <= touches, "{lucky} von {touches}");
    }

    const TAU_F: f64 = std::f64::consts::TAU;

    #[test]
    fn rim_distance_is_the_distance_to_the_nearest_edge() {
        // Bodenfläche z = 0 des Quaders [0,10] × [0,20] × [0,30]: Punkt (3, 4, 0) → 3; Punkt
        // (3, 4, 2) über der Fläche → √(3² + 2²); Punkt (12, 25, 0) außerhalb → √(2² + 5²).
        let solid = planora_kernel::primitives::make_box(
            DVec3::new(10.0, 20.0, 30.0),
            &Tolerance::default(),
        )
        .unwrap();
        let (fid, _) = solid
            .faces
            .iter()
            .find(|(_, f)| {
                matches!(f.surface, Surface::Plane { frame } if frame.origin.z.abs() < 1e-12
                    && frame.z.z.abs() > 0.5)
            })
            .expect("Bodenfläche");
        let r = Region::new(face_geom(&solid, fid), 1e-9);
        for (p, want) in [
            (DVec3::new(3.0, 4.0, 0.0), 3.0),
            (DVec3::new(3.0, 4.0, 2.0), 13f64.sqrt()),
            (DVec3::new(12.0, 25.0, 0.0), 29f64.sqrt()),
        ] {
            assert!((r.rim_distance(p) - want).abs() < 1e-12, "{p:?}");
        }
    }

    #[test]
    fn a_touch_next_to_the_rim_of_a_freeform_face_is_unsafe() {
        // Kuppelflicken z = 4u(1−u)·4v(1−v) über [0,10]² (biquadratisch, Gipfel (5, 5, 1)).
        // Ein Strahl in Höhe 1 längs X berührt den Gipfel: Der Kernel verweigert ihn (streifend),
        // die Flickenteilung findet dort eine Berührung. Liegt die Stelle neben der beschnittenen
        // Fläche, aber nah an ihrem Rand, ist der Strahl unsicher (früher: übergangen); weit vom
        // Rand bleibt sie ohne Folge.
        use planora_math::nurbs::NurbsSurface;
        let h = [0.0, 2.0, 0.0];
        let mut control = Vec::new();
        for (i, hx) in h.iter().enumerate() {
            for (j, hy) in h.iter().enumerate() {
                control.push(DVec3::new(5.0 * i as f64, 5.0 * j as f64, hx * hy));
            }
        }
        let k = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let sf = NurbsSurface::new(2, 2, k.clone(), k, 3, 3, control, vec![1.0; 9]).unwrap();
        let n = planora_kernel::NurbsSurfaceData::new(sf, false, false).unwrap();
        assert!((n.surface().point(0.5, 0.5).z - 1.0).abs() < 1e-12);
        let patches = n.surface().bezier_patches().unwrap();
        let (o, d) = (DVec3::new(-5.0, 5.0, 1.0), DVec3::X);
        assert!(
            n.ray_hits_checked(o, d, 20.0).is_none(),
            "Kernel verweigert nicht"
        );
        let asked = std::cell::Cell::new(false);
        let run = |rim: bool| {
            nurbs_hits(
                &n,
                &patches,
                o,
                d,
                (0.0, 20.0),
                10.0,
                0.0,
                (&|_| false, &|_, _| {
                    asked.set(true);
                    rim
                }),
            )
        };
        assert!(matches!(run(true), Err(m) if m.contains("Rand")));
        assert!(asked.get());
        // Fern vom Rand: Die Berührung neben der Fläche bleibt ohne Folge (die beiden
        // Newton-Treffer am Gipfel selbst ordnet erst die Einordnung ein).
        let r = run(false);
        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn cubic_roots_are_exact() {
        // (x − 1)(x − 2)(x + 3) = x³ − 7x + 6; x³ + x + 1 hat eine Wurzel; x³ = 0.
        let mut r = depressed_cubic_roots(-7.0, 6.0);
        r.sort_by(f64::total_cmp);
        for (g, w) in r.iter().zip([-3.0, 1.0, 2.0]) {
            assert!((g - w).abs() < 1e-12, "{r:?}");
        }
        let r = depressed_cubic_roots(1.0, 1.0);
        assert_eq!(r.len(), 1);
        assert!((r[0] * r[0] * r[0] + r[0] + 1.0).abs() < 1e-14);
        assert_eq!(depressed_cubic_roots(0.0, 0.0), vec![0.0]);
    }
}
