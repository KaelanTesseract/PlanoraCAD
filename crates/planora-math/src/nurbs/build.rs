//! Exakte Konstruktionen: Kreis- und Ellipsenbogen (rationale quadratische Segmente),
//! natürlicher kubischer Spline, Verschiebefläche (Extrusion) und Drehfläche.

use super::dense::solve_dense;
use super::fit::MAX_FIT_CONTROL_POINTS;
use super::{KnotVector, MAX_COORD, NurbsCurve, NurbsError, NurbsSurface};
use crate::{DVec3, TAU};
use planora_i18n::tr;
use std::f64::consts::FRAC_PI_2;

/// Kleinster erlaubter Öffnungswinkel eines Bogens (Winkeltoleranz aus `Tolerance::default()`);
/// kleinere Bögen hätten praktisch zusammenfallende Kontrollpunkte.
const MIN_ARC_SWEEP: f64 = 1e-9;
/// Ist `|a × b|` kleiner als dieser Bruchteil von `|a|·|b|`, gelten die Achsen als parallel.
const PARALLEL_RATIO: f64 = 1e-12;
/// Kleinste Länge einer Richtung (Extrusion, Drehachse) in Millimetern.
const MIN_DIRECTION_LENGTH: f64 = 1e-12;
/// Ist ein Kontrollpunkt näher als das an der Drehachse, gilt er als auf ihr liegend
/// (alle Kreis-Kontrollpunkte fallen dann exakt mit ihm zusammen).
const ON_AXIS_DISTANCE: f64 = 1e-12;

/// Kreisbogen in den Koordinaten `(a, b)`: Kontrollpunkte `C + ca·a + cb·b`, Gewichte und
/// Anzahl der Segmente. `sweep` muss geprüft sein (`MIN_ARC_SWEEP ≤ |sweep| ≤ 2π`).
struct ArcCoeffs {
    coeffs: Vec<(f64, f64)>,
    weights: Vec<f64>,
    segments: usize,
}

fn check_sweep(start: f64, sweep: f64) -> Result<(), NurbsError> {
    if !start.is_finite() || !sweep.is_finite() {
        return Err(NurbsError::Input(tr!("math-sweep-finite")));
    }
    if sweep.abs() > TAU {
        return Err(NurbsError::Input(tr!("math-sweep-large")));
    }
    if sweep.abs() < MIN_ARC_SWEEP {
        return Err(NurbsError::Input(tr!(
            "math-sweep-small",
            min = MIN_ARC_SWEEP.to_string()
        )));
    }
    Ok(())
}

/// Piegl/Tiller A7.1, linear in `(a, b)`: Segmente zu höchstens 90°, Zwischenpunkt im Schnitt
/// der Tangenten (Abstand `1 / cos(θ/2)` vom Mittelpunkt), Gewicht `cos(θ/2)`.
fn arc_coeffs(start: f64, sweep: f64) -> ArcCoeffs {
    let segments = ((sweep.abs() / FRAC_PI_2).ceil() as usize).clamp(1, 4);
    let theta = sweep / segments as f64;
    let wm = (0.5 * theta).cos();
    let full = sweep.abs() == TAU;
    let mut coeffs = Vec::with_capacity(2 * segments + 1);
    let mut weights = Vec::with_capacity(2 * segments + 1);
    coeffs.push((start.cos(), start.sin()));
    weights.push(1.0);
    for k in 0..segments {
        let mid = start + theta * (k as f64 + 0.5);
        coeffs.push((mid.cos() / wm, mid.sin() / wm));
        weights.push(wm);
        let end = if k + 1 == segments {
            start + sweep
        } else {
            start + theta * (k + 1) as f64
        };
        coeffs.push((end.cos(), end.sin()));
        weights.push(1.0);
    }
    if full {
        // Vollkreis: Ende exakt gleich Anfang (geschlossen ohne Rundungsfehler).
        let last = coeffs.len() - 1;
        coeffs[last] = coeffs[0];
    }
    ArcCoeffs {
        coeffs,
        weights,
        segments,
    }
}

/// Knotenvektor Grad 2 auf `[lo, hi]` mit doppelten inneren Knoten an den Segmentgrenzen.
fn arc_knots(segments: usize, lo: f64, hi: f64) -> Vec<f64> {
    let mut k = vec![lo; 3];
    for s in 1..segments {
        let t = lo + (hi - lo) * s as f64 / segments as f64;
        k.push(t);
        k.push(t);
    }
    k.extend([hi; 3]);
    k
}

