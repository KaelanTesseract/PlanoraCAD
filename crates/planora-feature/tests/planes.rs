//! Konstruktionsebenen (Skizzen 6, Task 5): je Art ein Frame-Test, Folgen bei geänderten Körpern,
//! Formeln und Löschen samt Abhängigen.

use planora_feature::planes::{AxisRef, PlaneBase, PlaneDef, PlaneFeature, PointRef, StdAxis};
use planora_feature::*;
use planora_kernel::{Solid, Surface};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::Sketch;

const EPS: f64 = 1e-9;

fn plane(def: PlaneDef) -> FeatureKind {
    FeatureKind::Plane(PlaneFeature { def })
}

fn add_box(d: &mut Document, size: DVec3) -> FeatureId {
    d.add_feature(
        "Quader1",
        FeatureKind::Box {
            size,
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
    let tol = Tolerance::default();
    let (id, _) = s
        .faces
        .iter()
        .find(|(f, _)| planora_sketch::plane_of_face(s, *f).is_some_and(|fr| fr.z.dot(n) > 0.999))
        .expect("Fläche fehlt");
    FaceRef::from_face(s, id, &tol).unwrap()
}

/// Gerade Kante zwischen den Punkten `p` und `q` (Reihenfolge beliebig) und ob `p` ihr Ende ist.
fn edge_between(s: &Solid, p: DVec3, q: DVec3) -> (EdgeRef, bool) {
    let tol = Tolerance::default();
    for (id, e) in s.edges.iter() {
        let (a, b) = (s.vertices[e.start].point, s.vertices[e.end].point);
        let same = |x: DVec3, y: DVec3| x.distance(y) < 1e-9;
        if same(a, p) && same(b, q) {
            return (EdgeRef::from_edge(s, id, &tol).unwrap(), false);
        }
        if same(a, q) && same(b, p) {
            return (EdgeRef::from_edge(s, id, &tol).unwrap(), true);
        }
    }
    panic!("Kante {p} – {q} fehlt");
}

fn frame_of(d: &Document, id: FeatureId) -> Frame {
    let f = d.feature(id).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    d.plane_frame(id).expect("kein Frame")
}

fn assert_near(a: DVec3, b: DVec3) {
    assert!(a.distance(b) < 1e-9, "{a} ≠ {b}");
}

#[test]
fn offset_from_xy_is_parallel_at_distance() {
    let mut d = Document::new();
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Offset {
                base: PlaneBase::Standard(StandardPlane::XY),
                distance: 12.5,
            }),
        )
        .unwrap();
    let f = frame_of(&d, p);
    assert!((f.origin.z - 12.5).abs() < EPS);
    assert_near(f.z, DVec3::Z);
    assert!(d.visible_bodies().is_empty(), "eine Ebene ist kein Körper");
    assert_eq!(d.feature(p).unwrap().kind.label(), "Ebene");
    d.check_invariants().unwrap();
}

#[test]
fn offset_follows_face_when_box_grows() {
    let mut d = Document::new();
    let q = add_box(&mut d, DVec3::new(10.0, 20.0, 30.0));
    let top = face_with_normal(solid(&d, q), DVec3::Z);
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Offset {
                base: PlaneBase::Face(top),
                distance: 5.0,
            }),
        )
        .unwrap();
    assert!((frame_of(&d, p).origin.z - 35.0).abs() < EPS);
    d.update(q, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.z = 40.0;
        }
    })
    .unwrap();
    let f = frame_of(&d, p);
    assert!((f.origin.z - 45.0).abs() < EPS, "{}", f.origin);
    assert_near(f.z, DVec3::Z);
    // Ohne den Quader ist die Ebene übersprungen und liefert keinen Frame.
    d.set_suppressed(q, true).unwrap();
    assert!(matches!(
        d.feature(p).unwrap().status,
        FeatureStatus::Skipped(_)
    ));
    assert_eq!(d.plane_frame(p), None);
    d.check_invariants().unwrap();
}

