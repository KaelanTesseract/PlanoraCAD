//! Zylinder, Kugel, Torus und Ellipse mit Nähten, Polen und Vollkreisen; Punktprobe auch
//! gespiegelt und gedreht (Teilprojekt 10, Task 7).

mod common;

use common::bodies::{notched, poses, torus_ring, tube};
use common::check::check_geometry;
use common::model::{Crv, Model, Surf};
use common::{checked, export, options};
use planora_exchange::Progress;
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Curve, Kernel, ProfileLoop, Segment, Solid, Surface};
use planora_math::{DMat4, DVec2, DVec3, Frame, PI};
use planora_step::{StepBody, StepError, write_step};

/// Verschiebung der großen Körper (mm) und ihr Radius.
const BIG_OFFSET: f64 = 200_000.0;
const BIG_RADIUS: f64 = 1000.0;
/// Verschiebung eines Eckpunkts (mm), die die Prüfung finden muss.
const SHIFT: f64 = 1e-6;

/// Höchstabstand eines Pols der Kugel von seinem Sollpunkt (mm); die Zahlen stehen verlustfrei
/// in der Datei.
const POLE_TOL: f64 = 1e-12;
/// Höchstabweichung der Halbachsen einer Schnittellipse vom Sollwert (mm).
const AXIS_TOL: f64 = 1e-9;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, m: DMat4) -> Solid {
    k().transform(s, &m).unwrap()
}

/// Zylinder r = 5, h = 40, oben schräg abgeschnitten (Ellipse als Schnittkante).
fn slanted() -> Solid {
    let cylinder = k().make_cylinder(5.0, 40.0).unwrap();
    let block = k().make_box(DVec3::splat(200.0)).unwrap();
    let block = moved(
        &block,
        DMat4::from_translation(DVec3::new(0.0, 0.0, 25.0))
            * DMat4::from_rotation_x(0.5)
            * DMat4::from_translation(DVec3::new(-100.0, -100.0, 0.0)),
    );
    k().boolean(&cylinder, &block, BoolOp::Difference).unwrap()
}

fn surfaces(m: &Model) -> Vec<&Surf> {
    m.shells()
        .iter()
        .flat_map(|s| s.faces.iter())
        .map(|f| &f.surface)
        .collect()
}

#[test]
fn every_curved_body_passes_all_checks_in_every_pose() {
    for (name, body) in [
        ("Zylinder", k().make_cylinder(4.0, 9.0).unwrap()),
        ("Kugel", k().make_sphere(6.0).unwrap()),
        ("Ring", torus_ring()),
        ("Bucht", notched()),
        ("Rohr", tube()),
        ("Schräg", slanted()),
    ] {
        for (pose, m) in poses() {
            let s = moved(&body, m);
            let (text, _) = export(&[(name, &s)]);
            let (_, stats) = std::panic::catch_unwind(|| checked(&text, &[&s]))
                .unwrap_or_else(|_| panic!("{name}, {pose}"));
            assert_eq!(stats.len(), 1, "{name}, {pose}");
        }
    }
}

#[test]
fn a_cylinder_has_one_seam_used_twice_and_full_circles_with_one_vertex() {
    let s = k().make_cylinder(4.0, 9.0).unwrap();
    let (text, _) = export(&[("Zylinder", &s)]);
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(
        (stats[0].faces, stats[0].edges, stats[0].vertices),
        (3, 3, 2)
    );
    let mantle = m.shells()[0]
        .faces
        .iter()
        .find(|f| matches!(f.surface, Surf::Cylinder { .. }))
        .unwrap();
    let line = m
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, Crv::Line { .. }))
        .map(|(id, _)| *id)
        .unwrap();
    let uses: Vec<bool> = mantle.bounds[0]
        .edges
        .iter()
        .filter(|o| o.edge == line)
        .map(|o| o.forward)
        .collect();
    assert_eq!(uses.len(), 2, "die Naht zweimal in derselben Schleife");
    assert_ne!(uses[0], uses[1], "in entgegengesetzter Richtung");
    let circles: Vec<_> = m
        .edges
        .values()
        .filter(|e| matches!(e.curve, Crv::Circle { .. }))
        .collect();
    assert_eq!(circles.len(), 2);
    assert!(
        circles.iter().all(|e| e.start == e.end),
        "Vollkreis: ein Eckpunkt"
    );
}

