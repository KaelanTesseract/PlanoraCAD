//! Dateiformat 22 (Teilprojekt Skizzen 11b, Task 7): Nur ein Dokument mit Skizzengruppe,
//! Höhenformel an einem Text, Drehprofil oder Isokurve wird mit Format 22 geschrieben; alle
//! anderen bleiben bytegleich. `{Name}` in einem Text ändert das Format nicht (Korrekturrunde 1:
//! die Datenform des Textes bleibt, nur seine Deutung ist neu). Der Rundlauf ist verlustfrei.

use planora_feature::planes::{AxisRef, StdAxis};
use planora_feature::*;
use planora_i18n::tr;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
use planora_sketch::{CurveId, Sketch};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Felder, die es erst ab Format 22 gibt.
const NEW_FIELDS: [&str; 5] = [
    "\"skizzengruppen\"",
    "\"naechste_gruppe\"",
    "\"hoehenformeln\"",
    "\"drehprofil\"",
    "\"isokurve\"",
];

fn format_of(text: &str) -> u64 {
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    v["format"].as_u64().unwrap()
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn new_sketch(name: &str) -> Sketch {
    Sketch::new(Frame::WORLD, name, Tolerance::default())
}

fn sketch_feature(sketch: Sketch) -> SketchFeature {
    SketchFeature {
        sketch,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
        text_formulas: TextFormulas::new(),
    }
}

/// Je Zeichen ein Quadrat aus vier Strecken (ohne Schrift, ohne `planora-text`).
fn shape(content: &str) -> Arc<TextShape> {
    let glyphs: Vec<Glyph> = content
        .chars()
        .enumerate()
        .map(|(i, _)| Glyph {
            origin: v(i as f64 * 0.8, 0.0),
            advance: 0.8,
            contours: vec![Contour {
                start: v(0.0, 0.0),
                segs: vec![
                    Seg::Line(v(0.6, 0.0)),
                    Seg::Line(v(0.6, 0.7)),
                    Seg::Line(v(0.0, 0.7)),
                    Seg::Line(v(0.0, 0.0)),
                ],
            }],
        })
        .collect();
    let width = content.chars().count() as f64 * 0.8;
    Arc::new(TextShape {
        lines: vec![ShapedLine { glyphs, width }],
        ascender: 0.9,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    })
}

/// Eine Skizze mit einem Text im Rahmen mit dem Inhalt `content`.
fn text_sketch(content: &str) -> (Sketch, TextId) {
    let mut sk = new_sketch("Skizze1");
    let t = sk
        .add_text_box(
            v(1.0, 2.0),
            v(41.0, 12.0),
            TextParams {
                content: content.into(),
                style: TextStyle {
                    font: "Gibt Es Nicht Sans".into(),
                    bold: false,
                    italic: false,
                    spacing: 0.0,
                },
                height: 5.0,
                flip_h: false,
                flip_v: false,
            },
            HAlign::Left,
            VAlign::Bottom,
            shape(content),
        )
        .unwrap();
    (sk, t)
}

fn document_with(features: Vec<(&str, FeatureKind)>) -> Document {
    let mut d = Document::new();
    for (name, kind) in features {
        d.add_feature(name, kind).unwrap();
    }
    d
}

/// Wie [`document_with`], vorher mit dem Parameter `Breite` = 20 (eine Höhenformel muss sich
/// beim Anlegen auswerten lassen, Task 10).
fn document_with_breite(features: Vec<(&str, FeatureKind)>) -> Document {
    let mut d = Document::new();
    d.add_parameter("Breite", "20", ParamUnit::Length, "")
        .unwrap();
    for (name, kind) in features {
        d.add_feature(name, kind).unwrap();
    }
    d
}

fn sketch_of(s: &Snapshot, i: usize) -> &SketchFeature {
    let FeatureKind::Sketch(sf) = &s.features[i].kind else {
        panic!("keine Skizze");
    };
    sf
}

/// Speichert, prüft Format 22, lädt und prüft bytegleiches Wiederschreiben.
fn roundtrip(snap: &Snapshot) -> (Snapshot, String) {
    assert_eq!(required_format(snap), 22);
    let text = encode_version(snap).unwrap();
    assert_eq!(format_of(&text), 22);
    let back = decode_version(&text, "x.json").unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(encode_version(&back).unwrap(), text);
    (back, text)
}

/// Drei Skizzen; `Skizze3` bleibt außerhalb jeder Gruppe.
fn three_sketches() -> Document {
    document_with(vec![
        (
            "Skizze1",
            FeatureKind::Sketch(sketch_feature(new_sketch("Skizze1"))),
        ),
        (
            "Skizze2",
            FeatureKind::Sketch(sketch_feature(new_sketch("Skizze2"))),
        ),
        (
            "Skizze3",
            FeatureKind::Sketch(sketch_feature(new_sketch("Skizze3"))),
        ),
    ])
}

/// Gruppe A (oben, sichtbar) enthält Gruppe B (ausgeblendet); in A liegt Skizze1, in B Skizze2.
fn nested_groups(d: &Document) -> SketchGroups {
    let ids: Vec<FeatureId> = d.features().iter().map(|f| f.id).collect();
    let mut g = SketchGroups::default();
    g.groups.push(SketchGroup {
        id: SketchGroupId(1),
        name: "Gruppe A".into(),
        parent: None,
        hidden: false,
    });
    g.groups.push(SketchGroup {
        id: SketchGroupId(2),
        name: "Gruppe B".into(),
        parent: Some(SketchGroupId(1)),
        hidden: true,
    });
    g.member.insert(ids[0], SketchGroupId(1));
    g.member.insert(ids[1], SketchGroupId(2));
    g.next = 3;
    g
}

/// Ein Quader als Quelle der Kopien und eine Skizze mit einer Linie dahinter.
fn copy_document(source: CopySource) -> Snapshot {
    let mut sk = new_sketch("Skizze1");
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    let line = sk.add_line(a, b).unwrap();
    let mut sf = sketch_feature(sk);
    sf.copies.push(EdgeCopy::new(line, source));
    let d = document_with(vec![
        (
            "Quader1",
            FeatureKind::Box {
                size: DVec3::new(10.0, 10.0, 10.0),
                origin: DVec3::ZERO,
            },
        ),
        ("Skizze1", FeatureKind::Sketch(sf)),
    ]);
    d.snapshot()
}

fn face(origin: u64, role: &str) -> FaceRef {
    FaceRef {
        origin,
        role: role.into(),
        hint: DVec3::new(1.0, 2.0, 3.0),
    }
}

fn edge(origin: u64) -> EdgeRef {
    EdgeRef {
        a: face(origin, "Wand 1"),
        b: face(origin, "Boden"),
        hint: DVec3::new(5.0, 0.0, 0.0),
    }
}

fn box_id(s: &Snapshot) -> u64 {
    s.features[0].id.0
}

#[test]
fn format_version_is_23() {
    assert_eq!(FORMAT_VERSION, 23);
}

/// Jede vorhandene Testdatei lädt und wird bytegleich wieder geschrieben, mit ihrem Format
/// (kleiner als 22) und ohne eines der neuen Felder.
#[test]
fn old_file_bytes_unchanged() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut seen = 0;
    for dir in [
        "data",
        "fixtures",
        "fixtures_format18",
        "fixtures_old_smooth",
        "fixtures_format21",
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
            // Zeilenenden wie geschrieben (Git kann sie beim Auschecken in CRLF wandeln).
            let raw = std::fs::read_to_string(&p).unwrap().replace("\r\n", "\n");
            let text = raw.trim_end_matches('\n');
            let snap = decode_version(text, &name).unwrap_or_else(|e| panic!("{name}: {e}"));
            let back = encode_version(&snap).unwrap();
            assert_eq!(back, text, "{name}");
            assert_eq!(format_of(&back), format_of(text), "{name}");
            assert!(format_of(&back) < 22, "{name}");
            for field in NEW_FIELDS {
                assert!(!back.contains(field), "{name}: {field}");
            }
            seen += 1;
        }
    }
    assert!(seen >= 16, "{seen}");
}

