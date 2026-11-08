//! Dateiformat 17 (Teilprojekt Skizzen 7): Nur ein Dokument mit einer neuen Kurvenart wird mit
//! Format 17 geschrieben, auch wenn der Eintrag gelöscht ist; alle anderen bleiben bytegleich.

use planora_feature::*;
use planora_math::{Frame, Tolerance};
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::{CurveId, CurveKind, Sketch};

fn format_of(text: &str) -> u64 {
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    v["format"].as_u64().unwrap()
}

fn control_sketch() -> (Sketch, CurveId) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let pts: Vec<_> = [
        (0.0, 0.0),
        (2.0, 5.0),
        (5.0, 7.0),
        (9.0, 6.0),
        (12.0, 3.0),
        (14.0, 0.0),
    ]
    .iter()
    .map(|&(x, y)| sk.add_point(x, y).unwrap())
    .collect();
    let c = sk.add_control_spline(&pts, 3).unwrap();
    (sk, c)
}

fn document(sketch: Sketch) -> Document {
    let mut d = Document::new();
    d.add_feature(
        "Skizze1",
        FeatureKind::Sketch(SketchFeature {
            sketch,
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas: Default::default(),
        }),
    )
    .unwrap();
    d
}

fn sketch_of(s: &Snapshot) -> &Sketch {
    let FeatureKind::Sketch(sf) = &s.features[0].kind else {
        panic!("keine Skizze");
    };
    &sf.sketch
}

#[test]
fn format_version_is_17() {
    assert_eq!(FORMAT_VERSION, 17);
}

#[test]
fn control_spline_needs_17() {
    let (sk, _) = control_sketch();
    let snap = document(sk).snapshot();
    assert_eq!(required_format(&snap), 17);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 17);
    assert!(text.contains("\"typ\": \"kontroll_spline\""), "{text}");
    assert!(text.contains("\"grad\": 3"), "{text}");
    assert!(text.contains("\"knoten\""), "{text}");
}

#[test]
fn deleted_control_spline_needs_17() {
    let (mut sk, c) = control_sketch();
    sk.remove_curve(c).unwrap();
    assert!(sk.curve(c).is_none());
    let snap = document(sk).snapshot();
    assert_eq!(required_format(&snap), 17);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 17);
    // Der gelöschte Eintrag steht in der Datei und überlebt den Rundlauf.
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(encode_version(&back).unwrap(), text);
}

#[test]
fn fit_spline_and_plain_sketch_stay_below_17() {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 5.0).unwrap();
    let c = sk.add_point(10.0, 0.0).unwrap();
    sk.add_spline(&[a, b, c]).unwrap();
    assert!(required_format(&document(sk).snapshot()) < 17);
}

#[test]
fn roundtrip_keeps_it() {
    let (sk, c) = control_sketch();
    let snap = document(sk.clone()).snapshot();
    let text = encode_version(&snap).unwrap();
    let back = decode_version(&text, "x.json").unwrap();
    let got = sketch_of(&back);
    assert_eq!(got.curve(c).unwrap().kind, sk.curve(c).unwrap().kind);
    let CurveKind::ControlSpline { degree, knots, .. } = &got.curve(c).unwrap().kind else {
        panic!("falsche Art");
    };
    assert_eq!(*degree, 3);
    assert_eq!(knots, &[0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 3.0]);
    assert_eq!(encode_version(&back).unwrap(), text);
    let doc = Document::from_snapshot(back, 0.05).unwrap();
    let again = doc.snapshot();
    let t = 1.37;
    let (p, q) = (
        sk.curve_eval(c, t).unwrap(),
        sketch_of(&again).curve_eval(c, t).unwrap(),
    );
    assert_eq!(p, q);
}

