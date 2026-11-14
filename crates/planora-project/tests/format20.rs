//! Dateiformat 20 (Teilprojekt Skizzen 10, Spec Entwurf 9): Nur ein Dokument mit 3D-Geometrie in
//! einer Skizze (3D-Punkt, Kurve mit Normale, 3D-Abhängigkeit, Haken „3D-Skizze“) wird mit
//! Format 20 geschrieben; alle anderen bleiben bytegleich. Der Rundlauf ist verlustfrei und
//! behält die Ids aller Einträge, auch der internen.

use planora_feature::*;
use planora_i18n::tr;
use planora_kernel::Curve;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec3, Frame, Tolerance};
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::helpers::include_curve;
use planora_sketch::{ConstraintKind as K, LineOrPoints, Sketch};
use std::sync::Arc;

fn format_of(text: &str) -> u64 {
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    v["format"].as_u64().unwrap()
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

fn new_sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default())
}

/// Speichert, lädt, prüft Format 20, gleiche Rohdaten und bytegleiches Wiederschreiben.
fn roundtrip(sk: Sketch) -> (Snapshot, String) {
    let snap = document(sk).snapshot();
    assert_eq!(required_format(&snap), 20);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 20);
    let back = decode_version(&text, "x.json").unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(sketch_of(&back).to_data(), sketch_of(&snap).to_data());
    assert_eq!(encode_version(&back).unwrap(), text);
    (back, text)
}

/// Die Rohdatei einer Skizze mit einem 3D-Punkt, geändert durch `f` an der Skizze.
fn edited(sk: Sketch, f: impl FnOnce(&mut serde_json::Value)) -> String {
    let text = encode_version(&document(sk).snapshot()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    f(&mut v["features"][0]["art"]["skizze"]);
    serde_json::to_string_pretty(&v).unwrap()
}

fn err_text(text: &str) -> String {
    decode_version(text, "x.json").unwrap_err().to_string()
}

#[test]
fn format_version_is_20() {
    assert_eq!(FORMAT_VERSION, 20);
}

/// Review Focus 2: Jede vorhandene Testdatei lädt und wird bytegleich wieder geschrieben, ohne
/// eines der neuen Felder.
#[test]
fn old_file_bytes_unchanged() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut seen = 0;
    for dir in [
        "data",
        "fixtures",
        "fixtures_format18",
        "fixtures_old_smooth",
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
            assert!(format_of(&back) < 20, "{name}");
            for field in [
                "\"drei_d\"",
                "\"normale\"",
                "\"az\"",
                "\"hoehen\"",
                "entlang_z",
                "fest3",
                "in_ebene",
                "ellipsenachsen3",
                "\"stueck\"",
                "\"einbezug\"",
                "\"skizzenkurve\"",
                "\"auf_flaeche\"",
                "\"schnittkurve\"",
            ] {
                assert!(!back.contains(field), "{name}: {field}");
            }
            seen += 1;
        }
    }
    assert!(seen >= 15, "{seen}");
}

#[test]
fn plain_sketch_stays_below_20() {
    let mut sk = new_sketch();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    let snap = document(sk).snapshot();
    assert!(required_format(&snap) < 20);
    let text = encode_version(&snap).unwrap();
    assert!(!text.contains("\"drei_d\""));
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let p = &v["features"][0]["art"]["skizze"]["punkte"][0];
    assert!(p.get("z").is_none(), "{p}");
}

