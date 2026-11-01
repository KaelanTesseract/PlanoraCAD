//! Dateiformat 13 (Teilprojekt Skizzen 1, Task 7): Profil-Verweise in der Datei und Übergang
//! alter Profilnummern beim Laden. Alte Dateien ergeben dieselben Körper wie mit der bisherigen
//! Profilbestimmung und werden unverändert zurückgeschrieben.

use planora_feature::*;
use planora_kernel::{AnalyticKernel, Solid};
use planora_math::{DVec2, DVec3};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_project::{ProjectError, decode_version, encode_version, required_format};
use planora_sketch::{CurveId, Sketch, extrude_profile, revolve_profile};

/// Alle Dateien der Tests mit Extrusion oder Rotation.
const OLD_FILES: [(&str, &str); 4] = [
    ("data/format1.json", include_str!("data/format1.json")),
    (
        "data/format1_voll.json",
        include_str!("data/format1_voll.json"),
    ),
    (
        "fixtures/format1_no_drawings.json",
        include_str!("fixtures/format1_no_drawings.json"),
    ),
    (
        "fixtures/format1_profile_numbers.json",
        include_str!("fixtures/format1_profile_numbers.json"),
    ),
];

fn load(text: &str) -> Result<Document, ProjectError> {
    let s = decode_version(text, "x.json")?;
    Document::from_snapshot(s, 0.05).map_err(|e| ProjectError::Invalid(e.to_string()))
}

fn new_sketch(plane: StandardPlane, name: &str) -> Sketch {
    Sketch::new(plane.frame(), name, Default::default())
}

fn rect(s: &mut Sketch, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<CurveId> {
    s.add_rectangle(DVec2::new(x0, y0), DVec2::new(x1, y1))
        .unwrap()
        .lines
        .to_vec()
}

fn line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let a = s.add_point(a.0, a.1).unwrap();
    let b = s.add_point(b.0, b.1).unwrap();
    s.add_line(a, b).unwrap()
}

fn add_sketch(d: &mut Document, name: &str, sketch: Sketch, plane: StandardPlane) -> FeatureId {
    d.add_feature(
        name,
        FeatureKind::Sketch(SketchFeature {
            sketch,
            plane: SketchPlane::Standard(plane),
            copies: vec![],
            formulas: Default::default(),
        }),
    )
    .unwrap()
}

/// Ein Dokument, wie es ein älteres Programm mit alten Profilnummern geschrieben hat: ein
/// Rechteck, quer durch eine offene Linie (heute zwei Flächen, Review-Fokus 4); zwei getrennte
/// Rechtecke, das zweite mit Loch, Profil 1; ein Rechteck, um seine linke Kante gedreht.
fn old_document() -> Document {
    let mut d = Document::new();
    let mut a = new_sketch(StandardPlane::XY, "Skizze 1");
    rect(&mut a, 0.0, 0.0, 20.0, 10.0);
    line(&mut a, (-5.0, 4.0), (25.0, 6.0));
    let a = add_sketch(&mut d, "Skizze 1", a, StandardPlane::XY);
    d.add_feature(
        "Extrusion 1",
        FeatureKind::Extrude {
            sketch: a,
            profiles: ProfileSel::Index(0),
            height: 5.0,
        },
    )
    .unwrap();

    let mut b = new_sketch(StandardPlane::XY, "Skizze 2");
    rect(&mut b, 40.0, 0.0, 50.0, 10.0);
    rect(&mut b, 60.0, 0.0, 80.0, 10.0);
    let m = b.add_point(70.0, 5.0).unwrap();
    b.add_circle(m, 3.0).unwrap();
    let b = add_sketch(&mut d, "Skizze 2", b, StandardPlane::XY);
    d.add_feature(
        "Extrusion 2",
        FeatureKind::Extrude {
            sketch: b,
            profiles: ProfileSel::Index(1),
            height: 3.0,
        },
    )
    .unwrap();

    let mut c = new_sketch(StandardPlane::XZ, "Skizze 3");
    let lines = rect(&mut c, 0.0, 0.0, 5.0, 8.0);
    // Drehachse ist die linke Kante (x = 0)
    let axis = *lines
        .iter()
        .find(|&&l| {
            c.curve(l)
                .unwrap()
                .kind
                .points()
                .iter()
                .all(|&p| c.point_xy(p).unwrap().x == 0.0)
        })
        .unwrap();
    let c = add_sketch(&mut d, "Skizze 3", c, StandardPlane::XZ);
    d.add_feature(
        "Rotation 1",
        FeatureKind::Revolve {
            sketch: c,
            profiles: ProfileSel::Index(0),
            axis,
        },
    )
    .unwrap();
    d
}

