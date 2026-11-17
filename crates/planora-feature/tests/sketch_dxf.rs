//! Skizzen 11b, Task 5: Skizze als DXF-Modell (`sketch_dxf`). Geprüft wird das ebene Modell,
//! nicht die Datei: Längen in der gewählten Einheit, Ebenen, Filter (Punkte, Projiziertes,
//! Konstruktion), Text als Kurven, 3D-Skizze abgelehnt, Lage auf versetzter Ebene.

use planora_drawing::out2d::Out2d;
use planora_drawing::plain_dxf::{DxfUnit, PlainDxf, PlainEntity};
use planora_feature::planes::{AxisRef, PlaneBase, PlaneDef, PlaneFeature, StdAxis};
use planora_feature::*;
use planora_i18n::tr;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::Sketch;
use planora_sketch::text::{HAlign, TextParams, VAlign};
use planora_sketch::text_shape::{TextShaper, TextStyle};
use planora_text::FontLibrary;
use std::sync::Arc;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn opts(unit: DxfUnit) -> SketchDxfOptions {
    SketchDxfOptions {
        unit,
        points: false,
        projected: false,
        construction: false,
    }
}

fn sketch_on(plane: SketchPlane, sketch: Sketch, copies: Vec<EdgeCopy>) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch,
        plane,
        copies,
        formulas: Default::default(),
    })
}

fn doc_with(sketch: Sketch) -> (Document, FeatureId) {
    let mut d = Document::new();
    let s = d
        .add_feature(
            "Skizze 1",
            sketch_on(SketchPlane::Standard(StandardPlane::XY), sketch, vec![]),
        )
        .unwrap();
    (d, s)
}

fn new_sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "Skizze", tol())
}

fn layer_of(m: &PlainDxf, e: &(usize, PlainEntity)) -> String {
    m.layers[e.0].name.clone()
}

fn lines(m: &PlainDxf) -> Vec<(DVec2, DVec2, String)> {
    m.entities
        .iter()
        .filter_map(|e| match &e.1 {
            PlainEntity::Curve(Out2d::Line { a, b }) => Some((*a, *b, layer_of(m, e))),
            _ => None,
        })
        .collect()
}

fn points(m: &PlainDxf) -> Vec<DVec2> {
    m.entities
        .iter()
        .filter_map(|e| match &e.1 {
            PlainEntity::Point(p) => Some(*p),
            _ => None,
        })
        .collect()
}

#[test]
fn rectangle_100mm_in_inch_is_3_937() {
    let mut sk = new_sketch();
    sk.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(100.0, 50.0))
        .unwrap();
    let (d, s) = doc_with(sk);
    let m = sketch_dxf(&d, s, &opts(DxfUnit::Inch)).unwrap();
    assert_eq!(m.unit, DxfUnit::Inch);
    let ls = lines(&m);
    assert_eq!(ls.len(), 4);
    let mut lens: Vec<f64> = ls.iter().map(|(a, b, _)| a.distance(*b)).collect();
    lens.sort_by(f64::total_cmp);
    assert!((lens[0] - 50.0 / 25.4).abs() < 1e-9, "{lens:?}");
    assert!((lens[3] - 100.0 / 25.4).abs() < 1e-9, "{lens:?}");
    assert!((lens[3] - 3.937_007_874).abs() < 1e-8);
    // Alles auf Ebene „0“.
    assert!(ls.iter().all(|l| l.2 == "0"));
    assert_eq!(m.layers[0].name, "0");
    assert_eq!(m.layers[0].color, 7);
    // In Millimeter unverändert.
    let m = sketch_dxf(&d, s, &opts(DxfUnit::Millimeter)).unwrap();
    let max = lines(&m)
        .iter()
        .map(|(a, b, _)| a.distance(*b))
        .fold(0.0, f64::max);
    assert!((max - 100.0).abs() < 1e-9);
}

