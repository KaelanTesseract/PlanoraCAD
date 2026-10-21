//! Freiformkurven und -flächen, Dreh- und Strangflächen aus STEP (STEP-Import, Aufgabe 5).
//!
//! STEP schreibt Knoten als Werte mit Vielfachheiten oder nur als Art (gleichförmig, fast
//! gleichförmig, Bézier); der Kernel verlangt geklemmte Knotenvektoren (je Grad + 1 gleiche
//! Endknoten). Ungeklemmte Vektoren werden deshalb durch Knoteneinfügen (Boehm, Piegl/Tiller A5.1,
//! auf homogenen Punkten) geklemmt: Der Bereich `[k[p], k[n]]` und die Kurve bleiben dieselben.
//!
//! Dreh- und Strangflächen (P8) werden, wo das exakt geht, analytisch (Ebene, Zylinder, Kegel,
//! Kugel, Torus), sonst als NURBS übernommen. Ob die natürliche Normale des Kernels gegen die
//! Normale von STEP zeigt (`flipped`), wird an einem Probepunkt gemessen: STEP-Normale aus der
//! Definition `σ(u, v)` (Ableitungen per zentraler Differenz), Kernel-Normale am Fußpunkt.
//!
//! Die Hilfen [`expand_knots`], [`implicit_knots`], [`clamp_knots`] und [`normalize_weights`]
//! liefern im Fehlerfall nur den Grund; der Resolver setzt „Eine Freiformfläche lässt sich nicht
//! übernehmen: …“ (bzw. „…kurve…“) und die Zeile davor. Die übrigen liefern ganze Sätze.

use crate::read::geom::{Skip, SurfaceGeom};
use planora_kernel::{Curve, KernelError, Surface};
use planora_math::nurbs::{MAX_DEGREE, MIN_KNOT_GAP_RATIO, MIN_WEIGHT, NurbsCurve, NurbsSurface};
use planora_math::{DVec3, Frame, TAU};
use std::ops::{Add, Mul};

/// Höchstzahl Knoten je Richtung (Summe der Vielfachheiten).
pub const MAX_KNOTS: usize = 10_000;

/// Abstand, bis zu dem erste und letzte Kontrollreihe als gleich gelten (wie im Kernel).
const SAME_ROW: f64 = 1e-9;

/// Gewichte gelten als gleich bis zu diesem Anteil; beim Mitteln bis zu [`CLOSE_WEIGHT`].
const SAME_WEIGHT: f64 = 1e-9;
const CLOSE_WEIGHT: f64 = 1e-6;

/// Zwei Einheitsrichtungen sind parallel bzw. senkrecht bis zu diesem Wert (Kreuz- bzw.
/// Skalarprodukt).
const ANGLE_EPS: f64 = 1e-10;

/// Abstand (relativ zu `1 + Abstand zur Achse`), bis zu dem ein Punkt auf der Achse liegt bzw.
/// eine Gerade die Achse schneidet.
const LINEAR_EPS: f64 = 1e-9;

/// Schrittweite der zentralen Differenzen, bezogen auf den Parameterbereich.
const DIFF_STEP: f64 = 1e-6;

/// Rand der Strangfläche über die Schleifen hinaus, bezogen auf deren Ausdehnung.
const EXTRUSION_MARGIN: f64 = 1e-6;

fn skip(m: impl Into<String>) -> Skip {
    Skip(m.into())
}

/// Knoten aus Vielfachheiten und Werten (STEP-Form); Summen > 10 000 oder fallende Werte → Skip.
pub fn expand_knots(mults: &[i64], values: &[f64]) -> Result<Vec<f64>, Skip> {
    if mults.len() != values.len() {
        return Err(skip(
            "Die Listen der Knoten und ihrer Vielfachheiten sind verschieden lang.",
        ));
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Err(skip("Ein Knoten ist keine endliche Zahl."));
    }
    if values.windows(2).any(|w| w[1] < w[0]) {
        return Err(skip("Die Knoten fallen."));
    }
    let mut total: usize = 0;
    for m in mults {
        let Ok(m) = usize::try_from(*m) else {
            return Err(skip("Eine Vielfachheit ist kleiner als 1."));
        };
        if m == 0 {
            return Err(skip("Eine Vielfachheit ist kleiner als 1."));
        }
        total = total.saturating_add(m);
        if total > MAX_KNOTS {
            return Err(skip("Es gibt mehr als 10 000 Knoten."));
        }
    }
    let mut out = Vec::with_capacity(total);
    for (m, v) in mults.iter().zip(values) {
        // `m` ist oben geprüft (1 ≤ m ≤ 10 000).
        out.extend(std::iter::repeat_n(*v, *m as usize));
    }
    Ok(out)
}

