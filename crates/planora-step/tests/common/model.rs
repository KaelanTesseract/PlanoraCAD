//! Aus der gelesenen Datei aufgebautes Modell (Spec §5.1, §5.2): Teile, Körper, Hüllen, Flächen,
//! Schleifen, Kanten, Eckpunkte sowie Kurven und Flächen mit eigenen Formeln (unabhängig vom
//! Kernel). Prüft beim Aufbau: Richtungen normiert, Koordinatensysteme senkrecht, Radien und
//! Winkel im erlaubten Bereich, Knoten und Vielfachheiten stimmig.

use super::reader::{Entity, StepFile, Value, parse};
use planora_math::{DVec3, PI, TAU};
use std::collections::BTreeMap;

/// Größte Abweichung der Länge einer Richtung von 1.
pub const DIR_TOL: f64 = 1e-9;
/// Größter Betrag des Skalarprodukts von Achse und Bezugsrichtung eines Koordinatensystems.
pub const ORTHO_TOL: f64 = 1e-9;
/// Probestellen je Kontrollpunkt bei der Suche des nächsten Kurvenpunkts.
const CURVE_SAMPLES_PER_CONTROL: usize = 16;
/// Schritte der Intervallschachtelung nach der groben Probe (Kurve).
const CURVE_REFINE_STEPS: usize = 80;
/// Newton-Schritte für den Fußpunkt auf einer Ellipse (quadratische Annäherung vom Startwinkel;
/// jeder Schritt wird nur angenommen, wenn der Abstand sinkt).
const ELLIPSE_NEWTON_STEPS: usize = 8;
/// Rasterpunkte je Knotenabschnitt und Richtung bei der Suche des nächsten Flächenpunkts.
const SURFACE_GRID_PER_SPAN: usize = 8;
/// Mindestzahl der Rasterpunkte je Richtung.
const SURFACE_GRID_MIN: usize = 16;
/// Höchstzahl der Gauß-Newton-Schritte auf einer Freiformfläche.
const SURFACE_NEWTON_STEPS: usize = 40;
/// Schrittweite der Differenzenquotienten, bezogen auf die Breite des Parameterbereichs.
const DIFF_STEP_REL: f64 = 1e-6;
/// Newton endet, wenn der Schritt kleiner als dieser Anteil des Parameterbereichs ist.
const NEWTON_STEP_REL: f64 = 1e-14;
/// So nah (mm) muss der Punkt aus dem Startwert liegen, damit die Rastersuche entfällt.
pub const NEAR_ACCEPT: f64 = 1e-9;
/// Vielfaches der Maschinengenauigkeit (relativ zur Größe der Koordinaten), bis zu dem ein Punkt
/// als auf der Kegelachse und damit als Spitze gilt: Eine gedrehte oder gespiegelte Spitze liegt
/// durch Rundung etwas neben der Achse, und dort ist die Richtung von der Achse weg bedeutungslos.
const APEX_AXIS_EPS_FACTOR: f64 = 64.0;
/// Die Normale wird so weit (Anteil des Bereichs) vom Rand entfernt ausgewertet; an Polen ist
/// `S_u` dort null.
const NORMAL_INSET_REL: f64 = 1e-4;

/// Rechtshändiges Koordinatensystem aus `AXIS2_PLACEMENT_3D` (`y = z × x`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub origin: DVec3,
    pub x: DVec3,
    pub y: DVec3,
    pub z: DVec3,
}

impl Axis {
    pub fn local(&self, p: DVec3) -> DVec3 {
        let d = p - self.origin;
        DVec3::new(d.dot(self.x), d.dot(self.y), d.dot(self.z))
    }

    /// Einheitsvektor in der xy-Ebene unter dem Winkel `u`.
    pub fn radial(&self, u: f64) -> DVec3 {
        self.x * u.cos() + self.y * u.sin()
    }
}

/// Knotenvektor mit Vielfachheiten ausgeschrieben.
fn expand_knots(mults: &[i64], values: &[f64]) -> Result<Vec<f64>, String> {
    if mults.len() != values.len() || mults.is_empty() {
        return Err("Vielfachheiten und Knotenwerte passen nicht zusammen".into());
    }
    if values.windows(2).any(|w| w[1] <= w[0]) {
        return Err("Knotenwerte nicht streng steigend".into());
    }
    let mut out = Vec::new();
    for (m, v) in mults.iter().zip(values) {
        if *m < 1 {
            return Err("Vielfachheit unter 1".into());
        }
        out.extend(std::iter::repeat_n(*v, *m as usize));
    }
    Ok(out)
}

/// Prüft Grad, Zahl der Kontrollpunkte und Randvielfachheiten (höchstens Grad + 1).
fn check_knots(degree: usize, count: usize, mults: &[i64], knots: &[f64]) -> Result<(), String> {
    if degree < 1 {
        return Err("Grad unter 1".into());
    }
    if knots.len() != count + degree + 1 {
        return Err(format!(
            "{} Knoten, erwartet {} (Kontrollpunkte + Grad + 1)",
            knots.len(),
            count + degree + 1
        ));
    }
    let edge_ok = |m: i64| m <= degree as i64 + 1;
    if !edge_ok(mults[0]) || !edge_ok(mults[mults.len() - 1]) {
        return Err("Randknoten öfter als Grad + 1".into());
    }
    if mults[1..mults.len() - 1].iter().any(|m| *m > degree as i64) {
        return Err("innerer Knoten öfter als der Grad".into());
    }
    Ok(())
}

/// Index des Knotenabschnitts mit `knots[i] <= u < knots[i + 1]`; am rechten Rand der letzte
/// nichtleere Abschnitt.
fn find_span(knots: &[f64], degree: usize, count: usize, u: f64) -> usize {
    if u >= knots[count] {
        let mut i = count - 1;
        while knots[i] >= knots[i + 1] {
            i -= 1;
        }
        return i;
    }
    let mut i = degree;
    while u >= knots[i + 1] {
        i += 1;
    }
    i
}