#[test]
fn sketch_with_3d_point_writes_20_and_roundtrips() {
    let mut sk = new_sketch();
    let a = sk.add_point3(DVec3::new(1.0, 2.0, 3.0)).unwrap();
    let b = sk.add_point(5.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    sk.add_constraint(K::Fixed3(a, 1.0, 2.0, 3.0)).unwrap();
    let (back, text) = roundtrip(sk);
    let s = sketch_of(&back);
    assert!(s.is_3d_point(a));
    assert!(!s.is_3d_point(b));
    assert_eq!(s.point_xyz(a).unwrap(), DVec3::new(1.0, 2.0, 3.0));
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let sk = &v["features"][0]["art"]["skizze"];
    assert!(sk["punkte"][0]["z"].is_u64(), "{}", sk["punkte"][0]);
    assert!(sk["punkte"][1].get("z").is_none());
    assert!(text.contains("\"fest3\""), "{text}");
    assert!(sk.get("drei_d").is_none());
}

/// Gelöscht zählt mit: Der Eintrag steht samt z in der Datei.
#[test]
fn deleted_3d_point_still_needs_20() {
    let mut sk = new_sketch();
    let a = sk.add_point3(DVec3::new(1.0, 2.0, 3.0)).unwrap();
    sk.remove_point(a).unwrap();
    roundtrip(sk);
}

#[test]
fn normal_and_along_z_roundtrip() {
    let mut sk = new_sketch();
    let n = DVec3::new(1.0, 0.0, 1.0).normalize();
    // Bogen in einer gekippten Ebene (InPlane), danach weitere Einträge: Ihre Ids bleiben.
    let m = sk.add_point3(DVec3::new(0.0, 0.0, 1.0)).unwrap();
    let s0 = sk.add_point3(DVec3::new(0.0, 2.0, 1.0)).unwrap();
    let e0 = sk
        .add_point3(DVec3::new(-2.0_f64.sqrt(), 0.0, 1.0 + 2.0_f64.sqrt()))
        .unwrap();
    let arc = sk.add_arc3(m, s0, e0, n).unwrap();
    let circle_center = sk.add_point3(DVec3::new(5.0, 0.0, 2.0)).unwrap();
    let circle = sk.add_circle3(circle_center, 1.5, n).unwrap();
    let ec = sk.add_point3(DVec3::new(-5.0, 0.0, 2.0)).unwrap();
    let ell = sk
        .add_ellipse3(ec, DVec3::new(0.0, 3.0, 0.0), 1.0, n)
        .unwrap();
    // Senkrechte Linie mit „entlang Z“, Abstand dahinter.
    let p = sk.add_point3(DVec3::new(8.0, 1.0, 0.0)).unwrap();
    let q = sk.add_point3(DVec3::new(8.0, 1.0, 4.0)).unwrap();
    let up = sk.add_line(p, q).unwrap();
    sk.add_constraint(K::AlongZ(LineOrPoints::Line(up)))
        .unwrap();
    let dist = sk.add_constraint(K::Distance(p, q, 4.0)).unwrap();
    let (back, text) = roundtrip(sk);
    let s = sketch_of(&back);
    assert!(s.is_3d_curve(arc) && s.is_3d_curve(circle) && s.is_3d_curve(ell));
    assert!(
        s.constraint(dist)
            .is_some_and(|d| d.kind == K::Distance(p, q, 4.0))
    );
    for name in ["\"normale\"", "\"az\"", "entlang_z", "in_ebene"] {
        assert!(text.contains(name), "{name} fehlt");
    }
}

/// Einbezogene Ellipse (interne `EllipseAxes3`) und Freiformkurve im Raum (Höhen der
/// Kontrollpunkte).
#[test]
fn included_ellipse_and_raised_fixed_spline_roundtrip() {
    let mut sk = new_sketch();
    let cf = Frame::from_origin_normal(
        DVec3::new(1.0, -2.0, 3.0),
        DVec3::new(1.0, 2.0, 2.0).normalize(),
    );
    let ell = Curve::Ellipse {
        frame: cf,
        major: 4.0,
        minor: 2.0,
    };
    let g = include_curve(&Frame::WORLD, &ell, 0.5, 2.0).unwrap();
    sk.add_included(&g).unwrap();
    let bend = Curve::Nurbs(Arc::new(
        NurbsCurve::new(
            2,
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            vec![
                DVec3::new(0.0, 0.0, 1.0),
                DVec3::new(2.0, 3.0, 2.0),
                DVec3::new(4.0, 0.0, 1.0),
            ],
            vec![1.0; 3],
        )
        .unwrap(),
    ));
    let g = include_curve(&Frame::WORLD, &bend, 0.0, 1.0).unwrap();
    sk.add_included(&g).unwrap();
    let (_, text) = roundtrip(sk);
    assert!(text.contains("ellipsenachsen3"), "{text}");
    assert!(text.contains("\"hoehen\""), "{text}");
}

#[test]
fn three_d_flag_alone_needs_20() {
    let mut sk = new_sketch();
    sk.set_three_d(true);
    let (back, text) = roundtrip(sk);
    assert!(sketch_of(&back).to_data().three_d);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v["features"][0]["art"]["skizze"]["drei_d"],
        serde_json::json!(true)
    );
}

