//! Annäherung einer Punktfolge durch eine kubische B-Spline-Kurve mit Fehlerschranke
//! (Ausgleich nach Piegl/Tiller 9.4.1, Knoten nach A9.7 bzw. Mittelung 9.8) und Abstand eines
//! Punktes zur Kurve.

use super::build::solve_components;
use super::{KnotVector, MAX_COORD, NurbsCurve, NurbsError};
use crate::DVec3;

/// Höchstzahl Kontrollpunkte einer Annäherung oder eines Interpolations-Splines
/// (begrenzt die Größe der dichten Gleichungssysteme).
pub const MAX_FIT_CONTROL_POINTS: usize = 512;
/// Ausgleich nach kleinsten Quadraten nur bis zu `Punktzahl / LSQ_MAX_FRACTION` Kontrollpunkten;
/// darüber wird interpoliert. Nahe an der Punktzahl hat der Ausgleich zu wenige Punkte je
/// Abschnitt, das System wird fast singulär und die Kontrollpunkte schwingen weit aus.
const LSQ_MAX_FRACTION: usize = 2;
/// Anfangszahl Kontrollpunkte der Annäherung (wird bis zur Schranke verdoppelt).
const FIT_START_CONTROL_POINTS: usize = 8;
/// Kleinste Gesamtlänge der Punktfolge in Millimetern (darunter fallen alle Punkte zusammen).
const MIN_FIT_LENGTH: f64 = 1e-12;
/// Aufeinanderfolgende Punkte, deren Abstand kleiner als dieser Bruchteil der Gesamtlänge ist,
/// werden für die Parametrisierung zusammengefasst (sonst entstünden Knoten unter der
/// Knotengrenze `MIN_KNOT_GAP_RATIO` = 1e-9 oder ein singuläres System). Der Wert liegt mit
/// Abstand über dieser Grenze, damit gemittelte Knoten (Lücke ≥ kleinste Parameterlücke) sie auch
/// nach Rundung einhalten. Zusammengefasste Punkte liegen höchstens 1e-8 · Gesamtlänge von einem
/// verbliebenen Punkt entfernt; der Fehler wird trotzdem für **alle** Originalpunkte geprüft, die
/// Fehlerschranke gilt also unverändert auch für sie.
const MIN_CHORD_RATIO: f64 = 1e-8;
/// Probestellen von `distance_to` je Kontrollpunkt.
const DISTANCE_SAMPLES_PER_CONTROL: usize = 16;
/// Zahl der besten örtlichen Probenminima, von denen `distance_to` Newton startet (mehrere, weil
/// das beste Probenminimum nicht immer im Einzugsbereich des wahren Minimums liegt).
const DISTANCE_NEWTON_STARTS: usize = 3;
/// Höchstzahl Newton-Schritte in `distance_to`.
const DISTANCE_NEWTON_STEPS: usize = 30;
/// Newton in `distance_to` endet, wenn der Schritt kleiner als dieser Bruchteil des
/// Parameterbereichs ist.
const DISTANCE_STEP_RATIO: f64 = 1e-15;

impl NurbsCurve {
    /// Ausgleichskurve (Grad 3, bei 2 oder 3 Punkten Grad 1 bzw. 2) durch die Punkte mit
    /// Fehlerschranke `tol` (größter Abstand der Punkte zur Kurve, gemessen mit `distance_to`).
    /// Beginnt mit wenigen Kontrollpunkten und verdoppelt, bis die Schranke gilt oder die
    /// Punktzahl erreicht ist (dann Interpolation). Anfang und Ende der Kurve sind der erste und
    /// letzte Punkt. Höchstens `MAX_FIT_CONTROL_POINTS` Kontrollpunkte; wird die Schranke damit
    /// nicht erreicht, ist das ein Fehler.
    pub fn fit(points: &[DVec3], tol: f64) -> Result<NurbsCurve, NurbsError> {
        fit_capped(points, tol, MAX_FIT_CONTROL_POINTS)
    }

