//! Dateiformat 7: Stückliste, Positionsnummern und Stücklistenblätter (Teilprojekt 3c-2, Task 5).
//! Kleinste passende Version, alte Dateien Byte für Byte, Rundreise, benannte Fehler.

use planora_drawing::parts::{MAX_BALLOONS, MAX_OVERRIDES, parts_sheet_name};
use planora_drawing::sheet_info::{FieldValue, Stamp};
use planora_drawing::{
    Balloon, BalloonId, BodyKey, Drawing, DrawingId, DrawingKind, Paper, Placement, Projection,
    RowOverride, Scale, Sheet, ViewId, ViewKind,
};
use planora_feature::*;
use planora_math::DVec3;
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use serde_json::{Value, json};

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

fn key(f: u64, i: u32) -> BodyKey {
    BodyKey {
        feature: f,
        index: i,
    }
}

fn text(s: &str) -> FieldValue {
    FieldValue::Text(s.into())
}

fn ov(name: &str) -> RowOverride {
    RowOverride {
        name: text(name),
        ..RowOverride::default()
    }
}

fn bad(t: &str) -> ProjectError {
    decode_version(t, "x.json").unwrap_err()
}

fn msg(e: ProjectError) -> String {
    e.to_string()
}

/// Dokument mit Zeichnung und Ansicht (Format 4), ohne Teile-Daten.
fn plain() -> (Document, DrawingId, ViewId) {
    let mut d = with_box();
    let id = d.add_drawing("Blatt", A4, Projection::First).unwrap();
    let v = d
        .add_view(
            id,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 0.0, y: 0.0 },
            None,
        )
        .unwrap();
    d.update_drawing(id, |dr| {
        for v in &mut dr.views {
            v.centerlines = false;
        }
    })
    .unwrap();
    (d, id, v)
}

fn fmt(d: &Document) -> u32 {
    required_format(&d.snapshot())
}

#[test]
fn format_7_only_with_parts_data() {
    assert_eq!(FORMAT_VERSION, 8);

    // nichts: wie bisher
    assert_eq!(fmt(&with_box()), 1);
    assert_eq!(fmt(&plain().0), 4);

    // nur Überschreibung (auch ohne Zeichnung)
    let mut o = with_box();
    o.set_parts_override(key(1, 0), ov("Welle")).unwrap();
    assert_eq!(fmt(&o), 7);
    assert!(
        encode_version(&o.snapshot())
            .unwrap()
            .contains("\"format\": 7")
    );
    // Überschreibung wieder automatisch: zurück auf 1
    o.set_parts_override(key(1, 0), RowOverride::default())
        .unwrap();
    assert_eq!(fmt(&o), 1);

    // Schalter an / aus
    let mut e = with_box();
    let id = e
        .add_drawing_stamped("Blatt", A4, Projection::First, &stamp())
        .unwrap();
    assert_eq!(fmt(&e), 6);
    let mut i = e.drawing(id).unwrap().info.clone().unwrap();
    i.parts_list = true;
    e.set_sheet_info(id, i.clone()).unwrap();
    assert_eq!(fmt(&e), 7);
    i.parts_list = false;
    e.set_sheet_info(id, i).unwrap();
    assert_eq!(fmt(&e), 6);

    // nur Stücklistenblatt
    let mut s = with_box();
    s.add_parts_sheets(A4, &stamp()).unwrap();
    assert_eq!(fmt(&s), 7);

    // nur Kreis
    let (mut c, id, v) = plain();
    c.add_balloon(id, v, key(1, 0), (0.5, 0.5), (10.0, 10.0))
        .unwrap();
    assert_eq!(fmt(&c), 7);
    // Kreis weg: wieder 4
    c.update_drawing(id, |d| d.balloons.clear()).unwrap();
    assert_eq!(fmt(&c), 4);
}