#[test]
fn all_curve_kinds_come_through() {
    let mut sk = new_sketch();
    let c = sk.add_point(0.0, 0.0).unwrap();
    sk.add_circle(c, 5.0).unwrap();
    let e = sk.add_point(30.0, 0.0).unwrap();
    sk.add_ellipse(e, DVec2::new(10.0, 0.0), 4.0).unwrap();
    let (a, b, cc) = (
        sk.add_point(60.0, 0.0).unwrap(),
        sk.add_point(70.0, 0.0).unwrap(),
        sk.add_point(60.0, 10.0).unwrap(),
    );
    sk.add_arc(a, b, cc).unwrap();
    let sp: Vec<_> = [(0.0, 20.0), (10.0, 25.0), (20.0, 20.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    sk.add_spline(&sp).unwrap();
    let (d, s) = doc_with(sk);
    let m = sketch_dxf(&d, s, &opts(DxfUnit::Millimeter)).unwrap();
    let kinds: Vec<&str> = m
        .entities
        .iter()
        .map(|e| match &e.1 {
            PlainEntity::Curve(Out2d::Arc { .. }) => "arc",
            PlainEntity::Curve(Out2d::Ellipse { .. }) => "ellipse",
            PlainEntity::Curve(Out2d::Spline { .. }) => "spline",
            PlainEntity::Curve(Out2d::Line { .. }) => "line",
            PlainEntity::Point(_) => "point",
        })
        .collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == "arc").count(),
        2,
        "{kinds:?}"
    );
    assert_eq!(kinds.iter().filter(|k| **k == "ellipse").count(), 1);
    assert_eq!(kinds.iter().filter(|k| **k == "spline").count(), 1);
    assert_eq!(kinds.len(), 4, "keine Punkte ohne Häkchen: {kinds:?}");
}

#[test]
fn construction_only_when_ticked() {
    let mut sk = new_sketch();
    let (a, b) = (
        sk.add_point(0.0, 0.0).unwrap(),
        sk.add_point(10.0, 0.0).unwrap(),
    );
    sk.add_line(a, b).unwrap();
    let (c, e) = (
        sk.add_point(0.0, 5.0).unwrap(),
        sk.add_point(10.0, 5.0).unwrap(),
    );
    let k = sk.add_line(c, e).unwrap();
    sk.set_construction(k, true).unwrap();
    let (f, g) = (
        sk.add_point(0.0, 8.0).unwrap(),
        sk.add_point(10.0, 8.0).unwrap(),
    );
    let mid = sk.add_line(f, g).unwrap();
    sk.set_centerline(mid, true).unwrap();
    let (d, s) = doc_with(sk);

    let m = sketch_dxf(&d, s, &opts(DxfUnit::Millimeter)).unwrap();
    assert_eq!(
        lines(&m).len(),
        1,
        "ohne Häkchen fehlen Konstruktion und Mittellinie"
    );

    let on = SketchDxfOptions {
        construction: true,
        ..opts(DxfUnit::Millimeter)
    };
    let m = sketch_dxf(&d, s, &on).unwrap();
    let ls = lines(&m);
    assert_eq!(ls.len(), 3);
    let constr = tr!("sketch-dxf-layer-construction");
    assert_eq!(ls.iter().filter(|l| l.2 == "0").count(), 1);
    assert_eq!(ls.iter().filter(|l| l.2 == constr).count(), 2);
    assert_eq!(m.layers[0].name, "0");
    assert_eq!(m.layers[1].name, constr);
    assert_eq!(m.layers[1].color, 8);
}

