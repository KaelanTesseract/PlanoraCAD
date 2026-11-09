//! Griffe am Spline durch Stützpunkte (Skizzen 7, Spec B): Rechnung nach B4 und die Befehle.
//!
//! Stützpunkte mit Griff und die Enden teilen den Spline in Strecken. Jede Strecke ist ein
//! C²-kubischer Spline mit Parameterabstand 1; an einem Griff ist die Ableitung vorgegeben, an einem
//! Ende ohne Griff ist er natürlich. Daraus folgen für jeden Knoten Wert, erste und zweite Ableitung,
//! und jedes Segment ist ein quintisches Hermite-Stück durch diese Werte. Damit ist die Kurve überall
//! C², auch an einem Griff, an dem die beiden Strecken verschiedene zweite Ableitungen hätten.
//!
//! Ohne Griffe läuft nichts von hier: `Spline` mit leerem `handles` nimmt den alten Codeweg.

use crate::constraint::{ConstraintKind, SmoothEnd, SmoothStep, smooth_end, smooth_step};
use crate::curves::{Sampled, V2};
use crate::dual::Dual;
use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, HandleCurvature, PointId, Sketch, SplineHandle};
use planora_i18n::tr;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3};
use std::collections::{HashMap, HashSet};

/// Grad der exakten Darstellung eines Splines mit Griffen.
const NURBS_DEGREE: usize = 5;

/// Vielfachheit der inneren Knoten: Die Kurve ist dort C².
const NURBS_INNER_MULTIPLICITY: usize = 3;

/// Größte Öffnung des Griff-Bogens in Radiant (B2).
pub(crate) const HANDLE_MAX_OPENING: f64 = 2.5;

/// Unter diesem `|κ|·L` gilt die Krümmung als fast null: Es gibt keinen endlichen Bogen (B5).
const NEAR_ZERO_CURVATURE: f64 = 1e-3;
/// Bis zu diesem Vielfachen von `tol.linear` gilt die Krümmung eines festen Spline-Endes als null.
const FIXED_SPLINE_FLAT: f64 = 0.1;

/// Startradius bei fast null Krümmung, als Vielfaches der Grifflänge (B5).
const NEAR_ZERO_RADIUS: f64 = 1000.0;

/// Wert, erste und zweite Ableitung nach `t` je Stützpunkt.
#[derive(Clone, Debug, PartialEq)]
pub struct Nodes<T> {
    pub value: Vec<T>,
    pub first: Vec<T>,
    pub second: Vec<T>,
}

/// Vorgabe an einem Griff für [`hermite_nodes`].
#[derive(Clone, Debug)]
pub enum NodeCurvature {
    /// Krümmung aus der Kurve: der Mittelwert beider Seiten.
    Form,
    /// Vorgegebene Krümmung; der Griff bestimmt nur den Anteil quer zur Tangente.
    Value(Dual),
}

/// Griff am Stützpunkt `index`: vorgegebene Ableitung und Krümmung.
#[derive(Clone, Debug)]
pub struct NodeHandle {
    pub index: usize,
    pub derivative: V2,
    pub curvature: NodeCurvature,
}

fn c(v: f64) -> Dual {
    Dual::constant(v)
}

/// Zweite Ableitungen einer Strecke durch `y` (mindestens zwei Werte, Parameterabstand 1). Mit
/// vorgegebener Ableitung `2·M₀ + M₁ = 6·(y₁ − y₀ − D₀)` bzw. `Mₙ₋₁ + 2·Mₙ = 6·(Dₙ − (yₙ − yₙ₋₁))`,
/// ohne `M = 0`. Thomas-Verfahren; die Koeffizienten sind Zahlen, die rechte Seite ist `Dual`.
fn stretch_second(y: &[Dual], start: Option<&Dual>, end: Option<&Dual>) -> Vec<Dual> {
    let n = y.len();
    let last = n - 1;
    let mut upper = vec![0.0; n];
    let mut right: Vec<Dual> = Vec::with_capacity(n);
    for i in 0..n {
        let (l, diag, u, r) = if i == 0 {
            match start {
                Some(d) => (0.0, 2.0, 1.0, 6.0 * (&y[1] - &y[0] - d)),
                None => (0.0, 1.0, 0.0, c(0.0)),
            }
        } else if i == last {
            match end {
                Some(d) => (1.0, 2.0, 0.0, 6.0 * (d - &(&y[last] - &y[last - 1]))),
                None => (0.0, 1.0, 0.0, c(0.0)),
            }
        } else {
            let r = 6.0 * (&y[i - 1] - &(2.0 * &y[i]) + &y[i + 1]);
            (1.0, 4.0, 1.0, r)
        };
        if i == 0 {
            upper[0] = u / diag;
            right.push(r / diag);
        } else {
            let den = diag - l * upper[i - 1];
            upper[i] = u / den;
            let v = (r - &(l * &right[i - 1])) / den;
            right.push(v);
        }
    }
    let mut m = right;
    for i in (0..last).rev() {
        let v = &m[i] - &(upper[i] * &m[i + 1]);
        m[i] = v;
    }
    m
}

/// Eine Koordinate aller Knoten: erste Ableitung und `M̄` (an einem Griff der Mittelwert beider
/// Seiten, an einem Spline-Ende der Wert der einen Seite).
fn axis(y: &[Dual], slope: &[Option<&Dual>]) -> (Vec<Dual>, Vec<Dual>) {
    let n = y.len();
    let mut first = vec![c(0.0); n];
    let mut mean = vec![c(0.0); n];
    let breaks: Vec<usize> = (0..n)
        .filter(|&i| i == 0 || i == n - 1 || slope[i].is_some())
        .collect();
    for w in breaks.windows(2) {
        let (a, b) = (w[0], w[1]);
        let m = stretch_second(&y[a..=b], slope[a], slope[b]);
        for (k, mk) in m.iter().enumerate() {
            let i = a + k;
            mean[i] = if k == 0 && a > 0 {
                // Die linke Strecke hat ihren Wert schon eingetragen.
                0.5 * (&mean[i] + mk)
            } else {
                mk.clone()
            };
            first[i] = match slope[i] {
                Some(d) => d.clone(),
                None if k + 1 < m.len() => &y[i + 1] - &y[i] - (2.0 * mk + &m[k + 1]) / 6.0,
                None => &y[i] - &y[i - 1] + (&m[k - 1] + &(2.0 * mk)) / 6.0,
            };
        }
    }
    (first, mean)
}

/// Knotenwerte nach B4 durch die Stützpunkte `y` (mindestens zwei) mit den Griffen `handles`. Ohne
/// Griffe sind es die des natürlichen Splines. Die Rechnung läuft in `Dual`; Zahlen rechnet
/// [`Nodes::numbers`] daraus, Konstanten kosten keinen Gradienten.
pub fn hermite_nodes(y: &[V2], handles: &[NodeHandle]) -> Nodes<V2> {
    let n = y.len();
    let mut by_node: Vec<Option<&NodeHandle>> = vec![None; n];
    for h in handles {
        by_node[h.index] = Some(h);
    }
    let coordinate = |pick: fn(&V2) -> &Dual| {
        let values: Vec<Dual> = y.iter().map(|p| pick(p).clone()).collect();
        let slope: Vec<Option<&Dual>> = by_node
            .iter()
            .map(|h| h.map(|h| pick(&h.derivative)))
            .collect();
        axis(&values, &slope)
    };
    let (first_x, mean_x) = coordinate(|p| &p.0);
    let (first_y, mean_y) = coordinate(|p| &p.1);
    let first: Vec<V2> = first_x.into_iter().zip(first_y).collect();
    let mut second: Vec<V2> = mean_x.into_iter().zip(mean_y).collect();
    for h in handles {
        if let NodeCurvature::Value(kappa) = &h.curvature {
            second[h.index] = with_curvature(&second[h.index], &h.derivative, kappa);
        }
    }
    Nodes {
        value: y.to_vec(),
        first,
        second,
    }
}

/// `κ·|D|²·N + ((M̄·D)/|D|²)·D` mit der linken Normalen `N`: Der Griff bestimmt nur den Anteil quer
/// zur Tangente. Bei verschwindender Ableitung gibt es keine Richtung, dann bleibt `M̄`.
fn with_curvature(mean: &V2, d: &V2, kappa: &Dual) -> V2 {
    let len2 = &d.0 * &d.0 + &d.1 * &d.1;
    if len2.v <= f64::MIN_POSITIVE {
        return mean.clone();
    }
    let along = (&mean.0 * &d.0 + &mean.1 * &d.1) / &len2;
    let across = kappa * &len2.sqrt();
    (
        &along * &d.0 - &across * &d.1,
        &along * &d.1 + &across * &d.0,
    )
}

