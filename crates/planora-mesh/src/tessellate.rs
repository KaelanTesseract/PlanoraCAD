use crate::discretize::Discretizer;
use crate::mesh::{EdgeLine, FaceRange, Mesh, TessellationOptions};
use planora_kernel::{Face, Id, Loop, Solid, Surface, UvRect};
use planora_math::{DVec2, DVec3, Frame, TAU};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MeshError {
    #[error("Fläche {0}: Triangulation fehlgeschlagen")]
    Triangulation(usize),
    #[error("Fläche {0}: gekrümmte Fläche ohne Parameterbereich")]
    MissingPatch(usize),
    #[error("Netz zu groß: mehr als 2³² Eckpunkte oder Indizes")]
    TooLarge,
    #[error("Fläche {0}: ein Randpunkt lässt sich nicht auf die Freiformfläche zurückrechnen")]
    OffSurface(usize),
}

/// Wandelt eine Anzahl in einen `u32`-Index um, ohne still abzuschneiden.
pub(crate) fn index_u32(n: usize) -> Result<u32, MeshError> {
    u32::try_from(n).map_err(|_| MeshError::TooLarge)
}

/// Prüft vorab, ob nach dem Anhängen von `extra` Elementen alle Indizes noch in `u32` passen.
pub(crate) fn ensure_room(current: usize, extra: usize) -> Result<(), MeshError> {
    current
        .checked_add(extra)
        .filter(|&n| n <= u32::MAX as usize)
        .map(|_| ())
        .ok_or(MeshError::TooLarge)
}

/// Zerlegt einen Körper in Dreiecke. Alle Dreiecke sind so orientiert, dass sie nach außen zeigen.
pub fn tessellate(solid: &Solid, options: &TessellationOptions) -> Result<Mesh, MeshError> {
    tessellate_with(solid, &Discretizer::new(solid, options.chord_tolerance))
}

/// Wie [`tessellate`], mit vorgegebener Teilung (etwa [`Discretizer::for_solids`], damit mehrere
/// Körper vergleichbar vernetzt werden).
pub fn tessellate_with(solid: &Solid, disc: &Discretizer) -> Result<Mesh, MeshError> {
    let disc = *disc;
    let polylines: Vec<Vec<DVec3>> = (0..solid.edges.len())
        .map(|i| disc.edge_polyline(solid, Id::from_index(i)))
        .collect();

    let mut mesh = Mesh::default();
    for (fid, face) in solid.faces.iter() {
        let first_index = index_u32(mesh.indices.len())?;
        match &face.surface {
            Surface::Plane { frame } => {
                tessellate_planar(face, frame, &polylines, &mut mesh).map_err(|e| match e {
                    PlanarError::Triangulation => MeshError::Triangulation(fid.index()),
                    PlanarError::Mesh(e) => e,
                })?;
            }
            _ => {
                let g = planora_kernel::boolean::face_geom(solid, fid);
                // Freiformflächen immer über die Triangulierung im Parameterraum: Der Rand
                // besteht dann genau aus den gemeinsamen Kantenpunkten (auch wenn die Kanten
                // analytisch sind und ihre Teilpunkte nicht auf Parameterlinien des Rasters
                // liegen), das Netz bleibt geschlossen.
                //
                // Analytische Flächen ohne Beschnitt nehmen das Raster über dem Parameterrechteck
                // (`tessellate_patch`), aber nur, wenn dessen Randpunkte genau die gemeinsamen
                // Kantenpunkte sind. Sonst (etwa ein Mantel, dessen Randkreis in mehrere Kanten
                // geteilt ist: gleiche Parameterfläche, aber zusätzliche Teilpunkte) hätte das
                // Raster andere Randpunkte als die Nachbarflächen, das Netz wäre offen. Dann
                // bekommen Mantelflächen von Zylinder und Kegel (gerade in `v`) einen Streifen
                // zwischen ihren beiden Randzeilen mit genau den Kantenpunkten
                // (`tessellate_strip`); die Triangulierung im Parameterraum würde das Innere mit
                // einem gleichmäßigen Raster füllen und bei großen Körpern ein Vielfaches an
                // Dreiecken erzeugen. Geht auch das nicht (Kugel, Ring, ein Schlitz im Mantel),
                // bleibt die Triangulierung im Parameterraum.
                let rings = || -> Vec<Vec<DVec3>> {
                    std::iter::once(&face.outer)
                        .chain(face.inner.iter())
                        .map(|lp| loop_points(lp, &polylines))
                        .collect()
                };
                let mut done = false;
                if !matches!(face.surface, Surface::Nurbs(_)) && !crate::trimmed::is_trimmed(&g) {
                    let patch = face.patch.ok_or(MeshError::MissingPatch(fid.index()))?;
                    let rings = rings();
                    if patch_boundary_matches(face, &patch, &disc, &rings) {
                        tessellate_patch(face, &patch, &disc, &mut mesh)?;
                        done = true;
                    } else if let Some(strip) = strip_rows(face, &patch, &rings) {
                        tessellate_strip(face, &strip, &mut mesh)?;
                        done = true;
                    }
                }
                if !done {
                    crate::trimmed::tessellate_trimmed(
                        &g,
                        &rings(),
                        &disc,
                        &mut mesh,
                        fid.index(),
                    )?;
                }
            }
        }
        mesh.face_ranges.push(FaceRange {
            face: fid,
            first_index,
            index_count: index_u32(mesh.indices.len())? - first_index,
        });
    }
    mesh.edge_lines = polylines
        .into_iter()
        .enumerate()
        .map(|(i, points)| EdgeLine {
            edge: Id::from_index(i),
            points,
        })
        .collect();
    Ok(mesh)
}