/// Werte `N_{span−p+k, p}(u)`, `k = 0..=p`, nach der Rekursion von Cox und de Boor.
fn basis(knots: &[f64], degree: usize, span: usize, u: f64) -> Vec<f64> {
    let p = degree;
    let mut prev = vec![0.0; p + 2];
    prev[p] = 1.0;
    for d in 1..=p {
        let mut cur = vec![0.0; p + 2];
        for k in (p - d)..=p {
            let i = span - p + k;
            let left = if knots[i + d] > knots[i] {
                (u - knots[i]) / (knots[i + d] - knots[i]) * prev[k]
            } else {
                0.0
            };
            let right = if knots[i + d + 1] > knots[i + 1] {
                (knots[i + d + 1] - u) / (knots[i + d + 1] - knots[i + 1]) * prev[k + 1]
            } else {
                0.0
            };
            cur[k] = left + right;
        }
        prev = cur;
    }
    prev.truncate(p + 1);
    prev
}

/// B-Spline-Kurve aus der Datei (Knoten ausgeschrieben).
#[derive(Clone, Debug, PartialEq)]
pub struct Spline1 {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub control: Vec<DVec3>,
    pub weights: Vec<f64>,
    pub closed: bool,
}

impl Spline1 {
    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.control.len()])
    }

    /// Punkt bei `t`; geschlossene Kurven falten `t` in den Bereich, offene begrenzen.
    pub fn point(&self, t: f64) -> DVec3 {
        let (a, b) = self.domain();
        let t = if self.closed && (t < a || t > b) {
            a + (t - a).rem_euclid(b - a)
        } else {
            t
        };
        let t = t.clamp(a, b);
        let span = find_span(&self.knots, self.degree, self.control.len(), t);
        let n = basis(&self.knots, self.degree, span, t);
        let (mut sum, mut w) = (DVec3::ZERO, 0.0);
        for (k, nk) in n.iter().enumerate() {
            let i = span - self.degree + k;
            sum += self.control[i] * (nk * self.weights[i]);
            w += nk * self.weights[i];
        }
        sum / w
    }
}

/// B-Spline-Fläche aus der Datei; `control[i][j]`, `i` in u-, `j` in v-Richtung.
#[derive(Clone, Debug, PartialEq)]
pub struct Spline2 {
    pub du: usize,
    pub dv: usize,
    pub ku: Vec<f64>,
    pub kv: Vec<f64>,
    pub control: Vec<Vec<DVec3>>,
    pub weights: Vec<Vec<f64>>,
    pub u_closed: bool,
    pub v_closed: bool,
}

impl Spline2 {
    pub fn counts(&self) -> (usize, usize) {
        (self.control.len(), self.control[0].len())
    }

    pub fn domain(&self) -> ((f64, f64), (f64, f64)) {
        let (nu, nv) = self.counts();
        (
            (self.ku[self.du], self.ku[nu]),
            (self.kv[self.dv], self.kv[nv]),
        )
    }

    pub fn point(&self, u: f64, v: f64) -> DVec3 {
        let (nu, nv) = self.counts();
        let ((u0, u1), (v0, v1)) = self.domain();
        let (u, v) = (u.clamp(u0, u1), v.clamp(v0, v1));
        let su = find_span(&self.ku, self.du, nu, u);
        let sv = find_span(&self.kv, self.dv, nv, v);
        let bu = basis(&self.ku, self.du, su, u);
        let bv = basis(&self.kv, self.dv, sv, v);
        let (mut sum, mut w) = (DVec3::ZERO, 0.0);
        for (a, nu_a) in bu.iter().enumerate() {
            let i = su - self.du + a;
            for (b, nv_b) in bv.iter().enumerate() {
                let j = sv - self.dv + b;
                let f = nu_a * nv_b * self.weights[i][j];
                sum += self.control[i][j] * f;
                w += f;
            }
        }
        sum / w
    }

    /// Ableitungen nach u und v über zentrale Differenzen (am Rand einseitig).
    pub fn derivs(&self, u: f64, v: f64) -> (DVec3, DVec3) {
        let ((u0, u1), (v0, v1)) = self.domain();
        let (hu, hv) = (DIFF_STEP_REL * (u1 - u0), DIFF_STEP_REL * (v1 - v0));
        let (ua, ub) = ((u - hu).max(u0), (u + hu).min(u1));
        let (va, vb) = ((v - hv).max(v0), (v + hv).min(v1));
        let su = (self.point(ub, v) - self.point(ua, v)) / (ub - ua);
        let sv = (self.point(u, vb) - self.point(u, va)) / (vb - va);
        (su, sv)
    }

    /// Geschlossene Richtungen über die Naht falten, offene begrenzen.
    fn fold(&self, u: f64, v: f64) -> (f64, f64) {
        let ((u0, u1), (v0, v1)) = self.domain();
        let f = |x: f64, a: f64, b: f64, closed: bool| {
            if closed {
                a + (x - a).rem_euclid(b - a)
            } else {
                x.clamp(a, b)
            }
        };
        (f(u, u0, u1, self.u_closed), f(v, v0, v1, self.v_closed))
    }

    /// Mustersuche ohne Ableitungen von `(u, v)` aus mit den Anfangsschritten `(hu, hv)`
    /// (für Stellen, an denen Newton nicht greift, etwa nahe an Polen).
    fn compass(&self, p: DVec3, start: (f64, f64, f64), hu: f64, hv: f64) -> (f64, f64, f64) {
        let ((u0, u1), (v0, v1)) = self.domain();
        let mut best = start;
        let (mut hu, mut hv) = (hu, hv);
        while hu > NEWTON_STEP_REL * (u1 - u0) || hv > NEWTON_STEP_REL * (v1 - v0) {
            let mut moved = false;
            for (du, dv) in [(hu, 0.0), (-hu, 0.0), (0.0, hv), (0.0, -hv)] {
                let (u, v) = self.fold(best.0 + du, best.1 + dv);
                let d = self.point(u, v).distance(p);
                if d < best.2 {
                    best = (u, v, d);
                    moved = true;
                }
            }
            if !moved {
                hu *= 0.5;
                hv *= 0.5;
            }
        }
        best
    }