impl Nodes<V2> {
    /// Die Werte ohne Gradienten.
    pub(crate) fn numbers(&self) -> Nodes<DVec2> {
        let plain = |v: &[V2]| v.iter().map(|p| DVec2::new(p.0.v, p.1.v)).collect();
        Nodes {
            value: plain(&self.value),
            first: plain(&self.first),
            second: plain(&self.second),
        }
    }
}

/// Segment bei `t` und lokaler Parameter wie bei `spline_eval` (außerhalb erstes bzw. letztes).
fn segment_of(t: f64, n: usize) -> usize {
    // `NaN.max(0.0)` ist 0; zu große Werte sättigen beim Umwandeln und werden begrenzt.
    (t.floor().max(0.0) as usize).min(n - 2)
}

/// Quintische Hermite-Grundfunktionen `[p₀, d₀, a₀, a₁, d₁, p₁]` bei `u`.
fn basis(u: f64) -> [f64; 6] {
    let (u2, u3) = (u * u, u * u * u);
    let (u4, u5) = (u3 * u, u3 * u2);
    [
        1.0 - 10.0 * u3 + 15.0 * u4 - 6.0 * u5,
        u - 6.0 * u3 + 8.0 * u4 - 3.0 * u5,
        0.5 * u2 - 1.5 * u3 + 1.5 * u4 - 0.5 * u5,
        0.5 * u3 - u4 + 0.5 * u5,
        -4.0 * u3 + 7.0 * u4 - 3.0 * u5,
        10.0 * u3 - 15.0 * u4 + 6.0 * u5,
    ]
}

/// Erste Ableitung der Grundfunktionen.
fn basis_first(u: f64) -> [f64; 6] {
    let (u2, u3) = (u * u, u * u * u);
    let u4 = u3 * u;
    [
        -30.0 * u2 + 60.0 * u3 - 30.0 * u4,
        1.0 - 18.0 * u2 + 32.0 * u3 - 15.0 * u4,
        u - 4.5 * u2 + 6.0 * u3 - 2.5 * u4,
        1.5 * u2 - 4.0 * u3 + 2.5 * u4,
        -12.0 * u2 + 28.0 * u3 - 15.0 * u4,
        30.0 * u2 - 60.0 * u3 + 30.0 * u4,
    ]
}

/// Zweite Ableitung der Grundfunktionen.
fn basis_second(u: f64) -> [f64; 6] {
    let (u2, u3) = (u * u, u * u * u);
    [
        -60.0 * u + 180.0 * u2 - 120.0 * u3,
        -36.0 * u + 96.0 * u2 - 60.0 * u3,
        1.0 - 9.0 * u + 18.0 * u2 - 10.0 * u3,
        3.0 * u - 12.0 * u2 + 10.0 * u3,
        -24.0 * u + 84.0 * u2 - 60.0 * u3,
        60.0 * u - 180.0 * u2 + 120.0 * u3,
    ]
}

impl Nodes<DVec2> {
    /// Die sechs Werte des Segments `i` in der Ordnung von [`basis`].
    fn coefficients(&self, i: usize) -> [DVec2; 6] {
        [
            self.value[i],
            self.first[i],
            self.second[i],
            self.second[i + 1],
            self.first[i + 1],
            self.value[i + 1],
        ]
    }

    fn combine(&self, i: usize, b: [f64; 6]) -> DVec2 {
        self.coefficients(i)
            .iter()
            .zip(b)
            .fold(DVec2::ZERO, |sum, (&q, w)| sum + q * w)
    }

    /// Punkt und Ableitung nach `t`; außerhalb wird das erste bzw. letzte Segment fortgesetzt.
    pub(crate) fn eval(&self, t: f64) -> (DVec2, DVec2) {
        let i = segment_of(t, self.value.len());
        let u = t - i as f64;
        (self.combine(i, basis(u)), self.combine(i, basis_first(u)))
    }

    /// Zweite Ableitung nach `t`.
    pub(crate) fn second_at(&self, t: f64) -> DVec2 {
        let i = segment_of(t, self.value.len());
        self.combine(i, basis_second(t - i as f64))
    }

    /// Die sechs Bézier-Punkte des Segments `i`.
    pub(crate) fn bezier(&self, i: usize) -> [DVec2; 6] {
        let (p0, d0, a0) = (self.value[i], self.first[i], self.second[i]);
        let (p1, d1, a1) = (self.value[i + 1], self.first[i + 1], self.second[i + 1]);
        [
            p0,
            p0 + d0 / 5.0,
            p0 + d0 * 0.4 + a0 / 20.0,
            p1 - d1 * 0.4 + a1 / 20.0,
            p1 - d1 / 5.0,
            p1,
        ]
    }

    /// Kontrollpunkte und Knoten als B-Spline Grad 5 mit dreifachen inneren Knoten. Die Kurve ist C²,
    /// darum stimmen die Blüten `f` beider Seiten überein, sobald drei Argumente `k` sind. Mit dem
    /// Abstand 1 gehören zu jedem inneren Knoten `k` genau `f(k−1, k−1, k, k, k)` (der Bézier-Punkt
    /// `b₃` links), `f(k−1, k, k, k, k+1) = 2·b₄ − b₃` und `f(k, k, k, k+1, k+1)` (`b₂` rechts).
    fn b_spline(&self) -> (Vec<f64>, Vec<DVec3>) {
        let n = self.value.len();
        let mut control = Vec::with_capacity(3 * n);
        let first = self.bezier(0);
        control.extend_from_slice(&first[..3]);
        for k in 1..n - 1 {
            let (left, right) = (self.bezier(k - 1), self.bezier(k));
            control.extend([left[3], left[4] * 2.0 - left[3], right[2]]);
        }
        let end = self.bezier(n - 2);
        control.extend_from_slice(&end[3..]);
        let mut knots = vec![0.0; NURBS_DEGREE + 1];
        for k in 1..n - 1 {
            knots.extend(std::iter::repeat_n(k as f64, NURBS_INNER_MULTIPLICITY));
        }
        knots.extend(std::iter::repeat_n((n - 1) as f64, NURBS_DEGREE + 1));
        (knots, control.iter().map(|p| p.extend(0.0)).collect())
    }
}

/// Punkt und Ableitung nach `t` in `Dual` (auch `t` kann Variable sein).
pub(crate) fn hermite_eval_dual(nodes: &Nodes<V2>, t: &Dual) -> (V2, V2) {
    let i = segment_of(t.v, nodes.value.len());
    let u = t - i as f64;
    let u2 = &u * &u;
    let u3 = &u2 * &u;
    let u4 = &u3 * &u;
    let u5 = &u4 * &u;
    let value = [
        1.0 - 10.0 * &u3 + 15.0 * &u4 - 6.0 * &u5,
        &u - &(6.0 * &u3) + 8.0 * &u4 - 3.0 * &u5,
        0.5 * &u2 - 1.5 * &u3 + 1.5 * &u4 - 0.5 * &u5,
        0.5 * &u3 - &u4 + 0.5 * &u5,
        -4.0 * &u3 + 7.0 * &u4 - 3.0 * &u5,
        10.0 * &u3 - 15.0 * &u4 + 6.0 * &u5,
    ];
    let slope = [
        -30.0 * &u2 + 60.0 * &u3 - 30.0 * &u4,
        1.0 - 18.0 * &u2 + 32.0 * &u3 - 15.0 * &u4,
        &u - &(4.5 * &u2) + 6.0 * &u3 - 2.5 * &u4,
        1.5 * &u2 - 4.0 * &u3 + 2.5 * &u4,
        -12.0 * &u2 + 28.0 * &u3 - 15.0 * &u4,
        30.0 * &u2 - 60.0 * &u3 + 30.0 * &u4,
    ];
    let k = [
        &nodes.value[i],
        &nodes.first[i],
        &nodes.second[i],
        &nodes.second[i + 1],
        &nodes.first[i + 1],
        &nodes.value[i + 1],
    ];
    let sum = |w: &[Dual; 6], pick: fn(&V2) -> &Dual| {
        k.iter()
            .zip(w)
            .fold(c(0.0), |acc, (q, b)| acc + b * pick(q))
    };
    (
        (sum(&value, |p| &p.0), sum(&value, |p| &p.1)),
        (sum(&slope, |p| &p.0), sum(&slope, |p| &p.1)),
    )
}

