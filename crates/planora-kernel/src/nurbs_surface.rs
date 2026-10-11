//! Kernel-Daten einer NURBS-Fläche: die Fläche selbst, Kennzeichen für geschlossene Richtungen
//! (Naht wie beim Zylinder), erkannte Pole und ein beim ersten Gebrauch aufgebautes Raster aus
//! Stützpunkten für die Suche nach dem nächsten Punkt.

use crate::error::KernelError;
use planora_math::nurbs::{MAX_CONTROL_TOTAL, NurbsSurface};
use planora_math::{DVec3, Frame};
use std::sync::OnceLock;

/// Zwei Kontrollpunkte, die höchstens so weit (mm) auseinander liegen, gelten als derselbe Punkt
/// (Nahtprüfung periodischer Flächen, Erkennung zusammengefallener Kontrollpunktreihen).
const SAME_POINT_TOL: f64 = 1e-9;
/// Zwei Gewichte, deren relativer Unterschied höchstens so groß ist, gelten als gleich.
const SAME_WEIGHT_RATIO: f64 = 1e-9;
/// Rasterpunkte je Kontrollpunkt und Richtung.
const GRID_PER_CONTROL: usize = 4;
/// Kleinste Zahl Rasterpunkte je Richtung.
const GRID_MIN: usize = 16;
/// Höchstzahl Rasterpunkte insgesamt (begrenzt Speicher und Suchaufwand).
const GRID_MAX_POINTS: usize = 250_000;
/// Zahl der besten örtlichen Rasterminima, von denen die Newton-Suche startet.
const NEWTON_STARTS: usize = 3;
/// Höchstzahl Newton-Schritte je Start.
const NEWTON_STEPS: usize = 40;
/// Newton endet, wenn der Schritt kleiner als dieser Bruchteil des Parameterbereichs ist.
const STEP_RATIO: f64 = 1e-12;
/// Größter Newton-Schritt je Richtung als Bruchteil des Parameterbereichs (Dämpfung).
const MAX_STEP_FRACTION: f64 = 0.5;
/// Höchstzahl Halbierungen des Schritts, bis der Abstand sinkt.
const MAX_HALVINGS: usize = 30;
/// Regularisierung der Hesse-Matrix als Bruchteil ihrer Spur (Pole, flache Richtungen).
const HESSIAN_REGULARIZATION: f64 = 1e-12;
/// Untergrenze der Regularisierung (Spur null).
const HESSIAN_FLOOR: f64 = 1e-300;
/// Versätze (Bruchteil des Wegs zur Bereichsmitte), an denen die Ersatznormale eines entarteten
/// Punkts (Pol) gesucht wird.
const NORMAL_OFFSETS: [f64; 3] = [1e-6, 1e-4, 1e-2];
/// Ergebnis von `signed_distance` für nicht endliche Punkte (sehr weit weg, nie NaN).
const FAR_AWAY: f64 = f64::MAX;

/// Raster aus Stützpunkten mit Position und Normale, Index `i * nv + j`.
struct Grid {
    nu: usize,
    nv: usize,
    us: Vec<f64>,
    vs: Vec<f64>,
    points: Vec<DVec3>,
    normals: Vec<DVec3>,
}

/// NURBS-Fläche im Kernel (siehe Modulbeschreibung). Wird über `Arc` geteilt.
pub struct NurbsSurfaceData {
    surface: NurbsSurface,
    frame: Frame,
    periodic_u: bool,
    periodic_v: bool,
    poles_v: Vec<f64>,
    grid: OnceLock<Grid>,
}

impl std::fmt::Debug for NurbsSurfaceData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NurbsSurfaceData")
            .field("surface", &self.surface)
            .field("periodic_u", &self.periodic_u)
            .field("periodic_v", &self.periodic_v)
            .field("poles_v", &self.poles_v)
            .finish_non_exhaustive()
    }
}

fn same_point(a: DVec3, b: DVec3) -> bool {
    a.distance(b) <= SAME_POINT_TOL
}

fn same_weight(a: f64, b: f64) -> bool {
    (a - b).abs() <= SAME_WEIGHT_RATIO * a.abs().max(b.abs())
}

