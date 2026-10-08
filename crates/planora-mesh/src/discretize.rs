use planora_kernel::{Curve, Edge, Id, Solid, Surface};
use planora_math::DVec3;
use std::f64::consts::PI;

/// Legt fest, in wie viele Segmente Bögen unterteilt werden. Der Winkelschritt hängt nur vom
/// größten Radius des Körpers ab; dadurch bekommen benachbarte Flächen und Kanten mit gleicher
/// Winkelspanne immer dieselben Teilpunkte und das Netz bleibt geschlossen.
#[derive(Clone, Copy, Debug)]
pub struct Discretizer {
    dtheta: f64,
}

impl Discretizer {
    pub fn new(solid: &Solid, chord_tolerance: f64) -> Self {
        let mut r_max = 0.0_f64;
        for (_, e) in solid.edges.iter() {
            r_max = r_max.max(match &e.curve {
                Curve::Line { .. } => 0.0,
                Curve::Circle { radius, .. } => *radius,
                Curve::Ellipse { major, minor, .. } => major.max(*minor),
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
            });
        }
        let tol = chord_tolerance.max(1e-9);
        let r = r_max.max(tol);
        let c = (1.0 - tol / r).clamp(-1.0, 1.0);
        // mindestens 16 Segmente pro Vollkreis, höchstens etwa 6300
        let dtheta = (2.0 * c.acos()).clamp(1e-3, PI / 8.0);
        Self { dtheta }
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
                let n = self.segments(e.t1 - e.t0);
                let mut pts: Vec<DVec3> = (0..=n)
                    .map(|k| curve.point(e.t0 + (e.t1 - e.t0) * k as f64 / n as f64))
                    .collect();
                pts[0] = a;
                pts[n] = b;
                pts
            }
        }
    }
}