/// Vorgabe zum Griff `h` am Stützpunkt `index` aus den Lagen `at` und `tip`.
fn node_handle(index: usize, at: &V2, tip: &V2, curvature: NodeCurvature) -> NodeHandle {
    NodeHandle {
        index,
        derivative: (3.0 * (&tip.0 - &at.0), 3.0 * (&tip.1 - &at.1)),
        curvature,
    }
}

/// Krümmung des Griffs mit der Bogenmitte `center`: `±1 / r`, positiv, wenn die Mitte links der
/// Laufrichtung `tip − at` liegt (B2). Mitte auf dem Stützpunkt: keine Krümmung; der Löser lässt so
/// einen Bogen nicht zu.
pub(crate) fn handle_kappa(at: &V2, tip: &V2, center: &V2) -> Dual {
    let (fx, fy) = (&tip.0 - &at.0, &tip.1 - &at.1);
    let (mx, my) = (&center.0 - &at.0, &center.1 - &at.1);
    let r = (&mx * &mx + &my * &my).sqrt();
    if r.v <= f64::MIN_POSITIVE {
        return c(0.0);
    }
    let kappa = c(1.0) / &r;
    if fx.v * my.v - fy.v * mx.v < 0.0 {
        -&kappa
    } else {
        kappa
    }
}

/// Knotenwerte eines Splines mit Griffen; `pt` liefert die Lage eines Punktes (im Löser als `Dual`).
pub(crate) fn nodes_of(
    points: &[PointId],
    handles: &[SplineHandle],
    pt: impl Fn(PointId) -> V2,
) -> Nodes<V2> {
    let y: Vec<V2> = points.iter().map(|&p| pt(p)).collect();
    let inputs: Vec<NodeHandle> = handles
        .iter()
        .filter_map(|h| {
            // Einen Griff neben den Stützpunkten lässt das Laden nicht zu.
            let index = points.iter().position(|&p| p == h.at)?;
            let (at, tip) = (&y[index], pt(h.tip));
            let curvature = match h.curvature {
                HandleCurvature::Form => NodeCurvature::Form,
                HandleCurvature::Straight => NodeCurvature::Value(c(0.0)),
                HandleCurvature::Arc { center, .. } => {
                    NodeCurvature::Value(handle_kappa(at, &tip, &pt(center)))
                }
            };
            Some(node_handle(index, at, &tip, curvature))
        })
        .collect();
    hermite_nodes(&y, &inputs)
}

/// Ist `p` das erste Ende des Splines?
fn curve_starts_at(kind: &CurveKind, p: PointId) -> bool {
    matches!(kind, CurveKind::Spline { points, .. } if points.first() == Some(&p))
}

/// Die Kurven eines Griffs: seine Linie und ein Krümmungsbogen.
fn handle_parts(h: &SplineHandle) -> impl Iterator<Item = CurveId> + use<> {
    let arc = match h.curvature {
        HandleCurvature::Arc { arc, .. } => Some(arc),
        _ => None,
    };
    std::iter::once(h.line).chain(arc)
}

/// Startlage des Griff-Bogens am Griff `at → tip` für die Krümmung `kappa`.
struct CurvatureArc {
    center: DVec2,
    other: DVec2,
    /// Liegt die Mitte links der Laufrichtung `tip − at`?
    left: bool,
}

impl CurvatureArc {
    /// `None`, wenn es keinen Griff oder keine Zahl gibt.
    fn new(at: DVec2, tip: DVec2, kappa: f64) -> Option<CurvatureArc> {
        let l = (tip - at).length();
        if kappa.is_finite() && l.is_finite() && kappa.abs() * l < NEAR_ZERO_CURVATURE {
            return CurvatureArc::with_radius(at, tip, NEAR_ZERO_RADIUS * l, true);
        }
        CurvatureArc::exact(at, tip, kappa)
    }

    /// Genau die Krümmung `kappa`, auch wenn der Bogen dadurch sehr groß wird. `None` bei `κ = 0`.
    fn exact(at: DVec2, tip: DVec2, kappa: f64) -> Option<CurvatureArc> {
        if !kappa.is_finite() || kappa == 0.0 {
            return None;
        }
        CurvatureArc::with_radius(at, tip, 1.0 / kappa.abs(), kappa > 0.0)
    }

    fn with_radius(at: DVec2, tip: DVec2, r: f64, left: bool) -> Option<CurvatureArc> {
        let forward = tip - at;
        let l = forward.length();
        if !l.is_finite() || l <= 0.0 {
            return None;
        }
        let side = if left { 1.0 } else { -1.0 };
        let center = at + forward.perp() / l * (side * r);
        // Vom Stützpunkt um die Öffnung weiter in Laufrichtung: Links dreht das gegen den
        // Uhrzeigersinn, rechts mit ihm.
        let turn = DVec2::from_angle(side * (l / r).min(HANDLE_MAX_OPENING));
        let other = center + turn.rotate(at - center);
        Some(CurvatureArc {
            center,
            other,
            left,
        })
    }
}

impl Sketch {
    /// Die abgetastete Form eines Splines: ohne Griffe wie eh und je.
    pub(crate) fn spline_sampled(&self, points: &[PointId], handles: &[SplineHandle]) -> Sampled {
        if handles.is_empty() {
            let xy: Vec<DVec2> = points.iter().map(|&p| self.pos(p)).collect();
            Sampled::spline(&xy)
        } else {
            Sampled::Hermite {
                nodes: nodes_of(points, handles, |p| {
                    let q = self.pos(p);
                    (c(q.x), c(q.y))
                })
                .numbers(),
            }
        }
    }