impl NurbsSurfaceData {
    /// Prüft die Kennzeichen und erkennt die Pole:
    /// - `periodic_u` nur, wenn erste und letzte Kontrollpunktreihe in u übereinstimmen
    ///   (Punkte und Gewichte), analog `periodic_v`;
    /// - eine in u zusammengefallene Kontrollpunktreihe (Pol) nur am Rand `v0` oder `v1`;
    /// - keine in v zusammengefallene Reihe (Pole in u-Richtung werden nicht unterstützt);
    /// - höchstens `MAX_CONTROL_TOTAL` Kontrollpunkte.
    pub fn new(
        surface: NurbsSurface,
        periodic_u: bool,
        periodic_v: bool,
    ) -> Result<Self, KernelError> {
        let (nu, nv) = surface.counts();
        if nu.saturating_mul(nv) > MAX_CONTROL_TOTAL {
            return Err(KernelError::InvalidSurface(format!(
                "zu viele Kontrollpunkte (höchstens {MAX_CONTROL_TOTAL})"
            )));
        }
        let same = |a: (usize, usize), b: (usize, usize)| {
            same_point(surface.control_at(a.0, a.1), surface.control_at(b.0, b.1))
                && same_weight(surface.weight_at(a.0, a.1), surface.weight_at(b.0, b.1))
        };
        if periodic_u && !(0..nv).all(|j| same((0, j), (nu - 1, j))) {
            return Err(KernelError::InvalidSurface(
                "die Fläche ist in u nicht geschlossen (erste und letzte Kontrollpunktreihe verschieden)"
                    .into(),
            ));
        }
        if periodic_v && !(0..nu).all(|i| same((i, 0), (i, nv - 1))) {
            return Err(KernelError::InvalidSurface(
                "die Fläche ist in v nicht geschlossen (erste und letzte Kontrollpunktreihe verschieden)"
                    .into(),
            ));
        }
        // Reihe mit festem u-Index zusammengefallen: Pol in u-Richtung.
        let column_collapsed = |i: usize| {
            (1..nv).all(|j| same_point(surface.control_at(i, j), surface.control_at(i, 0)))
        };
        if (0..nu).any(column_collapsed) {
            return Err(KernelError::InvalidSurface(
                "eine Linie in v-Richtung fällt zu einem Punkt zusammen (Pol in u-Richtung wird nicht unterstützt)"
                    .into(),
            ));
        }
        let (_, (v0, v1)) = surface.domain();
        let mut poles_v = Vec::new();
        for j in 0..nv {
            let collapsed =
                (1..nu).all(|i| same_point(surface.control_at(i, j), surface.control_at(0, j)));
            if !collapsed {
                continue;
            }
            if j == 0 {
                poles_v.push(v0);
            } else if j == nv - 1 {
                poles_v.push(v1);
            } else {
                return Err(KernelError::InvalidSurface(
                    "eine innere Kontrollpunktreihe fällt zu einem Punkt zusammen (innerer Pol)"
                        .into(),
                ));
            }
        }
        let mut data = NurbsSurfaceData {
            surface,
            frame: Frame::WORLD,
            periodic_u,
            periodic_v,
            poles_v,
            grid: OnceLock::new(),
        };
        let (lo, hi) = data.control_bounds();
        data.frame.origin = (lo + hi) * 0.5;
        Ok(data)
    }

    pub fn surface(&self) -> &NurbsSurface {
        &self.surface
    }

    /// Referenzsystem (Mitte des Hüllquaders der Kontrollpunkte, Weltachsen); ohne geometrische
    /// Bedeutung.
    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    pub fn is_periodic_u(&self) -> bool {
        self.periodic_u
    }

    pub fn is_periodic_v(&self) -> bool {
        self.periodic_v
    }

    /// v-Werte, an denen die u-Linie zu einem Punkt zusammenfällt (an den Rändern erkannt).
    pub fn poles_v(&self) -> &[f64] {
        &self.poles_v
    }

