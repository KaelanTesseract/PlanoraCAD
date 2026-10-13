//! Ecken: Gehrung zweier Rundungen, Kugelstück an drei Kanten (Teilprojekt 8, Task 11).

use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI};

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

fn blend(s: &Solid, edges: &[Id<Edge>], kind: BlendKind) -> Result<Blended, BlendError> {
    k().blend(
        s,
        &BlendRequest {
            edges: edges
                .iter()
                .map(|&edge| BlendEdge {
                    edge,
                    first_face: None,
                })
                .collect(),
            kind,
            tangent_chain: false,
        },
    )
}

fn names(s: &Solid) -> Vec<String> {
    s.faces.iter().map(|(id, _)| face_name(s, id)).collect()
}

#[test]
fn two_fillets_at_a_corner_meet_in_a_miter() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&b, "Deckel", "Wand 1");
    let e2 = edge_between(&b, "Deckel", "Wand 2");
    let r = blend(&b, &[e1, e2], BlendKind::Fillet { radius: 1.0 }).unwrap();
    validate(&r.solid).unwrap();
    assert_eq!(r.solid.faces.len(), 8);
    let c = blend(&b, &[e1, e2], BlendKind::ChamferEqual { distance: 1.0 }).unwrap();
    validate(&c.solid).unwrap();
}

#[test]
fn three_fillets_at_a_corner_get_a_sphere_patch() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let edges = [
        edge_between(&b, "Deckel", "Wand 1"),
        edge_between(&b, "Deckel", "Wand 2"),
        edge_between(&b, "Wand 1", "Wand 2"),
    ];
    let r = blend(&b, &edges, BlendKind::Fillet { radius: 1.0 }).unwrap();
    validate(&r.solid).unwrap();
    assert!(names(&r.solid).contains(&"Ecke 1".to_string()));
    let (_, corner) = r
        .solid
        .faces
        .iter()
        .find(|(id, _)| face_name(&r.solid, *id) == "Ecke 1")
        .unwrap();
    match &corner.surface {
        Surface::Sphere { frame, radius } => {
            assert!((radius - 1.0).abs() < 1e-12);
            assert!(
                frame.origin.distance(DVec3::new(9.0, 1.0, 9.0)) < 1e-9,
                "{:?}",
                frame.origin
            );
        }
        other => panic!("{other:?}"),
    }
    // Schätzung: Kanten bis zur Kugelebene, dazu das Eckstück.
    let a = 1.0 - PI / 4.0;
    let want = -(3.0 * a * (10.0 - 1.0) + (1.0 - PI / 6.0));
    assert!(
        (r.volume_change - want).abs() < 2e-3 * want.abs(),
        "{} statt {want}",
        r.volume_change
    );
}

#[test]
fn all_twelve_box_edges_round_into_eight_corners() {
    let b = k().make_box(DVec3::new(10.0, 8.0, 6.0)).unwrap();
    let edges: Vec<Id<Edge>> = b.edges.iter().map(|(id, _)| id).collect();
    let r = blend(&b, &edges, BlendKind::Fillet { radius: 1.0 }).unwrap();
    validate(&r.solid).unwrap();
    assert_eq!(r.solid.faces.len(), 6 + 12 + 8);
    for n in 1..=8 {
        assert!(names(&r.solid).contains(&format!("Ecke {n}")));
    }
}

#[test]
fn unsupported_corners_are_named() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let three = [
        edge_between(&b, "Deckel", "Wand 1"),
        edge_between(&b, "Deckel", "Wand 2"),
        edge_between(&b, "Wand 1", "Wand 2"),
    ];
    assert!(matches!(
        blend(&b, &three, BlendKind::ChamferEqual { distance: 1.0 }),
        Err(BlendError::Corner(m)) if m.contains("Fase")
    ));
    // Konvexe Deckelkante trifft die konkave Innenkante des L-Profils.
    let l = l_profile();
    let mixed = [
        edge_between(&l, "Deckel", "Wand 3"),
        edge_between(&l, "Wand 3", "Wand 4"),
    ];
    assert!(matches!(
        blend(&l, &mixed, BlendKind::Fillet { radius: 1.0 }),
        Err(BlendError::Corner(m)) if m.contains("konkav")
    ));
}

