use crate::viewcube::{ISO_VIEW, ViewFace};
use glam::{Mat4, Vec2, Vec3};

/// Umlaufkamera um einen Zielpunkt. Z zeigt nach oben.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    /// Drehung um die z-Achse (0 = Kamera steht auf der +x-Seite).
    pub yaw: f32,
    /// Höhenwinkel (0 = waagerecht, positiv = von oben).
    pub pitch: f32,
    pub fov_y: f32,
    pub aspect: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 150.0,
            yaw: -0.8,
            pitch: 0.5,
            fov_y: 45.0_f32.to_radians(),
            aspect: 1.0,
        }
    }
}

impl OrbitCamera {
    pub const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.01;

    pub fn eye(&self) -> Vec3 {
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        self.target + self.distance * Vec3::new(cp * cy, cp * sy, sp)
    }

    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye(), self.target, Vec3::Z)
    }

    fn near_far(&self) -> (f32, f32) {
        (
            (self.distance * 0.01).max(0.001),
            self.distance * 100.0 + 10_000.0,
        )
    }

    pub fn projection(&self) -> Mat4 {
        let (near, far) = self.near_far();
        glam::camera::rh::proj::directx::perspective(self.fov_y, self.aspect, near, far)
    }

    pub fn view_proj(&self) -> Mat4 {
        self.projection() * self.view()
    }

    /// Setzt das Seitenverhältnis. Zu kleine (oder nicht endliche) Größen werden ignoriert.
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        if width >= 1.0 && height >= 1.0 && width.is_finite() && height.is_finite() {
            self.aspect = width / height;
        }
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.01;
        self.pitch = (self.pitch + dy * 0.01).clamp(-Self::MAX_PITCH, Self::MAX_PITCH);
    }

    /// Verschiebt das Ziel parallel zum Bildschirm; `dx`, `dy` in Pixeln.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        let height = viewport_height.max(1.0);
        let scale = 2.0 * self.distance * (self.fov_y * 0.5).tan() / height;
        let inv = self.view().inverse();
        let right = inv.x_axis.truncate();
        let up = inv.y_axis.truncate();
        self.target += (-right * dx + up * dy) * scale;
    }

    /// `scroll` in Punkten; positiv = hineinzoomen.
    pub fn zoom(&mut self, scroll: f32) {
        self.distance = (self.distance * (-scroll * 0.002).exp()).clamp(0.01, 1.0e6);
    }

    /// Strahl durch einen Punkt in normierten Gerätekoordinaten (x, y in [-1, 1], y nach oben).
    pub fn ray(&self, ndc: Vec2) -> (Vec3, Vec3) {
        let inv = self.view_proj().inverse();
        let near = inv.project_point3(Vec3::new(ndc.x, ndc.y, 0.0));
        let far = inv.project_point3(Vec3::new(ndc.x, ndc.y, 1.0));
        (near, (far - near).normalize())
    }

    /// Bildschirmposition in Pixeln (Ursprung oben links) oder `None` hinter der Kamera.
    pub fn project(&self, p: Vec3, viewport: Vec2) -> Option<Vec2> {
        let clip = self.view_proj() * p.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(Vec2::new(
            (ndc.x * 0.5 + 0.5) * viewport.x,
            (0.5 - ndc.y * 0.5) * viewport.y,
        ))
    }

    /// Richtet die Kamera so aus, dass der Quader `min..max` ins Bild passt.
    pub fn frame_bounds(&mut self, min: Vec3, max: Vec3) {
        self.target = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(1e-3);
        let half = (self.fov_y * 0.5)
            .min((self.fov_y * self.aspect).atan() * 0.5)
            .max(0.05);
        self.distance = (radius / half.sin() * 1.15).clamp(0.01, 1.0e6);
    }

    /// Richtung zur Lichtquelle (leicht versetzt vom Betrachter, damit Flächen Tiefe bekommen).
    pub fn light_dir(&self) -> Vec3 {
        let inv = self.view().inverse();
        (inv.z_axis.truncate() + inv.y_axis.truncate() * 0.5 + inv.x_axis.truncate() * 0.3)
            .normalize()
    }

    /// Standardansicht von der gegebenen Seite (Blickrichtung auf das Ziel bleibt erhalten).
    pub fn set_view(&mut self, face: ViewFace) {
        let (yaw, pitch) = face.view();
        self.yaw = yaw;
        self.pitch = pitch;
    }

    pub fn set_iso_view(&mut self) {
        (self.yaw, self.pitch) = ISO_VIEW;
    }
}
