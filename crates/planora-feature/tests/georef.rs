//! Bezüge von Maßen (Teilprojekt 12, Aufgabe 8): Umwandlung der Verweise und Auflösung gegen
//! die exakten Körper. Sollwerte aus den Eingabemaßen der Features (Formelmodell), nicht aus
//! derselben Rechnung.

use planora_drawing::dimension::{DimError, DimKind, EndSel, Geo3, GeoRef, GeoRefData, TagData};
use planora_drawing::{Basis, ViewKind, dimension_key, measure};
use planora_feature::*;
use planora_kernel::{BoolOp, Curve, Edge, Face, Id, Solid, Surface, edge_probe_point};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::model::Sketch;

const EPS: f64 = 1e-9;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn near(a: DVec3, b: DVec3) -> bool {
    a.distance(b) <= EPS
}

fn bx(size: DVec3, origin: DVec3) -> FeatureKind {
    FeatureKind::Box { size, origin }
}

/// Der sichtbare Körper des Features `name`.
fn body<'a>(d: &'a Document, name: &str) -> &'a Solid {
    d.visible_bodies()
        .into_iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("Körper {name}"))
        .solid
}

fn all_solids(d: &Document) -> Vec<&Solid> {
    d.visible_bodies().into_iter().map(|b| b.solid).collect()
}

/// Die gerade Kante mit den Endpunkten `a` und `b` (in beliebiger Richtung).
fn line_edge(s: &Solid, a: DVec3, b: DVec3) -> Id<Edge> {
    s.edges
        .iter()
        .find(|(_, e)| {
            let (p, q) = (s.vertices[e.start].point, s.vertices[e.end].point);
            matches!(e.curve, Curve::Line { .. })
                && ((near(p, a) && near(q, b)) || (near(p, b) && near(q, a)))
        })
        .map(|(i, _)| i)
        .unwrap_or_else(|| panic!("Kante {a} – {b}"))
}

fn face_where(s: &Solid, pred: impl Fn(&Surface) -> bool) -> Id<Face> {
    s.faces
        .iter()
        .find(|(_, f)| pred(&f.surface))
        .map(|(i, _)| i)
        .expect("Fläche")
}

/// Strecke `{a, b}` ohne Rücksicht auf die Richtung.
fn same_segment(g: &Geo3, a: DVec3, b: DVec3) -> bool {
    match g {
        Geo3::Line { a: p, b: q } => (near(*p, a) && near(*q, b)) || (near(*p, b) && near(*q, a)),
        _ => false,
    }
}

#[test]
fn edge_ref_roundtrip_through_geo_ref_data() {
    let mut d = Document::new();
    d.add_feature("Quader", bx(DVec3::new(10.0, 30.0, 20.0), DVec3::ZERO))
        .unwrap();
    let s = body(&d, "Quader");
    let e = line_edge(s, DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 20.0));
    let r = EdgeRef::from_edge(s, e, &tol()).unwrap();
    let data = from_edge_ref(&r);
    assert_eq!(to_edge_ref(&data), Some(r.clone()));
    assert_eq!(to_face_ref(&data), None);
    assert_eq!(geo_ref_for_edge(s, e, &tol()).unwrap(), data);
    // Und in der Gegenrichtung: beliebige Daten (auch ohne Körper) bleiben erhalten.
    let t = |o: u64, role: &str, x: f64| TagData {
        origin: o,
        role: role.into(),
        hint: DVec3::new(x, -0.5, 1e-7),
    };
    let raw = GeoRefData::Edge {
        a: t(7, "Wand 3", 1.25),
        b: t(9, "Deckel", -3.0),
        hint: DVec3::new(0.1, 0.2, 0.3),
    };
    assert_eq!(from_edge_ref(&to_edge_ref(&raw).unwrap()), raw);
}