/// Knoten für UNIFORM/QUASI_UNIFORM/BEZIER/PIECEWISE_BEZIER-Formen (knot_type ohne Liste).
/// `kind` ist der Name der Knotenart (`UNIFORM_KNOTS`, `QUASI_UNIFORM_KNOTS`, `BEZIER_KNOTS`,
/// `PIECEWISE_BEZIER_KNOTS`), `count` die Zahl der Kontrollpunkte (ISO 10303-42).
pub fn implicit_knots(kind: &str, degree: usize, count: usize) -> Result<Vec<f64>, Skip> {
    if degree == 0 || degree > MAX_DEGREE {
        return Err(skip(format!(
            "Der Grad {degree} ist nicht erlaubt (1 bis {MAX_DEGREE})."
        )));
    }
    if count < degree + 1 {
        return Err(skip("Es gibt zu wenige Kontrollpunkte für den Grad."));
    }
    if count + degree + 1 > MAX_KNOTS {
        return Err(skip("Es gibt mehr als 10 000 Knoten."));
    }
    let p = degree;
    match kind {
        // −p, −p + 1, …, count: gleiche Abstände, ungeklemmt.
        "UNIFORM_KNOTS" => Ok((0..count + p + 1).map(|i| i as f64 - p as f64).collect()),
        // p + 1 Nullen, dann 1, 2, …, zuletzt p + 1-mal count − p.
        "QUASI_UNIFORM_KNOTS" => {
            let last = (count - p) as f64;
            let mut k = vec![0.0; p + 1];
            k.extend((1..count - p).map(|i| i as f64));
            k.extend(std::iter::repeat_n(last, p + 1));
            Ok(k)
        }
        // Bézier-Stücke: Ränder p + 1-fach, innen p-fach, Werte 0, 1, 2, …
        "BEZIER_KNOTS" | "PIECEWISE_BEZIER_KNOTS" => {
            if !(count - 1).is_multiple_of(p) {
                return Err(skip(
                    "Die Zahl der Kontrollpunkte passt nicht zu einer Bézier-Form.",
                ));
            }
            let segments = (count - 1) / p;
            let mut k = vec![0.0; p + 1];
            for s in 1..segments {
                k.extend(std::iter::repeat_n(s as f64, p));
            }
            k.extend(std::iter::repeat_n(segments as f64, p + 1));
            Ok(k)
        }
        other => Err(skip(format!(
            "Die Knotenart {other} wird nicht unterstützt."
        ))),
    }
}

/// Prüft Grad und Knoten (vor dem Klemmen): Grad 1 bis 9, passende Zahl, nicht fallend, Bereich
/// `[k[p], k[n]]` nicht leer, keine innere Vielfachheit über dem Grad, kein Knotenabstand unter
/// `MIN_KNOT_GAP_RATIO` · Bereich.
pub fn check_knots(degree: usize, knots: &[f64], count: usize) -> Result<(), Skip> {
    if degree == 0 || degree > MAX_DEGREE {
        return Err(skip(format!(
            "Der Grad {degree} ist nicht erlaubt (1 bis {MAX_DEGREE})."
        )));
    }
    if count < degree + 1 {
        return Err(skip("Es gibt zu wenige Kontrollpunkte für den Grad."));
    }
    if knots.len() != count + degree + 1 {
        return Err(skip(
            "Die Zahl der Knoten passt nicht zu Grad und Kontrollpunkten.",
        ));
    }
    if knots.iter().any(|k| !k.is_finite()) {
        return Err(skip("Ein Knoten ist keine endliche Zahl."));
    }
    if knots.windows(2).any(|w| w[1] < w[0]) {
        return Err(skip("Die Knoten fallen."));
    }
    let (a, b) = (knots[degree], knots[count]);
    if b <= a {
        return Err(skip("Der Parameterbereich ist leer."));
    }
    let mut run = 1;
    for w in knots.windows(2) {
        run = if w[1] == w[0] { run + 1 } else { 1 };
        if run > degree && w[0] > a && w[0] < b {
            return Err(skip(
                "Ein innerer Knoten kommt öfter vor, als der Grad erlaubt.",
            ));
        }
    }
    let gap = knots[degree..=count]
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|g| *g > 0.0)
        .fold(f64::INFINITY, f64::min);
    if gap < MIN_KNOT_GAP_RATIO * (b - a) {
        return Err(skip("Zwei Knoten liegen zu nah beieinander."));
    }
    Ok(())
}

