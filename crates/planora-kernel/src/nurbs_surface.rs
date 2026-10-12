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
/// Newton endet, wenn der Schritt kleiner als dieser Bruchteil des Parameterbereichs ist
/// (oder kleiner als die Auflösung des Parameters, siehe `PARAM_RESOLUTION_ULPS`).
const STEP_RATIO: f64 = 1e-12;
/// Auflösung eines Parameterwerts in Vielfachen von `f64::EPSILON · |Wert|`: Liegt der Bereich
/// weit von 0 (etwa `[1e5, 1e5 + 1]`), ist ein Schritt von `STEP_RATIO` · Bereich in `f64` gar
/// nicht darstellbar; dann gilt die Auflösung als Schrittgrenze und als Rest beim Gradienten.
const PARAM_RESOLUTION_ULPS: f64 = 4.0;
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
/// Rasterpunkte je Kachel und Richtung (Kacheln mit Hüllquader beschleunigen die Rastersuche).
const TILE: usize = 8;
/// Stationarität einer freien Richtung: `|r · S_i| ≤ |S_i| · (ORTHO_TOL · |r| + ROUND_TOL · Größe)`
/// (Kosinus zwischen Abstandsvektor und Tangente; der zweite Term deckt Rundung bei Punkten auf
/// der Fläche ab, `Größe` = Betrag der beteiligten Koordinaten).
const ORTHO_TOL: f64 = 1e-6;
const ROUND_TOL: f64 = 1e-12;
/// Ein Startwert-Ergebnis gilt als nicht global, wenn ein Rasterpunkt um mehr als diesen
/// Bruchteil (des Abstandsquadrats) näher liegt.
const VERIFY_RATIO: f64 = 1e-9;

/// Parameterpaar `(u, v)`.
type Uv = (f64, f64);

/// Kachel des Rasters `(i0, i1, j0, j1, lo, hi)`: Indexbereiche `[i0, i1) × [j0, j1)` und der
/// Hüllquader ihrer Rasterpunkte.
type Tile = (usize, usize, usize, usize, DVec3, DVec3);

/// Raster aus Stützpunkten mit Position und Normale, Index `i * nv + j`, dazu Kacheln mit dem
/// Hüllquader ihrer Rasterpunkte.
struct Grid {
    nu: usize,
    nv: usize,
    us: Vec<f64>,
    vs: Vec<f64>,
    points: Vec<DVec3>,
    normals: Vec<DVec3>,
    tiles: Vec<Tile>,
    /// Größte Diagonale einer Rasterzelle: jeder Flächenpunkt liegt höchstens so weit von einem
    /// Eckpunkt seiner Zelle entfernt (bis auf die Wölbung innerhalb der Zelle).
    cell_reach: f64,
}

/// Quadrat des Abstands von `p` zum Quader `[lo, hi]` (untere Schranke für alle Punkte darin).
fn box_distance2(p: DVec3, lo: DVec3, hi: DVec3) -> f64 {
    let d = (lo - p).max(DVec3::ZERO).max(p - hi);
    d.length_squared()
}