#[test]
fn plain_document_stays_below_22() {
    let (sk, _) = text_sketch("Breite mm");
    let d = document_with(vec![("Skizze1", FeatureKind::Sketch(sketch_feature(sk)))]);
    let snap = d.snapshot();
    assert!(snap.sketch_groups.is_empty());
    assert!(required_format(&snap) <= 21);
    let text = encode_version(&snap).unwrap();
    assert!(format_of(&text) <= 21);
    for field in NEW_FIELDS {
        assert!(!text.contains(field), "{field}");
    }
    // Geschweifte Klammern ohne gültigen Namen sind kein Verweis.
    for content in ["{}", "{1a} mm", "{ Breite }", "Breite}", "{Breite"] {
        let (sk, _) = text_sketch(content);
        let d = document_with(vec![("Skizze1", FeatureKind::Sketch(sketch_feature(sk)))]);
        assert!(required_format(&d.snapshot()) <= 21, "{content}");
    }
}

#[test]
fn group_writes_22_and_roundtrips() {
    let d = three_sketches();
    let mut snap = d.snapshot();
    let ids: Vec<FeatureId> = snap.features.iter().map(|f| f.id).collect();
    let mut g = SketchGroups::default();
    g.groups.push(SketchGroup {
        id: SketchGroupId(1),
        name: "Halter".into(),
        parent: None,
        hidden: false,
    });
    g.member.insert(ids[0], SketchGroupId(1));
    g.member.insert(ids[2], SketchGroupId(1));
    g.next = 2;
    snap.sketch_groups = g.clone();
    let (back, text) = roundtrip(&snap);
    assert_eq!(back.sketch_groups, g);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let gruppe = &v["skizzengruppen"][0];
    assert_eq!(gruppe["kennung"], 1);
    assert_eq!(gruppe["name"], "Halter");
    assert!(gruppe.get("eltern").is_none(), "{text}");
    assert!(gruppe.get("aus").is_none(), "{text}");
    assert_eq!(
        gruppe["skizzen"],
        serde_json::json!([ids[0].0, ids[2].0]),
        "{text}"
    );
    // Auch eine leere Gruppe ist Inhalt und braucht Format 22.
    let mut empty = d.snapshot();
    empty.sketch_groups.groups.push(SketchGroup {
        id: SketchGroupId(1),
        name: "Leer".into(),
        parent: None,
        hidden: false,
    });
    empty.sketch_groups.next = 2;
    let (back, _) = roundtrip(&empty);
    assert_eq!(back.sketch_groups, empty.sketch_groups);
}

