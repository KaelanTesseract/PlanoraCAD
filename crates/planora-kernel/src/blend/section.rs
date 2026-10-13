//! Querschnitt der Verrundung an einem Kantenpunkt (Spec §3.1, R3, R5): Der Kugelmittelpunkt
//! wird per Newton aus den vorzeichenbehafteten Abständen beider Flächen bestimmt (keine
//! Versatzfläche als Objekt); die Berührpunkte sind die Fußpunkte. Fasen: Punkt im
//! Sehnenabstand auf jeder Fläche oder Abstand und Winkel.

use super::error::BlendError;
use crate::geometry::Surface;
use planora_math::DVec3;

/// Höchstzahl Newton-Schritte je Querschnitt (deterministische Arbeitsgrenze).
pub const SECTION_NEWTON_STEPS: usize = 60;
/// Restfehler der Abstandsgleichungen relativ zum Maßstab `1 + |o| + r`, unter dem Newton
/// endet. Weit unter `tol.linear`; analytische Flächen erreichen ihn nach wenigen Schritten.
pub const SECTION_RESIDUAL_REL: f64 = 1e-12;
/// Kleinster Betrag der Jacobi-Determinante; darunter sind die Flächen im Querschnitt parallel.
const MIN_JACOBIAN: f64 = 1e-12;
/// Größter Newton-Schritt in Vielfachen des Maßes (Dämpfung gegen Sprünge auf fremde Lagen).
const MAX_STEP_FACTOR: f64 = 0.5;
/// Untergrenze für `1 + nA · nB` beim Startpunkt; darunter stehen die Normalen fast
/// entgegengesetzt (Schneide), der Start wäre unendlich weit weg.
const MIN_BISECTOR: f64 = 1e-6;
/// Kleinster Betrag von `∇B · Richtung` beim Strahl der Winkelfase; darunter läuft der Strahl
/// fast parallel zur Fläche B.
const MIN_RAY_SLOPE: f64 = 1e-9;
/// Spielraum der Krümmungsprüfung (relativ): Ein Radius gleich dem Krümmungsradius ist schon
/// zu groß (die Kugel fiele zu einem Punkt).
const CURVATURE_MARGIN: f64 = 1e-9;

/// Eine Seite der Kante: Trägerfläche mit Außenrichtung.
#[derive(Clone, Copy, Debug)]
pub struct Side<'a> {
    pub surface: &'a Surface,
    pub same_sense: bool,
}

impl Side<'_> {
    fn sign(&self) -> f64 {
        if self.same_sense { 1.0 } else { -1.0 }
    }

    /// Vorzeichenbehafteter Abstand, positiv außerhalb des Materials.
    pub fn outside(&self, p: DVec3) -> f64 {
        self.surface.implicit(p) * self.sign()
    }

    /// Außennormale am Fußpunkt von `p` (Gradient von [`Side::outside`]).
    pub fn normal_at(&self, p: DVec3) -> DVec3 {
        let (u, v) = self.surface.project(p);
        self.surface.normal(u, v) * self.sign()
    }
}

/// Querschnitt: Kugelmittelpunkt (bei Fasen der Kantenpunkt) und Berührpunkte auf A und B.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub center: DVec3,
    pub pa: DVec3,
    pub pb: DVec3,
}

/// Fasenmaße (Bogenmaß beim Winkel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChamferSpec {
    /// Sehnenabstand auf Fläche A und auf Fläche B.
    Distances { a: f64, b: f64 },
    /// Sehnenabstand auf Fläche A und Winkel der Fase zu Fläche A.
    DistanceAngle { distance: f64, angle: f64 },
}

/// Löst das 2×2-System `M · (x, y) = (f, g)`; `None` bei fast singulärer Matrix.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn solve2(m: [[f64; 2]; 2], f: f64, g: f64) -> Option<(f64, f64)> {
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    if !(det.abs() > MIN_JACOBIAN) {
        return None;
    }
    Some((
        (f * m[1][1] - g * m[0][1]) / det,
        (m[0][0] * g - m[1][0] * f) / det,
    ))
}

/// Begrenzt einen Schritt auf `max` (Länge).
fn damp(step: DVec3, max: f64) -> DVec3 {
    let len = step.length();
    if len > max { step * (max / len) } else { step }
}

