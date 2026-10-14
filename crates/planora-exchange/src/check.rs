//! Prüfung eines Netzes (Spec §4) und Verschmelzen bitgleicher Punkte (P2, P3).

use crate::mesh::ExchangeMesh;
use planora_math::DVec3;

/// Ergebnis der Netzprüfung (Spec §4; `flipped_edges` ergänzt, Plan P2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshReport {
    pub triangles: usize,
    pub vertices: usize,
    /// Kanten, die nur zu einem Dreieck gehören.
    pub open_edges: usize,
    /// Kanten, die zu mehr als zwei Dreiecken gehören.
    pub non_manifold_edges: usize,
    /// Kanten zweier Dreiecke, die in derselben Richtung laufen (unstimmige Orientierung).
    pub flipped_edges: usize,
    /// Dreiecke mit doppeltem Punkt oder Flächeninhalt genau null (gezählt, nicht gelöscht).
    pub degenerate_triangles: usize,
    /// Jede Kante gehört zu genau zwei Dreiecken in entgegengesetzter Richtung.
    pub closed: bool,
}

impl MeshReport {
    /// Bericht mehrerer Körper: Zahlen addiert, geschlossen nur, wenn es alle sind (und es
    /// mindestens einen gibt).
    pub fn combine(reports: &[MeshReport]) -> MeshReport {
        let mut sum = MeshReport {
            closed: !reports.is_empty(),
            ..MeshReport::default()
        };
        for r in reports {
            sum.triangles += r.triangles;
            sum.vertices += r.vertices;
            sum.open_edges += r.open_edges;
            sum.non_manifold_edges += r.non_manifold_edges;
            sum.flipped_edges += r.flipped_edges;
            sum.degenerate_triangles += r.degenerate_triangles;
            sum.closed &= r.closed;
        }
        sum
    }
}

/// Prüft das Netz (Punktnummern müssen gültig sein, siehe [`ExchangeMesh::validate`]).
///
/// Jede Kante eines Dreiecks mit drei verschiedenen Punkten kommt in eine von zwei Listen:
/// „vorwärts“ (kleinere Nummer zuerst) oder „rückwärts“. Nach dem Sortieren zählt je Kante:
/// eins → offen, zwei in Gegenrichtung → in Ordnung, zwei in gleicher Richtung → falsch herum,
/// mehr → an mehr als zwei Dreiecken.
pub fn check_mesh(mesh: &ExchangeMesh) -> MeshReport {
    let tris = mesh.indices.as_chunks::<3>().0;
    let mut forward: Vec<u64> = Vec::new();
    let mut backward: Vec<u64> = Vec::new();
    let mut degenerate = 0;
    for t in tris {
        let [a, b, c] = *t;
        if a == b || b == c || a == c {
            degenerate += 1;
            continue;
        }
        let (pa, pb, pc) = (
            mesh.point(a as usize),
            mesh.point(b as usize),
            mesh.point(c as usize),
        );
        if (pb - pa).cross(pc - pa) == DVec3::ZERO {
            degenerate += 1;
        }
        for (p, q) in [(a, b), (b, c), (c, a)] {
            if p < q {
                forward.push(edge_key(p, q));
            } else {
                backward.push(edge_key(q, p));
            }
        }
    }
    forward.sort_unstable();
    backward.sort_unstable();
    let edges = forward.len() + backward.len();
    let (mut open, mut non_manifold, mut flipped) = (0, 0, 0);
    let (mut i, mut j) = (0, 0);
    loop {
        let key = match (forward.get(i), backward.get(j)) {
            (Some(&f), Some(&b)) => f.min(b),
            (Some(&f), None) => f,
            (None, Some(&b)) => b,
            (None, None) => break,
        };
        let nf = run_length(&forward, &mut i, key);
        let nb = run_length(&backward, &mut j, key);
        match (nf + nb, nf) {
            (1, _) => open += 1,
            (2, 1) => {}
            (2, _) => flipped += 1,
            _ => non_manifold += 1,
        }
    }
    MeshReport {
        triangles: tris.len(),
        vertices: mesh.positions.len(),
        open_edges: open,
        non_manifold_edges: non_manifold,
        flipped_edges: flipped,
        degenerate_triangles: degenerate,
        closed: edges > 0 && open == 0 && non_manifold == 0 && flipped == 0,
    }
}

fn edge_key(p: u32, q: u32) -> u64 {
    (u64::from(p) << 32) | u64::from(q)
}

/// Zählt die Einträge gleich `key` ab `*i` und rückt `*i` dahinter.
fn run_length(list: &[u64], i: &mut usize, key: u64) -> usize {
    let start = *i;
    while list.get(*i) == Some(&key) {
        *i += 1;
    }
    *i - start
}

/// Verschmilzt Punkte mit bitgleichen Koordinaten (−0 gilt als +0) und lässt Punkte weg, die
/// kein Dreieck benutzt (Plan P3). Die Punkte stehen danach in der Reihenfolge ihrer ersten
/// Verwendung; Dreiecke und ihre Reihenfolge bleiben. Kein Verschmelzen mit Toleranz (Spec §4).
/// `positions.len()` muss in `u32` passen (Aufrufer halten [`crate::MAX_VERTICES`] ein).
pub fn weld_exact(positions: &[[f32; 3]], indices: &[u32]) -> ExchangeMesh {
    let key = |i: u32| positions[i as usize].map(|c| if c == 0.0 { 0u32 } else { c.to_bits() });
    let mut order: Vec<u32> = (0..positions.len() as u32).collect();
    order.sort_unstable_by_key(|&i| (key(i), i));
    // Vertreter jeder Gruppe gleicher Punkte: die kleinste Nummer (dank der Sortierung vorn).
    let mut representative = vec![0u32; positions.len()];
    for group in order.chunk_by(|&a, &b| key(a) == key(b)) {
        for &i in group {
            representative[i as usize] = group[0];
        }
    }
    let mut new_id = vec![u32::MAX; positions.len()];
    let mut out = Vec::new();
    let mut out_indices = Vec::with_capacity(indices.len());
    for &i in indices {
        let r = representative[i as usize] as usize;
        if new_id[r] == u32::MAX {
            new_id[r] = out.len() as u32;
            out.push(positions[r]);
        }
        out_indices.push(new_id[r]);
    }
    ExchangeMesh {
        positions: out,
        indices: out_indices,
    }
}