fn loop_points(lp: &Loop, polylines: &[Vec<DVec3>]) -> Vec<DVec3> {
    let mut pts = Vec::new();
    for he in &lp.half_edges {
        let poly = &polylines[he.edge.index()];
        if he.forward {
            pts.extend_from_slice(&poly[..poly.len() - 1]);
        } else {
            pts.extend(poly[1..].iter().rev());
        }
    }
    pts
}

enum PlanarError {
    Triangulation,
    Mesh(MeshError),
}

impl From<MeshError> for PlanarError {
    fn from(e: MeshError) -> Self {
        PlanarError::Mesh(e)
    }
}

fn tessellate_planar(
    face: &Face,
    frame: &Frame,
    polylines: &[Vec<DVec3>],
    mesh: &mut Mesh,
) -> Result<(), PlanarError> {
    let normal = if face.same_sense { frame.z } else { -frame.z };
    let rings: Vec<Vec<DVec3>> = std::iter::once(&face.outer)
        .chain(face.inner.iter())
        .map(|lp| loop_points(lp, polylines))
        .collect();

    let mut flat: Vec<f64> = Vec::new();
    let mut holes: Vec<usize> = Vec::new();
    for (i, ring) in rings.iter().enumerate() {
        if ring.len() < 3 {
            return Err(PlanarError::Triangulation);
        }
        if i > 0 {
            holes.push(flat.len() / 2);
        }
        for p in ring {
            let local = frame.to_local(*p);
            flat.push(local.x);
            flat.push(local.y);
        }
    }
    let mut triangles =
        earcutr::earcut(&flat, &holes, 2).map_err(|_| PlanarError::Triangulation)?;
    // `earcut` entfernt beim Verbinden der Löcher Punkte, die mit ihren Nachbarn auf einer
    // Geraden liegen (etwa wenn die Brücke zu einem Loch genau auf dessen Kante trifft); die
    // Nachbarflächen behalten diese Punkte, das Netz wäre an einem T-Stoß offen. Dann wird
    // dieselbe Fläche mit der eingeschränkten Delaunay-Triangulierung aufgebaut, die jeden
    // Randpunkt als Ecke behält. Gute `earcut`-Ergebnisse bleiben unverändert.
    let mut ranges = Vec::with_capacity(rings.len());
    let mut start = 0;
    for ring in &rings {
        ranges.push(start..start + ring.len());
        start += ring.len();
    }
    if !uses_every_boundary_edge_once(&triangles, &ranges) {
        let pts: Vec<DVec2> = flat
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| DVec2::new(c[0], c[1]))
            .collect();
        triangles = crate::trimmed::triangulate_planar_cdt(&pts, &ranges)
            .ok_or(PlanarError::Triangulation)?
            .into_iter()
            .flatten()
            .collect();
    }

    ensure_room(mesh.positions.len(), flat.len() / 2)?;
    ensure_room(mesh.indices.len(), triangles.len())?;
    let base = index_u32(mesh.positions.len())?;
    for p in rings.iter().flatten() {
        mesh.positions.push(*p);
        mesh.normals.push(normal);
    }
    // Durch `ensure_room` passen `base + a` usw. sicher in `u32`.
    for t in triangles.as_chunks::<3>().0 {
        let (a, mut b, mut c) = (t[0], t[1], t[2]);
        let area2 = (flat[2 * b] - flat[2 * a]) * (flat[2 * c + 1] - flat[2 * a + 1])
            - (flat[2 * c] - flat[2 * a]) * (flat[2 * b + 1] - flat[2 * a + 1]);
        // Gegen den Uhrzeigersinn in der Ebene = Normale +z des Frames.
        if area2 < 0.0 {
            std::mem::swap(&mut b, &mut c);
        }
        if !face.same_sense {
            std::mem::swap(&mut b, &mut c);
        }
        mesh.indices
            .extend_from_slice(&[base + a as u32, base + b as u32, base + c as u32]);
    }
    Ok(())
}