/// Rundung: Kugelmittelpunkt in der Ebene durch `o` senkrecht zu `tangent` mit Abstand `radius`
/// zu beiden Flächen, im Material bei konvexen und außerhalb bei konkaven Kanten. `start` ist
/// ein Startwert (etwa der Mittelpunkt des vorigen Querschnitts); ohne ihn gilt die
/// Ebenenformel `c = o − s·r·(nA + nB)/(1 + nA·nB)` mit den Normalen an `o`.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn fillet_section(
    a: Side,
    b: Side,
    o: DVec3,
    tangent: DVec3,
    convex: bool,
    radius: f64,
    start: Option<DVec3>,
) -> Result<Section, BlendError> {
    let t = tangent.normalize_or_zero();
    if t == DVec3::ZERO || !(radius > 0.0) {
        return Err(BlendError::SpineLost);
    }
    let (b1, b2) = t.any_orthonormal_pair();
    let target = if convex { -radius } else { radius };
    let mut c = match start {
        Some(c) => c - t * (c - o).dot(t),
        None => {
            let (na, nb) = (a.normal_at(o), b.normal_at(o));
            let denom = 1.0 + na.dot(nb);
            if !(denom > MIN_BISECTOR) {
                return Err(BlendError::SpineLost);
            }
            o + (na + nb) * (target / denom)
        }
    };
    let eps = SECTION_RESIDUAL_REL * (1.0 + o.abs().max_element() + radius);
    for _ in 0..SECTION_NEWTON_STEPS {
        let (da, db) = (a.outside(c), b.outside(c));
        let (fa, fb) = (da - target, db - target);
        if !(fa.is_finite() && fb.is_finite()) {
            return Err(BlendError::SpineLost);
        }
        let (ga, gb) = (a.normal_at(c), b.normal_at(c));
        if fa.abs() <= eps && fb.abs() <= eps {
            return Ok(Section {
                center: c,
                pa: c - ga * da,
                pb: c - gb * db,
            });
        }
        let m = [[ga.dot(b1), ga.dot(b2)], [gb.dot(b1), gb.dot(b2)]];
        let (x, y) = solve2(m, fa, fb).ok_or(BlendError::SpineLost)?;
        c -= damp(b1 * x + b2 * y, MAX_STEP_FACTOR * radius);
    }
    Err(BlendError::SpineLost)
}

/// Punkt auf Fläche `side` in der Ebene durch `o` senkrecht zu `t` im Sehnenabstand `d` von `o`,
/// Startrichtung `w` (in die Fläche hinein).
fn chord_point(side: Side, o: DVec3, t: DVec3, w: DVec3, d: f64) -> Result<DVec3, BlendError> {
    let (b1, b2) = t.any_orthonormal_pair();
    let eps = SECTION_RESIDUAL_REL * (1.0 + o.abs().max_element() + d);
    let mut p = o + w * d;
    for _ in 0..SECTION_NEWTON_STEPS {
        let rel = p - o;
        let (f, g) = (side.outside(p), rel.length() - d);
        if !(f.is_finite() && g.is_finite()) {
            return Err(BlendError::SpineLost);
        }
        if f.abs() <= eps && g.abs() <= eps {
            return Ok(p);
        }
        let n = side.normal_at(p);
        let dir = rel.normalize_or_zero();
        let m = [[n.dot(b1), n.dot(b2)], [dir.dot(b1), dir.dot(b2)]];
        let (x, y) = solve2(m, f, g).ok_or(BlendError::SpineLost)?;
        p -= damp(b1 * x + b2 * y, MAX_STEP_FACTOR * d);
    }
    Err(BlendError::SpineLost)
}

