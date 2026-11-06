//! Dateiformat 15 (Teilprojekt Skizzen 5, Task 2): die neuen Abhängigkeitsarten. Nur eine Skizze
//! mit einem Eintrag dieser Arten (auch einem gelöschten) hebt die Formatnummer auf 15; ohne sie
//! bleibt der Text bytegleich wie bisher.

use planora_feature::*;
use planora_math::{DVec2, Tolerance};
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::shapes::{SlotPlan, SlotStyle};
use planora_sketch::{ConstraintId, CurveId, PointId, Sketch};

fn new_sketch() -> Sketch {
    Sketch::new(StandardPlane::XY.frame(), "Skizze 1", Tolerance::default())
}

fn document(sk: Sketch) -> Document {
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

fn line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> (CurveId, PointId, PointId) {
    let (pa, pb) = (
        s.add_point(a.0, a.1).unwrap(),
        s.add_point(b.0, b.1).unwrap(),
    );
    (s.add_line(pa, pb).unwrap(), pa, pb)
}

/// Skizze mit jeweils einer Abhängigkeit der sechs neuen Arten; liefert auch die Ids.
fn all_kinds() -> (Sketch, Vec<ConstraintId>) {
    let mut s = new_sketch();
    let mut ids = vec![];
    let (l1, _, _) = line(&mut s, (0.0, 0.0), (10.0, 0.0));
    let (l2, _, _) = line(&mut s, (15.0, 0.1), (25.0, 0.0));
    ids.push(s.add_constraint(K::Collinear(l1, l2)).unwrap());
    let (l3, _, joint) = line(&mut s, (-10.0, 20.0), (0.0, 20.0));
    let q = [
        s.add_point(5.0, 23.0).unwrap(),
        s.add_point(10.0, 20.0).unwrap(),
    ];
    let sp = s.add_spline(&[joint, q[0], q[1]]).unwrap();
    ids.push(s.add_constraint(K::Smooth(l3, joint, sp, joint)).unwrap());
    let (p, q2) = (
        s.add_point(30.0, 30.0).unwrap(),
        s.add_point(34.0, 37.0).unwrap(),
    );
    ids.push(s.add_constraint(K::DistanceX(p, q2, 4.0)).unwrap());
    ids.push(s.add_constraint(K::DistanceY(p, q2, 7.0)).unwrap());
    let (axis, _, _) = line(&mut s, (0.0, -30.0), (10.0, -30.0));
    s.set_centerline(axis, true).unwrap();
    let r = s.add_point(3.0, -34.0).unwrap();
    ids.push(
        s.add_constraint(K::CenterlineDiameter(r, axis, 8.0))
            .unwrap(),
    );
    let slot = s
        .add_slot(
            &SlotPlan {
                a: DVec2::new(0.0, -60.0),
                b: DVec2::new(20.0, -60.0),
                radius: 3.0,
            },
            SlotStyle::CenterToCenter,
        )
        .unwrap();
    ids.push(
        s.add_constraint(K::SlotOverall(
            slot.centers[0],
            slot.centers[1],
            slot.caps[0],
            26.0,
        ))
        .unwrap(),
    );
    (s, ids)
}

#[test]
fn format_version_is_15() {
    assert_eq!(FORMAT_VERSION, 15);
}

#[test]
fn every_new_kind_round_trips_through_the_data_and_the_project_format() {
    let (s, _) = all_kinds();
    let data = s.to_data();
    // Seite (branch) je Eintrag einmal anders setzen, damit sie nachweislich erhalten bleibt.
    let mut data2 = data.clone();
    for (i, c) in data2.constraints.iter_mut().enumerate() {
        if matches!(c.kind, K::DistanceX(..) | K::DistanceY(..)) {
            c.branch = if i % 2 == 0 { -1 } else { 1 };
        }
    }
    for d in [data, data2] {
        let sk = Sketch::from_data(d.clone()).unwrap();
        assert_eq!(sk.to_data(), d);
        let doc = document(sk);
        let snap = doc.snapshot();
        assert_eq!(required_format(&snap), 15);
        let text = encode_version(&snap).unwrap();
        assert!(text.contains("\"format\": 15"), "{text}");
        for name in [
            "kollinear",
            "glatt",
            "abstand_waagerecht",
            "abstand_senkrecht",
            "durchmesser_zur_achse",
            "langloch_gesamt",
        ] {
            assert!(text.contains(&format!("\"typ\": \"{name}\"")), "{name}");
        }
        let back = load(&text).unwrap();
        assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
        let FeatureKind::Sketch(sf) = &back.features()[0].kind else {
            panic!("keine Skizze")
        };
        assert_eq!(sf.sketch.to_data(), d);
    }
}

#[test]
fn every_new_kind_alone_needs_format_15_even_when_removed() {
    let (s, ids) = all_kinds();
    let kinds: Vec<K> = s.constraints().map(|(_, c)| c.kind.clone()).collect();
    assert_eq!(kinds.iter().filter(|k| k.needs_format_15()).count(), 6);
    for id in &ids {
        // Nur dieser Eintrag: alle anderen entfernen und den eigenen leben lassen, dann auch
        // ihn entfernen. Die Einträge bleiben in der Datei, also bleibt es bei 15.
        let mut sk = s.clone();
        sk.remove_constraint(*id).unwrap();
        assert!(sk.needs_format_15());
    }
    let mut sk = s;
    for id in &ids {
        sk.remove_constraint(*id).unwrap();
    }
    assert!(sk.constraints().all(|(_, c)| !c.kind.needs_format_15()));
    assert!(sk.needs_format_15());
    assert_eq!(required_format(&document(sk).snapshot()), 15);
}

#[test]
fn required_format_is_unchanged_without_new_entries_and_15_with_collinear() {
    let mut s = new_sketch();
    let (l1, _, _) = line(&mut s, (0.0, 0.0), (10.0, 0.0));
    let (l2, _, _) = line(&mut s, (15.0, 0.1), (25.0, 0.0));
    s.add_constraint(K::Parallel(l1, l2)).unwrap();
    assert!(!s.needs_format_15());
    assert_eq!(required_format(&document(s.clone()).snapshot()), 1);
    s.set_centerline(l1, true).unwrap();
    assert_eq!(required_format(&document(s.clone()).snapshot()), 14);

    let mut live = s.clone();
    let c = live.add_constraint(K::Collinear(l1, l2)).unwrap();
    assert!(live.needs_format_15());
    assert_eq!(required_format(&document(live.clone()).snapshot()), 15);
    live.remove_constraint(c).unwrap();
    assert!(live.needs_format_15());
    assert_eq!(required_format(&document(live).snapshot()), 15);
}

#[test]
fn every_fixture_loads_and_writes_the_same_bytes() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let doc = load(&text).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let out = encode_version(&doc.snapshot()).unwrap();
        let head = |t: &str| {
            t.lines()
                .find(|l| l.contains("\"format\""))
                .unwrap()
                .trim()
                .to_string()
        };
        assert_eq!(head(&out), head(&text), "{name}: Formatnummer");
        assert_eq!(out, text, "{name}: Bytes");
        n += 1;
    }
    assert!(n >= 8, "{n}");
}

#[test]
fn a_head_newer_than_the_known_format_is_too_new() {
    let (s, _) = all_kinds();
    let text = encode_version(&document(s).snapshot()).unwrap();
    assert!(load(&text).is_ok());
    let newer = text.replacen("\"format\": 15", "\"format\": 16", 1);
    let e = load(&newer).unwrap_err();
    assert!(matches!(e, ProjectError::TooNew { found: 16, .. }), "{e:?}");
}
