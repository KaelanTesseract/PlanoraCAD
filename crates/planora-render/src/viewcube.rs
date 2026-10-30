use crate::camera::OrbitCamera;
use glam::{Vec2, Vec3};
use planora_i18n::tr;

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

    /// Name der Seite in der aktuellen Sprache.
    pub fn label(self) -> String {
        match self {
            ViewFace::Front => tr!("render-face-front"),
            ViewFace::Back => tr!("render-face-back"),
            ViewFace::Left => tr!("render-face-left"),
            ViewFace::Right => tr!("render-face-right"),
            ViewFace::Top => tr!("render-face-top"),
            ViewFace::Bottom => tr!("render-face-bottom"),
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
#[cfg(test)]
#[derive(Clone, Debug)]
struct CubeFace {
    face: ViewFace,
    corners: [Vec2; 4],
    /// Größer = näher an der Kamera.
    depth: f32,
}

/// Projiziert den Würfel orthografisch mit der Drehung der Kamera und liefert die sichtbaren
/// Seiten, die am weitesten hinten liegende zuerst.
#[cfg(test)]
fn cube_faces(camera: &OrbitCamera) -> Vec<CubeFace> {
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

/// Liegt `p` im konvexen Vieleck `corners` (Umlaufsinn egal, Rand zählt mit)?
fn point_in_convex(corners: &[Vec2], p: Vec2) -> bool {
    let n = corners.len();
    let mut sign = 0.0_f32;
    for i in 0..n {
        let a = corners[i];
        let b = corners[(i + 1) % n];
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
#[cfg(test)]
fn cube_hit_test(faces: &[CubeFace], p: Vec2) -> Option<ViewFace> {
    faces
        .iter()
        .rev()
        .find(|f| point_in_convex(&f.corners, p))
        .map(|f| f.face)
}

/// Anteil der Kantenlänge, den ein Flächenquadrat einnimmt (halbe Seitenlänge in Würfeleinheiten,
/// Würfel ±1). Der Rest bildet die schrägen Kantenstreifen und Eckendreiecke.
pub const INSET: f32 = 0.6;

/// Anzahl der anklickbaren Bereiche: 6 Flächen, 12 Kanten, 8 Ecken.
pub(crate) const REGION_COUNT: usize = 26;

/// Anklickbarer Bereich des Navigationswürfels.
///
/// Kanten speichern ihre Seiten in fester Reihenfolge (Oben/Unten vor Vorne/Hinten vor
/// Links/Rechts), damit gleiche Kanten gleich sind. Ecken tragen je eine Seite pro Achse:
/// `x` ∈ {Links, Rechts}, `y` ∈ {Vorne, Hinten}, `z` ∈ {Oben, Unten}.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum CubeRegion {
    Face(ViewFace),
    Edge {
        a: ViewFace,
        b: ViewFace,
    },
    Corner {
        x: ViewFace,
        y: ViewFace,
        z: ViewFace,
    },
}

const Z_FACES: [ViewFace; 2] = [ViewFace::Top, ViewFace::Bottom];
const Y_FACES: [ViewFace; 2] = [ViewFace::Front, ViewFace::Back];
const X_FACES: [ViewFace; 2] = [ViewFace::Left, ViewFace::Right];

/// Alle 26 Bereiche: erst die Flächen, dann die Kanten, dann die Ecken.
pub fn all_regions() -> [CubeRegion; REGION_COUNT] {
    let mut out = [CubeRegion::Face(ViewFace::Front); REGION_COUNT];
    let mut i = 0;
    let mut push = |r: CubeRegion| {
        out[i] = r;
        i += 1;
    };
    for f in ViewFace::ALL {
        push(CubeRegion::Face(f));
    }
    for (first, second) in [(Z_FACES, Y_FACES), (Z_FACES, X_FACES), (Y_FACES, X_FACES)] {
        for a in first {
            for b in second {
                push(CubeRegion::Edge { a, b });
            }
        }
    }
    for z in Z_FACES {
        for y in Y_FACES {
            for x in X_FACES {
                push(CubeRegion::Corner { x, y, z });
            }
        }
    }
    out
}

impl CubeRegion {
    /// Ist der Bereich einer der 26 gültigen? Kanten tragen zwei verschiedene Achsen in der
    /// Reihenfolge Z vor Y vor X; Ecken je eine Seite pro Achse (`x` ∈ {Links, Rechts},
    /// `y` ∈ {Vorne, Hinten}, `z` ∈ {Oben, Unten}).
    pub fn is_valid(self) -> bool {
        let rank = |f: ViewFace| match f {
            ViewFace::Top | ViewFace::Bottom => 0,
            ViewFace::Front | ViewFace::Back => 1,
            ViewFace::Left | ViewFace::Right => 2,
        };
        match self {
            CubeRegion::Face(_) => true,
            CubeRegion::Edge { a, b } => rank(a) < rank(b),
            CubeRegion::Corner { x, y, z } => {
                X_FACES.contains(&x) && Y_FACES.contains(&y) && Z_FACES.contains(&z)
            }
        }
    }

    /// Außennormale des Bereichs (Kanten und Ecken: gemittelte Seitennormalen), Länge 1. Ein
    /// ungültiger Bereich (z. B. Kante Oben-Unten) liefert den Nullvektor statt NaN.
    pub fn normal(self) -> Vec3 {
        match self {
            CubeRegion::Face(f) => f.normal(),
            CubeRegion::Edge { a, b } => (a.normal() + b.normal()).normalize_or_zero(),
            CubeRegion::Corner { x, y, z } => {
                (x.normal() + y.normal() + z.normal()).normalize_or_zero()
            }
        }
    }

    /// Name in der aktuellen Sprache, z. B. „Vorne“, „Oben-Rechts“, „Oben-Vorne-Rechts“.
    pub fn name(self) -> String {
        match self {
            CubeRegion::Face(f) => f.label(),
            CubeRegion::Edge { a, b } => format!("{}-{}", a.label(), b.label()),
            CubeRegion::Corner { x, y, z } => {
                format!("{}-{}-{}", z.label(), y.label(), x.label())
            }
        }
    }

    /// Umriss auf dem abgeschrägten Würfel (Würfeleinheiten, Weltachsen), von außen gesehen
    /// gegen den Uhrzeigersinn.
    fn outline(self) -> Vec<Vec3> {
        match self {
            CubeRegion::Face(f) => {
                let n = f.normal();
                let (t1, t2) = f.tangents();
                [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                    .iter()
                    .map(|&(a, b)| n + (t1 * a + t2 * b) * INSET)
                    .collect()
            }
            CubeRegion::Edge { a, b } => {
                let (na, nb) = (a.normal(), b.normal());
                // Kantenrichtung so, dass der Umlauf von außen gegen den Uhrzeigersinn läuft.
                let e = nb.cross(na) * INSET;
                let on_a = na + nb * INSET;
                let on_b = nb + na * INSET;
                vec![on_a - e, on_a + e, on_b + e, on_b - e]
            }
            CubeRegion::Corner { x, y, z } => {
                let (nx, ny, nz) = (x.normal(), y.normal(), z.normal());
                let mut pts = vec![
                    nx + (ny + nz) * INSET,
                    ny + (nx + nz) * INSET,
                    nz + (nx + ny) * INSET,
                ];
                if (pts[1] - pts[0]).cross(pts[2] - pts[0]).dot(nx + ny + nz) < 0.0 {
                    pts.swap(1, 2);
                }
                pts
            }
        }
    }
}

/// Sichtbarer Bereich des Navigationswürfels in Würfeleinheiten (x rechts, y oben).
#[derive(Clone, Debug)]
pub struct CubePiece {
    pub region: CubeRegion,
    /// 3 (Ecke) oder 4 Punkte (Fläche, Kante).
    pub corners: Vec<Vec2>,
    /// Größer = näher an der Kamera.
    pub depth: f32,
}

/// Projiziert den abgeschrägten Würfel orthografisch und liefert die Bereiche, die stärker als
/// `min_facing` zur Kamera zeigen (`depth > min_facing`), den am weitesten hinten liegenden
/// zuerst. Fast flach gesehene Bereiche erscheinen im Bild nur als schmale Streifen; wer sie nicht
/// malen will, lässt sie hier weg und testet Klicks gegen dieselbe Liste. Bei einer nicht
/// endlichen Kamera (NaN) können Bereiche mit nicht endlichen Zahlen darunter sein; der Aufrufer
/// filtert sie.
pub fn cube_pieces_visible(camera: &OrbitCamera, min_facing: f32) -> Vec<CubePiece> {
    let view = camera.view();
    let to_view = |v: Vec3| view.transform_vector3(v);
    let mut pieces: Vec<CubePiece> = all_regions()
        .iter()
        .filter_map(|&region| {
            let facing = to_view(region.normal()).z;
            if facing <= min_facing {
                return None;
            }
            let corners = region
                .outline()
                .into_iter()
                .map(|p| {
                    let v = to_view(p);
                    Vec2::new(v.x, v.y)
                })
                .collect();
            Some(CubePiece {
                region,
                corners,
                depth: facing,
            })
        })
        .collect();
    pieces.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    pieces
}

/// Welcher (vorderste) Bereich liegt unter dem Punkt `p` (Würfeleinheiten)?
pub fn pieces_hit_test(pieces: &[CubePiece], p: Vec2) -> Option<CubeRegion> {
    pieces
        .iter()
        .rev()
        .find(|piece| point_in_convex(&piece.corners, p))
        .map(|piece| piece.region)
}

/// Bereich, auf den die Kamera gerade blickt: Die Richtung vom Ziel zur Kamera weicht höchstens
/// `tolerance_deg` Grad von seiner Normalen ab (bei mehreren der nächste). Sonst `None`.
///
/// Am Höhenwinkel-Anschlag (`±MAX_PITCH`) gilt die Kamera als genau senkrecht: Weiter kann sie
/// nicht kippen, die Draufsicht ist dort also erreicht.
pub fn current_region(camera: &OrbitCamera, tolerance_deg: f32) -> Option<CubeRegion> {
    if !(camera.yaw.is_finite() && camera.pitch.is_finite() && camera.distance.is_finite()) {
        return None;
    }
    let at_pole = camera.pitch.abs() >= OrbitCamera::MAX_PITCH - 1e-4;
    let dir = if at_pole {
        Vec3::Z * camera.pitch.signum()
    } else {
        let (sp, cp) = camera.pitch.sin_cos();
        let (sy, cy) = camera.yaw.sin_cos();
        Vec3::new(cp * cy, cp * sy, sp)
    };
    let limit = tolerance_deg.to_radians();
    all_regions()
        .into_iter()
        .map(|r| (r, dir.angle_between(r.normal())))
        .filter(|&(_, angle)| angle <= limit)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(r, _)| r)
}

/// Lese-Richtungen der Beschriftung auf einer Fläche: `(rechts, oben)` in Weltachsen.
///
/// Seitenflächen: oben = +Z; Oben-Fläche: oben = +Y (nach hinten); Unten-Fläche: oben = −Y.
/// `rechts = oben × Normale`. Damit gilt `rechts × oben = Normale`: Von außen auf die Fläche
/// gesehen (Blick entlang −Normale) zeigt „rechts“ im Bild nach rechts, die Schrift ist also nie
/// gespiegelt.
pub fn face_text_frame(face: ViewFace) -> (Vec3, Vec3) {
    let up = match face {
        ViewFace::Top => Vec3::Y,
        ViewFace::Bottom => Vec3::NEG_Y,
        _ => Vec3::Z,
    };
    (up.cross(face.normal()), up)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible(cam: &OrbitCamera) -> Vec<ViewFace> {
        let mut v: Vec<_> = cube_faces(cam).iter().map(|f| f.face).collect();
        v.sort_by_key(|f| format!("{f:?}"));
        v
    }

    #[test]
    fn front_view_shows_only_the_front_face() {
        let mut cam = OrbitCamera::default();
        cam.set_view(ViewFace::Front);
        assert_eq!(visible(&cam), vec![ViewFace::Front]);
    }

    #[test]
    fn top_view_shows_only_the_top_face() {
        let mut cam = OrbitCamera::default();
        cam.set_view(ViewFace::Top);
        assert_eq!(visible(&cam), vec![ViewFace::Top]);
    }

    #[test]
    fn isometric_view_shows_three_faces() {
        let mut cam = OrbitCamera::default();
        cam.set_iso_view();
        assert_eq!(
            visible(&cam),
            vec![ViewFace::Front, ViewFace::Right, ViewFace::Top]
        );
    }

    #[test]
    fn clicking_the_center_of_the_front_face_selects_it() {
        let mut cam = OrbitCamera::default();
        cam.set_view(ViewFace::Front);
        let faces = cube_faces(&cam);
        assert_eq!(cube_hit_test(&faces, Vec2::ZERO), Some(ViewFace::Front));
        assert_eq!(cube_hit_test(&faces, Vec2::new(5.0, 5.0)), None);
    }

    #[test]
    fn every_standard_view_looks_at_the_expected_side() {
        // Die Kamera steht auf der Seite der Fläche, die sie ansieht.
        let expectations = [
            (ViewFace::Front, Vec3::NEG_Y),
            (ViewFace::Back, Vec3::Y),
            (ViewFace::Right, Vec3::X),
            (ViewFace::Left, Vec3::NEG_X),
            (ViewFace::Top, Vec3::Z),
            (ViewFace::Bottom, Vec3::NEG_Z),
        ];
        for (face, side) in expectations {
            let mut cam = OrbitCamera::default();
            cam.set_view(face);
            let dir = (cam.eye() - cam.target).normalize();
            assert!(dir.dot(side) > 0.99, "{face:?}: {dir:?}");
        }
    }

    #[test]
    fn region_count_matches_all_regions() {
        assert_eq!(REGION_COUNT, 26);
        assert_eq!(all_regions().len(), REGION_COUNT);
    }
}
