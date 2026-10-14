//! Prüfkörper mit bekannten Maßen und eigene Messungen (ohne Code aus `planora-drawing`).

use planora_drawing::{basis::Basis, stroke::Stroke};
use planora_kernel::primitives::{make_box, make_cone, make_cylinder, make_sphere};
use planora_kernel::{
    BlendEdge, BlendKind, BlendRequest, Curve, Edge, Face, HalfEdge, Id, Loop, Profile,
    ProfileLoop, Segment, Solid, Surface, UvRect, Vertex, blend, extrude, revolve, validate,
};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, TAU, Tolerance};
use std::sync::Arc;

fn tol() -> Tolerance {
    Tolerance::default()
}

/// Quader `[0,a] × [0,b] × [0,c]` (Ecke im Ursprung).
pub fn box_solid(a: f64, b: f64, c: f64) -> Solid {
    make_box(DVec3::new(a, b, c), &tol()).unwrap()
}

/// Zylinder, Achse Z, Mittelpunkt des Bodens im Ursprung.
pub fn cylinder_solid(r: f64, h: f64) -> Solid {
    make_cylinder(r, h, &tol()).unwrap()
}

/// Kugel um den Ursprung.
pub fn sphere_solid(r: f64) -> Solid {
    make_sphere(r, &tol()).unwrap()
}

/// Kegelstumpf, Achse Z, Boden im Ursprung: Bodenradius `r0`, Deckelradius `r1`, Höhe `h`
/// (`r1 == 0`: spitzer Kegel).
pub fn cone_solid(r0: f64, r1: f64, h: f64) -> Solid {
    if r1 == 0.0 {
        return make_cone(r0, h, &tol()).unwrap();
    }
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(r0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(r1, h),
            },
            Segment::Line {
                to: DVec2::new(0.0, h),
            },
        ],
    };
    revolve(&chain, &Frame::WORLD, &tol()).unwrap()
}

/// Vollring (eine Torusfläche, zwei Nähte), Achse Z, Mittelpunkt im Ursprung.
pub fn torus_solid(major: f64, minor: f64) -> Solid {
    let mut s = Solid::new();
    let v = s.add_vertex(DVec3::new(major + minor, 0.0, 0.0));
    // Meridiankreis bei u = 0: Punkt(t) = Torus(u = 0, v = t).
    let meridian = Frame {
        origin: DVec3::new(major, 0.0, 0.0),
        x: DVec3::X,
        y: DVec3::Z,
        z: -DVec3::Y,
    };
    let e_v = s.add_edge(
        Curve::Circle {
            frame: meridian,
            radius: minor,
        },
        0.0,
        TAU,
        v,
        v,
    );
    // Äußerer Parallelkreis bei v = 0: Punkt(t) = Torus(u = t, v = 0).
    let e_u = s.add_edge(
        Curve::Circle {
            frame: Frame::WORLD,
            radius: major + minor,
        },
        0.0,
        TAU,
        v,
        v,
    );
    let he = |edge: Id<Edge>, forward: bool| HalfEdge { edge, forward };
    s.add_face(Face {
        surface: Surface::Torus {
            frame: Frame::WORLD,
            major,
            minor,
        },
        same_sense: true,
        outer: Loop {
            half_edges: vec![he(e_u, true), he(e_v, true), he(e_u, false), he(e_v, false)],
        },
        inner: vec![],
        patch: Some(UvRect {
            u0: 0.0,
            u1: TAU,
            v0: 0.0,
            v1: TAU,
        }),
        tag: None,
    });
    s.seal();
    validate(&s).expect("Torus ist gültig");
    s
}

/// Halbzylinder: Halbkreis (Radius `r`, Mittelpunkt im Ursprung, y ≥ 0) mal Höhe `h` in Z.
pub fn half_cylinder_solid(r: f64, h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::new(-r, 0.0),
            segments: vec![
                Segment::Line {
                    to: DVec2::new(r, 0.0),
                },
                Segment::Arc {
                    center: DVec2::ZERO,
                    sweep: std::f64::consts::PI,
                },
            ],
        },
        holes: vec![],
    };
    extrude(&profile, &Frame::WORLD, h, &tol()).unwrap()
}

