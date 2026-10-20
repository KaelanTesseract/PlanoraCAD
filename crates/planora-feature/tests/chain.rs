use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::model::{CurveKind, Sketch};

fn volume(s: &planora_kernel::Solid) -> f64 {
    mass_properties(
        &tessellate(
            s,
            &TessellationOptions {
                chord_tolerance: 1e-3,
            },
        )
        .unwrap(),
    )
    .volume
}

fn rect(w: f64, h: f64) -> Sketch {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    sk.add_rectangle(DVec2::ZERO, DVec2::new(w, h)).unwrap();
    sk
}

fn sketch_feature(sk: Sketch) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch: sk,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
    })
}

fn z_range(s: &planora_kernel::Solid) -> (f64, f64) {
    let zs: Vec<f64> = s.vertices.iter().map(|(_, v)| v.point.z).collect();
    (
        zs.iter().cloned().fold(f64::MAX, f64::min),
        zs.iter().cloned().fold(f64::MIN, f64::max),
    )
}

#[test]
fn changing_a_sketch_or_a_height_updates_the_whole_chain() {
    let mut d = Document::new();
    let s = d
        .add_feature("Skizze 1", sketch_feature(rect(40.0, 30.0)))
        .unwrap();
    let e = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 10.0,
            },
        )
        .unwrap();
    assert!((volume(d.visible_bodies()[0].solid) - 12000.0).abs() < 1e-3);
    d.update(e, |k| {
        if let FeatureKind::Extrude { height, .. } = k {
            *height = 20.0;
        }
    })
    .unwrap();
    assert!((volume(d.visible_bodies()[0].solid) - 24000.0).abs() < 1e-3);
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.sketch = rect(50.0, 30.0);
        }
    })
    .unwrap();
    assert!((volume(d.visible_bodies()[0].solid) - 30000.0).abs() < 1e-3);
}

fn deckel_ref(d: &Document, extrude: FeatureId) -> FaceRef {
    let solid = d
        .body(BodyRef {
            feature: extrude,
            index: 0,
        })
        .unwrap();
    let (id, _) = solid
        .faces
        .iter()
        .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == "Deckel"))
        .unwrap();
    FaceRef::from_face(solid, id, &Tolerance::default()).unwrap()
}

fn circle_sketch() -> Sketch {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let c = sk.add_point(20.0, 15.0).unwrap();
    sk.add_circle(c, 5.0).unwrap();
    sk
}

#[test]
fn a_sketch_on_a_face_follows_the_body() {
    let mut d = Document::new();
    let s1 = d
        .add_feature("Skizze 1", sketch_feature(rect(40.0, 30.0)))
        .unwrap();
    let e1 = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s1,
                profile: 0,
                height: 10.0,
            },
        )
        .unwrap();
    let face = deckel_ref(&d, e1);
    let s2 = d
        .add_feature(
            "Skizze 2",
            FeatureKind::Sketch(SketchFeature {
                sketch: circle_sketch(),
                plane: SketchPlane::OnFace(face),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let e2 = d
        .add_feature(
            "Extrusion 2",
            FeatureKind::Extrude {
                sketch: s2,
                profile: 0,
                height: 5.0,
            },
        )
        .unwrap();
    assert_eq!(
        d.feature(e2).unwrap().status,
        FeatureStatus::Ok,
        "{:?}",
        d.feature(e2).unwrap().status
    );
    let (z0, z1) = z_range(
        d.body(BodyRef {
            feature: e2,
            index: 0,
        })
        .unwrap(),
    );
    assert!(
        (z0 - 10.0).abs() < 1e-9 && (z1 - 15.0).abs() < 1e-9,
        "{z0} {z1}"
    );
    // erste Extrusion höher → die Skizze auf der Deckelfläche folgt
    d.update(e1, |k| {
        if let FeatureKind::Extrude { height, .. } = k {
            *height = 20.0;
        }
    })
    .unwrap();
    let (z0, z1) = z_range(
        d.body(BodyRef {
            feature: e2,
            index: 0,
        })
        .unwrap(),
    );
    assert!(
        (z0 - 20.0).abs() < 1e-9 && (z1 - 25.0).abs() < 1e-9,
        "{z0} {z1}"
    );
    // und beide lassen sich vereinigen
    let u = d
        .add_feature(
            "Vereinigung",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: e1,
                    index: 0,
                },
                tool: BodyRef {
                    feature: e2,
                    index: 0,
                },
                op: BoolOp::Union,
            },
        )
        .unwrap();
    assert_eq!(
        d.feature(u).unwrap().status,
        FeatureStatus::Ok,
        "{:?}",
        d.feature(u).unwrap().status
    );
    let want = 40.0 * 30.0 * 20.0 + std::f64::consts::PI * 25.0 * 5.0;
    assert!((volume(d.visible_bodies()[0].solid) - want).abs() < 1.0);
}

