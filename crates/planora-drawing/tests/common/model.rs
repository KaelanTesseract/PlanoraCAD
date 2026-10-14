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