    /// Abstand eines Punktes zur Kurve: gleichmäßige Probe (16 Stellen je Kontrollpunkt), dann
    /// höchstens 30 Newton-Schritte auf `(C(u) − p) · C'(u) = 0` (begrenzt auf den
    /// Parameterbereich) von den `DISTANCE_NEWTON_STARTS` besten örtlichen Minima der Probe und
    /// zusätzlich von beiden Enden aus (bei geschlossenen Kurven liegt das wahre Minimum oft knapp
    /// hinter der Naht). Das Ergebnis ist immer der Abstand zu einem echten Kurvenpunkt (nie
    /// kleiner als der wahre Abstand). Nicht endliche Punkte ergeben `NaN`.
    pub fn distance_to(&self, p: DVec3) -> f64 {
        if !p.is_finite() {
            return f64::NAN;
        }
        let (a, b) = self.domain();
        let samples = DISTANCE_SAMPLES_PER_CONTROL * self.control_points().len();
        let u_at = |i: usize| a + (b - a) * i as f64 / samples as f64;
        let d: Vec<f64> = (0..=samples)
            .map(|i| self.point(u_at(i)).distance_squared(p))
            .collect();
        let mut minima: Vec<usize> = (0..=samples)
            .filter(|&i| (i == 0 || d[i] <= d[i - 1]) && (i == samples || d[i] <= d[i + 1]))
            .collect();
        minima.sort_by(|x, y| d[*x].total_cmp(&d[*y]));
        minima
            .iter()
            .take(DISTANCE_NEWTON_STARTS)
            .map(|i| u_at(*i))
            .chain([a, b])
            .map(|u| self.newton_distance(u, p))
            .fold(f64::INFINITY, f64::min)
    }

    /// Abstand von `p` zum nächsten Kurvenpunkt, den Newton von `u` aus findet (höchstens
    /// `DISTANCE_NEWTON_STEPS` Schritte); immer der Abstand zu einem echten Kurvenpunkt, also
    /// nie kleiner als der wahre Abstand.
    fn newton_distance(&self, u: f64, p: DVec3) -> f64 {
        let (a, b) = self.domain();
        let mut u = u.clamp(a, b);
        let mut best = self.point(u).distance_squared(p);
        for _ in 0..DISTANCE_NEWTON_STEPS {
            let c = self.derivs(u, 2);
            let r = c[0] - p;
            let f = r.dot(c[1]);
            let df = c[1].length_squared() + r.dot(c[2]);
            if df.is_nan() || df <= 0.0 || !f.is_finite() {
                break;
            }
            let next = (u - f / df).clamp(a, b);
            best = best.min(self.point(next).distance_squared(p));
            if (next - u).abs() <= DISTANCE_STEP_RATIO * (b - a) {
                break;
            }
            u = next;
        }
        best.sqrt()
    }
}