/// Fügt `u` einmal ein (Boehm, A5.1); `k`: letzter Index mit `knots[k] <= u`, `s`: Vielfachheit
/// von `u`. Verlangt `knots[k] <= u < knots[k + 1]`.
fn insert_once<V>(p: usize, knots: &mut Vec<f64>, control: &mut Vec<V>, u: f64)
where
    V: Copy + Add<Output = V> + Mul<f64, Output = V>,
{
    let k = knots.iter().rposition(|x| *x <= u).unwrap_or(0);
    let s = knots.iter().filter(|x| **x == u).count();
    let n = control.len();
    let mut q = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let v = if i + p <= k {
            control[i]
        } else if i + s <= k {
            let alpha = (u - knots[i]) / (knots[i + p] - knots[i]);
            control[i] * alpha + control[i - 1] * (1.0 - alpha)
        } else {
            control[i - 1]
        };
        q.push(v);
    }
    knots.insert(k + 1, u);
    *control = q;
}

/// Klemmt den Anfang: `a = knots[p]` bis zur Vielfachheit `p` einfügen, dann vorne so viele
/// Knoten und Kontrollpunkte weglassen, dass genau `p + 1` Knoten `a` am Anfang stehen.
fn clamp_start<V>(p: usize, knots: &mut Vec<f64>, control: &mut Vec<V>)
where
    V: Copy + Add<Output = V> + Mul<f64, Output = V>,
{
    let a = knots[p];
    if knots[..=p].iter().all(|k| *k == a) && knots.get(p + 1).is_some_and(|k| *k > a) {
        return;
    }
    let s = knots.iter().filter(|k| **k == a).count();
    for _ in s..p {
        insert_once(p, knots, control, a);
    }
    let last = knots.iter().rposition(|k| *k == a).unwrap_or(p);
    let start = last - p;
    knots.drain(..start);
    knots[0] = a;
    control.drain(..start);
}

/// Klemmt einen Knotenvektor durch Knoteneinfügen (Boehm, A5.1, homogen): Bereich
/// [k[p], k[n]] bleibt, Kurve unverändert, danach geklemmt (je p+1 gleiche Endknoten).
/// `V` ist der homogene Punkt `(w·P, w)` (z. B. `DVec3::extend`), bei Flächen eine Reihe davon.
pub fn clamp_knots<V>(
    degree: usize,
    knots: &[f64],
    control: &[V],
) -> Result<(Vec<f64>, Vec<V>), Skip>
where
    V: Copy + Add<Output = V> + Mul<f64, Output = V>,
{
    check_knots(degree, knots, control.len())?;
    let p = degree;
    let mut k = knots.to_vec();
    let mut c = control.to_vec();
    clamp_start(p, &mut k, &mut c);
    // Das Ende über die gespiegelte Folge (Negieren ist exakt).
    let mut mk: Vec<f64> = k.iter().rev().map(|x| -x).collect();
    c.reverse();
    clamp_start(p, &mut mk, &mut c);
    let k: Vec<f64> = mk.iter().rev().map(|x| -x).collect();
    c.reverse();
    Ok((k, c))
}

/// Teilt alle Gewichte durch das größte; kleinstes danach < MIN_WEIGHT → Skip.
pub fn normalize_weights(w: &mut [f64]) -> Result<(), Skip> {
    if w.iter().any(|x| !(x.is_finite() && *x > 0.0)) {
        return Err(skip("Ein Gewicht ist nicht größer als null."));
    }
    let max = w.iter().copied().fold(0.0, f64::max);
    if max <= 0.0 {
        return Err(skip("Es gibt keine Gewichte."));
    }
    for x in w.iter_mut() {
        *x /= max;
    }
    if w.iter().any(|x| *x < MIN_WEIGHT) {
        return Err(skip(
            "Die Gewichte sind zu verschieden (das kleinste ist kleiner als ein Millionstel des größten).",
        ));
    }
    Ok(())
}

