//! Dateiformat 13 (Teilprojekt Skizzen 1, Task 2): Kurvenart Ellipsenbogen. Nur eine Skizze mit
//! einem Ellipsenbogen (auch einem gelöschten) hebt die Formatnummer auf 13.

use planora_feature::*;
use planora_math::{DVec2, Tolerance};
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::{CurveKind, Sketch};

/// Skizze mit Viertelellipse von (10, 0) nach (0, 5) und der Sehne zurück, dazu eine Extrusion.
/// Bei `remove_arc` wird der Bogen wieder gelöscht (der Eintrag bleibt in der Datei).
fn document(with_arc: bool, remove_arc: bool) -> Document {
    let mut sk = Sketch::new(StandardPlane::XY.frame(), "Skizze 1", Tolerance::default());
    let c = sk.add_point(0.0, 0.0).unwrap();
    let a = sk.add_point(10.0, 0.0).unwrap();
    let b = sk.add_point(0.0, 5.0).unwrap();
    if with_arc {
        let e = sk
            .add_elliptic_arc(c, DVec2::new(10.0, 0.0), 5.0, a, b)
            .unwrap();
        if remove_arc {
            sk.remove_curve(e).unwrap();
        }
    }
    sk.add_line(b, a).unwrap();
    let mut d = Document::new();
    let sid = d
        .add_feature(
            "Skizze 1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    if with_arc && !remove_arc {
        d.add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: sid,
                profiles: ProfileSel::Index(0),
                height: 4.0,
            },
        )
        .unwrap();
    }
    d
}

fn load(text: &str) -> Result<Document, ProjectError> {
    let s = decode_version(text, "x.json")?;
    Document::from_snapshot(s, 0.05).map_err(|e| ProjectError::Invalid(e.to_string()))
}

#[test]
fn format_version_is_13() {
    assert_eq!(FORMAT_VERSION, 14);
}

#[test]
fn a_sketch_with_an_elliptic_arc_writes_format_13_and_loads_the_same() {
    let d = document(true, false);
    let s = d.snapshot();
    assert_eq!(required_format(&s), 13);
    let text = encode_version(&s).unwrap();
    assert!(text.contains("\"format\": 13"), "{text}");
    assert!(text.contains("\"typ\": \"ellipsenbogen\""), "{text}");
    assert!(text.contains("\"typ\": \"ellipsenbogen_enden\""), "{text}");
    let back = load(&text).unwrap();
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
    let FeatureKind::Sketch(sf) = &back.features()[0].kind else {
        panic!("keine Skizze")
    };
    assert_eq!(
        sf.sketch
            .curves()
            .filter(|(_, c)| matches!(c.kind, CurveKind::EllipticArc { .. }))
            .count(),
        1
    );
    let FeatureKind::Sketch(orig) = &d.features()[0].kind else {
        unreachable!()
    };
    assert_eq!(sf.sketch.to_data(), orig.sketch.to_data());
}

#[test]
fn a_deleted_elliptic_arc_still_needs_format_13() {
    let s = document(true, true).snapshot();
    assert_eq!(required_format(&s), 13);
    let text = encode_version(&s).unwrap();
    assert!(text.contains("\"format\": 13"), "{text}");
    load(&text).unwrap();
}

#[test]
fn without_an_elliptic_arc_the_file_keeps_its_format() {
    let s = document(false, false).snapshot();
    assert_eq!(required_format(&s), 1);
    let text = encode_version(&s).unwrap();
    assert!(text.contains("\"format\": 1,"), "{text}");
}

#[test]
fn format_15_is_too_new() {
    let text = encode_version(&document(true, false).snapshot()).unwrap();
    let e = load(&text.replacen("\"format\": 13", "\"format\": 15", 1)).unwrap_err();
    assert!(matches!(e, ProjectError::TooNew { found: 15, .. }), "{e:?}");
}
