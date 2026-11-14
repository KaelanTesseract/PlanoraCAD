//! Skizzen 10, Task 17: Rechnung „Auf Fläche projizieren“ (Spec 6). Kurven, Kanten und Ecken
//! werden auf Körperflächen oder Konstruktionsebenen geworfen, nach dem nächsten Punkt oder
//! entlang eines Vektors; das Ergebnis steht in Skizzenkoordinaten.

use planora_feature::surface_project::ProjectError;
use planora_feature::*;
use planora_kernel::{BoolOp, Solid, SurfaceKind};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::helpers::Included;
use planora_sketch::model::{CurveId, Sketch};
use std::time::Duration;

fn tol() -> Tolerance {
    Tolerance {
        linear: 1e-4,
        ..Tolerance::default()
    }
}

fn body(d: &Document, feature: FeatureId) -> Solid {
    d.body(BodyRef { feature, index: 0 }).unwrap().clone()
}

/// Skizze auf XY mit einer Linie von `a` nach `b`.
fn line_sketch(d: &mut Document, a: (f64, f64), b: (f64, f64)) -> (FeatureId, CurveId) {
    let mut sk = Sketch::new(Frame::WORLD, "Quelle", Tolerance::default());
    let p = sk.add_point(a.0, a.1).unwrap();
    let q = sk.add_point(b.0, b.1).unwrap();
    let c = sk.add_line(p, q).unwrap();
    (add_sketch(d, sk), c)
}

fn add_sketch(d: &mut Document, sk: Sketch) -> FeatureId {
    d.add_feature(
        "Quelle",
        FeatureKind::Sketch(SketchFeature {
            sketch: sk,
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas: Default::default(),
        }),
    )
    .unwrap()
}

/// Alle Flächen der Art `kind` des Körpers als Ziele.
fn faces_of(solid: &Solid, kind: SurfaceKind) -> Vec<FaceTarget> {
    solid
        .faces
        .iter()
        .filter(|(_, f)| f.surface.kind() == kind)
        .map(|(id, _)| {
            FaceTarget::Face(FaceRef::from_face(solid, id, &Tolerance::default()).unwrap())
        })
        .collect()
}

fn run(d: &Document, spec: &SurfaceProjection) -> Result<Vec<Included>, ProjectError> {
    project_to_surface(
        &DocView::new(d, d.features().len(), Frame::WORLD),
        spec,
        &tol(),
        SURFACE_PROJECT_BUDGET,
    )
}

/// Punkte entlang eines Ergebnisstücks (Spline oder Linie).
fn samples(g: &Included) -> Vec<DVec3> {
    match g {
        Included::Spline { curve } => {
            let (a, b) = curve.domain();
            (0..=200)
                .map(|i| curve.point(a + (b - a) * i as f64 / 200.0))
                .collect()
        }
        Included::Line(a, b) => (0..=20).map(|i| a.lerp(*b, i as f64 / 20.0)).collect(),
        Included::Point(p) => vec![*p],
        g => panic!("unerwartet: {g:?}"),
    }
}

fn ends(g: &Included) -> (DVec3, DVec3) {
    let s = samples(g);
    (s[0], s[s.len() - 1])
}

#[test]
fn line_closest_onto_cylinder_follows_radius() {
    let mut d = Document::new();
    let cyl = d
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: 5.0,
                height: 20.0,
                origin: DVec3::new(0.0, 0.0, -10.0),
            },
        )
        .unwrap();
    let (s, c) = line_sketch(&mut d, (-3.0, 8.0), (3.0, 8.0));
    let spec = SurfaceProjection {
        sources: vec![CurveSource::SketchCurve {
            sketch: s,
            curve: c,
        }],
        faces: faces_of(&body(&d, cyl), SurfaceKind::Cylinder),
        kind: ProjectType::ClosestPoint,
    };
    let got = run(&d, &spec).unwrap();
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(matches!(got[0], Included::Spline { .. }), "{got:?}");
    for p in samples(&got[0]) {
        let r = (p.x * p.x + p.y * p.y).sqrt();
        assert!((r - 5.0).abs() <= tol().linear, "Radius {r} bei {p:?}");
        assert!(p.z.abs() <= tol().linear, "Höhe {p:?}");
    }
    let (a, b) = ends(&got[0]);
    let want = |x: f64| DVec3::new(x, 8.0, 0.0).normalize() * 5.0;
    assert!(a.distance(want(-3.0)) < 1e-6, "{a:?}");
    assert!(b.distance(want(3.0)) < 1e-6, "{b:?}");
}

