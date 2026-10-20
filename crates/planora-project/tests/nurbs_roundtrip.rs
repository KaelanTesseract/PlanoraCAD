//! Speichern und Laden eines Dokuments mit Ellipsen-Skizze, Extrusion und Boolescher Operation
//! (Format 1, unverändert) sowie feindliche Dateien mit ungültigen Ellipsenwerten. Die Golden-
//! Dateien (`format1.json`, `format1_voll.json`) sichern, dass das Format selbst gleich bleibt.

use planora_feature::*;

use planora_kernel::{BoolOp, Kernel};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_project::*;
use planora_sketch::ConstraintKind as K;
use planora_sketch::model::Sketch;

fn body(feature: FeatureId) -> BodyRef {
    BodyRef { feature, index: 0 }
}

fn freeform_document() -> Document {
    let mut d = Document::new();
    let mut sk = Sketch::new(Frame::WORLD, "Ellipse", Tolerance::default());
    let c = sk.add_point(0.0, 0.0).unwrap();
    let e = sk.add_ellipse(c, DVec2::new(5.0, 0.0), 3.0).unwrap();
    sk.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    sk.add_constraint(K::EllipseAngle(e, 0.0)).unwrap();
    sk.add_constraint(K::EllipseMajor(e, 5.0)).unwrap();
    sk.add_constraint(K::EllipseMinor(e, 3.0)).unwrap();
    let s = d
        .add_feature(
            "Ellipse",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let ex = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 4.0,
            },
        )
        .unwrap();
    let b = d
        .add_feature(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::splat(10.0),
                origin: DVec3::new(4.0, -5.0, -3.0),
            },
        )
        .unwrap();
    d.add_feature(
        "Abzug 1",
        FeatureKind::Boolean {
            target: body(ex),
            tool: body(b),
            op: BoolOp::Difference,
        },
    )
    .unwrap();
    d
}

fn volume(s: &planora_kernel::Solid) -> f64 {
    mass_properties(
        &tessellate(
            s,
            &TessellationOptions {
                chord_tolerance: 1e-2,
            },
        )
        .unwrap(),
    )
    .volume
}

#[test]
fn a_freeform_document_round_trips_with_equal_volumes_and_statuses() {
    let d = freeform_document();
    d.check_invariants().unwrap();
    for f in d.features() {
        assert_eq!(f.status, FeatureStatus::Ok, "{}: {:?}", f.name, f.status);
    }
    let text = encode_version(&d.snapshot()).unwrap();
    assert!(text.contains("\"format\": 1"), "Format 1 genügt");
    let back = Document::from_snapshot(decode_version(&text, "test").unwrap(), 0.05).unwrap();
    back.check_invariants().unwrap();
    assert_eq!(d.features().len(), back.features().len());
    for (a, b) in d.features().iter().zip(back.features()) {
        assert_eq!((&a.name, a.id, &a.status), (&b.name, b.id, &b.status));
    }
    let (va, vb) = (d.visible_bodies(), back.visible_bodies());
    assert_eq!(va.len(), 1);
    assert_eq!(vb.len(), 1);
    let (x, y) = (volume(va[0].solid), volume(vb[0].solid));
    assert!(((x - y) / x).abs() < 1e-9, "{x} gegen {y}");
    assert_eq!(text, encode_version(&back.snapshot()).unwrap());
}

/// Feindliche Dateien: ein ungültiger Ellipsenwert (NaN, null, unendlich, Text) wird beim Laden
/// abgelehnt, nicht stillschweigend zu 0 oder zu einer kaputten Skizze.
#[test]
fn a_nan_ellipse_value_is_rejected_at_load() {
    let text = encode_version(&freeform_document().snapshot()).unwrap();
    let needle = "\"typ\": \"ellipse_klein\",\n";
    let at = text.find(needle).expect("Ellipsenmaß in der Datei");
    let value_at = at + text[at..].find("\"wert\": ").unwrap() + "\"wert\": ".len();
    let value_end = value_at + text[value_at..].find('\n').unwrap();
    assert_eq!(&text[value_at..value_end], "3.0");
    for bad in ["NaN", "null", "1e999", "\"x\"", "Infinity"] {
        let hostile = format!("{}{bad}{}", &text[..value_at], &text[value_end..]);
        let err = decode_version(&hostile, "feindlich.json")
            .expect_err(&format!("{bad} muss abgelehnt werden"));
        assert!(!err.to_string().is_empty());
    }
    // Auch eine NaN-Koordinate des Ellipsenmittelpunkts (Punkt) wird abgelehnt.
    let first_point = text.find("\"x\": ").expect("Punktkoordinate");
    let end = first_point + text[first_point..].find('\n').unwrap();
    let hostile = format!("{}\"x\": NaN,{}", &text[..first_point], &text[end..]);
    assert!(decode_version(&hostile, "feindlich.json").is_err());
}

/// Gültiges JSON, aber unsinnige Ellipsenmaße: Laden und Neuberechnen dürfen nicht abstürzen;
/// entweder lehnt das Laden die Datei ab, oder das Dokument wird neu berechnet, und jeder
/// sichtbare Körper ist gültig (sonst trägt das Feature einen Fehlerstatus statt eines Körpers).
/// Negative, null, winzige (1e-300) und riesige Werte lehnt schon das Laden ab; eine sehr flache
/// Ellipse (1e-5) und kleine = große Halbachse (5) kommen bis zur Neuberechnung.
#[test]
fn absurd_but_finite_ellipse_values_never_panic_and_never_produce_a_body() {
    let text = encode_version(&freeform_document().snapshot()).unwrap();
    let needle = "\"typ\": \"ellipse_klein\",\n";
    let at = text.find(needle).unwrap();
    let value_at = at + text[at..].find("\"wert\": ").unwrap() + "\"wert\": ".len();
    let value_end = value_at + text[value_at..].find('\n').unwrap();
    // Zählt die Werte, die bis zur Neuberechnung kommen: Würde jeder Wert schon beim Laden
    // abgelehnt, prüfte der Test die Neuberechnung gar nicht.
    let mut recomputed = 0;
    for bad in ["-3.0", "0.0", "1e300", "1e-300", "1e-5", "5.0"] {
        let hostile = format!("{}{bad}{}", &text[..value_at], &text[value_end..]);
        let Ok(snap) = decode_version(&hostile, "feindlich.json") else {
            continue;
        };
        let Ok(doc) = Document::from_snapshot(snap, 0.05) else {
            continue;
        };
        doc.check_invariants().unwrap();
        for b in doc.visible_bodies() {
            doc.kernel().validate(b.solid).unwrap();
        }
        recomputed += 1;
    }
    assert!(
        recomputed >= 2,
        "zu wenige Werte erreichten die Neuberechnung: {recomputed}"
    );
}