/// Kurve des Kernels als NURBS über [t0, t1] (Gerade Grad 1, Kreis/Ellipse rational Grad 2).
/// Eine Freiformkurve wird ganz übernommen (ihr eigener Bereich; `t0`, `t1` zählen nicht).
pub fn curve_to_nurbs(c: &Curve, t0: f64, t1: f64) -> Result<NurbsCurve, Skip> {
    let why = |e: planora_math::nurbs::NurbsError| {
        skip(format!(
            "Eine Freiformkurve lässt sich nicht übernehmen: {e}"
        ))
    };
    match c {
        Curve::Line { .. } => NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![c.point(t0), c.point(t1)],
            vec![1.0, 1.0],
        )
        .map_err(why),
        Curve::Circle { frame, radius } => NurbsCurve::elliptic_arc(
            frame.origin,
            frame.x * *radius,
            frame.y * *radius,
            t0,
            t1 - t0,
        )
        .map_err(why),
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => NurbsCurve::elliptic_arc(
            frame.origin,
            frame.x * *major,
            frame.y * *minor,
            t0,
            t1 - t0,
        )
        .map_err(why),
        Curve::Nurbs(n) => Ok((**n).clone()),
    }
}

/// Teile einer Fläche: Grade, Knoten, Zahl und Kontrollpunkte samt Gewichten (Index `i·nv + j`).
struct Parts {
    du: usize,
    dv: usize,
    ku: Vec<f64>,
    kv: Vec<f64>,
    nu: usize,
    nv: usize,
    control: Vec<DVec3>,
    weights: Vec<f64>,
}

impl Parts {
    fn of(s: &NurbsSurface) -> Parts {
        let (nu, nv) = s.counts();
        let mut control = Vec::with_capacity(nu * nv);
        let mut weights = Vec::with_capacity(nu * nv);
        for i in 0..nu {
            for j in 0..nv {
                control.push(s.control_at(i, j));
                weights.push(s.weight_at(i, j));
            }
        }
        Parts {
            du: s.degree_u(),
            dv: s.degree_v(),
            ku: s.knots_u().knots().to_vec(),
            kv: s.knots_v().knots().to_vec(),
            nu,
            nv,
            control,
            weights,
        }
    }

    fn build(self) -> Result<NurbsSurface, Skip> {
        NurbsSurface::new(
            self.du,
            self.dv,
            self.ku,
            self.kv,
            self.nu,
            self.nv,
            self.control,
            self.weights,
        )
        .map_err(|e| skip(e.to_string()))
    }

    /// u und v getauscht (Kontrollpunkt `(i, j)` wird `(j, i)`).
    fn transposed(self) -> Parts {
        let mut control = Vec::with_capacity(self.control.len());
        let mut weights = Vec::with_capacity(self.weights.len());
        for j in 0..self.nv {
            for i in 0..self.nu {
                control.push(self.control[i * self.nv + j]);
                weights.push(self.weights[i * self.nv + j]);
            }
        }
        Parts {
            du: self.dv,
            dv: self.du,
            ku: self.kv,
            kv: self.ku,
            nu: self.nv,
            nv: self.nu,
            control,
            weights,
        }
    }

    /// Indizes der ersten und letzten Reihe in u (`along_u`) bzw. v, paarweise.
    fn edge_pairs(&self, along_u: bool) -> Vec<(usize, usize)> {
        let (nu, nv) = (self.nu, self.nv);
        if along_u {
            (0..nv).map(|j| (j, (nu - 1) * nv + j)).collect()
        } else {
            (0..nu).map(|i| (i * nv, i * nv + nv - 1)).collect()
        }
    }

    /// Geschlossen in dieser Richtung: erste und letzte Reihe gleich (≤ 1e-9) → ja; bis `weld_tol`
    /// → beide Reihen auf ihren Mittelwert gesetzt, dann ja; sonst nein.
    fn close(&mut self, along_u: bool, weld_tol: f64) -> bool {
        let pairs = self.edge_pairs(along_u);
        let mut dist: f64 = 0.0;
        let mut wrel: f64 = 0.0;
        for &(a, b) in &pairs {
            dist = dist.max(self.control[a].distance(self.control[b]));
            let (wa, wb) = (self.weights[a], self.weights[b]);
            wrel = wrel.max((wa - wb).abs() / wa.max(wb));
        }
        if dist <= SAME_ROW && wrel <= SAME_WEIGHT {
            return true;
        }
        if dist <= weld_tol && wrel <= CLOSE_WEIGHT {
            for (a, b) in pairs {
                let p = (self.control[a] + self.control[b]) * 0.5;
                let w = 0.5 * (self.weights[a] + self.weights[b]);
                self.control[a] = p;
                self.control[b] = p;
                self.weights[a] = w;
                self.weights[b] = w;
            }
            return true;
        }
        false
    }