/// Schreibt die Fixtur (alte Profilnummern, Format 1) neu, wenn `PLANORA_UPDATE_GOLDEN` gesetzt
/// ist; sonst muss das alte Dokument genau ihren Text ergeben.
#[test]
fn the_fixture_with_old_profile_numbers_is_what_an_old_program_wrote() {
    let text = encode_version(&old_document().snapshot()).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/format1_profile_numbers.json");
    if std::env::var("PLANORA_UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, &text).unwrap();
    }
    let fixture = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, fixture);
    assert!(text.contains("\"format\": 1,"), "{text}");
    assert!(!text.contains("\"profile\""), "{text}");
}

struct Body {
    volume: f64,
    hull: (DVec3, DVec3),
    faces: usize,
}

fn body(solid: &Solid) -> Body {
    let mesh = tessellate(
        solid,
        &TessellationOptions {
            chord_tolerance: 1e-3,
        },
    )
    .unwrap();
    let hull = mesh
        .positions
        .iter()
        .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), q| {
            (lo.min(*q), hi.max(*q))
        });
    Body {
        volume: mass_properties(&mesh).volume,
        hull,
        faces: solid.faces.len(),
    }
}

/// Alte Profilnummer jeder Extrusion und Rotation, wie sie in der Datei steht.
fn numbers_in(text: &str) -> Vec<(u64, usize)> {
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    v["features"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| matches!(f["art"]["typ"].as_str(), Some("extrusion" | "rotation")))
        .map(|f| {
            (
                f["id"].as_u64().unwrap(),
                f["art"]["profil"].as_u64().unwrap() as usize,
            )
        })
        .collect()
}

#[test]
fn old_files_give_the_same_bodies_and_are_written_back_unchanged() {
    let kernel = AnalyticKernel::default();
    let mut migrated = 0;
    for (name, text) in OLD_FILES {
        let numbers = numbers_in(text);
        assert!(!numbers.is_empty(), "{name}");
        let doc = load(text).unwrap();
        for (id, n) in numbers {
            let f = doc.feature(FeatureId(id)).unwrap();
            assert_eq!(f.status, FeatureStatus::Ok, "{name}: {}", f.name);
            let (sketch, profiles) = match &f.kind {
                FeatureKind::Extrude {
                    sketch, profiles, ..
                }
                | FeatureKind::Revolve {
                    sketch, profiles, ..
                } => (*sketch, profiles),
                _ => unreachable!(),
            };
            migrated += usize::from(matches!(profiles, ProfileSel::Refs(_)));
            let sk = doc.sketch_of(sketch).unwrap();
            let old = &sk.legacy_profiles().profiles[n];
            let expected = match &f.kind {
                FeatureKind::Extrude { height, .. } => {
                    extrude_profile(&kernel, sk, old, *height).unwrap()
                }
                FeatureKind::Revolve { axis, .. } => {
                    revolve_profile(&kernel, sk, old, *axis).unwrap()
                }
                _ => unreachable!(),
            };
            let got = doc
                .body(BodyRef {
                    feature: f.id,
                    index: 0,
                })
                .unwrap();
            assert!(
                doc.body(BodyRef {
                    feature: f.id,
                    index: 1
                })
                .is_none(),
                "{name}: {} hat mehr als einen Körper",
                f.name
            );
            let (a, b) = (body(&expected), body(got));
            assert!(
                ((a.volume - b.volume) / a.volume).abs() < 1e-9,
                "{name}: {}: Volumen {} statt {}",
                f.name,
                b.volume,
                a.volume
            );
            assert!(
                a.hull.0.distance(b.hull.0) < 1e-9 && a.hull.1.distance(b.hull.1) < 1e-9,
                "{name}: {}: Hülle",
                f.name
            );
            assert_eq!(a.faces, b.faces, "{name}: {}: Flächen", f.name);
        }
        let s = doc.snapshot();
        let format = serde_json::from_str::<serde_json::Value>(text).unwrap()["format"]
            .as_u64()
            .unwrap() as u32;
        assert_eq!(required_format(&s), format, "{name}");
        assert_eq!(
            encode_version(&s).unwrap(),
            text.trim_end_matches('\n'),
            "{name}"
        );
    }
    // Jede Datei-Nummer wird beim Laden umgerechnet.
    assert_eq!(migrated, 1 + 2 + 2 + 3);
}

