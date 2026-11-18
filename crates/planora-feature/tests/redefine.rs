//! Skizzen 6, Task 6: Skizzen auf Konstruktionsebenen und „Skizzenebene neu festlegen“ (wie
//! Fusion). Die 2D-Geometrie bleibt, nur das Koordinatensystem wechselt; Verweise dürfen nur
//! nach hinten zeigen.

use planora_feature::planes::{PlaneBase, PlaneDef, PlaneFeature};
use planora_feature::*;
use planora_kernel::{BoolOp, Solid};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::{CurveId, CurveKind, Sketch};

const EPS: f64 = 1e-9;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn add_box(d: &mut Document, name: &str, size: DVec3, origin: DVec3) -> FeatureId {
    d.add_feature(name, FeatureKind::Box { size, origin })
        .unwrap()
}

fn offset_plane(d: &mut Document, base: PlaneBase, distance: f64) -> FeatureId {
    d.add_feature(
        "Ebene1",
        FeatureKind::Plane(PlaneFeature {
            def: PlaneDef::Offset { base, distance },
        }),
    )
    .unwrap()
}

fn set_offset(d: &mut Document, p: FeatureId, value: f64) {
    d.update(p, |k| {
        if let FeatureKind::Plane(PlaneFeature {
            def: PlaneDef::Offset { distance, .. },
        }) = k
        {
            *distance = value;
        }
    })
    .unwrap();
}

/// Skizze mit einem Rechteck (0,0)–(4,3) auf `plane`.
fn add_rect_sketch(d: &mut Document, plane: SketchPlane) -> FeatureId {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", tol());
    sk.add_rectangle(DVec2::ZERO, DVec2::new(4.0, 3.0)).unwrap();
    d.add_feature(
        "Skizze1",
        FeatureKind::Sketch(SketchFeature {
            sketch: sk,
            plane,
            copies: vec![],
            formulas: Default::default(),
            text_formulas: Default::default(),
        }),
    )
    .unwrap()
}

fn extrude(d: &mut Document, s: FeatureId) -> FeatureId {
    let e = d
        .add_feature(
            "Extrusion1",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Index(0),
                height: 2.0,
            },
        )
        .unwrap();
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    e
}

fn body(d: &Document, feature: FeatureId) -> &Solid {
    d.body(BodyRef { feature, index: 0 }).unwrap()
}

/// Kleinste und größte z-Koordinate der Ecken.
fn z_range(s: &Solid) -> (f64, f64) {
    s.vertices
        .iter()
        .map(|(_, v)| v.point.z)
        .fold((f64::MAX, f64::MIN), |(lo, hi), z| (lo.min(z), hi.max(z)))
}

/// Ebene Fläche mit der äußeren Normale `n`.
fn face_with_normal(s: &Solid, n: DVec3) -> FaceRef {
    let (id, _) = s
        .faces
        .iter()
        .find(|(f, _)| planora_sketch::plane_of_face(s, *f).is_some_and(|fr| fr.z.dot(n) > 0.999))
        .expect("Fläche fehlt");
    FaceRef::from_face(s, id, &tol()).unwrap()
}

fn sketch_def(d: &Document, s: FeatureId) -> &SketchFeature {
    match &d.feature(s).unwrap().kind {
        FeatureKind::Sketch(sf) => sf,
        k => panic!("keine Skizze: {k:?}"),
    }
}

fn assert_ok(d: &Document, id: FeatureId) {
    let f = d.feature(id).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{}: {:?}", f.name, f.status);
}

