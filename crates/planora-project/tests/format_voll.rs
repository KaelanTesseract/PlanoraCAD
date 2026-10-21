//! Golden-Datei mit allen Teilen von Format 1: alle Feature-Arten, alle Kurvenarten, alle
//! Abhängigkeitsarten, beide Ebenenarten, Kantenkopie, gesteuert/ausgeschaltet.

use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_project::*;
use planora_sketch::{ConstraintKind as K, LineOrPoints, Sketch, plane_of_face};

/// Skizze mit allen Kurvenarten und jeder Abhängigkeitsart (auch Flags gesteuert/aus).
fn rich_sketch(frame: Frame) -> Sketch {
    let mut sk = Sketch::new(frame, "Alles", Tolerance::default());
    let mut pts = Vec::new();
    for (x, y) in [
        (0.0, 0.0),
        (10.0, 0.0),
        (0.0, 5.0),
        (10.0, 5.0),
        (0.0, 10.0),
        (10.0, 12.0),
        (20.0, 0.0),
        (20.0, 10.0),
        (30.0, 5.0),
        (40.0, 5.0),
        (50.0, 0.0),
        (55.0, 0.0),
        (50.0, 5.0),
        (60.0, 0.0),
        (70.0, 0.0),
        (72.0, 3.0),
        (75.0, 1.0),
    ] {
        pts.push(sk.add_point(x, y).unwrap());
    }
    let p = |i: usize| pts[i];
    let l1 = sk.add_line(p(0), p(1)).unwrap();
    let l2 = sk.add_line(p(2), p(3)).unwrap();
    let l3 = sk.add_line(p(4), p(5)).unwrap();
    let l4 = sk.add_line(p(6), p(7)).unwrap();
    let k1 = sk.add_circle(p(8), 3.0).unwrap();
    let k2 = sk.add_circle(p(9), 3.0).unwrap();
    sk.add_arc(p(10), p(11), p(12)).unwrap();
    let el = sk.add_ellipse(p(13), DVec2::new(6.0, 0.0), 3.0).unwrap();
    sk.add_spline(&[p(14), p(15), p(16)]).unwrap();
    sk.set_construction(l3, true).unwrap();
    let mut add = |k: K| sk.add_constraint(k).unwrap();
    add(K::Coincident(p(1), p(6)));
    add(K::PointOnCurve(p(7), l2));
    add(K::Horizontal(LineOrPoints::Line(l1)));
    add(K::Vertical(LineOrPoints::Points(p(6), p(7))));
    add(K::Parallel(l1, l2));
    add(K::Perpendicular(l1, l4));
    add(K::Tangent(l2, k2));
    add(K::Equal(k1, k2));
    add(K::Fixed(p(0), 0.0, 0.0));
    add(K::Symmetric(p(2), p(3), l4));
    add(K::Midpoint(p(5), l2));
    let dist = add(K::Distance(p(0), p(1), 10.0));
    add(K::DistancePointLine(p(4), l1, 10.0));
    add(K::Radius(k1, 3.0));
    let diam = add(K::Diameter(k2, 6.0));
    add(K::Angle(l1, l3, 0.2));
    add(K::EllipseMajor(el, 6.0));
    add(K::EllipseMinor(el, 3.0));
    add(K::EllipseAngle(el, 0.0));
    sk.set_driven(dist, true).unwrap();
    sk.set_enabled(diam, false).unwrap();
    sk
}