#[test]
fn old_files_stay_byte_identical() {
    const F4: &str = include_str!("fixtures/format4_drawings_no_dims.json");
    const F5: &str = include_str!("fixtures/format5_drawings_centerlines.json");
    const F6: &str = include_str!("fixtures/format6_sheet_info.json");
    assert!(F4.contains("\"format\": 4") && F5.contains("\"format\": 5"));
    assert!(F6.contains("\"format\": 6") && F6.contains("blattangaben"));
    assert!(F6.contains("projektangaben") && F6.contains("Müller & Söhne"));
    for t in [F4, F5, F6] {
        assert!(!t.contains("stueckliste") && !t.contains("positionen"));
        let s = decode_version(t, "x.json").unwrap();
        assert!(s.parts_overrides.is_empty());
        assert!(
            s.drawings
                .iter()
                .all(|d| d.kind == DrawingKind::Views && d.balloons.is_empty())
        );
        assert_eq!(encode_version(&s).unwrap(), t);
        let d = Document::from_snapshot(s, 0.05).unwrap();
        assert_eq!(encode_version(&d.snapshot()).unwrap(), t);
    }
}

/// Dokument mit allem: Überschreibungen, drei Stücklistenblätter (von Hand, nicht abgeglichen),
/// Kreise mit Kennungslücke und verlorenem Körper, Schalter an.
fn full_doc() -> Document {
    let special = "ä ö ü ß Ø ° ± –";
    let mut d = with_box();
    let id = d
        .add_drawing_stamped("Blatt", A4, Projection::First, &stamp())
        .unwrap();
    let v1 = d
        .add_view(
            id,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 0.0, y: 0.0 },
            None,
        )
        .unwrap();
    let v2 = d
        .add_view(
            id,
            ViewKind::Top,
            Scale { num: 1, den: 2 },
            Placement::Free { x: 50.0, y: 5.0 },
            None,
        )
        .unwrap();
    let mut i = d.drawing(id).unwrap().info.clone().unwrap();
    i.parts_list = true;
    d.set_sheet_info(id, i).unwrap();
    d.set_parts_override(
        key(1, 0),
        RowOverride {
            name: text(special),
            qty: text("2 Stück"),
            material: text("S235 ±"),
            note: text("Bem. ä"),
        },
    )
    .unwrap();
    d.set_parts_override(key(7, 3), ov("Verloren")).unwrap();
    d.set_parts_override(
        key(2, 0),
        RowOverride {
            note: text(""),
            ..RowOverride::default()
        },
    )
    .unwrap();
    let b1 = d
        .add_balloon(id, v1, key(1, 0), (0.25, 0.75), (10.0, -12.5))
        .unwrap();
    let b2 = d
        .add_balloon(id, v2, key(99, 4), (1.0, 0.0), (-3.0, 4.0))
        .unwrap();
    let b3 = d
        .add_balloon(id, v1, key(1, 0), (0.5, 0.5), (0.0, 0.0))
        .unwrap();
    assert!(b1.0 < b2.0 && b2.0 < b3.0);
    d.update_drawing(id, |dr| dr.balloons.retain(|b| b.id != b2))
        .unwrap();
    // Zähler größer als der größte Wert (gelöschte Kennung)
    let b4 = d
        .add_balloon(id, v2, key(99, 4), (0.1, 0.9), (5.0, 5.0))
        .unwrap();
    d.update_drawing(id, |dr| dr.balloons.retain(|b| b.id != b4))
        .unwrap();
    assert_eq!(d.drawing(id).unwrap().balloons.len(), 2);

    // Drei Stücklistenblätter, ohne Abgleich eingesetzt (eine Zeile braucht nur eines).
    let mut s = d.snapshot();
    for p in 1..=3u32 {
        let mut z = Drawing::new(
            DrawingId(10 + u64::from(p)),
            parts_sheet_name(p),
            A4,
            Projection::First,
        );
        z.kind = DrawingKind::PartsList { page: p };
        z.info = s.drawings[0].info.clone();
        z.info.as_mut().unwrap().parts_list = false;
        s.drawings.push(z);
    }
    s.next_drawing = 20;
    Document::from_snapshot(s, 0.05).unwrap()
}