#[test]
fn an_old_number_becomes_references_to_todays_faces() {
    let text = include_str!("fixtures/format1_profile_numbers.json");
    let s = decode_version(text, "x.json").unwrap();
    let FeatureKind::Extrude {
        profiles: ProfileSel::Refs(refs),
        ..
    } = &s.features[1].kind
    else {
        panic!("{:?}", s.features[1].kind)
    };
    // Die offene Linie teilt das alte Profil in zwei Flächen.
    assert_eq!(refs.len(), 2);
    let FeatureKind::Sketch(sf) = &s.features[0].kind else {
        unreachable!()
    };
    assert_eq!(sf.sketch.legacy_index_of(refs), Some(0));
}

/// Neue Datei: zwei getrennte Flächen in einer Extrusion lassen sich nicht als alte Nummer
/// schreiben.
fn two_profiles_document() -> Document {
    let mut d = Document::new();
    let mut sk = new_sketch(StandardPlane::XY, "Skizze 1");
    rect(&mut sk, 0.0, 0.0, 10.0, 10.0);
    rect(&mut sk, 20.0, 0.0, 30.0, 10.0);
    let refs = sk.profiles().refs;
    assert_eq!(refs.len(), 2);
    let sid = add_sketch(&mut d, "Skizze 1", sk, StandardPlane::XY);
    d.add_feature(
        "Extrusion 1",
        FeatureKind::Extrude {
            sketch: sid,
            profiles: ProfileSel::Refs(refs),
            height: 4.0,
        },
    )
    .unwrap();
    d
}

#[test]
fn a_feature_with_two_profiles_writes_format_13_and_loads_the_same() {
    let d = two_profiles_document();
    assert_eq!(d.features()[1].status, FeatureStatus::Ok);
    let s = d.snapshot();
    assert_eq!(required_format(&s), 13);
    let text = encode_version(&s).unwrap();
    assert!(text.contains("\"format\": 13"), "{text}");
    assert!(text.contains("\"profile\""), "{text}");
    assert!(!text.contains("\"profil\""), "{text}");
    let back = load(&text).unwrap();
    assert_eq!(back.features()[1].kind, d.features()[1].kind);
    assert_eq!(back.features()[1].status, FeatureStatus::Ok);
    for i in 0..2 {
        let r = |doc: &Document| {
            body(
                doc.body(BodyRef {
                    feature: doc.features()[1].id,
                    index: i,
                })
                .unwrap(),
            )
        };
        let (a, b) = (r(&d), r(&back));
        assert_eq!((a.volume, a.hull, a.faces), (b.volume, b.hull, b.faces));
    }
    assert_eq!(encode_version(&back.snapshot()).unwrap(), text);
}

/// Ein Verweis auf genau ein altes Profil bleibt eine Nummer: Ein Rechteck schreibt `profil: 0`
/// in Format 1, auch wenn der Verweis aus einer neuen Auswahl stammt.
#[test]
fn a_reference_to_an_old_profile_is_written_as_its_number() {
    let mut d = Document::new();
    let mut sk = new_sketch(StandardPlane::XY, "Skizze 1");
    rect(&mut sk, 0.0, 0.0, 8.0, 6.0);
    let refs = sk.profiles().refs;
    let sid = add_sketch(&mut d, "Skizze 1", sk, StandardPlane::XY);
    d.add_feature(
        "Extrusion 1",
        FeatureKind::Extrude {
            sketch: sid,
            profiles: ProfileSel::Refs(refs),
            height: 4.0,
        },
    )
    .unwrap();
    let s = d.snapshot();
    assert_eq!(required_format(&s), 1);
    let text = encode_version(&s).unwrap();
    assert!(text.contains("\"format\": 1,"), "{text}");
    assert!(text.contains("\"profil\": 0"), "{text}");
    assert!(!text.contains("\"profile\""), "{text}");
}

