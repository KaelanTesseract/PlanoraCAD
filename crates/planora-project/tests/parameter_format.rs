//! Dateiformat 9: Parametertabelle und Formeln (Benannte Parameter, Aufgabe 7). Format 9 nur
//! mit Parametern oder Formeln, alte Dateien Byte für Byte wie bisher, Rundreise, abgelehnte
//! Dateien mit deutscher Meldung, eine kaputte Feature-Formel lädt.

use planora_drawing::{Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_feature::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_math::{DVec3, Frame, Tolerance};
use planora_project::docstore::{DocStore, LockMode};
use planora_project::fsutil::new_id;
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::{ConstraintId, ConstraintKind, LineOrPoints, Sketch};
use serde_json::{Value, json};

use ParamUnit::{Angle, Length, Number};

// ---- Bausteine ----------------------------------------------------------------------------

fn box_kind() -> FeatureKind {
    FeatureKind::Box {
        size: DVec3::new(30.0, 20.0, 10.0),
        origin: DVec3::ZERO,
    }
}

fn quader() -> Document {
    let mut d = Document::new();
    d.add_feature("Quader 1", box_kind()).unwrap();
    d
}

fn formulas(list: &[(FieldKey, &str)]) -> Formulas {
    list.iter().map(|(k, t)| (*k, t.to_string())).collect()
}

/// Skizze: Punkte p0, p1, p2, Linie p0–p1. Abhängigkeiten in dieser Reihenfolge:
/// 0 Fest(p0) (wird gelöscht), 1 Abstand p0–p1 = 10, 2 Abstand p1–p2 = 10, 3 Waagerecht (Linie).
fn sketch_kind(formula_at_2: Option<&str>) -> (FeatureKind, [ConstraintId; 4]) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let p0 = sk.add_point(0.0, 0.0).unwrap();
    let p1 = sk.add_point(10.0, 0.0).unwrap();
    let p2 = sk.add_point(10.0, 10.0).unwrap();
    let line = sk.add_line(p0, p1).unwrap();
    let fixed = sk
        .add_constraint(ConstraintKind::Fixed(p0, 0.0, 0.0))
        .unwrap();
    let d1 = sk
        .add_constraint(ConstraintKind::Distance(p0, p1, 10.0))
        .unwrap();
    let d2 = sk
        .add_constraint(ConstraintKind::Distance(p1, p2, 10.0))
        .unwrap();
    let horizontal = sk
        .add_constraint(ConstraintKind::Horizontal(LineOrPoints::Line(line)))
        .unwrap();
    // Die erste Abhängigkeit wird gelöscht: Die Formel steht danach an Index 2.
    sk.remove_constraint(fixed).unwrap();
    let mut f = SketchFormulas::new();
    if let Some(t) = formula_at_2 {
        f.insert(d2, t.to_string());
    }
    (
        FeatureKind::Sketch(SketchFeature {
            sketch: sk,
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas: f,
        }),
        [fixed, d1, d2, horizontal],
    )
}

/// Kante Deckel/Wand 1 des Körpers.
fn top_edge(d: &Document, body: BodyRef) -> EdgeRef {
    let s = d.body(body).unwrap();
    let e = s
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.contains(&"Deckel".to_string()) && n.contains(&"Wand 1".to_string())
        })
        .unwrap();
    EdgeRef::from_edge(s, e, &Tolerance::default()).unwrap()
}

/// Dokument für Rundreise und Dateigestalt: drei Parameter (Länge mit Kommentar, Winkel,
/// Zahl), Quader mit Formeln, Fase mit Winkelformel, Skizze mit Formel an Abhängigkeit 2
/// (nach gelöschter erster Abhängigkeit).
fn full_document() -> Document {
    let mut d = Document::new();
    d.add_parameter("Breite", "40", Length, "Außenmaß").unwrap();
    d.add_parameter("w", "30", Angle, "").unwrap();
    d.add_parameter("a", "2", Number, "").unwrap();
    let q = d
        .add_feature_with_formulas(
            "Quader 1",
            box_kind(),
            formulas(&[(FieldKey::Breite, "Breite"), (FieldKey::Hoehe, "Breite/4")]),
        )
        .unwrap();
    let body = BodyRef {
        feature: q,
        index: 0,
    };
    let edge = top_edge(&d, body);
    d.add_feature_with_formulas(
        "Fase 1",
        FeatureKind::Verrunden {
            body,
            edges: vec![edge],
            art: BlendArt::FaseWinkel {
                abstand: 1.0,
                winkel: 45.0,
            },
            tangentenkette: false,
        },
        formulas(&[(FieldKey::Winkel, "w")]),
    )
    .unwrap();
    let (kind, _) = sketch_kind(Some("a"));
    d.add_feature_with_formulas("Skizze 1", kind, Formulas::new())
        .unwrap();
    d
}

