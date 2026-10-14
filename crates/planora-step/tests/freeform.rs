//! Freiformkurven und -flächen: rational und nicht rational, geschlossen, mit Polen; Knoten mit
//! Vielfachheiten; Punktprobe und Volumen aus den gelesenen B-Splines (eigenes de Boor)
//! (Teilprojekt 10, Task 10).

mod common;

use common::check::measure;
use common::model::{Crv, Model, Surf};
use common::{checked, export};
use planora_kernel::{
    AnalyticKernel, Curve, Kernel, Profile, ProfileLoop, Segment, Solid, Surface, UvRect,
};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU};
use std::sync::Arc;

/// Sehnentoleranz der Netze (Freiform: gröber, die Suche des Fußpunkts ist teuer).
const CHORD: f64 = 0.01;
/// Wie in `stage2.rs`: Volumenfehler höchstens `SAG_FACTOR · CHORD · Oberfläche`.
const SAG_FACTOR: f64 = 2.0;
/// Schranke gegen das Kernel-Netz derselben Teilung (Fußpunktsuche auf der Freiformfläche).
const MESH_MATCH_REL: f64 = 1e-7;
/// Größter Abstand eines Punkts der STEP-Fläche von seiner Fläche in mm: Die Fußpunktsuche auf
/// Freiformflächen löst nur bis hierher auf (gleiche Größenordnung wie `MESH_MATCH_REL`).
const MAX_OFFSET: f64 = 1e-7;
/// Teilung der Simpson-Summen in den Formeln.
const SIMPSON_STEPS: usize = 2000;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Simpson-Summe von `f` über `[a, b]`.
fn simpson(a: f64, b: f64, f: impl Fn(f64) -> f64) -> f64 {
    let n = SIMPSON_STEPS;
    let h = (b - a) / n as f64;
    let mut s = f(a) + f(b);
    for i in 1..n {
        let w = if i % 2 == 1 { 4.0 } else { 2.0 };
        s += w * f(a + h * i as f64);
    }
    s * h / 3.0
}

/// Ellipse 6 × 3 als geschlossene rationale Kurve, extrudiert um 5.
fn ellipse_prism() -> Solid {
    let c =
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * 6.0, DVec3::Y * 3.0, 0.0, TAU).unwrap();
    k().extrude(
        &Profile {
            outer: ProfileLoop::nurbs_loop(c).unwrap(),
            holes: vec![],
        },
        &Frame::WORLD,
        5.0,
    )
    .unwrap()
}

