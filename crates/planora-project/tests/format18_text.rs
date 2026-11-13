//! Dateiformat 18 (Teilprojekt Skizzen 8, Spec G): Nur ein Dokument mit einem Text in einer Skizze
//! wird mit Format 18 geschrieben, auch wenn der Text gelöscht ist; alle anderen bleiben
//! bytegleich. In der Datei stehen nie Schriftdaten, nur Familienname, Fett und Kursiv und die
//! erzeugten Kurven als normale Einträge.

use planora_feature::*;
use planora_i18n::tr;
use planora_math::{DVec2, Frame, Tolerance};
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::text::{HAlign, TextId, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
use planora_sketch::{CurveId, Sketch, TextRecord};
use std::sync::Arc;

fn format_of(text: &str) -> u64 {
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    v["format"].as_u64().unwrap()
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Je Zeichen ein Quadrat aus vier Strecken und ein Loch mit einer quadratischen und einer
/// kubischen Bézierkurve (ohne Schrift, ohne `planora-text`).
fn shape(content: &str) -> Arc<TextShape> {
    let glyphs: Vec<Glyph> = content
        .chars()
        .enumerate()
        .map(|(i, _)| Glyph {
            origin: v(i as f64 * 0.8, 0.0),
            advance: 0.8,
            contours: vec![
                Contour {
                    start: v(0.0, 0.0),
                    segs: vec![
                        Seg::Line(v(0.6, 0.0)),
                        Seg::Line(v(0.6, 0.7)),
                        Seg::Line(v(0.0, 0.7)),
                        Seg::Line(v(0.0, 0.0)),
                    ],
                },
                Contour {
                    start: v(0.2, 0.2),
                    segs: vec![
                        Seg::Line(v(0.4, 0.2)),
                        Seg::Quad(v(0.5, 0.35), v(0.4, 0.5)),
                        Seg::Cubic(v(0.35, 0.55), v(0.25, 0.55), v(0.2, 0.5)),
                        Seg::Line(v(0.2, 0.2)),
                    ],
                },
            ],
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

/// Eine Skizze mit einem Text im Rahmen; alle Werte abweichend von den Vorgaben.
fn text_sketch() -> (Sketch, TextId) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let content = "Ab\ncd";
    let t = sk
        .add_text_box(
            v(1.0, 2.0),
            v(41.0, 12.0),
            TextParams {
                content: content.into(),
                style: TextStyle {
                    font: "Gibt Es Nicht Sans".into(),
                    bold: true,
                    italic: true,
                    spacing: 37.5,
                },
                height: 3.25,
                flip_h: true,
                flip_v: true,
            },
            HAlign::Right,
            VAlign::Top,
            shape(content),
        )
        .unwrap();
    (sk, t)
}

fn document(sketch: Sketch) -> Document {
    let mut d = Document::new();
    d.add_feature(
        "Skizze1",
        FeatureKind::Sketch(SketchFeature {
            sketch,
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas: Default::default(),
        }),
    )
    .unwrap();
    d
}

fn sketch_of(s: &Snapshot) -> &Sketch {
    let FeatureKind::Sketch(sf) = &s.features[0].kind else {
        panic!("keine Skizze");
    };
    &sf.sketch
}

fn encoded() -> String {
    let (sk, _) = text_sketch();
    encode_version(&document(sk).snapshot()).unwrap()
}

/// Datei mit Text, geändert durch `f` am ersten Texteintrag.
fn edited(f: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut v: serde_json::Value = serde_json::from_str(&encoded()).unwrap();
    f(&mut v["features"][0]["art"]["skizze"]["texte"][0]);
    serde_json::to_string_pretty(&v).unwrap()
}

#[test]
fn format_version_is_18() {
    assert_eq!(FORMAT_VERSION, 19);
}

#[test]
fn old_files_stay_byte_identical() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut seen = 0;
    for dir in ["data", "fixtures", "step_fixtures"] {
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
            assert!(!back.contains("\"texte\""), "{name}");
            seen += 1;
        }
    }
    assert!(seen >= 13, "{seen}");
}

#[test]
fn text_needs_format_18() {
    let (sk, _) = text_sketch();
    let snap = document(sk).snapshot();
    assert_eq!(required_format(&snap), 18);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 18);
    assert!(text.contains("\"texte\""), "{text}");
    // Ohne Text bleibt die Skizze unter 18 und ohne Feld `texte`.
    let mut plain = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let a = plain.add_point(0.0, 0.0).unwrap();
    let b = plain.add_point(5.0, 0.0).unwrap();
    plain.add_line(a, b).unwrap();
    let snap = document(plain).snapshot();
    assert!(required_format(&snap) < 18);
    assert!(!encode_version(&snap).unwrap().contains("\"texte\""));
}

#[test]
fn deleted_text_still_needs_format_18() {
    let (mut sk, t) = text_sketch();
    sk.remove_text(t).unwrap();
    assert!(sk.text(t).is_none());
    let snap = document(sk).snapshot();
    assert_eq!(required_format(&snap), 18);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 18);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v["features"][0]["art"]["skizze"]["texte"][0]["lebt"],
        serde_json::json!(false)
    );
    // Der gelöschte Eintrag steht in der Datei und überlebt den Rundlauf.
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(sketch_of(&back).to_data(), sketch_of(&snap).to_data());
    assert_eq!(encode_version(&back).unwrap(), text);
}