    /// Schaltet am Stützpunkt `at` des Splines `c` den Tangentengriff ein: eine Konstruktionslinie
    /// von `at` zu einer neuen Spitze `at + C'/3`. Die Form ändert sich nicht. Abgelehnt, wenn die
    /// Kurve dort steht (Ableitung unter der Toleranz), dann bleibt die Skizze unverändert. Ist `c`
    /// Vorlage oder Exemplar eines Musters, wird das Muster aufgelöst.
    pub fn activate_tangent_handle(
        &mut self,
        c: CurveId,
        at: PointId,
    ) -> Result<CurveId, SketchError> {
        self.check_curve(c)?;
        let CurveKind::Spline { points, handles } = &self.curves[c].kind else {
            return Err(SketchError::WrongType(tr!("sketch-err-handle-kind")));
        };
        let Some(index) = points.iter().position(|&p| p == at) else {
            return Err(SketchError::InvalidInput(tr!("sketch-err-handle-point")));
        };
        if handles.iter().any(|h| h.at == at) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-handle-active")));
        }
        let geom = self.geom(c).ok_or(SketchError::UnknownObject)?;
        let reach = geom.eval(index as f64).1 / 3.0;
        if !reach.is_finite() || reach.length() <= self.tol.linear {
            return Err(SketchError::Degenerate(tr!("sketch-err-handle-still")));
        }
        let tip_at = self.pos(at) + reach;
        self.atomically(|s| {
            let tip = s.add_point(tip_at.x, tip_at.y)?;
            let line = s.add_line(at, tip)?;
            s.curves[line].construction = true;
            let mut kind = s.curves[c].kind.clone();
            if let CurveKind::Spline { handles, .. } = &mut kind {
                handles.push(SplineHandle {
                    at,
                    line,
                    tip,
                    curvature: HandleCurvature::Form,
                });
            }
            s.set_curve_kind(c, kind);
            Ok(line)
        })
    }

    /// Schaltet den Griff am Stützpunkt `at` des Splines `c` aus: Linie, Spitze und ein
    /// Krümmungsgriff entfallen, die Kurve springt auf die Form ohne ihn. Abhängigkeiten an der Kurve
    /// folgen beim nächsten Lösen.
    pub fn deactivate_handle(&mut self, c: CurveId, at: PointId) -> Result<(), SketchError> {
        self.check_curve(c)?;
        let CurveKind::Spline { handles, .. } = &self.curves[c].kind else {
            return Err(SketchError::WrongType(tr!("sketch-err-handle-kind")));
        };
        let Some(h) = handles.iter().find(|h| h.at == at) else {
            return Err(SketchError::InvalidInput(tr!("sketch-err-handle-none")));
        };
        let line = h.line;
        self.remove_curve(line)
    }

    /// Schaltet am Stützpunkt `at` des Splines `c` den Krümmungsgriff ein und, falls er fehlt, in
    /// demselben Schritt den Tangentengriff. Der Bogen übernimmt die aktuelle Krümmung, die Form
    /// ändert sich nicht (B5). Wo die Krümmung fast null ist, startet er bei `r = 1000·L` links; nur
    /// dort weicht die Form minimal ab. Liefert den Griff-Bogen. Abgelehnt bleibt die Skizze, wie sie
    /// ist.
    ///
    /// An einem Ende mit Glatt wirkt das wie [`prepare_smooth`](Self::prepare_smooth) (C3, Ausnahme zu
    /// B5): Der Griff übernimmt die Werte des Partners, und die Skizze wird gleich gelöst. Abgelehnt,
    /// wenn der Partner keine Krümmung hat oder die Skizze danach nicht mehr lösbar ist.
    pub fn activate_curvature_handle(
        &mut self,
        c: CurveId,
        at: PointId,
    ) -> Result<CurveId, SketchError> {
        match self.smooth_partner_at(c, at) {
            None => self.activate_curvature_plain(c, at),
            Some((partner, partner_at)) => {
                self.activate_curvature_on_smooth(c, at, partner, partner_at)
            }
        }
    }

    /// Krümmungsgriff an einem Ende mit Glatt, siehe [`activate_curvature_handle`](Self::activate_curvature_handle).
    fn activate_curvature_on_smooth(
        &mut self,
        c: CurveId,
        at: PointId,
        partner: CurveId,
        partner_at: PointId,
    ) -> Result<CurveId, SketchError> {
        self.check_curve(c)?;
        if matches!(
            self.handle_at(c, at).map(|h| h.curvature),
            Ok(HandleCurvature::Arc { .. })
        ) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-handle-active")));
        }
        let flat = || SketchError::InvalidInput(tr!("sketch-err-handle-smooth-flat"));
        let wrong_end = || SketchError::WrongType(tr!("sketch-err-smooth-end"));
        let partner_end = smooth_end(self, partner, partner_at).ok_or_else(wrong_end)?;
        if matches!(
            partner_end,
            SmoothEnd::Line | SmoothEnd::Natural | SmoothEnd::Straight
        ) {
            return Err(flat());
        }
        // Ein Widerspruch an anderer Stelle bleibt, wie er ist: Abgelehnt wird nur, was dieser Befehl
        // schlechter macht.
        let before = self.clone().solve();
        self.atomically(|s| {
            if !s.has_handle_at(c, at) {
                s.activate_tangent_handle(c, at)?;
            }
            if partner_end == SmoothEnd::Form {
                // Beide nur aus der Form: gleichrangig, beide bekommen einen Krümmungsgriff.
                if !s.smooth_mean(c, at, partner, partner_at)? {
                    return Err(flat());
                }
            } else {
                let (into_partner, partner_kappa) = s
                    .smooth_end_values(partner, partner_at)
                    .ok_or_else(wrong_end)?;
                let first = curve_starts_at(&s.curves[c].kind, at);
                let reach = s.aim_tip(c, at, if first { -into_partner } else { into_partner })?;
                if matches!(
                    partner_end,
                    SmoothEnd::ControlSpline | SmoothEnd::FixedSpline
                ) && partner_kappa.abs() * reach < NEAR_ZERO_CURVATURE
                {
                    return Err(flat());
                }
                let kappa = if first { -partner_kappa } else { partner_kappa };
                s.place_curvature_arc(c, at, kappa)?;
            }
            if s.solve().is_worse_than(&before) {
                return Err(SketchError::InvalidInput(tr!(
                    "sketch-err-handle-smooth-unsolved"
                )));
            }
            match s.handle_at(c, at)?.curvature {
                HandleCurvature::Arc { arc, .. } => Ok(arc),
                _ => unreachable!("the curvature handle exists by now"),
            }
        })
    }

    /// Der Krümmungsgriff aus der aktuellen Form, ohne Blick auf ein Glatt.
    fn activate_curvature_plain(
        &mut self,
        c: CurveId,
        at: PointId,
    ) -> Result<CurveId, SketchError> {
        self.check_curve(c)?;
        let CurveKind::Spline { points, handles } = &self.curves[c].kind else {
            return Err(SketchError::WrongType(tr!("sketch-err-handle-kind")));
        };
        let Some(index) = points.iter().position(|&p| p == at) else {
            return Err(SketchError::InvalidInput(tr!("sketch-err-handle-point")));
        };
        let existing = handles.iter().find(|h| h.at == at).map(|h| h.curvature);
        if matches!(existing, Some(HandleCurvature::Arc { .. })) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-handle-active")));
        }
        self.atomically(|s| {
            if existing.is_none() {
                s.activate_tangent_handle(c, at)?;
            }
            let CurveKind::Spline { points, handles } = &s.curves[c].kind else {
                unreachable!("checked above");
            };
            let Some(tip) = handles.iter().find(|h| h.at == at).map(|h| h.tip) else {
                unreachable!("the handle exists by now");
            };
            let nodes = nodes_of(points, handles, |p| {
                let q = s.pos(p);
                (Dual::constant(q.x), Dual::constant(q.y))
            })
            .numbers();
            let (d, a) = (nodes.first[index], nodes.second[index]);
            let (from, to) = (s.pos(at), s.pos(tip));
            let layout = CurvatureArc::new(from, to, d.perp_dot(a) / d.length().powi(3))
                .ok_or_else(|| SketchError::Degenerate(tr!("sketch-err-handle-still")))?;
            let center = s.add_point(layout.center.x, layout.center.y)?;
            let other = s.add_point(layout.other.x, layout.other.y)?;
            // Bögen laufen gegen den Uhrzeigersinn: Links beginnt der Bogen am Stützpunkt, rechts
            // endet er dort. In beiden Fällen liegt er vor dem Griff (B2).
            let arc = if layout.left {
                s.add_arc(center, at, other)?
            } else {
                s.add_arc(center, other, at)?
            };
            s.curves[arc].construction = true;
            let mut kind = s.curves[c].kind.clone();
            if let CurveKind::Spline { handles, .. } = &mut kind
                && let Some(h) = handles.iter_mut().find(|h| h.at == at)
            {
                h.curvature = HandleCurvature::Arc { arc, center, other };
            }
            s.set_curve_kind(c, kind);
            s.push_internal(ConstraintKind::SplineCurvatureHandle(c, at));
            Ok(arc)
        })
    }

    /// Schaltet nur den Krümmungsgriff am Stützpunkt `at` aus; der Tangentengriff bleibt, die Kurve
    /// springt auf die Form ohne ihn (B8).
    pub fn deactivate_curvature_handle(
        &mut self,
        c: CurveId,
        at: PointId,
    ) -> Result<(), SketchError> {
        self.check_curve(c)?;
        let CurveKind::Spline { handles, .. } = &self.curves[c].kind else {
            return Err(SketchError::WrongType(tr!("sketch-err-handle-kind")));
        };
        let arc = handles.iter().find_map(|h| match h.curvature {
            HandleCurvature::Arc { arc, .. } if h.at == at => Some(arc),
            _ => None,
        });
        let Some(arc) = arc else {
            return Err(SketchError::InvalidInput(tr!("sketch-err-handle-none")));
        };
        self.remove_curve(arc)
    }

    /// Bereitet Glatt zwischen dem Ende `pa` von `a` und dem Ende `pb` von `b` vor (Spec C3): Griffe
    /// an oder aus und ihre Lage auf den Werten des Partners, vor dem Lösen. Die Griffzeilen sind
    /// reine Winkel; große Sprünge des Radius erreicht das Lösen von selbst nicht. Die Griffe behalten
    /// ihre Länge, ein neuer also ein Drittel der aktuellen Ableitung. Die Abhängigkeit selbst legt
    /// der Aufrufer an. Abgelehnt mit denselben Gründen wie sie; dann bleibt die Skizze, wie sie ist.
    pub fn prepare_smooth(
        &mut self,
        a: CurveId,
        pa: PointId,
        b: CurveId,
        pb: PointId,
    ) -> Result<(), SketchError> {
        self.check_structure(&ConstraintKind::Smooth(a, pa, b, pb))?;
        // Immer in derselben Folge, damit auch die neuen Ids nicht von der Wahlreihenfolge abhängen.
        let (a, pa, b, pb) = if a <= b {
            (a, pa, b, pb)
        } else {
            (b, pb, a, pa)
        };
        let wrong_end = || SketchError::WrongType(tr!("sketch-err-smooth-end"));
        let (ea, eb) = (
            smooth_end(self, a, pa).ok_or_else(wrong_end)?,
            smooth_end(self, b, pb).ok_or_else(wrong_end)?,
        );
        self.atomically(|s| {
            if smooth_step(ea, eb) == SmoothStep::Mean {
                if !s.smooth_mean(a, pa, b, pb)? {
                    s.straighten_both(a, pa, b, pb)?;
                }
                return Ok(());
            }
            s.smooth_step_at(a, pa, smooth_step(ea, eb), b, pb, eb)?;
            s.smooth_step_at(b, pb, smooth_step(eb, ea), a, pa, ea)
        })
    }

    /// Beide Griffe auf den Mittelwert aus Tangente und Krümmung (C3, gleicher Rang). `false`, wenn er
    /// fast 0 ist: Dann gibt es keinen endlichen Bogen, und nur die Tangenten stehen auf dem Mittel.
    /// Beide Griffe müssen da sein. Das Ergebnis hängt nicht von der Reihenfolge ab: `x − y` und
    /// `y − x` sind genau entgegengesetzt.
    fn smooth_mean(
        &mut self,
        a: CurveId,
        pa: PointId,
        b: CurveId,
        pb: PointId,
    ) -> Result<bool, SketchError> {
        let wrong_end = || SketchError::WrongType(tr!("sketch-err-smooth-end"));
        let (ua, ka) = self.smooth_end_values(a, pa).ok_or_else(wrong_end)?;
        let (ub, kb) = self.smooth_end_values(b, pb).ok_or_else(wrong_end)?;
        let into_a = ua.normalize_or_zero() - ub.normalize_or_zero();
        let mean = 0.5 * (ka - kb);
        let mut reach: f64 = 0.0;
        for (c, at, into) in [(a, pa, into_a), (b, pb, -into_a)] {
            let first = curve_starts_at(&self.curves[c].kind, at);
            reach = reach.max(self.aim_tip(c, at, if first { into } else { -into })?);
        }
        if mean.abs() * reach < NEAR_ZERO_CURVATURE {
            return Ok(false);
        }
        for (c, at, kappa) in [(a, pa, mean), (b, pb, -mean)] {
            let first = curve_starts_at(&self.curves[c].kind, at);
            self.place_curvature_arc(c, at, if first { kappa } else { -kappa })?;
        }
        Ok(true)
    }

    /// Der Mittelwert ist fast 0: Beide Griffe werden `Straight`.
    fn straighten_both(
        &mut self,
        a: CurveId,
        pa: PointId,
        b: CurveId,
        pb: PointId,
    ) -> Result<(), SketchError> {
        self.straighten_handle(a, pa)?;
        self.straighten_handle(b, pb)
    }

    /// Führt den Schritt `step` für das Ende `at` der Seite `c` gegen den Partner aus.
    fn smooth_step_at(
        &mut self,
        c: CurveId,
        at: PointId,
        step: SmoothStep,
        partner: CurveId,
        partner_at: PointId,
        partner_end: SmoothEnd,
    ) -> Result<(), SketchError> {
        if step == SmoothStep::Keep {
            return Ok(());
        }
        // Vor jeder Änderung an `c`: Die Werte des Partners hängen nicht von ihr ab, ein neuer Griff
        // braucht aber die alte Form.
        let wrong_end = || SketchError::WrongType(tr!("sketch-err-smooth-end"));
        let (into_partner, partner_kappa) = self
            .smooth_end_values(partner, partner_at)
            .ok_or_else(wrong_end)?;
        if !self.has_handle_at(c, at) {
            self.activate_tangent_handle(c, at)?;
        }
        let first = curve_starts_at(&self.curves[c].kind, at);
        // In `t`-Richtung: Am Anfang läuft sie in die Kurve, am Ende aus ihr heraus. Der Partner liegt
        // gegenüber, also dreht sich beides um.
        let reach = self.aim_tip(c, at, if first { -into_partner } else { into_partner })?;
        let kappa = if first { -partner_kappa } else { partner_kappa };
        // Ein fast gerader Kontrollpunkt-Spline: Es gibt keinen endlichen Bogen (B5), er passt sich
        // über die Krümmungszeile auf 0 an. Der feste Spline passt sich nie an: `Straight` nur, wenn seine
        // Krümmung die Zeile schon erfüllt, sonst ein sehr großer Bogen mit genau seinem Wert.
        let flat = match partner_end {
            SmoothEnd::ControlSpline => kappa.abs() * reach < NEAR_ZERO_CURVATURE,
            SmoothEnd::FixedSpline => kappa.abs() <= FIXED_SPLINE_FLAT * self.tol.linear,
            _ => false,
        };
        if step == SmoothStep::Straighten || flat {
            self.straighten_handle(c, at)
        } else {
            self.place_curvature_arc(c, at, kappa)
        }
    }

    /// Gibt es am Stützpunkt `at` des Splines `c` einen Griff?
    fn has_handle_at(&self, c: CurveId, at: PointId) -> bool {
        matches!(
            &self.curves[c].kind,
            CurveKind::Spline { handles, .. } if handles.iter().any(|h| h.at == at)
        )
    }

    /// Der Griff am Stützpunkt `at`.
    fn handle_at(&self, c: CurveId, at: PointId) -> Result<SplineHandle, SketchError> {
        let CurveKind::Spline { handles, .. } = &self.curves[c].kind else {
            return Err(SketchError::WrongType(tr!("sketch-err-handle-kind")));
        };
        handles
            .iter()
            .find(|h| h.at == at)
            .copied()
            .ok_or_else(|| SketchError::InvalidInput(tr!("sketch-err-handle-none")))
    }

    /// Richtet die Griffspitze auf `forward` aus und behält die Länge. Liefert die Grifflänge. Ohne
    /// Richtung (entartete Partnerkurve) bleibt die Spitze; das Lösen entscheidet dann.
    fn aim_tip(&mut self, c: CurveId, at: PointId, forward: DVec2) -> Result<f64, SketchError> {
        let tip = self.handle_at(c, at)?.tip;
        let from = self.pos(at);
        let reach = self.pos(tip).distance(from);
        if let Some(unit) = forward.try_normalize() {
            let to = from + unit * reach;
            self.set_point_xy(tip, to.x, to.y)?;
        }
        Ok(reach)
    }

    /// Griff am Stützpunkt `at` auf `Straight`; ein Krümmungsbogen entfällt samt Zeile.
    fn straighten_handle(&mut self, c: CurveId, at: PointId) -> Result<(), SketchError> {
        if matches!(
            self.handle_at(c, at)?.curvature,
            HandleCurvature::Arc { .. }
        ) {
            self.deactivate_curvature_handle(c, at)?;
        }
        let mut kind = self.curves[c].kind.clone();
        if let CurveKind::Spline { handles, .. } = &mut kind
            && let Some(h) = handles.iter_mut().find(|h| h.at == at)
        {
            h.curvature = HandleCurvature::Straight;
        }
        self.set_curve_kind(c, kind);
        Ok(())
    }

    /// Legt den Krümmungsgriff am Stützpunkt `at` auf die Krümmung `kappa` (in `t`-Richtung), ohne den
    /// Fast-null-Ersatz aus B5. Ein vorhandener Bogen bleibt mit seinen Abhängigkeiten; liegt er auf
    /// der anderen Seite, tauschen Start und Ende, denn Bögen laufen gegen den Uhrzeigersinn.
    fn place_curvature_arc(
        &mut self,
        c: CurveId,
        at: PointId,
        kappa: f64,
    ) -> Result<(), SketchError> {
        let tip = self.handle_at(c, at)?.tip;
        let layout = CurvatureArc::exact(self.pos(at), self.pos(tip), kappa)
            .ok_or_else(|| SketchError::Degenerate(tr!("sketch-err-handle-still")))?;
        if !matches!(
            self.handle_at(c, at)?.curvature,
            HandleCurvature::Arc { .. }
        ) {
            self.activate_curvature_plain(c, at)?;
        }
        let HandleCurvature::Arc { arc, center, other } = self.handle_at(c, at)?.curvature else {
            unreachable!("the curvature handle exists by now");
        };
        self.set_point_xy(center, layout.center.x, layout.center.y)?;
        self.set_point_xy(other, layout.other.x, layout.other.y)?;
        let (start, end) = if layout.left {
            (at, other)
        } else {
            (other, at)
        };
        self.set_curve_kind(arc, CurveKind::Arc { center, start, end });
        Ok(())
    }

    /// Der Spline als exakte NURBS-Kurve: Grad 5, innere Knoten dreifach, Parameter wie
    /// [`Sketch::curve_eval`]. Für Profile eines Splines mit Griffen.
    pub fn handled_spline_nurbs(&self, c: CurveId) -> Result<NurbsCurve, SketchError> {
        self.check_curve(c)?;
        let CurveKind::Spline { points, handles } = &self.curves[c].kind else {
            return Err(SketchError::WrongType(tr!("sketch-err-handle-kind")));
        };
        let tol = self.tol.linear;
        // `add_spline` prüft das beim Anlegen; das Lösen kann Punkte danach zusammenlegen.
        if points
            .windows(2)
            .any(|w| self.pos(w[0]).distance(self.pos(w[1])) <= tol)
        {
            return Err(SketchError::Degenerate(tr!("sketch-err-spline-neighbours")));
        }
        let nodes = nodes_of(points, handles, |p| {
            let q = self.pos(p);
            (Dual::constant(q.x), Dual::constant(q.y))
        })
        .numbers();
        let (knots, control) = nodes.b_spline();
        let weights = vec![1.0; control.len()];
        NurbsCurve::new(NURBS_DEGREE, knots, control, weights).map_err(|e| {
            SketchError::Degenerate(tr!("sketch-err-spline-invalid", detail = e.to_string()))
        })
    }

    /// Hat die Kurve `c` Griffe?
    pub(crate) fn has_handles(&self, c: CurveId) -> bool {
        matches!(
            self.curve(c).map(|d| &d.kind),
            Some(CurveKind::Spline { handles, .. }) if !handles.is_empty()
        )
    }

    /// Ist `c` die Linie oder der Bogen eines Griffs?
    pub fn is_handle_curve(&self, c: CurveId) -> bool {
        self.handled_splines().any(|(_, handles)| {
            handles.iter().any(|h| {
                h.line == c || matches!(h.curvature, HandleCurvature::Arc { arc, .. } if arc == c)
            })
        })
    }

    /// Wo die gedämpften Tangentengriffe des Splines `c` hinzeigen (B6): je Stützpunkt ohne Griff die
    /// Spitze, die [`activate_tangent_handle`](Self::activate_tangent_handle) anlegen würde. Ein Punkt,
    /// an dem die Kurve steht, fehlt, denn dort lehnte sie ab. Leer für andere Kurven.
    pub fn handle_ghost_tips(&self, c: CurveId) -> Vec<(PointId, DVec2)> {
        let Some(CurveKind::Spline { points, handles }) = self.curve(c).map(|d| &d.kind) else {
            return Vec::new();
        };
        let Some(geom) = self.geom(c) else {
            return Vec::new();
        };
        points
            .iter()
            .enumerate()
            .filter(|(_, p)| handles.iter().all(|h| h.at != **p))
            .filter_map(|(index, &p)| {
                let reach = geom.eval(index as f64).1 / 3.0;
                (reach.is_finite() && reach.length() > self.tol.linear)
                    .then(|| (p, self.pos(p) + reach))
            })
            .collect()
    }

    /// Die gedämpften Marken der Krümmungsgriffe des Splines `c` (B6): an jedem Griff ohne Bogen ein
    /// Punkt auf der Normalen im Abstand der Grifflänge, auf der Seite, auf die sich die Kurve dort
    /// krümmt (bei fast null Krümmung links, wie der Bogen selbst, B5).
    pub fn handle_curvature_marks(&self, c: CurveId) -> Vec<(PointId, DVec2)> {
        let Some(CurveKind::Spline { points, handles }) = self.curve(c).map(|d| &d.kind) else {
            return Vec::new();
        };
        let nodes = nodes_of(points, handles, |p| {
            let q = self.pos(p);
            (Dual::constant(q.x), Dual::constant(q.y))
        })
        .numbers();
        handles
            .iter()
            .filter(|h| !matches!(h.curvature, HandleCurvature::Arc { .. }))
            .filter_map(|h| {
                let index = points.iter().position(|&p| p == h.at)?;
                let (from, to) = (self.pos(h.at), self.pos(h.tip));
                let (d, a) = (nodes.first[index], nodes.second[index]);
                let layout = CurvatureArc::new(from, to, d.perp_dot(a) / d.length().powi(3))?;
                let side = if layout.left { 1.0 } else { -1.0 };
                Some((h.at, from + (to - from).perp() * side))
            })
            .collect()
    }

    /// Spline und Griff, zu denen die Griff-Linie oder der Griff-Bogen `c` gehört.
    pub fn handle_of_curve(&self, c: CurveId) -> Option<(CurveId, SplineHandle)> {
        self.handled_splines().find_map(|(spline, handles)| {
            let h = handles.iter().find(|h| handle_parts(h).any(|k| k == c))?;
            Some((spline, *h))
        })
    }

    /// Spline und Griff, zu denen die Spitze, die Bogenmitte oder das freie Bogenende `p` gehört.
    pub fn handle_of_point(&self, p: PointId) -> Option<(CurveId, SplineHandle)> {
        self.handled_splines().find_map(|(spline, handles)| {
            let h = handles.iter().find(|h| {
                h.tip == p
                    || matches!(h.curvature, HandleCurvature::Arc { center, other, .. }
                        if center == p || other == p)
            })?;
            Some((spline, *h))
        })
    }

    /// Die Griffe am Stützpunkt `at`, an allen Splines, die dort liegen.
    pub fn handles_at(&self, at: PointId) -> Vec<(CurveId, SplineHandle)> {
        self.handled_splines()
            .flat_map(|(spline, handles)| {
                handles
                    .iter()
                    .filter(move |h| h.at == at)
                    .map(move |h| (spline, *h))
            })
            .collect()
    }

    /// Die Punkte, die nur Griffe benutzen (Spitze, Mitte, freies Ende). Benutzt eine andere Kurve
    /// einen, gilt er als gewöhnlicher Punkt.
    pub fn handle_only_points(&self) -> HashSet<PointId> {
        crate::snap::only_handle_points(self, &self.handle_curves())
    }

    /// Lebende Splines mit Griffen.
    fn handled_splines(&self) -> impl Iterator<Item = (CurveId, &[SplineHandle])> {
        self.curves().filter_map(|(id, d)| match &d.kind {
            CurveKind::Spline { handles, .. } if !handles.is_empty() => {
                Some((id, handles.as_slice()))
            }
            _ => None,
        })
    }

    /// Alle Griff-Linien und Griff-Bögen. Sie schneiden nichts, und der Fang kennt sie nicht als Kurven.
    pub(crate) fn handle_curves(&self) -> HashSet<CurveId> {
        self.handled_splines()
            .flat_map(|(_, handles)| handles.iter().flat_map(handle_parts))
            .collect()
    }

    /// `curves` mit allem, was als Griff oder Spline dazugehört: Kopien nehmen den Spline immer mit
    /// seinen Griffen. Ein Griff steht für seinen Spline, die Griffe folgen hinten, in der Reihenfolge
    /// der Splines; die Liste hängt also nicht davon ab, welchen Teil jemand gewählt hat. Ohne Griffe
    /// genau `curves`.
    pub(crate) fn with_handle_family(&self, curves: &[CurveId]) -> Vec<CurveId> {
        if self.handle_family(curves).is_empty() {
            return curves.to_vec();
        }
        let mut out: Vec<CurveId> = Vec::with_capacity(curves.len());
        for &c in curves {
            let owner = self.handled_splines().find_map(|(spline, handles)| {
                handles
                    .iter()
                    .any(|h| handle_parts(h).any(|part| part == c))
                    .then_some(spline)
            });
            let c = owner.unwrap_or(c);
            if !out.contains(&c) {
                out.push(c);
            }
        }
        for spline in out.clone() {
            // `curves` kommt von außen und kann eine Kennung enthalten, die es nicht gibt.
            if let Some(CurveKind::Spline { handles, .. }) = self.curve(spline).map(|d| &d.kind) {
                out.extend(handles.iter().flat_map(handle_parts));
            }
        }
        out
    }

    /// Die Vorlagen ohne die Griff-Kurven ihrer Splines: Das Muster führt sie als Vorlagen, damit sie
    /// Teil jedes Exemplars sind; der Nutzer wählt nur den Spline.
    pub(crate) fn without_handle_parts(&self, sources: &[CurveId]) -> Vec<CurveId> {
        let parts: Vec<CurveId> = sources
            .iter()
            .filter_map(|&c| match self.curve(c).map(|d| &d.kind) {
                Some(CurveKind::Spline { handles, .. }) => {
                    Some(handles.iter().flat_map(handle_parts))
                }
                _ => None,
            })
            .flatten()
            .collect();
        sources
            .iter()
            .copied()
            .filter(|c| !parts.contains(c))
            .collect()
    }

    /// Gibt der Kopie `copy` des Splines `source` dessen Griffe auf den abgebildeten Punkten und
    /// Kurven, je Krümmungsgriff mit eigener interner Zeile. Anderes als ein Spline bleibt, wie es ist.
    /// Fehlt ein Teil in den Abbildungen (nur aus einer fremden Datei möglich, deren Muster den Griff
    /// nicht als Vorlage führt), bleibt die Kopie an dieser Stelle ohne Griff.
    pub(crate) fn carry_handles(
        &mut self,
        source: CurveId,
        copy: CurveId,
        points: &HashMap<PointId, PointId>,
        curves: &HashMap<CurveId, CurveId>,
    ) {
        let CurveKind::Spline { handles, .. } = &self.curves[source].kind else {
            return;
        };
        if handles.is_empty() {
            return;
        }
        let map_handle = |h: &SplineHandle| {
            Some(SplineHandle {
                at: *points.get(&h.at)?,
                line: *curves.get(&h.line)?,
                tip: *points.get(&h.tip)?,
                curvature: match h.curvature {
                    HandleCurvature::Arc { arc, center, other } => HandleCurvature::Arc {
                        arc: *curves.get(&arc)?,
                        center: *points.get(&center)?,
                        other: *points.get(&other)?,
                    },
                    other => other,
                },
            })
        };
        let mapped: Vec<SplineHandle> = handles.iter().filter_map(map_handle).collect();
        let rows: Vec<PointId> = mapped
            .iter()
            .filter(|h| matches!(h.curvature, HandleCurvature::Arc { .. }))
            .map(|h| h.at)
            .collect();
        let mut kind = self.curves[copy].kind.clone();
        if let CurveKind::Spline { handles, .. } = &mut kind {
            *handles = mapped;
        }
        self.set_curve_kind(copy, kind);
        for at in rows {
            self.push_internal(ConstraintKind::SplineCurvatureHandle(copy, at));
        }
    }

    /// Verteilt beim Stutzen und Brechen die Griffe des Splines `c` auf seine Stücke `pieces`: Jeder
    /// Griff geht mit seinem Stützpunkt, ein Griff an einem weggefallenen Punkt entfällt samt Linie,
    /// Spitze, Bogen und interner Zeile. `pieces` sind noch nicht angelegt.
    pub(crate) fn share_handles(
        &mut self,
        c: CurveId,
        pieces: &mut [CurveKind],
    ) -> Result<(), SketchError> {
        let CurveKind::Spline { handles, .. } = &self.curves[c].kind else {
            return Ok(());
        };
        let mut gone = Vec::new();
        for h in handles {
            let home = pieces.iter_mut().find_map(|piece| match piece {
                CurveKind::Spline { points, handles } if points.contains(&h.at) => Some(handles),
                _ => None,
            });
            match home {
                Some(list) => list.push(*h),
                None => gone.push(h.line),
            }
        }
        for line in gone {
            self.remove_curve(line)?;
        }
        Ok(())
    }

    /// Die Krümmungsgriffe der Splines unter `curves`: Bogen, Mitte und freies Ende.
    pub(crate) fn handle_arcs(&self, curves: &[CurveId]) -> Vec<(CurveId, PointId, PointId)> {
        let mut out = Vec::new();
        for &c in curves {
            if let Some(CurveKind::Spline { handles, .. }) = self.curve(c).map(|d| &d.kind) {
                out.extend(handles.iter().filter_map(|h| match h.curvature {
                    HandleCurvature::Arc { arc, center, other } => Some((arc, center, other)),
                    _ => None,
                }));
            }
        }
        out
    }

    /// Die internen Zeilen der Krümmungsgriffe des Stücks `to` hingen bisher am Spline `from`.
    pub(crate) fn move_handle_rows(&mut self, from: CurveId, to: CurveId) {
        let CurveKind::Spline { handles, .. } = &self.curves[to].kind else {
            return;
        };
        let moved: Vec<PointId> = handles
            .iter()
            .filter(|h| matches!(h.curvature, HandleCurvature::Arc { .. }))
            .map(|h| h.at)
            .collect();
        if moved.is_empty() {
            return;
        }
        let rows: Vec<_> = self
            .constraints
            .iter()
            .filter_map(|(id, k)| match k.kind {
                ConstraintKind::SplineCurvatureHandle(spline, at)
                    if k.alive && spline == from && moved.contains(&at) =>
                {
                    Some((id, at))
                }
                _ => None,
            })
            .collect();
        for (id, at) in rows {
            self.constraints[id].kind = ConstraintKind::SplineCurvatureHandle(to, at);
        }
        self.touch_structure();
    }

    /// Die Griff-Teile und Splines, die mit den Kurven `curves` gehen müssen: Griffe eines Splines und
    /// der Spline eines Griffs. So bleibt beim Verschieben ein Griff am Stützpunkt.
    pub(crate) fn handle_family(&self, curves: &[CurveId]) -> Vec<CurveId> {
        let mut out = Vec::new();
        for (spline, handles) in self.handled_splines() {
            let parts: Vec<CurveId> = handles.iter().flat_map(handle_parts).collect();
            if curves.contains(&spline) || parts.iter().any(|p| curves.contains(p)) {
                out.push(spline);
                out.extend(parts);
            }
        }
        out
    }

    /// Nimmt vor dem Löschen der Kurve `c` den Griff aus dem Modell, zu dem sie gehört: am Spline alle
    /// seine Griffe, an der Griff-Linie den ganzen Griff, am Griff-Bogen nur den Krümmungsgriff.
    /// Liefert, was danach mitgeht.
    pub(crate) fn release_handles_of(&mut self, c: CurveId) -> ReleasedHandle {
        let mut out = ReleasedHandle::default();
        let mut own = |h: &SplineHandle, whole: bool| {
            if whole {
                out.curves.push(h.line);
                out.points.push(h.tip);
            }
            if let HandleCurvature::Arc { arc, center, other } = h.curvature {
                out.curves.push(arc);
                out.points.extend([center, other]);
            }
        };
        if let CurveKind::Spline { handles, .. } = &mut self.curves[c].kind {
            // Der Spline wird gelöscht und behält keine Griffe im gelöschten Eintrag.
            for h in std::mem::take(handles) {
                own(&h, true);
            }
            return out;
        }
        let found = self.handled_splines().find_map(|(spline, handles)| {
            handles
                .iter()
                .position(|h| h.line == c)
                .map(|i| (spline, i, true))
                .or_else(|| {
                    handles
                        .iter()
                        .position(
                            |h| matches!(h.curvature, HandleCurvature::Arc { arc, .. } if arc == c),
                        )
                        .map(|i| (spline, i, false))
                })
        });
        let Some((spline, i, whole)) = found else {
            return out;
        };
        let mut kind = self.curves[spline].kind.clone();
        if let CurveKind::Spline { handles, .. } = &mut kind {
            own(&handles[i], whole);
            // Die Zeilen des Krümmungsgriffs hängen am Spline und gingen sonst nicht mit dem Bogen.
            let row = ConstraintKind::SplineCurvatureHandle(spline, handles[i].at);
            let dead: Vec<_> = self
                .constraints
                .iter()
                .filter(|(_, k)| k.alive && k.kind == row)
                .map(|(id, _)| id)
                .collect();
            for id in dead {
                self.constraints[id].alive = false;
            }
            if whole {
                handles.remove(i);
            } else {
                handles[i].curvature = HandleCurvature::Form;
            }
        }
        self.set_curve_kind(spline, kind);
        out.spline = Some(spline);
        // Die Kurve selbst löscht der Aufrufer.
        out.curves.retain(|&k| k != c);
        out
    }

    /// Löscht, was [`release_handles_of`](Self::release_handles_of) übrig ließ. Punkte, die eine andere
    /// Kurve noch benutzt, bleiben.
    pub(crate) fn finish_release(&mut self, released: ReleasedHandle) -> Result<(), SketchError> {
        for k in released.curves {
            if self.check_curve(k).is_ok() {
                self.remove_curve(k)?;
            }
        }
        self.drop_orphans(&released.points)?;
        if let Some(spline) = released.spline {
            self.reseed_extras(spline);
        }
        Ok(())
    }

    /// Griff-Teile zum Punkt `p`: die Griff-Linie an einer Spitze, der Bogen an einem Punkt des
    /// Krümmungsgriffs. Sie gehen statt des ganzen Splines. Der Stützpunkt zählt hier nicht.
    pub(crate) fn handle_parts_at(&self, p: PointId) -> Vec<CurveId> {
        let mut out = Vec::new();
        for (_, handles) in self.handled_splines() {
            for h in handles {
                if h.tip == p {
                    out.push(h.line);
                } else if let HandleCurvature::Arc { arc, center, other } = h.curvature
                    && (center == p || other == p)
                {
                    out.push(arc);
                }
            }
        }
        out
    }
}