/// `fit` mit wählbarer Höchstzahl Kontrollpunkten (`cap ≤ MAX_FIT_CONTROL_POINTS`).
///
/// Fehlermaß: je Punkt zuerst Newton vom eigenen Parameter aus (Abstand zu einem echten
/// Kurvenpunkt, also eine obere Schranke des wahren Abstands); nur wenn das die Schranke
/// überschreitet, zusätzlich `distance_to` (globale Suche). So bleibt der Aufwand je Durchgang
/// etwa linear in der Punktzahl.
fn fit_capped(points: &[DVec3], tol: f64, cap: usize) -> Result<NurbsCurve, NurbsError> {
    if points.len() < 2 {
        return Err(NurbsError::Input("mindestens zwei Punkte nötig".into()));
    }
    if !tol.is_finite() || tol <= 0.0 {
        return Err(NurbsError::Input(
            "die Fehlerschranke muss endlich und positiv sein".into(),
        ));
    }
    if points
        .iter()
        .any(|p| !p.is_finite() || p.abs().max_element() > MAX_COORD)
    {
        return Err(NurbsError::Input(format!(
            "Punkte müssen endlich sein und höchstens {MAX_COORD} betragen"
        )));
    }
    let total: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    if total.is_nan() || total <= MIN_FIT_LENGTH {
        return Err(NurbsError::Fit(
            "die Punkte fallen zusammen (Gesamtlänge null)".into(),
        ));
    }
    // Zu nahe aufeinanderfolgende Punkte zusammenfassen; der letzte Punkt bleibt immer.
    // `rep[i]` ist der Index des Punktes in `q`, der Punkt `i` vertritt.
    let min_chord = MIN_CHORD_RATIO * total;
    let mut q: Vec<DVec3> = vec![points[0]];
    let mut rep = vec![0usize; points.len()];
    for (i, x) in points.iter().enumerate().skip(1) {
        if x.distance(q[q.len() - 1]) >= min_chord {
            q.push(*x);
        }
        rep[i] = q.len() - 1;
    }
    let last = points[points.len() - 1];
    if q[q.len() - 1] != last {
        // Der letzte Punkt wurde mit Vorgängern zusammengefasst: er ersetzt sie (außer den
        // Anfang); Vertreter der entfernten Punkte werden auf den neuen letzten begrenzt.
        while q.len() > 1 && q[q.len() - 1].distance(last) < min_chord {
            q.pop();
        }
        q.push(last);
        let r = q.len() - 1;
        for v in &mut rep {
            *v = (*v).min(r);
        }
        rep[points.len() - 1] = r;
    }
    if q.len() < 2 {
        return Err(NurbsError::Fit(
            "die Punkte fallen zusammen (Gesamtlänge null)".into(),
        ));
    }
    let params = chord_params(&q);
    let n = q.len();
    let p = 3.min(n - 1);
    let cap = n.min(cap).min(MAX_FIT_CONTROL_POINTS);
    let mut m = next_count(cap.min(FIT_START_CONTROL_POINTS.max(p + 1)), n, cap);
    loop {
        // Ein misslungener Ausgleich (singulär, Kontrollpunkte außerhalb der Grenzen) gilt als
        // „nicht gut genug“: weiter mit mehr Kontrollpunkten bzw. Interpolation.
        let (curve, failure) = if m == n {
            (Some(interpolate(&q, &params, p)?), None)
        } else {
            match least_squares(&q, &params, p, m) {
                Ok(c) => (Some(c), None),
                Err(e) => (None, Some(e)),
            }
        };
        // Größter Abstand; beim ersten Punkt über der Schranke wird abgebrochen (die Runde ist
        // dann ohnehin verloren, die teure globale Suche für alle weiteren Punkte entfällt).
        let err = match &curve {
            Some(c) => {
                let mut worst = 0.0_f64;
                for (x, r) in points.iter().zip(&rep) {
                    let mut d = c.newton_distance(params[*r], *x);
                    if d > tol {
                        d = d.min(c.distance_to(*x));
                    }
                    // `!(d <= tol)` fängt auch NaN.
                    if d.is_nan() || d > tol {
                        worst = if d.is_nan() { f64::INFINITY } else { d };
                        break;
                    }
                    worst = worst.max(d);
                }
                worst
            }
            None => f64::INFINITY,
        };
        if let Some(c) = curve
            && err <= tol
        {
            return Ok(c);
        }
        if m >= cap {
            let why = match failure {
                Some(e) => format!("Ausgleich misslungen: {e}"),
                None => format!("Abstand {err} über der Schranke"),
            };
            return Err(NurbsError::Fit(format!(
                "Fehlerschranke {tol} mit höchstens {cap} Kontrollpunkten nicht erreichbar ({why})"
            )));
        }
        m = next_count(2 * m, n, cap);
    }
}

/// Nächste Kontrollpunktzahl: höchstens `cap`; liegt sie über `n / LSQ_MAX_FRACTION` und ist
/// Interpolation erlaubt (`n ≤ cap`), wird gleich interpoliert (`n`).
fn next_count(m: usize, n: usize, cap: usize) -> usize {
    if m >= n || (m * LSQ_MAX_FRACTION > n && n <= cap) {
        n.min(cap)
    } else {
        m.min(cap)
    }
}

/// Sehnenlängen-Parameter in `[0, 1]` (Anfang und Ende exakt).
fn chord_params(q: &[DVec3]) -> Vec<f64> {
    let mut t = Vec::with_capacity(q.len());
    let mut s = 0.0;
    t.push(0.0);
    for w in q.windows(2) {
        s += w[0].distance(w[1]);
        t.push(s);
    }
    let total = s;
    for v in &mut t {
        *v /= total;
    }
    let last = t.len() - 1;
    t[last] = 1.0;
    t
}

/// Interpolation (P/T 9.2.1): Knoten durch Mittelung der Parameter (9.8), `n × n`-System.
fn interpolate(q: &[DVec3], t: &[f64], p: usize) -> Result<NurbsCurve, NurbsError> {
    let n = q.len();
    let mut knots = vec![0.0; p + 1];
    for j in 1..n - p {
        knots.push(t[j..j + p].iter().sum::<f64>() / p as f64);
    }
    knots.extend(vec![1.0; p + 1]);
    let kv = KnotVector::new(p, knots.clone())?;
    let mut mat = vec![0.0; n * n];
    for (k, tk) in t.iter().enumerate() {
        let span = kv.find_span(*tk);
        for (j, v) in kv.basis(span, *tk).iter().enumerate() {
            mat[k * n + span - p + j] = *v;
        }
    }
    let control = solve_components(&mat, q, n)?;
    NurbsCurve::new(p, knots, control, vec![1.0; n])
}

