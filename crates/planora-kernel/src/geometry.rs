use crate::error::KernelError;
use crate::nurbs_surface::NurbsSurfaceData;
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DVec3, Frame, PI, TAU, Tolerance};
use std::sync::Arc;

/// Probestellen je Kontrollpunkt bei der Rückrechnung eines Punkts auf eine NURBS-Kurve.
const CURVE_SAMPLES_PER_CONTROL: usize = 16;
/// Zahl der besten örtlichen Probenminima, von denen Newton startet (dazu beide Enden).
const CURVE_NEWTON_STARTS: usize = 3;
/// Höchstzahl Newton-Schritte je Start.
const CURVE_NEWTON_STEPS: usize = 30;
/// Newton endet, wenn der Schritt kleiner als dieser Bruchteil des Parameterbereichs ist.
const CURVE_STEP_RATIO: f64 = 1e-15;

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
    /// Freiformkurve; `t` ist der NURBS-Parameter, Bereich = `curve.domain()`. Bei geschlossenen
    /// Kurven werden Parameter außerhalb des Bereichs um die Periode gefaltet.
    Nurbs(Arc<NurbsCurve>),
}

/// Anfang und Ende einer NURBS-Kurve fallen innerhalb der Längentoleranz zusammen.
fn nurbs_closed(c: &NurbsCurve) -> bool {
    c.is_closed(Tolerance::default().linear)
}

/// Faltet `t` bei geschlossenen Kurven in den Bereich (nur außerhalb, damit die Enden selbst
/// erhalten bleiben); sonst unverändert (die Auswertung begrenzt dann auf den Bereich).
fn nurbs_param(c: &NurbsCurve, t: f64) -> f64 {
    let (a, b) = c.domain();
    if t.is_finite() && (t < a || t > b) && nurbs_closed(c) {
        a + (t - a).rem_euclid(b - a)
    } else {
        t
    }
}

/// Newton auf `(C(u) − p) · C'(u) = 0` von `u` aus (begrenzt auf den Bereich); liefert den
/// besten gesehenen Parameter samt Abstandsquadrat.
fn nurbs_newton(c: &NurbsCurve, u: f64, p: DVec3) -> (f64, f64) {
    let (a, b) = c.domain();
    let mut u = u.clamp(a, b);
    let mut best = (u, c.point(u).distance_squared(p));
    for _ in 0..CURVE_NEWTON_STEPS {
        let d = c.derivs(u, 2);
        let r = d[0] - p;
        let f = r.dot(d[1]);
        let df = d[1].length_squared() + r.dot(d[2]);
        if df.is_nan() || df <= 0.0 || !f.is_finite() {
            break;
        }
        let next = (u - f / df).clamp(a, b);
        let dn = c.point(next).distance_squared(p);
        if dn < best.1 {
            best = (next, dn);
        }
        if (next - u).abs() <= CURVE_STEP_RATIO * (b - a) {
            break;
        }
        u = next;
    }
    best
}

/// Parameter des nächsten Kurvenpunkts (grobe Probe, dann Newton von den besten örtlichen
/// Minima und beiden Enden); nicht endliche Punkte ergeben den Anfang des Bereichs.
fn nurbs_closest_param(c: &NurbsCurve, p: DVec3) -> f64 {
    let (a, b) = c.domain();
    if !p.is_finite() {
        return a;
    }
    let n = CURVE_SAMPLES_PER_CONTROL * c.control_points().len();
    let u_at = |i: usize| a + (b - a) * i as f64 / n as f64;
    let d: Vec<f64> = (0..=n)
        .map(|i| c.point(u_at(i)).distance_squared(p))
        .collect();
    let mut minima: Vec<usize> = (0..=n)
        .filter(|&i| (i == 0 || d[i] <= d[i - 1]) && (i == n || d[i] <= d[i + 1]))
        .collect();
    minima.sort_by(|x, y| d[*x].total_cmp(&d[*y]));
    minima
        .iter()
        .take(CURVE_NEWTON_STARTS)
        .map(|i| u_at(*i))
        .chain([a, b])
        .map(|u| nurbs_newton(c, u, p))
        .fold((a, f64::INFINITY), |x, y| if y.1 < x.1 { y } else { x })
        .0
}

impl Curve {
    pub fn nurbs(c: NurbsCurve) -> Curve {
        Curve::Nurbs(Arc::new(c))
    }

    /// Periode des Parameters: Kreis und Ellipse `TAU`, geschlossene NURBS-Kurve die Breite des
    /// Parameterbereichs, sonst `None`.
    pub fn period(&self) -> Option<f64> {
        match self {
            Curve::Line { .. } => None,
            Curve::Circle { .. } | Curve::Ellipse { .. } => Some(TAU),
            Curve::Nurbs(c) => nurbs_closed(c).then(|| {
                let (a, b) = c.domain();
                b - a
            }),
        }
    }