#[test]
fn face_ref_roundtrip() {
    let mut d = Document::new();
    d.add_feature(
        "Zylinder",
        FeatureKind::Cylinder {
            radius: 3.0,
            height: 5.0,
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let s = body(&d, "Zylinder");
    let f = face_where(s, |x| matches!(x, Surface::Cylinder { .. }));
    let r = FaceRef::from_face(s, f, &tol()).unwrap();
    let data = from_face_ref(&r);
    assert_eq!(to_face_ref(&data), Some(r));
    assert_eq!(to_edge_ref(&data), None);
    assert_eq!(geo_ref_for_face(s, f, &tol()).unwrap(), data);
    let raw = GeoRefData::Face(TagData {
        origin: 12,
        role: "Mantel".into(),
        hint: DVec3::new(-1.0, 2.5, 1e-300),
    });
    assert_eq!(from_face_ref(&to_face_ref(&raw).unwrap()), raw);
}

#[test]
fn resolve_line_circle_axis_sphere_point() {
    let mut d = Document::new();
    d.add_feature("Quader", bx(DVec3::new(10.0, 30.0, 20.0), DVec3::ZERO))
        .unwrap();
    let c0 = DVec3::new(50.0, 0.0, 0.0);
    let (r, h) = (3.0, 5.0);
    d.add_feature(
        "Zylinder",
        FeatureKind::Cylinder {
            radius: r,
            height: h,
            origin: c0,
        },
    )
    .unwrap();
    let k0 = DVec3::new(0.0, 60.0, 0.0);
    d.add_feature(
        "Kugel",
        FeatureKind::Sphere {
            radius: 4.0,
            origin: k0,
        },
    )
    .unwrap();
    let solids = all_solids(&d);
    assert_eq!(solids.len(), 3);
    let resolve = |g: GeoRef| resolve_geo(&solids, &g, &tol());

    // Quaderkante x = 10, y = 0 von z = 0 bis 20.
    let q = body(&d, "Quader");
    let (pa, pb) = (DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 20.0));
    let edge = geo_ref_for_edge(q, line_edge(q, pa, pb), &tol()).unwrap();
    let line = resolve(GeoRef::Edge(edge.clone())).unwrap();
    assert!(same_segment(&line, pa, pb), "{line:?}");
    // Eckpunkte: Anfang und Ende der Kante, verschieden, beide aus {pa, pb}.
    let start = resolve(GeoRef::EdgeEnd {
        edge: edge.clone(),
        end: EndSel::Start,
    })
    .unwrap();
    let end = resolve(GeoRef::EdgeEnd {
        edge: edge.clone(),
        end: EndSel::End,
    })
    .unwrap();
    match (&line, &start, &end) {
        (Geo3::Line { a, b }, Geo3::Point(s), Geo3::Point(e)) => {
            assert!(near(*a, *s) && near(*b, *e), "{start:?} {end:?}");
            assert!(!near(*s, *e));
        }
        other => panic!("{other:?}"),
    }

    // Obere Kreiskante des Zylinders: Mitte c0 + h·z, Normale ±z, Radius r.
    let z = solids_find_circle(body(&d, "Zylinder"), c0.z + h);
    let zyl = body(&d, "Zylinder");
    let circle_ref = geo_ref_for_edge(zyl, z, &tol()).unwrap();
    let top = c0 + DVec3::Z * h;
    match resolve(GeoRef::Edge(circle_ref.clone())).unwrap() {
        Geo3::Circle {
            center,
            normal,
            radius,
        } => {
            assert!(near(center, top), "{center}");
            assert!((normal.normalize().z.abs() - 1.0).abs() <= EPS, "{normal}");
            assert!((radius - r).abs() <= EPS);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        resolve(GeoRef::Center(circle_ref.clone())).unwrap(),
        Geo3::Point(top)
    );
    // Kreisende: ein Vollkreis hat nur einen Eckpunkt, Anfang und Ende liegen auf dem Kreis.
    for end in [EndSel::Start, EndSel::End] {
        let Geo3::Point(p) = resolve(GeoRef::EdgeEnd {
            edge: circle_ref.clone(),
            end,
        })
        .unwrap() else {
            panic!("Punkt erwartet")
        };
        assert!(((p - top).length() - r).abs() <= EPS && (p.z - top.z).abs() <= EPS);
    }

    // Mantelfläche: Achse durch c0 entlang ±z, Radius r, Bereich von z = 0 bis h.
    let mantel = face_where(zyl, |x| matches!(x, Surface::Cylinder { .. }));
    let face_ref = geo_ref_for_face(zyl, mantel, &tol()).unwrap();
    let axis = resolve(GeoRef::Face(face_ref)).unwrap();
    match &axis {
        Geo3::Axis {
            origin,
            dir,
            radius,
            t0,
            t1,
        } => {
            assert!((dir.length() - 1.0).abs() <= EPS && (dir.z.abs() - 1.0).abs() <= EPS);
            assert!((radius - r).abs() <= EPS);
            assert!(t0 <= t1);
            let (e0, e1) = (*origin + *dir * *t0, *origin + *dir * *t1);
            assert!(
                (near(e0, c0) && near(e1, top)) || (near(e0, top) && near(e1, c0)),
                "{e0} {e1}"
            );
        }
        other => panic!("{other:?}"),
    }
    // Gleicher Schlüssel für Kreiskante und Mantelfläche derselben Achse (wie im Vorschlag).
    let key = |g: &Geo3| {
        let (b, k) = (Basis::of(ViewKind::Front), ViewKind::Front);
        dimension_key(DimKind::Diameter, std::slice::from_ref(g), &b, k, 60.0).unwrap()
    };
    let circle = resolve(GeoRef::Edge(circle_ref)).unwrap();
    assert_eq!(key(&axis), key(&circle));
    // Messung über die Achse von der Seite: Ø 6.
    let m = measure(
        DimKind::Diameter,
        &[axis],
        &Basis::of(ViewKind::Front),
        ViewKind::Front,
        60.0,
    )
    .unwrap();
    assert!((m.value - 2.0 * r).abs() <= EPS);

    // Kugelfläche.
    let kugel = body(&d, "Kugel");
    let kf = face_where(kugel, |x| matches!(x, Surface::Sphere { .. }));
    let sphere = resolve(GeoRef::Face(geo_ref_for_face(kugel, kf, &tol()).unwrap())).unwrap();
    match sphere {
        Geo3::Sphere { center, radius } => {
            assert!(near(center, k0));
            assert!((radius - 4.0).abs() <= EPS);
        }
        other => panic!("{other:?}"),
    }
}

/// Die Kreiskante des Körpers in der Höhe `z`.
fn solids_find_circle(s: &Solid, z: f64) -> Id<Edge> {
    s.edges
        .iter()
        .find(|(_, e)| {
            matches!(&e.curve, Curve::Circle { frame, .. } if (frame.origin.z - z).abs() <= EPS)
        })
        .map(|(i, _)| i)
        .expect("Kreiskante")
}

#[test]
fn resolve_after_model_edit_follows() {
    let mut d = Document::new();
    let f = d
        .add_feature("Quader", bx(DVec3::new(10.0, 30.0, 20.0), DVec3::ZERO))
        .unwrap();
    let q = body(&d, "Quader");
    let left = geo_ref_for_edge(
        q,
        line_edge(q, DVec3::ZERO, DVec3::new(0.0, 0.0, 20.0)),
        &tol(),
    )
    .unwrap();
    let right = geo_ref_for_edge(
        q,
        line_edge(q, DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 20.0)),
        &tol(),
    )
    .unwrap();
    let value = |d: &Document| {
        let solids = all_solids(d);
        let geos: Vec<Geo3> = [&left, &right]
            .iter()
            .map(|r| resolve_geo(&solids, &GeoRef::Edge((*r).clone()), &tol()).unwrap())
            .collect();
        let m = measure(
            DimKind::Horizontal,
            &geos,
            &Basis::of(ViewKind::Front),
            ViewKind::Front,
            30.0,
        )
        .unwrap();
        (m.value, geos)
    };
    assert!((value(&d).0 - 10.0).abs() <= EPS);
    d.update(f, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.x = 20.0;
        }
    })
    .unwrap();
    let (v, geos) = value(&d);
    assert!((v - 20.0).abs() <= EPS, "{v}");
    assert!(
        same_segment(
            &geos[1],
            DVec3::new(20.0, 0.0, 0.0),
            DVec3::new(20.0, 0.0, 20.0)
        ),
        "{:?}",
        geos[1]
    );
}

