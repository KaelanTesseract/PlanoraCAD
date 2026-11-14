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

// --- Task 18: Schnittkurve (Spec 7) und Neuberechnung beider (Spec 10) ---

fn add_sketch_on(d: &mut Document, sk: Sketch, plane: StandardPlane) -> FeatureId {
    d.add_feature(
        "Skizze",
        FeatureKind::Sketch(SketchFeature {
            sketch: sk,
            plane: SketchPlane::Standard(plane),
            copies: vec![],
            formulas: Default::default(),
        }),
    )
    .unwrap()
}

fn run_intersection(d: &Document, spec: &IntersectionSpec) -> Result<Vec<Included>, ProjectError> {
    intersection_curve(
        &DocView::new(d, d.features().len(), Frame::WORLD),
        spec,
        &tol(),
        SURFACE_PROJECT_BUDGET,
    )
}

/// Die Deckfläche (Normale +z, oben) des Körpers.
fn top_face(solid: &Solid) -> FaceRef {
    let hi = solid
        .vertices
        .iter()
        .map(|(_, v)| v.point.z)
        .fold(f64::MIN, f64::max);
    let (id, _) = solid
        .faces
        .iter()
        .find(|(f, _)| {
            planora_sketch::plane_of_face(solid, *f)
                .is_some_and(|fr| fr.z.z > 0.5 && (fr.origin.z - hi).abs() < 1e-9)
        })
        .unwrap();
    FaceRef::from_face(solid, id, &Tolerance::default()).unwrap()
}

#[test]
fn intersection_with_face_equals_along_sketch_normal() {
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
    // Quelle auf der Vorderebene (Normale -y): Linie quer durch die Kugel.
    let mut sk = Sketch::new(StandardPlane::XZ.frame(), "Quelle", Tolerance::default());
    let p = sk.add_point(-4.0, -2.0).unwrap();
    let q = sk.add_point(5.0, 1.0).unwrap();
    let line = sk.add_line(p, q).unwrap();
    let s = add_sketch_on(&mut d, sk, StandardPlane::XZ);
    let faces = faces_of(&body(&d, sph), SurfaceKind::Sphere);
    let FaceTarget::Face(face) = faces[0].clone() else {
        unreachable!()
    };
    let source = CurveSource::SketchCurve {
        sketch: s,
        curve: line,
    };
    let got = run_intersection(
        &d,
        &IntersectionSpec {
            curves: vec![source.clone()],
            targets: vec![IntersectTarget::Face(face)],
        },
    )
    .unwrap();
    let normal = StandardPlane::XZ.frame().z;
    let want = run(
        &d,
        &SurfaceProjection {
            sources: vec![source],
            faces,
            kind: ProjectType::AlongVector(normal),
        },
    )
    .unwrap();
    assert!(!got.is_empty());
    assert_eq!(got, want);
    // Gegen eine Ebene quer zur Normale: die exakte Linie wie „Entlang Vektor“ (hier die Ebene
    // der Quelle selbst, also die Linie an ihrer Stelle).
    let plane = |p: StandardPlane| IntersectionSpec {
        curves: vec![CurveSource::SketchCurve {
            sketch: s,
            curve: line,
        }],
        targets: vec![IntersectTarget::Plane(PlaneId::Standard(p))],
    };
    let got = run_intersection(&d, &plane(StandardPlane::XZ)).unwrap();
    let [Included::Line(a, b)] = got.as_slice() else {
        panic!("keine Linie: {got:?}");
    };
    assert!(a.distance(DVec3::new(-4.0, 0.0, -2.0)) < 1e-12, "{a:?}");
    assert!(b.distance(DVec3::new(5.0, 0.0, 1.0)) < 1e-12, "{b:?}");
    // Eine Ebene, die die Normale enthält, trifft der Auszug nicht eindeutig.
    assert_eq!(
        run_intersection(&d, &plane(StandardPlane::XY)),
        Err(ProjectError::NoHit)
    );
}

