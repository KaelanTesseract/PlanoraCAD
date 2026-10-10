use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::{DVec2, DVec3};
use planora_project::*;

/// Dokument mit allen Feature-Arten: Skizze auf Grundebene, Extrusion, Rotation, Skizze auf
/// einer Fläche mit Kantenkopie, Quader, Zylinder, Kugel, Kegel, Boolesch, Verschieben.
fn full_document() -> Document {
    let mut d = Document::new();
    let b = d
        .add_feature(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::new(30.0, 20.0, 10.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let cyl = d
        .add_feature(
            "Zylinder 1",
            FeatureKind::Cylinder {
                radius: 4.0,
                height: 14.0,
                origin: DVec3::new(10.0, 10.0, -2.0),
            },
        )
        .unwrap();
    d.add_feature(
        "Kugel 1",
        FeatureKind::Sphere {
            radius: 3.0,
            origin: DVec3::new(50.0, 0.0, 0.0),
        },
    )
    .unwrap();
    d.add_feature(
        "Kegel 1",
        FeatureKind::Cone {
            radius: 3.0,
            height: 6.0,
            origin: DVec3::new(60.0, 0.0, 0.0),
        },
    )
    .unwrap();
    let diff = d
        .add_feature(
            "Abzug",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: b,
                    index: 0,
                },
                tool: BodyRef {
                    feature: cyl,
                    index: 0,
                },
                op: BoolOp::Difference,
            },
        )
        .unwrap();
    d.add_feature(
        "Weg",
        FeatureKind::Move {
            body: BodyRef {
                feature: diff,
                index: 0,
            },
            delta: DVec3::new(0.0, 0.0, 5.0),
        },
    )
    .unwrap();
    // Skizze + Extrusion
    let mut sk =
        planora_sketch::Sketch::new(StandardPlane::XY.frame(), "Skizze 1", Default::default());
    sk.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(8.0, 6.0))
        .unwrap();
    let sid = d
        .add_feature(
            "Skizze 1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
            }),
        )
        .unwrap();
    d.add_feature(
        "Extrusion 1",
        FeatureKind::Extrude {
            sketch: sid,
            profile: 0,
            height: 4.0,
        },
    )
    .unwrap();
    d
}

#[test]
fn the_round_trip_keeps_structure_values_and_results() {
    let d = full_document();
    d.check_invariants().unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    let back = decode_version(&text, "test").unwrap();
    let e = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(d.features().len(), e.features().len());
    for (a, b) in d.features().iter().zip(e.features()) {
        assert_eq!(
            (&a.name, a.id, a.suppressed, &a.status),
            (&b.name, b.id, b.suppressed, &b.status)
        );
        assert_eq!(a.kind.label(), b.kind.label());
    }
    // zweiter Schreibvorgang ergibt denselben Text (stabiles Format)
    assert_eq!(text, encode_version(&e.snapshot()).unwrap());
}

#[test]
fn the_golden_file_pins_format_1() {
    let text = encode_version(&full_document().snapshot()).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/format1.json");
    if std::env::var("PLANORA_UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    let golden = std::fs::read_to_string(&path)
        .expect("tests/data/format1.json fehlt – mit PLANORA_UPDATE_GOLDEN=1 erzeugen");
    // Die Golden-Datei muss sich (a) fehlerfrei lesen lassen und (b) zum selben Text führen.
    let snap = decode_version(&golden, "format1.json").unwrap();
    assert_eq!(
        encode_version(&snap).unwrap(),
        golden.trim_end_matches('\n')
    );
    let doc = Document::from_snapshot(snap, 0.05).unwrap();
    doc.check_invariants().unwrap();
    assert_eq!(doc.features().len(), 8);
}

fn bad(text: &str) -> ProjectError {
    decode_version(text, "x.json").unwrap_err()
}

#[test]
fn damaged_and_unfit_files_give_errors_not_panics() {
    let good = encode_version(&full_document().snapshot()).unwrap();
    assert!(matches!(bad(""), ProjectError::Corrupt { .. }));
    assert!(matches!(bad("{"), ProjectError::Corrupt { .. }));
    assert!(matches!(
        bad(&good[..good.len() / 2]),
        ProjectError::Corrupt { .. }
    ));
    assert!(matches!(bad("[]"), ProjectError::Corrupt { .. }));
    assert!(matches!(
        bad(r#"{"format":99,"marker":0,"naechste_id":1,"features":[]}"#),
        ProjectError::TooNew { found: 99, .. }
    ));
    assert!(matches!(
        bad(r#"{"format":1,"marker":"x","naechste_id":1,"features":[]}"#),
        ProjectError::Corrupt { .. }
    ));
    // Zahl zu groß
    let huge = good.replacen("30.0", "1e300", 1);
    assert!(matches!(bad(&huge), ProjectError::Corrupt { .. }));
    // unbekannte Operation
    assert!(matches!(
        bad(&good.replace("abziehen", "teilen")),
        ProjectError::Corrupt { .. }
    ));
    // unbekannte Felder werden ignoriert
    let extra = good.replacen("\"marker\"", "\"zukunft\": 1,\n  \"marker\"", 1);
    decode_version(&extra, "x.json").unwrap();
}

#[test]
fn a_decoded_but_inconsistent_snapshot_is_rejected_by_the_document() {
    let good = encode_version(&full_document().snapshot()).unwrap();
    // Marker hinter dem Ende
    let s = decode_version(&good.replacen("\"marker\": 8", "\"marker\": 80", 1), "x").unwrap();
    assert!(Document::from_snapshot(s, 0.05).is_err());
}

#[test]
fn a_non_finite_number_is_never_written() {
    let mut s = full_document().snapshot();
    if let FeatureKind::Box { size, .. } = &mut s.features[0].kind {
        size.x = f64::NAN;
    }
    assert!(encode_version(&s).is_err());
}