fn corrupt(text: &str, needle: &str) -> String {
    match load(text) {
        Err(e @ ProjectError::Corrupt { .. }) => {
            let m = e.to_string();
            assert!(m.contains(needle), "{m}");
            m
        }
        other => panic!("{needle}: {other:?}"),
    }
}

#[test]
fn broken_references_give_named_errors() {
    let good = encode_version(&two_profiles_document().snapshot()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&good).unwrap();
    // Feld `i` des ersten Stücks des ersten Verweises geändert
    let piece = |i: usize, x: serde_json::Value| {
        let mut bad = v.clone();
        bad["features"][1]["art"]["profile"][0]["aussen"][0][i] = x;
        serde_json::to_string(&bad).unwrap()
    };
    corrupt(
        &piece(0, serde_json::json!(999)),
        &planora_i18n::tr!("proj-profile-curve-unknown", curve = "999"),
    );
    let first = &v["features"][1]["art"]["profile"][0]["aussen"][0];
    let (u0, u1) = (first[1].as_f64().unwrap(), first[2].as_f64().unwrap());
    assert!(u0 < u1);
    let invalid = planora_i18n::tr!("proj-profile-piece-invalid");
    // u0 > u1, mehr als ein Umlauf, negative Länge
    corrupt(&piece(1, serde_json::json!(u1 + 0.5)), &invalid);
    corrupt(&piece(2, serde_json::json!(u0 + 1.5)), &invalid);
    corrupt(&piece(4, serde_json::json!(-1.0)), &invalid);
    // NaN lässt sich in JSON nicht schreiben: Lesefehler, keine Panik
    let t = piece(1, serde_json::json!("NAN_HERE")).replace("\"NAN_HERE\"", "NaN");
    assert!(t.contains("NaN"));
    assert!(matches!(load(&t), Err(ProjectError::Corrupt { .. })));
    // beide Felder
    let mut bad = v.clone();
    bad["features"][1]["art"]["profil"] = serde_json::json!(0);
    corrupt(
        &serde_json::to_string(&bad).unwrap(),
        &planora_i18n::tr!("proj-err-profile-field"),
    );
    // keins
    let mut bad = v.clone();
    bad["features"][1]["art"]
        .as_object_mut()
        .unwrap()
        .remove("profile");
    corrupt(
        &serde_json::to_string(&bad).unwrap(),
        &planora_i18n::tr!("proj-err-profile-field"),
    );
    // Verweise gibt es erst ab Format 13
    let mut bad = v.clone();
    bad["format"] = serde_json::json!(12);
    corrupt(
        &serde_json::to_string(&bad).unwrap(),
        &planora_i18n::tr!("proj-feat-profile-refs"),
    );
    // die Rotation ebenso: keins der Felder
    let rot = include_str!("fixtures/format1_profile_numbers.json");
    let mut bad: serde_json::Value = serde_json::from_str(rot).unwrap();
    let f = bad["features"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["art"]["typ"] == "rotation")
        .unwrap();
    f["art"].as_object_mut().unwrap().remove("profil");
    corrupt(
        &serde_json::to_string(&bad).unwrap(),
        &planora_i18n::tr!("proj-err-profile-field"),
    );
}

/// Dreieck (0,0),(20,10),(20,-10) mit einem Kreis innen, der die Hypotenuse berührt.
fn tangent_circle_sketch(s_par: f64, r: f64) -> Sketch {
    let mut s = new_sketch(StandardPlane::XY, "Skizze 1");
    line(&mut s, (0.0, 0.0), (20.0, 10.0));
    line(&mut s, (20.0, 10.0), (20.0, -10.0));
    line(&mut s, (20.0, -10.0), (0.0, 0.0));
    let n = DVec2::new(1.0, -2.0).normalize();
    let c = DVec2::new(2.0 * s_par, s_par) + n * r;
    let m = s.add_point(c.x, c.y).unwrap();
    s.add_circle(m, r).unwrap();
    s
}

