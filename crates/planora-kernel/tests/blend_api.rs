//! Kernel-Operation `blend` (Teilprojekt 8, Task 5).

use planora_kernel::blend::build::CAVITY;
use planora_kernel::blend::topo::{edge_angle, edge_sides, edge_uses, face_name};
use planora_kernel::blend::{MIN_CHAMFER_ANGLE, min_blend_size};
use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU, Tolerance};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
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

fn req(edges: &[Id<Edge>], kind: BlendKind) -> BlendRequest {
    BlendRequest {
        edges: edges
            .iter()
            .map(|&edge| BlendEdge {
                edge,
                first_face: None,
            })
            .collect(),
        kind,
        tangent_chain: false,
    }
}

#[test]
fn a_fillet_through_the_kernel_trait() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = k()
        .blend(&s, &req(&[e], BlendKind::Fillet { radius: 2.0 }))
        .unwrap();
    k().validate(&b.solid).unwrap();
    assert_eq!(b.edges, vec![e]);
    let want = -(1.0 - PI / 4.0) * 4.0 * 10.0;
    assert!(
        (b.volume_change - want).abs() < 1e-3 * want.abs(),
        "{}",
        b.volume_change
    );
    let tags: Vec<_> = b
        .solid
        .faces
        .iter()
        .filter_map(|(_, f)| f.tag.clone())
        .collect();
    assert!(tags.iter().any(|t| t.role == "Rundung 1" && t.origin == 0));
    let c = k()
        .blend(
            &s,
            &req(
                &[e],
                BlendKind::ChamferAngle {
                    distance: 1.0,
                    angle: PI / 4.0,
                },
            ),
        )
        .unwrap();
    assert!(
        c.solid
            .faces
            .iter()
            .any(|(_, f)| f.tag.as_ref().unwrap().role == "Fase 1")
    );
    assert!(
        (c.volume_change + 0.5 * 10.0).abs() < 1e-9,
        "{}",
        c.volume_change
    );
}

#[test]
fn hostile_requests_never_panic() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let bad_kinds = [
        BlendKind::Fillet { radius: f64::NAN },
        BlendKind::Fillet {
            radius: f64::INFINITY,
        },
        BlendKind::Fillet { radius: -1.0 },
        BlendKind::Fillet { radius: 0.0 },
        BlendKind::Fillet { radius: 1e300 },
        BlendKind::ChamferEqual {
            distance: f64::NEG_INFINITY,
        },
        BlendKind::ChamferTwo {
            distance1: 1.0,
            distance2: -2.0,
        },
        BlendKind::ChamferAngle {
            distance: 1.0,
            angle: 0.0,
        },
        BlendKind::ChamferAngle {
            distance: 1.0,
            angle: PI,
        },
        BlendKind::ChamferAngle {
            distance: 1.0,
            angle: f64::NAN,
        },
        BlendKind::VariableFillet {
            points: vec![(0.0, 1.0)],
        },
        BlendKind::VariableFillet {
            points: vec![(0.5, 1.0), (0.2, 1.0)],
        },
        BlendKind::VariableFillet {
            points: vec![(0.0, 1.0), (1.5, 1.0)],
        },
        BlendKind::VariableFillet {
            points: vec![(0.0, 1.0), (1.0, -1.0)],
        },
        BlendKind::VariableFillet {
            points: (0..65).map(|i| (i as f64 / 64.0, 1.0)).collect(),
        },
    ];
    for kind in bad_kinds {
        let r = k().blend(&s, &req(&[e], kind.clone()));
        assert!(
            matches!(r, Err(BlendError::InvalidParameter(_))),
            "{kind:?}: {:?}",
            r.map(|_| ())
        );
    }
    assert_eq!(
        k().blend(&s, &req(&[], BlendKind::Fillet { radius: 1.0 }))
            .map(|_| ()),
        Err(BlendError::NoEdges)
    );
    let many: Vec<Id<Edge>> = (0..257).map(|_| e).collect();
    assert!(matches!(
        k().blend(&s, &req(&many, BlendKind::Fillet { radius: 1.0 })),
        Err(BlendError::InvalidParameter(_))
    ));
    assert_eq!(
        k().blend(
            &s,
            &req(&[Id::from_index(500)], BlendKind::Fillet { radius: 1.0 })
        )
        .map(|_| ()),
        Err(BlendError::EdgeNotFound)
    );
    // Doppelte Kanten zählen einmal.
    let twice = k()
        .blend(&s, &req(&[e, e], BlendKind::Fillet { radius: 1.0 }))
        .unwrap();
    assert_eq!(twice.edges, vec![e]);
}

