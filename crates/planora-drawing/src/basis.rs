//! Ansichtsbasis: Blickrichtung und Blattachsen je Ansichtsart.

use crate::model::ViewKind;
use planora_math::{DVec2, DVec3};

/// Orthonormale Basis einer Ansicht. `d` ist die Blickrichtung, `ex` zeigt auf dem Blatt
/// nach rechts, `ey` nach oben; es gilt `ex × ey = −d`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Basis {
    pub d: DVec3,
    pub ex: DVec3,
    pub ey: DVec3,
}

impl Basis {
    /// Basis der Ansichtsart.
    pub fn of(kind: ViewKind) -> Basis {
        let v = DVec3::new;
        let (d, ey) = match kind {
            ViewKind::Front => (v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)),
            ViewKind::Back => (v(0.0, -1.0, 0.0), v(0.0, 0.0, 1.0)),
            ViewKind::Left => (v(1.0, 0.0, 0.0), v(0.0, 0.0, 1.0)),
            ViewKind::Right => (v(-1.0, 0.0, 0.0), v(0.0, 0.0, 1.0)),
            ViewKind::Top => (v(0.0, 0.0, -1.0), v(0.0, 1.0, 0.0)),
            ViewKind::Bottom => (v(0.0, 0.0, 1.0), v(0.0, -1.0, 0.0)),
            ViewKind::Isometric => {
                let d = v(-1.0, 1.0, -1.0).normalize();
                let eye = -d;
                let z = v(0.0, 0.0, 1.0);
                (d, (z - eye * z.dot(eye)).normalize())
            }
        };
        Basis {
            d,
            ex: d.cross(ey),
            ey,
        }
    }

    /// Projektion eines Raumpunkts auf die Bildebene.
    pub fn project(&self, p: DVec3) -> DVec2 {
        DVec2::new(p.dot(self.ex), p.dot(self.ey))
    }

    /// Tiefe entlang der Blickrichtung (größer = weiter weg).
    pub fn depth(&self, p: DVec3) -> f64 {
        p.dot(self.d)
    }

    /// Richtung vom Körper zum Auge.
    pub fn eye_dir(&self) -> DVec3 {
        -self.d
    }
}
