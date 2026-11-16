//! Dateiformat 21 (Teilprojekt Skizzen 11a, Task 7): Nur ein Dokument mit Krümmungskamm,
//! Tangente am Ende, Gegenwinkel oder Glatt am Ellipsenbogen wird mit Format 21 geschrieben;
//! alle anderen bleiben bytegleich. Der Rundlauf ist verlustfrei, auch für gelöschte Einträge.

use planora_feature::*;
use planora_i18n::tr;
use planora_math::{DVec2, Frame, Tolerance};
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use planora_sketch::comb::CombSettings;
use planora_sketch::{ConstraintId, ConstraintKind as K, CurveId, PointId, Sketch};

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

fn pt(s: &mut Sketch, x: f64, y: f64) -> PointId {
    s.add_point(x, y).unwrap()
}

/// Speichert, lädt, prüft Format 21, gleiche Rohdaten und bytegleiches Wiederschreiben.
fn roundtrip(sk: Sketch) -> (Snapshot, String) {
    let snap = document(sk).snapshot();
    assert_eq!(required_format(&snap), 21);
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 21);
    let back = decode_version(&text, "x.json").unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(sketch_of(&back).to_data(), sketch_of(&snap).to_data());
    assert_eq!(encode_version(&back).unwrap(), text);
    (back, text)
}

/// Die Rohdatei, geändert durch `f` an der Skizze.
fn edited(sk: Sketch, f: impl FnOnce(&mut serde_json::Value)) -> String {
    let text = encode_version(&document(sk).snapshot()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    f(&mut v["features"][0]["art"]["skizze"]);
    // Die geänderte Datei gibt sich als Format 21 aus, sonst käme die Formatprüfung zuerst.
    v["format"] = serde_json::json!(21);
    serde_json::to_string_pretty(&v).unwrap()
}

fn err_text(text: &str) -> String {
    decode_version(text, "x.json").unwrap_err().to_string()
}

/// Zwei Linien am gemeinsamen Punkt.
fn two_lines() -> (Sketch, CurveId, CurveId, PointId) {
    let mut s = new_sketch();
    let a = pt(&mut s, 0.0, 0.0);
    let b = pt(&mut s, 10.0, 0.0);
    let c = pt(&mut s, 14.0, 3.0);
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(b, c).unwrap();
    (s, l1, l2, b)
}

/// Ein Viertelkreis um den Ursprung.
fn arc_sketch() -> (Sketch, CurveId) {
    let mut s = new_sketch();
    let m = pt(&mut s, 0.0, 0.0);
    let st = pt(&mut s, 5.0, 0.0);
    let en = pt(&mut s, 0.0, 5.0);
    let arc = s.add_arc(m, st, en).unwrap();
    (s, arc)
}

/// Zwei Linien mit dem Winkelmaß mit Gegenwinkel.
fn angle_sketch() -> (Sketch, ConstraintId) {
    let mut s = new_sketch();
    let o = pt(&mut s, 0.0, 0.0);
    let a = pt(&mut s, 10.0, 0.0);
    let b = pt(&mut s, 5.0, 5.0 * 3f64.sqrt());
    let l1 = s.add_line(o, a).unwrap();
    let l2 = s.add_line(o, b).unwrap();
    let id = s
        .add_constraint_supplement(K::Angle(l1, l2, 0.0), true)
        .unwrap();
    (s, id)
}

fn with_format(text: String, from: u32, to: u32) -> String {
    text.replacen(
        &format!("\"format\": {from}"),
        &format!("\"format\": {to}"),
        1,
    )
}

#[test]
fn format_version_is_21() {
    assert_eq!(FORMAT_VERSION, 21);
}

/// Jede vorhandene Testdatei lädt und wird bytegleich wieder geschrieben, ohne eines der neuen
/// Felder.
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
            assert!(format_of(&back) < 21, "{name}");
            for field in ["\"kamm\"", "\"gegenwinkel\"", "tangente_ende"] {
                assert!(!back.contains(field), "{name}: {field}");
            }
            seen += 1;
        }
    }
    assert!(seen >= 15, "{seen}");
}