/// Prüf-Schwerpunkt 3: Gruppe A ⊃ Gruppe B (ausgeblendet) ⊃ Skizze; Laden ergibt dieselben
/// Gruppen samt Augen, auch im gebauten Dokument.
#[test]
fn nested_hidden_groups_roundtrip() {
    let d = three_sketches();
    let groups = nested_groups(&d);
    let mut snap = d.snapshot();
    snap.sketch_groups = groups.clone();
    let (back, text) = roundtrip(&snap);
    assert_eq!(back.sketch_groups, groups);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let b = &v["skizzengruppen"][1];
    assert_eq!(b["eltern"], 1, "{text}");
    assert_eq!(b["aus"], true, "{text}");
    let doc = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(doc.sketch_groups(), &groups);
    assert_eq!(doc.snapshot().sketch_groups, groups);
    // Eine von Hand geschriebene Datei mit derselben Struktur lädt gleich und schreibt sich
    // bytegleich.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures_format22/nested_groups.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n");
    let fixture = raw.trim_end_matches('\n');
    let snap = decode_version(fixture, "nested_groups.json").unwrap();
    assert_eq!(encode_version(&snap).unwrap(), fixture);
    let g = &snap.sketch_groups;
    assert_eq!(g.groups.len(), 2);
    assert_eq!(g.groups[1].parent, Some(g.groups[0].id));
    assert!(g.groups[1].hidden && !g.groups[0].hidden);
    assert_eq!(g.member.get(&snap.features[0].id), Some(&g.groups[1].id));
    assert_eq!(g.next, 3);
}