    /// Hüllquader der Kontrollpunkte (enthält die ganze Fläche).
    pub fn control_bounds(&self) -> (DVec3, DVec3) {
        let (nu, nv) = self.surface.counts();
        (0..nu)
            .flat_map(|i| (0..nv).map(move |j| (i, j)))
            .map(|(i, j)| self.surface.control_at(i, j))
            .fold(
                (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
                |(lo, hi), p| (lo.min(p), hi.max(p)),
            )
    }

    /// Dieselben Kennzeichen für die abgebildete Fläche; die Prüfungen laufen erneut.
    pub fn transformed(&self, f: impl Fn(DVec3) -> DVec3) -> Result<Self, KernelError> {
        let s = self
            .surface
            .transformed(f)
            .map_err(|e| KernelError::InvalidSurface(e.to_string()))?;
        NurbsSurfaceData::new(s, self.periodic_u, self.periodic_v)
    }

    /// Faltet Parameter geschlossener Richtungen in den Bereich (nur außerhalb des Bereichs,
    /// damit die Ränder selbst erhalten bleiben).
    pub fn wrap(&self, u: f64, v: f64) -> (f64, f64) {
        let ((u0, u1), (v0, v1)) = self.surface.domain();
        let fold = |x: f64, a: f64, b: f64, periodic: bool| {
            if periodic && x.is_finite() && (x < a || x > b) {
                a + (x - a).rem_euclid(b - a)
            } else {
                x
            }
        };
        (
            fold(u, u0, u1, self.periodic_u),
            fold(v, v0, v1, self.periodic_v),
        )
    }

    pub fn point(&self, u: f64, v: f64) -> DVec3 {
        let (u, v) = self.wrap(u, v);
        self.surface.point(u, v)
    }

    /// Einheitsnormale `S_u × S_v`; an entarteten Stellen (Pol) die Normale eines leicht zur
    /// Bereichsmitte versetzten Punkts. `DVec3::ZERO` nur, wenn auch dort keine existiert.
    pub fn normal(&self, u: f64, v: f64) -> DVec3 {
        let (u, v) = self.wrap(u, v);
        let n = self.surface.normal(u, v);
        if n != DVec3::ZERO {
            return n;
        }
        let ((u0, u1), (v0, v1)) = self.surface.domain();
        let (u, v) = (
            if u.is_finite() { u.clamp(u0, u1) } else { u0 },
            if v.is_finite() { v.clamp(v0, v1) } else { v0 },
        );
        let (uc, vc) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
        NORMAL_OFFSETS
            .iter()
            .map(|f| self.surface.normal(u + (uc - u) * f, v + (vc - v) * f))
            .find(|n| *n != DVec3::ZERO)
            .unwrap_or(DVec3::ZERO)
    }

    fn grid(&self) -> &Grid {
        self.grid.get_or_init(|| {
            let (cu, cv) = self.surface.counts();
            let mut nu = (GRID_PER_CONTROL * cu).max(GRID_MIN);
            let mut nv = (GRID_PER_CONTROL * cv).max(GRID_MIN);
            if nu * nv > GRID_MAX_POINTS {
                let f = (GRID_MAX_POINTS as f64 / (nu * nv) as f64).sqrt();
                nu = ((nu as f64 * f) as usize).max(2);
                nv = ((nv as f64 * f) as usize).max(2);
                nv = nv.min(GRID_MAX_POINTS / nu);
            }
            let ((u0, u1), (v0, v1)) = self.surface.domain();
            let at = |a: f64, b: f64, k: usize, n: usize| a + (b - a) * k as f64 / (n - 1) as f64;
            let us: Vec<f64> = (0..nu).map(|i| at(u0, u1, i, nu)).collect();
            let vs: Vec<f64> = (0..nv).map(|j| at(v0, v1, j, nv)).collect();
            let mut points = Vec::with_capacity(nu * nv);
            let mut normals = Vec::with_capacity(nu * nv);
            for &u in &us {
                for &v in &vs {
                    points.push(self.surface.point(u, v));
                    normals.push(self.normal(u, v));
                }
            }
            Grid {
                nu,
                nv,
                us,
                vs,
                points,
                normals,
            }
        })
    }

    /// Nächster Rasterpunkt: (Index, Abstand).
    fn nearest_grid_point(&self, p: DVec3) -> (usize, f64) {
        let g = self.grid();
        g.points
            .iter()
            .enumerate()
            .map(|(k, q)| (k, q.distance_squared(p)))
            .fold((0, f64::INFINITY), |a, b| if b.1 < a.1 { b } else { a })
    }

    /// Örtliche Minima des Abstands über dem Raster (8er-Nachbarschaft, über die Naht hinweg bei
    /// geschlossenen Richtungen), die besten `NEWTON_STARTS` als Parameterpaare.
    fn grid_starts(&self, p: DVec3) -> Vec<(f64, f64)> {
        let g = self.grid();
        let d: Vec<f64> = g.points.iter().map(|q| q.distance_squared(p)).collect();
        let neighbour = |k: isize, n: usize, periodic: bool| -> Option<usize> {
            if (0..n as isize).contains(&k) {
                Some(k as usize)
            } else if periodic {
                // Erster und letzter Rasterpunkt fallen zusammen: über die Naht hinweg den
                // vorletzten bzw. zweiten nehmen.
                Some(if k < 0 { n - 2 } else { 1 })
            } else {
                None
            }
        };
        let mut minima: Vec<usize> = Vec::new();
        for i in 0..g.nu {
            for j in 0..g.nv {
                let here = d[i * g.nv + j];
                let mut is_min = true;
                'n: for di in -1isize..=1 {
                    for dj in -1isize..=1 {
                        if di == 0 && dj == 0 {
                            continue;
                        }
                        let (Some(a), Some(b)) = (
                            neighbour(i as isize + di, g.nu, self.periodic_u),
                            neighbour(j as isize + dj, g.nv, self.periodic_v),
                        ) else {
                            continue;
                        };
                        if d[a * g.nv + b] < here {
                            is_min = false;
                            break 'n;
                        }
                    }
                }
                if is_min {
                    minima.push(i * g.nv + j);
                }
            }
        }
        minima.sort_by(|a, b| d[*a].total_cmp(&d[*b]));
        // Gleich gute Nachbarn (flache Minima) nicht mehrfach starten.
        minima.dedup_by(|a, b| d[*a] == d[*b]);
        minima
            .into_iter()
            .take(NEWTON_STARTS)
            .map(|k| (g.us[k / g.nv], g.vs[k % g.nv]))
            .collect()
    }