/// Ausgleich nach der Methode der kleinsten Quadrate (P/T 9.4.1) mit `m` Kontrollpunkten,
/// festem ersten und letzten Punkt; Knoten nach A9.7 (jeder Abschnitt enthält Parameter).
fn least_squares(q: &[DVec3], t: &[f64], p: usize, m: usize) -> Result<NurbsCurve, NurbsError> {
    let n = q.len();
    let d = n as f64 / (m - p) as f64;
    let mut knots = vec![0.0; p + 1];
    for j in 1..m - p {
        let jd = j as f64 * d;
        let i = (jd.floor() as usize).clamp(1, n - 1);
        let alpha = jd - i as f64;
        knots.push((1.0 - alpha) * t[i - 1] + alpha * t[i]);
    }
    knots.extend(vec![1.0; p + 1]);
    let kv = KnotVector::new(p, knots.clone())?;
    let (q0, qn) = (q[0], q[n - 1]);
    let k = m - 2; // Unbekannte P_1 … P_{m−2}
    let mut ntn = vec![0.0; k * k];
    let mut ntr = vec![DVec3::ZERO; k];
    for (qk, tk) in q[1..n - 1].iter().zip(&t[1..n - 1]) {
        let span = kv.find_span(*tk);
        let basis = kv.basis(span, *tk);
        let idx = |j: usize| span - p + j;
        let mut r = *qk;
        for (j, v) in basis.iter().enumerate() {
            if idx(j) == 0 {
                r -= q0 * *v;
            } else if idx(j) == m - 1 {
                r -= qn * *v;
            }
        }
        for (a, va) in basis.iter().enumerate() {
            let ia = idx(a);
            if ia == 0 || ia == m - 1 {
                continue;
            }
            ntr[ia - 1] += r * *va;
            for (b, vb) in basis.iter().enumerate() {
                let ib = idx(b);
                if ib == 0 || ib == m - 1 {
                    continue;
                }
                ntn[(ia - 1) * k + ib - 1] += va * vb;
            }
        }
    }
    let inner = solve_components(&ntn, &ntr, k)?;
    let mut control = Vec::with_capacity(m);
    control.push(q0);
    control.extend(inner);
    control.push(qn);
    NurbsCurve::new(p, knots, control, vec![1.0; m])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_handles_duplicates_small_sets_and_bad_tolerance() {
        let pts = [
            DVec3::ZERO,
            DVec3::ZERO,
            DVec3::X,
            DVec3::X,
            DVec3::new(2.0, 1.0, 0.0),
        ];
        let c = NurbsCurve::fit(&pts, 1e-9).unwrap();
        for p in &pts {
            assert!(c.distance_to(*p) < 1e-9);
        }
        let line = NurbsCurve::fit(&[DVec3::ZERO, DVec3::X], 1e-9).unwrap();
        assert_eq!(line.degree(), 1);
        assert!(NurbsCurve::fit(&pts, 0.0).is_err());
        assert!(NurbsCurve::fit(&pts, f64::NAN).is_err());
        assert!(NurbsCurve::fit(&[DVec3::ZERO, DVec3::X * 2e9], 1.0).is_err());
    }

    #[test]
    fn fit_reports_an_unreachable_bound() {
        // Zickzackfolge mit mehr Punkten als erlaubten Kontrollpunkten (kleine Grenze, damit der
        // Test schnell bleibt; der Weg ist derselbe wie mit MAX_FIT_CONTROL_POINTS)
        let pts: Vec<DVec3> = (0..40)
            .map(|i| DVec3::new(i as f64, if i % 2 == 0 { 0.0 } else { 1.0 }, 0.0))
            .collect();
        assert!(matches!(
            fit_capped(&pts, 1e-6, 16),
            Err(NurbsError::Fit(_))
        ));
        // ohne Grenze unter der Punktzahl wird interpoliert
        let c = fit_capped(&pts, 1e-6, MAX_FIT_CONTROL_POINTS).unwrap();
        assert_eq!(c.control_points().len(), pts.len());
    }

    #[test]
    fn distance_of_a_point_to_a_line() {
        let c = NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![DVec3::ZERO, DVec3::X * 4.0],
            vec![1.0; 2],
        )
        .unwrap();
        assert!((c.distance_to(DVec3::new(1.3, 2.0, 0.0)) - 2.0).abs() < 1e-14);
        assert!((c.distance_to(DVec3::new(7.0, 4.0, 0.0)) - 5.0).abs() < 1e-14);
        assert!(c.distance_to(DVec3::new(f64::NAN, 0.0, 0.0)).is_nan());
    }
}
