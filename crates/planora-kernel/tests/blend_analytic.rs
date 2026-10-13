//! Analytische Rundungs- und Fasenflächen (Teilprojekt 8, Task 3).

use planora_kernel::blend::BlendError;
use planora_kernel::blend::analytic::*;
use planora_kernel::blend::section::ChamferSpec;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, PI, TAU, Tolerance};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn tol() -> Tolerance {
    Tolerance::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

fn geom(s: &Solid, a: &str, b: &str, kind: SectionKind) -> Result<Option<EdgeGeom>, BlendError> {
    let e = edge_between(s, a, b);
    let first = edge_uses(s, e)
        .into_iter()
        .find(|u| face_name(s, u.face) == a)
        .map(|u| u.face);
    let sides = edge_sides(s, e, first).unwrap();
    let angle = edge_angle(s, e, &sides).unwrap();
    analytic_geom(s, e, &sides, angle.convex, kind, &tol())
}

/// Querschnittsfläche zwischen Kante (Innenwinkel 90°) und Viertelkreis: (1 − π/4)·r².
fn spandrel90(r: f64) -> f64 {
    (1.0 - PI / 4.0) * r * r
}

/// Abstand des Schwerpunkts dieser Fläche von jeder der beiden Seiten (rechter Winkel).
fn spandrel90_offset(r: f64) -> f64 {
    r * (5.0 / 6.0 - PI / 4.0) / (1.0 - PI / 4.0)
}

#[test]
fn a_box_edge_fillet_is_a_cylinder_along_the_edge() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let g = geom(&s, "Deckel", "Wand 1", SectionKind::Fillet { radius: 2.0 })
        .unwrap()
        .unwrap();
    match &g.surface {
        Surface::Cylinder { frame, radius } => {
            assert!((radius - 2.0).abs() < 1e-12);
            assert!(frame.z.cross(DVec3::X).length() < 1e-12);
            assert!((frame.origin.y - 2.0).abs() < 1e-12 && (frame.origin.z - 8.0).abs() < 1e-12);
        }
        other => panic!("{other:?}"),
    }
    assert!(g.same_sense && g.convex && !g.closed && g.analytic);
    // Berührkurven liegen auf ihren Flächen und laufen über die ganze Kante.
    let (t0, t1) = g.range;
    assert!(((t1 - t0).abs() - 10.0).abs() < 1e-12);
    for f in [0.0, 0.3, 1.0] {
        let t = t0 + (t1 - t0) * f;
        let (pa, pb) = (g.contact_a.point(t), g.contact_b.point(t));
        assert!(
            (pa.z - 10.0).abs() < 1e-12 && (pa.y - 2.0).abs() < 1e-12,
            "{pa:?}"
        );
        assert!(pb.y.abs() < 1e-12 && (pb.z - 8.0).abs() < 1e-12, "{pb:?}");
    }
    assert!(g.section_start.start().distance(g.contact_a.point(t0)) < 1e-12);
    assert!(g.section_start.end().distance(g.contact_b.point(t0)) < 1e-12);
    assert!(g.section_end.start().distance(g.contact_a.point(t1)) < 1e-12);
    match g.volume {
        VolumeModel::Prism { area } => {
            assert!((area - spandrel90(2.0)).abs() < 1e-3 * spandrel90(2.0))
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_concave_fillet_faces_the_other_way() {
    let s = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::polygon(&[
                    DVec2::new(0.0, 0.0),
                    DVec2::new(10.0, 0.0),
                    DVec2::new(10.0, 4.0),
                    DVec2::new(4.0, 4.0),
                    DVec2::new(4.0, 10.0),
                    DVec2::new(0.0, 10.0),
                ])
                .unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            5.0,
        )
        .unwrap();
    let g = geom(&s, "Wand 3", "Wand 4", SectionKind::Fillet { radius: 1.0 })
        .unwrap()
        .unwrap();
    assert!(!g.convex && !g.same_sense);
    match &g.surface {
        Surface::Cylinder { frame, .. } => {
            assert!(frame.origin.truncate().distance(DVec2::new(5.0, 5.0)) < 1e-12)
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_cylinder_top_edge_gives_a_closed_torus_and_a_cone_chamfer() {
    let s = k().make_cylinder(5.0, 8.0).unwrap();
    let g = geom(&s, "Wand 1", "Deckel", SectionKind::Fillet { radius: 1.0 })
        .unwrap()
        .unwrap();
    match &g.surface {
        Surface::Torus {
            major,
            minor,
            frame,
        } => {
            assert!((major - 4.0).abs() < 1e-10 && (minor - 1.0).abs() < 1e-12);
            assert!((frame.origin.z - 7.0).abs() < 1e-10);
        }
        other => panic!("{other:?}"),
    }
    assert!(g.closed);
    assert!(((g.range.1 - g.range.0).abs() - TAU).abs() < 1e-12);
    assert!(g.section_start.start().distance(g.section_end.start()) < 1e-10);
    match g.volume {
        VolumeModel::Pappus {
            area,
            centroid_radius,
        } => {
            assert!((area - spandrel90(1.0)).abs() < 1e-3 * spandrel90(1.0));
            assert!((centroid_radius - (5.0 - spandrel90_offset(1.0))).abs() < 1e-3);
        }
        other => panic!("{other:?}"),
    }
    let c = geom(
        &s,
        "Wand 1",
        "Deckel",
        SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 1.0 }),
    )
    .unwrap()
    .unwrap();
    assert!(matches!(c.surface, Surface::Cone { .. }), "{:?}", c.surface);
    match c.volume {
        VolumeModel::Pappus {
            area,
            centroid_radius,
        } => {
            assert!((area - 0.5).abs() < 1e-12);
            assert!((centroid_radius - (5.0 - 1.0 / 3.0)).abs() < 1e-12);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn chamfers_on_a_box_edge_honour_face_one() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let two = geom(
        &s,
        "Deckel",
        "Wand 1",
        SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 2.5 }),
    )
    .unwrap()
    .unwrap();
    assert!(matches!(two.surface, Surface::Plane { .. }));
    let p = two.contact_a.point(two.range.0);
    assert!(
        (p.y - 1.0).abs() < 1e-12 && (p.z - 10.0).abs() < 1e-12,
        "Fläche 1 = Deckel: {p:?}"
    );
    let q = two.contact_b.point(two.range.0);
    assert!((q.z - 7.5).abs() < 1e-12, "{q:?}");
    let angle = geom(
        &s,
        "Deckel",
        "Wand 1",
        SectionKind::Chamfer(ChamferSpec::DistanceAngle {
            distance: 1.0,
            angle: 30f64.to_radians(),
        }),
    )
    .unwrap()
    .unwrap();
    let q = angle.contact_b.point(angle.range.0);
    assert!(
        (10.0 - q.z - 30f64.to_radians().tan()).abs() < 1e-10,
        "{q:?}"
    );
}

#[test]
fn hemisphere_and_cone_base_edges_are_tori() {
    let r_big = 5.0;
    let hemi = k()
        .revolve(
            &ProfileLoop {
                start: DVec2::ZERO,
                segments: vec![
                    Segment::Line {
                        to: DVec2::new(r_big, 0.0),
                    },
                    Segment::Arc {
                        center: DVec2::ZERO,
                        sweep: PI / 2.0,
                    },
                ],
            },
            &Frame::WORLD,
        )
        .unwrap();
    let g = geom(
        &hemi,
        "Fläche 1",
        "Fläche 2",
        SectionKind::Fillet { radius: 1.0 },
    )
    .unwrap()
    .unwrap();
    match &g.surface {
        Surface::Torus { major, .. } => assert!((major - 15f64.sqrt()).abs() < 1e-9, "{major}"),
        other => panic!("{other:?}"),
    }
    let cone = k().make_cone(4.0, 6.0).unwrap();
    let g = geom(
        &cone,
        "Fläche 1",
        "Fläche 2",
        SectionKind::Fillet { radius: 0.5 },
    )
    .unwrap()
    .unwrap();
    assert!(matches!(g.surface, Surface::Torus { .. }));
    // Berührpunkt auf dem Kegelmantel liegt auf dem Mantel.
    let fb = &cone.faces[g.sides.b.face];
    let p = g.contact_b.point(g.range.0);
    assert!(fb.surface.implicit(p).abs() < 1e-9);
}

#[test]
fn curvature_and_axis_limits_are_named() {
    let thin = k().make_cylinder(1.0, 5.0).unwrap();
    let r = geom(
        &thin,
        "Wand 1",
        "Deckel",
        SectionKind::Fillet { radius: 1.5 },
    );
    assert!(
        matches!(r, Err(BlendError::CurvatureTooSmall { .. })),
        "{r:?}"
    );
    // Zylinder r = 2, Rundung 1,5: die Kugelbahn (ρ = 0,5) läge näher an der Achse als der Radius.
    let mid = k().make_cylinder(2.0, 5.0).unwrap();
    let r = geom(
        &mid,
        "Wand 1",
        "Deckel",
        SectionKind::Fillet { radius: 1.5 },
    );
    assert!(
        matches!(&r, Err(BlendError::Unsupported(m)) if m.contains("Drehachse")),
        "{r:?}"
    );
}

#[test]
fn freeform_edges_are_not_analytic() {
    let ellipse = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(0.0, 3.0, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    let s = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::nurbs_loop(ellipse).unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            4.0,
        )
        .unwrap();
    let g = geom(&s, "Wand 1", "Deckel", SectionKind::Fillet { radius: 0.5 }).unwrap();
    assert!(g.is_none());
}

/// Alle Zahlen einer gelieferten Geometrie sind endlich.
fn assert_finite(g: &EdgeGeom) {
    let (t0, t1) = g.range;
    for f in [0.0, 0.5, 1.0] {
        let t = t0 + (t1 - t0) * f;
        assert!(
            g.contact_a.point(t).is_finite() && g.contact_b.point(t).is_finite(),
            "{g:?}"
        );
        assert!(
            g.section_start.at(f).is_finite() && g.section_end.at(f).is_finite(),
            "{g:?}"
        );
    }
    assert!(g.reach.is_finite(), "{g:?}");
    let v = match g.volume {
        VolumeModel::Prism { area } => area,
        VolumeModel::Pappus {
            area,
            centroid_radius,
        } => area + centroid_radius,
        VolumeModel::Total { volume } => volume,
    };
    assert!(v.is_finite(), "{g:?}");
}

#[test]
fn hostile_sizes_never_panic_or_give_nan() {
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let cyl = k().make_cylinder(5.0, 8.0).unwrap();
    let bad = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -1.0,
        1e-300,
        1e300,
    ];
    for s in [&cube, &cyl] {
        let (a, b) = if std::ptr::eq(s, &cube) {
            ("Deckel", "Wand 1")
        } else {
            ("Wand 1", "Deckel")
        };
        for &x in &bad {
            let kinds = [
                SectionKind::Fillet { radius: x },
                SectionKind::Chamfer(ChamferSpec::Distances { a: x, b: 1.0 }),
                SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: x }),
                SectionKind::Chamfer(ChamferSpec::DistanceAngle {
                    distance: x,
                    angle: 0.5,
                }),
                SectionKind::Chamfer(ChamferSpec::DistanceAngle {
                    distance: 1.0,
                    angle: x,
                }),
            ];
            for kind in kinds {
                match geom(s, a, b, kind) {
                    Ok(Some(g)) => assert_finite(&g),
                    Ok(None) => panic!("analytische Kante erwartet ({kind:?})"),
                    Err(_) => {}
                }
            }
        }
    }
}

#[test]
fn polygon_area_and_centroid_of_a_unit_square() {
    let pts = [DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0), DVec3::Y];
    let (a, c) = polygon_area_centroid(&pts);
    assert!((a - 1.0).abs() < 1e-15);
    assert!(c.distance(DVec3::new(0.5, 0.5, 0.0)) < 1e-15);
}