#[test]
fn format_21_is_too_new() {
    let mut sk = new_sketch();
    sk.set_three_d(true);
    let text = encode_version(&document(sk).snapshot()).unwrap().replacen(
        "\"format\": 20",
        "\"format\": 21",
        1,
    );
    let e = decode_version(&text, "x.json").unwrap_err();
    assert!(matches!(e, ProjectError::TooNew { found: 21, .. }), "{e:?}");
}

/// Eine Datei mit kleinerer Formatnummer darf die neuen Felder nicht enthalten.
#[test]
fn format_19_with_3d_data_rejected() {
    let mut sk = new_sketch();
    sk.add_point3(DVec3::new(1.0, 2.0, 3.0)).unwrap();
    let text = encode_version(&document(sk).snapshot()).unwrap().replacen(
        "\"format\": 20",
        "\"format\": 19",
        1,
    );
    let e = err_text(&text);
    let want = tr!(
        "proj-since-format",
        what = tr!("proj-feat-3d-sketch"),
        format = "20"
    );
    assert!(e.contains(&want), "{e}");
}

#[test]
fn unknown_constraint_on_3d_rejected_on_load() {
    let mut sk = new_sketch();
    let a = sk.add_point3(DVec3::new(1.0, 0.0, 1.0)).unwrap();
    let b = sk.add_point3(DVec3::new(-1.0, 0.0, 1.0)).unwrap();
    let c = sk.add_point(0.0, -1.0).unwrap();
    let d = sk.add_point(0.0, 1.0).unwrap();
    let axis = sk.add_line(c, d).unwrap();
    let text = edited(sk, |s| {
        s["abhaengigkeiten"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "art": {"typ": "symmetrisch", "a": a.index(), "b": b.index(),
                        "achse": axis.index()},
                "zweig": 0, "gesteuert": false, "aktiv": true, "intern": false,
                "extra": null, "lebt": true
            }));
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-err-not-in-3d")), "{e}");
}

#[test]
fn along_z_on_2d_line_rejected_on_load() {
    let mut sk = new_sketch();
    sk.set_three_d(true);
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(0.0, 1.0).unwrap();
    let l = sk.add_line(a, b).unwrap();
    let text = edited(sk, |s| {
        s["abhaengigkeiten"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "art": {"typ": "entlang_z", "ziel": {"typ": "linie", "kurve": l.index()}},
                "zweig": 0, "gesteuert": false, "aktiv": true, "intern": false,
                "extra": null, "lebt": true
            }));
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-err-along-z-flat")), "{e}");
}

/// Skizze mit gekipptem Bogen, für die Prüfungen von Normale und `InPlane`.
fn tilted_arc_sketch() -> Sketch {
    let mut sk = new_sketch();
    let n = DVec3::new(1.0, 0.0, 1.0).normalize();
    let m = sk.add_point3(DVec3::new(0.0, 0.0, 1.0)).unwrap();
    let s0 = sk.add_point3(DVec3::new(0.0, 2.0, 1.0)).unwrap();
    let e0 = sk
        .add_point3(DVec3::new(-2.0_f64.sqrt(), 0.0, 1.0 + 2.0_f64.sqrt()))
        .unwrap();
    sk.add_arc3(m, s0, e0, n).unwrap();
    sk
}

