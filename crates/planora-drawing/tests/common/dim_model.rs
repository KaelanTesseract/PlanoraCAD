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
            arc: None,
        },
        Geo3::Circle {
            center: DVec3::new(0.0, 0.0, h),
            normal: DVec3::Z,
            radius: r,
            arc: None,
        },
    )
}

/// Mantelfläche des Zylinders (Achse z durch den Ursprung, Fläche von z = 0 bis z = h).
pub fn cylinder_axis(r: f64, h: f64) -> Geo3 {
    Geo3::Axis {
        origin: DVec3::ZERO,
        dir: DVec3::Z,
        radius: r,
        t0: 0.0,
        t1: h,
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

/// Eigene Auflösung einer Vorschlagsherkunft nach den Regeln der Aufgabe 8, von Hand aus den
/// Daten des Körpers (ohne Code aus `planora-drawing`): Kante mit Gerade → `Line` zwischen
/// Anfangs- und Endpunkt (Eckpunkte), Kante mit Kreis → `Circle` (Rahmenursprung, Rahmen-z,
/// Radius; Bogen aus den Punkten bei `t0`, `(t0 + t1)/2`, `t1`, außer die Kreiskanten mit
/// gleicher Mitte und gleichem Radius ergeben zusammen mindestens 2π − 1e-6); Kantenende → `Point` (Eckpunkt); Mitte → `Point` (Kreismitte); Zylinderfläche →
/// `Axis` (Rahmenursprung, normierte Rahmen-z, Radius, Bereich der Randkanten längs der Achse,
/// abgetastet mit 4096 Schritten je Kante); Kugelfläche → `Sphere`. Sonst `None`.
pub fn resolve_origin(
    solids: &[&planora_kernel::Solid],
    o: &planora_drawing::suggest::CandidateOrigin,
) -> Option<Geo3> {
    use planora_drawing::dimension::EndSel;
    use planora_drawing::suggest::CandidateOrigin;
    use planora_kernel::{Curve, Edge, Face, Id, Surface};
    let edge = |b: usize, e: usize| -> Option<(&planora_kernel::Solid, &Edge)> {
        let s = *solids.get(b)?;
        (e < s.edges.len()).then(|| (s, &s.edges[Id::<Edge>::from_index(e)]))
    };
    match *o {
        CandidateOrigin::Edge { body, edge: e } => {
            let (s, e) = edge(body, e)?;
            match &e.curve {
                Curve::Line { .. } => Some(Geo3::Line {
                    a: s.vertices[e.start].point,
                    b: s.vertices[e.end].point,
                }),
                Curve::Circle { frame, radius } => {
                    let turn: f64 = s
                        .edges
                        .iter()
                        .filter_map(|(_, o)| match &o.curve {
                            Curve::Circle {
                                frame: f,
                                radius: r,
                            } if f.origin.distance(frame.origin) < 1e-9
                                && (r - radius).abs() < 1e-9 =>
                            {
                                Some((o.t1 - o.t0).abs())
                            }
                            _ => None,
                        })
                        .sum();
                    let arc = (turn < std::f64::consts::TAU - 1e-6).then(|| {
                        planora_drawing::dimension::Arc3 {
                            start: e.curve.point(e.t0),
                            mid: e.curve.point(0.5 * (e.t0 + e.t1)),
                            end: e.curve.point(e.t1),
                        }
                    });
                    Some(Geo3::Circle {
                        center: frame.origin,
                        normal: frame.z,
                        radius: *radius,
                        arc,
                    })
                }
                _ => None,
            }
        }
        CandidateOrigin::EdgeEnd { body, edge: e, end } => {
            let (s, e) = edge(body, e)?;
            let v = if end == EndSel::Start { e.start } else { e.end };
            Some(Geo3::Point(s.vertices[v].point))
        }
        CandidateOrigin::Center { body, edge: e } => match &edge(body, e)?.1.curve {
            Curve::Circle { frame, .. } => Some(Geo3::Point(frame.origin)),
            _ => None,
        },
        CandidateOrigin::Face { body, face } => {
            let s = *solids.get(body)?;
            if face >= s.faces.len() {
                return None;
            }
            let f = &s.faces[Id::<Face>::from_index(face)];
            match &f.surface {
                Surface::Cylinder { frame, radius } => {
                    let z = frame.z.normalize();
                    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                    for lp in std::iter::once(&f.outer).chain(f.inner.iter()) {
                        for he in &lp.half_edges {
                            let e = &s.edges[he.edge];
                            for i in 0..=4096 {
                                let t = e.t0 + (e.t1 - e.t0) * i as f64 / 4096.0;
                                let h = (e.curve.point(t) - frame.origin).dot(z);
                                lo = lo.min(h);
                                hi = hi.max(h);
                            }
                        }
                    }
                    Some(Geo3::Axis {
                        origin: frame.origin,
                        dir: z,
                        radius: *radius,
                        t0: lo,
                        t1: hi,
                    })
                }
                Surface::Sphere { frame, radius } => Some(Geo3::Sphere {
                    center: frame.origin,
                    radius: *radius,
                }),
                _ => None,
            }
        }
    }
}

/// Stimmen zwei Bezüge bis auf `tol` überein (gleiche Art, alle Zahlen; Richtungen normiert)?
pub fn geo_near(a: &Geo3, b: &Geo3, tol: f64) -> bool {
    let p = |x: DVec3, y: DVec3| x.distance(y) <= tol;
    let f = |x: f64, y: f64| (x - y).abs() <= tol;
    match (a, b) {
        (Geo3::Point(x), Geo3::Point(y)) => p(*x, *y),
        (Geo3::Line { a: a1, b: b1 }, Geo3::Line { a: a2, b: b2 }) => p(*a1, *a2) && p(*b1, *b2),
        (
            Geo3::Circle {
                center: c1,
                normal: n1,
                radius: r1,
                arc: a1,
            },
            Geo3::Circle {
                center: c2,
                normal: n2,
                radius: r2,
                arc: a2,
            },
        ) => {
            let arcs = match (a1, a2) {
                (None, None) => true,
                (Some(x), Some(y)) => {
                    p(x.mid, y.mid)
                        && ((p(x.start, y.start) && p(x.end, y.end))
                            || (p(x.start, y.end) && p(x.end, y.start)))
                }
                _ => false,
            };
            p(*c1, *c2) && p(n1.normalize(), n2.normalize()) && f(*r1, *r2) && arcs
        }
        (
            Geo3::Axis {
                origin: o1,
                dir: d1,
                radius: r1,
                t0: s1,
                t1: e1,
            },
            Geo3::Axis {
                origin: o2,
                dir: d2,
                radius: r2,
                t0: s2,
                t1: e2,
            },
        ) => {
            p(*o1, *o2)
                && p(d1.normalize(), d2.normalize())
                && f(*r1, *r2)
                && f(*s1, *s2)
                && f(*e1, *e2)
        }
        (
            Geo3::Sphere {
                center: c1,
                radius: r1,
            },
            Geo3::Sphere {
                center: c2,
                radius: r2,
            },
        ) => p(*c1, *c2) && f(*r1, *r2),
        _ => false,
    }
}

/// Radien aller Kreise, die ein Zufallskörper der Prüfreihe nach seinem Formelmodell haben kann
/// (eigene Formeln aus den Erzeugungsdaten): Zylinder `r`; Kegel `r0`, `r1` und `r(z)` an jeder
/// Ebene quer zu seiner Achse (Quaderseiten, Enden der Zylinder und Kegel); Kugel `R` und
/// `√(R² − d²)` an jeder achsparallelen Ebene im Abstand `d < R` von der Mitte; Schnittkreis
/// zweier Kugeln; Schnitt einer Kugel mit einem Kegel, dessen Achse durch ihre Mitte geht.
pub fn formula_radii(csg: &super::model::Csg) -> Vec<f64> {
    use super::model::Prim;
    let prims: Vec<Prim> = csg.parts.iter().map(|(p, _)| *p).collect();
    // Achsparallele Ebenen (Weltachse, Lage).
    let mut planes: Vec<(usize, f64)> = Vec::new();
    for p in &prims {
        match *p {
            Prim::Box { lo, hi } => {
                for k in 0..3 {
                    planes.push((k, lo[k]));
                    planes.push((k, hi[k]));
                }
            }
            Prim::Cylinder { axis, base, h, .. } | Prim::Cone { axis, base, h, .. } => {
                planes.push((axis, base[axis]));
                planes.push((axis, base[axis] + h));
            }
            Prim::Sphere { .. } => {}
        }
    }
    let mut out = Vec::new();
    for p in &prims {
        match *p {
            Prim::Box { .. } => {}
            Prim::Cylinder { r, .. } => out.push(r),
            Prim::Cone {
                axis,
                base,
                r0,
                r1,
                h,
            } => {
                out.push(r0);
                if r1 > 0.0 {
                    out.push(r1);
                }
                for &(k, at) in &planes {
                    let z = at - base[axis];
                    if k == axis && z > 0.0 && z < h {
                        out.push(r0 + (r1 - r0) * z / h);
                    }
                }
            }
            Prim::Sphere { c, r } => {
                out.push(r);
                for &(k, at) in &planes {
                    let d = (c[k] - at).abs();
                    if d < r {
                        out.push((r * r - d * d).sqrt());
                    }
                }
                for q in &prims {
                    match *q {
                        Prim::Sphere { c: c2, r: r2 } if c2 != c => {
                            let d = c.distance(c2);
                            let a = (d * d + r * r - r2 * r2) / (2.0 * d);
                            if a.abs() < r {
                                out.push((r * r - a * a).sqrt());
                            }
                        }
                        Prim::Cone {
                            axis,
                            base,
                            r0,
                            r1,
                            h,
                        } => {
                            // Achse durch die Kugelmitte? Dann ρ(z) = r0 + k·z (z ab `base`)
                            // und ρ² + (z − zc)² = R².
                            let mut off = c - base;
                            let zc = off[axis];
                            off[axis] = 0.0;
                            if off.length() > 1e-9 {
                                continue;
                            }
                            let k = (r1 - r0) / h;
                            let qa = k * k + 1.0;
                            let qb = 2.0 * (r0 * k - zc);
                            let qc = r0 * r0 + zc * zc - r * r;
                            let disc = qb * qb - 4.0 * qa * qc;
                            if disc >= 0.0 {
                                for s in [-1.0, 1.0] {
                                    let z = (-qb + s * disc.sqrt()) / (2.0 * qa);
                                    if (0.0..=h).contains(&z) {
                                        out.push(r0 + k * z);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    out
}
