//! Dateiformat 5: Maße und Mittellinien in der Versionsdatei. Alte Dateien Byte für Byte,
//! kleinste passende Version, Rundreise aller Arten mit bitgenauen Zahlen, feindliche Dateien
//! mit benannten Fehlern (Teilprojekt 12, Aufgabe 9).

use planora_drawing::dimension::{
    ChainMode, ChainSpec, DimId, DimKind, DimSpec, EndSel, GeoRef, GeoRefData, TagData,
    Tolerance as DimTolerance,
};
use planora_drawing::{Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_feature::*;
use planora_math::DVec3;
use planora_project::docstore::{DocStore, LockMode};
use planora_project::fsutil::new_id;
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("fixtures/format4_drawings_no_dims.json");

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

fn scale(num: u32, den: u32) -> Scale {
    Scale { num, den }
}

/// Derselbe Aufbau wie beim Erzeugen der Fixtur (alle Ansichtsarten, Mittellinien aus).
fn fixture_document() -> (Document, DrawingId) {
    let mut d = box_document();
    let z = d
        .add_drawing(
            "Blatt 1",
            Sheet {
                paper: Paper::A3,
                landscape: true,
            },
            Projection::First,
        )
        .unwrap();
    let front = d
        .add_view(
            z,
            ViewKind::Front,
            scale(1, 2),
            Placement::Free {
                x: 120.5,
                y: 150.25,
            },
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
            z,
            k,
            scale(1, 2),
            Placement::Aligned {
                distance: 20.0 + i as f64,
            },
            Some(front),
        )
        .unwrap();
    }
    d.add_view(
        z,
        ViewKind::Isometric,
        scale(1, 1),
        Placement::Free { x: 330.0, y: 220.0 },
        None,
    )
    .unwrap();
    set_centerlines(&mut d, z, false);
    (d, z)
}

fn set_centerlines(d: &mut Document, z: DrawingId, on: bool) {
    d.update_drawing(z, |x| {
        for v in &mut x.views {
            v.centerlines = on && v.kind.is_normal();
        }
    })
    .unwrap();
}

fn tag(origin: u64, role: &str, hint: [f64; 3]) -> TagData {
    TagData {
        origin,
        role: role.into(),
        hint: DVec3::from_array(hint),
    }
}

fn edge(a: &str, b: &str, hint: [f64; 3]) -> GeoRefData {
    GeoRefData::Edge {
        a: tag(1, a, [hint[0], hint[1], 10.0]),
        b: tag(1, b, [0.0, hint[1], hint[2]]),
        hint: DVec3::from_array(hint),
    }
}