#[test]
fn sketch_on_offset_plane_follows_distance() {
    let mut d = Document::new();
    let p = offset_plane(&mut d, PlaneBase::Standard(StandardPlane::XY), 10.0);
    let s = add_rect_sketch(&mut d, SketchPlane::OnPlane(p));
    assert_ok(&d, s);
    assert_eq!(d.feature(s).unwrap().kind.dependencies(), vec![p]);
    assert!((d.sketch_of(s).unwrap().frame().origin.z - 10.0).abs() < EPS);
    let e = extrude(&mut d, s);
    let (lo, hi) = z_range(body(&d, e));
    assert!(
        (lo - 10.0).abs() < EPS && (hi - 12.0).abs() < EPS,
        "{lo} {hi}"
    );

    set_offset(&mut d, p, 25.0);
    assert_ok(&d, s);
    assert_ok(&d, e);
    assert!((d.sketch_of(s).unwrap().frame().origin.z - 25.0).abs() < EPS);
    let (lo, hi) = z_range(body(&d, e));
    assert!(
        (lo - 25.0).abs() < EPS && (hi - 27.0).abs() < EPS,
        "{lo} {hi}"
    );
    // Eine unterdrückte Ebene hält die Skizze an (wie eine fehlende Fläche).
    d.set_suppressed(p, true).unwrap();
    assert!(matches!(
        d.feature(s).unwrap().status,
        FeatureStatus::Skipped(_)
    ));
    d.check_invariants().unwrap();
}

#[test]
fn redefine_keeps_2d_geometry() {
    let mut d = Document::new();
    let p = offset_plane(&mut d, PlaneBase::Standard(StandardPlane::XZ), 7.0);
    let s = add_rect_sketch(&mut d, SketchPlane::Standard(StandardPlane::XY));
    let before: Vec<(u64, u64)> = {
        let sk = d.sketch_of(s).unwrap();
        sk.points()
            .map(|(id, _)| {
                let q = sk.point_xy(id).unwrap();
                (q.x.to_bits(), q.y.to_bits())
            })
            .collect()
    };
    assert_eq!(before.len(), 4);
    d.set_sketch_plane(s, SketchPlane::OnPlane(p)).unwrap();
    assert_ok(&d, s);
    assert_eq!(sketch_def(&d, s).plane, SketchPlane::OnPlane(p));
    let sk = d.sketch_of(s).unwrap();
    let after: Vec<(u64, u64)> = sk
        .points()
        .map(|(id, _)| {
            let q = sk.point_xy(id).unwrap();
            (q.x.to_bits(), q.y.to_bits())
        })
        .collect();
    assert_eq!(before, after, "Punkte in Skizzenkoordinaten bitgleich");
    // Frame neu: XZ-Ebene um 7 entlang ihrer Normalen (−Y) verschoben.
    let f = sk.frame();
    assert!(
        f.origin.distance(DVec3::new(0.0, -7.0, 0.0)) < EPS,
        "{}",
        f.origin
    );
    assert!(f.z.distance(-DVec3::Y) < EPS);
    d.check_invariants().unwrap();
}

#[test]
fn redefine_reprojects_copies() {
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::new(10.0, 20.0, 5.0), DVec3::ZERO);
    let p = offset_plane(&mut d, PlaneBase::Standard(StandardPlane::XZ), 7.0);
    // Kante entlang x bei y = 20, z = 5, verknüpft auf XY projiziert.
    let solid = body(&d, b).clone();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", tol());
    let mut found: Option<(CurveId, EdgeRef)> = None;
    for (id, e) in solid.edges.iter() {
        let (a, q) = (solid.vertices[e.start].point, solid.vertices[e.end].point);
        let along_x = (a.y - 20.0).abs() < EPS
            && (q.y - 20.0).abs() < EPS
            && (a.z - 5.0).abs() < EPS
            && (q.z - 5.0).abs() < EPS;
        if along_x {
            let c = sk
                .add_projected_curve(
                    &e.curve,
                    e.t0,
                    e.t1,
                    ProjectStyle {
                        construction: false,
                    },
                )
                .unwrap();
            found = Some((c, EdgeRef::from_edge(&solid, id, &tol()).unwrap()));
            break;
        }
    }
    let (curve, edge) = found.expect("Kante fehlt");
    let s = d
        .add_feature(
            "Skizze1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![EdgeCopy {
                    target: planora_feature::CopyTarget::Curve(curve),
                    piece: 0,
                    of: 1,
                    source: CopySource::Edge(edge),
                }],
                formulas: Default::default(),
                text_formulas: Default::default(),
            }),
        )
        .unwrap();
    assert_ok(&d, s);
    let ends = |d: &Document| {
        let sk = d.sketch_of(s).unwrap();
        let CurveKind::Line { a, b } = sk.curve(curve).unwrap().kind else {
            panic!("keine Linie");
        };
        (sk.point_xy(a).unwrap(), sk.point_xy(b).unwrap())
    };
    let (a, q) = ends(&d);
    assert!(
        (a.y - 20.0).abs() < EPS && (q.y - 20.0).abs() < EPS,
        "{a} {q}"
    );

    d.set_sketch_plane(s, SketchPlane::OnPlane(p)).unwrap();
    assert_ok(&d, s);
    assert!(d.feature(s).unwrap().warning().is_none(), "Bezug bleibt");
    // Auf der XZ-Ebene ist die lokale y-Achse die Welt-z-Achse: die Kante liegt bei y = 5.
    let (a, q) = ends(&d);
    assert!(
        (a.y - 5.0).abs() < EPS && (q.y - 5.0).abs() < EPS,
        "{a} {q}"
    );
    assert!(((a.x - q.x).abs() - 10.0).abs() < EPS, "{a} {q}");
    d.check_invariants().unwrap();
}