#[test]
fn the_ball_center_is_at_radius_from_three_planes() {
    let n = [DVec3::Z, -DVec3::Y, DVec3::X];
    let s =
        planora_kernel::blend::corner::ball_center(n, DVec3::new(10.0, 0.0, 10.0), 1.0).unwrap();
    assert!(s.distance(DVec3::new(9.0, 1.0, 9.0)) < 1e-12);
    assert!(
        planora_kernel::blend::corner::ball_center(
            [DVec3::Z, DVec3::Z, DVec3::X],
            DVec3::ZERO,
            1.0
        )
        .is_none()
    );
    // Feindliche Zahlen: kein Mittelpunkt statt NaN.
    assert!(planora_kernel::blend::corner::ball_center(n, DVec3::splat(f64::NAN), 1.0).is_none());
    assert!(planora_kernel::blend::corner::ball_center(n, DVec3::ZERO, f64::INFINITY).is_none());
}

// ---------------------------------------------------------------------------------------------
// Zusätzliche Prüfungen (Task 11): einspringende und konkave Ecken, schräge Ecken, Löcher und
// Taschen am Kugelstück, zu kurze Kanten.

/// L-Profil (Schenkel 4 mm, Höhe 5): Wand 3 (y = 4) und Wand 4 (x = 4) bilden die konkave
/// Innenkante.
fn l_profile() -> Solid {
    k().extrude(
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
    .unwrap()
}

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

/// Würfel 10 minus Tasche von oben (x, y 2…8, Boden bei z = 5): Boden- und Wandkanten der Tasche
/// sind konkav.
fn pocket_box() -> Solid {
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let tool = k()
        .transform(
            &k().make_box(DVec3::new(6.0, 6.0, 6.0)).unwrap(),
            &DMat4::from_translation(DVec3::new(2.0, 2.0, 5.0)),
        )
        .unwrap();
    k().boolean(&cube, &tool, BoolOp::Difference).unwrap()
}

#[test]
fn a_reentrant_corner_is_named() {
    // Zwei konvexe Deckelkanten an der einspringenden Ecke (4, 4, 5), dritte Kante konkav: Die
    // Gehrung wäre nicht die Schnittmenge der einzeln gerundeten Körper.
    let l = l_profile();
    let pair = [
        edge_between(&l, "Deckel", "Wand 3"),
        edge_between(&l, "Deckel", "Wand 4"),
    ];
    for kind in [
        BlendKind::Fillet { radius: 1.0 },
        BlendKind::ChamferEqual { distance: 1.0 },
    ] {
        match blend(&l, &pair, kind) {
            Err(BlendError::Corner(m)) => {
                assert!(m.contains("konkav") && m.contains("Wand 3"), "{m}")
            }
            other => panic!("{:?}", other.map(|b| b.solid.faces.len())),
        }
    }
    // Die übrigen (konvexen) Ecken der Deckelschleife bekommen eine Gehrung.
    let outer = [
        edge_between(&l, "Deckel", "Wand 1"),
        edge_between(&l, "Deckel", "Wand 2"),
        edge_between(&l, "Deckel", "Wand 3"),
    ];
    let r = blend(&l, &outer, BlendKind::Fillet { radius: 1.0 }).unwrap();
    validate(&r.solid).unwrap();
    assert_eq!(r.solid.faces.len(), l.faces.len() + 3);
}

fn face_named(s: &Solid, name: &str) -> Id<Face> {
    s.faces
        .iter()
        .map(|(id, _)| id)
        .find(|&id| face_name(s, id) == name)
        .unwrap_or_else(|| panic!("keine Fläche {name}"))
}

#[test]
fn a_miter_needs_mirror_images() {
    // Fase 1 × 2: mit Fläche 1 = Deckel an beiden Kanten spiegelgleich (Gehrung), sonst treffen
    // sich die Fasen nicht auf der winkelhalbierenden Ebene.
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&b, "Deckel", "Wand 1");
    let e2 = edge_between(&b, "Deckel", "Wand 2");
    let kind = BlendKind::ChamferTwo {
        distance1: 1.0,
        distance2: 2.0,
    };
    let req = |f1: &str, f2: &str| BlendRequest {
        edges: vec![
            BlendEdge {
                edge: e1,
                first_face: Some(face_named(&b, f1)),
            },
            BlendEdge {
                edge: e2,
                first_face: Some(face_named(&b, f2)),
            },
        ],
        kind: kind.clone(),
        tangent_chain: false,
    };
    let ok = k().blend(&b, &req("Deckel", "Deckel")).unwrap();
    validate(&ok.solid).unwrap();
    let ok = k().blend(&b, &req("Wand 1", "Wand 2")).unwrap();
    validate(&ok.solid).unwrap();
    match k().blend(&b, &req("Deckel", "Wand 2")) {
        Err(BlendError::Corner(m)) => {
            assert!(
                m.contains("winkelhalbierenden") && m.contains("Deckel"),
                "{m}"
            )
        }
        other => panic!("{:?}", other.map(|b| b.solid.faces.len())),
    }
}