fn bad(t: &str) -> ProjectError {
    decode_version(t, "x.json").unwrap_err()
}

fn msg(t: &str) -> String {
    let e = bad(t);
    assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
    e.to_string()
}

fn edited(text: &str, f: impl FnOnce(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(text).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

/// Lesen wie in der Anwendung: Datei lesen, dann `Document::from_snapshot`.
fn load(t: &str) -> Result<Document, String> {
    let s = decode_version(t, "x.json").map_err(|e| e.to_string())?;
    Document::from_snapshot(s, 0.05).map_err(|e| e.to_string())
}

/// Die Datei, die das Dokument schreiben würde, als JSON-Wert.
fn saved(d: &Document) -> Value {
    serde_json::from_str(&encode_version(&d.snapshot()).unwrap()).unwrap()
}

fn sketch_of(d: &Document, name: &str) -> SketchFeature {
    match &d.features().iter().find(|f| f.name == name).unwrap().kind {
        FeatureKind::Sketch(sf) => sf.clone(),
        k => panic!("{k:?}"),
    }
}

fn dim_value(sf: &SketchFeature, id: ConstraintId) -> f64 {
    match sf.sketch.constraint(id).unwrap().kind {
        ConstraintKind::Distance(_, _, v) => v,
        ref k => panic!("{k:?}"),
    }
}

fn section_document() -> Document {
    let mut d = quader();
    let id = d
        .add_drawing(
            "Blatt",
            Sheet {
                paper: Paper::A3,
                landscape: true,
            },
            Projection::First,
        )
        .unwrap();
    let front = d
        .add_view(
            id,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 0.0, y: 0.0 },
            None,
        )
        .unwrap();
    d.update_drawing(id, |dr| {
        dr.add_section(front, ViewKind::Top, 0.0, 80.0).unwrap();
    })
    .unwrap();
    d
}

const NEW_KEYS: [&str; 3] = ["\"parameter\"", "\"formeln\"", "\"naechster_parameter\""];

// ---- (a) Formatnummer ---------------------------------------------------------------------

#[test]
fn format_9_only_with_parameters_or_formulas() {
    assert_eq!(FORMAT_VERSION, 12);
    let mut d = quader();
    assert_eq!(required_format(&d.snapshot()), 1);

    // Nur ein Parameter (ohne Formel irgendwo): 9.
    let p = d.add_parameter("Breite", "40", Length, "").unwrap();
    assert_eq!(required_format(&d.snapshot()), 9);
    let text = encode_version(&d.snapshot()).unwrap();
    assert!(text.contains("\"format\": 9") && text.contains("\"naechster_parameter\": 2"));
    // Parameter gelöscht: wieder 1, ohne neue Felder (auch ohne Zähler).
    d.remove_parameter(p).unwrap();
    assert_eq!(required_format(&d.snapshot()), 1);
    let text = encode_version(&d.snapshot()).unwrap();
    for k in NEW_KEYS {
        assert!(!text.contains(k), "{k}: {text}");
    }

    // Nur eine Feature-Formel ohne Parameter: 9.
    let mut f = Document::new();
    f.add_feature_with_formulas(
        "Quader 1",
        box_kind(),
        formulas(&[(FieldKey::Breite, "2*5")]),
    )
    .unwrap();
    assert_eq!(required_format(&f.snapshot()), 9);
    let text = encode_version(&f.snapshot()).unwrap();
    assert!(text.contains("\"format\": 9") && !text.contains("\"parameter\""));
    assert!(!text.contains("naechster_parameter"), "{text}");

    // Nur eine Skizzenformel: 9; dieselbe Skizze ohne Formel: 1.
    let mut s = Document::new();
    let (kind, _) = sketch_kind(Some("12"));
    s.add_feature_with_formulas("Skizze 1", kind, Formulas::new())
        .unwrap();
    assert_eq!(required_format(&s.snapshot()), 9);
    let mut s1 = Document::new();
    s1.add_feature("Skizze 1", sketch_kind(None).0).unwrap();
    assert_eq!(required_format(&s1.snapshot()), 1);

    // Regel 9 vor Regel 8: Schnittansicht und Parameter ergeben 9, die Schnittansicht bleibt.
    let mut sec = section_document();
    assert_eq!(required_format(&sec.snapshot()), 8);
    sec.add_parameter("k", "1", Number, "").unwrap();
    assert_eq!(required_format(&sec.snapshot()), 9);
    let text = encode_version(&sec.snapshot()).unwrap();
    assert!(text.contains("\"format\": 9") && text.contains("\"schnitt\""));
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(back.drawings, sec.snapshot().drawings);
}

// ---- (b) Rundreise ------------------------------------------------------------------------

#[test]
fn roundtrip() {
    let d = full_document();
    let snap = d.snapshot();
    assert_eq!(required_format(&snap), 9);
    let text = encode_version(&snap).unwrap();
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(back.parameters, snap.parameters, "Tabelle gleich");
    assert_eq!(back.next_param, snap.next_param);
    assert_eq!(back.next_param, 4);
    for (a, b) in snap.features.iter().zip(&back.features) {
        assert_eq!((a.id, &a.name, &a.formulas), (b.id, &b.name, &b.formulas));
    }
    let e = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(e.parameters(), d.parameters());
    for p in d.parameters() {
        assert_eq!(
            e.parameter_value(p.id),
            d.parameter_value(p.id),
            "{}",
            p.name
        );
    }
    for (a, b) in d.features().iter().zip(e.features()) {
        assert_eq!(a.formulas(), b.formulas(), "{}", a.name);
        assert_eq!(a.status, b.status, "{}", a.name);
        match (&a.kind, &b.kind) {
            (FeatureKind::Sketch(x), FeatureKind::Sketch(y)) => {
                assert_eq!(x.formulas, y.formulas);
                let [_, _, d2, _] = sketch_kind(None).1;
                assert_eq!(dim_value(x, d2), 2.0, "Formelwert in der Skizze");
                assert_eq!(dim_value(x, d2), dim_value(y, d2));
            }
            (x, y) => assert_eq!(x, y, "{}", a.name),
        }
    }
    // Die Werte stehen in den Feldern: Breite 40, Höhe 10, Winkel 30.
    let q = &e.features()[0];
    assert_eq!(field_value(&q.kind, FieldKey::Breite), Some(40.0));
    assert_eq!(field_value(&q.kind, FieldKey::Hoehe), Some(10.0));
    assert_eq!(
        field_value(&e.features()[1].kind, FieldKey::Winkel),
        Some(30.0)
    );
    assert_eq!(q.status, FeatureStatus::Ok);
    assert_eq!(e.features()[1].status, FeatureStatus::Ok);
    // Erneut geschrieben: zeichengleich.
    assert_eq!(encode_version(&e.snapshot()).unwrap(), text);
}

#[test]
fn versions_and_autosave_keep_parameters() {
    let p = std::env::temp_dir().join(format!("planora-param-{}.pcad", new_id()));
    let d = full_document();
    let text = encode_version(&d.snapshot()).unwrap();
    let mut st = DocStore::create(&p, &quader().snapshot(), "").unwrap();
    let v = st.save(Some(1), &d.snapshot(), "Parameter", None).unwrap();
    assert_eq!(encode_version(&st.load_version(v).unwrap()).unwrap(), text);
    st.write_autosave(Some(v), &d.snapshot()).unwrap();
    let auto = st.read_autosave().unwrap().unwrap();
    assert_eq!(encode_version(&auto.snapshot).unwrap(), text);
    // Die alte Version ohne Parameter bleibt Format 1.
    let old = encode_version(&st.load_version(1).unwrap()).unwrap();
    assert!(old.contains("\"format\": 1") && !old.contains("\"parameter\""));
    drop(st);
    let (st, _) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert_eq!(encode_version(&st.load_version(v).unwrap()).unwrap(), text);
    drop(st);
    std::fs::remove_dir_all(&p).unwrap();
}

// ---- (c) Gestalt der Datei ----------------------------------------------------------------

#[test]
fn json_shape() {
    let text = encode_version(&full_document().snapshot()).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["format"], 9);
    assert_eq!(
        v["parameter"],
        json!([
            {"id": 1, "name": "Breite", "formel": "40", "art": "laenge", "kommentar": "Außenmaß"},
            {"id": 2, "name": "w", "formel": "30", "art": "winkel"},
            {"id": 3, "name": "a", "formel": "2", "art": "zahl"},
        ])
    );
    assert_eq!(v["naechster_parameter"], 4);
    let f = &v["features"];
    assert_eq!(
        f[0]["formeln"],
        json!({"breite": "Breite", "hoehe": "Breite/4"})
    );
    assert_eq!(f[1]["formeln"], json!({"winkel": "w"}));
    assert_eq!(
        f[2]["art"]["formeln"],
        json!([{"abhaengigkeit": 2, "formel": "a"}])
    );
    // Die Skizze selbst trägt kein Feature-Feld „formeln“, die Fase keinen Kommentar.
    assert!(f[2].get("formeln").is_none());
    assert!(v["parameter"][1].get("kommentar").is_none());
}