#[test]
fn two_sketch_curves_from_top_and_front_make_3d_curve() {
    let mut d = Document::new();
    let mut top = Sketch::new(Frame::WORLD, "Oben", Tolerance::default());
    let m = top.add_point(0.0, 0.0).unwrap();
    let circle = top.add_circle(m, 10.0).unwrap();
    let s_top = add_sketch_on(&mut d, top, StandardPlane::XY);
    // Vorn (XZ: x nach X, y nach Z): die Linie z = x.
    let mut front = Sketch::new(StandardPlane::XZ.frame(), "Vorn", Tolerance::default());
    let a = front.add_point(-20.0, -20.0).unwrap();
    let b = front.add_point(20.0, 20.0).unwrap();
    let line = front.add_line(a, b).unwrap();
    let s_front = add_sketch_on(&mut d, front, StandardPlane::XZ);
    let spec = IntersectionSpec {
        curves: vec![CurveSource::SketchCurve {
            sketch: s_top,
            curve: circle,
        }],
        targets: vec![IntersectTarget::SketchCurve {
            sketch: s_front,
            curve: line,
        }],
    };
    let got = run_intersection(&d, &spec).unwrap();
    // Der ganze Kreis trifft: ein geschlossenes Stück, eine Ellipse im Raum.
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(matches!(got[0], Included::Spline { .. }), "{got:?}");
    let pts = samples(&got[0]);
    for p in &pts {
        let r = (p.x * p.x + p.y * p.y).sqrt();
        assert!((r - 10.0).abs() <= tol().linear, "Radius {r} bei {p:?}");
        assert!((p.z - p.x).abs() <= tol().linear, "z = x verletzt: {p:?}");
    }
    let (s, e) = ends(&got[0]);
    assert!(s.distance(e) < 1e-6, "geschlossen: {s:?} {e:?}");
    let zmax = pts.iter().map(|p| p.z).fold(f64::MIN, f64::max);
    assert!((zmax - 10.0).abs() < 1e-2, "{zmax}");
}

#[test]
fn curve_from_active_sketch_refused() {
    let mut d = Document::new();
    let mut top = Sketch::new(Frame::WORLD, "Oben", Tolerance::default());
    let m = top.add_point(0.0, 0.0).unwrap();
    let circle = top.add_circle(m, 10.0).unwrap();
    let s_top = add_sketch_on(&mut d, top, StandardPlane::XY);
    let mut front = Sketch::new(StandardPlane::XZ.frame(), "Vorn", Tolerance::default());
    let a = front.add_point(-20.0, -20.0).unwrap();
    let b = front.add_point(20.0, 20.0).unwrap();
    let line = front.add_line(a, b).unwrap();
    let s_front = add_sketch_on(&mut d, front, StandardPlane::XZ);
    let spec = IntersectionSpec {
        curves: vec![CurveSource::SketchCurve {
            sketch: s_top,
            curve: circle,
        }],
        targets: vec![IntersectTarget::SketchCurve {
            sketch: s_front,
            curve: line,
        }],
    };
    let at = |i: usize| {
        intersection_curve(
            &DocView::new(&d, i, Frame::WORLD),
            &spec,
            &tol(),
            SURFACE_PROJECT_BUDGET,
        )
    };
    // Aktiv ist die Skizze „Vorn“ (Stelle 1): Ihre Kurve ist kein Ziel.
    assert_eq!(at(1), Err(ProjectError::Lost));
    // Aktiv ist „Oben“ (Stelle 0): Ihre Kurve ist keine Quelle.
    assert_eq!(at(0), Err(ProjectError::Lost));
    assert!(at(2).is_ok());
    // Nur Skizzenkurven sind Quellen der Schnittkurve.
    let not_a_curve = IntersectionSpec {
        curves: vec![CurveSource::ConstructionPoint(FeatureId(99))],
        targets: vec![IntersectTarget::Plane(PlaneId::Standard(StandardPlane::XY))],
    };
    assert_eq!(run_intersection(&d, &not_a_curve), Err(ProjectError::Lost));
}

