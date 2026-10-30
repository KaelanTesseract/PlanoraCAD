//! Endabschlüsse und Umbau der Nachbarflächen (Teilprojekt 8, Task 4).

use planora_kernel::blend::BlendError;
use planora_kernel::blend::analytic::*;
use planora_kernel::blend::build::*;
use planora_kernel::blend::section::ChamferSpec;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU, Tolerance};

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
            // Die Berührkurve auf Wand 1 reicht bis zur Bodenkante oder darüber hinaus.
            Err(BlendError::TooLarge { face }) => assert_eq!(face, "Wand 1"),
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

/// Kante mit den Endpunkten `p0` und `p1` (in beliebiger Richtung).
fn edge_at(s: &Solid, p0: DVec3, p1: DVec3) -> Id<Edge> {
    let near = |v: Id<Vertex>, p: DVec3| s.vertices[v].point.distance(p) < 1e-9;
    s.edges
        .iter()
        .find(|(_, e)| {
            (near(e.start, p0) && near(e.end, p1)) || (near(e.start, p1) && near(e.end, p0))
        })
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("keine Kante {p0} – {p1}"))
}

fn round_edge(s: &Solid, e: Id<Edge>, radius: f64) -> Result<Solid, BlendError> {
    let sides = edge_sides(s, e, None)?;
    let angle = edge_angle(s, e, &sides)?;
    let kind = SectionKind::Fillet { radius };
    let g = analytic_geom(s, e, &sides, angle.convex, kind, &tol())?.expect("analytische Kante");
    let geoms = vec![g];
    let plan = end_cuts(s, &geoms, &tol())?;
    assemble(s, &geoms, &plan, &["Rundung 1".into()], &tol())
}

/// Würfel 10 mit Löchern r = 0,5 durch die ganze Höhe an den Stellen `holes`.
fn holed_box(holes: &[DVec2]) -> Solid {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::splat(10.0)),
        holes: holes.iter().map(|c| ProfileLoop::circle(*c, 0.5)).collect(),
    };
    k().extrude(&profile, &Frame::WORLD, 10.0).unwrap()
}

fn assert_hole_refused(r: Result<Solid, BlendError>) {
    match r {
        Err(BlendError::Unsupported(m)) => assert!(m.contains("Bereich der Rundung"), "{m}"),
        other => panic!("{:?}", other.map(|s| s.faces.len())),
    }
}

#[test]
fn holes_in_the_rounded_area_are_refused_by_name() {
    // Regression (Review Task 4): vier Löcher je 1,5 mm vor einer Wand; die Rundung der
    // Deckelkante an x = 0 trägt das Loch bei (1,5; 5) ganz (r = 3) oder teilweise (r = 1,2 und
    // 1,5: die Berührkurve läuft durch das Loch) ab. Früher kam ein „gültiger“ falscher Körper.
    let s = holed_box(&[
        DVec2::new(1.5, 5.0),
        DVec2::new(8.5, 5.0),
        DVec2::new(5.0, 1.5),
        DVec2::new(5.0, 8.5),
    ]);
    let e = edge_at(&s, DVec3::new(0.0, 0.0, 10.0), DVec3::new(0.0, 10.0, 10.0));
    for r in [3.0, 1.2, 1.5] {
        assert_hole_refused(round_edge(&s, e, r));
    }
    // Berührkurve deutlich vor dem Loch (0,5 < 1): gültig, das Loch bleibt im Deckel.
    let ok = round_edge(&s, e, 0.5).unwrap();
    validate(&ok).unwrap();
    assert_eq!(ok.faces.len(), s.faces.len() + 1);
}

#[test]
fn a_hole_near_the_corner_of_an_end_face_is_refused_or_kept() {
    // Loch bei (1,5; 1,5) nahe der senkrechten Kante an (0, 0): Deckel und Boden sind die
    // Abschlussflächen. Bei r = 3 liegt das Loch innerhalb der Abschlusskurve (Abstand zur
    // Bogenmitte 2,12 + 0,5 < 3) und bleibt; bei r = 5 schneidet der Bogen das Loch, bei r = 6
    // liegt es ganz im abgetragenen Bereich.
    let s = holed_box(&[DVec2::new(1.5, 1.5)]);
    let e = edge_at(&s, DVec3::ZERO, DVec3::new(0.0, 0.0, 10.0));
    let ok = round_edge(&s, e, 3.0).unwrap();
    validate(&ok).unwrap();
    assert_eq!(ok.faces.len(), s.faces.len() + 1);
    for r in [5.0, 6.0] {
        assert_hole_refused(round_edge(&s, e, r));
    }
}

