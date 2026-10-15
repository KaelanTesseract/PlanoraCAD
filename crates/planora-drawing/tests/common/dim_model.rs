//! Formelmodelle für die Maßprüfung (Teilprojekt 12): die Geometrie der Prüfkörper als `Geo3`,
//! **von Hand** aus den Maßen geschrieben (Eckpunkte, Kanten, Kreise, Achsen) – ohne
//! Auflösung über `planora-feature` und ohne Rechnung aus `planora-drawing`.

use planora_drawing::dimension::Geo3;
use planora_drawing::model::ViewKind;
use planora_math::{DVec2, DVec3};

/// Weltachse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ax {
    X,
    Y,
    Z,
}

/// Die sechs Normansichten.
pub const NORMAL_VIEWS: [ViewKind; 6] = [
    ViewKind::Front,
    ViewKind::Back,
    ViewKind::Left,
    ViewKind::Right,
    ViewKind::Top,
    ViewKind::Bottom,
];

/// Von Hand aufgeschriebene Bildachsen der Normansichten: Weltachse und Vorzeichen, auf die
/// `ex` (nach rechts) und `ey` (nach oben) zeigen (Vorne: x nach rechts, z nach oben; Hinten:
/// −x nach rechts; Links (Blick +x): −y nach rechts; Rechts (Blick −x): +y nach rechts; Oben
/// (Blick −z): x nach rechts, y nach oben; Unten (Blick +z): x nach rechts, −y nach oben).
pub fn view_axes(kind: ViewKind) -> ((Ax, f64), (Ax, f64)) {
    match kind {
        ViewKind::Front => ((Ax::X, 1.0), (Ax::Z, 1.0)),
        ViewKind::Back => ((Ax::X, -1.0), (Ax::Z, 1.0)),
        ViewKind::Left => ((Ax::Y, -1.0), (Ax::Z, 1.0)),
        ViewKind::Right => ((Ax::Y, 1.0), (Ax::Z, 1.0)),
        ViewKind::Top => ((Ax::X, 1.0), (Ax::Y, 1.0)),
        ViewKind::Bottom => ((Ax::X, 1.0), (Ax::Y, -1.0)),
        ViewKind::Isometric => panic!("keine Normansicht"),
    }
}

/// Komponente eines Punkts entlang einer Weltachse.
pub fn comp(p: DVec3, ax: Ax) -> f64 {
    match ax {
        Ax::X => p.x,
        Ax::Y => p.y,
        Ax::Z => p.z,
    }
}

/// Bildpunkt einer Normansicht nach der Tabelle [`view_axes`] (ohne `Basis`).
pub fn image(kind: ViewKind, p: DVec3) -> DVec2 {
    let ((hx, hs), (vx, vs)) = view_axes(kind);
    DVec2::new(hs * comp(p, hx), vs * comp(p, vx))
}

/// Maße `(a, b, c)` eines Quaders entlang x, y, z.
pub fn box_dims(a: f64, b: f64, c: f64) -> [(Ax, f64); 3] {
    [(Ax::X, a), (Ax::Y, b), (Ax::Z, c)]
}

/// Maß des Quaders entlang einer Achse.
pub fn dim_along(dims: [(Ax, f64); 3], ax: Ax) -> f64 {
    dims.iter().find(|(x, _)| *x == ax).unwrap().1
}

/// Eine Kante des Quaders mit ihrer Achsrichtung.
#[derive(Clone, Debug)]
pub struct BoxEdge {
    pub axis: Ax,
    pub geo: Geo3,
}

/// Die 12 Kanten des Quaders `o + [0,a] × [0,b] × [0,c]` als `Geo3::Line`, je Achse vier,
/// Endpunkte von Hand: Kanten entlang x bei `(y, z) ∈ {0, b} × {0, c}` usw.
pub fn box_edges(o: DVec3, a: f64, b: f64, c: f64) -> Vec<BoxEdge> {
    let p = |x: f64, y: f64, z: f64| o + DVec3::new(x, y, z);
    let mut e = Vec::new();
    for (y, z) in [(0.0, 0.0), (b, 0.0), (0.0, c), (b, c)] {
        e.push(BoxEdge {
            axis: Ax::X,
            geo: Geo3::Line {
                a: p(0.0, y, z),
                b: p(a, y, z),
            },
        });
    }
    for (x, z) in [(0.0, 0.0), (a, 0.0), (0.0, c), (a, c)] {
        e.push(BoxEdge {
            axis: Ax::Y,
            geo: Geo3::Line {
                a: p(x, 0.0, z),
                b: p(x, b, z),
            },
        });
    }
    for (x, y) in [(0.0, 0.0), (a, 0.0), (0.0, b), (a, b)] {
        e.push(BoxEdge {
            axis: Ax::Z,
            geo: Geo3::Line {
                a: p(x, y, 0.0),
                b: p(x, y, c),
            },
        });
    }
    e
}