#[test]
fn text_formula_writes_22() {
    let (sk, t) = text_sketch("Abc");
    let mut sf = sketch_feature(sk);
    sf.text_formulas.insert(t, "Breite / 4".into());
    let d = document_with_breite(vec![("Skizze1", FeatureKind::Sketch(sf))]);
    let (back, text) = roundtrip(&d.snapshot());
    let mut expected = BTreeMap::new();
    expected.insert(t, "Breite / 4".to_string());
    assert_eq!(sketch_of(&back, 0).text_formulas, expected);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v["features"][0]["art"]["hoehenformeln"],
        serde_json::json!([{ "text": 0, "formel": "Breite / 4" }]),
        "{text}"
    );
}

/// Ein Dokument, in dem nur `{…}` im Text neu ist, schreibt kein Format 22 (auch nicht mit
/// gelöschtem Text), sondern das Format des Textes (18).
#[test]
fn param_ref_in_text_keeps_format() {
    for delete in [false, true] {
        let (mut sk, t) = text_sketch("{Breite} mm");
        if delete {
            sk.remove_text(t).unwrap();
        }
        let d = document_with(vec![("Skizze1", FeatureKind::Sketch(sketch_feature(sk)))]);
        let snap = d.snapshot();
        assert_eq!(required_format(&snap), 18, "{delete}");
        let text = encode_version(&snap).unwrap();
        assert_eq!(format_of(&text), 18);
        assert!(text.contains("{Breite} mm"));
        let back = decode_version(&text, "x.json").unwrap();
        assert_eq!(encode_version(&back).unwrap(), text);
    }
}

/// Eine Datei im Format 21 mit `{Breite}` im Text (hier eine geschriebene Datei mit
/// nachträglich getipptem Verweis) lädt; neu geschrieben ergibt sie dieselben Bytes.
#[test]
fn format_21_file_with_param_ref_loads_unchanged() {
    // Ein Kamm hält die Datei bei Format 21 (sonst schriebe sie sich als 18).
    let (mut sk, _) = text_sketch("Breite mm");
    let m = sk.add_point(0.0, 60.0).unwrap();
    let st = sk.add_point(5.0, 60.0).unwrap();
    let en = sk.add_point(0.0, 65.0).unwrap();
    let arc = sk.add_arc(m, st, en).unwrap();
    sk.set_comb(arc, Some(planora_sketch::comb::CombSettings::default()))
        .unwrap();
    let d = document_with(vec![("Skizze1", FeatureKind::Sketch(sketch_feature(sk)))]);
    let written = encode_version(&d.snapshot()).unwrap();
    assert_eq!(format_of(&written), 21);
    let text = written.replacen("\"Breite mm\"", "\"{Breite} mm\"", 1);
    assert!(text.contains("{Breite} mm"));
    let snap = decode_version(&text, "x.json").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(encode_version(&snap).unwrap(), text);
}

/// Task 11: Der gezeigte (ersetzte) Inhalt eines Textes ist nur zur Laufzeit da; die Datei hat den
/// Rohinhalt, und ein Dokument mit `{…}` als einzigem Neuen bleibt bei Format 18.
#[test]
fn shown_never_in_file() {
    let (mut sk, t) = text_sketch("{Breite} mm");
    assert!(sk.set_text_shown(t, Some("20,00 mm".into())));
    let mut d = document_with(vec![("Skizze1", FeatureKind::Sketch(sketch_feature(sk)))]);
    d.add_parameter("Breite", "20", ParamUnit::Length, "")
        .unwrap();
    let snap = d.snapshot();
    assert_eq!(required_format(&snap), 18);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 18);
    assert!(text.contains("{Breite} mm"));
    assert!(!text.contains("20,00 mm"));
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(encode_version(&back).unwrap(), text);
}