#[test]
fn angle_about_x_axis_45() {
    let mut d = Document::new();
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Angle {
                axis: AxisRef::Standard(StdAxis::X),
                angle: 45.0,
            }),
        )
        .unwrap();
    let f = frame_of(&d, p);
    let c = std::f64::consts::FRAC_1_SQRT_2;
    assert_near(f.z, DVec3::new(0.0, -c, c));
    assert_near(f.x, DVec3::X);
    assert_near(f.origin, DVec3::ZERO);
    assert!(f.is_orthonormal(1e-12));
}

#[test]
fn angle_about_box_edge_uses_adjacent_face() {
    let mut d = Document::new();
    let q = add_box(&mut d, DVec3::new(10.0, 20.0, 30.0));
    // Obere Kante entlang X bei y = 0, z = 30.
    let (edge, _) = edge_between(
        solid(&d, q),
        DVec3::new(0.0, 0.0, 30.0),
        DVec3::new(10.0, 0.0, 30.0),
    );
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Angle {
                axis: AxisRef::Edge(edge),
                angle: 0.0,
            }),
        )
        .unwrap();
    let f = frame_of(&d, p);
    // Bei 0° liegt die Ebene in einer der angrenzenden Flächen (Deckel oder Vorderseite).
    assert!(f.z.dot(DVec3::Z).abs() > 0.999 || f.z.dot(DVec3::Y).abs() > 0.999);
    assert!(f.x.dot(DVec3::X).abs() > 0.999);
    assert!((f.origin.z - 30.0).abs() < EPS && f.origin.y.abs() < EPS);
}

#[test]
fn angle_about_sketch_line_uses_sketch_plane() {
    let mut d = Document::new();
    let tol = Tolerance::default();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", tol);
    let a = sk.add_point(0.0, 5.0).unwrap();
    let b = sk.add_point(10.0, 5.0).unwrap();
    let line = sk.add_line(a, b).unwrap();
    let s = d
        .add_feature(
            "Skizze1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Angle {
                axis: AxisRef::SketchLine {
                    sketch: s,
                    curve: line,
                },
                angle: 90.0,
            }),
        )
        .unwrap();
    let f = frame_of(&d, p);
    // Skizzenebene XY um 90° um die Linie (Richtung +X) gedreht: Normale −Y.
    assert_near(f.z, -DVec3::Y);
    assert!((f.origin.y - 5.0).abs() < EPS);
}

#[test]
fn mid_of_two_box_faces() {
    let mut d = Document::new();
    let q = add_box(&mut d, DVec3::new(10.0, 20.0, 30.0));
    let s = solid(&d, q);
    let (left, right) = (
        face_with_normal(s, -DVec3::X),
        face_with_normal(s, DVec3::X),
    );
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Mid {
                a: PlaneBase::Face(left),
                b: PlaneBase::Face(right),
            }),
        )
        .unwrap();
    let f = frame_of(&d, p);
    assert!((f.origin.x - 5.0).abs() < EPS, "{}", f.origin);
    assert_near(f.z, -DVec3::X);
}

#[test]
fn mid_of_non_parallel_is_bisector() {
    let mut d = Document::new();
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Mid {
                a: PlaneBase::Standard(StandardPlane::XY),
                b: PlaneBase::Standard(StandardPlane::YZ),
            }),
        )
        .unwrap();
    let f = frame_of(&d, p);
    let c = std::f64::consts::FRAC_1_SQRT_2;
    assert_near(f.z, DVec3::new(-c, 0.0, c));
    assert!(
        f.x.dot(DVec3::Y).abs() > 1.0 - EPS,
        "Schnittgerade = Y-Achse"
    );
    assert!(f.origin.x.abs() < EPS && f.origin.z.abs() < EPS);
    assert!(f.is_orthonormal(1e-12));
}