/// Die acht Ecken des Quaders `o + [0,a] × [0,b] × [0,c]`.
pub fn box_corners(o: DVec3, a: f64, b: f64, c: f64) -> Vec<DVec3> {
    let mut v = Vec::new();
    for x in [0.0, a] {
        for y in [0.0, b] {
            for z in [0.0, c] {
                v.push(o + DVec3::new(x, y, z));
            }
        }
    }
    v
}

/// Kante des Quaders entlang `axis`, deren Endpunkte auf der Achse `at` die Lage `value` haben
/// (bei mehreren die erste in der Reihenfolge von [`box_edges`]).
pub fn edge_at(edges: &[BoxEdge], axis: Ax, at: Ax, value: f64) -> Geo3 {
    edges
        .iter()
        .find(|e| {
            e.axis == axis
                && match &e.geo {
                    Geo3::Line { a, .. } => comp(*a, at) == value,
                    _ => false,
                }
        })
        .unwrap_or_else(|| panic!("keine Kante {axis:?} bei {at:?} = {value}"))
        .geo
        .clone()
}

/// Deckel- und Bodenkreis des Zylinders (Achse z, Boden im Ursprung): `(Boden, Deckel)`.
pub fn cylinder_circles(r: f64, h: f64) -> (Geo3, Geo3) {
    (
        Geo3::Circle {
            center: DVec3::ZERO,
            normal: DVec3::Z,
            radius: r,
        },
        Geo3::Circle {
            center: DVec3::new(0.0, 0.0, h),
            normal: DVec3::Z,
            radius: r,
        },
    )
}

/// Mantelfläche des Zylinders (Achse z durch den Ursprung).
pub fn cylinder_axis(r: f64) -> Geo3 {
    Geo3::Axis {
        origin: DVec3::ZERO,
        dir: DVec3::Z,
        radius: r,
    }
}

/// Kugel um `center`.
pub fn sphere(center: DVec3, r: f64) -> Geo3 {
    Geo3::Sphere { center, radius: r }
}

/// Kante als `Geo3::Line` aus zwei Punkten.
pub fn line(a: DVec3, b: DVec3) -> Geo3 {
    Geo3::Line { a, b }
}

/// Punkt in der Vorderansicht-Ebene `y = 0`: `(x, z)`.
pub fn xz(x: f64, z: f64) -> DVec3 {
    DVec3::new(x, 0.0, z)
}

/// Winkelprofil (L) in der Ebene y = 0, Vorderseite einer Extrusion entlang y: Umriss
/// (0,0) → (40,0) → (40,5) → (5,5) → (5,30) → (0,30); hier nur die beiden Schenkelkanten am
/// Ursprung: `(unten, links)`.
pub fn l_profile_corner_edges() -> (Geo3, Geo3) {
    (
        line(xz(0.0, 0.0), xz(40.0, 0.0)),
        line(xz(0.0, 0.0), xz(0.0, 30.0)),
    )
}

/// Platte 40×30 mit 45°-Fase 5×5 an der Ecke oben rechts, Umriss in y = 0:
/// (0,0) → (40,0) → (40,25) → (35,30) → (0,30). Liefert `(Fase, Oberkante, Unterkante)`.
pub fn chamfer_plate_edges() -> (Geo3, Geo3, Geo3) {
    (
        line(xz(40.0, 25.0), xz(35.0, 30.0)),
        line(xz(35.0, 30.0), xz(0.0, 30.0)),
        line(xz(0.0, 0.0), xz(40.0, 0.0)),
    )
}

/// Ecke des Quaders `[0,a] × [0,b] × [0,c]`, um die z-Achse um `deg` Grad gedreht (gegen den
/// Uhrzeigersinn von oben): `(x, y, z) → (x·cos − y·sin, x·sin + y·cos, z)`.
pub fn rot_z(p: DVec3, deg: f64) -> DVec3 {
    let (s, c) = deg.to_radians().sin_cos();
    DVec3::new(p.x * c - p.y * s, p.x * s + p.y * c, p.z)
}

/// Drehung um die y-Achse um `deg` Grad: `(x, y, z) → (x·cos + z·sin, y, −x·sin + z·cos)`.
pub fn rot_y(p: DVec3, deg: f64) -> DVec3 {
    let (s, c) = deg.to_radians().sin_cos();
    DVec3::new(p.x * c + p.z * s, p.y, -p.x * s + p.z * c)
}

/// Kleiner eigener Zufallszahlengenerator (xorshift64*), deterministisch je Startwert.
pub struct DimRng(u64);

impl DimRng {
    pub fn new(seed: u64) -> DimRng {
        DimRng((seed ^ 0x2545_F491_4F6C_DD1D) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Vielfaches von 0,5 in `[lo, hi]` (beide Vielfache von 0,5).
    pub fn half_steps(&mut self, lo: f64, hi: f64) -> f64 {
        let n = ((hi - lo) * 2.0).round() as u64;
        lo + 0.5 * (self.next_u64() % (n + 1)) as f64
    }
}
