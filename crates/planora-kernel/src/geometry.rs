use planora_math::{DVec3, Frame};

/// Parametrische Kurve. Die Parameterbereiche stehen an der [`Edge`](crate::Edge).
#[derive(Clone, Debug)]
pub enum Curve {
    /// `origin + dir * t`, `dir` hat die Länge 1 (t = Länge entlang der Geraden).
    Line { origin: DVec3, dir: DVec3 },
    /// `origin + radius * (cos t * x + sin t * y)`
    Circle { frame: Frame, radius: f64 },
    /// `origin + major * cos t * x + minor * sin t * y`
    Ellipse {
        frame: Frame,
        major: f64,
        minor: f64,
    },
}

impl Curve {
    pub fn point(&self, t: f64) -> DVec3 {
        match self {
            Curve::Line { origin, dir } => *origin + *dir * t,
            Curve::Circle { frame, radius } => {
                frame.origin + (frame.x * t.cos() + frame.y * t.sin()) * *radius
            }
            Curve::Ellipse {
                frame,
                major,
                minor,
            } => frame.origin + frame.x * (major * t.cos()) + frame.y * (minor * t.sin()),
        }
    }
}

/// Rechteckiger Parameterbereich einer gekrümmten Fläche.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UvRect {
    pub u0: f64,
    pub u1: f64,
    pub v0: f64,
    pub v1: f64,
}

/// Analytische Fläche. Die natürliche Normale ist `∂P/∂u × ∂P/∂v`.
#[derive(Clone, Debug)]
pub enum Surface {
    /// `P = origin + u * x + v * y`, Normale `z`.
    Plane { frame: Frame },
    /// `u` = Winkel um `z`, `v` = Höhe entlang `z`.
    Cylinder { frame: Frame, radius: f64 },
    /// `u` = Winkel, `v` = Höhe; Radius an der Stelle `v` ist `radius + slope * v`.
    Cone {
        frame: Frame,
        radius: f64,
        slope: f64,
    },
    /// `u` = Länge, `v` = Breite in [-π/2, π/2].
    Sphere { frame: Frame, radius: f64 },
    /// `u` = Winkel um `z`, `v` = Winkel um das Rohr.
    Torus {
        frame: Frame,
        major: f64,
        minor: f64,
    },
}

fn radial(frame: &Frame, u: f64) -> DVec3 {
    frame.x * u.cos() + frame.y * u.sin()
}

impl Surface {
    pub fn frame(&self) -> &Frame {
        match self {
            Surface::Plane { frame }
            | Surface::Cylinder { frame, .. }
            | Surface::Cone { frame, .. }
            | Surface::Sphere { frame, .. }
            | Surface::Torus { frame, .. } => frame,
        }
    }

    pub fn point(&self, u: f64, v: f64) -> DVec3 {
        match self {
            Surface::Plane { frame } => frame.origin + frame.x * u + frame.y * v,
            Surface::Cylinder { frame, radius } => {
                frame.origin + radial(frame, u) * *radius + frame.z * v
            }
            Surface::Cone {
                frame,
                radius,
                slope,
            } => frame.origin + radial(frame, u) * (*radius + *slope * v) + frame.z * v,
            Surface::Sphere { frame, radius } => {
                frame.origin + (radial(frame, u) * v.cos() + frame.z * v.sin()) * *radius
            }
            Surface::Torus {
                frame,
                major,
                minor,
            } => {
                frame.origin
                    + radial(frame, u) * (*major + *minor * v.cos())
                    + frame.z * (*minor * v.sin())
            }
        }
    }

    /// Natürliche Einheitsnormale `∂P/∂u × ∂P/∂v`.
    pub fn normal(&self, u: f64, v: f64) -> DVec3 {
        match self {
            Surface::Plane { frame } => frame.z,
            Surface::Cylinder { frame, .. } => radial(frame, u),
            Surface::Cone { frame, slope, .. } => {
                (radial(frame, u) - frame.z * *slope) / (1.0 + slope * slope).sqrt()
            }
            Surface::Sphere { frame, .. } | Surface::Torus { frame, .. } => {
                radial(frame, u) * v.cos() + frame.z * v.sin()
            }
        }
    }
}