/// Task 11: Umbenennen eines Parameters schreibt den Verweis im Text um; das Dateiformat bleibt
/// das alte (18 für Texte), die Datei hat den neuen Rohinhalt.
#[test]
fn rename_keeps_old_format() {
    let (sk, _) = text_sketch("{Breite} mm");
    let mut d = document_with(vec![("Skizze1", FeatureKind::Sketch(sketch_feature(sk)))]);
    let p = d
        .add_parameter("Breite", "20", ParamUnit::Length, "")
        .unwrap();
    let before = encode_version(&d.snapshot()).unwrap();
    assert_eq!(format_of(&before), 18);
    d.set_parameter(
        p,
        ParamEdit {
            name: "Neu".into(),
            formula: "20".into(),
            unit: ParamUnit::Length,
            comment: String::new(),
        },
    )
    .unwrap();
    let snap = d.snapshot();
    assert_eq!(required_format(&snap), 18);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 18);
    assert!(text.contains("{Neu} mm"), "{text}");
    assert!(!text.contains("{Breite}"));
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(encode_version(&back).unwrap(), text);
}

/// Die nächste Gruppenkennung steht in der Datei (`naechste_gruppe`) und kommt gleich zurück,
/// auch wenn sie größer ist als größte + 1.
#[test]
fn group_next_roundtrips() {
    let d = three_sketches();
    let mut snap = d.snapshot();
    let mut g = nested_groups(&d);
    g.next = 9;
    snap.sketch_groups = g.clone();
    let (back, text) = roundtrip(&snap);
    assert_eq!(back.sketch_groups, g);
    // Skizzen gelten nach dem Laden nach ihrem Stand als neu (`same_content`); gleich ist der
    // Snapshot darum über seine Datei.
    assert_eq!(
        encode_version(&back).unwrap(),
        encode_version(&snap).unwrap()
    );
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["naechste_gruppe"], 9, "{text}");
    // Fehlt das Feld, gilt größte + 1.
    let mut v2 = v.clone();
    v2.as_object_mut().unwrap().remove("naechste_gruppe");
    let back = decode_version(&serde_json::to_string_pretty(&v2).unwrap(), "x.json").unwrap();
    assert_eq!(back.sketch_groups.next, 3);
}

/// Die Datei als JSON-Wert, mit Gruppen und Format 22.
fn grouped_value() -> serde_json::Value {
    let d = three_sketches();
    let mut snap = d.snapshot();
    snap.sketch_groups = nested_groups(&d);
    serde_json::from_str(&encode_version(&snap).unwrap()).unwrap()
}

/// Fehlertext beim Laden des geänderten Werts.
fn load_err(v: &serde_json::Value) -> String {
    decode_version(&serde_json::to_string_pretty(v).unwrap(), "x.json")
        .unwrap_err()
        .to_string()
}

#[test]
fn bad_sketch_groups_refused() {
    // Doppelte Kennung
    let mut v = grouped_value();
    v["skizzengruppen"][1]["kennung"] = 1.into();
    v["skizzengruppen"][1]
        .as_object_mut()
        .unwrap()
        .remove("eltern");
    let e = load_err(&v);
    assert!(e.contains(&tr!("proj-sketch-group-twice", id = "1")), "{e}");
    // Elternkreis A → B → A
    let mut v = grouped_value();
    v["skizzengruppen"][0]["eltern"] = 2.into();
    let e = load_err(&v);
    assert!(
        e.contains(&tr!("proj-sketch-group-parent", id = "1")),
        "{e}"
    );
    // Fehlende Eltern
    let mut v = grouped_value();
    v["skizzengruppen"][1]["eltern"] = 77.into();
    let e = load_err(&v);
    assert!(
        e.contains(&tr!("proj-sketch-group-parent", id = "2")),
        "{e}"
    );
    // Ein Mitglied, das keine Skizze ist
    let mut v = grouped_value();
    v["skizzengruppen"][0]["skizzen"] = serde_json::json!([999]);
    let e = load_err(&v);
    assert!(
        e.contains(&tr!(
            "proj-sketch-group-not-sketch",
            id = "1",
            feature = "999"
        )),
        "{e}"
    );
    // Eine Skizze in zwei Gruppen
    let mut v = grouped_value();
    let first = v["skizzengruppen"][1]["skizzen"][0].clone();
    v["skizzengruppen"][0]["skizzen"] = serde_json::json!([first]);
    let e = load_err(&v);
    assert!(
        e.contains(&tr!(
            "proj-sketch-group-member-twice",
            feature = first.to_string()
        )),
        "{e}"
    );
    // Nächste Kennung zu klein
    let mut v = grouped_value();
    v["naechste_gruppe"] = 2.into();
    let e = load_err(&v);
    assert!(
        e.contains(&tr!("proj-sketch-group-next-small", next = "2", min = "3")),
        "{e}"
    );
    // Größte mögliche Kennung: keine nächste Kennung möglich
    let mut v = grouped_value();
    v["skizzengruppen"][1]["kennung"] = u64::MAX.into();
    v["skizzengruppen"][1]
        .as_object_mut()
        .unwrap()
        .remove("eltern");
    v.as_object_mut().unwrap().remove("naechste_gruppe");
    let e = load_err(&v);
    assert!(e.contains(&tr!("proj-sketch-group-id-max")), "{e}");
}

