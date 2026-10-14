//! Geometrie (Spec §3): Punkte, Richtungen, Koordinatensysteme, Kurven und Flächen.

use crate::entity::{Entities, logical, refs};
use crate::error::StepError;
use crate::number::push_real;
use planora_kernel::{Curve, NurbsSurfaceData, Surface, UvRect};
use planora_math::nurbs::{KnotVector, NurbsCurve};
use planora_math::{DVec3, Frame, Tolerance};
use std::fmt::Write as _;

/// Abweichung, bis zu der ein Koordinatensystem des Kernels als senkrecht und normiert gilt.
/// Die Systeme des Kernels weichen um Rundungsreste (etwa 1e-15) ab; schief gebaute Systeme
/// werden benannt abgelehnt statt falsch geschrieben.
pub const FRAME_TOL: f64 = 1e-9;

/// Ablehnung eines Körpers ohne Namen; [`crate::check_body`] setzt den Namen ein.
pub(crate) fn reject(reason: &str) -> StepError {
    StepError::InvalidBody {
        name: String::new(),
        reason: reason.to_string(),
    }
}

/// Hängt `(x,y,z)` an.
fn push_triple(s: &mut String, v: DVec3) -> Result<(), StepError> {
    s.push('(');
    push_real(s, v.x)?;
    s.push(',');
    push_real(s, v.y)?;
    s.push(',');
    push_real(s, v.z)?;
    s.push(')');
    Ok(())
}

pub(crate) fn point(e: &mut Entities, p: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("CARTESIAN_POINT('',");
    push_triple(&mut s, p)?;
    s.push(')');
    e.push(&s)
}

pub(crate) fn direction(e: &mut Entities, d: DVec3) -> Result<u64, StepError> {
    let mut s = String::from("DIRECTION('',");
    push_triple(&mut s, d)?;
    s.push(')');
    e.push(&s)
}

/// `AXIS2_PLACEMENT_3D` mit Ursprung, z-Achse (`axis`) und x-Achse (`ref_direction`).
pub(crate) fn placement(
    e: &mut Entities,
    origin: DVec3,
    z: DVec3,
    x: DVec3,
) -> Result<u64, StepError> {
    let p = point(e, origin)?;
    let a = direction(e, z)?;
    let r = direction(e, x)?;
    e.push(&format!("AXIS2_PLACEMENT_3D('',#{p},#{a},#{r})"))
}

/// Koordinatensystem des Kernels; schiefe oder nicht normierte Systeme werden abgelehnt.
pub(crate) fn frame_placement(e: &mut Entities, f: &Frame) -> Result<u64, StepError> {
    if !f.is_orthonormal(FRAME_TOL) {
        return Err(reject(
            "Ein Koordinatensystem ist schief oder nicht normiert",
        ));
    }
    placement(e, f.origin, f.z, f.x)
}

/// Eintrag `NAME('',#placement,zahl,…)`.
fn with_numbers(
    e: &mut Entities,
    name: &str,
    placement: u64,
    numbers: &[f64],
) -> Result<u64, StepError> {
    let mut s = format!("{name}('',#{placement}");
    for x in numbers {
        s.push(',');
        push_real(&mut s, *x)?;
    }
    s.push(')');
    e.push(&s)
}

/// Prüft eine Länge, die in STEP positiv sein muss (Radius, Halbachse).
fn positive(x: f64, what: &str) -> Result<f64, StepError> {
    if !x.is_finite() {
        return Err(StepError::NotFinite);
    }
    if x > 0.0 {
        Ok(x)
    } else {
        Err(reject(&format!("{what} ist nicht größer als null")))
    }
}

/// Trägerkurve einer Kante.
pub(crate) fn curve(e: &mut Entities, c: &Curve) -> Result<u64, StepError> {
    match c {
        Curve::Line { origin, dir } => {
            let p = point(e, *origin)?;
            let d = direction(e, *dir)?;
            let v = e.push(&format!("VECTOR('',#{d},1.)"))?;
            e.push(&format!("LINE('',#{p},#{v})"))
        }
        Curve::Circle { frame, radius } => {
            let r = positive(*radius, "Der Radius eines Kreises")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "CIRCLE", a, &[r])
        }
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => {
            let (a, b) = (
                positive(*major, "Eine Halbachse einer Ellipse")?,
                positive(*minor, "Eine Halbachse einer Ellipse")?,
            );
            if b > a {
                // STEP legt die erste Halbachse in die Bezugsrichtung; die längere kommt dorthin
                // (x' = y, y' = −x, derselbe Umlaufsinn um z). Der Parameter verschiebt sich um
                // π/2; die Kante wird über ihre Eckpunkte begrenzt.
                if !frame.is_orthonormal(FRAME_TOL) {
                    return Err(reject(
                        "Ein Koordinatensystem ist schief oder nicht normiert",
                    ));
                }
                let p = placement(e, frame.origin, frame.z, frame.y)?;
                with_numbers(e, "ELLIPSE", p, &[b, a])
            } else {
                let p = frame_placement(e, frame)?;
                with_numbers(e, "ELLIPSE", p, &[a, b])
            }
        }
        Curve::Nurbs(c) => nurbs_curve(e, c),
    }
}