#[test]
fn projected_only_when_ticked() {
    let mut d = Document::new();
    let b = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(20.0, 10.0, 5.0),
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
    // Kante von (0,0,0) nach (20,0,0).
    let (id, edge) = solid
        .edges
        .iter()
        .find(|(_, e)| {
            let (p, q) = (solid.vertices[e.start].point, solid.vertices[e.end].point);
            let on = |v: DVec3| v.y.abs() < 1e-9 && v.z.abs() < 1e-9;
            on(p) && on(q)
        })
        .unwrap();
    let mut sk = new_sketch();
    let (p0, p1) = (
        sk.add_point(0.0, 20.0).unwrap(),
        sk.add_point(10.0, 20.0).unwrap(),
    );
    sk.add_line(p0, p1).unwrap();
    let c = sk
        .add_projected_curve(
            &edge.curve,
            edge.t0,
            edge.t1,
            ProjectStyle {
                construction: false,
            },
        )
        .unwrap();
    let copy = EdgeCopy::new(
        c,
        CopySource::Edge(EdgeRef::from_edge(&solid, id, &tol()).unwrap()),
    );
    let s = d
        .add_feature(
            "Skizze 1",
            sketch_on(SketchPlane::Standard(StandardPlane::XY), sk, vec![copy]),
        )
        .unwrap();
    assert_eq!(d.feature(s).unwrap().status, FeatureStatus::Ok);
    let m = sketch_dxf(&d, s, &opts(DxfUnit::Millimeter)).unwrap();
    assert_eq!(lines(&m).len(), 1, "projizierte Kante fehlt ohne Häkchen");
    let on = SketchDxfOptions {
        projected: true,
        ..opts(DxfUnit::Millimeter)
    };
    let m = sketch_dxf(&d, s, &on).unwrap();
    let ls = lines(&m);
    assert_eq!(ls.len(), 2);
    assert!(ls.iter().all(|l| l.2 == "0"), "nicht Konstruktion: Ebene 0");
    assert!(ls.iter().any(|(a, b, _)| a.distance(*b) > 19.9));
    // Das Konstruktionshäkchen allein holt sie nicht herein.
    let only_constr = SketchDxfOptions {
        construction: true,
        ..opts(DxfUnit::Millimeter)
    };
    assert_eq!(lines(&sketch_dxf(&d, s, &only_constr).unwrap()).len(), 1);
}

#[test]
fn free_point_only_when_ticked() {
    let mut sk = new_sketch();
    let (a, b) = (
        sk.add_point(0.0, 0.0).unwrap(),
        sk.add_point(25.4, 0.0).unwrap(),
    );
    sk.add_line(a, b).unwrap();
    sk.add_point(25.4, 50.8).unwrap();
    let (d, s) = doc_with(sk);
    let m = sketch_dxf(&d, s, &opts(DxfUnit::Millimeter)).unwrap();
    assert!(points(&m).is_empty());
    let on = SketchDxfOptions {
        points: true,
        ..opts(DxfUnit::Inch)
    };
    let m = sketch_dxf(&d, s, &on).unwrap();
    let ps = points(&m);
    // Nur der freie Punkt, nicht die Endpunkte der Linie.
    assert_eq!(ps.len(), 1, "{ps:?}");
    assert!(ps[0].distance(DVec2::new(1.0, 2.0)) < 1e-9, "{ps:?}");
}

#[test]
fn text_as_curves_not_text_entity() {
    let lib = FontLibrary::bundled_only();
    let style = TextStyle {
        font: "Montserrat".into(),
        bold: false,
        italic: false,
        spacing: 0.0,
    };
    let shape = lib.shape("A", &style).unwrap();
    let mut sk = new_sketch();
    sk.add_text_box(
        DVec2::new(0.0, 0.0),
        DVec2::new(40.0, 10.0),
        TextParams {
            content: "A".into(),
            style,
            height: 5.0,
            flip_h: false,
            flip_v: false,
        },
        HAlign::Left,
        VAlign::Bottom,
        Arc::new(shape),
    )
    .unwrap();
    let (d, s) = doc_with(sk);
    let m = sketch_dxf(&d, s, &opts(DxfUnit::Millimeter)).unwrap();
    assert!(!m.entities.is_empty(), "Buchstabenkurven fehlen");
    assert!(
        m.entities
            .iter()
            .all(|e| matches!(e.1, PlainEntity::Curve(_)) && layer_of(&m, e) == "0")
    );
    // Der Rahmen (vier Konstruktionslinien) fehlt: Kein Objekt ist eine ganze Rahmenseite von
    // (0, 0) bis (40, 10).
    for (_, e) in &m.entities {
        if let PlainEntity::Curve(Out2d::Line { a, b }) = e {
            let long = a.distance(*b) >= 9.99;
            assert!(!long, "Rahmenlinie? {a} {b}");
        }
    }
    let n_plain = m.entities.len();
    let on = SketchDxfOptions {
        construction: true,
        ..opts(DxfUnit::Millimeter)
    };
    let m = sketch_dxf(&d, s, &on).unwrap();
    assert_eq!(
        m.entities.len(),
        n_plain + 4,
        "vier Rahmenlinien mit Häkchen"
    );
}

