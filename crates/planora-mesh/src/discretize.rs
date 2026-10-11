use planora_kernel::{Curve, Edge, Id, Solid, Surface, UvRect, nurbs_divisions};
use planora_math::DVec3;
use planora_math::nurbs::{NurbsSurface, polygon_turning};
use std::f64::consts::{PI, TAU};

/// Höchstzahl der Abschnitte einer Freiformkante bzw. je Richtung einer Freiformfläche.
pub const MAX_NURBS_DIVISIONS: usize = 512;

/// Höchstzahl der Verfeinerungsrunden nach der Sehnenprüfung (Freiformkanten und -flächen).
const MAX_REFINE_ROUNDS: usize = 3;

/// Zuschlag beim Verfeinern: Die Sehnenabweichung fällt quadratisch mit der Schrittweite, der
/// neue Schritt wird mit diesem Faktor etwas kleiner gewählt, als die Schätzung verlangt.
const REFINE_MARGIN: f64 = 1.2;

/// Neue Abschnittszahl, wenn die größte gemessene Sehnenabweichung `dev` über `tol` liegt.
fn refined(n: usize, dev: f64, tol: f64) -> usize {
    let f = (dev / tol).sqrt() * REFINE_MARGIN;
    let m = if f.is_finite() {
        (n as f64 * f).ceil().min(MAX_NURBS_DIVISIONS as f64) as usize
    } else {
        MAX_NURBS_DIVISIONS
    };
    m.max(n + 1).min(MAX_NURBS_DIVISIONS)
}

/// Gleichmäßige Teilung von `a … b` in `n` Abschnitte (`n + 1` Werte, Enden exakt).
fn steps(a: f64, b: f64, n: usize) -> Vec<f64> {
    (0..=n)
        .map(|k| {
            if k == n {
                b
            } else {
                a + (b - a) * k as f64 / n as f64
            }
        })
        .collect()
}

/// Diagonale des Hüllquaders der Punkte: Radiusmaß einer Freiformkurve oder -fläche für den
/// gemeinsamen Winkelschritt. Der Krümmungsradius einer Freiformgeometrie ist nicht bekannt;
/// die volle Diagonale (statt der halben, die für einen Kreis ungefähr der Radius wäre) wählt den
/// Winkelschritt bewusst feiner, damit Körper mit Freiformflächen auch im Volumen genau werden
/// (die Freiformkanten und -flächen selbst werden zusätzlich nach der Sehnentoleranz verfeinert).
fn diagonal(points: impl Iterator<Item = DVec3>) -> f64 {
    let (lo, hi) = points.fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(p), hi.max(p)),
    );
    (hi - lo).length()
}

/// Legt fest, in wie viele Segmente Bögen unterteilt werden. Der Winkelschritt hängt nur vom
/// größten Radius des Körpers ab; dadurch bekommen benachbarte Flächen und Kanten mit gleicher
/// Winkelspanne immer dieselben Teilpunkte und das Netz bleibt geschlossen.
///
/// Ein Vollkreis hat mindestens 16 und höchstens 720 Segmente (Winkelschritt 0,5°). Die obere
/// Grenze hält das Netz sehr großer Körper beherrschbar; dort wird die Sehnentoleranz dann
/// nicht mehr garantiert eingehalten.
#[derive(Clone, Copy, Debug)]
pub struct Discretizer {
    dtheta: f64,
    /// Sehnentoleranz (mm), für die Verfeinerung von Freiformkanten und -flächen.
    tol: f64,
}

impl Discretizer {
    pub fn new(solid: &Solid, chord_tolerance: f64) -> Self {
        let mut r_max = 0.0_f64;
        for (_, e) in solid.edges.iter() {
            r_max = r_max.max(match &e.curve {
                Curve::Line { .. } => 0.0,
                Curve::Circle { radius, .. } => *radius,
                Curve::Ellipse { major, minor, .. } => major.max(*minor),
                Curve::Nurbs(c) => diagonal(c.control_points().iter().copied()),
            });
        }
        for (_, f) in solid.faces.iter() {
            r_max = r_max.max(match &f.surface {
                Surface::Plane { .. } => 0.0,
                Surface::Cylinder { radius, .. } => *radius,
                Surface::Cone { radius, slope, .. } => match f.patch {
                    Some(p) => (radius + slope * p.v0)
                        .abs()
                        .max((radius + slope * p.v1).abs()),
                    None => *radius,
                },
                Surface::Sphere { radius, .. } => *radius,
                Surface::Torus { major, minor, .. } => major + minor,
                Surface::Nurbs(d) => {
                    let (lo, hi) = d.control_bounds();
                    diagonal([lo, hi].into_iter())
                }
            });
        }
        let tol = chord_tolerance.max(1e-9);
        let r = r_max.max(tol);
        let c = (1.0 - tol / r).clamp(-1.0, 1.0);
        // mindestens 16, höchstens 720 Segmente pro Vollkreis
        let dtheta = (2.0 * c.acos()).clamp(TAU / 720.0, PI / 8.0);
        Self { dtheta, tol }
    }