#[test]
fn resolve_after_deleting_the_feature_is_lost() {
    let mut d = Document::new();
    let f = d
        .add_feature("Quader", bx(DVec3::splat(10.0), DVec3::ZERO))
        .unwrap();
    d.add_feature("Andere", bx(DVec3::splat(5.0), DVec3::new(50.0, 0.0, 0.0)))
        .unwrap();
    let q = body(&d, "Quader");
    let e = geo_ref_for_edge(
        q,
        line_edge(q, DVec3::ZERO, DVec3::new(0.0, 0.0, 10.0)),
        &tol(),
    )
    .unwrap();
    let edge_ref = GeoRef::Edge(e.clone());
    assert!(resolve_geo(&all_solids(&d), &edge_ref, &tol()).is_ok());
    d.delete(f).unwrap();
    assert_eq!(all_solids(&d).len(), 1);
    for r in [
        edge_ref,
        GeoRef::EdgeEnd {
            edge: e.clone(),
            end: EndSel::End,
        },
        GeoRef::Center(e),
    ] {
        match resolve_geo(&all_solids(&d), &r, &tol()) {
            Err(DimError::Lost(why)) => assert!(why.contains("nicht mehr vorhanden"), "{why}"),
            other => panic!("Lost erwartet: {other:?}"),
        }
    }
    // Auch ohne Körper: verloren, kein Absturz.
    let face = GeoRef::Face(GeoRefData::Face(TagData {
        origin: 1,
        role: "Deckel".into(),
        hint: DVec3::ZERO,
    }));
    assert!(matches!(
        resolve_geo(&[], &face, &tol()),
        Err(DimError::Lost(_))
    ));
}

