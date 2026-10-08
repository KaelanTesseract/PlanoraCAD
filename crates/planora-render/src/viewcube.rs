use crate::camera::OrbitCamera;
use glam::{Vec2, Vec3};

/// `(yaw, pitch)` der isometrischen Standardansicht.
pub const ISO_VIEW: (f32, f32) = (-std::f32::consts::FRAC_PI_4, 0.615_479_7);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViewFace {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
}

impl ViewFace {
    pub const ALL: [ViewFace; 6] = [
        ViewFace::Front,
        ViewFace::Back,
        ViewFace::Left,
        ViewFace::Right,
        ViewFace::Top,
        ViewFace::Bottom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ViewFace::Front => "Vorne",
            ViewFace::Back => "Hinten",
            ViewFace::Left => "Links",
            ViewFace::Right => "Rechts",
            ViewFace::Top => "Oben",
            ViewFace::Bottom => "Unten",
        }
    }

    /// Außennormale der Würfelseite (Vorne = -y, Rechts = +x, Oben = +z).
    fn normal(self) -> Vec3 {
        match self {
            ViewFace::Front => Vec3::NEG_Y,
            ViewFace::Back => Vec3::Y,
            ViewFace::Left => Vec3::NEG_X,
            ViewFace::Right => Vec3::X,
            ViewFace::Top => Vec3::Z,
            ViewFace::Bottom => Vec3::NEG_Z,
        }
    }

    /// Zwei Tangenten, sodass `t1 × t2 = normal` (Ecken laufen gegen den Uhrzeigersinn von außen).
    fn tangents(self) -> (Vec3, Vec3) {
        match self {
            ViewFace::Right => (Vec3::Y, Vec3::Z),
            ViewFace::Left => (Vec3::Z, Vec3::Y),
            ViewFace::Front => (Vec3::X, Vec3::Z),
            ViewFace::Back => (Vec3::Z, Vec3::X),
            ViewFace::Top => (Vec3::X, Vec3::Y),
            ViewFace::Bottom => (Vec3::Y, Vec3::X),
        }
    }

    /// `(yaw, pitch)` einer Kamera, die diese Seite frontal ansieht.
    pub fn view(self) -> (f32, f32) {
        use std::f32::consts::{FRAC_PI_2, PI};
        match self {
            ViewFace::Front => (-FRAC_PI_2, 0.0),
            ViewFace::Back => (FRAC_PI_2, 0.0),
            ViewFace::Right => (0.0, 0.0),
            ViewFace::Left => (PI, 0.0),
            ViewFace::Top => (-FRAC_PI_2, OrbitCamera::MAX_PITCH),
            ViewFace::Bottom => (-FRAC_PI_2, -OrbitCamera::MAX_PITCH),
        }
    }
}

/// Sichtbare Seite des Navigationswürfels in Einheitskoordinaten (x rechts, y oben, Würfel ±1).
#[derive(Clone, Debug)]
pub struct CubeFace {
    pub face: ViewFace,
    pub corners: [Vec2; 4],
    /// Größer = näher an der Kamera.
    pub depth: f32,
}

/// Projiziert den Würfel orthografisch mit der Drehung der Kamera und liefert die sichtbaren
/// Seiten, die am weitesten hinten liegende zuerst.
pub fn cube_faces(camera: &OrbitCamera) -> Vec<CubeFace> {
    let view = camera.view();
    let to_view = |v: Vec3| view.transform_vector3(v);
    let mut faces: Vec<CubeFace> = ViewFace::ALL
        .iter()
        .filter_map(|&face| {
            let n = face.normal();
            let facing = to_view(n).z;
            // Schwelle über 0,01 (= Rest der Neigung in Draufsicht), sonst zählt die Vorderseite mit.
            if facing <= 0.02 {
                return None;
            }
            let (t1, t2) = face.tangents();
            let corner = |a: f32, b: f32| {
                let p = to_view(n + t1 * a + t2 * b);
                Vec2::new(p.x, p.y)
            };
            Some(CubeFace {
                face,
                corners: [
                    corner(-1.0, -1.0),
                    corner(1.0, -1.0),
                    corner(1.0, 1.0),
                    corner(-1.0, 1.0),
                ],
                depth: facing,
            })
        })
        .collect();
    faces.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    faces
}

fn point_in_quad(corners: &[Vec2; 4], p: Vec2) -> bool {
    let mut sign = 0.0_f32;
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let cross = (b - a).perp_dot(p - a);
        if cross.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

/// Welche (vorderste) Seite liegt unter dem Punkt `p` (Einheitskoordinaten)?
pub fn cube_hit_test(faces: &[CubeFace], p: Vec2) -> Option<ViewFace> {
    faces
        .iter()
        .rev()
        .find(|f| point_in_quad(&f.corners, p))
        .map(|f| f.face)
}
