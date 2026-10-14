//! Normalen je Punkt mit harten Kanten (Spec §6, Plan P8): An einem Punkt teilen sich die
//! anliegenden Dreiecke eine Normale, solange ihre Flächennormalen weniger als den Grenzwinkel
//! von der des ersten Dreiecks der Gruppe abweichen; sonst bekommt der Punkt eine weitere Kopie.

use crate::mesh::ExchangeMesh;
use planora_math::DVec3;

/// Grenzwinkel für glatte Normalen (Spec §6: unter 30° glatt, sonst harte Kante).
pub const SMOOTH_ANGLE_DEG: f64 = 30.0;

/// Netz mit einer Normale je Punkt; Punkte an harten Kanten sind verdoppelt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shaded {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// Normalen je Punkt (Netz muss gültig sein). Gruppen entstehen in der Reihenfolge der Ecken;
/// die Normale einer Gruppe ist die nach Fläche gewichtete Summe, normiert (ohne Fläche +z).
pub fn shade(mesh: &ExchangeMesh, max_angle_deg: f64) -> Shaded {
    let tris = mesh.indices.as_chunks::<3>().0;
    let cos_limit = max_angle_deg.to_radians().cos();
    let weighted: Vec<DVec3> = tris
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.point(i as usize));
            (b - a).cross(c - a)
        })
        .collect();
    let unit: Vec<DVec3> = weighted.iter().map(|n| n.normalize_or_zero()).collect();
    // Ecken je Punkt: Punkt v hat die Ecken corners[start[v]..start[v + 1]].
    let mut start = vec![0usize; mesh.positions.len() + 1];
    for &i in &mesh.indices {
        start[i as usize] += 1;
    }
    let mut sum = 0;
    for s in &mut start {
        let count = *s;
        *s = sum;
        sum += count;
    }
    let mut fill = start.clone();
    let mut corners = vec![0usize; mesh.indices.len()];
    for (c, &i) in mesh.indices.iter().enumerate() {
        corners[fill[i as usize]] = c;
        fill[i as usize] += 1;
    }
    let mut out = Shaded::default();
    let mut corner_vertex = vec![0u32; mesh.indices.len()];
    let mut group_of = vec![0usize; mesh.indices.len()];
    // Gruppen eines Punkts: Einheitsnormale des ersten Dreiecks, Summe der Flächennormalen.
    let mut groups: Vec<(DVec3, DVec3)> = Vec::new();
    for (v, range) in start.windows(2).enumerate() {
        let here = &corners[range[0]..range[1]];
        groups.clear();
        for &c in here {
            let f = c / 3;
            // Streng größer: bei genau dem Grenzwinkel ist die Kante hart (Spec §6).
            let g = match groups
                .iter()
                .position(|(seed, _)| seed.dot(unit[f]) > cos_limit)
            {
                Some(g) => g,
                None => {
                    groups.push((unit[f], DVec3::ZERO));
                    groups.len() - 1
                }
            };
            groups[g].1 += weighted[f];
            group_of[c] = g;
        }
        let first = out.positions.len() as u32;
        for (_, n) in &groups {
            let u = n.try_normalize().unwrap_or(DVec3::Z);
            out.positions.push(mesh.positions[v]);
            out.normals.push([u.x as f32, u.y as f32, u.z as f32]);
        }
        for &c in here {
            corner_vertex[c] = first + group_of[c] as u32;
        }
    }
    out.indices = corner_vertex;
    out
}
