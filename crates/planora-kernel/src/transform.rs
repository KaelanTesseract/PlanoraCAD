use crate::error::KernelError;
use crate::geometry::{Curve, Surface, UvRect};
use crate::topology::{Face, Loop, Solid};
use planora_math::DMat4;

impl Solid {
    /// Wendet eine Ähnlichkeitstransformation an (verschieben, drehen, spiegeln,
    /// gleichmäßig skalieren). IDs von Ecken, Kanten und Flächen bleiben erhalten.
    pub fn transformed(&self, m: &DMat4) -> Result<Solid, KernelError> {
        let det = m.determinant();
        let s = det.abs().cbrt();
        let cols = [
            m.x_axis.truncate(),
            m.y_axis.truncate(),
            m.z_axis.truncate(),
        ];
        let eps = 1e-9 * s.max(1.0);
        // Alle Einträge müssen endlich sein, auch die Verschiebung (nicht nur die Determinante).
        let is_similarity = m.is_finite()
            && s.is_finite()
            && s > 1e-12
            && cols.iter().all(|c| (c.length() - s).abs() <= eps)
            && cols[0].dot(cols[1]).abs() <= eps * s
            && cols[0].dot(cols[2]).abs() <= eps * s
            && cols[1].dot(cols[2]).abs() <= eps * s
            && m.x_axis.w == 0.0
            && m.y_axis.w == 0.0
            && m.z_axis.w == 0.0
            && m.w_axis.w == 1.0;
        if !is_similarity {
            return Err(KernelError::NonSimilarityTransform);
        }
        let mirrored = det < 0.0;

        let mut out = Solid::new();
        for (_, v) in self.vertices.iter() {
            out.add_vertex(m.transform_point3(v.point));
        }
        for (_, e) in self.edges.iter() {
            let (curve, t0, t1) = transform_curve(&e.curve, e.t0, e.t1, m, s);
            out.add_edge(curve, t0, t1, e.start, e.end);
        }
        for (_, f) in self.faces.iter() {
            let (surface, same_sense, patch) = transform_surface(f, m, s, mirrored);
            let fix = |l: &Loop| if mirrored { l.reversed() } else { l.clone() };
            out.add_face(Face {
                surface,
                same_sense,
                outer: fix(&f.outer),
                inner: f.inner.iter().map(fix).collect(),
                patch,
                tag: f.tag.clone(),
            });
        }
        out.shells = self.shells.clone();
        Ok(out)
    }
}

fn transform_curve(c: &Curve, t0: f64, t1: f64, m: &DMat4, s: f64) -> (Curve, f64, f64) {
    match c {
        Curve::Line { origin, dir } => (
            Curve::Line {
                origin: m.transform_point3(*origin),
                dir: m.transform_vector3(*dir).normalize(),
            },
            t0 * s,
            t1 * s,
        ),
        Curve::Circle { frame, radius } => (
            Curve::Circle {
                frame: frame.transformed(m),
                radius: radius * s,
            },
            t0,
            t1,
        ),
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => (
            Curve::Ellipse {
                frame: frame.transformed(m),
                major: major * s,
                minor: minor * s,
            },
            t0,
            t1,
        ),
    }
}

/// Bei einer Spiegelung bleibt die Parametrisierung `(u, v)` erhalten, außer dass `v` (die Achse
/// bzw. der Breitenwinkel) das Vorzeichen wechselt. Ebenen behalten `(u, v)`, ihre natürliche
/// Normale `x × y` zeigt aber nach der Spiegelung entgegengesetzt, daher kippt `same_sense`.
fn transform_surface(
    f: &Face,
    m: &DMat4,
    s: f64,
    mirrored: bool,
) -> (Surface, bool, Option<UvRect>) {
    let length_range = |r: UvRect| {
        if mirrored {
            UvRect {
                v0: -s * r.v1,
                v1: -s * r.v0,
                ..r
            }
        } else {
            UvRect {
                v0: s * r.v0,
                v1: s * r.v1,
                ..r
            }
        }
    };
    let angle_range = |r: UvRect| {
        if mirrored {
            UvRect {
                v0: -r.v1,
                v1: -r.v0,
                ..r
            }
        } else {
            r
        }
    };
    match &f.surface {
        Surface::Plane { frame } => (
            Surface::Plane {
                frame: frame.transformed(m),
            },
            if mirrored {
                !f.same_sense
            } else {
                f.same_sense
            },
            None,
        ),
        Surface::Cylinder { frame, radius } => (
            Surface::Cylinder {
                frame: frame.transformed(m),
                radius: radius * s,
            },
            f.same_sense,
            f.patch.map(length_range),
        ),
        Surface::Cone {
            frame,
            radius,
            slope,
        } => (
            Surface::Cone {
                frame: frame.transformed(m),
                radius: radius * s,
                slope: if mirrored { -slope } else { *slope },
            },
            f.same_sense,
            f.patch.map(length_range),
        ),
        Surface::Sphere { frame, radius } => (
            Surface::Sphere {
                frame: frame.transformed(m),
                radius: radius * s,
            },
            f.same_sense,
            f.patch.map(angle_range),
        ),
        Surface::Torus {
            frame,
            major,
            minor,
        } => (
            Surface::Torus {
                frame: frame.transformed(m),
                major: major * s,
                minor: minor * s,
            },
            f.same_sense,
            f.patch.map(angle_range),
        ),
    }
}