/// Fase: Berührpunkte im Sehnenabstand (Fläche A, Fläche B) oder Punkt auf A im Abstand und
/// Schnitt des Strahls unter dem Winkel zu A mit B. `wa`, `wb` zeigen von der Kante in die
/// Flächen hinein (siehe `topo::inward_directions`).
///
/// Winkel (P4): gemessen im Querschnitt (Ebene senkrecht zur Kante durch den Kantenpunkt)
/// zwischen der Fasenlinie und der Tangente von Fläche A im Berührpunkt `pa`, die zur Kante hin
/// zeigt; die Fase läuft von dort ins Material. Bei ebener Fläche A (und bei Flächen, die im
/// Querschnitt gerade sind, etwa ein Zylinder längs seiner Achse) ist das der Winkel zur Sehne.
#[allow(clippy::too_many_arguments, clippy::neg_cmp_op_on_partial_ord)]
pub fn chamfer_section(
    a: Side,
    b: Side,
    o: DVec3,
    tangent: DVec3,
    convex: bool,
    wa: DVec3,
    wb: DVec3,
    spec: ChamferSpec,
) -> Result<Section, BlendError> {
    let t = tangent.normalize_or_zero();
    if t == DVec3::ZERO {
        return Err(BlendError::SpineLost);
    }
    match spec {
        ChamferSpec::Distances { a: da, b: db } => Ok(Section {
            center: o,
            pa: chord_point(a, o, t, wa, da)?,
            pb: chord_point(b, o, t, wb, db)?,
        }),
        ChamferSpec::DistanceAngle { distance, angle } => {
            let pa = chord_point(a, o, t, wa, distance)?;
            let back = o - pa;
            let na = a.normal_at(pa);
            // Tangente von Fläche A im Querschnitt bei `pa` (senkrecht zu Kante und Normale),
            // zur Kante hin gerichtet; bei Ebenen gleich der Sehnenrichtung zurück zur Kante.
            let mut u = t.cross(na).normalize_or_zero();
            if u.dot(back) < 0.0 {
                u = -u;
            }
            if u == DVec3::ZERO || !na.is_finite() {
                return Err(BlendError::SpineLost);
            }
            let s = if convex { 1.0 } else { -1.0 };
            // Drehung in der Querschnittsebene: `u` und `na` stehen senkrecht aufeinander, der
            // Winkel zwischen Fase und Tangente von A ist genau `angle` (P4).
            let dir = u * angle.cos() - na * (s * angle.sin());
            let misfit = || {
                BlendError::InvalidParameter(
                    "Der Winkel der Fase passt nicht zu dieser Kante (die Fase trifft die zweite Fläche nicht)".into(),
                )
            };
            let eps = SECTION_RESIDUAL_REL * (1.0 + o.abs().max_element() + distance);
            let mut lam = back.length();
            for _ in 0..SECTION_NEWTON_STEPS {
                let q = pa + dir * lam;
                let f = b.outside(q);
                if !f.is_finite() {
                    return Err(misfit());
                }
                if f.abs() <= eps {
                    return if lam > 0.0 {
                        Ok(Section {
                            center: o,
                            pa,
                            pb: q,
                        })
                    } else {
                        Err(misfit())
                    };
                }
                let slope = b.normal_at(q).dot(dir);
                if !(slope.abs() > MIN_RAY_SLOPE) {
                    return Err(misfit());
                }
                lam -= f / slope;
            }
            Err(misfit())
        }
    }
}

/// Liegt die Kugel auf der gewölbten Innenseite einer gekrümmten Fläche und ist ihr Radius
/// mindestens so groß wie der Krümmungsradius dort, findet sie keine Lage:
/// `CurvatureTooSmall` (Spec §3.4). Ebenen und Freiformflächen werden hier nicht geprüft (bei
/// Freiformflächen prüft die Bahn die Genauigkeit, Task 14).
pub fn check_curvature(
    side: Side,
    p: DVec3,
    convex: bool,
    radius: f64,
    face: &str,
) -> Result<(), BlendError> {
    // Vorzeichen von `implicit` am Kugelmittelpunkt: < 0 heißt Achsseite (innen gewölbt).
    let ball = if convex { -1.0 } else { 1.0 } * if side.same_sense { 1.0 } else { -1.0 };
    if ball > 0.0 {
        return Ok(());
    }
    let rc = match side.surface {
        Surface::Cylinder { radius, .. } | Surface::Sphere { radius, .. } => Some(*radius),
        Surface::Torus { minor, .. } => Some(*minor),
        Surface::Cone { frame, slope, .. } => {
            let l = frame.to_local(p);
            Some((l.x * l.x + l.y * l.y).sqrt() * (1.0 + slope * slope).sqrt())
        }
        Surface::Plane { .. } | Surface::Nurbs(_) => None,
    };
    match rc {
        Some(rc) if radius >= rc * (1.0 - CURVATURE_MARGIN) => {
            Err(BlendError::CurvatureTooSmall { face: face.into() })
        }
        _ => Ok(()),
    }
}
