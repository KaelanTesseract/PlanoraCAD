//! Kantenketten, Tangentenkette, glatte Übergänge (Teilprojekt 8, Task 9).

use planora_kernel::blend::MAX_BLEND_EDGES;
use planora_kernel::blend::build::CAVITY;
use planora_kernel::blend::chain::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU};

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

/// Stadion: zwei Geraden (Länge 10) und zwei Halbkreise (r = 2), alles tangential.
fn stadium(h: f64) -> Solid {
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
                Segment::Arc {
                    center: DVec2::new(0.0, 2.0),
                    sweep: PI,
                },
            ],
        },
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, h).unwrap()
}

fn req(edges: &[Id<Edge>], kind: BlendKind, chain: bool) -> BlendRequest {
    BlendRequest {
        edges: edges
            .iter()
            .map(|&edge| BlendEdge {
                edge,
                first_face: None,
            })
            .collect(),
        kind,
        tangent_chain: chain,
    }
}

#[test]
fn the_top_loop_of_a_stadium_is_one_smooth_chain() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let chain = tangent_chain(&s, e);
    assert_eq!(chain.len(), 4, "{chain:?}");
    assert!(chain.contains(&e));
    // Auf einem Quader schließt keine Kante glatt an.
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&b, "Deckel", "Wand 1");
    assert_eq!(tangent_chain(&b, e), vec![e]);
}

#[test]
fn selected_edges_are_grouped_and_measured() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let picked = select_edges(&s, &req(&[e], BlendKind::Fillet { radius: 0.5 }, true)).unwrap();
    assert_eq!(picked.len(), 4);
    assert!(picked.iter().all(|c| c.chain == 0));
    let total = 20.0 + 4.0 * PI;
    assert!(
        (picked[0].chain_length - total).abs() < 1e-2,
        "{}",
        picked[0].chain_length
    );
    let sum: f64 = picked.iter().map(|c| c.length).sum();
    assert!((sum - picked[0].chain_length).abs() < 1e-9);
    // Anteile wachsen längs der Kette von 0 bis 1.
    let mut ends: Vec<(f64, f64)> = picked
        .iter()
        .map(|c| (c.fraction(0.0), c.fraction(c.length)))
        .collect();
    ends.iter_mut().for_each(|(a, b)| {
        if a > b {
            std::mem::swap(a, b)
        }
    });
    ends.sort_by(|x, y| x.0.total_cmp(&y.0));
    assert!(ends[0].0.abs() < 1e-12 && (ends[3].1 - 1.0).abs() < 1e-12);
    // Ohne Tangentenkette nur die gewählte Kante; zwei getrennte Kanten sind zwei Ketten.
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&b, "Deckel", "Wand 1");
    let e2 = edge_between(&b, "Boden", "Wand 3");
    let picked = select_edges(
        &b,
        &req(&[e1, e2, e1], BlendKind::Fillet { radius: 0.5 }, false),
    )
    .unwrap();
    assert_eq!(picked.len(), 2);
    assert_ne!(picked[0].chain, picked[1].chain);
}

#[test]
fn a_tangent_chain_fillet_runs_around_the_stadium() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = k()
        .blend(&s, &req(&[e], BlendKind::Fillet { radius: 0.5 }, true))
        .unwrap();
    validate(&b.solid).unwrap();
    assert_eq!(b.edges.len(), 4);
    let roles: Vec<String> = b
        .solid
        .faces
        .iter()
        .map(|(id, _)| face_name(&b.solid, id))
        .collect();
    for n in 1..=4 {
        assert!(roles.contains(&format!("Rundung {n}")), "{roles:?}");
    }
    // Steiner: Querschnitt mal Weg des Schwerpunkts (Umfang minus 2π·Abstand).
    let area = (1.0 - PI / 4.0) * 0.25;
    let offset = 0.5 * (5.0 / 6.0 - PI / 4.0) / (1.0 - PI / 4.0);
    let want = -area * (20.0 + 4.0 * PI - 2.0 * PI * offset);
    assert!(
        (b.volume_change - want).abs() < 2e-3 * want.abs(),
        "{} statt {want}",
        b.volume_change
    );
}

#[test]
fn without_the_tangent_chain_the_end_at_a_curved_wall_is_named() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    match k().blend(&s, &req(&[e], BlendKind::Fillet { radius: 0.5 }, false)) {
        Err(BlendError::Unsupported(m)) => assert!(m.contains("nicht abschließen"), "{m}"),
        other => panic!("{:?}", other.map(|_| ())),
    }
}