// ---- (d) Alte Dateien ---------------------------------------------------------------------

#[test]
fn old_files_unchanged() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut n = 0;
    for sub in ["data", "fixtures"] {
        for entry in std::fs::read_dir(root.join(sub)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).unwrap();
            let s = decode_version(&text, &name).unwrap();
            assert!(s.parameters.is_empty(), "{name}");
            assert!(required_format(&s) <= 8, "{name}");
            let again = encode_version(&s).unwrap();
            assert_eq!(
                again.replace("\r\n", "\n"),
                text.replace("\r\n", "\n"),
                "{name}"
            );
            // Auch über das Dokument (wie Öffnen und Speichern in der Anwendung).
            let doc = Document::from_snapshot(s, 0.05).unwrap();
            assert_eq!(
                encode_version(&doc.snapshot())
                    .unwrap()
                    .replace("\r\n", "\n"),
                text.replace("\r\n", "\n"),
                "{name}"
            );
            for k in NEW_KEYS {
                assert!(!again.contains(k), "{name}: {k}");
            }
            n += 1;
        }
    }
    assert_eq!(n, 11, "alle Golden-Dateien");

    // Ein Schnitt-Dokument ohne Parameter: Format 8 wie bisher, ohne neue Felder.
    let text = encode_version(&section_document().snapshot()).unwrap();
    assert!(text.contains("\"format\": 8"), "{text}");
    for k in NEW_KEYS {
        assert!(!text.contains(k), "{k}");
    }
}