/// Ist die Triangulierung am Rand dicht? Jede Ringkante (Indizes `k`, `k + 1` je Ring) kommt in
/// genau einem Dreieck vor, jede andere Kante in genau zwei.
fn uses_every_boundary_edge_once(triangles: &[usize], rings: &[std::ops::Range<usize>]) -> bool {
    let key = |a: usize, b: usize| (a.min(b), a.max(b));
    let mut count: HashMap<(usize, usize), u32> = HashMap::new();
    for t in triangles.as_chunks::<3>().0 {
        for k in 0..3 {
            *count.entry(key(t[k], t[(k + 1) % 3])).or_insert(0) += 1;
        }
    }
    let mut boundary = 0;
    for r in rings {
        let n = r.len();
        for k in 0..n {
            let e = key(r.start + k, r.start + (k + 1) % n);
            if count.get(&e) != Some(&1) {
                return false;
            }
            boundary += 1;
        }
    }
    let ones = count.values().filter(|c| **c == 1).count();
    ones == boundary && count.values().all(|c| *c == 1 || *c == 2)
}

/// Teilung `(nu, nv)` des Rasters über dem Parameterrechteck einer analytischen Fläche.
fn patch_counts(face: &Face, patch: &UvRect, disc: &Discretizer) -> (usize, usize) {
    let nu = disc.segments(patch.u1 - patch.u0);
    let nv = match face.surface {
        Surface::Sphere { .. } | Surface::Torus { .. } => disc.segments(patch.v1 - patch.v0),
        _ => 1,
    };
    (nu, nv)
}

/// Parameter des Rasterpunkts `(i, j)` (genau wie in [`tessellate_patch`]).
fn patch_uv(patch: &UvRect, (nu, nv): (usize, usize), i: usize, j: usize) -> (f64, f64) {
    (
        patch.u0 + (patch.u1 - patch.u0) * i as f64 / nu as f64,
        patch.v0 + (patch.v1 - patch.v0) * j as f64 / nv as f64,
    )
}

/// Relativer Abstand (bezogen auf die größte Koordinate, mindestens 1), bis zu dem ein Randpunkt
/// des Rasters und ein gemeinsamer Kantenpunkt als derselbe Punkt gelten. Beide werten dieselbe
/// Geometrie nur mit verschiedenen Formeln aus (Kurve gegen Fläche); gemessen weichen sie in
/// geschlossenen Netzen höchstens um etwa 6e-14 ab (Kugel r = 100), echte Teilpunkte liegen
/// dagegen mindestens etwa 1e-4 neben dem nächsten Rasterpunkt.
const PATCH_MATCH_REL: f64 = 1e-12;

