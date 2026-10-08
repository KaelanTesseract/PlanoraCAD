use crate::mesh::Mesh;
use std::collections::HashMap;

/// Wahr, wenn das Netz nach dem Verschweißen nahe beieinanderliegender Punkte (Abstand < `eps`)
/// geschlossen und durchgängig orientiert ist: Jede gerichtete Kante kommt genau einmal vor,
/// ebenso ihre Gegenkante. Entartete Dreiecke werden ignoriert.
pub fn is_watertight(mesh: &Mesh, eps: f64) -> bool {
    let mut ids: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let remap: Vec<u32> = mesh
        .positions
        .iter()
        .map(|p| {
            let key = (
                (p.x / eps).round() as i64,
                (p.y / eps).round() as i64,
                (p.z / eps).round() as i64,
            );
            let next = ids.len() as u32;
            *ids.entry(key).or_insert(next)
        })
        .collect();

    let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
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
