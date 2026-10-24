//! Dateiformat 4 (seit Teilprojekt 12 mit Mittellinien Format 5): Zeichnungen in der Versionsdatei. Rundreise, kleinste passende Version, alte
//! Dateien Byte für Byte, feindliche Dateien mit benannten Fehlern (Teilprojekt 11, Task 13).

use planora_drawing::{
    Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
};
use planora_feature::*;
use planora_math::DVec3;
use planora_project::docstore::{DocStore, LockMode};
use planora_project::fsutil::new_id;
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use serde_json::{Value, json};

// ---- Aufbau ----------------------------------------------------------------------------

fn box_document() -> Document {
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

fn free(x: f64, y: f64) -> Placement {
    Placement::Free { x, y }
}

fn scale(num: u32, den: u32) -> Scale {
    Scale { num, den }
}

/// Zwei Zeichnungen (eine gelöschte dazwischen, also Lücke in den Kennungen), Ansichten aller
/// Arten, freie und ausgerichtete Lage, verdeckte Kanten an und aus, eine gelöschte Ansicht.
/// Normansichten zeigen nach Vorgabe Mittellinien; die Datei hat deshalb Format 5.
fn drawing_document() -> Document {
    let mut d = box_document();
    let a = d
        .add_drawing(
            "Blatt 1",
            Sheet {
                paper: Paper::A3,
                landscape: true,
            },
            Projection::First,
        )
        .unwrap();
    let gone = d
        .add_drawing(
            "Weg",
            Sheet {
                paper: Paper::A4,
                landscape: false,
            },
            Projection::First,
        )
        .unwrap();
    let b = d
        .add_drawing(
            "Blatt 2",
            Sheet {
                paper: Paper::A0,
                landscape: false,
            },
            Projection::Third,
        )
        .unwrap();
    d.delete_drawing(gone).unwrap();
    let front = d
        .add_view(
            a,
            ViewKind::Front,
            scale(1, 2),
            free(0.1, 12345.678901234),
            None,
        )
        .unwrap();
    for (i, k) in [
        ViewKind::Back,
        ViewKind::Left,
        ViewKind::Right,
        ViewKind::Top,
        ViewKind::Bottom,
    ]
    .into_iter()
    .enumerate()
    {
        d.add_view(
            a,
            k,
            scale(1, 2),
            Placement::Aligned {
                distance: 20.5 + i as f64,
            },
            Some(front),
        )
        .unwrap();
    }
    let iso = d
        .add_view(
            a,
            ViewKind::Isometric,
            scale(5, 1),
            free(-0.0, -99999.5),
            None,
        )
        .unwrap();
    // Gelöschte Ansicht: der Zähler liegt danach über der größten Kennung + 1.
    let tmp = d
        .add_view(a, ViewKind::Front, scale(1, 1), free(1.0, 1.0), None)
        .unwrap();
    d.update_drawing(a, |dr| {
        dr.remove_view(tmp);
        for v in &mut dr.views {
            v.show_hidden = v.id == iso || v.kind == ViewKind::Left;
        }
    })
    .unwrap();
    let top = d
        .add_view(
            b,
            ViewKind::Top,
            scale(1000, 1),
            free(100000.0, -100000.0),
            None,
        )
        .unwrap();
    d.add_view(
        b,
        ViewKind::Front,
        scale(1, 1000),
        Placement::Aligned { distance: -0.0 },
        Some(top),
    )
    .unwrap();
    d
}

fn snapshot_of(d: &Document) -> Snapshot {
    d.snapshot()
}

fn good() -> String {
    encode_version(&snapshot_of(&drawing_document())).unwrap()
}

fn bad(text: &str) -> ProjectError {
    decode_version(text, "x.json").unwrap_err()
}

/// Laden wie in der Anwendung: Datei lesen, dann `Document::from_snapshot` (Fehler → Invalid).
fn load(text: &str) -> Result<Document, ProjectError> {
    let s = decode_version(text, "x.json")?;
    Document::from_snapshot(s, 0.05).map_err(|e| ProjectError::Invalid(e.to_string()))
}

fn edited(text: &str, f: impl FnOnce(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(text).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

fn view0(v: &mut Value) -> &mut Value {
    &mut v["zeichnungen"][0]["ansichten"][0]
}

/// Ersetzt eine Zahl, die `serde_json::Value` nicht darstellen kann, im Text.
fn with_raw_number(text: &str, path: impl FnOnce(&mut Value) -> &mut Value, raw: &str) -> String {
    const MARK: f64 = 4242.125;
    let t = edited(text, |v| *path(v) = json!(MARK));
    assert_eq!(t.matches("4242.125").count(), 1);
    t.replacen("4242.125", raw, 1)
}

fn corrupt_msg(e: ProjectError) -> String {
    match e {
        ProjectError::Corrupt { msg, .. } => msg,
        other => panic!("beschädigt erwartet, nicht {other:?}"),
    }
}

// ---- (a) Rundreise -----------------------------------------------------------------------

#[test]
fn snapshot_with_drawings_roundtrips_exactly() {
    let d = drawing_document();
    let s = d.snapshot();
    assert_eq!(s.drawings.len(), 2);
    assert_eq!(s.next_drawing, 4);
    assert_eq!(s.drawings[0].views.len(), 7);
    assert_eq!(s.drawings[0].next_view, 9);
    let text = encode_version(&s).unwrap();
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(back.drawings, s.drawings);
    assert_eq!(back.next_drawing, s.next_drawing);
    assert_eq!(back.features.len(), s.features.len());
    assert_eq!(encode_version(&back).unwrap(), text, "Rundreise stabil");
    let doc = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(doc.drawings(), d.drawings());
    // Alle Ansichtsarten und beide Lagen stehen in der Datei.
    for art in ["vorne", "hinten", "links", "rechts", "oben", "unten", "iso"] {
        assert!(text.contains(&format!("\"art\": \"{art}\"")), "{art}");
    }
    assert!(text.contains("\"typ\": \"frei\"") && text.contains("\"typ\": \"ausgerichtet\""));
    assert!(
        text.contains("\"projektion\": \"erst\"") && text.contains("\"projektion\": \"dritt\"")
    );
    assert!(text.contains("\"naechste_zeichnung\": 4"));
}

#[test]
fn every_paper_projection_and_kind_roundtrips() {
    let mut d = box_document();
    let papers = [Paper::A4, Paper::A3, Paper::A2, Paper::A1, Paper::A0];
    let mut n = 0;
    for p in papers {
        for landscape in [false, true] {
            for proj in [Projection::First, Projection::Third] {
                n += 1;
                let id = d
                    .add_drawing(
                        &format!("Z{n}"),
                        Sheet {
                            paper: p,
                            landscape,
                        },
                        proj,
                    )
                    .unwrap();
                for k in ViewKind::ALL {
                    d.add_view(id, k, scale(1, 1), free(n as f64, -(n as f64)), None)
                        .unwrap();
                }
            }
        }
    }
    let s = d.snapshot();
    let back = decode_version(&encode_version(&s).unwrap(), "x.json").unwrap();
    assert_eq!(back.drawings, s.drawings);
    assert_eq!(back.drawings.len(), 20);
}

#[test]
fn positions_roundtrip_bit_exact() {
    let values = [
        0.1,
        -0.1,
        12345.678901234,
        99999.99999999999,
        1.0e5,
        -1.0e5,
        5e-324,
        f64::MIN_POSITIVE,
        1.0 / 3.0,
        -0.0,
        0.0,
    ];
    let mut d = box_document();
    let id = d
        .add_drawing(
            "Z",
            Sheet {
                paper: Paper::A4,
                landscape: false,
            },
            Projection::First,
        )
        .unwrap();
    for (i, &x) in values.iter().enumerate() {
        let y = values[values.len() - 1 - i];
        let v = d
            .add_view(id, ViewKind::Front, scale(1, 1), free(x, y), None)
            .unwrap();
        d.add_view(
            id,
            ViewKind::Top,
            scale(1, 1),
            Placement::Aligned { distance: x },
            Some(v),
        )
        .unwrap();
    }
    let s = d.snapshot();
    let back = decode_version(&encode_version(&s).unwrap(), "x.json").unwrap();
    let bits = |dr: &Drawing| -> Vec<u64> {
        dr.views
            .iter()
            .flat_map(|v| match v.placement {
                Placement::Free { x, y } => vec![x.to_bits(), y.to_bits()],
                Placement::Aligned { distance } => vec![distance.to_bits()],
            })
            .collect()
    };
    assert_eq!(bits(&back.drawings[0]), bits(&s.drawings[0]));
    // -0.0 bleibt negativ null (Vorzeichenbit erhalten).
    match back.drawings[0].views[0].placement {
        Placement::Free { x, .. } => assert_eq!(x.to_bits(), 0.1f64.to_bits()),
        _ => panic!(),
    }
    let neg = back.drawings[0]
        .views
        .iter()
        .find_map(|v| match v.placement {
            Placement::Aligned { distance } if distance == 0.0 && distance.is_sign_negative() => {
                Some(())
            }
            _ => None,
        });
    assert!(neg.is_some(), "-0.0 als Abstand muss -0.0 bleiben");
}

// ---- (b) Formatnummer --------------------------------------------------------------------

#[test]
fn format_4_with_drawings_and_5_with_centerlines() {
    // Format 6 gilt seit Teilprojekt 3c-1 (Blattangaben), Format 7 seit 3c-2; Mittellinien brauchen Format 5.
    assert_eq!(FORMAT_VERSION, 11);
    let plain = box_document();
    assert_eq!(required_format(&plain.snapshot()), 1);
    let mut d = drawing_document();
    assert_eq!(required_format(&d.snapshot()), 5);
    let text = encode_version(&d.snapshot()).unwrap();
    assert!(text.contains("\"format\": 5"), "{text}");
    // Eine leere Zeichnung reicht für Format 4.
    let mut e = box_document();
    e.add_drawing(
        "Leer",
        Sheet {
            paper: Paper::A4,
            landscape: false,
        },
        Projection::First,
    )
    .unwrap();
    assert_eq!(required_format(&e.snapshot()), 4);
    let t = encode_version(&e.snapshot()).unwrap();
    assert!(t.contains("\"ansichten\": []"), "{t}");
    assert_eq!(
        decode_version(&t, "x").unwrap().drawings,
        e.snapshot().drawings
    );
    // Alle Zeichnungen gelöscht: wieder das alte Format ohne Zeichnungsfelder.
    let ids: Vec<DrawingId> = d.drawings().iter().map(|z| z.id).collect();
    for id in ids {
        d.delete_drawing(id).unwrap();
    }
    assert_eq!(required_format(&d.snapshot()), 1);
    let t = encode_version(&d.snapshot()).unwrap();
    assert!(t.contains("\"format\": 1"), "{t}");
    assert!(!t.contains("zeichnung"), "{t}");
    assert_eq!(t, encode_version(&box_document().snapshot()).unwrap());
}

// ---- (c) Alte Dateien Byte für Byte ------------------------------------------------------

#[test]
fn old_files_stay_byte_for_byte() {
    let cases = [
        (
            include_str!("data/format1_voll.json"),
            include_str!("fixtures/format1_no_drawings.json"),
            "\"format\": 1",
        ),
        (
            include_str!("data/format2.json"),
            include_str!("fixtures/format2_no_drawings.json"),
            "\"format\": 2",
        ),
        (
            include_str!("data/format3.json"),
            include_str!("fixtures/format3_no_drawings.json"),
            "\"format\": 3",
        ),
    ];
    for (source, fixture, format) in cases {
        assert!(!fixture.contains('\r'), "Fixtur mit LF");
        let doc = Document::from_snapshot(decode_version(source, "alt").unwrap(), 0.05).unwrap();
        let s = doc.snapshot();
        assert!(s.drawings.is_empty());
        let text = encode_version(&s).unwrap();
        assert_eq!(text, fixture, "{format}");
        assert!(text.contains(format));
        assert!(!text.contains("zeichnungen") && !text.contains("naechste_zeichnung"));
        // Die Fixtur selbst liest sich und schreibt sich unverändert zurück.
        let back = decode_version(fixture, "fixtur").unwrap();
        assert!(back.drawings.is_empty());
        assert_eq!(back.next_drawing, 1);
        assert_eq!(encode_version(&back).unwrap(), fixture, "{format}");
    }
}

#[test]
fn old_files_with_unknown_fields_stay_readable() {
    let fixture = include_str!("fixtures/format3_no_drawings.json");
    let t = edited(fixture, |v| {
        v["zukunft"] = json!({"etwas": [1, 2, 3]});
        v["features"][0]["neu"] = json!("x");
    });
    let s = decode_version(&t, "x.json").unwrap();
    Document::from_snapshot(s, 0.05).unwrap();
    // Auch in einer Zeichnung und Ansicht werden unbekannte Felder übergangen.
    let t = edited(&good(), |v| {
        v["zeichnungen"][0]["kommentar"] = json!("neu");
        v["zeichnungen"][0]["blatt"]["rand"] = json!(10);
        view0(v)["farbe"] = json!("rot");
        view0(v)["lage"]["winkel"] = json!(0.5);
    });
    let doc = load(&t).unwrap();
    assert_eq!(doc.drawings(), drawing_document().drawings());
}

// ---- (d) Format 3 mit Zeichnungen --------------------------------------------------------

#[test]
fn format_3_file_with_drawings_is_refused() {
    let good = good();
    assert!(good.contains("\"format\": 5"));
    for older in ["\"format\": 1", "\"format\": 2", "\"format\": 3"] {
        let msg = corrupt_msg(bad(&good.replacen("\"format\": 5", older, 1)));
        assert!(msg.contains("Dateiformat 4"), "{msg}");
    }
    // Nur der Zähler, aber Format 3: ebenso beschädigt.
    let t = edited(include_str!("fixtures/format3_no_drawings.json"), |v| {
        v["naechste_zeichnung"] = json!(5);
    });
    assert!(corrupt_msg(bad(&t)).contains("Dateiformat 4"));
}

// ---- (e) Feindliche Dateien --------------------------------------------------------------

#[test]
fn hostile_files_are_refused_not_panicking() {
    let good = good();
    load(&good).unwrap();
    let raw = |p: fn(&mut Value) -> &mut Value, r: &str| with_raw_number(&good, p, r);
    fn x(v: &mut Value) -> &mut Value {
        &mut view0(v)["lage"]["x"]
    }
    fn abstand(v: &mut Value) -> &mut Value {
        &mut v["zeichnungen"][0]["ansichten"][1]["lage"]["abstand"]
    }
    fn id(v: &mut Value) -> &mut Value {
        &mut v["zeichnungen"][0]["id"]
    }
    // (Text, erwarteter Teil der Meldung) – alle beim Lesen als beschädigt erkannt.
    let corrupt: Vec<(String, &str)> = vec![
        (raw(x, "1e999"), ""),
        (raw(x, "-1e999"), ""),
        (raw(abstand, "1e999"), ""),
        (raw(x, "1e300"), "Zahl ungültig oder zu groß"),
        (raw(abstand, "-1e300"), "Zahl ungültig oder zu groß"),
        (raw(x, "100000.001"), "Zahl ungültig oder zu groß"),
        (edited(&good, |v| *x(v) = Value::Null), ""),
        (edited(&good, |v| *x(v) = json!("1")), ""),
        (
            edited(&good, |v| view0(v)["art"] = json!("seitlich")),
            "Unbekannte Ansichtsart „seitlich“",
        ),
        (
            edited(&good, |v| view0(v)["art"] = json!("Vorne")),
            "Unbekannte Ansichtsart",
        ),
        (edited(&good, |v| view0(v)["art"] = json!(3)), ""),
        (
            edited(&good, |v| {
                v["zeichnungen"][0]["blatt"]["papier"] = json!("A5")
            }),
            "Unbekanntes Papierformat „A5“",
        ),
        (
            edited(&good, |v| {
                v["zeichnungen"][0]["projektion"] = json!("zweit")
            }),
            "Unbekannte Projektion „zweit“",
        ),
        (
            edited(&good, |v| view0(v)["lage"]["typ"] = json!("schief")),
            "",
        ),
        (
            edited(&good, |v| view0(v)["lage"] = json!({"x": 1.0, "y": 2.0})),
            "",
        ),
        (
            edited(&good, |v| view0(v)["massstab"] = json!([0, 1])),
            "Maßstab",
        ),
        (
            edited(&good, |v| view0(v)["massstab"] = json!([1, 0])),
            "Maßstab",
        ),
        (
            edited(&good, |v| view0(v)["massstab"] = json!([1, 1001])),
            "Maßstab",
        ),
        (
            edited(&good, |v| view0(v)["massstab"] = json!([1001, 1])),
            "Maßstab",
        ),
        (edited(&good, |v| view0(v)["massstab"] = json!([-1, 1])), ""),
        (edited(&good, |v| view0(v)["massstab"] = json!([1])), ""),
        (edited(&good, |v| view0(v)["massstab"] = json!([])), ""),
        (
            edited(&good, |v| view0(v)["massstab"] = json!([1, 2, 3])),
            "",
        ),
        (
            edited(&good, |v| view0(v)["massstab"] = json!([4294967296u64, 1])),
            "",
        ),
        (
            edited(&good, |v| view0(v)["verdeckt_zeigen"] = json!("ja")),
            "",
        ),
        (edited(&good, |v| view0(v)["eltern"] = json!(-3)), ""),
        (edited(&good, |v| view0(v)["id"] = json!(-1)), ""),
        (
            edited(&good, |v| view0(v)["id"] = json!(0)),
            "ungültige Kennung",
        ),
        (
            edited(&good, |v| view0(v)["id"] = json!(u64::MAX)),
            "ungültige Kennung",
        ),
        (
            edited(&good, |v| view0(v)["id"] = json!(9)),
            "ungültige Kennung",
        ),
        (
            edited(&good, |v| {
                v["zeichnungen"][0]["naechste_ansicht"] = json!(u64::MAX)
            }),
            "Zähler ist ungültig",
        ),
        (
            edited(&good, |v| {
                v["zeichnungen"][0]["naechste_ansicht"] = json!(0);
                view0(v)["id"] = json!(u64::MAX);
            }),
            "ungültige Kennung",
        ),
        (edited(&good, |v| *id(v) = json!(-1)), ""),
        (edited(&good, |v| *id(v) = json!(0)), "ungültige Kennung"),
        (edited(&good, |v| *id(v) = json!(4)), "ungültige Kennung"),
        (
            edited(&good, |v| *id(v) = json!(u64::MAX)),
            "ungültige Kennung",
        ),
        (
            edited(&good, |v| v["naechste_zeichnung"] = json!(1)),
            "ungültige Kennung",
        ),
        (
            edited(&good, |v| v["naechste_zeichnung"] = json!(3)),
            "ungültige Kennung",
        ),
        (
            edited(&good, |v| v["naechste_zeichnung"] = json!(u64::MAX)),
            "Zähler ist ungültig",
        ),
        (edited(&good, |v| v["naechste_zeichnung"] = json!(-1)), ""),
        (
            edited(&good, |v| {
                v["naechste_zeichnung"] = json!(0);
                *id(v) = json!(u64::MAX);
            }),
            "ungültige Kennung",
        ),
        (edited(&good, |v| v["zeichnungen"] = Value::Null), ""),
        (edited(&good, |v| v["zeichnungen"] = json!({})), ""),
        (
            edited(&good, |v| v["zeichnungen"][0]["ansichten"] = json!("x")),
            "",
        ),
        (
            edited(&good, |v| v["zeichnungen"][0]["name"] = json!(5)),
            "",
        ),
        (
            edited(&good, |v| v["zeichnungen"][0]["blatt"]["quer"] = json!(1)),
            "",
        ),
        (
            edited(&good, |v| v["zeichnungen"][0]["blatt"] = Value::Null),
            "",
        ),
        (
            edited(&good, |v| {
                let one = v["zeichnungen"][0].clone();
                let list: Vec<Value> = (1..=1001)
                    .map(|i| {
                        let mut z = one.clone();
                        z["id"] = json!(i);
                        z["name"] = json!(format!("Z{i}"));
                        z
                    })
                    .collect();
                v["zeichnungen"] = Value::Array(list);
                v["naechste_zeichnung"] = json!(2000);
            }),
            "zu viele Einträge",
        ),
        (
            edited(&good, |v| {
                let one = view0(v).clone();
                let list: Vec<Value> = (1..=65)
                    .map(|i| {
                        let mut a = one.clone();
                        a["id"] = json!(i);
                        a
                    })
                    .collect();
                v["zeichnungen"][0]["ansichten"] = Value::Array(list);
                v["zeichnungen"][0]["naechste_ansicht"] = json!(100);
            }),
            "zu viele Einträge",
        ),
    ];
    for (i, (text, want)) in corrupt.into_iter().enumerate() {
        assert_ne!(text, good, "Fall {i}: Änderung muss greifen");
        let e = bad(&text);
        let full = e.to_string();
        assert!(
            full.starts_with("Die Datei „x.json“ ist beschädigt: "),
            "Fall {i}: {full}"
        );
        let msg = corrupt_msg(e);
        assert!(!msg.is_empty() && msg.contains(want), "Fall {i}: {msg}");
    }

    // Beim Lesen formal in Ordnung, aber von der Dokumentprüfung abgelehnt (Invalid).
    let invalid: Vec<(String, &str)> = vec![
        (
            edited(&good, |v| {
                let a = v["zeichnungen"][0]["ansichten"][0].clone();
                v["zeichnungen"][0]["ansichten"][1] = a;
            }),
            "kommt doppelt vor",
        ),
        (
            edited(&good, |v| {
                v["zeichnungen"][0]["ansichten"][1]["eltern"] = json!(8)
            }),
            "Ausgangsansicht",
        ),
        (
            edited(&good, |v| {
                v["zeichnungen"][0]["ansichten"][1]["eltern"] = Value::Null
            }),
            "ohne Ausgangsansicht",
        ),
        (
            edited(&good, |v| view0(v)["eltern"] = json!(2)),
            "abgeleitete Ansicht",
        ),
        (
            edited(&good, |v| {
                // Iso als Ausgangsansicht
                v["zeichnungen"][0]["ansichten"][1]["eltern"] = json!(7);
                let iso = v["zeichnungen"][0]["ansichten"][6].clone();
                let arr = v["zeichnungen"][0]["ansichten"].as_array_mut().unwrap();
                arr.remove(6);
                arr.insert(0, iso);
            }),
            "isometrischen",
        ),
        (
            edited(&good, |v| v["zeichnungen"][0]["name"] = json!("")),
            "Namen",
        ),
        (
            edited(&good, |v| v["zeichnungen"][0]["name"] = json!("   ")),
            "Namen",
        ),
        (
            edited(&good, |v| {
                v["zeichnungen"][0]["name"] = json!("x".repeat(201))
            }),
            "zu lang",
        ),
        (
            edited(&good, |v| v["zeichnungen"][1]["name"] = json!("blatt 1")),
            "Zeichnungsname kommt doppelt vor",
        ),
        (
            edited(&good, |v| v["zeichnungen"][1]["id"] = json!(1)),
            "Zeichnungskennung kommt doppelt vor",
        ),
    ];
    for (i, (text, want)) in invalid.into_iter().enumerate() {
        assert_ne!(text, good, "Fall {i}: Änderung muss greifen");
        decode_version(&text, "x.json").unwrap();
        match load(&text) {
            Err(ProjectError::Invalid(msg)) => assert!(msg.contains(want), "Fall {i}: {msg}"),
            other => panic!("Fall {i}: {other:?}"),
        }
    }
}

#[test]
fn missing_counters_are_derived_from_the_ids() {
    let good = good();
    let t = edited(&good, |v| {
        v.as_object_mut().unwrap().remove("naechste_zeichnung");
        v["zeichnungen"][0]
            .as_object_mut()
            .unwrap()
            .remove("naechste_ansicht");
        v["zeichnungen"][1]["naechste_ansicht"] = json!(0);
    });
    let s = decode_version(&t, "x.json").unwrap();
    assert_eq!(s.next_drawing, 4, "größte Kennung 3 + 1");
    assert_eq!(s.drawings[0].next_view, 8, "größte Ansichtskennung 7 + 1");
    assert_eq!(s.drawings[1].next_view, 3);
    // Leere Ansichtsliste ohne Zähler: 1
    let t = edited(&good, |v| {
        v["naechste_zeichnung"] = json!(0);
        v["zeichnungen"][0]["ansichten"] = json!([]);
        v["zeichnungen"][0]["naechste_ansicht"] = json!(0);
    });
    let s = decode_version(&t, "x.json").unwrap();
    assert_eq!(s.drawings[0].next_view, 1);
    assert!(s.drawings[0].views.is_empty());
    load(&t).unwrap();
    // Format 4 mit leerer Zeichnungsliste ist in Ordnung.
    let t = edited(&good, |v| v["zeichnungen"] = json!([]));
    let s = decode_version(&t, "x.json").unwrap();
    assert!(s.drawings.is_empty());
}

// ---- (f) Neuere Formate ------------------------------------------------------------------

#[test]
fn too_new_format_is_refused_with_clear_text() {
    // Seit den Rückrollstellen ist Format 11 gültig; „zu neu“ ist jetzt 12.
    let good = good();
    let e = bad(&good.replacen("\"format\": 5", "\"format\": 12", 1));
    assert!(matches!(e, ProjectError::TooNew { found: 12, .. }), "{e:?}");
    let msg = e.to_string();
    assert!(
        msg.contains("neueren Programmversion") && msg.contains("Format 12"),
        "{msg}"
    );
    // Auch eine Datei, deren übrige Struktur unbekannt ist.
    let e = bad("{\"format\": 12, \"zeichnungen\": \"ganz anders\"}");
    assert!(matches!(e, ProjectError::TooNew { found: 12, .. }), "{e:?}");
}

// ---- Versionsspeicher und Autosave ------------------------------------------------------

#[test]
fn versions_and_autosave_keep_drawings() {
    let p = std::env::temp_dir().join(format!("planora-draw-{}.pcad", new_id()));
    let d = drawing_document();
    let s = d.snapshot();
    let mut st = DocStore::create(&p, &s, "Mit Zeichnungen").unwrap();
    let plain = box_document().snapshot();
    st.save(Some(1), &plain, "", None).unwrap();
    st.write_autosave(Some(2), &s).unwrap();
    drop(st);
    let (st, _) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert_eq!(st.load_version(1).unwrap().drawings, s.drawings);
    assert!(st.load_version(2).unwrap().drawings.is_empty());
    let auto = st.read_autosave().unwrap().unwrap();
    assert_eq!(auto.snapshot.drawings, s.drawings);
    assert_eq!(auto.snapshot.next_drawing, s.next_drawing);
    drop(st);
    let _ = std::fs::remove_dir_all(&p);
    // Kennungen bleiben auch nach dem Laden eindeutig.
    let back = Document::from_snapshot(decode_version(&good(), "x").unwrap(), 0.05).unwrap();
    assert_eq!(back.drawing(DrawingId(3)).unwrap().views[0].id, ViewId(1));
}
