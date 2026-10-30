//! Alle Meldungen von planora-feature (Fehlertypen, Formelfehler, Bezeichnungen, Konstanten)
//! erscheinen unter Englisch ohne deutsche Zeichen und ohne Schlüssel-Ids. Dieser Test ruft nie
//! `set_global`.

use planora_feature::expr::{ParamDef, check_name, eval_text, evaluate_table};
use planora_feature::georef::{edge_gone, face_gone};
use planora_feature::*;
use planora_i18n::{english, german, with_lang};
use planora_kernel::BoolOp;
use planora_math::{DQuat, DVec3};

fn looks_like_key(s: &str) -> bool {
    s.contains('-')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && s.starts_with(|c: char| c.is_ascii_lowercase())
}

fn assert_english(what: &str, text: &str) {
    assert!(!text.is_empty(), "{what}: leer");
    assert!(
        !text.chars().any(|c| "äöüßÄÖÜ„".contains(c)),
        "{what}: deutsche Zeichen in \"{text}\""
    );
    assert!(
        !looks_like_key(text),
        "{what}: sieht wie eine Id aus: {text}"
    );
}

fn boxed(d: &mut Document, name: &str, size: f64) -> FeatureId {
    d.add_feature(
        name,
        FeatureKind::Box {
            size: DVec3::splat(size),
            origin: DVec3::ZERO,
        },
    )
    .unwrap()
}

fn body(feature: FeatureId) -> BodyRef {
    BodyRef { feature, index: 0 }
}