fn check_point(p: DVec3, what: impl FnOnce() -> String) -> Result<(), NurbsError> {
    if !p.is_finite() || p.abs().max_element() > MAX_COORD {
        return Err(NurbsError::Input(tr!(
            "math-point-range",
            what = what(),
            max = MAX_COORD.to_string()
        )));
    }
    Ok(())
}

impl NurbsCurve {
    /// Kreis-/Ellipsenbogen: `P(t) = center + cos t · a + sin t · b`, `t ∈ [start, start + sweep]`;
    /// Parameter `0..1`, gleichmäßig je Segment (höchstens 90° je Segment). Der Parameter
    /// entspricht dem Winkel nur an den Segmentgrenzen (Knoten), dazwischen nicht linear;
    /// für den Parameter zu einem Punkt die Rückrechnung (`project`) verwenden. `|sweep| ≤ 2π` (Vollkreis erlaubt),
    /// `a` und `b` nicht parallel, Öffnungswinkel mindestens `MIN_ARC_SWEEP`.
    pub fn elliptic_arc(
        center: DVec3,
        a: DVec3,
        b: DVec3,
        start: f64,
        sweep: f64,
    ) -> Result<NurbsCurve, NurbsError> {
        check_point(center, || tr!("math-what-center"))?;
        check_point(a, || tr!("math-what-axis-a"))?;
        check_point(b, || tr!("math-what-axis-b"))?;
        if a.cross(b).length() <= PARALLEL_RATIO * a.length() * b.length()
            || a.length() < MIN_DIRECTION_LENGTH
            || b.length() < MIN_DIRECTION_LENGTH
        {
            return Err(NurbsError::Input(tr!("math-axes-degenerate")));
        }
        check_sweep(start, sweep)?;
        let arc = arc_coeffs(start, sweep);
        let control = arc
            .coeffs
            .iter()
            .map(|(ca, cb)| center + a * *ca + b * *cb)
            .collect();
        NurbsCurve::new(2, arc_knots(arc.segments, 0.0, 1.0), control, arc.weights)
    }

    /// Natürlicher kubischer Spline durch die Punkte (`C(params[i]) = points[i]`, `C'' = 0` an
    /// beiden Enden); `params` streng steigend, gleiche Zahl wie Punkte, mindestens 2,
    /// höchstens `MAX_FIT_CONTROL_POINTS − 2`. Bei zwei Punkten eine Gerade (Grad 1).
    pub fn natural_cubic(points: &[DVec3], params: &[f64]) -> Result<NurbsCurve, NurbsError> {
        let n = points.len();
        if n != params.len() {
            return Err(NurbsError::Input(tr!("math-points-params-count")));
        }
        if n < 2 {
            return Err(NurbsError::Input(tr!("math-two-points")));
        }
        if n > MAX_FIT_CONTROL_POINTS - 2 {
            return Err(NurbsError::TooLarge(tr!(
                "math-max-points",
                max = (MAX_FIT_CONTROL_POINTS - 2).to_string()
            )));
        }
        for p in points {
            check_point(*p, || tr!("math-what-each-point"))?;
        }
        if params.iter().any(|t| !t.is_finite()) {
            return Err(NurbsError::Input(tr!("math-params-finite")));
        }
        if params.windows(2).any(|w| w[1] <= w[0]) {
            return Err(NurbsError::Input(tr!("math-params-increasing")));
        }
        let (t0, t1) = (params[0], params[n - 1]);
        if n == 2 {
            return NurbsCurve::new(1, vec![t0, t0, t1, t1], points.to_vec(), vec![1.0; 2]);
        }
        let mut knots = vec![t0; 4];
        knots.extend_from_slice(&params[1..n - 1]);
        knots.extend([t1; 4]);
        let kv = KnotVector::new(3, knots.clone())?;
        let m = n + 2;
        let mut mat = vec![0.0; m * m];
        let mut rhs = vec![DVec3::ZERO; m];
        let mut row = 0;
        let mut put = |t: f64, order: usize, value: DVec3, row: &mut usize| {
            let span = kv.find_span(t);
            let d = kv.basis_derivs(span, t, order);
            for (j, v) in d[order].iter().enumerate() {
                mat[*row * m + span - 3 + j] = *v;
            }
            rhs[*row] = value;
            *row += 1;
        };
        put(t0, 2, DVec3::ZERO, &mut row);
        for (p, t) in points.iter().zip(params) {
            put(*t, 0, *p, &mut row);
        }
        put(t1, 2, DVec3::ZERO, &mut row);
        let control = solve_components(&mat, &rhs, m)?;
        NurbsCurve::new(3, knots, control, vec![1.0; m])
    }
}