/// Zwei Kreise, die sich von außen in Richtung `angle` berühren.
fn touching_circles_sketch(angle: f64) -> Sketch {
    let mut s = new_sketch(StandardPlane::XY, "Skizze 1");
    let m = s.add_point(0.0, 0.0).unwrap();
    s.add_circle(m, 3.0).unwrap();
    let c = DVec2::new(angle.cos(), angle.sin()) * 5.0;
    let m = s.add_point(c.x, c.y).unwrap();
    s.add_circle(m, 2.0).unwrap();
    s
}

/// Gesamtprüfung C1: Ein Kreis mit genau einem Knoten (berührt eine Linie oder einen anderen
/// Kreis) ist ein Stück über einen vollen Umlauf. Sein Verweis muss sich speichern und laden
/// lassen; früher lag `u1` je nach Lage des Knotens um ein ulp über `u0 + 1`.
#[test]
fn a_disc_bounded_by_a_circle_with_one_node_saves_and_loads() {
    let mut sketches: Vec<Sketch> = (0..40)
        .map(|i| tangent_circle_sketch(4.0 + 0.1 * i as f64, 2.0 + 0.05 * i as f64))
        .collect();
    sketches.extend((0..24).map(|i| touching_circles_sketch(0.37 + i as f64 * 0.26)));
    let (mut discs, mut ulp_cases) = (0, 0);
    for sk in sketches {
        let refs = sk.profiles().refs;
        for r in refs
            .iter()
            .filter(|r| r.outer.len() == 1 && r.holes.is_empty())
        {
            let p = &r.outer[0];
            assert_eq!(p.u1, p.u0 + 1.0, "voller Umlauf: {p:?}");
            let t0 = p.u0 * std::f64::consts::TAU;
            if (t0 + std::f64::consts::TAU) / std::f64::consts::TAU > p.u0 + 1.0 {
                ulp_cases += 1;
            }
            discs += 1;
            let mut d = Document::new();
            let sid = add_sketch(&mut d, "Skizze 1", sk.clone(), StandardPlane::XY);
            d.add_feature(
                "Extrusion 1",
                FeatureKind::Extrude {
                    sketch: sid,
                    profiles: ProfileSel::Refs(vec![r.clone()]),
                    height: 5.0,
                },
            )
            .unwrap();
            assert_eq!(d.features()[1].status, FeatureStatus::Ok);
            let text = encode_version(&d.snapshot()).unwrap();
            let back = load(&text).unwrap();
            assert_eq!(back.features()[1].status, FeatureStatus::Ok);
            let b = |doc: &Document| {
                body(
                    doc.body(BodyRef {
                        feature: doc.features()[1].id,
                        index: 0,
                    })
                    .unwrap(),
                )
            };
            let (x, y) = (b(&d), b(&back));
            assert_eq!((x.volume, x.hull, x.faces), (y.volume, y.hull, y.faces));
        }
    }
    assert!(discs >= 40, "{discs} Scheiben");
    assert!(ulp_cases > 0, "die Lagen treffen den ulp-Fall nicht");
}

/// Ein Stück über einen vollen Umlauf mit `u1` ein ulp über `u0 + 1` (so schrieb es das
/// Programm vor der Nachbesserung) lädt trotzdem.
#[test]
fn a_full_turn_piece_one_ulp_too_long_still_loads() {
    let mut d = Document::new();
    let sk = touching_circles_sketch(0.5);
    let r = sk
        .profiles()
        .refs
        .into_iter()
        .find(|r| r.outer.len() == 1)
        .unwrap();
    let sid = add_sketch(&mut d, "Skizze 1", sk, StandardPlane::XY);
    d.add_feature(
        "Extrusion 1",
        FeatureKind::Extrude {
            sketch: sid,
            profiles: ProfileSel::Refs(vec![r]),
            height: 5.0,
        },
    )
    .unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let piece = &mut v["features"][1]["art"]["profile"][0]["aussen"][0];
    let u0 = piece[1].as_f64().unwrap();
    piece[2] = serde_json::json!(f64::from_bits((u0 + 1.0).to_bits() + 1));
    let back = load(&serde_json::to_string(&v).unwrap()).unwrap();
    assert_eq!(back.features()[1].status, FeatureStatus::Ok);
}
