use crate::mesh::Mesh;
use planora_math::DVec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MassProperties {
    /// Vorzeichenbehaftetes Volumen; positiv, wenn alle Dreiecke nach außen zeigen.
    pub volume: f64,
    pub surface_area: f64,
    pub centroid: DVec3,
}

/// Bezugspunkt der Spatprodukte: Mitte des Hüllquaders (Ursprung, wenn leer oder nicht endlich).
fn reference_point(points: &[DVec3]) -> DVec3 {
    let Some(first) = points.first() else {
        return DVec3::ZERO;
    };
    let (lo, hi) = points
        .iter()
        .fold((*first, *first), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    let mid = 0.5 * (lo + hi);
    if mid.is_finite() { mid } else { DVec3::ZERO }
}

/// Volumen, Oberfläche und Schwerpunkt (Divergenzsatz über die Dreiecke).
///
/// Die Spatprodukte werden relativ zur Mitte des Hüllquaders gebildet, nicht zum Ursprung: Bei
/// kleinen Teilen weit vom Ursprung löschen sich sonst große Summanden gegenseitig aus (ein Teil
/// von 0,05 mm bei 50 000 mm Abstand ergab ein negatives Volumen). Für geschlossene Netze ist das
/// Ergebnis mathematisch dasselbe.
pub fn mass_properties(mesh: &Mesh) -> MassProperties {
    let o = reference_point(&mesh.positions);
    let mut volume = 0.0;
    let mut area = 0.0;
    let mut weighted = DVec3::ZERO;
    for t in mesh.indices.as_chunks::<3>().0 {
        let a = mesh.positions[t[0] as usize] - o;
        let b = mesh.positions[t[1] as usize] - o;
        let c = mesh.positions[t[2] as usize] - o;
        let v = a.dot(b.cross(c)) / 6.0;
        volume += v;
        weighted += v * (a + b + c) / 4.0;
        area += 0.5 * (b - a).cross(c - a).length();
    }
    let centroid = if volume.abs() > 1e-12 {
        o + weighted / volume
    } else {
        DVec3::ZERO
    };
    MassProperties {
        volume,
        surface_area: area,
        centroid,
    }
}