// ---- (e) Abgelehnte und milde gelesene Dateien --------------------------------------------

/// Eine Datei mit `naechster_parameter = u64::MAX - 1` lädt; ein neuer Parameter wird dann mit
/// Meldung abgelehnt, statt einen Stand zu erzeugen, der sich nicht mehr laden ließe.
#[test]
fn counter_just_below_max_loads_but_refuses_more() {
    let mut d = Document::new();
    d.add_parameter("Breite", "40", Length, "").unwrap();
    let text = edited(&encode_version(&d.snapshot()).unwrap(), |v| {
        v["naechster_parameter"] = json!(u64::MAX - 1);
    });
    let mut loaded = load(&text).unwrap();
    assert_eq!(
        loaded
            .add_parameter("Tiefe", "20", Length, "")
            .unwrap_err()
            .to_string(),
        "Ungültige Eingabe: Die Parameter-Kennungen sind aufgebraucht"
    );
    let again = encode_version(&loaded.snapshot()).unwrap();
    assert!(load(&again).is_ok(), "gespeichert lädt der Stand wieder");
}

#[test]
fn rejected_and_lenient() {
    // Grundlage ohne Formeln: Quader und Skizze, Format 1.
    let mut plain = quader();
    plain.add_feature("Skizze 1", sketch_kind(None).0).unwrap();
    let base = encode_version(&plain.snapshot()).unwrap();
    assert!(base.contains("\"format\": 1"));
    let param = json!([{"id": 1, "name": "Breite", "formel": "40", "art": "laenge"}]);

    // Neue Felder vor Format 9: beschädigt, mit dem Hinweis auf Format 9.
    type Change<'a> = Box<dyn Fn(&mut Value) + 'a>;
    let only_9: Vec<(&str, Change)> = vec![
        ("parameter", Box::new(|v| v["parameter"] = param.clone())),
        ("zähler", Box::new(|v| v["naechster_parameter"] = 3.into())),
        (
            "feature-formel",
            Box::new(|v| v["features"][0]["formeln"] = json!({"breite": "2*5"})),
        ),
        (
            "skizzenformel",
            Box::new(|v| {
                v["features"][1]["art"]["formeln"] = json!([{"abhaengigkeit": 2, "formel": "5"}])
            }),
        ),
    ];
    for (what, f) in &only_9 {
        for older in 1..=8 {
            let t = edited(&base, |v| {
                f(v);
                v["format"] = older.into();
            });
            let m = msg(&t);
            assert!(
                m.contains("Parameter und Formeln gibt es erst ab Dateiformat 9"),
                "{what}, Format {older}: {m}"
            );
        }
        // Mit Format 9 lädt dieselbe Datei.
        let t = edited(&base, |v| {
            f(v);
            v["format"] = 9.into();
        });
        load(&t).unwrap_or_else(|e| panic!("{what}: {e}"));
    }

    let with = |f: &dyn Fn(&mut Value)| {
        edited(&base, |v| {
            v["format"] = 9.into();
            v["parameter"] = param.clone();
            f(v);
        })
    };
    assert!(load(&with(&|_| {})).is_ok());

    // Unbekannte Art.
    let m = msg(&with(&|v| v["parameter"][0]["art"] = "laenge2".into()));
    assert!(
        m.contains("Parameter „Breite“: unbekannte Art „laenge2“"),
        "{m}"
    );
    // Art mit falschem Typ, Liste statt Tabelle: beschädigt, kein Absturz.
    for t in [
        with(&|v| v["parameter"][0]["art"] = 3.into()),
        with(&|v| v["parameter"] = "x".into()),
        with(&|v| v["parameter"][0]["formel"] = 40.into()),
        with(&|v| v["parameter"][0]["id"] = (-1).into()),
        with(&|v| v["features"][0]["formeln"] = 5.into()),
        with(&|v| v["features"][0]["formeln"] = json!({"breite": 5})),
        with(&|v| v["features"][1]["art"]["formeln"] = json!({"a": 1})),
        with(&|v| {
            v["features"][1]["art"]["formeln"] = json!([{"abhaengigkeit": -1, "formel": "1"}])
        }),
        with(&|v| v["naechster_parameter"] = "1".into()),
    ] {
        msg(&t);
    }

    // Unbekannter und unpassender Feldschlüssel.
    let m = msg(&with(&|v| {
        v["features"][0]["formeln"] = json!({"foo": "1"})
    }));
    assert!(
        m.contains("„Quader 1“: unbekanntes Formelfeld „foo“"),
        "{m}"
    );
    let m = msg(&with(&|v| {
        v["features"][0]["formeln"] = json!({"Breite": "1"})
    }));
    assert!(m.contains("unbekanntes Formelfeld „Breite“"), "{m}");
    let m = msg(&with(&|v| {
        v["features"][0]["formeln"] = json!({"radius": "1"})
    }));
    assert!(
        m.contains("„Quader 1“: Das Feld „radius“ kann keine Formel haben"),
        "{m}"
    );
    // Ein Feldschlüssel doppelt (nur im Text möglich).
    let good = with(&|v| v["features"][0]["formeln"] = json!({"breite": "1"}));
    let dup = good.replacen(
        "\"breite\": \"1\"",
        "\"breite\": \"1\", \"breite\": \"2\"",
        1,
    );
    assert_ne!(dup, good);
    assert!(msg(&dup).contains("kommt doppelt vor"), "{}", msg(&dup));

    // Skizzenformeln an fehlender, gelöschter, nicht maßgebender oder anzeigender
    // Abhängigkeit: beschädigt.
    let sketch_formula = |i: u64| {
        move |v: &mut Value| {
            v["features"][1]["art"]["formeln"] = json!([{"abhaengigkeit": i, "formel": "5"}])
        }
    };
    for i in [99, 4, 0, (1u64 << 32) + 2] {
        let m = msg(&with(&sketch_formula(i)));
        assert!(
            m.contains("„Skizze 1“: Formel an einer fehlenden Abhängigkeit"),
            "{i}: {m}"
        );
    }
    let m = msg(&with(&sketch_formula(3)));
    assert!(
        m.contains("„Skizze 1“: Formel an einer Abhängigkeit, die kein bestimmendes Maß ist"),
        "{m}"
    );
    let m = msg(&with(&|v| {
        sketch_formula(2)(v);
        v["features"][1]["art"]["skizze"]["abhaengigkeiten"][2]["gesteuert"] = true.into();
    }));
    assert!(m.contains("kein bestimmendes Maß"), "{m}");
    let m = msg(&with(&|v| {
        v["features"][1]["art"]["formeln"] = json!([
            {"abhaengigkeit": 2, "formel": "5"},
            {"abhaengigkeit": 2, "formel": "6"},
        ])
    }));
    assert!(
        m.contains("„Skizze 1“: Eine Abhängigkeit hat mehr als eine Formel"),
        "{m}"
    );
    // An einem abgeschalteten Maß bleibt die Formel (Aufgabe 6, Review F5).
    let off = with(&|v| {
        sketch_formula(2)(v);
        v["features"][1]["art"]["skizze"]["abhaengigkeiten"][2]["aktiv"] = false.into();
    });
    let doc = load(&off).unwrap();
    let sf = sketch_of(&doc, "Skizze 1");
    assert_eq!(sf.formulas.len(), 1);
    assert_eq!(
        saved(&doc)["features"][1]["art"]["formeln"],
        json!([{"abhaengigkeit": 2, "formel": "5"}])
    );

    // Grenzen: Formel 201 Zeichen (Feature, Skizze, Parameter), Name 65, Kommentar 201,
    // 501 Parameter; genau an der Grenze lädt die Datei.
    let long = format!("1{}", "+0".repeat(100)); // 201 Zeichen
    assert_eq!(long.chars().count(), 201);
    let ok = &long[..199];
    let m = msg(&with(&|v| {
        v["features"][0]["formeln"] = json!({"breite": long})
    }));
    assert!(
        m.contains("„Quader 1“: Die Formel ist zu lang (höchstens 200 Zeichen)"),
        "{m}"
    );
    assert!(
        load(&with(
            &|v| v["features"][0]["formeln"] = json!({"breite": ok})
        ))
        .is_ok()
    );
    let m = msg(&with(&|v| {
        v["features"][1]["art"]["formeln"] = json!([{"abhaengigkeit": 2, "formel": long}])
    }));
    assert!(m.contains("„Skizze 1“: Die Formel ist zu lang"), "{m}");
    let m = msg(&with(&|v| {
        v["parameter"][0]["formel"] = long.clone().into()
    }));
    assert!(
        m.contains("Parameter „Breite“: Die Formel ist zu lang"),
        "{m}"
    );
    let m = msg(&with(&|v| {
        v["parameter"][0]["name"] = "n".repeat(65).into()
    }));
    assert!(m.contains("Name ist zu lang (höchstens 64 Zeichen)"), "{m}");
    assert!(load(&with(&|v| v["parameter"][0]["name"] = "n".repeat(64).into())).is_ok());
    let m = msg(&with(&|v| {
        v["parameter"][0]["kommentar"] = "k".repeat(201).into()
    }));
    assert!(
        m.contains("Parameter „Breite“: Der Kommentar ist zu lang (höchstens 200 Zeichen)"),
        "{m}"
    );
    assert!(
        load(&with(
            &|v| v["parameter"][0]["kommentar"] = "k".repeat(200).into()
        ))
        .is_ok()
    );
    let many = |n: u64| {
        move |v: &mut Value| {
            v["parameter"] = (1..=n)
                .map(|i| json!({"id": i, "name": format!("p{i}"), "formel": "1", "art": "zahl"}))
                .collect();
        }
    };
    let m = msg(&with(&many(501)));
    assert!(m.contains("Höchstens 500 Parameter"), "{m}");
    assert!(load(&with(&many(500))).is_ok());

    // Kennungen: doppelt, 0, Zähler zu klein oder u64::MAX.
    let two = json!([
        {"id": 1, "name": "a", "formel": "1", "art": "zahl"},
        {"id": 1, "name": "b", "formel": "2", "art": "zahl"},
    ]);
    let m = msg(&with(&|v| v["parameter"] = two.clone()));
    assert!(
        m.contains("Parameter „b“: Die Kennung kommt doppelt vor"),
        "{m}"
    );
    let m = msg(&with(&|v| v["parameter"][0]["id"] = 0.into()));
    assert!(m.contains("Parameter: ungültige Kennung"), "{m}");
    let m = msg(&with(&|v| v["naechster_parameter"] = 1.into()));
    assert!(m.contains("Parameter: ungültige Kennung"), "{m}");
    let m = msg(&with(&|v| v["naechster_parameter"] = u64::MAX.into()));
    assert!(m.contains("Parameter: Der Zähler ist ungültig"), "{m}");
    // Fehlender Zähler: aus den Kennungen abgeleitet.
    let t = with(&|v| v["parameter"][0]["id"] = 7.into());
    assert_eq!(decode_version(&t, "x").unwrap().next_param, 8);

    // Die Tabelle selbst prüft das Dokument: doppelte Namen, Kreis, unbekannter Name,
    // reservierter Name, falsche Einheit.
    let table = |list: Value| with(&move |v| v["parameter"] = list.clone());
    let cases = [
        (
            json!([
                {"id": 1, "name": "a", "formel": "1", "art": "zahl"},
                {"id": 2, "name": "A", "formel": "2", "art": "zahl"},
            ]),
            "gibt es schon",
        ),
        (
            json!([
                {"id": 1, "name": "a", "formel": "b + 1", "art": "zahl"},
                {"id": 2, "name": "b", "formel": "a", "art": "zahl"},
            ]),
            "im Kreis",
        ),
        (
            json!([{"id": 1, "name": "a", "formel": "Gibtsnicht", "art": "zahl"}]),
            "Gibtsnicht",
        ),
        (
            json!([{"id": 1, "name": "sin", "formel": "1", "art": "zahl"}]),
            "sin",
        ),
        (
            json!([{"id": 1, "name": "a", "formel": "(", "art": "zahl"}]),
            "Parameter „a“",
        ),
    ];
    for (list, part) in cases {
        let t = table(list);
        // Die Datei selbst ist lesbar, das Dokument lehnt die Tabelle ab.
        decode_version(&t, "x").unwrap();
        let e = load(&t).unwrap_err();
        assert!(
            e.contains("Parametertabelle") && e.contains(part),
            "{part}: {e}"
        );
    }

    // Eine kaputte Feature-Formel lädt: Fehlerstatus, Feld mit dem Wert aus der Datei.
    for broken in ["Gibtsnicht", "2*(", "Breite + 1°"] {
        let t = with(&|v| v["features"][0]["formeln"] = json!({"breite": broken}));
        let doc = load(&t).unwrap_or_else(|e| panic!("{broken}: {e}"));
        let q = &doc.features()[0];
        assert!(
            matches!(q.status, FeatureStatus::Error(_)),
            "{broken}: {:?}",
            q.status
        );
        assert_eq!(
            field_value(&q.kind, FieldKey::Breite),
            Some(30.0),
            "{broken}"
        );
        assert_eq!(q.formulas().len(), 1);
        // Gespeichert bleiben Formel und Wert aus der Datei.
        let again = saved(&doc);
        assert_eq!(again["features"][0]["formeln"], json!({"breite": broken}));
        assert_eq!(again["features"][0]["art"]["groesse"][0], 30.0);
    }
    // Eine kaputte Skizzenformel lädt ebenso (Skizze im Fehlerstatus).
    let t = with(&|v| {
        v["features"][1]["art"]["formeln"] = json!([{"abhaengigkeit": 2, "formel": "Gibtsnicht"}])
    });
    let doc = load(&t).unwrap();
    assert!(matches!(doc.features()[1].status, FeatureStatus::Error(_)));
    assert_eq!(
        saved(&doc)["features"][1]["art"]["formeln"],
        json!([{"abhaengigkeit": 2, "formel": "Gibtsnicht"}])
    );
}