#[test]
fn bad_text_formulas_refused() {
    let (mut sk, a) = text_sketch("Abc");
    let b = sk
        .add_text_box(
            v(1.0, 30.0),
            v(41.0, 40.0),
            TextParams {
                content: "De".into(),
                style: TextStyle {
                    font: "Gibt Es Nicht Sans".into(),
                    bold: false,
                    italic: false,
                    spacing: 0.0,
                },
                height: 5.0,
                flip_h: false,
                flip_v: false,
            },
            HAlign::Left,
            VAlign::Bottom,
            shape("De"),
        )
        .unwrap();
    sk.remove_text(b).unwrap();
    let mut sf = sketch_feature(sk);
    sf.text_formulas.insert(a, "Breite".into());
    let snap = document_with_breite(vec![("Skizze1", FeatureKind::Sketch(sf))]).snapshot();
    let base: serde_json::Value = serde_json::from_str(&encode_version(&snap).unwrap()).unwrap();
    // Formel am gelöschten Text
    let mut v = base.clone();
    v["features"][0]["art"]["hoehenformeln"][0]["text"] = b.index().into();
    assert!(load_err(&v).contains(&tr!("proj-text-formula-missing-text")));
    // Text außerhalb der Liste
    let mut v = base.clone();
    v["features"][0]["art"]["hoehenformeln"][0]["text"] = 5.into();
    assert!(load_err(&v).contains(&tr!("proj-text-formula-missing-text")));
    // Doppelte Formel
    let mut v = base.clone();
    let f = v["features"][0]["art"]["hoehenformeln"][0].clone();
    v["features"][0]["art"]["hoehenformeln"] = serde_json::json!([f.clone(), f]);
    assert!(load_err(&v).contains(&tr!("proj-text-formula-twice")));
}

#[test]
fn bad_spun_and_iso_refused() {
    let id = box_id(&copy_document(CopySource::Edge(edge(1))));
    let spun = copy_document(CopySource::SpunProfile(SpunSpec {
        selection: SpunSelection::Bodies(vec![]),
        axis: SpunAxis::Own(CurveId::from_index(0)),
        tolerance: 0.1,
        flip: false,
    }));
    let spun: serde_json::Value = serde_json::from_str(&encode_version(&spun).unwrap()).unwrap();
    let tolerance = tr!(
        "proj-number-invalid",
        what = tr!(
            "proj-field",
            name = "Skizze1",
            label = tr!("proj-l-tolerance")
        )
    );
    for bad in [0.0, -1.0, 1e300] {
        let mut v = spun.clone();
        v["features"][1]["art"]["kopien"][0]["drehprofil"]["toleranz"] = bad.into();
        let e = load_err(&v);
        assert!(e.contains(&tolerance), "{bad}: {e}");
    }
    // NaN lässt sich in JSON nicht schreiben; `null` an seiner Stelle ist beschädigt.
    let mut v = spun.clone();
    v["features"][1]["art"]["kopien"][0]["drehprofil"]["toleranz"] = serde_json::Value::Null;
    load_err(&v);
    let iso = copy_document(CopySource::Isoparametric {
        face: face(id, "Wand 2"),
        direction: IsoDirection::U,
        value: 0.5,
    });
    let iso: serde_json::Value = serde_json::from_str(&encode_version(&iso).unwrap()).unwrap();
    let share = tr!(
        "proj-number-invalid",
        what = tr!(
            "proj-field",
            name = "Skizze1",
            label = tr!("proj-l-iso-share")
        )
    );
    for bad in [-0.01, 1.01, 1e300] {
        let mut v = iso.clone();
        v["features"][1]["art"]["kopien"][0]["isokurve"]["anteil"] = bad.into();
        let e = load_err(&v);
        assert!(e.contains(&share), "{bad}: {e}");
    }
    let mut v = iso.clone();
    v["features"][1]["art"]["kopien"][0]["isokurve"]["richtung"] = "w".into();
    let e = load_err(&v);
    assert!(
        e.contains(&tr!("proj-unknown-iso-direction", value = "w")),
        "{e}"
    );
}

