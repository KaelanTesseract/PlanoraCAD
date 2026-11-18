//! Dateiformat 17 (Teilprojekt Skizzen 7): Nur ein Dokument mit einer neuen Kurvenart wird mit
//! Format 17 geschrieben, auch wenn der Eintrag gelöscht ist; alle anderen bleiben bytegleich.

use planora_feature::*;
use planora_math::{Frame, Tolerance};
use planora_project::{ProjectError, decode_version, encode_version, required_format};
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
            text_formulas: Default::default(),
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

// Skizzen 8: Die höchste Formatnummer prüft jetzt `format18_text.rs`.

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
    // Skizzen 7: Der Krümmungsgriff kommt jetzt vom Befehl, seine interne Zeile gehört dazu.
    let (sk, c, _, at) = handled_sketch();
    let mut sk = sk;
    sk.activate_curvature_handle(c, at).unwrap();
    let text = encode_version(&document(sk.clone()).snapshot()).unwrap();
    assert!(text.contains("\"kruemmung\""), "{text}");
    assert!(text.contains("\"typ\": \"bogen\""), "{text}");
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(
        sketch_of(&back).curve(c).unwrap().kind,
        sk.curve(c).unwrap().kind
    );
    assert_eq!(encode_version(&back).unwrap(), text);
    assert!(text.contains("\"typ\": \"kruemmungs_griff\""), "{text}");
    assert_eq!(sketch_of(&back).to_data(), sk.to_data());
}

