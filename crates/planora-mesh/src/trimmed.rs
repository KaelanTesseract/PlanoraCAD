//! Beschnittene gekrümmte Flächen: eingeschränkte Delaunay-Triangulierung im Parameterraum.

use crate::discretize::Discretizer;
use crate::mesh::Mesh;
use crate::tessellate::{MeshError, ensure_room, index_u32};
use planora_kernel::boolean::{
    FaceGeom, loop_points, oriented_area, points_to_uv, points_to_uv_indexed, winding,
};
use planora_kernel::{Id, NURBS_EDGE_TOL, NurbsSurfaceData, Surface, UvRect};
use planora_math::nurbs::NurbsSurface;
use planora_math::{DVec2, DVec3, TAU};
use spade::handles::FixedVertexHandle;
use spade::{
    ConstrainedDelaunayTriangulation, Point2, PositionInTriangulation, Triangulation,
    mitigate_underflow,
};

type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;

/// Obergrenze für die Zahl der Rasterpunkte einer Fläche; darüber wird das Raster in `v` gröber.
const MAX_GRID_POINTS: usize = 250_000;

/// Ist die Fläche gegenüber ihrer Rechteck-Domäne beschnitten (Löcher oder fehlende Anteile)?
/// Ebenen und Flächen ohne Domäne gelten nie als beschnitten.
pub fn is_trimmed(g: &FaceGeom) -> bool {
    if matches!(g.surface, Surface::Plane { .. }) {
        return false;
    }
    let Some(d) = g.domain else { return false };
    if g.loops.len() > 1 {
        return true;
    }
    let Some(outer) = g.loops.first() else {
        return false;
    };
    let poly = points_to_uv(&g.surface, g.same_sense, &loop_points(outer));
    let full = (d.u1 - d.u0) * (d.v1 - d.v0);
    let a = oriented_area(&poly, g.same_sense);
    (a - full).abs() > 1e-6 * full.abs().max(1e-12)
}

/// Maßstab, mit dem `(u, v)` für die Triangulierung gestreckt wird, damit Abstände ungefähr
/// Längen auf der Fläche entsprechen (Delaunay bevorzugt dann Dreiecke mit kleiner Winkelspanne).
fn uv_scale(surface: &Surface, polys: &[Vec<DVec2>]) -> DVec2 {
    match surface {
        Surface::Cylinder { radius, .. } => DVec2::new(radius.abs(), 1.0),
        Surface::Cone { radius, slope, .. } => {
            let r = polys
                .iter()
                .flatten()
                .map(|p| (radius + slope * p.y).abs())
                .fold(0.0, f64::max);
            DVec2::new(r.max(1e-9), 1.0)
        }
        Surface::Torus { major, minor, .. } => DVec2::new(major + minor, minor.abs()),
        Surface::Sphere { radius, .. } => DVec2::splat(radius.abs()),
        Surface::Nurbs(d) => nurbs_uv_scale(d.surface(), &nurbs_patch(d, polys)),
        Surface::Plane { .. } => DVec2::ONE,
    }
}

/// Abbildung einer Parameterrichtung in den gestreckten Raum der Triangulierung: linear (fester
/// Maßstab, wie bisher) oder stückweise linear über den Knotenabschnitten einer Freiformfläche,
/// deren Ableitungslänge von Abschnitt zu Abschnitt stark schwankt (sonst wären die Dreiecke in
/// schnellen Abschnitten im gestreckten Raum extrem schmal).
#[derive(Clone, Debug)]
enum Axis {
    Linear(f64),
    /// Stützstellen `t` (steigend) und Bildwerte `s` (steigend); außerhalb linear fortgesetzt
    /// (bei `period` um ganze Perioden verschoben).
    Pieces {
        t: Vec<f64>,
        s: Vec<f64>,
        period: Option<f64>,
    },
}

impl Axis {
    fn map(&self, x: f64) -> f64 {
        match self {
            Axis::Linear(k) => x * k,
            Axis::Pieces { t, s, period } => {
                let (t0, t1) = (t[0], t[t.len() - 1]);
                let (s0, s1) = (s[0], s[s.len() - 1]);
                if let Some(p) = period
                    && x.is_finite()
                    && (x < t0 || x > t1)
                {
                    let k = ((x - t0) / p).floor();
                    return Self::map_inside(t, s, x - k * p) + k * (s1 - s0);
                }
                Self::map_inside(t, s, x)
            }
        }
    }

    /// Stückweise lineare Abbildung; jenseits der Enden mit der Steigung des Randabschnitts.
    fn map_inside(t: &[f64], s: &[f64], x: f64) -> f64 {
        let n = t.len();
        let k = t.partition_point(|v| *v <= x).clamp(1, n - 1);
        let (a, b) = (t[k - 1], t[k]);
        s[k - 1] + (s[k] - s[k - 1]) * (x - a) / (b - a)
    }

    fn inverse(&self, y: f64) -> f64 {
        match self {
            Axis::Linear(k) => y / k,
            Axis::Pieces { t, s, period } => {
                let (t0, t1) = (t[0], t[t.len() - 1]);
                let (s0, s1) = (s[0], s[s.len() - 1]);
                if period.is_some() && y.is_finite() && (y < s0 || y > s1) {
                    let k = ((y - s0) / (s1 - s0)).floor();
                    return Self::map_inside(s, t, y - k * (s1 - s0)) + k * (t1 - t0);
                }
                Self::map_inside(s, t, y)
            }
        }
    }

    /// Mittlere Steigung (gestreckte Länge je Parametereinheit).
    fn mean(&self) -> f64 {
        match self {
            Axis::Linear(k) => *k,
            Axis::Pieces { t, s, .. } => (s[s.len() - 1] - s[0]) / (t[t.len() - 1] - t[0]),
        }
    }
}

/// Abbildung `(u, v)` → gestreckter Raum der Triangulierung (je Richtung eine [`Axis`]).
#[derive(Clone, Debug)]
struct UvMap {
    u: Axis,
    v: Axis,
}

impl UvMap {
    fn linear(scale: DVec2) -> UvMap {
        UvMap {
            u: Axis::Linear(scale.x),
            v: Axis::Linear(scale.y),
        }
    }

    fn apply(&self, p: DVec2) -> DVec2 {
        match (&self.u, &self.v) {
            // genau wie früher `p * scale`
            (Axis::Linear(a), Axis::Linear(b)) => p * DVec2::new(*a, *b),
            _ => DVec2::new(self.u.map(p.x), self.v.map(p.y)),
        }
    }

    fn invert(&self, q: DVec2) -> DVec2 {
        match (&self.u, &self.v) {
            (Axis::Linear(a), Axis::Linear(b)) => q / DVec2::new(*a, *b),
            _ => DVec2::new(self.u.inverse(q.x), self.v.inverse(q.y)),
        }
    }

