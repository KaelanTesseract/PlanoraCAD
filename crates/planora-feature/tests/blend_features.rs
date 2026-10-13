//! Feature „Verrunden“: Zeitleiste, Neuberechnung, Verweise, Fehler (Teilprojekt 8, Task 7).

use planora_feature::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Solid};
use planora_math::{DVec3, PI, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};

fn volume(s: &Solid) -> f64 {
    mass_properties(
        &tessellate(
            s,
            &TessellationOptions {
                chord_tolerance: 1e-3,
            },
        )
        .unwrap(),
    )
    .volume
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

fn edge_ref(doc: &Document, body: BodyRef, a: &str, b: &str) -> EdgeRef {
    let s = doc.body(body).unwrap();
    EdgeRef::from_edge(s, edge_between(s, a, b), &Tolerance::default()).unwrap()
}

fn with_box(size: f64) -> (Document, BodyRef) {
    let mut d = Document::new();
    let f = d
        .add_feature(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::splat(size),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    (
        d,
        BodyRef {
            feature: f,
            index: 0,
        },
    )
}

fn fillet(body: BodyRef, edges: Vec<EdgeRef>, art: BlendArt) -> FeatureKind {
    FeatureKind::Verrunden {
        body,
        edges,
        art,
        tangentenkette: true,
    }
}

fn status(d: &Document, id: FeatureId) -> FeatureStatus {
    d.feature(id).unwrap().status.clone()
}

#[test]
fn a_fillet_feature_replaces_its_body_and_follows_edits() {
    let (mut d, b) = with_box(10.0);
    let r = edge_ref(&d, b, "Deckel", "Wand 1");
    let f = d
        .add_feature(
            "Rundung 1",
            fillet(b, vec![r], BlendArt::Rundung { radius: 2.0 }),
        )
        .unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    assert_eq!(d.feature(f).unwrap().kind.label(), "Rundung");
    assert_eq!(d.feature(f).unwrap().kind.dependencies(), vec![b.feature]);
    let visible = d.visible_bodies();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].body.feature, f);
    let spandrel = (1.0 - PI / 4.0) * 4.0;
    assert!((volume(visible[0].solid) - (1000.0 - spandrel * 10.0)).abs() < 0.05);
    // Größerer Quader: die Kante wird über Marke und Hinweis wiedergefunden.
    d.update(b.feature, |k| {
        if let FeatureKind::Box { size, .. } = k {
            *size = DVec3::splat(12.0);
        }
    })
    .unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    let v = volume(d.visible_bodies()[0].solid);
    assert!((v - (1728.0 - spandrel * 12.0)).abs() < 0.05, "{v}");
    // Neue Flächen tragen die Herkunft des Features.
    let s = d.visible_bodies()[0].solid;
    assert!(s.faces.iter().any(|(_, x)| {
        x.tag
            .as_ref()
            .is_some_and(|t| t.role == "Rundung 1" && t.origin == f.0)
    }));
    d.check_invariants().unwrap();
}

#[test]
fn chamfer_arts_compute_and_switch() {
    let (mut d, b) = with_box(10.0);
    let r = edge_ref(&d, b, "Deckel", "Wand 1");
    let f = d
        .add_feature(
            "Fase 1",
            fillet(
                b,
                vec![r],
                BlendArt::FaseZwei {
                    abstand1: 1.0,
                    abstand2: 2.0,
                },
            ),
        )
        .unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    assert_eq!(d.feature(f).unwrap().kind.label(), "Fase");
    let v = volume(d.visible_bodies()[0].solid);
    assert!((v - (1000.0 - 0.5 * 2.0 * 10.0)).abs() < 1e-6, "{v}");
    d.update(f, |k| {
        if let FeatureKind::Verrunden { art, .. } = k {
            *art = BlendArt::FaseWinkel {
                abstand: 1.0,
                winkel: 45.0,
            };
        }
    })
    .unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    let v = volume(d.visible_bodies()[0].solid);
    assert!((v - (1000.0 - 0.5 * 10.0)).abs() < 1e-6, "{v}");
}