fn in_plane_rows(s: &serde_json::Value) -> Vec<usize> {
    s["abhaengigkeiten"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, c)| c["art"]["typ"] == "in_ebene")
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn normal_without_in_plane_rejected() {
    let text = edited(tilted_arc_sketch(), |s| {
        let rows = in_plane_rows(s);
        assert_eq!(rows.len(), 1);
        s["abhaengigkeiten"][rows[0]]["lebt"] = serde_json::json!(false);
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-err-data-in-plane")), "{e}");
}

#[test]
fn second_live_in_plane_rejected() {
    let text = edited(tilted_arc_sketch(), |s| {
        let row = s["abhaengigkeiten"][in_plane_rows(s)[0]].clone();
        s["abhaengigkeiten"].as_array_mut().unwrap().push(row);
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-err-data-in-plane")), "{e}");
}

#[test]
fn live_in_plane_without_normal_rejected() {
    let text = edited(tilted_arc_sketch(), |s| {
        s["kurven"][0]
            .as_object_mut()
            .unwrap()
            .remove("normale")
            .unwrap();
        // Ohne Normale hat auch kein Punkt mehr eine Höhe; nur InPlane bleibt.
        for p in s["punkte"].as_array_mut().unwrap() {
            p.as_object_mut().unwrap().remove("z");
        }
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-err-data-in-plane")), "{e}");
}

#[test]
fn normal_on_2d_points_rejected() {
    let text = edited(tilted_arc_sketch(), |s| {
        s["punkte"][1].as_object_mut().unwrap().remove("z").unwrap();
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-err-data-curve-normal")), "{e}");
}

#[test]
fn z_on_text_point_rejected() {
    use planora_math::DVec2;
    use planora_sketch::text::{HAlign, TextParams, VAlign};
    use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
    let v = DVec2::new;
    let shape = Arc::new(TextShape {
        lines: vec![ShapedLine {
            glyphs: vec![Glyph {
                origin: v(0.0, 0.0),
                advance: 0.8,
                contours: vec![Contour {
                    start: v(0.0, 0.0),
                    segs: vec![
                        Seg::Line(v(0.6, 0.0)),
                        Seg::Line(v(0.6, 0.7)),
                        Seg::Line(v(0.0, 0.0)),
                    ],
                }],
            }],
            width: 0.8,
        }],
        ascender: 0.9,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    });
    let mut sk = new_sketch();
    let t = sk
        .add_text_box(
            v(0.0, 0.0),
            v(10.0, 5.0),
            TextParams {
                content: "A".into(),
                style: TextStyle {
                    font: "Sans".into(),
                    bold: false,
                    italic: false,
                    spacing: 0.0,
                },
                height: 2.0,
                flip_h: false,
                flip_v: false,
            },
            HAlign::Left,
            VAlign::Bottom,
            shape,
        )
        .unwrap();
    let glyph = sk.text(t).unwrap().points[0];
    sk.set_three_d(true);
    let text = edited(sk, |s| {
        let nv = s["werte"].as_array().unwrap().len();
        s["werte"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!(0.0));
        s["punkte"][glyph.index()]["z"] = serde_json::json!(nv);
    });
    let e = err_text(&text);
    assert!(
        e.contains(&tr!(
            "sketch-err-text-data",
            reason = tr!("sketch-err-text-data-3d")
        )),
        "{e}"
    );
}

/// Ein Text in 3D (Task 13): Rahmen und Buchstaben aus 3D-Punkten in der Ebene YZ bei x = 10.
fn text_3d_sketch() -> (Sketch, planora_sketch::text::TextId) {
    use planora_math::DVec2;
    use planora_sketch::text::{HAlign, TextParams, VAlign};
    use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
    let v = DVec2::new;
    let shape = Arc::new(TextShape {
        lines: vec![ShapedLine {
            glyphs: vec![Glyph {
                origin: v(0.0, 0.0),
                advance: 0.8,
                contours: vec![Contour {
                    start: v(0.0, 0.0),
                    segs: vec![
                        Seg::Line(v(0.6, 0.0)),
                        Seg::Quad(v(0.7, 0.4), v(0.6, 0.7)),
                        Seg::Cubic(v(0.4, 0.8), v(0.2, 0.8), v(0.0, 0.7)),
                        Seg::Line(v(0.0, 0.0)),
                    ],
                }],
            }],
            width: 0.8,
        }],
        ascender: 0.9,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    });
    let mut sk = new_sketch();
    let t = sk
        .add_text_box3(
            [DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 40.0, 10.0)],
            DVec3::X,
            TextParams {
                content: "A".into(),
                style: TextStyle {
                    font: "Sans".into(),
                    bold: false,
                    italic: false,
                    spacing: 0.0,
                },
                height: 2.0,
                flip_h: false,
                flip_v: false,
            },
            HAlign::Left,
            VAlign::Bottom,
            shape,
        )
        .unwrap();
    (sk, t)
}

#[test]
fn text_3d_roundtrips_and_needs_20() {
    let (sk, t) = text_3d_sketch();
    assert!(sk.has_3d_entry());
    let (back, text) = roundtrip(sk);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let texte = &v["features"][0]["art"]["skizze"]["texte"];
    assert_eq!(texte[0]["normale"], serde_json::json!([1.0, 0.0, 0.0]));
    // Die Ecken hält je ein internes InPlane an den Rahmenlinien 0 bis 2, mit gleichen Ids.
    let skizze = &v["features"][0]["art"]["skizze"];
    let rows = in_plane_rows(skizze);
    assert_eq!(rows.len(), 3);
    let linien = &texte[0]["lage"]["rahmen"]["linien"];
    for (k, &row) in rows.iter().enumerate() {
        assert_eq!(skizze["abhaengigkeiten"][row]["art"]["kurve"], linien[k]);
    }
    let d = sketch_of(&back).text(t).unwrap();
    assert_eq!(d.normal, Some(DVec3::X));
    assert!(d.points.iter().all(|&p| sketch_of(&back).is_3d_point(p)));
}

#[test]
fn text_3d_out_of_plane_or_without_normal_rejected() {
    let (sk, t) = text_3d_sketch();
    let glyph = sk.text(t).unwrap().points[1];
    let reason = |key: &str| tr!("sketch-err-text-data", reason = planora_i18n::t(key));

    // Ohne Normale ist es ein Text in der Skizzenebene: Die Ebenenbindungen am Rahmen gehören
    // zu nichts, und z an den Buchstaben wäre falsch.
    let text = edited(sk.clone(), |s| {
        s["texte"][0]
            .as_object_mut()
            .unwrap()
            .remove("normale")
            .unwrap();
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-err-data-in-plane")), "{e}");
    let text = edited(sk.clone(), |s| {
        s["texte"][0]
            .as_object_mut()
            .unwrap()
            .remove("normale")
            .unwrap();
        for row in in_plane_rows(s) {
            s["abhaengigkeiten"][row]["lebt"] = serde_json::json!(false);
        }
    });
    let e = err_text(&text);
    assert!(e.contains(&reason("sketch-err-text-data-3d")), "{e}");

    // Ein Buchstabe außerhalb der Ebene.
    let text = edited(sk.clone(), |s| {
        let xi = s["punkte"][glyph.index()]["x"].as_u64().unwrap() as usize;
        s["werte"][xi] = serde_json::json!(11.0);
    });
    let e = err_text(&text);
    assert!(e.contains(&reason("sketch-err-text-data-plane")), "{e}");

    // Eine Normale, die nicht normiert ist.
    let text = edited(sk, |s| {
        s["texte"][0]["normale"] = serde_json::json!([2.0, 0.0, 0.0]);
    });
    let e = err_text(&text);
    assert!(e.contains(&reason("sketch-err-text-data-plane")), "{e}");
}

#[test]
fn text_3d_frame_without_in_plane_rejected() {
    let (sk, _) = text_3d_sketch();
    // Ein fehlendes InPlane an einer Rahmenlinie ließe die Ecken aus der Textebene.
    let text = edited(sk.clone(), |s| {
        let rows = in_plane_rows(s);
        s["abhaengigkeiten"][rows[1]]["lebt"] = serde_json::json!(false);
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-err-data-in-plane")), "{e}");
    // An Linie 3 ist es keins der Rahmenlinien 0 bis 2.
    let text = edited(sk, |s| {
        let rows = in_plane_rows(s);
        let linie3 = s["texte"][0]["lage"]["rahmen"]["linien"][3].clone();
        s["abhaengigkeiten"][rows[2]]["art"]["kurve"] = linie3;
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-err-data-in-plane")), "{e}");
}

// --- Task 15: Herkunft mit Stücken und neue Herkunftsarten ---

/// Schlüssel der Kopien, die erst Format 20 kennt.
const COPY_KEYS_20: [&str; 6] = [
    "\"stueck\"",
    "\"punkt\"",
    "\"einbezug\"",
    "\"skizzenkurve\"",
    "\"auf_flaeche\"",
    "\"schnittkurve\"",
];

fn box_document() -> (Document, FeatureId) {
    let mut d = Document::new();
    let q = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(10.0, 20.0, 5.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    (d, q)
}

fn box_solid(d: &Document, q: FeatureId) -> planora_kernel::Solid {
    d.body(BodyRef {
        feature: q,
        index: 0,
    })
    .unwrap()
    .clone()
}

fn face_up(solid: &planora_kernel::Solid) -> FaceRef {
    let (id, _) = solid
        .faces
        .iter()
        .find(|(f, _)| planora_sketch::plane_of_face(solid, *f).is_some_and(|fr| fr.z.z > 0.5))
        .unwrap();
    FaceRef::from_face(solid, id, &Tolerance::default()).unwrap()
}

fn copies_in(s: &Snapshot, id: FeatureId) -> Vec<EdgeCopy> {
    let f = s.features.iter().find(|f| f.id == id).unwrap();
    let FeatureKind::Sketch(sf) = &f.kind else {
        panic!("keine Skizze");
    };
    sf.copies.clone()
}

#[test]
fn format20_piece_and_new_sources_roundtrip() {
    let (mut d, q) = box_document();
    let solid = box_solid(&d, q);
    let (edge_id, _) = solid.edges.iter().next().unwrap();
    let edge = EdgeRef::from_edge(&solid, edge_id, &Tolerance::default()).unwrap();
    let top = face_up(&solid);
    let plane = d
        .add_feature(
            "Ebene1",
            FeatureKind::Plane(planes::PlaneFeature {
                def: planes::PlaneDef::Offset {
                    base: planes::PlaneBase::Standard(StandardPlane::XY),
                    distance: 5.0,
                },
            }),
        )
        .unwrap();
    let mut sk1 = Sketch::new(StandardPlane::XZ.frame(), "Skizze1", Tolerance::default());
    let a = sk1.add_point(0.0, 0.0).unwrap();
    let b = sk1.add_point(10.0, 10.0).unwrap();
    let src = sk1.add_line(a, b).unwrap();
    let s1 = d
        .add_feature(
            "Skizze1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk1,
                plane: SketchPlane::Standard(StandardPlane::XZ),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let sketch_curve = IncludeRef::SketchCurve {
        sketch: s1,
        curve: src,
    };
    let curve_sources = vec![
        CopySource::Include(IncludeRef::Edge(edge.clone())),
        CopySource::Include(IncludeRef::FaceBoundary(top.clone())),
        CopySource::Include(IncludeRef::Plane(PlaneId::Feature(plane))),
        CopySource::Include(IncludeRef::Plane(PlaneId::Standard(StandardPlane::YZ))),
        CopySource::Include(IncludeRef::Axis(planes::StdAxis::Y)),
        CopySource::Include(sketch_curve.clone()),
        CopySource::SketchCurve {
            sketch: s1,
            curve: src,
        },
        CopySource::OnSurface(SurfaceProjection {
            sources: vec![sketch_curve.clone()],
            targets: vec![IncludeRef::FaceBoundary(top.clone())],
        }),
        CopySource::Intersection(IntersectionSpec {
            sources: vec![sketch_curve.clone()],
            targets: vec![IncludeRef::Plane(PlaneId::Standard(StandardPlane::XY))],
        }),
    ];
    let point_sources = vec![
        CopySource::Include(IncludeRef::Vertex(VertexRef {
            edge: edge.clone(),
            end: true,
        })),
        CopySource::Include(IncludeRef::ConstructionPoint(FeatureId(99))),
        CopySource::Include(IncludeRef::Origin),
        CopySource::SketchCurve {
            sketch: s1,
            curve: src,
        },
    ];
    let mut sk = Sketch::new(Frame::WORLD, "Skizze2", Tolerance::default());
    let mut copies = Vec::new();
    for (k, source) in curve_sources.into_iter().enumerate() {
        let p = sk.add_point(k as f64, 40.0).unwrap();
        let q = sk.add_point(k as f64, 50.0).unwrap();
        let c = sk.add_line(p, q).unwrap();
        copies.push(EdgeCopy {
            target: CopyTarget::Curve(c),
            source,
            // Stückindizes über 0 (auch einer, der nicht passt: die Warnung zählt nicht zur Datei).
            piece: (k % 3) as u32,
        });
    }
    for (k, source) in point_sources.into_iter().enumerate() {
        let ids = sk
            .add_included(&planora_sketch::helpers::Included::Point(DVec3::new(
                k as f64, 60.0, 0.0,
            )))
            .unwrap();
        copies.push(EdgeCopy {
            target: CopyTarget::Point(ids.points[0]),
            source,
            piece: k as u32,
        });
    }
    let s2 = d
        .add_feature(
            "Skizze2",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies,
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let snap = d.snapshot();
    assert_eq!(required_format(&snap), 20);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 20);
    for key in COPY_KEYS_20 {
        assert!(text.contains(key), "{key}");
    }
    let back = decode_version(&text, "x.json").unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(copies_in(&back, s2), copies_in(&snap, s2));
    assert_eq!(copies_in(&back, s2).len(), 13);
    assert_eq!(encode_version(&back).unwrap(), text);
    // Laden rechnet, ohne zu scheitern: Verlorene Bezüge sind Warnungen.
    let doc = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(doc.feature(s2).unwrap().status, FeatureStatus::Ok);
}

/// Alte Arten (Kante, Schnitt, Silhouette) mit Stück 0 bleiben unter 20 und bytegleich, ohne die
/// neuen Schlüssel.
#[test]
fn old_copy_sources_bytes_unchanged() {
    let (d0, q) = box_document();
    let solid = box_solid(&d0, q);
    let top = face_up(&solid);
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let (edge_id, edge_curve) = solid
        .edges
        .iter()
        .find_map(|(id, _)| sk.add_projected_edge(&solid, id).ok().map(|c| (id, c)))
        .unwrap();
    let mut copies = vec![EdgeCopy::new(
        edge_curve,
        CopySource::Edge(EdgeRef::from_edge(&solid, edge_id, &Tolerance::default()).unwrap()),
    )];
    for (k, source) in [
        CopySource::Section {
            face: top.clone(),
            index: 0,
        },
        CopySource::Silhouette {
            face: top.clone(),
            index: 0,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let p = sk.add_point(k as f64, 40.0).unwrap();
        let q = sk.add_point(k as f64, 50.0).unwrap();
        copies.push(EdgeCopy::new(sk.add_line(p, q).unwrap(), source));
    }
    for n in 1..=copies.len() {
        let mut d = d0.clone();
        d.add_feature(
            "Skizze1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk.clone(),
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: copies[..n].to_vec(),
                formulas: Default::default(),
            }),
        )
        .unwrap();
        let snap = d.snapshot();
        assert!(required_format(&snap) < 20, "{n}");
        let text = encode_version(&snap).unwrap();
        assert!(format_of(&text) < 20);
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let kopien = v["features"][1]["art"]["kopien"].to_string();
        assert!(kopien.contains("\"kurve\""), "{kopien}");
        for key in COPY_KEYS_20 {
            assert!(!kopien.contains(key), "{n}: {key}");
        }
        let back = decode_version(&text, "x.json").unwrap();
        assert_eq!(encode_version(&back).unwrap(), text, "{n}");
    }
}

/// Eine Datei mit kleinerer Formatnummer darf die neuen Herkunftsarten nicht enthalten; eine
/// Kopie braucht genau ein Ziel.
#[test]
fn new_copy_fields_need_format_20_and_one_target() {
    let (mut d, _) = box_document();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let ids = sk
        .add_included(&planora_sketch::helpers::Included::Point(DVec3::ZERO))
        .unwrap();
    d.add_feature(
        "Skizze1",
        FeatureKind::Sketch(SketchFeature {
            sketch: sk,
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![EdgeCopy {
                target: CopyTarget::Point(ids.points[0]),
                source: CopySource::Include(IncludeRef::Origin),
                piece: 0,
            }],
            formulas: Default::default(),
        }),
    )
    .unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    assert_eq!(format_of(&text), 20);
    let old = text.replacen("\"format\": 20", "\"format\": 19", 1);
    let want = tr!(
        "proj-since-format",
        what = tr!("proj-feat-3d-sketch"),
        format = "20"
    );
    let e = err_text(&old);
    assert!(e.contains(&want), "{e}");

    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["features"][1]["art"]["kopien"][0]["kurve"] = serde_json::json!(0);
    let both = serde_json::to_string_pretty(&v).unwrap();
    let e = err_text(&both);
    assert!(e.contains(&tr!("proj-copy-source-count")), "{e}");
}