    /// Nächster Punkt: `(u, v, Abstand)`. `hint` = Startwert; ohne Startwert (oder wenn die
    /// Suche vom Startwert aus nicht konvergiert) Rastersuche mit den besten drei örtlichen
    /// Rasterminima. `None`, wenn keine Suche konvergiert oder `p` nicht endlich ist.
    pub fn closest(&self, p: DVec3, hint: Option<(f64, f64)>) -> Option<(f64, f64, f64)> {
        if !p.is_finite() {
            return None;
        }
        if let Some((u, v)) = hint.filter(|(u, v)| u.is_finite() && v.is_finite()) {
            let (u, v) = self.clamp(self.wrap(u, v));
            if let Some(r) = self.newton(p, u, v) {
                return Some(r);
            }
        }
        self.grid_starts(p)
            .into_iter()
            .filter_map(|(u, v)| self.newton(p, u, v))
            .min_by(|a, b| a.2.total_cmp(&b.2))
    }

    /// `(u, v)` des nächsten Punkts; bei Nichtkonvergenz die Bereichsmitte (Aufrufer prüfen den
    /// Abstand selbst).
    pub fn project(&self, p: DVec3) -> (f64, f64) {
        self.project_near_opt(p, None)
    }

    /// Wie `project`, mit Startwert.
    pub fn project_near(&self, p: DVec3, hint: (f64, f64)) -> (f64, f64) {
        self.project_near_opt(p, Some(hint))
    }

    fn project_near_opt(&self, p: DVec3, hint: Option<(f64, f64)>) -> (f64, f64) {
        match self.closest(p, hint) {
            Some((u, v, _)) => (u, v),
            None => {
                let ((u0, u1), (v0, v1)) = self.surface.domain();
                (0.5 * (u0 + u1), 0.5 * (v0 + v1))
            }
        }
    }

    /// Vorzeichenbehafteter Abstand: Abstand zum nächsten Punkt `S`, Vorzeichen aus
    /// `(p − S) · n` (bei `n = 0` positiv). Konvergiert die Suche nicht, gilt der nächste
    /// Rasterpunkt mit dessen Normale. Nie NaN: nicht endliche Punkte ergeben `f64::MAX`.
    pub fn signed_distance(&self, p: DVec3) -> f64 {
        if !p.is_finite() {
            return FAR_AWAY;
        }
        let (q, n, d) = match self.closest(p, None) {
            Some((u, v, d)) => (self.surface.point(u, v), self.normal(u, v), d),
            None => {
                let (k, d2) = self.nearest_grid_point(p);
                let g = self.grid();
                (g.points[k], g.normals[k], d2.sqrt())
            }
        };
        if !d.is_finite() {
            return FAR_AWAY;
        }
        if (p - q).dot(n) < 0.0 { -d } else { d }
    }

    fn clamp(&self, (u, v): (f64, f64)) -> (f64, f64) {
        let ((u0, u1), (v0, v1)) = self.surface.domain();
        (u.clamp(u0, u1), v.clamp(v0, v1))
    }

