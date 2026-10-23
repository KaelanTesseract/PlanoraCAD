//! Körper der Tests, die mehrere Testdateien brauchen (aus `curved.rs` und `freeform.rs`
//! hierher verschoben, Rümpfe unverändert; STEP-Import, Aufgabe 10).

use super::Lcg;
use planora_kernel::boolean::BoolOp;
use planora_kernel::{
    AnalyticKernel, BooleanError, Kernel, Profile, ProfileLoop, Segment, Solid, Surface, UvRect,
};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DMat4, DVec2, DVec3, EulerRot, Frame, PI, TAU};
use std::sync::Arc;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, m: DMat4) -> Solid {
    k().transform(s, &m).unwrap()
}

/// Lagen, in denen jeder Körper geprüft wird: wie gebaut, verschoben und gedreht, gespiegelt,
/// gespiegelt und gedreht.
pub fn poses() -> Vec<(&'static str, DMat4)> {
    vec![
        ("wie gebaut", DMat4::IDENTITY),
        (
            "gedreht",
            DMat4::from_translation(DVec3::new(5.0, -3.0, 7.0))
                * DMat4::from_rotation_x(0.9)
                * DMat4::from_rotation_z(0.4),
        ),
        ("gespiegelt", DMat4::from_scale(DVec3::new(1.0, -1.0, 1.0))),
        (
            "gespiegelt und gedreht",
            DMat4::from_rotation_y(-1.3) * DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)),
        ),
    ]
}

/// Ring mit Torusfläche: Kette (0,0) → (8,0) → Halbkreis um (8,3) nach außen → (0,6).
pub fn torus_ring() -> Solid {
    k().revolve(
        &ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(8.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(8.0, 3.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 6.0),
                },
            ],
        },
        &Frame::WORLD,
    )
    .unwrap()
}

/// Quader 10 × 10 × 4 mit einer nach innen gewölbten Bucht (Bogen im Uhrzeigersinn).
pub fn notched() -> Solid {
    let outer = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(10.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(10.0, 10.0),
            },
            Segment::Arc {
                center: DVec2::new(5.0, 10.0),
                sweep: -PI,
            },
            Segment::Line { to: DVec2::ZERO },
        ],
    };
    k().extrude(
        &Profile {
            outer,
            holes: vec![],
        },
        &Frame::WORLD,
        4.0,
    )
    .unwrap()
}

/// Rohr: Kreis r = 5 mit Loch r = 3, Höhe 8.
pub fn tube() -> Solid {
    k().extrude(
        &Profile {
            outer: ProfileLoop::circle(DVec2::ZERO, 5.0),
            holes: vec![ProfileLoop::circle(DVec2::ZERO, 3.0)],
        },
        &Frame::WORLD,
        8.0,
    )
    .unwrap()
}

/// Kubischer Spline durch (10,0), (12,5), (10,10) (nicht rational).
pub fn bulge() -> NurbsCurve {
    NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(12.0, 5.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
        ],
        &[0.0, 0.5, 1.0],
    )
    .unwrap()
}

/// Quadrat 10 × 10, rechte Seite als Spline, Höhe 4.
pub fn spline_prism() -> Solid {
    let outer = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(10.0, 0.0),
            },
            Segment::Nurbs {
                curve: Arc::new(bulge()),
            },
            Segment::Line {
                to: DVec2::new(0.0, 10.0),
            },
            Segment::Line { to: DVec2::ZERO },
        ],
    };
    k().extrude(
        &Profile {
            outer,
            holes: vec![],
        },
        &Frame::WORLD,
        4.0,
    )
    .unwrap()
}

/// Spline in der (r, z)-Halbebene von (4, 0) über (5, 3) nach (3, 6).
pub fn meridian() -> NurbsCurve {
    NurbsCurve::natural_cubic(
        &[
            DVec3::new(4.0, 0.0, 0.0),
            DVec3::new(5.0, 3.0, 0.0),
            DVec3::new(3.0, 6.0, 0.0),
        ],
        &[0.0, 0.5, 1.0],
    )
    .unwrap()
}

/// Drehkörper: (0,0) → (4,0) → Spline → (3,6) → (0,6); die Mantelfläche ist eine rationale
/// Drehfläche.
pub fn vase() -> Solid {
    k().revolve(
        &ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(4.0, 0.0),
                },
                Segment::Nurbs {
                    curve: Arc::new(meridian()),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 6.0),
                },
            ],
        },
        &Frame::WORLD,
    )
    .unwrap()
}