    /// Parameterbereich (nur NURBS-Kurven haben einen festen Bereich).
    pub fn domain(&self) -> Option<(f64, f64)> {
        match self {
            Curve::Nurbs(c) => Some(c.domain()),
            _ => None,
        }
    }

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
            Curve::Nurbs(c) => c.point(nurbs_param(c, t)),
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
    /// Freiformfläche; `(u, v)` sind die NURBS-Parameter, Bereich = `surface().domain()`.
    /// Geschlossene Richtungen (Naht wie beim Zylinder) falten Parameter außerhalb des Bereichs.
    Nurbs(Arc<NurbsSurfaceData>),
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
            Surface::Nurbs(d) => d.frame(),
        }
    }

    /// Freiformfläche mit Prüfung der Kennzeichen (siehe [`NurbsSurfaceData::new`]).
    pub fn nurbs(
        surface: NurbsSurface,
        periodic_u: bool,
        periodic_v: bool,
    ) -> Result<Surface, KernelError> {
        Ok(Surface::Nurbs(Arc::new(NurbsSurfaceData::new(
            surface, periodic_u, periodic_v,
        )?)))
    }

    /// Wie [`Surface::project`], mit Startwert (für fortlaufende Punkte einer Schleife). Für
    /// analytische Flächen ist der Startwert ohne Bedeutung.
    pub fn project_near(&self, p: DVec3, hint: (f64, f64)) -> (f64, f64) {
        match self {
            Surface::Nurbs(d) => d.project_near(p, hint),
            _ => self.project(p),
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
            Surface::Nurbs(d) => d.point(u, v),
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
            Surface::Nurbs(d) => d.normal(u, v),
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
            Curve::Nurbs(c) => nurbs_closest_param(c, p),
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
            Curve::Nurbs(c) => c.tangent(nurbs_param(c, t)),
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
            Surface::Nurbs(d) => d.signed_distance(p),
        }
    }

    /// Umkehrabbildung Punkt → (u, v). Bei periodischen analytischen Flächen liegt `u` in
    /// (−π, π]. Freiformflächen: nächster Punkt; konvergiert die Suche nicht, die Mitte des
    /// Parameterbereichs (Aufrufer prüfen den Abstand selbst).
    pub fn project(&self, p: DVec3) -> (f64, f64) {
        let l = self.frame().to_local(p);
        let u = l.y.atan2(l.x);
        let rho = (l.x * l.x + l.y * l.y).sqrt();
        match self {
            Surface::Plane { .. } => (l.x, l.y),
            Surface::Cylinder { .. } | Surface::Cone { .. } => (u, l.z),
            Surface::Sphere { .. } => (u, l.z.atan2(rho)),
            Surface::Torus { major, .. } => (u, l.z.atan2(rho - major)),
            Surface::Nurbs(d) => d.project(p),
        }
    }

    /// Geschlossen in mindestens einer Richtung (Freiformflächen: Kennzeichen in u oder v).
    pub fn is_periodic(&self) -> bool {
        match self {
            Surface::Plane { .. } => false,
            Surface::Nurbs(d) => d.is_periodic_u() || d.is_periodic_v(),
            _ => true,
        }
    }

    /// Werte von `v`, bei denen die u-Linie zu einem Punkt zusammenfällt (Kugelpole, Kegelspitze).
    pub fn pole_vs(&self) -> Vec<f64> {
        match self {
            Surface::Sphere { .. } => vec![-PI / 2.0, PI / 2.0],
            Surface::Cone { radius, slope, .. } if slope.abs() > 1e-12 => vec![-radius / slope],
            Surface::Nurbs(d) => d.poles_v().to_vec(),
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
    Nurbs,
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
            SurfaceKind::Nurbs => "Freiformfläche",
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
            Surface::Nurbs(_) => SurfaceKind::Nurbs,
        }
    }
}

/// Art einer Kurve ohne Parameter, für Anzeigen in der Oberfläche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CurveKind {
    Line,
    Circle,
    Ellipse,
    Nurbs,
}

impl CurveKind {
    /// Deutscher Anzeigename.
    pub fn label(self) -> &'static str {
        match self {
            CurveKind::Line => "Gerade",
            CurveKind::Circle => "Kreis",
            CurveKind::Ellipse => "Ellipse",
            CurveKind::Nurbs => "Freiformkurve",
        }
    }
}

impl Curve {
    pub fn kind(&self) -> CurveKind {
        match self {
            Curve::Line { .. } => CurveKind::Line,
            Curve::Circle { .. } => CurveKind::Circle,
            Curve::Ellipse { .. } => CurveKind::Ellipse,
            Curve::Nurbs(_) => CurveKind::Nurbs,
        }
    }
}