    /// Gedämpfte Newton-Suche nach dem nächsten Punkt von `(u, v)` aus (Bereichsbegrenzung in
    /// offenen Richtungen, Umlauf in geschlossenen). `Some((u, v, Abstand))` bei Konvergenz:
    /// Schritt kleiner als `STEP_RATIO` · Bereich oder kein Abstieg mehr möglich (Minimum bis auf
    /// Rundung erreicht).
    fn newton(&self, p: DVec3, mut u: f64, mut v: f64) -> Option<(f64, f64, f64)> {
        let ((u0, u1), (v0, v1)) = self.surface.domain();
        let (ru, rv) = (u1 - u0, v1 - v0);
        let dist2 = |u: f64, v: f64| self.surface.point(u, v).distance_squared(p);
        let mut f = dist2(u, v);
        for _ in 0..NEWTON_STEPS {
            let d = self.surface.derivs(u, v, 2);
            let r = d[0][0] - p;
            let (su, sv) = (d[1][0], d[0][1]);
            let (suu, suv, svv) = (d[2][0], d[1][1], d[0][2]);
            let g = [r.dot(su), r.dot(sv)];
            let mut h = [
                [su.dot(su) + r.dot(suu), su.dot(sv) + r.dot(suv)],
                [0.0, sv.dot(sv) + r.dot(svv)],
            ];
            h[1][0] = h[0][1];
            let det = h[0][0] * h[1][1] - h[0][1] * h[1][0];
            if !(h[0][0] > 0.0 && det > 0.0) {
                // Nicht positiv definit: Gauß-Newton (immer positiv semidefinit).
                h = [[su.dot(su), su.dot(sv)], [su.dot(sv), sv.dot(sv)]];
            }
            let reg = (HESSIAN_REGULARIZATION * (h[0][0] + h[1][1])).max(HESSIAN_FLOOR);
            h[0][0] += reg;
            h[1][1] += reg;
            // Offene Richtung am Rand, Schritt nach außen: diese Richtung festhalten.
            let solve = |h: &[[f64; 2]; 2], fix_u: bool, fix_v: bool| -> (f64, f64) {
                match (fix_u, fix_v) {
                    (true, true) => (0.0, 0.0),
                    (true, false) => (0.0, -g[1] / h[1][1]),
                    (false, true) => (-g[0] / h[0][0], 0.0),
                    (false, false) => {
                        let det = h[0][0] * h[1][1] - h[0][1] * h[1][0];
                        (
                            -(h[1][1] * g[0] - h[0][1] * g[1]) / det,
                            -(h[0][0] * g[1] - h[1][0] * g[0]) / det,
                        )
                    }
                }
            };
            let blocked = |x: f64, dx: f64, a: f64, b: f64, periodic: bool| {
                !periodic && ((x <= a && dx < 0.0) || (x >= b && dx > 0.0))
            };
            let (mut du, mut dv) = solve(&h, false, false);
            let (bu, bv) = (
                blocked(u, du, u0, u1, self.periodic_u),
                blocked(v, dv, v0, v1, self.periodic_v),
            );
            if bu || bv {
                (du, dv) = solve(&h, bu, bv);
            }
            if !du.is_finite() || !dv.is_finite() {
                return None;
            }
            let scale = 1.0_f64
                .min(MAX_STEP_FRACTION * ru / du.abs().max(f64::MIN_POSITIVE))
                .min(MAX_STEP_FRACTION * rv / dv.abs().max(f64::MIN_POSITIVE));
            let (du, dv) = (du * scale, dv * scale);
            let mut t = 1.0;
            let mut next = None;
            for _ in 0..MAX_HALVINGS {
                let (nu, nv) = self.clamp(self.wrap(u + du * t, v + dv * t));
                let fnew = dist2(nu, nv);
                if fnew < f {
                    next = Some((nu, nv, fnew));
                    break;
                }
                t *= 0.5;
            }
            let Some((nu, nv, fnew)) = next else {
                // Kein Abstieg mehr möglich: Minimum bis auf Rundung erreicht.
                return Some((u, v, f.sqrt()));
            };
            let step_u = periodic_delta(nu - u, ru, self.periodic_u).abs() / ru;
            let step_v = periodic_delta(nv - v, rv, self.periodic_v).abs() / rv;
            (u, v, f) = (nu, nv, fnew);
            if step_u.max(step_v) < STEP_RATIO {
                return Some((u, v, f.sqrt()));
            }
        }
        None
    }
}

/// Kürzeste Differenz über die Naht hinweg (in geschlossenen Richtungen).
fn periodic_delta(d: f64, period: f64, periodic: bool) -> f64 {
    if periodic {
        d - period * (d / period).round()
    } else {
        d
    }
}