    /// Eine Reihe mit festem u-Index fällt zu einem Punkt zusammen (Pol in u-Richtung).
    fn column_collapsed(&self) -> bool {
        (0..self.nu).any(|i| {
            let row = &self.control[i * self.nv..(i + 1) * self.nv];
            row.iter().all(|p| p.distance(row[0]) <= SAME_ROW)
        })
    }
}

fn kernel_reason(e: KernelError) -> Skip {
    match e {
        KernelError::InvalidSurface(m) => Skip(m),
        other => Skip(other.to_string()),
    }
}

/// Freiformfläche des Kernels: geschlossen in u bzw. v genau dann, wenn erste und letzte
/// Kontrollreihe gleich sind (bis `weld_tol` gemittelt); fällt eine Reihe mit festem u-Index
/// zusammen (Pol in u-Richtung, vom Kernel abgelehnt), werden u und v getauscht. Liefert die
/// Fläche und ob getauscht wurde (dann zeigt die natürliche Normale andersherum). Der Fehler ist
/// nur der Grund (Meldung des Kernels).
pub fn finish_nurbs_surface(ns: NurbsSurface, weld_tol: f64) -> Result<(Surface, bool), Skip> {
    let mut parts = Parts::of(&ns);
    let pu = parts.close(true, weld_tol);
    let pv = parts.close(false, weld_tol);
    let collapsed = parts.column_collapsed();
    let s = parts.build()?;
    match Surface::nurbs(s.clone(), pu, pv) {
        Ok(surface) => Ok((surface, false)),
        Err(e) if !collapsed => Err(kernel_reason(e)),
        Err(_) => {
            let t = Parts::of(&s).transposed().build()?;
            Surface::nurbs(t, pv, pu)
                .map(|surface| (surface, true))
                .map_err(kernel_reason)
        }
    }
}

/// Misst, ob die natürliche Normale der Fläche im Punkt `p` gegen `step_normal` zeigt.
// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn measured_flip(surface: &Surface, p: DVec3, step_normal: DVec3) -> Result<bool, Skip> {
    let unclear = || skip("Die Richtung einer Fläche lässt sich nicht bestimmen.");
    let len = step_normal.length();
    if !(len.is_finite() && len > 0.0) {
        return Err(unclear());
    }
    let (u, v) = surface.project(p);
    let foot = surface.point(u, v);
    if !(foot.distance(p) <= 1e-6 * (1.0 + p.length())) {
        return Err(unclear());
    }
    let d = surface.normal(u, v).dot(step_normal / len);
    if !(d.abs() >= 0.5) {
        return Err(unclear());
    }
    Ok(d < 0.0)
}

/// Breite des Parameterbereichs einer Kurve für die Schrittweite der Differenzen.
fn curve_range(c: &Curve) -> f64 {
    match c {
        Curve::Line { .. } => 1.0,
        Curve::Circle { .. } | Curve::Ellipse { .. } => TAU,
        Curve::Nurbs(n) => {
            let (a, b) = n.domain();
            b - a
        }
    }
}

/// Probestellen auf der Kurve (Gerade: nur `line_t`).
fn curve_samples(c: &Curve, line_t: f64) -> Vec<f64> {
    match c {
        Curve::Line { .. } => vec![line_t],
        Curve::Circle { .. } | Curve::Ellipse { .. } => {
            (0..8).map(|k| k as f64 * TAU / 8.0 + 0.1).collect()
        }
        Curve::Nurbs(n) => {
            let (a, b) = n.domain();
            (1..10).map(|k| a + (b - a) * k as f64 / 10.0).collect()
        }
    }
}

/// STEP-Normale `σ_u × σ_v` mit zentralen Differenzen; liefert Punkt und Normale an der Stelle
/// mit der längsten Normale unter `samples` (Kurvenparameter), dazu `v = v_at`.
fn step_probe(
    sigma: impl Fn(f64, f64) -> DVec3,
    samples: &[f64],
    hu: f64,
    hv: f64,
    v_at: f64,
) -> Option<(DVec3, DVec3)> {
    samples
        .iter()
        .map(|&u| {
            let su = (sigma(u + hu, v_at) - sigma(u - hu, v_at)) / (2.0 * hu);
            let sv = (sigma(u, v_at + hv) - sigma(u, v_at - hv)) / (2.0 * hv);
            (sigma(u, v_at), su.cross(sv))
        })
        .filter(|(p, n)| p.is_finite() && n.is_finite())
        .max_by(|a, b| a.1.length().total_cmp(&b.1.length()))
}