/// Löst `mat · X = rhs` je Koordinate (x, y, z) mit einer Kopie der Matrix.
pub(super) fn solve_components(
    mat: &[f64],
    rhs: &[DVec3],
    m: usize,
) -> Result<Vec<DVec3>, NurbsError> {
    let mut out = vec![DVec3::ZERO; m];
    for c in 0..3 {
        let mut a = mat.to_vec();
        let mut b: Vec<f64> = rhs.iter().map(|p| p[c]).collect();
        solve_dense(&mut a, &mut b, m)?;
        for (o, v) in out.iter_mut().zip(b) {
            o[c] = v;
        }
    }
    Ok(out)
}

impl NurbsSurface {
    /// Verschiebefläche: Kurve entlang `dir`; Grad (Kurvengrad, 1), u = Kurvenparameter,
    /// v ∈ [0, 1] (v = 0 ist die Kurve, v = 1 die verschobene Kurve).
    pub fn extrude(curve: &NurbsCurve, dir: DVec3) -> Result<NurbsSurface, NurbsError> {
        if !dir.is_finite() || dir.length() < MIN_DIRECTION_LENGTH {
            return Err(NurbsError::Input(tr!("math-direction")));
        }
        let nu = curve.control_points().len();
        let mut control = Vec::with_capacity(2 * nu);
        let mut weights = Vec::with_capacity(2 * nu);
        for (p, w) in curve.control_points().iter().zip(curve.weights()) {
            control.extend([*p, *p + dir]);
            weights.extend([*w, *w]);
        }
        NurbsSurface::new(
            curve.degree(),
            1,
            curve.knots().knots().to_vec(),
            vec![0.0, 0.0, 1.0, 1.0],
            nu,
            2,
            control,
            weights,
        )
    }