/// Platte mit einer Freiformwand: Grundriss (0,0) → (10,0) → Spline durch (10,0), (12,5), (10,10)
/// → (0,10) → (0,0), Höhe 5. Liefert den Körper und den Index der Freiformfläche.
pub fn nurbs_slab_solid() -> (Solid, usize) {
    let spline = NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(12.0, 5.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
        ],
        &[0.0, 1.0, 2.0],
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Nurbs {
                    curve: Arc::new(spline),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 10.0),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let s = extrude(&profile, &Frame::WORLD, 5.0, &tol()).unwrap();
    let face = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        .map(|(id, _)| id.index())
        .expect("Freiformfläche");
    (s, face)
}

/// Punkt auf der Freiformwand der Platte (Mitte des Splines, halbe Höhe) und ihre Außennormale,
/// aus der Symmetrie des Splines (Spiegelung an y = 5) von Hand bestimmt.
pub fn nurbs_slab_center_point_and_normal() -> (DVec3, DVec3) {
    (DVec3::new(12.0, 5.0, 2.5), DVec3::X)
}

/// Quader mit einer verrundeten Kante (Kante parallel zu Y bei x = a, z = c, Radius `r`).
pub fn box_with_one_rounded_edge(a: f64, b: f64, c: f64, r: f64) -> Solid {
    let s = box_solid(a, b, c);
    let at = |id: Id<Vertex>| s.vertices[id].point;
    let edge = s
        .edges
        .iter()
        .find(|(_, e)| {
            let (p, q) = (at(e.start), at(e.end));
            (p.x - a).abs() < 1e-9
                && (q.x - a).abs() < 1e-9
                && (p.z - c).abs() < 1e-9
                && (q.z - c).abs() < 1e-9
                && (p.y - q.y).abs() > 1e-9
        })
        .map(|(id, _)| id)
        .expect("Kante bei x = a, z = c");
    let req = BlendRequest {
        edges: vec![BlendEdge {
            edge,
            first_face: None,
        }],
        kind: BlendKind::Fillet { radius: r },
        tangent_chain: false,
    };
    let out = blend::blend(&s, &req, &tol()).unwrap();
    validate(&out.solid).expect("gerundeter Quader ist gültig");
    out.solid
}

/// Flächen je Kante (Index in `solid.faces`), je Halbkante einmal.
fn faces_of_edges(s: &Solid) -> Vec<Vec<usize>> {
    let mut v: Vec<Vec<usize>> = vec![vec![]; s.edges.len()];
    for (fid, f) in s.faces.iter() {
        for lp in std::iter::once(&f.outer).chain(f.inner.iter()) {
            for he in &lp.half_edges {
                v[he.edge.index()].push(fid.index());
            }
        }
    }
    v
}

fn is_seam(faces: &[usize]) -> bool {
    faces.len() >= 2 && faces.iter().all(|&f| f == faces[0])
}

/// Zahl der Nahtkanten (beide Seiten dieselbe Fläche).
pub fn count_seam_edges(s: &Solid) -> usize {
    faces_of_edges(s).iter().filter(|f| is_seam(f)).count()
}

/// Zahl der glatten Kanten zwischen zwei **verschiedenen** Flächen: Winkel der Außennormalen an
/// 5 gleichverteilten Stellen der Kante überall höchstens 0,5°.
pub fn count_smooth_edges(s: &Solid) -> usize {
    let limit = (0.5f64).to_radians().cos();
    let uses = faces_of_edges(s);
    let mut n = 0;
    for (eid, e) in s.edges.iter() {
        let f = &uses[eid.index()];
        if f.len() != 2 || is_seam(f) {
            continue;
        }
        let smooth = (0..5).all(|i| {
            let t = e.t0 + (e.t1 - e.t0) * i as f64 / 4.0;
            let p = e.curve.point(t);
            let normal = |fi: usize| {
                let face = &s.faces[Id::<Face>::from_index(fi)];
                let (u, v) = face.surface.project(p);
                let n = face.surface.normal(u, v);
                if face.same_sense { n } else { -n }
            };
            normal(f[0]).dot(normal(f[1])) >= limit
        });
        if smooth {
            n += 1;
        }
    }
    n
}