/// Quader 20 × 20 × 5, Quellskizze mit der Linie (1, 10) – (19, 10) auf XY, Zielskizze auf XY mit
/// den Kopien der Quellen von `make` (je eine Gruppe). Liefert Dokument, Quader, Zielskizze und
/// die Kurven der Kopien.
fn recompute_doc(
    make: impl Fn(FeatureId, CurveId, FaceRef) -> Vec<CopySource>,
) -> (Document, FeatureId, FeatureId, Vec<CurveId>) {
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
    let (s, c) = line_sketch(&mut d, (1.0, 10.0), (19.0, 10.0));
    let face = top_face(&body(&d, bx));
    let view = DocView::new(&d, d.features().len(), Frame::WORLD);
    let doc_tol = d.kernel().tolerance;
    let mut sk = Sketch::new(Frame::WORLD, "Ziel", Tolerance::default());
    let mut copies = Vec::new();
    let mut curves = Vec::new();
    for source in make(s, c, face) {
        let pieces = match &source {
            CopySource::OnSurface(p) => {
                project_to_surface(&view, p, &doc_tol, SURFACE_PROJECT_BUDGET).unwrap()
            }
            CopySource::Intersection(p) => {
                intersection_curve(&view, p, &doc_tol, SURFACE_PROJECT_BUDGET).unwrap()
            }
            other => panic!("{other:?}"),
        };
        for (k, g) in pieces.iter().enumerate() {
            let ids = sk.add_included(g).unwrap();
            curves.push(ids.curves[0]);
            copies.push(EdgeCopy {
                target: CopyTarget::Curve(ids.curves[0]),
                source: source.clone(),
                piece: k as u32,
                of: pieces.len() as u32,
            });
        }
    }
    let t = d
        .add_feature(
            "Ziel",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies,
                formulas: Default::default(),
            }),
        )
        .unwrap();
    (d, bx, t, curves)
}

fn on_top(s: FeatureId, c: CurveId, face: FaceRef) -> CopySource {
    CopySource::OnSurface(SurfaceProjection {
        sources: vec![CurveSource::SketchCurve {
            sketch: s,
            curve: c,
        }],
        faces: vec![FaceTarget::Face(face)],
        kind: ProjectType::AlongVector(DVec3::Z),
    })
}

fn cut_on_top(s: FeatureId, c: CurveId, face: FaceRef) -> CopySource {
    CopySource::Intersection(IntersectionSpec {
        curves: vec![CurveSource::SketchCurve {
            sketch: s,
            curve: c,
        }],
        targets: vec![IntersectTarget::Face(face)],
    })
}

/// Höhen (z) entlang der berechneten Kurve `c` der Skizze `s`.
fn heights(d: &Document, s: FeatureId, c: CurveId) -> Vec<f64> {
    let (curve, t0, t1) = d.sketch_of(s).unwrap().curve_world(c).unwrap();
    (0..=20)
        .map(|i| curve.point(t0 + (t1 - t0) * i as f64 / 20.0).z)
        .collect()
}

fn set_box_height(d: &mut Document, b: FeatureId, z: f64) {
    d.update(b, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.z = z;
        }
    })
    .unwrap();
}

fn warning_of(d: &Document, s: FeatureId) -> String {
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    f.warning().unwrap_or_default().to_string()
}

#[test]
fn recompute_follows_body_change() {
    let (mut d, bx, t, curves) =
        recompute_doc(|s, c, face| vec![on_top(s, c, face.clone()), cut_on_top(s, c, face)]);
    assert_eq!(curves.len(), 2);
    assert_eq!(warning_of(&d, t), "");
    for &c in &curves {
        assert!(heights(&d, t, c).iter().all(|z| (z - 5.0).abs() < 1e-6));
    }
    set_box_height(&mut d, bx, 8.0);
    assert_eq!(warning_of(&d, t), "");
    for &c in &curves {
        let h = heights(&d, t, c);
        assert!(h.iter().all(|z| (z - 8.0).abs() < 1e-6), "{h:?}");
    }
    d.check_invariants().unwrap();
}

