use crate::mesh::Mesh;
use planora_math::DVec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MassProperties {
    /// Vorzeichenbehaftetes Volumen; positiv, wenn alle Dreiecke nach außen zeigen.
    pub volume: f64,
    pub surface_area: f64,
    pub centroid: DVec3,
}

/// Volumen, Oberfläche und Schwerpunkt (Divergenzsatz über die Dreiecke).
pub fn mass_properties(mesh: &Mesh) -> MassProperties {
    let mut volume = 0.0;
    let mut area = 0.0;
    let mut weighted = DVec3::ZERO;
    for t in mesh.indices.as_chunks::<3>().0 {
        let a = mesh.positions[t[0] as usize];
        let b = mesh.positions[t[1] as usize];
        let c = mesh.positions[t[2] as usize];
        let v = a.dot(b.cross(c)) / 6.0;
        volume += v;
        weighted += v * (a + b + c) / 4.0;
        area += 0.5 * (b - a).cross(c - a).length();
    }
    let centroid = if volume.abs() > 1e-12 {
        weighted / volume
    } else {
        DVec3::ZERO
    };
    MassProperties {
        volume,
        surface_area: area,
        centroid,
    }
}