fn spec(view: u64, kind: DimKind, refs: Vec<GeoRef>) -> DimSpec {
    DimSpec {
        id: DimId(0),
        view: planora_drawing::ViewId(view),
        kind,
        refs,
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

fn chain(group: u64, mode: ChainMode, order: u32) -> Option<ChainSpec> {
    Some(ChainSpec { group, mode, order })
}

/// Zeichnung mit Maßen aller Arten, aller Bezugsarten, aller Toleranzarten, Kette und
/// Bezugsmaßen, eigenem Text mit Sonderzeichen; ein gelöschtes Maß (Zähler über größter
/// Kennung + 1). Ansichten wie in der Fixtur: 1 vorne, 2 hinten, 3 links, 4 rechts, 5 oben,
/// 6 unten, 7 iso.
fn dims_document() -> (Document, DrawingId) {
    let (mut d, z) = fixture_document();
    set_centerlines(&mut d, z, true);
    let top_edge = || GeoRef::Edge(edge("Deckel", "Wand 1", [15.0, 0.0, 10.0]));
    let mut add = |s: DimSpec| d.add_dimension(z, s).unwrap();
    // 1: waagerecht, Kante + Kantenanfang, Kette 1
    let mut s = spec(
        1,
        DimKind::Horizontal,
        vec![
            top_edge(),
            GeoRef::EdgeEnd {
                edge: edge("Wand 2", "Boden", [30.0, 0.0, 0.0]),
                end: EndSel::Start,
            },
        ],
    );
    s.chain = chain(1, ChainMode::Chain, 0);
    add(s);
    // 2: waagerecht, Kantenende + Mitte, Kette 1, symmetrische Toleranz
    let mut s = spec(
        1,
        DimKind::Horizontal,
        vec![
            GeoRef::EdgeEnd {
                edge: edge("Wand 2", "Boden", [30.0, 0.0, 0.0]),
                end: EndSel::End,
            },
            GeoRef::Center(edge("Loch 1, Wand 1", "Deckel", [5.0, 5.0, 10.0])),
        ],
    );
    s.chain = chain(1, ChainMode::Chain, 1);
    s.tolerance = DimTolerance::Symmetric(0.05);
    add(s);
    // 3 (wird gelöscht)
    let gone = add(spec(1, DimKind::Vertical, vec![top_edge(), top_edge()]));
    // 4: senkrecht in „hinten“, Bezugsmaße, Abmaße, eigener Text mit Sonderzeichen, Vorschlag
    let mut s = spec(2, DimKind::Vertical, vec![top_edge(), top_edge()]);
    s.chain = chain(7, ChainMode::Baseline, 0);
    s.tolerance = DimTolerance::Limits {
        upper: 0.1,
        lower: -0.2,
    };
    s.text_override = Some("Ø 12 <&\"'> ± − °".into());
    s.suggested = true;
    add(s);
    // 5: senkrecht, Bezugsmaße, Grenzmaße (Abweichungen)
    let mut s = spec(2, DimKind::Vertical, vec![top_edge(), top_edge()]);
    s.chain = chain(7, ChainMode::Baseline, 1);
    s.tolerance = DimTolerance::LimitSizes {
        upper: 0.1,
        lower: -0.1,
    };
    add(s);
    // 6: schräg in „rechts“, Passung
    let mut s = spec(
        4,
        DimKind::Aligned,
        vec![
            GeoRef::EdgeEnd {
                edge: edge("Wand 3", "Deckel", [0.0, 20.0, 10.0]),
                end: EndSel::Start,
            },
            GeoRef::EdgeEnd {
                edge: edge("Wand 3", "Deckel", [0.0, 20.0, 10.0]),
                end: EndSel::End,
            },
        ],
    );
    s.tolerance = DimTolerance::Fit("H7".into());
    s.decimals = 3;
    add(s);
    // 7: Durchmesser an einer Fläche, eigener Führungswinkel
    let mut s = spec(
        1,
        DimKind::Diameter,
        vec![GeoRef::Face(GeoRefData::Face(tag(
            1,
            "Loch 1, Wand 1",
            [5.0, 5.0, 5.0],
        )))],
    );
    s.angle = 135.0;
    s.text_shift = (2.5, -1.25);
    add(s);
    // 8: Radius an einer Kreiskante
    add(spec(
        5,
        DimKind::Radius,
        vec![GeoRef::Edge(edge(
            "Loch 1, Wand 1",
            "Deckel",
            [5.0, 5.0, 10.0],
        ))],
    ));
    // 9: Winkel zwischen zwei Kanten, keine Nachkommastellen
    let mut s = spec(
        6,
        DimKind::Angle,
        vec![
            top_edge(),
            GeoRef::Edge(edge("Wand 1", "Wand 2", [30.0, 0.0, 5.0])),
        ],
    );
    s.decimals = 0;
    s.offset = -15.5;
    add(s);
    d.delete_dimension(z, gone).unwrap();
    // Zweite Zeichnung ohne Maße, Mittellinien nach Vorgabe.
    let z2 = d
        .add_drawing(
            "Blatt 2",
            Sheet {
                paper: Paper::A4,
                landscape: false,
            },
            Projection::Third,
        )
        .unwrap();
    d.add_view(
        z2,
        ViewKind::Top,
        scale(1, 1),
        Placement::Free { x: 1.0, y: 2.0 },
        None,
    )
    .unwrap();
    (d, z)
}

fn good() -> String {
    encode_version(&dims_document().0.snapshot()).unwrap()
}

fn bad(text: &str) -> ProjectError {
    decode_version(text, "x.json").unwrap_err()
}

fn load(text: &str) -> Result<Document, ProjectError> {
    let s = decode_version(text, "x.json")?;
    Document::from_snapshot(s, 0.05).map_err(|e| ProjectError::Invalid(e.to_string()))
}

fn edited(text: &str, f: impl FnOnce(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(text).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

fn dim(v: &mut Value, i: usize) -> &mut Value {
    &mut v["zeichnungen"][0]["masse"][i]
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

// ---- (a) Alte Dateien Byte für Byte ------------------------------------------------------

#[test]
fn old_format_4_files_stay_byte_for_byte() {
    assert!(!FIXTURE.contains('\r'), "Fixtur mit LF");
    assert!(FIXTURE.contains("\"format\": 4"));
    // Die Fixtur liest sich und schreibt sich unverändert zurück.
    let back = decode_version(FIXTURE, "fixtur").unwrap();
    assert_eq!(back.drawings.len(), 1);
    assert_eq!(back.drawings[0].views.len(), 7);
    assert!(back.drawings[0].dimensions.is_empty());
    assert!(back.drawings[0].views.iter().all(|v| !v.centerlines));
    let text = encode_version(&back).unwrap();
    assert_eq!(text, FIXTURE);
    // Auch über das Dokument (wie in der Anwendung).
    let doc = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(encode_version(&doc.snapshot()).unwrap(), FIXTURE);
    // Der neue Schreiber schreibt denselben Aufbau mit ausgeschalteten Mittellinien Byte für
    // Byte wie der alte.
    let (d, _) = fixture_document();
    let text = encode_version(&d.snapshot()).unwrap();
    assert_eq!(text, FIXTURE);
    assert!(!text.contains("mittellinien") && !text.contains("masse"));
    assert!(!text.contains("naechstes_mass"));
}

// ---- (b) Formatnummer --------------------------------------------------------------------

#[test]
fn format_5_only_with_dimensions_or_centerlines() {
    assert_eq!(FORMAT_VERSION, 11);
    let (mut d, z) = fixture_document();
    assert_eq!(required_format(&d.snapshot()), 4);
    // Eine einzige Ansicht mit Mittellinien reicht für Format 5; dann trägt jede Ansicht das
    // Feld.
    d.update_drawing(z, |x| x.views[2].centerlines = true)
        .unwrap();
    assert_eq!(required_format(&d.snapshot()), 5);
    let t = encode_version(&d.snapshot()).unwrap();
    assert!(t.contains("\"format\": 5"), "{t}");
    assert_eq!(t.matches("\"mittellinien\": true").count(), 1, "{t}");
    assert_eq!(t.matches("\"mittellinien\": false").count(), 6, "{t}");
    assert!(!t.contains("\"masse\""), "ohne Maße keine Maßliste: {t}");
    assert!(t.contains("\"naechstes_mass\": 1"), "{t}");
    let back = decode_version(&t, "x").unwrap();
    assert_eq!(back.drawings, d.snapshot().drawings);
    // Mittellinien aus, aber ein Maß: ebenfalls Format 5.
    set_centerlines(&mut d, z, false);
    let id = d
        .add_dimension(
            z,
            spec(
                1,
                DimKind::Diameter,
                vec![GeoRef::Face(GeoRefData::Face(tag(1, "Mantel", [0.0; 3])))],
            ),
        )
        .unwrap();
    assert_eq!(required_format(&d.snapshot()), 5);
    let t = encode_version(&d.snapshot()).unwrap();
    assert!(
        t.contains("\"format\": 5") && t.contains("\"masse\""),
        "{t}"
    );
    // Maß gelöscht, Mittellinien aus: wieder Format 4, Byte für Byte wie die alte Datei.
    d.delete_dimension(z, id).unwrap();
    assert_eq!(required_format(&d.snapshot()), 4);
    assert_eq!(encode_version(&d.snapshot()).unwrap(), FIXTURE);
    // Neue Normansichten haben Mittellinien (Vorgabe): Format 5.
    let mut e = box_document();
    let z = e
        .add_drawing(
            "Z",
            Sheet {
                paper: Paper::A4,
                landscape: false,
            },
            Projection::First,
        )
        .unwrap();
    assert_eq!(required_format(&e.snapshot()), 4, "leere Zeichnung");
    e.add_view(
        z,
        ViewKind::Isometric,
        scale(1, 1),
        Placement::Free { x: 0.0, y: 0.0 },
        None,
    )
    .unwrap();
    assert_eq!(
        required_format(&e.snapshot()),
        4,
        "Isometrie ohne Mittellinien"
    );
    e.add_view(
        z,
        ViewKind::Front,
        scale(1, 1),
        Placement::Free { x: 0.0, y: 0.0 },
        None,
    )
    .unwrap();
    assert_eq!(required_format(&e.snapshot()), 5);
    // Ohne Zeichnungen bleibt es beim alten Format.
    assert_eq!(required_format(&box_document().snapshot()), 1);
}

// ---- (c) Rundreise -----------------------------------------------------------------------

#[test]
fn roundtrip_all_kinds() {
    let (d, z) = dims_document();
    let s = d.snapshot();
    let dr = d.drawing(z).unwrap();
    assert_eq!(dr.dimensions.len(), 8);
    assert_eq!(dr.next_dim, 10);
    let text = encode_version(&s).unwrap();
    assert!(text.contains("\"format\": 5"));
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(back.drawings, s.drawings);
    assert_eq!(encode_version(&back).unwrap(), text, "Rundreise stabil");
    let doc = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(doc.drawings(), d.drawings());
    // Alle Arten stehen mit ihrem Namen in der Datei.
    for art in [
        "waagerecht",
        "senkrecht",
        "schraeg",
        "durchmesser",
        "radius",
        "winkel",
    ] {
        assert!(text.contains(&format!("\"art\": \"{art}\"")), "{art}");
    }
    for typ in [
        "kante",
        "kantenende",
        "mitte",
        "flaeche",
        "keine",
        "symmetrisch",
        "abmasse",
        "grenzmasse",
        "passung",
    ] {
        assert!(text.contains(&format!("\"typ\": \"{typ}\"")), "{typ}");
    }
    for s in [
        "\"ende\": \"anfang\"",
        "\"ende\": \"ende\"",
        "\"modus\": \"kette\"",
        "\"modus\": \"bezug\"",
        "\"naechstes_mass\": 10",
        "\"vorgeschlagen\": true",
        "\"text\": \"H7\"",
    ] {
        assert!(text.contains(s), "{s}");
    }
    // Die gelöschte Kennung 3 ist nicht vergeben; der Zähler bleibt.
    let ids: Vec<u64> = doc
        .drawing(z)
        .unwrap()
        .dimensions
        .iter()
        .map(|m| m.id.0)
        .collect();
    assert_eq!(ids, [1, 2, 4, 5, 6, 7, 8, 9]);
    // Grenzmaße bleiben Abweichungen (keine Umrechnung beim Speichern).
    let ls = &doc.drawing(z).unwrap().dimensions[3];
    assert_eq!(
        ls.tolerance,
        DimTolerance::LimitSizes {
            upper: 0.1,
            lower: -0.1
        }
    );
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v["zeichnungen"][0]["masse"][3]["toleranz"],
        json!({"typ": "grenzmasse", "oben": 0.1, "unten": -0.1})
    );
}

// ---- (d) Format 4 mit Maßen --------------------------------------------------------------

#[test]
fn format_4_with_dimensions_is_refused() {
    let good = good();
    let as4 = good.replacen("\"format\": 5", "\"format\": 4", 1);
    assert!(corrupt_msg(bad(&as4)).contains("Dateiformat 5"));
    // Nur Mittellinien (auch „aus“) in Format 4: beschädigt.
    let t = edited(FIXTURE, |v| {
        v["zeichnungen"][0]["ansichten"][0]["mittellinien"] = json!(false)
    });
    assert!(corrupt_msg(bad(&t)).contains("Dateiformat 5"));
    // Nur der Maßzähler in Format 4: beschädigt.
    let t = edited(FIXTURE, |v| {
        v["zeichnungen"][0]["naechstes_mass"] = json!(3)
    });
    assert!(corrupt_msg(bad(&t)).contains("Dateiformat 5"));
    // Ältere Formate melden zuerst die Zeichnungen.
    let t = good.replacen("\"format\": 5", "\"format\": 3", 1);
    assert!(corrupt_msg(bad(&t)).contains("Dateiformat 4"));
    // Eine leere Maßliste in Format 4 ist harmlos.
    let t = edited(FIXTURE, |v| v["zeichnungen"][0]["masse"] = json!([]));
    assert_eq!(
        encode_version(&decode_version(&t, "x").unwrap()).unwrap(),
        FIXTURE
    );
}

// ---- (e) Feindliche Dateien --------------------------------------------------------------

#[test]
fn hostile_dimension_files() {
    let good = good();
    load(&good).unwrap();
    let raw = |p: fn(&mut Value) -> &mut Value, r: &str| with_raw_number(&good, p, r);
    fn abstand(v: &mut Value) -> &mut Value {
        &mut dim(v, 0)["abstand"]
    }
    fn winkel(v: &mut Value) -> &mut Value {
        &mut dim(v, 0)["winkel"]
    }
    fn versatz(v: &mut Value) -> &mut Value {
        &mut dim(v, 0)["textversatz"][1]
    }
    fn hinweis(v: &mut Value) -> &mut Value {
        &mut dim(v, 0)["bezuege"][0]["hinweis"][0]
    }
    fn marke(v: &mut Value) -> &mut Value {
        &mut dim(v, 0)["bezuege"][0]["a"]["hinweis"][2]
    }
    fn flaeche(v: &mut Value) -> &mut Value {
        &mut dim(v, 5)["bezuege"][0]["marke"]["hinweis"][1]
    }
    fn toleranz(v: &mut Value) -> &mut Value {
        &mut dim(v, 1)["toleranz"]["wert"]
    }
    fn many(v: &mut Value, n: u64) {
        let one = dim(v, 5).clone();
        let list: Vec<Value> = (1..=n)
            .map(|i| {
                let mut m = one.clone();
                m["id"] = json!(i);
                m
            })
            .collect();
        v["zeichnungen"][0]["masse"] = Value::Array(list);
        v["zeichnungen"][0]["naechstes_mass"] = json!(n + 1);
    }
    let corrupt: Vec<(String, &str)> = vec![
        (raw(abstand, "NaN"), ""),
        (raw(abstand, "1e999"), ""),
        (raw(winkel, "-1e999"), ""),
        (raw(abstand, "1e300"), "Abstand: Zahl ungültig oder zu groß"),
        (raw(winkel, "-1e300"), "Winkel: Zahl ungültig oder zu groß"),
        (
            raw(versatz, "1e300"),
            "Textversatz: Zahl ungültig oder zu groß",
        ),
        (raw(abstand, "100000.001"), "Zahl ungültig oder zu groß"),
        (
            raw(hinweis, "1e300"),
            "Hinweispunkt: Zahl ungültig oder zu groß",
        ),
        (raw(hinweis, "1e999"), ""),
        (raw(marke, "-2000000000.000001"), "Hinweispunkt"),
        (raw(hinweis, "2.000001e9"), "Hinweispunkt"),
        (raw(flaeche, "1e300"), "Hinweispunkt"),
        (
            raw(toleranz, "1e300"),
            "Toleranz: Zahl ungültig oder zu groß",
        ),
        (raw(toleranz, "1000.5"), "Toleranz"),
        (edited(&good, |v| *abstand(v) = Value::Null), ""),
        (edited(&good, |v| *abstand(v) = json!("1")), ""),
        (
            edited(&good, |v| dim(v, 0)["art"] = json!("quer")),
            "Unbekannte Maßart „quer“",
        ),
        (
            edited(&good, |v| dim(v, 0)["art"] = json!("Waagerecht")),
            "Unbekannte Maßart",
        ),
        (
            edited(&good, |v| dim(v, 0)["bezuege"][0]["typ"] = json!("punkt")),
            "",
        ),
        (
            edited(&good, |v| dim(v, 0)["toleranz"]["typ"] = json!("ungefaehr")),
            "",
        ),
        (
            edited(&good, |v| dim(v, 0)["bezuege"][1]["ende"] = json!("mitte")),
            "Unbekanntes Kantenende „mitte“",
        ),
        (
            edited(&good, |v| dim(v, 0)["kette"]["modus"] = json!("kreuz")),
            "Unbekannte Kettenart „kreuz“",
        ),
        (
            edited(&good, |v| dim(v, 0)["stellen"] = json!(9)),
            "höchstens 3 Nachkommastellen",
        ),
        (edited(&good, |v| dim(v, 0)["stellen"] = json!(256)), ""),
        (edited(&good, |v| dim(v, 0)["stellen"] = json!(-1)), ""),
        (
            edited(&good, |v| dim(v, 1)["id"] = json!(1)),
            "Maßkennung kommt doppelt vor",
        ),
        (
            edited(&good, |v| dim(v, 1)["id"] = json!(10)),
            "ungültige Kennung",
        ),
        (
            edited(&good, |v| dim(v, 1)["id"] = json!(0)),
            "ungültige Kennung",
        ),
        (edited(&good, |v| dim(v, 1)["id"] = json!(-2)), ""),
        (
            edited(&good, |v| v["zeichnungen"][0]["naechstes_mass"] = json!(9)),
            "ungültige Kennung",
        ),
        (
            edited(&good, |v| {
                v["zeichnungen"][0]["naechstes_mass"] = json!(u64::MAX)
            }),
            "Zähler ist ungültig",
        ),
        (
            edited(&good, |v| {
                v["zeichnungen"][0]["naechstes_mass"] = json!(0);
                dim(v, 1)["id"] = json!(u64::MAX);
            }),
            "ungültige Kennung",
        ),
        (
            // Waagerechtes Maß mit nur einem Bezug
            edited(&good, |v| {
                dim(v, 0)["bezuege"].as_array_mut().unwrap().pop();
            }),
            "falsche Zahl von Bezügen",
        ),
        (
            // Durchmesser mit zwei Bezügen
            edited(&good, |v| {
                let r = dim(v, 5)["bezuege"][0].clone();
                dim(v, 5)["bezuege"].as_array_mut().unwrap().push(r);
            }),
            "falsche Zahl von Bezügen",
        ),
        (
            edited(&good, |v| {
                let r = dim(v, 0)["bezuege"][0].clone();
                dim(v, 0)["bezuege"] = json!([r.clone(), r.clone(), r.clone(), r.clone(), r]);
            }),
            "Bezüge: zu viele Einträge (höchstens 4)",
        ),
        (edited(&good, |v| dim(v, 0)["bezuege"] = json!("x")), ""),
        (
            edited(&good, |v| dim(v, 0)["ansicht"] = json!(99)),
            "gehört zu keiner Ansicht",
        ),
        (
            edited(&good, |v| dim(v, 0)["ansicht"] = json!(7)),
            "In der isometrischen Ansicht gibt es keine Maße",
        ),
        (
            edited(&good, |v| many(v, 2001)),
            "Maße: zu viele Einträge (höchstens 2000)",
        ),
        (
            edited(&good, |v| dim(v, 0)["text"] = json!("x".repeat(201))),
            "Der eigene Maßtext ist zu lang",
        ),
        (
            edited(&good, |v| dim(v, 0)["text"] = json!("Ø".repeat(201))),
            "Der eigene Maßtext ist zu lang",
        ),
        (
            edited(&good, |v| {
                dim(v, 0)["toleranz"] = json!({"typ": "passung", "text": "H".repeat(51)})
            }),
            "Passungsangabe ist zu lang",
        ),
        (
            edited(&good, |v| {
                dim(v, 0)["bezuege"][0]["b"]["rolle"] = json!("r".repeat(65))
            }),
            "Die Rolle einer Fläche ist zu lang (höchstens 64 Zeichen)",
        ),
        (
            edited(&good, |v| {
                dim(v, 5)["bezuege"][0]["marke"]["rolle"] = json!("ä".repeat(65))
            }),
            "Rolle",
        ),
        (
            edited(&good, |v| dim(v, 1)["kette"]["modus"] = json!("bezug")),
            "Die Maße einer Kette mischen Kettenmaße und Bezugsmaße",
        ),
        (
            // Stelle in der Kette über der Grenze (gestapelte Maßlinien lägen sonst beliebig
            // weit außerhalb).
            edited(&good, |v| dim(v, 1)["kette"]["reihenfolge"] = json!(2000)),
            "Die Stelle in der Maßkette ist zu groß (höchstens 1999)",
        ),
        (
            edited(&good, |v| {
                dim(v, 1)["kette"]["reihenfolge"] = json!(u32::MAX)
            }),
            "Die Stelle in der Maßkette ist zu groß",
        ),
        (
            edited(&good, |v| {
                v["zeichnungen"][0]["ansichten"][0]["mittellinien"] = json!("ja")
            }),
            "",
        ),
        (
            edited(&good, |v| v["zeichnungen"][0]["masse"] = json!({})),
            "",
        ),
        (
            edited(&good, |v| dim(v, 0)["textversatz"] = json!([1.0])),
            "",
        ),
        (
            edited(&good, |v| {
                dim(v, 0).as_object_mut().unwrap().remove("vorgeschlagen");
            }),
            "",
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

    // Grenzen selbst sind erlaubt.
    let ok = edited(&good, |v| {
        dim(v, 0)["text"] = json!("Ø".repeat(200));
        dim(v, 0)["bezuege"][0]["b"]["rolle"] = json!("ä".repeat(64));
        dim(v, 0)["abstand"] = json!(-100000.0);
        // Hinweispunkte sind Modellpunkte: bis 2·MAX_COORD_MM, nicht nur bis MAX_POS_MM.
        dim(v, 0)["bezuege"][0]["hinweis"][1] = json!(-2.0e9);
        dim(v, 0)["bezuege"][0]["a"]["hinweis"][0] = json!(1.0e9);
        dim(v, 0)["stellen"] = json!(3);
        dim(v, 1)["toleranz"]["wert"] = json!(1000.0);
        dim(v, 1)["kette"]["reihenfolge"] = json!(1999);
    });
    load(&ok).unwrap();
    load(&edited(&good, |v| many(v, 2000))).unwrap();

    // Beim Lesen formal in Ordnung, aber von der Dokumentprüfung abgelehnt (Invalid).
    let invalid: Vec<(String, &str)> = vec![
        (
            // Gleiche Gruppe, andere Art
            edited(&good, |v| dim(v, 1)["art"] = json!("senkrecht")),
            "dieselbe Art",
        ),
        (
            // Gleiche Gruppe, andere Ansicht
            edited(&good, |v| dim(v, 1)["ansicht"] = json!(2)),
            "derselben Ansicht",
        ),
        (
            edited(&good, |v| dim(v, 1)["kette"]["reihenfolge"] = json!(0)),
            "doppelt",
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

// ---- (f) Mittellinien in alten Dateien ---------------------------------------------------

#[test]
fn centerlines_default_is_false_for_old_files() {
    // Neue Normansichten zeigen Mittellinien …
    let mut d = box_document();
    let z = d
        .add_drawing(
            "Z",
            Sheet {
                paper: Paper::A4,
                landscape: false,
            },
            Projection::First,
        )
        .unwrap();
    d.add_view(
        z,
        ViewKind::Front,
        scale(1, 1),
        Placement::Free { x: 0.0, y: 0.0 },
        None,
    )
    .unwrap();
    assert!(d.drawing(z).unwrap().views[0].centerlines);
    // … Ansichten aus Format 4 nicht.
    let s = decode_version(FIXTURE, "alt").unwrap();
    let kinds: Vec<ViewKind> = s.drawings[0].views.iter().map(|v| v.kind).collect();
    assert_eq!(kinds, ViewKind::ALL);
    assert!(s.drawings[0].views.iter().all(|v| !v.centerlines));
    let doc = Document::from_snapshot(s, 0.05).unwrap();
    assert!(doc.drawings()[0].views.iter().all(|v| !v.centerlines));
    // Fehlt das Feld in Format 5, gilt ebenfalls „aus“.
    let good = good();
    let t = edited(&good, |v| {
        v["zeichnungen"][0]["ansichten"][0]
            .as_object_mut()
            .unwrap()
            .remove("mittellinien");
    });
    let s = decode_version(&t, "x").unwrap();
    assert!(!s.drawings[0].views[0].centerlines);
    assert!(s.drawings[0].views[1].centerlines);
}

// ---- (g) Neuere Formate ------------------------------------------------------------------

#[test]
fn newer_format_10_is_refused_with_the_clear_text() {
    let good = good();
    let e = bad(&good.replacen("\"format\": 5", "\"format\": 12", 1));
    assert!(matches!(e, ProjectError::TooNew { found: 12, .. }), "{e:?}");
    let msg = e.to_string();
    assert!(
        msg.contains("neueren Programmversion") && msg.contains("Format 12"),
        "{msg}"
    );
    let e = bad("{\"format\": 12, \"zeichnungen\": [{\"masse\": \"ganz anders\"}]}");
    assert!(matches!(e, ProjectError::TooNew { found: 12, .. }), "{e:?}");
}

// ---- (h) Versionsspeicher und Autosave ---------------------------------------------------

#[test]
fn autosave_and_version_store_keep_dimensions() {
    let p = std::env::temp_dir().join(format!("planora-dims-{}.pcad", new_id()));
    let (d, z) = dims_document();
    let s = d.snapshot();
    let mut st = DocStore::create(&p, &s, "Mit Maßen").unwrap();
    let (old, _) = fixture_document();
    st.save(Some(1), &old.snapshot(), "", None).unwrap();
    st.write_autosave(Some(2), &s).unwrap();
    drop(st);
    let (st, _) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert_eq!(st.load_version(1).unwrap().drawings, s.drawings);
    let v2 = st.load_version(2).unwrap();
    assert!(v2.drawings[0].dimensions.is_empty());
    assert!(v2.drawings[0].views.iter().all(|v| !v.centerlines));
    let auto = st.read_autosave().unwrap().unwrap();
    assert_eq!(auto.snapshot.drawings, s.drawings);
    drop(st);
    // Die Versionsdateien selbst: Version 1 in Format 5, Version 2 Byte für Byte wie Format 4.
    let read = |n: &str| std::fs::read_to_string(p.join("versionen").join(n)).unwrap();
    assert!(read("0001.json").contains("\"masse\""));
    assert_eq!(
        read("0002.json"),
        FIXTURE,
        "Version 2 Byte für Byte wie Format 4"
    );
    let _ = std::fs::remove_dir_all(&p);
    let back = Document::from_snapshot(auto.snapshot, 0.05).unwrap();
    assert_eq!(back.drawing(z).unwrap().next_dim, 10);
}

// ---- Hinweispunkte weit draußen -----------------------------------------------------------

/// Alles, was das Dokument annimmt, lässt sich speichern und wieder laden: Hinweispunkte bis
/// `MAX_HINT_MM` (2·MAX_COORD_MM); darüber lehnen Dokument und Leser gleichermaßen ab.
#[test]
fn far_hints_save_and_load_and_beyond_the_limit_are_refused_everywhere() {
    assert_eq!(MAX_HINT_MM, 2.0 * MAX_COORD_MM);
    let face = |h: f64| {
        spec(
            1,
            DimKind::Diameter,
            vec![GeoRef::Face(GeoRefData::Face(tag(
                1,
                "Mantel",
                [h, -h, 0.5],
            )))],
        )
    };
    let (mut d, z) = fixture_document();
    d.add_dimension(z, face(1.0e9)).unwrap();
    d.add_dimension(z, face(MAX_HINT_MM)).unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    let doc = load(&text).unwrap();
    assert_eq!(doc.drawings(), d.drawings());
    for h in [2.000001e9, 1.0e300, f64::INFINITY] {
        let before = d.drawing(z).unwrap().clone();
        let e = d.add_dimension(z, face(h)).unwrap_err();
        assert!(e.to_string().contains("Bezug"), "{e}");
        assert_eq!(d.drawing(z).unwrap(), &before);
    }
    // Zu lange Rolle: ebenfalls schon beim Anlegen abgelehnt.
    let mut long = face(0.0);
    long.refs = vec![GeoRef::Face(GeoRefData::Face(tag(
        1,
        &"r".repeat(65),
        [0.0; 3],
    )))];
    let e = d.add_dimension(z, long).unwrap_err();
    assert!(e.to_string().contains("Rolle"), "{e}");
    assert_eq!(MAX_ROLE_CHARS, 64);
}

// ---- (i) Zahlen bitgenau -----------------------------------------------------------------

#[test]
fn float_values_roundtrip_exactly() {
    let values = [
        0.1,
        -0.1,
        1.0 / 3.0,
        -0.0,
        0.0,
        5e-324,
        f64::MIN_POSITIVE,
        99999.99999999999,
        1.0e5,
        -1.0e5,
        12.345678901234567,
    ];
    let (mut d, z) = fixture_document();
    for (i, &x) in values.iter().enumerate() {
        let y = values[values.len() - 1 - i];
        let t = (x / 1000.0).clamp(-1000.0, 1000.0);
        let mut s = spec(
            1,
            DimKind::Horizontal,
            vec![
                GeoRef::Edge(GeoRefData::Edge {
                    a: tag(1, "A", [x, y, -x]),
                    b: tag(2, "B", [y, x, -0.0]),
                    hint: DVec3::new(x, -y, 1.0 / 7.0),
                }),
                GeoRef::Face(GeoRefData::Face(tag(3, "C", [-x, y, x]))),
            ],
        );
        // Ein Bezug „Fläche“ ist für waagerechte Maße nicht sinnvoll, aber in der Datei
        // zulässig (die Messung meldet ihn später); hier geht es nur um die Zahlen.
        s.offset = x;
        s.angle = y;
        s.text_shift = (y, x);
        s.tolerance = if i % 2 == 0 {
            DimTolerance::Limits {
                upper: t,
                lower: -t,
            }
        } else {
            DimTolerance::LimitSizes {
                upper: -t,
                lower: t,
            }
        };
        d.add_dimension(z, s).unwrap();
    }
    let s = d.snapshot();
    let back = decode_version(&encode_version(&s).unwrap(), "x.json").unwrap();
    fn bits(dr: &Drawing) -> Vec<u64> {
        let mut out = Vec::new();
        let mut v3 = |v: DVec3| out.extend([v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]);
        let mut hints = Vec::new();
        for m in &dr.dimensions {
            for r in &m.refs {
                match r {
                    GeoRef::Edge(GeoRefData::Edge { a, b, hint }) => {
                        hints.extend([a.hint, b.hint, *hint])
                    }
                    GeoRef::Face(GeoRefData::Face(t)) => hints.push(t.hint),
                    _ => panic!(),
                }
            }
        }
        for h in hints {
            v3(h);
        }
        for m in &dr.dimensions {
            out.extend([
                m.offset.to_bits(),
                m.angle.to_bits(),
                m.text_shift.0.to_bits(),
                m.text_shift.1.to_bits(),
            ]);
            match m.tolerance {
                DimTolerance::Limits { upper, lower }
                | DimTolerance::LimitSizes { upper, lower } => {
                    out.extend([upper.to_bits(), lower.to_bits()])
                }
                _ => panic!(),
            }
        }
        out
    }
    let want = bits(&s.drawings[0]);
    assert_eq!(want.len(), values.len() * (9 + 3 + 4 + 2));
    assert_eq!(bits(&back.drawings[0]), want);
    // -0.0 bleibt negativ null.
    let m = &back.drawings[0].dimensions[3];
    assert!(m.offset == 0.0 && m.offset.is_sign_negative());
    assert_eq!(m.offset.to_bits(), (-0.0f64).to_bits());
}