/// Was nach dem Austragen eines Griffs mit gelöscht wird, siehe
/// [`Sketch::release_handles_of`].
#[derive(Default)]
pub(crate) struct ReleasedHandle {
    curves: Vec<CurveId>,
    points: Vec<PointId>,
    /// Der Spline, der weiterlebt und seine Form geändert hat.
    spline: Option<CurveId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> V2 {
        (c(x), c(y))
    }

    fn close(a: &Dual, b: f64) -> bool {
        (a.v - b).abs() < 1e-12
    }

    #[test]
    fn stretch_with_both_ends_given_has_one_segment() {
        // Ein Stück, beide Enden vorgegeben: y = t³ hat D₀ = 0, D₁ = 3, M = 0 und 6.
        let m = stretch_second(&[c(0.0), c(1.0)], Some(&c(0.0)), Some(&c(3.0)));
        assert!(close(&m[0], 0.0) && close(&m[1], 6.0), "{m:?}");
        // Natürlich am Anfang, vorgegeben am Ende: M₀ = 0, 2·M₁ = 6·(D − Δy).
        let m = stretch_second(&[c(0.0), c(1.0)], None, Some(&c(2.0)));
        assert!(close(&m[0], 0.0) && close(&m[1], 3.0), "{m:?}");
    }

