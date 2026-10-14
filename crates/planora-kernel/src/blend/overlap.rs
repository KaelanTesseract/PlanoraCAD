//! Durchdringung im Flächeninneren (Korrekturwelle K1): Schneidet das Innere einer Fläche des
//! Ergebnisses das Innere einer neuen Rundungs-, Fasen- oder Eckfläche, ohne dass eine Kante
//! beteiligt ist (etwa eine Kugeltasche, die sich in den abgetragenen Bereich wölbt)?
//!
//! `check_piercing` prüft nur Kanten gegen die neue Fläche, `check_holes`/`check_crossings` nur
//! die Schleifen umgebauter Flächen. Hier wird Fläche gegen Fläche geschnitten (`intersect_surfaces`)
//! und jede Schnittkurve an den Stellen geteilt, an denen Randkanten der einen Fläche die andere
//! Trägerfläche treffen (wie beim Beschnitt der Booleschen Operationen); zwischen zwei Haltepunkten
//! ändert sich die Lage eines Kurvenstücks zu beiden Flächen nicht.
//!
//! Regeln (Schutzprinzip: im Zweifel benannt ablehnen):
//! - Sicher getrennt, dann keine weitere Prüfung: Hüllquader auseinander; Drehflächen mit
//!   parallelen Achsen, deren Bereiche von Abstand zur Achse oder Höhe sich verfehlen; Fläche ganz
//!   außerhalb der Kugel einer Kugelfläche; Fläche ganz auf einer Seite einer Ebene (bei
//!   benachbarten Flächen darf sie die Ebene berühren).
//! - **Nicht benachbarte** Flächen (keine gemeinsame Ecke oder Kante) dürfen keinen Punkt teilen:
//!   Jedes Kurvenstück, das in keiner der beiden Flächen außerhalb liegt, ist ein Treffer. Berühren,
//!   Zusammenfallen und nicht unterstützte Flächenpaare ergeben `Unknown` (benannte Ablehnung).
//! - **Benachbarte** Flächen teilen Rand: Treffer nur, wenn ein Kurvenstück im Inneren beider
//!   Flächen liegt. Berühren und nicht unterstützte Paare gelten hier als geprüft (ihr gemeinsamer
//!   Rand ist von den übrigen Prüfungen abgedeckt); das ist eine bekannte Restlücke.
//! - Gesucht werden gemeinsame Punkte. Eine Fläche, die ganz im Hohlraum einer Tasche läge, ohne
//!   sie zu berühren, erkennt diese Prüfung nicht (dafür müssten Ränder durch die Tasche laufen,
//!   was `check_piercing`, `check_holes` und `check_crossings` sehen).

use super::build::{Aabb, ON_CURVE_FACTOR, apart, seg_box};
use crate::boolean::{
    Containment, CurveSurface, FaceGeom, Piece, Seg, SurfaceIntersection, curve_surface_roots,
    intersect_surfaces, loop_points, point_in_face,
};
use crate::geometry::{Curve, Surface, UvRect};
use crate::validate::edge_surface_tol;
use planora_math::{DVec3, PI, TAU, Tolerance};

/// Kleinster Parameterabstand zweier Haltepunkte, der noch ein eigenes Kurvenstück ergibt (wie
/// `MIN_PARAM_SPAN` im Beschnitt der Booleschen Operationen).
const MIN_PARAM_SPAN: f64 = 1e-9;
/// Bruchteile eines Kurvenstücks, an denen seine Lage befragt wird: die Mitte zuerst, weitere nur,
/// wenn ein Punkt zufällig auf dem Rand liegt (wie `classify` im Beschnitt).
const PROBES: [f64; 5] = [0.5, 0.381_966, 0.618_034, 0.25, 0.75];

/// Ergebnis der Prüfung eines Flächenpaars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Crossing {
    /// Die Flächen durchdringen sich nicht.
    Clear,
    /// Die Flächen durchdringen (oder, nicht benachbart, berühren) sich.
    Hit,
    /// Lässt sich nicht sicher entscheiden (Flächenpaar ohne Schnittrechner, Berühren).
    Unknown,
}

/// Alle Randstücke einer Fläche (Außenschleife und Löcher).
fn segs(p: &Piece) -> impl Iterator<Item = &Seg> {
    std::iter::once(&p.outer).chain(p.holes.iter()).flatten()
}

fn geom(p: &Piece) -> FaceGeom {
    FaceGeom {
        surface: p.surface.clone(),
        same_sense: p.same_sense,
        domain: p.domain,
        loops: std::iter::once(p.outer.clone())
            .chain(p.holes.iter().cloned())
            .collect(),
        tag: None,
    }
}