#[test]
fn hostile_blend_parameters_become_error_status() {
    let (mut d, b) = with_box(10.0);
    let r = edge_ref(&d, b, "Deckel", "Wand 1");
    let points = |n: usize| -> Vec<(f64, f64)> {
        (0..n)
            .map(|i| (100.0 * i as f64 / (n - 1) as f64, 1.0))
            .collect()
    };
    let arts = [
        BlendArt::Rundung { radius: f64::NAN },
        BlendArt::Rundung {
            radius: f64::INFINITY,
        },
        BlendArt::Rundung {
            radius: f64::NEG_INFINITY,
        },
        BlendArt::Rundung { radius: 0.0 },
        BlendArt::Rundung { radius: -1.0 },
        BlendArt::Rundung { radius: 1e300 },
        BlendArt::Rundung { radius: 65.0 },
        BlendArt::FaseGleich { abstand: 0.0 },
        BlendArt::FaseGleich { abstand: f64::NAN },
        BlendArt::FaseZwei {
            abstand1: 0.0,
            abstand2: 1.0,
        },
        BlendArt::FaseZwei {
            abstand1: 1.0,
            abstand2: -2.0,
        },
        BlendArt::FaseZwei {
            abstand1: f64::NAN,
            abstand2: 1.0,
        },
        BlendArt::FaseZwei {
            abstand1: 1.0,
            abstand2: f64::INFINITY,
        },
        BlendArt::FaseWinkel {
            abstand: 0.0,
            winkel: 30.0,
        },
        BlendArt::FaseWinkel {
            abstand: -1.0,
            winkel: 30.0,
        },
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 0.0,
        },
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 180.0,
        },
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: -30.0,
        },
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: f64::NAN,
        },
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: f64::INFINITY,
        },
        BlendArt::RundungVariabel {
            punkte: vec![(50.0, 1.0), (10.0, 1.0)],
        },
        BlendArt::RundungVariabel { punkte: vec![] },
        BlendArt::RundungVariabel { punkte: points(65) },
    ];
    for art in arts {
        let f = d
            .add_feature("X", fillet(b, vec![r.clone()], art.clone()))
            .unwrap();
        match status(&d, f) {
            FeatureStatus::Error(e) => {
                // Deutsche, lesbare Meldung (Kernel- und Feature-Texte nennen Maß, Winkel,
                // Radius, Abstand, Punkte oder den Bereich) ohne Zahlenmüll.
                assert!(!e.is_empty(), "{art:?}");
                assert!(
                    [
                        "Maß",
                        "Winkel",
                        "Radius",
                        "Abstand",
                        "Punkte",
                        "Positionen",
                        "außerhalb"
                    ]
                    .iter()
                    .any(|w| e.contains(w)),
                    "{art:?}: {e}"
                );
                assert!(e.len() < 400, "{art:?}: {e}");
            }
            s => panic!("{art:?}: {s:?}"),
        }
        // Ein Folgefeature auf dem Ergebnis wird übersprungen und rechnet nie.
        let m = d
            .add_feature(
                "M",
                FeatureKind::Move {
                    body: BodyRef {
                        feature: f,
                        index: 0,
                    },
                    delta: DVec3::X,
                },
            )
            .unwrap();
        assert!(
            matches!(status(&d, m), FeatureStatus::Skipped(_)),
            "{art:?}: {:?}",
            status(&d, m)
        );
        d.delete(m).unwrap();
        d.delete(f).unwrap();
        d.check_invariants().unwrap();
    }
    let f = d
        .add_feature("Leer", fillet(b, vec![], BlendArt::Rundung { radius: 1.0 }))
        .unwrap();
    match status(&d, f) {
        FeatureStatus::Error(e) => assert!(e.contains("keine Kante"), "{e}"),
        s => panic!("{s:?}"),
    }
    d.delete(f).unwrap();
    // Doppelte Kanten: dokumentierte Abweichung von Review Focus 3 (dort wird eine Meldung
    // erwartet). Der Kernel entfernt Duplikate; das Ergebnis ist dasselbe wie mit einer Kante
    // und gültig (kein falscher Körper, kein Absturz).
    let single = d
        .add_feature(
            "Eine",
            fillet(b, vec![r.clone()], BlendArt::Rundung { radius: 1.0 }),
        )
        .unwrap();
    assert_eq!(status(&d, single), FeatureStatus::Ok);
    let want = volume(d.visible_bodies()[0].solid);
    d.delete(single).unwrap();
    let dup = d
        .add_feature(
            "Doppelt",
            fillet(b, vec![r.clone(), r], BlendArt::Rundung { radius: 1.0 }),
        )
        .unwrap();
    assert_eq!(status(&d, dup), FeatureStatus::Ok);
    assert!((volume(d.visible_bodies()[0].solid) - want).abs() < 1e-9);
    d.check_invariants().unwrap();
}