    #[test]
    fn nodes_without_handles_are_the_natural_spline() {
        let y = [v(0.0, 0.0), v(10.0, 6.0), v(21.0, -3.0), v(30.0, 8.0)];
        let nodes = hermite_nodes(&y, &[]);
        let xs: Vec<Dual> = y.iter().map(|p| p.0.clone()).collect();
        let m = crate::curves::spline_second_derivatives(&xs);
        for (i, mi) in m.iter().enumerate() {
            assert!((nodes.second[i].0.v - mi.v).abs() < 1e-12);
        }
    }

    #[test]
    fn gradient_matches_differences() {
        // Griff am mittleren Punkt, mit Krümmungsvorgabe: Gradient gegen Differenzen.
        let n = 8;
        let eval = |x: &[f64], t: f64| {
            let var = |i: usize| Dual::variable(x[i], i, n);
            let y = [(var(0), var(1)), (var(2), var(3)), (var(4), var(5))];
            let handle = NodeHandle {
                index: 1,
                derivative: (var(6), var(7)),
                curvature: NodeCurvature::Value(c(0.05)),
            };
            hermite_eval_dual(&hermite_nodes(&y, &[handle]), &c(t))
        };
        let x = [0.0, 0.0, 10.0, 6.0, 21.0, -3.0, 9.0, 1.0];
        let h = 1e-6;
        for t in [0.3, 1.0, 1.7] {
            let (q, d) = eval(&x, t);
            for i in 0..n {
                let (mut up, mut down) = (x, x);
                up[i] += h;
                down[i] -= h;
                let (qu, du) = eval(&up, t);
                let (qd, dd) = eval(&down, t);
                let fd = |a: f64, b: f64| (a - b) / (2.0 * h);
                assert!((fd(qu.0.v, qd.0.v) - q.0.grad(i)).abs() < 1e-7);
                assert!((fd(qu.1.v, qd.1.v) - q.1.grad(i)).abs() < 1e-7);
                assert!((fd(du.0.v, dd.0.v) - d.0.grad(i)).abs() < 1e-7);
                assert!((fd(du.1.v, dd.1.v) - d.1.grad(i)).abs() < 1e-7);
            }
        }
    }