#[test]
fn spun_profile_copy_roundtrips() {
    let base = copy_document(CopySource::Edge(edge(1)));
    let id = box_id(&base);
    let specs = [
        SpunSpec {
            selection: SpunSelection::Bodies(vec![BodyRef {
                feature: FeatureId(id),
                index: 0,
            }]),
            axis: SpunAxis::Ref(AxisRef::Standard(StdAxis::Z)),
            tolerance: 0.01,
            flip: false,
        },
        SpunSpec {
            selection: SpunSelection::Elements(vec![
                CurveSource::Edge(edge(id)),
                CurveSource::Vertex(VertexRef {
                    edge: edge(id),
                    end: true,
                }),
            ]),
            axis: SpunAxis::Own(CurveId::from_index(0)),
            tolerance: 0.25,
            flip: true,
        },
        SpunSpec {
            selection: SpunSelection::Elements(vec![CurveSource::SketchCurve {
                sketch: FeatureId(id),
                curve: CurveId::from_index(3),
            }]),
            axis: SpunAxis::Ref(AxisRef::Edge(edge(id))),
            tolerance: 0.1,
            flip: false,
        },
        SpunSpec {
            selection: SpunSelection::Bodies(vec![]),
            axis: SpunAxis::Ref(AxisRef::SketchLine {
                sketch: FeatureId(id),
                curve: CurveId::from_index(2),
            }),
            tolerance: 0.1,
            flip: false,
        },
    ];
    for spec in specs {
        let snap = copy_document(CopySource::SpunProfile(spec.clone()));
        let (back, text) = roundtrip(&snap);
        assert!(text.contains("\"drehprofil\""), "{text}");
        assert_eq!(
            sketch_of(&back, 1).copies[0].source,
            CopySource::SpunProfile(spec)
        );
    }
}

#[test]
fn isoparametric_copy_roundtrips() {
    let base = copy_document(CopySource::Edge(edge(1)));
    let id = box_id(&base);
    for (direction, value, name) in [(IsoDirection::U, 0.25, "u"), (IsoDirection::V, 1.0, "v")] {
        let source = CopySource::Isoparametric {
            face: face(id, "Wand 2"),
            direction,
            value,
        };
        let snap = copy_document(source.clone());
        // Das Dokument führt den Hinweispunkt der Fläche beim Rechnen nach (seit Skizzen 11b,
        // Task 15), darum steht im Schnappschuss nicht mehr der getippte Wert. Der Rundlauf darf
        // nichts davon verändern (`roundtrip` prüft zusätzlich Speichern, Laden, Speichern
        // bytegleich); Fläche (ohne Hinweis), Richtung und Anteil bleiben die der Quelle.
        let stored = sketch_of(&snap, 1).copies[0].source.clone();
        let (back, text) = roundtrip(&snap);
        assert_eq!(sketch_of(&back, 1).copies[0].source, stored);
        match (&stored, &source) {
            (
                CopySource::Isoparametric {
                    face: f,
                    direction: d,
                    value: x,
                },
                CopySource::Isoparametric {
                    face: g,
                    direction: e,
                    value: y,
                },
            ) => {
                assert_eq!((f.origin, &f.role, d, x), (g.origin, &g.role, e, y));
            }
            other => panic!("{other:?}"),
        }
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let iso = &v["features"][1]["art"]["kopien"][0]["isokurve"];
        assert_eq!(iso["richtung"], name, "{text}");
        assert_eq!(iso["anteil"], value, "{text}");
        assert_eq!(iso["flaeche"]["rolle"], "Wand 2", "{text}");
    }
}