/// Format-9-Text: Quader 1, Skizze 1 und Parameter „Breite“; `f` ändert ihn danach.
fn text9(f: impl FnOnce(&mut Value)) -> String {
    let mut plain = quader();
    plain.add_feature("Skizze 1", sketch_kind(None).0).unwrap();
    edited(&encode_version(&plain.snapshot()).unwrap(), |v| {
        v["format"] = 9.into();
        v["parameter"] = json!([{"id": 1, "name": "Breite", "formel": "40", "art": "laenge"}]);
        f(v);
    })
}

#[test]
fn foreign_text_in_messages_is_short() {
    // 5 MB aus „ß“ (zwei Bytes je Zeichen): gekürzt auf 39 Zeichen und „…“, ohne ein Zeichen
    // zu zerschneiden.
    let huge = "ß".repeat(2_500_000);
    assert_eq!(huge.len(), 5_000_000);
    let short = format!("„{}…“", "ß".repeat(39));
    let check = |m: String, start: &str| {
        assert!(m.chars().count() < 200, "Meldung zu lang: {}", m.len());
        assert!(m.contains(&format!("{start}{short}")), "{m}");
    };
    check(
        msg(&text9(|v| v["parameter"][0]["art"] = huge.clone().into())),
        "Parameter „Breite“: unbekannte Art ",
    );
    check(
        msg(&text9(|v| {
            v["features"][0]["formeln"] = json!({ huge.clone(): "1" });
        })),
        "„Quader 1“: unbekanntes Formelfeld ",
    );
    // Doppelter langer Feldschlüssel (nur im Text möglich).
    let one = text9(|v| v["features"][0]["formeln"] = json!({ "QQ": "1" }));
    let dup = one.replacen(
        "\"QQ\": \"1\"",
        &format!("\"{huge}\": \"1\", \"{huge}\": \"2\""),
        1,
    );
    check(msg(&dup), "Das Formelfeld ");
    // Genau 40 Zeichen bleiben ganz, 41 werden gekürzt.
    let forty = "ä".repeat(40);
    let m = msg(&text9(|v| v["parameter"][0]["art"] = forty.clone().into()));
    assert!(m.contains(&format!("unbekannte Art „{forty}“")), "{m}");
    let m = msg(&text9(|v| v["parameter"][0]["art"] = "ä".repeat(41).into()));
    assert!(
        m.contains(&format!("unbekannte Art „{}…“", "ä".repeat(39))),
        "{m}"
    );
}