#[test]
fn too_large_sizes_are_refused_by_name() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    match k().blend(&s, &req(&[e], BlendKind::Fillet { radius: 11.0 })) {
        Err(BlendError::TooLarge { face }) => {
            assert!(face == "Deckel" || face == "Wand 1", "{face}")
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
    match k().blend(&s, &req(&[e], BlendKind::ChamferEqual { distance: 12.0 })) {
        Err(BlendError::TooLarge { .. }) => {}
        other => panic!("{:?}", other.map(|_| ())),
    }
    // Flache Platte: die Rundung bräuchte mehr Höhe, als die Wand hat.
    let flat = k().make_box(DVec3::new(10.0, 10.0, 1.0)).unwrap();
    let e = edge_between(&flat, "Deckel", "Wand 1");
    match k().blend(&flat, &req(&[e], BlendKind::Fillet { radius: 2.0 })) {
        Err(BlendError::TooLarge { face }) => assert_eq!(face, "Wand 1"),
        other => panic!("{:?}", other.map(|_| ())),
    }
    let msg = BlendError::TooLarge {
        face: "Wand 1".into(),
    }
    .to_string();
    assert!(
        msg.contains("„Wand 1“") && msg.contains("kleineres Maß"),
        "{msg}"
    );
}

#[test]
fn seams_tangents_and_curvature_are_named_through_the_api() {
    let c = k().make_cylinder(1.0, 5.0).unwrap();
    let seam = c
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, Curve::Line { .. }))
        .unwrap()
        .0;
    assert_eq!(
        k().blend(&c, &req(&[seam], BlendKind::Fillet { radius: 0.2 }))
            .map(|_| ()),
        Err(BlendError::SeamEdge)
    );
    let top = edge_between(&c, "Wand 1", "Deckel");
    assert!(matches!(
        k().blend(&c, &req(&[top], BlendKind::Fillet { radius: 1.5 })),
        Err(BlendError::CurvatureTooSmall { .. })
    ));
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
    let slot = k().extrude(&profile, &Frame::WORLD, 3.0).unwrap();
    let e = edge_between(&slot, "Wand 1", "Wand 2");
    assert!(matches!(
        k().blend(&slot, &req(&[e], BlendKind::Fillet { radius: 0.5 })),
        Err(BlendError::NearlyTangent { .. })
    ));
}

#[test]
fn an_invalid_body_is_reported() {
    let mut s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    // R-1: eine leere Schalenliste gilt als eine Schale; eine Fläche ohne Schale ist ungültig.
    s.shells[0].faces.pop();
    assert!(matches!(
        k().blend(&s, &req(&[e], BlendKind::Fillet { radius: 1.0 })),
        Err(BlendError::InvalidInput(_))
    ));
}

// ---- Zusätzliche Tests (Task 5, über den Plan hinaus) ----

/// Querschnittsfläche und Schwerpunktabstand (von der Kante aus, je Achse) des Zwickels zwischen
/// rechtem Winkel und Viertelkreis mit Radius `r`.
fn spandrel(r: f64) -> (f64, f64) {
    (
        (1.0 - PI / 4.0) * r * r,
        r * (10.0 - 3.0 * PI) / (12.0 - 3.0 * PI),
    )
}

/// Konkave Kante zwischen „Wand 3“ und „Wand 4“ eines L-Profils, Höhe 5.
fn l_profile() -> Solid {
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
    k().extrude(&profile, &Frame::WORLD, 5.0).unwrap()
}