#[test]
fn a_lost_face_is_reported_at_its_feature_and_dependents_are_skipped() {
    let mut d = Document::new();
    let s1 = d
        .add_feature("Skizze 1", sketch_feature(rect(40.0, 30.0)))
        .unwrap();
    let e1 = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s1,
                profile: 0,
                height: 10.0,
            },
        )
        .unwrap();
    let mut face = deckel_ref(&d, e1);
    face.role = "Wand 9".into();
    let s2 = d
        .add_feature(
            "Skizze 2",
            FeatureKind::Sketch(SketchFeature {
                sketch: circle_sketch(),
                plane: SketchPlane::OnFace(face),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let e2 = d
        .add_feature(
            "Extrusion 2",
            FeatureKind::Extrude {
                sketch: s2,
                profile: 0,
                height: 5.0,
            },
        )
        .unwrap();
    assert!(
        matches!(&d.feature(s2).unwrap().status, FeatureStatus::Error(m) if m.contains("Wand 9")),
        "{:?}",
        d.feature(s2).unwrap().status
    );
    assert!(matches!(
        d.feature(e2).unwrap().status,
        FeatureStatus::Skipped(_)
    ));
    assert_eq!(
        d.feature(e1).unwrap().status,
        FeatureStatus::Ok,
        "der Rest bleibt benutzbar"
    );
    assert_eq!(d.visible_bodies().len(), 1);
}

#[test]
fn copied_body_edges_follow_a_resized_body() {
    let mut d = Document::new();
    let b = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(10.0, 20.0, 5.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let solid = d
        .body(BodyRef {
            feature: b,
            index: 0,
        })
        .unwrap()
        .clone();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let tol = Tolerance::default();
    let mut copy = None;
    for (id, _) in solid.edges.iter() {
        if let Ok(c) = sk.add_projected_edge(&solid, id)
            && matches!(sk.curve(c).unwrap().kind, CurveKind::Line { .. })
            && (sk.line_length(c).unwrap() - 10.0).abs() < 1e-9
        {
            copy = Some((c, EdgeRef::from_edge(&solid, id, &tol).unwrap()));
            break;
        }
    }
    let (curve, edge) = copy.expect("eine Kante der Länge 10");
    let s = d
        .add_feature(
            "Skizze",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![EdgeCopy { curve, edge }],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    d.update(b, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.x = 12.0;
        }
    })
    .unwrap();
    assert_eq!(
        d.feature(s).unwrap().status,
        FeatureStatus::Ok,
        "{:?}",
        d.feature(s).unwrap().status
    );
    let sk = d.sketch_of(s).unwrap();
    assert!(
        (sk.line_length(curve).unwrap() - 12.0).abs() < 1e-7,
        "die Kantenkopie ist mitgewachsen"
    );
}

#[test]
fn deleting_an_extrusion_removes_the_sketch_on_its_face_and_the_extrusion_on_it() {
    let mut d = Document::new();
    let s1 = d
        .add_feature("Skizze 1", sketch_feature(rect(40.0, 30.0)))
        .unwrap();
    let e1 = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s1,
                profile: 0,
                height: 10.0,
            },
        )
        .unwrap();
    let face = deckel_ref(&d, e1);
    let s2 = d
        .add_feature(
            "Skizze 2",
            FeatureKind::Sketch(SketchFeature {
                sketch: circle_sketch(),
                plane: SketchPlane::OnFace(face),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let e2 = d
        .add_feature(
            "Extrusion 2",
            FeatureKind::Extrude {
                sketch: s2,
                profile: 0,
                height: 5.0,
            },
        )
        .unwrap();
    assert_eq!(d.dependents(e1), vec![s2, e2]);
    assert_eq!(
        d.delete(e1),
        Err(FeatureError::HasDependents(vec![s2.0, e2.0]))
    );
    assert_eq!(d.features().len(), 4, "unverändert");
    let removed = d.delete_with_dependents(e1).unwrap();
    assert_eq!(removed, vec![e1, s2, e2]);
    let left: Vec<FeatureId> = d.features().iter().map(|f| f.id).collect();
    assert_eq!(left, vec![s1]);
    assert!(d.visible_bodies().is_empty());
    d.check_invariants().unwrap();
}