#[test]
fn along_vector_onto_sphere() {
    let mut d = Document::new();
    let sph = d
        .add_feature(
            "Kugel",
            FeatureKind::Sphere {
                radius: 10.0,
                origin: DVec3::new(0.0, 0.0, -3.0),
            },
        )
        .unwrap();
    let mut sk = Sketch::new(Frame::WORLD, "Quelle", Tolerance::default());
    let m = sk.add_point(0.0, 0.0).unwrap();
    let circle = sk.add_circle(m, 5.0).unwrap();
    let s = add_sketch(&mut d, sk);
    let solid = body(&d, sph);
    let spec = SurfaceProjection {
        sources: vec![CurveSource::SketchCurve {
            sketch: s,
            curve: circle,
        }],
        faces: faces_of(&solid, SurfaceKind::Sphere),
        kind: ProjectType::AlongVector(DVec3::Z),
    };
    let got = run(&d, &spec).unwrap();
    assert!(!got.is_empty());
    let h = -3.0 + 75f64.sqrt();
    for g in &got {
        for p in samples(g) {
            let r = (p.x * p.x + p.y * p.y).sqrt();
            assert!((r - 5.0).abs() <= tol().linear, "Radius {r} bei {p:?}");
            assert!((p.z - h).abs() <= tol().linear, "Höhe {p:?}");
        }
    }
    // Der ganze Kreis trifft die obere Kugelhälfte: ein geschlossenes Stück.
    assert_eq!(got.len(), 1, "{got:?}");
    let (a, b) = ends(&got[0]);
    assert!(a.distance(b) < 1e-6, "geschlossen: {a:?} {b:?}");
}

#[test]
fn plane_target_along_normal_is_exact_line() {
    let mut d = Document::new();
    let (s, c) = line_sketch(&mut d, (2.0, 1.0), (10.0, 5.0));
    let spec = SurfaceProjection {
        sources: vec![CurveSource::SketchCurve {
            sketch: s,
            curve: c,
        }],
        faces: vec![FaceTarget::Plane(PlaneId::Standard(StandardPlane::YZ))],
        kind: ProjectType::AlongVector(DVec3::X),
    };
    let got = run(&d, &spec).unwrap();
    assert_eq!(got.len(), 1, "{got:?}");
    let Included::Line(a, b) = got[0] else {
        panic!("keine Linie: {got:?}");
    };
    assert!(a.distance(DVec3::new(0.0, 1.0, 0.0)) < 1e-12, "{a:?}");
    assert!(b.distance(DVec3::new(0.0, 5.0, 0.0)) < 1e-12, "{b:?}");
}

#[test]
fn face_with_hole_splits_curve() {
    let mut d = Document::new();
    let bx = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(20.0, 20.0, 5.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let cyl = d
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: 3.0,
                height: 9.0,
                origin: DVec3::new(10.0, 10.0, -2.0),
            },
        )
        .unwrap();
    let cut = d
        .add_feature(
            "Loch",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: bx,
                    index: 0,
                },
                tool: BodyRef {
                    feature: cyl,
                    index: 0,
                },
                op: BoolOp::Difference,
            },
        )
        .unwrap();
    let solid = body(&d, cut);
    let (top, _) = solid
        .faces
        .iter()
        .find(|(f, _)| {
            planora_sketch::plane_of_face(&solid, *f)
                .is_some_and(|fr| fr.z.z > 0.5 && (fr.origin.z - 5.0).abs() < 1e-9)
        })
        .unwrap();
    let top = FaceRef::from_face(&solid, top, &Tolerance::default()).unwrap();
    let (s, c) = line_sketch(&mut d, (1.0, 10.0), (19.0, 10.0));
    for kind in [
        ProjectType::ClosestPoint,
        ProjectType::AlongVector(DVec3::Z),
    ] {
        let spec = SurfaceProjection {
            sources: vec![CurveSource::SketchCurve {
                sketch: s,
                curve: c,
            }],
            faces: vec![FaceTarget::Face(top.clone())],
            kind,
        };
        let got = run(&d, &spec).unwrap();
        assert_eq!(got.len(), 2, "{got:?}");
        let (a0, a1) = ends(&got[0]);
        let (b0, b1) = ends(&got[1]);
        assert!(a0.distance(DVec3::new(1.0, 10.0, 5.0)) < 1e-6, "{a0:?}");
        assert!(a1.distance(DVec3::new(7.0, 10.0, 5.0)) < 1e-3, "{a1:?}");
        assert!(b0.distance(DVec3::new(13.0, 10.0, 5.0)) < 1e-3, "{b0:?}");
        assert!(b1.distance(DVec3::new(19.0, 10.0, 5.0)) < 1e-6, "{b1:?}");
        for p in samples(&got[0]).into_iter().chain(samples(&got[1])) {
            assert!((p.z - 5.0).abs() <= tol().linear, "{p:?}");
            assert!((p.y - 10.0).abs() <= tol().linear, "{p:?}");
        }
    }
}

