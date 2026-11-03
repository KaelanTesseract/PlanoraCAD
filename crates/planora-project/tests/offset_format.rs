//! Der Versatz (Skizzen 4, Task 4) läuft über vorhandene Kurven und Abhängigkeiten: kein neuer
//! Formatzweig, die Formatnummer bleibt, wie sie vor dem Versatz war.

use planora_feature::*;
use planora_math::Tolerance;
use planora_project::{decode_version, encode_version, required_format};
use planora_sketch::{CurveId, Sketch};

fn rounded_corner() -> (Sketch, Vec<CurveId>) {
    let mut sk = Sketch::new(StandardPlane::XY.frame(), "Skizze 1", Tolerance::default());
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(30.0, 0.0).unwrap();
    let m = sk.add_point(30.0, 10.0).unwrap();
    let c = sk.add_point(40.0, 10.0).unwrap();
    let d = sk.add_point(40.0, 40.0).unwrap();
    let curves = vec![
        sk.add_line(a, b).unwrap(),
        sk.add_arc(m, b, c).unwrap(),
        sk.add_line(c, d).unwrap(),
    ];
    (sk, curves)
}

fn document(sk: &Sketch) -> Document {
    let mut d = Document::new();
    d.add_feature(
        "Skizze 1",
        FeatureKind::Sketch(SketchFeature {
            sketch: sk.clone(),
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas: Default::default(),
        }),
    )
    .unwrap();
    d
}

#[test]
fn an_offset_needs_no_new_format_and_comes_back_unchanged() {
    let (mut sk, curves) = rounded_corner();
    let format = required_format(&document(&sk).snapshot());
    let chains = sk.ordered_chains(&curves).unwrap();
    let plan = sk.offset_plan(&chains, 3.0).unwrap();
    let ids = sk.add_offset(&plan).unwrap();
    assert!(ids.tied);
    assert_eq!(required_format(&document(&sk).snapshot()), format);

    let text = encode_version(&document(&sk).snapshot()).unwrap();
    let back = Document::from_snapshot(decode_version(&text, "x.json").unwrap(), 0.05).unwrap();
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
    let FeatureKind::Sketch(sf) = &back.features()[0].kind else {
        panic!("keine Skizze")
    };
    assert_eq!(sf.sketch.to_data(), sk.to_data());
}