#[test]
fn full_round_trip() {
    let d = full_doc();
    assert_eq!(d.parts_sheets().len(), 3);
    let s = d.snapshot();
    assert!(s.drawings[0].next_balloon > 3);
    let text = encode_version(&s).unwrap();
    assert!(text.contains("\"format\": 7"));
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(back.parts_overrides, s.parts_overrides);
    assert_eq!(back.parts_overrides.len(), 3);
    assert_eq!(back.drawings, s.drawings);
    assert_eq!(back.next_drawing, s.next_drawing);
    assert!(back.drawings[0].info.as_ref().unwrap().parts_list);
    assert_eq!(
        back.drawings[0].balloons,
        vec![
            Balloon {
                id: BalloonId(1),
                view: ViewId(1),
                body: key(1, 0),
                anchor: (0.25, 0.75),
                offset: (10.0, -12.5)
            },
            Balloon {
                id: BalloonId(3),
                view: ViewId(1),
                body: key(1, 0),
                anchor: (0.5, 0.5),
                offset: (0.0, 0.0)
            },
        ]
    );
    assert_eq!(back.drawings[0].next_balloon, s.drawings[0].next_balloon);
    assert_eq!(back.drawings[2].kind, DrawingKind::PartsList { page: 2 });
    // Dokument: laden gleicht nichts ab (3 Blätter bleiben, obwohl eine Zeile nur eines braucht),
    // zweites Speichern ändert kein Byte.
    let doc = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(doc.parts_sheets().len(), 3);
    assert_eq!(doc.drawings(), d.drawings());
    assert_eq!(doc.parts_overrides(), d.parts_overrides());
    assert_eq!(encode_version(&doc.snapshot()).unwrap(), text);
    // Überschreibung mit leerem Text bleibt leerer Text, nicht automatisch
    assert_eq!(
        doc.parts_overrides()[&key(2, 0)].note,
        FieldValue::Text(String::new())
    );
}

/// Der Zähler allein (keine Kreise mehr) bleibt in einer Format-7-Datei erhalten.
#[test]
fn balloon_counter_survives_without_balloons() {
    let mut d = full_doc();
    let id = d.drawings()[0].id;
    d.update_drawing(id, |dr| dr.balloons.clear()).unwrap();
    let n = d.drawings()[0].next_balloon;
    assert!(n > 1);
    let s = d.snapshot();
    assert_eq!(required_format(&s), 7);
    let back = decode_version(&encode_version(&s).unwrap(), "x.json").unwrap();
    assert_eq!(back.drawings[0].next_balloon, n);
}

fn text7() -> String {
    encode_version(&full_doc().snapshot()).unwrap()
}