/// NURBS-Fläche im Kernel (siehe Modulbeschreibung). Wird über `Arc` geteilt.
///
/// Die natürliche Normale `S_u × S_v` folgt allein der Parametrisierung. Bei einer Drehfläche aus
/// `NurbsSurface::revolve` (u = Winkel gegen den Uhrzeigersinn um die Achse) zeigt sie nach außen,
/// wenn die Profilkurve (v) in Achsrichtung läuft, und nach innen, wenn sie gegen die
/// Achsrichtung läuft – etwa bei einem Halbkreis von oben nach unten (Kugel mit Innennormale).
/// Wer Körper baut, muss `same_sense` deshalb aus der Geometrie bestimmen, nicht annehmen.
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
            let mut tiles = Vec::new();
            for i0 in (0..nu).step_by(TILE) {
                for j0 in (0..nv).step_by(TILE) {
                    let (i1, j1) = ((i0 + TILE).min(nu), (j0 + TILE).min(nv));
                    let (mut lo, mut hi) =
                        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
                    for i in i0..i1 {
                        for q in &points[i * nv + j0..i * nv + j1] {
                            lo = lo.min(*q);
                            hi = hi.max(*q);
                        }
                    }
                    tiles.push((i0, i1, j0, j1, lo, hi));
                }
            }
            let mut cell_reach = 0.0_f64;
            for i in 0..nu - 1 {
                for j in 0..nv - 1 {
                    let at = |a: usize, b: usize| points[a * nv + b];
                    cell_reach = cell_reach
                        .max(at(i, j).distance(at(i + 1, j + 1)))
                        .max(at(i + 1, j).distance(at(i, j + 1)));
                }
            }
            Grid {
                nu,
                nv,
                us,
                vs,
                points,
                normals,
                tiles,
                cell_reach,
            }
        })
    }

    /// Kacheln nach unterer Abstandsschranke zu `p` geordnet: `(Schranke², Kachel)`.
    fn tile_order(&self, p: DVec3) -> Vec<(f64, usize)> {
        let g = self.grid();
        let mut order: Vec<(f64, usize)> = g
            .tiles
            .iter()
            .enumerate()
            .map(|(k, t)| (box_distance2(p, t.4, t.5), k))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        order
    }

    /// Ist der Rasterpunkt `(i, j)` mit Abstandsquadrat `here` ein örtliches Minimum
    /// (8er-Nachbarschaft, über die Naht hinweg bei geschlossenen Richtungen)?
    fn is_grid_minimum(&self, p: DVec3, i: usize, j: usize, here: f64) -> bool {
        let g = self.grid();
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
        for di in -1isize..=1 {
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
                if g.points[a * g.nv + b].distance_squared(p) < here {
                    return false;
                }
            }
        }
        true
    }

    /// Die bis zu `k` besten Rasterpunkte (nur örtliche Minima, wenn `local`), aufsteigend nach
    /// Abstandsquadrat, gleich gute nur einmal. Bei `local` zählen nur Minima, deren Abstand
    /// höchstens um `cell_reach` über dem besten liegt (weiter entfernte Minima können nicht im
    /// Einzugsbereich des nächsten Flächenpunkts liegen, denn dessen Zelle hat einen Eckpunkt
    /// innerhalb `cell_reach`). Kacheln, deren Schranke nicht unter der aktuellen Grenze liegt,
    /// werden übersprungen.
    fn grid_best(&self, p: DVec3, k: usize, local: bool) -> Vec<(usize, f64)> {
        let g = self.grid();
        let mut best: Vec<(usize, f64)> = Vec::with_capacity(k + 1);
        let limit_of = |best: &[(usize, f64)]| {
            let kth = if best.len() == k {
                best[k - 1].1
            } else {
                f64::INFINITY
            };
            match best.first() {
                Some(b) if local => kth.min((b.1.sqrt() + g.cell_reach).powi(2)),
                _ => kth,
            }
        };
        for (bound, t) in self.tile_order(p) {
            if bound >= limit_of(&best) {
                break;
            }
            let (i0, i1, j0, j1, ..) = g.tiles[t];
            for i in i0..i1 {
                for j in j0..j1 {
                    let idx = i * g.nv + j;
                    let d2 = g.points[idx].distance_squared(p);
                    if d2 >= limit_of(&best)
                        || best.iter().any(|b| b.1 == d2)
                        || (local && !self.is_grid_minimum(p, i, j, d2))
                    {
                        continue;
                    }
                    let pos = best.partition_point(|b| b.1 < d2);
                    best.insert(pos, (idx, d2));
                    best.truncate(k);
                    // Ein neues Bestes kann früher gefundene Minima aus dem Fenster schieben.
                    let limit = limit_of(&best);
                    best.retain(|b| b.1 <= limit);
                }
            }
        }
        best
    }

    /// Liegt ein Rasterpunkt mit Abstandsquadrat unter `limit2`?
    fn grid_has_closer(&self, p: DVec3, limit2: f64) -> bool {
        let g = self.grid();
        g.tiles.iter().any(|&(i0, i1, j0, j1, lo, hi)| {
            box_distance2(p, lo, hi) < limit2
                && (i0..i1).any(|i| {
                    g.points[i * g.nv + j0..i * g.nv + j1]
                        .iter()
                        .any(|q| q.distance_squared(p) < limit2)
                })
        })
    }

    /// Nächster Rasterpunkt: `(u, v, Index, Abstand)`.
    fn nearest_grid_point(&self, p: DVec3) -> (f64, f64, usize, f64) {
        let g = self.grid();
        let (k, d2) = self.grid_best(p, 1, false)[0];
        (g.us[k / g.nv], g.vs[k % g.nv], k, d2.sqrt())
    }

    /// Örtliche Minima des Abstands über dem Raster, die besten `NEWTON_STARTS` als
    /// Parameterpaare, und der beste Rasterpunkt samt Abstandsquadrat (für `extra_starts`).
    fn grid_starts(&self, p: DVec3) -> (Vec<Uv>, Option<(usize, f64)>) {
        let g = self.grid();
        let best = self.grid_best(p, NEWTON_STARTS, true);
        let starts = best
            .iter()
            .map(|&(k, _)| (g.us[k / g.nv], g.vs[k % g.nv]))
            .collect();
        (starts, best.first().copied())
    }

    /// Zusätzliche Startpunkte, wenn das Ergebnis am Rand liegt oder keine Suche konvergiert:
    /// die Ecken des Bereichs im Fenster des besten Rasterpunkts und die Mitten der vier Zellen
    /// um ihn. An Rändern können zwei Minima (etwa Rand- und Eckminimum) in derselben
    /// Rasterzelle liegen, die nur einen Startpunkt hätte.
    fn extra_starts(&self, p: DVec3, (best, d0): (usize, f64)) -> Vec<(f64, f64)> {
        let g = self.grid();
        let window = (d0.sqrt() + g.cell_reach).powi(2);
        let corners = [0, g.nv - 1, (g.nu - 1) * g.nv, g.nu * g.nv - 1];
        let mut starts: Vec<(f64, f64)> = corners
            .into_iter()
            .filter(|&c| g.points[c].distance_squared(p) <= window)
            .map(|k| (g.us[k / g.nv], g.vs[k % g.nv]))
            .collect();
        let (bi, bj) = (best / g.nv, best % g.nv);
        for (di, dj) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            let (Some(i0), Some(j0)) = ((bi + di).checked_sub(1), (bj + dj).checked_sub(1)) else {
                continue;
            };
            if i0 + 1 < g.nu && j0 + 1 < g.nv {
                starts.push((
                    0.5 * (g.us[i0] + g.us[i0 + 1]),
                    0.5 * (g.vs[j0] + g.vs[j0 + 1]),
                ));
            }
        }
        starts
    }

    /// Liegt `(u, v)` auf dem Rand einer offenen Richtung?
    fn on_open_edge(&self, u: f64, v: f64) -> bool {
        let ((u0, u1), (v0, v1)) = self.surface.domain();
        (!self.periodic_u && (u <= u0 || u >= u1)) || (!self.periodic_v && (v <= v0 || v >= v1))
    }

    /// Nächster Punkt: `(u, v, Abstand)`. `hint` = Startwert (das Ergebnis ist dann der vom
    /// Startwert aus erreichte örtliche Fußpunkt); ohne Startwert (oder wenn die Suche vom
    /// Startwert aus nicht konvergiert) Rastersuche mit den besten drei örtlichen Rasterminima.
    /// `u`, `v` liegen immer im Parameterbereich (geschlossene Richtungen gefaltet, nicht
    /// fortlaufend entfaltet). `None`, wenn keine Suche konvergiert oder `p` nicht endlich ist.
    pub fn closest(&self, p: DVec3, hint: Option<(f64, f64)>) -> Option<(f64, f64, f64)> {
        if !p.is_finite() {
            return None;
        }
        if let Some(r) = self.newton_from_hint(p, hint) {
            return Some(r);
        }
        self.global_closest(p)
    }

    fn newton_from_hint(&self, p: DVec3, hint: Option<(f64, f64)>) -> Option<(f64, f64, f64)> {
        let (u, v) = hint.filter(|(u, v)| u.is_finite() && v.is_finite())?;
        let (u, v) = self.clamp(self.wrap(u, v));
        self.newton(p, u, v)
    }

    fn global_closest(&self, p: DVec3) -> Option<(f64, f64, f64)> {
        let (starts, best) = self.grid_starts(p);
        let run = |starts: Vec<(f64, f64)>| {
            starts
                .into_iter()
                .filter_map(|(u, v)| self.newton(p, u, v))
                .min_by(|a, b| a.2.total_cmp(&b.2))
        };
        let result = run(starts);
        let Some(best) = best else {
            return result;
        };
        if result.is_some_and(|r| !self.on_open_edge(r.0, r.1)) {
            return result;
        }
        [result, run(self.extra_starts(p, best))]
            .into_iter()
            .flatten()
            .min_by(|a, b| a.2.total_cmp(&b.2))
    }

    /// Wie `closest`, aber ein Startwert-Ergebnis wird nur genommen, wenn kein Rasterpunkt
    /// näher liegt (sonst globale Suche): der Startwert beschleunigt, ändert aber das Ergebnis
    /// nicht gegenüber der Suche ohne Startwert (bis auf Minima, die tiefer als der
    /// Rasterabstand zwischen zwei Rasterpunkten liegen).
    fn closest_verified(&self, p: DVec3, hint: Option<(f64, f64)>) -> Option<(f64, f64, f64)> {
        if !p.is_finite() {
            return None;
        }
        if let Some(r) = self.newton_from_hint(p, hint)
            && !self.grid_has_closer(p, r.2 * r.2 * (1.0 - VERIFY_RATIO))
        {
            return Some(r);
        }
        self.global_closest(p)
    }

    /// `(u, v)` des nächsten Punkts (im Parameterbereich, gefaltet). Konvergiert die Suche nicht
    /// (oder ist `p` nicht endlich), der nächste Rasterpunkt – ein echter Flächenpunkt, aber nicht
    /// unbedingt der nächste; wer das unterscheiden muss, nimmt `closest` (liefert dann `None`).
    pub fn project(&self, p: DVec3) -> (f64, f64) {
        self.project_near_opt(p, None)
    }

    /// Wie `project`, mit Startwert. Das Ergebnis ist im Parameterbereich gefaltet (nicht
    /// fortlaufend zum Startwert entfaltet); das Entfalten an der Naht ist Sache des Aufrufers.
    pub fn project_near(&self, p: DVec3, hint: (f64, f64)) -> (f64, f64) {
        self.project_near_opt(p, Some(hint))
    }

    fn project_near_opt(&self, p: DVec3, hint: Option<(f64, f64)>) -> (f64, f64) {
        match self.closest(p, hint) {
            Some((u, v, _)) => (u, v),
            None if p.is_finite() => {
                let (u, v, ..) = self.nearest_grid_point(p);
                (u, v)
            }
            None => {
                let g = self.grid();
                (g.us[0], g.vs[0])
            }
        }
    }

    /// Vorzeichenbehafteter Abstand: Abstand zum nächsten Punkt `S`, Vorzeichen aus
    /// `(p − S) · n` (bei `n = 0` positiv). Konvergiert die Suche nicht, gilt der nächste
    /// Rasterpunkt mit dessen Normale. Nie NaN: nicht endliche Punkte ergeben `f64::MAX`.
    pub fn signed_distance(&self, p: DVec3) -> f64 {
        self.signed_distance_hinted(p, None).0
    }

    /// Wie `signed_distance`, mit Startwert für die Fußpunktsuche (geprüft wie in
    /// `closest_verified`); liefert auch den gefundenen Fußpunkt als Startwert für den nächsten
    /// Aufruf.
    pub fn signed_distance_hinted(
        &self,
        p: DVec3,
        hint: Option<(f64, f64)>,
    ) -> (f64, Option<(f64, f64)>) {
        if !p.is_finite() {
            return (FAR_AWAY, None);
        }
        let (q, n, d, foot) = match self.closest_verified(p, hint) {
            Some((u, v, d)) => (self.surface.point(u, v), self.normal(u, v), d, (u, v)),
            None => {
                let (u, v, k, d) = self.nearest_grid_point(p);
                let g = self.grid();
                (g.points[k], g.normals[k], d, (u, v))
            }
        };
        if !d.is_finite() {
            return (FAR_AWAY, None);
        }
        (if (p - q).dot(n) < 0.0 { -d } else { d }, Some(foot))
    }

    fn clamp(&self, (u, v): (f64, f64)) -> (f64, f64) {
        let ((u0, u1), (v0, v1)) = self.surface.domain();
        (u.clamp(u0, u1), v.clamp(v0, v1))
    }

    /// Gedämpfte, projizierte Newton-Suche nach dem nächsten Punkt von `(u, v)` aus.
    ///
    /// Offene Richtungen sind auf den Bereich begrenzt, geschlossene laufen um. Welche
    /// Randbedingung aktiv ist, entscheidet der Gradient (nicht der Newton-Schritt): Eine
    /// Richtung am Rand ist fest, wenn der Abstieg (−Gradient) nach außen zeigt; für die freien
    /// Richtungen wird das verkleinerte System gelöst und geprüft, ob deren Schritt nun selbst
    /// über einen Rand führt (dann auch fest). `Some((u, v, Abstand))` bei Konvergenz: Schritt
    /// kleiner als `STEP_RATIO` · Bereich (mindestens die Parameterauflösung), oder kein Abstieg mehr möglich **und** der Gradient in
    /// den freien Richtungen ist null (bis auf `ORTHO_TOL`/`ROUND_TOL`). Sonst `None`.
    fn newton(&self, p: DVec3, mut u: f64, mut v: f64) -> Option<(f64, f64, f64)> {
        let ((u0, u1), (v0, v1)) = self.surface.domain();
        let (ru, rv) = (u1 - u0, v1 - v0);
        let (pu, pv) = (self.periodic_u, self.periodic_v);
        // Kleinster darstellbarer Parameterschritt je Richtung (absolut).
        let resolution =
            |a: f64, b: f64| PARAM_RESOLUTION_ULPS * f64::EPSILON * a.abs().max(b.abs());
        let (res_u, res_v) = (resolution(u0, u1), resolution(v0, v1));
        let dist2 = |u: f64, v: f64| self.surface.point(u, v).distance_squared(p);
        let mut f = dist2(u, v);
        for _ in 0..NEWTON_STEPS {
            let d = self.surface.derivs(u, v, 2);
            let r = d[0][0] - p;
            let (su, sv) = (d[1][0], d[0][1]);
            let (suu, suv, svv) = (d[2][0], d[1][1], d[0][2]);
            let g = [r.dot(su), r.dot(sv)];
            if !g[0].is_finite() || !g[1].is_finite() {
                return None;
            }
            // Volle Hesse-Matrix und Gauß-Newton-Näherung (positiv semidefinit), beide mit
            // Regularisierung; welche gilt, entscheidet der freie Block (siehe `solve`).
            let regularize = |mut m: [[f64; 2]; 2]| {
                let reg =
                    (HESSIAN_REGULARIZATION * (m[0][0].abs() + m[1][1].abs())).max(HESSIAN_FLOOR);
                m[0][0] += reg;
                m[1][1] += reg;
                m
            };
            let gn = regularize([[su.dot(su), su.dot(sv)], [su.dot(sv), sv.dot(sv)]]);
            let full = regularize([
                [su.dot(su) + r.dot(suu), su.dot(sv) + r.dot(suv)],
                [su.dot(sv) + r.dot(suv), sv.dot(sv) + r.dot(svv)],
            ]);
            // Aktive Randbedingungen aus dem Gradienten.
            let at_bound_out = |x: f64, gx: f64, a: f64, b: f64, periodic: bool| {
                !periodic && ((x <= a && gx > 0.0) || (x >= b && gx < 0.0))
            };
            let mut fix = [
                at_bound_out(u, g[0], u0, u1, pu),
                at_bound_out(v, g[1], v0, v1, pv),
            ];
            // Newton im freien Block; ist er nicht positiv definit, Gauß-Newton (sonst führte
            // eine konkave feste Richtung dazu, die Krümmung der freien zu verlieren).
            let solve = |fix: [bool; 2]| -> [f64; 2] {
                match fix {
                    [true, true] => [0.0, 0.0],
                    [true, false] => {
                        let h = if full[1][1] > 0.0 {
                            full[1][1]
                        } else {
                            gn[1][1]
                        };
                        [0.0, -g[1] / h]
                    }
                    [false, true] => {
                        let h = if full[0][0] > 0.0 {
                            full[0][0]
                        } else {
                            gn[0][0]
                        };
                        [-g[0] / h, 0.0]
                    }
                    [false, false] => {
                        let det_full = full[0][0] * full[1][1] - full[0][1] * full[1][0];
                        let h = if full[0][0] > 0.0 && det_full > 0.0 {
                            full
                        } else {
                            gn
                        };
                        let det = h[0][0] * h[1][1] - h[0][1] * h[1][0];
                        [
                            -(h[1][1] * g[0] - h[0][1] * g[1]) / det,
                            -(h[0][0] * g[1] - h[1][0] * g[0]) / det,
                        ]
                    }
                }
            };
            let mut step = solve(fix);
            // Führt der Schritt einer freien Richtung am Rand nach außen, wird sie auch fest.
            let outward = |x: f64, dx: f64, a: f64, b: f64, periodic: bool| {
                !periodic && ((x <= a && dx < 0.0) || (x >= b && dx > 0.0))
            };
            let more = [
                !fix[0] && outward(u, step[0], u0, u1, pu),
                !fix[1] && outward(v, step[1], v0, v1, pv),
            ];
            if more[0] || more[1] {
                fix = [fix[0] || more[0], fix[1] || more[1]];
                step = solve(fix);
            }
            // Stationär in den freien Richtungen?
            let size = d[0][0].abs().max(p.abs()).max_element();
            let rl = r.length();
            // Rest durch die Parameterauflösung: `|S_i| · res_i` Abstand ist nicht zu unterbieten.
            let flat = |gi: f64, si: DVec3, res: f64| {
                let l = si.length();
                gi.abs() <= l * (ORTHO_TOL * rl + ROUND_TOL * size + l * res)
            };
            let stationary = (fix[0] || flat(g[0], su, res_u)) && (fix[1] || flat(g[1], sv, res_v));
            if !step[0].is_finite() || !step[1].is_finite() {
                return None;
            }
            let Some((nu, nv, fnew)) = self.line_search(u, v, step, f, &dist2).or_else(|| {
                // Newton-Richtung ohne Abstieg: Gradientenschritt (skaliert mit der
                // Diagonale) in den freien Richtungen versuchen.
                let gs = [
                    if fix[0] { 0.0 } else { -g[0] / gn[0][0] },
                    if fix[1] { 0.0 } else { -g[1] / gn[1][1] },
                ];
                self.line_search(u, v, gs, f, &dist2)
            }) else {
                // Kein Abstieg mehr möglich: nur bei verschwindendem Gradienten ein Minimum.
                return stationary.then(|| (u, v, f.sqrt()));
            };
            let step_u = periodic_delta(nu - u, ru, pu).abs();
            let step_v = periodic_delta(nv - v, rv, pv).abs();
            (u, v, f) = (nu, nv, fnew);
            if step_u < (STEP_RATIO * ru).max(res_u) && step_v < (STEP_RATIO * rv).max(res_v) {
                return Some((u, v, f.sqrt()));
            }
        }
        None
    }

    /// Rückwärts-Halbierung entlang `step` (begrenzt auf `MAX_STEP_FRACTION` des Bereichs je
    /// Richtung, projiziert auf den Bereich), bis der Abstand echt sinkt.
    fn line_search(
        &self,
        u: f64,
        v: f64,
        step: [f64; 2],
        f: f64,
        dist2: &impl Fn(f64, f64) -> f64,
    ) -> Option<(f64, f64, f64)> {
        let ((u0, u1), (v0, v1)) = self.surface.domain();
        let scale = 1.0_f64
            .min(MAX_STEP_FRACTION * (u1 - u0) / step[0].abs().max(f64::MIN_POSITIVE))
            .min(MAX_STEP_FRACTION * (v1 - v0) / step[1].abs().max(f64::MIN_POSITIVE));
        let (du, dv) = (step[0] * scale, step[1] * scale);
        if du == 0.0 && dv == 0.0 {
            return None;
        }
        let mut t = 1.0;
        for _ in 0..MAX_HALVINGS {
            let (nu, nv) = self.clamp(self.wrap(u + du * t, v + dv * t));
            let fnew = dist2(nu, nv);
            if fnew < f {
                return Some((nu, nv, fnew));
            }
            t *= 0.5;
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