#[test]
fn concave_fillets_inside_a_pocket_meet_in_a_miter() {
    let s = pocket_box();
    let floor = [
        edge_at(&s, DVec3::new(2.0, 2.0, 5.0), DVec3::new(8.0, 2.0, 5.0)),
        edge_at(&s, DVec3::new(8.0, 2.0, 5.0), DVec3::new(8.0, 8.0, 5.0)),
    ];
    for kind in [
        BlendKind::Fillet { radius: 1.0 },
        BlendKind::ChamferEqual { distance: 1.0 },
    ] {
        let r = blend(&s, &floor, kind.clone()).unwrap();
        validate(&r.solid).unwrap();
        assert_eq!(r.solid.faces.len(), s.faces.len() + 2);
        // Konkav: Material kommt hinzu.
        assert!(r.volume_change > 0.0, "{kind:?}: {}", r.volume_change);
    }
    // Drei konkave Kanten an einer Ecke der Tasche: benannt abgelehnt.
    let three = [
        floor[0],
        floor[1],
        edge_at(&s, DVec3::new(8.0, 2.0, 5.0), DVec3::new(8.0, 2.0, 10.0)),
    ];
    match blend(&s, &three, BlendKind::Fillet { radius: 1.0 }) {
        Err(BlendError::Corner(m)) => assert!(m.contains("konkav"), "{m}"),
        other => panic!("{:?}", other.map(|b| b.solid.faces.len())),
    }
}

/// Dreiecksprisma (0,0), (10,0), (5,8), Höhe 5: keine rechtwinkligen Ecken zwischen den Wänden.
fn triangle_prism() -> Solid {
    k().extrude(
        &Profile {
            outer: ProfileLoop::polygon(&[
                DVec2::new(0.0, 0.0),
                DVec2::new(10.0, 0.0),
                DVec2::new(5.0, 8.0),
            ])
            .unwrap(),
            holes: vec![],
        },
        &Frame::WORLD,
        5.0,
    )
    .unwrap()
}

#[test]
fn oblique_corners_miter_and_get_a_sphere() {
    let s = triangle_prism();
    let top: Vec<Id<Edge>> = ["Wand 1", "Wand 2", "Wand 3"]
        .iter()
        .map(|w| edge_between(&s, "Deckel", w))
        .collect();
    // Gehrung an spitzen Ecken (Wände senkrecht, spiegelgleich zur Winkelhalbierenden).
    let r = blend(&s, &top, BlendKind::Fillet { radius: 1.0 }).unwrap();
    validate(&r.solid).unwrap();
    assert_eq!(r.solid.faces.len(), s.faces.len() + 3);
    // Drei Kanten an einer schrägen Ecke: Kugelstück (Task 17; vorher nur rechtwinklig).
    let three = [top[0], top[1], edge_between(&s, "Wand 1", "Wand 2")];
    let c = blend(&s, &three, BlendKind::Fillet { radius: 1.0 }).unwrap();
    validate(&c.solid).unwrap();
    assert_eq!(c.solid.faces.len(), s.faces.len() + 4);
    assert!(names(&c.solid).contains(&"Ecke 1".to_string()));
}

