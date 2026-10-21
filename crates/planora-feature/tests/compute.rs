use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::model::Sketch;

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

pub fn rect_sketch(w: f64, h: f64) -> SketchFeature {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    sk.add_rectangle(DVec2::ZERO, DVec2::new(w, h)).unwrap();
    SketchFeature {
        sketch: sk,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
    }
}

#[test]
fn primitives_and_extrusion_produce_bodies_with_origin_tags() {
    let mut d = Document::new();
    let b = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(2.0, 3.0, 4.0),
                origin: DVec3::new(10.0, 0.0, 0.0),
            },
        )
        .unwrap();
    let s = d
        .add_feature("Skizze 1", FeatureKind::Sketch(rect_sketch(40.0, 30.0)))
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
    for id in [b, s, e] {
        assert_eq!(d.feature(id).unwrap().status, FeatureStatus::Ok, "{id:?}");
    }
    let vis = d.visible_bodies();
    assert_eq!(vis.len(), 2, "die Skizze liefert keinen Körper");
    assert!((volume(vis[0].solid) - 24.0).abs() < 1e-6);
    assert!((volume(vis[1].solid) - 12000.0).abs() < 1e-3);
    let bounds_x = vis[0]
        .solid
        .vertices
        .iter()
        .map(|(_, v)| v.point.x)
        .fold(f64::MAX, f64::min);
    assert!((bounds_x - 10.0).abs() < 1e-9, "Position");
    assert!(
        vis[1]
            .solid
            .faces
            .iter()
            .all(|(_, f)| f.tag.as_ref().unwrap().origin == e.0)
    );
    assert_eq!(vis[1].name, "Extrusion 1");
    assert!(d.sketch_of(s).is_some());
}