/// Gesamtlänge der projizierten Striche: Mittelpunktsregel `∑ |p'(t)| dt` mit 4096 Stützstellen
/// je Strich, `p'` als symmetrischer Differenzenquotient der projizierten Kurvenpunkte.
pub fn total_len(strokes: &[Stroke], basis: &Basis) -> f64 {
    const N: usize = 4096;
    let mut total = 0.0;
    for s in strokes {
        let proj = |t: f64| {
            let p = s.curve.point(t);
            (p.dot(basis.ex), p.dot(basis.ey))
        };
        let dt = (s.t1 - s.t0) / N as f64;
        let h = (s.t1 - s.t0).abs() * 1e-6;
        for i in 0..N {
            let t = s.t0 + dt * (i as f64 + 0.5);
            let (a, b) = (proj(t + h), proj(t - h));
            let d = ((a.0 - b.0) / (2.0 * h)).hypot((a.1 - b.1) / (2.0 * h));
            total += d * dt.abs();
        }
    }
    total
}

/// Kubische Bézierkurve als NURBS (offener Knotenvektor `[0,0,0,0,1,1,1,1]`, Gewichte 1) mit
/// den vier Punkten als Kontrollpunkten.
pub fn cubic_from_controls(p: &[(f64, f64, f64)]) -> NurbsCurve {
    assert_eq!(p.len(), 4, "genau vier Kontrollpunkte");
    NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        p.iter().map(|&(x, y, z)| DVec3::new(x, y, z)).collect(),
        vec![1.0; 4],
    )
    .unwrap()
}

/// Kubische Bézierkurve (wie [`cubic_from_controls`]), die **durch** die vier Punkte läuft, bei
/// den Parametern 0, 1/3, 2/3, 1. Die inneren Kontrollpunkte von Hand aus den
/// Bernstein-Gleichungen `B(1/3) = Q1`, `B(2/3) = Q2`:
/// `R1 = 27·Q1 − 8·Q0 − Q3 = 12·P1 + 6·P2`, `R2 = 27·Q2 − Q0 − 8·Q3 = 6·P1 + 12·P2`,
/// also `P1 = (2·R1 − R2) / 18`, `P2 = (2·R2 − R1) / 18`.
pub fn cubic_through_controls(q: &[(f64, f64, f64)]) -> NurbsCurve {
    assert_eq!(q.len(), 4, "genau vier Punkte");
    let v: Vec<DVec3> = q.iter().map(|&(x, y, z)| DVec3::new(x, y, z)).collect();
    let r1 = v[1] * 27.0 - v[0] * 8.0 - v[3];
    let r2 = v[2] * 27.0 - v[0] - v[3] * 8.0;
    let p1 = (r1 * 2.0 - r2) / 18.0;
    let p2 = (r2 * 2.0 - r1) / 18.0;
    let c = NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        vec![v[0], p1, p2, v[3]],
        vec![1.0; 4],
    )
    .unwrap();
    for (i, want) in v.iter().enumerate() {
        let got = c.point(i as f64 / 3.0);
        assert!(
            got.distance(*want) < 1e-9,
            "Kurve läuft nicht durch Punkt {i}"
        );
    }
    c
}