/// Sind die Randpunkte des Rasters über dem Parameterrechteck (Zeilen `j = 0, nv`, Spalten
/// `i = 0, nu`) als Punktmenge dieselben wie die gemeinsamen Kantenpunkte der Schleifen `rings`?
/// Nur dann schließt das Raster dicht an die Nachbarflächen an.
fn patch_boundary_matches(
    face: &Face,
    patch: &UvRect,
    disc: &Discretizer,
    rings: &[Vec<DVec3>],
) -> bool {
    let (nu, nv) = patch_counts(face, patch, disc);
    let mut grid: Vec<DVec3> = Vec::with_capacity(2 * (nu + nv) + 4);
    for j in 0..=nv {
        for i in 0..=nu {
            if i == 0 || i == nu || j == 0 || j == nv {
                let (u, v) = patch_uv(patch, (nu, nv), i, j);
                grid.push(face.surface.point(u, v));
            }
        }
    }
    let loops: Vec<DVec3> = rings.iter().flatten().copied().collect();
    let scale = grid
        .iter()
        .chain(loops.iter())
        .map(|p| p.abs().max_element())
        .fold(1.0, f64::max);
    if !scale.is_finite() {
        return false;
    }
    let eps = PATCH_MATCH_REL * scale;
    same_point_sets(&grid, &loops, eps)
}

/// Hat jeder Punkt von `a` einen Punkt von `b` höchstens `eps` entfernt und umgekehrt?
pub(crate) fn same_point_sets(a: &[DVec3], b: &[DVec3], eps: f64) -> bool {
    use crate::check::PointIndex;
    if !(eps > 0.0 && eps.is_finite()) {
        return false;
    }
    let ia = PointIndex::new(a.iter().copied().zip(0..), eps);
    let ib = PointIndex::new(b.iter().copied().zip(0..), eps);
    a.iter().all(|p| ib.find(*p).is_some()) && b.iter().all(|p| ia.find(*p).is_some())
}

/// Randzeile eines Streifens: Parameter `u` (aufsteigend von `u0` bis `u1`) und 3D-Lage (genau der
/// gemeinsame Kantenpunkt).
type StripRow = Vec<(f64, DVec3)>;

/// Die beiden Randzeilen `v = v0` und `v = v1` einer Mantelfläche ([`strip_rows`]).
struct Strip {
    v: [f64; 2],
    rows: [StripRow; 2],
}

/// Relativer Abstand, bis zu dem ein Kantenpunkt auf einer Randzeile (`v`) oder an einem Ende
/// (`u`) des Parameterrechtecks liegt. Die Rückrechnung der Punkte auf Parameter ist auf
/// Rundungsfehler genau; Punkte im Inneren einer Seite (etwa eine geteilte Naht) liegen weit
/// darüber.
const STRIP_REL: f64 = 1e-9;