/// Knoten als verschiedene Werte mit Vielfachheiten (Spec §3): `((m1,m2,…), (k1,k2,…))`. Der
/// Kernel speichert gleiche Knoten bitgleich, daher genügt der genaue Vergleich.
fn knot_lists(knots: &KnotVector) -> Result<(String, String), StepError> {
    let mut values: Vec<f64> = Vec::new();
    let mut mults: Vec<usize> = Vec::new();
    for &k in knots.knots() {
        match values.last() {
            Some(&last) if last == k => {
                if let Some(m) = mults.last_mut() {
                    *m += 1;
                }
            }
            _ => {
                values.push(k);
                mults.push(1);
            }
        }
    }
    let mut m = String::from("(");
    for (i, n) in mults.iter().enumerate() {
        if i > 0 {
            m.push(',');
        }
        let _ = write!(m, "{n}");
    }
    m.push(')');
    let mut v = String::new();
    push_list(&mut v, &values)?;
    Ok((m, v))
}

/// Hängt `(x1,x2,…)` an.
fn push_list(s: &mut String, xs: &[f64]) -> Result<(), StepError> {
    s.push('(');
    for (i, x) in xs.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        push_real(s, *x)?;
    }
    s.push(')');
    Ok(())
}

/// Freiformkurve (Spec §3): `B_SPLINE_CURVE_WITH_KNOTS`; mit Gewichten ungleich 1 als
/// zusammengesetzter Eintrag mit `RATIONAL_B_SPLINE_CURVE` (Teile in alphabetischer Reihenfolge,
/// wie ISO 10303-21 sie verlangt). Geschlossene Kurven tragen das Kennzeichen `.T.`.
fn nurbs_curve(e: &mut Entities, c: &NurbsCurve) -> Result<u64, StepError> {
    let mut points = Vec::with_capacity(c.control_points().len());
    for p in c.control_points() {
        points.push(point(e, *p)?);
    }
    let degree = c.degree();
    let closed = logical(c.is_closed(Tolerance::default().linear));
    let (mults, values) = knot_lists(c.knots())?;
    let knots = format!("{mults},{values}");
    let cps = refs(&points);
    if c.weights().iter().all(|w| *w == 1.0) {
        return e.push(&format!(
            "B_SPLINE_CURVE_WITH_KNOTS('',{degree},{cps},.UNSPECIFIED.,{closed},.F.,{knots},.UNSPECIFIED.)"
        ));
    }
    let mut weights = String::new();
    push_list(&mut weights, c.weights())?;
    e.push(&format!(
        "(BOUNDED_CURVE()B_SPLINE_CURVE({degree},{cps},.UNSPECIFIED.,{closed},.F.)\
         B_SPLINE_CURVE_WITH_KNOTS({knots},.UNSPECIFIED.)CURVE()GEOMETRIC_REPRESENTATION_ITEM()\
         RATIONAL_B_SPLINE_CURVE({weights})REPRESENTATION_ITEM(''))"
    ))
}

/// Freiformfläche (Spec §3): Kontrollpunkte zeilenweise in u (`control[i][j]`), Kennzeichen
/// „geschlossen“ in u und v wie im Kernel; rational wie bei [`nurbs_curve`].
fn nurbs_surface(e: &mut Entities, d: &NurbsSurfaceData) -> Result<u64, StepError> {
    let s = d.surface();
    let (nu, nv) = s.counts();
    let mut rows = String::from("(");
    let mut weights = String::from("(");
    let mut rational = false;
    for i in 0..nu {
        let mut row = Vec::with_capacity(nv);
        let mut w = Vec::with_capacity(nv);
        for j in 0..nv {
            row.push(point(e, s.control_at(i, j))?);
            w.push(s.weight_at(i, j));
            rational |= s.weight_at(i, j) != 1.0;
        }
        if i > 0 {
            rows.push(',');
            weights.push(',');
        }
        rows.push_str(&refs(&row));
        push_list(&mut weights, &w)?;
    }
    rows.push(')');
    weights.push(')');
    let (du, dv) = (s.degree_u(), s.degree_v());
    let (uc, vc) = (logical(d.is_periodic_u()), logical(d.is_periodic_v()));
    let (mu, ku) = knot_lists(s.knots_u())?;
    let (mv, kv) = knot_lists(s.knots_v())?;
    let knots = format!("{mu},{mv},{ku},{kv}");
    if !rational {
        return e.push(&format!(
            "B_SPLINE_SURFACE_WITH_KNOTS('',{du},{dv},{rows},.UNSPECIFIED.,{uc},{vc},.F.,{knots},.UNSPECIFIED.)"
        ));
    }
    e.push(&format!(
        "(BOUNDED_SURFACE()B_SPLINE_SURFACE({du},{dv},{rows},.UNSPECIFIED.,{uc},{vc},.F.)\
         B_SPLINE_SURFACE_WITH_KNOTS({knots},.UNSPECIFIED.)GEOMETRIC_REPRESENTATION_ITEM()\
         RATIONAL_B_SPLINE_SURFACE({weights})REPRESENTATION_ITEM('')SURFACE())"
    ))
}

