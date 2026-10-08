use glam::DVec3;

/// Zentrale Toleranzen. Wird jeder Kernel-Operation durchgereicht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerance {
    /// Längentoleranz in Millimetern.
    pub linear: f64,
    /// Winkeltoleranz in Radiant.
    pub angular: f64,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            linear: 1e-6,
            angular: 1e-9,
        }
    }
}

impl Tolerance {
    pub fn points_equal(&self, a: DVec3, b: DVec3) -> bool {
        a.distance(b) <= self.linear
    }
}