#[test]
fn volume_estimate_matches_closed_formulas() {
    // Die Rundung wird für die Schätzung als Polygon mit 64 Bogenstücken gerechnet
    // (`SECTION_ARC_SAMPLES`), daher 1e-3 relativ; Fasen sind exakt.
    const ARC_REL: f64 = 1e-3;
    const EXACT: f64 = 1e-9;
    let close = |got: f64, want: f64, rel: f64| {
        assert!(
            (got - want).abs() <= rel * want.abs(),
            "geschätzt {got}, erwartet {want}"
        );
    };
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&cube, "Deckel", "Wand 1");
    // Prisma, gerade Kante: Zwickel × Länge.
    let (a, _) = spandrel(3.0);
    let b = k()
        .blend(&cube, &req(&[e], BlendKind::Fillet { radius: 3.0 }))
        .unwrap();
    close(b.volume_change, -a * 10.0, ARC_REL);
    // Fase mit zwei Abständen: Dreieck ½·1·2 × 10.
    let b = k()
        .blend(
            &cube,
            &req(
                &[e],
                BlendKind::ChamferTwo {
                    distance1: 1.0,
                    distance2: 2.0,
                },
            ),
        )
        .unwrap();
    close(b.volume_change, -10.0, EXACT);
    // Winkelfase 30° zu Fläche 1 mit Abstand 2: Gegenkathete 2·tan 30°.
    let b = k()
        .blend(
            &cube,
            &req(
                &[e],
                BlendKind::ChamferAngle {
                    distance: 2.0,
                    angle: PI / 6.0,
                },
            ),
        )
        .unwrap();
    close(
        b.volume_change,
        -0.5 * 2.0 * 2.0 * (PI / 6.0).tan() * 10.0,
        EXACT,
    );
    // Konkave Kante: Material kommt hinzu (positiv).
    let l = l_profile();
    let e = edge_between(&l, "Wand 3", "Wand 4");
    let sides = edge_sides(&l, e, None).unwrap();
    assert!(!edge_angle(&l, e, &sides).unwrap().convex);
    let (a, _) = spandrel(1.0);
    let b = k()
        .blend(&l, &req(&[e], BlendKind::Fillet { radius: 1.0 }))
        .unwrap();
    close(b.volume_change, a * 5.0, ARC_REL);
    // Pappus, Kreiskante (Zylinder R = 5, Deckelkante): Zwickel × 2π × Schwerpunktabstand.
    let (big_r, r) = (5.0, 1.0);
    let cyl = k().make_cylinder(big_r, 8.0).unwrap();
    let e = edge_between(&cyl, "Wand 1", "Deckel");
    let (a, xbar) = spandrel(r);
    let b = k()
        .blend(&cyl, &req(&[e], BlendKind::Fillet { radius: r }))
        .unwrap();
    close(b.volume_change, -a * TAU * (big_r - xbar), ARC_REL);
    // Fase am Kreis: Dreieck ½·d², Schwerpunkt bei R − d/3.
    let d = 1.0;
    let b = k()
        .blend(&cyl, &req(&[e], BlendKind::ChamferEqual { distance: d }))
        .unwrap();
    close(
        b.volume_change,
        -0.5 * d * d * TAU * (big_r - d / 3.0),
        EXACT,
    );
}

#[test]
fn a_contact_curve_leaving_a_narrow_face_names_that_face() {
    // L-Profil: der Deckel ist an der Kante zu „Wand 1“ (y = 0) für x > 4 nur 4 mm breit. Eine
    // Berührkurve bei y = 4,5 verlässt ihn mitten auf der Kante (`check_contacts`); ohne diese
    // Prüfung nennt erst der Abschluss eine andere Fläche („Wand 2“).
    let l = l_profile();
    let e = edge_between(&l, "Wand 1", "Deckel");
    for kind in [
        BlendKind::Fillet { radius: 4.5 },
        BlendKind::ChamferEqual { distance: 4.5 },
    ] {
        match k().blend(&l, &req(&[e], kind.clone())) {
            Err(BlendError::TooLarge { face }) => assert_eq!(face, "Deckel", "{kind:?}"),
            other => panic!("{kind:?}: {:?}", other.map(|_| ())),
        }
    }
    // Knapp darunter passt es.
    let b = k()
        .blend(&l, &req(&[e], BlendKind::Fillet { radius: 3.5 }))
        .unwrap();
    k().validate(&b.solid).unwrap();
}

#[test]
fn sizes_below_the_minimum_are_named_too_small() {
    let tol = Tolerance::default();
    let min = min_blend_size(&tol);
    assert!(min > 10.0 * tol.linear, "Abstand zur Verschweißweite");
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let cyl = k().make_cylinder(5.0, 8.0).unwrap();
    let edges = [
        (&cube, edge_between(&cube, "Deckel", "Wand 1")),
        (&cyl, edge_between(&cyl, "Wand 1", "Deckel")),
    ];
    let below = 0.5 * min;
    let too_small = [
        BlendKind::Fillet { radius: below },
        BlendKind::ChamferEqual { distance: below },
        BlendKind::ChamferTwo {
            distance1: 1.0,
            distance2: below,
        },
        BlendKind::ChamferAngle {
            distance: below,
            angle: PI / 4.0,
        },
        BlendKind::Fillet {
            radius: f64::MIN_POSITIVE,
        },
    ];
    for (s, e) in edges {
        for kind in &too_small {
            match k().blend(s, &req(&[e], kind.clone())) {
                Err(BlendError::InvalidParameter(m)) => {
                    assert!(m.contains("ist zu klein"), "{kind:?}: {m}")
                }
                other => panic!("{kind:?}: {:?}", other.map(|_| ())),
            }
        }
        // Genau das Mindestmaß ergibt einen gültigen Körper.
        let at_min = [
            BlendKind::Fillet { radius: min },
            BlendKind::ChamferEqual { distance: min },
            BlendKind::ChamferTwo {
                distance1: min,
                distance2: 2.0 * min,
            },
            BlendKind::ChamferAngle {
                distance: min,
                angle: PI / 4.0,
            },
        ];
        for kind in at_min {
            let b = k()
                .blend(s, &req(&[e], kind.clone()))
                .unwrap_or_else(|err| panic!("{kind:?}: {err}"));
            k().validate(&b.solid).unwrap();
            assert!(b.volume_change < 0.0 && b.volume_change.is_finite());
        }
    }
    // Winkelgrenzen 1° und 179° (P4) gehören dazu, knapp darunter bzw. darüber nicht.
    let (s, e) = (&cube, edge_between(&cube, "Deckel", "Wand 1"));
    for angle in [
        MIN_CHAMFER_ANGLE * (1.0 - 1e-9),
        PI - MIN_CHAMFER_ANGLE * (1.0 - 1e-9),
        -PI / 4.0,
        f64::INFINITY,
    ] {
        let r = k().blend(
            s,
            &req(
                &[e],
                BlendKind::ChamferAngle {
                    distance: 0.1,
                    angle,
                },
            ),
        );
        match r {
            Err(BlendError::InvalidParameter(m)) => assert!(m.contains("Winkel"), "{m}"),
            other => panic!("{angle}: {:?}", other.map(|_| ())),
        }
    }
    let b = k()
        .blend(
            s,
            &req(
                &[e],
                BlendKind::ChamferAngle {
                    distance: 0.1,
                    angle: MIN_CHAMFER_ANGLE,
                },
            ),
        )
        .unwrap();
    k().validate(&b.solid).unwrap();
}

