//! Verrundungen überstehen Schreiben und Lesen (Teilprojekt 8, Task 7; Formatnummer in Task 18).

use planora_feature::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_math::{DVec3, Tolerance};
use planora_project::{decode_version, encode_version};

#[test]
fn every_blend_art_survives_a_round_trip() {
    let mut d = Document::new();
    let arts = [
        BlendArt::Rundung { radius: 1.5 },
        BlendArt::RundungVariabel {
            punkte: vec![(0.0, 1.0), (40.0, 1.5), (100.0, 2.0)],
        },
        BlendArt::FaseGleich { abstand: 1.0 },
        BlendArt::FaseZwei {
            abstand1: 1.0,
            abstand2: 2.0,
        },
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 30.0,
        },
    ];
    for (i, art) in arts.iter().enumerate() {
        let f = d
            .add_feature(
                format!("Quader {}", i + 1),
                FeatureKind::Box {
                    size: DVec3::splat(10.0),
                    origin: DVec3::new(20.0 * i as f64, 0.0, 0.0),
                },
            )
            .unwrap();
        let body = BodyRef {
            feature: f,
            index: 0,
        };
        let s = d.body(body).unwrap();
        let e = s
            .edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| {
                let n: Vec<String> = edge_uses(s, id)
                    .iter()
                    .map(|u| face_name(s, u.face))
                    .collect();
                n.contains(&"Deckel".to_string()) && n.contains(&"Wand 1".to_string())
            })
            .unwrap();
        let r = EdgeRef::from_edge(s, e, &Tolerance::default()).unwrap();
        d.add_feature(
            format!("{} {}", art.label(), i + 1),
            FeatureKind::Verrunden {
                body,
                edges: vec![r],
                art: art.clone(),
                tangentenkette: i % 2 == 0,
            },
        )
        .unwrap();
    }
    let text = encode_version(&d.snapshot()).unwrap();
    assert!(text.contains("\"verrunden\"") && text.contains("\"rundung_variabel\""));
    let back = decode_version(&text, "test").unwrap();
    let e = Document::from_snapshot(back, 0.05).unwrap();
    for (a, b) in d.features().iter().zip(e.features()) {
        match (&a.kind, &b.kind) {
            (
                FeatureKind::Verrunden {
                    art: x,
                    tangentenkette: tx,
                    edges: ex,
                    ..
                },
                FeatureKind::Verrunden {
                    art: y,
                    tangentenkette: ty,
                    edges: ey,
                    ..
                },
            ) => {
                assert_eq!((x, tx, ex), (y, ty, ey));
                assert_eq!(a.status, b.status);
            }
            (FeatureKind::Box { .. }, FeatureKind::Box { .. }) => {}
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(text, encode_version(&e.snapshot()).unwrap());
}