/// Zerlegt die Schleifenpunkte `rings` einer unbeschnittenen Mantelfläche von Zylinder oder
/// Kegel (gerade in `v`) in die Randzeilen `v = v0` und `v = v1`, jeweils nach `u` geordnet und
/// von `u0` bis `u1` reichend (bei einem vollen Umlauf mit dem Nahtpunkt an beiden Enden). Eine
/// Kegelspitze wird zu einer Zeile mit denselben `u` wie die andere Zeile (je ein Spitzenpunkt
/// mit eigener Normale, wie beim Raster). Die Schleife darf nur an den Enden `u0`/`u1` (Naht
/// bzw. Seitenkanten) zwischen den Zeilen wechseln, genau zweimal; sonst (etwa ein Schlitz quer
/// durch den Mantel, schmaler als die Flächentoleranz von `is_trimmed`) wäre der Streifen über
/// den Schlitz gespannt. `None`, wenn das nicht geht (andere Flächenart, Löcher, ein Punkt
/// abseits der Randzeilen etwa auf einer geteilten Naht, ein Wechsel abseits der Enden); dann
/// bleibt es bei der Triangulierung im Parameterraum.
fn strip_rows(face: &Face, patch: &UvRect, rings: &[Vec<DVec3>]) -> Option<Strip> {
    if !matches!(
        face.surface,
        Surface::Cylinder { .. } | Surface::Cone { .. }
    ) || rings.len() != 1
    {
        return None;
    }
    let (u0, u1, v0, v1) = (patch.u0, patch.u1, patch.v0, patch.v1);
    let span = u1 - u0;
    if !(span > 0.0 && span <= TAU * (1.0 + STRIP_REL) && v1 != v0 && (v1 - v0).is_finite()) {
        return None;
    }
    let full = (span - TAU).abs() <= STRIP_REL * TAU;
    let eps_u = STRIP_REL * TAU;
    let eps_v = STRIP_REL * (v1 - v0).abs().max(1.0);
    let scale = rings[0]
        .iter()
        .map(|p| p.abs().max_element())
        .fold(1.0, f64::max);
    let weld = 1e-12 * scale;
    // Zeile und `u` je Schleifenpunkt, in Schleifenreihenfolge.
    let mut seq: Vec<(usize, f64)> = Vec::with_capacity(rings[0].len());
    let mut rows: [StripRow; 2] = [Vec::new(), Vec::new()];
    for p in &rings[0] {
        let (u, v) = face.surface.project(*p);
        let row = if (v - v0).abs() <= eps_v {
            0
        } else if (v - v1).abs() <= eps_v {
            1
        } else {
            return None;
        };
        let mut uu = u0 + (u - u0).rem_euclid(TAU);
        if uu >= u0 + TAU - eps_u {
            uu = u0;
        }
        if !uu.is_finite() || uu > u1 + eps_u {
            return None;
        }
        let uu = uu.min(u1);
        seq.push((row, uu));
        rows[row].push((uu, *p));
    }
    // Kegelspitze: alle Punkte einer Zeile an einer Stelle.
    let mut apex = [false; 2];
    for (k, row) in rows.iter().enumerate() {
        let first = row.first()?.1;
        apex[k] = row.iter().all(|(_, p)| p.distance(first) <= weld);
    }
    if apex[0] && apex[1] {
        return None;
    }
    // Wechsel zwischen den Zeilen nur an den Enden, genau zwei.
    let at_end = |u: f64| (u - u0).abs() <= eps_u || (u - u1).abs() <= eps_u;
    let mut changes = 0;
    for k in 0..seq.len() {
        let (a, b) = (seq[k], seq[(k + 1) % seq.len()]);
        if a.0 != b.0 {
            changes += 1;
            if [a, b].iter().any(|(r, u)| !apex[*r] && !at_end(*u)) {
                return None;
            }
        }
    }
    if changes != 2 {
        return None;
    }
    for ((row, v), is_apex) in rows.iter_mut().zip([v0, v1]).zip(apex) {
        if is_apex {
            continue;
        }
        row.sort_by(|a, b| a.0.total_cmp(&b.0));
        row.dedup_by(|b, a| a.1.distance(b.1) <= weld);
        if (row[0].0 - u0).abs() > eps_u {
            return None;
        }
        row[0].0 = u0;
        let last = row.len() - 1;
        if full {
            // Nahtpunkt am Ende noch einmal (gleiche Lage bei u1).
            let seam = row[0].1;
            if face.surface.point(u1, v).distance(seam) > 1e-9 * scale
                || (row[last].0 - u1).abs() <= eps_u
            {
                return None;
            }
            row.push((u1, seam));
        } else {
            if (row[last].0 - u1).abs() > eps_u {
                return None;
            }
            row[last].0 = u1;
        }
        if row.len() < 2 {
            return None;
        }
    }
    // Spitze: ein Punkt je `u` der anderen Zeile.
    for k in 0..2 {
        if apex[k] {
            let tip = rows[k][0].1;
            rows[k] = rows[1 - k].iter().map(|(u, _)| (*u, tip)).collect();
        }
    }
    Some(Strip { v: [v0, v1], rows })
}

/// Vernetzt eine Mantelfläche von Zylinder oder Kegel als Streifen zwischen ihren Randzeilen
/// ([`strip_rows`]): Die Zeilen werden nach `u` verzahnt, jedes Dreieck überspannt in `u`
/// höchstens einen Abschnitt einer der beiden Zeilen (die Sehnentoleranz der Kantenteilung gilt
/// damit auch im Inneren; in `v` ist die Fläche gerade). Randpunkte sind genau die
/// Kantenpunkte, das Netz schließt dicht an die Nachbarflächen an.
fn tessellate_strip(face: &Face, strip: &Strip, mesh: &mut Mesh) -> Result<(), MeshError> {
    let [bottom, top] = &strip.rows;
    let (nb, nt) = (bottom.len(), top.len());
    let sign = if face.same_sense { 1.0 } else { -1.0 };
    ensure_room(mesh.positions.len(), nb + nt)?;
    ensure_room(mesh.indices.len(), 3 * (nb + nt))?;
    let base = index_u32(mesh.positions.len())?;
    for (row, v) in [bottom, top].into_iter().zip(strip.v) {
        for (u, p) in row {
            mesh.positions.push(*p);
            mesh.normals.push(face.surface.normal(*u, v) * sign);
        }
    }
    // Durch `ensure_room` passt jeder Index sicher in `u32`.
    let b = |i: usize| base + i as u32;
    let t = |j: usize| base + (nb + j) as u32;
    let (mut i, mut j) = (0, 0);
    while i + 1 < nb || j + 1 < nt {
        let step_bottom = j + 1 == nt || (i + 1 < nb && bottom[i + 1].0 <= top[j + 1].0);
        if step_bottom {
            push_triangle(mesh, [b(i), b(i + 1), t(j)], face.same_sense);
            i += 1;
        } else {
            push_triangle(mesh, [b(i), t(j + 1), t(j)], face.same_sense);
            j += 1;
        }
    }
    Ok(())
}