#[test]
fn ambiguous_after_split_by_boolean_uses_hint() {
    // Platte 20 × 20 × 5, quer eine Nut (x 8…12, z 3…5): Die obere Kante bei y = 0 zerfällt
    // in zwei Kanten mit denselben Flächenmarken (x 0…8 und x 12…20).
    let mut d = Document::new();
    let plate = d
        .add_feature("Platte", bx(DVec3::new(20.0, 20.0, 5.0), DVec3::ZERO))
        .unwrap();
    let groove = d
        .add_feature(
            "Nut",
            bx(DVec3::new(4.0, 30.0, 2.0), DVec3::new(8.0, -5.0, 3.0)),
        )
        .unwrap();
    d.add_feature(
        "Schnitt",
        FeatureKind::Boolean {
            target: BodyRef {
                feature: plate,
                index: 0,
            },
            tool: BodyRef {
                feature: groove,
                index: 0,
            },
            op: BoolOp::Difference,
        },
    )
    .unwrap();
    let solids = all_solids(&d);
    assert_eq!(solids.len(), 1);
    let s = solids[0];
    let p = |x: f64| DVec3::new(x, 0.0, 5.0);
    let e1 = line_edge(s, p(0.0), p(8.0));
    let e2 = line_edge(s, p(12.0), p(20.0));
    let r1 = geo_ref_for_edge(s, e1, &tol()).unwrap();
    let r2 = geo_ref_for_edge(s, e2, &tol()).unwrap();
    let (x1, x2) = (to_edge_ref(&r1).unwrap(), to_edge_ref(&r2).unwrap());
    let tags = |r: &EdgeRef| (r.a.tag(), r.b.tag());
    assert_eq!(tags(&x1), tags(&x2), "beide Stücke tragen dieselben Marken");
    // Der Hinweispunkt entscheidet.
    let g1 = resolve_geo(&solids, &GeoRef::Edge(r1.clone()), &tol()).unwrap();
    let g2 = resolve_geo(&solids, &GeoRef::Edge(r2.clone()), &tol()).unwrap();
    assert!(same_segment(&g1, p(0.0), p(8.0)), "{g1:?}");
    assert!(same_segment(&g2, p(12.0), p(20.0)), "{g2:?}");
    // Auch ein Hinweis nur in der Nähe genügt.
    let mut nearby = x2.clone();
    nearby.hint = p(13.0);
    let g = resolve_geo(&solids, &GeoRef::Edge(from_edge_ref(&nearby)), &tol()).unwrap();
    assert!(same_segment(&g, p(12.0), p(20.0)), "{g:?}");
    // Hinweis genau zwischen den Prüfpunkten beider Stücke: mehrdeutig.
    let mid = 0.5 * (edge_probe_point(s, e1).unwrap() + edge_probe_point(s, e2).unwrap());
    let mut tie = x1;
    tie.hint = mid;
    assert_eq!(
        resolve_geo(&solids, &GeoRef::Edge(from_edge_ref(&tie)), &tol()),
        Err(DimError::Ambiguous)
    );
    assert_eq!(
        resolve_geo(
            &solids,
            &GeoRef::EdgeEnd {
                edge: from_edge_ref(&tie),
                end: EndSel::Start
            },
            &tol()
        ),
        Err(DimError::Ambiguous)
    );
}