#[test]
fn corners_on_curved_faces_are_named() {
    // D-Profil: Gerade (0,0)–(10,0), Halbkreis zurück; an den Ecken trifft die ebene Wand die
    // Zylinderwand.
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(5.0, 0.0),
                    sweep: PI,
                },
            ],
        },
        holes: vec![],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 4.0).unwrap();
    let pair = [
        edge_between(&s, "Deckel", "Wand 1"),
        edge_between(&s, "Deckel", "Wand 2"),
    ];
    match blend(&s, &pair, BlendKind::Fillet { radius: 1.0 }) {
        Err(BlendError::Corner(m)) => assert!(m.contains("Wand 2"), "{m}"),
        other => panic!("{:?}", other.map(|b| b.solid.faces.len())),
    }
    let three = [
        pair[0],
        pair[1],
        edge_at(&s, DVec3::ZERO, DVec3::new(0.0, 0.0, 4.0)),
    ];
    match blend(&s, &three, BlendKind::Fillet { radius: 1.0 }) {
        Err(BlendError::Corner(m)) => assert!(m.contains("Wand"), "{m}"),
        Err(e) => panic!("andere Ablehnung: {e:?}"),
        Ok(b) => panic!("gültig: {}", b.solid.faces.len()),
    }
}

/// Die drei Kanten an der Würfelecke (0, 0, 10).
fn corner_edges(s: &Solid, h: f64) -> [Id<Edge>; 3] {
    let v = DVec3::new(0.0, 0.0, h);
    [
        edge_at(s, v, DVec3::new(10.0, 0.0, h)),
        edge_at(s, v, DVec3::new(0.0, 10.0, h)),
        edge_at(s, v, DVec3::ZERO),
    ]
}

#[test]
fn a_pocket_reaching_into_the_corner_ball_is_refused() {
    // Tasche von unten (x, y 1,3…1,7) bis z = 9,5: Sie liegt außerhalb aller drei Rundungen
    // (Abstand von der senkrechten Achse (3, 3) höchstens 2,4 < 3), reicht aber in das Kugelstück
    // um (3, 3, 7) mit r = 3. Nur die Prüfung des Eckstücks findet sie.
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let pocket = |top: f64| {
        let tool = k()
            .transform(
                &k().make_box(DVec3::new(0.4, 0.4, top + 1.0)).unwrap(),
                &DMat4::from_translation(DVec3::new(1.3, 1.3, -1.0)),
            )
            .unwrap();
        k().boolean(&cube, &tool, BoolOp::Difference).unwrap()
    };
    let deep = pocket(9.5);
    match blend(
        &deep,
        &corner_edges(&deep, 10.0),
        BlendKind::Fillet { radius: 3.0 },
    ) {
        Err(BlendError::Unsupported(m)) => assert!(m.contains("Bereich der Rundung"), "{m}"),
        other => panic!("{:?}", other.map(|b| b.solid.faces.len())),
    }
    // Bis z = 8 bleibt die Tasche in der Kugel (Abstand der obersten Ecke 2,6 < 3): gültig.
    let flat = pocket(8.0);
    let r = blend(
        &flat,
        &corner_edges(&flat, 10.0),
        BlendKind::Fillet { radius: 3.0 },
    )
    .unwrap();
    validate(&r.solid).unwrap();
    assert_eq!(r.solid.faces.len(), flat.faces.len() + 4);
}

#[test]
fn holes_and_short_edges_at_a_corner_are_named() {
    // Loch (r 0,5) bei (1,5; 1,5) durch die ganze Höhe: Bei r = 3 liegt es im Deckel außerhalb der
    // neuen Deckelkontur (x, y ≥ 3).
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::splat(10.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(1.5, 1.5), 0.5)],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 10.0).unwrap();
    match blend(
        &s,
        &corner_edges(&s, 10.0),
        BlendKind::Fillet { radius: 3.0 },
    ) {
        Err(BlendError::Unsupported(m)) => assert!(m.contains("Bereich der Rundung"), "{m}"),
        other => panic!("{:?}", other.map(|b| b.solid.faces.len())),
    }
    // r = 0,5: Loch weit genug weg, gültig.
    let r = blend(
        &s,
        &corner_edges(&s, 10.0),
        BlendKind::Fillet { radius: 0.5 },
    )
    .unwrap();
    validate(&r.solid).unwrap();
    // Flacher Quader: die senkrechten Kanten (1,5) sind kürzer als die beiden Kugelecken (2 · 1).
    let flat = k().make_box(DVec3::new(10.0, 10.0, 1.5)).unwrap();
    let all: Vec<Id<Edge>> = flat.edges.iter().map(|(id, _)| id).collect();
    assert!(matches!(
        blend(&flat, &all, BlendKind::Fillet { radius: 1.0 }),
        Err(BlendError::TooLarge { .. })
    ));
}