    #[test]
    fn dual_and_numbers_agree_also_with_straight_and_zero_derivative() {
        let y = [v(0.0, 0.0), v(10.0, 6.0), v(21.0, -3.0)];
        for d in [v(9.0, 1.0), v(0.0, 0.0)] {
            let handle = NodeHandle {
                index: 1,
                derivative: d,
                curvature: NodeCurvature::Value(c(0.0)),
            };
            let dual = hermite_nodes(&y, &[handle]);
            let plain = dual.numbers();
            if d_is_zero(&dual.first[1]) {
                // Keine Richtung: bleibt M̄ und endlich.
                assert!(plain.second[1].is_finite());
            } else {
                // Krümmung 0: Die zweite Ableitung liegt auf der Tangente.
                assert!(plain.second[1].perp_dot(plain.first[1]).abs() < 1e-12);
            }
            for k in -2..=42 {
                let t = k as f64 / 20.0;
                let (p, dp) = plain.eval(t);
                let (q, dq) = hermite_eval_dual(&dual, &c(t));
                assert!((p.x - q.0.v).abs() < 1e-11 && (p.y - q.1.v).abs() < 1e-11);
                assert!((dp.x - dq.0.v).abs() < 1e-11 && (dp.y - dq.1.v).abs() < 1e-11);
            }
        }
    }

