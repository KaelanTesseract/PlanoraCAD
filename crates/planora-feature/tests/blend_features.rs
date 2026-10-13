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
    let arts = [
        BlendArt::Rundung { radius: f64::NAN },
        BlendArt::Rundung { radius: -1.0 },
        BlendArt::Rundung { radius: 1e300 },
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 0.0,
        },
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 180.0,
        },
        BlendArt::RundungVariabel {
            punkte: vec![(50.0, 1.0), (10.0, 1.0)],
        },
        BlendArt::RundungVariabel { punkte: vec![] },
    ];
    for art in arts {
        let f = d
            .add_feature("X", fillet(b, vec![r.clone()], art.clone()))
            .unwrap();
        assert!(matches!(status(&d, f), FeatureStatus::Error(_)), "{art:?}");
        // Ein Folgefeature auf dem Ergebnis wird übersprungen oder scheitert, rechnet aber nie.
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
        assert!(!matches!(status(&d, m), FeatureStatus::Ok), "{art:?}");
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
    match status(&d, f2) {
        FeatureStatus::Error(e) => assert!(e.contains("nicht mehr vorhanden"), "{e}"),
        s => panic!("{s:?}"),
    }
    // Unterdrücken des Quaders: beide Rundungen werden übersprungen, Dokument bleibt gültig.
    d.set_suppressed(b.feature, true).unwrap();
    assert!(matches!(status(&d, f1), FeatureStatus::Skipped(_)));
    d.set_suppressed(b.feature, false).unwrap();
    assert_eq!(status(&d, f1), FeatureStatus::Ok);
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
