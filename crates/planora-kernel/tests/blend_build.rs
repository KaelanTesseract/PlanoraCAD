//! Endabschlüsse und Umbau der Nachbarflächen (Teilprojekt 8, Task 4).

use planora_kernel::blend::BlendError;
use planora_kernel::blend::analytic::*;
use planora_kernel::blend::build::*;
use planora_kernel::blend::section::ChamferSpec;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
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

fn parts(
    s: &Solid,
    a: &str,
    b: &str,
    kind: SectionKind,
) -> Result<(Vec<EdgeGeom>, EndPlan), BlendError> {
    let e = edge_between(s, a, b);
    let sides = edge_sides(s, e, None)?;
    let angle = edge_angle(s, e, &sides)?;
    let g = analytic_geom(s, e, &sides, angle.convex, kind, &tol())?.expect("analytische Kante");
    let geoms = vec![g];
    let plan = end_cuts(s, &geoms, &tol())?;
    Ok((geoms, plan))
}

fn build(s: &Solid, a: &str, b: &str, kind: SectionKind, role: &str) -> Solid {
    let (geoms, plan) = parts(s, a, b, kind).unwrap();
    let r = assemble(s, &geoms, &plan, &[role.to_string()], &tol()).unwrap();
    validate(&r).unwrap();
    r
}

fn roles(s: &Solid) -> Vec<String> {
    let mut v: Vec<String> = s.faces.iter().map(|(id, _)| face_name(s, id)).collect();
    v.sort();
    v
}

fn loop_len(s: &Solid, role: &str) -> usize {
    let (_, f) = s
        .faces
        .iter()
        .find(|(id, _)| face_name(s, *id) == role)
        .unwrap();
    f.outer.half_edges.len()
}

#[test]
fn a_box_edge_fillet_assembles_into_a_valid_solid() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let r = build(
        &s,
        "Deckel",
        "Wand 1",
        SectionKind::Fillet { radius: 2.0 },
        "Rundung 1",
    );
    assert_eq!(
        (r.faces.len(), r.edges.len(), r.vertices.len()),
        (7, 15, 10)
    );
    assert_eq!(
        roles(&r),
        [
            "Boden",
            "Deckel",
            "Rundung 1",
            "Wand 1",
            "Wand 2",
            "Wand 3",
            "Wand 4"
        ]
    );
    // Die Seitenwände bekommen je eine Abschlusskante (Bogen) statt der Ecke.
    assert_eq!(loop_len(&r, "Wand 2"), 5);
    assert_eq!(loop_len(&r, "Wand 4"), 5);
    let fillet = r
        .faces
        .iter()
        .find(|(id, _)| face_name(&r, *id) == "Rundung 1")
        .unwrap()
        .1;
    assert_eq!(fillet.tag.as_ref().unwrap().origin, 0);
    assert!(fillet.patch.is_some());
}

#[test]
fn a_cylinder_top_fillet_closes_at_the_seam() {
    let s = k().make_cylinder(5.0, 8.0).unwrap();
    let r = build(
        &s,
        "Wand 1",
        "Deckel",
        SectionKind::Fillet { radius: 1.0 },
        "Rundung 1",
    );
    assert_eq!((r.faces.len(), r.edges.len(), r.vertices.len()), (4, 5, 3));
}

#[test]
fn a_concave_edge_extends_the_caps() {
    let profile = Profile {
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
    };
    let s = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let r = build(
        &s,
        "Wand 3",
        "Wand 4",
        SectionKind::Fillet { radius: 1.0 },
        "Rundung 1",
    );
    assert_eq!(loop_len(&r, "Deckel"), 7);
    assert_eq!(loop_len(&r, "Boden"), 7);
}

#[test]
fn chamfers_assemble_too() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let kind = SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 2.0 });
    let r = build(&s, "Deckel", "Wand 1", kind, "Fase 1");
    let (_, f) = r
        .faces
        .iter()
        .find(|(id, _)| face_name(&r, *id) == "Fase 1")
        .unwrap();
    assert!(matches!(f.surface, Surface::Plane { .. }));
    let c = k().make_cylinder(5.0, 8.0).unwrap();
    let kind = SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 1.0 });
    let r = build(&c, "Wand 1", "Deckel", kind, "Fase 1");
    let (_, f) = r
        .faces
        .iter()
        .find(|(id, _)| face_name(&r, *id) == "Fase 1")
        .unwrap();
    assert!(matches!(f.surface, Surface::Cone { .. }));
}

