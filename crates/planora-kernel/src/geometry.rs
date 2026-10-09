use planora_math::{DVec3, Frame, PI};

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

impl Curve {
    /// Parameter `t` des Kurvenpunkts, der `p` am nächsten liegt (für Punkte auf der Kurve exakt).
    pub fn project(&self, p: DVec3) -> f64 {
        match self {
            Curve::Line { origin, dir } => (p - *origin).dot(*dir),
            Curve::Circle { frame, .. } => {
                let l = frame.to_local(p);
                l.y.atan2(l.x)
            }
            Curve::Ellipse {
                frame,
                major,
                minor,
            } => {
                let l = frame.to_local(p);
                (l.y / minor).atan2(l.x / major)
            }
        }
    }

    /// Ableitung nach `t` (nicht normiert).
    pub fn tangent(&self, t: f64) -> DVec3 {
        match self {
            Curve::Line { dir, .. } => *dir,
            Curve::Circle { frame, radius } => (frame.y * t.cos() - frame.x * t.sin()) * *radius,
            Curve::Ellipse {
                frame,
                major,
                minor,
            } => frame.y * (minor * t.cos()) - frame.x * (major * t.sin()),
        }
    }
}

impl Surface {
    /// Vorzeichenbehaftete Abstandsfunktion: > 0 auf der Seite der natürlichen Normale,
    /// 0 auf der Fläche (Zylinder, Kegel, Kugel, Torus: außen positiv).
    pub fn implicit(&self, p: DVec3) -> f64 {
        match self {
            Surface::Plane { frame } => (p - frame.origin).dot(frame.z),
            Surface::Cylinder { frame, radius } => {
                let d = p - frame.origin;
                let h = d.dot(frame.z);
                (d - frame.z * h).length() - radius
            }
            Surface::Cone {
                frame,
                radius,
                slope,
            } => {
                let d = p - frame.origin;
                let h = d.dot(frame.z);
                let rho = (d - frame.z * h).length();
                (rho - (radius + slope * h)) / (1.0 + slope * slope).sqrt()
            }
            Surface::Sphere { frame, radius } => (p - frame.origin).length() - radius,
            Surface::Torus {
                frame,
                major,
                minor,
            } => {
                let d = p - frame.origin;
                let h = d.dot(frame.z);
                let rho = (d - frame.z * h).length();
                ((rho - major).powi(2) + h * h).sqrt() - minor
            }
        }
    }

    /// Umkehrabbildung Punkt → (u, v). Bei periodischen Flächen liegt `u` in (−π, π].
    pub fn project(&self, p: DVec3) -> (f64, f64) {
        let l = self.frame().to_local(p);
        let u = l.y.atan2(l.x);
        let rho = (l.x * l.x + l.y * l.y).sqrt();
        match self {
            Surface::Plane { .. } => (l.x, l.y),
            Surface::Cylinder { .. } | Surface::Cone { .. } => (u, l.z),
            Surface::Sphere { .. } => (u, l.z.atan2(rho)),
            Surface::Torus { major, .. } => (u, l.z.atan2(rho - major)),
        }
    }

    pub fn is_periodic(&self) -> bool {
        !matches!(self, Surface::Plane { .. })
    }

    /// Werte von `v`, bei denen die u-Linie zu einem Punkt zusammenfällt (Kugelpole, Kegelspitze).
    pub fn pole_vs(&self) -> Vec<f64> {
        match self {
            Surface::Sphere { .. } => vec![-PI / 2.0, PI / 2.0],
            Surface::Cone { radius, slope, .. } if slope.abs() > 1e-12 => vec![-radius / slope],
            _ => vec![],
        }
    }
}

/// Art einer Fläche ohne Parameter, für Anzeigen in der Oberfläche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceKind {
    Plane,
    Cylinder,
    Cone,
    Sphere,
    Torus,
}

impl SurfaceKind {
    /// Deutscher Anzeigename.
    pub fn label(self) -> &'static str {
        match self {
            SurfaceKind::Plane => "Ebene",
            SurfaceKind::Cylinder => "Zylinderfläche",
            SurfaceKind::Cone => "Kegelfläche",
            SurfaceKind::Sphere => "Kugelfläche",
            SurfaceKind::Torus => "Torusfläche",
        }
    }
}

impl Surface {
    pub fn kind(&self) -> SurfaceKind {
        match self {
            Surface::Plane { .. } => SurfaceKind::Plane,
            Surface::Cylinder { .. } => SurfaceKind::Cylinder,
            Surface::Cone { .. } => SurfaceKind::Cone,
            Surface::Sphere { .. } => SurfaceKind::Sphere,
            Surface::Torus { .. } => SurfaceKind::Torus,
        }
    }
}

/// Art einer Kurve ohne Parameter, für Anzeigen in der Oberfläche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CurveKind {
    Line,
    Circle,
    Ellipse,
}

impl CurveKind {
    /// Deutscher Anzeigename.
    pub fn label(self) -> &'static str {
        match self {
            CurveKind::Line => "Gerade",
            CurveKind::Circle => "Kreis",
            CurveKind::Ellipse => "Ellipse",
        }
    }
}

impl Curve {
    pub fn kind(&self) -> CurveKind {
        match self {
            Curve::Line { .. } => CurveKind::Line,
            Curve::Circle { .. } => CurveKind::Circle,
            Curve::Ellipse { .. } => CurveKind::Ellipse,
        }
    }
}