    /// Gauß-Newton auf `|S(u, v) − p|` von `(u, v)` aus (offene Richtungen begrenzt, geschlossene
    /// über die Naht gefaltet); liefert den besten gesehenen Punkt `(u, v, Abstand)`.
    fn newton(&self, p: DVec3, u: f64, v: f64) -> (f64, f64, f64) {
        let ((u0, u1), (v0, v1)) = self.domain();
        let (mut u, mut v) = (u, v);
        let mut best = (u, v, self.point(u, v).distance(p));
        for _ in 0..SURFACE_NEWTON_STEPS {
            let r = self.point(u, v) - p;
            let (su, sv) = self.derivs(u, v);
            let (a, b, c) = (su.dot(su), su.dot(sv), sv.dot(sv));
            let det = a * c - b * b;
            if det.abs() <= f64::MIN_POSITIVE {
                break;
            }
            let (gu, gv) = (su.dot(r), sv.dot(r));
            let du = (c * gu - b * gv) / det;
            let dv = (a * gv - b * gu) / det;
            (u, v) = self.fold(u - du, v - dv);
            let d = self.point(u, v).distance(p);
            if d < best.2 {
                best = (u, v, d);
            }
            if du.abs() <= NEWTON_STEP_REL * (u1 - u0) && dv.abs() <= NEWTON_STEP_REL * (v1 - v0) {
                break;
            }
        }
        best
    }

    /// Nächster Flächenpunkt `(u, v, Abstand)`: Newton vom Startwert `hint`, wenn der dabei
    /// gefundene Punkt höchstens [`NEAR_ACCEPT`] entfernt ist; sonst Raster, dann Newton.
    pub fn closest_near(&self, p: DVec3, hint: Option<(f64, f64)>) -> (f64, f64, f64) {
        if let Some((u, v)) = hint {
            let r = self.newton(p, u, v);
            if r.2 <= NEAR_ACCEPT {
                return r;
            }
        }
        let ((u0, u1), (v0, v1)) = self.domain();
        let (nu, nv) = self.counts();
        let gu = (SURFACE_GRID_PER_SPAN * (nu - self.du)).max(SURFACE_GRID_MIN);
        let gv = (SURFACE_GRID_PER_SPAN * (nv - self.dv)).max(SURFACE_GRID_MIN);
        let mut best = (u0, v0, f64::INFINITY);
        for a in 0..=gu {
            let u = u0 + (u1 - u0) * a as f64 / gu as f64;
            for b in 0..=gv {
                let v = v0 + (v1 - v0) * b as f64 / gv as f64;
                let d = self.point(u, v).distance(p);
                if d < best.2 {
                    best = (u, v, d);
                }
            }
        }
        let r = self.newton(p, best.0, best.1);
        if r.2 <= NEAR_ACCEPT {
            return r;
        }
        self.compass(p, r, (u1 - u0) / gu as f64, (v1 - v0) / gv as f64)
    }

    pub fn closest(&self, p: DVec3) -> (f64, f64, f64) {
        self.closest_near(p, None)
    }

    /// Natürliche Einheitsnormale `S_u × S_v` bei `(u, v)`; am Rand (Pole liegen dort) etwas
    /// nach innen versetzt ausgewertet.
    pub fn normal(&self, u: f64, v: f64) -> DVec3 {
        let ((u0, u1), (v0, v1)) = self.domain();
        let (iu, iv) = (NORMAL_INSET_REL * (u1 - u0), NORMAL_INSET_REL * (v1 - v0));
        let (su, sv) = self.derivs(u.clamp(u0 + iu, u1 - iu), v.clamp(v0 + iv, v1 - iv));
        su.cross(sv).normalize_or_zero()
    }
}

/// Kurve einer Kante.
#[derive(Clone, Debug, PartialEq)]
pub enum Crv {
    /// `p + t · d` (`d` = Richtung mal Betrag des `VECTOR`).
    Line {
        p: DVec3,
        d: DVec3,
    },
    Circle {
        a: Axis,
        r: f64,
    },
    Ellipse {
        a: Axis,
        r1: f64,
        r2: f64,
    },
    Spline(Spline1),
}

impl Crv {
    pub fn point(&self, t: f64) -> DVec3 {
        match self {
            Crv::Line { p, d } => *p + *d * t,
            Crv::Circle { a, r } => a.origin + a.radial(t) * *r,
            Crv::Ellipse { a, r1, r2 } => a.origin + a.x * (r1 * t.cos()) + a.y * (r2 * t.sin()),
            Crv::Spline(s) => s.point(t),
        }
    }

    /// Ableitung nach dem Parameter (Freiform: Differenzenquotient).
    pub fn tangent(&self, t: f64) -> DVec3 {
        match self {
            Crv::Line { d, .. } => *d,
            Crv::Circle { a, r } => (a.y * t.cos() - a.x * t.sin()) * *r,
            Crv::Ellipse { a, r1, r2 } => a.y * (r2 * t.cos()) - a.x * (r1 * t.sin()),
            Crv::Spline(s) => {
                let (lo, hi) = s.domain();
                let t = if s.closed {
                    lo + (t - lo).rem_euclid(hi - lo)
                } else {
                    t.clamp(lo, hi)
                };
                let h = DIFF_STEP_REL * (hi - lo);
                let (ta, tb) = ((t - h).max(lo), (t + h).min(hi));
                (s.point(tb) - s.point(ta)) / (tb - ta)
            }
        }
    }

    /// Periode: Kreis und Ellipse `TAU`, geschlossene B-Splines die Breite des Bereichs.
    pub fn period(&self) -> Option<f64> {
        match self {
            Crv::Line { .. } => None,
            Crv::Circle { .. } | Crv::Ellipse { .. } => Some(TAU),
            Crv::Spline(s) => s.closed.then(|| {
                let (a, b) = s.domain();
                b - a
            }),
        }
    }

