//! Dateiformat 14 (Teilprojekt Skizzen 3, Task 4): Mittellinie als dritter Linienstil. Nur eine
//! Skizze mit einer Mittellinie (auch einer gelöschten) hebt die Formatnummer auf 14; ohne
//! Mittellinie bleibt der Text bytegleich wie bisher.

use planora_feature::*;
use planora_math::Tolerance;
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::{LineStyle, Sketch};

/// Skizze mit einer Linie. `style`: 0 normal, 1 Konstruktion, 2 Mittellinie; `remove` löscht
/// die Linie wieder (der Eintrag bleibt in der Datei).
fn document(style: u8, remove: bool) -> Document {
    let mut sk = Sketch::new(StandardPlane::XY.frame(), "Skizze 1", Tolerance::default());
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    let l = sk.add_line(a, b).unwrap();
    match style {
        1 => sk.set_construction(l, true).unwrap(),
        2 => sk.set_centerline(l, true).unwrap(),
        _ => {}
    }
    if remove {
        sk.remove_curve(l).unwrap();
    }
    let mut d = Document::new();
    d.add_feature(
        "Skizze 1",
        FeatureKind::Sketch(SketchFeature {
            sketch: sk,
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas: Default::default(),
        }),
    )
    .unwrap();
    d
}

fn load(text: &str) -> Result<Document, ProjectError> {
    let s = decode_version(text, "x.json")?;
    Document::from_snapshot(s, 0.05).map_err(|e| ProjectError::Invalid(e.to_string()))
}

#[test]
fn format_version_is_15() {
    assert_eq!(FORMAT_VERSION, 15);
}

#[test]
fn a_centerline_writes_format_14_and_loads_the_same() {
    let d = document(2, false);
    let s = d.snapshot();
    assert_eq!(required_format(&s), 14);
    let text = encode_version(&s).unwrap();
    assert!(text.contains("\"format\": 14"), "{text}");
    assert!(text.contains("\"mittellinie\": true"), "{text}");
    let back = load(&text).unwrap();
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
    let FeatureKind::Sketch(sf) = &back.features()[0].kind else {
        panic!("keine Skizze")
    };
    assert!(
        sf.sketch
            .curves()
            .any(|(_, c)| c.style() == LineStyle::Centerline)
    );
    let FeatureKind::Sketch(orig) = &d.features()[0].kind else {
        unreachable!()
    };
    assert_eq!(sf.sketch.to_data(), orig.sketch.to_data());
}

#[test]
fn a_deleted_centerline_still_needs_format_14() {
    let s = document(2, true).snapshot();
    assert_eq!(required_format(&s), 14);
    let text = encode_version(&s).unwrap();
    assert!(text.contains("\"format\": 14"), "{text}");
    load(&text).unwrap();
}

#[test]
fn without_a_centerline_the_file_keeps_its_format_and_bytes() {
    for style in [0, 1] {
        let s = document(style, false).snapshot();
        assert_eq!(required_format(&s), 1, "style {style}");
        let text = encode_version(&s).unwrap();
        assert!(text.contains("\"format\": 1,"), "{text}");
        assert!(!text.contains("mittellinie"), "{text}");
    }
}

#[test]
fn a_construction_line_alone_does_not_raise_the_format() {
    assert_eq!(
        required_format(&document(1, false).snapshot()),
        required_format(&document(0, false).snapshot())
    );
}

#[test]
fn a_centerline_without_construction_in_the_file_is_rejected() {
    let text = encode_version(&document(2, false).snapshot()).unwrap();
    // Die Zeile "konstruktion" der Linie steht vor "mittellinie".
    let broken = text.replacen("\"konstruktion\": true,\n", "\"konstruktion\": false,\n", 2);
    assert_ne!(broken, text);
    assert!(load(&broken).is_err());
}

#[test]
fn format_16_is_too_new() {
    let text = encode_version(&document(2, false).snapshot()).unwrap();
    let e = load(&text.replacen("\"format\": 14", "\"format\": 16", 1)).unwrap_err();
    assert!(matches!(e, ProjectError::TooNew { found: 16, .. }), "{e:?}");
}