/// Geschlossene kubische Freiformkurve in der XY-Ebene: Anfang und Ende bei (10, 0, 0), dort
/// senkrecht nach +Y laufend; sie umrundet den Ursprung einmal.
pub fn closed_nurbs_loop() -> NurbsCurve {
    let pts = [
        (10.0, 0.0),
        (10.0, 10.0),
        (-10.0, 10.0),
        (-10.0, -10.0),
        (10.0, -10.0),
        (10.0, -5.0),
        (10.0, 0.0),
    ];
    NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0],
        pts.iter().map(|&(x, y)| DVec3::new(x, y, 0.0)).collect(),
        vec![1.0; 7],
    )
    .unwrap()
}

/// Zahl der Schnittpunkte der projizierten Striche, unabhängig von der Bibliothek gezählt:
/// beide Striche werden an `n + 1` gleichverteilten Parametern abgetastet (eigene Projektion
/// `(P·ex, P·ey)`), dann werden alle Streckenpaare der beiden Polygonzüge geprüft (halboffene
/// Strecken `[0, 1)`, damit ein Schnitt genau in einer Ecke nur einmal zählt). Ein Raster über
/// den Strecken von `b` spart die meisten Paartests.
pub fn brute_force_cross_count(a: &Stroke, b: &Stroke, basis: &Basis, n: usize) -> usize {
    let poly = |s: &Stroke| -> Vec<(f64, f64)> {
        (0..=n)
            .map(|i| {
                let t = s.t0 + (s.t1 - s.t0) * i as f64 / n as f64;
                let p = s.curve.point(t);
                (p.dot(basis.ex), p.dot(basis.ey))
            })
            .collect()
    };
    let (pa, pb) = (poly(a), poly(b));
    let (mut lo, mut hi) = (
        (f64::INFINITY, f64::INFINITY),
        (f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for p in pa.iter().chain(pb.iter()) {
        lo = (lo.0.min(p.0), lo.1.min(p.1));
        hi = (hi.0.max(p.0), hi.1.max(p.1));
    }
    const G: usize = 512;
    let cw = ((hi.0 - lo.0) / G as f64).max(1e-12);
    let ch = ((hi.1 - lo.1) / G as f64).max(1e-12);
    let cell = |x: f64, y: f64| {
        let i = (((x - lo.0) / cw).floor().max(0.0) as usize).min(G - 1);
        let j = (((y - lo.1) / ch).floor().max(0.0) as usize).min(G - 1);
        (i, j)
    };
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); G * G];
    let seg_cells = |p: (f64, f64), q: (f64, f64)| {
        let (i0, j0) = cell(p.0.min(q.0), p.1.min(q.1));
        let (i1, j1) = cell(p.0.max(q.0), p.1.max(q.1));
        (i0, j0, i1, j1)
    };
    for k in 0..pb.len() - 1 {
        let (i0, j0, i1, j1) = seg_cells(pb[k], pb[k + 1]);
        for i in i0..=i1 {
            for j in j0..=j1 {
                grid[i * G + j].push(k);
            }
        }
    }
    let cross = |u: (f64, f64), v: (f64, f64)| u.0 * v.1 - u.1 * v.0;
    let sub = |u: (f64, f64), v: (f64, f64)| (u.0 - v.0, u.1 - v.1);
    let mut seen = vec![usize::MAX; pb.len()];
    let mut count = 0;
    for k in 0..pa.len() - 1 {
        let (a0, a1) = (pa[k], pa[k + 1]);
        let (i0, j0, i1, j1) = seg_cells(a0, a1);
        for i in i0..=i1 {
            for j in j0..=j1 {
                for &m in &grid[i * G + j] {
                    if seen[m] == k {
                        continue;
                    }
                    seen[m] = k;
                    let (b0, b1) = (pb[m], pb[m + 1]);
                    let r = sub(a1, a0);
                    let s = sub(b1, b0);
                    let den = cross(r, s);
                    if den == 0.0 {
                        continue;
                    }
                    let w = sub(b0, a0);
                    let u = cross(w, s) / den;
                    let v = cross(w, r) / den;
                    if (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v) {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}