    /// Parameter des nächsten Kurvenpunkts.
    pub fn param(&self, q: DVec3) -> f64 {
        match self {
            Crv::Line { p, d } => (q - *p).dot(*d) / d.length_squared(),
            Crv::Circle { a, .. } => {
                let l = a.local(q);
                l.y.atan2(l.x)
            }
            Crv::Ellipse { a, r1, r2 } => {
                // Startwert: Winkel im auf einen Kreis gestreckten Raum (für Punkte auf der
                // Ellipse genau). Bei sehr schlanken Ellipsen vergrößert das Strecken Rundungs-
                // fehler um das Achsenverhältnis, daher Newton auf dem echten Abstand.
                let l = a.local(q);
                let mut t = (l.y / r2).atan2(l.x / r1);
                let mut best = self.point(t).distance(q);
                for _ in 0..ELLIPSE_NEWTON_STEPS {
                    let c = self.point(t) - q;
                    let d1 = self.tangent(t);
                    let d2 = -(c + q - a.origin);
                    let denom = d1.dot(d1) + c.dot(d2);
                    if denom <= 0.0 {
                        break;
                    }
                    let next = t - c.dot(d1) / denom;
                    let dist = self.point(next).distance(q);
                    if dist >= best {
                        break;
                    }
                    (t, best) = (next, dist);
                }
                t
            }
            Crv::Spline(s) => {
                let (lo, hi) = s.domain();
                let n = CURVE_SAMPLES_PER_CONTROL * s.control.len();
                let at = |i: usize| lo + (hi - lo) * i as f64 / n as f64;
                let best = (0..=n)
                    .min_by(|x, y| {
                        s.point(at(*x))
                            .distance(q)
                            .total_cmp(&s.point(at(*y)).distance(q))
                    })
                    .unwrap_or(0);
                let (mut a, mut b) = (at(best.saturating_sub(1)), at((best + 1).min(n)));
                for _ in 0..CURVE_REFINE_STEPS {
                    let m1 = a + (b - a) / 3.0;
                    let m2 = b - (b - a) / 3.0;
                    if s.point(m1).distance(q) < s.point(m2).distance(q) {
                        b = m2;
                    } else {
                        a = m1;
                    }
                }
                0.5 * (a + b)
            }
        }
    }

    /// Abstand zur Kurve (Ellipse: über den Winkel; für Punkte auf der Kurve genau).
    pub fn distance(&self, q: DVec3) -> f64 {
        match self {
            Crv::Circle { a, r } => {
                let l = a.local(q);
                ((l.x.hypot(l.y) - r).powi(2) + l.z * l.z).sqrt()
            }
            _ => self.point(self.param(q)).distance(q),
        }
    }
}

/// Trägerfläche einer Fläche.
#[derive(Clone, Debug, PartialEq)]
pub enum Surf {
    Plane(Axis),
    Cylinder {
        a: Axis,
        r: f64,
    },
    /// Radius `r` in der Ebene des Ursprungs, halber Öffnungswinkel `semi`.
    Cone {
        a: Axis,
        r: f64,
        semi: f64,
    },
    Sphere {
        a: Axis,
        r: f64,
    },
    Torus {
        a: Axis,
        major: f64,
        minor: f64,
    },
    Spline(Spline2),
}

impl Surf {
    /// Abstand zur Fläche (Freiform: über den nächsten Punkt).
    pub fn distance(&self, p: DVec3) -> f64 {
        match self {
            Surf::Plane(a) => a.local(p).z.abs(),
            Surf::Cylinder { a, r } => {
                let l = a.local(p);
                (l.x.hypot(l.y) - r).abs()
            }
            Surf::Cone { a, r, semi } => {
                let l = a.local(p);
                ((l.x.hypot(l.y) - (r + l.z * semi.tan())) * semi.cos()).abs()
            }
            Surf::Sphere { a, r } => ((p - a.origin).length() - r).abs(),
            Surf::Torus { a, major, minor } => {
                let l = a.local(p);
                ((l.x.hypot(l.y) - major).hypot(l.z) - minor).abs()
            }
            Surf::Spline(s) => s.closest(p).2,
        }
    }

    /// Fußpunkt (nächster Flächenpunkt) und natürliche Einheitsnormale dort (an Polen und
    /// Spitzen kann sie null sein). `hint` ist bei Freiformflächen Startwert und Ergebnis der
    /// Parametersuche (für benachbarte Punkte); bei analytischen Flächen bleibt er unberührt.
    pub fn foot(&self, p: DVec3, hint: &mut Option<(f64, f64)>) -> (DVec3, DVec3) {
        let radial = |a: &Axis, l: DVec3| (a.x * l.x + a.y * l.y).normalize_or_zero();
        match self {
            Surf::Plane(a) => (p - a.z * a.local(p).z, a.z),
            Surf::Cylinder { a, r } => {
                let l = a.local(p);
                let n = radial(a, l);
                (a.origin + n * *r + a.z * l.z, n)
            }
            Surf::Cone { a, r, semi } => {
                // Fußpunkt auf der Mantellinie in der Meridianebene; an der Spitze (auf der
                // Achse) keine Normale.
                let l = a.local(p);
                let rho = l.x.hypot(l.y);
                let scale = [p.x, p.y, p.z, a.origin.x, a.origin.y, a.origin.z]
                    .iter()
                    .fold(1.0_f64, |m, v| m.max(v.abs()));
                let on_axis = rho <= APEX_AXIS_EPS_FACTOR * f64::EPSILON * scale;
                let dir = if on_axis { DVec3::ZERO } else { radial(a, l) };
                let (s, c) = semi.sin_cos();
                let d = (rho - (r + l.z * s / c)) * c;
                let foot = a.origin + dir * (rho - d * c) + a.z * (l.z + d * s);
                let n = if dir == DVec3::ZERO {
                    DVec3::ZERO
                } else {
                    dir * c - a.z * s
                };
                (foot, n)
            }
            Surf::Sphere { a, r } => {
                let n = (p - a.origin).normalize_or_zero();
                (a.origin + n * *r, n)
            }
            Surf::Torus { a, major, minor } => {
                let center = a.origin + radial(a, a.local(p)) * *major;
                let n = (p - center).normalize_or_zero();
                (center + n * *minor, n)
            }
            Surf::Spline(s) => {
                let (u, v, _) = s.closest_near(p, *hint);
                *hint = Some((u, v));
                (s.point(u, v), s.normal(u, v))
            }
        }
    }