#[test]
fn a_sphere_is_one_face_with_poles_as_vertices() {
    let s = k().make_sphere(6.0).unwrap();
    let (text, _) = export(&[("Kugel", &s)]);
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(
        (stats[0].faces, stats[0].edges, stats[0].vertices),
        (1, 1, 2)
    );
    assert!(matches!(surfaces(&m)[..], [Surf::Sphere { r, .. }] if *r == 6.0));
    let poles: Vec<DVec3> = m.vertices.values().copied().collect();
    for z in [-6.0, 6.0] {
        let pole = DVec3::new(0.0, 0.0, z);
        assert!(
            poles.iter().any(|p| p.distance(pole) < POLE_TOL),
            "Pol {pole} fehlt: {poles:?}"
        );
    }
    let face = &m.shells()[0].faces[0];
    assert_eq!(face.bounds.len(), 1);
    let b = &face.bounds[0];
    assert_eq!(b.edges.len(), 2);
    assert_eq!(b.edges[0].edge, b.edges[1].edge, "Meridian hin und zurück");
    assert_ne!(b.edges[0].forward, b.edges[1].forward);
}

#[test]
fn torus_rings_keep_both_radii() {
    let s = torus_ring();
    let (text, _) = export(&[("Ring", &s)]);
    let (m, _) = checked(&text, &[&s]);
    let tori: Vec<(f64, f64)> = surfaces(&m)
        .iter()
        .filter_map(|s| match s {
            Surf::Torus { major, minor, .. } => Some((*major, *minor)),
            _ => None,
        })
        .collect();
    assert_eq!(tori, vec![(8.0, 3.0)]);
}

#[test]
fn arcs_running_clockwise_are_edges_against_the_curve() {
    let s = notched();
    let (text, _) = export(&[("Bucht", &s)]);
    let (m, _) = checked(&text, &[&s]);
    let backwards = m
        .edges
        .values()
        .filter(|e| matches!(e.curve, Crv::Circle { .. }) && !e.same_sense)
        .count();
    assert_eq!(backwards, 2, "Bogen unten und oben laufen gegen den Kreis");
    let inner = m.shells()[0]
        .faces
        .iter()
        .filter(|f| matches!(f.surface, Surf::Cylinder { .. }))
        .count();
    assert_eq!(inner, 1);
}

#[test]
fn an_oblique_cut_gives_an_ellipse_with_the_long_axis_first() {
    let s = slanted();
    let (text, _) = export(&[("Schräg", &s)]);
    let (m, _) = checked(&text, &[&s]);
    let ellipses: Vec<(f64, f64)> = m
        .edges
        .values()
        .filter_map(|e| match e.curve {
            Crv::Ellipse { r1, r2, .. } => Some((r1, r2)),
            _ => None,
        })
        .collect();
    assert_eq!(ellipses.len(), 1, "{ellipses:?}");
    assert!((ellipses[0].0 - 5.0 / 0.5f64.cos()).abs() < AXIS_TOL);
    assert!((ellipses[0].1 - 5.0).abs() < AXIS_TOL);
}

/// Dieselbe Ellipse mit vertauschten Halbachsen (kurze Achse in x, Parameter um +π/2 versetzt).
fn swap_ellipses(s: &Solid) -> Solid {
    let mut out = s.clone();
    let ids: Vec<_> = out.edges.iter().map(|(id, _)| id).collect();
    for id in ids {
        let e = &mut out.edges[id];
        if let Curve::Ellipse {
            frame,
            major,
            minor,
        } = e.curve
        {
            e.curve = Curve::Ellipse {
                frame: Frame {
                    x: -frame.y,
                    y: frame.x,
                    ..frame
                },
                major: minor,
                minor: major,
            };
            e.t0 += PI / 2.0;
            e.t1 += PI / 2.0;
        }
    }
    out
}

/// Spindelring: Halbkreis mit Radius 3 um (2, 0) – der kleine Radius des Torus (3) ist größer
/// als der große (2).
fn spindle_ring() -> Solid {
    k().revolve(
        &ProfileLoop {
            start: DVec2::new(0.0, -3.0),
            segments: vec![
                Segment::Line {
                    to: DVec2::new(2.0, -3.0),
                },
                Segment::Arc {
                    center: DVec2::new(2.0, 0.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 3.0),
                },
            ],
        },
        &Frame::WORLD,
    )
    .unwrap()
}

#[test]
fn a_spindle_torus_is_refused_by_name_and_a_regular_ring_is_not() {
    let spindle = spindle_ring();
    assert!(
        spindle.faces.iter().any(
            |(_, f)| matches!(f.surface, Surface::Torus { major, minor, .. } if minor > major)
        ),
        "die Probe hat einen Spindeltorus"
    );
    let ring = torus_ring();
    let bodies = [
        StepBody {
            name: "Spindel".into(),
            solid: &spindle,
        },
        StepBody {
            name: "Ring".into(),
            solid: &ring,
        },
    ];
    let mut out = Vec::new();
    let report = write_step(&bodies, &options(), &mut out, &Progress::new()).unwrap();
    assert_eq!(report.bodies, 1);
    assert_eq!(report.rejected.len(), 1);
    assert!(
        matches!(&report.rejected[0], StepError::InvalidBody { name, reason }
            if name == "Spindel" && reason.contains("Torusfläche")),
        "{:?}",
        report.rejected
    );
    let text = String::from_utf8(out).unwrap();
    assert!(
        !text.contains("Spindel"),
        "für den Körper wird nichts geschrieben"
    );
    checked(&text, &[&ring]);
}