#[test]
fn curve_missing_face_is_nohit() {
    let mut d = Document::new();
    let cyl = d
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: 5.0,
                height: 20.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let (s, c) = line_sketch(&mut d, (20.0, 0.0), (30.0, 0.0));
    let solid = body(&d, cyl);
    let spec = SurfaceProjection {
        sources: vec![CurveSource::SketchCurve {
            sketch: s,
            curve: c,
        }],
        faces: faces_of(&solid, SurfaceKind::Cylinder)
            .into_iter()
            .chain(faces_of(&solid, SurfaceKind::Plane))
            .collect(),
        kind: ProjectType::AlongVector(DVec3::Z),
    };
    assert_eq!(run(&d, &spec), Err(ProjectError::NoHit));
}

#[test]
fn budget_exceeded_is_tooslow_and_returns_nothing() {
    let mut d = Document::new();
    let sph = d
        .add_feature(
            "Kugel",
            FeatureKind::Sphere {
                radius: 10.0,
                origin: DVec3::new(0.0, 0.0, -3.0),
            },
        )
        .unwrap();
    let mut sk = Sketch::new(Frame::WORLD, "Quelle", Tolerance::default());
    let m = sk.add_point(0.0, 0.0).unwrap();
    let circle = sk.add_circle(m, 5.0).unwrap();
    let s = add_sketch(&mut d, sk);
    let spec = SurfaceProjection {
        sources: vec![CurveSource::SketchCurve {
            sketch: s,
            curve: circle,
        }],
        faces: faces_of(&body(&d, sph), SurfaceKind::Sphere),
        kind: ProjectType::ClosestPoint,
    };
    let got = project_to_surface(
        &DocView::new(&d, d.features().len(), Frame::WORLD),
        &spec,
        &Tolerance::default(),
        Duration::from_millis(1),
    );
    assert_eq!(got, Err(ProjectError::TooSlow));
    assert_eq!(SURFACE_PROJECT_BUDGET, Duration::from_secs(2));
    assert_eq!(MAX_SURFACE_SAMPLES, 2000);
}

/// Drehkörper aus einem Spline-Profil um die y-Achse (Freiformfläche), wie in den
/// NURBS-Feature-Tests.
fn spline_revolve(d: &mut Document) -> FeatureId {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let ap = sk.add_point(0.0, -2.0).unwrap();
    let aq = sk.add_point(0.0, 8.0).unwrap();
    let axis = sk.add_line(ap, aq).unwrap();
    sk.set_construction(axis, true).unwrap();
    let p: Vec<_> = [
        (0.0, 0.0),
        (0.0, 5.0),
        (2.0, 5.0),
        (3.0, 3.0),
        (2.5, 1.0),
        (2.0, 0.0),
    ]
    .iter()
    .map(|&(x, y)| sk.add_point(x, y).unwrap())
    .collect();
    sk.add_line(p[0], p[1]).unwrap();
    sk.add_line(p[1], p[2]).unwrap();
    sk.add_spline(&p[2..6]).unwrap();
    sk.add_line(p[5], p[0]).unwrap();
    let s = add_sketch(d, sk);
    d.add_feature(
        "Rotation",
        FeatureKind::Revolve {
            sketch: s,
            profiles: ProfileSel::Index(0),
            axis,
        },
    )
    .unwrap()
}

#[test]
fn freeform_face_no_panic() {
    let mut d = Document::new();
    let rev = spline_revolve(&mut d);
    let solid = body(&d, rev);
    let faces = faces_of(&solid, SurfaceKind::Nurbs);
    assert!(!faces.is_empty());
    let (s, c) = line_sketch(&mut d, (5.0, 1.0), (5.0, 4.0));
    for kind in [
        ProjectType::ClosestPoint,
        ProjectType::AlongVector(-DVec3::X),
    ] {
        let spec = SurfaceProjection {
            sources: vec![CurveSource::SketchCurve {
                sketch: s,
                curve: c,
            }],
            faces: faces.clone(),
            kind,
        };
        match run(&d, &spec) {
            Ok(got) => {
                for g in &got {
                    for p in samples(g) {
                        assert!(p.is_finite(), "{p:?}");
                        // Die Treffer liegen auf einer der Freiformflächen.
                        let gap = solid
                            .faces
                            .iter()
                            .filter(|(_, f)| f.surface.kind() == SurfaceKind::Nurbs)
                            .map(|(_, f)| f.surface.implicit(p).abs())
                            .fold(f64::INFINITY, f64::min);
                        assert!(gap <= tol().linear, "Abstand {gap} bei {p:?}");
                    }
                }
            }
            Err(e) => assert_eq!(e, ProjectError::NoHit),
        }
    }
}

