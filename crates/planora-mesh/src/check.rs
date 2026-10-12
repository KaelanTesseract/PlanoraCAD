use crate::mesh::Mesh;
use planora_kernel::Solid;
use planora_math::DVec3;
use std::collections::HashMap;

/// Wahr, wenn das Netz nach dem Verschweißen nahe beieinanderliegender Punkte (Abstand < `eps`)
/// geschlossen und durchgängig orientiert ist: Jede gerichtete Kante kommt genau einmal vor,
/// ebenso ihre Gegenkante. Entartete Dreiecke werden ignoriert.
pub fn is_watertight(mesh: &Mesh, eps: f64) -> bool {
    let mut ids: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let remap: Vec<usize> = mesh
        .positions
        .iter()
        .map(|p| {
            let key = (
                (p.x / eps).round() as i64,
                (p.y / eps).round() as i64,
                (p.z / eps).round() as i64,
            );
            let next = ids.len();
            *ids.entry(key).or_insert(next)
        })
        .collect();

    let mut directed: HashMap<(usize, usize), u32> = HashMap::new();
    for t in mesh.indices.as_chunks::<3>().0 {
        let (a, b, c) = (
            remap[t[0] as usize],
            remap[t[1] as usize],
            remap[t[2] as usize],
        );
        if a == b || b == c || a == c {
            continue;
        }
        for edge in [(a, b), (b, c), (c, a)] {
            *directed.entry(edge).or_insert(0) += 1;
        }
    }
    !directed.is_empty()
        && directed
            .iter()
            .all(|(&(p, q), &n)| n == 1 && directed.get(&(q, p)) == Some(&1))
}

/// Relativer Abstand (bezogen auf die größte Koordinate der Fläche, mindestens 1), bis zu dem ein
/// Netzpunkt als Randpunkt (gemeinsamer Kantenpunkt) gilt. Randpunkte des Rasters über dem
/// Parameterrechteck und eingefügte Polpunkte weichen nur um Rundungsfehler ab (gemessen bis etwa
/// 6e-14 bei Koordinaten bis 100).
const BOUNDARY_MATCH_REL: f64 = 1e-9;

/// Punkte, sortiert nach `x`, für die Suche nach einem Punkt im Abstand `eps`.
struct PointIndex {
    sorted: Vec<(DVec3, usize)>,
}

impl PointIndex {
    fn new(points: impl Iterator<Item = (DVec3, usize)>) -> Self {
        let mut sorted: Vec<(DVec3, usize)> = points.collect();
        sorted.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        Self { sorted }
    }

    /// Kennung des nächsten Punkts höchstens `eps` von `p` entfernt.
    fn find(&self, p: DVec3, eps: f64) -> Option<usize> {
        let start = self.sorted.partition_point(|q| q.0.x < p.x - eps);
        self.sorted[start..]
            .iter()
            .take_while(|q| q.0.x <= p.x + eps)
            .map(|q| (q.0.distance(p), q.1))
            .filter(|(d, _)| *d <= eps)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    }

    /// Kleinste Kennung aller Punkte höchstens `eps` von `p` entfernt.
    fn find_min(&self, p: DVec3, eps: f64) -> Option<usize> {
        let start = self.sorted.partition_point(|q| q.0.x < p.x - eps);
        self.sorted[start..]
            .iter()
            .take_while(|q| q.0.x <= p.x + eps)
            .filter(|q| q.0.distance(p) <= eps)
            .map(|q| q.1)
            .min()
    }
}