    /// Sehnentoleranz in Millimetern (mindestens 1e-9).
    pub fn chord_tolerance(&self) -> f64 {
        self.tol
    }

    /// Anzahl der Segmente für eine Winkelspanne (im Bogenmaß), mindestens 1.
    pub fn segments(&self, span: f64) -> usize {
        ((span.abs() / self.dtheta) - 1e-9).ceil().max(1.0) as usize
    }

    /// Linienzug einer Kante, beginnend und endend exakt an den Eckpunkten der Kante.
    pub fn edge_polyline(&self, solid: &Solid, id: Id<Edge>) -> Vec<DVec3> {
        let e = &solid.edges[id];
        let a = solid.vertices[e.start].point;
        let b = solid.vertices[e.end].point;
        match &e.curve {
            Curve::Line { .. } => vec![a, b],
            curve => {
                let n = self.curve_segments(curve, e.t0, e.t1);
                let mut pts: Vec<DVec3> = (0..=n)
                    .map(|k| curve.point(e.t0 + (e.t1 - e.t0) * k as f64 / n as f64))
                    .collect();
                pts[0] = a;
                pts[n] = b;
                pts
            }
        }
    }

    /// Abschnittszahl einer Kante `t0 … t1`. Freiformkanten: nach Bézier-Stücken, Grad und
    /// Knickwinkel des Kontrollpolygons ([`nurbs_divisions`] mit dem Winkelschritt des
    /// Discretizers), dann bis zu drei Runden verfeinert, bis die Sehnenmitte jedes Abschnitts
    /// höchstens um die Sehnentoleranz von der Kurve abweicht; höchstens
    /// [`MAX_NURBS_DIVISIONS`]. Hängt nur von der Kante ab, beide anliegenden Flächen bekommen
    /// also dieselben Punkte. Andere Kurven: [`Discretizer::segments`] der Parameterspanne (Winkel).
    pub fn curve_segments(&self, curve: &Curve, t0: f64, t1: f64) -> usize {
        let Curve::Nurbs(c) = curve else {
            return self.segments(t1 - t0);
        };
        let mut n = nurbs_divisions(c, t0, t1, self.dtheta).clamp(1, MAX_NURBS_DIVISIONS);
        if !(t0.is_finite() && t1.is_finite()) {
            return n;
        }
        for _ in 0..MAX_REFINE_ROUNDS {
            let ts = steps(t0, t1, n);
            let pts: Vec<DVec3> = ts.iter().map(|t| curve.point(*t)).collect();
            let dev = (0..n)
                .map(|k| {
                    let mid = curve.point(0.5 * (ts[k] + ts[k + 1]));
                    mid.distance(0.5 * (pts[k] + pts[k + 1]))
                })
                .fold(0.0, f64::max);
            if dev <= self.tol || n >= MAX_NURBS_DIVISIONS {
                break;
            }
            n = refined(n, dev, self.tol);
        }
        n
    }