#[test]
fn mid_of_the_same_plane_is_an_error() {
    let mut d = Document::new();
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Mid {
                a: PlaneBase::Standard(StandardPlane::XY),
                b: PlaneBase::Standard(StandardPlane::XY),
            }),
        )
        .unwrap();
    match &d.feature(p).unwrap().status {
        FeatureStatus::Error(m) => assert!(m.contains("fallen zusammen"), "{m}"),
        s => panic!("{s:?}"),
    }
    assert_eq!(d.plane_frame(p), None);
}

#[test]
fn three_points_from_box_corners() {
    let mut d = Document::new();
    let q = add_box(&mut d, DVec3::new(10.0, 20.0, 30.0));
    let s = solid(&d, q);
    let (o, px, py) = (
        DVec3::ZERO,
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(0.0, 20.0, 0.0),
    );
    let (ex, o_is_end) = edge_between(s, o, px);
    let (ey, o_is_end_y) = edge_between(s, o, py);
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::ThreePoints([
                PointRef::EdgeEnd {
                    edge: ex.clone(),
                    end: o_is_end,
                },
                PointRef::EdgeEnd {
                    edge: ex,
                    end: !o_is_end,
                },
                PointRef::EdgeEnd {
                    edge: ey,
                    end: !o_is_end_y,
                },
            ])),
        )
        .unwrap();
    let f = frame_of(&d, p);
    assert_near(f.origin, o);
    assert_near(f.x, DVec3::X);
    assert_near(f.z, DVec3::Z);
}

#[test]
fn collinear_points_error() {
    let mut d = Document::new();
    let tol = Tolerance::default();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", tol);
    let a = sk.add_point(5.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    let s = d
        .add_feature(
            "Skizze1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::ThreePoints([
                PointRef::Origin,
                PointRef::SketchPoint {
                    sketch: s,
                    point: a,
                },
                PointRef::SketchPoint {
                    sketch: s,
                    point: b,
                },
            ])),
        )
        .unwrap();
    match &d.feature(p).unwrap().status {
        FeatureStatus::Error(m) => assert!(m.contains("auf einer Geraden"), "{m}"),
        st => panic!("{st:?}"),
    }
    assert_eq!(d.plane_frame(p), None);
}

#[test]
fn tangent_to_cylinder_at_90() {
    let mut d = Document::new();
    let c = d
        .add_feature(
            "Zylinder1",
            FeatureKind::Cylinder {
                radius: 5.0,
                height: 10.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let s = solid(&d, c);
    let tol = Tolerance::default();
    let (id, _) = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .unwrap();
    let face = FaceRef::from_face(s, id, &tol).unwrap();
    let top = face_with_normal(s, DVec3::Z);
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Tangent {
                face,
                base: PlaneBase::Standard(StandardPlane::XZ),
                angle: 90.0,
            }),
        )
        .unwrap();
    let f = frame_of(&d, p);
    // Normale von XZ ist −Y; um 90° um Z gedreht: +X. Berührpunkt bei x = 5.
    assert_near(f.z, DVec3::X);
    assert!((f.origin.x - 5.0).abs() < EPS && f.origin.y.abs() < EPS);
    assert!(f.x.dot(DVec3::Z).abs() > 1.0 - EPS, "x entlang der Achse");

    // Eine ebene Fläche ist keine gültige Tangentialfläche.
    let bad = d
        .add_feature(
            "Ebene2",
            plane(PlaneDef::Tangent {
                face: top,
                base: PlaneBase::Standard(StandardPlane::XZ),
                angle: 0.0,
            }),
        )
        .unwrap();
    match &d.feature(bad).unwrap().status {
        FeatureStatus::Error(m) => assert!(m.contains("Zylinder"), "{m}"),
        st => panic!("{st:?}"),
    }
}