#[test]
fn plain_sketch_stays_below_21() {
    let mut s = new_sketch();
    let a = pt(&mut s, 0.0, 0.0);
    let b = pt(&mut s, 5.0, 0.0);
    s.add_line(a, b).unwrap();
    let m = pt(&mut s, 0.0, 10.0);
    let st = pt(&mut s, 5.0, 10.0);
    let en = pt(&mut s, 0.0, 15.0);
    s.add_arc(m, st, en).unwrap();
    let q: Vec<PointId> = [(20.0, 0.0), (23.0, 4.0), (27.0, 1.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    s.add_spline(&q).unwrap();
    let snap = document(s).snapshot();
    assert!(required_format(&snap) < 21);
    let text = encode_version(&snap).unwrap();
    for field in ["\"kamm\"", "\"gegenwinkel\"", "tangente_ende"] {
        assert!(!text.contains(field), "{field}");
    }
}

#[test]
fn midpoint_line_stays_below_21() {
    let mut s = new_sketch();
    let a = pt(&mut s, 0.0, 0.0);
    let b = pt(&mut s, 10.0, 0.0);
    let m = pt(&mut s, 5.0, 0.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Midpoint(m, l)).unwrap();
    assert!(required_format(&document(s).snapshot()) < 21);
}

/// Ein G2-Übergang aus Spline Grad 5 zwischen Linie und Bogen braucht ohne Ellipsenbogen kein 21.
#[test]
fn g2_blend_without_elliptic_arc_below_21() {
    let mut s = new_sketch();
    let l0 = pt(&mut s, 0.0, 0.0);
    let l1 = pt(&mut s, 10.0, 0.0);
    let line = s.add_line(l0, l1).unwrap();
    let m = pt(&mut s, 40.0, 0.0);
    let st = pt(&mut s, 40.0, 10.0);
    let en = pt(&mut s, 50.0, 0.0);
    let arc = s.add_arc(m, st, en).unwrap();
    let mid: Vec<PointId> = [(15.0, 5.0), (20.0, 6.0), (30.0, 6.0), (35.0, 5.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let sp = s
        .add_control_spline(&[l1, mid[0], mid[1], mid[2], mid[3], st], 5)
        .unwrap();
    s.add_constraint(K::Smooth(line, l1, sp, l1)).unwrap();
    s.add_constraint(K::Smooth(sp, st, arc, st)).unwrap();
    assert!(required_format(&document(s).snapshot()) < 21);
}

#[test]
fn comb_writes_21_and_roundtrips() {
    let (mut s, arc) = arc_sketch();
    let want = CombSettings {
        density: 30,
        scale: 2.5,
    };
    s.set_comb(arc, Some(want)).unwrap();
    let (back, text) = roundtrip(s);
    assert_eq!(sketch_of(&back).comb(arc), Some(want));
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let k = &v["features"][0]["art"]["skizze"]["kurven"][0]["kamm"];
    assert_eq!(k["dichte"], 30);
    assert_eq!(k["massstab"], 2.5);
}

/// Ein gelöschter Kurveneintrag mit Kamm steht in der Datei und braucht 21.
#[test]
fn deleted_curve_with_comb_needs_21() {
    let (mut s, arc) = arc_sketch();
    s.set_comb(arc, Some(CombSettings::default())).unwrap();
    s.remove_curve(arc).unwrap();
    roundtrip(s);
}

#[test]
fn end_tangent_writes_21_and_roundtrips() {
    let (mut s, l1, l2, b) = two_lines();
    let id = s.add_constraint(K::EndTangent(l1, b, l2, b)).unwrap();
    let (back, text) = roundtrip(s);
    assert_eq!(
        sketch_of(&back).constraint(id).unwrap().kind,
        K::EndTangent(l1, b, l2, b)
    );
    assert!(text.contains("tangente_ende"), "{text}");
}

#[test]
fn deleted_end_tangent_needs_21() {
    let (mut s, l1, l2, b) = two_lines();
    let id = s.add_constraint(K::EndTangent(l1, b, l2, b)).unwrap();
    s.remove_constraint(id).unwrap();
    roundtrip(s);
}

#[test]
fn supplement_writes_21_and_roundtrips() {
    let (s, id) = angle_sketch();
    let kind = s.constraint(id).unwrap().kind.clone();
    let measured = s.measure_constraint(id).unwrap();
    let (back, text) = roundtrip(s);
    let b = sketch_of(&back);
    assert!(b.constraint(id).unwrap().supplement());
    // Wert und gemessener Winkel springen beim Laden nicht.
    assert_eq!(b.constraint(id).unwrap().kind, kind);
    assert_eq!(
        b.measure_constraint(id).unwrap().to_bits(),
        measured.to_bits()
    );
    assert!(text.contains("\"gegenwinkel\": true"), "{text}");
}

#[test]
fn plain_angle_stays_below_21() {
    let mut s = new_sketch();
    let o = pt(&mut s, 0.0, 0.0);
    let a = pt(&mut s, 10.0, 0.0);
    let b = pt(&mut s, 5.0, 8.0);
    let l1 = s.add_line(o, a).unwrap();
    let l2 = s.add_line(o, b).unwrap();
    s.add_constraint(K::Angle(l1, l2, 1.0)).unwrap();
    let snap = document(s).snapshot();
    assert!(required_format(&snap) < 21);
    assert!(!encode_version(&snap).unwrap().contains("gegenwinkel"));
}

fn elliptic_smooth() -> Sketch {
    let mut s = new_sketch();
    let p: Vec<PointId> = [(0.0, 0.0), (8.0, 0.0), (0.0, 4.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let e = s
        .add_elliptic_arc(p[0], DVec2::new(8.0, 0.0), 4.0, p[1], p[2])
        .unwrap();
    let q: Vec<PointId> = [(-3.0, 5.5), (-7.0, 6.0), (-11.0, 4.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let sp = s.add_spline(&[p[2], q[0], q[1], q[2]]).unwrap();
    s.add_constraint(K::Smooth(e, p[2], sp, p[2])).unwrap();
    s
}

#[test]
fn smooth_on_elliptic_arc_writes_21() {
    let (_, text) = roundtrip(elliptic_smooth());
    assert!(text.contains("\"glatt\""), "{text}");
}

#[test]
fn format_20_with_comb_rejected() {
    let (mut s, arc) = arc_sketch();
    s.set_comb(arc, Some(CombSettings::default())).unwrap();
    let text = with_format(encode_version(&document(s).snapshot()).unwrap(), 21, 20);
    let want = tr!(
        "proj-since-format",
        what = tr!("proj-feat-sketch-check"),
        format = "21"
    );
    let e = err_text(&text);
    assert!(e.contains(&want), "{e}");
}

#[test]
fn format_20_with_supplement_or_end_tangent_rejected() {
    let (s, _) = angle_sketch();
    let text = with_format(encode_version(&document(s).snapshot()).unwrap(), 21, 20);
    assert!(err_text(&text).contains(&tr!("proj-feat-sketch-check")));
    let (mut s, l1, l2, b) = two_lines();
    s.add_constraint(K::EndTangent(l1, b, l2, b)).unwrap();
    let text = with_format(encode_version(&document(s).snapshot()).unwrap(), 21, 20);
    assert!(err_text(&text).contains(&tr!("proj-feat-sketch-check")));
}

#[test]
fn format_22_is_too_new() {
    let (mut s, arc) = arc_sketch();
    s.set_comb(arc, Some(CombSettings::default())).unwrap();
    let text = with_format(encode_version(&document(s).snapshot()).unwrap(), 21, 22);
    let e = decode_version(&text, "x.json").unwrap_err();
    assert!(matches!(e, ProjectError::TooNew { found: 22, .. }), "{e:?}");
}

/// Ein Kamm an einer Linie ist ein Lesefehler.
#[test]
fn comb_on_line_rejected_on_load() {
    let (s, ..) = two_lines();
    let text = edited(s, |s| {
        s["kurven"][0]["kamm"] = serde_json::json!({"dichte": 20, "massstab": 1.0});
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-err-comb-line")), "{e}");
}

/// Werte außerhalb der Bereiche sind ein Lesefehler.
#[test]
fn comb_out_of_range_rejected_on_load() {
    for (dichte, massstab) in [(1u32, 1.0), (101, 1.0), (20, 0.001), (20, 101.0), (20, 0.0)] {
        let (s, _) = arc_sketch();
        let text = edited(s, |s| {
            s["kurven"][0]["kamm"] = serde_json::json!({"dichte": dichte, "massstab": massstab});
        });
        let e = err_text(&text);
        assert!(
            e.contains(&tr!("sketch-err-comb-range")),
            "{dichte} {massstab}: {e}"
        );
    }
}

/// Ein Gegenwinkel an einer anderen Art als `Angle` ist ein Lesefehler.
#[test]
fn supplement_on_other_kind_rejected_on_load() {
    let (mut s, l1, l2, _) = two_lines();
    s.add_constraint(K::Parallel(l1, l2)).unwrap();
    let text = edited(s, |s| {
        s["abhaengigkeiten"][0]["gegenwinkel"] = serde_json::json!(true);
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-supplement-angle-only")), "{e}");
}