    /// Unterteilung `(nu, nv)` eines Parameterrechtecks einer Freiformfläche: je Richtung
    /// `max(Bézier-Abschnitte · Grad, ⌈Knickwinkel / dtheta⌉)`, mindestens 2 · Abschnitte,
    /// höchstens [`MAX_NURBS_DIVISIONS`]. Der Knickwinkel ist die größte Winkelsumme der
    /// Kontrollpolygone in dieser Richtung, anteilig zur Breite des Rechtecks. Danach bis zu drei
    /// Verfeinerungsrunden, bis die Sehne jedes Gitterstücks (in u und in v) und die Zellmitte
    /// (gegen den Mittelwert der vier Ecken) höchstens um die Sehnentoleranz von der Fläche
    /// abweichen. Das Rechteck wird auf den
    /// Parameterbereich der Fläche begrenzt (leer oder nicht endlich: ganzer Bereich).
    pub fn nurbs_grid(&self, s: &NurbsSurface, patch: &UvRect) -> (usize, usize) {
        let ((du0, du1), (dv0, dv1)) = s.domain();
        let (u0, u1) = clamp_range(patch.u0, patch.u1, du0, du1);
        let (v0, v1) = clamp_range(patch.v0, patch.v1, dv0, dv1);
        let (cu, cv) = s.counts();
        let turn_u = (0..cv)
            .map(|j| {
                let row: Vec<DVec3> = (0..cu).map(|i| s.control_at(i, j)).collect();
                polygon_turning(&row)
            })
            .fold(0.0, f64::max);
        let turn_v = (0..cu)
            .map(|i| {
                let col: Vec<DVec3> = (0..cv).map(|j| s.control_at(i, j)).collect();
                polygon_turning(&col)
            })
            .fold(0.0, f64::max);
        let initial = |spans: usize, degree: usize, turn: f64, frac: f64| {
            let spans = spans.max(1);
            let by_angle = (turn * frac / self.dtheta).ceil();
            let by_angle = if by_angle.is_finite() {
                by_angle.max(0.0) as usize
            } else {
                0
            };
            (spans * degree)
                .max(by_angle)
                .max(2 * spans)
                .clamp(1, MAX_NURBS_DIVISIONS)
        };
        let mut nu = initial(
            s.knots_u().spans_in(u0, u1),
            s.degree_u(),
            turn_u,
            (u1 - u0) / (du1 - du0),
        );
        let mut nv = initial(
            s.knots_v().spans_in(v0, v1),
            s.degree_v(),
            turn_v,
            (v1 - v0) / (dv1 - dv0),
        );
        for _ in 0..MAX_REFINE_ROUNDS {
            let (us, vs) = (steps(u0, u1, nu), steps(v0, v1, nv));
            let grid: Vec<Vec<DVec3>> = vs
                .iter()
                .map(|v| us.iter().map(|u| s.point(*u, *v)).collect())
                .collect();
            let mut dev_u = 0.0_f64;
            for (j, v) in vs.iter().enumerate() {
                for i in 0..nu {
                    let mid = s.point(0.5 * (us[i] + us[i + 1]), *v);
                    dev_u = dev_u.max(mid.distance(0.5 * (grid[j][i] + grid[j][i + 1])));
                }
            }
            let mut dev_v = 0.0_f64;
            for (i, u) in us.iter().enumerate() {
                for j in 0..nv {
                    let mid = s.point(*u, 0.5 * (vs[j] + vs[j + 1]));
                    dev_v = dev_v.max(mid.distance(0.5 * (grid[j][i] + grid[j + 1][i])));
                }
            }
            // Zellmitte gegen den Mittelwert der vier Ecken: erfasst die Verwindung, also die
            // Abweichung der Diagonalen, über die die Triangulierung die Zellen teilt.
            let mut dev_c = 0.0_f64;
            for j in 0..nv {
                for i in 0..nu {
                    let mid = s.point(0.5 * (us[i] + us[i + 1]), 0.5 * (vs[j] + vs[j + 1]));
                    let avg =
                        0.25 * (grid[j][i] + grid[j][i + 1] + grid[j + 1][i] + grid[j + 1][i + 1]);
                    dev_c = dev_c.max(mid.distance(avg));
                }
            }
            // Ist nur die Zellmitte zu ungenau, wird die stärker gekrümmte Richtung verfeinert.
            if dev_c > self.tol && dev_u <= self.tol && dev_v <= self.tol {
                if dev_u >= dev_v {
                    dev_u = dev_c;
                } else {
                    dev_v = dev_c;
                }
            }
            let fine_u = dev_u <= self.tol || nu >= MAX_NURBS_DIVISIONS;
            let fine_v = dev_v <= self.tol || nv >= MAX_NURBS_DIVISIONS;
            if fine_u && fine_v {
                break;
            }
            if !fine_u {
                nu = refined(nu, dev_u, self.tol);
            }
            if !fine_v {
                nv = refined(nv, dev_v, self.tol);
            }
        }
        (nu, nv)
    }
}

/// Begrenzt `a … b` auf den Bereich `d0 … d1` (aufsteigend sortiert); ein leeres oder nicht
/// endliches Ergebnis ergibt den ganzen Bereich.
fn clamp_range(a: f64, b: f64, d0: f64, d1: f64) -> (f64, f64) {
    let (lo, hi) = (a.min(b).max(d0), a.max(b).min(d1));
    if lo.is_finite() && hi.is_finite() && hi > lo {
        (lo, hi)
    } else {
        (d0, d1)
    }
}