#[test]
fn a_deleted_curvature_row_still_needs_17() {
    // Griff samt Bogen wieder aus: Es bleibt nur der gelöschte Eintrag der internen Zeile.
    let (mut sk, c, _, at) = handled_sketch();
    sk.activate_curvature_handle(c, at).unwrap();
    sk.deactivate_handle(c, at).unwrap();
    assert!(
        sk.curve_entries()
            .all(|k| !matches!(k, CurveKind::Spline { handles, .. } if !handles.is_empty()))
    );
    let snap = document(sk).snapshot();
    assert_eq!(required_format(&snap), 17);
    let text = encode_version(&snap).unwrap();
    assert!(text.contains("kruemmungs_griff"));
    assert_eq!(
        encode_version(&decode_version(&text, "x.json").unwrap()).unwrap(),
        text
    );
    // Eine ältere Formatangabe passt nicht zu diesem Inhalt.
    let old = text.replace("\"format\": 17", "\"format\": 16");
    assert_ne!(old, text);
    assert!(decode_version(&old, "x.json").is_err());
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

// ---- Glatt (Task 9, Spec C4, Review Focus 4) --------------------------------------------------

/// Datei von vor Skizzen 7 (Format 15): `Smooth` zwischen einem Spline ohne Griff und einem Bogen.
const OLD_SMOOTH: &str = include_str!("fixtures_old_smooth/format15_smooth_spline_arc.json");

/// Glatt, Bogen und Spline der alten Datei, dazu ihr gemeinsamer Punkt.
fn smooth_of(
    sk: &Sketch,
) -> (
    planora_sketch::ConstraintId,
    CurveId,
    CurveId,
    planora_sketch::PointId,
) {
    let (id, a, pa, b) = sk
        .constraints()
        .find_map(|(id, c)| match c.kind {
            planora_sketch::ConstraintKind::Smooth(a, pa, b, _) => Some((id, a, pa, b)),
            _ => None,
        })
        .expect("kein Glatt in der Datei");
    (id, a, b, pa)
}

fn dense(sk: &Sketch, c: CurveId) -> Vec<planora_math::DVec2> {
    let (lo, hi) = sk.curve_domain(c).unwrap();
    (0..=200)
        .map(|k| {
            sk.curve_eval(c, lo + (hi - lo) * f64::from(k) / 200.0)
                .unwrap()
        })
        .collect()
}

/// Krümmung in die Kurve hinein am Bereichsanfang (`start`) oder -ende, nur über die Lage gemessen.
fn kappa_into(sk: &Sketch, c: CurveId, start: bool) -> f64 {
    let (lo, hi) = sk.curve_domain(c).unwrap();
    let h = (hi - lo) * 2e-3;
    let (t, side) = if start { (lo, 1.0) } else { (hi, -1.0) };
    let f: [planora_math::DVec2; 6] =
        std::array::from_fn(|k| sk.curve_eval(c, t + side * k as f64 * h).unwrap());
    let d = (f[0] * (-137.0 / 60.0) + f[1] * 5.0 - f[2] * 5.0 + f[3] * (10.0 / 3.0) - f[4] * 1.25
        + f[5] * 0.2)
        / h
        * side;
    let a = (f[0] * (15.0 / 4.0) - f[1] * (77.0 / 6.0) + f[2] * (107.0 / 6.0) - f[3] * 13.0
        + f[4] * (61.0 / 12.0)
        - f[5] * (5.0 / 6.0))
        / (h * h);
    let k = d.perp_dot(a) / d.length().powi(3);
    if start { k } else { -k }
}

/// Krümmungssprung an der Fuge der alten Datei: Bogenende (hinten) gegen Spline-Anfang.
fn jump(sk: &Sketch, arc: CurveId, spline: CurveId) -> f64 {
    (kappa_into(sk, arc, false) + kappa_into(sk, spline, true)).abs()
}

#[test]
fn old_g1_on_arc_stays() {
    assert_eq!(format_of(OLD_SMOOTH), 15);
    let snap = decode_version(OLD_SMOOTH, "alt.json").unwrap();
    let doc = Document::from_snapshot(snap, 0.05).unwrap();
    let mut sk = sketch_of(&doc.snapshot()).clone();
    let (id, arc, spline, _) = smooth_of(&sk);
    let (shape_a, shape_s) = (dense(&sk, arc), dense(&sk, spline));
    let vars: Vec<u64> = sk.variables().iter().map(|v| v.to_bits()).collect();
    let k = jump(&sk, arc, spline);
    assert!(k > 1e-3, "die alte Datei hat einen Krümmungssprung: {k}");
    assert_eq!(sk.evaluate(id, sk.variables()).unwrap().residuals.len(), 1);

    let r = sk.solve();
    assert_eq!(r.status, planora_sketch::SolveStatus::Solved, "{r:?}");
    assert_eq!(r.iterations, 0, "{r:?}");
    let now: Vec<u64> = sk.variables().iter().map(|v| v.to_bits()).collect();
    assert_eq!(now, vars, "Lagen bitgleich");
    assert_eq!(dense(&sk, arc), shape_a);
    assert_eq!(dense(&sk, spline), shape_s);
    assert_eq!(jump(&sk, arc, spline), k, "der Sprung bleibt");

    // Auch nach dem Lösen dieselben Bytes im alten Format.
    let mut after = doc.snapshot();
    let FeatureKind::Sketch(sf) = &mut after.features[0].kind else {
        panic!("keine Skizze");
    };
    sf.sketch = sk;
    assert!(required_format(&after) < 17);
    assert_eq!(encode_version(&after).unwrap(), OLD_SMOOTH);
}

#[test]
fn redone_smooth_on_old_file_is_g2_and_17() {
    let snap = decode_version(OLD_SMOOTH, "alt.json").unwrap();
    let mut sk = sketch_of(&snap).clone();
    let (id, arc, spline, joint) = smooth_of(&sk);
    sk.remove_constraint(id).unwrap();
    sk.prepare_smooth(arc, joint, spline, joint).unwrap();
    let new = sk
        .add_constraint(planora_sketch::ConstraintKind::Smooth(
            arc, joint, spline, joint,
        ))
        .unwrap();
    assert_eq!(sk.evaluate(new, sk.variables()).unwrap().residuals.len(), 2);
    assert_eq!(sk.solve().status, planora_sketch::SolveStatus::Solved);
    assert!(jump(&sk, arc, spline) < 1e-6, "{}", jump(&sk, arc, spline));
    let doc = document(sk.clone());
    assert_eq!(required_format(&doc.snapshot()), 17);
    let text = encode_version(&doc.snapshot()).unwrap();
    assert_eq!(format_of(&text), 17);
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(encode_version(&back).unwrap(), text);
    let mut again = sketch_of(&back).clone();
    assert_eq!(again.solve().status, planora_sketch::SolveStatus::Solved);
    assert!(jump(&again, arc, spline) < 1e-6);
}

// ---- Fester Spline (Task 10) -------------------------------------------------------------------

fn fixed_sketch() -> (Sketch, CurveId) {
    let curve = planora_math::nurbs::NurbsCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0],
        vec![
            planora_math::DVec3::new(0.0, 0.0, 0.0),
            planora_math::DVec3::new(3.0, 6.0, 1.0),
            planora_math::DVec3::new(7.0, 5.0, 2.0),
            planora_math::DVec3::new(9.1, 0.3, 1.0),
        ],
        vec![1.0, 3.0, 0.4, 1.0],
    )
    .unwrap();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let c = sk
        .add_projected_curve(
            &planora_kernel::Curve::Nurbs(std::sync::Arc::new(curve)),
            0.0,
            1.0,
            planora_sketch::helpers::ProjectStyle {
                construction: false,
            },
        )
        .unwrap();
    (sk, c)
}

