//! Dateiformat 12 (Teilprojekt Zoll, Task 7): die Maßeinheit je Zeichnung. Alte Dateien lesen
//! sich als Millimeter und schreiben sich Byte für Byte zurück; nur eine Zeichnung in Zoll hebt
//! die Formatnummer auf 12 und trägt `"einheit": "in"`.

use planora_drawing::sheet_info::Stamp;
use planora_drawing::{Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_feature::*;
use planora_i18n::{DisplayUnit, german, with_lang};
use planora_math::DVec3;
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use serde_json::{Value, json};

const A3: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-31".into(),
        user: "dennis".into(),
    }
}

/// Ein Quader und zwei Zeichnungen mit Blattangaben und je einer Ansicht.
fn document(unit_b: DisplayUnit) -> Document {
    let mut d = Document::new();
    d.add_feature(
        "Quader 1",
        FeatureKind::Box {
            size: DVec3::new(30.0, 20.0, 10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    for (name, unit) in [("Blatt 1", DisplayUnit::Millimeter), ("Blatt 2", unit_b)] {
        let id = d
            .add_drawing_stamped_in(name, A3, Projection::First, &stamp(), unit)
            .unwrap();
        d.add_view(
            id,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 100.0, y: 100.0 },
            None,
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
fn format_version_is_12() {
    assert_eq!(FORMAT_VERSION, 12);
}

#[test]
fn millimeter_drawings_keep_their_format_and_bytes() {
    let d = document(DisplayUnit::Millimeter);
    let s = d.snapshot();
    assert_eq!(required_format(&s), 6);
    let text = encode_version(&s).unwrap();
    assert!(text.contains("\"format\": 6"), "{text}");
    assert!(!text.contains("einheit"), "{text}");
}

#[test]
fn old_files_read_as_millimeters_and_write_back_byte_for_byte() {
    let fixtures = [
        include_str!("fixtures/format4_drawings_no_dims.json"),
        include_str!("fixtures/format5_drawings_centerlines.json"),
        include_str!("fixtures/format6_sheet_info.json"),
        include_str!("fixtures/format7_parts.json"),
    ];
    for f in fixtures {
        let s = decode_version(f, "alt").unwrap();
        assert!(!s.drawings.is_empty());
        assert!(s.drawings.iter().all(|d| d.unit == DisplayUnit::Millimeter));
        assert_eq!(encode_version(&s).unwrap(), f);
    }
}

#[test]
fn an_inch_drawing_raises_the_format_and_roundtrips() {
    let d = document(DisplayUnit::Zoll);
    let s = d.snapshot();
    assert_eq!(required_format(&s), 12);
    let text = encode_version(&s).unwrap();
    assert!(text.contains("\"format\": 12"), "{text}");
    assert_eq!(text.matches("\"einheit\"").count(), 1, "{text}");
    assert!(text.contains("\"einheit\": \"in\""), "{text}");
    let back = load(&text).unwrap();
    let units: Vec<DisplayUnit> = back.drawings().iter().map(|d| d.unit).collect();
    assert_eq!(units, vec![DisplayUnit::Millimeter, DisplayUnit::Zoll]);
    assert_eq!(back.drawings(), d.drawings());
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
}

#[test]
fn explicit_mm_reads_as_millimeters() {
    let text = encode_version(&document(DisplayUnit::Millimeter).snapshot()).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["zeichnungen"][1]["einheit"] = json!("mm");
    let back = load(&serde_json::to_string_pretty(&v).unwrap()).unwrap();
    assert!(
        back.drawings()
            .iter()
            .all(|d| d.unit == DisplayUnit::Millimeter)
    );
}

#[test]
fn unknown_unit_is_refused_with_a_named_error() {
    let text = encode_version(&document(DisplayUnit::Zoll).snapshot()).unwrap();
    let t = text.replacen("\"einheit\": \"in\"", "\"einheit\": \"ft\"", 1);
    let e = with_lang(&german(), || load(&t).unwrap_err());
    let msg = match e {
        ProjectError::Corrupt { msg, .. } => msg,
        other => panic!("beschädigt erwartet, nicht {other:?}"),
    };
    assert!(msg.contains("Maßeinheit") && msg.contains("ft"), "{msg}");
    assert!(msg.contains("Blatt 2"), "{msg}");
}