#[test]
fn recompute_piece_change_warns() {
    let (mut d, bx, t, curves) = recompute_doc(|s, c, face| vec![on_top(s, c, face)]);
    assert_eq!(curves.len(), 1);
    assert_eq!(warning_of(&d, t), "");
    let before = heights(&d, t, curves[0]);
    // Vor die Skizzen ein Loch in den Quader: Die Linie zerfällt in zwei Stücke.
    d.set_marker(1).unwrap();
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
    d.add_feature(
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
    d.set_marker(d.features().len()).unwrap();
    let w = warning_of(&d, t);
    assert!(
        w.contains(&planora_i18n::tr!(
            "feature-sketch-reference-mismatch",
            count = 1
        )),
        "{w}"
    );
    assert_eq!(heights(&d, t, curves[0]), before);
    d.check_invariants().unwrap();
}

#[test]
fn recompute_tooslow_keeps_position_and_warns() {
    let (mut d, bx, t, curves) =
        recompute_doc(|s, c, face| vec![on_top(s, c, face.clone()), cut_on_top(s, c, face)]);
    assert_eq!(warning_of(&d, t), "");
    let before: Vec<Vec<f64>> = curves.iter().map(|&c| heights(&d, t, c)).collect();
    {
        let _budget = surface_project::recompute_budget_for_tests(Duration::from_millis(1));
        let _slow = surface_project::slow_fit_for_tests(Duration::from_millis(5));
        set_box_height(&mut d, bx, 8.0);
    }
    let w = warning_of(&d, t);
    assert!(
        w.contains(&planora_i18n::tr!(
            "feature-sketch-too-slow",
            count = 2,
            seconds = 2
        )),
        "{w}"
    );
    for (k, &c) in curves.iter().enumerate() {
        assert_eq!(heights(&d, t, c), before[k]);
    }
    // Mit der normalen Zeitgrenze folgt die Kurve wieder.
    set_box_height(&mut d, bx, 9.0);
    assert_eq!(warning_of(&d, t), "");
    assert!(
        heights(&d, t, curves[0])
            .iter()
            .all(|z| (z - 9.0).abs() < 1e-6)
    );
}

/// Task 19, Fix-Runde 1: Mit der Toleranz des Dokuments (1e-6) und der echten Zeitgrenze
/// `SURFACE_PROJECT_BUDGET` gelingen die einfachen Fälle: Kreis oben gegen Linie vorn
/// (Schnittkurve) und eine Linie auf einen Zylinder (beide Projektionsarten). Vorher kostete die
/// Prüfung der Annäherung (globaler Abstand je Mittelpunkt) allein über 3 s im Release.
#[test]
fn simple_cases_fit_budget_with_document_tolerance() {
    let doc_tol = Tolerance::default();
    let mut d = Document::new();
    let mut top = Sketch::new(Frame::WORLD, "Oben", Tolerance::default());
    let m = top.add_point(0.0, 0.0).unwrap();
    let circle = top.add_circle(m, 10.0).unwrap();
    let s_top = add_sketch_on(&mut d, top, StandardPlane::XY);
    let mut front = Sketch::new(StandardPlane::XZ.frame(), "Vorn", Tolerance::default());
    let a = front.add_point(-15.0, -15.0).unwrap();
    let b = front.add_point(15.0, 15.0).unwrap();
    let line = front.add_line(a, b).unwrap();
    let s_front = add_sketch_on(&mut d, front, StandardPlane::XZ);
    let spec = IntersectionSpec {
        curves: vec![CurveSource::SketchCurve {
            sketch: s_top,
            curve: circle,
        }],
        targets: vec![IntersectTarget::SketchCurve {
            sketch: s_front,
            curve: line,
        }],
    };
    let view = DocView::new(&d, d.features().len(), Frame::WORLD);
    let got = intersection_curve(&view, &spec, &doc_tol, SURFACE_PROJECT_BUDGET).unwrap();
    for p in samples(&got[0]) {
        assert!((p.x.hypot(p.y) - 10.0).abs() <= doc_tol.linear, "{p:?}");
        assert!((p.z - p.x).abs() <= doc_tol.linear, "{p:?}");
    }
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
    for kind in [
        ProjectType::ClosestPoint,
        ProjectType::AlongVector(DVec3::Y),
    ] {
        let spec = SurfaceProjection {
            sources: vec![CurveSource::SketchCurve {
                sketch: s,
                curve: c,
            }],
            faces: faces_of(&body(&d, cyl), SurfaceKind::Cylinder),
            kind: kind.clone(),
        };
        let view = DocView::new(&d, d.features().len(), Frame::WORLD);
        let got = project_to_surface(&view, &spec, &doc_tol, SURFACE_PROJECT_BUDGET)
            .unwrap_or_else(|e| panic!("{kind:?}: {e:?}"));
        for p in samples(&got[0]) {
            assert!(
                (p.x.hypot(p.y) - 5.0).abs() <= doc_tol.linear,
                "{kind:?} {p:?}"
            );
        }
    }
}
