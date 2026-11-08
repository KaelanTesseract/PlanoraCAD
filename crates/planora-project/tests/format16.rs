//! Dateiformat 16 (Teilprojekt Skizzen 6, Task 7): Konstruktionsebenen, Skizzen auf
//! Konstruktionsebenen sowie Schnitt- und Silhouettenprojektionen. Nur ein Dokument mit einem
//! dieser Teile wird mit Format 16 geschrieben; alle anderen Dateien bleiben bytegleich.

use planora_feature::planes::{AxisRef, PlaneBase, PlaneDef, PlaneFeature, PointRef, StdAxis};
use planora_feature::*;
use planora_kernel::{Solid, Surface};
use planora_math::{DVec3, Frame, Tolerance};
use planora_project::{ProjectError, decode_version, encode_version, required_format};
use planora_sketch::{CurveId, PointId, Sketch};

fn load(text: &str) -> Result<Document, ProjectError> {
    let s = decode_version(text, "x.json")?;
    Document::from_snapshot(s, 0.05).map_err(|e| ProjectError::Invalid(e.to_string()))
}

fn format_of(text: &str) -> u64 {
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    v["format"].as_u64().unwrap()
}

fn plane(def: PlaneDef) -> FeatureKind {
    FeatureKind::Plane(PlaneFeature { def })
}

fn sketch_kind(sketch: Sketch, plane: SketchPlane, copies: Vec<EdgeCopy>) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch,
        plane,
        copies,
        formulas: Default::default(),
    })
}