#[test]
fn an_end_face_that_cannot_be_cut_is_named() {
    // Langloch: der obere Bogen endet an tangential anschließenden ebenen Wänden; Torus und
    // diese Ebenen schneiden sich nicht (die Berührkurve erreicht die Wand nicht).
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(10.0, 2.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 4.0),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 3.0).unwrap();
    // Radius 0,5 statt 1: Bei 1 läge die Kugelbahn (Abstand 2 − 1 = 1) genau am Radius von der
    // Drehachse; das lehnt schon `analytic_geom` (Task 3) ab, bevor ein Abschluss gesucht wird.
    // Mit 0,5 reicht die Berührkurve auf dem Deckel nur bis y = 3,5 und erreicht die Wand y = 4 nicht.
    let r = parts(&s, "Deckel", "Wand 2", SectionKind::Fillet { radius: 0.5 });
    match r {
        Err(BlendError::Unsupported(m)) => {
            assert!(m.contains("nicht abschließen") && m.contains("Wand"), "{m}")
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
}

#[test]
fn parameters_unwrap_near_the_reference() {
    let c = Curve::Circle {
        frame: Frame::WORLD,
        radius: 2.0,
    };
    let p = DVec3::new(0.0, -2.0, 0.0);
    assert!((param_near(&c, p, 0.0) + PI / 2.0).abs() < 1e-12);
    assert!((param_near(&c, p, TAU) - 1.5 * PI).abs() < 1e-12);
    let l = Curve::Line {
        origin: DVec3::ZERO,
        dir: DVec3::X,
    };
    assert_eq!(param_near(&l, DVec3::new(3.0, 1.0, 0.0), 100.0), 3.0);
}

/// Probe über Körper, Kanten und Maße bis weit über die Flächen hinaus: nie Panik; ein `Ok`
/// besteht `validate` und hat nur endliche Ecken, sonst kommt eine benannte Meldung.
#[test]
fn oversized_and_degenerate_sizes_never_give_a_wrong_body() {
    let slot = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(10.0, 2.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 4.0),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let ell = Profile {
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
    };
    let bodies = [
        k().make_box(DVec3::splat(10.0)).unwrap(),
        k().make_box(DVec3::new(10.0, 10.0, 1.0)).unwrap(),
        k().make_cylinder(5.0, 8.0).unwrap(),
        k().make_cone(4.0, 6.0).unwrap(),
        k().extrude(&ell, &Frame::WORLD, 5.0).unwrap(),
        k().extrude(&slot, &Frame::WORLD, 3.0).unwrap(),
    ];
    let sizes = [1e-5, 0.5, 2.0, 4.0, 4.999_999, 5.0, 6.0, 10.0, 12.0, 1e3];
    let (mut ok, mut refused, mut too_large) = (0, 0, 0);
    for s in &bodies {
        for (e, _) in s.edges.iter() {
            let Ok(sides) = edge_sides(s, e, None) else {
                continue;
            };
            let Ok(angle) = edge_angle(s, e, &sides) else {
                continue;
            };
            for &d in &sizes {
                let kinds = [
                    SectionKind::Fillet { radius: d },
                    SectionKind::Chamfer(ChamferSpec::Distances { a: d, b: d }),
                    SectionKind::Chamfer(ChamferSpec::Distances { a: d, b: 0.5 * d }),
                ];
                for kind in kinds {
                    let Ok(Some(g)) = analytic_geom(s, e, &sides, angle.convex, kind, &tol())
                    else {
                        continue;
                    };
                    let geoms = vec![g];
                    let r = end_cuts(s, &geoms, &tol())
                        .and_then(|plan| assemble(s, &geoms, &plan, &["Rundung 1".into()], &tol()));
                    match r {
                        Ok(r) => {
                            validate(&r).unwrap();
                            assert!(r.vertices.iter().all(|(_, v)| v.point.is_finite()));
                            ok += 1;
                        }
                        Err(err) => {
                            let m = err.to_string();
                            assert!(!m.is_empty() && !m.contains("NaN"), "{m}");
                            too_large += matches!(err, BlendError::TooLarge { .. }) as usize;
                            refused += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(
        ok > 0 && refused > 0 && too_large > 0,
        "{ok} {refused} {too_large}"
    );
}

#[test]
fn a_radius_beyond_the_face_is_too_large_by_name() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    for r in [10.0, 12.0] {
        let (geoms, plan) =
            parts(&s, "Deckel", "Wand 1", SectionKind::Fillet { radius: r }).unwrap();
        match assemble(&s, &geoms, &plan, &["Rundung 1".into()], &tol()) {
            Err(BlendError::TooLarge { face }) => assert!(
                face == "Wand 2" || face == "Wand 4" || face == "Deckel" || face == "Wand 1",
                "{face}"
            ),
            other => panic!("{:?}", other.map(|_| ())),
        }
    }
    // Abschlüsse und Rollen müssen zu den Kanten passen (sonst benannt, keine Panik).
    let (geoms, plan) = parts(&s, "Deckel", "Wand 1", SectionKind::Fillet { radius: 1.0 }).unwrap();
    assert!(matches!(
        assemble(&s, &geoms, &plan, &[], &tol()),
        Err(BlendError::ResultInvalid(_))
    ));
    assert!(matches!(
        assemble(
            &s,
            &geoms,
            &EndPlan::default(),
            &["Rundung 1".into()],
            &tol()
        ),
        Err(BlendError::ResultInvalid(_))
    ));
}
