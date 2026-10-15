//! Dateiformat 2: Golden-Datei mit Rundung, variabler Rundung und Fasen; Format 1 bleibt;
//! feindliche Dateien (Teilprojekt 8, Task 18).

use planora_feature::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_math::{DVec3, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};

fn blend_document() -> Document {
    let mut d = Document::new();
    let arts = [
        BlendArt::Rundung { radius: 1.5 },
        BlendArt::RundungVariabel {
            punkte: vec![(0.0, 1.0), (50.0, 1.5), (100.0, 1.2)],
        },
        BlendArt::FaseZwei {
            abstand1: 1.0,
            abstand2: 2.0,
        },
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 30.0,
        },
    ];
    for (i, art) in arts.into_iter().enumerate() {
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
        let name = format!("{} {}", art.label(), i + 1);
        d.add_feature(
            name,
            FeatureKind::Verrunden {
                body,
                edges: vec![r],
                art,
                tangentenkette: i % 2 == 0,
            },
        )
        .unwrap();
    }
    d
}

fn volumes(d: &Document) -> Vec<f64> {
    d.visible_bodies()
        .iter()
        .map(|v| {
            mass_properties(
                &tessellate(
                    v.solid,
                    &TessellationOptions {
                        chord_tolerance: 1e-3,
                    },
                )
                .unwrap(),
            )
            .volume
        })
        .collect()
}

fn bad(text: &str) -> ProjectError {
    decode_version(text, "x.json").unwrap_err()
}

#[test]
fn the_golden_file_pins_format_2() {
    let d = blend_document();
    assert!(
        d.features().iter().all(|f| f.status == FeatureStatus::Ok),
        "alle Features berechenbar"
    );
    let text = encode_version(&d.snapshot()).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/format2.json");
    if std::env::var("PLANORA_UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, &text).unwrap();
    }
    let golden = std::fs::read_to_string(&path)
        .expect("tests/data/format2.json fehlt – mit PLANORA_UPDATE_GOLDEN=1 erzeugen");
    assert_eq!(text, golden.trim_end_matches('\n'));
    assert!(golden.contains("\"format\": 2"));
    let back =
        Document::from_snapshot(decode_version(&golden, "format2.json").unwrap(), 0.05).unwrap();
    back.check_invariants().unwrap();
    let (a, b) = (volumes(&d), volumes(&back));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert!((x - y).abs() < 1e-9 * x.abs(), "{x} / {y}");
    }
}