/// Eine Datei, die sich als Format 21 ausgibt, aber Inhalt aus Format 22 hat, ist beschädigt.
#[test]
fn format_21_file_with_22_data_refused() {
    let expected = tr!(
        "proj-since-format",
        what = tr!("proj-feat-sketch-transfer"),
        format = "22"
    );
    let as_21 = |snap: &Snapshot| {
        encode_version(snap)
            .unwrap()
            .replacen("\"format\": 22", "\"format\": 21", 1)
    };
    let err = |text: &str| decode_version(text, "x.json").unwrap_err();
    // Skizzengruppen
    let d = three_sketches();
    let mut snap = d.snapshot();
    snap.sketch_groups = nested_groups(&d);
    let e = err(&as_21(&snap));
    assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
    assert!(e.to_string().contains(&expected), "{e}");
    // Höhenformel
    let (sk, t) = text_sketch("Abc");
    let mut sf = sketch_feature(sk);
    sf.text_formulas.insert(t, "3 mm".into());
    let snap = document_with(vec![("Skizze1", FeatureKind::Sketch(sf))]).snapshot();
    assert!(err(&as_21(&snap)).to_string().contains(&expected));
    // Nur die nächste Gruppenkennung (ohne Gruppen)
    let mut v = grouped_value();
    v["skizzengruppen"] = serde_json::json!([]);
    v["format"] = 21.into();
    let e = load_err(&v);
    assert!(e.contains(&expected), "{e}");
    // Isokurve
    let id = box_id(&copy_document(CopySource::Edge(edge(1))));
    let snap = copy_document(CopySource::Isoparametric {
        face: face(id, "Wand 2"),
        direction: IsoDirection::U,
        value: 0.5,
    });
    assert!(err(&as_21(&snap)).to_string().contains(&expected));
}

/// Task 8: Was das Programm selbst nicht mehr lädt oder still verlöre, wird nie geschrieben:
/// Mitglied mit unbekannter Gruppe, Mitglied einer fehlenden Skizze, Mitglied ohne Gruppen.
#[test]
fn broken_groups_are_never_written() {
    let d = three_sketches();
    let ids: Vec<FeatureId> = d.snapshot().features.iter().map(|f| f.id).collect();
    let group = |id: u64| SketchGroup {
        id: SketchGroupId(id),
        name: "G".into(),
        parent: None,
        hidden: false,
    };
    type Break<'a> = Box<dyn Fn(&mut Snapshot) + 'a>;
    let broken: Vec<Break> = vec![
        // Unbekannte Gruppenkennung (fiele beim Schreiben still weg).
        Box::new(|s| {
            s.sketch_groups.groups.push(group(0));
            s.sketch_groups.next = 1;
            s.sketch_groups.member.insert(ids[0], SketchGroupId(5));
        }),
        // Mitglied ohne jede Gruppe.
        Box::new(|s| {
            s.sketch_groups.member.insert(ids[0], SketchGroupId(0));
        }),
        // Skizze gibt es nicht.
        Box::new(|s| {
            s.sketch_groups.groups.push(group(0));
            s.sketch_groups.next = 1;
            s.sketch_groups
                .member
                .insert(FeatureId(999), SketchGroupId(0));
        }),
    ];
    for (k, f) in broken.iter().enumerate() {
        let mut s = d.snapshot();
        f(&mut s);
        match encode_version(&s) {
            Err(ProjectError::Invalid(_)) => {}
            other => panic!("Fall {k}: {other:?}"),
        }
    }
    // Ein gültiger Stand geht durch.
    let mut s = d.snapshot();
    s.sketch_groups.groups.push(group(0));
    s.sketch_groups.next = 1;
    s.sketch_groups.member.insert(ids[0], SketchGroupId(0));
    assert_eq!(format_of(&encode_version(&s).unwrap()), 22);
}