fn add_box(d: &mut Document) -> FeatureId {
    d.add_feature(
        "Quader1",
        FeatureKind::Box {
            size: DVec3::new(40.0, 20.0, 10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap()
}

fn solid(d: &Document, f: FeatureId) -> &Solid {
    d.body(BodyRef {
        feature: f,
        index: 0,
    })
    .unwrap()
}

/// Ebene Fläche mit der äußeren Normale `n`.
fn face_with_normal(s: &Solid, n: DVec3) -> FaceRef {
    let (id, _) = s
        .faces
        .iter()
        .find(|(f, _)| planora_sketch::plane_of_face(s, *f).is_some_and(|fr| fr.z.dot(n) > 0.999))
        .expect("Fläche fehlt");
    FaceRef::from_face(s, id, &Tolerance::default()).unwrap()
}

/// Erste gerade Kante des Körpers.
fn some_edge(s: &Solid) -> EdgeRef {
    let (id, _) = s.edges.iter().next().unwrap();
    EdgeRef::from_edge(s, id, &Tolerance::default()).unwrap()
}

/// Skizze in XY mit einer Linie (für Achse und Punkt der Ebenen).
fn line_sketch() -> (Sketch, CurveId, PointId) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let a = sk.add_point(0.0, 5.0).unwrap();
    let b = sk.add_point(10.0, 5.0).unwrap();
    let line = sk.add_line(a, b).unwrap();
    (sk, line, b)
}

/// Alle Dateien unter den vorhandenen Format-Testdaten (`tests/data`, `tests/fixtures`).
fn old_files() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut out = Vec::new();
    for dir in ["data", "fixtures"] {
        let mut entries: Vec<_> = std::fs::read_dir(root.join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        entries.sort();
        for p in entries {
            out.push((
                p.display().to_string(),
                std::fs::read_to_string(&p).unwrap(),
            ));
        }
    }
    assert!(out.len() >= 12, "{}", out.len());
    out
}

// Skizzen 7: Die höchste Formatnummer prüft jetzt `format_17.rs`.

#[test]
fn old_files_roundtrip_bytewise() {
    for (name, text) in old_files() {
        let text = text.trim_end_matches('\n');
        let snap = decode_version(text, &name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let back = encode_version(&snap).unwrap();
        assert_eq!(format_of(&back), format_of(text), "{name}");
        assert!(format_of(&back) < 16, "{name}");
        assert_eq!(back, text, "{name}");
    }
}

/// Dokument mit je einer Ebene jeder Art (Versatz mit Formel) samt ihren Bezügen.
fn plane_document() -> (Document, Vec<(FeatureId, &'static str)>) {
    let mut d = Document::new();
    d.add_parameter("a", "3", ParamUnit::Length, "").unwrap();
    let quader = add_box(&mut d);
    let zyl = d
        .add_feature(
            "Zylinder1",
            FeatureKind::Cylinder {
                radius: 5.0,
                height: 10.0,
                origin: DVec3::new(100.0, 0.0, 0.0),
            },
        )
        .unwrap();
    let (sk, line, point) = line_sketch();
    let skizze = d
        .add_feature(
            "Skizze1",
            sketch_kind(sk, SketchPlane::Standard(StandardPlane::XY), vec![]),
        )
        .unwrap();
    let top = face_with_normal(solid(&d, quader), DVec3::Z);
    let side = face_with_normal(solid(&d, quader), DVec3::X);
    let edge = some_edge(solid(&d, quader));
    let cyl_face = {
        let s = solid(&d, zyl);
        let (id, _) = s
            .faces
            .iter()
            .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
            .unwrap();
        FaceRef::from_face(s, id, &Tolerance::default()).unwrap()
    };
    let mut out = Vec::new();
    let versatz = d
        .add_feature_with_formulas(
            "Ebene1",
            plane(PlaneDef::Offset {
                base: PlaneBase::Face(top.clone()),
                distance: 1.0,
            }),
            [(FieldKey::Abstand, "a*2".to_string())]
                .into_iter()
                .collect(),
        )
        .unwrap();
    out.push((versatz, "versatz"));
    for (name, axis) in [
        ("Ebene2", AxisRef::Standard(StdAxis::X)),
        ("Ebene3", AxisRef::Edge(edge.clone())),
        (
            "Ebene4",
            AxisRef::SketchLine {
                sketch: skizze,
                curve: line,
            },
        ),
    ] {
        let id = d
            .add_feature(name, plane(PlaneDef::Angle { axis, angle: 30.0 }))
            .unwrap();
        out.push((id, "winkel"));
    }
    let mitte = d
        .add_feature(
            "Ebene5",
            plane(PlaneDef::Mid {
                a: PlaneBase::Plane(versatz),
                b: PlaneBase::Standard(StandardPlane::XY),
            }),
        )
        .unwrap();
    out.push((mitte, "mitte"));
    let drei = d
        .add_feature(
            "Ebene6",
            plane(PlaneDef::ThreePoints([
                PointRef::Origin,
                PointRef::EdgeEnd { edge, end: true },
                PointRef::SketchPoint {
                    sketch: skizze,
                    point,
                },
            ])),
        )
        .unwrap();
    out.push((drei, "drei_punkte"));
    let tang = d
        .add_feature(
            "Ebene7",
            plane(PlaneDef::Tangent {
                face: cyl_face,
                base: PlaneBase::Face(side),
                angle: 90.0,
            }),
        )
        .unwrap();
    out.push((tang, "tangential"));
    (d, out)
}

#[test]
fn plane_feature_writes_16_and_roundtrips() {
    let (d, planes) = plane_document();
    let text = encode_version(&d.snapshot()).unwrap();
    assert_eq!(required_format(&d.snapshot()), 16);
    assert_eq!(format_of(&text), 16);
    let back = load(&text).unwrap();
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
    for &(id, art) in &planes {
        assert!(text.contains(&format!("\"art\": \"{art}\"")), "{art}");
        let (a, b) = (d.feature(id).unwrap(), back.feature(id).unwrap());
        assert_eq!(a.kind, b.kind, "{art}");
        assert_eq!(a.formulas(), b.formulas(), "{art}");
        assert_eq!(b.status, FeatureStatus::Ok, "{art}");
        assert_eq!(a.status, FeatureStatus::Ok, "{art}");
        assert_eq!(back.plane_frame(id), d.plane_frame(id), "{art}");
    }
    // Die Formel des Abstands steht unter „abstand“ und ergibt nach dem Laden wieder 6 mm.
    assert!(text.contains("\"abstand\": \"a*2\""), "{text}");
    // Jede Ebene für sich hebt das Format auf 16; ohne Ebenen bliebe es darunter.
    let full = d.snapshot();
    let is_plane = |f: &SnapshotFeature| matches!(f.kind, FeatureKind::Plane(_));
    let mut none = full.clone();
    none.features.retain(|f| !is_plane(f));
    none.features.iter_mut().for_each(|f| f.formulas.clear());
    none.parameters.clear();
    assert!(required_format(&none) < 16);
    for (id, art) in &planes {
        let mut alone = none.clone();
        alone
            .features
            .extend(full.features.iter().filter(|f| f.id == *id).cloned());
        assert_eq!(required_format(&alone), 16, "{art}");
    }
}

#[test]
fn on_plane_sketch_writes_16() {
    let mut d = Document::new();
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Offset {
                base: PlaneBase::Standard(StandardPlane::XY),
                distance: 10.0,
            }),
        )
        .unwrap();
    let frame = d.plane_frame(p).unwrap();
    let mut sk = Sketch::new(frame, "Skizze1", Tolerance::default());
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    let s = d
        .add_feature("Skizze1", sketch_kind(sk, SketchPlane::OnPlane(p), vec![]))
        .unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    assert_eq!(format_of(&text), 16);
    assert!(text.contains("\"typ\": \"ebene\""), "{text}");
    let back = load(&text).unwrap();
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
    let FeatureKind::Sketch(sf) = &back.feature(s).unwrap().kind else {
        panic!("keine Skizze");
    };
    assert_eq!(sf.plane, SketchPlane::OnPlane(p));
    assert_eq!(back.feature(s).unwrap().status, FeatureStatus::Ok);
}

/// Skizze in XZ (schneidet den Quader) mit einer Linie, deren Herkunft `source` ist.
fn copy_document(source: impl FnOnce(&Solid) -> CopySource) -> (Document, FeatureId) {
    let mut d = Document::new();
    let q = add_box(&mut d);
    let src = source(solid(&d, q));
    let mut sk = Sketch::new(StandardPlane::XZ.frame(), "Skizze1", Tolerance::default());
    let a = sk.add_point(0.0, 10.0).unwrap();
    let b = sk.add_point(40.0, 10.0).unwrap();
    let curve = sk.add_line(a, b).unwrap();
    let s = d
        .add_feature(
            "Skizze1",
            sketch_kind(
                sk,
                SketchPlane::Standard(StandardPlane::XZ),
                vec![EdgeCopy { curve, source: src }],
            ),
        )
        .unwrap();
    (d, s)
}

fn copies_of(d: &Document, s: FeatureId) -> Vec<EdgeCopy> {
    match &d.feature(s).unwrap().kind {
        FeatureKind::Sketch(sf) => sf.copies.clone(),
        _ => panic!("keine Skizze"),
    }
}

#[test]
fn section_copy_writes_16() {
    for key in ["schnitt", "silhouette"] {
        let (d, s) = copy_document(|solid| {
            let face = face_with_normal(solid, DVec3::Z);
            if key == "schnitt" {
                CopySource::Section { face, index: 0 }
            } else {
                CopySource::Silhouette { face, index: 0 }
            }
        });
        let text = encode_version(&d.snapshot()).unwrap();
        assert_eq!(format_of(&text), 16, "{key}");
        assert!(text.contains(&format!("\"{key}\"")), "{key}");
        assert!(!text.contains("\"kante\""), "{key}");
        let snap = decode_version(&text, "x.json").unwrap();
        assert_eq!(encode_version(&snap).unwrap(), text, "{key}");
        let back = Document::from_snapshot(snap, 0.05).unwrap();
        assert_eq!(copies_of(&back, s), copies_of(&d, s), "{key}");
    }
}

#[test]
fn edge_copy_with_linked_curve_stays_15() {
    let mut d = Document::new();
    let q = add_box(&mut d);
    let s = solid(&d, q);
    let mut sk = Sketch::new(StandardPlane::XY.frame(), "Skizze1", Tolerance::default());
    // Erste Kante, die sich in XY projizieren lässt (senkrechte ergäben nur einen Punkt).
    let (edge_id, curve) = s
        .edges
        .iter()
        .find_map(|(id, _)| sk.add_projected_edge(s, id).ok().map(|c| (id, c)))
        .unwrap();
    // Verknüpfte Projektion: normale Geometrie statt Konstruktion.
    sk.set_construction(curve, false).unwrap();
    let copy = EdgeCopy {
        curve,
        source: CopySource::Edge(EdgeRef::from_edge(s, edge_id, &Tolerance::default()).unwrap()),
    };
    d.add_feature(
        "Skizze1",
        sketch_kind(sk, SketchPlane::Standard(StandardPlane::XY), vec![copy]),
    )
    .unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    assert!(format_of(&text) <= 15, "{}", format_of(&text));
    assert!(text.contains("\"kante\""));
    assert!(!text.contains("\"schnitt\"") && !text.contains("\"silhouette\""));
    assert_eq!(
        encode_version(&load(&text).unwrap().snapshot()).unwrap(),
        text
    );
}

#[test]
fn format15_file_with_plane_is_rejected() {
    let since = |t: &str| {
        let e = load(t).unwrap_err();
        assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
        let m = e.to_string();
        assert!(m.contains("erst ab Dateiformat 16"), "{m}");
    };
    // Konstruktionsebene.
    let (d, _) = plane_document();
    let text = encode_version(&d.snapshot()).unwrap();
    since(&text.replacen("\"format\": 16", "\"format\": 15", 1));
    // Skizze auf einer Konstruktionsebene (ohne die Ebene selbst in der Datei).
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["format"] = 15.into();
    let features = v["features"].as_array_mut().unwrap();
    features.retain(|f| f["art"]["typ"] != "konstruktionsebene");
    let sketch = features
        .iter_mut()
        .find(|f| f["art"]["typ"] == "skizze")
        .unwrap();
    sketch["art"]["lage"] = serde_json::json!({"typ": "ebene", "feature": 99});
    since(&serde_json::to_string_pretty(&v).unwrap());
    // Schnittprojektion.
    let (d, _) = copy_document(|s| CopySource::Section {
        face: face_with_normal(s, DVec3::Z),
        index: 0,
    });
    let text = encode_version(&d.snapshot()).unwrap();
    since(&text.replacen("\"format\": 16", "\"format\": 15", 1));
}

#[test]
fn a_copy_needs_exactly_one_source() {
    let (d, _) = copy_document(|s| CopySource::Section {
        face: face_with_normal(s, DVec3::Z),
        index: 0,
    });
    let text = encode_version(&d.snapshot()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let copy = &mut v["features"][1]["art"]["kopien"][0];
    copy["silhouette"] = copy["schnitt"].clone();
    let e = load(&serde_json::to_string_pretty(&v).unwrap()).unwrap_err();
    assert!(
        e.to_string()
            .contains(&planora_i18n::tr!("proj-copy-source-count")),
        "{e}"
    );
    let copy = &mut v["features"][1]["art"]["kopien"][0];
    copy.as_object_mut().unwrap().remove("schnitt");
    copy.as_object_mut().unwrap().remove("silhouette");
    assert!(load(&serde_json::to_string_pretty(&v).unwrap()).is_err());
}

#[test]
fn a_huge_sketch_index_in_a_plane_is_rejected() {
    let (d, _) = plane_document();
    let text = encode_version(&d.snapshot()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let f = v["features"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["art"]["achse"]["typ"] == "skizzenlinie")
        .unwrap();
    // 2^32: würde beim Abschneiden zur Kurve 0.
    f["art"]["achse"]["kurve"] = 4_294_967_296u64.into();
    let e = decode_version(&serde_json::to_string_pretty(&v).unwrap(), "x.json").unwrap_err();
    assert!(matches!(e, ProjectError::Corrupt { .. }), "{e:?}");
}

/// Controller-Entscheid: Eine Projektion, deren Kurve in der Skizze nicht mehr lebt, fällt beim
/// Laden weg (`SketchFeature::prune_copies`).
#[test]
fn a_copy_of_a_dead_curve_is_dropped_on_load() {
    let (d, s) = copy_document(|solid| CopySource::Edge(some_edge(solid)));
    let text = encode_version(&d.snapshot()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let kopien = v["features"][1]["art"]["kopien"].as_array_mut().unwrap();
    let mut stale = kopien[0].clone();
    stale["kurve"] = 999.into();
    kopien.push(stale);
    let edited = serde_json::to_string_pretty(&v).unwrap();
    let snap = decode_version(&edited, "x.json").unwrap();
    let back = Document::from_snapshot(snap, 0.05).unwrap();
    assert_eq!(copies_of(&back, s), copies_of(&d, s));
    // Gespeichert wird wieder der Text ohne den toten Eintrag.
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
}