#[test]
fn a_pocket_reaching_into_the_rounded_area_is_refused() {
    // Tasche von unten (x 0,5…1,5, y 4…6, z bis 8,8), oben geschlossen: keine Fläche der Tasche
    // grenzt an die Kante, aber r = 3 trägt bis z ≈ 8,66 bei x = 0,5 ab und öffnet sie.
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let pocket = k()
        .transform(
            &k().make_box(DVec3::new(1.0, 2.0, 9.8)).unwrap(),
            &DMat4::from_translation(DVec3::new(0.5, 4.0, -1.0)),
        )
        .unwrap();
    let s = k().boolean(&cube, &pocket, BoolOp::Difference).unwrap();
    let e = edge_at(&s, DVec3::new(0.0, 0.0, 10.0), DVec3::new(0.0, 10.0, 10.0));
    match round_edge(&s, e, 3.0) {
        Err(BlendError::Unsupported(m)) => assert!(m.contains("Bereich der Rundung"), "{m}"),
        other => panic!("{:?}", other.map(|s| s.faces.len())),
    }
    // r = 1 bleibt über der Tasche (Bogen bei x = 0,5 erst ab z ≈ 9,87).
    let ok = round_edge(&s, e, 1.0).unwrap();
    validate(&ok).unwrap();
}

fn moved(s: Solid, by: DVec3) -> Solid {
    k().transform(&s, &DMat4::from_translation(by)).unwrap()
}

fn assert_cavity_refused(r: Result<Solid, BlendError>) {
    match r {
        Err(BlendError::Unsupported(m)) => assert_eq!(m, cavity()),
        other => panic!("{:?}", other.map(|s| s.faces.len())),
    }
}

fn round_or_chamfer(s: &Solid, e: Id<Edge>, kind: SectionKind) -> Result<Solid, BlendError> {
    let sides = edge_sides(s, e, None)?;
    let angle = edge_angle(s, e, &sides)?;
    let g = analytic_geom(s, e, &sides, angle.convex, kind, &tol())?.expect("analytische Kante");
    let geoms = vec![g];
    let plan = end_cuts(s, &geoms, &tol())?;
    assemble(s, &geoms, &plan, &["Rundung 1".into()], &tol())
}

#[test]
fn bodies_with_cavities_are_refused_by_name() {
    // Regression (Review Task 4, Runde 2): innere Schalen im oder quer zum abgetragenen Bereich
    // ergaben früher einen „gültigen“ falschen Körper.
    let cube = || k().make_box(DVec3::splat(10.0)).unwrap();
    let top_edge = |s: &Solid| edge_at(s, DVec3::new(0.0, 0.0, 10.0), DVec3::new(0.0, 10.0, 10.0));
    // Quaderförmiger Hohlraum x 0,2…0,6, y 4…6, z 9,4…9,8 unter der Deckelkante an x = 0.
    let void = moved(
        k().make_box(DVec3::new(0.4, 2.0, 0.4)).unwrap(),
        DVec3::new(0.2, 4.0, 9.4),
    );
    let s = k().boolean(&cube(), &void, BoolOp::Difference).unwrap();
    assert_eq!(s.shells.len(), 2);
    let e = top_edge(&s);
    assert_cavity_refused(round_or_chamfer(&s, e, SectionKind::Fillet { radius: 3.0 }));
    let chamfer = SectionKind::Chamfer(ChamferSpec::Distances { a: 3.0, b: 3.0 });
    assert_cavity_refused(round_or_chamfer(&s, e, chamfer));
    // Kugelhohlraum r = 0,4 bei (0,6; 0,6; 5) an der senkrechten Kante (0, 0).
    let ball = |x: f64, y: f64| moved(k().make_sphere(0.4).unwrap(), DVec3::new(x, y, 5.0));
    let vertical =
        |s: &Solid, x: f64, y: f64| edge_at(s, DVec3::new(x, y, 0.0), DVec3::new(x, y, 10.0));
    let s = k()
        .boolean(&cube(), &ball(0.6, 0.6), BoolOp::Difference)
        .unwrap();
    let fillet = SectionKind::Fillet { radius: 3.0 };
    assert_cavity_refused(round_or_chamfer(&s, vertical(&s, 0.0, 0.0), fillet));
    // Kugelhohlraum im Abstand 3,2 von der Achse der Rundung (schneidet die neue Fläche), an
    // allen vier senkrechten Kanten.
    let off = 3.0 - 3.2 / 2f64.sqrt();
    for (x, y) in [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)] {
        let (bx, by) = (
            if x == 0.0 { off } else { 10.0 - off },
            if y == 0.0 { off } else { 10.0 - off },
        );
        let s = k()
            .boolean(&cube(), &ball(bx, by), BoolOp::Difference)
            .unwrap();
        assert_cavity_refused(round_or_chamfer(&s, vertical(&s, x, y), fillet));
    }
    // Ohne Hohlraum unverändert gültig.
    let s = cube();
    assert!(round_or_chamfer(&s, top_edge(&s), fillet).is_ok());
    // Ohne Schalenliste gilt der Körper (wie bei `validate`) als eine Schale.
    let mut bare = cube();
    bare.shells.clear();
    let e = top_edge(&bare);
    validate(&round_or_chamfer(&bare, e, fillet).unwrap()).unwrap();
}