fn edited(f: impl FnOnce(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(&text7()).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

fn balloon_json(id: u64) -> Value {
    json!({"id": id, "ansicht": 1, "koerper": [1, 0], "anker": [0.5, 0.5], "versatz": [1.0, 1.0]})
}

#[test]
fn rejects_bad_parts_data() {
    // Format 6 mit genau einem Stücklisten-Feld: jedes allein wird abgelehnt (Basis: die
    // eingefrorene Format-6-Datei, ohne jedes andere Stücklisten-Feld).
    let base: Value =
        serde_json::from_str(include_str!("fixtures/format6_sheet_info.json")).unwrap();
    type Edit = fn(&mut Value);
    let cases: [(&str, Edit); 5] = [
        ("stuecklistenseite", |v| {
            v["zeichnungen"][0]["stuecklistenseite"] = 1.into()
        }),
        ("positionen", |v| {
            v["zeichnungen"][0]["positionen"] = json!([balloon_json(1)])
        }),
        ("naechste_position", |v| {
            v["zeichnungen"][0]["naechste_position"] = 5.into()
        }),
        ("stueckliste", |v| {
            v["stueckliste"] = json!([{"koerper": [1, 0], "benennung": "a"}])
        }),
        ("schalter", |v| {
            v["zeichnungen"][0]["blattangaben"]["stueckliste"] = true.into()
        }),
    ];
    assert!(decode_version(&base.to_string(), "x.json").is_ok());
    for (name, f) in cases {
        let mut v = base.clone();
        f(&mut v);
        let m = msg(bad(&serde_json::to_string_pretty(&v).unwrap()));
        assert!(m.contains("erst ab Dateiformat 7"), "{name}: {m}");
    }

    // Format 9 ist zu neu, Format 7 nicht
    let t = edited(|v| v["format"] = 9.into());
    assert!(matches!(bad(&t), ProjectError::TooNew { found: 9, .. }));
    assert!(decode_version(&text7(), "x.json").is_ok());

    // 201 Kreise
    let t = edited(|v| {
        v["zeichnungen"][0]["positionen"] =
            Value::Array((1..=(MAX_BALLOONS as u64 + 1)).map(balloon_json).collect());
        v["zeichnungen"][0]["naechste_position"] = 1000.into();
    });
    let m = msg(bad(&t));
    assert!(m.contains("Zeichnung „Blatt“") && m.contains("200"), "{m}");

    // Benennung 61 Zeichen, Steuerzeichen
    let t = edited(|v| v["stueckliste"][0]["benennung"] = "ä".repeat(61).into());
    let m = msg(bad(&t));
    assert!(m.contains("Stückliste") && m.contains("Benennung"), "{m}");
    let t = edited(|v| v["stueckliste"][0]["bemerkung"] = "a\u{1}b".into());
    assert!(msg(bad(&t)).contains("Stückliste"));

    // Seite 0, Stücklistenblatt mit Ansicht / Kreis
    let t = edited(|v| v["zeichnungen"][1]["stuecklistenseite"] = 0.into());
    let m = msg(bad(&t));
    assert!(
        m.contains("Zeichnung „Stückliste“") && m.contains("Seite"),
        "{m}"
    );
    let t = edited(|v| {
        v["zeichnungen"][1]["ansichten"] = v["zeichnungen"][0]["ansichten"].clone();
    });
    assert!(msg(bad(&t)).contains("Zeichnung „Stückliste“"));
    let t = edited(|v| {
        v["zeichnungen"][1]["positionen"] = json!([balloon_json(1)]);
        v["zeichnungen"][1]["naechste_position"] = 2.into();
    });
    assert!(msg(bad(&t)).contains("Zeichnung „Stückliste“"));

    // Anker NaN (null im JSON), außerhalb, riesiger Versatz
    let t = edited(|v| v["zeichnungen"][0]["positionen"][0]["anker"][0] = Value::Null);
    assert!(matches!(bad(&t), ProjectError::Corrupt { .. }));
    let t = edited(|v| v["zeichnungen"][0]["positionen"][0]["anker"][0] = 1.5.into());
    let m = msg(bad(&t));
    assert!(
        m.contains("Zeichnung „Blatt“") && m.contains("Positionsnummer"),
        "{m}"
    );
    let t = edited(|v| v["zeichnungen"][0]["positionen"][0]["versatz"][1] = 1e300.into());
    assert!(msg(bad(&t)).contains("Zeichnung „Blatt“"));

    // Falsche Typen / Schlüssel
    let t = edited(|v| v["zeichnungen"][0]["positionen"][0]["id"] = "eins".into());
    assert!(matches!(bad(&t), ProjectError::Corrupt { .. }));
    let t = edited(|v| v["zeichnungen"][0]["positionen"][0]["anker"] = json!([0.5]));
    assert!(matches!(bad(&t), ProjectError::Corrupt { .. }));
    let t = edited(|v| v["zeichnungen"][0]["positionen"][0]["koerper"] = json!([1, -1]));
    assert!(matches!(bad(&t), ProjectError::Corrupt { .. }));
    let t = edited(|v| v["stueckliste"][0]["menge"] = 5.into());
    assert!(matches!(bad(&t), ProjectError::Corrupt { .. }));
    let t = edited(|v| v["stueckliste"] = json!({"a": 1}));
    assert!(matches!(bad(&t), ProjectError::Corrupt { .. }));
    let t = edited(|v| v["zeichnungen"][1]["stuecklistenseite"] = (-1).into());
    assert!(matches!(bad(&t), ProjectError::Corrupt { .. }));

    // Körperindex über u32
    let t = edited(|v| v["stueckliste"][0]["koerper"] = json!([1, 4_294_967_296u64]));
    let m = msg(bad(&t));
    assert!(m.contains("Stückliste") && m.contains("Körper"), "{m}");
    let t =
        edited(|v| v["zeichnungen"][0]["positionen"][0]["koerper"] = json!([1, 4_294_967_296u64]));
    assert!(msg(bad(&t)).contains("Zeichnung „Blatt“"));

    // Doppelte Schlüssel, doppelte Kennung, Kennung 0, Zähler zu klein, Ansicht fehlt
    let t = edited(|v| {
        let first = v["stueckliste"][0].clone();
        v["stueckliste"].as_array_mut().unwrap().push(first);
    });
    let m = msg(bad(&t));
    assert!(m.contains("Stückliste") && m.contains("doppelt"), "{m}");
    let t = edited(|v| v["zeichnungen"][0]["positionen"][1]["id"] = 1.into());
    assert!(msg(bad(&t)).contains("Zeichnung „Blatt“"));
    let t = edited(|v| v["zeichnungen"][0]["positionen"][0]["id"] = 0.into());
    assert!(matches!(bad(&t), ProjectError::Corrupt { .. }));
    let t = edited(|v| v["zeichnungen"][0]["naechste_position"] = 3.into());
    assert!(matches!(bad(&t), ProjectError::Corrupt { .. }));
    let t = edited(|v| v["zeichnungen"][0]["positionen"][0]["ansicht"] = 77.into());
    assert!(msg(bad(&t)).contains("Zeichnung „Blatt“"));
    let t = edited(|v| v["zeichnungen"][0]["positionen"][0]["ansicht"] = 0.into());
    let m = msg(bad(&t));
    assert!(
        m.contains("Zeichnung „Blatt“") && m.contains("Positionsnummer"),
        "{m}"
    );

    // Eintrag ganz ohne eigene Texte
    let t = edited(|v| v["stueckliste"] = json!([{"koerper": [3, 0]}]));
    let m = msg(bad(&t));
    assert!(m.contains("Stückliste"), "{m}");

    // Seiten mit Lücke: das Dokument lehnt beim Laden ab
    let t = edited(|v| v["zeichnungen"][2]["stuecklistenseite"] = 5.into());
    let s = decode_version(&t, "x.json").unwrap();
    assert!(Document::from_snapshot(s, 0.05).is_err());
}

#[test]
fn hostile_sizes_are_cut_off() {
    // Riesige Listen: benannter Fehler, ohne dass die Liste angelegt wird.
    let t = edited(|v| {
        v["stueckliste"] = Value::Array(
            (0..200_000u64)
                .map(|k| json!({"koerper": [k, 0], "benennung": "x"}))
                .collect(),
        );
    });
    let m = msg(bad(&t));
    assert!(
        m.contains("Stückliste") && m.contains(&MAX_OVERRIDES.to_string()),
        "{m}"
    );
    let n = MAX_OVERRIDES as u64;
    let t = edited(|v| {
        v["stueckliste"] = Value::Array(
            (0..=n)
                .map(|k| json!({"koerper": [k, 0], "benennung": "x"}))
                .collect(),
        );
    });
    assert!(msg(bad(&t)).contains("Stückliste"));
    // genau die Höchstzahl lädt
    let t = edited(|v| {
        v["stueckliste"] = Value::Array(
            (0..n)
                .map(|k| json!({"koerper": [k, 0], "benennung": "x"}))
                .collect(),
        );
    });
    assert_eq!(
        decode_version(&t, "x.json").unwrap().parts_overrides.len(),
        MAX_OVERRIDES
    );
    let t = edited(|v| {
        v["zeichnungen"][0]["positionen"] =
            Value::Array((0..300_000).map(|k| balloon_json(k + 1)).collect());
    });
    let m = msg(bad(&t));
    assert!(m.contains("Zeichnung „Blatt“") && m.contains("200"), "{m}");
    // genau 200 Kreise laden
    let t = edited(|v| {
        v["zeichnungen"][0]["positionen"] = Value::Array((1..=200u64).map(balloon_json).collect());
        v["zeichnungen"][0]["naechste_position"] = 201.into();
    });
    assert_eq!(
        decode_version(&t, "x.json").unwrap().drawings[0]
            .balloons
            .len(),
        200
    );
}

#[test]
fn encode_refuses_invalid_numbers() {
    let mut s = full_doc().snapshot();
    s.drawings[0].balloons[0].anchor.0 = f64::NAN;
    assert!(encode_version(&s).is_err());
}

#[test]
fn unknown_fields_are_ignored() {
    let t = edited(|v| {
        v["zeichnungen"][0]["positionen"][0]["zukunft"] = "egal".into();
        v["stueckliste"][0]["neu"] = 1.into();
        v["zeichnungen"][0]["blattangaben"]["zusatz"] = true.into();
    });
    let s = decode_version(&t, "x.json").unwrap();
    let d = Document::from_snapshot(s, 0.05).unwrap();
    assert_eq!(d.drawings()[0].balloons.len(), 2);
    assert_eq!(d.parts_overrides().len(), 3);
}

#[test]
fn loading_does_not_alter_anything() {
    // Stücklistenblätter, deren Zahl nicht zu den Zeilen passt, und ein Kreis auf einen fehlenden
    // Körper: nichts wird abgeglichen, bereinigt oder neu nummeriert, kein Änderungszähler.
    let d = full_doc();
    let text = encode_version(&d.snapshot()).unwrap();
    let doc = Document::from_snapshot(decode_version(&text, "x.json").unwrap(), 0.05).unwrap();
    assert_eq!(doc.drawings().len(), d.drawings().len());
    assert_eq!(doc.edit_count(), 0);
    assert_eq!(doc.parts_overrides().len(), 3);
}

fn store_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "planora-parts-{}.pcad",
        planora_project::fsutil::new_id()
    ))
}