#[test]
fn three_d_sketch_refused() {
    let mut sk = new_sketch();
    let a = sk.add_point3(DVec3::new(0.0, 0.0, 5.0)).unwrap();
    let b = sk.add_point3(DVec3::new(10.0, 0.0, 5.0)).unwrap();
    sk.add_line(a, b).unwrap();
    let (d, s) = doc_with(sk);
    let err = sketch_dxf(&d, s, &opts(DxfUnit::Millimeter)).unwrap_err();
    assert_eq!(err, SketchDxfError::NotPlanar);
    assert_eq!(err.message(), tr!("sketch-dxf-not-planar"));
    assert_eq!(
        planora_i18n::with_lang(&planora_i18n::german(), || err.message()),
        "Nur ebene Skizzen lassen sich als DXF speichern."
    );
    assert_eq!(
        planora_i18n::with_lang(&planora_i18n::english(), || err.message()),
        "Only planar sketches can be saved as DXF."
    );
}

#[test]
fn only_construction_and_unticked_is_empty() {
    let mut sk = new_sketch();
    let (a, b) = (
        sk.add_point(0.0, 0.0).unwrap(),
        sk.add_point(10.0, 0.0).unwrap(),
    );
    let l = sk.add_line(a, b).unwrap();
    sk.set_construction(l, true).unwrap();
    sk.add_point(3.0, 3.0).unwrap();
    let (d, s) = doc_with(sk);
    let err = sketch_dxf(&d, s, &opts(DxfUnit::Millimeter)).unwrap_err();
    assert_eq!(err, SketchDxfError::Empty);
    assert_eq!(err.message(), tr!("sketch-dxf-empty"));
    // Mit Häkchen ist es nicht mehr leer.
    let on = SketchDxfOptions {
        construction: true,
        ..opts(DxfUnit::Millimeter)
    };
    assert!(sketch_dxf(&d, s, &on).is_ok());
    // Kein Skizzen-Feature: unbekannt.
    let mut d2 = Document::new();
    let b = d2
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::ONE,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    assert_eq!(
        sketch_dxf(&d2, b, &opts(DxfUnit::Millimeter)).unwrap_err(),
        SketchDxfError::Unknown
    );
}

#[test]
fn sketch_on_offset_plane_uses_local_xy() {
    let mut d = Document::new();
    let offset = d
        .add_feature(
            "Ebene",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Offset {
                    base: PlaneBase::Standard(StandardPlane::XY),
                    distance: 37.0,
                },
            }),
        )
        .unwrap();
    // Gedrehte Ebene: 30° um die X-Achse.
    let rotated = d
        .add_feature(
            "Gedreht",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Angle {
                    axis: AxisRef::Standard(StdAxis::X),
                    angle: 30.0,
                },
            }),
        )
        .unwrap();
    for plane in [offset, rotated] {
        let mut sk = new_sketch();
        let (a, b) = (
            sk.add_point(10.0, 20.0).unwrap(),
            sk.add_point(40.0, 20.0).unwrap(),
        );
        sk.add_line(a, b).unwrap();
        let s = d
            .add_feature("Skizze", sketch_on(SketchPlane::OnPlane(plane), sk, vec![]))
            .unwrap();
        assert_eq!(d.feature(s).unwrap().status, FeatureStatus::Ok);
        let f = *d.sketch_of(s).unwrap().frame();
        assert!(
            f.origin.length() > 1.0 || f.z.distance(DVec3::Z) > 1e-3,
            "die Ebene liegt nicht in der Grundebene"
        );
        let m = sketch_dxf(&d, s, &opts(DxfUnit::Millimeter)).unwrap();
        let ls = lines(&m);
        assert_eq!(ls.len(), 1);
        let (a, b, _) = ls[0].clone();
        assert!(a.distance(DVec2::new(10.0, 20.0)) < 1e-9, "{a} {b}");
        assert!(b.distance(DVec2::new(40.0, 20.0)) < 1e-9, "{a} {b}");
    }
}
