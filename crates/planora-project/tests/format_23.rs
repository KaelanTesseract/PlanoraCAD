//! Dateiformat 23 (Teilprojekt Skizzen 12, Task 2): Anzeigeschalter je Skizze. Nur ein Dokument,
//! in dem ein `SketchDisplay` von der Vorgabe abweicht, wird mit Format 23 geschrieben; alle
//! anderen bleiben bytegleich. Je Skizze steht ein Eintrag mit nur den abweichenden Feldern.

use planora_feature::*;
use planora_i18n::tr;
use planora_math::{DVec3, Frame, Tolerance};
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::Sketch;

fn format_of(text: &str) -> u64 {
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    v["format"].as_u64().unwrap()
}

fn sketch_feature(name: &str) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch: Sketch::new(Frame::WORLD, name, Tolerance::default()),
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
        text_formulas: TextFormulas::new(),
    })
}

/// Zwei Skizzen und ein Quader; liefert das Dokument und die Kennungen in dieser Reihenfolge.
fn document() -> (Document, Vec<FeatureId>) {
    let mut d = Document::new();
    d.add_feature("Skizze1", sketch_feature("Skizze1")).unwrap();
    d.add_feature("Skizze2", sketch_feature("Skizze2")).unwrap();
    d.add_feature(
        "Quader1",
        FeatureKind::Box {
            size: DVec3::new(40.0, 20.0, 10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let ids = d.features().iter().map(|f| f.id).collect();
    (d, ids)
}

fn file_value(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap()
}

fn load_err(v: &serde_json::Value) -> ProjectError {
    decode_version(&v.to_string(), "x.json").unwrap_err()
}

#[test]
fn format_version_is_23() {
    assert_eq!(FORMAT_VERSION, 23);
}

/// Jede vorhandene Testdatei (Format <= 22) lädt und wird bytegleich wieder geschrieben.
#[test]
fn old_files_stay_byte_identical() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut seen = 0;
    for dir in [
        "data",
        "fixtures",
        "fixtures_format18",
        "fixtures_old_smooth",
        "fixtures_format21",
        "fixtures_format22",
        "step_fixtures",
    ] {
        let mut entries: Vec<_> = std::fs::read_dir(root.join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        entries.sort();
        for p in entries {
            let name = p.display().to_string();
            let raw = std::fs::read_to_string(&p).unwrap().replace("\r\n", "\n");
            let text = raw.trim_end_matches('\n');
            let snap = decode_version(text, &name).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(snap.sketch_display.is_empty(), "{name}");
            let back = encode_version(&snap).unwrap();
            assert_eq!(back, text, "{name}");
            assert!(format_of(&back) <= 22, "{name}");
            assert!(!back.contains("\"skizzenanzeige\""), "{name}");
            seen += 1;
        }
    }
    assert!(seen >= 17, "{seen}");
}

#[test]
fn display_needs_format_23() {
    let (d, ids) = document();
    let mut snap = d.snapshot();
    assert!(required_format(&snap) <= 22);
    let text = encode_version(&snap).unwrap();
    assert!(format_of(&text) <= 22);
    assert!(!text.contains("skizzenanzeige"));
    snap.sketch_display.insert(
        ids[0],
        SketchDisplay {
            slice: true,
            ..Default::default()
        },
    );
    assert_eq!(required_format(&snap), 23);
    assert_eq!(format_of(&encode_version(&snap).unwrap()), 23);
}

/// Jedes Feld einzeln umgeschaltet: Rundlauf verlustfrei, nur dieses Feld steht in der Datei.
#[test]
fn display_roundtrip() {
    let (d, ids) = document();
    let def = SketchDisplay::default();
    let cases: Vec<(&str, SketchDisplay)> = vec![
        ("slice", SketchDisplay { slice: true, ..def }),
        (
            "profil",
            SketchDisplay {
                profiles: false,
                ..def
            },
        ),
        (
            "punkte",
            SketchDisplay {
                points: false,
                ..def
            },
        ),
        (
            "masse",
            SketchDisplay {
                dimensions: false,
                ..def
            },
        ),
        (
            "abhaengigkeiten",
            SketchDisplay {
                constraints: false,
                ..def
            },
        ),
        (
            "konstruktion",
            SketchDisplay {
                construction: false,
                ..def
            },
        ),
        (
            "projiziert",
            SketchDisplay {
                projected: false,
                ..def
            },
        ),
        (
            "masse_fertig",
            SketchDisplay {
                dimensions_finished: true,
                ..def
            },
        ),
    ];
    for (key, disp) in &cases {
        let mut snap = d.snapshot();
        snap.sketch_display.insert(ids[1], *disp);
        let text = encode_version(&snap).unwrap();
        assert_eq!(format_of(&text), 23, "{key}");
        let v = file_value(&text);
        let list = v["skizzenanzeige"].as_array().unwrap();
        assert_eq!(list.len(), 1, "{key}");
        let entry = list[0].as_object().unwrap();
        assert_eq!(entry["skizze"].as_u64().unwrap(), ids[1].0, "{key}");
        assert_eq!(entry.len(), 2, "{key}: {entry:?}");
        assert!(entry.contains_key(*key), "{key}");
        let back = decode_version(&text, "x.json").unwrap();
        assert_eq!(back.sketch_display, snap.sketch_display, "{key}");
        assert_eq!(encode_version(&back).unwrap(), text, "{key}");
    }
    // Alle Felder und beide Skizzen; Liste aufsteigend nach Skizze.
    let mut snap = d.snapshot();
    let all = SketchDisplay {
        slice: true,
        profiles: false,
        points: false,
        dimensions: false,
        constraints: false,
        construction: false,
        projected: false,
        dimensions_finished: true,
    };
    snap.sketch_display.insert(ids[1], all);
    snap.sketch_display.insert(ids[0], all);
    let text = encode_version(&snap).unwrap();
    let v = file_value(&text);
    let list = v["skizzenanzeige"].as_array().unwrap();
    assert_eq!(list[0]["skizze"].as_u64().unwrap(), ids[0].0);
    assert_eq!(list[1]["skizze"].as_u64().unwrap(), ids[1].0);
    assert_eq!(list[0].as_object().unwrap().len(), 9);
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(back.sketch_display, snap.sketch_display);
}

/// Gültige Datei mit Eintrag für die erste Skizze (Slice an); liefert auch die Kennungen.
fn valid_value() -> (serde_json::Value, Vec<FeatureId>) {
    let (d, ids) = document();
    let mut snap = d.snapshot();
    snap.sketch_display.insert(
        ids[0],
        SketchDisplay {
            slice: true,
            ..Default::default()
        },
    );
    (file_value(&encode_version(&snap).unwrap()), ids)
}

#[test]
fn display_in_old_format_is_corrupt() {
    let expected = tr!(
        "proj-since-format",
        what = tr!("proj-feat-sketch-display"),
        format = "23"
    );
    for old in [22, 21, 16, 1] {
        let (mut v, _) = valid_value();
        v["format"] = old.into();
        let e = load_err(&v);
        assert!(matches!(e, ProjectError::Corrupt { .. }), "{old}: {e:?}");
        assert!(e.to_string().contains(&expected), "{old}: {e}");
    }
}

#[test]
fn display_for_unknown_sketch_is_corrupt() {
    let (mut v, _) = valid_value();
    v["skizzenanzeige"][0]["skizze"] = 999.into();
    let e = load_err(&v);
    assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
    assert!(
        e.to_string()
            .contains(&tr!("proj-sketch-display-unknown", feature = "999")),
        "{e}"
    );
}

/// Ein Feature, das keine Skizze ist, zählt als unbekannte Skizze.
#[test]
fn display_for_non_sketch_is_corrupt() {
    let (mut v, ids) = valid_value();
    v["skizzenanzeige"][0]["skizze"] = ids[2].0.into();
    let e = load_err(&v);
    assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
    assert!(
        e.to_string().contains(&tr!(
            "proj-sketch-display-unknown",
            feature = ids[2].0.to_string()
        )),
        "{e}"
    );
}

#[test]
fn duplicate_display_entry_is_corrupt() {
    let (mut v, ids) = valid_value();
    let first = v["skizzenanzeige"][0].clone();
    v["skizzenanzeige"].as_array_mut().unwrap().push(first);
    let e = load_err(&v);
    assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
    assert!(
        e.to_string().contains(&tr!(
            "proj-sketch-display-twice",
            feature = ids[0].0.to_string()
        )),
        "{e}"
    );
}

#[test]
fn default_entry_in_file_is_corrupt() {
    let (v0, ids) = valid_value();
    let id = ids[0].0;
    let expected = tr!("proj-sketch-display-default", feature = id.to_string());
    // Nur die Kennung, kein abweichendes Feld.
    let mut v = v0.clone();
    v["skizzenanzeige"][0] = serde_json::json!({ "skizze": id });
    let e = load_err(&v);
    assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
    assert!(e.to_string().contains(&expected), "{e}");
    // Ein Feld mit dem Vorgabewert weicht auch nicht ab.
    let mut v = v0;
    v["skizzenanzeige"][0] = serde_json::json!({ "skizze": id, "slice": false });
    let e = load_err(&v);
    assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
    assert!(e.to_string().contains(&expected), "{e}");
}