#[test]
fn roundtrip_keeps_text_params_and_ids() {
    let (sk, t) = text_sketch();
    let snap = document(sk).snapshot();
    let text = encode_version(&snap).unwrap();
    let back = decode_version(&text, "x.json").unwrap();
    let (before, after) = (sketch_of(&snap), sketch_of(&back));
    assert_eq!(after.to_data(), before.to_data());
    let (a, b) = (before.text(t).unwrap(), after.text(t).unwrap());
    assert_eq!(b.content, "Ab\ncd");
    assert_eq!(b.style.font, "Gibt Es Nicht Sans");
    assert!(b.style.bold && b.style.italic);
    assert_eq!(b.style.spacing, 37.5);
    assert_eq!(b.height, 3.25);
    assert!(b.flip_h && b.flip_v);
    assert!(matches!(
        b.placement,
        TextPlacement::Box {
            h: HAlign::Right,
            v: VAlign::Top,
            ..
        }
    ));
    assert_eq!(b.placement, a.placement);
    assert_eq!(b.curves, a.curves, "gleiche Kurven-Ids");
    assert_eq!(b.points, a.points, "gleiche Punkt-Ids");
    // Die erzeugten Kurven stehen mit ihrer Lage beim Speichern in der Datei.
    for &c in &a.curves {
        assert_eq!(after.curve(c).unwrap().kind, before.curve(c).unwrap().kind);
    }
    for &p in &a.points {
        assert_eq!(after.point_xy(p).unwrap(), before.point_xy(p).unwrap());
    }
    assert!(
        !after.is_text_shaped(t),
        "geladen ist ein Text noch nicht gesetzt (Spec G4)"
    );
    assert_eq!(encode_version(&back).unwrap(), text);
    // Die Felder heißen wie in Spec G2, die Ausrichtungen sind Wörter.
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let e = &v["features"][0]["art"]["skizze"]["texte"][0];
    assert_eq!(e["inhalt"], "Ab\ncd");
    assert_eq!(e["schrift"], "Gibt Es Nicht Sans");
    assert_eq!(e["fett"], true);
    assert_eq!(e["kursiv"], true);
    assert_eq!(e["hoehe"], 3.25);
    assert_eq!(e["abstand"], 37.5);
    assert_eq!(e["umkehren_waagerecht"], true);
    assert_eq!(e["umkehren_senkrecht"], true);
    assert_eq!(e["lage"]["rahmen"]["waagerecht"], "rechts");
    assert_eq!(e["lage"]["rahmen"]["senkrecht"], "oben");
    assert_eq!(e["lebt"], true);
}