    /// Natürliche Einheitsnormale am nächsten Flächenpunkt.
    pub fn normal_at(&self, p: DVec3) -> DVec3 {
        self.foot(p, &mut None).1
    }

    /// Nächster Flächenpunkt.
    pub fn project(&self, p: DVec3) -> DVec3 {
        self.foot(p, &mut None).0
    }

    pub fn is_spline(&self) -> bool {
        matches!(self, Surf::Spline(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Oriented {
    /// Nummer des `EDGE_CURVE`.
    pub edge: u64,
    pub forward: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bound {
    pub outer: bool,
    pub orientation: bool,
    pub edges: Vec<Oriented>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FaceM {
    pub id: u64,
    pub surface: Surf,
    pub same_sense: bool,
    pub bounds: Vec<Bound>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShellM {
    pub id: u64,
    pub faces: Vec<FaceM>,
    /// `.F.` bei einem Hohlraum (`ORIENTED_CLOSED_SHELL`), sonst `.T.`.
    pub orientation: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SolidM {
    pub name: String,
    pub outer: ShellM,
    pub voids: Vec<ShellM>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Product {
    pub name: String,
    pub solids: Vec<SolidM>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeM {
    /// Nummern der `VERTEX_POINT`.
    pub start: u64,
    pub end: u64,
    pub curve: Crv,
    pub same_sense: bool,
}

/// Ganze Datei als Modell.
#[derive(Clone, Debug)]
pub struct Model {
    pub file: StepFile,
    /// Unsicherheit aus dem geometrischen Kontext (Millimeter).
    pub uncertainty: f64,
    pub products: Vec<Product>,
    pub edges: BTreeMap<u64, EdgeM>,
    pub vertices: BTreeMap<u64, DVec3>,
}

impl Model {
    /// Liest den Text und baut das Modell; jeder Fehler nennt den Eintrag.
    pub fn read(text: &str) -> Result<Model, String> {
        let file = parse(text)?;
        let mut m = Model {
            file,
            uncertainty: 0.0,
            products: Vec::new(),
            edges: BTreeMap::new(),
            vertices: BTreeMap::new(),
        };
        m.check_header()?;
        let context = m.context()?;
        for sdr in m.file.ids_of("SHAPE_DEFINITION_REPRESENTATION") {
            let product = m.product(sdr, context)?;
            m.products.push(product);
        }
        Ok(m)
    }

    fn check_header(&self) -> Result<(), String> {
        let schema = self.file.header[2].args[0].list()?;
        if schema.len() != 1 || !schema[0].text()?.starts_with("AUTOMOTIVE_DESIGN") {
            return Err(format!("Schema {schema:?}"));
        }
        Ok(())
    }

    /// Geometrischer Kontext: Millimeter, Radiant, Unsicherheit.
    fn context(&mut self) -> Result<u64, String> {
        let ids: Vec<u64> = self
            .file
            .entities
            .iter()
            .filter(|(_, e)| e.part("GEOMETRIC_REPRESENTATION_CONTEXT").is_some())
            .map(|(id, _)| *id)
            .collect();
        let [id] = ids[..] else {
            return Err(format!("{} geometrische Kontexte", ids.len()));
        };
        let e = self.file.get(id)?.clone();
        if e.part("GEOMETRIC_REPRESENTATION_CONTEXT").unwrap()[0].int()? != 3 {
            return Err("Kontext nicht dreidimensional".into());
        }
        let units = e
            .part("GLOBAL_UNIT_ASSIGNED_CONTEXT")
            .ok_or("Einheiten fehlen")?[0]
            .refs()?;
        let mut length = None;
        let mut angle = false;
        for u in units {
            let unit = self.file.get(u)?;
            let si = unit.part("SI_UNIT").ok_or("keine SI-Einheit")?;
            if unit.part("LENGTH_UNIT").is_some() {
                let milli = Value::Enum("MILLI".into());
                if si[0] != milli || si[1] != Value::Enum("METRE".into()) {
                    return Err("Länge nicht in Millimetern".into());
                }
                length = Some(u);
            }
            if unit.part("PLANE_ANGLE_UNIT").is_some() {
                if si[0] != Value::Unset || si[1] != Value::Enum("RADIAN".into()) {
                    return Err("Winkel nicht in Radiant".into());
                }
                angle = true;
            }
        }
        let length = length.ok_or("keine Längeneinheit")?;
        if !angle {
            return Err("keine Winkeleinheit".into());
        }
        let unc = e
            .part("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT")
            .ok_or("Unsicherheit fehlt")?[0]
            .refs()?;
        let [u] = unc[..] else {
            return Err("genau eine Unsicherheit erwartet".into());
        };
        let a = self.file.simple(u, "UNCERTAINTY_MEASURE_WITH_UNIT")?;
        match &a[0] {
            Value::Typed(name, v) if name == "LENGTH_MEASURE" => self.uncertainty = v[0].real()?,
            other => return Err(format!("Unsicherheit {other:?}")),
        }
        if a[1].reference()? != length {
            return Err("Unsicherheit nicht in der Längeneinheit".into());
        }
        if self.file.get(id)?.part("REPRESENTATION_CONTEXT").is_none() {
            return Err("REPRESENTATION_CONTEXT fehlt".into());
        }
        Ok(id)
    }

    /// Teil aus `SHAPE_DEFINITION_REPRESENTATION` über Definition bis zum Produkt.
    fn product(&mut self, sdr: u64, context: u64) -> Result<Product, String> {
        let a = self
            .file
            .simple(sdr, "SHAPE_DEFINITION_REPRESENTATION")?
            .to_vec();
        let pds = self
            .file
            .simple(a[0].reference()?, "PRODUCT_DEFINITION_SHAPE")?;
        let pd = self
            .file
            .simple(pds[2].reference()?, "PRODUCT_DEFINITION")?;
        let pdf = self
            .file
            .simple(pd[2].reference()?, "PRODUCT_DEFINITION_FORMATION")?;
        let prod = self.file.simple(pdf[2].reference()?, "PRODUCT")?;
        let name = prod[0].text()?.to_string();
        if prod[1].text()? != name {
            return Err("Kennung und Name des Produkts verschieden".into());
        }
        let shape = self
            .file
            .simple(a[1].reference()?, "ADVANCED_BREP_SHAPE_REPRESENTATION")?
            .to_vec();
        if shape[0].text()? != name {
            return Err("Name der Darstellung weicht ab".into());
        }
        if shape[2].reference()? != context {
            return Err("Darstellung ohne den geometrischen Kontext".into());
        }
        let mut solids = Vec::new();
        let mut placements = 0;
        for item in shape[1].refs()? {
            match self.file.name_of(item)? {
                Some("AXIS2_PLACEMENT_3D") => {
                    let ax = self.axis(item)?;
                    if ax.origin != DVec3::ZERO || ax.z != DVec3::Z || ax.x != DVec3::X {
                        return Err("Lage des Teils nicht im Ursprung".into());
                    }
                    placements += 1;
                }
                Some("MANIFOLD_SOLID_BREP") => {
                    let b = self.file.simple(item, "MANIFOLD_SOLID_BREP")?.to_vec();
                    solids.push(SolidM {
                        name: b[0].text()?.to_string(),
                        outer: self.shell(b[1].reference()?, true)?,
                        voids: Vec::new(),
                    });
                }
                Some("BREP_WITH_VOIDS") => {
                    let b = self.file.simple(item, "BREP_WITH_VOIDS")?.to_vec();
                    let mut voids = Vec::new();
                    for v in b[2].refs()? {
                        let o = self.file.simple(v, "ORIENTED_CLOSED_SHELL")?.to_vec();
                        if o[1] != Value::Derived {
                            return Err("ORIENTED_CLOSED_SHELL ohne *".into());
                        }
                        let orientation = o[3].logical()?;
                        if orientation {
                            return Err("Hohlraum ohne Umkehrung (.F.)".into());
                        }
                        // `.F.` kehrt alle Flächen um: hier gleich angewendet, damit die Flächen
                        // des Modells wie alle anderen nach außen (ins Hohlrauminnere) zeigen.
                        let mut shell = self.shell(o[2].reference()?, false)?;
                        for f in &mut shell.faces {
                            f.same_sense = !f.same_sense;
                            for b in &mut f.bounds {
                                b.orientation = !b.orientation;
                            }
                        }
                        voids.push(shell);
                    }
                    if voids.is_empty() {
                        return Err("BREP_WITH_VOIDS ohne Hohlraum".into());
                    }
                    solids.push(SolidM {
                        name: b[0].text()?.to_string(),
                        outer: self.shell(b[1].reference()?, true)?,
                        voids,
                    });
                }
                other => return Err(format!("unerwartetes Element {other:?}")),
            }
        }
        if placements != 1 || solids.is_empty() {
            return Err(format!("{placements} Lagen, {} Körper", solids.len()));
        }
        Ok(Product { name, solids })
    }

    fn shell(&mut self, id: u64, orientation: bool) -> Result<ShellM, String> {
        let a = self.file.simple(id, "CLOSED_SHELL")?.to_vec();
        let mut faces = Vec::new();
        for f in a[1].refs()? {
            faces.push(self.face(f)?);
        }
        Ok(ShellM {
            id,
            faces,
            orientation,
        })
    }

    fn face(&mut self, id: u64) -> Result<FaceM, String> {
        let a = self.file.simple(id, "ADVANCED_FACE")?.to_vec();
        let mut bounds = Vec::new();
        for b in a[1].refs()? {
            let (outer, args) = match self.file.name_of(b)? {
                Some("FACE_OUTER_BOUND") => (true, self.file.simple(b, "FACE_OUTER_BOUND")?),
                Some("FACE_BOUND") => (false, self.file.simple(b, "FACE_BOUND")?),
                other => return Err(format!("#{b}: Rand erwartet, gefunden {other:?}")),
            };
            let (lp, orientation) = (args[1].reference()?, args[2].logical()?);
            let l = self.file.simple(lp, "EDGE_LOOP")?.to_vec();
            let mut edges = Vec::new();
            for oe in l[1].refs()? {
                let o = self.file.simple(oe, "ORIENTED_EDGE")?.to_vec();
                if o[1] != Value::Derived || o[2] != Value::Derived {
                    return Err(format!("#{oe}: Anfang und Ende müssen * sein"));
                }
                let edge = o[3].reference()?;
                self.edge(edge)?;
                edges.push(Oriented {
                    edge,
                    forward: o[4].logical()?,
                });
            }
            if edges.is_empty() {
                return Err(format!("#{lp}: leere Schleife"));
            }
            bounds.push(Bound {
                outer,
                orientation,
                edges,
            });
        }
        if bounds.iter().filter(|b| b.outer).count() != 1 {
            return Err(format!("#{id}: genau ein äußerer Rand erwartet"));
        }
        Ok(FaceM {
            id,
            surface: self.surface(a[2].reference()?)?,
            same_sense: a[3].logical()?,
            bounds,
        })
    }

    fn edge(&mut self, id: u64) -> Result<(), String> {
        if self.edges.contains_key(&id) {
            return Ok(());
        }
        let a = self.file.simple(id, "EDGE_CURVE")?.to_vec();
        let start = self.vertex(a[1].reference()?)?;
        let end = self.vertex(a[2].reference()?)?;
        let curve = self.curve(a[3].reference()?)?;
        self.edges.insert(
            id,
            EdgeM {
                start,
                end,
                curve,
                same_sense: a[4].logical()?,
            },
        );
        Ok(())
    }

    fn vertex(&mut self, id: u64) -> Result<u64, String> {
        let a = self.file.simple(id, "VERTEX_POINT")?;
        let p = self.point(a[1].reference()?)?;
        self.vertices.insert(id, p);
        Ok(id)
    }

    pub fn point(&self, id: u64) -> Result<DVec3, String> {
        let a = self.file.simple(id, "CARTESIAN_POINT")?;
        let c = a[1].reals()?;
        let [x, y, z] = c[..] else {
            return Err(format!("#{id}: drei Koordinaten erwartet"));
        };
        Ok(DVec3::new(x, y, z))
    }

    /// Richtung, normiert bis [`DIR_TOL`].
    pub fn direction(&self, id: u64) -> Result<DVec3, String> {
        let a = self.file.simple(id, "DIRECTION")?;
        let c = a[1].reals()?;
        let [x, y, z] = c[..] else {
            return Err(format!("#{id}: drei Komponenten erwartet"));
        };
        let d = DVec3::new(x, y, z);
        if (d.length() - 1.0).abs() > DIR_TOL {
            return Err(format!("#{id}: Richtung nicht normiert ({})", d.length()));
        }
        Ok(d)
    }

    /// Koordinatensystem; Achse und Bezugsrichtung senkrecht bis [`ORTHO_TOL`].
    pub fn axis(&self, id: u64) -> Result<Axis, String> {
        let a = self.file.simple(id, "AXIS2_PLACEMENT_3D")?;
        let origin = self.point(a[1].reference()?)?;
        let z = self.direction(a[2].reference()?)?;
        let x = self.direction(a[3].reference()?)?;
        if z.dot(x).abs() > ORTHO_TOL {
            return Err(format!("#{id}: Achse und Bezugsrichtung nicht senkrecht"));
        }
        Ok(Axis {
            origin,
            x,
            y: z.cross(x),
            z,
        })
    }

    fn positive(v: &Value, what: &str) -> Result<f64, String> {
        let x = v.real()?;
        if x > 0.0 {
            Ok(x)
        } else {
            Err(format!("{what} nicht positiv: {x}"))
        }
    }

    fn curve(&self, id: u64) -> Result<Crv, String> {
        let e = self.file.get(id)?;
        if let Entity::Complex(_) = e {
            return Ok(Crv::Spline(self.spline_curve(id)?));
        }
        let name = self.file.name_of(id)?.unwrap_or_default();
        let a = self.file.simple(id, name)?;
        Ok(match name {
            "LINE" => {
                let p = self.point(a[1].reference()?)?;
                let v = self.file.simple(a[2].reference()?, "VECTOR")?;
                let d = self.direction(v[1].reference()?)?;
                Crv::Line {
                    p,
                    d: d * Model::positive(&v[2], "Betrag des Vektors")?,
                }
            }
            "CIRCLE" => Crv::Circle {
                a: self.axis(a[1].reference()?)?,
                r: Model::positive(&a[2], "Kreisradius")?,
            },
            "ELLIPSE" => Crv::Ellipse {
                a: self.axis(a[1].reference()?)?,
                r1: Model::positive(&a[2], "Halbachse 1")?,
                r2: Model::positive(&a[3], "Halbachse 2")?,
            },
            "B_SPLINE_CURVE_WITH_KNOTS" => Crv::Spline(self.spline_curve(id)?),
            other => return Err(format!("#{id}: Kurvenart {other} unbekannt")),
        })
    }

    /// `B_SPLINE_CURVE_WITH_KNOTS` einfach oder zusammengesetzt mit `RATIONAL_B_SPLINE_CURVE`.
    fn spline_curve(&self, id: u64) -> Result<Spline1, String> {
        let (degree, cps, form, closed, si, mults, values, spec, rational) =
            match self.file.get(id)? {
                Entity::Simple(r) if r.name == "B_SPLINE_CURVE_WITH_KNOTS" => {
                    let a = &r.args;
                    (&a[1], &a[2], &a[3], &a[4], &a[5], &a[6], &a[7], &a[8], None)
                }
                e @ Entity::Complex(_) => {
                    let want = [
                        "BOUNDED_CURVE",
                        "B_SPLINE_CURVE",
                        "B_SPLINE_CURVE_WITH_KNOTS",
                        "CURVE",
                        "GEOMETRIC_REPRESENTATION_ITEM",
                        "RATIONAL_B_SPLINE_CURVE",
                        "REPRESENTATION_ITEM",
                    ];
                    if e.names() != want {
                        return Err(format!("#{id}: Teile {:?}", e.names()));
                    }
                    let b = e.part("B_SPLINE_CURVE").unwrap();
                    let k = e.part("B_SPLINE_CURVE_WITH_KNOTS").unwrap();
                    let w = e.part("RATIONAL_B_SPLINE_CURVE").unwrap();
                    (
                        &b[0],
                        &b[1],
                        &b[2],
                        &b[3],
                        &b[4],
                        &k[0],
                        &k[1],
                        &k[2],
                        Some(&w[0]),
                    )
                }
                other => {
                    return Err(format!(
                        "#{id}: B-Spline erwartet, gefunden {:?}",
                        other.names()
                    ));
                }
            };
        if form.enumeration()? != "UNSPECIFIED" || si.logical()? {
            return Err(format!("#{id}: Form oder Selbstschnitt unerwartet"));
        }
        if spec.enumeration()? != "UNSPECIFIED" {
            return Err(format!("#{id}: Knotenart nicht UNSPECIFIED"));
        }
        let degree = usize::try_from(degree.int()?).map_err(|e| e.to_string())?;
        let control: Vec<DVec3> = cps
            .refs()?
            .into_iter()
            .map(|c| self.point(c))
            .collect::<Result<_, _>>()?;
        let mults = mults.ints()?;
        let knots = expand_knots(&mults, &values.reals()?)?;
        check_knots(degree, control.len(), &mults, &knots)?;
        let weights = match rational {
            Some(w) => {
                let w = w.reals()?;
                if w.len() != control.len() || w.iter().any(|x| *x <= 0.0) {
                    return Err(format!("#{id}: Gewichte ungültig"));
                }
                w
            }
            None => vec![1.0; control.len()],
        };
        Ok(Spline1 {
            degree,
            knots,
            control,
            weights,
            closed: closed.logical()?,
        })
    }

    fn surface(&self, id: u64) -> Result<Surf, String> {
        if let Entity::Complex(_) = self.file.get(id)? {
            return Ok(Surf::Spline(self.spline_surface(id)?));
        }
        let name = self.file.name_of(id)?.unwrap_or_default();
        let a = self.file.simple(id, name)?;
        Ok(match name {
            "PLANE" => Surf::Plane(self.axis(a[1].reference()?)?),
            "CYLINDRICAL_SURFACE" => Surf::Cylinder {
                a: self.axis(a[1].reference()?)?,
                r: Model::positive(&a[2], "Zylinderradius")?,
            },
            "CONICAL_SURFACE" => {
                let r = a[2].real()?;
                let semi = a[3].real()?;
                if r < 0.0 {
                    return Err(format!("#{id}: Kegelradius negativ"));
                }
                if !(semi > 0.0 && semi < PI / 2.0) {
                    return Err(format!("#{id}: Öffnungswinkel {semi} außerhalb (0, π/2)"));
                }
                Surf::Cone {
                    a: self.axis(a[1].reference()?)?,
                    r,
                    semi,
                }
            }
            "SPHERICAL_SURFACE" => Surf::Sphere {
                a: self.axis(a[1].reference()?)?,
                r: Model::positive(&a[2], "Kugelradius")?,
            },
            "TOROIDAL_SURFACE" => {
                let major = Model::positive(&a[2], "großer Torusradius")?;
                let minor = Model::positive(&a[3], "kleiner Torusradius")?;
                if minor >= major {
                    return Err(format!(
                        "#{id}: Torus mit kleinem Radius {minor} nicht kleiner als dem großen {major} (Spindeltorus)"
                    ));
                }
                Surf::Torus {
                    a: self.axis(a[1].reference()?)?,
                    major,
                    minor,
                }
            }
            "B_SPLINE_SURFACE_WITH_KNOTS" => Surf::Spline(self.spline_surface(id)?),
            other => return Err(format!("#{id}: Flächenart {other} unbekannt")),
        })
    }

    /// `B_SPLINE_SURFACE_WITH_KNOTS` einfach oder zusammengesetzt mit
    /// `RATIONAL_B_SPLINE_SURFACE`.
    fn spline_surface(&self, id: u64) -> Result<Spline2, String> {
        type Parts<'v> = (&'v [Value], &'v [Value], Option<&'v Value>);
        let (b, k, w): Parts = match self.file.get(id)? {
            Entity::Simple(r) if r.name == "B_SPLINE_SURFACE_WITH_KNOTS" => {
                (&r.args[1..8], &r.args[8..13], None)
            }
            e @ Entity::Complex(_) => {
                let want = [
                    "BOUNDED_SURFACE",
                    "B_SPLINE_SURFACE",
                    "B_SPLINE_SURFACE_WITH_KNOTS",
                    "GEOMETRIC_REPRESENTATION_ITEM",
                    "RATIONAL_B_SPLINE_SURFACE",
                    "REPRESENTATION_ITEM",
                    "SURFACE",
                ];
                if e.names() != want {
                    return Err(format!("#{id}: Teile {:?}", e.names()));
                }
                (
                    e.part("B_SPLINE_SURFACE").unwrap(),
                    e.part("B_SPLINE_SURFACE_WITH_KNOTS").unwrap(),
                    Some(&e.part("RATIONAL_B_SPLINE_SURFACE").unwrap()[0]),
                )
            }
            other => {
                return Err(format!(
                    "#{id}: B-Spline-Fläche erwartet, gefunden {:?}",
                    other.names()
                ));
            }
        };
        if b.len() != 7 || k.len() != 5 {
            return Err(format!("#{id}: falsche Zahl der Parameter"));
        }
        if b[3].enumeration()? != "UNSPECIFIED" || b[6].logical()? {
            return Err(format!("#{id}: Form oder Selbstschnitt unerwartet"));
        }
        if k[4].enumeration()? != "UNSPECIFIED" {
            return Err(format!("#{id}: Knotenart nicht UNSPECIFIED"));
        }
        let du = usize::try_from(b[0].int()?).map_err(|e| e.to_string())?;
        let dv = usize::try_from(b[1].int()?).map_err(|e| e.to_string())?;
        let mut control = Vec::new();
        for row in b[2].list()? {
            let pts: Vec<DVec3> = row
                .refs()?
                .into_iter()
                .map(|c| self.point(c))
                .collect::<Result<_, _>>()?;
            control.push(pts);
        }
        let nv = control.first().map_or(0, Vec::len);
        if control.len() < 2 || nv < 2 || control.iter().any(|r| r.len() != nv) {
            return Err(format!("#{id}: Kontrollpunktnetz nicht rechteckig"));
        }
        let (mu, mv) = (k[0].ints()?, k[1].ints()?);
        let ku = expand_knots(&mu, &k[2].reals()?)?;
        let kv = expand_knots(&mv, &k[3].reals()?)?;
        check_knots(du, control.len(), &mu, &ku)?;
        check_knots(dv, nv, &mv, &kv)?;
        let weights = match w {
            Some(w) => {
                let mut rows = Vec::new();
                for row in w.list()? {
                    rows.push(row.reals()?);
                }
                if rows.len() != control.len()
                    || rows
                        .iter()
                        .any(|r| r.len() != nv || r.iter().any(|x| *x <= 0.0))
                {
                    return Err(format!("#{id}: Gewichte ungültig"));
                }
                rows
            }
            None => vec![vec![1.0; nv]; control.len()],
        };
        Ok(Spline2 {
            du,
            dv,
            ku,
            kv,
            control,
            weights,
            u_closed: b[4].logical()?,
            v_closed: b[5].logical()?,
        })
    }

    /// Alle Hüllen aller Teile (Körper für Körper, außen zuerst).
    pub fn shells(&self) -> Vec<&ShellM> {
        self.products
            .iter()
            .flat_map(|p| p.solids.iter())
            .flat_map(|s| std::iter::once(&s.outer).chain(s.voids.iter()))
            .collect()
    }

    /// Anfangs- und Endpunkt einer orientierten Kante in Laufrichtung.
    pub fn ends(&self, o: &Oriented) -> (u64, u64) {
        let e = &self.edges[&o.edge];
        if o.forward {
            (e.start, e.end)
        } else {
            (e.end, e.start)
        }
    }
}