/// Kegel (Spec §3): Der Kernel beschreibt den Radius an der Höhe `v` als `radius + slope · v`;
/// STEP verlangt am Ursprung einen Radius ≥ 0 und einen halben Öffnungswinkel in (0, π/2). Die
/// Lage wird deshalb auf die Mitte des v-Bereichs der Fläche verschoben (dort ist der Radius der
/// Fläche positiv) und bei fallendem Radius (`slope < 0`) die Achse umgekehrt; die natürliche
/// Normale (von der Achse weg) bleibt dieselbe. `slope = 0` ist ein Zylinder.
fn cone(
    e: &mut Entities,
    frame: &Frame,
    radius: f64,
    slope: f64,
    patch: Option<UvRect>,
) -> Result<u64, StepError> {
    if !(radius.is_finite() && slope.is_finite()) {
        return Err(StepError::NotFinite);
    }
    if !frame.is_orthonormal(FRAME_TOL) {
        return Err(reject(
            "Ein Koordinatensystem ist schief oder nicht normiert",
        ));
    }
    let Some(p) = patch else {
        return Err(reject("Eine Kegelfläche hat keinen Parameterbereich"));
    };
    let v_mid = 0.5 * (p.v0 + p.v1);
    if !v_mid.is_finite() {
        return Err(StepError::NotFinite);
    }
    let rho = radius + slope * v_mid;
    if rho.is_nan() || rho <= 0.0 {
        return Err(reject("Eine Kegelfläche liegt jenseits ihrer Spitze"));
    }
    let origin = frame.origin + frame.z * v_mid;
    if slope == 0.0 {
        let a = placement(e, origin, frame.z, frame.x)?;
        return with_numbers(e, "CYLINDRICAL_SURFACE", a, &[rho]);
    }
    let z = if slope > 0.0 { frame.z } else { -frame.z };
    let a = placement(e, origin, z, frame.x)?;
    with_numbers(e, "CONICAL_SURFACE", a, &[rho, slope.abs().atan()])
}

/// Trägerfläche einer Fläche; `patch` ist der Parameterbereich der Fläche (für den Kegel).
pub(crate) fn surface(
    e: &mut Entities,
    s: &Surface,
    patch: Option<UvRect>,
) -> Result<u64, StepError> {
    match s {
        Surface::Plane { frame } => {
            let a = frame_placement(e, frame)?;
            e.push(&format!("PLANE('',#{a})"))
        }
        Surface::Cylinder { frame, radius } => {
            let r = positive(*radius, "Der Radius eines Zylinders")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "CYLINDRICAL_SURFACE", a, &[r])
        }
        Surface::Sphere { frame, radius } => {
            let r = positive(*radius, "Der Radius einer Kugel")?;
            let a = frame_placement(e, frame)?;
            with_numbers(e, "SPHERICAL_SURFACE", a, &[r])
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            let big = positive(*major, "Der große Radius eines Torus")?;
            let small = positive(*minor, "Der kleine Radius eines Torus")?;
            if small >= big {
                // Ein Spindel- oder Hornring (kleiner Radius ≥ großer) ist in STEP keine
                // gewöhnliche Torusfläche; eine Sonderfläche ist hier nicht überprüft.
                return Err(reject(
                    "Eine Torusfläche mit kleinem Radius größer oder gleich dem großen lässt sich noch nicht als STEP ausgeben.",
                ));
            }
            let a = frame_placement(e, frame)?;
            with_numbers(e, "TOROIDAL_SURFACE", a, &[big, small])
        }
        Surface::Cone {
            frame,
            radius,
            slope,
        } => cone(e, frame, *radius, *slope, patch),
        Surface::Nurbs(d) => nurbs_surface(e, d),
    }
}