/// Alle Texte, die die Bibliothek in der jeweils geltenden Sprache liefert. Die Texte
/// entstehen beim Aufruf, also müssen auch die Fehler hier in der Sprache entstehen.
fn all_texts() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // Fehlertypen
    let errors = [
        FeatureError::UnknownFeature,
        FeatureError::ForwardReference,
        FeatureError::FaceNotFound {
            origin: 3,
            role: "top".into(),
        },
        FeatureError::EdgeNotFound {
            a: "a".into(),
            b: "b".into(),
        },
        FeatureError::UntaggedFace,
        FeatureError::UntaggedEdge,
        FeatureError::BodyNotFound {
            feature: 1,
            index: 2,
        },
        FeatureError::Kernel("x".into()),
        FeatureError::Sketch("x".into()),
        FeatureError::ImplausibleResult,
        FeatureError::ImplausibleFreeformResult,
        FeatureError::ImplausibleBlend,
        FeatureError::MeshBodyLocked,
        FeatureError::MeshMissing(4),
        FeatureError::StepShapeMissing(5),
        FeatureError::StepShape("x".into()),
        FeatureError::HasDependents(vec![1, 2]),
        FeatureError::InvalidInput("x".into()),
        FeatureError::OutOfRange("The box".into()),
        FeatureError::ParameterInUse {
            name: "p".into(),
            users: vec!["a".into(), "b".into()],
        },
    ];
    out.extend(errors.iter().map(|e| e.to_string()));

    // Konstanten, die zu Funktionen wurden
    out.extend([
        too_many_balloons(),
        mesh_locked(),
        follow_page_delete(),
        parts_name_taken(),
        parts_page_name_taken(),
        parts_sheet_exists(),
        parts_sheet_rename(),
        face_gone(),
        edge_gone(),
    ]);

    // Bezeichnungen
    for u in ParamUnit::ALL {
        out.push(u.label());
    }
    let sketch_free: Vec<FeatureKind> = vec![
        FeatureKind::Box {
            size: DVec3::ONE,
            origin: DVec3::ZERO,
        },
        FeatureKind::Cylinder {
            radius: 1.0,
            height: 1.0,
            origin: DVec3::ZERO,
        },
        FeatureKind::Sphere {
            radius: 1.0,
            origin: DVec3::ZERO,
        },
        FeatureKind::Cone {
            radius: 1.0,
            height: 1.0,
            origin: DVec3::ZERO,
        },
        FeatureKind::Move {
            body: body(FeatureId(1)),
            delta: DVec3::ZERO,
        },
        FeatureKind::Boolean {
            target: body(FeatureId(1)),
            tool: body(FeatureId(2)),
            op: BoolOp::Union,
        },
        FeatureKind::Extrude {
            sketch: FeatureId(1),
            profile: 0,
            height: 1.0,
        },
        FeatureKind::Revolve {
            sketch: FeatureId(1),
            profile: 0,
            axis: planora_sketch::CurveId::from_index(0),
        },
        FeatureKind::MeshBody {
            mesh: MeshId(1),
            offset: DVec3::ZERO,
            rotation: DQuat::IDENTITY,
            scale: 1.0,
        },
        FeatureKind::StepBody {
            shape: StepShapeId(1),
        },
    ];
    for k in &sketch_free {
        out.push(k.label());
        for key in field_keys(k) {
            out.push(field_label(k, key));
        }
    }
    let arts = [
        BlendArt::Rundung { radius: 1.0 },
        BlendArt::RundungVariabel { punkte: vec![] },
        BlendArt::FaseGleich { abstand: 1.0 },
        BlendArt::FaseZwei {
            abstand1: 1.0,
            abstand2: 1.0,
        },
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 45.0,
        },
    ];
    for a in &arts {
        out.push(a.label());
        out.push(a.kind_label());
    }
    for key in [
        FieldKey::Breite,
        FieldKey::Tiefe,
        FieldKey::Hoehe,
        FieldKey::Radius,
        FieldKey::X,
        FieldKey::Y,
        FieldKey::Z,
        FieldKey::Abstand,
        FieldKey::Abstand1,
        FieldKey::Abstand2,
        FieldKey::Winkel,
        FieldKey::Massstab,
    ] {
        out.push(field_label(&sketch_free[0], key));
    }

    // Formelfehler
    let no_names = |_: &str| None;
    let long = "1+".repeat(150);
    let deep = format!("{}1{}", "(".repeat(70), ")".repeat(70));
    for text in [
        "",
        "(",
        "1+",
        "1/0",
        "sqrt(-1)",
        "sqrt(1mm)",
        "2mm^1.5",
        "2^3mm",
        "1mm+1deg",
        "1mm+1mm*1mm",
        "1mm*1mm+1mm*1mm*1mm",
        "1e400",
        "#",
        "foo(1)",
        "max(1)",
        "sin(1,2)",
        "sin(1mm)",
        "nichtda",
        long.as_str(),
        deep.as_str(),
    ] {
        if let Err(e) = eval_text(text, ParamUnit::Number, &no_names) {
            out.push(e.0);
        }
    }
    for (text, want) in [("1mm", ParamUnit::Angle), ("1deg", ParamUnit::Length)] {
        if let Err(e) = eval_text(text, want, &no_names) {
            out.push(e.0);
        }
    }
    for name in ["", "1a", "sqrt", "pi", &"a".repeat(80)] {
        if let Err(e) = check_name(name) {
            out.push(e.0);
        }
    }
    let defs = |items: &[(&'static str, &'static str)]| {
        items
            .iter()
            .map(|(n, f)| ParamDef {
                name: n,
                formula: f,
                unit: ParamUnit::Length,
            })
            .collect::<Vec<_>>()
    };
    for items in [
        vec![("a", "1"), ("A", "2")],
        vec![("a", "a+1")],
        vec![("a", "b+1"), ("b", "a+1")],
        vec![("a", "zzz")],
        vec![("a", "1+")],
    ] {
        if let Err(e) = evaluate_table(&defs(&items)) {
            out.push(e.message);
        }
    }

    // Dokumentvorgänge
    let mut d = Document::new();
    let a = boxed(&mut d, "A", 4.0);
    let b = boxed(&mut d, "B", 4.0);
    let mut note = |r: Result<(), FeatureError>| {
        if let Err(e) = r {
            out.push(e.to_string());
        }
    };
    note(
        d.add_feature(
            "Big",
            FeatureKind::Box {
                size: DVec3::splat(1e12),
                origin: DVec3::ZERO,
            },
        )
        .map(|_| ()),
    );
    note(
        d.add_feature(
            "Cyl",
            FeatureKind::Cylinder {
                radius: 1e12,
                height: 1.0,
                origin: DVec3::ZERO,
            },
        )
        .map(|_| ()),
    );
    note(
        d.add_feature(
            "Sph",
            FeatureKind::Sphere {
                radius: 1e12,
                origin: DVec3::ZERO,
            },
        )
        .map(|_| ()),
    );
    note(
        d.add_feature(
            "Mv",
            FeatureKind::Move {
                body: body(a),
                delta: DVec3::splat(1e12),
            },
        )
        .map(|_| ()),
    );
    note(
        d.add_feature(
            "Self",
            FeatureKind::Boolean {
                target: body(a),
                tool: body(a),
                op: BoolOp::Union,
            },
        )
        .map(|_| ()),
    );
    note(
        d.add_feature(
            "Ext",
            FeatureKind::Extrude {
                sketch: a,
                profile: 0,
                height: 1.0,
            },
        )
        .map(|_| ()),
    );
    note(
        d.add_feature(
            "Blend",
            FeatureKind::Verrunden {
                body: body(a),
                edges: vec![],
                art: BlendArt::Rundung { radius: 1.0 },
                tangentenkette: false,
            },
        )
        .map(|_| ()),
    );
    note(d.rename(a, "").map(|_| ()));
    note(d.rename(FeatureId(99), "x").map(|_| ()));
    note(d.set_marker(99));
    let u = d
        .add_feature(
            "U",
            FeatureKind::Boolean {
                target: body(a),
                tool: body(b),
                op: BoolOp::Union,
            },
        )
        .unwrap();
    let _ = u;
    note(d.delete(a));
    for (name, formula) in [("", "1"), ("p", "1+"), ("p", "zzz"), ("a", "1")] {
        let r = d.add_parameter(name, formula, ParamUnit::Length, "");
        note(r.map(|_| ()));
    }
    let p = d.add_parameter("p", "5", ParamUnit::Length, "").unwrap();
    note(d.add_parameter("P", "5", ParamUnit::Length, "").map(|_| ()));
    note(
        d.add_parameter("q", &"1+".repeat(150), ParamUnit::Length, "")
            .map(|_| ()),
    );
    let long_comment = "c".repeat(300);
    note(
        d.add_parameter("r", "1", ParamUnit::Length, &long_comment)
            .map(|_| ()),
    );
    let c = d
        .add_feature_with_formulas(
            "C",
            FeatureKind::Box {
                size: DVec3::ONE,
                origin: DVec3::ZERO,
            },
            [(FieldKey::Breite, "p".to_string())].into_iter().collect(),
        )
        .unwrap();
    let _ = c;
    note(d.remove_parameter(p));
    note(d.remove_parameter(ParamId(999)));
    out
}

#[test]
fn all_messages_speak_english() {
    let en = with_lang(&english(), all_texts);
    let de = with_lang(&german(), all_texts);
    assert!(en.len() >= 90, "zu wenige Texte: {}", en.len());
    assert_eq!(en.len(), de.len(), "gleiche Fälle in beiden Sprachen");
    for t in &en {
        assert_english("Meldung", t);
    }
    // Gleich lauten nur Wörter, die in beiden Sprachen gleich sind.
    const SAME_IN_BOTH: [&str; 7] = [
        "Position x",
        "Position y",
        "Position z",
        "Radius",
        "Δx",
        "Δy",
        "Δz",
    ];
    for (a, b) in en.iter().zip(&de) {
        assert!(
            a != b || SAME_IN_BOTH.contains(&a.as_str()),
            "in beiden Sprachen gleich: {a}"
        );
    }
}