/// Review Focus 5: Eine Fläche eines späteren Features ist keine gültige Ebene; das Dokument
/// bleibt unverändert.
#[test]
fn redefine_to_later_face_is_rejected() {
    let mut d = Document::new();
    let s = add_rect_sketch(&mut d, SketchPlane::Standard(StandardPlane::XY));
    let b = add_box(&mut d, "Quader", DVec3::new(10.0, 10.0, 10.0), DVec3::ZERO);
    let later_plane = offset_plane(&mut d, PlaneBase::Standard(StandardPlane::XY), 3.0);
    let top = face_with_normal(body(&d, b), DVec3::Z);
    let edits = d.edit_count();
    let before = sketch_def(&d, s).clone();
    assert_eq!(
        d.set_sketch_plane(s, SketchPlane::OnFace(top)),
        Err(FeatureError::ForwardReference)
    );
    assert_eq!(
        d.set_sketch_plane(s, SketchPlane::OnPlane(later_plane)),
        Err(FeatureError::ForwardReference)
    );
    assert_eq!(d.edit_count(), edits);
    assert_eq!(sketch_def(&d, s), &before);
    assert_eq!(
        FeatureError::ForwardReference.to_string(),
        "Ein Feature darf nur auf Features verweisen, die vor ihm stehen"
    );
    d.check_invariants().unwrap();
}

/// Review Focus 5: Die Deckfläche der eigenen Extrusion würde eine Schleife bilden.
#[test]
fn redefine_to_own_extrusion_face_is_rejected() {
    let mut d = Document::new();
    let s = add_rect_sketch(&mut d, SketchPlane::Standard(StandardPlane::XY));
    let e = extrude(&mut d, s);
    let top = face_with_normal(body(&d, e), DVec3::Z);
    let edits = d.edit_count();
    let before = sketch_def(&d, s).clone();
    assert_eq!(
        d.set_sketch_plane(s, SketchPlane::OnFace(top)),
        Err(FeatureError::ForwardReference)
    );
    assert_eq!(d.edit_count(), edits);
    assert_eq!(sketch_def(&d, s), &before);
    assert_ok(&d, s);
    assert_ok(&d, e);
    d.check_invariants().unwrap();
}