/// Kubischer Spline durch (10,0), (12,5), (10,10) (nicht rational).
fn bulge() -> NurbsCurve {
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
fn spline_prism() -> Solid {
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
fn meridian() -> NurbsCurve {
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
fn vase() -> Solid {
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
fn nurbs_sphere() -> Solid {
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

/// Fläche zwischen Spline und der Linie x = 10 (Spline rechts davon).
fn bulge_area() -> f64 {
    let c = bulge();
    // ∫ (x − 10) dy entlang des Splines.
    simpson(0.0, 1.0, |t| {
        let d = c.derivs(t, 1);
        (d[0].x - 10.0) * d[1].y
    })
}

/// Volumen der Vase: π ∫ r² dz über den Spline (die Strecken auf der Achse und die waagrechten
/// tragen nichts bei).
fn vase_volume() -> f64 {
    let c = meridian();
    PI * simpson(0.0, 1.0, |t| {
        let d = c.derivs(t, 1);
        d[0].x * d[0].x * d[1].y
    })
    .abs()
}

/// Rationale Kurve Grad 3 mit ungleichen Gewichten und einem inneren Knoten bei 0,4 durch
/// `points` (fünf Stück): Vertauschte Gewichte oder Knoten würden die Gestalt ändern.
fn weighted(points: [DVec3; 5]) -> NurbsCurve {
    NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0, 1.0],
        points.to_vec(),
        vec![1.0, 3.0, 0.5, 2.0, 1.0],
    )
    .unwrap()
}

/// Rechte Seite des Prismas mit ungleichen Gewichten, von (10, 0) nach (10, 10).
fn weighted_side() -> NurbsCurve {
    weighted([
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(13.0, 2.0, 0.0),
        DVec3::new(11.0, 5.0, 0.0),
        DVec3::new(14.0, 8.0, 0.0),
        DVec3::new(10.0, 10.0, 0.0),
    ])
}

/// Quadrat 10 × 10 mit der gewichteten rechten Seite, Höhe 4.
fn weighted_prism() -> Solid {
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

/// Meridian mit ungleichen Gewichten in der (r, z)-Halbebene von (4, 0) nach (3, 6).
fn weighted_meridian() -> NurbsCurve {
    weighted([
        DVec3::new(4.0, 0.0, 0.0),
        DVec3::new(6.0, 1.0, 0.0),
        DVec3::new(3.0, 3.0, 0.0),
        DVec3::new(5.0, 5.0, 0.0),
        DVec3::new(3.0, 6.0, 0.0),
    ])
}

/// Drehkörper mit dem gewichteten Meridian.
fn weighted_vase() -> Solid {
    k().revolve(
        &ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(4.0, 0.0),
                },
                Segment::Nurbs {
                    curve: Arc::new(weighted_meridian()),
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

/// Volumen des gewichteten Prismas: Quadrat plus Fläche zwischen Kurve und x = 10.
fn weighted_prism_volume() -> f64 {
    let c = weighted_side();
    let area = simpson(0.0, 1.0, |t| {
        let d = c.derivs(t, 1);
        (d[0].x - 10.0) * d[1].y
    });
    (100.0 + area) * 4.0
}

/// Volumen der gewichteten Vase: π ∫ r² dz.
fn weighted_vase_volume() -> f64 {
    let c = weighted_meridian();
    PI * simpson(0.0, 1.0, |t| {
        let d = c.derivs(t, 1);
        d[0].x * d[0].x * d[1].y
    })
    .abs()
}

/// (Name, Körper, Volumen nach Formel).
fn bodies() -> Vec<(&'static str, Solid, f64)> {
    vec![
        ("Gewichte-Prisma", weighted_prism(), weighted_prism_volume()),
        ("Gewichte-Vase", weighted_vase(), weighted_vase_volume()),
        ("Ellipse", ellipse_prism(), PI * 6.0 * 3.0 * 5.0),
        (
            "Spline-Prisma",
            spline_prism(),
            (100.0 + bulge_area()) * 4.0,
        ),
        ("Vase", vase(), vase_volume()),
        ("NURBS-Kugel", nurbs_sphere(), 4.0 / 3.0 * PI * 125.0),
    ]
}

fn poses() -> Vec<(&'static str, DMat4)> {
    vec![
        ("wie gebaut", DMat4::IDENTITY),
        (
            "gedreht",
            DMat4::from_translation(DVec3::new(3.0, 1.0, -2.0)) * DMat4::from_rotation_x(1.1),
        ),
        ("gespiegelt", DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0))),
    ]
}

#[test]
fn freeform_bodies_pass_all_checks_and_keep_their_volume() {
    for (name, body, volume) in bodies() {
        for (pose, m) in poses() {
            let s = k().transform(&body, &m).unwrap();
            let (text, _) = export(&[(name, &s)]);
            let (model, _) = std::panic::catch_unwind(|| checked(&text, &[&s]))
                .unwrap_or_else(|_| panic!("{name}, {pose}"));
            let r = measure(&model, &[&s], CHORD).unwrap();
            assert_eq!(r.flipped, 0, "{name}, {pose}");
            assert!(
                r.max_offset < MAX_OFFSET,
                "{name}, {pose}: {}",
                r.max_offset
            );
            assert!(
                ((r.volume - r.kernel_volume) / r.kernel_volume).abs() < MESH_MATCH_REL,
                "{name}, {pose}: {} gegen {}",
                r.volume,
                r.kernel_volume
            );
            assert!(
                (r.volume - volume).abs() <= SAG_FACTOR * CHORD * r.area,
                "{name}, {pose}: {} statt {volume}",
                r.volume
            );
        }
    }
}

#[test]
fn rational_and_plain_b_splines_use_the_right_entities() {
    let s = ellipse_prism();
    let (text, _) = export(&[("Ellipse", &s)]);
    assert!(text.contains("RATIONAL_B_SPLINE_CURVE("));
    assert!(text.contains("RATIONAL_B_SPLINE_SURFACE("));
    let m = Model::read(&text).unwrap();
    let closed: Vec<bool> = m
        .edges
        .values()
        .filter_map(|e| match &e.curve {
            Crv::Spline(c) => Some(c.closed),
            _ => None,
        })
        .collect();
    assert_eq!(closed, vec![true, true], "Ober- und Unterkante geschlossen");
    let wall = m.shells()[0]
        .faces
        .iter()
        .find_map(|f| match &f.surface {
            Surf::Spline(sp) => Some(sp.clone()),
            _ => None,
        })
        .unwrap();
    assert!(wall.u_closed && !wall.v_closed);
    let p = Model::read(&export(&[("Prisma", &spline_prism())]).0).unwrap();
    assert_eq!(
        p.file.count("B_SPLINE_CURVE_WITH_KNOTS"),
        2,
        "einfach, nicht rational"
    );
    assert_eq!(p.file.count("B_SPLINE_SURFACE_WITH_KNOTS"), 1);
    assert!(
        !export(&[("Prisma", &spline_prism())])
            .0
            .contains("RATIONAL")
    );
}

#[test]
fn knots_are_written_as_values_with_multiplicities() {
    let s = ellipse_prism();
    let (text, _) = export(&[("Ellipse", &s)]);
    let m = Model::read(&text).unwrap();
    let c = m
        .edges
        .values()
        .find_map(|e| match &e.curve {
            Crv::Spline(c) => Some(c.clone()),
            _ => None,
        })
        .unwrap();
    // Vollellipse aus vier Viertelbögen vom Grad 2: Knoten 0 (3×), 1/4, 1/2, 3/4 (je 2×), 1 (3×).
    let kernel = s
        .edges
        .iter()
        .find_map(|(_, e)| match &e.curve {
            Curve::Nurbs(n) => Some(n.knots().knots().to_vec()),
            _ => None,
        })
        .unwrap();
    assert_eq!(c.knots, kernel);
    assert_eq!(c.degree, 2);
    assert!(text.contains("(3,2,2,2,3)"), "Vielfachheiten");
}

#[test]
fn poles_of_a_freeform_sphere_are_vertices_with_a_seam_used_twice() {
    let s = nurbs_sphere();
    let (text, _) = export(&[("NURBS-Kugel", &s)]);
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(
        (stats[0].faces, stats[0].edges, stats[0].vertices),
        (1, 1, 2)
    );
    let face = &m.shells()[0].faces[0];
    assert!(face.surface.is_spline());
    let b = &face.bounds[0];
    assert_eq!(b.edges.len(), 2);
    assert_eq!(b.edges[0].edge, b.edges[1].edge);
}