fn tessellate_patch(
    face: &Face,
    patch: &UvRect,
    disc: &Discretizer,
    mesh: &mut Mesh,
) -> Result<(), MeshError> {
    let (nu, nv) = patch_counts(face, patch, disc);
    let sign = if face.same_sense { 1.0 } else { -1.0 };
    let vertex_count = (nu + 1).checked_mul(nv + 1).ok_or(MeshError::TooLarge)?;
    let index_count = nu
        .checked_mul(nv)
        .and_then(|n| n.checked_mul(6))
        .ok_or(MeshError::TooLarge)?;
    ensure_room(mesh.positions.len(), vertex_count)?;
    ensure_room(mesh.indices.len(), index_count)?;
    let base = index_u32(mesh.positions.len())?;
    for j in 0..=nv {
        for i in 0..=nu {
            let (u, v) = patch_uv(patch, (nu, nv), i, j);
            mesh.positions.push(face.surface.point(u, v));
            mesh.normals.push(face.surface.normal(u, v) * sign);
        }
    }
    // Durch `ensure_room` passt jeder Index sicher in `u32`.
    let idx = |i: usize, j: usize| base + (j * (nu + 1) + i) as u32;
    for j in 0..nv {
        for i in 0..nu {
            let (a, b, c, d) = (idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
            for tri in [[a, b, c], [a, c, d]] {
                push_triangle(mesh, tri, face.same_sense);
            }
        }
    }
    Ok(())
}

/// Fügt ein Dreieck hinzu und lässt entartete Dreiecke (an den Polen) weg.
fn push_triangle(mesh: &mut Mesh, tri: [u32; 3], keep_order: bool) {
    const EPS: f64 = 1e-12;
    let p = [
        mesh.positions[tri[0] as usize],
        mesh.positions[tri[1] as usize],
        mesh.positions[tri[2] as usize],
    ];
    if p[0].distance(p[1]) < EPS || p[1].distance(p[2]) < EPS || p[0].distance(p[2]) < EPS {
        return;
    }
    let t = if keep_order {
        tri
    } else {
        [tri[0], tri[2], tri[1]]
    };
    mesh.indices.extend_from_slice(&t);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_counts_beyond_u32_are_rejected() {
        assert_eq!(index_u32(7), Ok(7));
        assert_eq!(index_u32(u32::MAX as usize), Ok(u32::MAX));
        assert_eq!(index_u32(u32::MAX as usize + 1), Err(MeshError::TooLarge));
        assert_eq!(ensure_room(10, 20), Ok(()));
        assert_eq!(ensure_room(u32::MAX as usize, 0), Ok(()));
        assert_eq!(ensure_room(u32::MAX as usize, 1), Err(MeshError::TooLarge));
        assert_eq!(ensure_room(usize::MAX, 1), Err(MeshError::TooLarge));
    }

    #[test]
    fn point_sets_compare_both_ways_within_eps() {
        let a = [DVec3::ZERO, DVec3::X, DVec3::X, DVec3::Y];
        let b = [DVec3::Y, DVec3::new(1e-13, 0.0, 0.0), DVec3::X];
        assert!(same_point_sets(&a, &b, 1e-12));
        // ein zusätzlicher Teilpunkt auf einer Seite
        let c = [DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::new(0.5, 0.0, 0.0)];
        assert!(!same_point_sets(&a, &c, 1e-12));
        assert!(!same_point_sets(&c, &a, 1e-12));
        assert!(!same_point_sets(
            &a,
            &[DVec3::new(1e-9, 0.0, 0.0), DVec3::X, DVec3::Y],
            1e-12
        ));
    }
}