#[test]
fn stale_references_and_cavities_are_named_before_any_geometry() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let fillet = BlendKind::Fillet { radius: 1.0 };
    // Fläche 1, die nicht an die Kante grenzt, oder eine fremde Flächen-Id.
    let boden = s
        .faces
        .iter()
        .find(|(id, _)| face_name(&s, *id) == "Boden")
        .unwrap()
        .0;
    for first_face in [Some(boden), Some(Id::from_index(999))] {
        let r = BlendRequest {
            edges: vec![BlendEdge {
                edge: e,
                first_face,
            }],
            kind: fillet.clone(),
            tangent_chain: false,
        };
        assert!(
            matches!(k().blend(&s, &r), Err(BlendError::InvalidInput(_))),
            "{first_face:?}"
        );
    }
    // Erste Fläche gesetzt: Abstand 1 liegt auf ihr, Abstand 2 auf der Wand (Höhe der Fase in z).
    let face_id = |name: &str| {
        s.faces
            .iter()
            .find(|(id, _)| face_name(&s, *id) == name)
            .unwrap()
            .0
    };
    let chamfer_height = |b: &Blended| {
        let (_, f) = b
            .solid
            .faces
            .iter()
            .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == "Fase 1"))
            .unwrap();
        let z: Vec<f64> = f
            .outer
            .half_edges
            .iter()
            .flat_map(|he| {
                let ed = &b.solid.edges[he.edge];
                [ed.start, ed.end].map(|v| b.solid.vertices[v].point.z)
            })
            .collect();
        z.iter().cloned().fold(f64::MIN, f64::max) - z.iter().cloned().fold(f64::MAX, f64::min)
    };
    let two = |first: Option<Id<Face>>| BlendRequest {
        edges: vec![BlendEdge {
            edge: e,
            first_face: first,
        }],
        kind: BlendKind::ChamferTwo {
            distance1: 1.0,
            distance2: 3.0,
        },
        tangent_chain: false,
    };
    let on_top = k().blend(&s, &two(Some(face_id("Deckel")))).unwrap();
    assert!((chamfer_height(&on_top) - 3.0).abs() < 1e-9);
    assert!((on_top.volume_change + 15.0).abs() < 1e-9);
    let on_wall = k().blend(&s, &two(Some(face_id("Wand 1")))).unwrap();
    assert!((chamfer_height(&on_wall) - 1.0).abs() < 1e-9);
    // Doppelte Nennung: die erste gilt (auch ihre erste Fläche).
    let mut twice = two(Some(face_id("Wand 1")));
    twice.edges.push(BlendEdge {
        edge: e,
        first_face: Some(face_id("Deckel")),
    });
    let b = k().blend(&s, &twice).unwrap();
    assert_eq!(b.edges, vec![e]);
    assert!((chamfer_height(&b) - 1.0).abs() < 1e-9);
    // Hohlraum: benannt, auch wenn er weit weg von der Kante liegt.
    let void = k()
        .transform(
            &k().make_box(DVec3::splat(1.0)).unwrap(),
            &DMat4::from_translation(DVec3::new(4.5, 4.5, 4.5)),
        )
        .unwrap();
    let hollow = k().boolean(&s, &void, BoolOp::Difference).unwrap();
    let e = edge_between(&hollow, "Deckel", "Wand 1");
    match k().blend(&hollow, &req(&[e], fillet)) {
        Err(BlendError::Unsupported(m)) => assert_eq!(m, CAVITY),
        other => panic!("{:?}", other.map(|_| ())),
    }
    // Der Eingangskörper bleibt unverändert.
    assert_eq!(k().counts(&s), (6, 12));
}