#[test]
fn corners_follow_the_tangent_chain_switch_and_stay_deterministic() {
    // Mit eingeschalteter Tangentenkette (keine glatten Nachbarn am Quader) dasselbe Ergebnis.
    let b = k().make_box(DVec3::new(10.0, 8.0, 6.0)).unwrap();
    let edges: Vec<Id<Edge>> = b.edges.iter().map(|(id, _)| id).collect();
    let one = blend(&b, &edges, BlendKind::Fillet { radius: 1.0 }).unwrap();
    let two = k()
        .blend(
            &b,
            &BlendRequest {
                edges: edges
                    .iter()
                    .rev()
                    .map(|&edge| BlendEdge {
                        edge,
                        first_face: None,
                    })
                    .collect(),
                kind: BlendKind::Fillet { radius: 1.0 },
                tangent_chain: true,
            },
        )
        .unwrap();
    assert_eq!(one.solid.faces.len(), two.solid.faces.len());
    assert!((one.volume_change - two.volume_change).abs() < 1e-9);
    // Ecke N nach Eckpunkt-Id (P9): gleiche Kugelmitten unabhängig von der Kantenreihenfolge.
    let centers = |s: &Solid| -> Vec<DVec3> {
        (1..=8)
            .map(|n| {
                let (_, f) = s
                    .faces
                    .iter()
                    .find(|(id, _)| face_name(s, *id) == format!("Ecke {n}"))
                    .unwrap();
                f.surface.frame().origin
            })
            .collect()
    };
    let (c1, c2) = (centers(&one.solid), centers(&two.solid));
    for (a, b) in c1.iter().zip(&c2) {
        assert!(a.distance(*b) < 1e-12, "{a} {b}");
    }
}

#[test]
fn equal_sizes_at_an_oblique_prism_corner_name_the_missing_mirror_image() {
    // Review Stufe 2: Deckelkante (Kantenwinkel 90°) und senkrechte Kante (Kantenwinkel der
    // Prismenecke, nicht 90°) mit gleichem Maß. Die Rundungen liegen nicht spiegelbildlich; die
    // Meldung darf nicht von verschiedenen Maßen oder Winkeln der Eingabe sprechen.
    let s = triangle_prism();
    let top = edge_between(&s, "Deckel", "Wand 1");
    let vertical = edge_at(&s, DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 5.0));
    for kind in [
        BlendKind::Fillet { radius: 1.0 },
        BlendKind::ChamferEqual { distance: 1.0 },
    ] {
        match blend(&s, &[top, vertical], kind.clone()) {
            Err(BlendError::Corner(m)) => assert!(
                m.contains("winkelhalbierenden")
                    && m.contains("spiegelbildlich")
                    && m.contains("Kantenwinkel"),
                "{kind:?}: {m}"
            ),
            other => panic!("{kind:?}: {:?}", other.map(|b| b.solid.faces.len())),
        }
    }
}

#[test]
fn oblique_corners_of_three_planes_and_skin_corners() {
    let tri = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::polygon(&[
                    DVec2::new(0.0, 0.0),
                    DVec2::new(10.0, 0.0),
                    DVec2::new(5.0, 75f64.sqrt()),
                ])
                .unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            6.0,
        )
        .unwrap();
    let edges = [
        edge_between(&tri, "Deckel", "Wand 1"),
        edge_between(&tri, "Deckel", "Wand 2"),
        edge_between(&tri, "Wand 1", "Wand 2"),
    ];
    let r = blend(&tri, &edges, BlendKind::Fillet { radius: 0.5 }).unwrap();
    validate(&r.solid).unwrap();
    assert!(names(&r.solid).contains(&"Ecke 1".to_string()));
    // Variabler Radius an einer Ecke: vor jeder Eckbehandlung benannt abgelehnt (build.rs,
    // `march::VARIABLE_CORNER`); Gehrung und Kugelecke sehen keine Hautflächen.
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let two = [
        edge_between(&b, "Deckel", "Wand 1"),
        edge_between(&b, "Deckel", "Wand 2"),
    ];
    assert!(matches!(
        blend(&b, &two, BlendKind::VariableFillet { points: vec![(0.0, 1.0), (1.0, 1.0)] }),
        Err(BlendError::Corner(m)) if m == planora_kernel::blend::march::VARIABLE_CORNER
    ));
}
