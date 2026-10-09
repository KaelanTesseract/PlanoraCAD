//! Beschnittene gekrümmte Flächen: eingeschränkte Delaunay-Triangulierung im Parameterraum.

use crate::discretize::Discretizer;
use crate::mesh::Mesh;
use crate::tessellate::{MeshError, ensure_room, index_u32};
use planora_kernel::Surface;
use planora_kernel::boolean::{
    FaceGeom, loop_points, oriented_area, points_to_uv, points_to_uv_indexed, winding,
};
use planora_math::{DVec2, DVec3, TAU};
use spade::handles::FixedVertexHandle;
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation, mitigate_underflow};

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
        Surface::Plane { .. } => DVec2::ONE,
    }
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
    scale: DVec2,
    pts3: Vec<DVec3>,
    uv: Vec<DVec2>,
}

impl Builder {
    /// Punkt einfügen (gleiche Lage ergibt denselben Punkt) und seine 3D-Lage merken.
    fn add(&mut self, p2: DVec2, p3: DVec3) -> Result<FixedVertexHandle, ()> {
        let s = p2 * self.scale;
        let q = mitigate_underflow(Point2::new(s.x, s.y));
        let h = self.cdt.insert(q).map_err(|_| ())?;
        if h.index() == self.pts3.len() {
            self.pts3.push(p3);
            self.uv.push(p2);
        }
        Ok(h)
    }
}

/// Verschiebt die Löcher einer periodischen Fläche um ganze Perioden, sodass sie in der
/// äußeren Schleife liegen (jede Schleife wird für sich aufgerollt). Liegt ein Loch bei keiner
/// Verschiebung in der äußeren Schleife, ist die Fläche widersprüchlich: `Err`.
fn align_holes(surface: &Surface, rings: &mut [Vec<(DVec2, Option<usize>)>]) -> Result<(), ()> {
    if rings.len() < 2 {
        return Ok(());
    }
    let outer: Vec<DVec2> = rings[0].iter().map(|(p, _)| *p).collect();
    let dus: &[f64] = if surface.is_periodic() {
        &[0.0, -TAU, TAU, -2.0 * TAU, 2.0 * TAU]
    } else {
        &[0.0]
    };
    let dvs: &[f64] = if matches!(surface, Surface::Torus { .. }) {
        &[0.0, -TAU, TAU]
    } else {
        &[0.0]
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
    scale: DVec2,
    h: f64,
    sign: f64,
) -> Result<Vec<DVec2>, MeshError> {
    let sp: Vec<Vec<DVec2>> = polys
        .iter()
        .map(|p| p.iter().map(|q| *q * scale).collect())
        .collect();
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for p in sp.iter().flatten() {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    // auch NaN führt zu einem leeren Raster
    let usable = h > 0.0 && hi.x > lo.x && hi.y > lo.y;
    if !usable {
        return Ok(Vec::new());
    }
    let (nu_f, nv_f) = (
        ((hi.x - lo.x) / h).ceil().max(1.0),
        ((hi.y - lo.y) / h).ceil().max(1.0),
    );
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
                out.push(q / scale);
            }
        }
    }
    Ok(out)
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
    align_holes(&g.surface, &mut indexed).map_err(|_| fail())?;
    let polys: Vec<Vec<DVec2>> = indexed
        .iter()
        .map(|r| r.iter().map(|(p, _)| *p).collect())
        .collect();
    let inside = |q: DVec2| -> bool {
        let w: i32 = polys.iter().map(|p| winding(p, q)).sum();
        f64::from(w) * sign >= 1.0
    };

    let scale = uv_scale(&g.surface, &polys);
    let mut b = Builder {
        cdt: Cdt::new(),
        scale,
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

    // Innenpunkte als Raster mit der Winkelschrittweite des Discretizers (gestreckt auf Längen):
    // so überspannt kein Dreieck mehr als etwa einen Winkelschritt und die Sehnentoleranz hält.
    let step = TAU / disc.segments(TAU).max(8) as f64;
    let h = match g.surface {
        Surface::Torus { minor, .. } => step * minor.abs(),
        _ => step * scale.x,
    };
    let grid = grid_points(&polys, scale, h, sign)?;
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

    // Dreiecke übernehmen, deren Schwerpunkt in der Fläche liegt
    let mut tris: Vec<[usize; 3]> = Vec::new();
    for f in b.cdt.inner_faces() {
        let v = f.vertices();
        let idx = [v[0].fix().index(), v[1].fix().index(), v[2].fix().index()];
        let centroid = (b.uv[idx[0]] + b.uv[idx[1]] + b.uv[idx[2]]) / 3.0;
        if !inside(centroid) {
            continue;
        }
        let (p0, p1, p2) = (b.pts3[idx[0]], b.pts3[idx[1]], b.pts3[idx[2]]);
        if (p1 - p0).cross(p2 - p0).length() < 1e-14 {
            continue; // entartet (an einem Pol)
        }
        // gegen den Uhrzeigersinn im Parameterraum = natürliche Normale; bei !same_sense umdrehen
        let area2 = (b.uv[idx[1]] - b.uv[idx[0]]).perp_dot(b.uv[idx[2]] - b.uv[idx[0]]);
        tris.push(if (area2 > 0.0) == g.same_sense {
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