#[test]
fn formula_drives_offset() {
    let mut d = Document::new();
    d.add_parameter("a", "7", ParamUnit::Length, "").unwrap();
    let def = plane(PlaneDef::Offset {
        base: PlaneBase::Standard(StandardPlane::XY),
        distance: 1.0,
    });
    assert_eq!(field_keys(&def), vec![FieldKey::Abstand]);
    let p = d
        .add_feature_with_formulas(
            "Ebene1",
            def,
            [(FieldKey::Abstand, "a*2".to_string())]
                .into_iter()
                .collect(),
        )
        .unwrap();
    assert!((frame_of(&d, p).origin.z - 14.0).abs() < EPS);
    // Negative Abstände sind erlaubt (Ebene auf der anderen Seite).
    d.add_parameter("b", "-3", ParamUnit::Length, "").unwrap();
    d.update_with_formulas(
        p,
        |_| {},
        [(FieldKey::Abstand, "b".to_string())].into_iter().collect(),
    )
    .unwrap();
    assert!((frame_of(&d, p).origin.z + 3.0).abs() < EPS);
    // Winkel als Formelfeld der Ebene unter Winkel.
    let angle = plane(PlaneDef::Angle {
        axis: AxisRef::Standard(StdAxis::Z),
        angle: 0.0,
    });
    assert_eq!(field_keys(&angle), vec![FieldKey::Winkel]);
    assert_eq!(field_value(&angle, FieldKey::Winkel), Some(0.0));
}

/// Review Focus 4: Löschen nimmt Abhängige mit. Skizzen auf Ebenen (`OnPlane`) kommen in
/// Task 6; hier hängen eine zweite Ebene und eine Mittelebene an der ersten, und eine Ebene
/// durch Skizzenpunkte hängt an der Skizze.
#[test]
fn deleting_plane_lists_dependents() {
    let mut d = Document::new();
    let p1 = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Offset {
                base: PlaneBase::Standard(StandardPlane::XY),
                distance: 10.0,
            }),
        )
        .unwrap();
    let p2 = d
        .add_feature(
            "Ebene2",
            plane(PlaneDef::Offset {
                base: PlaneBase::Plane(p1),
                distance: 5.0,
            }),
        )
        .unwrap();
    assert!((frame_of(&d, p2).origin.z - 15.0).abs() < EPS);
    let p3 = d
        .add_feature(
            "Ebene3",
            plane(PlaneDef::Mid {
                a: PlaneBase::Plane(p2),
                b: PlaneBase::Standard(StandardPlane::XY),
            }),
        )
        .unwrap();
    assert!((frame_of(&d, p3).origin.z - 7.5).abs() < EPS);
    assert_eq!(
        d.delete(p1),
        Err(FeatureError::HasDependents(vec![p2.0, p3.0]))
    );
    assert_eq!(d.features().len(), 3, "nichts gelöscht");
    assert_eq!(d.delete_with_dependents(p1).unwrap(), vec![p1, p2, p3]);
    assert!(d.features().is_empty());
    d.check_invariants().unwrap();

    // Skizze → Ebene durch Skizzenpunkte → Löschen der Skizze nennt die Ebene.
    let tol = Tolerance::default();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", tol);
    let a = sk.add_point(5.0, 0.0).unwrap();
    let b = sk.add_point(0.0, 5.0).unwrap();
    let s = d
        .add_feature(
            "Skizze1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::ThreePoints([
                PointRef::Origin,
                PointRef::SketchPoint {
                    sketch: s,
                    point: a,
                },
                PointRef::SketchPoint {
                    sketch: s,
                    point: b,
                },
            ])),
        )
        .unwrap();
    assert_near(frame_of(&d, p).z, DVec3::Z);
    assert_eq!(d.delete(s), Err(FeatureError::HasDependents(vec![p.0])));
    assert_eq!(d.delete_with_dependents(s).unwrap(), vec![s, p]);
    d.check_invariants().unwrap();
}

#[test]
fn a_plane_never_becomes_a_body_tip() {
    let mut d = Document::new();
    let p = d
        .add_feature(
            "Ebene1",
            plane(PlaneDef::Offset {
                base: PlaneBase::Standard(StandardPlane::XY),
                distance: 1.0,
            }),
        )
        .unwrap();
    assert!(!d.body_tips().contains(&p));
}