    /// Mittlerer Maßstab je Richtung.
    fn mean(&self) -> DVec2 {
        DVec2::new(self.u.mean(), self.v.mean())
    }
}

/// Schwanken die mittleren Ableitungslängen einer Freiformfläche zwischen ihren Knotenabschnitten
/// um mehr als diesen Faktor, wird stückweise gestreckt (siehe [`Axis`]); sonst linear wie bisher.
const MAX_SPAN_SPEED_RATIO: f64 = 4.0;

/// Stützstellen je Knotenabschnitt (und quer dazu) für die Ableitungslänge eines Abschnitts.
const SPAN_SPEED_SAMPLES: usize = 5;

/// Stückweise Streckung einer Richtung (`along_u`) einer Freiformfläche über ihren
/// Knotenabschnitten im Bereich `lo … hi`, oder `None`, wenn die Ableitungslängen der Abschnitte
/// höchstens um [`MAX_SPAN_SPEED_RATIO`] schwanken (dann bleibt es beim linearen Maßstab).
fn span_axis(
    s: &NurbsSurface,
    along_u: bool,
    (lo, hi): (f64, f64),
    (qlo, qhi): (f64, f64),
    period: Option<f64>,
) -> Option<Axis> {
    let knots = if along_u { s.knots_u() } else { s.knots_v() };
    let (d0, d1) = knots.domain();
    let mut t: Vec<f64> = vec![d0];
    for &k in knots.knots() {
        if k > *t.last()? && k <= d1 {
            t.push(k);
        }
    }
    if t.len() < 3 || lo.is_nan() || hi.is_nan() || lo >= hi {
        return None;
    }
    let n = SPAN_SPEED_SAMPLES;
    let speeds: Vec<f64> = t
        .windows(2)
        .map(|w| {
            let mut sum = 0.0;
            for i in 0..n {
                let a = w[0] + (w[1] - w[0]) * (i as f64 + 0.5) / n as f64;
                for j in 0..n {
                    let b = qlo + (qhi - qlo) * (j as f64 + 0.5) / n as f64;
                    let (u, v) = if along_u { (a, b) } else { (b, a) };
                    let d = s.derivs(u, v, 1);
                    sum += if along_u {
                        d[1][0].length()
                    } else {
                        d[0][1].length()
                    };
                }
            }
            sum / (n * n) as f64
        })
        .collect();
    let max = speeds.iter().copied().fold(0.0, f64::max);
    let min = speeds.iter().copied().fold(f64::INFINITY, f64::min);
    if !(max.is_finite() && min > 0.0) || max <= MAX_SPAN_SPEED_RATIO * min {
        return None;
    }
    let mut out = vec![t[0] * speeds[0]];
    for (k, w) in t.windows(2).enumerate() {
        let last = out[out.len() - 1];
        out.push(last + (w[1] - w[0]) * speeds[k].max(MIN_SCALE));
    }
    Some(Axis::Pieces { t, s: out, period })
}

/// Abbildung in den gestreckten Raum: analytische Flächen und gleichmäßige Freiformflächen linear
/// mit [`uv_scale`]; Freiformflächen mit stark schwankender Ableitungslänge zwischen den
/// Knotenabschnitten stückweise ([`span_axis`]).
fn uv_map(surface: &Surface, polys: &[Vec<DVec2>]) -> UvMap {
    let lin = UvMap::linear(uv_scale(surface, polys));
    let Surface::Nurbs(d) = surface else {
        return lin;
    };
    let patch = nurbs_patch(d, polys);
    let s = d.surface();
    let pu = surface.u_period();
    let pv = surface.v_period();
    let u = span_axis(s, true, (patch.u0, patch.u1), (patch.v0, patch.v1), pu);
    let v = span_axis(s, false, (patch.v0, patch.v1), (patch.u0, patch.u1), pv);
    if u.is_none() && v.is_none() {
        return lin;
    }
    UvMap {
        u: u.unwrap_or(lin.u),
        v: v.unwrap_or(lin.v),
    }
}

/// Zahl der Stützstellen je Richtung für die mittleren Ableitungslängen einer Freiformfläche.
const SCALE_SAMPLES: usize = 9;

/// Kleinster Maßstab einer Parameterrichtung (verhindert eine Division durch null, wenn eine
/// Richtung überall entartet ist).
const MIN_SCALE: f64 = 1e-9;

/// Maßstab einer Freiformfläche: mittlere Längen `|S_u|` und `|S_v|` auf einem Gitter über dem
/// Parameterrechteck (Längen auf der Fläche je Parametereinheit).
fn nurbs_uv_scale(s: &NurbsSurface, patch: &UvRect) -> DVec2 {
    let n = SCALE_SAMPLES;
    let mut sum = DVec2::ZERO;
    for i in 0..n {
        for j in 0..n {
            let u = patch.u0 + (patch.u1 - patch.u0) * i as f64 / (n - 1) as f64;
            let v = patch.v0 + (patch.v1 - patch.v0) * j as f64 / (n - 1) as f64;
            let d = s.derivs(u, v, 1);
            sum += DVec2::new(d[1][0].length(), d[0][1].length());
        }
    }
    let mean = sum / (n * n) as f64;
    let fix = |x: f64| if x.is_finite() { x.max(MIN_SCALE) } else { 1.0 };
    DVec2::new(fix(mean.x), fix(mean.y))
}