/// Versionsspeicher, Wiederherstellen und Autosicherung tragen Überschreibungen, Kreise und
/// Stücklistenblätter unverändert; eine ältere Version ohne diese Daten bleibt Format 1.
#[test]
fn store_restore_and_autosave_carry_parts_data() {
    use planora_project::docstore::{DocStore, LockMode, MAIN_BRANCH};
    let full = full_doc().snapshot();
    let old = with_box().snapshot();
    let p = store_dir();
    let mut st = DocStore::create(&p, &old, "alt").unwrap();
    let v2 = st.save(Some(1), &full, "voll", None).unwrap();
    let back = st.load_version(v2).unwrap();
    assert_eq!(back.parts_overrides, full.parts_overrides);
    assert_eq!(back.drawings, full.drawings);
    let v1_text = std::fs::read_to_string(p.join("versionen").join("0001.json")).unwrap();
    assert!(v1_text.contains("\"format\": 1") && !v1_text.contains("stueckliste"));
    let v2_text = std::fs::read_to_string(p.join("versionen").join("0002.json")).unwrap();
    assert!(v2_text.contains("\"format\": 7") && v2_text.contains("stueckliste"));
    // Wiederherstellen der vollen Version auf den Hauptzweig
    let v3 = st.restore(v2, MAIN_BRANCH).unwrap();
    let r = st.load_version(v3).unwrap();
    assert_eq!(r.parts_overrides, full.parts_overrides);
    assert_eq!(r.drawings, full.drawings);
    // Autosicherung
    st.write_autosave(Some(v3), &full).unwrap();
    let a = st.read_autosave().unwrap().unwrap();
    assert_eq!(a.snapshot.parts_overrides, full.parts_overrides);
    assert_eq!(a.snapshot.drawings, full.drawings);
    assert_eq!(a.snapshot.next_drawing, full.next_drawing);
    drop(st);
    let (st, _) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert_eq!(
        st.load_version(1).unwrap().parts_overrides.len(),
        0,
        "alte Version bleibt ohne Stücklistendaten"
    );
    let _ = std::fs::remove_dir_all(&p);
}

