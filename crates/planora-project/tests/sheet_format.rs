//! Dateiformat 6: Blatt- und Projektangaben. Kleinste passende Version, alte Dateien Byte für
//! Byte, Rundreise, benannte Fehler (Teilprojekt 3c-1, Task 3).

use planora_drawing::sheet_info::{
    FieldValue, NotesValue, ProjectInfo, Revision, SheetInfo, Stamp,
};
use planora_drawing::{Paper, Projection, Sheet};
use planora_feature::*;
use planora_math::DVec3;
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use serde_json::Value;

const A4: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-17".into(),
        user: "dennis".into(),
    }
}

fn with_box() -> Document {
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

fn bad(text: &str) -> ProjectError {
    decode_version(text, "x.json").unwrap_err()
}

fn msg(e: ProjectError) -> String {
    e.to_string()
}

/// Dokument mit einer Zeichnung ohne Blattangaben, mit Ansicht (Format 4).
fn plain_doc() -> Document {
    let mut d = with_box();
    let id = d.add_drawing("Blatt", A4, Projection::First).unwrap();
    d.add_view(
        id,
        planora_drawing::ViewKind::Front,
        planora_drawing::Scale { num: 1, den: 1 },
        planora_drawing::Placement::Free { x: 0.0, y: 0.0 },
        None,
    )
    .unwrap();
    d.update_drawing(id, |dr| {
        for v in &mut dr.views {
            v.centerlines = false;
        }
    })
    .unwrap();
    d
}

#[test]
fn format_6_only_with_sheet_data() {
    assert_eq!(FORMAT_VERSION, 12);
    let d = plain_doc();
    assert_eq!(required_format(&d.snapshot()), 4);

    let mut e = with_box();
    assert_eq!(required_format(&e.snapshot()), 1);
    e.add_drawing_stamped("Blatt", A4, Projection::First, &stamp())
        .unwrap();
    assert_eq!(required_format(&e.snapshot()), 6);

    let mut only_project = with_box();
    only_project
        .set_project_info(ProjectInfo::new(&stamp()))
        .unwrap();
    assert_eq!(required_format(&only_project.snapshot()), 6);
    assert!(
        encode_version(&only_project.snapshot())
            .unwrap()
            .contains("\"format\": 6")
    );

    // Eine Zeichnung mit Mittellinien (Format 5) bleibt 5, ohne Satz.
    let mut f5 = plain_doc();
    let id = f5.drawings()[0].id;
    let v = f5.drawings()[0].views[0].id;
    f5.update_drawing(id, |dr| {
        dr.views.iter_mut().find(|x| x.id == v).unwrap().centerlines = true;
    })
    .unwrap();
    assert_eq!(required_format(&f5.snapshot()), 5);
}

/// Eingefrorene Dateien aus den Formaten 4 und 5 (von Hand nach den Regeln dieser Formate
/// gebaut, nicht vom heutigen Programm erzeugt): Lesen und wieder Schreiben ändert kein Byte.
#[test]
fn old_files_stay_byte_identical() {
    const FIXTURE_4: &str = include_str!("fixtures/format4_drawings_no_dims.json");
    const FIXTURE_5: &str = include_str!("fixtures/format5_drawings_centerlines.json");
    assert!(FIXTURE_4.contains("\"format\": 4") && FIXTURE_5.contains("\"format\": 5"));
    for t in [FIXTURE_4, FIXTURE_5] {
        assert!(!t.contains("projektangaben") && !t.contains("blattangaben"));
        let s = decode_version(t, "x.json").unwrap();
        assert!(s.project_info.is_none() && s.drawings.iter().all(|d| d.info.is_none()));
        assert_eq!(encode_version(&s).unwrap(), t);
        // auch nach dem Umweg über ein Dokument
        let d = Document::from_snapshot(s, 0.05).unwrap();
        assert_eq!(encode_version(&d.snapshot()).unwrap(), t);
    }
}

fn full_info() -> SheetInfo {
    let t = |s: &str| FieldValue::Text(s.to_string());
    let special = "ä ö ü ß Ø ° ± –";
    SheetInfo {
        created: "2026-10-17".into(),
        number: t("ZN-0815"),
        title: t(special),
        subtitle: t("Zusatz ä"),
        doc_type: t("Montagezeichnung"),
        status: t("freigegeben"),
        issued: t("2026-11-01"),
        revision: t("B"),
        sheet_of: t("2 von 5"),
        format: t("A3"),
        scale: t("1:2"),
        weight: t("1,25 kg"),
        notes: NotesValue::Lines(vec!["Hinweis ä".into(), special.into()]),
        revisions: (0..23)
            .map(|k| Revision {
                letter: if k == 4 { t("Ä") } else { FieldValue::Auto },
                description: format!("Änderung {k} {special}"),
                date: "2026-10-18".into(),
                name: "dg".into(),
            })
            .collect(),
        parts_list: false,
    }
}

fn full_project() -> ProjectInfo {
    let t = |s: &str| FieldValue::Text(s.to_string());
    ProjectInfo {
        login: "dennis".into(),
        owner: t("Müller & Söhne"),
        creator: t("D. G."),
        checked: t("A. B."),
        approved: t("C. D."),
        material: t("S235JR"),
        tolerance: t("ISO 2768-f"),
        notes: NotesValue::Lines(vec!["Projekthinweis ±".into(), "Zweiter".into()]),
    }
}

fn round_trip(d: &Document) -> Document {
    let text = encode_version(&d.snapshot()).unwrap();
    let back = Document::from_snapshot(decode_version(&text, "x.json").unwrap(), 0.05).unwrap();
    assert_eq!(back.drawings(), d.drawings());
    assert_eq!(back.project_info(), d.project_info());
    // zweites Speichern ändert nichts
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
    back
}

#[test]
fn full_round_trip() {
    let mut d = with_box();
    let id = d
        .add_drawing_stamped("Blatt", A4, Projection::First, &stamp())
        .unwrap();
    d.set_sheet_info(id, full_info()).unwrap();
    d.set_project_info(full_project()).unwrap();
    round_trip(&d);

    // leere/automatische Variante, auch leere eigene Hinweise
    let mut e = with_box();
    let id = e
        .add_drawing_stamped("Blatt", A4, Projection::Third, &stamp())
        .unwrap();
    round_trip(&e);
    let mut i = e.drawing(id).unwrap().info.clone().unwrap();
    i.notes = NotesValue::Lines(vec![]);
    e.set_sheet_info(id, i).unwrap();
    let back = round_trip(&e);
    assert_eq!(
        back.drawing(id).unwrap().info.as_ref().unwrap().notes,
        NotesValue::Lines(vec![])
    );
    let mut p = e.project_info().unwrap().clone();
    p.notes = NotesValue::Lines(vec![]);
    e.set_project_info(p).unwrap();
    round_trip(&e);
}

fn sheet_text() -> String {
    let mut d = with_box();
    let id = d
        .add_drawing_stamped("Blatt A", A4, Projection::First, &stamp())
        .unwrap();
    d.set_sheet_info(id, full_info()).unwrap();
    d.set_project_info(full_project()).unwrap();
    encode_version(&d.snapshot()).unwrap()
}

fn edited(f: impl FnOnce(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(&sheet_text()).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

#[test]
fn rejects_bad_sheet_data() {
    // Format 5 mit Blattangaben oder Projektangaben
    let t = edited(|v| v["format"] = 5.into());
    assert!(
        msg(bad(&t)).contains("Blattangaben gibt es erst ab Dateiformat 6"),
        "{}",
        msg(bad(&t))
    );
    let t = edited(|v| {
        v["format"] = 5.into();
        v["zeichnungen"][0]
            .as_object_mut()
            .unwrap()
            .remove("blattangaben");
    });
    assert!(msg(bad(&t)).contains("Blattangaben gibt es erst ab Dateiformat 6"));

    // 24 Änderungen
    let t = edited(|v| {
        let a = v["zeichnungen"][0]["blattangaben"]["aenderungen"]
            .as_array_mut()
            .unwrap();
        let first = a[0].clone();
        a.push(first);
    });
    let m = msg(bad(&t));
    assert!(m.contains("Zeichnung „Blatt A“") && m.contains("23"), "{m}");

    // 25 Änderungen und 8 Hinweise: gleiche Meldung mit dem richtigen Höchstwert
    let t = edited(|v| {
        let a = v["zeichnungen"][0]["blattangaben"]["aenderungen"]
            .as_array_mut()
            .unwrap();
        let first = a[0].clone();
        a.extend(std::iter::repeat_n(first, 2));
    });
    let m = msg(bad(&t));
    assert!(m.contains("Zeichnung „Blatt A“") && m.contains("23"), "{m}");
    let t = edited(|v| {
        v["zeichnungen"][0]["blattangaben"]["hinweise"] =
            Value::Array((0..8).map(|k| format!("h{k}").into()).collect());
    });
    let m = msg(bad(&t));
    assert!(
        m.contains("Zeichnung „Blatt A“") && m.contains("höchstens 6"),
        "{m}"
    );
    let t = edited(|v| {
        v["projektangaben"]["hinweise"] =
            Value::Array((0..8).map(|k| format!("h{k}").into()).collect());
    });
    let m = msg(bad(&t));
    assert!(
        m.contains("Projektangaben") && m.contains("höchstens 6"),
        "{m}"
    );

    // 7 Hinweise
    let t = edited(|v| {
        v["zeichnungen"][0]["blattangaben"]["hinweise"] =
            Value::Array((0..7).map(|k| format!("h{k}").into()).collect());
    });
    let m = msg(bad(&t));
    assert!(m.contains("Zeichnung „Blatt A“") && m.contains('6'), "{m}");

    // Titel 61 Zeichen
    let t = edited(|v| v["zeichnungen"][0]["blattangaben"]["titel"] = "ä".repeat(61).into());
    let m = msg(bad(&t));
    assert!(
        m.contains("Zeichnung „Blatt A“") && m.contains("Titel"),
        "{m}"
    );

    // Projektangaben
    let t = edited(|v| v["projektangaben"]["werkstoff"] = "x".repeat(81).into());
    let m = msg(bad(&t));
    assert!(
        m.contains("Projektangaben") && m.contains("Werkstoff"),
        "{m}"
    );
    let t = edited(|v| {
        v["projektangaben"]["hinweise"] =
            Value::Array((0..7).map(|k| format!("h{k}").into()).collect());
    });
    assert!(msg(bad(&t)).contains("Projektangaben"));

    // Steuerzeichen
    let t = edited(|v| v["zeichnungen"][0]["blattangaben"]["nummer"] = "a\u{1}b".into());
    assert!(msg(bad(&t)).contains("Zeichnung „Blatt A“"));

    // Format 12 ist zu neu
    let t = edited(|v| v["format"] = 12.into());
    assert!(matches!(bad(&t), ProjectError::TooNew { found: 13, .. }));
}

#[test]
fn huge_lists_are_cut_off_while_reading() {
    let t = edited(|v| {
        v["zeichnungen"][0]["blattangaben"]["aenderungen"] = Value::Array(
            (0..100_000)
                .map(|_| serde_json::json!({"beschreibung": "x", "datum": "d", "name": "n"}))
                .collect(),
        );
    });
    let m = msg(bad(&t));
    assert!(m.contains("Zeichnung „Blatt A“") && m.contains("23"), "{m}");
}

#[test]
fn unknown_fields_are_ignored() {
    let t = edited(|v| {
        v["zeichnungen"][0]["blattangaben"]["zukunft"] = "egal".into();
        v["zeichnungen"][0]["blattangaben"]["aenderungen"][0]["neu"] = 1.into();
        v["projektangaben"]["zukunft"] = true.into();
    });
    let s = decode_version(&t, "x.json").unwrap();
    let d = Document::from_snapshot(s, 0.05).unwrap();
    assert_eq!(d.project_info().unwrap().login, "dennis");
    assert_eq!(
        d.drawings()[0].info.as_ref().unwrap().number,
        FieldValue::Text("ZN-0815".into())
    );
}