    #[test]
    fn unknown_ids_do_not_panic_in_the_handle_lists() {
        let mut s = Sketch::new(
            planora_math::Frame::WORLD,
            "S",
            planora_math::Tolerance::default(),
        );
        let pts: Vec<PointId> = [(0.0, 0.0), (10.0, 6.0), (21.0, -3.0)]
            .iter()
            .map(|&(x, y)| s.add_point(x, y).unwrap())
            .collect();
        let spline = s.add_spline(&pts).unwrap();
        s.activate_tangent_handle(spline, pts[1]).unwrap();
        let gone = CurveId::from_index(100_000);
        // Die Liste nennt den Griff des Splines und die fremde Kennung, die bleibt, wie sie ist.
        let family = s.with_handle_family(&[spline, gone]);
        assert!(
            family.contains(&gone) && family.contains(&spline),
            "{family:?}"
        );
        assert_eq!(s.without_handle_parts(&[gone]), vec![gone]);
        assert!(s.handle_arcs(&[gone]).is_empty());
        assert!(!s.has_handles(gone));
        assert!(s.handle_ghost_tips(gone).is_empty());
        assert!(s.handle_curvature_marks(gone).is_empty());
    }

    fn d_is_zero(d: &V2) -> bool {
        d.0.v == 0.0 && d.1.v == 0.0
    }

    #[test]
    fn basis_functions_are_consistent() {
        // An den Enden die Einheitswerte, dazwischen die Ableitungen gegen Differenzen.
        for (u, want) in [
            (
                0.0,
                (
                    [1., 0., 0., 0., 0., 0.],
                    [0., 1., 0., 0., 0., 0.],
                    [0., 0., 1., 0., 0., 0.],
                ),
            ),
            (
                1.0,
                (
                    [0., 0., 0., 0., 0., 1.],
                    [0., 0., 0., 0., 1., 0.],
                    [0., 0., 0., 1., 0., 0.],
                ),
            ),
        ] {
            assert_eq!(basis(u), want.0);
            assert_eq!(basis_first(u), want.1);
            assert_eq!(basis_second(u), want.2);
        }
        let h = 1e-6;
        for u in [0.2, 0.5, 0.9, 1.3] {
            for k in 0..6 {
                let d = (basis(u + h)[k] - basis(u - h)[k]) / (2.0 * h);
                assert!((d - basis_first(u)[k]).abs() < 1e-8);
                let d = (basis_first(u + h)[k] - basis_first(u - h)[k]) / (2.0 * h);
                assert!((d - basis_second(u)[k]).abs() < 1e-8);
            }
        }
    }
}