#[test]
fn fixed_needs_17_and_roundtrips() {
    let (sk, c) = fixed_sketch();
    let want = sk.curve(c).unwrap().kind.clone();
    let snap = document(sk).snapshot();
    assert_eq!(required_format(&snap), 17);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 17);
    assert!(text.contains("\"typ\": \"fester_spline\""), "{text}");
    assert!(text.contains("\"gewichte\""), "{text}");
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(sketch_of(&back).curve(c).unwrap().kind, want, "bitgleich");
    assert_eq!(encode_version(&back).unwrap(), text);
}

#[test]
fn deleted_fixed_needs_17() {
    let (mut sk, c) = fixed_sketch();
    sk.remove_curve(c).unwrap();
    let snap = document(sk).snapshot();
    assert_eq!(required_format(&snap), 17);
    let text = encode_version(&snap).unwrap();
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(encode_version(&back).unwrap(), text);
}

/// Kaputte Daten werden mit Meldung abgelehnt, nicht repariert.
#[test]
fn broken_fixed_data_is_refused() {
    let (sk, _) = fixed_sketch();
    let text = encode_version(&document(sk).snapshot()).unwrap();
    fn find(v: &mut serde_json::Value) -> Option<&mut serde_json::Value> {
        if v.get("typ").and_then(|t| t.as_str()) == Some("fester_spline") {
            return Some(v);
        }
        match v {
            serde_json::Value::Object(m) => m.values_mut().find_map(find),
            serde_json::Value::Array(a) => a.iter_mut().find_map(find),
            _ => None,
        }
    }
    type Edit = fn(&mut serde_json::Value);
    // Der Grund kommt von der Prüfung selbst: dieselben Daten, direkt an `NurbsCurve::new`.
    let math = |grad: usize, knoten: Vec<f64>, gewichte: Vec<f64>| {
        let punkte = vec![planora_math::DVec3::ZERO; 4];
        planora_math::nurbs::NurbsCurve::new(grad, knoten, punkte, gewichte)
            .unwrap_err()
            .to_string()
    };
    let knots = vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0];
    let weights = vec![1.0, 3.0, 0.4, 1.0];
    let fixed = planora_i18n::tr!("sketch-err-data-fixed-spline");
    let cases: Vec<(&str, String, Edit)> = vec![
        ("Knoten", math(2, vec![0.0, 1.0], weights.clone()), |e| {
            e["knoten"] = serde_json::json!([0.0, 1.0])
        }),
        ("Grad", math(7, knots.clone(), weights.clone()), |e| {
            e["grad"] = serde_json::json!(7)
        }),
        (
            "Gewichte",
            math(2, knots.clone(), vec![1.0, 0.0, 1.0, 1.0]),
            |e| e["gewichte"] = serde_json::json!([1.0, 0.0, 1.0, 1.0]),
        ),
        ("Zahl der Gewichte", math(2, knots, vec![1.0]), |e| {
            e["gewichte"] = serde_json::json!([1.0])
        }),
        (
            "Punkt fehlt",
            planora_i18n::tr!("sketch-err-data-curve-point"),
            |e| e["ende"] = serde_json::json!(999),
        ),
        ("geschlossen, aber offen", fixed, |e| {
            e["ende"] = e["start"].clone()
        }),
    ];
    for (what, reason, edit) in cases {
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        edit(find(&mut v).expect("Eintrag"));
        let e = decode_version(&v.to_string(), "x.json")
            .err()
            .unwrap_or_else(|| panic!("{what}: nicht abgelehnt"))
            .to_string();
        assert!(e.contains(&reason), "{what}: {e}");
    }
}
