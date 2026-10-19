//! Dateiformat 8: Schnittansichten (Teilprojekt 3d, Aufgabe 4). Format 8 nur mit Schnitten,
//! alte Dateien Byte für Byte wie bisher, Rundreise, benannte Fehler.

use planora_drawing::{
    DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind, ViewSpec,
};
use planora_feature::*;
use planora_math::DVec3;
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use serde_json::Value;

const A3: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};

fn doc_with_box() -> Document {
    let mut d = Document::new();
    d.add_feature(
        "Quader 1",
        FeatureKind::Box {
            size: DVec3::new(30.0, 20.0, 10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    d
}

/// Dokument mit Vorne (Kennung 1) und, wenn `section`, einem Schnitt nach Oben.
fn doc(section: bool) -> (Document, DrawingId, ViewId) {
    let mut d = doc_with_box();
    let id = d.add_drawing("Blatt", A3, Projection::First).unwrap();
    let front = d
        .add_view(
            id,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 0.0, y: 0.0 },
            None,
        )
        .unwrap();
    if section {
        d.update_drawing(id, |dr| {
            dr.add_section(front, ViewKind::Top, 0.0, 80.0).unwrap();
        })
        .unwrap();
    }
    (d, id, front)
}

fn fmt(d: &Document) -> u32 {
    required_format(&d.snapshot())
}

fn bad(t: &str) -> ProjectError {
    decode_version(t, "x.json").unwrap_err()
}

fn edited(text: &str, f: impl FnOnce(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(text).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

#[test]
fn format_8_only_with_sections() {
    assert_eq!(FORMAT_VERSION, 8);
    let (plain, _, _) = doc(false);
    // Ohne Schnitt wie bisher: Format 5 (die Normansicht hat Mittellinien), kein Schnittfeld.
    assert_eq!(fmt(&plain), 5);
    let text = encode_version(&plain.snapshot()).unwrap();
    assert!(!text.contains("\"schnitt\""), "{text}");
    assert!(text.contains("\"format\": 5"));

    let (mut d, id, front) = doc(true);
    assert_eq!(fmt(&d), 8);
    // Eigene Werte: Versatz 12,5, Buchstabe B, Schraffur aus, Abstand 4.
    d.update_drawing(id, |dr| {
        let s = dr.views[1].section.as_mut().unwrap();
        s.offset = 12.5;
        s.letter = 'B';
        s.hatch.on = false;
        s.hatch.spacing = 4.0;
    })
    .unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    assert!(text.contains("\"format\": 8") && text.contains("\"schnitt\""));
    let back = decode_version(&text, "x.json").unwrap();
    let sec_of = |views: &[ViewSpec]| views[1].section.unwrap();
    let before = d.snapshot();
    let (a, b) = (&before.drawings[0], &back.drawings[0]);
    assert_eq!(a, b, "Rundreise gleich");
    let s = sec_of(&b.views);
    assert_eq!(s.offset, 12.5);
    assert_eq!(s.letter, 'B');
    assert!(!s.hatch.on);
    assert_eq!(s.hatch.spacing, 4.0);
    assert_eq!(b.views[0].section, None);
    assert_eq!(encode_version(&back).unwrap(), text, "stabil");
    let doc2 = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(encode_version(&doc2.snapshot()).unwrap(), text);

    // Letzten Schnitt gelöscht: wieder kleiner als 8 und ohne Schnittfeld.
    let sec_id = d.drawing(id).unwrap().views[1].id;
    d.update_drawing(id, |dr| {
        dr.remove_view(sec_id);
    })
    .unwrap();
    assert!(fmt(&d) <= 7);
    assert!(!encode_version(&d.snapshot()).unwrap().contains("schnitt"));
    let _ = front;
}

#[test]
fn a_section_in_a_format_8_file_with_other_marks_keeps_them() {
    // Maße und Stückliste werden mit Format 8 weitergeschrieben.
    let (mut d, id, _) = doc(true);
    let mut i = planora_drawing::sheet_info::SheetInfo::new(&planora_drawing::sheet_info::Stamp {
        date: "2026-10-19".into(),
        user: "dennis".into(),
    });
    i.parts_list = true;
    d.set_sheet_info(id, i).unwrap();
    assert_eq!(fmt(&d), 8);
    let text = encode_version(&d.snapshot()).unwrap();
    let back = decode_version(&text, "x.json").unwrap();
    assert!(back.drawings[0].info.as_ref().unwrap().parts_list);
    assert_eq!(back.drawings[0], d.snapshot().drawings[0]);
}

#[test]
fn format_errors() {
    let (d, _, _) = doc(true);
    let good = encode_version(&d.snapshot()).unwrap();
    assert!(decode_version(&good, "x.json").is_ok());

    // Format 7 mit Schnitt: beschädigt, mit dem Hinweis auf Format 8.
    let t = good.replacen("\"format\": 8", "\"format\": 7", 1);
    match bad(&t) {
        ProjectError::Corrupt { .. } => {}
        other => panic!("{other:?}"),
    }
    assert!(bad(&t).to_string().contains("erst ab Dateiformat 8"));
    for older in [4, 5, 6] {
        let t = good.replacen("\"format\": 8", &format!("\"format\": {older}"), 1);
        assert!(
            bad(&t).to_string().contains("erst ab Dateiformat"),
            "Format {older}"
        );
    }

    // Buchstabe „I“, „AB“, leer, „a“: beschädigt, mit dem Zeichnungsnamen.
    for letter in ["I", "AB", "", "a", "O", "Q"] {
        let t = edited(&good, |v| {
            v["zeichnungen"][0]["ansichten"][1]["schnitt"]["buchstabe"] = letter.into()
        });
        let e = bad(&t);
        assert!(
            matches!(e, ProjectError::Corrupt { .. }),
            "{letter:?}: {e:?}"
        );
        assert!(
            e.to_string().contains("Zeichnung „Blatt“"),
            "{letter:?}: {e}"
        );
    }

    // Doppelter Buchstabe, Maßstab, Versatz, Abstand: beschädigt.
    let dup = edited(&good, |v| {
        let a = v["zeichnungen"][0]["ansichten"][1].clone();
        let mut b = a.clone();
        b["id"] = 3.into();
        v["zeichnungen"][0]["ansichten"]
            .as_array_mut()
            .unwrap()
            .push(b);
        v["zeichnungen"][0]["naechste_ansicht"] = 4.into();
    });
    assert!(bad(&dup).to_string().contains("doppelt"));
    let scale = edited(&good, |v| {
        v["zeichnungen"][0]["ansichten"][1]["massstab"] = serde_json::json!([2, 1])
    });
    assert!(bad(&scale).to_string().contains("Maßstab"));
    let off = edited(&good, |v| {
        v["zeichnungen"][0]["ansichten"][1]["schnitt"]["versatz"] = 1.0e9.into()
    });
    assert!(matches!(bad(&off), ProjectError::Corrupt { .. }));
    let fine = edited(&good, |v| {
        v["zeichnungen"][0]["ansichten"][1]["schnitt"]["schraffurabstand"] = 0.1.into()
    });
    let m = bad(&fine).to_string();
    assert!(
        m.contains("0,5 mm") && m.contains("Zeichnung „Blatt“"),
        "{m}"
    );
    let wrong_kind = edited(&good, |v| {
        v["zeichnungen"][0]["ansichten"][1]["art"] = "hinten".into()
    });
    assert!(bad(&wrong_kind).to_string().contains("Schnittebene"));

    // Format 9 ist zu neu.
    let t = good.replacen("\"format\": 8", "\"format\": 9", 1);
    assert!(matches!(bad(&t), ProjectError::TooNew { found: 9, .. }));
}

#[test]
fn format_7_files_load_unchanged() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut n = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("\"schnitt\""), "{name}");
        let s = decode_version(&text, &name).unwrap();
        assert!(
            s.drawings
                .iter()
                .all(|d| d.views.iter().all(|v| v.section.is_none())),
            "{name}"
        );
        assert!(required_format(&s) <= 7, "{name}");
        // Neu geschrieben: Byte für Byte die Datei (Zeilenenden wie im Repository).
        let again = encode_version(&s).unwrap();
        assert_eq!(
            again.replace("\r\n", "\n"),
            text.replace("\r\n", "\n"),
            "{name}"
        );
        n += 1;
    }
    assert_eq!(n, 7, "alle sieben Fixturen");
}