    /// Drehfläche: Kurve um die Achse (Punkt + Richtung) über den Winkel `sweep`
    /// (`0 < |sweep| ≤ 2π`). **u = Winkel** im Bereich `[start, start + sweep]`, aber u = Winkel
    /// nur an den Segmentgrenzen (dazwischen rational quadratisch, nicht linear im Winkel; für
    /// den Parameter zu einem Punkt die Rückrechnung `project` verwenden); **v = Kurvenparameter**.
    /// Liegt die ganze Profilkurve auf der Achse, ist das ein Fehler (keine Fläche). Bei negativem
    /// `sweep` überdeckt die Fläche dieselben Winkel, der u-Bereich ist dann steigend
    /// `[start + sweep, start]` (u bleibt der Winkel, die Orientierung wie bei positivem Winkel).
    pub fn revolve(
        curve: &NurbsCurve,
        axis_origin: DVec3,
        axis_dir: DVec3,
        start: f64,
        sweep: f64,
    ) -> Result<NurbsSurface, NurbsError> {
        check_point(axis_origin, || tr!("math-what-axis-point"))?;
        if !axis_dir.is_finite() || axis_dir.length() < MIN_DIRECTION_LENGTH {
            return Err(NurbsError::Input(tr!("math-axis-direction")));
        }
        check_sweep(start, sweep)?;
        let (s0, sw) = if sweep < 0.0 {
            (start + sweep, -sweep)
        } else {
            (start, sweep)
        };
        let d = axis_dir.normalize();
        let arc = arc_coeffs(s0, sw);
        let nu = arc.coeffs.len();
        let nv = curve.control_points().len();
        let mut control = vec![DVec3::ZERO; nu * nv];
        let mut weights = vec![0.0; nu * nv];
        let mut off_axis = false;
        for (j, (p, w)) in curve
            .control_points()
            .iter()
            .zip(curve.weights())
            .enumerate()
        {
            let o = axis_origin + d * (*p - axis_origin).dot(d);
            let mut x = *p - o;
            if x.length() < ON_AXIS_DISTANCE {
                x = DVec3::ZERO;
            }
            off_axis |= x != DVec3::ZERO;
            let y = d.cross(x);
            for (i, ((ca, cb), om)) in arc.coeffs.iter().zip(&arc.weights).enumerate() {
                control[i * nv + j] = if x == DVec3::ZERO {
                    *p
                } else {
                    o + x * *ca + y * *cb
                };
                weights[i * nv + j] = w * om;
            }
        }
        if !off_axis {
            return Err(NurbsError::Input(tr!("math-profile-on-axis")));
        }
        NurbsSurface::new(
            2,
            curve.degree(),
            arc_knots(arc.segments, s0, s0 + sw),
            curve.knots().knots().to_vec(),
            nu,
            nv,
            control,
            weights,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PI;

    #[test]
    fn negative_sweeps_and_bad_inputs() {
        let (a, b) = (DVec3::X, DVec3::Y);
        let c = NurbsCurve::elliptic_arc(DVec3::ZERO, a, b, 1.0, -2.0).unwrap();
        let (u0, u1) = c.domain();
        assert!(
            c.point(u0)
                .distance(DVec3::new(1f64.cos(), 1f64.sin(), 0.0))
                < 1e-14
        );
        assert!(
            c.point(u1)
                .distance(DVec3::new((-1f64).cos(), (-1f64).sin(), 0.0))
                < 1e-14
        );
        assert!(NurbsCurve::elliptic_arc(DVec3::ZERO, a, b, 0.0, 1e-12).is_err());
        assert!(NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::ZERO, b, 0.0, 1.0).is_err());
        assert!(NurbsCurve::elliptic_arc(DVec3::ZERO, a * 2e9, b, 0.0, 1.0).is_err());
        assert!(NurbsCurve::elliptic_arc(DVec3::ZERO, a, b, f64::INFINITY, 1.0).is_err());

        let line = NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![DVec3::X, DVec3::X + DVec3::Z],
            vec![1.0; 2],
        )
        .unwrap();
        let s = NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 1.0, -PI).unwrap();
        let ((u0, u1), _) = s.domain();
        assert_eq!((u0, u1), (1.0 - PI, 1.0));
        assert!(
            s.point(u0, 0.0)
                .distance(DVec3::new((1.0 - PI).cos(), (1.0 - PI).sin(), 0.0))
                < 1e-14
        );
        // Winkelbereich jenseits der Knotengrenze ergibt einen Fehler, keinen ungültigen Knoten
        assert!(NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 2e9, 1.0).is_err());
        assert!(NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, 1e-12).is_err());
        assert!(NurbsSurface::extrude(&line, DVec3::ZERO).is_err());
        let on_axis = NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![DVec3::Z, DVec3::Z * 3.0],
            vec![1.0; 2],
        )
        .unwrap();
        assert!(NurbsSurface::revolve(&on_axis, DVec3::ZERO, DVec3::Z, 0.0, TAU).is_err());
        assert!(NurbsSurface::extrude(&line, DVec3::new(f64::NAN, 0.0, 0.0)).is_err());
    }

    #[test]
    fn natural_cubic_rejects_bad_params() {
        let pts = [DVec3::ZERO, DVec3::X, DVec3::Y];
        assert!(NurbsCurve::natural_cubic(&pts, &[0.0, 1.0]).is_err());
        assert!(NurbsCurve::natural_cubic(&pts, &[0.0, f64::NAN, 2.0]).is_err());
        // Parameterlücke unter der Knotengrenze: Fehler statt ungültiger Knoten
        assert!(NurbsCurve::natural_cubic(&pts, &[0.0, 1e-12, 1.0]).is_err());
        assert!(NurbsCurve::natural_cubic(&pts, &[0.0, 1e-9, 2e-9]).is_err());
        let line = NurbsCurve::natural_cubic(&pts[..2], &[2.0, 5.0]).unwrap();
        assert_eq!(line.degree(), 1);
        let many = vec![DVec3::ZERO; MAX_FIT_CONTROL_POINTS];
        let ts: Vec<f64> = (0..MAX_FIT_CONTROL_POINTS).map(|i| i as f64).collect();
        assert!(NurbsCurve::natural_cubic(&many, &ts).is_err());
    }
}