/// Alle Kreise gelöscht, sonst nichts mit Format 7: der Zähler wird nicht geschrieben, die Datei
/// bleibt Format 4 und lässt sich wieder lesen.
#[test]
fn deleted_balloons_do_not_force_format_7() {
    let (mut d, id, v) = plain();
    d.add_balloon(id, v, key(1, 0), (0.5, 0.5), (10.0, 10.0))
        .unwrap();
    d.update_drawing(id, |dr| dr.balloons.clear()).unwrap();
    assert!(d.drawings()[0].next_balloon > 1);
    let s = d.snapshot();
    assert_eq!(required_format(&s), 4);
    let t = encode_version(&s).unwrap();
    assert!(t.contains("\"format\": 4") && !t.contains("naechste_position"));
    let back = decode_version(&t, "x.json").unwrap();
    assert_eq!(back.drawings[0].next_balloon, 1);
}

/// Überschreibungen allein, ohne jede Zeichnung: gehen nicht verloren, Format 7.
#[test]
fn overrides_without_drawings_round_trip() {
    let mut d = with_box();
    d.set_parts_override(key(1, 0), ov("Welle ä")).unwrap();
    d.set_parts_override(key(5, 2), ov("Weg")).unwrap();
    assert!(d.drawings().is_empty());
    let t = encode_version(&d.snapshot()).unwrap();
    assert!(t.contains("\"format\": 7") && t.contains("stueckliste"));
    let back = decode_version(&t, "x.json").unwrap();
    assert_eq!(&back.parts_overrides, d.parts_overrides());
    assert_eq!(back.parts_overrides.len(), 2);
}

/// Eingefrorene Format-7-Datei (mit dem fertigen Schreiber erzeugt): Überschreibungen, ein
/// Stücklistenblatt, Kreise mit Lücke und Zähler, Schalter, dazu eine Zeichnung ohne Kreise.
/// Lesen und Wiederschreiben ändert kein Byte.
#[test]
fn format_7_file_stays_byte_identical() {
    const F7: &str = include_str!("fixtures/format7_parts.json");
    assert!(F7.contains("\"format\": 7"));
    for k in [
        "\"stueckliste\": [",
        "\"stuecklistenseite\": 1",
        "\"positionen\": [",
        "\"naechste_position\": 4",
        "\"stueckliste\": true",
    ] {
        assert!(F7.contains(k), "{k}");
    }
    let s = decode_version(F7, "x.json").unwrap();
    assert_eq!(s.parts_overrides.len(), 2);
    assert_eq!(encode_version(&s).unwrap(), F7);
    let d = Document::from_snapshot(s, 0.05).unwrap();
    assert_eq!(encode_version(&d.snapshot()).unwrap(), F7);
}
