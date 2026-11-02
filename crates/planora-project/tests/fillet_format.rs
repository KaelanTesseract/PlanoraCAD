//! Rundung und Fase (Skizzen 4, Task 2) laufen über vorhandene Kurven und Abhängigkeiten: kein
//! neuer Formatzweig, die Formatnummer bleibt, wie sie vor der Rundung war.

use planora_feature::*;
use planora_math::{DVec2, Tolerance};
use planora_project::{decode_version, encode_version, required_format};
use planora_sketch::fillet::{ChamferKind, CurvePick};
use planora_sketch::{CurveId, Sketch};

fn corner() -> (Sketch, CurveId, CurveId) {
    let mut sk = Sketch::new(StandardPlane::XY.frame(), "Skizze 1", Tolerance::default());
    let a = sk.add_point(40.0, 0.0).unwrap();
    let p = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(0.0, 30.0).unwrap();
    let l1 = sk.add_line(a, p).unwrap();
    let l2 = sk.add_line(p, b).unwrap();
    (sk, l1, l2)
}

fn picks(l1: CurveId, l2: CurveId) -> (CurvePick, CurvePick) {
    (
        CurvePick {
            curve: l1,
            at: DVec2::new(20.0, 0.0),
        },
        CurvePick {
            curve: l2,
            at: DVec2::new(0.0, 15.0),
        },
    )
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

fn format_of(sk: &Sketch) -> u32 {
    required_format(&document(sk).snapshot())
}

/// Schreiben und Laden geben dieselbe Skizze und denselben Text.
fn assert_round_trip(sk: &Sketch) {
    let d = document(sk);
    let text = encode_version(&d.snapshot()).unwrap();
    let back = Document::from_snapshot(decode_version(&text, "x.json").unwrap(), 0.05).unwrap();
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
    let FeatureKind::Sketch(sf) = &back.features()[0].kind else {
        panic!("keine Skizze")
    };
    assert_eq!(sf.sketch.to_data(), sk.to_data());
}

#[test]
fn a_fillet_needs_no_new_format_and_the_copy_brings_everything_back() {
    let (mut sk, l1, l2) = corner();
    let copy = sk.clone();
    let format = format_of(&sk);
    let (a, b) = picks(l1, l2);
    let plan = sk.fillet_plan(a, b, 5.0).unwrap();
    sk.add_fillet(&plan, None).unwrap();
    assert_eq!(format_of(&sk), format);
    assert_round_trip(&sk);
    let sk = copy.clone();
    assert_eq!(sk.to_data(), copy.to_data());
    assert_eq!(format_of(&sk), format);
    assert_round_trip(&sk);
}

#[test]
fn a_chamfer_needs_no_new_format() {
    for kind in [
        ChamferKind::Equal(5.0),
        ChamferKind::TwoDistances(5.0, 6.0),
        ChamferKind::DistanceAngle(5.0, 0.6),
    ] {
        let (mut sk, l1, l2) = corner();
        let format = format_of(&sk);
        let (a, b) = picks(l1, l2);
        let plan = sk.chamfer_plan(a, b, kind).unwrap();
        sk.add_chamfer(&plan).unwrap();
        assert_eq!(format_of(&sk), format);
        assert_round_trip(&sk);
    }
}