#[test]
fn independent_edges_in_one_feature_add_up() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&b, "Deckel", "Wand 1");
    let e2 = edge_between(&b, "Boden", "Wand 3");
    let r = k()
        .blend(
            &b,
            &req(&[e1, e2], BlendKind::Fillet { radius: 1.0 }, false),
        )
        .unwrap();
    validate(&r.solid).unwrap();
    assert_eq!(r.solid.faces.len(), 8);
    let want = -2.0 * (1.0 - PI / 4.0) * 10.0;
    assert!((r.volume_change - want).abs() < 1e-3 * want.abs());
}

#[test]
fn two_edges_at_a_corner_are_refused_until_corners_exist() {
    // Übergang (P12): Task 11 ersetzt diesen Test durch Gehrung und Kugelecke.
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let e1 = edge_between(&b, "Deckel", "Wand 1");
    let e2 = edge_between(&b, "Deckel", "Wand 2");
    assert!(matches!(
        k().blend(
            &b,
            &req(&[e1, e2], BlendKind::Fillet { radius: 1.0 }, false)
        ),
        Err(BlendError::Corner(_))
    ));
}

// ---------------------------------------------------------------------------------------------
// Zusätzliche Prüfungen (Task 9): Gruppierung, Fläche 1 längs der Kette, Ablehnungen, Grenzen.

fn face_named(s: &Solid, name: &str) -> Id<Face> {
    s.faces
        .iter()
        .map(|(id, _)| id)
        .find(|&id| face_name(s, id) == name)
        .unwrap_or_else(|| panic!("keine Fläche {name}"))
}

fn with_first(
    e: Id<Edge>,
    first_face: Option<Id<Face>>,
    kind: BlendKind,
    chain: bool,
) -> BlendRequest {
    BlendRequest {
        edges: vec![BlendEdge {
            edge: e,
            first_face,
        }],
        kind,
        tangent_chain: chain,
    }
}

/// Rundes Vieleck: regelmäßiges `n`-Eck (Umkreis `r0`) um `rho` nach außen versetzt – `n`
/// Geraden und `n` tangentiale Bögen (r = `rho`), Höhe `h`. Dazu der Umfang des Vielecks.
fn rounded_polygon(n: usize, r0: f64, rho: f64, h: f64) -> (Solid, f64) {
    let p = |k: usize| {
        let a = TAU * k as f64 / n as f64;
        DVec2::new(a.cos(), a.sin()) * r0
    };
    let normal = |k: usize| {
        let a = TAU * (k as f64 + 0.5) / n as f64;
        DVec2::new(a.cos(), a.sin())
    };
    let mut segments = Vec::new();
    for k in 0..n {
        segments.push(Segment::Line {
            to: p(k + 1) + normal(k) * rho,
        });
        segments.push(Segment::Arc {
            center: p(k + 1),
            sweep: TAU / n as f64,
        });
    }
    let profile = Profile {
        outer: ProfileLoop {
            start: p(0) + normal(0) * rho,
            segments,
        },
        holes: vec![],
    };
    let side = 2.0 * r0 * (PI / n as f64).sin();
    (
        k().extrude(&profile, &Frame::WORLD, h).unwrap(),
        n as f64 * side,
    )
}

/// Rundung r an der Deckelschleife: Querschnitt (1 − π/4) r², Schwerpunkt im Abstand
/// r (5/6 − π/4)/(1 − π/4) von der Wand; Weg = Umfang des Vielecks + 2π (ρ − Abstand).
fn top_fillet_volume(r: f64, polygon: f64, rho: f64) -> f64 {
    let area = (1.0 - PI / 4.0) * r * r;
    let offset = r * (5.0 / 6.0 - PI / 4.0) / (1.0 - PI / 4.0);
    -area * (polygon + TAU * (rho - offset))
}