fn rich_document() -> Document {
    let mut d = Document::new();
    let bx = |o: f64| FeatureKind::Box {
        size: DVec3::new(20.0, 20.0, 10.0),
        origin: DVec3::new(o, 0.0, 0.0),
    };
    let body = |f: FeatureId| BodyRef {
        feature: f,
        index: 0,
    };
    let b1 = d.add_feature("Quader 1", bx(0.0)).unwrap();
    let z1 = d
        .add_feature(
            "Zylinder 1",
            FeatureKind::Cylinder {
                radius: 4.0,
                height: 14.0,
                origin: DVec3::new(10.0, 10.0, -2.0),
            },
        )
        .unwrap();
    let u = d
        .add_feature(
            "Vereinigung",
            FeatureKind::Boolean {
                target: body(b1),
                tool: body(z1),
                op: BoolOp::Union,
            },
        )
        .unwrap();
    d.add_feature(
        "Weg",
        FeatureKind::Move {
            body: body(u),
            delta: DVec3::new(0.0, 0.0, 5.0),
        },
    )
    .unwrap();
    let b2 = d.add_feature("Quader 2", bx(100.0)).unwrap();
    let k2 = d
        .add_feature(
            "Kugel 2",
            FeatureKind::Sphere {
                radius: 5.0,
                origin: DVec3::new(110.0, 10.0, 10.0),
            },
        )
        .unwrap();
    d.add_feature(
        "Abzug",
        FeatureKind::Boolean {
            target: body(b2),
            tool: body(k2),
            op: BoolOp::Difference,
        },
    )
    .unwrap();
    let b3 = d.add_feature("Quader 3", bx(200.0)).unwrap();
    let c3 = d
        .add_feature(
            "Kegel 3",
            FeatureKind::Cone {
                radius: 6.0,
                height: 6.0,
                origin: DVec3::new(210.0, 10.0, 2.0),
            },
        )
        .unwrap();
    d.add_feature(
        "Schnitt",
        FeatureKind::Boolean {
            target: body(b3),
            tool: body(c3),
            op: BoolOp::Intersection,
        },
    )
    .unwrap();
    // Rotation: Skizze auf XZ, Achse als Hilfslinie
    let mut rs = Sketch::new(StandardPlane::XZ.frame(), "Profil", Tolerance::default());
    // Die linke Rechteckkante ist die Drehachse.
    let rect = rs
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 10.0))
        .unwrap();
    let axis = rect.lines[3];
    let rid = d
        .add_feature(
            "Profil",
            FeatureKind::Sketch(SketchFeature {
                sketch: rs,
                plane: SketchPlane::Standard(StandardPlane::XZ),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    d.add_feature(
        "Rotation 1",
        FeatureKind::Revolve {
            sketch: rid,
            profile: 0,
            axis,
        },
    )
    .unwrap();
    d.add_feature(
        "Alles",
        FeatureKind::Sketch(SketchFeature {
            sketch: rich_sketch(StandardPlane::YZ.frame()),
            plane: SketchPlane::Standard(StandardPlane::YZ),
            copies: vec![],
            formulas: Default::default(),
        }),
    )
    .unwrap();
    // Extrusion; Skizze auf deren Deckel mit Kantenkopie
    let mut base = Sketch::new(StandardPlane::XY.frame(), "Grund", Tolerance::default());
    base.add_rectangle(DVec2::new(300.0, 0.0), DVec2::new(340.0, 30.0))
        .unwrap();
    let sid = d
        .add_feature(
            "Grund",
            FeatureKind::Sketch(SketchFeature {
                sketch: base,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let ex = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: sid,
                profile: 0,
                height: 10.0,
            },
        )
        .unwrap();
    let solid = d.body(body(ex)).unwrap().clone();
    let tol = Tolerance::default();
    let (fid, _) = solid
        .faces
        .iter()
        .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == "Deckel"))
        .unwrap();
    let face = FaceRef::from_face(&solid, fid, &tol).unwrap();
    let mut fs = Sketch::new(plane_of_face(&solid, fid).unwrap(), "Auf Fläche", tol);
    let mut copy = None;
    for (id, _) in solid.edges.iter() {
        if let Ok(c) = fs.add_projected_edge(&solid, id)
            && let Ok(e) = EdgeRef::from_edge(&solid, id, &tol)
        {
            copy = Some(EdgeCopy { curve: c, edge: e });
            break;
        }
    }
    let copy = copy.expect("eine kopierbare Kante");
    d.add_feature(
        "Auf Fläche",
        FeatureKind::Sketch(SketchFeature {
            sketch: fs,
            plane: SketchPlane::OnFace(face),
            copies: vec![copy],
            formulas: Default::default(),
        }),
    )
    .unwrap();
    d
}

#[test]
fn the_rich_golden_file_pins_every_part_of_format_1() {
    let d = rich_document();
    d.check_invariants().unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/format1_voll.json");
    if std::env::var("PLANORA_UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    let golden = std::fs::read_to_string(&path)
        .expect("tests/data/format1_voll.json fehlt – mit PLANORA_UPDATE_GOLDEN=1 erzeugen");
    assert_eq!(text, golden.trim_end_matches('\n'));
    let snap = decode_version(&golden, "format1_voll.json").unwrap();
    assert_eq!(encode_version(&snap).unwrap(), text);
    let doc = Document::from_snapshot(snap, 0.05).unwrap();
    doc.check_invariants().unwrap();
    assert_eq!(doc.features().len(), d.features().len());
    for (a, b) in d.features().iter().zip(doc.features()) {
        assert_eq!(a.status, b.status, "{}", a.name);
    }
    for key in [
        "\"rotation\"",
        "\"boolesch\"",
        "\"verschieben\"",
        "\"flaeche\"",
        "\"vereinigen\"",
        "\"abziehen\"",
        "\"schneiden\"",
        "\"xz\"",
        "\"yz\"",
        "\"linie\"",
        "\"kreis\"",
        "\"bogen\"",
        "\"ellipse\"",
        "\"spline\"",
        "\"bogen_radius\"",
        "\"ellipse_winkel\"",
        "\"kante\"",
    ] {
        assert!(text.contains(key), "fehlt: {key}");
    }
}

#[test]
fn the_rich_document_computes_without_errors() {
    for f in rich_document().features() {
        assert_eq!(f.status, FeatureStatus::Ok, "{}", f.name);
    }
}
