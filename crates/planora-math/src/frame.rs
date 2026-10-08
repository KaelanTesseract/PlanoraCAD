use glam::{DMat4, DVec2, DVec3};

/// Rechtshändiges, orthonormales Koordinatensystem (`x × y = z`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub origin: DVec3,
    pub x: DVec3,
    pub y: DVec3,
    pub z: DVec3,
}

impl Frame {
    pub const WORLD: Frame = Frame {
        origin: DVec3::ZERO,
        x: DVec3::X,
        y: DVec3::Y,
        z: DVec3::Z,
    };

    /// Koordinatensystem mit gegebenem Ursprung, dessen z-Achse `normal` ist.
    pub fn from_origin_normal(origin: DVec3, normal: DVec3) -> Frame {
        let z = normal.normalize();
        let x = z.any_orthonormal_vector();
        let y = z.cross(x);
        Frame { origin, x, y, z }
    }

    pub fn point(&self, x: f64, y: f64, z: f64) -> DVec3 {
        self.origin + self.x * x + self.y * y + self.z * z
    }

    pub fn point_2d(&self, p: DVec2) -> DVec3 {
        self.origin + self.x * p.x + self.y * p.y
    }

    pub fn to_local(&self, p: DVec3) -> DVec3 {
        let d = p - self.origin;
        DVec3::new(d.dot(self.x), d.dot(self.y), d.dot(self.z))
    }

    /// Wendet eine Ähnlichkeitstransformation an. Die z-Achse wird immer als `x × y`
    /// neu berechnet, damit das System auch bei Spiegelungen rechtshändig bleibt.
    pub fn transformed(&self, m: &DMat4) -> Frame {
        let origin = m.transform_point3(self.origin);
        let x = m.transform_vector3(self.x).normalize();
        let y = m.transform_vector3(self.y).normalize();
        let z = x.cross(y);
        Frame { origin, x, y, z }
    }
}