/// Dreht `x` um die Einheitsachse `axis` um `angle` (rechtshändig).
fn rotate(x: DVec3, axis: DVec3, angle: f64) -> DVec3 {
    let (s, c) = angle.sin_cos();
    x * c + axis.cross(x) * s + axis * (axis.dot(x) * (1.0 - c))
}

/// Frame mit Ursprung `origin`, Achse `z` und `x` (beide Einheitslänge, senkrecht).
fn frame(origin: DVec3, x: DVec3, z: DVec3) -> Frame {
    Frame {
        origin,
        x,
        y: z.cross(x),
        z,
    }
}

/// Drehfläche (`SURFACE_OF_REVOLUTION`, P8): Kurve `curve` um die Achse durch `origin` mit der
/// Einheitsrichtung `axis`. Gerade parallel → Zylinder, senkrecht → Ebene, die Achse schneidend
/// → Kegel, schräg daneben → Skip; Kreis in einer Ebene durch die Achse → Kugel bzw. Torus;
/// sonst NURBS (`NurbsSurface::revolve` über die ganze Kurve). `flipped` gemessen.
pub fn revolution(
    curve: &Curve,
    origin: DVec3,
    axis: DVec3,
    weld_tol: f64,
) -> Result<SurfaceGeom, Skip> {
    let v = axis;
    let foot = |p: DVec3| origin + v * (p - origin).dot(v);
    let near = |d: f64, p: DVec3| d <= LINEAR_EPS * (1.0 + (p - origin).length());
    let none = || skip("Eine Drehfläche hat keine Ausdehnung.");
    // Probe und Bezugspunkt einer Geraden: ihr Ursprung, liegt er auf der Achse, 1 weiter.
    let line_t = match curve {
        Curve::Line { origin: o, .. } if near((*o - foot(*o)).length(), *o) => 1.0,
        _ => 0.0,
    };
    let surface = match curve {
        Curve::Line { origin: o, dir } => {
            let cross = dir.cross(v);
            let r_o = (*o - foot(*o)).length();
            if cross.length() <= ANGLE_EPS {
                if near(r_o, *o) {
                    return Err(none());
                }
                let f = foot(*o);
                Surface::Cylinder {
                    frame: frame(f, (*o - f) / r_o, v),
                    radius: r_o,
                }
            } else if dir.dot(v).abs() <= ANGLE_EPS {
                let f = foot(*o);
                Surface::Plane {
                    frame: frame(f, v.any_orthonormal_vector(), v),
                }
            } else {
                // Abstand der Geraden von der Achse.
                let n = cross.normalize();
                if !near((*o - origin).dot(n).abs(), *o) {
                    return Err(skip(
                        "Eine Drehfläche aus einer schrägen Geraden wird noch nicht unterstützt.",
                    ));
                }
                // Bezugspunkt R neben der Achse (siehe `line_t`); der Kegel liegt auf seiner
                // Seite der Spitze.
                let r = *o + *dir * line_t;
                let fr = foot(r);
                let rr = (r - fr).length();
                let e = (r - fr) / rr;
                // Radius entlang der Geraden: rr + s · (dir · e); Spitze bei s = −rr / (dir · e).
                let along = dir.dot(v);
                let h_r = (r - origin).dot(v);
                let apex_h = h_r - rr * along / dir.dot(e);
                // Die Achse des Kegels zeigt dorthin, wo der Radius wächst.
                let z = if h_r > apex_h { v } else { -v };
                Surface::Cone {
                    frame: frame(fr, e, z),
                    radius: rr,
                    slope: cross.length() / along.abs(),
                }
            }
        }
        Curve::Circle { frame: cf, radius } => {
            let c = cf.origin;
            let in_axis_plane =
                cf.z.dot(v).abs() <= ANGLE_EPS && near((c - origin).dot(cf.z).abs(), c);
            if in_axis_plane {
                let fc = foot(c);
                let major = (c - fc).length();
                if near(major, c) {
                    Surface::Sphere {
                        frame: frame(fc, v.any_orthonormal_vector(), v),
                        radius: *radius,
                    }
                } else if *radius >= major {
                    return Err(skip(
                        "Eine Torusfläche mit kleinem Radius größer oder gleich dem großen wird noch nicht unterstützt.",
                    ));
                } else {
                    Surface::Torus {
                        frame: frame(fc, (c - fc) / major, v),
                        major,
                        minor: *radius,
                    }
                }
            } else {
                revolve_nurbs(curve, origin, v, weld_tol)?
            }
        }
        _ => revolve_nurbs(curve, origin, v, weld_tol)?,
    };
    let sigma = |t: f64, angle: f64| {
        let p = curve.point(t);
        origin + rotate(p - origin, v, angle)
    };
    let range = match curve {
        Curve::Line { origin: o, .. } => (*o - origin).length().max(1.0),
        _ => curve_range(curve),
    };
    let (p, n) = step_probe(
        sigma,
        &curve_samples(curve, line_t),
        DIFF_STEP * range,
        DIFF_STEP * TAU,
        0.0,
    )
    .ok_or_else(none)?;
    let flipped = measured_flip(&surface, p, n)?;
    Ok(SurfaceGeom::Ready { surface, flipped })
}