#[test]
fn path_text_and_alignment_words_roundtrip() {
    let (sk, _) = text_sketch();
    let mut d = sk.to_data();
    let a = d.points.len();
    let line = CurveId::from_index(d.curves.len());
    // Eine freie Linie als Pfad, dazu ein zweiter Text (ohne erzeugte Kurven) auf ihr.
    let mut s = Sketch::from_data(d.clone()).unwrap();
    let p = s.add_point(0.0, -10.0).unwrap();
    let q = s.add_point(30.0, -10.0).unwrap();
    assert_eq!(p.index(), a);
    assert_eq!(s.add_line(p, q).unwrap(), line);
    d = s.to_data();
    for (h, word) in [
        (HAlign::Left, "links"),
        (HAlign::Center, "mitte"),
        (HAlign::Right, "rechts"),
    ] {
        let mut d = d.clone();
        let mut rec: TextRecord = d.texts[0].clone();
        rec.curves.clear();
        rec.points.clear();
        rec.placement = TextPlacement::Path {
            path: line,
            above: true,
            fit: true,
            h,
        };
        d.texts.push(rec);
        let sk = Sketch::from_data(d.clone()).unwrap();
        let snap = document(sk).snapshot();
        let text = encode_version(&snap).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let lage = &v["features"][0]["art"]["skizze"]["texte"][1]["lage"]["pfad"];
        assert_eq!(lage["kurve"], line.index());
        assert_eq!(lage["oberhalb"], true);
        assert_eq!(lage["anpassen"], true);
        assert_eq!(lage["waagerecht"], word);
        let back = decode_version(&text, "x.json").unwrap();
        assert_eq!(sketch_of(&back).to_data(), sketch_of(&snap).to_data());
        assert_eq!(encode_version(&back).unwrap(), text);
    }
    for (va, word) in [
        (VAlign::Top, "oben"),
        (VAlign::Middle, "mitte"),
        (VAlign::Bottom, "unten"),
    ] {
        let mut d = d.clone();
        if let TextPlacement::Box { v, .. } = &mut d.texts[0].placement {
            *v = va;
        }
        let text = encode_version(&document(Sketch::from_data(d).unwrap()).snapshot()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let lage = &v["features"][0]["art"]["skizze"]["texte"][0]["lage"]["rahmen"];
        assert_eq!(lage["senkrecht"], word);
        let back = decode_version(&text, "x.json").unwrap();
        assert_eq!(encode_version(&back).unwrap(), text);
    }
}

#[test]
fn format_17_file_with_text_rejected() {
    let mut v: serde_json::Value = serde_json::from_str(&encoded()).unwrap();
    v["format"] = 17.into();
    let text = serde_json::to_string_pretty(&v).unwrap();
    let e = decode_version(&text, "x.json").unwrap_err();
    assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
    let want = tr!(
        "proj-since-format",
        what = tr!("proj-feat-text"),
        format = "18"
    );
    assert!(e.to_string().contains(&want), "{e}");
}

#[test]
fn format_20_is_too_new() {
    let text = encoded().replacen("\"format\": 18", "\"format\": 20", 1);
    let e = decode_version(&text, "x.json").unwrap_err();
    assert!(matches!(e, ProjectError::TooNew { found: 20, .. }), "{e:?}");
}

/// Verdirbt den ersten Texteintrag einer Datei.
type Spoil = Box<dyn FnOnce(&mut serde_json::Value)>;

#[test]
fn bad_texts_in_the_file_are_refused() {
    let cases: Vec<(&str, Spoil)> = vec![
        ("Höhe 0", Box::new(|t| t["hoehe"] = 0.0.into())),
        (
            "Höhe null",
            Box::new(|t| t["hoehe"] = serde_json::Value::Null),
        ),
        ("Abstand 1001", Box::new(|t| t["abstand"] = 1001.0.into())),
        (
            "501 Zeichen",
            Box::new(|t| t["inhalt"] = "x".repeat(501).into()),
        ),
        (
            "Rahmen mit 3 Linien",
            Box::new(|t| {
                t["lage"]["rahmen"]["linien"].as_array_mut().unwrap().pop();
            }),
        ),
        (
            "Rahmen mit 5 Ecken",
            Box::new(|t| {
                t["lage"]["rahmen"]["ecken"]
                    .as_array_mut()
                    .unwrap()
                    .push(0.into());
            }),
        ),
        (
            "unbekannte Ausrichtung",
            Box::new(|t| t["lage"]["rahmen"]["waagerecht"] = "quer".into()),
        ),
        (
            "Pfad gleich einer Textkurve",
            Box::new(|t| {
                let c = t["kurven"][0].clone();
                t["lage"] = serde_json::json!({"pfad": {
                    "kurve": c, "oberhalb": false, "anpassen": false, "waagerecht": "links"
                }});
            }),
        ),
        (
            "fehlende Kurve",
            Box::new(|t| {
                t["kurven"]
                    .as_array_mut()
                    .unwrap()
                    .push(1_000_000_000_000_000u64.into());
            }),
        ),
        (
            "fehlende Ecke",
            Box::new(|t| t["lage"]["rahmen"]["ecken"][2] = (u32::MAX as u64 + 3).into()),
        ),
        (
            "fehlender Pfad",
            Box::new(|t| {
                t["lage"] = serde_json::json!({"pfad": {
                    "kurve": 4_294_967_296u64, "oberhalb": false, "anpassen": false,
                    "waagerecht": "links"
                }});
            }),
        ),
        (
            "eine Kurve zweimal",
            Box::new(|t| {
                let c = t["kurven"][0].clone();
                t["kurven"].as_array_mut().unwrap().push(c);
            }),
        ),
    ];
    for (what, f) in cases {
        let text = edited(f);
        let e = decode_version(&text, "x.json");
        assert!(
            matches!(e, Err(ProjectError::Corrupt { .. })),
            "{what}: {e:?}"
        );
    }
}

/// Längste Folge von Zeichen, die in Base64 vorkommen dürfen.
fn longest_base64_run(text: &str) -> usize {
    let mut best = 0;
    let mut run = 0;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '+' || ch == '/' || ch == '=' {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}

#[test]
fn file_contains_no_font_bytes() {
    let text = encoded();
    assert!(!text.contains("glyf"), "keine Schrifttabellen");
    assert!(
        longest_base64_run(&text) <= 1024,
        "keine eingebetteten Daten: {}",
        longest_base64_run(&text)
    );
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let texts = v["features"][0]["art"]["skizze"]["texte"]
        .as_array()
        .unwrap();
    assert_eq!(texts.len(), 1);
    let keys = |o: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = o.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    let mut want = vec![
        "inhalt",
        "schrift",
        "fett",
        "kursiv",
        "hoehe",
        "abstand",
        "umkehren_waagerecht",
        "umkehren_senkrecht",
        "lage",
        "kurven",
        "punkte",
        "lebt",
    ];
    want.sort();
    assert_eq!(keys(&texts[0]), want);
    assert_eq!(keys(&texts[0]["lage"]), vec!["rahmen"]);
    assert_eq!(
        keys(&texts[0]["lage"]["rahmen"]),
        vec!["ecken", "linien", "senkrecht", "waagerecht"]
    );
    // Die erzeugten Kurven sind normale Einträge der Skizze (Linien, Kegelschnitte, Splines).
    let kurven = v["features"][0]["art"]["skizze"]["kurven"]
        .as_array()
        .unwrap();
    for c in texts[0]["kurven"].as_array().unwrap() {
        let art = &kurven[c.as_u64().unwrap() as usize]["art"]["typ"];
        assert!(
            ["linie", "kegelschnitt", "kontroll_spline"].contains(&art.as_str().unwrap()),
            "{art}"
        );
    }
}