#[test]
fn limits_count_characters_not_bytes() {
    // Name: 64 × „ä“ (128 Bytes) lädt, 65 nicht.
    assert!(load(&text9(|v| v["parameter"][0]["name"] = "ä".repeat(64).into())).is_ok());
    let m = msg(&text9(|v| {
        v["parameter"][0]["name"] = "ä".repeat(65).into()
    }));
    assert!(m.contains("Name ist zu lang"), "{m}");
    // Kommentar: 200 × „ö“ lädt, 201 nicht.
    let ok = load(&text9(|v| {
        v["parameter"][0]["kommentar"] = "ö".repeat(200).into()
    }))
    .unwrap();
    assert_eq!(ok.parameters()[0].comment.chars().count(), 200);
    let m = msg(&text9(|v| {
        v["parameter"][0]["kommentar"] = "ö".repeat(201).into()
    }));
    assert!(m.contains("Kommentar ist zu lang"), "{m}");
    // Formel mit „ü“: 200 Zeichen werden gelesen, 201 nicht (Feature, Skizze, Parameter).
    let f200 = format!("1+{}", "ü".repeat(198));
    let f201 = format!("1+{}", "ü".repeat(199));
    assert_eq!((f200.chars().count(), f201.chars().count()), (200, 201));
    let feature = |t: &str| {
        let t = t.to_string();
        text9(move |v| v["features"][0]["formeln"] = json!({ "breite": t }))
    };
    let doc = load(&feature(&f200)).unwrap();
    assert!(matches!(doc.features()[0].status, FeatureStatus::Error(_)));
    assert!(msg(&feature(&f201)).contains("Die Formel ist zu lang"));
    let sketch = |t: &str| {
        let t = t.to_string();
        text9(move |v| {
            v["features"][1]["art"]["formeln"] = json!([{"abhaengigkeit": 2, "formel": t}])
        })
    };
    assert!(load(&sketch(&f200)).is_ok());
    assert!(msg(&sketch(&f201)).contains("Die Formel ist zu lang"));
    let param = |t: &str| {
        let t = t.to_string();
        text9(move |v| v["parameter"][0]["formel"] = t.into())
    };
    assert_eq!(
        decode_version(&param(&f200), "x").unwrap().parameters[0].formula,
        f200
    );
    assert!(msg(&param(&f201)).contains("Parameter „Breite“: Die Formel ist zu lang"));
}

// ---- (f) Zu neu ---------------------------------------------------------------------------

#[test]
fn too_new() {
    let good = encode_version(&full_document().snapshot()).unwrap();
    let e = bad(&good.replacen("\"format\": 9", "\"format\": 13", 1));
    assert!(matches!(e, ProjectError::TooNew { found: 13, .. }), "{e:?}");
    let e = bad("{\"format\": 13, \"parameter\": \"ganz anders\"}");
    assert!(matches!(e, ProjectError::TooNew { found: 13, .. }), "{e:?}");
    // Format 9 selbst ist nicht zu neu.
    assert!(decode_version(&good, "x").is_ok());
}