#[test]
fn a_run_across_the_start_of_a_closed_chain_is_one_chain() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let lp = tangent_chain(&s, e);
    assert_eq!(lp[0], e, "geschlossene Kette beginnt bei der Kante");
    // Gewählt: die Kante und ihr Vorgänger in der Kette (über den Kettenanfang hinweg).
    let fillet = BlendKind::Fillet { radius: 0.5 };
    let picked = select_edges(&s, &req(&[e, lp[3]], fillet.clone(), false)).unwrap();
    assert_eq!(picked.len(), 2);
    assert_eq!(picked[0].chain, picked[1].chain, "{picked:?}");
    let total = picked[0].length + picked[1].length;
    assert!((picked[0].chain_length - total).abs() < 1e-12);
    // Der Vorgänger kommt in Laufrichtung zuerst; sein Ende ist der Anfang der Kante.
    assert!(picked[1].offset.abs() < 1e-12);
    assert!((picked[0].offset - picked[1].length).abs() < 1e-12);
    let run_end = |c: &ChainedEdge| {
        if c.reversed {
            c.fraction(0.0)
        } else {
            c.fraction(c.length)
        }
    };
    let run_start = |c: &ChainedEdge| {
        if c.reversed {
            c.fraction(c.length)
        } else {
            c.fraction(0.0)
        }
    };
    assert!((run_end(&picked[1]) - run_start(&picked[0])).abs() < 1e-12);
    assert!(run_start(&picked[1]).abs() < 1e-12 && (run_end(&picked[0]) - 1.0).abs() < 1e-12);
    // Zwei gegenüberliegende Geraden: nicht benachbart, zwei Ketten.
    let picked = select_edges(&s, &req(&[lp[0], lp[2]], fillet.clone(), false)).unwrap();
    assert_ne!(picked[0].chain, picked[1].chain);
    // Mit Kette: alle vier in der Reihenfolge der Kette ab der gewählten Kante (P9).
    let picked = select_edges(&s, &req(&[lp[1]], fillet, true)).unwrap();
    let order: Vec<Id<Edge>> = picked.iter().map(|c| c.edge.edge).collect();
    assert_eq!(order, tangent_chain(&s, lp[1]));
}

#[test]
fn explicitly_chosen_smooth_edges_join_like_the_tangent_chain() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let all = tangent_chain(&s, e);
    let kind = BlendKind::Fillet { radius: 0.5 };
    let chained = k().blend(&s, &req(&[e], kind.clone(), true)).unwrap();
    let chosen = k().blend(&s, &req(&all, kind, false)).unwrap();
    validate(&chosen.solid).unwrap();
    assert_eq!(chosen.edges, all);
    assert_eq!(chained.edges, all);
    assert!((chosen.volume_change - chained.volume_change).abs() < 1e-12);
    assert_eq!(chosen.solid.faces.len(), chained.solid.faces.len());
    // Je Kante eine Rundungsfläche (P3): Zylinder an den Geraden, Torus an den Bögen.
    for (i, &edge) in all.iter().enumerate() {
        let role = format!("Rundung {}", i + 1);
        let f = face_named(&chained.solid, &role);
        let line = matches!(s.edges[edge].curve, Curve::Line { .. });
        match chained.solid.faces[f].surface {
            Surface::Cylinder { .. } => assert!(line, "{role}"),
            Surface::Torus { .. } => assert!(!line, "{role}"),
            ref other => panic!("{role}: {:?}", other.kind()),
        }
    }
}

#[test]
fn chamfer_sizes_follow_face_one_along_the_chain() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let kind = BlendKind::ChamferTwo {
        distance1: 0.3,
        distance2: 0.6,
    };
    // Querschnitt 0,3 · 0,6 / 2; Schwerpunkt im Abstand (Maß auf dem Deckel)/3 von der Wand.
    let want = |on_top: f64| -0.09 * (20.0 + TAU * (2.0 - on_top / 3.0));
    for (first, on_top) in [("Wand 1", 0.6), ("Deckel", 0.3)] {
        let r = with_first(e, Some(face_named(&s, first)), kind.clone(), true);
        let b = k().blend(&s, &r).unwrap();
        validate(&b.solid).unwrap();
        assert_eq!(b.edges.len(), 4);
        assert!(
            (b.volume_change - want(on_top)).abs() < 1e-9,
            "Fläche 1 {first}: {} statt {}",
            b.volume_change,
            want(on_top)
        );
    }
    // Vom Bogen aus gewählt: Fläche 1 = Halbzylinder, die geraden Wände setzen ihn fort.
    let arc = edge_between(&s, "Deckel", "Wand 2");
    let r = with_first(arc, Some(face_named(&s, "Wand 2")), kind.clone(), true);
    let b = k().blend(&s, &r).unwrap();
    assert!((b.volume_change - want(0.6)).abs() < 1e-9);
    // Ausdrücklich gewählte Kanten mit wechselnder Fläche 1: die Querschnitte passen an den
    // glatten Ecken nicht zusammen – benannt, kein Körper.
    let deckel = face_named(&s, "Deckel");
    let edges: Vec<BlendEdge> = tangent_chain(&s, e)
        .into_iter()
        .enumerate()
        .map(|(i, edge)| {
            let wall = edge_uses(&s, edge)
                .iter()
                .map(|u| u.face)
                .find(|&f| f != deckel);
            BlendEdge {
                edge,
                first_face: if i == 0 { Some(deckel) } else { wall },
            }
        })
        .collect();
    let r = BlendRequest {
        edges,
        kind,
        tangent_chain: false,
    };
    match k().blend(&s, &r) {
        Err(BlendError::Unsupported(m)) => assert!(m.contains("glatten Übergang"), "{m}"),
        other => panic!("{:?}", other.map(|b| b.volume_change)),
    }
}

