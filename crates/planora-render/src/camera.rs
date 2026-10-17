use crate::viewcube::{CubeRegion, ISO_VIEW, ViewFace};
use glam::{Mat4, Vec2, Vec3};

/// Ab diesem Betrag der z-Komponente gilt eine Blickrichtung in `look_along` als senkrecht; die
/// Drehung um z richtet sich dann nach dem Aufwärts-Hinweis statt nach der Blickrichtung.
const NEAR_VERTICAL: f32 = 0.999;

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
    /// Seitenverhältnis des ganzen Fensters.
    pub aspect: f32,
    /// Verschiebung der Bildmitte nach der Perspektive (normierte Gerätekoordinaten, x nach
    /// rechts, y nach oben): Das Ziel erscheint in der Mitte des freien Rechtecks.
    pub ndc_offset: Vec2,
    /// Seitenverhältnis des freien Rechtecks (ohne Felder gleich `aspect`).
    pub visible_aspect: f32,
    /// Höhe des freien Rechtecks im Verhältnis zur Fensterhöhe (ohne Felder 1).
    pub visible_height_fraction: f32,
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
            ndc_offset: Vec2::ZERO,
            visible_aspect: 1.0,
            visible_height_fraction: 1.0,
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
        let proj = glam::camera::rh::proj::directx::perspective(self.fov_y, self.aspect, near, far);
        if self.ndc_offset == Vec2::ZERO {
            // Ohne Versatz bitgleich zur bisherigen Projektion.
            return proj;
        }
        Mat4::from_translation(self.ndc_offset.extend(0.0)) * proj
    }

    pub fn view_proj(&self) -> Mat4 {
        self.projection() * self.view()
    }

    /// Setzt das Seitenverhältnis. Zu kleine (oder nicht endliche) Größen werden ignoriert.
    /// Ohne Versatz folgt das sichtbare Seitenverhältnis dem Fenster. Das gilt auch für ein
    /// mittig liegendes, kleineres freies Rechteck (Versatz 0): Aufrufer mit freiem Rechteck
    /// rufen daher in jedem Bild `set_visible` nach `set_viewport` auf (oder nur `set_visible`).
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        if width >= 1.0 && height >= 1.0 && width.is_finite() && height.is_finite() {
            self.aspect = width / height;
            if self.ndc_offset == Vec2::ZERO {
                self.visible_aspect = self.aspect;
            }
        }
    }

    /// `full`: Größe des ganzen Fensters in Pixeln; `visible`: freies Rechteck (Ecke oben links
    /// und Größe in Pixeln, Ursprung oben links des Fensters). Setzt `aspect`, `ndc_offset`,
    /// `visible_aspect` und `visible_height_fraction`. Zu kleine (oder nicht endliche) Größen
    /// werden ignoriert.
    pub fn set_visible(&mut self, full: Vec2, visible: (Vec2, Vec2)) {
        let (min, size) = visible;
        let ok = |v: Vec2| v.is_finite() && v.x >= 1.0 && v.y >= 1.0;
        if !ok(full) || !ok(size) || !min.is_finite() {
            return;
        }
        let centre = min + size * 0.5;
        self.aspect = full.x / full.y;
        self.ndc_offset = Vec2::new(centre.x / full.x * 2.0 - 1.0, 1.0 - centre.y / full.y * 2.0);
        self.visible_aspect = size.x / size.y;
        self.visible_height_fraction = size.y / full.y;
    }

    /// Kein freies Rechteck: Versatz 0, sichtbares Seitenverhältnis gleich dem des Fensters.
    pub fn clear_visible(&mut self) {
        self.ndc_offset = Vec2::ZERO;
        self.visible_aspect = self.aspect;
        self.visible_height_fraction = 1.0;
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
        // Inverse in f64: in f32 liegt der Strahl sonst bis etwa 1e-3 mm neben dem Punkt.
        let inv = self.view_proj().as_dmat4().inverse();
        let near = inv.project_point3(glam::DVec3::new(ndc.x as f64, ndc.y as f64, 0.0));
        let far = inv.project_point3(glam::DVec3::new(ndc.x as f64, ndc.y as f64, 1.0));
        (near.as_vec3(), (far - near).normalize().as_vec3())
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

    /// Richtet die Kamera so aus, dass der Quader `min..max` ins freie Rechteck passt.
    pub fn frame_bounds(&mut self, min: Vec3, max: Vec3) {
        self.target = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(1e-3);
        // Öffnungswinkel und Seitenverhältnis des freien Rechtecks. „Kein freies Rechteck“
        // heißt Versatz 0 und Anteil 1: Dann gelten genau `fov_y` und `aspect`, und
        // `visible_aspect` wird nicht beachtet. Grund: Kameras aus Strukturliteralen
        // (`aspect: 1.5, ..OrbitCamera::default()`) setzen nur `aspect`, `visible_aspect`
        // bliebe dort auf dem Standardwert 1 stehen und würde das Einpassen verändern.
        let (fov_vis, aspect_vis) =
            if self.ndc_offset == Vec2::ZERO && self.visible_height_fraction == 1.0 {
                (self.fov_y, self.aspect)
            } else {
                let fov = 2.0 * ((self.fov_y * 0.5).tan() * self.visible_height_fraction).atan();
                (fov, self.visible_aspect)
            };
        let half = (fov_vis * 0.5)
            .min((fov_vis * aspect_vis).atan() * 0.5)
            .max(0.05);
        self.distance = (radius / half.sin() * 1.15).clamp(0.01, 1.0e6);
    }

    /// Halbe Öffnung des freien Rechtecks als Tangens `(waagerecht, senkrecht)`; ohne freies
    /// Rechteck die des ganzen Fensters (wie in [`OrbitCamera::frame_bounds`]).
    fn visible_tangents(&self) -> (f32, f32) {
        let tan_v = (self.fov_y * 0.5).tan();
        if self.ndc_offset == Vec2::ZERO && self.visible_height_fraction == 1.0 {
            (tan_v * self.aspect, tan_v)
        } else {
            let tan_v = tan_v * self.visible_height_fraction;
            (tan_v * self.visible_aspect, tan_v)
        }
    }

    /// Richtet die Kamera bei unveränderter Blickrichtung so aus, dass alle `points` ins freie
    /// Rechteck passen: Das Ziel kommt in die Mitte ihrer Hülle (quer zur Blickrichtung), der
    /// Abstand ist der kleinste, bei dem jeder Punkt mit dem Rand von `frame_bounds` (15 %)
    /// hineinpasst. Anders als `frame_bounds` (Kugel um den Quader) füllt eine ebene Fläche, auf
    /// die man senkrecht blickt, so die Breite oder Höhe des freien Rechtecks bis auf den Rand.
    /// Ohne endliche Punkte bleibt die Kamera unverändert.
    pub fn frame_points(&mut self, points: &[Vec3]) {
        let pts: Vec<Vec3> = points.iter().copied().filter(|p| p.is_finite()).collect();
        if pts.is_empty() {
            return;
        }
        let inv = self.view().inverse();
        let right = inv.x_axis.truncate();
        let up = inv.y_axis.truncate();
        let back = inv.z_axis.truncate();
        let local: Vec<Vec3> = pts
            .iter()
            .map(|p| Vec3::new(p.dot(right), p.dot(up), p.dot(back)))
            .collect();
        let (lo, hi) = local
            .iter()
            .fold((local[0], local[0]), |(a, b), &p| (a.min(p), b.max(p)));
        let c = (lo + hi) * 0.5;
        let (tan_h, tan_v) = self.visible_tangents();
        let (tan_h, tan_v) = (tan_h.max(1e-3), tan_v.max(1e-3));
        let margin = 1.15;
        // Ein Punkt im Abstand `dz` vor der Zielebene liegt `distance - dz` vor dem Auge.
        let mut distance = 1e-3_f32;
        for p in &local {
            let d = p - c;
            distance = distance
                .max(d.z + margin * d.x.abs() / tan_h)
                .max(d.z + margin * d.y.abs() / tan_v);
        }
        // Mindestens doppelt so weit weg wie der vorderste Punkt vor der Zielebene (sonst läge
        // er bei sehr flachen Hüllen dicht am oder hinter dem Auge).
        distance = distance.max((hi.z - c.z) * 2.0);
        self.target = right * c.x + up * c.y + back * c.z;
        self.distance = distance.clamp(0.01, 1.0e6);
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

    /// Stellt die Kamera vom Ziel aus in Richtung `to_camera` (Höhenwinkel auf `±MAX_PITCH`
    /// geklemmt). Bei (fast) senkrechter Blickrichtung wird die Drehung so gewählt, dass die
    /// Bildschirm-Aufwärtsrichtung der Projektion von `up_hint` entspricht.
    pub fn look_along(&mut self, to_camera: Vec3, up_hint: Vec3) {
        let d = to_camera.normalize_or_zero();
        if d == Vec3::ZERO {
            return;
        }
        self.pitch = d.z.asin().clamp(-Self::MAX_PITCH, Self::MAX_PITCH);
        self.yaw = if d.z.abs() > NEAR_VERTICAL {
            let up = Vec3::new(up_hint.x, up_hint.y, 0.0);
            if d.z > 0.0 {
                (-up.y).atan2(-up.x)
            } else {
                up.y.atan2(up.x)
            }
        } else {
            d.y.atan2(d.x)
        };
    }

    pub fn set_iso_view(&mut self) {
        (self.yaw, self.pitch) = ISO_VIEW;
    }

    /// Startansicht des Würfels (Home-Knopf): die isometrische Standardansicht. Das Einpassen
    /// übernimmt der Aufrufer.
    pub fn home(&mut self) {
        self.set_iso_view();
    }

    /// Blickt auf einen Bereich des Navigationswürfels: Flächen wie `set_view`, Kanten und Ecken
    /// entlang ihrer Normalen (Kante 45° zwischen den Seiten, Ecke im Isometrie-Winkel).
    pub fn set_view_region(&mut self, region: CubeRegion) {
        match region {
            CubeRegion::Face(face) => self.set_view(face),
            // Kanten und Ecken sind nie senkrecht (|z| ≤ 1/√2), der Aufwärts-Hinweis wirkt nicht.
            _ => self.look_along(region.normal(), Vec3::Z),
        }
    }
}
