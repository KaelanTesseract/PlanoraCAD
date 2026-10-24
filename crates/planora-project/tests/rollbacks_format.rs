//! Dateiformat 11: Rückrollstellen je Körper (Verlaufsleiste). Format 11 nur mit Stellen, Dateien
//! ohne Stellen Byte für Byte wie bisher (Format 10 und älter), Rundreise mit gleichen Zuständen,
//! feindliche Stellen mit deutscher Meldung.

use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::DVec3;
use planora_project::{
    FORMAT_VERSION, ProjectError, decode_version, encode_version, required_format,
};
use serde_json::{Value, json};

// ---- Bausteine ----------------------------------------------------------------------------

fn body(feature: FeatureId) -> BodyRef {
    BodyRef { feature, index: 0 }
}

fn moved(d: &mut Document, name: &str, of: FeatureId, delta: DVec3) -> FeatureId {
    d.add_feature(
        name,
        FeatureKind::Move {
            body: body(of),
            delta,
        },
    )
    .unwrap()
}

/// Quader, Verschieben, Zylinder, Verschieben, Vereinigen. Liefert `[a, ma, b, mb, u]`.
fn union_model() -> (Document, [FeatureId; 5]) {
    let mut d = Document::new();
    let a = d
        .add_feature(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::splat(4.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let ma = moved(&mut d, "Verschieben 1", a, DVec3::new(1.0, 0.0, 0.0));
    let b = d
        .add_feature(
            "Zylinder 1",
            FeatureKind::Cylinder {
                radius: 1.0,
                height: 6.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let mb = moved(&mut d, "Verschieben 2", b, DVec3::new(0.0, 1.0, 0.0));
    let u = d
        .add_feature(
            "Vereinigen 1",
            FeatureKind::Boolean {
                target: body(ma),
                tool: body(mb),
                op: BoolOp::Union,
            },
        )
        .unwrap();
    (d, [a, ma, b, mb, u])
}

fn statuses(d: &Document) -> Vec<(FeatureId, FeatureStatus)> {
    d.features()
        .iter()
        .map(|f| (f.id, f.status.clone()))
        .collect()
}

fn bad(text: &str) -> ProjectError {
    decode_version(text, "x.json").unwrap_err()
}

/// Laden wie in der Anwendung: Datei lesen, dann `Document::from_snapshot`.
fn load(text: &str) -> Result<Document, ProjectError> {
    let s = decode_version(text, "x.json")?;
    Document::from_snapshot(s, 0.05).map_err(|e| ProjectError::Invalid(e.to_string()))
}

fn edited(text: &str, f: impl FnOnce(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(text).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

fn fixture(sub: &str, name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(sub)
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

/// Gültige Format-11-Datei: Stelle auf „Verschieben 1“ im Verlauf von „Vereinigen 1“.
fn good() -> (String, Document, [FeatureId; 5]) {
    let (mut d, ids) = union_model();
    d.set_rollback(ids[4], ids[1]).unwrap();
    (encode_version(&d.snapshot()).unwrap(), d, ids)
}

// ---- Tests --------------------------------------------------------------------------------

#[test]
fn format_version_is_11() {
    assert_eq!(FORMAT_VERSION, 11);
}

#[test]
fn file_without_rollbacks_is_byte_identical_to_format_10() {
    // Eingefrorene Format-10-Datei: lesen, bauen, schreiben → dieselben Bytes, weiter Format 10.
    let text = fixture("step_fixtures", "format10_step.json");
    let s = decode_version(&text, "format10_step.json").unwrap();
    assert!(s.rollbacks.is_empty());
    assert_eq!(required_format(&s), 10);
    let mut d = Document::from_snapshot(s, 0.05).unwrap();
    let again = encode_version(&d.snapshot()).unwrap();
    assert_eq!(again.replace("\r\n", "\n"), text.replace("\r\n", "\n"));
    assert!(!again.contains("rueckrollstellen"));

    // Stelle setzen → Format 11; Stelle entfernen → wieder genau die alten Bytes.
    let tip = d.body_tips()[0];
    let step = d.chain(tip)[0];
    d.set_rollback(tip, step).unwrap();
    let with = encode_version(&d.snapshot()).unwrap();
    assert!(with.contains("\"format\": 11"), "{with}");
    assert!(with.contains("\"rueckrollstellen\""), "{with}");
    d.clear_all_rollbacks();
    let back = encode_version(&d.snapshot()).unwrap();
    assert_eq!(back.replace("\r\n", "\n"), text.replace("\r\n", "\n"));

    // Alle älteren eingefrorenen Dateien: kein neues Feld, Format unverändert.
    for (sub, name) in [
        ("data", "format1.json"),
        ("data", "format1_voll.json"),
        ("data", "format2.json"),
        ("data", "format3.json"),
        ("fixtures", "format7_parts.json"),
    ] {
        let text = fixture(sub, name);
        let s = decode_version(&text, name).unwrap();
        assert!(required_format(&s) < 11, "{name}");
        let again = encode_version(&s).unwrap();
        assert!(!again.contains("rueckrollstellen"), "{name}");
        assert_eq!(
            again.replace("\r\n", "\n"),
            text.replace("\r\n", "\n"),
            "{name}"
        );
    }

    // Ein Dokument ohne Stellen schreibt die Formatnummer wie vorher (hier 1).
    let (d, _) = union_model();
    assert_eq!(required_format(&d.snapshot()), 1);
}

#[test]
fn file_with_rollbacks_roundtrips_as_format_11() {
    let (text, d, [_a, ma, b, mb, u]) = good();
    assert_eq!(required_format(&d.snapshot()), 11);
    assert!(text.contains("\"format\": 11"));
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["rueckrollstellen"], json!([ma.0]));

    let loaded = load(&text).unwrap();
    assert_eq!(loaded.rollbacks(), &[ma]);
    assert_eq!(statuses(&loaded), statuses(&d));
    assert!(!loaded.is_active(ma) && !loaded.is_active(u) && loaded.is_active(b));
    // Wieder geschrieben: dieselben Bytes.
    assert_eq!(encode_version(&loaded.snapshot()).unwrap(), text);

    // Zwei Stellen in beiden Eingangsketten des Vereinigens.
    let mut d2 = loaded;
    d2.set_rollback(u, mb).unwrap();
    let text2 = encode_version(&d2.snapshot()).unwrap();
    let loaded2 = load(&text2).unwrap();
    assert_eq!(loaded2.rollbacks(), &[ma, mb]);
    assert_eq!(statuses(&loaded2), statuses(&d2));
}

#[test]
fn format_11_with_unknown_step_id_is_rejected() {
    let (text, _, _) = good();
    let t = edited(&text, |v| v["rueckrollstellen"] = json!([99]));
    let e = bad(&t);
    assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
    let m = e.to_string();
    assert!(
        m.contains("Rückrollstelle verweist auf einen unbekannten Schritt (Kennung 99)"),
        "{m}"
    );

    // Keine Liste von Zahlen: beschädigt.
    let t = edited(&text, |v| v["rueckrollstellen"] = json!("ganz anders"));
    assert!(matches!(bad(&t), ProjectError::Corrupt { .. }));

    // Doppelt oder zwei Stellen im Verlauf desselben Körpers: Laden lehnt ab, nichts wird
    // still bereinigt.
    let (_, _, [a, ma, ..]) = good();
    let t = edited(&text, |v| v["rueckrollstellen"] = json!([ma.0, ma.0]));
    let m = load(&t).unwrap_err().to_string();
    assert!(m.contains("doppelt"), "{m}");
    let t = edited(&text, |v| v["rueckrollstellen"] = json!([a.0, ma.0]));
    let m = load(&t).unwrap_err().to_string();
    assert!(
        m.contains("„Quader 1“ und „Verschieben 1“ liegen im Verlauf desselben Körpers"),
        "{m}"
    );

    // Stellen in einer Datei mit Format 10: beschädigt.
    let t = text.replacen("\"format\": 11", "\"format\": 10", 1);
    let m = bad(&t).to_string();
    assert!(
        m.contains("Rückrollstellen gibt es erst ab Dateiformat 11"),
        "{m}"
    );
}

#[test]
fn older_program_limit() {
    let (text, _, _) = good();
    let e = bad(&text.replacen("\"format\": 11", "\"format\": 12", 1));
    assert!(matches!(e, ProjectError::TooNew { found: 12, .. }), "{e:?}");
    let e = bad("{\"format\": 12, \"rueckrollstellen\": \"ganz anders\"}");
    assert!(matches!(e, ProjectError::TooNew { found: 12, .. }), "{e:?}");
    // Format 11 selbst ist nicht zu neu.
    assert!(decode_version(&text, "x.json").is_ok());
}