#[test]
fn a_boolean_consumes_its_inputs_and_checks_the_volume() {
    let mut d = Document::new();
    let a = d
        .add_feature(
            "A",
            FeatureKind::Box {
                size: DVec3::splat(10.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let c = d
        .add_feature(
            "Z",
            FeatureKind::Cylinder {
                radius: 2.0,
                height: 10.0,
                origin: DVec3::new(5.0, 5.0, 0.0),
            },
        )
        .unwrap();
    let diff = d
        .add_feature(
            "Abzug",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: a,
                    index: 0,
                },
                tool: BodyRef {
                    feature: c,
                    index: 0,
                },
                op: BoolOp::Difference,
            },
        )
        .unwrap();
    assert_eq!(d.feature(diff).unwrap().status, FeatureStatus::Ok);
    let vis = d.visible_bodies();
    assert_eq!(vis.len(), 1);
    assert_eq!(vis[0].body.feature, diff);
    assert!((volume(vis[0].solid) - (1000.0 - std::f64::consts::PI * 4.0 * 10.0)).abs() < 0.5);
    assert!(
        d.body(BodyRef {
            feature: a,
            index: 0
        })
        .is_some(),
        "Ergebnisse früherer Features bleiben lesbar"
    );
}

#[test]
fn a_move_replaces_its_body_and_keeps_the_tags() {
    let mut d = Document::new();
    let a = d
        .add_feature(
            "A",
            FeatureKind::Box {
                size: DVec3::splat(2.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let m = d
        .add_feature(
            "Verschieben",
            FeatureKind::Move {
                body: BodyRef {
                    feature: a,
                    index: 0,
                },
                delta: DVec3::new(0.0, 0.0, 5.0),
            },
        )
        .unwrap();
    let vis = d.visible_bodies();
    assert_eq!(vis.len(), 1);
    assert_eq!(
        vis[0].body,
        BodyRef {
            feature: m,
            index: 0
        }
    );
    let zmin = vis[0]
        .solid
        .vertices
        .iter()
        .map(|(_, v)| v.point.z)
        .fold(f64::MAX, f64::min);
    assert!((zmin - 5.0).abs() < 1e-9);
    assert!(
        vis[0]
            .solid
            .faces
            .iter()
            .all(|(_, f)| f.tag.as_ref().unwrap().origin == a.0),
        "Marken bleiben beim Verschieben"
    );
}

#[test]
fn revolve_from_a_sketch_axis() {
    let mut d = Document::new();
    let mut sk = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let (p, q) = (
        sk.add_point(0.0, -1.0).unwrap(),
        sk.add_point(0.0, 6.0).unwrap(),
    );
    let axis = sk.add_line(p, q).unwrap();
    sk.set_construction(axis, true).unwrap();
    sk.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(3.0, 5.0))
        .unwrap();
    let s = d
        .add_feature(
            "S",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let r = d
        .add_feature(
            "Rotation",
            FeatureKind::Revolve {
                sketch: s,
                profile: 0,
                axis,
            },
        )
        .unwrap();
    assert_eq!(
        d.feature(r).unwrap().status,
        FeatureStatus::Ok,
        "{:?}",
        d.feature(r).unwrap().status
    );
    let v = volume(d.visible_bodies()[0].solid);
    assert!((v - std::f64::consts::PI * 9.0 * 5.0).abs() < 0.2);
}

#[test]
fn adding_features_validates_references() {
    let mut d = Document::new();
    assert!(matches!(
        d.add_feature(
            "X",
            FeatureKind::Extrude {
                sketch: FeatureId(7),
                profile: 0,
                height: 1.0
            }
        ),
        Err(FeatureError::UnknownFeature)
    ));
    assert!(d.features().is_empty(), "nichts wurde angelegt");
}

#[test]
fn a_sketch_on_a_face_follows_the_face_and_moving_the_marker_reuses_results() {
    let mut d = Document::new();
    let a = d
        .add_feature(
            "A",
            FeatureKind::Box {
                size: DVec3::new(50.0, 50.0, 5.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let solid = d
        .body(BodyRef {
            feature: a,
            index: 0,
        })
        .unwrap();
    let top = solid
        .faces
        .iter()
        .map(|(id, _)| id)
        .find(|&f| {
            planora_sketch::plane_of_face(solid, f)
                .is_some_and(|fr| fr.z.z > 0.5 && fr.origin.z > 4.0)
        })
        .unwrap();
    let face = FaceRef::from_face(solid, top, &Tolerance::default()).unwrap();
    let mut sk = rect_sketch(10.0, 10.0);
    sk.plane = SketchPlane::OnFace(face);
    let s = d.add_feature("S", FeatureKind::Sketch(sk)).unwrap();
    let e = d
        .add_feature(
            "E",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 3.0,
            },
        )
        .unwrap();
    assert_eq!(
        d.feature(e).unwrap().status,
        FeatureStatus::Ok,
        "{:?}",
        d.feature(e).unwrap().status
    );
    assert!((d.sketch_of(s).unwrap().frame().origin.z - 5.0).abs() < 1e-9);
    let zmax = d
        .body(BodyRef {
            feature: e,
            index: 0,
        })
        .unwrap()
        .vertices
        .iter()
        .map(|(_, v)| v.point.z)
        .fold(f64::MIN, f64::max);
    assert!((zmax - 8.0).abs() < 1e-9);
    assert!(matches!(
        d.add_feature(
            "X",
            FeatureKind::Extrude {
                sketch: a,
                profile: 0,
                height: 1.0
            }
        )
        .map(|x| d.feature(x).unwrap().status.clone()),
        Ok(FeatureStatus::Error(_))
    ));
    let before: *const planora_kernel::Solid = d
        .body(BodyRef {
            feature: e,
            index: 0,
        })
        .unwrap();
    d.set_marker(1).unwrap();
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::NotComputed);
    assert_eq!(d.visible_bodies().len(), 1);
    d.set_marker(4).unwrap();
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    let after: *const planora_kernel::Solid = d
        .body(BodyRef {
            feature: e,
            index: 0,
        })
        .unwrap();
    assert!(
        std::ptr::eq(before, after),
        "das Ergebnis wurde übernommen, nicht neu berechnet"
    );
}

#[test]
fn an_implausible_or_invalid_profile_index_is_an_error_not_a_panic() {
    let mut d = Document::new();
    let s = d
        .add_feature("S", FeatureKind::Sketch(rect_sketch(1.0, 1.0)))
        .unwrap();
    let e = d
        .add_feature(
            "E",
            FeatureKind::Extrude {
                sketch: s,
                profile: 3,
                height: 1.0,
            },
        )
        .unwrap();
    assert!(
        matches!(&d.feature(e).unwrap().status, FeatureStatus::Error(m) if m.contains("Profil 3"))
    );
    let b = d
        .add_feature(
            "B",
            FeatureKind::Box {
                size: DVec3::ONE,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let same = d
        .add_feature(
            "U",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: b,
                    index: 0,
                },
                tool: BodyRef {
                    feature: b,
                    index: 0,
                },
                op: BoolOp::Union,
            },
        )
        .unwrap();
    assert!(matches!(
        d.feature(same).unwrap().status,
        FeatureStatus::Error(_)
    ));
    assert!(matches!(
        d.add_feature(
            "F",
            FeatureKind::Move {
                body: BodyRef {
                    feature: same,
                    index: 0
                },
                delta: DVec3::X
            }
        )
        .map(|x| d.feature(x).unwrap().status.clone()),
        Ok(FeatureStatus::Skipped(_))
    ));
}