#[test]
fn blend_references_follow_upstream_changes() {
    let (mut d, b) = with_box(10.0);
    let r = edge_ref(&d, b, "Deckel", "Wand 1");
    let f1 = d
        .add_feature(
            "Rundung 1",
            fillet(b, vec![r.clone()], BlendArt::Rundung { radius: 1.0 }),
        )
        .unwrap();
    // Dieselbe Kante noch einmal: sie ist durch die erste Rundung verschwunden.
    let f2 = d
        .add_feature(
            "Rundung 2",
            fillet(
                BodyRef {
                    feature: f1,
                    index: 0,
                },
                vec![r],
                BlendArt::Rundung { radius: 1.0 },
            ),
        )
        .unwrap();
    let not_found = |d: &Document| match status(d, f2) {
        FeatureStatus::Error(e) => assert!(e.contains("nicht mehr vorhanden"), "{e}"),
        s => panic!("{s:?}"),
    };
    not_found(&d);
    // Unterdrücken des Quaders: beide Rundungen werden übersprungen, Dokument bleibt gültig.
    d.set_suppressed(b.feature, true).unwrap();
    assert!(matches!(status(&d, f1), FeatureStatus::Skipped(_)));
    assert!(matches!(status(&d, f2), FeatureStatus::Skipped(_)));
    d.set_suppressed(b.feature, false).unwrap();
    assert_eq!(status(&d, f1), FeatureStatus::Ok);
    // Nach dem Wiedereinschalten rechnet f2 wieder und meldet erneut die verschwundene Kante.
    not_found(&d);
    d.check_invariants().unwrap();

    // Eine Rundung, die gültig war, verliert ihre Kante durch eine Änderung davor: f1 rundet die
    // Kante A, f2 die gegenüberliegende Kante B. Ändert man f1 auf B, ist B verbraucht, f2 meldet
    // die fehlende Kante und das Folgefeature wird übersprungen.
    let (mut d, b) = with_box(10.0);
    let a = edge_ref(&d, b, "Deckel", "Wand 1");
    let edge_b = edge_ref(&d, b, "Deckel", "Wand 3");
    let f1 = d
        .add_feature(
            "Rundung 1",
            fillet(b, vec![a.clone()], BlendArt::Rundung { radius: 1.0 }),
        )
        .unwrap();
    let body1 = BodyRef {
        feature: f1,
        index: 0,
    };
    let f2 = d
        .add_feature(
            "Rundung 2",
            fillet(
                body1,
                vec![edge_b.clone()],
                BlendArt::Rundung { radius: 1.0 },
            ),
        )
        .unwrap();
    let m = d
        .add_feature(
            "M",
            FeatureKind::Move {
                body: BodyRef {
                    feature: f2,
                    index: 0,
                },
                delta: DVec3::X,
            },
        )
        .unwrap();
    assert_eq!(status(&d, f1), FeatureStatus::Ok);
    assert_eq!(status(&d, f2), FeatureStatus::Ok);
    assert_eq!(status(&d, m), FeatureStatus::Ok);
    d.update(f1, |k| {
        if let FeatureKind::Verrunden { edges, .. } = k {
            *edges = vec![edge_b.clone()];
        }
    })
    .unwrap();
    assert_eq!(status(&d, f1), FeatureStatus::Ok);
    match status(&d, f2) {
        FeatureStatus::Error(e) => assert!(e.contains("nicht mehr vorhanden"), "{e}"),
        s => panic!("{s:?}"),
    }
    assert!(matches!(status(&d, m), FeatureStatus::Skipped(_)));
    // Zurück auf Kante A: beide rechnen wieder.
    d.update(f1, |k| {
        if let FeatureKind::Verrunden { edges, .. } = k {
            *edges = vec![a.clone()];
        }
    })
    .unwrap();
    assert_eq!(status(&d, f2), FeatureStatus::Ok);
    assert_eq!(status(&d, m), FeatureStatus::Ok);
    d.check_invariants().unwrap();
}

#[test]
fn rounding_a_rounded_edge_is_named() {
    let (mut d, b) = with_box(10.0);
    let r = edge_ref(&d, b, "Deckel", "Wand 1");
    let f1 = d
        .add_feature(
            "Rundung 1",
            fillet(b, vec![r], BlendArt::Rundung { radius: 2.0 }),
        )
        .unwrap();
    let body = BodyRef {
        feature: f1,
        index: 0,
    };
    let tangent = edge_ref(&d, body, "Rundung 1", "Deckel");
    let f2 = d
        .add_feature(
            "Rundung 2",
            fillet(body, vec![tangent], BlendArt::Rundung { radius: 0.5 }),
        )
        .unwrap();
    match status(&d, f2) {
        FeatureStatus::Error(e) => assert!(e.contains("tangential"), "{e}"),
        s => panic!("{s:?}"),
    }
    // Eine andere Kante des Ergebnisses geht.
    let other = edge_ref(&d, body, "Boden", "Wand 3");
    let f3 = d
        .add_feature(
            "Fase 1",
            fillet(body, vec![other], BlendArt::FaseGleich { abstand: 1.0 }),
        )
        .unwrap();
    assert_eq!(status(&d, f3), FeatureStatus::Ok);
}