/// NURBS-Drehfläche über die ganze Kurve (Kreis und Ellipse als voller Umlauf).
fn revolve_nurbs(
    curve: &Curve,
    origin: DVec3,
    axis: DVec3,
    weld_tol: f64,
) -> Result<Surface, Skip> {
    let why = |m: String| skip(format!("Eine Drehfläche lässt sich nicht übernehmen: {m}"));
    let c = curve_to_nurbs(curve, 0.0, TAU)?;
    let ns = NurbsSurface::revolve(&c, origin, axis, 0.0, TAU).map_err(|e| why(e.to_string()))?;
    let (surface, _) = finish_nurbs_surface(ns, weld_tol).map_err(|Skip(m)| why(m))?;
    Ok(surface)
}

/// Strangfläche (`SURFACE_OF_LINEAR_EXTRUSION`, P8) der Kurve `curve` entlang der
/// Einheitsrichtung `dir`: Gerade → Ebene, Kreis entlang seiner Normale → Zylinder (beide mit
/// gemessenem `flipped`); sonst [`SurfaceGeom::Extrusion`], deren Bereich die Schleifen
/// bestimmen ([`finish_extrusion`]).
pub fn extrusion(curve: &Curve, dir: DVec3) -> Result<SurfaceGeom, Skip> {
    let none = || skip("Eine Strangfläche hat keine Ausdehnung.");
    let surface = match curve {
        Curve::Line { origin, dir: d } => {
            let y = dir - *d * d.dot(dir);
            if y.length() <= ANGLE_EPS {
                return Err(none());
            }
            Surface::Plane {
                frame: frame(*origin, *d, d.cross(y.normalize())),
            }
        }
        Curve::Circle { frame: cf, radius } if cf.z.cross(dir).length() <= ANGLE_EPS => {
            Surface::Cylinder {
                frame: *cf,
                radius: *radius,
            }
        }
        _ => {
            return Ok(SurfaceGeom::Extrusion {
                curve: curve_to_nurbs(curve, 0.0, TAU)?,
                dir,
            });
        }
    };
    let sigma = |t: f64, w: f64| curve.point(t) + dir * w;
    let (p, n) = step_probe(
        sigma,
        &curve_samples(curve, 0.0),
        DIFF_STEP * curve_range(curve),
        DIFF_STEP,
        0.0,
    )
    .ok_or_else(none)?;
    let flipped = measured_flip(&surface, p, n)?;
    Ok(SurfaceGeom::Ready { surface, flipped })
}

/// Verschiebefläche der Kurve von `w0` bis `w1` entlang `d` (Einheit).
fn extrude_between(
    curve: &NurbsCurve,
    d: DVec3,
    w0: f64,
    w1: f64,
    weld_tol: f64,
) -> Result<Surface, Skip> {
    let why = |m: String| {
        skip(format!(
            "Eine Strangfläche lässt sich nicht übernehmen: {m}"
        ))
    };
    let base = curve
        .transformed(|p| p + d * w0)
        .map_err(|e| why(e.to_string()))?;
    let ns = NurbsSurface::extrude(&base, d * (w1 - w0)).map_err(|e| why(e.to_string()))?;
    let (s, _) = finish_nurbs_surface(ns, weld_tol).map_err(|Skip(m)| why(m))?;
    Ok(s)
}

/// Strangfläche über den Bereich der Schleifenpunkte entlang `dir` (Rand 1e-6 relativ).
/// Erst eine sichere Hülle (Schleifen gegen Kontrollpunkte entlang `dir`), dann der genaue
/// Bereich aus den Fußpunkten der Schleifenpunkte. u = Kurvenparameter, v ∈ [0, 1] steigt mit
/// `dir`; geschlossen in u, wenn die Kurve geschlossen ist.
pub fn finish_extrusion(
    curve: &NurbsCurve,
    dir: DVec3,
    loops: &[Vec<DVec3>],
) -> Result<Surface, Skip> {
    finish_extrusion_with_tol(curve, dir, loops, 0.0)
}