#[test]
fn redefine_is_one_edit() {
    let mut d = Document::new();
    let p = offset_plane(&mut d, PlaneBase::Standard(StandardPlane::XY), 4.0);
    let s = add_rect_sketch(&mut d, SketchPlane::Standard(StandardPlane::XY));
    let e = extrude(&mut d, s);
    let edits = d.edit_count();
    d.set_sketch_plane(s, SketchPlane::OnPlane(p)).unwrap();
    assert_eq!(d.edit_count(), edits + 1, "ein Rückgängig-Schritt");
    assert_ok(&d, e);
    let (lo, hi) = z_range(body(&d, e));
    assert!(
        (lo - 4.0).abs() < EPS && (hi - 6.0).abs() < EPS,
        "{lo} {hi}"
    );
    // Dieselbe Ebene noch einmal ändert nichts.
    d.set_sketch_plane(s, SketchPlane::OnPlane(p)).unwrap();
    assert_eq!(d.edit_count(), edits + 1);
    // Kein Feature „Ebene“: abgelehnt, nichts geändert.
    let b = d.features()[0].id;
    let q = add_box(&mut d, "Quader", DVec3::ONE, DVec3::ZERO);
    let edits = d.edit_count();
    assert!(matches!(
        d.set_sketch_plane(s, SketchPlane::OnPlane(q)),
        Err(FeatureError::InvalidInput(_))
    ));
    // Keine Skizze: abgelehnt.
    assert!(matches!(
        d.set_sketch_plane(b, SketchPlane::Standard(StandardPlane::XZ)),
        Err(FeatureError::InvalidInput(_))
    ));
    assert_eq!(d.edit_count(), edits);
    d.check_invariants().unwrap();
}

/// Aus Task 5 übernommen: Löschen einer Ebene nennt die Skizze darauf und nimmt mit
/// `delete_with_dependents` Skizze und Extrusion mit.
#[test]
fn deleting_plane_with_sketch_and_extrusion() {
    let mut d = Document::new();
    let p = offset_plane(&mut d, PlaneBase::Standard(StandardPlane::XY), 10.0);
    let s = add_rect_sketch(&mut d, SketchPlane::OnPlane(p));
    let e = extrude(&mut d, s);
    assert_eq!(
        d.delete(p),
        Err(FeatureError::HasDependents(vec![s.0, e.0]))
    );
    assert_eq!(d.features().len(), 3, "nichts gelöscht");
    assert_eq!(d.delete_with_dependents(p).unwrap(), vec![p, s, e]);
    assert!(d.features().is_empty());
    d.check_invariants().unwrap();
}

/// Aus Task 5 übernommen: Verschwindet die Fläche unter einer Versatzebene, ist die Ebene ein
/// Fehler ohne Frame, und die Skizze darauf wird übersprungen.
#[test]
fn lost_plane_reference_is_error() {
    let mut d = Document::new();
    let a = add_box(&mut d, "A", DVec3::new(20.0, 20.0, 10.0), DVec3::ZERO);
    // Werkzeug schneidet zuerst nur unten ab; die Deckfläche von A bleibt.
    let t = add_box(
        &mut d,
        "B",
        DVec3::new(30.0, 30.0, 7.0),
        DVec3::new(-5.0, -5.0, -5.0),
    );
    let cut = d
        .add_feature(
            "Differenz",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: a,
                    index: 0,
                },
                tool: BodyRef {
                    feature: t,
                    index: 0,
                },
                op: BoolOp::Difference,
            },
        )
        .unwrap();
    assert_ok(&d, cut);
    let top = face_with_normal(body(&d, cut), DVec3::Z);
    assert_eq!(top.origin, a.0, "Deckfläche stammt von A");
    let p = offset_plane(&mut d, PlaneBase::Face(top), 2.0);
    assert!((d.plane_frame(p).unwrap().origin.z - 12.0).abs() < EPS);
    let s = add_rect_sketch(&mut d, SketchPlane::OnPlane(p));
    assert_ok(&d, s);

    // Das Werkzeug wandert nach oben und nimmt die Deckfläche von A weg.
    d.update(t, |k| {
        if let FeatureKind::Box { origin, .. } = k {
            origin.z = 5.0;
        }
    })
    .unwrap();
    assert_ok(&d, cut);
    assert!(matches!(
        d.feature(p).unwrap().status,
        FeatureStatus::Error(_)
    ));
    assert_eq!(d.plane_frame(p), None);
    assert!(matches!(
        d.feature(s).unwrap().status,
        FeatureStatus::Skipped(_)
    ));
    d.check_invariants().unwrap();
}