/// Parameterrechteck einer Freiformfläche für Raster und Maßstab: Hüllrechteck der
/// Randpolygone, auf den Parameterbereich begrenzt. Reicht es in einer geschlossenen Richtung
/// über den Bereich hinaus (aufgerollte Naht), gilt dort der ganze Bereich.
fn nurbs_patch(d: &NurbsSurfaceData, polys: &[Vec<DVec2>]) -> UvRect {
    let ((du0, du1), (dv0, dv1)) = d.surface().domain();
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for p in polys.iter().flatten() {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let range = |lo: f64, hi: f64, a: f64, b: f64, periodic: bool| {
        if !(lo.is_finite() && hi.is_finite()) || hi <= lo || (periodic && (lo < a || hi > b)) {
            (a, b)
        } else {
            let (x, y) = (lo.max(a), hi.min(b));
            if y > x { (x, y) } else { (a, b) }
        }
    };
    let (u0, u1) = range(lo.x, hi.x, du0, du1, d.is_periodic_u());
    let (v0, v1) = range(lo.y, hi.y, dv0, dv1, d.is_periodic_v());
    UvRect { u0, u1, v0, v1 }
}

/// Größtes Seitenverhältnis einer Rasterzelle im gestreckten Parameterraum. Ohne Grenze hätte
/// etwa eine Regelfläche (Grad 1 in v) nur eine Innenzeile; die Delaunay-Triangulierung bildet
/// dann an Beschnitten lange, schräge Dreiecke über viele Spalten.
const MAX_CELL_ASPECT: f64 = 1.5;

/// Rasterzelle (in Parametern) einer beschnittenen Freiformfläche aus der *nötigen* Schrittweite
/// je Richtung (`need`, aus [`Discretizer::nurbs_needed_grid`]): Die längere Kante wird im
/// gestreckten Raum auf höchstens [`MAX_CELL_ASPECT`] mal die kürzere begrenzt. Maßgeblich ist die
/// nötige Schrittweite der anderen Richtung, nicht ein feineres Raster – sonst würde etwa eine
/// lange Röhre in ihrer geraden Richtung unnötig fein geteilt.
fn isotropic_cell(need: DVec2, scale: DVec2) -> DVec2 {
    let s = need * scale;
    if !(s.x > 0.0 && s.y > 0.0 && s.is_finite()) {
        return need;
    }
    let mut out = need;
    if s.y > MAX_CELL_ASPECT * s.x {
        out.y = MAX_CELL_ASPECT * s.x / scale.y;
    } else if s.x > MAX_CELL_ASPECT * s.y {
        out.x = MAX_CELL_ASPECT * s.y / scale.x;
    }
    out
}

/// Höchstzahl der Verfeinerungsrunden nach der Triangulierung einer Freiformfläche.
const MAX_CDT_REFINE_ROUNDS: usize = 24;

/// Zuschlag beim Teilen einer zu ungenauen Innenkante (wie `REFINE_MARGIN` beim Raster).
const SPLIT_MARGIN: f64 = 1.2;

/// Höchstzahl Stücke, in die eine Innenkante je Runde geteilt wird.
const MAX_EDGE_SPLIT: f64 = 16.0;

/// Verfeinert die Triangulierung einer Freiformfläche, bis jedes Dreieck im Inneren der Fläche
/// höchstens um `tol` von ihr abweicht: gemessen an den Mitten der Innenkanten (Kantenmitte im
/// Parameterraum gegen die Mitte der 3D-Sehne) und am Schwerpunkt. Zu ungenaue Innenkanten werden
/// in der Mitte geteilt, sonst wird der Schwerpunkt eingefügt. Randkanten (Zwangskanten) bleiben
/// unberührt, damit die Randpunkte mit den Nachbarflächen übereinstimmen; weicht eine Randkante
/// selbst zu weit ab, werden ihre Dreiecke nicht weiter verfeinert. Begrenzt durch
/// [`MAX_CDT_REFINE_ROUNDS`] und [`MAX_GRID_POINTS`]. `Ok(true)`: Toleranz erreicht; `Ok(false)`:
/// eine Grenze wurde erreicht oder der Rand ist zu grob, die Toleranz gilt nicht überall (der Aufrufer vermerkt die Fläche
/// in `Mesh::inexact_faces`).
///
/// Zusätzlich bekommt jedes Dreieck, dessen drei Ecken Randpunkte sind (Index unter
/// `n_boundary`), seinen Schwerpunkt als Innenpunkt, sobald es um mehr als [`fin_tolerance`] von
/// der Fläche abweicht, auch unter `tol`. Sonst kann ein solches Dreieck über einem kurzen
/// Bogenstück einer ebenen Schnittkante genau in der Ebene der Nachbarfläche liegen, die dasselbe
/// Stück zwischen Bogen und Sehne mit denselben Randpunkten abdeckt: vier Dreiecke an einer Kante
/// („Flosse“), das Netz wäre nicht geschlossen.
fn refine_to_chord_tolerance(
    b: &mut Builder,
    surface: &Surface,
    tol: f64,
    n_boundary: usize,
) -> Result<bool, ()> {
    use std::collections::HashMap;
    let fin_tol = fin_tolerance(&b.pts3[..n_boundary.min(b.pts3.len())]);
    // Eine Randkante (Zwangskante), die selbst weiter als `tol` von der Fläche abweicht (etwa
    // weil die Kantenteilung an ihrer Obergrenze liegt), lässt sich hier nicht verbessern: Die
    // Fläche wird als ungenau gemeldet. An den Ecken solcher Randkanten gilt als Schranke die
    // Abweichung der Randkante selbst (genauer geht es dort nicht); sonst wanderten immer neue
    // Punkte gegen die zu grobe Sehne.
    let mut coarse: HashMap<usize, f64> = HashMap::new();
    for e in b.cdt.undirected_edges() {
        if !e.is_constraint_edge() {
            continue;
        }
        let [p, q] = e.vertices().map(|v| v.fix().index());
        // Echter Abstand der Sehnenmitte zur Fläche: Die Parametermitte einer Randkante liegt
        // bei gekrümmten Rändern nicht auf der Randkurve und würde die Abweichung überschätzen.
        let dev = surface.implicit(0.5 * (b.pts3[p] + b.pts3[q])).abs();
        if dev > tol {
            for k in [p, q] {
                let e = coarse.entry(k).or_insert(dev);
                *e = e.max(dev);
            }
        }
    }
    let limited = !coarse.is_empty();
    // Schranke an einer Ecke: `tol`, an Ecken zu grober Randkanten deren Abweichung.
    let local = |i: usize| coarse.get(&i).copied().unwrap_or(tol);
    for _ in 0..MAX_CDT_REFINE_ROUNDS {
        let inner = inner_faces_by_parity(&b.cdt);
        let mut edge_bad: HashMap<(usize, usize), bool> = HashMap::new();
        let mut inserts: Vec<DVec2> = Vec::new();
        // Schwerpunkte von Dreiecken nur aus Randpunkten (siehe oben), getrennt gezählt: Lässt
        // sich keiner davon einfügen, ist die Sehnentoleranz trotzdem erreicht.
        let mut fin_inserts: Vec<DVec2> = Vec::new();
        for f in b.cdt.inner_faces() {
            if !inner[f.fix().index()] {
                continue;
            }
            let v = f.vertices();
            let idx = [v[0].fix().index(), v[1].fix().index(), v[2].fix().index()];
            if degenerate(b, idx) {
                continue;
            }
            let mut any_bad = false;
            for e in f.adjacent_edges() {
                if e.is_constraint_edge() {
                    continue;
                }
                let (i, j) = (e.from().fix().index(), e.to().fix().index());
                let key = (i.min(j), i.max(j));
                if let Some(bad) = edge_bad.get(&key) {
                    any_bad |= *bad;
                    continue;
                }
                let mid = 0.5 * (b.uv[i] + b.uv[j]);
                let chord = 0.5 * (b.pts3[i] + b.pts3[j]);
                let limit = local(i).max(local(j));
                let dev = deviation(surface, mid, chord, limit);
                let bad = dev > limit;
                edge_bad.insert(key, bad);
                if bad {
                    // Sehnenabweichung fällt quadratisch: in k gleiche Stücke teilen.
                    let k = ((dev / limit).sqrt() * SPLIT_MARGIN)
                        .ceil()
                        .clamp(2.0, MAX_EDGE_SPLIT);
                    let k = k as usize;
                    for m in 1..k {
                        let t = m as f64 / k as f64;
                        inserts.push(b.uv[i] + (b.uv[j] - b.uv[i]) * t);
                    }
                    any_bad = true;
                }
            }
            if any_bad {
                continue;
            }
            let c = (b.uv[idx[0]] + b.uv[idx[1]] + b.uv[idx[2]]) / 3.0;
            let avg = (b.pts3[idx[0]] + b.pts3[idx[1]] + b.pts3[idx[2]]) / 3.0;
            let limit = idx.iter().map(|i| local(*i)).fold(tol, f64::max);
            if deviation(surface, c, avg, limit) > limit {
                inserts.push(c);
            } else if idx.iter().all(|i| *i < n_boundary)
                && deviation(surface, c, avg, fin_tol) > fin_tol
            {
                fin_inserts.push(c);
            }
        }
        if inserts.is_empty() && fin_inserts.is_empty() {
            return Ok(!limited);
        }
        if b.pts3.len() + inserts.len() + fin_inserts.len() > MAX_GRID_POINTS {
            // Hält die Fläche die Sehnentoleranz schon ein und fehlen nur Schwerpunkte von
            // Randdreiecken, ist sie nicht ungenau (nur diese Punkte entfallen).
            return Ok(inserts.is_empty() && !limited);
        }
        let mut added = 0;
        for q in &inserts {
            added += usize::from(b.add_interior(*q, surface.point(q.x, q.y))?);
        }
        let mut fins_added = 0;
        for q in fin_inserts {
            fins_added += usize::from(b.add_interior(q, surface.point(q.x, q.y))?);
        }
        if added == 0 && fins_added == 0 {
            // Nichts mehr einzufügen (alle Punkte lägen auf dem Rand oder einer Ecke). Waren es
            // nur Schwerpunkte von Randdreiecken, hält das Netz die Sehnentoleranz trotzdem.
            return Ok(inserts.is_empty() && !limited);
        }
    }
    // Letzte Runde: Toleranz nur erreicht, wenn nichts mehr einzufügen wäre.
    Ok(!limited && within_chord_tolerance(b, surface, tol))
}

/// Anteil der Flächengröße (Diagonale der Randpunkte), ab dem ein Dreieck nur aus Randpunkten
/// als von der Fläche abweichend gilt (siehe [`refine_to_chord_tolerance`]).
const FIN_REL: f64 = 1e-9;

/// Untergrenze dieser Schranke: ein Hundertstel von [`NURBS_EDGE_TOL`]. Randpunkte von
/// Freiformflächen liegen nur bis `NURBS_EDGE_TOL` genau auf der Fläche; der Schwerpunkt eines
/// Randdreiecks weicht darum oft schon um diesen Betrag ab. Schwerpunkte solcher Dreiecke werden
/// dann eingefügt (wenige Punkte, unschädlich); die Schranke trennt nur ebene Dreiecke ohne
/// messbare Abweichung (dort hilft ein zusätzlicher Punkt nicht) von gekrümmten, nicht die
/// Größe der Abweichung ab.
const FIN_FLOOR: f64 = NURBS_EDGE_TOL / 100.0;

/// Schranke für Dreiecke nur aus Randpunkten: `FIN_REL` mal Diagonale der Randpunkte, mindestens
/// [`FIN_FLOOR`]. Weit unter jeder üblichen Sehnentoleranz, weil die Flosse bei beliebig kleiner
/// Bogenhöhe entsteht.
fn fin_tolerance(boundary: &[DVec3]) -> f64 {
    let (lo, hi) = boundary.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let size = (hi - lo).length();
    if size.is_finite() {
        (FIN_REL * size).max(FIN_FLOOR)
    } else {
        FIN_FLOOR
    }
}

/// Abweichung eines Netzpunkts `p` (Sehnen- oder Dreiecksmitte) von der Fläche. Schnell über den
/// Flächenpunkt an der Parametermitte `uv` (eine obere Schranke); liegt dieser weiter als
/// `limit` entfernt, entscheidet der echte Abstand zur Fläche. Nötig an Polen: Dort ist `u`
/// beliebig, die Parametermitte einer Kante vom Pol liegt dann weit neben der Sehne, obwohl
/// die Sehne die Fläche genau trifft.
fn deviation(surface: &Surface, uv: DVec2, p: DVec3, limit: f64) -> f64 {
    let quick = surface.point(uv.x, uv.y).distance(p);
    if quick <= limit {
        quick
    } else {
        surface.implicit(p).abs().min(quick)
    }
}

/// Kleinster Abstand zweier Ecken eines Dreiecks im Raum, unter dem es als entartet gilt (wie in
/// der Ausgabe: Polpunkte haben zwei Parameterlagen, aber eine 3D-Lage).
const WELD: f64 = 1e-12;

/// Fallen zwei Ecken des Dreiecks im Raum zusammen? Solche Dreiecke entfallen in der Ausgabe
/// und werden nicht verfeinert.
fn degenerate(b: &Builder, idx: [usize; 3]) -> bool {
    let [p0, p1, p2] = idx.map(|i| b.pts3[i]);
    p0.distance(p1) < WELD || p1.distance(p2) < WELD || p0.distance(p2) < WELD
}

/// Halten alle Innenkanten und Schwerpunkte der Dreiecke im Inneren die Toleranz ein?
fn within_chord_tolerance(b: &Builder, surface: &Surface, tol: f64) -> bool {
    let inner = inner_faces_by_parity(&b.cdt);
    b.cdt.inner_faces().all(|f| {
        let v = f.vertices();
        let idx = [v[0].fix().index(), v[1].fix().index(), v[2].fix().index()];
        if !inner[f.fix().index()] || degenerate(b, idx) {
            return true;
        }
        let edges_ok = f.adjacent_edges().into_iter().all(|e| {
            if e.is_constraint_edge() {
                return true;
            }
            let (i, j) = (e.from().fix().index(), e.to().fix().index());
            let mid = 0.5 * (b.uv[i] + b.uv[j]);
            deviation(surface, mid, 0.5 * (b.pts3[i] + b.pts3[j]), tol) <= tol
        });
        let c = (b.uv[idx[0]] + b.uv[idx[1]] + b.uv[idx[2]]) / 3.0;
        let avg = (b.pts3[idx[0]] + b.pts3[idx[1]] + b.pts3[idx[2]]) / 3.0;
        edges_ok && deviation(surface, c, avg, tol) <= tol
    })
}

/// Abstand von `q` zur Strecke `a`–`b`.
fn segment_distance(q: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 {
        ((q - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    q.distance(a + ab * t)
}

struct Builder {
    cdt: Cdt,
    map: UvMap,
    pts3: Vec<DVec3>,
    uv: Vec<DVec2>,
}

impl Builder {
    /// Wie `add`, aber nur für Punkte im Inneren: Fällt der Punkt (im gestreckten Raum) genau
    /// auf eine Randkante oder eine vorhandene Ecke, wird er nicht eingefügt – eine geteilte
    /// Randkante hätte eine Ecke, die der Nachbarfläche fehlt (Netz nicht dicht).
    /// `Ok(true)`, wenn eine neue Ecke entstand.
    fn add_interior(&mut self, p2: DVec2, p3: DVec3) -> Result<bool, ()> {
        let s = self.map.apply(p2);
        let q = mitigate_underflow(Point2::new(s.x, s.y));
        match self.cdt.locate(q) {
            PositionInTriangulation::OnVertex(_) => return Ok(false),
            PositionInTriangulation::OnEdge(e)
                if self.cdt.is_constraint_edge(e.as_undirected()) =>
            {
                return Ok(false);
            }
            _ => {}
        }
        let before = self.pts3.len();
        self.add(p2, p3)?;
        Ok(self.pts3.len() > before)
    }

    /// Punkt einfügen (gleiche Lage ergibt denselben Punkt) und seine 3D-Lage merken.
    fn add(&mut self, p2: DVec2, p3: DVec3) -> Result<FixedVertexHandle, ()> {
        let s = self.map.apply(p2);
        let q = mitigate_underflow(Point2::new(s.x, s.y));
        let h = self.cdt.insert(q).map_err(|_| ())?;
        if h.index() == self.pts3.len() {
            self.pts3.push(p3);
            self.uv.push(p2);
        }
        Ok(h)
    }
}

/// Verschiebt die Löcher einer periodischen Fläche um ganze Perioden (`u_period`/`v_period`),
/// sodass sie in der äußeren Schleife liegen (jede Schleife wird für sich aufgerollt). Liegt ein
/// Loch bei keiner Verschiebung in der äußeren Schleife, ist die Fläche widersprüchlich: `Err`.
fn align_holes(surface: &Surface, rings: &mut [Vec<(DVec2, Option<usize>)>]) -> Result<(), ()> {
    if rings.len() < 2 {
        return Ok(());
    }
    let outer: Vec<DVec2> = rings[0].iter().map(|(p, _)| *p).collect();
    let dus: Vec<f64> = match surface.u_period() {
        Some(t) => vec![0.0, -t, t, -2.0 * t, 2.0 * t],
        None => vec![0.0],
    };
    let dvs: Vec<f64> = match surface.v_period() {
        Some(t) => vec![0.0, -t, t],
        None => vec![0.0],
    };
    for ring in rings.iter_mut().skip(1) {
        let Some(&(first, _)) = ring.first() else {
            return Err(());
        };
        let s = dus
            .iter()
            .flat_map(|du| dvs.iter().map(move |dv| DVec2::new(*du, *dv)))
            .find(|s| winding(&outer, first + *s) != 0)
            .ok_or(())?;
        for (p, _) in ring.iter_mut() {
            *p += s;
        }
    }
    Ok(())
}

/// Rasterpunkte (in `(u, v)`) im Inneren der Fläche, mit Abstand zum Rand. Abstände werden im
/// gestreckten Raum (`scale`) gemessen, die Maschenweite dort ist `h`. Wären es mehr als
/// `MAX_GRID_POINTS` Rasterpunkte, wird das Raster in `v` gröber; reicht das nicht (zu viele
/// Spalten), folgt `TooLarge`.
fn grid_points(
    polys: &[Vec<DVec2>],
    map: &UvMap,
    h: f64,
    sign: f64,
) -> Result<Vec<DVec2>, MeshError> {
    grid_points_with(polys, map, sign, |lo, hi| {
        // auch NaN führt zu einem leeren Raster
        if h.is_nan() || h <= 0.0 {
            return None;
        }
        Some((
            ((hi.x - lo.x) / h).ceil().max(1.0),
            ((hi.y - lo.y) / h).ceil().max(1.0),
            h,
        ))
    })
}

/// Wie [`grid_points`], aber mit Zellgrößen in Parametern (`cell`, ungestreckt): Freiformflächen
/// bekommen ihre Teilung aus [`Discretizer::nurbs_grid`]. Der Randabstand ist ein Bruchteil der
/// kleineren gestreckten Zellkante.
fn grid_points_cells(
    polys: &[Vec<DVec2>],
    map: &UvMap,
    cell: DVec2,
    sign: f64,
) -> Result<Vec<DVec2>, MeshError> {
    let c = cell * map.mean();
    grid_points_with(polys, map, sign, |lo, hi| {
        if !(c.x > 0.0 && c.y > 0.0) {
            return None;
        }
        Some((
            ((hi.x - lo.x) / c.x).ceil().max(1.0),
            ((hi.y - lo.y) / c.y).ceil().max(1.0),
            c.x,
        ))
    })
}

/// Gemeinsamer Kern der Raster: `cells(lo, hi)` liefert im gestreckten Raum die Spalten- und
/// Zeilenzahl über dem Hüllrechteck `lo … hi` sowie die Zellbreite für den Randabstand (`None`:
/// kein Raster).
fn grid_points_with(
    polys: &[Vec<DVec2>],
    map: &UvMap,
    sign: f64,
    cells: impl Fn(DVec2, DVec2) -> Option<(f64, f64, f64)>,
) -> Result<Vec<DVec2>, MeshError> {
    let sp: Vec<Vec<DVec2>> = polys
        .iter()
        .map(|p| p.iter().map(|q| map.apply(*q)).collect())
        .collect();
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for p in sp.iter().flatten() {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    // auch NaN führt zu einem leeren Raster
    let usable = hi.x > lo.x && hi.y > lo.y;
    if !usable {
        return Ok(Vec::new());
    }
    let Some((nu_f, nv_f, h)) = cells(lo, hi) else {
        return Ok(Vec::new());
    };
    // Mindestens zwei Zeilen müssen in die Obergrenze passen (auch ∞ wird hier abgewiesen).
    if !(nu_f.is_finite() && nv_f.is_finite()) || nu_f * 2.0 > MAX_GRID_POINTS as f64 {
        return Err(MeshError::TooLarge);
    }
    let nu = nu_f as usize;
    let nv = (nv_f as usize).min(MAX_GRID_POINTS / nu);
    (nu + 1).checked_mul(nv + 1).ok_or(MeshError::TooLarge)?;
    let at = |i: usize, j: usize| {
        DVec2::new(
            lo.x + (hi.x - lo.x) * i as f64 / nu as f64,
            lo.y + (hi.y - lo.y) * j as f64 / nv as f64,
        )
    };

    // Punkte nahe am Rand ausschließen: je Randstrecke nur die Rasterpunkte in ihrer Umgebung prüfen.
    let margin = 0.3 * h.min((hi.y - lo.y) / nv as f64);
    let mut blocked = vec![false; (nu + 1) * (nv + 1)];
    let cell =
        |x: f64, n: usize, lo: f64, hi: f64| ((x - lo) / (hi - lo) * n as f64).clamp(0.0, n as f64);
    for p in &sp {
        for k in 0..p.len() {
            let (a, b) = (p[k], p[(k + 1) % p.len()]);
            let (i0, i1) = (
                cell(a.x.min(b.x) - margin, nu, lo.x, hi.x).floor() as usize,
                cell(a.x.max(b.x) + margin, nu, lo.x, hi.x).ceil() as usize,
            );
            let (j0, j1) = (
                cell(a.y.min(b.y) - margin, nv, lo.y, hi.y).floor() as usize,
                cell(a.y.max(b.y) + margin, nv, lo.y, hi.y).ceil() as usize,
            );
            for i in i0..=i1 {
                for j in j0..=j1 {
                    if segment_distance(at(i, j), a, b) < margin {
                        blocked[j * (nu + 1) + i] = true;
                    }
                }
            }
        }
    }

    // Innen/außen je Rasterzeile über die Kreuzungen der Randstrecken (wie `winding`).
    let mut out = Vec::new();
    let mut crossings: Vec<(f64, i32)> = Vec::new();
    for j in 1..nv {
        let y = at(0, j).y;
        crossings.clear();
        for p in &sp {
            for k in 0..p.len() {
                let (a, b) = (p[k], p[(k + 1) % p.len()]);
                let dw = if a.y <= y && b.y > y {
                    1
                } else if b.y <= y && a.y > y {
                    -1
                } else {
                    continue;
                };
                let x = a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y);
                crossings.push((x, dw));
            }
        }
        crossings.sort_by(|p, q| p.0.total_cmp(&q.0));
        // Windungszahl eines Punkts = Summe der Kreuzungen rechts von ihm.
        let mut w: i32 = crossings.iter().map(|c| c.1).sum();
        let mut c = 0;
        for i in 1..nu {
            let q = at(i, j);
            while c < crossings.len() && crossings[c].0 <= q.x {
                w -= crossings[c].1;
                c += 1;
            }
            if f64::from(w) * sign >= 1.0 && !blocked[j * (nu + 1) + i] {
                out.push(map.invert(q));
            }
        }
    }
    Ok(out)
}

/// Eingeschränkte Delaunay-Triangulierung eines ebenen Polygons mit Löchern (Ersatz, wenn
/// `earcut` kein dichtes Ergebnis liefert, siehe `tessellate_planar`). `points` sind alle
/// Ringpunkte hintereinander, `rings` die Indexbereiche je Ring. Liefert Dreiecke gegen den
/// Uhrzeigersinn aus Indizes in `points`; jeder Ringpunkt bleibt eine Ecke (keine T-Stöße).
/// `None`, wenn sich Randkanten kreuzen oder ein Punkt nicht eingefügt werden kann.
pub(crate) fn triangulate_planar_cdt(
    points: &[DVec2],
    rings: &[std::ops::Range<usize>],
) -> Option<Vec<[usize; 3]>> {
    let mut cdt = Cdt::new();
    let mut handles = Vec::with_capacity(points.len());
    // Index von `spade` → erster Ringpunkt an dieser Lage
    let mut source: Vec<usize> = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        let h = cdt.insert(mitigate_underflow(Point2::new(p.x, p.y))).ok()?;
        if h.index() == source.len() {
            source.push(i);
        }
        handles.push(h);
    }
    for r in rings {
        let n = r.len();
        for k in 0..n {
            let (p, q) = (handles[r.start + k], handles[r.start + (k + 1) % n]);
            if p == q || cdt.exists_constraint(p, q) {
                continue;
            }
            if !cdt.can_add_constraint(p, q) {
                return None;
            }
            cdt.add_constraint(p, q);
        }
    }
    let inner = inner_faces_by_parity(&cdt);
    Some(
        cdt.inner_faces()
            .filter(|f| inner[f.fix().index()])
            .map(|f| {
                let v = f.vertices();
                [
                    source[v[0].fix().index()],
                    source[v[1].fix().index()],
                    source[v[2].fix().index()],
                ]
            })
            .collect(),
    )
}

/// Innen/außen je Dreieck (Index = Flächenindex von `spade`) per Breitensuche ab dem Außengebiet:
/// Beim Überqueren einer Zwangskante wechselt die Zugehörigkeit. Deterministisch, O(Dreiecke).
fn inner_faces_by_parity(cdt: &Cdt) -> Vec<bool> {
    let n = cdt.num_all_faces();
    let mut state: Vec<Option<bool>> = vec![None; n];
    let mut queue = std::collections::VecDeque::new();
    // Startdreiecke: Nachbarn des Außengebiets
    for f in cdt.inner_faces() {
        for e in f.adjacent_edges() {
            if e.rev().face().is_outer() {
                let i = f.fix().index();
                if state[i].is_none() {
                    state[i] = Some(e.is_constraint_edge());
                    queue.push_back(f.fix());
                }
                break;
            }
        }
    }
    while let Some(h) = queue.pop_front() {
        let inside = state[h.index()].unwrap_or(false);
        for e in cdt.face(h).adjacent_edges() {
            let Some(nb) = e.rev().face().as_inner() else {
                continue;
            };
            let j = nb.fix().index();
            if state[j].is_none() {
                state[j] = Some(inside != e.is_constraint_edge());
                queue.push_back(nb.fix());
            }
        }
    }
    state.into_iter().map(|s| s.unwrap_or(false)).collect()
}

/// Triangulierung einer beschnittenen gekrümmten Fläche im Parameterraum.
/// `rings` sind die gemeinsamen 3D-Randpunkte je Schleife (aus den Kantenpolylinien, Reihenfolge wie
/// `g.loops`), damit Nachbarflächen exakt dieselben Randpunkte haben.
pub fn tessellate_trimmed(
    g: &FaceGeom,
    rings: &[Vec<DVec3>],
    disc: &Discretizer,
    mesh: &mut Mesh,
    face_index: usize,
) -> Result<(), MeshError> {
    let fail = || MeshError::Triangulation(face_index);
    let sign = if g.same_sense { 1.0 } else { -1.0 };
    let mut indexed: Vec<Vec<(DVec2, Option<usize>)>> = rings
        .iter()
        .map(|r| points_to_uv_indexed(&g.surface, g.same_sense, r))
        .collect();
    if indexed.is_empty() || indexed.iter().any(|r| r.len() < 3) {
        return Err(fail());
    }
    // Freiformflächen: Die Rückrechnung kann scheitern (Suche konvergiert nicht); ein Randpunkt,
    // dessen Parameter nicht auf ihn zurückführen, ergäbe ein offenes Netz. Dann Fehler.
    if matches!(g.surface, Surface::Nurbs(_)) {
        for (ring, pts) in indexed.iter().zip(rings.iter()) {
            for (q, src) in ring {
                if let Some(i) = src
                    && g.surface.point(q.x, q.y).distance(pts[*i]) > NURBS_EDGE_TOL
                {
                    return Err(MeshError::OffSurface(face_index));
                }
            }
        }
    }
    align_holes(&g.surface, &mut indexed).map_err(|_| fail())?;
    let polys: Vec<Vec<DVec2>> = indexed
        .iter()
        .map(|r| r.iter().map(|(p, _)| *p).collect())
        .collect();
    let map = uv_map(&g.surface, &polys);
    let scale = map.mean();
    let mut b = Builder {
        cdt: Cdt::new(),
        map: map.clone(),
        pts3: Vec::new(),
        uv: Vec::new(),
    };

    // Randpunkte: exakte 3D-Lage aus den gemeinsamen Polylinien; eingefügte Polpunkte werten die
    // Fläche aus. Ein so ausgewerteter Polpunkt weicht von der Polecke der Nachbarflächen nur um
    // Rundungsfehler (~1e-15·Radius) ab, weit unter der Schweißtoleranz beim Verschweißen über die
    // Position (`is_watertight`: 1e-6); die Pol-Kante selbst ist entartet und entfällt.
    let mut handles: Vec<Vec<FixedVertexHandle>> = Vec::with_capacity(indexed.len());
    for (ring, pts) in indexed.iter().zip(rings.iter()) {
        let mut hs = Vec::with_capacity(ring.len());
        for (p2, src) in ring {
            let p3 = match src {
                Some(i) => pts[*i],
                None => g.surface.point(p2.x, p2.y),
            };
            hs.push(b.add(*p2, p3).map_err(|_| fail())?);
        }
        handles.push(hs);
    }
    // Randpunkte haben die Indizes `0 .. n_boundary` (vor dem Raster eingefügt).
    let n_boundary = b.pts3.len();

    // Innenpunkte als Raster mit der Winkelschrittweite des Discretizers (gestreckt auf Längen):
    // so überspannt kein Dreieck mehr als etwa einen Winkelschritt und die Sehnentoleranz hält.
    // Freiformflächen: Teilung aus `nurbs_grid` (Bézier-Abschnitte, Knickwinkel, Sehnenprüfung)
    // über dem Parameterrechteck der Randpolygone.
    let grid = match &g.surface {
        Surface::Nurbs(d) => {
            let patch = nurbs_patch(d, &polys);
            let cells = |(nu, nv): (usize, usize)| {
                DVec2::new(
                    (patch.u1 - patch.u0) / nu as f64,
                    (patch.v1 - patch.v0) / nv as f64,
                )
            };
            // Unbeschnitten: Raster aus `nurbs_grid` (wie der Plan, auch für die Volumengenauigkeit).
            // Beschnitten: nötige Schrittweite je Richtung, Zellen fast isotrop, damit die
            // Delaunay-Triangulierung an den Beschnitten keine langen, schrägen Dreiecke bildet.
            let cell = if is_trimmed(g) {
                isotropic_cell(cells(disc.nurbs_needed_grid(d.surface(), &patch)), scale)
            } else {
                cells(disc.nurbs_grid(d.surface(), &patch))
            };
            grid_points_cells(&polys, &map, cell, sign)?
        }
        _ => {
            let step = TAU / disc.segments(TAU).max(8) as f64;
            let h = match g.surface {
                Surface::Torus { minor, .. } => step * minor.abs(),
                _ => step * scale.x,
            };
            grid_points(&polys, &map, h, sign)?
        }
    };
    ensure_room(b.pts3.len(), grid.len())?;
    for q in grid {
        b.add(q, g.surface.point(q.x, q.y)).map_err(|_| fail())?;
    }

    // Randkanten als Zwangskanten; sich kreuzende Randkanten wären ein Fehler der Eingabe.
    for hs in &handles {
        for i in 0..hs.len() {
            let (p, q) = (hs[i], hs[(i + 1) % hs.len()]);
            if p == q || b.cdt.exists_constraint(p, q) {
                continue;
            }
            if !b.cdt.can_add_constraint(p, q) {
                return Err(fail());
            }
            b.cdt.add_constraint(p, q);
        }
    }

    if matches!(g.surface, Surface::Nurbs(_)) {
        let exact =
            refine_to_chord_tolerance(&mut b, &g.surface, disc.chord_tolerance(), n_boundary)
                .map_err(|_| fail())?;
        if !exact {
            mesh.inexact_faces.push(Id::from_index(face_index));
        }
    }

    // Dreiecke der Fläche: Flutfüllung über die Triangulierung, beginnend außen (Außengebiet
    // gehört nicht zur Fläche); jede überquerte Zwangskante (Rand) wechselt innen/außen. Das ist
    // exakt bezüglich der Randkanten, auch für Splitter, deren Schwerpunkt auf dem Rand läge.
    let inner = inner_faces_by_parity(&b.cdt);
    let mut tris: Vec<[usize; 3]> = Vec::new();
    for f in b.cdt.inner_faces() {
        if !inner[f.fix().index()] {
            continue;
        }
        // `spade` liefert die Ecken gegen den Uhrzeigersinn (im gestreckten und damit auch im
        // ungestreckten Parameterraum) = natürliche Normale; bei !same_sense umdrehen.
        let v = f.vertices();
        let idx = [v[0].fix().index(), v[1].fix().index(), v[2].fix().index()];
        // Nur Dreiecke mit zusammenfallenden Ecken entfallen (Polpunkte: zwei UV-Punkte, eine
        // 3D-Lage), wie beim Raster in `tessellate_patch`. Flächenlose Dreiecke mit getrennten
        // Ecken bleiben: Am Kegelspitzenpunkt liegen Rasterpunkte einer Mantellinie mit der
        // Spitze auf einer Geraden; ohne diese Fächer entstünden T-Stöße (Netz nicht dicht).
        if degenerate(&b, idx) {
            continue;
        }
        tris.push(if g.same_sense {
            idx
        } else {
            [idx[0], idx[2], idx[1]]
        });
    }

    ensure_room(mesh.positions.len(), b.pts3.len())?;
    ensure_room(mesh.indices.len(), tris.len().saturating_mul(3))?;
    let base = index_u32(mesh.positions.len())?;
    for (p, q) in b.pts3.iter().zip(b.uv.iter()) {
        mesh.positions.push(*p);
        mesh.normals.push(g.surface.normal(q.x, q.y) * sign);
    }
    // Durch `ensure_room` passt jeder Index sicher in `u32`.
    for t in tris {
        mesh.indices.extend(t.iter().map(|&i| base + i as u32));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use planora_kernel::Solid;
    use planora_math::Frame;

    fn geom(surface: Surface) -> FaceGeom {
        FaceGeom {
            surface,
            same_sense: true,
            domain: None,
            loops: Vec::new(),
            tag: None,
        }
    }

    /// Rechteck im Parameterraum als 3D-Punkte (gegen den Uhrzeigersinn, bei `hole` andersherum).
    fn rect(s: &Surface, u: (f64, f64), v: (f64, f64), hole: bool) -> Vec<DVec3> {
        let mut c = vec![(u.0, v.0), (u.1, v.0), (u.1, v.1), (u.0, v.1)];
        if hole {
            c.reverse();
        }
        c.into_iter().map(|(a, b)| s.point(a, b)).collect()
    }

    fn disc() -> Discretizer {
        Discretizer::new(&Solid::new(), 0.001)
    }

    #[test]
    fn huge_grid_is_rejected_instead_of_exhausting_memory() {
        // Torus mit R ≫ r: Maschenweite folgt dem kleinen Rohrradius, in u wären es ~5·10⁵ Spalten.
        let s = Surface::Torus {
            frame: Frame::WORLD,
            major: 1000.0,
            minor: 0.001,
        };
        let ring = rect(&s, (0.0, 0.2), (0.0, 1.0), false);
        let mut mesh = Mesh::default();
        assert_eq!(
            tessellate_trimmed(&geom(s), &[ring], &disc(), &mut mesh, 3),
            Err(MeshError::TooLarge)
        );
    }

    #[test]
    fn hole_outside_every_period_is_an_error() {
        let s = Surface::Cylinder {
            frame: Frame::WORLD,
            radius: 1.0,
        };
        let outer = rect(&s, (0.0, 1.0), (0.0, 1.0), false);
        let hole = rect(&s, (3.0, 3.5), (0.2, 0.8), true);
        let mut mesh = Mesh::default();
        assert_eq!(
            tessellate_trimmed(&geom(s), &[outer, hole], &disc(), &mut mesh, 7),
            Err(MeshError::Triangulation(7))
        );
    }

    /// Ohne Innenraster (nur ein fein geteilter Rand) füllt die Verfeinerung die Fläche so,
    /// dass jedes Dreieck die Sehnentoleranz einhält.
    #[test]
    fn refinement_alone_reaches_the_chord_tolerance() {
        use planora_math::nurbs::{NurbsCurve, NurbsSurface};
        // Kugel (r = 2) als Drehfläche eines Halbkreises: in beiden Richtungen gekrümmt.
        let half = NurbsCurve::elliptic_arc(
            DVec3::ZERO,
            DVec3::new(0.0, 0.0, 2.0),
            DVec3::new(2.0, 0.0, 0.0),
            std::f64::consts::PI,
            -std::f64::consts::PI,
        )
        .unwrap();
        let s = Surface::nurbs(
            NurbsSurface::revolve(&half, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
            true,
            false,
        )
        .unwrap();
        let tol = 0.001;
        // Rechteck u ∈ [0, 1,5], v ∈ [0,3, 0,7], alle Seiten fein geteilt (Sehne ≪ tol).
        let n = 200;
        let side = |a: DVec2, b: DVec2| (0..n).map(move |k| a + (b - a) * (k as f64 / n as f64));
        let c = [
            DVec2::new(0.0, 0.3),
            DVec2::new(1.5, 0.3),
            DVec2::new(1.5, 0.7),
            DVec2::new(0.0, 0.7),
        ];
        let ring: Vec<DVec2> = (0..4).flat_map(|i| side(c[i], c[(i + 1) % 4])).collect();
        let mut b = Builder {
            cdt: Cdt::new(),
            map: UvMap::linear(DVec2::new(2.0, 6.0)),
            pts3: Vec::new(),
            uv: Vec::new(),
        };
        let hs: Vec<_> = ring
            .iter()
            .map(|q| b.add(*q, s.point(q.x, q.y)).unwrap())
            .collect();
        for i in 0..hs.len() {
            b.cdt.add_constraint(hs[i], hs[(i + 1) % hs.len()]);
        }
        assert!(
            refine_to_chord_tolerance(&mut b, &s, tol, ring.len()).unwrap(),
            "Toleranz erreicht"
        );
        assert!(b.pts3.len() > ring.len(), "Verfeinerung fügt Punkte ein");
        let inner = inner_faces_by_parity(&b.cdt);
        let mut worst = 0.0_f64;
        for f in b.cdt.inner_faces() {
            if !inner[f.fix().index()] {
                continue;
            }
            let v = f.vertices();
            let p = [0, 1, 2].map(|k| b.pts3[v[k].fix().index()]);
            for i in 0..=8 {
                for j in 0..=8 - i {
                    let (wa, wb) = (i as f64 / 8.0, j as f64 / 8.0);
                    let q = p[0] * wa + p[1] * wb + p[2] * (1.0 - wa - wb);
                    worst = worst.max((2.0 - q.length()).abs());
                }
            }
        }
        assert!(worst <= tol * 1.05, "Abweichung {worst}");
    }

    #[test]
    fn hole_inside_is_accepted() {
        let s = Surface::Cylinder {
            frame: Frame::WORLD,
            radius: 1.0,
        };
        let outer = rect(&s, (0.0, 1.0), (0.0, 1.0), false);
        let hole = rect(&s, (0.3, 0.6), (0.2, 0.8), true);
        let mut mesh = Mesh::default();
        assert_eq!(
            tessellate_trimmed(&geom(s), &[outer, hole], &disc(), &mut mesh, 7),
            Ok(())
        );
        assert!(!mesh.indices.is_empty());
    }
}