#[test]
fn the_model_refuses_a_torus_whose_small_radius_is_not_smaller() {
    let ring = torus_ring();
    let (text, _) = export(&[("Ring", &ring)]);
    assert!(text.contains(",8.,3.)"), "{text}");
    for swapped in [",3.,8.)", ",8.,8.)"] {
        let broken = text.replacen(",8.,3.)", swapped, 1);
        let e = Model::read(&broken).expect_err(swapped);
        assert!(e.contains("Spindeltorus"), "{swapped}: {e}");
    }
}

/// Walze mit Radius 8 und Höhe 10 mit einer runden Nut (Halbkreis, Radius 2) in der Mantelfläche:
/// der Torus ist hier hohl, die Fläche zeigt gegen seine natürliche Normale.
fn grooved_tube() -> Solid {
    k().revolve(
        &ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(8.0, 0.0),
                },
                Segment::Line {
                    to: DVec2::new(8.0, 3.0),
                },
                Segment::Arc {
                    center: DVec2::new(8.0, 5.0),
                    sweep: -PI,
                },
                Segment::Line {
                    to: DVec2::new(8.0, 10.0),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 10.0),
                },
            ],
        },
        &Frame::WORLD,
    )
    .unwrap()
}

#[test]
fn a_groove_is_a_hollow_torus_face_in_every_pose() {
    let body = grooved_tube();
    for (pose, m) in poses() {
        let s = moved(&body, m);
        let (text, _) = export(&[("Nut", &s)]);
        let (model, _) = std::panic::catch_unwind(|| checked(&text, &[&s]))
            .unwrap_or_else(|_| panic!("Nut, {pose}"));
        let hollow = model
            .shells()
            .iter()
            .flat_map(|sh| sh.faces.iter())
            .filter(|f| matches!(f.surface, Surf::Torus { .. }) && !f.same_sense)
            .count();
        assert_eq!(hollow, 1, "{pose}");
    }
}

/// Große Körper weit vom Ursprung (Koordinaten über 2e5 mm; mit fester Schranke 1e-11 rot):
/// die Rundungsfehler wachsen mit den Koordinaten, die Prüfung bleibt grün.
#[test]
fn large_bodies_far_from_the_origin_pass_all_checks() {
    let far = DMat4::from_translation(DVec3::new(BIG_OFFSET, 0.0, 0.0))
        * DMat4::from_rotation_x(0.9)
        * DMat4::from_rotation_z(0.4);
    for (name, body) in [
        (
            "Zylinder",
            k().make_cylinder(BIG_RADIUS, 3.0 * BIG_RADIUS).unwrap(),
        ),
        ("Kugel", k().make_sphere(BIG_RADIUS).unwrap()),
        (
            "Ring",
            moved(
                &torus_ring(),
                DMat4::from_scale(DVec3::splat(BIG_RADIUS / 8.0)),
            ),
        ),
    ] {
        let s = moved(&body, far);
        let (text, _) = export(&[(name, &s)]);
        std::panic::catch_unwind(|| checked(&text, &[&s])).unwrap_or_else(|_| panic!("{name}"));
    }
}

#[test]
fn a_vertex_moved_by_a_micrometre_is_still_caught() {
    let s = k().make_cylinder(4.0, 9.0).unwrap();
    let (text, _) = export(&[("Zylinder", &s)]);
    let mut m = Model::read(&text).unwrap();
    check_geometry(&m).expect("unverändert grün");
    let v = m.vertices.values_mut().next().unwrap();
    v.x += SHIFT;
    assert!(check_geometry(&m).is_err(), "Verschiebung um {SHIFT} mm");
}

#[test]
fn ellipses_with_the_short_axis_first_are_turned_around() {
    let s = swap_ellipses(&slanted());
    let swapped = s
        .edges
        .iter()
        .find_map(|(_, e)| match e.curve {
            Curve::Ellipse { major, minor, .. } => Some(minor > major),
            _ => None,
        })
        .unwrap();
    assert!(swapped, "die Probe hat die kurze Achse vorn");
    let (text, _) = export(&[("Schräg", &s)]);
    let (m, _) = checked(&text, &[&s]);
    for e in m.edges.values() {
        if let Crv::Ellipse { r1, r2, .. } = e.curve {
            assert!(r1 >= r2, "{r1} < {r2}");
        }
    }
}