/// Datei mit Kontrollpunkt-Spline, geändert durch `f`.
fn edited(f: impl FnOnce(&mut serde_json::Value)) -> String {
    let (sk, _) = control_sketch();
    let text = encode_version(&document(sk).snapshot()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

#[test]
fn an_older_format_number_with_it_is_corrupt() {
    let text = edited(|v| v["format"] = 16.into());
    assert!(matches!(
        decode_version(&text, "x.json"),
        Err(ProjectError::Corrupt { .. })
    ));
}

#[test]
fn bad_knots_in_the_file_are_refused() {
    fn knots(v: &mut serde_json::Value) -> &mut serde_json::Value {
        let curves = v["features"][0]["art"]["skizze"]["kurven"]
            .as_array_mut()
            .unwrap();
        let spline = curves
            .iter_mut()
            .find(|c| c["art"]["typ"] == "kontroll_spline")
            .unwrap();
        &mut spline["art"]["knoten"]
    }
    let text = edited(|v| {
        knots(v).as_array_mut().unwrap().pop();
    });
    assert!(decode_version(&text, "x.json").is_err());
    let text = edited(|v| *knots(v) = serde_json::json!([3, 2, 1, 0, 0, 0, 0, 0, 0, 0]));
    assert!(decode_version(&text, "x.json").is_err());
}

#[test]
fn old_files_stay_byte_equal() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut seen = 0;
    for dir in ["data", "fixtures"] {
        let mut entries: Vec<_> = std::fs::read_dir(root.join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        entries.sort();
        for p in entries {
            let name = p.display().to_string();
            let raw = std::fs::read_to_string(&p).unwrap();
            let text = raw.trim_end_matches('\n');
            let snap = decode_version(text, &name).unwrap_or_else(|e| panic!("{name}: {e}"));
            let back = encode_version(&snap).unwrap();
            assert_eq!(back, text, "{name}");
            assert!(format_of(&back) < 17, "{name}");
            seen += 1;
        }
    }
    assert!(seen >= 12, "{seen}");
}

// ---- Kegelschnitt (Task 5) ------------------------------------------------------------------

fn conic_sketch() -> (Sketch, CurveId) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let a = sk.add_point(0.0, 0.0).unwrap();
    let m = sk.add_point(5.0, 8.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    let c = sk.add_conic(a, b, m, 0.35).unwrap();
    (sk, c)
}

/// Spline mit einem Griff am mittleren Stützpunkt; die Griff-Linie und die Spitze sind dabei.
fn handled_sketch() -> (Sketch, CurveId, CurveId, planora_sketch::PointId) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let pts: Vec<_> = [(0.0, 0.0), (10.0, 6.0), (20.0, -2.0), (30.0, 3.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    let c = sk.add_spline(&pts).unwrap();
    let line = sk.activate_tangent_handle(c, pts[1]).unwrap();
    (sk, c, line, pts[1])
}

#[test]
fn handles_need_17_and_roundtrip() {
    let (sk, c, line, _) = handled_sketch();
    let snap = document(sk.clone()).snapshot();
    assert_eq!(required_format(&snap), 17);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 17);
    assert!(text.contains("\"griffe\""), "{text}");
    assert!(text.contains("\"spitze\""), "{text}");
    // Griff mit Form: keine Angabe zur Krümmung in der Datei
    assert!(!text.contains("\"kruemmung\""), "{text}");
    let back = decode_version(&text, "x.json").unwrap();
    let got = sketch_of(&back);
    assert_eq!(got.curve(c).unwrap().kind, sk.curve(c).unwrap().kind);
    assert!(got.curve(line).unwrap().construction);
    assert_eq!(encode_version(&back).unwrap(), text);
    assert_eq!(got.curve_eval(c, 1.3), sk.curve_eval(c, 1.3));
    // Kein Griff in einer älteren Formatnummer.
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["format"] = 16.into();
    let old = serde_json::to_string_pretty(&v).unwrap();
    assert!(matches!(
        decode_version(&old, "x.json"),
        Err(ProjectError::Corrupt { .. })
    ));
}

#[test]
fn without_handles_the_spline_needs_no_17_and_no_griffe_field() {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let pts: Vec<_> = [(0.0, 0.0), (10.0, 6.0), (20.0, -2.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    sk.add_spline(&pts).unwrap();
    let snap = document(sk).snapshot();
    assert!(required_format(&snap) < 17);
    let text = encode_version(&snap).unwrap();
    assert!(!text.contains("griffe"), "{text}");
}

#[test]
fn a_switched_off_handle_needs_no_17_any_more() {
    let (mut sk, c, _, at) = handled_sketch();
    sk.deactivate_handle(c, at).unwrap();
    assert!(required_format(&document(sk).snapshot()) < 17);
}

#[test]
fn arc_handle_roundtrips() {
    let (sk, c, _, at) = handled_sketch();
    let mut sk = sk;
    let center = sk.add_point(10.0, 16.0).unwrap();
    let other = sk.add_point(20.0, 16.0).unwrap();
    let arc = sk.add_arc(center, at, other).unwrap();
    sk.set_construction(arc, true).unwrap();
    let mut d = sk.to_data();
    let CurveKind::Spline { handles, .. } = &mut d.curves[c.index()].kind else {
        panic!("kein Spline");
    };
    handles[0].curvature = planora_sketch::HandleCurvature::Arc { arc, center, other };
    let sk = Sketch::from_data(d).unwrap();
    let text = encode_version(&document(sk.clone()).snapshot()).unwrap();
    assert!(text.contains("\"kruemmung\""), "{text}");
    assert!(text.contains("\"typ\": \"bogen\""), "{text}");
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(
        sketch_of(&back).curve(c).unwrap().kind,
        sk.curve(c).unwrap().kind
    );
    assert_eq!(encode_version(&back).unwrap(), text);
}

#[test]
fn bad_handle_in_the_file_is_refused() {
    let (sk, ..) = handled_sketch();
    let text = encode_version(&document(sk).snapshot()).unwrap();
    // Griff an einem Punkt, der kein Stützpunkt ist: an der Spitze selbst.
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let griffe = v["features"][0]["art"]["skizze"]["kurven"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|k| k["art"]["griffe"].is_array())
        .unwrap();
    let griff = &mut griffe["art"]["griffe"][0];
    griff["an"] = griff["spitze"].clone();
    let broken = serde_json::to_string_pretty(&v).unwrap();
    assert!(decode_version(&broken, "x.json").is_err());
}

#[test]
fn conic_needs_17_and_roundtrips() {
    let (sk, c) = conic_sketch();
    let snap = document(sk.clone()).snapshot();
    assert_eq!(required_format(&snap), 17);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 17);
    assert!(text.contains("\"typ\": \"kegelschnitt\""), "{text}");
    assert!(text.contains("\"scheitel\""), "{text}");
    assert!(text.contains("\"rho\": 0.35"), "{text}");
    let back = decode_version(&text, "x.json").unwrap();
    let got = sketch_of(&back);
    assert_eq!(got.curve(c).unwrap().kind, sk.curve(c).unwrap().kind);
    assert_eq!(encode_version(&back).unwrap(), text);
    assert_eq!(got.curve_eval(c, 0.3), sk.curve_eval(c, 0.3));
    // Kein Dokument mit Rho in einer älteren Formatnummer.
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["format"] = 16.into();
    let old = serde_json::to_string_pretty(&v).unwrap();
    assert!(matches!(
        decode_version(&old, "x.json"),
        Err(ProjectError::Corrupt { .. })
    ));
}

#[test]
fn deleted_conic_needs_17() {
    let (mut sk, c) = conic_sketch();
    sk.remove_curve(c).unwrap();
    let snap = document(sk).snapshot();
    assert_eq!(required_format(&snap), 17);
    let text = encode_version(&snap).unwrap();
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(encode_version(&back).unwrap(), text);
}

#[test]
fn bad_rho_in_the_file_is_refused() {
    let (sk, _) = conic_sketch();
    let text = encode_version(&document(sk).snapshot()).unwrap();
    for rho in [
        serde_json::json!(0.0),
        serde_json::json!(1.0),
        serde_json::json!(-0.1),
        serde_json::json!(2.5),
    ] {
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let curves = v["features"][0]["art"]["skizze"]["kurven"]
            .as_array_mut()
            .unwrap();
        let conic = curves
            .iter_mut()
            .find(|c| c["art"]["typ"] == "kegelschnitt")
            .unwrap();
        conic["art"]["rho"] = rho.clone();
        let edited = serde_json::to_string_pretty(&v).unwrap();
        assert!(decode_version(&edited, "x.json").is_err(), "{rho}");
    }
}