/// Wie [`finish_extrusion`]; eine bis `weld_tol` geschlossene Kurve ergibt eine geschlossene
/// Fläche.
// Der negierte Vergleich lehnt absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn finish_extrusion_with_tol(
    curve: &NurbsCurve,
    dir: DVec3,
    loops: &[Vec<DVec3>],
    weld_tol: f64,
) -> Result<Surface, Skip> {
    let d = dir.normalize();
    if !d.is_finite() {
        return Err(skip("Eine Strangfläche hat keine Ausdehnung."));
    }
    let pts: Vec<DVec3> = loops.iter().flatten().copied().collect();
    if pts.is_empty() {
        return Err(skip("Eine Fläche hat keinen Rand."));
    }
    let range = |it: &mut dyn Iterator<Item = f64>| {
        it.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
            (lo.min(x), hi.max(x))
        })
    };
    let (c0, c1) = range(&mut curve.control_points().iter().map(|p| p.dot(d)));
    let (p0, p1) = range(&mut pts.iter().map(|p| p.dot(d)));
    let (w0, w1) = (p0 - c1, p1 - c0);
    let pad = EXTRUSION_MARGIN * (w1 - w0).abs().max(1.0);
    let rough = extrude_between(curve, d, w0 - pad, w1 + pad, weld_tol)?;
    let span = w1 - w0 + 2.0 * pad;
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for p in &pts {
        let (_, v) = rough.project(*p);
        let w = w0 - pad + v * span;
        lo = lo.min(w);
        hi = hi.max(w);
    }
    if !(hi > lo) {
        return Err(skip("Eine Strangfläche hat keine Ausdehnung."));
    }
    let margin = EXTRUSION_MARGIN * (hi - lo);
    extrude_between(curve, d, lo - margin, hi + margin, weld_tol)
}

/// Ob die natürliche Normale einer mit [`finish_extrusion`] gebauten Fläche gegen die Normale
/// von STEP (`λ'(u) × dir`) zeigt; gemessen an einem Probepunkt der Fläche.
pub fn extrusion_flipped(curve: &NurbsCurve, dir: DVec3, surface: &Surface) -> Result<bool, Skip> {
    let (a, b) = curve.domain();
    let h = DIFF_STEP * (b - a);
    let samples: Vec<f64> = (1..10).map(|k| a + (b - a) * k as f64 / 10.0).collect();
    let v_mid = match surface {
        Surface::Nurbs(s) => {
            let (_, (v0, v1)) = s.surface().domain();
            0.5 * (v0 + v1)
        }
        _ => 0.5,
    };
    let best = samples
        .iter()
        .map(|&t| {
            let du = (curve.point(t + h) - curve.point(t - h)) / (2.0 * h);
            (t, du.cross(dir))
        })
        .max_by(|x, y| x.1.length().total_cmp(&y.1.length()));
    let Some((t, n)) = best else {
        return Err(skip(
            "Die Richtung einer Fläche lässt sich nicht bestimmen.",
        ));
    };
    measured_flip(surface, surface.point(t, v_mid), n)
}

/// Eine bis `weld_tol` geschlossene Freiformkurve (Anfang und Ende auseinander, aber nicht
/// weiter als `weld_tol`; die Kurve selbst deutlich länger) mit genau geschlossenen Enden: beide
/// Endpunkte auf ihren Mittelwert. `None`, wenn sie schon geschlossen ist oder es nicht passt.
pub fn close_curve(c: &NurbsCurve, weld_tol: f64) -> Option<NurbsCurve> {
    let pts = c.control_points();
    let (first, last) = (*pts.first()?, *pts.last()?);
    let gap = first.distance(last);
    let extent = pts.iter().map(|p| p.distance(first)).fold(0.0, f64::max);
    if gap <= SAME_ROW || gap > weld_tol || extent <= 10.0 * weld_tol {
        return None;
    }
    let mid = (first + last) * 0.5;
    let mut control = pts.to_vec();
    let n = control.len();
    control[0] = mid;
    control[n - 1] = mid;
    NurbsCurve::new(
        c.degree(),
        c.knots().knots().to_vec(),
        control,
        c.weights().to_vec(),
    )
    .ok()
}