/// Enger Hüllquader eines Kurvenstücks: Kreis- und Ellipsenbögen über ihre Enden und die
/// Extrempunkte je Koordinate, die im Parameterbereich liegen (Koordinate `i` ist
/// `o_i + a_i cos t + b_i sin t`, extrem bei `t = atan2(b_i, a_i)` und gegenüber); sonst
/// `seg_box`.
fn tight_seg_box(seg: &Seg) -> Option<Aabb> {
    let (o, a, b) = match &seg.curve {
        Curve::Circle { frame, radius } => (frame.origin, frame.x * *radius, frame.y * *radius),
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => (frame.origin, frame.x * *major, frame.y * *minor),
        _ => return seg_box(seg),
    };
    let (t0, t1) = (seg.t0.min(seg.t1), seg.t0.max(seg.t1));
    if !(t0.is_finite() && t1.is_finite()) {
        return None;
    }
    let (p0, p1) = (seg.start(), seg.end());
    let (mut lo, mut hi) = (p0.min(p1), p0.max(p1));
    let point = |t: f64| o + a * t.cos() + b * t.sin();
    for i in 0..3 {
        let phase = b[i].atan2(a[i]);
        for base in [phase, phase + PI] {
            // kleinstes base + k·2π ≥ t0
            let t = base + ((t0 - base) / TAU).ceil() * TAU;
            if t <= t1 {
                let q = point(t);
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
    }
    Some((lo, hi))
}

/// Vereinigung der Hüllquader aller Randstücke (`None`, wenn einer fehlt).
fn loops_box(p: &Piece) -> Option<Aabb> {
    let mut out: Option<Aabb> = None;
    for s in segs(p) {
        let (lo, hi) = tight_seg_box(s)?;
        out = Some(match out {
            Some((a, b)) => (a.min(lo), b.max(hi)),
            None => (lo, hi),
        });
    }
    out
}

/// Zuschlag zum Quader von Kugel- und Torusstücken, relativ zur Größe der Fläche: Die Lageprüfung
/// eines Extrempunkts nähert den Rand durch Stützpunkte an (Sehnenabweichung höchstens etwa
/// 1e-3 · Radius bei 72 Stützpunkten je Umlauf). Liegt der Extrempunkt um diese Weite innerhalb und
/// wird als außen gewertet, überragt die Fläche den Rand um höchstens etwa (1e-3)²/2 · Radius.
const CANDIDATE_SLACK: f64 = 1e-6;
/// Sinus, unter dem eine Weltachse als parallel zur Achse eines Torus gilt (dann sind die
/// Extrempunkte Kreise statt einzelner Punkte); der Quader wächst dafür um diesen Anteil der
/// Torusgröße.
const AXIS_SIN: f64 = 1e-9;

/// Sicherer Hüllquader einer beschnittenen Fläche (`None`: unbekannt, kein Vorfilter).
///
/// Grundsatz: Das Größte einer Koordinate (Richtung `±e_i`) über der Fläche liegt auf ihrem Rand
/// oder in einem inneren kritischen Punkt (Normale parallel zu `e_i`).
/// - Ebene, Zylinder, Kegel: Kritische Punkte bilden ganze Mantellinien, längs derer die
///   Koordinate gleich bleibt; das Stück einer solchen Linie in der Fläche endet auf dem Rand. Der
///   Quader der Ränder (`seg_box`) genügt also.
/// - Kugel: dazu die sechs Pole `c ± r·e_i`, sofern sie in der Fläche liegen.
/// - Torus: dazu je Richtung die vier Punkte mit Normale `±e_i` (bei achsparalleler Richtung der
///   obere bzw. untere Kreis des Rohrs: Liegt er teilweise in der Fläche, kreuzt er den Rand, liegt
///   er ganz darin, zeigt das ein einzelner Punkt).
/// - Freiformfläche: Hülle der Kontrollpunkte.
pub(super) fn face_box(p: &Piece, tol: &Tolerance) -> Option<Aabb> {
    let finite = |b: Aabb| (b.0.is_finite() && b.1.is_finite()).then_some(b);
    if let Surface::Nurbs(d) = &p.surface {
        return finite(d.control_bounds());
    }
    let (mut lo, mut hi) = loops_box(p)?;
    let mut candidates: Vec<DVec3> = Vec::new();
    let mut grow = 0.0;
    let axes = [DVec3::X, DVec3::Y, DVec3::Z];
    match &p.surface {
        Surface::Sphere { frame, radius } => {
            grow = CANDIDATE_SLACK * radius.abs();
            for d in axes {
                candidates.push(frame.origin + d * radius.abs());
                candidates.push(frame.origin - d * radius.abs());
            }
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            grow = (AXIS_SIN + CANDIDATE_SLACK) * (major.abs() + minor.abs());
            for d in axes {
                let dz = d.dot(frame.z);
                let dr = d - frame.z * dz;
                let rho = dr.length();
                if rho <= AXIS_SIN {
                    for v in [PI / 2.0, -PI / 2.0] {
                        candidates.push(p.surface.point(0.0, v));
                    }
                } else {
                    let u = dr.dot(frame.y).atan2(dr.dot(frame.x));
                    for (uu, c) in [(u, rho), (u + PI, -rho)] {
                        for s in [1.0, -1.0] {
                            candidates.push(p.surface.point(uu, (s * dz).atan2(s * c)));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    if !candidates.is_empty() {
        let g = geom(p);
        for q in candidates {
            if point_in_face(&g, q, tol) != Containment::Outside {
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
    }
    finite((lo - DVec3::splat(grow), hi + DVec3::splat(grow)))
}

/// Teilen die Flächen eine Ecke (ein Randstückende der einen fällt mit einem der anderen
/// zusammen)? Nach dem Umbau enden Berühr- und Abschlusskurven an Ecken der Nachbarflächen. Eine
/// übersehene Nachbarschaft (etwa eine Ecke mitten auf einer Kante) ist sicher: Sie macht die
/// Prüfung nur strenger (dann zählt schon Berühren).
fn adjacent(a: &Piece, b: &Piece, weld: f64) -> bool {
    let ends = |x: &Piece| -> Vec<DVec3> { segs(x).flat_map(|s| [s.start(), s.end()]).collect() };
    let eb = ends(b);
    ends(a)
        .iter()
        .any(|p| eb.iter().any(|q| p.distance(*q) <= weld))
}

/// Punkte, an denen die Randkante `e` die Trägerfläche `other` trifft (wie `edge_hits` im
/// Beschnitt): Nullstellen, Kantenenden innerhalb der Toleranz auf der Fläche, bei ganz
/// aufliegenden Kanten ihre Enden.
fn edge_hits(e: &Seg, other: &Surface, tol: &Tolerance) -> Vec<DVec3> {
    let (lo, hi) = (e.t0.min(e.t1), e.t0.max(e.t1));
    match curve_surface_roots(&e.curve, lo, hi, other, tol) {
        CurveSurface::Roots(rs) => {
            let mut out: Vec<DVec3> = rs.iter().map(|r| e.curve.point(r.t)).collect();
            for p in [e.start(), e.end()] {
                if other.implicit(p).abs() <= edge_surface_tol(&e.curve, other, tol) {
                    out.push(p);
                }
            }
            out
        }
        CurveSurface::Lies => vec![e.start(), e.end()],
    }
}

/// Parameter von `p` auf der Kurve, falls `p` (innerhalb der Toleranz) auf ihr liegt; bei
/// geschlossenen Kurven in eine Periode gefaltet.
fn param_on_curve(c: &Curve, p: DVec3, tol: &Tolerance) -> Option<f64> {
    let t = c.project(p);
    let t = match c.period() {
        Some(period) => {
            let a = c.domain().map_or(0.0, |(a, _)| a);
            a + (t - a).rem_euclid(period)
        }
        None => t,
    };
    (c.point(t).distance(p) <= tol.linear * ON_CURVE_FACTOR).then_some(t)
}

/// Lage eines Kurvenstücks zur Fläche (erster Probepunkt, der nicht auf dem Rand liegt).
fn classify(face: &FaceGeom, seg: &Seg, tol: &Tolerance) -> Containment {
    for f in PROBES {
        let c = point_in_face(face, seg.at(f), tol);
        if c != Containment::Boundary {
            return c;
        }
    }
    Containment::Boundary
}

/// Liegen Randpunkte der einen Fläche in der anderen (beide auf derselben Trägerfläche)?
/// `strict`: nur das Innere zählt (benachbarte Flächen), sonst auch der Rand.
fn coincident_overlap(a: &FaceGeom, b: &FaceGeom, strict: bool, tol: &Tolerance) -> bool {
    let hit = |c: Containment| match c {
        Containment::Inside => true,
        Containment::Boundary => !strict,
        Containment::Outside => false,
    };
    [(a, b), (b, a)].iter().any(|(x, y)| {
        x.loops
            .iter()
            .flat_map(|l| {
                loop_points(l)
                    .into_iter()
                    .chain(l.iter().map(|s| s.at(0.5)))
            })
            .any(|p| hit(point_in_face(y, p, tol)))
    })
}

/// Stützpunkte je Randstück für die Lage zur Achse (`meridian_box`) und, bei Ellipsen und
/// Freiformkurven, für den Abstand zu einem Kugelmittelpunkt.
const DISTANCE_SAMPLES: usize = 256;
/// Sinus, unter dem zwei Achsen als parallel gelten.
const PARALLEL_SIN: f64 = 1e-12;

/// Bereich `(ρ_min, ρ_max, h_min, h_max)` von Abstand zur Achse und Höhe längs der Achse.
type Meridian = (f64, f64, f64, f64);

/// Drehachse (Ursprung, Richtung) eines Zylinders, Kegels oder Torus; Kugeln haben jede Achse
/// durch ihren Mittelpunkt (`None` als Richtung).
fn axis(s: &Surface) -> Option<(DVec3, Option<DVec3>)> {
    match s {
        Surface::Cylinder { frame, .. }
        | Surface::Cone { frame, .. }
        | Surface::Torus { frame, .. } => Some((frame.origin, Some(frame.z))),
        Surface::Sphere { frame, .. } => Some((frame.origin, None)),
        _ => None,
    }
}

/// Bereich von Abstand zur Achse `(o, z)` und Höhe längs ihr über der beschnittenen Drehfläche `p`,
/// deren Drehachse `(o, z)` ist (bei Kugeln jede Achse durch den Mittelpunkt).
///
/// Rand: Stichproben, erweitert um die größte Sehne zwischen zwei Stichproben (Abstand und Höhe
/// ändern sich höchstens so schnell wie die Bogenlänge, die Sehne schätzt sie). Inneres: Extreme
/// von Abstand und Höhe liegen sonst auf Parallelkreisen (Äquatoren, oberer/unterer Rohrkreis des
/// Torus) oder in Polen und Kegelspitzen; ein solcher Kreis kreuzt den Rand oder liegt ganz in der
/// Fläche, dann zeigt das ein einzelner Punkt.
fn meridian_box(p: &Piece, o: DVec3, z: DVec3, tol: &Tolerance) -> Option<Meridian> {
    let rho_h = |q: DVec3| {
        let d = q - o;
        let h = d.dot(z);
        ((d - z * h).length(), h)
    };
    let (mut r0, mut r1, mut h0, mut h1) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    let mut take = |q: DVec3, slack: f64| {
        let (r, h) = rho_h(q);
        r0 = r0.min(r - slack);
        r1 = r1.max(r + slack);
        h0 = h0.min(h - slack);
        h1 = h1.max(h + slack);
    };
    for s in segs(p) {
        let pts: Vec<DVec3> = (0..=DISTANCE_SAMPLES)
            .map(|k| s.at(k as f64 / DISTANCE_SAMPLES as f64))
            .collect();
        let gap = pts
            .windows(2)
            .map(|w| w[0].distance(w[1]))
            .fold(0.0, f64::max);
        for q in pts {
            take(q, gap);
        }
    }
    // Ein Punkt senkrecht zur Achse (für Kreispunkte).
    let w = z.any_orthonormal_vector();
    let mut candidates: Vec<DVec3> = Vec::new();
    let mut size = 0.0;
    match &p.surface {
        Surface::Cylinder { .. } => {}
        Surface::Cone {
            frame,
            radius,
            slope,
        } => {
            if slope.abs() > PARALLEL_SIN {
                candidates.push(frame.origin + frame.z * (-radius / slope));
            }
        }
        Surface::Sphere { frame, radius } => {
            size = radius.abs();
            let c = frame.origin;
            candidates.extend([
                c + z * radius.abs(),
                c - z * radius.abs(),
                c + w * radius.abs(),
            ]);
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            size = major.abs() + minor.abs();
            let (x, c) = (frame.x, frame.origin);
            candidates.extend([
                c + x * (major + minor),
                c + x * (major - minor),
                c + x * *major + frame.z * *minor,
                c + x * *major - frame.z * *minor,
            ]);
        }
        _ => return None,
    }
    if !candidates.is_empty() {
        let g = geom(p);
        for q in candidates {
            if point_in_face(&g, q, tol) != Containment::Outside {
                take(q, CANDIDATE_SLACK * size);
            }
        }
    }
    let r0 = r0 - CANDIDATE_SLACK * size;
    let r1 = r1 + CANDIDATE_SLACK * size;
    let all = [r0, r1, h0, h1];
    all.iter()
        .all(|v| v.is_finite())
        .then_some((r0.max(0.0), r1, h0, h1))
}

/// Liegen zwei Drehflächen mit parallelen Achsen (oder eine davon eine Kugel) sicher getrennt?
/// Bezug ist die Achse `(o, z)` von `a`. Jeder Parallelkreis von `b` (Radius `s` um die eigene
/// Achse im Abstand `D`) hat Punkte im Abstand `|D − s| … D + s` von der Achse von `a`; die Höhen
/// verschieben sich um den Abstand der Ursprünge längs `z`. Getrennt, wenn sich die Bereiche von
/// Abstand oder Höhe um mehr als `margin` verfehlen. Sonst (oder nicht anwendbar) `false`.
fn revolved_apart(a: &Piece, b: &Piece, margin: f64, tol: &Tolerance) -> bool {
    let (Some((oa, za)), Some((ob, zb))) = (axis(&a.surface), axis(&b.surface)) else {
        return false;
    };
    // Bezugsrichtung: eine echte Drehachse (eine Kugel ist um jede Achse drehsymmetrisch).
    let Some(z) = za.or(zb) else {
        return false;
    };
    let zb = zb.unwrap_or(z);
    if z.cross(zb).length() > PARALLEL_SIN {
        return false;
    }
    let (Some(ma), Some(mb)) = (meridian_box(a, oa, z, tol), meridian_box(b, ob, z, tol)) else {
        return false;
    };
    let d = oa - ob;
    let off = d.dot(z);
    let dist = (d - z * off).length();
    let (s0, s1) = (mb.0, mb.1);
    let rho_b = ((dist - s1).max(s0 - dist).max(0.0), dist + s1);
    // Höhe eines Punkts von `b` über `oa`: eigene Höhe über `ob` minus `off`.
    let h_b = (mb.2 - off, mb.3 - off);
    rho_b.0 > ma.1 + margin
        || ma.0 > rho_b.1 + margin
        || h_b.0 > ma.3 + margin
        || ma.2 > h_b.1 + margin
}

/// Untere Schranke für den Abstand der Punkte eines Randstücks von `c`. Gerade und Kreisbogen
/// genau (Lotfußpunkt bzw. Extremwinkel, sonst die Enden); sonst Stichproben, verkleinert um die
/// größte Sehne zwischen zwei Stichproben (der Abstand ändert sich höchstens so schnell wie die
/// Bogenlänge).
fn seg_distance(s: &Seg, c: DVec3) -> f64 {
    let ends = s.start().distance(c).min(s.end().distance(c));
    match &s.curve {
        Curve::Line { .. } => ends.min(s.closest(c).1),
        Curve::Circle { frame, radius } => {
            let (t0, t1) = (s.t0.min(s.t1), s.t0.max(s.t1));
            let l = frame.to_local(c);
            let phase = l.y.atan2(l.x);
            let t = phase + ((t0 - phase) / TAU).ceil() * TAU;
            if t <= t1 {
                let q = frame.origin + (frame.x * t.cos() + frame.y * t.sin()) * *radius;
                ends.min(q.distance(c))
            } else {
                ends
            }
        }
        _ => {
            let pts: Vec<DVec3> = (0..=DISTANCE_SAMPLES)
                .map(|k| s.at(k as f64 / DISTANCE_SAMPLES as f64))
                .collect();
            let gap = pts
                .windows(2)
                .map(|w| w[0].distance(w[1]))
                .fold(0.0, f64::max);
            pts.iter().map(|q| q.distance(c) - gap).fold(ends, f64::min)
        }
    }
}

/// Liegt die Fläche `other` sicher außerhalb der ganzen Kugel der Kugelfläche `ball`? Der Abstand zum Mittelpunkt `c` ist über der Fläche am Rand oder in einem
/// Fußpunkt extrem (Lot von `c` auf die Trägerfläche): Ebene ein Punkt, Zylinder zwei (Gerade
/// durch die Achse; liegt `c` auf der Achse, ein Kreis, für den ein Punkt genügt), Kugel zwei,
/// Torus vier (in der Meridianebene von `c`). Kegel und Freiformflächen: nicht anwendbar.
fn ball_apart(ball: &Piece, other: &Piece, margin: f64, tol: &Tolerance) -> bool {
    let Surface::Sphere { frame, radius } = &ball.surface else {
        return false;
    };
    let (c, r) = (frame.origin, radius.abs());
    // Richtung von der Achse `(o, z)` zu `c` (beliebig, wenn `c` auf der Achse liegt).
    let radial = |o: DVec3, z: DVec3| {
        let d = c - o;
        let w = d - z * d.dot(z);
        if w.length() > PARALLEL_SIN * (1.0 + d.length()) {
            w.normalize()
        } else {
            z.any_orthonormal_vector()
        }
    };
    let candidates: Vec<DVec3> = match &other.surface {
        Surface::Plane { frame: f } => vec![c - f.z * (c - f.origin).dot(f.z)],
        Surface::Cylinder {
            frame: f,
            radius: rc,
        } => {
            let w = radial(f.origin, f.z);
            let foot = f.origin + f.z * (c - f.origin).dot(f.z);
            vec![foot + w * rc.abs(), foot - w * rc.abs()]
        }
        Surface::Sphere {
            frame: f,
            radius: rs,
        } => {
            let d = c - f.origin;
            let w = if d.length() > PARALLEL_SIN {
                d.normalize()
            } else {
                DVec3::X
            };
            vec![f.origin + w * rs.abs(), f.origin - w * rs.abs()]
        }
        Surface::Torus {
            frame: f,
            major,
            minor,
        } => {
            let w = radial(f.origin, f.z);
            let mut out = Vec::new();
            for q in [f.origin + w * *major, f.origin - w * *major] {
                let d = c - q;
                let u = if d.length() > PARALLEL_SIN {
                    d.normalize()
                } else {
                    f.z
                };
                out.extend([q + u * minor.abs(), q - u * minor.abs()]);
            }
            out
        }
        _ => return false,
    };
    let mut lo = segs(other)
        .map(|s| seg_distance(s, c))
        .fold(f64::INFINITY, f64::min);
    let g = geom(other);
    for q in candidates {
        if point_in_face(&g, q, tol) != Containment::Outside {
            lo = lo.min(q.distance(c));
        }
    }
    let slack = CANDIDATE_SLACK * (r + 1.0);
    // Nur „ganz außerhalb“ zählt; eine Fläche ganz im Inneren der Kugel (im Hohlraum einer
    // Tasche) wird bewusst nicht als getrennt gewertet. NaN ergibt `false`.
    lo - slack > r + margin
}

/// Punkte, in deren konvexer Hülle die Fläche liegt: bei Freiformflächen die Kontrollpunkte
/// (positive Gewichte), sonst die Ecken des sicheren Hüllquaders.
fn hull_points(p: &Piece, tol: &Tolerance) -> Option<Vec<DVec3>> {
    if let Surface::Nurbs(d) = &p.surface {
        let s = d.surface();
        let (nu, nv) = s.counts();
        return Some(
            (0..nu)
                .flat_map(|i| (0..nv).map(move |j| s.control_at(i, j)))
                .collect(),
        );
    }
    let (lo, hi) = face_box(p, tol)?;
    Some(
        (0..8)
            .map(|k| {
                DVec3::new(
                    if k & 1 == 0 { lo.x } else { hi.x },
                    if k & 2 == 0 { lo.y } else { hi.y },
                    if k & 4 == 0 { lo.z } else { hi.z },
                )
            })
            .collect(),
    )
}

/// Liegt `other` ganz auf einer Seite der Ebene der ebenen Fläche `plane`? `touching`: Berühren
/// (bis `margin`) ist erlaubt (benachbarte Flächen: dann kann sich das Innere nicht kreuzen),
/// sonst muss der Abstand größer als `margin` sein. Billiger Vorfilter vor dem Flächenschnitt
/// (der für Freiformflächen teuer ist).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn plane_side_clear(
    plane: &Piece,
    other: &Piece,
    touching: bool,
    margin: f64,
    tol: &Tolerance,
) -> bool {
    let Surface::Plane { frame } = &plane.surface else {
        return false;
    };
    let Some(pts) = hull_points(other, tol) else {
        return false;
    };
    let limit = if touching { -margin } else { margin };
    let d: Vec<f64> = pts
        .iter()
        .map(|p| (*p - frame.origin).dot(frame.z))
        .collect();
    // NaN ergibt `false` in beiden Richtungen.
    d.iter().all(|&x| x >= limit) || d.iter().all(|&x| -x >= limit)
}

/// Höchstzahl Zellenpaare, die `patches_apart` prüft, bevor es aufgibt.
const PATCH_PAIR_BUDGET: usize = 4000;
/// Zuschlag zum Parameterbereich einer Fläche (Anteil seiner Breite): Der Bereich wird aus
/// Stützpunkten der Schleife bestimmt und kann den wahren Bereich um eine Sehnenabweichung
/// unterschätzen.
const DOMAIN_MARGIN: f64 = 1e-2;

/// Schranken der zweiten Ableitungen `(|P_uu|, |P_vv|)` einer analytischen Fläche über dem
/// Parameterstreifen `v ∈ [v0, v1]` (`None` für Freiformflächen).
fn second_derivative_bounds(s: &Surface, v0: f64, v1: f64) -> Option<(f64, f64)> {
    match s {
        Surface::Plane { .. } => Some((0.0, 0.0)),
        Surface::Cylinder { radius, .. } => Some((radius.abs(), 0.0)),
        Surface::Cone { radius, slope, .. } => Some((
            (radius + slope * v0).abs().max((radius + slope * v1).abs()),
            0.0,
        )),
        Surface::Sphere { radius, .. } => Some((radius.abs(), radius.abs())),
        Surface::Torus { major, minor, .. } => Some((major.abs() + minor.abs(), minor.abs())),
        Surface::Nurbs(_) => None,
    }
}

/// Sicherer Hüllquader des Flächenstücks über der Parameterzelle: Quader der vier Eckpunkte,
/// erweitert um den Fehler der bilinearen Interpolation (`du²/8 · |P_uu| + dv²/8 · |P_vv|`; die
/// bilineare Fläche liegt in der konvexen Hülle der Ecken).
fn cell_box(s: &Surface, c: &UvRect) -> Option<Aabb> {
    let (puu, pvv) = second_derivative_bounds(s, c.v0, c.v1)?;
    let (du, dv) = (c.u1 - c.u0, c.v1 - c.v0);
    let sag = (du * du * puu + dv * dv * pvv) / 8.0;
    let corners = [
        s.point(c.u0, c.v0),
        s.point(c.u1, c.v0),
        s.point(c.u0, c.v1),
        s.point(c.u1, c.v1),
    ];
    let lo = corners
        .iter()
        .fold(DVec3::splat(f64::INFINITY), |a, p| a.min(*p));
    let hi = corners
        .iter()
        .fold(DVec3::splat(f64::NEG_INFINITY), |a, p| a.max(*p));
    let b = (lo - DVec3::splat(sag), hi + DVec3::splat(sag));
    (b.0.is_finite() && b.1.is_finite()).then_some(b)
}

/// Schranken der ersten Ableitungen `(|P_u|, |P_v|)` über dem Streifen `v ∈ [v0, v1]`.
fn first_derivative_bounds(s: &Surface, v0: f64, v1: f64) -> (f64, f64) {
    match s {
        Surface::Plane { .. } | Surface::Nurbs(_) => (1.0, 1.0),
        Surface::Cylinder { radius, .. } => (radius.abs(), 1.0),
        Surface::Cone { radius, slope, .. } => (
            (radius + slope * v0).abs().max((radius + slope * v1).abs()),
            (1.0 + slope * slope).sqrt(),
        ),
        Surface::Sphere { radius, .. } => (radius.abs(), radius.abs()),
        Surface::Torus { major, minor, .. } => (major.abs() + minor.abs(), minor.abs()),
    }
}

/// Zelle in zwei Hälften längs der Richtung mit der größeren Ausdehnung im Raum (Parameterbreite
/// mal Schranke der Ableitung; Eckpunkte taugen nicht, bei einem vollen Umlauf fallen sie
/// zusammen).
fn split_cell(s: &Surface, c: &UvRect) -> [UvRect; 2] {
    let (lu, lv) = first_derivative_bounds(s, c.v0, c.v1);
    if (c.u1 - c.u0) * lu >= (c.v1 - c.v0) * lv {
        let m = 0.5 * (c.u0 + c.u1);
        [UvRect { u1: m, ..*c }, UvRect { u0: m, ..*c }]
    } else {
        let m = 0.5 * (c.v0 + c.v1);
        [UvRect { v1: m, ..*c }, UvRect { v0: m, ..*c }]
    }
}

/// Liegen die Trägerflächenstücke über den (erweiterten) Parameterbereichen beider Flächen sicher
/// auseinander? Zellenpaare werden geteilt, bis ihre sicheren Quader getrennt sind; reicht das
/// Budget nicht oder fehlt ein Bereich (Freiformfläche, kein Bereich), dann `false`. Die Bereiche
/// umfassen die beschnittenen Flächen (P10), die Prüfung ist also eher zu streng.
fn patches_apart(a: &Piece, b: &Piece, margin: f64) -> bool {
    let grow = |d: UvRect| {
        let (gu, gv) = (DOMAIN_MARGIN * (d.u1 - d.u0), DOMAIN_MARGIN * (d.v1 - d.v0));
        UvRect {
            u0: d.u0 - gu,
            u1: d.u1 + gu,
            v0: d.v0 - gv,
            v1: d.v1 + gv,
        }
    };
    let (Some(da), Some(db)) = (a.domain.map(grow), b.domain.map(grow)) else {
        return false;
    };
    let finite = |d: &UvRect| [d.u0, d.u1, d.v0, d.v1].iter().all(|x| x.is_finite());
    if !(finite(&da) && finite(&db)) {
        return false;
    }
    let mut stack = vec![(da, db)];
    let mut budget = PATCH_PAIR_BUDGET;
    while let Some((ca, cb)) = stack.pop() {
        let (Some(ba), Some(bb)) = (cell_box(&a.surface, &ca), cell_box(&b.surface, &cb)) else {
            return false;
        };
        if apart(&ba, &bb, margin) {
            continue;
        }
        if budget == 0 {
            return false;
        }
        budget -= 1;
        let (sa, sb) = ((ba.1 - ba.0).length(), (bb.1 - bb.0).length());
        if sa >= sb {
            for h in split_cell(&a.surface, &ca) {
                stack.push((h, cb));
            }
        } else {
            for h in split_cell(&b.surface, &cb) {
                stack.push((ca, h));
            }
        }
    }
    true
}

/// Prüft ein Flächenpaar. `tol` ist die Modelltoleranz (für den Flächenschnitt), `ptol` die
/// Toleranz des Umbaus (Haltepunkte, Lage; mit Freiformflächen die weitere Knotentoleranz).
pub(super) fn crossing(a: &Piece, b: &Piece, tol: &Tolerance, ptol: &Tolerance) -> Crossing {
    if let (Some(x), Some(y)) = (face_box(a, ptol), face_box(b, ptol))
        && apart(&x, &y, ON_CURVE_FACTOR * ptol.linear)
    {
        return Crossing::Clear;
    }
    let near = adjacent(a, b, ON_CURVE_FACTOR * ptol.linear);
    let margin = ON_CURVE_FACTOR * ptol.linear;
    if revolved_apart(a, b, margin, ptol)
        || ball_apart(a, b, margin, ptol)
        || ball_apart(b, a, margin, ptol)
        || plane_side_clear(a, b, near, margin, ptol)
        || plane_side_clear(b, a, near, margin, ptol)
    {
        return Crossing::Clear;
    }
    let (ga, gb) = (geom(a), geom(b));
    let curves = match intersect_surfaces(&a.surface, &b.surface, tol) {
        SurfaceIntersection::None => return Crossing::Clear,
        SurfaceIntersection::Touch
        | SurfaceIntersection::Unsupported
        | SurfaceIntersection::UnsupportedBecause(_) => {
            return if near || patches_apart(a, b, margin) {
                Crossing::Clear
            } else {
                Crossing::Unknown
            };
        }
        SurfaceIntersection::Coincident => {
            return if coincident_overlap(&ga, &gb, near, ptol) {
                Crossing::Hit
            } else {
                Crossing::Clear
            };
        }
        SurfaceIntersection::Curves(cs) => cs,
    };
    let mut hits: Vec<DVec3> = Vec::new();
    for (own, other) in [(a, &b.surface), (b, &a.surface)] {
        for e in segs(own) {
            hits.extend(edge_hits(e, other, ptol));
        }
    }
    for s in [&a.surface, &b.surface] {
        for pv in s.pole_vs() {
            hits.push(s.point(0.0, pv));
        }
    }
    for c in curves {
        let mut ts: Vec<f64> = Vec::new();
        let mut pts: Vec<DVec3> = Vec::new();
        let mut add = |t: f64| {
            let q = c.point(t);
            if !pts.iter().any(|x| x.distance(q) <= ptol.linear) {
                pts.push(q);
                ts.push(t);
            }
        };
        for p in &hits {
            if let Some(t) = param_on_curve(&c, *p, ptol) {
                add(t);
            }
        }
        // Offene Freiformkurven (Äste des numerischen Schnittrechners): Enden sind Haltepunkte.
        if let (Curve::Nurbs(_), None, Some((t0, t1))) = (&c, c.period(), c.domain()) {
            add(t0);
            add(t1);
        }
        ts.sort_by(f64::total_cmp);
        let mut spans: Vec<(f64, f64)> = ts.windows(2).map(|w| (w[0], w[1])).collect();
        if let Some(period) = c.period() {
            match (ts.first(), ts.last()) {
                (Some(&first), Some(&last)) => spans.push((last, first + period)),
                _ => {
                    let t0 = c.domain().map_or(0.0, |(t0, _)| t0);
                    spans.push((t0, t0 + period));
                }
            }
        }
        for (t0, t1) in spans {
            if (t1 - t0).abs() <= MIN_PARAM_SPAN {
                continue;
            }
            let seg = Seg::new(c.clone(), t0, t1);
            if seg.length() <= ptol.linear {
                continue;
            }
            let (ca, cb) = (classify(&ga, &seg, ptol), classify(&gb, &seg, ptol));
            let hit = if near {
                ca == Containment::Inside && cb == Containment::Inside
            } else {
                ca != Containment::Outside && cb != Containment::Outside
            };
            if hit {
                return Crossing::Hit;
            }
        }
    }
    Crossing::Clear
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::face_geom;
    use crate::geometry::Curve;
    use crate::topology::Solid;
    use crate::{AnalyticKernel, Kernel};
    use planora_math::DMat4;

    /// Rasterweite der Stichproben je Richtung.
    const GRID: usize = 60;
    /// Abstand vom Rand, unter dem Stichproben nicht zählen.
    const RIM: f64 = 0.01;

    fn piece(s: &Solid, k: usize) -> Piece {
        let (id, _) = s.faces.iter().nth(k).unwrap();
        let g = face_geom(s, id);
        let mut loops = g.loops.into_iter();
        Piece {
            surface: g.surface,
            same_sense: g.same_sense,
            domain: g.domain,
            outer: loops.next().unwrap_or_default(),
            holes: loops.collect(),
            tag: None,
        }
    }

    fn moved(s: Solid, by: DVec3) -> Solid {
        AnalyticKernel::default()
            .transform(&s, &DMat4::from_translation(by))
            .unwrap()
    }

    /// Fläche des Würfels, deren Ebene `z = 0` ist (Boden).
    fn floor(cube: &Solid) -> Piece {
        (0..cube.faces.len())
            .map(|k| piece(cube, k))
            .find(|p| segs(p).all(|s| s.start().z.abs() < 1e-12 && s.end().z.abs() < 1e-12))
            .unwrap()
    }

    #[test]
    fn face_pairs_are_judged() {
        let k = AnalyticKernel::default();
        let tol = Tolerance::default();
        let cube = k.make_box(DVec3::splat(10.0)).unwrap();
        let bottom = floor(&cube);
        let ball = |c: DVec3| piece(&moved(k.make_sphere(1.0).unwrap(), c), 0);
        // Kugel schneidet den Boden in einem Kreis ganz im Inneren beider Flächen.
        assert_eq!(
            crossing(&bottom, &ball(DVec3::new(5.0, 5.0, 0.5)), &tol, &tol),
            Crossing::Hit
        );
        // Kugel über dem Boden: Hüllquader auseinander bzw. kein Schnitt.
        assert_eq!(
            crossing(&bottom, &ball(DVec3::new(5.0, 5.0, 3.0)), &tol, &tol),
            Crossing::Clear
        );
        assert_eq!(
            crossing(&bottom, &ball(DVec3::new(5.0, 5.0, 1.0 + 1e-3)), &tol, &tol),
            Crossing::Clear
        );
        // Schnittkreis außerhalb des Bodens (Kugel neben dem Würfel).
        assert_eq!(
            crossing(&bottom, &ball(DVec3::new(-1.5, 5.0, 0.5)), &tol, &tol),
            Crossing::Clear
        );
        // Kugel neben einem Zylindermantel ohne gemeinsame Achse: kein Schnittrechner.
        let cyl = k.make_cylinder(2.0, 4.0).unwrap();
        let wall = (0..cyl.faces.len())
            .map(|i| piece(&cyl, i))
            .find(|p| matches!(p.surface, Surface::Cylinder { .. }))
            .unwrap();
        assert_eq!(
            crossing(&wall, &ball(DVec3::new(1.5, 1.0, 2.0)), &tol, &tol),
            Crossing::Unknown
        );
        // Kugel schräg neben dem Mantel: Hüllquader überlappen, der kleinste Abstand des Mantels
        // vom Mittelpunkt (≈ 1,54) ist größer als der Radius.
        assert_eq!(
            crossing(&wall, &ball(DVec3::new(2.5, 2.5, 2.0)), &tol, &tol),
            Crossing::Clear
        );
        assert!(ball_apart(
            &ball(DVec3::new(2.5, 2.5, 2.0)),
            &wall,
            1e-5,
            &tol
        ));
        // Senkrechter Zylinder r = 0,2 im Abstand 0,8 von der Achse innerhalb des Mantels:
        // getrennt über den Abstand zur gemeinsamen Achsrichtung.
        let tube = moved(
            k.make_cylinder(0.2, 6.0).unwrap(),
            DVec3::new(0.8, 0.0, -1.0),
        );
        let hole = (0..tube.faces.len())
            .map(|i| piece(&tube, i))
            .find(|p| matches!(p.surface, Surface::Cylinder { .. }))
            .unwrap();
        assert!(revolved_apart(&wall, &hole, 1e-5, &tol));
        assert!(!revolved_apart(
            &wall,
            &ball(DVec3::new(1.5, 1.0, 2.0)),
            1e-5,
            &tol
        ));
        // Zellenteilung: getrennt neben dem Mantel, nicht getrennt bei echter Durchdringung.
        assert!(patches_apart(&wall, &ball(DVec3::new(2.5, 2.5, 2.0)), 1e-5));
        assert!(!patches_apart(
            &wall,
            &ball(DVec3::new(1.5, 1.0, 2.0)),
            1e-5
        ));
    }

    #[test]
    fn arc_boxes_are_tight_and_contain_their_arcs() {
        let frame = planora_math::Frame {
            origin: DVec3::new(1.0, -2.0, 3.0),
            x: DVec3::new(0.6, 0.8, 0.0),
            y: DVec3::new(0.0, 0.0, 1.0),
            z: DVec3::new(0.8, -0.6, 0.0),
        };
        for (t0, t1) in [
            (0.3, 2.0),
            (2.0, 0.3),
            (-7.0, -6.5),
            (-1.0, 9.0),
            (4.0, 4.5),
        ] {
            for curve in [
                Curve::Circle { frame, radius: 2.5 },
                Curve::Ellipse {
                    frame,
                    major: 3.0,
                    minor: 1.0,
                },
            ] {
                let seg = Seg::new(curve, t0, t1);
                let (lo, hi) = tight_seg_box(&seg).unwrap();
                let (mut slo, mut shi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
                for k in 0..=4000 {
                    let p = seg.at(k as f64 / 4000.0);
                    assert!(p.cmpge(lo - 1e-12).all() && p.cmple(hi + 1e-12).all());
                    slo = slo.min(p);
                    shi = shi.max(p);
                }
                // eng: höchstens die Sehnenabweichung der Stichproben größer
                assert!((slo - lo).abs().max_element() < 1e-5, "{lo} {slo}");
                assert!((shi - hi).abs().max_element() < 1e-5, "{hi} {shi}");
            }
        }
    }

    #[test]
    fn cell_boxes_contain_their_patches() {
        let frame = planora_math::Frame {
            origin: DVec3::new(1.0, -2.0, 3.0),
            x: DVec3::new(0.6, 0.8, 0.0),
            y: DVec3::new(0.0, 0.0, 1.0),
            z: DVec3::new(0.8, -0.6, 0.0),
        };
        let surfaces = [
            Surface::Plane { frame },
            Surface::Cylinder { frame, radius: 2.0 },
            Surface::Cone {
                frame,
                radius: 1.5,
                slope: -0.4,
            },
            Surface::Sphere { frame, radius: 1.7 },
            Surface::Torus {
                frame,
                major: 3.0,
                minor: 0.8,
            },
        ];
        let cells = [
            (0.0, TAU, -1.0, 2.0),
            (0.3, 1.1, 0.2, 0.9),
            (-2.0, -1.9, -1.5, 1.5),
            (1.0, 4.0, -0.1, 0.0),
        ];
        for s in &surfaces {
            for &(u0, u1, v0, v1) in &cells {
                let c = UvRect { u0, u1, v0, v1 };
                let (lo, hi) = cell_box(s, &c).unwrap();
                for a in 0..=30 {
                    for b in 0..=30 {
                        let p = s.point(
                            u0 + (u1 - u0) * a as f64 / 30.0,
                            v0 + (v1 - v0) * b as f64 / 30.0,
                        );
                        assert!(
                            p.cmpge(lo - 1e-12).all() && p.cmple(hi + 1e-12).all(),
                            "{s:?} {c:?}: {p}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn face_boxes_contain_their_faces() {
        let k = AnalyticKernel::default();
        let tol = Tolerance::default();
        let cyl = moved(
            k.make_cylinder(2.0, 4.0).unwrap(),
            DVec3::new(1.0, -2.0, 0.5),
        );
        let ball = moved(k.make_sphere(1.5).unwrap(), DVec3::new(-3.0, 0.0, 1.0));
        // Kugeltasche an einer Würfelecke (Kugelstück ohne Pole im Inneren).
        let pocket = k
            .boolean(
                &k.make_box(DVec3::splat(4.0)).unwrap(),
                &moved(k.make_sphere(1.5).unwrap(), DVec3::new(0.5, 0.7, 3.6)),
                crate::BoolOp::Difference,
            )
            .unwrap();
        // Gerundeter, schräg gedrehter Zylinderrand (Torusstück).
        let raw = k.make_cylinder(3.0, 4.0).unwrap();
        let rim = raw
            .edges
            .iter()
            .find(|(_, e)| matches!(e.curve, Curve::Circle { .. }))
            .map(|(id, _)| id)
            .unwrap();
        let round = k
            .blend(
                &raw,
                &crate::BlendRequest {
                    edges: vec![crate::BlendEdge {
                        edge: rim,
                        first_face: None,
                    }],
                    kind: crate::BlendKind::Fillet { radius: 1.0 },
                    tangent_chain: true,
                },
            )
            .unwrap()
            .solid;
        let turned = k
            .transform(
                &round,
                &(DMat4::from_rotation_x(0.7) * DMat4::from_rotation_y(-0.4)),
            )
            .unwrap();
        for s in [&cyl, &ball, &pocket, &turned] {
            for i in 0..s.faces.len() {
                let p = piece(s, i);
                let (lo, hi) = face_box(&p, &tol).unwrap();
                let g = geom(&p);
                // Ebenen: großzügiges Raster; gekrümmte Flächen: ganzer Parameterbereich.
                let (u0, u1, v0, v1) = match p.surface {
                    Surface::Plane { .. } => (-20.0, 20.0, -20.0, 20.0),
                    Surface::Sphere { .. } => (-PI, PI, -PI / 2.0, PI / 2.0),
                    Surface::Torus { .. } => (-PI, PI, -PI, PI),
                    _ => (-PI, PI, -20.0, 20.0),
                };
                let mut hits = 0;
                for a in 0..=GRID {
                    for c in 0..=GRID {
                        let u = u0 + (u1 - u0) * a as f64 / GRID as f64;
                        let v = v0 + (v1 - v0) * c as f64 / GRID as f64;
                        let q = p.surface.point(u, v);
                        // Die Lageprüfung nähert den Rand durch Stützpunkte an; Punkte dicht am
                        // Rand sind kein Prüfstein für den Quader.
                        let near_rim = segs(&p).any(|s| s.closest(q).1 < RIM);
                        if !near_rim && point_in_face(&g, q, &tol) != Containment::Outside {
                            hits += 1;
                            assert!(
                                q.cmpge(lo - 1e-9).all() && q.cmple(hi + 1e-9).all(),
                                "{q} nicht in {lo} {hi}"
                            );
                        }
                    }
                }
                assert!(hits > 0, "Fläche {i} ohne Stichprobe");
            }
        }
        assert!(
            turned
                .faces
                .iter()
                .any(|(_, f)| matches!(f.surface, Surface::Torus { .. }))
        );
    }
}