/// Prüft die Randbedingung jeder Fläche eines Netzes von `solid` (aus [`crate::tessellate`]): Der
/// Rand ihrer Dreiecke (gerichtete Kanten, die sich innerhalb der Fläche nicht mit einer
/// Gegenkante aufheben) besteht genau aus den Strecken zwischen aufeinanderfolgenden Punkten ihrer
/// Schleifen (Kantenpolylinien `mesh.edge_lines` in Schleifenrichtung). Netzpunkte werden über
/// ihre Lage den Schleifenpunkten zugeordnet ([`BOUNDARY_MATCH_REL`]); Strecken einer Naht, die
/// eine Schleife hin und zurück durchläuft, heben sich auf wie die Nahtkanten des Netzes.
///
/// Gilt sie für jede Fläche, schließen die Flächen ohne T-Stöße aneinander an (beide Nachbarn
/// einer Kante haben genau deren Polylinienpunkte als Rand). Überlagerte Dreiecke im Inneren
/// zweier Flächen erkennt sie nicht. `Err` nennt die erste Fläche, für die sie nicht gilt.
pub fn check_face_boundaries(solid: &Solid, mesh: &Mesh) -> Result<(), String> {
    let polyline = |e: usize| -> Option<&Vec<DVec3>> {
        mesh.edge_lines
            .get(e)
            .filter(|l| l.edge.index() == e)
            .map(|l| &l.points)
    };
    for range in &mesh.face_ranges {
        let fid = range.face;
        let Some(face) = solid.faces.get(fid) else {
            return Err(format!("Fläche {} fehlt im Körper", fid.index()));
        };
        // Schleifenpunkte wie in `tessellate::loop_points`
        let mut rings: Vec<Vec<DVec3>> = Vec::new();
        for lp in std::iter::once(&face.outer).chain(face.inner.iter()) {
            let mut pts = Vec::new();
            for he in &lp.half_edges {
                let Some(poly) = polyline(he.edge.index()) else {
                    return Err(format!("Kante {} ohne Linienzug", he.edge.index()));
                };
                if he.forward {
                    pts.extend_from_slice(&poly[..poly.len() - 1]);
                } else {
                    pts.extend(poly[1..].iter().rev());
                }
            }
            rings.push(pts);
        }
        let first = range.first_index as usize;
        let tris = &mesh.indices[first..first + range.index_count as usize];
        let scale = rings
            .iter()
            .flatten()
            .copied()
            .chain(tris.iter().map(|&i| mesh.positions[i as usize]))
            .map(|p| p.abs().max_element())
            .fold(1.0, f64::max);
        let eps = BOUNDARY_MATCH_REL * scale;

        // Schleifenpunkte verschweißen: Kennung = kleinster Index eines Schleifenpunkts an
        // derselben Lage (Pole, Nahtenden).
        let ids: Vec<DVec3> = rings.iter().flatten().copied().collect();
        let index = PointIndex::new(ids.iter().copied().zip(0..));
        let canonical: Vec<usize> = ids
            .iter()
            .enumerate()
            .map(|(i, p)| index.find_min(*p, eps).unwrap_or(i))
            .collect();
        let mut ring_ids: Vec<Vec<usize>> = Vec::new();
        let mut k = 0;
        for ring in &rings {
            ring_ids.push(canonical[k..k + ring.len()].to_vec());
            k += ring.len();
        }
        let n_loop = ids.len();
        // Netzpunkte: Schleifenpunkt in der Nähe oder eigene Kennung hinter den Schleifenpunkten.
        let mut vertex_id: HashMap<u32, usize> = HashMap::new();
        let mut id_of = |v: u32| -> usize {
            *vertex_id.entry(v).or_insert_with(|| {
                index
                    .find(mesh.positions[v as usize], eps)
                    .map_or(n_loop + v as usize, |i| canonical[i])
            })
        };
        // Gerichtete Kanten als Kette: (a, b) mit a < b zählt +1, die Gegenrichtung −1.
        let mut chain: HashMap<(usize, usize), i64> = HashMap::new();
        let mut add = |a: usize, b: usize, k: i64| {
            if a == b {
                return;
            }
            let (key, s) = if a < b { ((a, b), k) } else { ((b, a), -k) };
            *chain.entry(key).or_insert(0) += s;
        };
        for t in tris.as_chunks::<3>().0 {
            let v = [id_of(t[0]), id_of(t[1]), id_of(t[2])];
            if v[0] == v[1] || v[1] == v[2] || v[0] == v[2] {
                continue;
            }
            for k in 0..3 {
                add(v[k], v[(k + 1) % 3], 1);
            }
        }
        for ring in &ring_ids {
            for k in 0..ring.len() {
                add(ring[k], ring[(k + 1) % ring.len()], -1);
            }
        }
        if let Some(((a, b), n)) = chain.iter().find(|(_, n)| **n != 0) {
            let at = |i: usize| {
                if i < n_loop {
                    format!("Randpunkt {:?}", ids[i])
                } else {
                    format!("Netzpunkt {:?}", mesh.positions[i - n_loop])
                }
            };
            return Err(format!(
                "Fläche {}: Rand der Dreiecke weicht von den Schleifen ab ({} – {}, Überschuss {n})",
                fid.index(),
                at(*a),
                at(*b)
            ));
        }
    }
    Ok(())
}