/// Kugel r = 5, deren Fläche eine NURBS-Drehfläche mit Polen ist (Kanten bleiben Kreise).
pub fn nurbs_sphere() -> Solid {
    let r = 5.0;
    let mut solid = k().make_sphere(r).unwrap();
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, -r),
        DVec3::new(r, 0.0, 0.0),
        0.0,
        PI,
    )
    .unwrap();
    let ns = NurbsSurface::revolve(&half, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    let ((u0, u1), (v0, v1)) = ns.domain();
    let surface = Surface::nurbs(ns, true, false).unwrap();
    let (id, f) = solid.faces.iter().next().unwrap();
    let p = DVec3::new(r * 0.6, r * 0.8, 0.0);
    let (ou, ov) = f.surface.project(p);
    let outward = f.surface.normal(ou, ov) * if f.same_sense { 1.0 } else { -1.0 };
    let (nu, nv) = surface.project(p);
    let same_sense = surface.normal(nu, nv).dot(outward) > 0.0;
    let f = &mut solid.faces[id];
    f.surface = surface;
    f.same_sense = same_sense;
    f.patch = Some(UvRect { u0, u1, v0, v1 });
    solid
}

/// Rationale Kurve Grad 3 mit ungleichen Gewichten und einem inneren Knoten bei 0,4 durch
/// `points` (fünf Stück): Vertauschte Gewichte oder Knoten würden die Gestalt ändern.
pub fn weighted(points: [DVec3; 5]) -> NurbsCurve {
    NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0, 1.0],
        points.to_vec(),
        vec![1.0, 3.0, 0.5, 2.0, 1.0],
    )
    .unwrap()
}

/// Rechte Seite des Prismas mit ungleichen Gewichten, von (10, 0) nach (10, 10).
pub fn weighted_side() -> NurbsCurve {
    weighted([
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(13.0, 2.0, 0.0),
        DVec3::new(11.0, 5.0, 0.0),
        DVec3::new(14.0, 8.0, 0.0),
        DVec3::new(10.0, 10.0, 0.0),
    ])
}

/// Quadrat 10 × 10 mit der gewichteten rechten Seite, Höhe 4.
pub fn weighted_prism() -> Solid {
    let outer = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(10.0, 0.0),
            },
            Segment::Nurbs {
                curve: Arc::new(weighted_side()),
            },
            Segment::Line {
                to: DVec2::new(0.0, 10.0),
            },
            Segment::Line { to: DVec2::ZERO },
        ],
    };
    k().extrude(
        &Profile {
            outer,
            holes: vec![],
        },
        &Frame::WORLD,
        4.0,
    )
    .unwrap()
}

/// Zufällige Lage: verschoben um bis zu ±2 und beliebig gedreht.
pub fn random_pose(rng: &mut Lcg) -> DMat4 {
    DMat4::from_translation(DVec3::new(
        rng.range(-2.0, 2.0),
        rng.range(-2.0, 2.0),
        rng.range(-2.0, 2.0),
    )) * DMat4::from_euler(
        EulerRot::XYZ,
        rng.range(0.0, 3.0),
        rng.range(0.0, 3.0),
        rng.range(0.0, 3.0),
    )
}

/// Zufälliger Körper: ein gedrehter Quader, verrechnet mit einem gedrehten Quader, einem Zylinder
/// oder einer Kugel (Paare, die der Kernel kann; Zylinder mit Zylinder oder Kugel schräg zur
/// Achse rechnet er noch nicht). `None` nur bei leerer Schnittmenge; jeder andere Fehler des
/// Kernels lässt den Test fehlschlagen.
pub fn random_body(rng: &mut Lcg) -> Option<Solid> {
    let size = |rng: &mut Lcg| {
        DVec3::new(
            rng.range(2.0, 8.0),
            rng.range(2.0, 8.0),
            rng.range(2.0, 8.0),
        )
    };
    let a = moved(&k().make_box(size(rng)).unwrap(), random_pose(rng));
    let b = match (rng.uniform() * 3.0) as usize {
        0 => k().make_box(size(rng)).unwrap(),
        1 => k()
            .make_cylinder(rng.range(1.0, 4.0), rng.range(2.0, 8.0))
            .unwrap(),
        _ => k().make_sphere(rng.range(1.0, 4.0)).unwrap(),
    };
    let b = moved(&b, random_pose(rng));
    let op =
        [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection][(rng.uniform() * 3.0) as usize];
    match k().boolean(&a, &b, op) {
        Ok(s) => Some(s),
        Err(BooleanError::EmptyResult) => None,
        Err(e) => panic!("Boolesche Operation {op:?}: {e}"),
    }
}