#[test]
fn many_chained_edges_round_and_the_limit_counts_after_expansion() {
    // 16 Geraden und 16 Bögen: eine Kette aus 32 Kanten.
    let (s, polygon) = rounded_polygon(16, 10.0, 2.0, 3.0);
    let e = s
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| tangent_chain(&s, id).len() == 32)
        .expect("Deckel- oder Bodenschleife");
    let b = k()
        .blend(&s, &req(&[e], BlendKind::Fillet { radius: 0.5 }, true))
        .unwrap();
    validate(&b.solid).unwrap();
    assert_eq!(b.edges.len(), 32);
    let want = top_fillet_volume(0.5, polygon, 2.0);
    assert!(
        (b.volume_change - want).abs() < 2e-3 * want.abs(),
        "{} statt {want}",
        b.volume_change
    );
    // 130 Geraden und 130 Bögen: 260 Kanten nach Ausdehnung, mehr als erlaubt.
    let (s, _) = rounded_polygon(130, 60.0, 2.0, 3.0);
    let e = s
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| matches!(s.edges[id].curve, Curve::Circle { .. }))
        .expect("Bogenkante");
    assert_eq!(tangent_chain(&s, e).len(), 260);
    match k().blend(&s, &req(&[e], BlendKind::Fillet { radius: 0.5 }, true)) {
        Err(BlendError::InvalidParameter(m)) => assert!(
            m.contains(&format!("Zu viele Kanten (höchstens {MAX_BLEND_EDGES})")),
            "{m}"
        ),
        other => panic!("{:?}", other.map(|b| b.edges.len())),
    }
    // Ohne Kette bleibt es eine Kante.
    let picked = select_edges(&s, &req(&[e], BlendKind::Fillet { radius: 0.5 }, false)).unwrap();
    assert_eq!(picked.len(), 1);
}