#[test]
fn documents_without_blends_still_write_format_1() {
    let mut d = Document::new();
    d.add_feature(
        "Quader 1",
        FeatureKind::Box {
            size: DVec3::ONE,
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    assert_eq!(required_format(&d.snapshot()), 1);
    assert!(
        encode_version(&d.snapshot())
            .unwrap()
            .contains("\"format\": 1")
    );
    assert_eq!(required_format(&blend_document().snapshot()), 2);
    assert_eq!(FORMAT_VERSION, 5); // seit Teilprojekt 12 (Maße, Mittellinien)
}

#[test]
fn a_format_1_file_with_a_blend_is_corrupt_and_format_3_is_too_new() {
    let good = encode_version(&blend_document().snapshot()).unwrap();
    match bad(&good.replacen("\"format\": 2", "\"format\": 1", 1)) {
        ProjectError::Corrupt { .. } => {}
        other => panic!("{other:?}"),
    }
    let e = bad(&good.replacen("\"format\": 2", "\"format\": 1", 1)).to_string();
    assert!(e.contains("Dateiformat 2"), "{e}");
    // Seit Teilprojekt 12 ist Format 5 gültig; „zu neu“ ist jetzt 6.
    assert!(matches!(
        bad(&good.replacen("\"format\": 2", "\"format\": 6", 1)),
        ProjectError::TooNew { found: 6, .. }
    ));
}

#[test]
fn hostile_blend_files_are_rejected_or_named() {
    let good = encode_version(&blend_document().snapshot()).unwrap();
    // Ändert die Art der Verrundung mit dem Typ `typ` im JSON-Baum.
    let edit = |typ: &str, f: &dyn Fn(&mut serde_json::Value)| -> String {
        let mut v: serde_json::Value = serde_json::from_str(&good).unwrap();
        let features = v["features"].as_array_mut().unwrap();
        let blend = features
            .iter_mut()
            .find(|x| x["art"]["typ"] == "verrunden" && x["art"]["art"]["typ"] == typ)
            .unwrap();
        f(&mut blend["art"]);
        serde_json::to_string_pretty(&v).unwrap()
    };
    // Beim Laden abgelehnt: riesige Zahl, NaN (kein gültiges JSON), null, unbekannte Art,
    // leere Kantenliste.
    for text in [
        edit("rundung", &|a| {
            a["art"]["radius"] = serde_json::json!(1e300)
        }),
        good.replacen("\"radius\": 1.5", "\"radius\": NaN", 1),
        edit("fase_zwei", &|a| {
            a["art"]["abstand1"] = serde_json::Value::Null
        }),
        edit("fase_zwei", &|a| {
            a["art"]["typ"] = serde_json::json!("fase_drei")
        }),
        edit("rundung", &|a| a["kanten"] = serde_json::json!([])),
    ] {
        assert_ne!(text, good, "Änderung muss greifen");
        assert!(matches!(bad(&text), ProjectError::Corrupt { .. }), "{text}");
    }
    // Geladen, aber beim Berechnen benannt gemeldet: negativer Radius, Winkel 0, unsortierte
    // Punkte.
    for text in [
        edit("rundung", &|a| a["art"]["radius"] = serde_json::json!(-1.5)),
        edit("fase_winkel", &|a| {
            a["art"]["winkel"] = serde_json::json!(0.0)
        }),
        edit("rundung_variabel", &|a| {
            a["art"]["punkte"][1][0] = serde_json::json!(150.0)
        }),
    ] {
        let d = Document::from_snapshot(decode_version(&text, "x.json").unwrap(), 0.05).unwrap();
        d.check_invariants().unwrap();
        assert!(
            d.features()
                .iter()
                .any(|f| matches!(f.status, FeatureStatus::Error(_))),
            "ein Feature meldet den Fehler"
        );
    }
}

#[test]
fn files_with_too_many_edges_or_points_are_named_quickly() {
    // Korrekturwelle (Minor c): 300 bzw. 20 000 gleiche Kanten und 100 Radienpunkte laden, das
    // Feature meldet die Grenze sofort (kein Suchen jeder Kante, kein Hänger).
    let good = encode_version(&blend_document().snapshot()).unwrap();
    let edit = |typ: &str, f: &dyn Fn(&mut serde_json::Value)| -> String {
        let mut v: serde_json::Value = serde_json::from_str(&good).unwrap();
        let features = v["features"].as_array_mut().unwrap();
        let blend = features
            .iter_mut()
            .find(|x| x["art"]["typ"] == "verrunden" && x["art"]["art"]["typ"] == typ)
            .unwrap();
        f(&mut blend["art"]);
        serde_json::to_string_pretty(&v).unwrap()
    };
    let repeat = |n: usize| {
        move |a: &mut serde_json::Value| {
            let one = a["kanten"][0].clone();
            a["kanten"] = serde_json::Value::Array(vec![one; n]);
        }
    };
    let points = |a: &mut serde_json::Value| {
        let p: Vec<serde_json::Value> = (0..100)
            .map(|i| serde_json::json!([i as f64, 1.0]))
            .collect();
        a["art"]["punkte"] = serde_json::Value::Array(p);
    };
    for (text, want) in [
        (edit("rundung", &repeat(300)), "Zu viele Kanten"),
        (edit("rundung", &repeat(20_000)), "Zu viele Kanten"),
        (edit("rundung_variabel", &points), "2 bis 64 Punkte"),
    ] {
        let d = Document::from_snapshot(decode_version(&text, "x.json").unwrap(), 0.05).unwrap();
        d.check_invariants().unwrap();
        assert!(
            d.features()
                .iter()
                .any(|f| matches!(&f.status, FeatureStatus::Error(e) if e.contains(want))),
            "{want}"
        );
    }
}