/// Review Stufe 2, I1: eine konkave und eine konvexe Kante in einem Feature (FaseZwei 0,5/0,8
/// an den senkrechten Kanten bei (4, 4) und (10, 0) eines L-Profils). Abtrag und Zugabe heben
/// sich auf; die Volumenprobe darf das richtige Ergebnis nicht ablehnen.
#[test]
fn convex_and_concave_edges_in_one_feature_pass_the_volume_probe() {
    use planora_math::Frame;
    use planora_sketch::model::Sketch;
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let pts: Vec<_> = [
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 4.0),
        (4.0, 4.0),
        (4.0, 10.0),
        (0.0, 10.0),
    ]
    .iter()
    .map(|&(x, y)| sk.add_point(x, y).unwrap())
    .collect();
    for i in 0..pts.len() {
        sk.add_line(pts[i], pts[(i + 1) % pts.len()]).unwrap();
    }
    let mut d = Document::new();
    let skf = d
        .add_feature(
            "Skizze 1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
            }),
        )
        .unwrap();
    let ex = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: skf,
                profile: 0,
                height: 5.0,
            },
        )
        .unwrap();
    let body = BodyRef {
        feature: ex,
        index: 0,
    };
    let s = d.body(body).unwrap();
    assert!((volume(s) - 320.0).abs() < 1e-6);
    let vertical = |x: f64, y: f64| {
        let id = s
            .edges
            .iter()
            .find(|(_, e)| {
                let (a, b) = (s.vertices[e.start].point, s.vertices[e.end].point);
                let at = |p: DVec3| (p.x - x).abs() < 1e-9 && (p.y - y).abs() < 1e-9;
                at(a) && at(b)
            })
            .map(|(id, _)| id)
            .unwrap();
        EdgeRef::from_edge(s, id, &Tolerance::default()).unwrap()
    };
    let edges = vec![vertical(4.0, 4.0), vertical(10.0, 0.0)];
    for art in [
        BlendArt::FaseZwei {
            abstand1: 0.5,
            abstand2: 0.8,
        },
        BlendArt::FaseGleich { abstand: 0.5 },
    ] {
        let f = d
            .add_feature(
                "Fase 1",
                FeatureKind::Verrunden {
                    body,
                    edges: edges.clone(),
                    art: art.clone(),
                    tangentenkette: false,
                },
            )
            .unwrap();
        assert_eq!(status(&d, f), FeatureStatus::Ok, "{art:?}");
        // Je Kante ein Dreieck mal Höhe 5: einmal abgetragen, einmal hinzugefügt.
        let v = volume(d.visible_bodies()[0].solid);
        assert!((v - 320.0).abs() < 1e-6, "{art:?}: {v}");
        d.delete(f).unwrap();
    }
}

#[test]
fn a_variable_fillet_feature_computes_and_passes_the_volume_probe() {
    // Task 14: Prozent → Anteil; Gesetz linear 1 → 2 längs der Deckelkante (Länge 10), also
    // abgetragen (1 − π/4) · 10 · 7/3. Status Ok heißt auch: Volumenprobe des Features bestanden.
    let (mut d, b) = with_box(10.0);
    let r = edge_ref(&d, b, "Deckel", "Wand 1");
    let f = d
        .add_feature(
            "Rundung 1",
            fillet(
                b,
                vec![r],
                BlendArt::RundungVariabel {
                    punkte: vec![(0.0, 1.0), (100.0, 2.0)],
                },
            ),
        )
        .unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    let want = 1000.0 - (1.0 - PI / 4.0) * 10.0 * 7.0 / 3.0;
    let v = volume(d.visible_bodies()[0].solid);
    assert!((v - want).abs() < 0.05, "{v} statt {want}");
    // Ein Radius unter dem Mindestmaß der Hautflächen (2e-3 mm) wird benannt.
    d.update(f, |k| {
        if let FeatureKind::Verrunden { art, .. } = k {
            *art = BlendArt::RundungVariabel {
                punkte: vec![(0.0, 1.0), (100.0, 1e-3)],
            };
        }
    })
    .unwrap();
    match status(&d, f) {
        FeatureStatus::Error(e) => assert!(e.contains("zu klein"), "{e}"),
        other => panic!("{other:?}"),
    }
    d.check_invariants().unwrap();
}