#[test]
fn holes_and_cavities_are_named_for_chains_too() {
    // Stadion mit einem Durchgangsloch r = 0,2 nahe dem rechten Bogen (Mitte (10, 2), r = 2).
    // Die Berührkurve der Rundung r = 0,5 auf dem Deckel ist dort der Kreis mit r = 1,5.
    let with_hole = |dx: f64, r: f64| {
        let hole = Profile {
            outer: ProfileLoop {
                start: DVec2::new(10.0 + dx + r, 2.0),
                segments: vec![Segment::Arc {
                    center: DVec2::new(10.0 + dx, 2.0),
                    sweep: TAU,
                }],
            },
            holes: vec![],
        };
        let below = Frame {
            origin: DVec3::new(0.0, 0.0, -1.0),
            ..Frame::WORLD
        };
        let tool = k().extrude(&hole, &below, 5.0).unwrap();
        k().boolean(&stadium(3.0), &tool, BoolOp::Difference)
            .unwrap()
    };
    let kind = BlendKind::Fillet { radius: 0.5 };
    // Loch über dem Kreis (Abstand 1,4 … 1,8 von der Bogenmitte): Die Berührkurve verlässt den
    // Deckel (zu groß für ihn) – benannt.
    let s = with_hole(1.6, 0.2);
    let e = edge_between(&s, "Deckel", "Wand 1");
    match k().blend(&s, &req(&[e], kind.clone(), true)) {
        Err(BlendError::TooLarge { face }) => assert_eq!(face, "Deckel"),
        Err(BlendError::Unsupported(m)) => assert!(m.contains("im Bereich der Rundung"), "{m}"),
        other => panic!("{:?}", other.map(|b| b.edges.len())),
    }
    // Loch ganz im abgetragenen Bereich (1,65 … 1,85, zwischen Berührkreis und Wand): Loch und
    // Lochwand liegen im Bereich der Rundung – benannt.
    let s = with_hole(1.75, 0.1);
    let e = edge_between(&s, "Deckel", "Wand 1");
    match k().blend(&s, &req(&[e], kind.clone(), true)) {
        Err(BlendError::Unsupported(m)) => assert!(m.contains("im Bereich der Rundung"), "{m}"),
        other => panic!("{:?}", other.map(|b| b.edges.len())),
    }
    // Loch innerhalb (0,6 … 1,0): gültig, die Kette läuft trotzdem ganz herum.
    let s = with_hole(0.8, 0.2);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = k().blend(&s, &req(&[e], kind.clone(), true)).unwrap();
    validate(&b.solid).unwrap();
    assert_eq!(b.edges.len(), 4);
    // Hohlraum weit weg: wie bisher benannt abgelehnt.
    let void = k().make_box(DVec3::splat(0.5)).unwrap();
    let void = k()
        .transform(&void, &DMat4::from_translation(DVec3::new(4.0, 1.5, 1.0)))
        .unwrap();
    let s = k()
        .boolean(&stadium(3.0), &void, BoolOp::Difference)
        .unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    match k().blend(&s, &req(&[e], kind, true)) {
        Err(BlendError::Unsupported(m)) => assert_eq!(m, CAVITY),
        other => panic!("{:?}", other.map(|b| b.edges.len())),
    }
}

#[test]
fn independent_edges_never_give_a_wrong_body() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    // Vier senkrechte Kanten auf einmal: je eine Rundungsfläche, Volumen addiert sich.
    let verticals: Vec<Id<Edge>> = [
        ("Wand 1", "Wand 2"),
        ("Wand 2", "Wand 3"),
        ("Wand 3", "Wand 4"),
        ("Wand 4", "Wand 1"),
    ]
    .iter()
    .map(|(a, c)| edge_between(&b, a, c))
    .collect();
    let r = k()
        .blend(
            &b,
            &req(&verticals, BlendKind::Fillet { radius: 2.0 }, true),
        )
        .unwrap();
    validate(&r.solid).unwrap();
    assert_eq!(r.solid.faces.len(), 10);
    let want = -4.0 * (4.0 - PI) * 10.0;
    assert!((r.volume_change - want).abs() < 1e-3 * want.abs());
    // Zwei parallele Kanten derselben Fläche, deren Rundungen sich auf ihr überschneiden
    // (Maß 5,5 auf einem 10 breiten Deckel): benannt, nie ein Körper.
    let e1 = edge_between(&b, "Deckel", "Wand 1");
    let e3 = edge_between(&b, "Deckel", "Wand 3");
    for kind in [
        BlendKind::Fillet { radius: 5.5 },
        BlendKind::ChamferEqual { distance: 5.5 },
    ] {
        match k().blend(&b, &req(&[e1, e3], kind.clone(), false)) {
            Err(BlendError::TooLarge { .. }) | Err(BlendError::Unsupported(_)) => {}
            other => panic!("{kind:?}: {:?}", other.map(|b| b.solid.faces.len())),
        }
    }
}

#[test]
fn chain_helpers_survive_unknown_ids() {
    let s = stadium(3.0);
    let ghost: Id<Edge> = Id::from_index(9999);
    assert!(tangent_chain(&s, ghost).is_empty());
    let v = s.edges.iter().next().unwrap().1.start;
    assert_eq!(smooth_neighbour(&s, ghost, v), None);
    assert_eq!(leaving_direction(&s, ghost, v), DVec3::ZERO);
    let fillet = BlendKind::Fillet { radius: 0.5 };
    assert!(matches!(
        select_edges(&s, &req(&[ghost], fillet.clone(), true)),
        Err(BlendError::EdgeNotFound)
    ));
    assert!(matches!(
        k().blend(&s, &req(&[ghost], fillet, true)),
        Err(BlendError::EdgeNotFound)
    ));
}
