//! Dateiformat 3: Netz-Körper (Golden-Datei), kleinste passende Version, alte Dateien,
//! feindliche Netzdaten (Teilprojekt 9, Task 11).

use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_feature::*;
use planora_math::{DQuat, DVec3};
use planora_project::{
    FORMAT_VERSION, ProjectError, base64, decode_version, encode_version, required_format,
};
use std::sync::Arc;

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> Arc<ExchangeMesh> {
    Arc::new(ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    })
}

/// Quader, zwei Netz-Körper mit demselben Netz (einer gedreht und vergrößert).
fn mesh_document() -> Document {
    let mut d = Document::new();
    d.add_feature(
        "Quader 1",
        FeatureKind::Box {
            size: DVec3::splat(10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let t = MeshTransform {
        offset: DVec3::new(20.0, 0.0, 0.0),
        rotation: DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2),
        scale: 2.0,
    };
    let a = d.add_mesh_body("Netz 1", tetra(), t).unwrap();
    let mesh = match &d.feature(a).unwrap().kind {
        FeatureKind::MeshBody { mesh, .. } => *mesh,
        other => panic!("{other:?}"),
    };
    d.add_feature(
        "Netz 2",
        FeatureKind::MeshBody {
            mesh,
            offset: DVec3::new(40.0, 0.0, 0.0),
            rotation: DQuat::IDENTITY,
            scale: 1.0,
        },
    )
    .unwrap();
    d
}

fn volumes(d: &Document) -> Vec<f64> {
    d.visible_meshes()
        .iter()
        .map(|v| v.mesh.mass(&v.transform).volume)
        .collect()
}

fn bad(text: &str) -> ProjectError {
    decode_version(text, "x.json").unwrap_err()
}

fn edited(good: &str, f: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut v: serde_json::Value = serde_json::from_str(good).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

fn daten(good: &str) -> Vec<u8> {
    let v: serde_json::Value = serde_json::from_str(good).unwrap();
    base64::decode(v["netze"][0]["daten"].as_str().unwrap()).unwrap()
}

fn with_bytes(good: &str, bytes: &[u8]) -> String {
    edited(good, |v| {
        v["netze"][0]["daten"] = serde_json::Value::String(base64::encode(bytes));
    })
}

/// Setzt `key` in der Art des ersten Netz-Körpers.
fn set_mesh_field(v: &mut serde_json::Value, key: &str, value: serde_json::Value) {
    let f = v["features"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["art"]["typ"] == "netz_koerper")
        .unwrap();
    f["art"][key] = value;
}

#[test]
fn the_golden_file_pins_format_3() {
    let d = mesh_document();
    assert!(d.features().iter().all(|f| f.status == FeatureStatus::Ok));
    let text = encode_version(&d.snapshot()).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/format3.json");
    if std::env::var("PLANORA_UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, &text).unwrap();
    }
    let golden = std::fs::read_to_string(&path)
        .expect("tests/data/format3.json fehlt – mit PLANORA_UPDATE_GOLDEN=1 erzeugen");
    assert_eq!(text, golden.trim_end_matches('\n'));
    assert!(golden.contains("\"format\": 3"));
    assert!(golden.contains("\"typ\": \"netz_koerper\""));
    assert_eq!(
        golden.matches("\"daten\"").count(),
        1,
        "ein Netz für zwei Netz-Körper"
    );
    let back =
        Document::from_snapshot(decode_version(&golden, "format3.json").unwrap(), 0.05).unwrap();
    back.check_invariants().unwrap();
    assert_eq!(volumes(&back), volumes(&d));
    assert_eq!(back.visible_bodies().len(), 1);
    assert_eq!(back.visible_meshes().len(), 2);
}

#[test]
fn the_smallest_fitting_version_is_written() {
    let mut d = Document::new();
    d.add_feature(
        "Q",
        FeatureKind::Box {
            size: DVec3::ONE,
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    assert_eq!(required_format(&d.snapshot()), 1);
    let mut m = mesh_document();
    assert_eq!(required_format(&m.snapshot()), 3);
    assert_eq!(FORMAT_VERSION, 5); // seit Teilprojekt 12 (Maße, Mittellinien)
    // Netz-Körper gelöscht: wieder Format 1, keine Netzdaten mehr
    let ids: Vec<FeatureId> = m
        .features()
        .iter()
        .filter(|f| matches!(f.kind, FeatureKind::MeshBody { .. }))
        .map(|f| f.id)
        .collect();
    for id in ids {
        m.delete(id).unwrap();
    }
    let text = encode_version(&m.snapshot()).unwrap();
    assert!(
        text.contains("\"format\": 1") && !text.contains("netze"),
        "{text}"
    );
}

#[test]
fn old_files_stay_readable_and_old_programs_report_too_new() {
    for name in ["format1.json", "format1_voll.json", "format2.json"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(name);
        let text = std::fs::read_to_string(&path).unwrap();
        let s = decode_version(&text, name).unwrap();
        assert!(s.meshes.is_empty(), "{name}");
        Document::from_snapshot(s, 0.05).unwrap();
    }
    let good = encode_version(&mesh_document().snapshot()).unwrap();
    assert!(
        good.contains("\"format\": 3"),
        "Programme mit Format ≤ 2 melden das über ihre Versionsprüfung als zu neu"
    );
    // Eine noch neuere Datei meldet dieses Programm als „zu neu“.
    assert!(matches!(
        bad(&good.replacen("\"format\": 3", "\"format\": 6", 1)),
        ProjectError::TooNew { found: 6, .. }
    ));
    // Format 1 oder 2 mit Netz-Körper: beschädigt
    for older in ["\"format\": 1", "\"format\": 2"] {
        let e = bad(&good.replacen("\"format\": 3", older, 1));
        assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
        assert!(e.to_string().contains("Dateiformat 3"), "{e}");
    }
}

#[test]
fn hostile_mesh_data_is_rejected_by_name() {
    let good = encode_version(&mesh_document().snapshot()).unwrap();
    let bytes = daten(&good);
    let mut more = bytes.clone();
    more[4..8].copy_from_slice(&1000u32.to_le_bytes());
    // 50 000 000 Punkte und Dreiecke im Kopf, Inhalt fehlt (8-GB-Behauptung)
    let mut claim = bytes.clone();
    claim[0..8].copy_from_slice(&[0x80, 0xF0, 0xFA, 0x02, 0x80, 0xF0, 0xFA, 0x02]);
    let mut nan = bytes.clone();
    nan[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
    let mut index = bytes.clone();
    let at = bytes.len() - 4;
    index[at..].copy_from_slice(&77u32.to_le_bytes());
    let named = [
        edited(&good, |v| {
            v["netze"][0]["daten"] = serde_json::json!("@@@@")
        }),
        edited(&good, |v| {
            v["netze"][0]["daten"] = serde_json::json!("QUJD Q")
        }),
        with_bytes(&good, &more),
        with_bytes(&good, &claim),
        with_bytes(&good, &nan),
        with_bytes(&good, &index),
        with_bytes(&good, &bytes[..bytes.len() - 4]),
        edited(&good, |v| {
            let n = v["netze"][0].clone();
            v["netze"].as_array_mut().unwrap().push(n);
        }),
        edited(&good, |v| v["netze"][0]["id"] = serde_json::json!(0)),
        edited(&good, |v| v["netze"] = serde_json::json!([])),
    ];
    for text in named {
        assert_ne!(text, good, "Änderung muss greifen");
        match bad(&text) {
            ProjectError::Corrupt { msg, .. } => assert!(msg.contains("Netz"), "{msg}"),
            other => panic!("{other:?}"),
        }
    }
    // Falscher Typ und riesige Zahl: beim Laden abgelehnt
    for text in [
        edited(&good, |v| v["netze"][0]["daten"] = serde_json::json!(5)),
        edited(&good, |v| {
            set_mesh_field(v, "versatz", serde_json::json!([1e300, 0.0, 0.0]));
        }),
    ] {
        assert!(matches!(bad(&text), ProjectError::Corrupt { .. }), "{text}");
    }
    // Geladen, aber beim Berechnen benannt gemeldet: Maßstab 0, Drehung nicht normiert
    for text in [
        edited(&good, |v| {
            set_mesh_field(v, "massstab", serde_json::json!(0.0))
        }),
        edited(&good, |v| {
            set_mesh_field(v, "drehung", serde_json::json!([0.0, 0.0, 0.0, 2.0]));
        }),
    ] {
        let d = Document::from_snapshot(decode_version(&text, "x.json").unwrap(), 0.05).unwrap();
        d.check_invariants().unwrap();
        assert!(
            d.features()
                .iter()
                .any(|f| matches!(f.status, FeatureStatus::Error(_)))
        );
    }
}