#[test]
fn point_source_gives_point() {
    let mut d = Document::new();
    let bx = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(2.0, 3.0, 4.0),
                origin: DVec3::new(1.0, 1.0, 5.0),
            },
        )
        .unwrap();
    let solid = body(&d, bx);
    let (edge, e) = solid.edges.iter().next().unwrap();
    let corner = solid.vertices[e.end].point;
    let spec = SurfaceProjection {
        sources: vec![CurveSource::Vertex(VertexRef {
            edge: EdgeRef::from_edge(&solid, edge, &Tolerance::default()).unwrap(),
            end: true,
        })],
        faces: vec![FaceTarget::Plane(PlaneId::Standard(StandardPlane::XY))],
        kind: ProjectType::ClosestPoint,
    };
    let got = run(&d, &spec).unwrap();
    assert_eq!(
        got,
        vec![Included::Point(DVec3::new(corner.x, corner.y, 0.0))]
    );
}

#[test]
fn lost_source_is_lost() {
    let d = Document::new();
    let spec = SurfaceProjection {
        sources: vec![CurveSource::ConstructionPoint(FeatureId(99))],
        faces: vec![FaceTarget::Plane(PlaneId::Standard(StandardPlane::XY))],
        kind: ProjectType::ClosestPoint,
    };
    assert_eq!(run(&d, &spec), Err(ProjectError::Lost));
}

/// Fix-Runde 1: Auch eine Annäherung, die die Zeitgrenze überschreitet, ergibt `TooSlow` und
/// nichts (der Test verlangsamt jede Annäherung künstlich).
#[test]
fn budget_exceeded_in_fit_is_tooslow() {
    let mut d = Document::new();
    let cyl = d
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: 5.0,
                height: 20.0,
                origin: DVec3::new(0.0, 0.0, -10.0),
            },
        )
        .unwrap();
    let (s, c) = line_sketch(&mut d, (-3.0, 8.0), (3.0, 8.0));
    let spec = SurfaceProjection {
        sources: vec![CurveSource::SketchCurve {
            sketch: s,
            curve: c,
        }],
        faces: faces_of(&body(&d, cyl), SurfaceKind::Cylinder),
        kind: ProjectType::ClosestPoint,
    };
    let view = DocView::new(&d, d.features().len(), Frame::WORLD);
    // Ohne Verzögerung reicht eine Sekunde sicher.
    assert!(project_to_surface(&view, &spec, &tol(), Duration::from_secs(1)).is_ok());
    let _slow = surface_project::slow_fit_for_tests(Duration::from_millis(1200));
    assert_eq!(
        project_to_surface(&view, &spec, &tol(), Duration::from_secs(1)),
        Err(ProjectError::TooSlow)
    );
}

/// Fix-Runde 1: Treffer jenseits des Rechenbereichs (Richtung fast parallel zur Ebene) zählen
/// nicht, im allgemeinen Weg wie im exakten Sonderfall.
#[test]
fn far_hits_are_dropped() {
    let mut d = Document::new();
    let bx = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(2.0, 3.0, 4.0),
                origin: DVec3::new(1.0, 1.0, 5.0),
            },
        )
        .unwrap();
    let solid = body(&d, bx);
    let (edge, _) = solid
        .edges
        .iter()
        .find(|(_, e)| solid.vertices[e.start].point.z > 8.9 && solid.vertices[e.end].point.z > 8.9)
        .unwrap();
    let edge = EdgeRef::from_edge(&solid, edge, &Tolerance::default()).unwrap();
    let along = ProjectType::AlongVector(DVec3::new(1.0, 0.0, 2e-9));
    for source in [
        CurveSource::Vertex(VertexRef {
            edge: edge.clone(),
            end: true,
        }),
        CurveSource::Edge(edge.clone()),
    ] {
        let spec = SurfaceProjection {
            sources: vec![source],
            faces: vec![FaceTarget::Plane(PlaneId::Standard(StandardPlane::XY))],
            kind: along.clone(),
        };
        assert_eq!(run(&d, &spec), Err(ProjectError::NoHit));
    }
}