fn spline_body() -> Document {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let p: Vec<_> = [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, 7.0), (0.0, 5.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    sk.add_line(p[0], p[1]).unwrap();
    sk.add_line(p[1], p[2]).unwrap();
    sk.add_spline(&p[2..5]).unwrap();
    sk.add_line(p[4], p[0]).unwrap();
    let mut d = Document::new();
    let s = d
        .add_feature(
            "Skizze 1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
            }),
        )
        .unwrap();
    d.add_feature(
        "Extrusion 1",
        FeatureKind::Extrude {
            sketch: s,
            profile: 0,
            height: 2.0,
        },
    )
    .unwrap();
    d
}

fn lost_text(r: Result<Geo3, DimError>) -> String {
    match r {
        Err(DimError::Lost(t)) => t,
        other => panic!("Lost erwartet: {other:?}"),
    }
}

#[test]
fn unsupported_kinds_are_named() {
    // Spline-Kante: nicht bemaßbar, keine Mitte; ihre Enden sind Punkte.
    let d = spline_body();
    let solids = all_solids(&d);
    assert_eq!(solids.len(), 1);
    let s = solids[0];
    let (spline, _) = s
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, Curve::Nurbs(_)))
        .expect("Spline-Kante");
    let r = geo_ref_for_edge(s, spline, &tol()).unwrap();
    assert_eq!(
        lost_text(resolve_geo(&solids, &GeoRef::Edge(r.clone()), &tol())),
        "Dieser Kantentyp lässt sich nicht bemaßen."
    );
    assert_eq!(
        lost_text(resolve_geo(&solids, &GeoRef::Center(r.clone()), &tol())),
        "Nur Kreise und Bögen haben eine Mitte."
    );
    assert!(matches!(
        resolve_geo(
            &solids,
            &GeoRef::EdgeEnd {
                edge: r,
                end: EndSel::Start
            },
            &tol()
        ),
        Ok(Geo3::Point(_))
    ));

    // Ebene Fläche eines Quaders und Kegelmantel: keine bemaßbare Fläche.
    let mut d = Document::new();
    d.add_feature("Quader", bx(DVec3::splat(10.0), DVec3::ZERO))
        .unwrap();
    d.add_feature(
        "Kegel",
        FeatureKind::Cone {
            radius: 4.0,
            height: 6.0,
            origin: DVec3::new(40.0, 0.0, 0.0),
        },
    )
    .unwrap();
    let solids = all_solids(&d);
    let q = body(&d, "Quader");
    let plane = face_where(q, |x| matches!(x, Surface::Plane { .. }));
    let k = body(&d, "Kegel");
    let cone = face_where(k, |x| matches!(x, Surface::Cone { .. }));
    for r in [
        geo_ref_for_face(q, plane, &tol()).unwrap(),
        geo_ref_for_face(k, cone, &tol()).unwrap(),
    ] {
        assert_eq!(
            lost_text(resolve_geo(&solids, &GeoRef::Face(r), &tol())),
            "Diese Fläche lässt sich nicht bemaßen."
        );
    }
    // Daten, die nicht zur Art des Bezugs passen (nur über eine kaputte Datei möglich).
    let face_data = geo_ref_for_face(q, plane, &tol()).unwrap();
    assert!(matches!(
        resolve_geo(&solids, &GeoRef::Edge(face_data), &tol()),
        Err(DimError::Lost(_))
    ));
}
